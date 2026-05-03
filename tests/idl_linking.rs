use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

#[test]
fn idl_creates_service_and_method_nodes() {
    let workspace = tempdir().unwrap();
    let idl_dir = workspace.path().join("http_idl");
    fs::create_dir_all(&idl_dir).unwrap();
    fs::copy(
        "tests/fixtures/multi_repo_with_idl/idl/user.proto",
        idl_dir.join("user.proto"),
    )
    .unwrap();

    fs::write(
        workspace.path().join("repolayer.yml"),
        r#"
repos:
  - path: ./http_idl
    type: idl
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
    let svcs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM nodes WHERE kind='idlservice'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let methods: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM nodes WHERE kind='idlmethod'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(svcs, 1, "expected 1 IdlService node, got {}", svcs);
    assert_eq!(methods, 2, "expected 2 IdlMethod nodes, got {}", methods);
}

#[test]
fn idl_link_detects_server_implements_and_client_invokes() {
    let workspace = tempdir().unwrap();
    let src = std::path::Path::new("tests/fixtures/multi_repo_with_idl");
    copy_dir_all(src, workspace.path()).unwrap();

    fs::write(
        workspace.path().join("repolayer.yml"),
        r#"
repos:
  - path: ./idl
    type: idl
  - path: ./server_repo
  - path: ./client_repo
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

    // Server side: at least 1 IMPLEMENTS edge from server_repo
    let implements: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM edges e
             JOIN nodes nf ON e.from_id = nf.id
             WHERE e.kind='implements' AND nf.repo='server_repo'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        implements >= 1,
        "expected ≥1 IMPLEMENTS edge from server_repo, got {}",
        implements
    );

    // Client side: at least 1 INVOKES edge from client_repo
    let invokes: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM edges e
             JOIN nodes nf ON e.from_id = nf.id
             WHERE e.kind='invokes' AND nf.repo='client_repo'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        invokes >= 1,
        "expected ≥1 INVOKES edge from client_repo, got {}",
        invokes
    );
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
