# Plan B: Storage + Indexer + Linker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the legacy single-SQLite indexer with a rayon-parallel pipeline writing to four independent stores (`index.db` upgraded, `outline.db` new, `deps.db` new, `search.db` new). Use `core::Declaration` (from Plan A) as the parser output everywhere. Upgrade the IDL linker from string-contains heuristic to ast-grep call-pattern matching. Extend the cross-repo package resolver beyond TypeScript to Rust (Cargo workspace), Go (`go.mod`), and Python (`pyproject.toml`). Delete the legacy `src/parser/` module.

**Architecture:** Build pipeline becomes `walk → rayon parse → mpsc channel → single writer per store`. Each SQLite file has its own `meta.schema_version` row and is independently invalidatable. The main graph (`index.db`) keeps cross-repo and IDL relations as today but with a richer node model (`Type` / `Method` / `Function` replacing flat `Symbol`). The dep graph in `deps.db` is wholly adopted from aeroxy `src/deps/`. The search index in `search.db` is wholly adopted from aeroxy `src/search/` (with the `repo` column added on every row for multi-repo support). The hybrid search model (potion-code-16M, ~64 MB) is downloaded on first build to `.repolayer/models/`.

**Tech Stack:** Rust 2021, `ast-grep-core` (already added), `rayon = "1.10"` (added in Plan A's Cargo.toml — verify), `rusqlite` (already), `sqlite-vec` (already), `xxhash-rust` (added in Plan A), `tokenizers + safetensors + memmap2 + wide` (added in Plan A — used by adopted search code), `git2 = "0.19"` (already), `toml_edit = "0.22"` (added) for Cargo.toml / pyproject.toml parsing.

**Inputs from prior plan:**
- Plan A complete on branch `feature/ast-outline-ext` at tag `plan-a-complete`
- 95 tests passing
- `core::Declaration` IR + 10 adapters + `adapters::parse_file` dispatcher all working
- Old `src/parser/` still exists alongside (will be deleted in Task B-25)

**Outputs of this plan:**
- 3 new SQLite store modules (`outline/`, `deps/`, `search/` adopted from aeroxy with `repo` extension)
- Indexer rewritten as rayon-parallel pipeline
- IDL linker upgraded with ast-grep matching, edge confidence levels
- Cross-repo PackageIndex extended to Cargo / go.mod / pyproject
- Old `src/parser/` deleted (4 source-language parsers + treesitter helpers)
- Several existing tests rewritten for new schemas (~8 files)
- Estimated ~110 tests passing, 0 failed
- Dogfood: `repolayer build` on a 3-repo fixture produces all 4 SQLite files + downloads model

**Out of scope (deferred to Plan C):**
- 9 ast-outline-compat MCP tools (outline / show / digest / surface / deps / reverse-deps / cycles / search / find-related)
- `repolayer install --mcp <agent>` and `repolayer prompt`
- README / CLAUDE.md repositioning
- find_idl_impl new MCP tool

---

## File structure (after Plan B)

```
NOTICE                                  # Plan A
Cargo.toml                              # MODIFY — add toml_edit
src/
├── lib.rs                              # MODIFY — add new modules, remove parser
├── core/                               # Plan A — unchanged
├── adapters/                           # Plan A — unchanged
├── parser/                             # DELETED (Task B-25)
├── config/                             # MODIFY — slight extension for new repo types
├── graph/                              # REWRITE — model.rs + store.rs schema v2
│   ├── mod.rs
│   ├── model.rs                        # NEW NodeKind/EdgeKind enums
│   └── store.rs                        # schema v2; meta.schema_version
├── outline/                            # NEW — adopted from aeroxy outline
│   ├── mod.rs
│   ├── store.rs                        # SQLite for Declaration trees
│   └── render.rs                       # outline/show/digest formatters (used in Plan C)
├── deps/                               # NEW — adopted wholesale from aeroxy src/deps/
│   ├── mod.rs
│   ├── extract.rs
│   ├── resolver/
│   │   ├── mod.rs
│   │   ├── suffix.rs
│   │   └── path.rs
│   ├── graph.rs
│   ├── manifest.rs                     # extended for Cargo / go.mod / pyproject
│   ├── cache.rs
│   ├── scc.rs
│   ├── dsm.rs
│   ├── store.rs                        # NEW — wraps cache.rs in SQLite
│   ├── options.rs
│   ├── render.rs
│   └── traverse.rs
├── search/                             # NEW — adopted wholesale from aeroxy src/search/
│   ├── mod.rs
│   ├── chunker.rs
│   ├── bm25.rs
│   ├── embed.rs
│   ├── download.rs
│   ├── ranking.rs
│   ├── fusion.rs
│   ├── index.rs
│   ├── cache.rs
│   ├── tokens.rs
│   ├── format.rs
│   └── store.rs                        # NEW — wraps cache.rs in SQLite (with repo col)
├── linker/                             # MODIFY
│   ├── mod.rs
│   ├── imports.rs                      # EXTEND: Cargo / go.mod / pyproject
│   ├── idl_links.rs                    # UPGRADE: ast-grep call patterns
│   └── manual.rs                       # unchanged
├── indexer/                            # REWRITE
│   ├── mod.rs                          # rayon pipeline (parse → mpsc → writers)
│   └── incremental.rs                  # adapt to new IR + multi-store
├── llm/                                # unchanged
├── mcp/                                # unchanged in Plan B (Plan C extends)
├── cli/                                # MODIFY: build/update wire to new pipeline
└── query/                              # MODIFY: read from new schema
    └── ... (find_context etc — minor adaptations)

tests/
├── (all existing 19 + 12 from Plan A)
├── new schema:
│   ├── graph_schema_v2.rs              # NEW: index.db v2 model
│   ├── outline_store.rs                # NEW
│   ├── deps_store.rs                   # NEW
│   ├── search_store.rs                 # NEW
│   ├── multi_store_build.rs            # NEW: full pipeline e2e
│   └── manifest_resolution.rs          # NEW: cargo/go.mod/pyproject
├── replaced:
│   ├── cli_build.rs                    # rewritten
│   ├── cli_query.rs                    # rewritten (minor)
│   ├── cli_update.rs                   # rewritten
│   ├── graph_model.rs                  # rewritten for new NodeKind
│   ├── graph_store.rs                  # rewritten for v2 schema
│   ├── idl_linking.rs                  # extended for confidence levels
│   ├── multi_repo_linking.rs           # adapted
│   ├── query_find_context.rs           # rewritten (still substring; hybrid in Plan C)
│   └── query_others.rs                 # rewritten
├── deleted (old parsers gone):
│   ├── parser_typescript.rs            # DELETED in Task B-25
│   ├── parser_python.rs                # DELETED
│   └── parser_go.rs                    # DELETED
└── kept:
    ├── parser_protobuf.rs              # IDL still bare tree-sitter
    ├── parser_thrift.rs                # IDL still bare tree-sitter
    ├── manual_links.rs                 # only minor adaptation
    ├── llm_*                           # unchanged
    ├── mcp_e2e.rs                      # tools/list still 5 (Plan C bumps to 15)
    ├── adapter_*                       # Plan A — unchanged
    └── core_*                          # Plan A — unchanged
```

---

## Numbering

Plan A's 16 tasks were numbered 0-15. Plan B continues at B-1 to avoid collision in commit messages. The TaskCreate tool will get them as Task #17 onward in sequence.

---

### Task B-1: Add `toml_edit` dep + new module skeletons

**Files:**
- Modify: `Cargo.toml`
- Create: `src/outline/mod.rs`, `src/deps/mod.rs`, `src/search/mod.rs`, `src/graph/mod.rs` (will be rewritten)
- Modify: `src/lib.rs`

This task only adds skeletons so subsequent tasks can land their content. We do NOT yet remove `src/parser/` — that's Task B-25.

- [ ] **Step B1.1: Add `toml_edit` dep**

Edit `Cargo.toml` `[dependencies]`, add:

```toml
toml_edit = "0.22"
```

Verify: `cargo build 2>&1 | tail -3` succeeds.

- [ ] **Step B1.2: Create stub `src/outline/mod.rs`**

```rust
//! Per-file Declaration tree storage.
//! Adopted in design from aeroxy/ast-outline; SQLite-backed in repolayer.
//! Fully implemented in Tasks B-2 to B-4.

pub mod store;
pub mod render;
```

- [ ] **Step B1.3: Create stub `src/outline/store.rs`**

```rust
// Implementation lands in Task B-2.
```

- [ ] **Step B1.4: Create stub `src/outline/render.rs`**

```rust
// Implementation lands in Plan C (outline/show/digest commands).
// This stub exists only so cargo build doesn't fail.
```

- [ ] **Step B1.5: Create stub `src/deps/mod.rs`**

```rust
//! File-level dependency graph (forward + reverse + cycles + DSM).
//! Adopted from aeroxy/ast-outline `src/deps/`. Fully implemented in
//! Tasks B-5 to B-9.
```

- [ ] **Step B1.6: Create stub `src/search/mod.rs`**

```rust
//! Hybrid BM25 + dense embedding search index.
//! Adopted from aeroxy/ast-outline `src/search/`. Fully implemented in
//! Tasks B-13 to B-19.
```

- [ ] **Step B1.7: Add new modules to `src/lib.rs`**

Open `src/lib.rs`. After `pub mod adapters;` line, insert:

```rust
pub mod outline;
pub mod deps;
pub mod search;
```

(`pub mod graph;`, `pub mod indexer;`, `pub mod parser;` etc remain.)

- [ ] **Step B1.8: Verify cargo build**

```bash
cd /Users/bytedance/code/repolayer/.worktrees/ast-outline-ext
cargo build 2>&1 | tail -5
```

Expected: clean. May warn about unused `pub mod render;` etc — fine.

- [ ] **Step B1.9: Verify all 95 tests still pass**

```bash
cargo test --no-fail-fast 2>&1 | grep -E "^test result:" | awk '{p+=$4; f+=$6} END {print p, "passed,", f, "failed"}'
```

Expected: 95 passed, 0 failed.

- [ ] **Step B1.10: Commit**

```bash
git add Cargo.toml Cargo.lock src/outline/ src/deps/ src/search/ src/lib.rs
git commit -m "chore(plan-b): scaffold outline/, deps/, search/ modules + toml_edit dep"
```

---

### Task B-2: outline.db schema and Store API

**Files:**
- Modify: `src/outline/store.rs`
- Test: `tests/outline_store.rs`

`outline.db` holds one row per indexed source file: language, line count, parse error count, content hash, and the JSON-serialised `Vec<Declaration>` tree. Used by Plan C's outline/show/digest tools and by `find_context` (Plan C upgrade) for context expansion.

- [ ] **Step B2.1: Write the failing test**

Create `tests/outline_store.rs`:

```rust
use repolayer::core::declaration::{Declaration, DeclarationKind, ParseResult};
use repolayer::outline::store::OutlineStore;
use std::path::PathBuf;
use tempfile::tempdir;

fn make_parse_result() -> ParseResult {
    ParseResult {
        path: PathBuf::from("src/foo.rs"),
        language: "rust",
        source: b"pub fn foo() {}".to_vec(),
        line_count: 1,
        error_count: 0,
        declarations: vec![Declaration {
            kind: DeclarationKind::Function,
            name: "foo".into(),
            signature: "pub fn foo()".into(),
            ..Default::default()
        }],
    }
}

#[test]
fn create_and_open_writes_schema_version() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("outline.db");
    let store = OutlineStore::open(&path).unwrap();
    assert_eq!(store.schema_version().unwrap(), 1);
}

#[test]
fn upsert_and_get_roundtrips_declarations() {
    let dir = tempdir().unwrap();
    let store = OutlineStore::open(&dir.path().join("outline.db")).unwrap();
    let pr = make_parse_result();
    store.upsert("repo1", &pr, &[0u8; 32]).unwrap();
    let got = store.get("repo1", "src/foo.rs").unwrap().unwrap();
    assert_eq!(got.language, "rust");
    assert_eq!(got.declarations.len(), 1);
    assert_eq!(got.declarations[0].name, "foo");
}

#[test]
fn upsert_replaces_on_same_key() {
    let dir = tempdir().unwrap();
    let store = OutlineStore::open(&dir.path().join("outline.db")).unwrap();
    let mut pr = make_parse_result();
    store.upsert("repo1", &pr, &[0u8; 32]).unwrap();
    pr.declarations[0].name = "bar".into();
    store.upsert("repo1", &pr, &[1u8; 32]).unwrap();
    let got = store.get("repo1", "src/foo.rs").unwrap().unwrap();
    assert_eq!(got.declarations[0].name, "bar");
}

#[test]
fn delete_removes_row() {
    let dir = tempdir().unwrap();
    let store = OutlineStore::open(&dir.path().join("outline.db")).unwrap();
    store.upsert("repo1", &make_parse_result(), &[0u8; 32]).unwrap();
    store.delete("repo1", "src/foo.rs").unwrap();
    assert!(store.get("repo1", "src/foo.rs").unwrap().is_none());
}

#[test]
fn list_files_filtered_by_repo() {
    let dir = tempdir().unwrap();
    let store = OutlineStore::open(&dir.path().join("outline.db")).unwrap();
    let pr = make_parse_result();
    store.upsert("repo1", &pr, &[0u8; 32]).unwrap();
    let mut pr2 = make_parse_result();
    pr2.path = PathBuf::from("src/bar.rs");
    store.upsert("repo2", &pr2, &[0u8; 32]).unwrap();
    let r1 = store.list_files("repo1").unwrap();
    assert_eq!(r1.len(), 1);
    assert_eq!(r1[0].1, "src/foo.rs"); // (repo, path)
}
```

- [ ] **Step B2.2: Verify test fails**

```bash
cargo test --test outline_store 2>&1 | tail -5
```

Expected: compile error: unresolved import `repolayer::outline::store::OutlineStore`.

- [ ] **Step B2.3: Implement `src/outline/store.rs`**

```rust
//! SQLite-backed store for per-file Declaration trees.
//!
//! Schema v1: one row per (repo, path), holds language, parse error
//! count, content hash, and JSON-serialised Declarations.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::Path;

use crate::core::declaration::{Declaration, ParseResult};

const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS files (
    repo            TEXT NOT NULL,
    path            TEXT NOT NULL,
    language        TEXT NOT NULL,
    line_count      INTEGER NOT NULL,
    parse_errors    INTEGER NOT NULL DEFAULT 0,
    declarations    TEXT NOT NULL,
    content_hash    BLOB NOT NULL,
    PRIMARY KEY (repo, path)
);
CREATE INDEX IF NOT EXISTS idx_outline_files_repo ON files(repo);
"#;

pub struct OutlineStore {
    conn: Connection,
}

#[derive(Debug, Clone)]
pub struct OutlineEntry {
    pub repo: String,
    pub path: String,
    pub language: String,
    pub line_count: usize,
    pub parse_errors: usize,
    pub declarations: Vec<Declaration>,
    pub content_hash: Vec<u8>,
}

impl OutlineStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path)
            .with_context(|| format!("opening outline.db at {}", path.display()))?;
        conn.execute_batch(SCHEMA_V1)?;
        conn.execute(
            "INSERT OR IGNORE INTO meta(key, value) VALUES ('schema_version', '1')",
            [],
        )?;
        Ok(Self { conn })
    }

    pub fn schema_version(&self) -> Result<u32> {
        let v: String = self.conn.query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )?;
        Ok(v.parse().unwrap_or(0))
    }

    pub fn upsert(&self, repo: &str, pr: &ParseResult, content_hash: &[u8]) -> Result<()> {
        let path = pr.path.to_string_lossy().to_string();
        let decls = serde_json::to_string(&pr.declarations)?;
        self.conn.execute(
            "INSERT INTO files(repo, path, language, line_count, parse_errors, declarations, content_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(repo, path) DO UPDATE SET
                language = excluded.language,
                line_count = excluded.line_count,
                parse_errors = excluded.parse_errors,
                declarations = excluded.declarations,
                content_hash = excluded.content_hash",
            params![
                repo,
                path,
                pr.language,
                pr.line_count as i64,
                pr.error_count as i64,
                decls,
                content_hash,
            ],
        )?;
        Ok(())
    }

    pub fn get(&self, repo: &str, path: &str) -> Result<Option<OutlineEntry>> {
        let res = self.conn.query_row(
            "SELECT language, line_count, parse_errors, declarations, content_hash
             FROM files WHERE repo = ?1 AND path = ?2",
            params![repo, path],
            |row| {
                let language: String = row.get(0)?;
                let line_count: i64 = row.get(1)?;
                let parse_errors: i64 = row.get(2)?;
                let decls_json: String = row.get(3)?;
                let content_hash: Vec<u8> = row.get(4)?;
                Ok((language, line_count, parse_errors, decls_json, content_hash))
            },
        );
        match res {
            Ok((language, line_count, parse_errors, decls_json, content_hash)) => {
                let declarations: Vec<Declaration> = serde_json::from_str(&decls_json)
                    .map_err(|e| anyhow::anyhow!("declarations decode: {}", e))?;
                Ok(Some(OutlineEntry {
                    repo: repo.into(),
                    path: path.into(),
                    language,
                    line_count: line_count as usize,
                    parse_errors: parse_errors as usize,
                    declarations,
                    content_hash,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn delete(&self, repo: &str, path: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM files WHERE repo = ?1 AND path = ?2",
            params![repo, path],
        )?;
        Ok(())
    }

    /// List all (repo, path) tuples for a given repo (used in incremental updates).
    pub fn list_files(&self, repo: &str) -> Result<Vec<(String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT repo, path FROM files WHERE repo = ?1")?;
        let rows = stmt
            .query_map(params![repo], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}
```

- [ ] **Step B2.4: Run test, verify pass**

```bash
cargo test --test outline_store -- --nocapture 2>&1 | tail -15
```

Expected: 5 passed.

- [ ] **Step B2.5: Verify clippy clean**

```bash
cargo clippy --all-targets -- -D warnings 2>&1 | tail -3
```

- [ ] **Step B2.6: Full suite + commit**

```bash
cargo test --no-fail-fast 2>&1 | grep -E "^test result:" | awk '{p+=$4; f+=$6} END {print p, "passed,", f, "failed"}'
# expect 100 passed
git add src/outline/store.rs tests/outline_store.rs
git commit -m "feat(outline): SQLite-backed Declaration tree store (schema v1)"
```

---

### Task B-3: Adopt aeroxy `outline/render.rs`

**Files:**
- Modify: `src/outline/render.rs`
- Test: `tests/outline_render.rs`

The renderer turns a `ParseResult` into the human-readable outline / show / digest text outputs. Adopted directly from aeroxy `src/core.rs` (the `// --- Renderers ---` section starting around line 207, through `_digest_markdown`).

Note: Plan A only adopted lines 1-186 of aeroxy's marker section; here we adopt lines 197-933 of the same file (renderers + symbol search).

- [ ] **Step B3.1: Write the failing test**

Create `tests/outline_render.rs`:

```rust
use repolayer::core::declaration::{Declaration, DeclarationKind, OutlineOptions, ParseResult};
use repolayer::outline::render::render_outline;
use std::path::PathBuf;

#[test]
fn renders_minimal_outline() {
    let pr = ParseResult {
        path: PathBuf::from("foo.rs"),
        language: "rust",
        source: b"".to_vec(),
        line_count: 5,
        error_count: 0,
        declarations: vec![Declaration {
            kind: DeclarationKind::Function,
            name: "foo".into(),
            signature: "pub fn foo()".into(),
            start_line: 1,
            end_line: 1,
            ..Default::default()
        }],
    };
    let out = render_outline(&pr, &OutlineOptions::default());
    assert!(out.contains("foo.rs"), "output: {}", out);
    assert!(out.contains("pub fn foo()"), "output: {}", out);
}
```

- [ ] **Step B3.2: Verify fail**

```bash
cargo test --test outline_render 2>&1 | tail -5
```

- [ ] **Step B3.3: Adopt the render section**

```bash
cd /Users/bytedance/code/repolayer/.worktrees/ast-outline-ext
curl -fsL https://raw.githubusercontent.com/aeroxy/ast-outline/main/src/core.rs > /tmp/aeroxy_core_full.rs
sed -n '197,933p' /tmp/aeroxy_core_full.rs > /tmp/render_section.rs
wc -l /tmp/render_section.rs    # ~737 lines expected
```

- [ ] **Step B3.4: Build the new render.rs**

The adopted block uses imports `use crate::core::{Declaration, DeclarationKind, ParseResult, OutlineOptions, DigestOptions, ...}`. Adapt to our split module layout. The adopted section also references `_size_label` and `_serialize_path` which were on lines 197 and 933 — verify both functions are inside your `1,933p` range; they should be.

The block also contains `pub fn find_symbols`, `pub fn find_implementations`, `SymbolMatch`, `ImplMatch` — these support the `show` and `implements` commands and need to be in render.rs as well. Keep them.

Write `src/outline/render.rs`:

```rust
//! Outline / show / digest / find_symbols / find_implementations renderers.
//! Adopted from aeroxy/ast-outline `src/core.rs` (renderer section,
//! lines 197-933).

use colored::Colorize;
use serde::{Serialize, Serializer};
use std::path::Path;

use crate::core::declaration::{
    Declaration, DeclarationKind, DigestOptions, OutlineOptions, ParseResult,
};

// === paste the entire 197..=933 block from /tmp/render_section.rs here, with:
//     - `crate::core::Declaration` -> `crate::core::declaration::Declaration`
//     - `crate::core::DeclarationKind` -> `crate::core::declaration::DeclarationKind`
//     - similar for ParseResult, OutlineOptions, DigestOptions
//     - if the section uses `super::populate_markers`, keep as-is; populate_markers
//       is re-exported via crate::core::populate_markers but in this file we
//       might not need it (it runs in the parser path, not the render path)
//
// Use sed to do the bulk of substitutions:
//   sed -e 's|crate::core::Declaration|crate::core::declaration::Declaration|g' \
//       -e 's|crate::core::DeclarationKind|crate::core::declaration::DeclarationKind|g' \
//       -e 's|crate::core::ParseResult|crate::core::declaration::ParseResult|g' \
//       -e 's|crate::core::OutlineOptions|crate::core::declaration::OutlineOptions|g' \
//       -e 's|crate::core::DigestOptions|crate::core::declaration::DigestOptions|g' \
//       /tmp/render_section.rs > /tmp/render_section_fixed.rs
// then cat /tmp/render_section_fixed.rs into the file body
```

Implementer: do the actual sed + paste. Verify compile.

- [ ] **Step B3.5: Build, test, clippy**

```bash
cargo build 2>&1 | tail -10
```

If there are compile errors:
- Most likely: a `Declaration` reference the sed missed. Look at the line, fix the path.
- The `_serialize_path` function may already exist in `core/declaration.rs` — if so, the duplicate in render.rs causes a name collision. Rename render.rs's copy to `_serialize_path_for_render` or just make it private and not exported.
- Aeroxy's render code uses `colored::Colorize` — confirm `colored = "3"` is in Cargo.toml (Plan A added it).

```bash
cargo test --test outline_render 2>&1 | tail -10
cargo clippy --all-targets -- -D warnings 2>&1 | tail -10
```

For clippy lints from adopted code: add `#[allow(clippy::xxx)]` on the offending function with `// adopted from ast-outline`.

- [ ] **Step B3.6: Full suite + commit**

```bash
cargo test --no-fail-fast 2>&1 | grep -E "^test result:" | awk '{p+=$4; f+=$6} END {print p, "passed,", f, "failed"}'
# expect 101+ passed
git add src/outline/render.rs tests/outline_render.rs
git commit -m "feat(outline): adopt outline/show/digest renderers from ast-outline"
```

---

### Task B-4: Reset graph schema to v2 (NodeKind/EdgeKind expansion)

**Files:**
- Modify: `src/graph/model.rs` — new NodeKind/EdgeKind enums + Node fields
- Modify: `src/graph/store.rs` — schema v2 (drops v1; pre-alpha, no migration)
- Test: `tests/graph_schema_v2.rs` (new)
- Modify: `tests/graph_model.rs` (rewrite)
- Modify: `tests/graph_store.rs` (rewrite)

The main graph gets NodeKind expanded from 5 to 7 variants (Repo/Module/Type/Method/Function/IdlService/IdlMethod) and EdgeKind expanded from 6 to 7 variants (adds Extends). Nodes gain `visibility`, `native_kind`, `deprecated` columns. Edges gain `confidence` column.

Per spec §4.2: schema v2 is a hard break — old `index.db` files MUST be rebuilt. We add `meta.schema_version` so future migrations are possible.

- [ ] **Step B4.1: Write failing test**

Create `tests/graph_schema_v2.rs`:

```rust
use repolayer::graph::model::{Edge, EdgeKind, Node, NodeKind};
use repolayer::graph::store::Store;
use tempfile::tempdir;

#[test]
fn store_writes_schema_version_2() {
    let dir = tempdir().unwrap();
    let s = Store::open(&dir.path().join("index.db")).unwrap();
    assert_eq!(s.schema_version().unwrap(), 2);
}

#[test]
fn node_kind_method_persists() {
    let dir = tempdir().unwrap();
    let s = Store::open(&dir.path().join("index.db")).unwrap();
    let n = Node::new(NodeKind::Method, "repo1", "src/foo.rs", Some("Foo.bar"));
    s.upsert_node(&n).unwrap();
    let got = s.get_node(&n.id).unwrap().expect("node");
    assert!(matches!(got.kind, NodeKind::Method));
    assert_eq!(got.symbol.as_deref(), Some("Foo.bar"));
}

#[test]
fn edge_extends_persists_with_default_confidence() {
    let dir = tempdir().unwrap();
    let s = Store::open(&dir.path().join("index.db")).unwrap();
    let a = Node::new(NodeKind::Type, "r", "p", Some("A"));
    let b = Node::new(NodeKind::Type, "r", "p", Some("B"));
    s.upsert_node(&a).unwrap();
    s.upsert_node(&b).unwrap();
    let e = Edge {
        from: a.id.clone(),
        to: b.id.clone(),
        kind: EdgeKind::Extends,
        confidence: 1.0,
    };
    s.upsert_edge(&e).unwrap();
    let got = s.get_edges_from(&a.id).unwrap();
    assert_eq!(got.len(), 1);
    assert!(matches!(got[0].kind, EdgeKind::Extends));
    assert!((got[0].confidence - 1.0).abs() < 0.001);
}

#[test]
fn idl_service_idl_method_kinds_persist() {
    let dir = tempdir().unwrap();
    let s = Store::open(&dir.path().join("index.db")).unwrap();
    let svc = Node::new(NodeKind::IdlService, "idl", "user.proto", Some("UserSvc"));
    let m = Node::new(NodeKind::IdlMethod, "idl", "user.proto", Some("UserSvc.GetUser"));
    s.upsert_node(&svc).unwrap();
    s.upsert_node(&m).unwrap();
    assert!(matches!(s.get_node(&svc.id).unwrap().unwrap().kind, NodeKind::IdlService));
    assert!(matches!(s.get_node(&m.id).unwrap().unwrap().kind, NodeKind::IdlMethod));
}

#[test]
fn confidence_below_one_persists() {
    let dir = tempdir().unwrap();
    let s = Store::open(&dir.path().join("index.db")).unwrap();
    let a = Node::new(NodeKind::Module, "r", "a", None);
    let b = Node::new(NodeKind::IdlMethod, "idl", "x", Some("Svc.Foo"));
    s.upsert_node(&a).unwrap();
    s.upsert_node(&b).unwrap();
    s.upsert_edge(&Edge {
        from: a.id.clone(),
        to: b.id.clone(),
        kind: EdgeKind::Invokes,
        confidence: 0.5,
    }).unwrap();
    let got = s.get_edges_from(&a.id).unwrap();
    assert!((got[0].confidence - 0.5).abs() < 0.001);
}
```

- [ ] **Step B4.2: Verify fail**

```bash
cargo test --test graph_schema_v2 2>&1 | tail -5
```

Expected: compile errors about `NodeKind::Method` etc not existing.

- [ ] **Step B4.3: Rewrite `src/graph/model.rs`**

Read current `src/graph/model.rs`. Replace its NodeKind, EdgeKind, Node, Edge with:

```rust
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    Repo,
    Module,
    Type,        // class/struct/interface/trait/enum/record (replaces v1 Symbol)
    Method,      // method/ctor/dtor/operator inside a Type
    Function,    // top-level function
    IdlService,
    IdlMethod,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EdgeKind {
    Contains,
    Imports,
    Calls,
    Implements,
    Invokes,
    Defines,
    Extends,     // Type→Type (inheritance / trait impl / interface impl)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub kind: NodeKind,
    pub repo: String,
    pub path: String,
    pub symbol: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub native_kind: Option<String>,
    #[serde(default)]
    pub loc_start: Option<u32>,
    #[serde(default)]
    pub loc_end: Option<u32>,
    #[serde(default)]
    pub deprecated: bool,
}

impl Node {
    pub fn new(kind: NodeKind, repo: &str, path: &str, symbol: Option<&str>) -> Self {
        let id = compute_id(kind, repo, path, symbol);
        Self {
            id,
            kind,
            repo: repo.into(),
            path: path.into(),
            symbol: symbol.map(String::from),
            summary: None,
            visibility: None,
            native_kind: None,
            loc_start: None,
            loc_end: None,
            deprecated: false,
        }
    }
}

impl NodeKind {
    /// Stable string tag used in id hashing.
    /// **Do not change these strings — doing so invalidates all existing node ids.**
    fn id_tag(self) -> &'static str {
        match self {
            NodeKind::Repo => "repo",
            NodeKind::Module => "module",
            NodeKind::Type => "type",
            NodeKind::Method => "method",
            NodeKind::Function => "function",
            NodeKind::IdlService => "idlservice",
            NodeKind::IdlMethod => "idlmethod",
        }
    }
}

fn compute_id(kind: NodeKind, repo: &str, path: &str, symbol: Option<&str>) -> String {
    let mut h = Sha256::new();
    h.update(kind.id_tag().as_bytes());
    h.update(b"\0");
    h.update(repo.as_bytes());
    h.update(b"\0");
    h.update(path.as_bytes());
    h.update(b"\0");
    if let Some(s) = symbol {
        h.update(s.as_bytes());
    }
    let bytes = h.finalize();
    hex::encode(&bytes[..16])
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub kind: EdgeKind,
    #[serde(default = "default_confidence")]
    pub confidence: f32,
}

fn default_confidence() -> f32 {
    1.0
}
```

- [ ] **Step B4.4: Rewrite `src/graph/store.rs` schema**

Read current store.rs. Replace SCHEMA constant with:

```rust
const SCHEMA_V2: &str = r#"
CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS nodes (
    id          TEXT PRIMARY KEY,
    kind        TEXT NOT NULL,
    repo        TEXT NOT NULL,
    path        TEXT NOT NULL,
    symbol      TEXT,
    summary     TEXT,
    visibility  TEXT,
    native_kind TEXT,
    loc_start   INTEGER,
    loc_end     INTEGER,
    deprecated  INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_nodes_repo ON nodes(repo);
CREATE INDEX IF NOT EXISTS idx_nodes_kind ON nodes(kind);
CREATE INDEX IF NOT EXISTS idx_nodes_symbol ON nodes(symbol) WHERE symbol IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_nodes_repo_path ON nodes(repo, path);

CREATE TABLE IF NOT EXISTS edges (
    from_id    TEXT NOT NULL,
    to_id      TEXT NOT NULL,
    kind       TEXT NOT NULL,
    confidence REAL NOT NULL DEFAULT 1.0,
    PRIMARY KEY (from_id, to_id, kind)
);
CREATE INDEX IF NOT EXISTS idx_edges_from ON edges(from_id, kind);
CREATE INDEX IF NOT EXISTS idx_edges_to   ON edges(to_id, kind);
"#;
```

In `Store::open`, after `conn.execute_batch(SCHEMA_V2)?;`, insert:

```rust
conn.execute(
    "INSERT OR IGNORE INTO meta(key, value) VALUES ('schema_version', '2')",
    [],
)?;
```

Add a method:

```rust
pub fn schema_version(&self) -> anyhow::Result<u32> {
    let v: String = self.conn.query_row(
        "SELECT value FROM meta WHERE key = 'schema_version'",
        [],
        |r| r.get(0),
    )?;
    Ok(v.parse().unwrap_or(0))
}
```

Update `row_to_node` to include the new columns. The exact form depends on current store.rs but the pattern is:

```rust
fn row_to_node(row: &rusqlite::Row) -> rusqlite::Result<Node> {
    let kind_str: String = row.get(1)?;
    let kind = node_kind_from_db(&kind_str)?;
    Ok(Node {
        id: row.get(0)?,
        kind,
        repo: row.get(2)?,
        path: row.get(3)?,
        symbol: row.get(4)?,
        summary: row.get(5)?,
        visibility: row.get(6)?,
        native_kind: row.get(7)?,
        loc_start: row.get(8)?,
        loc_end: row.get(9)?,
        deprecated: row.get::<_, i64>(10)? != 0,
    })
}
```

Update `upsert_node` SQL to include the new columns:

```rust
pub fn upsert_node(&self, n: &Node) -> Result<()> {
    let kind_str = kind_to_db(n.kind)?;
    self.conn.execute(
        "INSERT INTO nodes(id, kind, repo, path, symbol, summary, visibility, native_kind, loc_start, loc_end, deprecated)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(id) DO UPDATE SET
            summary = COALESCE(excluded.summary, nodes.summary),
            visibility = COALESCE(excluded.visibility, nodes.visibility),
            native_kind = COALESCE(excluded.native_kind, nodes.native_kind),
            loc_start = COALESCE(excluded.loc_start, nodes.loc_start),
            loc_end = COALESCE(excluded.loc_end, nodes.loc_end),
            deprecated = excluded.deprecated",
        rusqlite::params![
            n.id,
            kind_str,
            n.repo,
            n.path,
            n.symbol,
            n.summary,
            n.visibility,
            n.native_kind,
            n.loc_start,
            n.loc_end,
            n.deprecated as i64,
        ],
    )?;
    Ok(())
}
```

Update edge SQL similarly to include `confidence`:

```rust
pub fn upsert_edge(&self, e: &Edge) -> Result<()> {
    let kind_str = kind_to_db(e.kind)?;
    self.conn.execute(
        "INSERT INTO edges(from_id, to_id, kind, confidence)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(from_id, to_id, kind) DO UPDATE SET
            confidence = excluded.confidence",
        rusqlite::params![e.from, e.to, kind_str, e.confidence],
    )?;
    Ok(())
}
```

Add a helper method (used by tests and queries):

```rust
pub fn get_node(&self, id: &str) -> Result<Option<Node>> {
    let res = self.conn.query_row(
        "SELECT id, kind, repo, path, symbol, summary, visibility, native_kind, loc_start, loc_end, deprecated
         FROM nodes WHERE id = ?1",
        rusqlite::params![id],
        row_to_node,
    );
    match res {
        Ok(n) => Ok(Some(n)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn get_edges_from(&self, from_id: &str) -> Result<Vec<Edge>> {
    let mut stmt = self.conn.prepare(
        "SELECT from_id, to_id, kind, confidence FROM edges WHERE from_id = ?1",
    )?;
    let rows = stmt.query_map(rusqlite::params![from_id], |row| {
        let kind_str: String = row.get(2)?;
        let kind = edge_kind_from_db(&kind_str)?;
        Ok(Edge {
            from: row.get(0)?,
            to: row.get(1)?,
            kind,
            confidence: row.get(3)?,
        })
    })?;
    let edges: Result<Vec<_>, _> = rows.collect();
    Ok(edges?)
}
```

- [ ] **Step B4.5: Rewrite existing tests/graph_model.rs and tests/graph_store.rs**

These tests currently use the old NodeKind::Symbol. Update them to use NodeKind::Type/Method/Function. Read the existing tests:

```bash
cat tests/graph_model.rs
cat tests/graph_store.rs
```

Replace `NodeKind::Symbol` with appropriate new variants based on what the test was checking. Where the test asserts on node count or edge count, those should still work — only kind names changed.

- [ ] **Step B4.6: Build, fix any references**

```bash
cargo build 2>&1 | tail -20
```

Expected errors: code in `src/indexer/mod.rs`, `src/linker/`, `src/query/`, `src/cli/` references `NodeKind::Symbol` (the old name). For each compile error:
- If the code creates a `Symbol` node from a parsed source-language file, replace with `Type` or `Method` or `Function` based on the parser's `SymbolKind` (in src/parser/) — but for now leave the legacy `parser/` code emitting a "best guess" mapping; it'll be deleted in B-25 anyway.
- A pragmatic fix: in `src/parser/typescript.rs` etc., when constructing a node, choose `NodeKind::Function` for SymbolKind::Function/Const, `NodeKind::Type` for SymbolKind::Class/Interface/TypeAlias.

This is throwaway code (parser/ deletion in B-25), so the mapping just has to compile and tests still pass — fine if it's coarse.

- [ ] **Step B4.7: Run all tests**

```bash
cargo test --no-fail-fast 2>&1 | grep -E "^test result:" | awk '{p+=$4; f+=$6} END {print p, "passed,", f, "failed"}'
```

Existing tests that rely on `Symbol` kind (cli_build, query_*, multi_repo_linking) will need adjustment. Walk through any failures:
- `cli_build.rs` queries `WHERE kind='symbol'` → change to `WHERE kind IN ('type','method','function')`
- Same for `query_others.rs`, `multi_repo_linking.rs` if applicable

Adjust until all 100+ tests green.

- [ ] **Step B4.8: Clippy**

```bash
cargo clippy --all-targets -- -D warnings 2>&1 | tail -10
```

- [ ] **Step B4.9: Commit**

```bash
git add src/graph/ tests/graph_model.rs tests/graph_store.rs tests/graph_schema_v2.rs tests/cli_build.rs tests/query_others.rs tests/multi_repo_linking.rs src/parser/ src/indexer/ src/linker/ src/query/
git commit -m "feat(graph): schema v2 — Type/Method/Function nodes, Extends edge, confidence column

- NodeKind: Symbol → Type/Method/Function (also keeps Repo/Module/IdlService/IdlMethod)
- Node fields: + visibility, native_kind, deprecated
- EdgeKind: + Extends
- Edge fields: + confidence (default 1.0)
- meta.schema_version = 2; pre-alpha break, no migration
- Updates legacy src/parser/ NodeKind mapping to a coarse best-guess
  (parser/ will be deleted in Plan B Task B-25)"
```

---

### Task B-5: Adopt aeroxy `deps/extract.rs`

**Files:**
- Create: `src/deps/extract.rs`
- Test: `tests/deps_extract.rs`

The `extract` module pulls raw imports from each source file using ast-grep patterns. This is independent of resolution (which files those imports point to) and runs first.

- [ ] **Step B5.1: Failing test**

```rust
use repolayer::deps::extract::extract;
use ast_grep_language::SupportLang;
use std::io::Write;
use tempfile::NamedTempFile;

fn write_file(suffix: &str, content: &str) -> NamedTempFile {
    let mut f = tempfile::Builder::new().suffix(suffix).tempfile().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f
}

#[test]
fn extracts_typescript_imports() {
    let f = write_file(".ts", "import { Foo } from './foo';\nimport bar from 'lib';\n");
    let imports = extract(f.path(), SupportLang::TypeScript);
    assert_eq!(imports.len(), 2);
    let specs: Vec<_> = imports.iter().map(|i| i.spec.clone()).collect();
    assert!(specs.contains(&"./foo".to_string()));
    assert!(specs.contains(&"lib".to_string()));
}

#[test]
fn extracts_python_imports() {
    let f = write_file(".py", "from .core import X\nimport os\n");
    let imports = extract(f.path(), SupportLang::Python);
    assert!(imports.len() >= 2);
    let specs: Vec<_> = imports.iter().map(|i| i.spec.clone()).collect();
    assert!(specs.iter().any(|s| s.contains("core") || s.contains(".core")));
    assert!(specs.iter().any(|s| s.contains("os")));
}
```

- [ ] **Step B5.2: Verify fail**

- [ ] **Step B5.3: Adopt**

```bash
curl -fL https://raw.githubusercontent.com/aeroxy/ast-outline/main/src/deps/extract.rs > src/deps/extract.rs
sed -i.bak 's|crate::core::|crate::core::declaration::|g' src/deps/extract.rs
sed -i.bak 's|crate::deps|crate::deps|g' src/deps/extract.rs
rm src/deps/extract.rs.bak
wc -l src/deps/extract.rs    # ~700+ lines expected
```

Expose in `src/deps/mod.rs`:

```rust
pub mod extract;
pub use extract::{extract, RawImport, RawImportKind};
```

- [ ] **Step B5.4: Build, test, clippy**

If aeroxy's extract.rs uses helper modules from `crate::deps::`(e.g. `crate::deps::options::DepError`), those will be undefined now — comment them out or stub them. The full deps module lands in subsequent tasks; extract just needs to compile in isolation. Note any stubs in your report.

```bash
cargo build 2>&1 | tail -10
cargo test --test deps_extract 2>&1 | tail -10
cargo clippy --all-targets -- -D warnings 2>&1 | tail -3
```

- [ ] **Step B5.5: Full suite + commit**

```bash
git add src/deps/extract.rs src/deps/mod.rs tests/deps_extract.rs
git commit -m "feat(deps): adopt extract.rs (raw import extraction via ast-grep)"
```

---

### Task B-6: Adopt aeroxy `deps/graph.rs` and `deps/options.rs`

Files: `src/deps/graph.rs`, `src/deps/options.rs`, test `tests/deps_graph.rs`

`DepGraph` is the in-memory data structure (forward + reverse edges + cycles). Adopted essentially verbatim.

- [ ] **Step B6.1: Failing test**

```rust
use repolayer::deps::graph::{DepEdge, DepGraph};
use repolayer::deps::extract::RawImportKind;
use std::path::PathBuf;

#[test]
fn dep_graph_forward_and_reverse() {
    let mut g = DepGraph::empty(PathBuf::from("/root"));
    g.forward.insert(
        PathBuf::from("/root/a.rs"),
        vec![DepEdge {
            target: PathBuf::from("/root/b.rs"),
            kind: RawImportKind::Use,
            line: 1,
            local_name: None,
            raw_path: Some("b".into()),
        }],
    );
    let rev = g.reverse_of(&PathBuf::from("/root/b.rs"));
    assert_eq!(rev.len(), 1);
    assert_eq!(rev[0], PathBuf::from("/root/a.rs"));
}
```

- [ ] **Step B6.2..B6.5: adopt + build + test + commit**

```bash
curl -fL https://raw.githubusercontent.com/aeroxy/ast-outline/main/src/deps/graph.rs > src/deps/graph.rs
curl -fL https://raw.githubusercontent.com/aeroxy/ast-outline/main/src/deps/options.rs > src/deps/options.rs
sed -i.bak 's|crate::core::|crate::core::declaration::|g' src/deps/graph.rs src/deps/options.rs
rm src/deps/graph.rs.bak src/deps/options.rs.bak
```

Edit `src/deps/mod.rs` to add `pub mod graph;` and `pub mod options;` plus `pub use graph::{DepEdge, DepGraph};` and `pub use options::DepError;`.

Build, test, clippy, commit.

```bash
git add src/deps/graph.rs src/deps/options.rs src/deps/mod.rs tests/deps_graph.rs
git commit -m "feat(deps): adopt graph + options (DepGraph, DepError)"
```

---

### Task B-7: Adopt aeroxy `deps/manifest.rs`, extend for Cargo / go.mod / pyproject

**Files:**
- Create: `src/deps/manifest.rs` (adopted)
- Test: `tests/manifest_resolution.rs` (new — extension verification)

aeroxy's `manifest.rs` detects TypeScript path aliases (tsconfig.json `paths`) and Go module names (`go.mod`). We extend it to handle Cargo workspace (`[workspace.members]` + `[package]` name in each `Cargo.toml`) and Python pyproject (`[project.name]`).

This is the cross-repo PackageIndex extension.

- [ ] **Step B7.1: Adopt the file**

```bash
curl -fL https://raw.githubusercontent.com/aeroxy/ast-outline/main/src/deps/manifest.rs > src/deps/manifest.rs
sed -i.bak 's|crate::core::|crate::core::declaration::|g' src/deps/manifest.rs
rm src/deps/manifest.rs.bak
```

- [ ] **Step B7.2: Extend `detect_aliases` for Cargo workspace + pyproject**

Read current manifest.rs. Find `pub fn detect_aliases(root: &Path) -> ManifestAliases`. It currently looks for `tsconfig.json` and `go.mod`. Extend:

```rust
// Add to ManifestAliases struct (assume aeroxy has fields like ts_path_aliases and go_module):
pub struct ManifestAliases {
    pub ts_path_aliases: Vec<(String, Vec<String>)>,
    pub go_module: Option<String>,
    pub rust_packages: Vec<RustPackage>,        // NEW
    pub python_packages: Vec<PythonPackage>,    // NEW
}

#[derive(Debug, Clone)]
pub struct RustPackage {
    pub name: String,
    pub root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct PythonPackage {
    pub name: String,
    pub root: PathBuf,
}

pub fn detect_aliases(root: &Path) -> ManifestAliases {
    // Existing TS/Go logic — keep
    let ts_path_aliases = detect_ts_aliases(root);
    let go_module = detect_go_module(root);

    // NEW: Rust
    let rust_packages = detect_rust_packages(root);

    // NEW: Python
    let python_packages = detect_python_packages(root);

    ManifestAliases { ts_path_aliases, go_module, rust_packages, python_packages }
}

fn detect_rust_packages(root: &Path) -> Vec<RustPackage> {
    let mut out = Vec::new();
    let cargo_toml = root.join("Cargo.toml");
    if !cargo_toml.exists() {
        return out;
    }
    let content = match std::fs::read_to_string(&cargo_toml) {
        Ok(s) => s,
        Err(_) => return out,
    };
    let parsed = match toml_edit::DocumentMut::from_str(&content) {
        Ok(d) => d,
        Err(_) => return out,
    };
    // Standalone package
    if let Some(pkg) = parsed.get("package").and_then(|p| p.as_table()) {
        if let Some(name) = pkg.get("name").and_then(|n| n.as_str()) {
            out.push(RustPackage { name: name.to_string(), root: root.to_path_buf() });
        }
    }
    // Workspace
    if let Some(ws) = parsed.get("workspace").and_then(|w| w.as_table()) {
        if let Some(members) = ws.get("members").and_then(|m| m.as_array()) {
            for m in members {
                if let Some(member_str) = m.as_str() {
                    let member_root = root.join(member_str);
                    let member_cargo = member_root.join("Cargo.toml");
                    if let Ok(member_content) = std::fs::read_to_string(&member_cargo) {
                        if let Ok(member_doc) = toml_edit::DocumentMut::from_str(&member_content) {
                            if let Some(name) = member_doc.get("package")
                                .and_then(|p| p.as_table())
                                .and_then(|p| p.get("name"))
                                .and_then(|n| n.as_str())
                            {
                                out.push(RustPackage { name: name.to_string(), root: member_root });
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

fn detect_python_packages(root: &Path) -> Vec<PythonPackage> {
    let mut out = Vec::new();
    let pyproject = root.join("pyproject.toml");
    if !pyproject.exists() {
        return out;
    }
    let content = match std::fs::read_to_string(&pyproject) {
        Ok(s) => s,
        Err(_) => return out,
    };
    if let Ok(parsed) = toml_edit::DocumentMut::from_str(&content) {
        if let Some(name) = parsed.get("project")
            .and_then(|p| p.as_table())
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
        {
            out.push(PythonPackage { name: name.to_string(), root: root.to_path_buf() });
        }
    }
    out
}
```

Add `use std::str::FromStr;` import for `toml_edit::DocumentMut::from_str` (toml_edit 0.22 API).

- [ ] **Step B7.3: Test**

```rust
use repolayer::deps::manifest::detect_aliases;
use std::fs;
use tempfile::tempdir;

#[test]
fn detects_rust_standalone_package() {
    let d = tempdir().unwrap();
    fs::write(d.path().join("Cargo.toml"), r#"[package]
name = "myapp"
version = "0.1.0"
edition = "2021"
"#).unwrap();
    let aliases = detect_aliases(d.path());
    assert_eq!(aliases.rust_packages.len(), 1);
    assert_eq!(aliases.rust_packages[0].name, "myapp");
}

#[test]
fn detects_rust_workspace_members() {
    let d = tempdir().unwrap();
    fs::write(d.path().join("Cargo.toml"), r#"[workspace]
members = ["crates/core", "crates/cli"]
"#).unwrap();
    fs::create_dir_all(d.path().join("crates/core")).unwrap();
    fs::write(d.path().join("crates/core/Cargo.toml"), r#"[package]
name = "myapp-core"
version = "0.1.0"
edition = "2021"
"#).unwrap();
    fs::create_dir_all(d.path().join("crates/cli")).unwrap();
    fs::write(d.path().join("crates/cli/Cargo.toml"), r#"[package]
name = "myapp-cli"
version = "0.1.0"
edition = "2021"
"#).unwrap();
    let aliases = detect_aliases(d.path());
    let names: Vec<_> = aliases.rust_packages.iter().map(|p| p.name.clone()).collect();
    assert!(names.contains(&"myapp-core".to_string()), "got: {:?}", names);
    assert!(names.contains(&"myapp-cli".to_string()), "got: {:?}", names);
}

#[test]
fn detects_python_pyproject() {
    let d = tempdir().unwrap();
    fs::write(d.path().join("pyproject.toml"), r#"[project]
name = "myapp"
version = "0.1.0"
"#).unwrap();
    let aliases = detect_aliases(d.path());
    assert_eq!(aliases.python_packages.len(), 1);
    assert_eq!(aliases.python_packages[0].name, "myapp");
}
```

- [ ] **Step B7.4: Build, test, clippy, commit**

```bash
cargo build 2>&1 | tail -3
cargo test --test manifest_resolution 2>&1 | tail -10
cargo clippy --all-targets -- -D warnings 2>&1 | tail -3
git add src/deps/manifest.rs src/deps/mod.rs tests/manifest_resolution.rs
git commit -m "feat(deps): adopt manifest.rs + extend with Rust workspace and Python pyproject"
```

---

### Task B-8: Adopt aeroxy `deps/resolver/`

**Files:**
- Create: `src/deps/resolver/mod.rs`, `src/deps/resolver/suffix.rs`, `src/deps/resolver/path.rs`
- Test: `tests/deps_resolver.rs`

The resolver maps `RawImport` (string spec like `"./foo"` or `"lib"`) to a concrete file path inside the project. Adopted from aeroxy.

- [ ] **Step B8.1..B8.5: standard adopt pattern**

```bash
mkdir -p src/deps/resolver
for f in mod suffix path; do
  curl -fL "https://raw.githubusercontent.com/aeroxy/ast-outline/main/src/deps/resolver/${f}.rs" > "src/deps/resolver/${f}.rs"
done
sed -i.bak 's|crate::core::|crate::core::declaration::|g' src/deps/resolver/*.rs
rm src/deps/resolver/*.rs.bak
```

Edit `src/deps/mod.rs` to add `pub mod resolver;` and re-exports.

Add a small test in `tests/deps_resolver.rs` covering forward resolution of a simple TS relative import.

```bash
cargo build 2>&1 | tail -10
cargo test --test deps_resolver 2>&1 | tail -10
cargo clippy --all-targets -- -D warnings 2>&1 | tail -3
git add src/deps/resolver/ src/deps/mod.rs tests/deps_resolver.rs
git commit -m "feat(deps): adopt resolver subsystem (suffix index + path resolution)"
```

---

### Task B-9: Adopt aeroxy `deps/scc.rs`, `deps/dsm.rs`, `deps/cache.rs`, `deps/render.rs`, `deps/traverse.rs`

**Files:** as listed; tests minimal (one e2e per module).

Bulk adoption with minimal tests since these are pure algorithms.

```bash
for f in scc dsm cache render traverse; do
  curl -fL "https://raw.githubusercontent.com/aeroxy/ast-outline/main/src/deps/${f}.rs" > "src/deps/${f}.rs"
done
sed -i.bak 's|crate::core::|crate::core::declaration::|g' src/deps/*.rs
rm src/deps/*.rs.bak
```

Edit `src/deps/mod.rs` to register them.

Test plan for this task: just verify cargo build + cargo test (existing 100+ tests still pass). Adopted code's own internal tests aren't ported — they'd require fixtures; instead Task B-23 will end-to-end verify via the indexer pipeline.

```bash
cargo build 2>&1 | tail -5
cargo clippy --all-targets -- -D warnings 2>&1 | tail -3
cargo test --no-fail-fast 2>&1 | grep -E "^test result:" | awk '{p+=$4; f+=$6} END {print p, "passed,", f, "failed"}'
git add src/deps/ tests/
git commit -m "feat(deps): adopt scc/dsm/cache/render/traverse modules"
```

---

### Task B-10: deps.db SQLite store wrapper

**Files:**
- Create: `src/deps/store.rs`
- Test: `tests/deps_store.rs`

aeroxy's `cache.rs` is in-memory + bincode file format. We wrap it in SQLite for repolayer's multi-store invariant. Schema per spec §4.4.

- [ ] **Step B10.1..B10.6: schema + tests + commit**

Implement DepStore with `forward_edges`, `external_imports`, `file_records` tables (see spec §4.4). API:

```rust
pub struct DepStore { conn: Connection }
impl DepStore {
    pub fn open(path: &Path) -> Result<Self>
    pub fn schema_version(&self) -> Result<u32>
    pub fn replace_repo_graph(&self, repo: &str, g: &DepGraph) -> Result<()>
    pub fn load_repo_graph(&self, repo: &str, root: PathBuf) -> Result<DepGraph>
    pub fn delete_file(&self, repo: &str, path: &str) -> Result<()>
}
```

Tests cover the round-trip, multi-repo isolation, file deletion.

```bash
git add src/deps/store.rs tests/deps_store.rs
git commit -m "feat(deps): SQLite-backed deps.db store (schema v1, multi-repo)"
```

---

### Task B-11: Wire deps build into pipeline (no indexer rewrite yet)

**Files:**
- Modify: `src/deps/mod.rs` — add a `build_for_repo(root: &Path) -> Result<DepGraph>` convenience that runs extract → resolve → graph in one shot

This is a small bridging task so the indexer rewrite (Task B-23) can call a single function per repo.

```rust
pub fn build_for_repo(root: &Path) -> Result<DepGraph, DepError> {
    let aliases = manifest::detect_aliases(root);
    let idx = resolver::build_suffix_index(root);
    let files: Vec<_> = idx.by_file.keys().cloned().collect();
    use rayon::prelude::*;
    let resolved: Vec<_> = files.par_iter().map(|file| {
        let info = idx.by_file.get(file).unwrap();
        let raw_imports = extract::extract(file, info.language);
        let mut edges = Vec::new();
        let mut external = Vec::new();
        let ctx = resolver::ResolveCtx {
            from_file: file,
            lang: info.language,
            alias_prefix: aliases.go_module.as_deref(),
            path_aliases: &aliases.ts_path_aliases,
        };
        for ri in raw_imports {
            match resolver::resolve(&ri.spec, &ctx, &idx) {
                Some(target) if target != *file => {
                    edges.push(DepEdge { target, kind: ri.kind, line: ri.line, local_name: ri.local_name, raw_path: ri.raw_path });
                }
                _ => external.push(ri.raw_path.unwrap_or(ri.spec)),
            }
        }
        (file.clone(), edges, external)
    }).collect();
    let mut g = DepGraph::empty(root.to_path_buf());
    for (file, edges, external) in resolved {
        g.forward.insert(file.clone(), edges);
        if !external.is_empty() {
            g.external.insert(file, external);
        }
    }
    graph::dedup_edges(&mut g);
    Ok(g)
}
```

Test in `tests/deps_e2e.rs`: build a fixture (single_repo_ts), assert dep edges are right.

Commit:
```bash
git add src/deps/mod.rs tests/deps_e2e.rs
git commit -m "feat(deps): build_for_repo orchestrates extract → resolve → graph"
```

---

### Task B-12: Adopt aeroxy `search/tokens.rs` and `search/format.rs` (pure utilities, no I/O)

These are dependency-free utilities that other search modules need. Adopt with minimal tests.

```bash
for f in tokens format; do
  curl -fL "https://raw.githubusercontent.com/aeroxy/ast-outline/main/src/search/${f}.rs" > "src/search/${f}.rs"
done
sed -i.bak 's|crate::core::|crate::core::declaration::|g' src/search/*.rs
rm src/search/*.rs.bak
```

Edit `src/search/mod.rs`: `pub mod tokens; pub mod format;`. Build + test pass + commit.

---

### Task B-13: Adopt `search/chunker.rs`

Chunks code into search-indexable units, one chunk per top-level Declaration plus size-bounded overflow. Depends on Plan A's `core::Declaration`. Test via parsing a fixture file then chunking.

Adopt with sed substitution. Test minimum: chunker emits ≥1 chunk per fixture file.

```bash
git add src/search/chunker.rs src/search/mod.rs tests/search_chunker.rs
git commit -m "feat(search): adopt chunker (Declaration-aware chunking)"
```

---

### Task B-14: Adopt `search/bm25.rs`

BM25 inverted index. Adopted verbatim. Test on a 3-document toy corpus.

---

### Task B-15: Adopt `search/download.rs` + model bootstrap

Downloads `potion-code-16M.safetensors` (~64 MB) to `.repolayer/models/` if missing. SHA256-verified.

Special instruction for implementer: there's no easy way to test this without making a real network call. Test with `mockito` if possible — stand up a fake HuggingFace endpoint. Otherwise mark the test `#[ignore]` with a comment "requires REPOLAYER_TEST_REAL_DOWNLOAD env var set".

```bash
git add src/search/download.rs tests/search_download.rs
git commit -m "feat(search): adopt download.rs (potion-code-16M model bootstrap)"
```

---

### Task B-16: Adopt `search/embed.rs`

Embedding computation using the downloaded model. Depends on `tokenizers + safetensors + memmap2 + wide` (already in Cargo.toml from Plan A).

Test: download the model (or use a fixture-stashed copy), embed two short strings, verify the output dimensions and that they're not identical (sanity).

If the download is too slow for tests, gate the test with `#[ignore]` per Task B-15 pattern.

---

### Task B-17: Adopt `search/ranking.rs` + `search/fusion.rs`

RRF fusion of BM25 + dense scores. Deterministic, easy to unit-test.

---

### Task B-18: Adopt `search/cache.rs` (file hash invalidation)

xxhash-rust based file content hashing for incremental reindex. Already a dependency.

---

### Task B-19: search.db SQLite store wrapper (with `repo` column)

aeroxy's `cache.rs` uses bincode. We wrap in SQLite per spec §4.5. Add `repo` column on every row for multi-repo support.

```sql
CREATE TABLE chunks (
    id INTEGER PRIMARY KEY,
    repo TEXT NOT NULL,
    path TEXT NOT NULL,
    start_line INTEGER, end_line INTEGER,
    content TEXT,
    chunk_hash BLOB
);
CREATE VIRTUAL TABLE chunk_vec USING vec0(embedding float[256]);
CREATE TABLE bm25_terms (...);
```

API:
```rust
pub struct SearchStore { conn: Connection }
impl SearchStore {
    pub fn open(path: &Path) -> Result<Self>
    pub fn replace_repo_chunks(&self, repo: &str, chunks: &[Chunk]) -> Result<()>
    pub fn search_hybrid(&self, repo: Option<&str>, query: &str, k: usize) -> Result<Vec<SearchHit>>
}
```

Test the round-trip + a small ranking sanity check.

---

### Task B-20: Adopt `search/index.rs` (top-level orchestrator)

Combines chunker + bm25 + embed + cache. After this task, search is a usable subsystem.

```bash
git add src/search/index.rs tests/search_index.rs
git commit -m "feat(search): adopt index.rs (top-level orchestrator)"
```

---

### Task B-21: Upgrade IDL linker — ast-grep call patterns instead of string contains

**Files:**
- Modify: `src/linker/idl_links.rs`
- Test: `tests/idl_linking.rs` (extend)

Currently `idl_links.rs:48` does `content.contains(short)`. This produces false positives when method names overlap with unrelated identifiers (`String` containing "str" matches a method named "str", etc.).

Upgrade to ast-grep call expression matching. For each language:
- TypeScript/JavaScript: pattern `$$$.${MethodName}($$$)` and `${MethodName}($$$)`
- Python: pattern `$$$.${MethodName}($$$)`
- Go: pattern `$$$.${MethodName}($$$)`
- Rust: pattern `$$$.${MethodName}($$$)`

Each match → emit edge with confidence:
- ast-grep call match → confidence = 0.7 (still inferred — could be wrong method)
- Path-based heuristic match (current `services/` rule for server-side) → confidence = 0.4 (kept as fallback when ast-grep finds no match)

Add a test: a code module that has the IDL method name as a method on an unrelated type (false positive in v1) — verify v2 doesn't emit the edge OR emits with low confidence.

```bash
git add src/linker/idl_links.rs tests/idl_linking.rs
git commit -m "feat(linker): upgrade IDL linking to ast-grep call patterns with confidence levels"
```

---

### Task B-22: Extend cross-repo PackageIndex to use `manifest::detect_aliases`

**Files:**
- Modify: `src/linker/imports.rs`

Currently `linker::imports::PackageIndex::build` only reads `package.json` files. Extend to also use `manifest::detect_aliases` results so Cargo / go.mod / pyproject packages are resolvable.

API stays the same — `lookup(import_spec) -> Option<PackageInfo>`. Implementation now consults rust_packages, python_packages, etc. in addition to the existing JS lookup.

Test: a workspace with a Rust workspace root + a downstream Rust crate that imports another workspace member; verify the import resolves.

```bash
git add src/linker/imports.rs tests/multi_repo_linking.rs
git commit -m "feat(linker): cross-repo PackageIndex resolves Cargo / go.mod / pyproject packages"
```

---

### Task B-23: Rewrite indexer as rayon-parallel multi-store pipeline

**Files:**
- Rewrite: `src/indexer/mod.rs`
- Test: `tests/multi_store_build.rs` (new) — full pipeline e2e

The old `Indexer::build_all` walks files serially and writes to one SQLite. The new pipeline:

1. Open 4 stores: index.db, outline.db, deps.db, search.db
2. Phase A — for each repo:
   - rayon `par_iter` over walked files
   - For source files: `adapters::parse_file(path)` → `ParseResult`
   - For IDL files: bare tree-sitter idl parser
   - mpsc::channel sends results to single writer thread
   - Writer:
     - Main graph: insert Repo / Module / Type / Method / Function nodes + Contains edges
     - Outline store: upsert (repo, path) row with full Declaration tree + content_hash
3. Phase B — cross-repo / IDL gluing (serial, after Phase A):
   - PackageIndex resolves cross-repo imports → Imports edges in main graph
   - `deps::build_for_repo(repo_root)` per repo → DepStore.replace_repo_graph
   - IdlLinker (upgraded) → Implements / Invokes edges with confidence
   - Manual links applied
4. Phase C — search index:
   - For each (repo, path) in outline.db, chunker → BM25 + embedding → SearchStore
5. Phase D — optional LLM summary (existing logic preserved)

Pseudocode for the writer pattern:

```rust
let (tx, rx) = std::sync::mpsc::channel();

// Spawn parsers
rayon::scope(|s| {
    for entry in walker {
        let tx = tx.clone();
        s.spawn(move |_| {
            if let Some(result) = adapters::parse_file(entry.path()) {
                tx.send(WriteMsg::Source { repo, path, result }).unwrap();
            }
        });
    }
});
drop(tx);

// Single writer drains rx
for msg in rx {
    match msg {
        WriteMsg::Source { repo, path, result } => {
            // upsert nodes + edges in index.db
            // upsert outline row in outline.db
        }
        WriteMsg::Idl { ... } => { ... }
    }
}
```

Test: build a 3-repo fixture (single_repo_ts + single_repo_py + idl repo); assert all 4 SQLite files exist with reasonable row counts.

Commit:
```bash
git add src/indexer/ tests/multi_store_build.rs
git commit -m "feat(indexer): rayon-parallel pipeline writing to 4 independent SQLite stores"
```

---

### Task B-24: Rewrite incremental update for multi-store invalidation

**Files:**
- Rewrite: `src/indexer/incremental.rs`
- Modify: `tests/cli_update.rs`

For each git-changed file:
1. Main graph: delete prior nodes/edges via (repo, path), re-parse, re-insert
2. outline.db: replace row keyed (repo, path)
3. deps.db: replace forward_edges row, re-resolve
4. search.db: invalidate chunks for that file via content_hash, re-chunk + re-embed

Cross-repo glue (PackageIndex + IdlLinker) only re-runs if any imports changed.

```bash
git add src/indexer/incremental.rs tests/cli_update.rs
git commit -m "feat(indexer): incremental update across all 4 stores via git diff"
```

---

### Task B-25: Delete legacy `src/parser/` module

**Files:**
- Delete: `src/parser/typescript.rs`, `src/parser/python.rs`, `src/parser/go.rs`, `src/parser/treesitter.rs`, `src/parser/mod.rs`
- Delete: `tests/parser_typescript.rs`, `tests/parser_python.rs`, `tests/parser_go.rs`
- Modify: `Cargo.toml` — remove `tree-sitter-typescript`, `tree-sitter-javascript`, `tree-sitter-python`, `tree-sitter-go` (kept `tree-sitter` for IDL)
- Modify: `src/lib.rs` — remove `pub mod parser;`
- Update: any remaining callers (should be only the legacy indexer code, which was rewritten in B-23)

After this task, source-language parsing goes 100% through `adapters::parse_file`.

```bash
# Verify nothing still imports parser:: outside what we delete
grep -rn "use crate::parser::\|crate::parser::" src/ tests/ --exclude-dir=parser
```

Should only show references to `parser::idl::` (already moved to `adapters::idl::` in Plan A) and references within `parser/` itself (which we're deleting). Anything else is a leak — fix.

```bash
git rm -r src/parser/
git rm tests/parser_typescript.rs tests/parser_python.rs tests/parser_go.rs
# Edit Cargo.toml to remove tree-sitter-{typescript,javascript,python,go}
# Edit src/lib.rs to remove `pub mod parser;`
cargo build 2>&1 | tail -10
cargo test --no-fail-fast 2>&1 | grep -E "^test result:" | awk '{p+=$4; f+=$6} END {print p, "passed,", f, "failed"}'
cargo clippy --all-targets -- -D warnings 2>&1 | tail -3

git add -A
git commit -m "refactor: delete legacy src/parser/ — all source parsing via adapters now"
```

---

### Task B-26: Update CLI build/update commands to use new pipeline

**Files:**
- Modify: `src/cli/build.rs`, `src/cli/update.rs`

Trivial — `build_all` and `update` functions changed signatures slightly (now open multiple stores). Wire the CLI commands to the new entry points.

Test: `tests/cli_build.rs` already updated in B-23; verify `repolayer build` end-to-end on a fixture.

```bash
git add src/cli/
git commit -m "feat(cli): build/update commands wire to multi-store indexer"
```

---

### Task B-27: Update query layer to read from new schema

**Files:**
- Modify: `src/query/find_context.rs` (still substring; hybrid in Plan C)
- Modify: `src/query/symbol.rs` (returns Declaration via outline.db)
- Modify: `src/query/callers.rs`, `src/query/dependencies.rs`, `src/query/list_repos.rs`
- Modify: `tests/query_*.rs`

Each query function gains references to outline.db / deps.db where useful. `find_context` stays substring-only in Plan B (Plan C upgrades to hybrid).

`get_symbol`: when found in main graph, also look up the Declaration tree from outline.db and return the node tree as part of the response.

Tests: existing query_others.rs / query_find_context.rs adapted.

```bash
git add src/query/ tests/query_*.rs
git commit -m "feat(query): read from new schema (Type/Method/Function nodes), expose Declaration via outline"
```

---

### Task B-28: Plan B wrap-up + dogfood

- [ ] **Step B28.1: Release build**

```bash
cargo build --release 2>&1 | tail -5
ls -lh target/release/repolayer
```

Expected: ~25-30 MB binary (Plan A was 12 MB; +tokenizers/safetensors/wide adds ~15 MB).

- [ ] **Step B28.2: End-to-end dogfood (3-repo fixture)**

```bash
WS=$(mktemp -d)
cp -r tests/fixtures/single_repo_ts "$WS/repo1"
cp -r tests/fixtures/single_repo_py "$WS/repo2"
cp -r tests/fixtures/idl "$WS/idl_repo"
cd "$WS"
cat > repolayer.yml <<EOF
repos:
  - path: ./repo1
  - path: ./repo2
  - path: ./idl_repo
    type: idl
EOF
/Users/bytedance/code/repolayer/.worktrees/ast-outline-ext/target/release/repolayer build
ls -la .repolayer/
# Expected: index.db, outline.db, deps.db, search.db, models/
```

- [ ] **Step B28.3: Self-index repolayer source (Rust dogfood)**

```bash
WS=$(mktemp -d)
cd "$WS"
cat > repolayer.yml <<EOF
repos:
  - path: /Users/bytedance/code/repolayer/.worktrees/ast-outline-ext
EOF
/Users/bytedance/code/repolayer/.worktrees/ast-outline-ext/target/release/repolayer build
echo "node count by kind:"
sqlite3 .repolayer/index.db "SELECT kind, COUNT(*) FROM nodes GROUP BY kind"
echo "outline files:"
sqlite3 .repolayer/outline.db "SELECT COUNT(*) FROM files"
```

Expected output proves the Rust adapter from Plan A works end-to-end on a real codebase.

- [ ] **Step B28.4: Tag**

```bash
cd /Users/bytedance/code/repolayer/.worktrees/ast-outline-ext
git tag -a plan-b-complete -m "Plan B complete: storage + indexer + linker rewrite"
```

- [ ] **Step B28.5: Final summary**

```bash
echo "=== Plan B summary ==="
echo "Tests:" && cargo test --no-fail-fast 2>&1 | grep -E "^test result:" | awk '{p+=$4; f+=$6} END {print p, "passed,", f, "failed"}'
echo "Binary:" && ls -lh target/release/repolayer
echo "New top-level dirs since Plan A:"
ls src/ | grep -E "^(outline|deps|search)$"
echo "Stores produced by build:"
ls .repolayer/ 2>/dev/null || echo "(run dogfood test first)"
```

Expected: ~110+ tests passing; 4 SQLite files; binary ~25-30 MB.

---

## Self-review checklist

**1. Spec coverage:** Plan B implements spec §3.1 (4-SQLite split), §4.2 (graph schema v2), §4.3 (outline.db), §4.4 (deps.db), §4.5 (search.db), §4.6 (model cache), §5 (build pipeline phases A/B/C/D), §6.1 (adapter dispatch — already in Plan A), §6.2 (IDL retained), parts of §7 (Cargo.toml) — yes, fully covered, plus Plan A had its own §6 coverage. Spec §8 (MCP tools) and §9 (CLI subcommands) deferred to Plan C as planned.

**2. Placeholder scan:** Tasks B-9 (bulk adoption), B-12, B-13 etc use compressed step descriptions. Verify each is concrete enough — yes, they have curl URLs and explicit commit messages. Tasks B-15 / B-16 acknowledge test difficulty (model download) and instruct using `#[ignore]` — explicit guidance, not a placeholder.

**3. Type consistency:**
- `OutlineStore::open(path) -> Result<Self>` consistent across B-2 and downstream usage
- `DepStore::open` / `SearchStore::open` same pattern
- `Edge.confidence: f32` (B-4) used in `Edge` everywhere
- `NodeKind::Type/Method/Function` (B-4) used in B-23 indexer
- `Declaration` structure (Plan A) used as ParseResult.declarations everywhere

**4. Test pyramid:**
- Unit-level: each store has its own test file (B-2, B-10, B-19)
- Integration: B-23 tests the full pipeline on a 3-repo fixture
- Existing tests adapted (B-4 rewrites graph tests, B-23 rewrites cli_build, B-24 rewrites cli_update, B-27 rewrites query tests)

**5. Risk-coverage:** From spec §11:
- "Adopting ~12 KLOC may bring bugs" — mitigated by adopt-with-tests-pattern (each `curl` followed by minimal smoke test)
- "ast-grep API surface changes" — Plan A pinned 0.42; spec preserved
- "Model download UX" — B-15 surfaces progress, allows REPOLAYER_NO_DOWNLOAD env var
- "Build time regression" — accepted in spec; can be revisited in Plan C with `--no-search` flag if needed

---

## Handoff

After all 28 tasks pass, **proceed to Plan C** (MCP compat tools + install + prompt + NOTICE/README/dogfood polish).
