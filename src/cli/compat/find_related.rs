//! `repolayer find-related` — find code chunks structurally similar to a given file:line.
//!
//! Strategy (Plan C MVP):
//! 1. Locate the chunk at `file:line` in `search.db`.
//! 2. Use the first ≤50 words of that chunk's content as a query.
//! 3. Run `search_substring` over all chunks; filter out the source itself.
//!
//! The full BM25+dense similarity path (via `Index::find_related`) is available
//! when `.ast-outline/index/` cache exists and can be wired in a later iteration.

use anyhow::Result;
use std::path::PathBuf;

pub async fn run(spec: String, k: usize, json: bool) -> Result<()> {
    // Parse "file:line" format (last colon wins; supports absolute Windows paths).
    let (file, line): (PathBuf, u32) = match spec.rsplit_once(':') {
        Some((f, l)) => (PathBuf::from(f), l.parse().unwrap_or(0)),
        None => (PathBuf::from(&spec), 0),
    };

    let workspace = std::env::current_dir()?;
    let db = workspace.join(".repolayer").join("search.db");
    if !db.exists() {
        anyhow::bail!(
            "no search index found — run `repolayer build` first"
        );
    }

    let store = crate::search::store::SearchStore::open(&db)?;

    let canonical = file.canonicalize().unwrap_or_else(|_| file.clone());
    let path_str = canonical.to_string_lossy().to_string();
    let rel_str = file.to_string_lossy().to_string();

    // The DB stores repo-relative paths (e.g. "src/auth.ts").
    // Build a suffix pattern so we can match regardless of whether the caller
    // passed an absolute or relative path.
    let suffix = canonical
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| rel_str.clone());

    // Find the chunk that contains `file:line`.
    let (target_content, stored_path): (String, String) = {
        let conn = store.conn();
        let line_i = line as i64;

        // Try exact match first (absolute path), then suffix match (relative path in DB).
        let mut stmt = conn.prepare(
            "SELECT content, path FROM chunks
             WHERE (path = ?1 OR path = ?2 OR path LIKE ?3)
               AND start_line <= ?4
               AND end_line >= ?4
             ORDER BY
               CASE WHEN path = ?1 OR path = ?2 THEN 0 ELSE 1 END
             LIMIT 1",
        )?;
        let like_pat = format!("%/{}", suffix);
        let mut rows = stmt.query(rusqlite::params![path_str, rel_str, like_pat, line_i])?;
        match rows.next()? {
            Some(row) => (row.get::<_, String>(0)?, row.get::<_, String>(1)?),
            None => anyhow::bail!(
                "no chunk at {}:{} — check the path and line number",
                canonical.display(),
                line
            ),
        }
    };

    // Build query from first ≤50 words of the source chunk's content.
    let query: String = target_content
        .split_whitespace()
        .take(50)
        .collect::<Vec<_>>()
        .join(" ");

    let mut hits = store.search_substring(&query, k + 1)?;

    // Filter out the source chunk itself (by absolute path, relative path, or stored path).
    hits.retain(|h| h.path != path_str && h.path != rel_str && h.path != stored_path);
    hits.truncate(k);

    if json {
        let entries: Vec<serde_json::Value> = hits
            .iter()
            .map(|h| serde_json::to_value(h).unwrap_or_default())
            .collect();
        let envelope = serde_json::json!({
            "schema_version": "ast-outline.find_related.v1",
            "source": format!("{}:{}", canonical.display(), line),
            "hits": entries,
        });
        println!("{}", serde_json::to_string_pretty(&envelope)?);
    } else {
        if hits.is_empty() {
            eprintln!("no related chunks found");
        } else {
            for hit in &hits {
                println!(
                    "{}:{}-{} (score {:.2})",
                    hit.path, hit.start_line, hit.end_line, hit.score
                );
            }
        }
    }
    Ok(())
}
