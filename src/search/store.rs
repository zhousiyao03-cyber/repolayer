//! SQLite-backed store for the hybrid BM25 + dense embedding search index.
//!
//! Schema v1: chunks table (with repo column), meta table.
//! One database file per workspace.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::Path;

use crate::search::chunker::Chunk;

/// Row returned by [`SearchStore::list_chunks`]: `(id, path, start_line, end_line, content)`.
pub type ChunkRow = (i64, String, u32, u32, String);

const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS chunks (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    repo        TEXT NOT NULL,
    path        TEXT NOT NULL,
    start_line  INTEGER NOT NULL,
    end_line    INTEGER NOT NULL,
    content     TEXT NOT NULL,
    chunk_hash  BLOB NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_chunks_repo_path ON chunks(repo, path);
CREATE INDEX IF NOT EXISTS idx_chunks_repo ON chunks(repo);
"#;

pub struct SearchStore {
    conn: Connection,
}

impl SearchStore {
    /// Open (or create) a search.db at the given path.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path)
            .with_context(|| format!("opening search.db at {}", path.display()))?;
        conn.execute_batch(SCHEMA_V1)?;
        conn.execute(
            "INSERT OR IGNORE INTO meta(key, value) VALUES ('schema_version', '1')",
            [],
        )?;
        Ok(Self { conn })
    }

    /// Return the schema version stored in the meta table.
    pub fn schema_version(&self) -> Result<u32> {
        let v: String = self.conn.query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )?;
        Ok(v.parse().unwrap_or(0))
    }

    /// Replace all chunks for a given repo with the supplied slice.
    /// Runs inside a single transaction: delete-all then insert-all.
    pub fn replace_repo_chunks(&self, repo: &str, chunks: &[Chunk]) -> Result<()> {
        self.conn
            .execute("DELETE FROM chunks WHERE repo = ?1", params![repo])?;

        for chunk in chunks {
            // Derive a stable hash from file path + start_line + end_line.
            let hash_bytes: Vec<u8> =
                format!("{}-{}-{}", chunk.file_path, chunk.start_line, chunk.end_line)
                    .into_bytes();
            self.conn.execute(
                "INSERT INTO chunks(repo, path, start_line, end_line, content, chunk_hash)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    repo,
                    chunk.file_path,
                    chunk.start_line as i64,
                    chunk.end_line as i64,
                    chunk.content,
                    hash_bytes,
                ],
            )?;
        }
        Ok(())
    }

    /// List all chunks for a given repo.
    /// Returns `(id, path, start_line, end_line, content)`.
    pub fn list_chunks(&self, repo: &str) -> Result<Vec<ChunkRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, path, start_line, end_line, content FROM chunks WHERE repo = ?1",
        )?;
        let rows = stmt.query_map(params![repo], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? as u32,
                row.get::<_, i64>(3)? as u32,
                row.get::<_, String>(4)?,
            ))
        })?;
        let v: Result<Vec<_>, _> = rows.collect();
        Ok(v?)
    }

    /// Delete all chunks for a specific file within a repo.
    pub fn delete_file(&self, repo: &str, path: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM chunks WHERE repo = ?1 AND path = ?2",
            params![repo, path],
        )?;
        Ok(())
    }
}
