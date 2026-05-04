//! find_idl_impl: given an IDL method name, return code locations that
//! implement (server-side) or invoke (client-side) it across all indexed repos.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::core::schema::JSON_SCHEMA_FIND_IDL_IMPL;
use crate::graph::model::EdgeKind;
use crate::graph::store::Store;

#[derive(Debug, Deserialize, Default)]
pub struct FindIdlImplArgs {
    pub method: String,
    #[serde(default)]
    pub service: Option<String>,
    #[serde(default = "default_true")]
    pub include_invokes: bool,
    #[serde(default = "default_true")]
    pub include_implements: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize)]
pub struct FindIdlImplResult {
    pub schema_version: &'static str,
    pub method: Option<IdlMethodInfo>,
    pub implements: Vec<ImplLocation>,
    pub invokes: Vec<ImplLocation>,
}

#[derive(Debug, Serialize)]
pub struct IdlMethodInfo {
    pub repo: String,
    pub path: String,
    pub symbol: String,
    pub line: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct ImplLocation {
    pub repo: String,
    pub path: String,
    pub symbol: Option<String>,
    pub confidence: f32,
}

pub fn find_idl_impl(store: &Store, args: &FindIdlImplArgs) -> Result<FindIdlImplResult> {
    let candidates = store.find_idl_methods_by_name(&args.method, args.service.as_deref())?;

    if candidates.is_empty() {
        return Ok(FindIdlImplResult {
            schema_version: JSON_SCHEMA_FIND_IDL_IMPL,
            method: None,
            implements: vec![],
            invokes: vec![],
        });
    }

    // Take first candidate (best single match)
    let idl_method_node = &candidates[0];
    let method_info = IdlMethodInfo {
        repo: idl_method_node.repo.clone(),
        path: idl_method_node.path.clone(),
        symbol: idl_method_node.symbol.clone().unwrap_or_default(),
        line: idl_method_node.loc_start,
    };

    let mut implements = Vec::new();
    let mut invokes = Vec::new();

    // Gather Implements edges pointing to this IDL method
    if args.include_implements {
        let edges = store.incoming_edges(&idl_method_node.id, EdgeKind::Implements)?;
        for edge in edges {
            if let Some(n) = store.get_node(&edge.from)? {
                implements.push(ImplLocation {
                    repo: n.repo,
                    path: n.path,
                    symbol: n.symbol,
                    confidence: edge.confidence,
                });
            }
        }
    }

    // Gather Invokes edges pointing to this IDL method
    if args.include_invokes {
        let edges = store.incoming_edges(&idl_method_node.id, EdgeKind::Invokes)?;
        for edge in edges {
            if let Some(n) = store.get_node(&edge.from)? {
                invokes.push(ImplLocation {
                    repo: n.repo,
                    path: n.path,
                    symbol: n.symbol,
                    confidence: edge.confidence,
                });
            }
        }
    }

    // Sort by confidence descending
    implements.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    invokes.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(FindIdlImplResult {
        schema_version: JSON_SCHEMA_FIND_IDL_IMPL,
        method: Some(method_info),
        implements,
        invokes,
    })
}
