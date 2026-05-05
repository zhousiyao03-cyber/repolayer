/// Integration test: `repolayer build` must write all 4 SQLite stores under
/// `.repolayer/` and populate them with data from a TypeScript fixture repo.
use assert_cmd::Command;
use rusqlite::Connection;
use std::fs;
use tempfile::tempdir;

fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst_path)?;
        } else {
            fs::copy(entry.path(), &dst_path)?;
        }
    }
    Ok(())
}

#[test]
fn build_writes_all_4_sqlite_stores() {
    let workspace = tempdir().unwrap();
    let src_repo = std::path::Path::new("tests/fixtures/single_repo_ts");
    let dst_repo = workspace.path().join("repo");
    copy_dir_all(src_repo, &dst_repo).unwrap();

    fs::write(
        workspace.path().join("repolayer.yml"),
        format!("repos:\n  - path: {}\n", dst_repo.display()),
    )
    .unwrap();

    Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(workspace.path())
        .arg("build")
        .assert()
        .success();

    let dir = workspace.path().join(".repolayer");
    assert!(dir.join("index.db").exists(), "index.db missing");
    assert!(dir.join("outline.db").exists(), "outline.db missing");
    assert!(dir.join("deps.db").exists(), "deps.db missing");
    assert!(dir.join("search.db").exists(), "search.db missing");

    // Sanity: index.db has node rows (at least 1 repo + 1 module + 1 symbol)
    let conn = Connection::open(dir.join("index.db")).unwrap();
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))
        .unwrap();
    assert!(n >= 4, "expected ≥4 nodes in index.db, got {}", n);

    // Sanity: index.db has type/function nodes from adapters::parse_file
    let sym_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM nodes WHERE kind IN ('type','method','function')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        sym_count >= 1,
        "expected ≥1 symbol node in index.db, got {}",
        sym_count
    );

    // Sanity: outline.db has at least one row (Declaration tree stored)
    let conn2 = Connection::open(dir.join("outline.db")).unwrap();
    let m: i64 = conn2
        .query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))
        .unwrap();
    assert!(m >= 1, "expected ≥1 outline file row, got {}", m);

    // Sanity: deps.db schema exists (forward_edges table present)
    let conn3 = Connection::open(dir.join("deps.db")).unwrap();
    let _deps_count: i64 = conn3
        .query_row("SELECT COUNT(*) FROM forward_edges", [], |r| r.get(0))
        .unwrap();
    // No assertion on count — TS fixture may have no resolvable intra-repo imports
    // depending on file layout. The important thing is the table exists.

    // Sanity: search.db has chunks (chunker ran on source files).
    // Use SearchStore::open so the sqlite-vec extension is registered on
    // this connection — bare Connection::open won't see chunk_vec.
    let store4 = repolayer::search::store::SearchStore::open(&dir.join("search.db")).unwrap();
    let chunks: i64 = store4
        .conn()
        .query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))
        .unwrap();
    assert!(chunks >= 1, "expected ≥1 chunk in search.db, got {}", chunks);

    // chunk_vec virtual table must exist
    let vec_table_count: i64 = store4
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'chunk_vec'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        vec_table_count, 1,
        "chunk_vec virtual table missing"
    );

    // Default behaviour: REPOLAYER_DOWNLOAD not set → no embeddings written.
    assert_eq!(
        store4.embedding_count().unwrap(),
        0,
        "expected 0 embeddings without REPOLAYER_DOWNLOAD=1"
    );
}

/// When the user has explicitly opted out via REPOLAYER_NO_DOWNLOAD AND has no
/// cache, the build must succeed without trying to fetch the model.
#[test]
fn build_with_no_download_succeeds() {
    let workspace = tempdir().unwrap();
    let src_repo = std::path::Path::new("tests/fixtures/single_repo_ts");
    let dst_repo = workspace.path().join("repo");
    copy_dir_all(src_repo, &dst_repo).unwrap();
    fs::write(
        workspace.path().join("repolayer.yml"),
        format!("repos:\n  - path: {}\n", dst_repo.display()),
    )
    .unwrap();

    // Point AST_OUTLINE_MODEL_DIR somewhere known-empty so we can be sure
    // there's no pre-existing cache satisfying the download check.
    let empty_cache = workspace.path().join("empty-cache");
    fs::create_dir_all(&empty_cache).unwrap();

    Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(workspace.path())
        .env("REPOLAYER_NO_DOWNLOAD", "1")
        .env("AST_OUTLINE_MODEL_DIR", &empty_cache)
        .arg("build")
        .assert()
        .success();

    // Embeddings must still be empty.
    let store =
        repolayer::search::store::SearchStore::open(&workspace.path().join(".repolayer/search.db"))
            .unwrap();
    assert_eq!(store.embedding_count().unwrap(), 0);
}
