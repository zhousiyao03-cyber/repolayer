use super::LlmProvider;
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};

pub struct DeepSeekProvider {
    api_key: String,
    base_url: String,
    client: Client,
}

impl DeepSeekProvider {
    pub fn new(api_key: &str, base_url: &str) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: base_url.into(),
            client: Client::new(),
        }
    }
}

#[async_trait]
impl LlmProvider for DeepSeekProvider {
    async fn summarize(&self, code: &str, file_path: &str) -> Result<String> {
        let body = json!({
            "model": "deepseek-chat",
            "max_tokens": 100,
            "messages": [{
                "role": "user",
                "content": format!("Summarize what this code does in 1-2 sentences. File: {}\n\n{}", file_path, code),
            }],
        });
        let res = self
            .client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("deepseek request failed")?;
        if !res.status().is_success() {
            return Err(anyhow!("deepseek returned status {}", res.status()));
        }
        let json: Value = res.json().await.context("deepseek response parse")?;
        let text = json["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| anyhow!("deepseek response missing choices[0].message.content"))?
            .to_string();
        Ok(text)
    }
}
