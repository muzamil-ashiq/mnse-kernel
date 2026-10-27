//! Event types — the core data model for everything flowing through MNSE.
//!
//! Design rules (from ADR 001 and ADR 004):
//! - Every event has `observed_by` — no anonymous events ever
//! - `parent_event_id` enables causal linking (edit → error → fix)
//! - `kernel_received_at` is a forensic timestamp set by the kernel on intake
//! - `EventType` variants are fully typed — no free-form strings for known events

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Event struct ──────────────────────────────────────────────────────────────

/// The atomic unit of truth in MNSE. Every adapter emits Events.
/// Every fact the kernel knows came from an Event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Unique event ID (UUID v4)
    pub id: Uuid,

    /// The type of event (fully typed, no stringly-typed dispatch)
    pub event_type: EventType,

    /// Which adapter observed this event (ADR 001 — mandatory)
    /// Format: "os", "shell", "browser", "vision", "audio", "system", "agent:<name>"
    /// Events with observed_by = "system" do NOT update adapter health (Epistemic Paradox fix)
    pub observed_by: String,

    /// Source category for fast filtering
    pub source: EventSource,

    /// Causal parent event ID — links this event to what caused it
    /// Example: ErrorDetected.parent = ProcessExecuted that failed
    pub parent_event_id: Option<Uuid>,

    /// When the event actually occurred (adapter time)
    pub timestamp: DateTime<Utc>,

    /// When the kernel received this event (forensic, set by kernel only)
    /// Used to detect slow/lagging adapters and ordering issues
    pub kernel_received_at: Option<DateTime<Utc>>,

    /// Optional free-form JSON payload for event-type-specific extra data
    /// Prefer typed fields in EventType variants over this
    pub payload: Option<serde_json::Value>,
}

impl Event {
    /// Create a new event. `kernel_received_at` is set by the kernel, not here.
    pub fn new(event_type: EventType, observed_by: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            source: EventSource::from_event_type(&event_type),
            event_type,
            observed_by: observed_by.into(),
            parent_event_id: None,
            timestamp: Utc::now(),
            kernel_received_at: None,
            payload: None,
        }
    }

    pub fn with_parent(mut self, parent: Uuid) -> Self {
        self.parent_event_id = Some(parent);
        self
    }

    pub fn with_payload(mut self, payload: serde_json::Value) -> Self {
        self.payload = Some(payload);
        self
    }

    /// Returns the human-readable name used for DB storage and Rhai classification
    pub fn type_name(&self) -> &'static str {
        self.event_type.type_name()
    }
}

// ── EventSource ───────────────────────────────────────────────────────────────

/// Coarse category used for quick filtering (ledger queries, MCP)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventSource {
    Os,
    Shell,
    Editor,
    System,
    Kernel, // internal events emitted by the kernel itself
}

impl EventSource {
    pub fn from_event_type(event_type: &EventType) -> Self {
        match event_type {
            EventType::WindowFocused { .. }
            | EventType::WorkspaceChanged { .. }
            | EventType::ClipboardChanged { .. }
            | EventType::IdleStateChanged { .. } => Self::Os,

            EventType::ProcessExecuted { .. }
            | EventType::DirectoryChanged { .. }
            | EventType::ErrorDetected { .. } => Self::Shell,

            EventType::FileSwitched { .. }
            | EventType::CursorMoved { .. }
            | EventType::SelectionChanged { .. }
            | EventType::DiagnosticsUpdated { .. } => Self::Editor,

            EventType::ResourcesUpdated { .. } | EventType::SystemHeartbeat => Self::System,

            _ => Self::System,
        }
    }
}

// ── EventType ─────────────────────────────────────────────────────────────────

/// All events that can flow through the MNSE EventBus.
///
/// Design principle: prefer typed variants over Custom { name, payload }.
/// Custom is an escape hatch only. If you find yourself using Custom for
/// a recurring pattern, add a typed variant.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "PascalCase")]
pub enum EventType {
    // ── OS Adapter ────────────────────────────────────────────────────────

    /// A different application window received focus
    WindowFocused {
        app_name: String,
        window_title: String,   // may be redacted (ADR 002)
        window_class: String,
        window_id: String,
        pid: Option<u32>,
        workspace: Option<u32>,
    },

    /// User switched to a different workspace
    WorkspaceChanged {
        workspace_id: u32,
        workspace_name: Option<String>,
        previous_workspace: Option<u32>,
        #[serde(default)]
        workspace_count: Option<u32>,
    },

    /// Clipboard content changed
    ClipboardChanged {
        content_hash: String,   // SHA256-8 of content (never store raw content)
        content_preview: Option<String>, // redacted unless MNSE_PRIVACY_MODE=false
        content_len: usize,
    },

    /// User went idle or became active
    IdleStateChanged {
        is_idle: bool,
        idle_seconds: Option<u64>,
    },

    // Noise — SSE only, NEVER stored in SQLite
    MonitorChanged { monitor_name: String },
    RunningAppsChanged,
    NetworkUpdated,

    // ── Shell Adapter ─────────────────────────────────────────────────────

    /// A shell command was executed and completed
    ProcessExecuted {
        command: String,
        exit_code: i32,
        duration_ms: u64,
        working_dir: String,
        stdout_tail: Option<String>, // last 500 chars of stdout (capped)
        stderr_tail: Option<String>, // last 500 chars of stderr (capped)
    },

    /// Shell working directory changed (cd)
    DirectoryChanged {
        new_dir: String,
        previous_dir: Option<String>,
    },

    /// A non-zero exit code was detected — high priority, always stored
    /// Emitted by the kernel as a derived event from ProcessExecuted
    ErrorDetected {
        command: String,
        exit_code: i32,
        error_pattern: Option<String>, // e.g. "RustCompile", "GitConflict"
        working_dir: String,
    },

    // ── Editor Adapters ───────────────────────────────────────────────────

    /// Active file changed in editor (VSCode or Neovim passive via OS window)
    FileSwitched {
        file_path: String,
        language: Option<String>,
        git_branch: Option<String>,
        line_count: Option<u32>,
    },

    // Noise — SSE only, NEVER stored in SQLite
    CursorMoved {
        line: u32,
        col: u32,
    },

    // Noise — SSE only, NEVER stored in SQLite
    SelectionChanged {
        text_hash: Option<String>,
        char_count: u32,
    },

    /// Compiler diagnostics changed
    DiagnosticsUpdated {
        file_path: String,
        error_count: u32,
        warning_count: u32,
    },

    // ── System Adapter ────────────────────────────────────────────────────

    // Noise — SSE only, NEVER stored in SQLite
    ResourcesUpdated {
        cpu_percent: f32,
        memory_used_mb: u64,
        memory_total_mb: u64,
        battery_percent: Option<f32>,
    },

    /// System adapter heartbeat (SSE only, never stored)
    SystemHeartbeat,

    // ── Adapter Lifecycle ─────────────────────────────────────────────────

    /// An adapter successfully completed the handshake
    AdapterConnected {
        adapter_name: String,
        protocol_version: String,
        session_id: Uuid,
    },

    /// An adapter disconnected (clean or unexpected)
    AdapterDisconnected {
        adapter_name: String,
        reason: DisconnectReason,
    },

    // SSE only — used for adapter health tracking, never stored
    AdapterHeartbeat {
        adapter_name: String,
    },

    // ── Escape Hatch ──────────────────────────────────────────────────────

    /// For custom/experimental events. If you use this more than twice for
    /// the same pattern, add a typed variant above.
    Custom {
        name: String,
        payload: serde_json::Value,
    },
}

impl EventType {
    /// The canonical string name used in SQLite and FTS5
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::WindowFocused { .. }      => "WindowFocused",
            Self::WorkspaceChanged { .. }   => "WorkspaceChanged",
            Self::ClipboardChanged { .. }   => "ClipboardChanged",
            Self::IdleStateChanged { .. }   => "IdleStateChanged",
            Self::MonitorChanged { .. }     => "MonitorChanged",
            Self::RunningAppsChanged        => "RunningAppsChanged",
            Self::NetworkUpdated            => "NetworkUpdated",
            Self::ProcessExecuted { .. }    => "ProcessExecuted",
            Self::DirectoryChanged { .. }   => "DirectoryChanged",
            Self::ErrorDetected { .. }      => "ErrorDetected",
            Self::FileSwitched { .. }       => "FileSwitched",
            Self::CursorMoved { .. }        => "CursorMoved",
            Self::SelectionChanged { .. }   => "SelectionChanged",
            Self::DiagnosticsUpdated { .. } => "DiagnosticsUpdated",
            Self::ResourcesUpdated { .. }   => "ResourcesUpdated",
            Self::SystemHeartbeat           => "SystemHeartbeat",
            Self::AdapterConnected { .. }   => "AdapterConnected",
            Self::AdapterDisconnected { .. }=> "AdapterDisconnected",
            Self::AdapterHeartbeat { .. }   => "AdapterHeartbeat",
            Self::Custom { .. }             => "Custom",
        }
    }

    /// Returns true for events that must NEVER be written to SQLite.
    /// These are high-frequency noise events that only flow through the SSE stream.
    pub fn is_noise(&self) -> bool {
        matches!(
            self,
            Self::CursorMoved { .. }
            | Self::ResourcesUpdated { .. }
            | Self::AdapterHeartbeat { .. }
            | Self::MonitorChanged { .. }
            | Self::SelectionChanged { .. }
            | Self::SystemHeartbeat
            | Self::RunningAppsChanged
            | Self::NetworkUpdated
        )
    }

    /// Returns true for high-priority events that get fast-path routing
    pub fn is_high_priority(&self) -> bool {
        matches!(self, Self::ErrorDetected { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DisconnectReason {
    Clean,
    Timeout,
    ProtocolError,
    AuthFailed,
    KernelShutdown,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_events_are_correctly_classified() {
        assert!(EventType::CursorMoved { line: 1, col: 1 }.is_noise());
        assert!(EventType::ResourcesUpdated {
            cpu_percent: 10.0,
            memory_used_mb: 4096,
            memory_total_mb: 16384,
            battery_percent: None,
        }.is_noise());
        assert!(EventType::AdapterHeartbeat { adapter_name: "os".into() }.is_noise());
        assert!(EventType::MonitorChanged { monitor_name: "DP-1".into() }.is_noise());
        assert!(EventType::SelectionChanged { text_hash: None, char_count: 0 }.is_noise());
        assert!(EventType::SystemHeartbeat.is_noise());
        assert!(EventType::RunningAppsChanged.is_noise());
        assert!(EventType::NetworkUpdated.is_noise());
    }

    #[test]
    fn milestone_events_are_not_noise() {
        assert!(!EventType::ProcessExecuted {
            command: "cargo build".into(),
            exit_code: 0,
            duration_ms: 1200,
            working_dir: "/tmp".into(),
            stdout_tail: None,
            stderr_tail: None,
        }.is_noise());
        assert!(!EventType::ErrorDetected {
            command: "cargo build".into(),
            exit_code: 1,
            error_pattern: Some("RustCompile".into()),
            working_dir: "/tmp".into(),
        }.is_noise());
    }

    #[test]
    fn event_type_names_are_stable() {
        assert_eq!(EventType::WindowFocused {
            app_name: "".into(), window_title: "".into(),
            window_class: "".into(), window_id: "".into(),
            pid: None, workspace: None,
        }.type_name(), "WindowFocused");
        assert_eq!(EventType::ErrorDetected {
            command: "".into(), exit_code: 1,
            error_pattern: None, working_dir: "".into(),
        }.type_name(), "ErrorDetected");
    }

    #[test]
    fn event_builder_sets_observed_by() {
        let e = Event::new(EventType::SystemHeartbeat, "system");
        assert_eq!(e.observed_by, "system");
        assert!(e.kernel_received_at.is_none()); // set by kernel on intake
    }

    #[test]
    fn high_priority_events_flagged() {
        assert!(EventType::ErrorDetected {
            command: "ls".into(), exit_code: 1,
            error_pattern: None, working_dir: "/".into(),
        }.is_high_priority());
        assert!(!EventType::WindowFocused {
            app_name: "code".into(), window_title: "test".into(),
            window_class: "code".into(), window_id: "1".into(),
            pid: None, workspace: None,
        }.is_high_priority());
    }

    #[test]
    fn event_source_from_event_type() {
        assert_eq!(
            EventSource::from_event_type(&EventType::ProcessExecuted {
                command: "ls".into(), exit_code: 0, duration_ms: 10,
                working_dir: "/".into(), stdout_tail: None, stderr_tail: None,
            }),
            EventSource::Shell
        );
        assert_eq!(
            EventSource::from_event_type(&EventType::FileSwitched {
                file_path: "main.rs".into(), language: Some("rust".into()),
                git_branch: None, line_count: None,
            }),
            EventSource::Editor
        );
    }
}
