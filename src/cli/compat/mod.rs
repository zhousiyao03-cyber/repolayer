//! ast-outline-compat subcommands. These mirror the 9 ast-outline tools
//! (outline, show, digest, surface, deps, reverse-deps, cycles, search,
//! find-related) so repolayer is a drop-in superset.

pub mod cycles;
pub mod deps;
pub mod digest;
pub mod find_related;
pub mod outline;
pub mod reverse_deps;
pub mod search;
pub mod show;
pub mod surface;

use anyhow::Result;
use std::path::Path;

/// Load (or build on the fly) a [`crate::deps::DepGraph`] for the given workspace.
///
/// The simpler "always build" path is used for Plan C; cache lookup via
/// `.repolayer/deps.db` can be added in a later iteration.
pub(crate) fn load_or_build_dep_graph(workspace_root: &Path) -> Result<crate::deps::DepGraph> {
    crate::deps::build_for_repo(workspace_root).map_err(Into::into)
}
