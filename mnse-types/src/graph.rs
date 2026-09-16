//! Knowledge graph types — node and edge taxonomy for mnse-graph.
//!
//! The graph is a Historian Model (ADR 004): fact-only, no guessed edges.
//! Every edge has confidence = 1.0 and was recorded because it was observed,
//! not inferred. Inference happens in the agent layer, not here.

use serde::{Deserialize, Serialize};

/// All node types in the knowledge graph.
///
/// Nodes are entities that persist in the graph across events.
/// Each node has a unique stable ID (prefix:hash format per ADR 005).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum NodeType {
    /// A file on disk (stable ID: "file:///absolute/path")
    File,
    /// A shell command (stable ID: "cmd:<sha256-8>")
    Command,
    /// A browser URL or domain (stable ID: "url:<sha256-8>")
    Url,
    /// A clipboard item (stable ID: "clip:<sha256-8>")
    Clipboard,
    /// An application (stable ID: "app:<name>")
    App,
    /// A git branch (stable ID: "branch:<repo>/<name>")
    Branch,
    /// A work session (60-minute window)
    Session,
    /// A burst of activity within a session (5-minute window)
    Burst,
    /// A search query extracted from a URL
    SearchQuery,
    /// A domain (e.g. "docs.rust-lang.org")
    Domain,
    /// A person identified by the vision adapter
    Person,
    /// A concept extracted from context (e.g. error patterns, topics)
    Concept,
}

impl NodeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::File        => "File",
            Self::Command     => "Command",
            Self::Url         => "Url",
            Self::Clipboard   => "Clipboard",
            Self::App         => "App",
            Self::Branch      => "Branch",
            Self::Session     => "Session",
            Self::Burst       => "Burst",
            Self::SearchQuery => "SearchQuery",
            Self::Domain      => "Domain",
            Self::Person      => "Person",
            Self::Concept     => "Concept",
        }
    }
}

/// All edge relations in the knowledge graph.
///
/// Design: All edges are facts (Historian Model). No CAUSED or INFORMED edges
/// unless explicitly recorded. The only probabilistic thing is `IN_STM_DURING`
/// which has confidence = 1.0 because it is a temporal fact, not a causal guess.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EdgeRelation {
    /// A command was executed in this directory/app context
    ExecutedIn,
    /// A command caused an error (parent_event_id chain)
    CausedError,
    /// A file was modified after this command ran (temporal proximity)
    ModifiedAfter,
    /// This event occurred during this STM window (temporal fact, not causal guess)
    InStmDuring,
    /// A browser search led to this URL being visited
    SearchedTo,
    /// This URL belongs to this domain
    BelongsToDomain,
    /// A file belongs to this git branch/repo context
    TrackedIn,
    /// A burst is part of a session
    PartOfSession,
    /// An event is part of a burst
    PartOfBurst,
    /// A person was visible during this event
    ObservedBy,
    /// Two nodes are temporally related (within same burst)
    CoOccurredWith,
    /// Edge weight increased (node was referenced again)
    Reinforces,
}

impl EdgeRelation {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ExecutedIn      => "EXECUTED_IN",
            Self::CausedError     => "CAUSED_ERROR",
            Self::ModifiedAfter   => "MODIFIED_AFTER",
            Self::InStmDuring     => "IN_STM_DURING",
            Self::SearchedTo      => "SEARCHED_TO",
            Self::BelongsToDomain => "BELONGS_TO_DOMAIN",
            Self::TrackedIn       => "TRACKED_IN",
            Self::PartOfSession   => "PART_OF_SESSION",
            Self::PartOfBurst     => "PART_OF_BURST",
            Self::ObservedBy      => "OBSERVED_BY",
            Self::CoOccurredWith  => "CO_OCCURRED_WITH",
            Self::Reinforces      => "REINFORCES",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_types_have_stable_strings() {
        assert_eq!(NodeType::File.as_str(), "File");
        assert_eq!(NodeType::Session.as_str(), "Session");
        assert_eq!(NodeType::Person.as_str(), "Person");
    }

    #[test]
    fn edge_relations_are_uppercase() {
        assert_eq!(EdgeRelation::InStmDuring.as_str(), "IN_STM_DURING");
        assert_eq!(EdgeRelation::CausedError.as_str(), "CAUSED_ERROR");
        assert_eq!(EdgeRelation::ExecutedIn.as_str(), "EXECUTED_IN");
    }

    #[test]
    fn node_type_serde_roundtrip() {
        let n = NodeType::File;
        let json = serde_json::to_string(&n).unwrap();
        let back: NodeType = serde_json::from_str(&json).unwrap();
        assert_eq!(n, back);
    }
}
