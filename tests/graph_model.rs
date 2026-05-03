use repolayer::graph::model::*;

#[test]
fn node_id_is_stable_across_runs() {
    let a = Node::new(
        NodeKind::Symbol,
        "promotion_member",
        "src/auth.ts",
        Some("login"),
    );
    let b = Node::new(
        NodeKind::Symbol,
        "promotion_member",
        "src/auth.ts",
        Some("login"),
    );
    assert_eq!(a.id, b.id);
}

#[test]
fn different_symbols_get_different_ids() {
    let a = Node::new(
        NodeKind::Symbol,
        "promotion_member",
        "src/auth.ts",
        Some("login"),
    );
    let b = Node::new(
        NodeKind::Symbol,
        "promotion_member",
        "src/auth.ts",
        Some("logout"),
    );
    assert_ne!(a.id, b.id);
}

#[test]
fn edge_serializes_with_kind() {
    let e = Edge {
        from: "n1".into(),
        to: "n2".into(),
        kind: EdgeKind::Calls,
    };
    let json = serde_json::to_string(&e).unwrap();
    assert!(json.contains(r#""kind":"calls""#));
}

#[test]
fn different_kinds_get_different_ids() {
    let a = Node::new(NodeKind::Symbol, "r", "src/x.ts", Some("foo"));
    let b = Node::new(NodeKind::Module, "r", "src/x.ts", Some("foo"));
    assert_ne!(a.id, b.id, "kind should be part of the id hash");
}

#[test]
fn idl_service_serializes_as_lowercase_no_underscore() {
    let n = Node::new(NodeKind::IdlService, "r", "x.proto", Some("S"));
    let json = serde_json::to_string(&n).unwrap();
    assert!(
        json.contains(r#""kind":"idlservice""#),
        "IdlService must serialize as 'idlservice' (intentional lowercase, matches SQL contract). got: {}",
        json
    );
}
