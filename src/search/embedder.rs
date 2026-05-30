//! Embedder trait + provider factory.
//!
//! Two impls land in later tasks:
//! - `LocalPotion` — the legacy 256-dim model2vec embedder
//! - `HttpEmbedder` — HTTP client for an OpenAI-compatible `/v1/embeddings` API
//!
//! Callers see a single dyn-trait object; construction picks the right impl
//! from `EmbeddingConfig`.

use anyhow::Result;
use async_trait::async_trait;

/// One f32 vector per input string, all L2-normalised, all length `dim()`.
#[async_trait]
pub trait Embedder: Send + Sync {
    fn dim(&self) -> usize;
    /// Encode a batch. Returned vec has the same length as `texts`; the i-th
    /// inner vec has length `self.dim()`.
    async fn encode_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
    /// Convenience: encode a single string. Default impl delegates to
    /// `encode_batch` so impls only have to write one method.
    async fn encode_one(&self, text: &str) -> Result<Vec<f32>> {
        let mut out = self.encode_batch(&[text.to_string()]).await?;
        out.pop()
            .ok_or_else(|| anyhow::anyhow!("empty result from encode_batch"))
    }
}

use crate::search::embed::{Embedder as PotionEmbedder, DIM as POTION_DIM};

/// Adapter around the existing safetensors + tokenizer-based potion-code-16M
/// model. Preserves backward compatibility when no embedding config is provided.
pub struct LocalPotion {
    inner: std::sync::Arc<PotionEmbedder>,
}

impl LocalPotion {
    pub fn new(inner: std::sync::Arc<PotionEmbedder>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl Embedder for LocalPotion {
    fn dim(&self) -> usize {
        POTION_DIM
    }
    async fn encode_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        // Local model is pure CPU; spawn-blocking keeps the runtime healthy
        // when called from async paths.
        let inner = self.inner.clone();
        let texts = texts.to_vec();
        tokio::task::spawn_blocking(move || {
            Ok(texts.iter().map(|t| inner.encode_one(t).to_vec()).collect())
        })
        .await
        .map_err(|e| anyhow::anyhow!("join error: {e}"))?
    }
}

use crate::config::EmbeddingConfig;
use crate::search::download::{ensure_model, ModelInfo};
use crate::search::http_embedder::{HttpEmbedder, HttpEmbedderStats};
use crate::search::ollama::OllamaEmbedder;
use anyhow::Context;

/// `(embedder, optional HTTP embedder stats handle)`. The second element is
/// `Some` only for the HTTP provider — observability lane for the indexer to
/// snapshot request / retry / char counters. Non-HTTP callers can simply
/// discard it.
pub type EmbedderWithStats = (Box<dyn Embedder>, Option<std::sync::Arc<HttpEmbedderStats>>);

/// Build an embedder according to the (optional) `EmbeddingConfig`. When the
/// config is `None`, returns `None` so callers can decide to fall through to
/// the legacy in-line potion path (matching `Indexer::try_embed`'s behaviour).
pub fn make_embedder(cfg: Option<&EmbeddingConfig>) -> anyhow::Result<Option<EmbedderWithStats>> {
    let Some(cfg) = cfg else {
        return Ok(None);
    };
    match cfg.provider.as_str() {
        "http" => {
            let emb = HttpEmbedder::from_config(cfg)?;
            let stats = Some(emb.stats());
            Ok(Some((Box::new(emb), stats)))
        }
        "ollama" => {
            // Ollama runs locally — no HTTP embedder stats to share. The indexer
            // observability lane will simply skip stats for this provider.
            let emb = OllamaEmbedder::from_config(cfg)?;
            Ok(Some((Box::new(emb), None)))
        }
        "potion-local" => {
            let dir = ensure_model(&ModelInfo::potion_code_16m())
                .map_err(|e| anyhow::anyhow!("downloading potion: {e}"))?;
            let raw =
                std::sync::Arc::new(PotionEmbedder::open(&dir).context("opening potion embedder")?);
            Ok(Some((Box::new(LocalPotion::new(raw)), None)))
        }
        other => anyhow::bail!("unknown embedding provider: {other}"),
    }
}

/// Encode a query string using the configured embedder when present. Falls
/// back to the legacy in-line potion path (`embed::try_encode_query`) when
/// either no embedding is configured or the configured embedder fails — keeping
/// query-time behaviour permissive (a missing embedding API key shouldn't blow
/// up search; it should silently drop to BM25-only).
pub async fn encode_query_async(cfg: Option<&EmbeddingConfig>, query: &str) -> Option<Vec<f32>> {
    if let Ok(Some((emb, _stats))) = make_embedder(cfg) {
        let prepared = prepare_query(cfg, query);
        if let Ok(v) = emb.encode_one(&prepared).await {
            return Some(v);
        }
    }
    crate::search::embed::try_encode_query(query)
}

/// Apply model-specific instruction wrapping when needed.
///
/// Qwen3-Embedding requires an `Instruct: ... \nQuery:...` prefix on the
/// **query side only**; chunks are embedded as-is. Per the Qwen3 model card,
/// omitting this prefix degrades retrieval by 1-5%. The instruction text
/// should be in English even for non-English corpora (model was trained that
/// way).
///
/// Other providers / models get the raw query.
fn prepare_query(cfg: Option<&EmbeddingConfig>, query: &str) -> String {
    let Some(c) = cfg else {
        return query.to_string();
    };
    if c.provider == "ollama" && c.model.starts_with("qwen3-embedding") {
        format!(
            "Instruct: Given a question in Chinese or English, retrieve code chunks that implement or relate to it\nQuery:{query}"
        )
    } else {
        query.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ConstEmbedder(usize);
    #[async_trait]
    impl Embedder for ConstEmbedder {
        fn dim(&self) -> usize {
            self.0
        }
        async fn encode_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| vec![0.0; self.0]).collect())
        }
    }

    #[tokio::test]
    async fn encode_one_falls_back_to_encode_batch() {
        let e = ConstEmbedder(8);
        let v = e.encode_one("hi").await.unwrap();
        assert_eq!(v.len(), 8);
    }

    #[tokio::test]
    #[ignore] // network-gated like the existing potion tests
    async fn local_potion_encodes_to_unit_vector() {
        use crate::search::download::{ensure_model, ModelInfo};
        use crate::search::embed::Embedder as PotionEmbedder;
        let dir = ensure_model(&ModelInfo::potion_code_16m()).unwrap();
        let raw = std::sync::Arc::new(PotionEmbedder::open(&dir).unwrap());
        let local = LocalPotion::new(raw);
        let out = local
            .encode_batch(&["fn add(a: i32) -> i32 { a + 1 }".into()])
            .await
            .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].len(), local.dim());
        let norm: f32 = out[0].iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-4);
    }
}
