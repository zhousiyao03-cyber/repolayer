use crate::graph::model::*;
use crate::graph::store::Store;
use anyhow::Result;
use ast_grep_language::{Language, LanguageExt, SupportLang};
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
                if !matches!(ext, "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "rs") {
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
                let lang_opt = SupportLang::from_path(path);

                for (qualified, m_node_id) in &methods {
                    // qualified is "ServiceName.MethodName" — extract short method name
                    let short = qualified.split('.').next_back().unwrap_or(qualified);

                    let confidence: Option<f32> = if let Some(lang) = lang_opt {
                        if has_call_to_method(&content, lang, short) {
                            // ast-grep found an actual call expression → high confidence
                            Some(0.7)
                        } else if content.contains(short) && is_server_side {
                            // literal hit in a server-side path → low-confidence fallback
                            Some(0.4)
                        } else {
                            None
                        }
                    } else {
                        // Unknown language — fall back to old heuristic
                        if content.contains(short) {
                            Some(0.4)
                        } else {
                            None
                        }
                    };

                    if let Some(conf) = confidence {
                        let edge_kind = if is_server_side {
                            EdgeKind::Implements
                        } else {
                            EdgeKind::Invokes
                        };
                        match self.store.upsert_edge(&Edge {
                            from: module_node.id.clone(),
                            to: m_node_id.clone(),
                            kind: edge_kind,
                            confidence: conf,
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

/// Returns true if `source` contains an actual call expression invoking `method_name`.
///
/// Uses ast-grep to parse the source and walk the AST looking for call_expression
/// (or equivalent) nodes that reference the given method name as a callee identifier.
/// This avoids false positives from string literals, comments, or unrelated identifiers.
pub fn has_call_to_method(source: &str, lang: SupportLang, method_name: &str) -> bool {
    // Node kinds that represent call expressions in each language.
    let call_kinds: &[&str] = match lang {
        SupportLang::TypeScript | SupportLang::Tsx | SupportLang::JavaScript => {
            &["call_expression"]
        }
        SupportLang::Python => &["call"],
        SupportLang::Go => &["call_expression"],
        SupportLang::Rust => &["call_expression", "method_call_expression"],
        _ => return false,
    };

    let grep = lang.ast_grep(source);
    let root = grep.root();

    for node in root.dfs() {
        let kind = node.kind();
        if !call_kinds.contains(&kind.as_ref()) {
            continue;
        }
        // Walk the direct children of the call node and one level of grandchildren.
        // This handles:
        //   - bare call:   method_name(...)          → child identifier == method_name
        //   - method call: receiver.method_name(...) → grandchild property_identifier == method_name
        for child in node.children() {
            if child.text().as_ref() == method_name {
                return true;
            }
            for grandchild in child.children() {
                if grandchild.text().as_ref() == method_name {
                    return true;
                }
            }
        }
    }
    false
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
