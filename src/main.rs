use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "repolayer", version, about = "Cross-repo agent index layer")]
struct Cli {
    #[command(subcommand)]
    cmd: repolayer::cli::Command,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Always write tracing logs to stderr so stdout remains clean for stdio
    // MCP transport (the `serve` subcommand communicates over stdout/stdin).
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    repolayer::cli::run(cli.cmd).await
}
