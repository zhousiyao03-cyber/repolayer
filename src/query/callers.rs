use crate::graph::model::*;
use crate::graph::store::Store;
use anyhow::Result;
use std::collections::HashSet;

/// BFS over inbound `Calls` edges starting from one node.
pub fn get_callers(store: &Store, name: &str, depth: usize) -> Result<Vec<Node>> {
    let candidates = store.search_symbols_substring(name, 50)?;
    let Some(start) = candidates
        .into_iter()
        .find(|n| n.symbol.as_deref() == Some(name))
    else {
        return Ok(vec![]);
    };
    walk_callers(store, &[start], depth)
        .map(|chains| chains.into_iter().map(|c| c.caller).collect())
}

/// One caller in a chain plus the target it directly reaches.
///
/// `target` is the immediate callee at depth-1; for deeper hops it is the
/// node the caller's outbound `Calls` edge points to (which is itself a
/// caller of the original symbol). This lets the CLI render
/// `caller -> target` pairs unambiguously when multiple definitions of the
/// same symbol exist across repos.
#[derive(Debug, Clone)]
pub struct CallerChain {
    pub caller: Node,
    pub target: Node,
    pub confidence: f32,
}

/// Aggregate inbound `Calls` edges across multiple starting nodes.
///
/// Behaviour:
/// - Each start node is treated as a target; its inbound `Calls` edges
///   produce depth-1 callers.
/// - At each subsequent hop, the previous frontier becomes the new set of
///   targets — so every `CallerChain.target` is itself reachable from one
///   of the original starts.
/// - A caller is reported at most once across the whole walk; the `target`
///   recorded is the one at which it was first discovered.
pub fn walk_callers(store: &Store, starts: &[Node], depth: usize) -> Result<Vec<CallerChain>> {
    if depth == 0 || starts.is_empty() {
        return Ok(vec![]);
    }
    let mut visited: HashSet<String> = HashSet::new();
    for s in starts {
        visited.insert(s.id.clone());
    }
    let mut frontier: Vec<Node> = starts.to_vec();
    let mut result: Vec<CallerChain> = Vec::new();

    for _ in 0..depth {
        let mut next: Vec<Node> = Vec::new();
        for target in &frontier {
            for e in store.incoming_edges(&target.id, EdgeKind::Calls)? {
                if visited.insert(e.from.clone()) {
                    if let Some(caller) = store.get_node(&e.from)? {
                        result.push(CallerChain {
                            caller: caller.clone(),
                            target: target.clone(),
                            confidence: e.confidence,
                        });
                        next.push(caller);
                    }
                }
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }
    Ok(result)
}

/// Convenience wrapper: resolve `name` to *all* nodes whose symbol matches
/// exactly (across every repo), then walk inbound `Calls` from each.
///
/// Returns the resolved start nodes alongside the caller chains so the CLI
/// can tell the user "no exact match" vs "matched but no callers".
pub fn get_callers_all(
    store: &Store,
    name: &str,
    depth: usize,
    repo_filter: Option<&str>,
) -> Result<(Vec<Node>, Vec<CallerChain>)> {
    let candidates = store.search_symbols_substring_filtered(name, repo_filter, 200)?;
    let starts: Vec<Node> = candidates
        .into_iter()
        .filter(|n| n.symbol.as_deref() == Some(name))
        .collect();
    if starts.is_empty() {
        return Ok((vec![], vec![]));
    }
    let chains = walk_callers(store, &starts, depth)?;
    Ok((starts, chains))
}
