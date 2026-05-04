use crate::core::schema::JSON_SCHEMA_FIND_CONTEXT;
use crate::graph::model::{EdgeKind, Node};
use crate::graph::store::Store;
use anyhow::Result;
use serde::Serialize;

const TOKENS_PER_ITEM_ESTIMATE: u32 = 80;
const MAX_ITEMS: usize = 30;
const PER_TOKEN_QUERY_LIMIT: usize = 100;

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

pub fn find_context(store: &Store, task: &str, budget_tokens: u32) -> Result<ContextResult> {
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

    // --- Substring search (primary path) ---
    let mut by_id: std::collections::HashMap<String, (f32, Node)> =
        std::collections::HashMap::new();
    for token in &tokens {
        for node in store.search_symbols_substring(token, PER_TOKEN_QUERY_LIMIT)? {
            let score = score_node(&node, &tokens);
            by_id
                .entry(node.id.clone())
                .and_modify(|(s, _)| *s = s.max(score))
                .or_insert((score, node));
        }
    }

    // --- Hybrid search enhancement (BM25+dense deferred to v0.2) ---
    // When the search subsystem is queryable in a future version, set B of
    // candidates would be merged here.  For now we annotate every candidate
    // with match_source = "substring" and leave the fusion path as a stub.

    let mut candidates: Vec<(f32, Node, &'static str)> = by_id
        .into_values()
        .map(|(s, n)| (s, n, "substring"))
        .collect();
    candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    // --- Build items ---
    let mut items = Vec::new();
    let mut tokens_used: u32 = 0;
    for (score, n, match_source) in candidates {
        if tokens_used + TOKENS_PER_ITEM_ESTIMATE > budget_tokens {
            break;
        }
        let cross_repo_edges = collect_cross_repo_edges(store, &n)?;
        items.push(ContextItem {
            repo: n.repo,
            path: n.path,
            symbol: n.symbol,
            summary: n.summary,
            relevance_score: score,
            match_source,
            // Substring is high-confidence on exact token matches.
            confidence: 1.0,
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
