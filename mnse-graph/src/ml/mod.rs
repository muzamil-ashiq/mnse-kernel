//! Modular Statistical & Cellular ML models for the Causal Knowledge Graph.
//!
//! Subsystems:
//! - Hawkes Attention Decay (`hawkes`): Dynamic short-term memory half-life modeling via self-exciting point processes.
//! - Causal Graph Diffusion (`diffusion`): Personalized PageRank / Random Walk with Restart for root-cause attribution.

pub mod diffusion;
pub mod hawkes;

pub use diffusion::{CausalAttribution, CausalDiffusion, PageRankDiffusion};
pub use hawkes::{HawkesConfig, HawkesDecayModel, TemporalDecayModel};
