use repolayer::graph::model::*;
use repolayer::graph::store::Store;
use tempfile::tempdir;

#[test]
fn open_creates_schema() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("index.db");
    let store = Store::open(&db).unwrap();
    drop(store);
    let store = Store::open(&db).unwrap();
    assert_eq!(store.count_nodes().unwrap(), 0);
}

#[test]
fn insert_and_get_node_roundtrips() {
    let dir = tempdir().unwrap();
    let store = Store::open(&dir.path().join("index.db")).unwrap();
    let node = Node::new(NodeKind::Symbol, "repo_a", "src/auth.ts", Some("login"));
    store.upsert_node(&node).unwrap();
    let fetched = store.get_node(&node.id).unwrap().unwrap();
    assert_eq!(fetched.symbol.as_deref(), Some("login"));
    assert_eq!(fetched.repo, "repo_a");
}

#[test]
fn upsert_is_idempotent() {
    let dir = tempdir().unwrap();
    let store = Store::open(&dir.path().join("index.db")).unwrap();
    let node = Node::new(NodeKind::Module, "r", "src/lib.ts", None);
    store.upsert_node(&node).unwrap();
    store.upsert_node(&node).unwrap();
    assert_eq!(store.count_nodes().unwrap(), 1);
}

#[test]
fn insert_edge_and_query() {
    let dir = tempdir().unwrap();
    let store = Store::open(&dir.path().join("index.db")).unwrap();
    let a = Node::new(NodeKind::Symbol, "r", "a.ts", Some("foo"));
    let b = Node::new(NodeKind::Symbol, "r", "b.ts", Some("bar"));
    store.upsert_node(&a).unwrap();
    store.upsert_node(&b).unwrap();
    let e = Edge { from: a.id.clone(), to: b.id.clone(), kind: EdgeKind::Calls };
    store.upsert_edge(&e).unwrap();
    let outgoing = store.outgoing_edges(&a.id, EdgeKind::Calls).unwrap();
    assert_eq!(outgoing.len(), 1);
    assert_eq!(outgoing[0].to, b.id);
}
