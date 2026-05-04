use repolayer::graph::model::{Edge, EdgeKind, Node, NodeKind};
use repolayer::graph::store::Store;
use repolayer::query::find_idl_impl::{find_idl_impl, FindIdlImplArgs};
use tempfile::tempdir;

fn setup_store() -> (tempfile::TempDir, Store) {
    let dir = tempdir().unwrap();
    let store = Store::open(&dir.path().join("index.db")).unwrap();
    (dir, store)
}

#[test]
fn finds_implementations_and_invocations() {
    let (_d, s) = setup_store();

    // Create IDL method node
    let idl_method = Node::new(
        NodeKind::IdlMethod,
        "idl",
        "user.proto",
        Some("UserService.GetUser"),
    );
    s.upsert_node(&idl_method).unwrap();

    // Server module that implements
    let server = Node::new(NodeKind::Module, "backend", "src/services/user.rs", None);
    s.upsert_node(&server).unwrap();
    s.upsert_edge(&Edge {
        from: server.id.clone(),
        to: idl_method.id.clone(),
        kind: EdgeKind::Implements,
        confidence: 0.7,
    })
    .unwrap();

    // Client module that invokes
    let client = Node::new(NodeKind::Module, "bff", "src/clients/user.rs", None);
    s.upsert_node(&client).unwrap();
    s.upsert_edge(&Edge {
        from: client.id.clone(),
        to: idl_method.id.clone(),
        kind: EdgeKind::Invokes,
        confidence: 0.4,
    })
    .unwrap();

    let result = find_idl_impl(
        &s,
        &FindIdlImplArgs {
            method: "GetUser".into(),
            service: None,
            include_invokes: true,
            include_implements: true,
        },
    )
    .unwrap();

    assert!(result.method.is_some());
    let method = result.method.unwrap();
    assert_eq!(method.repo, "idl");
    assert_eq!(method.path, "user.proto");
    assert_eq!(method.symbol, "UserService.GetUser");

    assert_eq!(result.implements.len(), 1);
    assert_eq!(result.invokes.len(), 1);
    assert_eq!(result.implements[0].repo, "backend");
    assert_eq!(result.invokes[0].repo, "bff");
    assert!((result.implements[0].confidence - 0.7).abs() < 0.001);
    assert!((result.invokes[0].confidence - 0.4).abs() < 0.001);
}

#[test]
fn excludes_kinds_when_disabled() {
    let (_d, s) = setup_store();

    let idl_method = Node::new(
        NodeKind::IdlMethod,
        "idl",
        "user.proto",
        Some("UserService.GetUser"),
    );
    s.upsert_node(&idl_method).unwrap();

    let client = Node::new(NodeKind::Module, "bff", "client.rs", None);
    s.upsert_node(&client).unwrap();
    s.upsert_edge(&Edge {
        from: client.id.clone(),
        to: idl_method.id.clone(),
        kind: EdgeKind::Invokes,
        confidence: 1.0,
    })
    .unwrap();

    let result = find_idl_impl(
        &s,
        &FindIdlImplArgs {
            method: "GetUser".into(),
            service: None,
            include_invokes: false,
            include_implements: true,
        },
    )
    .unwrap();

    // invokes disabled → empty
    assert_eq!(result.invokes.len(), 0);
    // no implements edges exist
    assert_eq!(result.implements.len(), 0);
}

#[test]
fn returns_no_method_when_not_found() {
    let (_d, s) = setup_store();

    let result = find_idl_impl(
        &s,
        &FindIdlImplArgs {
            method: "NonExistent".into(),
            ..Default::default()
        },
    )
    .unwrap();

    assert!(result.method.is_none());
    assert_eq!(result.implements.len(), 0);
    assert_eq!(result.invokes.len(), 0);
}

#[test]
fn service_filter_narrows_results() {
    let (_d, s) = setup_store();

    // Two methods with the same local name in different services
    let method_a = Node::new(
        NodeKind::IdlMethod,
        "idl",
        "a.proto",
        Some("ServiceA.DoThing"),
    );
    let method_b = Node::new(
        NodeKind::IdlMethod,
        "idl",
        "b.proto",
        Some("ServiceB.DoThing"),
    );
    s.upsert_node(&method_a).unwrap();
    s.upsert_node(&method_b).unwrap();

    let result = find_idl_impl(
        &s,
        &FindIdlImplArgs {
            method: "DoThing".into(),
            service: Some("ServiceA".into()),
            include_invokes: true,
            include_implements: true,
        },
    )
    .unwrap();

    // Only ServiceA.DoThing should be returned
    let m = result.method.unwrap();
    assert_eq!(m.symbol, "ServiceA.DoThing");
}

#[test]
fn sorted_by_confidence_descending() {
    let (_d, s) = setup_store();

    let idl_method = Node::new(
        NodeKind::IdlMethod,
        "idl",
        "svc.proto",
        Some("Svc.Method"),
    );
    s.upsert_node(&idl_method).unwrap();

    // Three implementors with different confidences
    for (repo, conf) in [("low", 0.2f32), ("high", 0.9), ("mid", 0.5)] {
        let n = Node::new(NodeKind::Module, repo, "impl.rs", None);
        s.upsert_node(&n).unwrap();
        s.upsert_edge(&Edge {
            from: n.id.clone(),
            to: idl_method.id.clone(),
            kind: EdgeKind::Implements,
            confidence: conf,
        })
        .unwrap();
    }

    let result = find_idl_impl(
        &s,
        &FindIdlImplArgs {
            method: "Method".into(),
            service: None,
            include_invokes: false,
            include_implements: true,
        },
    )
    .unwrap();

    assert_eq!(result.implements.len(), 3);
    // Should be high(0.9) > mid(0.5) > low(0.2)
    assert!((result.implements[0].confidence - 0.9).abs() < 0.001);
    assert!((result.implements[1].confidence - 0.5).abs() < 0.001);
    assert!((result.implements[2].confidence - 0.2).abs() < 0.001);
}

#[test]
fn schema_version_present() {
    let (_d, s) = setup_store();

    let result = find_idl_impl(
        &s,
        &FindIdlImplArgs {
            method: "Any".into(),
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(result.schema_version, "repolayer.find_idl_impl.v1");
}
