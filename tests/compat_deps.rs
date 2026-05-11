use std::fs;
use tempfile::tempdir;

#[path = "common/mod.rs"]
mod common;
use common::repolayer_cmd;

#[test]
fn deps_runs_on_simple_workspace() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("repolayer.yml"), "repos:\n  - path: ./\n").unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/foo.ts"), "export const x = 1;\n").unwrap();
    fs::write(
        dir.path().join("src/bar.ts"),
        "import { x } from './foo';\nconsole.log(x);\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{"name":"test","version":"0.0.1"}"#,
    )
    .unwrap();

    repolayer_cmd()
        .current_dir(dir.path())
        .arg("deps")
        .arg("src/bar.ts")
        .assert()
        .success();
}

#[test]
fn deps_json_flag_produces_valid_json() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("repolayer.yml"), "repos:\n  - path: ./\n").unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/a.ts"), "export const a = 1;\n").unwrap();
    fs::write(dir.path().join("src/b.ts"), "import { a } from './a';\n").unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{"name":"test2","version":"0.0.1"}"#,
    )
    .unwrap();

    let output = repolayer_cmd()
        .current_dir(dir.path())
        .arg("deps")
        .arg("src/b.ts")
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
    assert_eq!(v["schema_version"], "ast-outline.deps.v1");
    assert!(v["edges"].is_array());
}

#[test]
fn deps_no_deps_exits_ok() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("repolayer.yml"), "repos:\n  - path: ./\n").unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/isolated.ts"), "export const z = 42;\n").unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{"name":"test3","version":"0.0.1"}"#,
    )
    .unwrap();

    // Even when there are no deps the command should succeed.
    repolayer_cmd()
        .current_dir(dir.path())
        .arg("deps")
        .arg("src/isolated.ts")
        .assert()
        .success();
}
