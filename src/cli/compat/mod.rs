//! ast-outline-compat subcommands. These mirror the 9 ast-outline tools
//! (outline, show, digest, surface, deps, reverse-deps, cycles, search,
//! find-related) so repolayer is a drop-in superset.

pub mod digest;
pub mod outline;
pub mod show;
// ... additional compat subcommands added per task (C-6 through C-12)
