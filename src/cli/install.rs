use anyhow::{anyhow, Result};
use std::path::PathBuf;

pub async fn run(agent: &str) -> Result<()> {
    let exe_path = std::env::current_exe()
        .map_err(|e| anyhow!("cannot determine repolayer binary path: {}", e))?;

    match agent {
        "claude-code" => install_claude_code(&exe_path),
        "cursor" => install_cursor(&exe_path),
        "gemini" => install_gemini(&exe_path),
        "codex" => install_codex(&exe_path),
        "copilot" => install_copilot(&exe_path),
        other => Err(anyhow!(
            "unknown agent: {} (try claude-code / cursor / gemini / codex / copilot)",
            other
        )),
    }
}

fn install_claude_code(exe: &std::path::Path) -> Result<()> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("no home dir"))?;
    let config_path = if cfg!(target_os = "macos") {
        home.join("Library")
            .join("Application Support")
            .join("Claude")
            .join("claude_desktop_config.json")
    } else {
        home.join(".config")
            .join("Claude")
            .join("claude_desktop_config.json")
    };

    let workspace = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    write_mcp_entry(
        &config_path,
        "mcpServers",
        "repolayer",
        &serde_json::json!({
            "command": exe.to_string_lossy(),
            "args": ["serve"],
            "cwd": workspace.to_string_lossy(),
        }),
    )
}

fn install_cursor(exe: &std::path::Path) -> Result<()> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("no home dir"))?;
    let config_path = home.join(".cursor").join("mcp.json");
    let workspace = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    write_mcp_entry(
        &config_path,
        "mcpServers",
        "repolayer",
        &serde_json::json!({
            "command": exe.to_string_lossy(),
            "args": ["serve"],
            "cwd": workspace.to_string_lossy(),
        }),
    )
}

fn install_gemini(exe: &std::path::Path) -> Result<()> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("no home dir"))?;
    let config_path = home
        .join(".config")
        .join("gemini-cli")
        .join("config.json");
    let workspace = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    write_mcp_entry(
        &config_path,
        "mcpServers",
        "repolayer",
        &serde_json::json!({
            "command": exe.to_string_lossy(),
            "args": ["serve"],
            "cwd": workspace.to_string_lossy(),
        }),
    )
}

fn install_codex(exe: &std::path::Path) -> Result<()> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("no home dir"))?;
    let config_path = home.join(".config").join("codex").join("mcp.json");
    let workspace = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    write_mcp_entry(
        &config_path,
        "mcpServers",
        "repolayer",
        &serde_json::json!({
            "command": exe.to_string_lossy(),
            "args": ["serve"],
            "cwd": workspace.to_string_lossy(),
        }),
    )
}

fn install_copilot(_exe: &std::path::Path) -> Result<()> {
    eprintln!("VS Code Copilot MCP support varies by version.");
    eprintln!("Add manually to settings.json under 'github.copilot.mcp.servers'.");
    Ok(())
}

/// Common helper: read existing JSON, add or replace an entry under `top_key.<server_name>`,
/// back up the original, and write back pretty-printed.
fn write_mcp_entry(
    config_path: &std::path::Path,
    top_key: &str,
    server_name: &str,
    server_config: &serde_json::Value,
) -> Result<()> {
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Read existing or start fresh
    let existing: serde_json::Value = if config_path.exists() {
        let content = std::fs::read_to_string(config_path)
            .map_err(|e| anyhow!("read {}: {}", config_path.display(), e))?;
        serde_json::from_str(&content)
            .map_err(|e| anyhow!("parse {}: {}", config_path.display(), e))?
    } else {
        serde_json::json!({})
    };

    // Backup
    if config_path.exists() {
        let backup_path = config_path.with_extension(format!(
            "json.bak.{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        ));
        std::fs::copy(config_path, &backup_path).ok();
    }

    // Mutate: existing[top_key][server_name] = server_config
    let mut root = match existing {
        serde_json::Value::Object(m) => m,
        _ => serde_json::Map::new(),
    };
    let top_entry = root
        .entry(top_key.to_string())
        .or_insert(serde_json::json!({}));
    if let Some(top_map) = top_entry.as_object_mut() {
        top_map.insert(server_name.to_string(), server_config.clone());
    } else {
        return Err(anyhow!("{} is not a JSON object", top_key));
    }

    let updated = serde_json::Value::Object(root);
    let pretty = serde_json::to_string_pretty(&updated)?;
    std::fs::write(config_path, pretty)?;

    println!("wrote {} entry to {}", server_name, config_path.display());
    Ok(())
}
