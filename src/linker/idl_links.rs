use crate::graph::model::*;
use crate::graph::store::Store;
use aho_corasick::AhoCorasick;
use anyhow::Result;
use ast_grep_language::{Language, LanguageExt, SupportLang};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::warn;

pub struct IdlLinker<'a> {
    pub store: &'a Store,
    pub repos: Vec<(String, PathBuf)>, // (repo_name, abs_root)
}

/// Minimum length for a method short name to participate in linking. Names
/// shorter than this (e.g. Go-style `Do`, `On`) generate too many false
/// positives; require qualified usage if needed.
const MIN_NAME_LEN: usize = 4;

impl<'a> IdlLinker<'a> {
    pub fn link_all(&self) -> Result<u64> {
        let methods = self.collect_idl_methods()?;
        if methods.is_empty() {
            return Ok(0);
        }

        // ── Build the dedup-by-short-name index ────────────────────────────
        // Many IDL methods share short names (e.g. `Get`, `List`). Collapse
        // to unique short names; remember every node id behind each name.
        let mut by_name: HashMap<String, Vec<String>> = HashMap::new();
        for (qualified, node_id) in &methods {
            let short = qualified
                .split('.')
                .next_back()
                .unwrap_or(qualified.as_str());
            if short.len() < MIN_NAME_LEN {
                continue;
            }
            // Heuristic noise filter: also skip names that are pure lowercase
            // common English words (e.g. `data`, `info`) which are highly
            // ambiguous. Keep names with any uppercase or `_`.
            if short.chars().all(|c| c.is_ascii_lowercase()) {
                continue;
            }
            by_name
                .entry(short.to_string())
                .or_default()
                .push(node_id.clone());
        }
        if by_name.is_empty() {
            return Ok(0);
        }
        let unique_names: Vec<String> = by_name.keys().cloned().collect();
        let ac = match AhoCorasick::new(&unique_names) {
            Ok(a) => a,
            Err(e) => {
                warn!("aho-corasick build failed: {}", e);
                return Ok(0);
            }
        };

        // ── Phase 1 (parallel): walk every (repo, file) and produce edges ──
        // Walking ignore::Walk respects .gitignore; the walk itself is cheap,
        // so we collect file paths first and parallelise the per-file work
        // (read + aho-corasick + ast-grep), which is the actual hot path.
        let file_targets: Vec<(String, PathBuf, PathBuf)> = self
            .repos
            .iter()
            .flat_map(|(repo, root)| {
                ignore::WalkBuilder::new(root)
                    .build()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                    .filter_map(|e| {
                        let p = e.path().to_path_buf();
                        let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("");
                        matches!(ext, "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "rs")
                            .then(|| (repo.clone(), root.clone(), p))
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        let edges: Vec<Edge> = file_targets
            .par_iter()
            .flat_map_iter(|(repo, root, path)| {
                scan_file(repo, root, path, &ac, &unique_names, &by_name).into_iter()
            })
            .collect();

        // ── Phase 2 (serial): write to SQLite ──────────────────────────────
        let mut count = 0u64;
        for e in &edges {
            match self.store.upsert_edge(e) {
                Ok(()) => count += 1,
                Err(err) => warn!("idl_link upsert failed: {}", err),
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

/// Process one source file. Pure (no I/O against `Store`), so safe to run
/// from rayon worker threads. Returns every IDL edge this file should produce.
fn scan_file(
    code_repo: &str,
    code_root: &Path,
    path: &Path,
    ac: &AhoCorasick,
    unique_names: &[String],
    by_name: &HashMap<String, Vec<String>>,
) -> Vec<Edge> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    // Cheap pre-filter: aho-corasick single pass.
    let mut hit_indices: HashSet<usize> = HashSet::new();
    for m in ac.find_iter(&content) {
        hit_indices.insert(m.pattern().as_usize());
    }
    if hit_indices.is_empty() {
        return Vec::new(); // 90%+ of files exit here
    }

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

    // Parse the file ONCE; collect ALL call-expression callees.
    let callees: Option<HashSet<String>> =
        lang_opt.map(|lang| collect_call_callees(&content, lang));

    let mut out = Vec::new();
    for &idx in &hit_indices {
        let short = &unique_names[idx];
        let in_call = callees
            .as_ref()
            .map(|set| set.contains(short.as_str()))
            .unwrap_or(false);

        let confidence: Option<f32> = if in_call {
            Some(0.7)
        } else if is_server_side || lang_opt.is_none() {
            // server-side path heuristic, or unknown language (substring is the best we have)
            Some(0.4)
        } else {
            None
        };

        let Some(conf) = confidence else { continue };
        let edge_kind = if is_server_side {
            EdgeKind::Implements
        } else {
            EdgeKind::Invokes
        };

        for m_node_id in &by_name[short] {
            out.push(Edge {
                from: module_node.id.clone(),
                to: m_node_id.clone(),
                kind: edge_kind,
                confidence: conf,
            });
        }
    }
    out
}

/// Walk the AST once and return every callee identifier seen in a call
/// expression. Replaces the per-method-name DFS that was the hot path in
/// the original implementation.
fn collect_call_callees(source: &str, lang: SupportLang) -> HashSet<String> {
    let call_kinds: &[&str] = match lang {
        SupportLang::TypeScript | SupportLang::Tsx | SupportLang::JavaScript => {
            &["call_expression"]
        }
        SupportLang::Python => &["call"],
        SupportLang::Go => &["call_expression"],
        SupportLang::Rust => &["call_expression", "method_call_expression"],
        _ => return HashSet::new(),
    };

    let mut out = HashSet::new();
    let grep = lang.ast_grep(source);
    let root = grep.root();
    for node in root.dfs() {
        let kind = node.kind();
        if !call_kinds.contains(&kind.as_ref()) {
            continue;
        }
        for child in node.children() {
            out.insert(child.text().to_string());
            for grandchild in child.children() {
                out.insert(grandchild.text().to_string());
            }
        }
    }
    out
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
