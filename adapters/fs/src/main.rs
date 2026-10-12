//! MNSE Filesystem & Git Sensor Adapter (`mnse-fs-adapter`).
//!
//! Universally watches the workspace directory via inotify (`notify`),
//! detects active code file saves & modifications across any editor (Zed, VSCode, Neovim, etc.),
//! extracts Git branch context, and emits `FileSwitched` events to `~/.mnse/kernel.sock`.

use clap::Parser;
use mnse_sdk::SocketClient;
use mnse_types::{Event, EventType};
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::process::Command;
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

    let client = SocketClient::default_local();
    let (tx, rx) = channel();

    let mut watcher = RecommendedWatcher::new(tx, Config::default())?;
    watcher.watch(&watch_dir, RecursiveMode::Recursive)?;

    let mut last_emitted_path = String::new();
    let mut last_emit_time = Instant::now();
    let debounce_dur = Duration::from_millis(cli.debounce_ms);

    loop {
        // Poll for inotify events
        if let Ok(res) = rx.recv_timeout(Duration::from_millis(100)) {
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
                                    let git_branch = get_git_branch(&watch_dir);

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
                                            tracing::debug!("Kernel socket emit error (offline): {}", e);
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
            | "html" | "css" | "k" | "rhai"
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
        "rhai" => "rhai",
        _ => "text",
    }
}

/// Extract current git branch if workspace is inside a git repository
fn get_git_branch(repo_dir: &Path) -> Option<String> {
    // Try reading .git/HEAD first for instant zero-fork read
    let head_path = repo_dir.join(".git/HEAD");
    if head_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&head_path) {
            let trimmed = content.trim();
            if let Some(branch) = trimmed.strip_prefix("ref: refs/heads/") {
                return Some(branch.to_string());
            }
        }
    }

    // Fallback to git CLI
    let output = Command::new("git")
        .current_dir(repo_dir)
        .args(["branch", "--show-current"])
        .output()
        .ok()?;

    if output.status.success() {
        let b = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !b.is_empty() {
            return Some(b);
        }
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
}
