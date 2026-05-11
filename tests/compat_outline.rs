use predicates::str::contains;
use std::fs;
use tempfile::tempdir;

#[path = "common/mod.rs"]
mod common;
use common::repolayer_cmd;

// ---------------------------------------------------------------------------
// CLI tests
// ---------------------------------------------------------------------------

#[test]
fn outline_command_emits_signatures_for_rust_file() {
    let dir = tempdir().unwrap();
    let f = dir.path().join("foo.rs");
    fs::write(&f, "pub fn add(a: i32, b: i32) -> i32 { a + b }\n").unwrap();

    repolayer_cmd()
        .arg("outline")
        .arg(&f)
        .assert()
        .success()
        // Header line includes the filename
        .stdout(contains("foo.rs"));
}

#[test]
fn outline_command_shows_function_signature() {
    let dir = tempdir().unwrap();
    let f = dir.path().join("math.rs");
    fs::write(&f, "pub fn multiply(x: i32, y: i32) -> i32 { x * y }\n").unwrap();

    let output = repolayer_cmd().arg("outline").arg(&f).output().unwrap();

    assert!(output.status.success(), "outline command should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    // The outline text output must include the function name.
    assert!(
        stdout.contains("multiply"),
        "expected 'multiply' in outline output: {}",
        stdout
    );
}

#[test]
fn outline_json_flag_emits_valid_json() {
    let dir = tempdir().unwrap();
    let f = dir.path().join("foo.rs");
    fs::write(&f, "pub fn add() -> i32 { 0 }\n").unwrap();

    let output = repolayer_cmd()
        .arg("outline")
        .arg("--json")
        .arg(&f)
        .output()
        .unwrap();

    assert!(output.status.success(), "outline --json should succeed");
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Must be parseable as JSON
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("outline --json must emit valid JSON");
    // Schema field must be present
    assert_eq!(
        parsed["schema"].as_str().unwrap_or(""),
        "ast-outline.outline.v1",
        "expected schema field: {}",
        stdout
    );
    // files array must exist
    assert!(
        parsed["files"].is_array(),
        "expected 'files' array in JSON output: {}",
        stdout
    );
}

#[test]
fn outline_json_output_contains_language_field() {
    let dir = tempdir().unwrap();
    let f = dir.path().join("bar.rs");
    fs::write(&f, "pub struct Foo { x: i32 }\n").unwrap();

    let output = repolayer_cmd()
        .arg("outline")
        .arg("--json")
        .arg(&f)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("\"language\""),
        "expected 'language' field in JSON output: {}",
        stdout
    );
}

#[test]
fn outline_directory_walks_all_supported_files() {
    let dir = tempdir().unwrap();
    let subdir = dir.path().join("src");
    fs::create_dir_all(&subdir).unwrap();
    fs::write(subdir.join("a.rs"), "pub fn a() {}\n").unwrap();
    fs::write(subdir.join("b.rs"), "pub fn b() {}\n").unwrap();

    let output = repolayer_cmd()
        .arg("outline")
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success(), "directory outline should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Both files must appear in the output
    assert!(stdout.contains("a.rs"), "a.rs not in output: {}", stdout);
    assert!(stdout.contains("b.rs"), "b.rs not in output: {}", stdout);
}

#[test]
fn outline_unknown_extension_warns_but_succeeds() {
    let dir = tempdir().unwrap();
    let f = dir.path().join("data.xyz");
    fs::write(&f, "some unknown content\n").unwrap();

    repolayer_cmd()
        .arg("outline")
        .arg(&f)
        .assert()
        // Should exit 0 — unknown files are silently skipped (warning to stderr)
        .success();
}

#[test]
fn outline_typescript_file_includes_function() {
    let dir = tempdir().unwrap();
    let f = dir.path().join("greet.ts");
    fs::write(
        &f,
        "export function greet(name: string): string { return `Hello ${name}`; }\n",
    )
    .unwrap();

    let output = repolayer_cmd().arg("outline").arg(&f).output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("greet"),
        "expected 'greet' function in outline: {}",
        stdout
    );
}
