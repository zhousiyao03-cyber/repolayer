use repolayer::core::declaration::{Declaration, DeclarationKind, ParseResult};
use repolayer::outline::store::OutlineStore;
use std::path::PathBuf;
use tempfile::tempdir;

fn make_parse_result() -> ParseResult {
    ParseResult {
        path: PathBuf::from("src/foo.rs"),
        language: "rust",
        source: b"pub fn foo() {}".to_vec(),
        line_count: 1,
        error_count: 0,
        declarations: vec![Declaration {
            kind: DeclarationKind::Function,
            name: "foo".into(),
            signature: "pub fn foo()".into(),
            ..Default::default()
        }],
    }
}

#[test]
fn create_and_open_writes_schema_version() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("outline.db");
    let store = OutlineStore::open(&path).unwrap();
    assert_eq!(store.schema_version().unwrap(), 1);
}

#[test]
fn upsert_and_get_roundtrips_declarations() {
    let dir = tempdir().unwrap();
    let store = OutlineStore::open(&dir.path().join("outline.db")).unwrap();
    let pr = make_parse_result();
    store.upsert("repo1", &pr, &[0u8; 32]).unwrap();
    let got = store.get("repo1", "src/foo.rs").unwrap().unwrap();
    assert_eq!(got.language, "rust");
    assert_eq!(got.declarations.len(), 1);
    assert_eq!(got.declarations[0].name, "foo");
}

#[test]
fn upsert_replaces_on_same_key() {
    let dir = tempdir().unwrap();
    let store = OutlineStore::open(&dir.path().join("outline.db")).unwrap();
    let mut pr = make_parse_result();
    store.upsert("repo1", &pr, &[0u8; 32]).unwrap();
    pr.declarations[0].name = "bar".into();
    store.upsert("repo1", &pr, &[1u8; 32]).unwrap();
    let got = store.get("repo1", "src/foo.rs").unwrap().unwrap();
    assert_eq!(got.declarations[0].name, "bar");
}

#[test]
fn delete_removes_row() {
    let dir = tempdir().unwrap();
    let store = OutlineStore::open(&dir.path().join("outline.db")).unwrap();
    store.upsert("repo1", &make_parse_result(), &[0u8; 32]).unwrap();
    store.delete("repo1", "src/foo.rs").unwrap();
    assert!(store.get("repo1", "src/foo.rs").unwrap().is_none());
}

#[test]
fn list_files_filtered_by_repo() {
    let dir = tempdir().unwrap();
    let store = OutlineStore::open(&dir.path().join("outline.db")).unwrap();
    let pr = make_parse_result();
    store.upsert("repo1", &pr, &[0u8; 32]).unwrap();
    let mut pr2 = make_parse_result();
    pr2.path = PathBuf::from("src/bar.rs");
    store.upsert("repo2", &pr2, &[0u8; 32]).unwrap();
    let r1 = store.list_files("repo1").unwrap();
    assert_eq!(r1.len(), 1);
    assert_eq!(r1[0].1, "src/foo.rs");
}
