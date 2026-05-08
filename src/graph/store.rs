use crate::graph::model::*;
use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::Path;

/// Apply read-leaning performance PRAGMAs to a freshly opened connection.
///
/// WAL keeps readers off the writer's lock; NORMAL fsync drops the per-write
/// stall (durability still survives crashes — only power-loss can lose the
/// last commit, which is fine for a derived index). 64 MB page cache + 256 MB
/// mmap fits the four ~100 MB stores comfortably on a dev box and turns hot
/// queries into pointer chases instead of pread syscalls.
pub(crate) fn apply_perf_pragmas(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "cache_size", -65536)?;
    conn.pragma_update(None, "mmap_size", 268_435_456i64)?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    Ok(())
}

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
        apply_perf_pragmas(&conn)?;
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

    pub fn count_edges(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM edges", [], |r| r.get(0))?)
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

    /// Hydrate a batch of nodes by id in a single query. Returns a map from
    /// id → Node; missing ids are silently absent. Replaces N+1 `get_node`
    /// loops in callers like `find_context::collect_cross_repo_edges`.
    pub fn get_nodes_by_ids(
        &self,
        ids: &[String],
    ) -> Result<std::collections::HashMap<String, Node>> {
        if ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let placeholders: String = (1..=ids.len())
            .map(|i| format!("?{}", i))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT id, kind, repo, path, symbol, summary, visibility, native_kind, loc_start, loc_end, deprecated
             FROM nodes WHERE id IN ({placeholders})"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let params_dyn: Vec<&dyn rusqlite::ToSql> =
            ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        let rows = stmt
            .query_map(params_dyn.as_slice(), row_to_node)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows.into_iter().map(|n| (n.id.clone(), n)).collect())
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

    /// Find IdlMethod nodes whose symbol contains `name`.
    /// If `service` is given, further restrict to symbols that start with `service`.
    pub fn find_idl_methods_by_name(
        &self,
        name: &str,
        service: Option<&str>,
    ) -> Result<Vec<Node>> {
        let escaped = name
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let pattern = format!("%{}%", escaped);
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, repo, path, symbol, summary, visibility, native_kind, loc_start, loc_end, deprecated
             FROM nodes
             WHERE kind = 'idlmethod' AND symbol LIKE ?1 ESCAPE '\\'",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![pattern], row_to_node)?
            .collect::<Result<Vec<_>, _>>()?;

        // If service filter given, narrow to symbols that start with the service prefix
        if let Some(svc) = service {
            let prefix = format!("{}.", svc);
            Ok(rows
                .into_iter()
                .filter(|n| {
                    n.symbol
                        .as_deref()
                        .map(|s| s.starts_with(&prefix) || s.starts_with(svc))
                        .unwrap_or(false)
                })
                .collect())
        } else {
            Ok(rows)
        }
    }

    /// Return every node anchored at a specific (repo, path).
    /// Used by find_context to map search-index hits back to graph nodes.
    pub fn nodes_at_path(&self, repo: &str, path: &str) -> Result<Vec<Node>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, repo, path, symbol, summary, visibility, native_kind, loc_start, loc_end, deprecated
             FROM nodes WHERE repo = ?1 AND path = ?2",
        )?;
        let rows = stmt
            .query_map(params![repo, path], row_to_node)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn search_symbols_substring(&self, q: &str, limit: usize) -> Result<Vec<Node>> {
        self.search_symbols_substring_filtered(q, None, limit)
    }

    /// Same as [`search_symbols_substring`] but optionally restricts matches
    /// to a single repo. Required by `repolayer query --repo <name>`.
    pub fn search_symbols_substring_filtered(
        &self,
        q: &str,
        repo_filter: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Node>> {
        // kind whitelist for `query`: user-written declarations *plus* IDL
        // service / method nodes. The latter were originally excluded so that
        // common short IDL method names (e.g. `Get`, `List`, defined dozens
        // of times across services) wouldn't crowd out business code matches.
        // Reality from a real session trace: the user almost always has to
        // grep IDL files anyway when tracing an API endpoint, so excluding
        // IDL nodes saved ~3 noise rows but cost 5 separate grep round-trips.
        // We now include them; agents that don't care can filter on `kind`.
        const KIND_WHITELIST: &str =
            "('type', 'method', 'function', 'symbol', 'idlmethod', 'idlservice')";
        let escaped = q
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let pattern = format!("%{}%", escaped);
        let rows: Vec<Node> = match repo_filter {
            None => {
                let sql = format!(
                    "SELECT id, kind, repo, path, symbol, summary, visibility, native_kind, loc_start, loc_end, deprecated
                     FROM nodes WHERE kind IN {KIND_WHITELIST}
                       AND (symbol LIKE ?1 ESCAPE '\\' OR path LIKE ?1 ESCAPE '\\')
                     LIMIT ?2"
                );
                let mut stmt = self.conn.prepare(&sql)?;
                let mapped = stmt.query_map(params![pattern, limit as i64], row_to_node)?;
                mapped.collect::<Result<Vec<_>, _>>()?
            }
            Some(repo) => {
                let sql = format!(
                    "SELECT id, kind, repo, path, symbol, summary, visibility, native_kind, loc_start, loc_end, deprecated
                     FROM nodes WHERE kind IN {KIND_WHITELIST}
                       AND repo = ?1
                       AND (symbol LIKE ?2 ESCAPE '\\' OR path LIKE ?2 ESCAPE '\\')
                     LIMIT ?3"
                );
                let mut stmt = self.conn.prepare(&sql)?;
                let mapped =
                    stmt.query_map(params![repo, pattern, limit as i64], row_to_node)?;
                mapped.collect::<Result<Vec<_>, _>>()?
            }
        };
        Ok(rows)
    }

    /// Distinct repo names recorded in the nodes table. Used to suggest
    /// alternatives when a `--repo` argument doesn't match any known repo.
    pub fn list_repo_names(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT repo FROM nodes ORDER BY repo")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let v: rusqlite::Result<Vec<_>> = rows.collect();
        Ok(v?)
    }
}
