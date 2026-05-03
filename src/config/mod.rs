mod schema;

pub use schema::*;

use anyhow::{Context, Result};
use std::path::Path;

impl Config {
    pub fn from_path(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("config file not found: {}", path.display()))?;
        let cfg: Config = serde_yml::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(cfg)
    }
}
