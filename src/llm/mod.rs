pub mod anthropic;
pub mod deepseek;
pub mod embedding;
pub mod summary;

use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Generate a 1-2 sentence summary of what `code_snippet` does.
    async fn summarize(&self, code_snippet: &str, file_path: &str) -> Result<String>;
}
