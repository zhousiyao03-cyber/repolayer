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
    /// Print the published public API of a package (follows pub use / __all__ / barrel files)
    Surface {
        /// Path to the package root (auto-detect manifest: Cargo.toml, pyproject.toml, package.json, __init__.py)
        path: Option<PathBuf>,
        /// Emit JSON instead of human-readable text
        #[arg(long)]
        json: bool,
    },
    /// Show forward dependencies of a file (what does X import)
    Deps {
        /// File or directory to query
        path: PathBuf,
        /// Maximum BFS depth
        #[arg(long, default_value_t = 1)]
        depth: usize,
        /// Emit JSON instead of human-readable text
        #[arg(long)]
        json: bool,
    },
    /// Show reverse dependencies of a file (who imports X)
    #[command(name = "reverse-deps")]
    ReverseDeps {
        /// File to query
        path: PathBuf,
        /// Emit JSON instead of human-readable text
        #[arg(long)]
        json: bool,
    },
    /// Find import cycles via Tarjan SCC (exits with code 1 if any cycle found)
    Cycles {
        /// Workspace root (defaults to current directory)
        path: Option<PathBuf>,
        /// Emit JSON instead of human-readable text
        #[arg(long)]
        json: bool,
    },
    /// Hybrid BM25 + semantic search across the indexed workspace
    Search {
        /// Query string (natural language or symbol name)
        query: String,
        /// Number of results to return
        #[arg(short, long, default_value_t = 10)]
        k: usize,
        /// Emit JSON instead of human-readable text
        #[arg(long)]
        json: bool,
    },
    /// Find code chunks structurally similar to a given file:line
    #[command(name = "find-related")]
    FindRelated {
        /// Target as "path/to/file.rs:42" (line number identifies the chunk)
        spec: String,
        /// Number of results to return
        #[arg(short, long, default_value_t = 5)]
        k: usize,
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
        Command::Surface { path, json } => {
            let p = path.unwrap_or_else(|| PathBuf::from("."));
            compat::surface::run(p, json).await
        }
        Command::Deps { path, depth, json } => compat::deps::run(path, depth, json).await,
        Command::ReverseDeps { path, json } => compat::reverse_deps::run(path, json).await,
        Command::Cycles { path, json } => compat::cycles::run(path, json).await,
        Command::Search { query, k, json } => compat::search::run(query, k, json).await,
        Command::FindRelated { spec, k, json } => compat::find_related::run(spec, k, json).await,
    }
}
