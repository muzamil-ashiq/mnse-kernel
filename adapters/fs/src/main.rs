//! MNSE Filesystem & Git Sensor Adapter (`mnse-fs-adapter`).
//!
//! Universally watches the workspace directory via inotify (`notify`),
//! detects active code file saves & modifications across any editor (Zed, VSCode, Neovim, etc.),
//! extracts Git branch context, and emits `FileSwitched` events via shared memory (`SlotBus`) with socket fallback.

use clap::Parser;
use mnse_sdk::AdaptiveClient;
use mnse_types::{Event, EventType};
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

#[derive(Parser, Debug)]
#[command(name = "mnse-fs-adapter", author = "Muzamil", version = "2.0.0", about = "Filesystem & Git Watcher for MNSE Kernel")]
struct Cli {
    /// Directory path to watch (defaults to current working directory)
    #[arg(short, long)]
    path: Option<PathBuf>,

    /// Debounce duration in milliseconds
    #[arg(short, long, default_value = "300")]
    debounce_ms: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let watch_dir = cli.path.unwrap_or_else(|| {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    });
    let watch_dir = watch_dir.canonicalize().unwrap_or(watch_dir);

    tracing::info!("🔍 MNSE Filesystem & Git Adapter active on {:?}", watch_dir);

    let client = AdaptiveClient::default_local();
    let (tx, rx) = channel();

    let mut watcher = RecommendedWatcher::new(tx, Config::default())?;
    watcher.watch(&watch_dir, RecursiveMode::Recursive)?;

    let mut last_emitted_path = String::new();
    let mut last_emit_time = Instant::now();
    let debounce_dur = Duration::from_millis(cli.debounce_ms);

    loop {
        // Poll for inotify events with eager draining
        if let Ok(first_res) = rx.recv_timeout(Duration::from_millis(100)) {
            let mut events = vec![first_res];
            while let Ok(next_res) = rx.try_recv() {
                events.push(next_res);
            }

            for res in events {
                match res {
                    Ok(event) => {
                        if is_relevant_event(&event.kind) {
                            for path in event.paths {
                                if is_ignored_path(&path) {
                                    continue;
                                }

                                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                                    if is_code_extension(ext) {
                                        let path_str = path.to_string_lossy().to_string();
                                        let now = Instant::now();

                                        // Debounce identical path within debounce duration
                                        if path_str == last_emitted_path && now.duration_since(last_emit_time) < debounce_dur {
                                            continue;
                                        }

                                        last_emitted_path = path_str.clone();
                                        last_emit_time = now;

                                        let lang = detect_language(ext);
                                        let git_branch = get_git_branch(&path);

                                        let ev = Event::new(
                                            EventType::FileSwitched {
                                                file_path: path_str.clone(),
                                                line_count: None,
                                                language: Some(lang.to_string()),
                                                git_branch,
                                            },
                                            "fs",
                                        );

                                        match client.emit(&ev).await {
                                            Ok(_) => {
                                                tracing::info!("FileSwitched: {} [{}]", path_str, lang);
                                            }
                                            Err(e) => {
                                                tracing::debug!("Kernel emit error (offline): {}", e);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => tracing::warn!("Watch error: {:?}", e),
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Filter for meaningful modification/creation file events
fn is_relevant_event(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(_) | EventKind::Modify(_)
    )
}

/// Skip build directories and hidden VCS metadata
fn is_ignored_path(path: &Path) -> bool {
    let p_str = path.to_string_lossy();
    p_str.contains("/.git/")
        || p_str.contains("/target/")
        || p_str.contains("/node_modules/")
        || p_str.contains("/.cache/")
        || p_str.contains("/dist/")
        || p_str.contains("/tmp/")
        || p_str.ends_with('~')
        || p_str.ends_with(".tmp")
}

/// Supported source and configuration extensions
fn is_code_extension(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "rs" | "py" | "js" | "ts" | "jsx" | "tsx" | "toml" | "json" | "yaml" | "yml"
            | "c" | "cpp" | "h" | "hpp" | "go" | "sh" | "fish" | "zsh" | "md" | "sql"
            | "html" | "css" | "k"
    )
}

/// Detect language name from file extension
fn detect_language(ext: &str) -> &'static str {
    match ext.to_lowercase().as_str() {
        "rs" => "rust",
        "py" => "python",
        "js" | "jsx" => "javascript",
        "ts" | "tsx" => "typescript",
        "toml" => "toml",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "c" | "h" => "c",
        "cpp" | "hpp" => "cpp",
        "go" => "go",
        "sh" | "bash" | "zsh" | "fish" => "shell",
        "sql" => "sql",
        "md" => "markdown",
        "k" => "k",
        _ => "text",
    }
}

/// Extract current git branch by searching upward for .git/HEAD
fn get_git_branch(file_or_dir: &Path) -> Option<String> {
    let mut curr = if file_or_dir.is_file() {
        file_or_dir.parent()
    } else {
        Some(file_or_dir)
    };

    while let Some(dir) = curr {
        let dot_git = dir.join(".git");
        let head_path = if dot_git.is_dir() {
            dot_git.join("HEAD")
        } else if dot_git.is_file() {
            // Git worktree or submodule: .git is a file containing "gitdir: /path/to/commondir"
            if let Ok(content) = std::fs::read_to_string(&dot_git) {
                if let Some(gitdir) = content.trim().strip_prefix("gitdir:") {
                    let gitdir_path = PathBuf::from(gitdir.trim());
                    let resolved = if gitdir_path.is_relative() {
                        dir.join(gitdir_path)
                    } else {
                        gitdir_path
                    };
                    resolved.join("HEAD")
                } else {
                    dir.join(".git/HEAD")
                }
            } else {
                dir.join(".git/HEAD")
            }
        } else {
            dir.join(".git/HEAD")
        };

        if head_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&head_path) {
                let trimmed = content.trim();
                if let Some(branch) = trimmed.strip_prefix("ref: refs/heads/") {
                    return Some(branch.to_string());
                } else if !trimmed.is_empty() {
                    // Detached HEAD: commit hash (e.g., "05d239b")
                    let short_sha = if trimmed.len() >= 7 { &trimmed[..7] } else { trimmed };
                    return Some(short_sha.to_string());
                }
            }
        }
        curr = dir.parent();
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_code_extension() {
        assert!(is_code_extension("rs"));
        assert!(is_code_extension("py"));
        assert!(is_code_extension("toml"));
        assert!(is_code_extension("k"));
        assert!(!is_code_extension("png"));
        assert!(!is_code_extension("exe"));
    }

    #[test]
    fn test_detect_language() {
        assert_eq!(detect_language("rs"), "rust");
        assert_eq!(detect_language("py"), "python");
        assert_eq!(detect_language("ts"), "typescript");
        assert_eq!(detect_language("k"), "k");
        assert_eq!(detect_language("xyz"), "text");
    }

    #[test]
    fn test_is_ignored_path() {
        assert!(is_ignored_path(Path::new("/home/user/project/.git/HEAD")));
        assert!(is_ignored_path(Path::new("/home/user/project/target/debug/app")));
        assert!(is_ignored_path(Path::new("/home/user/project/node_modules/pkg")));
        assert!(!is_ignored_path(Path::new("/home/user/project/src/main.rs")));
    }

    #[test]
    fn test_get_git_branch_standard_and_detached_and_worktree() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();

        // Standard branch
        std::fs::write(repo.join(".git/HEAD"), "ref: refs/heads/feature/branch-x\n").unwrap();
        let src_file = repo.join("src/main.rs");
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(&src_file, "fn main() {}").unwrap();

        assert_eq!(get_git_branch(&src_file), Some("feature/branch-x".to_string()));

        // Detached HEAD
        std::fs::write(repo.join(".git/HEAD"), "05d239b62a6fe123456789\n").unwrap();
        assert_eq!(get_git_branch(&src_file), Some("05d239b".to_string()));

        // Git worktree pointer (.git as file)
        let worktree = temp.path().join("worktree");
        std::fs::create_dir_all(worktree.join("src")).unwrap();
        let wt_file = worktree.join("src/lib.rs");
        std::fs::write(&wt_file, "pub fn f() {}").unwrap();
        let common_git = temp.path().join("common_git");
        std::fs::create_dir_all(&common_git).unwrap();
        std::fs::write(common_git.join("HEAD"), "ref: refs/heads/worktree-branch\n").unwrap();
        std::fs::write(worktree.join(".git"), format!("gitdir: {}\n", common_git.display())).unwrap();

        assert_eq!(get_git_branch(&wt_file), Some("worktree-branch".to_string()));
    }
}
