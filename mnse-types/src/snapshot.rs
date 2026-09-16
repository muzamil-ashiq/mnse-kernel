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

    // ── Browser Context (from Browser Adapter) ───────────────────────────
    pub browser_url: Option<String>,         // [REDACTED] if privacy_mode
    pub browser_title: Option<String>,       // [REDACTED] if privacy_mode
    pub browser_domain: Option<String>,      // always shown
    pub browser_search_query: Option<String>,// [REDACTED] if privacy_mode

    // ── Vision Context (from Vision Adapter) ─────────────────────────────
    pub person_visible: Option<String>,      // "mab", "stranger", or None
    pub person_confidence: Option<f32>,

    // ── Audio Context (from Audio Adapter) ───────────────────────────────
    pub last_transcription: Option<String>,  // last Whisper result
    pub voice_active: bool,

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
            browser_url: None,
            browser_title: None,
            browser_domain: None,
            browser_search_query: None,
            person_visible: None,
            person_confidence: None,
            last_transcription: None,
            voice_active: false,
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
        if let Some(domain) = &self.browser_domain {
            parts.push(format!("web:{}", domain));
        }
        if let Some(person) = &self.person_visible {
            parts.push(format!("person:{}", person));
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
    pub fn to_toon_rich(&self) -> String {
        let mut out = String::new();
        out.push_str("📍 Live Context Snapshot [mnse-v2.0]\n");
        out.push_str("──────────────────────────────────────────────────\n");
        out.push_str(&format!("  Timestamp:  {}\n", self.timestamp.format("%H:%M:%S UTC")));

        if let Some(app) = &self.active_app {
            out.push_str(&format!("  Active App: {}\n", app));
        }
        if let Some(title) = &self.window_title {
            out.push_str(&format!("  Window:     {}\n", title));
        }
        if let Some(ws) = self.workspace {
            out.push_str(&format!("  Workspace:  {}\n", ws));
        }
        if let Some(file) = &self.editor_file {
            let home_str = dirs::home_dir().map(|h| h.to_string_lossy().to_string()).unwrap_or_default();
            let compressed = if !home_str.is_empty() { file.replace(&home_str, "~") } else { file.clone() };
            out.push_str(&format!("  Open File:  {}\n", compressed));
        }
        if let Some(lang) = &self.editor_language {
            out.push_str(&format!("  Language:   {}\n", lang));
        }
        if let Some(cwd) = &self.cwd {
            let home_str = dirs::home_dir().map(|h| h.to_string_lossy().to_string()).unwrap_or_default();
            let compressed = if !home_str.is_empty() { cwd.replace(&home_str, "~") } else { cwd.clone() };
            out.push_str(&format!("  CWD:        {}\n", compressed));
        }
        if let Some(branch) = &self.git_branch {
            out.push_str(&format!("  Git Branch: {}\n", branch));
        }

        if !self.adapters.is_empty() {
            out.push_str("\n🔌 Active Adapters:\n");
            for a in &self.adapters {
                let status_str = if a.connected { "online" } else { "offline" };
                out.push_str(&format!("   • {:<15} [{}] ({} events)\n", a.name, status_str, a.event_count));
            }
        }

        if !self.recent_commands.is_empty() {
            out.push_str("\n🖥️ Recent Commands:\n");
            for cmd in self.recent_commands.iter().take(5) {
                out.push_str(&format!("   • {} (exit: {})\n", cmd.command, cmd.exit_code));
            }
        }

        if !self.recent_errors.is_empty() {
            out.push_str("\n❌ Recent Errors:\n");
            for err in self.recent_errors.iter().take(5) {
                out.push_str(&format!("   • {} (exit: {})\n", err.command, err.exit_code));
            }
        }

        if !self.recent_files.is_empty() {
            let home_str = dirs::home_dir().map(|h| h.to_string_lossy().to_string()).unwrap_or_default();
            let compressed_files: Vec<String> = self.recent_files
                .iter()
                .take(5)
                .map(|f| if !home_str.is_empty() { f.replace(&home_str, "~") } else { f.clone() })
                .collect();
            out.push_str(&format!("\n📝 Recent Files:\n   • {}\n", compressed_files.join("\n   • ")));
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
    fn toon_omits_empty_fields() {
        let snap = ContextSnapshot::empty();
        let toon = snap.to_toon();
        // Only timestamp should be present
        assert!(toon.starts_with("ts:"));
        assert!(!toon.contains("app:"));
        assert!(!toon.contains("file:"));
    }
}
