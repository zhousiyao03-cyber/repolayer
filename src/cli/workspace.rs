//! Resolve where the index lives for read-only commands.
//!
//! Default: current working directory's `.repolayer/`. Override:
//! `REPOLAYER_INDEX=<dir>` env var. The motivation is that an agent (or a
//! human) often `cd`s into a specific business repo to edit code, but
//! still wants the cross-repo index from a separately-checked-out
//! workspace (e.g. `~/repolayer_ttec/`). Pinning the index location to
//! one env var decouples "where I'm editing" from "where the index lives"
//! without hijacking cwd.
//!
//! Write-side commands (`build`, `update`) deliberately *don't* go through
//! here — they bind to cwd so a stray invocation doesn't write a 100MB
//! index in a surprising place.

use anyhow::{bail, Result};
use std::path::PathBuf;

/// Environment variable name. Documented in SKILL.md.
pub const ENV_VAR: &str = "REPOLAYER_INDEX";

/// Resolve the directory that should contain `.repolayer/<name>.db`. Looks
/// at `$REPOLAYER_INDEX` first, then falls back to `current_dir()`.
///
/// Returns the *workspace root* (the directory **containing** `.repolayer/`),
/// not `.repolayer/` itself — callers join the per-store filename.
pub fn resolve_workspace() -> Result<PathBuf> {
    if let Ok(raw) = std::env::var(ENV_VAR) {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            let p = PathBuf::from(trimmed);
            if !p.is_dir() {
                bail!(
                    "{} = '{}' is not an existing directory. Unset it or point it at a workspace that contains a .repolayer/ directory.",
                    ENV_VAR,
                    trimmed,
                );
            }
            return Ok(p);
        }
    }
    Ok(std::env::current_dir()?)
}

/// Convenience: resolve workspace and join `.repolayer/<store>` in one shot.
/// Used by query, search, find-related, view — every read-only command.
pub fn store_path(store_filename: &str) -> Result<PathBuf> {
    Ok(resolve_workspace()?.join(".repolayer").join(store_filename))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Env vars are process-global, so tests that mutate $REPOLAYER_INDEX
    // need to run serially. cargo runs tests in parallel by default.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn env_unset_falls_back_to_cwd() {
        let _g = ENV_LOCK.lock().unwrap();
        // SAFETY: tests run under ENV_LOCK; concurrent FFI getenv is fine
        // for the short window inside resolve_workspace().
        unsafe {
            std::env::remove_var(ENV_VAR);
        }
        let ws = resolve_workspace().unwrap();
        assert_eq!(ws, std::env::current_dir().unwrap());
    }

    #[test]
    fn env_pointing_at_dir_is_used() {
        let _g = ENV_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var(ENV_VAR, tmp.path());
        }
        let ws = resolve_workspace().unwrap();
        unsafe {
            std::env::remove_var(ENV_VAR);
        }
        assert_eq!(
            ws.canonicalize().unwrap(),
            tmp.path().canonicalize().unwrap()
        );
    }

    #[test]
    fn env_pointing_at_nonexistent_path_errors() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var(ENV_VAR, "/no/such/path/repolayer_test_xyz");
        }
        let res = resolve_workspace();
        unsafe {
            std::env::remove_var(ENV_VAR);
        }
        assert!(res.is_err(), "should error when env points nowhere");
        let msg = res.unwrap_err().to_string();
        assert!(
            msg.contains("REPOLAYER_INDEX"),
            "msg should name the env var: {msg}"
        );
    }

    #[test]
    fn empty_env_treated_as_unset() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var(ENV_VAR, "  ");
        }
        let ws = resolve_workspace().unwrap();
        unsafe {
            std::env::remove_var(ENV_VAR);
        }
        // Falls back to cwd, no error.
        assert_eq!(ws, std::env::current_dir().unwrap());
    }
}
