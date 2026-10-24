//! 角色动作帧模型：把 Crystal 的动作帧表（`Frames.cs`）与扁平 `.wzl` 的帧布局
//! 变成可测的纯函数。
//!
//! # Crystal 的帧公式
//!
//! Crystal 的 `Frame` 只有四个影响播放的字段：
//! `Start`、`Count`、`Skip`、`Interval`（`Client/MirObjects/Frames.cs:203-244`）。
//! 绘制帧号是
//!
//! ```text
//! DrawFrame = Frame.Start + (Frame.OffSet * (byte)Direction) + FrameIndex
//!             // PlayerObject.cs:761、MonsterObject.cs:357
//! Frame.OffSet = Count + Skip          // Frames.cs:209-212
//! ```
//!
//! 也就是说 `Skip` **不是**「播放前先跳过的帧」，而是**每个方向段末尾的空槽数**：
//! 方向 `d` 的有效帧是 `Start + d*(Count+Skip) + 0..Count`，随后 `Skip` 个槽位没人用。
//! 例如人类行走 `Frame(32, 6, 0, 100)`（`Frames.cs:158`）在 Crystal 的 `.Lib` 里
//! 每个方向占 6 帧；本项目的扁平 `hum.wzl` 每个方向占 8 帧（6 张有效图 + 2 个空槽），
//! 所以本模块把 `Skip` 填成 2，保持 `Count/Interval` 与 Crystal 完全一致。
//!
//! # 扁平 `hum.wzl` 的动作起点
//!
//! `resource/data/hum.wzl` 的每个外观块是 600 帧，动作顺序与经典 `Hum.wil` 相同，
//! 但每个方向固定占 8 帧。实测（`tests/animation_assets.rs`、`hum.wzx` 的帧头）
//! 得到下表；`Start` 是「方向 0 的第 0 帧」在本外观块内的偏移：
//!
//! | 动作 | Start | Count | Skip | 方向步长 | Interval | 本模块常量 |
//! |---|---|---|---|---|---|---|
//! | 站立 | 0 | 4 | 4 | 8 | 500 ms | [`HUMAN_STANDING`] |
//! | 行走 | 64 | 6 | 2 | 8 | 100 ms | [`HUMAN_WALKING`] |
//! | 跑步 | 128 | 6 | 2 | 8 | 100 ms | [`HUMAN_RUNNING`] |
//! | 攻击 1（旧） | 200 | 6 | 2 | 8 | **50 ms** | [`HUMAN_ATTACK1`] |
//! | 攻击 1（R47 入围） | 200 | 6 | 2 | 8 | **100 ms** | [`HUMAN_ATTACK1_R47`] |
//! | 攻击 2 | 264 | 6 | 2 | 8 | **50 ms** | [`HUMAN_ATTACK2`] |
//! | 攻击 3（旧） | 328 | 8 | 0 | 8 | **50 ms** | [`HUMAN_ATTACK3`] |
//! | 攻击 3（R47 入围） | 328 | 8 | 0 | 8 | **100 ms** | [`HUMAN_ATTACK3_R47`] |
//! | 受击 | 472 | 3 | 5 | 8 | 100 ms | [`HUMAN_STRUCK`] |
//! | 死亡 | 536 | 4 | 4 | 8 | 100 ms | [`HUMAN_DYING`] |
//! | 尸体 | 539 | 1 | 7 | 8 | 1000 ms | [`HUMAN_DEAD`] |
//!
//! `Count/Interval` 来自 Crystal `Frames.cs:157-170`；`Start/Skip` 是扁平语料的
//! 适配值。适配依据是经典 `Hum.wil` 的 600 帧布局（站立 0~63、走 64~127、
//! 跑 128~191、攻击 192~263、双手攻击 264~327、强行攻击 328~391、施法 392~455、
//! 挖肉 456~471、受击 472~535、死亡 536~599），并用真实素材逐帧验证；
//! 192~199 是 8 个方向的 1 帧「姿态」，攻击 1 从 200 开始。
//!
//! # 怪物帧表
//!
//! Crystal 的默认怪物表在 `Frames.cs:25-31`：
//!
//! ```text
//! Standing  Frame(0,   4,  0, 500)
//! Walking   Frame(32,  6,  0, 100)
//! Attack1   Frame(80,  6,  0, 100)
//! Struck    Frame(128, 2,  0, 200)
//! Die       Frame(144, 10, 0, 100)
//! Dead      Frame(153, 1,  9, 1000)
//! ```
//!
//! Crystal 的 `.Lib` 还能带「每个怪物自己的帧表」（`MonsterObject.cs:276` 的
//! `BodyLibrary.Frames`），扁平 `.wzl` 没有这份数据（`!Mon.dat` 尚未解析），
//! 所以本模块按 `resource/data/mon1.wzl` 第 0 个图像实测到的常见布局做保守默认：
//! 站立 0、行走 64、攻击 128、受击 280、死亡 344；未收录的怪物至少能正确播放
//! **站立 4 帧 + 走 6 帧**。除死亡段按素材实测取 6 帧外，`Count/Interval` 取
//! Crystal 默认表。
//!
//! # 播放规则
//!
//! [`ActorAnimation::frame_at`] 按 `(now - started_at) / interval` 选局部帧：
//!
//! * `Standing` 循环播放 `Count` 帧；
//! * `Walking/Running` 到 `Count` 后回站立第 0 帧并置 `finished = true`
//!   （Crystal `ProcessFrames` 在走/跑结束时调用 `SetAction()` 回 Standing，
//!   `PlayerObject.cs:2317-2323`）；
//! * `Attacking/Struck/Dying` 结束后**保持最后一帧**并置 `finished = true`
//!   （Crystal 先 `FrameIndex = Frame.Count - 1` 再切动作，`PlayerObject.cs:2500-2508`）；
//! * `Dead` 永远停在尸体帧。

use mir_core::enums::MirDirection;

use crate::entities::EntityView;

/// 方向数量（Crystal 的 `MirDirection` 是 8 方向，`Shared/Enum.cs`）。
pub const DIRECTION_COUNT: u16 = 8;

/// 一张动作帧表：对应 Crystal 的 `Frame`（`Frames.cs:203-244`）。
///
/// * [`Self::start`]：方向 0 的第 0 帧在本外观块内的偏移；
/// * [`Self::count`]：每个方向的有效帧数；
/// * [`Self::skip`]：每个方向段末尾的空槽数（Crystal 的 `Skip`，见 `Frames.cs:209-212`）；
/// * [`Self::interval_ms`]：两帧之间的毫秒数。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameSequence {
    /// 方向 0 的第 0 帧在动作块内的偏移。
    pub start: u16,
    /// 每个方向的有效帧数。
    pub count: u16,
    /// 每个方向段末尾的空槽数。
    pub skip: u16,
    /// 帧间隔（毫秒）。
    pub interval_ms: u64,
}

impl FrameSequence {
    /// 构造一张帧表。
    #[must_use]
    pub const fn new(start: u16, count: u16, skip: u16, interval_ms: u64) -> Self {
        Self {
            start,
            count,
            skip,
            interval_ms,
        }
    }

    /// 每个方向占的帧数：`Count + Skip`（Crystal `Frame.OffSet`，`Frames.cs:209-212`）。
    #[must_use]
    pub const fn stride(self) -> u16 {
        self.count + self.skip
    }

    /// 整个动作的时长（`Count * Interval`）。
    #[must_use]
    pub fn duration_ms(self) -> u64 {
        u64::from(self.count) * self.interval_ms
    }

    /// 某个方向、某个局部帧的绝对帧号（不含外观块基址）。
    ///
    /// `local` 超出 `0..count` 时会被夹到有效范围（Crystal 的 `FrameIndex` 也只在
    /// `0..Count-1` 内使用）。
    #[must_use]
    pub fn index_of(self, direction: MirDirection, local: u16) -> u16 {
        let local = local.min(self.count.saturating_sub(1));
        self.start
            .saturating_add(u16::from(direction as u8).saturating_mul(self.stride()))
            .saturating_add(local)
    }

    /// 给定已经过去的毫秒数，算出当前帧与是否播完。
    ///
    /// 局部帧是 `elapsed / interval`，最大到 `count - 1`；`finished` 在
    /// `elapsed >= count * interval` 时为真。循环/回站立等策略由
    /// [`ActorAnimation::frame_at`] 负责。
    #[must_use]
    pub fn frame_at(self, direction: MirDirection, elapsed_ms: u64) -> FrameChoice {
        let interval = self.interval_ms.max(1);
        let frame_number = elapsed_ms / interval;
        let finished = frame_number >= u64::from(self.count);
        let local = u16::try_from(frame_number.min(u64::from(self.count))).unwrap_or(0);
        FrameChoice {
            index: self.index_of(direction, local),
            finished,
        }
    }
}

/// [`FrameSequence::frame_at`] / [`ActorAnimation::frame_at`] 的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameChoice {
    /// 相对本外观块（不含 `block*600` 基址）的绝对帧号。
    pub index: u16,
    /// 动作是否已经播完（走/跑/攻击/受击/死亡）。
    pub finished: bool,
}

/// 角色动作（Crystal `MirAction` 的子集，只保留本作需要的 7 种）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ActorAction {
    /// 站立（循环 4 帧）。
    #[default]
    Standing,
    /// 行走（6 帧，结束后回站立）。
    Walking,
    /// 跑步（6 帧，结束后回站立）。
    Running,
    /// 攻击（6 帧，结束后保持最后一帧）。
    Attacking,
    /// 受击（3 帧）。
    Struck,
    /// 正在死亡（4 帧）。
    Dying,
    /// 尸体（停在最后一帧）。
    Dead,
}

impl ActorAction {
    /// 稳定的小写名字（日志/测试用）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Standing => "standing",
            Self::Walking => "walking",
            Self::Running => "running",
            Self::Attacking => "attacking",
            Self::Struck => "struck",
            Self::Dying => "dying",
            Self::Dead => "dead",
        }
    }

    /// 是否是「播完就回 Standing」的短动作（走/跑）。
    #[must_use]
    pub const fn returns_to_standing(self) -> bool {
        matches!(self, Self::Walking | Self::Running)
    }

    /// 是否是「播完保持最后一帧」的一次性动作（攻击/受击/死亡）。
    #[must_use]
    pub const fn holds_last_frame(self) -> bool {
        matches!(self, Self::Attacking | Self::Struck | Self::Dying)
    }

    /// 是否循环播放。
    #[must_use]
    pub const fn loops(self) -> bool {
        matches!(self, Self::Standing)
    }
}

/// R47 普通空挥选中的 Crystal 攻击变体（仅客户端本地视觉）。
///
/// Crystal `PlayerObject.cs:1055-1075` 的 default 分支只在 Shift + 无目标时
/// `Next(100) >= 20 ? Attack1 : Attack3`；本作只把 Warrior/未骑乘/Spell.None
/// 的 Shift+Space 空挥映射到这里，其余分支继续用旧 `HUMAN_ATTACK1`（50ms）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttackVariant {
    /// Crystal `MirAction.Attack1`：逻辑 136/6 帧，转换后扁平布局 200/6，100ms。
    Attack1,
    /// Crystal `MirAction.Attack3`：逻辑 232/8 帧，转换后扁平布局 328/8，100ms。
    Attack3,
}

impl AttackVariant {
    /// 稳定的小写名字（日志/manifest/测试）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Attack1 => "attack1",
            Self::Attack3 => "attack3",
        }
    }

    /// Crystal 逻辑帧表（不是扁平 `.wzl` 的物理索引）。
    #[must_use]
    pub const fn crystal_sequence(self) -> FrameSequence {
        match self {
            Self::Attack1 => FrameSequence::new(136, 6, 0, 100),
            Self::Attack3 => FrameSequence::new(232, 8, 0, 100),
        }
    }

    /// Crystal effect（翅膀/武器光效列）逻辑帧表。
    #[must_use]
    pub const fn crystal_effect_sequence(self) -> FrameSequence {
        match self {
            Self::Attack1 => FrameSequence::new(168, 6, 0, 100),
            Self::Attack3 => FrameSequence::new(264, 8, 0, 100),
        }
    }

    /// game-assets/扁平 `.wzl` 转换后的身体帧表（100ms；R47 入围路径）。
    #[must_use]
    pub const fn converted_sequence(self) -> FrameSequence {
        match self {
            Self::Attack1 => HUMAN_ATTACK1_R47,
            Self::Attack3 => HUMAN_ATTACK3_R47,
        }
    }

    /// 转换后的翅膀/effect 帧表；Attack3 有 8 帧 effect，Attack1 有 6 帧。
    #[must_use]
    pub const fn converted_effect_sequence(self) -> FrameSequence {
        match self {
            Self::Attack1 => WING_ATTACK1_R47,
            Self::Attack3 => WING_ATTACK3_R47,
        }
    }
}

/// 帧表所属的素材种类：玩家、怪物、NPC。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BodyKind {
    /// 玩家（`hum*.wzl` / `weapon*.wzl` / `hair*.wzl`，600 帧一个外观块）。
    #[default]
    Human,
    /// 怪物（`monN.wzl`，430 帧一个图像）。
    Monster,
    /// NPC（`npc*.wzl`，60 帧一个图像）。
    Npc,
}

impl BodyKind {
    /// 稳定的小写名字。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::Monster => "monster",
            Self::Npc => "npc",
        }
    }
}

/// 原生怪物（t40 `frames.json`）的动作生命期。
///
/// Crystal `MonsterObject.cs:524/642` 把 `Frame.Interval` 写入 `FrameInterval`，
/// `ProcessFrames` 在 `UpdateFrame() >= Frame.Count` 时切换动作，所以总时长是
/// `Count × Interval`，不是服务端僵直常量。这里优先查实际帧表；只有整张表缺失
/// （无游戏素材）才回退 `Frames.cs:25-31` 的 DefaultMonster：Standing 4×500、
/// Walk/Run 6×100、Attack1 6×100、Struck 2×200、Die 10×100；Dead 不自动转。
/// 表存在但缺动作时返回 `None`，由调用方保持动作，绝不伪造默认时长。
#[must_use]
pub fn monster_action_duration_ms(
    action: ActorAction,
    frames: Option<&mir_asset::FrameSet>,
) -> Option<u64> {
    if action == ActorAction::Dead {
        return None;
    }
    let raw = match action {
        ActorAction::Standing => 0,
        ActorAction::Walking => 1,
        ActorAction::Running => 2,
        ActorAction::Attacking => 9,
        ActorAction::Struck => 18,
        ActorAction::Dying => 21,
        ActorAction::Dead => unreachable!(),
    };
    let Some(frames) = frames else {
        // 整张原生表缺失（无 game-assets 素材/未导入）才用 DefaultMonster。
        return monster_native_duration_ms(action);
    };
    frames.get(raw).and_then(|frame| {
        let count = u64::try_from(frame.count).ok()?;
        let interval = u64::try_from(frame.interval).ok()?;
        count.checked_mul(interval)
    })
}

/// 无帧表时的 `DefaultMonster` 时长（`Frames.cs:25-31`）。
#[must_use]
pub const fn monster_native_duration_ms(action: ActorAction) -> Option<u64> {
    match action {
        ActorAction::Standing => Some(2_000),
        ActorAction::Walking | ActorAction::Running | ActorAction::Attacking => Some(600),
        ActorAction::Struck => Some(400),
        ActorAction::Dying => Some(1_000),
        ActorAction::Dead => None,
    }
}

// ---------------------------------------------------------------------------
// 人类帧表
// ---------------------------------------------------------------------------

/// 人类站立：`Frames.cs:157` 的 `Frame(0, 4, 0, 500)`。
///
/// 扁平 `hum.wzl` 每个方向 8 帧（4 张有效 + 4 空槽），所以 `skip = 4`。
pub const HUMAN_STANDING: FrameSequence = FrameSequence::new(0, 4, 4, 500);
/// 人类行走：`Frames.cs:158` 的 `Frame(32, 6, 0, 100)`。
///
/// 扁平布局从 64 开始（每方向 6 张有效 + 2 空槽）。
pub const HUMAN_WALKING: FrameSequence = FrameSequence::new(64, 6, 2, 100);
/// 人类跑步：`Frames.cs:159` 的 `Frame(80, 6, 0, 100)`。
///
/// 扁平布局从 128 开始。
pub const HUMAN_RUNNING: FrameSequence = FrameSequence::new(128, 6, 2, 100);
/// 人类攻击 1：`Frames.cs:162` 的 `Frame(136, 6, 0, 100)`。
///
/// 扁平布局从 200 开始（192~199 是 8 方向的 1 帧姿态）。
///
/// 间隔取 **50 ms**（6 帧 = 300 ms）而不是 Crystal 帧表里的 100 ms：服务端
/// `PLAYER_ATTACK_DAMAGE_DELAY_MS = 300`（`world.rs` 移植 `HumanObject.cs:3201`
/// 的 `DelayedAction`），镜头要在伤害落地时正好播完挥砍；这个节奏也是本轮任务
/// 要求的「攻击 300 ms」。跑步仍保持 6 × 100 ms = 600 ms（与服务器跑动一致）。
pub const HUMAN_ATTACK1: FrameSequence = FrameSequence::new(200, 6, 2, 50);
/// R47 入围路径的 Attack1：转换后扁平布局 200/6 帧，但间隔用 Crystal 的
/// **100ms**（6 帧 = 600ms），不再采用旧的 50ms/300ms 对齐。
pub const HUMAN_ATTACK1_R47: FrameSequence = FrameSequence::new(200, 6, 2, 100);
/// R47 入围路径的 Attack3（双手）：转换后扁平布局 328/8 帧，Crystal 100ms
/// （8 帧 = 800ms）；不能把它误写成逻辑起点 232。
pub const HUMAN_ATTACK3_R47: FrameSequence = FrameSequence::new(328, 8, 0, 100);
/// 人类攻击 2：`Frames.cs:163` 的 `Frame(184, 6, 0, 100)`；间隔同 [`HUMAN_ATTACK1`]。
pub const HUMAN_ATTACK2: FrameSequence = FrameSequence::new(264, 6, 2, 50);
/// 人类攻击 3（双手）：`Frames.cs:164` 的 `Frame(232, 8, 0, 100)`；间隔 50 ms。
pub const HUMAN_ATTACK3: FrameSequence = FrameSequence::new(328, 8, 0, 50);
/// 人类受击：`Frames.cs:168` 的 `Frame(360, 3, 0, 100)`。
pub const HUMAN_STRUCK: FrameSequence = FrameSequence::new(472, 3, 5, 100);
/// 人类死亡：`Frames.cs:169` 的 `Frame(384, 4, 0, 100)`。
pub const HUMAN_DYING: FrameSequence = FrameSequence::new(536, 4, 4, 100);
/// 人类尸体：`Frames.cs:170` 的 `Frame(387, 1, 3, 1000)`。
///
/// Crystal 的 `Start=387` 是「死亡段第 3 帧」；扁平布局的死亡段从 536 开始，
/// 所以尸体帧是 `536 + 3 = 539`。
pub const HUMAN_DEAD: FrameSequence = FrameSequence::new(539, 1, 7, 1000);

/// 取人类的动作帧表。
// 简化：所有攻击都映射到 Attack1。原因：`ActorAction` 不区分 Attack1..4，
// Crystal 的变招选择（`PlayerObject.cs:1055-1075`）需要职业/武器/随机数，尚未接入。
#[must_use]
pub const fn human_frames(action: ActorAction) -> FrameSequence {
    human_frames_for(action, None)
}

/// 取人类动作帧表；`AttackVariant::Some` 时 Attack1/3 使用 R47 的 100ms 表，
/// `None` 保持旧行为（Attack1 50ms）。
#[must_use]
pub const fn human_frames_for(
    action: ActorAction,
    variant: Option<AttackVariant>,
) -> FrameSequence {
    match (action, variant) {
        (ActorAction::Attacking, Some(AttackVariant::Attack1)) => HUMAN_ATTACK1_R47,
        (ActorAction::Attacking, Some(AttackVariant::Attack3)) => HUMAN_ATTACK3_R47,
        (ActorAction::Attacking, None) => HUMAN_ATTACK1,
        _ => match action {
            ActorAction::Standing => HUMAN_STANDING,
            ActorAction::Walking => HUMAN_WALKING,
            ActorAction::Running => HUMAN_RUNNING,
            ActorAction::Struck => HUMAN_STRUCK,
            ActorAction::Dying => HUMAN_DYING,
            ActorAction::Dead => HUMAN_DEAD,
            ActorAction::Attacking => HUMAN_ATTACK1,
        },
    }
}

// ---------------------------------------------------------------------------
// 怪物帧表
// ---------------------------------------------------------------------------

/// 怪物站立：Crystal 默认怪物表 `Frames.cs:25` 的 `Frame(0, 4, 0, 500)`。
pub const MONSTER_STANDING: FrameSequence = FrameSequence::new(0, 4, 4, 500);
/// 怪物行走：`Frames.cs:26` 的 `Frame(32, 6, 0, 100)`。
///
/// 扁平 `monN.wzl` 实测行走段从 64 开始（每个方向 6 张有效 + 2 空槽）。
pub const MONSTER_WALKING: FrameSequence = FrameSequence::new(64, 6, 2, 100);
/// 怪物攻击 1：`Frames.cs:27` 的 `Frame(80, 6, 0, 100)`。
///
/// 扁平 `monN.wzl` 实测攻击段从 128 开始。间隔同人类攻击取 50 ms（300 ms 总时长），
/// 与服务端 `PLAYER_ATTACK_DAMAGE_DELAY_MS` 的命中节奏对齐。
pub const MONSTER_ATTACK1: FrameSequence = FrameSequence::new(128, 6, 2, 50);
/// 怪物受击：`Frames.cs:28` 的 `Frame(128, 2, 0, 200)`。
///
/// 扁平 `monN.wzl` 的受击段不在固定位置；这里的 280 是 `mon1.wzl` 第 0 个图像的
/// 实测值（4 张图中取 Crystal 的 2 帧，间隔 200 ms）。没有该段的怪物会回退到站立帧。
// 简化：受击起点写死为 mon1 图像 0 的实测偏移。原因：扁平 `.wzl` 没有
// Crystal `.Lib` 的逐怪物 `Frames` 表（`MonsterObject.cs:276`），`!Mon.dat` 尚未解析。
pub const MONSTER_STRUCK: FrameSequence = FrameSequence::new(280, 2, 6, 200);
/// 怪物死亡：`Frames.cs:29` 的 `Frame(144, 10, 0, 100)`。
///
/// 扁平 `mon1.wzl` 第 0 个图像的死亡段从 344 开始，每个方向 6 张有效图。
// 简化：Crystal 默认表是 10 帧，这里按素材实测取 6 帧。原因：`mon1.wzl` 图像 0
// 的死亡段每个方向只有 6 张非空图，多取会落到下一段/空帧。
pub const MONSTER_DYING: FrameSequence = FrameSequence::new(344, 6, 2, 100);
/// 怪物尸体：`Frames.cs:30` 的 `Frame(153, 1, 9, 1000)`。
///
/// 扁平布局取死亡段最后一帧（344 + 5 = 349）。
// 简化：尸体起点用死亡段最后一帧的实测值。原因：与 MONSTER_DYING 同一份扁平布局。
pub const MONSTER_DEAD: FrameSequence = FrameSequence::new(349, 1, 7, 1000);

/// 取怪物的动作帧表（未收录怪物用保守默认；见模块文档）。
#[must_use]
pub const fn monster_frames(action: ActorAction) -> FrameSequence {
    match action {
        ActorAction::Standing => MONSTER_STANDING,
        ActorAction::Walking | ActorAction::Running => MONSTER_WALKING,
        ActorAction::Attacking => MONSTER_ATTACK1,
        ActorAction::Struck => MONSTER_STRUCK,
        ActorAction::Dying => MONSTER_DYING,
        ActorAction::Dead => MONSTER_DEAD,
    }
}

// ---------------------------------------------------------------------------
// NPC 帧表
// ---------------------------------------------------------------------------

/// NPC 站立：Crystal 默认 NPC 表 `Frames.cs:18` 的 `Frame(0, 4, 0, 450)`。
///
/// 扁平 `npc.wzl` 第 0 个图像实测 4 帧一组、每组间隔 10 帧，所以 `skip = 6`。
// 简化：NPC 只播放站立，且只对前 4 个方向可靠（`npc.wzl` 第 0 图像 30~59 帧是
// 其它动作）。原因：NPC 通常只朝下站立，精确的逐 NPC 帧表尚未接入。
pub const NPC_STANDING: FrameSequence = FrameSequence::new(0, 4, 6, 450);
/// NPC 采集动作（`Frames.cs:19`，本作未使用）。
pub const NPC_HARVEST: FrameSequence = FrameSequence::new(12, 10, 0, 200);

/// 取 NPC 的动作帧表（NPC 只有站立；其余动作回退到站立）。
#[must_use]
pub const fn npc_frames(action: ActorAction) -> FrameSequence {
    match action {
        ActorAction::Standing
        | ActorAction::Walking
        | ActorAction::Running
        | ActorAction::Attacking
        | ActorAction::Struck
        | ActorAction::Dying
        | ActorAction::Dead => NPC_STANDING,
    }
}

// ---------------------------------------------------------------------------
// 翅膀（WingEffect）帧表
// ---------------------------------------------------------------------------
//
// Crystal 的 `PlayerObject.DrawWings` 用 `DrawWingFrame`，它不是身体的 `DrawFrame`：
// `DrawWingFrame = Frame.EffectStart + Frame.EffectOffSet * direction + EffectFrameIndex`
// （`PlayerObject.cs:762`）。`Frames.cs` 给每个身体动作都带了第二组 effect 帧参数
// （`new Frame(start, count, skip, interval, effectStart, effectCount, effectSkip, effectInterval)`，
// `Frames.cs:157-170` 的第五～八角）。因此翅膀与身体使用**不同的** Start/Count/Interval：
// 站立每方向 8 帧、250 ms（`Frames.cs:157`），行走/跑步/攻击也是每方向 8 帧，
// 但间隔 100 ms（`:158-164`）；受击、死亡、尸体用各自的 effect 段（`:168-170`）。

/// 扁平 `wingeffect_x.wzl` 每个翅膀外观块占的帧数。
///
/// 实测 `wingeffect_x.wzx` 有 1200 项 = 2 个外观 × 600 帧；每个外观块与 `hum.wzl`
/// 一样是 600 帧布局（方向步长 8、动作起点见下面的常量）。
pub const WING_FRAMES_PER_APPEARANCE: u32 = 600;
/// 翅膀站立：`Frames.cs:157` 的 effect 段 `Frame(..., 0, 8, 0, 250)`。
pub const WING_STANDING: FrameSequence = FrameSequence::new(0, 8, 0, 250);
/// 翅膀行走：`Frames.cs:158` 的 effect 段 `Frame(..., 64, 8, 0, 100)`。
pub const WING_WALKING: FrameSequence = FrameSequence::new(64, 8, 0, 100);
/// 翅膀跑步：`Frames.cs:159` 的 effect 段 `Frame(..., 112, 8, 0, 100)`。
pub const WING_RUNNING: FrameSequence = FrameSequence::new(112, 8, 0, 100);
/// 翅膀攻击：`Frames.cs:162` 的攻击 1 effect 段 `Frame(..., 168, 8, 0, 100)`。
// 简化：所有攻击都映射到攻击 1 的 effect 段。原因：`ActorAction` 不区分 Attack1..4。
pub const WING_ATTACKING: FrameSequence = FrameSequence::new(168, 8, 0, 100);
/// R47 Attack1 的 effect：逻辑 168/6 帧，转换后每方向仍 8 槽（6 有效+2 空）。
pub const WING_ATTACK1_R47: FrameSequence = FrameSequence::new(168, 6, 2, 100);
/// R47 Attack3 的 effect：逻辑 264/8 帧，转换后每方向 8 槽。
pub const WING_ATTACK3_R47: FrameSequence = FrameSequence::new(264, 8, 0, 100);
/// 翅膀受击：`Frames.cs:168` 的 effect 段 `Frame(..., 392, 3, 0, 100)`。
pub const WING_STRUCK: FrameSequence = FrameSequence::new(392, 3, 0, 100);
/// 翅膀死亡：`Frames.cs:169` 的 effect 段 `Frame(..., 416, 4, 0, 100)`。
pub const WING_DYING: FrameSequence = FrameSequence::new(416, 4, 0, 100);
/// 翅膀尸体：`Frames.cs:170` 的 effect 段 `Frame(..., 419, 1, 3, 1000)`。
pub const WING_DEAD: FrameSequence = FrameSequence::new(419, 1, 3, 1000);

/// 取某个动作的翅膀帧表（对应 `Frames.cs:157-170` 的 effect 列）。
#[must_use]
pub const fn wing_frames(action: ActorAction) -> FrameSequence {
    wing_frames_for(action, None)
}

/// 取翅膀 effect 帧表；R47 入围攻击按变体使用 168/6（Attack1）或 264/8（Attack3）。
#[must_use]
pub const fn wing_frames_for(action: ActorAction, variant: Option<AttackVariant>) -> FrameSequence {
    match (action, variant) {
        (ActorAction::Attacking, Some(AttackVariant::Attack1)) => WING_ATTACK1_R47,
        (ActorAction::Attacking, Some(AttackVariant::Attack3)) => WING_ATTACK3_R47,
        _ => match action {
            ActorAction::Standing => WING_STANDING,
            ActorAction::Walking => WING_WALKING,
            ActorAction::Running => WING_RUNNING,
            ActorAction::Attacking => WING_ATTACKING,
            ActorAction::Struck => WING_STRUCK,
            ActorAction::Dying => WING_DYING,
            ActorAction::Dead => WING_DEAD,
        },
    }
}

/// 取某个素材种类的动作帧表。
#[must_use]
pub const fn frames_for(body_kind: BodyKind, action: ActorAction) -> FrameSequence {
    frames_for_variant(body_kind, action, None)
}

/// 取某个素材种类的动作帧表；变体只对 Human 生效，怪物/NPC 忽略。
#[must_use]
pub const fn frames_for_variant(
    body_kind: BodyKind,
    action: ActorAction,
    variant: Option<AttackVariant>,
) -> FrameSequence {
    match body_kind {
        BodyKind::Human => human_frames_for(action, variant),
        BodyKind::Monster => monster_frames(action),
        BodyKind::Npc => npc_frames(action),
    }
}

// ---------------------------------------------------------------------------
// 一个正在播放的动画
// ---------------------------------------------------------------------------

/// 一个角色当前正在播放的动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActorAnimation {
    /// 动作。
    pub action: ActorAction,
    /// 动作开始时间（Unix 毫秒）。
    pub started_at_ms: u64,
    /// 该动作的帧表。
    pub frames: FrameSequence,
    /// 朝向。
    pub direction: MirDirection,
    /// 素材种类（决定回站立时用哪张帧表）。
    pub body_kind: BodyKind,
    /// R47 攻击变体；`None` = 旧行为/非入围攻击。
    pub attack_variant: Option<AttackVariant>,
}

impl ActorAnimation {
    /// 用动作、朝向、素材种类与开始时间构造动画。
    #[must_use]
    pub const fn new(
        action: ActorAction,
        direction: MirDirection,
        body_kind: BodyKind,
        started_at_ms: u64,
    ) -> Self {
        Self::new_with_variant(action, direction, body_kind, started_at_ms, None)
    }

    /// 带 R47 攻击变体构造动画；帧表因此在建立时固定，逐帧/重绘不重抽。
    #[must_use]
    pub const fn new_with_variant(
        action: ActorAction,
        direction: MirDirection,
        body_kind: BodyKind,
        started_at_ms: u64,
        attack_variant: Option<AttackVariant>,
    ) -> Self {
        Self {
            action,
            started_at_ms,
            frames: frames_for_variant(body_kind, action, attack_variant),
            direction,
            body_kind,
            attack_variant,
        }
    }

    /// 从渲染层实体构造动画。
    #[must_use]
    pub fn from_view(view: &EntityView) -> Self {
        let body_kind = match view.kind {
            crate::entities::EntityKind::Player => BodyKind::Human,
            crate::entities::EntityKind::Monster | crate::entities::EntityKind::Item => {
                BodyKind::Monster
            }
            crate::entities::EntityKind::Npc => BodyKind::Npc,
        };
        let action = if view.dead && !matches!(view.action, ActorAction::Dying | ActorAction::Dead)
        {
            ActorAction::Dead
        } else {
            view.action
        };
        Self::new_with_variant(
            action,
            view.direction,
            body_kind,
            view.action_started_ms,
            view.attack_variant,
        )
    }

    /// 动作时长（毫秒）。
    #[must_use]
    pub fn duration_ms(&self) -> u64 {
        self.frames.duration_ms()
    }

    /// 当前应该画哪一帧。
    ///
    /// 走/跑播完后返回**站立第 0 帧**（`finished = true`），与 Crystal
    /// `SetAction()` 回 Standing 的行为一致（`PlayerObject.cs:2317-2323`）。
    #[must_use]
    pub fn frame_at(&self, now_ms: u64) -> FrameChoice {
        let elapsed = now_ms.saturating_sub(self.started_at_ms);
        if self.action.loops() {
            let interval = self.frames.interval_ms.max(1);
            let count = u64::from(self.frames.count.max(1));
            let local = u16::try_from((elapsed / interval) % count).unwrap_or(0);
            return FrameChoice {
                index: self.frames.index_of(self.direction, local),
                finished: false,
            };
        }
        if self.action == ActorAction::Dead {
            return FrameChoice {
                index: self.frames.index_of(self.direction, 0),
                finished: false,
            };
        }
        let choice = self.frames.frame_at(self.direction, elapsed);
        if choice.finished && self.action.returns_to_standing() {
            let standing = frames_for(self.body_kind, ActorAction::Standing);
            return FrameChoice {
                index: standing.index_of(self.direction, 0),
                finished: true,
            };
        }
        choice
    }

    /// 动作是否已经播完。
    #[must_use]
    pub fn finished_at(&self, now_ms: u64) -> bool {
        if self.action.loops() || self.action == ActorAction::Dead {
            return false;
        }
        now_ms.saturating_sub(self.started_at_ms) >= self.duration_ms()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOWN: MirDirection = MirDirection::Down;
    const UP_RIGHT: MirDirection = MirDirection::UpRight;

    #[test]
    fn human_tables_match_crystal_counts_and_flat_layout() {
        assert_eq!(HUMAN_STANDING, FrameSequence::new(0, 4, 4, 500));
        assert_eq!(HUMAN_WALKING, FrameSequence::new(64, 6, 2, 100));
        assert_eq!(HUMAN_RUNNING, FrameSequence::new(128, 6, 2, 100));
        assert_eq!(HUMAN_ATTACK1, FrameSequence::new(200, 6, 2, 50));
        assert_eq!(HUMAN_STRUCK, FrameSequence::new(472, 3, 5, 100));
        assert_eq!(HUMAN_DYING, FrameSequence::new(536, 4, 4, 100));
        assert_eq!(HUMAN_DEAD, FrameSequence::new(539, 1, 7, 1000));
        assert_eq!(HUMAN_WALKING.duration_ms(), 600);
        assert_eq!(HUMAN_ATTACK1.duration_ms(), 300);
        assert_eq!(HUMAN_STRUCK.duration_ms(), 300);
        assert_eq!(HUMAN_DYING.duration_ms(), 400);
    }

    #[test]
    fn monster_defaults_match_crystal_and_flat_layout() {
        assert_eq!(MONSTER_STANDING, FrameSequence::new(0, 4, 4, 500));
        assert_eq!(MONSTER_WALKING, FrameSequence::new(64, 6, 2, 100));
        assert_eq!(MONSTER_ATTACK1, FrameSequence::new(128, 6, 2, 50));
        assert_eq!(MONSTER_DYING, FrameSequence::new(344, 6, 2, 100));
        assert_eq!(monster_frames(ActorAction::Running), MONSTER_WALKING);
        assert_eq!(monster_frames(ActorAction::Struck), MONSTER_STRUCK);
    }

    #[test]
    fn frame_at_starts_at_zero_and_steps_by_interval() {
        let sequence = HUMAN_WALKING;
        assert_eq!(sequence.frame_at(DOWN, 0).index, 64 + 4 * 8);
        assert_eq!(sequence.frame_at(DOWN, 99).index, 64 + 4 * 8);
        assert_eq!(sequence.frame_at(DOWN, 100).index, 64 + 4 * 8 + 1);
        assert_eq!(sequence.frame_at(DOWN, 500).index, 64 + 4 * 8 + 5);
        assert!(!sequence.frame_at(DOWN, 500).finished);
        assert!(sequence.frame_at(DOWN, 600).finished);
        assert_eq!(sequence.frame_at(DOWN, 600).index, 64 + 4 * 8 + 5);
        assert_eq!(sequence.frame_at(DOWN, 5_000).index, 64 + 4 * 8 + 5);
    }

    #[test]
    fn skip_is_padding_at_the_end_of_each_direction() {
        // 行走每个方向 6 张有效图 + 2 个空槽：方向 1 从 +8 开始，而不是 +6。
        assert_eq!(HUMAN_WALKING.stride(), 8);
        assert_eq!(HUMAN_WALKING.frame_at(UP_RIGHT, 0).index, 64 + 8);
        assert_eq!(HUMAN_WALKING.frame_at(UP_RIGHT, 100).index, 64 + 8 + 1);
        assert_eq!(HUMAN_STANDING.stride(), 8);
        assert_eq!(HUMAN_STANDING.frame_at(UP_RIGHT, 0).index, 8);
    }

    #[test]
    fn standing_loops_and_never_finishes() {
        let animation = ActorAnimation::new(ActorAction::Standing, DOWN, BodyKind::Human, 1_000);
        assert_eq!(animation.frame_at(1_000).index, 4 * 8);
        assert_eq!(animation.frame_at(1_500).index, 4 * 8 + 1);
        assert_eq!(animation.frame_at(2_000).index, 4 * 8 + 2);
        assert_eq!(animation.frame_at(3_000).index, 4 * 8);
        assert!(!animation.frame_at(99_999).finished);
        assert!(!animation.finished_at(99_999));
    }

    #[test]
    fn walking_returns_to_the_standing_frame_after_600_ms() {
        let animation = ActorAnimation::new(ActorAction::Walking, DOWN, BodyKind::Human, 1_000);
        let early = animation.frame_at(1_000);
        let middle = animation.frame_at(1_300);
        assert!(!early.finished);
        assert_ne!(early.index, middle.index, "走路动画必须在不同时间取不同帧");
        let done = animation.frame_at(1_600);
        assert!(done.finished);
        assert_eq!(done.index, HUMAN_STANDING.index_of(DOWN, 0));
        assert!(animation.finished_at(1_600));
        assert!(!animation.finished_at(1_599));
    }

    #[test]
    fn attacking_holds_the_last_frame_after_finishing() {
        let animation = ActorAnimation::new(ActorAction::Attacking, DOWN, BodyKind::Human, 10_000);
        let last = HUMAN_ATTACK1.index_of(DOWN, 5);
        // 攻击 6 帧 × 50 ms = 300 ms（与服务器 300 ms 命中延迟对齐）。
        assert_eq!(HUMAN_ATTACK1.duration_ms(), 300);
        assert_eq!(
            animation.frame_at(10_250).index,
            HUMAN_ATTACK1.index_of(DOWN, 5)
        );
        assert!(!animation.frame_at(10_250).finished);
        assert_eq!(animation.frame_at(10_300).index, last);
        assert!(animation.frame_at(10_300).finished);
        assert_eq!(animation.frame_at(20_000).index, last, "播完后保持最后一帧");
    }

    #[test]
    fn struck_and_dying_use_crystal_count_and_hold_the_last_frame() {
        let struck = ActorAnimation::new(ActorAction::Struck, DOWN, BodyKind::Human, 0);
        assert_eq!(struck.frame_at(0).index, 472 + 4 * 8);
        assert_eq!(struck.frame_at(200).index, 472 + 4 * 8 + 2);
        assert!(struck.frame_at(300).finished);
        assert_eq!(struck.frame_at(10_000).index, 472 + 4 * 8 + 2);

        let dying = ActorAnimation::new(ActorAction::Dying, DOWN, BodyKind::Human, 0);
        assert_eq!(dying.frame_at(0).index, 536 + 4 * 8);
        assert_eq!(dying.frame_at(300).index, 536 + 4 * 8 + 3);
        assert!(dying.frame_at(400).finished);
    }

    #[test]
    fn dead_always_draws_the_corpse_frame() {
        let animation = ActorAnimation::new(ActorAction::Dead, DOWN, BodyKind::Human, 0);
        assert_eq!(animation.frame_at(0).index, 539 + 4 * 8);
        assert_eq!(animation.frame_at(999_999).index, 539 + 4 * 8);
        assert!(!animation.frame_at(999_999).finished);
        assert!(!animation.finished_at(999_999));
    }

    #[test]
    fn wing_tables_match_crystals_effect_columns() {
        // `Frames.cs:157-170` 每行的第五～八角是 effect 段。
        assert_eq!(WING_STANDING, FrameSequence::new(0, 8, 0, 250));
        assert_eq!(WING_WALKING, FrameSequence::new(64, 8, 0, 100));
        assert_eq!(WING_RUNNING, FrameSequence::new(112, 8, 0, 100));
        assert_eq!(WING_ATTACKING, FrameSequence::new(168, 8, 0, 100));
        assert_eq!(WING_STRUCK, FrameSequence::new(392, 3, 0, 100));
        assert_eq!(WING_DYING, FrameSequence::new(416, 4, 0, 100));
        assert_eq!(WING_DEAD, FrameSequence::new(419, 1, 3, 1000));
        assert_eq!(wing_frames(ActorAction::Running), WING_RUNNING);
        assert_eq!(WING_STANDING.index_of(DOWN, 0), 4 * 8);
        assert_eq!(WING_STANDING.index_of(DOWN, 7), 4 * 8 + 7);
        assert_eq!(WING_STANDING.duration_ms(), 2_000);
    }

    #[test]
    fn r47_attack_variant_logical_vs_converted_tables_and_8_directions() {
        // Crystal 逻辑帧（不是扁平 .wzl 物理索引）。
        assert_eq!(
            AttackVariant::Attack1.crystal_sequence(),
            FrameSequence::new(136, 6, 0, 100)
        );
        assert_eq!(
            AttackVariant::Attack3.crystal_sequence(),
            FrameSequence::new(232, 8, 0, 100)
        );
        // game-assets/hum.wzl 转换后：起点 200/328，每方向 8 槽。
        assert_eq!(
            AttackVariant::Attack1.converted_sequence(),
            FrameSequence::new(200, 6, 2, 100)
        );
        assert_eq!(
            AttackVariant::Attack3.converted_sequence(),
            FrameSequence::new(328, 8, 0, 100)
        );
        assert_eq!(
            AttackVariant::Attack1.converted_sequence().duration_ms(),
            600
        );
        assert_eq!(
            AttackVariant::Attack3.converted_sequence().duration_ms(),
            800
        );

        // 男女共享身体/effect 帧表（Crystal 的性别差异是 ArmourOffSet/WingOffset，
        // 不是 Frames 表的 Start/Count）。这里仍按两性别各跑一次矩阵，防止未来把
        // 性别错误塞进帧索引。
        for _gender in [
            mir_core::enums::MirGender::Male,
            mir_core::enums::MirGender::Female,
        ] {
            for direction in [
                MirDirection::Up,
                MirDirection::UpRight,
                MirDirection::Right,
                MirDirection::DownRight,
                MirDirection::Down,
                MirDirection::DownLeft,
                MirDirection::Left,
                MirDirection::UpLeft,
            ] {
                let dir = u16::from(direction as u8);
                for frame in 0..6 {
                    assert_eq!(
                        AttackVariant::Attack1
                            .converted_sequence()
                            .index_of(direction, frame),
                        200 + dir * 8 + frame
                    );
                    assert_eq!(
                        AttackVariant::Attack1
                            .crystal_sequence()
                            .index_of(direction, frame),
                        136 + dir * 6 + frame,
                        "Crystal 逻辑帧 Start=136、每方向 6 槽"
                    );
                    assert_eq!(
                        AttackVariant::Attack1
                            .converted_effect_sequence()
                            .index_of(direction, frame),
                        168 + dir * 8 + frame
                    );
                    assert_eq!(
                        AttackVariant::Attack1
                            .crystal_effect_sequence()
                            .index_of(direction, frame),
                        168 + dir * 6 + frame
                    );
                }
                for frame in 0..8 {
                    assert_eq!(
                        AttackVariant::Attack3
                            .converted_sequence()
                            .index_of(direction, frame),
                        328 + dir * 8 + frame
                    );
                    assert_eq!(
                        AttackVariant::Attack3
                            .crystal_sequence()
                            .index_of(direction, frame),
                        232 + dir * 8 + frame,
                        "Crystal 逻辑 232 ≠ 物理 328"
                    );
                    assert_eq!(
                        AttackVariant::Attack3
                            .converted_effect_sequence()
                            .index_of(direction, frame),
                        264 + dir * 8 + frame
                    );
                    assert_eq!(
                        AttackVariant::Attack3
                            .crystal_effect_sequence()
                            .index_of(direction, frame),
                        264 + dir * 8 + frame
                    );
                }
            }
        }
    }

    #[test]
    fn r47_attack_variant_boundaries_600_and_800ms() {
        let attack1 = ActorAnimation::new_with_variant(
            ActorAction::Attacking,
            DOWN,
            BodyKind::Human,
            10_000,
            Some(AttackVariant::Attack1),
        );
        assert_eq!(attack1.duration_ms(), 600);
        assert_eq!(attack1.frame_at(10_000).index, 200 + 4 * 8);
        assert_eq!(attack1.frame_at(10_099).index, 200 + 4 * 8);
        assert_eq!(attack1.frame_at(10_100).index, 200 + 4 * 8 + 1);
        assert_eq!(attack1.frame_at(10_300).index, 200 + 4 * 8 + 3);
        assert!(!attack1.frame_at(10_300).finished, "300ms 不得提前结束");
        assert_eq!(attack1.frame_at(10_599).index, 200 + 4 * 8 + 5);
        assert!(attack1.frame_at(10_600).finished);
        assert!(attack1.frame_at(10_900).finished);

        let attack3 = ActorAnimation::new_with_variant(
            ActorAction::Attacking,
            DOWN,
            BodyKind::Human,
            20_000,
            Some(AttackVariant::Attack3),
        );
        assert_eq!(attack3.duration_ms(), 800);
        assert_eq!(attack3.frame_at(20_000).index, 328 + 4 * 8);
        assert_eq!(attack3.frame_at(20_100).index, 328 + 4 * 8 + 1);
        assert_eq!(attack3.frame_at(20_600).index, 328 + 4 * 8 + 6);
        assert_eq!(attack3.frame_at(20_700).index, 328 + 4 * 8 + 7, "末帧起点");
        assert_eq!(attack3.frame_at(20_799).index, 328 + 4 * 8 + 7);
        assert!(!attack3.frame_at(20_799).finished);
        assert!(attack3.frame_at(20_800).finished);
        assert!(attack3.frame_at(20_900).finished);

        // 旧路径（None）仍为 50ms×6=300ms，供非入围分支使用。
        let legacy = ActorAnimation::new(ActorAction::Attacking, DOWN, BodyKind::Human, 0);
        assert_eq!(legacy.duration_ms(), 300);
    }

    #[test]
    fn r47_wing_effect_uses_variant_specific_tables() {
        assert_eq!(
            wing_frames_for(ActorAction::Attacking, Some(AttackVariant::Attack1)),
            FrameSequence::new(168, 6, 2, 100)
        );
        assert_eq!(
            wing_frames_for(ActorAction::Attacking, Some(AttackVariant::Attack3)),
            FrameSequence::new(264, 8, 0, 100)
        );
        assert_eq!(
            wing_frames_for(ActorAction::Attacking, None),
            WING_ATTACKING
        );
    }

    #[test]
    fn monster_frames_use_their_own_offsets() {
        let animation = ActorAnimation::new(ActorAction::Walking, DOWN, BodyKind::Monster, 0);
        assert_eq!(animation.frame_at(0).index, 64 + 4 * 8);
        assert_eq!(animation.frame_at(100).index, 64 + 4 * 8 + 1);
        let attack = ActorAnimation::new(ActorAction::Attacking, DOWN, BodyKind::Monster, 0);
        assert_eq!(attack.frame_at(0).index, 128 + 4 * 8);
        assert!(!attack.frame_at(299).finished);
        assert!(attack.frame_at(300).finished);
    }

    #[test]
    fn monster_native_lifetime_falls_back_to_default_monster() {
        assert_eq!(
            monster_action_duration_ms(ActorAction::Standing, None),
            Some(2_000)
        );
        assert_eq!(
            monster_action_duration_ms(ActorAction::Walking, None),
            Some(600)
        );
        assert_eq!(
            monster_action_duration_ms(ActorAction::Running, None),
            Some(600)
        );
        assert_eq!(
            monster_action_duration_ms(ActorAction::Attacking, None),
            Some(600)
        );
        assert_eq!(
            monster_action_duration_ms(ActorAction::Struck, None),
            Some(400)
        );
        assert_eq!(
            monster_action_duration_ms(ActorAction::Dying, None),
            Some(1_000)
        );
        assert_eq!(monster_action_duration_ms(ActorAction::Dead, None), None);
    }

    #[test]
    fn monster_duration_derives_from_frame_count_and_interval() {
        let frame = mir_asset::Frame {
            action: 9,
            start: 0,
            count: 6,
            skip: 0,
            interval: 200,
            effect_start: 0,
            effect_count: 0,
            effect_skip: 0,
            effect_interval: 0,
            reverse: false,
            blend: false,
        };
        let frames = mir_asset::FrameSet {
            frames: vec![frame],
            unknown_actions: Vec::new(),
        };
        assert_eq!(
            monster_action_duration_ms(ActorAction::Attacking, Some(&frames)),
            Some(1_200)
        );
        // 表里缺 Struck：不能伪造 DefaultMonster 的 400ms，由调用方保持当前动作。
        assert_eq!(
            monster_action_duration_ms(ActorAction::Struck, Some(&frames)),
            None
        );
        assert_eq!(
            monster_action_duration_ms(ActorAction::Dead, Some(&frames)),
            None
        );
    }
}
