//! Gatekeeper — Central admission controller, rate limiter, deduplicator, and PII redactor.
//!
//! Intercepts all incoming signals before they touch the EventBus, Ledger, or InspectorState.

use crate::adapter_registry::AdapterRegistry;
use crate::ml::{AdaptiveKalmanFilter, CellularErrorClassifier, ErrorClassifier, RateEstimator};
use crate::redaction::Redactor;
use chrono::Utc;
use mnse_types::{Event, EventType};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, PartialEq, Eq)]
pub enum Admission {
    Admitted,
    Dropped(String),
}

pub struct Gatekeeper {
    registry: Arc<AdapterRegistry>,
    redactor: Arc<Redactor>,
    error_classifier: Arc<dyn ErrorClassifier>,
    rate_estimator: Arc<dyn RateEstimator>,
    // Per-adapter rate limiter: observed_by -> (current_second_epoch, count)
    rate_counters: Mutex<HashMap<String, (i64, u32)>>,
    // Anti-jitter deduplicator: observed_by + event_type_name -> (last_timestamp_ms, hash)
    dedup_cache: Mutex<HashMap<String, (i64, String)>>,
    max_events_per_sec: u32,
}

impl Gatekeeper {
    pub fn new(registry: Arc<AdapterRegistry>, redactor: Arc<Redactor>, max_events_per_sec: u32) -> Self {
        Self {
            registry,
            redactor,
            error_classifier: Arc::new(CellularErrorClassifier::new()),
            rate_estimator: Arc::new(AdaptiveKalmanFilter::default()),
            rate_counters: Mutex::new(HashMap::new()),
            dedup_cache: Mutex::new(HashMap::new()),
            max_events_per_sec,
        }
    }

    pub fn with_classifier(mut self, classifier: Arc<dyn ErrorClassifier>) -> Self {
        self.error_classifier = classifier;
        self
    }

    pub fn with_rate_estimator(mut self, rate_estimator: Arc<dyn RateEstimator>) -> Self {
        self.rate_estimator = rate_estimator;
        self
    }

    /// Process and evaluate incoming event for admission.
    /// If admitted, redacts PII in-place and sets forensic intake timestamp.
    pub fn admit(&self, event: &mut Event) -> Admission {
        // 1. Sanity check: observed_by must be present
        if event.observed_by.trim().is_empty() {
            return Admission::Dropped("missing observed_by adapter attribute".to_string());
        }

        // 2. Admission Check: is adapter enabled in registry?
        if !self.registry.is_enabled(&event.observed_by) {
            return Admission::Dropped(format!("adapter '{}' is currently disabled", event.observed_by));
        }

        // 3. Timestamp Sanity: Reject timestamps skewed way into the future (> 1 hour)
        let now_epoch = Utc::now().timestamp();
        let ev_epoch = event.timestamp.timestamp();
        if ev_epoch > now_epoch + 3600 {
            return Admission::Dropped("event timestamp is too far in future".to_string());
        }

        // 4. Rate Limiting Protection (per-adapter window & Kalman adaptive tracking)
        let now_sec = Utc::now().timestamp();
        let (current_count, is_anomaly, z_score) = if let Ok(mut counters) = self.rate_counters.lock() {
            let entry = counters.entry(event.observed_by.clone()).or_insert((now_sec, 0));
            if entry.0 == now_sec {
                entry.1 += 1;
            } else {
                *entry = (now_sec, 1);
            }
            let count = entry.1;
            let metrics = self.rate_estimator.observe(&event.observed_by, now_sec, count);
            (count, metrics.is_burst_anomaly, metrics.z_score)
        } else {
            (1, false, 0.0)
        };

        if current_count > self.max_events_per_sec {
            return Admission::Dropped(format!(
                "rate limit exceeded (> {} ev/s) for adapter '{}' (Kalman Z={:.1})",
                self.max_events_per_sec, event.observed_by, z_score
            ));
        }

        if is_anomaly && current_count > self.max_events_per_sec / 2 {
            tracing::warn!(
                "Adaptive rate limiter detected surge anomaly on adapter '{}' (count: {}, Z={:.1})",
                event.observed_by, current_count, z_score
            );
        }

        // 5. Anti-Jitter Deduplication (drop exact duplicate events within 50ms)
        let now_ms = Utc::now().timestamp_millis();
        let dedup_key = format!("{}:{}", event.observed_by, event.event_type.type_name());
        let event_summary = format!("{:?}", event.event_type);

        if let Ok(mut cache) = self.dedup_cache.lock() {
            if let Some((last_ms, last_summary)) = cache.get(&dedup_key) {
                if now_ms - *last_ms < 50 && last_summary == &event_summary {
                    return Admission::Dropped("anti-jitter: duplicate event dropped".to_string());
                }
            }
            cache.insert(dedup_key, (now_ms, event_summary));
        }

        // 6. Zero Trust PII Redaction
        self.redactor.redact_event(event);

        // 6.5. Semantic Micro-ML Enrichment (Cellular Error Classifier)
        if let EventType::ErrorDetected {
            ref command,
            exit_code,
            ref mut error_pattern,
            ..
        } = event.event_type
        {
            let (cat, conf) = self.error_classifier.classify(command, exit_code, None);
            if error_pattern.is_none() {
                *error_pattern = Some(cat.as_str().to_string());
            }
            let mut payload_map = match event.payload.take() {
                Some(serde_json::Value::Object(map)) => map,
                _ => serde_json::Map::new(),
            };
            payload_map.insert("classifier_category".to_string(), serde_json::json!(cat.as_str()));
            payload_map.insert("classifier_confidence".to_string(), serde_json::json!(conf));
            event.payload = Some(serde_json::Value::Object(payload_map));
        }

        // 7. Forensic intake timestamp recorded by kernel
        event.kernel_received_at = Some(Utc::now());

        Admission::Admitted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mnse_types::EventType;
    use tempfile::NamedTempFile;

    #[test]
    fn test_gatekeeper_admission_and_disable() {
        let temp = NamedTempFile::new().unwrap();
        let registry = Arc::new(AdapterRegistry::load_from(temp.path()));
        let redactor = Arc::new(Redactor::new(false));
        let gatekeeper = Gatekeeper::new(registry.clone(), redactor, 50);

        let mut event = Event::new(
            EventType::WindowFocused {
                app_name: "kitty".into(),
                window_title: "Terminal".into(),
                window_class: "kitty".into(),
                window_id: "0x1".into(),
                pid: None,
                workspace: Some(1),
            },
            "os",
        );

        assert_eq!(gatekeeper.admit(&mut event), Admission::Admitted);

        // Disable 'os' adapter
        registry.set_enabled("os", false).unwrap();
        let mut event2 = Event::new(
            EventType::WindowFocused {
                app_name: "kitty".into(),
                window_title: "Terminal".into(),
                window_class: "kitty".into(),
                window_id: "0x2".into(),
                pid: None,
                workspace: Some(1),
            },
            "os",
        );

        assert!(matches!(gatekeeper.admit(&mut event2), Admission::Dropped(_)));
    }

    #[test]
    fn test_gatekeeper_enriches_error_detected() {
        let temp = NamedTempFile::new().unwrap();
        let registry = Arc::new(AdapterRegistry::load_from(temp.path()));
        let redactor = Arc::new(Redactor::new(false));
        let gatekeeper = Gatekeeper::new(registry, redactor, 50);

        let mut error_event = Event::new(
            EventType::ErrorDetected {
                command: "cargo build --release".into(),
                exit_code: 101,
                error_pattern: None,
                working_dir: "/tmp".into(),
            },
            "shell",
        );

        assert_eq!(gatekeeper.admit(&mut error_event), Admission::Admitted);

        if let EventType::ErrorDetected { error_pattern, .. } = &error_event.event_type {
            assert_eq!(error_pattern.as_deref(), Some("RustCompile"));
        } else {
            panic!("Expected ErrorDetected event");
        }

        let payload = error_event.payload.expect("payload should be present");
        assert_eq!(payload["classifier_category"], "RustCompile");
        assert!(payload["classifier_confidence"].as_f64().unwrap() >= 0.60);
    }
}
