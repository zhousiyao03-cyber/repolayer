use crate::config::Config;
use crate::indexer::Indexer;
use anyhow::Result;
use git2::Repository;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{info, warn};

pub async fn update(workspace_root: PathBuf, db_path: PathBuf, config: Config) -> Result<()> {
    let mut indexer = Indexer::new(workspace_root.clone(), db_path, config.clone())?;

    // Identify changed files in each repo via git diff (working tree vs HEAD)
    // Group by repo so the per-repo passes below see all of that repo's changes at once.
    let mut by_repo: HashMap<String, RepoChanges> = HashMap::new();
    for r in &config.repos {
        if r.is_idl() {
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
                if !paths.is_empty() {
                    by_repo
                        .entry(repo_name.clone())
                        .or_insert_with(|| RepoChanges {
                            root: root.clone(),
                            files: Vec::new(),
                        })
                        .files
                        .extend(paths);
                }
            }
            Err(_) => {
                info!("{} is not a git repo, skipping incremental", root.display());
            }
        }
    }

    let total_changed: usize = by_repo.values().map(|c| c.files.len()).sum();
    if total_changed == 0 {
        println!("no changes detected");
        return Ok(());
    }
    info!("re-indexing {} changed files", total_changed);

    // Per-file reindex of index.db + outline.db (existing path).
    for (repo_name, ch) in &by_repo {
        for path in &ch.files {
            if let Err(e) = indexer.reindex_file(repo_name, path) {
                warn!("reindex {} failed: {}", path.display(), e);
            }
        }
    }

    // Per-file deps + search refresh, one repo at a time so SuffixIndex is shared.
    for (repo_name, ch) in &by_repo {
        // ── deps: build only the changed files, reusing one SuffixIndex ──────
        match crate::deps::build_for_files(&ch.root, &ch.files) {
            Ok(results) => {
                for r in results {
                    let key = r.file.to_string_lossy();
                    if let Err(e) = indexer.deps_store.upsert_file_edges(
                        repo_name,
                        key.as_ref(),
                        &r.edges,
                        &r.external,
                    ) {
                        warn!("deps upsert_file_edges {} failed: {}", r.file.display(), e);
                    }
                }
                // Files that were deleted from disk (or whose language is now
                // unrecognised) won't show up in `results`. Wipe their rows
                // explicitly so stale edges don't linger.
                let resolved_keys: std::collections::HashSet<String> = ch
                    .files
                    .iter()
                    .filter(|p| p.exists())
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect();
                for path in &ch.files {
                    let key = path.to_string_lossy().into_owned();
                    if !resolved_keys.contains(&key) {
                        if let Err(e) = indexer.deps_store.delete_file(repo_name, &key) {
                            warn!("deps delete_file {} failed: {}", path.display(), e);
                        }
                    }
                }
            }
            Err(e) => warn!("deps::build_for_files({}) failed: {}", repo_name, e),
        }

        // ── search: re-chunk only the changed files. search.db stores chunk
        // paths exactly as `Indexer::build_all` does — `repo_path.join(rel)`
        // where `rel` is whatever ParseResult.path is (absolute, set by the
        // adapters). Mirror that here so unchanged files keep their chunk
        // ids stable across builds.
        let mut new_chunk_ids: Vec<i64> = Vec::new();
        for path in &ch.files {
            let key = path.to_string_lossy();
            if let Err(e) = indexer.search_store.delete_file(repo_name, key.as_ref()) {
                warn!("search delete_file {} failed: {}", path.display(), e);
                continue;
            }
            if !path.exists() {
                continue;
            }
            let chunks = crate::search::chunker::chunk_file(path, key.as_ref());
            match indexer.search_store.insert_file_chunks(repo_name, &chunks) {
                Ok(ids) => new_chunk_ids.extend(ids),
                Err(e) => warn!("search insert_file_chunks {} failed: {}", path.display(), e),
            }
        }

        // ── search: refresh embeddings for the new chunks if the model is
        // already cached. Mirrors the cache-only policy in the build path —
        // we never download here, since `update` should be cheap.
        if !new_chunk_ids.is_empty() {
            if let Err(e) = embed_chunks_if_cached(&mut indexer.search_store, &new_chunk_ids) {
                warn!("incremental embedding failed (continuing): {}", e);
            }
        }
    }

    // Per-file summary refresh (only when summary.enabled). We re-summarise
    // exactly the changed files, not the whole repo — keeps `update` from
    // burning thousands of LLM calls per day.
    if let (Some(emb_cfg), Some(sum_cfg)) = (
        indexer.config.embedding.as_ref(),
        indexer.config.summary.as_ref(),
    ) {
        if sum_cfg.enabled {
            // Build (repo, path) pairs using the same path key the chunk lane
            // writes — absolute-path string from `path.to_string_lossy()` —
            // so `run_summary_phase_for_files`' filter matches.
            let mut changed_pairs: Vec<(String, String)> = Vec::new();
            for (repo_name, ch) in &by_repo {
                for path in &ch.files {
                    changed_pairs.push((repo_name.clone(), path.to_string_lossy().into_owned()));
                }
            }
            match crate::llm::summary_store::run_summary_phase_for_files(
                &indexer.search_store,
                &changed_pairs,
                emb_cfg,
                sum_cfg,
            )
            .await
            {
                Ok(n) => info!("incremental summary: refreshed {n} files"),
                Err(e) => warn!("incremental summary failed: {e}"),
            }
        }
    }

    println!("updated {} files", total_changed);
    Ok(())
}

struct RepoChanges {
    root: PathBuf,
    files: Vec<PathBuf>,
}

/// Encode a set of chunk ids and write the resulting vectors to `chunk_vec`.
/// Returns `Ok(())` and skips silently if the embedding model isn't already
/// cached on disk — `update` should never block on a 64 MB download.
fn embed_chunks_if_cached(
    store: &mut crate::search::store::SearchStore,
    ids: &[i64],
) -> Result<()> {
    use crate::search::download::{ensure_model, ModelInfo};
    use crate::search::embed::Embedder;

    // Cache-only check: same logic as cached_model_present() in indexer/mod.rs,
    // but local so we don't have to pub it.
    let env_dir = std::env::var("AST_OUTLINE_MODEL_DIR")
        .ok()
        .map(std::path::PathBuf::from);
    let model_dir = match env_dir {
        Some(d) => d,
        None => match dirs::cache_dir() {
            Some(c) => c.join("ast-outline").join("models").join("potion-code-16m"),
            None => return Ok(()), // no cache dir; nothing we can do
        },
    };
    let cached = model_dir.join("model.safetensors").is_file()
        && model_dir.join("tokenizer.json").is_file()
        && model_dir.join("manifest.json").is_file();
    if !cached {
        return Ok(()); // skip silently — search keeps working via BM25/substring
    }

    let info = ModelInfo::potion_code_16m();
    let dir = ensure_model(&info).map_err(|e| anyhow::anyhow!("ensure_model failed: {}", e))?;
    let embedder = Embedder::open(&dir).map_err(|e| anyhow::anyhow!("loading embedder: {}", e))?;

    // Pull (id, content) for the ids we just inserted.
    let placeholders: String = ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("?{}", i + 1))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!("SELECT id, content FROM chunks WHERE id IN ({placeholders})");
    let conn = store.conn();
    let mut stmt = conn.prepare(&sql)?;
    let params_dyn: Vec<&dyn rusqlite::ToSql> =
        ids.iter().map(|i| i as &dyn rusqlite::ToSql).collect();
    let rows = stmt.query_map(params_dyn.as_slice(), |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    let collected: rusqlite::Result<Vec<(i64, String)>> = rows.collect();
    let collected = collected?;
    drop(stmt);
    let pairs: Vec<(i64, Vec<f32>)> = collected
        .into_iter()
        .map(|(id, content)| (id, embedder.encode_one(&content).to_vec()))
        .collect();
    store.upsert_embeddings_batch(&pairs)?;
    Ok(())
}
