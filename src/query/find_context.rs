use crate::core::schema::JSON_SCHEMA_FIND_CONTEXT;
use crate::graph::model::{EdgeKind, Node};
use crate::graph::store::Store;
use crate::search::store::{SearchHit, SearchStore};
use anyhow::Result;
use serde::Serialize;

const TOKENS_PER_ITEM_ESTIMATE: u32 = 80;
const MAX_ITEMS: usize = 30;
const PER_TOKEN_QUERY_LIMIT: usize = 100;
/// How many search-index chunks to pull before mapping back to graph nodes.
/// Search-side already does its own k truncation; this is just a ceiling.
const SEARCH_CANDIDATE_K: usize = 60;
/// RRF constant — same as `crate::search::fusion`. 60 is the standard choice
/// from the original RRF paper; smaller values weight the top of each ranking
/// more aggressively, larger values smooth across ranks.
const RRF_K: f32 = 60.0;

#[derive(Debug, Serialize)]
pub struct ContextResult {
    pub schema_version: &'static str,
    pub items: Vec<ContextItem>,
    pub total_tokens: u32,
    pub suggestion: String,
}

#[derive(Debug, Serialize)]
pub struct ContextItem {
    pub repo: String,
    pub path: String,
    pub symbol: Option<String>,
    pub summary: Option<String>,
    pub relevance_score: f32,
    /// How this item was discovered: "substring" | "search" | "fusion"
    pub match_source: &'static str,
    /// Confidence in the match (0.0–1.0). Substring exact matches = 1.0.
    pub confidence: f32,
    /// Rough token cost estimate for reading this item.
    pub estimated_tokens: u32,
    pub call_chain: Option<Vec<String>>,
    /// Edges from this node that cross into a different repo.
    pub cross_repo_edges: Vec<EdgeRef>,
}

#[derive(Debug, Serialize)]
pub struct EdgeRef {
    pub kind: String,
    pub target_repo: String,
    pub target_path: String,
    pub target_symbol: Option<String>,
    pub confidence: f32,
}

/// Find relevant context for a task description.
///
/// Two retrieval lanes:
/// - **Substring** over `nodes.symbol` / `nodes.path`. High precision when
///   the agent already knows part of the symbol it's looking for.
/// - **BM25 + dense embedding** over chunk text from `search.db`. Catches
///   conceptual / synonym matches the substring lane misses. Only available
///   when `search` is `Some(_)`; the embedding model is used only when
///   already cached on disk (no surprise downloads at query time).
///
/// When both lanes produce candidates we merge with reciprocal rank fusion;
/// ties go to whichever lane had the higher rank.
pub fn find_context(
    store: &Store,
    search: Option<&SearchStore>,
    task: &str,
    budget_tokens: u32,
) -> Result<ContextResult> {
    let tokens: Vec<String> = task
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() > 2)
        .map(String::from)
        .collect();

    if tokens.is_empty() {
        return Ok(ContextResult {
            schema_version: JSON_SCHEMA_FIND_CONTEXT,
            items: vec![],
            total_tokens: 0,
            suggestion:
                "No usable query tokens (task description was too short or non-alphanumeric)"
                    .into(),
        });
    }

    // ── Lane 1: substring over graph nodes ──────────────────────────────────
    let mut substring_pool: std::collections::HashMap<String, (f32, Node)> =
        std::collections::HashMap::new();
    for token in &tokens {
        for node in store.search_symbols_substring(token, PER_TOKEN_QUERY_LIMIT)? {
            let s = score_node(&node, &tokens);
            substring_pool
                .entry(node.id.clone())
                .and_modify(|(prev, _)| *prev = prev.max(s))
                .or_insert((s, node));
        }
    }
    let mut substring_ranked: Vec<(f32, Node)> = substring_pool.into_values().collect();
    substring_ranked
        .sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    // ── Lane 2: hybrid search (BM25 + optional dense embedding) ─────────────
    // Each search hit is a chunk (file + line range); we lift it to whichever
    // graph node is anchored at the same `(repo, path)`. If multiple nodes
    // share that path (e.g. several functions in a file), we pick the one
    // whose loc_start falls inside the chunk's line range — preferring the
    // most-specific match keeps the context item actionable.
    let mut search_ranked: Vec<(f32, Node)> = Vec::new();
    if let Some(search_store) = search {
        let qv = crate::search::embed::try_encode_query(task);
        let hits = search_store
            .search_hybrid(task, SEARCH_CANDIDATE_K, qv.as_deref(), None)
            .unwrap_or_default();
        let mut seen_node_ids = std::collections::HashSet::new();
        for hit in &hits {
            if let Some(node) = pick_node_for_hit(store, hit)? {
                if seen_node_ids.insert(node.id.clone()) {
                    search_ranked.push((hit.score, node));
                }
            }
        }
    }

    // ── Fusion: build the final ranking ─────────────────────────────────────
    // Reciprocal rank fusion is robust to the wildly different score scales
    // between substring (0..~10) and search (RRF-of-RRF, ~0..0.03). For each
    // node id we collect 1/(k+rank) contributions from whichever lane(s) it
    // appears in, then sort.
    let mut fused: std::collections::HashMap<String, FusedEntry> =
        std::collections::HashMap::new();
    for (rank, (_, node)) in substring_ranked.iter().enumerate() {
        let contrib = 1.0 / (RRF_K + rank as f32 + 1.0);
        fused
            .entry(node.id.clone())
            .and_modify(|e| {
                e.score += contrib;
                e.in_substring = true;
            })
            .or_insert_with(|| FusedEntry {
                score: contrib,
                in_substring: true,
                in_search: false,
                node: node.clone(),
            });
    }
    for (rank, (_, node)) in search_ranked.iter().enumerate() {
        let contrib = 1.0 / (RRF_K + rank as f32 + 1.0);
        fused
            .entry(node.id.clone())
            .and_modify(|e| {
                e.score += contrib;
                e.in_search = true;
            })
            .or_insert_with(|| FusedEntry {
                score: contrib,
                in_substring: false,
                in_search: true,
                node: node.clone(),
            });
    }
    let mut candidates: Vec<FusedEntry> = fused.into_values().collect();
    candidates
        .sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    // ── Materialize items under the token budget ────────────────────────────
    let mut items = Vec::new();
    let mut tokens_used: u32 = 0;
    for entry in candidates {
        if tokens_used + TOKENS_PER_ITEM_ESTIMATE > budget_tokens {
            break;
        }
        let cross_repo_edges = collect_cross_repo_edges(store, &entry.node)?;
        let (match_source, confidence) = match (entry.in_substring, entry.in_search) {
            (true, true) => ("fusion", 1.0),
            (true, false) => ("substring", 1.0),
            (false, true) => ("search", 0.7),
            // Unreachable — every fused entry came from at least one lane.
            (false, false) => ("substring", 0.5),
        };
        items.push(ContextItem {
            repo: entry.node.repo,
            path: entry.node.path,
            symbol: entry.node.symbol,
            summary: entry.node.summary,
            relevance_score: entry.score,
            match_source,
            confidence,
            estimated_tokens: TOKENS_PER_ITEM_ESTIMATE,
            call_chain: None,
            cross_repo_edges,
        });
        tokens_used += TOKENS_PER_ITEM_ESTIMATE;
        if items.len() >= MAX_ITEMS {
            break;
        }
    }

    let suggestion = if items.is_empty() {
        "No matches found. Try different query terms or run `repolayer build` to ensure the index is up to date.".to_string()
    } else {
        format!("Read these {} files for full context.", items.len())
    };

    Ok(ContextResult {
        schema_version: JSON_SCHEMA_FIND_CONTEXT,
        items,
        total_tokens: tokens_used,
        suggestion,
    })
}

struct FusedEntry {
    score: f32,
    in_substring: bool,
    in_search: bool,
    node: Node,
}

/// Pick the graph node that best represents a search hit.
///
/// Preference order:
/// 1. A Function/Method/Type node whose `loc_start` falls inside the hit's
///    line range — that's the symbol the chunk actually shows.
/// 2. Otherwise, the Module node for the file (always present after build).
/// 3. Otherwise, `None` — the search index may include files that didn't
///    contribute graph nodes (e.g. markdown), in which case we drop the hit.
fn pick_node_for_hit(store: &Store, hit: &SearchHit) -> Result<Option<Node>> {
    use crate::graph::model::NodeKind;

    let nodes = store.nodes_at_path(&hit.repo, &hit.path)?;
    if nodes.is_empty() {
        return Ok(None);
    }

    let line_start = hit.start_line;
    let line_end = hit.end_line;

    // Pass 1: most-specific symbol whose loc_start is within the chunk.
    let inside: Vec<&Node> = nodes
        .iter()
        .filter(|n| {
            matches!(
                n.kind,
                NodeKind::Function | NodeKind::Method | NodeKind::Type
            )
        })
        .filter(|n| {
            n.loc_start
                .is_some_and(|s| s >= line_start && s <= line_end)
        })
        .collect();
    if let Some(best) = inside.into_iter().max_by_key(|n| n.loc_start.unwrap_or(0)) {
        return Ok(Some(best.clone()));
    }

    // Pass 2: module node for the file.
    if let Some(module) = nodes.iter().find(|n| matches!(n.kind, NodeKind::Module)) {
        return Ok(Some(module.clone()));
    }

    Ok(None)
}

/// Collect edges from `node` that cross into a different repo.
/// Only `Imports`, `Invokes`, and `Implements` edge kinds are surfaced.
fn collect_cross_repo_edges(store: &Store, node: &Node) -> Result<Vec<EdgeRef>> {
    let edges = store.get_edges_from(&node.id)?;
    let mut out = Vec::new();
    for edge in edges {
        if !matches!(
            edge.kind,
            EdgeKind::Imports | EdgeKind::Invokes | EdgeKind::Implements
        ) {
            continue;
        }
        if let Some(target) = store.get_node(&edge.to)? {
            if target.repo != node.repo {
                out.push(EdgeRef {
                    kind: format!("{:?}", edge.kind),
                    target_repo: target.repo,
                    target_path: target.path,
                    target_symbol: target.symbol,
                    confidence: edge.confidence,
                });
            }
        }
    }
    Ok(out)
}

fn score_node(n: &Node, tokens: &[String]) -> f32 {
    let mut s = 0.0;
    let symbol = n.symbol.as_deref().unwrap_or("").to_lowercase();
    let path = n.path.to_lowercase();
    for t in tokens {
        if symbol.contains(t.as_str()) {
            s += 3.0;
        }
        if path.contains(t.as_str()) {
            s += 1.5;
        }
        if let Some(summary) = &n.summary {
            if summary.to_lowercase().contains(t.as_str()) {
                s += 1.0;
            }
        }
    }
    s
}
