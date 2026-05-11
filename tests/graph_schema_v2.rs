use repolayer::graph::model::{Edge, EdgeKind, Node, NodeKind};
use repolayer::graph::store::Store;
use tempfile::tempdir;

#[test]
fn store_writes_schema_version_2() {
    let dir = tempdir().unwrap();
    let s = Store::open(&dir.path().join("index.db")).unwrap();
    assert_eq!(s.schema_version().unwrap(), 2);
}

#[test]
fn node_kind_method_persists() {
    let dir = tempdir().unwrap();
    let s = Store::open(&dir.path().join("index.db")).unwrap();
    let n = Node::new(NodeKind::Method, "repo1", "src/foo.rs", Some("Foo.bar"));
    s.upsert_node(&n).unwrap();
    let got = s.get_node(&n.id).unwrap().expect("node");
    assert!(matches!(got.kind, NodeKind::Method));
    assert_eq!(got.symbol.as_deref(), Some("Foo.bar"));
}

#[test]
fn edge_extends_persists_with_default_confidence() {
    let dir = tempdir().unwrap();
    let s = Store::open(&dir.path().join("index.db")).unwrap();
    let a = Node::new(NodeKind::Type, "r", "p", Some("A"));
    let b = Node::new(NodeKind::Type, "r", "p", Some("B"));
    s.upsert_node(&a).unwrap();
    s.upsert_node(&b).unwrap();
    let e = Edge {
        from: a.id.clone(),
        to: b.id.clone(),
        kind: EdgeKind::Extends,
        confidence: 1.0,
    };
    s.upsert_edge(&e).unwrap();
    let got = s.get_edges_from(&a.id).unwrap();
    assert_eq!(got.len(), 1);
    assert!(matches!(got[0].kind, EdgeKind::Extends));
    assert!((got[0].confidence - 1.0).abs() < 0.001);
}

#[test]
fn idl_service_idl_method_kinds_persist() {
    let dir = tempdir().unwrap();
    let s = Store::open(&dir.path().join("index.db")).unwrap();
    let svc = Node::new(NodeKind::IdlService, "idl", "user.proto", Some("UserSvc"));
    let m = Node::new(
        NodeKind::IdlMethod,
        "idl",
        "user.proto",
        Some("UserSvc.GetUser"),
    );
    s.upsert_node(&svc).unwrap();
    s.upsert_node(&m).unwrap();
    assert!(matches!(
        s.get_node(&svc.id).unwrap().unwrap().kind,
        NodeKind::IdlService
    ));
    assert!(matches!(
        s.get_node(&m.id).unwrap().unwrap().kind,
        NodeKind::IdlMethod
    ));
}

#[test]
fn confidence_below_one_persists() {
    let dir = tempdir().unwrap();
    let s = Store::open(&dir.path().join("index.db")).unwrap();
    let a = Node::new(NodeKind::Module, "r", "a", None);
    let b = Node::new(NodeKind::IdlMethod, "idl", "x", Some("Svc.Foo"));
    s.upsert_node(&a).unwrap();
    s.upsert_node(&b).unwrap();
    s.upsert_edge(&Edge {
        from: a.id.clone(),
        to: b.id.clone(),
        kind: EdgeKind::Invokes,
        confidence: 0.5,
    })
    .unwrap();
    let got = s.get_edges_from(&a.id).unwrap();
    assert!((got[0].confidence - 0.5).abs() < 0.001);
}
