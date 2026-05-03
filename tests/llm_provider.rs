use repolayer::llm::{anthropic::AnthropicProvider, deepseek::DeepSeekProvider, LlmProvider};

#[tokio::test]
async fn anthropic_returns_summary() {
    let mut server = mockito::Server::new_async().await;
    let _m = server
        .mock("POST", "/v1/messages")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"content":[{"type":"text","text":"This module handles authentication."}]}"#)
        .create_async()
        .await;

    let provider = AnthropicProvider::new("test-key", &server.url());
    let result = provider
        .summarize("export function login() {}", "auth.ts")
        .await
        .unwrap();
    assert!(
        result.contains("authentication"),
        "summary should contain expected text, got: {}",
        result
    );
}

#[tokio::test]
async fn anthropic_propagates_http_error() {
    let mut server = mockito::Server::new_async().await;
    let _m = server
        .mock("POST", "/v1/messages")
        .with_status(500)
        .with_body("internal error")
        .create_async()
        .await;
    let provider = AnthropicProvider::new("test-key", &server.url());
    let result = provider.summarize("code", "x.ts").await;
    assert!(result.is_err(), "500 should produce an error");
}

#[tokio::test]
async fn deepseek_returns_summary() {
    let mut server = mockito::Server::new_async().await;
    let _m = server
        .mock("POST", "/v1/chat/completions")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"choices":[{"message":{"content":"This is from DeepSeek."}}]}"#)
        .create_async()
        .await;
    let provider = DeepSeekProvider::new("test-key", &server.url());
    let result = provider
        .summarize("export function x() {}", "x.ts")
        .await
        .unwrap();
    assert!(
        result.contains("DeepSeek"),
        "summary should contain expected text, got: {}",
        result
    );
}
