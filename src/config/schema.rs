use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub repos: Vec<RepoConfig>,
    #[serde(default)]
    pub links: Vec<LinkConfig>,
    pub llm: Option<LlmConfig>,
    #[serde(default)]
    pub embedding: Option<EmbeddingConfig>,
    #[serde(default)]
    pub summary: Option<SummaryConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RepoConfig {
    pub path: PathBuf,
    #[serde(default)]
    pub r#type: Option<RepoType>,
    pub name: Option<String>,
    /// Go / external module path prefixes that map to this repo, used by
    /// the import-based cross-repo linker. If absent, the linker auto-reads
    /// the module path from this repo's `go.mod` for Go projects. IDL repos
    /// (proto/thrift) typically have a separately-generated Go SDK repo and
    /// should declare its module path explicitly here, e.g.
    /// `["github.com/example/idl_gen"]` for an `http_idl` source repo.
    #[serde(default)]
    pub module_aliases: Vec<String>,
}

impl RepoConfig {
    pub fn is_idl(&self) -> bool {
        matches!(self.r#type, Some(RepoType::Idl))
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RepoType {
    Code,
    Idl,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LinkConfig {
    pub from: String,
    pub to: String,
    pub kind: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LlmConfig {
    #[serde(default)]
    pub enabled: bool,
    pub provider: String,
    pub api_key_env: String,
    #[serde(default)]
    pub summary: bool,
    #[serde(default)]
    pub query_translation: bool,
    /// Enable embedding-based reranking in find_context (TODO v0.2 — currently no-op).
    #[serde(default)]
    pub embedding: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct EmbeddingConfig {
    /// "http" | "potion-local". Default "potion-local" preserves old behavior.
    pub provider: String,
    /// Model identifier passed to the HTTP embedding provider. Ignored for
    /// potion-local.
    #[serde(default)]
    pub model: String,
    /// Full URL of the `/v1/embeddings`-shaped endpoint.
    #[serde(default)]
    pub endpoint: String,
    /// Env var holding the API key (sent as `Authorization: Bearer`).
    #[serde(default)]
    pub api_key_env: String,
    /// Output dimension. MUST match the model — written into search.db meta.
    pub dim: u32,
    /// Embeddings per HTTP request. Tune to your provider's per-request limit.
    #[serde(default = "default_batch_size")]
    pub batch_size: u32,
    /// Concurrent in-flight HTTP requests during build.
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: u32,
    #[serde(default = "default_request_timeout_ms")]
    pub request_timeout_ms: u32,
    /// Soft client-side rate cap (defense in depth against quota violations).
    /// Keeps a single build from exhausting the provider's rate limit.
    #[serde(default = "default_qpm_cap")]
    pub qpm_cap: u32,
}

fn default_batch_size() -> u32 {
    // Conservative default; adjust to your provider's per-request batch limit.
    8
}
fn default_max_concurrent() -> u32 {
    4
}
fn default_request_timeout_ms() -> u32 {
    30_000
}
fn default_qpm_cap() -> u32 {
    120
}

#[derive(Debug, Deserialize, Clone)]
pub struct SummaryConfig {
    #[serde(default)]
    pub enabled: bool,
    /// "deepseek" | "anthropic"
    pub provider: String,
    pub api_key_env: String,
    #[serde(default = "default_summary_base_url")]
    pub base_url: String,
    /// Max chars sent to the LLM per chunk (truncation cutoff). 4000 by default.
    #[serde(default = "default_max_chunk_chars")]
    pub max_chunk_chars: u32,
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: u32,
    /// Skip files smaller than this many bytes — they're usually too trivial
    /// to be worth a summary token spend.
    #[serde(default = "default_min_file_bytes")]
    pub min_file_bytes: u32,
    /// Skip files with fewer than this many chunks. Reduces LLM-call cost on
    /// tiny single-export files. Default 2 trades a bit of recall on edge
    /// cases for ~10-15% fewer summary calls.
    #[serde(default = "default_min_chunks_per_file")]
    pub min_chunks_per_file: u32,
    /// Path-substring blacklist. Any file whose repo-relative path contains one
    /// of these substrings is skipped during summary generation. Defaults cover
    /// the common cases (tests, generated, type-only files) — override in yml
    /// when an exception is needed.
    #[serde(default = "default_path_blacklist")]
    pub path_blacklist: Vec<String>,
}

fn default_summary_base_url() -> String {
    "https://api.deepseek.com".to_string()
}
fn default_max_chunk_chars() -> u32 {
    4000
}
fn default_min_file_bytes() -> u32 {
    200
}
fn default_min_chunks_per_file() -> u32 {
    2
}
pub fn default_path_blacklist() -> Vec<String> {
    vec![
        // test fixtures + spec files
        ".test.".into(),
        "_test.".into(),
        ".spec.".into(),
        // pure type declarations
        ".d.ts".into(),
        // generated / vendored
        "/__generated__/".into(),
        "/types/".into(),
        "_pb.go".into(),
        "_pb2.py".into(),
        ".thrift.gen.".into(),
        // build outputs
        "/dist/".into(),
        "/build/".into(),
        "/.next/".into(),
    ]
}
