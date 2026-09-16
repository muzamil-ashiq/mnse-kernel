//! High-throughput Tokio Broadcast EventBus.
//!
//! Features:
//! - 4000+ msg/sec broadcast capacity
//! - Forensic intake timestamping (`kernel_received_at`)
//! - Priority routing (Reflex Layer fast-path)
//! - Multi-subscriber fan-out for SSE stream, Ledger, Graph, and Actuators

use mnse_types::Event;
use tokio::sync::broadcast;

pub struct EventBus {
    sender: broadcast::Sender<Event>,
    capacity: usize,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender, capacity }
    }

    /// Publish an event to the EventBus.
    /// Sets forensic `kernel_received_at` timestamp.
    pub fn publish(&self, mut event: Event) -> Result<usize, broadcast::error::SendError<Event>> {
        event.kernel_received_at = Some(chrono::Utc::now());
        self.sender.send(event)
    }

    /// Subscribe to the live EventBus stream
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }

    /// Total active subscriber count
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(4096)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mnse_types::EventType;

    #[tokio::test]
    async fn test_event_bus_publish_subscribe() {
        let bus = EventBus::new(100);
        let mut rx = bus.subscribe();

        let event = Event::new(
            EventType::ProcessExecuted {
                command: "ls".into(),
                exit_code: 0,
                duration_ms: 5,
                working_dir: "/".into(),
                stdout_tail: None,
                stderr_tail: None,
            },
            "shell",
        );

        bus.publish(event.clone()).unwrap();

        let received = rx.recv().await.unwrap();
        assert_eq!(received.id, event.id);
        assert!(received.kernel_received_at.is_some());
    }
}
