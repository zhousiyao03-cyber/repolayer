use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

#[test]
fn install_writes_cursor_config() {
    let home = tempdir().unwrap();

    Command::cargo_bin("repolayer")
        .unwrap()
        .env("HOME", home.path())
        .arg("install")
        .arg("--mcp")
        .arg("cursor")
        .assert()
        .success();

    let config = home.path().join(".cursor").join("mcp.json");
    assert!(config.exists(), "expected {}", config.display());

    let content = fs::read_to_string(&config).unwrap();
    assert!(
        content.contains("repolayer"),
        "config should mention repolayer: {}",
        content
    );
    assert!(
        content.contains("mcpServers"),
        "config should have mcpServers key: {}",
        content
    );
}

#[test]
fn install_unknown_agent_errors() {
    let home = tempdir().unwrap();
    Command::cargo_bin("repolayer")
        .unwrap()
        .env("HOME", home.path())
        .arg("install")
        .arg("--mcp")
        .arg("nonexistent-agent")
        .assert()
        .failure();
}

#[test]
fn install_preserves_existing_servers() {
    let home = tempdir().unwrap();
    let cursor_dir = home.path().join(".cursor");
    fs::create_dir_all(&cursor_dir).unwrap();
    fs::write(
        cursor_dir.join("mcp.json"),
        r#"{"mcpServers":{"existing":{"command":"foo"}}}"#,
    )
    .unwrap();

    Command::cargo_bin("repolayer")
        .unwrap()
        .env("HOME", home.path())
        .arg("install")
        .arg("--mcp")
        .arg("cursor")
        .assert()
        .success();

    let content = fs::read_to_string(cursor_dir.join("mcp.json")).unwrap();
    assert!(
        content.contains("\"existing\""),
        "should preserve existing entry: {}",
        content
    );
    assert!(
        content.contains("\"repolayer\""),
        "should add repolayer: {}",
        content
    );
}
