use ast_grep_language::{LanguageExt, SupportLang};
use repolayer::adapters::base::LanguageAdapter;
use repolayer::adapters::typescript::TypeScriptAdapter;
use repolayer::core::declaration::DeclarationKind;
use std::path::Path;

fn parse_ts(src: &str) -> repolayer::core::declaration::ParseResult {
    let lang = SupportLang::TypeScript;
    let root = lang.ast_grep(src);
    TypeScriptAdapter.parse(Path::new("test.ts"), src.as_bytes(), root.root())
}

#[test]
fn parses_exported_class_with_methods() {
    let src = "export class User {\n  constructor() {}\n  greet(): string { return 'hi'; }\n}\n";
    let r = parse_ts(src);
    let user = r.declarations.iter().find(|d| d.name == "User").expect("User class");
    assert!(matches!(user.kind, DeclarationKind::Class));
    let method_names: Vec<_> = user.children.iter().map(|c| c.name.clone()).collect();
    assert!(method_names.contains(&"constructor".to_string()));
    assert!(method_names.contains(&"greet".to_string()));
}

#[test]
fn parses_interface() {
    let r = parse_ts("export interface Foo { id: number; name: string; }\n");
    let foo = r.declarations.iter().find(|d| d.name == "Foo").unwrap();
    assert!(matches!(foo.kind, DeclarationKind::Interface));
}

#[test]
fn parses_function_declaration() {
    let r = parse_ts("export function add(a: number, b: number): number { return a + b; }\n");
    let add = r.declarations.iter().find(|d| d.name == "add").unwrap();
    assert!(matches!(add.kind, DeclarationKind::Function));
}
