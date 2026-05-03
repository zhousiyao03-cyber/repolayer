use repolayer::config::Config;
use std::path::PathBuf;

#[test]
fn loads_minimal_config() {
    let path = PathBuf::from("tests/fixtures/configs/minimal.yml");
    let cfg = Config::from_path(&path).unwrap();
    assert_eq!(cfg.repos.len(), 1);
    assert_eq!(cfg.repos[0].path, PathBuf::from("../my_repo"));
    assert!(cfg.llm.is_none());
}

#[test]
fn loads_full_config_with_idl_and_links() {
    let path = PathBuf::from("tests/fixtures/configs/full.yml");
    let cfg = Config::from_path(&path).unwrap();
    assert_eq!(cfg.repos.len(), 5);
    assert!(cfg.repos.iter().any(|r| r.is_idl()));
    assert_eq!(cfg.links.len(), 1);
    assert!(cfg.llm.is_some());
}

#[test]
fn missing_file_returns_clear_error() {
    let err = Config::from_path(&PathBuf::from("/no/such/file")).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("not found") || msg.contains("No such file"), "got: {}", msg);
}
