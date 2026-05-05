use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub repos: Vec<RepoConfig>,
    #[serde(default)]
    pub links: Vec<LinkConfig>,
    pub llm: Option<LlmConfig>,
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
    /// `["code.byted.org/oec/http_idl_gen"]` for an `http_idl` source repo.
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
