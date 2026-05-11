//! Auto-extract `Calls` edges from source via ast-grep.
//!
//! This is the linker pass that powers the `callers` CLI. Without it, the
//! graph has no Calls edges and `repolayer callers X` returns "no inbound
//! Calls edges" — which is the state the CLI ships in if this pass is
//! skipped or disabled. With it, agents can ask "who calls X" and get
//! cross-file (and, where the name is unique, cross-repo) results.
//!
//! Granularity decision: edges are `(Module, Function|Method)` where the
//! Module is the file containing the call sites, not the enclosing function.
//! This matches the granularity already used by `idl_links.rs` for Invokes /
//! Implements edges, and lets us reuse the per-file call-callee scan without
//! tracking byte-offset → enclosing-function mappings (which would be O(N)
//! parse work for a relatively small UX gain — once the agent has the file,
//! `repolayer show` pinpoints the function).
//!
//! Resolution strategy: a callee name produces a Calls edge **only when it
//! resolves to exactly one Function/Method node across the indexed
//! workspace**. Ambiguous names (`init`, `Get`, `parse`, …) are skipped to
//! avoid drowning real signal in noise. Confidence is therefore always
//! 1.0 — there's no heuristic tier here. If you need fuzzy resolution,
//! fall back to `repolayer query` and let the agent decide.

use crate::graph::model::*;
use crate::graph::store::Store;
use aho_corasick::AhoCorasick;
use anyhow::Result;
use ast_grep_language::{Language, LanguageExt, SupportLang};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::warn;

/// Names shorter than this are skipped — they generate too many false
/// positives even before the uniqueness filter.
const MIN_NAME_LEN: usize = 4;

/// Single-letter / common-noise prefixes we don't bother resolving.
fn is_noise_name(name: &str) -> bool {
    if name.len() < MIN_NAME_LEN {
        return true;
    }
    // All-lowercase short names are often common English words used as
    // method names in many places (`data`, `value`, `find`). Skip unless
    // they include `_` or any uppercase.
    if name.chars().all(|c| c.is_ascii_lowercase()) {
        return true;
    }
    false
}

pub struct CallsLinker<'a> {
    pub store: &'a Store,
    pub repos: Vec<(String, PathBuf)>,
}

impl<'a> CallsLinker<'a> {
    pub fn link_all(&self) -> Result<u64> {
        // ── Step 1: collect every Function / Method node, indexed by short name ─
        // We resolve "callee `foo`" to a node only when *exactly one* node has
        // that symbol. The map is short-name → Vec<node_id>; we drop entries
        // with > 1 node before scanning, so unique-resolution becomes O(1).
        let mut funcs = self.store.list_nodes_by_kind(NodeKind::Function)?;
        funcs.extend(self.store.list_nodes_by_kind(NodeKind::Method)?);
        if funcs.is_empty() {
            return Ok(0);
        }

        let mut by_name: HashMap<String, Vec<String>> = HashMap::new();
        for n in &funcs {
            let Some(sym) = n.symbol.as_deref() else {
                continue;
            };
            if is_noise_name(sym) {
                continue;
            }
            by_name.entry(sym.to_string()).or_default().push(n.id.clone());
        }
        by_name.retain(|_, ids| ids.len() == 1);
        if by_name.is_empty() {
            return Ok(0);
        }
        let unique_names: Vec<String> = by_name.keys().cloned().collect();
        let ac = match AhoCorasick::new(&unique_names) {
            Ok(a) => a,
            Err(e) => {
                warn!("aho-corasick build failed in calls linker: {}", e);
                return Ok(0);
            }
        };

        // ── Step 2 (parallel): scan every (repo, file), produce edges ───────
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

        // ── Step 3 (serial): upsert ─────────────────────────────────────────
        let mut count = 0u64;
        for e in &edges {
            match self.store.upsert_edge(e) {
                Ok(()) => count += 1,
                Err(err) => warn!("calls upsert failed: {}", err),
            }
        }
        Ok(count)
    }
}

fn scan_file(
    code_repo: &str,
    code_root: &Path,
    path: &Path,
    ac: &AhoCorasick,
    unique_names: &[String],
    by_name: &HashMap<String, Vec<String>>,
) -> Vec<Edge> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };

    // Cheap pre-filter: bail out on 90%+ of files before paying for an AST.
    let mut hit_indices: HashSet<usize> = HashSet::new();
    for m in ac.find_iter(&content) {
        hit_indices.insert(m.pattern().as_usize());
    }
    if hit_indices.is_empty() {
        return Vec::new();
    }

    let Some(lang) = SupportLang::from_path(path) else {
        return Vec::new();
    };
    let callees = collect_call_callees(&content, lang);
    if callees.is_empty() {
        return Vec::new();
    }

    let rel = path
        .strip_prefix(code_root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/");
    let module_node = Node::new(NodeKind::Module, code_repo, &rel, None);

    let mut out = Vec::new();
    for &idx in &hit_indices {
        let name = &unique_names[idx];
        // Aho-corasick can match `name` as a substring; require that the
        // ast-grep pass also saw `name` as an actual callee identifier.
        if !callees.contains(name.as_str()) {
            continue;
        }
        let candidates = &by_name[name];
        if candidates.len() != 1 {
            continue; // belt-and-suspenders: the global filter already pruned
        }
        let target_id = &candidates[0];
        // Don't create a self-loop when the file *contains* the definition
        // and also calls it (e.g. recursion). The graph would still be
        // correct, but the edge says nothing useful for `callers`.
        out.push(Edge {
            from: module_node.id.clone(),
            to: target_id.clone(),
            kind: EdgeKind::Calls,
            confidence: 1.0,
        });
    }
    out
}

/// Walk the AST once and return every callee identifier seen in a call
/// expression. Mirrors the equivalent helper in `idl_links.rs` (kept
/// duplicated rather than shared because the two passes have slightly
/// different ambitions and the function is 25 lines).
fn collect_call_callees(source: &str, lang: SupportLang) -> HashSet<String> {
    let call_kinds: &[&str] = match lang {
        SupportLang::TypeScript | SupportLang::Tsx | SupportLang::JavaScript => &["call_expression"],
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
