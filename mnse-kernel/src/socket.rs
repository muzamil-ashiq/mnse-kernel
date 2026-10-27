//! Unix Domain Socket Server (`~/.mnse/kernel.sock`).
//!
//! Enforces ADR 001 & security policy:
//! - Unix socket file permissions set to 0600 (user-only read/write)
//! - SO_PEERCRED UID verification on Linux
//! - High-speed event ingestion & control commands

use crate::http_server::AppState;
use mnse_types::Event;
use serde::{Deserialize, Serialize};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "lowercase")]
pub enum SocketCommand {
    Ping,
    Snapshot { format: Option<String> },
    Emit { event: Event },
    Adapter { action: String, name: Option<String> },
    Tap { adapter: Option<String>, raw: Option<bool> },
}

#[derive(Debug, Serialize)]
pub struct SocketResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

pub async fn start_socket_server(socket_path: &str, state: AppState) -> anyhow::Result<()> {
    let path = Path::new(socket_path);
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    tracing::info!("✅ Unix Socket server listening at {}", socket_path);

    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                #[cfg(target_os = "linux")]
                if let Ok(cred) = stream.peer_cred() {
                    let expected_uid = std::fs::metadata(path).map(|m| m.uid()).unwrap_or(cred.uid());
                    if cred.uid() != expected_uid {
                        tracing::warn!("Rejected unauthorized socket connection from UID {}", cred.uid());
                        continue;
                    }
                }

                let state_clone = state.clone();
                tokio::spawn(async move {
                    let (reader, mut writer) = stream.into_split();
                    let mut lines = BufReader::new(reader).lines();

                    while let Ok(Some(line)) = lines.next_line().await {
                        if line.trim().is_empty() {
                            continue;
                        }

                        // Handle Creator Tap streaming command
                        if let Ok(SocketCommand::Tap { adapter, raw }) = serde_json::from_str::<SocketCommand>(&line) {
                            let resp = SocketResponse {
                                status: "streaming".to_string(),
                                data: None,
                            };
                            let json_resp = serde_json::to_string(&resp).unwrap_or_default();
                            if writer.write_all(format!("{}\n", json_resp).as_bytes()).await.is_err() {
                                break;
                            }

                            let mut rx = state_clone.bus.subscribe();
                            let is_raw = raw.unwrap_or(false);
                            while let Ok(mut event) = rx.recv().await {
                                if let Some(ref target) = adapter {
                                    let matches = event.observed_by.eq_ignore_ascii_case(target)
                                        || format!("{:?}", event.source).eq_ignore_ascii_case(target);
                                    if !matches {
                                        continue;
                                    }
                                }
                                if !is_raw {
                                    state_clone.redactor.redact_event(&mut event);
                                }
                                let ev_json = serde_json::to_string(&event).unwrap_or_default();
                                if writer.write_all(format!("{}\n", ev_json).as_bytes()).await.is_err() {
                                    break;
                                }
                            }
                            break;
                        }

                        let resp = handle_socket_line(&line, &state_clone).await;
                        let json_resp = serde_json::to_string(&resp).unwrap_or_default();
                        let _ = writer.write_all(format!("{}\n", json_resp).as_bytes()).await;
                    }
                });
            }
            Err(e) => {
                tracing::warn!("Socket connection accept error: {}", e);
            }
        }
    }
}

async fn handle_socket_line(line: &str, state: &AppState) -> SocketResponse {
    // Try parsing as control command first
    if let Ok(cmd) = serde_json::from_str::<SocketCommand>(line) {
        match cmd {
            SocketCommand::Ping => SocketResponse {
                status: "pong".to_string(),
                data: None,
            },
            SocketCommand::Snapshot { format } => {
                let mut snap = state.inspector.snapshot();
                state.redactor.redact_snapshot(&mut snap);
                let format_str = format.as_deref().unwrap_or("toon");
                if format_str == "json" {
                    SocketResponse {
                        status: "ok".to_string(),
                        data: serde_json::to_value(&snap).ok(),
                    }
                } else {
                    SocketResponse {
                        status: "ok".to_string(),
                        data: Some(serde_json::Value::String(snap.to_toon())),
                    }
                }
            }
            SocketCommand::Emit { mut event } => {
                match state.gatekeeper.admit(&mut event) {
                    crate::gatekeeper::Admission::Admitted => {
                        state.inspector.process_event(&event);
                        let _ = state.ledger.store_event(&event);
                        let _ = state.bus.publish(event);
                        SocketResponse {
                            status: "accepted".to_string(),
                            data: None,
                        }
                    }
                    crate::gatekeeper::Admission::Dropped(reason) => SocketResponse {
                        status: "dropped".to_string(),
                        data: Some(serde_json::Value::String(reason)),
                    },
                }
            }
            SocketCommand::Adapter { action, name } => match action.as_str() {
                "list" => {
                    let registered = state.registry.list();
                    let snap = state.inspector.snapshot();
                    let health_map: std::collections::HashMap<String, mnse_types::AdapterStatus> = snap
                        .adapters
                        .into_iter()
                        .map(|a| (a.name.clone(), a))
                        .collect();

                    let list: Vec<serde_json::Value> = registered
                        .into_iter()
                        .map(|entry| {
                            let live = health_map.get(&entry.name);
                            let connected = live.map(|l| l.connected).unwrap_or(false);
                            let event_count = live.map(|l| l.event_count).unwrap_or(0);
                            let last_seen = live.and_then(|l| l.last_seen);
                            serde_json::json!({
                                "name": entry.name,
                                "enabled": entry.enabled,
                                "connected": connected,
                                "event_count": event_count,
                                "last_seen": last_seen,
                                "description": entry.description,
                            })
                        })
                        .collect();
                    SocketResponse {
                        status: "ok".to_string(),
                        data: Some(serde_json::Value::Array(list)),
                    }
                }
                "enable" | "on" => {
                    if let Some(n) = name {
                        let _ = state.registry.set_enabled(&n, true);
                        SocketResponse {
                            status: "ok".to_string(),
                            data: Some(serde_json::json!({ "adapter": n, "enabled": true })),
                        }
                    } else {
                        SocketResponse {
                            status: "error".to_string(),
                            data: Some(serde_json::Value::String("Missing adapter name".into())),
                        }
                    }
                }
                "disable" | "off" => {
                    if let Some(n) = name {
                        let _ = state.registry.set_enabled(&n, false);
                        SocketResponse {
                            status: "ok".to_string(),
                            data: Some(serde_json::json!({ "adapter": n, "enabled": false })),
                        }
                    } else {
                        SocketResponse {
                            status: "error".to_string(),
                            data: Some(serde_json::Value::String("Missing adapter name".into())),
                        }
                    }
                }
                _ => SocketResponse {
                    status: "error".to_string(),
                    data: Some(serde_json::Value::String(format!("Unknown action: {}", action))),
                },
            },
            SocketCommand::Tap { .. } => SocketResponse {
                status: "ok".to_string(),
                data: Some(serde_json::Value::String("Tap stream established".into())),
            },
        }
    } else if let Ok(mut event) = serde_json::from_str::<Event>(line) {
        // Direct event ingestion fallback via Gatekeeper
        match state.gatekeeper.admit(&mut event) {
            crate::gatekeeper::Admission::Admitted => {
                state.inspector.process_event(&event);
                let _ = state.ledger.store_event(&event);
                let _ = state.bus.publish(event);
                SocketResponse {
                    status: "accepted".to_string(),
                    data: None,
                }
            }
            crate::gatekeeper::Admission::Dropped(reason) => SocketResponse {
                status: "dropped".to_string(),
                data: Some(serde_json::Value::String(reason)),
            },
        }
    } else {
        SocketResponse {
            status: "error".to_string(),
            data: Some(serde_json::Value::String("Invalid JSON payload".to_string())),
        }
    }
}
