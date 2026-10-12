//! MNSE Graph — Causal knowledge graph (Historian Model).
//!
//! Stores entities (nodes) and temporal/causal facts (edges) with
//! automatic Time-Tree sessioning (Session -> Burst -> Event) and
//! Short-Term Memory (STM) sliding windows.

pub mod db;
pub mod graph_builder;
pub mod ml;
pub mod session_manager;

pub use db::GraphDb;
pub use graph_builder::GraphBuilder;
pub use ml::{
    CausalAttribution, CausalDiffusion, HawkesConfig, HawkesDecayModel, PageRankDiffusion,
    TemporalDecayModel,
};
pub use session_manager::SessionManager;
