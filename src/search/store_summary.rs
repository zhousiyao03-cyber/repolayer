//! Summary-lane storage: `summary_chunks` (text) + `summary_vec` (kNN).
//!
//! Sibling of [`crate::search::store::SearchStore`]'s chunk lane. Shares the
//! underlying SQLite connection via `SearchStore::conn_mut()` — there's no
//! independent open path. The dim used for `summary_vec` matches whatever
//! `SearchStore` was opened with (single embedder per index).

use crate::search::store::SearchStore;
use anyhow::{Context, Result};
use rusqlite::params;

/// One row in `summary_chunks`: the LLM-generated summary text plus its
/// scope (module / type / function / method) and source location.
#[derive(Debug, Clone)]
pub struct SummaryChunk {
    pub repo: String,
    pub path: String,
    pub scope: String,
    pub text: String,
}

/// Thin wrapper around the parent `SearchStore` exposing only the
/// summary-lane CRUD + kNN. Holds a borrow, not ownership, so callers
/// keep their existing `SearchStore` handle alive.
pub struct SummaryStore<'a> {
    parent: &'a SearchStore,
}

impl<'a> SummaryStore<'a> {
    pub fn new(parent: &'a SearchStore) -> Self {
        Self { parent }
    }

    /// Insert a single summary row and return its autoincrement id.
    /// `created_at` is populated by the SQLite default; not passed here.
    pub fn insert(&self, s: &SummaryChunk) -> Result<i64> {
        let conn = self.parent.conn_mut();
        conn.execute(
            "INSERT INTO summary_chunks(repo, path, scope, text) VALUES (?1, ?2, ?3, ?4)",
            params![s.repo, s.path, s.scope, s.text],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Insert or replace the embedding for a given summary row. `summary_id`
    /// must equal `summary_chunks.id`; vector length must equal the parent
    /// store's `embedding_dim()` (same value used for `chunk_vec`).
    pub fn upsert_embedding(&self, summary_id: i64, vector: &[f32]) -> Result<()> {
        let dim = self.parent.embedding_dim()?;
        if vector.len() != dim {
            anyhow::bail!(
                "summary upsert_embedding: expected {} dims, got {}",
                dim,
                vector.len()
            );
        }
        let mut bytes = Vec::with_capacity(vector.len() * 4);
        for f in vector {
            bytes.extend_from_slice(&f.to_le_bytes());
        }
        let conn = self.parent.conn_mut();
        conn.execute(
            "INSERT OR REPLACE INTO summary_vec(rowid, embedding) VALUES (?1, ?2)",
            params![summary_id, bytes],
        )?;
        Ok(())
    }

    /// Drop the summary row + its embedding for a single `(repo, path)` —
    /// used by the incremental `repolayer update` path so a per-file refresh
    /// doesn't have to wipe the entire repo's summary lane. vec0 has no
    /// foreign-key cascade, so we delete from `summary_vec` first.
    pub fn delete_for_path(&self, repo: &str, path: &str) -> Result<()> {
        let conn = self.parent.conn_mut();
        conn.execute(
            "DELETE FROM summary_vec WHERE rowid IN \
             (SELECT id FROM summary_chunks WHERE repo = ?1 AND path = ?2)",
            rusqlite::params![repo, path],
        )?;
        conn.execute(
            "DELETE FROM summary_chunks WHERE repo = ?1 AND path = ?2",
            rusqlite::params![repo, path],
        )?;
        Ok(())
    }

    /// Drop every summary row + its embedding for the given repo. vec0 has
    /// no foreign-key cascade, so we delete from `summary_vec` first.
    pub fn delete_repo(&self, repo: &str) -> Result<()> {
        let conn = self.parent.conn_mut();
        conn.execute(
            "DELETE FROM summary_vec WHERE rowid IN (SELECT id FROM summary_chunks WHERE repo = ?1)",
            params![repo],
        )?;
        conn.execute("DELETE FROM summary_chunks WHERE repo = ?1", params![repo])?;
        Ok(())
    }

    /// Fetch a single row by id. Used by the summary-lane fusion path to
    /// hydrate kNN hits.
    pub fn get_by_id(&self, id: i64) -> Result<SummaryChunk> {
        let conn = self.parent.conn_mut();
        let row = conn
            .query_row(
                "SELECT repo, path, scope, text FROM summary_chunks WHERE id = ?1",
                params![id],
                |r| {
                    Ok(SummaryChunk {
                        repo: r.get(0)?,
                        path: r.get(1)?,
                        scope: r.get(2)?,
                        text: r.get(3)?,
                    })
                },
            )
            .context("get_by_id")?;
        Ok(row)
    }

    /// kNN over `summary_vec`. Returns `(summary_id, distance)` ascending.
    pub fn knn(&self, query: &[f32], k: usize) -> Result<Vec<(i64, f32)>> {
        let dim = self.parent.embedding_dim()?;
        if query.len() != dim {
            anyhow::bail!("summary knn: expected {} dims, got {}", dim, query.len());
        }
        let mut bytes = Vec::with_capacity(query.len() * 4);
        for f in query {
            bytes.extend_from_slice(&f.to_le_bytes());
        }
        let conn = self.parent.conn_mut();
        let mut stmt = conn.prepare(
            "SELECT rowid, distance FROM summary_vec WHERE embedding MATCH ?1
             ORDER BY distance LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![bytes, k as i64], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, f64>(1)? as f32))
        })?;
        let v: rusqlite::Result<Vec<_>> = rows.collect();
        Ok(v?)
    }

    /// Number of embedded summary rows. Lets callers gate dense fusion on
    /// "is the summary lane populated yet?".
    pub fn count(&self) -> Result<i64> {
        Ok(self
            .parent
            .conn_mut()
            .query_row("SELECT COUNT(*) FROM summary_vec", [], |r| r.get(0))?)
    }
}
