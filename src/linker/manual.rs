use crate::config::Config;
use crate::graph::model::*;
use crate::graph::store::Store;
use anyhow::Result;

pub fn apply_manual_links(store: &Store, config: &Config) -> Result<u64> {
    let mut count = 0;
    for link in &config.links {
        let from = Node::new(NodeKind::Repo, &link.from, "", None);
        let to = Node::new(NodeKind::Repo, &link.to, "", None);
        store.upsert_node(&from)?;
        store.upsert_node(&to)?;
        store.upsert_edge(&Edge {
            from: from.id,
            to: to.id,
            kind: match link.kind.as_str() {
                "calls" => EdgeKind::Calls,
                "invokes" => EdgeKind::Invokes,
                _ => EdgeKind::Imports, // generic "depends on"
            },
            confidence: 1.0,
        })?;
        count += 1;
    }
    Ok(count)
}
