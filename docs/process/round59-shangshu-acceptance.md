# R59 尚书省独立验收（第59轮：客户端 125 渲染最小闭环）

- 验收对象：`JJC-20261026-006`（Doing）；HEAD `1b51a85` = `origin/main`；工作树仅 2 个历史冻结 JSON 未跟踪（44,844,165B / 45,219,533B）。
- 方式：只读 `crates/` 与证据；仅写本文件与 `target/round59/shangshu-acceptance/**`；未 commit、未跑 `ci.ps1`/ignored 测试。
- **verdict：ACCEPT-WITH-PARTIAL（代码/文档/证据可接受）；看板收口 BLOCK（C 项不成立，`done` 被 readiness 拦截）**。

## A 门禁原件真实性 — PASS
- `attempt-1.log` 重算 **204,916B**、sha256 **0d96e061…9721**，与 `attempt-1.meta.txt` 记录一致。
- meta：head=`edaa5cd`、exit=0、09:24:22–09:27:11Z、capture=`full(*>)`、命令 `pwsh ci.ps1 -KillStale`。
- 12 项 OK（fmt / clippy / duo_smoke / workspace test / 协议生成 / 文档链接 / TextKey / 两项棘轮 / 老资源格式 / 图标 / PG），末行「==> 全部通过（12 项）」。
- 门禁后 `git diff --stat edaa5cd..1b51a85` = **4 文件 +460/−11**，全为 docs（md/json），无 crates/scripts。

## B 提交边界 — PASS
| 提交 | 文件 | 行数 | 与清单 |
|---|---|---|---|
| `7939133` | 10 | +2840/−2 | 一致（ui/projectile.rs 859 行等） |
| `edaa5cd` | 6 | +246/−8 | 一致 |
| `cde01a3` | 2 | +11/−11 | 一致（M-1 行号） |
| `1b51a85` | 2 | +449 | 一致（D6 产物 12,565B + 16,261B） |
- 树哈希：`01-tree-hashes.txt` 锚 `edaa5cd` 时 **16/16 相符**（逐文件 `git show edaa5cd:<p>|sha256sum`）；对 HEAD 为 14/16，2 个 docs 由 `cde01a3` 有意改动（client.md 163,013B、README 13,460B vs 清单 163,030B/13,385B）——属审计后必改闭环而非篡改，但清单未随 HEAD 重签。
- 最大新增 **37,167B**（projectile.rs），无 >1MiB 新文件；无 `target/`、`game-assets/`、`resource/` 入库。

## C 子任务映射 — 不成立（阻断）
- D6=`sub-d4af3456` **Done**，delegate-result 已回写（刑部→尚书省）。
- D1–D5（`sub-33ddfe11`/`f290c4de`/`375a7fe0`/`4adf53b8`/`9092eb29`）**仍 Doing**，todos 0/0，无 delegate-result；coordinator `sub-6f65c463` 亦 Doing。
- 父单 JSON：`todos: []`，flow 仅「皇上→太子 下旨」，无中书→门下→尚书流水。「7 子任务均 Done」与看板不符。
- 结论：看板未收口，`done` 必被 readiness 拒绝；须 Lead 回填 D1–D5 结果并转 Done 后方可收口，不得绕过。

## D 目标闭合 — PASS（G4 附 PARTIAL）
- G1 render **16/0**；G2 assets **6/0**；G4 sound **8/8**（`lead-verify/02-lib-sound-retry.log`；首跑 `02` 为 7/1，1 failed 系临时目录 PermissionDenied 环境失败，已复跑）；G3 命中 570 由 render #11–13 覆盖；G5 run-checks **9/9、总 EXIT=0**（`lead-verify/10-run-checks.log`）。
- G4：逻辑落盘 + cue 断言；真实可听 **PARTIAL 属实**（`take_sound_cues()` 未接 `app.rs` 音频通道，文档未冒充可听）。
- 环境性失败：`mir-render` 全量 4 failed / `mir-client --lib` 16 failed 为临时目录 PermissionDenied（D6 已核），本验收未复跑全量；门禁 `cargo test --workspace` 在 `edaa5cd` 为 OK。
- G6 文档入树；M-1 已由 `cde01a3` 闭环；`check_links` 1734 链接 **exit 0**。

## E 假绿/捷径 — PASS
- 新增 `#[allow]`/`#[ignore]`/`cfg(feature)` = **0**；260 条新增断言中中文断言 = **0**。
- i18n：player-visible 66 ≤ 阈值 70；no-test 0；`run-checks.sh:245` 有 0-tests 正则守卫。
- 无「125 全链 / 渲染完整 / R59 完整 / 全绿」正面断言；`docs/client.md:1390`「460 用例全绿」为 R7 既存文本，本轮 diff 0 命中。

## F 治理与证据 — PASS（D1–D5 回写缺，见 C）
- 三件 sha 相符：plan `5a233b58…`、review `7fce80fa…`、dispatch `99965f88…`。
- 证据目录 d1–d5b、lead-verify、gate、commit-verify、audit 齐备；D6 delegate-result 已回写。

## G 结论边界 — PASS
- 口径为「最小闭环 + 已知偏离/PARTIAL」；manifest 6 条 non_claims、README §3 台账 7 条、§4 R60 承接 6 项（brief 记 5 项，差异为第 6 项 Hero/真 socket 前置）。
- 禁称项无违反；未发现逐字一致/可听已证等过度声明。

## PARTIAL 台账（7 条）
`D-R59-1` 无落点不生成/不播（非对齐）；`P-2` 事件无 ID/序号；`P-3` duration/count 冻结；`P-4` draw 不叠逐帧 `ImageInfo.offset`；`P-5` 真实可听 PARTIAL（素材已导入）；`P-6` 无 Missile/Effect 类体系；`P-7` 无 Hero/真 socket。

## R60 承接（6 项）
1 事件 ID/`started_ms`；2 每帧重算 Duration/Count；3 wzl 逐帧 offset；4 命中生命周期落 socket 级用例；5 M34/106 帧 PNG 生成可复跑；6 Hero 实例/真 socket。

## CLI（原始 exit/stdout 见 04-cli.log）
- `memo` **exit 0**（「任务记忆：shangshu」）；`todo … 7 … completed` **exit 0**（#7 → completed，进度 1/1）。
- `done --as shangshu` **exit 3**：「❌ 委派 `JJC-20261026-006-sub-9092eb29` 尚未完成或结果未回填，禁止收口」——与 C 一致，未绕过。
- `validate --as shangshu` **exit 0**：「169 条任务符合 schema v3.1」。
- 建议：Lead 回填 D1–D5 `delegate-result`→`Done`，再执行父单 `done`；R60 将树哈希清单自身哈希与门禁 HEAD 头纳入冻结。
