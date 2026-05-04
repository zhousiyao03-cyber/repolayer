//! `repolayer digest` — compact public API map of a module.
//!
//! Mirrors the `ast-outline digest` command: renders a one-page overview
//! of signatures and line ranges for all files in the given paths.
//! Supports files and directories; directories are walked recursively,
//! honouring `.gitignore`.

use anyhow::Result;
use std::path::PathBuf;

pub async fn run(paths: Vec<PathBuf>, json: bool) -> Result<()> {
    use crate::adapters::parse_file;
    use crate::core::declaration::DigestOptions;
    use crate::outline::render::render_digest;

    let opts = DigestOptions::default();

    // Collect all ParseResults first so we can pass a slice to render_digest.
    let mut results = Vec::new();

    for path in &paths {
        if path.is_file() {
            if let Some(pr) = parse_file(path) {
                results.push(pr);
            } else {
                eprintln!("warning: no adapter for {}", path.display());
            }
        } else if path.is_dir() {
            for entry in ignore::WalkBuilder::new(path).build().flatten() {
                if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                    if let Some(pr) = parse_file(entry.path()) {
                        results.push(pr);
                    }
                }
            }
        } else {
            eprintln!("warning: path not found: {}", path.display());
        }
    }

    if json {
        // JSON mode: serialize as an envelope with file results.
        let envelope = serde_json::json!({
            "schema_version": "ast-outline.digest.v1",
            "files": results.iter().map(|pr| serde_json::to_value(pr).unwrap()).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&envelope)?);
    } else {
        // Text mode: use render_digest with no root restriction.
        print!("{}", render_digest(&results, &opts, None));
    }

    Ok(())
}
