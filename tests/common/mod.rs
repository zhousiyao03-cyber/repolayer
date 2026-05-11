//! Shared test helpers.
//!
//! Integration tests in this crate spawn the real `repolayer` binary via
//! `assert_cmd`. Every such invocation must shed the user's ambient
//! `REPOLAYER_INDEX` environment variable, otherwise a developer running
//! `cargo test` locally with that variable set will have read-only
//! commands resolve against the user's global index instead of the per-test
//! temp workspace — producing confusing failures unrelated to the change
//! under test.
//!
//! Use `repolayer_cmd()` everywhere instead of bare `Command::cargo_bin`.

#![allow(dead_code)]

use assert_cmd::Command;

/// Build a `Command` for the `repolayer` test binary with `REPOLAYER_INDEX`
/// stripped from the environment. Always prefer this over
/// `Command::cargo_bin("repolayer")` directly.
pub fn repolayer_cmd() -> Command {
    let mut cmd = Command::cargo_bin("repolayer").expect("repolayer binary must be built");
    cmd.env_remove("REPOLAYER_INDEX");
    cmd
}
