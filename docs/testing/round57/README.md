# 第 57 轮（R57）文档与证据索引

**本轮范围**（主单 `JJC-20261026-004`，文档批 B；HEAD `fe86aaae`）：G1 FireBounce 服务端生产者、G2 包序/协议不回归由 A 批在途实现；本索引只登记 B 批文档承接（G3）与已落盘证据。

**口径**：状态列只允许「已落盘」「待 T 抽核」「代码批在途」；本轮不预写门禁通过类结论、不宣称整轮验收完成。**客户端 125 渲染 PARTIAL、125 全链未交付**；判定权归门禁后的 T 与 Lead。

## 1. B 批交付（7 处，纯文档）

| # | 文件 | 改动 | 状态 |
|---|---|---|---|
| 1 | `docs/world/magic.md` | §9 支持矩阵新增 FireBounce 行：服务端生产者（R57）+ 客户端 125 渲染 PARTIAL | 已落盘 |
| 2 | `docs/world/aoi.md` | `:291` 生产零调用者接线时点「待 R56」→「待 R57+」 | 已落盘 |
| 3 | `docs/process/round56-plan.md` | `:45` 证据路径勘误为 `target/round56/docs/attempt-1/00-evidence-greps.log`，保留「原文（R56）」原路径 | 已落盘 |
| 4 | `README.md` | R52 行（`:48`，并同步 `:76` 同句）「125 无服务端生产者」→「R57 起服务端有生产者（FireBounce），客户端 125 渲染仍 PARTIAL」 | 已落盘 |
| 5 | `docs/roadmap.md` | 新增 §七「第 57 轮状态（在途）」；R55 maps 脚注统一为「条件 PASS（需 `MIR2_DEV_DB_PATH`）」 | 已落盘 |
| 6 | `docs/testing/round56/README.md` | `:3` 受测 HEAD 改双标注「代码基线 `e3b365f` / 门禁 HEAD `b5369d7`」 | 已落盘 |
| 7 | `docs/testing/round57/README.md` | 本索引 | 已落盘 |

## 2. 代码批与终审（在途，不预判）

| 项 | 状态 | 说明 |
|---|---|---|
| A 批（`crates/mir-server/...`） | 已落盘（提交 `025564c`） | FireBounce 生产者与 6 个测试；证据 `target/round57/firebounce/attempt-1/` |
| Lead 全量门禁 | 已运行（attempt-2，exit0 12/12） | `target/round57/gate/attempt-2.log` + `.meta.txt`；attempt-1 仅收 success 流，保留为历史 |
| T 终审 | 已复审（v2，PASS-WITH-PARTIAL） | `round57-final-audit-v2.md`、`final-manifest-v2.json`；T1 原件 `round57-final-audit.md`/`final-manifest.json` 并存；**T 产物走第三次提交（文档/审计批，post-gate），不纳入门禁候选 sha** |

## 3. B 批证据索引（`target/round57/docs/attempt-1/`）

| 路径 | 内容 |
|---|---|
| `01-check-links.log` | `py -3 tools/docs/check_links.py` 原始输出与 exit |
| `02-git-diff-check.log` | `git diff --check` 原始输出与 exit |
| `03-grep-redline.log` | 本轮 `git diff -U0` 新增行红线词扫描 + 既有历史 token 处置 |
| `04-status.md` | 7 处 before→after 与逐条 grep 证据 |
| `meta.json` | 证据目录清单（head、文件 sha256、日志 bytes/sha、exit） |

## 4. 承接与边界

- R56 承接：P2 `crates/mir-server/src/world.rs:1378` BEL 注释改派 A；P3 `aoi.md` 既有 7 处旧结论行保留（非本轮新增）；P4 门禁 meta 增候选文件 sha；P5 命名统一；P6 本索引与 `round56/README.md` 已修正。
- PARTIAL（按 R57 终审 §二/§四 L6 补全索引；证据见 `docs/testing/round57/round57-final-audit-v2.md`）：
  - 125 沿用 `World::broadcast`，不回显施法者本人（已知偏差；`crates/mir-server/src/magic.rs` 模块「简化与差异」节，终审 §二 C5/P1）。
  - 客户端 125 渲染未接（D3 PARTIAL；原生渲染与 410/570 特效亦未覆盖；`crates/mir-server/src/magic.rs` 模块「简化与差异」节）。
  - Hero 种族无实例，候选过滤以 Monster/Player 构造（`magic.rs` `fire_bounce_next_target` 注释；终审 §二 C4）。
  - `can_fly` 存在双实现登记：`crates/mir-server/src/magic.rs:4029`（通用）与 `:4047`（FireBounce 落点专用）；两处语义分别维护，本轮未合并（T2 D=PARTIAL 登记，R58 评估）。
  - 一般场景仅保证命中次数 ≤ level+2；受控场景才锁 level+2 且 125 数 = level+1（`magic.rs:2071` 与 `crates/mir-server/tests/round57_firebounce.rs`）。
  - 终局弹跳 `remaining==0` 的 RNG 抽签顺序与 Crystal 有 1 次 `MirRng` 差异（M2 未登记→本轮登记，待 R58 评估；`magic.rs:2774`）。
  - 默认 DB 缺 FireBounce 记录时的生产学习路径（以 A0 实测为准）。
  - 次级目标随机序列（集合等价、MINOR）。
- 不写未证的整轮验收表述；`docs/testing/round56/README.md` 的 R56 结论不因本索引改变。
