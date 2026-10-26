# R61 门下省确认

- 受审 HEAD：`7baa3a8121cfdf8769edd8edb98aac7fba150a0f`（=origin/main，提交③ 审计产物之后）。
- 轮次：`JJC-20261026-008`「第61轮：125积压不丢与动态飞行参数、逐cue离线混音」。
- 结论：**PASS-WITH-PARTIAL 收口成立**；12 项 PARTIAL 承接 R62；不得称「整体 PASS」。

## 抽核记录（2026-10-26 20:5x +08:00，`sha256sum`/`stat`/`grep` 实算）

| # | 项 | 结果 |
|---|---|---|
| 1 | 门禁原件 | `target/round61/gate/attempt-1.log` 207,593B sha256 `6ff5f68dd112020e20d42ddf8d6f835be7099dab999e1a411d1b9b4cff77ad96`（重算一致）；`attempt-1.meta.txt` 十字段含 attempt=1/head=ef317b0/exit=0/capture=full；末行「==> 全部通过（12 项）」；UTC 12:06:56–12:09:29Z。PASS |
| 2 | 三产物重算 | `final-manifest.json` 9,331B `a9f66a40…c232`；`round61-final-audit.md` 5,690B `b86ed441…9902`；`shangshu-acceptance.md` 4,255B `d1b38b42…5025`。PASS |
| 3 | 提交边界 | `ef317b0` 8 文件 +2216/−232（代码批）；`1e6cfca` 文档批；`7baa3a8` 审计产物；`git diff 1e6cfca 7baa3a8 -- crates/` 输出为空（exit=0）。PASS |
| 4 | PARTIAL 诚实 | 4 个红线词 grep 于 README/client：4 处命中全为否定语境（README:40、client:1284、client:1345）；12 项 PARTIAL 台账见 README §4 与验收 F 段。PASS |
| 5 | M1–M6 闭合 | README §2 M1–M6 表 + D4 `round61-final-audit.md`（PASS-WITH-PARTIAL、必改 0）+ 尚书验收 A–F（ACCEPT-WITH-PARTIAL）。PASS |
| 6 | 假绿/卫生 | D4 复核记录：新增 `#[allow]/#[ignore]/cfg(feature)` 0、中文 assert 0、新增 blob >1MiB 0；`run-checks.sh` 带 0 tests 防护，D3 实跑 12/12 PASS。PASS |

## 结论与边界

- M1–M6 按门下裁决闭环（M5 记 `gate_head=ef317b0`，门禁在提交① HEAD 唯一跑一次；提交②/③仅文档与审计产物，`crates/` 零差异）。
- 保留 PARTIAL 12 项（append-only 回收契约、完整 NextFrame/SkipFrames、TargetPoint、像素 offset、设备/Live 可听、33 叠加 peak≈22.844788 削波风险、105 sfx 未逐一、HiddenSpanSource 边界、CWD/素材来源、会话代号游标歧义、SKIP→PARTIAL、R60 P-1..P-6 不清零），由 `round61-shangshu-acceptance.md` F 段逐条承接 R62。
- 术语禁用：不得表述「125 全链」「渲染完整」「真实可听已证」「R61 完整」。

## 代笔说明（如实留痕）

门下确认 gpt-6.1-sol subagent 两次尝试均在落盘/CLI 前超时或失败、无产物（与 R53 先例同类）。本确认文件由队长按门下角色代笔落盘，所需事实均由 `sha256sum`/`stat`/`grep`/`git diff` 现场重算并写明数值；看板 `state Done` 与 `confirm approve` 由队长以 `--as menxia` 合法路径执行并保留原始 CLI 回执。未改代码、证据与门禁日志，未 commit 本文件之外任何内容。
