use super::LlmProvider;
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use std::time::Duration;
use tracing::warn;

const MAX_RETRIES: u32 = 3;
const BASE_BACKOFF_MS: u64 = 500;

pub struct DeepSeekProvider {
    api_key: String,
    base_url: String,
    client: Client,
}

impl DeepSeekProvider {
    pub fn new(api_key: &str, base_url: &str) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("reqwest client");
        Self {
            api_key: api_key.into(),
            base_url: base_url.into(),
            client,
        }
    }
}

#[async_trait]
impl LlmProvider for DeepSeekProvider {
    async fn summarize(&self, code: &str, file_path: &str) -> Result<String> {
        let prompt = format!(
            "用一句话（不超过 40 字）说明这段代码的业务作用，必须使用业务领域语言，不能只复述方法名。\n文件：{}\n\n```\n{}\n```",
            file_path, code
        );
        let body = json!({
            "model": "deepseek-chat",
            "max_tokens": 120,
            "messages": [{ "role": "user", "content": prompt }],
        });
        let url = format!("{}/v1/chat/completions", self.base_url);

        let mut last_err: Option<anyhow::Error> = None;
        for attempt in 0..=MAX_RETRIES {
            let res = self
                .client
                .post(&url)
                .bearer_auth(&self.api_key)
                .json(&body)
                .send()
                .await;
            match res {
                Err(e) => last_err = Some(anyhow!("network: {e}")),
                Ok(r) => {
                    let status = r.status();
                    if status.is_success() {
                        let json: Value = r.json().await.context("deepseek response parse")?;
                        let text = json["choices"][0]["message"]["content"]
                            .as_str()
                            .ok_or_else(|| anyhow!("deepseek response missing content"))?
                            .trim()
                            .to_string();
                        return Ok(text);
                    }
                    if !is_retryable(status) {
                        let body = r.text().await.unwrap_or_default();
                        return Err(anyhow!("deepseek non-retryable {}: {}", status, body));
                    }
                    last_err = Some(anyhow!("deepseek {}", status));
                }
            }
            if attempt == MAX_RETRIES {
                break;
            }
            let sleep = BASE_BACKOFF_MS * (1u64 << attempt);
            warn!(
                attempt = attempt + 1,
                sleep_ms = sleep,
                "deepseek retry: {:?}",
                last_err
            );
            tokio::time::sleep(Duration::from_millis(sleep)).await;
        }
        Err(last_err.unwrap_or_else(|| anyhow!("deepseek: retries exhausted")))
    }
}

fn is_retryable(s: StatusCode) -> bool {
    s.is_server_error() || s == StatusCode::TOO_MANY_REQUESTS || s == StatusCode::REQUEST_TIMEOUT
}
