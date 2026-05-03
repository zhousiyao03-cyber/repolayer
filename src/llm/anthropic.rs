use super::LlmProvider;
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};

pub struct AnthropicProvider {
    api_key: String,
    base_url: String,
    client: Client,
}

impl AnthropicProvider {
    pub fn new(api_key: &str, base_url: &str) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: base_url.into(),
            client: Client::new(),
        }
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn summarize(&self, code: &str, file_path: &str) -> Result<String> {
        let body = json!({
            "model": "claude-haiku-4-5-20251001",
            "max_tokens": 100,
            "messages": [{
                "role": "user",
                "content": format!("Summarize what this code does in 1-2 sentences. File: {}\n\n{}", file_path, code),
            }],
        });
        let res = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .context("anthropic request failed")?;
        if !res.status().is_success() {
            return Err(anyhow!("anthropic returned status {}", res.status()));
        }
        let json: Value = res.json().await.context("anthropic response parse")?;
        let text = json["content"][0]["text"]
            .as_str()
            .ok_or_else(|| anyhow!("anthropic response missing content[0].text"))?
            .to_string();
        Ok(text)
    }
}
