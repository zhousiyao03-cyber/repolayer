use ast_grep_language::{LanguageExt, SupportLang};
use repolayer::adapters::base::LanguageAdapter;
use repolayer::adapters::swift::SwiftAdapter;
use repolayer::core::declaration::{Declaration, DeclarationKind};
use std::path::Path;

fn parse_swift(src: &str) -> repolayer::core::declaration::ParseResult {
    let root = SupportLang::Swift.ast_grep(src);
    SwiftAdapter.parse(Path::new("test.swift"), src.as_bytes(), root.root())
}

fn find<'a>(decls: &'a [Declaration], name: &str) -> Option<&'a Declaration> {
    for d in decls {
        if d.name == name {
            return Some(d);
        }
        if let Some(x) = find(&d.children, name) {
            return Some(x);
        }
    }
    None
}

const FIXTURE: &str = r#"import Foundation

class Foo: NSObject {
    var name: String = ""
    func greet(_ msg: String) -> String { return msg }
}

struct Point {
    let x: Int
    let y: Int
}

protocol Drawable {
    func draw()
}

enum Color {
    case red, green
}

func topLevel() {}
"#;

#[test]
fn parses_class_with_members() {
    let r = parse_swift(FIXTURE);
    let foo = find(&r.declarations, "Foo").expect("Foo class");
    assert!(matches!(foo.kind, DeclarationKind::Class));
    assert_eq!(foo.bases, vec!["NSObject".to_string()]);

    let name = find(&foo.children, "name").expect("name property");
    assert!(matches!(name.kind, DeclarationKind::Property));

    let greet = find(&foo.children, "greet").expect("greet method");
    assert!(matches!(greet.kind, DeclarationKind::Method));
}

#[test]
fn parses_struct() {
    let r = parse_swift(FIXTURE);
    let point = find(&r.declarations, "Point").expect("Point struct");
    assert!(matches!(point.kind, DeclarationKind::Struct));
    assert!(find(&point.children, "x").is_some(), "x field");
    assert!(find(&point.children, "y").is_some(), "y field");
}

#[test]
fn parses_protocol_as_interface() {
    let r = parse_swift(FIXTURE);
    let drawable = find(&r.declarations, "Drawable").expect("Drawable protocol");
    assert!(matches!(drawable.kind, DeclarationKind::Interface));
    assert!(find(&drawable.children, "draw").is_some(), "draw method");
}

#[test]
fn parses_enum_with_cases() {
    let r = parse_swift(FIXTURE);
    let color = find(&r.declarations, "Color").expect("Color enum");
    assert!(matches!(color.kind, DeclarationKind::Enum));
    assert!(find(&color.children, "red").is_some(), "red case");
    assert!(find(&color.children, "green").is_some(), "green case");
}

#[test]
fn parses_top_level_function() {
    let r = parse_swift(FIXTURE);
    let f = find(&r.declarations, "topLevel").expect("topLevel func");
    assert!(matches!(f.kind, DeclarationKind::Function));
}

#[test]
fn parses_fixture_file() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/swift/sample.swift"
    );
    let src = std::fs::read_to_string(path).expect("fixture");
    let r = parse_swift(&src);
    assert_eq!(r.language, "swift");
    assert!(find(&r.declarations, "Foo").is_some());
    assert!(find(&r.declarations, "Point").is_some());
}
