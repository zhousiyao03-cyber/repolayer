use repolayer::search::chunker::Chunk;
use repolayer::search::store::{SearchLane, SearchStore};
use repolayer::search::store_summary::{SummaryChunk, SummaryStore};
use tempfile::tempdir;

#[test]
fn write_and_read_summary_chunk() {
    let dir = tempdir().unwrap();
    let p = dir.path().join("search.db");
    let s = SearchStore::open_with_dim(&p, 1024).unwrap();
    let store = SummaryStore::new(&s);
    let id = store
        .insert(&SummaryChunk {
            repo: "order_service".into(),
            path: "src/foo.go".into(),
            scope: "module".into(),
            text: "Handles the order creation request, validates the idempotency key, and writes a ledger entry.".into(),
        })
        .unwrap();
    let row = store.get_by_id(id).unwrap();
    assert_eq!(row.text, "Handles the order creation request, validates the idempotency key, and writes a ledger entry.");
    assert_eq!(row.scope, "module");
}

#[test]
fn summary_embedding_dim_matches_store_dim() {
    let dir = tempdir().unwrap();
    let p = dir.path().join("search.db");
    let s = SearchStore::open_with_dim(&p, 1024).unwrap();
    let store = SummaryStore::new(&s);
    let id = store
        .insert(&SummaryChunk {
            repo: "x".into(),
            path: "x.go".into(),
            scope: "module".into(),
            text: "...".into(),
        })
        .unwrap();
    let v = vec![0.0f32; 1024];
    store.upsert_embedding(id, &v).unwrap();
    let bad = vec![0.0f32; 256];
    let err = store.upsert_embedding(id, &bad).unwrap_err();
    assert!(err.to_string().contains("1024"));
}

#[test]
fn hybrid_search_includes_summary_lane_hits() {
    // Seed two code chunks (so the chunks table has rows the summary lane
    // can merge against) plus one summary embedding. The summary's
    // (repo, path) matches the second chunk's (repo, path), and its
    // embedding points the same direction as the query vector.
    //
    // We deliberately keep the *code* chunk at x.go's embedding orthogonal
    // to the query — otherwise the regular chunk_vec kNN already surfaces
    // x.go and the summary lane is doing no work. The other.go chunk has
    // the lexical anchor so BM25 contributes; combined with the summary
    // lane's sem_ranked entry for x.go, the search resolves to the Fusion
    // lane and x.go must appear in the hits.
    let dir = tempdir().unwrap();
    let p = dir.path().join("search.db");
    let s = SearchStore::open_with_dim(&p, 4).unwrap();

    let chunks = vec![
        Chunk {
            content: "alpha keyword token".into(),
            file_path: "other.go".into(),
            start_line: 1,
            end_line: 1,
            start_byte: 0,
            end_byte: 19,
            language: "go".into(),
        },
        Chunk {
            content: "lorem ipsum dolor".into(),
            file_path: "x.go".into(),
            start_line: 1,
            end_line: 1,
            start_byte: 0,
            end_byte: 17,
            language: "go".into(),
        },
    ];
    let ids = s.insert_file_chunks("r1", &chunks).unwrap();
    // other.go chunk: orthogonal to query (axis 0 only).
    s.upsert_embedding(ids[0], &[1.0, 0.0, 0.0, 0.0]).unwrap();
    // x.go chunk: orthogonal to query (axis 2). Direct chunk_vec kNN with
    // the query vector below will rank this dead last among the dense
    // candidates with distance √2, which exceeds DENSE_LOOSE_DIST_MAX
    // (1.10), so the regular dense path drops it. Only the summary lane
    // can rescue x.go.
    s.upsert_embedding(ids[1], &[0.0, 0.0, 1.0, 0.0]).unwrap();

    let summary_store = SummaryStore::new(&s);
    let sid = summary_store
        .insert(&SummaryChunk {
            repo: "r1".into(),
            path: "x.go".into(),
            scope: "module".into(),
            text: "summary of x.go".into(),
        })
        .unwrap();
    // Summary embedding aligned with the query vector (axis 1).
    summary_store
        .upsert_embedding(sid, &[0.0, 1.0, 0.0, 0.0])
        .unwrap();

    let query_vec = [0.0f32, 1.0, 0.0, 0.0];
    let (hits, lane) = s
        .search_hybrid("keyword", 10, Some(&query_vec), None)
        .unwrap();

    assert_eq!(
        lane,
        SearchLane::Fusion,
        "expected fusion lane, got {lane:?}"
    );
    let paths: Vec<&str> = hits.iter().map(|h| h.path.as_str()).collect();
    assert!(
        paths.contains(&"x.go"),
        "expected x.go in hits (summary lane should bridge), got {paths:?}"
    );
}
