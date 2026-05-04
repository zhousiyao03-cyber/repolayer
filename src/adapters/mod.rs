//! Source-language adapters built on `ast-grep-core`.
//!
//! Each adapter implements [`base::LanguageAdapter`] for one language
//! family. IDL parsers under [`idl`] use bare tree-sitter and emit a
//! different output shape (services/methods rather than Declarations);
//! they are dispatched separately by the indexer.

pub mod base;
pub mod python;
pub mod typescript;
pub mod go;
pub mod rust;
pub mod csharp;
// Adapters land here in tasks 3-11:
// pub mod rust;
// pub mod csharp;
// pub mod java;
// pub mod kotlin;
// pub mod scala;
// pub mod markdown;

// IDL adapters move from src/parser/idl/ in Task 13:
// pub mod idl;
