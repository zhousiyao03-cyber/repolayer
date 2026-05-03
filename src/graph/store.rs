use crate::graph::model::*;
use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::Path;

pub struct Store {
    conn: Connection,
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS nodes (
    id          TEXT PRIMARY KEY,
    kind        TEXT NOT NULL,
    repo        TEXT NOT NULL,
    path        TEXT NOT NULL,
    symbol      TEXT,
    summary     TEXT,
    owner       TEXT,
    loc_start   INTEGER,
    loc_end     INTEGER
);
CREATE INDEX IF NOT EXISTS idx_nodes_repo ON nodes(repo);
CREATE INDEX IF NOT EXISTS idx_nodes_kind ON nodes(kind);
CREATE INDEX IF NOT EXISTS idx_nodes_symbol ON nodes(symbol) WHERE symbol IS NOT NULL;

CREATE TABLE IF NOT EXISTS edges (
    from_id  TEXT NOT NULL,
    to_id    TEXT NOT NULL,
    kind     TEXT NOT NULL,
    PRIMARY KEY (from_id, to_id, kind)
);
CREATE INDEX IF NOT EXISTS idx_edges_from ON edges(from_id, kind);
CREATE INDEX IF NOT EXISTS idx_edges_to   ON edges(to_id, kind);

CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating dir {}", parent.display()))?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("opening sqlite at {}", path.display()))?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    pub fn count_nodes(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))?)
    }

    pub fn upsert_node(&self, n: &Node) -> Result<()> {
        let kind = serde_json::to_value(n.kind)?
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("kind serialization failed"))?
            .to_string();
        self.conn.execute(
            "INSERT INTO nodes (id, kind, repo, path, symbol, summary, owner, loc_start, loc_end)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
                kind=excluded.kind, repo=excluded.repo, path=excluded.path,
                symbol=excluded.symbol, summary=excluded.summary, owner=excluded.owner,
                loc_start=excluded.loc_start, loc_end=excluded.loc_end",
            params![
                n.id,
                kind,
                n.repo,
                n.path,
                n.symbol,
                n.summary,
                n.owner,
                n.loc_start,
                n.loc_end,
            ],
        )?;
        Ok(())
    }

    pub fn get_node(&self, id: &str) -> Result<Option<Node>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, repo, path, symbol, summary, owner, loc_start, loc_end
             FROM nodes WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            let kind_str: String = row.get(1)?;
            let kind: NodeKind = serde_json::from_value(serde_json::Value::String(kind_str))?;
            Ok(Some(Node {
                id: row.get(0)?,
                kind,
                repo: row.get(2)?,
                path: row.get(3)?,
                symbol: row.get(4)?,
                summary: row.get(5)?,
                owner: row.get(6)?,
                loc_start: row.get(7)?,
                loc_end: row.get(8)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn upsert_edge(&self, e: &Edge) -> Result<()> {
        let kind = serde_json::to_value(e.kind)?
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("edge kind serialization failed"))?
            .to_string();
        self.conn.execute(
            "INSERT INTO edges (from_id, to_id, kind) VALUES (?1, ?2, ?3)
             ON CONFLICT(from_id, to_id, kind) DO NOTHING",
            params![e.from, e.to, kind],
        )?;
        Ok(())
    }

    pub fn outgoing_edges(&self, from: &str, kind: EdgeKind) -> Result<Vec<Edge>> {
        let kind_str = serde_json::to_value(kind)?
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("edge kind serialization failed"))?
            .to_string();
        let mut stmt = self
            .conn
            .prepare("SELECT from_id, to_id, kind FROM edges WHERE from_id = ?1 AND kind = ?2")?;
        let edges = stmt
            .query_map(params![from, kind_str], |row| {
                let k: String = row.get(2)?;
                let kind: EdgeKind =
                    serde_json::from_value(serde_json::Value::String(k)).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            2,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                Ok(Edge {
                    from: row.get(0)?,
                    to: row.get(1)?,
                    kind,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(edges)
    }

    pub fn incoming_edges(&self, to: &str, kind: EdgeKind) -> Result<Vec<Edge>> {
        let kind_str = serde_json::to_value(kind)?
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("edge kind serialization failed"))?
            .to_string();
        let mut stmt = self
            .conn
            .prepare("SELECT from_id, to_id, kind FROM edges WHERE to_id = ?1 AND kind = ?2")?;
        let edges = stmt
            .query_map(params![to, kind_str], |row| {
                let k: String = row.get(2)?;
                let kind: EdgeKind =
                    serde_json::from_value(serde_json::Value::String(k)).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            2,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                Ok(Edge {
                    from: row.get(0)?,
                    to: row.get(1)?,
                    kind,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(edges)
    }
}
