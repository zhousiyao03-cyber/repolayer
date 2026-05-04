use repolayer::core::declaration::{Declaration, DeclarationKind, ParseResult};
use std::path::PathBuf;

#[test]
fn declaration_kind_serializes_to_canonical_string() {
    let json = serde_json::to_string(&DeclarationKind::Class).unwrap();
    assert_eq!(json, "\"class\"");
    let json = serde_json::to_string(&DeclarationKind::EnumMember).unwrap();
    assert_eq!(json, "\"enum_member\"");
    let json = serde_json::to_string(&DeclarationKind::Constructor).unwrap();
    assert_eq!(json, "\"ctor\"");
}

#[test]
fn declaration_default_has_namespace_kind() {
    let d = Declaration::default();
    assert!(matches!(d.kind, DeclarationKind::Namespace));
    assert_eq!(d.name, "");
    assert!(d.children.is_empty());
}

#[test]
fn parse_result_is_constructible() {
    let r = ParseResult {
        path: PathBuf::from("/tmp/foo.rs"),
        language: "rust",
        source: b"".to_vec(),
        line_count: 0,
        error_count: 0,
        declarations: vec![],
    };
    assert_eq!(r.language, "rust");
}
