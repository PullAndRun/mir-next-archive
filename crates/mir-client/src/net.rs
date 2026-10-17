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
//! | `S.UserInformation` | 自己、`inventory`、`magics`、金币、HP/MP |
//! | `S.NewItemInfo` | `item_infos`：物品名/图标号/价格/堆叠上限 |
//! | `S.GainedItem` | 并入背包（可堆叠物品先并堆）；系统聊天 |
//! | `S.MoveItem` | 背包内交换（`grid = Inventory` 且 `success`） |
//! | `S.EquipItem` / `S.RemoveItem` | 装备离/回背包（见下方已知差异） |
//! | `S.DeleteItem` | 按数量移除背包物品 |
//! | `S.RefreshItem` | 用新数据替换同 `unique_id` 的物品（Crystal 的 `ItemChanged`） |
//! | `S.ItemRepaired` | 更新持久度 |
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
//! * **换下的旧装备不回背包快照**：`S.EquipItem` 只带被穿上的物品 ID，换下来的旧装备
//!   数据不在包里，要等下一次 `S.UserInformation` 才能补全；`S.RemoveItem` 同理。
//! * **`S.NPCResponse` 不带 NPC 对象 ID**：客户端取离自己最近的 NPC 作为对话对象
//!   （[`WorldSnapshot::nearest_npc`]）；世界点击 NPC 时会直接记下被点的对象。
//! * **最大 HP/MP 不在协议里**：状态栏进度条用「本次会话见过的最高值」近似，
//!   数字显示为 `当前/高水位`，见 [`SelfPlayer::hp_max`]。

use std::collections::BTreeMap;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use mir_core::Point;
use mir_core::enums::{ChatType, MirClass, MirDirection, MirGender, MirGridType, PanelType, Spell};
use mir_protocol::codec::FrameBuffer;
use mir_protocol::data::{ClientMagic, ItemInfo, UserItem};
use mir_protocol::packets::client as c;
use mir_protocol::{ClientPacket, DateTimeBinary, ServerPacket};
use mir_render::{EntityKind, EntityView};

/// 连接超时。
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// 单次 `read` 的超时：决定 UI 命令（走路/聊天）的最大延迟。
const READ_TIMEOUT: Duration = Duration::from_millis(20);
/// `KeepAlive` 间隔（Crystal 客户端在没有待发包时定期发送）。
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(5);
/// 聊天记录上限。
const CHAT_LOG_LIMIT: usize = 100;
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

/// 网络线程 → UI 的通知。
#[derive(Debug, Clone)]
pub enum NetEvent {
    /// 世界快照更新（登录阶段也会发，`status` 在快照里）。
    ///
    /// 装箱是为了避免每个 `NetEvent`（包括错误/关闭）都被快照撑大。
    Snapshot(Box<WorldSnapshot>),
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
    /// 发型（暂未用于绘制）。
    pub hair: u8,
    /// 金币。
    pub gold: u32,
    /// 是否死亡。
    pub dead: bool,
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
    /// 是否死亡。
    pub dead: bool,
    /// 生命百分比（`S.ObjectHealth`；怪物才有）。
    pub hp_percent: Option<u8>,
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemInfoView {
    /// 模板编号。
    pub index: i32,
    /// 名称（服务端已本地化）。
    pub name: String,
    /// `items.wzl` 图像号。
    pub image: u16,
    /// 基础价格。
    pub price: u32,
    /// 最大堆叠数（1 表示不可堆叠）。
    pub stack_size: u16,
}

impl ItemInfoView {
    /// 从协议模板里抽取界面字段。
    #[must_use]
    pub fn from_item_info(info: &ItemInfo) -> Self {
        Self {
            index: info.index,
            name: info.name.clone(),
            image: info.image,
            price: info.price,
            stack_size: info.stack_size,
        }
    }
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
    /// 聊天记录（最新在末尾，最多 [`CHAT_LOG_LIMIT`] 条）。
    pub chat: Vec<ChatLine>,
    /// 背包（`S.UserInformation.Inventory`；`None` 槽位为空，长度固定 46）。
    pub inventory: Vec<Option<InventoryItem>>,
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

    /// 某个包收到过多少次（测试/调试用）。
    #[must_use]
    pub fn packet_count(&self, name: &str) -> u32 {
        self.packet_counts.get(name).copied().unwrap_or(0)
    }

    /// 用一条服务端包更新快照。
    ///
    /// 网络线程内部每解码一个包就调用一次；无头 UI 测试也用它注入假的
    /// `NPCResponse`/`NPCGoods`/`UserInformation`，不需要真实 NPC。
    pub fn apply_packet(&mut self, packet: &ServerPacket) {
        apply_packet_to(self, packet);
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
    /// 自己的外观暂时固定为 0 号身体（`UserInformation` 不带装备外观；等号角/装备
    /// 外观表接进来后再填）。自己的实体 ID 如果同时也出现在 `entities` 里，会被跳过，
    /// 避免画两遍。
    #[must_use]
    pub fn entity_views(&self) -> Vec<EntityView> {
        let mut views = Vec::with_capacity(self.entities.len() + 1);
        let self_id = self.self_player.as_ref().map(|me| me.object_id);
        if let Some(me) = &self.self_player {
            views.push(EntityView {
                position: me.position,
                direction: me.direction,
                kind: EntityKind::Player,
                image: 0,
                name: Some(me.name.clone()),
                dead: me.dead,
                hp_percent: None,
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
                name: Some(entity.name.clone()),
                dead: entity.dead,
                hp_percent: entity.hp_percent,
            });
        }
        views
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
            handle_command(&mut stream, &snapshot, command)?;
        }

        match stream.read(&mut buffer) {
            Ok(0) => {
                snapshot.status = NetStatus::Closed;
                send_snapshot(events, &snapshot);
                return Ok(());
            }
            Ok(read) => {
                frames.push(&buffer[..read]);
                while let Some((index, payload)) = frames.next_frame()? {
                    match ServerPacket::decode(index, &payload) {
                        Ok(packet) => {
                            apply_packet_to(&mut snapshot, &packet);
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
    snapshot: &WorldSnapshot,
    command: NetCommand,
) -> Result<()> {
    if snapshot.status != NetStatus::InGame {
        return Ok(());
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
fn apply_packet_to(snapshot: &mut WorldSnapshot, packet: &ServerPacket) {
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
                    WorldEntity {
                        object_id: npc.object_id,
                        kind: EntityKind::Npc,
                        name: npc.name.clone(),
                        position: npc.location,
                        direction: MirDirection::Down,
                        image: npc.image,
                        dead: false,
                        hp_percent: None,
                    },
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
            }
        }
        ServerPacket::UserInformation(info) => {
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
            });
            // `Inventory = null` 表示「客户端保留旧数据」（Crystal 的语义）。
            if let Some(inventory) = &info.inventory {
                snapshot.inventory = inventory
                    .iter()
                    .map(|item| item.as_ref().map(InventoryItem::from_user_item))
                    .collect();
            }
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
            let image = u16::try_from(player.armour.max(0)).unwrap_or(0);
            upsert_entity(
                snapshot,
                WorldEntity {
                    object_id: player.object_id,
                    kind: EntityKind::Player,
                    name: player.name.clone(),
                    position: player.location,
                    direction: player.direction,
                    image,
                    dead: player.dead,
                    hp_percent: None,
                },
            );
        }
        ServerPacket::ObjectMonster(monster) => {
            upsert_entity(
                snapshot,
                WorldEntity {
                    object_id: monster.object_id,
                    kind: EntityKind::Monster,
                    name: monster.name.clone(),
                    position: monster.location,
                    direction: monster.direction,
                    image: u16::from(monster.image),
                    dead: monster.dead,
                    hp_percent: None,
                },
            );
        }
        ServerPacket::ObjectNPC(npc) => {
            upsert_entity(
                snapshot,
                WorldEntity {
                    object_id: npc.object_id,
                    kind: EntityKind::Npc,
                    name: npc.name.clone(),
                    position: npc.location,
                    direction: npc.direction,
                    image: npc.image,
                    dead: false,
                    hp_percent: None,
                },
            );
        }
        ServerPacket::ObjectItem(item) => {
            upsert_entity(
                snapshot,
                WorldEntity {
                    object_id: item.object_id,
                    kind: EntityKind::Item,
                    name: item.name.clone(),
                    position: item.location,
                    direction: MirDirection::Down,
                    image: item.image,
                    dead: false,
                    hp_percent: None,
                },
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
                WorldEntity {
                    object_id: gold.object_id,
                    kind: EntityKind::Item,
                    name: format!("金币 {}", gold.gold),
                    position: gold.location,
                    direction: MirDirection::Down,
                    image,
                    dead: false,
                    hp_percent: None,
                },
            );
        }
        ServerPacket::ObjectRemove(remove) => {
            snapshot.entities.remove(&remove.object_id);
        }
        ServerPacket::ObjectTurn(turn) => {
            update_entity(snapshot, turn.object_id, |entity| {
                entity.position = turn.location;
                entity.direction = turn.direction;
            });
        }
        ServerPacket::ObjectWalk(walk) => {
            update_entity(snapshot, walk.object_id, |entity| {
                entity.position = walk.location;
                entity.direction = walk.direction;
            });
        }
        ServerPacket::ObjectRun(run) => {
            update_entity(snapshot, run.object_id, |entity| {
                entity.position = run.location;
                entity.direction = run.direction;
            });
        }
        ServerPacket::ObjectAttack(attack) => {
            update_entity(snapshot, attack.object_id, |entity| {
                entity.position = attack.location;
                entity.direction = attack.direction;
            });
        }
        ServerPacket::ObjectStruck(struck) => {
            update_entity(snapshot, struck.object_id, |entity| {
                entity.direction = struck.direction;
            });
        }
        ServerPacket::ObjectDied(died) => {
            update_entity(snapshot, died.object_id, |entity| {
                entity.dead = true;
                entity.position = died.location;
                entity.direction = died.direction;
                entity.hp_percent = Some(0);
            });
        }
        ServerPacket::ObjectRevived(revived) => {
            update_entity(snapshot, revived.object_id, |entity| {
                entity.dead = false;
            });
        }
        ServerPacket::ObjectHealth(health) => {
            update_entity(snapshot, health.object_id, |entity| {
                entity.hp_percent = Some(health.percent.min(100));
            });
        }
        ServerPacket::Death(death) => {
            if let Some(me) = &mut snapshot.self_player {
                me.dead = true;
                me.position = death.location;
                me.direction = death.direction;
            }
        }
        ServerPacket::Revived(_) => {
            if let Some(me) = &mut snapshot.self_player {
                me.dead = false;
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
            // 只有背包内的交换能直接映射到界面格子；装备/仓库等网格本作还没有面板。
            if movement.success && movement.grid == MirGridType::Inventory {
                snap_move_inventory(snapshot, movement.from, movement.to);
            }
        }
        ServerPacket::EquipItem(equipped) => {
            // 穿上的物品离开背包；换下来的旧装备不在本作背包快照里（见模块文档的已知差异）。
            if equipped.success && equipped.grid == MirGridType::Inventory {
                remove_inventory_item(snapshot, equipped.unique_id);
            }
        }
        ServerPacket::RemoveItem(removed) => {
            // 脱下装备会回到背包，但 `S.RemoveItem` 不带物品数据，等下一次
            // `S.UserInformation` 再补全；这里至少让旧格子空出来。
            if removed.success && removed.grid == MirGridType::Inventory {
                remove_inventory_item(snapshot, removed.unique_id);
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
            update_entity(snapshot, update.object_id, |entity| {
                entity.image = u16::try_from(update.armour.max(0)).unwrap_or(0);
            });
        }
        ServerPacket::Disconnect(disconnect) => {
            snapshot.status = NetStatus::Failed;
            snapshot.last_error = Some(format!("服务器断开连接（reason = {}）", disconnect.reason));
        }
        _ => {}
    }
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
                object_id,
                kind: EntityKind::Player,
                name: me.name.clone(),
                position: me.position,
                direction: me.direction,
                image: 0,
                dead: me.dead,
                hp_percent: None,
            };
            update(&mut temporary);
            me.position = temporary.position;
            me.direction = temporary.direction;
            me.dead = temporary.dead;
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

#[cfg(test)]
mod tests {
    use super::*;
    use mir_core::enums::Monster as MonsterImage;
    use mir_protocol::data::{ClientMagic, ItemInfo, SelectInfo, UserItem};
    use mir_protocol::packets::server::{
        DeleteItem, MoveItem, NPCGoods, NPCResponse, NewItemInfo, ObjectItem, ObjectMonster,
        ObjectNPC, ObjectWalk, UserInformation, UserLocation,
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
        assert_eq!(snapshot.entity_views().len(), 3);

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
}
