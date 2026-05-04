# Design: ast-outline extension — repolayer v0.2

**Status:** Approved (brainstorming complete)
**Date:** 2026-05-04
**Branch:** `feature/ast-outline-ext`
**Worktree:** `.worktrees/ast-outline-ext`

## 1. Goal

Reposition repolayer as the **cross-repo / IDL-aware extension of [aeroxy/ast-outline](https://github.com/aeroxy/ast-outline)**. Keep repolayer's differentiator (multi-repo workspace + protobuf/thrift IDL graph + manual cross-repo links + MCP server tailored to multi-repo agent flows). Adopt ast-outline's parser stack, IR, dep-graph, hybrid search, and 9 single-repo tools wholesale via direct code adoption (MIT-compatible, with NOTICE attribution).

The end state is a single static Rust binary that exposes **15 MCP tools** (5 repolayer-native + 9 ast-outline-compat + 1 new IDL-impl tool) and equivalent CLI subcommands.

This is a **breaking** v0.0.x → v0.2 change. No migration from existing `index.db` files; users re-run `repolayer build`.

## 2. Repositioning

### 2.1 README header (final wording, written into repo root README)

> repolayer = ast-outline (aeroxy/ast-outline) + cross-repo graph + IDL linking + MCP server tailored for multi-repo agent workflows.
>
> Built on top of aeroxy/ast-outline's parsing, IR, dep-graph, and hybrid search. Extends with: multi-repo workspace model, IDL (protobuf/thrift) as first-class graph nodes, cross-repo import resolution, manual cross-repo links, and 6 MCP tools focused on multi-repo navigation in addition to the 9 inherited from ast-outline.

### 2.2 NOTICE file (repo root)

```
This product includes software developed by:

  ast-outline (https://github.com/aeroxy/ast-outline)
  Copyright (c) 2026 Aero <aero.windwalker@gmail.com>
  Licensed under the MIT License.

Components copied or adapted from ast-outline:
  - src/core/declaration.rs (from src/core.rs)
  - src/adapters/* (except idl/)
  - src/deps/*
  - src/search/*
  - src/surface/*
  - portions of src/outline/*

The following components are original to repolayer:
  - cross-repo workspace model (config/, linker/)
  - IDL parsing and graph (adapters/idl/, graph/)
  - MCP server with multi-repo tools (mcp/)
  - cross-repo import resolution
```

LICENSE itself stays MIT (compatible).

## 3. Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  MCP server (rmcp stdio) + CLI subcommands                      │
│  ─ repolayer-native (6): find_context, get_symbol, get_callers, │
│    get_dependencies, list_repos, find_idl_impl                  │
│  ─ ast-outline-compat (9): outline, show, digest, surface,      │
│    deps, reverse-deps, cycles, search, find-related             │
├─────────────────────────────────────────────────────────────────┤
│  Query layer                                                     │
│  ─ cross_repo_query/  (repolayer-native)                         │
│  ─ outline_query/     (adopted from ast-outline)                 │
│  ─ deps_query/        (adopted from ast-outline)                 │
│  ─ search_query/      (adopted from ast-outline)                 │
│  ─ surface_query/     (adopted from ast-outline)                 │
├─────────────────────────────────────────────────────────────────┤
│  Storage layer (4 SQLite + 1 model cache)                       │
│  ─ .repolayer/index.db    -- main graph (cross-repo + IDL)      │
│  ─ .repolayer/outline.db  -- per-file Declaration trees         │
│  ─ .repolayer/deps.db     -- file-level dep graph (cached)      │
│  ─ .repolayer/search.db   -- BM25 + dense embedding index       │
│  ─ .repolayer/models/     -- potion-code-16M weights (~64 MB)   │
├─────────────────────────────────────────────────────────────────┤
│  Parser layer                                                    │
│  ─ adapters/  (10 from ast-outline)  ← ast-grep-core            │
│      rust / csharp / java / kotlin / scala / typescript /        │
│      python / go / markdown / javascript-via-typescript          │
│  ─ adapters/idl/  (repolayer-original)  ← bare tree-sitter      │
│      protobuf / thrift                                           │
└─────────────────────────────────────────────────────────────────┘
```

### 3.1 Module dependency direction (acyclic)

```
cli/          → all
mcp/          → query/, indexer/
indexer/      → adapters/, linker/, graph/, outline/, deps/, search/, llm/
query/        → graph/, outline/, deps/, search/, surface/
linker/       → graph/, adapters/idl/
search/       → core/, adapters/
deps/         → core/, adapters/
outline/      → core/
surface/      → core/, deps/
graph/        → core/
adapters/     → core/
core/         → (leaf)
```

`core/` is the leaf. All upper layers depend on it; nothing inside `core/` depends on anything else in the crate.

## 4. Data model

### 4.1 `core::Declaration` IR (adopted verbatim from ast-outline)

See aeroxy `src/core.rs:80`. Direct copy. Fields:

- `kind: DeclarationKind` — 20-variant enum (Namespace/Class/Struct/Interface/Record/Enum/EnumMember/Method/Function/Constructor/Destructor/Property/Indexer/Field/Event/Delegate/Operator/Heading/CodeBlock + `Module` added by repolayer)
- `name`, `signature`, `bases: Vec<String>`, `attrs`, `docs`, `docs_inside`, `visibility`
- `start_line/end_line/start_byte/end_byte/doc_start_byte`
- `native_kind: Option<String>` (e.g. Rust `trait` ↔ canonical `Interface`)
- `modifiers: Vec<String>` (async/static/abstract/...)
- `deprecated: bool`
- `children: Vec<Declaration>` — nested

Repolayer adds zero fields to this struct. All cross-repo / IDL extensions live in the graph layer below, not in `Declaration`.

### 4.2 Main graph (`index.db`) — repolayer-original schema

Schema version: **2** (existing v1 schema is dropped — pre-alpha, no migration).

#### Tables

```sql
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
-- meta('schema_version', '2'), meta('built_at', '...'), etc.

CREATE TABLE nodes (
    id          TEXT PRIMARY KEY,        -- SHA256(kind‖repo‖path‖symbol)[..16] hex
    kind        TEXT NOT NULL,           -- see NodeKind below
    repo        TEXT NOT NULL,
    path        TEXT NOT NULL,
    symbol      TEXT,                    -- qualified e.g. "UserService.create"
    summary     TEXT,                    -- LLM-generated, optional
    visibility  TEXT,                    -- "public"/"private"/"protected"/""
    native_kind TEXT,                    -- e.g. "trait" for canonical Interface
    loc_start   INTEGER,
    loc_end     INTEGER,
    deprecated  INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_nodes_repo ON nodes(repo);
CREATE INDEX idx_nodes_kind ON nodes(kind);
CREATE INDEX idx_nodes_symbol ON nodes(symbol) WHERE symbol IS NOT NULL;
CREATE INDEX idx_nodes_repo_path ON nodes(repo, path);  -- for outline.db join

CREATE TABLE edges (
    from_id    TEXT NOT NULL,
    to_id      TEXT NOT NULL,
    kind       TEXT NOT NULL,
    confidence REAL NOT NULL DEFAULT 1.0,  -- 0..1, < 1 means inferred
    PRIMARY KEY (from_id, to_id, kind)
);
CREATE INDEX idx_edges_from ON edges(from_id, kind);
CREATE INDEX idx_edges_to   ON edges(to_id, kind);
```

#### NodeKind (extended)

```rust
pub enum NodeKind {
    Repo,
    Module,         // file-level container
    Type,           // class/struct/interface/trait/enum/record (replaces flat "Symbol")
    Method,         // method/ctor/dtor/operator inside a Type
    Function,       // top-level function (not inside a Type)
    IdlService,
    IdlMethod,
}
```

Field/Property/EnumMember are **NOT** main-graph nodes — they live as `children` of their parent `Type`'s `Declaration` in `outline.db`. Rationale: agent rarely queries field-level cross-repo relations; querying them would explode node count.

#### EdgeKind (extended)

```rust
pub enum EdgeKind {
    Contains,    // Repo→Module, Module→Type/Function, Type→Method
    Imports,     // Module→Module (cross-repo and within-repo coarse)
    Calls,       // Method/Function→Method/Function (Phase 2: best-effort, marked low confidence)
    Implements,  // Module/Type→IdlMethod (server side)
    Invokes,     // Module/Type→IdlMethod (client side)
    Defines,     // IdlService→IdlMethod (and Repo→IdlService for IDL repos)
    Extends,     // Type→Type (inheritance / trait impl / interface impl)
}
```

Edge endpoints by kind (validation contract):

| EdgeKind | from | to |
|---|---|---|
| Contains | Repo | Module \| IdlService |
| Contains | Module | Type \| Function |
| Contains | Type | Method |
| Contains | IdlService | IdlMethod |
| Imports | Module | Module |
| Calls | Method \| Function | Method \| Function |
| Implements | Module \| Type | IdlMethod |
| Invokes | Module \| Type | IdlMethod |
| Defines | Repo | IdlService |
| Extends | Type | Type |

`confidence` column lets callers distinguish ast-derived edges (1.0) from heuristic IDL/string-match edges (0.5–0.8). MCP responses surface this so agents know when to verify.

### 4.3 outline.db — adopted from ast-outline `outline/`

Per-file `Declaration` tree as JSON, plus parse error count.

```sql
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE files (
    repo            TEXT NOT NULL,
    path            TEXT NOT NULL,
    language        TEXT NOT NULL,
    line_count      INTEGER NOT NULL,
    declarations    TEXT NOT NULL,    -- serde_json of Vec<Declaration>
    parse_errors    INTEGER NOT NULL DEFAULT 0,
    content_hash    BLOB NOT NULL,    -- xxhash64 for invalidation
    PRIMARY KEY (repo, path)
);
CREATE INDEX idx_files_repo ON files(repo);
```

Joined to main graph via `(repo, path)` when an MCP tool needs both relations and structure.

### 4.4 deps.db — adopted from ast-outline `deps/`

Adopted essentially verbatim. Schema is ast-outline's existing one with a `repo` column added (so multi-repo workspaces have one deps file but per-repo-scoped queries are cheap). cycle/SCC/DSM logic untouched.

```sql
CREATE TABLE meta (...);
CREATE TABLE forward_edges (
    repo        TEXT NOT NULL,
    from_path   TEXT NOT NULL,
    to_path     TEXT NOT NULL,
    edge_kind   TEXT NOT NULL,
    line        INTEGER,
    local_name  TEXT,
    raw_path    TEXT,
    PRIMARY KEY (repo, from_path, to_path, edge_kind)
);
CREATE INDEX idx_deps_reverse ON forward_edges(repo, to_path);
CREATE TABLE external_imports (
    repo TEXT NOT NULL, from_path TEXT NOT NULL, raw TEXT NOT NULL
);
CREATE TABLE file_records (    -- for cache invalidation
    repo TEXT NOT NULL, path TEXT NOT NULL,
    mtime_ns INTEGER, size INTEGER, content_hash BLOB,
    PRIMARY KEY (repo, path)
);
```

### 4.5 search.db — adopted from ast-outline `search/`

BM25 inverted index + dense embedding via potion-code-16M, with RRF fusion at query time. Adopted verbatim with a `repo` column added per row. SQLite + `sqlite-vec` virtual table for embeddings (already a dependency in repolayer's `Cargo.toml`).

```sql
CREATE TABLE meta (...);
CREATE TABLE chunks (
    id          INTEGER PRIMARY KEY,
    repo        TEXT NOT NULL,
    path        TEXT NOT NULL,
    start_line  INTEGER, end_line INTEGER,
    content     TEXT,
    chunk_hash  BLOB
);
CREATE VIRTUAL TABLE chunk_vec USING vec0(embedding float[256]);   -- potion-code-16M dim
CREATE TABLE bm25_terms (...);  -- aeroxy schema, copied
```

### 4.6 model cache `.repolayer/models/`

`potion-code-16M.safetensors` (~64 MB), downloaded on first `repolayer build` if missing. Source: `https://huggingface.co/minishlab/potion-code-16M`. SHA256 verified against constant in source. `download.rs` from aeroxy adopted verbatim.

## 5. Build pipeline (rayon-parallel)

```
repolayer build
    │
    ├─ Phase 0: load config + open 4 stores
    │
    ├─ Phase A: walk + parse  (rayon par_iter over files)
    │     │
    │     ├─ for code repo file:
    │     │     ast-grep adapter → ParseResult { Declaration tree }
    │     │     extract imports → RawImport list
    │     │
    │     ├─ for IDL repo file:
    │     │     bare tree-sitter parser → IDL services + methods
    │     │
    │     └─ results sent through std::sync::mpsc::channel
    │           ↓
    │        single writer thread:
    │           ├─ index.db   ← Type/Method/Function nodes + Contains edges
    │           ├─ outline.db ← full Declaration tree per file
    │           └─ deps.db    ← raw imports (resolved later)
    │
    ├─ Phase B: cross-repo gluing  (serial, repolayer-original)
    │     ├─ PackageIndex resolves cross-repo imports (TS pkg.json,
    │     │   Cargo workspace, go.mod, pyproject — extended)
    │     ├─ Resolved imports written as Imports edges to index.db
    │     ├─ deps.db resolver runs for within-repo imports
    │     ├─ IdlLinker scans code modules for IDL method names — but
    │     │   now using ast-grep call_expression patterns rather than
    │     │   `content.contains(short)` heuristic. Confidence = 0.7
    │     │   for ast match, 0.4 for fallback string match.
    │     └─ Manual links from repolayer.yml applied
    │
    ├─ Phase C: search index  (rayon)
    │     ├─ chunker.rs walks every file's Declaration tree, emits
    │     │   one chunk per top-level Declaration (size-bounded)
    │     ├─ bm25 inverted index built (rayon par_iter)
    │     ├─ potion-code embedding computed per chunk
    │     │   (CPU batch, par_iter chunks of 32)
    │     └─ search.db written
    │
    └─ Phase D: optional LLM summary  (existing logic, unchanged)
```

### Concurrency rules

- **Parsing parallel, writing serial.** Each phase may spawn rayon for parsing/computation but must serialize SQLite writes through a single writer thread per store (mpsc channel).
- **Stores opened once at build start, dropped at end.** No per-file reopens.
- **Crash-safety: each phase commits its own transaction.** Phase failure leaves prior phases' data intact and the meta `built_at` only updated at end.

### incremental update (`repolayer update`)

Existing git-diff approach kept and extended:

1. git diff per repo → changed file set
2. For each changed file:
   - main graph: delete prior nodes/edges via `(repo, path)`, re-parse, re-insert
   - outline.db: replace row keyed `(repo, path)`
   - deps.db: replace forward_edges row, re-resolve
   - search.db: invalidate chunks for that file via content_hash, re-chunk + re-embed
3. Cross-repo glue: only re-run `IdlLinker` and `PackageIndex` if any imports changed (detected via diff in deps.db's external_imports for the file).

## 6. Adapter layer

### 6.1 Source-language adapters (10) — adopted

Trait `LanguageAdapter` from aeroxy `adapters/base.rs`:

```rust
pub trait LanguageAdapter {
    fn language_name(&self) -> &'static str;
    fn extensions(&self) -> &'static [&'static str];
    fn parse<'a, D: Doc>(
        &self,
        path: &Path,
        source: &[u8],
        root: Node<'a, D>,
    ) -> ParseResult;
}
```

Adopted: rust.rs, csharp.rs, java.rs, kotlin.rs, scala.rs, typescript.rs (handles .ts/.tsx/.js/.jsx/.mjs/.cjs), python.rs, go.rs, markdown.rs.

Plus an `adapters/mod.rs` registry:

```rust
pub static ADAPTERS: Lazy<Vec<Box<dyn LanguageAdapter + Send + Sync>>> = ...;
pub fn get_adapter_for(path: &Path) -> Option<&'static dyn LanguageAdapter>;
```

Replaces current `parse_by_extension` hardcoded match in `indexer/mod.rs:14`.

### 6.2 IDL adapters (2) — repolayer-original

`adapters/idl/protobuf.rs` and `adapters/idl/thrift.rs` retained with current implementation. They do NOT implement `LanguageAdapter` (different output type — IDL services, not Declarations). Indexer dispatches them via `RepoConfig::is_idl()` separately, as today.

### 6.3 Why the dual stack

- ast-grep-language has protobuf grammar but not thrift; switching protobuf alone leaves an inconsistent split.
- IDL parsing only needs to extract service names, method names, request/response types — this is shallow and the existing tree-sitter code is ~200 lines per format. Migrating buys nothing.
- The dual stack is bounded: IDL adapters use bare tree-sitter; everything else uses ast-grep. The boundary is `adapters/idl/` directory.

## 7. Cargo.toml changes

### Added

```toml
ast-grep-core = "0.42"
ast-grep-language = "0.42"
rayon = "1.10"
similar = "2"
xxhash-rust = { version = "0.8", features = ["xxh3"] }
fs2 = "0.4"
toml_edit = "0.22"
tokenizers = { version = "0.21", default-features = false, features = ["onig"] }
safetensors = "0.6"
memmap2 = "0.9"           # mmap potion-code weights
wide = "0.7"              # SIMD cosine for embedding rerank
bincode = { version = "2", features = ["serde"] }
colored = "3"
once_cell = "1"
```

### Kept

```toml
tree-sitter = "0.24"          # for IDL adapters
# tree-sitter-typescript / -javascript / -python / -go REMOVED
# (subsumed by ast-grep-language)
clap = "4.5"
serde = "1"
serde_json = "1"
serde_yml = "0.0.12"
anyhow = "1"
tokio = "1.40"
rusqlite = "0.32"
sqlite-vec = "0.1"
ignore = "0.4"
git2 = "0.19"
rmcp = "1.6"
schemars = "1"
reqwest = "0.12"
async-trait = "0.1"
sha2 = "0.10"
hex = "0.4"
tracing = "0.1"
tracing-subscriber = "0.3"
thiserror = "2"
regex = "1.10"
zerocopy = "0.8"
```

### Removed

```toml
tree-sitter-typescript
tree-sitter-javascript
tree-sitter-python
tree-sitter-go
```

These are now provided by `ast-grep-language`. (`tree-sitter` itself stays for IDL.)

Estimated release binary size: ~30 MB (currently 12 MB; +ast-grep core, +rayon, +tokenizers, +safetensors).

## 8. MCP tools

### 8.1 repolayer-native (6)

| Tool | Function | Notes |
|---|---|---|
| `find_context` | Hybrid BM25 + dense rerank → graph-augment with cross-repo edges | upgraded from substring |
| `get_symbol` | Symbol definition + callers + callees + related IDL methods | upgraded: returns `Declaration` + cross-refs |
| `get_callers` | Reverse call graph traversal | unchanged signature |
| `get_dependencies` | Forward import graph (with cross-repo) | reads main graph, not deps.db |
| `list_repos` | Indexed repos with metadata | unchanged |
| `find_idl_impl` | Given an IDL method, find code modules that Implement or Invoke it | new |

### 8.2 ast-outline-compat (9, all adopted)

`outline`, `show`, `digest`, `surface`, `deps`, `reverse-deps`, `cycles`, `search`, `find-related`. Behaviour identical to ast-outline. Each tool's `repo` argument is optional — defaulting to "first repo in workspace" for single-repo cases, required when multiple repos.

### 8.3 Schema versioning

Every MCP tool response is wrapped in:

```json
{
  "schema_version": "repolayer.<tool>.v1",
  "data": { ... }
}
```

Adopted from ast-outline's `JSON_SCHEMA_*` constants (`core::schema::*`). Bumping `v1 → v2` requires explicit decision; no silent breakage.

### 8.4 Confidence surfacing

Tools that return graph edges include each edge's `confidence` field. For `find_idl_impl` and IDL-related results in `get_symbol`, this matters most: the `Implements`/`Invokes` heuristic can be wrong.

## 9. CLI subcommands

```
repolayer init              # write repolayer.yml template
repolayer build             # full build, all 4 SQLite + model cache
repolayer update            # incremental git-diff
repolayer query <text>      # debug substring search (kept for parity, deprecated msg)
repolayer serve             # MCP stdio
repolayer install --mcp <agent>  # NEW: write MCP config
                                  #     agents: claude-code, cursor, gemini, codex, copilot
repolayer prompt            # NEW: print agent system-prompt snippet
repolayer outline <path>    # NEW: ast-outline-compat
repolayer show <path> <sym> # NEW
repolayer digest <path>     # NEW
repolayer surface [path]    # NEW
repolayer deps <path>       # NEW
repolayer reverse-deps <p>  # NEW
repolayer cycles            # NEW
repolayer search <query>    # NEW
repolayer find-related <p>  # NEW
```

`install --mcp` writes appropriate config files:
- claude-code: `~/Library/Application Support/Claude/claude_desktop_config.json` mcpServers section
- cursor: `~/.cursor/mcp.json`
- etc.

Each writes a backup first. Exit code 0 if applied, 1 if config malformed (no clobber).

## 10. Testing strategy

### 10.1 Inheritance from current 19 test files

| Existing test | Status |
|---|---|
| `cli_build.rs` | rewrite — new schema + 4 SQLite |
| `cli_query.rs` | rewrite — new symbol model |
| `cli_update.rs` | rewrite — multi-store invalidation |
| `config_loading.rs` | keep |
| `graph_model.rs` | rewrite — new NodeKind/EdgeKind |
| `graph_store.rs` | rewrite |
| `idl_linking.rs` | extend — confidence levels, ast-grep matching |
| `manual_links.rs` | keep with minor adapter |
| `mcp_e2e.rs` | extend — list_tools must show 15 |
| `multi_repo_linking.rs` | keep |
| `parser_go.rs` / `_python.rs` / `_typescript.rs` | replace — ast-grep adapter tests |
| `parser_protobuf.rs` / `_thrift.rs` | keep — IDL still bare tree-sitter |
| `query_find_context.rs` | rewrite — hybrid search |
| `query_others.rs` | extend |
| `llm_*` | keep — summary unchanged |

### 10.2 New integration tests

- `outline_e2e.rs` — outline + show + digest on fixtures
- `surface_e2e.rs` — surface on rust/python/typescript fixtures
- `deps_cycles.rs` — cycle detection roundtrip
- `search_hybrid.rs` — BM25 + dense fusion on fixture corpus
- `find_idl_impl.rs` — new tool

### 10.3 Fixture additions

Existing `tests/fixtures/single_repo_{ts,py,go}` retained. Add `single_repo_rust` (so we can dogfood repolayer indexing itself). Add `cross_repo_with_search` for end-to-end hybrid-search test.

## 11. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Adopting ~12 KLOC of ast-outline code may bring bugs we don't understand | Adopt module-by-module with their tests; do not modify adopted files in the first pass; only add wrappers |
| ast-grep API surface changes between minor versions | Pin to `0.42.x`, document the upgrade path |
| Model download (~64 MB) on first run is a UX hazard | Surface progress, allow `REPOLAYER_NO_DOWNLOAD=1` to disable embedding (fallback BM25-only) |
| Build time goes from <1s to multi-second on fixtures | Acceptable trade — search index build dominates; gated by `--no-search` flag for fast iteration |
| Search index doesn't update incrementally | Out of scope for v0.2 — `repolayer update` invalidates affected chunks but rebuilds them; not an FTS index re-merge |
| Cross-store consistency on partial build failure | Each store has its own meta key `built_at`; queries warn if any store is older than `index.db` |
| sqlite-vec capability gaps on some platforms | Fall back to in-memory cosine if `vec0` virtual table fails to load (logged warning) |
| LICENSE compliance | NOTICE file enumerates adopted files; LICENSE-3RD-PARTY copies aeroxy's LICENSE verbatim |

## 12. Out of scope (deferred)

- HTTP transport for MCP server (stdio only)
- Vector index incremental merging (rebuilds on update)
- Real call graph extraction (stays best-effort string-match in v0.2; promote in v0.3)
- Java / Kotlin / Scala IDL parsers (only protobuf + thrift for now)
- Symbol-level imports (file-level only — see Q5 in brainstorming session)
- Web UI / docs site
- Cross-platform release via cargo-dist (planned for v0.2.1)

## 13. Success criteria

The refactor is complete when:

1. `cargo test` passes with at least the same test count as before (62) — actually expected ~85 after additions.
2. `cargo clippy --all-targets -- -D warnings` is clean.
3. `cargo build --release` produces a single binary; `repolayer build` on a 3-repo fixture (single_repo_ts + single_repo_py + idl_repo) succeeds and writes all 4 SQLite files + downloads model on first run.
4. MCP `tools/list` returns 15 tools; an end-to-end `find_context` call returns hybrid-search-ranked results with cross-repo edges in the response.
5. Dogfood: `repolayer build` on the repolayer source tree itself succeeds (requires Rust adapter present).
6. NOTICE file present; README repositioned.

## 14. Implementation order (handed to writing-plans next)

The plan should sequence the work to keep the tree compiling at each milestone. Suggested coarse order:

1. **Skeleton** — add new dependencies, create `core/`, `adapters/base.rs`, empty modules; keep old code compiling alongside.
2. **Adapter migration** — bring in ast-outline adapters one by one (rust → typescript → python → go → others). Each adapter ships with a passing test before the next is added. Old `parser/` deleted after all 4 current languages are covered.
3. **Storage split** — implement outline.db / deps.db / search.db schemas as no-op stubs first, then wire the writer threads.
4. **Indexer rewrite** — new pipeline with rayon, single-writer pattern.
5. **Linker upgrade** — IDL linker uses ast-grep; cross-repo PackageIndex extended.
6. **Query layer** — `find_context` hybrid; `get_symbol` returns Declaration; new `find_idl_impl`.
7. **MCP + CLI** — 9 new tools wired up; schema versioning everywhere.
8. **Install + prompt** — `repolayer install --mcp` and `repolayer prompt`.
9. **Polish** — NOTICE, README rewrite, fixture additions, dogfood self-index.
