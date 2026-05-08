use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

/// Helper: set up a minimal workspace, run `build`, return the tempdir.
fn make_workspace_with_build() -> tempfile::TempDir {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(
        dir.path().join("src/auth.ts"),
        "export function authenticate(user: string): boolean { return user === 'admin'; }\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("src/session.ts"),
        "export function createSession(userId: string): string { return `sess_${userId}`; }\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("repolayer.yml"),
        "repos:\n  - { name: my_test_repo, path: ./ }\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{"name":"search-test","version":"0.1.0"}"#,
    )
    .unwrap();

    Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(dir.path())
        .arg("build")
        .assert()
        .success();

    dir
}

#[test]
fn search_no_index_exits_nonzero() {
    let dir = tempdir().unwrap();
    Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(dir.path())
        .arg("search")
        .arg("authenticate")
        .assert()
        .failure();
}

#[test]
fn search_runs_after_build() {
    let dir = make_workspace_with_build();
    Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(dir.path())
        .arg("search")
        .arg("authenticate")
        .assert()
        .success();
}

#[test]
fn search_uses_repolayer_index_env_when_cwd_has_no_index() {
    // Simulates: agent cd's into a business repo (no .repolayer/) but
    // REPOLAYER_INDEX points at the cross-repo workspace.
    let workspace = make_workspace_with_build();
    let unrelated_dir = tempdir().unwrap();
    Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(unrelated_dir.path()) // no .repolayer/ here
        .env("REPOLAYER_INDEX", workspace.path())
        .arg("search")
        .arg("authenticate")
        .arg("--json")
        .assert()
        .success();
}

#[test]
fn search_repolayer_index_env_pointing_nowhere_errors_clearly() {
    let dir = tempdir().unwrap();
    let output = Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(dir.path())
        .env("REPOLAYER_INDEX", "/no/such/dir/repolayer_test_42")
        .arg("search")
        .arg("anything")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("REPOLAYER_INDEX"),
        "stderr should explain the env var: {stderr}"
    );
}

#[test]
fn search_json_produces_valid_json() {
    let dir = make_workspace_with_build();
    let output = Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(dir.path())
        .arg("search")
        .arg("authenticate")
        .arg("--json")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");
    assert_eq!(v["schema_version"], "repolayer.search.v1");
    assert_eq!(v["query"], "authenticate");
    assert_eq!(v["full_content"], false);
    assert!(v["hits"].is_array());
    // We should find at least the auth.ts chunk.
    let hits = v["hits"].as_array().unwrap();
    assert!(!hits.is_empty(), "expected at least one hit for 'authenticate'");
    // Default JSON output omits `content` (token-heavy) in favour of `preview`.
    let first = &hits[0];
    assert!(first.get("content").is_none(), "default JSON should omit content");
    assert!(first["preview"].is_string(), "default JSON should include preview");
}

#[test]
fn search_full_content_includes_chunk_body() {
    let dir = make_workspace_with_build();
    let output = Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(dir.path())
        .arg("search")
        .arg("authenticate")
        .arg("--json")
        .arg("--full-content")
        .output()
        .unwrap();
    assert!(output.status.success());
    let v: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["full_content"], true);
    let first = &v["hits"][0];
    assert!(first["content"].is_string(), "--full-content must include the chunk body");
    assert!(first.get("preview").is_none(), "--full-content drops the preview field");
}

#[test]
fn search_no_match_exits_zero() {
    // Empty result is still a success exit code; a message goes to stderr.
    let dir = make_workspace_with_build();
    Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(dir.path())
        .arg("search")
        .arg("xyzzy_no_such_symbol_ever")
        .assert()
        .success();
}

#[test]
fn search_repo_filter_passes_through() {
    let dir = make_workspace_with_build();
    let output = Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(dir.path())
        .arg("search")
        .arg("authenticate")
        .arg("--repo")
        .arg("my_test_repo")
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let v: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["repo_filter"], "my_test_repo");
    let hits = v["hits"].as_array().unwrap();
    assert!(!hits.is_empty(), "expected hits in my_test_repo");
    for hit in hits {
        assert_eq!(hit["repo"], "my_test_repo");
    }
}

#[test]
fn search_unknown_repo_errors_with_suggestion() {
    let dir = make_workspace_with_build();
    let output = Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(dir.path())
        .arg("search")
        .arg("authenticate")
        .arg("--repo")
        .arg("my_test_rep") // typo
        .output()
        .unwrap();
    assert!(!output.status.success(), "should fail on unknown repo");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("my_test_repo"),
        "stderr should suggest the correct name: {stderr}"
    );
}

#[test]
fn search_k_limits_results() {
    let dir = make_workspace_with_build();
    let output = Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(dir.path())
        .arg("search")
        .arg("export")
        .arg("-k")
        .arg("1")
        .arg("--json")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let hits = v["hits"].as_array().unwrap();
    assert!(hits.len() <= 1, "expected ≤1 hit, got {}", hits.len());
}
