//! GraphBuilder — Short-Term Memory (STM) & Causal Fact Graph Ingestion.
//!
//! Implements the Historian Model (ADR 004):
//! Records facts as they occur. When files are edited or commands fail,
//! items active in STM (5-minute sliding window) are linked via `IN_STM_DURING`.

use crate::db::GraphDb;
use crate::ml::{HawkesDecayModel, TemporalDecayModel};
use mnse_types::{Event, EventType};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
struct STMItem {
    id: String,
    node_type: String,
    _label: String,
    timestamp: i64,
}

pub struct GraphBuilder {
    db: Arc<GraphDb>,
    stm: Arc<Mutex<VecDeque<STMItem>>>,
    decay_model: Arc<dyn TemporalDecayModel>,
}

impl GraphBuilder {
    pub fn new(db: Arc<GraphDb>) -> Self {
        Self {
            db,
            stm: Arc::new(Mutex::new(VecDeque::new())),
            decay_model: Arc::new(HawkesDecayModel::default()),
        }
    }

    pub fn with_decay_model(mut self, decay_model: Arc<dyn TemporalDecayModel>) -> Self {
        self.decay_model = decay_model;
        self
    }

    /// Process incoming event for graph construction & STM tracking
    pub async fn process_event(&self, event: &Event) {
        let event_ts = event.timestamp.timestamp();
        self.decay_model.record_event(event_ts);
        self.prune_stm(event_ts);

        match &event.event_type {
            EventType::ProcessExecuted {
                command, exit_code, ..
            } => {
                self.handle_command(event_ts, command, *exit_code, event.payload.as_ref());
            }

            EventType::ErrorDetected {
                command,
                exit_code,
                error_pattern,
                working_dir,
            } => {
                self.handle_error(event_ts, command, *exit_code, error_pattern.as_deref(), working_dir);
            }

            EventType::WindowFocused {
                app_name,
                window_title,
                workspace,
                ..
            } => {
                self.handle_window_focus(event_ts, app_name, window_title, workspace.unwrap_or(0));
            }

            EventType::FileSwitched {
                file_path,
                language,
                git_branch,
                ..
            } => {
                self.handle_file_switched(event_ts, file_path, language.as_deref(), git_branch.as_deref());
            }

            EventType::UrlChanged {
                url,
                title,
                domain,
                search_query,
            } => {
                self.handle_browser_url(event_ts, url, title.as_deref(), domain, search_query.as_deref());
            }

            EventType::ClipboardChanged {
                content_hash,
                content_preview,
                content_len,
            } => {
                self.handle_clipboard(event_ts, content_hash, content_preview.as_deref(), *content_len);
            }

            EventType::PersonVisible { identity, confidence } => {
                let person_id = format!("person:{}", identity);
                let _ = self.db.upsert_node(
                    &person_id,
                    "Person",
                    identity,
                    Some(serde_json::json!({ "confidence": confidence })),
                );
                self.add_to_stm(person_id, "Person", identity, event_ts);
            }

            _ => {}
        }
    }

    fn handle_command(&self, event_ts: i64, command: &str, exit_code: i32, payload: Option<&serde_json::Value>) {
        if command.is_empty() {
            return;
        }

        let cmd_hash = format!("{:x}", md5::compute(command));
        let node_id = format!("cmd:{}", &cmd_hash[..8]);

        let _ = self.db.upsert_node(
            &node_id,
            "Command",
            command,
            Some(serde_json::json!({ "exit_code": exit_code })),
        );

        if let Some(cwd) = payload.and_then(|p| p.get("cwd")).and_then(|v| v.as_str()) {
            let dir_id = format!("dir:{}", cwd);
            let _ = self.db.upsert_node(&dir_id, "Directory", cwd, None);
            let _ = self.db.upsert_edge(&node_id, &dir_id, "EXECUTED_IN", 1, 1.0);
        }

        if exit_code != 0 {
            self.add_to_stm(node_id, "Command", command, event_ts);
        }
    }

    fn handle_error(&self, event_ts: i64, command: &str, exit_code: i32, pattern: Option<&str>, working_dir: &str) {
        let err_label = pattern.unwrap_or(command);
        let err_hash = format!("{:x}", md5::compute(err_label));
        let err_id = format!("error:{}", &err_hash[..8]);

        let _ = self.db.upsert_node(
            &err_id,
            "Error",
            err_label,
            Some(serde_json::json!({
                "command": command,
                "exit_code": exit_code,
                "dir": working_dir
            })),
        );

        let dir_id = format!("dir:{}", working_dir);
        let _ = self.db.upsert_node(&dir_id, "Directory", working_dir, None);
        let _ = self.db.upsert_edge(&err_id, &dir_id, "OCCURRED_IN", 1, 1.0);

        self.add_to_stm(err_id, "Error", err_label, event_ts);
    }

    fn handle_window_focus(&self, event_ts: i64, app_name: &str, window_title: &str, workspace: u32) {
        let app_id = format!("app:{}", app_name);
        let _ = self.db.upsert_node(
            &app_id,
            "App",
            app_name,
            Some(serde_json::json!({ "workspace": workspace })),
        );
        self.add_to_stm(app_id.clone(), "App", app_name, event_ts);

        let mut hasher = Sha256::new();
        hasher.update(format!("{}{}", app_name, window_title).as_bytes());
        let win_hash = hex::encode(hasher.finalize());
        let win_id = format!("win:{}", &win_hash[..8]);

        let _ = self.db.upsert_node(
            &win_id,
            "Window",
            window_title,
            Some(serde_json::json!({ "app": app_name, "workspace": workspace })),
        );

        let _ = self.db.upsert_edge(&win_id, &app_id, "INSTANCE_OF", 1, 1.0);
        self.add_to_stm(win_id, "Window", window_title, event_ts);
    }

    fn handle_file_switched(&self, event_ts: i64, file_path: &str, language: Option<&str>, git_branch: Option<&str>) {
        let file_id = format!("file://{}", file_path);
        let mut props = serde_json::json!({});
        if let Some(lang) = language {
            props["language"] = serde_json::json!(lang);
        }
        if let Some(branch) = git_branch {
            props["git_branch"] = serde_json::json!(branch);
        }

        let _ = self.db.upsert_node(&file_id, "File", file_path, Some(props));

        // Link active STM items to this file (IN_STM_DURING)
        let stm = match self.stm.lock() {
            Ok(g) => g,
            Err(_) => return,
        };

        for item in stm.iter() {
            if event_ts >= item.timestamp && (item.node_type == "Error" || item.node_type == "Concept" || item.node_type == "Command") {
                let _ = self.db.upsert_edge(&item.id, &file_id, "IN_STM_DURING", 1, 1.0);
            }
        }
    }

    fn handle_browser_url(&self, event_ts: i64, url_str: &str, _title: Option<&str>, domain: &str, search_query: Option<&str>) {
        let domain_id = format!("domain:{}", domain);
        let _ = self.db.upsert_node(&domain_id, "Domain", domain, None);

        if let Some(query) = search_query {
            if !query.is_empty() {
                let concept_id = format!("concept:{}", query);
                let _ = self.db.upsert_node(
                    &concept_id,
                    "Concept",
                    query,
                    Some(serde_json::json!({ "url": url_str })),
                );
                let _ = self.db.upsert_edge(&concept_id, &domain_id, "SEARCHED_ON", 1, 1.0);
                self.add_to_stm(concept_id, "Concept", query, event_ts);
            }
        }
    }

    fn handle_clipboard(&self, event_ts: i64, content_hash: &str, preview: Option<&str>, content_len: usize) {
        let clip_id = format!("clip:{}", &content_hash[..8.min(content_hash.len())]);
        let label = preview.unwrap_or("[clipboard]");

        let _ = self.db.upsert_node(
            &clip_id,
            "Clipboard",
            label,
            Some(serde_json::json!({ "hash": content_hash, "length": content_len })),
        );

        let stm = match self.stm.lock() {
            Ok(g) => g,
            Err(_) => return,
        };

        if let Some(app_item) = stm.iter().rev().find(|i| i.node_type == "App") {
            let _ = self.db.upsert_edge(&clip_id, &app_item.id, "COPIED_FROM", 1, 1.0);
        }
        drop(stm);

        self.add_to_stm(clip_id, "Clipboard", label, event_ts);
    }

    fn add_to_stm(&self, id: String, node_type: &str, label: &str, timestamp: i64) {
        if let Ok(mut stm) = self.stm.lock() {
            stm.push_back(STMItem {
                id,
                node_type: node_type.to_string(),
                _label: label.to_string(),
                timestamp,
            });
        }
    }

    fn prune_stm(&self, current_ts: i64) {
        if let Ok(mut stm) = self.stm.lock() {
            while let Some(front) = stm.front() {
                if !self.decay_model.should_retain(front.timestamp, current_ts) {
                    stm.pop_front();
                } else {
                    break;
                }
            }
        }
    }
}
