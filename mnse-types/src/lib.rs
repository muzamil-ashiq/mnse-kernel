//! # mnse-types
//!
//! Shared types for the MNSE (Machine Native State Engine) kernel.
//! Zero business logic — pure data structures shared across all crates.
//!
//! Every crate in the workspace depends on this. It has no dependencies
//! except serde, uuid, and chrono.

pub mod event;
pub mod snapshot;
pub mod graph;
pub mod priority;
pub mod temporal;

pub use event::{DisconnectReason, Event, EventSource, EventType};
pub use snapshot::{AdapterStatus, ContextSnapshot, DailyStats, RecentCommand, RecentError, SessionInfo};
pub use graph::{NodeType, EdgeRelation};
pub use priority::Priority;
pub use temporal::parse_natural_time;
