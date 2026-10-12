//! InspectorState — Live Context State Engine.
//!
//! Maintains the canonical live snapshot (`ContextSnapshot`) in memory.
//! Updated atomically as events pass through the EventBus.
//! Computes `GET /snapshot` and `k current.state` responses.

use chrono::Utc;
use mnse_types::{
    AdapterStatus, ContextSnapshot, Event, EventType, RecentCommand, RecentError,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub struct InspectorState {
    inner: Arc<Mutex<ContextSnapshot>>,
    adapter_health: Arc<Mutex<HashMap<String, AdapterStatus>>>,
}

impl InspectorState {
    pub fn new(privacy_mode: bool) -> Self {
        let mut snap = ContextSnapshot::empty();
        snap.privacy_mode = privacy_mode;

        Self {
            inner: Arc::new(Mutex::new(snap)),
            adapter_health: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Return a copy of the current live snapshot
    pub fn snapshot(&self) -> ContextSnapshot {
        let mut snap = match self.inner.lock() {
            Ok(g) => g.clone(),
            Err(_) => ContextSnapshot::empty(),
        };

        snap.timestamp = Utc::now();

        // Populate adapter health statuses
        if let Ok(health) = self.adapter_health.lock() {
            snap.adapters = health.values().cloned().collect();
        }

        snap
    }

    /// Process incoming event and update live state
    pub fn process_event(&self, event: &Event) {
        // Epistemic Paradox Fix (ADR 001): Only update health if event is directly observed by that adapter
        if !event.observed_by.is_empty() && event.observed_by != "system" && event.observed_by != "unknown" {
            if let Ok(mut health) = self.adapter_health.lock() {
                let status = health.entry(event.observed_by.clone()).or_insert_with(|| AdapterStatus {
                    name: event.observed_by.clone(),
                    connected: true,
                    last_seen: Some(event.timestamp),
                    event_count: 0,
                    version: None,
                });
                status.connected = true;
                status.last_seen = Some(event.timestamp);
                status.event_count += 1;
            }
        }

        let mut snap = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => return,
        };

        snap.timestamp = Utc::now();

        match &event.event_type {
            EventType::WindowFocused {
                app_name,
                window_title,
                window_class,
                window_id,
                workspace,
                ..
            } => {
                snap.active_app = Some(app_name.clone());
                snap.window_title = Some(window_title.clone());
                snap.window_class = Some(window_class.clone());
                snap.window_id = Some(window_id.clone());
                snap.workspace = *workspace;
            }

            EventType::WorkspaceChanged {
                workspace_id,
                workspace_count,
                ..
            } => {
                if *workspace_id > 0 {
                    snap.workspace = Some(*workspace_id);
                }
                if let Some(count) = workspace_count {
                    snap.workspace_count = Some(*count);
                }
            }

            EventType::ProcessExecuted {
                command,
                exit_code,
                duration_ms,
                working_dir,
                ..
            } => {
                snap.last_command = Some(command.clone());
                snap.last_exit_code = Some(*exit_code);
                snap.last_command_duration_ms = Some(*duration_ms);
                snap.cwd = Some(working_dir.clone());

                if *exit_code != 0 {
                    snap.terminal_has_error = true;
                    snap.last_error_command = Some(command.clone());
                    snap.recent_errors.insert(
                        0,
                        RecentError {
                            command: command.clone(),
                            exit_code: *exit_code,
                            pattern: None,
                            timestamp: event.timestamp,
                        },
                    );
                    snap.recent_errors.truncate(10);
                } else {
                    snap.terminal_has_error = false;
                }

                snap.recent_commands.insert(
                    0,
                    RecentCommand {
                        command: command.clone(),
                        exit_code: *exit_code,
                        cwd: working_dir.clone(),
                        timestamp: event.timestamp,
                        duration_ms: *duration_ms,
                    },
                );
                snap.recent_commands.truncate(10);
            }

            EventType::ErrorDetected {
                command,
                exit_code,
                error_pattern,
                ..
            } => {
                snap.terminal_has_error = true;
                snap.error_pattern = error_pattern.clone();
                snap.last_error_command = Some(command.clone());

                snap.recent_errors.insert(
                    0,
                    RecentError {
                        command: command.clone(),
                        exit_code: *exit_code,
                        pattern: error_pattern.clone(),
                        timestamp: event.timestamp,
                    },
                );
                snap.recent_errors.truncate(10);
            }

            EventType::FileSwitched {
                file_path,
                language,
                git_branch,
                ..
            } => {
                snap.editor_file = Some(file_path.clone());
                snap.editor_language = language.clone();
                snap.git_branch = git_branch.clone();

                if !snap.recent_files.contains(file_path) {
                    snap.recent_files.insert(0, file_path.clone());
                    snap.recent_files.truncate(10);
                }
            }

            EventType::UrlChanged {
                url,
                title,
                domain,
                search_query,
            } => {
                snap.browser_url = Some(url.clone());
                snap.browser_title = title.clone();
                snap.browser_domain = Some(domain.clone());
                snap.browser_search_query = search_query.clone();
            }

            EventType::ClipboardChanged {
                content_hash,
                content_preview,
                ..
            } => {
                snap.clipboard_hash = Some(content_hash.clone());
                snap.clipboard_preview = content_preview.clone();
            }

            EventType::PersonVisible { identity, confidence } => {
                snap.person_visible = Some(identity.clone());
                snap.person_confidence = Some(*confidence);
            }

            EventType::PersonGone { .. } => {
                snap.person_visible = None;
                snap.person_confidence = None;
            }

            EventType::TranscriptionResult { text, .. } => {
                snap.last_transcription = Some(text.clone());
            }

            EventType::ResourcesUpdated {
                cpu_percent,
                memory_used_mb,
                memory_total_mb,
                battery_percent,
            } => {
                snap.cpu_percent = Some(*cpu_percent);
                snap.memory_used_mb = Some(*memory_used_mb);
                snap.memory_total_mb = Some(*memory_total_mb);
                snap.battery_percent = *battery_percent;
            }

            EventType::AdapterConnected {
                adapter_name,
                protocol_version,
                ..
            } => {
                if let Ok(mut health) = self.adapter_health.lock() {
                    health.insert(
                        adapter_name.clone(),
                        AdapterStatus {
                            name: adapter_name.clone(),
                            connected: true,
                            last_seen: Some(event.timestamp),
                            event_count: 1,
                            version: Some(protocol_version.clone()),
                        },
                    );
                }
            }

            EventType::AdapterDisconnected { adapter_name, .. } => {
                if let Ok(mut health) = self.adapter_health.lock() {
                    if let Some(status) = health.get_mut(adapter_name) {
                        status.connected = false;
                    }
                }
            }

            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inspector_state_updates() {
        let inspector = InspectorState::new(true);

        let event = Event::new(
            EventType::WindowFocused {
                app_name: "code".into(),
                window_title: "main.rs - Project".into(),
                window_class: "code".into(),
                window_id: "0x123".into(),
                pid: None,
                workspace: Some(1),
            },
            "os",
        );

        inspector.process_event(&event);

        let snap = inspector.snapshot();
        assert_eq!(snap.active_app.unwrap(), "code");
        assert_eq!(snap.workspace, Some(1));
    }
}
