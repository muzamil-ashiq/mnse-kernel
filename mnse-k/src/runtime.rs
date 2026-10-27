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

pub fn val_to_i64(v: &serde_json::Value) -> Option<i64> {
    v.as_i64()
        .or_else(|| v.as_f64().map(|f| f as i64))
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

pub fn val_to_u64(v: &serde_json::Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.as_f64().filter(|f| *f >= 0.0).map(|f| f as u64))
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
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

            KernelOp::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let cond_val = Box::pin(self.eval_op(cond, ctx)).await?;
                let is_truthy = match cond_val {
                    serde_json::Value::Bool(b) => b,
                    serde_json::Value::Number(n) => n.as_f64().map(|v| v != 0.0).unwrap_or(false),
                    serde_json::Value::String(s) => !s.is_empty(),
                    serde_json::Value::Array(a) => !a.is_empty(),
                    serde_json::Value::Object(o) => !o.is_empty(),
                    serde_json::Value::Null => false,
                };

                if is_truthy {
                    let mut last = serde_json::Value::Null;
                    for op in then_branch {
                        last = Box::pin(self.eval_op(op, ctx)).await?;
                    }
                    Ok(last)
                } else if let Some(else_ops) = else_branch {
                    let mut last = serde_json::Value::Null;
                    for op in else_ops {
                        last = Box::pin(self.eval_op(op, ctx)).await?;
                    }
                    Ok(last)
                } else {
                    Ok(serde_json::Value::Null)
                }
            }

            KernelOp::TryCatch {
                try_body,
                catch_var,
                catch_body,
            } => {
                let mut try_result = Ok(serde_json::Value::Null);
                let mut caught_err: Option<String> = None;

                for op in try_body {
                    match Box::pin(self.eval_op(op, ctx)).await {
                        Ok(v) => try_result = Ok(v),
                        Err(e) => {
                            caught_err = Some(e.to_string());
                            break;
                        }
                    }
                }

                if let Some(err_msg) = caught_err {
                    ctx.bindings.insert(catch_var.clone(), serde_json::Value::String(err_msg));
                    let mut last = serde_json::Value::Null;
                    for op in catch_body {
                        last = Box::pin(self.eval_op(op, ctx)).await?;
                    }
                    Ok(last)
                } else {
                    try_result
                }
            }

            KernelOp::Var(name) => {
                let parts: Vec<&str> = name.split('.').collect();
                if let Some(mut val) = ctx.bindings.get(parts[0]).cloned() {
                    for part in &parts[1..] {
                        val = match val {
                            serde_json::Value::Object(map) => {
                                map.get(*part).cloned().unwrap_or(serde_json::Value::Null)
                            }
                            _ => serde_json::Value::Null,
                        };
                    }
                    Ok(val)
                } else if let Some(serde_json::Value::Object(map)) = ctx.bindings.get("it") {
                    if let Some(mut val) = map.get(parts[0]).cloned() {
                        for part in &parts[1..] {
                            val = match val {
                                serde_json::Value::Object(submap) => {
                                    submap.get(*part).cloned().unwrap_or(serde_json::Value::Null)
                                }
                                _ => serde_json::Value::Null,
                            };
                        }
                        Ok(val)
                    } else {
                        Ok(serde_json::Value::Null)
                    }
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
                        let mut item_ctx = ctx.clone();
                        item_ctx.bindings.insert("it".to_string(), item.clone());
                        if let serde_json::Value::Object(map) = &item {
                            for (k, v) in map {
                                item_ctx.bindings.insert(k.clone(), v.clone());
                            }
                        }
                        let res = Box::pin(self.eval_op(pred, &mut item_ctx)).await?;
                        if res.as_bool().unwrap_or(false) {
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
                match it {
                    serde_json::Value::Object(map) => {
                        let mut proj = serde_json::Map::new();
                        for f in fields {
                            if let Some(v) = map.get(f) {
                                proj.insert(f.clone(), v.clone());
                            }
                        }
                        Ok(serde_json::Value::Object(proj))
                    }
                    serde_json::Value::Array(items) => {
                        let mut projected_items = Vec::new();
                        for item in items {
                            if let serde_json::Value::Object(map) = item {
                                let mut proj = serde_json::Map::new();
                                for f in fields {
                                    if let Some(v) = map.get(f) {
                                        proj.insert(f.clone(), v.clone());
                                    }
                                }
                                projected_items.push(serde_json::Value::Object(proj));
                            } else {
                                projected_items.push(item);
                            }
                        }
                        Ok(serde_json::Value::Array(projected_items))
                    }
                    _ => Ok(it),
                }
            }

            KernelOp::Binary { op, left, right } => {
                let l = Box::pin(self.eval_op(left, ctx)).await?;
                let r = Box::pin(self.eval_op(right, ctx)).await?;

                let res = match op.as_str() {
                    "==" | "EqEq" => match (&l, &r) {
                        (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
                            if let (Some(fa), Some(fb)) = (a.as_f64(), b.as_f64()) {
                                (fa - fb).abs() < f64::EPSILON
                            } else {
                                l == r
                            }
                        }
                        _ => l == r,
                    },
                    "!=" | "Neq" => match (&l, &r) {
                        (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
                            if let (Some(fa), Some(fb)) = (a.as_f64(), b.as_f64()) {
                                (fa - fb).abs() >= f64::EPSILON
                            } else {
                                l != r
                            }
                        }
                        _ => l != r,
                    },
                    ">" | "Gt" => match (&l, &r) {
                        (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
                            a.as_f64().unwrap_or(0.0) > b.as_f64().unwrap_or(0.0)
                        }
                        _ => false,
                    },
                    "<" | "Lt" => match (&l, &r) {
                        (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
                            a.as_f64().unwrap_or(0.0) < b.as_f64().unwrap_or(0.0)
                        }
                        _ => false,
                    },
                    ">=" | "Gte" => match (&l, &r) {
                        (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
                            a.as_f64().unwrap_or(0.0) >= b.as_f64().unwrap_or(0.0)
                        }
                        _ => false,
                    },
                    "<=" | "Lte" => match (&l, &r) {
                        (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
                            a.as_f64().unwrap_or(0.0) <= b.as_f64().unwrap_or(0.0)
                        }
                        _ => false,
                    },
                    _ => false,
                };
                Ok(serde_json::Value::Bool(res))
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
K Language v3.0 (MNSE Kernel) - Command Reference

Present State Queries:
  current.state (or 'state')       Live developer context (app, window, cwd, editor, diag)
  current.app   (or 'app')         Active window / application name
  current.file                     Current active file in editor
  current.toon                     Compact single-line TOON state representation
  current.json                     Full machine JSON snapshot

Vector Slicing & Pipelines:
  data[pred].field                 Filter elements by predicate and project fields
  data[0] / data[-1]               Direct index (with negative indexing from end)
  data[start..end]                 Range slice for arrays and strings
  expr | where pred | select f1    Pipeline stage filtering and field selection

AI Capability Platform:
  pdf.inspect <path>               Inspect PDF structure, page count, fonts & metadata
  pdf.extract <path> [page]        Extract layout-preserved text from PDF
  pdf.render_page <path> [page]    Render PDF page as 150 DPI PNG image
  pdf.slice <in> <from> <to> <out> Lossless structural page extraction (QPDF)
  pdf.merge <files...> <out>       Merge multiple PDF documents (Poppler)
  doc.convert <in> <fmt> [out]     Convert document (DOCX, PPTX, XLSX, HTML, TXT)
  doc.ocr <image> [lang]           Extract text from image/scanned document (Tesseract)
  doc.detect_type <file>           True MIME type inspection from bytes
  git.status                       Repository status (branch, modified, staged, untracked)
  git.diff                         Working tree diff stats
  git.log [limit]                  Recent commit history

Sandboxed Shell Execution:
  sh { <command> }                 Execute host shell command with $var interpolation

Recent Activity Queries:
  recent.errors   (or 'errors')    Recent compiler and runtime errors
  recent.commands (or 'commands')  Recent shell commands executed
  recent.files    (or 'files')     Recent files switched or edited

Historical Ledger Queries:
  history today                    Ledger events from today
  history yesterday                Ledger events from yesterday
  history YYYY-MM-DD               Ledger events from a specific date
  summary today                    Activity statistics for today

Causal Reasoning:
  cause of error                   Attribution graph search for latest error
  cause of <target>                Graph diffusion search for target entity
  related <query>                  Cosine and graph similarity search

Creator Control & Time-Travel:
  tap [adapter] [--raw] [--json]   Stream live telemetry events from kernel in real-time
  replay --from <time> [--speed]   Chronologically play back historical events from ledger
  state --at <natural-time>        Reconstruct workstation context at past point in time

Adapter & System Management:
  adapter list                     List adapter connection, status & telemetry
  adapter on <name>                Enable adapter (os, hyprland, fs, shell)
  adapter off <name>               Disable adapter
  care                             Kernel daemon health and privacy status
  help                             Display this help reference\n";
                Ok(serde_json::Value::String(help_text.to_string()))
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

            KernelOp::Sh(cmd) => {
                let mut interpolated = cmd.clone();
                for (key, val) in &ctx.bindings {
                    let pattern = format!("${}", key);
                    let val_str = match val {
                        serde_json::Value::String(s) => s.clone(),
                        serde_json::Value::Number(n) => n.to_string(),
                        serde_json::Value::Bool(b) => b.to_string(),
                        _ => val.to_string(),
                    };
                    interpolated = interpolated.replace(&pattern, &val_str);
                }

                let start = std::time::Instant::now();
                let output = tokio::process::Command::new("sh")
                    .arg("-c")
                    .arg(&interpolated)
                    .output()
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to execute shell command: {}", e))?;

                let duration_ms = start.elapsed().as_millis() as u64;
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let exit_code = output.status.code().unwrap_or(-1);
                let ok = output.status.success();

                Ok(serde_json::json!({
                    "stdout": stdout,
                    "stderr": stderr,
                    "exit_code": exit_code,
                    "ok": ok,
                    "duration_ms": duration_ms
                }))
            }

            KernelOp::Call { namespace, operation, args } => {
                let mut evaled_args = Vec::new();
                for a in args {
                    evaled_args.push(Box::pin(self.eval_op(a, ctx)).await?);
                }

                // If invoking git capability and no repo path was passed, check if process cwd is in a repo.
                // If not, inspect active kernel snapshot cwd.
                if namespace == "git" && (evaled_args.is_empty() || (operation == "log" && evaled_args.len() < 2)) {
                    let in_repo = std::process::Command::new("git")
                        .arg("rev-parse")
                        .arg("--is-inside-work-tree")
                        .output()
                        .map(|o| o.status.success())
                        .unwrap_or(false);

                    if !in_repo {
                        if let Ok(snap) = self.get_snapshot().await {
                            if let Some(cwd) = snap.cwd {
                                if std::path::Path::new(&cwd).exists() {
                                    evaled_args.push(serde_json::Value::String(cwd));
                                }
                            }
                        }
                    }
                }

                let evidence = crate::capabilities::dispatch_capability(namespace, operation, &evaled_args)?;

                // Fire audit event to kernel ledger if reachable
                let ev_payload = serde_json::to_value(&evidence).unwrap_or_default();
                let _ = self.http_client.emit(&mnse_types::Event::new(
                    mnse_types::EventType::Custom {
                        name: format!("CapabilityExecuted:{}.{}", namespace, operation),
                        payload: ev_payload,
                    },
                    "k-capability",
                )).await;

                Ok(serde_json::to_value(evidence)?)
            }

            KernelOp::Index { target, index } => {
                let target_val = Box::pin(self.eval_op(target, ctx)).await?;

                match target_val {
                    serde_json::Value::Array(items) => {
                        // First check if index evaluates directly to a number in the outer context
                        let direct_eval = Box::pin(self.eval_op(index, ctx)).await;
                        if let Ok(ref val) = direct_eval {
                            if let Some(idx) = val_to_i64(val) {
                                let real_idx = if idx < 0 {
                                    items.len() as i64 + idx
                                } else {
                                    idx
                                };
                                if real_idx >= 0 && (real_idx as usize) < items.len() {
                                    return Ok(items[real_idx as usize].clone());
                                } else {
                                    return Ok(serde_json::Value::Null);
                                }
                            }
                        }
                        // Treat as a vector predicate filter
                        let mut filtered = Vec::new();
                        for item in items {
                            let mut item_ctx = ctx.clone();
                            item_ctx.bindings.insert("it".to_string(), item.clone());
                            if let serde_json::Value::Object(map) = &item {
                                for (k, v) in map {
                                    item_ctx.bindings.insert(k.clone(), v.clone());
                                }
                            }
                            let res = Box::pin(self.eval_op(index, &mut item_ctx)).await?;
                            let truthy = match res {
                                serde_json::Value::Bool(b) => b,
                                serde_json::Value::Number(n) => n.as_f64().map(|v| v != 0.0).unwrap_or(false),
                                serde_json::Value::String(s) => !s.is_empty(),
                                serde_json::Value::Array(a) => !a.is_empty(),
                                serde_json::Value::Object(o) => !o.is_empty(),
                                serde_json::Value::Null => false,
                            };
                            if truthy {
                                filtered.push(item);
                            }
                        }
                        Ok(serde_json::Value::Array(filtered))
                    }
                    serde_json::Value::Object(map) => {
                        let key_val = Box::pin(self.eval_op(index, ctx)).await?;
                        if let Some(key) = key_val.as_str() {
                            Ok(map.get(key).cloned().unwrap_or(serde_json::Value::Null))
                        } else {
                            Ok(serde_json::Value::Null)
                        }
                    }
                    serde_json::Value::String(s) => {
                        let direct_eval = Box::pin(self.eval_op(index, ctx)).await?;
                        if let Some(n) = val_to_i64(&direct_eval) {
                            let chars: Vec<char> = s.chars().collect();
                            let real_idx = if n < 0 {
                                chars.len() as i64 + n
                            } else {
                                n
                            };
                            if real_idx >= 0 && (real_idx as usize) < chars.len() {
                                Ok(serde_json::Value::String(chars[real_idx as usize].to_string()))
                            } else {
                                Ok(serde_json::Value::Null)
                            }
                        } else {
                            Ok(serde_json::Value::Null)
                        }
                    }
                    _ => Ok(serde_json::Value::Null),
                }
            }

            KernelOp::Slice { target, start, end } => {
                let target_val = Box::pin(self.eval_op(target, ctx)).await?;
                let start_idx = if let Some(s) = start {
                    let v = Box::pin(self.eval_op(s, ctx)).await?;
                    val_to_i64(&v)
                } else {
                    None
                };
                let end_idx = if let Some(e) = end {
                    let v = Box::pin(self.eval_op(e, ctx)).await?;
                    val_to_i64(&v)
                } else {
                    None
                };

                match target_val {
                    serde_json::Value::Array(items) => {
                        let len = items.len() as i64;
                        let mut s = start_idx.unwrap_or(0);
                        if s < 0 { s = (len + s).max(0); }
                        let s = (s as usize).min(items.len());

                        let mut e = end_idx.unwrap_or(len);
                        if e < 0 { e = (len + e).max(0); }
                        let e = (e as usize).min(items.len());

                        if s <= e {
                            Ok(serde_json::Value::Array(items[s..e].to_vec()))
                        } else {
                            Ok(serde_json::Value::Array(Vec::new()))
                        }
                    }
                    serde_json::Value::String(str_val) => {
                        let chars: Vec<char> = str_val.chars().collect();
                        let len = chars.len() as i64;
                        let mut s = start_idx.unwrap_or(0);
                        if s < 0 { s = (len + s).max(0); }
                        let s = (s as usize).min(chars.len());

                        let mut e = end_idx.unwrap_or(len);
                        if e < 0 { e = (len + e).max(0); }
                        let e = (e as usize).min(chars.len());

                        if s <= e {
                            let sub: String = chars[s..e].iter().collect();
                            Ok(serde_json::Value::String(sub))
                        } else {
                            Ok(serde_json::Value::String(String::new()))
                        }
                    }
                    _ => Ok(target_val),
                }
            }

            KernelOp::FieldAccess { target, field } => {
                let target_val = Box::pin(self.eval_op(target, ctx)).await?;
                match target_val {
                    serde_json::Value::Object(map) => {
                        Ok(map.get(field).cloned().unwrap_or(serde_json::Value::Null))
                    }
                    serde_json::Value::Array(items) => {
                        // Vector projection across array elements
                        let mut projected = Vec::new();
                        for item in items {
                            if let serde_json::Value::Object(map) = item {
                                projected.push(map.get(field).cloned().unwrap_or(serde_json::Value::Null));
                            } else {
                                projected.push(serde_json::Value::Null);
                            }
                        }
                        Ok(serde_json::Value::Array(projected))
                    }
                    _ => Ok(serde_json::Value::Null),
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planner::KernelOp;

    #[tokio::test]
    async fn test_runtime_if_else_evaluation() {
        let runtime = Runtime::default_local();
        let mut ctx = KContext::default();

        // if true { "matched_then" } else { "matched_else" }
        let op_then = KernelOp::If {
            cond: Box::new(KernelOp::Literal(serde_json::Value::Bool(true))),
            then_branch: vec![KernelOp::Literal(serde_json::Value::String("matched_then".into()))],
            else_branch: Some(vec![KernelOp::Literal(serde_json::Value::String("matched_else".into()))]),
        };
        let res = runtime.eval_op(&op_then, &mut ctx).await.unwrap();
        assert_eq!(res, serde_json::Value::String("matched_then".into()));

        // if false { "matched_then" } else { "matched_else" }
        let op_else = KernelOp::If {
            cond: Box::new(KernelOp::Literal(serde_json::Value::Bool(false))),
            then_branch: vec![KernelOp::Literal(serde_json::Value::String("matched_then".into()))],
            else_branch: Some(vec![KernelOp::Literal(serde_json::Value::String("matched_else".into()))]),
        };
        let res = runtime.eval_op(&op_else, &mut ctx).await.unwrap();
        assert_eq!(res, serde_json::Value::String("matched_else".into()));
    }

    #[tokio::test]
    async fn test_runtime_try_catch_evaluation() {
        let runtime = Runtime::default_local();
        let mut ctx = KContext::default();

        // try { let $x = 42; $x } catch (err) { "fallback" }
        let op_try = KernelOp::TryCatch {
            try_body: vec![
                KernelOp::Let {
                    name: "x".into(),
                    val: Box::new(KernelOp::Literal(serde_json::json!(42))),
                },
                KernelOp::Var("x".into()),
            ],
            catch_var: "err".into(),
            catch_body: vec![KernelOp::Literal(serde_json::Value::String("fallback".into()))],
        };
        let res = runtime.eval_op(&op_try, &mut ctx).await.unwrap();
        assert_eq!(res, serde_json::json!(42));
    }

    #[tokio::test]
    async fn test_runtime_sh_execution() {
        let runtime = Runtime::default_local();
        let mut ctx = KContext::default();
        ctx.bindings.insert("greeting".into(), serde_json::json!("hello mnse"));

        let op = KernelOp::Sh("echo $greeting".into());
        let res = runtime.eval_op(&op, &mut ctx).await.unwrap();
        assert_eq!(res.get("ok").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(res.get("stdout").and_then(|v| v.as_str()), Some("hello mnse"));
        assert_eq!(res.get("exit_code").and_then(|v| v.as_i64()), Some(0));
    }

    #[tokio::test]
    async fn test_runtime_vector_indexing_and_projection() {
        let runtime = Runtime::default_local();
        let mut ctx = KContext::default();

        let errors = serde_json::json!([
            { "command": "cargo build", "exit_code": 1 },
            { "command": "ls", "exit_code": 0 },
            { "command": "pytest", "exit_code": 2 }
        ]);
        ctx.bindings.insert("errors".into(), errors);

        // errors[exit_code != 0]
        let filter_op = KernelOp::Index {
            target: Box::new(KernelOp::Var("errors".into())),
            index: Box::new(KernelOp::Binary {
                op: "!=".into(),
                left: Box::new(KernelOp::Var("exit_code".into())),
                right: Box::new(KernelOp::Literal(serde_json::json!(0))),
            }),
        };

        // .command
        let proj_op = KernelOp::FieldAccess {
            target: Box::new(filter_op),
            field: "command".into(),
        };

        let res = runtime.eval_op(&proj_op, &mut ctx).await.unwrap();
        assert_eq!(
            res,
            serde_json::json!(["cargo build", "pytest"])
        );
    }

    #[tokio::test]
    async fn test_runtime_slice_and_negative_index() {
        let runtime = Runtime::default_local();
        let mut ctx = KContext::default();
        ctx.bindings.insert("items".into(), serde_json::json!(["a", "b", "c", "d"]));

        // items[-1]
        let neg_idx = KernelOp::Index {
            target: Box::new(KernelOp::Var("items".into())),
            index: Box::new(KernelOp::Literal(serde_json::json!(-1))),
        };
        let last = runtime.eval_op(&neg_idx, &mut ctx).await.unwrap();
        assert_eq!(last, serde_json::json!("d"));

        // items[1..3]
        let slice_op = KernelOp::Slice {
            target: Box::new(KernelOp::Var("items".into())),
            start: Some(Box::new(KernelOp::Literal(serde_json::json!(1)))),
            end: Some(Box::new(KernelOp::Literal(serde_json::json!(3)))),
        };
        let sliced = runtime.eval_op(&slice_op, &mut ctx).await.unwrap();
        assert_eq!(sliced, serde_json::json!(["b", "c"]));
    }

    #[tokio::test]
    async fn test_end_to_end_execute() {
        let res = crate::execute("let $x = 10; let $y = 20; if true { $y } else { $x }").await.unwrap();
        assert_eq!(res, serde_json::json!(20));

        let sh_res = crate::execute("sh { echo \"k3_works\" }").await.unwrap();
        assert_eq!(sh_res.get("stdout").and_then(|v| v.as_str()), Some("k3_works"));
    }

    #[tokio::test]
    async fn test_pipeline_where_filtering() {
        let runtime = Runtime::default_local();
        let mut ctx = KContext::default();
        let cmds = serde_json::json!([
            { "command": "git status", "exit_code": 0 },
            { "command": "cargo build", "exit_code": 1 },
            { "command": "ls -la", "exit_code": 0 }
        ]);
        ctx.bindings.insert("it".into(), cmds);

        // where exit_code == 0
        let where_op = KernelOp::Where(Box::new(KernelOp::Binary {
            op: "==".into(),
            left: Box::new(KernelOp::Var("exit_code".into())),
            right: Box::new(KernelOp::Literal(serde_json::json!(0))),
        }));

        let filtered = runtime.eval_op(&where_op, &mut ctx).await.unwrap();
        assert_eq!(
            filtered,
            serde_json::json!([
                { "command": "git status", "exit_code": 0 },
                { "command": "ls -la", "exit_code": 0 }
            ])
        );
    }

    #[tokio::test]
    async fn test_end_to_end_indexing_and_slicing() {
        let mut ctx = KContext::default();
        ctx.bindings.insert("items".into(), serde_json::json!(["apple", "banana", "cherry", "date"]));

        // Negative indexing
        let neg_res = crate::execute_with_ctx("items[-1]", &mut ctx).await.unwrap();
        assert_eq!(neg_res, serde_json::json!("date"));

        // Range slice
        let slice_res = crate::execute_with_ctx("items[0..2]", &mut ctx).await.unwrap();
        assert_eq!(slice_res, serde_json::json!(["apple", "banana"]));
    }
}
