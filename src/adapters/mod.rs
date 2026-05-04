//! Source-language adapters built on `ast-grep-core`.
//!
//! Each adapter implements [`base::LanguageAdapter`] for one language
//! family. IDL parsers under [`idl`] use bare tree-sitter and emit a
//! different output shape (services/methods rather than Declarations);
//! they are dispatched separately by the indexer.

pub mod base;
pub mod python;
pub mod typescript;
pub mod go;
pub mod rust;
pub mod csharp;
pub mod java;
pub mod kotlin;
pub mod scala;
pub mod markdown;
pub mod idl;

use std::path::Path;
use ast_grep_core::Language;
use ast_grep_language::{LanguageExt, SupportLang};
use crate::core::declaration::ParseResult;
use crate::core::populate_markers;
use base::LanguageAdapter;

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
