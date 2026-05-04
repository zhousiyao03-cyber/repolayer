use crate::config::Config;
use crate::indexer::Indexer;
use anyhow::Result;
use git2::Repository;
use std::collections::HashSet;
use std::path::PathBuf;
use tracing::{info, warn};

pub fn update(workspace_root: PathBuf, db_path: PathBuf, config: Config) -> Result<()> {
    let mut indexer = Indexer::new(workspace_root.clone(), db_path, config.clone())?;

    // Identify changed files in each repo via git diff (working tree vs HEAD)
    let mut changed: Vec<(String, PathBuf)> = Vec::new(); // (repo_name, abs_path)
    for r in &config.repos {
        if r.is_idl() {
            // For IDL, fall back to full re-index (rare in practice)
            continue;
        }
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
        match Repository::open(&root) {
            Ok(repo) => {
                let head_tree = repo.head().and_then(|h| h.peel_to_tree()).ok();
                let diff = repo.diff_tree_to_workdir_with_index(head_tree.as_ref(), None)?;
                let mut paths: Vec<PathBuf> = Vec::new();
                diff.foreach(
                    &mut |delta, _| {
                        if let Some(p) = delta.new_file().path() {
                            paths.push(root.join(p));
                        }
                        true
                    },
                    None,
                    None,
                    None,
                )?;
                for p in paths {
                    changed.push((repo_name.clone(), p));
                }
            }
            Err(_) => {
                info!("{} is not a git repo, skipping incremental", root.display());
            }
        }
    }

    if changed.is_empty() {
        println!("no changes detected");
        return Ok(());
    }

    info!("re-indexing {} changed files", changed.len());

    // Per-file reindex for index.db + outline.db (and per-file delete from deps/search)
    for (repo_name, path) in &changed {
        if let Err(e) = indexer.reindex_file(repo_name, path) {
            warn!("reindex {} failed: {}", path.display(), e);
        }
    }

    // Collect all affected non-IDL repos for bulk deps + search rebuild
    let affected_repos: HashSet<String> = changed
        .iter()
        .map(|(r, _)| r.clone())
        .collect();

    for repo_name in &affected_repos {
        // Find the repo config entry (match by resolved name)
        let cfg_repo = config.repos.iter().find(|r| {
            let root = if r.path.is_absolute() {
                r.path.clone()
            } else {
                workspace_root.join(&r.path)
            };
            let name = r.name.clone().unwrap_or_else(|| {
                root.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default()
            });
            name == *repo_name
        });
        let Some(rcfg) = cfg_repo else {
            continue;
        };
        if rcfg.is_idl() {
            continue; // IDL repos don't have deps/search graphs
        }
        let root = if rcfg.path.is_absolute() {
            rcfg.path.clone()
        } else {
            workspace_root.join(&rcfg.path)
        };

        // Rebuild deps graph for the whole repo (correct but slow — TODO v0.2.1: per-file)
        match crate::deps::build_for_repo(&root) {
            Ok(graph) => {
                if let Err(e) = indexer.deps_store.replace_repo_graph(repo_name, &graph) {
                    warn!("deps replace failed for {}: {}", repo_name, e);
                }
            }
            Err(e) => warn!("deps::build_for_repo({}) failed: {}", repo_name, e),
        }

        // Rebuild search chunks for the whole repo (correct but slow — TODO v0.2.1: per-file)
        let outline_files = match indexer.outline_store.list_files(repo_name) {
            Ok(f) => f,
            Err(e) => {
                warn!("outline_store.list_files({}) failed: {}", repo_name, e);
                continue;
            }
        };
        let mut all_chunks = Vec::new();
        for (_, rel) in &outline_files {
            let abs = root.join(rel);
            let chunks = crate::search::chunker::chunk_file(&abs, rel);
            all_chunks.extend(chunks);
        }
        if let Err(e) = indexer.search_store.replace_repo_chunks(repo_name, &all_chunks) {
            warn!("search_store write failed for {}: {}", repo_name, e);
        }
    }

    println!("updated {} files", changed.len());
    Ok(())
}
