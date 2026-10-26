# R56 刑部独立终审（T）｜JJC-20261026-003-sub-280260aa

- **受审 HEAD**：`b5369d799a46143b823580ce9c355ea268207fd0`（= `origin/main`）
- **代码基线**：`e3b365f`（R55 收口）
- **审计角色**：xingbu / T（gpt-6.1-sol，只读，不兼职作者）
- **门禁**：`target/round56/gate/attempt-1.log`（194,543B，sha256 `0ad14a35…ef8`，exit 0，12/12）
- **结论：PASS-WITH-PARTIAL**（0 项必改；6 项 PARTIAL/非阻断登记；不宣称 AOI-5 关闭、不宣称 R56 完整）

---

## A. 门禁原件真实性 —— **PASS**

| 检查 | 结果 | 证据 |
|---|---|---|
| log bytes | 194,543 = meta.bytes | `wc -c target/round56/gate/attempt-1.log`；`attempt-1.meta.txt` 字段 |
| log sha256 | `0ad14a35694be86724b7621d4606436b54e8ba4925b50f0614eeb19c4dfe0ef8` = meta.sha256 | `sha256sum` 复算 |
| meta head | `b5369d799a…207fd0` == `git rev-parse HEAD` == `origin/main` | meta :2；`git rev-parse HEAD origin/main` |
| exit | 0 | meta `exit=0` |
| 起止 | start `2026-10-26T05:05:11Z` / end `05:07:29Z`（本地 13:05:11–13:07:29，log mtime 13:07 一致） | meta :5-6；`ls -la` |
| 12 项 OK | 12 条 `OK   名称` 汇总行 + 末行 `==> 全部通过（12 项）`（GB18030 解码，log L3315-3327） | PowerShell `GetEncoding(936)` |
| 无 FAIL/伪绿 | `FAIL`、`attempt-2`、`首红`、`REJECT` 计数均 0 | PowerShell 正则计数 |
| R56 独有测试名 | `ground_item_expiry_strict_ms_boundary` ×1、`aoi_acceptance_recycle_all_kinds` ×2；`aoi.rs` 在门禁内两次 `running 14 tests` / `14 passed`（L1673/1689、L2920/2936） | 排除旧轮日志冒充 |
| attempt 数 | `target/round56/gate/` 仅 attempt-1；无 attempt-2 | `ls -la` |
| 首红申报 | A 自证首红（clippy 101 doc_markdown、07/08/09 误跑批次）已在 `00-summary.md` §5 与 `superseded/` 如实申报，非隐瞒 | `00-summary.md` |

## B. 提交树绑定 —— **PASS（6/6 MATCH）**

`git show <commit>:<path> | sha256sum` 逐条复算，与 `target/round56/commit-verify/01-tree-hashes.txt`（612B，sha256 `37451f3f…9cf51`）全等：

| 文件 | bytes | sha256 | MATCH |
|---|---|---|---|
| `crates/mir-server/tests/aoi.rs`（d7a23e3） | 58,553 | `605d1f9eabaddaaf76f6cc1d86950bc92de5920c1fedf40e0036f0535a0eb7e1` | ✅ |
| `docs/world/aoi.md` | 25,511 | `653c8f00f0de1d227077f3fe7273d42af9804a65efafab66c372026bb0989da9` | ✅ |
| `docs/testing/round56/README.md` | 3,201 | `10b36817fb299f08f82ebc4044b74c681f08b072ac28195a239ab72babbd117d` | ✅ |
| `docs/process/round56-plan.md` | 14,446 | `a4473c9f2994b65dbaabd569e7251328b9d71fc9d3483dc2cbe73ec5637d769d` | ✅ |
| `docs/process/round56-menxia-review.md` | 9,919 | `611868cd088382493812a315182599c56bc803f2becfcd814118a30be65c2f21` | ✅ |
| `docs/process/round56-shangshu-dispatch.md` | 8,929 | `8c581ce126ec807718996c3d10d6c5404256330b6521e225a17487f4d3e86ec6` | ✅ |

- **M6 提交一范围**：`git show --stat d7a23e3` = 仅 `crates/mir-server/tests/aoi.rs` +139，无其他文件；`git diff e3b365f..d7a23e3 --stat` 同。
- **提交二**：`git show --numstat b5369d7` = 5 文档 +336 行（aoi.md +32、README +46、plan +109、review +69、dispatch +80），与派发件一致。
- **无重叠**：`git diff --name-only e3b365f..d7a23e3` 与 `d7a23e3..b5369d7` 文件集不相交。
- **工作树**：`git status --porcelain` 仅 2 个未跟踪历史大冻结 JSON（`docs/testing/monster-native/player-resource-freeze{,-before}.json`，44.8MB/45.2MB，未入库，>1MiB 只登记不入库符合 R7）。

## C. M1/M2 实现 —— **PASS**

`git show d7a23e3:crates/mir-server/tests/aoi.rs`（= 工作树，sha 全等）：

- **M1 冻结**：`world.advance_clock(0);` 位于 **:1128**（`enter`/`drain` 之后、两次 `spawn_ground_item`(:1132/:1142) 之前），:1112-1127 有 `[M1→A]` 注释与“不冻结必红”自证说明。
- **M1 显式断言**：**:1154-1157** `assert!(normal_expire_time == coin_expire_time, "both ground items must share the same expire_time when the clock is frozen before spawning")`（`:1155` 为断言行）；自证 06 log 在 `aoi.rs:1156` panic 即此断言，说明可失败。
- **M2 新例**：**:1336** `fn ground_item_expiry_strict_ms_boundary()`（doc 注释 :1324-1334），含：
  - 两次 spawn 间 `advance_clock(1)`，断言 `expire_b - expire_a == 1`（:1371-1374）；
  - 三时刻 tick：a 边界两者都在且**无 `ObjectRemove`**（:1376-1390）、a+1 仅删 a（:1392-1407）、b+1 删 b（:1409-1420）；
  - **二次 tick 无重复移除**（:1422-1427）；
  - 只读 `expire_time`/`ground_item`/包，无新生产 API、无 `#[allow]`（全文搜索 0 命中）、断言全英文。
- **未碰 src/**：`git diff e3b365f..b5369d7 -- crates/mir-server/src` 为空；`git diff e3b365f..d7a23e3 --stat` 仅 tests/aoi.rs。

## D. M3 证据形态 —— **PASS**

- **无双位置过滤器**：全目录 grep `cargo test … <A> <B>` = 0 命中；`10-final-list.log` 命令为 `-- --list`。
- **三件套原始 exit**：`10-final-list.log` = `14 tests, 0 benchmarks` + `EXIT=0`（sha `cd4566ad…`）；`11-final-single-recycle.log` = `1 passed; 13 filtered out` + `EXIT=0`（`75940a0a…`）；`12-final-single-boundary.log` = `1 passed` + `EXIT=0`（`2626795a…`）；`13-final-whole-file.log` = `14 passed; 0 failed` + `EXIT=0`（`eb9cfd43…`）。
- **100 次自证**（逐次 `$LASTEXITCODE`，非管道吞 exit）：
  - `08-nofreeze-sleep-100-runs.log`：`RUNS=100 FAILS=100 PASSES=0`（sha `c9b87aa7…`）→ 旧/不冻结树必红；
  - `09-fixed-clean-100-runs.log`：`RUNS=100 FAILS=0 PASSES=100`（sha `f4133a5c…`）；
  - `07-nofreeze-100-runs.log`：`RUNS=100 FAILS=0 PASSES=100`（sha `12323b9f…`）**如实记录负结果**：无 sleep 时两次 spawn 跨毫秒概率极低，机器侥幸全绿——`00-summary.md` §4 明确写为“负结果”并据此说明 M1 显式断言不可省，**非假绿**。
- **旧树红点**：`04-oldtree-sleep-red.log` panic 在 `aoi.rs:1203` `the expired coin pile must be removed by the World tick`，与 A 自述一致（只认原件，原件在）。
- **变体 sha 齐全**：`23-variant-sha256.log` + `variant-*.rs.sha256` + `fixed-aoi.rs.bak`（sha `a93f150a…` 记录 `605d1f9e…`）。

## E. M4 文档红线 —— **PASS（含 1 项口径非阻断）**

- **§8.9 五条证据逐条在**：`docs/world/aoi.md:263-294`（:263 标题；①容器原语/②Crystal 运行期不调用/③无地图卸载生命周期/④淘汰路径排除/⑤动态刷出仅测试），每条带 Rust 行号 + Crystal 锚点（`Map.cs:2375-2383`、`Envir.cs:4367-4371`、`HumanObject` 等）；结论行 :291 明写 **AOI-5 整体维持 PARTIAL**、`despawn_npc` 生产 0 调用。
- **:123-128 零改动**：`git diff e3b365f..b5369d7 -- docs/world/aoi.md` hunk 头 `@@ -260,6 +260,38 @@`，新增全部落在 :263-294；:120-132 新旧逐行比对完全一致。
- **README**：46 行（≤60）；状态列仅「已落盘」「待 T 抽核」；`grep -nE "PASS|已关闭|已接线"` 对 README = **0**（exit 1）、对 aoi.md §263-294 = **0**（`04-grep-redline.log`，sha `8f6cde04…`）；含 **「roadmap 脚注差异留待 R57 统一」**（:40-41）；:46 明写“不宣称 AOI 全量验收或本轮完成”。
- **非阻断偏差 P3**：menxia M4 / dispatch §2-B 的字面命令是“对两个**整文件** grep = 0”，实际 `docs/world/aoi.md` 整文件仍有 **7 处既有** PASS 字样（:349/:353-355/:357/:359/:365，均属 R54/R55 状态表，不在本轮 diff 内；B 的 `04-grep-redline.log` 与 README 已显式限定“只核新增行范围”，未冒充整文件 0）。R56 新增行 0 命中，符合任务书 E 项口径；建议 R57 清理既有行或修订该字面口径。

## F. M5 绑定 —— **PASS**

- 门禁 meta `head` == 受审 HEAD == `origin/main`（三方一致，A 项已列）。
- 候选 6 文件 sha 均由 `git show <commit>:<path>` **从提交对象直接重算**并 6/6 MATCH（B 项表）；`22-sha-bind.log`（sha `ec958e4e…`）另绑定工作树最终件 `605d1f9e…`，与提交一致。
- 全部判定只用原件（`target/round56/aoi/attempt-1/`、`gate/`、`commit-verify/`、`docs/attempt-1/`）与 git 对象，不采信 A/B 自述；自述与原件冲突处无（A 自述 07 负结果、首红均能在原件找到）。
- **非阻断 P4**：`attempt-1.meta.txt` 只有 head/log bytes/sha，**没有** menxia M5 字面要求的“候选文件 sha”字段；本审计改用提交对象哈希完成同强度绑定，建议 R57 门禁 meta 增列文件 sha。

## G. 假绿与卫生 —— **PASS**

- **无 deps 伪造**：两提交无 `Cargo.toml`/`Cargo.lock` 改动（diff 文件集外）；门禁 log 无依赖替换痕迹。
- **i18n 复跑**：`py -3 tools/i18n/scan_hardcoded.py --player-visible --no-test --strict --fail-above 0` → `合计 0 处硬编码中文（33 个文件已扫描）`、`[OK] 命中 0 处`、**exit 0**（与门禁 L3242 一致）。
- **>1MiB**：两提交新增文件最大 109 行文档；工作树 2 个 44.8/45.2MB 冻结 JSON 未跟踪、未入库。
- **target//.env 入库**：`git ls-files target .env` 空。
- **禁法**：无 `#[allow]`、无手删容器、无 cargo 并发、100 次循环逐次取原始 exit。

## H. PARTIAL 诚实性 —— **PASS**

- G2 裁决为“**无真实接线点**”，`docs/world/aoi.md:291` 与 README:42-43 均维持 **AOI-5 整体 PARTIAL**，未写 CLOSED、未写“已接线”，未宣称 AOI 全量关闭或 R56 完整（`grep "R56 完整\|AOI-5 已关闭\|AOI 全量验收仍不可关闭"` 无正面误用；aoi.md:361 为“仍不可关闭”的否定表述）。
- **M7** `crates/mir-server/src/world.rs:1378` 注释 `advance_clock` 首字母 0x07(BEL) 经 `sed | cat -v` 实测确认（`^Gdvance_clock`）；README:44-45 已登记 R57；本轮禁改 src/ 已遵守。

---

## Findings

### 必改（阻断）：0 条

A–H 全部满足任务书口径，无假绿、无未申报首红、无越界写、无 >1MiB 入库；门禁 12/12 原件可复算。

### 非阻断（建议 R57 或后续文档提交处理）

1. **P3 M4 整文件口径偏差**：`docs/world/aoi.md` 整文件 7 处既有 PASS（R54/R55 状态表），本轮新增节 0；B 已披露范围。建议 R57 清理或修订 M4 字面命令。
2. **P4 门禁 meta 无候选文件 sha**：M5 字面要求不可满足；本审计以提交对象绑定替代（6/6 MATCH）。建议门禁 meta 增列 6 文件 sha。
3. **P5 命名不一致**：menxia:32 / dispatch:36 写 `meta.json`，实际 `attempt-1.meta.txt`；README 索引写 `round56-final-manifest.json`，T 产物按任务书为 `final-manifest.json`。建议后续文档提交统一或加别名。
4. **P6 README 受测 HEAD**：:5 写 `e3b365f`（代码基线），门禁 HEAD 为 `b5369d7`；建议改为“代码基线 e3b365f / 门禁 HEAD b5369d7”。
5. **07 负结果可被误读**：`RUNS=100 FAILS=0` 在无 sleep 时是“未复现”而非“已修复”；`00-summary.md` 已解释，建议今后同类自证在文件名加 `negative` 前缀。

## PARTIAL 清单

| # | 项 | 状态 |
|---|---|---|
| P1 | AOI-5 整体（NPC 回收原语生产接线） | **PARTIAL**，无真实接线点；原语保留待 R57+ |
| P2 | `world.rs:1378` 0x07(BEL) 注释 | 登记 **R57** |
| P3 | M4 整文件 grep 口径（7 处既有 PASS） | 非阻断，已披露；R57 清理/修订 |
| P4 | 门禁 meta 缺文件 sha 字段 | 非阻断，提交对象绑定替代；R57 增列 |
| P5 | meta/产物命名不一致 | 非阻断，后续文档提交统一 |
| P6 | README 受测 HEAD 标注 | 非阻断，R57 改写 |

## 不做清单（本轮明确顺延，非静默丢失）

- **D=125（`S.ObjectProjectile`）真生产者**：Crystal 唯一生产者是 `FireBounce` 二次弹跳（`HumanObject.cs:5818-5827`），本作无 bounce 机制，顺延 R57。
- **原生 dash 帧/视觉/音频/时序**：顺延 R57。
- **真 socket / 真实客户端窗口**：无客户端 harness，顺延。
- **NPC 生产接线**：无真实接线点，不发明生命周期、不为关闭而造需求。
- **`docs/roadmap.md` 修改**：脚注差异 R57 统一。
- **`world.rs:1378` BEL 修复**：R57。
- **AOI-5 整体关闭 / R56 完整表述**：禁止，维持 PARTIAL。

## 提交建议

1. 本三件产物（`docs/testing/round56/final-manifest.json`、`docs/testing/round56/round56-final-audit.md`、`target/round56/audit/summary.log`）由 Lead 作为**独立文档提交**入库（不要 amend `b5369d7`，保持候选提交树不变）；`target/` 路径按仓库惯例不入库。
2. 提交前建议顺带修正 P5/P6（README 索引名与 HEAD 标注）与 P3 字面口径，或在 R57 统一；不阻塞本轮收口。
3. 保持 `b5369d7` 为受审 HEAD；若需再次门禁，必须新 attempt（attempt-2）并重算 meta/log sha，不得覆盖 attempt-1。
4. T 只读边界已遵守：未改候选/文档（除本三件产物）、未跑 cargo 全量、未跑 ci.ps1、未做任何 git 写操作。
