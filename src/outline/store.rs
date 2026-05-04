//! SQLite-backed store for per-file Declaration trees.
//!
//! Schema v1: one row per (repo, path), holds language, parse error
//! count, content hash, and JSON-serialised Declarations.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::Path;

use crate::core::declaration::{Declaration, ParseResult};

const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS files (
    repo            TEXT NOT NULL,
    path            TEXT NOT NULL,
    language        TEXT NOT NULL,
    line_count      INTEGER NOT NULL,
    parse_errors    INTEGER NOT NULL DEFAULT 0,
    declarations    TEXT NOT NULL,
    content_hash    BLOB NOT NULL,
    PRIMARY KEY (repo, path)
);
CREATE INDEX IF NOT EXISTS idx_outline_files_repo ON files(repo);
"#;

pub struct OutlineStore {
    conn: Connection,
}

#[derive(Debug, Clone)]
pub struct OutlineEntry {
    pub repo: String,
    pub path: String,
    pub language: String,
    pub line_count: usize,
    pub parse_errors: usize,
    pub declarations: Vec<Declaration>,
    pub content_hash: Vec<u8>,
}

impl OutlineStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path)
            .with_context(|| format!("opening outline.db at {}", path.display()))?;
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

    pub fn upsert(&self, repo: &str, pr: &ParseResult, content_hash: &[u8]) -> Result<()> {
        let path = pr.path.to_string_lossy().to_string();
        let decls = serde_json::to_string(&pr.declarations)?;
        self.conn.execute(
            "INSERT INTO files(repo, path, language, line_count, parse_errors, declarations, content_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(repo, path) DO UPDATE SET
                language = excluded.language,
                line_count = excluded.line_count,
                parse_errors = excluded.parse_errors,
                declarations = excluded.declarations,
                content_hash = excluded.content_hash",
            params![
                repo,
                path,
                pr.language,
                pr.line_count as i64,
                pr.error_count as i64,
                decls,
                content_hash,
            ],
        )?;
        Ok(())
    }

    pub fn get(&self, repo: &str, path: &str) -> Result<Option<OutlineEntry>> {
        let res = self.conn.query_row(
            "SELECT language, line_count, parse_errors, declarations, content_hash
             FROM files WHERE repo = ?1 AND path = ?2",
            params![repo, path],
            |row| {
                let language: String = row.get(0)?;
                let line_count: i64 = row.get(1)?;
                let parse_errors: i64 = row.get(2)?;
                let decls_json: String = row.get(3)?;
                let content_hash: Vec<u8> = row.get(4)?;
                Ok((language, line_count, parse_errors, decls_json, content_hash))
            },
        );
        match res {
            Ok((language, line_count, parse_errors, decls_json, content_hash)) => {
                let declarations: Vec<Declaration> = serde_json::from_str(&decls_json)
                    .map_err(|e| anyhow::anyhow!("declarations decode: {}", e))?;
                Ok(Some(OutlineEntry {
                    repo: repo.into(),
                    path: path.into(),
                    language,
                    line_count: line_count as usize,
                    parse_errors: parse_errors as usize,
                    declarations,
                    content_hash,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn delete(&self, repo: &str, path: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM files WHERE repo = ?1 AND path = ?2",
            params![repo, path],
        )?;
        Ok(())
    }

    pub fn list_files(&self, repo: &str) -> Result<Vec<(String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT repo, path FROM files WHERE repo = ?1")?;
        let rows = stmt
            .query_map(params![repo], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}
