# repolayer

> repolayer = ast-outline ([aeroxy/ast-outline](https://github.com/aeroxy/ast-outline)) + cross-repo graph + IDL linking + MCP server tailored for multi-repo agent workflows.

Built on top of [aeroxy/ast-outline](https://github.com/aeroxy/ast-outline)'s parsing, IR, dep-graph, and hybrid search. Extends with: multi-repo workspace model, IDL (protobuf/thrift) as first-class graph nodes, cross-repo import resolution, manual cross-repo links, and 6 MCP tools focused on multi-repo navigation in addition to the 9 inherited from ast-outline.

## Status: v0.2.0-alpha

- 15 MCP tools (6 native + 9 ast-outline-compat)
- 4 SQLite stores: `index.db` (graph), `outline.db` (per-file Declaration trees), `deps.db` (file-level dependency graph), `search.db` (BM25 + dense embedding chunks)
- 10 source-language adapters via `ast-grep-core`: Rust / C# / Python / TypeScript / JavaScript / Java / Kotlin / Scala / Go / Markdown
- 2 IDL parsers (bare tree-sitter): protobuf / thrift
- Single static binary (~47 MB release)

## When to use repolayer vs ast-outline

- **Single repo, just want outline / show / search** → use [ast-outline](https://github.com/aeroxy/ast-outline) directly. Smaller binary, no index to maintain.
- **Multi-repo workspace, microservice with IDL contracts, agent that needs cross-repo navigation** → repolayer is the natural extension.

## Install

```bash
git clone https://github.com/zhousiyao03-cyber/repolayer
cd repolayer
cargo install --path .
```

Requires Rust 1.75+. Installs `repolayer` to `~/.cargo/bin`. (cargo-dist binaries planned for v0.2.1.)

## Quickstart

```bash
repolayer init               # creates repolayer.yml
# edit repolayer.yml — see Configuration below
repolayer build              # full index → 4 SQLite files in .repolayer/
repolayer update             # incremental re-index of git-changed files
repolayer serve              # MCP stdio server (for AI agents)
```

## Connecting AI agents

```bash
repolayer install --mcp claude-code   # writes claude_desktop_config.json
repolayer install --mcp cursor        # writes ~/.cursor/mcp.json
repolayer install --mcp gemini        # writes ~/.config/gemini-cli/config.json
repolayer install --mcp codex         # writes ~/.config/codex/mcp.json
```

Restart your agent after install. Then teach it which tool to call when:

```bash
repolayer prompt >> CLAUDE.md
repolayer prompt >> AGENTS.md
```

## CLI subcommands (16 total)

```
# repolayer-native
repolayer init / build / update / serve / query / install / prompt

# ast-outline-compat
repolayer outline <paths>             # signatures + line ranges, no method bodies
repolayer show <file> <Symbol>        # source body of a symbol
repolayer digest <paths>              # one-page module map
repolayer surface [path]              # published API (resolves pub use / __all__ / barrels)
repolayer deps <path>                 # forward import dependencies
repolayer reverse-deps <path>         # who imports this (refactor blast radius)
repolayer cycles                      # find import cycles via Tarjan SCC (CI-gateable)
repolayer search "<query>"            # BM25 + semantic search
repolayer find-related <file:line>    # similar code chunks
```

Every command also accepts `--json` for machine-readable output.

## MCP tools (15 total)

**6 repolayer-native:**
- `find_context(task_description, budget_tokens)` — minimal relevant context across all repos, with cross-repo edges
- `get_symbol(name, repo?)` — definition + callers + callees
- `get_callers(symbol, depth)` — reverse call graph
- `get_dependencies(repo_or_module, depth)` — forward dep graph
- `list_repos()` — indexed repos with metadata
- `find_idl_impl(method, service?)` — IDL method to code implementations across repos

**9 ast-outline-compat:**
- `outline / show / digest / surface / deps / reverse_deps / cycles / search / find_related`

Every response is wrapped with a stable `schema_version: "<id>.v1"` field for client integration.

## Configuration (`repolayer.yml`)

```yaml
repos:
  - path: ./
  - path: ../another_repo
  - path: ../my_idl_repo
    type: idl                # IDL repos define cross-cutting service contracts

# Optional: declare cross-repo dependencies that aren't visible from imports
links:
  - from: bff
    to: backend_api
    kind: http               # or rpc / calls / invokes

# Optional: LLM-driven module summaries
llm:
  enabled: false
  provider: anthropic        # or deepseek
  api_key_env: ANTHROPIC_API_KEY
  summary: false
```

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  MCP server (rmcp stdio) + 16 CLI subcommands                   │
├─────────────────────────────────────────────────────────────────┤
│  Query layer                                                    │
│   find_context / get_symbol / get_callers / find_idl_impl       │
│   outline / show / digest / surface / deps / reverse-deps       │
│   cycles / search / find-related                                │
├─────────────────────────────────────────────────────────────────┤
│  Storage (4 independent SQLite stores in .repolayer/)           │
│   index.db    main graph (cross-repo + IDL)                     │
│   outline.db  per-file Declaration trees                        │
│   deps.db     file-level dependency graph                       │
│   search.db   BM25 + dense embedding chunks                     │
├─────────────────────────────────────────────────────────────────┤
│  Parser layer                                                   │
│   adapters/      10 source-language adapters (ast-grep-core)    │
│   adapters/idl/  protobuf + thrift (bare tree-sitter)           │
└─────────────────────────────────────────────────────────────────┘
```

See [`docs/superpowers/specs/2026-05-04-ast-outline-extension-design.md`](docs/superpowers/specs/2026-05-04-ast-outline-extension-design.md) for the full design.

## Roadmap

- **v0.2.1**: cargo-dist cross-platform release binaries; per-file incremental deps + search update; full BM25+dense fusion in `search` command (currently substring fallback)
- **v0.3**: real call graph extraction (Calls edges); HTTP transport for MCP

## License

MIT — see [LICENSE](LICENSE) and [NOTICE](NOTICE) for adopted ast-outline components.
