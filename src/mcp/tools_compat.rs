//! ast-outline-compat MCP tools. Wraps query/render functions in a
//! schema-versioned MCP-friendly interface.
//!
//! Each function is implemented as a method on `Tools` (defined in
//! `mcp/tools.rs`) so that the `#[rmcp::tool_router]` impl in `mcp/mod.rs`
//! can dispatch through the same `Arc<Tools>` that powers the original tools.

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::mcp::tools::Tools;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct OutlineArgs {
    /// Files or directories to outline. Each entry may be an absolute or
    /// relative path; directories are walked recursively, honouring
    /// `.gitignore`.
    pub paths: Vec<String>,
    /// If true, return machine-readable JSON (schema `ast-outline.outline.v1`)
    /// instead of the human-readable terminal format.
    #[serde(default)]
    pub json: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ShowArgs {
    /// Absolute or relative path to the source file to extract symbols from.
    pub file: String,
    /// Symbol names to extract (suffix-matching: `"add"` or `"math.add"` both work).
    /// Pass multiple names to fetch several symbols in one call.
    pub symbols: Vec<String>,
    /// If true, return machine-readable JSON (schema `ast-outline.show.v1`)
    /// instead of annotated source text.
    #[serde(default)]
    pub json: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SurfaceArgs {
    /// Path to the package root (absolute or relative). Auto-detects manifest:
    /// Cargo.toml (Rust), pyproject.toml / __init__.py (Python),
    /// package.json (TypeScript/JavaScript). Defaults to `.` (current directory).
    #[serde(default)]
    pub path: Option<String>,
    /// If true, return machine-readable JSON (schema `ast-outline.surface.v1`)
    /// instead of the human-readable terminal format.
    #[serde(default)]
    pub json: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DigestArgs {
    /// Files or directories to digest. Each entry may be an absolute or
    /// relative path; directories are walked recursively, honouring
    /// `.gitignore`.
    pub paths: Vec<String>,
    /// If true, return machine-readable JSON (schema `ast-outline.digest.v1`)
    /// instead of the human-readable terminal format.
    #[serde(default)]
    pub json: bool,
}

impl Tools {
    /// Extract the source body of one or more symbols from a file.
    pub fn show(&self, args: ShowArgs) -> anyhow::Result<Value> {
        use crate::adapters::parse_file;
        use crate::core::schema::JSON_SCHEMA_SHOW;
        use crate::outline::render::{find_symbols, render_json_show};
        use std::path::Path;

        let path = Path::new(&args.file);
        let pr = parse_file(path)
            .ok_or_else(|| anyhow::anyhow!("no adapter for {}", args.file))?;

        if args.json {
            let mut all_matches = Vec::new();
            for symbol in &args.symbols {
                all_matches.extend(find_symbols(&pr, symbol));
            }
            let raw = render_json_show(&pr, &all_matches, false);
            let parsed: Value = serde_json::from_str(&raw)?;
            return Ok(parsed);
        }

        // Text mode: collect annotated source snippets.
        let mut entries: Vec<Value> = Vec::new();
        let mut not_found: Vec<String> = Vec::new();
        for symbol in &args.symbols {
            let matches = find_symbols(&pr, symbol);
            if matches.is_empty() {
                not_found.push(symbol.clone());
                continue;
            }
            for m in matches {
                entries.push(serde_json::json!({
                    "symbol": m.qualified_name,
                    "start_line": m.start_line,
                    "end_line": m.end_line,
                    "source": m.source,
                }));
            }
        }

        Ok(serde_json::json!({
            "schema_version": JSON_SCHEMA_SHOW,
            "file": args.file,
            "matches": entries,
            "not_found": not_found,
        }))
    }
}

impl Tools {
    /// Outline one or more files / directories. Returns either the
    /// terminal-formatted text or a schema-versioned JSON document.
    pub fn outline(&self, args: OutlineArgs) -> anyhow::Result<Value> {
        use crate::adapters::parse_file;
        use crate::core::declaration::OutlineOptions;
        use crate::core::schema::JSON_SCHEMA_OUTLINE;
        use crate::outline::render::{render_json_outline, render_outline};
        use std::path::Path;

        let opts = OutlineOptions::default();
        let mut results = Vec::new();

        for spec in &args.paths {
            let path = Path::new(spec);
            if path.is_file() {
                if let Some(pr) = parse_file(path) {
                    results.push(pr);
                }
            } else if path.is_dir() {
                for entry in ignore::WalkBuilder::new(path).build().flatten() {
                    if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                        if let Some(pr) = parse_file(entry.path()) {
                            results.push(pr);
                        }
                    }
                }
            }
            // unknown / missing paths are silently skipped (MCP callers should
            // validate paths before calling — we don't want to error the whole
            // request for one bad entry).
        }

        if args.json {
            // render_json_outline already wraps with schema + files array.
            let raw = render_json_outline(&results, &opts, false);
            let parsed: Value = serde_json::from_str(&raw)?;
            Ok(parsed)
        } else {
            // Plain text: collect per-file outlines and wrap in a top-level
            // JSON envelope so MCP callers always receive structured data.
            let text_chunks: Vec<Value> = results
                .iter()
                .map(|pr| Value::String(render_outline(pr, &opts)))
                .collect();

            Ok(serde_json::json!({
                "schema_version": JSON_SCHEMA_OUTLINE,
                "format": "text",
                "files": text_chunks,
            }))
        }
    }
}

impl Tools {
    /// Digest one or more files / directories (compact public API map).
    /// Returns either the terminal-formatted text or a schema-versioned JSON document.
    pub fn digest(&self, args: DigestArgs) -> anyhow::Result<Value> {
        use crate::adapters::parse_file;
        use crate::core::declaration::DigestOptions;
        use crate::core::schema::JSON_SCHEMA_DIGEST;
        use crate::outline::render::render_digest;
        use std::path::Path;

        let opts = DigestOptions::default();
        let mut results = Vec::new();

        for spec in &args.paths {
            let path = Path::new(spec);
            if path.is_file() {
                if let Some(pr) = parse_file(path) {
                    results.push(pr);
                }
            } else if path.is_dir() {
                for entry in ignore::WalkBuilder::new(path).build().flatten() {
                    if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                        if let Some(pr) = parse_file(entry.path()) {
                            results.push(pr);
                        }
                    }
                }
            }
            // unknown / missing paths are silently skipped (MCP callers should
            // validate paths before calling — we don't want to error the whole
            // request for one bad entry).
        }

        if args.json {
            // JSON mode: serialize as an envelope with file results.
            let files: Vec<Value> = results
                .iter()
                .map(|pr| serde_json::to_value(pr).unwrap())
                .collect();
            Ok(serde_json::json!({
                "schema_version": JSON_SCHEMA_DIGEST,
                "files": files,
            }))
        } else {
            // Plain text: use render_digest and wrap in envelope.
            let text = render_digest(&results, &opts, None);
            Ok(serde_json::json!({
                "schema_version": JSON_SCHEMA_DIGEST,
                "format": "text",
                "text": text,
            }))
        }
    }
}

impl Tools {
    /// Resolve the published public API surface of a package.
    ///
    /// Follows `pub use` re-exports (Rust), `__all__` (Python), barrel files /
    /// `export` clauses (TypeScript), and `export` clauses (Scala) to return
    /// the exact set of symbols a downstream consumer can reach.
    pub fn surface(&self, args: SurfaceArgs) -> anyhow::Result<Value> {
        use crate::core::schema::JSON_SCHEMA_SURFACE;
        use crate::surface::options::{OutputMode, SurfaceOptions};
        use crate::surface::render;
        use std::path::Path;

        let path_str = args.path.unwrap_or_else(|| ".".to_string());
        let path = Path::new(&path_str);

        let opts = if args.json {
            SurfaceOptions {
                output: OutputMode::Json { compact: false },
                ..SurfaceOptions::default()
            }
        } else {
            SurfaceOptions::default()
        };

        let entries = crate::surface::resolve_surface(path, &opts)
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        if args.json {
            // The JSON renderer already produces the schema-versioned document;
            // parse it back so the MCP envelope stays consistent.
            let raw = render::render(&entries, opts.output, opts.include_chain);
            let parsed: Value = serde_json::from_str(&raw)?;
            Ok(parsed)
        } else {
            let text = render::render(&entries, opts.output, opts.include_chain);
            Ok(serde_json::json!({
                "schema_version": JSON_SCHEMA_SURFACE,
                "format": "text",
                "text": text,
            }))
        }
    }
}
