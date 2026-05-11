//! SQLite-backed store for file-level dependency graphs.
//!
//! Schema v1: one row per (repo, from_path, to_path, edge_kind) tuple.
//! Loaded into an in-memory DepGraph for traversal queries.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

use crate::deps::graph::{DepEdge, DepGraph, ImportKind};

const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS forward_edges (
    repo        TEXT NOT NULL,
    from_path   TEXT NOT NULL,
    to_path     TEXT NOT NULL,
    edge_kind   TEXT NOT NULL,
    line        INTEGER,
    local_name  TEXT,
    raw_path    TEXT,
    PRIMARY KEY (repo, from_path, to_path, edge_kind)
);
CREATE INDEX IF NOT EXISTS idx_deps_reverse ON forward_edges(repo, to_path);

CREATE TABLE IF NOT EXISTS external_imports (
    repo       TEXT NOT NULL,
    from_path  TEXT NOT NULL,
    raw        TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS file_records (
    repo         TEXT NOT NULL,
    path         TEXT NOT NULL,
    mtime_ns     INTEGER,
    size         INTEGER,
    content_hash BLOB,
    PRIMARY KEY (repo, path)
);
"#;

pub struct DepStore {
    conn: Connection,
}

impl DepStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path)
            .with_context(|| format!("opening deps.db at {}", path.display()))?;
        crate::graph::store::apply_perf_pragmas(&conn)?;
        conn.execute_batch(SCHEMA_V1)?;
        conn.execute(
            "INSERT OR IGNORE INTO meta(key, value) VALUES ('schema_version', '1')",
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

    pub fn replace_repo_graph(&self, repo: &str, g: &DepGraph) -> Result<()> {
        self.conn
            .execute("DELETE FROM forward_edges WHERE repo = ?1", params![repo])?;
        self.conn.execute(
            "DELETE FROM external_imports WHERE repo = ?1",
            params![repo],
        )?;

        for (from_path, edges) in &g.forward {
            for e in edges {
                let kind_str = import_kind_to_str(e.kind);
                self.conn.execute(
                    "INSERT OR REPLACE INTO forward_edges(repo, from_path, to_path, edge_kind, line, local_name, raw_path)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        repo,
                        from_path.to_string_lossy().as_ref(),
                        e.target.to_string_lossy().as_ref(),
                        kind_str,
                        e.line as i64,
                        e.local_name,
                        e.raw_path,
                    ],
                )?;
            }
        }
        for (from_path, externals) in &g.external {
            for raw in externals {
                self.conn.execute(
                    "INSERT INTO external_imports(repo, from_path, raw) VALUES (?1, ?2, ?3)",
                    params![repo, from_path.to_string_lossy().as_ref(), raw],
                )?;
            }
        }
        Ok(())
    }

    pub fn load_repo_graph(&self, repo: &str, root: PathBuf) -> Result<DepGraph> {
        let mut g = DepGraph::empty(root);
        let mut stmt = self.conn.prepare(
            "SELECT from_path, to_path, edge_kind, line, local_name, raw_path
             FROM forward_edges WHERE repo = ?1",
        )?;
        let rows = stmt.query_map(params![repo], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<i64>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })?;
        for r in rows {
            let (from_str, to_str, kind_str, line, local_name, raw_path) = r?;
            let kind = parse_import_kind(&kind_str)
                .ok_or_else(|| anyhow::anyhow!("bad ImportKind: {}", kind_str))?;
            let edge = DepEdge {
                target: PathBuf::from(to_str),
                kind,
                line: line.unwrap_or(0) as u32,
                local_name,
                raw_path,
            };
            g.forward
                .entry(PathBuf::from(from_str))
                .or_default()
                .push(edge);
        }
        // external imports
        let mut stmt = self
            .conn
            .prepare("SELECT from_path, raw FROM external_imports WHERE repo = ?1")?;
        let rows = stmt.query_map(params![repo], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for r in rows {
            let (from_str, raw) = r?;
            g.external
                .entry(PathBuf::from(from_str))
                .or_default()
                .push(raw);
        }
        Ok(g)
    }

    /// Return every external_imports row as (repo, from_path, raw).
    /// Used by the import-based cross-repo linker.
    pub fn list_external_imports(&self) -> Result<Vec<(String, String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT repo, from_path, raw FROM external_imports")?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn delete_file(&self, repo: &str, path: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM forward_edges WHERE repo = ?1 AND from_path = ?2",
            params![repo, path],
        )?;
        self.conn.execute(
            "DELETE FROM external_imports WHERE repo = ?1 AND from_path = ?2",
            params![repo, path],
        )?;
        Ok(())
    }

    /// Replace one file's forward edges + externals atomically.
    /// `from_path` is the source file path stored as TEXT (caller normalizes).
    /// `edges` are resolved internal targets; `externals` are unresolved
    /// import specs (used by the cross-repo Imports linker).
    pub fn upsert_file_edges(
        &self,
        repo: &str,
        from_path: &str,
        edges: &[DepEdge],
        externals: &[String],
    ) -> Result<()> {
        self.conn.execute(
            "DELETE FROM forward_edges WHERE repo = ?1 AND from_path = ?2",
            params![repo, from_path],
        )?;
        self.conn.execute(
            "DELETE FROM external_imports WHERE repo = ?1 AND from_path = ?2",
            params![repo, from_path],
        )?;
        for e in edges {
            let kind_str = import_kind_to_str(e.kind);
            self.conn.execute(
                "INSERT OR REPLACE INTO forward_edges(repo, from_path, to_path, edge_kind, line, local_name, raw_path)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    repo,
                    from_path,
                    e.target.to_string_lossy().as_ref(),
                    kind_str,
                    e.line as i64,
                    e.local_name,
                    e.raw_path,
                ],
            )?;
        }
        for raw in externals {
            self.conn.execute(
                "INSERT INTO external_imports(repo, from_path, raw) VALUES (?1, ?2, ?3)",
                params![repo, from_path, raw],
            )?;
        }
        Ok(())
    }
}

/// Serialize `ImportKind` to a stable lowercase string tag.
fn import_kind_to_str(kind: ImportKind) -> &'static str {
    match kind {
        ImportKind::Use => "use",
        ImportKind::Mod => "mod",
        ImportKind::From => "from",
        ImportKind::Bare => "bare",
        ImportKind::NamedFrom => "namedfrom",
        ImportKind::StarFrom => "starfrom",
        ImportKind::Static => "static",
        ImportKind::Alias => "alias",
        ImportKind::Glob => "glob",
    }
}

/// Deserialize `ImportKind` from the stored lowercase string tag.
fn parse_import_kind(s: &str) -> Option<ImportKind> {
    match s {
        "use" => Some(ImportKind::Use),
        "mod" => Some(ImportKind::Mod),
        "from" => Some(ImportKind::From),
        "bare" => Some(ImportKind::Bare),
        "namedfrom" => Some(ImportKind::NamedFrom),
        "starfrom" => Some(ImportKind::StarFrom),
        "static" => Some(ImportKind::Static),
        "alias" => Some(ImportKind::Alias),
        "glob" => Some(ImportKind::Glob),
        _ => None,
    }
}
