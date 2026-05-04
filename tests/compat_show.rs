use assert_cmd::Command;
use predicates::str::contains;
use std::fs;
use tempfile::tempdir;

// ---------------------------------------------------------------------------
// CLI tests
// ---------------------------------------------------------------------------

#[test]
fn show_command_extracts_function_body() {
    let dir = tempdir().unwrap();
    let f = dir.path().join("foo.rs");
    fs::write(&f, "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n").unwrap();

    Command::cargo_bin("repolayer")
        .unwrap()
        .arg("show")
        .arg(&f)
        .arg("add")
        .assert()
        .success()
        .stdout(contains("a + b"));
}

#[test]
fn show_with_json_emits_schema() {
    let dir = tempdir().unwrap();
    let f = dir.path().join("foo.rs");
    fs::write(&f, "pub fn add() {}\n").unwrap();

    let output = Command::cargo_bin("repolayer")
        .unwrap()
        .arg("show")
        .arg("--json")
        .arg(&f)
        .arg("add")
        .output()
        .unwrap();

    assert!(output.status.success(), "show --json should succeed");
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Must contain the schema identifier
    assert!(
        stdout.contains("ast-outline.show.v1") || stdout.contains("schema"),
        "expected schema field in JSON output, got: {}",
        stdout
    );
}

#[test]
fn show_json_output_is_valid_json() {
    let dir = tempdir().unwrap();
    let f = dir.path().join("math.rs");
    fs::write(
        &f,
        "pub fn multiply(x: i32, y: i32) -> i32 { x * y }\n",
    )
    .unwrap();

    let output = Command::cargo_bin("repolayer")
        .unwrap()
        .arg("show")
        .arg("--json")
        .arg(&f)
        .arg("multiply")
        .output()
        .unwrap();

    assert!(output.status.success(), "show --json should succeed");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("show --json must emit valid JSON");
    assert_eq!(
        parsed["schema"].as_str().unwrap_or(""),
        "ast-outline.show.v1",
        "schema field mismatch: {}",
        stdout
    );
}

#[test]
fn show_unknown_symbol_reports_not_found() {
    let dir = tempdir().unwrap();
    let f = dir.path().join("foo.rs");
    fs::write(&f, "pub fn add() {}\n").unwrap();

    let output = Command::cargo_bin("repolayer")
        .unwrap()
        .arg("show")
        .arg(&f)
        .arg("nonexistent_function")
        .output()
        .unwrap();

    // Exit code 0 — unknown symbols are reported on stderr, not as a failure.
    assert!(
        output.status.success(),
        "show with unknown symbol should still exit 0"
    );
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("not found"),
        "expected 'not found' in stderr, got: {}",
        stderr
    );
}

#[test]
fn show_multiple_symbols_returns_all() {
    let dir = tempdir().unwrap();
    let f = dir.path().join("ops.rs");
    fs::write(
        &f,
        "pub fn add(a: i32, b: i32) -> i32 { a + b }\npub fn sub(a: i32, b: i32) -> i32 { a - b }\n",
    )
    .unwrap();

    let output = Command::cargo_bin("repolayer")
        .unwrap()
        .arg("show")
        .arg(&f)
        .arg("add")
        .arg("sub")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("a + b"), "expected add body: {}", stdout);
    assert!(stdout.contains("a - b"), "expected sub body: {}", stdout);
}

#[test]
fn show_unknown_extension_errors() {
    let dir = tempdir().unwrap();
    let f = dir.path().join("data.xyz");
    fs::write(&f, "some content\n").unwrap();

    let output = Command::cargo_bin("repolayer")
        .unwrap()
        .arg("show")
        .arg(&f)
        .arg("anything")
        .output()
        .unwrap();

    // Should fail — no adapter for unknown extension.
    assert!(
        !output.status.success(),
        "show on unknown file type should fail"
    );
}

#[test]
fn show_suffix_matching_works() {
    let dir = tempdir().unwrap();
    let f = dir.path().join("greet.ts");
    fs::write(
        &f,
        "export function greet(name: string): string { return `Hello ${name}`; }\n",
    )
    .unwrap();

    Command::cargo_bin("repolayer")
        .unwrap()
        .arg("show")
        .arg(&f)
        .arg("greet")
        .assert()
        .success()
        .stdout(contains("Hello"));
}
