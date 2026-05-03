use anyhow::Result;
use async_trait::async_trait;
use repolayer::graph::model::*;
use repolayer::graph::store::Store;
use repolayer::llm::summary::summarize_modules;
use repolayer::llm::LlmProvider;
use std::sync::Arc;
use tempfile::tempdir;

struct AlwaysOk(&'static str);
#[async_trait]
impl LlmProvider for AlwaysOk {
    async fn summarize(&self, _code: &str, _path: &str) -> Result<String> {
        Ok(self.0.to_string())
    }
}

struct AlwaysErr;
#[async_trait]
impl LlmProvider for AlwaysErr {
    async fn summarize(&self, _code: &str, _path: &str) -> Result<String> {
        anyhow::bail!("simulated LLM error")
    }
}

#[tokio::test]
async fn summaries_are_written_to_module_nodes() {
    let dir = tempdir().unwrap();
    let store = Store::open(&dir.path().join("index.db")).unwrap();

    // Create a real file so summarize_modules can read it
    let repo_root = dir.path().join("repo");
    std::fs::create_dir_all(repo_root.join("src")).unwrap();
    std::fs::write(repo_root.join("src/auth.ts"), "export function login() {}").unwrap();

    let n = Node::new(NodeKind::Module, "repo", "src/auth.ts", None);
    store.upsert_node(&n).unwrap();

    let provider: Arc<dyn LlmProvider> = Arc::new(AlwaysOk("This handles authentication."));
    summarize_modules(&store, provider, &[("repo".to_string(), repo_root.clone())])
        .await
        .unwrap();

    let after = store.get_node(&n.id).unwrap().unwrap();
    assert_eq!(
        after.summary.as_deref(),
        Some("This handles authentication.")
    );
}

#[tokio::test]
async fn failed_summaries_are_skipped_not_fatal() {
    let dir = tempdir().unwrap();
    let store = Store::open(&dir.path().join("index.db")).unwrap();
    let repo_root = dir.path().join("repo");
    std::fs::create_dir_all(repo_root.join("src")).unwrap();
    std::fs::write(repo_root.join("src/a.ts"), "export const x = 1").unwrap();
    std::fs::write(repo_root.join("src/b.ts"), "export const y = 2").unwrap();

    let na = Node::new(NodeKind::Module, "repo", "src/a.ts", None);
    let nb = Node::new(NodeKind::Module, "repo", "src/b.ts", None);
    store.upsert_node(&na).unwrap();
    store.upsert_node(&nb).unwrap();

    let provider: Arc<dyn LlmProvider> = Arc::new(AlwaysErr);
    // Should NOT bail — failed modules just don't get summaries
    let result = summarize_modules(&store, provider, &[("repo".to_string(), repo_root)]).await;
    assert!(
        result.is_ok(),
        "summarize_modules should not fail when LLM errors"
    );

    let after_a = store.get_node(&na.id).unwrap().unwrap();
    assert!(after_a.summary.is_none(), "no summary on failed module");
}

#[tokio::test]
async fn skips_modules_already_summarized() {
    let dir = tempdir().unwrap();
    let store = Store::open(&dir.path().join("index.db")).unwrap();
    let repo_root = dir.path().join("repo");
    std::fs::create_dir_all(repo_root.join("src")).unwrap();
    std::fs::write(repo_root.join("src/a.ts"), "export const x = 1").unwrap();

    let mut n = Node::new(NodeKind::Module, "repo", "src/a.ts", None);
    n.summary = Some("Existing summary".into());
    store.upsert_node(&n).unwrap();

    let provider: Arc<dyn LlmProvider> = Arc::new(AlwaysOk("New summary"));
    summarize_modules(&store, provider, &[("repo".to_string(), repo_root)])
        .await
        .unwrap();

    let after = store.get_node(&n.id).unwrap().unwrap();
    assert_eq!(
        after.summary.as_deref(),
        Some("Existing summary"),
        "should not overwrite"
    );
}

#[tokio::test]
async fn skips_module_if_file_missing() {
    let dir = tempdir().unwrap();
    let store = Store::open(&dir.path().join("index.db")).unwrap();
    let repo_root = dir.path().join("repo");
    std::fs::create_dir_all(&repo_root).unwrap();
    // Note: don't create src/missing.ts

    let n = Node::new(NodeKind::Module, "repo", "src/missing.ts", None);
    store.upsert_node(&n).unwrap();

    let provider: Arc<dyn LlmProvider> = Arc::new(AlwaysOk("anything"));
    let result = summarize_modules(&store, provider, &[("repo".to_string(), repo_root)]).await;
    assert!(result.is_ok());
    // Module with missing file gets no summary
    let after = store.get_node(&n.id).unwrap().unwrap();
    assert!(after.summary.is_none());
}
