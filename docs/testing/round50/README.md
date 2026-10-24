# R50 证据索引（第 50 轮：独立商城面板 + i18n 收口 + 证据重跑）

> 父单 `JJC-20261025-002`；本索引由礼部 R50-H（子单 `JJC-20261025-002-sub-a78487f6`）维护。
> 基线 HEAD `d5707c6d7f61af8f02c15f14f54fb4b24b6e4068`；A 批文档提交后 `2a7e77e`；
> **R50 的功能改动（B/C/E/F 的代码与测试）未提交**，仍在工作树里。
> 文中所有行号写「`文件:行` @ `d5707c6`/`2a7e77e`+R50 工作树（2026-10-25 取样）」；
> 并发改动会让行号漂移，引用前先按内容 grep，不要拿行号当永久事实。
> 已实测漂移：R50-C 的 `k-hotkey-evidence.txt` 记 `app.rs` 2853/2920/2951-2953/2272，
> 06:2x 复核为 2857/2920/2955-2957/2268（§4 引用的是取证值）。
>
> 本索引**只引用目标树里真实存在的文件**。证据根目录：`target/round50/`（`target/` 不入库）。
> 判读纪律：没有证据路径的数字不写；命令红过的必须写红；「PASS」必须有日志原文。

## 0. 一页速览

| 批次 | 内容 | 结论（实测） | 证据入口 |
|---|---|---|---|
| R50-01 | 制度文档事实核对 | `facts ok: 77`（plan）/ `57`（review）；108 条锚点 hit 92 / deviation 16 | [`target/round50/facts/`](../../../target/round50/facts/) |
| R50-A | 门禁行刷新（文档） | 3 个文件就地刷新，无行删除 | [`target/round50/r50-a.log`](../../../target/round50/r50-a.log) |
| R50-B | 服务端商城包链 | `cargo test -p mir-server` **879 passed / 0 failed**；gameshop **21**、gameshop_wire **3**、gameshop_failure **1** | [`target/round50/r50-b/README.md`](../../../target/round50/r50-b/README.md) |
| R50-C | 客户端独立商城面板 | lib gameshop **15 passed**、gameshop_panel **10 passed**、round2_panels **28 passed**（TMP 覆盖） | [`target/round50/r50-c/`](../../../target/round50/r50-c/) |
| R50-D | 商城素材 manifest | **82 条 / ok 82 / partial 0**；`--check`、`--verify-icons`、`--verify-release` 均 exit 0 | [`target/round50/r50-d/README.md`](../../../target/round50/r50-d/README.md) |
| R50-E | i18n no-test 15 → 0 | no-test strict **0**（exit 0）；strict **60**（≤70，exit 0）；`ci.ps1:136` 阈值 15→0 | [`target/round50/r50-e/README.md`](../../../target/round50/r50-e/README.md) |
| R50-F | 证据重跑 | F1 `SAME_SNAPSHOT_PASS` 7/7、F2 `F2_PASS` 5/5、F3 语义一致（像素 3/25 帧不同）、冻结目录 `ZERO_CHANGE` 70 文件 | [`target/round50/r50-f/r50-f-evidence-summary.md`](../../../target/round50/r50-f/r50-f-evidence-summary.md) |
| R50-H | 文档终稿（本索引对应） | 5 个 docs 改写 + 本索引；无代码改动 | 本文 §7 |

## 1. R50-01 制度文档事实核对

目录：[`target/round50/facts/`](../../../target/round50/facts/)

| 文件 | 用途 |
|---|---|
| [`facts.json`](../../../target/round50/facts/facts.json) | 79,374 B，108 条锚点 + 12 条 key_checks + 6 条 doc_defects（本轮事实源） |
| [`evidence.md`](../../../target/round50/facts/evidence.md) | 人读摘要（5,128 B） |
| [`verify.py`](../../../target/round50/facts/verify.py) | 核对器：`py target/round50/facts/verify.py docs/process/round50-plan.md` |
| [`verify.plan.out.txt`](../../../target/round50/facts/verify.plan.out.txt) / [`verify.review.out.txt`](../../../target/round50/facts/verify.review.out.txt) | 两次核对的原始输出 |

输出原文（`verify.plan.out.txt` 末三行）：

```text
facts ok: 77
facts entries: 108 (hit 92 / deviation 16); key_checks: 12; doc_defects: 6
```

`verify.review.out.txt` 末行同为 108/92/16、12、6，但首行是 `facts ok: 57`——
两份文档各自的「ok」计数口径不同（77 是 plan 的可判定锚点数，57 是 review 的），
**不要把 77 与 57 混写成一个数**。deviation 16 条与 doc_defects 6 条见 `evidence.md`，
本轮不改 `docs/process/**`。

## 2. R50-A 门禁行刷新（文档批）

证据：[`target/round50/r50-a.log`](../../../target/round50/r50-a.log)（自身记录 BEFORE/AFTER 哈希与
`DIFF3_*` 隔离证明）。

* 写入范围：`README.md`（第 49 轮门禁行）、`docs/testing/round49/README.md`（`:5`/`:57` 就地改 +
  末尾「## 门禁结果（R50-A）」节）、`docs/roadmap.md`（§六）。
* 基线 `d5707c6` → A 批提交后 `2a7e77e`（`docs/process/round50-plan.md` §四记录）。
* 该批**已提交**；R50-B/C/E/F 的改动**未提交**，两者不要混为「全都已入库」。

## 3. R50-B 服务端商城包链

入口：[`target/round50/r50-b/README.md`](../../../target/round50/r50-b/README.md)（139 行）、
复跑脚本 [`run-acceptance.sh`](../../../target/round50/r50-b/run-acceptance.sh)。

| 日志 | 命令要点 | 结果 |
|---|---|---|
| [`01-fmt-my-files.log`](../../../target/round50/r50-b/01-fmt-my-files.log) | `cargo fmt --check`（B 触及文件） | 0 |
| [`02-clippy.log`](../../../target/round50/r50-b/02-clippy.log) | `cargo clippy -p mir-server -p mir-protocol --all-targets` | 0 warning |
| [`03-test-gameshop.log`](../../../target/round50/r50-b/03-test-gameshop.log) | `cargo test -p mir-server --test gameshop` | **21 passed / 0 failed** |
| [`04-test-gameshop-wire.log`](../../../target/round50/r50-b/04-test-gameshop-wire.log) | `--test gameshop_wire` | **3 passed / 0 failed** |
| [`05-test-gameshop-failure.log`](../../../target/round50/r50-b/05-test-gameshop-failure.log) | `--test gameshop_failure` | **1 passed / 0 failed** |
| [`06-test-protocol-game-shop.log`](../../../target/round50/r50-b/06-test-protocol-game-shop.log) | `cargo test -p mir-protocol game_shop` | 0 |
| [`07-test-mir-server-lib.log`](../../../target/round50/r50-b/07-test-mir-server-lib.log) | `-p mir-server --lib` | 0 |
| [`08-test-mir-server.log`](../../../target/round50/r50-b/08-test-mir-server.log) | `cargo test -p mir-server`（55 套件求和） | **879 passed / 0 failed**，exit 0 |
| [`09-fmt-workspace.log`](../../../target/round50/r50-b/09-fmt-workspace.log) | 全 workspace fmt | 0 |
| [`10-gameshop-helper-diff.txt`](../../../target/round50/r50-b/10-gameshop-helper-diff.txt) | 测试 helper 容差 200→512（Lead 批准） | 仅测试夹具 |

代码改动（工作树，未提交）：`crates/mir-server/src/session.rs`（登录链 `send_gameshop_info`
与库存包时序）、`crates/mir-server/src/market.rs`（`game_shop_info_packet` /
`game_shop_info_packets` / `stock_packet`）、`crates/mir-server/tests/{gameshop.rs,
gameshop_wire.rs,gameshop_failure.rs}`。

已知历史红：[`00d-transient-E-lib-compile-error.log`](../../../target/round50/r50-b/00d-transient-E-lib-compile-error.log)
是 B 执行期间 E 并行改动造成的**瞬时**编译失败（非 B 缺陷，已在同一目录留档）。

## 4. R50-C 客户端独立商城面板

目录：[`target/round50/r50-c/`](../../../target/round50/r50-c/)（**无 README**，以日志原文为准）。

| 日志/产物 | 命令 | 结果 |
|---|---|---|
| [`c1-lib-gameshop.log`](../../../target/round50/r50-c/c1-lib-gameshop.log) | `cargo test -p mir-client --lib ui::gameshop` | **15 passed / 0 failed**（273 filtered） |
| [`c2-gameshop-panel.log`](../../../target/round50/r50-c/c2-gameshop-panel.log) | `cargo test -p mir-client --test gameshop_panel` | **10 passed / 0 failed** |
| [`c3-round2-panels-tmp-override.log`](../../../target/round50/r50-c/c3-round2-panels-tmp-override.log) | `--test round2_panels`（显式 `TEMP/TMP`） | **28 passed / 0 failed** |
| [`c3-round2-panels.log`](../../../target/round50/r50-c/c3-round2-panels.log) | 同上，未覆盖 `TEMP/TMP` | 27 passed / **1 failed**：`settings_roundtrip_through_disk_survives_restart`（`tests/round2_panels.rs:198`，`std::env::temp_dir()` 写权限；环境问题，与商城无关） |
| [`c4-lib-labels.log`](../../../target/round50/r50-c/c4-lib-labels.log) | `--lib ui::labels` | **7 passed** |
| [`k-hotkey-evidence.txt`](../../../target/round50/r50-c/k-hotkey-evidence.txt) | `K` 键逐条取证 | 见 `docs/client.md` §5.16.1 |
| `gameshop-panel.png` / `-320x240.png` / `-empty.png` / `-disconnected.png` / `gameshop-market-coexist.png` | 面板截图（各 1 张） | 单次取证，无像素回归基线（PARTIAL） |

`k-hotkey-evidence.txt` 的锚点（`@2a7e77e`+R50 工作树）：
`app.rs:2853`（`handle_live_key`）、`app.rs:2920`（文本输入早退）、`app.rs:2951-2953`（`"k"` 分支）、
`app.rs:2272`（`live_toggle_gameshop` 内唯一 toggle 调用）。

## 5. R50-D 商城素材 manifest

入口：[`target/round50/r50-d/README.md`](../../../target/round50/r50-d/README.md)。

| 日志 | 命令 | 结果 |
|---|---|---|
| [`commands.log`](../../../target/round50/r50-d/commands.log) | `gameshop_manifest.py --write/--check/--verify-icons/--verify-release` | 四条均 exit 0 |
| [`cross-check.txt`](../../../target/round50/r50-d/cross-check.txt) | manifest vs Rust `db.json` / clean-report / mirdb-report / sha256sum / 发布包 | 一致 |
| [`icons.sha256`](../../../target/round50/r50-d/icons.sha256) | 82 个图标的逐条哈希 | 82 行 |
| [`mirdb-report.txt`](../../../target/round50/r50-d/mirdb-report.txt) | `Server.MirDB` 商城段解析 | `gameshop: 118` |
| [`partial-sim.log`](../../../target/round50/r50-d/partial-sim.log) + `partial-sim/` | 缺 1 图标 / 篡改 1 字节 / 篡改源 DB 三类 fail-closed 模拟（隔离副本） | A: `partial=1` → `--check` exit 1；B: 哈希不符 exit 1；C: `--write` exit 2 |
| [`gaps.txt`](../../../target/round50/r50-d/gaps.txt) | 未覆盖/降级清单 | 见 `docs/world/gameshop.md` §1.1 PARTIAL |

产物：`docs/assets/gameshop-manifest.json`（82 条、`ok=82`、`partial=0`、
`license.policy_id=resource-policy-2026-10-20`、发布包 `sprites-item-1.mnp`
827 条 / 772,675 B / `sha256=3b28fa1b…`）。

## 6. R50-E i18n 三分收口

入口：[`target/round50/r50-e/README.md`](../../../target/round50/r50-e/README.md)。

| 日志 | 命令 | 结果 |
|---|---|---|
| [`00-baseline-no-test-strict.log`](../../../target/round50/r50-e/00-baseline-no-test-strict.log) | `--player-visible --no-test --strict --fail-above 0`（基线 `d5707c6`） | **15 处 / exit 1** |
| [`09-final-no-test-strict.log`](../../../target/round50/r50-e/09-final-no-test-strict.log) | 同命令（最终） | **0 处 / exit 0** |
| [`10-final-strict.log`](../../../target/round50/r50-e/10-final-strict.log) | `--player-visible --strict --fail-above 70` | **60 处 / exit 0**（≤70） |
| [`11-final-verify-text-keys.log`](../../../target/round50/r50-e/11-final-verify-text-keys.log) | `py tools/i18n/verify_text_keys.py` | exit 0（207 行→200 键、241 键、两包 241、差异 0） |
| [`12-final-cargo-test-mir-core.log`](../../../target/round50/r50-e/12-final-cargo-test-mir-core.log) | `cargo test -p mir-core` | 48 unit + 9 integration + 2 doc 全绿 |
| [`13-final-embedded-pack-test.log`](../../../target/round50/r50-e/13-final-embedded-pack-test.log) | `embedded_pack_matches_builtin_text` | 1 passed / exit 0 |
| [`14-regression-mir-server.log`](../../../target/round50/r50-e/14-regression-mir-server.log) | `cargo test -p mir-server` | 全绿（唯一失败为 `conquest_e2e::conquest_reload_from_postgres` 缺 `MIR2_TEST_DATABASE_URL`，与本改动无关，见日志） |
| [`05-tree-state.txt`](../../../target/round50/r50-e/05-tree-state.txt) | 工作树 diff（含 `tools/dev/ci.ps1:136` 15→0） | 阈值改动的原始 diff |
| [`07-wiring-mutation-test.log`](../../../target/round50/r50-e/07-wiring-mutation-test.log) | 显示边界接线的「改坏变红/恢复变绿」 | 可红测试通过 |

15 条去向的两条 deviation（8 条显示名走并行显示词表、写范围扩展 `world.rs`/`maps.rs` 由 Lead 授权）
已写进 [`docs/i18n.md`](../../i18n.md) §7.8，本索引不重复。

## 7. R50-F 证据重跑

入口：[`target/round50/r50-f/r50-f-evidence-summary.md`](../../../target/round50/r50-f/r50-f-evidence-summary.md)、
交付回执 [`delegate-result.txt`](../../../target/round50/r50-f/delegate-result.txt)。

| 项 | 结果 | 证据 |
|---|---|---|
| F1 `monster_native_evidence`（`--ignored`） | exit 0；`SAME_SNAPSHOT_PASS` **7/7** | [`run-native.log`](../../../target/round50/r50-f/run-native.log)、[`f1-snapshot.json`](../../../target/round50/r50-f/f1-snapshot.json) |
| F2 `monster_motion`（`--ignored`） | exit 0；`F2_PASS` **5/5** | [`run-motion.log`](../../../target/round50/r50-f/run-motion.log)、[`f2-motion-snapshot.json`](../../../target/round50/r50-f/f2-motion-snapshot.json) |
| F3 attack 两跑 | 两跑 exit 0；语义 `SEMANTIC_IDENTICAL`（digest `2a80ecc411e48530…`）；**像素 3/25 帧不同**（不写 0 diff） | [`attack-run1.log`](../../../target/round50/r50-f/attack-run1.log)、[`attack-run2.log`](../../../target/round50/r50-f/attack-run2.log)、[`f3-attack-semantics.json`](../../../target/round50/r50-f/f3-attack-semantics.json)、[`f3-png-getfilehash.json`](../../../target/round50/r50-f/f3-png-getfilehash.json) |
| 冻结目录 | `ZERO_CHANGE`：70 文件 / 129,219,674 B，added=0 removed=0 changed=0 | [`frozen-before.json`](../../../target/round50/r50-f/frozen-before.json)、[`frozen-after.json`](../../../target/round50/r50-f/frozen-after.json)、[`frozen-compare-final.json`](../../../target/round50/r50-f/frozen-compare-final.json) |
| 代码改动 | 仅两个测试各 +15/−1（`MIR_NATIVE_EVIDENCE_DIR`/`MIR_MOTION_EVIDENCE_DIR` 覆盖，默认不变） | [`test-diff.patch`](../../../target/round50/r50-f/test-diff.patch) |
| 产物清单 | 147 文件 / 75,431,443 B 逐文件 sha256 | [`artifact-inventory.json`](../../../target/round50/r50-f/artifact-inventory.json) |

攻击运行尝试残档：[`run-native-attempt1-tmp-perm-denied.log`](../../../target/round50/r50-f/run-native-attempt1-tmp-perm-denied.log)、
[`run-native-attempt2-tmp-pinned-denied.log`](../../../target/round50/r50-f/run-native-attempt2-tmp-pinned-denied.log)
是沙箱临时目录权限导致的前两次失败（如实保留）；成功路径以 `run-native.log` 为准。

## 8. R50-H 文档终稿（本批）

改动文件（全部在允许白名单内）：

| 文件 | 内容 |
|---|---|
| [`docs/world/gameshop.md`](../../world/gameshop.md) | 重写：Crystal 包链对齐（§7.3）、`GIndex` 口径偏离（§7.3.3，T 核对点）、库存公式（§8.1）、失败零包（§8.4）、售罄保留 vs Crystal 移除（§7.4）、客户端面板（§7.3.4）、manifest（§7.3.5）、PARTIAL（§1.1、§12） |
| [`docs/client.md`](../../client.md) | §5.16 更新 + 新增 §5.16.1 独立商城面板/`K` 规则/`GameShopState`/占位规则；§8 剩余项同步 |
| [`docs/i18n.md`](../../i18n.md) | §7.1 口径改 60/0、§7.3/§7.4 标历史并指向 §7.8；新增 §7.8（15 条去向、两条 deviation、`ci.ps1:136`、日志链） |
| [`docs/assets/licensing.md`](../../assets/licensing.md) | 新增 §5 商城素材 manifest（字段、命令、exit code、fail-closed） |
| [`docs/assets/game-assets.md`](../../assets/game-assets.md) | §5.1.1 末尾加「数字更正」块：827 条 / 772,675 B / `sha256=3b28fa1b…`，并标 45 包总量未重测（PARTIAL） |
| [`docs/testing/round50/README.md`](README.md) | 本索引 |

约束遵守：只写上述 6 个文件；未 `git add/commit`、未跑 `cargo`/`ci.ps1`、未改
`crates/`、`config/`、`tools/`、`README.md`、`docs/roadmap.md`、`docs/testing/round49/**`、
`docs/process/**`。本批**不新增任何实现状态**：所有「已实现/已通过」均回指本索引或子单证据。

## 9. 未决与 T 核对点

1. `S.GameShopStock.GIndex` 用商品 `GIndex` 而非 Crystal 的 `Info.Index`（`docs/world/gameshop.md` §7.3.3）——
   请核对中书决策 1 原文是否就是这个口径。
2. 服务端 `gameshop.rs` 的 21 条用例里第 16-21 条只给了「6 条新增」总数，
   逐条名称以 `target/round50/r50-b/README.md` 为准；H 未逐条展开。
3. `docs/assets/game-assets.md` 的 45 包总量 / 151,750 条是 2026-10-22 旧值，本轮未重测（PARTIAL）。
4. 客户端面板只有单次截图，无像素回归基线；「售罄保留条目」与 Crystal 的「移除条目」
   差异需在验收单确认。
5. `docs/process/round50-{plan,menxia-review,shangshu-dispatch}.md` 的 deviation 16 / doc_defects 6
   属 R50-01 核对范围，本轮**未修改**这些文件。
