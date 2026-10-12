//! Runtime Evaluator for K Language.
//!
//! Executes KernelOps using HTTP / Socket clients to query live kernel state,
//! historical ledger events, and causal graph associations.

use crate::planner::{BehaviorTriggerOp, KernelOp};
use anyhow::Result;
use mnse_graph::{CausalDiffusion, GraphDb, PageRankDiffusion};
use mnse_ledger::{EventFilter, Ledger};
use mnse_sdk::{HttpClient, SocketClient};
use mnse_types::ContextSnapshot;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KContext {
    pub bindings: HashMap<String, serde_json::Value>,
}

pub struct Runtime {
    http_client: HttpClient,
    _socket_client: SocketClient,
}

impl Runtime {
    pub fn new(http_client: HttpClient, socket_client: SocketClient) -> Self {
        Self {
            http_client,
            _socket_client: socket_client,
        }
    }

    pub fn default_local() -> Self {
        Self::new(HttpClient::default_local(), SocketClient::default_local())
    }

    /// Evaluate an entire array of KernelOps
    pub async fn eval_ops(&self, ops: &[KernelOp], ctx: &mut KContext) -> Result<serde_json::Value> {
        let mut last_res = serde_json::Value::Null;
        for op in ops {
            last_res = self.eval_op(op, ctx).await?;
        }
        Ok(last_res)
    }

    /// Evaluate a single KernelOp
    pub async fn eval_op(&self, op: &KernelOp, ctx: &mut KContext) -> Result<serde_json::Value> {
        match op {
            KernelOp::QueryCurrent(target) => match target.as_str() {
                "file" | "editor_file" => {
                    let snap = self.get_snapshot().await?;
                    Ok(serde_json::Value::String(
                        snap.editor_file.unwrap_or_else(|| "[none]".to_string()),
                    ))
                }
                "app" | "active_app" => {
                    let snap = self.get_snapshot().await?;
                    Ok(serde_json::Value::String(
                        snap.active_app.unwrap_or_else(|| "[none]".to_string()),
                    ))
                }
                "state" | "snapshot" | "toon" => {
                    let snap = self.get_snapshot().await?;
                    Ok(serde_json::Value::String(snap.to_toon_rich()))
                }
                "compact" => {
                    let snap = self.get_snapshot().await?;
                    Ok(serde_json::Value::String(snap.to_toon()))
                }
                "json" => {
                    let snap = self.get_snapshot().await?;
                    Ok(serde_json::to_value(snap)?)
                }
                _ => {
                    let snap = self.get_snapshot().await?;
                    Ok(serde_json::Value::String(snap.to_toon_rich()))
                }
            },

            KernelOp::QueryRecent(target) => match target.as_str() {
                "errors" | "recent.errors" => {
                    let snap = self.get_snapshot().await?;
                    Ok(serde_json::to_value(snap.recent_errors)?)
                }
                "files" | "recent.files" => {
                    let snap = self.get_snapshot().await?;
                    Ok(serde_json::to_value(snap.recent_files)?)
                }
                "commands" | "recent.commands" => {
                    let snap = self.get_snapshot().await?;
                    Ok(serde_json::to_value(snap.recent_commands)?)
                }
                _ => {
                    let snap = self.get_snapshot().await?;
                    Ok(serde_json::to_value(snap.recent_commands)?)
                }
            },

            KernelOp::QueryHistory(range) => {
                let date_str = match range.as_str() {
                    "today" => chrono::Utc::now().format("%Y-%m-%d").to_string(),
                    "yesterday" => (chrono::Utc::now() - chrono::Duration::days(1))
                        .format("%Y-%m-%d")
                        .to_string(),
                    _ => range.clone(),
                };

                let events = match self.http_client.get_ledger_events(Some(&date_str), Some(50)).await {
                    Ok(evs) => evs,
                    Err(_) => {
                        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
                        let ledger_path = home.join(".mnse/ledger.db");
                        if ledger_path.exists() {
                            match Ledger::open_read_only(&ledger_path) {
                                Ok(ledger) => {
                                    let filter = EventFilter::new().with_date(&date_str).with_limit(50);
                                    ledger.query(filter).unwrap_or_default()
                                }
                                Err(_) => Vec::new(),
                            }
                        } else {
                            Vec::new()
                        }
                    }
                };

                Ok(serde_json::to_value(events)?)
            }

            KernelOp::QueryCausal { op, target } => {
                let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
                let graph_path = home.join(".mnse/graph.db");

                if !graph_path.exists() {
                    return Ok(serde_json::json!({
                        "causal_op": op,
                        "target": target,
                        "attributions": [],
                        "status": "Graph database not yet initialized"
                    }));
                }

                let db = match GraphDb::new(&graph_path) {
                    Ok(db) => db,
                    Err(e) => {
                        return Ok(serde_json::json!({
                            "causal_op": op,
                            "target": target,
                            "error": format!("Failed to open graph database: {}", e)
                        }));
                    }
                };

                let start_node_opt = if target.starts_with("latest.error") || target == "error" {
                    db.get_latest_node_by_type("Error").unwrap_or(None)
                } else if target.starts_with("latest.cmd") || target == "command" {
                    db.get_latest_node_by_type("Command").unwrap_or(None)
                } else if target.starts_with("latest.file") || target == "file" {
                    db.get_latest_node_by_type("File").unwrap_or(None)
                } else if let Ok(Some(n)) = db.get_node(target) {
                    Some(n)
                } else {
                    db.search_nodes(target).ok().and_then(|v| v.into_iter().next())
                };

                let start_node = match start_node_opt {
                    Some(n) => n,
                    None => {
                        return Ok(serde_json::json!({
                            "causal_op": op,
                            "target": target,
                            "attributions": [],
                            "explanation": format!("No active knowledge graph nodes found matching target '{}'", target)
                        }));
                    }
                };

                let seed_id = start_node.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let seed_type = start_node.get("type").and_then(|v| v.as_str()).unwrap_or("");
                let seed_label = start_node.get("label").and_then(|v| v.as_str()).unwrap_or("");

                let model = PageRankDiffusion::default();
                let attributions = match model.diffuse(&db, seed_id, 5) {
                    Ok(attrs) => attrs,
                    Err(e) => {
                        return Ok(serde_json::json!({
                            "causal_op": op,
                            "target": target,
                            "seed_id": seed_id,
                            "error": format!("Diffusion calculation failed: {}", e)
                        }));
                    }
                };

                Ok(serde_json::json!({
                    "causal_op": op,
                    "target": target,
                    "seed": {
                        "id": seed_id,
                        "type": seed_type,
                        "label": seed_label,
                    },
                    "attributions": attributions
                }))
            }

            KernelOp::Let { name, val } => {
                let evaled = Box::pin(self.eval_op(val, ctx)).await?;
                ctx.bindings.insert(name.clone(), evaled.clone());
                Ok(evaled)
            }

            KernelOp::Var(name) => {
                if let Some(val) = ctx.bindings.get(name) {
                    Ok(val.clone())
                } else {
                    Ok(serde_json::Value::Null)
                }
            }

            KernelOp::Literal(val) => Ok(val.clone()),

            KernelOp::Pipeline(stages) => {
                let mut current_val = serde_json::Value::Null;
                for stage in stages {
                    ctx.bindings.insert("it".to_string(), current_val.clone());
                    current_val = Box::pin(self.eval_op(stage, ctx)).await?;
                }
                Ok(current_val)
            }

            KernelOp::Where(pred) => {
                let it = ctx.bindings.get("it").cloned().unwrap_or(serde_json::Value::Null);
                if let serde_json::Value::Array(items) = it {
                    let mut filtered = Vec::new();
                    for item in items {
                        ctx.bindings.insert("it".to_string(), item.clone());
                        let res = Box::pin(self.eval_op(pred, ctx)).await?;
                        if res.as_bool().unwrap_or(true) {
                            filtered.push(item);
                        }
                    }
                    Ok(serde_json::Value::Array(filtered))
                } else {
                    Ok(it)
                }
            }

            KernelOp::Select(fields) => {
                let it = ctx.bindings.get("it").cloned().unwrap_or(serde_json::Value::Null);
                if let serde_json::Value::Object(map) = it {
                    let mut proj = serde_json::Map::new();
                    for f in fields {
                        if let Some(v) = map.get(f) {
                            proj.insert(f.clone(), v.clone());
                        }
                    }
                    Ok(serde_json::Value::Object(proj))
                } else {
                    Ok(it)
                }
            }

            KernelOp::Behavior { name, triggers } => {
                self.mount_behavior(name, triggers).await?;
                Ok(serde_json::json!({ "behavior": name, "status": "mounted" }))
            }

            KernelOp::Care => {
                let snap = self.get_snapshot().await?;
                Ok(serde_json::json!({
                    "kernel_status": "online",
                    "adapters": snap.adapters,
                    "privacy_mode": snap.privacy_mode
                }))
            }

            KernelOp::Summary { range } => {
                let date_str = match range.as_str() {
                    "today" => chrono::Utc::now().format("%Y-%m-%d").to_string(),
                    "yesterday" => (chrono::Utc::now() - chrono::Duration::days(1))
                        .format("%Y-%m-%d")
                        .to_string(),
                    _ => range.clone(),
                };

                let stats = match self.http_client.get_daily_stats(&date_str).await {
                    Ok(s) => s,
                    Err(_) => {
                        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
                        let ledger_path = home.join(".mnse/ledger.db");
                        if ledger_path.exists() {
                            match Ledger::open_read_only(&ledger_path) {
                                Ok(ledger) => ledger.daily_stats(&date_str).unwrap_or_default(),
                                Err(_) => mnse_types::DailyStats::default(),
                            }
                        } else {
                            mnse_types::DailyStats::default()
                        }
                    }
                };

                Ok(serde_json::json!({
                    "summary_range": range,
                    "date": stats.date,
                    "events_total": stats.events_total,
                    "commands_run": stats.commands_run,
                    "files_touched": stats.files_touched,
                    "errors_count": stats.errors_count,
                    "focused_apps": stats.focused_apps
                }))
            }

            KernelOp::Help => {
                let help_text = "\
K Language v2.0 (MNSE Kernel) - Command Reference

Present State Queries:
  current.state (or 'state')       Live developer context (app, window, cwd, editor, diag)
  current.app   (or 'app')         Active window / application name
  current.file                     Current active file in editor
  current.toon                     Compact single-line TOON state representation
  current.json                     Full machine JSON snapshot

Recent Activity Queries:
  recent.errors   (or 'errors')    Recent compiler and runtime errors
  recent.commands (or 'commands')  Recent shell commands executed
  recent.files    (or 'files')     Recent files switched or edited

Historical Ledger Queries:
  history today                    Ledger events from today
  history yesterday                Ledger events from yesterday
  history YYYY-MM-DD               Ledger events from a specific date
  summary today                    Activity statistics for today
  summary YYYY-MM-DD               Activity statistics for a specific date

Causal Reasoning:
  cause of error                   Attribution graph search for latest error
  cause of <target>                Graph diffusion search for target entity
  related <query>                  Cosine and graph similarity search

Adapter & System Management:
  adapter list                     List adapter connection, status & telemetry
  adapter on <name>                Enable adapter (os, hyprland, fs, shell, browser)
  adapter off <name>               Disable adapter
  care                             Kernel daemon health and privacy status
  help                             Display this help reference\n";
                Ok(serde_json::Value::String(help_text.to_string()))
            }

            KernelOp::Action(action) => {
                let event = mnse_types::Event::new(
                    mnse_types::EventType::ActionRequested(action.clone()),
                    "k-script",
                );
                let _ = self.http_client.emit(&event).await;
                Ok(serde_json::json!({ "action": "dispatched" }))
            }

            KernelOp::Adapter { action, name } => match action.as_str() {
                "list" | "status" => {
                    match self.http_client.get_adapters().await {
                        Ok(list) => {
                            let mut out = String::new();
                            out.push_str("ADAPTERS\n");
                            for item in list {
                                let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
                                let enabled = item.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
                                let connected = item.get("connected").and_then(|v| v.as_bool()).unwrap_or(false);
                                let count = item.get("event_count").and_then(|v| v.as_u64()).unwrap_or(0);
                                let last = item.get("last_seen").and_then(|v| v.as_str()).unwrap_or("--:--");
                                let short_last = if last.len() >= 16 { &last[11..16] } else { "--:--" };

                                let state_tag = if enabled {
                                    if connected { "online  [enabled ]" } else { "offline [enabled ]" }
                                } else {
                                    "muted   [disabled]"
                                };

                                out.push_str(&format!(
                                    "  {:<10}  {:<18}  {:>5} events   last seen {}\n",
                                    name, state_tag, count, short_last
                                ));
                            }
                            Ok(serde_json::Value::String(out))
                        }
                        Err(e) => Ok(serde_json::json!({ "error": format!("Kernel offline or unreachable: {}", e) })),
                    }
                }
                "on" | "enable" => {
                    if let Some(n) = name {
                        match self.http_client.enable_adapter(n).await {
                            Ok(_) => Ok(serde_json::Value::String(format!("Adapter '{}' is now ENABLED (ON)", n))),
                            Err(e) => Ok(serde_json::Value::String(format!("Failed to enable adapter '{}': {}", n, e))),
                        }
                    } else {
                        Ok(serde_json::Value::String("Error: specify adapter name to enable (e.g. 'k adapter on os')".to_string()))
                    }
                }
                "off" | "disable" => {
                    if let Some(n) = name {
                        match self.http_client.disable_adapter(n).await {
                            Ok(_) => Ok(serde_json::Value::String(format!("Adapter '{}' is now DISABLED (OFF)", n))),
                            Err(e) => Ok(serde_json::Value::String(format!("Failed to disable adapter '{}': {}", n, e))),
                        }
                    } else {
                        Ok(serde_json::Value::String("Error: specify adapter name to disable (e.g. 'k adapter off os')".to_string()))
                    }
                }
                _ => Ok(serde_json::Value::String(format!("Unknown adapter action: {}", action))),
            },

            _ => Ok(serde_json::Value::Null),
        }
    }

    async fn get_snapshot(&self) -> Result<ContextSnapshot> {
        self.http_client.get_snapshot("json").await
    }

    /// Mount behavior and serialize to disk (`~/.mnse/behaviors.json`)
    async fn mount_behavior(&self, name: &str, triggers: &[BehaviorTriggerOp]) -> Result<()> {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let behaviors_file = home.join(".mnse/behaviors.json");

        let mut existing: HashMap<String, serde_json::Value> = if behaviors_file.exists() {
            let content = std::fs::read_to_string(&behaviors_file)?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            HashMap::new()
        };

        existing.insert(name.to_string(), serde_json::to_value(triggers)?);
        if let Some(parent) = behaviors_file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&behaviors_file, serde_json::to_string_pretty(&existing)?)?;
        tracing::info!("Mounted and serialized behavior '{}' to {:?}", name, behaviors_file);
        Ok(())
    }
}
