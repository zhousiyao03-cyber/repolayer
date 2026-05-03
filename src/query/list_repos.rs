use crate::graph::model::*;
use crate::graph::store::Store;
use anyhow::Result;

pub fn list_repos(store: &Store) -> Result<Vec<Node>> {
    store.list_nodes_by_kind(NodeKind::Repo)
}
