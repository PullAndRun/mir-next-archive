# 第 55 轮（R55）证据索引

**状态：🟡 进行中。** 本索引只登记已落盘的交付与证据路径，**不预写 G2/G3 判定**（判定由刑部 T 签发、尚书验收、门下确认）；不宣称本轮已完整收口。

- 主单：`JJC-20261025-005`；HEAD=`67c618c`（`main` = `origin/main`）；仓库 `C:\Users\diana\Documents\github\mir-next`。
- 依据件：[R55 方案](../../process/round55-plan.md)、[尚书派发](../../process/round55-shangshu-dispatch.md)、[门下评审](../../process/round55-menxia-review.md)。
- 本轮为文档勘误 + 测试面补证轮：不改生产协议/DB/CI 契约；A 单未 commit/push。

## 1. 单号与状态列

| 子单 | 负责 | 写范围 | 状态（截至本索引落盘） | 证据路径 |
| --- | --- | --- | --- | --- |
| A | 礼部（sol-relay/gpt-6.1-sol） | `docs/process/round54-plan.md`（仅 :44/:103 + 同行勘误注）、本文件、`docs/roadmap.md` 一行基线脚注 | 🟡 已落盘，待尚书验收/刑部抽核 | `target/round55/docs/attempt-1/` |
| B | 兵部（deepseek-flash 编码） | 仅 `crates/mir-server/tests/maps.rs` | 🟡 已落盘且证据齐，待 T 抽核（G2 未签发） | `target/round55/maps/attempt-1/` |
| C | 兵部（deepseek-flash 编码） | `crates/mir-server/src/npc.rs` + 新建回收测试 | 🟡 已落盘且证据齐，待 T 抽核（G3 未签发） | `target/round55/npc/attempt-1/` |
| T | 刑部（只读终审） | 本轮审计件/清单 | ⏳ 未启动（依赖 A/B/C 完整结果 + Lead 唯一全量门禁） | `target/round55/audit/` |

## 2. G1 文档勘误（A）

- `docs/process/round54-plan.md` `:44` 改为“对**所列每个**发现用例的目标**逐个**运行”，`:103` 改为五路由名 `aoi_acceptance_{enter_snapshot,leave_cleanup,change_map_sets,recycle_all_kinds,broadcast_identity_order}` + `B`。
- 两处均以**同行** `> 原文（R54）：…` 前缀保留旧文，文件位置不变；`docs/roadmap.md` 仅追加一行 maps 基线脚注。
- 验收原始输出（`target/round55/docs/attempt-1/`，全部原样执行）：
  - `01-check-links.log`：`py -3 tools/docs/check_links.py` → `[OK] 1701 条相对链接、2 篇 README 文档引用、市场包编号全部一致`，exit 0（日志为 GBK 原始字节，UTF-8 转写见 `00-summary.md`）。
  - `02-git-diff-check.log`：`git diff --check` → exit 0（仅 LF→CRLF 提示）。
  - `03-grep-raw.log`：`grep -n "对发现用例的目标运行\|A1/A2/A4/A5/A6/B" docs/process/round54-plan.md` → **1 行**（`:44` 的同行勘误注）。
  - `04-grep-no-original.log`：同一 grep 再加 `| grep -v 原文` → **0 命中**（grep 空结果 exit 1，即无非勘误注残留）。

## 3. 本轮基线（maps）

- `cargo test -p mir-server --test maps` 共 **20 例**：**16 例有效**、**4 例需 `MIR2_DEV_DB_PATH`**（`real_database_*`，缺省时提前 return 跳过，不计 PASS）。
- 该基线只登记现状，不构成 B 单/G2 判定；带 env 真跑与不带 env 全量的原始读数以 B 单证据为准。

## 4. PARTIAL 承接

- AOI-5 整体 PARTIAL，仅子项“无回收原语”CLOSED；生产零调用者，接线待 R56。
- R54 AOI 五路由 4 PASS / 1 PARTIAL（回收路由 NPC 子项无生产回收入口）不因本轮文档勘误改变结论，见 [R54 证据索引](../round54/README.md)。
- maps 4 例缺 DB 时的早退/跳过继续按 PARTIAL 承接，本轮文档单不关闭该缺口。
- G2/G3 判定、门禁 12 项结论以 Lead 原件与 T 审计件为准，本索引不代签。

## 5. 不做清单

- **D（125 真生产者/原生 dash 帧）顺延 R56**。
- 不改 `crates/`、不跑 `cargo`/CI、不 commit/push（A 单口径）。
- 不改 `DB_CANDIDATES`、不下载/改 DB 素材、不手造 NPC 生命周期、不改 CI 契约、不新增工具。
- 不把 maps 4 例 skip 写成 PASS，不把 AOI-5 记为已关闭，不宣称本轮已完整收口，不预写 G2/G3 判定。

## 6. 证据文件说明

- `target/round55/docs/attempt-1/00-summary.md`：HEAD、改动文件、四条验收命令原始输出转写与 exit。
- `target/round55/docs/attempt-1/files.json`：每个改动文件的 path/bytes/sha256。
