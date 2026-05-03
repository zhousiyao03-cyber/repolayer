use anyhow::Result;
use clap::Subcommand;

pub mod build;
pub mod init;

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
}

pub async fn run(cmd: Command) -> Result<()> {
    match cmd {
        Command::Init => init::run().await,
        Command::Build => build::run().await,
        Command::Update => anyhow::bail!("not implemented yet"),
        Command::Query { .. } => anyhow::bail!("not implemented yet"),
        Command::Callers { .. } => anyhow::bail!("not implemented yet"),
        Command::Serve { .. } => anyhow::bail!("not implemented yet"),
    }
}
