//! `repolayer search` — hybrid BM25 + dense embedding search.
//!
//! Builds an in-memory BM25 index over all chunks on every call (postings
//! aren't persisted; ms-level work even on workspaces with thousands of
//! chunks). If the embedding model is cached locally and `chunk_vec` has
//! rows, also runs a dense kNN against the query embedding and fuses with
//! RRF. Falls back to substring matching when both paths produce no hits.

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

    // Try to encode the query with the cached embedding model. If the model
    // isn't there, we silently degrade to BM25-only.
    let qv = crate::search::embed::try_encode_query(&query);

    let hits = store.search_hybrid(&query, k, qv.as_deref(), None)?;

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
    } else if hits.is_empty() {
        eprintln!("no results for '{}'", query);
    } else {
        for (i, hit) in hits.iter().enumerate() {
            println!(
                "[{}] {}:{}-{} (repo: {}, score: {:.4})",
                i + 1,
                hit.path,
                hit.start_line,
                hit.end_line,
                hit.repo,
                hit.score,
            );
        }
    }
    Ok(())
}

