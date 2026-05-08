use assert_cmd::Command;
use predicates::str::contains;
use std::fs;
use tempfile::tempdir;

#[test]
fn query_finds_symbols_by_substring() {
    let workspace = tempdir().unwrap();
    let repo_src = std::path::Path::new("tests/fixtures/single_repo_ts");
    let repo_dst = workspace.path().join("single_repo_ts");
    copy_dir_all(repo_src, &repo_dst).unwrap();

    fs::write(
        workspace.path().join("repolayer.yml"),
        format!("repos:\n  - path: {}\n", repo_dst.display()),
    )
    .unwrap();

    Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(workspace.path())
        .arg("build")
        .assert()
        .success();

    Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(workspace.path())
        .args(["query", "login"])
        .assert()
        .success()
        .stdout(contains("login"));
}

#[test]
fn query_returns_no_matches_message_when_empty() {
    let workspace = tempdir().unwrap();
    let repo_src = std::path::Path::new("tests/fixtures/single_repo_ts");
    let repo_dst = workspace.path().join("single_repo_ts");
    copy_dir_all(repo_src, &repo_dst).unwrap();

    fs::write(
        workspace.path().join("repolayer.yml"),
        format!("repos:\n  - path: {}\n", repo_dst.display()),
    )
    .unwrap();

    Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(workspace.path())
        .arg("build")
        .assert()
        .success();

    Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(workspace.path())
        .args(["query", "nonexistent_xyzabc"])
        .assert()
        .success()
        .stdout(contains("no matches"));
}

#[test]
fn query_treats_underscore_as_literal_not_wildcard() {
    let workspace = tempdir().unwrap();
    let repo = workspace.path().join("repo");
    fs::create_dir_all(repo.join("src")).unwrap();
    // Two symbols: "get_user" (literal underscore) and "getXuser" (X in middle).
    // A naive LIKE %get_user% would match both because _ is a wildcard.
    fs::write(
        repo.join("src/a.ts"),
        "export function get_user() {}\nexport function getXuser() {}\n",
    )
    .unwrap();

    fs::write(
        workspace.path().join("repolayer.yml"),
        format!("repos:\n  - path: {}\n", repo.display()),
    )
    .unwrap();

    Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(workspace.path())
        .arg("build")
        .assert()
        .success();

    let output = Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(workspace.path())
        .args(["query", "get_user"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("get_user"), "must match literal underscore");
    assert!(
        !stdout.contains("getXuser"),
        "underscore must NOT be treated as a wildcard (was the regression). stdout: {}",
        stdout
    );
}

#[test]
fn query_includes_idl_method_and_service_nodes() {
    // Regression: previous kind whitelist excluded `idlmethod` / `idlservice`,
    // forcing the agent to grep IDL files separately when tracing an API
    // endpoint. Verify the IDL fixture's service + method nodes show up.
    let workspace = tempdir().unwrap();
    let repo_src = std::path::Path::new("tests/fixtures/idl");
    let repo_dst = workspace.path().join("idl");
    copy_dir_all(repo_src, &repo_dst).unwrap();

    fs::write(
        workspace.path().join("repolayer.yml"),
        format!(
            "repos:\n  - {{ name: idl, path: {}, type: idl }}\n",
            repo_dst.display()
        ),
    )
    .unwrap();

    Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(workspace.path())
        .arg("build")
        .assert()
        .success();

    let out = Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(workspace.path())
        .args(["query", "GetBenefit", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let kinds: std::collections::HashSet<String> = v["matches"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["kind"].as_str().map(|s| s.to_string()))
        .collect();
    assert!(
        kinds.contains("idlmethod"),
        "query should now surface idlmethod nodes; got kinds = {kinds:?}"
    );
}

#[test]
fn query_repo_filter_restricts_to_named_repo() {
    // Two repos with the same symbol; --repo must filter to one of them.
    let workspace = tempdir().unwrap();
    let repo_a = workspace.path().join("repo_a");
    let repo_b = workspace.path().join("repo_b");
    fs::create_dir_all(repo_a.join("src")).unwrap();
    fs::create_dir_all(repo_b.join("src")).unwrap();
    fs::write(
        repo_a.join("src/a.ts"),
        "export function shared_symbol() {}\n",
    )
    .unwrap();
    fs::write(
        repo_b.join("src/b.ts"),
        "export function shared_symbol() {}\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join("repolayer.yml"),
        format!(
            "repos:\n  - {{ name: alpha, path: {} }}\n  - {{ name: beta, path: {} }}\n",
            repo_a.display(),
            repo_b.display()
        ),
    )
    .unwrap();

    Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(workspace.path())
        .arg("build")
        .assert()
        .success();

    // Without filter: should hit both repos.
    let no_filter = Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(workspace.path())
        .args(["query", "shared_symbol", "--json"])
        .output()
        .unwrap();
    let nf: serde_json::Value = serde_json::from_slice(&no_filter.stdout).unwrap();
    let nf_repos: std::collections::HashSet<String> = nf["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["repo"].as_str().unwrap().to_string())
        .collect();
    assert!(nf_repos.contains("alpha") && nf_repos.contains("beta"));

    // With --repo alpha: only alpha.
    let filtered = Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(workspace.path())
        .args(["query", "shared_symbol", "--repo", "alpha", "--json"])
        .output()
        .unwrap();
    let f: serde_json::Value = serde_json::from_slice(&filtered.stdout).unwrap();
    assert_eq!(f["repo_filter"], "alpha");
    for m in f["matches"].as_array().unwrap() {
        assert_eq!(m["repo"], "alpha");
    }
}

#[test]
fn query_unknown_repo_errors_with_suggestion() {
    let workspace = tempdir().unwrap();
    let repo = workspace.path().join("repo");
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src/a.ts"), "export function fn() {}\n").unwrap();
    fs::write(
        workspace.path().join("repolayer.yml"),
        format!("repos:\n  - {{ name: known_name, path: {} }}\n", repo.display()),
    )
    .unwrap();

    Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(workspace.path())
        .arg("build")
        .assert()
        .success();

    let out = Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(workspace.path())
        .args(["query", "fn", "--repo", "knwon_name"]) // typo
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("known_name"),
        "stderr should suggest the correct repo name: {stderr}"
    );
}

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
