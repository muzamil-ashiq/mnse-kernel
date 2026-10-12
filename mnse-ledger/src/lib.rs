//! MNSE Ledger — Persistent event ledger for kernel durability.
//!
//! Stores all non-noise events with time-based retrieval, FTS5 full-text search,
//! auto-archiving, and multi-tier retention policies.

pub mod db;
pub mod query;

pub use db::LedgerDb;
pub use query::EventFilter;

use anyhow::Result;
use mnse_types::{ContextSnapshot, DailyStats, Event};
use std::path::{Path, PathBuf};

/// High-level Ledger interface
pub struct Ledger {
    db: LedgerDb,
    path: PathBuf,
}

impl Ledger {
    /// Open or create ledger at path
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let db = LedgerDb::open(&path)?;
        Ok(Self { db, path: path_buf })
    }

    /// Open ledger in read-only mode (safe for concurrent inspection)
    pub fn open_read_only<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let db = LedgerDb::open_read_only(&path)?;
        Ok(Self { db, path: path_buf })
    }

    /// Store a single event (returns Ok(false) if event was filtered out as noise)
    pub fn store_event(&self, event: &Event) -> Result<bool> {
        self.db.insert_event(event)
    }

    /// Store a batch of events inside a single transaction
    pub fn store_events_batch(&self, events: &[Event]) -> Result<u64> {
        self.db.insert_events_batch(events)
    }

    /// Store a live context snapshot
    pub fn store_snapshot(&self, snapshot: &ContextSnapshot) -> Result<String> {
        self.db.insert_snapshot(snapshot)
    }

    /// Query events using filter criteria
    pub fn query(&self, filter: EventFilter) -> Result<Vec<Event>> {
        self.db.query_events(filter)
    }

    /// Query event by full UUID or short prefix
    pub fn get_event_by_id(&self, id: &str) -> Result<Option<Event>> {
        self.db.query_event_by_id(id)
    }

    /// Query latest snapshot before timestamp
    pub fn get_latest_snapshot_before(&self, timestamp: i64) -> Result<Option<ContextSnapshot>> {
        self.db.query_latest_snapshot_before(timestamp)
    }

    /// Query events within timestamp range
    pub fn query_events_in_range(&self, start: i64, end: i64) -> Result<Vec<Event>> {
        self.db.query_events_in_range(start, end)
    }

    /// Query recent events chronologically for startup replay
    pub fn recent_events(&self, limit: usize) -> Result<Vec<Event>> {
        self.db.query_recent_events(limit)
    }

    /// Get last error event before timestamp (for causal linking)
    pub fn get_last_error_before(
        &self,
        before_timestamp: i64,
        max_age_secs: i64,
    ) -> Result<Option<Event>> {
        self.db.get_last_error_before(before_timestamp, max_age_secs)
    }

    /// Get daily activity statistics
    pub fn daily_stats(&self, date: &str) -> Result<DailyStats> {
        self.db.get_daily_stats(date)
    }

    /// Prune events older than specified days (with JSONL auto-archiving)
    pub fn prune_events(&self, days: u32) -> Result<u64> {
        self.db.prune_events_before(days)
    }

    /// Total milestone events count
    pub fn total_events(&self) -> Result<u64> {
        self.db.count_events()
    }

    /// List distinct event sources in database
    pub fn get_sources(&self) -> Result<Vec<String>> {
        self.db.get_sources()
    }

    /// Create a raw backup of the ledger database
    pub fn backup(&self) -> Result<PathBuf> {
        let backup_path = self.path.with_extension("db.bak");
        std::fs::copy(&self.path, &backup_path)?;
        Ok(backup_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mnse_types::EventType;
    use tempfile::tempdir;

    #[test]
    fn test_ledger_wrapper() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("ledger.db");
        let ledger = Ledger::open(&path).unwrap();

        let event = Event::new(
            EventType::ProcessExecuted {
                command: "ls -la".into(),
                exit_code: 0,
                duration_ms: 10,
                working_dir: "/tmp".into(),
                stdout_tail: None,
                stderr_tail: None,
            },
            "shell",
        );

        let stored = ledger.store_event(&event).unwrap();
        assert!(stored);

        assert_eq!(ledger.total_events().unwrap(), 1);

        let found = ledger.get_event_by_id(&event.id.to_string()).unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, event.id);

        let backup = ledger.backup().unwrap();
        assert!(backup.exists());
    }
}
