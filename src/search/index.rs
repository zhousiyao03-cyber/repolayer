//! Temp stub for B-12. Full impl in B-20 (Adopt search/index.rs).
//!
//! Provides the minimal `Meta`, `SearchHit`, `ModelMeta` surface
//! required by `search::format`. Replace entirely when B-20 is executed.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// Search hit result.
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub chunk: Chunk,
    pub score: f32,
}

/// A chunk of source code.
#[derive(Debug, Clone)]
pub struct Chunk {
    pub content: String,
    pub file_path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub start_byte: u32,
    pub end_byte: u32,
    pub language: String,
}

/// Model metadata for the search index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMeta {
    pub id: String,
    pub dim: usize,
}

/// Search index metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meta {
    pub schema: String,
    pub ast_outline_version: String,
    pub model: ModelMeta,
    pub created_unix: u64,
    pub chunk_count: u32,
    pub embedding_dtype: String,
    pub tombstones: Vec<String>,
}
