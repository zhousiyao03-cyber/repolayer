# repolayer

[![CI](https://github.com/zhousiyao03-cyber/repolayer/actions/workflows/ci.yml/badge.svg)](https://github.com/zhousiyao03-cyber/repolayer/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](./LICENSE)

> A cross-repo code-index CLI for AI agents. Higher signal-to-noise than
> grep, more cross-repo than LSP, and far cheaper in tokens than reading
> whole files.
>
> Built on top of [aeroxy/ast-outline](https://github.com/aeroxy/ast-outline) —
> extended with multi-repo workspaces, 12 source-language adapters
> (including Swift and Objective-C), IDL (protobuf / thrift) as
> first-class graph nodes, cross-repo Implements / Invokes / Imports /
> Calls edges, and a pluggable embedding backend (local potion model,
> Ollama, or any OpenAI-compatible HTTP endpoint) for semantic search.

## Status: v0.2.0-alpha

- **18 CLI subcommands** covering symbol lookup, who-calls-this,
  IDL method → server / client tracing, hybrid BM25 + semantic search,
  outline / function-body extraction, and the dependency graph.
- **4 SQLite stores under `.repolayer/`** — `index.db` (graph), `outline.db`
  (per-file declaration trees), `deps.db` (file-level dependency graph),
  `search.db` (BM25 + dense embedding chunks via
  [sqlite-vec](https://github.com/asg017/sqlite-vec)).
- **12 source-language adapters**: Rust, C#, Python, TypeScript,
  JavaScript, Java, Kotlin, Scala, Go, Swift, Markdown via
  `ast-grep-core`, plus Objective-C via bare tree-sitter
  (`tree-sitter-objc`, like the IDL parsers).
- **2 IDL parsers** (bare tree-sitter): protobuf, thrift.
- Single static binary (~47 MB release build).

## When to use repolayer vs ast-outline

- **Single repo, you just want outline / show / search** → use
  [ast-outline](https://github.com/aeroxy/ast-outline) directly. No
  index to maintain.
- **Multi-repo workspace, microservice with IDL contracts, an agent
  that needs cross-repo navigation** → repolayer is the natural
  extension.

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

repolayer ships as a **Claude Code skill**, not an MCP server. The
agent calls the CLI directly via Bash; the skill description teaches it
which subcommand to reach for. It works out of the box with Cursor /
Codex / any agent that has shell access.

```bash
repolayer install --skill claude-code
# Writes ~/.claude/skills/repolayer/SKILL.md
# Restart Claude Code, then ask normal cross-repo questions.
```

The skill ([`skills/repolayer.md`](skills/repolayer.md)) ships with a
decision table mapping common tasks to subcommand sequences (e.g.
"trace an API end-to-end" → `query → find-idl-impl → show`).

### Decoupling cwd from index location

For agents editing inside one specific business repo while wanting the
cross-repo index from a separate workspace, set:

```bash
export REPOLAYER_INDEX=$HOME/my_workspace
```

Read-only commands that touch *only* the index (`query`, `search`,
`callers`, `find-idl-impl`, `find-related`, `view`) honour the env
var; commands that also read source files keep using cwd to resolve
relative paths. Write commands (`build`, `update`, `init`) deliberately
stay cwd-bound to avoid surprise writes.

## CLI subcommands

```bash
# Index management
repolayer init                            # create repolayer.yml
repolayer build                           # full index from scratch
repolayer update                          # incremental (via git diff)

# Cross-repo graph lookup (index-only; honours $REPOLAYER_INDEX)
repolayer query "<text>" [--repo NAME]    # exact symbol / IDL method / path substring
repolayer callers <symbol> [--depth N]    # who calls X (inbound Calls edges)
repolayer find-idl-impl <method>          # IDL method → impls (server) + invokers (client)
repolayer search "<query>" [--repo NAME]  # hybrid BM25 + dense semantic
repolayer find-related <file>:<line>      # similar code chunks

# Per-file structure (reads source; cwd-bound)
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
| `--repo <name>` | `query` / `search` / `callers` | Restrict to one repo from `repolayer.yml`. Typo → "Did you mean ..." |
| `--depth N` | `callers` / `deps` | BFS hops (default 1) |
| `--service <name>` | `find-idl-impl` | Disambiguate when multiple IDL services declare the same method |
| `--full-content` | `search --json` | Include chunk bodies (default omits to save tokens; only `path:lines` + 200-char preview) |
| `--json` / `-k` | every read command | Standard output / hit count |

## Configuration (`repolayer.yml`)

```yaml
repos:
  - { name: my_backend, path: ./services/backend }
  - { name: my_frontend, path: ../frontend_monorepo }
  - { name: my_idl, path: ../idl_repo, type: idl }   # protobuf / thrift definitions

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

# Optional: pluggable embedding backend for semantic search.
# Defaults to the built-in 256-dim potion model (offline, no config needed).
# embedding:
#   provider: ollama        # local sidecar: `ollama serve`, zero auth
#   model: qwen3-embedding:0.6b
#   endpoint: http://localhost:11434
#   dim: 1024
#
#   # ...or any OpenAI-compatible HTTP endpoint:
#   # provider: http
#   # model: your-embedding-model
#   # endpoint: https://api.openai.com/v1/embeddings
#   # api_key_env: EMBEDDING_API_KEY
```

### Embedding backends

The dense-search lane is pluggable via the `embedding` block above:

- **`potion-local`** (default) — built-in 256-dim model2vec embedder. Offline,
  no API key, downloaded once on first build. Good enough for most repos.
- **`ollama`** — point at a local `ollama serve` daemon to use a code-aware
  multilingual model (e.g. `qwen3-embedding`) without any GGUF wiring.
- **`http`** — any OpenAI-compatible `/v1/embeddings` endpoint. Auto-batches,
  retries on 5xx/429/timeout, and rate-limits client-side.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│  18 CLI subcommands                                                 │
├─────────────────────────────────────────────────────────────────────┤
│  Query layer (read-only)                                            │
│    index-only (honours $REPOLAYER_INDEX):                           │
│       query / callers / find-idl-impl / search / find-related /     │
│       view                                                          │
│    source-reading (cwd-bound):                                      │
│       outline / show / digest / surface / deps / reverse-deps /     │
│       cycles                                                        │
├─────────────────────────────────────────────────────────────────────┤
│  Storage (4 independent SQLite stores in .repolayer/)               │
│    index.db    main graph:                                          │
│                  nodes ∈ Repo / Module / Type / Method / Function / │
│                          IdlService / IdlMethod                     │
│                  edges ∈ Contains / Imports / Calls / Implements /  │
│                          Invokes / Defines / Extends                │
│                  edges.confidence ∈ [0, 1] (1.0 = AST-derived;      │
│                          < 1.0 = heuristic)                         │
│    outline.db  per-file Declaration trees (JSON, indexed by path)   │
│    deps.db     file-level forward + reverse import edges            │
│    search.db   chunks + 256-d embeddings (vec0 virtual table)       │
├─────────────────────────────────────────────────────────────────────┤
│  Parser layer                                                       │
│    adapters/      12 source-language adapters (ast-grep + tree-sitter)│
│    adapters/idl/  protobuf + thrift (bare tree-sitter)              │
├─────────────────────────────────────────────────────────────────────┤
│  Cross-repo gluing (linker/)                                        │
│    imports        PackageIndex → cross-repo Imports edges  (1.0)    │
│    calls          ast-grep call expressions → Calls edges  (1.0,    │
│                     name-unique resolution only)                    │
│    idl_links      IDL method → Implements / Invokes        (0.7 AST │
│                     call match, 0.4 path heuristic)                 │
│    manual         yml-declared explicit links                       │
└─────────────────────────────────────────────────────────────────────┘
```

### Call-graph extraction

`linker/calls` walks every source file via ast-grep and emits a Calls
edge from the caller's file (`Module` node) to the callee `Function` /
`Method` node when the callee name resolves to **exactly one** node
across the indexed workspace. This keeps confidence at 1.0 by
construction — ambiguous names (`init`, `Get`, `parse`, lowercase short
words) are skipped to avoid drowning real signal in noise.

Caller granularity is the file, not the enclosing function. The
`callers` CLI prints `caller -> target` lines so multi-definition cases
stay unambiguous; follow up with `repolayer outline <caller-path>` to
pinpoint the call site inside the file.

For function-level callers or for ambiguous names, declare the
relationship explicitly in `repolayer.yml` under `links: [{kind:
calls, ...}]`.

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

`search` indexes **declaration-aware chunks** (≤ 1500 chars, packed
greedily, never splits a function in half). One file → ~3.5 chunks on
average. See [`src/search/chunker.rs`](src/search/chunker.rs) for the
algorithm.

## Roadmap

- **v0.2.1** — cargo-dist cross-platform release binaries; per-file
  embedding-aware incremental update; module-level LLM summary backfill.
- **v0.3** — function-level Calls edges (currently file-level); impact
  analysis (`who uses this type / field`); HTTP transport for the skill
  (multi-machine workspaces).

## Testing

```bash
cargo test                                  # ~320 integration tests
cargo test --test cli_query                 # one suite
cargo clippy --all-targets -- -D warnings   # lint must be clean
```

## License

MIT — see [LICENSE](LICENSE) and [NOTICE](NOTICE) for adopted
ast-outline components.
