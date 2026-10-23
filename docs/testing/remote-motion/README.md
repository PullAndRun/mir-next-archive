# R46：其他玩家 Walk/Run 移动插值（证据与覆盖边界）

第46轮只做**已知非自机 Player** 的 `S.ObjectWalk` / `S.ObjectRun` 普通走/跑插值。
怪物/NPC、攻击 2/3/4、坐骑/Sprint/Pushed/Jump/Dash、预测/补偿队列、寻路/锁定、
光照/UI/音效均不在范围。本轮为**提交前候选**（父提交 `85bfc21`）；D2 证据冻结与哈希见 §5
及 `frozen-evidence.sha256`；Crystal 参考 `github/Crystal` HEAD `0e315fe327192afe52c3d7357ddd1f5b7e26c5b8`。

## 1. 行为与公式

权威坐标：`S.ObjectWalk`/`S.ObjectRun` 到达时，`WorldEntity.position` **立即**落到包里的
`location`；只有「已知非自机 Player + 动作步数（Walk=1、Run=2）+ 方向与位移一致」才建立
`RemoteMotion`（`from` = 上一权威位置，`to` = 包落点，`started_ms` = 客户端应用该包的
`now_ms`，动作与 motion 共用）。

绘制（R46 定稿，使用 **from + 推进量**，不是相对终点的剩余量）：

```text
screen = world_to_screen(self_camera_center, remote.from)
       + self_world_offset
       + remote_advance(elapsed)
remote_advance = -Crystal OffSetMove   // 从 from 指向 to，走右/下为正
```

`EntityView.position = remote.from`，`EntityView.pixel_offset = remote_advance`；
不合法的 ObjectWalk/ObjectRun（步数/方向不符、跳变）只落权威包点、**不建插值**，
动作字段仍按包播放（保持既有动画语义；位置 snap，不猜路径）。**未知对象**则明确
**不创建实体、不建 motion**，等 `S.ObjectPlayer`（或对象发现包）建立实体后，后续
ObjectWalk/ObjectRun 才走权威落点；不存在“先更新后创建”的隐式补对象。
相机中心/地形只由自机决定，**绝不叠加他者偏移**。排序用 R46 新增的
`EntityView.sort_y`：普通实体 = `position.y`；走/跑远端 = `max(from.y,to.y)`，
对应 Crystal `DrawY`（`PlayerObject.cs:845`），不重写既有排序算法。

Crystal 出处：`PlayerObject.cs:786-795`（i/Movement）、`:797-834`（Count/Index/八方向/
截断后 `%2`）、`:845`（DrawY）、`:847-854`（其他玩家 `DrawLocation += User.OffSetMove`
再 `-=` 自身 `OffSetMove`）；`Frames.cs:158-159`（普通 Walk/Run = 6 帧 × 100ms）。

## 2. 文件清单

| 文件 | 内容 | 入库 |
|---|---|---|
| `crates/mir-client/src/camera.rs` | `RemoteMotion`（有效性/同钟/advance_at）；`prepare_frame` 自机路径不变 | ✅ |
| `crates/mir-client/src/net.rs` | `remote_motions` 状态；`apply_remote_step`（ObjectWalk/Run）；`entity_views` 每帧套远端 offset；`tick_animations` 到期/死亡清理；ObjectRemove/Turn/Attack/Struck/Died/Hidden/MapChanged/重连清理；`upsert_entity` 同 ID 重建清理 | ✅ |
| `crates/mir-render/src/entities.rs` | `EntityView.sort_y`；`render_at_offset` 按 `(sort_y, position.x)` 排序（DrawY 语义） | ✅ |
| `crates/mir-client/src/app.rs` | 断线/重连/切图同时清 `remote_motions`；render 仍只调用 `camera::prepare_frame` | ✅ |
| `crates/mir-client/tests/remote_motion.rs` | M1/M2/M3/M4 确定性测试 + 状态/回归/排序冒烟 + `#[ignore]` 性能测试 | ✅ |
| `crates/mir-client/tests/remote_motion_live.rs` | 两个真实 `NetClient` + `MemoryStore` 的 live 证据 | ✅ |
| [`frame-comparison.csv`](./frame-comparison.csv) | 96 行（8方向 × Walk/Run × 6帧）独立二维参考期望 | ✅ |
| [`deterministic-tests.log`](./deterministic-tests.log) | M1/M2/M3/M4 确定性测试真实输出（10 passed, 1 ignored） | ✅ |
| [`parallel-tests.log`](./parallel-tests.log) | `cargo test -p mir-client -p mir-render` 并行全量真实输出（exit 0） | ✅ |
| [`live-run.log`](./live-run.log) | live 测试真实输出 | ✅ |
| [`live-manifest.json`](./live-manifest.json) | 早期 live manifest 副本（22 样本，历史对照，非 D2 提交批次） | ✅ |
| [`frozen/manifest.json`](./frozen/manifest.json) | **D2 提交批次 manifest**（最终门禁当次 live 重跑；22 样本，22 张被引用 PNG） | ✅ |
| [`frozen-evidence.sha256`](./frozen-evidence.sha256) | **D2 冻结清单**：manifest（build 原路径 + docs 副本）+ 22 张被引用 PNG 的 SHA256（24 条） | ✅ |
| [`final-gate-log.sha256`](./final-gate-log.sha256) | 同批最终门禁日志的 SHA256 与本地冻结副本路径（日志本地不入库） | ✅ |
| [`perf.json`](./perf.json) | M4 性能证据（1/50/200 远端 × 1000 帧，含 85bfc21 worktree 基线） | ✅ |
| [`perf-run.log`](./perf-run.log) | M4 新增侧真实输出（与 `perf.json` 同一 baseline JSON） | ✅ |
| [`baseline-perf.log`](./baseline-perf.log) | 85bfc21 worktree 基线命令输出 | ✅ |
| [`baseline-raw.json`](./baseline-raw.json) | 85bfc21 原始采样（每档 1000 个 `times_ns`） | ✅ |
| [`baseline-bench.rs`](./baseline-bench.rs) | 可复制进 85bfc21 worktree 的 baseline 基准源码 | ✅ |
| `build/evidence/remote-motion/` | live PNG + 原始 manifest（临时） | ❌ `.gitignore`（门禁重跑会覆盖；提交以 D2 冻结副本 + 哈希为准） |

## 3. 门下令 M1–M4 对应证据

| 项 | 测试/证据 | 结果 |
|---|---|---|
| M1 坐标量/符号 | `m1_north_south_first_frame_and_600ms_endpoint_use_from_and_advance`：向北首帧 `(0,-6)`、600ms 终点 `(0,-32)`；向南 `(0,6)`/`(0,32)`；同时断言 `pixel_offset` 与 `camera::world_offset` 反号 | passed |
| M2 同钟/延迟 | `m2_delayed_packets_do_not_count_transmission_time_as_action_progress`：delay 200/600ms，动作与 motion `started_ms == receive_ms`，采样 `receive+0/100/300/500/599/600`。`--nocapture` 记录 `send=2000000 receive=2000200/2000600 motion_started=同 receive` | passed |
| M3 96 组合 | `m3_all_96_direction_action_frame_combinations_match_independent_table` 使用文件内硬编码 `REMOTE_ADVANCE`（不由生产 helper 生成期望），循环 8×2×6；`m3_boundary_0_99_100_300_500_599_600_700ms` 覆盖边界 | passed |
| M4 状态有界 | `m4_state_is_bounded_and_has_no_residue`：10 远端 → 删一半 → 全到期；同 ID 反复创建/删除 50 次；死亡/改类型；默认快照重连 | passed |
| M4 性能 | `m4_perf_1_50_200_remotes_1000_frames_baseline_vs_new`（`--ignored`，release）：1/50/200 远端各 1000 帧，读取 85bfc21 worktree 原始 JSON；固定退化判定 p95 ≤ baseline×1.40 + 500ns | passed（见 `perf.json` / `perf-run.log`） |
| 打断/非法/未知/ID 复用 | `interruptions_invalid_unknown_and_id_reuse_clear_state` | passed |
| 双远端隔离 | `two_remote_players_are_isolated` | passed |
| 自机静止/移动回归 | `self_static_and_moving_regressions_keep_anchors_and_global_offset`：自机 anchor/global offset 与远端 screen 分离 | passed |
| 排序冒烟 | `sorting_smoke_moving_players_cross_a_standing_entity_once`：上下行穿站立实体，各出现一次，`sort_y=max(from.y,to.y)` | passed |

性能原始数据（release，同机；baseline 为**独立 worktree checkout 85bfc21 的 release 构建**；
原始 1000 点 `times_ns` 仅归档在 `baseline-raw.json`（baseline 侧）；新增侧只归档汇总
`perf.json` / `perf-run.log`，不宣称新侧原始采样已入库）：

```text
remotes=1    baseline median/p95 = 0.1/0.2 µs    new = 0.1/0.1 µs
remotes=50   baseline median/p95 = 1.2/2.1 µs    new = 1.4/1.4 µs
remotes=200  baseline median/p95 = 4.8/7.9 µs    new = 6.0/6.3 µs
```

测量前固定阈值：`new p95 ≤ baseline p95 × 1.40 + 500ns`（全过；按 +40% 写死，
不是 30%）。本次归档的 200 档观测 ratio=0.80；同机重复批次 baseline 5.3–6.3µs、
new 6.0–8µs，ratio 约 1.0–1.3，均在预算内。**不能据此声称普遍提速/低于 30%**——
微秒级计时噪声明显，阈值只用于防退化，不把噪声写成收益。

baseline 命令：
```powershell
git worktree add target/baseline-85bfc21 85bfc21
Copy-Item docs/testing/remote-motion/baseline-bench.rs target/baseline-85bfc21/crates/mir-client/tests/remote_perf_baseline.rs
$env:R46_PERF_OUT="$pwd\target\remote-motion-baseline.json"
Set-Location target/baseline-85bfc21
cargo test --release -p mir-client --test remote_perf_baseline -- --ignored --nocapture
```
新增侧跑 `remote_motion` 的 `m4_perf`，它读取该 baseline 原始 JSON 再写
`target/remote-motion-perf.json`。两文件（`perf-run.log` / `perf.json`）由同一次
baseline JSON 生成，数值一致；`baseline-raw.json` 是 85bfc21 的原始采样。
`perf.json` 记录 baseline commit full `85bfc21befae1725d73982afedabefeaca09f110`
及 4 个基线源文件 blob（net/camera/entities/app），可复核构建来源。
每帧只构造 `Vec<EntityView>`（既有行为），没有复制 `WorldSnapshot.entities` /
完整实体集合；状态表只含有效远端 motion。

## 4. Live 证据（两个真实 NetClient + MemoryStore）

命令：

```powershell
cargo test -p mir-client --test remote_motion_live -- --nocapture
```

结果：**1 passed / 0 failed，16.33s**，目标 A（`TargetHero`, id 7）真实走 2 步 + 跑 2 格，
观察者 B（`Observer`, id 8）收到 `ObjectWalk`/`ObjectRun`，并在 A 的 RemoteMotion
仍活动时自己也走一步：

```text
walk 330,270 -> 330,271 (Down, B ObjectWalk)
walk 330,271 -> 331,272 (DownRight, B ObjectWalk)
run  331,272 -> 333,274 (DownRight, B ObjectRun)
B 并发走一步 330,270 -> 331,270（A 的 run motion 仍活动）
22 样本（2 Walk×7 帧含 600ms 落点 + 1 Run×7 帧含 600ms 落点 + 并发 1 帧）
```

- 地形：真实 `game-assets/maps/0.mnm`（`library-map.json + gfx`）。
- 采样：`simulated_now_ms = 该包应用时刻 + k×100ms`，与 `wall_captured_ms` 分列；
  600ms 以接收基准。
- manifest 每帧含：observer/target object_id、`observer_from/to/pixel_offset/screen`、
  `remote_from/to/pixel_offset/screen`（B 自身 motion 绝不填进 remote_*）、camera center、
  global offset、PNG、模拟/墙钟时间。
- 固定地形校验：每采样帧用 `MapScene::render_offset` **单独渲染地形层（无实体）**，
  比较各帧地形层逐字节一致（manifest `terrain_layer_identical=true`）；并行时怪物/
  其他实体不参与该断言。B/A 并发帧同时断言 B anchor `(384,256)`、B global offset
  只来自 B 的 SelfMotion、A remote screen = center+global+Advance。
- 本测试覆盖 **Down 与 DownRight 两段链**；不能代表 8 方向全帧动态，公式覆盖以 M3 为准。

## 5. 覆盖边界 / 未测项（不得冒充）

- 只做**非自机 Player** 的普通 Walk/Run；怪物/NPC、坐骑、Sprint、Pushed/Jump/Dash、
  攻击变招、技能位移不做。
- 未知对象按 R46-X1-04 选择 A：**忽略、不创建实体/不建 motion**，依赖
  `S.ObjectPlayer` 等发现包先建立实体；没有“先更新后创建”的对象发现补齐逻辑。
- live 覆盖 **Down 与 DownRight 两段链**、单观察者；其他方向与多观察者组合未实测。`observer-moving-frame0.png` 是旧批次残留、未被 D2 manifest 引用，已标历史未关联并移出 D2 冻结；D2 只冻结 manifest + 22 张被引用 PNG。
- **窗口四类（中央/触边/沿边/反向离边）与 FPS/录像未测**，保留为残余风险。
- `build/evidence/remote-motion/` 是 gitignore 临时目录，门禁重跑会覆盖；**提交证据以 D2 冻结副本
  `target/remote-motion-frozen-final-20261024T0300/` + `frozen/manifest.json` + `frozen-evidence.sha256` 为准**，
  `live-manifest.json` 仅作早期 22 样本历史对照。
- 未改协议/服务端/DB/素材；本轮为提交前候选（父 `85bfc21`），提交记录见 `git log`。

## 6. 复跑命令

```powershell
cargo test -p mir-client --test remote_motion
cargo test -p mir-client --test remote_motion_live -- --nocapture
# 性能：先生成 85bfc21 baseline（见 §4 的 worktree + baseline-bench.rs），再：
cargo test --release -p mir-client --test remote_motion -- --ignored m4_perf --nocapture
# 并行门禁（必须绿；地形层比较已消除实体串扰）：
cargo test -p mir-client -p mir-render
cargo fmt -p mir-client -p mir-render -- --check
cargo clippy -p mir-client -p mir-render --all-targets -- -D warnings
```
