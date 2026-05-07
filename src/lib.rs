pub mod core;
pub mod adapters;
pub mod file_filter;
pub mod outline;
pub mod surface;
pub mod deps;
pub mod search;
pub mod cli;
pub mod config;
pub mod graph;
pub mod indexer;
pub mod linker;
pub mod llm;
pub mod query;

// Re-export helpers used by surface/ language resolvers.
pub use adapters::parse_file;
pub use adapters::walk_and_parse;
