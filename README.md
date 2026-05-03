# repolayer

> Stop dumping 400k tokens. Index your repos once, let agents query like a database.

`repolayer` is a cross-repo index layer for AI coding agents (Claude Code, Cursor, Codex, …). It pre-computes a structured graph of one or more repositories — modules, exported symbols, call relations, IDL contracts — and exposes it via [MCP](https://modelcontextprotocol.io/). Your agent fetches a precise minimal context (2k-5k tokens) instead of brute-forcing 400k tokens of `grep` and `read`.

## Status

**Pre-alpha (`v0.0.1-alpha`).** Phase 1 complete: single-repo TypeScript / Python indexing via CLI. MCP server, multi-repo + IDL support, and LLM-driven enhancements are coming in Phase 2 / 3.

## Phase 1 capabilities

- CLI: `repolayer init` / `build` / `query`
- TypeScript / JavaScript / TSX / JSX / MJS parsing via [tree-sitter](https://tree-sitter.github.io/) — extracts exported functions, classes, interfaces, type aliases, and consts
- Python parsing — extracts top-level functions and classes (including `@decorator`-wrapped), filters underscore-prefixed names
- SQLite-backed graph (`.repolayer/index.db`) with stable SHA256 node IDs
- Substring symbol search across the indexed graph

## Quickstart (build from source)

Requires Rust 1.75+.

```bash
git clone https://github.com/zhousiyao03/repolayer
cd repolayer
cargo install --path .

# In your project directory:
repolayer init             # creates repolayer.yml
# edit repolayer.yml to point at your repos, then:
repolayer build            # writes .repolayer/index.db
repolayer query "auth"     # substring search across symbols
```

## Configuration (`repolayer.yml`)

```yaml
repos:
  - path: ./
  # - path: ../another_repo
  # - path: ../my_idl_repo
  #   type: idl   # IDL repos are recognised but not yet indexed (Phase 2)

# Optional: manual cross-repo links (Phase 2)
# links:
#   - from: bff
#     to: backend_api
#     kind: http

# Optional: LLM-driven summaries / query translation (Phase 3)
# llm:
#   enabled: false
```

## Roadmap

- **Phase 2**: Go parser, multi-repo cross-package import resolution, protobuf / thrift IDL graph, MCP server with 5 tools (`find_context`, `get_symbol`, `get_callers`, `get_dependencies`, `list_repos`), incremental updates via `git diff`.
- **Phase 3**: Optional LLM enhancement (Anthropic / DeepSeek summaries, embedding-based reranking), real-world dogfooding, public `v0.1.0` release with cross-platform binaries.

## Why Rust?

Single static binary distribution, native [tree-sitter](https://tree-sitter.github.io/) bindings, zero runtime dependencies for end users.

## License

MIT — see [LICENSE](LICENSE).
