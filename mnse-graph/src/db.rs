//! SQLite-backed graph database for storing nodes and edges (Historian Model).

use anyhow::Result;
use chrono::Utc;
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Mutex;

/// GraphDb manages nodes and edges tables in SQLite
pub struct GraphDb {
    conn: Mutex<Connection>,
}

impl GraphDb {
    /// Safe lock acquisition - handles mutex poisoning gracefully
    fn acquire_lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.conn.lock().map_err(|e| {
            tracing::error!("Graph database lock poisoned: {}", e);
            anyhow::anyhow!("graph database lock poisoned")
        })
    }

    /// Open or create graph database
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_millis(5000))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "cache_size", "-32000")?;
        conn.pragma_update(None, "temp_store", "MEMORY")?;

        let db = Self {
            conn: Mutex::new(conn),
        };
        db.migrate()?;
        Ok(db)
    }

    /// Initialize graph schema
    fn migrate(&self) -> Result<()> {
        let conn = self.acquire_lock()?;

        // 1. Nodes Table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS nodes (
                id TEXT PRIMARY KEY,
                type TEXT NOT NULL,
                label TEXT NOT NULL,
                properties TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_nodes_type ON nodes(type)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_nodes_updated ON nodes(updated_at DESC)",
            [],
        )?;

        // 2. Edges Table (Weighted, Fact-only relations)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS edges (
                source_id TEXT NOT NULL,
                target_id TEXT NOT NULL,
                relation TEXT NOT NULL,
                weight INTEGER DEFAULT 1,
                confidence REAL DEFAULT 1.0,
                properties TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY (source_id, target_id, relation),
                FOREIGN KEY(source_id) REFERENCES nodes(id) ON DELETE CASCADE,
                FOREIGN KEY(target_id) REFERENCES nodes(id) ON DELETE CASCADE
            )",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target_id)",
            [],
        )?;

        Ok(())
    }

    /// Upsert node into the graph
    pub fn upsert_node(
        &self,
        id: &str,
        node_type: &str,
        label: &str,
        properties: Option<serde_json::Value>,
    ) -> Result<()> {
        let conn = self.acquire_lock()?;
        let now = Utc::now().timestamp();
        let props_str = properties
            .map(|p| p.to_string())
            .unwrap_or_else(|| "{}".to_string());

        conn.execute(
            "INSERT INTO nodes (id, type, label, properties, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)
             ON CONFLICT(id) DO UPDATE SET
                updated_at = ?5,
                properties = json_patch(COALESCE(properties, '{}'), ?4),
                label = ?3",
            params![id, node_type, label, props_str, now],
        )?;

        Ok(())
    }

    /// Upsert edge into the graph
    pub fn upsert_edge(
        &self,
        source_id: &str,
        target_id: &str,
        relation: &str,
        weight_increment: i32,
        confidence: f64,
    ) -> Result<()> {
        let conn = self.acquire_lock()?;
        let now = Utc::now().timestamp();

        conn.execute(
            "INSERT INTO edges (source_id, target_id, relation, weight, confidence, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT(source_id, target_id, relation) DO UPDATE SET
                weight = weight + ?4,
                confidence = ?5,
                updated_at = ?6",
            params![source_id, target_id, relation, weight_increment, confidence, now],
        )?;

        Ok(())
    }

    /// Get single node by ID
    pub fn get_node(&self, id: &str) -> Result<Option<serde_json::Value>> {
        let conn = self.acquire_lock()?;
        let mut stmt = conn.prepare(
            "SELECT id, type, label, properties, created_at, updated_at FROM nodes WHERE id = ?1",
        )?;

        let mut rows = stmt.query([id])?;
        if let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let type_name: String = row.get(1)?;
            let label: String = row.get(2)?;
            let props_str: Option<String> = row.get(3)?;
            let created_at: i64 = row.get(4)?;
            let updated_at: i64 = row.get(5)?;

            let properties = props_str
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_else(|| serde_json::json!({}));

            Ok(Some(serde_json::json!({
                "id": id,
                "type": type_name,
                "label": label,
                "properties": properties,
                "created_at": created_at,
                "updated_at": updated_at
            })))
        } else {
            Ok(None)
        }
    }

    /// Get outgoing related nodes for a node
    pub fn get_related(&self, id: &str, relation: Option<&str>) -> Result<Vec<serde_json::Value>> {
        let conn = self.acquire_lock()?;
        let mut results = Vec::new();

        if let Some(rel) = relation {
            let mut stmt = conn.prepare(
                "SELECT target_id, relation, weight, confidence FROM edges WHERE source_id = ?1 AND relation = ?2",
            )?;
            let rows = stmt.query_map(params![id, rel], |row| {
                Ok(serde_json::json!({
                    "target_id": row.get::<_, String>(0)?,
                    "relation": row.get::<_, String>(1)?,
                    "weight": row.get::<_, i32>(2)?,
                    "confidence": row.get::<_, f64>(3)?
                }))
            })?;
            for r in rows {
                results.push(r?);
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT target_id, relation, weight, confidence FROM edges WHERE source_id = ?1",
            )?;
            let rows = stmt.query_map(params![id], |row| {
                Ok(serde_json::json!({
                    "target_id": row.get::<_, String>(0)?,
                    "relation": row.get::<_, String>(1)?,
                    "weight": row.get::<_, i32>(2)?,
                    "confidence": row.get::<_, f64>(3)?
                }))
            })?;
            for r in rows {
                results.push(r?);
            }
        }

        Ok(results)
    }

    /// Get incoming edges for a target node
    pub fn get_incoming(&self, id: &str, relation: Option<&str>) -> Result<Vec<serde_json::Value>> {
        let conn = self.acquire_lock()?;
        let mut results = Vec::new();

        if let Some(rel) = relation {
            let mut stmt = conn.prepare(
                "SELECT source_id, relation, weight, confidence FROM edges WHERE target_id = ?1 AND relation = ?2",
            )?;
            let rows = stmt.query_map(params![id, rel], |row| {
                Ok(serde_json::json!({
                    "source_id": row.get::<_, String>(0)?,
                    "relation": row.get::<_, String>(1)?,
                    "weight": row.get::<_, i32>(2)?,
                    "confidence": row.get::<_, f64>(3)?
                }))
            })?;
            for r in rows {
                results.push(r?);
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT source_id, relation, weight, confidence FROM edges WHERE target_id = ?1",
            )?;
            let rows = stmt.query_map(params![id], |row| {
                Ok(serde_json::json!({
                    "source_id": row.get::<_, String>(0)?,
                    "relation": row.get::<_, String>(1)?,
                    "weight": row.get::<_, i32>(2)?,
                    "confidence": row.get::<_, f64>(3)?
                }))
            })?;
            for r in rows {
                results.push(r?);
            }
        }

        Ok(results)
    }

    /// Delete a node (cascade deletes connected edges)
    pub fn delete_node(&self, id: &str) -> Result<()> {
        let conn = self.acquire_lock()?;
        conn.execute("DELETE FROM nodes WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Search nodes by label or type
    pub fn search_nodes(&self, query: &str) -> Result<Vec<serde_json::Value>> {
        let conn = self.acquire_lock()?;
        let pattern = format!("%{}%", query);
        let mut stmt = conn.prepare(
            "SELECT id, type, label, properties 
             FROM nodes 
             WHERE label LIKE ?1 OR type LIKE ?1 OR properties LIKE ?1
             ORDER BY updated_at DESC LIMIT 20",
        )?;

        let rows = stmt.query_map(params![pattern], |row| {
            let id: String = row.get(0)?;
            let type_name: String = row.get(1)?;
            let label: String = row.get(2)?;
            let props_str: Option<String> = row.get(3)?;
            let props: Option<serde_json::Value> =
                props_str.and_then(|s| serde_json::from_str(&s).ok());

            Ok(serde_json::json!({
                "id": id,
                "type": type_name,
                "label": label,
                "properties": props
            }))
        })?;

        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }
        Ok(results)
    }

    /// Session -> Burst -> Event tree query
    pub fn get_sessions_in_range(&self, start_ts: i64, end_ts: i64) -> Result<Vec<serde_json::Value>> {
        let conn = self.acquire_lock()?;
        let mut stmt = conn.prepare(
            "SELECT id, label, properties FROM nodes \
             WHERE type = 'Session' \
               AND json_extract(properties, '$.start_time') BETWEEN ?1 AND ?2 \
             ORDER BY json_extract(properties, '$.start_time') DESC",
        )?;

        let session_rows = stmt.query_map(params![start_ts, end_ts], |row| {
            let id: String = row.get(0)?;
            let label: String = row.get(1)?;
            let props_str: Option<String> = row.get(2)?;
            let props: Option<serde_json::Value> = props_str.and_then(|s| serde_json::from_str(&s).ok());
            Ok((id, label, props))
        })?;

        let mut sessions = Vec::new();
        for s in session_rows {
            let (s_id, s_label, s_props) = s?;

            let mut burst_stmt = conn.prepare(
                "SELECT n.id, n.label, n.properties FROM nodes n \
                 JOIN edges e ON e.source_id = n.id \
                 WHERE e.target_id = ?1 AND e.relation = 'PART_OF' AND n.type = 'Burst' \
                 ORDER BY json_extract(n.properties, '$.start_time') ASC",
            )?;

            let burst_rows = burst_stmt.query_map(params![s_id], |row| {
                let id: String = row.get(0)?;
                let label: String = row.get(1)?;
                let props_str: Option<String> = row.get(2)?;
                let props: Option<serde_json::Value> = props_str.and_then(|s| serde_json::from_str(&s).ok());
                Ok((id, label, props))
            })?;

            let mut bursts = Vec::new();
            for b in burst_rows {
                let (b_id, b_label, b_props) = b?;

                let mut event_stmt = conn.prepare(
                    "SELECT n.id, n.type, n.label, n.properties FROM nodes n \
                     JOIN edges e ON e.source_id = n.id \
                     WHERE e.target_id = ?1 AND e.relation = 'IN_BURST' \
                     ORDER BY json_extract(n.properties, '$.timestamp') ASC",
                )?;

                let event_rows = event_stmt.query_map(params![b_id], |row| {
                    let id: String = row.get(0)?;
                    let node_type: String = row.get(1)?;
                    let label: String = row.get(2)?;
                    let props_str: Option<String> = row.get(3)?;
                    let props: Option<serde_json::Value> = props_str.and_then(|s| serde_json::from_str(&s).ok());

                    Ok(serde_json::json!({
                        "id": id,
                        "type": node_type,
                        "label": label,
                        "properties": props
                    }))
                })?;

                let mut events = Vec::new();
                for ev in event_rows {
                    events.push(ev?);
                }

                bursts.push(serde_json::json!({
                    "id": b_id,
                    "label": b_label,
                    "properties": b_props,
                    "children": events
                }));
            }

            sessions.push(serde_json::json!({
                "id": s_id,
                "label": s_label,
                "properties": s_props,
                "children": bursts
            }));
        }

        Ok(sessions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_graph_node_upsert_and_get() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let db = GraphDb::new(&path).unwrap();

        db.upsert_node("file:///main.rs", "File", "main.rs", Some(serde_json::json!({"lang": "rust"})))
            .unwrap();

        let node = db.get_node("file:///main.rs").unwrap().unwrap();
        assert_eq!(node["label"], "main.rs");
        assert_eq!(node["type"], "File");
        assert_eq!(node["properties"]["lang"], "rust");
    }

    #[test]
    fn test_edge_upsert_and_traversal() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let db = GraphDb::new(&path).unwrap();

        db.upsert_node("cmd:123", "Command", "cargo build", None).unwrap();
        db.upsert_node("file:///main.rs", "File", "main.rs", None).unwrap();

        db.upsert_edge("cmd:123", "file:///main.rs", "TOUCHED", 1, 1.0).unwrap();

        let related = db.get_related("cmd:123", Some("TOUCHED")).unwrap();
        assert_eq!(related.len(), 1);
        assert_eq!(related[0]["target_id"], "file:///main.rs");

        let incoming = db.get_incoming("file:///main.rs", None).unwrap();
        assert_eq!(incoming.len(), 1);
        assert_eq!(incoming[0]["source_id"], "cmd:123");
    }
}
