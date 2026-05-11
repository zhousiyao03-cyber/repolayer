use repolayer::deps::graph::{DepEdge, DepGraph, ImportKind};
use repolayer::deps::store::DepStore;
use tempfile::tempdir;

fn make_graph(root: &std::path::Path) -> DepGraph {
    let mut g = DepGraph::empty(root.to_path_buf());
    g.forward.insert(
        root.join("a.rs"),
        vec![DepEdge {
            target: root.join("b.rs"),
            kind: ImportKind::Use,
            line: 1,
            local_name: None,
            raw_path: Some("b".into()),
        }],
    );
    g
}

#[test]
fn open_writes_schema_version_1() {
    let dir = tempdir().unwrap();
    let s = DepStore::open(&dir.path().join("deps.db")).unwrap();
    assert_eq!(s.schema_version().unwrap(), 1);
}

#[test]
fn round_trip_repo_graph() {
    let dir = tempdir().unwrap();
    let s = DepStore::open(&dir.path().join("deps.db")).unwrap();
    let workspace_root = tempdir().unwrap();
    let g = make_graph(workspace_root.path());
    s.replace_repo_graph("repo1", &g).unwrap();
    let loaded = s
        .load_repo_graph("repo1", workspace_root.path().to_path_buf())
        .unwrap();
    assert_eq!(loaded.forward.len(), 1);
    let key = workspace_root.path().join("a.rs");
    assert_eq!(loaded.forward.get(&key).map(|v| v.len()).unwrap_or(0), 1);
}

#[test]
fn replace_clears_old_edges() {
    let dir = tempdir().unwrap();
    let s = DepStore::open(&dir.path().join("deps.db")).unwrap();
    let ws = tempdir().unwrap();
    s.replace_repo_graph("repo1", &make_graph(ws.path()))
        .unwrap();
    let g2 = DepGraph::empty(ws.path().to_path_buf()); // empty
    s.replace_repo_graph("repo1", &g2).unwrap();
    let loaded = s.load_repo_graph("repo1", ws.path().to_path_buf()).unwrap();
    assert!(loaded.forward.is_empty() || loaded.forward.values().all(|v| v.is_empty()));
}

#[test]
fn multi_repo_isolation() {
    let dir = tempdir().unwrap();
    let s = DepStore::open(&dir.path().join("deps.db")).unwrap();
    let ws = tempdir().unwrap();
    s.replace_repo_graph("repo1", &make_graph(ws.path()))
        .unwrap();
    s.replace_repo_graph("repo2", &DepGraph::empty(ws.path().to_path_buf()))
        .unwrap();
    let r1 = s.load_repo_graph("repo1", ws.path().to_path_buf()).unwrap();
    let r2 = s.load_repo_graph("repo2", ws.path().to_path_buf()).unwrap();
    assert_eq!(r1.forward.len(), 1);
    assert!(r2.forward.is_empty() || r2.forward.values().all(|v| v.is_empty()));
}
