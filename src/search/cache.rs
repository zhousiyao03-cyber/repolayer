//! Temp stub for B-9. Full impl in B-18 (Adopt search/cache.rs).
//!
//! Provides the minimal `FileRecord` / `Delta` / `compute_delta` surface
//! required by `deps::cache`. Replace entirely when B-18 is executed.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Per-file fingerprint used for freshness detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRecord {
    pub path: String,
    pub mtime_ns: i128,
    pub size: u64,
    pub content_hash: u64,
    pub chunk_start: u32,
    pub chunk_end: u32,
}

/// Result of comparing stored fingerprints against the current filesystem.
#[derive(Default)]
pub struct Delta {
    pub added: Vec<std::path::PathBuf>,
    pub removed: Vec<std::path::PathBuf>,
    pub modified: Vec<std::path::PathBuf>,
}

impl Delta {
    /// True when any file change means the cache must be rebuilt.
    pub fn requires_rebuild(&self) -> bool {
        !self.added.is_empty() || !self.removed.is_empty() || !self.modified.is_empty()
    }
}

/// Stub: always returns an empty delta (no rebuild required) until B-18
/// replaces this with real mtime/hash comparison logic.
pub fn compute_delta(_root: &Path, _records: &[FileRecord]) -> Delta {
    Delta::default()
}

/// Hash a file by path (stub — returns 0 until B-18).
pub fn hash_file(_p: &Path) -> std::io::Result<u64> {
    Ok(0)
}
