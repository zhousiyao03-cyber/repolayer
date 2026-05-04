use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

#[test]
fn cycles_no_cycles_exits_zero() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("repolayer.yml"),
        "repos:\n  - path: ./\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    // Acyclic graph: a → b → c
    fs::write(dir.path().join("src/c.ts"), "export const C = 3;\n").unwrap();
    fs::write(
        dir.path().join("src/b.ts"),
        "import { C } from './c';\nexport const B = C + 1;\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("src/a.ts"),
        "import { B } from './b';\nconsole.log(B);\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{"name":"cycles-test","version":"0.0.1"}"#,
    )
    .unwrap();

    // No cycles → exit code 0.
    Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(dir.path())
        .arg("cycles")
        .assert()
        .success();
}

#[test]
fn cycles_json_no_cycles_produces_valid_json() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("repolayer.yml"),
        "repos:\n  - path: ./\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/standalone.ts"), "export const S = 0;\n").unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{"name":"cycles-test2","version":"0.0.1"}"#,
    )
    .unwrap();

    let output = Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(dir.path())
        .arg("cycles")
        .arg("--json")
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout)
        .expect("stdout should be valid JSON");
    assert_eq!(v["schema_version"], "ast-outline.cycles.v1");
    assert!(v["cycles"].is_array());
    assert_eq!(v["cycles"].as_array().unwrap().len(), 0);
}
