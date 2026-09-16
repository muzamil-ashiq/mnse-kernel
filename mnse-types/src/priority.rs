//! Priority levels for event routing in the EventBus.
//!
//! High-priority events get routed first regardless of queue depth.
//! This enables the Reflex Layer: sensor event → actuator response in <5ms
//! for critical hardware signals (FallDetected, ObstacleDetected).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Low    = 0,
    Medium = 1,
    High   = 2,
    /// Reflex path — bypasses ledger/graph, direct sensor→actuator routing
    Critical = 3,
}

impl Default for Priority {
    fn default() -> Self {
        Self::Medium
    }
}

impl Priority {
    /// Default priority assignment based on event type name.
    /// The Rhai RuleEngine can override this per-event.
    pub fn from_event_type_name(name: &str) -> Self {
        match name {
            // Critical: direct hardware reflex path
            "ObstacleDetected" | "FallDetected" | "EmergencyStop" => Self::Critical,

            // High: user needs to know immediately
            "ErrorDetected" | "WakeWordDetected" | "ActionRequested" => Self::High,

            // Medium: normal activity
            "ProcessExecuted" | "WindowFocused" | "FileSwitched"
            | "UrlChanged" | "PersonVisible" | "PersonGone"
            | "TranscriptionResult" | "DiagnosticsUpdated"
            | "WorkspaceChanged" | "ClipboardChanged" | "DirectoryChanged"
            | "AgentStarted" | "AgentEnded"
            | "AdapterConnected" | "AdapterDisconnected" => Self::Medium,

            // Low: noise and housekeeping (many of these never reach SQLite anyway)
            _ => Self::Low,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_ordering_is_correct() {
        assert!(Priority::Critical > Priority::High);
        assert!(Priority::High > Priority::Medium);
        assert!(Priority::Medium > Priority::Low);
    }

    #[test]
    fn error_is_high_priority() {
        assert_eq!(Priority::from_event_type_name("ErrorDetected"), Priority::High);
    }

    #[test]
    fn cursor_moved_is_low_priority() {
        assert_eq!(Priority::from_event_type_name("CursorMoved"), Priority::Low);
    }

    #[test]
    fn wake_word_is_high_priority() {
        assert_eq!(Priority::from_event_type_name("WakeWordDetected"), Priority::High);
    }
}
