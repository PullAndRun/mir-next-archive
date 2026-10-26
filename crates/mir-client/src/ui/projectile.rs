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
//! * **D-R59-1**：Crystal `GameScene.cs:4699` 无条件 `CreateProjectile`（落点在
//!   `CreateProjectile` 里可用 `TargetPoint` 兜底，`:4701` 才用 `missile.Target` 挂命中）。
//!   125 包只带 `source`/`destination` 两个对象 ID，`destination` 不在快照里就没有落点
//!   坐标，本作**不生成**实体（不崩溃、不伪造坐标）。
//! * Crystal `Effect.cs:174-181` 每帧重算 `Duration`/`Count`（目标移动会改变总时长）；
//!   本作在生成时冻结，目标当前位置只用于插值落点。
//! * `draw_effect` 不叠加 wzl 每帧的 `ImageInfo.offset`（Crystal `MLibrary.cs:658-673`
//!   的 `offSet=true`），属 mir-render 绘制层缺口，D1 不改 `mir-render`。
//!
//! # 已知限制
//!
//! `WorldSnapshot::projectile_events` 在会话侧是累计队列（`net.rs:7246-7260`，上限 16
//! 条后丢最旧），每个快照克隆都带着历史事件。只持 `&WorldSnapshot` 的绘制路径没有事件
//! ID 可用，只能按位置游标去重；队列满 16 条后「丢最旧 + 追加」从 `len` 上不可观测，
//! 可能漏掉新事件。彻底修复要在 `net.rs` 给事件编号，超出 D1 写范围，登记为残留风险。
//!
//! * 命中 570 特效缺素材时由 `scene.draw_effect` 静默跳过（返回 `false`，不 panic、
//!   不画占位块）；「逻辑错」与「缺素材」的区分：帧号/时序由 [`hit_effect_index`]
//!   的单测钉死，素材是否齐备由 D4 的素材用例单独判定（`game-assets/` 不入库）。
//! * G4 只做到「何时播什么文件」的纯数据层：文件名映射在 `mir_render::sound`，
//!   播放端由 [`ProjectileSoundSink`] 注入。生产路径把 cue 记进 [`RecordingSoundSink`]
//!   （[`Projectiles::take_sound_cues`]）；接 `app.rs` 的真实播放不在 R59 D2 写范围，
//!   真实可听记 **PARTIAL**（R59 计划 §五 R2）。

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
/// 音效 cue 队列上限（超出丢最旧；与 `net.rs` 的 `SOUND_EVENT_LIMIT` 同法，
/// 防止生产路径无人 drain 时无限增长）。
pub const SOUND_CUE_LIMIT: usize = 32;

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

/// 已飞行 `elapsed_ms` 毫秒时的帧号（向下取整）。
#[must_use]
pub fn frame_at(elapsed_ms: u64) -> u32 {
    u32::try_from(elapsed_ms / FRAME_INTERVAL_MS).unwrap_or(u32::MAX)
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

/// 命中特效在 `magic.wzl` 的帧号：`570 + (elapsed_ms / 60) % 10`。
///
/// Crystal `Effect.cs:53` 每帧 `Duration / Count = 600 / 10 = 60ms` 把 `CurrentFrame`
/// 加一，`Effect.cs:128`（基类 `Draw`）直接画 `BaseIndex + CurrentFrame`，所以 10 帧
/// 依次是 `570..=579`。`Effect.Process` 在 `CurrentFrame >= Count` 时删除，
/// 调用方应先过滤 [`HitEffectView::finished`]；这里的取模只为让公式本身对超时值
/// 仍落在 `570..=579`。
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
    /// 出生时刻（Unix 毫秒）。
    pub started_ms: i64,
    /// 飞行总时长（生成时冻结，见模块文档的偏离）。
    pub duration_ms: u64,
    /// 飞行帧数（`max(1, duration / 30)`）。
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

    /// 是否已到达/超时（下一帧就该删除）。
    #[must_use]
    pub fn finished(&self) -> bool {
        self.frame >= self.frame_count
    }
}

/// 一个正在播放的命中特效（Crystal `Effect` 的 `Owner` 形态子集）。
///
/// `GameScene.cs:4708` 的 `new Effect(Libraries.Magic, 570, 10, 600, sender.Target)`
/// 走 `Effect.cs:81-88` 的 `owner` 构造函数：`Owner = Target`、`Source = Owner.CurrentLocation`，
/// 基类 `Draw` 在 `Owner != null` 时每帧取 `Owner.DrawLocation`（`Effect.cs:117-119`），
/// 所以本结构每帧重取目标当前位置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HitEffectView {
    /// 目标对象 ID（每帧用它重取当前位置）。
    pub target_id: u32,
    /// 最近一帧的目标格（目标消失后停在最后已知位置）。
    pub position: Point,
    /// 起始时刻（Unix 毫秒）= 投射物删除（到达）时刻（Crystal `Effect.Start`）。
    pub started_ms: i64,
}

impl HitEffectView {
    /// 已播放的毫秒数（时钟回退/负差值按 0）。
    #[must_use]
    pub fn elapsed_ms(&self, now_ms: i64) -> u64 {
        u64::try_from(now_ms.saturating_sub(self.started_ms).max(0)).unwrap_or(u64::MAX)
    }

    /// 当前 `magic.wzl` 帧号（`570..=579`）。
    #[must_use]
    pub fn sprite_index(&self, now_ms: i64) -> u32 {
        hit_effect_index(self.elapsed_ms(now_ms))
    }

    /// 是否播完（`Effect.Process` 在 `CurrentFrame >= Count` 时 `Remove`，
    /// `Effect.cs:91-106`）。
    #[must_use]
    pub fn finished(&self, now_ms: i64) -> bool {
        self.elapsed_ms(now_ms) >= HIT_EFFECT_DURATION_MS
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
/// 超出 [`SOUND_CUE_LIMIT`] 时丢最旧一条（生产路径可能长时间无人 drain）。
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
        if self.played.len() >= SOUND_CUE_LIMIT {
            self.played.remove(0);
        }
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

    /// 每帧同步：先消费新事件，再按 `now_ms` 推进帧/落点并删除到达的实体。
    ///
    /// 幂等：同一快照、同一时刻重复调用不会重复生成，也不会多推进帧。
    /// 出生/命中音效记录进内部 [`RecordingSoundSink`]（[`Self::take_sound_cues`]
    /// 供音频通道取走）；需要自定义播放端时用 [`Self::sync_with_sink`]。
    pub fn sync(&mut self, snapshot: &WorldSnapshot, now_ms: i64) {
        // 先取出内部 sink 避开双重可变借用，同步完再放回。
        let mut sink = std::mem::take(&mut self.sounds);
        self.sync_with_sink(snapshot, now_ms, &mut sink);
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
    /// 快照是整体克隆的（`net.rs:4477-4480`），事件表在会话侧累计且从不 drain
    /// （`net.rs:7246-7260`），所以这里用位置游标去重；绘制路径只持 `&WorldSnapshot`，
    /// 不能 `mem::take`，更不能 clone 整个 vec。
    fn consume(
        &mut self,
        snapshot: &WorldSnapshot,
        now_ms: i64,
        sink: &mut dyn ProjectileSoundSink,
    ) {
        let events = &snapshot.projectile_events;
        if events.len() < self.consumed {
            // 事件表被重置（重连/换快照）：从头再消费。
            self.consumed = 0;
        }
        while self.consumed < events.len() {
            let event = events[self.consumed];
            self.consumed += 1;
            self.spawn(event, snapshot, now_ms, sink);
        }
    }

    /// 一个事件生成一个实体；`source`/`destination` 解析不到坐标就丢弃（见 D-R59-1）。
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
        let Some(destination) = snapshot.object_position(event.destination) else {
            // 偏离 D-R59-1：Crystal `GameScene.cs:4699` 无条件生成（`:4701` 才按目标
            // 挂命中）；125 没有落点坐标时不生成实体，不造坐标，也不播出生音
            // （C3：出生音效随投射物生成，无投射物则无声）。
            return;
        };
        let duration = duration_ms(source, destination);
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
            duration_ms: duration,
            frame_count: frame_count(duration),
            dir16: direction16(source, destination),
            frame: 0,
        });
    }

    /// 推进帧号与落点；到达（`frame >= frame_count`）即删除并生成命中表现。
    ///
    /// 目标当前位置每帧重取（Crystal `Effect.cs:174-181` 的
    /// `Destination = Target.CurrentLocation`）；目标消失就停在最后已知位置。
    /// 删除点对应 Crystal `Missile.Complete`（`GameScene.cs:4701-4710`）：
    /// 目标存在且未死亡才加 570 命中特效 + 命中音效（`:4708` / `:4709`）。
    fn advance(
        &mut self,
        snapshot: &WorldSnapshot,
        now_ms: i64,
        sink: &mut dyn ProjectileSoundSink,
    ) {
        for view in &mut self.views {
            if let Some(position) = snapshot.object_position(view.destination_id) {
                view.destination = position;
            }
            let elapsed = now_ms.saturating_sub(view.started_ms).max(0);
            view.frame = frame_at(u64::try_from(elapsed).unwrap_or(u64::MAX));
        }
        let mut hit_effects = Vec::new();
        self.views.retain(|view| {
            if !view.finished() {
                return true;
            }
            if let Some(position) = hit_target_position(snapshot, view.destination_id) {
                hit_effects.push(HitEffectView {
                    target_id: view.destination_id,
                    position,
                    started_ms: now_ms,
                });
                if let Some(file) = projectile_sound(view.spell, ProjectileSoundPhase::Hit) {
                    // Crystal `GameScene.cs:4709`：命中音与 570 特效同一时机。
                    sink.play(file);
                }
            }
            false
        });
        self.hit_effects.extend(hit_effects);
    }

    /// 推进命中特效：位置跟目标（Crystal `Effect.Draw` 用 `Owner.DrawLocation`），
    /// 播满 [`HIT_EFFECT_DURATION_MS`] 后删除（`Effect.Process` 的 `Remove`）。
    fn advance_hit_effects(&mut self, snapshot: &WorldSnapshot, now_ms: i64) {
        for effect in &mut self.hit_effects {
            if let Some(position) = snapshot.object_position(effect.target_id) {
                effect.position = position;
            }
        }
        self.hit_effects.retain(|effect| !effect.finished(now_ms));
    }

    /// 画在飞投射物与命中特效：`FxKind::Magic` + `draw_effect`（缺素材时静默跳过）。
    ///
    /// Crystal 的 `DrawLocation` 是「出生格屏幕坐标 + 该帧偏移」
    /// （`Effect.cs:206-212`），`world_to_screen` 对应它前面的摄像机换算；命中特效
    /// 则直接画在目标格（`Effect.cs:117-119` 的 `Owner.DrawLocation`）。
    pub fn draw(&self, frame: &mut Frame, scene: &mut MapScene, center: Point, now_ms: i64) {
        for view in &self.views {
            let (x, y) = scene.world_to_screen(center, view.source);
            let (offset_x, offset_y) = view.offset();
            scene.draw_effect(
                frame,
                FxKind::Magic,
                view.sprite_index(),
                x + offset_x,
                y + offset_y,
            );
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

    #[test]
    fn flight_formulas_match_crystal_effect() {
        let source = Point::new(10, 10);
        let target = Point::new(15, 10);
        assert_eq!(max_distance(source, target), 5);
        assert_eq!(duration_ms(source, target), 250);
        assert_eq!(frame_count(250), 8);
        assert_eq!(frame_count(0), 1);
        assert_eq!(frame_at(0), 0);
        assert_eq!(frame_at(150), 5);
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

    #[test]
    fn events_spawn_once_and_follow_target_position() {
        let mut snapshot = snapshot_with(Point::new(10, 10), Point::new(15, 10));
        snapshot.projectile_events.push(fire_bounce(1, 2));
        let mut projectiles = Projectiles::default();
        projectiles.sync(&snapshot, 1_000);
        assert_eq!(projectiles.events_consumed(), 1);
        assert_eq!(projectiles.len(), 1);
        let view = projectiles.views()[0];
        assert_eq!(view.source, Point::new(10, 10));
        assert_eq!(view.destination, Point::new(15, 10));
        assert_eq!(view.duration_ms, 250);
        assert_eq!(view.frame_count, 8);
        assert_eq!(view.dir16, 4);
        assert_eq!(view.frame, 0);
        assert_eq!(view.sprite_index(), BASE_INDEX + 40);
        // 同一快照再同步：不重复生成。
        projectiles.sync(&snapshot, 1_000);
        assert_eq!(projectiles.events_consumed(), 1);
        assert_eq!(projectiles.len(), 1);
        // 目标移动后按当前位置重取落点。
        snapshot.entities.get_mut(&2).expect("target").position = Point::new(15, 12);
        projectiles.sync(&snapshot, 1_150);
        assert_eq!(projectiles.views()[0].destination, Point::new(15, 12));
        assert_eq!(projectiles.views()[0].frame, 5);
        // 到达/超时删除。
        projectiles.sync(&snapshot, 1_240);
        assert_eq!(projectiles.len(), 0);
        // 落点未知（125 没有落点坐标）不生成实体，但仍算已消费（D-R59-1）。
        snapshot.projectile_events.push(fire_bounce(1, 999));
        projectiles.sync(&snapshot, 1_300);
        assert_eq!(projectiles.events_consumed(), 2);
        assert_eq!(projectiles.len(), 0);
        // 源未知同样丢弃。
        snapshot.projectile_events.push(fire_bounce(999, 2));
        projectiles.sync(&snapshot, 1_300);
        assert_eq!(projectiles.events_consumed(), 3);
        assert_eq!(projectiles.len(), 0);
        // 事件表被重置（重连）：游标归零，同一事件可以再次生成。
        let mut fresh = snapshot_with(Point::new(10, 10), Point::new(10, 11));
        fresh.projectile_events.push(fire_bounce(1, 2));
        projectiles.sync(&fresh, 1_400);
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
        // 未到达（frame 6/8）不出命中特效。
        projectiles.sync(&snapshot, 1_200);
        assert!(projectiles.hit_effects().is_empty());
        // 到达前目标走开一格：570 特效挂在目标身上（Effect.Owner = Target）。
        snapshot.entities.get_mut(&2).expect("target").position = Point::new(15, 11);
        projectiles.sync(&snapshot, 1_250);
        assert_eq!(projectiles.len(), 0);
        let hit = projectiles.hit_effects();
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].target_id, 2);
        assert_eq!(hit[0].position, Point::new(15, 11));
        assert_eq!(hit[0].sprite_index(1_250), 570);
        assert_eq!(hit[0].sprite_index(1_310), 571);
        assert_eq!(hit[0].sprite_index(1_790), 579);
        // 不到 600ms 不删除，满 600ms 删除。
        projectiles.sync(&snapshot, 1_849);
        assert_eq!(projectiles.hit_effects().len(), 1);
        projectiles.sync(&snapshot, 1_850);
        assert!(projectiles.hit_effects().is_empty());
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
        projectiles.sync(&snapshot, 1_000);
        snapshot.entities.get_mut(&2).expect("target").dead = true;
        projectiles.sync(&snapshot, 1_250);
        assert!(projectiles.is_empty());
        assert!(projectiles.hit_effects().is_empty());
        assert_eq!(projectiles.sound_cues(), ["M34-1.wav"].as_slice());
        // 目标从快照消失（:4701 的 missile.Target == null）：同样没有命中表现。
        let mut snapshot = snapshot_with(Point::new(10, 10), Point::new(15, 10));
        snapshot.projectile_events.push(fire_bounce(1, 2));
        let mut projectiles = Projectiles::default();
        projectiles.sync(&snapshot, 2_000);
        snapshot.entities.remove(&2);
        projectiles.sync(&snapshot, 2_250);
        assert!(projectiles.is_empty());
        assert!(projectiles.hit_effects().is_empty());
        assert_eq!(projectiles.sound_cues(), ["M34-1.wav"].as_slice());
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
        projectiles.sync_with_sink(&snapshot, 1_250, &mut sink);
        assert_eq!(sink.played, ["M34-1.wav", "M34-2.wav"].as_slice());
        // 实体删除后再同步不会重复命中音。
        projectiles.sync_with_sink(&snapshot, 1_300, &mut sink);
        assert_eq!(sink.played.len(), 2);
        let spawns = sink
            .played
            .iter()
            .filter(|cue| **cue == "M34-1.wav")
            .count();
        let hits = sink
            .played
            .iter()
            .filter(|cue| **cue == "M34-2.wav")
            .count();
        assert_eq!((spawns, hits), (1, 1));
        // 生产路径（无 fake sink）同样把两条 cue 记进本地队列。
        let mut projectiles = Projectiles::default();
        projectiles.sync(&snapshot, 2_000);
        projectiles.sync(&snapshot, 2_250);
        assert_eq!(
            projectiles.sound_cues(),
            ["M34-1.wav", "M34-2.wav"].as_slice()
        );
    }

    #[test]
    fn unknown_destination_generates_no_projectile_or_sound() {
        // C3 / D-R59-1：125 的 destination 解析不到坐标 → 不生成投射物、命中与出生音。
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
}
