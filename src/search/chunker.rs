//! Temp stub for B-12. Full impl in B-13 (Adopt search/chunker.rs).
//!
//! Re-exports `Chunk` from `search::index` to avoid duplication.
//! Replace entirely when B-13 is executed.

#![allow(dead_code)]

pub use crate::search::index::Chunk;
