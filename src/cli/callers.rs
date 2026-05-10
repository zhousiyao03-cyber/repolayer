use crate::cli::repo_filter::require_repo;
use crate::cli::workspace;
use crate::graph::store::Store;
use crate::query::callers::get_callers_all;
use anyhow::{bail, Result};

pub async fn run(symbol: String, depth: usize, repo: Option<String>, json: bool) -> Result<()> {
    let db_path = workspace::store_path("index.db")?;
    if !db_path.exists() {
        bail!(
            "no index found at {} — run `repolayer build` first (or set $REPOLAYER_INDEX to a workspace that has one)",
            db_path.display()
        );
    }
    let store = Store::open(&db_path)?;

    let validated_repo = match repo.as_deref() {
        None => None,
        Some(name) => {
            let known = store.list_repo_names()?;
            Some(require_repo(name, &known)?.to_string())
        }
    };

    let (starts, chains) = get_callers_all(&store, &symbol, depth, validated_repo.as_deref())?;

    if json {
        let starts_json: Vec<serde_json::Value> = starts
            .iter()
            .map(|n| {
                serde_json::json!({
                    "id": n.id,
                    "repo": n.repo,
                    "path": n.path,
                    "symbol": n.symbol,
                    "kind": n.kind,
                    "line": n.loc_start,
                })
            })
            .collect();
        let chains_json: Vec<serde_json::Value> = chains
            .iter()
            .map(|c| {
                serde_json::json!({
                    "caller": {
                        "repo": c.caller.repo,
                        "path": c.caller.path,
                        "symbol": c.caller.symbol,
                        "kind": c.caller.kind,
                        "line": c.caller.loc_start,
                    },
                    "target": {
                        "repo": c.target.repo,
                        "path": c.target.path,
                        "symbol": c.target.symbol,
                    },
                    "confidence": c.confidence,
                })
            })
            .collect();
        let envelope = serde_json::json!({
            "schema_version": "repolayer.get_callers.v1",
            "symbol": symbol,
            "depth": depth,
            "repo_filter": validated_repo,
            "definitions": starts_json,
            "callers": chains_json,
        });
        println!("{}", serde_json::to_string_pretty(&envelope)?);
        return Ok(());
    }

    let scope = match validated_repo.as_deref() {
        Some(r) => format!(" in repo={}", r),
        None => String::new(),
    };

    if starts.is_empty() {
        println!("# no exact match for symbol '{}'{}", symbol, scope);
        println!(
            "# fallback: try `repolayer query \"{}\"` to see candidates (substring match)",
            symbol
        );
        return Ok(());
    }

    println!(
        "# {} definition(s) of '{}'{}, {} caller(s) within depth {}",
        starts.len(),
        symbol,
        scope,
        chains.len(),
        depth,
    );
    for s in &starts {
        println!(
            "@def\t{}\t{}::{}\t{}",
            s.repo,
            s.path,
            s.symbol.as_deref().unwrap_or(""),
            s.loc_start.map(|l| l.to_string()).unwrap_or_default(),
        );
    }
    if chains.is_empty() {
        println!("# no inbound Calls edges");
        println!(
            "# Calls edges are AST-derived where confidence==1.0; absence means \
             no static call site was indexed (dynamic dispatch / reflection / \
             unindexed call sites won't show up)."
        );
        return Ok(());
    }
    println!("# caller -> target  (repo\tpath::symbol\tline\tconfidence)");
    for c in chains {
        println!(
            "{}\t{}::{}\t{}\tconf={:.2}\t-> {}::{}",
            c.caller.repo,
            c.caller.path,
            c.caller.symbol.as_deref().unwrap_or(""),
            c.caller
                .loc_start
                .map(|l| l.to_string())
                .unwrap_or_default(),
            c.confidence,
            c.target.path,
            c.target.symbol.as_deref().unwrap_or(""),
        );
    }
    Ok(())
}
