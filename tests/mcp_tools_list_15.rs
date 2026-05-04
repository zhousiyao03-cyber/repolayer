use serde_json::json;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use tempfile::tempdir;

#[test]
fn mcp_serve_lists_all_15_tools() {
    // Build an index first
    let workspace = tempdir().unwrap();
    let repo_src = std::path::Path::new("tests/fixtures/single_repo_ts");
    let repo_dst = workspace.path().join("single_repo_ts");
    copy_dir_all(repo_src, &repo_dst).unwrap();
    fs::write(
        workspace.path().join("repolayer.yml"),
        format!("repos:\n  - path: {}\n", repo_dst.display()),
    )
    .unwrap();

    assert_cmd::Command::cargo_bin("repolayer")
        .unwrap()
        .current_dir(workspace.path())
        .arg("build")
        .assert()
        .success();

    // Start the MCP server
    let mut child = Command::new(env!("CARGO_BIN_EXE_repolayer"))
        .current_dir(workspace.path())
        .arg("serve")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    // Send initialize
    let init = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "0" }
        }
    });
    writeln!(stdin, "{}", init).unwrap();
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    assert!(line.contains("\"id\":1"), "init response: {}", line);

    // Send notifications/initialized
    let initialized = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    writeln!(stdin, "{}", initialized).unwrap();

    // Send tools/list
    let list = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list"
    });
    writeln!(stdin, "{}", list).unwrap();
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();

    // Parse the tools/list response
    let tool_names: Vec<String> = match serde_json::from_str::<serde_json::Value>(&line) {
        Ok(v) => {
            if let Some(tools) = v
                .get("result")
                .and_then(|r| r.get("tools"))
                .and_then(|t| t.as_array())
            {
                tools
                    .iter()
                    .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(String::from))
                    .collect()
            } else {
                Vec::new()
            }
        }
        Err(_) => Vec::new(),
    };

    let expected = vec![
        // 6 native:
        "find_context",
        "get_symbol",
        "get_callers",
        "get_dependencies",
        "list_repos",
        "find_idl_impl",
        // 9 compat:
        "outline",
        "show",
        "digest",
        "surface",
        "deps",
        "reverse_deps",
        "cycles",
        "search",
        "find_related",
    ];

    assert_eq!(
        tool_names.len(),
        15,
        "expected 15 tools, got {}: {:?}",
        tool_names.len(),
        tool_names
    );

    for name in &expected {
        assert!(
            tool_names.iter().any(|t| t == name),
            "missing tool: {} (got: {:?})",
            name,
            tool_names
        );
    }

    let _ = child.kill();
    let _ = child.wait();
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
