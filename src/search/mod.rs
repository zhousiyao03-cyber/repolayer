//! Hybrid BM25 + dense embedding search index.
//! Adopted from aeroxy/ast-outline `src/search/`. Fully implemented in
//! Tasks B-12 to B-20.

pub mod bm25;
pub mod cache;
pub mod chunker;
pub mod download;
pub mod embed;
pub mod index; // temp stub; full impl in B-20
pub mod tokens;
pub mod format;
pub mod ranking;
pub mod fusion;
