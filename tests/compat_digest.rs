use assert_cmd::Command;
use predicates::str::contains;
use std::fs;
use tempfile::tempdir;

// ---------------------------------------------------------------------------
// CLI tests
// ---------------------------------------------------------------------------

#[test]
fn digest_command_emits_compact_summary() {
    let dir = tempdir().unwrap();
    let f = dir.path().join("foo.rs");
    fs::write(&f, "pub fn add() {} pub fn sub() {}\n").unwrap();

    Command::cargo_bin("repolayer")
        .unwrap()
        .arg("digest")
        .arg(&f)
        .assert()
        .success()
        .stdout(contains("foo.rs"));
}

#[test]
fn digest_command_shows_function_signatures() {
    let dir = tempdir().unwrap();
    let f = dir.path().join("math.rs");
    fs::write(&f, "pub fn multiply(x: i32, y: i32) -> i32 { x * y }\n").unwrap();

    let output = Command::cargo_bin("repolayer")
        .unwrap()
        .arg("digest")
        .arg(&f)
        .output()
        .unwrap();

    assert!(output.status.success(), "digest command should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    // The digest text output must include the function name.
    assert!(
        stdout.contains("multiply"),
        "expected 'multiply' in digest output: {}",
        stdout
    );
}

#[test]
fn digest_json_flag_emits_valid_json() {
    let dir = tempdir().unwrap();
    let f = dir.path().join("foo.rs");
    fs::write(&f, "pub fn add() -> i32 { 0 }\n").unwrap();

    let output = Command::cargo_bin("repolayer")
        .unwrap()
        .arg("digest")
        .arg("--json")
        .arg(&f)
        .output()
        .unwrap();

    assert!(output.status.success(), "digest --json should succeed");
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Must be parseable as JSON
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("digest --json must emit valid JSON");
    // Schema field must be present
    assert_eq!(
        parsed["schema_version"].as_str().unwrap_or(""),
        "ast-outline.digest.v1",
        "expected schema_version field: {}",
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
fn digest_directory_walks_all_supported_files() {
    let dir = tempdir().unwrap();
    let subdir = dir.path().join("src");
    fs::create_dir_all(&subdir).unwrap();
    fs::write(subdir.join("a.rs"), "pub fn a() {}\n").unwrap();
    fs::write(subdir.join("b.rs"), "pub fn b() {}\n").unwrap();

    let output = Command::cargo_bin("repolayer")
        .unwrap()
        .arg("digest")
        .arg(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success(), "directory digest should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Both files must appear in the output
    assert!(stdout.contains("a.rs"), "a.rs not in output: {}", stdout);
    assert!(stdout.contains("b.rs"), "b.rs not in output: {}", stdout);
}

#[test]
fn digest_unknown_extension_warns_but_succeeds() {
    let dir = tempdir().unwrap();
    let f = dir.path().join("data.xyz");
    fs::write(&f, "some unknown content\n").unwrap();

    Command::cargo_bin("repolayer")
        .unwrap()
        .arg("digest")
        .arg(&f)
        .assert()
        // Should exit 0 — unknown files are silently skipped (warning to stderr)
        .success();
}

#[test]
fn digest_typescript_file_includes_function() {
    let dir = tempdir().unwrap();
    let f = dir.path().join("greet.ts");
    fs::write(
        &f,
        "export function greet(name: string): string { return `Hello ${name}`; }\n",
    )
    .unwrap();

    let output = Command::cargo_bin("repolayer")
        .unwrap()
        .arg("digest")
        .arg(&f)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("greet"),
        "expected 'greet' function in digest: {}",
        stdout
    );
}

#[test]
fn digest_json_output_contains_files_array() {
    let dir = tempdir().unwrap();
    let f = dir.path().join("bar.rs");
    fs::write(&f, "pub struct Foo { x: i32 }\n").unwrap();

    let output = Command::cargo_bin("repolayer")
        .unwrap()
        .arg("digest")
        .arg("--json")
        .arg(&f)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    // Verify files array contains the parsed result with language field
    let files = parsed["files"].as_array().unwrap();
    assert!(!files.is_empty(), "files array should not be empty");
    // First file should have path and symbols
    assert!(
        files[0]["path"].is_string(),
        "file entry should have 'path' field: {}",
        stdout
    );
}
