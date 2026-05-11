//! `repolayer search` — hybrid BM25 + dense embedding search.
//!
//! Builds an in-memory BM25 index over all chunks on every call (postings
//! aren't persisted; ms-level work even on workspaces with thousands of
//! chunks). If the embedding model is cached locally and `chunk_vec` has
//! rows, also runs a dense kNN against the query embedding and fuses with
//! RRF. Falls back to substring matching when both paths produce no hits.
//!
//! Output discipline: by default the JSON envelope drops the chunk body
//! (callers already get path:line_range and can fetch with `repolayer show`).
//! Pass `--full-content` when you actually need the body inline.

use crate::cli::repo_filter::require_repo;
use crate::cli::workspace;
use crate::search::store::SearchLane;
use anyhow::Result;

const PREVIEW_CHARS: usize = 200;

pub async fn run(
    query: String,
    k: usize,
    repo: Option<String>,
    json: bool,
    full_content: bool,
) -> Result<()> {
    let db = workspace::store_path("search.db")?;
    if !db.exists() {
        anyhow::bail!(
            "no search index found at {} — run `repolayer build` first (or set $REPOLAYER_INDEX)",
            db.display()
        );
    }

    let store = crate::search::store::SearchStore::open(&db)?;

    let validated_repo = match repo.as_deref() {
        None => None,
        Some(name) => {
            let known = store.list_repo_names()?;
            Some(require_repo(name, &known)?.to_string())
        }
    };

    // Try to encode the query with the cached embedding model. If the model
    // isn't there, we silently degrade to BM25-only.
    let qv = crate::search::embed::try_encode_query(&query);

    let (hits, lane) =
        store.search_hybrid_filtered(&query, k, qv.as_deref(), None, validated_repo.as_deref())?;

    if json {
        let entries: Vec<serde_json::Value> = hits
            .iter()
            .map(|h| {
                let mut entry = serde_json::json!({
                    "repo": h.repo,
                    "path": h.path,
                    "start_line": h.start_line,
                    "end_line": h.end_line,
                    "score": h.score,
                });
                if full_content {
                    entry["content"] = serde_json::Value::String(h.content.clone());
                } else {
                    entry["preview"] = serde_json::Value::String(preview(&h.content));
                }
                entry
            })
            .collect();
        let envelope = serde_json::json!({
            "schema_version": "repolayer.search.v1",
            "query": query,
            "repo_filter": validated_repo,
            "lane": lane.as_str(),
            "hits": entries,
            "full_content": full_content,
        });
        println!("{}", serde_json::to_string_pretty(&envelope)?);
        return Ok(());
    }

    let scope = match validated_repo.as_deref() {
        Some(r) => format!(" in repo={}", r),
        None => String::new(),
    };
    if hits.is_empty() {
        println!("# no results for '{}'{}", query, scope);
        if validated_repo.is_some() {
            println!(
                "# fallback: drop --repo to widen, or `rg \"{}\" <repo path>` for literal grep",
                query
            );
        } else {
            println!(
                "# fallback: `repolayer query \"<symbol>\"` for exact symbol lookup, or `rg \"{}\"` for literal grep",
                query
            );
        }
        return Ok(());
    }
    println!(
        "# {} hits for '{}'{} — lane={} — fetch bodies with `repolayer show <path> <symbol>`",
        hits.len(),
        query,
        scope,
        lane.as_str(),
    );
    if let Some(warning) = lane_warning(lane) {
        println!("# {}", warning);
    }
    for (i, hit) in hits.iter().enumerate() {
        println!(
            "[{}] {}\t{}:{}-{}\tscore={:.4}",
            i + 1,
            hit.repo,
            hit.path,
            hit.start_line,
            hit.end_line,
            hit.score,
        );
    }
    Ok(())
}

/// One-line caveat to print above the result list when a lane is more noisy
/// than fusion. Returns `None` when the lane is fully trustworthy.
fn lane_warning(lane: SearchLane) -> Option<&'static str> {
    match lane {
        SearchLane::Fusion | SearchLane::Empty => None,
        SearchLane::Bm25Only => Some(
            "lane=bm25_only: no semantic match — results rank purely by token overlap. \
             Try rephrasing or `rg` if these don't fit.",
        ),
        SearchLane::SemanticOnly => Some(
            "lane=semantic_only: query had no lexical anchor in the index — results are \
             approximate. Verify with `repolayer query` if you know a symbol name.",
        ),
        SearchLane::Substring => Some(
            "lane=substring: neither lexical nor semantic search matched — falling back to \
             plain LIKE. Often noisy; treat results as candidates only.",
        ),
    }
}

/// Build a short single-line preview suitable for an LLM-facing JSON envelope.
/// Collapses interior whitespace so a long indented function header doesn't
/// blow past the cap on the first line alone.
fn preview(content: &str) -> String {
    let mut buf = String::with_capacity(PREVIEW_CHARS + 4);
    let mut last_was_ws = true;
    for ch in content.chars() {
        if buf.chars().count() >= PREVIEW_CHARS {
            buf.push('…');
            break;
        }
        if ch.is_whitespace() {
            if !last_was_ws {
                buf.push(' ');
                last_was_ws = true;
            }
        } else {
            buf.push(ch);
            last_was_ws = false;
        }
    }
    buf.trim().to_string()
}
