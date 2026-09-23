use anyhow::Result;
use mnse_types::Event;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

pub struct EventBuffer {
    buffer_path: PathBuf,
}

impl EventBuffer {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        let mnse_dir = home.join(".mnse");
        let _ = std::fs::create_dir_all(&mnse_dir);
        let buffer_path = mnse_dir.join("event_buffer.jsonl");

        Self { buffer_path }
    }

    /// Append event to offline JSONL buffer
    pub fn push(&self, event: &Event) -> Result<()> {
        let json_line = serde_json::to_string(event)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.buffer_path)?;
        writeln!(file, "{}", json_line)?;
        Ok(())
    }

    /// Flush up to max_events from buffer and send via client
    pub async fn flush(&self, client: &mnse_sdk::SocketClient, max_events: usize) -> Result<usize> {
        if !self.buffer_path.exists() {
            return Ok(0);
        }

        let file = File::open(&self.buffer_path)?;
        let reader = BufReader::new(file);
        let mut remaining = Vec::new();
        let mut flushed = 0;

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            if flushed < max_events {
                if let Ok(event) = serde_json::from_str::<Event>(&line) {
                    if client.emit(&event).await.is_ok() {
                        flushed += 1;
                        continue;
                    }
                }
            }

            remaining.push(line);
        }

        if flushed > 0 {
            if remaining.is_empty() {
                let _ = std::fs::remove_file(&self.buffer_path);
            } else {
                let mut file = File::create(&self.buffer_path)?;
                for line in remaining {
                    writeln!(file, "{}", line)?;
                }
            }
        }

        Ok(flushed)
    }
}
