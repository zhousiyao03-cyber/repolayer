use repolayer::adapters::parse_file;
use repolayer::core::declaration::DeclarationKind;
use std::io::Write;
use tempfile::NamedTempFile;

fn write_temp(suffix: &str, content: &str) -> NamedTempFile {
    let mut f = tempfile::Builder::new().suffix(suffix).tempfile().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f
}

#[test]
fn dispatches_python() {
    let f = write_temp(".py", "def hello():\n    pass\n");
    let r = parse_file(f.path()).expect("parsed");
    assert_eq!(r.language, "python");
    assert!(r.declarations.iter().any(|d| d.name == "hello"));
}

#[test]
fn dispatches_typescript() {
    let f = write_temp(".ts", "export function add(a: number) { return a; }\n");
    let r = parse_file(f.path()).expect("parsed");
    assert_eq!(r.language, "typescript");
    assert!(r.declarations.iter().any(|d| d.name == "add"));
}

#[test]
fn dispatches_rust() {
    let f = write_temp(".rs", "pub fn add() {}\n");
    let r = parse_file(f.path()).expect("parsed");
    assert_eq!(r.language, "rust");
    // For rust, top-level fn might not be wrapped in a namespace
    let names: Vec<_> = collect_names(&r.declarations);
    assert!(names.contains(&"add".to_string()), "names: {:?}", names);
}

#[test]
fn dispatches_markdown() {
    let f = write_temp(".md", "# Title\n\n## Section\n");
    let r = parse_file(f.path()).expect("parsed");
    assert_eq!(r.language, "markdown");
    assert!(r.declarations.iter().any(|d| matches!(d.kind, DeclarationKind::Heading)));
}

#[test]
fn returns_none_for_unknown_extension() {
    let f = write_temp(".xyz", "...");
    assert!(parse_file(f.path()).is_none());
}

#[test]
fn populates_markers_after_dispatch() {
    let f = write_temp(".rs", "trait T {}\n");
    let r = parse_file(f.path()).expect("parsed");
    let t = collect_first_named(&r.declarations, "T").expect("T trait");
    assert!(matches!(t.kind, DeclarationKind::Interface));
    assert_eq!(t.native_kind.as_deref(), Some("trait"));
}

fn collect_names(decls: &[repolayer::core::declaration::Declaration]) -> Vec<String> {
    let mut out = Vec::new();
    for d in decls {
        out.push(d.name.clone());
        out.extend(collect_names(&d.children));
    }
    out
}

fn collect_first_named<'a>(
    decls: &'a [repolayer::core::declaration::Declaration],
    name: &str,
) -> Option<&'a repolayer::core::declaration::Declaration> {
    for d in decls {
        if d.name == name {
            return Some(d);
        }
        if let Some(x) = collect_first_named(&d.children, name) {
            return Some(x);
        }
    }
    None
}
