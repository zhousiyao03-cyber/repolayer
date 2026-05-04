//! File-level dependency graph (forward + reverse + cycles + DSM).
//! Adopted from aeroxy/ast-outline `src/deps/`. Fully implemented in
//! Tasks B-5 to B-11.

pub mod extract;
pub mod graph;
pub mod resolver;

pub use extract::{extract, RawImport};
pub use graph::ImportKind;
