use ast_grep_language::{LanguageExt, SupportLang};
use repolayer::adapters::base::LanguageAdapter;
use repolayer::adapters::csharp::CSharpAdapter;
use repolayer::core::declaration::{Declaration, DeclarationKind};
use std::path::Path;

fn parse_cs(src: &str) -> repolayer::core::declaration::ParseResult {
    let lang = SupportLang::CSharp;
    let root = lang.ast_grep(src);
    CSharpAdapter.parse(Path::new("test.cs"), src.as_bytes(), root.root())
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
    let src = "namespace App {\n  public class User {\n    public string Name() => \"\";\n  }\n}\n";
    let r = parse_cs(src);
    let user = find_named(&r.declarations, "User").expect("User class");
    assert!(matches!(user.kind, DeclarationKind::Class));
    let methods: Vec<_> = user.children.iter().map(|c| c.name.clone()).collect();
    assert!(methods.contains(&"Name".to_string()), "User children: {:?}", methods);
}

#[test]
fn parses_interface() {
    let src = "namespace App {\n  public interface IGreeter { string Greet(); }\n}\n";
    let r = parse_cs(src);
    let g = find_named(&r.declarations, "IGreeter").expect("IGreeter");
    assert!(matches!(g.kind, DeclarationKind::Interface));
}
