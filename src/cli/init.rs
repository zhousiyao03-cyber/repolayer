use anyhow::{bail, Result};
use std::path::PathBuf;

const TEMPLATE: &str = r#"# repolayer.yml — describe the repos to index together
repos:
  - path: ./
  # - path: ../another_repo
  # - path: ../my_idl_repo
  #   type: idl

# links:
#   - from: bff
#     to: backend_api
#     kind: http

# llm:
#   enabled: false
#   provider: anthropic
#   api_key_env: ANTHROPIC_API_KEY
#   summary: false
#   query_translation: false
"#;

const HTTP_EMBEDDING_BLOCK: &str = r#"
# ── Optional: HTTP embedding provider (OpenAI-compatible) ──
# embedding:
#   provider: http
#   model: your-embedding-model
#   endpoint: https://api.openai.com/v1/embeddings
#   api_key_env: EMBEDDING_API_KEY
#   dim: 1024
#   batch_size: 8     # adjust to your provider's batch limit
#   max_concurrent: 4
#   request_timeout_ms: 30000
#   qpm_cap: 120

# ── Optional: LLM-generated Chinese summary lane (boosts NL business queries) ──
# summary:
#   enabled: true
#   provider: deepseek
#   api_key_env: DEEPSEEK_API_KEY
#   base_url: https://api.deepseek.com
#   max_chunk_chars: 4000
#   max_concurrent: 4
#   min_file_bytes: 200
#   min_chunks_per_file: 2
#   # path_blacklist defaults cover tests/.d.ts/__generated__/_pb.go/dist/build/.next
"#;

pub async fn run() -> Result<()> {
    let path = PathBuf::from("repolayer.yml");
    if path.exists() {
        bail!("repolayer.yml already exists");
    }
    let contents = format!("{TEMPLATE}{HTTP_EMBEDDING_BLOCK}");
    std::fs::write(&path, contents)?;
    println!("created repolayer.yml");
    Ok(())
}
