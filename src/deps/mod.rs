//! File-level dependency graph (forward + reverse + cycles + DSM).
//! Adopted from aeroxy/ast-outline `src/deps/`. Fully implemented in
//! Tasks B-5 to B-11.

pub mod extract;
pub mod graph;
pub mod manifest;
pub mod options;
pub mod resolver;

pub use extract::{extract, RawImport};
pub use graph::{DepEdge, DepGraph, ImportKind};
pub use manifest::{detect_aliases, ProjectAliases, PythonPackage, RustPackage};
pub use options::DepError;
pub use resolver::{build_suffix_index, resolve, ResolveCtx};
