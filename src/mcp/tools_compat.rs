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

// ── dep-graph args ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DepsArgs {
    /// Absolute or relative path to the file (or directory) to query.
    pub path: String,
    /// Maximum BFS hop depth (default 1 = direct imports only).
    #[serde(default = "default_depth")]
    pub depth: usize,
    /// If true, return machine-readable JSON (schema `ast-outline.deps.v1`).
    #[serde(default)]
    pub json: bool,
}
fn default_depth() -> usize {
    1
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReverseDepsArgs {
    /// Absolute or relative path to the file to look up callers for.
    pub path: String,
    /// If true, return machine-readable JSON (schema `ast-outline.reverse-deps.v1`).
    #[serde(default)]
    pub json: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CyclesArgs {
    /// Workspace root to scan (absolute or relative). Defaults to `.`.
    #[serde(default)]
    pub path: Option<String>,
    /// If true, return machine-readable JSON (schema `ast-outline.cycles.v1`).
    #[serde(default)]
    pub json: bool,
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

impl Tools {
    /// Forward import dependencies of a file (what does X import), up to `depth` hops.
    pub fn deps(&self, args: DepsArgs) -> anyhow::Result<Value> {
        use crate::cli::compat::deps::find_workspace_root;
        use crate::cli::compat::load_or_build_dep_graph;
        use crate::core::schema::JSON_SCHEMA_DEPS;
        use std::path::Path;

        let path = Path::new(&args.path);
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

        let workspace = if canonical.is_file() {
            canonical.parent().unwrap_or(&canonical).to_path_buf()
        } else {
            canonical.clone()
        };
        let workspace_root = find_workspace_root(&workspace).unwrap_or(workspace);

        let g = load_or_build_dep_graph(&workspace_root)?;

        let queries: Vec<std::path::PathBuf> = if canonical.is_file() {
            vec![canonical.clone()]
        } else {
            g.forward
                .keys()
                .filter(|p| p.starts_with(&canonical))
                .cloned()
                .collect()
        };

        let depth = args.depth.max(1);
        let mut all_edges: Vec<serde_json::Value> = Vec::new();
        for q in &queries {
            let mut visited = std::collections::HashSet::new();
            let mut frontier = vec![q.clone()];
            visited.insert(q.clone());
            for _ in 0..depth {
                let mut next = Vec::new();
                for p in &frontier {
                    if let Some(edges) = g.forward.get(p) {
                        for e in edges {
                            let line_suf = if e.line > 0 {
                                format!(" L{}", e.line)
                            } else {
                                String::new()
                            };
                            all_edges.push(serde_json::json!({
                                "from": p.display().to_string(),
                                "to": e.target.display().to_string(),
                                "kind": e.kind.label(),
                                "line": e.line,
                                "line_label": line_suf,
                            }));
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

        Ok(serde_json::json!({
            "schema_version": JSON_SCHEMA_DEPS,
            "edges": all_edges,
        }))
    }
}

impl Tools {
    /// Reverse import dependencies — who imports the given file (refactor blast radius).
    pub fn reverse_deps(&self, args: ReverseDepsArgs) -> anyhow::Result<Value> {
        use crate::cli::compat::deps::find_workspace_root;
        use crate::cli::compat::load_or_build_dep_graph;
        use crate::core::schema::JSON_SCHEMA_REVERSE_DEPS;
        use std::path::Path;

        let path = Path::new(&args.path);
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

        let workspace = if canonical.is_file() {
            canonical.parent().unwrap_or(&canonical).to_path_buf()
        } else {
            canonical.clone()
        };
        let workspace_root = find_workspace_root(&workspace).unwrap_or(workspace);

        let g = load_or_build_dep_graph(&workspace_root)?;

        let mut callers: Vec<serde_json::Value> = Vec::new();
        let mut from_paths: Vec<std::path::PathBuf> = g
            .forward
            .iter()
            .filter(|(_, edges)| edges.iter().any(|e| e.target == canonical))
            .map(|(from, _)| from.clone())
            .collect();
        from_paths.sort();

        for from in &from_paths {
            if let Some(edges) = g.forward.get(from) {
                for e in edges {
                    if e.target == canonical {
                        callers.push(serde_json::json!({
                            "from": from.display().to_string(),
                            "kind": e.kind.label(),
                            "line": e.line,
                        }));
                    }
                }
            }
        }

        Ok(serde_json::json!({
            "schema_version": JSON_SCHEMA_REVERSE_DEPS,
            "target": canonical.display().to_string(),
            "callers": callers,
        }))
    }
}

// ── search + find-related args ───────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchArgs {
    /// Query string — natural language phrase or symbol name.
    pub query: String,
    /// Maximum number of results to return (default 10).
    #[serde(default = "default_k_10")]
    pub k: usize,
    /// If true, return machine-readable JSON (schema `ast-outline.search.v1`).
    #[serde(default)]
    pub json: bool,
}
fn default_k_10() -> usize {
    10
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindRelatedArgs {
    /// Target as `"path/to/file.rs:42"` — the line number identifies which chunk to use.
    pub spec: String,
    /// Maximum number of results to return (default 5).
    #[serde(default = "default_k_5")]
    pub k: usize,
    /// If true, return machine-readable JSON (schema `ast-outline.find_related.v1`).
    #[serde(default)]
    pub json: bool,
}
fn default_k_5() -> usize {
    5
}

impl Tools {
    /// Hybrid BM25 + substring search across the workspace search index.
    pub fn search(&self, args: SearchArgs) -> anyhow::Result<Value> {
        use crate::core::schema::JSON_SCHEMA_SEARCH;

        let workspace = std::env::current_dir()?;
        let db = workspace.join(".repolayer").join("search.db");
        if !db.exists() {
            anyhow::bail!(
                "no search index found at {} — run `repolayer build` first",
                db.display()
            );
        }

        let store = crate::search::store::SearchStore::open(&db)?;
        let qv = crate::search::embed::try_encode_query(&args.query);
        let hits = store.search_hybrid(&args.query, args.k, qv.as_deref(), None)?;

        if args.json {
            let entries: Vec<Value> = hits
                .iter()
                .map(|h| serde_json::to_value(h).unwrap_or_default())
                .collect();
            Ok(serde_json::json!({
                "schema_version": JSON_SCHEMA_SEARCH,
                "query": args.query,
                "hits": entries,
            }))
        } else {
            let entries: Vec<Value> = hits
                .iter()
                .enumerate()
                .map(|(i, h)| {
                    serde_json::json!({
                        "rank": i + 1,
                        "path": h.path,
                        "start_line": h.start_line,
                        "end_line": h.end_line,
                        "repo": h.repo,
                        "score": h.score,
                    })
                })
                .collect();
            Ok(serde_json::json!({
                "schema_version": JSON_SCHEMA_SEARCH,
                "query": args.query,
                "hits": entries,
            }))
        }
    }
}

impl Tools {
    /// Find code chunks similar to a given `file:line` location.
    pub fn find_related(&self, args: FindRelatedArgs) -> anyhow::Result<Value> {
        use crate::core::schema::JSON_SCHEMA_FIND_RELATED;
        use std::path::PathBuf;

        let (file, line): (PathBuf, u32) = match args.spec.rsplit_once(':') {
            Some((f, l)) => (PathBuf::from(f), l.parse().unwrap_or(0)),
            None => (PathBuf::from(&args.spec), 0),
        };

        let workspace = std::env::current_dir()?;
        let db = workspace.join(".repolayer").join("search.db");
        if !db.exists() {
            anyhow::bail!("no search index found — run `repolayer build` first");
        }

        let store = crate::search::store::SearchStore::open(&db)?;

        let canonical = file.canonicalize().unwrap_or_else(|_| file.clone());
        let path_str = canonical.to_string_lossy().to_string();
        let rel_str = file.to_string_lossy().to_string();
        let suffix = canonical
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| rel_str.clone());

        // Find the source chunk (DB stores repo-relative paths).
        let (target_content, stored_path): (String, String) = {
            let conn = store.conn();
            let line_i = line as i64;
            let like_pat = format!("%/{}", suffix);
            let mut stmt = conn.prepare(
                "SELECT content, path FROM chunks
                 WHERE (path = ?1 OR path = ?2 OR path LIKE ?3)
                   AND start_line <= ?4
                   AND end_line >= ?4
                 ORDER BY
                   CASE WHEN path = ?1 OR path = ?2 THEN 0 ELSE 1 END
                 LIMIT 1",
            )?;
            let mut rows = stmt.query(rusqlite::params![path_str, rel_str, like_pat, line_i])?;
            match rows.next()? {
                Some(row) => (row.get::<_, String>(0)?, row.get::<_, String>(1)?),
                None => anyhow::bail!(
                    "no chunk at {}:{} — check path and line number",
                    canonical.display(),
                    line
                ),
            }
        };

        let query: String = target_content
            .split_whitespace()
            .take(50)
            .collect::<Vec<_>>()
            .join(" ");

        let qv = crate::search::embed::try_encode_query(&target_content);
        let mut hits = store.search_hybrid(&query, args.k + 1, qv.as_deref(), None)?;
        hits.retain(|h| h.path != path_str && h.path != rel_str && h.path != stored_path);
        hits.truncate(args.k);

        let entries: Vec<Value> = hits
            .iter()
            .map(|h| serde_json::to_value(h).unwrap_or_default())
            .collect();

        Ok(serde_json::json!({
            "schema_version": JSON_SCHEMA_FIND_RELATED,
            "source": format!("{}:{}", canonical.display(), line),
            "hits": entries,
        }))
    }
}

impl Tools {
    /// Find import cycles via Tarjan SCC. Returns all cycle groups (>= 2 members).
    pub fn cycles(&self, args: CyclesArgs) -> anyhow::Result<Value> {
        use crate::cli::compat::deps::find_workspace_root;
        use crate::cli::compat::load_or_build_dep_graph;
        use crate::core::schema::JSON_SCHEMA_CYCLES;
        use std::path::Path;

        let path_str = args.path.unwrap_or_else(|| ".".to_string());
        let workspace = Path::new(&path_str).to_path_buf();
        let workspace = workspace
            .canonicalize()
            .unwrap_or_else(|_| workspace.clone());
        let workspace_root = find_workspace_root(&workspace).unwrap_or(workspace);

        let g = load_or_build_dep_graph(&workspace_root)?;

        let cycles = crate::deps::scc::detect(&g, 2);
        let entries: Vec<Vec<String>> = cycles
            .into_iter()
            .map(|c| c.members.iter().map(|p| p.display().to_string()).collect())
            .collect();

        Ok(serde_json::json!({
            "schema_version": JSON_SCHEMA_CYCLES,
            "cycles": entries,
        }))
    }
}

