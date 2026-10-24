//! 原生玩家素材布局：Crystal `.Lib` 的玩家帧表、外观偏移与按库键加载。
//!
//! 这一层只描述“原生玩家库怎么摆”，不改变现有 `hum*.wzl` 扁平布局的渲染路径。
//! 原生库由 `mir-tools assets unpack --plan` 拆到
//! `gfx/sprites/player-native/<library_key>/`，`library-map.json` 用同一个键登记。
//!
//! # 帧表来源
//!
//! Crystal 的 Player 段在 `Client/MirObjects/Frames.cs:157-185`：
//! 每个动作是 `Frame(Start, Count, Skip, Interval)`；方向 `d` 的第 `f` 帧为
//! `Start + d * (Count + Skip) + f`（`Frames.cs:209-212`、`PlayerObject.cs:761`）。
//! 这里逐项照抄，**不使用**旧 `hum.wzl` 的 8 槽/方向适配值。
//!
//! # 外观偏移来源
//!
//! * 战士/道士/法师：`ArmourOffSet/HairOffSet = Gender==Male ? 0 : 808`，
//!   `WeaponOffSet = Gender==Male ? 0 : 416`（`PlayerObject.cs:584-586`）。
//! * 刺客：女性 `altAnim` 为真时 armour/hair 512、weapon 512；否则
//!   armour/hair 808、weapon 416（`PlayerObject.cs:536-538`）。
//! * 弓手：女性 `altAnim` 为真时 armour/hair 352、weapon 352；否则
//!   armour/hair 808、weapon 416（`PlayerObject.cs:445-447`）。
//!
//! 最终绘制帧号还要加上库偏移（`PlayerObject.cs:5045-5073` 的
//! `DrawFrame + ArmourOffSet` 等）。
//!
//! # 库键
//!
//! 原生目录名会撞 `file_stem`（`CArmour/00.Lib` 和 `AArmour/00.Lib` 都是 `00`），
//! 所以使用带域前缀的稳定键：`carmour-00`、`ahair-00`、`aweapon-00-l`、
//! `aweapon-00-r`、`arweapon-00`、`arweapon-00-s`。

use std::fmt;

use mir_core::enums::{MirClass, MirDirection, MirGender};

use crate::animation::{ActorAction, AttackVariant, FrameSequence};
use crate::entities::EntityLibraries;

/// “没有武器”在 `EntityView::weapon_shape` 里的哨兵值（见 `entities::NO_LAYER`）。
const NO_WEAPON_SHAPE: u16 = u16::MAX;

/// 原生玩家动作（`Frames.cs` Player 段用到的动作名）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NativeAction {
    /// 站立：`Frames.cs:157`。
    Standing,
    /// 行走：`Frames.cs:158`。
    Walking,
    /// 跑步：`Frames.cs:159`。
    Running,
    /// 姿态：`Frames.cs:160`。
    Stance,
    /// 姿态 2（Skip=5，测试方向步长用）：`Frames.cs:161`。
    Stance2,
    /// 攻击 1：`Frames.cs:162`。
    Attack1,
    /// 攻击 2：`Frames.cs:163`。
    Attack2,
    /// 攻击 3：`Frames.cs:164`。
    Attack3,
    /// 攻击 4：`Frames.cs:165`。
    Attack4,
    /// 施法：`Frames.cs:166`。
    Spell,
    /// 挖肉：`Frames.cs:167`。
    Harvest,
    /// 受击：`Frames.cs:168`。
    Struck,
    /// 死亡：`Frames.cs:169`。
    Die,
    /// 尸体（Skip=3，测试方向步长用）：`Frames.cs:170`。
    Dead,
    /// 复活：`Frames.cs:171`。
    Revive,
    /// 挖矿：`Frames.cs:172`。
    Mine,
    /// 突刺：`Frames.cs:173`。
    Lunge,
    /// 潜行：`Frames.cs:176`。
    Sneek,
    /// 冲刺攻击（Skip=3，测试方向步长用）：`Frames.cs:177`。
    DashAttack,
    /// 弓手行走：`Frames.cs:180`。
    WalkingBow,
    /// 弓手跑步：`Frames.cs:181`。
    RunningBow,
    /// 弓手攻击 1：`Frames.cs:182`。
    AttackRange1,
    /// 弓手攻击 2：`Frames.cs:183`。
    AttackRange2,
    /// 弓手攻击 3：`Frames.cs:184`。
    AttackRange3,
    /// 跳跃：`Frames.cs:185`。
    Jump,
}

/// Crystal Player 帧表；`Start/Count/Skip/Interval` 与源码逐项一致。
pub struct NativeFrameTable;

impl NativeFrameTable {
    /// 站立 `Frame(0, 4, 0, 500)`。
    pub const STANDING: FrameSequence = FrameSequence::new(0, 4, 0, 500);
    /// 行走 `Frame(32, 6, 0, 100)`。
    pub const WALKING: FrameSequence = FrameSequence::new(32, 6, 0, 100);
    /// 跑步 `Frame(80, 6, 0, 100)`。
    pub const RUNNING: FrameSequence = FrameSequence::new(80, 6, 0, 100);
    /// 姿态 `Frame(128, 1, 0, 1000)`。
    pub const STANCE: FrameSequence = FrameSequence::new(128, 1, 0, 1000);
    /// 姿态 2 `Frame(300, 1, 5, 1000)`。
    pub const STANCE2: FrameSequence = FrameSequence::new(300, 1, 5, 1000);
    /// 攻击 1 `Frame(136, 6, 0, 100)`。
    pub const ATTACK1: FrameSequence = FrameSequence::new(136, 6, 0, 100);
    /// 攻击 2 `Frame(184, 6, 0, 100)`。
    pub const ATTACK2: FrameSequence = FrameSequence::new(184, 6, 0, 100);
    /// 攻击 3 `Frame(232, 8, 0, 100)`。
    pub const ATTACK3: FrameSequence = FrameSequence::new(232, 8, 0, 100);
    /// 攻击 4 `Frame(416, 6, 0, 100)`。
    pub const ATTACK4: FrameSequence = FrameSequence::new(416, 6, 0, 100);
    /// 施法 `Frame(296, 6, 0, 100)`。
    pub const SPELL: FrameSequence = FrameSequence::new(296, 6, 0, 100);
    /// 受击 `Frame(360, 3, 0, 100)`。
    pub const STRUCK: FrameSequence = FrameSequence::new(360, 3, 0, 100);
    /// 死亡 `Frame(384, 4, 0, 100)`。
    pub const DIE: FrameSequence = FrameSequence::new(384, 4, 0, 100);
    /// 尸体 `Frame(387, 1, 3, 1000)`。
    pub const DEAD: FrameSequence = FrameSequence::new(387, 1, 3, 1000);
    /// 潜行 `Frame(464, 6, 0, 100)`。
    pub const SNEEK: FrameSequence = FrameSequence::new(464, 6, 0, 100);
    /// 冲刺攻击 `Frame(80, 3, 3, 100)`。
    pub const DASH_ATTACK: FrameSequence = FrameSequence::new(80, 3, 3, 100);
    /// 弓手行走 `Frame(0, 6, 0, 100)`。
    pub const WALKING_BOW: FrameSequence = FrameSequence::new(0, 6, 0, 100);
    /// 弓手跑步 `Frame(48, 6, 0, 100)`。
    pub const RUNNING_BOW: FrameSequence = FrameSequence::new(48, 6, 0, 100);
    /// 弓手攻击 1 `Frame(96, 8, 0, 100)`。
    pub const ATTACK_RANGE1: FrameSequence = FrameSequence::new(96, 8, 0, 100);
    /// 弓手攻击 2 `Frame(160, 8, 0, 100)`。
    pub const ATTACK_RANGE2: FrameSequence = FrameSequence::new(160, 8, 0, 100);
    /// 弓手攻击 3 `Frame(224, 8, 0, 100)`。
    pub const ATTACK_RANGE3: FrameSequence = FrameSequence::new(224, 8, 0, 100);
    /// 跳跃 `Frame(288, 8, 0, 100)`。
    pub const JUMP: FrameSequence = FrameSequence::new(288, 8, 0, 100);

    /// 按动作取帧表。
    #[must_use]
    pub const fn get(action: NativeAction) -> FrameSequence {
        match action {
            NativeAction::Standing => Self::STANDING,
            NativeAction::Walking => Self::WALKING,
            NativeAction::Running => Self::RUNNING,
            NativeAction::Stance => Self::STANCE,
            NativeAction::Stance2 => Self::STANCE2,
            NativeAction::Attack1 => Self::ATTACK1,
            NativeAction::Attack2 => Self::ATTACK2,
            NativeAction::Attack3 => Self::ATTACK3,
            NativeAction::Attack4 => Self::ATTACK4,
            NativeAction::Spell => Self::SPELL,
            NativeAction::Harvest => FrameSequence::new(344, 2, 0, 300),
            NativeAction::Struck => Self::STRUCK,
            NativeAction::Die => Self::DIE,
            NativeAction::Dead => Self::DEAD,
            NativeAction::Revive => FrameSequence::new(384, 4, 0, 100),
            NativeAction::Mine => FrameSequence::new(184, 6, 0, 100),
            NativeAction::Lunge => FrameSequence::new(139, 1, 5, 1000),
            NativeAction::Sneek => Self::SNEEK,
            NativeAction::DashAttack => Self::DASH_ATTACK,
            NativeAction::WalkingBow => Self::WALKING_BOW,
            NativeAction::RunningBow => Self::RUNNING_BOW,
            NativeAction::AttackRange1 => Self::ATTACK_RANGE1,
            NativeAction::AttackRange2 => Self::ATTACK_RANGE2,
            NativeAction::AttackRange3 => Self::ATTACK_RANGE3,
            NativeAction::Jump => Self::JUMP,
        }
    }
}

/// Crystal Player 的 effect 帧表（`Frames.cs:157-185` 每项的 `EffectStart/
/// EffectCount/EffectSkip/EffectInterval`）。
///
/// 翅膀/光效必须使用这张表，不能复用身体帧表：例如站立 body 是 4×500ms，
/// effect 是 8×250ms；Attack3 body 从 232 起，effect 从 264 起。
pub struct NativeEffectTable;

impl NativeEffectTable {
    /// 按动作取 effect 帧表。
    #[must_use]
    pub const fn get(action: NativeAction) -> FrameSequence {
        match action {
            NativeAction::Standing => FrameSequence::new(0, 8, 0, 250),
            NativeAction::Walking => FrameSequence::new(64, 6, 0, 100),
            NativeAction::Running => FrameSequence::new(112, 6, 0, 100),
            NativeAction::Stance => FrameSequence::new(160, 1, 0, 1000),
            NativeAction::Stance2 => FrameSequence::new(332, 1, 5, 1000),
            NativeAction::Attack1 => FrameSequence::new(168, 6, 0, 100),
            NativeAction::Attack2 | NativeAction::Mine => FrameSequence::new(216, 6, 0, 100),
            NativeAction::Attack3 => FrameSequence::new(264, 8, 0, 100),
            NativeAction::Attack4 => FrameSequence::new(448, 6, 0, 100),
            NativeAction::Spell => FrameSequence::new(328, 6, 0, 100),
            NativeAction::Harvest => FrameSequence::new(376, 2, 0, 300),
            NativeAction::Struck => FrameSequence::new(392, 3, 0, 100),
            NativeAction::Die | NativeAction::Revive => FrameSequence::new(416, 4, 0, 100),
            NativeAction::Dead => FrameSequence::new(419, 1, 3, 1000),
            NativeAction::Lunge => FrameSequence::new(300, 1, 5, 1000),
            NativeAction::Sneek => FrameSequence::new(496, 6, 0, 100),
            NativeAction::DashAttack => FrameSequence::new(112, 3, 3, 100),
            NativeAction::WalkingBow => FrameSequence::new(0, 6, 0, 100),
            NativeAction::RunningBow => FrameSequence::new(48, 6, 0, 100),
            NativeAction::AttackRange1 => FrameSequence::new(96, 8, 0, 100),
            NativeAction::AttackRange2 => FrameSequence::new(160, 8, 0, 100),
            NativeAction::AttackRange3 => FrameSequence::new(224, 8, 0, 100),
            NativeAction::Jump => FrameSequence::new(288, 8, 0, 100),
        }
    }
}

/// 原生玩家的绘制层顺序（Crystal `PlayerObject.cs:4886-4922`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeDrawLayer {
    /// 身体（`DrawBody`）。
    Body,
    /// 头发（`DrawHead`）。
    Head,
    /// 翅膀/特效（`DrawWings`）。
    Wing,
    /// 某只手的武器（`DrawWeapon`/`DrawWeapon2`）。
    Weapon(NativeHand),
}

/// 计算一帧的原生层顺序。
///
/// Crystal 按方向把一只手放到身体前、另一只手放到身体后：
/// 左/上/左上/左下先画 `WeaponLibrary1`，其余方向先画 `WeaponLibrary2`；
/// 身体之后，Up/UpLeft/UpRight/Right/Left 先头发后翅膀，其余方向先翅膀后头发。
/// 最后再画另一只手。`has_main`/`has_off` 决定实际存在的库；弓手的 AR 武器是
/// Crystal `WeaponLibrary2`，`archer_extra_off` 为真时它不参与普通的前/后手相位，
/// 而是在身体之后按 Crystal `PlayerObject.cs:4921-4922` 对**所有方向**额外画一次。
#[must_use]
pub fn draw_order(
    direction: MirDirection,
    has_main: bool,
    has_off: bool,
    archer_extra_off: bool,
) -> ([NativeDrawLayer; 6], usize) {
    let left_group = matches!(
        direction,
        MirDirection::Left | MirDirection::Up | MirDirection::UpLeft | MirDirection::DownLeft
    );
    let head_first = matches!(
        direction,
        MirDirection::Up
            | MirDirection::UpLeft
            | MirDirection::UpRight
            | MirDirection::Right
            | MirDirection::Left
    );
    let (pre, post) = if left_group {
        (NativeHand::Main, NativeHand::Off)
    } else {
        (NativeHand::Off, NativeHand::Main)
    };
    let mut layers = [NativeDrawLayer::Body; 6];
    let mut len = 0;
    let mut push = |layer| {
        layers[len] = layer;
        len += 1;
    };
    match pre {
        NativeHand::Main if has_main => push(NativeDrawLayer::Weapon(NativeHand::Main)),
        NativeHand::Off if has_off => push(NativeDrawLayer::Weapon(NativeHand::Off)),
        _ => {}
    }
    push(NativeDrawLayer::Body);
    if head_first {
        push(NativeDrawLayer::Head);
        push(NativeDrawLayer::Wing);
    } else {
        push(NativeDrawLayer::Wing);
        push(NativeDrawLayer::Head);
    }
    match post {
        NativeHand::Main if has_main => push(NativeDrawLayer::Weapon(NativeHand::Main)),
        NativeHand::Off if has_off => push(NativeDrawLayer::Weapon(NativeHand::Off)),
        _ => {}
    }
    // Crystal `PlayerObject.cs:4921-4922`：弓手持职业武器时，无论方向，
    // 都在方向分支之后无条件再 `DrawWeapon2()`。因为弓手 Main=null，这不会
    // 画主手，只会让 Off 的 AR 武器在左组末尾出现第二次（同层重复幂等）。
    if archer_extra_off && has_off {
        push(NativeDrawLayer::Weapon(NativeHand::Off));
    }
    (layers, len)
}

/// 把客户端的简化动作映射到 Crystal 的 Player 动作。
///
/// `PlayerObject.cs:1271-1283` 对持弓的弓手把 Walking/Running 换成
/// `WalkingBow/RunningBow`；攻击仍由 [`AttackVariant`] 区分 A1..A4。
#[must_use]
pub fn native_action(
    class: MirClass,
    alt_anim: bool,
    action: ActorAction,
    attack_variant: Option<AttackVariant>,
) -> NativeAction {
    match action {
        ActorAction::Standing => NativeAction::Standing,
        ActorAction::Walking if class == MirClass::Archer && alt_anim => NativeAction::WalkingBow,
        ActorAction::Walking => NativeAction::Walking,
        ActorAction::Running if class == MirClass::Archer && alt_anim => NativeAction::RunningBow,
        ActorAction::Running => NativeAction::Running,
        // A2/A4 由原生选招结果直接提供（`EntityView::native_action`），
        // `AttackVariant` 仍只承载 R47 的 A1/A3 视觉变体。
        ActorAction::Attacking => match attack_variant {
            Some(AttackVariant::Attack3) => NativeAction::Attack3,
            Some(AttackVariant::Attack1) | None => NativeAction::Attack1,
        },
        ActorAction::Struck => NativeAction::Struck,
        ActorAction::Dying => NativeAction::Die,
        ActorAction::Dead => NativeAction::Dead,
    }
}

/// 武器所属的 Crystal 外观域。它由 `Weapon / 100` 决定，和职业/altAnim 无关；
/// 只有与当前职业匹配时才允许走职业武器库。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WeaponSegment {
    /// `CWeapons`（0..99）。
    Common,
    /// `AWeaponsR/L`（100..199，刺客）。
    Assassin,
    /// `ARWeapons/ARWeaponsS`（200..299，弓手）。
    Archer,
}

impl WeaponSegment {
    /// 按原始外观号分段。
    #[must_use]
    pub const fn from_shape(shape: u16) -> Self {
        if shape < 100 {
            Self::Common
        } else if shape < 200 {
            Self::Assassin
        } else if shape < 300 {
            Self::Archer
        } else {
            Self::Common
        }
    }
}

/// 一个玩家当前帧的原生库选择状态。
///
/// `alt_anim` 是 Crystal `PlayerObject.cs:267` 的**逐帧局部变量**：刺客/弓手只在
/// 对应动作集合且持职业武器时为真；其余动作和武器回 C 系。身体/头发/翅膀随
/// `alt_anim` 换域，武器另按 [`WeaponSegment`] 换域。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlayerLayout {
    /// 职业。
    pub class: MirClass,
    /// 性别。
    pub gender: MirGender,
    /// 当前动作是否走 alt 外观（Crystal `altAnim`）。
    pub alt_anim: bool,
    /// 当前武器段。
    pub weapon_segment: WeaponSegment,
}

impl PlayerLayout {
    /// 构造一个明确指定 `alt_anim` 的布局（测试/调用方已判定动作时用）。
    #[must_use]
    pub const fn new(
        class: MirClass,
        gender: MirGender,
        alt_anim: bool,
        weapon_shape: u16,
    ) -> Self {
        Self {
            class,
            gender,
            alt_anim,
            weapon_segment: WeaponSegment::from_shape(weapon_shape),
        }
    }

    /// 按动作计算 Crystal 的 `altAnim`，再构造布局。
    ///
    /// * 刺客：`HasClassWeapon || Weapon < 0` 且在 `:462-479` 列出的动作集合；
    /// * 弓手：持弓且在 `Walking/Running/AttackRange1/AttackRange2`，`Jump` 单独为真；
    /// * 战/道/法：恒假。
    ///
    /// 本作尚未建模 `AttackRange1..3`/`Sneek`/`Spell` 等动作；当前用 `Attacking`
    /// 覆盖攻击集合，`Dying/Dead` 对应 `Die`。未建模的动作按“不在集合”处理。
    #[must_use]
    pub fn for_action(
        class: MirClass,
        gender: MirGender,
        action: ActorAction,
        attack_variant: Option<AttackVariant>,
        weapon_shape: u16,
        riding: bool,
    ) -> Option<Self> {
        let native = Self::native_for_actor(class, action, attack_variant, weapon_shape);
        Self::for_native_action(class, gender, native, weapon_shape, riding)
    }

    /// 把本作的粗粒度 [`ActorAction`] 映射成 Crystal 原生动作。
    ///
    /// 弓手持职业武器时，`Walking/Running` 对应 `WalkingBow/RunningBow`；普通
    /// `Attacking` 仍映射 A1/A3（只有调用方明确给出 `AttackRange1/2` 时才走远程表）。
    #[must_use]
    pub fn native_for_actor(
        class: MirClass,
        action: ActorAction,
        attack_variant: Option<AttackVariant>,
        weapon_shape: u16,
    ) -> NativeAction {
        let has_class_weapon = matches!(
            (class, WeaponSegment::from_shape(weapon_shape)),
            (MirClass::Assassin, WeaponSegment::Assassin)
                | (MirClass::Archer, WeaponSegment::Archer)
        );
        match action {
            ActorAction::Standing => NativeAction::Standing,
            ActorAction::Walking if class == MirClass::Archer && has_class_weapon => {
                NativeAction::WalkingBow
            }
            ActorAction::Walking => NativeAction::Walking,
            ActorAction::Running if class == MirClass::Archer && has_class_weapon => {
                NativeAction::RunningBow
            }
            ActorAction::Running => NativeAction::Running,
            ActorAction::Attacking => match attack_variant {
                Some(AttackVariant::Attack3) => NativeAction::Attack3,
                Some(AttackVariant::Attack1) | None => NativeAction::Attack1,
            },
            ActorAction::Struck => NativeAction::Struck,
            ActorAction::Dying => NativeAction::Die,
            ActorAction::Dead => NativeAction::Dead,
        }
    }

    /// 按已经确定的 [`NativeAction`] 计算布局。
    ///
    /// 这是唯一决定 `altAnim` 的入口：调用方必须先选定动作（例如弓手
    /// `AttackRange1/2` 与普通 `Attack1`），不能只看 `ActorAction::Attacking`。
    #[must_use]
    pub fn for_native_action(
        class: MirClass,
        gender: MirGender,
        native: NativeAction,
        weapon_shape: u16,
        riding: bool,
    ) -> Option<Self> {
        if riding {
            return None;
        }
        let segment = WeaponSegment::from_shape(weapon_shape);
        let has_class_weapon = matches!(
            (class, segment),
            (MirClass::Assassin, WeaponSegment::Assassin)
                | (MirClass::Archer, WeaponSegment::Archer)
        );
        let alt_anim = match class {
            MirClass::Assassin => {
                let has = has_class_weapon || weapon_shape == NO_WEAPON_SHAPE;
                has && matches!(
                    native,
                    NativeAction::Standing
                        | NativeAction::Walking
                        | NativeAction::Running
                        | NativeAction::Attack1
                        | NativeAction::Attack2
                        | NativeAction::Attack3
                        | NativeAction::Attack4
                        | NativeAction::Struck
                        | NativeAction::Die
                        | NativeAction::Sneek
                        | NativeAction::Spell
                        | NativeAction::DashAttack
                )
            }
            MirClass::Archer => {
                has_class_weapon
                    && matches!(
                        native,
                        NativeAction::WalkingBow
                            | NativeAction::RunningBow
                            | NativeAction::AttackRange1
                            | NativeAction::AttackRange2
                            | NativeAction::Jump
                    )
            }
            MirClass::Warrior | MirClass::Wizard | MirClass::Taoist => false,
        };
        Some(Self::new(class, gender, alt_anim, weapon_shape))
    }

    /// 对应的原生库身份。
    #[must_use]
    pub const fn library_id(self) -> NativeLibraryId {
        NativeLibraryId::new(self.class, self.gender, self.alt_anim)
    }

    /// 当前动作对应的原生动作。
    #[must_use]
    pub fn native_action(
        self,
        action: ActorAction,
        attack_variant: Option<AttackVariant>,
    ) -> NativeAction {
        native_action(self.class, self.alt_anim, action, attack_variant)
    }
}

/// 原生帧号：`Start + direction * (Count + Skip) + frame`，再加该层的外观偏移。
///
/// 偏移来自 [`NativeLibraryId::armour_offset`] / [`NativeLibraryId::weapon_offset`]；
/// 传 `0` 可单独核对帧表。
#[must_use]
pub fn frame_index(
    sequence: FrameSequence,
    direction: MirDirection,
    frame: u16,
    layer_offset: u16,
) -> u32 {
    let local = frame.min(sequence.count.saturating_sub(1));
    u32::from(sequence.start)
        + u32::from(direction as u8) * u32::from(sequence.stride())
        + u32::from(local)
        + u32::from(layer_offset)
}

/// 原生玩家可以选择的层。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NativeLayer {
    /// 身体（`ArmourOffSet`）。
    Armour,
    /// 头发（`HairOffSet`，与身体同偏移）。
    Hair,
    /// 翅膀/特效（Crystal `HumEffect`；域随 altAnim，偏移独立 `WingOffset`）。
    Wing,
    /// 武器；刺客区分 R/L，弓手只画主手。
    Weapon(NativeHand),
}

/// 武器主副手。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NativeHand {
    /// 主手：刺客 `R`、弓手无后缀、战士/道士/法师唯一武器。
    Main,
    /// 副手：刺客 `L`、弓手 `S`；其他职业返回 `None`。
    Off,
}

/// 原生玩家库的身份（职业 + 性别 + `altAnim`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NativeLibraryId {
    /// 职业。
    pub class: MirClass,
    /// 性别。
    pub gender: MirGender,
    /// Crystal 的 `altAnim`（女性刺客/弓手的备用动作段）。
    pub alt_anim: bool,
}

impl NativeLibraryId {
    /// 新建身份。
    #[must_use]
    pub const fn new(class: MirClass, gender: MirGender, alt_anim: bool) -> Self {
        Self {
            class,
            gender,
            alt_anim,
        }
    }

    /// 是否使用 C 域（战/道/法）的通用外观。
    #[must_use]
    pub const fn is_common(self) -> bool {
        matches!(
            self.class,
            MirClass::Warrior | MirClass::Wizard | MirClass::Taoist
        )
    }

    /// 该身份的身体/头发库偏移。
    #[must_use]
    pub const fn armour_offset(self) -> u16 {
        if self.is_common() {
            return match self.gender {
                MirGender::Male => 0,
                MirGender::Female => 808,
            };
        }
        match (self.class, self.gender) {
            (MirClass::Assassin, MirGender::Female) => {
                if self.alt_anim {
                    512
                } else {
                    808
                }
            }
            (MirClass::Archer, MirGender::Female) => {
                if self.alt_anim {
                    352
                } else {
                    808
                }
            }
            _ => 0,
        }
    }

    /// 该身份的头发偏移（与身体相同）。
    #[must_use]
    pub const fn hair_offset(self) -> u16 {
        self.armour_offset()
    }

    /// 该身份的武器库偏移。
    #[must_use]
    pub const fn weapon_offset(self) -> u16 {
        if self.is_common() {
            return match self.gender {
                MirGender::Male => 0,
                MirGender::Female => 416,
            };
        }
        match (self.class, self.gender) {
            (MirClass::Assassin, MirGender::Female) => {
                if self.alt_anim {
                    512
                } else {
                    416
                }
            }
            (MirClass::Archer, MirGender::Female) => {
                if self.alt_anim {
                    352
                } else {
                    416
                }
            }
            _ => 0,
        }
    }

    /// 该身份的翅膀/特效库偏移（Crystal `WingOffset`）。
    ///
    /// 与身体偏移不同：普通女性 840，刺客 alt 544，弓手 alt 352；男性恒 0。
    #[must_use]
    pub const fn wing_offset(self) -> u16 {
        if matches!(self.gender, MirGender::Male) {
            return 0;
        }
        match (self.class, self.alt_anim) {
            (MirClass::Assassin, true) => 544,
            (MirClass::Archer, true) => 352,
            _ => 840,
        }
    }

    /// 域前缀（`c` / `a` / `ar`）。
    #[must_use]
    pub const fn domain(self) -> &'static str {
        if !self.alt_anim || self.is_common() {
            "c"
        } else {
            match self.class {
                MirClass::Assassin => "a",
                MirClass::Archer => "ar",
                _ => "c",
            }
        }
    }

    /// 库键；形状号越界或该职业没有该层时返回 `None`。
    ///
    /// * C：`carmour-00` / `chair-00` / `cweapon-00`（`Weapon` 0..）；
    /// * A：`aarmour-00` / `ahair-00` / `aweapon-00-r`、`aweapon-00-l`
    ///   （`Weapon` 100..，主手 R、副手 L）；
    /// * AR：`ararmour-00` / `arhair-00` / `arweapon-00`、`arweapon-00-s`
    ///   （`Weapon` 200..；`altAnim` 为真才用 `-s`，且不区分左右手）。
    ///
    /// 身体/头发/翅膀的域随 `altAnim`；武器域只随武器号和职业，不随
    /// `altAnim` 丢掉职业武器库（Crystal `PlayerObject.cs:389-412/484-503`）。
    #[must_use]
    pub fn library_key(self, layer: NativeLayer, shape: u16) -> Option<String> {
        if let NativeLayer::Weapon(hand) = layer {
            return self.weapon_key(hand, shape);
        }
        if !self.is_common() && self.alt_anim {
            let domain = self.domain();
            return match layer {
                NativeLayer::Armour => (shape < 100).then(|| format!("{domain}armour-{shape:02}")),
                NativeLayer::Hair => (shape < 100).then(|| format!("{domain}hair-{shape:02}")),
                NativeLayer::Wing => {
                    (shape < 100).then(|| format!("{domain}hum-effect-{shape:02}"))
                }
                NativeLayer::Weapon(_) => None,
            };
        }
        match layer {
            NativeLayer::Armour => (shape < 100).then(|| format!("carmour-{shape:02}")),
            NativeLayer::Hair => (shape < 100).then(|| format!("chair-{shape:02}")),
            NativeLayer::Wing => (shape < 100).then(|| format!("chum-effect-{shape:02}")),
            NativeLayer::Weapon(_) => None,
        }
    }

    /// 武器库：C 武器、刺客职业武器、弓手职业武器三套；域只看武器段与职业。
    fn weapon_key(self, hand: NativeHand, shape: u16) -> Option<String> {
        match (self.class, WeaponSegment::from_shape(shape)) {
            (MirClass::Assassin, WeaponSegment::Assassin) => {
                let suffix = match hand {
                    NativeHand::Main => "r",
                    NativeHand::Off => "l",
                };
                Some(format!("aweapon-{:02}-{suffix}", shape - 100))
            }
            (MirClass::Archer, WeaponSegment::Archer) => match hand {
                // Crystal 把弓放进 `WeaponLibrary2`（`PlayerObject.cs:406-411`），
                // 对应本实现的 Off 槽；`Main` 为空，绘制时由 `draw_order`
                // 在身体后按所有方向补画一次。
                NativeHand::Main => None,
                NativeHand::Off => Some(format!(
                    "arweapon-{:02}{}",
                    shape - 200,
                    if self.alt_anim { "-s" } else { "" }
                )),
            },
            (_, WeaponSegment::Common) => {
                (shape < 100 && hand == NativeHand::Main).then(|| format!("cweapon-{shape:02}"))
            }
            _ => None,
        }
    }
}

/// 按 `library_key` 从已配置的 [`EntityLibraries`]（散装或小包）取原生库。
///
/// 这里不复制图库，只把“键 → 来源”的查找集中到一个有名字的入口，避免调用方
/// 又走回 `file_stem` 撞名路径。
pub struct NativeLibrary;

impl NativeLibrary {
    /// 打开一个原生库；缺库返回 `None`。
    pub fn open<'a>(
        libraries: &'a mut EntityLibraries,
        key: &str,
    ) -> Option<&'a mir_asset::LibrarySource> {
        libraries.native_library(key)
    }
}

impl fmt::Display for NativeLibraryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}/{:?}/alt={}",
            self.domain(),
            self.gender,
            self.alt_anim
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_table_matches_crystal_and_handles_skip() {
        assert_eq!(NativeFrameTable::STANDING, FrameSequence::new(0, 4, 0, 500));
        assert_eq!(NativeFrameTable::WALKING, FrameSequence::new(32, 6, 0, 100));
        assert_eq!(NativeFrameTable::RUNNING, FrameSequence::new(80, 6, 0, 100));
        assert_eq!(
            NativeFrameTable::ATTACK1,
            FrameSequence::new(136, 6, 0, 100)
        );
        assert_eq!(
            NativeFrameTable::ATTACK3,
            FrameSequence::new(232, 8, 0, 100)
        );
        assert_eq!(
            NativeFrameTable::STANCE2,
            FrameSequence::new(300, 1, 5, 1000)
        );
        assert_eq!(NativeFrameTable::DEAD, FrameSequence::new(387, 1, 3, 1000));
        assert_eq!(
            NativeFrameTable::DASH_ATTACK,
            FrameSequence::new(80, 3, 3, 100)
        );

        // 方向步长包含 Skip：Stance2 的 d=1 在第 6 个槽，Dead 的 d=1 在第 4 个槽。
        assert_eq!(
            frame_index(NativeFrameTable::STANCE2, MirDirection::UpRight, 0, 0),
            306
        );
        assert_eq!(
            frame_index(NativeFrameTable::DEAD, MirDirection::UpRight, 0, 0),
            391
        );
        assert_eq!(
            frame_index(NativeFrameTable::DASH_ATTACK, MirDirection::Right, 2, 0),
            94
        );
    }

    #[test]
    fn every_player_action_matches_crystal_start_count_skip() {
        let expected = [
            (NativeAction::Standing, 0, 4, 0, 500),
            (NativeAction::Walking, 32, 6, 0, 100),
            (NativeAction::Running, 80, 6, 0, 100),
            (NativeAction::Stance, 128, 1, 0, 1000),
            (NativeAction::Stance2, 300, 1, 5, 1000),
            (NativeAction::Attack1, 136, 6, 0, 100),
            (NativeAction::Attack2, 184, 6, 0, 100),
            (NativeAction::Attack3, 232, 8, 0, 100),
            (NativeAction::Attack4, 416, 6, 0, 100),
            (NativeAction::Spell, 296, 6, 0, 100),
            (NativeAction::Harvest, 344, 2, 0, 300),
            (NativeAction::Struck, 360, 3, 0, 100),
            (NativeAction::Die, 384, 4, 0, 100),
            (NativeAction::Dead, 387, 1, 3, 1000),
            (NativeAction::Revive, 384, 4, 0, 100),
            (NativeAction::Mine, 184, 6, 0, 100),
            (NativeAction::Lunge, 139, 1, 5, 1000),
            (NativeAction::Sneek, 464, 6, 0, 100),
            (NativeAction::DashAttack, 80, 3, 3, 100),
            (NativeAction::WalkingBow, 0, 6, 0, 100),
            (NativeAction::RunningBow, 48, 6, 0, 100),
            (NativeAction::AttackRange1, 96, 8, 0, 100),
            (NativeAction::AttackRange2, 160, 8, 0, 100),
            (NativeAction::AttackRange3, 224, 8, 0, 100),
            (NativeAction::Jump, 288, 8, 0, 100),
        ];
        for (action, start, count, skip, interval) in expected {
            assert_eq!(
                NativeFrameTable::get(action),
                FrameSequence::new(start, count, skip, interval),
                "{action:?}"
            );
        }
    }

    #[test]
    fn wing_key_uses_wing_shape_not_zero_and_has_independent_offset() {
        let female = NativeLibraryId::new(MirClass::Warrior, MirGender::Female, false);
        assert_eq!(
            female.library_key(NativeLayer::Wing, 4).as_deref(),
            Some("chum-effect-04")
        );
        assert_eq!(female.wing_offset(), 840);
        let assassin_alt = NativeLibraryId::new(MirClass::Assassin, MirGender::Female, true);
        assert_eq!(
            assassin_alt.library_key(NativeLayer::Wing, 4).as_deref(),
            Some("ahum-effect-04")
        );
        assert_eq!(assassin_alt.wing_offset(), 544);
        let archer_alt = NativeLibraryId::new(MirClass::Archer, MirGender::Female, true);
        assert_eq!(
            archer_alt.library_key(NativeLayer::Wing, 4).as_deref(),
            Some("arhum-effect-04")
        );
        assert_eq!(archer_alt.wing_offset(), 352);
    }

    #[test]
    fn offsets_match_player_object() {
        let male = NativeLibraryId::new(MirClass::Warrior, MirGender::Male, false);
        let female = NativeLibraryId::new(MirClass::Warrior, MirGender::Female, false);
        assert_eq!((male.armour_offset(), male.weapon_offset()), (0, 0));
        assert_eq!((female.armour_offset(), female.weapon_offset()), (808, 416));

        let a_female_alt = NativeLibraryId::new(MirClass::Assassin, MirGender::Female, true);
        let a_female = NativeLibraryId::new(MirClass::Assassin, MirGender::Female, false);
        assert_eq!(
            (a_female_alt.armour_offset(), a_female_alt.weapon_offset()),
            (512, 512)
        );
        assert_eq!(
            (a_female.armour_offset(), a_female.weapon_offset()),
            (808, 416)
        );

        let ar_female_alt = NativeLibraryId::new(MirClass::Archer, MirGender::Female, true);
        let ar_female = NativeLibraryId::new(MirClass::Archer, MirGender::Female, false);
        assert_eq!(
            (ar_female_alt.armour_offset(), ar_female_alt.weapon_offset()),
            (352, 352)
        );
        assert_eq!(
            (ar_female.armour_offset(), ar_female.weapon_offset()),
            (808, 416)
        );
        assert_eq!(
            (
                male.wing_offset(),
                female.wing_offset(),
                a_female_alt.wing_offset(),
                a_female.wing_offset(),
                ar_female_alt.wing_offset(),
                ar_female.wing_offset(),
            ),
            (0, 840, 544, 840, 352, 840)
        );
    }

    #[test]
    fn library_keys_cover_class_suffixes_and_domains() {
        let c = NativeLibraryId::new(MirClass::Warrior, MirGender::Male, false);
        assert_eq!(
            c.library_key(NativeLayer::Armour, 0).as_deref(),
            Some("carmour-00")
        );
        assert_eq!(
            c.library_key(NativeLayer::Weapon(NativeHand::Main), 5)
                .as_deref(),
            Some("cweapon-05")
        );
        assert_eq!(c.library_key(NativeLayer::Weapon(NativeHand::Off), 5), None);

        let a = NativeLibraryId::new(MirClass::Assassin, MirGender::Female, false);
        assert_eq!(
            a.library_key(NativeLayer::Armour, 0).as_deref(),
            Some("carmour-00")
        );
        assert_eq!(a.armour_offset(), 808);
        assert_eq!(
            a.library_key(NativeLayer::Weapon(NativeHand::Main), 100)
                .as_deref(),
            Some("aweapon-00-r")
        );
        assert_eq!(
            a.library_key(NativeLayer::Weapon(NativeHand::Main), 5)
                .as_deref(),
            Some("cweapon-05")
        );

        let a_alt = NativeLibraryId::new(MirClass::Assassin, MirGender::Female, true);
        assert_eq!(
            a_alt.library_key(NativeLayer::Armour, 0).as_deref(),
            Some("aarmour-00")
        );
        assert_eq!(
            a_alt
                .library_key(NativeLayer::Weapon(NativeHand::Main), 100)
                .as_deref(),
            Some("aweapon-00-r")
        );
        assert_eq!(
            a_alt
                .library_key(NativeLayer::Weapon(NativeHand::Off), 100)
                .as_deref(),
            Some("aweapon-00-l")
        );

        let ar = NativeLibraryId::new(MirClass::Archer, MirGender::Female, false);
        assert_eq!(
            ar.library_key(NativeLayer::Armour, 0).as_deref(),
            Some("carmour-00")
        );
        assert_eq!(
            ar.library_key(NativeLayer::Weapon(NativeHand::Main), 200),
            None
        );
        assert_eq!(
            ar.library_key(NativeLayer::Weapon(NativeHand::Off), 200)
                .as_deref(),
            Some("arweapon-00")
        );
        assert_eq!(
            ar.library_key(NativeLayer::Weapon(NativeHand::Off), 250)
                .as_deref(),
            Some("arweapon-50")
        );
        assert_eq!(
            ar.library_key(NativeLayer::Hair, 8).as_deref(),
            Some("chair-08")
        );

        let ar_alt = NativeLibraryId::new(MirClass::Archer, MirGender::Female, true);
        assert_eq!(
            ar_alt.library_key(NativeLayer::Armour, 0).as_deref(),
            Some("ararmour-00")
        );
        assert_eq!(ar_alt.armour_offset(), 352);
        assert_eq!(
            ar_alt.library_key(NativeLayer::Weapon(NativeHand::Main), 200),
            None
        );
        assert_eq!(
            ar_alt
                .library_key(NativeLayer::Weapon(NativeHand::Off), 200)
                .as_deref(),
            Some("arweapon-00-s")
        );
        assert_eq!(
            ar_alt.library_key(NativeLayer::Hair, 8).as_deref(),
            Some("arhair-08")
        );
    }

    #[test]
    fn alt_anim_selection_matches_crystal_action_sets() {
        // 刺客持职业武器：站立/走/跑/攻击/受击/死亡走 A 域。
        for action in [
            ActorAction::Standing,
            ActorAction::Walking,
            ActorAction::Running,
            ActorAction::Attacking,
            ActorAction::Struck,
            ActorAction::Dying,
        ] {
            let layout = PlayerLayout::for_action(
                MirClass::Assassin,
                MirGender::Female,
                action,
                Some(AttackVariant::Attack3),
                100,
                false,
            )
            .expect("骰子局");
            assert!(layout.alt_anim, "{action:?}");
            assert_eq!(layout.library_id().armour_offset(), 512);
        }
        // 普通武器（非职业武器）始终 C 域。
        let c = PlayerLayout::for_action(
            MirClass::Assassin,
            MirGender::Female,
            ActorAction::Standing,
            None,
            5,
            false,
        )
        .expect("C 局");
        assert!(!c.alt_anim);
        assert_eq!(c.library_id().armour_offset(), 808);

        // 弓手持职业武器：只有弓动作（WalkingBow/RunningBow）与远程攻击
        // AttackRange1/2、Jump 走 AR 域；普通 Attack1/站立仍 C 域。
        for action in [ActorAction::Walking, ActorAction::Running] {
            let layout = PlayerLayout::for_action(
                MirClass::Archer,
                MirGender::Female,
                action,
                Some(AttackVariant::Attack1),
                200,
                false,
            )
            .expect("AR 局");
            assert!(layout.alt_anim, "{action:?}");
            assert_eq!(layout.library_id().armour_offset(), 352);
        }
        for native in [NativeAction::AttackRange1, NativeAction::AttackRange2] {
            let layout = PlayerLayout::for_native_action(
                MirClass::Archer,
                MirGender::Female,
                native,
                200,
                false,
            )
            .expect("AR 远程局");
            assert!(layout.alt_anim, "{native:?}");
            assert_eq!(layout.library_id().armour_offset(), 352);
            assert_eq!(NativeFrameTable::get(native).count, 8);
        }
        // 普通近战 Attack1 不是 altAnim，必须仍拿 C 系，不能 808 偏移查 ararmour。
        let melee = PlayerLayout::for_action(
            MirClass::Archer,
            MirGender::Female,
            ActorAction::Attacking,
            Some(AttackVariant::Attack1),
            200,
            false,
        )
        .expect("C 局");
        assert!(!melee.alt_anim);
        assert_eq!(melee.library_id().armour_offset(), 808);
        assert_eq!(
            melee
                .library_id()
                .library_key(NativeLayer::Armour, 0)
                .as_deref(),
            Some("carmour-00")
        );
        let standing = PlayerLayout::for_action(
            MirClass::Archer,
            MirGender::Female,
            ActorAction::Standing,
            None,
            200,
            false,
        )
        .expect("C 局");
        assert!(!standing.alt_anim);
        assert_eq!(standing.library_id().armour_offset(), 808);

        // 骑马不解析原生库，交给旧扁平路径。
        assert!(
            PlayerLayout::for_action(
                MirClass::Warrior,
                MirGender::Male,
                ActorAction::Standing,
                None,
                0,
                true
            )
            .is_none()
        );
    }

    #[test]
    fn alt_anim_changes_body_domain_not_offset() {
        let a = PlayerLayout::new(MirClass::Assassin, MirGender::Female, false, 100);
        let a_alt = PlayerLayout::new(MirClass::Assassin, MirGender::Female, true, 100);
        assert_eq!(a.library_id().armour_offset(), 808);
        assert_eq!(a_alt.library_id().armour_offset(), 512);
        assert_eq!(
            a.library_id()
                .library_key(NativeLayer::Armour, 0)
                .as_deref(),
            Some("carmour-00")
        );
        assert_eq!(
            a_alt
                .library_id()
                .library_key(NativeLayer::Armour, 0)
                .as_deref(),
            Some("aarmour-00")
        );

        let ar = PlayerLayout::new(MirClass::Archer, MirGender::Female, false, 200);
        let ar_alt = PlayerLayout::new(MirClass::Archer, MirGender::Female, true, 200);
        assert_eq!(ar.library_id().armour_offset(), 808);
        assert_eq!(ar_alt.library_id().armour_offset(), 352);
        assert_eq!(
            ar.library_id()
                .library_key(NativeLayer::Armour, 0)
                .as_deref(),
            Some("carmour-00")
        );
        assert_eq!(
            ar_alt
                .library_id()
                .library_key(NativeLayer::Armour, 0)
                .as_deref(),
            Some("ararmour-00")
        );
    }

    #[test]
    fn draw_order_follows_crystal_weapon_and_head_swaps() {
        use MirDirection::{Down, DownLeft, DownRight, Left, Right, Up, UpLeft, UpRight};
        let main = NativeDrawLayer::Weapon(NativeHand::Main);
        let off = NativeDrawLayer::Weapon(NativeHand::Off);
        let body = NativeDrawLayer::Body;
        let head = NativeDrawLayer::Head;
        let wing = NativeDrawLayer::Wing;
        let cases = [
            (Left, [main, body, head, wing, off]),
            (Up, [main, body, head, wing, off]),
            (UpLeft, [main, body, head, wing, off]),
            (DownLeft, [main, body, wing, head, off]),
            (UpRight, [off, body, head, wing, main]),
            (Right, [off, body, head, wing, main]),
            (DownRight, [off, body, wing, head, main]),
            (Down, [off, body, wing, head, main]),
        ];
        for (direction, expected) in cases {
            let (actual, len) = draw_order(direction, true, true, false);
            assert_eq!(&actual[..len], &expected, "{direction:?}");
        }
        // 弓手 AR 在 Off（WeaponLibrary2），Main 为空；Crystal 在所有方向
        // 末尾再画一次 Weapon2，因此左组在身体后出现两次 Off，右组前/后各一次。
        let left_expected = [body, head, wing, off, off];
        let left_expected_low = [body, wing, head, off, off];
        let right_expected = [off, body, head, wing, off];
        let right_expected_low = [off, body, wing, head, off];
        for (direction, expected) in [
            (Left, &left_expected[..]),
            (Up, &left_expected[..]),
            (UpLeft, &left_expected[..]),
            (DownLeft, &left_expected_low[..]),
            (UpRight, &right_expected[..]),
            (Right, &right_expected[..]),
            (DownRight, &right_expected_low[..]),
            (Down, &right_expected_low[..]),
        ] {
            let (actual, len) = draw_order(direction, false, true, true);
            assert_eq!(&actual[..len], expected, "archer {direction:?}");
            assert!(
                !actual[..len].contains(&main),
                "弓手 Main 必须为空，不能画 WeaponLibrary1"
            );
        }
    }

    #[test]
    fn archer_layout_and_draw_order_are_consistent_for_all_directions() {
        use MirDirection::{Down, DownLeft, DownRight, Left, Right, Up, UpLeft, UpRight};
        let directions = [Up, UpRight, Right, DownRight, Down, DownLeft, Left, UpLeft];
        for gender in [MirGender::Male, MirGender::Female] {
            // 普通攻击：alt=false，C 身体/头发，AR 武器（非 -s），Off 手。
            let attack = PlayerLayout::for_action(
                MirClass::Archer,
                gender,
                ActorAction::Attacking,
                Some(AttackVariant::Attack1),
                200,
                false,
            )
            .expect("archer attack layout");
            assert!(!attack.alt_anim);
            assert_eq!(
                attack
                    .library_id()
                    .library_key(NativeLayer::Armour, 0)
                    .as_deref(),
                Some("carmour-00")
            );
            assert_eq!(
                attack
                    .library_id()
                    .library_key(NativeLayer::Weapon(NativeHand::Main), 200),
                None
            );
            assert_eq!(
                attack
                    .library_id()
                    .library_key(NativeLayer::Weapon(NativeHand::Off), 200)
                    .as_deref(),
                Some("arweapon-00")
            );
            for direction in directions {
                let (actual, len) = draw_order(direction, false, true, true);
                let off = NativeDrawLayer::Weapon(NativeHand::Off);
                assert_eq!(
                    actual[..len].iter().filter(|layer| **layer == off).count(),
                    2,
                    "Crystal 每个方向都额外 DrawWeapon2：{direction:?}"
                );
                let left_group = matches!(direction, Left | Up | UpLeft | DownLeft);
                let body_pos = actual[..len]
                    .iter()
                    .position(|layer| *layer == NativeDrawLayer::Body)
                    .expect("body layer");
                assert_eq!(
                    body_pos,
                    usize::from(!left_group),
                    "body 前只能有右组的 Weapon2：{direction:?}"
                );
                let off_positions: Vec<_> = actual[..len]
                    .iter()
                    .enumerate()
                    .filter(|(_, layer)| **layer == off)
                    .map(|(index, _)| index)
                    .collect();
                if left_group {
                    assert!(
                        off_positions.iter().all(|index| *index > body_pos),
                        "左组弓层必须在 body 之后：{direction:?} {off_positions:?}"
                    );
                } else {
                    assert_eq!(off_positions[0], body_pos - 1);
                    assert!(off_positions[1] > body_pos);
                }
            }

            // 持弓行走：alt=true，AR 身体/头发，ARWeaponsS，Off 手。
            let walking = PlayerLayout::for_action(
                MirClass::Archer,
                gender,
                ActorAction::Walking,
                None,
                200,
                false,
            )
            .expect("archer walking layout");
            assert!(walking.alt_anim);
            assert_eq!(
                walking
                    .library_id()
                    .library_key(NativeLayer::Armour, 0)
                    .as_deref(),
                Some("ararmour-00")
            );
            assert_eq!(
                walking
                    .library_id()
                    .library_key(NativeLayer::Weapon(NativeHand::Off), 200)
                    .as_deref(),
                Some("arweapon-00-s")
            );
            for direction in directions {
                let (actual, len) = draw_order(direction, false, true, true);
                assert_eq!(
                    actual[..len]
                        .iter()
                        .filter(|layer| **layer == NativeDrawLayer::Weapon(NativeHand::Off))
                        .count(),
                    2,
                    "WalkingBow {direction:?}"
                );
            }
        }
    }

    #[test]
    fn wing_effect_table_uses_independent_effect_columns() {
        // Standing 的 effect 段 stride=Count+Skip=8（不是 body 的 4 帧）。
        let standing = NativeEffectTable::get(NativeAction::Standing);
        assert_eq!(standing, FrameSequence::new(0, 8, 0, 250));
        assert_eq!(standing.index_of(MirDirection::Down, 7), 39);
        let attack1 = NativeEffectTable::get(NativeAction::Attack1);
        assert_eq!(attack1, FrameSequence::new(168, 6, 0, 100));
        // Down=4，effectOffset=6，frame=5 => 168+24+5=197；再加 WingOffset 840。
        let wing = NativeLibraryId::new(MirClass::Warrior, MirGender::Female, false);
        assert_eq!(
            u32::from(wing.wing_offset()) + u32::from(attack1.index_of(MirDirection::Down, 5)),
            1037
        );
    }
}
