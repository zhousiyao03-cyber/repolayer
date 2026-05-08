use crate::cli::repo_filter::require_repo;
use crate::cli::workspace;
use crate::graph::store::Store;
use anyhow::{bail, Result};

pub async fn run(text: String, repo: Option<String>, json: bool) -> Result<()> {
    let db_path = workspace::store_path("index.db")?;
    if !db_path.exists() {
        bail!(
            "no index found at {} — run `repolayer build` first (or set $REPOLAYER_INDEX to a workspace that has one)",
            db_path.display()
        );
    }
    let store = Store::open(&db_path)?;

    // Validate `--repo` against the index's known repos before querying.
    // This produces a "did you mean ..." error on typos, which is much
    // friendlier to agents than silently returning zero hits.
    let validated_repo = match repo.as_deref() {
        None => None,
        Some(name) => {
            let known = store.list_repo_names()?;
            Some(require_repo(name, &known)?.to_string())
        }
    };

    let results = store.search_symbols_substring_filtered(
        &text,
        validated_repo.as_deref(),
        20,
    )?;

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
            "repo_filter": validated_repo,
            "matches": entries,
        });
        println!("{}", serde_json::to_string_pretty(&envelope)?);
        return Ok(());
    }

    let scope = match validated_repo.as_deref() {
        Some(r) => format!(" in repo={}", r),
        None => String::new(),
    };
    if results.is_empty() {
        println!("# no matches for '{}'{}", text, scope);
        if validated_repo.is_some() {
            println!("# fallback: drop --repo to widen, or try `repolayer search` for fuzzy / semantic matches");
        } else {
            println!("# fallback: try `repolayer search \"{}\"` for fuzzy / semantic matches, or `rg` for literal lookup", text);
        }
        return Ok(());
    }
    println!(
        "# {} matches for '{}'{} — repo\tpath::symbol\tline",
        results.len(),
        text,
        scope,
    );
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
