---
name: repolayer
description: |
  repolayer 是一个跨仓代码索引 CLI。如果当前工作区有 .repolayer/ 目录，
  就可以用它做跨仓搜索、符号查询、文件 outline、依赖图查询、IDL 实现 / 调用方追溯等。
  比直接读文件 / 跨仓 grep 更快、token 更省、能识别 IDL ↔ 实现的跨仓边。
---

# repolayer

**预构建的多仓代码索引 CLI。** 工作区根目录下的 `.repolayer/` 是索引产物
（4 个 SQLite 库：graph / outline / deps / hybrid search），由 `repolayer build`
生成、`repolayer update` 增量刷新。

二进制位置：`which repolayer`（一般是 `/usr/local/bin/repolayer`）。

如果 `.repolayer/` 不存在，所有查询命令会报 "no .repolayer/ index found"。

---

## 命令参考

### 跨仓搜索 / 符号定位

#### `repolayer find-context "<task description>" [--budget-tokens N]`

给一段任务描述（自然语言），返回与之相关的代码 chunks，按混合 BM25 + 语义相关度排序，
连带跨仓 Imports / Invokes / Implements 边。适合任务起点不明确时。
返回 JSON，含 `match_source: "substring" | "search" | "fusion"`、`repo`、`path`、`line_range`、
跨仓 edges。

#### `repolayer search "<keyword>" [-k 10]`

混合 BM25 + 语义检索，返回 top-K 个 chunk，每个带 `repo:path:line_range` 和 score。
只索引 declaration（function / method / type / module 头），不索引每一行 —— 所以
比 rg 信噪比高，但对"找具体某行"这类任务命中率不如 rg。

#### `repolayer query "<symbol>"`

在图里精确查 declaration（function / method / type / idlmethod / idlservice）。
返回所有命中的 `repo:path::symbol` 列表。同名符号在多个仓时全部返回。

#### `repolayer callers "<symbol>"`

返回调用了该 symbol 的位置（需要 ast-grep 能解析的语言；置信度 < 1 的命中是启发式）。

#### `repolayer find-related <file:line>`

找跟某段代码语义相似的 chunks。

#### `repolayer find-idl-impl "<idl method name>"`

repolayer 独有命令。给一个 IDL 方法名（protobuf rpc / thrift function），
返回所有 `Implements` 边（疑似实现侧 handler）和 `Invokes` 边（疑似调用方），
跨仓、按 confidence 排序。

### 单文件 / 模块结构

#### `repolayer outline <path...>`

打印一个或多个文件的 declaration tree（签名 + 行号，**不含函数体**）。
比 `cat` 全文省 ~80% token。多 path 时聚合输出。

#### `repolayer show <file> <symbol>`

提取某个 symbol 的源码体（含函数体）。比 `cat <file> | sed -n 'A,Bp'` 更稳，
因为它读的是 ast 边界而非行号区间。

#### `repolayer digest <path>`

打印模块的 public API 摘要（一页纸密度），适合快速建立 mental model。

#### `repolayer surface <path>`

打印包对外 API（Rust `pub use` / Python `__all__` / TS barrel `export {}` / Scala `export`）。
跟 `digest` 的区别：`surface` 只看 re-export，`digest` 还包含模块内的 public 声明。

### 依赖图

#### `repolayer deps <path>`

这个文件 import 了什么（解析后的本地路径 + 外部包）。

#### `repolayer reverse-deps <path>`

谁 import 了这个文件（同仓 + 跨仓）。

#### `repolayer cycles`

打印当前工作区的 import 循环。

### 工作区

#### `repolayer list-repos`

列出 `repolayer.yml` 里登记的所有仓：name / 绝对路径 / 语言 / 是否 IDL 仓。

---

## 与 grep / find 的关系

repolayer 不取代 `rg` / `grep` / `find` / `Read` —— 它们各有适用场景。下面是一些
**事实层面**的等价关系，知道这些可以避免做重复劳动：

| 想干的事 | 一条 repolayer 命令通常已经够 | 而不是 |
|---|---|---|
| 找一个符号 / 函数 / 方法在哪 | `repolayer query "GetXxx"` | `find . \| xargs grep "GetXxx"` |
| 找包含某关键词的代码 | `repolayer search "<keyword>"` | `rg "<keyword>"` 全工作区 |
| 一个 IDL 方法的实现 + 所有调用方 | `repolayer find-idl-impl "<method>"` | 在每个仓单独 grep 拼装 |
| 看一个文件的结构 / 函数签名 | `repolayer outline <file>` | `cat <file>` 全文 |
| 提取某个函数的源码 | `repolayer show <file> <symbol>` | `sed -n 'A,Bp' <file>` 估算行号 |

`repolayer query` 默认已经匹配同一符号的多种命名变体（驼峰 / 蛇形 / 不同语言），
所以**不需要对一个符号反复跑多次 grep 试不同写法**。

如果 `repolayer search` / `query` 返回 0 条，再回退到 `rg` 字面找通常更靠谱
（可能是符号在评论或字符串里、未被索引为 declaration）。

---

## SQL escape hatch

`.repolayer/index.db` 是一个普通 SQLite 库，可以直接 `sqlite3` 查。
如果某种图查询用 CLI 子命令表达不出来，可以走 SQL。

```sql
-- nodes
nodes(
  id        TEXT PRIMARY KEY,   -- 16-byte hex (sha256 truncated)
  kind      TEXT,               -- repo / module / type / method / function / idlservice / idlmethod
  repo      TEXT,
  path      TEXT,
  symbol    TEXT,
  summary   TEXT,
  ...
)

-- edges
edges(
  from_id     TEXT,
  to_id       TEXT,
  kind        TEXT,              -- contains / imports / calls / implements / invokes / defines / extends
  confidence  REAL                -- 1.0 = 来自 ast；< 1 = 启发式
)
```

例：找某个 IDL 方法的所有跨仓 implements / invokes：

```bash
sqlite3 .repolayer/index.db "
SELECT n_caller.repo, e.kind, n_caller.path
FROM nodes n_idl
JOIN edges e ON e.to_id = n_idl.id
JOIN nodes n_caller ON n_caller.id = e.from_id
WHERE n_idl.kind='idlmethod' AND n_idl.symbol = 'GetDiscountList'
  AND e.kind IN ('invokes', 'implements')
ORDER BY e.confidence DESC, n_caller.repo;"
```

`.repolayer/{outline,deps,search}.db` 各有独立 schema；大多数图查询走 `index.db` 就够。

---

## 错误信息含义

| 输出 | 含义 |
|---|---|
| `no .repolayer/ index found` | 索引未建。需要 `repolayer build` |
| `no changes detected`（update 时）| git diff 没看到变化 |
| `... is not a git repo, skipping incremental` | 该仓不是 git 仓库，update 跳过；要刷新需用 `build` |
| 命令成功但返回 0 条结果 | 索引里没有匹配。可能：拼错、关键词太具体、文件未提交（git diff 还没看到）、索引过期 |
| confidence < 1 的边 | 启发式匹配（call expression 模式 / 路径启发式），不是 ast-grounded |

`repolayer update` 增量刷索引（只重做 git diff 命中的文件）；`repolayer build` 全量重建。
