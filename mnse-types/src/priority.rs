//! Priority levels for event routing in the EventBus.
//!
//! High-priority events get routed first regardless of queue depth.
//! Fast-path dispatch is used for critical error detection and fast response.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Low    = 0,
    Medium = 1,
    High   = 2,
    Critical = 3,
}

impl Default for Priority {
    fn default() -> Self {
        Self::Medium
    }
}

impl Priority {
    /// Default priority assignment based on event type name.
    pub fn from_event_type_name(name: &str) -> Self {
        match name {
            // High: user / agent needs to know immediately
            "ErrorDetected" => Self::High,

            // Medium: normal activity
            "ProcessExecuted"
            | "WindowFocused"
            | "FileSwitched"
            | "DiagnosticsUpdated"
            | "WorkspaceChanged"
            | "ClipboardChanged"
            | "DirectoryChanged"
            | "AdapterConnected"
            | "AdapterDisconnected" => Self::Medium,

            // Low: noise and housekeeping
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
    fn process_executed_is_medium_priority() {
        assert_eq!(Priority::from_event_type_name("ProcessExecuted"), Priority::Medium);
    }
}
