//! Verify that `repolayer build` auto-extracts Calls edges via the
//! CallsLinker, and that the new `callers` CLI surfaces them end-to-end
//! against a real source fixture (no edge injection).
//!
//! Granularity reminder: edges are `(Module, Function)`. The Module is
//! the file containing the call site, NOT the enclosing function. Tests
//! assert on the file path of the caller, not on a per-function caller
//! symbol.

use rusqlite::Connection;
use std::fs;
use tempfile::tempdir;

#[path = "common/mod.rs"]
mod common;
use common::repolayer_cmd;

fn make_caller_callee_workspace() -> tempfile::TempDir {
    let workspace = tempdir().unwrap();
    let repo = workspace.path().join("repo");
    fs::create_dir_all(repo.join("src")).unwrap();
    // Unique name `computeMembershipDigest` keeps the uniqueness filter happy
    // even if other fixtures get added later. `auth_caller.ts` calls it,
    // `digest_util.ts` defines it.
    fs::write(
        repo.join("src/digest_util.ts"),
        "export function computeMembershipDigest(input: string): string {\n  return input;\n}\n",
    )
    .unwrap();
    fs::write(
        repo.join("src/auth_caller.ts"),
        "import { computeMembershipDigest } from './digest_util';\n\
         export function authorize(token: string) {\n\
           return computeMembershipDigest(token);\n\
         }\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join("repolayer.yml"),
        format!("repos:\n  - {{ name: r, path: {} }}\n", repo.display()),
    )
    .unwrap();
    repolayer_cmd()
        .current_dir(workspace.path())
        .arg("build")
        .assert()
        .success();
    workspace
}

#[test]
fn build_extracts_calls_edge_from_real_source() {
    let ws = make_caller_callee_workspace();
    let conn = Connection::open(ws.path().join(".repolayer/index.db")).unwrap();
    let calls: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM edges e
             JOIN nodes nf ON e.from_id = nf.id
             JOIN nodes nt ON e.to_id   = nt.id
             WHERE e.kind = 'calls'
               AND nt.symbol = 'computeMembershipDigest'
               AND nf.path = 'src/auth_caller.ts'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        calls >= 1,
        "expected ≥1 Calls edge from src/auth_caller.ts to computeMembershipDigest, got {calls}"
    );
}

#[test]
fn callers_cli_surfaces_auto_extracted_edge() {
    let ws = make_caller_callee_workspace();
    let out = repolayer_cmd()
        .current_dir(ws.path())
        .args(["callers", "computeMembershipDigest", "--json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let callers = v["callers"].as_array().unwrap();
    assert!(
        !callers.is_empty(),
        "expected at least one caller chain, got empty"
    );
    let paths: Vec<&str> = callers
        .iter()
        .filter_map(|c| c["caller"]["path"].as_str())
        .collect();
    assert!(
        paths.iter().any(|p| *p == "src/auth_caller.ts"),
        "auth_caller.ts should appear as caller; got paths = {paths:?}"
    );
}

#[test]
fn ambiguous_name_does_not_get_calls_edge() {
    // Two definitions of `init` in different files — the uniqueness filter
    // must skip it, so the auto-extractor produces zero Calls edges for it.
    let workspace = tempdir().unwrap();
    let repo = workspace.path().join("repo");
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(
        repo.join("src/a.ts"),
        "export function init() {}\nexport function callsItA() { init(); }\n",
    )
    .unwrap();
    fs::write(
        repo.join("src/b.ts"),
        "export function init() {}\nexport function callsItB() { init(); }\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join("repolayer.yml"),
        format!("repos:\n  - {{ name: r, path: {} }}\n", repo.display()),
    )
    .unwrap();
    repolayer_cmd()
        .current_dir(workspace.path())
        .arg("build")
        .assert()
        .success();

    let conn = Connection::open(workspace.path().join(".repolayer/index.db")).unwrap();
    let calls: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM edges e
             JOIN nodes nt ON e.to_id = nt.id
             WHERE e.kind = 'calls' AND nt.symbol = 'init'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        calls, 0,
        "ambiguous name `init` should not produce Calls edges (confidence=1.0 unique-only rule)"
    );
}

#[test]
fn noise_short_names_are_skipped() {
    // Names like `get` (3 chars) or all-lowercase `data` should be filtered
    // by is_noise_name and not produce edges, even if unique.
    let workspace = tempdir().unwrap();
    let repo = workspace.path().join("repo");
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(
        repo.join("src/lib.ts"),
        "export function get() {}\nexport function caller() { get(); }\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join("repolayer.yml"),
        format!("repos:\n  - {{ name: r, path: {} }}\n", repo.display()),
    )
    .unwrap();
    repolayer_cmd()
        .current_dir(workspace.path())
        .arg("build")
        .assert()
        .success();

    let conn = Connection::open(workspace.path().join(".repolayer/index.db")).unwrap();
    let calls: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM edges e
             JOIN nodes nt ON e.to_id = nt.id
             WHERE e.kind = 'calls' AND nt.symbol = 'get'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        calls, 0,
        "short noise name `get` should be skipped by the linker"
    );
}
