use ast_grep_language::{LanguageExt, SupportLang};
use repolayer::adapters::base::LanguageAdapter;
use repolayer::adapters::rust::RustAdapter;
use repolayer::core::declaration::{Declaration, DeclarationKind};
use std::path::Path;

fn parse_rust(src: &str) -> repolayer::core::declaration::ParseResult {
    let lang = SupportLang::Rust;
    let root = lang.ast_grep(src);
    RustAdapter.parse(Path::new("test.rs"), src.as_bytes(), root.root())
}

/// Find first declaration anywhere in the tree (top-level or nested) by name.
fn find_named<'a>(decls: &'a [Declaration], name: &str) -> Option<&'a Declaration> {
    for d in decls {
        if d.name == name { return Some(d); }
        if let Some(x) = find_named(&d.children, name) { return Some(x); }
    }
    None
}

#[test]
fn parses_struct() {
    let r = parse_rust("pub struct User { pub id: u64, name: String }\n");
    let user = find_named(&r.declarations, "User").expect("User");
    assert!(matches!(user.kind, DeclarationKind::Struct));
}

#[test]
fn parses_trait_with_methods() {
    let src = "pub trait Greeter {\n    fn greet(&self) -> String;\n}\n";
    let r = parse_rust(src);
    let g = find_named(&r.declarations, "Greeter").expect("Greeter");
    // aeroxy maps Rust `trait` to canonical Interface kind.
    assert!(matches!(g.kind, DeclarationKind::Interface), "got {:?}", g.kind);
    let methods: Vec<_> = g.children.iter().map(|c| c.name.clone()).collect();
    assert!(methods.contains(&"greet".to_string()), "Greeter children: {:?}", methods);
}

#[test]
fn parses_impl_block_groups_methods_under_struct() {
    let src = "pub struct U;\n\nimpl U {\n    pub fn new() -> Self { Self }\n    pub fn name(&self) -> String { String::new() }\n}\n";
    let r = parse_rust(src);
    let u = find_named(&r.declarations, "U").expect("U");
    let methods: Vec<_> = u.children.iter().map(|c| c.name.clone()).collect();
    assert!(methods.contains(&"new".to_string()), "U children: {:?}", methods);
    assert!(methods.contains(&"name".to_string()), "U children: {:?}", methods);
}
