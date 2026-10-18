//! NPC：实体、脚本对话、商店买卖与修理。
//!
//! 移植自 Crystal 的四个文件：
//!
//! | 本模块 | Crystal | 内容 |
//! |---|---|---|
//! | [`Npc`] | `Server/MirObjects/NPCObject.cs:8-459` | 地图上的 NPC 对象（位置、外观、可见性限制） |
//! | [`World::call_npc`] | `NPCScript.Call(PlayerObject, objectID, key)`（`NPC/NPCScript.cs:827-893`）、`PlayerObject.CallNPC`（`PlayerObject.cs:7900-7942`） | 脚本执行、`S.NPCResponse` |
//! | [`World::buy_item`] | `PlayerObject.BuyItem`（`:7944-7969`）+ `NPCScript.Buy`（`:1138-1237`） | 买 |
//! | [`World::sell_item`] | `PlayerObject.SellItem`（`:7988-8091`） | 卖 |
//! | [`World::repair_item`] | `PlayerObject.RepairItem`（`:8092-8160`） | 修 |
//! | [`item_price`] / [`repair_price`] | `UserItem.Price` / `UserItem.RepairPrice`（`Shared/Data/ItemData.cs:516-563`） | 定价公式 |
//! | [`World::map_entry_info`] | `PlayerObject.CheckMapInfo`（`:1009-1056`） | 进图时下发 `S.NewMapInfo`（含 `NPCs` 与 `Movements`） |
//!
//! 脚本语法与条件/动作支持矩阵见 `docs/world/npc.md`；解析器本身在
//! [`mir_db::npc_script`]（纯数据、无服务端依赖）。
//!
//! # 为什么「NPC 列表」走 `NewMapInfo` 而不是 `MapInformation`
//!
//! Crystal 的 `ServerPackets.MapInformation`（`Shared/ServerPackets.cs:469-517`）里**没有**
//! `NPCs` / `Movements` 字段：进图时的这两组数据由 `S.NewMapInfo`（`:519-540`）携带的
//! `ClientMapInfo`（`Shared/Data/ClientData.cs:320-357`）下发，随后视野内的 NPC 再逐个用
//! `S.ObjectNPC`（`:381-395`）补齐外观。本作严格照此实现，因此 `MapInformation` 的线格式
//! 保持不变（它是 Crystal 客户端的既有协议，追加字段会让真实客户端解包错位）。

use std::collections::BTreeSet;
use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{BindMode, ChatType, ItemType, MirClass, MirDirection, MirGender, PanelType};
use mir_db::npc_script::{
    CompareOp, EXIT_SECTION, MAIN_SECTION, NpcAction, NpcActionKind, NpcCheck, NpcCheckKind,
    NpcPage, NpcScript,
};
use mir_db::{MapInfo, NpcInfo};
use mir_protocol::ServerPacket;
use mir_protocol::data::{ClientMapInfo, ClientMovementInfo, ClientNPCInfo, ItemInfo, UserItem};
use mir_protocol::packets::server::{
    NPCGoods, NPCRepair, NPCResponse, NPCSRepair, NPCSell, ObjectNPC, SellItem,
};

use crate::items::item_weight;
use crate::world::{Player, World};

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

/// NPC 对话/交易的距离上限：Crystal `Globals.DataRange = 16`
/// （`Shared/Globals.cs:37`），`Functions.InRange` 是切比雪夫距离
/// （`Shared/Functions/Functions.cs`：`|dx| <= i && |dy| <= i`）。
///
/// 注意它与 [`crate::world::VIEW_RANGE`]（10）不是一回事：后者是本作简化的广播半径，
/// 这里用的是 Crystal 原文里的 NPC 交互距离。
pub const NPC_TALK_RANGE: i32 = 16;

/// 一次对话最多跟进多少个脚本 `GOTO`（防脚本自跳导致的死循环）。
/// Crystal 靠 `ActionList` 的 `Time == -1` 逐个处理（`PlayerObject.cs:7930-7942`），没有上限。
pub const MAX_NPC_HOPS: usize = 8;

/// 标记位数量上限：Crystal `Globals.FlagIndexCount = 1999`（`Shared/Globals.cs:31`）。
pub const FLAG_INDEX_COUNT: i32 = 1999;

/// NPC 名字颜色：Crystal 构造函数写死 `NameColour = Color.Lime`
/// （`NPCObject.cs:50`），`ToArgb()` 后是 `0xFF00FF00`，即 `-16_711_936`。
pub const NPC_NAME_COLOUR: i32 = -16_711_936;

/// `S.NPCGoods.HideAddedStats`：Crystal `Settings.GoodsHideAddedStats = true`
/// （`Server/Settings.cs:286`）。
pub const GOODS_HIDE_ADDED_STATS: bool = true;

/// 买东西的页面（Crystal `BuyKey`，`NPCScript.cs:45`）。
pub const BUY_KEY: &str = "[@BUY]";
/// 买卖同页（`BuySellKey`，`:47`）。
pub const BUY_SELL_KEY: &str = "[@BUYSELL]";
/// 卖东西的页面（`SellKey`，`:46`）。
pub const SELL_KEY: &str = "[@SELL]";
/// 修理（`RepairKey`，`:48`）。
pub const REPAIR_KEY: &str = "[@REPAIR]";
/// 特修（`SRepairKey`，`:49`）。
pub const SPECIAL_REPAIR_KEY: &str = "[@SREPAIR]";
/// 仓库（`StorageKey`，`:55`）。
///
/// 点这个按钮时服务端发 `S.UserStorage` + `S.NPCStorage`（`NPCScript.cs:1002-1006`），
/// 见 [`crate::storage`]。
pub const STORAGE_KEY: &str = "[@STORAGE]";
/// 写信（`SendParcelKey`，`NPCScript.cs:62`）：服务端回一个空的 `S.MailSendRequest`
/// （`:1078-1080`），客户端据此打开写信窗口。见 [`crate::mail`]。
pub const SEND_PARCEL_KEY: &str = "[@SENDPARCEL]";
/// 邮局受理包裹（`CollectParcelKey`，`NPCScript.cs:63`）。
///
/// 服务端把**所有包裹**标记为已受理并回 `S.ParcelCollected`（`:1081-1098`）。
pub const COLLECT_PARCEL_KEY: &str = "[@COLLECTPARCEL]";
/// 创建行会的页面（`GuildCreateKey`，`NPCScript.cs:60`）。
///
/// 点这个按钮时服务端**不打开面板**，而是发 `S.GuildNameRequest` 让客户端弹输入框
/// （`NPCScript.cs:1050-1062`），玩家回 `C.GuildNameReturn{Name}` 后再创建
/// （`PlayerObject.cs:10029-10060`）。见 [`crate::guild`]。
pub const GUILD_CREATE_KEY: &str = "[@CREATEGUILD]";

/// 可以「买」的页面（Crystal `PlayerObject.BuyItem`，`:7948-7955`）。
pub const BUY_PAGES: &[&str] = &[
    BUY_SELL_KEY,
    BUY_KEY,
    "[@BUYBACK]",
    "[@BUYUSED]",
    "[@PEARLBUY]",
    "[@BUYNEW]",
    "[@BUYSELLNEW]",
];

/// 可以「卖」的页面（Crystal `PlayerObject.SellItem`，`:7998`）。
pub const SELL_PAGES: &[&str] = &[BUY_SELL_KEY, SELL_KEY];

/// 可以「修」的页面（Crystal `PlayerObject.RepairItem`，`:8098`）。
pub const REPAIR_PAGES: &[&str] = &[REPAIR_KEY];

/// 特修用的是另一个页面（`PlayerObject.RepairItem(..., special: true)`，`:8098`）。
pub const SPECIAL_REPAIR_PAGES: &[&str] = &[SPECIAL_REPAIR_KEY];

// ---------------------------------------------------------------------------
// 玩家侧的对话状态
// ---------------------------------------------------------------------------

/// 一个玩家与 NPC 交互时的状态。
///
/// 字段与 Crystal `PlayerObject` / `CharacterInfo` 上的分散成员对应：
///
/// | 本结构 | Crystal |
/// |---|---|
/// | [`NpcState::npc_object_id`] | `PlayerObject.NPCObjectID`（`PlayerObject.cs:122`） |
/// | [`NpcState::page_key`] | `PlayerObject.NPCPage.Key` |
/// | [`NpcState::buttons`] | `PlayerObject.NPCSuccess`（每段「检查结果 → 可见按钮」的映射，`NPCScript.cs:839-847`） |
/// | [`NpcState::flags`] | `CharacterInfo.Flags`（`Globals.FlagIndexCount` 个 bool） |
///
/// 任务状态不在这里：`CHECKQUEST` 读的是 [`Player::quests`] / [`Player::completed_quests`]
/// （Crystal `PlayerObject.CurrentQuests` / `CompletedQuests`，见 `crate::quests`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NpcState {
    /// 当前正在对话的 NPC 对象 ID；`0` = 没在对话。
    pub npc_object_id: u32,
    /// 当前对话页的 key（大写、带方括号）。
    pub page_key: String,
    /// 当前页里「结果匹配的分支」暴露出来的按钮白名单。
    pub buttons: BTreeSet<String>,
    /// NPC 脚本标记位（Crystal `Info.Flags`，只有 `true` 的下标才在集合里）。
    pub flags: BTreeSet<i32>,
    /// `NeedMove` 传送点记下的目标地图文件名（Crystal `NPCData["NPCMoveMap"]`，
    /// `Server/MirObjects/PlayerObject.cs:4497`），由 NPC 动作 `ENTERMAP` 消费
    /// （`NPCSegment.cs:4065-4074`）。
    pub move_map: Option<String>,
    /// 见 [`NpcState::move_map`]（Crystal `NPCData["NPCMoveCoord"]`，`:4498`）。
    pub move_coord: Option<Point>,
    /// 是否已经答应「可以创建行会」（Crystal `PlayerObject.CanCreateGuild`，`PlayerObject.cs:141`）。
    ///
    /// 只有点了 `[@CREATEGUILD]` 页面之后才置位，随后 `C.GuildNameReturn` 才被接受；
    /// 创建成功/失败都会清掉（`PlayerObject.cs:10031`、`:10059`）。
    pub can_create_guild: bool,
}

impl NpcState {
    /// 结束对话（Crystal 里没有显式函数：`NPCObjectID` 留在最后一次调用的 NPC 上，
    /// 但 `NPCSuccess.Clear()` 会让所有按钮失效）。
    pub fn close(&mut self) {
        self.npc_object_id = 0;
        self.page_key.clear();
        self.buttons.clear();
    }
}

// ---------------------------------------------------------------------------
// NPC 实体
// ---------------------------------------------------------------------------

/// 地图上的一个 NPC。
///
/// 数据来自 `Server.MirDB` 的 `NpcInfo`（`Server/MirDatabase/NPCInfo.cs:6-38`），运行期只多一个
/// `object_id` 与朝向。Crystal 的 `NPCObject` 还带 `VisibleLog` / `BuyBack` / `UsedGoods` /
/// `Speech`（`NPCObject.cs:33-45`），本作都未实现，见 `docs/world/npc.md` 的未做清单。
#[derive(Debug, Clone, PartialEq)]
pub struct Npc {
    /// 运行时对象 ID（`Envir.NextObjectID`，Crystal `MapObject.ObjectID`）。
    pub object_id: u32,
    /// 数据库主键（`NpcInfo.Index`）。
    pub index: i32,
    /// 所在地图**主键**（`NpcInfo.MapIndex`；客户端资料里的 `MapIndex` 用它，
    /// 不是地图文件名）。
    pub map_index: i32,
    /// 所在地图文件名（例如 `"0"`）。
    pub map: String,
    /// 显示名（`NpcInfo.Name`，字形里带下划线，客户端会替换成空格）。
    pub name: String,
    /// 脚本文件名（`NpcInfo.FileName`，相对 `Envir/NPCs/`，不含 `.txt`）。
    pub file_name: String,
    /// 坐标（`NpcInfo.Location`）。
    pub position: Point,
    /// 朝向：Crystal 建对象时取 `Random.Next(3)`（`NPCObject.cs:52`）。
    pub direction: MirDirection,
    /// 外观编号（`NpcInfo.Image`）。
    pub image: u16,
    /// 价格倍率基数（`NpcInfo.Rate`，百分比；`PriceRate = Rate / 100F`）。
    pub rate: u16,
    /// 是否显示在大地图（`NpcInfo.ShowOnBigMap`）。
    pub show_on_big_map: bool,
    /// 大地图图标（`NpcInfo.BigMapIcon`）。
    pub big_map_icon: i32,
    /// 是否可通过大地图传送（`NpcInfo.CanTeleportTo`）。
    pub can_teleport_to: bool,
    /// 可见所需标记位（`NpcInfo.FlagNeeded`，0 = 不限制）。
    pub flag_needed: i32,
    /// 可见所需最低等级（`NpcInfo.MinLev`，0 = 不限制）。
    pub min_lev: i16,
    /// 可见所需最高等级（`NpcInfo.MaxLev`，0 = 不限制）。
    pub max_lev: i16,
    /// 可见所需职业（`NpcInfo.ClassRequired`，空 = 不限制；取值是 `MirClass` 的名字）。
    pub class_required: String,
    /// 这个 NPC 能接/交的任务编号（`NpcInfo.CollectQuestIndexes ∪ FinishQuestIndexes`，
    /// Crystal 在 `S.ObjectNPC` 里叫 `QuestIDs`，`NPCObject.cs:392-393`）。
    pub quest_indexes: Vec<i32>,
    /// 这个 NPC 可以**发布**的任务（`NpcInfo.CollectQuestIndexes`，`NPCInfo.cs:40`）。
    ///
    /// Crystal 真正的来源是 NPC 脚本的 `[Quests]` 段（`NPCScript.ParseQuests`，
    /// `NPCScript.cs:685-721`），数据库的这两个列表是编辑器里的等价物；
    /// 本作两者都认（见 [`World::npc_gives_quest`]）。
    pub collect_quest_indexes: Vec<i32>,
    /// 这个 NPC 可以**交付**的任务（`NpcInfo.FinishQuestIndexes`，`NPCInfo.cs:41`）。
    pub finish_quest_indexes: Vec<i32>,
}

impl Npc {
    /// 从数据库模板构造（`object_id` 由 [`World::spawn_npcs`] 分配）。
    #[must_use]
    pub fn from_info(info: &NpcInfo, map: &str, object_id: u32, direction: MirDirection) -> Self {
        let mut quest_indexes = info.collect_quest_indexes.clone();
        for index in &info.finish_quest_indexes {
            if !quest_indexes.contains(index) {
                quest_indexes.push(*index);
            }
        }
        Self {
            object_id,
            index: info.index,
            map_index: info.map_index,
            map: map.to_owned(),
            name: info.name.clone(),
            file_name: info.file_name.clone(),
            position: info.location,
            direction,
            image: info.image,
            rate: info.rate,
            show_on_big_map: info.show_on_big_map,
            big_map_icon: info.big_map_icon,
            can_teleport_to: info.can_teleport_to,
            flag_needed: info.flag_needed,
            min_lev: info.min_lev,
            max_lev: info.max_lev,
            class_required: info.class_required.clone(),
            quest_indexes,
            collect_quest_indexes: info.collect_quest_indexes.clone(),
            finish_quest_indexes: info.finish_quest_indexes.clone(),
        }
    }

    /// 客户端资料（Crystal `NPCInfo.ClientInformation`，`Server/MirDatabase/NPCInfo.cs` 末尾）。
    ///
    /// 逐字段照抄，包括 `Icon = BigMapIcon`（Crystal 就是拿大地图图标当提示框图标）。
    /// `ObjectID` 用运行期 ID（Crystal 的 `ClientInformation` 里是 0，由调用方回填，
    /// `PlayerObject.cs:1050`）。
    #[must_use]
    pub fn client_info(&self) -> ClientNPCInfo {
        ClientNPCInfo {
            index: self.index,
            file_name: self.file_name.clone(),
            name: self.name.clone(),
            map_index: self.map_index,
            location: self.position,
            image: self.image,
            rate: self.rate,
            show_on_big_map: self.show_on_big_map,
            big_map_icon: self.big_map_icon,
            object_id: self.object_id,
            icon: self.big_map_icon,
            can_teleport_to: self.can_teleport_to,
        }
    }

    /// 外观包（Crystal `NPCObject.GetInfo`，`NPCObject.cs:381-395`）。
    #[must_use]
    pub fn object_packet(&self) -> ObjectNPC {
        ObjectNPC {
            object_id: self.object_id,
            name: self.name.clone(),
            name_colour: mir_core::Color(NPC_NAME_COLOUR),
            image: self.image,
            // `NpcInfo.Colour` 没有存进 `Server.MirDB`（`NPCInfo.cs:20` 的字段不落盘），
            // 解码出来恒是 `Color.Empty`（0）。
            colour: mir_core::Color(0),
            location: self.position,
            direction: self.direction,
            quest_i_ds: self.quest_indexes.clone(),
        }
    }

    /// 名字显示用的「带空格的」形式：Crystal 客户端把 `_` 换成空格，
    /// 服务端在 `<$NPCNAME>` 里也做了一次（`NPCSegment.cs:1459`）。
    #[must_use]
    pub fn display_name(&self) -> String {
        self.name.replace('_', " ")
    }
}

// ---------------------------------------------------------------------------
// 错误
// ---------------------------------------------------------------------------

/// NPC 交互失败的原因。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NpcError {
    /// 玩家不在世界里。
    #[error("玩家 {0} 不在世界里")]
    NotInWorld(u32),
    /// 玩家已死亡：Crystal 的所有 NPC 入口第一行都是 `if (Dead) return;`。
    #[error("玩家 {player} 已死亡，不能与 NPC 交互")]
    Dead {
        /// 玩家对象 ID。
        player: u32,
    },
    /// 找不到这个 NPC（同图范围内）。
    #[error("NPC {0} 不在当前地图上")]
    NoSuchNpc(u32),
    /// 距离太远（Crystal `Functions.InRange(..., Globals.DataRange)`）。
    #[error("离 NPC {object_id} 太远（上限 {range} 格）")]
    OutOfRange {
        /// NPC 对象 ID。
        object_id: u32,
        /// 距离上限。
        range: i32,
    },
    /// NPC 对这个玩家不可见（等级/职业/标记位限制，Crystal `NPCObject.CheckVisible`，`:424-462`）。
    #[error("NPC {object_id} 对玩家不可见：{reason}")]
    NotVisible {
        /// NPC 对象 ID。
        object_id: u32,
        /// 具体原因。
        reason: &'static str,
    },
    /// 没有打开任何 NPC 对话窗口。
    #[error("玩家不在 NPC 对话中")]
    NoDialog,
    /// 点了一个当前页面没有展示的按钮（Crystal 会记一条 `PlayerPreventedNpcAccess` 日志）。
    #[error("当前页面没有按钮 {key}")]
    NotACurrentButton {
        /// 被拒绝的 key。
        key: String,
    },
    /// 脚本找不到或解析失败。
    #[error("NPC 脚本 {file} 不可用")]
    NoScript {
        /// 脚本文件名。
        file: String,
    },
    /// 当前页面不允许这个操作（例如不在 `[@BUY]` 里点买）。
    #[error("当前页面 {page} 不允许该操作")]
    NotShopPage {
        /// 当前页面 key。
        page: String,
    },
    /// 客户端给的 `PanelType` 与本作实现的入口不符。
    #[error("不支持的交易面板 {0:?}")]
    UnsupportedPanel(PanelType),
    /// 商品编号不在这个 NPC 的商品表里。
    #[error("商品 {item_index} 不在商品表里")]
    NoSuchGoods {
        /// 商品编号（Crystal `UserItem.UniqueID`，即脚本行号）。
        item_index: u64,
    },
    /// 数量非法（0 或超过堆叠上限）。
    #[error("数量 {count} 非法（堆叠上限 {stack}）")]
    InvalidCount {
        /// 请求数量。
        count: u16,
        /// 模板的堆叠上限。
        stack: u16,
    },
    /// 钱不够。
    #[error("金币不足：需要 {cost}，只有 {gold}")]
    NotEnoughGold {
        /// 需要的金币。
        cost: u32,
        /// 持有金币。
        gold: i64,
    },
    /// 背包满。
    #[error("背包已满")]
    InventoryFull,
    /// 超重（本作扩展，见 [`World::enforce_bag_weight`]）。
    #[error("超重：{weight} > {limit}")]
    TooHeavy {
        /// 交易后的总重量。
        weight: u32,
        /// 负重上限。
        limit: u32,
    },
    /// 背包里没有这件物品。
    #[error("背包里没有物品 {0}")]
    NoSuchItem(u64),
    /// 这件物品被绑定禁止出售（`BindMode.DontSell`）。
    #[error("物品 {0} 不可出售")]
    CannotSell(u64),
    /// 这件物品被绑定禁止修理（`BindMode.DontRepair`）。
    #[error("物品 {0} 不可修理")]
    CannotRepair(u64),
    /// 这个 NPC 的 `[Types]` 不允许交易该物品类型。
    #[error("这个 NPC 不收购该类型的物品")]
    TypeNotAllowed,
    /// 物品没有损耗，不需要修理（Crystal 仍走完流程，但费用为 0）。
    #[error("物品 {0} 不需要修理")]
    NothingToRepair(u64),
}

// ---------------------------------------------------------------------------
// 报告结构
// ---------------------------------------------------------------------------

/// 一次 NPC 对话的结果（同时已经发包给玩家）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NpcReply {
    /// 最终页面的正文（已渲染占位符、去掉颜色标记）。
    pub page: Vec<String>,
    /// 最终页面的 key。
    pub page_key: String,
    /// 是否结束了对话（`@exit`）。
    pub closed: bool,
    /// 本次打开的服务端面板（`"buy"` / `"sell"` / `"repair"` / `"srepair"`）。
    pub panels: Vec<&'static str>,
    /// 脚本里出现了、但本作没有实现的条件（原样保留，便于日志与文档）。
    pub unsupported_checks: Vec<String>,
    /// 脚本里出现了、但本作没有实现的动作。
    pub unsupported_actions: Vec<String>,
}

/// 买成功后的报告。
#[derive(Debug, Clone, PartialEq)]
pub struct BuyReport {
    /// 买到手的物品（已经有独立的 UUID）。
    pub item: UserItem,
    /// 花掉的金币。
    pub cost: u32,
}

/// 卖成功后的报告。
#[derive(Debug, Clone, PartialEq)]
pub struct SellReport {
    /// 卖出的物品 UUID。
    pub unique_id: u64,
    /// 卖出的数量。
    pub count: u16,
    /// 得到的金币（`Price() / 2`）。
    pub gold: u32,
    /// 物品模板名（日志用）。
    pub item_name: String,
}

/// 修理成功后的报告。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairReport {
    /// 物品 UUID。
    pub unique_id: u64,
    /// 花掉的金币。
    pub cost: u32,
    /// 修好后的最大持久（会被磨损一点点，见 [`World::repair_item`]）。
    pub max_dura: u16,
    /// 修好后的当前持久（= 最大持久）。
    pub current_dura: u16,
}

// ---------------------------------------------------------------------------
// 定价公式
// ---------------------------------------------------------------------------

/// 物品售价：Crystal `UserItem.Price`（`Shared/Data/ItemData.cs:516-542`）。
///
/// ```csharp
/// uint p = Info.Price;
/// if (Info.Durability > 0) {                                  // :523-535
///     float r = ((Info.Price / 2F) / Info.Durability);
///     p = (uint)(MaxDura * r);
///     if (MaxDura > 0) r = CurrentDura / (float)MaxDura; else r = 0;
///     p = (uint)Math.Floor(p / 2F + ((p / 2F) * r) + Info.Price / 2F);
/// }
/// p = (uint)(p * (AddedStats.Count * 0.1F + 1F));              // :538
/// return p * Count;                                            // :541
/// ```
///
/// * `AddedStats.Count` 是**属性绝对值之和**（`Shared/Data/Stat.cs:4`），不是条目数；
/// * 中间过程用 `f32`（C# 是 `float`），`(uint)` 转换是**向零截断**；
/// * 最后一步 `p * Count` 在 C# 里是 `uint` 乘法，溢出会回绕；这里用饱和乘，
///   避免 4 字节物品把价格算成天价（见 `docs/world/npc.md` 的差异清单）。
#[must_use]
#[allow(clippy::cast_precision_loss)] // C# 侧就是 `float` 运算（`ItemData.cs:525-538`），这里逐位对齐
pub fn item_price(info: &ItemInfo, item: &UserItem) -> u32 {
    let mut price = info.price as f32;
    if info.durability > 0 {
        let mut ratio = (info.price as f32 / 2.0) / f32::from(info.durability);
        price = f32::from(item.max_dura) * ratio;
        ratio = if item.max_dura > 0 {
            f32::from(item.current_dura) / f32::from(item.max_dura)
        } else {
            0.0
        };
        price = (price / 2.0 + (price / 2.0) * ratio + info.price as f32 / 2.0).floor();
    }
    price *= item.added_stats.count() as f32 * 0.1 + 1.0;
    (price as u32).saturating_mul(u32::from(item.count))
}

/// 修理费：Crystal `UserItem.RepairPrice`（`Shared/Data/ItemData.cs:543-563`）。
///
/// ```csharp
/// if (Info == null || Info.Durability == 0) return 0;          // :545-546
/// var p = Info.Price;
/// if (Info.Durability > 0) {
///     p = (uint)Math.Floor(MaxDura * ((Info.Price / 2F) / Info.Durability) + Info.Price / 2F);
///     p = (uint)(p * (AddedStats.Count * 0.1F + 1F));
/// }
/// var cost = p * Count - Price();                              // :557
/// if (RentalInformation == null) return cost;                  // :559-560
/// return cost * 2;                                             // :562
/// ```
///
/// 简化：租借信息（`RentalInformation`）本作没有，所以恒走 `:559-560` 分支；
/// C# 的 `p * Count - Price()` 是无符号减法（负数会回绕成天文数字），这里用
/// `saturating_sub` 截到 0。
#[must_use]
#[allow(clippy::cast_precision_loss)] // 同 [`item_price`]：C# 是 `float`/`uint` 混合运算
pub fn repair_price(info: &ItemInfo, item: &UserItem) -> u32 {
    if info.durability == 0 {
        return 0;
    }
    let mut price = f32::from(item.max_dura)
        * ((info.price as f32 / 2.0) / f32::from(info.durability))
        + info.price as f32 / 2.0;
    price = price.floor();
    price *= item.added_stats.count() as f32 * 0.1 + 1.0;
    let full = (price as u32).saturating_mul(u32::from(item.count));
    full.saturating_sub(item_price(info, item))
}

/// 按 NPC 的价格倍率折算：Crystal `cost * PriceRate(player)`（`NPCScript.cs:1188`、`PlayerObject.cs:8137`）。
///
/// C# 写的是 `(uint)(cost * PriceRate)`：`uint` 先转 `float`、相乘、再向零截断。
#[must_use]
#[allow(clippy::cast_precision_loss)] // 见上：必须保持与 C# 相同的 float 语义
fn apply_price_rate(cost: u32, rate: f32) -> u32 {
    (cost as f32 * rate) as u32
}

// ---------------------------------------------------------------------------
// World：NPC 刷出与查询
// ---------------------------------------------------------------------------

impl World {
    /// 所有 NPC（刷出顺序）。
    #[must_use]
    pub fn npcs(&self) -> &[Npc] {
        &self.npcs
    }

    /// 按对象 ID 取 NPC。
    #[must_use]
    pub fn npc(&self, object_id: u32) -> Option<&Npc> {
        self.npcs.iter().find(|npc| npc.object_id == object_id)
    }

    /// 某张地图上的 NPC。
    #[must_use]
    pub fn npcs_on(&self, map: &str) -> Vec<&Npc> {
        self.npcs.iter().filter(|npc| npc.map == map).collect()
    }

    /// 按 `Server.MirDB` 的 NPC 表刷出某张地图的 NPC，返回新分配的对象 ID。
    ///
    /// 移植 `MapInfo.CreateMap`（`Server/MirDatabase/MapInfo.cs:197-215`）里
    /// 「把 `MapIndex` 匹配的 `NPCInfo` 塞进这张图」+ `Map.Load` 的
    /// `AddObject(new NPCObject(info) { CurrentMap = this })`
    /// （`Server/MirEnvir/Map.cs:490-496`）：
    ///
    /// * 只取 `map_index` 等于这张地图主键的 NPC；
    /// * 坐标必须在地图内（Crystal 的 `ValidPoint`，`Map.cs:493`），否则跳过；
    /// * 朝向随机 `0..3`（都是「下/左下/左」三个方向，`NPCObject.cs:52`）。
    ///
    /// 同时把新 NPC 的外观包广播给同图视野内的玩家（Crystal `AddObject` →
    /// `MapObject.Add` 会 `Broadcast(GetInfo())`）。
    pub fn spawn_npcs(&mut self, map_file: &str) -> Vec<u32> {
        let Some(data) = self.game_data.clone() else {
            return Vec::new();
        };
        let Some(map_info) = data.map_info(map_file).cloned() else {
            tracing::warn!(map = map_file, "数据库里没有这张地图，无法刷 NPC");
            return Vec::new();
        };
        let infos: Vec<NpcInfo> = data.npcs_for_map(map_file).into_iter().cloned().collect();
        let mut spawned = Vec::with_capacity(infos.len());
        for info in &infos {
            // 同一张图的同一个 `NpcInfo.Index` 只刷一次：Crystal 在 `Map.Load` 里只跑一遍
            // （`Map.cs:490-496`），重复调用等价于重复 Load，不应该多出一份 NPC。
            if self
                .npcs
                .iter()
                .any(|npc| npc.map == map_info.file_name && npc.index == info.index)
            {
                continue;
            }
            if !self.valid_npc_point(&map_info.file_name, info.location) {
                tracing::debug!(
                    npc = %info.name,
                    x = info.location.x,
                    y = info.location.y,
                    "NPC 坐标不在地图内，跳过"
                );
                continue;
            }
            let direction = MirDirection::from_value(self.rng.next_bounded(3) as u8)
                .unwrap_or(MirDirection::Down);
            let object_id = self.allocate_object_id();
            let mut npc = Npc::from_info(info, &map_info.file_name, object_id, direction);
            // Crystal 的 `NPCScript.ParseQuests` 把 `[Quests]` 关联的任务塞进 `NPCObject.Quests`，
            // `S.ObjectNPC` 的 `QuestIDs` 就是它（`NPCScript.cs:716-717`、`NPCObject.cs:392-393`）。
            // 样本数据库里的两个任务列表全是空的，所以必须把脚本关联补进来，
            // 否则客户端连一个任务图标都画不出来。
            self.fill_npc_quest_links(&mut npc);
            let frame = ServerPacket::ObjectNPC(npc.object_packet()).encode_frame();
            self.npcs.push(npc.clone());
            for player in self.players.values() {
                if player.map == map_info.file_name
                    && player
                        .position
                        .in_range(info.location, crate::world::VIEW_RANGE)
                    && npc_visible_to(player, &npc)
                {
                    let _ = player.sender.send(frame.clone());
                }
            }
            spawned.push(object_id);
        }
        tracing::info!(
            map = %map_info.file_name,
            title = %map_info.title,
            npcs = spawned.len(),
            "已刷出地图 NPC"
        );
        spawned
    }

    /// 直接放一个 NPC 进世界（测试与 GM 用；生产路径走 [`World::spawn_npcs`]）。
    ///
    /// 返回新对象 ID；坐标越界时返回 `None`。
    pub fn spawn_npc(&mut self, map_file: &str, info: &NpcInfo) -> Option<u32> {
        if !self.valid_npc_point(map_file, info.location) {
            return None;
        }
        let object_id = self.allocate_object_id();
        let mut npc = Npc::from_info(info, map_file, object_id, MirDirection::Down);
        self.fill_npc_quest_links(&mut npc);
        self.npcs.push(npc);
        Some(object_id)
    }

    /// 把 NPC 脚本 `[Quests]` 里的**发布**任务并进 `Npc::quest_indexes`
    /// （`S.ObjectNPC.QuestIDs` 用的是正数编号，`NPCObject.cs:392-393`）。
    fn fill_npc_quest_links(&self, npc: &mut Npc) {
        let Some(script) = self.npc_script(&npc.file_name) else {
            return;
        };
        for &index in script.quest_indexes() {
            if index > 0 && !npc.quest_indexes.contains(&index) {
                npc.quest_indexes.push(index);
            }
        }
    }

    /// NPC 坐标是否在地图内（Crystal `Map.ValidPoint`，`Map.cs:493`）。
    fn valid_npc_point(&self, map_file: &str, point: Point) -> bool {
        if point.x < 0 || point.y < 0 {
            return false;
        }
        let (max_x, max_y) = self.map_bounds(map_file);
        point.x <= max_x && point.y <= max_y
    }

    /// 取（并缓存）NPC 脚本。
    ///
    /// 缓存挂在 [`crate::gamedata::GameData`] 上（与掉落表同一套做法），
    /// 对应 Crystal 的 `Envir.Scripts` 字典（`NPCScript.cs:102`）。
    pub fn npc_script(&self, file_name: &str) -> Option<Arc<NpcScript>> {
        self.game_data.as_ref()?.npc_script(file_name)
    }

    /// 某个 NPC 的商品表（`[TRADE]` 段 + 物品模板）。
    ///
    /// Crystal `ProcessSpecial` 里的 `sentGoods`（`NPCScript.cs:929-947`）：
    /// 按 `CreateShopItem(info, (uint)i)` 的规则生成「商店物品」：
    /// `UniqueID = 脚本行号`、`CurrentDura = MaxDura = Info.Durability`、`IsShopItem = true`。
    /// 模板查不到的行直接跳过（Crystal 在解析阶段就过滤掉了）。
    #[must_use]
    pub fn npc_goods(&self, npc: &Npc) -> Vec<UserItem> {
        let Some(script) = self.npc_script(&npc.file_name) else {
            return Vec::new();
        };
        let Some(data) = self.game_data.as_ref() else {
            return Vec::new();
        };
        let mut goods = Vec::with_capacity(script.goods().len());
        for line in script.goods() {
            let Some(info) = data.item_by_name(&line.name) else {
                continue;
            };
            goods.push(shop_item(info, line.unique_id, line.count));
        }
        goods
    }

    /// 进图时下发的 `ClientMapInfo`（Crystal `PlayerObject.CheckMapInfo`，`:1009-1056`）。
    ///
    /// * `Title` / `BigMap` 来自 `MapInfo`，`Width`/`Height` 来自已加载的地图网格
    ///   （Crystal 取 `map.Width`/`map.Height`，`:1024-1025`）；
    /// * `Movements` 只收 `ShowOnBigMap` 的传送点，`Title` 取**目的地**地图的标题
    ///   （`:1030-1045`）；
    /// * `NPCs` 只收 `ShowOnBigMap` 的 NPC，按 `BigMapIcon` 升序（`:1047-1052`），
    ///   `ObjectID` 填运行期 ID，其余字段来自 `NpcInfo.ClientInformation`。
    ///
    /// 简化：`RequiredGroup` / `WorldMapSetupInfo` 与 `Connection.SentMapInfo` 去重未实现
    /// （本作只在进世界时发一次，切图不发 `NewMapInfo`）。
    #[must_use]
    pub fn map_entry_info(&self, map_file: &str, title: &str) -> Option<ClientMapInfo> {
        let data = self.game_data.as_ref()?;
        let map_info: &MapInfo = data.map_info(map_file)?;
        let (width, height) = self
            .map_of(map_file)
            .map_or((0, 0), |map| (map.width, map.height));
        let mut movements = Vec::new();
        for movement in &map_info.movements {
            if !movement.show_on_big_map {
                continue;
            }
            let Some(destination) = data.map_info_by_index(movement.map_index) else {
                continue;
            };
            movements.push(ClientMovementInfo {
                destination: movement.map_index,
                title: destination.title.clone(),
                location: movement.source,
                icon: movement.icon,
            });
        }
        let mut npcs: Vec<&Npc> = self
            .npcs
            .iter()
            .filter(|npc| npc.map == map_info.file_name && npc.show_on_big_map)
            .collect();
        npcs.sort_by_key(|npc| npc.big_map_icon);
        let npcs = npcs.into_iter().map(Npc::client_info).collect();
        Some(ClientMapInfo {
            title: title.to_owned(),
            width,
            height,
            big_map: i32::from(map_info.big_map),
            movements,
            npcs,
        })
    }

    /// 进图/切图时要给这个玩家下发的视野内 NPC 帧（Crystal `MapObject.Add`，
    /// `MapObject.cs:286-308`；在 [`World::enter`] 与
    /// [`World::change_map`](crate::maps::World::change_map) 里调用）。
    ///
    /// 返回 `(S.ObjectNPC 帧, NPC 对象 ID)`，调用方负责真正发送并记录可见集合。
    /// 不可见的 NPC（等级/职业/标记位不符）**不发**，与 `CheckVisible` 的结论一致。
    pub(crate) fn visible_npc_frames(&self, player: &Player) -> Vec<(Vec<u8>, u32)> {
        let mut frames = Vec::new();
        for npc in &self.npcs {
            if npc.map == player.map
                && npc
                    .position
                    .in_range(player.position, crate::world::VIEW_RANGE)
                && npc_visible_to(player, npc)
            {
                frames.push((
                    ServerPacket::ObjectNPC(npc.object_packet()).encode_frame(),
                    npc.object_id,
                ));
            }
        }
        frames
    }
}

/// NPC 对这个玩家是否可见（Crystal `NPCObject.CheckVisible`，`NPCObject.cs:424-462`）。
///
/// 简化：攻城（`Conq.WarIsOn` + `ConquestVisible`）与时段/星期可见性未实现。
fn npc_visible_to(player: &Player, npc: &Npc) -> bool {
    if npc.flag_needed != 0 && !player.npc.flags.contains(&npc.flag_needed) {
        return false;
    }
    if npc.min_lev != 0 && i32::from(player.level) < i32::from(npc.min_lev) {
        return false;
    }
    if npc.max_lev != 0 && i32::from(player.level) > i32::from(npc.max_lev) {
        return false;
    }
    npc.class_required.is_empty() || class_name(player.class) == npc.class_required
}

/// 生成一件「商店物品」（Crystal `Envir.CreateShopItem`，`Envir.cs:4426-4439`）。
#[must_use]
fn shop_item(info: &ItemInfo, unique_id: u64, count: u16) -> UserItem {
    UserItem {
        unique_id,
        item_index: info.index,
        current_dura: info.durability,
        max_dura: info.durability,
        count,
        is_shop_item: true,
        ..UserItem::default()
    }
}

// ---------------------------------------------------------------------------
// World：对话执行
// ---------------------------------------------------------------------------

/// 一段检查的结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChecksOutcome {
    /// 全部通过（走 `#SAY` / `#ACT`）。
    Pass,
    /// 有一项不通过（走 `#ELSESAY` / `#ELSEACT`）。
    Fail,
}

impl World {
    /// 玩家点了 NPC 或对话里的按钮（`C.CallNPC`）。
    ///
    /// 逐条对应 Crystal `PlayerObject.CallNPC`（`PlayerObject.cs:7900-7929`）：
    ///
    /// 1. `if (Dead) return;`（`:7902`）；
    /// 2. 在**当前地图**里按对象 ID 找 NPC（`:7906-7909`）；
    /// 3. 距离检查 `Functions.InRange(ob.CurrentLocation, CurrentLocation, Globals.DataRange)`（`:7910`）；
    /// 4. `CheckVisible` —— 标记位/等级/职业不满足就拒绝（`:7912-7914`、`NPCObject.cs:424-462`）；
    /// 5. 不是 `[@MAIN]` 时必须满足「当前页确实展示过这个按钮」（`NPCScript.cs:831-857`）；
    /// 6. 逐段跑检查 → `S.NPCResponse`（`NPCScript.cs:870-900`）；
    /// 7. 处理 `@exit`：客户端自己关窗（`NPCDialog.cs:288-293`），这里顺便清掉服务端状态。
    ///
    /// 与 Crystal 的差异：`NPCDelayed`（`:831-861`）与 `S.NPCRequestInput`（`[@@"` 输入框，
    /// `:863-868`）没有实现，见 `docs/world/npc.md`。
    pub fn call_npc(
        &mut self,
        player_id: u32,
        npc_object_id: u32,
        key: &str,
    ) -> Result<NpcReply, NpcError> {
        let Some(player) = self.players.get(&player_id) else {
            return Err(NpcError::NotInWorld(player_id));
        };
        if player.dead {
            return Err(NpcError::Dead { player: player_id });
        }
        let player_map = player.map.clone();
        let player_position = player.position;
        let Some(npc) = self
            .npcs
            .iter()
            .find(|npc| npc.object_id == npc_object_id && npc.map == player_map)
            .cloned()
        else {
            return Err(NpcError::NoSuchNpc(npc_object_id));
        };
        if !npc.position.in_range(player_position, NPC_TALK_RANGE) {
            return Err(NpcError::OutOfRange {
                object_id: npc_object_id,
                range: NPC_TALK_RANGE,
            });
        }
        self.check_npc_visible(player_id, &npc)?;

        let key = mir_db::npc_script::normalize_key(key);
        // `@exit` 与 `CLOSE` 都是「结束对话」。Crystal 只有客户端侧的 `@exit`
        // （`NPCDialogs.cs:288-293`）；`CLOSE` 是本作额外接受的写法，方便脚本/GM 手动关窗。
        if key == EXIT_SECTION || key == "[@CLOSE]" {
            if let Some(player) = self.players.get_mut(&player_id) {
                player.npc.close();
            }
            self.send_to_player(
                player_id,
                &ServerPacket::NPCResponse(NPCResponse { page: Vec::new() }).encode_frame(),
            );
            return Ok(NpcReply {
                page_key: EXIT_SECTION.to_owned(),
                closed: true,
                ..NpcReply::default()
            });
        }
        // Crystal 只在「不是 MAIN」时校验按钮白名单（`NPCScript.cs:833-856`）。
        if key != MAIN_SECTION {
            let state = &self.players[&player_id].npc;
            if state.npc_object_id != npc_object_id {
                return Err(NpcError::NoDialog);
            }
            if !state.buttons.contains(&key) {
                tracing::warn!(
                    player = %self.players[&player_id].name,
                    npc = %npc.name,
                    key = %key,
                    "玩家点了当前页面没展示的 NPC 按钮（Crystal 的 PlayerPreventedNpcAccess）"
                );
                return Err(NpcError::NotACurrentButton { key });
            }
        }

        let Some(script) = self.npc_script(&npc.file_name) else {
            return Err(NpcError::NoScript {
                file: npc.file_name.clone(),
            });
        };

        let mut reply = NpcReply::default();
        let mut page_key = key;
        for _ in 0..MAX_NPC_HOPS {
            let Some(page) = script.section(&page_key).cloned() else {
                tracing::debug!(
                    script = %script.file_name(),
                    key = %page_key,
                    "NPC 脚本里没有这个页面"
                );
                break;
            };
            let run = self.run_npc_page(player_id, &npc, &page);
            // 每个页面都发一条 `S.NPCResponse`（Crystal `Response`，`NPCScript.cs:895-900`）。
            self.send_to_player(
                player_id,
                &ServerPacket::NPCResponse(NPCResponse {
                    page: run.speech.clone(),
                })
                .encode_frame(),
            );
            reply
                .panels
                .extend(self.open_npc_panel(player_id, &npc, &page.key));
            reply.unsupported_checks.extend(run.unsupported_checks);
            reply.unsupported_actions.extend(run.unsupported_actions);
            if let Some(player) = self.players.get_mut(&player_id) {
                player.npc.npc_object_id = npc.object_id;
                page.key.clone_into(&mut player.npc.page_key);
                player.npc.buttons = run.buttons;
            }
            reply.page = run.speech;
            reply.page_key = page.key;
            match run.next_page {
                Some(next) => page_key = next,
                None => break,
            }
        }
        Ok(reply)
    }

    /// NPC 可见性：Crystal `NPCObject.CheckVisible`（`NPCObject.cs:424-462`）。
    ///
    /// 判定本身在 [`npc_visible_to`]（发 `ObjectNPC` 时也用同一个函数），
    /// 这里只负责把失败原因翻成带具体理由的错误。
    fn check_npc_visible(&self, player_id: u32, npc: &Npc) -> Result<(), NpcError> {
        let Some(player) = self.players.get(&player_id) else {
            return Err(NpcError::NotInWorld(player_id));
        };
        if npc_visible_to(player, npc) {
            return Ok(());
        }
        let reason = if npc.flag_needed != 0 && !player.npc.flags.contains(&npc.flag_needed) {
            "缺少所需标记位"
        } else if npc.min_lev != 0 && i32::from(player.level) < i32::from(npc.min_lev) {
            "等级低于下限"
        } else if npc.max_lev != 0 && i32::from(player.level) > i32::from(npc.max_lev) {
            "等级高于上限"
        } else {
            "职业不符"
        };
        Err(NpcError::NotVisible {
            object_id: npc.object_id,
            reason,
        })
    }

    /// 跑一个页面：逐段检查 → 执行动作 → 收集正文，返回正文/按钮/要跳转的页面。
    ///
    /// 对应 Crystal `NPCScript.Call` 里的 `foreach (NPCSegment segment in page.SegmentList)`
    /// 循环（`NPCScript.cs:878-890`）与 `NPCSegment.Success`/`Failed`（`:4956-4974`）。
    fn run_npc_page(&mut self, player_id: u32, npc: &Npc, page: &NpcPage) -> PageRun {
        let mut run = PageRun::default();
        let mut broken = false;
        let mut buttons = BTreeSet::new();
        for segment in &page.segments {
            if broken {
                // Crystal `page.BreakFromSegments`：本页剩下的段落不再处理。
                break;
            }
            let outcome =
                self.evaluate_checks(player_id, &segment.checks, &mut run.unsupported_checks);
            let (actions, say, segment_buttons) = match outcome {
                ChecksOutcome::Pass => (&segment.actions, &segment.say, &segment.buttons),
                ChecksOutcome::Fail => (
                    &segment.else_actions,
                    &segment.else_say,
                    &segment.else_buttons,
                ),
            };
            // Crystal 先 `Act` 再追加正文（`NPCSegment.cs:4956-4964`）。
            for action in actions {
                self.run_npc_action(player_id, npc, action, &mut broken, &mut run);
            }
            for line in say {
                run.speech.push(self.render_say_line(player_id, npc, line));
            }
            buttons.extend(segment_buttons.iter().cloned());
        }
        run.buttons = buttons;
        run
    }

    /// 执行一条动作（Crystal `NPCSegment.Act(IList<NPCActions>, PlayerObject)`，`:3047-4750`）。
    fn run_npc_action(
        &mut self,
        player_id: u32,
        npc: &Npc,
        action: &NpcAction,
        broken: &mut bool,
        run: &mut PageRun,
    ) {
        match action.kind {
            NpcActionKind::Goto => {
                if let Some(target) = action.params.first() {
                    // Crystal 先压入 `DelayedAction(DelayedType.NPC, -1, …)`，
                    // 由 `CallNPCNextPage` 立刻跟进（`NPCSegment.cs:3684-3689`、`PlayerObject.cs:7930-7942`）。
                    run.next_page = Some(mir_db::npc_script::normalize_key(target));
                } else {
                    tracing::debug!(npc = %npc.name, "GOTO 缺参数：{}", action.source);
                }
            }
            NpcActionKind::Break => {
                *broken = true;
            }
            NpcActionKind::GiveGold => {
                let Some(amount) = parse_u32(&action.params, 0) else {
                    return;
                };
                // `:3248-3251`：溢出时截到 `uint.MaxValue - Gold`。
                let capped = if self.players[&player_id].gold + i64::from(amount)
                    >= i64::from(u32::MAX)
                {
                    u32::try_from(i64::from(u32::MAX) - self.players[&player_id].gold).unwrap_or(0)
                } else {
                    amount
                };
                self.gain_gold(player_id, capped);
            }
            NpcActionKind::TakeGold => {
                let Some(amount) = parse_u32(&action.params, 0) else {
                    return;
                };
                // `:3259-3262`：不够就扣光。
                let gold = self.players[&player_id].gold;
                let capped = u32::try_from(gold.min(i64::from(amount))).unwrap_or(0);
                // 扣光时 `lose_gold` 一定成功（金额已夹到持有量）。
                let _ = self.lose_gold(player_id, capped);
            }
            NpcActionKind::SetFlag => {
                let Some(index) = parse_i32(&action.params, 0) else {
                    return;
                };
                let Some(value) = parse_u32(&action.params, 1) else {
                    return;
                };
                // `:3713`：`flagIndex < 0 || flagIndex >= Globals.FlagIndexCount` 直接丢弃。
                if !(0..FLAG_INDEX_COUNT).contains(&index) {
                    tracing::debug!(index, "SET 的标记位下标越界");
                    return;
                }
                if let Some(player) = self.players.get_mut(&player_id) {
                    if value == 0 {
                        player.npc.flags.remove(&index);
                    } else {
                        player.npc.flags.insert(index);
                    }
                }
            }
            NpcActionKind::Unsupported | NpcActionKind::Unknown => {
                // `mir-db` 只把少数动作认到具体种类（见 `crates/mir-db/src/npc_script.rs`
                // 的 `parse_action`）；切图类动作在这里按关键字补上，
                // 与 Crystal `NPCSegment.ParseAct`/`Act` 对齐（见 [`World::run_map_action`]）。
                if !self.run_map_action(player_id, npc, action) {
                    run.unsupported_actions.push(action.source.clone());
                }
            }
            // 下面三个是**本作扩展**：Crystal 的 `NPCActions` 里没有对应的动作
            // （任务接取/交付只能由客户端弹任务面板后发 `C.AcceptQuest`/`C.FinishQuest`，
            // `PlayerObject.cs:11251`、`:11350`）。见 `docs/world/quests.md`。
            NpcActionKind::AcceptQuest => {
                let Some(index) = parse_i32(&action.params, 0) else {
                    tracing::debug!(npc = %npc.name, "ACCEPTQUEST 缺参数：{}", action.source);
                    return;
                };
                match self.accept_quest(player_id, npc.object_id, index) {
                    Ok(report) => tracing::debug!(
                        npc = %npc.name,
                        quest = report.quest_index,
                        "NPC 动作接取任务"
                    ),
                    Err(error) => {
                        tracing::debug!(npc = %npc.name, %error, "NPC 动作接取任务被拒绝");
                        // 让脚本知道结果，便于写 `<提示/@...>`。
                        run.quest_events
                            .push(format!("ACCEPTQUEST {index} 失败：{error}"));
                    }
                }
            }
            NpcActionKind::FinishQuest => {
                let Some(index) = parse_i32(&action.params, 0) else {
                    tracing::debug!(npc = %npc.name, "FINISHQUEST 缺参数：{}", action.source);
                    return;
                };
                // 脚本里没有「选第几个可选奖励」的写法，交给 `C.FinishQuest` 的
                // `SelectedItemIndex`；这里固定 `-1`（不发任选奖励）。
                match self.finish_quest(player_id, index, -1) {
                    Ok(report) => tracing::debug!(
                        npc = %npc.name,
                        quest = report.quest_index,
                        gold = report.gold,
                        exp = report.exp,
                        "NPC 动作交付任务"
                    ),
                    Err(error) => {
                        tracing::debug!(npc = %npc.name, %error, "NPC 动作交付任务被拒绝");
                        run.quest_events
                            .push(format!("FINISHQUEST {index} 失败：{error}"));
                    }
                }
            }
            NpcActionKind::AbandonQuest => {
                let Some(index) = parse_i32(&action.params, 0) else {
                    tracing::debug!(npc = %npc.name, "ABANDONQUEST 缺参数：{}", action.source);
                    return;
                };
                if let Err(error) = self.abandon_quest(player_id, index) {
                    tracing::debug!(npc = %npc.name, %error, "NPC 动作放弃任务被拒绝");
                    run.quest_events
                        .push(format!("ABANDONQUEST {index} 失败：{error}"));
                }
            }
            NpcActionKind::CreateGuild => {
                // 本作扩展（Crystal 只有 `[@CREATEGUILD]` 页面 + `C.GuildNameReturn`）。
                let Some(name) = action.params.first() else {
                    tracing::debug!(npc = %npc.name, "CREATEGUILD 缺参数：{}", action.source);
                    return;
                };
                match self.create_guild(player_id, name) {
                    Ok(guild_id) => {
                        // 成功提示照 `PlayerObject.cs:3299` 的 GM 命令写法。
                        self.send_system_chat(player_id, &format!("行会 {name} 创建成功。"));
                        tracing::info!(npc = %npc.name, guild = guild_id, %name, "NPC 动作创建行会");
                    }
                    Err(error) => {
                        tracing::debug!(npc = %npc.name, %error, "NPC 动作创建行会被拒绝");
                        self.send_system_chat(player_id, &error.message());
                        run.quest_events
                            .push(format!("CREATEGUILD {name} 失败：{error}"));
                    }
                }
            }
        }
    }

    /// 切图类 NPC 动作：`MOVE` / `MAPMOVE` / `ENTERMAP`。
    ///
    /// 返回 `true` 表示「认识这个关键字」（即使因为参数缺失/地图不存在而没有执行），
    /// 调用方就不会把它记进 `unsupported_actions`。
    ///
    /// | 动作 | Crystal | 本作 |
    /// |---|---|---|
    /// | `MOVE <地图> <x> <y>` | `NPCSegment.cs:452-459`（解析）、`:3075-3088`（执行） | 同步切图；缺坐标时忽略（Crystal 会随机传送） |
    /// | `MAPMOVE <地图> <x> <y>` | 只在 GM 聊天命令里出现（`PlayerObject.cs:2872-2913`） | **本作扩展**：`MOVE` 的别名（任务要求的写法） |
    /// | `ENTERMAP` | `NPCSegment.cs:993-995`（解析）、`:4065-4074`（执行） | 消费 `NeedMove` 传送点记下的目的地 |
    fn run_map_action(&mut self, player_id: u32, npc: &Npc, action: &NpcAction) -> bool {
        let keyword = action
            .source
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        match keyword.as_str() {
            "MOVE" | "MAPMOVE" => {
                // Crystal：`if (parts.Length < 2) return;`（`NPCSegment.cs:453`）。
                let Some(map_arg) = action.params.first() else {
                    return true;
                };
                let Some(map_file) = self.resolve_script_map(map_arg) else {
                    tracing::debug!(
                        npc = %npc.name,
                        map = %map_arg,
                        "MOVE 的目标地图不存在，忽略"
                    );
                    return true;
                };
                // Crystal `NPCSegment.cs:3080-3086`：`coords.X > 0 && coords.Y > 0` 才按坐标传送，
                // 否则随机传送（本作没有实现随机传送，缺坐标时忽略）。
                let (Some(x), Some(y)) =
                    (parse_i32(&action.params, 1), parse_i32(&action.params, 2))
                else {
                    tracing::debug!(
                        npc = %npc.name,
                        source = %action.source,
                        "MOVE 缺少坐标，忽略（Crystal 会随机传送）"
                    );
                    return true;
                };
                let direction = self.players[&player_id].direction;
                match self.change_map(player_id, &map_file, Point::new(x, y), direction) {
                    Ok(report) => tracing::debug!(
                        npc = %npc.name,
                        player = %self.players[&player_id].name,
                        to = %report.to_map,
                        x = report.position.x,
                        y = report.position.y,
                        "NPC 动作切图"
                    ),
                    Err(error) => {
                        tracing::debug!(npc = %npc.name, %error, "NPC 动作切图失败");
                    }
                }
                true
            }
            "ENTERMAP" => {
                // Crystal `NPCSegment.cs:4067-4072`：取出 `NeedMove` 传送点记下的地图/坐标并清掉。
                let pending = self
                    .players
                    .get(&player_id)
                    .and_then(|player| player.npc.move_map.clone().zip(player.npc.move_coord));
                let Some((map_file, point)) = pending else {
                    return true;
                };
                if let Some(player) = self.players.get_mut(&player_id) {
                    player.npc.move_map = None;
                    player.npc.move_coord = None;
                }
                let direction = self.players[&player_id].direction;
                match self.change_map(player_id, &map_file, point, direction) {
                    Ok(report) => tracing::debug!(
                        npc = %npc.name,
                        player = %self.players[&player_id].name,
                        to = %report.to_map,
                        x = report.position.x,
                        y = report.position.y,
                        "ENTERMAP 切图"
                    ),
                    Err(error) => {
                        tracing::debug!(npc = %npc.name, %error, "ENTERMAP 切图失败");
                    }
                }
                true
            }
            _ => false,
        }
    }

    /// NPC 脚本里的地图参数。
    ///
    /// Crystal 的脚本写的是**地图文件名**（`Envir.GetMapByNameAndInstance(param[0])`，
    /// `Server/MirEnvir/Envir.cs:4607-4614`），所以**先按文件名查**；查不到再按
    /// `MapInfo.Index` 查（**本作扩展**，方便写 `MOVE 20 10 10`）。
    /// 顺序不能反：Mir2 的地图文件名全是数字，按主键优先会把 `MOVE 1 ...` 解析成错误的地图。
    fn resolve_script_map(&self, arg: &str) -> Option<String> {
        let file = arg
            .strip_suffix(".map")
            .or_else(|| arg.strip_suffix(".MAP"))
            .unwrap_or(arg);
        if let Some(info) = self.game_data.as_ref().and_then(|data| data.map_info(file)) {
            return Some(info.file_name.clone());
        }
        // 数据库里没有这张图、但世界已经加载了它（测试里直接 insert_map 的图）。
        if self.map_of(file).is_some() {
            return Some(file.to_owned());
        }
        let index = arg.parse::<i32>().ok()?;
        let info = self
            .game_data
            .as_ref()
            .and_then(|data| data.map_info_by_index(index))?;
        Some(info.file_name.clone())
    }

    /// 逐条检查 `#IF`（Crystal `NPCSegment.Check(PlayerObject)`，`:2077-2950`）。
    ///
    /// Crystal 的语义：任一检查不通过就走 else 分支；运算符写错时打日志并**整段通过**
    /// （`:2116-2120` 的 `catch { … return true; }`）。本作同样处理。
    fn evaluate_checks(
        &mut self,
        player_id: u32,
        checks: &[NpcCheck],
        unsupported: &mut Vec<String>,
    ) -> ChecksOutcome {
        for check in checks {
            match self.evaluate_check(player_id, check, unsupported) {
                CheckOutcome::Pass => {}
                CheckOutcome::Fail => return ChecksOutcome::Fail,
                CheckOutcome::InvalidOperator => return ChecksOutcome::Pass,
            }
        }
        ChecksOutcome::Pass
    }

    /// 单条检查。返回 [`CheckOutcome`]。
    fn evaluate_check(
        &mut self,
        player_id: u32,
        check: &NpcCheck,
        unsupported: &mut Vec<String>,
    ) -> CheckOutcome {
        match check.kind {
            NpcCheckKind::Level => {
                let Some(op) = param_compare_op(&check.params, 0) else {
                    return CheckOutcome::InvalidOperator;
                };
                let Some(value) = parse_i32(&check.params, 1) else {
                    return CheckOutcome::Fail;
                };
                let level = i32::from(self.players[&player_id].level);
                op.compare(&level, &value).into()
            }
            NpcCheckKind::Gold => {
                let Some(op) = param_compare_op(&check.params, 0) else {
                    return CheckOutcome::InvalidOperator;
                };
                let Some(value) = parse_u32(&check.params, 1) else {
                    return CheckOutcome::Fail;
                };
                let gold = u32::try_from(self.players[&player_id].gold).unwrap_or(0);
                op.compare(&gold, &value).into()
            }
            NpcCheckKind::PkPoint => {
                // PK 系统未实现：Crystal 读 `player.PKPoints`，本作恒为 0。
                let Some(op) = param_compare_op(&check.params, 0) else {
                    return CheckOutcome::InvalidOperator;
                };
                let Some(value) = parse_i32(&check.params, 1) else {
                    return CheckOutcome::Fail;
                };
                op.compare(&0, &value).into()
            }
            NpcCheckKind::Item => self.evaluate_item_check(player_id, check),
            NpcCheckKind::Class => {
                let Some(name) = check.params.first() else {
                    return CheckOutcome::Fail;
                };
                let Some(class) = parse_class(name) else {
                    return CheckOutcome::Fail;
                };
                (self.players[&player_id].class == class).into()
            }
            NpcCheckKind::Gender => {
                let Some(name) = check.params.first() else {
                    return CheckOutcome::Fail;
                };
                let Some(gender) = parse_gender(name) else {
                    return CheckOutcome::Fail;
                };
                (self.players[&player_id].gender == gender).into()
            }
            NpcCheckKind::Quest => {
                // `:2518-2535`：`CHECKQUEST <序号> ACTIVE|COMPLETE`。
                //
                // Crystal 的判定**不是**「ACTIVE 走一个分支、其它走另一个」：
                //
                // ```csharp
                // string tempString = param[1].ToUpper();
                // if (tempString == "ACTIVE") failed = !player.CurrentQuests.Any(e => e.Index == tempInt);
                // else failed = !player.CompletedQuests.Contains(tempInt);
                // ```
                //
                // 也就是说**除 `ACTIVE` 之外的任何写法都按 `COMPLETE` 处理**——真实样本里有 32 处
                // `CHECKQUEST n 1` / `CHECKQUEST n 0`（例如 `BichonProvince/BichonWall/Blacksmith1.txt`
                // 的 `CHECKQUEST 155 1`），按 Crystal 的语义它们都在问「任务 155 完成了吗」。
                // 本作照抄这个行为，见 `docs/world/quests.md` 的异常清单。
                let Some(index) = parse_i32(&check.params, 0) else {
                    return CheckOutcome::Fail;
                };
                let mode = check
                    .params
                    .get(1)
                    .map(|value| value.to_uppercase())
                    .unwrap_or_default();
                let player = &self.players[&player_id];
                let found = if mode == "ACTIVE" {
                    player
                        .quests
                        .iter()
                        .any(|progress| progress.quest_index == index && progress.is_in_progress())
                } else {
                    player.completed_quests.contains(&index)
                };
                found.into()
            }
            NpcCheckKind::Flag => {
                let Some(index) = parse_i32(&check.params, 0) else {
                    return CheckOutcome::Fail;
                };
                let Some(value) = parse_u32(&check.params, 1) else {
                    return CheckOutcome::Fail;
                };
                // `:2337`：下标越界也算不通过。
                if !(0..=FLAG_INDEX_COUNT).contains(&index) {
                    return CheckOutcome::Fail;
                }
                let flag = self.players[&player_id].npc.flags.contains(&index);
                (flag == (value != 0)).into()
            }
            NpcCheckKind::Random => {
                // `:2414-2422`：`failed = 0 != Envir.Random.Next(0, tempInt)`，
                // 也就是「摇到 0 才算通过」，概率是 1/n（不是百分比）。
                let Some(bound) = parse_i32(&check.params, 0) else {
                    return CheckOutcome::Fail;
                };
                if bound <= 0 {
                    // `.NET Random.Next(0, 0)` 返回 0（min == max），即通过。
                    return CheckOutcome::Pass;
                }
                (self.rng.next_bounded(bound as u32) == 0).into()
            }
            NpcCheckKind::Unsupported | NpcCheckKind::Unknown => {
                unsupported.push(check.source.clone());
                // 不支持的条件按「不满足」处理：走 else 分支，绝不让没验证过的分支放行。
                CheckOutcome::Fail
            }
        }
    }

    /// `CHECKITEM <名字> [数量] [持久(秒)]`（Crystal `:2176-2207`）。
    ///
    /// 语义照抄：遍历背包里模板相同的物品；给了持久就跳过 `CurrentDura < 持久*1000` 的；
    /// 数量够就通过。`[数量]` 缺省 1。
    fn evaluate_item_check(&self, player_id: u32, check: &NpcCheck) -> CheckOutcome {
        let Some(name) = check.params.first() else {
            return CheckOutcome::Fail;
        };
        let Some(wanted) = parse_u16_at(&check.params, 1) else {
            return CheckOutcome::Fail;
        };
        let dura = parse_u16_at(&check.params, 2);
        let Some(info) = self.item_info_by_name(name) else {
            // 模板不存在：Crystal 的 `info` 为 null，背包里不可能有 `Info == null` 的物品，
            // 于是 `count` 减不下去、判定失败。
            return CheckOutcome::Fail;
        };
        let item_index = info.index;
        let Some(player) = self.players.get(&player_id) else {
            return CheckOutcome::Fail;
        };
        let mut remaining = wanted;
        for item in player.inventory.iter().flatten() {
            if item.item_index != item_index {
                continue;
            }
            if let Some(dura) = dura {
                if item.current_dura < dura.saturating_mul(1000) {
                    continue;
                }
            }
            if remaining > item.count {
                remaining -= item.count;
                continue;
            }
            return CheckOutcome::Pass;
        }
        CheckOutcome::Fail
    }

    /// 渲染一行 `#SAY`：替换 `<$占位符>`、去掉 `{文字/颜色}` 颜色标记。
    ///
    /// 占位符来自 Crystal `NPCSegment.ReplaceValue`（`:1266-1641`）。与 Crystal 的差异：
    ///
    /// * Crystal 用**贪婪**正则 `\<\$(.*)\>` 一次只替换「最宽的一个」匹配，遇到一行里多个
    ///   占位符时会留下未替换的；本作逐个替换（更符合直觉，见 `docs/world/npc.md`）；
    /// * 未知占位符原样保留（Crystal 也是原样返回）；
    /// * `{文字/颜色}` 是 **客户端**渲染的颜色标记（`Client/MirScenes/Dialogs/NPCDialogs.cs`），
    ///   本作客户端还不支持，服务端直接剥掉外壳只留文字。
    fn render_say_line(&self, player_id: u32, npc: &Npc, line: &str) -> String {
        let Some(player) = self.players.get(&player_id) else {
            return line.to_owned();
        };
        let mut text = String::with_capacity(line.len());
        let mut rest = line;
        while let Some(start) = rest.find("<$") {
            text.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            let Some(end) = after.find('>') else {
                text.push_str(rest);
                rest = "";
                break;
            };
            let key = &after[..end];
            if let Some(value) = self.placeholder_value(player, npc, key) {
                text.push_str(&value);
            } else {
                text.push_str("<$");
                text.push_str(key);
                text.push('>');
            }
            rest = &after[end + 1..];
        }
        text.push_str(rest);
        strip_color_markup(&text).trim_end().to_owned()
    }

    /// 一个占位符的取值（`None` = 不认识，保留原样）。
    fn placeholder_value(&self, player: &Player, npc: &Npc, key: &str) -> Option<String> {
        let key = key.to_uppercase();
        let value = match key.as_str() {
            "NPCNAME" => npc.display_name(),
            "USERNAME" => player.name.clone(),
            "LEVEL" => player.level.to_string(),
            "CLASS" => class_name(player.class).to_owned(),
            "MAP" => player.map.clone(),
            "X_COORD" => player.position.x.to_string(),
            "Y_COORD" => player.position.y.to_string(),
            "HP" => player.hp.to_string(),
            "MAXHP" => player.stats.hp.to_string(),
            "MP" => player.mp.to_string(),
            "MAXMP" => player.stats.mp.to_string(),
            "GAMEGOLD" => player.gold.to_string(),
            "PKPOINT" => "0".to_owned(),
            "USERCOUNT" => self.players.len().to_string(),
            "ARMOUR" | "WEAPON" | "RING_L" | "RING_R" | "BRACELET_L" | "BRACELET_R"
            | "NECKLACE" | "BELT" | "BOOTS" | "HELMET" | "AMULET" | "STONE" | "TORCH" => {
                self.equipment_name(player, &key)?
            }
            _ => return None,
        };
        Some(value)
    }

    /// 装备槽的显示名（Crystal `ReplaceValue` 的 `ARMOUR`/`WEAPON`/… 分支，`:1498-1549`）。
    ///
    /// 简化：Crystal 从语言表取「无武器」之类的文案，本作没有这些键，空槽直接返回 `"无"`。
    fn equipment_name(&self, player: &Player, key: &str) -> Option<String> {
        let slot = match key {
            "ARMOUR" => crate::items::equipment_slot_for(ItemType::Armour),
            "WEAPON" => crate::items::equipment_slot_for(ItemType::Weapon),
            "RING_L" | "RING_R" => crate::items::equipment_slot_for(ItemType::Ring),
            "BRACELET_L" | "BRACELET_R" => crate::items::equipment_slot_for(ItemType::Bracelet),
            "NECKLACE" => crate::items::equipment_slot_for(ItemType::Necklace),
            "BELT" => crate::items::equipment_slot_for(ItemType::Belt),
            "BOOTS" => crate::items::equipment_slot_for(ItemType::Boots),
            "HELMET" => crate::items::equipment_slot_for(ItemType::Helmet),
            "AMULET" => crate::items::equipment_slot_for(ItemType::Amulet),
            "STONE" => crate::items::equipment_slot_for(ItemType::Stone),
            "TORCH" => crate::items::equipment_slot_for(ItemType::Torch),
            _ => None,
        }?;
        let item = player.equipment.get(slot).and_then(Option::as_ref);
        match item.and_then(|item| self.item_info(item.item_index)) {
            Some(info) => Some(crate::items::friendly_name(&info.name)),
            None => Some("无".to_owned()),
        }
    }

    /// 打开服务端面板（Crystal `ProcessSpecial`，`NPCScript.cs:919-1136`）。
    ///
    /// 只实现买卖/修理三个面板与「创建行会」的名字请求；其余内建页面（仓库、锻造、
    /// 攻城…）本作没有，静默跳过。
    fn open_npc_panel(&mut self, player_id: u32, npc: &Npc, page_key: &str) -> Vec<&'static str> {
        let rate = self.price_rate(npc);
        let mut opened = Vec::new();
        match page_key {
            BUY_KEY | BUY_SELL_KEY => {
                // `:927-953`：先发商品表，`[@BUYSELL]` 再发一次「卖」面板。
                let goods = self.npc_goods(npc);
                self.send_to_player(
                    player_id,
                    &ServerPacket::NPCGoods(NPCGoods {
                        list: goods,
                        rate,
                        type_: PanelType::Buy,
                        hide_added_stats: GOODS_HIDE_ADDED_STATS,
                    })
                    .encode_frame(),
                );
                opened.push("buy");
                if page_key == BUY_SELL_KEY {
                    self.send_to_player(
                        player_id,
                        &ServerPacket::NPCSell(NPCSell {}).encode_frame(),
                    );
                    opened.push("sell");
                }
            }
            SELL_KEY => {
                // `:968-970`
                self.send_to_player(player_id, &ServerPacket::NPCSell(NPCSell {}).encode_frame());
                opened.push("sell");
            }
            REPAIR_KEY => {
                // `:971-973`
                self.send_to_player(
                    player_id,
                    &ServerPacket::NPCRepair(NPCRepair { rate }).encode_frame(),
                );
                opened.push("repair");
            }
            SPECIAL_REPAIR_KEY => {
                // `:974-976`
                self.send_to_player(
                    player_id,
                    &ServerPacket::NPCSRepair(NPCSRepair { rate }).encode_frame(),
                );
                opened.push("srepair");
            }
            STORAGE_KEY => {
                // `case StorageKey:`（`NPCScript.cs:1002-1006`）：
                // `ResetStorageUnlock(); SendStorage(); Enqueue(new S.NPCStorage());`
                // `SendStorage` 每个连接只在下一次打开时不再发 `S.UserStorage`（见 [`crate::storage`]）。
                self.send_storage_panels(player_id);
                opened.push("storage");
            }
            SEND_PARCEL_KEY => {
                // `case SendParcelKey: player.Enqueue(new S.MailSendRequest());`（`:1078-1080`）。
                self.mail_open_send_page(player_id);
                opened.push("sendmail");
            }
            COLLECT_PARCEL_KEY => {
                // `case CollectParcelKey:`（`:1081-1098`）：标记包裹已受理 → `S.ParcelCollected`
                // → `GetMail()`（`S.ReceiveMail`）。
                if let Err(error) = self.mail_collect_parcel_page(player_id) {
                    tracing::debug!(player = player_id, %error, "邮局受理包裹失败");
                }
                opened.push("collectmail");
            }
            GUILD_CREATE_KEY => {
                // `case GuildCreateKey:`（`NPCScript.cs:1050-1062`）：
                // 等级不够只提示；已在行会里也只提示；否则置 `CanCreateGuild` 并发
                // `S.GuildNameRequest`（`:1057-1058`）。
                match self.request_guild_name(player_id) {
                    Ok(()) => {
                        if let Some(player) = self.players.get_mut(&player_id) {
                            player.npc.can_create_guild = true;
                        }
                        opened.push("guild");
                    }
                    Err(error) => {
                        tracing::debug!(
                            npc = %npc.name,
                            %error,
                            "请求创建行会被拒绝"
                        );
                        self.send_system_chat(player_id, &error.message());
                    }
                }
            }
            _ => {}
        }
        opened
    }

    /// 价格倍率：Crystal `NPCScript.PriceRate`（`NPCScript.cs:111-133`）。
    ///
    /// `callingNPC.Info.Rate / 100F`；没有攻城系统，所以分支恒定。找不到 NPC 时返回 1.0。
    #[must_use]
    pub fn price_rate(&self, npc: &Npc) -> f32 {
        f32::from(npc.rate) / 100.0
    }
}

/// 页面执行结果。
#[derive(Debug, Clone, Default)]
struct PageRun {
    /// 正文（已渲染）。
    speech: Vec<String>,
    /// 这一页暴露的按钮（结果匹配的分支）。
    buttons: BTreeSet<String>,
    /// `GOTO` 的目标页面（Crystal 的 `DelayedAction`）。
    next_page: Option<String>,
    /// 没实现的条件。
    unsupported_checks: Vec<String>,
    /// 没实现的动作。
    unsupported_actions: Vec<String>,
    /// 任务动作（本作扩展的 `ACCEPTQUEST`/`FINISHQUEST`/`ABANDONQUEST`）的失败原因。
    quest_events: Vec<String>,
}

/// 一条检查的结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CheckOutcome {
    /// 通过。
    Pass,
    /// 不通过。
    Fail,
    /// 运算符写错：Crystal 打日志后整段通过（`NPCSegment.cs:2116-2120`）。
    InvalidOperator,
}

impl From<bool> for CheckOutcome {
    fn from(passed: bool) -> Self {
        if passed { Self::Pass } else { Self::Fail }
    }
}

// ---------------------------------------------------------------------------
// World：买卖与修理
// ---------------------------------------------------------------------------

impl World {
    /// 买（`C.BuyItem`）。
    ///
    /// 入口检查照抄 Crystal `PlayerObject.BuyItem`（`:7944-7969`）：
    /// `Dead` / `count < 1` / 面板类型 / 当前页 key / 距离。真正的交易在
    /// `NPCScript.Buy`（`:1138-1237`）：
    ///
    /// 1. 按 `UniqueID` 在商品表里找货，找不到就什么都不做；
    /// 2. `count == 0 || count > Info.StackSize` → 什么都不做；
    /// 3. `cost = goods.Price() * PriceRate`（`:1187-1189`，`float` 乘完向零截断）；
    /// 4. `cost > Gold` → 什么都不做；
    /// 5. `CanGainItem` 为假 → 什么都不做；
    /// 6. 扣钱 → 本人 `S.LoseGold`（`:1208-1209`）→ `GainItem`（本人 `S.GainedItem`）。
    ///
    /// **扩展**：开启 [`World::enforce_bag_weight`] 时额外查负重（Crystal 只在拾取时查）。
    pub fn buy_item(
        &mut self,
        player_id: u32,
        item_index: u64,
        count: u16,
        panel: PanelType,
    ) -> Result<BuyReport, NpcError> {
        let Some(player) = self.players.get(&player_id) else {
            return Err(NpcError::NotInWorld(player_id));
        };
        if player.dead {
            return Err(NpcError::Dead { player: player_id });
        }
        if count < 1 {
            return Err(NpcError::InvalidCount { count, stack: 0 });
        }
        if panel != PanelType::Buy {
            return Err(NpcError::UnsupportedPanel(panel));
        }
        if !BUY_PAGES.contains(&player.npc.page_key.as_str()) {
            return Err(NpcError::NotShopPage {
                page: player.npc.page_key.clone(),
            });
        }
        let npc = self.shop_npc(player_id)?;
        if !npc.position.in_range(player.position, NPC_TALK_RANGE) {
            return Err(NpcError::OutOfRange {
                object_id: npc.object_id,
                range: NPC_TALK_RANGE,
            });
        }
        let goods = self
            .npc_goods(&npc)
            .into_iter()
            .find(|goods| goods.unique_id == item_index)
            .ok_or(NpcError::NoSuchGoods { item_index })?;
        let info = self
            .item_info(goods.item_index)
            .cloned()
            .ok_or(NpcError::NoSuchGoods { item_index })?;
        if count > info.stack_size {
            return Err(NpcError::InvalidCount {
                count,
                stack: info.stack_size,
            });
        }
        // `NPCScript.cs:1185`：`goods.Count = count;`（商品条目上的数量就是这次买的数量）。
        let priced = UserItem {
            count,
            ..goods.clone()
        };
        let cost = apply_price_rate(item_price(&info, &priced), self.price_rate(&npc));
        let gold = self.players[&player_id].gold;
        if i64::from(cost) > gold {
            return Err(NpcError::NotEnoughGold { cost, gold });
        }
        // 简化：Crystal 用 `Envir.CreateFreshItem(goods.Info)`（全新 UUID、满持久），
        // 本作等价于 `create_fresh_item`。
        let item = UserItem {
            count,
            ..self.create_fresh_item(&info)
        };
        if !self.can_gain_item(player_id, &info, count) {
            return Err(NpcError::InventoryFull);
        }
        if self.enforce_bag_weight {
            let added = u32::try_from(item_weight(&info, &item).max(0)).unwrap_or(0);
            let limit =
                u32::try_from(self.players[&player_id].stats.bag_weight.max(0)).unwrap_or(0);
            let total = self.bag_weight(player_id).saturating_add(added);
            if total > limit {
                return Err(NpcError::TooHeavy {
                    weight: total,
                    limit,
                });
            }
        }
        // `:1208-1209`：扣钱 + `S.LoseGold`。
        self.lose_gold(player_id, cost)
            .map_err(|_| NpcError::NotEnoughGold { cost, gold })?;
        // `:1217`：`GainItem(item)` → `S.GainedItem`。
        self.send_to_player(
            player_id,
            &ServerPacket::GainedItem(mir_protocol::packets::server::GainedItem {
                item: item.clone(),
            })
            .encode_frame(),
        );
        let added = self.add_item_to_inventory(player_id, item.clone());
        debug_assert!(added, "can_gain_item 通过后必然放得下");
        if !added {
            return Err(NpcError::InventoryFull);
        }
        // 买东西也算「获得物品」，收集类任务目标要跟着刷新（见 `crate::quests`）。
        self.refresh_quest_items(player_id);
        Ok(BuyReport { item, cost })
    }

    /// 卖（`C.SellItem`）。
    ///
    /// 逐条对应 Crystal `PlayerObject.SellItem`（`:7988-8091`）：
    ///
    /// 1. 无论成败都先发 `S.SellItem { UniqueID, Count, Success }`（`:7990`、`:8082-8083`、
    ///    `:8090`）——失败时 `Success = false`；
    /// 2. `Dead` / `count == 0` / 当前页不是 `[@SELL]`/`[@BUYSELL]` → 失败；
    /// 3. 背包里找不到 UUID 或数量不够 → 失败；
    /// 4. `BindMode.DontSell` → 失败；
    /// 5. 脚本 `[Types]` 非空且不含该类型 → 系统聊天 + 失败（`:8041-8046`）；
    /// 6. 可堆叠且只卖一部分 → 拆出一件新物品（`:8048-8062`），否则整格清空；
    /// 7. `Success = true` → `GainGold(Price() / 2)`（`:8082-8084`）。
    ///
    /// 简化：
    /// * `GoodsOn` 的「回购列表」（`callingNPC.BuyBack`，`:8066-8080`）未实现；
    /// * `RentalInformation` 的 `DontSell`（`:8033-8037`）未实现（本作没有租借）；
    /// * `RefreshBagWeight()`（`:8085`）不需要——本作的负重是即时算的。
    pub fn sell_item(
        &mut self,
        player_id: u32,
        unique_id: u64,
        count: u16,
    ) -> Result<SellReport, NpcError> {
        let failure = SellItem {
            unique_id,
            count,
            success: false,
        };
        let Some(player) = self.players.get(&player_id) else {
            return Err(NpcError::NotInWorld(player_id));
        };
        if player.dead || count == 0 {
            self.send_to_player(player_id, &ServerPacket::SellItem(failure).encode_frame());
            return Err(NpcError::Dead { player: player_id });
        }
        if !SELL_PAGES.contains(&player.npc.page_key.as_str()) {
            self.send_to_player(player_id, &ServerPacket::SellItem(failure).encode_frame());
            return Err(NpcError::NotShopPage {
                page: player.npc.page_key.clone(),
            });
        }
        let npc = self.shop_npc(player_id)?;
        if !npc.position.in_range(player.position, NPC_TALK_RANGE) {
            // Crystal 这里直接 `return`，连失败包都不发（`:8008`）。保持一致。
            return Err(NpcError::OutOfRange {
                object_id: npc.object_id,
                range: NPC_TALK_RANGE,
            });
        }
        let Some((slot, item)) = self.find_inventory_item(player_id, unique_id) else {
            self.send_to_player(player_id, &ServerPacket::SellItem(failure).encode_frame());
            return Err(NpcError::NoSuchItem(unique_id));
        };
        if count > item.count {
            self.send_to_player(player_id, &ServerPacket::SellItem(failure).encode_frame());
            return Err(NpcError::InvalidCount {
                count,
                stack: item.count,
            });
        }
        let Some(info) = self.item_info(item.item_index).cloned() else {
            self.send_to_player(player_id, &ServerPacket::SellItem(failure).encode_frame());
            return Err(NpcError::NoSuchItem(unique_id));
        };
        if info.bind.contains(BindMode::DontSell) {
            self.send_to_player(player_id, &ServerPacket::SellItem(failure).encode_frame());
            return Err(NpcError::CannotSell(unique_id));
        }
        if let Some(script) = self.npc_script(&npc.file_name) {
            if !script.types().is_empty() && !script.types().contains(&(info.item_type as i32)) {
                self.send_system_chat(player_id, "这个 NPC 不收购这种物品。");
                self.send_to_player(player_id, &ServerPacket::SellItem(failure).encode_frame());
                return Err(NpcError::TypeNotAllowed);
            }
        }
        // `:8048-8062`：可堆叠且只卖一部分 → 复制出一件，原格数量减少。
        let sold = if info.stack_size > 1 && count != item.count {
            let mut part = self.create_fresh_item(&info);
            part.count = count;
            if let Some(slot_item) = self
                .players
                .get_mut(&player_id)
                .and_then(|player| player.inventory.get_mut(slot))
                .and_then(Option::as_mut)
            {
                slot_item.count -= count;
            }
            part
        } else {
            if let Some(slot_item) = self
                .players
                .get_mut(&player_id)
                .and_then(|player| player.inventory.get_mut(slot))
            {
                *slot_item = None;
            }
            item
        };
        let gold = item_price(&info, &sold) / 2;
        // `:8082-8083`：先发成功包，再进账。
        self.send_to_player(
            player_id,
            &ServerPacket::SellItem(SellItem {
                unique_id,
                count,
                success: true,
            })
            .encode_frame(),
        );
        self.gain_gold(player_id, gold);
        Ok(SellReport {
            unique_id,
            count,
            gold,
            item_name: info.name.clone(),
        })
    }

    /// 修（`C.RepairItem`）。
    ///
    /// 逐条对应 Crystal `PlayerObject.RepairItem`（`:8092-8160`）：
    ///
    /// 1. **先**发 `S.RepairItem { UniqueID }`（`:8094`），客户端用它清掉「修理中」标记；
    /// 2. `Dead` → 返回；当前页不是 `[@REPAIR]` → 返回；
    /// 3. 背包里找不到物品 → 返回（不再发别的包）；
    /// 4. `BindMode.DontRepair` → 系统聊天「无法修理」；
    /// 5. 脚本 `[Types]` 非空且不含该类型 → 系统聊天「这里不能修」；
    /// 6. `cost = RepairPrice() * PriceRate`；`cost > Gold` → 返回；
    /// 7. 扣钱 → `S.LoseGold`（`:8148-8149`）；
    /// 8. `MaxDura = max(0, MaxDura - (MaxDura - CurrentDura)/30)`（`:8152`，修一次掉一点上限），
    ///    `CurrentDura = MaxDura`；
    /// 9. `S.ItemRepaired { UniqueID, MaxDura, CurrentDura }`（`:8157`）。
    ///
    /// 特修（`special`）不在本作范围内：没有对应客户端包（`C.SRepairItem` 未接入）。
    pub fn repair_item(
        &mut self,
        player_id: u32,
        unique_id: u64,
    ) -> Result<RepairReport, NpcError> {
        self.send_to_player(
            player_id,
            &ServerPacket::RepairItem(mir_protocol::packets::server::RepairItem { unique_id })
                .encode_frame(),
        );
        let Some(player) = self.players.get(&player_id) else {
            return Err(NpcError::NotInWorld(player_id));
        };
        if player.dead {
            return Err(NpcError::Dead { player: player_id });
        }
        if !REPAIR_PAGES.contains(&player.npc.page_key.as_str()) {
            return Err(NpcError::NotShopPage {
                page: player.npc.page_key.clone(),
            });
        }
        let npc = self.shop_npc(player_id)?;
        if !npc.position.in_range(player.position, NPC_TALK_RANGE) {
            return Err(NpcError::OutOfRange {
                object_id: npc.object_id,
                range: NPC_TALK_RANGE,
            });
        }
        let Some((slot, item)) = self.find_inventory_item(player_id, unique_id) else {
            return Err(NpcError::NoSuchItem(unique_id));
        };
        let Some(info) = self.item_info(item.item_index).cloned() else {
            return Err(NpcError::NoSuchItem(unique_id));
        };
        if info.bind.contains(BindMode::DontRepair) {
            self.send_system_chat(player_id, "这件物品无法修理。");
            return Err(NpcError::CannotRepair(unique_id));
        }
        if let Some(script) = self.npc_script(&npc.file_name) {
            if !script.types().is_empty() && !script.types().contains(&(info.item_type as i32)) {
                self.send_system_chat(player_id, "这里不能修理这种物品。");
                return Err(NpcError::TypeNotAllowed);
            }
        }
        let cost = apply_price_rate(repair_price(&info, &item), self.price_rate(&npc));
        let gold = self.players[&player_id].gold;
        if i64::from(cost) > gold {
            return Err(NpcError::NotEnoughGold { cost, gold });
        }
        if cost > 0 {
            self.lose_gold(player_id, cost)
                .map_err(|_| NpcError::NotEnoughGold { cost, gold })?;
        }
        // `:8152`：`MaxDura = Math.Max(0, MaxDura - (MaxDura - CurrentDura) / 30)`。
        let wear = item.max_dura.saturating_sub(item.current_dura) / 30;
        let max_dura = item.max_dura.saturating_sub(wear);
        if let Some(slot_item) = self
            .players
            .get_mut(&player_id)
            .and_then(|player| player.inventory.get_mut(slot))
            .and_then(Option::as_mut)
        {
            slot_item.max_dura = max_dura;
            slot_item.current_dura = max_dura;
        }
        self.send_to_player(
            player_id,
            &ServerPacket::ItemRepaired(mir_protocol::packets::server::ItemRepaired {
                unique_id,
                max_dura,
                current_dura: max_dura,
            })
            .encode_frame(),
        );
        Ok(RepairReport {
            unique_id,
            cost,
            max_dura,
            current_dura: max_dura,
        })
    }

    /// 当前对话的 NPC；没在对话或 NPC 已经不在世界里就报 [`NpcError::NoDialog`]。
    fn shop_npc(&self, player_id: u32) -> Result<Npc, NpcError> {
        let Some(player) = self.players.get(&player_id) else {
            return Err(NpcError::NotInWorld(player_id));
        };
        let npc_id = player.npc.npc_object_id;
        if npc_id == 0 {
            return Err(NpcError::NoDialog);
        }
        self.npc(npc_id).cloned().ok_or(NpcError::NoSuchNpc(npc_id))
    }

    /// 给本人发一条系统聊天。
    pub(crate) fn send_system_chat(&self, player_id: u32, message: &str) {
        self.send_to_player(
            player_id,
            &ServerPacket::Chat(mir_protocol::packets::server::Chat {
                message: message.to_owned(),
                type_: ChatType::System,
            })
            .encode_frame(),
        );
    }
}

// ---------------------------------------------------------------------------
// 小工具
// ---------------------------------------------------------------------------

/// `MirClass` 的 C# `ToString()` 结果（`NPCSegment.ReplaceValue` 的 `<$CLASS>`、`CheckVisible` 的职业比较）。
#[must_use]
pub fn class_name(class: MirClass) -> &'static str {
    match class {
        MirClass::Warrior => "Warrior",
        MirClass::Wizard => "Wizard",
        MirClass::Taoist => "Taoist",
        MirClass::Assassin => "Assassin",
        MirClass::Archer => "Archer",
    }
}

/// 解析职业名（Crystal `MirClass.TryParse(param[0], true, …)`，忽略大小写）。
fn parse_class(text: &str) -> Option<MirClass> {
    [
        MirClass::Warrior,
        MirClass::Wizard,
        MirClass::Taoist,
        MirClass::Assassin,
        MirClass::Archer,
    ]
    .into_iter()
    .find(|class| class_name(*class).eq_ignore_ascii_case(text))
}

/// 解析性别名（Crystal `MirGender.TryParse(param[0], false, …)`，**区分**大小写）。
fn parse_gender(text: &str) -> Option<MirGender> {
    match text {
        "Male" => Some(MirGender::Male),
        "Female" => Some(MirGender::Female),
        _ => None,
    }
}

fn parse_i32(params: &[String], index: usize) -> Option<i32> {
    params.get(index)?.parse().ok()
}

fn parse_u32(params: &[String], index: usize) -> Option<u32> {
    params.get(index)?.parse().ok()
}

fn parse_u16_at(params: &[String], index: usize) -> Option<u16> {
    params.get(index)?.parse().ok()
}

/// 取第 `index` 个参数作为比较运算符。
fn param_compare_op(params: &[String], index: usize) -> Option<CompareOp> {
    CompareOp::parse(params.get(index)?)
}

/// 去掉 `{文字/颜色}` 的颜色外壳，只留文字（客户端颜色标记，本作客户端不支持）。
///
/// 支持嵌套：`{<$WEAPON>/CORAL}` 这类占位符在替换之后就是 `{屠龙/CORAL}`；
/// 按钮 `<文字/@goto>` 原样保留（`<` `>` 内部不参与切分）。
#[must_use]
pub fn strip_color_markup(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth = 0usize;
    let mut buffer = String::new();
    // 每一层 `{…}` 的内容；遇到 `/颜色` 时把前缀留下。
    let mut frames: Vec<String> = Vec::new();
    for ch in text.chars() {
        match ch {
            '{' => {
                depth += 1;
                frames.push(buffer);
                buffer = String::new();
            }
            '}' if depth > 0 => {
                depth -= 1;
                let inner = buffer;
                let text_part = match inner.rfind('/') {
                    // 颜色代码只由字母数字组成；否则把 `/` 当正文。
                    Some(index) if inner[index + 1..].chars().all(char::is_alphanumeric) => {
                        inner[..index].to_owned()
                    }
                    _ => inner,
                };
                buffer = frames.pop().unwrap_or_default();
                buffer.push_str(text_part.trim_end());
            }
            other => buffer.push(other),
        }
    }
    // 没闭合的 `{` 原样吐回去（脚本写错时不吞字）。
    for frame in frames {
        out.push('{');
        out.push_str(&frame);
    }
    out.push_str(&buffer);
    out
}
