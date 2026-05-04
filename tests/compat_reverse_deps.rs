use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

#[test]
fn reverse_deps_runs_on_simple_workspace() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("repolayer.yml"),
        "repos:\n  - path: ./\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/utils.ts"), "export function helper() {}\n").unwrap();
    fs::write(
        dir.path().join("src/main.ts"),
        "import { helper } from './utils';\nhelper();\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{"name":"rev-test","version":"0.0.1"}"#,
    )
    .unwrap();

    // Ask who imports utils.ts — should succeed regardless of whether it
    // finds callers (src/main.ts should be detected).
    Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(dir.path())
        .arg("reverse-deps")
        .arg("src/utils.ts")
        .assert()
        .success();
}

#[test]
fn reverse_deps_json_flag_produces_valid_json() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("repolayer.yml"),
        "repos:\n  - path: ./\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/lib.ts"), "export const LIB = true;\n").unwrap();
    fs::write(
        dir.path().join("src/consumer.ts"),
        "import { LIB } from './lib';\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{"name":"rev-test2","version":"0.0.1"}"#,
    )
    .unwrap();

    let output = Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(dir.path())
        .arg("reverse-deps")
        .arg("src/lib.ts")
        .arg("--json")
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout)
        .expect("stdout should be valid JSON");
    assert_eq!(v["schema_version"], "ast-outline.reverse-deps.v1");
    assert!(v["callers"].is_array());
    assert!(v["target"].is_string());
}
