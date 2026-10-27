//! Client implementation for HTTP & Unix domain socket kernel communication.

use anyhow::Result;
use mnse_types::{ContextSnapshot, DailyStats, Event};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

#[derive(Debug, Serialize)]
struct SocketCommandPayload<'a> {
    cmd: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    event: Option<&'a Event>,
}

#[derive(Debug, Deserialize)]
struct SocketResponsePayload {
    status: String,
    data: Option<serde_json::Value>,
}

/// HTTP Client for interacting with MNSE Kernel REST API
pub struct HttpClient {
    base_url: String,
    client: reqwest::Client,
}

impl HttpClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client: reqwest::Client::new(),
        }
    }

    pub fn default_local() -> Self {
        Self::new("http://127.0.0.1:7780")
    }

    /// Fetch context snapshot (with optional point-in-time "at" natural time query)
    pub async fn get_snapshot(&self, format: &str) -> Result<ContextSnapshot> {
        self.get_snapshot_at(format, None).await
    }

    /// Fetch context snapshot at a specific point in time
    pub async fn get_snapshot_at(&self, format: &str, at: Option<&str>) -> Result<ContextSnapshot> {
        let mut req = self.client.get(format!("{}/snapshot", self.base_url));
        req = req.query(&[("format", format)]);
        if let Some(t) = at {
            req = req.query(&[("at", t)]);
        }
        let resp = req.send().await?.json::<ContextSnapshot>().await?;
        Ok(resp)
    }

    /// Fetch formatted TOON string
    pub async fn get_snapshot_toon(&self) -> Result<String> {
        self.get_snapshot_toon_at("rich", None).await
    }

    /// Fetch formatted TOON string at a specific point in time
    pub async fn get_snapshot_toon_at(&self, format: &str, at: Option<&str>) -> Result<String> {
        let mut req = self.client.get(format!("{}/snapshot", self.base_url));
        req = req.query(&[("format", format)]);
        if let Some(t) = at {
            req = req.query(&[("at", t)]);
        }
        let resp = req.send().await?.text().await?;
        Ok(resp)
    }

    /// Emit an event to the kernel
    pub async fn emit(&self, event: &Event) -> Result<()> {
        let url = format!("{}/emit", self.base_url);
        self.client.post(&url).json(event).send().await?;
        Ok(())
    }

    /// Query all registered adapters with live telemetry
    pub async fn get_adapters(&self) -> Result<Vec<serde_json::Value>> {
        let url = format!("{}/adapters", self.base_url);
        let resp = self.client.get(&url).send().await?.json::<Vec<serde_json::Value>>().await?;
        Ok(resp)
    }

    /// Enable an adapter (turn ON)
    pub async fn enable_adapter(&self, name: &str) -> Result<()> {
        let url = format!("{}/adapters/{}/enable", self.base_url, name);
        self.client.post(&url).send().await?;
        Ok(())
    }

    /// Disable an adapter (turn OFF)
    pub async fn disable_adapter(&self, name: &str) -> Result<()> {
        let url = format!("{}/adapters/{}/disable", self.base_url, name);
        self.client.post(&url).send().await?;
        Ok(())
    }

    /// Query ledger event history
    pub async fn get_ledger_events(
        &self,
        date: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<Event>> {
        let mut url = format!("{}/ledger/events?", self.base_url);
        if let Some(d) = date {
            url.push_str(&format!("date={}&", d));
        }
        if let Some(l) = limit {
            url.push_str(&format!("limit={}&", l));
        }
        let resp = self.client.get(&url).send().await?.json::<Vec<Event>>().await?;
        Ok(resp)
    }

    /// Query ledger daily activity statistics
    pub async fn get_daily_stats(&self, date: &str) -> Result<DailyStats> {
        let url = format!("{}/ledger/stats?date={}", self.base_url, date);
        let resp = self.client.get(&url).send().await?.json::<DailyStats>().await?;
        Ok(resp)
    }
}

/// Unix Socket Client for low-latency IPC with MNSE Kernel (`~/.mnse/kernel.sock`)
pub struct SocketClient {
    socket_path: PathBuf,
}

impl SocketClient {
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }

    pub fn default_local() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        Self::new(home.join(".mnse/kernel.sock"))
    }

    /// Send ping command over socket
    pub async fn ping(&self) -> Result<bool> {
        let payload = SocketCommandPayload {
            cmd: "ping",
            format: None,
            event: None,
        };
        let resp = self.send_command(&payload).await?;
        Ok(resp.status == "pong")
    }

    /// Emit an event over Unix socket
    pub async fn emit(&self, event: &Event) -> Result<()> {
        let payload = SocketCommandPayload {
            cmd: "emit",
            format: None,
            event: Some(event),
        };
        let resp = self.send_command(&payload).await?;
        if resp.status == "accepted" || resp.status == "ok" {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Socket emit rejected: {}", resp.status))
        }
    }

    /// Fetch TOON snapshot over Unix socket
    pub async fn get_snapshot_toon(&self) -> Result<String> {
        let payload = SocketCommandPayload {
            cmd: "snapshot",
            format: Some("toon"),
            event: None,
        };
        let resp = self.send_command(&payload).await?;
        if let Some(serde_json::Value::String(s)) = resp.data {
            Ok(s)
        } else {
            Err(anyhow::anyhow!("Invalid snapshot response over socket"))
        }
    }

    async fn send_command(&self, payload: &SocketCommandPayload<'_>) -> Result<SocketResponsePayload> {
        let mut stream = UnixStream::connect(&self.socket_path).await?;
        let json_line = serde_json::to_string(payload)?;
        stream.write_all(format!("{}\n", json_line).as_bytes()).await?;

        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).await?;

        let resp = serde_json::from_str::<SocketResponsePayload>(&line)?;
        Ok(resp)
    }

    /// Open a live tap stream to receive real-time events over Unix socket
    pub async fn tap_stream(
        &self,
        adapter: Option<&str>,
        raw: bool,
    ) -> Result<tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>> {
        let stream = UnixStream::connect(&self.socket_path).await?;
        let (reader, mut writer) = stream.into_split();
        let cmd = serde_json::json!({
            "cmd": "tap",
            "adapter": adapter,
            "raw": raw,
        });
        writer.write_all(format!("{}\n", serde_json::to_string(&cmd)?).as_bytes()).await?;

        let mut lines = BufReader::new(reader).lines();
        // Consume first acknowledgement line: {"status":"streaming"}
        if let Ok(Some(_ack)) = lines.next_line().await {
            // ready
        }
        Ok(lines)
    }
}

/// Ultra-fast nanosecond-latency Shared Memory Client via SlotBus
pub struct SlotBusClient {
    producer: std::sync::Mutex<Option<mnse_slotbus::SlotBusProducer>>,
    shm_name: String,
}

impl SlotBusClient {
    pub fn default_local() -> Self {
        Self::new(mnse_slotbus::DEFAULT_SHM_NAME)
    }

    pub fn new(shm_name: &str) -> Self {
        let producer = mnse_slotbus::SlotBusProducer::open(shm_name).ok();
        Self {
            producer: std::sync::Mutex::new(producer),
            shm_name: shm_name.to_string(),
        }
    }

    /// Emits an event to kernel via shared memory ring buffer in <500 nanoseconds
    pub fn emit(&self, event: &Event) -> Result<()> {
        let mut guard = self.producer.lock().unwrap();
        if guard.is_none() {
            *guard = mnse_slotbus::SlotBusProducer::open(&self.shm_name).ok();
        }

        if let Some(ref p) = *guard {
            p.send_event(event).map_err(|e| anyhow::anyhow!("{}", e))
        } else {
            Err(anyhow::anyhow!("SlotBus producer offline (shm: {})", self.shm_name))
        }
    }
}

/// Hybrid high-performance client: writes directly to lockless shared memory (`SlotBusClient`),
/// falling back seamlessly to UNIX domain socket (`SocketClient`) if the shared memory segment is offline.
pub struct AdaptiveClient {
    pub shm: SlotBusClient,
    pub socket: SocketClient,
}

impl AdaptiveClient {
    pub fn default_local() -> Self {
        Self {
            shm: SlotBusClient::default_local(),
            socket: SocketClient::default_local(),
        }
    }

    pub fn new(shm_name: &str, socket_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            shm: SlotBusClient::new(shm_name),
            socket: SocketClient::new(socket_path),
        }
    }

    /// Emits an event with <500ns shared memory priority, falling back to UNIX socket
    pub async fn emit(&self, event: &Event) -> Result<()> {
        if self.shm.emit(event).is_ok() {
            return Ok(());
        }
        self.socket.emit(event).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_defaults() {
        let http = HttpClient::default_local();
        assert_eq!(http.base_url, "http://127.0.0.1:7780");

        let socket = SocketClient::default_local();
        assert!(socket.socket_path.to_string_lossy().contains("kernel.sock"));

        let adaptive = AdaptiveClient::default_local();
        assert!(adaptive.socket.socket_path.to_string_lossy().contains("kernel.sock"));
    }
}
