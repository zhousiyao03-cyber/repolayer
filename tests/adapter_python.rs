use ast_grep_language::{LanguageExt, SupportLang};
use repolayer::adapters::base::LanguageAdapter;
use repolayer::adapters::python::PythonAdapter;
use repolayer::core::declaration::DeclarationKind;
use std::path::Path;

fn parse(src: &str) -> repolayer::core::declaration::ParseResult {
    let lang = SupportLang::Python;
    let root_doc = lang.ast_grep(src);
    PythonAdapter.parse(Path::new("test.py"), src.as_bytes(), root_doc.root())
}

#[test]
fn parses_top_level_function() {
    let r = parse("def hello(name):\n    return name\n");
    let names: Vec<_> = r.declarations.iter().map(|d| d.name.clone()).collect();
    assert!(names.contains(&"hello".to_string()), "decls: {:?}", names);
    let f = r.declarations.iter().find(|d| d.name == "hello").unwrap();
    assert!(matches!(f.kind, DeclarationKind::Function));
}

#[test]
fn parses_class_with_method() {
    let src = "class User:\n    def __init__(self):\n        pass\n    def name(self):\n        pass\n";
    let r = parse(src);
    let user = r.declarations.iter().find(|d| d.name == "User").expect("User class");
    assert!(matches!(user.kind, DeclarationKind::Class));
    let method_names: Vec<_> = user.children.iter().map(|c| c.name.clone()).collect();
    assert!(method_names.contains(&"__init__".to_string()));
    assert!(method_names.contains(&"name".to_string()));
}

#[test]
fn parses_inheritance_bases() {
    let r = parse("class Admin(User, Auditable):\n    pass\n");
    let admin = r.declarations.iter().find(|d| d.name == "Admin").unwrap();
    assert_eq!(admin.bases, vec!["User", "Auditable"]);
}
