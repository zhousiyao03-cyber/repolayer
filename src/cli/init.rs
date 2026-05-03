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

pub async fn run() -> Result<()> {
    let path = PathBuf::from("repolayer.yml");
    if path.exists() {
        bail!("repolayer.yml already exists");
    }
    std::fs::write(&path, TEMPLATE)?;
    println!("created repolayer.yml");
    Ok(())
}
