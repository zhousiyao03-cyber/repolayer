use crate::graph::store::Store;
use crate::search::store::SearchStore;
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

    // search.db is optional — if open fails, find_context degrades to its
    // substring-only path rather than blocking the server.
    let search_db_path = PathBuf::from(".repolayer/search.db");
    let search_store = if search_db_path.exists() {
        match SearchStore::open(&search_db_path) {
            Ok(s) => Some(Arc::new(Mutex::new(s))),
            Err(e) => {
                tracing::warn!(
                    "search.db present but failed to open ({}); find_context will use substring only",
                    e
                );
                None
            }
        }
    } else {
        None
    };

    crate::mcp::run_stdio(store, search_store).await
}
