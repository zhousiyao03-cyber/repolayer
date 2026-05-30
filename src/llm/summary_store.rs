//! Summary phase orchestrator. Walks each repo's chunks, asks the configured
//! LLM for a brief Chinese summary per module (one summary per file for now;
//! per-function granularity is a v0.3 follow-up), embeds the summaries via
//! the configured embedder, writes to `summary_chunks` + `summary_vec`.

use crate::config::{EmbeddingConfig, SummaryConfig};
use crate::llm::{anthropic::AnthropicProvider, deepseek::DeepSeekProvider, LlmProvider};
use crate::search::embedder::make_embedder;
use crate::search::store::SearchStore;
use crate::search::store_summary::{SummaryChunk, SummaryStore};
use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::warn;

pub async fn run_summary_phase(
    search_store: &SearchStore,
    repos: &[(String, PathBuf)],
    emb_cfg: &EmbeddingConfig,
    sum_cfg: &SummaryConfig,
) -> Result<usize> {
    if !sum_cfg.enabled {
        return Ok(0);
    }

    let provider = build_provider(sum_cfg)?;
    let (embedder, _http_stats) = make_embedder(Some(emb_cfg))?
        .ok_or_else(|| anyhow!("embedder factory returned None despite config"))?;
    let summary_store = SummaryStore::new(search_store);
    let sem = Arc::new(Semaphore::new(sum_cfg.max_concurrent as usize));

    let mut total = 0usize;
    for (repo_name, _repo_root) in repos {
        // Clear stale summaries for this repo before writing fresh ones.
        if let Err(e) = summary_store.delete_repo(repo_name) {
            warn!("delete summary rows for {repo_name} failed: {e}");
        }
        // Group chunks by file → one summary per file.
        let chunks = match search_store.list_chunks(repo_name) {
            Ok(c) => c,
            Err(e) => {
                warn!("list_chunks({repo_name}): {e}");
                continue;
            }
        };
        let mut by_path: HashMap<String, Vec<String>> = HashMap::new();
        for (_, path, _, _, content) in chunks {
            by_path.entry(path).or_default().push(content);
        }

        // 1. Fan out LLM calls — filtered by:
        //    - path blacklist (test / generated / types)
        //    - min chunks per file (skip tiny files)
        //    - min file bytes (skip near-empty files)
        let mut handles = Vec::new();
        for (path, contents) in by_path.iter() {
            if path_is_blacklisted(path, &sum_cfg.path_blacklist) {
                continue;
            }
            if (contents.len() as u32) < sum_cfg.min_chunks_per_file {
                continue;
            }
            let merged = contents.join("\n\n");
            if merged.len() < sum_cfg.min_file_bytes as usize {
                continue;
            }
            let truncated: String = merged
                .chars()
                .take(sum_cfg.max_chunk_chars as usize)
                .collect();
            let p_clone = path.clone();
            let provider = provider.clone();
            let permit = sem.clone().acquire_owned().await.unwrap();
            handles.push(tokio::spawn(async move {
                let _g = permit;
                provider
                    .summarize(&truncated, &p_clone)
                    .await
                    .map(|s| (p_clone, s))
            }));
        }
        let mut pairs: Vec<(String, String)> = Vec::with_capacity(handles.len());
        for h in handles {
            match h.await {
                Ok(Ok(pair)) => pairs.push(pair),
                Ok(Err(e)) => warn!("LLM summary failed: {e}"),
                Err(e) => warn!("summary task join: {e}"),
            }
        }

        // 2. Batch-embed all summaries.
        let texts: Vec<String> = pairs.iter().map(|(_, s)| s.clone()).collect();
        let vectors = embedder
            .encode_batch(&texts)
            .await
            .context("embedding summaries")?;
        if vectors.len() != pairs.len() {
            return Err(anyhow!(
                "embedder returned {} vectors for {} summaries",
                vectors.len(),
                pairs.len()
            ));
        }

        // 3. Persist.
        for ((path, text), v) in pairs.iter().zip(vectors.iter()) {
            let id = summary_store.insert(&SummaryChunk {
                repo: repo_name.clone(),
                path: path.clone(),
                scope: "module".into(),
                text: text.clone(),
            })?;
            summary_store.upsert_embedding(id, v)?;
            total += 1;
        }
    }
    Ok(total)
}

/// Per-file incremental variant. Re-summarise only the supplied `(repo, path)`
/// list — used by `repolayer update`. Costs ~1 LLM call + 1 embedding call per
/// changed file rather than per-repo full sweep.
pub async fn run_summary_phase_for_files(
    search_store: &SearchStore,
    changed: &[(String, String)], // (repo, repo-relative path)
    emb_cfg: &EmbeddingConfig,
    sum_cfg: &SummaryConfig,
) -> Result<usize> {
    if !sum_cfg.enabled || changed.is_empty() {
        return Ok(0);
    }

    let provider = build_provider(sum_cfg)?;
    let (embedder, _http_stats) =
        make_embedder(Some(emb_cfg))?.ok_or_else(|| anyhow!("embedder factory returned None"))?;
    let summary_store = SummaryStore::new(search_store);
    let sem = Arc::new(Semaphore::new(sum_cfg.max_concurrent as usize));

    // 1. Fan out LLM calls (one per changed file).
    let mut handles = Vec::new();
    for (repo, path) in changed {
        // Pull this file's chunks; if none exist (deleted file), only clear.
        let chunks_for_file: Vec<String> = search_store
            .list_chunks(repo)?
            .into_iter()
            .filter(|(_, p, _, _, _)| p == path)
            .map(|(_, _, _, _, c)| c)
            .collect();
        // Always clear stale summary first.
        if let Err(e) = summary_store.delete_for_path(repo, path) {
            warn!("delete_for_path({repo}, {path}): {e}");
        }
        if chunks_for_file.is_empty() {
            continue;
        }
        let merged = chunks_for_file.join("\n\n");
        if merged.len() < sum_cfg.min_file_bytes as usize {
            continue;
        }
        let truncated: String = merged
            .chars()
            .take(sum_cfg.max_chunk_chars as usize)
            .collect();
        let repo_c = repo.clone();
        let path_c = path.clone();
        let provider = provider.clone();
        let permit = sem.clone().acquire_owned().await.unwrap();
        handles.push(tokio::spawn(async move {
            let _g = permit;
            provider
                .summarize(&truncated, &path_c)
                .await
                .map(|s| (repo_c, path_c, s))
        }));
    }

    let mut tuples: Vec<(String, String, String)> = Vec::with_capacity(handles.len());
    for h in handles {
        match h.await {
            Ok(Ok(t)) => tuples.push(t),
            Ok(Err(e)) => warn!("LLM summary failed: {e}"),
            Err(e) => warn!("summary task join: {e}"),
        }
    }
    if tuples.is_empty() {
        return Ok(0);
    }

    // 2. Batch-embed.
    let texts: Vec<String> = tuples.iter().map(|(_, _, s)| s.clone()).collect();
    let vectors = embedder
        .encode_batch(&texts)
        .await
        .context("embedding summaries")?;

    // 3. Persist.
    let mut wrote = 0usize;
    for ((repo, path, text), v) in tuples.iter().zip(vectors.iter()) {
        let id = summary_store.insert(&SummaryChunk {
            repo: repo.clone(),
            path: path.clone(),
            scope: "module".into(),
            text: text.clone(),
        })?;
        summary_store.upsert_embedding(id, v)?;
        wrote += 1;
    }
    Ok(wrote)
}

fn build_provider(cfg: &SummaryConfig) -> Result<Arc<dyn LlmProvider>> {
    let api_key = std::env::var(&cfg.api_key_env)
        .with_context(|| format!("env var {} not set", cfg.api_key_env))?;
    match cfg.provider.as_str() {
        "deepseek" => Ok(Arc::new(DeepSeekProvider::new(&api_key, &cfg.base_url))),
        "anthropic" => Ok(Arc::new(AnthropicProvider::new(&api_key, &cfg.base_url))),
        other => Err(anyhow!("unknown summary provider: {other}")),
    }
}

/// Does `path` contain any blacklist substring? Case-sensitive — patterns are
/// usually lowercase file-extension style.
fn path_is_blacklisted(path: &str, blacklist: &[String]) -> bool {
    blacklist.iter().any(|p| path.contains(p))
}

#[cfg(test)]
mod filter_tests {
    use super::*;

    fn default_bl() -> Vec<String> {
        crate::config::default_path_blacklist()
    }

    #[test]
    fn skips_test_files() {
        let bl = default_bl();
        assert!(path_is_blacklisted("src/foo.test.ts", &bl));
        assert!(path_is_blacklisted("internal/bar_test.go", &bl));
        assert!(path_is_blacklisted("specs/baz.spec.ts", &bl));
    }

    #[test]
    fn skips_generated_files() {
        let bl = default_bl();
        assert!(path_is_blacklisted("api/foo_pb.go", &bl));
        assert!(path_is_blacklisted("src/__generated__/types.ts", &bl));
        assert!(path_is_blacklisted("src/types/api.d.ts", &bl));
    }

    #[test]
    fn keeps_real_business_code() {
        let bl = default_bl();
        assert!(!path_is_blacklisted("src/handler/voucher.go", &bl));
        assert!(!path_is_blacklisted(
            "packages/apps/.../coupon-panel/src/main.tsx",
            &bl
        ));
    }
}
