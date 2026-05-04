use repolayer::graph::model::*;
use repolayer::graph::store::Store;
use repolayer::query::{callers, dependencies, list_repos, symbol};
use tempfile::tempdir;

#[test]
fn get_symbol_returns_definition_and_callers() {
    let dir = tempdir().unwrap();
    let store = Store::open(&dir.path().join("index.db")).unwrap();

    let a = Node::new(NodeKind::Function, "r", "a.ts", Some("foo"));
    let b = Node::new(NodeKind::Function, "r", "b.ts", Some("bar"));
    store.upsert_node(&a).unwrap();
    store.upsert_node(&b).unwrap();
    store
        .upsert_edge(&Edge {
            from: b.id.clone(),
            to: a.id.clone(),
            kind: EdgeKind::Calls,
            confidence: 1.0,
        })
        .unwrap();

    let result = symbol::get_symbol(&store, "foo", None).unwrap().unwrap();
    assert_eq!(result.symbol.symbol.as_deref(), Some("foo"));
    assert_eq!(result.callers.len(), 1);
    assert_eq!(result.callers[0].symbol.as_deref(), Some("bar"));
}

#[test]
fn get_symbol_returns_none_for_unknown() {
    let dir = tempdir().unwrap();
    let store = Store::open(&dir.path().join("index.db")).unwrap();
    let result = symbol::get_symbol(&store, "nonexistent", None).unwrap();
    assert!(result.is_none());
}

#[test]
fn get_callers_walks_depth_2() {
    let dir = tempdir().unwrap();
    let store = Store::open(&dir.path().join("index.db")).unwrap();
    // build chain: c → b → a
    let a = Node::new(NodeKind::Function, "r", "a.ts", Some("a"));
    let b = Node::new(NodeKind::Function, "r", "b.ts", Some("b"));
    let c = Node::new(NodeKind::Function, "r", "c.ts", Some("c"));
    for n in [&a, &b, &c] {
        store.upsert_node(n).unwrap();
    }
    store
        .upsert_edge(&Edge {
            from: b.id.clone(),
            to: a.id.clone(),
            kind: EdgeKind::Calls,
            confidence: 1.0,
        })
        .unwrap();
    store
        .upsert_edge(&Edge {
            from: c.id.clone(),
            to: b.id.clone(),
            kind: EdgeKind::Calls,
            confidence: 1.0,
        })
        .unwrap();

    let chain = callers::get_callers(&store, "a", 2).unwrap();
    let names: Vec<_> = chain.iter().filter_map(|n| n.symbol.clone()).collect();
    assert!(names.contains(&"b".to_string()));
    assert!(names.contains(&"c".to_string()));
}

#[test]
fn get_callers_respects_depth() {
    let dir = tempdir().unwrap();
    let store = Store::open(&dir.path().join("index.db")).unwrap();
    let a = Node::new(NodeKind::Function, "r", "a.ts", Some("a"));
    let b = Node::new(NodeKind::Function, "r", "b.ts", Some("b"));
    let c = Node::new(NodeKind::Function, "r", "c.ts", Some("c"));
    for n in [&a, &b, &c] {
        store.upsert_node(n).unwrap();
    }
    store
        .upsert_edge(&Edge {
            from: b.id.clone(),
            to: a.id.clone(),
            kind: EdgeKind::Calls,
            confidence: 1.0,
        })
        .unwrap();
    store
        .upsert_edge(&Edge {
            from: c.id.clone(),
            to: b.id.clone(),
            kind: EdgeKind::Calls,
            confidence: 1.0,
        })
        .unwrap();
    let chain = callers::get_callers(&store, "a", 1).unwrap();
    let names: Vec<_> = chain.iter().filter_map(|n| n.symbol.clone()).collect();
    assert!(names.contains(&"b".to_string()));
    assert!(
        !names.contains(&"c".to_string()),
        "depth 1 should not reach c"
    );
}

#[test]
fn get_dependencies_walks_imports() {
    let dir = tempdir().unwrap();
    let store = Store::open(&dir.path().join("index.db")).unwrap();
    let a = Node::new(NodeKind::Module, "r", "a.ts", None);
    let b = Node::new(NodeKind::Module, "r", "b.ts", None);
    let c = Node::new(NodeKind::Module, "r", "c.ts", None);
    for n in [&a, &b, &c] {
        store.upsert_node(n).unwrap();
    }
    // a imports b imports c
    store
        .upsert_edge(&Edge {
            from: a.id.clone(),
            to: b.id.clone(),
            kind: EdgeKind::Imports,
            confidence: 1.0,
        })
        .unwrap();
    store
        .upsert_edge(&Edge {
            from: b.id.clone(),
            to: c.id.clone(),
            kind: EdgeKind::Imports,
            confidence: 1.0,
        })
        .unwrap();

    let deps = dependencies::get_dependencies(&store, "a.ts", 2).unwrap();
    let paths: Vec<_> = deps.iter().map(|n| n.path.as_str()).collect();
    assert!(paths.contains(&"b.ts"));
    assert!(paths.contains(&"c.ts"));
}

#[test]
fn list_repos_returns_distinct() {
    let dir = tempdir().unwrap();
    let store = Store::open(&dir.path().join("index.db")).unwrap();
    store
        .upsert_node(&Node::new(NodeKind::Repo, "r1", "", None))
        .unwrap();
    store
        .upsert_node(&Node::new(NodeKind::Repo, "r2", "", None))
        .unwrap();
    let repos = list_repos::list_repos(&store).unwrap();
    assert_eq!(repos.len(), 2);
    let names: Vec<_> = repos.iter().map(|n| n.repo.as_str()).collect();
    assert!(names.contains(&"r1"));
    assert!(names.contains(&"r2"));
}
