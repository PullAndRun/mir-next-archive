# R57 门下省确认（JJC-20261026-004）

- **角色**：门下省确认官 menxia（gpt-6.1-sol，只读；未改 `crates/`、未跑 `cargo`/`ci.ps1`、未 commit）
- **时间**：2026-10-26T15:04–15:08+08:00（Asia/Shanghai）
- **受审 HEAD**：`eab6986`（受测树 `38f3afe`=attempt-2 门禁树）
- **输入**：`target/round57/menxia-confirm/01-sha.log`（9 件 sha256+bytes）；CLI 原件 `02-cli.log`；检查 `03-checks.log`

## 一、结论：确认 Done（APPROVE）

**主单 `JJC-20261026-004` 准予 Done**；口径沿用 **ACCEPT-WITH-PARTIAL**：服务端 125 生产者=已实现、客户端 125 渲染=PARTIAL、125 不回显施法者=已知偏差。**不称全链、不称 R57 完整验收**；PARTIAL 8 条不闭合、整体转 R58。

## 二、必核 1–6

1. **M1–M5 闭环**：①候选种族按 C6 修正——`crates/mir-server/tests/round57_firebounce.rs:717` 测试名 `..._chain_accepts_player_hero_candidates`，`ignores_players` 全文件 0 命中，`magic.rs` `fire_bounce_next_target` 同时收 `monsters_on_cell` 与 `aoi_ids_in_cell(..., AoiKind::Player)`；②命令全路径——门禁 `attempt-2.log:1235-1236` 两同名测试以 `packets::{client,server}::roundtrip::default_packets_roundtrip` 实名运行，A 件 `15-protocol-roundtrip.log` 记 `1 passed; 0 failed; 177 filtered out`，**残留** `run-checks.sh:33` 文本仍写 `...::tests::...`（与日志不一致、脚本不可复现，R58 #1，非阻断）；③客户端渲染 PARTIAL——`docs/world/magic.md:687` 与 `docs/testing/round57/README.md:42` 只写「服务端生产者 / 客户端未接」；④BEL 改派 A——`crates/mir-server/src/world.rs:1378` BEL 计数 0，`025564c` 该文件仅 1 行注释替换（+1/−1）、行数不变，`dvance_clock` 缺 `a` 为既有笔误（R58 #5）；⑤D1 全链阈值（level+2/level+1）见测试名与 `README:45`，D2 实读 `candidates.sort_unstable()` + `self.rng.next_u32() % len`，D3/D4 如上。
2. **门禁 attempt-2 原件 — 真实**：200,569 B / sha256 `c47cef85e79ce655cc78ddaf834b90a37f59cdc3b26c56501882f06548e4e41e`，与 meta `LOG_BYTES/LOG_SHA256` 全等；`^OK   `=**12**、`:3338 ==> 全部通过（12 项）`、`test result: FAILED`=0、`^failures:`=0；meta `HEAD=38f3afe`、`EXIT_CODE=0`、`06:44:02Z–06:46:15Z`、`DURATION_S=133`；13 候选在 `38f3afe` 逐文件复算 **13/13 MATCH**；`GIT_DIFF_SHA256`=空串 sha。与尚书复算一致。
3. **提交边界 — 3/10/2/6**：`025564c`=3 码（+1044/−5）、`858147b`=10 docs、`38f3afe`=2 docs、`eab6986`=6 docs（`crates/` **0**）；`38f3afe..HEAD` 仅 6 docs（+767/−5），`magic.md` 仅单行 v1→v2。派发 §六 要求门禁后改动重跑 attempt-N+1；本轮 0 代码、改动均为 T2 §七条件 1/2 要求的审计件与引用，属「第三次文档提交」设计位，**按追认处理**：以 HEAD 重跑 `check_links`（[OK] 1706 条）与 `git diff --check`（exit0）替代 attempt-3（其余 11 项不读 docs）。
4. **T2 唯一必改 — 闭合且引用到位**：`docs/testing/round57/README.md:44` 已登记 `can_fly` 双实现（`magic.rs:4029`/`:4047`）；`:40` 与 `magic.md:687` 引用均改指 `round57-final-audit-v2.md`；v1/v2 与两份 manifest 均入 `eab6986`。
5. **表述红线与 PARTIAL 台账**：全仓 `125 已全链|125 全链|R57 完整` 17 命中全为否定/禁令（「不得称/不写/未交付/未证」），**0 正面表述**；`README:40-48` PARTIAL 实测 **8 条**，无一条被写成闭环。
6. **R58 承接 12 项可追溯**：1→T2 §六 M3；2→L3；3→L2；4→L5；5→L7；6→`README:44`；7→`:46`/`magic.rs:2774`；8→`:47`；9→`:48`；10→`:41`；11→`:43`；12→`:42`；另 `round57-shangshu-acceptance.md:49` 的追认路径由本件执行。

## 三、R58 承接确认

- 12 项按尚书验收 §四 原序、原优先级承接；本件不改口径、不新增承项。
- 追认仅限 docs；R58 任何代码/文档改动仍须新门禁 attempt-N 绑定。

## 四、CLI 回执（全部 `--as menxia`，原件 `02-cli.log`）

| # | 命令 | exit | 回执 |
|---|---|---|---|
| a | `memo … menxia "<确认结论>" --as menxia` | 0 | `📝 JJC-20261026-004 任务记忆：menxia` |
| b | `state … Done --as menxia` | 0 | `⏳ 高风险操作 Review→Done，已进入 PendingConfirm，需 门下省 执行 confirm approve` |
| c | `show … --as menxia` | 0 | 状态 **PendingConfirm**；`⏳ 待确认：menxia 申请 Review → Done` |
| d | `confirm … approve "<理由>" --as menxia` | 0 | `✅ JJC-20261026-004 confirm approve → Done` |
| e | `show … --as menxia` | 0 | 状态 **Done**；流转含 `PendingConfirm → Done ✅ 批准` |
| f | `validate --as menxia` | 0 | `✅ 看板校验通过：154 条任务符合 schema v3.1` |

（CLI 对长文有清洗：`show` 的当前摘要显示为 `M1–M5–D4`、尾部截断；memo/confirm 完整原文见 `02-cli.log`，不影响状态。）

## 五、边界声明

- 只读确认：未改 `crates/`、门禁原件、看板卡片内容；仅新增本文件与 `target/round57/menxia-confirm/**`（3 件）。
- 未跑 `cargo`/`ci.ps1`；12/12 以 attempt-2 原件 + meta 复算为证，未重跑门禁。
- 签发仅表示「本轮交付 Done」，不等于「R57 完整验收」；不追认 Crystal「无效目标仍扣蓝」差异、126/149、客户端渲染。
