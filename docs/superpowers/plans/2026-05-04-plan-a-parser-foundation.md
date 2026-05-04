# Plan A: Parser Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Lay the foundation of the v0.2 refactor: introduce the `core::Declaration` IR (adopted from aeroxy/ast-outline), the ast-grep–based adapter trait, all 10 source-language adapters (adopted), and a `parse_file_for_hook`-style dispatcher. Keep the existing `parser/` module compiling alongside so the rest of the codebase continues to work; the indexer is wired to the new path in **Plan B**.

**Architecture:** Add a parallel parser stack under `src/core/` and `src/adapters/` (ast-grep-based) without removing the existing `src/parser/` (bare tree-sitter) yet. Two stacks coexist behind a thin dispatcher (`adapters::parse_file`) that returns a `ParseResult` with a tree of `Declaration` nodes. The IDL parsers under `src/parser/idl/` move untouched into `src/adapters/idl/`. The current 4 source-language parsers (TS/JS/Py/Go) become dead code at end of Plan A — they'll be deleted in Plan B once the indexer is rewritten.

**Tech Stack:** Rust 2021, `ast-grep-core 0.42`, `ast-grep-language 0.42`, `tree-sitter 0.24` (kept for IDL only), `colored 3` (used by Declaration's line-suffix renderer), `serde`, `once_cell`.

**Inputs from prior steps:**
- Spec at `docs/superpowers/specs/2026-05-04-ast-outline-extension-design.md`
- Worktree `.worktrees/ast-outline-ext` on branch `feature/ast-outline-ext`
- Baseline: 62 tests passing on `master` at commit `058d45e`

**Outputs of this plan:**
- 11 new files under `src/core/` and `src/adapters/`
- Existing 62 tests still pass (we don't touch them)
- 10 new adapter unit tests (~1 per adapter)
- A NOTICE file at repo root attributing aeroxy/ast-outline
- A green `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo build --release`

**Out of scope:** indexer rewrite, storage split (4 SQLite), dep graph, search index, MCP tool changes. All deferred to Plans B & C.

---

## File structure

After Plan A, the tree looks like this (new files only; existing files untouched unless noted):

```
NOTICE                                  # NEW — attribution per MIT
Cargo.toml                              # MODIFY — add deps, do not remove yet

src/
├── lib.rs                              # MODIFY — add `pub mod core; pub mod adapters;`
├── core/                               # NEW
│   ├── mod.rs
│   ├── declaration.rs                  # ADOPTED from aeroxy/src/core.rs (IR types)
│   ├── markers.rs                      # ADOPTED from aeroxy/src/core.rs (post-processing)
│   └── schema.rs                       # ADOPTED from aeroxy/src/core.rs (JSON_SCHEMA_*)
├── adapters/                           # NEW
│   ├── mod.rs                          # NEW — registry + dispatcher (from aeroxy main_helpers.rs)
│   ├── base.rs                         # ADOPTED from aeroxy
│   ├── rust.rs                         # ADOPTED
│   ├── csharp.rs                       # ADOPTED
│   ├── java.rs                         # ADOPTED
│   ├── kotlin.rs                       # ADOPTED
│   ├── scala.rs                        # ADOPTED
│   ├── typescript.rs                   # ADOPTED (handles ts/tsx/js/jsx/cjs/mjs)
│   ├── python.rs                       # ADOPTED
│   ├── go.rs                           # ADOPTED
│   ├── markdown.rs                     # ADOPTED
│   └── idl/                            # MOVED from src/parser/idl/ (no behavioral change)
│       ├── mod.rs
│       ├── protobuf.rs
│       └── thrift.rs
├── parser/                             # KEPT (will be deleted in Plan B)
│   └── ... (existing files, untouched)
└── ... (everything else untouched)

tests/
├── adapter_rust.rs                     # NEW
├── adapter_typescript.rs               # NEW (replaces parser_typescript.rs in Plan B)
├── adapter_python.rs                   # NEW
├── adapter_go.rs                       # NEW
├── adapter_java.rs                     # NEW
├── adapter_kotlin.rs                   # NEW
├── adapter_scala.rs                    # NEW
├── adapter_csharp.rs                   # NEW
├── adapter_markdown.rs                 # NEW
├── adapter_dispatch.rs                 # NEW
└── ... (existing 19 test files, untouched)
```

The existing `src/parser/idl/` is **moved** to `src/adapters/idl/`. The IDL parsers don't implement `LanguageAdapter` (they emit different output types), but living under `adapters/` keeps related code together.

---

## Notes on adopted code

For each "ADOPTED from aeroxy" file, the steps below say:

> Step: copy `https://raw.githubusercontent.com/aeroxy/ast-outline/main/src/<path>` to `<dest>` and apply the import-rewrite from `crate::core::*` and `crate::adapters::*` paths (already correct — aeroxy and repolayer use identical module names).

Because aeroxy's module structure is `src/core.rs` and `src/adapters/*`, but we use `src/core/declaration.rs` (a sub-module), every adopted file needs its `use crate::core::...` lines re-pointed to `use crate::core::declaration::...` (where applicable). This is a **mechanical sed**, not a code change.

aeroxy's `core.rs` is a single 1268-line file. We split it across 3 files (`declaration.rs` / `markers.rs` / `schema.rs`) for readability. The split is mechanical: types/impls stay together, no logic changes.

LICENSE: aeroxy is MIT. Our LICENSE stays MIT. We add a `NOTICE` file (Task 0 below) listing adopted components per the spec §2.2.

---

### Task 0: Attribution + dependency wiring

**Files:**
- Create: `NOTICE`
- Modify: `Cargo.toml`

- [ ] **Step 0.1: Add `NOTICE` file**

Create `/Users/bytedance/code/repolayer/.worktrees/ast-outline-ext/NOTICE` with:

```
This product includes software developed by:

  ast-outline (https://github.com/aeroxy/ast-outline)
  Copyright (c) 2026 Aero <aero.windwalker@gmail.com>
  Licensed under the MIT License.

Components copied or adapted from ast-outline:
  - src/core/declaration.rs, src/core/markers.rs, src/core/schema.rs
    (split from aeroxy src/core.rs)
  - src/adapters/* (except idl/)

The following components are original to repolayer:
  - cross-repo workspace model (config/, linker/)
  - IDL parsing and graph (adapters/idl/, graph/)
  - MCP server with multi-repo tools (mcp/)
  - cross-repo import resolution
```

- [ ] **Step 0.2: Add new dependencies to `Cargo.toml`**

Edit `Cargo.toml`. Find the `[dependencies]` block and add the following lines (preserve the existing entries):

```toml
ast-grep-core = "0.42"
ast-grep-language = "0.42"
colored = "3"
once_cell = "1"
```

Do **not** remove `tree-sitter-typescript`, `tree-sitter-javascript`, `tree-sitter-python`, or `tree-sitter-go` yet — they're still used by `src/parser/`. They get removed in Plan B.

- [ ] **Step 0.3: Verify everything still compiles**

```bash
cargo build 2>&1 | tail -5
```

Expected: `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in <N>s`. No warnings about unused new deps (they're declared but not yet used — that's fine).

- [ ] **Step 0.4: Commit**

```bash
git add NOTICE Cargo.toml Cargo.lock
git commit -m "chore: add ast-grep deps and NOTICE for adopted ast-outline code"
```

---

### Task 1: `core/declaration.rs` — adopt the IR types

**Files:**
- Create: `src/core/mod.rs`
- Create: `src/core/declaration.rs`
- Test: `tests/core_declaration.rs`

- [ ] **Step 1.1: Write the failing test first**

Create `/Users/bytedance/code/repolayer/.worktrees/ast-outline-ext/tests/core_declaration.rs`:

```rust
use repolayer::core::declaration::{Declaration, DeclarationKind, ParseResult};
use std::path::PathBuf;

#[test]
fn declaration_kind_serializes_to_canonical_string() {
    let json = serde_json::to_string(&DeclarationKind::Class).unwrap();
    assert_eq!(json, "\"class\"");
    let json = serde_json::to_string(&DeclarationKind::EnumMember).unwrap();
    assert_eq!(json, "\"enum_member\"");
    let json = serde_json::to_string(&DeclarationKind::Constructor).unwrap();
    assert_eq!(json, "\"ctor\"");
}

#[test]
fn declaration_default_has_namespace_kind() {
    let d = Declaration::default();
    assert!(matches!(d.kind, DeclarationKind::Namespace));
    assert_eq!(d.name, "");
    assert!(d.children.is_empty());
}

#[test]
fn parse_result_is_constructible() {
    let r = ParseResult {
        path: PathBuf::from("/tmp/foo.rs"),
        language: "rust",
        source: b"".to_vec(),
        line_count: 0,
        error_count: 0,
        declarations: vec![],
    };
    assert_eq!(r.language, "rust");
}
```

- [ ] **Step 1.2: Run test to verify it fails**

```bash
cargo test --test core_declaration 2>&1 | tail -10
```

Expected: compile error — `unresolved module 'core'`.

- [ ] **Step 1.3: Add `core` module to lib.rs**

Edit `src/lib.rs` to add `pub mod core;` at the top (alongside existing `pub mod ...`):

```rust
pub mod core;
pub mod cli;
pub mod config;
pub mod graph;
pub mod indexer;
pub mod linker;
pub mod llm;
pub mod mcp;
pub mod parser;
pub mod query;
```

- [ ] **Step 1.4: Create `src/core/mod.rs`**

```rust
pub mod declaration;
pub mod markers;
pub mod schema;

pub use declaration::*;
pub use markers::populate_markers;
pub use schema::*;
```

- [ ] **Step 1.5: Create `src/core/declaration.rs`**

Copy the IR section from `https://raw.githubusercontent.com/aeroxy/ast-outline/main/src/core.rs` lines 1–195 into `src/core/declaration.rs`. The file content is exactly:

```rust
use colored::Colorize;
use serde::{Serialize, Serializer};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy, Default)]
pub enum DeclarationKind {
    #[default]
    Namespace,
    Class,
    Struct,
    Interface,
    Record,
    Enum,
    EnumMember,
    Method,
    Function,
    Constructor,
    Destructor,
    Property,
    Indexer,
    Field,
    Event,
    Delegate,
    Operator,
    Heading,
    CodeBlock,
}

impl DeclarationKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Namespace => "namespace",
            Self::Class => "class",
            Self::Struct => "struct",
            Self::Interface => "interface",
            Self::Record => "record",
            Self::Enum => "enum",
            Self::EnumMember => "enum_member",
            Self::Method => "method",
            Self::Function => "function",
            Self::Constructor => "ctor",
            Self::Destructor => "dtor",
            Self::Property => "property",
            Self::Indexer => "indexer",
            Self::Field => "field",
            Self::Event => "event",
            Self::Delegate => "delegate",
            Self::Operator => "operator",
            Self::Heading => "heading",
            Self::CodeBlock => "code_block",
        }
    }
}

impl std::fmt::Display for DeclarationKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Serialize for DeclarationKind {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Declaration {
    pub kind: DeclarationKind,
    pub name: String,
    pub signature: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bases: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub docs: Vec<String>,
    pub docs_inside: bool,
    pub visibility: String,
    pub start_line: usize,
    pub end_line: usize,
    pub start_byte: usize,
    pub end_byte: usize,
    pub doc_start_byte: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modifiers: Vec<String>,
    #[serde(default, skip_serializing_if = "_is_false")]
    pub deprecated: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Declaration>,
}

fn _is_false(b: &bool) -> bool {
    !*b
}

impl Declaration {
    pub fn lines_suffix(&self) -> String {
        if self.start_line == 0 {
            String::new()
        } else if self.start_line == self.end_line {
            format!("  L{}", self.start_line)
                .truecolor(150, 150, 150)
                .to_string()
        } else {
            format!("  L{}-{}", self.start_line, self.end_line)
                .truecolor(150, 150, 150)
                .to_string()
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ParseResult {
    #[serde(serialize_with = "_serialize_path")]
    pub path: PathBuf,
    pub language: &'static str,
    #[serde(skip)]
    pub source: Vec<u8>,
    pub line_count: usize,
    pub error_count: usize,
    pub declarations: Vec<Declaration>,
}

fn _serialize_path<S: Serializer>(p: &Path, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&p.to_string_lossy())
}

#[derive(Debug, Clone)]
pub struct OutlineOptions {
    pub include_private: bool,
    pub include_fields: bool,
    pub include_docs: bool,
    pub include_attributes: bool,
    pub include_line_numbers: bool,
    pub max_doc_lines: usize,
    pub max_members: Option<usize>,
}

impl Default for OutlineOptions {
    fn default() -> Self {
        Self {
            include_private: true,
            include_fields: true,
            include_docs: true,
            include_attributes: true,
            include_line_numbers: true,
            max_doc_lines: 6,
            max_members: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DigestOptions {
    pub include_private: bool,
    pub include_fields: bool,
    pub max_members_per_type: usize,
    pub max_heading_depth: usize,
}

impl Default for DigestOptions {
    fn default() -> Self {
        Self {
            include_private: false,
            include_fields: false,
            max_members_per_type: 50,
            max_heading_depth: 3,
        }
    }
}
```

- [ ] **Step 1.6: Create stub `src/core/markers.rs`**

```rust
//! Marker post-processing: derives `native_kind`, `modifiers`, `deprecated`
//! fields on Declarations from the language-native conventions captured
//! by adapters in `attrs`/`signature`.
//!
//! Plan A ships a no-op stub so the dispatcher compiles. The full
//! implementation (adopted from aeroxy/src/core.rs lines 200-1268) lands
//! in Plan A Task 12.

use crate::core::declaration::Declaration;

pub fn populate_markers(_decls: &mut [Declaration], _language: &'static str) {
    // No-op stub. Replaced in Task 12.
}
```

- [ ] **Step 1.7: Create stub `src/core/schema.rs`**

```rust
//! Stable JSON schema identifiers used in MCP tool responses.
//! Bump on breaking changes.

pub const JSON_SCHEMA_OUTLINE: &str = "ast-outline.outline.v1";
pub const JSON_SCHEMA_SHOW: &str = "ast-outline.show.v1";
pub const JSON_SCHEMA_IMPLEMENTS: &str = "ast-outline.implements.v1";
pub const JSON_SCHEMA_SURFACE: &str = "ast-outline.surface.v1";
pub const JSON_SCHEMA_DEPS: &str = "ast-outline.deps.v1";
pub const JSON_SCHEMA_REVERSE_DEPS: &str = "ast-outline.reverse-deps.v1";
pub const JSON_SCHEMA_CYCLES: &str = "ast-outline.cycles.v1";
pub const JSON_SCHEMA_GRAPH: &str = "ast-outline.graph.v1";
pub const JSON_SCHEMA_DEPS_INDEX: &str = "ast-outline.deps-index.v1";

// repolayer-original (added in later plans):
pub const JSON_SCHEMA_FIND_CONTEXT: &str = "repolayer.find_context.v1";
pub const JSON_SCHEMA_GET_SYMBOL: &str = "repolayer.get_symbol.v1";
pub const JSON_SCHEMA_GET_CALLERS: &str = "repolayer.get_callers.v1";
pub const JSON_SCHEMA_GET_DEPENDENCIES: &str = "repolayer.get_dependencies.v1";
pub const JSON_SCHEMA_LIST_REPOS: &str = "repolayer.list_repos.v1";
pub const JSON_SCHEMA_FIND_IDL_IMPL: &str = "repolayer.find_idl_impl.v1";
```

- [ ] **Step 1.8: Run test to verify it passes**

```bash
cargo test --test core_declaration -- --nocapture 2>&1 | tail -10
```

Expected: 3 passed; 0 failed.

- [ ] **Step 1.9: Verify clippy is clean**

```bash
cargo clippy --all-targets -- -D warnings 2>&1 | tail -10
```

Expected: `Finished` with no errors.

- [ ] **Step 1.10: Commit**

```bash
git add src/core/ src/lib.rs tests/core_declaration.rs
git commit -m "feat(core): adopt Declaration IR from aeroxy/ast-outline (split into 3 files)"
```

---

### Task 2: `adapters/base.rs` + `adapters/mod.rs` skeleton

**Files:**
- Create: `src/adapters/mod.rs`
- Create: `src/adapters/base.rs`

- [ ] **Step 2.1: Add `adapters` to lib.rs**

Edit `src/lib.rs`:

```rust
pub mod core;
pub mod adapters;
pub mod cli;
// ... rest unchanged
```

- [ ] **Step 2.2: Create `src/adapters/base.rs`**

Content (verbatim from aeroxy `src/adapters/base.rs`, with `crate::core::ParseResult` import path adjusted to `crate::core::declaration::ParseResult`):

```rust
use crate::core::declaration::ParseResult;
use ast_grep_core::{Doc, Node};
use std::path::Path;

pub trait LanguageAdapter {
    fn language_name(&self) -> &'static str;

    /// Parses the file content, using an initial root Node parsed by ast_grep
    fn parse<'a, D: Doc>(&self, path: &Path, source: &[u8], root: Node<'a, D>) -> ParseResult;
}

pub fn count_parse_errors<D: Doc>(root: Node<D>) -> usize {
    let mut total = 0;
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == "ERROR" || n.is_missing() {
            total += 1;
        }
        for child in n.children() {
            stack.push(child);
        }
    }
    total
}

pub fn field_text<'a, D: Doc>(node: &Node<'a, D>, field_name: &str) -> Option<String> {
    node.field(field_name).map(|n| n.text().into_owned())
}

pub fn collapse_ws(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
```

- [ ] **Step 2.3: Create stub `src/adapters/mod.rs`**

```rust
//! Source-language adapters built on `ast-grep-core`.
//!
//! Each adapter implements [`base::LanguageAdapter`] for one language
//! (or a small family — `typescript.rs` covers ts/tsx/js/jsx/cjs/mjs).
//! IDL parsers under [`idl`] use bare tree-sitter and emit a different
//! shape (`IdlFile` rather than `ParseResult`); they are dispatched
//! separately by the indexer.

pub mod base;
// Adapters land here in tasks 3-11:
// pub mod rust;
// pub mod csharp;
// pub mod java;
// pub mod kotlin;
// pub mod scala;
// pub mod typescript;
// pub mod python;
// pub mod go;
// pub mod markdown;

// IDL adapters land here in task 13 (move from src/parser/idl/):
// pub mod idl;
```

- [ ] **Step 2.4: Verify it compiles**

```bash
cargo build 2>&1 | tail -5
```

Expected: clean build.

- [ ] **Step 2.5: Commit**

```bash
git add src/adapters/ src/lib.rs
git commit -m "feat(adapters): scaffold base trait + module"
```

---

### Task 3: Adopt `adapters/python.rs`

**Files:**
- Create: `src/adapters/python.rs`
- Test: `tests/adapter_python.rs`
- Modify: `src/adapters/mod.rs`

The Python adapter is the smallest (8.5 KB) and the simplest — good first integration target.

- [ ] **Step 3.1: Write the failing test**

Create `/Users/bytedance/code/repolayer/.worktrees/ast-outline-ext/tests/adapter_python.rs`:

```rust
use ast_grep_core::Language;
use ast_grep_language::{LanguageExt, SupportLang};
use repolayer::adapters::base::LanguageAdapter;
use repolayer::adapters::python::PythonAdapter;
use repolayer::core::declaration::DeclarationKind;
use std::path::Path;

fn parse(src: &str) -> repolayer::core::declaration::ParseResult {
    let lang = SupportLang::Python;
    let root_doc = lang.ast_grep(src.to_string());
    PythonAdapter.parse(Path::new("test.py"), src.as_bytes(), root_doc.root())
}

#[test]
fn parses_top_level_function() {
    let r = parse("def hello(name):\n    return name\n");
    let names: Vec<_> = r.declarations.iter().map(|d| d.name.clone()).collect();
    assert!(names.contains(&"hello".to_string()), "decls: {:?}", names);
    let f = r.declarations.iter().find(|d| d.name == "hello").unwrap();
    assert!(matches!(f.kind, DeclarationKind::Function));
}

#[test]
fn parses_class_with_method() {
    let src = "class User:\n    def __init__(self):\n        pass\n    def name(self):\n        pass\n";
    let r = parse(src);
    let user = r.declarations.iter().find(|d| d.name == "User").expect("User class");
    assert!(matches!(user.kind, DeclarationKind::Class));
    let method_names: Vec<_> = user.children.iter().map(|c| c.name.clone()).collect();
    assert!(method_names.contains(&"__init__".to_string()));
    assert!(method_names.contains(&"name".to_string()));
}

#[test]
fn parses_inheritance_bases() {
    let r = parse("class Admin(User, Auditable):\n    pass\n");
    let admin = r.declarations.iter().find(|d| d.name == "Admin").unwrap();
    assert_eq!(admin.bases, vec!["User", "Auditable"]);
}
```

- [ ] **Step 3.2: Run to verify it fails**

```bash
cargo test --test adapter_python 2>&1 | tail -5
```

Expected: compile error — `unresolved module 'python'`.

- [ ] **Step 3.3: Adopt `adapters/python.rs` from aeroxy**

Run:

```bash
curl -sL https://raw.githubusercontent.com/aeroxy/ast-outline/main/src/adapters/python.rs \
  > src/adapters/python.rs
```

Then edit `src/adapters/python.rs` and replace every occurrence of `crate::core::` with `crate::core::declaration::` (because aeroxy uses `core.rs` flat, we use `core/declaration.rs`).

Verify substitution covered all uses:

```bash
grep -n 'crate::core' src/adapters/python.rs
```

Expected: every line shows `crate::core::declaration::...`. If any line still says `crate::core::Declaration` etc., fix manually.

- [ ] **Step 3.4: Register `python` in `adapters/mod.rs`**

Edit `src/adapters/mod.rs`, uncomment `pub mod python;`:

```rust
pub mod base;
pub mod python;
```

- [ ] **Step 3.5: Run test to verify it passes**

```bash
cargo test --test adapter_python 2>&1 | tail -10
```

Expected: 3 passed; 0 failed. If aeroxy's adapter assumes `core.rs` flat (still has bare `crate::core::Declaration` references), the compile error message will tell you which line — fix the import path there.

- [ ] **Step 3.6: Verify clippy clean**

```bash
cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
```

- [ ] **Step 3.7: Commit**

```bash
git add src/adapters/python.rs src/adapters/mod.rs tests/adapter_python.rs
git commit -m "feat(adapters): adopt python adapter from aeroxy/ast-outline"
```

---

### Task 4: Adopt `adapters/typescript.rs`

**Files:**
- Create: `src/adapters/typescript.rs`
- Test: `tests/adapter_typescript.rs`
- Modify: `src/adapters/mod.rs`

- [ ] **Step 4.1: Write the failing test**

Create `tests/adapter_typescript.rs`:

```rust
use ast_grep_core::Language;
use ast_grep_language::{LanguageExt, SupportLang};
use repolayer::adapters::base::LanguageAdapter;
use repolayer::adapters::typescript::TypeScriptAdapter;
use repolayer::core::declaration::DeclarationKind;
use std::path::Path;

fn parse_ts(src: &str) -> repolayer::core::declaration::ParseResult {
    let lang = SupportLang::TypeScript;
    let root = lang.ast_grep(src.to_string());
    TypeScriptAdapter.parse(Path::new("test.ts"), src.as_bytes(), root.root())
}

#[test]
fn parses_exported_class_with_methods() {
    let src = "export class User {\n  constructor() {}\n  greet(): string { return 'hi'; }\n}\n";
    let r = parse_ts(src);
    let user = r.declarations.iter().find(|d| d.name == "User").expect("User class");
    assert!(matches!(user.kind, DeclarationKind::Class));
    let method_names: Vec<_> = user.children.iter().map(|c| c.name.clone()).collect();
    assert!(method_names.contains(&"constructor".to_string()));
    assert!(method_names.contains(&"greet".to_string()));
}

#[test]
fn parses_interface() {
    let r = parse_ts("export interface Foo { id: number; name: string; }\n");
    let foo = r.declarations.iter().find(|d| d.name == "Foo").unwrap();
    assert!(matches!(foo.kind, DeclarationKind::Interface));
}

#[test]
fn parses_function_declaration() {
    let r = parse_ts("export function add(a: number, b: number): number { return a + b; }\n");
    let add = r.declarations.iter().find(|d| d.name == "add").unwrap();
    assert!(matches!(add.kind, DeclarationKind::Function));
}
```

- [ ] **Step 4.2: Run to verify it fails**

```bash
cargo test --test adapter_typescript 2>&1 | tail -5
```

Expected: compile error.

- [ ] **Step 4.3: Adopt the adapter**

```bash
curl -sL https://raw.githubusercontent.com/aeroxy/ast-outline/main/src/adapters/typescript.rs \
  > src/adapters/typescript.rs
```

Apply the same `crate::core::` → `crate::core::declaration::` substitution as Task 3.

- [ ] **Step 4.4: Register in `mod.rs`**

```rust
pub mod base;
pub mod python;
pub mod typescript;
```

- [ ] **Step 4.5: Run test, verify it passes**

```bash
cargo test --test adapter_typescript 2>&1 | tail -10
```

Expected: 3 passed.

- [ ] **Step 4.6: Commit**

```bash
git add src/adapters/typescript.rs src/adapters/mod.rs tests/adapter_typescript.rs
git commit -m "feat(adapters): adopt typescript adapter (covers ts/tsx/js/jsx/cjs/mjs)"
```

---

### Task 5: Adopt `adapters/go.rs`

**Files:**
- Create: `src/adapters/go.rs`
- Test: `tests/adapter_go.rs`
- Modify: `src/adapters/mod.rs`

- [ ] **Step 5.1: Write the failing test**

```rust
use ast_grep_core::Language;
use ast_grep_language::{LanguageExt, SupportLang};
use repolayer::adapters::base::LanguageAdapter;
use repolayer::adapters::go::GoAdapter;
use repolayer::core::declaration::DeclarationKind;
use std::path::Path;

fn parse_go(src: &str) -> repolayer::core::declaration::ParseResult {
    let lang = SupportLang::Go;
    let root = lang.ast_grep(src.to_string());
    GoAdapter.parse(Path::new("test.go"), src.as_bytes(), root.root())
}

#[test]
fn parses_exported_func() {
    let r = parse_go("package main\n\nfunc Add(a, b int) int { return a + b }\n");
    let add = r.declarations.iter().find(|d| d.name == "Add").expect("Add func");
    assert!(matches!(add.kind, DeclarationKind::Function));
}

#[test]
fn parses_struct_with_methods() {
    let src = "package main\n\ntype User struct { ID int }\n\nfunc (u *User) Name() string { return \"\" }\n";
    let r = parse_go(src);
    let user = r.declarations.iter().find(|d| d.name == "User").expect("User struct");
    assert!(matches!(user.kind, DeclarationKind::Struct));
    // aeroxy groups methods under their receiver type. Verify Name() is a child of User.
    let method_names: Vec<_> = user.children.iter().map(|c| c.name.clone()).collect();
    assert!(method_names.contains(&"Name".to_string()), "User children: {:?}", method_names);
}
```

- [ ] **Step 5.2: Run to verify it fails**

```bash
cargo test --test adapter_go 2>&1 | tail -5
```

- [ ] **Step 5.3: Adopt the adapter**

```bash
curl -sL https://raw.githubusercontent.com/aeroxy/ast-outline/main/src/adapters/go.rs \
  > src/adapters/go.rs
```

Apply the `crate::core::` → `crate::core::declaration::` substitution.

- [ ] **Step 5.4: Register**

Edit `src/adapters/mod.rs` to add `pub mod go;`.

- [ ] **Step 5.5: Run, verify pass**

```bash
cargo test --test adapter_go 2>&1 | tail -10
```

- [ ] **Step 5.6: Commit**

```bash
git add src/adapters/go.rs src/adapters/mod.rs tests/adapter_go.rs
git commit -m "feat(adapters): adopt go adapter"
```

---

### Task 6: Adopt `adapters/rust.rs`

**Files:**
- Create: `src/adapters/rust.rs`
- Test: `tests/adapter_rust.rs`
- Modify: `src/adapters/mod.rs`

This unblocks dogfooding — we'll be able to index repolayer itself.

- [ ] **Step 6.1: Write the failing test**

```rust
use ast_grep_core::Language;
use ast_grep_language::{LanguageExt, SupportLang};
use repolayer::adapters::base::LanguageAdapter;
use repolayer::adapters::rust::RustAdapter;
use repolayer::core::declaration::DeclarationKind;
use std::path::Path;

fn parse_rust(src: &str) -> repolayer::core::declaration::ParseResult {
    let lang = SupportLang::Rust;
    let root = lang.ast_grep(src.to_string());
    RustAdapter.parse(Path::new("test.rs"), src.as_bytes(), root.root())
}

#[test]
fn parses_struct() {
    let r = parse_rust("pub struct User { pub id: u64, name: String }\n");
    let user = r.declarations.iter().find(|d| d.name == "User").unwrap();
    assert!(matches!(user.kind, DeclarationKind::Struct));
}

#[test]
fn parses_trait_with_methods() {
    let src = "pub trait Greeter {\n    fn greet(&self) -> String;\n}\n";
    let r = parse_rust(src);
    let g = r.declarations.iter().find(|d| d.name == "Greeter").unwrap();
    // aeroxy maps Rust `trait` to canonical Interface kind
    assert!(matches!(g.kind, DeclarationKind::Interface));
    let methods: Vec<_> = g.children.iter().map(|c| c.name.clone()).collect();
    assert!(methods.contains(&"greet".to_string()));
}

#[test]
fn parses_impl_block_groups_methods_under_struct() {
    let src = "pub struct U;\n\nimpl U {\n    pub fn new() -> Self { Self }\n    pub fn name(&self) -> String { String::new() }\n}\n";
    let r = parse_rust(src);
    let u = r.declarations.iter().find(|d| d.name == "U").unwrap();
    let methods: Vec<_> = u.children.iter().map(|c| c.name.clone()).collect();
    assert!(methods.contains(&"new".to_string()));
    assert!(methods.contains(&"name".to_string()));
}
```

- [ ] **Step 6.2: Run, fail**

```bash
cargo test --test adapter_rust 2>&1 | tail -5
```

- [ ] **Step 6.3: Adopt**

```bash
curl -sL https://raw.githubusercontent.com/aeroxy/ast-outline/main/src/adapters/rust.rs \
  > src/adapters/rust.rs
```

Apply substitution.

- [ ] **Step 6.4: Register**

Edit `src/adapters/mod.rs` adding `pub mod rust;`.

- [ ] **Step 6.5: Run, pass**

```bash
cargo test --test adapter_rust 2>&1 | tail -10
```

- [ ] **Step 6.6: Commit**

```bash
git add src/adapters/rust.rs src/adapters/mod.rs tests/adapter_rust.rs
git commit -m "feat(adapters): adopt rust adapter (unblocks dogfood)"
```

---

### Task 7: Adopt `adapters/csharp.rs`

**Files:**
- Create: `src/adapters/csharp.rs`
- Test: `tests/adapter_csharp.rs`
- Modify: `src/adapters/mod.rs`

- [ ] **Step 7.1: Write the failing test**

```rust
use ast_grep_core::Language;
use ast_grep_language::{LanguageExt, SupportLang};
use repolayer::adapters::base::LanguageAdapter;
use repolayer::adapters::csharp::CSharpAdapter;
use repolayer::core::declaration::DeclarationKind;
use std::path::Path;

fn parse_cs(src: &str) -> repolayer::core::declaration::ParseResult {
    let lang = SupportLang::CSharp;
    let root = lang.ast_grep(src.to_string());
    CSharpAdapter.parse(Path::new("test.cs"), src.as_bytes(), root.root())
}

#[test]
fn parses_class_with_method() {
    let src = "namespace App {\n  public class User {\n    public string Name() => \"\";\n  }\n}\n";
    let r = parse_cs(src);
    // walk into namespace if needed
    let user = find_named(&r.declarations, "User").expect("User class found");
    assert!(matches!(user.kind, DeclarationKind::Class));
    let methods: Vec<_> = user.children.iter().map(|c| c.name.clone()).collect();
    assert!(methods.contains(&"Name".to_string()));
}

fn find_named<'a>(decls: &'a [repolayer::core::declaration::Declaration], name: &str)
    -> Option<&'a repolayer::core::declaration::Declaration>
{
    for d in decls {
        if d.name == name { return Some(d); }
        if let Some(found) = find_named(&d.children, name) { return Some(found); }
    }
    None
}
```

- [ ] **Step 7.2: Run, fail**

```bash
cargo test --test adapter_csharp 2>&1 | tail -5
```

- [ ] **Step 7.3: Adopt**

```bash
curl -sL https://raw.githubusercontent.com/aeroxy/ast-outline/main/src/adapters/csharp.rs \
  > src/adapters/csharp.rs
```

Substitute. Register in `mod.rs` (`pub mod csharp;`).

- [ ] **Step 7.4: Run, pass**

```bash
cargo test --test adapter_csharp 2>&1 | tail -10
```

- [ ] **Step 7.5: Commit**

```bash
git add src/adapters/csharp.rs src/adapters/mod.rs tests/adapter_csharp.rs
git commit -m "feat(adapters): adopt csharp adapter"
```

---

### Task 8: Adopt `adapters/java.rs`

**Files:**
- Create: `src/adapters/java.rs`
- Test: `tests/adapter_java.rs`
- Modify: `src/adapters/mod.rs`

- [ ] **Step 8.1: Write the failing test**

```rust
use ast_grep_core::Language;
use ast_grep_language::{LanguageExt, SupportLang};
use repolayer::adapters::base::LanguageAdapter;
use repolayer::adapters::java::JavaAdapter;
use repolayer::core::declaration::DeclarationKind;
use std::path::Path;

fn parse_java(src: &str) -> repolayer::core::declaration::ParseResult {
    let lang = SupportLang::Java;
    let root = lang.ast_grep(src.to_string());
    JavaAdapter.parse(Path::new("Test.java"), src.as_bytes(), root.root())
}

#[test]
fn parses_class_with_method() {
    let src = "public class User {\n  public String greet() { return \"\"; }\n}\n";
    let r = parse_java(src);
    let user = r.declarations.iter().find(|d| d.name == "User").unwrap();
    assert!(matches!(user.kind, DeclarationKind::Class));
    let methods: Vec<_> = user.children.iter().map(|c| c.name.clone()).collect();
    assert!(methods.contains(&"greet".to_string()));
}

#[test]
fn parses_interface() {
    let r = parse_java("public interface Greeter { String greet(); }\n");
    let g = r.declarations.iter().find(|d| d.name == "Greeter").unwrap();
    assert!(matches!(g.kind, DeclarationKind::Interface));
}
```

- [ ] **Step 8.2: Run, fail.** Expected compile error.

- [ ] **Step 8.3: Adopt**

```bash
curl -sL https://raw.githubusercontent.com/aeroxy/ast-outline/main/src/adapters/java.rs \
  > src/adapters/java.rs
```

Substitute. Register `pub mod java;`.

- [ ] **Step 8.4: Run, pass**

```bash
cargo test --test adapter_java 2>&1 | tail -10
```

- [ ] **Step 8.5: Commit**

```bash
git add src/adapters/java.rs src/adapters/mod.rs tests/adapter_java.rs
git commit -m "feat(adapters): adopt java adapter"
```

---

### Task 9: Adopt `adapters/kotlin.rs`

**Files:**
- Create: `src/adapters/kotlin.rs`
- Test: `tests/adapter_kotlin.rs`
- Modify: `src/adapters/mod.rs`

- [ ] **Step 9.1: Write the failing test**

```rust
use ast_grep_core::Language;
use ast_grep_language::{LanguageExt, SupportLang};
use repolayer::adapters::base::LanguageAdapter;
use repolayer::adapters::kotlin::KotlinAdapter;
use repolayer::core::declaration::DeclarationKind;
use std::path::Path;

fn parse_kt(src: &str) -> repolayer::core::declaration::ParseResult {
    let lang = SupportLang::Kotlin;
    let root = lang.ast_grep(src.to_string());
    KotlinAdapter.parse(Path::new("test.kt"), src.as_bytes(), root.root())
}

#[test]
fn parses_class() {
    let r = parse_kt("class User(val id: Int) { fun name(): String = \"\" }\n");
    let user = r.declarations.iter().find(|d| d.name == "User").unwrap();
    assert!(matches!(user.kind, DeclarationKind::Class));
    let methods: Vec<_> = user.children.iter().map(|c| c.name.clone()).collect();
    assert!(methods.contains(&"name".to_string()));
}

#[test]
fn parses_data_class_native_kind() {
    let r = parse_kt("data class Pair(val a: Int, val b: Int)\n");
    let p = r.declarations.iter().find(|d| d.name == "Pair").unwrap();
    // canonical kind is Class; native_kind reflects "data class"
    assert!(matches!(p.kind, DeclarationKind::Class));
    // native_kind population happens in markers.rs (Task 12) — for now it can be None.
}
```

- [ ] **Step 9.2: Run, fail.**

- [ ] **Step 9.3: Adopt**

```bash
curl -sL https://raw.githubusercontent.com/aeroxy/ast-outline/main/src/adapters/kotlin.rs \
  > src/adapters/kotlin.rs
```

Substitute. Register `pub mod kotlin;`.

- [ ] **Step 9.4: Run, pass**

```bash
cargo test --test adapter_kotlin 2>&1 | tail -10
```

- [ ] **Step 9.5: Commit**

```bash
git add src/adapters/kotlin.rs src/adapters/mod.rs tests/adapter_kotlin.rs
git commit -m "feat(adapters): adopt kotlin adapter"
```

---

### Task 10: Adopt `adapters/scala.rs`

**Files:**
- Create: `src/adapters/scala.rs`
- Test: `tests/adapter_scala.rs`
- Modify: `src/adapters/mod.rs`

- [ ] **Step 10.1: Write the failing test**

```rust
use ast_grep_core::Language;
use ast_grep_language::{LanguageExt, SupportLang};
use repolayer::adapters::base::LanguageAdapter;
use repolayer::adapters::scala::ScalaAdapter;
use repolayer::core::declaration::DeclarationKind;
use std::path::Path;

fn parse_scala(src: &str) -> repolayer::core::declaration::ParseResult {
    let lang = SupportLang::Scala;
    let root = lang.ast_grep(src.to_string());
    ScalaAdapter.parse(Path::new("test.scala"), src.as_bytes(), root.root())
}

#[test]
fn parses_class() {
    let r = parse_scala("class User(id: Int) {\n  def name(): String = \"\"\n}\n");
    let u = r.declarations.iter().find(|d| d.name == "User").unwrap();
    assert!(matches!(u.kind, DeclarationKind::Class));
    let methods: Vec<_> = u.children.iter().map(|c| c.name.clone()).collect();
    assert!(methods.contains(&"name".to_string()));
}

#[test]
fn parses_trait() {
    let r = parse_scala("trait Greeter { def greet: String }\n");
    let g = r.declarations.iter().find(|d| d.name == "Greeter").unwrap();
    assert!(matches!(g.kind, DeclarationKind::Interface));
}
```

- [ ] **Step 10.2: Run, fail.**

- [ ] **Step 10.3: Adopt**

```bash
curl -sL https://raw.githubusercontent.com/aeroxy/ast-outline/main/src/adapters/scala.rs \
  > src/adapters/scala.rs
```

Substitute. Register `pub mod scala;`.

- [ ] **Step 10.4: Run, pass**

```bash
cargo test --test adapter_scala 2>&1 | tail -10
```

- [ ] **Step 10.5: Commit**

```bash
git add src/adapters/scala.rs src/adapters/mod.rs tests/adapter_scala.rs
git commit -m "feat(adapters): adopt scala adapter"
```

---

### Task 11: Adopt `adapters/markdown.rs`

**Files:**
- Create: `src/adapters/markdown.rs`
- Test: `tests/adapter_markdown.rs`
- Modify: `src/adapters/mod.rs`
- Modify: `Cargo.toml` — add `tree-sitter-md`

The markdown adapter doesn't use `ast-grep-language` (Markdown isn't in SupportLang); it uses `tree-sitter-md` directly. Aeroxy ships it as a special case — we follow.

- [ ] **Step 11.1: Add `tree-sitter-md` to `Cargo.toml`**

Add to `[dependencies]`:

```toml
tree-sitter-md = "0.5.3"
```

- [ ] **Step 11.2: Write the failing test**

```rust
use repolayer::adapters::markdown::parse_markdown;
use repolayer::core::declaration::DeclarationKind;
use std::path::Path;

#[test]
fn parses_headings() {
    let src = "# Title\n\n## Section A\n\n### Sub A1\n\n## Section B\n";
    let r = parse_markdown(Path::new("doc.md"), src.as_bytes());
    let names: Vec<_> = r.declarations.iter().map(|d| d.name.clone()).collect();
    assert!(names.contains(&"Title".to_string()));
    let title = r.declarations.iter().find(|d| d.name == "Title").unwrap();
    assert!(matches!(title.kind, DeclarationKind::Heading));
}
```

- [ ] **Step 11.3: Run, fail.**

- [ ] **Step 11.4: Adopt**

```bash
curl -sL https://raw.githubusercontent.com/aeroxy/ast-outline/main/src/adapters/markdown.rs \
  > src/adapters/markdown.rs
```

The markdown adapter exposes `parse_markdown(path, source)` directly (not via `LanguageAdapter` trait, because tree-sitter-md isn't in `SupportLang`). Substitute imports as before.

Register `pub mod markdown;` in `adapters/mod.rs`.

- [ ] **Step 11.5: Run, pass**

```bash
cargo test --test adapter_markdown 2>&1 | tail -10
```

- [ ] **Step 11.6: Commit**

```bash
git add src/adapters/markdown.rs src/adapters/mod.rs tests/adapter_markdown.rs Cargo.toml Cargo.lock
git commit -m "feat(adapters): adopt markdown adapter (tree-sitter-md)"
```

---

### Task 12: Adopt `core/markers.rs` — populate native_kind / modifiers / deprecated

**Files:**
- Modify: `src/core/markers.rs`
- Test: `tests/core_markers.rs`

Aeroxy's `core.rs` has a long marker post-processing block (~200-1268 lines). It walks each adapter's `Declaration` tree and populates `native_kind`, `modifiers`, `deprecated` based on language conventions in `attrs` and `signature`. Adapters stay focused on tree traversal; markers add presentation polish.

- [ ] **Step 12.1: Write the failing test**

Create `tests/core_markers.rs`:

```rust
use repolayer::core::declaration::{Declaration, DeclarationKind};
use repolayer::core::markers::populate_markers;

#[test]
fn populates_async_modifier_from_signature() {
    let mut decls = vec![Declaration {
        kind: DeclarationKind::Method,
        name: "fetch".into(),
        signature: "async fn fetch(&self) -> Result<()>".into(),
        ..Default::default()
    }];
    populate_markers(&mut decls, "rust");
    assert!(decls[0].modifiers.iter().any(|m| m == "async"));
}

#[test]
fn detects_deprecated_attribute() {
    let mut decls = vec![Declaration {
        kind: DeclarationKind::Function,
        name: "old".into(),
        signature: "fn old()".into(),
        attrs: vec!["#[deprecated]".into()],
        ..Default::default()
    }];
    populate_markers(&mut decls, "rust");
    assert!(decls[0].deprecated);
}

#[test]
fn rust_trait_native_kind() {
    let mut decls = vec![Declaration {
        kind: DeclarationKind::Interface,
        name: "Greeter".into(),
        signature: "trait Greeter".into(),
        ..Default::default()
    }];
    populate_markers(&mut decls, "rust");
    assert_eq!(decls[0].native_kind.as_deref(), Some("trait"));
}
```

- [ ] **Step 12.2: Run, fail (still has stub).**

- [ ] **Step 12.3: Adopt the marker logic from aeroxy**

Replace the stub at `src/core/markers.rs` with the contents of aeroxy `src/core.rs` lines 200–end (the marker post-processing section). The fastest way:

```bash
curl -sL https://raw.githubusercontent.com/aeroxy/ast-outline/main/src/core.rs \
  | sed -n '/^\/\/ --- Marker post-processing ---/,$p' \
  > /tmp/aeroxy_markers.rs
wc -l /tmp/aeroxy_markers.rs
```

Open `/tmp/aeroxy_markers.rs` and `src/core/markers.rs`, copy the content (everything from `// --- Marker post-processing ---` to end of file). Wrap with the existing module header:

```rust
//! Marker post-processing: derives `native_kind`, `modifiers`, `deprecated`
//! fields on Declarations from the language-native conventions captured
//! by adapters in `attrs`/`signature`.

use crate::core::declaration::{Declaration, DeclarationKind};

// ... (paste the marker logic here, with `crate::core::Declaration` -> `crate::core::declaration::Declaration` substitution)

pub fn populate_markers(decls: &mut [Declaration], language: &'static str) {
    // ... (keep aeroxy's implementation)
}
```

- [ ] **Step 12.4: Run, pass**

```bash
cargo test --test core_markers 2>&1 | tail -10
```

Expected: 3 passed.

- [ ] **Step 12.5: Re-run all adapter tests to make sure markers don't break them**

```bash
cargo test --test adapter_python --test adapter_typescript --test adapter_go \
           --test adapter_rust --test adapter_csharp --test adapter_java \
           --test adapter_kotlin --test adapter_scala --test adapter_markdown 2>&1 | grep -E "^test result:"
```

Expected: all 9 lines say `ok. N passed; 0 failed`.

- [ ] **Step 12.6: Commit**

```bash
git add src/core/markers.rs tests/core_markers.rs
git commit -m "feat(core): adopt marker post-processing (native_kind/modifiers/deprecated)"
```

---

### Task 13: Move `parser/idl/` → `adapters/idl/`

**Files:**
- Move: `src/parser/idl/*` → `src/adapters/idl/*`
- Modify: `src/parser/mod.rs` (remove `pub mod idl;`)
- Modify: `src/adapters/mod.rs` (add `pub mod idl;`)
- Modify: any callers of `crate::parser::idl::*` (only `src/indexer/mod.rs` lines 148, 168 currently)

The IDL parsers don't change shape — they still use bare tree-sitter and emit IDL-specific output (services + methods, not `Declaration`). The move is just for code organization.

- [ ] **Step 13.1: Move files**

```bash
git mv src/parser/idl src/adapters/idl
```

- [ ] **Step 13.2: Re-export from `adapters/mod.rs`**

Edit `src/adapters/mod.rs`:

```rust
pub mod base;
pub mod python;
pub mod typescript;
pub mod go;
pub mod rust;
pub mod csharp;
pub mod java;
pub mod kotlin;
pub mod scala;
pub mod markdown;
pub mod idl;
```

- [ ] **Step 13.3: Remove from `parser/mod.rs`**

Edit `src/parser/mod.rs` — delete the line `pub mod idl;`.

- [ ] **Step 13.4: Update callers**

Find all uses of `crate::parser::idl`:

```bash
grep -rn "parser::idl" src/
```

Expected: hits in `src/indexer/mod.rs` (around lines 148, 168 in the `index_idl_repo` fn). Replace with `crate::adapters::idl`.

- [ ] **Step 13.5: Run all tests**

```bash
cargo test 2>&1 | grep -E "^test result:"
```

Expected: all rows say `ok`. The existing `parser_protobuf.rs` and `parser_thrift.rs` tests — they import `repolayer::parser::idl::*`. Find and update:

```bash
grep -rn "parser::idl" tests/
```

Expected: `tests/parser_protobuf.rs`, `tests/parser_thrift.rs`. Replace `parser::idl` with `adapters::idl` in those two files.

- [ ] **Step 13.6: Re-run, all green**

```bash
cargo test 2>&1 | grep -E "^test result:"
```

- [ ] **Step 13.7: Commit**

```bash
git add -A
git commit -m "refactor: move parser/idl/ to adapters/idl/ (no behavioral change)"
```

---

### Task 14: `adapters::parse_file` dispatcher (replaces `main_helpers::parse_file_for_hook`)

**Files:**
- Modify: `src/adapters/mod.rs`
- Test: `tests/adapter_dispatch.rs`

A single entry point that picks the right adapter based on file extension and returns a `ParseResult`. Adopted from aeroxy `src/main_helpers.rs:parse_file_for_hook`.

- [ ] **Step 14.1: Write the failing test**

Create `tests/adapter_dispatch.rs`:

```rust
use repolayer::adapters::parse_file;
use repolayer::core::declaration::DeclarationKind;
use std::io::Write;
use tempfile::NamedTempFile;

fn write_temp(suffix: &str, content: &str) -> NamedTempFile {
    let mut f = tempfile::Builder::new().suffix(suffix).tempfile().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f
}

#[test]
fn dispatches_python() {
    let f = write_temp(".py", "def hello():\n    pass\n");
    let r = parse_file(f.path()).expect("parsed");
    assert_eq!(r.language, "python");
    assert!(r.declarations.iter().any(|d| d.name == "hello"));
}

#[test]
fn dispatches_typescript() {
    let f = write_temp(".ts", "export function add(a: number) { return a; }\n");
    let r = parse_file(f.path()).expect("parsed");
    assert_eq!(r.language, "typescript");
    assert!(r.declarations.iter().any(|d| d.name == "add"));
}

#[test]
fn dispatches_rust() {
    let f = write_temp(".rs", "pub fn add() {}\n");
    let r = parse_file(f.path()).expect("parsed");
    assert_eq!(r.language, "rust");
    assert!(r.declarations.iter().any(|d| d.name == "add"));
}

#[test]
fn dispatches_markdown() {
    let f = write_temp(".md", "# Title\n\n## Section\n");
    let r = parse_file(f.path()).expect("parsed");
    assert_eq!(r.language, "markdown");
    assert!(r.declarations.iter().any(|d| d.kind == DeclarationKind::Heading));
}

#[test]
fn returns_none_for_unknown_extension() {
    let f = write_temp(".xyz", "...");
    assert!(parse_file(f.path()).is_none());
}

#[test]
fn populates_markers_after_dispatch() {
    let f = write_temp(".rs", "trait T {}\n");
    let r = parse_file(f.path()).expect("parsed");
    let t = r.declarations.iter().find(|d| d.name == "T").unwrap();
    assert!(matches!(t.kind, DeclarationKind::Interface));
    assert_eq!(t.native_kind.as_deref(), Some("trait"));
}
```

- [ ] **Step 14.2: Run, fail (compile error: `parse_file` not found)**

- [ ] **Step 14.3: Implement the dispatcher in `adapters/mod.rs`**

Replace `src/adapters/mod.rs` content with:

```rust
//! Source-language adapters built on `ast-grep-core`.
//!
//! Each adapter implements [`base::LanguageAdapter`] for one language
//! family. IDL parsers under [`idl`] use bare tree-sitter and emit a
//! different output shape; they are dispatched separately by the indexer.

pub mod base;
pub mod python;
pub mod typescript;
pub mod go;
pub mod rust;
pub mod csharp;
pub mod java;
pub mod kotlin;
pub mod scala;
pub mod markdown;
pub mod idl;

use std::path::Path;
use ast_grep_core::Language;
use ast_grep_language::{LanguageExt, SupportLang};
use crate::core::declaration::ParseResult;
use crate::core::populate_markers;
use base::LanguageAdapter;

/// Parse a single file, returning a `ParseResult` if the extension is
/// supported by any adapter. Returns `None` for unknown extensions.
///
/// IDL files (`.proto`, `.thrift`) are NOT dispatched here — the indexer
/// handles them via `crate::adapters::idl` directly because they emit a
/// different output type (services/methods rather than Declarations).
pub fn parse_file(path: &Path) -> Option<ParseResult> {
    let source = std::fs::read_to_string(path).ok()?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    // Markdown has its own grammar (tree-sitter-md), not ast-grep-language.
    if matches!(ext, "md" | "markdown" | "mdx" | "mdown") {
        let mut r = markdown::parse_markdown(path, source.as_bytes());
        populate_markers(&mut r.declarations, r.language);
        return Some(r);
    }

    let lang = SupportLang::from_path(path)?;
    let mut result = match lang {
        SupportLang::Rust => rust::RustAdapter.parse(
            path, source.as_bytes(), lang.ast_grep(source.clone()).root(),
        ),
        SupportLang::Python => python::PythonAdapter.parse(
            path, source.as_bytes(), lang.ast_grep(source.clone()).root(),
        ),
        SupportLang::TypeScript | SupportLang::Tsx | SupportLang::JavaScript => {
            typescript::TypeScriptAdapter.parse(
                path, source.as_bytes(), lang.ast_grep(source.clone()).root(),
            )
        }
        SupportLang::CSharp => csharp::CSharpAdapter.parse(
            path, source.as_bytes(), lang.ast_grep(source.clone()).root(),
        ),
        SupportLang::Go => go::GoAdapter.parse(
            path, source.as_bytes(), lang.ast_grep(source.clone()).root(),
        ),
        SupportLang::Java => java::JavaAdapter.parse(
            path, source.as_bytes(), lang.ast_grep(source.clone()).root(),
        ),
        SupportLang::Kotlin => kotlin::KotlinAdapter.parse(
            path, source.as_bytes(), lang.ast_grep(source.clone()).root(),
        ),
        SupportLang::Scala => scala::ScalaAdapter.parse(
            path, source.as_bytes(), lang.ast_grep(source.clone()).root(),
        ),
        _ => return None,
    };

    // Central marker enrichment so adapters stay focused on tree walking.
    populate_markers(&mut result.declarations, result.language);
    Some(result)
}
```

- [ ] **Step 14.4: Add `tempfile` to dev-dependencies if not already**

```bash
grep tempfile Cargo.toml
```

Expected: already present (existing dev-dep). If missing, add:

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 14.5: Run, pass**

```bash
cargo test --test adapter_dispatch 2>&1 | tail -10
```

Expected: 6 passed.

- [ ] **Step 14.6: Run full test suite to confirm no regression**

```bash
cargo test 2>&1 | grep -E "^test result:" | awk '{ p+=$4; f+=$6 } END { print "passed:", p, "failed:", f }'
```

Expected: at least 88 passed (62 baseline + ~26 new), 0 failed.

- [ ] **Step 14.7: Verify clippy clean**

```bash
cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
```

- [ ] **Step 14.8: Commit**

```bash
git add src/adapters/mod.rs tests/adapter_dispatch.rs
git commit -m "feat(adapters): add parse_file dispatcher unifying all 9 source adapters + markdown"
```

---

### Task 15: Plan A wrap-up — release build + final verification

- [ ] **Step 15.1: Release build**

```bash
cargo build --release 2>&1 | tail -5
```

Expected: `Finished \`release\` profile [optimized] target(s)`. Note the binary size increase (12 MB → ~25 MB expected).

- [ ] **Step 15.2: Confirm binary still works**

```bash
./target/release/repolayer --help
./target/release/repolayer init --help
```

Expected: clap help output. The CLI hasn't changed — Plan A is internal-only.

- [ ] **Step 15.3: Run baseline dogfood (existing fixture)**

```bash
WS=$(mktemp -d)
cp -r tests/fixtures/single_repo_ts "$WS/repo"
cd "$WS"
printf 'repos:\n  - path: ./repo\n' > repolayer.yml
/Users/bytedance/code/repolayer/.worktrees/ast-outline-ext/target/release/repolayer build
ls -la .repolayer/
cd -
rm -rf "$WS"
```

Expected: `indexed N nodes, M edges`. The current indexer path still uses `crate::parser::*`, so output should match pre-Plan-A behavior. (This proves we haven't broken anything.)

- [ ] **Step 15.4: Commit final tag**

```bash
git tag -a plan-a-complete -m "Plan A complete: parser foundation (Declaration IR + 10 adapters)"
```

- [ ] **Step 15.5: Print summary**

Run:

```bash
echo "=== Plan A summary ==="
echo "Files added under src/core/:"
ls src/core/
echo ""
echo "Files added under src/adapters/:"
ls src/adapters/
echo ""
echo "New tests:"
ls tests/adapter_*.rs tests/core_*.rs
echo ""
echo "Test count:"
cargo test 2>&1 | grep -E "^test result:" | awk '{ p+=$4 } END { print p, "tests passing" }'
echo ""
echo "Binary size (release):"
ls -lh target/release/repolayer
```

Expected: ~88+ tests passing, binary ~25 MB.

---

## Self-review checklist (executed by plan author before handoff)

**1. Spec coverage:** Plan A covers spec sections §4.1 (`Declaration` IR) and §6 (Adapter layer). Out-of-scope items (storage split, indexer rewrite, MCP changes, etc.) are explicitly deferred to Plans B/C. ✓

**2. Placeholder scan:** No "TBD/TODO/etc." in actionable steps. The marker module ships as a no-op stub in Task 1 with a clear pointer to Task 12 — this is intentional staging, not a placeholder. ✓

**3. Type consistency:** `parse_file` (Task 14) returns `Option<ParseResult>` matching `core::declaration::ParseResult` (Task 1). `LanguageAdapter::parse` signature matches across all adapter tasks. `populate_markers` signature in Task 12 matches stub from Task 1.6. ✓

**4. Adopted code paths:** Every "ADOPTED" step gives an exact `curl` command and the import-rewrite rule. The rewrite is mechanical (`crate::core::` → `crate::core::declaration::`). If aeroxy adapters import `crate::core::Declaration` directly (which they do), the substitution covers it. ✓

**5. Existing test preservation:** Plan A does NOT modify any existing test file except `parser_protobuf.rs` and `parser_thrift.rs` in Task 13 (pure module-path update from `parser::idl` → `adapters::idl`). Old `parser_typescript.rs` etc. remain — they test `parser/typescript.rs` which still exists. They are deleted in Plan B. ✓

---

## Handoff

After all 15 tasks pass: **proceed to Plan B (storage + indexer + linker rewrite)**, which depends on `core::Declaration` and `adapters::parse_file` being stable.
