# R65-D 文档索引：单 UI 消费生命周期与快照契约

- 子单：`JJC-20261027-002-sub-ce003bd2`；主题：G1 既有单 UI 确认/回收生命周期补证；G2（known_positions 与事件表容量政策）整项移出本轮。
- 基线功能 HEAD：`3730e2be7983b1cbbef704103b05a804bfb02489`；A/B 功能变更已提交，D 仅写本轮三份白名单文档。
- 范围边界：仅本地消费契约、测试与证据索引；不改 wire、协议、服务端语义、运行时容量/LRU 或 R61–R64 冻结材料。

## 交付物与独立证据

| 项 | 文件/行数 | 测试名与实测计数 |
|---|---|---|
| A | [net.rs](../../../crates/mir-client/src/net.rs) 9,779 行（+413）；[app.rs](../../../crates/mir-client/src/app.rs) 4,435 行（+129）。[A 证据](../../../target/round65/a/attempt-1/README.md)、[A manifest](../../../target/round65/a/attempt-1/sha256-manifest.json) | lib 5/5、bin 1/1 通过；`r65_half_consumed_prefix_tracks_one_based_watermark`、`r65_lifecycle_repeat_backward_clamp_and_generation`、`r65_ack_command_is_in_game_gated_and_writes_zero_bytes`、`r65_wait_for_snapshot_waits_for_predicate_and_never_acks`、`r65_total_saturates_without_overflow_and_never_decreases`、`r65_next_projectile_ack_requires_strict_nonzero_watermark_advance`。另 r64_ 回归 lib 7、bin 1 通过。 |
| B | [round65_projectile_lifecycle.rs](../../../crates/mir-client/tests/round65_projectile_lifecycle.rs) 649 行；[B 证据](../../../target/round65/b/attempt-1/README.md)、[红证据](../../../target/round65/b/attempt-1/red-evidence.md) | 集成测试 5/5 通过：`r65_g1_1_half_consumed_prefix_through_real_125_wire`、`r65_g1_2_pre_trim_clone_is_value_independent`、`r65_g1_3_consume_half_ack_half_then_push_more_keeps_suffix`、`r65_g1_3_repeat_backward_tail_and_old_generation_acks_are_safe`、`r65_g1_3_new_generation_first_payload_resets_once_and_clears_state`。有效 no-op 红及错前缀负控均 EXIT=101。 |
| C | [刑部独立核验](round65-c-evidence.md)：74 行 / 10,269 B / SHA256 `8cf5d5848e829411eb6aa5ce193558acebc35ba9d05b65b06a13a2a4384d530a` | 独立 13 命令全 EXIT=0；10 个测试目标共 60 次测试执行，0 failed / 0 ignored；错前缀独立重放 EXIT=101，身份断言 left=1/right=3。结论 PASS-WITH-PARTIAL，必改 0。 |

## 唯一门禁记录

- [attempt-1 log](../../../target/round65/gate/attempt-1.log)：功能 HEAD `3730e2be7983b1cbbef704103b05a804bfb02489`；EXIT=0；12 项 OK / 0 FAIL；206,593 B；SHA256 `3c9f40fcf0b889bb51291ca9ba88125b80ca894ed0fefc65082e25300c64bfb3`。
- UTC：`2026-10-26T19:49:35Z`–`2026-10-26T19:53:18Z`；[meta](../../../target/round65/gate/attempt-1.meta.txt) 记录完整 HEAD、EXIT、UTC、bytes、SHA、items 与 operator。门禁后 `git diff 3730e2be7983b1cbbef704103b05a804bfb02489 -- crates` 为空。
- 此处的 PASS 仅指这次门禁记录；不代表本轮最终验收。D 未代跑门禁。

## PARTIAL 台账（原编号承接）

- 本轮：G1.2 clone 仅证明值独立，不证明生产多消费者或并发消费安全；没有全局最低水位。G2 容量政策未做。B 的 total 饱和用例直接写入 `projectile_events_total = u64::MAX` 后再走 125 wire；饱和值本身不是 wire 可达性证据。
- 通用边界：事件 backlog、clone 与 `known_positions` 默认仍可无界增长；不丢最旧只是契约/待评审，不是容量或淘汰实现。全局最低水位、事件/位置容量、LRU、服务端回收、素材/音频（含设备/Live）仍 PARTIAL。
- R61 P01–P12：P01 clone/最低水位/长期内存；P02 生产 SkipFrames/ActionFeed；P03 全局 TargetPoint/容量/失效；P04 ImageInfo 与渲染扩面；P05–P06 设备/Live 与削波（33 路 peak 22.844788）；P07 105 sfx 逐项；P08 HiddenSpanSource 全局化；P09 CWD/素材来源；P10 饱和恢复；P11–P12 缺素材与 R60 P-1–P-6 均未清零。
- R62 四项：①服务端物理回收未做；②`known_positions` 容量/LRU 未做；③生产 `owner_skip_frames` 仍 false；④R63 局部 570 时钟不外推至素材或全局效果。
- R63 P1–P7：P1 生产 SkipFrames/ActionFeed；P2 事件限容/服务端与全局回收；P3 位置缓存容量/LRU；P4 真实素材可听/渲染；P5 局部严格时钟及 R61 交接；P6 历史阶段不等于当前终态；P7 旧门禁 meta 缺 EXIT/HEAD，均继续承接。
- R64 六项：①多消费者/clone 最低水位；②事件表容量；③`known_positions` 容量/LRU；④服务端回收；⑤定向测试不等于唯一门禁；⑥R61/R62/R63 原编号 PARTIAL 延续。均未由 G1 本地补证清零。
- C 报告偏差披露：A 三份 patch.json 实际为 GB18030；B 曾覆盖旧 v1 日志，旧版不作为首败保全证据；最终证据经 C 核验。不得据此扩大结论。

## 阶段、复跑与检查

- 当前主单 `JJC-20261027-002` 仍为 Doing；尚书验收、门下确认与看板 Done 均未发生。结论仅可表述为“门禁 PASS；独立核验 PASS-WITH-PARTIAL、必改 0；最终验收待定”。
- 复跑 A（独立 target/tmp 见 A README）：`cargo test -p mir-client --lib r65_ -- --nocapture`；`cargo test -p mir-client --bin mir-client r65_ -- --nocapture`；对应 `r64_` 两命令；`cargo fmt -p mir-client -- --check`；`cargo clippy -p mir-client --all-targets -- -D warnings`。
- 复跑 B：`cargo test -p mir-client --test round65_projectile_lifecycle -- --nocapture`；`cargo test -p mir-client --test round64_projectile_ack`；`cargo test -p mir-client --lib ui::projectile::tests::`；`cargo fmt -p mir-client -- --check`；`cargo clippy -p mir-client --all-targets -- -D warnings`。
- 复跑 C：独立复跑 A/B；另跑 `cargo test -p mir-client --test round64_event_reclaim -- --nocapture`、`cargo test -p mir-client --test round63_projectile_wire -- --nocapture`、`cargo test -p mir-client --test round62_projectile_wire -- --nocapture`、`cargo fmt --all -- --check`、`cargo clippy -p mir-client --all-targets -- -D warnings`；完整清单及逐项计数见 C 证据与 `target/round65/c/attempt-1/runs.json`。以上均为复跑命令，本任务未代跑 cargo。
- 文档验收命令：`py -3 tools/docs/check_links.py`；`git diff --check`；`node -e "JSON.parse(require('fs').readFileSync('docs/testing/round65/final-manifest.json','utf8'))"`；manifest 元数据复算。唯一门禁不复跑。
