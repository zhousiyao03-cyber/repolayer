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
