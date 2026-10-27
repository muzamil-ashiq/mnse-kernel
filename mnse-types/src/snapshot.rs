//! ContextSnapshot — the live view of what the user is doing right now.
//!
//! This is what `GET /snapshot` returns and what K's `current.state` queries.
//! Always computed from live InspectorState — never stored in the ledger directly.
//! TOON and JSON are different rendered views of this struct.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Adapter health status shown in `k care` output
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdapterStatus {
    pub name: String,
    pub connected: bool,
    pub last_seen: Option<DateTime<Utc>>,
    pub event_count: u64,
    pub version: Option<String>,
}

/// Tracks active session + burst for the knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub burst_id: Option<String>,
    pub started_at: DateTime<Utc>,
    pub event_count: u64,
    pub files_touched: u32,
    pub commands_run: u32,
    pub errors_count: u32,
}

/// The complete live context snapshot.
///
/// Field naming: short, unambiguous, always present (Option only when genuinely unknown).
/// Redaction: fields marked `[REDACTED]` in comments are filtered when MNSE_PRIVACY_MODE=true.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSnapshot {
    // ── Metadata ─────────────────────────────────────────────────────────
    pub timestamp: DateTime<Utc>,
    pub schema_version: String,         // "mnse-v2.0"
    pub privacy_mode: bool,            // true = redaction active

    // ── OS Context (from OS Adapter) ─────────────────────────────────────
    pub active_app: Option<String>,
    pub window_title: Option<String>,  // [REDACTED] if privacy_mode
    pub window_class: Option<String>,
    pub window_id: Option<String>,
    pub workspace: Option<u32>,
    pub workspace_count: Option<u32>,   // total workspaces known from hyprland
    pub is_idle: bool,
    pub idle_seconds: Option<u64>,

    // ── Clipboard ────────────────────────────────────────────────────────
    pub clipboard_hash: Option<String>,
    pub clipboard_preview: Option<String>, // [REDACTED] if privacy_mode

    // ── Shell Context (from Shell Adapter) ────────────────────────────────
    pub cwd: Option<String>,
    pub last_command: Option<String>,
    pub last_exit_code: Option<i32>,
    pub last_command_duration_ms: Option<u64>,
    pub terminal_has_error: bool,
    pub error_pattern: Option<String>,     // e.g. "RustCompile", "GitConflict"
    pub last_error_command: Option<String>,

    // ── Editor Context (from OS window title / editor adapters) ──────────
    pub editor_file: Option<String>,
    pub editor_language: Option<String>,
    pub editor_line: Option<u32>,
    pub editor_diagnostics_errors: Option<u32>,
    pub editor_diagnostics_warnings: Option<u32>,
    pub git_branch: Option<String>,
    pub git_repo: Option<String>,

    // ── System (from System Adapter) ─────────────────────────────────────
    pub cpu_percent: Option<f32>,
    pub memory_used_mb: Option<u64>,
    pub memory_total_mb: Option<u64>,
    pub battery_percent: Option<f32>,

    // ── Session / Graph context ───────────────────────────────────────────
    pub session: Option<SessionInfo>,
    pub daily_stats: DailyStats,

    // ── Adapter Health ────────────────────────────────────────────────────
    pub adapters: Vec<AdapterStatus>,

    // ── Recent History (short list for TOON context) ──────────────────────
    pub recent_commands: Vec<RecentCommand>,
    pub recent_files: Vec<String>,
    pub recent_errors: Vec<RecentError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DailyStats {
    pub date: String,          // "2026-08-12"
    pub events_total: u64,
    pub commands_run: u32,
    pub files_touched: u32,
    pub errors_count: u32,
    pub focused_apps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentCommand {
    pub command: String,
    pub exit_code: i32,
    pub cwd: String,
    pub timestamp: DateTime<Utc>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentError {
    pub command: String,
    pub exit_code: i32,
    pub pattern: Option<String>,
    pub timestamp: DateTime<Utc>,
}

impl ContextSnapshot {
    /// Create a blank snapshot with defaults
    pub fn empty() -> Self {
        Self {
            timestamp: Utc::now(),
            schema_version: "mnse-v2.0".to_string(),
            privacy_mode: true,
            active_app: None,
            window_title: None,
            window_class: None,
            window_id: None,
            workspace: None,
            workspace_count: None,
            is_idle: false,
            idle_seconds: None,
            clipboard_hash: None,
            clipboard_preview: None,
            cwd: None,
            last_command: None,
            last_exit_code: None,
            last_command_duration_ms: None,
            terminal_has_error: false,
            error_pattern: None,
            last_error_command: None,
            editor_file: None,
            editor_language: None,
            editor_line: None,
            editor_diagnostics_errors: None,
            editor_diagnostics_warnings: None,
            git_branch: None,
            git_repo: None,
            cpu_percent: None,
            memory_used_mb: None,
            memory_total_mb: None,
            battery_percent: None,
            session: None,
            daily_stats: DailyStats::default(),
            adapters: Vec::new(),
            recent_commands: Vec::new(),
            recent_files: Vec::new(),
            recent_errors: Vec::new(),
        }
    }

    /// Render snapshot into compact TOON (Token-Optimized Object Notation) format for LLM agents.
    pub fn to_toon(&self) -> String {
        let mut parts = Vec::new();
        parts.push(format!("ts:{}", self.timestamp.format("%H:%M")));

        if let Some(app) = &self.active_app {
            parts.push(format!("app:{}", app));
        }
        if let Some(file) = &self.editor_file {
            let basename = std::path::Path::new(file)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(file.as_str());
            parts.push(format!("file:{}", basename));
        }
        if let Some(line) = self.editor_line {
            parts.push(format!("ln:{}", line));
        }
        if let Some(branch) = &self.git_branch {
            parts.push(format!("git:{}", branch));
        }
        if let Some(cmd) = &self.last_command {
            let short = cmd.split_whitespace().next().unwrap_or(cmd.as_str());
            parts.push(format!("scmd:{}", short));
        }
        if let Some(ec) = self.last_exit_code {
            if ec != 0 {
                parts.push(format!("ec:{}", ec));
            }
        }
        if let Some(pat) = &self.error_pattern {
            parts.push(format!("err:{}", pat));
        }
        if let Some(cwd) = &self.cwd {
            let basename = std::path::Path::new(cwd)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(cwd.as_str());
            parts.push(format!("cwd:{}", basename));
        }

        let failed: Vec<&str> = self.adapters.iter()
            .filter(|a| !a.connected)
            .map(|a| a.name.as_str())
            .collect();
        if !failed.is_empty() {
            parts.push(format!("dead:{}", failed.join(",")));
        }

        parts.join(" ")
    }

    /// Render snapshot into Rich TOON format for CLI & Human display
    /// Render snapshot into Lean Diagnostic TOON format (Option 3 with Traceable IDs)
    pub fn to_toon_rich(&self) -> String {
        let home_str = dirs::home_dir()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_default();

        let compress = |s: &str| -> String {
            if !home_str.is_empty() { s.replace(&home_str, "~") } else { s.to_string() }
        };

        // Helper to produce a deterministic 6-character hex hash from a string (ADR 005)
        let short_id = |prefix: &str, s: &str| -> String {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(s.as_bytes());
            let res = hasher.finalize();
            format!("{}:{:02x}{:02x}{:02x}", prefix, res[0], res[1], res[2])
        };

        let mut out = String::new();

        // ── Header ─────────────────────────────────────────────────────────
        let idle_tag = if self.is_idle {
            format!(" [idle: {}s]", self.idle_seconds.unwrap_or(0))
        } else {
            String::new()
        };

        out.push_str(&format!(
            "[mnse v2.0] {}{}\n",
            self.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
            idle_tag
        ));

        // ── Focus ──────────────────────────────────────────────────────────
        let has_focus = self.active_app.is_some()
            || self.window_title.is_some()
            || self.workspace.is_some()
            || self.cwd.is_some()
            || self.git_branch.is_some()
            || self.editor_file.is_some();

        if has_focus {
            out.push_str("\nfocus:\n");
            if let Some(app) = &self.active_app {
                let win_id = self.window_id.as_deref().unwrap_or("0x0");
                let ws_str = match (self.workspace, self.workspace_count) {
                    (Some(ws), Some(tot)) => format!(", ws: {}/{}", ws, tot),
                    (Some(ws), None) => format!(", ws: {}", ws),
                    _ => String::new(),
                };
                out.push_str(&format!("  app: {} [win:{}{}]\n", app, win_id, ws_str));
            }
            if let Some(title) = &self.window_title {
                let short = if title.chars().count() > 64 {
                    format!("{}...", &title[..title.char_indices().nth(64).map(|(i,_)| i).unwrap_or(title.len())])
                } else {
                    title.clone()
                };
                out.push_str(&format!("  win: {}\n", short));
            }
            if let Some(cwd) = &self.cwd {
                let git_tag = self.git_branch.as_deref()
                    .map(|b| {
                        let repo_str = self.git_repo.as_deref().map(|r| format!(" ({})", r)).unwrap_or_default();
                        format!(" (git: {}{})", b, repo_str)
                    })
                    .unwrap_or_default();
                out.push_str(&format!("  dir: {}{}\n", compress(cwd), git_tag));
            }
            if let Some(file) = &self.editor_file {
                let lang_tag = self.editor_language.as_deref()
                    .map(|l| format!(" [{}]", l))
                    .unwrap_or_default();
                let line_tag = self.editor_line
                    .map(|l| format!(":L{}", l))
                    .unwrap_or_default();
                out.push_str(&format!("  file: {}{}{}\n", compress(file), line_tag, lang_tag));
            }
            if let Some(err_count) = self.editor_diagnostics_errors {
                if err_count > 0 {
                    let warn_count = self.editor_diagnostics_warnings.unwrap_or(0);
                    out.push_str(&format!("  diag: {} errors, {} warnings\n", err_count, warn_count));
                }
            }
        }

        // ── Commands ───────────────────────────────────────────────────────
        if !self.recent_commands.is_empty() {
            out.push_str("\ncommands:\n");
            for cmd in self.recent_commands.iter().take(5) {
                let mark = if cmd.exit_code == 0 { "." } else { "x" };
                let dur = if cmd.duration_ms > 0 {
                    format!(", dur: {:.1}s", cmd.duration_ms as f64 / 1000.0)
                } else {
                    String::new()
                };
                let cid = short_id("cmd", &cmd.command);
                let short_cmd = if cmd.command.len() > 50 {
                    format!("{}...", &cmd.command[..50])
                } else {
                    cmd.command.clone()
                };
                out.push_str(&format!(
                    "  {} [{}] {} (exit: {}{})\n",
                    mark, cid, short_cmd, cmd.exit_code, dur
                ));
            }
        }

        // ── Errors ─────────────────────────────────────────────────────────
        if !self.recent_errors.is_empty() {
            out.push_str("\nerrors:\n");
            for err in self.recent_errors.iter().take(3) {
                let eid = short_id("err", &err.command);
                let pattern_tag = err.pattern.as_deref()
                    .map(|p| format!(" [{}, exit: {}]", p, err.exit_code))
                    .unwrap_or_else(|| format!(" [exit: {}]", err.exit_code));
                let short_cmd = if err.command.len() > 48 {
                    format!("{}...", &err.command[..48])
                } else {
                    err.command.clone()
                };
                out.push_str(&format!("  [{}] {}{}\n", eid, short_cmd, pattern_tag));
            }
        }

        // ── Recent Files ───────────────────────────────────────────────────
        if !self.recent_files.is_empty() {
            out.push_str("\nrecent files:\n");
            for f in self.recent_files.iter().take(3) {
                out.push_str(&format!("  {}\n", compress(f)));
            }
        }

        // ── Telemetry & Adapters ───────────────────────────────────────────
        let has_telemetry = !self.adapters.is_empty()
            || self.cpu_percent.is_some()
            || self.memory_used_mb.is_some()
            || self.daily_stats.events_total > 0;

        if has_telemetry {
            out.push_str("\ntelemetry:\n");

            if !self.adapters.is_empty() {
                let adapter_strs: Vec<String> = self.adapters.iter().map(|a| {
                    let ts = a.last_seen
                        .map(|t| t.format("%H:%M").to_string())
                        .unwrap_or_else(|| "--:--".into());
                    let state = if a.connected { "" } else { " (off)" };
                    format!("{}:{}{}({})", a.name, a.event_count, state, ts)
                }).collect();
                out.push_str(&format!("  adapters: {}\n", adapter_strs.join(", ")));
            }

            let mut sys_parts = Vec::new();
            if let Some(cpu) = self.cpu_percent {
                sys_parts.push(format!("cpu {:.1}%", cpu));
            }
            if let (Some(used), Some(total)) = (self.memory_used_mb, self.memory_total_mb) {
                let pct = (used as f64 / total as f64) * 100.0;
                let used_gb = used as f64 / 1024.0;
                let total_gb = total as f64 / 1024.0;
                sys_parts.push(format!("ram {:.1}/{:.1}GB ({:.0}%)", used_gb, total_gb, pct));
            }
            if let Some(bat) = self.battery_percent {
                sys_parts.push(format!("bat {:.0}%", bat));
            }
            if !sys_parts.is_empty() {
                out.push_str(&format!("  system:   {}\n", sys_parts.join(", ")));
            }

            let s = &self.daily_stats;
            if s.events_total > 0 {
                out.push_str(&format!(
                    "  today:    {} ev / {} cmd / {} err / {} files\n",
                    s.events_total, s.commands_run, s.errors_count, s.files_touched
                ));
            }
        }

        out
    }


}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_snapshot_has_schema_version() {
        let snap = ContextSnapshot::empty();
        assert_eq!(snap.schema_version, "mnse-v2.0");
        assert!(snap.privacy_mode); // safe by default
    }

    #[test]
    fn toon_contains_expected_parts() {
        let mut snap = ContextSnapshot::empty();
        snap.active_app = Some("zed".into());
        snap.editor_file = Some("/home/mab/project/main.rs".into());
        snap.editor_line = Some(42);
        snap.git_branch = Some("feat/v2".into());
        snap.last_command = Some("cargo build --release".into());
        snap.last_exit_code = Some(1);
        snap.error_pattern = Some("RustCompile".into());

        let toon = snap.to_toon();
        assert!(toon.contains("app:zed"), "missing app: {}", toon);
        assert!(toon.contains("file:main.rs"), "missing file: {}", toon);
        assert!(toon.contains("ln:42"), "missing ln: {}", toon);
        assert!(toon.contains("git:feat/v2"), "missing git: {}", toon);
        assert!(toon.contains("scmd:cargo"), "missing scmd: {}", toon);
        assert!(toon.contains("ec:1"), "missing ec: {}", toon);
        assert!(toon.contains("err:RustCompile"), "missing err: {}", toon);
    }

    #[test]
    fn toon_omits_zero_exit_code() {
        let mut snap = ContextSnapshot::empty();
        snap.last_exit_code = Some(0);
        assert!(!snap.to_toon().contains("ec:0"));
    }

    #[test]
    fn toon_rich_option3_format_and_ids() {
        let mut snap = ContextSnapshot::empty();
        snap.active_app = Some("kitty".into());
        snap.window_id = Some("0x1a2b".into());
        snap.workspace = Some(1);
        snap.workspace_count = Some(4);
        snap.cwd = Some("/home/mab/Applications/my-project".into());
        snap.recent_commands.push(RecentCommand {
            command: "cargo test --workspace".into(),
            exit_code: 0,
            cwd: "/home/mab/Applications/my-project".into(),
            timestamp: Utc::now(),
            duration_ms: 12400,
        });

        let rich = snap.to_toon_rich();
        assert!(rich.contains("app: kitty [win:0x1a2b, ws: 1/4]"), "expected format, got: {}", rich);
        assert!(rich.contains(". [cmd:"), "expected command with id, got: {}", rich);
        assert!(!rich.contains("─"), "must not contain line characters: {}", rich);
        assert!(!rich.contains("📍"), "must not contain emoji: {}", rich);
    }
}
