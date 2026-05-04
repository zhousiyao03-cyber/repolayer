# Plan C: MCP Tools + Install + Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire all 15 MCP tools (6 repolayer-native + 9 ast-outline-compat), add `repolayer install --mcp` and `repolayer prompt`, upgrade `find_context` to hybrid BM25+dense search, and reposition the project (README + CLAUDE.md). After this plan, repolayer v0.2 is feature-complete and ready for `cargo install` dogfood.

**Architecture:** All ast-outline-compat MCP tools (`outline / show / digest / surface / deps / reverse-deps / cycles / search / find-related`) call into the subsystems built in Plan B (outline.db, deps.db, search.db, surface module). New `find_idl_impl` MCP tool reads the main graph for IdlMethod nodes' inbound `Implements`/`Invokes` edges. CLI commands mirror MCP tool names. `install --mcp <agent>` writes config to the right location per agent (Claude Code / Cursor / Gemini / Codex / Copilot). `repolayer prompt` outputs a stdout snippet teaching agents which tool to call when.

**Tech Stack:** All Plan B deps + `dirs = "5"` (added in Plan A or here, for resolving agent config locations).

**Inputs from prior plans:**
- Plan B complete on branch `feature/ast-outline-ext` at tag `plan-b-complete`
- ~110 tests passing
- Stores: index.db v2, outline.db, deps.db, search.db all populated by `repolayer build`
- 5 native MCP tools still working (find_context, get_symbol, get_callers, get_dependencies, list_repos) — but with new schema
- adapters::parse_file dispatcher
- core::Declaration IR with markers
- Surface subsystem present in src/surface/ (NOT YET ADOPTED — comes in this plan, Task C-3)

**Outputs of this plan:**
- 9 new MCP compat tools registered, all schema-versioned
- `find_idl_impl` new repolayer-native MCP tool
- `find_context` upgraded to hybrid BM25+dense
- `repolayer install --mcp <agent>` for 5 agent types
- `repolayer prompt` stdout snippet
- 9 new CLI subcommands mirroring MCP compat tools
- `surface/` module fully adopted from aeroxy
- README rewritten per spec §2.1; CLAUDE.md updated
- ~125+ tests passing
- Final dogfood: `repolayer build` + connect to Claude Code via MCP works end-to-end

**Out of scope (deferred to v0.2.1+):**
- HTTP transport for MCP (stdio only)
- cargo-dist cross-platform binaries
- Search index incremental merging
- Real call graph (Calls edge population)

---

## File structure (after Plan C — final v0.2)

```
README.md                               # REWRITTEN
CLAUDE.md                               # UPDATED
NOTICE                                  # extended
Cargo.toml                              # may add `dirs`

src/
├── core/                               # Plan A — unchanged
├── adapters/                           # Plan A — unchanged
├── outline/                            # Plan B
├── deps/                                # Plan B
├── search/                              # Plan B
├── surface/                            # NEW (Plan C Task C-3) — adopted from aeroxy
│   ├── mod.rs
│   ├── entry.rs
│   ├── entry_point.rs
│   ├── fallback.rs
│   ├── imports.rs
│   ├── manifest.rs
│   ├── module_graph.rs
│   ├── options.rs
│   ├── render.rs
│   ├── python.rs
│   ├── rust.rs
│   ├── scala.rs
│   └── typescript.rs
├── graph/                              # Plan B
├── linker/                             # Plan B
├── indexer/                            # Plan B
├── llm/                                # unchanged
├── mcp/
│   ├── mod.rs                          # MODIFY — register 10 new tools
│   ├── tools.rs                        # MODIFY — add find_idl_impl + hybrid find_context
│   └── tools_compat.rs                 # NEW — 9 ast-outline-compat tools
├── cli/
│   ├── (existing 5 subcommands)
│   ├── install.rs                      # NEW
│   ├── prompt.rs                       # NEW
│   └── compat/                         # NEW
│       ├── mod.rs
│       ├── outline.rs
│       ├── show.rs
│       ├── digest.rs
│       ├── surface.rs
│       ├── deps.rs
│       ├── reverse_deps.rs
│       ├── cycles.rs
│       ├── search.rs
│       └── find_related.rs
└── query/
    ├── find_context.rs                 # UPGRADE — hybrid search
    ├── find_idl_impl.rs                # NEW
    └── (others unchanged)

tests/
├── (Plans A+B's existing files)
├── mcp_tools_list_15.rs                # NEW
├── compat_outline.rs                   # NEW
├── compat_show.rs                      # NEW
├── compat_digest.rs                    # NEW
├── compat_surface.rs                   # NEW
├── compat_deps.rs                      # NEW
├── compat_reverse_deps.rs              # NEW
├── compat_cycles.rs                    # NEW
├── compat_search.rs                    # NEW
├── compat_find_related.rs              # NEW
├── find_idl_impl.rs                    # NEW
├── find_context_hybrid.rs              # NEW
├── install_mcp.rs                      # NEW
└── prompt_command.rs                   # NEW
```

---

### Task C-1: Hybrid `find_context` upgrade

**Files:**
- Rewrite: `src/query/find_context.rs`
- Test: `tests/find_context_hybrid.rs`
- Modify: `tests/query_find_context.rs` (existing — adjust expected output)

The Plan B `find_context` is still substring-only. Upgrade to: substring + BM25 + dense rerank → augment with cross-repo edges from main graph.

Pipeline:
1. Tokenize `task_description` (existing)
2. Substring search in main graph (existing) → candidate set A
3. `SearchStore::search_hybrid(repo=None, task_description, k=20)` → candidate set B
4. Merge A and B (dedup by (repo, path))
5. For each candidate, attach related cross-repo edges from main graph (Imports / Invokes / Implements) — within budget
6. Token budget enforcement (existing)
7. Return `ContextResult` with `schema_version: "repolayer.find_context.v1"`

```rust
#[derive(Debug, Serialize)]
pub struct ContextResult {
    pub schema_version: &'static str,  // "repolayer.find_context.v1"
    pub items: Vec<ContextItem>,
    pub total_tokens: u32,
    pub suggestion: String,
}

#[derive(Debug, Serialize)]
pub struct ContextItem {
    pub repo: String,
    pub path: String,
    pub symbol: Option<String>,
    pub summary: Option<String>,
    pub relevance_score: f32,
    pub match_source: MatchSource,    // NEW: substring | bm25 | dense | fusion
    pub confidence: f32,              // NEW: 0..1
    pub call_chain: Option<Vec<String>>,
    pub estimated_tokens: u32,        // NEW: per-item token estimate
    pub cross_repo_edges: Vec<EdgeRef>, // NEW
}

#[derive(Debug, Serialize)]
pub struct EdgeRef {
    pub kind: String,           // "Imports" / "Invokes" / "Implements"
    pub target_repo: String,
    pub target_path: String,
    pub target_symbol: Option<String>,
    pub confidence: f32,
}
```

Test: build a multi-repo fixture, query `find_context("user authentication")`, verify hits include both substring matches AND semantically-similar files (e.g., a `login.ts` even though "authentication" doesn't substring-match its content).

```bash
git add src/query/find_context.rs tests/find_context_hybrid.rs tests/query_find_context.rs
git commit -m "feat(query): hybrid find_context (substring + BM25 + dense rerank + edges)"
```

---

### Task C-2: New `find_idl_impl` query + MCP tool

**Files:**
- Create: `src/query/find_idl_impl.rs`
- Modify: `src/mcp/tools.rs`
- Modify: `src/mcp/mod.rs`
- Create: `tests/find_idl_impl.rs`

Given an IDL method (qualified `ServiceName.MethodName` or just `MethodName` with optional `service` filter), return:
- The IdlMethod node (defining IDL repo, .proto/.thrift path, line)
- All inbound `Implements` edges (server-side modules) with confidence
- All inbound `Invokes` edges (client-side modules) with confidence
- Sort by confidence descending so high-confidence ast-grep matches lead

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindIdlImplArgs {
    pub method: String,              // "UserService.GetUser" or just "GetUser"
    #[serde(default)]
    pub service: Option<String>,
    #[serde(default)]
    pub include_invokes: bool,       // default true
    #[serde(default)]
    pub include_implements: bool,    // default true
}

#[derive(Debug, Serialize)]
pub struct FindIdlImplResult {
    pub schema_version: &'static str,
    pub method: IdlMethodInfo,
    pub implements: Vec<ImplLocation>,
    pub invokes: Vec<ImplLocation>,
}

#[derive(Debug, Serialize)]
pub struct ImplLocation {
    pub repo: String,
    pub path: String,
    pub symbol: Option<String>,
    pub confidence: f32,
}
```

MCP tool wiring in `src/mcp/mod.rs`:

```rust
#[rmcp::tool(
    description = "Find code modules that implement (server-side) or invoke (client-side) a given IDL method across all indexed repos."
)]
fn find_idl_impl(
    &self,
    Parameters(args): Parameters<FindIdlImplArgs>,
) -> Result<CallToolResult, rmcp::ErrorData> {
    into_result(self.tools.find_idl_impl(args))
}
```

Test: 3-repo fixture with `idl_repo` (declares `UserService.GetUser`), `bff_repo` (invokes), `backend_repo` (implements via path heuristic). Verify both come back with correct confidence levels.

```bash
git add src/query/find_idl_impl.rs src/mcp/ tests/find_idl_impl.rs
git commit -m "feat(query): find_idl_impl tool — IDL method to code implementations across repos"
```

---

### Task C-3: Adopt `surface/` subsystem

**Files:**
- Adopt: `src/surface/{mod,entry,entry_point,fallback,imports,manifest,module_graph,options,render,python,rust,scala,typescript}.rs` from aeroxy
- Test: `tests/compat_surface.rs`

`surface` resolves a package's true public API by following `pub use` re-exports (Rust), `__all__` (Python), barrel files (TS), `export` clauses (Scala). Read by Plan C's `surface` MCP tool and CLI command.

```bash
mkdir -p src/surface
for f in mod entry entry_point fallback imports manifest module_graph options render python rust scala typescript; do
  curl -fL "https://raw.githubusercontent.com/aeroxy/ast-outline/main/src/surface/${f}.rs" > "src/surface/${f}.rs"
done

# Mass substitution (the same `crate::core::` and possibly `crate::deps::` paths)
for f in src/surface/*.rs; do
  sed -i.bak \
    -e 's|crate::core::|crate::core::declaration::|g' \
    "$f"
  rm "${f}.bak"
done
```

Edit `src/lib.rs` to add `pub mod surface;`.

Cargo build to surface any unresolved deps (e.g. surface may reference deps:: types — those exist after Plan B).

Test: build a fixture Rust crate with `pub use` re-exports; verify `surface` returns the re-exported items rather than the internal modules.

```bash
git add src/surface/ src/lib.rs tests/compat_surface.rs
git commit -m "feat(surface): adopt surface subsystem (pub-use / __all__ / barrel resolution)"
```

---

### Task C-4: Compat tool — `outline`

**Files:**
- Create: `src/cli/compat/mod.rs`, `src/cli/compat/outline.rs`
- Modify: `src/cli/mod.rs` — add `Outline { ... }` subcommand
- Modify: `src/mcp/tools_compat.rs` (new file) — add outline tool
- Modify: `src/mcp/mod.rs` — register outline tool
- Test: `tests/compat_outline.rs`

CLI form:

```bash
repolayer outline path/to/file.rs              # one file
repolayer outline src/                          # whole dir
repolayer outline --json src/foo.rs             # JSON
repolayer outline --imports src/                # show imports
```

MCP form: `outline(paths: Vec<String>, options: OutlineOptions, json: bool)` returns text or JSON.

Implementation:
1. For each path: if a file → look up in outline.db (or parse on-demand if missing); if a dir → walk + list entries
2. `outline::render::render_outline(&pr, &options)` for human-readable
3. JSON: serialize the `Vec<ParseResult>` directly (the `Declaration` tree already serializes via Plan A markers)
4. Both forms include `schema_version: "ast-outline.outline.v1"`

Tests:
- `compat_outline.rs`: outline a fixture file, verify text output contains expected signatures
- Same fixture, JSON form, verify schema_version field

```bash
git add src/cli/compat/ src/mcp/tools_compat.rs src/mcp/mod.rs src/cli/mod.rs tests/compat_outline.rs
git commit -m "feat(compat): outline command (CLI + MCP)"
```

---

### Task C-5: Compat tool — `show`

**Files:**
- Create: `src/cli/compat/show.rs`
- Modify: `src/mcp/tools_compat.rs`, `src/mcp/mod.rs`
- Test: `tests/compat_show.rs`

`repolayer show <file> <Symbol> [<Symbol>...]` — extract source body of one or more symbols.

Implementation: load file's `Vec<Declaration>` from outline.db, run `find_symbols(symbol)` (from outline/render.rs adopted in Plan B Task B-3), slice the source bytes by `start_byte..end_byte`, return concatenated.

```bash
git add src/cli/compat/show.rs src/mcp/tools_compat.rs src/mcp/mod.rs tests/compat_show.rs
git commit -m "feat(compat): show command (CLI + MCP)"
```

---

### Task C-6: Compat tool — `digest`

**Files:**
- Create: `src/cli/compat/digest.rs`
- Modify: `src/mcp/tools_compat.rs`, `src/mcp/mod.rs`
- Test: `tests/compat_digest.rs`

Module-level digest: walk outline.db rows for the given path prefix, format with `outline::render::render_digest`.

```bash
git add src/cli/compat/digest.rs src/mcp/tools_compat.rs src/mcp/mod.rs tests/compat_digest.rs
git commit -m "feat(compat): digest command (CLI + MCP)"
```

---

### Task C-7: Compat tool — `surface`

**Files:**
- Create: `src/cli/compat/surface.rs`
- Modify: `src/mcp/tools_compat.rs`, `src/mcp/mod.rs`
- Test: `tests/compat_surface.rs` (already created in C-3)

`repolayer surface [path]` — auto-detect Cargo.toml / pyproject / package.json / __init__.py and resolve published API.

Wraps `surface::render::render_surface(...)` from C-3.

```bash
git add src/cli/compat/surface.rs src/mcp/tools_compat.rs src/mcp/mod.rs
git commit -m "feat(compat): surface command (CLI + MCP)"
```

---

### Task C-8: Compat tool — `deps`

**Files:**
- Create: `src/cli/compat/deps.rs`
- Modify: `src/mcp/tools_compat.rs`, `src/mcp/mod.rs`
- Test: `tests/compat_deps.rs`

`repolayer deps <path> [--depth N]` — forward dependencies. Reads `deps.db` via `DepStore::load_repo_graph`.

Args: path can be a file (gives that file's imports) or a repo root (gives entire graph). Optional depth parameter for transitive expansion.

```bash
git add src/cli/compat/deps.rs src/mcp/tools_compat.rs src/mcp/mod.rs tests/compat_deps.rs
git commit -m "feat(compat): deps command (CLI + MCP)"
```

---

### Task C-9: Compat tool — `reverse-deps`

**Files:**
- Create: `src/cli/compat/reverse_deps.rs`
- Modify: `src/mcp/tools_compat.rs`, `src/mcp/mod.rs`
- Test: `tests/compat_reverse_deps.rs`

`repolayer reverse-deps <path>` — uses `DepGraph::reverse_of(path)`. The "blast radius" tool.

```bash
git add src/cli/compat/reverse_deps.rs src/mcp/tools_compat.rs src/mcp/mod.rs tests/compat_reverse_deps.rs
git commit -m "feat(compat): reverse-deps command (CLI + MCP)"
```

---

### Task C-10: Compat tool — `cycles`

**Files:**
- Create: `src/cli/compat/cycles.rs`
- Modify: `src/mcp/tools_compat.rs`, `src/mcp/mod.rs`
- Test: `tests/compat_cycles.rs`

`repolayer cycles [path]` — runs Tarjan SCC over the dep graph (already in `deps/scc.rs` from Plan B). Exit code: 0 if no cycles, 1 if any (so it can be a CI gate).

```bash
git add src/cli/compat/cycles.rs src/mcp/tools_compat.rs src/mcp/mod.rs tests/compat_cycles.rs
git commit -m "feat(compat): cycles command (CLI + MCP, CI-gateable exit code)"
```

---

### Task C-11: Compat tool — `search`

**Files:**
- Create: `src/cli/compat/search.rs`
- Modify: `src/mcp/tools_compat.rs`, `src/mcp/mod.rs`
- Test: `tests/compat_search.rs`

`repolayer search "<query>" [-k N]` — wraps `SearchStore::search_hybrid` from Plan B Task B-19. CLI human-readable output (top-k chunks with snippets); MCP JSON schema-versioned.

```bash
git add src/cli/compat/search.rs src/mcp/tools_compat.rs src/mcp/mod.rs tests/compat_search.rs
git commit -m "feat(compat): search command (BM25 + dense fusion CLI + MCP)"
```

---

### Task C-12: Compat tool — `find-related`

**Files:**
- Create: `src/cli/compat/find_related.rs`
- Modify: `src/mcp/tools_compat.rs`, `src/mcp/mod.rs`
- Test: `tests/compat_find_related.rs`

`repolayer find-related <file>:<line>` — embeds the chunk at that location, queries SearchStore for similar embeddings (cosine), returns top-k with dep-graph-aware boost.

The dep-graph boost: if a result file has any (forward or reverse) dep edge to the source file, multiply its score by 1.2 (constant). Reuses `DepStore::load_repo_graph`.

```bash
git add src/cli/compat/find_related.rs src/mcp/tools_compat.rs src/mcp/mod.rs tests/compat_find_related.rs
git commit -m "feat(compat): find-related command (semantic similarity + dep-graph boost)"
```

---

### Task C-13: Verify MCP tools/list returns 15 tools

**Files:**
- Test: `tests/mcp_tools_list_15.rs` (new — extends mcp_e2e.rs which checked 5)

End-to-end: spawn `repolayer serve`, send tools/list request, verify response has 15 tools with the expected names:

```rust
let expected_names = vec![
    // 6 native
    "find_context", "get_symbol", "get_callers", "get_dependencies",
    "list_repos", "find_idl_impl",
    // 9 compat
    "outline", "show", "digest", "surface",
    "deps", "reverse_deps", "cycles", "search", "find_related",
];
```

Each tool should also have a non-empty inputSchema with `$schema` field present.

```bash
git add tests/mcp_tools_list_15.rs
git commit -m "test(mcp): verify 15 tools listed by MCP server (6 native + 9 compat)"
```

---

### Task C-14: `repolayer install --mcp <agent>` command

**Files:**
- Create: `src/cli/install.rs`
- Modify: `src/cli/mod.rs` — add Install subcommand
- Test: `tests/install_mcp.rs`

5 supported agents:

| Agent | Config location |
|---|---|
| claude-code | `~/Library/Application Support/Claude/claude_desktop_config.json` (macOS) / equivalent on Linux/Windows |
| cursor | `~/.cursor/mcp.json` |
| gemini | `~/.config/gemini-cli/config.json` |
| codex | `~/.config/codex/mcp.json` |
| copilot | VS Code settings.json `"github.copilot.mcp.servers"` key |

For each agent, the command:
1. Opens the file (creates parent dirs if needed)
2. Parses existing JSON (or starts `{}`)
3. Adds/replaces a `repolayer` entry under the right key
4. Backs up the original to `<file>.bak.<timestamp>`
5. Writes back, pretty-printed
6. Exit code 0 on success, 1 if existing config malformed

Use `dirs::home_dir()` for cross-platform home resolution.

```rust
pub fn run(agent: &str) -> anyhow::Result<()> {
    let exe_path = std::env::current_exe()?;
    match agent {
        "claude-code" => install_claude_code(&exe_path),
        "cursor" => install_cursor(&exe_path),
        // ...
        _ => anyhow::bail!("unknown agent: {} (try claude-code / cursor / gemini / codex / copilot)", agent),
    }
}
```

CLI:
```bash
repolayer install --mcp claude-code
repolayer install --mcp cursor
```

Test: use `tempfile::tempdir()` to redirect `$HOME`, run install, verify the JSON file is written correctly.

```bash
git add src/cli/install.rs src/cli/mod.rs tests/install_mcp.rs
git commit -m "feat(cli): install --mcp <agent> for Claude Code / Cursor / Gemini / Codex / Copilot"
```

---

### Task C-15: `repolayer prompt` command

**Files:**
- Create: `src/cli/prompt.rs`
- Modify: `src/cli/mod.rs`
- Test: `tests/prompt_command.rs`

Outputs a markdown snippet to stdout that teaches an agent which tool to call when. Embedded as a `const PROMPT: &str = "..."` in source.

```markdown
## repolayer — multi-repo code navigation

This workspace is indexed by `repolayer`. Prefer these tools over reading
files directly:

### Cross-repo navigation
- "Where do I start for task X?" → `find_context(task_description, budget_tokens)`
- "Who calls this symbol?" → `get_callers(symbol, depth=2)`
- "What depends on this file?" → `reverse_deps(path)`
- "Show me an IDL method's implementations" → `find_idl_impl(method)`

### Single-file structure
- "What's in this file?" → `outline(paths)` (signatures, no bodies)
- "Show me this method's source" → `show(path, symbol)`
- "What's this directory contain?" → `digest(path)`
- "What's this package's public API?" → `surface(path)`

### Search
- "Find code about X" → `search(query, k)` (BM25 + semantic)
- "Find code similar to file:line" → `find_related(path:line)`

### Dependency graph
- "What does this file import?" → `deps(path)`
- "Are there import cycles?" → `cycles()`

Don't read whole files when an outline / show / search call would do.
```

CLI:
```bash
repolayer prompt >> CLAUDE.md
repolayer prompt >> AGENTS.md
```

```bash
git add src/cli/prompt.rs src/cli/mod.rs tests/prompt_command.rs
git commit -m "feat(cli): prompt command outputs agent-steering snippet"
```

---

### Task C-16: README rewrite + NOTICE finalization

**Files:**
- Rewrite: `README.md`
- Extend: `NOTICE`

README structure (per spec §2.1 wording at top):

```markdown
# repolayer

> repolayer = ast-outline (aeroxy/ast-outline) + cross-repo graph + IDL linking + MCP server tailored for multi-repo agent workflows.

Built on top of [aeroxy/ast-outline](https://github.com/aeroxy/ast-outline)'s
parsing, IR, dep-graph, and hybrid search. Extends with: multi-repo workspace
model, IDL (protobuf/thrift) as first-class graph nodes, cross-repo import
resolution, manual cross-repo links, and 6 MCP tools focused on multi-repo
navigation in addition to the 9 inherited from ast-outline.

## Status: v0.2 alpha

15 MCP tools, 4 SQLite stores, hybrid BM25+dense semantic search,
single static binary.

## When to use repolayer vs ast-outline

- **Single repo, single agent, just want outline / search** → use [ast-outline](https://github.com/aeroxy/ast-outline) directly. Smaller binary, no index to maintain.
- **Multi-repo workspace, microservice with IDL contracts, agent that needs cross-repo navigation** → repolayer is the natural extension.

## Install

cargo install --path .  (cargo-dist binaries: planned v0.2.1)

## Quickstart

[normal usage section, 15 commands listed]

## Connecting to Claude Code

repolayer install --mcp claude-code

[etc — covers cursor / gemini / codex / copilot]

## Architecture

[link to docs/superpowers/specs/2026-05-04-ast-outline-extension-design.md]

## License

MIT. See NOTICE for adopted components.
```

NOTICE: extend with surface and search adoption (already noted broadly in Plan A's NOTICE; tighten exact file list).

```bash
git add README.md NOTICE
git commit -m "docs: rewrite README for v0.2 (cross-repo extension of ast-outline)"
```

---

### Task C-17: CLAUDE.md update

**Files:**
- Rewrite: `CLAUDE.md`

The current CLAUDE.md describes Plan A baseline. Update for v0.2:
- Architecture: 4 SQLite + 1 model cache + 10 adapters + IDL retained
- Commands: full 16 subcommands
- Module map: includes outline/, deps/, search/, surface/
- Test structure: ~125 tests
- Conventions: keep, add note about the kotlin local patch in NOTICE

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md for v0.2 architecture"
```

---

### Task C-18: Plan C wrap-up + final dogfood

- [ ] **Step C18.1: Cargo install + run from system**

```bash
cd /Users/bytedance/code/repolayer/.worktrees/ast-outline-ext
cargo install --path .
which repolayer && repolayer --version
```

- [ ] **Step C18.2: Index repolayer itself**

```bash
WS=$(mktemp -d)
cd "$WS"
cat > repolayer.yml <<EOF
repos:
  - path: /Users/bytedance/code/repolayer/.worktrees/ast-outline-ext
EOF
repolayer build
sqlite3 .repolayer/index.db "SELECT kind, COUNT(*) FROM nodes GROUP BY kind"
```

Expect: ≥ 1 repo, ≥ 50 modules, ≥ 100 types, ≥ 200 methods (rough estimate for repolayer's own ~5000-line codebase).

- [ ] **Step C18.3: Test all 15 MCP tools end-to-end**

```bash
cd "$WS"
echo "=== tools/list ==="
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"smoketest","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  | repolayer serve 2>/dev/null | tail -1 | python3 -c "import sys, json; r = json.load(sys.stdin); print(len(r['result']['tools']), 'tools'); [print('  -', t['name']) for t in r['result']['tools']]"
```

Expected: `15 tools` followed by all 15 names.

For each high-value tool (`find_context`, `find_idl_impl`, `outline`, `surface`), run a real call:

```bash
echo "=== find_context ==="
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"smoke","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"find_context","arguments":{"task_description":"parser dispatch"}}}' \
  | repolayer serve 2>/dev/null | tail -1 | python3 -m json.tool | head -30
```

- [ ] **Step C18.4: Test `repolayer install --mcp claude-code`**

```bash
# Backup any existing config first
cp ~/Library/Application\ Support/Claude/claude_desktop_config.json /tmp/claude_config_backup_$(date +%s).json 2>/dev/null || true
repolayer install --mcp claude-code
cat ~/Library/Application\ Support/Claude/claude_desktop_config.json | python3 -m json.tool | head -20
```

Expected: a `repolayer` entry under `mcpServers`. Restore the backup if you don't want to keep it.

- [ ] **Step C18.5: Tag final**

```bash
cd /Users/bytedance/code/repolayer/.worktrees/ast-outline-ext
git tag -a v0.2.0-alpha -m "v0.2.0-alpha: ast-outline extension complete

15 MCP tools (6 native + 9 compat). 4 SQLite stores
(index/outline/deps/search). Hybrid BM25 + dense search via
potion-code-16M. 10 source-language adapters via ast-grep-core.
Repolayer-original cross-repo + IDL graph. Single 25-30 MB binary."
```

- [ ] **Step C18.6: Final summary**

```bash
echo "=== Plan C summary ==="
echo "Tests:" && cargo test --no-fail-fast 2>&1 | grep -E "^test result:" | awk '{p+=$4; f+=$6} END {print p, "passed,", f, "failed"}'
echo "Binary:" && ls -lh target/release/repolayer
echo "MCP tools:" && cargo run --release -- serve <<EOF 2>/dev/null | tail -1 | python3 -c "import sys, json; print(len(json.load(sys.stdin)['result']['tools']))"
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"x","version":"0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
EOF
echo "CLI subcommands:" && repolayer --help 2>&1 | grep -c "^  [a-z]"
echo "Files in src/ (top-level):" && ls src/ | wc -l
```

Expected:
- 125+ tests passing, 0 failed
- Binary 25-30 MB
- 15 MCP tools
- 16 CLI subcommands (init/build/update/query/serve/install/prompt + 9 compat)
- ~13 src/ top-level dirs (cli/core/adapters/outline/deps/search/surface/graph/linker/indexer/llm/mcp/query + lib.rs/main.rs)

---

## Self-review checklist

**1. Spec coverage:** Plan C completes spec §8 (15 MCP tools), §9 (CLI subcommands incl. install/prompt), §2.1 (README rewrite), §13 (success criteria — all 6 met by C-18). NOTICE file matches §2.2.

**2. Placeholder scan:** Each tool task (C-4 through C-12) follows the same template; the template body is concrete (file paths, MCP wiring snippet, test approach). The compat tools wrap functions that already exist after Plan B (outline/render, deps/store, search/store, surface), so implementation is mechanical.

**3. Type consistency:** `ContextResult` (C-1), `FindIdlImplResult` (C-2), `SearchHit` (referenced from Plan B), `Declaration` (Plan A) all consistent. `schema_version: "<id>.v1"` everywhere via core::schema constants from Plan A.

**4. Dependency direction (acyclic check):**
- cli/compat → query → outline+deps+search+surface (subsystems)
- mcp/tools_compat → query → subsystems
- install → no upper deps (just stdlib + dirs + serde_json)
- No cycles introduced.

**5. Test budget realism:** Plan C adds 13 new test files (1 per compat tool + tools_list + idl_impl + install + prompt + hybrid context). Each is a small e2e test. Total expected ~125 = Plan B's 110 + 15 new.

---

## Handoff to user

After Plan C completion, the merge story is:

```
master
  ↑
  └── feature/ast-outline-ext (worktree)
         ├── tag plan-a-complete (parser foundation)
         ├── tag plan-b-complete (storage + indexer)
         └── tag v0.2.0-alpha (this plan, complete)
```

User decides:
1. Merge feature/ast-outline-ext to master?
2. Cargo publish v0.2.0-alpha to crates.io?
3. Set up cargo-dist for cross-platform binaries (deferred)?

Use `superpowers:finishing-a-development-branch` skill at that point.
