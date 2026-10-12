//! Hawkes Self-Exciting Point Process Attention Decay.
//!
//! Models developer cognitive load and short-term memory (STM) half-life:
//! λ(t) = μ + ∑_{t_i < t} α * exp(-β * (t - t_i))
//!
//! When activity arrives in bursts (e.g. frantic debugging, rapid compile/test cycles),
//! intensity λ(t) increases, dynamically tuning the attention retention threshold.

use std::collections::VecDeque;
use std::sync::Mutex;

/// Pluggable trait for temporal memory decay and attention tracking
pub trait TemporalDecayModel: Send + Sync {
    /// Record an event arrival timestamp (epoch seconds)
    fn record_event(&self, ts: i64);

    /// Check if an STM item created at `item_ts` is still retained at `current_ts`
    fn should_retain(&self, item_ts: i64, current_ts: i64) -> bool;

    /// Current self-exciting intensity λ(t)
    fn current_intensity(&self, current_ts: i64) -> f64;

    /// Normalized activation score [0.0, 1.0] for an item created at `item_ts`
    fn item_activation(&self, item_ts: i64, current_ts: i64) -> f64;
}

#[derive(Debug, Clone)]
pub struct HawkesConfig {
    /// Baseline background intensity μ (events/sec)
    pub base_rate: f64,
    /// Excitation magnitude α per incoming event
    pub excitation: f64,
    /// Exponential decay coefficient β (half-life = ln(2)/β)
    pub decay_rate: f64,
    /// Minimum activation threshold θ to remain in memory
    pub min_threshold: f64,
    /// Maximum retention horizon in seconds (hard upper bound)
    pub max_age_seconds: i64,
}

impl Default for HawkesConfig {
    fn default() -> Self {
        Self {
            base_rate: 0.05,
            excitation: 0.80,
            decay_rate: 0.015, // half-life ≈ 46.2 seconds
            min_threshold: 0.05,
            max_age_seconds: 600, // 10 minutes maximum cutoff
        }
    }
}

pub struct HawkesDecayModel {
    config: HawkesConfig,
    history: Mutex<VecDeque<i64>>,
}

impl HawkesDecayModel {
    pub fn new(config: HawkesConfig) -> Self {
        Self {
            config,
            history: Mutex::new(VecDeque::new()),
        }
    }
}

impl Default for HawkesDecayModel {
    fn default() -> Self {
        Self::new(HawkesConfig::default())
    }
}

impl TemporalDecayModel for HawkesDecayModel {
    fn record_event(&self, ts: i64) {
        if let Ok(mut history) = self.history.lock() {
            history.push_back(ts);

            // Prune history older than max_age_seconds
            let cutoff = ts - self.config.max_age_seconds;
            while let Some(&front) = history.front() {
                if front < cutoff {
                    history.pop_front();
                } else {
                    break;
                }
            }
        }
    }

    fn current_intensity(&self, current_ts: i64) -> f64 {
        let history = match self.history.lock() {
            Ok(h) => h,
            Err(_) => return self.config.base_rate,
        };

        let mut lambda = self.config.base_rate;
        for &t_i in history.iter().rev() {
            let delta = current_ts - t_i;
            if delta < 0 {
                continue;
            }
            if delta > self.config.max_age_seconds {
                break;
            }
            lambda += self.config.excitation * (-self.config.decay_rate * delta as f64).exp();
        }

        lambda
    }

    fn item_activation(&self, item_ts: i64, current_ts: i64) -> f64 {
        if current_ts < item_ts {
            return 1.0;
        }
        let delta = (current_ts - item_ts) as f64;
        (-self.config.decay_rate * delta).exp().clamp(0.0, 1.0)
    }

    fn should_retain(&self, item_ts: i64, current_ts: i64) -> bool {
        let delta = current_ts - item_ts;
        if delta < 0 {
            return true;
        }
        if delta > self.config.max_age_seconds {
            return false;
        }

        let activation = self.item_activation(item_ts, current_ts);
        let lambda = self.current_intensity(current_ts);

        // Dynamic retention threshold scaling:
        // During bursty activity cascades (high λ), obsolete items decay out faster
        // During quiet periods (low λ), active items linger up to the threshold
        let dynamic_threshold = self.config.min_threshold
            * (1.0 + 0.3 * ((lambda - self.config.base_rate) / (self.config.base_rate + 1.0)));

        activation >= dynamic_threshold.clamp(self.config.min_threshold, 0.50)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hawkes_excitation_and_decay() {
        let model = HawkesDecayModel::default();
        let t0 = 1000;

        // Base rate before any events
        assert!((model.current_intensity(t0) - 0.05).abs() < 1e-4);

        // Record a burst of 5 events
        for i in 0..5 {
            model.record_event(t0 + i);
        }

        // Intensity immediately after burst should be much higher than base rate
        let burst_intensity = model.current_intensity(t0 + 5);
        assert!(burst_intensity > 2.0);

        // After long passage of time (300 seconds), intensity decays back towards base rate
        let decayed_intensity = model.current_intensity(t0 + 300);
        assert!(decayed_intensity < burst_intensity);
        assert!(decayed_intensity < 0.20);
    }

    #[test]
    fn test_hawkes_retention_window() {
        let model = HawkesDecayModel::default();
        let t0 = 1000;

        // Fresh item should always be retained
        assert!(model.should_retain(t0, t0 + 10));

        // After 40 seconds (roughly one half-life), still retained
        assert!(model.should_retain(t0, t0 + 40));

        // Item older than max_age_seconds (600s) is dropped
        assert!(!model.should_retain(t0, t0 + 601));
    }
}
