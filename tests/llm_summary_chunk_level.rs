//! Summary orchestrator: produces N summary_chunks per repo, embeds via the
//! configured embedder, writes both tables.

use mockito::Server;
use repolayer::config::{EmbeddingConfig, SummaryConfig};
use repolayer::llm::summary_store::run_summary_phase;
use repolayer::search::store::SearchStore;
use repolayer::search::store_summary::SummaryStore;
use serde_json::json;
use tempfile::tempdir;

#[tokio::test]
async fn summary_phase_writes_per_module_summaries() {
    // Mock DeepSeek + HTTP embedder.
    let mut llm = Server::new_async().await;
    let llm_body = json!({
        "choices": [{ "message": { "content": "Handles the user login request and validates the session token." } }]
    });
    let _llm_mock = llm
        .mock("POST", mockito::Matcher::Any)
        .with_status(200)
        .with_body(llm_body.to_string())
        .expect_at_least(1)
        .create_async()
        .await;
    let mut emb = Server::new_async().await;
    let vec_1024: Vec<f32> = (0..1024).map(|_| 0.01).collect();
    let emb_body = json!({ "data": [ { "embedding": vec_1024, "index": 0 } ] });
    let _emb_mock = emb
        .mock("POST", mockito::Matcher::Any)
        .with_status(200)
        .with_body(emb_body.to_string())
        .expect_at_least(1)
        .create_async()
        .await;

    std::env::set_var("DEEPSEEK_KEY_TEST", "k");
    std::env::set_var("EMBEDDING_API_KEY_TEST", "k");

    let tmp = tempdir().unwrap();
    let p = tmp.path().join("search.db");
    let store = SearchStore::open_with_dim(&p, 1024).unwrap();
    // Seed one chunk so the summary phase has a module to summarise.
    // (In real builds the chunks are inserted earlier in Phase C.)
    use repolayer::search::chunker::Chunk;
    store
        .replace_repo_chunks(
            "order_service",
            &[Chunk {
                content: "func ConsumeVoucher(req *ConsumeReq) {...}".into(),
                file_path: "src/voucher.go".into(),
                start_line: 1,
                end_line: 10,
                start_byte: 0,
                end_byte: 50,
                language: "go".into(),
            }],
        )
        .unwrap();

    let emb_cfg = EmbeddingConfig {
        provider: "http".into(),
        model: "your-embedding-model".into(),
        endpoint: format!("{}/v1/embeddings", emb.url()),
        api_key_env: "EMBEDDING_API_KEY_TEST".into(),
        dim: 1024,
        batch_size: 32,
        max_concurrent: 2,
        request_timeout_ms: 5000,
        qpm_cap: 60,
    };
    let sum_cfg = SummaryConfig {
        enabled: true,
        provider: "deepseek".into(),
        api_key_env: "DEEPSEEK_KEY_TEST".into(),
        base_url: llm.url(),
        max_chunk_chars: 4000,
        max_concurrent: 2,
        min_file_bytes: 0,
        min_chunks_per_file: 1,
        path_blacklist: vec![],
    };

    let count = run_summary_phase(
        &store,
        &[("order_service".into(), tmp.path().to_path_buf())],
        &emb_cfg,
        &sum_cfg,
    )
    .await
    .unwrap();
    assert!(count >= 1);

    let summary_store = SummaryStore::new(&store);
    assert!(summary_store.count().unwrap() >= 1);
}

#[tokio::test]
async fn summary_phase_for_files_only_processes_changed_files() {
    let mut llm = Server::new_async().await;
    let llm_body = json!({ "choices": [{ "message": { "content": "中文摘要测试。" } }] });
    let llm_mock = llm
        .mock("POST", mockito::Matcher::Any)
        .with_status(200)
        .with_body(llm_body.to_string())
        .expect(1) // exactly ONE call — for the one changed file
        .create_async()
        .await;
    let mut emb = Server::new_async().await;
    let vec_1024: Vec<f32> = (0..1024).map(|_| 0.01).collect();
    let emb_body = json!({ "data": [ { "embedding": vec_1024, "index": 0 } ] });
    let _emb_mock = emb
        .mock("POST", mockito::Matcher::Any)
        .with_status(200)
        .with_body(emb_body.to_string())
        .expect(1)
        .create_async()
        .await;

    std::env::set_var("DEEPSEEK_KEY_T2", "k");
    std::env::set_var("EMBEDDING_API_KEY_T2", "k");
    let tmp = tempdir().unwrap();
    let p = tmp.path().join("search.db");
    let store = SearchStore::open_with_dim(&p, 1024).unwrap();
    use repolayer::search::chunker::Chunk;
    store
        .replace_repo_chunks(
            "r",
            &[
                Chunk {
                    content: "old code A".into(),
                    file_path: "a.go".into(),
                    start_line: 1,
                    end_line: 5,
                    start_byte: 0,
                    end_byte: 20,
                    language: "go".into(),
                },
                Chunk {
                    content: "stable code B".into(),
                    file_path: "b.go".into(),
                    start_line: 1,
                    end_line: 5,
                    start_byte: 0,
                    end_byte: 20,
                    language: "go".into(),
                },
            ],
        )
        .unwrap();

    let emb_cfg = EmbeddingConfig {
        provider: "http".into(),
        model: "your-embedding-model".into(),
        endpoint: format!("{}/v1/embeddings", emb.url()),
        api_key_env: "EMBEDDING_API_KEY_T2".into(),
        dim: 1024,
        batch_size: 32,
        max_concurrent: 2,
        request_timeout_ms: 5000,
        qpm_cap: 60,
    };
    let sum_cfg = SummaryConfig {
        enabled: true,
        provider: "deepseek".into(),
        api_key_env: "DEEPSEEK_KEY_T2".into(),
        base_url: llm.url(),
        max_chunk_chars: 4000,
        max_concurrent: 2,
        min_file_bytes: 0,
        min_chunks_per_file: 1,
        path_blacklist: vec![],
    };

    // Only "a.go" changed; "b.go" should NOT be re-summarised.
    let changed = vec![("r".to_string(), "a.go".to_string())];
    repolayer::llm::summary_store::run_summary_phase_for_files(
        &store, &changed, &emb_cfg, &sum_cfg,
    )
    .await
    .unwrap();
    llm_mock.assert_async().await;
}
