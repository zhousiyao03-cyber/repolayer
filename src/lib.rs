pub mod adapters;
pub mod cli;
pub mod config;
pub mod core;
pub mod deps;
pub mod file_filter;
pub mod graph;
pub mod indexer;
pub mod linker;
pub mod llm;
pub mod outline;
pub mod query;
pub mod search;
pub mod surface;

// Re-export helpers used by surface/ language resolvers.
pub use adapters::parse_file;
pub use adapters::walk_and_parse;
