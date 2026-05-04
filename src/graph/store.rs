use crate::graph::model::*;
use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::Path;

/// Single-threaded handle to the SQLite-backed graph store.
///
/// `rusqlite::Connection` is `Send` but `!Sync`, so this struct cannot be
/// shared across async tasks via `Arc<Store>` directly. For multi-threaded use
/// (e.g. the MCP server in `crate::mcp`), wrap in `Arc<tokio::sync::Mutex<Store>>`,
/// or open a fresh `Store` per request and let SQLite's file lock coordinate.
pub struct Store {
    conn: Connection,
}

const SCHEMA_V2: &str = r#"
CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS nodes (
    id          TEXT PRIMARY KEY,
    kind        TEXT NOT NULL,
    repo        TEXT NOT NULL,
    path        TEXT NOT NULL,
    symbol      TEXT,
    summary     TEXT,
    visibility  TEXT,
    native_kind TEXT,
    loc_start   INTEGER,
    loc_end     INTEGER,
    deprecated  INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_nodes_repo ON nodes(repo);
CREATE INDEX IF NOT EXISTS idx_nodes_kind ON nodes(kind);
CREATE INDEX IF NOT EXISTS idx_nodes_symbol ON nodes(symbol) WHERE symbol IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_nodes_repo_path ON nodes(repo, path);

CREATE TABLE IF NOT EXISTS edges (
    from_id    TEXT NOT NULL,
    to_id      TEXT NOT NULL,
    kind       TEXT NOT NULL,
    confidence REAL NOT NULL DEFAULT 1.0,
    PRIMARY KEY (from_id, to_id, kind)
);
CREATE INDEX IF NOT EXISTS idx_edges_from ON edges(from_id, kind);
CREATE INDEX IF NOT EXISTS idx_edges_to   ON edges(to_id, kind);
"#;

// === serde helpers for enum <-> SQL TEXT roundtrip ===

fn kind_to_db<T: serde::Serialize>(k: T) -> anyhow::Result<String> {
    let v = serde_json::to_value(k)?;
    v.as_str()
        .map(String::from)
        .ok_or_else(|| anyhow::anyhow!("enum serialization did not produce a string"))
}

fn node_kind_from_db(s: &str) -> rusqlite::Result<NodeKind> {
    serde_json::from_value(serde_json::Value::String(s.to_string())).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn edge_kind_from_db(s: &str) -> rusqlite::Result<EdgeKind> {
    serde_json::from_value(serde_json::Value::String(s.to_string())).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn row_to_node(row: &rusqlite::Row) -> rusqlite::Result<Node> {
    let kind_str: String = row.get(1)?;
    let kind = node_kind_from_db(&kind_str)?;
    Ok(Node {
        id: row.get(0)?,
        kind,
        repo: row.get(2)?,
        path: row.get(3)?,
        symbol: row.get(4)?,
        summary: row.get(5)?,
        visibility: row.get(6)?,
        native_kind: row.get(7)?,
        loc_start: row.get(8)?,
        loc_end: row.get(9)?,
        deprecated: row.get::<_, i64>(10)? != 0,
    })
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating dir {}", parent.display()))?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("opening sqlite at {}", path.display()))?;
        conn.execute_batch(SCHEMA_V2)?;
        conn.execute(
            "INSERT OR IGNORE INTO meta(key, value) VALUES ('schema_version', '2')",
            [],
        )?;
        Ok(Self { conn })
    }

    pub fn schema_version(&self) -> Result<u32> {
        let v: String = self.conn.query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )?;
        Ok(v.parse().unwrap_or(0))
    }

    pub fn count_nodes(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))?)
    }

    pub fn upsert_node(&self, n: &Node) -> Result<()> {
        let kind_str = kind_to_db(n.kind)?;
        self.conn.execute(
            "INSERT INTO nodes(id, kind, repo, path, symbol, summary, visibility, native_kind, loc_start, loc_end, deprecated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(id) DO UPDATE SET
                summary = COALESCE(excluded.summary, nodes.summary),
                visibility = COALESCE(excluded.visibility, nodes.visibility),
                native_kind = COALESCE(excluded.native_kind, nodes.native_kind),
                loc_start = COALESCE(excluded.loc_start, nodes.loc_start),
                loc_end = COALESCE(excluded.loc_end, nodes.loc_end),
                deprecated = excluded.deprecated",
            rusqlite::params![
                n.id, kind_str, n.repo, n.path, n.symbol, n.summary,
                n.visibility, n.native_kind, n.loc_start, n.loc_end,
                n.deprecated as i64,
            ],
        )?;
        Ok(())
    }

    pub fn get_node(&self, id: &str) -> Result<Option<Node>> {
        let res = self.conn.query_row(
            "SELECT id, kind, repo, path, symbol, summary, visibility, native_kind, loc_start, loc_end, deprecated
             FROM nodes WHERE id = ?1",
            rusqlite::params![id],
            row_to_node,
        );
        match res {
            Ok(n) => Ok(Some(n)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn upsert_edge(&self, e: &Edge) -> Result<()> {
        let kind_str = kind_to_db(e.kind)?;
        self.conn.execute(
            "INSERT INTO edges(from_id, to_id, kind, confidence)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(from_id, to_id, kind) DO UPDATE SET
                confidence = excluded.confidence",
            rusqlite::params![e.from, e.to, kind_str, e.confidence],
        )?;
        Ok(())
    }

    pub fn get_edges_from(&self, from_id: &str) -> Result<Vec<Edge>> {
        let mut stmt = self.conn.prepare(
            "SELECT from_id, to_id, kind, confidence FROM edges WHERE from_id = ?1",
        )?;
        let rows = stmt.query_map(rusqlite::params![from_id], |row| {
            let kind_str: String = row.get(2)?;
            let kind = edge_kind_from_db(&kind_str)?;
            Ok(Edge {
                from: row.get(0)?,
                to: row.get(1)?,
                kind,
                confidence: row.get(3)?,
            })
        })?;
        let edges: Result<Vec<_>, _> = rows.collect();
        Ok(edges?)
    }

    pub fn outgoing_edges(&self, from: &str, kind: EdgeKind) -> Result<Vec<Edge>> {
        let kind_str = kind_to_db(kind)?;
        let mut stmt = self
            .conn
            .prepare("SELECT from_id, to_id, kind, confidence FROM edges WHERE from_id = ?1 AND kind = ?2")?;
        let edges = stmt
            .query_map(params![from, kind_str], |row| {
                Ok(Edge {
                    from: row.get(0)?,
                    to: row.get(1)?,
                    kind: edge_kind_from_db(&row.get::<_, String>(2)?)?,
                    confidence: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(edges)
    }

    pub fn incoming_edges(&self, to: &str, kind: EdgeKind) -> Result<Vec<Edge>> {
        let kind_str = kind_to_db(kind)?;
        let mut stmt = self
            .conn
            .prepare("SELECT from_id, to_id, kind, confidence FROM edges WHERE to_id = ?1 AND kind = ?2")?;
        let edges = stmt
            .query_map(params![to, kind_str], |row| {
                Ok(Edge {
                    from: row.get(0)?,
                    to: row.get(1)?,
                    kind: edge_kind_from_db(&row.get::<_, String>(2)?)?,
                    confidence: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(edges)
    }

    pub fn list_nodes_by_kind(&self, kind: NodeKind) -> Result<Vec<Node>> {
        let kind_str = kind_to_db(kind)?;
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, repo, path, symbol, summary, visibility, native_kind, loc_start, loc_end, deprecated
             FROM nodes WHERE kind = ?1",
        )?;
        let rows = stmt
            .query_map(params![kind_str], row_to_node)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn delete_module(&self, repo: &str, path: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM nodes WHERE repo = ?1 AND path = ?2",
            params![repo, path],
        )?;
        // Clean up dangling edges that reference deleted nodes
        self.conn.execute(
            "DELETE FROM edges WHERE from_id NOT IN (SELECT id FROM nodes) OR to_id NOT IN (SELECT id FROM nodes)",
            [],
        )?;
        Ok(())
    }

    pub fn search_symbols_substring(&self, q: &str, limit: usize) -> Result<Vec<Node>> {
        let escaped = q
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let pattern = format!("%{}%", escaped);
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, repo, path, symbol, summary, visibility, native_kind, loc_start, loc_end, deprecated
             FROM nodes WHERE kind IN ('type', 'method', 'function', 'symbol')
               AND (symbol LIKE ?1 ESCAPE '\\' OR path LIKE ?1 ESCAPE '\\')
             LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![pattern, limit as i64], row_to_node)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}
