# 第 49 轮（R49）证据索引

- 日期：2026-10-25（Asia/Shanghai）
- 子单：`JJC-20261025-001`（R49-01 冻结、R49-05 文档与 i18n 收口、R49-06 后续）
- **当前门禁：`current_gate=PARTIAL`**。本索引只登记本轮真实产物；最终全量门禁由队长跑完后另记，不沿用历史批次的固定结论。
- 入库约束：`docs/testing/monster-native/player-resource-freeze-before.json`（44,844,165 B）与 `player-resource-freeze.json`（45,219,533 B）超过 1 MiB，**只登记路径/字节/SHA256，不入库**；路径与哈希见 `wip-freeze.json` 与 `../monster-native/freeze.json`。

## 1. 冻结与审计

| 文件 | 内容 |
|---|---|
| [`wip-freeze.json`](wip-freeze.json) | R49-01 WIP 冻结：33 条状态（19 tracked modified + 14 untracked entries → 90 个展开文件）、**109 个 WIP 文件冻结**、4 个证据目录、`commit_candidates` / `must_exclude`、两个大 JSON 路径与哈希 |
| [`manifest.json`](manifest.json) | R49-01 冻结清单与证据哈希；本阶段（R49-05）在保留初始冻结的前提下**追加**文档阶段记录，见 `r49_05_documentation_stage` |
| [`round49-xingbu-wip-audit.md`](round49-xingbu-wip-audit.md) | 刑部 WIP 审计：证据链批次/二进制/源文件哈希错配、R47 冻结目录被刷新、pack 数字过期、README 描述过期等 BLOCK 项 |
| [`audit-probe-results.json`](audit-probe-results.json) | 审计只读探针原始结果 |
| [`audit-detail-results.json`](audit-detail-results.json) | 审计细项结果 |
| [`audit-report-validation.json`](audit-report-validation.json) | 审计报告校验元数据 |
| [`code-review-report-round49.html`](code-review-report-round49.html) | 本轮代码审查报告（HTML） |

冻结事实（与 R49-01 一致）：

- HEAD `f0be3645a96776f876d033480f6fac4a1769214d`；`git status` 冻结时 33 条（19 modified + 14 untracked）。
- `manifest.json` 任务 `sub-aa73c714`；本子单 `sub-fd937482` 只追加不重写。
- 两个大 JSON 的 SHA256：before `70845aefd4dd689a…`、after `c7d896e7f5eee470…`（全值见 `../monster-native/freeze.json`）。

## 2. 四项文档检查（原始退出码）

逐条命令、时间与输出见 [`i18n.log`](i18n.log)。摘要：

| # | 命令 | 退出码 | 结果 |
|---|---|---|---|
| 1 | `py -3 tools/i18n/scan_hardcoded.py --player-visible --strict --fail-above 70` | 0 | 66 处（上限 70，通过） |
| 2 | `py -3 tools/i18n/scan_hardcoded.py --player-visible --no-test --strict` | 0 | **15 处**（文档口径 0 未达：market 5 + monster 9 + server 1） |
| 3 | `py -3 tools/docs/check_links.py` | 0 | **670 条相对链接**、2 篇 README 文档引用、市场包编号全部一致 |
| 4 | `git diff --check` / `git status --porcelain` | 0 / 0 | 无空白错误；状态条目数见 `i18n.log` |

> 检查 3 首次在新建本 README 与 `i18n.log` 之前运行时退出码为 1（报这两个文件断链），文件落盘后复跑通过；首跑失败输出保留在 `target/r49/r49-05-check3-links-first-fail.log`，摘要记录在 [`i18n.log`](i18n.log)。

两条棘轮的真实计数：`--strict`（含测试夹具，排除 Display/assert）**66**，早期基线 69；`--no-test --strict` **15**，历史文档口径 0 至今未达。11 个非 strict 命中（不分区）为 269，加 `--no-test` 为 26，仅作参考、不是门禁口径。

## 3. 分领域证据

| 目录 | 状态 | 说明 |
|---|---|---|
| [`../monster-native/`](../monster-native/README.md) | 🟡 PARTIAL | 原生怪物帧表：52 PNG / 38 unique / 224 CSV 行；**12 个 pack / 29,762 条目 / 183,611,378 B**；`freeze.json` 记录采样批次 `1792847715360`，raw-manifest 与磁盘二进制较新，差异见其 §8 |
| [`../monster-motion/`](../monster-motion/README.md) | 🟡 PARTIAL | 走/跑插值 4 组 PNG 证据；未做 Push/Jump/Dash、坐骑、窗口边界 |
| [`../native-monster/`](../native-monster/README.md) | 🟡 PARTIAL | 118 个 v3 used 库导入；4 个 v2 无 frameSeek；全库 >800 appearance 缺 `.Lib` |
| [`../attack-variants/`](../attack-variants/README.md) | ✅ R47 冻结 / 🟡 R49 | R47 批次 `1792807006205` 冻结产物只读（README 本轮修订）；R49 起 live 测试只写 `target/attack-variants/`（`MIR_ATTACK_EVIDENCE_DIR` 可覆盖、`#[ignore]` 不进全量门禁） |

## 4. 收口边界（不夸大）

- **已收口（限 118 个 used 怪物库、目标客户端路径）**：怪物原生帧表 + 走/跑插值。
- **未完成 / PARTIAL**：缺源 >800 与 4 个 v2 拒绝；Blend 117 / Reverse / Effect / Sorting；坐骑、攻击变招、Push-Jump-Dash；四类窗口边界/FPS 未实测。
- **独立商城未完成，移至 R50**（前置条件见 [`../../world/gameshop.md`](../../world/gameshop.md) §12），不计入 R49。
- **i18n**：`--no-test --strict` 仍有 15 处（3 个文件），不得写成 0；新增玩家可见文字必须走 TextKey/Label。
- **Attack 证据**：R47 目录保留原样；PNG 位级非确定（两次运行 2/25 帧像素差），审计以 manifest 语义（索引/图层/选帧摘要 `d7a645df075396ac`）为准。
- **门禁**：`current_gate=PARTIAL`，待队长最终全量门禁；不写“12/12”固定值。

## 5. 相关文档

- [`../../client.md`](../../client.md) §6.2/§8：怪物原生帧表、走/跑插值、坐骑与攻击变招边界。
- [`../../roadmap.md`](../../roadmap.md) §六：R49 收口状态与 R50 前项。
- [`../../i18n.md`](../../i18n.md) §7：两条棘轮的真实计数与基线解释。
- [`../../../README.md`](../../../README.md)：当前状态表第 49 轮行与文档索引。
