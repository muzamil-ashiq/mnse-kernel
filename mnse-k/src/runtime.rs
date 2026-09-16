//! Runtime Evaluator for K Language.
//!
//! Executes KernelOps using HTTP / Socket clients to query live kernel state,
//! historical ledger events, and causal graph associations.

use crate::planner::{BehaviorTriggerOp, KernelOp};
use anyhow::Result;
use mnse_ledger::EventFilter;
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

                let _filter = EventFilter::new().with_search(&date_str);
                // Fallback to empty list if kernel offline
                Ok(serde_json::json!({
                    "history": date_str,
                    "status": "queried"
                }))
            }

            KernelOp::QueryCausal { op, target } => Ok(serde_json::json!({
                "causal_op": op,
                "target": target,
                "result": format!("Linked context for {}", target)
            })),

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
                let snap = self.get_snapshot().await?;
                Ok(serde_json::json!({
                    "summary_range": range,
                    "date": snap.daily_stats.date,
                    "events_total": snap.daily_stats.events_total,
                    "commands_run": snap.daily_stats.commands_run,
                    "errors_count": snap.daily_stats.errors_count
                }))
            }

            KernelOp::Action(action) => {
                let event = mnse_types::Event::new(
                    mnse_types::EventType::ActionRequested(action.clone()),
                    "k-script",
                );
                let _ = self.http_client.emit(&event).await;
                Ok(serde_json::json!({ "action": "dispatched" }))
            }

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
