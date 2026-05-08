use crate::graph::store::Store;
use anyhow::{bail, Result};
use std::path::PathBuf;

pub async fn run(text: String, json: bool) -> Result<()> {
    let db_path = PathBuf::from(".repolayer/index.db");
    if !db_path.exists() {
        bail!(
            "no index found at {} — run `repolayer build` first",
            db_path.display()
        );
    }
    let store = Store::open(&db_path)?;
    let results = store.search_symbols_substring(&text, 20)?;

    if json {
        let entries: Vec<serde_json::Value> = results
            .iter()
            .map(|n| {
                serde_json::json!({
                    "repo": n.repo,
                    "path": n.path,
                    "symbol": n.symbol,
                    "kind": n.kind,
                    "line": n.loc_start,
                })
            })
            .collect();
        let envelope = serde_json::json!({
            "schema_version": "repolayer.query.v1",
            "query": text,
            "matches": entries,
        });
        println!("{}", serde_json::to_string_pretty(&envelope)?);
        return Ok(());
    }

    if results.is_empty() {
        println!("# no matches for '{}'", text);
        println!("# fallback: try `repolayer search \"{}\"` for fuzzy / semantic matches, or `rg` for literal lookup", text);
        return Ok(());
    }
    println!("# {} matches for '{}' — repo\tpath::symbol\tline", results.len(), text);
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
