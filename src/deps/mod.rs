//! File-level dependency graph (forward + reverse + cycles + DSM).
//! Adopted from aeroxy/ast-outline `src/deps/`. Fully implemented in
//! Tasks B-5 to B-11.

pub mod cache;
pub mod dsm;
pub mod extract;
pub mod graph;
pub mod manifest;
pub mod options;
pub mod render;
pub mod resolver;
pub mod scc;
pub mod traverse;

pub use extract::{extract, RawImport};
pub use graph::{DepEdge, DepGraph, ImportKind};
pub use manifest::{detect_aliases, ProjectAliases, PythonPackage, RustPackage};
pub use options::DepError;
pub use resolver::{build_suffix_index, resolve, ResolveCtx};
