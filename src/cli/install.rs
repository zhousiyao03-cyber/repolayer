use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};

const SKILL_MD: &str = include_str!("../../skills/repolayer.md");

pub async fn run(agent: &str) -> Result<()> {
    let dest = skill_path_for(agent)?;
    install_skill(&dest)
}

/// Claude Code requires skills to live as `<skills_dir>/<name>/SKILL.md`,
/// not `<skills_dir>/<name>.md`. Single-file form is silently ignored
/// by the loader, which means the skill effectively isn't installed.
fn skill_path_for(agent: &str) -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("no home dir"))?;
    match agent {
        "claude-code" => Ok(home
            .join(".claude")
            .join("skills")
            .join("repolayer")
            .join("SKILL.md")),
        other => Err(anyhow!(
            "unknown / unsupported agent: {} (supported: claude-code)",
            other
        )),
    }
}

fn install_skill(dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow!("create {}: {}", parent.display(), e))?;
    }

    // Migrate from old single-file location if it exists.
    let parent = dest.parent().and_then(|p| p.parent()).unwrap_or(dest);
    let legacy = parent.join("repolayer.md");
    if legacy.exists() && !dest.exists() {
        std::fs::rename(&legacy, dest)
            .map_err(|e| anyhow!("migrate {}: {}", legacy.display(), e))?;
        println!("migrated {} → {}", legacy.display(), dest.display());
    } else if legacy.exists() {
        // Both exist: keep new dir form, archive legacy.
        let backup = legacy.with_extension(format!(
            "md.legacy.{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        ));
        std::fs::rename(&legacy, &backup).ok();
        println!("archived legacy {} → {}", legacy.display(), backup.display());
    }

    if dest.exists() {
        let existing = std::fs::read_to_string(dest)
            .map_err(|e| anyhow!("read {}: {}", dest.display(), e))?;
        if existing == SKILL_MD {
            println!("skill already up to date at {}", dest.display());
            return Ok(());
        }
        let backup = dest.with_extension(format!(
            "md.bak.{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        ));
        std::fs::copy(dest, &backup)
            .map_err(|e| anyhow!("backup {}: {}", dest.display(), e))?;
        println!("backed up existing skill to {}", backup.display());
    }

    std::fs::write(dest, SKILL_MD).map_err(|e| anyhow!("write {}: {}", dest.display(), e))?;
    println!("installed skill at {}", dest.display());
    Ok(())
}
