# 第 60 轮（R60）验收索引：125 音效真实通道 + 投射物事件去重

> 基线 HEAD `3a9a104`（＝本轮起点 `origin/main`）→ 交付代码 HEAD `bb53321`
> 「feat(client): R60 125 音效真实通道与投射物事件去重」（4 文件 +593/−12；`app.rs`、`ui/mod.rs`、`net.rs`、`ui/projectile.rs`）。
> 口径：**通道打通 + 去重 + PARTIAL 诚实登记**。本文**不**宣称「125 音频完整」「无重复」「真实可听已证」。

## 1. 目标与交付矩阵

| 目标 | 断言（可证伪） | 落点 | 状态 |
|---|---|---|---|
| **G1** 125 音效真实通道 | `take_projectile_sound_cues` 有生产消费者，`apply_snapshot` 同帧 drain 并逐个 `play`；`play` 返回 true 才算已播 | `crates/mir-client/src/ui/mod.rs:1275` `take_projectile_sound_cues`；`crates/mir-client/src/app.rs:2757` drain、`:2759` `play_projectile_cues`、`:201` 自由函数、`:209` 可注入核心；底座 `crates/mir-client/src/audio.rs:592` `play`（本轮零改动） | ✅ 通道通（可听性 **PARTIAL**，见 §4） |
| **G2** 125 事件去重 | 绝对序号游标：`dropped=total−len`、`cursor=max(consumed,dropped)`、`index=cursor−dropped`；`len=16,total=17,consumed=16` → 新增 **1** 个 view（旧实现 0） | `crates/mir-client/src/net.rs:2772` `projectile_events_total`、`:7265` 自增、`:2911` 重建归零；`crates/mir-client/src/ui/projectile.rs:461` `consume` | ✅ 已修（第 17+ 事件不再被吞） |
| **G3** 文档/脚本/树哈希 | 本文 + 11 项脚本 + 树哈希清单 + `client.md` §5.27；`check_links` / `git diff --check` exit 0 | `docs/testing/round60/` 三件、`docs/client.md`、`docs/roadmap.md` 九、`docs/testing/round59/README.md` P-2/§4 | ✅ 已落盘（待 D4 终审） |
| **G4** 唯一门禁不回归 | `ci.ps1 -KillStale` 在 `bb53321` 上 attempt-1 一次、exit 0、12/12 | 日志 `target/round60/gate/attempt-1.log` | ✅ exit=0（**终审结论不由本文代写**） |

## 2. 门禁记录（G4，唯一 attempt-1）

| 项 | 值 |
|---|---|
| 命令 | `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale *> target/round60/gate/attempt-1.log` |
| 结果 | **EXIT=0，12 项全 OK**，末行「==> 全部通过（12 项）」 |
| `gate_head` | `bb53321`（＝代码提交① HEAD，`git rev-parse --short HEAD`） |
| 日志路径 | `target/round60/gate/attempt-1.log` |
| 字节数 / sha256 | **205,317 B** / `193b6fe4f8a0a46397a0026c2e8a2d66f0e041c9fa33f0ad6ca3a17d1f3678dd` |
| 时间（UTC） | 10:29:53–10:32:26Z（本地 18:29–18:32） |

D1/D2/Lead 定向证据：`target/round60/d1/attempt-1/`（`--bin mir-client` 21 passed、`--lib audio` 19 passed、M34 素材断言无 SKIP）、
`target/round60/d2/attempt-1/`（红→绿 `04-red-before.log` exit 101 → `05-green-after.log` exit 0）、`target/round60/lead-verify/`。
本轮 D3 证据：`target/round60/d3/attempt-1/`（`01-run-checks-list.log`、`02-run-checks-run.log`、`02-run-checks-attempt-1-FAIL.log`、`logs/` 18 个文件、`03-check-links.log`、`04-diff-check.log`、`05-redline-grep.log`、`06-tree-hashes.log`、`commit-tree-hashes.txt`、`07-check-links-after.log`、`00-summary.md`、[gen-tree-hashes.sh](gen-tree-hashes.sh)）。

## 3. 测试名清单与计数（D1 5 例 / D2 6 例）

```text
D1（--bin mir-client，21 passed；5 个 R60 内联用例，均在 crates/mir-client/src/app.rs 的 mod tests）
  projectile_sound_cues_are_drained_and_played_once_each      # 出生 1 条 cue 播 1 次、同帧再取为空
  projectile_sound_cues_are_drained_only_once                 # 二次 drain 不重播
  no_projectile_events_drain_no_sound_cues                    # 无事件不触达播放通道
  disabled_or_zero_volume_sound_player_skips_projectile_cues_without_panic  # enabled=false / gain<=0 降级
  projectile_sound_assets_are_real_mna_clips                  # M34-1/2 → .mna 后缀 + AudioClip::open 解码成功（缺素材打印 SKIP:）
D1（--lib audio，19 passed） 既有降级/频控/fake-sink 用例全绿，无 SKIP
D2（--lib ui::projectile，12 passed；4 个 R60 内联用例 + 8 个 R59 既有）
  r60_d2_second_ring_full_event_is_consumed_once              # len=16,total=17 → 新增 1；push 至 18 → 再新增 1
  r60_d2_dropped_events_are_not_replayed                      # ring 丢最旧的事件不重放旧 view
  r60_d2_snapshot_rebuild_resets_cursor_without_replay         # 真重建（空表）→ events_consumed()==0
  r60_d2_consecutive_consumes_do_not_duplicate_payloads        # 连续 sync 不重复派发
D2（--lib net::，30 passed；含 2 个 R60 内联用例）
  r60_d2_projectile_events_total_counts_every_pushed_event    # 每次 push +1（含 ring 截断后）
  r60_d2_projectile_events_total_resets_with_rebuilt_snapshot # 默认/重建快照归零
回归：round59_projectile_render 16 passed、round58_projectile_wire 7 passed、mir-render sound 9 passed（断言值一个未改）
```

## 4. F6 更正说明（旧诊断作废）

| | 旧诊断（R59 / R60 rev1） | 更正后事实（R60 落盘） |
|---|---|---|
| ring 满的表现 | 「ring 截断 → 游标错位 → 整表重放」 | 该分支**生产不可达**：`while` 终态恒 `consumed == len`、`len` 由 `push_projectile_event` 封顶 16 |
| 真实缺陷 | — | **第 17 个及之后的 125 事件被永久静默丢弃**：`consumed` 停在 16、`views` 不再增长，之后本会话新 125 表现全消失 |
| 修复口径 | 防重放 | 绝对序号游标（`projectile_events_total`）：已消费事件不重放 + 第 17+ 事件不再被吞；**不保证不丢**（ring 16 仍丢最旧） |
| 确定性证据 | — | `len=16,total=17,consumed=16`：旧实现新增 **0**（`target/round60/d2/attempt-1/04-red-before.log`，exit 101），新实现新增 **1**（`05-green-after.log`，exit 0） |

## 5. PARTIAL / 已知限制（禁称「音频完整 / 无重复 / 真实可听」）

| 编号 | 项 | 边界 |
|---|---|---|
| P-1 | **可听性 PARTIAL** | headless 本机无音频设备：只证 cue→解码→混音器路径与降级不 panic；`take_projectile_sound_cues` 的集成点是 `apply_snapshot`，**无 Live 窗口级集成测试**（依赖真实窗口），未做真人手听 |
| P-2 | `from_repo_root` 找不到 M34 素材 | 既有 `audio.rs`/`main.rs` 根路径行为（CWD 相对 `default_roots()` 才生效），本轮未改代码；换机时按 `SKIP: missing M34-*.mna` 记非 PASS |
| P-3 | ring 16 仍丢最旧（R4a 不修） | 已消费事件不重放、第 17+ 事件不再被吞；但卡顿 >16 事件时更早于 ring 的历史事件仍会丢 |
| P-4 | 同文件 100 ms 频控 | 同帧多 cue 只响一次，≠ Crystal「每发都响」（批准保留的降噪设计）；drain 计数区分「取走 cue 数」与「`play` 返回 true 数」 |
| P-5 | 素材来源 | `game-assets/audio/sfx/M34-{1,2}.mna` 由 R59 从 `resource/community/gqf2008-crystal/Sound/M34-{1,2}.wav` 导入；`game-assets/`、`resource/` 均 `.gitignore:27` 忽略，证据只在 `target/` |
| P-6 | 本次脚本运行环境 | attempt-1 首跑 01/02 两项 FAIL（exit 101，均因系统 TEMP 不可写导致用例「建临时目录」`PermissionDenied`，与 R60 逻辑无关）；脚本改为把 `TMP/TEMP/TMPDIR` 指到仓内 `target/tmp` 后 **11/11 PASS、exit 0**。首跑日志保留为 `02-run-checks-attempt-1-FAIL.log`，不删不盖 |

## 6. R61 承接

1. **可听性收口**：真机手听一次 `M34-1/2` 并留证；补 Live 窗口级集成测试（把 drain 点抽到可注入的帧函数）。
2. **`started_ms` 时间戳**：R59 §4-1 只做了绝对序号，事件仍无时间戳/时长；需要「迟到事件」判定时再加。
3. **ring 16 扩容评估**（R4a）：按最大卡顿事件量定容量或改无锁队列，属协议外纯客户端改动。
4. **R59 未决 P-3/P-4**：`duration/count` 每帧重算、`magic.wzl` 逐帧 `ImageInfo.offset`。
5. **素材生成可复跑**：`M34-1/-2.mna` 与 106 帧 PNG 的 import 流程脚本化（`game-assets/` 不入库）。

## 7. 复跑命令

```bash
# 全部 11 项（--list 零执行；发现数校验 + 0 tests/0 passed 判 FAIL + SKIP/PARTIAL 记非 PASS）
bash docs/testing/round60/run-checks.sh --list      # 只列清单，exit 0
bash docs/testing/round60/run-checks.sh             # 本机实测：11/11 PASS、exit 0
# 分项
cargo test -p mir-client --bin mir-client                    # 21 passed
cargo test -p mir-client --lib audio                         # 19 passed
cargo test -p mir-client --lib ui::projectile                # 12 passed
cargo test -p mir-client --lib net::                         # 30 passed
cargo test -p mir-client --test round59_projectile_render    # 16 passed（断言值不得改）
cargo test -p mir-client --test round58_projectile_wire      # 7 passed
cargo test -p mir-render --lib sound                         # 9 passed
cargo fmt --all --check && cargo clippy -p mir-client -p mir-render --all-targets -- -D warnings
py -3 tools/docs/check_links.py                              # exit 0
git diff --check                                             # exit 0
# 树哈希（清单头部三字段 + 生成命令；清单自身 sha 只入 D4 final-manifest.json）
bash docs/testing/round60/gen-tree-hashes.sh    # 重生成 docs/testing/round60/commit-tree-hashes.txt
```

**说明**：`run-checks.sh` 的 01/02 项依赖系统临时目录；若 `TMP` 不可写，脚本已自动改指 `target/tmp`（见 P-6）。
`--list` 不写盘、不编译；出现 `SKIP:`/`PARTIAL:` 行一律判非 PASS 并使整体 exit 非 0。
