//! Adaptive Kalman Filter for Real-time Signal Rate Estimation & Anomaly Detection.
//!
//! Tracks underlying event arrival rate x_k (events/sec) and variance P_k per adapter:
//! Predict:
//!   x^-_k = x_{k-1}
//!   P^-_k = P_{k-1} + Q
//!
//! Correct:
//!   K_k = P^-_k / (P^-_k + R)
//!   x_k = x^-_k + K_k * (z_k - x^-_k)
//!   P_k = (1 - K_k) * P^-_k
//!
//! Anomaly:
//!   Z = (z_k - x^-_k) / sqrt(P^-_k + R) > 3.0σ

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateMetrics {
    pub estimated_rate: f64,
    pub variance: f64,
    pub z_score: f64,
    pub is_burst_anomaly: bool,
}

/// Pluggable trait for adaptive event rate tracking
pub trait RateEstimator: Send + Sync {
    /// Observe count of events for `source` in epoch second `epoch_sec`
    fn observe(&self, source: &str, epoch_sec: i64, count: u32) -> RateMetrics;

    /// Get current estimated rate for `source`
    fn current_rate(&self, source: &str) -> f64;
}

#[derive(Debug, Clone)]
struct AdapterKalmanState {
    rate: f64,
    p: f64,
    last_epoch: i64,
}

pub struct AdaptiveKalmanFilter {
    process_noise_q: f64,
    measurement_noise_r: f64,
    z_threshold: f64,
    anomaly_floor: f64,
    states: Mutex<HashMap<String, AdapterKalmanState>>,
}

impl Default for AdaptiveKalmanFilter {
    fn default() -> Self {
        Self {
            process_noise_q: 0.5,
            measurement_noise_r: 2.0,
            z_threshold: 3.0,
            anomaly_floor: 15.0,
            states: Mutex::new(HashMap::new()),
        }
    }
}

impl AdaptiveKalmanFilter {
    pub fn new(process_noise_q: f64, measurement_noise_r: f64, z_threshold: f64, anomaly_floor: f64) -> Self {
        Self {
            process_noise_q,
            measurement_noise_r,
            z_threshold,
            anomaly_floor,
            states: Mutex::new(HashMap::new()),
        }
    }
}

impl RateEstimator for AdaptiveKalmanFilter {
    fn observe(&self, source: &str, epoch_sec: i64, count: u32) -> RateMetrics {
        let mut states = match self.states.lock() {
            Ok(s) => s,
            Err(_) => {
                return RateMetrics {
                    estimated_rate: count as f64,
                    variance: 1.0,
                    z_score: 0.0,
                    is_burst_anomaly: false,
                }
            }
        };

        let state = states.entry(source.to_string()).or_insert_with(|| AdapterKalmanState {
            rate: count as f64,
            p: 5.0,
            last_epoch: epoch_sec,
        });

        // 1. Time Update (Predict)
        let elapsed_sec = (epoch_sec - state.last_epoch).max(1) as f64;
        let p_minus = state.p + self.process_noise_q * elapsed_sec;
        let x_minus = state.rate;

        // 2. Innovation & Standardized Z-Score
        let z = count as f64;
        let innovation = z - x_minus;
        let s = p_minus + self.measurement_noise_r;
        let z_score = if s > 0.0 { innovation / s.sqrt() } else { 0.0 };

        // 3. Measurement Update (Correct)
        let k = p_minus / (p_minus + self.measurement_noise_r);
        let x_updated = (x_minus + k * innovation).max(0.0);
        let p_updated = (1.0 - k) * p_minus;

        state.rate = x_updated;
        state.p = p_updated;
        state.last_epoch = epoch_sec;

        let is_burst_anomaly = z_score > self.z_threshold && z >= self.anomaly_floor;

        RateMetrics {
            estimated_rate: x_updated,
            variance: p_updated,
            z_score,
            is_burst_anomaly,
        }
    }

    fn current_rate(&self, source: &str) -> f64 {
        if let Ok(states) = self.states.lock() {
            states.get(source).map(|s| s.rate).unwrap_or(0.0)
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kalman_rate_convergence_and_burst_detection() {
        let kalman = AdaptiveKalmanFilter::default();

        // Feed steady stream of 5 events/sec for 10 seconds
        for t in 0..10 {
            let metrics = kalman.observe("fs", t, 5);
            assert!(!metrics.is_burst_anomaly);
        }

        // Estimated rate should have converged close to 5.0
        let rate = kalman.current_rate("fs");
        assert!((rate - 5.0).abs() < 1.0);

        // Sudden surge of 60 events/sec at t=11
        let surge = kalman.observe("fs", 11, 60);
        assert!(surge.z_score > 3.0);
        assert!(surge.is_burst_anomaly);
    }
}
