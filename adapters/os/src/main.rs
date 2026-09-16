//! MNSE OS Adapter for Hyprland desktop environment.

use mnse_sdk::SocketClient;
use mnse_types::{Event, EventType};
use serde_json::Value;
use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("🪟 Starting MNSE OS Adapter (Hyprland)...");

    let client = SocketClient::default_local();
    let mut last_window_id = String::new();

    loop {
        if let Ok(output) = Command::new("hyprctl").arg("activewindow").arg("-j").output() {
            if output.status.success() {
                if let Ok(val) = serde_json::from_slice::<Value>(&output.stdout) {
                    let app_name = val.get("class").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
                    let window_title = val.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let window_id = val.get("address").and_then(|v| v.as_str()).unwrap_or("0x0").to_string();
                    let workspace = val.get("workspace").and_then(|w| w.get("id")).and_then(|i| i.as_u64()).map(|u| u as u32);

                    if window_id != last_window_id && !app_name.is_empty() {
                        last_window_id = window_id.clone();
                        let event = Event::new(
                            EventType::WindowFocused {
                                app_name: app_name.clone(),
                                window_title: window_title.clone(),
                                window_class: app_name,
                                window_id,
                                pid: None,
                                workspace,
                            },
                            "hyprland",
                        );

                        if let Err(e) = client.emit(&event).await {
                            tracing::debug!("Failed to emit WindowFocused event to socket: {}", e);
                        } else {
                            tracing::info!("Emitted WindowFocused: {}", window_title);
                        }
                    }
                }
            }
        }

        sleep(Duration::from_millis(500)).await;
    }
}
