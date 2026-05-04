use repolayer::deps::graph::{DepEdge, DepGraph, ImportKind};
use std::path::PathBuf;

#[test]
fn dep_graph_forward_and_reverse() {
    let mut g = DepGraph::empty(PathBuf::from("/root"));
    g.forward.insert(
        PathBuf::from("/root/a.rs"),
        vec![DepEdge {
            target: PathBuf::from("/root/b.rs"),
            kind: ImportKind::Use,
            line: 1,
            local_name: None,
            raw_path: Some("b".into()),
        }],
    );
    // verify forward retrieval
    assert_eq!(g.forward.get(&PathBuf::from("/root/a.rs")).unwrap().len(), 1);

    // verify reverse adjacency
    let rev = g.reverse_adjacency();
    let callers = rev.get(&PathBuf::from("/root/b.rs")).unwrap();
    assert_eq!(callers.len(), 1);
    assert_eq!(callers[0], PathBuf::from("/root/a.rs"));
}

#[test]
fn dep_error_compiles() {
    // DepError has no `Other` variant; use `BadFormat` instead.
    let _e: repolayer::deps::DepError = repolayer::deps::DepError::BadFormat("test".into());
}
