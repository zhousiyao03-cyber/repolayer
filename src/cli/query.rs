use crate::graph::store::Store;
use anyhow::{bail, Result};
use std::path::PathBuf;

pub async fn run(text: String) -> Result<()> {
    let db_path = PathBuf::from(".repolayer/index.db");
    if !db_path.exists() {
        bail!(
            "no index found at {} — run `repolayer build` first",
            db_path.display()
        );
    }
    let store = Store::open(&db_path)?;
    let results = store.search_symbols_substring(&text, 20)?;
    if results.is_empty() {
        println!("no matches");
        return Ok(());
    }
    for n in results {
        println!(
            "{}\t{}::{}\t{}",
            n.repo,
            n.path,
            n.symbol.as_deref().unwrap_or(""),
            n.loc_start.map(|l| l.to_string()).unwrap_or_default(),
        );
    }
    Ok(())
}
