use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

#[test]
fn manual_link_creates_repo_to_repo_edge() {
    let workspace = tempdir().unwrap();
    fs::create_dir_all(workspace.path().join("repo_x/src")).unwrap();
    fs::create_dir_all(workspace.path().join("repo_y/src")).unwrap();
    fs::write(
        workspace.path().join("repo_x/src/a.ts"),
        "export function a() {}",
    )
    .unwrap();
    fs::write(
        workspace.path().join("repo_y/src/b.ts"),
        "export function b() {}",
    )
    .unwrap();
    fs::write(
        workspace.path().join("repo_x/package.json"),
        r#"{"name":"x"}"#,
    )
    .unwrap();
    fs::write(
        workspace.path().join("repo_y/package.json"),
        r#"{"name":"y"}"#,
    )
    .unwrap();

    fs::write(
        workspace.path().join("repolayer.yml"),
        r#"
repos:
  - path: ./repo_x
  - path: ./repo_y
links:
  - from: repo_x
    to: repo_y
    kind: http
"#,
    )
    .unwrap();

    Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(workspace.path())
        .arg("build")
        .assert()
        .success();

    let conn = rusqlite::Connection::open(workspace.path().join(".repolayer/index.db")).unwrap();
    let manual_edges: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM edges e
             JOIN nodes nf ON e.from_id=nf.id JOIN nodes nt ON e.to_id=nt.id
             WHERE nf.kind='repo' AND nt.kind='repo' AND nf.repo='repo_x' AND nt.repo='repo_y'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        manual_edges, 1,
        "expected exactly 1 manual repo->repo edge, got {}",
        manual_edges
    );
}
