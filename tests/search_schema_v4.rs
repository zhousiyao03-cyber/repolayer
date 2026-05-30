//! Schema-v4 migration tests for SearchStore.

use repolayer::search::store::SearchStore;
use rusqlite::Connection;
use tempfile::tempdir;

#[test]
fn fresh_open_with_dim_1024_creates_v4() {
    let dir = tempdir().unwrap();
    let p = dir.path().join("search.db");
    let s = SearchStore::open_with_dim(&p, 1024).unwrap();
    assert_eq!(s.schema_version().unwrap(), 4);
    assert_eq!(s.embedding_dim().unwrap(), 1024);
}

#[test]
fn reopen_with_different_dim_recreates_chunk_vec() {
    let dir = tempdir().unwrap();
    let p = dir.path().join("search.db");
    {
        let s = SearchStore::open_with_dim(&p, 256).unwrap();
        assert_eq!(s.embedding_dim().unwrap(), 256);
    }
    {
        let s = SearchStore::open_with_dim(&p, 1024).unwrap();
        assert_eq!(s.embedding_dim().unwrap(), 1024);
        // chunks table preserved; chunk_vec dropped+recreated.
        let conn = Connection::open(&p).unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name='chunk_vec'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "chunk_vec should exist");
    }
}

#[test]
fn upsert_embedding_rejects_wrong_dim() {
    let dir = tempdir().unwrap();
    let p = dir.path().join("search.db");
    let s = SearchStore::open_with_dim(&p, 1024).unwrap();
    let bad = vec![0.0f32; 256];
    let err = s.upsert_embedding(1, &bad).unwrap_err();
    assert!(err.to_string().contains("1024"), "{err}");
}
