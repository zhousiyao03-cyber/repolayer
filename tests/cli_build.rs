use predicates::str::contains;
use std::fs;
use tempfile::tempdir;

#[path = "common/mod.rs"]
mod common;
use common::repolayer_cmd;

#[test]
fn build_creates_db_with_nodes_and_edges() {
    let workspace = tempdir().unwrap();
    let repo_src = std::path::Path::new("tests/fixtures/single_repo_ts");
    let repo_dst = workspace.path().join("single_repo_ts");
    copy_dir_all(repo_src, &repo_dst).unwrap();

    let cfg_path = workspace.path().join("repolayer.yml");
    fs::write(
        &cfg_path,
        format!("repos:\n  - path: {}\n", repo_dst.display()),
    )
    .unwrap();

    let mut cmd = repolayer_cmd();
    cmd.current_dir(workspace.path())
        .arg("build")
        .assert()
        .success()
        .stdout(contains("indexed"));

    let db = workspace.path().join(".repolayer/index.db");
    assert!(db.exists(), "DB must be created at {}", db.display());

    let conn = rusqlite::Connection::open(&db).unwrap();
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM nodes WHERE kind IN ('type','method','function')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(n >= 3, "expected ≥3 symbol nodes, got {}", n);
    let m: i64 = conn
        .query_row("SELECT COUNT(*) FROM edges WHERE kind='imports'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(m >= 1, "expected ≥1 import edge, got {}", m);
}

fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}

#[test]
fn build_counts_contains_edges_in_stats() {
    let workspace = tempdir().unwrap();
    let repo_src = std::path::Path::new("tests/fixtures/single_repo_ts");
    let repo_dst = workspace.path().join("single_repo_ts");
    copy_dir_all(repo_src, &repo_dst).unwrap();

    fs::write(
        workspace.path().join("repolayer.yml"),
        format!("repos:\n  - path: {}\n", repo_dst.display()),
    )
    .unwrap();

    let output = repolayer_cmd()
        .current_dir(workspace.path())
        .arg("build")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse "indexed N nodes, M edges" — find the line that starts with "indexed"
    // (tracing logs may also appear in stdout, so we cannot use nth(3) on the full output)
    let indexed_line = stdout
        .lines()
        .find(|l| l.trim_start().starts_with("indexed"))
        .expect("could not find 'indexed ...' line in stdout");
    let edges_count = indexed_line
        .split_whitespace()
        .nth(3)
        .and_then(|s| s.parse::<u64>().ok())
        .expect("could not parse edges count from 'indexed' line");

    let conn = rusqlite::Connection::open(workspace.path().join(".repolayer/index.db")).unwrap();
    let total_edges: i64 = conn
        .query_row("SELECT COUNT(*) FROM edges", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        edges_count as i64, total_edges,
        "stats.edges should match DB edge count (Contains was the regression)"
    );
    assert!(
        total_edges > 1,
        "expected multiple edges (Contains + Imports)"
    );
}

#[test]
fn build_does_not_leave_orphan_module_nodes() {
    let workspace = tempdir().unwrap();
    let repo_src = std::path::Path::new("tests/fixtures/single_repo_ts");
    let repo_dst = workspace.path().join("single_repo_ts");
    copy_dir_all(repo_src, &repo_dst).unwrap();

    fs::write(
        workspace.path().join("repolayer.yml"),
        format!("repos:\n  - path: {}\n", repo_dst.display()),
    )
    .unwrap();

    repolayer_cmd()
        .current_dir(workspace.path())
        .arg("build")
        .assert()
        .success();

    let conn = rusqlite::Connection::open(workspace.path().join(".repolayer/index.db")).unwrap();
    // Every module node must have an incoming Contains edge from a repo
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
    assert_eq!(orphans, 0, "found {} orphan module nodes", orphans);
}
