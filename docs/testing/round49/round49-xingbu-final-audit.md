# R49-06 刑部/御史台只读终审报告

- 子单：`JJC-20261025-001-sub-6282c186`
- 角色：刑部/御史台只读终审（gpt-6.1-sol），不改实现、不跑门禁、不提交
- 时间：2026-10-25 04:xx +08:00
- 仓库：`C:\Users\diana\Documents\github\mir-next`，HEAD `f0be364`
- 审计范围：工作树 39 条 `git status --porcelain`（25 M + 14 untracked，展开 119 文件）
- 判据来源：`docs/process/round49-shangshu-dispatch.md` §三/§六、`docs/process/round49-menxia-review.md` §五、队长唯一门禁 `target/round49/final-gate.meta.txt` + `final-gate.log`
- 机器可读清单：`docs/testing/round49/final-manifest.json`（99,382 B，sha256 `4aac0e5120dbc1a9335aa86b6b86aacedadbcbe79ae2080abf6188f96cd036aa`）

## 1. 裁定

**verdict = PASS-WITH-PARTIAL**

理由（有证据的 PASS 面）：
1. 队长唯一全量门禁实测 **12/12 项 OK、原始 exit code 0、176s**；退出码取自 meta 字段而非 Tee/管道码。
2. 119 个待审文件逐文件 bytes+sha256 与冻结/R49-05 基线对照，**任何漂移 = []**；104 个 R49-01 冻结文件、12 个 R49-05 文档阶段文件全部一致，team-and-roles.md 单列。
3. 入库卫生通过：无 `>1MiB` 文件进入候选、无 `target/`、`.env`、`github/`、`resource/`、`game-assets/` 命中；两个 44/45MB JSON 已单列排除。
4. 验收命令链完整（13 条，见 §4）；ignored 证据测试在缺素材时 **assert 失败而非空过**（源码复核，见 §6）。
5. 文档自洽：monster-motion README 只引用 4 张实存 PNG；monster-native README/freeze.json 如实标 PARTIAL；attack-variants README 已描述新版行为；66/15/670/39 与原始日志一致。

保留的 PARTIAL（PASS 面不能覆盖的缺口）：i18n no-test 仍为 15（阈值 15，未清零）；monster-native 证据批次/二进制/源 blob 不一致；“放宽不改已接受表语义”论证仅覆盖 118 个 used 库；attack PNG 位级非确定；历史 self_moving/landed 产物缺失；R50 范围缺口与 4 v2/>800 缺源；根 README 与 round49 README 仍写门禁前状态。

未发现需要判 BLOCK 的假绿/伪造证据：门禁、哈希、退出码、证据日志四者交叉一致；已知不一致均被如实标注而非掩盖。

## 2. 门禁复核（不得以 Tee/管道码冒充）

meta：`exit_code=0`，`start_utc=2026-10-24T19:54:36Z` → `end_utc=2026-10-24T19:57:32Z`，`duration_s=176`。
日志：`target/round49/final-gate.log`，182,941 B，sha256 `5d475f4fb78a0ffd98d14ff9b0400d48c8d73ebbd3eb941c528a90f7048d9426`（复核一致）。
时间笔误：任务简报写 `2026-10-25T19:54:36Z`，meta/log 实测为 **2026-10-24T19:54:36Z**（本地 03:54:36→03:57:32），以 meta 为准。

| Code | Seconds | Name |
|---|---|---|
| OK | 2s | cargo fmt --all --check |
| OK | 6s | cargo clippy --workspace --all-targets -D warnings |
| OK | 8s | cargo test -p mir-server --test duo_smoke（串行） |
| OK | 153s | cargo test --workspace |
| OK | 1s | 协议生成器 --check |
| OK | 0s | 文档链接与包编号 |
| OK | 0s | TextKey 完整性 |
| OK | 0s | 硬编码棘轮（player-visible，阈值 70） |
| OK | 0s | 硬编码棘轮（no-test，阈值 15） |
| OK | 0s | 运行时不引用老资源格式 |
| OK | 1s | 物品图标完整性 |
| OK | 2s | PostgreSQL 集成测试（mir-store 串行） |

末行 `==> 全部通过（12 项）`；各项 Seconds 之和 173s，与 176s 的差额为启动/收尾。逐项摘要另存 `target/round49/final-gate-summary.log`（1,400 B，sha256 `3f3a9534895a2d26ef1bb5cd23188987495fbe62aa7b049f25fb672347763ac5`），未改动原始日志。

## 3. 工作树与哈希对照（39 条 → 119 文件）

- 39 条 = 25 M + 14 untracked（展开 94 文件）。
- 来源分解（逐文件 path/status/bytes/sha256 见 final-manifest.json）：
  - R49-01 冻结基线：104 文件，`any_mismatch=[]`；
  - R49-05 文档阶段：12 文件，全部与 manifest 中 `r49_05.sha256` 一致；
  - team-and-roles 重写：1 文件（`docs/process/team-and-roles.md`，无冻结基线，内容自述“2026-10-25 修订”，属 R49-06 允许演进）；
  - 审计产物：2 文件（`docs/testing/round49/wip-freeze.json`、`manifest.json`，自指，不参与冻结漂移对照）。
- 新增的 6 个 M：`README.md`、`docs/i18n.md`、`docs/process/team-and-roles.md`、`docs/roadmap.md`、`docs/testing/attack-variants/README.md`、`docs/world/gameshop.md` —— 5 个属 R49-05 文档阶段，1 个属团队分工重写；无未申报漂移。
- 抽查 `git hash-object --path`：`crates/mir-client/src/camera.rs` `ca93a4f5…`、`crates/mir-render/src/animation.rs` `47192e1d…` 与冻结 worktree blob 一致；`docs/client.md`、`docs/testing/monster-native/README.md` 与 R49-05 sha 一致。
- 入库卫生：`>1MiB` 仅 2 个（`player-resource-freeze-before.json` 44,844,165 B；`player-resource-freeze.json` 45,219,533 B），均已排除；禁止路径命中 0。提交候选 117 文件、41,408,996 B。

## 4. 验收命令覆盖（真实退出码与日志）

| # | 命令 | 退出码 | 依据 |
|---|---|---|---|
| 01a | `cargo test -p mir-asset --test real_framesets` | 0 | `target/r49-evidence-tests.log`（1 passed/3 ignored，ALLDONE；日志未打印 shell 码） |
| 01b | 同上 `-- --ignored --nocapture` | 0 | `target/r49/frames-negative-stride.log` 显式 `[exit code: 0]`，4 passed |
| 02 | `cargo test -p mir-render --test monster_native -- --ignored --nocapture` | 0 | 02:14 基线，3 passed（8.20s） |
| 03 | `cargo test -p mir-render --test monster_native_evidence -- --ignored --nocapture` | 0 | 02:14 基线，1 passed（33.99s） |
| 04 | `cargo test -p mir-client --lib -- camera::monster_motion_tests` | 0 | 02:14 基线，6 passed |
| 05 | `cargo test -p mir-client --test monster_motion -- --ignored --nocapture` | 0 | `target/r49/x1-03-cli-frames.log` 显式 `EXIT=0`，1 passed/4 filtered（3.89s） |
| 06 | `cargo test -p mir-tools --test monster_native_import -- --ignored --nocapture` | 0 | 02:14 基线，4 passed |
| 07 | `py -3 tools/i18n/scan_hardcoded.py --player-visible --strict --fail-above 70` | 0 | `r49-05-check1-strict.log`：66 处，`[OK] 未超过上限 70` |
| 08 | `py -3 tools/i18n/scan_hardcoded.py --no-test --player-visible --strict --fail-above 15` | 0 | `r49-05-check2-no-test.log`：15 处 = 阈值，未超；无显式 `[OK]` 行 |
| 09 | `py -3 tools/docs/check_links.py` | 0 | `r49-05-check3-links.log`：`[OK] 670 条相对链接…`（首次运行 exit 1 留档） |
| 10 | `git diff --check` | 0 | `r49-05-check4-diff-check.log` 末行显式 `[exit code: 0]` |
| 11 | `pwsh -File tools/dev/ci.ps1 -KillStale` | 0 | `final-gate.meta.txt` exit_code=0；12/12 |
| 12 | `cargo test -p mir-client --test cli_monster_frames -- --ignored` | 0 | x1-03 显式 `EXIT=0`，2 passed（6.67s） |
| 13 | `cargo test -p mir-client --test remote_motion_live -- --ignored` | 0 | x1-03 显式 `EXIT=0`，1 passed（17.85s，R46 双客户端） |

说明：02:14 的六段基线未逐段打印 shell 退出码，以 `test result: ok` + 驱动 `ALLDONE` 为证据（manifest 中 `exit_code_explicit_in_log=false` 如实标注）；其余关键命令均有显式退出码。

## 5. 关键论证判断：负 stride 放宽与 monster_native_evidence 时效缺口

- `target/r49/frames-negative-stride-scan.json`（933 B，sha256 `f53bcca9…`）实测：`items=123`、`valid_v3_frames=118`、`newly_accepted_frames=[]`、`negative_stride_frames_in_used_libs=0`、`single_frame_negative_skip_frames_in_used_libs=0`；非 v3 项为 515/530/531/534（old_no_frames）与 800（file_missing）。
- **论证成立范围**：在 118 个 used 库上，“放宽不改变已接受表语义”成立（`newly_accepted=[]` + 负 stride/skip 命中 0）。这足以支撑“已接受表不受放宽影响”。
- **不可外推**：任务要求的“全 900 库 oldRejected/newlyAccepted=0”在本轮工作树中**没有落盘产物**（`target/r49/scan-alliance-libs.mjs` 只是扫描脚本，无对应 JSON）。因此该论证不能覆盖全库，也不能消除 `monster_native_evidence` 的批次/二进制/source blob 不一致（P02）。
- 结论：monster_native_evidence 时效缺口保留为 PARTIAL，不以该论证判 PASS。

## 6. 捷径/假绿黑名单核对

- **ignored 缺素材是否 assert**：是。`crates/mir-render/tests/monster_native_evidence.rs:100-107` 对 `maps/0.mnm` 与 `packs/manifest.packs.json` 使用 `assert!`，错误信息明确“缺素材必须失败而不是空过”；`crates/mir-client/tests/cli_monster_frames.rs:92-96` 对 `0.map`/`0.mnm` 同样 `assert!`。未发现 `return Ok(())` 式静默跳过。
- **@revive / 静态 fixture / 伪造库存**：抽查新增测试未发现 `@revive`、伪造库存或整表硬编码；`monster_native_evidence.rs:57` 的 `"player fixture altAnim"` 是对 manifest 玩家布局的断言文案，不是伪造素材。
- **跨动作借帧 / 硬编码坐标**：本轮为只读抽样，未做全量 AST 扫描；x1-03 证据测试通过真实 render 场景取帧表对照（`render_scene` 注释与实现一致），未见过渡桩。该两项列为“未全量复核”，不据此加分。
- **PNG 位级非确定**：已披露（attack README §8，2/25 帧差，语义摘要 `d7a645df075396ac` 稳定），不属于伪造。
- **CLI 恒空表**：x1-03 证据为 273 个 lib 帧表与 CLI session 同源校验，`cli_monster_frames` 2 passed，未再呈现历史“恒空表”症状。

## 7. 文档自洽

- `docs/testing/monster-motion/README.md`（2,624 B，`a47f6bcc…`）只引用 4 张实存 PNG：`frame0`、`frame5`、`frame6_005_600ms`、`frame10_184_1000ms`。
- `docs/testing/monster-native/README.md`（7,724 B，`04e702b4…`）与 `freeze.json`（5,066 B，`4908cf14…`）如实标 PARTIAL，未伪造 batch/binary/blob 一致。
- `docs/testing/attack-variants/README.md`（17,952 B，`e8452aea…`）§8 描述新版行为：默认写 `target/attack-variants/`（`MIR_ATTACK_EVIDENCE_DIR` 可覆盖），绝不写 `docs/testing/attack-variants`；R47 批次只读；当前工作树该目录仅 README 为 M，27 张 PNG 已恢复与 HEAD 一致。
- `docs/testing/round49/README.md`（5,635 B，`d2e8dcfa…`）与 `i18n.log`（5,152 B，`7eafa30…`）的 66/15/670/39 与原始日志一致；`i18n.log` §5 的 669 为 03:46 历史中间值，§6 与最终复测为 670，属同文档内时间序列。
- 滞后项（不代改）：`README.md:74` 第 49 轮行仍写“current_gate=PARTIAL，待队长最终全量门禁”；`docs/testing/round49/README.md` 开头与 §4 同样滞后。

## 8. PARTIAL 清单（完整）

| ID | 项 | 状态 | 影响 |
|---|---|---|---|
| P01 | i18n no-test 15 处（阈值 15，未清零）；player-visible 66 处（阈值 70） | PARTIAL | 门禁通过但技术债保留 |
| P02 | monster-native 证据：batch 1792847715360 早于 raw-manifest 1792865639318；client_binary `00b3df83…`→`da0ab79a…`；source_blobs entities/animation/camera 三处不一致 | PARTIAL | 证据非同快照；已如实标注 |
| P03 | historical `self_moving.png`/`landed.png` 不存在（全仓）；README 已改为 4 张实存 PNG，仅 `crates/mir-client/tests/monster_motion.rs:334-335` 保留清理逻辑 | PARTIAL | 需队长/门下确认门下 §5.3 原文是否降级 |
| P04 | attack-variants PNG 位级非确定（2/25 帧差） | PARTIAL | 语义级可复现 |
| P05 | Blend117/Reverse/Effect/Sorting 未实现；坐骑/攻击变招/Push-Jump-Dash/商城列 R50；4 个 v2；`>800` 缺源（800 file_missing 等） | PARTIAL | 范围缺口，非假绿 |
| P06 | README.md:74 与 round49/README 仍写门禁前 PARTIAL | PARTIAL | 文档滞后，建议门禁后更新 |
| P07 | “放宽不改语义”仅 118 used 库有落盘证据，全 900 库无产物 | PARTIAL | 论证限域，不可外推 |
| P08 | R46 `remote_motion_live` 新增 `#[ignore]`，默认门禁不执行 | PARTIAL | 门禁覆盖缩小，需队长决定 |

## 9. 未决项与建议

1. P02 的 monster-native 证据若要 PASS，需以同一快照重跑并同时刷新 raw-manifest/binary/source_blobs 哈希；本轮只读，不代跑。
2. P03 建议队长向门下确认“4 张实存 PNG 已足够”是否可关闭 §5.3 的 self_moving/landed 要求。
3. P06 建议在最终提交前更新 README.md:74 与 round49/README 的门禁状态（不改本轮审计结论）。
4. P08 的 `#[ignore]` 应保留单独的执行记录（x1-03 日志已满足），并在门禁说明中注明覆盖缩小。
5. 全 900 库负 stride 扫描若需作为正式证据，应把 `target/r49/scan-alliance-libs.mjs` 的输出落盘为 JSON 并纳入下一轮证据。

## 10. 提交建议

**建议提交**：117 个已哈希候选 + 2 个门禁后终审文档（本报告与 final-manifest.json），共 119 路径。
- 提交边界：仅 `git add` 上述路径；**必须排除** `docs/testing/monster-native/player-resource-freeze-before.json`（44,844,165 B）、`player-resource-freeze.json`（45,219,533 B）、`target/**`、`.env`、`github/**`、`resource/**`、`game-assets/**`。
- 口径：final-manifest.json 与本报告是**门禁之后写入的审计产物**，属 `docs/process/round49-shangshu-dispatch.md` §六已声明的允许终审产物；它们不改变门禁所测代码/文档树（仅新增 docs/testing/round49 下两文件），因此不使 12/12 门禁失效。
- 提交前建议仅做两件小事：更新 README.md:74 / round49 README 的门禁表述；确认 team-and-roles.md 重写属本轮预期提交内容。
- 不提交 `target/round49/final-gate-summary.log`（target/** 排除；本地摘要用于复核）。

## 11. 复现

见 `docs/testing/round49/final-manifest.json` 的 `reproduce_commands`（15 条），核心：
`git status --porcelain` → 39 条；`py -3 target/round49/final_audit_hashes.py` → 119 文件、`any_mismatch=[]`；`Get-FileHash target/round49/final-gate.log` → `5d475f4f…`；门禁 `pwsh -File tools/dev/ci.ps1 -KillStale` → 12/12、exit 0。

---
本报告为只读终审结论；未运行全量门禁、未重跑 ignored 测试、未修改 crates/证据/旧日志、未 git add/commit。
