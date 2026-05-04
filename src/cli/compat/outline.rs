//! `repolayer outline` — print structural outline of source files.
//!
//! Mirrors the `ast-outline outline` command: renders signatures and line
//! ranges without method bodies. Supports files and directories; directories
//! are walked recursively, honouring `.gitignore`.

use anyhow::Result;
use std::path::PathBuf;

pub async fn run(paths: Vec<PathBuf>, json: bool) -> Result<()> {
    use crate::adapters::parse_file;
    use crate::core::declaration::OutlineOptions;
    use crate::outline::render::{render_json_outline, render_outline};

    let opts = OutlineOptions::default();

    // Collect all ParseResults first so we can pass a slice to render_json_outline.
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
        println!("{}", render_json_outline(&results, &opts, true));
    } else {
        for pr in &results {
            print!("{}", render_outline(pr, &opts));
        }
    }

    Ok(())
}
