//! 自机走/跑插值与相机（Crystal `PlayerObject.OffSetMove` 的整数版）。
//!
//! Crystal 在 `Client/MirObjects/PlayerObject.cs:765-841` 的 `Moving OffSet` 区段里：
//!
//! * `i = 1`（走）/ `2`（跑）（`PlayerObject.cs:786-790`）；普通 Walk/Run 都走
//!   6 帧 × 100ms（`Client/MirObjects/Frames.cs:158-159`；`Frame(start, count, skip,
//!   interval)` 的参数顺序见 `Frames.cs:219-229`，`Skip = 0` 只参与帧号偏移、不参与
//!   位移公式）；坐骑的 `MountWalking` 是 8 帧、`MountRunning` 是 6 帧
//!   （`Frames.cs:189-190`），不在本模块的适用范围内。
//! * 偏移按 **C# 先浮点乘、再整体向零截断**：
//!   `(int)((CellWidth * i / (float)Count) * (FrameIndex + 1))`，也就是 `(int)` 包住
//!   整个乘积（`PlayerObject.cs:809-830`）；最后做一次偶化
//!   `X % 2 + X`、`Y % 2 + Y`（`:834`）。
//! * 八方向的 `OffSetMove` 指向 `Movement`（起点）方向（`PlayerObject.cs:806-832`）。
//!
//! 于是 6 帧的偏移序列（已偶化）是：
//!
//! | 动作 | i | 横向（±） | 纵向（±） |
//! |---|---|---|---|
//! | Walk | 1 | 8, 16, 24, 32, 40, 48 | 6, 10, 16, 22, 26, 32 |
//! | Run | 2 | 16, 32, 48, 64, 80, 96 | 10, 22, 32, 42, 54, 64 |
//!
//! # 符号：自机推进量 vs 世界坐标偏移
//!
//! Crystal 的 `OffSetMove` 直接加到地形/其他对象的绘制坐标上
//! （`GameScene.cs:10680`、`:10684`、`:10814`、`:10820`），所以它是**世界坐标偏移**，
//! 指向本次移动的起点：走右为负、走下为负。
//!
//! 本模块把两个量分开、并互为相反数（逐轴）：
//!
//! * [`self_motion_offset`]：**自机相对起点向目标推进的像素**（走右第 0 帧 = `+8`）；
//! * [`world_offset`]：**渲染器的全局偏移**，等价于 Crystal 的 `OffSetMove`
//!   （走右第 0 帧 = `-8`），加到地形/其他实体上。
//!
//! 自机 `EntityView::pixel_offset` 取 `-world_offset`（= 自机推进量），把自己钉在
//! 屏幕中心 —— Crystal 对 `User` 的 `DrawLocation` 不叠加 `User.OffSetMove`
//! （`PlayerObject.cs:847-854`）。
//!
//! # 相机：自机恒在屏幕中心
//!
//! Crystal 的相机锚点是自机的 `Movement`（走/跑的起点格，`PlayerObject.cs:795`、
//! `:847`）：自机 `DrawLocation` 恒为 `OffSetX*Cell` 的屏幕中心，世界反向滚动。
//! 本作把「相机中心 = 起点格」交给 `mir-render`，全局偏移 `world_offset` 加到地形/
//! 其他实体，自机像素偏移抵消。`apply_camera` 是窗口循环与端到端测试共用的生产函数。

use crate::net::WorldSnapshot;
use mir_core::Point;
use mir_core::enums::MirDirection;
use mir_render::{ActorAction, CELL_HEIGHT, CELL_WIDTH, EntityView};

/// 走/跑的帧数（`Frames.cs:158-159` 的 `Count = 6`）。
pub const MOTION_FRAMES: i32 = 6;
/// 每帧时长（`Frames.cs:158-159` 的 `Interval = 100`）。
pub const MOTION_INTERVAL_MS: u64 = 100;
/// 一次走/跑动作的总时长（6 帧 × 100ms，与服务端 `MOVE_DELAY_MS = 600` 一致）。
pub const MOTION_DURATION_MS: u64 = MOTION_INTERVAL_MS * 6;
/// 走一步的格数（`PlayerObject.cs:790` 的 `else i = 1`）。
pub const WALK_STEPS: i32 = 1;
/// 跑两步的格数（`PlayerObject.cs:788-789`，未计 Sprint 的 `i = 2`）。
pub const RUN_STEPS: i32 = 2;

/// 自己最近一步的移动：从 `from` 格走到 `to` 格，用于像素级插值。
///
/// 只对「走 1 格 / 跑 2 格」的直线（含 45° 对角）建插值；传送、出生、跨图跳变
/// 之类不建，避免画面拉一条长线。连续走/跑时由最新一步覆盖。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelfMotion {
    /// 起点格（Crystal 的 `Movement` 即 `CurrentLocation` 倒退 `i` 格，`:795`）。
    pub from: Point,
    /// 终点格（Crystal 的 `CurrentLocation`）。
    pub to: Point,
    /// 本地收到这次落点的时刻（Unix 毫秒），帧号从此刻起算。
    pub started_ms: u64,
}

impl SelfMotion {
    /// 按一次坐标变化建插值；不是走/跑（跳变、原地、斜跳）时返回 `None`。
    ///
    /// 合法性判定与服务端一致：横向/纵向 1~2 格，或 45° 对角 1~2 格。
    #[must_use]
    pub fn new(from: Point, to: Point, started_ms: u64) -> Option<Self> {
        let dx = (to.x - from.x).abs();
        let dy = (to.y - from.y).abs();
        let steps = dx.max(dy);
        let straight = dx == 0 || dy == 0 || dx == dy;
        ((WALK_STEPS..=RUN_STEPS).contains(&steps) && straight).then_some(Self {
            from,
            to,
            started_ms,
        })
    }

    /// 从动作开始到 `now_ms` 经过的毫秒数（时钟回拨时按 0 处理）。
    #[must_use]
    pub const fn elapsed_ms(self, now_ms: u64) -> u64 {
        now_ms.saturating_sub(self.started_ms)
    }

    /// 这次走/跑是否还在 600ms 的动作时间内。
    #[must_use]
    pub const fn is_active(self, now_ms: u64) -> bool {
        self.elapsed_ms(now_ms) < MOTION_DURATION_MS
    }

    /// 动作内 `now_ms` 的自机推进像素（从起点指向目标，恒为正方向）。
    #[must_use]
    pub fn offset_at(self, now_ms: u64) -> (i32, i32) {
        self_motion_offset(self.from, self.to, self.elapsed_ms(now_ms))
    }

    /// 动作内 `now_ms` 的渲染器全局偏移（= Crystal `OffSetMove`，指向起点）。
    #[must_use]
    pub fn world_offset_at(self, now_ms: u64) -> (i32, i32) {
        world_offset(self.from, self.to, self.elapsed_ms(now_ms))
    }
}

/// 其他玩家（非自机 Player）最近一步走/跑的运动状态。
///
/// 与 [`SelfMotion`] 使用同一套 Crystal `OffSetMove` 公式，但基点/用法不同：
/// 自机把推进量用于抵消全局偏移保持屏幕中心；远端把推进量作为
/// `EntityView.pixel_offset` 叠加在**自己的**世界坐标上（相机/地形不动）。
/// `started_ms` 是客户端应用该 `S.ObjectWalk`/`S.ObjectRun` 的接收时刻。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteMotion {
    /// 起点格（Crystal `Movement` / 上一条权威位置）。
    pub from: Point,
    /// 终点格（该包 `location`）。
    pub to: Point,
    /// `Walking` 或 `Running`。
    pub action: ActorAction,
    /// 客户端应用该包的时刻（Unix 毫秒），动作与 motion 共用。
    pub started_ms: u64,
}

impl RemoteMotion {
    /// 按一次 ObjectWalk/ObjectRun 建状态；步数必须与动作严格一致（走 1、跑 2），
    /// 且为直线/45° 对角。不合法返回 `None`（调用方只落点，不伪造一步）。
    #[must_use]
    pub fn new(from: Point, to: Point, action: ActorAction, started_ms: u64) -> Option<Self> {
        let expected = match action {
            ActorAction::Walking => WALK_STEPS,
            ActorAction::Running => RUN_STEPS,
            _ => return None,
        };
        let dx = (to.x - from.x).abs();
        let dy = (to.y - from.y).abs();
        let steps = dx.max(dy);
        let straight = dx == 0 || dy == 0 || dx == dy;
        (steps == expected && straight).then_some(Self {
            from,
            to,
            action,
            started_ms,
        })
    }

    /// 从应用时刻到 `now_ms` 的毫秒数（时钟回拨按 0）。
    #[must_use]
    pub const fn elapsed_ms(self, now_ms: u64) -> u64 {
        now_ms.saturating_sub(self.started_ms)
    }

    /// 是否仍在 600ms 动作时间内。
    #[must_use]
    pub const fn is_active(self, now_ms: u64) -> bool {
        self.elapsed_ms(now_ms) < MOTION_DURATION_MS
    }

    /// 当前时刻的远端推进量（从 `from` 推向 `to`，与 `self_motion_offset` 同向）。
    ///
    /// 绘制公式（在 `from` 起点上叠加）：
    ///
    /// ```text
    /// screen = world_to_screen(self_center, from) + self_world_offset + advance
    /// ```
    ///
    /// 即 `-remote CrystalOffSetMove`，**不是**相对终点的剩余偏移。
    #[must_use]
    pub fn advance_at(self, now_ms: u64) -> (i32, i32) {
        self_motion_offset(self.from, self.to, self.elapsed_ms(now_ms))
    }
}

/// 怪物走/跑帧表参数（`frames.json` 的 Walking/Running）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonsterMotionParams {
    /// Walking 动作帧数。
    pub walk_count: u8,
    /// Walking 每帧毫秒。
    pub walk_interval_ms: u64,
    /// Running 动作帧数。
    pub run_count: u8,
    /// Running 每帧毫秒。
    pub run_interval_ms: u64,
}

impl Default for MonsterMotionParams {
    fn default() -> Self {
        Self {
            walk_count: 6,
            walk_interval_ms: MOTION_INTERVAL_MS,
            run_count: 6,
            run_interval_ms: MOTION_INTERVAL_MS,
        }
    }
}

/// 怪物（非玩家）最近一步走/跑的运动状态。
///
/// 与 [`RemoteMotion`] 不同，Crystal `MonsterObject.cs:345-433` 的
/// `Movement` 是**当前格身后** 1/2 格，`DrawingLocation = Movement + OffSetMove`；
/// 因此渲染基点是 `from`，`pixel_offset` 是 `-OffSetMove`（[`MonsterMotion::advance_at`]），
/// 不是 `OffSetMove` 本身。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonsterMotion {
    /// 动作起点（Crystal `Movement`，当前格身后 `steps` 格）。
    pub from: Point,
    /// 当前权威格（Crystal `CurrentLocation`，即终点）。
    pub to: Point,
    /// 朝向（`Movement` → `CurrentLocation`）。
    pub direction: MirDirection,
    /// `Walking` 或 `Running`。
    pub action: ActorAction,
    /// 该包应用时刻。
    pub started_ms: u64,
    /// 实际帧数（`Frame.Count`）。
    pub frame_count: u8,
    /// 每帧毫秒（`Frame.Interval`）。
    pub interval_ms: u64,
}

impl MonsterMotion {
    /// 建一条怪物动作；只接受走 1、跑 2 的直线/45° 位移。
    #[must_use]
    pub fn new(
        from: Point,
        to: Point,
        action: ActorAction,
        started_ms: u64,
        frame_count: u8,
        interval_ms: u64,
    ) -> Option<Self> {
        let expected = match action {
            ActorAction::Walking => WALK_STEPS,
            ActorAction::Running => RUN_STEPS,
            _ => return None,
        };
        let dx = (to.x - from.x).abs();
        let dy = (to.y - from.y).abs();
        let steps = dx.max(dy);
        let straight = dx == 0 || dy == 0 || dx == dy;
        (frame_count > 0 && steps == expected && straight).then_some(Self {
            from,
            to,
            direction: from.direction_to(to),
            action,
            started_ms,
            frame_count,
            interval_ms: interval_ms.max(1),
        })
    }

    /// 已过毫秒。
    #[must_use]
    pub const fn elapsed_ms(self, now_ms: u64) -> u64 {
        now_ms.saturating_sub(self.started_ms)
    }

    /// 动作总时长。
    #[must_use]
    pub const fn duration_ms(self) -> u64 {
        self.frame_count as u64 * self.interval_ms
    }

    /// 是否仍在动作时间内。
    #[must_use]
    pub const fn is_active(self, now_ms: u64) -> bool {
        self.elapsed_ms(now_ms) < self.duration_ms()
    }

    /// 当前帧序号（0 起；超过末帧按末帧，但通常 `is_active` 已过滤）。
    #[must_use]
    pub const fn frame_index(self, now_ms: u64) -> u32 {
        let index = self.elapsed_ms(now_ms) / self.interval_ms;
        if index >= self.frame_count as u64 {
            self.frame_count as u32 - 1
        } else {
            index as u32
        }
    }

    /// 当前正向推进量（从起点格指向终点格）。
    ///
    /// Crystal 的 `OffSetMove` 是“从终点回推起点”的负向量；渲染器把实体画在
    /// 起点格上，必须加 **`-OffSetMove`** 才是正确方向（t44 R49-X1-04）。
    #[must_use]
    pub fn advance_at(self, now_ms: u64) -> (i32, i32) {
        let (x, y) = self.offset_at(now_ms);
        (-x, -y)
    }

    /// 当前 `OffSetMove`（Crystal `MonsterObject.cs:388-420` 的 8 方向公式）。
    ///
    /// 注意 C# 的 `%` 对负数保留负号；这里用 Rust `%`（不是 `rem_euclid`）。
    #[must_use]
    pub fn offset_at(self, now_ms: u64) -> (i32, i32) {
        let index = self.frame_index(now_ms) + 1;
        let count = i32::from(self.frame_count);
        let steps = match self.action {
            ActorAction::Running => RUN_STEPS,
            _ => WALK_STEPS,
        };
        let dx = CELL_WIDTH * steps * index as i32 / count;
        let dy = CELL_HEIGHT * steps * index as i32 / count;
        let (x, y) = match self.direction {
            MirDirection::Up => (0, dy),
            MirDirection::UpRight => (-dx, dy),
            MirDirection::Right => (-dx, 0),
            MirDirection::DownRight => (-dx, -dy),
            MirDirection::Down => (0, -dy),
            MirDirection::DownLeft => (dx, -dy),
            MirDirection::Left => (dx, 0),
            MirDirection::UpLeft => (dx, dy),
        };
        (x % 2 + x, y % 2 + y)
    }
}

/// 一次权威快照相对上一条快照的坐标变化，对 motion 的处置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepMotion {
    /// 坐标没变（聊天/他人动作/撞墙回拉）：保留现有 motion，不重启动作/动画计时。
    Unchanged,
    /// 坐标跳变或不是走/跑位移（传送、出生、切图等）：清掉 motion。
    Invalid,
    /// 有效走/跑落点：新 motion 以「网络层应用该落点并同步重设自机动作」的时刻为起点。
    Started(SelfMotion),
}

/// 用网络层刚应用到快照的权威落点决定 motion。
///
/// 关键：`started_ms` 取 [`crate::net::SelfPlayer::action_started_ms`]，而网络层在应用
/// `S.UserLocation` 时会把自机该次走/跑动作也从**同一时刻**重设
/// （`net::apply_self_location`）。因此视觉动作与 motion 共用「接收/应用时刻」
/// 基准，发包→回包延迟不会再让 walking gate 提前清掉插值（X1-01）。
///
/// 同坐标（`old == new`）返回 [`StepMotion::Unchanged`]：重复快照/撞墙回拉不重启计时。
#[must_use]
pub fn motion_after_authoritative_step(
    snapshot: &WorldSnapshot,
    old: Point,
    new: Point,
) -> StepMotion {
    if old == new {
        return StepMotion::Unchanged;
    }
    let Some(me) = snapshot.self_player.as_ref() else {
        return StepMotion::Invalid;
    };
    let expected = match me.action {
        ActorAction::Walking => WALK_STEPS,
        ActorAction::Running => RUN_STEPS,
        _ => return StepMotion::Invalid,
    };
    // 动作与应用时刻必须同钟；0 表示还没人给这次动作定过基准。
    if me.action_started_ms == 0 {
        return StepMotion::Invalid;
    }
    let dx = (new.x - old.x).abs();
    let dy = (new.y - old.y).abs();
    let steps = dx.max(dy);
    let straight = dx == 0 || dy == 0 || dx == dy;
    // 快照丢失后可能一次看到跨两步的坐标差；与动作步数不一致时宁可不动，也不错误地
    // 把两次走当成一次跑或把两次跑当成一次走（避免回退重播/速度错位）。
    if steps != expected || !straight {
        return StepMotion::Invalid;
    }
    SelfMotion::new(old, new, me.action_started_ms).map_or(StepMotion::Invalid, StepMotion::Started)
}

/// 一步移动在 `elapsed_ms` 时刻 **Crystal 的整数截断值**（未套方向符号）。
///
/// ```text
/// index = min(elapsed_ms / 100, 5)
/// x = (int)((CellWidth  * i / 6.0) * (index + 1))   // CellWidth  = 48
/// y = (int)((CellHeight * i / 6.0) * (index + 1))   // CellHeight = 32
/// ```
///
/// `i` 由这次移动的格数决定：走 1、跑 2。**截断作用于整个乘积**，不是先截断商
/// （Crystal `PlayerObject.cs:809-830` 的 `(int)((...) * (index + 1))`）。
// Crystal 这里用的是 C# `float`（单精度）运算：`(float)(count)` 把除数转成 f32。
// `i32 → f32` 会有精度提示，但本式子的取值只有 48/32/6 这几个小整数，最大乘积也
// 远小于 2^23，转 f32 精确无损；为了逐位复刻 Crystal 的 float 语义，这里保留 f32。
#[allow(clippy::cast_precision_loss)]
#[must_use]
fn motion_magnitude(steps: i32, elapsed_ms: u64) -> (i32, i32) {
    let index = (elapsed_ms / MOTION_INTERVAL_MS).min((MOTION_FRAMES - 1) as u64) as i32;
    let factor = (index + 1) as f32;
    // C# `(int)` 向零截断；Rust 的 `as i32` 对正浮点同样是截断。
    let x = ((CELL_WIDTH * steps) as f32 / MOTION_FRAMES as f32 * factor) as i32;
    let y = ((CELL_HEIGHT * steps) as f32 / MOTION_FRAMES as f32 * factor) as i32;
    (x, y)
}

/// C# `value % 2 + value`（`PlayerObject.cs:834`）。
///
/// Rust 与 C# 的 `%` 同为「余数符号跟被除数」，负奇数会再往外扩 1 像素。
fn evenize(value: i32) -> i32 {
    value % 2 + value
}

/// 一步移动在 `elapsed_ms` 时刻的**自机推进像素**（从起点推向目标，带方向）。
///
/// 这是本作的对内口径（单测按它锁定）：走右第 0 帧 `(+8, 0)`、走下第 0 帧
/// `(0, +6)`。Crystal 的 `OffSetMove` 是它的相反数，见 [`world_offset`]。
#[must_use]
pub fn self_motion_offset(from: Point, to: Point, elapsed_ms: u64) -> (i32, i32) {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let steps = dx.abs().max(dy.abs());
    // 传送/跳变（>2 格）与不存在的斜跳不产生偏移。
    if steps == 0 || steps > RUN_STEPS || (dx != 0 && dy != 0 && dx.abs() != dy.abs()) {
        return (0, 0);
    }
    let (magnitude_x, magnitude_y) = motion_magnitude(steps, elapsed_ms);
    // 自机推进方向 = 从起点指向目标：X 随 dx、Y 随 dy 的符号。
    let sign_x = dx / steps;
    let sign_y = dy / steps;
    (evenize(magnitude_x * sign_x), evenize(magnitude_y * sign_y))
}

/// 一步移动在 `elapsed_ms` 时刻的**世界坐标偏移**（= Crystal `OffSetMove`）。
///
/// 逐轴等于自机推进量的相反数：Crystal 的 `OffSetMove` 指向 `Movement`（起点）
/// （`PlayerObject.cs:806-832`），走右为负、走下为负。这个值加到地形与**其他**
/// 对象上；自机用 [`CameraFrame::self_pixel_offset`] 抵消。
#[must_use]
pub fn world_offset(from: Point, to: Point, elapsed_ms: u64) -> (i32, i32) {
    let (x, y) = self_motion_offset(from, to, elapsed_ms);
    // 直接取反会破坏 `i32::MIN`（不会出现），用 `wrapping_neg` 明确意图。
    (x.wrapping_neg(), y.wrapping_neg())
}

/// 一帧相机状态：中心格 + 世界像素偏移（地形/其他实体都要加）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CameraFrame {
    /// 相机中心格（Crystal 的 `User.Movement`，即本次走/跑的起点）。
    pub center: Point,
    /// 世界坐标偏移（Crystal 的 `User.OffSetMove`），加到地形/其他实体上。
    pub offset: (i32, i32),
    /// 自机推进量（`-offset`），由自机 `EntityView::pixel_offset` 消费。
    pub self_offset: (i32, i32),
    /// 动作已进行的毫秒数。
    pub elapsed_ms: u64,
}

impl CameraFrame {
    /// 自机实体要加的像素偏移：`-世界偏移`，把自机固定在屏幕中心。
    #[must_use]
    pub const fn self_pixel_offset(self) -> (i32, i32) {
        self.self_offset
    }
}

/// 当前时刻的相机；动作已满 600ms 时返回 `None`（调用方清空 motion）。
#[must_use]
pub fn camera_for(motion: &SelfMotion, now_ms: u64) -> Option<CameraFrame> {
    let elapsed_ms = motion.elapsed_ms(now_ms);
    if elapsed_ms >= MOTION_DURATION_MS {
        return None;
    }
    let self_offset = self_motion_offset(motion.from, motion.to, elapsed_ms);
    Some(CameraFrame {
        center: motion.from,
        offset: (self_offset.0.wrapping_neg(), self_offset.1.wrapping_neg()),
        self_offset,
        elapsed_ms,
    })
}

/// 把这一帧的相机与自机抵消偏移应用到实体列表（生产渲染路径）。
///
/// **`entities[0]` 必须是自机**——[`mir_client::net::WorldSnapshot::entity_views`]
/// 约定先放自己、再放其他对象。自机的绘制坐标固定为 `motion.from`（Crystal 的
/// `Movement`，屏幕中心），`pixel_offset` 取 `-世界偏移`；其他实体保持自己的
/// 位置与偏移（0），由渲染器统一加全局 `offset`。
///
/// 返回 `None` 表示这次动作已结束，调用方应清空 `motion`，本帧不叠加偏移。
pub fn apply_camera(
    entities: &mut [EntityView],
    motion: &SelfMotion,
    now_ms: u64,
) -> Option<CameraFrame> {
    let camera = camera_for(motion, now_ms)?;
    if let Some(self_view) = entities.first_mut() {
        self_view.position = motion.from;
        self_view.pixel_offset = camera.self_pixel_offset();
    }
    Some(camera)
}

/// 一帧生产渲染前处理后的 motion 状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionPhase {
    /// 本帧没有 motion（站立/行走动作已结束/还没收到权威落点）。
    None,
    /// motion 仍在 600ms 动作时间内，已应用自机居中与世界偏移。
    Active(CameraFrame),
    /// motion 自然到期（`now - started_ms >= 600ms`），本帧刚被清空。
    Expired,
    /// 动作在 600ms 内被真实打断（攻击/受击/死亡/转身等），motion 被清空。
    Interrupted,
}

/// [`prepare_frame`] 的输出：可直接交给渲染器的实体 + 相机中心 + 全局像素偏移。
#[derive(Debug)]
pub struct PreparedFrame {
    /// 实体列表（自机已在第 0 位，位置/偏移已按 motion 调整）。
    pub entities: Vec<EntityView>,
    /// 相机中心格（motion 有效时是起点 `from`，否则是权威坐标）。
    pub center: Point,
    /// 全局像素偏移（地形与实体层共用；motion 有效时为 Crystal `OffSetMove`）。
    pub offset: (i32, i32),
    /// 本帧 motion 的处置结果（测试与调试用）。
    pub phase: MotionPhase,
}

/// 生产渲染前处理：推进动画 → 判定 motion 生命周期 → 应用自机居中。
///
/// 顺序固定为：
/// 1. [`WorldSnapshot::tick_animations`]：自然播完的动作切回 Standing/Dead；
/// 2. **自然到期**：`now - motion.started_ms >= 600ms` 先清 motion
///    （生命周期只由 `motion.started_ms` 决定，不看 action）；
/// 3. **显式打断**：仍在 600ms 内但自机动作已不是走/跑（攻击/受击/死亡/转身等），清 motion；
/// 4. 仍有效：`apply_camera` 把自机钉在屏幕中心并返回世界偏移。
///
/// `fallback_center` 用于快照里还没有自己时的相机中心（登录/离线过渡）。
pub fn prepare_frame(
    snapshot: &mut WorldSnapshot,
    motion: &mut Option<SelfMotion>,
    fallback_center: Point,
    now_ms: u64,
) -> PreparedFrame {
    snapshot.tick_animations(now_ms);
    let mut entities = snapshot.entity_views(now_ms);
    let mut center = snapshot
        .self_player
        .as_ref()
        .map_or(fallback_center, |me| me.position);
    let mut offset = (0, 0);
    let mut phase = MotionPhase::None;
    if let Some(current) = *motion {
        if current.elapsed_ms(now_ms) >= MOTION_DURATION_MS {
            // 自然到期：动作与 motion 同钟，tick_animations 本帧也把动作切回 Standing。
            *motion = None;
            phase = MotionPhase::Expired;
        } else if !entities
            .first()
            .is_some_and(|view| matches!(view.action, ActorAction::Walking | ActorAction::Running))
        {
            // 显式打断：攻击/受击/死亡/转身等真实动作切换，立即清插值。
            *motion = None;
            phase = MotionPhase::Interrupted;
        } else if let Some(camera) = apply_camera(&mut entities, &current, now_ms) {
            center = camera.center;
            offset = camera.offset;
            phase = MotionPhase::Active(camera);
        } else {
            // elapsed < duration 时 apply_camera 必返回 Some；保险按自然到期处理。
            *motion = None;
            phase = MotionPhase::Expired;
        }
    }
    PreparedFrame {
        entities,
        center,
        offset,
        phase,
    }
}

#[cfg(test)]
mod tests {
    use crate::net::{PlayerLooks, SelfPlayer, WorldSnapshot};
    use mir_core::enums::{MirClass, MirDirection, MirGender};
    use mir_protocol::ServerPacket;
    use mir_protocol::packets::server::UserLocation;
    use mir_render::{ActorAction, EntityKind, Viewport, geometry};

    use super::*;

    /// 模拟“发包/UI 应用”的本地 t0（Unix 毫秒，必须非 0）。
    const T0: u64 = 1_000_000;

    fn walk(from: Point, direction: MirDirection) -> (Point, Point) {
        (from, from.adjacent(direction))
    }

    #[test]
    fn walking_offsets_match_crystal_integer_truncation_and_evenizing() {
        let from = Point::new(10, 10);
        // 走右：i=1、Count=6，x 每帧 8px；自机推进量为正、世界偏移为负。
        let (_, right) = walk(from, MirDirection::Right);
        let expected_x = [8, 16, 24, 32, 40, 48];
        for (index, magnitude) in expected_x.iter().enumerate() {
            let elapsed = (index as u64) * 100;
            assert_eq!(
                self_motion_offset(from, right, elapsed),
                (*magnitude, 0),
                "走右第 {index} 帧（自机推进）"
            );
            assert_eq!(
                world_offset(from, right, elapsed),
                (-magnitude, 0),
                "走右第 {index} 帧（Crystal OffSetMove）"
            );
        }
        // 走下：`(int)(32/6.0*(index+1))` = 5,10,16,21,26,32，偶化成
        // 6,10,16,22,26,32（gpt 审核口径）；世界偏移取反。
        let (_, down) = walk(from, MirDirection::Down);
        let expected_y = [6, 10, 16, 22, 26, 32];
        for (index, magnitude) in expected_y.iter().enumerate() {
            let elapsed = (index as u64) * 100;
            assert_eq!(
                self_motion_offset(from, down, elapsed),
                (0, *magnitude),
                "走下第 {index} 帧（自机推进）"
            );
            assert_eq!(
                world_offset(from, down, elapsed),
                (0, -magnitude),
                "走下第 {index} 帧（Crystal OffSetMove）"
            );
        }
        // 走上/左：同一数值、相反符号（自机口径：上为负 y、左为负 x）。
        let (_, up) = walk(from, MirDirection::Up);
        assert_eq!(self_motion_offset(from, up, 500), (0, -32));
        assert_eq!(world_offset(from, up, 500), (0, 32));
        let (_, left) = walk(from, MirDirection::Left);
        assert_eq!(self_motion_offset(from, left, 500), (-48, 0));
        assert_eq!(world_offset(from, left, 500), (48, 0));
        // 四个对角：自机推进 = (dx,dy) 同号；Crystal 世界偏移 = 806-832 的符号。
        for (direction, self_expected, world_expected) in [
            (MirDirection::UpRight, (48, -32), (-48, 32)),
            (MirDirection::DownRight, (48, 32), (-48, -32)),
            (MirDirection::DownLeft, (-48, 32), (48, -32)),
            (MirDirection::UpLeft, (-48, -32), (48, 32)),
        ] {
            let (_, to) = walk(from, direction);
            assert_eq!(
                self_motion_offset(from, to, 500),
                self_expected,
                "{direction:?} 第 5 帧（自机推进）"
            );
            assert_eq!(
                world_offset(from, to, 500),
                world_expected,
                "{direction:?} 第 5 帧（Crystal OffSetMove）"
            );
        }
    }

    #[test]
    fn running_offsets_cover_two_cells() {
        let from = Point::new(10, 10);
        // 跑右：i=2 → x 每帧 16px，第 5 帧 96px。
        assert_eq!(self_motion_offset(from, Point::new(12, 10), 0), (16, 0));
        assert_eq!(self_motion_offset(from, Point::new(12, 10), 500), (96, 0));
        assert_eq!(world_offset(from, Point::new(12, 10), 500), (-96, 0));
        // 跑下：`(int)(64/6.0*index)` = 10,21,32,42,53,64，偶化成
        // 10,22,32,42,54,64（gpt 审核口径）。
        let down = Point::new(10, 12);
        let expected = [10, 22, 32, 42, 54, 64];
        for (index, magnitude) in expected.iter().enumerate() {
            assert_eq!(
                self_motion_offset(from, down, (index as u64) * 100),
                (0, *magnitude),
                "跑下第 {index} 帧（自机推进）"
            );
        }
        assert_eq!(world_offset(from, down, 500), (0, -64));
        // 跑右下对角：自机推进 (96, 64)，Crystal 世界偏移 (-96, -64)。
        assert_eq!(self_motion_offset(from, Point::new(12, 12), 500), (96, 64));
        assert_eq!(world_offset(from, Point::new(12, 12), 500), (-96, -64));
        // 超过 600ms 停在最后一帧（调用方此时清 motion）。
        assert_eq!(self_motion_offset(from, Point::new(12, 10), 900), (96, 0));
    }

    #[test]
    fn teleports_and_illegal_steps_have_no_offset() {
        let from = Point::new(10, 10);
        assert_eq!(self_motion_offset(from, from, 0), (0, 0));
        assert_eq!(self_motion_offset(from, Point::new(13, 10), 500), (0, 0));
        assert_eq!(self_motion_offset(from, Point::new(12, 11), 500), (0, 0));
        assert_eq!(self_motion_offset(from, Point::new(12, 14), 500), (0, 0));
    }

    #[test]
    fn self_motion_accepts_walk_run_and_diagonals_only() {
        let from = Point::new(10, 10);
        assert!(SelfMotion::new(from, Point::new(11, 10), 0).is_some());
        assert!(SelfMotion::new(from, Point::new(12, 12), 0).is_some());
        assert!(SelfMotion::new(from, Point::new(11, 11), 0).is_some());
        assert!(SelfMotion::new(from, from, 0).is_none());
        assert!(SelfMotion::new(from, Point::new(10, 13), 0).is_none());
        assert!(SelfMotion::new(from, Point::new(12, 11), 0).is_none());
    }

    #[test]
    fn camera_centers_on_movement_cell_and_expires_after_600_ms() {
        let motion = SelfMotion::new(Point::new(10, 10), Point::new(12, 10), MOTION_INTERVAL_MS)
            .expect("跑两步");
        let first = camera_for(&motion, MOTION_INTERVAL_MS).expect("第 0 帧");
        assert_eq!(
            first.center,
            Point::new(10, 10),
            "相机锚点是 Movement（起点）"
        );
        assert_eq!(first.self_offset, (16, 0), "跑右第 0 帧自机推进");
        assert_eq!(first.offset, (-16, 0), "跑右第 0 帧世界偏移");
        assert_eq!(first.elapsed_ms, 0);
        let last = camera_for(&motion, MOTION_INTERVAL_MS + 500).expect("第 5 帧");
        assert_eq!(last.offset, (-96, 0), "跑右第 5 帧世界偏移");
        assert!(camera_for(&motion, MOTION_INTERVAL_MS + MOTION_DURATION_MS).is_none());
    }

    #[test]
    fn apply_camera_keeps_self_at_screen_center_and_moves_the_world() {
        let viewport = Viewport::DEFAULT;
        let from = Point::new(10, 10);
        let motion = SelfMotion::new(from, Point::new(11, 10), 0).expect("走一步");
        // 快照里自己的坐标已经是终点（服务器先回 UserLocation 再本地插值）；
        // entity_views 保证自己排第一。
        let mut entities = [
            EntityView::new(
                Point::new(11, 10),
                MirDirection::Right,
                EntityKind::Player,
                0,
            ),
            EntityView::new(
                Point::new(12, 10),
                MirDirection::Down,
                EntityKind::Monster,
                27,
            ),
        ];
        let now_ms = 300;
        let camera = apply_camera(&mut entities, &motion, now_ms).expect("第 3 帧");
        assert_eq!(camera.offset, (-32, 0), "走右第 3 帧世界偏移");
        assert_eq!(camera.self_offset, (32, 0), "走右第 3 帧自机推进");
        assert_eq!(entities[0].position, from, "自机绘制在 Movement 格");
        assert_eq!(
            entities[0].pixel_offset,
            (32, 0),
            "自机像素偏移 = -世界偏移"
        );
        assert_eq!(entities[1].pixel_offset, (0, 0), "其他实体不抵消");
        // 自机：格原点 + 世界偏移 + 自身偏移 = 屏幕中心（Crystal 的 DrawLocation）。
        let (origin_x, origin_y) =
            geometry::world_to_screen(viewport, camera.center, entities[0].position);
        assert_eq!(
            (
                origin_x + camera.offset.0 + entities[0].pixel_offset.0,
                origin_y + camera.offset.1 + entities[0].pixel_offset.1
            ),
            viewport.center_origin()
        );
        // 地形（同一格的瓦片锚点）也吃同一个世界偏移 —— 世界反向滚动。
        let (tile_x, tile_y) = geometry::tile_origin(viewport, camera.center, from);
        assert_eq!(
            tile_x + camera.offset.0,
            viewport.center_origin().0 - viewport.offset_x() + camera.offset.0,
            "瓦片锚点 = 格原点 - OffSetX + 世界偏移"
        );
        assert_eq!(tile_y + camera.offset.1, origin_y + camera.offset.1);
    }

    // ---- X1-01：动作与 motion 共用接收/应用时刻 ----

    fn snapshot_with_self(position: Point, direction: MirDirection) -> WorldSnapshot {
        WorldSnapshot {
            self_player: Some(SelfPlayer {
                object_id: 1,
                real_id: 1,
                name: "LatencyHero".to_owned(),
                position,
                direction,
                hp: 100,
                hp_max: 100,
                mp: 0,
                mp_max: 0,
                level: 1,
                class: MirClass::Warrior,
                gender: MirGender::Male,
                hair: 0,
                gold: 0,
                credit: 0,
                dead: false,
                action: ActorAction::Standing,
                action_started_ms: 0,
                attack_variant: None,
                native_action: None,
                looks: PlayerLooks::default(),
            }),
            ..WorldSnapshot::default()
        }
    }

    /// 模拟一次走/跑：t0 发包本地起动画，t0+delay 收到 `UserLocation`。
    ///
    /// 回包时先 `tick_animations`（重现 X1-01 里 delay600 动作已 Standing 的现场），
    /// 再应用权威落点。网络层会在应用落点时把动作重置到同一时刻。
    fn step_case(
        direction: MirDirection,
        running: bool,
        delay_ms: u64,
    ) -> (WorldSnapshot, SelfMotion) {
        let from = Point::new(10, 10);
        let steps = if running { RUN_STEPS } else { WALK_STEPS };
        let to = from.moved(direction, steps);
        let action = if running {
            ActorAction::Running
        } else {
            ActorAction::Walking
        };
        let mut snapshot = snapshot_with_self(from, direction);
        snapshot.apply_local_action(action, direction, T0);
        let received = T0 + delay_ms;
        snapshot.tick_animations(received);
        snapshot.apply_packet_at(
            &ServerPacket::UserLocation(UserLocation {
                location: to,
                direction,
            }),
            received,
        );
        let motion = match motion_after_authoritative_step(&snapshot, from, to) {
            StepMotion::Started(motion) => motion,
            other => panic!("有效走/跑落点应建立 motion，实际 {other:?}"),
        };
        assert_eq!(
            motion.started_ms, received,
            "motion 起点必须是网络层应用 UserLocation 的时刻"
        );
        assert_eq!(
            snapshot
                .self_player
                .as_ref()
                .expect("自己")
                .action_started_ms,
            received,
            "动作计时必须与 motion 共用同一接收/应用时刻"
        );
        (snapshot, motion)
    }

    fn assert_active(
        prepared: &PreparedFrame,
        expected_center: Point,
        expected_world: (i32, i32),
        expected_action: ActorAction,
        expected_elapsed: u64,
    ) {
        let MotionPhase::Active(camera) = prepared.phase else {
            panic!("应为 Active，实际 {:?}", prepared.phase);
        };
        assert_eq!(camera.offset, expected_world, "世界偏移");
        assert_eq!(camera.elapsed_ms, expected_elapsed);
        assert_eq!(
            camera.center, expected_center,
            "camera.center == motion.from"
        );
        assert_eq!(prepared.offset, expected_world, "渲染器全局偏移");
        assert_eq!(prepared.center, expected_center, "相机中心 = motion.from");
        let self_view = &prepared.entities[0];
        assert_eq!(self_view.position, camera.center, "自机绘制在起点格");
        assert_eq!(self_view.action, expected_action, "自机视觉动作");
        assert_eq!(
            self_view.pixel_offset,
            (-expected_world.0, -expected_world.1),
            "自机 pixel_offset = -世界偏移"
        );
        let (origin_x, origin_y) =
            geometry::world_to_screen(Viewport::DEFAULT, prepared.center, self_view.position);
        assert_eq!(
            (
                origin_x + prepared.offset.0 + self_view.pixel_offset.0,
                origin_y + prepared.offset.1 + self_view.pixel_offset.1
            ),
            Viewport::DEFAULT.center_origin(),
            "自机屏幕锚点必须恒定"
        );
    }

    const WALK_RIGHT_FRAMES: [(u64, (i32, i32)); 5] = [
        (0, (-8, 0)),
        (100, (-16, 0)),
        (300, (-32, 0)),
        (500, (-48, 0)),
        (599, (-48, 0)),
    ];
    const RUN_RIGHT_FRAMES: [(u64, (i32, i32)); 5] = [
        (0, (-16, 0)),
        (100, (-32, 0)),
        (300, (-64, 0)),
        (500, (-96, 0)),
        (599, (-96, 0)),
    ];

    #[test]
    fn delayed_walk_reply_keeps_action_and_motion_on_one_clock() {
        for delay in [200_u64, 600] {
            let (snapshot, motion) = step_case(MirDirection::Right, false, delay);
            let base = motion.started_ms;
            assert_eq!(base, T0 + delay);
            for (elapsed, expected_world) in WALK_RIGHT_FRAMES {
                let mut snapshot = snapshot.clone();
                let mut active = Some(motion);
                let prepared =
                    prepare_frame(&mut snapshot, &mut active, Point::new(0, 0), base + elapsed);
                assert_active(
                    &prepared,
                    motion.from,
                    expected_world,
                    ActorAction::Walking,
                    elapsed,
                );
                assert_eq!(active, Some(motion), "{elapsed}ms 时 motion 不应被清");
                assert_eq!(
                    snapshot
                        .self_player
                        .as_ref()
                        .expect("自己")
                        .action_started_ms,
                    base,
                    "render 不应重置动作计时"
                );
            }
            // 600ms 自然到期：清 motion，稳定落在 to（不残留偏移）。
            let mut snapshot = snapshot.clone();
            let mut active = Some(motion);
            let prepared = prepare_frame(&mut snapshot, &mut active, Point::new(0, 0), base + 600);
            assert_eq!(prepared.phase, MotionPhase::Expired);
            assert!(active.is_none());
            assert_eq!(prepared.center, motion.to);
            assert_eq!(prepared.offset, (0, 0));
            assert_eq!(prepared.entities[0].position, motion.to);
            assert_eq!(prepared.entities[0].action, ActorAction::Standing);
            let prepared = prepare_frame(&mut snapshot, &mut active, Point::new(0, 0), base + 700);
            assert_eq!(prepared.phase, MotionPhase::None);
            assert_eq!(prepared.center, motion.to);
            assert_eq!(prepared.offset, (0, 0));
        }
    }

    #[test]
    fn delayed_run_reply_keeps_action_and_motion_on_one_clock() {
        for delay in [200_u64, 600] {
            let (snapshot, motion) = step_case(MirDirection::Right, true, delay);
            let base = motion.started_ms;
            assert_eq!(base, T0 + delay);
            for (elapsed, expected_world) in RUN_RIGHT_FRAMES {
                let mut snapshot = snapshot.clone();
                let mut active = Some(motion);
                let prepared =
                    prepare_frame(&mut snapshot, &mut active, Point::new(0, 0), base + elapsed);
                assert_active(
                    &prepared,
                    motion.from,
                    expected_world,
                    ActorAction::Running,
                    elapsed,
                );
                assert_eq!(active, Some(motion), "{elapsed}ms 时 motion 不应被清");
            }
            let mut snapshot = snapshot.clone();
            let mut active = Some(motion);
            let prepared = prepare_frame(&mut snapshot, &mut active, Point::new(0, 0), base + 600);
            assert_eq!(prepared.phase, MotionPhase::Expired);
            assert!(active.is_none());
            assert_eq!(prepared.center, motion.to);
            assert_eq!(prepared.offset, (0, 0));
            assert_eq!(prepared.entities[0].position, motion.to);
            assert_eq!(prepared.entities[0].action, ActorAction::Standing);
        }
    }

    #[test]
    fn same_position_snapshot_does_not_restart_motion_or_animation() {
        let (mut snapshot, motion) = step_case(MirDirection::Right, false, 200);
        let base = motion.started_ms;
        let action_started = snapshot
            .self_player
            .as_ref()
            .expect("自己")
            .action_started_ms;
        let bump_before = snapshot.wall_bump_id;
        // 回包后 100ms 又来一份同坐标快照（撞墙回拉/聊天/他人动作等）。
        snapshot.apply_packet_at(
            &ServerPacket::UserLocation(UserLocation {
                location: motion.to,
                direction: MirDirection::Right,
            }),
            base + 100,
        );
        assert_eq!(snapshot.wall_bump_id, bump_before + 1, "同坐标应记撞墙回拉");
        assert_eq!(
            snapshot
                .self_player
                .as_ref()
                .expect("自己")
                .action_started_ms,
            action_started,
            "同坐标快照不得重启动作计时"
        );
        assert_eq!(
            motion_after_authoritative_step(&snapshot, motion.to, motion.to),
            StepMotion::Unchanged,
            "同坐标快照应保留 motion"
        );
        // motion 仍按原始 600ms 到期，没有被同坐标快照延长。
        let mut active = Some(motion);
        let prepared = prepare_frame(&mut snapshot, &mut active, Point::new(0, 0), base + 599);
        assert_active(&prepared, motion.from, (-48, 0), ActorAction::Walking, 599);
        let prepared = prepare_frame(&mut snapshot, &mut active, Point::new(0, 0), base + 600);
        assert_eq!(prepared.phase, MotionPhase::Expired);
        assert!(active.is_none());
    }

    #[test]
    fn explicit_interrupts_clear_motion_immediately() {
        // 攻击：仍在 600ms 内，但动作已切 Attacking → 立即清 motion。
        let (mut snapshot, motion) = step_case(MirDirection::Right, false, 200);
        let base = motion.started_ms;
        snapshot.apply_local_action(ActorAction::Attacking, MirDirection::Right, base + 50);
        let mut active = Some(motion);
        let prepared = prepare_frame(&mut snapshot, &mut active, Point::new(0, 0), base + 50);
        assert_eq!(prepared.phase, MotionPhase::Interrupted);
        assert!(active.is_none(), "显式打断应立即清 motion");
        assert_eq!(prepared.offset, (0, 0));
        assert_eq!(prepared.center, motion.to);
        assert_eq!(prepared.entities[0].action, ActorAction::Attacking);

        // 死亡：Dying/Dead 同样是显式打断。
        let (mut snapshot, motion) = step_case(MirDirection::Down, true, 200);
        let base = motion.started_ms;
        {
            let me = snapshot.self_player.as_mut().expect("自己");
            me.set_action(ActorAction::Dying, base + 50);
        }
        assert!(snapshot.self_player.as_ref().expect("自己").dead);
        let mut active = Some(motion);
        let prepared = prepare_frame(&mut snapshot, &mut active, Point::new(0, 0), base + 50);
        assert_eq!(prepared.phase, MotionPhase::Interrupted);
        assert!(active.is_none(), "死亡应立即清 motion");
    }

    #[test]
    fn next_authoritative_step_replaces_previous_motion() {
        let from = Point::new(10, 10);
        let mid = Point::new(11, 10);
        let to = Point::new(12, 10);
        let mut snapshot = snapshot_with_self(from, MirDirection::Right);
        // 第一步：t0 发包，t0+200 收到落点。
        snapshot.apply_local_action(ActorAction::Walking, MirDirection::Right, T0);
        snapshot.tick_animations(T0 + 200);
        snapshot.apply_packet_at(
            &ServerPacket::UserLocation(UserLocation {
                location: mid,
                direction: MirDirection::Right,
            }),
            T0 + 200,
        );
        let motion1 = match motion_after_authoritative_step(&snapshot, from, mid) {
            StepMotion::Started(motion) => motion,
            other => panic!("第一步应 Started，实际 {other:?}"),
        };
        assert_eq!(motion1.started_ms, T0 + 200);
        // 第二步：t0+800 收到下一落点，替换前一步（不回退重播第一步）。
        snapshot.apply_local_action(ActorAction::Walking, MirDirection::Right, T0 + 800);
        snapshot.tick_animations(T0 + 800);
        snapshot.apply_packet_at(
            &ServerPacket::UserLocation(UserLocation {
                location: to,
                direction: MirDirection::Right,
            }),
            T0 + 800,
        );
        let motion2 = match motion_after_authoritative_step(&snapshot, mid, to) {
            StepMotion::Started(motion) => motion,
            other => panic!("第二步应 Started，实际 {other:?}"),
        };
        assert_eq!(motion2.started_ms, T0 + 800);
        assert_eq!(motion2.from, mid);
        assert_eq!(motion2.to, to);
        let mut active = Some(motion2);
        let prepared = prepare_frame(&mut snapshot, &mut active, Point::new(0, 0), T0 + 800);
        assert_active(&prepared, motion2.from, (-8, 0), ActorAction::Walking, 0);
        assert_eq!(prepared.center, mid, "新一步的相机中心是新的起点");
    }

    #[test]
    fn teleport_jump_invalidates_motion_and_snaps_to_target() {
        let (mut snapshot, motion) = step_case(MirDirection::Right, false, 200);
        let base = motion.started_ms;
        // 传送/跳变 >2 格：网络层只更新坐标，不重设动作；App 侧 StepMotion::Invalid。
        let target = Point::new(14, 10);
        snapshot.apply_packet_at(
            &ServerPacket::UserLocation(UserLocation {
                location: target,
                direction: MirDirection::Right,
            }),
            base + 100,
        );
        assert_eq!(
            motion_after_authoritative_step(&snapshot, motion.to, target),
            StepMotion::Invalid,
            "3 格跳变不是走/跑，必须让 App 清 motion"
        );
        // 模拟 app.rs::update_self_motion 的 Invalid 分支。
        let mut active = None;
        let prepared = prepare_frame(&mut snapshot, &mut active, Point::new(0, 0), base + 100);
        assert_eq!(prepared.phase, MotionPhase::None);
        assert_eq!(prepared.center, target);
        assert_eq!(prepared.offset, (0, 0));
        assert_eq!(prepared.entities[0].position, target);
    }
}

#[cfg(test)]
mod monster_motion_tests {
    use super::*;

    fn motion(direction: MirDirection, running: bool, count: u8) -> MonsterMotion {
        let to = Point::new(10, 10);
        let steps = if running { RUN_STEPS } else { WALK_STEPS };
        let from = to.moved(direction.reverse(), steps);
        MonsterMotion::new(
            from,
            to,
            if running {
                ActorAction::Running
            } else {
                ActorAction::Walking
            },
            0,
            count,
            100,
        )
        .expect("合法怪兽步")
    }

    #[test]
    fn monster_walk_offsets_follow_crystal_eight_directions() {
        let expected = [
            (MirDirection::Up, (0, 6)),
            (MirDirection::UpRight, (-8, 6)),
            (MirDirection::Right, (-8, 0)),
            (MirDirection::DownRight, (-8, -6)),
            (MirDirection::Down, (0, -6)),
            (MirDirection::DownLeft, (8, -6)),
            (MirDirection::Left, (8, 0)),
            (MirDirection::UpLeft, (8, 6)),
        ];
        for (direction, offset) in expected {
            assert_eq!(
                motion(direction, false, 6).offset_at(0),
                offset,
                "{direction:?}"
            );
        }
        assert_eq!(
            motion(MirDirection::Right, false, 6).offset_at(500),
            (-48, 0)
        );
    }

    #[test]
    fn monster_render_advance_is_negated_offset() {
        // 渲染器把实体画在起点格，必须加 -OffSetMove；八方向符号不能照抄 offset。
        let expected = [
            (MirDirection::Up, (0, -6)),
            (MirDirection::UpRight, (8, -6)),
            (MirDirection::Right, (8, 0)),
            (MirDirection::DownRight, (8, 6)),
            (MirDirection::Down, (0, 6)),
            (MirDirection::DownLeft, (-8, 6)),
            (MirDirection::Left, (-8, 0)),
            (MirDirection::UpLeft, (-8, -6)),
        ];
        for (direction, advance) in expected {
            let motion = motion(direction, false, 6);
            assert_eq!(motion.advance_at(0), advance, "{direction:?}");
            assert_eq!(
                motion.advance_at(0),
                (-motion.offset_at(0).0, -motion.offset_at(0).1)
            );
        }
        assert_eq!(
            motion(MirDirection::Right, false, 6).advance_at(500),
            (48, 0)
        );
        assert_eq!(
            motion(MirDirection::Right, true, 6).advance_at(500),
            (96, 0)
        );
    }

    #[test]
    fn monster_run_offsets_use_two_cells() {
        assert_eq!(motion(MirDirection::Right, true, 6).offset_at(0), (-16, 0));
        assert_eq!(
            motion(MirDirection::Right, true, 6).offset_at(500),
            (-96, 0)
        );
    }

    #[test]
    fn monster_frame_count_and_interval_drive_duration() {
        let walk = motion(MirDirection::Down, false, 8);
        assert_eq!(walk.duration_ms(), 800);
        assert!(walk.is_active(799));
        assert!(!walk.is_active(800));
        assert_eq!(walk.frame_index(0), 0);
        assert_eq!(walk.frame_index(700), 7);
        // 5 帧时单帧步长为 48/5=9.6，C# 截断为 9，偶数化后为 10。
        assert_eq!(motion(MirDirection::Right, false, 5).offset_at(0), (-10, 0));
    }

    #[test]
    fn monster_motion_rejects_bad_steps() {
        let from = Point::new(10, 10);
        assert!(
            MonsterMotion::new(from, Point::new(12, 10), ActorAction::Walking, 0, 6, 100).is_none()
        );
        assert!(
            MonsterMotion::new(from, Point::new(10, 10), ActorAction::Running, 0, 6, 100).is_none()
        );
    }

    #[test]
    fn monster_right_walk_screen_position_converges_without_96px_jump() {
        // Crystal `MonsterObject.cs:384-437`：实体画在 `Movement`（from），
        // 屏幕再加 **-OffSetMove**。若直接加 OffSetMove，最后一帧会画到终点左侧
        // 96px，然后 motion 到期瞬间跳到终点——这正是 t44 R49-X1-04。
        let motion = motion(MirDirection::Right, false, 6);
        let from_x = motion.from.x * 48;
        let to_x = motion.to.x * 48;
        assert_eq!(motion.from, Point::new(9, 10));
        assert_eq!(motion.to, Point::new(10, 10));
        assert_eq!(from_x + motion.advance_at(0).0, from_x + 8);
        let buggy_last = from_x + motion.offset_at(500).0;
        assert_eq!(buggy_last, to_x - 96);
        assert_eq!(to_x - buggy_last, 96);
        let mut previous = from_x + motion.advance_at(0).0;
        for elapsed in [100, 200, 300, 400, 500] {
            let x = from_x + motion.advance_at(elapsed).0;
            assert!(x >= previous, "{elapsed}ms 屏幕 x 回退：{previous} -> {x}");
            assert!(x <= to_x, "{elapsed}ms 屏幕 x 越过终点：{x} > {to_x}");
            previous = x;
        }
        assert_eq!(previous, to_x);
        // motion 到期后实体改画在 to；末帧与终点同一像素列，无跳变。
        assert_eq!(from_x + motion.advance_at(500).0, motion.to.x * 48);
    }
}
