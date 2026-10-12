//! Modular Cellular Machine Learning and Statistical Mechanics subsystem.
//!
//! Houses pluggable, zero-dependency statistical and micro-ML models
//! behind explicit traits (`Send + Sync`) to enable dynamic replacement,
//! offline inference (<10 microseconds), and high-reliability sensory analysis.

pub mod error_classifier;
pub mod kalman;

pub use error_classifier::{CellularErrorClassifier, ErrorCategory, ErrorClassifier};
pub use kalman::{AdaptiveKalmanFilter, RateEstimator, RateMetrics};
