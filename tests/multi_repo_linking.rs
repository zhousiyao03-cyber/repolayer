use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

#[test]
fn cross_repo_import_creates_edge() {
    let workspace = tempdir().unwrap();
    let src = std::path::Path::new("tests/fixtures/multi_repo");
    copy_dir_all(src, workspace.path()).unwrap();

    fs::write(
        workspace.path().join("repolayer.yml"),
        r#"
repos:
  - path: ./repo_a
  - path: ./repo_b
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
    let cross_edges: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM edges e
             JOIN nodes nf ON e.from_id = nf.id
             JOIN nodes nt ON e.to_id = nt.id
             WHERE e.kind='imports' AND nf.repo != nt.repo",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        cross_edges >= 1,
        "expected ≥1 cross-repo import edge, got {}",
        cross_edges
    );
}

#[test]
fn package_json_with_subpath_import_resolves() {
    let workspace = tempdir().unwrap();
    let repo_a = workspace.path().join("repo_a");
    let repo_b = workspace.path().join("repo_b");
    fs::create_dir_all(repo_a.join("src/sub")).unwrap();
    fs::create_dir_all(repo_b.join("src")).unwrap();
    fs::write(
        repo_a.join("package.json"),
        r#"{"name":"@org/lib","version":"1","main":"src/index.ts"}"#,
    )
    .unwrap();
    fs::write(repo_a.join("src/index.ts"), "export const x = 1;").unwrap();
    fs::write(
        repo_b.join("package.json"),
        r#"{"name":"@org/app","version":"1","main":"src/index.ts"}"#,
    )
    .unwrap();
    // Import a sub-path: @org/lib/sub/foo (subpath imports often used in real codebases)
    fs::write(
        repo_b.join("src/index.ts"),
        r#"import { y } from "@org/lib/sub/foo";"#,
    )
    .unwrap();

    fs::write(
        workspace.path().join("repolayer.yml"),
        r#"
repos:
  - path: ./repo_a
  - path: ./repo_b
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
    let cross_edges: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM edges e
             JOIN nodes nf ON e.from_id = nf.id
             JOIN nodes nt ON e.to_id = nt.id
             WHERE e.kind='imports' AND nf.repo='repo_b' AND nt.repo='repo_a'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        cross_edges >= 1,
        "subpath import @org/lib/sub/foo should match @org/lib package, got {} edges",
        cross_edges
    );
}

#[test]
fn cross_repo_target_with_no_main_field_does_not_orphan() {
    let workspace = tempdir().unwrap();
    let repo_a = workspace.path().join("repo_a");
    let repo_b = workspace.path().join("repo_b");
    fs::create_dir_all(repo_a.join("src")).unwrap();
    fs::create_dir_all(repo_b.join("src")).unwrap();
    // repo_a's package.json has NO `main` field
    fs::write(
        repo_a.join("package.json"),
        r#"{"name":"@org/no-main","version":"1"}"#,
    )
    .unwrap();
    fs::write(repo_a.join("src/index.ts"), "export const x = 1;").unwrap();
    fs::write(
        repo_b.join("package.json"),
        r#"{"name":"@org/app","version":"1","main":"src/index.ts"}"#,
    )
    .unwrap();
    fs::write(
        repo_b.join("src/index.ts"),
        r#"import { x } from "@org/no-main";"#,
    )
    .unwrap();

    fs::write(
        workspace.path().join("repolayer.yml"),
        r#"
repos:
  - path: ./repo_a
  - path: ./repo_b
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
    // every module node should have an incoming Contains edge from a repo
    let orphans: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM nodes n
             WHERE n.kind = 'module'
               AND NOT EXISTS (
                 SELECT 1 FROM edges e
                 JOIN nodes nf ON e.from_id = nf.id
                 WHERE e.to_id = n.id AND e.kind = 'contains' AND nf.kind = 'repo'
               )",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        orphans, 0,
        "no module should be orphan even when target package has no `main` field"
    );

    // and the cross-repo Imports edge should still exist
    let cross: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM edges e
             JOIN nodes nf ON e.from_id = nf.id
             JOIN nodes nt ON e.to_id = nt.id
             WHERE e.kind='imports' AND nf.repo='repo_b' AND nt.repo='repo_a'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(cross >= 1, "cross-repo Imports edge must still exist");
}

fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}
