# R70 C 首轮证据勘误（R70-T1）

- 范围：更正 `docs/testing/round70/round70-c-evidence.md` 与 `target/round70/c/attempt-1/runs.json` 对首轮独立执行的概括；**原件保留不覆盖**，本文件为追加勘误，同样是证据。
- 作者/时间：Lead 按终审 R70-T1 转录；C 首轮执行者为 gpt `fd4a61e6`（已终态）。UTC 2026-10-27（终审复核：`target/round70/t/attempt-1/runs.json`、`static-audit.json`）。
- 依据：原始日志 `target/round70/c/attempt-1/logs/04-render.log` L21–27（Compiling/Finished 12.70s）、L159–180（四失败 trace）、L180（`129 passed; 4 failed`）；`01-round70.log`、`02-wire.log`、`03-live.log`、`05-clippy.log` 有 `cl : Command line error D8050` 编译失败。

## 事实更正

| 项 | 原文错误概括 | 原始日志事实 |
|---|---|---|
| 04 `cargo test -p mir-render --lib` | 「未运行 tests（ring 编译失败）」 | **编译成功（12.70s），实际运行 133 例：129 passed / 4 failed / 0 ignored**；四败均为 `Os { code: 5, kind: PermissionDenied }` 临时目录/文件（`entities.rs:2244` 两例、`text.rs:790`、`tiles.rs:372`），非 ring D8050。 |
| 01/02/03/05 | 「5 条 Cargo 命令均被 D8050 阻断」 | 这 4 条确为 MSVC `cl D8050` 编译失败、未运行 tests；**总数是「4 条编译阻断 + 1 条执行后权限失败」**，不能概括为「5 条/未运行 tests」。 |
| 06 `cargo fmt --check` | EXIT0 | 一致，EXIT0。 |
| `target/round70/c/attempt-1/runs.json` 第 8 行 | 同 04 的错误概括 | 以本勘误与 `04-render.log` 为准；runs 原件保留作为当时报告文本的历史记录。 |

## 影响与后续

- 功能/测试实现无影响：该 4 失败为**环境性临时目录权限**（宿主 TEMP 指向受限目录），不是产品断言失败；C2 在全新 `CARGO_TARGET_DIR` 与 `CARGO_PROFILE_DEV_DEBUG=0` 下独立复跑 render lib **133/0** 已关闭「独立执行」缺口（见 `round70-c-addendum.md`）。
- 但 C2/终审的 133/0 **不能改写首轮历史**：首轮的真实分类是「4 命令 D8050 阻断 + 1 命令 129/4 权限失败」。
- `docs/testing/round70/README.md` 的 C 首轮行与 `final-manifest.json` 的 `disclosures[0]` 已按本勘误刷新；终审报告留原件并在 addendum 复评。

## 不复述范围

- 不改变 C 首轮 verdict 的历史表述（PASS-WITH-PARTIAL/首轮独立执行 PARTIAL）；本勘误仅更正失败分类与计数。
- 不涉及四变异判定（M2 初始注入器 E0599 不计有效红的披露成立）；不涉及实现、测试、wire、历史 freeze。
