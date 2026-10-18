//! 实体（玩家、怪物、NPC、地面物品）的软件绘制。
//!
//! 地图本身由 [`crate::MapScene`] 画完后，本模块在同一个[`Frame`]上追加实体精灵。
//! 所有精灵都从 `resource/data` 的平面 `.wzl` 图库里按需解码，并按水晶（Crystal）
//! 客户端同样的等距锚点定位；素材缺失时画一个彩色占位方块，保证无素材的 CI 也能
//! 得到非空画面。
//!
//! # 精灵库映射
//!
//! Crystal 的客户端按对象的 `Race` 选择图库（`Client/MirObjects/PlayerObject.cs:553-561`、
//! `MonsterObject.cs:208`、`NPCObject.cs:77-80`、`ItemObject.cs:32,49`）。本项目的素材
//! 语料是**扁平**的 `resource/data/*.wzl`（平铺的多外观库），因此这里用文件名直接映射：
//!
//! | 实体 | 图库文件 | 图像号规则 | Crystal 出处 |
//! |---|---|---|---|
//! | 玩家身体 | `hum.wzl` / `hum2.wzl` / `hum3.wzl` | 块号 `armour`；站立帧 `block*600 + direction*8 + n` | 外观选择 `PlayerObject.cs:553`；帧号 `PlayerObject.cs:761`；站立帧集合 `Frames.cs:157` |
//! | 玩家武器 | `weapon.wzl` / `weapon2.wzl` / `weapon3.wzl` | 块号 `weapon`；帧号与身体**同步** | `PlayerObject.cs:415-431`、`:5062` |
//! | 玩家头发 | `hair.wzl` / `hair2..4.wzl` | 块号 `hair`；帧号与身体**同步** | `PlayerObject.cs:554`、`:5054` |
//! | 玩家盾牌 | `shield.wzl`（语料里没有） | 同上 | Crystal 没有盾牌层，见下文 |
//! | 怪物 | `mon{image/10+1}.wzl` | `image` 每 10 个一组对应一个 `Mon1..MonN.wzl`；每个图像占 430 帧 | 图像 → 图库 `MonsterObject.cs:208`；默认帧集合 `Frames.cs:25-31` |
//! | NPC | `npc.wzl`、`npc2.wzl`… | `image` 每 100 个换一个库；每个图像占 60 帧 | `NPCObject.cs:77-80`；默认 NPC 帧集合 `Frames.cs:18-19` |
//! | 物品 | `items.wzl`（缺省回退 `dnitems.wzl`） | 图像号就是物品的 `Image` | `ItemObject.cs:32,37,49` |
//!
//! 关于 430/600/60 这几个步长：Crystal 的 `.Lib` 是「一个外观一个库」，帧起点由
//! `Frame.Start` 给出（`Frames.cs:157-171`）；扁平 `.wzl` 把多个外观塞进一个文件，
//! 所以需要步长才能定位到第 N 个外观。这些步长来自本素材语料（盛大 Mir2 扁平包）的
//! 实际布局：`hum.wzx` 有 14400 帧 = 24×600，`npc.wzx` 有 4900 帧 = 81×60 + 40。
//! 怪物每个图像 430 帧是经典 Mir2 的布局，但**并非所有库都恰好这么多**（`mon1.wzx`
//! 只有 1086 帧）；因此绘制时按「精确帧 → 该图像的站立帧 → 库首帧」逐级回退，
//! 实在没有素材就画占位方块。`!Mon.dat` 里存有每只怪的精确动作帧表，本模块尚未解析。
//!
//! # 玩家装备分层（纸娃娃）
//!
//! Crystal 的玩家对象把外观拆成四层，`PlayerObject.Draw` 按方向决定武器画在身体
//! **前**还是**后**（`PlayerObject.cs:4877-4926`）：
//!
//! ```text
//! DrawMount();                                        // 本作未做坐骑
//! if (dir ∈ {Left, Up, UpLeft, DownLeft}) DrawWeapon(); else DrawWeapon2();
//! DrawBody();                                         // PlayerObject.cs:4894
//! DrawHead();                                         // 头发，PlayerObject.cs:4896-4911
//! if (dir ∈ {UpRight, Right, DownRight, Down}) DrawWeapon(); else DrawWeapon2();
//!                                                     // PlayerObject.cs:4914-4923
//! ```
//!
//! 也就是说朝左上/上/左下/左时武器在身体**后面**，朝右上/右/右下/下时武器在身体
//! **前面**；`DrawWeapon2` 是另一只手的武器（刺客双持），普通职业的
//! `WeaponLibrary2` 恒为 `null`（`PlayerObject.cs:413-431`），所以本作不同时画两把。
//! 本模块用 [`PlayerLayer`] + [`player_layer_plan`] 把这套顺序表达成数据，绘制与
//! 测试共用同一份顺序表。
//!
//! 三层的帧号与身体**完全相同**：`block*600 + direction*8 + n`（`n = 0..=3` 站立帧）。
//! Crystal 里武器/头发也是 `DrawFrame + WeaponOffSet / HairOffSet`（`PlayerObject.cs:5054`、
//! `:5062`），`DrawFrame` 就是身体的帧号（`:761`）。男/女偏移（`ArmourOffSet = 808`、
//! `WeaponOffSet = 416`、`HairOffSet = 808`，`PlayerObject.cs:584-587`）只存在于
//! Crystal 的 `.Lib` 布局里；扁平语料把男/女外观编成**不同的块号**，因此这里不加偏移。
//!
//! 头发的颜色：Crystal **没有**单独的头发调色，`DrawHead` 用的是与身体/武器同一个
//! `DrawColour`（`PlayerObject.cs:5042,5054`），而 `DrawColour` 只随中毒状态变化
//! （`:865-883`，橙色=延迟爆炸、灰色=麻痹、蓝色=冰冻……）。所以本模块也不给头发染色。
//! 仓库里也没有 `Settings.DrawBlack` 之类的开关。
//!
//! 盾牌：Crystal 的玩家对象**没有**盾牌层（全仓库没有 `DrawShield`），本作保留
//! [`EntityView::shield`] 字段并在 `shield.wzl` 存在时画一层，位于头发之后、前向武器
//! 之前；本素材语料没有 `shield.wzl`，所以这一层实际总是退化成「跳过」。
//!
//! # 锚点
//!
//! * 玩家/怪物/NPC：与 Crystal 一样以**格原点**为锚点，再叠加图像头里的
//!   `offset_x/offset_y`（`MLibrary.Draw(..., offSet: true)`，`MLibrary.cs:658-665`；
//!   角色各层 `BodyLibrary.Draw(..., DrawLocation, ..., true)`，`PlayerObject.cs:5045`）
//!   ——四层共用同一个格原点与各自的图像偏移，所以不需要额外的层间偏移。
//! * 物品：Crystal 把物品在格子里居中（`ItemObject.cs:78-79`），不叠加图像偏移
//!   （`ItemObject.cs:72-73` 的 `Draw` 使用默认 `offSet: false`）。
//!
//! 实体按地图 Y 升序、同一行按 X 升序绘制（与 `GameScene.cs:10798-11008` 的对象
//! 循环顺序一致），靠下的对象覆盖靠上的对象。
//!
//! # 与 Crystal 的差异
//!
//! * 不画坐骑、翅膀、光效、中毒/等级特效（`EntityView` 没有这些字段）。
//! * 不画名字文字；`name` 字段保留给调试叠层/以后加字体时使用。
//! * 不做移动插值（Crystal 的 `OffSetMove`/`Movement` 平滑位移，`PlayerObject.cs:765-810`）：
//!   实体直接站在服务器给的格子上，动作帧仍然逐帧播放。
//! * 刺客双持（`WeaponLibrary2`）与弓箭手的第二遍武器（`PlayerObject.cs:4921-4922`）
//!   没有实现：只画一把武器。
//!
//! # 动作动画
//!
//! [`EntityView::action`] + [`EntityView::action_started_ms`] 描述「现在在做什么、
//! 什么时候开始的」；帧表与 `frame_at` 在 [`crate::animation`] 里。玩家身体、武器、
//! 头发、盾牌共用同一个动作帧号（Crystal `DrawFrame` 同时用于 `BodyLibrary`/
//! `WeaponLibrary`/`HairLibrary`，`PlayerObject.cs:761,5054,5062`），怪物/NPC 用各自的
//! 图库帧表。没有动作字段的旧调用（[`EntityRenderer::render`]）按动画计数当作
//! 100 ms 节拍推进时间，行为与之前的站立帧播放兼容。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mir_asset::{Image, ImageLibrary};
use mir_core::Point;
use mir_core::enums::MirDirection;

use crate::Frame;
use crate::animation::{ActorAction, ActorAnimation, FrameChoice, HUMAN_STANDING, NPC_STANDING};
use crate::geometry::{self, CELL_HEIGHT, CELL_WIDTH, Viewport};
use crate::scene::remove_black;

/// 每个玩家外观（身体/武器/头发）在扁平 `.wzl` 里占的帧数。
///
/// 语料实测：`hum.wzx` 14400 帧 = 24×600，`hum2.wzx` 18000 = 30×600，
/// `weapon.wzx` 45600 = 76×600，`hair.wzx` 3600 = 6×600。
pub const PLAYER_FRAMES_PER_APPEARANCE: u32 = 600;
/// 玩家每个方向占的帧数：4 帧站立 + 4 帧空位（本素材语料的布局）。
pub const PLAYER_DIRECTION_STRIDE: u32 = 8;
/// 站立动画每方向的有效帧数（`Frames.cs:157` 的 `Frame(0, 4, 0, 500)`）。
pub const PLAYER_STANDING_FRAMES: u32 = 4;
/// 怪物每个图像占的帧数（经典 Mir2 布局）。
pub const MONSTER_FRAMES_PER_IMAGE: u32 = 430;
/// 怪物每个方向占的帧数：4 帧站立 + 4 帧空位。
pub const MONSTER_DIRECTION_STRIDE: u32 = 8;
/// NPC 每个图像占的帧数。
pub const NPC_FRAMES_PER_IMAGE: u32 = 60;
/// 「这一层没有」的哨兵值（空手、没有盾牌、没有头发）。
pub const NO_LAYER: u16 = u16::MAX;
/// `hum*.wzl` 每个库里的外观块数（实测：`hum` 24、`hum2` 30、`hum3` 46）。
pub const HUM_BLOCKS: [u16; 3] = [24, 30, 46];
/// `hair*.wzl` 每个库里的发型块数（实测：`hair` 6、`hair2` 22、`hair3` 5、`hair4` 152）。
pub const HAIR_BLOCKS: [u16; 4] = [6, 22, 5, 152];
/// 实体解码缓存上限（张）。一张 64×80 的 RGBA 图约 20 KiB。
///
/// 一个玩家最多占 4 层（身体/武器/头发/盾牌），所以上限比单层时翻了倍。
pub const MAX_ENTITY_CACHED_IMAGES: usize = 1024;

/// 实体类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EntityKind {
    /// 玩家（自己或其他人）。
    Player,
    /// 怪物。
    Monster,
    /// NPC。
    Npc,
    /// 地面物品（含金币）。
    Item,
}

impl EntityKind {
    /// 占位方块的颜色（正常状态）。
    #[must_use]
    pub const fn color(self) -> [u8; 4] {
        match self {
            Self::Player => [96, 148, 255, 255],
            Self::Monster => [224, 72, 72, 255],
            Self::Npc => [96, 208, 112, 255],
            Self::Item => [240, 208, 72, 255],
        }
    }

    /// 占位方块的颜色（死亡状态）。
    #[must_use]
    pub const fn dead_color(self) -> [u8; 4] {
        match self {
            Self::Player | Self::Monster => [110, 110, 110, 255],
            Self::Npc | Self::Item => self.color(),
        }
    }
}

/// 画一个实体所需的全部信息。
///
/// 这是 `mir-render` 与客户端网络快照之间的唯一接口：网络层把 `ServerPacket`
/// 转成这个结构，渲染层不依赖 `mir-protocol`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityView {
    /// 地图坐标。
    pub position: Point,
    /// 朝向（0=上，顺时针）。
    pub direction: MirDirection,
    /// 实体类型。
    pub kind: EntityKind,
    /// 图像号：玩家与 `armour` 同义（保留给怪物/NPC/物品），怪物是 `Monster` 枚举值，
    /// NPC 是 `Image`，物品是物品的 `Image`。
    pub image: u16,
    /// 衣服外观号（装备槽 `Armour` 的 `ItemInfo.Shape`）→ `hum*.wzl` 的块号。
    ///
    /// `0` 是「没穿衣服」的裸体外观（语料 `hum.wzl` 块 0），也是 Crystal 的缺省值
    /// （`Client/MirObjects/UserObject.cs:209`）。非玩家实体忽略这个字段。
    pub armour: u16,
    /// 武器外观号（装备槽 `Weapon` 的 `ItemInfo.Shape % 100`）→ `weapon*.wzl` 的块号。
    ///
    /// [`NO_LAYER`] 表示空手，此时不画武器层。
    pub weapon: u16,
    /// 武器形状的原始值（`ItemInfo.Shape`，含 100/200 段位）。
    ///
    /// Crystal 用 `Weapon / Globals.ClassWeaponCount` 区分普通/刺客/弓箭手武器
    /// （`PlayerObject.cs:41-55`），本模块用 `weapon_shape / 100` 选
    /// `weapon.wzl` / `weapon2.wzl` / `weapon3.wzl`。
    pub weapon_shape: u16,
    /// 发型号（`S.UserInformation.hair` / `S.ObjectPlayer.hair`）→ `hair*.wzl` 的块号。
    ///
    /// [`NO_LAYER`] 表示光头，不画头发层。
    pub hair: u16,
    /// 盾牌外观号 → `shield.wzl` 的块号；[`NO_LAYER`] 表示没有盾牌。
    ///
    /// Crystal 的玩家没有盾牌层，本作保留这一层以便以后接自定义素材。
    pub shield: u16,
    /// 名字（本里程碑不画文字，保留给调试与后续 UI）。
    pub name: Option<String>,
    /// 是否死亡。
    pub dead: bool,
    /// 生命百分比（0..=100）；`None` 表示不画血条。
    pub hp_percent: Option<u8>,
    /// 当前动作；默认 [`ActorAction::Standing`]（站立 4 帧循环）。
    pub action: ActorAction,
    /// 当前动作的开始时间（Unix 毫秒）。默认 `0`：渲染时按动画计数换算时间。
    pub action_started_ms: u64,
}

impl EntityView {
    /// 一个只有最基本字段的实体（非玩家用）。
    #[must_use]
    pub fn new(position: Point, direction: MirDirection, kind: EntityKind, image: u16) -> Self {
        Self {
            position,
            direction,
            kind,
            image,
            armour: 0,
            weapon: NO_LAYER,
            weapon_shape: NO_LAYER,
            hair: NO_LAYER,
            shield: NO_LAYER,
            name: None,
            dead: false,
            hp_percent: None,
            action: ActorAction::Standing,
            action_started_ms: 0,
        }
    }

    /// 玩家外观层里是否画武器。
    #[must_use]
    pub fn has_weapon(&self) -> bool {
        self.weapon != NO_LAYER && self.weapon_shape != NO_LAYER
    }

    /// 这个实体当前动作的动画（含帧表、朝向与开始时间）。
    ///
    /// 死亡实体如果还没切到 `Dying/Dead`，一律按 [`ActorAction::Dead`] 画尸体帧，
    /// 对应 Crystal `Dead` 动作（`Frames.cs:170`）。
    #[must_use]
    pub fn animation(&self) -> ActorAnimation {
        ActorAnimation::from_view(self)
    }

    /// 触发一个新动作（把开始时间改成 `now_ms`）。
    pub fn set_action(&mut self, action: ActorAction, now_ms: u64) {
        self.action = action;
        self.action_started_ms = now_ms;
        if action == ActorAction::Dying {
            self.dead = true;
        }
        if matches!(
            action,
            ActorAction::Standing | ActorAction::Walking | ActorAction::Running
        ) {
            self.dead = false;
        }
    }
}

/// 玩家身体的图层。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PlayerLayer {
    /// 武器（`weapon*.wzl`）。
    Weapon,
    /// 身体（`hum*.wzl`）。
    Body,
    /// 头发（`hair*.wzl`）。
    Hair,
    /// 盾牌（`shield.wzl`；Crystal 没有这一层）。
    Shield,
}

impl PlayerLayer {
    /// 稳定的小写名字（测试/日志用）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Weapon => "weapon",
            Self::Body => "body",
            Self::Hair => "hair",
            Self::Shield => "shield",
        }
    }
}

/// 精灵图库标识：决定去哪个 `.wzl` 找图。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SpriteLibrary {
    /// `hum.wzl`（1）/ `hum2.wzl`（2）/ `hum3.wzl`（3）。
    Hum(u8),
    /// `weapon.wzl`（1）/ `weapon2.wzl`（2）/ `weapon3.wzl`（3）。
    Weapon(u8),
    /// `hair.wzl`（1）/ `hair2.wzl`（2）/ `hair3.wzl`（3）/ `hair4.wzl`（4）。
    Hair(u8),
    /// `shield.wzl`。
    Shield,
    /// `npc.wzl` / `npc2.wzl` / `npc3.wzl` / `npc4.wzl`。
    Npc(u8),
    /// `mon1.wzl` … `monN.wzl`。
    Monster(u16),
    /// `items.wzl`（找不到时回退 `dnitems.wzl`）。
    Item,
}

impl SpriteLibrary {
    /// 图库的主文件名（不含扩展名）。
    #[must_use]
    pub fn stem(self) -> String {
        match self {
            Self::Hum(1) => "hum".to_owned(),
            Self::Hum(index) => format!("hum{index}"),
            Self::Weapon(1) => "weapon".to_owned(),
            Self::Weapon(index) => format!("weapon{index}"),
            Self::Hair(1) => "hair".to_owned(),
            Self::Hair(index) => format!("hair{index}"),
            Self::Shield => "shield".to_owned(),
            Self::Npc(1) => "npc".to_owned(),
            Self::Npc(index) => format!("npc{index}"),
            Self::Monster(index) => format!("mon{index}"),
            Self::Item => "items".to_owned(),
        }
    }
}

/// 一层精灵的绘制计划。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayerFrame {
    /// 图层。
    pub layer: PlayerLayer,
    /// 去哪张图库找。
    pub library: SpriteLibrary,
    /// 精确帧号。
    pub frame: u32,
}

impl LayerFrame {
    /// 该外观块的首帧：精确帧缺失时回退到这里。
    #[must_use]
    pub fn block_start(self) -> u32 {
        self.frame / PLAYER_FRAMES_PER_APPEARANCE * PLAYER_FRAMES_PER_APPEARANCE
    }
}

/// 一个玩家的最多 4 层绘制计划（按绘制先后排列）。
///
/// 用定长数组而不是 `Vec`，避免每帧每个玩家都分配一次堆内存。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerLayerPlan {
    /// 定长缓冲。
    layers: [LayerFrame; 4],
    /// 有效长度。
    len: usize,
}

impl PlayerLayerPlan {
    /// 有效层（按绘制先后）。
    #[must_use]
    pub fn layers(&self) -> &[LayerFrame] {
        &self.layers[..self.len]
    }

    /// 层数。
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// 是否一层都没有（身体层总会入计划，所以实际不会为空）。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// 是否有某一层。
    #[must_use]
    pub fn contains(&self, layer: PlayerLayer) -> bool {
        self.layers().iter().any(|item| item.layer == layer)
    }

    /// 追加一层（内部用；超过 4 层时忽略）。
    fn push(&mut self, frame: LayerFrame) {
        if self.len < self.layers.len() {
            self.layers[self.len] = frame;
            self.len += 1;
        }
    }
}

/// 衣服外观号 → (`hum*.wzl` 库序号 1..=3, 库内块号)。
///
/// 语料实测块数：[`HUM_BLOCKS`]。超出语料范围的外观号落在最后一个库的最后一块
/// （宁可撞外观，也不越界取到别人的帧）。
#[must_use]
pub fn player_library(armour: u16) -> (u8, u16) {
    let mut remaining = armour;
    for (index, count) in HUM_BLOCKS.iter().enumerate() {
        if remaining < *count {
            return (u8::try_from(index + 1).unwrap_or(1), remaining);
        }
        remaining -= count;
    }
    let last = HUM_BLOCKS.len();
    (
        u8::try_from(last).unwrap_or(3),
        HUM_BLOCKS[last - 1].saturating_sub(1),
    )
}

/// 发型号 → (`hair*.wzl` 库序号 1..=4, 库内块号)。
///
/// 语料实测块数：[`HAIR_BLOCKS`]。
#[must_use]
pub fn hair_library(hair: u16) -> (u8, u16) {
    let mut remaining = hair;
    for (index, count) in HAIR_BLOCKS.iter().enumerate() {
        if remaining < *count {
            return (u8::try_from(index + 1).unwrap_or(1), remaining);
        }
        remaining -= count;
    }
    let last = HAIR_BLOCKS.len();
    (
        u8::try_from(last).unwrap_or(4),
        HAIR_BLOCKS[last - 1].saturating_sub(1),
    )
}

/// 武器形状 → (`weapon*.wzl` 库序号 1..=3, 库内块号)。
///
/// Crystal 把 0..99 的 `Shape` 放 `CWeapons`、100..199 放 `AWeaponsL/R`（刺客）、
/// 200.. 放 `ARWeapons`（弓箭手），块号是 `Shape - 100` / `Shape - 200`
/// （`PlayerObject.cs:402-409,497-502`、`MirGameShopCell.cs:505-547`）。
/// 扁平语料把这三段分别放在 `weapon.wzl`、`weapon2.wzl`、`weapon3.wzl`，
/// 所以库序号 = `shape / 100 + 1`，块号 = `shape % 100`。
#[must_use]
pub fn weapon_library(weapon_shape: u16) -> (u8, u16) {
    let series = (weapon_shape / 100).min(2);
    (u8::try_from(series + 1).unwrap_or(1), weapon_shape % 100)
}

/// 某个方向是否把武器画在身体**前面**。
///
/// Crystal：`Direction == UpRight || Right || DownRight || Down` 时身体之后画武器
/// （`PlayerObject.cs:4916`）；其余四个方向（Left/Up/UpLeft/DownLeft）在身体之前画
/// （`:4888`）。
#[must_use]
pub const fn weapon_drawn_in_front(direction: MirDirection) -> bool {
    matches!(
        direction,
        MirDirection::UpRight | MirDirection::Right | MirDirection::DownRight | MirDirection::Down
    )
}

/// 一个玩家的层绘制计划（顺序即绘制先后）。
///
/// 顺序表与 Crystal `PlayerObject.Draw`（`:4877-4926`）一致：
///
/// * 朝 Left/Up/UpLeft/DownLeft：武器 → 身体 → 头发 → 盾牌；
/// * 朝 UpRight/Right/DownRight/Down：身体 → 头发 → 盾牌 → 武器。
///
/// 空图层（`NO_LAYER`）不会出现在计划里。四层共用同一个动作帧号：
/// Crystal 的 `DrawFrame` 同时用于身体/武器/头发（`PlayerObject.cs:761,5054,5062`）。
///
/// 这个重载保留旧的「动画计数」接口：`animation` 是 100 ms 节拍的序号，只用于
/// 站立帧（`animation % 4`）。新代码应使用 [`player_layer_plan_at`]，它按真实
/// 时间与 [`ActorAction`] 取帧。
#[must_use]
pub fn player_layer_plan(entity: &EntityView, animation: u32) -> PlayerLayerPlan {
    let mut copy = entity.clone();
    copy.action = if copy.dead {
        ActorAction::Dead
    } else {
        ActorAction::Standing
    };
    copy.action_started_ms = 0;
    // 站立间隔 500 ms；用 `animation` 个 100 ms 节拍推进，局部帧正好是 `animation % 4`。
    let now_ms = u64::from(animation % PLAYER_STANDING_FRAMES) * HUMAN_STANDING.interval_ms;
    player_layer_plan_with_choice(&copy, choice_for(&copy, now_ms))
}

/// 按真实时间（Unix 毫秒）生成一个玩家的层绘制计划。
///
/// 身体/武器/头发/盾牌都使用同一个动作帧偏移（见 [`player_layer_plan`]）。
#[must_use]
pub fn player_layer_plan_at(entity: &EntityView, now_ms: u64) -> PlayerLayerPlan {
    player_layer_plan_with_choice(entity, choice_for(entity, now_ms))
}

/// 实体当前应画的动作帧。
fn choice_for(entity: &EntityView, now_ms: u64) -> FrameChoice {
    entity.animation().frame_at(now_ms)
}

/// 用已经算好的动作帧号拼层计划。
fn player_layer_plan_with_choice(entity: &EntityView, choice: FrameChoice) -> PlayerLayerPlan {
    let mut plan = PlayerLayerPlan {
        layers: [LayerFrame {
            layer: PlayerLayer::Body,
            library: SpriteLibrary::Hum(1),
            frame: 0,
        }; 4],
        len: 0,
    };
    let direction = entity.direction;
    let action_frame = u32::from(choice.index);

    let (body_library, body_block) = player_library(entity.armour);
    let body = LayerFrame {
        layer: PlayerLayer::Body,
        library: SpriteLibrary::Hum(body_library),
        frame: u32::from(body_block) * PLAYER_FRAMES_PER_APPEARANCE + action_frame,
    };
    let weapon = entity.has_weapon().then(|| {
        let (library, block) = weapon_library(entity.weapon_shape);
        LayerFrame {
            layer: PlayerLayer::Weapon,
            library: SpriteLibrary::Weapon(library),
            frame: u32::from(block) * PLAYER_FRAMES_PER_APPEARANCE + action_frame,
        }
    });
    let hair = (entity.hair != NO_LAYER).then(|| {
        let (library, block) = hair_library(entity.hair);
        LayerFrame {
            layer: PlayerLayer::Hair,
            library: SpriteLibrary::Hair(library),
            frame: u32::from(block) * PLAYER_FRAMES_PER_APPEARANCE + action_frame,
        }
    });
    let shield = (entity.shield != NO_LAYER).then(|| LayerFrame {
        layer: PlayerLayer::Shield,
        library: SpriteLibrary::Shield,
        frame: u32::from(entity.shield) * PLAYER_FRAMES_PER_APPEARANCE + action_frame,
    });

    if weapon_drawn_in_front(direction) {
        plan.push(body);
        if let Some(hair) = hair {
            plan.push(hair);
        }
        if let Some(shield) = shield {
            plan.push(shield);
        }
        if let Some(weapon) = weapon {
            plan.push(weapon);
        }
    } else {
        if let Some(weapon) = weapon {
            plan.push(weapon);
        }
        plan.push(body);
        if let Some(hair) = hair {
            plan.push(hair);
        }
        if let Some(shield) = shield {
            plan.push(shield);
        }
    }
    plan
}

/// 实体图库集合：按需打开 `resource/data` 里的 `.wzl`，只缓存打开过的库。
///
/// 与 [`crate::TileLibraries`] 不同，这里不预先打开任何文件：`resource/data` 里有
/// 800 多个 `.wzl`，全部打开会浪费句柄；实体只用得到 `hum*`、`weapon*`、`hair*`、
/// `npc*`、`mon*`、`items`。
#[derive(Debug, Default)]
pub struct EntityLibraries {
    /// 素材目录（诊断用）。
    directory: PathBuf,
    /// 小写文件名（不含扩展名）→ 路径。
    files: BTreeMap<String, PathBuf>,
    /// 已尝试打开的图库；`None` 表示文件不存在或打开失败。
    opened: BTreeMap<String, Option<ImageLibrary>>,
}

impl EntityLibraries {
    /// 扫描目录，建立文件名索引（不打开任何图像库）。
    ///
    /// 目录不存在时返回空集合（地图渲染仍然可用，实体退化成占位方块）。
    #[must_use]
    pub fn open_dir(directory: impl AsRef<Path>) -> Self {
        let directory = directory.as_ref().to_path_buf();
        let mut files = BTreeMap::new();
        if let Ok(entries) = std::fs::read_dir(&directory) {
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(extension) = path.extension() else {
                    continue;
                };
                let extension = extension.to_string_lossy().to_ascii_lowercase();
                if extension != "wzl" && extension != "lib" {
                    continue;
                }
                let Some(stem) = path.file_stem() else {
                    continue;
                };
                files.insert(stem.to_string_lossy().to_ascii_lowercase(), path);
            }
        }
        Self {
            directory,
            files,
            opened: BTreeMap::new(),
        }
    }

    /// 空图库集合（测试与无素材环境）。
    #[must_use]
    pub fn empty(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
            files: BTreeMap::new(),
            opened: BTreeMap::new(),
        }
    }

    /// 扫描时使用的素材目录。
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// 目录里找到了多少个可用的图像库文件（不要求已打开）。
    #[must_use]
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// 是否找到了某个图库文件（不要求已打开）。
    #[must_use]
    pub fn has(&self, library: SpriteLibrary) -> bool {
        self.files.contains_key(&library.stem())
    }

    /// 已尝试打开（成功或失败）的图库数量。
    #[must_use]
    pub fn opened_count(&self) -> usize {
        self.opened.len()
    }

    /// 取一个图库；文件不存在或打开失败返回 `None`。
    fn library(&mut self, stem: &str) -> Option<&ImageLibrary> {
        if !self.opened.contains_key(stem) {
            // 物品图库 `items` 缺失时回退到 `dnitems`：两者都是 Crystal
            // `Libraries.FloorItems` 在本项目扁平语料里的对应物，帧号一致。
            let fallback = if stem == "items" {
                self.files.get("dnitems")
            } else {
                None
            };
            let opened =
                self.files
                    .get(stem)
                    .or(fallback)
                    .and_then(|path| match ImageLibrary::open(path) {
                        Ok(library) => Some(library),
                        Err(error) => {
                            eprintln!("警告：实体图库 {} 打开失败：{error}", path.display());
                            None
                        }
                    });
            self.opened.insert(stem.to_owned(), opened);
        }
        self.opened.get(stem).and_then(Option::as_ref)
    }

    /// 某一帧是否存在：只读记录头，不解码像素。
    fn has_frame(&mut self, library: SpriteLibrary, index: u32) -> bool {
        let Ok(index) = usize::try_from(index) else {
            return false;
        };
        self.library(&library.stem())
            .is_some_and(|library| library.info(index).is_ok_and(|info| !info.is_empty()))
    }

    /// 解码单张图。
    fn image(&mut self, library: SpriteLibrary, index: u32) -> Option<Image> {
        let index = usize::try_from(index).ok()?;
        self.library(&library.stem())?
            .image(index)
            .ok()
            .filter(|image| !image.is_empty())
    }
}

/// 实体绘制器：图库 + 解码缓存。
#[derive(Debug, Default)]
pub struct EntityRenderer {
    /// 实体图库。
    libraries: EntityLibraries,
    /// `(图库, 帧号) → 解码结果`；`None` 表示该帧不存在或解码失败。
    cache: BTreeMap<(SpriteLibrary, u32), Option<Image>>,
    /// `(图库, 外观块首帧) → 站立段的实际起点偏移`（`0` 或 [`PLAYER_DIRECTION_STRIDE`]）。
    ///
    /// `weapon.wzl` 里有 25/76 个武器外观块的站立段整体**后移 8 帧**（块首 8 帧是空的，
    /// 见 `tests/player_layers.rs` 与 `docs/client.md` §9.14）。身体/头发库全部对齐，
    /// 所以这个探测对它们总是得到 0。
    phase: BTreeMap<(SpriteLibrary, u32), u32>,
}

impl EntityRenderer {
    /// 用已有图库构造绘制器。
    #[must_use]
    pub fn new(libraries: EntityLibraries) -> Self {
        Self {
            libraries,
            cache: BTreeMap::new(),
            phase: BTreeMap::new(),
        }
    }

    /// 空绘制器（所有实体都画占位方块）。
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// 当前图库集合。
    #[must_use]
    pub fn libraries(&self) -> &EntityLibraries {
        &self.libraries
    }

    /// 当前图库集合（可变）。
    pub fn libraries_mut(&mut self) -> &mut EntityLibraries {
        &mut self.libraries
    }

    /// 已成功解码并缓存的图像张数（缺图/解码失败的占位不计）。
    #[must_use]
    pub fn cache_len(&self) -> usize {
        self.cache.values().filter(|image| image.is_some()).count()
    }

    /// 在已经画好地形的帧上追加实体。
    ///
    /// `animation_frame` 对应 Crystal 的 `MapControl.AnimationCount`
    /// （`GameScene.cs:1188-1193`），按 100 ms 一个节拍换算成时间交给
    /// [`Self::render_at`]；`0` 得到确定性的静态帧。
    pub fn render(
        &mut self,
        frame: &mut Frame,
        viewport: Viewport,
        center: Point,
        entities: &[EntityView],
        animation_frame: u32,
    ) {
        self.render_at(
            frame,
            viewport,
            center,
            entities,
            u64::from(animation_frame).saturating_mul(100),
        );
    }

    /// 按真实时间（Unix 毫秒）在已经画好地形的帧上追加实体。
    ///
    /// 每个实体用 [`ActorAnimation::frame_at`] 根据自己的动作/开始时间取帧；
    /// 动作缺失的帧先回退到该外观的站立帧，再回退到库首帧，最后画占位方块。
    pub fn render_at(
        &mut self,
        frame: &mut Frame,
        viewport: Viewport,
        center: Point,
        entities: &[EntityView],
        now_ms: u64,
    ) {
        // 按 Y 升序、同一行按 X 升序；`sort_by_key` 是稳定排序，同格实体保持输入顺序。
        let mut order: Vec<usize> = (0..entities.len()).collect();
        order.sort_by_key(|&index| (entities[index].position.y, entities[index].position.x));
        for index in order {
            let entity = &entities[index];
            let (origin_x, origin_y) = geometry::world_to_screen(viewport, center, entity.position);
            let top = self.draw_entity(frame, entity, origin_x, origin_y, now_ms);
            if let Some(percent) = entity.hp_percent {
                let bar_y = top.unwrap_or(origin_y) - 6;
                draw_health_bar(frame, origin_x, bar_y, percent, entity.dead);
            } else if entity.dead {
                draw_dead_marker(frame, origin_x, origin_y);
            }
        }
    }

    /// 画一个实体，返回精灵顶边的屏幕 Y（用于血条定位）。
    fn draw_entity(
        &mut self,
        frame: &mut Frame,
        entity: &EntityView,
        origin_x: i32,
        origin_y: i32,
        now_ms: u64,
    ) -> Option<i32> {
        if entity.kind == EntityKind::Player {
            return self.draw_player(frame, entity, origin_x, origin_y, now_ms);
        }
        let (library, exact, base, item) = sprite_target(entity, now_ms);
        // 动作帧缺失时先回退到该方向的站立帧，再回退图像首帧/库首帧。
        let standing = match entity.kind {
            EntityKind::Monster => {
                base + u32::from(direction_index(entity.direction)) * MONSTER_DIRECTION_STRIDE
            }
            EntityKind::Npc => {
                let (_, npc_base) = npc_library(entity.image);
                npc_base
                    + u32::from(direction_index(entity.direction))
                        * u32::from(NPC_STANDING.stride())
            }
            EntityKind::Player | EntityKind::Item => base,
        };
        let candidates = [exact, standing, base, 0];
        for index in candidates {
            if let Some(image) = self.image(library, index) {
                let (x, y) = if item {
                    (
                        origin_x + (CELL_WIDTH - i32::from(image.info.width)) / 2,
                        origin_y + (CELL_HEIGHT - i32::from(image.info.height)) / 2,
                    )
                } else {
                    (
                        origin_x + i32::from(image.info.offset_x),
                        origin_y + i32::from(image.info.offset_y),
                    )
                };
                frame.blit(&image, x, y);
                return Some(y);
            }
        }
        // 素材缺失：画一个小方块，至少让测试与无素材客户端看到"这里有个东西"。
        draw_placeholder(frame, origin_x, origin_y, entity);
        None
    }

    /// 画一个玩家：按 [`player_layer_plan_at`] 的顺序叠身体/头发/盾牌/武器。
    ///
    /// 每层都共用一个格原点与各自的图像偏移（Crystal `PlayerObject.cs:5045-5089`）；
    /// 某一层的精确帧缺失时先回退到该外观的站立帧，仍缺就跳过这一层。身体层是唯一
    /// 必需层：身体也缺失时退回占位方块。
    fn draw_player(
        &mut self,
        frame: &mut Frame,
        entity: &EntityView,
        origin_x: i32,
        origin_y: i32,
        now_ms: u64,
    ) -> Option<i32> {
        let plan = player_layer_plan_at(entity, now_ms);
        let mut top = None;
        let mut body_drawn = false;
        for item in plan.layers() {
            let offset = self.phase_offset(item.library, item.block_start());
            let standing = item.block_start()
                + u32::from(direction_index(entity.direction)) * PLAYER_DIRECTION_STRIDE;
            let candidates = [
                item.frame.saturating_add(offset),
                standing.saturating_add(offset),
                item.block_start().saturating_add(offset),
                0,
            ];
            let mut drawn = None;
            for index in candidates {
                if let Some(image) = self.image(item.library, index) {
                    let x = origin_x + i32::from(image.info.offset_x);
                    let y = origin_y + i32::from(image.info.offset_y);
                    frame.blit(&image, x, y);
                    drawn = Some(y);
                    break;
                }
            }
            if item.layer == PlayerLayer::Body {
                body_drawn = drawn.is_some();
                top = drawn;
            }
        }
        if !body_drawn {
            draw_placeholder(frame, origin_x, origin_y, entity);
        }
        top
    }

    /// 一个外观块的站立段起点偏移：`0`（对齐）或 [`PLAYER_DIRECTION_STRIDE`]（整体后移）。
    ///
    /// 判据：站立段第 0 个方向的前 4 帧里只要有一帧有图，就认为块是对齐的；否则
    /// （块首 8 帧全空、`块首+8` 有图）认为站立段整体后移了一个方向（8 帧）。
    /// 本素材语料里 `weapon.wzl` 有 25/76 个块是后一种，身体/头发库全部对齐。
    /// 结果按块缓存，所以每个外观块最多扫 8 个记录头。
    fn phase_offset(&mut self, library: SpriteLibrary, block_start: u32) -> u32 {
        let key = (library, block_start);
        if let Some(offset) = self.phase.get(&key) {
            return *offset;
        }
        let aligned = (0..PLAYER_STANDING_FRAMES)
            .any(|frame| self.libraries.has_frame(library, block_start + frame));
        let offset = if aligned { 0 } else { PLAYER_DIRECTION_STRIDE };
        self.phase.insert(key, offset);
        offset
    }

    /// 取一张物品图标（`items.wzl`，缺失时回退 `dnitems.wzl`）。
    ///
    /// 背包/商店面板用；与地面物品走同一条精灵路径，所以图标与地图上看到的一致。
    /// 返回的是缓存图像的克隆，调用方可以随意缩放/裁剪。
    #[must_use]
    pub fn item_icon(&mut self, image: u16) -> Option<Image> {
        self.image(SpriteLibrary::Item, u32::from(image))
    }

    /// 取一张实体图（带缓存与纯黑透明处理）。
    fn image(&mut self, library: SpriteLibrary, index: u32) -> Option<Image> {
        let key = (library, index);
        if let Some(cached) = self.cache.get(&key) {
            return cached.clone();
        }
        if self.cache.len() >= MAX_ENTITY_CACHED_IMAGES {
            self.cache.clear();
        }
        let image = self.libraries.image(library, index).map(|mut image| {
            remove_black(&mut image);
            image
        });
        self.cache.insert(key, image.clone());
        image
    }
}

/// 计算一个非玩家实体要用的图库、精确帧与「该图像首帧」。
///
/// `item` 为真时调用方按物品居中规则定位。精确帧来自实体的动作帧表
/// （[`ActorAnimation::frame_at`]）；怪物/NPC 的动作帧是相对图像首帧的偏移，
/// 所以要加上 `base`。
fn sprite_target(entity: &EntityView, now_ms: u64) -> (SpriteLibrary, u32, u32, bool) {
    match entity.kind {
        // 玩家走 `draw_player`，不会到这里；给一个不会命中的组合兜底。
        EntityKind::Player => (SpriteLibrary::Hum(1), 0, 0, false),
        EntityKind::Monster => {
            let library = monster_library(entity.image);
            let base = u32::from(entity.image % 10) * MONSTER_FRAMES_PER_IMAGE;
            let choice = entity.animation().frame_at(now_ms);
            let exact = base + u32::from(choice.index);
            (SpriteLibrary::Monster(library), exact, base, false)
        }
        EntityKind::Npc => {
            let (library, base) = npc_library(entity.image);
            let choice = entity.animation().frame_at(now_ms);
            let exact = base + u32::from(choice.index);
            (SpriteLibrary::Npc(library), exact, base, false)
        }
        EntityKind::Item => {
            let base = u32::from(entity.image);
            (SpriteLibrary::Item, base, base, true)
        }
    }
}

/// NPC 图像号 → 图库与首帧。
///
/// * NPC：`image < 100` 用 `npc.wzl`，`100..200` 用 `npc2.wzl`，依此类推
///   （`NPCObject.cs:77-80` 的 `Libraries.NPCs[Image]` 在本项目扁平语料里的对应）；
/// * 每个 NPC 占 60 帧（本素材语料布局），取站立首帧。
#[must_use]
pub fn npc_library(image: u16) -> (u8, u32) {
    let library = u8::try_from(image / 100 + 1).unwrap_or(u8::MAX).clamp(1, 4);
    let slot = u32::from(image % 100);
    (library, slot * NPC_FRAMES_PER_IMAGE)
}

/// 怪物图像号 → `monN.wzl` 编号。
///
/// 经典 Mir2 的怪物图像按 10 个一组分库：0..=9 → `Mon1`，10..=19 → `Mon2`……
/// 本项目的扁平语料沿用这个分组（见 `resource/data/mon1.wzl` … `mon53.wzl`）。
#[must_use]
pub const fn monster_library(image: u16) -> u16 {
    image / 10 + 1
}

/// 玩家（及武器/头发/盾牌层）的站立帧号：`block*600 + direction*8 + n`。
///
/// `block` 是**图库内**的外观块号（`player_library` / `weapon_library` /
/// `hair_library` 的第二个返回值），不是 `ItemInfo.Shape` 的原始值。
#[must_use]
pub fn player_standing_frame(block: u16, direction: MirDirection, animation: u32) -> u32 {
    u32::from(block) * PLAYER_FRAMES_PER_APPEARANCE
        + u32::from(direction_index(direction)) * PLAYER_DIRECTION_STRIDE
        + animation % PLAYER_STANDING_FRAMES
}

/// 怪物站立帧号（供测试与本模块文档使用）。
#[must_use]
pub fn monster_standing_frame(image: u16, direction: MirDirection, animation: u32) -> u32 {
    u32::from(image % 10) * MONSTER_FRAMES_PER_IMAGE
        + u32::from(direction_index(direction)) * MONSTER_DIRECTION_STRIDE
        + animation % (MONSTER_DIRECTION_STRIDE / 2)
}

/// `MirDirection` 的线上序号（0=Up）。
fn direction_index(direction: MirDirection) -> u8 {
    direction as u8
}

/// 画素材缺失时的彩色占位方块。
fn draw_placeholder(frame: &mut Frame, origin_x: i32, origin_y: i32, entity: &EntityView) {
    let color = if entity.dead {
        entity.kind.dead_color()
    } else {
        entity.kind.color()
    };
    let left = origin_x + (CELL_WIDTH - 12) / 2;
    let top = origin_y + (CELL_HEIGHT - 18) / 2;
    frame.fill_rect(left - 1, top - 1, 14, 20, [0, 0, 0, 255]);
    frame.fill_rect(left, top, 12, 18, color);
}

/// 画 3px 血条（黑底 + 绿/黄/红前景）。
fn draw_health_bar(frame: &mut Frame, origin_x: i32, y: i32, percent: u8, dead: bool) {
    let width: i32 = 18;
    let left = origin_x + (CELL_WIDTH - width) / 2;
    let bar_width = u32::try_from(width + 2).unwrap_or(0);
    frame.fill_rect(left - 1, y - 1, bar_width, 5, [0, 0, 0, 255]);
    let color = if dead {
        [90, 90, 90, 255]
    } else if percent > 50 {
        [72, 208, 96, 255]
    } else if percent > 25 {
        [240, 208, 72, 255]
    } else {
        [232, 72, 72, 255]
    };
    let filled = u32::from(percent.min(100)) * u32::try_from(width).unwrap_or(0) / 100;
    frame.fill_rect(left, y, filled, 3, color);
}

/// 死亡但无血量信息时的灰色小横条。
fn draw_dead_marker(frame: &mut Frame, origin_x: i32, origin_y: i32) {
    let left = origin_x + (CELL_WIDTH - 18) / 2;
    frame.fill_rect(
        left,
        origin_y + CELL_HEIGHT - 4,
        18,
        3,
        [110, 110, 110, 255],
    );
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::sync::atomic::{AtomicU32, Ordering};

    use flate2::Compression;
    use flate2::write::ZlibEncoder;

    use super::*;

    /// 自动清理的临时目录。
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let index = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "mir-render-entities-{}-{tag}-{index}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("创建临时目录");
            Self(path)
        }

        fn join(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// 合成素材里的一帧：尺寸、相对格原点的偏移、纯色。
    struct SyntheticFrame {
        /// 帧号。
        index: u32,
        /// 宽。
        width: u16,
        /// 高。
        height: u16,
        /// 绘制偏移 X。
        offset_x: i16,
        /// 绘制偏移 Y。
        offset_y: i16,
        /// 颜色（RGB24，alpha 恒 255）。
        color: [u8; 3],
    }

    fn zlib(data: &[u8]) -> Vec<u8> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data).expect("写 zlib");
        encoder.finish().expect("收尾 zlib")
    }

    /// 写一对合成 `.wzl`/`.wzx`：只填 `frames` 里列出的帧，其余帧是空记录。
    fn write_library(dir: &TempDir, stem: &str, frames: &[SyntheticFrame]) {
        let count = frames
            .iter()
            .map(|frame| frame.index as usize + 1)
            .max()
            .unwrap_or(1);
        let mut offsets = vec![0u32; count];
        let mut wzl = vec![0u8; 48];
        for frame in frames {
            let width = usize::from(frame.width);
            let height = usize::from(frame.height);
            // RGB24；每行按 4 字节对齐（`WeMadeLibrary.cs:231-234`）。
            let row = (width * 3).div_ceil(4) * 4;
            let mut raw = vec![0u8; row * height];
            for line in 0..height {
                for column in 0..width {
                    let at = line * row + column * 3;
                    raw[at..at + 3].copy_from_slice(&frame.color);
                }
            }
            let payload = zlib(&raw);
            let offset = u32::try_from(wzl.len()).expect("偏移放得下");
            offsets[frame.index as usize] = offset;
            wzl.push(6); // `format::RGB24`
            wzl.extend_from_slice(&[0, 0, 0]);
            wzl.extend_from_slice(&frame.width.to_le_bytes());
            wzl.extend_from_slice(&frame.height.to_le_bytes());
            wzl.extend_from_slice(&frame.offset_x.to_le_bytes());
            wzl.extend_from_slice(&frame.offset_y.to_le_bytes());
            wzl.extend_from_slice(
                &u32::try_from(payload.len())
                    .expect("长度放得下")
                    .to_le_bytes(),
            );
            wzl.extend_from_slice(&payload);
        }
        let mut wzx = vec![0u8; 48];
        let declared = u32::try_from(offsets.len()).expect("数量放得下");
        wzx[44..48].copy_from_slice(&declared.to_le_bytes());
        for offset in offsets {
            wzx.extend_from_slice(&offset.to_le_bytes());
        }
        std::fs::write(dir.join(&format!("{stem}.wzl")), wzl).expect("写 .wzl");
        std::fs::write(dir.join(&format!("{stem}.wzx")), wzx).expect("写 .wzx");
    }

    /// 身体颜色（红）、武器颜色（绿）、头发颜色（蓝）。
    const BODY_COLOR: [u8; 3] = [200, 60, 60];
    const WEAPON_COLOR: [u8; 3] = [60, 200, 60];
    const HAIR_COLOR: [u8; 3] = [60, 60, 200];
    /// 武器层覆盖身体的位置（相对格原点）：两者的公共区域。
    const OVERLAP: (i32, i32) = (22, -30);

    /// 一套带身体/武器/头发的合成素材目录。
    fn synthetic_assets() -> TempDir {
        let dir = TempDir::new("assets");
        // 身体：块 0（`hum.wzl`），8 个方向各 1 帧站立，40×60 盖住格子上方。
        let mut body: Vec<SyntheticFrame> = (0..8u32)
            .map(|direction| SyntheticFrame {
                index: direction * PLAYER_DIRECTION_STRIDE,
                width: 40,
                height: 60,
                offset_x: 0,
                offset_y: -58,
                color: BODY_COLOR,
            })
            .collect();
        // 走路：扁平布局从 64 开始，每方向 6 帧（`HUMAN_WALKING`）；每帧颜色不同，
        // 这样「同一实体在两个时间点渲染结果不同」可以被逐像素断言抓到。
        body.extend((0..8u32).flat_map(|direction| {
            (0..6u32).map(move |frame| SyntheticFrame {
                index: 64 + direction * PLAYER_DIRECTION_STRIDE + frame,
                width: 40,
                height: 60,
                offset_x: 0,
                offset_y: -58,
                color: walking_color(frame),
            })
        }));
        write_library(&dir, "hum", &body);
        // 武器：块 3（`weapon.wzl`）第 2 段，20×20 与身体重叠。
        let weapon: Vec<SyntheticFrame> = (0..8u32)
            .map(|direction| SyntheticFrame {
                index: 3 * PLAYER_FRAMES_PER_APPEARANCE + direction * PLAYER_DIRECTION_STRIDE,
                width: 20,
                height: 20,
                offset_x: 12,
                offset_y: -40,
                color: WEAPON_COLOR,
            })
            .collect();
        write_library(&dir, "weapon", &weapon);
        // 头发：块 2（`hair.wzl`），24×10 压在头顶。
        let hair: Vec<SyntheticFrame> = (0..8u32)
            .map(|direction| SyntheticFrame {
                index: 2 * PLAYER_FRAMES_PER_APPEARANCE + direction * PLAYER_DIRECTION_STRIDE,
                width: 24,
                height: 10,
                offset_x: 8,
                offset_y: -66,
                color: HAIR_COLOR,
            })
            .collect();
        write_library(&dir, "hair", &hair);
        // 第二套武器库：块 3 的站立段**整体后移 8 帧**（语料里 25/76 个武器块是这样），
        // 并且每个方向用不同颜色，用来验证「取到的是这个方向的帧」而不只是「有图」。
        let shifted: Vec<SyntheticFrame> = (0..8u32)
            .map(|direction| SyntheticFrame {
                index: 3 * PLAYER_FRAMES_PER_APPEARANCE
                    + PLAYER_DIRECTION_STRIDE
                    + direction * PLAYER_DIRECTION_STRIDE,
                width: 20,
                height: 20,
                offset_x: 12,
                offset_y: -40,
                color: shifted_color(direction),
            })
            .collect();
        write_library(&dir, "weapon2", &shifted);
        dir
    }

    /// 后移块里第 `direction` 个方向的颜色（每个方向都不同）。
    fn shifted_color(direction: u32) -> [u8; 3] {
        [60, 200, u8::try_from(20 + direction * 20).unwrap_or(0)]
    }

    /// 走路第 `frame` 帧的颜色（每帧都不同，用来断言动画确实在动）。
    fn walking_color(frame: u32) -> [u8; 3] {
        [u8::try_from(40 + frame * 25).unwrap_or(0), 120, 220]
    }

    fn equipped_player(direction: MirDirection) -> EntityView {
        let mut player = EntityView::new(Point::new(2, 3), direction, EntityKind::Player, 0);
        player.armour = 0;
        player.weapon = 3;
        player.weapon_shape = 3;
        player.hair = 2;
        player.shield = NO_LAYER;
        player.name = Some("装备测试".to_owned());
        player
    }

    /// 渲染一个实体到 200×200 的帧，返回帧与格原点。
    fn render_one(renderer: &mut EntityRenderer, player: &EntityView) -> (Frame, i32, i32) {
        let mut frame = Frame::new(200, 200);
        frame.clear([0, 0, 0, 255]);
        let viewport = Viewport::new(200, 200);
        let center = player.position;
        renderer.render(
            &mut frame,
            viewport,
            center,
            std::slice::from_ref(player),
            0,
        );
        let (origin_x, origin_y) = geometry::world_to_screen(viewport, center, player.position);
        (frame, origin_x, origin_y)
    }

    /// 按真实时间渲染一个实体到 200×200 的帧。
    fn render_one_at(
        renderer: &mut EntityRenderer,
        player: &EntityView,
        now_ms: u64,
    ) -> (Frame, i32, i32) {
        let mut frame = Frame::new(200, 200);
        frame.clear([0, 0, 0, 255]);
        let viewport = Viewport::new(200, 200);
        let center = player.position;
        renderer.render_at(
            &mut frame,
            viewport,
            center,
            std::slice::from_ref(player),
            now_ms,
        );
        let (origin_x, origin_y) = geometry::world_to_screen(viewport, center, player.position);
        (frame, origin_x, origin_y)
    }

    fn differing_pixels(left: &Frame, right: &Frame) -> usize {
        left.pixels
            .chunks_exact(4)
            .zip(right.pixels.chunks_exact(4))
            .filter(|(a, b)| a != b)
            .count()
    }

    fn has_color(frame: &Frame, color: [u8; 3]) -> bool {
        frame.pixels.chunks_exact(4).any(|pixel| {
            pixel[0] == color[0] && pixel[1] == color[1] && pixel[2] == color[2] && pixel[3] != 0
        })
    }

    #[test]
    fn equipment_layers_change_the_rendered_frame() {
        let dir = synthetic_assets();
        let mut renderer = EntityRenderer::new(EntityLibraries::open_dir(&dir.0));
        assert_eq!(renderer.libraries().file_count(), 4);

        let bare = EntityView::new(Point::new(2, 3), MirDirection::Down, EntityKind::Player, 0);
        let (bare_frame, _, _) = render_one(&mut renderer, &bare);
        let (dressed_frame, _, _) = render_one(&mut renderer, &equipped_player(MirDirection::Down));

        assert!(
            differing_pixels(&bare_frame, &dressed_frame) > 200,
            "带装备的帧必须与裸体帧不同（实际差异 {} 像素）",
            differing_pixels(&bare_frame, &dressed_frame)
        );
        assert!(
            has_color(&dressed_frame, BODY_COLOR),
            "身体层像素应出现在帧里"
        );
        assert!(
            has_color(&dressed_frame, WEAPON_COLOR),
            "武器层像素应出现在帧里"
        );
        assert!(
            has_color(&dressed_frame, HAIR_COLOR),
            "头发层像素应出现在帧里"
        );
        assert_eq!(
            renderer.cache_len(),
            3,
            "身体/武器/头发各缓存一张（第二次渲染复用同一张身体帧）"
        );
    }

    #[test]
    fn walking_animation_changes_the_rendered_frame() {
        let dir = synthetic_assets();
        let mut renderer = EntityRenderer::new(EntityLibraries::open_dir(&dir.0));
        let mut player =
            EntityView::new(Point::new(2, 3), MirDirection::Down, EntityKind::Player, 0);
        player.action = ActorAction::Walking;
        player.action_started_ms = 1_000;
        player.weapon = NO_LAYER;
        player.weapon_shape = NO_LAYER;
        player.hair = NO_LAYER;

        // 同一实体、同一位置：只改时间，走路动作必须换帧。
        let (first, _, _) = render_one_at(&mut renderer, &player, 1_000);
        let (second, _, _) = render_one_at(&mut renderer, &player, 1_300);
        assert!(
            differing_pixels(&first, &second) > 400,
            "走路动画在两个时间点必须画出不同的帧（实际差异 {} 像素）",
            differing_pixels(&first, &second)
        );

        // 600 ms 后回到站立第 0 帧：颜色应重新变回身体色。
        let (standing_again, _, _) = render_one_at(&mut renderer, &player, 1_600);
        assert!(
            has_color(&standing_again, BODY_COLOR),
            "走完 6 帧后必须回到站立帧"
        );
        assert!(
            !has_color(&standing_again, walking_color(0)),
            "回到站立帧后不应再出现走路帧的颜色"
        );
    }

    #[test]
    fn every_player_layer_uses_the_same_action_frame() {
        let mut player = equipped_player(MirDirection::Down);
        player.action = ActorAction::Walking;
        player.action_started_ms = 5_000;
        let plan = player_layer_plan_at(&player, 5_250);
        assert_eq!(plan.len(), 3, "身体/武器/头发三层都在");
        // 走路方向 Down=4、局部帧 2：64 + 4*8 + 2 = 98。
        let expected = u32::from(crate::animation::HUMAN_WALKING.index_of(MirDirection::Down, 2));
        for layer in plan.layers() {
            assert_eq!(
                layer.frame % PLAYER_FRAMES_PER_APPEARANCE,
                expected,
                "图层 {:?} 的动作帧必须与身体同步",
                layer.layer
            );
        }
    }

    #[test]
    fn shifted_weapon_block_is_compensated() {
        // 合成素材里 `weapon2.wzl` 块 3 的站立段整体后移 8 帧，且每个方向颜色不同：
        // 不补偿的话会取到**隔壁方向**的帧（颜色不对）或空帧（整层消失）。
        let dir = synthetic_assets();
        let mut renderer = EntityRenderer::new(EntityLibraries::open_dir(&dir.0));
        let mut player = equipped_player(MirDirection::Down);
        player.weapon_shape = 103; // 100 段 → weapon2.wzl
        player.weapon = 3;
        let (frame, origin_x, origin_y) = render_one(&mut renderer, &player);
        let pixel = frame
            .pixel(
                u32::try_from(origin_x + OVERLAP.0).expect("x"),
                u32::try_from(origin_y + OVERLAP.1).expect("y"),
            )
            .expect("重叠点应在帧内");
        // 朝下是 `MirDirection::Down` = 4，武器在身体之后画 → 重叠处是第 4 个方向的颜色。
        assert_eq!(
            &pixel[..3],
            &shifted_color(4),
            "后移块必须取到当前方向的帧（不是隔壁方向）"
        );

        // 朝上（方向 0）时武器在身体之后 → 被身体盖住，颜色是身体色。
        let mut behind = equipped_player(MirDirection::Up);
        behind.weapon_shape = 103;
        behind.weapon = 3;
        let (frame, origin_x, origin_y) = render_one(&mut renderer, &behind);
        let pixel = frame
            .pixel(
                u32::try_from(origin_x + OVERLAP.0).expect("x"),
                u32::try_from(origin_y + OVERLAP.1).expect("y"),
            )
            .expect("重叠点应在帧内");
        assert_eq!(
            &pixel[..3],
            &BODY_COLOR,
            "相位补偿不能改层顺序（朝上时武器仍被身体挡住）"
        );
    }

    #[test]
    fn weapon_is_drawn_behind_the_body_when_facing_up_or_left() {
        let dir = synthetic_assets();
        let mut renderer = EntityRenderer::new(EntityLibraries::open_dir(&dir.0));

        // 朝下：武器在身体之后画 → 重叠处是武器色。
        let (front, origin_x, origin_y) =
            render_one(&mut renderer, &equipped_player(MirDirection::Down));
        let pixel = front
            .pixel(
                u32::try_from(origin_x + OVERLAP.0).expect("x"),
                u32::try_from(origin_y + OVERLAP.1).expect("y"),
            )
            .expect("重叠点应在帧内");
        assert_eq!(
            &pixel[..3],
            &WEAPON_COLOR,
            "朝下时武器应盖在身体上（层顺序：身体→头发→武器）"
        );

        // 朝上：武器在身体之前画 → 重叠处是身体色。
        let (behind, origin_x, origin_y) =
            render_one(&mut renderer, &equipped_player(MirDirection::Up));
        let pixel = behind
            .pixel(
                u32::try_from(origin_x + OVERLAP.0).expect("x"),
                u32::try_from(origin_y + OVERLAP.1).expect("y"),
            )
            .expect("重叠点应在帧内");
        assert_eq!(
            &pixel[..3],
            &BODY_COLOR,
            "朝上时武器应被身体挡住（层顺序：武器→身体→头发）"
        );
        assert_ne!(
            behind.pixel(
                u32::try_from(origin_x + OVERLAP.0).expect("x"),
                u32::try_from(origin_y + OVERLAP.1).expect("y")
            ),
            front.pixel(
                u32::try_from(origin_x + OVERLAP.0).expect("x"),
                u32::try_from(origin_y + OVERLAP.1).expect("y")
            ),
            "层顺序画反时这个断言会失败"
        );
    }

    #[test]
    fn missing_layer_frames_degrade_to_the_bare_body() {
        let dir = synthetic_assets();
        let mut renderer = EntityRenderer::new(EntityLibraries::open_dir(&dir.0));
        let bare = EntityView::new(Point::new(2, 3), MirDirection::Down, EntityKind::Player, 0);
        let (bare_frame, _, _) = render_one(&mut renderer, &bare);

        // 武器块 9 在合成素材里没有帧；头发号也不存在 → 只剩身体层。
        let mut player = equipped_player(MirDirection::Down);
        player.weapon = 9;
        player.weapon_shape = 9;
        player.hair = 5;
        let (frame, _, _) = render_one(&mut renderer, &player);
        assert_eq!(
            differing_pixels(&bare_frame, &frame),
            0,
            "缺层的素材必须安静降级，不能画错层"
        );

        // 连身体库都不存在：退回占位方块。
        let mut empty = EntityRenderer::empty();
        let (placeholder, _, _) = render_one(&mut empty, &equipped_player(MirDirection::Down));
        assert!(placeholder.distinct_color_count() >= 2);
    }

    fn view(kind: EntityKind, image: u16) -> EntityView {
        EntityView::new(Point::new(2, 3), MirDirection::Down, kind, image)
    }

    #[test]
    fn monster_libraries_follow_classic_ten_per_file_grouping() {
        assert_eq!(monster_library(0), 1);
        assert_eq!(monster_library(9), 1);
        assert_eq!(monster_library(10), 2);
        assert_eq!(monster_library(27), 3);
        assert_eq!(monster_library(208), 21);
    }

    #[test]
    fn npc_libraries_switch_every_hundred_images() {
        assert_eq!(npc_library(0), (1, 0));
        assert_eq!(npc_library(7), (1, 420));
        assert_eq!(npc_library(100), (2, 0));
        assert_eq!(npc_library(250), (3, 50 * NPC_FRAMES_PER_IMAGE));
        assert_eq!(npc_library(999), (4, 99 * NPC_FRAMES_PER_IMAGE));
    }

    #[test]
    fn standing_frames_use_direction_and_animation() {
        assert_eq!(player_standing_frame(2, MirDirection::Up, 0), 1200);
        assert_eq!(
            player_standing_frame(2, MirDirection::Down, 5),
            1200 + 4 * 8 + 1
        );
        assert_eq!(monster_standing_frame(27, MirDirection::Up, 0), 7 * 430);
        assert_eq!(
            monster_standing_frame(27, MirDirection::Down, 3),
            7 * 430 + 4 * 8 + 3
        );
    }

    #[test]
    fn player_and_layer_libraries_follow_the_flat_corpus() {
        // 身体：hum 0..23、hum2 24..53、hum3 54..99。
        assert_eq!(player_library(0), (1, 0));
        assert_eq!(player_library(23), (1, 23));
        assert_eq!(player_library(24), (2, 0));
        assert_eq!(player_library(53), (2, 29));
        assert_eq!(player_library(54), (3, 0));
        assert_eq!(player_library(99), (3, 45));
        // 超出语料范围 → 最后一个库的最后一块。
        assert_eq!(player_library(200), (3, 45));
        // 武器：shape/100 选库，shape%100 是块号。
        assert_eq!(weapon_library(0), (1, 0));
        assert_eq!(weapon_library(37), (1, 37));
        assert_eq!(weapon_library(106), (2, 6));
        assert_eq!(weapon_library(218), (3, 18));
        // 头发：hair 0..5、hair2 6..27、hair3 28..32、hair4 33..。
        assert_eq!(hair_library(0), (1, 0));
        assert_eq!(hair_library(5), (1, 5));
        assert_eq!(hair_library(6), (2, 0));
        assert_eq!(hair_library(27), (2, 21));
        assert_eq!(hair_library(28), (3, 0));
        assert_eq!(hair_library(33), (4, 0));
        assert_eq!(hair_library(999), (4, 151));
        assert_eq!(SpriteLibrary::Hum(1).stem(), "hum");
        assert_eq!(SpriteLibrary::Hum(3).stem(), "hum3");
        assert_eq!(SpriteLibrary::Weapon(2).stem(), "weapon2");
        assert_eq!(SpriteLibrary::Hair(4).stem(), "hair4");
        assert_eq!(SpriteLibrary::Shield.stem(), "shield");
    }

    #[test]
    fn layer_plan_puts_the_weapon_behind_for_left_facing_directions() {
        let mut player = view(EntityKind::Player, 0);
        player.armour = 3;
        player.hair = 2;
        player.weapon = 7;
        player.weapon_shape = 7;

        // 朝下：武器在身体之后（最前面）。
        let plan = player_layer_plan(&player, 0);
        assert_eq!(
            plan.layers()
                .iter()
                .map(|item| item.layer)
                .collect::<Vec<_>>(),
            vec![PlayerLayer::Body, PlayerLayer::Hair, PlayerLayer::Weapon]
        );
        assert_eq!(plan.layers()[0].library, SpriteLibrary::Hum(1));
        assert_eq!(plan.layers()[0].frame, 3 * 600 + 4 * 8);
        assert_eq!(plan.layers()[1].library, SpriteLibrary::Hair(1));
        assert_eq!(plan.layers()[1].frame, 2 * 600 + 4 * 8);
        assert_eq!(plan.layers()[2].library, SpriteLibrary::Weapon(1));
        assert_eq!(plan.layers()[2].frame, 7 * 600 + 4 * 8);

        // 朝上：武器在身体之前（被身体挡住）。
        player.direction = MirDirection::Up;
        let plan = player_layer_plan(&player, 0);
        assert_eq!(
            plan.layers()
                .iter()
                .map(|item| item.layer)
                .collect::<Vec<_>>(),
            vec![PlayerLayer::Weapon, PlayerLayer::Body, PlayerLayer::Hair]
        );
        assert_eq!(plan.layers()[0].frame, 7 * 600);
    }

    #[test]
    fn layer_plan_skips_missing_layers() {
        let mut player = view(EntityKind::Player, 0);
        player.armour = 30; // → hum2 块 6
        player.hair = NO_LAYER;
        player.weapon = NO_LAYER;
        player.weapon_shape = NO_LAYER;
        player.shield = NO_LAYER;
        let plan = player_layer_plan(&player, 3);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan.layers()[0].layer, PlayerLayer::Body);
        assert_eq!(plan.layers()[0].library, SpriteLibrary::Hum(2));
        assert_eq!(plan.layers()[0].frame, 6 * 600 + 4 * 8 + 3);
        assert!(!player.has_weapon());

        // 盾牌层排在头发之后、前向武器之前。
        player.direction = MirDirection::DownRight;
        player.hair = 0;
        player.shield = 1;
        player.weapon = 2;
        player.weapon_shape = 2;
        let plan = player_layer_plan(&player, 0);
        assert_eq!(
            plan.layers()
                .iter()
                .map(|item| item.layer)
                .collect::<Vec<_>>(),
            vec![
                PlayerLayer::Body,
                PlayerLayer::Hair,
                PlayerLayer::Shield,
                PlayerLayer::Weapon
            ]
        );
        assert_eq!(plan.layers()[2].library, SpriteLibrary::Shield);
    }

    #[test]
    fn block_start_is_the_first_frame_of_the_appearance() {
        let frame = LayerFrame {
            layer: PlayerLayer::Weapon,
            library: SpriteLibrary::Weapon(1),
            frame: 7 * 600 + 5 * 8 + 2,
        };
        assert_eq!(frame.block_start(), 7 * 600);
    }

    #[test]
    fn weapon_front_matches_crystals_direction_split() {
        for direction in [
            MirDirection::UpRight,
            MirDirection::Right,
            MirDirection::DownRight,
            MirDirection::Down,
        ] {
            assert!(weapon_drawn_in_front(direction), "{direction:?}");
        }
        for direction in [
            MirDirection::Up,
            MirDirection::UpLeft,
            MirDirection::Left,
            MirDirection::DownLeft,
        ] {
            assert!(!weapon_drawn_in_front(direction), "{direction:?}");
        }
    }

    #[test]
    fn missing_libraries_draw_colored_placeholders() {
        let mut renderer = EntityRenderer::empty();
        let mut frame = Frame::new(200, 200);
        frame.clear([0, 0, 0, 255]);
        let entities = [
            view(EntityKind::Player, 0),
            EntityView {
                position: Point::new(3, 3),
                ..view(EntityKind::Monster, 5)
            },
            EntityView {
                position: Point::new(2, 4),
                ..view(EntityKind::Npc, 0)
            },
            EntityView {
                position: Point::new(1, 3),
                ..view(EntityKind::Item, 0)
            },
        ];
        renderer.render(
            &mut frame,
            Viewport::new(200, 200),
            Point::new(2, 3),
            &entities,
            0,
        );
        assert!(frame.distinct_color_count() >= 5, "四种占位色都应出现");
        assert_eq!(renderer.cache_len(), 0, "没有素材就不该缓存图像");
    }

    #[test]
    fn entities_are_sorted_by_y_then_x() {
        // 两个同格实体：后画的会覆盖先画的；排序必须稳定。
        let mut entities = vec![
            view(EntityKind::Player, 0),
            EntityView {
                kind: EntityKind::Monster,
                ..view(EntityKind::Monster, 5)
            },
            EntityView {
                position: Point::new(2, 2),
                ..view(EntityKind::Item, 0)
            },
        ];
        let mut order: Vec<usize> = (0..entities.len()).collect();
        order.sort_by_key(|&index| (entities[index].position.y, entities[index].position.x));
        assert_eq!(order, vec![2, 0, 1]);
        entities.clear();
    }

    #[test]
    fn hp_bar_is_clipped_inside_the_frame() {
        let mut frame = Frame::new(40, 40);
        frame.clear([0, 0, 0, 255]);
        draw_health_bar(&mut frame, 0, -10, 100, false);
        // 完全在帧外：什么也不画。
        assert_eq!(frame.distinct_color_count(), 1);
        draw_health_bar(&mut frame, 20, 10, 50, false);
        assert!(frame.distinct_color_count() >= 2);
    }
}
