//! Cross-repo linker driven by `deps.db.external_imports`.
//!
//! The IDL-name based linker (`idl_links.rs`) emits low-confidence edges
//! based on substring/short-name heuristics. This module produces
//! **high-confidence** edges by following actual `import` statements:
//!
//! 1. For every workspace repo, derive one or more **module path prefixes**:
//!    - Go repos: read the `module` line from `go.mod`.
//!    - Any repo: also include user-declared `module_aliases` from the YAML
//!      config (typically used for IDL repos whose generated Go SDK lives
//!      under a separate module path, e.g. `http_idl` → `http_idl_gen`).
//!
//! 2. Walk every row in `deps.db.external_imports`. If the imported module
//!    path matches one of the prefixes (longest-prefix wins), emit an
//!    `Imports` edge from the importing module node to the target repo's
//!    Repo node, at confidence 0.9.
//!
//! Why a Repo-level edge rather than a file-level one: the imported `raw`
//! is a Go *package* path (e.g. `github.com/example/gen/foo/bar/baz`),
//! and resolving that to a specific source file inside the target workspace
//! repo requires Go module layout knowledge we don't (yet) bake in. The
//! Repo-level edge is precise enough for "which workspace repos does this
//! file actually call?" and lets `find_context` surface the dependency.
//!
//! confidence: 0.9 (one notch below 1.0 because the edge is generated from
//! a textual import path; a future "link to specific file" pass can promote
//! the relevant edges to 1.0).

use crate::config::Config;
use crate::deps::store::DepStore;
use crate::graph::model::{Edge, EdgeKind, Node, NodeKind};
use crate::graph::store::Store;
use anyhow::Result;
use std::path::Path;
use tracing::warn;

/// Aggregated alias info for one workspace repo.
struct RepoAlias {
    repo_name: String,
    /// All module-path prefixes that resolve into this repo.
    prefixes: Vec<String>,
}

/// Build the alias table from config + each repo's `go.mod` (when present).
fn build_aliases(workspace_root: &Path, config: &Config) -> Vec<RepoAlias> {
    let mut out = Vec::new();
    for r in &config.repos {
        let root = if r.path.is_absolute() {
            r.path.clone()
        } else {
            workspace_root.join(&r.path)
        };
        let repo_name = r.name.clone().unwrap_or_else(|| {
            root.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "repo".to_string())
        });

        let mut prefixes: Vec<String> = r.module_aliases.clone();

        // Try to auto-detect from go.mod for Go repos.
        if let Ok(content) = std::fs::read_to_string(root.join("go.mod")) {
            for line in content.lines() {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix("module ") {
                    let module = rest.trim().trim_matches('"').to_string();
                    if !module.is_empty() && !prefixes.contains(&module) {
                        prefixes.push(module);
                    }
                    break;
                }
            }
        }

        if !prefixes.is_empty() {
            out.push(RepoAlias {
                repo_name,
                prefixes,
            });
        }
    }
    out
}

/// Match an imported module path to the longest-prefix repo. Returns the
/// matched repo's name on success.
fn longest_match<'a>(aliases: &'a [RepoAlias], import: &str) -> Option<&'a str> {
    let mut best: Option<(&str, usize)> = None;
    for a in aliases {
        for p in &a.prefixes {
            if import == p || import.starts_with(&format!("{}/", p)) {
                let len = p.len();
                if best.map(|(_, n)| len > n).unwrap_or(true) {
                    best = Some((a.repo_name.as_str(), len));
                }
            }
        }
    }
    best.map(|(name, _)| name)
}

/// Convert an absolute path to a forward-slash relative path under `root`.
fn relpath(abs: &Path, root: &Path) -> String {
    abs.strip_prefix(root)
        .unwrap_or(abs)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Run the import-based linker. Returns the number of edges emitted.
pub fn link(
    store: &Store,
    deps_store: &DepStore,
    workspace_root: &Path,
    config: &Config,
) -> Result<u64> {
    let aliases = build_aliases(workspace_root, config);
    if aliases.is_empty() {
        return Ok(0);
    }

    // Map of repo_name → workspace root so we can compute relpath for
    // each `from_path` (which deps.db stores as an absolute path).
    let repo_roots: Vec<(String, std::path::PathBuf)> = config
        .repos
        .iter()
        .map(|r| {
            let root = if r.path.is_absolute() {
                r.path.clone()
            } else {
                workspace_root.join(&r.path)
            };
            let name = r.name.clone().unwrap_or_else(|| {
                root.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "repo".to_string())
            });
            (name, root)
        })
        .collect();

    let imports = deps_store.list_external_imports()?;

    let mut count = 0u64;
    for (from_repo, from_abs, raw) in imports {
        let target = match longest_match(&aliases, &raw) {
            Some(t) => t,
            None => continue,
        };
        if target == from_repo {
            continue; // self-import, no cross-repo edge
        }

        // Find the source file's owning workspace root so we can compute
        // a stable relative path for the module node.
        let root = repo_roots
            .iter()
            .find_map(|(n, r)| (*n == from_repo).then_some(r.as_path()));
        let Some(root) = root else { continue };
        let rel = relpath(Path::new(&from_abs), root);

        let from_module = Node::new(NodeKind::Module, &from_repo, &rel, None);
        let to_repo = Node::new(NodeKind::Repo, target, "", None);

        if let Err(e) = store.upsert_edge(&Edge {
            from: from_module.id,
            to: to_repo.id,
            kind: EdgeKind::Imports,
            confidence: 0.9,
        }) {
            warn!("imports_to_repo upsert failed: {}", e);
            continue;
        }
        count += 1;
    }
    Ok(count)
}
