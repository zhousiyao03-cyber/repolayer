use std::fs;
use tempfile::tempdir;

#[path = "common/mod.rs"]
mod common;
use common::repolayer_cmd;

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

    repolayer_cmd()
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

    repolayer_cmd()
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

/// Unit test for the ast-grep call-pattern helper.
///
/// Verifies that a method name appearing only inside a string literal does NOT
/// produce a match, while an actual call expression DOES.
#[test]
fn ast_grep_filters_false_positive_in_string_literal() {
    use ast_grep_language::SupportLang;
    use repolayer::linker::idl_links::has_call_to_method;

    // TypeScript: method name only in a string — should NOT match.
    let ts_literal_only = r#"const msg = "GetUser fetched successfully";"#;
    assert!(
        !has_call_to_method(ts_literal_only, SupportLang::TypeScript, "GetUser"),
        "string literal should not trigger a call match"
    );

    // TypeScript: actual method call — SHOULD match.
    let ts_call = r#"const result = client.GetUser(userId);"#;
    assert!(
        has_call_to_method(ts_call, SupportLang::TypeScript, "GetUser"),
        "member call expression should match"
    );

    // TypeScript: bare function call — SHOULD match.
    let ts_bare_call = r#"GetUser(userId);"#;
    assert!(
        has_call_to_method(ts_bare_call, SupportLang::TypeScript, "GetUser"),
        "bare function call should match"
    );

    // Go: actual call expression — SHOULD match.
    let go_call = r#"package main
func main() { GetBenefit(42) }"#;
    assert!(
        has_call_to_method(go_call, SupportLang::Go, "GetBenefit"),
        "Go call expression should match"
    );

    // Go: name only in a comment — should NOT match (no call expression node).
    let go_comment = r#"package main
// GetBenefit is called elsewhere
func main() {}"#;
    assert!(
        !has_call_to_method(go_comment, SupportLang::Go, "GetBenefit"),
        "Go comment should not trigger a call match"
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
