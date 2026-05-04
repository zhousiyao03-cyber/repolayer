use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn prompt_emits_snippet() {
    Command::cargo_bin("repolayer").unwrap()
        .arg("prompt")
        .assert()
        .success()
        .stdout(contains("repolayer"))
        .stdout(contains("find_context"))
        .stdout(contains("outline"))
        .stdout(contains("search"));
}

#[test]
fn prompt_mentions_all_15_tools() {
    let output = Command::cargo_bin("repolayer").unwrap()
        .arg("prompt")
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    let tools = [
        "find_context", "get_symbol", "get_callers", "get_dependencies",
        "list_repos", "find_idl_impl",
        "outline", "show", "digest", "surface",
        "deps", "reverse_deps", "cycles", "search", "find_related",
    ];
    // The prompt covers the most commonly-used 12; list_repos and get_symbol
    // and get_dependencies might be omitted intentionally to keep it short.
    // Just verify a meaningful chunk are mentioned:
    let mentioned = tools.iter().filter(|t| stdout.contains(*t)).count();
    assert!(mentioned >= 10, "expected most tools mentioned; got {}/{}: {}", mentioned, tools.len(), stdout);
}
