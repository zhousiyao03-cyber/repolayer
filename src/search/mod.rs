//! Hybrid BM25 + dense embedding search index.
//! Adopted from aeroxy/ast-outline `src/search/`. Fully implemented in
//! Tasks B-12 to B-20.

pub mod bm25;
pub mod cache;
pub mod chunker;
pub mod download;
pub mod embed;
pub mod embedder;
pub mod format;
pub mod fusion;
pub mod http_embedder;
pub mod index; // temp stub; full impl in B-20
pub mod ollama;
pub mod ranking;
pub mod store;
pub mod store_summary;
pub mod tokens;
