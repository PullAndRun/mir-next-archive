# R47：战士空挥 Attack1/Attack3 选招（Gate 0 + 证据）

第47轮只做**自机 Warrior、未骑乘、Spell.None** 的普通空挥本地视觉选招。
基线 `55bd709`，Crystal `0e315fe327192afe52c3d7357ddd1f5b7e26c5b8`。
不做 Attack2/4、他职业、技能、双击、坐骑/疾风、远端随机同步、300ms 伤害队列、
冷却、怪物/NPC、寻路、光照/UI/音效；最终候选通过 12 项门禁后冻结提交。

## 1. Gate 0：Crystal 与转换资产对照

Crystal `PlayerObject.cs:1055-1075` 的 default 分支：

```text
if CMain.Shift && TargetObject == null:
    Next(100) >= 20 ? Attack1 : Attack3
else:
    Attack1
```

本作把 `TargetObject == null` 严格映射为 `target_object_id.is_none()`（锁定无效 ID 也算
有目标，不回退进随机分支）。`[0,99]`：`0..19 = Attack3`，`20..99 = Attack1`；
`roll >= 100` 是非法值，返回 None（生产 RNG 只产生 0..99）。

| 项 | Crystal 逻辑（Frames.cs） | 本作扁平/game-assets 转换 | 依据 |
|---|---|---|---|
| Attack1 身体 | Start 136, Count 6, Skip 0, 100ms | Start 200, Count 6, Skip 2, 100ms | `Frames.cs:162`；`hum.wzl` 每方向 8 槽，起点实测 200 |
| Attack3 身体 | Start 232, Count 8, Skip 0, 100ms | Start 328, Count 8, Skip 0, 100ms | `Frames.cs:164`；`hum.wzl` 起点 328，**232 只是逻辑号，不是物理索引** |
| Attack1 effect | Start 168, Count 6, Skip 0, 100ms | Start 168, Count 6, Skip 2, 100ms | `Frames.cs:162` 第五～八角；`wingeffect_x.wzl` 每方向 8 槽 |
| Attack3 effect | Start 264, Count 8, Skip 0, 100ms | Start 264, Count 8, Skip 0, 100ms | `Frames.cs:164`；不能复用身体 328 |
| 帧间隔 | `PlayerObject.cs:1303-1304` FrameInterval=Frame.Interval | 入围路径 100ms；旧路径 50ms | `Frames.cs` |
| 时长 | Attack1 6×100=600ms；Attack3 8×100=800ms | 同左 | `Frames.cs` |

男女：Crystal 的性别差异在 `ArmourOffSet`/`HairOffSet`/`WingOffset`
（`PlayerObject.cs:584-587` 等），不改变 `Frames` 的 Start/Count；本作帧表矩阵对男女
相同，live 仍各抽一个角色验证素材层。阴影/武器光效的逐武器 `CWeaponEffect` 映射未移植。

### 现状偏离与收敛

`crates/mir-render/src/animation.rs` 旧实现把**所有**攻击映射到 `HUMAN_ATTACK1`
（200/6/2/**50ms**），`human_frames(Attacking)` 只有 300ms。R47 新增：

* `AttackVariant::{Attack1, Attack3}`（`mir-render::animation`）；
* `HUMAN_ATTACK1_R47 = (200,6,2,100)`、`HUMAN_ATTACK3_R47 = (328,8,0,100)`；
* `frames_for_variant` / `ActorAnimation::new_with_variant`；`ActorAnimation.attack_variant`；
* `WING_ATTACK1_R47=(168,6,2,100)`、`WING_ATTACK3_R47=(264,8,0,100)` 与
  `wing_frames_for`；Attack3 无通用挥砍 effect 映射，`draw_attack_swing` 真实降级为
  **不画**通用 `effect.wzl` 挥砍光效，不复用 Attack1 索引冒充。

旧 `None` 路径完全保留 50ms/300ms，供非入围职业/骑乘/技能/远端使用。

## 2. A1–A3 可执行方案

* **A1 选择器**：`crates/mir-client/src/attack.rs` 纯函数
  `select_attack_variant(context, roll)`；单测硬编码 0/19/20/99/100，枚举
  class×Shift×target×Spell.None/非None×骑乘；只有入围 Warrior/未骑乘/Spell.None 的
  Shift+无目标使用 roll，其余固定 Attack1 或 None。生产 `local_attack_command` 只在
  Crystal 的 `Shift && TargetObject == null` 随机分支调一次 `AttackRng::next_roll`；
  锁定/普通 Space/非入围不消耗随机数，变体写入 `SelfPlayer.attack_variant` 后
  render/snapshot 不再抽取。
* **A2 帧表/资产**：`animation.rs` 单测锁 start/count/skip/100ms、逻辑 vs 转换索引、
  8 方向 × 男女 × 6/8 帧矩阵、0/99/100/599/600/799/800/900ms；entities 单测锁
  body/weapon/hair/shield/wing 同一 variant 帧契约。资源不足时 renderer 的既有
  缺层回退（`ensure_image`/占位）保持不变，不把站立帧冒充攻击帧。
* **A3 生产链**：`NetCommand::Attack { direction, variant }`（variant 不进入协议）→
  `ClientApp::send`/`NetClient::handle_command` 本地动作 → `SelfPlayer.set_attack_action`
  → `EntityView.attack_variant` → `ActorAnimation::from_view` → renderer。
  `finished_action`/`tick_animations`/`entity_views` 使用 variant 时长：Attack1 600ms、
  Attack3 800ms，300ms 绝不再提前 Standing；受击/死亡/转身/删除/切图/移动/连续新攻击
  清旧 variant，同坐标快照不重启。回归 R45 camera12、R46 remote96/live。

### X1 复核修正（2026-10-24）

* **X1-01 动作时钟**：本地动作只在**本地命令应用时刻** `receive_now_ms` 建立
  `SelfPlayer.set_attack_action(variant, receive_now_ms)`；发送时刻只留在 `send()` 的
  测试记录里，不进入动作进度。结束条件固定为
  `receive_now_ms - action_started_ms >= variant_duration`（600/800ms）。同一坐标的
  迟到 `UserLocation`（200/600ms）只确认位置，不重启动动作；因此 200/600ms 传输延迟
  分别还剩 400/0ms，不把网络延迟算成动作进度。同一动作的 render/snapshot 共用
  `action_started_ms`，不再出现 action 已 Standing、motion 还在走的双时钟。
  单测 `a3_local_attack_clock_is_not_shifted_by_late_server_ack` 在发包后
  200/600ms 注入同坐标 `UserLocation`，断言动作仍从发包时刻起算、到
  600/800ms 结束，绝不顺延到“回包后再播一遍”。
* **X1-02 真实素材证据**：live test 的 `Attack` 步骤与 observer 断言仍是**严格观察者**
  （observer 的 `ObjectAttack` 计数和目标实体 `Attacking` 必须成立），才加载
  `game-assets/maps/0.mnm`；25 帧/4 段（含 600/800ms 终点）写入 manifest、日志和 PNG
  字节哈希，不拿无素材占位图冒充。
* **X1-03 种子**：生产种子为 `epoch_ms: u64` + 进程 ID 的全 64 位 splitmix64 混合
  （`seed_from_epoch_and_process`），不再 `u32::try_from(epoch_ms).unwrap_or(1)` 截断；
  单测锁相邻毫秒、不同进程、低 32 位相同/高 32 位不同的 epoch 均不碰撞，且 `AttackRng`
  每次动作最多一次 `next_roll`。
* **X1-04 性能证据**：baseline/new 均为真实构建；两侧都显式 `black_box` 输入，
  防止常量折叠；原始 `batches_ns`、批次大小/次数、总计、阈值公式全部落 JSON，可从
  `baseline-raw.json`/`perf.json` 复算；无每帧随机、无全实体复制。
* **锁定目标**：`target_object_id.is_some()` 即算有目标（即使 ID 无效/不可达），固定
  Attack1，只有 Shift+无目标才抽 Attack3；测试 `a1_locked_or_invalid_target_is_not_none`
  锁住该分支。

### 300ms 统一结束冲突处理

旧 `finished_action` 用 `frames_for(Human, Attacking)` 的 300ms 统一结束；R47 给它增加
`attack_variant` 参数，改查 `frames_for_variant`。因此服务端 300ms 伤害队列不参与本地
视觉时长：Attack3 到 800ms 才结束；客户端不改服务端/协议/数值队列。

## 3. 文件与证据

| 文件 | 内容 |
|---|---|
| `crates/mir-client/src/attack.rs` | 选招纯函数 + RNG（仅随机分支每动作一次） |
| `crates/mir-client/src/net.rs` | `attack_variant` 字段/本地动作/时长/tick/打断 |
| `crates/mir-client/src/app.rs` | Shift/Space、锁定路径、本地命令入口 |
| `crates/mir-client/src/ui/mod.rs` | `NetCommand::Attack` 结构变体 |
| `crates/mir-render/src/animation.rs` | variant 帧表/逻辑 vs 转换 |
| `crates/mir-render/src/entities.rs` | 层契约/Attack3 挥砍光效降级 |
| `crates/mir-client/tests/attack_variant.rs` | A1/A3 确定性 |
| `crates/mir-client/tests/attack_variant_live.rs` | 两真实 NetClient + MemoryStore |
| `frame-table.csv` | 112 行（2 变体 × 8 方向 × 6/8 帧，逻辑/转换/effect） |
| `live-manifest.json` / `live-run.log` | 真实 game-assets 25 样本（默认散装 + release pack 校验） |
| `analyze-usage-attack3.log` / `attack3-asset-plan.json` / `attack3-unpack.log` | 引用分析（67 库）、`hum` 补齐计划 296 输出/35,520 PNG、344..351 输入输出清单 |
| `attack3-pack-rebuild.log` | 从默认 `game-assets` 重建 release `.mnp` 的输出 |
| `deterministic-tests.log` / `full-suite-tests.log` | 确定性测试与全量回归输出 |
| `perf.json` / `perf-run.log` | release 性能证据（1000 次动作建立+帧决策） |
| `baseline-raw.json` / `baseline-perf.log` / `baseline-bench.rs` | 55bd709 独立 worktree 基线采样/命令/源码 |

## 4. live 证据摘要

默认构建：
`cargo test -p mir-client --test attack_variant_live -- --nocapture`。
**1 passed**，两真实 `NetClient` + `MemoryStore`；证据走 `game-assets`
默认散装 `gfx`（manifest `assets_source: "unpacked"`），不使用 `resource/data`。
legacy 交叉校验另跑
`MIR2_TEST_LEGACY=1 cargo test -p mir-client --features mir-render/legacy --test attack_variant_live -- --nocapture`，
只在此时读完整 `hum*.wzl` 对照，不作为默认证据来源。
female/male 两个 Warrior 互为观察者：

* female Attack1 Down（6 帧+600ms 终点）、Attack1 Right（2 帧，用于像素对照）、
  Attack3 Right（8 帧+800ms 终点）；male Attack1 Right（6 帧+终点）。
* 每次 `NetCommand::Attack { variant }` 只发一次既有 `C.Attack`；观察者
  `ObjectAttack` 计数递增且目标实体 `Attacking`。
* 默认构建（无 `mir-render/legacy`）真实 `game-assets/maps/0.mnm` 地图 +
  `game-assets/gfx` 散装实体图库的 PNG；manifest 顶层含 actors/class/gender、协议、
  `asset_mode`/`assets_source`（必须为 `unpacked`）/`release_pack_check`（release 包
  hum/hum2 的 Attack3 344..351 逐帧检查）、
  `batch_id`/`generated_at_unix_ms`；每条样本含 variant/source_started/elapsed/
  `logical_index`/`attack_frame_index`/body 层 `actual_index`/各层帧/墙钟/
  `png_byte_hash`。每个 `steps[]` 另含 `sent_wall_ms`、`command_enqueued_wall_ms`、
  `attacker_ack_wall_ms`、`send_to_ack_ms`、`action_started_ms`，把发包、命令入队、
  回包和动作起点分开；协议没有时间戳，动作进度只取 `action_started_ms..now_ms`。
  **不写** manifest 中不存在的 shift/target/Spell 字段；注入 roll
  只出现在 `steps[].injected_roll`。
* 同 female/Right/装备/背景，Attack1 Right 首帧逻辑 148 → body 层 216；
  Attack3 Right 首帧逻辑 248 → body 层 344；`pixel_hash` 不同（像素级证明选到不同素材）。
* `actual_index` 是实际 body 层渲染帧（含外观块基址，Standing Down/Right 为
  32/16，male Right 为 16）；`attack_frame_index` 是扁平攻击序列帧（216/344），
  `logical_index` 是 Crystal 136/232 逻辑序列帧。三者分列，禁止混用。
* `build/evidence/attack-variants/` 与 `docs/testing/attack-variants/` 的
  `live-manifest.json` 字节一致；25 张 PNG 的 `png_byte_hash` 由测试落盘后读回字节计算，
  `live-run.log` 头部的 `batch_id` 与 manifest 的 `generated_at_unix_ms` 同批。
* 标准角色无武器/无翅膀时只有 body/hair 层；武器/盾/翅膀层由 `entities` 单测锁帧。
  Attack3 的通用挥砍光效降级为无，manifest/docs 记录，不冒充完整对齐。

## 5. 性能（预算与实测）

release、同机；baseline 为 **55bd709 独立 worktree 真实构建**（同一份
`baseline-bench.rs`，`black_box` 防止常量折叠），new 为当前 R47。场景：1000 次
「攻击动作建立 + 帧决策」，10 批×100 次；每次在真实 `START_MS` 上取
`[0,99,100,299,300,499,500,599,600,799,800]` 共 11 个采样时刻（逐项与
`perf.json`/`baseline-raw.json` 的 `offsets_ms` 一致，没有 700ms，也没有 11 个之外的）。
baseline 只做旧 Attack1/50ms 动画建立+取帧；new 分相测量 `select_attack_variant`、
`new_with_variant`、`frame_at`，以及三者的端到端。

```text
baseline batch p95 = 47.0 ns/action（median 27.0，total 28600 ns）
new      batch p95 = selection 2.0 / build 23.0 / frames 67.0 / end_to_end 67.0 ns/action
固定预算（测量前）：new end_to_end p95 ≤ baseline p95×1.40 + 1000ns = 1065.8ns
实测 ratio = 1.43 → 通过（仍低于 1065.8ns 固定预算）
```

测的是 10 个「每批 100 次动作」的批耗时数组，`p95` 按该数组计算；不是单帧
指令级基准，`end_to_end` 与分相之和也不要求逐项相加相等（内联/批间抖动）。
微秒以下计时噪声很大，**不据此声称提速**；基线/新侧原始 `batches_ns` 在
`baseline-raw.json` / `perf.json`，可用其中 total/sum 复算。无每帧随机（选招只在
动作建立时一次）、无无界状态、无全实体复制。

## 6. 最终验收冻结（2026-10-24）

* 最终门禁：`pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`，12/12 全绿，
  耗时 255.76 秒；完整输出 `final-gate.log`，结果 `final-gate-result.json`，
  启动前 23 个源码/测试/工具 blob 见 `final-candidate.json`；结束后逐项复核无变化。
  原始日志为 `target/gate-r47-20261024-095531.log`；前一截断门禁不计通过。
* 本次 workspace 门禁重跑了 live，**最终批次为 `1792807006205`**，不是御史台
  t20 先前审核的 `1792806118685`。后者为历史；源码候选不变，不能混称同次运行。
  当前 `live-manifest.json`/`live-run.log`、25 张 PNG 与 build 目录逐字节一致。
* 最终 manifest SHA256：`310d549196195dcaa732895aaa9ac65612375e65c1bbb10469b67e95b953ba5a`；
  全部 PNG 的测试 FNV 与独立 SHA256、CSV/perf/日志/候选哈希见 `final-freeze.json`。
  `live-evidence-check.log` 已更新为此批次。其余 legacy/perf/确定性输出是各自独立
  运行的历史证据，不声称与本次 live 共用墙钟批次。
* 默认素材为 `asset_mode=dev` / `assets_source=unpacked`；release packs=46，
  hum/hum2 Attack3 344..351 校验通过。`game-assets` 为 gitignored，不包含于源码
  提交；发布/新环境必须按资产计划携带或重建默认散装素材与 `.mnp`，不能仅拉代码。
* 女性 Attack3 344..351 八活动帧 + 800ms Standing、男性 Attack1 六帧 + 600ms
  Standing、112 行帧表、11 个性能采样时刻及既定预算均独立核对通过。

## 7. 限制

* 窗口四类边界/FPS/录像未测；不声称。
* 只做本地视觉变体，不做远端随机同步；旁观者只证服务器链路。
* 本机 Rust 进程写默认 `%TEMP%` 被 ACL 拒绝（`os error 5`，非代码断言失败）；全量回归
  用 `TEMP/TMP=target/tmp-suite` 指向工作区临时目录运行，原始输出见
  `full-suite-tests.log`。不把环境失败写成通过。
* 默认素材已补齐：`game-assets/gfx/sprites/player/body-000` 与 `body-024` 均含
  Attack3 344..351（8/8），默认 `auto → dev` 启动路径不再回退 Standing；发布
  `game-assets/packs` 已重建，live manifest 的 `release_pack_check` 逐帧验证 pack
  路径的 344..351。补充来源是 `hum.wzl` 前 296 帧/7 个身体层（计划 296 输出、
  35,520 PNG，`analyze-usage-attack3.log`/`attack3-asset-plan.json`）；其余 67 库
  未使用的 27,810 个输出没有写入，避免把全世界素材伪装成“已补齐”。
  `--features mir-render/legacy` 只用于 `resource/data` 完整 WZL
  交叉校验（需同时 `MIR2_TEST_LEGACY=1`），不再作为默认证据来源。
* 默认 live 通过记录在 `live-run.log`；legacy 交叉校验另跑并保存
  `legacy-live-run.log`；全量默认回归在 `full-suite-tests.log`。若只跑
  legacy 而没跑默认路径，不能算本项通过。
* 逐武器 `CWeaponEffect`/阴影层未移植；Attack3 通用挥砍光效降级。
* 女性/男性素材偏移（ArmourOffSet）沿用现有渲染路径，本轮未改。
* `build/evidence/attack-variants/` 为 gitignore 临时 PNG；提交证据以本目录
  manifest/CSV/日志为准。
