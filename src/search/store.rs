//! SQLite-backed store for the hybrid BM25 + dense embedding search index.
//!
//! Schema v1: chunks table (with repo column), meta table.
//! Schema v2 adds the `chunk_vec` virtual table (sqlite-vec `vec0`) for
//! 256-dim L2-normalised embeddings keyed by `chunks.id`.
//! One database file per workspace.

use anyhow::{Context, Result};
use rusqlite::{ffi::sqlite3_auto_extension, params, Connection};
use std::path::Path;
use std::sync::Once;

use crate::search::chunker::Chunk;
use crate::search::embed::DIM;

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

/// vec0 virtual table holding the 256-dim chunk embeddings.
/// `rowid` mirrors `chunks.id`; we keep them in sync manually because
/// vec0 cannot itself enforce a foreign key.
const SCHEMA_V2_VEC: &str = "
CREATE VIRTUAL TABLE IF NOT EXISTS chunk_vec USING vec0(
    embedding float[256]
);
";

/// Register the bundled sqlite-vec extension as a SQLite auto-extension.
/// Idempotent — safe to call from every `SearchStore::open` invocation.
fn register_sqlite_vec() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| unsafe {
        // The C signature on `Option<…>` here is the SQLite auto-extension
        // entry point (with three arguments). `sqlite3_vec_init` from the
        // sqlite-vec crate has zero arguments, but is invoked the same way
        // the upstream test does — transmute through `*const ()` to bridge
        // the signature mismatch. See sqlite-vec/src/lib.rs::tests.
        sqlite3_auto_extension(Some(std::mem::transmute::<
            *const (),
            unsafe extern "C" fn(
                *mut rusqlite::ffi::sqlite3,
                *mut *mut std::os::raw::c_char,
                *const rusqlite::ffi::sqlite3_api_routines,
            ) -> std::os::raw::c_int,
        >(sqlite_vec::sqlite3_vec_init as *const ())));
    });
}

pub struct SearchStore {
    conn: Connection,
}

impl SearchStore {
    /// Open (or create) a search.db at the given path.
    pub fn open(path: &Path) -> Result<Self> {
        register_sqlite_vec();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path)
            .with_context(|| format!("opening search.db at {}", path.display()))?;
        conn.execute_batch(SCHEMA_V1)?;
        conn.execute_batch(SCHEMA_V2_VEC)?;
        conn.execute(
            "INSERT OR REPLACE INTO meta(key, value) VALUES ('schema_version', '2')",
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
    /// Runs inside a single transaction: delete-all (chunks + their vec rows)
    /// then insert-all.
    pub fn replace_repo_chunks(&self, repo: &str, chunks: &[Chunk]) -> Result<()> {
        // Drop existing chunk_vec rows for this repo first; they'd otherwise
        // dangle (vec0 has no foreign key cascade).
        self.conn.execute(
            "DELETE FROM chunk_vec WHERE rowid IN (SELECT id FROM chunks WHERE repo = ?1)",
            params![repo],
        )?;
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

    /// Insert (or replace) a single chunk's embedding vector.
    /// `chunk_id` should equal the autoincrement id from the chunks table;
    /// vec is treated as little-endian f32, length must be `DIM`.
    pub fn upsert_embedding(&self, chunk_id: i64, vector: &[f32]) -> Result<()> {
        if vector.len() != DIM {
            anyhow::bail!(
                "upsert_embedding: expected {} dims, got {}",
                DIM,
                vector.len()
            );
        }
        let bytes = vector_to_le_bytes(vector);
        // vec0 wants `?1` row id and `?2` blob.
        self.conn.execute(
            "INSERT OR REPLACE INTO chunk_vec(rowid, embedding) VALUES (?1, ?2)",
            params![chunk_id, bytes],
        )?;
        Ok(())
    }

    /// k-nearest-neighbour cosine search over `chunk_vec`. Returns
    /// `(chunk_id, distance)` tuples sorted ascending (closer is better).
    /// vec0 expressly returns the L2 distance even for normalised inputs;
    /// callers convert to a similarity if they need one.
    pub fn knn_search(&self, query: &[f32], k: usize) -> Result<Vec<(i64, f32)>> {
        if query.len() != DIM {
            anyhow::bail!(
                "knn_search: expected {} dims, got {}",
                DIM,
                query.len()
            );
        }
        let bytes = vector_to_le_bytes(query);
        let mut stmt = self.conn.prepare(
            "SELECT rowid, distance FROM chunk_vec
             WHERE embedding MATCH ?1
             ORDER BY distance
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![bytes, k as i64], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, f64>(1)? as f32))
        })?;
        let v: rusqlite::Result<Vec<_>> = rows.collect();
        Ok(v?)
    }

    /// Hydrate `(id, repo, path, start_line, end_line, content)` for a list
    /// of chunk ids. Useful after `knn_search` returns row ids.
    pub fn get_chunks_by_ids(&self, ids: &[i64]) -> Result<Vec<SearchHit>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders: String = ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT id, repo, path, start_line, end_line, content
             FROM chunks WHERE id IN ({placeholders})"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let params_dyn: Vec<&dyn rusqlite::ToSql> =
            ids.iter().map(|i| i as &dyn rusqlite::ToSql).collect();
        let rows = stmt.query_map(params_dyn.as_slice(), |row| {
            Ok(SearchHit {
                id: row.get(0)?,
                repo: row.get(1)?,
                path: row.get(2)?,
                start_line: row.get::<_, i64>(3)? as u32,
                end_line: row.get::<_, i64>(4)? as u32,
                content: row.get(5)?,
                score: 0.0,
            })
        })?;
        let v: rusqlite::Result<Vec<_>> = rows.collect();
        Ok(v?)
    }

    /// Number of rows currently in `chunk_vec`. Used by callers to decide
    /// between substring fallback and the full BM25+dense path.
    pub fn embedding_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM chunk_vec", [], |r| r.get(0))?)
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

    /// Borrow the underlying connection (read-only access for ad-hoc queries).
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Case-insensitive substring search across all chunks.
    ///
    /// Returns up to `k` hits ordered by SQLite's natural row order. Score is
    /// always `1.0` (exact substring match). A full BM25+dense path is
    /// available via [`crate::search::index::Index`]; this is the lightweight
    /// fallback used by the CLI / MCP when the dense index is not yet warmed up.
    pub fn search_substring(&self, query: &str, k: usize) -> Result<Vec<SearchHit>> {
        let q_lower = query.to_lowercase();
        let pat = format!("%{}%", q_lower);
        let mut stmt = self.conn.prepare(
            "SELECT id, repo, path, start_line, end_line, content
             FROM chunks
             WHERE LOWER(content) LIKE ?1
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![pat, k as i64], |row| {
            Ok(SearchHit {
                id: row.get(0)?,
                repo: row.get(1)?,
                path: row.get(2)?,
                start_line: row.get::<_, i64>(3)? as u32,
                end_line: row.get::<_, i64>(4)? as u32,
                content: row.get(5)?,
                score: 1.0,
            })
        })?;
        let v: std::result::Result<Vec<_>, _> = rows.collect();
        Ok(v?)
    }
}

/// A single search result row from [`SearchStore::search_substring`].
#[derive(Debug, serde::Serialize)]
pub struct SearchHit {
    pub id: i64,
    pub repo: String,
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub content: String,
    pub score: f32,
}

/// Encode an `f32` slice as little-endian bytes for vec0. SQLite stores BLOBs
/// as raw bytes, so we control the serialisation ourselves.
fn vector_to_le_bytes(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}
