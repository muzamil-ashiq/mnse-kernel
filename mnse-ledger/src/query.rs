//! Ledger query parameters and filtering options.

use serde::{Deserialize, Serialize};

/// Filter options for querying events from the ledger
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventFilter {
    /// Exact date (YYYY-MM-DD)
    pub date: Option<String>,

    /// Start range (timestamp string or YYYY-MM-DD)
    pub from: Option<String>,

    /// End range (timestamp string or YYYY-MM-DD)
    pub to: Option<String>,

    /// Filter by source category ("os", "shell", "browser", "editor", etc.)
    pub source: Option<String>,

    /// Filter by exact event_type_name ("ProcessExecuted", "ErrorDetected", etc.)
    pub event_type: Option<String>,

    /// List of event_type_names to exclude from results
    pub exclude_types: Option<Vec<String>>,

    /// Full-text search query (FTS5 across payload and text fields)
    pub search: Option<String>,

    /// Maximum number of events to return (default: 100)
    pub limit: Option<usize>,
}

impl EventFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn with_type(mut self, event_type: impl Into<String>) -> Self {
        self.event_type = Some(event_type.into());
        self
    }

    pub fn with_search(mut self, query: impl Into<String>) -> Self {
        self.search = Some(query.into());
        self
    }

    pub fn with_date(mut self, date: impl Into<String>) -> Self {
        self.date = Some(date.into());
        self
    }

    pub fn with_from(mut self, from: impl Into<String>) -> Self {
        self.from = Some(from.into());
        self
    }

    pub fn with_to(mut self, to: impl Into<String>) -> Self {
        self.to = Some(to.into());
        self
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn exclude(mut self, types: Vec<String>) -> Self {
        self.exclude_types = Some(types);
        self
    }
}
