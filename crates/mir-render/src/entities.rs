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
//! | 玩家坐骑 | `horse_x.wzl`（3 个 640 帧块） | 块号 `mount-1`；动作表见 [`mount_frames`] | `DrawMount` `PlayerObject.cs:5084-5089`；`Frames.cs:188-192` |
//! | 玩家翅膀 | `wingeffect_x.wzl`（2 个 600 帧块） | 块号 `wings-1`；effect 帧表见 `animation::wing_frames` | `DrawWings` `PlayerObject.cs:5075-5081`、`:762`；`Frames.cs:157-170` |
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
//! Crystal 的玩家对象把外观拆成多层，`PlayerObject.Draw` 按方向决定武器画在身体
//! **前**还是**后**（`PlayerObject.cs:4877-4926`）：
//!
//! ```text
//! DrawMount();                                        // :4884，坐骑在身体之下
//! if (dir ∈ {Left, Up, UpLeft, DownLeft}) DrawWeapon(); else DrawWeapon2();
//! DrawBody();                                         // PlayerObject.cs:4894
//! DrawHead();                                         // 头发，PlayerObject.cs:4896-4911
//! DrawWings();                                        // 翅膀，PlayerObject.cs:4901/4908
//! if (dir ∈ {UpRight, Right, DownRight, Down}) DrawWeapon(); else DrawWeapon2();
//!                                                     // PlayerObject.cs:4914-4923
//! ```
//!
//! 也就是说朝左上/上/左下/左时武器在身体**后面**，朝右上/右/右下/下时武器在身体
//! **前面**；`DrawWeapon2` 是另一只手的武器（刺客双持），普通职业的
//! `WeaponLibrary2` 恒为 `null`（`PlayerObject.cs:413-431`），所以本作不同时画两把。
//! Crystal 在骑乘时不画武器（`:4886,4914`），本作同样跳过；翅膀固定画在头发之后
//! （需求；Crystal 会按方向交换 `DrawHead`/`DrawWings`，`:4896-4911`）。
//! 本模块用 [`PlayerLayer`] + [`player_layer_plan`] 把这套顺序表达成数据，绘制与
//! 测试共用同一份顺序表。
//!
//! 身体/武器/头发的帧号与身体**完全相同**：`block*600 + direction*8 + n`
//! （`n = 0..=3` 站立帧）；坐骑按 [`MOUNT_FRAMES_PER_APPEARANCE`]（640）分块，
//! 翅膀按 [`WING_FRAMES_PER_APPEARANCE`]（600）分块，使用各自的 effect 帧表。
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
//!   ——每一层共用同一个格原点与各自的图像偏移，所以不需要额外的层间偏移。
//! * 物品：Crystal 把物品在格子里居中（`ItemObject.cs:78-79`），不叠加图像偏移
//!   （`ItemObject.cs:72-73` 的 `Draw` 使用默认 `offSet: false`）。
//!
//! 实体按地图 Y 升序、同一行按 X 升序绘制（与 `GameScene.cs:10798-11008` 的对象
//! 循环顺序一致），靠下的对象覆盖靠上的对象。
//!
//! # 与 Crystal 的差异
//!
//! * 不画武器发光、光效、中毒/等级特效（`EntityView` 没有这些字段）。
//! * 坐骑只接扁平 `horse_x.wzl` 的 3 个干净坐骑块；`horse.wzl`/`horse2.wzl` 是带骑手的
//!   整身外观，不拿来当坐骑层（见模块「与 Crystal 的差异」）。
//! * 不画名字文字；名字由 `mir-client` 的 `ui::worldtext` 在世界层绘制。
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
//! `WeaponLibrary`/`HairLibrary`，`PlayerObject.cs:761,5054,5062`）；坐骑用
//! [`mount_frames`] + 自己的起点/间隔，翅膀用 [`crate::animation::wing_frames`] 的
//! effect 列（`PlayerObject.cs:762`）；怪物/NPC 用各自的图库帧表。没有动作字段的旧调用
//! （[`EntityRenderer::render`]）按动画计数当作
//! 100 ms 节拍推进时间，行为与之前的站立帧播放兼容。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mir_asset::{Image, ImageInfo, LibrarySource};
use mir_core::Point;
use mir_core::enums::MirDirection;

use crate::Frame;
use crate::animation::{
    ActorAction, ActorAnimation, BodyKind, FrameChoice, FrameSequence, HUMAN_STANDING,
    NPC_STANDING, WING_FRAMES_PER_APPEARANCE, wing_frames,
};
use crate::geometry::{self, CELL_HEIGHT, CELL_WIDTH, Viewport};
use crate::scene::remove_black_all;

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
/// 扁平 `horse_x.wzl` 每个坐骑外观块占的帧数。
///
/// 实测 `horse_x.wzx` 有 1960 项 = 3 个完整外观 × 640 帧 + 40 帧残块；
/// `horse.wzl` 是 9 × 640、`horse2.wzl` 是 6 × 640，布局相同。Crystal 的 `Mount/00.Lib`
/// 只有 216 帧（`MountStanding` 的 416 偏移被 `DrawMount` 减掉，`PlayerObject.cs:5089`），
/// 扁平语料把整个「骑马身体」布局塞进 640 帧，所以块大小取 640。
pub const MOUNT_FRAMES_PER_APPEARANCE: u32 = 640;
/// `horse_x.wzl` 里的坐骑外观块数（3 个完整块；第 4 块只有 40 帧，丢弃）。
pub const HORSE_X_BLOCKS: u16 = 3;
/// `wingeffect_x.wzl` 里的翅膀外观块数（1200 帧 = 2 × 600）。
pub const WING_BLOCKS: u16 = 2;
/// 坐骑站立：Crystal `Frames.cs:188` 的 `Frame(416, 4, 0, 500)`。
///
/// `DrawMount` 用 `DrawFrame - 416` 取坐骑图（`PlayerObject.cs:5089`），
/// 扁平 `horse_x.wzl` 的站立段正好从块首开始，方向步长 8（4 张有效 + 4 空槽）。
pub const MOUNT_STANDING: FrameSequence = FrameSequence::new(0, 4, 4, 500);
/// 坐骑行走：Crystal `Frames.cs:189` 的 `Frame(448, 8, 0, 100)`。
///
/// 扁平语料每个方向只有 6 张有效图（与 `hum.wzl` 行走相同），所以把 `count/skip`
/// 改成 6/2；`start = 64` 是 `horse*.wzl` 实测的行走段起点。
// 简化：Crystal 的坐骑行走每方向 8 帧，扁平素材只有 6 帧。原因：`horse*.wzl`
// 的行走段与人类 `hum.wzl` 一样是 6 帧 + 2 空槽。
pub const MOUNT_WALKING: FrameSequence = FrameSequence::new(64, 6, 2, 100);
/// 坐骑跑步：Crystal `Frames.cs:190` 的 `Frame(512, 6, 0, 100)`。
///
/// 扁平布局从 128 开始（每方向 6 帧 + 2 空槽）。
pub const MOUNT_RUNNING: FrameSequence = FrameSequence::new(128, 6, 2, 100);
/// 坐骑受击：Crystal `Frames.cs:191` 的 `Frame(560, 3, 0, 100)`。
///
/// 扁平布局从 192 开始（每方向 3 帧 + 5 空槽）。
pub const MOUNT_STRUCK: FrameSequence = FrameSequence::new(192, 3, 5, 100);
/// 坐骑攻击：Crystal `Frames.cs:192` 的 `Frame(584, 6, 0, 100)`。
///
/// 扁平布局的攻击段从 384 开始（每方向 6 帧 + 2 空槽）。
// 简化：攻击起点用扁平 horse_x.wzl 实测的 384（Crystal 的相对偏移是 168）。
// 原因：扁平 `.wzl` 的动作顺序与 Crystal `Mount/*.Lib` 不同。
pub const MOUNT_ATTACKING: FrameSequence = FrameSequence::new(384, 6, 2, 100);
/// 坐骑死亡：Crystal 的 `Frames.cs` 没有 `MountDie` 动作；扁平素材在 512 起有
/// 每方向 3 帧的死亡段，这里复用它，Interval 与人类受击相同。
// 简化：Crystal 没有坐骑死亡/尸体动作。原因：扁平 `horse*.wzl` 有这两段，
// 不播的话骑乘时死亡会停在站立首帧。
pub const MOUNT_DYING: FrameSequence = FrameSequence::new(512, 3, 5, 100);
/// 坐骑尸体：扁平素材在 576 起有每方向 8 帧的尸体段，取第 1 帧。
pub const MOUNT_DEAD: FrameSequence = FrameSequence::new(576, 1, 7, 1000);

/// 取坐骑动作帧表（起点已换算成扁平 `horse*.wzl` 的块内偏移）。
#[must_use]
pub const fn mount_frames(action: ActorAction) -> FrameSequence {
    match action {
        ActorAction::Standing => MOUNT_STANDING,
        ActorAction::Walking => MOUNT_WALKING,
        ActorAction::Running => MOUNT_RUNNING,
        ActorAction::Attacking => MOUNT_ATTACKING,
        ActorAction::Struck => MOUNT_STRUCK,
        ActorAction::Dying => MOUNT_DYING,
        ActorAction::Dead => MOUNT_DEAD,
    }
}
/// `hum*.wzl` 每个库里的外观块数（实测：`hum` 24、`hum2` 30、`hum3` 46）。
pub const HUM_BLOCKS: [u16; 3] = [24, 30, 46];
/// `hair*.wzl` 每个库里的发型块数（实测：`hair` 6、`hair2` 22、`hair3` 5、`hair4` 152）。
pub const HAIR_BLOCKS: [u16; 4] = [6, 22, 5, 152];
/// 实体解码缓存上限（张）。一张 64×80 的 RGBA 图约 20 KiB。
///
/// 一个玩家最多占 4 层（身体/武器/头发/盾牌），所以上限比单层时翻了倍。
pub const MAX_ENTITY_CACHED_IMAGES: usize = 1024;

/// 头顶血条宽（Crystal `32 * PercentHealth / 100`，`MapObject.cs:506`）。
pub const HEALTH_BAR_WIDTH: i32 = 32;
/// 头顶血条高（Crystal 的 4 像素，`MapObject.cs:506`）。
pub const HEALTH_BAR_HEIGHT: u32 = 4;
/// 血条在格内的横向偏移（Crystal `DisplayRectangle.X + 8`，`MapObject.cs:478,506`）。
pub const HEALTH_BAR_OFFSET_X: i32 = 8;
/// 血条前景色：>50% 绿色（对应 Crystal `Prguse2` 的 1/11/12 号条，
/// `MapObject.cs:481-504`；本作没有这套 UI 图库，改按百分比着色）。
pub const HEALTH_GREEN: [u8; 4] = [72, 208, 96, 255];
/// 血条前景色：25%..=50% 黄色。
pub const HEALTH_YELLOW: [u8; 4] = [240, 208, 72, 255];
/// 血条前景色：<=25% 红色。
pub const HEALTH_RED: [u8; 4] = [232, 72, 72, 255];
/// 死亡实体的血条色（Crystal 死亡时不画血条，`MapObject.cs:470`；本作保留灰色条）。
pub const HEALTH_DEAD: [u8; 4] = [90, 90, 90, 255];
/// 血条底板色（Crystal 用 `Prguse2` 的 0 号图，`MapObject.cs:478`）。
pub const HEALTH_BACKGROUND: [u8; 4] = [0, 0, 0, 255];

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
    /// 绘制排序用的 Y（默认等于 `position.y`）。
    ///
    /// Crystal 的 `DrawY = Movement.Y > CurrentLocation.Y ? Movement.Y : CurrentLocation.Y`
    /// （`PlayerObject.cs:845`）：走/跑中的对象按**起点和终点里更靠南**的行排序，
    /// 避免向下的移动在起点行就被南侧站立对象盖住。R46 远端插值用本字段保留该语义，
    /// 不改既有排序算法（仍按 `(sort_y, position.x)` 升序）。
    pub sort_y: i32,
    /// 绘制时在格原点上叠加的像素偏移（默认 `(0, 0)`）。
    ///
    /// Crystal 把玩家自己的 `OffSetMove` 加到**其他**对象的绘制坐标上
    /// （`PlayerObject.cs:850-854`），自机则保持屏幕中心。本作把这条规则拆成
    /// 「全局像素偏移（地形/实体层统一加）+ 实体自身像素偏移」：自机取全局偏移的
    /// 相反数把自己钉在屏幕中心；远端玩家 R46 用自身推进量（`RemoteMotion`）。
    /// 详见 `mir-client/src/camera.rs`。
    pub pixel_offset: (i32, i32),
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
    /// 坐骑外观号：`0` = 没有坐骑，`1..` = `S.ObjectPlayer.MountType + 1`。
    ///
    /// 来自 `S.ObjectPlayer.riding_mount` / `S.MountUpdate`（`PlayerObject.cs:202-219`
    /// 的 `MountUpdate` 设置 `MountType`/`RidingMount`）。扁平 `horse_x.wzl` 有 3 个
    /// 完整外观块，外观号对 [`HORSE_X_BLOCKS`] 取模。
    pub mount: u16,
    /// 翅膀外观号：`0` = 没有翅膀，`1..` = `S.ObjectPlayer.wing_effect`
    /// （Crystal `PlayerObject.WingEffect`，`PlayerObject.cs:78,141,176`）。
    ///
    /// 扁平 `wingeffect_x.wzl` 有 2 个外观块，外观号对 [`WING_BLOCKS`] 取模。
    pub wings: u16,
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
            sort_y: position.y,
            pixel_offset: (0, 0),
            direction,
            kind,
            image,
            armour: 0,
            weapon: NO_LAYER,
            weapon_shape: NO_LAYER,
            hair: NO_LAYER,
            shield: NO_LAYER,
            mount: 0,
            wings: 0,
            name: None,
            dead: false,
            hp_percent: None,
            action: ActorAction::Standing,
            action_started_ms: 0,
        }
    }

    /// 设置绘制像素偏移（自机抵消全局相机偏移用，见 [`Self::pixel_offset`]）。
    #[must_use]
    pub fn with_pixel_offset(mut self, offset: (i32, i32)) -> Self {
        self.pixel_offset = offset;
        self
    }

    /// 玩家外观层里是否画武器。
    #[must_use]
    pub fn has_weapon(&self) -> bool {
        self.weapon != NO_LAYER && self.weapon_shape != NO_LAYER
    }

    /// 是否骑着坐骑。
    #[must_use]
    pub const fn has_mount(&self) -> bool {
        self.mount != 0
    }

    /// 是否带翅膀层。
    ///
    /// Crystal 只在 `WingEffect in 1..99` 时给 `WingLibrary` 赋值（`PlayerObject.cs:577`），
    /// `>= 100` 是其它特效（`:650`），所以这里也排除。
    #[must_use]
    pub const fn has_wings(&self) -> bool {
        self.wings != 0 && self.wings < 100
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
    /// 坐骑（`horse_x.wzl`；画在身体之下）。
    Mount,
    /// 武器（`weapon*.wzl`）。
    Weapon,
    /// 身体（`hum*.wzl`）。
    Body,
    /// 头发（`hair*.wzl`）。
    Hair,
    /// 翅膀（`wingeffect_x.wzl`；画在头发之后）。
    Wing,
    /// 盾牌（`shield.wzl`；Crystal 没有这一层）。
    Shield,
}

impl PlayerLayer {
    /// 稳定的小写名字（测试/日志用）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mount => "mount",
            Self::Weapon => "weapon",
            Self::Body => "body",
            Self::Hair => "hair",
            Self::Wing => "wing",
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
    /// `horse_x.wzl`（扁平语料里的坐骑外观库）。
    Mount,
    /// `wingeffect_x.wzl`（扁平语料里的翅膀外观库）。
    Wing,
    /// `shield.wzl`。
    Shield,
    /// `npc.wzl` / `npc2.wzl` / `npc3.wzl` / `npc4.wzl`。
    Npc(u8),
    /// `mon1.wzl` … `monN.wzl`。
    Monster(u16),
    /// `items.wzl`（找不到时回退 `dnitems.wzl`）。
    Item,
    /// `effect.wzl`：攻击挥砍/技能命中的美术光效（扁平语料的 `effect*.wzl`）。
    Effect,
    /// `magic.wzl`：技能光效（回合二的雷电/火墙等命中闪光）。
    Magic,
    /// `magic2.wzl`：攻击挥砍/雷电束等第二套技能光效。
    Magic2,
}

/// 特效图库种类（[`EntityRenderer::effect_image`] 用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FxKind {
    /// 攻击挥砍光效（`magic2.wzl` 第 0~2 帧的火花）。
    Swing,
    /// 技能命中闪光（`magic.wzl` 前 10 帧的火环）。
    Magic,
    /// 雷电类技能的命中闪光（`magic2.wzl` 第 20~29 帧的闪电束）。
    Bolt,
}

impl FxKind {
    /// 对应的精灵图库。
    #[must_use]
    pub const fn library(self) -> SpriteLibrary {
        match self {
            Self::Swing | Self::Bolt => SpriteLibrary::Magic2,
            Self::Magic => SpriteLibrary::Magic,
        }
    }

    /// 图库里的起始帧号。
    #[must_use]
    pub const fn start_frame(self) -> u32 {
        match self {
            Self::Bolt => 20,
            Self::Swing | Self::Magic => 0,
        }
    }
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
            Self::Mount => "horse_x".to_owned(),
            Self::Wing => "wingeffect_x".to_owned(),
            Self::Shield => "shield".to_owned(),
            Self::Npc(1) => "npc".to_owned(),
            Self::Npc(index) => format!("npc{index}"),
            Self::Monster(index) => format!("mon{index}"),
            Self::Item => "items".to_owned(),
            Self::Effect => "effect".to_owned(),
            Self::Magic => "magic".to_owned(),
            Self::Magic2 => "magic2".to_owned(),
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
    ///
    /// 坐骑块是 [`MOUNT_FRAMES_PER_APPEARANCE`]（640），翅膀与身体/武器/头发是
    /// [`PLAYER_FRAMES_PER_APPEARANCE`]（600）。
    #[must_use]
    pub fn block_start(self) -> u32 {
        let block = if self.layer == PlayerLayer::Mount {
            MOUNT_FRAMES_PER_APPEARANCE
        } else {
            PLAYER_FRAMES_PER_APPEARANCE
        };
        self.frame / block * block
    }
}

/// 一个玩家的最多 6 层绘制计划（按绘制先后排列）。
///
/// 用定长数组而不是 `Vec`，避免每帧每个玩家都分配一次堆内存。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerLayerPlan {
    /// 定长缓冲。
    layers: [LayerFrame; 6],
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
/// ```text
/// DrawMount();                                   // :4884，身体之下
/// 朝左四方向（Left/Up/UpLeft/DownLeft）: DrawWeapon();
/// DrawBody();                                    // :4894
/// DrawHead();                                    // :4898 或 :4910（头发）
/// DrawWings();                                   // :4901 或 :4908（需求：头发之后）
/// 朝右四方向（UpRight/Right/DownRight/Down）: DrawWeapon();
/// ```
///
/// 本作把翅膀固定画在头发之后（需求），与 Crystal 按方向交换 `DrawHead`/`DrawWings`
/// 的两支（`:4896-4911`）略有不同；坐骑永远最先画（`:4884`），骑乘时 Crystal 不画
/// 武器（`:4886`、`:4914`），本作照做。
///
/// 空图层（`NO_LAYER` / `0`）不会出现在计划里。身体/武器/头发共用同一个动作帧号：
/// Crystal 的 `DrawFrame` 同时用于它们（`PlayerObject.cs:761,5054,5062`）；坐骑用
/// [`mount_frames`]，翅膀用 `PlayerObject.DrawWingFrame`（`PlayerObject.cs:762`）与
/// `wing_frames`，各自有自己的起点与间隔。
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
    player_layer_plan_with_choice(&copy, choice_for(&copy, now_ms), now_ms)
}

/// 按真实时间（Unix 毫秒）生成一个玩家的层绘制计划。
#[must_use]
pub fn player_layer_plan_at(entity: &EntityView, now_ms: u64) -> PlayerLayerPlan {
    player_layer_plan_with_choice(entity, choice_for(entity, now_ms), now_ms)
}

/// 实体当前应画的动作帧。
fn choice_for(entity: &EntityView, now_ms: u64) -> FrameChoice {
    entity.animation().frame_at(now_ms)
}

/// 按动作帧表的「循环/回站立/保持最后一帧」规则取帧。
///
/// 与 [`ActorAnimation::frame_at`] 相同的播放规则，但帧表由调用方给出（坐骑/翅膀）。
fn sequence_choice(
    entity: &EntityView,
    now_ms: u64,
    frames: FrameSequence,
    standing: FrameSequence,
) -> FrameChoice {
    let elapsed = now_ms.saturating_sub(entity.action_started_ms);
    let action = entity.animation().action;
    if action == ActorAction::Standing {
        let interval = frames.interval_ms.max(1);
        let count = u64::from(frames.count.max(1));
        let local = u16::try_from((elapsed / interval) % count).unwrap_or(0);
        return FrameChoice {
            index: frames.index_of(entity.direction, local),
            finished: false,
        };
    }
    if action == ActorAction::Dead {
        return FrameChoice {
            index: frames.index_of(entity.direction, 0),
            finished: false,
        };
    }
    let choice = frames.frame_at(entity.direction, elapsed);
    if choice.finished && action.returns_to_standing() {
        return FrameChoice {
            index: standing.index_of(entity.direction, 0),
            finished: true,
        };
    }
    choice
}

/// 用已经算好的动作帧号拼层计划。
fn player_layer_plan_with_choice(
    entity: &EntityView,
    choice: FrameChoice,
    now_ms: u64,
) -> PlayerLayerPlan {
    let mut plan = PlayerLayerPlan {
        layers: [LayerFrame {
            layer: PlayerLayer::Body,
            library: SpriteLibrary::Hum(1),
            frame: 0,
        }; 6],
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
    let mount = entity.has_mount().then(|| {
        // 简化：`horse_x.wzl` 只有 3 个完整坐骑块，外观号对 3 取模。
        // 原因：扁平语料没有 Crystal `Mount/00..11.Lib` 那样的一外观一文件。
        let block = (entity.mount - 1) % HORSE_X_BLOCKS.max(1);
        let choice = sequence_choice(
            entity,
            now_ms,
            mount_frames(entity.animation().action),
            mount_frames(ActorAction::Standing),
        );
        LayerFrame {
            layer: PlayerLayer::Mount,
            library: SpriteLibrary::Mount,
            frame: u32::from(block) * MOUNT_FRAMES_PER_APPEARANCE + u32::from(choice.index),
        }
    });
    let wing = entity.has_wings().then(|| {
        // 简化：`wingeffect_x.wzl` 只有 2 个翅膀块，外观号对 2 取模。
        // 原因：Crystal 的 `CHumEffect/*.Lib` 有 6 个文件，扁平语料只合成了 2 个。
        let block = (entity.wings - 1) % WING_BLOCKS.max(1);
        let choice = sequence_choice(
            entity,
            now_ms,
            wing_frames(entity.animation().action),
            wing_frames(ActorAction::Standing),
        );
        LayerFrame {
            layer: PlayerLayer::Wing,
            library: SpriteLibrary::Wing,
            frame: u32::from(block) * WING_FRAMES_PER_APPEARANCE + u32::from(choice.index),
        }
    });

    if let Some(mount) = mount {
        plan.push(mount);
    }
    let weapon = if entity.has_mount() { None } else { weapon };
    // 简化：Crystal 按方向交换 `DrawHead`/`DrawWings`（PlayerObject.cs:4896-4911），
    // 本作按需求固定为「头发 → 翅膀」。原因：需求明确要求翅膀画在头发之后。
    if weapon_drawn_in_front(direction) {
        plan.push(body);
        if let Some(hair) = hair {
            plan.push(hair);
        }
        if let Some(wing) = wing {
            plan.push(wing);
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
        if let Some(wing) = wing {
            plan.push(wing);
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
/// `npc*`、`mon*`、`items`、`horse_x`、`wingeffect_x`。
#[derive(Debug, Default)]
pub struct EntityLibraries {
    /// 素材目录（诊断用）。
    directory: PathBuf,
    /// 小写文件名（不含扩展名）→ 路径。
    files: BTreeMap<String, PathBuf>,
    /// 拆包目录根（`<asset_root>/gfx`）；`None` 表示只用原始 `.wzl`。
    gfx_dir: Option<PathBuf>,
    /// 素材根（`library-map.json` 的 `dirs` 相对它解析）。
    asset_root: Option<PathBuf>,
    /// 库映射表（`stem → dirs/prefix`）。
    map: Option<mir_asset::LibraryMap>,
    /// 发布期小包集合。
    packs: Option<mir_asset::PackSet>,
    /// 是否允许回退原始 `.wzl`（只有 `asset_mode = legacy` 为真）。
    allow_legacy: bool,
    /// 已尝试打开的图库；`None` 表示文件不存在或打开失败。
    opened: BTreeMap<String, Option<LibrarySource>>,
}

impl EntityLibraries {
    /// 扫描目录，建立文件名索引（不打开任何图像库）。
    ///
    /// 目录不存在时返回空集合（地图渲染仍然可用，实体退化成占位方块）。
    #[must_use]
    pub fn open_dir(directory: impl AsRef<Path>) -> Self {
        let directory = directory.as_ref().to_path_buf();
        #[allow(unused_mut)]
        let mut files = BTreeMap::new();
        // 老格式目录扫描只在 `legacy` feature 下编译（运行时只读自有格式）。
        #[cfg(feature = "legacy")]
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
            gfx_dir: None,
            asset_root: None,
            map: None,
            packs: None,
            allow_legacy: true,
            opened: BTreeMap::new(),
        }
    }

    /// 同 [`Self::open_dir`]，但优先用 `<gfx_dir>/<stem>/index.json` 的拆包目录。
    #[must_use]
    pub fn open_dir_with_gfx(directory: impl AsRef<Path>, gfx_dir: Option<&Path>) -> Self {
        let mut libraries = Self::open_dir(directory);
        libraries.gfx_dir = gfx_dir.map(Path::to_path_buf);
        libraries
    }

    /// 完整加载：**散装（library-map 多目录）→ 小包（`.mnp`）→ 原始 `.wzl`**。
    ///
    /// 与 [`Self::open_dir_with_gfx`] 的区别：release 形态下没有 `.wzl` 文件，
    /// 必须靠 `map` + `packs` 才能找到图库。
    #[must_use]
    pub fn open_with_sources(
        directory: impl AsRef<Path>,
        gfx_dir: Option<&Path>,
        asset_root: Option<&Path>,
        map: Option<&mir_asset::LibraryMap>,
        packs: Option<&mir_asset::PackSet>,
        allow_legacy: bool,
    ) -> Self {
        let mut libraries = Self::open_dir(directory);
        libraries.gfx_dir = gfx_dir.map(Path::to_path_buf);
        libraries.asset_root = asset_root.map(Path::to_path_buf);
        libraries.map = map.cloned();
        libraries.packs = packs.cloned();
        libraries.allow_legacy = allow_legacy;
        libraries
    }

    /// 空图库集合（测试与无素材环境）。
    #[must_use]
    pub fn empty(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
            files: BTreeMap::new(),
            gfx_dir: None,
            asset_root: None,
            map: None,
            packs: None,
            allow_legacy: true,
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
        let stem = library.stem();
        self.map_dirs(&stem).is_some() || self.path_for_stem(&stem).is_some()
    }

    /// `library-map.json` 里该 stem 的**存在**的拆包目录。
    fn map_dirs(&self, stem: &str) -> Option<Vec<PathBuf>> {
        let map = self.map.as_ref()?;
        let root = self.asset_root.as_ref()?;
        let dirs = map.dirs(stem)?;
        let paths: Vec<PathBuf> = dirs
            .iter()
            .map(|dir| root.join(dir))
            .filter(|path| path.join("index.json").is_file())
            .collect();
        if paths.is_empty() { None } else { Some(paths) }
    }

    /// `library-map.json` 里该 stem 的包前缀（且包集合里确实有内容）。
    fn map_prefix(&self, stem: &str) -> Option<mir_asset::PackSet> {
        let map = self.map.as_ref()?;
        let packs = self.packs.as_ref()?;
        let set = packs.filter_prefix(map.prefix(stem)?);
        if set.is_empty() { None } else { Some(set) }
    }

    /// 文件名 → 实际路径，含本项目素材的兼容回退。
    ///
    /// * `items` → `dnitems`（两者都是 Crystal `Libraries.FloorItems` 的扁平对应物）；
    /// * `horse_x` → `horse` → `mount`（不同扁平包把坐骑叫不同名字）；
    /// * `wingeffect_x` → `wingeffect` → `wings`。
    fn path_for_stem(&self, stem: &str) -> Option<&PathBuf> {
        let fallback: &[&str] = match stem {
            "items" => &["dnitems"],
            "horse_x" => &["horse", "mount"],
            "wingeffect_x" => &["wingeffect", "wings"],
            _ => &[],
        };
        self.files
            .get(stem)
            .or_else(|| fallback.iter().find_map(|name| self.files.get(*name)))
    }

    /// 已尝试打开（成功或失败）的图库数量。
    #[must_use]
    pub fn opened_count(&self) -> usize {
        self.opened.len()
    }

    /// 取一个图库；文件不存在或打开失败返回 `None`。
    fn library(&mut self, stem: &str) -> Option<&LibrarySource> {
        if !self.opened.contains_key(stem) {
            let opened = self
                .map_dirs(stem)
                .and_then(|paths| match LibrarySource::from_dirs(&paths) {
                    Ok(library) => Some(library),
                    Err(error) => {
                        eprintln!("警告：实体拆包图库 {stem} 打开失败：{error}");
                        None
                    }
                })
                .or_else(|| self.map_prefix(stem).map(LibrarySource::Pack))
                .or_else(|| {
                    #[cfg(not(feature = "legacy"))]
                    {
                        let _ = stem;
                        None
                    }
                    #[cfg(feature = "legacy")]
                    {
                        if !self.allow_legacy {
                            return None;
                        }
                        self.path_for_stem(stem).cloned().and_then(|path| {
                            match LibrarySource::open_with_gfx(&path, self.gfx_dir.as_deref()) {
                                Ok(library) => Some(library),
                                Err(error) => {
                                    eprintln!(
                                        "警告：实体图库 {} 打开失败：{error}",
                                        path.display()
                                    );
                                    None
                                }
                            }
                        })
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
    /// 解码缓存命中次数。
    cache_hits: u64,
    /// 解码缓存未命中次数（真正解码）。
    cache_misses: u64,
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
            cache_hits: 0,
            cache_misses: 0,
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
        self.render_at_offset(frame, viewport, center, entities, now_ms, (0, 0));
    }

    /// 与 [`Self::render_at`] 相同，但把整个实体层按 `offset` 像素平移。
    ///
    /// Crystal 的相机跟随不是移动中心格，而是把玩家当前移动的
    /// `OffSetMove` 加到所有绘制坐标上（`GameScene.cs` 各 `DrawLocation.Offset(User.OffSetMove)`），
    /// 本参数就是那个偏移。地形层用同一偏移时，玩家相对屏幕保持居中。
    /// 每个实体自己的 [`EntityView::pixel_offset`] 也在这里叠加：自机用
    /// `-offset` 抵消全局偏移，仍画在屏幕中心（`PlayerObject.cs:847-854`）。
    pub fn render_at_offset(
        &mut self,
        frame: &mut Frame,
        viewport: Viewport,
        center: Point,
        entities: &[EntityView],
        now_ms: u64,
        offset: (i32, i32),
    ) {
        // 按排序 Y（默认 position.y；走/跑对象为 Crystal DrawY 的 max(from.y,to.y)）
        // 升序、同一行按 X 升序；`sort_by_key` 是稳定排序，同格实体保持输入顺序。
        let mut order: Vec<usize> = (0..entities.len()).collect();
        order.sort_by_key(|&index| (entities[index].sort_y, entities[index].position.x));
        for index in order {
            let entity = &entities[index];
            let (origin_x, origin_y) = geometry::world_to_screen(viewport, center, entity.position);
            let origin_x = origin_x + offset.0 + entity.pixel_offset.0;
            let origin_y = origin_y + offset.1 + entity.pixel_offset.1;
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
            if self.ensure_image(library, index) {
                let Some(info) = self.cached_info(library, index) else {
                    continue;
                };
                let (x, y) = if item {
                    (
                        origin_x + (CELL_WIDTH - i32::from(info.width)) / 2,
                        origin_y + (CELL_HEIGHT - i32::from(info.height)) / 2,
                    )
                } else {
                    (
                        origin_x + i32::from(info.offset_x),
                        origin_y + i32::from(info.offset_y),
                    )
                };
                self.blit_cached(frame, library, index, x, y);
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
                if self.ensure_image(item.library, index) {
                    let Some(info) = self.cached_info(item.library, index) else {
                        continue;
                    };
                    let x = origin_x + i32::from(info.offset_x);
                    let y = origin_y + i32::from(info.offset_y);
                    self.blit_cached(frame, item.library, index, x, y);
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
        self.draw_attack_swing(frame, entity, origin_x, origin_y, now_ms);
        top
    }

    /// 攻击挥砍光效：`effect.wzl` 的 3 张光效按挥砍进度播放。
    ///
    /// Crystal 的武器光效来自每把武器自己的 `CWeaponEffect` 库（`PlayerObject.cs:419`），
    /// 扁平语料没有这份映射；这里用通用 `effect.wzl` 第 0~2 帧代替，缺文件/缺帧
    /// 时静默跳过（[`Self::effect_image`] 返回 `None`）。
    fn draw_attack_swing(
        &mut self,
        frame: &mut Frame,
        entity: &EntityView,
        origin_x: i32,
        origin_y: i32,
        now_ms: u64,
    ) {
        if entity.kind != EntityKind::Player || entity.action != ActorAction::Attacking {
            return;
        }
        let animation = ActorAnimation::new(
            ActorAction::Attacking,
            entity.direction,
            BodyKind::Human,
            entity.action_started_ms,
        );
        let choice = animation.frame_at(now_ms);
        let sequence = animation.frames;
        let local = choice
            .index
            .saturating_sub(sequence.start)
            .saturating_sub(u16::from(entity.direction as u8) * sequence.stride());
        // 6 帧挥砍映射到 3 张光效：0~1 起手、2~3 挥出、4~5 收招。
        let effect_index = match local {
            0 | 1 => 0,
            2 | 3 => 1,
            _ => 2,
        };
        let Some(image) = self.effect_image(FxKind::Swing, effect_index) else {
            return;
        };
        let (dx, dy) = swing_offset(entity.direction);
        frame.blit(
            &image,
            origin_x + dx + i32::from(image.info.offset_x),
            origin_y + dy + i32::from(image.info.offset_y),
        );
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

    /// 取一张特效精灵（`effect.wzl` / `magic.wzl`）。
    ///
    /// 缺文件/缺帧时返回 `None`（调用方静默跳过，不画占位方块）。
    pub fn effect_image(&mut self, kind: FxKind, index: u32) -> Option<Image> {
        self.image(kind.library(), index + kind.start_frame())
    }

    /// 特效帧尺寸（必要时解码进缓存，但不克隆像素）。
    pub fn effect_size(&mut self, kind: FxKind, index: u32) -> Option<(u32, u32)> {
        let library = kind.library();
        let frame_index = index + kind.start_frame();
        if !self.ensure_image(library, frame_index) {
            return None;
        }
        let info = self.cached_info(library, frame_index)?;
        Some((info.width.into(), info.height.into()))
    }

    /// 直接把特效画到帧上（零拷贝）。
    pub fn draw_effect(
        &mut self,
        frame: &mut Frame,
        kind: FxKind,
        index: u32,
        x: i32,
        y: i32,
    ) -> bool {
        let library = kind.library();
        let frame_index = index + kind.start_frame();
        if !self.ensure_image(library, frame_index) {
            return false;
        }
        self.blit_cached(frame, library, frame_index, x, y);
        true
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
            // `.wzl` 的 8 位调色板与 RGB565 都没有 alpha；纯黑是「空像素」，
            // 必须统一置透明，否则头发/武器/坐骑会在角色周围留下黑框
            // （Crystal 的 `.Lib` 在写入时已做过 removeBlack，`MLibraryV2.cs:419-426`）。
            remove_black_all(&mut image);
            image
        });
        self.cache.insert(key, image.clone());
        image
    }

    /// 确保 `(图库, 帧号)` 已解码进缓存；只返回是否存在，不克隆图像。
    ///
    /// 每帧画实体走这条路径：命中时零拷贝，避免把整张 RGBA 图从缓存复制出来
    /// （100 个玩家 × 4 层/帧的克隆曾是实体绘制的主要开销）。
    fn ensure_image(&mut self, library: SpriteLibrary, index: u32) -> bool {
        let key = (library, index);
        if self.cache.contains_key(&key) {
            self.cache_hits = self.cache_hits.saturating_add(1);
            return matches!(self.cache.get(&key), Some(Some(_)));
        }
        self.cache_misses = self.cache_misses.saturating_add(1);
        if self.cache.len() >= MAX_ENTITY_CACHED_IMAGES {
            self.cache.clear();
        }
        let image = self.libraries.image(library, index).map(|mut image| {
            remove_black_all(&mut image);
            image
        });
        self.cache.insert(key, image);
        matches!(self.cache.get(&key), Some(Some(_)))
    }

    /// 缓存里已解码图像的头信息（用于定位，不克隆像素）。
    fn cached_info(&self, library: SpriteLibrary, index: u32) -> Option<ImageInfo> {
        self.cache
            .get(&(library, index))
            .and_then(Option::as_ref)
            .map(|image| image.info)
    }

    /// 把缓存里的图直接贴到帧上（零拷贝）。
    fn blit_cached(&self, frame: &mut Frame, library: SpriteLibrary, index: u32, x: i32, y: i32) {
        if let Some(Some(image)) = self.cache.get(&(library, index)) {
            frame.blit(image, x, y);
        }
    }

    /// 实体图库缓存统计：`(命中, 未命中)`（命中 = 该帧已在缓存里）。
    #[must_use]
    pub const fn cache_stats(&self) -> (u64, u64) {
        (self.cache_hits, self.cache_misses)
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
/// 攻击光效相对格原点的位移：按朝向把光效挪到武器挥出的那一侧。
fn swing_offset(direction: MirDirection) -> (i32, i32) {
    match direction {
        MirDirection::Up => (0, -20),
        MirDirection::UpRight => (16, -14),
        MirDirection::Right => (24, 0),
        MirDirection::DownRight => (16, 10),
        MirDirection::Down => (0, 12),
        MirDirection::DownLeft => (-16, 10),
        MirDirection::Left => (-24, 0),
        MirDirection::UpLeft => (-16, -14),
    }
}

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

/// 画头顶血条：黑底 + 按百分比宽度的前景（Crystal `MapObject.DrawHealth`，
/// `Client/MirObjects/MapObject.cs:465-507`）。
///
/// * 前景宽 `32 × PercentHealth / 100`、高 4（`:506`），左边界在格原点 + 8（`:478`）；
/// * Crystal 的前景是 `Prguse2` 的 4 张预着色图（1 = 怪物、10 = 队友玩家、
///   11 = 队友的宠物、12 = 其他玩家/自己，`:481-504`）；本作没有那套 UI 图库，改为
///   按血量百分比着色（[`HEALTH_GREEN`] / [`HEALTH_YELLOW`] / [`HEALTH_RED`]），
///   死亡用 [`HEALTH_DEAD`]（Crystal 死亡时不画，`:470`；这里保留一条灰条以提示尸体）。
/// * 纵向位置：Crystal 是 `DisplayRectangle.Y - 64`（`:506`），本作的精灵顶边由
///   [`EntityRenderer::draw_entity`] 返回，血条画在它上方 6 像素（简化：64 像素是按
///   16 位色时代的格子高度算的，照搬到本作会飘到名字上面）。
fn draw_health_bar(frame: &mut Frame, origin_x: i32, y: i32, percent: u8, dead: bool) {
    let left = origin_x + HEALTH_BAR_OFFSET_X;
    let bar_width = u32::try_from(HEALTH_BAR_WIDTH).unwrap_or(0);
    frame.fill_rect(
        left - 1,
        y - 1,
        bar_width + 2,
        HEALTH_BAR_HEIGHT + 2,
        HEALTH_BACKGROUND,
    );
    let color = if dead {
        HEALTH_DEAD
    } else if percent > 50 {
        HEALTH_GREEN
    } else if percent > 25 {
        HEALTH_YELLOW
    } else {
        HEALTH_RED
    };
    let filled = u32::from(percent.min(100)) * bar_width / 100;
    frame.fill_rect(left, y, filled, HEALTH_BAR_HEIGHT, color);
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
    #![allow(dead_code)]
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
    /// 坐骑颜色（黄）与翅膀颜色（紫）。
    const MOUNT_COLOR: [u8; 3] = [230, 200, 40];
    const WING_COLOR: [u8; 3] = [200, 60, 220];
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
        // 坐骑：`horse_x.wzl` 块 0，站立 4 帧/方向；70×70 比身体宽，左右两侧露出来，
        // 身体覆盖的部分用来验证「坐骑画在身体之下」。
        let mount: Vec<SyntheticFrame> = (0..8u32)
            .flat_map(|direction| {
                (0..4u32).map(move |frame| SyntheticFrame {
                    index: direction * PLAYER_DIRECTION_STRIDE + frame,
                    width: 70,
                    height: 70,
                    offset_x: -15,
                    offset_y: -40,
                    color: MOUNT_COLOR,
                })
            })
            .collect();
        write_library(&dir, "horse_x", &mount);
        // 翅膀：`wingeffect_x.wzl` 块 0，站立 8 帧/方向（`Frames.cs:157` 的 effect 段）；
        // 压在头顶与身体上沿，用来验证「翅膀画在头发之后」。
        let wing: Vec<SyntheticFrame> = (0..8u32)
            .flat_map(|direction| {
                (0..8u32).map(move |frame| SyntheticFrame {
                    index: direction * PLAYER_DIRECTION_STRIDE + frame,
                    width: 60,
                    height: 20,
                    offset_x: 0,
                    offset_y: -70,
                    color: WING_COLOR,
                })
            })
            .collect();
        write_library(&dir, "wingeffect_x", &wing);
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
    #[cfg(feature = "legacy")]
    fn equipment_layers_change_the_rendered_frame() {
        let dir = synthetic_assets();
        let mut renderer = EntityRenderer::new(EntityLibraries::open_dir(&dir.0));
        assert_eq!(renderer.libraries().file_count(), 6);

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
    #[cfg(feature = "legacy")]
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
    #[cfg(feature = "legacy")]
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
    #[cfg(feature = "legacy")]
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

    fn mounted_player(direction: MirDirection) -> EntityView {
        let mut player = equipped_player(direction);
        player.mount = 1;
        player.wings = 1;
        player
    }

    #[test]
    fn mount_and_wings_add_layers_and_the_mount_hides_the_weapon() {
        // 朝下：坐骑 → 身体 → 头发 → 翅膀（武器在骑乘时不画，PlayerObject.cs:4886,4914）。
        let player = mounted_player(MirDirection::Down);
        let plan = player_layer_plan_at(&player, 0);
        assert_eq!(
            plan.layers()
                .iter()
                .map(|item| item.layer)
                .collect::<Vec<_>>(),
            vec![
                PlayerLayer::Mount,
                PlayerLayer::Body,
                PlayerLayer::Hair,
                PlayerLayer::Wing
            ]
        );
        let mount = plan.layers()[0];
        assert_eq!(mount.library, SpriteLibrary::Mount);
        assert_eq!(mount.frame, 4 * 8, "朝下 = 方向 4，站立第 0 帧");
        assert_eq!(mount.block_start(), 0);
        let wing = plan.layers()[3];
        assert_eq!(wing.library, SpriteLibrary::Wing);
        assert_eq!(wing.frame, 4 * 8, "翅膀朝下、站立第 0 帧");
        assert_eq!(wing.block_start(), 0);

        // 朝上：武器本来在身体之前，骑乘时同样不画。
        let mut up = mounted_player(MirDirection::Up);
        up.weapon = 3;
        up.weapon_shape = 3;
        let plan = player_layer_plan_at(&up, 0);
        assert!(!plan.contains(PlayerLayer::Weapon), "骑乘时不画武器");
        assert_eq!(plan.layers()[0].layer, PlayerLayer::Mount);
        assert_eq!(plan.layers()[2].layer, PlayerLayer::Hair);
        assert_eq!(plan.layers()[3].layer, PlayerLayer::Wing);

        // WingEffect >= 100 是其它特效，不是翅膀层（PlayerObject.cs:577,650,5077）。
        let mut special = mounted_player(MirDirection::Down);
        special.wings = 100;
        assert!(!player_layer_plan_at(&special, 0).contains(PlayerLayer::Wing));
    }

    #[test]
    #[cfg(feature = "legacy")]
    fn mount_and_wings_change_the_rendered_frame() {
        let dir = synthetic_assets();
        let mut renderer = EntityRenderer::new(EntityLibraries::open_dir(&dir.0));
        let plain = equipped_player(MirDirection::Down);
        let mounted = mounted_player(MirDirection::Down);

        let (plain_frame, _, _) = render_one(&mut renderer, &plain);
        let (mounted_frame, origin_x, origin_y) = render_one(&mut renderer, &mounted);
        assert!(
            differing_pixels(&plain_frame, &mounted_frame) > 200,
            "带坐骑/翅膀的帧必须与没有时不同（实际差异 {} 像素）",
            differing_pixels(&plain_frame, &mounted_frame)
        );
        assert!(has_color(&mounted_frame, MOUNT_COLOR), "坐骑像素应出现");
        assert!(has_color(&mounted_frame, WING_COLOR), "翅膀像素应出现");
        // 身体盖住坐骑中部：坐骑在身体之下。
        let overlap = mounted_frame
            .pixel(
                u32::try_from(origin_x + OVERLAP.0).expect("x"),
                u32::try_from(origin_y + OVERLAP.1).expect("y"),
            )
            .expect("重叠点应在帧内");
        assert_eq!(&overlap[..3], &BODY_COLOR, "坐骑必须画在身体之下");
        // 身体右侧露出的坐骑部分仍是坐骑色。
        let exposed = mounted_frame
            .pixel(
                u32::try_from(origin_x + 48).expect("x"),
                u32::try_from(origin_y - 20).expect("y"),
            )
            .expect("坐骑应在帧内");
        assert_eq!(&exposed[..3], &MOUNT_COLOR, "身体之外应看到坐骑");
        // 翅膀画在头发之后：两者重叠处是翅膀色（朝下时武器不画，不会盖住翅膀）。
        let wing_over_hair = mounted_frame
            .pixel(
                u32::try_from(origin_x + 10).expect("x"),
                u32::try_from(origin_y - 60).expect("y"),
            )
            .expect("翅膀应在帧内");
        assert_eq!(&wing_over_hair[..3], &WING_COLOR, "翅膀必须画在头发之后");
    }

    #[test]
    fn mount_tables_follow_frames_and_the_flat_horse_layout() {
        // `Frames.cs:188-192` 的 Count/Interval；Start 是扁平 horse_x.wzl 的块内偏移。
        assert_eq!(MOUNT_STANDING, FrameSequence::new(0, 4, 4, 500));
        assert_eq!(MOUNT_WALKING.count, 6, "扁平素材行走 6 帧");
        assert_eq!(MOUNT_WALKING.start, 64);
        assert_eq!(MOUNT_RUNNING.start, 128);
        assert_eq!(MOUNT_STRUCK.start, 192);
        assert_eq!(MOUNT_ATTACKING.start, 384);
        assert_eq!(mount_frames(ActorAction::Walking), MOUNT_WALKING);
        assert_eq!(mount_frames(ActorAction::Dead), MOUNT_DEAD);
    }

    #[test]
    fn mount_and_wing_blocks_use_their_own_frame_sizes() {
        assert_eq!(MOUNT_FRAMES_PER_APPEARANCE, 640);
        assert_eq!(WING_FRAMES_PER_APPEARANCE, 600);
        assert_eq!(HORSE_X_BLOCKS, 3);
        assert_eq!(WING_BLOCKS, 2);
        let mount = LayerFrame {
            layer: PlayerLayer::Mount,
            library: SpriteLibrary::Mount,
            frame: 2 * MOUNT_FRAMES_PER_APPEARANCE + 32,
        };
        assert_eq!(mount.block_start(), 1280);
        let wing = LayerFrame {
            layer: PlayerLayer::Wing,
            library: SpriteLibrary::Wing,
            frame: WING_FRAMES_PER_APPEARANCE + 64,
        };
        assert_eq!(wing.block_start(), 600);
        assert_eq!(SpriteLibrary::Mount.stem(), "horse_x");
        assert_eq!(SpriteLibrary::Wing.stem(), "wingeffect_x");
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
    fn render_at_offset_composes_global_and_entity_pixel_offsets() {
        let mut renderer = EntityRenderer::empty();
        let viewport = Viewport::new(200, 200);
        let center = Point::new(2, 3);
        let mut frame = Frame::new(200, 200);
        frame.clear([0, 0, 0, 255]);
        // 同格一玩家一怪物：怪物带 +48px 自身偏移，应与玩家占位块正好错开一格。
        let entities = [
            EntityView::new(center, MirDirection::Down, EntityKind::Player, 0),
            EntityView::new(center, MirDirection::Down, EntityKind::Monster, 5)
                .with_pixel_offset((48, 0)),
        ];
        let offset = (5, -3);
        renderer.render_at_offset(&mut frame, viewport, center, &entities, 0, offset);
        let (origin_x, origin_y) = geometry::world_to_screen(viewport, center, center);
        let player_left = origin_x + (CELL_WIDTH - 12) / 2 + offset.0;
        let player_top = origin_y + (CELL_HEIGHT - 18) / 2 + offset.1;
        let pixel = |x: i32, y: i32| -> [u8; 4] {
            let index = (usize::try_from(y).unwrap() * frame.width as usize
                + usize::try_from(x).unwrap())
                * 4;
            frame.pixels[index..index + 4].try_into().unwrap()
        };
        assert_eq!(
            pixel(player_left + 6, player_top + 9),
            EntityKind::Player.color(),
            "玩家占位块 = 格原点 + 全局偏移"
        );
        assert_eq!(
            pixel(player_left + 48 + 6, player_top + 9),
            EntityKind::Monster.color(),
            "怪物占位块 = 格原点 + 全局偏移 + 自身像素偏移"
        );
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

    #[test]
    #[cfg(feature = "legacy")]
    fn effect_libraries_are_lazy_and_missing_effects_are_silent() {
        let dir = TempDir::new("fx-lazy");
        write_library(
            &dir,
            "magic2",
            &[SyntheticFrame {
                index: 0,
                width: 12,
                height: 9,
                offset_x: -3,
                offset_y: -8,
                color: [255, 80, 20],
            }],
        );
        let mut renderer = EntityRenderer::new(EntityLibraries::open_dir(&dir.0));
        assert!(renderer.libraries().has(SpriteLibrary::Magic2));
        let image = renderer
            .effect_image(FxKind::Swing, 0)
            .expect("magic2.wzl 第 0 帧应能解码");
        assert_eq!((image.info.width, image.info.height), (12, 9));
        // 缺 `magic.wzl`：返回 None（不 panic、不画占位方块）。
        assert!(renderer.effect_image(FxKind::Magic, 0).is_none());
        // 缺帧同理。
        assert!(renderer.effect_image(FxKind::Swing, 99).is_none());
    }

    #[test]
    #[cfg(feature = "legacy")]
    fn attacking_player_draws_the_swing_effect_sprite() {
        let dir = TempDir::new("fx-swing");
        write_library(
            &dir,
            "hum",
            &[SyntheticFrame {
                index: 0,
                width: 20,
                height: 30,
                offset_x: 0,
                offset_y: -20,
                color: BODY_COLOR,
            }],
        );
        let frames: Vec<SyntheticFrame> = (0..3)
            .map(|index| SyntheticFrame {
                index,
                width: 12,
                height: 12,
                offset_x: 0,
                offset_y: -10,
                color: [0, 255, 0],
            })
            .collect();
        write_library(&dir, "magic2", &frames);
        let mut renderer = EntityRenderer::new(EntityLibraries::open_dir(&dir.0));
        let mut attacking = view(EntityKind::Player, 0);
        attacking.action = ActorAction::Attacking;
        attacking.action_started_ms = 0;
        attacking.direction = MirDirection::Down;
        let mut frame = Frame::new(200, 200);
        frame.clear([0, 0, 0, 255]);
        renderer.render(
            &mut frame,
            Viewport::new(200, 200),
            Point::new(2, 3),
            &[attacking.clone()],
            10,
        );
        let swing_pixels = frame
            .pixels
            .chunks_exact(4)
            .filter(|pixel| pixel[0] == 0 && pixel[1] == 255 && pixel[2] == 0)
            .count();
        assert!(swing_pixels > 0, "攻击时应叠一层挥砍光效");

        // 站着不画光效。
        let mut standing = attacking.clone();
        standing.action = ActorAction::Standing;
        let mut still = Frame::new(200, 200);
        still.clear([0, 0, 0, 255]);
        renderer.render(
            &mut still,
            Viewport::new(200, 200),
            Point::new(2, 3),
            &[standing],
            10,
        );
        assert!(
            !still
                .pixels
                .chunks_exact(4)
                .any(|pixel| pixel[0] == 0 && pixel[1] == 255 && pixel[2] == 0),
            "站立时不应画挥砍光效"
        );
    }
}
