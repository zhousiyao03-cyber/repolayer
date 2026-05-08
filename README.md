# repolayer

> 跨仓代码索引 CLI，给 AI agent 提供「比 grep 信噪比高、比 LSP 更跨仓、比 Read 全文省 token」的代码导航能力。
>
> Built on top of [aeroxy/ast-outline](https://github.com/aeroxy/ast-outline) — extends with multi-repo workspace, IDL (protobuf/thrift) as first-class graph nodes, and cross-repo Implements/Invokes/Imports edges.

## Status: v0.2.0-alpha

- 4 SQLite stores under `.repolayer/`：`index.db`（graph）/ `outline.db`（per-file Declaration trees）/ `deps.db`（file-level dependency graph）/ `search.db`（BM25 + dense embedding chunks via [`sqlite-vec`](https://github.com/asg017/sqlite-vec)）
- 10 source-language adapters via `ast-grep-core`：Rust / C# / Python / TypeScript / JavaScript / Java / Kotlin / Scala / Go / Markdown
- 2 IDL parsers (bare tree-sitter)：protobuf / thrift
- Single static binary (~47 MB release)

## When to use repolayer vs ast-outline

- **Single repo, just want outline / show / search** → use [ast-outline](https://github.com/aeroxy/ast-outline) directly. No index to maintain.
- **Multi-repo workspace, microservice with IDL contracts, agent that needs cross-repo navigation** → repolayer is the natural extension.

## Install

```bash
git clone https://github.com/zhousiyao03-cyber/repolayer
cd repolayer
cargo install --path .
```

Requires Rust 1.75+. Installs `repolayer` to `~/.cargo/bin`.

## Quickstart

```bash
repolayer init               # creates repolayer.yml in cwd
# edit repolayer.yml — list the repos you want to index
repolayer build              # full index → 4 SQLite files in .repolayer/
repolayer update             # incremental re-index of git-changed files
```

## Connecting AI agents (Claude Code skill)

repolayer ships as a **Claude Code skill**, not an MCP server. The agent calls
the CLI directly via Bash; the skill description teaches it which subcommand
to reach for. (Compatible with Cursor / Codex out of the box — they all have
shell access.)

```bash
repolayer install --skill claude-code
# Writes ~/.claude/skills/repolayer/SKILL.md
# Restart Claude Code, then ask normal cross-repo questions.
```

The skill ([`skills/repolayer.md`](skills/repolayer.md)) ships with a decision
tree mapping common tasks to subcommand sequences (e.g. "trace an API
end-to-end" → `query → outline → show`).

### Decoupling cwd from index location

For agents editing inside one specific business repo while wanting the
cross-repo index from a separate workspace, set:

```bash
export REPOLAYER_INDEX=$HOME/my_workspace
```

Read-only commands (`query` / `search` / `find-related` / `view`) honor the env
var; write commands (`build` / `update` / `init`) deliberately stay cwd-bound
to avoid surprise writes.

## CLI subcommands

```bash
# Index management
repolayer init                            # create repolayer.yml
repolayer build                           # full index from scratch
repolayer update                          # incremental (via git diff)

# Cross-repo lookup (read-only; honors $REPOLAYER_INDEX)
repolayer query "<text>" [--repo NAME]    # exact symbol / IDL method / path substring
repolayer search "<query>" [--repo NAME]  # hybrid BM25 + dense semantic; URL/string-friendly
repolayer find-related <file>:<line>      # similar code chunks

# Per-file structure (no index needed; pure ast-grep)
repolayer outline <paths>                 # signatures + line ranges, no bodies
repolayer show <file> <Symbol>            # source body by AST boundaries
repolayer digest <paths>                  # one-page public API map
repolayer surface [path]                  # resolves pub use / __all__ / barrels

# Dependency graph
repolayer deps <path>                     # forward imports
repolayer reverse-deps <path>             # who imports this
repolayer cycles                          # Tarjan SCC; exit 1 if cycles (CI gate)

# Setup / view
repolayer install --skill claude-code     # deploy SKILL.md
repolayer view --out <dir>                # static HTML viewer of the index
```

`--json` works on every read command for machine-readable output.

### Useful flags

| Flag | Where | Effect |
|---|---|---|
| `--repo <name>` | `query` / `search` | Restrict to one repo from `repolayer.yml`. Typo → "Did you mean ..." |
| `--full-content` | `search --json` | Include chunk body (default omits to save tokens; only path:lines + 200-char preview) |
| `--json` / `-k` | every read command | Standard output / hit count |

## Configuration (`repolayer.yml`)

```yaml
repos:
  - { name: my_backend, path: ./services/backend }
  - { name: my_frontend, path: ../frontend_monorepo }
  - { name: my_idl, path: ../idl_repo, type: idl }   # protobuf/thrift definitions

# Optional: declare cross-repo dependencies that imports don't expose
links:
  - from: bff
    to: backend_api
    kind: http     # or rpc / calls / invokes

# Optional: LLM-driven module summaries (off by default)
llm:
  enabled: false
  provider: anthropic       # or deepseek
  api_key_env: ANTHROPIC_API_KEY
  summary: false
```

## Architecture

```
┌────────────────────────────────────────────────────────────────────┐
│  16 CLI subcommands                                                 │
├────────────────────────────────────────────────────────────────────┤
│  Query layer (read-only, honors $REPOLAYER_INDEX)                   │
│   query / search / find-related / view                              │
│   outline / show / digest / surface / deps / reverse-deps / cycles  │
├────────────────────────────────────────────────────────────────────┤
│  Storage (4 independent SQLite stores in .repolayer/)               │
│   index.db    main graph: nodes (Repo/Module/Type/Method/Function/  │
│               IdlService/IdlMethod) + edges (Contains/Imports/      │
│               Calls/Implements/Invokes/Defines/Extends),            │
│               edges.confidence ∈ [0, 1] for heuristic vs ast-grounded│
│   outline.db  per-file Declaration trees (JSON, indexed by path)    │
│   deps.db     file-level forward + reverse import edges             │
│   search.db   chunks + 256-d embeddings (vec0 virtual table)        │
├────────────────────────────────────────────────────────────────────┤
│  Parser layer                                                       │
│   adapters/      10 source-language adapters (ast-grep-core)        │
│   adapters/idl/  protobuf + thrift (bare tree-sitter)               │
├────────────────────────────────────────────────────────────────────┤
│  Cross-repo gluing                                                   │
│   linker/imports     PackageIndex → cross-repo Imports edges (1.0)  │
│   linker/idl_links   IDL method → impl/invokes (0.7 ast / 0.4 path) │
│   linker/manual      yml-declared explicit links                    │
└────────────────────────────────────────────────────────────────────┘
```

### Search retrieval pipeline

```
search "<query>"
  ├─ tokenize → BM25 over chunk content (in-memory, rebuilt per query)
  ├─ encode_query → vec0 kNN (sqlite-vec, 256-d L2-normalized)
  ├─ filter:  L2 distance ≤ 1.10 (loose) for fusion
  │           L2 distance ≤ 1.00 (strict) when BM25 had zero hits
  ├─ RRF fusion: 1/(60+rank), alpha = 0.3 (symbol queries) | 0.5 (NL)
  └─ output:  hits + lane = fusion / bm25_only / semantic_only / substring
              (lane tells the agent how much to trust the result)
```

### Chunking

`search` indexes **declaration-aware chunks** (≤ 1500 chars, packed greedy,
never splits a function in half). One file → average 3.5 chunks. See
[`src/search/chunker.rs`](src/search/chunker.rs) for the algorithm.

## Roadmap

- **v0.2.1**: cargo-dist cross-platform release binaries; per-file embedding-aware incremental update; module-level LLM summary backfill
- **v0.3**: real call graph extraction (Calls edges between functions); HTTP transport for the skill (multi-machine workspaces)

## Testing

```bash
cargo test                                  # 290+ integration tests
cargo test --test cli_query                 # one suite
cargo clippy --all-targets -- -D warnings   # lint must be clean
```

## License

MIT — see [LICENSE](LICENSE) and [NOTICE](NOTICE) for adopted ast-outline components.
