---
name: repolayer
description: |
  跨仓代码索引 CLI。用户问"X 在哪定义""X 的全链路""谁调用了 X""谁实现了这个 IDL 方法"
  "这个 service / RPC 对应哪段代码"等跨仓导航问题时，优先用它，比 grep / find / 整文件 Read
  更快、token 更省，且能识别跨仓 Imports / Calls / Implements 边。也覆盖单仓的精确符号查询、
  混合检索、outline / 函数体提取、依赖图查询。
  环境变量 $REPOLAYER_INDEX 已指向集中索引目录（如 ~/repolayer_ttec/），
  即使 cwd 下没有 .repolayer/ 也可直接调用。
---

# repolayer

预构建的多仓代码索引 CLI。工作区根目录下的 `.repolayer/` 是索引产物（4 个 SQLite
库：graph / outline / deps / search），由 `repolayer build` 全量生成、
`repolayer update` 按 git diff 增量刷新。

二进制：`which repolayer`。

**索引位置**：默认从当前 cwd 找 `.repolayer/`。如果设了环境变量 `REPOLAYER_INDEX=<dir>`，
查询类命令里**只读索引、不读源文件**的那几条（`query` / `search` / `find-related` / `view`）
会改从该目录读索引——适合"在业务仓里编辑代码、但用集中式工作区做跨仓查询"的工作流。
其余读源文件的命令（`outline` / `show` / `digest` / `surface` / `deps` / `reverse-deps` /
`cycles`）仍然以 cwd 为锚解析相对路径，需要先 `cd` 到目标仓。
写索引的命令（`build` / `update` / `init`）始终绑定 cwd，避免误写。

如果 `.repolayer/` 不存在或 `repolayer.yml` 没配置，所有查询命令会报
"no index found" 并提示先 build（或设 `$REPOLAYER_INDEX`）。

## 决策树（先读这个）

| 起点 | 用什么 |
|---|---|
| 我知道符号名（精确 / 子串），含 IDL method / service | `repolayer query "Name"` |
| 同上但只想在某个仓里找 | `repolayer query "Name" --repo <name>` |
| 我知道关键词或行为描述（不知道符号名） | `repolayer search "..."` |
| 同上但只在某个仓里找 | `repolayer search "..." --repo <name>` |
| 找一段 URL / API path / 字面字符串（如 `/api/v1/...`） | `repolayer search "/api/v1/..."` 然后再缩小 |
| 我有一个文件，想看它的结构 | `repolayer outline <file>` |
| 我有一个文件 + 符号，想看函数体 | `repolayer show <file> <symbol>` |
| 我想看一个目录 / 包对外暴露什么 | `repolayer digest <dir>` 或 `repolayer surface <dir>` |
| 我想知道 X 文件 import 了什么 | `repolayer deps <file>` |
| 我想知道谁 import 了 X 文件 | `repolayer reverse-deps <file>` |
| 找跟 X:line 相似的代码块 | `repolayer find-related <file>:<line>` |
| 工作区有 import 循环吗 | `repolayer cycles` |

**⚠️ cwd 规则（v0.2 实际行为）**：

- 走 `$REPOLAYER_INDEX` 全局索引、**不挑 cwd** 的命令：`query` / `search`。
  这两条只读 `index.db` / `search.db`，路径在索引里已是绝对/带 repo 前缀，
  在任何目录下都能直接调用（包括 `~`、`/tmp`、`code/repolayer` 等）。
- **必须 cwd 在目标仓里**的命令：`outline` / `show` / `digest` / `surface` /
  `deps` / `reverse-deps` / `find-related` / `cycles`。这些要读源文件本身，
  接受的是相对路径。在错误目录下会报 `path not found` 或 `no adapter for ...`。
  正确做法：先从 `query` / `search` 结果里拿到绝对路径，再 `cd <repo-root>` 后调用。
  注意 hook 会在每条 Bash 命令后把 cwd reset 回 session 起始目录，所以**每次都要拼**
  `cd /Users/bytedance/<repo> && repolayer outline biz/handler/x.go`，不能依赖前一条 `cd`。

**追接口/IDL 全链路的标准动作**：

```
# Step 1：在任何目录下，一次拿到全链路节点
repolayer query "<MethodName>"
# 结果包含：handler 多个仓 + IDL 定义（http_idl/rpc_idl）+ TS stubs + router 入口

# Step 2：根据上一步的 repo + path，cd 进对应仓再 outline / show
cd /Users/bytedance/<be-repo> && repolayer outline biz/handler/<file>.go
cd /Users/bytedance/<be-repo> && repolayer show biz/handler/<file>.go <Method>

# Step 3（可选）：URL 反查前端调用方
repolayer search "/api/v1/<path>"
```

**不要**为了找 IDL 定义专门 `find ... -name "*.proto" | xargs grep`——`query` 已经包含
`idlmethod` / `idlservice` 节点，IDL 定义会和业务 handler 一起出现在结果里。

**默认顺序**：先 `query` / `search` 定位 → 再 `outline` 看结构 → 再 `show` 取函数体。
不要直接 `Read` 整个文件，除非已经用 outline / show 拿到上下文还不够。

**多仓时优先加 `--repo`**：在 ttec 这种 40+ 仓的工作区里，跨仓 BM25 噪声会把
真正相关的本仓结果挤出 top-K。如果你已经知道答案在哪个仓，加 `--repo`
能让 BM25 IDF 在该仓内重新计算，结果更贴合。仓名拼错时 CLI 会列 5 个最近候选，
直接抓正确名重试即可。

---

## 命令参考

### `repolayer query <text> [--repo <name>] [--json]`

在 graph 里查 declaration——含 type / method / function / **idlmethod / idlservice**。
对 symbol 名和 path **同时**做子串匹配，返回 `repo \t path::symbol \t line`，最多 20 条。

**IDL 也在结果里**：追接口全链路时，`query "GetXxx"` 一次能同时返回业务 handler、
http_idl 的 proto rpc 定义、rpc_idl 的 thrift method 定义，不用专门 grep IDL 文件。

`--repo <name>` 限制结果到指定仓（必须匹配 `repolayer.yml` 里的 name；
拼错时报错并给最近建议）。多仓里同名符号太多时优先加 `--repo` 收敛。

```
$ repolayer query "GetDiscountList"
# 20 matches for 'GetDiscountList' — repo	path::symbol	line
oec_promotion_voucher_api	handler.go::GetDiscountList	206
oec_promotion_voucher_api	biz/handler/get_discount_list.go::NewGetDiscountListHandler	63
...
```

`--json` 返回 `{schema_version, query, matches: [{repo, path, symbol, kind, line}]}`。
0 命中时退出码仍为 0，stdout 给降级建议（试 `search` 或 `rg`）。

### `repolayer search <query> [-k N] [--repo <name>] [--json] [--full-content]`

混合 BM25 + 语义检索，返回 top-K 个 chunk（默认 10）。
索引粒度是 declaration（function / method / type 头），不是逐行 —— 比 `rg` 信噪比高，
但找具体行不如 `rg`。

text 输出第一行带 `lane=...` 标识，每行 `[i] repo \t path:start-end \t score`。
JSON 默认**不返 chunk 内容**，只返 200 字符 `preview`，envelope 含 `lane` 字段。需要完整函数体时：

- 已知 `path:line_range` → `repolayer show <path> <symbol>` （AST-边界精确）
- 真要原文 chunk → 加 `--full-content`（注意 token 成本）

`--repo <name>` 限制到单个仓。多仓 query 默认会被高 IDF 跨仓项挤掉本仓内的"刚好够相关"
结果——加 `--repo` 后 BM25 在该仓内重新算 IDF，结果排名更贴合该仓自身。
拼错仓名时会报 "Did you mean ..." + 最接近的 5 个候选名，可直接抓正确名重试。

**lane 字段含义（影响结果可信度）**：

| lane | 含义 | 怎么对待 |
|---|---|---|
| `fusion` | BM25 + 语义都命中并融合 | 最可信。但 query 含很多通用词（`token`、`get`、`list`）时，BM25 也会被噪声拖累 —— 看到结果落在 svg / asset / lockfile 里就是这种情况 |
| `bm25_only` | 只有词法匹配 | 关键词搜得到、行为描述大概率搜不到。换 query 或回退 `rg` |
| `semantic_only` | 只有语义匹配（已经过严格阈值） | query 没词法锚点。结果排名偏弱，建议交叉验证 |
| `substring` | 兜底 LIKE 匹配 | 噪音很多，仅作候选。优先 `rg` |

### `repolayer outline <path...> [--json]`

打印文件 / 目录的 declaration tree（签名 + 行号，**不含函数体**）。
比 `cat` 全文省 ~80% token。多 path 时聚合输出。

### `repolayer show <file> <symbol> [<symbol>...] [--json]`

按 AST 边界提取一个或多个 symbol 的源码（含函数体）。
Symbol 用后缀匹配：`TakeDamage` 或 `Player.TakeDamage` 消歧。
比 `sed -n 'A,Bp'` 估算行号靠谱。

### `repolayer digest <path> [--json]`

模块 public API 一页摘要。比 outline 更密、跨多文件。适合快速建立 mental model。

### `repolayer surface <path> [--json]`

打印包对外的 public API（解析 Rust `pub use` / Python `__all__` /
TS barrel `export {}` / Scala `export`）。
和 `digest` 区别：surface 只看 re-export 后真正暴露的 API；digest 还包含模块内部 public 声明。

### `repolayer deps <path> [--depth N] [--json]`

forward dep：这个文件 import 了什么。`--depth` 控制 BFS 深度，默认 1。

### `repolayer reverse-deps <path> [--json]`

reverse dep：谁 import 了这个文件，跨仓。

### `repolayer cycles [<path>] [--json]`

import 循环检测（Tarjan SCC）。有循环时退出码 1（适合 CI gate）。

### `repolayer find-related <file>:<line> [-k N] [--json]`

找语义相似的 chunk。`<file>:<line>` 直接粘贴自 search 输出。

---

## 与 grep / find / Read 的关系

repolayer 不取代 `rg` / `grep` / `find` / `Read`：

| 想干的事 | 优先用 | 原因 |
|---|---|---|
| 找一个符号定义（含 IDL method / service） | `repolayer query` | 已建索引，含 IDL 节点 |
| 关键词 / 行为描述 / API URL / 字面字符串 | `repolayer search` | chunk content 被索引，URL 能命中 |
| 看文件结构 | `repolayer outline` | token 省 5-10 倍 |
| 提取一个函数体 | `repolayer show` | AST 边界，不用估行号 |
| 找一行注释 / 一个 import 路径具体文本 | `rg` | 单行字面、search 的 chunk 太粗 |
| 看一个具体大文件改没改 | `Read` + offset/limit | repolayer 不存全文 |

**典型反模式**：
- ❌ 用 `rg "FuncName"` 找符号定义 → ✅ 用 `query`
- ❌ `Read` 整个 1000 行 handler.go → ✅ 先 `outline` 再 `show <file> <symbol>`
- ❌ 用 `search --full-content` 一次拉 10 个 chunk 全文 → ✅ 默认 preview 已够定位，需要再 `show`
- ❌ `search "..."` 后用 jq 过滤到某仓 → ✅ 直接 `--repo <name>`，BM25 也会在仓内重新算 IDF
- ❌ 同名符号在多仓里 `query` 后人肉挑 → ✅ `query "..." --repo <name>` 直接收敛
- ❌ `find http_idl rpc_idl -name "*.proto" \| xargs grep "Method"` → ✅ `repolayer query "Method"` 一次拿到所有 IDL 定义 + 业务 handler
- ❌ `grep -rn "/api/v1/foo"` 在 monorepo 里找前端调用方 → ✅ `repolayer search "/api/v1/foo"`，命中后再用 grep 收敛到具体文件

如果 `query` / `search` 返回 0 条，再回退到 `rg` 字面查（可能符号在注释 / 字符串里、
或文件未提交导致 git diff 没刷过来）。

---

## 错误信息含义

| 输出 | 含义 |
|---|---|
| `no index found at .repolayer/index.db — run \`repolayer build\` first` | 索引未建 |
| `no .repolayer/ index found` | 同上（其他子命令的提示文案） |
| `# no matches / # no results` | 索引里没匹配。优先按 stdout 提示降级 |
| `no callers found for <path>` | reverse-deps 0 命中。可能是真没人调，也可能是该文件不在已索引仓里 |
| `Error: unknown repo 'xxx'. Did you mean: a, b, c, ...` | `--repo` 拼错。从建议里抓正确名直接重试，**不要**回退 `rg` |
| `# WARNING: N parse errors`（outline）| 解析失败，outline 部分缺，对应文件直接 `Read` 兜底 |

`repolayer update` 增量刷（只重做 git diff 命中文件）；`repolayer build` 全量重建。

---

## SQL escape hatch

`.repolayer/index.db` 是普通 SQLite 库，CLI 表达不出来的图查询可以直接 SQL：

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

Schema：

```sql
nodes(id, kind, repo, path, symbol, summary, visibility, native_kind, loc_start, loc_end, deprecated)
-- kind ∈ repo / module / type / method / function / idlservice / idlmethod
edges(from_id, to_id, kind, confidence)
-- kind ∈ contains / imports / calls / implements / invokes / defines / extends
-- confidence: 1.0 = ast-derived；< 1.0 = 启发式（path 模式 / 名称匹配）
```

`.repolayer/{outline,deps,search}.db` 各有独立 schema；大多数图查询用 `index.db` 就够。
