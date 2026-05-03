use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    Repo,
    Module,
    Symbol,
    IdlService,
    IdlMethod,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EdgeKind {
    Contains,
    Imports,
    Calls,
    Implements,
    Invokes,
    Defines,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub kind: NodeKind,
    pub repo: String,
    pub path: String,
    pub symbol: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub loc_start: Option<u32>,
    #[serde(default)]
    pub loc_end: Option<u32>,
}

impl Node {
    pub fn new(kind: NodeKind, repo: &str, path: &str, symbol: Option<&str>) -> Self {
        let id = compute_id(kind, repo, path, symbol);
        Self {
            id,
            kind,
            repo: repo.into(),
            path: path.into(),
            symbol: symbol.map(String::from),
            summary: None,
            owner: None,
            loc_start: None,
            loc_end: None,
        }
    }
}

impl NodeKind {
    /// Stable string tag used in id hashing.
    /// **Do not change these strings — doing so invalidates all existing node ids.**
    fn id_tag(self) -> &'static str {
        match self {
            NodeKind::Repo => "repo",
            NodeKind::Module => "module",
            NodeKind::Symbol => "symbol",
            NodeKind::IdlService => "idlservice",
            NodeKind::IdlMethod => "idlmethod",
        }
    }
}

fn compute_id(kind: NodeKind, repo: &str, path: &str, symbol: Option<&str>) -> String {
    let mut h = Sha256::new();
    h.update(kind.id_tag().as_bytes());
    h.update(b"\0");
    h.update(repo.as_bytes());
    h.update(b"\0");
    h.update(path.as_bytes());
    h.update(b"\0");
    if let Some(s) = symbol {
        h.update(s.as_bytes());
    }
    let bytes = h.finalize();
    hex::encode(&bytes[..16])
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub kind: EdgeKind,
}
