use ast_grep_language::{LanguageExt, SupportLang};
use repolayer::adapters::base::LanguageAdapter;
use repolayer::adapters::go::GoAdapter;
use repolayer::core::declaration::DeclarationKind;
use std::path::Path;

fn parse_go(src: &str) -> repolayer::core::declaration::ParseResult {
    let lang = SupportLang::Go;
    let root = lang.ast_grep(src);
    GoAdapter.parse(Path::new("test.go"), src.as_bytes(), root.root())
}

// Go adapter wraps everything under a package Namespace declaration.
// Helper to get children of the package namespace.
fn package_children(r: &repolayer::core::declaration::ParseResult) -> &Vec<repolayer::core::declaration::Declaration> {
    let ns = r.declarations.iter().find(|d| matches!(d.kind, DeclarationKind::Namespace))
        .expect("package namespace");
    &ns.children
}

#[test]
fn parses_exported_func() {
    let r = parse_go("package main\n\nfunc Add(a, b int) int { return a + b }\n");
    let children = package_children(&r);
    let add = children.iter().find(|d| d.name == "Add").expect("Add func");
    assert!(matches!(add.kind, DeclarationKind::Function));
}

#[test]
fn parses_struct_with_methods() {
    let src = "package main\n\ntype User struct { ID int }\n\nfunc (u *User) Name() string { return \"\" }\n";
    let r = parse_go(src);
    let children = package_children(&r);
    let user = children.iter().find(|d| d.name == "User").expect("User struct");
    assert!(matches!(user.kind, DeclarationKind::Struct));
    let method_names: Vec<_> = user.children.iter().map(|c| c.name.clone()).collect();
    assert!(method_names.contains(&"Name".to_string()), "User children: {:?}", method_names);
}
