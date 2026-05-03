use crate::graph::model::*;
use crate::graph::store::Store;
use anyhow::Result;
use std::path::PathBuf;
use tracing::warn;

pub struct IdlLinker<'a> {
    pub store: &'a Store,
    pub repos: Vec<(String, PathBuf)>, // (repo_name, abs_root)
}

impl<'a> IdlLinker<'a> {
    pub fn link_all(&self) -> Result<u64> {
        let methods = self.collect_idl_methods()?;
        if methods.is_empty() {
            return Ok(0);
        }
        let mut count = 0u64;

        for (code_repo, code_root) in &self.repos {
            for entry in ignore::WalkBuilder::new(code_root).build().flatten() {
                if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                    continue;
                }
                let path = entry.path();
                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                if !matches!(ext, "ts" | "tsx" | "js" | "jsx" | "py" | "go") {
                    continue;
                }
                let content = match std::fs::read_to_string(path) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let rel = path
                    .strip_prefix(code_root)
                    .unwrap_or(path)
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                let module_node = Node::new(NodeKind::Module, code_repo, &rel, None);

                let is_server_side = path_suggests_server(&rel);

                for (qualified, m_node_id) in &methods {
                    // qualified is "ServiceName.MethodName" — extract short method name
                    let short = qualified.split('.').next_back().unwrap_or(qualified);
                    if content.contains(short) {
                        let edge_kind = if is_server_side {
                            EdgeKind::Implements
                        } else {
                            EdgeKind::Invokes
                        };
                        match self.store.upsert_edge(&Edge {
                            from: module_node.id.clone(),
                            to: m_node_id.clone(),
                            kind: edge_kind,
                        }) {
                            Ok(()) => count += 1,
                            Err(e) => warn!("idl_link upsert failed: {}", e),
                        }
                    }
                }
            }
        }
        Ok(count)
    }

    fn collect_idl_methods(&self) -> Result<Vec<(String, String)>> {
        // returns Vec<(qualified_name, node_id)>
        let nodes = self.store.list_nodes_by_kind(NodeKind::IdlMethod)?;
        Ok(nodes
            .into_iter()
            .filter_map(|n| n.symbol.clone().map(|s| (s, n.id)))
            .collect())
    }
}

fn path_suggests_server(rel: &str) -> bool {
    let l = rel.to_lowercase();
    [
        "service/",
        "services/",
        "handler/",
        "handlers/",
        "impl/",
        "server/",
    ]
    .iter()
    .any(|p| l.contains(p))
}
