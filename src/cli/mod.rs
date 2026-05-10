use anyhow::Result;
use clap::Subcommand;
use std::path::PathBuf;

pub mod build;
pub mod callers;
pub mod compat;
pub mod find_idl_impl;
pub mod init;
pub mod install;
pub mod query;
pub mod repo_filter;
pub mod update;
pub mod view;
pub mod workspace;

#[derive(Subcommand)]
pub enum Command {
    /// Initialize a repolayer.yml in current directory
    Init,
    /// Build the graph from scratch
    Build,
    /// Incrementally update graph based on git diff
    Update,
    /// Query the graph for declarations whose symbol contains <text>
    Query {
        /// Substring to match against declaration symbols
        text: String,
        /// Restrict matches to a single repo (must match a name in repolayer.yml)
        #[arg(long)]
        repo: Option<String>,
        /// Emit JSON instead of human-readable text
        #[arg(long)]
        json: bool,
    },
    /// Find inbound `Calls` edges for a symbol — i.e. who calls it.
    /// Aggregates across all definitions of the exact name (handy when a
    /// symbol like `init` is defined in multiple repos).
    Callers {
        /// Exact symbol name (no substring match — use `query` first if unsure)
        symbol: String,
        /// BFS depth over the Calls edge (1 = direct callers only)
        #[arg(long, default_value_t = 1)]
        depth: usize,
        /// Restrict definitions to a single repo (must match a name in repolayer.yml)
        #[arg(long)]
        repo: Option<String>,
        /// Emit JSON instead of human-readable text
        #[arg(long)]
        json: bool,
    },
    /// Resolve an IDL method to its server implementations (Implements) and
    /// client invocations (Invokes) across all indexed repos. Output is
    /// sorted by edge confidence — higher means stronger evidence.
    #[command(name = "find-idl-impl")]
    FindIdlImpl {
        /// IDL method name (e.g. `GetMember`)
        method: String,
        /// Disambiguate by IDL service name (e.g. `MemberService`)
        #[arg(long)]
        service: Option<String>,
        /// Skip Implements (server-side) edges
        #[arg(long)]
        no_implements: bool,
        /// Skip Invokes (client-side) edges
        #[arg(long)]
        no_invokes: bool,
        /// Emit JSON instead of human-readable text
        #[arg(long)]
        json: bool,
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
        /// Restrict to a single repo (must match a name in repolayer.yml).
        /// BM25 IDF is computed over just that repo, so common workspace
        /// terms aren't penalised inside the repo.
        #[arg(long)]
        repo: Option<String>,
        /// Emit JSON instead of human-readable text
        #[arg(long)]
        json: bool,
        /// Include the full chunk body in JSON output (default: short preview only).
        /// Hits already include path:line_range — fetch bodies with `repolayer show`.
        #[arg(long)]
        full_content: bool,
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
    /// Install the repolayer skill into an AI agent's skills directory
    Install {
        /// Agent name (claude-code)
        #[arg(long)]
        skill: String,
    },
    /// Export the indices to a self-contained HTML viewer
    View {
        /// Output directory (created if missing)
        #[arg(long)]
        out: PathBuf,
        /// Limit export to a single repo by name (default: all repos)
        #[arg(long)]
        repo: Option<String>,
    },
}

pub async fn run(cmd: Command) -> Result<()> {
    match cmd {
        Command::Init => init::run().await,
        Command::Build => build::run().await,
        Command::Update => update::run().await,
        Command::Query { text, repo, json } => query::run(text, repo, json).await,
        Command::Callers {
            symbol,
            depth,
            repo,
            json,
        } => callers::run(symbol, depth, repo, json).await,
        Command::FindIdlImpl {
            method,
            service,
            no_implements,
            no_invokes,
            json,
        } => find_idl_impl::run(method, service, no_implements, no_invokes, json).await,
        Command::Outline { paths, json } => compat::outline::run(paths, json).await,
        Command::Show {
            file,
            symbols,
            json,
        } => compat::show::run(file, symbols, json).await,
        Command::Digest { paths, json } => compat::digest::run(paths, json).await,
        Command::Surface { path, json } => {
            let p = path.unwrap_or_else(|| PathBuf::from("."));
            compat::surface::run(p, json).await
        }
        Command::Deps { path, depth, json } => compat::deps::run(path, depth, json).await,
        Command::ReverseDeps { path, json } => compat::reverse_deps::run(path, json).await,
        Command::Cycles { path, json } => compat::cycles::run(path, json).await,
        Command::Search {
            query,
            k,
            repo,
            json,
            full_content,
        } => compat::search::run(query, k, repo, json, full_content).await,
        Command::FindRelated { spec, k, json } => compat::find_related::run(spec, k, json).await,
        Command::Install { skill } => install::run(&skill).await,
        Command::View { out, repo } => view::run(out, repo).await,
    }
}
