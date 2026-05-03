use crate::graph::model::*;
use crate::graph::store::Store;
use anyhow::Result;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SymbolResult {
    pub symbol: Node,
    pub callers: Vec<Node>,
    pub callees: Vec<Node>,
}

pub fn get_symbol(store: &Store, name: &str, repo: Option<&str>) -> Result<Option<SymbolResult>> {
    let candidates = store.search_symbols_substring(name, 50)?;
    let exact: Option<Node> = candidates
        .into_iter()
        .find(|n| n.symbol.as_deref() == Some(name) && repo.map(|r| n.repo == r).unwrap_or(true));
    let Some(sym) = exact else {
        return Ok(None);
    };

    let mut callers = Vec::new();
    for e in store.incoming_edges(&sym.id, EdgeKind::Calls)? {
        if let Some(n) = store.get_node(&e.from)? {
            callers.push(n);
        }
    }
    let mut callees = Vec::new();
    for e in store.outgoing_edges(&sym.id, EdgeKind::Calls)? {
        if let Some(n) = store.get_node(&e.to)? {
            callees.push(n);
        }
    }
    Ok(Some(SymbolResult {
        symbol: sym,
        callers,
        callees,
    }))
}
