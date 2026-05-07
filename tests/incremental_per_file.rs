//! Validates that `repolayer update` updates deps.db and search.db at file
//! granularity — i.e. files that didn't change keep their original chunk ids
//! and forward edges, only the changed file is touched.

use assert_cmd::Command;
use std::fs;
use std::process::Command as Std;
use tempfile::tempdir;

fn git_init_commit(repo: &std::path::Path) {
    Std::new("git").current_dir(repo).args(["init", "-q"]).status().unwrap();
    Std::new("git").current_dir(repo).args(["add", "."]).status().unwrap();
    Std::new("git")
        .current_dir(repo)
        .args(["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qm", "init"])
        .status()
        .unwrap();
}

#[test]
fn update_keeps_chunk_ids_for_unchanged_files() {
    let workspace = tempdir().unwrap();
    let repo = workspace.path().join("repo");
    fs::create_dir_all(repo.join("src")).unwrap();
    // Two source files. We'll edit `a.ts` and assert `b.ts`'s chunk ids are stable.
    fs::write(
        repo.join("src/a.ts"),
        "export function alpha() { return 1; }\n\
         export function alphaTwo() { return 2; }\n",
    )
    .unwrap();
    fs::write(
        repo.join("src/b.ts"),
        "export function beta() { return 2; }\n\
         export function betaTwo() { return 3; }\n\
         export function betaThree() { return 4; }\n",
    )
    .unwrap();
    fs::write(repo.join("package.json"), r#"{"name":"r"}"#).unwrap();
    git_init_commit(&repo);

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

    let search_db = workspace.path().join(".repolayer/search.db");
    let b_ids_before: Vec<i64> = {
        let conn = rusqlite::Connection::open(&search_db).unwrap();
        let mut stmt = conn
            .prepare("SELECT id FROM chunks WHERE path LIKE '%/src/b.ts' ORDER BY id")
            .unwrap();
        let rows = stmt.query_map([], |r| r.get::<_, i64>(0)).unwrap();
        rows.collect::<Result<Vec<_>, _>>().unwrap()
    };
    assert!(
        !b_ids_before.is_empty(),
        "b.ts should chunk to ≥1 row after build"
    );

    // Edit a.ts only.
    fs::write(
        repo.join("src/a.ts"),
        "export function alpha() { return 1; }\n\
         export function alphaTwo() { return 11; }\n\
         export function alphaThree() { return 22; }\n",
    )
    .unwrap();

    Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(workspace.path())
        .arg("update")
        .assert()
        .success();

    let conn = rusqlite::Connection::open(&search_db).unwrap();

    // b.ts chunk ids must be unchanged — we touched a.ts only.
    let b_ids_after: Vec<i64> = {
        let mut stmt = conn
            .prepare("SELECT id FROM chunks WHERE path LIKE '%/src/b.ts' ORDER BY id")
            .unwrap();
        let rows = stmt.query_map([], |r| r.get::<_, i64>(0)).unwrap();
        rows.collect::<Result<Vec<_>, _>>().unwrap()
    };
    assert_eq!(
        b_ids_before, b_ids_after,
        "unchanged file b.ts should keep its chunk ids across `update`"
    );

    // a.ts content must reflect the new function name.
    let a_content: String = conn
        .query_row(
            "SELECT GROUP_CONCAT(content, '||') FROM chunks WHERE path LIKE '%/src/a.ts'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        a_content.contains("alphaTwo"),
        "a.ts chunks should contain the new alphaTwo function, got: {a_content}"
    );
}

#[test]
fn update_clears_dangling_chunk_vec_for_deleted_file() {
    // Regression: search/store.rs::delete_file used to leave chunk_vec rows
    // behind. After deletion + `update`, the vec0 table must shrink in lock-
    // step with the chunks table.
    let workspace = tempdir().unwrap();
    let repo = workspace.path().join("repo");
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src/a.ts"), "export function alpha() {}\n").unwrap();
    fs::write(repo.join("src/b.ts"), "export function beta() {}\n").unwrap();
    fs::write(repo.join("package.json"), r#"{"name":"r"}"#).unwrap();
    git_init_commit(&repo);

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

    // Delete b.ts on disk.
    fs::remove_file(repo.join("src/b.ts")).unwrap();

    Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(workspace.path())
        .arg("update")
        .assert()
        .success();

    // Open via SearchStore so vec0 is loaded and chunk_vec is queryable.
    let store =
        repolayer::search::store::SearchStore::open(&workspace.path().join(".repolayer/search.db"))
            .unwrap();
    let conn = store.conn();
    let b_chunks: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM chunks WHERE path LIKE '%/src/b.ts'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(b_chunks, 0, "deleted file b.ts must have no chunks left");

    // chunk_vec rowid must equal chunks.id for any live row.
    let dangling: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM chunk_vec v
             LEFT JOIN chunks c ON c.id = v.rowid
             WHERE c.id IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(dangling, 0, "no chunk_vec row should outlive its chunk");
}

#[test]
fn update_refreshes_deps_for_changed_file_only() {
    // Two TS files where a.ts imports b.ts. Editing a.ts to drop the import
    // must remove that forward_edges row, while b.ts's outgoing edges (none
    // here, but the forward_edges table mustn't gain spurious rows either)
    // stay untouched.
    let workspace = tempdir().unwrap();
    let repo = workspace.path().join("repo");
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src/a.ts"), "import { beta } from './b';\nexport const x = beta();\n")
        .unwrap();
    fs::write(repo.join("src/b.ts"), "export const beta = () => 1;\n").unwrap();
    fs::write(repo.join("package.json"), r#"{"name":"r"}"#).unwrap();
    git_init_commit(&repo);

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

    let deps_db = workspace.path().join(".repolayer/deps.db");
    let edges_before: i64 = {
        let conn = rusqlite::Connection::open(&deps_db).unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM forward_edges WHERE from_path LIKE '%a.ts'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert!(edges_before >= 1, "a.ts should have ≥1 outgoing edge before edit");

    // Drop the import in a.ts.
    fs::write(repo.join("src/a.ts"), "export const x = 42;\n").unwrap();

    Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(workspace.path())
        .arg("update")
        .assert()
        .success();

    let conn = rusqlite::Connection::open(&deps_db).unwrap();
    let edges_after: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM forward_edges WHERE from_path LIKE '%a.ts'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        edges_after, 0,
        "a.ts had its only import removed; forward_edges row should be gone"
    );
}
