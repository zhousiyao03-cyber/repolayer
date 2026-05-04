//! `repolayer search` — hybrid BM25 + substring search across the workspace.
//!
//! Plan C MVP uses a SQLite substring fallback (`SearchStore::search_substring`).
//! The full BM25+dense path is available via `Index::search` in
//! `src/search/index.rs` but requires the embedding model to be cached on disk;
//! that path is used automatically when the `.ast-outline/index/` cache exists
//! (wired in a later iteration).

use anyhow::Result;

pub async fn run(query: String, k: usize, json: bool) -> Result<()> {
    let workspace = std::env::current_dir()?;
    let db = workspace.join(".repolayer").join("search.db");
    if !db.exists() {
        anyhow::bail!(
            "no search index found at {} — run `repolayer build` first",
            db.display()
        );
    }

    let store = crate::search::store::SearchStore::open(&db)?;
    let hits = store.search_substring(&query, k)?;

    if json {
        let entries: Vec<serde_json::Value> = hits
            .iter()
            .map(|h| serde_json::to_value(h).unwrap_or_default())
            .collect();
        let envelope = serde_json::json!({
            "schema_version": "ast-outline.search.v1",
            "query": query,
            "hits": entries,
        });
        println!("{}", serde_json::to_string_pretty(&envelope)?);
    } else {
        if hits.is_empty() {
            eprintln!("no results for '{}'", query);
        } else {
            for (i, hit) in hits.iter().enumerate() {
                println!(
                    "[{}] {}:{}-{} (repo: {}, score: {:.2})",
                    i + 1,
                    hit.path,
                    hit.start_line,
                    hit.end_line,
                    hit.repo,
                    hit.score,
                );
            }
        }
    }
    Ok(())
}
