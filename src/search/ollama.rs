//! Ollama HTTP embedder (sidecar pattern).
//!
//! Calls a local `ollama serve` daemon (default `http://localhost:11434`) for
//! embeddings. Used when you want a code-aware multilingual model (e.g.
//! `qwen3-embedding:0.6b`) without taking on the candle / GGUF integration
//! cost. Ollama handles the model loading, quantisation, batching and tokenizer
//! work; we just speak its `/api/embed` JSON protocol.
//!
//! Differences vs `HttpEmbedder` (kept intentionally minimal):
//! - No auth (bearer) — localhost only.
//! - No QPM cap — local resource, bounded by `max_concurrent`.
//! - Different response shape: `{"embeddings": [[...], [...]]}` vs OpenAI's
//!   `{"data": [{"embedding": [...]}]}`.
//! - Endpoint in the config is the daemon base URL; we append `/api/embed`.

use crate::config::EmbeddingConfig;
use crate::search::embedder::Embedder;
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tracing::{debug, warn};

const MAX_RETRIES: u32 = 3;
const BASE_BACKOFF_MS: u64 = 200;

/// Build-time / runtime counters surfaced via observability. Mirrors
/// `HttpEmbedderStats` so the indexer can treat both providers uniformly.
#[derive(Default, Debug)]
pub struct OllamaStats {
    pub requests: AtomicU64,
    pub retries: AtomicU64,
    pub input_chars: AtomicU64,
    pub vectors_returned: AtomicU64,
}

pub struct OllamaEmbedder {
    client: Client,
    /// Full embedding URL, e.g. `http://localhost:11434/api/embed`.
    url: String,
    model: String,
    dim: usize,
    batch_size: usize,
    sem: Arc<Semaphore>,
    pub stats: Arc<OllamaStats>,
}

impl OllamaEmbedder {
    pub fn from_config(cfg: &EmbeddingConfig) -> Result<Self> {
        if cfg.provider != "ollama" {
            bail!(
                "OllamaEmbedder requires provider=ollama, got {}",
                cfg.provider
            );
        }
        // Allow either a base URL ("http://localhost:11434") or the full path.
        // Sidecar deployments tend to use the base URL; tests may pin a mock
        // server's exact `/api/embed` path.
        let base = cfg.endpoint.trim_end_matches('/').to_string();
        let url = if base.ends_with("/api/embed") {
            base
        } else {
            format!("{base}/api/embed")
        };
        let client = Client::builder()
            .timeout(Duration::from_millis(cfg.request_timeout_ms as u64))
            .build()
            .context("building reqwest client")?;
        Ok(Self {
            client,
            url,
            model: cfg.model.clone(),
            dim: cfg.dim as usize,
            batch_size: cfg.batch_size as usize,
            sem: Arc::new(Semaphore::new(cfg.max_concurrent.max(1) as usize)),
            stats: Arc::new(OllamaStats::default()),
        })
    }

    /// Snapshot handle to the live counters. Cheap clone (Arc).
    pub fn stats(&self) -> Arc<OllamaStats> {
        self.stats.clone()
    }

    async fn one_batch(&self, batch: &[String]) -> Result<Vec<Vec<f32>>> {
        let _permit = self.sem.acquire().await.map_err(|e| anyhow!("sem: {e}"))?;
        let batch_chars: u64 = batch.iter().map(|s| s.chars().count() as u64).sum();
        self.stats
            .input_chars
            .fetch_add(batch_chars, Ordering::Relaxed);
        // Ollama accepts both `input: "single"` and `input: ["a", "b"]`. Always
        // send the array form to keep response parsing one-shape.
        let body = json!({ "model": self.model, "input": batch });
        let mut last_err: Option<anyhow::Error> = None;
        for attempt in 0..=MAX_RETRIES {
            let res = self.client.post(&self.url).json(&body).send().await;
            match res {
                Err(e) => {
                    last_err = Some(anyhow!("network: {e}"));
                }
                Ok(r) => {
                    let status = r.status();
                    if status.is_success() {
                        self.stats.requests.fetch_add(1, Ordering::Relaxed);
                        let parsed: EmbeddingResponse = r
                            .json()
                            .await
                            .context("parsing ollama embedding response")?;
                        let vectors = parsed.into_vectors(self.dim)?;
                        if vectors.len() != batch.len() {
                            bail!(
                                "ollama returned {} vectors for {} inputs",
                                vectors.len(),
                                batch.len()
                            );
                        }
                        self.stats
                            .vectors_returned
                            .fetch_add(vectors.len() as u64, Ordering::Relaxed);
                        return Ok(vectors);
                    }
                    if !is_retryable(status) {
                        let text = r.text().await.unwrap_or_default();
                        bail!("ollama non-retryable {}: {}", status, text);
                    }
                    let text = r.text().await.unwrap_or_default();
                    last_err = Some(anyhow!("ollama {}: {}", status, text));
                }
            }
            if attempt == MAX_RETRIES {
                break;
            }
            self.stats.retries.fetch_add(1, Ordering::Relaxed);
            let backoff = BASE_BACKOFF_MS * (1u64 << attempt);
            let jitter = (rand_jitter() * backoff as f64) as u64;
            let total = backoff.saturating_add(jitter);
            warn!(
                attempt = attempt + 1,
                backoff_ms = total,
                "ollama retrying after error: {:?}",
                last_err
            );
            tokio::time::sleep(Duration::from_millis(total)).await;
        }
        Err(last_err.unwrap_or_else(|| anyhow!("ollama: all retries exhausted")))
    }
}

/// Hard cap on per-input character length. Same rationale as the HTTP
/// embedder's truncation: ollama models silently truncate (or OOM on huge inputs);
/// we'd rather pre-truncate to a known-safe length so the rest of the
/// chunk's leading declarations make it into the embedding.
///
/// 24k chars covers ~95th percentile chunks in ttec workload. Qwen3
/// embedding has a 32k token context so we have headroom even with
/// dense CJK input.
const MAX_INPUT_CHARS: usize = 24_000;

fn truncate_input(text: &str) -> String {
    if text.chars().count() <= MAX_INPUT_CHARS {
        return text.to_string();
    }
    text.chars().take(MAX_INPUT_CHARS).collect()
}

#[async_trait]
impl Embedder for OllamaEmbedder {
    fn dim(&self) -> usize {
        self.dim
    }
    async fn encode_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let prepared: Vec<String> = texts.iter().map(|s| truncate_input(s)).collect();
        let mut out = Vec::with_capacity(prepared.len());
        for chunk in prepared.chunks(self.batch_size) {
            match self.one_batch(chunk).await {
                Ok(v) => out.extend(v),
                Err(e) => {
                    // Fall back to one-at-a-time so a single rotten input
                    // doesn't kill the whole batch. Zero-vector on per-input
                    // failure keeps the index consistent (BM25 still works).
                    warn!(
                        "ollama batch failed ({}), falling back to per-input encoding",
                        e
                    );
                    for input in chunk {
                        match self.one_batch(std::slice::from_ref(input)).await {
                            Ok(mut v) if v.len() == 1 => out.push(v.pop().unwrap()),
                            Ok(_) => {
                                warn!("ollama returned wrong count for single input; zero vector");
                                out.push(vec![0.0; self.dim]);
                            }
                            Err(single_err) => {
                                warn!("ollama dropped input ({}); zero vector", single_err);
                                out.push(vec![0.0; self.dim]);
                            }
                        }
                    }
                }
            }
        }
        debug!(
            "ollama encode_batch: {} inputs in {} chunks",
            prepared.len(),
            prepared.len().div_ceil(self.batch_size)
        );
        Ok(out)
    }
}

fn is_retryable(s: StatusCode) -> bool {
    // Ollama returns 503 when the model is still loading on first call,
    // and 500 occasionally on transient runner issues. Treat both as retryable.
    s.is_server_error() || s == StatusCode::REQUEST_TIMEOUT
}

fn rand_jitter() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    (nanos % 1_000) as f64 / 1_000.0
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    embeddings: Vec<Vec<f32>>,
}

impl EmbeddingResponse {
    fn into_vectors(self, dim: usize) -> Result<Vec<Vec<f32>>> {
        let mut out = Vec::with_capacity(self.embeddings.len());
        for v in self.embeddings {
            if v.len() != dim {
                bail!("ollama returned dim {} but config says {}", v.len(), dim);
            }
            // Ollama returns already-normalised vectors for most embedding
            // models (verified empirically: qwen3-embedding norm = 1.0000).
            // We normalise again defensively in case a model variant doesn't.
            out.push(l2_normalise(v));
        }
        Ok(out)
    }
}

fn l2_normalise(mut v: Vec<f32>) -> Vec<f32> {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-9 {
        let inv = 1.0 / norm;
        for x in &mut v {
            *x *= inv;
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;

    fn cfg(endpoint: String, model: &str, dim: u32) -> EmbeddingConfig {
        EmbeddingConfig {
            provider: "ollama".into(),
            model: model.into(),
            endpoint,
            api_key_env: String::new(),
            dim,
            batch_size: 8,
            max_concurrent: 2,
            request_timeout_ms: 5000,
            qpm_cap: 0,
        }
    }

    fn unit_vec(dim: usize, seed: f32) -> Vec<f32> {
        // simple non-zero vector; client will renormalise
        (0..dim).map(|i| seed + i as f32 * 0.01).collect()
    }

    #[tokio::test]
    async fn rejects_wrong_provider() {
        let mut cfg = cfg("http://x".into(), "m", 4);
        cfg.provider = "http".into();
        assert!(OllamaEmbedder::from_config(&cfg).is_err());
    }

    #[tokio::test]
    async fn accepts_base_or_full_endpoint() {
        let base = OllamaEmbedder::from_config(&cfg("http://x:11434".into(), "m", 4)).unwrap();
        assert!(base.url.ends_with("/api/embed"));
        let full =
            OllamaEmbedder::from_config(&cfg("http://x:11434/api/embed".into(), "m", 4)).unwrap();
        assert_eq!(full.url, "http://x:11434/api/embed");
    }

    #[tokio::test]
    async fn encodes_batch_via_mock() {
        let mut server = Server::new_async().await;
        let v1 = unit_vec(4, 1.0);
        let v2 = unit_vec(4, 2.0);
        let body = json!({"embeddings": [v1, v2]}).to_string();
        let m = server
            .mock("POST", "/api/embed")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await;

        let emb =
            OllamaEmbedder::from_config(&cfg(server.url(), "qwen3-embedding:0.6b", 4)).unwrap();
        let out = emb
            .encode_batch(&["hello".into(), "world".into()])
            .await
            .unwrap();
        m.assert_async().await;
        assert_eq!(out.len(), 2);
        // Output should be L2-normalised
        for v in &out {
            assert_eq!(v.len(), 4);
            let n: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            assert!((n - 1.0).abs() < 1e-4, "norm = {n}");
        }
        assert_eq!(emb.stats.requests.load(Ordering::Relaxed), 1);
        assert_eq!(emb.stats.vectors_returned.load(Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn dim_mismatch_falls_back_to_zero_vector() {
        // When the server returns a wrong-dim vector, one_batch bails. The
        // batch path then falls back to per-input (which also fails the same
        // way) and emits a zero-dim placeholder so the index stays consistent.
        // This is the production-correct behaviour: a misconfigured model
        // shouldn't kill the whole build.
        let mut server = Server::new_async().await;
        let body = json!({"embeddings": [unit_vec(8, 1.0)]}).to_string();
        server
            .mock("POST", "/api/embed")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            // batch attempt + 1 fallback per-input attempt = at least 2 hits
            .expect_at_least(2)
            .create_async()
            .await;
        let emb = OllamaEmbedder::from_config(&cfg(server.url(), "m", 4)).unwrap();
        let out = emb.encode_batch(&["hi".into()]).await.unwrap();
        assert_eq!(out.len(), 1);
        // zero vector (4 zeros) emitted as placeholder
        assert_eq!(out[0], vec![0.0; 4]);
    }

    #[tokio::test]
    async fn retries_on_5xx_then_succeeds() {
        let mut server = Server::new_async().await;
        let _fail = server
            .mock("POST", "/api/embed")
            .with_status(503)
            .with_body("model loading")
            .expect(2)
            .create_async()
            .await;
        let _ok = server
            .mock("POST", "/api/embed")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(json!({"embeddings": [unit_vec(4, 1.0)]}).to_string())
            .create_async()
            .await;
        let emb = OllamaEmbedder::from_config(&cfg(server.url(), "m", 4)).unwrap();
        let out = emb.encode_batch(&["hi".into()]).await.unwrap();
        assert_eq!(out.len(), 1);
        assert!(emb.stats.retries.load(Ordering::Relaxed) >= 1);
    }

    #[tokio::test]
    async fn batches_large_input() {
        let mut server = Server::new_async().await;
        // 20 inputs, batch_size 8 → 3 batches of size 8, 8, 4.
        for sz in [8usize, 8, 4] {
            let vecs: Vec<Vec<f32>> = (0..sz).map(|i| unit_vec(4, i as f32 + 0.5)).collect();
            server
                .mock("POST", "/api/embed")
                .with_status(200)
                .with_header("content-type", "application/json")
                .with_body(json!({ "embeddings": vecs }).to_string())
                .expect(1)
                .create_async()
                .await;
        }
        let emb = OllamaEmbedder::from_config(&cfg(server.url(), "m", 4)).unwrap();
        let inputs: Vec<String> = (0..20).map(|i| format!("input {i}")).collect();
        let out = emb.encode_batch(&inputs).await.unwrap();
        assert_eq!(out.len(), 20);
    }
}
