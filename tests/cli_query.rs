use assert_cmd::Command;
use predicates::str::contains;
use std::fs;
use tempfile::tempdir;

#[test]
fn query_finds_symbols_by_substring() {
    let workspace = tempdir().unwrap();
    let repo_src = std::path::Path::new("tests/fixtures/single_repo_ts");
    let repo_dst = workspace.path().join("single_repo_ts");
    copy_dir_all(repo_src, &repo_dst).unwrap();

    fs::write(
        workspace.path().join("repolayer.yml"),
        format!("repos:\n  - path: {}\n", repo_dst.display()),
    )
    .unwrap();

    Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(workspace.path())
        .arg("build")
        .assert()
        .success();

    Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(workspace.path())
        .args(["query", "login"])
        .assert()
        .success()
        .stdout(contains("login"));
}

#[test]
fn query_returns_no_matches_message_when_empty() {
    let workspace = tempdir().unwrap();
    let repo_src = std::path::Path::new("tests/fixtures/single_repo_ts");
    let repo_dst = workspace.path().join("single_repo_ts");
    copy_dir_all(repo_src, &repo_dst).unwrap();

    fs::write(
        workspace.path().join("repolayer.yml"),
        format!("repos:\n  - path: {}\n", repo_dst.display()),
    )
    .unwrap();

    Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(workspace.path())
        .arg("build")
        .assert()
        .success();

    Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(workspace.path())
        .args(["query", "nonexistent_xyzabc"])
        .assert()
        .success()
        .stdout(contains("no matches"));
}

#[test]
fn query_treats_underscore_as_literal_not_wildcard() {
    let workspace = tempdir().unwrap();
    let repo = workspace.path().join("repo");
    fs::create_dir_all(repo.join("src")).unwrap();
    // Two symbols: "get_user" (literal underscore) and "getXuser" (X in middle).
    // A naive LIKE %get_user% would match both because _ is a wildcard.
    fs::write(
        repo.join("src/a.ts"),
        "export function get_user() {}\nexport function getXuser() {}\n",
    )
    .unwrap();

    fs::write(
        workspace.path().join("repolayer.yml"),
        format!("repos:\n  - path: {}\n", repo.display()),
    )
    .unwrap();

    Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(workspace.path())
        .arg("build")
        .assert()
        .success();

    let output = Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(workspace.path())
        .args(["query", "get_user"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("get_user"), "must match literal underscore");
    assert!(
        !stdout.contains("getXuser"),
        "underscore must NOT be treated as a wildcard (was the regression). stdout: {}",
        stdout
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
