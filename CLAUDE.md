# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

`repolayer` is a Rust CLI + MCP server that pre-computes a SQLite-backed code graph (modules, symbols, imports, IDL services) across one or more repos and serves it to AI agents (Claude Code, Cursor, …) via the [Model Context Protocol](https://modelcontextprotocol.io/). Single static binary, tree-sitter parsing, no runtime deps for end users.

Status: pre-alpha (`v0.0.2-alpha`). Phase 2 done (multi-repo, IDL, MCP, incremental). Phase 3 in progress (LLM enhancement).

## Common commands

```bash
# Build / test
cargo build                                  # debug build
cargo build --release                        # release (LTO, stripped)
cargo test                                   # run all tests
cargo test --test cli_build                  # run a single integration test file
cargo test parser_typescript::                # run all tests in a module by name prefix
cargo test -- --nocapture                    # show test stdout/stderr
cargo clippy --all-targets -- -D warnings    # lint
cargo fmt                                    # format

# End-to-end usage of the binary itself
cargo run -- init                            # creates repolayer.yml in CWD
cargo run -- build                           # full index → .repolayer/index.db
cargo run -- update                          # incremental re-index via `git diff`
cargo run -- query "auth"                    # substring symbol search (debug)
cargo run -- serve                           # start MCP stdio server (logs to stderr)
```

The `serve` subcommand uses **stdio** for MCP — stdout is reserved for the protocol, so all `tracing` output is forced to stderr (see `src/main.rs`). Don't add `println!` to anything reachable from the serve path. The `--http` flag exists in the CLI but currently bails with "not implemented yet".

## Architecture

Pipeline (build): config → walk repos → tree-sitter parse → upsert nodes/edges into SQLite → cross-repo import resolution → IDL linking → manual links → optional LLM summaries.

Module map (`src/`):

- `cli/` — clap subcommands. `Command` enum in `cli/mod.rs` is the entry point dispatched from `main.rs`. One file per subcommand: `init`, `build`, `update`, `query`, `serve`.
- `config/schema.rs` — `repolayer.yml` shape: `repos[]`, optional `links[]` (manual cross-repo edges), optional `llm` block.
- `graph/model.rs` — `Node` / `Edge` types. **Node IDs are SHA256(kind ‖ repo ‖ path ‖ symbol) truncated to 16 bytes hex.** `NodeKind::id_tag()` strings (`"repo"`, `"module"`, `"symbol"`, `"idlservice"`, `"idlmethod"`) are part of the ID hash — changing them invalidates every existing index.
- `graph/store.rs` — SQLite schema and CRUD. `Connection` is `Send + !Sync`, so anything sharing a `Store` across async tasks must wrap it in `Arc<Mutex<Store>>` (this is exactly what `mcp::tools::Tools` does).
- `parser/` — tree-sitter parsers for source languages. `Parser` trait (`parser/mod.rs`) returns `ParsedFile { symbols, imports }`. Implementations: `typescript.rs` (handles `.ts/.tsx/.js/.jsx/.mjs`), `python.rs`, `go.rs`. `treesitter.rs` holds shared helpers.
- `parser/idl/` — separate parsers for `.proto` and `.thrift`. IDL repos produce `IdlService` / `IdlMethod` nodes connected by `Defines` / `Contains`.
- `indexer/` — `Indexer::build_all` is the orchestrator. `incremental.rs` uses `git2` to diff working tree vs HEAD and calls `Indexer::reindex_file` per changed file (which deletes the old module subgraph then re-parses).
- `linker/` — post-parse graph stitching:
  - `imports.rs` — `PackageIndex` scans all non-IDL repos' `package.json` so that bare imports (`@scope/foo`) can resolve to a target repo's `main` module (cross-repo TS).
  - `idl_links.rs` — heuristically links IDL methods to code modules: scans every code file for the short method name; emits `Implements` if the file path looks server-side (currently `services/` heuristic in `path_suggests_server`), otherwise `Invokes`.
  - `manual.rs` — applies user-declared `links:` from `repolayer.yml`.
- `query/` — read-only graph traversals exposed by both the CLI `query` debug command and the MCP server. `find_context.rs` is the core ranking logic: tokenize task description → substring search per token → score by (symbol match × 3 + path × 1.5 + summary × 1) → fill until token budget.
- `mcp/` — `rmcp`-based stdio server. Tools are wired with `#[rmcp::tool(...)]` macros in `mcp/mod.rs`; argument types live in `mcp/tools.rs`. Five tools: `find_context`, `get_symbol`, `get_callers`, `get_dependencies`, `list_repos`.
- `llm/` — Phase 3. `LlmProvider` trait (`mod.rs`) with `summarize(snippet, path)`. Implementations: `anthropic.rs`, `deepseek.rs` (both REST via `reqwest`). `summary.rs` walks unsummarized `Module` nodes and stores results back on the node (truncates source to 4000 chars, retries up to 3×, never fatal). `embedding.rs` is scaffolded as `EmbeddingProvider` trait but only `NotImplementedEmbedding` exists — vector reranking is deferred to v0.2.

### Graph model invariants worth knowing

- Every `Module` node must be reachable from its `Repo` via a `Contains` edge. The indexer pre-creates `Contains` edges when resolving imports so target modules are never orphans, even if the walker hasn't visited the target file yet (see comment in `indexer/mod.rs` around the `resolve_import` block — `upsert_node` is idempotent and edge stats deliberately avoid double-counting).
- `Imports` edges only count against `stats.edges` once, even though both the importer and the walker will create the target module node.
- Cross-repo imports without a known `package.json` `main` field synthesize a `package.json` module under the target repo and connect it via `Contains` — this synthesized edge is not counted in stats.
- Edge directions: `Contains` (parent → child), `Imports` (importer → imported), `Calls`, `Implements` (server module → IDL method), `Invokes` (client module → IDL method), `Defines` (IDL repo → service).

### MCP server gotchas

- Stdio only. `cli/serve.rs` errors on `--http`.
- Requires `.repolayer/index.db` to exist; tells the user to run `repolayer build` first.
- `Tools` holds `Arc<Mutex<Store>>` so tool calls serialize on the SQLite connection. Don't try to clone the `Store` or share it without the mutex.
- All `tracing` output goes to stderr (configured in `main.rs`) — required so it doesn't corrupt the stdio JSON-RPC stream.

## Test structure

Integration tests live under `tests/`, one file per concern (`cli_build.rs`, `parser_typescript.rs`, `idl_linking.rs`, `mcp_e2e.rs`, …). Fixtures are in `tests/fixtures/{single_repo_ts, single_repo_py, single_repo_go, multi_repo, multi_repo_with_idl, idl, configs}` and are copied into a `tempfile::tempdir()` per test before being indexed. `assert_cmd` invokes the real `repolayer` binary; `mockito` stubs LLM HTTP endpoints; `insta` is a dev-dep but isn't widely used yet.

## Conventions

- Errors use `anyhow::Result` at boundaries; `thiserror` is available but rarely needed.
- Async only at I/O boundaries (`tokio::main`, LLM calls, MCP transport). Parsing and graph CRUD are sync.
- When changing `NodeKind` / `EdgeKind` serde tags, remember they are persisted as TEXT in SQLite and used in node ID hashing — bump the schema or write a migration rather than silently breaking existing `.repolayer/index.db` files.
- Tree-sitter parser versions are pinned to `0.23.x` (TS/JS/Go/Python) against `tree-sitter = 0.24`; bumping any one parser usually requires bumping all of them together.
