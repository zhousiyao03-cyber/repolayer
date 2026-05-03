use anyhow::{anyhow, Result};
use async_trait::async_trait;

/// Generate dense vector embeddings for arbitrary text.
///
/// Used for semantic reranking in `find_context` (planned for v0.2).
/// MVP ships with [`NotImplementedEmbedding`] — providers like OpenAI,
/// Voyage, or self-hosted text-embedding-inference services should be
/// implemented in subsequent releases.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Returns the embedding dimension (e.g. 1024 for OpenAI text-embedding-3-small at 1024d).
    fn dim(&self) -> usize;

    /// Embed a single text snippet.
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
}

/// Placeholder implementation for providers that don't (yet) expose a real
/// embedding API (e.g. Anthropic, DeepSeek). Always returns an error.
pub struct NotImplementedEmbedding {
    provider_name: String,
}

impl NotImplementedEmbedding {
    pub fn new(provider_name: &str) -> Self {
        Self {
            provider_name: provider_name.into(),
        }
    }
}

#[async_trait]
impl EmbeddingProvider for NotImplementedEmbedding {
    fn dim(&self) -> usize {
        0
    }

    async fn embed(&self, _text: &str) -> Result<Vec<f32>> {
        Err(anyhow!(
            "embedding API not implemented for provider {}; \
             use OpenAI / Voyage / self-hosted TEI in a future release",
            self.provider_name
        ))
    }
}

// TODO(v0.2):
//  - implement OpenAIEmbedding (text-embedding-3-small, 1024d)
//  - implement VoyageEmbedding (voyage-3, 1024d)
//  - integrate sqlite-vec virtual table (vec0) for vector storage
//  - rerank `find_context` results using cosine similarity (e.g. 0.7 vec + 0.3 substring)
