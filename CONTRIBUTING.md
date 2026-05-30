# Contributing to repolayer

Thanks for your interest in repolayer! This is a Rust CLI + MCP server that
indexes one or more repos into SQLite stores and serves them to AI agents.
Contributions of all sizes are welcome — bug reports, docs, new language
adapters, and features.

## Quick start

```bash
git clone https://github.com/zhousiyao03-cyber/repolayer
cd repolayer
cargo build
cargo test          # network/model-gated tests are #[ignore]'d, so this is hermetic
```

You need a stable Rust toolchain (install via [rustup](https://rustup.rs)).
Everything else is vendored through Cargo.

## Before you open a PR

Run the same three checks CI runs — they are the merge gate:

```bash
cargo fmt --all -- --check          # formatting
cargo clippy --all-targets -- -D warnings   # lints (warnings are errors)
cargo test                          # all hermetic tests must pass
```

`cargo fmt --all` (without `--check`) auto-fixes formatting.

## Project layout

The module map lives in [`CLAUDE.md`](./CLAUDE.md) under *Architecture* — start
there. Highlights:

- `src/adapters/` — one source-language adapter per file (ast-grep based).
- `src/adapters/idl/` — protobuf / thrift parsers (bare tree-sitter).
- `src/graph/`, `src/deps/`, `src/outline/`, `src/search/` — the four SQLite stores.
- `src/linker/` — cross-repo edge stitching (imports, calls, IDL links).
- `src/query/` + `src/mcp/` — read-only traversals exposed over CLI and MCP.

## Conventions

- Errors use `anyhow::Result` at boundaries; `thiserror` is rare.
- Async only at I/O boundaries (LLM calls, MCP transport). Parsing and graph
  CRUD are synchronous.
- **Adopted ast-outline files are upstream contract.** Files marked
  `// adopted from aeroxy/ast-outline` should not be edited in place where
  avoidable; record local patches in [`NOTICE`](./NOTICE).
- `NodeKind` / `EdgeKind` serde tags are persisted as TEXT in SQLite and feed
  node-ID hashing. Changing them invalidates existing `.repolayer/` indexes —
  bump `meta.schema_version` and write a migration instead.

## Commit messages

We use [Conventional Commits](https://www.conventionalcommits.org/):
`feat(search): ...`, `fix(deps): ...`, `docs: ...`, `chore: ...`, `test: ...`.

## Adding a language adapter

1. Add `src/adapters/<lang>.rs` implementing the `LanguageAdapter` trait
   (`src/adapters/base.rs`).
2. Wire it into the dispatcher in `src/adapters/mod.rs`.
3. Add `tests/adapter_<lang>.rs` with fixtures under `tests/fixtures/`.
4. Update the language list in `README.md` and `CLAUDE.md`.

## Reporting bugs

Open an issue with: what you ran, what you expected, what happened, and your
OS + `repolayer --version`. A minimal repo or file that reproduces the problem
is the single most helpful thing you can include.

## License

By contributing, you agree your contributions are licensed under the
[MIT License](./LICENSE), the same as the project.
