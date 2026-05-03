use crate::graph::model::*;
use crate::graph::store::Store;
use anyhow::Result;
use std::collections::HashSet;

pub fn get_callers(store: &Store, name: &str, depth: usize) -> Result<Vec<Node>> {
    let candidates = store.search_symbols_substring(name, 50)?;
    let Some(start) = candidates
        .into_iter()
        .find(|n| n.symbol.as_deref() == Some(name))
    else {
        return Ok(vec![]);
    };
    let mut visited: HashSet<String> = HashSet::new();
    let mut frontier: Vec<String> = vec![start.id.clone()];
    let mut result: Vec<Node> = Vec::new();

    for _ in 0..depth {
        let mut next: Vec<String> = Vec::new();
        for id in &frontier {
            for e in store.incoming_edges(id, EdgeKind::Calls)? {
                if visited.insert(e.from.clone()) {
                    if let Some(n) = store.get_node(&e.from)? {
                        result.push(n);
                        next.push(e.from);
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
