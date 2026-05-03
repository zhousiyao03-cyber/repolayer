use crate::graph::model::*;
use crate::graph::store::Store;
use anyhow::Result;
use std::collections::HashSet;

pub fn get_dependencies(store: &Store, repo_or_module: &str, depth: usize) -> Result<Vec<Node>> {
    // Try to find start node by path (Module) or symbol name (Symbol).
    // First look among Module nodes by matching path suffix, then fall back to symbol search.
    let start_opt = {
        let modules = store.list_nodes_by_kind(NodeKind::Module)?;
        modules
            .into_iter()
            .find(|n| n.path == repo_or_module || n.path.ends_with(repo_or_module))
    };

    let start = match start_opt {
        Some(n) => n,
        None => {
            // fall back to symbol search
            let candidates = store.search_symbols_substring(repo_or_module, 50)?;
            match candidates.into_iter().next() {
                Some(n) => n,
                None => return Ok(vec![]),
            }
        }
    };

    let mut visited: HashSet<String> = HashSet::new();
    let mut frontier: Vec<String> = vec![start.id.clone()];
    let mut result: Vec<Node> = Vec::new();

    for _ in 0..depth {
        let mut next: Vec<String> = Vec::new();
        for id in &frontier {
            for e in store.outgoing_edges(id, EdgeKind::Imports)? {
                if visited.insert(e.to.clone()) {
                    if let Some(n) = store.get_node(&e.to)? {
                        result.push(n);
                        next.push(e.to);
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
