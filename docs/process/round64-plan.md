# R64 中书规划：125 事件表确认水位回收

## 一、决策、基线与选题

- 主任务：`JJC-20261027-001`「第64轮：投射物事件表安全裁剪与确认水位」。本阶段只规划、建主单、转 Menxia 并 validate；不建子单、不改 crates、不 commit/push、不运行 cargo 构建与 `tools/dev/ci.ps1`。
- 基线：只读核实 `HEAD=origin/main=6f82f3b`；工作树仅 R63 遗留两份未跟踪冻结 JSON，本规划不触碰。
- 选题：`docs/roadmap.md:236-245` 候选 2 的**客户端可证部分**——125 事件表按消费者确认水位做安全前缀回收（P2/P01 的可闭环子集）；配套文档批登记 `known_positions` 容量/LRU 与服务端回收边界契约。
- 不选候选 1（生产 SkipFrames）：只读确认 Rust 无 ActionFeed 等价位（`pending_action|action_queue|QueuedAction` 全仓 0 命中；`net.rs:1049-1063,1149-1153` 收到动作即覆盖并重置开始时间），动作不等播放完成即应用，`SkipFrames=false` 在当前架构下语义自洽；要接线须先建 FIFO 与动画生命周期（R63 已排除），单轮不可闭环。
- 不选候选 3/4/5：掉落表/护身符需 `mir2_dev` 数据与 Crystal 服务端 DB（本仓无表源），550 场景依赖不入库的 `game-assets/`，索敌切换缺可复现红侧；均留后续并维持 PARTIAL。

## 二、事实锚点

| 项 | Crystal 锚点 | Rust 现状与缺口 |
|---|---|---|
| 125 产生与字段 | `Client/MirScenes/GameScene.cs:4687-4715` 每条 125 立即生成；`Shared/ServerPackets.cs:3560-3578` 仅 Spell/Source/Destination | `crates/mir-client/src/net.rs:7296-7312` 接受即 append，无上限、无删除 |
| 生命周期 | `Client/MirObjects/Effect.cs:38-48,81-96`（R63 已核：frame≥10 移除，无永久队列） | 125 视图到达即删；**事件表**无删除路径，`net.rs:2760-2769` 明示仅整份重建清表 |
| 有界对照 | — | `net.rs:165-167,7267-7269,7282-7285`：damage 32、effect 16 有界；仅 125 无界 |
| 快照成本 | — | `net.rs:4524-4526` 每次 `NetEvent::Snapshot` clone 整表；`app.rs:2711-2739` 整份替换；表随会话单调增长 |
| 消费游标 | — | `ui/projectile.rs:566` `consumed`；`:668-731` `consume` 的 `dropped=total-len` 兼容路径（`:708-723`）、`:711` rebuilt、`:694-703` 跨代复位 |
| UI 同步点 | — | `app.rs:2756` `live.ui.sync(&live.snapshot)`；`ui/mod.rs:1280,1573` |
| 命令通道 | — | `net.rs:307` `NetCommand`、`:3767` send、`:3890-3946` 会话循环与 6 处 `send_snapshot` |
| 会话代号 | — | `net.rs:2778-2786` generation、`:2990-2994` new_session、`:2924-2926` 重建归零；`ui/projectile.rs:693` |
| SkipFrames（非目标） | `Client/MirObjects/PlayerObject.cs:750,966-967`、`MonsterObject.cs:346,528-529` | `ui/projectile.rs:546-556` 生产恒 false，PARTIAL 保留 |
| TargetPoint 缓存（非目标） | `Client/MirObjects/PlayerObject.cs:3546-3556` | `ui/projectile.rs:735-739` 每帧全量 insert，无容量/LRU |

## 三、目标与非目标

- G1（A）：net 侧回收契约——`WorldSnapshot::trim_projectile_events_through(through)`，只删绝对序号 ≤ `through` 的前缀，`total` 不减，越界夹取，`reclaimed=total-len`。
- G2（B）：UI 确认水位与接线——`consumed` 绝对水位上报、仅在推进时发进程内 ack、`generation` 过期 ack 忽略；慢消费者不丢未确认事件。
- G3（D）：契约文档——`known_positions` 容量/LRU 未决项、服务端回收边界（Crystal 无 ack 字段，不发明协议）、PARTIAL 台账。
- 非目标：不改 wire/协议/服务端；不建 ActionFeed/SkipFrames；不实现 `known_positions` 任意上限或 LRU；不做音频/素材/渲染；不清零 R61 P01–P12；不改 `consume` 既有语义除非退门下。

## 四、子任务 DAG（严格 A→B→C→D）

尚书准入后建子单；每单先落 test-only 红（见 §五），再实现。白名单外零改动。

| 单 | 责任 | 交付契约（文件 + 测试名） | 验收命令 | 禁止项 |
|---|---|---|---|---|
| A | 兵部 | `net.rs`：`trim_projectile_events_through`、`reclaimed()`、`NetCommand::AckProjectileEvents{generation,through}`、会话循环 send 前应用、`to_packet` 无包分支；`tests/round64_event_reclaim.rs`：`r64_trim_acked_prefix_keeps_absolute_total`、`r64_trim_noop_below_base_and_clamps_above_total`、`r64_stale_generation_ack_is_ignored`、`r64_ack_command_emits_no_client_packet`、`r64_push_after_trim_keeps_cursor_arithmetic` | `cargo test -p mir-client --lib net::tests::r64_`；`cargo test -p mir-client --test round64_event_reclaim -- --nocapture` | 不改 `consume`、不改 wire 编解码、不删旧断言、不加依赖 |
| B | 工部 | `ui/projectile.rs`：`consumed_abs_seq()`；`ui/mod.rs`：`UiState::projectile_ack()`；`app.rs:2756` 同步后仅水位推进时 `self.net.send(AckProjectileEvents)` 并记录 last_acked；`tests/round64_projectile_ack.rs`：`r64_ack_sent_only_when_watermark_advances`、`r64_acked_prefix_reclaimed_without_losing_unacked`、`r64_reconnect_generation_reset_after_trim`、`r64_trimmed_snapshot_does_not_replay` | `cargo test -p mir-client --lib ui::projectile::tests::r64_`；`cargo test -p mir-client --test round64_projectile_ack -- --nocapture` | 不改 `net.rs`（A 已交 API）、不改 125 时钟/渲染、不每帧重复 ack |
| C | 刑部 | `target/round64/c/attempt-1/**` 仅证据；独立读代码、复跑 A/B 定向与历史 targets、核对 wire 零差异、`reclaimed` 与游标口径、禁称扫描 | 独立复跑 §五命令 + `cargo test -p mir-client --test round59_projectile_render --test round61_projectile_queue --test round62_projectile_wire --test round63_projectile_wire`；`clippy -D warnings`、`fmt --check`、`git diff --check` | 不改实现/测试、不跑 `ci.ps1` 冒充门禁、不把本地绿称门禁 |
| D | 礼部 | `docs/testing/round64/**`（README/manifest/hash/run-checks）、`docs/process/round64-shangshu-*.md`、`round64-menxia-*.md`、`docs/roadmap.md` §十三/新增 §十四、`docs/client.md`；契约：容量/LRU 未决项、服务端边界、PARTIAL、supersede/旧测试映射 | `py -3 tools/docs/check_links.py`；`git diff --check`；JSON/SHA 复算；文件 ≤1MiB、UTF-8 无 BOM/LF | 不预写 PASS、不覆盖 R59–R63 冻结证据、不写任意容量常数 |

## 五、红→绿与证据要求

1. 红：A/B 先在白名单内落 **test-only + 最小 no-op 桩**（`trim` 返回 0 不删；`consumed_abs_seq` 返回 0），运行新过滤器取得 **assert 失败**；编译错误、panic、`unimplemented` 不算红，须在报告中注明桩与断言行。
2. 绿：A 实现 trim、B 接线后同一测试文件不改断言通过；记录 HEAD、UTC 起止、命令、exit、关键断言输出。
3. C 独立复跑 A/B 定向命令与四历史 targets，核对红日志来自 test-only 树、绿来自功能树；任何 SKIP→PARTIAL 且总结果非全 PASS，ignored 列旧/新数量与位置。
4. 全量门禁仅 Lead 在功能提交 HEAD 执行 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale` 一次；失败保留原日志、另编号 attempt-N，不覆盖失败证据。

定向命令（实施后执行；本规划不执行）：

```bash
cargo test -p mir-client --lib net::tests::r64_
cargo test -p mir-client --lib ui::projectile::tests::r64_
cargo test -p mir-client --test round64_event_reclaim -- --nocapture
cargo test -p mir-client --test round64_projectile_ack -- --nocapture
cargo test -p mir-client --test round59_projectile_render --test round61_projectile_queue --test round62_projectile_wire --test round63_projectile_wire
cargo clippy -p mir-client --all-targets -- -D warnings
cargo fmt --all -- --check
py -3 tools/docs/check_links.py
git diff --check
```

## 六、数据流与边界

- wire 不变：`S.ObjectProjectile`(125) 仍 Spell/Source/Destination 三字段；ack 是进程内 `NetCommand`，A 必须以测试钉死「不产生 `ClientPacket`」。
- 快照语义：`total` 会话内单调不减；`len` 可因回收下降；`reclaimed=total-len`；`events[i]` 绝对序号=`reclaimed+i`；整份重建时 `total/len/reclaimed` 归零并换 generation（`net.rs:2924-2926,2990-2994`）。
- 消费者约束：单个 `UiState`/`Projectiles`；只在已消费后 ack，`through ≤ consumed`；过期 generation 的 ack 丢弃；重连后新代从 0 开始，不重放已回收事件。
- 兼容：手工/旧形态快照 `total==0` 时 `reclaimed=0`，`consume` 的 `dropped` 路径与 R59–R63 断言不变；不承诺事件表有界上限（慢消费者仍可增长）。

## 七、风险与回滚

- R1 提前回收丢未确认事件（慢消费者、多消费者、旧 ack）：以「单消费者 + watermark ≤ consumed + generation 校验 + 慢消费者测试」约束；C 独立核对。
- R2 把「回收」误报为「丢失」或「已限容」：报告必须分列 `reclaimed`、未确认 `len`、`total`；禁称有界。
- R3 命令通道/主循环改动回归：A 只加分发分支与无包断言，B 只在 `app.rs:2756` 同步点发命令；最终由 Lead 唯一门禁兜底。
- 回滚：纯客户端、无 wire/数据迁移；revert 功能提交即恢复 append-only；文档批独立提交可单独回退。

## 八、最终验收与 PARTIAL 判定

- 主功能批**仅**证明「客户端单消费者、按确认水位安全前缀回收」；必须登记 PARTIAL：`known_positions` 容量/LRU 未实现（G3 仅契约）、服务端回收协议不存在且未发明、多消费者/克隆快照全局最低水位未证、SkipFrames/ActionFeed（P1）未接、570 素材与可听（P4）、R61 P01–P12 其余项按原编号承接。
- 禁称：「事件表已限容/有界/无内存增长」「已物理回收全场景」「生产 SkipFrames 已接」「R64 已 PASS」「R61 十二项已清零」；红绿与门禁结论只能引用实际命令输出。
- 计划本身验收：UTF-8 无 BOM、LF、≤200 行、单文件交付；CLI create/state/validate 原始输出由 Lead 核对，不另落 target 文件。

## 九、提交与门禁顺序

1. 门下评审本计划 → 尚书准入派 A→B→C；C 完成定向测试与独立审查后交 Lead。
2. Lead 按白名单提交功能批（含新旧测试），在功能 HEAD 跑唯一 `ci.ps1`，保存 attempt 日志/exit/项数/SHA256。
3. 通过后 D 按事实写文档批；Lead 检查 links/diff/hash 后提交；独立门下终审 → 指定 confirm_by → Lead 决定 push。
4. 门禁后任何功能/测试改动使旧 HEAD 门禁失效，必须重跑；纯文档事实可后提交。

## 十、待门下裁决

1. 是否批准「确认水位 + 安全前缀回收」为主功能批；服务端协议不发明、仅交边界契约。
2. 是否接受 A/B 以 no-op 桩取得行为 assert 红；若要求真实基线红，则二单须从 `6f82f3b` 重建隔离基线，证据成本另计。
3. `known_positions` 是否允许本轮只交契约不实现（Crystal 无 LRU/容量依据，任意上限会丢可信落点）。
4. 是否允许功能批触及 `net.rs` 的 `NetCommand`/会话循环（R63 白名单曾排除）；若否，回收退化为纯文档、主功能批另选。

当前为中书规划成稿；审批、红绿、门禁与终审结论不由本计划预写。
