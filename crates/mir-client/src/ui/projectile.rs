//! 投射物飞行层（`S.ObjectProjectile`(125) 的 `Spell.FireBounce`）。
//!
//! 本模块只移植 Crystal 的**数据与公式**，不建 `Missile`/`Effect` 类体系：
//!
//! * G1 事件消费：每帧按位置游标从 `WorldSnapshot::projectile_events` 取走新事件，
//!   一次事件生成一个 [`ProjectileView`]，同帧多事件按到达顺序生成；快照是整体克隆的，
//!   事件表在会话侧累计且从不 drain，所以只能用游标去重，不能 `mem::take`/clone 整个
//!   vec（绘制路径只持 `&WorldSnapshot`）；
//! * G2 飞行插值，参数取自 `Client/MirScenes/GameScene.cs:4699` 的
//!   `CreateProjectile(410, Libraries.Magic, true, 6, 30, 4, targetID: Destination)`：
//!
//! ```text
//! duration = MaxDistance(Source, Destination) * 50          PlayerObject.cs:3546-3547
//! count    = max(1, duration / 30)                          Effect.cs:174-181
//! index    = 410 + (frame % 6) + dir16 * 10                 Effect.cs:196
//! offset   = (Destination - Source) * (48, 32) * frame / count   Effect.cs:206-212
//! ```
//!
//! `Direction16` 按 `GameScene.cs:12044-12065` 逐字移植（16 向、每 22.5° 一档，
//! 0 = 上、4 = 右、8 = 下、12 = 左）；`MaxDistance` 按 `Functions.cs:312`。
//!
//! * G3 命中：到达帧（删除实体时）若目标**仍存在且未死亡**，在目标格播放
//!   `magic.wzl[570..=579]`（每帧 60 ms、共 600 ms）——Crystal `GameScene.cs:4708`
//!   的 `new Effect(Libraries.Magic, 570, 10, 600, sender.Target)`。目标未知或已死亡
//!   则跳过（`:4701` 的 `missile.Target != null`、`:4707` 的 `Dead` 早退）；
//! * G4 音效：出生/命中时把 `mir_render::sound::projectile_sound` 的文件名交给
//!   [`ProjectileSoundSink`]（Crystal `GameScene.cs:4697` 的 `M34-1.wav`、`:4709`
//!   的 `M34-2.wav`，各播一次）。生产路径记录进 [`RecordingSoundSink`]，等待
//!   `mir-client` 的音频通道取走（见「已知限制」）。
//!
//! # 偏离（登记在案）
//!
//! * **D-R59-1（R62 C2 起收敛）**：Crystal `GameScene.cs:4699` 无条件 `CreateProjectile`，
//!   落点在 `CreateProjectile` 里可用 `TargetPoint`/`MapControl.GetObject` 兜底
//!   （`PlayerObject.cs:3546-3556`），`:4701` 才用 `missile.Target` 挂命中。125 包只带
//!   `source`/`destination` 两个对象 ID、wire 无点坐标；本作仅用**可信客户端缓存**
//!   （对象最后已知位置，[`Projectiles::known_positions`]）兜底：目标在任一历史快照里
//!   出现过 → 按缓存落点生成；**从未出现过 → 不生成、不造实体/坐标、不改 wire**。
//! * **R62 C1 严格时钟**：R61 的 `elapsed` 直接换算帧号已删除出生产路径，改为 Crystal
//!   `Effect.cs:81-96` 的 `NextFrame`/`SkipFrames` 状态机（[`ProjectileView::advance_clock`]）：
//!   `now <= NextFrame` 保持（等号不进）；到期时 `Owner.SkipFrames` 为真先额外 `+1`，
//!   再普通 `+1`；越过 `Count` 删除；每次 Process 至多前进一帧，大时间跳跃不逐帧追补；
//!   同刻重复调用不变。每帧仍先按目标当前位置重算 `Duration`/`Count`（Crystal
//!   `Effect.cs:176-189`，R61 M2）。
//! * **R62 C3 像素 offset 是运动量**：屏幕位置 = 格投影（已含 camera/actor 偏移）+
//!   运动 offset，[`ProjectileView::draw_location`] 只叠加一次，不做二次叠加；
//!   offset 由 [`draw_offset`] 按 `(Destination - Source) * (48,32) * frame / count` 给出。
//! * **R62 C4 会话代号/回收**：`WorldSnapshot::session_generation`（客户端本地字段，非 wire）
//!   给每份会话快照一个单调代号；[`Projectiles::consume`] 只用绝对序号 + 代号判定：
//!   同代号空表**不复位**（真裁剪不丢未确认、不重放已确认），新代号首批非空恰复位一次并
//!   清旧 view/hit/cue。仍不新增服务端回收接口，事件表只增不减的物理回收归后续轮次。
//! * `draw_effect` 不叠加 wzl 每帧的 `ImageInfo.offset`（Crystal `MLibrary.cs:658-673`
//!   的 `offSet=true`），属 mir-render 绘制层缺口，D1 不改 `mir-render`；C3 的 offset
//!   指**运动量**（飞行进度），不是 `ImageInfo` offset 缺口修复。
//!
//! # 已知限制
//!
//! `WorldSnapshot::projectile_events` 在会话侧是**只追加**队列（`net.rs` 的
//! `push_projectile_event` 不设上限、不丢最旧；Crystal 客户端每收到一个 125 就生成
//! 投射物，没有丢帧语义），只持 `&WorldSnapshot` 的绘制路径不能 `mem::take`。R60 G2
//! 起快照同时维护 `WorldSnapshot::projectile_events_total`（事件绝对序号，每次 push
//! +1、快照重建归零），[`Projectiles`] 以「已消费绝对序号」为游标；R61 M1 去掉 16 条
//! 事件上限与 32 条 cue 上限后，积压只会多不会丢（`dropped` 只服务手工构造快照与
//! 旧截断形态的兼容路径）。残留风险：
//!
//! * 事件表与 cue 队列只增不减：R62 C4 已加会话代号 + 绝对序号的**逻辑**回收判定
//!   （同代号裁剪空表不复位、不丢最旧冒充回收、不重放已确认），但没有服务端快照/
//!   事件表物理裁剪协议，长期运行的内存回收仍记 **PARTIAL**；本轮不新增强制回收接口。
//! * `SkipFrames` 的**生产来源**：Crystal 取 `Owner.SkipFrames`
//!   （`PlayerObject.cs:750` 的 `ActionFeed.Count > 0`、`MonsterObject.cs:346` 的
//!   `ActionFeed.Count > 1`）；本作客户端没有 `ActionFeed` 的等价数据（`WorldEntity`/
//!   `SelfPlayer` 无动作队列字段），[`owner_skip_frames`] 生产路径返回 `false`，
//!   语义/固定预期表由单测以显式 `skip_frames` 位覆盖 —— 记 **PARTIAL**（数据来源缺口，
//!   不是时钟逻辑缺口）。
//! * 快照被整体重建、且新表在 UI 首次同步前已累计超过旧游标时，靠
//!   `session_generation` 代号区分新旧会话；代号只由 `net.rs` 的会话快照生成函数递增，
//!   测试里可显式赋值。
//!
//! * 命中 570 特效缺素材时由 `scene.draw_effect` 静默跳过（返回 `false`，不 panic、
//!   不画占位块）；「逻辑错」与「缺素材」的区分：帧号/时序由
//!   [`HitEffectView::advance_clock`] 的严格时钟固定预期表单测钉死（R63 G1），素材
//!   是否齐备由 D4 的素材用例单独判定（`game-assets/` 不入库）。
//! * G4 只做到「何时播什么文件」的纯数据层：文件名映射在 `mir_render::sound`，
//!   播放端由 [`ProjectileSoundSink`] 注入。生产路径把 cue 记进 [`RecordingSoundSink`]
//!   （[`Projectiles::take_sound_cues`]）；接 `app.rs` 的真实播放不在 R59 D2 写范围，
//!   真实可听记 **PARTIAL**（R59 计划 §五 R2）。

use std::collections::HashMap;

use mir_core::Point;
use mir_core::enums::Spell;
use mir_render::sound::{ProjectileSoundPhase, projectile_sound};
use mir_render::{CELL_HEIGHT, CELL_WIDTH, Frame, FxKind, MapScene};

use crate::net::{ProjectileEvent, WorldSnapshot};

/// `magic.wzl` 起始帧号（Crystal `GameScene.cs:4699`，`Libraries.Magic` + 410）。
pub const BASE_INDEX: u32 = 410;
/// 每个方向 6 帧（`CreateProjectile(..., 6, ...)` 的 `count`）。
pub const FRAME_COUNT: u32 = 6;
/// 方向步长 = `Skip + FrameCount` = 4 + 6（`Effect.cs:196`）。
pub const DIRECTION_STRIDE: u32 = 10;
/// 飞行帧间隔（毫秒；`CreateProjectile(..., 30, ...)` 的 `interval`）。
pub const FRAME_INTERVAL_MS: u64 = 30;
/// 每格飞行时长（毫秒；`MaxDistance * 50`，`PlayerObject.cs:3547`）。
pub const CELL_FLIGHT_MS: u64 = 50;
/// 方向档数（16 向）。
pub const DIRECTIONS: u8 = 16;

/// 命中特效在 `magic.wzl` 的起始帧号（Crystal `GameScene.cs:4708`
/// 的 `new Effect(Libraries.Magic, 570, 10, 600, sender.Target)`）。
pub const HIT_EFFECT_BASE_INDEX: u32 = 570;
/// 命中特效帧数（`:4708` 的 `count = 10`）。
pub const HIT_EFFECT_FRAME_COUNT: u32 = 10;
/// 命中特效总时长（毫秒；`:4708` 的 `duration = 600`）。
pub const HIT_EFFECT_DURATION_MS: u64 = 600;
/// 命中特效每帧时长 = `600 / 10 = 60`（Crystal `Effect.cs:53` 的 `Duration / Count`）。
pub const HIT_EFFECT_FRAME_MS: u64 = HIT_EFFECT_DURATION_MS / HIT_EFFECT_FRAME_COUNT as u64;

/// 两点间的切比雪夫距离（Crystal `Functions.cs:312` 的 `MaxDistance`）。
#[must_use]
pub fn max_distance(a: Point, b: Point) -> u32 {
    (a.x - b.x).unsigned_abs().max((a.y - b.y).unsigned_abs())
}

/// 飞行总时长（毫秒）= `MaxDistance * 50`。
#[must_use]
pub fn duration_ms(source: Point, destination: Point) -> u64 {
    u64::from(max_distance(source, destination)) * CELL_FLIGHT_MS
}

/// 飞行帧数 = `max(1, duration / 30)`。
#[must_use]
pub fn frame_count(duration: u64) -> u32 {
    u32::try_from(duration / FRAME_INTERVAL_MS)
        .unwrap_or(u32::MAX)
        .max(1)
}

/// 每帧时长 = `duration / count`（Crystal `Effect.cs:55-60` 的 `Duration / Count`，
/// C# 整数除法；`count >= 1`）。
#[must_use]
pub fn frame_interval_ms(duration_ms: u64, count: u32) -> u64 {
    duration_ms / u64::from(count.max(1))
}

/// 下一帧边界（Crystal `Effect.cs:46`/`:96` 的
/// `NextFrame = Start + (Duration / Count) * (CurrentFrame + 1)`）。
#[must_use]
pub fn next_frame_after(start_ms: i64, interval_ms: u64, frame: u32) -> i64 {
    let interval = i64::try_from(interval_ms).unwrap_or(i64::MAX);
    start_ms.saturating_add(interval.saturating_mul(i64::from(frame.saturating_add(1))))
}

/// R59 的 elapsed 公式：`max(1, (elapsed - 1) / interval)`，`interval == 0` 时
/// `elapsed == 0 → 0`、否则 `count`。
///
/// **历史兼容**：R62 C1 起飞行时钟改为严格 `NextFrame`/`SkipFrames` 状态机
/// （[`ProjectileView::advance_clock`]），生产路径**不再**用 elapsed 反推帧号；
/// 该函数仅为 R59 集成证据（`crates/mir-client/tests/round59_projectile_render.rs`）
/// 保留，新代码不要调用。
#[must_use]
pub fn frame_at(elapsed_ms: u64, duration_ms: u64, count: u32) -> u32 {
    let interval = frame_interval_ms(duration_ms, count);
    if interval == 0 {
        return if elapsed_ms == 0 { 0 } else { count };
    }
    let frame = elapsed_ms.saturating_sub(1) / interval;
    u32::try_from(frame).unwrap_or(u32::MAX).min(count)
}

/// Crystal `GameScene.Direction16`（`GameScene.cs:12044-12065`）的移植。
///
/// 返回 0..=15：0 = 上（y 减小）、4 = 右、8 = 下、12 = 左，顺时针每档 22.5°，
/// 数值等于两倍 `MirDirection` 的角度编号。`source == destination` 时 Crystal 会
/// `0/0` 得 `NaN`（未定义），这里固定返回 0。
#[must_use]
pub fn direction16(source: Point, destination: Point) -> u8 {
    if source == destination {
        return 0;
    }
    // 逐字照抄 C#：`c = source`、`a = (c.X, 0)`、`b = destination`；
    // `b.Y -= c.Y; c.Y += bc; b.Y += bc;` 是让 `ab` 与 `ac` 落在同一条边上。
    // （Crystal 后面只用 `c.X` 比较象限，所以 `c.Y` 的改动对结果无影响。）
    let c_x = f64::from(source.x);
    let c_y = f64::from(source.y);
    let bc = distance_f32(c_x, c_y, f64::from(destination.x), f64::from(destination.y));
    let ac = bc;
    let b_x = f64::from(destination.x);
    let b_y = f64::from(destination.y) - c_y + f64::from(bc);
    let ab = distance_f32(b_x, b_y, c_x, 0.0);
    let (ac, bc, ab) = (f64::from(ac), f64::from(bc), f64::from(ab));
    // 余弦定理；数学上 x ∈ [-1, 1]，clamp 只为挡浮点误差。
    let x = (ac * ac + bc * bc - ab * ab) / (2.0 * ac * bc);
    let mut angle = x.clamp(-1.0, 1.0).acos() * 180.0 / std::f64::consts::PI;
    if f64::from(destination.x) < c_x {
        angle = 360.0 - angle;
    }
    angle += 11.25;
    if angle > 360.0 {
        angle -= 360.0;
    }
    // `(int)(angle / 22.5F)`：向零截断；clamp 0..=15 挡浮点误差与理论上的 16。
    let sector = (angle / 22.5) as i32;
    u8::try_from(sector.clamp(0, i32::from(DIRECTIONS - 1))).unwrap_or(0)
}

/// Crystal 的 `Distance`：`sqrt((p2.X - p1.X)^2 + (p2.Y - p1.Y)^2)`，C# 里以
/// `double` 计算后再转 `float` 存进局部变量。
fn distance_f32(x1: f64, y1: f64, x2: f64, y2: f64) -> f32 {
    (x2 - x1).hypot(y2 - y1) as f32
}

/// 当前帧的 `magic.wzl` 帧号：`410 + frame % 6 + dir16 * 10`（`Effect.cs:196`）。
///
/// 结果恒在 `410..=565`：每个方向 6 帧 × 16 向，不会落到 `416..=419`、`566..=569`
/// 的空帧（`magic.wzl` 里这两段是 0×0）。
#[must_use]
pub fn sprite_index(frame: u32, dir16: u8) -> u32 {
    BASE_INDEX + (frame % FRAME_COUNT) + u32::from(dir16.min(DIRECTIONS - 1)) * DIRECTION_STRIDE
}

/// 命中特效在 `magic.wzl` 的帧号（R59 elapsed 公式）：`570 + (elapsed_ms / 60) % 10`。
///
/// Crystal `Effect.cs:53` 每帧 `Duration / Count = 600 / 10 = 60ms` 把 `CurrentFrame`
/// 加一，`Effect.cs:128`（基类 `Draw`）直接画 `BaseIndex + CurrentFrame`，所以 10 帧
/// 依次是 `570..=579`。
///
/// **历史兼容**：R63 G1 起 [`HitEffectView`] 用严格 `NextFrame` 状态时钟
/// （[`HitEffectView::advance_clock`]）推进 `frame`，生产路径**不再**用 elapsed 反推
/// 帧号或删除时机；本函数仅作离线历史 helper（旧证据/公式测试）保留，不得用于决定
/// `HitEffectView` 的帧号或生命周期。
#[must_use]
pub fn hit_effect_index(elapsed_ms: u64) -> u32 {
    HIT_EFFECT_BASE_INDEX
        + u32::try_from(elapsed_ms / HIT_EFFECT_FRAME_MS).unwrap_or(u32::MAX)
            % HIT_EFFECT_FRAME_COUNT
}

/// 该帧的位置偏移（像素）：`(Destination - Source) * (48, 32) * frame / count`。
///
/// 逐字照 `Effect.cs:206-212`（C# 整数除法向零截断）；`frame == count` 时正好是
/// 目标格，`frame == 0` 时是源格。
#[must_use]
pub fn draw_offset(source: Point, destination: Point, frame: u32, count: u32) -> (i32, i32) {
    let count = count.max(1);
    let frame = frame.min(count);
    let dx = (i64::from(destination.x) - i64::from(source.x)) * i64::from(CELL_WIDTH);
    let dy = (i64::from(destination.y) - i64::from(source.y)) * i64::from(CELL_HEIGHT);
    let offset_x = dx * i64::from(frame) / i64::from(count);
    let offset_y = dy * i64::from(frame) / i64::from(count);
    (clamp_i32(offset_x), clamp_i32(offset_y))
}

/// 把可能越界的 i64 收成 i32（地图坐标远小于这里，只为逻辑完备）。
fn clamp_i32(value: i64) -> i32 {
    i32::try_from(value).unwrap_or(if value.is_negative() {
        i32::MIN
    } else {
        i32::MAX
    })
}

/// 一个正在飞行的投射物（Crystal `Missile` 的数据子集）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectileView {
    /// 技能（目前只会是 `Spell::FireBounce`，留给 D2 命中音效/回调）。
    pub spell: Spell,
    /// 出生格（固定；Crystal `Missile.Source`）。
    pub source: Point,
    /// 目标对象 ID（每帧用它重取目标当前位置）。
    pub destination_id: u32,
    /// 最近一帧的目标格（目标消失后保留最后已知位置）。
    pub destination: Point,
    /// 出生时刻（Unix 毫秒；Crystal `Effect.Start`）。
    pub started_ms: i64,
    /// 下一个帧边界（Unix 毫秒；Crystal `Effect.NextFrame`，`:46`/`:96`）。
    ///
    /// C1 起帧号只由这个状态机推进：`now <= next_frame_ms` 保持旧帧，越过才前进一步。
    pub next_frame_ms: i64,
    /// 上一次时钟步进的时刻（同刻重复 `sync` 不重复推进）。
    ///
    /// Crystal 每次游戏循环只 `Process` 一次；本作 `sync` 可能在同一帧被重复调用，
    /// 用这个守卫满足 C1「同刻重复调用不变」。
    pub last_tick_ms: i64,
    /// `Owner.SkipFrames` 等价位（Crystal `Effect.cs:83`）：到期时额外前进一帧。
    ///
    /// 生成时由 [`owner_skip_frames`] 取；生产来源见模块注释的 PARTIAL 登记。
    pub skip_frames: bool,
    /// 最近一帧重算的飞行总时长（R61 M2：每帧按目标当前位置重算，不再冻结）。
    pub duration_ms: u64,
    /// 最近一帧重算的飞行帧数（`max(1, duration / 30)`）。
    pub frame_count: u32,
    /// 16 向方向号（0..=15）。
    pub dir16: u8,
    /// 当前帧号（0..`frame_count`）。
    pub frame: u32,
}

impl ProjectileView {
    /// 当前帧的 `magic.wzl` 帧号。
    #[must_use]
    pub fn sprite_index(&self) -> u32 {
        sprite_index(self.frame, self.dir16)
    }

    /// 当前位置相对出生格的像素偏移。
    #[must_use]
    pub fn offset(&self) -> (i32, i32) {
        draw_offset(self.source, self.destination, self.frame, self.frame_count)
    }

    /// 一次严格 `NextFrame` 时钟步进（Crystal `Effect.Process`，`Effect.cs:81-96`）。
    ///
    /// * `now <= NextFrame` 时保持旧帧（等号不进，`:81`）；
    /// * 到期时先按 `skip_frames` 额外 +1（`:83`），再普通 +1（`:85`）；
    /// * 越过 `frame_count` 返回 `true`（Crystal `Remove()`），由调用方删除并结算命中；
    /// * 否则按新帧号重算 `next_frame_ms`（`:96`，用**当前**重算后的 Duration/Count）；
    /// * 同一时刻重复调用不推进（`last_tick_ms` 守卫：Crystal 每帧只 `Process` 一次）。
    ///
    /// 大时间跳跃**不逐帧追补**：一次步进至多前进一帧（`skip_frames` 时至多两帧），
    /// 未越过的边界不会在本步补掉（C1 固定预期表）。
    pub fn advance_clock(&mut self, now_ms: i64) -> bool {
        if now_ms <= self.next_frame_ms || now_ms <= self.last_tick_ms {
            return false;
        }
        self.last_tick_ms = now_ms;
        let mut frame = self.frame;
        if self.skip_frames {
            frame = frame.saturating_add(1);
        }
        frame = frame.saturating_add(1);
        if frame >= self.frame_count {
            return true;
        }
        self.frame = frame;
        self.next_frame_ms = next_frame_after(
            self.started_ms,
            frame_interval_ms(self.duration_ms, self.frame_count),
            frame,
        );
        false
    }

    /// 屏幕坐标 = 出生格投影 + 该帧运动 offset（**只叠加一次**）。
    ///
    /// `origin` 是调用方算好的「出生格屏幕坐标」（`MapScene::world_to_screen` 已含
    /// camera/actor 偏移）。Crystal `Effect.Draw` 先取 `Source` 屏幕坐标再
    /// `Offset(运动量)` 一次（`Effect.cs:206-212`）；本函数把「只叠加一次」钉成纯函数，
    /// 避免调用方重复叠加 camera/actor 偏移（R62 C3）。
    #[must_use]
    pub fn draw_location(&self, origin: (i32, i32)) -> (i32, i32) {
        let (offset_x, offset_y) = self.offset();
        (
            origin.0.saturating_add(offset_x),
            origin.1.saturating_add(offset_y),
        )
    }

    /// 是否已到达/超时（下一帧就该删除）。
    #[must_use]
    pub fn finished(&self) -> bool {
        self.frame >= self.frame_count
    }
}

/// 一个正在播放的命中特效（Crystal `Effect` 的 `Owner` 形态子集）。
///
/// `GameScene.cs:4708` 的 `new Effect(Libraries.Magic, 570, 10, 600, sender.Target)`
/// 走 `Effect.cs:38-48,81-96` 的 `owner` 构造函数：`Owner = Target`、
/// `Source = Owner.CurrentLocation`，基类 `Draw` 在 `Owner != null` 时每帧取
/// `Owner.DrawLocation`（`Effect.cs:117-119`），所以本结构每帧重取目标当前位置。
/// 帧号走严格 `NextFrame` 状态时钟（R63 G1），不再由 elapsed 反推。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HitEffectView {
    /// 目标对象 ID（每帧用它重取当前位置）。
    pub target_id: u32,
    /// 最近一帧的目标格（目标消失后停在最后已知位置）。
    pub position: Point,
    /// 起始时刻（Unix 毫秒）= 投射物删除（到达）时刻（Crystal `Effect.Start`）。
    pub started_ms: i64,
    /// 当前帧号（0..[`HIT_EFFECT_FRAME_COUNT`]；`Effect.cs:128` 画 `BaseIndex + CurrentFrame`）。
    pub frame: u32,
    /// 下一个帧边界（Unix 毫秒；Crystal `Effect.NextFrame`，`Effect.cs:46`/`:96`）。
    ///
    /// 帧号**只由这个状态机推进**：`now <= next_frame_ms` 保持旧帧，越过才前进一步。
    pub next_frame_ms: i64,
    /// 上一次时钟步进的时刻（同刻重复 `sync` 不重复推进；Crystal 每循环只 `Process` 一次）。
    pub last_tick_ms: i64,
    /// `Owner.SkipFrames` 等价位（Crystal `Effect.cs:83`）：到期时额外前进一帧。
    ///
    /// 生产路径经 [`owner_skip_frames`] 固定取 `false`；测试可注入 `true`。
    pub skip_frames: bool,
}

impl HitEffectView {
    /// 生成一个 570 命中特效视图（Crystal `Effect.cs:38-48,81-96`）。
    ///
    /// `Duration = 600`、`Count = 10` → 每帧 60ms，
    /// `NextFrame = Start + (Duration/Count) * (CurrentFrame + 1) = Start + 60`。
    #[must_use]
    pub fn new(target_id: u32, position: Point, started_ms: i64) -> Self {
        Self {
            target_id,
            position,
            started_ms,
            frame: 0,
            next_frame_ms: next_frame_after(started_ms, HIT_EFFECT_FRAME_MS, 0),
            last_tick_ms: started_ms,
            skip_frames: false,
        }
    }

    /// 已播放的毫秒数（离线诊断用；**不参与**帧号/删除判定）。
    #[must_use]
    pub fn elapsed_ms(&self, now_ms: i64) -> u64 {
        u64::try_from(now_ms.saturating_sub(self.started_ms).max(0)).unwrap_or(u64::MAX)
    }

    /// 当前 `magic.wzl` 帧号：`570 + frame`（`Effect.cs:128` 的 `BaseIndex + CurrentFrame`）。
    ///
    /// 参数只为兼容旧签名保留，**已不参与帧号计算**；绘制查询是纯读，不推进时钟。
    #[must_use]
    pub fn sprite_index(&self, _now_ms: i64) -> u32 {
        HIT_EFFECT_BASE_INDEX.saturating_add(self.frame)
    }

    /// 是否播完（`Effect.Process` 在 `CurrentFrame >= Count` 时 `Remove`，
    /// `Effect.cs:91-106`）。
    ///
    /// 参数只为兼容旧签名保留，**已不参与删除判定**。
    #[must_use]
    pub fn finished(&self, _now_ms: i64) -> bool {
        self.frame >= HIT_EFFECT_FRAME_COUNT
    }

    /// 一次严格 `NextFrame` 时钟步进（Crystal `Effect.Process`，`Effect.cs:81-96`）。
    ///
    /// * `now <= next_frame_ms` 时保持旧帧（等号不进，`:81`）；
    /// * 到期先按 `skip_frames` 额外 +1（`:83`），再普通 +1（`:85`）；
    /// * 前进后 `frame >= 10` 返回 `true`（Crystal `Remove()`，由调用方删除）；
    /// * 否则按新帧号重算 `next_frame_ms`（`:96`）；
    /// * 同一时刻重复调用不推进（`last_tick_ms` 守卫：Crystal 每帧只 `Process` 一次）；
    /// * 时钟回退（`now < last_tick_ms`）不推进；早于 `started_ms` 由 `now <= next` 挡住。
    ///
    /// 大时间跳跃**不逐帧追补**：一次步进至多前进一帧（`skip_frames` 时至多两帧）。
    pub fn advance_clock(&mut self, now_ms: i64) -> bool {
        if now_ms <= self.next_frame_ms || now_ms <= self.last_tick_ms {
            return false;
        }
        self.last_tick_ms = now_ms;
        let mut frame = self.frame;
        if self.skip_frames {
            frame = frame.saturating_add(1);
        }
        frame = frame.saturating_add(1);
        if frame >= HIT_EFFECT_FRAME_COUNT {
            return true;
        }
        self.frame = frame;
        self.next_frame_ms = next_frame_after(self.started_ms, HIT_EFFECT_FRAME_MS, frame);
        false
    }
}

/// 投射物音效的播放端（[`Projectiles::sync_with_sink`] 的注入点）。
///
/// 本模块只把「要播的文件名」交给 sink，不直接依赖音频后端；生产路径由
/// [`Projectiles::sync`] 记录进 [`RecordingSoundSink`]，测试用自定义 sink
/// （fake sink）钉死文件名与次数。
pub trait ProjectileSoundSink {
    /// 播一个音效文件名（来自 [`projectile_sound`]，如 `"M34-1.wav"`）。
    fn play(&mut self, file: &'static str);
}

/// 录制型 sink：按顺序记下每次要播的文件名。
///
/// * 生产路径 [`Projectiles::sync`] 用它当本地 cue 队列，
///   [`Projectiles::take_sound_cues`] 供 `mir-client` 的音频通道每帧 drain；
/// * 没有音频后端（无头模式）时它不影响逻辑。
///
/// 队列不设上限：Crystal 客户端每收到一个 125 就生成投射物（没有丢帧语义），
/// 丢 cue 等价于丢投射物，所以这里只按顺序记录，由消费端每帧 drain（R61 M1）。
#[derive(Debug, Default, Clone)]
pub struct RecordingSoundSink {
    /// 已记录、尚未取走的文件名（按播放顺序）。
    played: Vec<&'static str>,
}

impl RecordingSoundSink {
    /// 已记录、尚未取走的文件名（按播放顺序）。
    #[must_use]
    pub fn played(&self) -> &[&'static str] {
        &self.played
    }

    /// 已记录条数。
    #[must_use]
    pub fn len(&self) -> usize {
        self.played.len()
    }

    /// 内部 `Vec` 的容量（R61 M1 内存口径证据用，不参与生产逻辑）。
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.played.capacity()
    }

    /// 没有待播内容。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.played.is_empty()
    }

    /// 取走全部 cue（音频通道每帧调用一次；取走后队列清空）。
    #[must_use]
    pub fn take(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut self.played)
    }
}

impl ProjectileSoundSink for RecordingSoundSink {
    fn play(&mut self, file: &'static str) {
        self.played.push(file);
    }
}

/// 命中表现的目标格：目标存在且未死亡时返回当前位置。
///
/// 对齐 Crystal `GameScene.cs:4701`（只有 `missile.Target != null` 才挂了命中回调）
/// 与 `:4707`（`sender.Target.CurrentAction == MirAction.Dead` 直接 return）。
/// 自己（`self_player`）也可能被当成目标对象，所以先查自己再查实体表。
#[must_use]
fn hit_target_position(snapshot: &WorldSnapshot, object_id: u32) -> Option<Point> {
    if let Some(me) = &snapshot.self_player {
        if me.object_id == object_id {
            return (!me.dead).then_some(me.position);
        }
    }
    snapshot
        .entities
        .get(&object_id)
        .and_then(|entity| (!entity.dead).then_some(entity.position))
}

/// `Owner.SkipFrames` 的客户端等价位（Crystal `Effect.cs:83`）。
///
/// Crystal 的这一位来自**动作队列深度**：`PlayerObject.cs:750`
/// `SkipFrames = this != User && ActionFeed.Count > 0`，`MonsterObject.cs:346`
/// `SkipFrames = ActionFeed.Count > 1`。本作客户端没有 `ActionFeed` 等价状态
/// （`WorldSnapshot` 只保留每个对象最近一次运动状态），所以生产路径固定返回
/// `false`；真正生效的「Owner != null」条件在 [`Projectiles::spawn`] 里：`source`
/// 解析不到对象就不生成。`SkipFrames` 位登记 **PARTIAL**：等 B 在 net/wiring 提供
/// backlog 等价位后，把这里改成读取该位（并改为每帧刷新）。
fn owner_skip_frames(_snapshot: &WorldSnapshot, _source_id: u32) -> bool {
    false
}

/// 客户端在飞的投射物队列（G1 事件消费 + G2 插值 + G3 命中状态）。
#[derive(Debug, Default)]
pub struct Projectiles {
    /// 在飞实体，按生成顺序（同帧多事件按到达顺序）。
    views: Vec<ProjectileView>,
    /// 正在播放的命中特效（Crystal 挂在目标身上的 `Effect`，按生成顺序）。
    hit_effects: Vec<HitEffectView>,
    /// 已消费的 `WorldSnapshot::projectile_events` 下标（快照整体克隆，游标去重）。
    consumed: usize,
    /// 生产路径采集的投射物音效 cue（等待音频通道 drain）。
    sounds: RecordingSoundSink,
    /// 当前快照的会话代号（R62 C4；对应 `WorldSnapshot::session_generation`）。
    generation: u64,
    /// 目标对象最后已知位置的可信客户端缓存（Crystal `TargetPoint` 兜底，R62 C2）。
    ///
    /// 125 包没有落点坐标（`Shared/ServerPackets.cs:3564-3578`）；目标对象从快照消失后，
    /// 后续 125 只能用「最后一次在快照里见到的位置」生成（Crystal
    /// `PlayerObject.cs:3546-3556` 用 `TargetPoint` 缓存同类语义）。没进过缓存的
    /// 对象仍然不生成、不造坐标。缓存按对象 ID 记，随会话代号复位清空。
    known_positions: HashMap<u32, Point>,
}

impl Projectiles {
    /// 已消费的事件条数（断言「同一事件只生成一次」用）。
    #[must_use]
    pub fn events_consumed(&self) -> usize {
        self.consumed
    }

    /// 当前在飞实体（按生成顺序）。
    #[must_use]
    pub fn views(&self) -> &[ProjectileView] {
        &self.views
    }

    /// 在飞实体数量。
    #[must_use]
    pub fn len(&self) -> usize {
        self.views.len()
    }

    /// 没有在飞实体。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.views.is_empty()
    }

    /// 正在播放的命中特效（按生成顺序）。
    #[must_use]
    pub fn hit_effects(&self) -> &[HitEffectView] {
        &self.hit_effects
    }

    /// 生产中已采集、等待音频通道取走的音效文件名（按播放顺序）。
    #[must_use]
    pub fn sound_cues(&self) -> &[&'static str] {
        self.sounds.played()
    }

    /// 取走音效 cue（`mir-client` 音频通道每帧调用；无音频后端时丢弃）。
    #[must_use]
    pub fn take_sound_cues(&mut self) -> Vec<&'static str> {
        self.sounds.take()
    }

    /// 音效 cue 队列内部 `Vec` 的容量（R61 M1 内存口径证据用，不参与生产逻辑）。
    #[must_use]
    pub fn sound_cue_capacity(&self) -> usize {
        self.sounds.capacity()
    }

    /// 在飞实体 `Vec` 的容量（R61 M1 内存口径证据用，不参与生产逻辑）。
    #[must_use]
    pub fn views_capacity(&self) -> usize {
        self.views.capacity()
    }

    /// 每帧同步：先消费新事件，再按 `now_ms` 推进帧/落点并删除到达的实体。
    ///
    /// 幂等：同一快照、同一时刻重复调用不会重复生成，也不会多推进帧（`last_tick_ms`
    /// 守卫；C1「同刻重复调用不变」）。出生/命中音效记录进内部
    /// [`RecordingSoundSink`]（[`Self::take_sound_cues`] 供音频通道取走）；需要自定义
    /// 播放端时用 [`Self::sync_with_sink`]。
    pub fn sync(&mut self, snapshot: &WorldSnapshot, now_ms: i64) {
        // 先取出内部 sink 避开双重可变借用，同步完再放回。
        let mut sink = std::mem::take(&mut self.sounds);
        let stale_cues = sink.played().len();
        let generation_before = self.generation;
        self.sync_with_sink(snapshot, now_ms, &mut sink);
        if self.generation != generation_before {
            // 新会话复位（C4）：丢弃旧会话遗留的 cue，保留本次同步新产生的 cue。
            let stale = stale_cues.min(sink.played().len());
            let _ = sink.played.drain(..stale);
        }
        self.sounds = sink;
    }

    /// 同 [`Self::sync`]，但音效交给任意 [`ProjectileSoundSink`]（fake sink 注入点）。
    pub fn sync_with_sink(
        &mut self,
        snapshot: &WorldSnapshot,
        now_ms: i64,
        sink: &mut dyn ProjectileSoundSink,
    ) {
        self.consume(snapshot, now_ms, sink);
        self.advance(snapshot, now_ms, sink);
        self.advance_hit_effects(snapshot, now_ms);
    }

    /// 只消费 `WorldSnapshot::projectile_events` 里的新事件。
    ///
    /// 游标是**已消费事件的绝对序号**，不是 `projectile_events` 的下标：R61 M1 起事件表
    /// 只追加（`net.rs` 的 `push_projectile_event` 不设上限、不丢最旧），正常快照下
    /// `projectile_events_total == len`、`dropped == 0`，`events[i]` 的绝对序号就是 `i`，
    /// 游标直接当下标用。R60 F6 的绝对序号机制保留，用于手工构造的快照（`total == 0`）
    /// 与旧截断形态的兼容路径。
    ///
    /// 重建语义（旧「事件表重置 → 游标归零」）收紧为**空表**或「旧游标比新表长且没有
    /// 事件被丢弃」：若仍按旧 `len < consumed` 归零，会把重建快照里已消费的事件整表
    /// 重放（R60 D2 契约 §二）。
    ///
    /// 会话复位（R62 C4）：会话代号变化（`WorldSnapshot::session_generation`）后，
    /// **首批非空事件**上恰好复位一次：清旧 view/hit/cue/位置缓存并把游标归零（旧会话的
    /// 绝对序号不能跨代复用）。同代号下的空表（滞后的克隆/裁剪）**不得**复位游标
    /// （C3「同 generation 裁剪空表不得复位」）；同代号下非空且旧游标超过新表长度仍按
    /// R61 兼容路径视为重建（手工构造快照没有代号可依）。
    fn consume(
        &mut self,
        snapshot: &WorldSnapshot,
        now_ms: i64,
        sink: &mut dyn ProjectileSoundSink,
    ) {
        let events = &snapshot.projectile_events;
        let len = events.len();
        let generation = snapshot.session_generation;
        if generation != self.generation {
            if len == 0 && snapshot.projectile_events_total == 0 {
                // 新会话还没有事件（重连后的首个空快照）：等首批载荷再复位，
                // 避免同一代被空快照反复复位；但本帧可见位置仍可进缓存（R63 G2）。
                self.cache_visible_entities(snapshot);
                return;
            }
            // 跨代复位必须先于本帧缓存填充（R63 G2）：否则 reset 会把新代首批
            // 可见但未被 spawn 引用的对象位置清掉，后续同代 125 丢事件。
            self.reset_for_new_generation(generation);
        }
        // 每帧刷新可信位置缓存（C2）：快照里出现的对象以本帧位置为准，消失后保留
        // 最后一次见到的位置，供后续 125 兜底（Crystal `TargetPoint` 同类语义）。
        self.cache_visible_entities(snapshot);
        let dropped = snapshot
            .projectile_events_total
            .saturating_sub(u64::try_from(len).unwrap_or(u64::MAX));
        let rebuilt = len > 0 && self.consumed > len && dropped == 0;
        if rebuilt {
            // 手工/旧形态快照的非空重建（无会话代号可依）：从头再消费。
            self.consumed = 0;
        }
        // 兼容路径：手工构造/旧形态快照可能声明丢弃数，跳过这些事件（不重放），
        // 游标直接推到丢弃边界。
        let mut cursor = u64::try_from(self.consumed).unwrap_or(u64::MAX);
        if dropped > cursor {
            cursor = dropped;
        }
        // 起点下标 = 绝对游标 - 丢弃数；`dropped` 超过 `len` 时 saturating 到底。
        let mut index = usize::try_from(cursor.saturating_sub(dropped)).unwrap_or(usize::MAX);
        while index < len {
            let event = events[index];
            index += 1;
            cursor = cursor.saturating_add(1);
            self.spawn(event, snapshot, now_ms, sink);
        }
        // 被丢弃但未消费的事件同样计入游标，避免下次按旧下标重复结算。
        self.consumed = usize::try_from(cursor).unwrap_or(usize::MAX);
    }

    /// 用本帧快照里可见的实体刷新可信位置缓存（C2）。
    fn cache_visible_entities(&mut self, snapshot: &WorldSnapshot) {
        for entity in snapshot.entities.values() {
            self.known_positions
                .insert(entity.object_id, entity.position);
        }
    }

    /// 新会话复位（R62 C4）：清在飞实体、命中特效、目标位置缓存并把游标归零。
    fn reset_for_new_generation(&mut self, generation: u64) {
        self.views.clear();
        self.hit_effects.clear();
        self.known_positions.clear();
        self.consumed = 0;
        self.generation = generation;
    }

    /// 一个事件生成一个实体；`source` 解析不到坐标就丢弃（见 D-R59-1）。
    ///
    /// 落点（R62 C2）：目标对象当前不可用时退回可信客户端缓存（该对象最后一次在快照里
    /// 出现的位置，Crystal `TargetPoint` 兜底 `PlayerObject.cs:3546-3556` 同类语义）；
    /// 缓存也没有才丢弃——不造坐标、不改 wire。命中判定仍要求目标存活
    /// （[`hit_target_position`]），目标重现不会补播已消费的旧事件。
    fn spawn(
        &mut self,
        event: ProjectileEvent,
        snapshot: &WorldSnapshot,
        now_ms: i64,
        sink: &mut dyn ProjectileSoundSink,
    ) {
        let Some(source) = snapshot.object_position(event.source) else {
            return;
        };
        self.known_positions.insert(event.source, source);
        let destination = snapshot
            .object_position(event.destination)
            .or_else(|| self.known_positions.get(&event.destination).copied());
        let Some(destination) = destination else {
            // 偏离 D-R59-1：Crystal `GameScene.cs:4699` 无条件生成（`:4701` 才按目标
            // 挂命中）；125 没有落点坐标、缓存也没有时不生成实体，不造坐标，也不播
            // 出生音（C3：出生音效随投射物生成，无投射物则无声）。
            return;
        };
        self.known_positions.insert(event.destination, destination);
        let duration = duration_ms(source, destination);
        let count = frame_count(duration);
        if let Some(file) = projectile_sound(event.spell, ProjectileSoundPhase::Spawn) {
            // Crystal `GameScene.cs:4697`：出生音在 `CreateProjectile` 之前播。
            sink.play(file);
        }
        self.views.push(ProjectileView {
            spell: event.spell,
            source,
            destination_id: event.destination,
            destination,
            started_ms: now_ms,
            next_frame_ms: next_frame_after(now_ms, frame_interval_ms(duration, count), 0),
            last_tick_ms: now_ms,
            skip_frames: owner_skip_frames(snapshot, event.source),
            duration_ms: duration,
            frame_count: count,
            dir16: direction16(source, destination),
            frame: 0,
        });
    }

    /// 先重取落点、再重算参数、最后推进帧号；到达（`frame >= frame_count`）即删除并
    /// 生成命中表现。
    ///
    /// Crystal `Missile.Process`（`Effect.cs:174-181`）每帧先
    /// `Destination = Target.CurrentLocation`，再重算
    /// `Duration = MaxDistance(Source, Destination) * 50` 与
    /// `Count = max(1, Duration / Interval)`；本函数按同样顺序执行（R61 M2）。参数不再
    /// 冻结：目标走远会拉长飞行、走近会缩短，`finished` 用重算后的参数判定。目标消失
    /// 就停在最后已知位置。删除点对应 Crystal `Missile.Complete`
    /// （`GameScene.cs:4701-4710`）：目标存在且未死亡才加 570 命中特效 + 命中音效
    /// （`:4708` / `:4709`）。
    fn advance(
        &mut self,
        snapshot: &WorldSnapshot,
        now_ms: i64,
        sink: &mut dyn ProjectileSoundSink,
    ) {
        // 每帧先重取目标位置 → 重算 Duration/Count（Crystal 顺序），再走一次严格
        // NextFrame 步进（C1）；大跳只前进一步，不做 elapsed 追赶。
        let mut cache_updates: Vec<(u32, Point)> = Vec::new();
        let mut views = std::mem::take(&mut self.views);
        for view in &mut views {
            if let Some(position) = snapshot.object_position(view.destination_id) {
                view.destination = position;
                cache_updates.push((view.destination_id, position));
            }
            view.duration_ms = duration_ms(view.source, view.destination);
            view.frame_count = frame_count(view.duration_ms);
        }
        for (id, position) in cache_updates {
            self.known_positions.insert(id, position);
        }
        let mut finished = Vec::new();
        views.retain_mut(|view| {
            if view.advance_clock(now_ms) {
                finished.push(*view);
                false
            } else {
                true
            }
        });
        self.views = views;
        for view in finished {
            // Crystal `Missile.Complete`（`GameScene.cs:4701-4710`）：目标存在且未死亡才
            // 生成 570 命中特效 + 命中音效；死目标/未知目标零命中（C2）。
            if let Some(position) = hit_target_position(snapshot, view.destination_id) {
                self.known_positions.insert(view.destination_id, position);
                let mut effect = HitEffectView::new(view.destination_id, position, now_ms);
                // Crystal `Effect.cs:83` 的 `Owner.SkipFrames`：命中特效的 Owner 是目标。
                // 生产路径固定 false（PARTIAL 登记见 `owner_skip_frames`）。
                effect.skip_frames = owner_skip_frames(snapshot, view.destination_id);
                self.hit_effects.push(effect);
                if let Some(file) = projectile_sound(view.spell, ProjectileSoundPhase::Hit) {
                    // Crystal `GameScene.cs:4709`：命中音与 570 特效同一时机。
                    sink.play(file);
                }
            }
        }
    }

    /// 推进命中特效：位置跟目标（Crystal `Effect.Draw` 用 `Owner.DrawLocation`），
    /// 帧号走严格 `NextFrame` 状态机，播满 10 帧后删除（`Effect.Process` 的 `Remove`）。
    ///
    /// 绘制查询（[`HitEffectView::sprite_index`]）是纯读，不参与推进。
    fn advance_hit_effects(&mut self, snapshot: &WorldSnapshot, now_ms: i64) {
        let mut cache_updates: Vec<(u32, Point)> = Vec::new();
        for effect in &mut self.hit_effects {
            if let Some(position) = snapshot.object_position(effect.target_id) {
                effect.position = position;
                cache_updates.push((effect.target_id, position));
            }
        }
        for (id, position) in cache_updates {
            self.known_positions.insert(id, position);
        }
        // 严格状态时钟（R63 G1）：每次 `sync` 至多推进一帧（skip 时至多两帧），
        // 同刻幂等；越过第 10 帧即 Remove。
        self.hit_effects
            .retain_mut(|effect| !effect.advance_clock(now_ms));
    }

    /// 画在飞投射物与命中特效：`FxKind::Magic` + `draw_effect`（缺素材时静默跳过）。
    ///
    /// Crystal 的 `DrawLocation` 是「出生格屏幕坐标 + 该帧偏移」
    /// （`Effect.cs:206-212`），`world_to_screen` 对应它前面的摄像机换算；偏移经
    /// [`ProjectileView::draw_location`] **只叠加一次**（R62 C3）。命中特效则直接画在
    /// 目标格（`Effect.cs:117-119` 的 `Owner.DrawLocation`）。
    pub fn draw(&self, frame: &mut Frame, scene: &mut MapScene, center: Point, now_ms: i64) {
        for view in &self.views {
            let origin = scene.world_to_screen(center, view.source);
            let (x, y) = view.draw_location(origin);
            scene.draw_effect(frame, FxKind::Magic, view.sprite_index(), x, y);
        }
        for effect in &self.hit_effects {
            let (x, y) = scene.world_to_screen(center, effect.position);
            let index = effect.sprite_index(now_ms);
            if !scene.draw_effect(frame, FxKind::Magic, index, x, y) {
                // 缺素材（`magic.wzl[570..=579]` 未导出）时静默跳过，不画占位块；
                // 「逻辑错」由 `hit_effect_index` 的单测钉死，两者不要混为一谈。
                tracing::debug!(index = index, "命中特效素材缺失，跳过绘制");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::WorldEntity;
    use mir_core::enums::MirDirection;
    use mir_render::EntityKind;

    fn snapshot_with(source: Point, destination: Point) -> WorldSnapshot {
        let mut snapshot = WorldSnapshot::default();
        snapshot.entities.insert(
            1,
            WorldEntity::new(
                1,
                EntityKind::Monster,
                String::from("caster"),
                source,
                MirDirection::Right,
                0,
            ),
        );
        snapshot.entities.insert(
            2,
            WorldEntity::new(
                2,
                EntityKind::Monster,
                String::from("target"),
                destination,
                MirDirection::Left,
                0,
            ),
        );
        snapshot
    }

    fn fire_bounce(source: u32, destination: u32) -> ProjectileEvent {
        ProjectileEvent {
            spell: Spell::FireBounce,
            source,
            destination,
        }
    }

    /// fake sink：记录每次要播的文件名（文件名与次数断言的注入点）。
    #[derive(Default)]
    struct FakeSink {
        played: Vec<&'static str>,
    }

    impl ProjectileSoundSink for FakeSink {
        fn play(&mut self, file: &'static str) {
            self.played.push(file);
        }
    }

    #[test]
    fn direction16_matches_crystal_sixteen_way_grid() {
        let origin = Point::new(10, 20);
        assert_eq!(direction16(origin, Point::new(10, 15)), 0);
        assert_eq!(direction16(origin, Point::new(15, 15)), 2);
        assert_eq!(direction16(origin, Point::new(15, 20)), 4);
        assert_eq!(direction16(origin, Point::new(15, 25)), 6);
        assert_eq!(direction16(origin, Point::new(10, 25)), 8);
        assert_eq!(direction16(origin, Point::new(5, 25)), 10);
        assert_eq!(direction16(origin, Point::new(5, 20)), 12);
        assert_eq!(direction16(origin, Point::new(5, 15)), 14);
        assert_eq!(direction16(origin, origin), 0);
    }

    /// 按 spawn 时刻构造一个「纯时钟」视图（不依赖快照），供 C1 固定预期表使用。
    fn make_view(source: Point, destination: Point, start_ms: i64) -> ProjectileView {
        let duration = duration_ms(source, destination);
        let count = frame_count(duration);
        ProjectileView {
            spell: Spell::FireBounce,
            source,
            destination_id: 2,
            destination,
            started_ms: start_ms,
            next_frame_ms: next_frame_after(start_ms, frame_interval_ms(duration, count), 0),
            last_tick_ms: start_ms,
            skip_frames: false,
            duration_ms: duration,
            frame_count: count,
            dir16: direction16(source, destination),
            frame: 0,
        }
    }

    /// 模拟 UI 循环：`start_ms` 起每 `step_ms` 调一次 `sync_with_sink`（含端点）。
    fn run_ticks(
        projectiles: &mut Projectiles,
        snapshot: &WorldSnapshot,
        start_ms: i64,
        end_ms: i64,
        step_ms: i64,
        sink: &mut dyn ProjectileSoundSink,
    ) {
        let step = step_ms.max(1);
        let mut now = start_ms;
        while now <= end_ms {
            projectiles.sync_with_sink(snapshot, now, sink);
            now = now.saturating_add(step);
        }
    }

    /// 逐帧推进直到没有在飞实体，返回最后一次 `sync` 的时刻。
    fn tick_until_empty(
        projectiles: &mut Projectiles,
        snapshot: &WorldSnapshot,
        start_ms: i64,
        step_ms: i64,
        limit_ms: i64,
        sink: &mut dyn ProjectileSoundSink,
    ) -> i64 {
        let step = step_ms.max(1);
        let mut now = start_ms;
        let mut last = start_ms;
        while now <= limit_ms {
            projectiles.sync_with_sink(snapshot, now, sink);
            last = now;
            if projectiles.is_empty() {
                return last;
            }
            now = now.saturating_add(step);
        }
        last
    }

    fn cue_count(sink: &FakeSink, file: &'static str) -> usize {
        sink.played.iter().filter(|cue| **cue == file).count()
    }

    #[test]
    fn flight_formulas_match_crystal_effect() {
        let source = Point::new(10, 10);
        let target = Point::new(15, 10);
        assert_eq!(max_distance(source, target), 5);
        assert_eq!(duration_ms(source, target), 250);
        assert_eq!(frame_count(250), 8);
        assert_eq!(frame_count(0), 1);
        assert_eq!(frame_interval_ms(250, 8), 31, "C# integer division 250 / 8");
        // Crystal `Effect.cs:46`/`:96`：
        // `NextFrame = Start + (Duration / Count) * (CurrentFrame + 1)`。
        assert_eq!(next_frame_after(1_000, 31, 0), 1_031);
        assert_eq!(next_frame_after(1_000, 31, 7), 1_248);
        assert_eq!(next_frame_after(0, 0, 0), 0);
        // 零距离：Count == 0 -> 1（Effect.cs:178-181），间隔 0，第一个 tick 到达。
        assert_eq!(frame_interval_ms(0, 1), 0);
        // 两格：100 / 3 = 33。
        assert_eq!(frame_interval_ms(100, 3), 33);
        assert_eq!(next_frame_after(1_000, 33, 2), 1_099);
        for dir16 in 0..DIRECTIONS {
            assert_eq!(
                sprite_index(0, dir16),
                BASE_INDEX + u32::from(dir16) * DIRECTION_STRIDE
            );
            for frame in 0..FRAME_COUNT {
                let index = sprite_index(frame, dir16);
                assert!(
                    (BASE_INDEX..=565).contains(&index),
                    "index {index} outside magic.wzl projectile range"
                );
                assert!(!(416..=419).contains(&index), "empty frame {index}");
                assert!(!(566..=569).contains(&index), "empty frame {index}");
            }
        }
        assert_eq!(sprite_index(FRAME_COUNT, 0), BASE_INDEX);
        assert_eq!(draw_offset(source, target, 0, 8), (0, 0));
        assert_eq!(draw_offset(source, target, 8, 8), (5 * 48, 0));
        assert_eq!(
            draw_offset(source, Point::new(10, 14), 4, 8),
            (0, 4 * 32 * 4 / 8)
        );
    }

    /// C1 固定预期表（尚书核）：等号不进、普通到期 +1、skip 到期 +2、大跳不追补、
    /// 同刻重复不变、删除只由下一次合法 `Process` 触发。
    #[test]
    fn c1_strict_next_frame_clock_fixed_expectation_table() {
        // 5 格：duration 250 / count 8 / interval 31，spawn @1000 → NextFrame 1031。
        let mut view = make_view(Point::new(10, 10), Point::new(15, 10), 1_000);
        assert_eq!(
            (view.duration_ms, view.frame_count, view.next_frame_ms),
            (250, 8, 1_031)
        );
        assert!(
            !view.advance_clock(1_031),
            "CMain.Time <= NextFrame 时保持旧帧"
        );
        assert_eq!(view.frame, 0);
        assert!(!view.advance_clock(1_032));
        assert_eq!((view.frame, view.next_frame_ms), (1, 1_062));
        assert!(!view.advance_clock(1_032), "同一时刻重复调用不推进");
        assert_eq!(view.frame, 1);
        // 大时间跳跃只前进一步，不按 elapsed 追赶。
        assert!(!view.advance_clock(2_000));
        assert_eq!((view.frame, view.next_frame_ms), (2, 1_093));
        assert!(!view.advance_clock(30_000));
        assert_eq!(view.frame, 3, "一次 Process 至多 +1 帧（无 skip）");
        // 删除也需要逐帧走完剩余边界（frame 3 → 8 共 5 次合法 Process）。
        let mut removed_at = None;
        for tick in 0..8 {
            if view.advance_clock(30_001 + tick) {
                removed_at = Some(tick);
                break;
            }
        }
        assert_eq!(removed_at, Some(4), "第 5 次合法 Process 才越过 count");
        assert_eq!(view.frame, 7);

        // SkipFrames 表：到期先额外 +1 再普通 +1（Effect.cs:83-85）。
        let mut skip = make_view(Point::new(10, 10), Point::new(15, 10), 1_000);
        skip.skip_frames = true;
        assert!(!skip.advance_clock(1_031));
        assert_eq!(skip.frame, 0);
        assert!(!skip.advance_clock(1_032));
        assert_eq!((skip.frame, skip.next_frame_ms), (2, 1_093));
        assert!(!skip.advance_clock(1_093), "等号仍不进");
        assert_eq!(skip.frame, 2);
        assert!(!skip.advance_clock(1_094));
        assert_eq!((skip.frame, skip.next_frame_ms), (4, 1_155));
        assert!(!skip.advance_clock(1_156));
        assert_eq!((skip.frame, skip.next_frame_ms), (6, 1_217));
        assert!(skip.advance_clock(1_218), "skip 下 6+2 >= 8 直接删除");
        assert_eq!(skip.frame, 6, "删除前不写入越界帧号");

        // 零距离：Count 至少 1；spawn 当刻等号保持，下一毫秒到达。
        let mut zero = make_view(Point::new(10, 10), Point::new(10, 10), 1_000);
        assert_eq!(
            (zero.duration_ms, zero.frame_count, zero.next_frame_ms),
            (0, 1, 1_000)
        );
        assert!(!zero.advance_clock(1_000));
        assert!(zero.advance_clock(1_001));
        // Owner 条件：客户端没有 ActionFeed 等价位，生产路径固定不 skip（PARTIAL 登记）。
        assert!(!owner_skip_frames(&WorldSnapshot::default(), 1));
    }

    #[test]
    fn events_spawn_once_and_follow_target_position() {
        let mut snapshot = snapshot_with(Point::new(10, 10), Point::new(15, 10));
        snapshot.projectile_events.push(fire_bounce(1, 2));
        let mut projectiles = Projectiles::default();
        let mut sink = FakeSink::default();
        projectiles.sync_with_sink(&snapshot, 1_000, &mut sink);
        assert_eq!(projectiles.events_consumed(), 1);
        assert_eq!(projectiles.len(), 1);
        let view = projectiles.views()[0];
        assert_eq!(view.source, Point::new(10, 10));
        assert_eq!(view.destination, Point::new(15, 10));
        assert_eq!(view.duration_ms, 250);
        assert_eq!(view.frame_count, 8);
        assert_eq!(view.next_frame_ms, 1_031);
        assert_eq!(view.dir16, 4);
        assert_eq!(view.frame, 0);
        assert_eq!(view.sprite_index(), BASE_INDEX + 40);
        // 同一快照再同步：不重复生成。
        projectiles.sync_with_sink(&snapshot, 1_000, &mut sink);
        assert_eq!(projectiles.events_consumed(), 1);
        assert_eq!(projectiles.len(), 1);
        // 目标移动后按当前位置重取落点，并按落点重算动态参数（R61 M2）。
        snapshot.entities.get_mut(&2).expect("target").position = Point::new(15, 12);
        projectiles.sync_with_sink(&snapshot, 1_032, &mut sink);
        assert_eq!(projectiles.views()[0].frame, 1);
        projectiles.sync_with_sink(&snapshot, 1_063, &mut sink);
        assert_eq!(projectiles.views()[0].destination, Point::new(15, 12));
        assert_eq!(projectiles.views()[0].duration_ms, 250);
        assert_eq!(projectiles.views()[0].frame, 2);
        // 逐帧走到到达（严格时钟下大跳不再一次追补到删除）。
        let arrival = tick_until_empty(&mut projectiles, &snapshot, 1_094, 31, 2_000, &mut sink);
        assert_eq!(arrival, 1_249);
        assert_eq!(projectiles.len(), 0);
        assert_eq!(projectiles.hit_effects().len(), 1);
        // 落点未知（125 没有落点坐标、缓存也没有）不生成实体，但仍算已消费（D-R59-1）。
        snapshot.projectile_events.push(fire_bounce(1, 999));
        projectiles.sync_with_sink(&snapshot, 1_300, &mut sink);
        assert_eq!(projectiles.events_consumed(), 2);
        assert_eq!(projectiles.len(), 0);
        // 源未知同样丢弃。
        snapshot.projectile_events.push(fire_bounce(999, 2));
        projectiles.sync_with_sink(&snapshot, 1_300, &mut sink);
        assert_eq!(projectiles.events_consumed(), 3);
        assert_eq!(projectiles.len(), 0);
        // 事件表被重置（同代号重建）：游标归零，同一事件可以再次生成。
        let mut fresh = snapshot_with(Point::new(10, 10), Point::new(10, 11));
        fresh.projectile_events.push(fire_bounce(1, 2));
        projectiles.sync_with_sink(&fresh, 1_400, &mut sink);
        assert_eq!(projectiles.events_consumed(), 1);
        assert_eq!(projectiles.len(), 1);
    }

    #[test]
    fn hit_effect_covers_all_ten_magic_frames() {
        // GameScene.cs:4708 的 new Effect(Magic, 570, 10, 600)：10 帧、600ms，每帧 60ms。
        assert_eq!(HIT_EFFECT_BASE_INDEX, 570);
        assert_eq!(HIT_EFFECT_FRAME_COUNT, 10);
        assert_eq!(HIT_EFFECT_DURATION_MS, 600);
        assert_eq!(HIT_EFFECT_FRAME_MS, 60);
        let frames: Vec<u32> = (0..HIT_EFFECT_FRAME_COUNT)
            .map(|frame| hit_effect_index(u64::from(frame) * HIT_EFFECT_FRAME_MS))
            .collect();
        assert_eq!(frames, (570..=579).collect::<Vec<u32>>());
        assert_eq!(hit_effect_index(0), 570);
        assert_eq!(hit_effect_index(59), 570);
        assert_eq!(hit_effect_index(60), 571);
        assert_eq!(hit_effect_index(540), 579);
        assert_eq!(hit_effect_index(599), 579);
        // 600ms 已播完（Effect.Process 会先 Remove）；取模只为让公式不越界。
        assert_eq!(hit_effect_index(HIT_EFFECT_DURATION_MS), 570);
    }

    #[test]
    fn hit_effect_starts_on_arrival_and_follows_target() {
        let mut snapshot = snapshot_with(Point::new(10, 10), Point::new(15, 10));
        snapshot.projectile_events.push(fire_bounce(1, 2));
        let mut projectiles = Projectiles::default();
        projectiles.sync(&snapshot, 1_000);
        assert_eq!(projectiles.len(), 1);
        assert!(projectiles.hit_effects().is_empty());
        assert_eq!(projectiles.sound_cues(), ["M34-1.wav"].as_slice());
        // 未到达（frame 2/8）不出命中特效。
        projectiles.sync(&snapshot, 1_032);
        projectiles.sync(&snapshot, 1_063);
        assert_eq!(projectiles.views()[0].frame, 2);
        assert!(projectiles.hit_effects().is_empty());
        // 到达前目标走开一格：570 特效挂在目标身上（Effect.Owner = Target）。
        snapshot.entities.get_mut(&2).expect("target").position = Point::new(15, 11);
        let mut arrival = 1_094;
        while !projectiles.is_empty() && arrival < 2_000 {
            projectiles.sync(&snapshot, arrival);
            if !projectiles.is_empty() {
                arrival += 31;
            }
        }
        assert_eq!(arrival, 1_249);
        let hit = projectiles.hit_effects();
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].target_id, 2);
        assert_eq!(hit[0].position, Point::new(15, 11));
        assert_eq!(hit[0].started_ms, arrival);
        // 严格时钟（R63 G1）：帧号只由 `NextFrame` 状态机推进，绘制查询是纯读。
        assert_eq!(hit[0].frame, 0);
        assert_eq!(hit[0].sprite_index(arrival), 570);
        assert_eq!(
            hit[0].sprite_index(arrival + 540),
            570,
            "绘制查询不得推进时钟"
        );
        assert!(!hit[0].finished(arrival + 600));
        // 大跳 599ms 只前进一步（不按 elapsed 追补），特效仍在。
        projectiles.sync(&snapshot, arrival + 599);
        assert_eq!(projectiles.hit_effects().len(), 1);
        assert_eq!(projectiles.hit_effects()[0].frame, 1);
        // 严格时钟下逐边界推进，第 10 次越过帧边界才删除。
        let mut now = arrival + 660;
        let mut ticks = 0;
        while !projectiles.hit_effects().is_empty() && ticks < 20 {
            projectiles.sync(&snapshot, now);
            now += 60;
            ticks += 1;
        }
        assert!(projectiles.hit_effects().is_empty(), "10 帧播完即删除");
        assert_eq!(
            projectiles.sound_cues(),
            ["M34-1.wav", "M34-2.wav"].as_slice()
        );
    }

    #[test]
    fn dead_or_unknown_target_skips_hit_effect() {
        // 目标已死亡：Crystal GameScene.cs:4707 直接 return，570 与命中音都不播。
        let mut snapshot = snapshot_with(Point::new(10, 10), Point::new(15, 10));
        snapshot.projectile_events.push(fire_bounce(1, 2));
        let mut projectiles = Projectiles::default();
        let mut sink = FakeSink::default();
        projectiles.sync_with_sink(&snapshot, 1_000, &mut sink);
        snapshot.entities.get_mut(&2).expect("target").dead = true;
        tick_until_empty(&mut projectiles, &snapshot, 1_032, 31, 2_000, &mut sink);
        assert!(projectiles.is_empty());
        assert!(projectiles.hit_effects().is_empty());
        assert_eq!(cue_count(&sink, "M34-1.wav"), 1);
        assert_eq!(cue_count(&sink, "M34-2.wav"), 0);
        // 目标从快照消失（:4701 的 missile.Target == null）：同样没有命中表现。
        let mut snapshot = snapshot_with(Point::new(10, 10), Point::new(15, 10));
        snapshot.projectile_events.push(fire_bounce(1, 2));
        let mut projectiles = Projectiles::default();
        let mut sink = FakeSink::default();
        projectiles.sync_with_sink(&snapshot, 2_000, &mut sink);
        snapshot.entities.remove(&2);
        tick_until_empty(&mut projectiles, &snapshot, 2_032, 31, 3_000, &mut sink);
        assert!(projectiles.is_empty());
        assert!(projectiles.hit_effects().is_empty());
        assert_eq!(cue_count(&sink, "M34-2.wav"), 0);
    }

    #[test]
    fn spawn_and_hit_sounds_are_m34_once_each() {
        let mut snapshot = snapshot_with(Point::new(10, 10), Point::new(15, 10));
        snapshot.projectile_events.push(fire_bounce(1, 2));
        let mut projectiles = Projectiles::default();
        let mut sink = FakeSink::default();
        projectiles.sync_with_sink(&snapshot, 1_000, &mut sink);
        // 同一快照重复同步：出生音只播一次。
        projectiles.sync_with_sink(&snapshot, 1_020, &mut sink);
        assert_eq!(sink.played, ["M34-1.wav"].as_slice());
        tick_until_empty(&mut projectiles, &snapshot, 1_032, 31, 2_000, &mut sink);
        assert_eq!(sink.played, ["M34-1.wav", "M34-2.wav"].as_slice());
        // 实体删除后再同步不会重复命中音。
        projectiles.sync_with_sink(&snapshot, 2_100, &mut sink);
        assert_eq!(sink.played.len(), 2);
        assert_eq!(
            (cue_count(&sink, "M34-1.wav"), cue_count(&sink, "M34-2.wav")),
            (1, 1)
        );
        // 生产路径（无 fake sink）同样把两条 cue 记进本地队列。
        let mut projectiles = Projectiles::default();
        projectiles.sync(&snapshot, 2_500);
        // 生产路径的命中 cue 也进内部队列：逐帧推进直到到达。
        let mut now = 2_532;
        while !projectiles.is_empty() && now < 4_000 {
            projectiles.sync(&snapshot, now);
            now += 31;
        }
        assert_eq!(
            projectiles.sound_cues(),
            ["M34-1.wav", "M34-2.wav"].as_slice()
        );
    }

    #[test]
    fn unknown_destination_generates_no_projectile_or_sound() {
        // C2 / D-R59-1：目标从未进过可信缓存、源/落点解析不到 → 不生成、不出声。
        let mut snapshot = snapshot_with(Point::new(10, 10), Point::new(15, 10));
        snapshot.projectile_events.push(fire_bounce(1, 999));
        let mut projectiles = Projectiles::default();
        let mut sink = FakeSink::default();
        projectiles.sync_with_sink(&snapshot, 1_000, &mut sink);
        assert!(projectiles.is_empty());
        assert!(projectiles.hit_effects().is_empty());
        assert!(sink.played.is_empty());
        assert!(projectiles.sound_cues().is_empty());
        // 源未知同样无声。
        snapshot.projectile_events.push(fire_bounce(999, 2));
        projectiles.sync_with_sink(&snapshot, 1_100, &mut sink);
        assert!(sink.played.is_empty());
        assert_eq!(projectiles.events_consumed(), 2);
    }

    /// C2：目标离开视野后，125 用「可信客户端缓存」（最后已知位置）兜底；从未见过
    /// 的对象仍不生成。Crystal `PlayerObject.cs:3546-3556` 的 `TargetPoint` 同类语义。
    #[test]
    fn c2_target_point_falls_back_to_last_known_client_cache() {
        let mut snapshot = snapshot_with(Point::new(10, 10), Point::new(15, 10));
        let mut projectiles = Projectiles::default();
        let mut sink = FakeSink::default();
        // 先同步一帧（无事件）把目标位置写进缓存。
        projectiles.sync_with_sink(&snapshot, 1_000, &mut sink);
        assert!(sink.played.is_empty());
        assert!(projectiles.sound_cues().is_empty());
        // 目标离开视野，随后 125 仍然到达：用缓存落点生成。
        snapshot.entities.remove(&2);
        snapshot.projectile_events.push(fire_bounce(1, 2));
        projectiles.sync_with_sink(&snapshot, 1_100, &mut sink);
        assert_eq!(projectiles.len(), 1);
        assert_eq!(projectiles.views()[0].destination, Point::new(15, 10));
        assert_eq!(sink.played, ["M34-1.wav"].as_slice());
        // 缓存里也没有的目标：不生成、不改 wire。
        snapshot.projectile_events.push(fire_bounce(1, 999));
        projectiles.sync_with_sink(&snapshot, 1_200, &mut sink);
        assert_eq!(projectiles.len(), 1);
        assert_eq!(projectiles.events_consumed(), 2);
    }

    /// C3：像素 offset 是运动量（屏幕坐标 = 格投影 + 该 offset，只叠加一次），静态目标
    /// 每轴单调、无越界、终点残差为零。
    #[test]
    fn c3_motion_offset_adds_once_and_is_monotonic() {
        let source = Point::new(10, 10);
        let destination = Point::new(15, 13);
        let (dx, dy) = (destination.x - source.x, destination.y - source.y);
        let mut previous = draw_offset(source, destination, 0, 8);
        assert_eq!(previous, (0, 0));
        for frame in 1..=8 {
            let offset = draw_offset(source, destination, frame, 8);
            assert!(offset.0 >= previous.0, "x 轴单调 ({frame})");
            assert!(offset.1 >= previous.1, "y 轴单调 ({frame})");
            assert!(offset.0 <= dx * 48, "x 不越过终点 ({frame})");
            assert!(offset.1 <= dy * 32, "y 不越过终点 ({frame})");
            previous = offset;
        }
        assert_eq!(
            draw_offset(source, destination, 8, 8),
            (dx * 48, dy * 32),
            "终点残差为零"
        );
        // 负向目标：每轴单调递减、不越过终点。
        let negative = Point::new(7, 6);
        let (nx, ny) = (negative.x - source.x, negative.y - source.y);
        let mut previous = draw_offset(source, negative, 0, 8);
        for frame in 1..=8 {
            let offset = draw_offset(source, negative, frame, 8);
            assert!(offset.0 <= previous.0, "负向 x 单调 ({frame})");
            assert!(offset.1 <= previous.1, "负向 y 单调 ({frame})");
            assert!(offset.0 >= nx * 48, "负向 x 不越过终点 ({frame})");
            assert!(offset.1 >= ny * 32, "负向 y 不越过终点 ({frame})");
            previous = offset;
        }
        assert_eq!(draw_offset(source, negative, 8, 8), (nx * 48, ny * 32));
        // 零距离。
        assert_eq!(draw_offset(source, source, 0, 1), (0, 0));
        assert_eq!(draw_offset(source, source, 1, 1), (0, 0));
        // draw_location 只叠加一次：origin 已是格投影（含 camera/actor 偏移）。
        let mut view = make_view(source, destination, 1_000);
        view.frame = 4;
        let origin = (100, 200);
        let offset = view.offset();
        assert_eq!(
            view.draw_location(origin),
            (origin.0 + offset.0, origin.1 + offset.1)
        );
        let once = view.draw_location(origin);
        assert_ne!(
            once,
            (once.0 + offset.0, once.1 + offset.1),
            "重复叠加 offset 必须是错误路径"
        );
    }

    /// C3/C4：同代号裁剪空表不得复位；新代号首批非空恰复位一次并清旧 view/hit/cue；
    /// 新代号空快照等首批载荷、不提前复位。
    #[test]
    fn c4_generation_resets_exactly_once_and_keeps_cursor() {
        let mut snapshot = multi_target_snapshot();
        push_event(&mut snapshot, 2);
        let mut projectiles = Projectiles::default();
        let mut sink = FakeSink::default();
        projectiles.sync_with_sink(&snapshot, 1_000, &mut sink);
        assert_eq!(projectiles.events_consumed(), 1);
        assert_eq!(projectiles.len(), 1);
        // 同代号的空表（滞后克隆/裁剪）：游标与视图都不得复位。
        let empty = WorldSnapshot::default();
        projectiles.sync_with_sink(&empty, 1_010, &mut sink);
        assert_eq!(projectiles.events_consumed(), 1, "同代号空表不得复位游标");
        assert_eq!(projectiles.len(), 1, "同代号空表不得清在飞实体");
        // 新代号首批非空：恰复位一次，清旧视图/位置缓存。
        let mut fresh = multi_target_snapshot();
        fresh.session_generation = 1;
        push_event(&mut fresh, 3);
        projectiles.sync_with_sink(&fresh, 1_100, &mut sink);
        assert_eq!(projectiles.events_consumed(), 1);
        assert_eq!(projectiles.len(), 1);
        assert_eq!(projectiles.views()[0].destination_id, 3);
        assert!(
            projectiles
                .views()
                .iter()
                .all(|view| view.destination_id != 2)
        );
        // 同代号再次同步：不重复复位，也不重放。
        projectiles.sync_with_sink(&fresh, 1_110, &mut sink);
        assert_eq!(projectiles.events_consumed(), 1);
        assert_eq!(projectiles.len(), 1);
        // 新代号空快照（重连后首个快照）：等首批载荷，不提前复位。
        let mut empty_new = multi_target_snapshot();
        empty_new.session_generation = 2;
        projectiles.sync_with_sink(&empty_new, 1_120, &mut sink);
        assert_eq!(
            projectiles.events_consumed(),
            1,
            "新代号空表等首批非空事件才复位"
        );
        assert_eq!(projectiles.len(), 1);
        // 复位同时清旧会话遗留的音效 cue（保留本代新 cue）。
        let mut producer = Projectiles::default();
        let mut first = multi_target_snapshot();
        push_event(&mut first, 2);
        producer.sync(&first, 2_000);
        assert_eq!(producer.sound_cues(), ["M34-1.wav"].as_slice());
        let mut second = multi_target_snapshot();
        second.session_generation = 1;
        push_event(&mut second, 3);
        producer.sync(&second, 2_050);
        assert_eq!(
            producer.sound_cues(),
            ["M34-1.wav"].as_slice(),
            "旧会话 cue 被清，本代 spawn cue 保留一条"
        );
    }

    // -----------------------------------------------------------------------
    // R61 M1 (D1): the projectile event table is append-only, no 16-entry ring.
    // -----------------------------------------------------------------------

    /// Snapshot with entities `1..=20` on distinct tiles (`1` = caster, `2..=20` targets),
    /// so a view's `(destination_id, destination)` payload identifies exactly one event.
    fn multi_target_snapshot() -> WorldSnapshot {
        let mut snapshot = WorldSnapshot::default();
        for id in 1..=20u32 {
            let x = i32::try_from(id).expect("test object id fits in i32") + 10;
            snapshot.entities.insert(
                id,
                WorldEntity::new(
                    id,
                    EntityKind::Monster,
                    format!("object-{id}"),
                    Point::new(x, 10),
                    MirDirection::Right,
                    0,
                ),
            );
        }
        snapshot
    }

    /// Append one event exactly like the real session path (`net.rs::push_projectile_event`):
    /// no cap, no drop-oldest, and the absolute sequence advances on every push.
    fn push_event(snapshot: &mut WorldSnapshot, destination: u32) {
        snapshot.projectile_events.push(fire_bounce(1, destination));
        snapshot.projectile_events_total = snapshot.projectile_events_total.saturating_add(1);
    }

    #[test]
    fn r61_backlog_is_consumed_without_loss() {
        let mut snapshot = multi_target_snapshot();
        for destination in 2..=18u32 {
            push_event(&mut snapshot, destination);
        }
        assert_eq!(
            snapshot.projectile_events_total, 17,
            "every accepted push must advance the absolute counter"
        );
        assert_eq!(snapshot.projectile_events.len(), 17);

        let mut projectiles = Projectiles::default();
        let mut sink = FakeSink::default();
        projectiles.sync_with_sink(&snapshot, 1_000, &mut sink);
        assert_eq!(projectiles.events_consumed(), 17);
        assert_eq!(projectiles.views().len(), 17);
        let payloads: Vec<u32> = projectiles
            .views()
            .iter()
            .map(|view| view.destination_id)
            .collect();
        assert_eq!(
            payloads,
            (2..=18u32).collect::<Vec<u32>>(),
            "arrival order preserved with no gap and no duplicate"
        );
        assert_eq!(sink.played, vec!["M34-1.wav"; 17]);

        // 同一快照同一时刻再次同步：不重放、不重复出 cue。
        projectiles.sync_with_sink(&snapshot, 1_000, &mut sink);
        assert_eq!(projectiles.events_consumed(), 17);
        assert_eq!(projectiles.views().len(), 17);
        assert_eq!(sink.played.len(), 17);

        // 每个视图恰好结算一次，包括最老的一个（R60 的 ring 在 UI 消费前就丢弃了它）。
        run_ticks(&mut projectiles, &snapshot, 1_010, 3_000, 10, &mut sink);
        assert!(projectiles.views().is_empty());
        assert_eq!(
            (cue_count(&sink, "M34-1.wav"), cue_count(&sink, "M34-2.wav")),
            (17, 17)
        );

        // 再次同步既不重新结算也不重播命中 cue。
        projectiles.sync_with_sink(&snapshot, 3_010, &mut sink);
        assert_eq!(cue_count(&sink, "M34-2.wav"), 17);
    }

    #[test]
    fn r61_appending_while_consuming_keeps_each_event_once() {
        let mut snapshot = multi_target_snapshot();
        for destination in 2..=18u32 {
            push_event(&mut snapshot, destination);
        }
        let mut projectiles = Projectiles::default();
        let mut sink = FakeSink::default();
        projectiles.sync_with_sink(&snapshot, 1_000, &mut sink);
        assert_eq!(projectiles.events_consumed(), 17);

        // 新事件在游标已经消费第一批之后到达：append-only 表同时保留两批，
        // 游标继续前进即可（不再需要 R60 ring 的位移补偿）。
        for destination in 19..=20u32 {
            push_event(&mut snapshot, destination);
        }
        assert_eq!(snapshot.projectile_events_total, 19);
        assert_eq!(snapshot.projectile_events.len(), 19);
        projectiles.sync_with_sink(&snapshot, 1_010, &mut sink);
        assert_eq!(projectiles.events_consumed(), 19);
        let payloads: Vec<u32> = projectiles
            .views()
            .iter()
            .map(|view| view.destination_id)
            .collect();
        assert_eq!(
            payloads,
            (2..=20u32).collect::<Vec<u32>>(),
            "only the new events spawn new views (17 + 2, no duplicate)"
        );
        // 逐帧推进：每个事件恰好结算一次。
        run_ticks(&mut projectiles, &snapshot, 1_020, 3_000, 10, &mut sink);
        assert!(projectiles.views().is_empty());
        assert_eq!(
            (cue_count(&sink, "M34-1.wav"), cue_count(&sink, "M34-2.wav")),
            (19, 19)
        );
        projectiles.sync_with_sink(&snapshot, 3_010, &mut sink);
        assert_eq!(projectiles.events_consumed(), 19);
        assert_eq!(cue_count(&sink, "M34-2.wav"), 19);
    }

    #[test]
    fn r61_rebuilt_snapshot_resets_cursor_without_replay() {
        let mut snapshot = multi_target_snapshot();
        push_event(&mut snapshot, 2);
        let mut projectiles = Projectiles::default();
        let mut sink = FakeSink::default();
        projectiles.sync_with_sink(&snapshot, 1_000, &mut sink);
        assert_eq!(projectiles.events_consumed(), 1);
        assert_eq!(projectiles.views().len(), 1);
        tick_until_empty(&mut projectiles, &snapshot, 1_010, 10, 3_000, &mut sink);
        assert!(projectiles.views().is_empty(), "the one view has finished");

        // Reconnect/rebuilt snapshot: new session generation, empty event table.
        let mut rebuilt = multi_target_snapshot();
        rebuilt.session_generation = 1;
        projectiles.sync_with_sink(&rebuilt, 3_000, &mut sink);
        assert_eq!(
            projectiles.events_consumed(),
            1,
            "a new-generation empty snapshot waits for the first payload"
        );
        assert!(projectiles.views().is_empty());

        // 重建后的第一条事件只消费一次，不会按旧游标重放/跳过。
        push_event(&mut rebuilt, 3);
        projectiles.sync_with_sink(&rebuilt, 3_010, &mut sink);
        assert_eq!(projectiles.events_consumed(), 1);
        assert_eq!(projectiles.views().len(), 1);
        assert_eq!(projectiles.views()[0].destination_id, 3);
    }

    #[test]
    fn r61_consecutive_consumes_do_not_duplicate_payloads() {
        let mut snapshot = multi_target_snapshot();
        for destination in 2..=4u32 {
            push_event(&mut snapshot, destination);
        }
        let mut projectiles = Projectiles::default();
        let mut sink = FakeSink::default();
        projectiles.sync_with_sink(&snapshot, 1_000, &mut sink);
        // Below one frame interval: every view is still in flight (`frame == 0`).
        projectiles.sync_with_sink(&snapshot, 1_010, &mut sink);
        assert_eq!(projectiles.events_consumed(), 3);
        let payloads: Vec<(u32, Point)> = projectiles
            .views()
            .iter()
            .map(|view| (view.destination_id, view.destination))
            .collect();
        assert_eq!(
            payloads,
            vec![
                (2, Point::new(12, 10)),
                (3, Point::new(13, 10)),
                (4, Point::new(14, 10)),
            ],
            "consecutive consumes keep arrival order and add no duplicate payload"
        );
        let mut ids: Vec<u32> = payloads.iter().map(|(id, _)| *id).collect();
        ids.sort_unstable();
        let mut unique = ids.clone();
        unique.dedup();
        assert_eq!(unique, ids, "no payload may appear twice");
        // A further sync without new events is a no-op.
        projectiles.sync_with_sink(&snapshot, 1_015, &mut sink);
        assert_eq!(projectiles.events_consumed(), 3);
        assert_eq!(projectiles.views().len(), 3);
    }

    #[test]
    fn r61_dynamic_flight_parameters_follow_the_target() {
        // 6 格：300ms / count 10（interval 30）；同一时序内远移 9 格（450/15）仍飞，
        // 近移 3 格（150/5）立即到达（Crystal Effect.cs:185-187）。
        let mut snapshot = snapshot_with(Point::new(10, 10), Point::new(16, 10));
        push_event(&mut snapshot, 2);
        let mut projectiles = Projectiles::default();
        let mut sink = FakeSink::default();
        projectiles.sync_with_sink(&snapshot, 1_000, &mut sink);
        assert_eq!(projectiles.views().len(), 1);
        assert_eq!(projectiles.views()[0].next_frame_ms, 1_030);
        // 逐帧到 frame 6（每个边界只进一帧）。
        for tick in 0..6 {
            projectiles.sync_with_sink(&snapshot, 1_031 + tick * 30, &mut sink);
        }
        let view = projectiles.views()[0];
        assert_eq!(
            (view.duration_ms, view.frame_count, view.frame),
            (300, 10, 6)
        );

        snapshot.entities.get_mut(&2).expect("target").position = Point::new(19, 16);
        projectiles.sync_with_sink(&snapshot, 1_181, &mut sink);
        let view = projectiles.views()[0];
        assert_eq!(view.destination, Point::new(19, 16));
        assert_eq!((view.duration_ms, view.frame_count), (450, 15));
        assert_eq!(view.frame, 6);
        assert_eq!(view.dir16, 4, "direction stays a spawn-time property");

        // 目标远离使 Count 变大：继续按同一状态机走帧。
        for tick in 0..4 {
            projectiles.sync_with_sink(&snapshot, 1_211 + tick * 30, &mut sink);
        }
        assert_eq!(projectiles.views().len(), 1, "10 < 15 frames: still flying");
        assert_eq!(projectiles.views()[0].frame, 10);

        // 目标近移 3 格（150/5）：下一次合法 Process 才结算（不按 elapsed 追补）。
        snapshot.entities.get_mut(&2).expect("target").position = Point::new(13, 10);
        projectiles.sync_with_sink(&snapshot, 1_301, &mut sink);
        assert_eq!(projectiles.views().len(), 1);
        assert_eq!(projectiles.views()[0].frame_count, 5);
        assert_eq!(projectiles.views()[0].frame, 10);
        projectiles.sync_with_sink(&snapshot, 1_331, &mut sink);
        assert_eq!(
            projectiles.views().len(),
            0,
            "frame 10 >= count 5: settles on the next Process tick"
        );
        assert_eq!(projectiles.hit_effects().len(), 1);
        assert_eq!(projectiles.hit_effects()[0].position, Point::new(13, 10));
        assert_eq!(sink.played, vec!["M34-1.wav", "M34-2.wav"]);

        // 同一时刻重复同步不会二次结算。
        projectiles.sync_with_sink(&snapshot, 1_331, &mut sink);
        assert_eq!(projectiles.hit_effects().len(), 1);
        assert_eq!(sink.played.len(), 2);
    }

    // -----------------------------------------------------------------------
    // R62 A (C1): strict NextFrame clock, no elapsed catch-up.
    // -----------------------------------------------------------------------

    /// Red baseline (uses only the R61 API): a strict `NextFrame` clock advances at
    /// most one frame per `Process` tick, so a 29-second jump may not catch up to the
    /// last frame / deletion. The R61 elapsed formula does catch up, which is exactly
    /// the behaviour C1 says to remove.
    #[test]
    fn r62_clock_advances_at_most_one_frame_per_tick_not_elapsed_catch_up() {
        let mut snapshot = snapshot_with(Point::new(10, 10), Point::new(15, 10));
        snapshot.projectile_events.push(fire_bounce(1, 2));
        let mut projectiles = Projectiles::default();
        projectiles.sync(&snapshot, 1_000);
        assert_eq!(projectiles.len(), 1);
        projectiles.sync(&snapshot, 30_000);
        assert_eq!(
            projectiles.len(),
            1,
            "one tick after a 29 s jump must still be in flight"
        );
        assert_eq!(
            projectiles.views()[0].frame,
            1,
            "at most one frame per tick"
        );
    }

    // -----------------------------------------------------------------------
    // R63 A (G1/G2)
    // -----------------------------------------------------------------------

    /// 红探针（R63 A）：现实现用 elapsed 反推帧号，一次 600ms 的 `sync` 会把 570 特效
    /// 直接删掉；严格状态时钟下大跳至多前进一帧，特效必须仍在。
    #[test]
    fn round63_hit_elapsed_catch_up_is_gone_probe() {
        // 零距离投射物：duration 0 / count 1，spawn 后下一个毫秒结算并挂命中特效。
        let mut snapshot = snapshot_with(Point::new(10, 10), Point::new(10, 10));
        snapshot.projectile_events.push(fire_bounce(1, 2));
        let mut projectiles = Projectiles::default();
        projectiles.sync(&snapshot, 1_000);
        assert_eq!(projectiles.len(), 1);
        projectiles.sync(&snapshot, 1_001);
        assert_eq!(projectiles.hit_effects().len(), 1);
        let started = projectiles.hit_effects()[0].started_ms;
        assert_eq!(started, 1_001);
        // 大跳 600ms：严格时钟只允许一次 `Process`（至多一帧），不得按 elapsed 删除。
        projectiles.sync(&snapshot, started + 600);
        assert_eq!(
            projectiles.hit_effects().len(),
            1,
            "严格时钟：大跳不追补，不按 elapsed 删除命中特效"
        );
    }

    /// R63 G1 固定预期表（Crystal `Effect.cs:38-48,81-96`）：`NextFrame = Start + 60*(Frame+1)`，
    /// 等号不进、超出才 +1、第 10 次越过边界才 Remove。
    #[test]
    fn round63_hit_clock_strict_boundary_and_completion() {
        let mut effect = HitEffectView::new(2, Point::new(15, 10), 1_000);
        assert_eq!((effect.frame, effect.next_frame_ms), (0, 1_060));
        assert!(!effect.advance_clock(1_060), "now <= NextFrame 保持旧帧");
        assert_eq!((effect.frame, effect.sprite_index(1_060)), (0, 570));
        assert!(!effect.advance_clock(1_061));
        assert_eq!((effect.frame, effect.sprite_index(1_061)), (1, 571));
        assert!(!effect.advance_clock(1_061), "同刻重复不推进");
        assert_eq!(
            (effect.frame, effect.next_frame_ms),
            (1, 1_120),
            "NextFrame = Start + 60*(Frame+1)"
        );
        assert!(!effect.advance_clock(1_120), "等号不进");
        assert_eq!((effect.frame, effect.sprite_index(1_120)), (1, 571));
        assert!(!effect.advance_clock(1_121));
        assert_eq!((effect.frame, effect.sprite_index(1_121)), (2, 572));
        // 逐边界推进到最后一帧 9（579），下一个边界 1600。
        let mut now = 1_121;
        while effect.frame < 9 {
            now += 60;
            assert!(!effect.advance_clock(now), "frame {now} 仍在播");
        }
        assert_eq!((now, effect.frame, effect.next_frame_ms), (1_541, 9, 1_600));
        assert_eq!(effect.sprite_index(1_541), 579);
        assert!(!effect.advance_clock(1_600), "1600 等号不进");
        assert_eq!((effect.frame, effect.sprite_index(1_600)), (9, 579));
        assert!(!effect.finished(1_600));
        assert!(effect.advance_clock(1_601), "1601 越过最后边界 → Remove");
        assert!(!effect.advance_clock(1_601), "同刻只删除一次");
        assert_eq!(effect.frame, 9, "删除前不写入越界帧号");
        assert!(
            !effect.finished(1_601),
            "未写入越界帧号，生命周期由 advance_clock 返回决定"
        );
    }

    /// R63 G1：大跳只前进一帧、不追补 elapsed；同刻重复 sync 不再进；绘制查询纯读。
    #[test]
    fn round63_hit_large_jump_advances_once_and_draw_is_pure() {
        let mut effect = HitEffectView::new(2, Point::new(15, 10), 1_000);
        assert!(!effect.advance_clock(11_000), "大跳只前进一帧");
        assert_eq!((effect.frame, effect.next_frame_ms), (1, 1_120));
        // 同刻重复 sync：不再推进。
        assert!(!effect.advance_clock(11_000));
        assert_eq!(effect.frame, 1);
        // 连续绘制查询是纯读：不推进时钟、不改变帧号。
        for _ in 0..5 {
            assert_eq!(effect.sprite_index(11_000), 571);
            assert_eq!(effect.sprite_index(200_000), 571);
            assert!(!effect.finished(200_000));
        }
        assert_eq!(
            (effect.frame, effect.next_frame_ms, effect.last_tick_ms),
            (1, 1_120, 11_000)
        );
        // 下一毫秒（> last_tick 且 > next）再进一帧，仍不补其它边界。
        assert!(!effect.advance_clock(11_001));
        assert_eq!((effect.frame, effect.next_frame_ms), (2, 1_180));
    }

    /// R63 G1：`SkipFrames` 到期先额外 +1 再普通 +1（Effect.cs:83-85）；到/越过 10 恰移除。
    #[test]
    fn round63_hit_skip_adds_one_extra_frame() {
        let mut skip = HitEffectView::new(2, Point::new(15, 10), 1_000);
        skip.skip_frames = true;
        assert!(!skip.advance_clock(1_060), "未到边界不推进");
        assert_eq!(skip.frame, 0);
        assert!(!skip.advance_clock(1_061));
        assert_eq!((skip.frame, skip.next_frame_ms), (2, 1_180));
        assert!(!skip.advance_clock(1_061), "同刻不再进");
        assert_eq!(skip.frame, 2);
        // 非 skip 同一时刻只 +1（对照）。
        let mut plain = HitEffectView::new(2, Point::new(15, 10), 1_000);
        assert!(!plain.advance_clock(1_061));
        assert_eq!((plain.frame, plain.next_frame_ms), (1, 1_120));
        // skip 逐边界 2 → 4 → 6 → 8，下一次越过 +2 >= 10 恰好移除一次。
        for now in [1_181_i64, 1_301, 1_421] {
            assert!(!skip.advance_clock(now), "frame 4/6/8 仍在播");
        }
        assert_eq!((skip.frame, skip.next_frame_ms), (8, 1_540));
        assert!(skip.advance_clock(1_541), "8 + 2 >= 10 → Remove");
        assert!(!skip.advance_clock(1_541), "同刻只移除一次");
        assert_eq!(skip.frame, 8, "删除前不写入越界帧号");
    }

    /// R63 G1：时钟回退/同刻为 no-op（不推进），之后有效 tick 可继续。
    #[test]
    fn round63_hit_clock_rollback_is_noop() {
        let mut effect = HitEffectView::new(2, Point::new(15, 10), 1_000);
        assert!(!effect.advance_clock(900), "早于 started：不推进");
        assert_eq!((effect.frame, effect.last_tick_ms), (0, 1_000));
        assert!(!effect.advance_clock(1_061));
        assert_eq!((effect.frame, effect.last_tick_ms), (1, 1_061));
        // 回退与同刻都不进。
        assert!(!effect.advance_clock(1_100));
        assert!(!effect.advance_clock(1_061));
        assert_eq!(
            (effect.frame, effect.next_frame_ms, effect.last_tick_ms),
            (1, 1_120, 1_061)
        );
        // 之后的有效 tick 可以继续。
        assert!(!effect.advance_clock(1_121));
        assert_eq!((effect.frame, effect.next_frame_ms), (2, 1_180));
    }

    fn test_entity(id: u32, position: Point) -> WorldEntity {
        WorldEntity::new(
            id,
            EntityKind::Monster,
            format!("object-{id}"),
            position,
            MirDirection::Right,
            0,
        )
    }

    /// R63 G2：新代号首批载荷里「可见但未被 spawn 引用」的对象位置必须留在缓存里，
    /// 供后续同代 125 在对象缺席时兜底；复位必须清旧代号缓存（不复活 99）。
    #[test]
    fn round63_first_payload_positions_survive_generation_reset() {
        let mut projectiles = Projectiles::default();
        let mut sink = FakeSink::default();
        // 旧代号（0）：只把 99 的位置写进缓存，没有事件。
        let mut old = WorldSnapshot::default();
        old.entities.insert(99, test_entity(99, Point::new(5, 5)));
        projectiles.sync_with_sink(&old, 1_000, &mut sink);
        assert!(projectiles.is_empty());
        // 新代号首批非空载荷：7/8/9 可见，125 只打 8（9 未被 spawn 引用）。
        let mut first = WorldSnapshot {
            session_generation: 1,
            ..WorldSnapshot::default()
        };
        first.entities.insert(7, test_entity(7, Point::new(10, 10)));
        first.entities.insert(8, test_entity(8, Point::new(20, 20)));
        first.entities.insert(9, test_entity(9, Point::new(30, 30)));
        first.projectile_events.push(fire_bounce(7, 8));
        first.projectile_events_total = 1;
        projectiles.sync_with_sink(&first, 1_100, &mut sink);
        assert_eq!(projectiles.views().len(), 1);
        assert_eq!(projectiles.views()[0].destination, Point::new(20, 20));
        // 下一同代快照：9 已不在可见集合，新 125 打 9 → 必须用首批缓存位置生成。
        let mut second = WorldSnapshot {
            session_generation: 1,
            ..WorldSnapshot::default()
        };
        second
            .entities
            .insert(7, test_entity(7, Point::new(10, 10)));
        second
            .entities
            .insert(8, test_entity(8, Point::new(20, 20)));
        second.projectile_events.push(fire_bounce(7, 8));
        second.projectile_events.push(fire_bounce(8, 9));
        second.projectile_events_total = 2;
        projectiles.sync_with_sink(&second, 1_200, &mut sink);
        assert_eq!(
            projectiles.views().len(),
            2,
            "首批可见但未被 spawn 引用的 9 必须保留位置缓存"
        );
        let hit9 = projectiles.views()[1];
        assert_eq!(hit9.destination_id, 9);
        assert_eq!(hit9.destination, Point::new(30, 30));
        // 复位清掉旧代号缓存：旧代见过的 99 不复活（否则会多生成一个 view）。
        second.projectile_events.push(fire_bounce(8, 99));
        second.projectile_events_total = 3;
        projectiles.sync_with_sink(&second, 1_300, &mut sink);
        assert_eq!(
            projectiles.views().len(),
            2,
            "旧代号缓存必须在 reset 中清掉，99 不得复活"
        );
        assert_eq!(projectiles.events_consumed(), 3, "未生成的事件仍计入游标");
    }
}
