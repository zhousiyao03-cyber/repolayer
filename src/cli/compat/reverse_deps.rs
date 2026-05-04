//! `repolayer reverse-deps <path>` — show who imports a given file
//! (refactor blast radius).

use anyhow::Result;
use std::path::PathBuf;

pub async fn run(path: PathBuf, json: bool) -> Result<()> {
    use crate::core::schema::JSON_SCHEMA_REVERSE_DEPS;

    let canonical = path.canonicalize().unwrap_or(path.clone());

    let workspace = if canonical.is_file() {
        canonical.parent().unwrap_or(&canonical).to_path_buf()
    } else {
        canonical.clone()
    };
    let workspace_root =
        super::deps::find_workspace_root(&workspace).unwrap_or(workspace);

    let g = super::load_or_build_dep_graph(&workspace_root)?;

    // Find all source files whose forward edges point at `canonical`.
    let mut callers: Vec<(PathBuf, String, u32)> = Vec::new();
    for (from, edges) in &g.forward {
        for e in edges {
            if e.target == canonical {
                callers.push((from.clone(), e.kind.label().to_string(), e.line));
            }
        }
    }
    callers.sort_by(|a, b| a.0.cmp(&b.0));

    if json {
        let entries: Vec<_> = callers
            .iter()
            .map(|(from, kind, line)| {
                serde_json::json!({
                    "from": from.display().to_string(),
                    "kind": kind,
                    "line": line,
                })
            })
            .collect();
        let envelope = serde_json::json!({
            "schema_version": JSON_SCHEMA_REVERSE_DEPS,
            "target": canonical.display().to_string(),
            "callers": entries,
        });
        println!("{}", serde_json::to_string_pretty(&envelope)?);
    } else if callers.is_empty() {
        eprintln!("no callers found for {}", canonical.display());
    } else {
        println!("Callers of {}:", canonical.display());
        for (from, kind, line) in &callers {
            let line_suffix = if *line > 0 {
                format!(" L{}", line)
            } else {
                String::new()
            };
            println!("  {} [{}{}]", from.display(), kind, line_suffix);
        }
    }
    Ok(())
}
