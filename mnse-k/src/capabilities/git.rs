//! Git Capability Provider — Repository status, diffs, and commit history.

use super::evidence::{EngineInfo, StructuredEvidence};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::process::Command;
use std::time::Instant;
use uuid::Uuid;

pub struct GitCapability;

impl GitCapability {
    /// Return structured git status (branch, modified, staged, untracked)
    pub fn status(repo_dir: Option<&str>) -> Result<StructuredEvidence> {
        let start_time = Instant::now();

        let mut cmd = Command::new("git");
        if let Some(dir) = repo_dir {
            cmd.current_dir(dir);
        }
        let output = cmd
            .arg("status")
            .arg("--porcelain=v1")
            .arg("-b")
            .output()
            .map_err(|e| anyhow!("Failed to execute git status: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("git status failed: {}", stderr.trim()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut branch = "unknown".to_string();
        let mut staged = Vec::new();
        let mut modified = Vec::new();
        let mut untracked = Vec::new();

        for line in stdout.lines() {
            if line.starts_with("##") {
                let rest = &line[2..].trim();
                branch = rest.split("...").next().unwrap_or("unknown").trim().to_string();
            } else if line.len() >= 3 {
                let code = &line[0..2];
                let file = line[3..].trim().to_string();
                match code {
                    "??" => untracked.push(file),
                    " M" | " D" => modified.push(file),
                    "M " | "A " | "D " => staged.push(file),
                    "MM" | "AM" => {
                        staged.push(file.clone());
                        modified.push(file);
                    }
                    _ => modified.push(file),
                }
            }
        }

        let duration_ms = start_time.elapsed().as_millis() as u64;

        Ok(StructuredEvidence {
            operation_id: Uuid::new_v4(),
            status: "success".to_string(),
            engine: EngineInfo {
                name: "git".to_string(),
                version: Some("2.48".to_string()),
            },
            input: None,
            output: None,
            data: json!({
                "branch": branch,
                "staged": staged,
                "modified": modified,
                "untracked": untracked,
                "clean": staged.is_empty() && modified.is_empty() && untracked.is_empty()
            }),
            warnings: Vec::new(),
            duration_ms,
        })
    }

    /// Return short git diff summary
    pub fn diff(repo_dir: Option<&str>) -> Result<StructuredEvidence> {
        let start_time = Instant::now();

        let mut cmd = Command::new("git");
        if let Some(dir) = repo_dir {
            cmd.current_dir(dir);
        }
        let output = cmd
            .arg("diff")
            .arg("--stat")
            .output()
            .map_err(|e| anyhow!("Failed to execute git diff: {}", e))?;

        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let duration_ms = start_time.elapsed().as_millis() as u64;

        Ok(StructuredEvidence {
            operation_id: Uuid::new_v4(),
            status: "success".to_string(),
            engine: EngineInfo {
                name: "git".to_string(),
                version: Some("2.48".to_string()),
            },
            input: None,
            output: None,
            data: json!({ "diff_stat": text }),
            warnings: Vec::new(),
            duration_ms,
        })
    }

    /// Return recent commit log entries
    pub fn log(n: usize, repo_dir: Option<&str>) -> Result<StructuredEvidence> {
        let start_time = Instant::now();

        let mut cmd = Command::new("git");
        if let Some(dir) = repo_dir {
            cmd.current_dir(dir);
        }
        let output = cmd
            .arg("log")
            .arg(format!("-n{}", n))
            .arg("--oneline")
            .output()
            .map_err(|e| anyhow!("Failed to execute git log: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let commits: Vec<Value> = stdout
            .lines()
            .filter_map(|l| {
                l.split_once(' ').map(|(hash, msg)| {
                    json!({
                        "hash": hash,
                        "message": msg
                    })
                })
            })
            .collect();

        let duration_ms = start_time.elapsed().as_millis() as u64;

        Ok(StructuredEvidence {
            operation_id: Uuid::new_v4(),
            status: "success".to_string(),
            engine: EngineInfo {
                name: "git".to_string(),
                version: Some("2.48".to_string()),
            },
            input: None,
            output: None,
            data: Value::Array(commits),
            warnings: Vec::new(),
            duration_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use std::path::Path;
    use tempfile::tempdir;

    #[test]
    fn test_git_capability_lifecycle() {
        let dir = tempdir().unwrap();
        let repo_path = dir.path().to_str().unwrap();

        // Initialize git repo
        let init_status = Command::new("git")
            .arg("init")
            .current_dir(repo_path)
            .status()
            .expect("git init");
        assert!(init_status.success());

        // Configure dummy user for committing
        Command::new("git")
            .args(["config", "user.name", "MNSE Test"])
            .current_dir(repo_path)
            .status()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@mnse.local"])
            .current_dir(repo_path)
            .status()
            .unwrap();

        // 1. Untracked file
        let file_path = dir.path().join("test.txt");
        let mut f = File::create(&file_path).unwrap();
        f.write_all(b"initial line\n").unwrap();
        drop(f);

        let status = GitCapability::status(Some(repo_path)).expect("git status untracked");
        assert_eq!(status.status, "success");
        let untracked = status.data["untracked"].as_array().unwrap();
        assert!(untracked.iter().any(|v| v.as_str() == Some("test.txt")));

        // 2. Stage file
        Command::new("git")
            .args(["add", "test.txt"])
            .current_dir(repo_path)
            .status()
            .unwrap();

        let status = GitCapability::status(Some(repo_path)).expect("git status staged");
        let staged = status.data["staged"].as_array().unwrap();
        assert!(staged.iter().any(|v| v.as_str() == Some("test.txt")));

        // 3. Commit
        Command::new("git")
            .args(["commit", "-m", "feat: initial commit"])
            .current_dir(repo_path)
            .status()
            .unwrap();

        let log = GitCapability::log(1, Some(repo_path)).expect("git log");
        let commits = log.data.as_array().unwrap();
        assert_eq!(commits.len(), 1);
        assert!(commits[0]["message"].as_str().unwrap().contains("initial commit"));

        // 4. Modify file & check diff
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&file_path)
            .unwrap();
        f.write_all(b"second line\n").unwrap();
        drop(f);

        let diff = GitCapability::diff(Some(repo_path)).expect("git diff");
        assert!(diff.data["diff_stat"].as_str().unwrap().contains("test.txt"));
    }

    #[test]
    fn test_git_injection_defense() {
        let injection_path = "/tmp/fake_dir; touch /tmp/mnse_injected_evil_file";
        let res = GitCapability::status(Some(injection_path));
        assert!(res.is_err());
        assert!(!Path::new("/tmp/mnse_injected_evil_file").exists());
    }
}
