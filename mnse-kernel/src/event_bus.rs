//! High-throughput Tokio Broadcast EventBus with Dual-Lane Priority Routing.
//!
//! Features:
//! - Dual-lane routing: Standard Telemetry Lane + Dedicated Reflex Fast-Path Lane
//! - Fast-path dispatch (<1ms) for high-priority signals (`ErrorDetected`)
//! - 4000+ msg/sec broadcast capacity without cross-lane head-of-line blocking
//! - Forensic intake timestamping (`kernel_received_at`)
//! - Multi-subscriber fan-out for SSE stream, Ledger, Graph, and Actuators

use mnse_types::Event;
use tokio::sync::broadcast;

pub struct EventBus {
    /// Telemetry lane: carries all events for standard subscribers (SSE, ledger, graph)
    telemetry_sender: broadcast::Sender<Event>,
    /// Reflex fast-path lane: carries ONLY high-priority/reflex events (e.g. ErrorDetected)
    reflex_sender: broadcast::Sender<Event>,
    capacity: usize,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (telemetry_sender, _) = broadcast::channel(capacity);
        let reflex_capacity = (capacity / 4).max(512);
        let (reflex_sender, _) = broadcast::channel(reflex_capacity);
        Self {
            telemetry_sender,
            reflex_sender,
            capacity,
        }
    }

    /// Publish an event to the EventBus.
    /// Sets forensic `kernel_received_at` timestamp.
    /// High-priority events are immediately dispatched to the Reflex Lane before the Telemetry Lane.
    pub fn publish(&self, mut event: Event) -> Result<usize, broadcast::error::SendError<Event>> {
        event.kernel_received_at = Some(chrono::Utc::now());

        // Fast-path reflex dispatch for high-priority signals
        if event.event_type.is_high_priority() {
            let _ = self.reflex_sender.send(event.clone());
        }

        // Full broadcast to telemetry stream (SSE, ledger, graph)
        self.telemetry_sender.send(event)
    }

    /// Subscribe to the standard EventBus telemetry stream (receives all events)
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.telemetry_sender.subscribe()
    }

    /// Subscribe strictly to the Reflex Fast-Path Lane (receives only high-priority events)
    pub fn subscribe_reflex(&self) -> broadcast::Receiver<Event> {
        self.reflex_sender.subscribe()
    }

    /// Total active telemetry subscriber count
    pub fn subscriber_count(&self) -> usize {
        self.telemetry_sender.receiver_count()
    }

    /// Total active reflex subscriber count
    pub fn reflex_subscriber_count(&self) -> usize {
        self.reflex_sender.receiver_count()
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

    #[tokio::test]
    async fn test_dual_lane_priority_routing() {
        let bus = EventBus::new(100);
        let mut telemetry_rx = bus.subscribe();
        let mut reflex_rx = bus.subscribe_reflex();

        // 1. Standard low/medium priority event
        let standard_event = Event::new(
            EventType::ProcessExecuted {
                command: "cargo build".into(),
                exit_code: 0,
                duration_ms: 100,
                working_dir: "/tmp".into(),
                stdout_tail: None,
                stderr_tail: None,
            },
            "shell",
        );

        // 2. High-priority reflex event
        let error_event = Event::new(
            EventType::ErrorDetected {
                command: "cargo build".into(),
                exit_code: 101,
                error_pattern: Some("RustCompile".into()),
                working_dir: "/tmp".into(),
            },
            "shell",
        );

        bus.publish(standard_event.clone()).unwrap();
        bus.publish(error_event.clone()).unwrap();

        // Telemetry lane receives BOTH events in order
        let t1 = telemetry_rx.recv().await.unwrap();
        assert_eq!(t1.id, standard_event.id);

        let t2 = telemetry_rx.recv().await.unwrap();
        assert_eq!(t2.id, error_event.id);

        // Reflex lane receives ONLY the high-priority error event
        let r1 = reflex_rx.recv().await.unwrap();
        assert_eq!(r1.id, error_event.id);

        // Ensure no other events were queued on reflex
        assert!(reflex_rx.try_recv().is_err());
    }
}
