use ast_grep_language::SupportLang;
use repolayer::search::chunker::{self, ChunkerKind};
use std::io::Write;
use std::path::PathBuf;

// ── chunk_file API ────────────────────────────────────────────────────────────

#[test]
fn chunks_a_simple_typescript_file() {
    let mut f = tempfile::Builder::new().suffix(".ts").tempfile().unwrap();
    f.write_all(b"export function foo() { return 1; }\nexport function bar() { return 2; }\n")
        .unwrap();
    let chunks = chunker::chunk_file(f.path(), "src/hello.ts");
    assert!(!chunks.is_empty(), "expected at least one chunk");
    assert_eq!(chunks[0].language, "typescript");
    assert!(
        chunks.iter().any(|c| c.content.contains("foo")),
        "expected 'foo' in some chunk"
    );
}

#[test]
fn chunk_file_returns_empty_for_unsupported_extension() {
    let mut f = tempfile::Builder::new()
        .suffix(".unknown_ext_xyz")
        .tempfile()
        .unwrap();
    f.write_all(b"hello world").unwrap();
    let chunks = chunker::chunk_file(f.path(), "x.unknown_ext_xyz");
    assert!(
        chunks.is_empty(),
        "unsupported extension should yield no chunks"
    );
}

#[test]
fn chunk_file_returns_empty_for_missing_file() {
    let chunks = chunker::chunk_file(
        std::path::Path::new("/tmp/does_not_exist_repolayer_test.rs"),
        "no/file.rs",
    );
    assert!(chunks.is_empty(), "missing file should yield empty vec");
}

// ── chunk_source API ──────────────────────────────────────────────────────────

#[test]
fn rust_single_function_one_chunk() {
    let src = "fn hello() {\n    println!(\"hi\");\n}\n";
    let chunks = chunker::chunk_source(src, "f.rs", ChunkerKind::AstGrep(SupportLang::Rust));
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].start_line, 1);
    assert_eq!(chunks[0].language, "rust");
    assert!(chunks[0].content.contains("fn hello"));
}

#[test]
fn rust_small_decls_pack_into_one_chunk() {
    let src = "fn a() { 1 }\nfn b() { 2 }\nfn c() { 3 }\n";
    let chunks = chunker::chunk_source(src, "f.rs", ChunkerKind::AstGrep(SupportLang::Rust));
    assert_eq!(chunks.len(), 1);
    assert!(chunks[0].content.contains("fn a"));
    assert!(chunks[0].content.contains("fn c"));
}

#[test]
fn rust_splits_large_decls_across_chunks() {
    let big_body: String = "    let x = 0;\n".repeat(80); // ~1200 chars per body
    let src = format!(
        "fn one() {{\n{big_body}}}\nfn two() {{\n{big_body}}}\nfn three() {{\n{big_body}}}\n"
    );
    let chunks = chunker::chunk_source(&src, "f.rs", ChunkerKind::AstGrep(SupportLang::Rust));
    assert!(
        chunks.len() >= 2,
        "expected packer to split large functions, got {}",
        chunks.len()
    );
    for c in &chunks {
        assert!(!c.content.trim().is_empty());
    }
}

#[test]
fn chunks_cover_full_source_without_gaps() {
    let src = "// header\nfn a() {}\n\nfn b() {}\n// trailer\n";
    let chunks = chunker::chunk_source(src, "f.rs", ChunkerKind::AstGrep(SupportLang::Rust));
    let joined: String = chunks.iter().map(|c| c.content.as_str()).collect();
    assert_eq!(
        joined, src,
        "chunks must reconstruct the original source exactly"
    );
}

#[test]
fn empty_source_yields_no_chunks() {
    let chunks = chunker::chunk_source("", "f.rs", ChunkerKind::AstGrep(SupportLang::Rust));
    assert!(chunks.is_empty());
    let chunks2 =
        chunker::chunk_source("   \n\t\n", "f.rs", ChunkerKind::AstGrep(SupportLang::Rust));
    assert!(chunks2.is_empty());
}

// ── is_indexable ──────────────────────────────────────────────────────────────

#[test]
fn is_indexable_accepts_rust_and_python() {
    assert_eq!(
        chunker::is_indexable(&PathBuf::from("a.rs")),
        Some(ChunkerKind::AstGrep(SupportLang::Rust))
    );
    assert_eq!(
        chunker::is_indexable(&PathBuf::from("a.py")),
        Some(ChunkerKind::AstGrep(SupportLang::Python))
    );
}

#[test]
fn is_indexable_accepts_markdown() {
    assert_eq!(
        chunker::is_indexable(&PathBuf::from("README.md")),
        Some(ChunkerKind::Markdown)
    );
    assert_eq!(
        chunker::is_indexable(&PathBuf::from("doc.MDX")),
        Some(ChunkerKind::Markdown)
    );
}

#[test]
fn is_indexable_rejects_unknown_extensions() {
    assert!(chunker::is_indexable(&PathBuf::from("foo.txt")).is_none());
    assert!(chunker::is_indexable(&PathBuf::from("Makefile")).is_none());
    assert!(chunker::is_indexable(&PathBuf::from("foo.bin")).is_none());
}

// ── Chunk struct fields ───────────────────────────────────────────────────────

#[test]
fn chunk_fields_are_populated_correctly() {
    let src = "fn hello() {\n    1\n}\n";
    let chunks = chunker::chunk_source(src, "src/lib.rs", ChunkerKind::AstGrep(SupportLang::Rust));
    assert_eq!(chunks.len(), 1);
    let c = &chunks[0];
    assert_eq!(c.file_path, "src/lib.rs");
    assert_eq!(c.start_line, 1);
    assert_eq!(c.start_byte, 0);
    assert_eq!(c.end_byte as usize, src.len());
}
