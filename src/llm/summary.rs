use crate::graph::model::*;
use crate::graph::store::Store;
use crate::llm::LlmProvider;
use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::warn;

const MAX_SNIPPET_LEN: usize = 4000;
const MAX_RETRIES: u32 = 3;

/// Generate LLM summaries for all unsummarized Module nodes in any of the given repos.
/// Failures are logged + skipped (never fatal).
pub async fn summarize_modules(
    store: &Store,
    provider: Arc<dyn LlmProvider>,
    repos: &[(String, PathBuf)], // (repo_name, repo_root)
) -> Result<()> {
    let modules = store.list_nodes_by_kind(NodeKind::Module)?;
    for m in modules {
        if m.summary.is_some() {
            continue;
        }
        // Find the abs path
        let Some((_, repo_root)) = repos.iter().find(|(name, _)| *name == m.repo) else {
            continue;
        };
        let abs = repo_root.join(&m.path);
        let snippet = match std::fs::read_to_string(&abs) {
            Ok(s) => {
                if s.len() > MAX_SNIPPET_LEN {
                    s[..MAX_SNIPPET_LEN].to_string()
                } else {
                    s
                }
            }
            Err(_) => continue,
        };
        match call_with_retry(&*provider, &snippet, &m.path, MAX_RETRIES).await {
            Ok(text) => {
                let mut updated = m.clone();
                updated.summary = Some(text);
                if let Err(e) = store.upsert_node(&updated) {
                    warn!("upsert summary for {} failed: {}", m.path, e);
                }
            }
            Err(e) => warn!("summary failed for {}: {}", m.path, e),
        }
    }
    Ok(())
}

async fn call_with_retry(p: &dyn LlmProvider, code: &str, path: &str, max: u32) -> Result<String> {
    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 0..max {
        match p.summarize(code, path).await {
            Ok(s) => return Ok(s),
            Err(e) => {
                last_err = Some(e);
                tokio::time::sleep(std::time::Duration::from_millis(500 * (attempt as u64 + 1)))
                    .await;
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("no error captured")))
}
