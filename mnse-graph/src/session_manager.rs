//! Session & Burst Hierarchy Manager (Time-Tree Model).
//!
//! Organizes raw events into:
//! Work Session (60-min window) -> Activity Burst (5-min window) -> Event Fact

use crate::db::GraphDb;
use mnse_types::{Event, EventType};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

const BURST_TIMEOUT_SECS: i64 = 5 * 60;   // 5 minutes
const SESSION_TIMEOUT_SECS: i64 = 60 * 60; // 60 minutes

#[derive(Debug, Clone)]
struct ActiveSession {
    id: String,
    last_event_time: i64,
}

#[derive(Debug, Clone)]
struct ActiveBurst {
    id: String,
    start_time: i64,
    last_event_time: i64,
}

pub struct SessionManager {
    db: Arc<GraphDb>,
    session: Arc<Mutex<Option<ActiveSession>>>,
    burst: Arc<Mutex<Option<ActiveBurst>>>,
}

impl SessionManager {
    pub fn new(db: Arc<GraphDb>) -> Self {
        Self {
            db,
            session: Arc::new(Mutex::new(None)),
            burst: Arc::new(Mutex::new(None)),
        }
    }

    /// Process incoming event and integrate into the Time-Tree hierarchy
    pub async fn process_event(&self, event: &Event) {
        // Noise events do not update sessions or bursts
        if event.event_type.is_noise() {
            return;
        }

        let event_ts = event.timestamp.timestamp();
        let mut session_id = String::new();

        // 1. Session Lifecycle
        {
            let mut session_guard = match self.session.lock() {
                Ok(g) => g,
                Err(e) => {
                    tracing::error!("Session lock poisoned: {}", e);
                    return;
                }
            };

            let should_create = if let Some(s) = session_guard.as_ref() {
                event_ts - s.last_event_time > SESSION_TIMEOUT_SECS
            } else {
                true
            };

            if should_create {
                let new_session = self.create_session(event_ts);
                session_id = new_session.id.clone();
                *session_guard = Some(new_session);

                // Reset burst on new session
                if let Ok(mut b_guard) = self.burst.lock() {
                    *b_guard = None;
                }
            } else if let Some(s) = session_guard.as_mut() {
                s.last_event_time = event_ts;
                session_id = s.id.clone();
                let _ = self.db.upsert_node(
                    &s.id,
                    "Session",
                    "Active Session",
                    Some(serde_json::json!({ "last_active": event_ts })),
                );
            }
        }

        if session_id.is_empty() {
            return;
        }

        // 2. Burst Lifecycle
        let mut burst_id = String::new();
        let mut burst_start_ts = 0i64;

        {
            let mut burst_guard = match self.burst.lock() {
                Ok(g) => g,
                Err(e) => {
                    tracing::error!("Burst lock poisoned: {}", e);
                    return;
                }
            };

            let should_create = if let Some(b) = burst_guard.as_ref() {
                event_ts - b.last_event_time > BURST_TIMEOUT_SECS
            } else {
                true
            };

            if should_create {
                let new_burst = self.create_burst(event_ts, &session_id);
                burst_id = new_burst.id.clone();
                burst_start_ts = new_burst.start_time;
                *burst_guard = Some(new_burst);
            } else if let Some(b) = burst_guard.as_mut() {
                b.last_event_time = event_ts;
                burst_id = b.id.clone();
                burst_start_ts = b.start_time;
                let _ = self.db.upsert_node(
                    &b.id,
                    "Burst",
                    "Active Burst",
                    Some(serde_json::json!({ "last_active": event_ts })),
                );
            }
        }

        // Skip linking out-of-order stale events
        if event_ts < burst_start_ts {
            tracing::warn!("Skipping stale event in session manager (ts < burst_start)");
            return;
        }

        // 3. Store Event Node & Link to Burst (IN_BURST)
        let event_node_id = event.id.to_string();
        let type_name = event.event_type.type_name();

        let mut props = serde_json::to_value(&event.event_type).unwrap_or_else(|_| serde_json::json!({}));
        if let Some(obj) = props.as_object_mut() {
            obj.insert("timestamp".to_string(), serde_json::json!(event_ts));
            obj.insert("observed_by".to_string(), serde_json::json!(&event.observed_by));
            obj.insert("source".to_string(), serde_json::json!(format!("{:?}", event.source)));
        }

        let _ = self.db.upsert_node(&event_node_id, "Event", type_name, Some(props));
        let _ = self.db.upsert_edge(&event_node_id, &burst_id, "IN_BURST", 1, 1.0);

        // 4. Semantic Node Linking (Commands, Files, Domains)
        match &event.event_type {
            EventType::ProcessExecuted { command, .. } => {
                let cmd_hash = format!("{:x}", md5::compute(command));
                let node_id = format!("cmd:{}", &cmd_hash[..8]);
                let _ = self.db.upsert_node(&node_id, "Command", command, None);
                let _ = self.db.upsert_edge(&event_node_id, &node_id, "EXECUTED", 1, 1.0);
            }
            EventType::FileSwitched { file_path, .. } => {
                let file_id = format!("file://{}", file_path);
                let _ = self.db.upsert_node(&file_id, "File", file_path, None);
                let _ = self.db.upsert_edge(&event_node_id, &file_id, "TOUCHED", 1, 1.0);
            }
            _ => {}
        }
    }

    fn create_session(&self, timestamp: i64) -> ActiveSession {
        let id = format!("session:{}", Uuid::new_v4());
        let label = format!("Session {}", timestamp);

        let _ = self.db.upsert_node(
            &id,
            "Session",
            &label,
            Some(serde_json::json!({
                "start_time": timestamp,
                "created_at": timestamp
            })),
        );

        ActiveSession {
            id,
            last_event_time: timestamp,
        }
    }

    fn create_burst(&self, timestamp: i64, session_id: &str) -> ActiveBurst {
        let id = format!("burst:{}", Uuid::new_v4());
        let label = format!("Burst {}", timestamp);

        let _ = self.db.upsert_node(
            &id,
            "Burst",
            &label,
            Some(serde_json::json!({
                "start_time": timestamp,
                "created_at": timestamp
            })),
        );

        let _ = self.db.upsert_edge(&id, session_id, "PART_OF", 1, 1.0);

        ActiveBurst {
            id,
            start_time: timestamp,
            last_event_time: timestamp,
        }
    }
}
