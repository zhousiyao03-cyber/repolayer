use crate::cli::workspace;
use crate::graph::store::Store;
use crate::query::find_idl_impl::{find_idl_impl, FindIdlImplArgs};
use anyhow::{bail, Result};

pub async fn run(
    method: String,
    service: Option<String>,
    no_implements: bool,
    no_invokes: bool,
    json: bool,
) -> Result<()> {
    let db_path = workspace::store_path("index.db")?;
    if !db_path.exists() {
        bail!(
            "no index found at {} — run `repolayer build` first (or set $REPOLAYER_INDEX to a workspace that has one)",
            db_path.display()
        );
    }
    let store = Store::open(&db_path)?;

    let args = FindIdlImplArgs {
        method: method.clone(),
        service,
        include_invokes: !no_invokes,
        include_implements: !no_implements,
    };
    let result = find_idl_impl(&store, &args)?;

    if json {
        let json_val = serde_json::to_value(&result)?;
        println!("{}", serde_json::to_string_pretty(&json_val)?);
        return Ok(());
    }

    let Some(method_info) = result.method.as_ref() else {
        println!("# no IDL method found matching '{}'", method);
        if let Some(svc) = args.service.as_deref() {
            println!("# (filtered by service='{}'; drop --service to widen)", svc);
        } else {
            println!(
                "# fallback: try `repolayer query \"{}\"` to see substring candidates",
                method
            );
        }
        return Ok(());
    };

    println!(
        "# IDL method: {}::{}\t({}:{})",
        method_info.repo,
        method_info.symbol,
        method_info.path,
        method_info
            .line
            .map(|l| l.to_string())
            .unwrap_or_else(|| "?".to_string()),
    );

    if args.include_implements {
        if result.implements.is_empty() {
            println!("# implements: none");
        } else {
            println!(
                "# {} implementation(s)  (server-side, sorted by confidence desc)",
                result.implements.len()
            );
            for loc in &result.implements {
                println!(
                    "impl\t{}\t{}::{}\tconf={:.2}",
                    loc.repo,
                    loc.path,
                    loc.symbol.as_deref().unwrap_or(""),
                    loc.confidence,
                );
            }
        }
    }

    if args.include_invokes {
        if result.invokes.is_empty() {
            println!("# invokes: none");
        } else {
            println!(
                "# {} invoker(s)  (client-side, sorted by confidence desc)",
                result.invokes.len()
            );
            for loc in &result.invokes {
                println!(
                    "call\t{}\t{}::{}\tconf={:.2}",
                    loc.repo,
                    loc.path,
                    loc.symbol.as_deref().unwrap_or(""),
                    loc.confidence,
                );
            }
        }
    }

    println!("# confidence guide: 1.0=AST exact, 0.7=AST call match, 0.4=path heuristic");
    Ok(())
}
