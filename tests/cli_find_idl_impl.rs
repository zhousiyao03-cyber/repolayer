use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn build_idl_workspace() -> tempfile::TempDir {
    let workspace = tempdir().unwrap();
    let src = std::path::Path::new("tests/fixtures/multi_repo_with_idl");
    copy_dir_all(src, workspace.path()).unwrap();
    fs::write(
        workspace.path().join("repolayer.yml"),
        r#"
repos:
  - path: ./idl
    type: idl
  - path: ./server_repo
  - path: ./client_repo
"#,
    )
    .unwrap();
    Command::cargo_bin("repolayer")
        .unwrap()
        .env_remove("REPOLAYER_INDEX")
        .current_dir(workspace.path())
        .arg("build")
        .assert()
        .success();
    workspace
}

#[test]
fn find_idl_impl_returns_implements_and_invokes() {
    let ws = build_idl_workspace();
    let out = Command::cargo_bin("repolayer")
        .unwrap()
        .env_remove("REPOLAYER_INDEX")
        .current_dir(ws.path())
        .args(["find-idl-impl", "GetBenefit", "--json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["schema_version"], "repolayer.find_idl_impl.v1");
    // IDL method node symbol is `<Service>.<Method>` — verify it ends with the
    // method name we asked for, regardless of service prefix.
    let sym = v["method"]["symbol"].as_str().unwrap();
    assert!(
        sym.ends_with("GetBenefit"),
        "expected method symbol to end with GetBenefit, got: {sym}"
    );

    let impl_repos: Vec<String> = v["implements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["repo"].as_str().unwrap().to_string())
        .collect();
    assert!(
        impl_repos.iter().any(|r| r == "server_repo"),
        "implements should include server_repo: {impl_repos:?}"
    );

    let invoke_repos: Vec<String> = v["invokes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["repo"].as_str().unwrap().to_string())
        .collect();
    assert!(
        invoke_repos.iter().any(|r| r == "client_repo"),
        "invokes should include client_repo: {invoke_repos:?}"
    );

    // Confidence field present and within [0, 1].
    for arr in [
        v["implements"].as_array().unwrap(),
        v["invokes"].as_array().unwrap(),
    ] {
        for item in arr {
            let conf = item["confidence"].as_f64().unwrap();
            assert!(
                (0.0..=1.0).contains(&conf),
                "confidence out of range: {conf}"
            );
        }
    }
}

#[test]
fn find_idl_impl_human_output_includes_confidence_guide() {
    let ws = build_idl_workspace();
    let out = Command::cargo_bin("repolayer")
        .unwrap()
        .env_remove("REPOLAYER_INDEX")
        .current_dir(ws.path())
        .args(["find-idl-impl", "GetBenefit"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("IDL method:"));
    assert!(stdout.contains("conf="));
    assert!(stdout.contains("confidence guide"));
}

#[test]
fn find_idl_impl_unknown_method_emits_fallback() {
    let ws = build_idl_workspace();
    let out = Command::cargo_bin("repolayer")
        .unwrap()
        .env_remove("REPOLAYER_INDEX")
        .current_dir(ws.path())
        .args(["find-idl-impl", "NoSuchRpc"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("no IDL method found"));
    assert!(stdout.contains("repolayer query"));
}

#[test]
fn find_idl_impl_no_implements_flag_skips_server_side() {
    let ws = build_idl_workspace();
    let out = Command::cargo_bin("repolayer")
        .unwrap()
        .env_remove("REPOLAYER_INDEX")
        .current_dir(ws.path())
        .args(["find-idl-impl", "GetBenefit", "--no-implements", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        v["implements"].as_array().unwrap().len(),
        0,
        "--no-implements should suppress server-side edges"
    );
    assert!(
        !v["invokes"].as_array().unwrap().is_empty(),
        "invokes should still be present"
    );
}
