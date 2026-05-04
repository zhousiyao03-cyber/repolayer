use repolayer::search::chunker::Chunk;
use repolayer::search::store::SearchStore;
use tempfile::tempdir;

fn make_chunk(path: &str, line: u32, content: &str) -> Chunk {
    Chunk {
        file_path: path.into(),
        start_line: line,
        end_line: line + 5,
        start_byte: 0,
        end_byte: content.len() as u32,
        language: "rust".into(),
        content: content.into(),
    }
}

#[test]
fn open_writes_schema_version_1() {
    let dir = tempdir().unwrap();
    let s = SearchStore::open(&dir.path().join("search.db")).unwrap();
    assert_eq!(s.schema_version().unwrap(), 1);
}

#[test]
fn round_trip_chunks() {
    let dir = tempdir().unwrap();
    let s = SearchStore::open(&dir.path().join("search.db")).unwrap();
    let chunks = vec![
        make_chunk("src/foo.rs", 1, "fn foo() {}"),
        make_chunk("src/bar.rs", 10, "fn bar() {}"),
    ];
    s.replace_repo_chunks("repo1", &chunks).unwrap();
    let listed = s.list_chunks("repo1").unwrap();
    assert_eq!(listed.len(), 2);
}

#[test]
fn replace_clears_old_chunks() {
    let dir = tempdir().unwrap();
    let s = SearchStore::open(&dir.path().join("search.db")).unwrap();
    s.replace_repo_chunks("repo1", &[make_chunk("src/foo.rs", 1, "old")])
        .unwrap();
    s.replace_repo_chunks("repo1", &[]).unwrap();
    assert_eq!(s.list_chunks("repo1").unwrap().len(), 0);
}

#[test]
fn multi_repo_isolation() {
    let dir = tempdir().unwrap();
    let s = SearchStore::open(&dir.path().join("search.db")).unwrap();
    s.replace_repo_chunks("repo1", &[make_chunk("a.rs", 1, "x")])
        .unwrap();
    s.replace_repo_chunks("repo2", &[make_chunk("b.rs", 1, "y")])
        .unwrap();
    assert_eq!(s.list_chunks("repo1").unwrap().len(), 1);
    assert_eq!(s.list_chunks("repo2").unwrap().len(), 1);
}

#[test]
fn delete_file_removes_only_target() {
    let dir = tempdir().unwrap();
    let s = SearchStore::open(&dir.path().join("search.db")).unwrap();
    let chunks = vec![
        make_chunk("src/foo.rs", 1, "fn foo() {}"),
        make_chunk("src/bar.rs", 10, "fn bar() {}"),
    ];
    s.replace_repo_chunks("repo1", &chunks).unwrap();
    s.delete_file("repo1", "src/foo.rs").unwrap();
    let listed = s.list_chunks("repo1").unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].1, "src/bar.rs");
}

#[test]
fn chunk_fields_preserved() {
    let dir = tempdir().unwrap();
    let s = SearchStore::open(&dir.path().join("search.db")).unwrap();
    let chunks = vec![make_chunk("src/lib.rs", 42, "pub fn answer() -> u32 { 42 }")];
    s.replace_repo_chunks("myrepo", &chunks).unwrap();
    let listed = s.list_chunks("myrepo").unwrap();
    assert_eq!(listed.len(), 1);
    let (_, path, start, end, content) = &listed[0];
    assert_eq!(path, "src/lib.rs");
    assert_eq!(*start, 42u32);
    assert_eq!(*end, 47u32);
    assert!(content.contains("answer"));
}
