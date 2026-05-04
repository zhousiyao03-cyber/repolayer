//! `repolayer cycles [path]` — detect import cycles via Tarjan SCC.
//! Exits with code 1 if any cycle is found (CI-gateable).

use anyhow::Result;
use std::path::PathBuf;

pub async fn run(path: Option<PathBuf>, json: bool) -> Result<()> {
    use crate::core::schema::JSON_SCHEMA_CYCLES;

    let workspace = path.unwrap_or_else(|| std::env::current_dir().unwrap());
    let workspace_root =
        super::deps::find_workspace_root(&workspace).unwrap_or(workspace);

    let g = super::load_or_build_dep_graph(&workspace_root)?;

    // detect() returns Cycle structs; filter out singletons (len == 1 without
    // self-edge is already handled inside scc::detect with min_size = 2).
    let cycles = crate::deps::scc::detect(&g, 2);
    let cycle_groups: Vec<Vec<PathBuf>> =
        cycles.into_iter().map(|c| c.members).collect();

    if json {
        let entries: Vec<_> = cycle_groups
            .iter()
            .map(|cycle| {
                cycle
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
            })
            .collect();
        let envelope = serde_json::json!({
            "schema_version": JSON_SCHEMA_CYCLES,
            "cycles": entries,
        });
        println!("{}", serde_json::to_string_pretty(&envelope)?);
    } else if cycle_groups.is_empty() {
        println!("no cycles detected");
    } else {
        println!("found {} cycle(s):", cycle_groups.len());
        for (i, cycle) in cycle_groups.iter().enumerate() {
            println!("  cycle #{}: {} files", i + 1, cycle.len());
            for p in cycle {
                println!("    {}", p.display());
            }
        }
    }

    // CI gate: non-zero exit if any cycle found.
    if !cycle_groups.is_empty() {
        std::process::exit(1);
    }
    Ok(())
}
