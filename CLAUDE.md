# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

`repolayer` is a Rust CLI + MCP server that pre-computes 4 SQLite-backed indices (graph, per-file outline, file-level deps, hybrid search) across one or more repos and serves them to AI agents via MCP. Single static binary, ast-grep-core parsing for source languages, bare tree-sitter for IDL (protobuf/thrift). No runtime deps for end users.

Status: **v0.2.0-alpha**. Repositioned as the cross-repo + IDL extension of [aeroxy/ast-outline](https://github.com/aeroxy/ast-outline) — see `NOTICE` for adopted components.

## Common commands

```bash
# Build / test
cargo build                                  # debug build
cargo build --release                        # release (LTO, stripped, ~47 MB)
cargo test                                   # run all 290+ integration tests
cargo test --test cli_build                  # run a single integration test file
cargo test adapter_                          # all adapter tests by name prefix
cargo test -- --nocapture                    # show test stdout/stderr
cargo clippy --all-targets -- -D warnings    # lint (must be clean)
cargo fmt                                    # format

# End-to-end usage
cargo run -- init                            # creates repolayer.yml in CWD
cargo run -- build                           # full index → 4 SQLite files in .repolayer/
cargo run -- update                          # incremental re-index via `git diff`
cargo run -- serve                           # start MCP stdio server (logs to stderr)
cargo run -- install --mcp claude-code       # write claude_desktop_config.json
cargo run -- prompt                          # output agent-steering snippet
```

`serve` uses **stdio** for MCP — stdout is reserved for the protocol, all `tracing` output goes to stderr (configured in `src/main.rs`). Don't add `println!` to anything reachable from the serve path.

## Architecture

Pipeline (build): config → walk repos in parallel via rayon → ast-grep parse (or bare tree-sitter for IDL) → mpsc-channel-style serial write to 4 SQLite stores → cross-repo gluing → search index → optional LLM summary.

### Module map (`src/`)

- `core/` — `Declaration` IR (adopted from aeroxy core.rs, split into 3 files): declaration.rs (types), markers.rs (post-processing for native_kind/modifiers/deprecated), schema.rs (JSON_SCHEMA_* constants for stable MCP schema versioning)
- `adapters/` — 12 source-language adapters. Most via `ast-grep-core` + `ast-grep-language` (`LanguageAdapter` trait in `base.rs`): rust / csharp / java / kotlin / scala / typescript / javascript / python / go / swift / markdown. Objective-C (`objc.rs`) uses bare tree-sitter (`tree-sitter-objc`) like the IDL parsers — it does NOT implement `LanguageAdapter`, exposing a free `parse_objc(path, source)` fn instead. Dispatcher: `parse_file(path) -> Option<ParseResult>` (markdown + objc `.m`/`.mm`/`.h` intercepted by extension before the ast-grep `match`).
- `adapters/idl/` — protobuf and thrift parsers (kept on bare `tree-sitter = "0.26"` because thrift has no ast-grep grammar). Different output type (IDL services/methods, not Declarations) so dispatched separately by indexer.
- `graph/` — main graph (`index.db`). NodeKind: Repo / Module / Type / Method / Function / IdlService / IdlMethod. EdgeKind: Contains / Imports / Calls / Implements / Invokes / Defines / Extends. Edges have a `confidence: f32` column (0..1) so callers can distinguish ast-derived from heuristic matches. Schema v2; `meta.schema_version = 2`.
- `outline/` — per-file Declaration tree storage (`outline.db`) + outline/show/digest/find_symbols/find_implementations renderers (adopted from aeroxy core.rs renderer section).
- `deps/` — file-level dependency graph (`deps.db`). Adopted from aeroxy `src/deps/`: extract.rs (raw imports via ast-grep), resolver/{mod,build,resolve}.rs, graph.rs, options.rs, manifest.rs (extended with Cargo workspace + pyproject), scc.rs (Tarjan), dsm.rs, cache.rs, render.rs, traverse.rs, store.rs (repolayer-original SQLite wrapper). `build_for_repo(root)` is the rayon-parallel orchestrator.
- `search/` — hybrid BM25 + dense embedding (`search.db`). Adopted from aeroxy `src/search/`: tokens.rs, format.rs, chunker.rs (Declaration-aware), bm25.rs, download.rs (potion-code-16M model bootstrap), embed.rs (tokenizers + safetensors + memmap2 + wide), ranking.rs, fusion.rs (RRF), cache.rs, index.rs (top-level orchestrator), store.rs (repolayer-original SQLite wrapper with `repo` column for multi-repo).
- `surface/` — published API extraction (Rust `pub use`, Python `__all__`, TypeScript barrel files, Scala `export`). Adopted from aeroxy `src/surface/`. Entry point: `surface::resolve_surface(path, &opts)`.
- `linker/` — post-parse graph stitching:
  - `imports.rs` — `PackageIndex` resolves cross-repo imports. Reads `package.json` (TS), Cargo workspace (Rust), `pyproject.toml` (Python), `go.mod` (Go) via `deps::manifest::detect_aliases`.
  - `idl_links.rs` — links IDL methods to code modules. **ast-grep call expression match → confidence 0.7; path-heuristic fallback (`services/` etc.) → 0.4; no match → no edge.** Replaced the v0.0.x string-contains heuristic.
  - `manual.rs` — applies user-declared `links:` from `repolayer.yml`.
- `indexer/` — `Indexer::build_all` orchestrates the 4-phase pipeline (parse, glue, search, summary). `incremental.rs` uses `git2` to diff working tree vs HEAD; per-file invalidation across all 4 stores. Bulk rebuild of deps + search per affected repo (TODO v0.2.1: per-file incremental there).
- `query/` — read-only graph traversals exposed by both CLI and MCP. `find_context.rs` returns substring-matched candidates with cross-repo edges (Imports/Invokes/Implements pointing to other repos), schema_version `repolayer.find_context.v1`. `find_idl_impl.rs` is repolayer-native: given an IDL method, returns inbound Implements + Invokes edges sorted by confidence.
- `mcp/` — `rmcp`-based stdio server. **15 tools total: 6 native + 9 ast-outline-compat.**
  - Native (`tools.rs` + `mod.rs`): `find_context`, `get_symbol`, `get_callers`, `get_dependencies`, `list_repos`, `find_idl_impl`
  - Compat (`tools_compat.rs`): `outline`, `show`, `digest`, `surface`, `deps`, `reverse_deps`, `cycles`, `search`, `find_related`
- `cli/` — clap subcommands. 16 total. `compat/` subdirectory holds the 9 ast-outline-compat command implementations; `install.rs` writes MCP config for 5 agents (claude-code / cursor / gemini / codex / copilot); `prompt.rs` outputs a markdown snippet teaching agents which tool to call when.
- `llm/` — Phase 3. `LlmProvider` trait with `summarize(snippet, path)`. Implementations: `anthropic.rs`, `deepseek.rs`. `summary.rs` walks unsummarized `Module` nodes (truncates to 4000 chars, retries up to 3×, never fatal). `embedding.rs` is unused now — embedding goes through search subsystem's potion-code path.
- `file_filter.rs` — adopted from aeroxy. Walker filters that mirror `.gitignore` + a hardcoded denylist (`.git`, `node_modules`, `target`, `dist`, …) + per-repo `.repolayer-ignore`.

### Graph model invariants

- **Node IDs are SHA256(kind ‖ repo ‖ path ‖ symbol) truncated to 16 bytes hex.** `NodeKind::id_tag()` strings are part of the ID hash — **changing them invalidates every existing index**.
- Field/Property/EnumMember are NOT main-graph nodes; they live as `children` of their parent Type's `Declaration` in `outline.db`. Rationale: agents rarely query field-level cross-repo relations; node count would explode.
- Method nodes are `Contains`-children of their parent Type node (when known); top-level functions are `Contains`-children of the Module node directly.
- Edge directions: `Contains` (parent → child), `Imports` (importer → imported), `Calls`, `Implements` (server module/type → IDL method), `Invokes` (client module/type → IDL method), `Defines` (IDL repo → service), `Extends` (Type → Type for inheritance/trait/interface impl).
- `confidence: f32` defaults to 1.0. Heuristic edges (current IDL link, future Calls extraction) carry < 1.0 so callers know to verify.

### Storage layout

`.repolayer/` directory contains:
- `index.db` — main graph, schema v2
- `outline.db` — per-file Declaration JSON, schema v1
- `deps.db` — forward_edges + external_imports + file_records, schema v1
- `search.db` — chunks + (future: chunk_vec virtual table for embeddings + bm25_terms), schema v1
- `models/` — potion-code-16M.safetensors (~64 MB, downloaded on first build if needed)

Each store has its own `meta.schema_version` row; bumping requires explicit migration.

### MCP server gotchas

- Stdio only. `cli/serve.rs` errors on `--http`.
- Requires `.repolayer/index.db` to exist; tells the user to run `repolayer build` first.
- `mcp::tools::Tools` holds `Arc<Mutex<Store>>` — tool calls serialize on the SQLite connection.
- All `tracing` output goes to stderr (configured in `main.rs`) — required so it doesn't corrupt the stdio JSON-RPC stream.
- Every tool response wraps data in `{ "schema_version": "<id>.v1", ... }`. Bumping `v1 → v2` is an explicit decision; no silent breakage.

## Test structure

Integration tests under `tests/`, one file per concern. ~290+ tests at v0.2.0-alpha.

**Fixtures**: `tests/fixtures/{single_repo_ts, single_repo_py, single_repo_go, multi_repo, multi_repo_with_idl, idl, configs}` copied into `tempfile::tempdir()` per test.

**Key tooling**: `assert_cmd` invokes the real `repolayer` binary; `mockito` stubs LLM HTTP endpoints; `tempfile` for isolated workspaces.

**Adapter / core test files** (added in Plan A): `adapter_{python,typescript,go,rust,csharp,java,kotlin,scala,markdown,dispatch}.rs`, `core_{declaration,markers}.rs`.

**Plan B test files**: `outline_store.rs`, `outline_render.rs`, `graph_schema_v2.rs`, `deps_extract.rs`, `deps_graph.rs`, `manifest_resolution.rs`, `deps_resolver.rs`, `deps_store.rs`, `deps_e2e.rs`, `search_chunker.rs`, `search_store.rs`, `multi_store_build.rs`.

**Plan C test files**: `find_idl_impl.rs`, `compat_{outline,show,digest,surface,deps,reverse_deps,cycles,search,find_related}.rs`, `mcp_tools_list_15.rs`, `install_mcp.rs`, `prompt_command.rs`.

## Conventions

- Errors use `anyhow::Result` at boundaries; `thiserror` rare.
- Async only at I/O boundaries (`tokio::main`, LLM calls, MCP transport). Parsing and graph CRUD are sync.
- **Don't modify adopted ast-outline files in place** — their content is contract from upstream. Local patches go in NOTICE under "Local patches on top of adopted code".
- When changing `NodeKind` / `EdgeKind` serde tags, remember they are persisted as TEXT in SQLite and used in node ID hashing — bump `meta.schema_version` and write a migration rather than silently breaking existing `.repolayer/index.db`.
- ast-grep grammars come from `ast-grep-language = "0.42"` (one crate covers all 9 source languages). IDL grammars are individual `tree-sitter-protobuf` etc. crates.
