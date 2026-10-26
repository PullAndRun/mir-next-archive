# R55 尚书省验收报告（验收官 / gpt-6.1-sol）

> 验收对象：第55轮（主单 `JJC-20261025-005`）｜基线 HEAD=`d9ea92e1cab2e830389a82fe1f83faf4e846102e`=`origin/main`
> 上游：刑部终审 `docs/testing/round55/round55-final-audit.md`（145 行，PASS-WITH-PARTIAL / 0 阻塞 / 必改 0）；门下 `docs/process/round55-menxia-review.md`（PASS-WITH-CHANGES，D1–D4 / M1–M5）；尚书派发 `docs/process/round55-shangshu-dispatch.md`（90 行）
> 验收时刻 2026-10-26T12:2x+08:00（Asia/Shanghai）｜只读复核 + 看板 CLI；**未跑 cargo / `ci.ps1`、未 commit / push、未改 `crates/`**

## ① 结论

**ACCEPT-WITH-PARTIAL（第55轮验收通过）** —— 上限即刑部口径，不扩张：**0 阻塞、必改 0 条**。

- **G1 PASS**：文档勘误既有更正、亦有同行留痕，无预判、无副作用。
- **G2 条件 PASS**：真实数据 4/4 真跑依赖显式 `MIR2_DEV_DB_PATH`；**CI 默认（无 env）仍 20 例中 16 例有效 + 4 例 skip**。
- **G3 子项 CLOSED / AOI-5 整体 PARTIAL**：`despawn_npc` 回收原语已落且与 Crystal 四要素对齐；**生产零调用者，接线待 R56**。
- **PARTIAL 6 条**（见 ④）整体转入 R56 承接；本轮验收**不宣称**"R55 完整 / AOI 全量 / AOI-5 已关闭"。

## ② 三关判定（对照刑部）

| 门 | 刑部判定 | 尚书复核 | 验收口径 |
| --- | --- | --- | --- |
| G1 文档勘误（A） | PASS | **PASS** | `round54-plan.md:44`/`:103` 同行勘误注；grep 1 行 / 去"原文"0 命中（exit 1）；check_links 1705 条 exit 0 |
| G2 maps 真实数据（B） | 条件 PASS | **条件 PASS** | 带 env 4/4 passed·0 skip；无 env 20 passed·4 skip——**CI 默认 16 有效 / 4 skip**，不得表述为"CI 已覆盖真实数据" |
| G3 NPC 回收（C） | 子项 CLOSED / AOI-5 整体 PARTIAL | **同** | `npc.rs:811` `despawn_npc` 容器/格/广播/关系四要素对齐 `Map.cs:2375-2383`；Crystal 与本作均零生产 NPC 调用者 |

- 三关**无作者自判代签**：G1–G3 由 T（刑部）签发，尚书仅复算不改判。
- G2 边界写入交付：条件 PASS 的成立前提是显式设置 env；未设则走 `GameData::discover()` + `eprintln!` 早退，**不算覆盖**。
- G3 边界写入交付：子项 CLOSED ≠ AOI-5 关闭；`despawn_npc` 唯一调用者是 R55 新测试 `crates/mir-server/tests/round55_npc_recycle.rs`。

## ③ 证据与门禁复核（实测）

| 项 | 期望 | 实测 | 判定 |
| --- | --- | --- | --- |
| HEAD | `d9ea92e1…46102e` = origin/main | 逐字相同 | ✅ |
| 门禁 log | `target/round55/gate/attempt-1.log` | **194,592 B** | ✅ |
| log sha256 | `43e7baaddab322275293ca2963f848c8969a42bdd2cb82aa0babdde25c2bdb9b` | 逐字节相同 | ✅ |
| 门禁 exit | 0 | meta `EXIT=0`，末行"==> 全部通过（12 项）" | ✅ |
| 12 项 | 全 OK | fmt/clippy/duo_smoke/workspace test/导出检查/文档/TextKey/硬编码 70→0/运行时资源/商品图标/PG 集成 | ✅ |
| 提交树 9 文件绑定 | 9/9 MATCH | 逐文件 `git show d9ea92e:` sha 比对，**MATCH=9 / MISMATCH=0** | ✅ |
| `target/` 入库 | 0 | `git ls-tree -r HEAD \| grep -c '^target/'` = 0 | ✅ |
| 45 MB 冻结 JSON | 排除 | 仍 `??` 未跟踪（2 份） | ✅ |

- 门禁 log 含 R55 独有 `round55_npc_recycle` 与 maps 真实用例名（`real_database_has_movement_points_that_resolve_to_maps` 等），排除旧轮日志冒充。
- 提交树绑定九件：plan / dispatch / menxia / round54-plan / roadmap / `docs/testing/round55/README.md` / `tests/maps.rs` / `src/npc.rs` / `tests/round55_npc_recycle.rs`；三提交文件集互不重叠（刑部 §B 已逐条比对）。
- 门禁后审计产物（`docs/testing/round55/final-manifest.json`、`round55-final-audit.md`、`target/round55/audit/summary.log`）属只读审计件，不改代码与 CI 契约，按既有口径**不使 attempt-1 门禁失效**。

## ④ PARTIAL 6 条与 R56 承接

| # | PARTIAL | 现状 | R56 承接 |
| --- | --- | --- | --- |
| P1 | G2 条件 PASS（CI 默认 16 有效 / 4 skip） | 真实数据仅定向覆盖 | 让 CI 默认覆盖或显式声明 seam |
| P2 | AOI-5 整体 PARTIAL | 仅子项"无回收原语"CLOSED | 补 NPC 回收生产链路 |
| P3 | `despawn_npc` 无生产调用者 | 唯一调用者为新测试 | 同上，需 NPC 生命周期真生产者 |
| P4 | `aoi_acceptance_recycle_all_kinds` 既有 flaky | C 首跑 1 红；Lead 与评审均 13/13 绿 | 定位 flake，不改断言迁就 |
| P5 | R52 承接（R52-1 125 ObjectProjectile 无生产者 / R52-2 真 socket 与客户端窗口 / R52-3 原生 dash 帧） | 未动 | 原样顺延 |
| P6 | R53 十九项 → R54 关闭 6、承接 13 | 未动 | 按 `docs/testing/round54/round54-carryover.md` 承接 |

- 门下列"不做"的 **D（125 真生产者 / 原生 dash 帧）** 已显式顺延 R56（README `:42` 与派发件同文），**未静默丢失**。
- PARTIAL 计数以本表为准：**6 条**；不把 P2/P3 合并算 1 条，也不把 P5 的子项展开充数。

## ⑤ M1 口径同文与零正面表述

- 三处**同文**（逐字一致）：①派发件 `docs/process/round55-shangshu-dispatch.md:21/:60/:72`；②A 的 `docs/testing/round55/README.md:35`；③刑部 `round55-final-audit.md` §0/§E。
- 同文措辞：**「AOI-5 整体 PARTIAL，仅子项"无回收原语"CLOSED；生产零调用者，接线待 R56」**。
- 零正面表述（实测）：`grep -c "AOI-5 整体 PARTIAL" docs/testing/round55/README.md` = **1**；`grep -n "AOI-5 已关闭\|R55 完整" -r docs/testing/round55 docs/process/round55-*` 命中行全部处于"禁止 / 不得 / 不宣称 / `forbidden_phrases` / `positive_claims_absent`"**否定语境**（含本报告 §①/§⑤ 自身两行），**0 条正面断言**。
- 判据保留：任一处写"AOI-5 已关闭"即判假绿；本轮四件回填后仍为 0 正面。

## ⑥ 验收决定与提交建议

- **决定**：第55轮 **ACCEPT-WITH-PARTIAL**。门禁真实、提交树绑定成立、三关判定与刑部一致，**无阻塞、必改 0**；6 条 PARTIAL 转 R56，不掩盖、不升格。
- **提交建议（由 Lead 执行，本文不 commit）**：提交刑部 2 个 docs 产物（`docs/testing/round55/final-manifest.json`、`docs/testing/round55/round55-final-audit.md`）+ 本验收文档（`docs/process/round55-shangshu-acceptance.md`）；**排除** 2 份 45 MB 冻结 JSON（`docs/testing/monster-native/player-resource-freeze*.json`），`target/` 不入库。
- 门禁后新增文档不改代码路径，按既有口径**不使 attempt-1 失效**；若 Lead 选择重跑门禁，须另行登记新 attempt，不复用本 log。
- 看板（实测）：`memo` **exit 0** 已记；`validate` **exit 0**（144 条 schema v3.1）；`done` **被 readiness 拦截 exit 3**——"委派 `JJC-20261025-005-sub-9cd91159` 尚未完成或结果未回填，禁止收口"，故主单仍 **Doing**（非 Review），按规不绕过、不改其他单，待 Lead 以 main 回填协调单后再收口。
