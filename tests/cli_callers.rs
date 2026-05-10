use assert_cmd::Command;
use repolayer::graph::model::*;
use repolayer::graph::store::Store;
use std::fs;
use tempfile::tempdir;

/// Build a tiny indexed workspace, then inject Calls edges directly into the
/// graph store. Calls edges are not yet auto-extracted by the indexer, so
/// CLI behaviour must be exercised via injection — exactly the path real
/// users hit when they declare `links: [{kind: calls, ...}]` or when a
/// future AST extractor lands.
fn build_workspace_with_calls() -> tempfile::TempDir {
    let workspace = tempdir().unwrap();
    let repo = workspace.path().join("repo");
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(
        repo.join("src/utils.ts"),
        "export function hash(s: string): string { return s; }\n",
    )
    .unwrap();
    fs::write(
        repo.join("src/auth.ts"),
        "import { hash } from './utils';\n\
         export function login(u: string, p: string) { return hash(p) === u; }\n\
         export function adminLogin(u: string, p: string) { return hash(p) === u; }\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join("repolayer.yml"),
        format!("repos:\n  - {{ name: r, path: {} }}\n", repo.display()),
    )
    .unwrap();

    Command::cargo_bin("repolayer")
        .unwrap()
        .env_remove("REPOLAYER_INDEX")
        .current_dir(workspace.path())
        .arg("build")
        .assert()
        .success();

    // Inject Calls edges: login -> hash, adminLogin -> hash.
    let store = Store::open(&workspace.path().join(".repolayer/index.db")).unwrap();
    let hash_node = store
        .search_symbols_substring("hash", 50)
        .unwrap()
        .into_iter()
        .find(|n| n.symbol.as_deref() == Some("hash"))
        .expect("hash function should be indexed");
    let login_node = store
        .search_symbols_substring("login", 50)
        .unwrap()
        .into_iter()
        .find(|n| n.symbol.as_deref() == Some("login"))
        .expect("login function should be indexed");
    let admin_node = store
        .search_symbols_substring("adminLogin", 50)
        .unwrap()
        .into_iter()
        .find(|n| n.symbol.as_deref() == Some("adminLogin"))
        .expect("adminLogin function should be indexed");
    for caller in [&login_node, &admin_node] {
        store
            .upsert_edge(&Edge {
                from: caller.id.clone(),
                to: hash_node.id.clone(),
                kind: EdgeKind::Calls,
                confidence: 1.0,
            })
            .unwrap();
    }
    drop(store);
    workspace
}

#[test]
fn callers_lists_inbound_calls_edges() {
    let ws = build_workspace_with_calls();

    let out = Command::cargo_bin("repolayer")
        .unwrap()
        .env_remove("REPOLAYER_INDEX")
        .current_dir(ws.path())
        .args(["callers", "hash", "--json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["schema_version"], "repolayer.get_callers.v1");
    assert_eq!(v["symbol"], "hash");
    let callers: Vec<String> = v["callers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["caller"]["symbol"].as_str().unwrap().to_string())
        .collect();
    assert!(callers.contains(&"login".to_string()));
    assert!(callers.contains(&"adminLogin".to_string()));
}

#[test]
fn callers_human_output_shows_definition_and_callers() {
    let ws = build_workspace_with_calls();
    let out = Command::cargo_bin("repolayer")
        .unwrap()
        .env_remove("REPOLAYER_INDEX")
        .current_dir(ws.path())
        .args(["callers", "hash"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("@def"),
        "expected definition line, got: {stdout}"
    );
    assert!(stdout.contains("login"));
    assert!(stdout.contains("conf="));
}

#[test]
fn callers_no_match_emits_friendly_fallback() {
    let ws = build_workspace_with_calls();
    let out = Command::cargo_bin("repolayer")
        .unwrap()
        .env_remove("REPOLAYER_INDEX")
        .current_dir(ws.path())
        .args(["callers", "nonexistent_xyz"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("no exact match"));
    assert!(stdout.contains("repolayer query"));
}

#[test]
fn callers_no_inbound_edges_explains_absence() {
    // hash is defined and has callers in our injected fixture, but `login`
    // itself has no inbound Calls — verify the "no callers" explainer.
    let ws = build_workspace_with_calls();
    let out = Command::cargo_bin("repolayer")
        .unwrap()
        .env_remove("REPOLAYER_INDEX")
        .current_dir(ws.path())
        .args(["callers", "login"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("no inbound Calls edges"));
    assert!(stdout.contains("dynamic dispatch"));
}

#[test]
fn callers_repo_filter_validates_unknown_repo() {
    let ws = build_workspace_with_calls();
    let out = Command::cargo_bin("repolayer")
        .unwrap()
        .env_remove("REPOLAYER_INDEX")
        .current_dir(ws.path())
        .args(["callers", "hash", "--repo", "wrong_name"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("r"),
        "should suggest the known repo name 'r': {stderr}"
    );
}

#[test]
fn callers_aggregates_across_same_named_definitions() {
    // Two repos each define `bootstrapXyz`, each with one caller — `callers
    // bootstrapXyz` should aggregate both definitions and report both
    // callers. A long unusual name avoids substring collisions with stdlib
    // or fixture symbols (e.g. `init`, `start`).
    let workspace = tempdir().unwrap();
    for (repo_name, dir) in [("alpha", "repo_a"), ("beta", "repo_b")] {
        let repo = workspace.path().join(dir);
        fs::create_dir_all(repo.join("src")).unwrap();
        fs::write(
            repo.join("src/m.ts"),
            format!(
                "export function bootstrapXyz() {{}}\nexport function caller_{}() {{ bootstrapXyz(); }}\n",
                repo_name
            ),
        )
        .unwrap();
    }
    fs::write(
        workspace.path().join("repolayer.yml"),
        "repos:\n  - { name: alpha, path: ./repo_a }\n  - { name: beta, path: ./repo_b }\n",
    )
    .unwrap();
    Command::cargo_bin("repolayer")
        .unwrap()
        .env_remove("REPOLAYER_INDEX")
        .current_dir(workspace.path())
        .arg("build")
        .assert()
        .success();

    let store = Store::open(&workspace.path().join(".repolayer/index.db")).unwrap();
    let defs: Vec<Node> = store
        .search_symbols_substring("bootstrapXyz", 50)
        .unwrap()
        .into_iter()
        .filter(|n| n.symbol.as_deref() == Some("bootstrapXyz"))
        .collect();
    assert_eq!(
        defs.len(),
        2,
        "expected 2 `bootstrapXyz` definitions across repos"
    );
    for def in &defs {
        let starter_name = format!("caller_{}", def.repo);
        let starter = store
            .search_symbols_substring(&starter_name, 50)
            .unwrap()
            .into_iter()
            .find(|n| n.repo == def.repo && n.symbol.as_deref() == Some(&starter_name))
            .unwrap_or_else(|| panic!("missing starter {} in repo {}", starter_name, def.repo));
        store
            .upsert_edge(&Edge {
                from: starter.id,
                to: def.id.clone(),
                kind: EdgeKind::Calls,
                confidence: 1.0,
            })
            .unwrap();
    }
    drop(store);

    let out = Command::cargo_bin("repolayer")
        .unwrap()
        .env_remove("REPOLAYER_INDEX")
        .current_dir(workspace.path())
        .args(["callers", "bootstrapXyz", "--json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        v["definitions"].as_array().unwrap().len(),
        2,
        "should surface both definitions"
    );
    let caller_repos: std::collections::HashSet<String> = v["callers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["caller"]["repo"].as_str().unwrap().to_string())
        .collect();
    assert!(caller_repos.contains("alpha"));
    assert!(caller_repos.contains("beta"));
}
