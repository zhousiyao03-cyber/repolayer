# repolayer

> Stop dumping 400k tokens. Index your repos once, let agents query like a database.

`repolayer` is a cross-repo index layer for AI coding agents (Claude Code, Cursor, Codex, …). It pre-computes a structured graph of one or more repositories — modules, exported symbols, call relations, IDL contracts — and exposes it via [MCP](https://modelcontextprotocol.io/). Your agent fetches a precise minimal context (2k-5k tokens) instead of brute-forcing 400k tokens of `grep` and `read`.

## Status

**Pre-alpha (`v0.0.2-alpha`).** Phase 2 complete: multi-repo indexing, IDL (protobuf/thrift), MCP server, incremental update. Phase 3 (LLM enhancement, dogfooding, public release) is up next.

## Phase 2 capabilities

- **Multi-repo indexing** with cross-repo import resolution (TS via `package.json` name lookup)
- **IDL graph** — `.proto` and `.thrift` services and methods become first-class graph nodes
- **`IMPLEMENTS` / `INVOKES` edges** automatically detected via path heuristic (e.g. `services/` → server, otherwise client)
- **Manual cross-repo links** declared in `repolayer.yml` for HTTP / RPC / opaque dependencies
- **MCP server** exposing 5 tools to Claude Code / Cursor / any MCP-compatible agent:
  - `find_context(task_description, budget_tokens)` — minimal relevant context for a coding task
  - `get_symbol(name, repo?)` — definition + callers + callees of a symbol, cross-repo
  - `get_callers(symbol, depth)` — reverse call chain
  - `get_dependencies(repo_or_module, depth)` — forward dependency graph
  - `list_repos()` — currently indexed repos with metadata
- **Incremental update** via `git diff` — only re-parse changed files
- 4 source-language parsers (TypeScript / JavaScript / Python / Go)
- 2 IDL parsers (protobuf / thrift)
- SQLite-backed graph (`.repolayer/index.db`) with stable SHA256 node IDs
- Substring symbol search via `repolayer query`

## Quickstart (build from source)

Requires Rust 1.75+.

```bash
git clone https://github.com/zhousiyao03/repolayer
cd repolayer
cargo install --path .

# In your workspace:
repolayer init                   # create repolayer.yml
# edit repolayer.yml to point at your repos, then:
repolayer build                  # full build → .repolayer/index.db
repolayer query "auth"           # substring search
repolayer update                 # incremental re-index of changed files
repolayer serve                  # start MCP server (stdio, for Claude Code)
```

## Configuration (`repolayer.yml`)

```yaml
repos:
  - path: ./
  - path: ../another_repo
  - path: ../my_idl_repo
    type: idl                # IDL repos define cross-cutting service contracts

# Optional: declare cross-repo dependencies that aren't visible from import statements
links:
  - from: bff
    to: backend_api
    kind: http               # or rpc / calls / invokes

# Optional (Phase 3, not yet wired): LLM-driven summaries / query translation
# llm:
#   enabled: false
#   provider: anthropic
#   api_key_env: ANTHROPIC_API_KEY
```

## Connecting to Claude Code

Add to your Claude Code MCP config:

```json
{
  "mcpServers": {
    "repolayer": {
      "command": "/path/to/repolayer",
      "args": ["serve"],
      "cwd": "/path/to/your/workspace"
    }
  }
}
```

## Roadmap

- **Phase 3** (in progress): optional LLM enhancement (Anthropic / DeepSeek summaries, embedding-based reranking), real-world dogfooding on multi-repo microservice systems, public `v0.1.0` release with cross-platform binaries via `cargo-dist`.

## Why Rust?

Single static binary distribution, native [tree-sitter](https://tree-sitter.github.io/) bindings, zero runtime dependencies for end users.

## License

MIT — see [LICENSE](LICENSE).
