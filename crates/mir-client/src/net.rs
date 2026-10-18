//! 客户端网络会话：连接 Crystal 服务端、走完登录链路、维护世界快照。
//!
//! # 为什么用阻塞 socket + 线程
//!
//! `mir-client` 的窗口是 winit 的单线程事件循环，网络只需要一个后台线程：
//! [`std::net::TcpStream`] 设 20 ms 读超时，线程每轮先处理 UI 通过 channel 发来的
//! [`NetCommand`]，再读一批服务器包；UI 侧用 [`NetClient::try_recv`] 取最新快照。
//! 这样没有 tokio runtime 也能跑，端到端测试同样可以直接驱动（见
//! `crates/mir-client/tests/live_world.rs`）。`mir-client` 的 `tokio` 依赖只用于测试里
//! 在进程内启动真实服务端。
//!
//! # 登录包序（与 `mir-server/src/session.rs` 对齐）
//!
//! ```text
//! TCP 连接
//!   ← S.Connected
//!   → C.ClientVersion { 16 字节占位哈希 }      // 服务端 check_version=false
//!   ← S.ClientVersion { result = 1 }
//!   → C.Login { account, password }
//!   ← S.Login { result = 3 }                   // 账号不存在
//!   → C.NewAccount { ...空资料... }            // 默认自动注册，--no-register 可关
//!   ← S.NewAccount { result = 8 }
//!   → C.Login { account, password }
//!   ← S.LoginSuccess { characters }
//!   → C.StartGame { character_index }          // 角色不存在时先 C.NewCharacter
//!   ← S.NewCharacterSuccess / S.NewCharacter
//!   ← S.StartGame { result = 4 }
//!   ← S.MapInformation
//!   ← S.NewMapInfo（服务端有数据库时）
//!   ← S.UserInformation（自己的对象 ID / 坐标 / HP/MP）
//! ```
//!
//! 详细语义见 `docs/protocol/login-flow.md`；这个模块只实现客户端侧的状态机，
//! 遇到任何阶段错误都通过 [`NetEvent::Error`] 抛出，不静默卡住。
//!
//! # 快照
//!
//! [`WorldSnapshot`] 是渲染层的唯一输入：它由 `ServerPacket` 增量更新，客户端每批
//! 包处理完把整个快照克隆给 UI。快照里的 `packet_counts` 记录收到过的包种类，
//! 端到端测试用它断言 `UserInformation`、`ObjectMonster`、`UserLocation` 都到了。
//!
//! # 界面消费的包
//!
//! | 包 | 快照字段 / 用途 |
//! |---|---|
//! | `S.UserInformation` | 自己、`inventory`、`magics`、金币、HP/MP、纸娃娃外观 |
//! | `S.ObjectPlayer` / `S.PlayerUpdate` | 其他玩家的坐标/朝向/外观（`PlayerLooks`） |
//! | `S.NewItemInfo` | `item_infos`：物品名/图标号/**外观号 `shape`**/价格/堆叠上限/类型/需求/属性表 |
//! | `S.GainedItem` | 并入背包（可堆叠物品先并堆）；系统聊天 |
//! | `S.MoveItem` | 背包内交换（`grid = Inventory` 且 `success`） |
//! | `S.EquipItem` / `S.RemoveItem` | 背包 ↔ 装备槽（按唯一编号搬运） |
//! | `S.DeleteItem` | 按数量移除背包物品 |
//! | `S.RefreshItem` | 用新数据替换同 `unique_id` 的物品（Crystal 的 `ItemChanged`） |
//! | `S.ItemRepaired` | 更新持久度 |
//! | `S.AddBuff` / `S.RemoveBuff` | `buffs`：图标栏数据（`ExpireTime` 由 UI 本地倒计时） |
//! | `S.ObjectStruck` / `S.DamageIndicator` | `damage_events`：世界飘字（`ui::worldtext`） |
//! | `S.ObjectEffect` | `effects`：魔法盾/元素结界等短光效 |
//! | `S.NPCResponse` | `npc_dialog`：原文行 + 版本号 + 最近 NPC 的 `object_id` |
//! | `S.NPCGoods` | `npc_goods`：货架 + 价格倍率（物品名/价格来自 `item_infos`） |
//! | `S.NPCSell` / `S.NPCRepair` / `S.NPCSRepair` | 商店面板的「可出售/可修理」提示 |
//! | `S.NewMagic` / `S.MagicLeveled` / `S.RemoveMagic` | `magics` |
//! | `S.GainedGold` / `S.LoseGold` | 金币 |
//! | `S.Chat` / `S.ObjectChat` | 聊天面板 |
//! | `S.MapInformation` / `S.NewMapInfo` / `S.MapChanged` | 地图名与 NPC 列表 |
//!
//! # 与 Crystal 客户端的已知差异
//!
//! * **没有 `ItemChanged` / `LostItem` 这两条包**：Crystal 里对应 `S.RefreshItem`
//!   与 `S.DeleteItem`，这里按它们实现。
//! * **换下的旧装备不回包**：`S.EquipItem` 只带被穿上的物品 ID 和目标槽，换下来的旧装备
//!   数据不在包里；客户端用本地背包里那件物品的数据补进装备槽，并把旧装备放回原背包格
//!   （与 Crystal `PlayerObject.EquipItem` 的 `:5674-5678` 语义一致）。
//! * **`S.NPCResponse` 不带 NPC 对象 ID**：客户端取离自己最近的 NPC 作为对话对象
//!   （[`WorldSnapshot::nearest_npc`]）；世界点击 NPC 时会直接记下被点的对象。
//! * **最大 HP/MP 不在协议里**：状态栏进度条用「本次会话见过的最高值」近似，
//!   数字显示为 `当前/高水位`，见 [`SelfPlayer::hp_max`]。
//! * **自己的外观要客户端算**：`S.UserInformation` 只给发型，衣服/武器外观来自装备槽里
//!   物品模板的 `Shape`（Crystal 服务器算好再塞进 `S.ObjectPlayer`，见
//!   `Client/MirObjects/UserObject.cs:205-250`）。所以快照在**每个包之后**重算一次
//!   [`PlayerLooks`]，让「装备先到、`S.NewItemInfo` 后到」也能收敛。

use std::collections::BTreeMap;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use mir_core::Point;
use mir_core::enums::{
    BuffType, ChatType, DamageType, ItemGrade, ItemType, MirClass, MirDirection, MirGender,
    MirGridType, PanelType, RequiredType, Spell, SpellEffect, Stat,
};
use mir_protocol::codec::FrameBuffer;
use mir_protocol::data::{ClientBuff, ClientMagic, ItemInfo, UserItem};
use mir_protocol::packets::client as c;
use mir_protocol::{ClientPacket, DateTimeBinary, ServerPacket};
use mir_render::animation::{ActorAction, BodyKind};
use mir_render::{EntityKind, EntityView};

/// 连接超时。
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// 单次 `read` 的超时：决定 UI 命令（走路/聊天）的最大延迟。
const READ_TIMEOUT: Duration = Duration::from_millis(20);
/// `KeepAlive` 间隔（Crystal 客户端在没有待发包时定期发送）。
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(5);
/// 聊天记录上限。
const CHAT_LOG_LIMIT: usize = 100;
/// 装备槽数量（Crystal `EquipmentSlot` 共 14 个：武器/衣服/头盔/火把/项链/双腕/双戒/
/// 护身符/腰带/靴子/宝石/坐骑）。
pub const EQUIPMENT_SLOTS: usize = 14;
/// 快照里保留的伤害事件条数：UI 每帧取「比上次更新」的那些画飘字。
const DAMAGE_EVENT_LIMIT: usize = 32;
/// 快照里保留的光效事件条数。
const EFFECT_EVENT_LIMIT: usize = 16;
/// 客户端版本哈希的占位值：Crystal 发的是客户端 exe 的 MD5（16 字节），
/// 服务端 `check_version = false` 时内容不重要，但长度保持 16 字节。
const VERSION_HASH: &[u8] = b"mir-next-client!";

/// 网络会话阶段。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NetStatus {
    /// 后台线程正在连接。
    #[default]
    Connecting,
    /// 已连上，等版本校验。
    Verifying,
    /// 正在登录/注册。
    LoggingIn,
    /// 已登录，正在选角/建角。
    SelectingCharacter,
    /// 正在进入世界。
    StartingGame,
    /// 已进入世界（`StartGame{4}` 已收到）。
    InGame,
    /// 服务器或本地关闭了连接。
    Closed,
    /// 登录/连接失败。
    Failed,
}

impl NetStatus {
    /// 可读名称（日志/叠层用）。
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Connecting => "连接中",
            Self::Verifying => "版本校验",
            Self::LoggingIn => "登录中",
            Self::SelectingCharacter => "选择角色",
            Self::StartingGame => "进入世界",
            Self::InGame => "已进入世界",
            Self::Closed => "已断开",
            Self::Failed => "失败",
        }
    }
}

/// 创建角色的参数（对应 `--create-character --class --gender`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateCharacter {
    /// 角色名。
    pub name: String,
    /// 职业。
    pub class: MirClass,
    /// 性别。
    pub gender: MirGender,
}

/// 连接参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectOptions {
    /// 服务器地址（`host:port`，默认 `127.0.0.1:7000`）。
    pub server: String,
    /// 账号。
    pub account: String,
    /// 口令。
    pub password: String,
    /// 角色名。
    pub character: String,
    /// 角色不存在时是否自动创建。
    pub create: Option<CreateCharacter>,
    /// 账号不存在时是否自动注册（默认 `true`；`--no-register` 关闭）。
    pub auto_register: bool,
}

/// UI → 网络线程的命令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetCommand {
    /// 转身（`C.Turn`）。
    Turn(MirDirection),
    /// 走一步（`C.Walk`）。
    Walk(MirDirection),
    /// 跑两步（`C.Run`）。
    Run(MirDirection),
    /// 物理攻击（`C.Attack`）。
    Attack(MirDirection),
    /// 捡起脚下物品（`C.PickUp`）。
    PickUp,
    /// 聊天（`C.Chat`）。
    Chat(String),
    /// 点 NPC 按钮（`C.CallNPC`）；`key = "@exit"` 关闭对话。
    CallNpc {
        /// NPC 对象 ID。
        object_id: u32,
        /// 脚本页键（含 `@`）。
        key: String,
    },
    /// 买一件商品（`C.BuyItem`）；`unique_id` 是商店物品的唯一编号。
    BuyItem {
        /// 商店物品唯一编号。
        unique_id: u64,
        /// 数量。
        count: u16,
    },
    /// 卖背包物品（`C.SellItem`）。
    SellItem {
        /// 背包物品唯一编号。
        unique_id: u64,
        /// 数量。
        count: u16,
    },
    /// 修理背包物品（`C.RepairItem`）。
    RepairItem {
        /// 背包物品唯一编号。
        unique_id: u64,
    },
    /// 丢弃背包物品（`C.DropItem`）。
    DropItem {
        /// 背包物品唯一编号。
        unique_id: u64,
        /// 数量。
        count: u16,
    },
    /// 使用背包物品（`C.UseItem`；药品/卷轴/技能书）。
    UseItem {
        /// 背包物品唯一编号。
        unique_id: u64,
    },
    /// 拖拽物品（`C.MoveItem`）。
    ///
    /// `grid` 是**拖拽起点**网格、`to_grid` 是落点网格：背包内交换时两者都是
    /// `Inventory`；背包 → 装备槽是 `Inventory` → `Equipment`；装备槽 → 背包反过来。
    /// 网络层把「任一侧是 `Equipment`」的请求统一发成 `C.MoveItem { grid: Equipment }`
    /// （`mir-server` 的 `World::move_equipment` 用 `from`/`to` 的方向判断穿/脱）。
    MoveItem {
        /// 起点网格。
        grid: MirGridType,
        /// 起点格（背包格或装备槽）。
        from: i32,
        /// 落点格（背包格或装备槽）。
        to: i32,
        /// 落点网格。
        to_grid: MirGridType,
    },
    /// 施法（`C.Magic`）。
    Magic {
        /// 技能。
        spell: Spell,
        /// 朝向。
        direction: MirDirection,
        /// 目标对象 ID（0 表示无目标）。
        target_id: u32,
        /// 目标格。
        location: Point,
    },
    /// 重新绑定技能快捷键（`C.MagicKey`）。
    MagicKey {
        /// 技能。
        spell: Spell,
        /// 新按键（1..=9）。
        key: u8,
        /// 旧按键。
        old_key: u8,
    },
    /// 关闭连接。
    Quit,
}

impl NetCommand {
    /// 这条命令是否让**自己**立即切动作；返回 `(动作, 朝向)`。
    ///
    /// 走/跑/攻击/转身在 Crystal 客户端都是先本地播放动作再发包
    /// （`PlayerObject.cs:1313-1334,1356-1429`），本作保留这个顺序。
    #[must_use]
    pub const fn local_animation(&self) -> Option<(ActorAction, MirDirection)> {
        match self {
            Self::Turn(direction) => Some((ActorAction::Standing, *direction)),
            Self::Walk(direction) => Some((ActorAction::Walking, *direction)),
            Self::Run(direction) => Some((ActorAction::Running, *direction)),
            Self::Attack(direction) => Some((ActorAction::Attacking, *direction)),
            _ => None,
        }
    }
}

/// 网络线程 → UI 的通知。
#[derive(Debug, Clone)]
pub enum NetEvent {
    /// 世界快照更新（登录阶段也会发，`status` 在快照里）。
    ///
    /// 装箱是为了避免每个 `NetEvent`（包括错误/关闭）都被快照撑大。
    Snapshot(Box<WorldSnapshot>),
    /// 收到 `S.MapChanged`：服务端把角色切到了另一张图
    /// （Crystal `Shared/ServerPackets.cs:2979-3025`、客户端处理 `GameScene.cs:3970-4009`）。
    ///
    /// 本事件**排在**对应的 [`NetEvent::Snapshot`] 之后发出，所以 UI 处理时快照里已经是新地图
    /// （实体已清空、坐标已更新）。UI 用 [`WorldSnapshot::map_reload_request`] 决定要不要
    /// 重载本地地形。
    MapChanged {
        /// 新地图主键（`S.MapChanged.MapIndex`）。
        map_index: i32,
        /// 落点（`S.MapChanged.Location`），相机要重置到这里。
        position: Point,
    },
    /// 致命错误（随后还会有 `Closed`）。
    Error(String),
    /// 连接已关闭。
    Closed(String),
}

/// 地图信息（来自 `S.MapInformation` / `S.NewMapInfo` / `S.MapChanged`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MapSnapshot {
    /// 地图编号。
    pub map_index: i32,
    /// 地图文件名（不含 `.map`，对应 `resource/map/<name>.map`）。
    pub file_name: String,
    /// 地图标题。
    pub title: String,
    /// 宽（`NewMapInfo` 才有；`MapInformation` 里没有）。
    pub width: i32,
    /// 高。
    pub height: i32,
}

/// 客户端收到 `S.MapChanged` 后要做的切图动作：本地地形文件 + 新的相机中心。
///
/// Crystal 客户端在 `GameScene.MapChanged` 里把 `MapControl.FileName` 换成
/// `Path.Combine(Settings.MapPath, p.FileName + ".map")` 并 `LoadMap()`
/// （`Client/MirScenes/GameScene.cs:3974-3988`）；本作按同样的约定拼本地路径：
/// 地图目录（默认 `resource/map`）+ `<FileName>.map`。`FileName` 为空时退回用地图主键，
/// 也就是「地图号 == 文件名」的老约定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapReloadRequest {
    /// 新地图主键（`S.MapChanged.MapIndex`）。
    pub map_index: i32,
    /// 地图文件名（不含 `.map`）。
    pub file_name: String,
    /// 落点：相机与「自己」的位置都重置到这里。
    pub position: Point,
    /// 本地地形文件路径（可能不存在，调用方负责打印警告并保留旧图）。
    pub path: PathBuf,
}

/// 玩家外观（纸娃娃层）：身体/武器/头发/盾牌的图库外观号。
///
/// 与 Crystal `PlayerObject.Armour / Weapon / Hair` 一一对应，但拆成了「图库内块号」
/// 与「原始形状」两部分，因为本项目的素材是扁平 `.wzl`（几个外观塞在一个文件里）：
///
/// | 字段 | 含义 | 来源 | 图库 |
/// |---|---|---|---|
/// | [`Self::armour`] | 衣服外观号 | 装备槽 `Armour` 的 `ItemInfo.Shape` | `hum.wzl` / `hum2.wzl` / `hum3.wzl` |
/// | [`Self::weapon`] | 武器外观号 | 装备槽 `Weapon` 的 `ItemInfo.Shape % 100` | `weapon*.wzl` |
/// | [`Self::weapon_shape`] | 武器形状原始值 | 装备槽 `Weapon` 的 `ItemInfo.Shape` | 决定用哪一张 `weapon*.wzl` |
/// | [`Self::hair`] | 发型号 | `S.UserInformation.hair` / `S.ObjectPlayer.hair` | `hair*.wzl` |
/// | [`Self::shield`] | 盾牌外观号 | 本作没有盾牌物品类型，恒为 [`mir_render::NO_LAYER`] | `shield.wzl`（语料里没有） |
///
/// 武器为什么要两个字段：Crystal 的武器 `Shape` 分三段（0..=99 普通、100..=199 刺客、
/// 200.. 弓箭手，见 `Client/MirObjects/PlayerObject.cs:41-55,402-409`），扁平语料把三段
/// 分别放在 `weapon.wzl`、`weapon2.wzl`、`weapon3.wzl`。所以「原始值」决定图库，
/// 「块号」决定图库内的位置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerLooks {
    /// 职业（武器分段的判断依据，Crystal `PlayerObject.HasClassWeapon`）。
    pub class: MirClass,
    /// 性别（Crystal 用它加 `ArmourOffSet`；扁平语料把男女编成不同块号，这里只记录）。
    pub gender: MirGender,
    /// 发型号 → `hair*.wzl` 块号；[`mir_render::NO_LAYER`] 表示光头。
    pub hair: u16,
    /// 衣服外观号 → `hum*.wzl` 块号；`0` 是裸体（语料 `hum.wzl` 块 0）。
    pub armour: u16,
    /// 武器外观号 → `weapon*.wzl` 块号；[`mir_render::NO_LAYER`] 表示空手。
    pub weapon: u16,
    /// 武器形状原始值（含 100/200 段位）；[`mir_render::NO_LAYER`] 表示空手。
    pub weapon_shape: u16,
    /// 盾牌外观号；[`mir_render::NO_LAYER`] 表示没有盾牌。
    pub shield: u16,
}

impl Default for PlayerLooks {
    /// 缺省外观：裸体（`armour = 0`）、空手、无盾牌、光头。
    ///
    /// 与 Crystal 的缺省值一致：`Armour = 0`、`Weapon = -1`（`UserObject.cs:207-211`）。
    fn default() -> Self {
        Self {
            class: MirClass::Warrior,
            gender: MirGender::Male,
            hair: mir_render::NO_LAYER,
            armour: 0,
            weapon: mir_render::NO_LAYER,
            weapon_shape: mir_render::NO_LAYER,
            shield: mir_render::NO_LAYER,
        }
    }
}

impl PlayerLooks {
    /// 从装备槽与物品模板推出外观（Crystal `UserObject.RefreshEquipmentStats`，
    /// `Client/MirObjects/UserObject.cs:205-250`）。
    ///
    /// 规则（逐条对拍 Crystal）：
    ///
    /// 1. 只认 `ItemType::Armour` 与 `ItemType::Weapon` 两类；其余槽位不影响外观
    ///    （`:236-245`）；
    /// 2. 持久度为 0 的装备当作没穿（`if (temp.CurrentDura == 0 && realItem.Durability > 0) continue;`
    ///    `:234`）；
    /// 3. 没有模板（`S.NewItemInfo` 还没到）的槽位跳过，外观保持旧值。
    #[must_use]
    pub fn from_equipment(
        equipment: &[Option<InventoryItem>],
        items: &BTreeMap<i32, ItemInfoView>,
        class: MirClass,
        gender: MirGender,
        hair: u8,
    ) -> Self {
        let mut looks = Self {
            class,
            gender,
            hair: u16::from(hair),
            ..Self::default()
        };
        for item in equipment.iter().flatten() {
            let Some(info) = items.get(&item.item_index) else {
                continue;
            };
            if item.current_dura == 0 && info.durability > 0 {
                // 持久度归零的装备在 Crystal 里不参与外观（`UserObject.cs:234`）。
                continue;
            }
            match info.item_type {
                ItemType::Armour => looks.armour = shape_block(info.shape),
                ItemType::Weapon => {
                    let shape = shape_block(info.shape);
                    looks.weapon_shape = shape;
                    looks.weapon = shape % 100;
                }
                _ => {}
            }
        }
        looks
    }

    /// 从 `S.ObjectPlayer` / `S.PlayerUpdate` 的外观字段直接构造。
    ///
    /// 服务端已经把 `Armour = ItemInfo.Shape`、`Weapon = ItemInfo.Shape` 算好了
    /// （`Server/MirObjects/HumanObject.cs:1840-1846`），所以这里只做「非负 + 拆段位」。
    #[must_use]
    pub fn from_server(
        class: MirClass,
        gender: MirGender,
        hair: u8,
        armour: i16,
        weapon: i16,
    ) -> Self {
        let weapon_shape = i16_to_layer(weapon);
        let (weapon_block, weapon_shape) = if weapon_shape == mir_render::NO_LAYER {
            (mir_render::NO_LAYER, mir_render::NO_LAYER)
        } else {
            (weapon_shape % 100, weapon_shape)
        };
        Self {
            class,
            gender,
            hair: u16::from(hair),
            armour: shape_block(armour),
            weapon: weapon_block,
            weapon_shape,
            shield: mir_render::NO_LAYER,
        }
    }

    /// 是否拿着武器（`Weapon >= 0`，Crystal `PlayerObject.cs:558`）。
    #[must_use]
    pub fn has_weapon(&self) -> bool {
        self.weapon != mir_render::NO_LAYER && self.weapon_shape != mir_render::NO_LAYER
    }
}

/// `ItemInfo.Shape` / 服务端 `Armour` 字段 → 图库块号（负数与 `NO_LAYER` 都算 0）。
fn shape_block(shape: i16) -> u16 {
    if shape < 0 {
        0
    } else {
        u16::try_from(shape).unwrap_or(0)
    }
}

/// 服务端的 `Weapon` 字段（`-1` 表示空手）→ 图库块号或 [`mir_render::NO_LAYER`]。
fn i16_to_layer(value: i16) -> u16 {
    if value < 0 {
        mir_render::NO_LAYER
    } else {
        u16::try_from(value).unwrap_or(mir_render::NO_LAYER)
    }
}

/// 自己的角色状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfPlayer {
    /// 对象 ID。
    pub object_id: u32,
    /// 名字。
    pub name: String,
    /// 坐标。
    pub position: Point,
    /// 朝向。
    pub direction: MirDirection,
    /// 当前 HP。
    pub hp: i32,
    /// 当前 MP。
    pub mp: i32,
    /// 本次会话见过的最高 HP（协议不发最大生命，用高水位近似进度条）。
    pub hp_max: i32,
    /// 本次会话见过的最高 MP。
    pub mp_max: i32,
    /// 等级。
    pub level: u16,
    /// 职业。
    pub class: MirClass,
    /// 性别。
    pub gender: MirGender,
    /// 发型（原始值；外观号见 [`Self::looks`]）。
    pub hair: u8,
    /// 金币。
    pub gold: u32,
    /// 是否死亡。
    pub dead: bool,
    /// 当前动作（自己的走/跑/攻击由本地命令驱动，死亡/复活由服务器包驱动）。
    pub action: ActorAction,
    /// 当前动作开始时间（Unix 毫秒）。
    pub action_started_ms: u64,
    /// 纸娃娃外观（身体/武器/头发/盾牌）；装备变化后由快照重新推导。
    pub looks: PlayerLooks,
}

impl SelfPlayer {
    /// 切到一个新动作并记录开始时间。
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

/// 世界里的其他对象。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldEntity {
    /// 对象 ID。
    pub object_id: u32,
    /// 类型。
    pub kind: EntityKind,
    /// 名字。
    pub name: String,
    /// 坐标。
    pub position: Point,
    /// 朝向。
    pub direction: MirDirection,
    /// 图像号（含义随 `kind` 变化，见 `mir_render::EntityView`）。
    pub image: u16,
    /// 纸娃娃外观（只有玩家有意义；服务端在 `S.ObjectPlayer` 里直接给外观号）。
    pub looks: PlayerLooks,
    /// 是否死亡。
    pub dead: bool,
    /// 生命百分比（`S.ObjectHealth`；怪物才有）。
    pub hp_percent: Option<u8>,
    /// 当前动作（`S.ObjectWalk/ObjectRun/ObjectAttack/ObjectStruck/ObjectDied` 驱动）。
    pub action: ActorAction,
    /// 当前动作开始时间（Unix 毫秒）。
    pub action_started_ms: u64,
}

impl WorldEntity {
    /// 只有最基本字段的实体（怪物/NPC/物品）：外观取 [`PlayerLooks::default`]。
    #[must_use]
    pub fn new(
        object_id: u32,
        kind: EntityKind,
        name: String,
        position: Point,
        direction: MirDirection,
        image: u16,
    ) -> Self {
        Self {
            object_id,
            kind,
            name,
            position,
            direction,
            image,
            looks: PlayerLooks::default(),
            dead: false,
            hp_percent: None,
            action: ActorAction::Standing,
            action_started_ms: 0,
        }
    }

    /// 切到一个新动作并记录开始时间。
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

/// 一条聊天记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatLine {
    /// 说话者；`None` 表示系统消息。
    pub speaker: Option<String>,
    /// 内容。
    pub text: String,
    /// 频道。
    pub kind: ChatType,
}

impl ChatLine {
    /// 单行展示文本（控制台日志用；没有字体时 UI 不画）。
    #[must_use]
    pub fn display(&self) -> String {
        match &self.speaker {
            Some(speaker) => format!("[{speaker}] {}", self.text),
            None => self.text.clone(),
        }
    }
}

/// 背包物品（`UserItem` 的界面子集）。
///
/// 只保留画格子需要的四个字段；镶嵌、觉醒、封印等复杂数据留在服务端/协议层，
/// 等以后做装备提示框时再扩展。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryItem {
    /// 唯一编号（`C.DropItem`/`C.SellItem`/`C.RepairItem` 用它定位）。
    pub unique_id: u64,
    /// 模板编号（指向 [`ItemInfoView`]）。
    pub item_index: i32,
    /// 堆叠数量。
    pub count: u16,
    /// 当前持久度。
    pub current_dura: u16,
    /// 最大持久度。
    pub max_dura: u16,
}

impl InventoryItem {
    /// 从协议物品里抽取界面字段。
    #[must_use]
    pub fn from_user_item(item: &UserItem) -> Self {
        Self {
            unique_id: item.unique_id,
            item_index: item.item_index,
            count: item.count,
            current_dura: item.current_dura,
            max_dura: item.max_dura,
        }
    }
}

/// 物品模板（`S.NewItemInfo`）。
///
/// 除了画格子需要的图标/价格，还带上了 tooltip 需要的类型、需求与属性表
/// （`ItemInfo.stats`，Crystal `Shared/Data/Stat.cs` 的 `Stats`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ItemInfoView {
    /// 物品模板编号。
    pub index: i32,
    /// 名称（服务端已本地化）。
    pub name: String,
    /// `items.wzl` 图像号。
    pub image: u16,
    /// 外观编号（`ItemInfo.Shape`）；装备的外观层就取自这里：
    /// 衣服 → `hum*.wzl` 块号，武器 → `weapon*.wzl` 块号（见 [`PlayerLooks`]）。
    pub shape: i16,
    /// 基础价格。
    pub price: u32,
    /// 最大堆叠数（1 表示不可堆叠）。
    pub stack_size: u16,
    /// 物品类型（`ItemType`：武器/衣服/药品/卷轴…）。
    pub item_type: ItemType,
    /// 需求属性的种类（`RequiredType`：默认按等级）。
    pub required_type: RequiredType,
    /// 需求数值（与 `required_type` 搭配）。
    pub required_amount: u8,
    /// 模板最大持久度（0 表示没有持久度概念）。
    pub durability: u16,
    /// 品级（影响 tooltip 名字颜色）。
    pub grade: ItemGrade,
    /// 基础属性表：`Stat` → 数值，按枚举排序。
    pub stats: BTreeMap<Stat, i32>,
}

impl ItemInfoView {
    /// 从协议模板里抽取界面字段。
    #[must_use]
    pub fn from_item_info(info: &ItemInfo) -> Self {
        Self {
            index: info.index,
            name: info.name.clone(),
            image: info.image,
            shape: info.shape,
            price: info.price,
            stack_size: info.stack_size,
            item_type: info.item_type,
            required_type: info.required_type,
            required_amount: info.required_amount,
            durability: info.durability,
            grade: info.grade,
            stats: info.stats.values().clone(),
        }
    }

    /// 取一项属性（缺失时为 0）。
    #[must_use]
    pub fn stat(&self, stat: Stat) -> i32 {
        self.stats.get(&stat).copied().unwrap_or(0)
    }

    /// `(min, max)` 形式的属性区间；两项都为 0 时返回 `None`。
    #[must_use]
    pub fn stat_range(&self, min: Stat, max: Stat) -> Option<(i32, i32)> {
        let low = self.stat(min);
        let high = self.stat(max);
        if low == 0 && high == 0 {
            return None;
        }
        Some((low, high))
    }
}

/// 客户端增益（`S.AddBuff` / `S.RemoveBuff` 维护的界面子集）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuffView {
    /// 增益类型。
    pub buff_type: BuffType,
    /// 是否在图标栏显示（Crystal `ClientBuff.Visible`）。
    pub visible: bool,
    /// 来源对象编号。
    pub object_id: u32,
    /// 到期时间（Unix 毫秒）；`infinite` 时无意义。
    pub expire_time: i64,
    /// 是否永久。
    pub infinite: bool,
    /// 计时是否暂停。
    pub paused: bool,
}

impl BuffView {
    /// 从协议增益里抽取界面字段。
    #[must_use]
    pub fn from_client_buff(buff: &ClientBuff) -> Self {
        Self {
            buff_type: buff.buff_type,
            visible: buff.visible,
            object_id: buff.object_id,
            expire_time: buff.expire_time,
            infinite: buff.infinite,
            paused: buff.paused,
        }
    }

    /// 剩余毫秒；永久/暂停/没有到期时间时返回 `None`（UI 显示「∞」）。
    #[must_use]
    pub fn remaining_millis(&self, now: i64) -> Option<i64> {
        if self.infinite || self.paused || self.expire_time <= 0 {
            return None;
        }
        Some(self.expire_time.saturating_sub(now).max(0))
    }

    /// 剩余秒数（向上取整，和 Crystal `Math.Round` 的观感接近）。
    #[must_use]
    pub fn remaining_seconds(&self, now: i64) -> Option<i64> {
        self.remaining_millis(now)
            .map(|millis| millis.saturating_add(999) / 1000)
    }
}

/// 一次伤害/受击事件（`S.ObjectStruck` / `S.DamageIndicator`）。
///
/// `id` 单调递增，UI 只画比上次见过的 ID 大的事件，避免快照整体克隆导致重复飘字。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DamageEvent {
    /// 递增编号。
    pub id: u64,
    /// 受击对象。
    pub object_id: u32,
    /// 伤害值；`ObjectStruck` 事件没有伤害数字，为 `None`。
    pub damage: Option<i32>,
    /// 伤害类型（`DamageIndicator` 才有）。
    pub damage_type: Option<DamageType>,
    /// 受击格（`ObjectStruck` 才有）。
    pub location: Option<Point>,
}

/// 一次光效事件（`S.ObjectEffect`；魔法盾/元素结界等）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectEvent {
    /// 递增编号。
    pub id: u64,
    /// 目标对象。
    pub object_id: u32,
    /// 特效类型。
    pub effect: SpellEffect,
    /// 持续毫秒（`ObjectEffect.time`）。
    pub time: u32,
}

/// 已学技能（`ClientMagic` 的界面子集）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MagicView {
    /// 技能名。
    pub name: String,
    /// 技能编号。
    pub spell: Spell,
    /// 当前等级。
    pub level: u8,
    /// 绑定的快捷键（0 或 10 以上表示未绑定）。
    pub key: u8,
    /// 基础耗蓝。
    pub base_cost: u8,
    /// 图标编号（未使用）。
    pub icon: u8,
    /// 施法距离。
    pub range: u8,
}

impl MagicView {
    /// 从协议技能里抽取界面字段。
    #[must_use]
    pub fn from_client_magic(magic: &ClientMagic) -> Self {
        Self {
            name: magic.name.clone(),
            spell: magic.spell,
            level: magic.level,
            key: magic.key,
            base_cost: magic.base_cost,
            icon: magic.icon,
            range: magic.range,
        }
    }
}

/// NPC 对话页（`S.NPCResponse`）。
///
/// `version` 每收到一页就递增，UI 用它判断「这是新一页」而不是靠文本比较。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NpcDialog {
    /// 页面正文（原始文本，按键标记由 UI 解析）。
    pub lines: Vec<String>,
    /// 发起对话的 NPC 对象 ID（取离自己最近的 NPC；服务端包里不带这个字段）。
    pub object_id: Option<u32>,
    /// 递增版本号。
    pub version: u64,
}

/// 商店货架条目（`S.NPCGoods` + `S.NewItemInfo` 拼出来）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShopEntry {
    /// 商店物品的唯一编号（`C.BuyItem.ItemIndex` 用的就是它）。
    pub unique_id: u64,
    /// 模板编号。
    pub item_index: i32,
    /// 名称。
    pub name: String,
    /// `items.wzl` 图像号。
    pub image: u16,
    /// 单价。
    pub price: u32,
    /// 货架数量。
    pub count: u16,
    /// 当前持久度。
    pub current_dura: u16,
    /// 最大持久度。
    pub max_dura: u16,
}

/// 商店货架（`S.NPCGoods`）。
#[derive(Debug, Clone, PartialEq)]
pub struct ShopGoods {
    /// 货架条目。
    pub list: Vec<ShopEntry>,
    /// 价格倍率（`NPCScript.PriceRate`）。
    pub rate: f32,
    /// 递增版本号。
    pub version: u64,
}

impl ShopGoods {
    /// 货架是否为空。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
}

/// NPC 面板开关（`S.NPCSell` / `S.NPCRepair` / `S.NPCSRepair`）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NpcPanels {
    /// 可以出售（`S.NPCSell`）。
    pub sell: bool,
    /// 可以普通修理（`S.NPCRepair`）。
    pub repair: bool,
    /// 可以特修（`S.NPCSRepair`）。
    pub special_repair: bool,
}

/// 实体种类 → 动画帧表种类。
fn body_kind_for(kind: EntityKind) -> BodyKind {
    match kind {
        EntityKind::Player => BodyKind::Human,
        EntityKind::Monster | EntityKind::Item => BodyKind::Monster,
        EntityKind::Npc => BodyKind::Npc,
    }
}

/// 一次性动作播完后应该切到哪个动作；`None` 表示还不到切换的时候。
fn finished_action(
    action: ActorAction,
    started_at_ms: u64,
    now_ms: u64,
    body_kind: BodyKind,
) -> Option<ActorAction> {
    let duration = mir_render::animation::frames_for(body_kind, action).duration_ms();
    if now_ms.saturating_sub(started_at_ms) < duration {
        return None;
    }
    match action {
        ActorAction::Walking
        | ActorAction::Running
        | ActorAction::Attacking
        | ActorAction::Struck => Some(ActorAction::Standing),
        ActorAction::Dying => Some(ActorAction::Dead),
        ActorAction::Standing | ActorAction::Dead => None,
    }
}

/// 把动作归一化：已经播完的走/跑/攻击/受击显示为 Standing，死亡显示为 Dead。
fn effective_action(
    action: ActorAction,
    started_at_ms: u64,
    now_ms: u64,
    body_kind: BodyKind,
) -> ActorAction {
    finished_action(action, started_at_ms, now_ms, body_kind).unwrap_or(action)
}

/// 客户端看到的世界。
#[derive(Debug, Clone)]
pub struct WorldSnapshot {
    /// 会话阶段。
    pub status: NetStatus,
    /// 当前地图。
    pub map: Option<MapSnapshot>,
    /// 自己。
    pub self_player: Option<SelfPlayer>,
    /// 其他对象（玩家/怪物/NPC/地面物品），按对象 ID 排序。
    pub entities: BTreeMap<u32, WorldEntity>,
    /// 聊天记录（最新在末尾，最多 `CHAT_LOG_LIMIT` 条）。
    pub chat: Vec<ChatLine>,
    /// 背包（`S.UserInformation.Inventory`；`None` 槽位为空，长度固定 46）。
    pub inventory: Vec<Option<InventoryItem>>,
    /// 装备（`S.UserInformation.Equipment`；槽位下标见 [`EQUIPMENT_SLOTS`]）。
    pub equipment: Vec<Option<InventoryItem>>,
    /// 当前增益（`S.AddBuff` / `S.RemoveBuff`；UI 用本地时钟算剩余秒数）。
    pub buffs: Vec<BuffView>,
    /// 最近的伤害/受击事件（尾部最新；UI 只消费新增的 `id`）。
    pub damage_events: Vec<DamageEvent>,
    /// 下一个伤害事件编号。
    pub next_damage_id: u64,
    /// 最近的光效事件（尾部最新；UI 只消费新增的 `id`）。
    pub effects: Vec<EffectEvent>,
    /// 下一个光效事件编号。
    pub next_effect_id: u64,
    /// 物品模板（`S.NewItemInfo`，按模板编号索引）。
    pub item_infos: BTreeMap<i32, ItemInfoView>,
    /// 已学技能（`S.UserInformation.Magics` + `S.NewMagic`/`S.MagicLeveled`）。
    pub magics: Vec<MagicView>,
    /// 当前 NPC 对话页（`S.NPCResponse`）。
    pub npc_dialog: Option<NpcDialog>,
    /// 当前商店货架（`S.NPCGoods`）。
    pub npc_goods: Option<ShopGoods>,
    /// NPC 面板提示（卖/修理）。
    pub npc_panels: NpcPanels,
    /// 收到过多少页 NPC 对话（版本号来源）。
    pub npc_dialog_version: u64,
    /// 收到过多少次商店货架（版本号来源）。
    pub npc_goods_version: u64,
    /// 收到过的包种类 → 次数。
    pub packet_counts: BTreeMap<&'static str, u32>,
    /// 解析失败的包数量。
    pub decode_errors: u32,
    /// 最近一次错误（登录失败、服务器断开原因等）。
    pub last_error: Option<String>,
}

impl Default for WorldSnapshot {
    fn default() -> Self {
        Self {
            status: NetStatus::Connecting,
            map: None,
            self_player: None,
            entities: BTreeMap::new(),
            chat: Vec::new(),
            inventory: Vec::new(),
            equipment: Vec::new(),
            buffs: Vec::new(),
            damage_events: Vec::new(),
            next_damage_id: 0,
            effects: Vec::new(),
            next_effect_id: 0,
            item_infos: BTreeMap::new(),
            magics: Vec::new(),
            npc_dialog: None,
            npc_goods: None,
            npc_panels: NpcPanels::default(),
            npc_dialog_version: 0,
            npc_goods_version: 0,
            packet_counts: BTreeMap::new(),
            decode_errors: 0,
            last_error: None,
        }
    }
}

impl WorldSnapshot {
    /// 当前是否已经进入世界且拿到了自己的 `UserInformation`。
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.status == NetStatus::InGame && self.self_player.is_some()
    }

    /// 收到 `S.MapChanged` 后要不要重载本地地形：返回请求（地图文件路径 + 新相机中心）。
    ///
    /// 快照里没有地图信息（还没收到 `MapInformation`）时返回 `None`。
    /// 注意这里**不检查文件是否存在**：缺图时由调用方打印警告并保留旧图
    /// （需求：缺文件时保留旧图/黑屏占位）。
    #[must_use]
    pub fn map_reload_request(&self, map_dir: impl AsRef<Path>) -> Option<MapReloadRequest> {
        let map = self.map.as_ref()?;
        let file_name = if map.file_name.is_empty() {
            map.map_index.to_string()
        } else {
            map.file_name.clone()
        };
        let path = map_dir.as_ref().join(format!("{file_name}.map"));
        let position = self
            .self_player
            .as_ref()
            .map_or_else(|| Point::new(0, 0), |player| player.position);
        Some(MapReloadRequest {
            map_index: map.map_index,
            file_name,
            position,
            path,
        })
    }

    /// 某个包收到过多少次（测试/调试用）。
    #[must_use]
    pub fn packet_count(&self, name: &str) -> u32 {
        self.packet_counts.get(name).copied().unwrap_or(0)
    }

    /// 用一条服务端包更新快照（动作开始时间取当前 Unix 毫秒）。
    ///
    /// 网络线程内部每解码一个包就调用一次；无头 UI 测试也用它注入假的
    /// `NPCResponse`/`NPCGoods`/`UserInformation`，不需要真实 NPC。
    pub fn apply_packet(&mut self, packet: &ServerPacket) {
        self.apply_packet_at(packet, now_millis_u64());
    }

    /// 用一条服务端包更新快照，并显式指定「当前时间」（Unix 毫秒）。
    ///
    /// 注入式动画测试用这个入口：同一个包在 `t=1000` 与 `t=1600` 应用会得到
    /// 不同的帧/动作阶段，不依赖系统时钟。
    pub fn apply_packet_at(&mut self, packet: &ServerPacket, now_ms: u64) {
        apply_packet_to(self, packet, now_ms);
    }

    /// 取物品模板。
    #[must_use]
    pub fn item_info(&self, item_index: i32) -> Option<&ItemInfoView> {
        self.item_infos.get(&item_index)
    }

    /// 物品图标号（没有模板时返回 `None`）。
    #[must_use]
    pub fn item_image(&self, item_index: i32) -> Option<u16> {
        self.item_info(item_index).map(|info| info.image)
    }

    /// 物品单价（没有模板时返回 `None`）。
    #[must_use]
    pub fn item_price(&self, item_index: i32) -> Option<u32> {
        self.item_info(item_index).map(|info| info.price)
    }

    /// 取背包格；越界或空格返回 `None`。
    #[must_use]
    pub fn inventory_item(&self, slot: usize) -> Option<&InventoryItem> {
        self.inventory.get(slot).and_then(Option::as_ref)
    }

    /// 按唯一编号找背包格。
    #[must_use]
    pub fn inventory_slot_of(&self, unique_id: u64) -> Option<usize> {
        self.inventory.iter().position(|item| {
            item.as_ref()
                .is_some_and(|item| item.unique_id == unique_id)
        })
    }

    /// 取装备槽里的物品；越界或空槽返回 `None`。
    #[must_use]
    pub fn equipment_item(&self, slot: usize) -> Option<&InventoryItem> {
        self.equipment.get(slot).and_then(Option::as_ref)
    }

    /// 按唯一编号找装备槽。
    #[must_use]
    pub fn equipment_slot_of(&self, unique_id: u64) -> Option<usize> {
        self.equipment.iter().position(|item| {
            item.as_ref()
                .is_some_and(|item| item.unique_id == unique_id)
        })
    }

    /// 某个对象（含自己）所在格；未知对象返回 `None`。
    #[must_use]
    pub fn object_position(&self, object_id: u32) -> Option<Point> {
        if let Some(me) = &self.self_player {
            if me.object_id == object_id {
                return Some(me.position);
            }
        }
        self.entities.get(&object_id).map(|entity| entity.position)
    }

    /// 某个对象的名字（自己也在内）。
    #[must_use]
    pub fn object_name(&self, object_id: u32) -> Option<&str> {
        if let Some(me) = &self.self_player {
            if me.object_id == object_id {
                return Some(me.name.as_str());
            }
        }
        self.entities
            .get(&object_id)
            .map(|entity| entity.name.as_str())
    }

    /// 离自己最近的 NPC（对话对象推断；没有 NPC 时返回 `None`）。
    #[must_use]
    pub fn nearest_npc(&self) -> Option<&WorldEntity> {
        let me = self.self_player.as_ref()?;
        self.entities
            .values()
            .filter(|entity| entity.kind == EntityKind::Npc)
            .min_by_key(|entity| me.position.max_distance(entity.position))
    }

    /// 取某个格子上的 NPC（世界点击对话用）。
    #[must_use]
    pub fn npc_at(&self, cell: Point) -> Option<&WorldEntity> {
        self.entities
            .values()
            .find(|entity| entity.kind == EntityKind::Npc && entity.position == cell)
    }

    /// 转成渲染层的实体列表：自己 + 其他对象。
    ///
    /// 玩家的外观来自 [`PlayerLooks`]：自己由装备槽推导（[`PlayerLooks::from_equipment`]，
    /// 装备变化后快照会重新算），其他人直接用 `S.ObjectPlayer` / `S.PlayerUpdate` 里的
    /// 外观号（[`PlayerLooks::from_server`]）。自己的实体 ID 如果同时也出现在 `entities`
    /// 里，会被跳过，避免画两遍。
    ///
    /// `now_ms` 用来把已经播完的一次性动作归一化：走/跑/攻击/受击结束回 Standing，
    /// 死亡动画结束变 Dead。窗口每帧传真实的 Unix 毫秒；无头测试可以传任意固定时间。
    #[must_use]
    pub fn entity_views(&self, now_ms: u64) -> Vec<EntityView> {
        let mut views = Vec::with_capacity(self.entities.len() + 1);
        let self_id = self.self_player.as_ref().map(|me| me.object_id);
        if let Some(me) = &self.self_player {
            views.push(EntityView {
                position: me.position,
                direction: me.direction,
                kind: EntityKind::Player,
                image: me.looks.armour,
                armour: me.looks.armour,
                weapon: me.looks.weapon,
                weapon_shape: me.looks.weapon_shape,
                hair: me.looks.hair,
                shield: me.looks.shield,
                name: Some(me.name.clone()),
                dead: me.dead,
                hp_percent: None,
                action: effective_action(me.action, me.action_started_ms, now_ms, BodyKind::Human),
                action_started_ms: me.action_started_ms,
            });
        }
        for (object_id, entity) in &self.entities {
            if Some(*object_id) == self_id {
                continue;
            }
            views.push(EntityView {
                position: entity.position,
                direction: entity.direction,
                kind: entity.kind,
                image: entity.image,
                armour: entity.looks.armour,
                weapon: entity.looks.weapon,
                weapon_shape: entity.looks.weapon_shape,
                hair: entity.looks.hair,
                shield: entity.looks.shield,
                name: Some(entity.name.clone()),
                dead: entity.dead,
                hp_percent: entity.hp_percent,
                action: effective_action(
                    entity.action,
                    entity.action_started_ms,
                    now_ms,
                    body_kind_for(entity.kind),
                ),
                action_started_ms: entity.action_started_ms,
            });
        }
        views
    }

    /// 给自己的角色切一个本地动作（走/跑/攻击/转身时立即生效，不等服务器回包）。
    pub fn apply_local_action(
        &mut self,
        action: ActorAction,
        direction: MirDirection,
        now_ms: u64,
    ) {
        if let Some(me) = &mut self.self_player {
            if me.dead {
                return;
            }
            me.direction = direction;
            me.set_action(action, now_ms);
        }
    }

    /// 推进动画状态：把已经播完的走/跑/攻击/受击切回 Standing，把死亡切到 Dead。
    ///
    /// 帧的逐帧选择在渲染层（[`mir_render::animation::ActorAnimation::frame_at`]）；
    /// 这里只维护「动作字段」，让快照本身也反映当前阶段。
    pub fn tick_animations(&mut self, now_ms: u64) {
        if let Some(me) = &mut self.self_player {
            if let Some(action) =
                finished_action(me.action, me.action_started_ms, now_ms, BodyKind::Human)
            {
                me.set_action(action, now_ms);
            }
        }
        for entity in self.entities.values_mut() {
            let body_kind = body_kind_for(entity.kind);
            if let Some(action) =
                finished_action(entity.action, entity.action_started_ms, now_ms, body_kind)
            {
                entity.set_action(action, now_ms);
            }
        }
    }

    /// 重新推导自己的纸娃娃外观（装备/模板/职业/发型变化后调用）。
    ///
    /// Crystal 在 `UserObject.RefreshEquipmentStats`（`Client/MirObjects/UserObject.cs:205-250`）
    /// 里做同一件事：清空 `Weapon/Armour`，再遍历装备槽按物品类型取 `Shape`。这里在
    /// **每处理完一个服务端包**后调用一次——14 个槽位的开销可以忽略，却能让
    /// 「装备先到、`S.NewItemInfo` 后到」这种顺序也最终收敛到正确外观。
    pub fn refresh_self_looks(&mut self) {
        let Some(me) = self.self_player.as_ref() else {
            return;
        };
        let looks = PlayerLooks::from_equipment(
            &self.equipment,
            &self.item_infos,
            me.class,
            me.gender,
            me.hair,
        );
        if let Some(me) = self.self_player.as_mut() {
            me.looks = looks;
        }
    }
}

/// 客户端会话句柄。
///
/// 创建时启动一个后台线程；线程连接失败、登录被拒、服务器断开都会通过
/// [`NetEvent`] 通知。`Drop` 会发送 `Quit` 并等线程退出（读超时 20 ms，最多等
/// 一次连接超时）。
pub struct NetClient {
    commands: Sender<NetCommand>,
    events: Receiver<NetEvent>,
    thread: Option<JoinHandle<()>>,
}

impl NetClient {
    /// 启动后台会话线程。
    ///
    /// 返回的句柄不代表已经连上；用 [`NetClient::wait_until_in_game`] 等登录链路走完。
    ///
    /// # Errors
    /// 线程创建失败时返回错误（连接错误通过事件通知）。
    pub fn connect(options: ConnectOptions) -> Result<Self> {
        let (command_tx, command_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("mir-client-net".to_owned())
            .spawn(move || {
                tracing::info!(server = %options.server, account = %options.account, "开始联机会话");
                let result = session(&options, &command_rx, &event_tx);
                match result {
                    Ok(()) => {
                        let _ = event_tx.send(NetEvent::Closed("连接已关闭".to_owned()));
                    }
                    Err(error) => {
                        let message = format!("{error:#}");
                        tracing::warn!(%message, "联机会话失败");
                        let _ = event_tx.send(NetEvent::Error(message.clone()));
                        let _ = event_tx.send(NetEvent::Closed(message));
                    }
                }
            })
            .context("启动网络线程失败")?;
        Ok(Self {
            commands: command_tx,
            events: event_rx,
            thread: Some(thread),
        })
    }

    /// 给网络线程发一个命令；线程已经退出时返回 `false`。
    #[must_use]
    pub fn send(&self, command: NetCommand) -> bool {
        self.commands.send(command).is_ok()
    }

    /// 非阻塞取一个事件。
    #[must_use]
    pub fn try_recv(&self) -> Option<NetEvent> {
        self.events.try_recv().ok()
    }

    /// 等到进入世界并且收到自己的 `UserInformation`。
    ///
    /// # Errors
    /// 超时、登录被拒、服务器断开时返回错误。
    pub fn wait_until_in_game(&self, timeout: Duration) -> Result<WorldSnapshot> {
        self.wait_for_snapshot(WorldSnapshot::is_ready, timeout)
    }

    /// 等到满足条件的快照。
    ///
    /// # Errors
    /// 超时、收到 [`NetEvent::Error`] 或连接关闭时返回错误。
    pub fn wait_for_snapshot(
        &self,
        predicate: impl Fn(&WorldSnapshot) -> bool,
        timeout: Duration,
    ) -> Result<WorldSnapshot> {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                bail!("等待服务器响应超时（{} ms）", timeout.as_millis());
            }
            match self.events.recv_timeout(remaining) {
                Ok(NetEvent::Snapshot(snapshot)) => {
                    let snapshot = *snapshot;
                    if predicate(&snapshot) {
                        return Ok(snapshot);
                    }
                }
                Ok(NetEvent::Error(message)) => bail!("{message}"),
                Ok(NetEvent::Closed(reason)) => bail!("连接已关闭：{reason}"),
                // 切图事件不改变等待条件，继续等下一个快照/错误。
                Ok(NetEvent::MapChanged { .. }) => {}
                Err(RecvTimeoutError::Timeout) => {
                    bail!("等待服务器响应超时（{} ms）", timeout.as_millis());
                }
                Err(RecvTimeoutError::Disconnected) => bail!("网络线程已退出"),
            }
        }
    }

    /// 主动关闭连接并等后台线程退出（可重复调用）。
    pub fn shutdown(&mut self) {
        let _ = self.commands.send(NetCommand::Quit);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for NetClient {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// 后台会话主循环。
fn session(
    options: &ConnectOptions,
    commands: &Receiver<NetCommand>,
    events: &Sender<NetEvent>,
) -> Result<()> {
    let mut stream = connect(&options.server)?;
    stream.set_nodelay(true).ok();
    stream.set_read_timeout(Some(READ_TIMEOUT))?;

    let mut snapshot = WorldSnapshot {
        status: NetStatus::Verifying,
        ..WorldSnapshot::default()
    };
    send_snapshot(events, &snapshot);

    let mut stage = Stage::AwaitConnected;
    let mut frames = FrameBuffer::new();
    let mut buffer = [0u8; 8192];
    let mut last_keepalive = Instant::now();
    let mut dirty = false;

    loop {
        while let Ok(command) = commands.try_recv() {
            if matches!(command, NetCommand::Quit) {
                let _ = send(&mut stream, &ClientPacket::Disconnect(c::Disconnect {}));
                snapshot.status = NetStatus::Closed;
                send_snapshot(events, &snapshot);
                return Ok(());
            }
            handle_command(&mut stream, &mut snapshot, command)?;
        }

        match stream.read(&mut buffer) {
            Ok(0) => {
                snapshot.status = NetStatus::Closed;
                send_snapshot(events, &snapshot);
                return Ok(());
            }
            Ok(read) => {
                frames.push(&buffer[..read]);
                // 这一批里的 `MapChanged`：快照发完后再发事件，UI 处理事件时快照已是新地图。
                let mut map_changes = Vec::new();
                while let Some((index, payload)) = frames.next_frame()? {
                    match ServerPacket::decode(index, &payload) {
                        Ok(packet) => {
                            if let ServerPacket::MapChanged(changed) = &packet {
                                map_changes.push((changed.map_index, changed.location));
                            }
                            apply_packet_to(&mut snapshot, &packet, now_millis_u64());
                            advance(&mut stream, options, &mut stage, &mut snapshot, &packet)?;
                            dirty = true;
                        }
                        Err(error) => {
                            tracing::warn!(index, %error, "无法解析服务端包，已跳过");
                            snapshot.decode_errors = snapshot.decode_errors.saturating_add(1);
                            dirty = true;
                        }
                    }
                }
                if dirty {
                    send_snapshot(events, &snapshot);
                    dirty = false;
                }
                for (map_index, position) in map_changes {
                    let _ = events.send(NetEvent::MapChanged {
                        map_index,
                        position,
                    });
                }
            }
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(error) => return Err(error).context("读取服务器数据失败"),
        }

        if stage == Stage::InGame && last_keepalive.elapsed() >= KEEPALIVE_INTERVAL {
            let time = now_millis();
            send(&mut stream, &ClientPacket::KeepAlive(c::KeepAlive { time }))?;
            last_keepalive = Instant::now();
        }
    }
}

/// 连接服务器：逐个地址尝试，带 5 秒超时。
fn connect(address: &str) -> Result<TcpStream> {
    let addresses = address
        .to_socket_addrs()
        .with_context(|| format!("无法解析服务器地址 {address}"))?;
    let mut last_error = None;
    for address in addresses {
        match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
            Ok(stream) => return Ok(stream),
            Err(error) => last_error = Some(error),
        }
    }
    match last_error {
        Some(error) => Err(error).with_context(|| format!("无法连接服务器 {address}")),
        None => Err(anyhow!("服务器地址 {address} 没有可用的套接字地址")),
    }
}

/// 登录状态机。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// 等 `S.Connected`。
    AwaitConnected,
    /// 等 `S.ClientVersion`。
    AwaitVersion,
    /// 等 `S.Login` / `S.LoginSuccess`。
    AwaitLogin,
    /// 等 `S.NewAccount`。
    AwaitNewAccount,
    /// 等 `S.NewCharacterSuccess` / `S.NewCharacter`。
    AwaitNewCharacter,
    /// 等 `S.StartGame`。
    AwaitStartGame,
    /// 已进入世界。
    InGame,
}

/// 根据收到的服务器包推进登录状态机。
fn advance(
    stream: &mut TcpStream,
    options: &ConnectOptions,
    stage: &mut Stage,
    snapshot: &mut WorldSnapshot,
    packet: &ServerPacket,
) -> Result<()> {
    match (*stage, packet) {
        (Stage::AwaitConnected, ServerPacket::Connected(_)) => {
            snapshot.status = NetStatus::Verifying;
            send(
                stream,
                &ClientPacket::ClientVersion(c::ClientVersion {
                    version_hash: VERSION_HASH.to_vec(),
                }),
            )?;
            *stage = Stage::AwaitVersion;
        }
        (Stage::AwaitVersion, ServerPacket::ClientVersion(version)) => {
            if version.result != 1 {
                bail!("服务器拒绝客户端版本（ClientVersion.result = 0）");
            }
            snapshot.status = NetStatus::LoggingIn;
            send_login(stream, options)?;
            *stage = Stage::AwaitLogin;
        }
        (Stage::AwaitLogin, ServerPacket::Login(result)) => match result.result {
            3 if options.auto_register => {
                send_new_account(stream, options)?;
                *stage = Stage::AwaitNewAccount;
            }
            3 => bail!(
                "账号 {} 不存在（已用 --no-register 关闭自动注册）",
                options.account
            ),
            4 => bail!("账号或口令错误"),
            5 => bail!("账号需要修改口令后才能登录"),
            1 => bail!("账号格式非法（3~15 位字母数字）"),
            2 => bail!("口令格式非法（5~15 位字母数字）"),
            0 => bail!("服务器禁止登录"),
            other => bail!("登录被拒绝（result = {other}）"),
        },
        (Stage::AwaitLogin, ServerPacket::LoginBanned(banned)) => {
            bail!("账号被封禁：{}", banned.reason);
        }
        (Stage::AwaitLogin, ServerPacket::LoginSuccess(login)) => {
            snapshot.status = NetStatus::SelectingCharacter;
            if let Some(character) = find_character(&login.characters, &options.character) {
                send_start_game(stream, character.index)?;
                *stage = Stage::AwaitStartGame;
            } else if let Some(create) = &options.create {
                send_new_character(stream, create)?;
                *stage = Stage::AwaitNewCharacter;
            } else {
                bail!(
                    "账号下没有角色 {}，请用 --create-character {} --class N --gender M 自动创建",
                    options.character,
                    options.character
                );
            }
        }
        (Stage::AwaitNewAccount, ServerPacket::NewAccount(result)) => match result.result {
            8 | 7 => {
                snapshot.status = NetStatus::LoggingIn;
                send_login(stream, options)?;
                *stage = Stage::AwaitLogin;
            }
            other => bail!("注册账号失败（result = {other}）"),
        },
        (Stage::AwaitNewCharacter, ServerPacket::NewCharacterSuccess(success)) => {
            send_start_game(stream, success.char_info.index)?;
            *stage = Stage::AwaitStartGame;
        }
        (Stage::AwaitNewCharacter, ServerPacket::NewCharacter(result)) => {
            let reason = match result.result {
                0 => "服务器禁止创建角色",
                1 => "角色名非法（3~15 位中文/字母/数字/下划线）",
                2 => "角色名已存在",
                4 => "角色数量已满",
                other => return Err(anyhow!("创建角色失败（result = {other}）")),
            };
            bail!("{reason}");
        }
        (Stage::AwaitStartGame, ServerPacket::StartGame(result)) => {
            if result.result == 4 {
                snapshot.status = NetStatus::InGame;
                *stage = Stage::InGame;
            } else {
                let reason = match result.result {
                    0 => "服务器禁止进入游戏",
                    1 => "会话没有账号",
                    2 => "角色不存在",
                    other => return Err(anyhow!("进入游戏失败（result = {other}）")),
                };
                bail!("{reason}");
            }
        }
        (Stage::AwaitStartGame, ServerPacket::StartGameBanned(banned)) => {
            bail!("无法进入游戏：{}", banned.reason);
        }
        (_, ServerPacket::Disconnect(disconnect)) => {
            bail!("服务器断开了连接（reason = {}）", disconnect.reason);
        }
        _ => {}
    }
    Ok(())
}

/// 查找角色：先精确匹配，再不区分大小写匹配。
fn find_character<'a>(
    characters: &'a [mir_protocol::data::SelectInfo],
    name: &str,
) -> Option<&'a mir_protocol::data::SelectInfo> {
    characters
        .iter()
        .find(|character| character.name == name)
        .or_else(|| {
            characters
                .iter()
                .find(|character| character.name.eq_ignore_ascii_case(name))
        })
}

fn send_login(stream: &mut TcpStream, options: &ConnectOptions) -> Result<()> {
    send(
        stream,
        &ClientPacket::Login(c::Login {
            account_id: options.account.clone(),
            password: options.password.clone(),
        }),
    )
}

fn send_new_account(stream: &mut TcpStream, options: &ConnectOptions) -> Result<()> {
    tracing::info!(account = %options.account, "账号不存在，自动注册");
    send(
        stream,
        &ClientPacket::NewAccount(c::NewAccount {
            account_id: options.account.clone(),
            password: options.password.clone(),
            birth_date: DateTimeBinary(0),
            user_name: String::new(),
            secret_question: String::new(),
            secret_answer: String::new(),
            e_mail_address: String::new(),
        }),
    )
}

fn send_new_character(stream: &mut TcpStream, create: &CreateCharacter) -> Result<()> {
    tracing::info!(name = %create.name, class = ?create.class, "创建角色");
    send(
        stream,
        &ClientPacket::NewCharacter(c::NewCharacter {
            name: create.name.clone(),
            gender: create.gender,
            class: create.class,
        }),
    )
}

fn send_start_game(stream: &mut TcpStream, character_index: i32) -> Result<()> {
    tracing::info!(character_index, "进入世界");
    send(
        stream,
        &ClientPacket::StartGame(c::StartGame { character_index }),
    )
}

/// 把 UI 命令转成客户端包；不在游戏里时静默丢弃。
fn handle_command(
    stream: &mut TcpStream,
    snapshot: &mut WorldSnapshot,
    command: NetCommand,
) -> Result<()> {
    if snapshot.status != NetStatus::InGame {
        return Ok(());
    }
    // 自己的动作没有服务器回显（服务端把 `ObjectWalk` 广播给别人，自己只收到
    // `UserLocation`），所以在发包的同时立即驱动本地动画。
    if let Some((action, direction)) = command.local_animation() {
        snapshot.apply_local_action(action, direction, now_millis_u64());
    }
    let packet = match command {
        NetCommand::Turn(direction) => ClientPacket::Turn(c::Turn { direction }),
        NetCommand::Walk(direction) => ClientPacket::Walk(c::Walk { direction }),
        NetCommand::Run(direction) => ClientPacket::Run(c::Run { direction }),
        NetCommand::Attack(direction) => ClientPacket::Attack(c::Attack {
            direction,
            spell: mir_core::enums::Spell::None,
        }),
        NetCommand::PickUp => ClientPacket::PickUp(c::PickUp {}),
        NetCommand::Chat(message) => {
            if message.trim().is_empty() {
                return Ok(());
            }
            // Crystal 的 `Globals.MaxChatLength = 80`（`MirConnection.cs:1183-1189`）；
            // 客户端先截断，避免一个超长粘贴包把 u16 帧长撑爆。
            let message = message.chars().take(80).collect::<String>();
            ClientPacket::Chat(c::Chat {
                message,
                linked_items: Vec::new(),
            })
        }
        NetCommand::CallNpc { object_id, key } => {
            ClientPacket::CallNPC(c::CallNPC { object_id, key })
        }
        NetCommand::BuyItem { unique_id, count } => ClientPacket::BuyItem(c::BuyItem {
            item_index: unique_id,
            count,
            type_: PanelType::Buy,
        }),
        NetCommand::SellItem { unique_id, count } => {
            ClientPacket::SellItem(c::SellItem { unique_id, count })
        }
        NetCommand::RepairItem { unique_id } => {
            ClientPacket::RepairItem(c::RepairItem { unique_id })
        }
        NetCommand::DropItem { unique_id, count } => ClientPacket::DropItem(c::DropItem {
            unique_id,
            count,
            hero_inventory: false,
        }),
        NetCommand::UseItem { unique_id } => ClientPacket::UseItem(c::UseItem {
            unique_id,
            grid: MirGridType::Inventory,
        }),
        NetCommand::MoveItem {
            grid,
            from,
            to,
            to_grid,
        } => {
            // 只要有一端是装备网格，就按 `mir-server` 的 `World::move_equipment` 发：
            // 它用 `grid = Equipment` + `from`/`to` 的方向区分穿/脱。
            let wire_grid = if grid == MirGridType::Equipment || to_grid == MirGridType::Equipment {
                MirGridType::Equipment
            } else {
                grid
            };
            ClientPacket::MoveItem(c::MoveItem {
                grid: wire_grid,
                from,
                to,
            })
        }
        NetCommand::Magic {
            spell,
            direction,
            target_id,
            location,
        } => ClientPacket::Magic(c::Magic {
            spell,
            direction,
            target_id,
            location,
            object_id: snapshot.self_player.as_ref().map_or(0, |me| me.object_id),
            spell_target_lock: false,
        }),
        NetCommand::MagicKey {
            spell,
            key,
            old_key,
        } => ClientPacket::MagicKey(c::MagicKey {
            spell,
            key,
            old_key,
        }),
        NetCommand::Quit => return Ok(()),
    };
    send(stream, &packet)
}

/// 发一个客户端包（含帧头）。
fn send(stream: &mut TcpStream, packet: &ClientPacket) -> Result<()> {
    let frame = packet.encode_frame();
    stream.write_all(&frame).context("写入服务器失败")?;
    stream.flush().context("刷新服务器连接失败")?;
    Ok(())
}

/// 把快照发给 UI；UI 已经不在了就忽略（线程会在下一轮发现命令通道断开）。
fn send_snapshot(events: &Sender<NetEvent>, snapshot: &WorldSnapshot) {
    let _ = events.send(NetEvent::Snapshot(Box::new(snapshot.clone())));
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
        })
}

/// 当前 Unix 毫秒（无符号；动作时间戳用）。
fn now_millis_u64() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

/// 包种类名；只列会被快照处理的包，其余归到 `"Other"`。
fn packet_name(packet: &ServerPacket) -> &'static str {
    match packet {
        ServerPacket::StartGame(_) => "StartGame",
        ServerPacket::MapInformation(_) => "MapInformation",
        ServerPacket::NewMapInfo(_) => "NewMapInfo",
        ServerPacket::MapChanged(_) => "MapChanged",
        ServerPacket::UserInformation(_) => "UserInformation",
        ServerPacket::UserLocation(_) => "UserLocation",
        ServerPacket::ObjectPlayer(_) => "ObjectPlayer",
        ServerPacket::ObjectMonster(_) => "ObjectMonster",
        ServerPacket::ObjectNPC(_) => "ObjectNPC",
        ServerPacket::ObjectItem(_) => "ObjectItem",
        ServerPacket::ObjectGold(_) => "ObjectGold",
        ServerPacket::ObjectRemove(_) => "ObjectRemove",
        ServerPacket::ObjectTurn(_) => "ObjectTurn",
        ServerPacket::ObjectWalk(_) => "ObjectWalk",
        ServerPacket::ObjectRun(_) => "ObjectRun",
        ServerPacket::ObjectAttack(_) => "ObjectAttack",
        ServerPacket::ObjectStruck(_) => "ObjectStruck",
        ServerPacket::DamageIndicator(_) => "DamageIndicator",
        ServerPacket::ObjectEffect(_) => "ObjectEffect",
        ServerPacket::AddBuff(_) => "AddBuff",
        ServerPacket::RemoveBuff(_) => "RemoveBuff",
        ServerPacket::ObjectDied(_) => "ObjectDied",
        ServerPacket::ObjectRevived(_) => "ObjectRevived",
        ServerPacket::Death(_) => "Death",
        ServerPacket::Revived(_) => "Revived",
        ServerPacket::HealthChanged(_) => "HealthChanged",
        ServerPacket::ObjectHealth(_) => "ObjectHealth",
        ServerPacket::LevelChanged(_) => "LevelChanged",
        ServerPacket::NewItemInfo(_) => "NewItemInfo",
        ServerPacket::GainedItem(_) => "GainedItem",
        ServerPacket::GainedGold(_) => "GainedGold",
        ServerPacket::LoseGold(_) => "LoseGold",
        ServerPacket::MoveItem(_) => "MoveItem",
        ServerPacket::EquipItem(_) => "EquipItem",
        ServerPacket::RemoveItem(_) => "RemoveItem",
        ServerPacket::DeleteItem(_) => "DeleteItem",
        ServerPacket::RefreshItem(_) => "RefreshItem",
        ServerPacket::ItemRepaired(_) => "ItemRepaired",
        ServerPacket::NewMagic(_) => "NewMagic",
        ServerPacket::RemoveMagic(_) => "RemoveMagic",
        ServerPacket::MagicLeveled(_) => "MagicLeveled",
        ServerPacket::Chat(_) => "Chat",
        ServerPacket::ObjectChat(_) => "ObjectChat",
        ServerPacket::NPCResponse(_) => "NPCResponse",
        ServerPacket::NPCGoods(_) => "NPCGoods",
        ServerPacket::NPCSell(_) => "NPCSell",
        ServerPacket::NPCRepair(_) => "NPCRepair",
        ServerPacket::NPCSRepair(_) => "NPCSRepair",
        ServerPacket::ObjectName(_) => "ObjectName",
        ServerPacket::PlayerUpdate(_) => "PlayerUpdate",
        ServerPacket::Disconnect(_) => "Disconnect",
        _ => "Other",
    }
}

/// 用一条服务端包更新快照。
fn apply_packet_to(snapshot: &mut WorldSnapshot, packet: &ServerPacket, now_ms: u64) {
    *snapshot
        .packet_counts
        .entry(packet_name(packet))
        .or_default() += 1;

    match packet {
        ServerPacket::StartGame(result) => {
            if result.result == 4 {
                snapshot.status = NetStatus::InGame;
            }
        }
        ServerPacket::MapInformation(info) => {
            snapshot.map = Some(MapSnapshot {
                map_index: info.map_index,
                file_name: info.file_name.clone(),
                title: info.title.clone(),
                width: 0,
                height: 0,
            });
        }
        ServerPacket::NewMapInfo(info) => {
            let map = snapshot.map.get_or_insert_with(MapSnapshot::default);
            map.map_index = info.map_index;
            map.title.clone_from(&info.info.title);
            map.width = info.info.width;
            map.height = info.info.height;
            for npc in &info.info.npcs {
                if npc.object_id == 0 {
                    continue;
                }
                snapshot.entities.insert(
                    npc.object_id,
                    WorldEntity::new(
                        npc.object_id,
                        EntityKind::Npc,
                        npc.name.clone(),
                        npc.location,
                        MirDirection::Down,
                        npc.image,
                    ),
                );
            }
        }
        ServerPacket::MapChanged(changed) => {
            snapshot.map = Some(MapSnapshot {
                map_index: changed.map_index,
                file_name: changed.file_name.clone(),
                title: changed.title.clone(),
                width: 0,
                height: 0,
            });
            snapshot.entities.clear();
            // 换地图等于离开当前 NPC，UI 必须把对话/商店收起来。
            snapshot.npc_dialog = None;
            snapshot.npc_goods = None;
            snapshot.npc_panels = NpcPanels::default();
            if let Some(me) = &mut snapshot.self_player {
                me.position = changed.location;
                me.direction = changed.direction;
                me.set_action(ActorAction::Standing, now_ms);
            }
        }
        ServerPacket::UserInformation(info) => {
            // `Inventory = null` 表示「客户端保留旧数据」（Crystal 的语义）。
            if let Some(inventory) = &info.inventory {
                snapshot.inventory = inventory
                    .iter()
                    .map(|item| item.as_ref().map(InventoryItem::from_user_item))
                    .collect();
            }
            // 装备同理：`null` 表示保留旧数据。先落装备再算外观，`looks` 才是新值。
            if let Some(equipment) = &info.equipment {
                snapshot.equipment = equipment
                    .iter()
                    .map(|item| item.as_ref().map(InventoryItem::from_user_item))
                    .collect();
            }
            let looks = PlayerLooks::from_equipment(
                &snapshot.equipment,
                &snapshot.item_infos,
                info.class,
                info.gender,
                info.hair,
            );
            snapshot.self_player = Some(SelfPlayer {
                object_id: info.object_id,
                name: info.name.clone(),
                position: info.location,
                direction: info.direction,
                hp: info.hp,
                mp: info.mp,
                hp_max: info.hp.max(1),
                mp_max: info.mp.max(1),
                level: info.level,
                class: info.class,
                gender: info.gender,
                hair: info.hair,
                gold: info.gold,
                dead: false,
                action: ActorAction::Standing,
                action_started_ms: 0,
                looks,
            });
            snapshot.magics = info
                .magics
                .iter()
                .map(MagicView::from_client_magic)
                .collect();
        }
        ServerPacket::UserLocation(location) => {
            if let Some(me) = &mut snapshot.self_player {
                me.position = location.location;
                me.direction = location.direction;
            }
        }
        ServerPacket::ObjectPlayer(player) => {
            // 服务端已经把外观算好（`Armour = ItemInfo.Shape`、`Weapon = ItemInfo.Shape`，
            // `Server/MirObjects/HumanObject.cs:1840-1846`），客户端直接拿来分层。
            let looks = PlayerLooks::from_server(
                player.class,
                player.gender,
                player.hair,
                player.armour,
                player.weapon,
            );
            upsert_entity(
                snapshot,
                WorldEntity {
                    object_id: player.object_id,
                    kind: EntityKind::Player,
                    name: player.name.clone(),
                    position: player.location,
                    direction: player.direction,
                    image: looks.armour,
                    looks,
                    dead: player.dead,
                    hp_percent: None,
                    action: if player.dead {
                        ActorAction::Dead
                    } else {
                        ActorAction::Standing
                    },
                    action_started_ms: now_ms,
                },
            );
        }
        ServerPacket::ObjectMonster(monster) => {
            upsert_entity(
                snapshot,
                WorldEntity {
                    dead: monster.dead,
                    action: if monster.dead {
                        ActorAction::Dead
                    } else {
                        ActorAction::Standing
                    },
                    action_started_ms: now_ms,
                    ..WorldEntity::new(
                        monster.object_id,
                        EntityKind::Monster,
                        monster.name.clone(),
                        monster.location,
                        monster.direction,
                        u16::from(monster.image),
                    )
                },
            );
        }
        ServerPacket::ObjectNPC(npc) => {
            upsert_entity(
                snapshot,
                WorldEntity::new(
                    npc.object_id,
                    EntityKind::Npc,
                    npc.name.clone(),
                    npc.location,
                    npc.direction,
                    npc.image,
                ),
            );
        }
        ServerPacket::ObjectItem(item) => {
            upsert_entity(
                snapshot,
                WorldEntity::new(
                    item.object_id,
                    EntityKind::Item,
                    item.name.clone(),
                    item.location,
                    MirDirection::Down,
                    item.image,
                ),
            );
        }
        ServerPacket::ObjectGold(gold) => {
            // Crystal 用 112..116 五帧画不同数量的金币堆（`ItemObject.cs:55-64`）。
            let image = match gold.gold {
                0..=99 => 112,
                100..=199 => 113,
                200..=499 => 114,
                500..=999 => 115,
                _ => 116,
            };
            upsert_entity(
                snapshot,
                WorldEntity::new(
                    gold.object_id,
                    EntityKind::Item,
                    format!("金币 {}", gold.gold),
                    gold.location,
                    MirDirection::Down,
                    image,
                ),
            );
        }
        ServerPacket::ObjectRemove(remove) => {
            snapshot.entities.remove(&remove.object_id);
        }
        ServerPacket::ObjectTurn(turn) => {
            update_entity(snapshot, turn.object_id, |entity| {
                entity.position = turn.location;
                entity.direction = turn.direction;
                entity.set_action(ActorAction::Standing, now_ms);
            });
        }
        ServerPacket::ObjectWalk(walk) => {
            update_entity(snapshot, walk.object_id, |entity| {
                entity.position = walk.location;
                entity.direction = walk.direction;
                entity.set_action(ActorAction::Walking, now_ms);
            });
        }
        ServerPacket::ObjectRun(run) => {
            update_entity(snapshot, run.object_id, |entity| {
                entity.position = run.location;
                entity.direction = run.direction;
                entity.set_action(ActorAction::Running, now_ms);
            });
        }
        ServerPacket::ObjectAttack(attack) => {
            update_entity(snapshot, attack.object_id, |entity| {
                entity.position = attack.location;
                entity.direction = attack.direction;
                entity.set_action(ActorAction::Attacking, now_ms);
            });
        }
        ServerPacket::ObjectStruck(struck) => {
            update_entity(snapshot, struck.object_id, |entity| {
                entity.direction = struck.direction;
                entity.set_action(ActorAction::Struck, now_ms);
            });
            push_damage_event(
                snapshot,
                struck.object_id,
                None,
                None,
                Some(struck.location),
            );
        }
        ServerPacket::DamageIndicator(indicator) => {
            push_damage_event(
                snapshot,
                indicator.object_id,
                Some(indicator.damage),
                Some(indicator.type_),
                None,
            );
        }
        ServerPacket::ObjectEffect(effect) => {
            push_effect_event(snapshot, effect.object_id, effect.effect, effect.time);
        }
        ServerPacket::AddBuff(add) => {
            let buff = BuffView::from_client_buff(&add.buff);
            if let Some(existing) = snapshot.buffs.iter_mut().find(|known| {
                known.buff_type == buff.buff_type && known.object_id == buff.object_id
            }) {
                *existing = buff;
            } else {
                snapshot.buffs.push(buff);
            }
        }
        ServerPacket::RemoveBuff(remove) => {
            snapshot.buffs.retain(|buff| {
                !(buff.buff_type == remove.type_
                    && (remove.object_id == 0 || buff.object_id == remove.object_id))
            });
        }
        ServerPacket::ObjectDied(died) => {
            update_entity(snapshot, died.object_id, |entity| {
                entity.position = died.location;
                entity.direction = died.direction;
                entity.hp_percent = Some(0);
                entity.set_action(ActorAction::Dying, now_ms);
            });
        }
        ServerPacket::ObjectRevived(revived) => {
            let object_id = revived.object_id;
            update_entity(snapshot, object_id, |entity| {
                entity.set_action(ActorAction::Standing, now_ms);
            });
        }
        ServerPacket::ObjectHealth(health) => {
            update_entity(snapshot, health.object_id, |entity| {
                entity.hp_percent = Some(health.percent.min(100));
            });
        }
        ServerPacket::Death(death) => {
            if let Some(me) = &mut snapshot.self_player {
                me.position = death.location;
                me.direction = death.direction;
                me.set_action(ActorAction::Dying, now_ms);
            }
        }
        ServerPacket::Revived(_) => {
            if let Some(me) = &mut snapshot.self_player {
                me.set_action(ActorAction::Standing, now_ms);
            }
        }
        ServerPacket::HealthChanged(health) => {
            if let Some(me) = &mut snapshot.self_player {
                me.hp = health.hp;
                me.mp = health.mp;
                me.hp_max = me.hp_max.max(health.hp).max(1);
                me.mp_max = me.mp_max.max(health.mp).max(1);
            }
        }
        ServerPacket::LevelChanged(level) => {
            if let Some(me) = &mut snapshot.self_player {
                me.level = level.level;
            }
        }
        ServerPacket::NewItemInfo(info) => {
            snapshot
                .item_infos
                .insert(info.info.index, ItemInfoView::from_item_info(&info.info));
        }
        ServerPacket::MoveItem(movement) => {
            // 背包内的交换能直接映射到界面格子；装备网格的移动由 `S.EquipItem` /
            // `S.RemoveItem` 回显（`mir-server` 的 `World::move_equipment`），这里只吞掉
            // 失败回执。
            if movement.success && movement.grid == MirGridType::Inventory {
                snap_move_inventory(snapshot, movement.from, movement.to);
            }
        }
        ServerPacket::EquipItem(equipped) => {
            // 穿上的物品离开背包、进入装备槽；换下来的旧装备回到同一个背包格
            // （Crystal `PlayerObject.EquipItem`，`PlayerObject.cs:5674-5678`）。
            if equipped.success && equipped.grid == MirGridType::Inventory {
                equip_from_inventory(snapshot, equipped.unique_id, equipped.to);
            }
        }
        ServerPacket::RemoveItem(removed) => {
            // 脱下装备会回到背包；`S.RemoveItem` 只带目标背包格，物品数据来自快照里的装备。
            if removed.success {
                if removed.grid == MirGridType::Equipment {
                    unequip_to_inventory(snapshot, removed.unique_id, removed.to);
                } else if removed.grid == MirGridType::Inventory {
                    remove_inventory_item(snapshot, removed.unique_id);
                }
            }
        }
        ServerPacket::DeleteItem(deleted) => {
            delete_inventory_item(snapshot, deleted.unique_id, deleted.count);
        }
        ServerPacket::RefreshItem(refreshed) => {
            refresh_inventory_item(snapshot, &refreshed.item);
        }
        ServerPacket::ItemRepaired(repaired) => {
            if let Some(slot) = snapshot.inventory_slot_of(repaired.unique_id) {
                if let Some(item) = snapshot.inventory.get_mut(slot).and_then(Option::as_mut) {
                    item.current_dura = repaired.current_dura;
                    item.max_dura = repaired.max_dura;
                }
            }
        }
        ServerPacket::NewMagic(magic) => {
            let view = MagicView::from_client_magic(&magic.magic);
            if let Some(existing) = snapshot
                .magics
                .iter_mut()
                .find(|known| known.spell == view.spell)
            {
                *existing = view;
            } else {
                snapshot.magics.push(view);
            }
        }
        ServerPacket::RemoveMagic(removed) => {
            let index = usize::try_from(removed.place_id).ok();
            if let Some(index) = index {
                if index < snapshot.magics.len() {
                    snapshot.magics.remove(index);
                }
            }
        }
        ServerPacket::MagicLeveled(leveled) => {
            if let Some(magic) = snapshot
                .magics
                .iter_mut()
                .find(|magic| magic.spell == leveled.spell)
            {
                magic.level = leveled.level;
            }
        }
        ServerPacket::GainedItem(gained) => {
            let item = InventoryItem::from_user_item(&gained.item);
            add_inventory_item(snapshot, item.clone());
            let name = snapshot.item_info(item.item_index).map_or_else(
                || format!("物品 #{}", item.item_index),
                |info| info.name.clone(),
            );
            push_chat(
                snapshot,
                ChatLine {
                    speaker: None,
                    text: format!("获得 {name} ×{}", item.count),
                    kind: ChatType::System,
                },
            );
        }
        ServerPacket::GainedGold(gained) => {
            if let Some(me) = &mut snapshot.self_player {
                me.gold = me.gold.saturating_add(gained.gold);
            }
            push_chat(
                snapshot,
                ChatLine {
                    speaker: None,
                    text: format!("获得金币 {}", gained.gold),
                    kind: ChatType::System,
                },
            );
        }
        ServerPacket::LoseGold(lost) => {
            if let Some(me) = &mut snapshot.self_player {
                me.gold = me.gold.saturating_sub(lost.gold);
            }
        }
        ServerPacket::Chat(chat) => {
            tracing::info!(message = %chat.message, "系统/频道聊天");
            push_chat(
                snapshot,
                ChatLine {
                    speaker: None,
                    text: chat.message.clone(),
                    kind: chat.type_,
                },
            );
        }
        ServerPacket::ObjectChat(chat) => {
            let speaker = snapshot
                .entities
                .get(&chat.object_id)
                .map(|entity| entity.name.clone());
            tracing::info!(
                speaker = speaker.as_deref().unwrap_or("?"),
                text = %chat.text,
                "对象聊天"
            );
            push_chat(
                snapshot,
                ChatLine {
                    speaker,
                    text: chat.text.clone(),
                    kind: chat.type_,
                },
            );
        }
        ServerPacket::NPCResponse(response) => {
            tracing::info!(lines = response.page.len(), "NPC 对话");
            snapshot.npc_dialog_version = snapshot.npc_dialog_version.saturating_add(1);
            let object_id = snapshot.nearest_npc().map(|npc| npc.object_id);
            snapshot.npc_dialog = Some(NpcDialog {
                lines: response.page.clone(),
                object_id,
                version: snapshot.npc_dialog_version,
            });
            // 每翻一页都先清掉旧货架/旧面板提示：服务端随后会为买/卖/修理页重新发
            // `S.NPCGoods` / `S.NPCSell` 等；普通页面则不应继续显示上一页的商店。
            snapshot.npc_goods = None;
            snapshot.npc_panels = NpcPanels::default();
            if response.page.is_empty() {
                // 空正文 = 服务端确认关窗（`Client/MirScenes/GameScene.cs:3891-3907`）。
                tracing::debug!("NPC 对话已关闭");
            }
        }
        ServerPacket::NPCGoods(goods) => {
            tracing::info!(
                items = goods.list.len(),
                rate = goods.rate,
                "NPC 商店商品列表"
            );
            snapshot.npc_goods_version = snapshot.npc_goods_version.saturating_add(1);
            let list = goods
                .list
                .iter()
                .map(|item| {
                    let info = snapshot.item_infos.get(&item.item_index);
                    ShopEntry {
                        unique_id: item.unique_id,
                        item_index: item.item_index,
                        name: info.map_or_else(
                            || format!("物品 #{}", item.item_index),
                            |info| info.name.clone(),
                        ),
                        image: info.map_or(0, |info| info.image),
                        price: info.map_or(0, |info| info.price),
                        count: item.count,
                        current_dura: item.current_dura,
                        max_dura: item.max_dura,
                    }
                })
                .collect();
            snapshot.npc_goods = Some(ShopGoods {
                list,
                rate: goods.rate,
                version: snapshot.npc_goods_version,
            });
        }
        ServerPacket::NPCSell(_) => {
            snapshot.npc_panels.sell = true;
        }
        ServerPacket::NPCRepair(_) => {
            snapshot.npc_panels.repair = true;
        }
        ServerPacket::NPCSRepair(_) => {
            snapshot.npc_panels.special_repair = true;
        }
        ServerPacket::ObjectName(name) => {
            update_entity(snapshot, name.object_id, |entity| {
                entity.name.clone_from(&name.name);
            });
        }
        ServerPacket::PlayerUpdate(update) => {
            // `S.PlayerUpdate` 只在换装后发（Crystal `PlayerObject.Update`，
            // `Client/MirObjects/PlayerObject.cs:170-180`），带的就是新的外观号。
            update_entity(snapshot, update.object_id, |entity| {
                entity.looks.armour = shape_block(update.armour);
                let weapon_shape = i16_to_layer(update.weapon);
                if weapon_shape == mir_render::NO_LAYER {
                    entity.looks.weapon = mir_render::NO_LAYER;
                    entity.looks.weapon_shape = mir_render::NO_LAYER;
                } else {
                    entity.looks.weapon_shape = weapon_shape;
                    entity.looks.weapon = weapon_shape % 100;
                }
                entity.image = entity.looks.armour;
            });
        }
        ServerPacket::Disconnect(disconnect) => {
            snapshot.status = NetStatus::Failed;
            snapshot.last_error = Some(format!("服务器断开连接（reason = {}）", disconnect.reason));
        }
        _ => {}
    }
    // 装备/模板/职业任一处变化都会影响自己的外观；每包重算一次（14 个槽位，开销可忽略），
    // 「装备先到、模板后到」这类顺序也能最终收敛。
    snapshot.refresh_self_looks();
}

/// 插入或更新一个实体。
fn upsert_entity(snapshot: &mut WorldSnapshot, entity: WorldEntity) {
    snapshot.entities.insert(entity.object_id, entity);
}

/// 修改一个实体；不存在时什么也不做。
fn update_entity(
    snapshot: &mut WorldSnapshot,
    object_id: u32,
    update: impl FnOnce(&mut WorldEntity),
) {
    if let Some(entity) = snapshot.entities.get_mut(&object_id) {
        update(entity);
    } else if let Some(me) = &mut snapshot.self_player {
        // 自己的 ObjectWalk/ObjectRun 在协议里只通过 UserLocation 回显；这里保留兜底，
        // 万一带了自己的 ID 也不会丢坐标。
        if me.object_id == object_id {
            // 用一个临时实体承接更新，再写回自己（字段集合不同，简单映射坐标/朝向）。
            let mut temporary = WorldEntity {
                dead: me.dead,
                looks: me.looks,
                ..WorldEntity::new(
                    object_id,
                    EntityKind::Player,
                    me.name.clone(),
                    me.position,
                    me.direction,
                    0,
                )
            };
            update(&mut temporary);
            me.position = temporary.position;
            me.direction = temporary.direction;
            me.dead = temporary.dead;
            me.action = temporary.action;
            me.action_started_ms = temporary.action_started_ms;
        }
    }
}

/// 追加聊天记录并裁剪到上限。
fn push_chat(snapshot: &mut WorldSnapshot, line: ChatLine) {
    snapshot.chat.push(line);
    if snapshot.chat.len() > CHAT_LOG_LIMIT {
        snapshot.chat.remove(0);
    }
}

/// 背包内交换两个格子（`S.MoveItem`）。
fn snap_move_inventory(snapshot: &mut WorldSnapshot, from: i32, to: i32) {
    let (Ok(from), Ok(to)) = (usize::try_from(from), usize::try_from(to)) else {
        return;
    };
    if from >= snapshot.inventory.len() || to >= snapshot.inventory.len() {
        return;
    }
    snapshot.inventory.swap(from, to);
}

/// 把一件物品放进背包：可堆叠就先并进已有堆，否则找第一个空格。
fn add_inventory_item(snapshot: &mut WorldSnapshot, item: InventoryItem) {
    let stack_size = snapshot
        .item_infos
        .get(&item.item_index)
        .map_or(1, |info| info.stack_size.max(1));
    if stack_size > 1 {
        let existing =
            snapshot.inventory.iter_mut().flatten().find(|existing| {
                existing.item_index == item.item_index && existing.count < stack_size
            });
        if let Some(existing) = existing {
            let room = stack_size.saturating_sub(existing.count);
            existing.count = existing.count.saturating_add(room.min(item.count));
            return;
        }
    }
    if let Some(slot) = snapshot.inventory.iter_mut().find(|slot| slot.is_none()) {
        *slot = Some(item);
    } else {
        tracing::warn!(
            item_index = item.item_index,
            unique_id = item.unique_id,
            "背包已满，丢弃新物品的界面记录"
        );
    }
}

/// 按唯一编号移除背包物品。
fn remove_inventory_item(snapshot: &mut WorldSnapshot, unique_id: u64) {
    if let Some(slot) = snapshot.inventory_slot_of(unique_id) {
        if let Some(slot) = snapshot.inventory.get_mut(slot) {
            *slot = None;
        }
    }
}

/// 按数量移除背包物品（`S.DeleteItem`）。
fn delete_inventory_item(snapshot: &mut WorldSnapshot, unique_id: u64, count: u16) {
    let Some(slot) = snapshot.inventory_slot_of(unique_id) else {
        return;
    };
    let Some(item) = snapshot.inventory.get_mut(slot).and_then(Option::as_mut) else {
        return;
    };
    if count >= item.count || item.count <= 1 {
        if let Some(slot) = snapshot.inventory.get_mut(slot) {
            *slot = None;
        }
    } else {
        item.count -= count;
    }
}

/// 用带新数据的 `S.RefreshItem` 更新背包里的同一件物品。
fn refresh_inventory_item(snapshot: &mut WorldSnapshot, item: &UserItem) {
    if let Some(slot) = snapshot.inventory_slot_of(item.unique_id) {
        if let Some(existing) = snapshot.inventory.get_mut(slot) {
            *existing = Some(InventoryItem::from_user_item(item));
        }
    }
}

/// 把一个背包物品穿到 `slot` 号装备槽；换下来的旧装备回到原背包格（空着时）或并入背包。
fn equip_from_inventory(snapshot: &mut WorldSnapshot, unique_id: u64, slot: i32) {
    let Ok(slot) = usize::try_from(slot) else {
        return;
    };
    if slot >= EQUIPMENT_SLOTS {
        return;
    }
    let Some(inventory_slot) = snapshot.inventory_slot_of(unique_id) else {
        return;
    };
    let Some(item) = snapshot
        .inventory
        .get_mut(inventory_slot)
        .and_then(Option::take)
    else {
        return;
    };
    if snapshot.equipment.len() < EQUIPMENT_SLOTS {
        snapshot.equipment.resize(EQUIPMENT_SLOTS, None);
    }
    let replaced = snapshot
        .equipment
        .get_mut(slot)
        .and_then(|target| target.replace(item));
    if let Some(replaced) = replaced {
        if snapshot
            .inventory
            .get(inventory_slot)
            .is_some_and(Option::is_none)
        {
            snapshot.inventory[inventory_slot] = Some(replaced);
        } else {
            add_inventory_item(snapshot, replaced);
        }
    }
}

/// 把一件装备脱到 `slot` 号背包格；目标格被占时并入第一个空位。
fn unequip_to_inventory(snapshot: &mut WorldSnapshot, unique_id: u64, slot: i32) {
    let Some(equipment_slot) = snapshot.equipment_slot_of(unique_id) else {
        return;
    };
    let Some(item) = snapshot
        .equipment
        .get_mut(equipment_slot)
        .and_then(Option::take)
    else {
        return;
    };
    if let Ok(slot) = usize::try_from(slot) {
        if slot < snapshot.inventory.len() && snapshot.inventory[slot].is_none() {
            snapshot.inventory[slot] = Some(item);
            return;
        }
    }
    add_inventory_item(snapshot, item);
}

/// 追加一条伤害事件（UI 用递增 ID 判断哪些是新的）。
fn push_damage_event(
    snapshot: &mut WorldSnapshot,
    object_id: u32,
    damage: Option<i32>,
    damage_type: Option<DamageType>,
    location: Option<Point>,
) {
    snapshot.next_damage_id = snapshot.next_damage_id.saturating_add(1);
    let id = snapshot.next_damage_id;
    snapshot.damage_events.push(DamageEvent {
        id,
        object_id,
        damage,
        damage_type,
        location,
    });
    if snapshot.damage_events.len() > DAMAGE_EVENT_LIMIT {
        snapshot.damage_events.remove(0);
    }
}

/// 追加一条光效事件（`S.ObjectEffect`）。
fn push_effect_event(snapshot: &mut WorldSnapshot, object_id: u32, effect: SpellEffect, time: u32) {
    snapshot.next_effect_id = snapshot.next_effect_id.saturating_add(1);
    let id = snapshot.next_effect_id;
    snapshot.effects.push(EffectEvent {
        id,
        object_id,
        effect,
        time,
    });
    if snapshot.effects.len() > EFFECT_EVENT_LIMIT {
        snapshot.effects.remove(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir_core::enums::EquipmentSlot;
    use mir_core::enums::Monster as MonsterImage;
    use mir_protocol::data::{ClientBuff, ClientMagic, ItemInfo, SelectInfo, Stats, UserItem};
    use mir_protocol::packets::server::{
        AddBuff, DamageIndicator, DeleteItem, EquipItem, MoveItem, NPCGoods, NPCResponse,
        NewItemInfo, ObjectEffect, ObjectItem, ObjectMonster, ObjectNPC, ObjectPlayer,
        ObjectStruck, ObjectWalk, PlayerUpdate, RemoveBuff, RemoveItem, UserInformation,
        UserLocation,
    };

    fn test_item(unique_id: u64, item_index: i32, count: u16) -> UserItem {
        UserItem {
            unique_id,
            item_index,
            count,
            ..UserItem::default()
        }
    }

    fn test_item_info(index: i32, name: &str, price: u32, stack_size: u16) -> ItemInfo {
        ItemInfo {
            index,
            name: name.to_owned(),
            image: u16::try_from(index).unwrap_or(0),
            price,
            stack_size,
            ..ItemInfo::default()
        }
    }

    fn user_information() -> UserInformation {
        UserInformation {
            object_id: 7,
            real_id: 1,
            name: "测试者".to_owned(),
            location: Point::new(330, 270),
            direction: MirDirection::Down,
            class: MirClass::Warrior,
            gender: MirGender::Male,
            level: 1,
            hair: 1,
            hp: 15,
            mp: 15,
            gold: 0,
            ..UserInformation::default()
        }
    }

    #[test]
    fn snapshot_handles_login_and_entities() {
        let mut snapshot = WorldSnapshot::default();
        snapshot.apply_packet(&ServerPacket::UserInformation(user_information()));
        assert_eq!(snapshot.packet_count("UserInformation"), 1);
        let me = snapshot.self_player.as_ref().expect("自己");
        assert_eq!(me.object_id, 7);
        assert_eq!(me.position, Point::new(330, 270));

        snapshot.apply_packet(&ServerPacket::ObjectMonster(ObjectMonster {
            object_id: 8,
            name: "骷髅".to_owned(),
            image: MonsterImage::Skeleton,
            location: Point::new(331, 271),
            direction: MirDirection::Left,
            ..ObjectMonster::default()
        }));
        snapshot.apply_packet(&ServerPacket::ObjectItem(ObjectItem {
            object_id: 9,
            name: "金币".to_owned(),
            location: Point::new(330, 271),
            image: 112,
            ..ObjectItem::default()
        }));
        assert_eq!(snapshot.entities.len(), 2);
        assert_eq!(snapshot.entity_views(0).len(), 3);

        snapshot.apply_packet(&ServerPacket::ObjectWalk(ObjectWalk {
            object_id: 8,
            location: Point::new(332, 271),
            direction: MirDirection::Right,
        }));
        assert_eq!(
            snapshot.entities[&8].position,
            Point::new(332, 271),
            "ObjectWalk 应更新怪物坐标"
        );

        snapshot.apply_packet(&ServerPacket::UserLocation(UserLocation {
            location: Point::new(330, 271),
            direction: MirDirection::Down,
        }));
        assert_eq!(
            snapshot.self_player.as_ref().unwrap().position,
            Point::new(330, 271)
        );

        snapshot.apply_packet(&ServerPacket::ObjectRemove(
            mir_protocol::packets::server::ObjectRemove { object_id: 8 },
        ));
        assert!(!snapshot.entities.contains_key(&8));
    }

    #[test]
    fn chat_log_is_capped_and_formats_speaker() {
        let mut snapshot = WorldSnapshot::default();
        for index in 0..(CHAT_LOG_LIMIT + 5) {
            push_chat(
                &mut snapshot,
                ChatLine {
                    speaker: None,
                    text: format!("第 {index} 条"),
                    kind: ChatType::System,
                },
            );
        }
        assert_eq!(snapshot.chat.len(), CHAT_LOG_LIMIT);
        assert_eq!(snapshot.chat.last().unwrap().text, "第 104 条");
        let line = ChatLine {
            speaker: Some("甲".to_owned()),
            text: "你好".to_owned(),
            kind: ChatType::Normal,
        };
        assert_eq!(line.display(), "[甲] 你好");
    }

    #[test]
    fn find_character_matches_exact_then_case_insensitive() {
        let characters = vec![
            SelectInfo {
                index: 3,
                name: "Hero".to_owned(),
                ..SelectInfo::default()
            },
            SelectInfo {
                index: 4,
                name: "mage".to_owned(),
                ..SelectInfo::default()
            },
        ];
        assert_eq!(
            find_character(&characters, "Hero").map(|c| c.index),
            Some(3)
        );
        assert_eq!(
            find_character(&characters, "MAGE").map(|c| c.index),
            Some(4)
        );
        assert!(find_character(&characters, "missing").is_none());
    }

    #[test]
    fn inventory_tracks_user_information_gain_move_and_delete() {
        let mut snapshot = WorldSnapshot::default();
        snapshot.apply_packet(&ServerPacket::NewItemInfo(NewItemInfo {
            info: test_item_info(1, "金创药", 50, 100),
        }));

        let mut information = user_information();
        information.inventory = Some(vec![
            Some(test_item(11, 1, 3)),
            None,
            Some(test_item(12, 2, 1)),
        ]);
        information.magics = vec![ClientMagic {
            name: "治愈术".to_owned(),
            spell: Spell::Healing,
            level: 1,
            key: 1,
            base_cost: 5,
            ..ClientMagic::default()
        }];
        snapshot.apply_packet(&ServerPacket::UserInformation(information));
        assert_eq!(snapshot.inventory.len(), 3);
        assert_eq!(snapshot.inventory_item(0).map(|item| item.count), Some(3));
        assert_eq!(snapshot.magics[0].name, "治愈术");
        assert_eq!(snapshot.magics[0].spell, Spell::Healing);
        assert_eq!(snapshot.item_info(1).map(|info| info.price), Some(50));

        // 同模板可堆叠物品并入已有堆（3 + 5 = 8）。
        snapshot.apply_packet(&ServerPacket::GainedItem(
            mir_protocol::packets::server::GainedItem {
                item: test_item(11, 1, 5),
            },
        ));
        assert_eq!(snapshot.inventory_item(0).map(|item| item.count), Some(8));

        // 背包内交换。
        snapshot.apply_packet(&ServerPacket::MoveItem(MoveItem {
            grid: MirGridType::Inventory,
            from: 0,
            to: 1,
            success: true,
        }));
        assert!(snapshot.inventory_item(0).is_none());
        assert_eq!(
            snapshot.inventory_item(1).map(|item| item.unique_id),
            Some(11)
        );

        // 删除整堆。
        snapshot.apply_packet(&ServerPacket::DeleteItem(DeleteItem {
            unique_id: 11,
            count: 8,
        }));
        assert!(snapshot.inventory_item(1).is_none());
        assert_eq!(snapshot.packet_count("DeleteItem"), 1);
    }

    #[test]
    fn npc_dialog_and_shop_resolve_names_prices_and_nearest_npc() {
        let mut snapshot = WorldSnapshot::default();
        snapshot.apply_packet(&ServerPacket::UserInformation(user_information()));
        snapshot.apply_packet(&ServerPacket::ObjectNPC(ObjectNPC {
            object_id: 42,
            name: "测试商人".to_owned(),
            image: 5,
            location: Point::new(331, 270),
            direction: MirDirection::Down,
            ..ObjectNPC::default()
        }));
        snapshot.apply_packet(&ServerPacket::NPCResponse(NPCResponse {
            page: vec!["欢迎光临".to_owned(), "<买东西/@BUY>".to_owned()],
        }));
        let dialog = snapshot.npc_dialog.as_ref().expect("对话页");
        assert_eq!(dialog.lines.len(), 2);
        assert_eq!(dialog.object_id, Some(42), "应取离自己最近的 NPC");
        assert_eq!(dialog.version, 1);

        snapshot.apply_packet(&ServerPacket::NewItemInfo(NewItemInfo {
            info: test_item_info(2, "木剑", 100, 1),
        }));
        snapshot.apply_packet(&ServerPacket::NPCGoods(NPCGoods {
            list: vec![test_item(99, 2, 1)],
            rate: 1.0,
            type_: PanelType::Buy,
            hide_added_stats: true,
        }));
        let goods = snapshot.npc_goods.as_ref().expect("货架");
        assert_eq!(goods.list[0].name, "木剑");
        assert_eq!(goods.list[0].price, 100);
        assert_eq!(goods.list[0].unique_id, 99);
        assert_eq!(goods.version, 1);

        // 空正文 = 服务端确认关窗，货架也要收起。
        snapshot.apply_packet(&ServerPacket::NPCResponse(NPCResponse { page: vec![] }));
        let dialog = snapshot.npc_dialog.as_ref().expect("空页仍在快照里");
        assert!(dialog.lines.is_empty());
        assert_eq!(dialog.version, 2);
        assert!(snapshot.npc_goods.is_none());
    }

    #[test]
    fn magic_leveled_updates_the_skill_list() {
        let mut snapshot = WorldSnapshot::default();
        let mut information = user_information();
        information.magics = vec![ClientMagic {
            name: "火球术".to_owned(),
            spell: Spell::FireBall,
            level: 1,
            key: 2,
            base_cost: 3,
            ..ClientMagic::default()
        }];
        snapshot.apply_packet(&ServerPacket::UserInformation(information));
        snapshot.apply_packet(&ServerPacket::MagicLeveled(
            mir_protocol::packets::server::MagicLeveled {
                object_id: 7,
                spell: Spell::FireBall,
                level: 2,
                experience: 10,
            },
        ));
        assert_eq!(snapshot.magics[0].level, 2);
        assert_eq!(snapshot.packet_count("MagicLeveled"), 1);
    }

    #[test]
    fn snapshot_tracks_equipment_buffs_damage_and_effects() {
        let mut snapshot = WorldSnapshot::default();
        let mut information = user_information();
        information.inventory = Some(vec![Some(test_item(11, 1, 1)), None]);
        information.equipment = Some(vec![None, Some(test_item(21, 10, 1))]);
        snapshot.apply_packet(&ServerPacket::UserInformation(information));
        assert_eq!(snapshot.equipment.len(), 2);
        assert_eq!(
            snapshot.equipment_item(1).map(|item| item.unique_id),
            Some(21)
        );

        // 穿上：背包 0 号格的物品进入 0 号装备槽。
        snapshot.apply_packet(&ServerPacket::EquipItem(EquipItem {
            grid: MirGridType::Inventory,
            unique_id: 11,
            to: 0,
            success: true,
        }));
        assert!(snapshot.inventory_item(0).is_none());
        assert_eq!(
            snapshot.equipment_item(0).map(|item| item.unique_id),
            Some(11)
        );
        // 脱下：装备槽 1 的旧装备回到背包 0 号格。
        snapshot.apply_packet(&ServerPacket::RemoveItem(RemoveItem {
            grid: MirGridType::Equipment,
            unique_id: 21,
            to: 0,
            success: true,
        }));
        assert!(snapshot.equipment_item(1).is_none());
        assert_eq!(
            snapshot.inventory_item(0).map(|item| item.unique_id),
            Some(21)
        );

        // buff 的新增/倒计时/移除。
        snapshot.apply_packet(&ServerPacket::AddBuff(AddBuff {
            buff: ClientBuff {
                buff_type: BuffType::MagicShield,
                visible: true,
                object_id: 7,
                expire_time: 10_000,
                infinite: false,
                paused: false,
                stats: Stats::default(),
                values: Vec::new(),
            },
        }));
        assert_eq!(snapshot.buffs.len(), 1);
        assert_eq!(snapshot.buffs[0].remaining_seconds(2_000), Some(8));
        snapshot.apply_packet(&ServerPacket::RemoveBuff(RemoveBuff {
            type_: BuffType::MagicShield,
            object_id: 7,
        }));
        assert!(snapshot.buffs.is_empty());

        // 伤害/受击事件与光效事件。
        snapshot.apply_packet(&ServerPacket::ObjectMonster(ObjectMonster {
            object_id: 8,
            name: "骷髅".to_owned(),
            image: MonsterImage::Skeleton,
            location: Point::new(1, 1),
            direction: MirDirection::Down,
            ..ObjectMonster::default()
        }));
        snapshot.apply_packet(&ServerPacket::ObjectStruck(ObjectStruck {
            object_id: 8,
            attacker_id: 7,
            location: Point::new(1, 1),
            direction: MirDirection::Left,
        }));
        snapshot.apply_packet(&ServerPacket::DamageIndicator(DamageIndicator {
            damage: 12,
            type_: DamageType::Hit,
            object_id: 8,
        }));
        assert_eq!(snapshot.damage_events.len(), 2);
        assert_eq!(snapshot.damage_events[0].damage, None, "受击包没有伤害值");
        assert_eq!(snapshot.damage_events[1].damage, Some(12));
        assert!(snapshot.damage_events[0].id < snapshot.damage_events[1].id);
        assert_eq!(snapshot.object_position(8), Some(Point::new(1, 1)));
        snapshot.apply_packet(&ServerPacket::ObjectEffect(ObjectEffect {
            object_id: 8,
            effect: SpellEffect::MagicShieldUp,
            effect_type: 0,
            delay_time: 0,
            time: 600,
        }));
        assert_eq!(snapshot.effects.len(), 1);
        assert_eq!(snapshot.effects[0].object_id, 8);
        assert_eq!(snapshot.packet_count("DamageIndicator"), 1);
        assert_eq!(snapshot.packet_count("ObjectEffect"), 1);
    }

    /// 带外观字段（`shape`）与类型的物品模板。
    fn equipment_info(index: i32, name: &str, item_type: ItemType, shape: i16) -> ItemInfo {
        ItemInfo {
            index,
            name: name.to_owned(),
            image: u16::try_from(index).unwrap_or(0),
            item_type,
            shape,
            durability: 5_000,
            ..ItemInfo::default()
        }
    }

    /// 14 个槽位的空装备表。
    fn empty_equipment() -> Vec<Option<UserItem>> {
        vec![None; EQUIPMENT_SLOTS]
    }

    /// 一件有持久度的物品：Crystal 会把「持久度 0 且模板有持久度」的装备当成没穿
    /// （`UserObject.cs:234`），所以测试里的装备必须带持久度。
    fn worn_item(unique_id: u64, item_index: i32) -> UserItem {
        UserItem {
            current_dura: 5_000,
            max_dura: 5_000,
            ..test_item(unique_id, item_index, 1)
        }
    }

    #[test]
    fn equipment_drives_the_paper_doll_appearance() {
        let mut snapshot = WorldSnapshot::default();
        let mut information = user_information();
        information.class = MirClass::Taoist;
        information.gender = MirGender::Female;
        information.hair = 3;
        let mut equipment = empty_equipment();
        equipment[EquipmentSlot::Weapon as usize] = Some(worn_item(31, 10));
        equipment[EquipmentSlot::Armour as usize] = Some(worn_item(32, 11));
        information.equipment = Some(equipment);
        snapshot.apply_packet(&ServerPacket::UserInformation(information));

        // 模板还没到：外观退化成裸体 + 空手，但发型已经来自 UserInformation。
        let looks = snapshot.self_player.as_ref().expect("自己").looks;
        assert_eq!(looks.armour, 0, "没有模板时不该瞎猜外观");
        assert!(!looks.has_weapon());
        assert_eq!(looks.hair, 3);
        assert_eq!(looks.class, MirClass::Taoist);
        assert_eq!(looks.gender, MirGender::Female);

        // `S.NewItemInfo` 到达 → 外观自动重算（不依赖包序）。
        snapshot.apply_packet(&ServerPacket::NewItemInfo(NewItemInfo {
            info: equipment_info(10, "炼狱", ItemType::Weapon, 36),
        }));
        snapshot.apply_packet(&ServerPacket::NewItemInfo(NewItemInfo {
            info: equipment_info(11, "重盔甲", ItemType::Armour, 2),
        }));
        let looks = snapshot.self_player.as_ref().expect("自己").looks;
        assert_eq!(looks.armour, 2, "衣服外观号 = ItemInfo.Shape");
        assert_eq!(looks.weapon, 36, "武器块号 = Shape");
        assert_eq!(looks.weapon_shape, 36, "普通武器的原始形状与块号相同");
        assert_eq!(looks.hair, 3);
        assert!(looks.has_weapon());
        assert_eq!(looks.shield, mir_render::NO_LAYER, "本作没有盾牌物品类型");

        // 渲染层拿到的是同一套外观。
        let views = snapshot.entity_views(0);
        let me = views.first().expect("自己永远排第一");
        assert_eq!(me.kind, EntityKind::Player);
        assert_eq!(me.armour, 2);
        assert_eq!(me.weapon, 36);
        assert_eq!(me.weapon_shape, 36);
        assert_eq!(me.hair, 3);
    }

    #[test]
    fn equipping_and_broken_equipment_refresh_the_appearance() {
        let mut snapshot = WorldSnapshot::default();
        let mut information = user_information();
        information.hair = 1;
        information.inventory = Some(vec![Some(worn_item(41, 20)), None]);
        information.equipment = Some(empty_equipment());
        snapshot.apply_packet(&ServerPacket::UserInformation(information));
        // 弓箭手武器（200 段）→ `weapon2.wzl` 之类的高段位图库，块号取 `% 100`。
        snapshot.apply_packet(&ServerPacket::NewItemInfo(NewItemInfo {
            info: equipment_info(20, "屠龙", ItemType::Weapon, 210),
        }));
        snapshot.apply_packet(&ServerPacket::NewItemInfo(NewItemInfo {
            info: equipment_info(21, "布衣", ItemType::Armour, 0),
        }));
        assert!(!snapshot.self_player.as_ref().unwrap().looks.has_weapon());

        // 穿上武器（背包 0 → 武器槽 0）：外观立刻刷新。
        snapshot.apply_packet(&ServerPacket::EquipItem(EquipItem {
            grid: MirGridType::Inventory,
            unique_id: 41,
            to: EquipmentSlot::Weapon as i32,
            success: true,
        }));
        let looks = snapshot.self_player.as_ref().unwrap().looks;
        assert_eq!(looks.weapon_shape, 210);
        assert_eq!(
            looks.weapon, 10,
            "200 段武器在 weapon*.wzl 里的块号是 Shape % 100"
        );

        // 持久度归零的装备在 Crystal 里不参与外观（`UserObject.cs:234`）。
        if let Some(item) = snapshot.equipment_item(0) {
            let mut broken = item.clone();
            broken.current_dura = 0;
            snapshot.equipment[0] = Some(broken);
        }
        snapshot.apply_packet(&ServerPacket::HealthChanged(
            mir_protocol::packets::server::HealthChanged { hp: 1, mp: 1 },
        ));
        assert!(
            !snapshot.self_player.as_ref().unwrap().looks.has_weapon(),
            "持久度为 0 的武器不算外观"
        );

        // 脱下来 → 又回到空手。
        snapshot.equipment[0] = None;
        snapshot.apply_packet(&ServerPacket::RemoveItem(RemoveItem {
            grid: MirGridType::Equipment,
            unique_id: 41,
            to: 0,
            success: true,
        }));
        assert!(!snapshot.self_player.as_ref().unwrap().looks.has_weapon());
        assert_eq!(snapshot.self_player.as_ref().unwrap().looks.armour, 0);
    }

    #[test]
    fn other_players_appearance_comes_from_object_player_and_player_update() {
        let mut snapshot = WorldSnapshot::default();
        snapshot.apply_packet(&ServerPacket::ObjectPlayer(ObjectPlayer {
            object_id: 5,
            name: "路人".to_owned(),
            class: MirClass::Archer,
            gender: MirGender::Female,
            level: 30,
            location: Point::new(12, 13),
            direction: MirDirection::Right,
            hair: 4,
            weapon: 218,
            armour: 37,
            ..ObjectPlayer::default()
        }));
        let view = snapshot
            .entity_views(0)
            .into_iter()
            .find(|view| view.kind == EntityKind::Player)
            .expect("路人玩家");
        assert_eq!(view.armour, 37);
        assert_eq!(view.weapon_shape, 218);
        assert_eq!(view.weapon, 18);
        assert_eq!(view.hair, 4);
        let looks = snapshot.entities[&5].looks;
        assert_eq!(looks.class, MirClass::Archer);
        assert_eq!(looks.gender, MirGender::Female);

        // 换装后服务端发 `S.PlayerUpdate`，客户端跟着换外观。
        snapshot.apply_packet(&ServerPacket::PlayerUpdate(PlayerUpdate {
            object_id: 5,
            light: 0,
            weapon: -1,
            weapon_effect: 0,
            armour: 6,
            wing_effect: 0,
        }));
        let view = snapshot
            .entity_views(0)
            .into_iter()
            .find(|view| view.kind == EntityKind::Player)
            .expect("路人玩家");
        assert_eq!(view.armour, 6);
        assert_eq!(view.weapon, mir_render::NO_LAYER, "Weapon = -1 表示空手");
        assert_eq!(view.weapon_shape, mir_render::NO_LAYER);
        assert_eq!(view.hair, 4, "PlayerUpdate 不改发型");
    }
}
