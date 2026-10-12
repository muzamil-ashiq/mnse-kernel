//! SQLite database operations for the Ledger (v2 Engine).

use anyhow::Result;
use chrono::{DateTime, Utc};
use mnse_types::{ContextSnapshot, DailyStats, Event, EventSource, EventType};
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use uuid::Uuid;

use crate::EventFilter;

/// Expected schema hash (SHA256 of normalized sqlite_master table SQLs)
const EXPECTED_SCHEMA_HASH: &str =
    "736df09a6331908bf29bd4975f8ea5cbf86ab7d457636e0539f1c71383bdca23";

/// SQLite-backed ledger database with safe lock handling and FTS5 search
pub struct LedgerDb {
    write_conn: Mutex<Connection>,
    read_conn: Mutex<Connection>,
}

impl LedgerDb {
    /// Safe lock acquisition for read queries
    fn acquire_lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.read_conn.lock().map_err(|e| {
            tracing::error!("Read database lock poisoned: {}", e);
            anyhow::anyhow!("read database lock poisoned")
        })
    }

    /// Safe lock acquisition for write operations
    fn acquire_write_lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.write_conn.lock().map_err(|e| {
            tracing::error!("Write database lock poisoned: {}", e);
            anyhow::anyhow!("write database lock poisoned")
        })
    }

    /// Open or create database at specified path
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        fn try_open(path: &Path) -> Result<Connection> {
            let conn = Connection::open(path)?;
            conn.busy_timeout(std::time::Duration::from_millis(5000))?;
            conn.pragma_update(None, "journal_mode", "WAL")?;
            conn.pragma_update(None, "synchronous", "NORMAL")?;
            conn.pragma_update(None, "foreign_keys", "ON")?;
            conn.pragma_update(None, "cache_size", "-64000")?; // 64MB Cache
            conn.pragma_update(None, "temp_store", "MEMORY")?;  // Keep temp tables in RAM
            conn.pragma_update(None, "mmap_size", "268435456")?; // Memory-map up to 256MB
            
            let integrity: String = conn
                .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
            if integrity != "ok" {
                return Err(anyhow::anyhow!("PRAGMA quick_check failed: {}", integrity));
            }
            Ok(conn)
        }

        let conn = match try_open(path) {
            Ok(conn) => conn,
            Err(e) => {
                let err_str = e.to_string();
                let is_corrupted = err_str.contains("malformed")
                    || err_str.contains("file is not a database")
                    || err_str.contains("quick_check failed");

                if is_corrupted && path.exists() {
                    tracing::error!("⚠️ ledger.db integrity check FAILED: {}. Performing auto-recovery...", e);
                    let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
                    let corrupt_path = path.with_extension(format!("db.corrupt.{}", timestamp));
                    tracing::warn!("Moving corrupt database {:?} to {:?}", path, corrupt_path);
                    let _ = std::fs::rename(path, &corrupt_path);
                    
                    let wal_path = path.with_extension("db-wal");
                    if wal_path.exists() {
                        let _ = std::fs::rename(&wal_path, path.with_extension(format!("db-wal.corrupt.{}", timestamp)));
                    }
                    let shm_path = path.with_extension("db-shm");
                    if shm_path.exists() {
                        let _ = std::fs::rename(&shm_path, path.with_extension(format!("db-shm.corrupt.{}", timestamp)));
                    }

                    let conn = Connection::open(path)?;
                    conn.busy_timeout(std::time::Duration::from_millis(5000))?;
                    conn.pragma_update(None, "journal_mode", "WAL")?;
                    conn.pragma_update(None, "synchronous", "NORMAL")?;
                    conn.pragma_update(None, "foreign_keys", "ON")?;
                    conn.pragma_update(None, "cache_size", "-64000")?;
                    conn.pragma_update(None, "temp_store", "MEMORY")?;
                    conn.pragma_update(None, "mmap_size", "268435456")?;
                    tracing::info!("Initialized a fresh ledger.db after recovery.");
                    conn
                } else {
                    return Err(e);
                }
            }
        };

        let read_conn = Connection::open(path)?;
        read_conn.busy_timeout(std::time::Duration::from_millis(5000))?;
        read_conn.pragma_update(None, "journal_mode", "WAL")?;
        read_conn.pragma_update(None, "synchronous", "NORMAL")?;
        read_conn.pragma_update(None, "foreign_keys", "ON")?;
        read_conn.pragma_update(None, "cache_size", "-64000")?;
        read_conn.pragma_update(None, "temp_store", "MEMORY")?;
        read_conn.pragma_update(None, "mmap_size", "268435456")?;

        let db = Self {
            write_conn: Mutex::new(conn),
            read_conn: Mutex::new(read_conn),
        };
        db.init_schema()?;
        Ok(db)
    }

    /// Open database in read-only mode (safe for concurrent CLI inspection)
    pub fn open_read_only<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let conn = Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(std::time::Duration::from_millis(3000))?;
        let read_conn = Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        read_conn.busy_timeout(std::time::Duration::from_millis(3000))?;
        Ok(Self {
            write_conn: Mutex::new(conn),
            read_conn: Mutex::new(read_conn),
        })
    }

    /// Initialize SQLite schema, FTS5 virtual table, and indices
    fn init_schema(&self) -> Result<()> {
        let conn = self.acquire_write_lock()?;
        conn.execute_batch(
            r#"
            -- Primary Events Table
            CREATE TABLE IF NOT EXISTS events (
                id TEXT PRIMARY KEY,
                timestamp INTEGER NOT NULL,
                date TEXT NOT NULL,
                source TEXT NOT NULL,
                event_type_name TEXT NOT NULL,
                event_type TEXT NOT NULL,
                observed_by TEXT NOT NULL,
                parent_event_id TEXT,
                payload TEXT,
                redacted BOOLEAN DEFAULT 0
            );

            -- Snapshots Table
            CREATE TABLE IF NOT EXISTS snapshots (
                id TEXT PRIMARY KEY,
                timestamp INTEGER NOT NULL,
                date TEXT NOT NULL,
                data TEXT NOT NULL
            );

            -- Fast Lookup Indexes
            CREATE INDEX IF NOT EXISTS idx_events_date ON events(date);
            CREATE INDEX IF NOT EXISTS idx_events_source ON events(source);
            CREATE INDEX IF NOT EXISTS idx_events_type_name ON events(event_type_name);
            CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);
            CREATE INDEX IF NOT EXISTS idx_events_parent ON events(parent_event_id);
            CREATE INDEX IF NOT EXISTS idx_events_type_ts ON events(event_type_name, timestamp DESC);

            -- FTS5 Virtual Table for Payload & Text Search
            CREATE VIRTUAL TABLE IF NOT EXISTS events_fts USING fts5(
                source,
                event_type_name,
                event_type,
                payload,
                content='events',
                content_rowid='rowid'
            );

            -- FTS5 Sync Triggers (Indexes text-heavy events only to avoid write amplification)
            CREATE TRIGGER IF NOT EXISTS events_ai AFTER INSERT ON events
            WHEN new.event_type_name IN ('ProcessExecuted', 'UrlChanged', 'FileSwitched', 'ErrorDetected', 'WindowFocused', 'DirectoryChanged', 'DiagnosticsUpdated', 'TranscriptionResult', 'Custom')
            BEGIN
                INSERT INTO events_fts(rowid, source, event_type_name, event_type, payload)
                VALUES (new.rowid, new.source, new.event_type_name, new.event_type, new.payload);
            END;

            CREATE TRIGGER IF NOT EXISTS events_ad AFTER DELETE ON events
            WHEN old.event_type_name IN ('ProcessExecuted', 'UrlChanged', 'FileSwitched', 'ErrorDetected', 'WindowFocused', 'DirectoryChanged', 'DiagnosticsUpdated', 'TranscriptionResult', 'Custom')
            BEGIN
                INSERT INTO events_fts(events_fts, rowid, source, event_type_name, event_type, payload)
                VALUES ('delete', old.rowid, old.source, old.event_type_name, old.event_type, old.payload);
            END;

            CREATE TRIGGER IF NOT EXISTS events_au AFTER UPDATE ON events
            WHEN new.event_type_name IN ('ProcessExecuted', 'UrlChanged', 'FileSwitched', 'ErrorDetected', 'WindowFocused', 'DirectoryChanged', 'DiagnosticsUpdated', 'TranscriptionResult', 'Custom')
                 OR old.event_type_name IN ('ProcessExecuted', 'UrlChanged', 'FileSwitched', 'ErrorDetected', 'WindowFocused', 'DirectoryChanged', 'DiagnosticsUpdated', 'TranscriptionResult', 'Custom')
            BEGIN
                INSERT INTO events_fts(events_fts, rowid, source, event_type_name, event_type, payload)
                VALUES ('delete', old.rowid, old.source, old.event_type_name, old.event_type, old.payload);
                INSERT INTO events_fts(rowid, source, event_type_name, event_type, payload)
                VALUES (new.rowid, new.source, new.event_type_name, new.event_type, new.payload);
            END;
            "#,
        )?;

        drop(conn);
        self.verify_schema()?;
        Ok(())
    }

    /// Verify schema consistency
    fn verify_schema(&self) -> Result<()> {
        let conn = self.acquire_write_lock()?;
        let mut stmt = conn.prepare(
            "SELECT sql FROM sqlite_master \
             WHERE sql IS NOT NULL \
               AND name NOT LIKE 'sqlite_%' \
               AND name NOT LIKE 'events_fts_%' \
             ORDER BY name;",
        )?;
        let sql_rows: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|s| s.ok())
            .collect();

        let combined_sql = sql_rows.join(" ");
        let normalized = combined_sql
            .to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        let mut hasher = Sha256::new();
        hasher.update(normalized.as_bytes());
        let hash = format!("{:x}", hasher.finalize());

        if hash != EXPECTED_SCHEMA_HASH {
            tracing::warn!("⚠️ MNSE Schema Hash Check: Schema on disk differs from static reference.");
            tracing::debug!("Actual schema hash: {}", hash);
        } else {
            tracing::info!("✅ Database schema hash verified (v2.0).");
        }

        Ok(())
    }

    /// Insert an event into SQLite.
    /// Returns `Ok(true)` if written, or `Ok(false)` if skipped (e.g. noise filter).
    pub fn insert_event(&self, event: &Event) -> Result<bool> {
        // Noise events NEVER hit SQLite (Day 1 rule)
        if event.event_type.is_noise() {
            return Ok(false);
        }

        let conn = self.acquire_write_lock()?;
        let ts_sec = event.timestamp.timestamp();
        let date_str = event.timestamp.format("%Y-%m-%d").to_string();
        let source_str = format!("{:?}", event.source);
        let type_name = event.event_type.type_name();
        let event_type_json = serde_json::to_string(&event.event_type)?;
        let payload_json = event
            .payload
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?
            .unwrap_or_else(|| "{}".to_string());
        let parent_id_str = event.parent_event_id.map(|u| u.to_string());

        conn.execute(
            "INSERT OR REPLACE INTO events (
                id, timestamp, date, source, event_type_name, event_type, 
                observed_by, parent_event_id, payload, redacted
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0)",
            params![
                event.id.to_string(),
                ts_sec,
                date_str,
                source_str,
                type_name,
                event_type_json,
                event.observed_by,
                parent_id_str,
                payload_json
            ],
        )?;

        Ok(true)
    }

    /// Insert a batch of events inside a transaction (skipping noise events)
    pub fn insert_events_batch(&self, events: &[Event]) -> Result<u64> {
        let milestone_events: Vec<&Event> = events
            .iter()
            .filter(|e| !e.event_type.is_noise())
            .collect();

        if milestone_events.is_empty() {
            return Ok(0);
        }

        let mut conn = self.acquire_write_lock()?;
        let tx = conn.transaction()?;
        let mut inserted = 0u64;

        {
            let mut stmt = tx.prepare_cached(
                "INSERT OR REPLACE INTO events (
                    id, timestamp, date, source, event_type_name, event_type, 
                    observed_by, parent_event_id, payload, redacted
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0)",
            )?;

            for event in milestone_events {
                let ts_sec = event.timestamp.timestamp();
                let date_str = event.timestamp.format("%Y-%m-%d").to_string();
                let source_str = format!("{:?}", event.source);
                let type_name = event.event_type.type_name();
                let event_type_json = serde_json::to_string(&event.event_type)?;
                let payload_json = event
                    .payload
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?
                    .unwrap_or_else(|| "{}".to_string());
                let parent_id_str = event.parent_event_id.map(|u| u.to_string());

                stmt.execute(params![
                    event.id.to_string(),
                    ts_sec,
                    date_str,
                    source_str,
                    type_name,
                    event_type_json,
                    event.observed_by,
                    parent_id_str,
                    payload_json
                ])?;
                inserted += 1;
            }
        }

        tx.commit()?;
        Ok(inserted)
    }

    /// Store snapshot
    pub fn insert_snapshot(&self, snapshot: &ContextSnapshot) -> Result<String> {
        let conn = self.acquire_write_lock()?;
        let id = Uuid::new_v4().to_string();
        let ts_sec = snapshot.timestamp.timestamp();
        let date_str = snapshot.timestamp.format("%Y-%m-%d").to_string();
        let data_json = serde_json::to_string(snapshot)?;

        conn.execute(
            "INSERT INTO snapshots (id, timestamp, date, data) VALUES (?1, ?2, ?3, ?4)",
            params![id, ts_sec, date_str, data_json],
        )?;

        Ok(id)
    }

    /// Query events using filter criteria
    pub fn query_events(&self, filter: EventFilter) -> Result<Vec<Event>> {
        let conn = self.acquire_lock()?;
        let has_fts = filter.search.is_some();
        let prefix = if has_fts { "e." } else { "" };

        let mut sql = if has_fts {
            String::from(
                "SELECT e.id, e.timestamp, e.source, e.event_type_name, e.event_type, \
                        e.observed_by, e.parent_event_id, e.payload \
                 FROM events e \
                 JOIN events_fts ON e.rowid = events_fts.rowid \
                 WHERE 1=1",
            )
        } else {
            String::from(
                "SELECT id, timestamp, source, event_type_name, event_type, \
                        observed_by, parent_event_id, payload \
                 FROM events WHERE 1=1",
            )
        };

        let mut params_vec: Vec<String> = Vec::new();

        if let Some(date) = &filter.date {
            sql.push_str(&format!(" AND {}date = ?", prefix));
            params_vec.push(date.clone());
        }

        if let Some(source) = &filter.source {
            sql.push_str(&format!(" AND {}source = ? COLLATE NOCASE", prefix));
            params_vec.push(source.clone());
        }

        if let Some(event_type) = &filter.event_type {
            sql.push_str(&format!(" AND {}event_type_name = ?", prefix));
            params_vec.push(event_type.clone());
        }

        if let Some(excluded) = &filter.exclude_types {
            if !excluded.is_empty() {
                let placeholders = excluded.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
                sql.push_str(&format!(" AND {}event_type_name NOT IN ({})", prefix, placeholders));
                for t in excluded {
                    params_vec.push(t.clone());
                }
            }
        }

        if let Some(from) = &filter.from {
            if let Ok(ts) = from.parse::<i64>() {
                sql.push_str(&format!(" AND {}timestamp >= ?", prefix));
                params_vec.push(ts.to_string());
            } else {
                let date = from.split_whitespace().next().unwrap_or(from);
                sql.push_str(&format!(" AND {}date >= ?", prefix));
                params_vec.push(date.to_string());
            }
        }

        if let Some(to) = &filter.to {
            if let Ok(ts) = to.parse::<i64>() {
                sql.push_str(&format!(" AND {}timestamp <= ?", prefix));
                params_vec.push(ts.to_string());
            } else {
                let date = to.split_whitespace().next().unwrap_or(to);
                sql.push_str(&format!(" AND {}date <= ?", prefix));
                params_vec.push(date.to_string());
            }
        }

        if let Some(search) = &filter.search {
            sql.push_str(" AND events_fts MATCH ?");
            params_vec.push(search.clone());
        }

        sql.push_str(&format!(" ORDER BY {}timestamp DESC", prefix));
        sql.push_str(&format!(" LIMIT {}", filter.limit.unwrap_or(100)));

        let mut stmt = conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec
            .iter()
            .map(|s| s as &dyn rusqlite::ToSql)
            .collect();

        let events = stmt
            .query_map(params_refs.as_slice(), |row| self.row_to_event(row))?
            .filter_map(|e| e.ok())
            .collect();

        Ok(events)
    }

    /// Query events within timestamp range
    pub fn query_events_in_range(&self, start_ts: i64, end_ts: i64) -> Result<Vec<Event>> {
        let conn = self.acquire_lock()?;
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, source, event_type_name, event_type, \
                    observed_by, parent_event_id, payload \
             FROM events \
             WHERE timestamp >= ?1 AND timestamp <= ?2 \
             ORDER BY timestamp ASC",
        )?;

        let events = stmt
            .query_map([start_ts, end_ts], |row| self.row_to_event(row))?
            .filter_map(|e| e.ok())
            .collect();

        Ok(events)
    }

    /// Query single event by ID or prefix match
    pub fn query_event_by_id(&self, id: &str) -> Result<Option<Event>> {
        let conn = self.acquire_lock()?;
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, source, event_type_name, event_type, \
                    observed_by, parent_event_id, payload \
             FROM events \
             WHERE id LIKE ?1 ESCAPE '\\' LIMIT 1",
        )?;

        let escaped = id.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        let pattern = format!("{}%", escaped);
        let mut rows = stmt.query([pattern])?;

        if let Some(row) = rows.next()? {
            Ok(Some(self.row_to_event(row)?))
        } else {
            Ok(None)
        }
    }

    /// Query recent events for replay
    pub fn query_recent_events(&self, limit: usize) -> Result<Vec<Event>> {
        let conn = self.acquire_lock()?;
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, source, event_type_name, event_type, \
                    observed_by, parent_event_id, payload \
             FROM events ORDER BY timestamp DESC LIMIT ?1",
        )?;

        let mut events: Vec<Event> = stmt
            .query_map([limit], |row| self.row_to_event(row))?
            .filter_map(|e| e.ok())
            .collect();

        events.reverse(); // Chronological order
        Ok(events)
    }

    /// Query latest snapshot before timestamp
    pub fn query_latest_snapshot_before(&self, timestamp: i64) -> Result<Option<ContextSnapshot>> {
        let conn = self.acquire_lock()?;
        let mut stmt = conn.prepare(
            "SELECT data FROM snapshots WHERE timestamp <= ?1 ORDER BY timestamp DESC LIMIT 1",
        )?;
        let mut rows = stmt.query([timestamp])?;

        if let Some(row) = rows.next()? {
            let data_json: String = row.get(0)?;
            let snapshot: ContextSnapshot = serde_json::from_str(&data_json)?;
            Ok(Some(snapshot))
        } else {
            Ok(None)
        }
    }

    /// Get last error event before timestamp (for causal linking)
    pub fn get_last_error_before(
        &self,
        before_timestamp: i64,
        max_age_secs: i64,
    ) -> Result<Option<Event>> {
        let conn = self.acquire_lock()?;
        let min_timestamp = before_timestamp - max_age_secs;

        let mut stmt = conn.prepare(
            "SELECT id, timestamp, source, event_type_name, event_type, \
                    observed_by, parent_event_id, payload \
             FROM events \
             WHERE timestamp < ?1 AND timestamp > ?2 \
             AND (event_type_name = 'ErrorDetected' OR \
                  (event_type_name = 'ProcessExecuted' AND event_type LIKE '%\"exit_code\":%' AND event_type NOT LIKE '%\"exit_code\":0%')) \
             ORDER BY timestamp DESC LIMIT 1",
        )?;

        let result = stmt
            .query_map(params![before_timestamp, min_timestamp], |row| {
                self.row_to_event(row)
            })?
            .filter_map(|e| e.ok())
            .next();

        Ok(result)
    }

    /// Total event count
    pub fn count_events(&self) -> Result<u64> {
        let conn = self.acquire_lock()?;
        let count: u64 = conn
            .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
            .unwrap_or(0);
        Ok(count)
    }

    /// Distinct sources in ledger
    pub fn get_sources(&self) -> Result<Vec<String>> {
        let conn = self.acquire_lock()?;
        let mut stmt = conn.prepare("SELECT DISTINCT source FROM events ORDER BY source ASC")?;
        let sources: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|s| s.ok())
            .collect();
        Ok(sources)
    }

    /// Daily stats calculation
    pub fn get_daily_stats(&self, date_str: &str) -> Result<DailyStats> {
        let conn = self.acquire_lock()?;

        let events_total: u64 = conn
            .query_row("SELECT COUNT(*) FROM events WHERE date = ?1", [date_str], |r| r.get(0))
            .unwrap_or(0);

        let commands_run: u32 = conn
            .query_row("SELECT COUNT(*) FROM events WHERE date = ?1 AND event_type_name = 'ProcessExecuted'", [date_str], |r| r.get(0))
            .unwrap_or(0);

        let files_touched: u32 = conn
            .query_row("SELECT COUNT(*) FROM events WHERE date = ?1 AND event_type_name = 'FileSwitched'", [date_str], |r| r.get(0))
            .unwrap_or(0);

        let errors_count: u32 = conn
            .query_row(
                "SELECT COUNT(*) FROM events WHERE date = ?1 AND (event_type_name = 'ErrorDetected' OR (event_type_name = 'ProcessExecuted' AND event_type NOT LIKE '%\"exit_code\":0%'))",
                [date_str],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let mut stmt = conn.prepare("SELECT DISTINCT observed_by FROM events WHERE date = ?1")?;
        let focused_apps: Vec<String> = stmt
            .query_map([date_str], |r| r.get(0))?
            .filter_map(|s| s.ok())
            .collect();

        Ok(DailyStats {
            date: date_str.to_string(),
            events_total,
            commands_run,
            files_touched,
            errors_count,
            focused_apps,
        })
    }

    /// Multi-tier Pruning & Archiving
    pub fn prune_events_before(&self, days: u32) -> Result<u64> {
        let conn = self.acquire_write_lock()?;
        let now_sec = Utc::now().timestamp();
        let long_cutoff = now_sec - (days as i64 * 86400);
        let short_cutoff = now_sec - (7 * 86400);

        // 1. Archive to JSONL before deletion
        let archived_bg = self.archive_events_before(&conn, short_cutoff, true)?;
        let archived_long = self.archive_events_before(&conn, long_cutoff, false)?;

        if archived_bg > 0 || archived_long > 0 {
            tracing::info!(
                "Archived {} background events and {} long-retention events before prune",
                archived_bg,
                archived_long
            );
        }

        // 2. Transactional Prune
        conn.execute("BEGIN IMMEDIATE", [])?;

        let deleted_bg = match conn.execute(
            "DELETE FROM events WHERE timestamp < ?1 AND event_type_name IN \
             ('ResourcesUpdated', 'AdapterHeartbeat', 'RunningAppsChanged', 'TabsUpdated', \
              'SystemUpdated', 'IdleStateChanged', 'CursorMoved', 'SelectionChanged')",
            params![short_cutoff],
        ) {
            Ok(c) => c,
            Err(e) => {
                let _ = conn.execute("ROLLBACK", []);
                return Err(e.into());
            }
        };

        let deleted_long = match conn.execute(
            "DELETE FROM events WHERE timestamp < ?1",
            params![long_cutoff],
        ) {
            Ok(c) => c,
            Err(e) => {
                let _ = conn.execute("ROLLBACK", []);
                return Err(e.into());
            }
        };

        let total_deleted = deleted_bg + deleted_long;

        if total_deleted > 10000 {
            tracing::info!("Executing VACUUM after large prune ({} rows deleted)", total_deleted);
            let _ = conn.execute("VACUUM", []);
        }

        conn.execute("COMMIT", [])?;
        Ok(total_deleted as u64)
    }

    /// Archive events to `~/.mnse/archive/events-YYYY-MM.jsonl`
    fn archive_events_before(&self, conn: &Connection, cutoff_ts: i64, bg_only: bool) -> Result<u64> {
        use std::io::Write;

        let archive_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".mnse/archive");
        std::fs::create_dir_all(&archive_dir)?;

        let month_str = DateTime::from_timestamp(cutoff_ts, 0)
            .map(|dt| dt.format("%Y-%m").to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let archive_path = archive_dir.join(format!("events-{}.jsonl", month_str));

        let sql = if bg_only {
            "SELECT id, timestamp, source, event_type_name, event_type, observed_by, parent_event_id, payload \
             FROM events WHERE timestamp < ?1 AND event_type_name IN \
             ('ResourcesUpdated', 'AdapterHeartbeat', 'RunningAppsChanged', 'TabsUpdated', \
              'SystemUpdated', 'IdleStateChanged', 'CursorMoved', 'SelectionChanged') \
             ORDER BY timestamp ASC"
        } else {
            "SELECT id, timestamp, source, event_type_name, event_type, observed_by, parent_event_id, payload \
             FROM events WHERE timestamp < ?1 ORDER BY timestamp ASC"
        };

        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map([cutoff_ts], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "timestamp": row.get::<_, i64>(1)?,
                "source": row.get::<_, String>(2)?,
                "event_type_name": row.get::<_, String>(3)?,
                "event_type": row.get::<_, String>(4)?,
                "observed_by": row.get::<_, String>(5)?,
                "parent_event_id": row.get::<_, Option<String>>(6)?,
                "payload": row.get::<_, Option<String>>(7)?
            }))
        })?;

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&archive_path)?;

        let mut count = 0u64;
        for row in rows {
            let val = row?;
            writeln!(file, "{}", val)?;
            count += 1;
        }

        Ok(count)
    }

    /// Map SQLite row to Event struct
    fn row_to_event(&self, row: &rusqlite::Row) -> rusqlite::Result<Event> {
        let id_str: String = row.get(0)?;
        let id = Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4());

        let timestamp_sec: i64 = row.get(1)?;
        let timestamp = DateTime::from_timestamp(timestamp_sec, 0).unwrap_or_else(Utc::now);

        let source_str: String = row.get(2)?;
        let _type_name: String = row.get(3)?;
        let event_type_str: String = row.get(4)?;
        let observed_by: String = row.get(5)?;
        let parent_str: Option<String> = row.get(6)?;
        let payload_str: Option<String> = row.get(7)?;

        let event_type: EventType = serde_json::from_str(&event_type_str)
            .unwrap_or_else(|_| EventType::Custom {
                name: "unknown".into(),
                payload: serde_json::Value::Null,
            });

        let source = serde_json::from_str::<EventSource>(&format!("\"{}\"", source_str.to_lowercase()))
            .unwrap_or_else(|_| EventSource::from_event_type(&event_type));

        let parent_event_id = parent_str.and_then(|s| Uuid::parse_str(&s).ok());
        let payload = payload_str.and_then(|s| serde_json::from_str(&s).ok());

        Ok(Event {
            id,
            event_type,
            observed_by,
            source,
            parent_event_id,
            timestamp,
            kernel_received_at: None,
            payload,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_test_event(cmd: &str) -> Event {
        Event::new(
            EventType::ProcessExecuted {
                command: cmd.into(),
                exit_code: 0,
                duration_ms: 50,
                working_dir: "/tmp".into(),
                stdout_tail: None,
                stderr_tail: None,
            },
            "shell",
        )
    }

    #[test]
    fn test_open_and_schema_init() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("ledger.db");
        let db = LedgerDb::open(&path);
        assert!(db.is_ok());
    }

    #[test]
    fn test_insert_and_query_event() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("ledger.db");
        let db = LedgerDb::open(&path).unwrap();

        let event = create_test_event("cargo test");
        let inserted = db.insert_event(&event).unwrap();
        assert!(inserted);

        let count = db.count_events().unwrap();
        assert_eq!(count, 1);

        let found = db.query_event_by_id(&event.id.to_string()).unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, event.id);
    }

    #[test]
    fn test_noise_events_are_skipped() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("ledger.db");
        let db = LedgerDb::open(&path).unwrap();

        let noise_event = Event::new(
            EventType::CursorMoved { line: 10, col: 5 },
            "editor",
        );
        let inserted = db.insert_event(&noise_event).unwrap();
        assert!(!inserted); // returns false

        let count = db.count_events().unwrap();
        assert_eq!(count, 0); // never written to SQLite
    }

    #[test]
    fn test_fts_search() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("ledger.db");
        let db = LedgerDb::open(&path).unwrap();

        let e1 = create_test_event("git commit -m 'feat: v2 core'");
        let e2 = create_test_event("npm run dev");
        db.insert_event(&e1).unwrap();
        db.insert_event(&e2).unwrap();

        let filter = EventFilter::new().with_search("commit");
        let results = db.query_events(filter).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, e1.id);
    }

    #[test]
    fn test_batch_insert() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("ledger.db");
        let db = LedgerDb::open(&path).unwrap();

        let events = vec![
            create_test_event("cmd1"),
            create_test_event("cmd2"),
            create_test_event("cmd3"),
        ];

        let count = db.insert_events_batch(&events).unwrap();
        assert_eq!(count, 3);
        assert_eq!(db.count_events().unwrap(), 3);
    }

    #[test]
    fn test_prune() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("ledger.db");
        let db = LedgerDb::open(&path).unwrap();

        let mut event = create_test_event("old command");
        // set timestamp to 40 days ago
        event.timestamp = Utc::now() - chrono::Duration::days(40);
        db.insert_event(&event).unwrap();

        assert_eq!(db.count_events().unwrap(), 1);
        let deleted = db.prune_events_before(30).unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(db.count_events().unwrap(), 0);
    }
}
