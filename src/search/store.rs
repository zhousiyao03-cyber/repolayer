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

/// Row returned by [`SearchStore::list_all_chunks`]: same as `ChunkRow` but
/// with the repo name in slot 1.
pub type ChunkRowWithRepo = (i64, String, String, u32, u32, String);

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
        crate::graph::store::apply_perf_pragmas(&conn)?;
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

    /// List every chunk across every repo. Returns
    /// `(id, repo, path, start_line, end_line, content)` rows. Used by
    /// `search_hybrid` so it can build an in-memory BM25 index.
    ///
    /// When `repo_filter` is Some, only chunks belonging to that repo are
    /// returned. The BM25 index is then built solely over that subset, so
    /// IDF weights reflect *the repo*, not the workspace — hits inside the
    /// repo aren't penalised for being common across the whole workspace.
    pub fn list_all_chunks(&self) -> Result<Vec<ChunkRowWithRepo>> {
        self.list_chunks_filtered(None)
    }

    /// Same as [`list_all_chunks`] but optionally restricted to a single repo.
    pub fn list_chunks_filtered(
        &self,
        repo_filter: Option<&str>,
    ) -> Result<Vec<ChunkRowWithRepo>> {
        let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<ChunkRowWithRepo> {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)? as u32,
                row.get::<_, i64>(4)? as u32,
                row.get::<_, String>(5)?,
            ))
        };
        let rows: rusqlite::Result<Vec<ChunkRowWithRepo>> = match repo_filter {
            None => {
                let mut stmt = self.conn.prepare(
                    "SELECT id, repo, path, start_line, end_line, content FROM chunks ORDER BY id",
                )?;
                let mapped = stmt.query_map([], map_row)?;
                mapped.collect()
            }
            Some(repo) => {
                let mut stmt = self.conn.prepare(
                    "SELECT id, repo, path, start_line, end_line, content FROM chunks WHERE repo = ?1 ORDER BY id",
                )?;
                let mapped = stmt.query_map(params![repo], map_row)?;
                mapped.collect()
            }
        };
        Ok(rows?)
    }

    /// Distinct repo names recorded in the chunks table. Used to produce
    /// agent-friendly "did-you-mean" hints when a `--repo` filter doesn't match.
    pub fn list_repo_names(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT repo FROM chunks ORDER BY repo")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let v: rusqlite::Result<Vec<_>> = rows.collect();
        Ok(v?)
    }

    /// Delete all chunks for a specific file within a repo.
    /// Drops the matching chunk_vec rows first so vec0 entries don't dangle.
    pub fn delete_file(&self, repo: &str, path: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM chunk_vec WHERE rowid IN \
             (SELECT id FROM chunks WHERE repo = ?1 AND path = ?2)",
            params![repo, path],
        )?;
        self.conn.execute(
            "DELETE FROM chunks WHERE repo = ?1 AND path = ?2",
            params![repo, path],
        )?;
        Ok(())
    }

    /// Insert one file's chunks (no delete first — caller must have cleared
    /// the existing chunks via `delete_file` if needed). Returns the rowid
    /// for each inserted chunk in input order so the caller can write
    /// embeddings.
    pub fn insert_file_chunks(&self, repo: &str, chunks: &[Chunk]) -> Result<Vec<i64>> {
        let mut ids = Vec::with_capacity(chunks.len());
        for chunk in chunks {
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
            ids.push(self.conn.last_insert_rowid());
        }
        Ok(ids)
    }

    /// Borrow the underlying connection (read-only access for ad-hoc queries).
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Hybrid BM25 + dense embedding search across every repo.
    ///
    /// - Always builds a fresh in-memory BM25 index over `chunks.content`
    ///   (postings can't live in SQLite without significant complication;
    ///   building over thousands of chunks is millisecond-level work).
    /// - If `query_embedding` is supplied AND `chunk_vec` has rows, also
    ///   pulls k×4 nearest neighbours from vec0 and fuses with RRF using
    ///   `alpha` (None → auto-pick: 0.3 for symbol-like queries, 0.5 for NL).
    /// - If neither path produces hits, falls through to substring matching
    ///   so the user always gets something deterministic.
    ///
    /// The returned `lane` tells callers (and ultimately the agent reading
    /// the output) which retrieval path produced these hits — important
    /// because semantic-only or substring fallback results are often noisy
    /// on bogus queries and shouldn't be trusted as confidently as fusion.
    pub fn search_hybrid(
        &self,
        query: &str,
        k: usize,
        query_embedding: Option<&[f32]>,
        alpha: Option<f32>,
    ) -> Result<(Vec<SearchHit>, SearchLane)> {
        self.search_hybrid_filtered(query, k, query_embedding, alpha, None)
    }

    /// Same as [`search_hybrid`], but optionally restrict the corpus to a
    /// single repo. The BM25 index is rebuilt over just that repo so its
    /// IDF reflects the repo's own term distribution; the dense kNN pull
    /// is over-fetched (~16×) and post-filtered, since vec0 has no native
    /// per-repo predicate.
    pub fn search_hybrid_filtered(
        &self,
        query: &str,
        k: usize,
        query_embedding: Option<&[f32]>,
        alpha: Option<f32>,
        repo_filter: Option<&str>,
    ) -> Result<(Vec<SearchHit>, SearchLane)> {
        use crate::search::{bm25::Bm25Index, fusion, tokens::tokenize};

        let all = self.list_chunks_filtered(repo_filter)?;
        if all.is_empty() {
            return Ok((Vec::new(), SearchLane::Empty));
        }

        // Map from chunk_id (vec0 rowid) to position in `all`. BM25 indexes
        // by position (0-indexed dense), but vec0 returns the chunk_id.
        // When repo_filter is set, this map only contains in-repo chunks —
        // dense neighbours from other repos get dropped at the lookup stage.
        let mut id_to_pos = std::collections::HashMap::with_capacity(all.len());
        for (pos, row) in all.iter().enumerate() {
            id_to_pos.insert(row.0, pos as u32);
        }

        let docs: Vec<Vec<String>> = all.iter().map(|r| tokenize(&r.5)).collect();
        let bm25 = Bm25Index::build(docs);

        let query_tokens = tokenize(query);
        let bm25_scores = bm25.get_scores(&query_tokens, None);

        // Top BM25-ranked positions.
        let mut bm25_ranked: Vec<(u32, f32)> = bm25_scores
            .iter()
            .enumerate()
            .filter_map(|(i, &s)| if s > 0.0 { Some((i as u32, s)) } else { None })
            .collect();
        bm25_ranked
            .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        bm25_ranked.truncate(k * 4);

        // Optional dense path. We feed vec0 distances back through RRF after
        // converting position-id pairs into the same rank space as BM25.
        //
        // Threshold rationale: vec0 returns L2 distance on L2-normalised
        // vectors. cos = 1 - d²/2, so a cosine floor of 0.4 (loose, "vaguely
        // related") translates to d² ≤ 1.2, i.e. d ≤ ~1.095. Anything beyond
        // that on a 256-dim potion-code embedding is statistical noise on
        // bogus queries and always fills 10 slots regardless of relevance.
        const DENSE_LOOSE_DIST_MAX: f32 = 1.10;
        // Stricter floor (cosine ≥ 0.5) when BM25 produced *zero* hits — the
        // query had no lexical anchor, so we need higher semantic confidence
        // to avoid hallucinated top-K. d² ≤ 1.0 → d ≤ 1.0.
        const DENSE_STRICT_DIST_MAX: f32 = 1.00;
        let mut sem_ranked: Vec<(u32, f32)> = Vec::new();
        if let Some(qv) = query_embedding {
            if self.embedding_count()? > 0 && qv.len() == DIM {
                // vec0 has no per-repo predicate, so when filtering to one repo
                // we over-fetch and let id_to_pos drop out-of-repo rows. The
                // 4× multiplier already over-fetches for k; an extra 4× for
                // repo filter handles workspaces where a repo holds ≤25% of
                // the chunks (typical for our 40-repo ttec workspace).
                let knn_k = if repo_filter.is_some() { k * 16 } else { k * 4 };
                let neighbours = self.knn_search(qv, knn_k)?;
                for (chunk_id, dist) in &neighbours {
                    if *dist > DENSE_LOOSE_DIST_MAX {
                        continue;
                    }
                    if let Some(&pos) = id_to_pos.get(chunk_id) {
                        // vec0 returns L2 distance for normalised vectors;
                        // smaller is better. Convert to a similarity by
                        // negating so RRF's "higher is better" assumption holds.
                        sem_ranked.push((pos, -dist));
                    }
                }
            }
        }

        // Fuse only when both lanes had hits. Otherwise fall back to whichever
        // produced something.
        let (chosen, lane): (Vec<(u32, f32)>, SearchLane) =
            if !sem_ranked.is_empty() && !bm25_ranked.is_empty() {
                let alpha = fusion::resolve_alpha(query, alpha);
                let sem_rrf = fusion::rrf_scores(&sem_ranked);
                let bm_rrf = fusion::rrf_scores(&bm25_ranked);
                let combined = fusion::combine(&sem_rrf, &bm_rrf, alpha);
                let mut v: Vec<(u32, f32)> = combined.into_iter().collect();
                v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                v.truncate(k);
                (v, SearchLane::Fusion)
            } else if !bm25_ranked.is_empty() {
                (
                    bm25_ranked.into_iter().take(k).collect(),
                    SearchLane::Bm25Only,
                )
            } else if !sem_ranked.is_empty() {
                // No lexical anchor — apply the strict semantic floor so a
                // gibberish query doesn't return its 10 closest random chunks.
                // sem_ranked stores `-dist`, so "below the strict threshold"
                // means score >= -DENSE_STRICT_DIST_MAX.
                let strict: Vec<(u32, f32)> = sem_ranked
                    .into_iter()
                    .filter(|(_, neg_dist)| *neg_dist >= -DENSE_STRICT_DIST_MAX)
                    .take(k)
                    .collect();
                if strict.is_empty() {
                    let hits = self.search_substring_filtered(query, k, repo_filter)?;
                    return Ok((hits, SearchLane::Substring));
                }
                (strict, SearchLane::SemanticOnly)
            } else {
                let hits = self.search_substring_filtered(query, k, repo_filter)?;
                return Ok((hits, SearchLane::Substring));
            };

        let hits: Vec<SearchHit> = chosen
            .into_iter()
            .filter_map(|(pos, score)| {
                let row = all.get(pos as usize)?;
                Some(SearchHit {
                    id: row.0,
                    repo: row.1.clone(),
                    path: row.2.clone(),
                    start_line: row.3,
                    end_line: row.4,
                    content: row.5.clone(),
                    score,
                })
            })
            .collect();
        Ok((hits, lane))
    }

    /// Case-insensitive substring search across all chunks.
    ///
    /// Returns up to `k` hits ordered by SQLite's natural row order. Score is
    /// always `1.0` (exact substring match). Used as the deterministic
    /// fallback when neither BM25 nor the dense index produces hits.
    pub fn search_substring(&self, query: &str, k: usize) -> Result<Vec<SearchHit>> {
        self.search_substring_filtered(query, k, None)
    }

    /// Substring fallback restricted to a single repo when `repo_filter` is set.
    pub fn search_substring_filtered(
        &self,
        query: &str,
        k: usize,
        repo_filter: Option<&str>,
    ) -> Result<Vec<SearchHit>> {
        let q_lower = query.to_lowercase();
        let pat = format!("%{}%", q_lower);
        let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<SearchHit> {
            Ok(SearchHit {
                id: row.get(0)?,
                repo: row.get(1)?,
                path: row.get(2)?,
                start_line: row.get::<_, i64>(3)? as u32,
                end_line: row.get::<_, i64>(4)? as u32,
                content: row.get(5)?,
                score: 1.0,
            })
        };
        let rows: rusqlite::Result<Vec<_>> = match repo_filter {
            None => {
                let mut stmt = self.conn.prepare(
                    "SELECT id, repo, path, start_line, end_line, content
                     FROM chunks
                     WHERE LOWER(content) LIKE ?1
                     LIMIT ?2",
                )?;
                let mapped = stmt.query_map(params![pat, k as i64], map_row)?;
                mapped.collect()
            }
            Some(repo) => {
                let mut stmt = self.conn.prepare(
                    "SELECT id, repo, path, start_line, end_line, content
                     FROM chunks
                     WHERE repo = ?1 AND LOWER(content) LIKE ?2
                     LIMIT ?3",
                )?;
                let mapped = stmt.query_map(params![repo, pat, k as i64], map_row)?;
                mapped.collect()
            }
        };
        Ok(rows?)
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

/// Which retrieval path produced a result set. The agent reads this to decide
/// how much to trust the hits — fusion is the most reliable; substring is
/// the last-resort tokenless fallback and frequently matches asset / lockfile
/// chunks on bogus queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchLane {
    /// BM25 + dense both contributed; results were fused via RRF.
    Fusion,
    /// BM25 had hits but dense did not (or was filtered by the loose floor).
    Bm25Only,
    /// Dense had hits, BM25 did not. Already filtered by the strict cosine
    /// floor — but with no lexical anchor the ranking can still be loose.
    SemanticOnly,
    /// Neither retrieval lane produced anything; chunks are returned by
    /// case-insensitive substring match. Treat with skepticism.
    Substring,
    /// The chunk corpus was empty — no index has been built for this repo.
    /// Also the default returned by `unwrap_or_default()` on errors.
    #[default]
    Empty,
}

impl SearchLane {
    pub fn as_str(&self) -> &'static str {
        match self {
            SearchLane::Fusion => "fusion",
            SearchLane::Bm25Only => "bm25_only",
            SearchLane::SemanticOnly => "semantic_only",
            SearchLane::Substring => "substring",
            SearchLane::Empty => "empty",
        }
    }
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
