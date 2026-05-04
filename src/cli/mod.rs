use anyhow::Result;
use clap::Subcommand;
use std::path::PathBuf;

pub mod build;
pub mod compat;
pub mod init;
pub mod query;
pub mod serve;
pub mod update;

#[derive(Subcommand)]
pub enum Command {
    /// Initialize a repolayer.yml in current directory
    Init,
    /// Build the graph from scratch
    Build,
    /// Incrementally update graph based on git diff
    Update,
    /// Query the graph from CLI (debug)
    Query { text: String },
    /// Find callers of a symbol
    Callers { symbol: String },
    /// Start MCP server (stdio by default)
    Serve {
        /// Listen on HTTP instead of stdio
        #[arg(long)]
        http: Option<String>,
    },
    /// Print structural outline of source files (signatures, line ranges, no method bodies)
    Outline {
        /// Files or directories to outline
        paths: Vec<PathBuf>,
        /// Emit JSON instead of human-readable text
        #[arg(long)]
        json: bool,
    },
    /// Print source body of one or more symbols from a file
    Show {
        /// Source file to extract from
        file: PathBuf,
        /// Symbol names (suffix-matching; pass multiple to fetch several)
        symbols: Vec<String>,
        /// Emit JSON instead of source code
        #[arg(long)]
        json: bool,
    },
    /// Compact public API map of a module (one-page overview, signatures only)
    Digest {
        /// Files or directories to digest
        paths: Vec<PathBuf>,
        /// Emit JSON instead of human-readable text
        #[arg(long)]
        json: bool,
    },
}

pub async fn run(cmd: Command) -> Result<()> {
    match cmd {
        Command::Init => init::run().await,
        Command::Build => build::run().await,
        Command::Update => update::run().await,
        Command::Query { text } => query::run(text).await,
        Command::Callers { .. } => anyhow::bail!("not implemented yet"),
        Command::Serve { http } => serve::run(http).await,
        Command::Outline { paths, json } => compat::outline::run(paths, json).await,
        Command::Show { file, symbols, json } => compat::show::run(file, symbols, json).await,
        Command::Digest { paths, json } => compat::digest::run(paths, json).await,
    }
}
