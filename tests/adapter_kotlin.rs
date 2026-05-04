use ast_grep_language::{LanguageExt, SupportLang};
use repolayer::adapters::base::LanguageAdapter;
use repolayer::adapters::kotlin::KotlinAdapter;
use repolayer::core::declaration::{Declaration, DeclarationKind};
use std::path::Path;

fn parse_kt(src: &str) -> repolayer::core::declaration::ParseResult {
    let lang = SupportLang::Kotlin;
    let root = lang.ast_grep(src);
    KotlinAdapter.parse(Path::new("test.kt"), src.as_bytes(), root.root())
}

fn find_named<'a>(decls: &'a [Declaration], name: &str) -> Option<&'a Declaration> {
    for d in decls {
        if d.name == name {
            return Some(d);
        }
        if let Some(x) = find_named(&d.children, name) {
            return Some(x);
        }
    }
    None
}

#[test]
fn parses_class_with_method() {
    let r = parse_kt("class User(val id: Int) {\n  fun name(): String = \"\"\n}\n");
    let user = find_named(&r.declarations, "User").expect("User");
    assert!(matches!(user.kind, DeclarationKind::Class));
    let methods: Vec<_> = user.children.iter().map(|c| c.name.clone()).collect();
    assert!(methods.contains(&"name".to_string()), "User children: {:?}", methods);
}

#[test]
fn parses_interface() {
    let r = parse_kt("interface Repo {\n  fun findById(id: Int): String\n}\n");
    let iface = find_named(&r.declarations, "Repo").expect("Repo");
    assert!(matches!(iface.kind, DeclarationKind::Interface));
}
