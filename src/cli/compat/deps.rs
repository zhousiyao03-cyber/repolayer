//! `repolayer deps <path>` — show forward import dependencies of a file
//! (or all files in a directory) up to `--depth` hops.

use anyhow::Result;
use std::path::PathBuf;

/// Walk up from `start` looking for a workspace marker file.
pub(crate) fn find_workspace_root(start: &std::path::Path) -> Option<PathBuf> {
    let mut cur = start.to_path_buf();
    loop {
        for marker in &[
            ".git",
            "Cargo.toml",
            "package.json",
            "pyproject.toml",
            "go.mod",
            "repolayer.yml",
        ] {
            if cur.join(marker).exists() {
                return Some(cur);
            }
        }
        match cur.parent() {
            Some(p) => cur = p.to_path_buf(),
            None => return None,
        }
    }
}

pub async fn run(path: PathBuf, depth: usize, json: bool) -> Result<()> {
    use crate::core::schema::JSON_SCHEMA_DEPS;

    let canonical = path.canonicalize().unwrap_or(path.clone());

    let workspace = if canonical.is_file() {
        canonical.parent().unwrap_or(&canonical).to_path_buf()
    } else {
        canonical.clone()
    };

    let workspace_root = find_workspace_root(&workspace).unwrap_or(workspace);

    let g = super::load_or_build_dep_graph(&workspace_root)?;

    // Collect starting nodes: if path is a file, just that file;
    // if it's a dir, all files under it that appear in the graph.
    let queries: Vec<PathBuf> = if canonical.is_file() {
        vec![canonical.clone()]
    } else {
        g.forward
            .keys()
            .filter(|p| p.starts_with(&canonical))
            .cloned()
            .collect()
    };

    let mut all_edges: Vec<(PathBuf, PathBuf, String, u32)> = Vec::new();
    for q in &queries {
        let mut visited = std::collections::HashSet::new();
        let mut frontier = vec![q.clone()];
        visited.insert(q.clone());
        for _ in 0..depth.max(1) {
            let mut next = Vec::new();
            for p in &frontier {
                if let Some(edges) = g.forward.get(p) {
                    for e in edges {
                        all_edges.push((
                            p.clone(),
                            e.target.clone(),
                            e.kind.label().to_string(),
                            e.line,
                        ));
                        if visited.insert(e.target.clone()) {
                            next.push(e.target.clone());
                        }
                    }
                }
            }
            frontier = next;
            if frontier.is_empty() {
                break;
            }
        }
    }

    if json {
        let entries: Vec<_> = all_edges
            .iter()
            .map(|(from, to, kind, line)| {
                serde_json::json!({
                    "from": from.display().to_string(),
                    "to": to.display().to_string(),
                    "kind": kind,
                    "line": line,
                })
            })
            .collect();
        let envelope = serde_json::json!({
            "schema_version": JSON_SCHEMA_DEPS,
            "edges": entries,
        });
        println!("{}", serde_json::to_string_pretty(&envelope)?);
    } else {
        for (from, to, kind, line) in &all_edges {
            let line_suffix = if *line > 0 {
                format!(" L{}", line)
            } else {
                String::new()
            };
            println!(
                "{} -> {} [{}{}]",
                from.display(),
                to.display(),
                kind,
                line_suffix
            );
        }
        if all_edges.is_empty() {
            eprintln!("no dependencies found for {}", canonical.display());
        }
    }
    Ok(())
}
