//! `repolayer show <file> <Symbol>...` — extract source body of one or more symbols.

use anyhow::Result;
use std::path::PathBuf;

pub async fn run(file: PathBuf, symbols: Vec<String>, json: bool) -> Result<()> {
    use crate::adapters::parse_file;
    use crate::outline::render::{find_symbols, render_json_show};

    let pr = parse_file(&file)
        .ok_or_else(|| anyhow::anyhow!("no adapter for {}", file.display()))?;

    if json {
        let mut all_matches = Vec::new();
        for symbol in &symbols {
            let matches = find_symbols(&pr, symbol);
            all_matches.extend(matches);
        }
        println!("{}", render_json_show(&pr, &all_matches, true));
        return Ok(());
    }

    // Text mode: print source for each matched symbol.
    let mut any_found = false;
    for symbol in &symbols {
        let matches = find_symbols(&pr, symbol);
        if matches.is_empty() {
            eprintln!("symbol not found: {symbol}");
            continue;
        }
        for m in matches {
            any_found = true;
            println!("// {} (lines {}-{})", m.qualified_name, m.start_line, m.end_line);
            println!("{}", m.source);
        }
    }

    if !any_found && !symbols.is_empty() {
        // At least one symbol was requested but nothing matched.
        // Exit 0 — callers check stderr for "not found" messages.
    }

    Ok(())
}
