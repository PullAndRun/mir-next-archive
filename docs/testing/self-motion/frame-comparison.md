# Walk/Run 自移动逐帧对照（Crystal ↔ 传奇-Next Rust）

- Rust 候选：`90b8188c7143bb4800d35abb65bd548a7f6d5f12` **+ 本轮未提交工作区改动**（`crates/mir-client/src/camera.rs`、`app.rs`、`crates/mir-render/src/entities.rs` 等）；manifest 里的 `rust_baseline` 只记录基线提交，提交前须以最终 commit 重新冻结并登记 commit sha。
- Crystal 参考：`github/Crystal` HEAD `0e315fe327192afe52c3d7357ddd1f5b7e26c5b8`
- 逐帧数据：同目录 [`frame-comparison.csv`](./frame-comparison.csv)（96 行 = Walk 8 方向 × 6 帧 + Run 8 方向 × 6 帧）。**CSV 是完整参考对照表，不是「每个组合都被单测逐项动态断言」的测试矩阵**；「对应测试名」列只表示公式族归属，实际动态覆盖见 §5。
- 证据批次：归档 [`live-world-run.log`](./live-world-run.log)（Down，21:53 独立运行，17.88s）与冻结 [`frozen-evidence.sha256`](./frozen-evidence.sha256)（Down，22:00 门禁重跑；manifest 生成 22:01:07）**不是同一次运行**；入库副本 `frozen/evidence-manifest.json` 与批次说明见 §7。
- 本轮修复前的口径（**21:25 旧口径：先截断再乘、f64 四舍五入、无 `%2`、自机被全局偏移推走**）已作废；本文/CSV/live 证据只反映修复后的最终口径。

## 1. Crystal 源码依据（逐字核对）

| 语义 | Crystal 位置 | 原文要点 |
|---|---|---|
| 格尺寸 | `Client/MirScenes/GameScene.cs:10292-10293` | `CellWidth = 48`、`CellHeight = 32` |
| 视口中心 | `GameScene.cs:10398-10399` | `OffSetX = ScreenWidth/2/CellWidth`、`OffSetY = ScreenHeight/2/CellHeight - 1`（800×600 → 8/8） |
| 走/跑步数 | `Client/MirObjects/PlayerObject.cs:795` | `Movement = PointMove(CurrentLocation, Direction, -i)`（起点格） |
| 帧数/帧长 | `Client/MirObjects/Frames.cs:158-159` | Walking `Frame(32,6,0,100)`、Running `Frame(80,6,0,100)` → Count=6、Skip=0、Interval=100ms |
| 帧号 | `PlayerObject.cs:797-798` | `count = Frame.Count; index = FrameIndex` |
| 整数偏移 | `PlayerObject.cs:806-832` | `OffSetMove = new Point((int)((CellWidth*i/(float)count)*(index+1)), ...)`，**`(int)` 包住整个乘积**；八方向符号见下表 |
| `%2` 偶化 | `PlayerObject.cs:834` | `OffSetMove = new Point(X % 2 + X, Y % 2 + Y)`（C# 余数符号跟被除数） |
| 自机不叠偏移 | `PlayerObject.cs:847-854` | `User` 的 `DrawLocation` 只加 `GlobalDisplayLocationOffset`；`OffSetMove` 只加给 `this != User` 的其他对象 |
| 地形吃偏移 | `GameScene.cs:10680`、`:10684` | `drawXCache = (...)*CellWidth - OffSetX + User.OffSetMove.X`、`drawYCache = (...)*CellHeight + User.OffSetMove.Y` |
| 对象层吃偏移 | `GameScene.cs:10814`、`:10820` | `drawY = (...)*CellHeight + User.OffSetMove.Y`、`drawX = (...)*CellWidth - OffSetX + User.OffSetMove.X` |

`Frame` 构造参数顺序 `Frame(start, count, skip, interval, ...)` 见 `Frames.cs:219-229`。

**任务书两处与源码不符，已按源码记录：**

1. 任务书旧示例 `走下的 y = 6,10,16,20,26,30` 是「先截断再乘」的产物；Crystal 的 `(int)` 包住整个乘积，正确序列是 **6,10,16,22,26,32**（Run 竖轴 **10,22,32,42,54,64**）。21:25 旧口径已修复，以本表/CSV/`camera` 单测为准。

   > 本表 Walk 竖轴序列 ±[6,10,16,22,26,32] 已经 gpt-6.1-sol 审核确认，并由 gongbu 独立复核表逐项核对（0 mismatch）。

2. 任务书写 `Skip=2`；Crystal `Frames.cs:158-159` 普通 Walk/Run 都是 `Skip=0`（第二个参数是 Count）。`Skip` 只影响帧号 `OffSet = Count + Skip`，**不参与位移公式**（偏移只用 Count/FrameIndex），对位移无影响。坐骑动作不是普通 Walk/Run：`MountWalking` 8 帧（`Frames.cs:189`）、`MountRunning` 6 帧（`Frames.cs:190`），本模块不适用；Crystal 的 `Sprint` 跑速会改成 `i = 3`（`PlayerObject.cs:788-789`），本作不支持疾风步，固定 `i = 2`。

## 2. 最终公式（Rust `crates/mir-client/src/camera.rs`）

```text
index = min(elapsed_ms / 100, 5)
magnitude_x = (int)((48 * i / 6.0) * (index + 1))   // i = 1 走 / 2 跑
magnitude_y = (int)((32 * i / 6.0) * (index + 1))   // (int) 向零截断，包住整个乘积
world_offset(Crystal OffSetMove) = 方向符号 × (magnitude_x, magnitude_y)
world_offset = (X % 2 + X, Y % 2 + Y)               // 与 C# 同语义
```

八方向世界偏移符号（`PlayerObject.cs:806-832` 原样）：

| 方向 | x | y | 方向 | x | y |
|---|---|---|---|---|---|
| Up | 0 | + | DownRight | − | − |
| UpRight | − | + | Down | 0 | − |
| Right | − | 0 | DownLeft | + | − |
| Left | + | 0 | UpLeft | + | + |

**符号换算（本轮定稿）：**

- `camera::world_offset()` = Crystal `OffSetMove`（指向 `Movement` 起点；走右为负、走下为负）。渲染器的全局偏移**就是**它，加到地形/其他实体上。
- `camera::self_motion_offset()` = 自机推进量（从起点推向目标，走右为正、走下为正），逐轴 = `-world_offset`，即 **`rust_offset = -crystal_off_set_move`**。
- 自机 `EntityView::pixel_offset = -全局 offset = self_motion_offset`；自机格原点恒为 `Viewport::center_origin()`。
- 二者是同一物理量的世界/自机两种表述，不是两套公式；逐帧数值见 CSV 的两组列。

## 3. 序列（已偶化，6 帧）

> gpt 审核确认的 Walk 竖轴序列 [6,10,16,22,26,32]；Run 竖轴 [10,22,32,42,54,64]。

完整口径：Walk 横轴 ±[8,16,24,32,40,48]、竖轴 ±[6,10,16,22,26,32]；Run 横轴 ±[16,32,48,64,80,96]、竖轴 ±[10,22,32,42,54,64]；Crystal 出处 `PlayerObject.cs:806-832`/`:834`，先浮点乘再整体向零截断 + `%2`。

| 动作 | i | 横向 ± | 纵向 ± |
|---|---|---|---|
| Walk | 1 | 8, 16, 24, 32, 40, 48 | 6, 10, 16, 22, 26, 32 |
| Run | 2 | 16, 32, 48, 64, 80, 96 | 10, 22, 32, 42, 54, 64 |

## 4. 坐标/相机变换顺序（Crystal 与 Rust 对照）

**Crystal（每帧，`PlayerObject.cs:765-861` + `GameScene.cs:10658-10946`）**

1. 动作帧推进：`index = FrameIndex`。
2. 走/跑时 `Movement = CurrentLocation` 沿方向倒退 `i` 格（起点），按整数公式算 `OffSetMove` 并 `%2` 偶化。
3. 自机：`DrawLocation = (Movement - User.Movement + OffSet)*Cell` → 恒为屏幕中心（`this == User` 不叠 `User.OffSetMove`，`:847-854`）。
4. 其他对象：`DrawLocation += User.OffSetMove`，再 `-= 自己的 OffSetMove`（`:850-854`）。
5. 地形与对象层：屏幕坐标 = `(cell - User.Movement)*Cell + OffSet*Cell + User.OffSetMove`（`GameScene.cs:10680/10684/10814/10820`）。

**Rust（`crates/mir-client/src/camera.rs` + `crates/mir-render`）**

1. `SelfMotion { from, to, started_ms }`；`from` 即 Crystal 的 `Movement`。
2. `world_offset(from, to, elapsed)` = Crystal `OffSetMove`；`self_motion_offset` = 其相反数（自机推进）。
3. `camera_for()`：`center = from`、`offset = world_offset`、`self_offset = -offset`。
4. `apply_camera()`（生产函数，`app.rs::render_current` 与 live 测试共用）：`entities[0]`（自机）`position = from`、`pixel_offset = self_offset`；其他实体 `pixel_offset` 不变。
5. `EntityRenderer::render_at_offset`：实体屏幕坐标 = `world_to_screen(center, position) + 全局 offset + entity.pixel_offset`；地形 = `tile_origin(center, cell) + 全局 offset`。
6. 因此自机 = 屏幕中心 `(384, 256)`（800×600 的 `center_origin()`），世界反向滚动；600ms 后清 motion，无残留偏移。

行号均为真实源码行：Rust 侧见 `crates/mir-client/src/camera.rs`、`crates/mir-client/src/app.rs` 的 `render_current`、`crates/mir-render/src/entities.rs` 的 `render_at_offset`、`crates/mir-render/src/scene.rs` 的 `render_with_entities_at_offset`。

## 5. 公式证明与 live 回归的分工（动态覆盖边界）

必须把三件事分开，避免把「参考对照表」误读成「全组合动态测试」：

1. **CSV 96 行 = 完整参考对照表**：8 方向 × 6 帧 × Walk/Run 的全部组合都列出预期值；它不是测试矩阵，也不代表每一行都被动态断言。CSV 的「对应测试名」列只表示公式族归属（同一公式被该测试覆盖），不表示该行被逐项断言。
2. **`camera.rs` 单测动态覆盖分两层（共 12 项，不是 96 组合全覆盖）**：

   **公式 6 项**：

| 测试 | 实际动态断言范围 |
|---|---|
| `walking_offsets_match_crystal_integer_truncation_and_evenizing` | Walk **右/下各 6 帧逐帧**；上/左与四个对角只在第 5 帧锁值（6 个方向各 1 帧）。合计 **18/48** 个 Walk「方向×帧」组合 |
| `running_offsets_cover_two_cells` | Run **下 6 帧逐帧**；右只锁第 0/5 帧；右下只锁第 5 帧。合计 **9/48** 个 Run 组合；另验 >600ms 停在最后一帧 |
| `teleports_and_illegal_steps_have_no_offset` | 原地/3 格跳/斜跳/非法位移均返回 `(0,0)` |
| `self_motion_accepts_walk_run_and_diagonals_only` | 走 1 格/跑 2 格/对角合法；原地、3 格、非 45° 拒绝 |
| `camera_centers_on_movement_cell_and_expires_after_600_ms` | `center == Movement`、第 0/5 帧偏移、600ms 返回 `None` |
| `apply_camera_keeps_self_at_screen_center_and_moves_the_world` | 走右第 3 帧：自机钉在屏幕中心、其他实体不抵消、地形锚点吃同一偏移 |

   **生命周期 / 生产前处理 6 项（X1-01）**，均走 `prepare_frame`（`tick_animations` → 自然到期
   `motion.started_ms + 600ms` → 显式打断 → `apply_camera`）：

| 测试 | 实际动态断言范围 |
|---|---|
| `delayed_walk_reply_keeps_action_and_motion_on_one_clock` | 回包延迟 200ms/600ms：`motion.started_ms` = 网络层应用时刻；599ms 内保持 Walking 与逐帧偏移，600ms 自然到期回终点、无残留 |
| `delayed_run_reply_keeps_action_and_motion_on_one_clock` | 同上，覆盖 Run（i=2）的延迟回包 |
| `same_position_snapshot_does_not_restart_motion_or_animation` | 同坐标快照（撞墙/聊天）不重启动作计时、保留 motion，也不延长 600ms |
| `next_authoritative_step_replaces_previous_motion` | 下一步权威落点替换前一步 motion，新起点/新计时，不回退重播 |
| `explicit_interrupts_clear_motion_immediately` | 600ms 内攻击/死亡真实打断：立即清 motion、offset 归零 |
| `teleport_jump_invalidates_motion_and_snaps_to_target` | 3 格传送跳变判 `Invalid`，清 motion 并直接吸附目标格 |

   `mir-render` 另有 `render_at_offset_composes_global_and_entity_pixel_offsets` 证明「全局偏移 + 实体像素偏移」的合成顺序。
3. **live_world 回归只抽单方向链**：`live_client_walk_run_capture_continuous_frames` 每次按出生点可用空间选一个方向（候选顺序 Down → DownRight → …），归档/冻结批次的 Down 链是 **2 Walk + 1 Run**（`(330,270)→(330,271)→(330,272)→(330,274)`）。它只证明「真实服务端 + 生产渲染入口」一致（2 passed；18 样本/18 PNG），**不代表 8 方向全帧动态验证**；公式独立证明仍以 `camera.rs` 单测为准。另注：live capture 在 `live_world.rs:348` 仍直接调 `apply_camera`、**未走 `prepare_frame`**，生命周期回归以 camera 单测为准（见 [`README.md`](./README.md) §3）。

## 6. live_world 真实走跑通过记录（批次 A：归档日志，Down）

命令（工作区根目录）：

```powershell
cargo test -p mir-client --test live_world -- --nocapture
```

结果（完整日志见 [`live-world-run.log`](./live-world-run.log)，文件时间 21:53:57，finished in **17.88s**）：2 passed / 0 failed，关键输出：

```text
PASS 选路：出生点 (330, 270) · 方向 Down · 连续空位 4 格 · 计划 2 次走 + 1 次跑（地图 0）
PASS 地形素材：game-assets/maps/0.mnm（library-map.json + gfx）
PASS walk #1 权威坐标 (330, 270) -> (330, 271) 方向 Down · UserLocation 0 -> 1
PASS walk #2 权威坐标 (330, 271) -> (330, 272) 方向 Down · UserLocation 1 -> 2
PASS run  权威坐标 (330, 272) -> (330, 274) 方向 Down · UserLocation 2 -> 3
PASS 证据：18 个时刻样本 · 18 张 PNG · manifest ...\build\evidence\self-motion\evidence-manifest.json
PASS 总位移：(330, 270) -> (330, 274) · 走 2 步 + 跑 2 格
PASS 相机：自机屏幕锚点恒为 (384, 256) · 世界偏移跑序列 [(0, -10), (0, -22), (0, -32), (0, -42), (0, -54), (0, -64)]
```

> 批次说明：本日志是 **Down** 方向链（2 Walk + 1 Run）的那次运行；其中第一条用例
> `live_client_walks_and_renders_a_frame` 打印「没有可用的地图素材，改用合成地图」属旧冒烟
> （不阻断）；第二条 G3 用例 `PASS 地形素材：game-assets/maps/0.mnm` 才是真实素材证据。
> 日志里的 manifest 路径是当次 `build/` 临时产物，已被后续运行覆盖，不能作为提交证据；
> 提交用 §7 的冻结批次。

断言覆盖：每次走/跑只回 1 条 `UserLocation`（0→1→2→3）、坐标链式推进（走 1 格、跑 2 格）、方向
与服务端一致、`camera.center == motion.from`、自机格原点恒为 `(384, 256)`、世界偏移 6 帧逐帧推进、
地形锚点变化 = 世界偏移变化、18 帧首末像素差 > 100。跑步触发方式：先走一步（`step_counter > 0`）
→ 等 600ms 动作结束 → `C.Run`，无 GM/无服务端强改。

## 7. 证据批次与冻结（提交验收以冻结副本 + 哈希为准）

`build/evidence/self-motion/` 是**可被重跑覆盖的临时目录**（`build/` 与 `target/` 都被 `.gitignore` 忽略）。
本轮能区分出的批次如下，**不能混为同一次运行**：

| 批次 | 方向/链 | 时间（本地） | 产物 | 状态 |
|---|---|---|---|---|
| A 归档日志 | Down 2 Walk + 1 Run：`(330,270)→(330,271)→(330,272)→(330,274)` | 21:53:57（17.88s） | `docs/testing/self-motion/live-world-run.log` | 日志已入库；当次 manifest 未保留 |
| B 临时覆盖 | DownRight 2 Walk + 1 Run：`(330,270)→(331,271)→(332,272)→(334,274)` | 21:54:05 | `build/evidence/self-motion/evidence-manifest.json`（15431 B） | **未冻结、已被覆盖**；仅用于说明「方向相同≠同一次运行」 |
| C 历史参考批次 | Down 2 Walk + 1 Run：`(330,270)→(330,271)→(330,272)→(330,274)` | 22:00 门禁重跑（manifest 生成 22:01:07） | `target/self-motion-frozen-20261023T2205/`（19 文件 = manifest + 18 PNG，本地） | **历史参考，不是提交选定**；旧 manifest sha256 `c08e2bc2…cf8`，仅作历史对照 |
| D 提交批次 | Down 2 Walk + 1 Run：`(330,270)→(330,271)→(330,272)→(330,274)` | **2026-10-24 00:29:48 最终门禁当次**（`target/final-gate-20261024-002851.log`，12/12） | `docs/testing/self-motion/frozen/evidence-manifest.json`（入库，sha256 `a3e874a9…b40a`）；`target/self-motion-frozen-final-20261024T0030/`（19 文件 = manifest + 18 PNG） | **本轮提交选定**；18 样本/18 PNG，哈希见 `frozen-evidence.sha256` |

- A 与 C 方向同为 Down、参数与结果相同，但 **不是同一次运行**：A 是 21:53 独立 `live_world` 运行（17.88s），
  C 是 22:00 门禁重跑产物（manifest 22:01:07 生成）；不得声称同一次运行。
- **本轮提交选定为 D 批次（2026-10-24 00:29:48 最终门禁当次）**；C 的 manifest/18 PNG 只保留为历史对照，不声称是提交选定。
- B 是审核期间出现的 DownRight 覆盖（manifest 21:54:05），**未冻结**，不能引用为提交证据。
- `frozen-evidence.sha256` 绑定 **D 批次**的 20 条哈希（manifest build 原路径 + docs 入库副本 + 18 PNG）；manifest sha256 = `a3e874a9e33ead3606bcbd8b5c0cd79b911abb70721585f1847d78592dd7b40a`；历史 C 旧 manifest `c08e2bc285a694f6e1cebe65ee2c1d1ae169a917f263dead2bea57c18a605cf8` 仅作对照，归档 log（A 批次）sha256 = `012291349fa239d00cd543f303e40c158f636bb8c1a1278bb4268f18b05e4bcd`。
- 最终门禁已执行并冻结 D（12/12，`final-gate.md`）；门禁后仅证据/文档归档变化，源码 blob 不变；若源码再变则重新冻结并重跑。
- 提交时以 **D 批次**的入库 manifest + PNG 副本 + 更新后的 `frozen-evidence.sha256` 为准（C 仅历史对照）；门禁/测试重跑会覆盖 `build/evidence/self-motion/`，不要引用 build 临时路径作为提交证据。
- 证据性质：**权威回包快照冻结后的确定性时间重放**（每步 6×100ms），`rendered_at_ms` 是模拟采样时刻，`captured_at_wall_ms` 是实际渲染墙钟；不是窗口实时录屏。
- 窗口截图：**未完成窗口人工截图**（见 §8），不以确定性重放冒充。

## 8. 窗口截图四类边界：逐项未完成说明

本轮未产出真实窗口（winit + softbuffer）截图；**四类真实窗口边界全部未实测**，≥600ms/跳变只有代码级断言、**无窗口录屏佐证**。以下逐项如实标注“未完成”，并给出可复现步骤（人工 5 分钟内可完成）。**这些是保留到最终验收的残余风险**；§7 的确定性重放证据不能替代窗口实测。

1. **中央移动**：未完成（未做窗口人工截图）。步骤：起服 → `cargo run -p mir-client -- --connect 127.0.0.1:7000 --account shot --password shot --character ShotHero --create-character ShotHero` → 在空旷处按住方向键约 0.6s → 按 `F2` 截 6 张（截图默认写 `build/screenshot-<n>.png`）。
2. **触边（撞墙/回拉）**：未完成（未做窗口人工截图）。步骤：走到地图墙边，继续按住方向键直到服务端回拉 → `F2` 截“贴边/回拉”两张。
3. **沿边**：未完成（未做窗口人工截图）。步骤：贴墙按住与墙平行的方向键连续走 → `F2` 连拍 3 张。
4. **反向离边**：未完成（未做窗口人工截图）。步骤：贴墙后按反方向（先转身再走）→ `F2` 连拍 3 张。
5. **≥600ms / 跳变无残留偏移**：已由单测覆盖（`camera_for` 在 600ms 返回 `None`；`SelfMotion::new` 拒绝跳变；`render_current` 动作结束/传送/重连/切图清 motion），**窗口人工确认未完成**。

> 说明：第 5 项的“无残留”有代码级断言，但没有窗口人工录屏佐证；第 1-4 项完全未做窗口实测，需按上述步骤补。

> **FPS/帧率口径**：本轮**未测 FPS/帧率**；`docs/client.md` 中出现的性能数字仅第 13 轮历史数据，
> 不适用于本轮移动插值/相机改动。

## 9. 门禁记录（本轮实跑）

| 命令 | 结果 |
|---|---|
| `cargo test -p mir-client --test live_world -- --nocapture` | 2 passed / 0 failed，17.88s（日志 `live-world-run.log`，批次 A/Down） |
| `cargo test -p mir-client -p mir-render` | exit 0，34 个 test target 全部 `ok`（合计 502 passed / 0 failed；日志 `build/gate-test-client-render.log`） |
| `cargo fmt -p mir-client -p mir-render -- --check` | exit 0（无 diff） |
| `cargo clippy -p mir-client -p mir-render --all-targets -- -D warnings` | exit 0（日志 `build/gate-clippy-client-render.log`） |

### 9.1 独立复核（提交前）

- gongbu 独立复核（X1-01 前，2026-10-23）：camera **公式 6/6**；96 行 CSV 与独立复核表逐项核对 **0 mismatch**；`live_world` **2 passed**，Down 权威链 `(330,270)→(330,271)→(330,272)→(330,274)`，**18 样本 / 18 PNG**，manifest **0 mismatch**；像素级地形逐帧位移 = `world_offset` 差。
- X1-01 修复后（2026-10-24）：`camera` 单测 **12/12** 通过（L1 复核实跑；新增 6 项覆盖 200/600ms 延迟回包、同坐标不重启、后续步替换、显式打断、传送跳变）。
- `live_world` capture 仍直接调 `apply_camera`、**未走 `prepare_frame`**；延迟/打断的生命周期回归以 camera 单测为准，live 只作分层证据（见 [`README.md`](./README.md) §3）。

> `build/` 被 `.gitignore` 忽略，门禁日志与 PNG 属本地产物；文档、CSV 与历史 C 的 `frozen/evidence-manifest.json` 入库。
> 提交前尚书会在最终候选上重跑 12 项门禁；重跑会覆盖 `build/evidence/self-motion/`，提交验收以 §7 的 **D 批次** + 更新后的 `frozen-evidence.sha256` 为准。
> 以上独立复核**不改变** §8 的四类窗口未实测结论。
