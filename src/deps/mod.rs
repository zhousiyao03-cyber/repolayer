//! File-level dependency graph (forward + reverse + cycles + DSM).
//! Adopted from aeroxy/ast-outline `src/deps/`. Fully implemented in
//! Tasks B-5 to B-11.

pub mod cache;
pub mod dsm;
pub mod extract;
pub mod graph;
pub mod manifest;
pub mod options;
pub mod render;
pub mod resolver;
pub mod scc;
pub mod store;
pub mod traverse;

pub use extract::{extract, RawImport};
pub use graph::{DepEdge, DepGraph, ImportKind};
pub use manifest::{detect_aliases, ProjectAliases, PythonPackage, RustPackage};
pub use options::DepError;
pub use resolver::{build_suffix_index, resolve, ResolveCtx};
pub use store::DepStore;

use rayon::prelude::*;
use std::path::Path;

/// Build a [`DepGraph`] for a single repository by walking all source files,
/// extracting raw imports, and resolving each import to a target file inside
/// `root`.
///
/// Uses rayon for parallel parse + resolve; the final `DepGraph` assembly is
/// single-threaded because `HashMap` is not `Sync`. `dedup_edges` is run at
/// the end to collapse duplicates that arise when multiple import statements
/// in the same file point to the same target.
///
/// # Errors
/// Returns [`DepError`] if `root` cannot be walked (currently infallible, but
/// typed so the caller's signature is forwards-compatible with B-23 which may
/// surface I/O errors).
pub fn build_for_repo(root: &Path) -> Result<DepGraph, DepError> {
    let aliases = detect_aliases(root);
    let idx = build_suffix_index(root);

    // Collect all files known to the index into a Vec so we can par_iter.
    let files: Vec<_> = idx.by_file.keys().cloned().collect();

    // Parallel extract + resolve phase.
    // Each element: (source_file, edges_to_internal_files, external_specs).
    let resolved: Vec<(std::path::PathBuf, Vec<DepEdge>, Vec<String>)> = files
        .par_iter()
        .map(|file| {
            let info = match idx.by_file.get(file) {
                Some(i) => i,
                None => return (file.clone(), Vec::new(), Vec::new()),
            };
            let raw_imports = extract(file, info.language);
            let mut edges: Vec<DepEdge> = Vec::new();
            let mut external: Vec<String> = Vec::new();
            let ctx = ResolveCtx {
                from_file: file,
                lang: info.language,
                alias_prefix: aliases.go_module.as_deref(),
                path_aliases: &aliases.ts_path_aliases,
            };
            for ri in raw_imports {
                match resolve(&ri.spec, &ctx, &idx) {
                    Some(target) if target != *file => {
                        edges.push(DepEdge {
                            target,
                            kind: ri.kind,
                            line: ri.line,
                            local_name: ri.local_name,
                            raw_path: ri.raw_path,
                        });
                    }
                    _ => {
                        // Unresolvable or self-referential — record as external.
                        external.push(ri.raw_path.unwrap_or(ri.spec));
                    }
                }
            }
            (file.clone(), edges, external)
        })
        .collect();

    // Single-threaded assembly into DepGraph.
    let mut g = DepGraph::empty(root.to_path_buf());
    for (file, edges, external) in resolved {
        g.forward.insert(file.clone(), edges);
        if !external.is_empty() {
            g.external.insert(file, external);
        }
    }
    graph::dedup_edges(&mut g);
    Ok(g)
}
