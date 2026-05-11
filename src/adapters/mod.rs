//! Source-language adapters built on `ast-grep-core`.
//!
//! Each adapter implements [`base::LanguageAdapter`] for one language
//! family. IDL parsers under [`idl`] use bare tree-sitter and emit a
//! different output shape (services/methods rather than Declarations);
//! they are dispatched separately by the indexer.

pub mod base;
pub mod csharp;
pub mod go;
pub mod idl;
pub mod java;
pub mod kotlin;
pub mod markdown;
pub mod python;
pub mod rust;
pub mod scala;
pub mod typescript;

use crate::core::declaration::ParseResult;
use crate::core::populate_markers;
use ast_grep_core::Language;
use ast_grep_language::{LanguageExt, SupportLang};
use base::LanguageAdapter;
use std::path::{Path, PathBuf};

/// Parse a single file, returning a `ParseResult` if the extension is
/// supported by any adapter. Returns `None` for unknown extensions.
///
/// IDL files (`.proto`, `.thrift`) are NOT dispatched here — the indexer
/// handles them via `crate::adapters::idl` directly because they emit a
/// different output type (services/methods rather than Declarations).
pub fn parse_file(path: &Path) -> Option<ParseResult> {
    let source = std::fs::read_to_string(path).ok()?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    // Markdown has its own grammar (tree-sitter-md), not ast-grep-language.
    if matches!(ext, "md" | "markdown" | "mdx" | "mdown") {
        let mut r = markdown::parse_markdown(path, source.as_bytes());
        populate_markers(&mut r.declarations, r.language);
        return Some(r);
    }

    let lang = SupportLang::from_path(path)?;
    let doc = lang.ast_grep(source.clone());
    let root = doc.root();
    let mut result = match lang {
        SupportLang::Rust => rust::RustAdapter.parse(path, source.as_bytes(), root),
        SupportLang::Python => python::PythonAdapter.parse(path, source.as_bytes(), root),
        SupportLang::TypeScript | SupportLang::Tsx | SupportLang::JavaScript => {
            typescript::TypeScriptAdapter.parse(path, source.as_bytes(), root)
        }
        SupportLang::CSharp => csharp::CSharpAdapter.parse(path, source.as_bytes(), root),
        SupportLang::Go => go::GoAdapter.parse(path, source.as_bytes(), root),
        SupportLang::Java => java::JavaAdapter.parse(path, source.as_bytes(), root),
        SupportLang::Kotlin => kotlin::KotlinAdapter.parse(path, source.as_bytes(), root),
        SupportLang::Scala => scala::ScalaAdapter.parse(path, source.as_bytes(), root),
        _ => return None,
    };

    // Central marker enrichment so adapters stay focused on tree walking.
    populate_markers(&mut result.declarations, result.language);
    Some(result)
}

/// Walk a set of paths in parallel (respecting `.gitignore` / `.ast-outline-ignore`),
/// parse each supported source file, and return all results sorted by path.
///
/// This mirrors the `walk_and_parse` helper from aeroxy/ast-outline and is used
/// by the surface/ language resolvers (Scala, fallback) that need to scan an
/// entire directory tree.
pub fn walk_and_parse(paths: &[PathBuf], glob_str: Option<&str>) -> Vec<ParseResult> {
    use ignore::WalkBuilder;

    let (tx, rx) = std::sync::mpsc::channel();

    if paths.is_empty() {
        return Vec::new();
    }

    // Filter out paths that don't exist.
    let existing: Vec<PathBuf> = paths.iter().filter(|p| p.exists()).cloned().collect();
    if existing.is_empty() {
        return Vec::new();
    }

    let mut builder = WalkBuilder::new(&existing[0]);
    for p in existing.iter().skip(1) {
        builder.add(p);
    }
    builder.hidden(false);

    if let Some(g) = glob_str {
        if let Ok(override_builder) = ignore::overrides::OverrideBuilder::new("").add(g) {
            if let Ok(over) = override_builder.build() {
                builder.overrides(over);
            }
        }
    }

    let walker = builder.build_parallel();
    walker.run(|| {
        let tx = tx.clone();
        Box::new(move |result| {
            if let Ok(entry) = result {
                if entry.file_type().is_some_and(|ft| ft.is_file()) {
                    if let Some(parsed) = parse_file(entry.path()) {
                        let _ = tx.send(parsed);
                    }
                }
            }
            ignore::WalkState::Continue
        })
    });

    drop(tx);
    let mut results: Vec<_> = rx.into_iter().collect();
    results.sort_by(|a, b| a.path.cmp(&b.path));
    results
}
