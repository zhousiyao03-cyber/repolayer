use repolayer::deps::build_for_repo;
use std::fs;
use tempfile::tempdir;

#[test]
fn builds_dep_graph_on_simple_typescript_workspace() {
    let d = tempdir().unwrap();
    fs::create_dir_all(d.path().join("src")).unwrap();
    fs::write(d.path().join("src/foo.ts"), "export const x = 1;\n").unwrap();
    fs::write(
        d.path().join("src/bar.ts"),
        "import { x } from './foo';\nexport const y = x + 1;\n",
    )
    .unwrap();
    fs::write(d.path().join("package.json"), r#"{"name":"test"}"#).unwrap();

    let g = build_for_repo(d.path()).expect("build_for_repo");
    // bar.ts should have a forward edge to foo.ts
    let bar = d.path().join("src/bar.ts");
    let edges = g.forward.get(&bar).expect("bar has edges");
    assert!(
        edges
            .iter()
            .any(|e| e.target.file_name().map(|f| f.to_string_lossy().contains("foo")).unwrap_or(false)),
        "expected edge to foo.ts, got: {:?}",
        edges.iter().map(|e| e.target.clone()).collect::<Vec<_>>()
    );
}

#[test]
fn graph_has_forward_entries_for_all_source_files() {
    let d = tempdir().unwrap();
    fs::create_dir_all(d.path().join("src")).unwrap();
    fs::write(d.path().join("src/a.ts"), "export const a = 1;\n").unwrap();
    fs::write(d.path().join("src/b.ts"), "export const b = 2;\n").unwrap();
    fs::write(d.path().join("package.json"), r#"{"name":"test"}"#).unwrap();

    let g = build_for_repo(d.path()).expect("build_for_repo");
    // Both files should appear as keys in forward (even with no edges).
    let a = d.path().join("src/a.ts");
    let b = d.path().join("src/b.ts");
    assert!(g.forward.contains_key(&a), "a.ts missing from forward map");
    assert!(g.forward.contains_key(&b), "b.ts missing from forward map");
}
