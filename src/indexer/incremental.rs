use crate::config::Config;
use crate::indexer::Indexer;
use anyhow::Result;
use git2::Repository;
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
    for (repo_name, path) in &changed {
        if let Err(e) = indexer.reindex_file(repo_name, path) {
            warn!("reindex {} failed: {}", path.display(), e);
        }
    }
    println!("updated {} files", changed.len());
    Ok(())
}
