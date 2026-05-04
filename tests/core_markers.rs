use repolayer::core::declaration::{Declaration, DeclarationKind};
use repolayer::core::markers::populate_markers;

#[test]
fn populates_async_modifier_from_rust_signature() {
    let mut decls = vec![Declaration {
        kind: DeclarationKind::Method,
        name: "fetch".into(),
        signature: "async fn fetch(&self) -> Result<()>".into(),
        ..Default::default()
    }];
    populate_markers(&mut decls, "rust");
    assert!(
        decls[0].modifiers.iter().any(|m| m == "async"),
        "modifiers: {:?}",
        decls[0].modifiers
    );
}

#[test]
fn detects_deprecated_attribute_rust() {
    let mut decls = vec![Declaration {
        kind: DeclarationKind::Function,
        name: "old".into(),
        signature: "fn old()".into(),
        attrs: vec!["#[deprecated]".into()],
        ..Default::default()
    }];
    populate_markers(&mut decls, "rust");
    assert!(decls[0].deprecated);
}

#[test]
fn rust_trait_native_kind() {
    let mut decls = vec![Declaration {
        kind: DeclarationKind::Interface,
        name: "Greeter".into(),
        signature: "trait Greeter".into(),
        ..Default::default()
    }];
    populate_markers(&mut decls, "rust");
    assert_eq!(decls[0].native_kind.as_deref(), Some("trait"));
}

#[test]
fn populates_markers_recursively_into_children() {
    let mut decls = vec![Declaration {
        kind: DeclarationKind::Class,
        name: "Outer".into(),
        signature: "class Outer".into(),
        children: vec![Declaration {
            kind: DeclarationKind::Method,
            name: "inner".into(),
            signature: "async fn inner()".into(),
            ..Default::default()
        }],
        ..Default::default()
    }];
    populate_markers(&mut decls, "rust");
    assert!(
        decls[0].children[0].modifiers.iter().any(|m| m == "async"),
        "child modifiers: {:?}",
        decls[0].children[0].modifiers
    );
}
