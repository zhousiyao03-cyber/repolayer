use std::fs;
use tempfile::tempdir;

#[path = "common/mod.rs"]
mod common;
use common::repolayer_cmd;

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
    fs::write(dir.path().join("repolayer.yml"), "repos:\n  - path: ./\n").unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{"name":"find-related-test","version":"0.1.0"}"#,
    )
    .unwrap();

    repolayer_cmd()
        .current_dir(dir.path())
        .arg("build")
        .assert()
        .success();

    dir
}

#[test]
fn find_related_no_index_exits_nonzero() {
    let dir = tempdir().unwrap();
    repolayer_cmd()
        .current_dir(dir.path())
        .arg("find-related")
        .arg("src/auth.ts:1")
        .assert()
        .failure();
}

#[test]
fn find_related_bad_spec_exits_nonzero() {
    let dir = make_workspace_with_build();
    // Non-existent file + line → no chunk found → error.
    repolayer_cmd()
        .current_dir(dir.path())
        .arg("find-related")
        .arg("nonexistent_file.ts:99")
        .assert()
        .failure();
}

#[test]
fn find_related_json_produces_valid_json() {
    let dir = make_workspace_with_build();
    let auth_path = dir.path().join("src/auth.ts");

    let output = repolayer_cmd()
        .current_dir(dir.path())
        .arg("find-related")
        .arg(format!("{}:1", auth_path.display()))
        .arg("--json")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("stdout should be valid JSON");
    assert_eq!(v["schema_version"], "ast-outline.find_related.v1");
    assert!(v["source"].is_string());
    assert!(v["hits"].is_array());

    // The source file itself should not appear in results.
    let hits = v["hits"].as_array().unwrap();
    for hit in hits {
        let path = hit["path"].as_str().unwrap_or("");
        assert!(
            !path.contains("auth.ts"),
            "source file should be excluded from results, got path: {path}"
        );
    }
}

#[test]
fn find_related_runs_after_build() {
    let dir = make_workspace_with_build();
    let auth_path = dir.path().join("src/auth.ts");

    repolayer_cmd()
        .current_dir(dir.path())
        .arg("find-related")
        .arg(format!("{}:1", auth_path.display()))
        .assert()
        .success();
}
