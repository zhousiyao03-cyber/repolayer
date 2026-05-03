use crate::graph::model::Node;
use crate::graph::store::Store;
use anyhow::Result;
use serde::Serialize;

const TOKENS_PER_ITEM_ESTIMATE: u32 = 80;
const MAX_ITEMS: usize = 30;
const PER_TOKEN_QUERY_LIMIT: usize = 100;

#[derive(Debug, Serialize)]
pub struct ContextResult {
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
    pub call_chain: Option<Vec<String>>,
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
            items: vec![],
            total_tokens: 0,
            suggestion:
                "No usable query tokens (task description was too short or non-alphanumeric)".into(),
        });
    }

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

    let mut candidates: Vec<(f32, Node)> = by_id.into_values().collect();
    candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut items = Vec::new();
    let mut tokens_used: u32 = 0;
    for (score, n) in candidates {
        if tokens_used + TOKENS_PER_ITEM_ESTIMATE > budget_tokens {
            break;
        }
        items.push(ContextItem {
            repo: n.repo,
            path: n.path,
            symbol: n.symbol,
            summary: n.summary,
            relevance_score: score,
            call_chain: None,
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
        items,
        total_tokens: tokens_used,
        suggestion,
    })
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
