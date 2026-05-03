use crate::graph::store::Store;
use anyhow::{bail, Result};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub async fn run(http: Option<String>) -> Result<()> {
    if http.is_some() {
        bail!("HTTP transport not implemented yet (v0.2). Use stdio.");
    }
    let db_path = PathBuf::from(".repolayer/index.db");
    if !db_path.exists() {
        bail!(
            "no index found at {} — run `repolayer build` first",
            db_path.display()
        );
    }
    let store = Arc::new(Mutex::new(Store::open(&db_path)?));
    crate::mcp::run_stdio(store).await
}
