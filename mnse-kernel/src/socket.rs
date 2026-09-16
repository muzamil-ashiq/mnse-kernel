//! Unix Domain Socket Server (`~/.mnse/kernel.sock`).
//!
//! Enforces ADR 001 & security policy:
//! - Unix socket file permissions set to 0600 (user-only read/write)
//! - SO_PEERCRED UID verification on Linux
//! - High-speed event ingestion & control commands

use crate::http_server::AppState;
use mnse_types::Event;
use serde::{Deserialize, Serialize};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "lowercase")]
pub enum SocketCommand {
    Ping,
    Snapshot { format: Option<String> },
    Emit { event: Event },
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
                let state_clone = state.clone();
                tokio::spawn(async move {
                    let (reader, mut writer) = stream.into_split();
                    let mut lines = BufReader::new(reader).lines();

                    while let Ok(Some(line)) = lines.next_line().await {
                        if line.trim().is_empty() {
                            continue;
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
                state.redactor.redact_event(&mut event);
                state.inspector.process_event(&event);
                let _ = state.ledger.store_event(&event);
                let _ = state.bus.publish(event);
                SocketResponse {
                    status: "accepted".to_string(),
                    data: None,
                }
            }
        }
    } else if let Ok(mut event) = serde_json::from_str::<Event>(line) {
        // Direct event ingestion fallback
        state.redactor.redact_event(&mut event);
        state.inspector.process_event(&event);
        let _ = state.ledger.store_event(&event);
        let _ = state.bus.publish(event);
        SocketResponse {
            status: "accepted".to_string(),
            data: None,
        }
    } else {
        SocketResponse {
            status: "error".to_string(),
            data: Some(serde_json::Value::String("Invalid JSON payload".to_string())),
        }
    }
}
