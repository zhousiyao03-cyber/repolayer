use repolayer::llm::embedding::{EmbeddingProvider, NotImplementedEmbedding};

#[tokio::test]
async fn not_implemented_embedding_returns_err() {
    let p = NotImplementedEmbedding::new("anthropic");
    let result = p.embed("some text").await;
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("anthropic") || msg.contains("not implemented"),
        "error should mention provider name or not implemented: {}",
        msg
    );
}
