use assert_cmd::Command;
use std::fs;
use std::process::Command as Std;
use tempfile::tempdir;

#[test]
fn update_reindexes_only_changed_files() {
    let workspace = tempdir().unwrap();
    let repo = workspace.path().join("repo");
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src/a.ts"), "export function alpha() {}").unwrap();
    fs::write(repo.join("package.json"), r#"{"name":"r"}"#).unwrap();
    Std::new("git")
        .current_dir(&repo)
        .args(["init", "-q"])
        .status()
        .unwrap();
    Std::new("git")
        .current_dir(&repo)
        .args(["add", "."])
        .status()
        .unwrap();
    Std::new("git")
        .current_dir(&repo)
        .args([
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "init",
        ])
        .status()
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

    // modify a file: add a new export
    fs::write(
        repo.join("src/a.ts"),
        "export function alpha() {}\nexport function beta() {}",
    )
    .unwrap();

    Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(workspace.path())
        .arg("update")
        .assert()
        .success();

    let conn = rusqlite::Connection::open(workspace.path().join(".repolayer/index.db")).unwrap();
    let beta: i64 = conn
        .query_row("SELECT COUNT(*) FROM nodes WHERE symbol='beta'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(beta, 1, "expected beta to be added by update");
    // Original alpha should still be there
    let alpha: i64 = conn
        .query_row("SELECT COUNT(*) FROM nodes WHERE symbol='alpha'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(alpha, 1);
}

#[test]
fn update_handles_no_git_repo_gracefully() {
    let workspace = tempdir().unwrap();
    let repo = workspace.path().join("repo");
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src/a.ts"), "export function x() {}").unwrap();
    fs::write(repo.join("package.json"), r#"{"name":"r"}"#).unwrap();
    // NO git init

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

    // update should not panic, just skip the non-git repo
    Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(workspace.path())
        .arg("update")
        .assert()
        .success();
}

#[test]
fn update_removes_deleted_symbols() {
    let workspace = tempdir().unwrap();
    let repo = workspace.path().join("repo");
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(
        repo.join("src/a.ts"),
        "export function alpha() {}\nexport function gamma() {}",
    )
    .unwrap();
    fs::write(repo.join("package.json"), r#"{"name":"r"}"#).unwrap();
    Std::new("git")
        .current_dir(&repo)
        .args(["init", "-q"])
        .status()
        .unwrap();
    Std::new("git")
        .current_dir(&repo)
        .args(["add", "."])
        .status()
        .unwrap();
    Std::new("git")
        .current_dir(&repo)
        .args([
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "init",
        ])
        .status()
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

    // remove gamma
    fs::write(repo.join("src/a.ts"), "export function alpha() {}").unwrap();

    Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(workspace.path())
        .arg("update")
        .assert()
        .success();

    let conn = rusqlite::Connection::open(workspace.path().join(".repolayer/index.db")).unwrap();
    let gamma: i64 = conn
        .query_row("SELECT COUNT(*) FROM nodes WHERE symbol='gamma'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(gamma, 0, "gamma should be deleted by update");
    let alpha: i64 = conn
        .query_row("SELECT COUNT(*) FROM nodes WHERE symbol='alpha'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(alpha, 1, "alpha should remain");
}
