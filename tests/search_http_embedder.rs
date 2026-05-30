//! HTTP-mocked tests for HttpEmbedder. A real endpoint is not reachable from CI.

use mockito::Server;
use repolayer::config::EmbeddingConfig;
use repolayer::search::embedder::Embedder;
use repolayer::search::http_embedder::HttpEmbedder;
use serde_json::json;

fn mock_cfg(server: &Server) -> EmbeddingConfig {
    EmbeddingConfig {
        provider: "http".into(),
        model: "your-embedding-model".into(),
        endpoint: format!("{}/v1/embeddings", server.url()),
        api_key_env: "EMBEDDING_API_KEY_TEST".into(),
        dim: 1024,
        batch_size: 32,
        max_concurrent: 4,
        request_timeout_ms: 5_000,
        qpm_cap: 10_000,
    }
}

#[tokio::test]
async fn single_batch_request_returns_vectors() {
    let mut server = Server::new_async().await;
    let vec1024: Vec<f32> = (0..1024).map(|_| 0.01).collect();
    let body = json!({
        "data": [
            { "embedding": vec1024.clone(), "index": 0 },
            { "embedding": vec1024.clone(), "index": 1 }
        ]
    });
    let m = server
        .mock("POST", mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(body.to_string())
        .create_async()
        .await;

    std::env::set_var("EMBEDDING_API_KEY_TEST", "fake-ak");
    let cfg = mock_cfg(&server);
    let e = HttpEmbedder::from_config(&cfg).unwrap();
    let out = e
        .encode_batch(&["hello".into(), "world".into()])
        .await
        .unwrap();
    m.assert_async().await;
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].len(), 1024);
    // L2-normalised
    let norm: f32 = out[0].iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!((norm - 1.0).abs() < 1e-3, "norm = {norm}");
}

#[tokio::test]
async fn retries_on_5xx() {
    let mut server = Server::new_async().await;
    let _fail = server
        .mock("POST", mockito::Matcher::Any)
        .with_status(503)
        .with_body("Service Unavailable")
        .expect(2)
        .create_async()
        .await;
    let vec1024: Vec<f32> = (0..1024).map(|_| 0.01).collect();
    let body = json!({ "data": [ { "embedding": vec1024, "index": 0 } ] });
    let _ok = server
        .mock("POST", mockito::Matcher::Any)
        .with_status(200)
        .with_body(body.to_string())
        .create_async()
        .await;

    std::env::set_var("EMBEDDING_API_KEY_TEST", "fake-ak");
    let cfg = mock_cfg(&server);
    let e = HttpEmbedder::from_config(&cfg).unwrap();
    let out = e.encode_batch(&["hi".into()]).await.unwrap();
    assert_eq!(out.len(), 1);
}

#[tokio::test]
async fn auto_batches_large_input_when_over_batch_size() {
    let mut server = Server::new_async().await;
    let vec1024: Vec<f32> = (0..1024).map(|_| 0.01).collect();

    // 70 inputs with batch_size=32 → 3 requests (32 + 32 + 6).
    // The implementation requires the response vector count to match the
    // request input count; so we register two mocks: one for the 32-sized
    // batches (expected twice) and one for the 6-sized final batch.
    let data_32: Vec<_> = (0..32)
        .map(|i| json!({"embedding": vec1024, "index": i}))
        .collect();
    let body_32 = json!({ "data": data_32 });
    let m_full = server
        .mock("POST", mockito::Matcher::Any)
        .match_body(mockito::Matcher::PartialJsonString(
            json!({ "input": (0..32).map(|i| format!("text {i}")).collect::<Vec<_>>() })
                .to_string(),
        ))
        .with_status(200)
        .with_body(body_32.to_string())
        .expect(1)
        .create_async()
        .await;
    let data_32b: Vec<_> = (0..32)
        .map(|i| json!({"embedding": vec1024, "index": i}))
        .collect();
    let body_32b = json!({ "data": data_32b });
    let m_full2 = server
        .mock("POST", mockito::Matcher::Any)
        .match_body(mockito::Matcher::PartialJsonString(
            json!({ "input": (32..64).map(|i| format!("text {i}")).collect::<Vec<_>>() })
                .to_string(),
        ))
        .with_status(200)
        .with_body(body_32b.to_string())
        .expect(1)
        .create_async()
        .await;
    let data_6: Vec<_> = (0..6)
        .map(|i| json!({"embedding": vec1024, "index": i}))
        .collect();
    let body_6 = json!({ "data": data_6 });
    let m_tail = server
        .mock("POST", mockito::Matcher::Any)
        .match_body(mockito::Matcher::PartialJsonString(
            json!({ "input": (64..70).map(|i| format!("text {i}")).collect::<Vec<_>>() })
                .to_string(),
        ))
        .with_status(200)
        .with_body(body_6.to_string())
        .expect(1)
        .create_async()
        .await;

    std::env::set_var("EMBEDDING_API_KEY_TEST", "fake-ak");
    let cfg = mock_cfg(&server);
    let e = HttpEmbedder::from_config(&cfg).unwrap();
    let inputs: Vec<String> = (0..70).map(|i| format!("text {i}")).collect();
    let out = e.encode_batch(&inputs).await.unwrap();
    assert_eq!(out.len(), 70);
    m_full.assert_async().await;
    m_full2.assert_async().await;
    m_tail.assert_async().await;
}
