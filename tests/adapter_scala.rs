use ast_grep_language::{LanguageExt, SupportLang};
use repolayer::adapters::base::LanguageAdapter;
use repolayer::adapters::scala::ScalaAdapter;
use repolayer::core::declaration::{Declaration, DeclarationKind};
use std::path::Path;

fn parse_scala(src: &str) -> repolayer::core::declaration::ParseResult {
    let lang = SupportLang::Scala;
    let root = lang.ast_grep(src);
    ScalaAdapter.parse(Path::new("test.scala"), src.as_bytes(), root.root())
}

fn find_named<'a>(decls: &'a [Declaration], name: &str) -> Option<&'a Declaration> {
    for d in decls {
        if d.name == name { return Some(d); }
        if let Some(x) = find_named(&d.children, name) { return Some(x); }
    }
    None
}

#[test]
fn parses_class() {
    let r = parse_scala("class User(id: Int) {\n  def name(): String = \"\"\n}\n");
    let u = find_named(&r.declarations, "User").expect("User");
    assert!(matches!(u.kind, DeclarationKind::Class));
    let methods: Vec<_> = u.children.iter().map(|c| c.name.clone()).collect();
    assert!(methods.contains(&"name".to_string()), "User children: {:?}", methods);
}

#[test]
fn parses_trait() {
    let r = parse_scala("trait Greeter { def greet: String }\n");
    let g = find_named(&r.declarations, "Greeter").expect("Greeter");
    assert!(matches!(g.kind, DeclarationKind::Interface));
}
