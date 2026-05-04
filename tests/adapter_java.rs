use ast_grep_language::{LanguageExt, SupportLang};
use repolayer::adapters::base::LanguageAdapter;
use repolayer::adapters::java::JavaAdapter;
use repolayer::core::declaration::{Declaration, DeclarationKind};
use std::path::Path;

fn parse_java(src: &str) -> repolayer::core::declaration::ParseResult {
    let lang = SupportLang::Java;
    let root = lang.ast_grep(src);
    JavaAdapter.parse(Path::new("Test.java"), src.as_bytes(), root.root())
}

fn find_named<'a>(decls: &'a [Declaration], name: &str) -> Option<&'a Declaration> {
    for d in decls {
        if d.name == name { return Some(d); }
        if let Some(x) = find_named(&d.children, name) { return Some(x); }
    }
    None
}

#[test]
fn parses_class_with_method() {
    let src = "public class User {\n  public String greet() { return \"\"; }\n}\n";
    let r = parse_java(src);
    let user = find_named(&r.declarations, "User").expect("User");
    assert!(matches!(user.kind, DeclarationKind::Class));
    let methods: Vec<_> = user.children.iter().map(|c| c.name.clone()).collect();
    assert!(methods.contains(&"greet".to_string()), "User children: {:?}", methods);
}

#[test]
fn parses_interface() {
    let r = parse_java("public interface Greeter { String greet(); }\n");
    let g = find_named(&r.declarations, "Greeter").expect("Greeter");
    assert!(matches!(g.kind, DeclarationKind::Interface));
}
