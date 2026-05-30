use repolayer::config::Config;

const YML_NEW: &str = r#"
repos:
  - { name: a, path: /tmp/a }
embedding:
  provider: http
  model: your-embedding-model
  endpoint: https://api.example.com/v1/embeddings
  api_key_env: EMBEDDING_API_KEY
  dim: 1024
  batch_size: 32
  max_concurrent: 4
  request_timeout_ms: 30000
summary:
  enabled: true
  provider: deepseek
  api_key_env: DEEPSEEK_API_KEY
  base_url: https://api.deepseek.com
  max_chunk_chars: 4000
  max_concurrent: 4
  min_chunks_per_file: 2
  path_blacklist:
    - ".test."
    - "_test."
    - ".d.ts"
"#;

const YML_BACKCOMPAT: &str = r#"
repos:
  - { name: a, path: /tmp/a }
"#;

#[test]
fn parses_new_schema_with_http_and_summary() {
    let cfg: Config = serde_yml::from_str(YML_NEW).expect("parse new yml");
    let emb = cfg.embedding.expect("embedding block");
    assert_eq!(emb.provider, "http");
    assert_eq!(emb.model, "your-embedding-model");
    assert_eq!(emb.dim, 1024);
    assert_eq!(emb.batch_size, 32);
    let sum = cfg.summary.expect("summary block");
    assert!(sum.enabled);
    assert_eq!(sum.provider, "deepseek");
    assert_eq!(sum.min_chunks_per_file, 2);
    assert!(sum.path_blacklist.iter().any(|p| p == ".test."));
}

#[test]
fn parses_back_compat_yml_without_new_blocks() {
    let cfg: Config = serde_yml::from_str(YML_BACKCOMPAT).expect("parse back-compat yml");
    assert!(cfg.embedding.is_none(), "embedding should default to None");
    assert!(cfg.summary.is_none(), "summary should default to None");
}
