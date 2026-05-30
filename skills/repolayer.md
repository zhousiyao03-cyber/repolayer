---
name: repolayer
description: |
  Cross-repo code-index CLI. Reach for it first when the user asks
  "where is X defined", "who calls X", "what implements this IDL method",
  "what's the full chain behind this service / RPC", or anything else that
  involves navigating multiple repos. It is faster and cheaper than
  grep / find / reading whole files, and it knows about cross-repo
  Imports / Calls / Implements edges. It also covers exact symbol lookup,
  hybrid (BM25 + semantic) search, outline / function-body extraction,
  and dependency-graph queries inside a single repo.

  When `$REPOLAYER_INDEX` is set, read-only queries resolve against that
  central workspace even when the current directory has no `.repolayer/`.
---

# repolayer

A pre-built multi-repo code index served via a CLI. The `.repolayer/`
directory under the workspace root holds four SQLite stores (graph /
outline / deps / search). `repolayer build` produces them from scratch;
`repolayer update` does an incremental refresh based on `git diff`.

Binary location: `which repolayer`.

**Index location.** Read-only commands look at the current working
directory's `.repolayer/` first. If `REPOLAYER_INDEX=<dir>` is set, the
queries that read *only* the index (`query` / `search` / `callers` /
`find-idl-impl` / `find-related` / `view`) switch to that directory.
This is for the "I'm editing in business repo A but want cross-repo
answers from a shared workspace" workflow. Commands that also touch
source files (`outline` / `show` / `digest` / `surface` / `deps` /
`reverse-deps` / `cycles`) keep using cwd to resolve relative paths,
so cd into the target repo first. Index-writing commands (`build` /
`update` / `init`) always bind to cwd to avoid surprising writes.

If `.repolayer/` is missing or `repolayer.yml` isn't set up, queries
exit with "no index found" and a hint to run `repolayer build` (or set
`$REPOLAYER_INDEX`).

## Decision table — read this first

| Starting point | Command |
|---|---|
| You know the symbol name (exact or substring), incl. IDL methods | `repolayer query "Name"` |
| Same, but restricted to one repo | `repolayer query "Name" --repo <name>` |
| **Who calls a function** (works across repos for unique names) | `repolayer callers <symbol>` |
| **Server / client of an IDL method** sorted by confidence | `repolayer find-idl-impl <Method>` |
| Keyword or behaviour description, you don't know the symbol | `repolayer search "..."` |
| Same, narrowed to one repo | `repolayer search "..." --repo <name>` |
| URL / API path / literal string (e.g. `/api/v1/...`) | `repolayer search "/api/v1/..."`, then narrow |
| Skim the structure of one file | `repolayer outline <file>` |
| Pull one function body out of a file | `repolayer show <file> <symbol>` |
| One-page public-API map of a directory or package | `repolayer digest <dir>` / `repolayer surface <dir>` |
| What does X import | `repolayer deps <file>` |
| Who imports X | `repolayer reverse-deps <file>` |
| Code chunks similar to `<file>:<line>` | `repolayer find-related <file>:<line>` |
| Are there import cycles | `repolayer cycles` |

**⚠️ cwd rules (v0.2 behaviour):**

- **Index-only commands, cwd doesn't matter:** `query`, `search`,
  `callers`, `find-idl-impl`. These read `index.db` / `search.db` only;
  paths come back with the repo prefix already baked in, so they work
  from anywhere (including `~`, `/tmp`, or the repolayer source tree).
- **Source-reading commands, cwd must be inside the target repo:**
  `outline`, `show`, `digest`, `surface`, `deps`, `reverse-deps`,
  `find-related`, `cycles`. These resolve paths relative to cwd; running
  them from the wrong directory yields `path not found` or
  `no adapter for ...`. Workflow: get an absolute path from `query` /
  `search`, then `cd <repo-root>` before invoking.
  Note that some session hooks reset cwd between commands, so prefer
  `cd /path/to/<repo> && repolayer outline path/to/handler.go`
  rather than relying on a previous `cd` line.

**The standard cross-cutting trace for an API or IDL endpoint:**

```
# Step 1 — single query, returns every relevant node across repos
repolayer query "<MethodName>"
# Hits: BE handlers in every repo + IDL definitions (http_idl / rpc_idl)
#       + TS stubs + router registrations

# Step 2 — pull function bodies once you've picked the right hit
cd /path/to/<be-repo> && repolayer outline path/to/<file>.go
cd /path/to/<be-repo> && repolayer show path/to/<file>.go <Method>

# Step 3 — bonus: server-side impls of the IDL method, ranked by confidence
repolayer find-idl-impl <MethodName>

# Step 4 — optional: find frontend call sites by URL
repolayer search "/api/v1/<path>"
```

**Do not** grep IDL files manually (`find ... -name "*.proto" | xargs grep`).
`query` already includes `idlmethod` / `idlservice` nodes; IDL hits
land in the same result set as BE handlers.

**Default ordering:** `query` / `search` to locate, then `outline` for
structure, then `show` to extract the body. Don't `Read` an entire file
unless outline / show didn't give you enough context.

**Prefer `--repo` in large workspaces.** In a 40+ repo workspace,
cross-repo BM25 noise can push the right hit out of top-K. If
you already know the repo, `--repo` recomputes IDF inside that repo so
results stop fighting unrelated workspace terms. Typos produce a
"did you mean ..." with the five closest names — pick one and retry.

**🚫 Fallback discipline (don't break these):**

1. **`repolayer show` saying `no adapter for ...`** means *that one
   file type* isn't supported (today: `.proto` / `.thrift`). Don't
   abandon repolayer because of it — keep using `repolayer query
   "<Method>"` for IDL nodes and finish locating with
   `grep -n "<Method>" <single-proto-file>`. Never escalate to a
   tree-wide `grep -r`.

2. **Never** `grep -r` / `grep -rln` / `find` across:
   - the workspace home dir (dozens of repos plus
     node_modules, minutes long)
   - a large frontend monorepo's `packages/` (~2 min)

   The harness turns commands that big into a background task you have
   to poll. Use `repolayer search "<term>"` (milliseconds) or
   `repolayer search "..." --repo <name>` instead — easily 100× faster.

3. **`repolayer query` / `search` returning zero hits** doesn't mean the
   index is broken. Common causes:
   - The query string is wrong (try snake_case vs camelCase, swap order).
   - The repo isn't in the index (check `repolayer.yml`; ask the user
     to add it rather than falling back to `grep -r ~/`).
   - It really doesn't exist (`rg` inside one repo is fine for tie-break,
     but still don't scan `~/`).

---

## Command reference

### `repolayer query <text> [--repo <name>] [--json]`

Substring match against declaration symbols across all repos. Kinds in
the hit set: `type` / `method` / `function` / **`idlmethod`** /
**`idlservice`**. Matches against both the symbol name and the file
path; returns up to 20 rows as `repo \t path::symbol \t line`.

IDL is included: tracing an endpoint via `query "GetXxx"` returns BE
handlers, http_idl proto rpc definitions, and rpc_idl thrift methods in
one shot — no separate grep over `.proto` / `.thrift`.

`--repo <name>` restricts to one repo (must match a name in
`repolayer.yml`; typos error out with a "did you mean ..." list). Prefer
`--repo` when you know the answer's location.

```
$ repolayer query "GetDiscountList"
# 20 matches for 'GetDiscountList' — repo	path::symbol	line
discount_api	handler.go::GetDiscountList	206
discount_api	internal/handler/get_discount_list.go::NewGetDiscountListHandler	63
...
```

`--json` returns `{schema_version, query, repo_filter, matches: [{repo,
path, symbol, kind, line}]}`. Zero hits exits with code 0 and a
fallback hint on stdout (try `search`, try `rg` inside one repo).

### `repolayer callers <symbol> [--depth N] [--repo <name>] [--json]`

Inbound `Calls` edges for `symbol` — i.e. "who calls this." Aggregates
across **every node whose symbol matches exactly**, so a function
defined in multiple repos surfaces all of its caller sets at once.
Each row pairs the caller with the target it reaches, so multi-definition
results don't get conflated.

Two sources feed `Calls` edges into the graph:

1. **Auto-extracted at build time** (default): the indexer walks
   ast-grep call expressions and emits a Calls edge from the caller's
   file (`Module` node) to the callee `Function` / `Method` node **only
   when the callee name resolves uniquely across the workspace**.
   Confidence is therefore always 1.0 for these edges. Ambiguous names
   (`init`, `Get`, `parse`, lowercase short words) are skipped to keep
   noise down.
2. **Manually declared** via `links: [{kind: calls, from: <repo>, to:
   <repo>}]` in `repolayer.yml`. Edge granularity is repo-level here,
   not function-level.

Caller granularity for auto-extracted edges is **the file**, not the
enclosing function. The CLI prints `caller -> target` lines so you can
see exactly which file reached which target. Once you've picked a
caller file, follow up with `repolayer outline <caller-path>` and
`repolayer show <caller-path> <function>` to pinpoint the call site.

```
$ repolayer callers computeMembershipDigest
# 1 definition(s) of 'computeMembershipDigest', 1 caller(s) within depth 1
@def	r	src/digest_util.ts::computeMembershipDigest	1
# caller -> target  (repo\tpath::symbol\tline\tconfidence)
r	src/auth_caller.ts::	    conf=1.00	-> src/digest_util.ts::computeMembershipDigest
```

`--depth N` walks N hops along inbound Calls (default 1). `--repo`
restricts which *definitions* are considered (callers from anywhere
still surface). Zero callers prints an explanation: extraction is name-
unique, so absence may just mean the callee name isn't unique
workspace-wide.

`--json` envelope:
`{schema_version, symbol, depth, repo_filter, definitions: [...],
callers: [{caller, target, confidence}]}`.

### `repolayer find-idl-impl <method> [--service <name>] [--no-implements] [--no-invokes] [--json]`

Given an IDL method name, returns server-side implementations
(`Implements` edges, e.g. Go handler files) and client-side invocations
(`Invokes` edges, e.g. TS API stub files), **sorted by edge
confidence**. Confidence semantics:

| Value | Meaning |
|---|---|
| 1.0 | AST-exact match (e.g. proto-declared) |
| 0.7 | AST call expression seen in a code file |
| 0.4 | Path heuristic only (e.g. `services/` directory + matching name) |

If `<method>` is ambiguous (`Get`, `List`), disambiguate with
`--service <ServiceName>`. `--no-implements` / `--no-invokes` scope the
result.

```
$ repolayer find-idl-impl GetBenefit
# IDL method: idl::MemberBenefitService.GetBenefit  (user.proto:5)
# 1 implementation(s)  (server-side, sorted by confidence desc)
impl	server_repo	services/member_service.go::	conf=0.70
# 1 invoker(s)  (client-side, sorted by confidence desc)
call	client_repo	src/api.ts::	conf=0.70
# confidence guide: 1.0=AST exact, 0.7=AST call match, 0.4=path heuristic
```

`--json` returns `{schema_version, method, implements, invokes}` with
every edge carrying its `confidence` field — agents that want to filter
out heuristic guesses should drop everything below 0.7.

### `repolayer search <query> [-k N] [--repo <name>] [--json] [--full-content]`

Hybrid BM25 + semantic search; returns the top-K chunks (default 10).
Indexing granularity is declaration (function / method / type header),
not line — so signal-to-noise is higher than `rg` for behaviour
descriptions but lower for pinpoint line lookups.

Text output prefixes a `lane=...` indicator; each row is `[i] repo \t
path:start-end \t score`. JSON output omits chunk bodies by default
(only a 200-char `preview`), since `path:line_range` is enough to
follow up with `repolayer show`. Pass `--full-content` if you really
need the body inline (mind the token cost).

`--repo <name>` restricts to a single repo — BM25 IDF is recomputed
inside that repo so common workspace terms don't drown out local
relevance.

**`lane` field semantics (affects how much to trust the result):**

| lane | Meaning | How to treat |
|---|---|---|
| `fusion` | BM25 and semantic both fired | Most trustworthy. If your query has lots of common tokens (`token`, `get`, `list`) BM25 itself gets noisy — watch for hits inside svg / asset / lockfile paths and discount them. |
| `bm25_only` | Lexical match only | Good for known keywords, bad for behaviour descriptions. Reword the query or fall back to `rg`. |
| `semantic_only` | Semantic match only (already past a strict threshold) | Useful when there's no lexical anchor. Rank tends to be weaker; cross-check before acting. |
| `substring` | LIKE fallback | Noisy; treat as candidates only. `rg` is usually better. |

### `repolayer outline <path...> [--json]`

Declaration tree (signatures + line ranges, **no function bodies**).
Saves ~80% of the tokens versus dumping the file. Pass multiple paths
to get a combined output.

### `repolayer show <file> <symbol> [<symbol>...] [--json]`

AST-bounded source for one or more symbols inside `<file>`. Symbol
names are suffix-matched, so `TakeDamage` and `Player.TakeDamage` both
work. Beats line-range `sed` because boundaries are exact.

### `repolayer digest <path> [--json]`

One-page public-API map for a module — denser than `outline` and
spans multiple files. Useful when building a mental model fast.

### `repolayer surface <path> [--json]`

Prints the published public API of a package by following re-exports
(`pub use` in Rust, `__all__` in Python, barrel `export {}` in TS,
`export` in Scala). The difference from `digest`: `surface` shows only
what's actually re-exported; `digest` shows internal public
declarations too.

### `repolayer deps <path> [--depth N] [--json]`

Forward dependency: what this file imports. `--depth` is BFS depth,
default 1.

### `repolayer reverse-deps <path> [--json]`

Reverse dependency: who imports this file, across repos.

### `repolayer cycles [<path>] [--json]`

Tarjan SCC over the import graph. Exits 1 if any cycle is found
(suitable for a CI gate).

### `repolayer find-related <file>:<line> [-k N] [--json]`

Structurally similar chunks. Paste a `<file>:<line>` straight from a
`search` result.

---

## Relationship to grep / find / Read

repolayer doesn't replace `rg` / `grep` / `find` / `Read`:

| Goal | Prefer | Why |
|---|---|---|
| Find a symbol definition (incl. IDL method / service) | `repolayer query` | Pre-built index, IDL included |
| Behaviour description / API URL / literal string | `repolayer search` | Chunk content is indexed, URLs hit cleanly |
| Skim file structure | `repolayer outline` | 5–10× less token |
| Pull one function body out | `repolayer show` | AST boundaries, no line estimation |
| Find who calls function X | `repolayer callers` | Uses precomputed Calls edges; no per-call grep |
| IDL method → server impl + client call sites | `repolayer find-idl-impl` | Confidence-ranked, knows about Implements vs Invokes |
| Find one comment / one literal import path | `rg` | Single-line literal; chunks are too coarse |
| Inspect one big file (did this change?) | `Read` + offset/limit | repolayer doesn't store full file text |

**Common anti-patterns:**

- ❌ `rg "FuncName"` to find a definition → ✅ `repolayer query`
- ❌ `rg "FuncName\\("` to find callers → ✅ `repolayer callers FuncName`
- ❌ `Read` a 1000-line `handler.go` → ✅ `outline` then `show <file> <symbol>`
- ❌ `search --full-content` pulling 10 full chunks → ✅ default preview is enough; `show` if not
- ❌ `search "..." | jq ... | grep <repo>` → ✅ `--repo <name>` (also recomputes IDF inside the repo)
- ❌ Same-named symbol in multiple repos: `query` then manual filter → ✅ `query "..." --repo <name>`
- ❌ `find http_idl rpc_idl -name "*.proto" \| xargs grep "Method"` → ✅ `repolayer query "Method"` (IDL is in the result set)
- ❌ `grep -rn "/api/v1/foo"` to find frontend call sites → ✅ `repolayer search "/api/v1/foo"`

If `query` / `search` returns zero, *then* fall back to `rg` for a
literal lookup (the symbol may live in a comment or string, or the file
isn't committed yet so `repolayer update` hasn't seen it).

---

## Error messages

| Output | Meaning |
|---|---|
| `no index found at .repolayer/index.db — run \`repolayer build\` first` | Index not built |
| `no .repolayer/ index found` | Same, from another subcommand |
| `# no matches` / `# no results` | The index has no match. Follow the stdout fallback hint |
| `# no exact match for symbol 'X'` (callers) | `callers` requires exact symbol name; try `query "X"` to see candidates |
| `# no inbound Calls edges` (callers) | Definition exists but no Calls edges point at it. Either the callee name isn't unique workspace-wide (auto-extraction is unique-only) or it's only invoked through dynamic dispatch / reflection |
| `# no IDL method found matching 'X'` (find-idl-impl) | The name doesn't match any `idlmethod` node. Try `query "X"` |
| `no callers found for <path>` (reverse-deps) | File-level reverse-deps came up empty; either nothing imports it or it isn't in an indexed repo |
| `Error: unknown repo 'xxx'. Did you mean: a, b, c, ...` | `--repo` typo. Pick from the suggestion and retry — **don't** fall back to `rg` |
| `# WARNING: N parse errors` (outline) | Parser couldn't handle parts of the file; `Read` it directly if outline is missing pieces |

`repolayer update` refreshes incrementally (only files touched by `git diff`);
`repolayer build` does a full rebuild.

---

## SQL escape hatch

`.repolayer/index.db` is a plain SQLite file. Any graph query the CLI
can't express directly can be done in SQL:

```bash
sqlite3 .repolayer/index.db "
SELECT n_caller.repo, e.kind, n_caller.path
FROM nodes n_idl
JOIN edges e ON e.to_id = n_idl.id
JOIN nodes n_caller ON n_caller.id = e.from_id
WHERE n_idl.kind = 'idlmethod' AND n_idl.symbol = 'GetDiscountList'
  AND e.kind IN ('invokes', 'implements')
ORDER BY e.confidence DESC;"
```

Schema:

```sql
nodes(id, kind, repo, path, symbol, summary, visibility, native_kind, loc_start, loc_end, deprecated)
-- kind ∈ repo / module / type / method / function / idlservice / idlmethod
edges(from_id, to_id, kind, confidence)
-- kind ∈ contains / imports / calls / implements / invokes / defines / extends
-- confidence: 1.0 = AST-derived; < 1.0 = heuristic (path pattern, ambiguous name)
```

`.repolayer/{outline,deps,search}.db` each have their own schema; most
graph queries only need `index.db`.
