# R69 Lead 裁定：C 刑部 BLOCK（必改 1）与历史证据保全

- 时间：2026-10-27T12:28+08 / 04:28Z
- 裁定人：Lead（队长）
- 事由：R69-C 刑部独立核验 verdict=BLOCK，必改 1 项，**仅溯源/历史保全问题，非功能/测试缺陷**：B 在 Lead 指令下以默认工作根重跑绿基线，覆盖了 `target/round69/b/attempt-1/` 的原始 runs/timings/isolation/logs/tool-green/tool-semantic/mutations/input-errors；`attempt-1/test-results.json`（d2510029…）、`test-results-repeat.json`、`notes.md` 保留，但 a77 版本的原始四变异/隔离/日志已不可恢复，B 无备份。

## 1. 根因与责任
- 直接根因：Lead 为取得「默认模式同语义」证据，要求 B 以默认 `--work-root target/round69/b` 重跑，默认工作根写入导致 attempt-1 内部证据被覆盖。
- 责任：Lead 指令设计缺陷（未要求先冷备 evidence 根）；B 已第一时间如实披露于 notes:179–191 并在交付说明中明示，**未伪造、未隐瞒**。C 依门下条件 5（历史证据不动/首败保全）判 BLOCK 成立，纪律正确。

## 2. 处置（不粉饰、不补造）
1. 新建 `target/round69/b/attempt-3/**`：以显式独立 `--work-root` 重做最终 SHA（1f3bb169…）下四变异 R1–R4/R4b 红绿，首败原文保全、目录只增不改；作为**最终 SHA 的权威首败证据**。
2. `attempt-3/PROVENANCE.md` 如实记录：a77 attempt-1 原始红/隔离/日志被默认重跑覆盖、无备份、不可恢复；保留物清单（attempt-1 test-results d2510029…、repeat、notes；attempt-2 before-fix-test-results.json、diff）；attempt-2 为接口修复冻结证据；attempt-3 为最终证据。
3. 门下条件 5 处理：**PARTIAL 关闭**——最终 SHA 证据完整且首败保全（重做件），但 a77 历史原件不可恢复；不得声称条件 5 完整闭合，本裁定与 PROVENANCE 公开披露即为其处置记录。
4. 制度改正：此后每轮证据目录 attempt-N **只增不改**；重跑一律新 attempt 或新 `--work-root`，由 Lead 在派发中写明；CI/门禁不得写既有 attempt 证据目录。

## 3. 后续
- B 完成 attempt-3 后，Lead 指派 C-addendum（gpt）仅核「最终 SHA B 红绿证据完整 + PROVENANCE 诚实 + 条件 5 PARTIAL 披露」。
- 之后 Lead 提交工具/矩阵/测试/文档，跑契约门禁，D 文档、T 终审、尚书验收、门下确认、看板 Done、push、汇报。
