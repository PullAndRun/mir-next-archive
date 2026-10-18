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
//! | `S.ObjectPlayer` / `S.PlayerUpdate` | 其他玩家的坐标/朝向/外观（`PlayerLooks`，含翅膀） |
//! | `S.MountUpdate` | 自己/其他玩家的坐骑外观与是否骑乘（`PlayerLooks.mount`） |
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
//! | `S.NewQuestInfo` | `quest_infos`：任务定义（名字/分组/限时/奖励） |
//! | `S.ChangeQuest` | `quests`：进行中任务的进度文本（`task_list` 已含「击杀 鹿 3/10」）+ 本地接取时刻 |
//! | `S.CompleteQuest` | `completed_quests`：已完成任务编号 |
//! | `S.ShareQuest` | 聊天面板里的一条系统消息 |
//! | `S.UserStorage` / `S.NPCStorage` | `storage`（80 格）+ `storage_open`（打开仓库面板） |
//! | `S.StoreItem` / `S.TakeBackItem` | 背包 ↔ 仓库的搬运回执 |
//! | `S.ReceiveMail` | `mails`：邮件列表（发件人/正文/附件/金币/已读/锁定/已领取） |
//! | `S.MailSendRequest` / `S.MailCost` / `S.MailSent` / `S.ParcelCollected` | 写信页与邮件操作的开关/回执 |
//! | `S.MailLockedItem` | `mail_locked_items`：附件格锁定状态 |
//! | `S.GuildStatus` | `guild`：行会名/等级/经验/资金/人数/我的职位与权限 |
//! | `S.GuildMemberChange` | `guild_ranks`：职位 + 成员（状态码语义见 [`apply_guild_member_change`]） |
//! | `S.GuildNoticeChange` | `guild_notice`：公告行 |
//! | `S.GuildStorageList` / `S.GuildStorageItemChange` | `guild_storage`（112 格） |
//! | `S.GuildStorageGoldChange` | 行会资金 |
//! | `S.GuildBuffList` | `guild_buff_infos`（全部可选）+ `guild_active_buffs`（已获得/激活） |
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
    MirGridType, PanelType, QuestState, QuestType, RequiredType, Spell, SpellEffect, Stat,
};
use mir_protocol::codec::FrameBuffer;
use mir_protocol::data::guild_rank::{GuildMember, GuildRank};
use mir_protocol::data::{
    ClientBuff, ClientMagic, ClientMail, ClientQuestProgress, GuildBuff, GuildBuffInfo,
    GuildStorageItem, ItemInfo, UserItem,
};
use mir_protocol::packets::client as c;
use mir_protocol::packets::server::{GuildBuffList, GuildMemberChange, GuildStorageItemChange};
use mir_protocol::{ClientPacket, DateTimeBinary, ServerPacket};
use mir_render::animation::{ActorAction, BodyKind};
use mir_render::sound::SoundId;
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
/// 个人仓库格数：Crystal `AccountInfo.Storage = new UserItem[80]`（`AccountInfo.cs:69`）
/// 与 `Globals.StorageGridSize = 80`（`Shared/Globals.cs:19`）；见
/// `crates/mir-server/src/storage.rs` 的 `STORAGE_SIZE`。本作没有扩充仓库。
pub const STORAGE_SLOTS: usize = 80;
/// 行会仓库格数：Crystal `GuildStorageItem[] StoredItems = new GuildStorageItem[112]`
/// （`Server/MirDatabase/GuildInfo.cs:22`；服务端 `GUILD_STORAGE_SLOTS`）。
pub const GUILD_STORAGE_SLOTS: usize = 112;
/// 交易格数：Crystal `TradeDialog.Grid = new MirItemCell[5 * 2]`（`TradeDialogs.cs:104`）
/// 与 `UserObject.Trade = new UserItem[10]`（`UserObject.cs:23`）；对方那一栏
/// `GuestTradeDialog.GuestGrid` 同样是 5 × 2（`TradeDialogs.cs:237`）。
pub const TRADE_SLOTS: usize = 10;
/// 队伍人数上限：Crystal `Globals.MaxGroup = 15`（`Shared/Globals.cs:21`；
/// 服务端 `crates/mir-server/src/group.rs` 的 `MAX_GROUP`）。
pub const MAX_GROUP: usize = 15;
/// 快照里保留的伤害事件条数：UI 每帧取「比上次更新」的那些画飘字。
const DAMAGE_EVENT_LIMIT: usize = 32;
/// 快照里保留的光效事件条数。
const EFFECT_EVENT_LIMIT: usize = 16;
/// 快照里保留的音效事件条数：UI 每帧把「比上次多」的那些交给播放器。
const SOUND_EVENT_LIMIT: usize = 32;
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
    /// 放弃任务（`C.AbandonQuest`）。
    AbandonQuest {
        /// 任务编号。
        quest_index: i32,
    },
    /// 阅读邮件（`C.ReadMail`；服务端会把整份列表重新发一次）。
    ReadMail {
        /// 邮件编号。
        mail_id: u64,
    },
    /// 领取邮件附件（`C.CollectParcel`）。
    CollectParcel {
        /// 邮件编号。
        mail_id: u64,
    },
    /// 删除邮件（`C.DeleteMail`）。
    DeleteMail {
        /// 邮件编号。
        mail_id: u64,
    },
    /// 锁定/解锁邮件（`C.LockMail`）。
    LockMail {
        /// 邮件编号。
        mail_id: u64,
        /// `true` = 锁定。
        lock: bool,
    },
    /// 发送邮件（`C.SendMail`）。
    SendMail {
        /// 收件人角色名。
        name: String,
        /// 正文。
        message: String,
        /// 附带金币。
        gold: u32,
        /// 附件物品唯一编号（固定 5 格，`0` 为空）。
        items_idx: [u64; 5],
        /// 是否贴邮票。
        stamped: bool,
    },
    /// 询邮费（`C.MailCost`；服务端回 `S.MailCost`）。
    MailCost {
        /// 附带金币。
        gold: u32,
        /// 附件物品唯一编号（固定 5 格，`0` 为空）。
        items_idx: [u64; 5],
        /// 是否贴邮票。
        stamped: bool,
    },
    /// 存物品到个人仓库（`C.StoreItem`）。
    StoreItem {
        /// 背包格。
        from: i32,
        /// 仓库格。
        to: i32,
    },
    /// 从个人仓库取物品（`C.TakeBackItem`）。
    TakeBackItem {
        /// 仓库格。
        from: i32,
        /// 背包格。
        to: i32,
    },
    /// 行会仓库物品变动（`C.GuildStorageItemChange`）。
    ///
    /// `type`：`0` 存入、`1` 取回、`2` 仓库内移动、`3` 拉取列表
    /// （`crates/mir-server/src/guild_storage.rs` 的模块文档）。
    GuildStorageItemChange {
        /// 变动类型。
        type_: u8,
        /// 起点格（含意随 `type_` 变化）。
        from: i32,
        /// 落点格。
        to: i32,
    },
    /// 行会资金存取（`C.GuildStorageGoldChange`；任何非 0 的 `type` 都按「取」处理）。
    GuildStorageGoldChange {
        /// `0` = 存入，其它 = 取出。
        type_: u8,
        /// 金额。
        amount: u32,
    },
    /// 行会增益（`C.GuildBuffUpdate`）：`0` 请求列表、`1` 购买、`2` 激活。
    GuildBuffUpdate {
        /// 动作。
        action: u8,
        /// 增益编号（请求列表时给 `0`）。
        id: i32,
    },
    /// 行会成员管理（`C.EditGuildMember`）。
    ///
    /// `change_type`：`0` 邀请、`1` 踢人/退会、`2` 升/降职、`3` 改职位名、
    /// `4` 新建职位、`5` 改职位权限（`crates/mir-server/src/guild.rs` 的模块文档）。
    EditGuildMember {
        /// 变动类型。
        change_type: u8,
        /// 职位序号（`2`/`3`/`5` 用）。
        rank_index: u8,
        /// 目标角色名（`0`/`1`/`2` 用）。
        name: String,
        /// 职位名（`3`/`4` 用）。
        rank_name: String,
    },
    /// 行会公告（`C.EditGuildNotice`）。
    EditGuildNotice {
        /// 公告行。
        notice: Vec<String>,
    },
    /// 拉取行会公告/成员（`C.RequestGuildInfo`）：`0` 公告、`1` 成员表。
    RequestGuildInfo {
        /// 信息类型。
        type_: u8,
    },
    /// 发起交易（`C.TradeRequest`）。
    ///
    /// 协议**不带对方名字**：服务端按自己的朝向取相邻格的玩家（Crystal
    /// `PlayerObject.TradeRequest`，`PlayerObject.cs:10623-10706`），所以客户端只发空包。
    TradeRequest,
    /// 回应交易邀请（`C.TradeReply`）。
    TradeReply {
        /// `true` = 接受。
        accept: bool,
    },
    /// 把背包物品放进自己的交易格（`C.DepositTradeItem`）。
    ///
    /// `from` 是背包格、`to` 是交易格（Crystal `MirItemCell.cs:1552-1559`）。
    TradeItem {
        /// 背包格。
        from: i32,
        /// 交易格（0..[`TRADE_SLOTS`]）。
        to: i32,
    },
    /// 把交易格里的物品拿回背包（`C.RetrieveTradeItem`）。
    ///
    /// `from` 是交易格、`to` 是背包格（Crystal `MirItemCell.cs:1140-1148`）。
    TradeTakeBack {
        /// 交易格（0..[`TRADE_SLOTS`]）。
        from: i32,
        /// 背包格。
        to: i32,
    },
    /// 放入金币（`C.TradeGold`）。
    ///
    /// `amount` 是**增量**：服务端做 `TradeGoldAmount += amount`（Crystal
    /// `PlayerObject.cs:10755`），客户端记的是累计值 [`TradeView::gold`]。
    TradeGold {
        /// 本次放入的金币数（> 0）。
        amount: u32,
    },
    /// 准备 / 确认（`C.TradeConfirm`）。
    ///
    /// `locked = true` 锁定己方报价，服务端在对方也锁定后互换物品与金币
    /// （Crystal `PlayerObject.TradeConfirm`，`PlayerObject.cs:10789-10888`）；
    /// `false` 解锁（Crystal 的 `ChangeLockState(!TradeLocked)` 第二段点击）。
    TradeConfirm {
        /// 是否锁定己方报价。
        locked: bool,
    },
    /// 取消交易（`C.TradeCancel`）。
    TradeCancel,
    /// 邀请入队（`C.AddMember`；服务端 `World::group_invite`）。
    AddMember {
        /// 被邀请者名字。
        name: String,
    },
    /// 退队 / 踢人（`C.DelMember`；发自己的名字等于退队，Crystal `GroupDialog.cs:216-224`）。
    DelMember {
        /// 目标名字（自己在队里时发自己的名字）。
        name: String,
    },
    /// 回应组队邀请（`C.GroupInvite`）。
    GroupReply {
        /// `true` = 接受。
        accept: bool,
    },
    /// 开关「允许组队」（`C.SwitchGroup`；Crystal `GroupDialog.cs:80`）。
    SwitchGroup {
        /// 是否允许别人邀请自己。
        allow_group: bool,
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
    /// 小地图图像号（`S.MapInformation.MiniMap` / `S.MapChanged.MiniMap`，
    /// `Shared/ServerPackets.cs:479,2989`；值是 `MapInfo.MiniMap`，`MirDatabase/MapInfo.cs:20`）。
    ///
    /// 它是 `resource/data/mmap.wzl` 的帧号（Crystal `MLibrary.cs:23` 把 `MMap`
    /// 打开成 `Libraries.MiniMap`）；`0` 表示这张图没有小地图
    /// （Crystal `MainDialogs.cs:1898` 会切到无图模式）。
    pub mini_map: u16,
    /// 大地图图像号（`S.MapInformation.BigMap` / `ClientMapInfo.BigMap`，
    /// `Shared/Data/ClientData.cs:324`；`BigMapDialog.cs:642` 用它取 `mmap.wzl` 的帧）。
    pub big_map: u16,
    /// 背景音乐编号（`S.MapInformation.Music` / `S.MapChanged.Music`，
    /// `Shared/ServerPackets.cs`；Crystal 在 `GameScene.cs:2146`、`:3985` 把它写进
    /// `MapControl.Music`，`GameScene.cs:10454-10466` 用 `SoundManager.PlayMusic` 播放）。
    ///
    /// `0` 表示服务器没有配置音乐（Crystal 静音）；示例数据库的所有地图都是 0。
    /// 客户端会用 [`mir_render::music_for_map`] 兜底，见 `mir-client::audio`。
    pub music: u16,
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
    /// 坐骑外观号：`0` = 没有坐骑，`1..` = `S.ObjectPlayer.mount_type + 1`
    /// （Crystal `PlayerObject.MountType`/`RidingMount`，`PlayerObject.cs:95-99,202-219`）。
    ///
    /// `S.UserInformation` 不携带坐骑字段，自己的坐骑由 `S.MountUpdate` 更新；
    /// 其他人由 `S.ObjectPlayer` 的 `mount_type`/`riding_mount` 直接给。
    pub mount: u16,
    /// 翅膀外观号：`0` = 没有翅膀，`1..99` = `S.ObjectPlayer.wing_effect`
    /// （Crystal `PlayerObject.WingEffect`，`PlayerObject.cs:78,141,176`）。
    ///
    /// `>= 100` 是特效而不是翅膀层（`PlayerObject.cs:577,650,5077`），渲染层会忽略。
    pub wings: u16,
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
            mount: 0,
            wings: 0,
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
            mount: 0,
            wings: 0,
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

/// `S.ObjectPlayer.MountType` + `RidingMount` → 渲染层坐骑外观号。
///
/// Crystal 的 `DrawMount` 要求 `MountType >= 0 && RidingMount`（`PlayerObject.cs:5086`），
/// 否则不画坐骑。`EntityView.mount` 用 `0` 表示「没有」，所以这里把 `MountType`
/// 加一保存（`MountType = 0` → 外观 1）。
#[must_use]
fn mount_layer(mount_type: i16, riding_mount: bool) -> u16 {
    if !riding_mount || mount_type < 0 {
        return 0;
    }
    u16::try_from(mount_type)
        .ok()
        .and_then(|value| value.checked_add(1))
        .unwrap_or(0)
}

/// `S.ObjectPlayer.WingEffect` → 渲染层翅膀外观号（`0` = 没有）。
///
/// Crystal 只在 `WingEffect in 1..99` 时给 `WingLibrary` 赋值（`PlayerObject.cs:577,526`），
/// `>= 100` 是其他特效（`:650`）；渲染层会再次过滤，这里原样保存以便 UI 诊断。
#[must_use]
fn wing_layer(wing_effect: u8) -> u16 {
    u16::from(wing_effect)
}

/// 自己的角色状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfPlayer {
    /// 对象 ID。
    pub object_id: u32,
    /// 角色编号（`S.UserInformation.real_id`，数据库主键）。
    ///
    /// 行会包的 `S.GuildStorageItemChange.user` 用的是**角色编号**而不是对象 ID
    /// （`crates/mir-server/src/guild_storage.rs` 传 `character_id`），客户端靠这个字段
    /// 判断「这次仓库变动是不是我做的」。
    pub real_id: u32,
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
    /// 等级（`S.ObjectPlayer.Level`；怪物/NPC 为 0）。组队面板的「等级」列用它。
    pub level: u16,
    /// 职业（`S.ObjectPlayer.Class`；不是玩家时为 [`MirClass::Warrior`] 占位，
    /// 组队面板按 [`Self::kind`] 决定是否显示）。
    pub class: MirClass,
    /// 服务端给的名字颜色（`S.ObjectPlayer.NameColour` / `S.ObjectColourChanged`）。
    ///
    /// Crystal 用它区分 PK 红/黄/棕/白名（服务端 `PlayerObject.GetNameColour`，
    /// `Server/MirObjects/PlayerObject.cs:1849-1890`）。`None` 表示没收到过。
    pub name_colour: Option<mir_core::Color>,
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
            level: 0,
            class: MirClass::Warrior,
            name_colour: None,
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

/// 一次音效事件：服务端包 → 「该响什么」。
///
/// 事件只保存**语义**（[`SoundId`]），文件名映射与播放都在客户端：
/// 映射在 `mir_render::sound`（纯数据），播放由 [`crate::audio::SoundPlayer`] 负责。
///
/// 与伤害/光效事件一样带递增 `id`：快照每帧整体克隆，消费方只处理比上次大的 ID。
///
/// 服务端**不会**把 `S.ObjectWalk` / `S.ObjectRun` / `S.ObjectAttack` 回发给动作发起者
/// （`mir-server/src/world.rs:2428-2449` 的 `broadcast` 明确跳过本人，Crystal 也一样），
/// 所以自己的走/跑/攻击音效由客户端在发包时本地预播
/// （[`WorldSnapshot::push_local_sound`]），这里的事件只覆盖「别人身上的动作」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoundEvent {
    /// 递增编号。
    pub id: u64,
    /// 该响的音效。
    pub sound: SoundId,
    /// 施法音用哪个技能（`S.ObjectAttack.spell`）；其它事件为 [`Spell::None`]。
    pub spell: Spell,
    /// 是否用女声那两档音效（`SoundList.cs:131-134` 的 `MaleFlinch/FemaleFlinch`、
    /// `MaleDie/FemaleDie`）。只有「自己」知道性别，别人一律按男声（简化）。
    pub female: bool,
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

/// 任务定义（`S.NewQuestInfo` 的界面子集）。
///
/// Crystal 在登录时把**全部**任务定义下发一遍（`PlayerObject.GetQuestInfo`，
/// `Server/MirObjects/PlayerObject.cs:1185`、`:1727-1731`），客户端按 `Index` 缓存，
/// 进度包 `S.ChangeQuest` 只带编号。界面用的名字/描述/限时/奖励都来自这里。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuestInfoView {
    /// 任务编号（`ClientQuestInfo.index`）。
    pub index: i32,
    /// 任务名。
    pub name: String,
    /// 任务分组（Crystal 任务日记按分组折叠）。
    pub group: String,
    /// 发布任务的 NPC 模板号。
    pub npc_index: u32,
    /// 交任务的 NPC 模板号。
    pub finish_npc_index: u32,
    /// 任务简介行。
    pub description: Vec<String>,
    /// 目标描述行。
    pub task_description: Vec<String>,
    /// 交任务时的描述行。
    pub return_description: Vec<String>,
    /// 完成后的描述行。
    pub completion_description: Vec<String>,
    /// 任务类型（决定 NPC 头顶感叹号的颜色，`QuestIcon`）。
    pub quest_type: QuestType,
    /// 限时秒数（`0` = 不限时）。
    pub time_limit_seconds: i32,
    /// 奖励金币。
    pub reward_gold: u32,
    /// 奖励经验。
    pub reward_exp: u32,
    /// 奖励元宝。
    pub reward_credit: u32,
    /// 接取等级下限。
    pub min_level: i32,
    /// 接取等级上限。
    pub max_level: i32,
}

impl QuestInfoView {
    /// 从协议定义里抽取界面字段。
    #[must_use]
    pub fn from_client_quest(info: &mir_protocol::data::ClientQuestInfo) -> Self {
        Self {
            index: info.index,
            name: info.name.clone(),
            group: info.group.clone(),
            npc_index: info.npc_index,
            finish_npc_index: info.finish_npc_index,
            description: info.description.clone(),
            task_description: info.task_description.clone(),
            return_description: info.return_description.clone(),
            completion_description: info.completion_description.clone(),
            quest_type: info.quest_type,
            time_limit_seconds: info.time_limit_in_seconds,
            reward_gold: info.reward_gold,
            reward_exp: info.reward_exp,
            reward_credit: info.reward_credit,
            min_level: info.min_level_needed,
            max_level: info.max_level_needed,
        }
    }
}

/// 一条进行中/已完成的任务进度（`S.ChangeQuest` + 定义）。
///
/// `task_list` 是服务端算好的进度文本（`crates/mir-server/src/quests.rs` 的 `task_list`，
/// 对应 Crystal `QuestProgressInfo.UpdateTasks`），例如「击杀 鹿 3/10」，客户端原样显示。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuestView {
    /// 任务编号。
    pub id: i32,
    /// 任务名（定义缺失时是 `任务 #n`）。
    pub name: String,
    /// 任务分组。
    pub group: String,
    /// 进度文本行。
    pub task_list: Vec<String>,
    /// 是否已接取（Crystal `ClientQuestProgress.Taken`）。
    pub taken: bool,
    /// **目标是否已达成**（Crystal `ClientQuestProgress.Completed`，不是「已交付」）。
    ///
    /// 为真时面板高亮成绿色并提示「去交付」。
    pub completed: bool,
    /// 是否是新任务（Crystal `New`，一天内接的）。
    pub new: bool,
    /// 限时秒数（来自定义，`0` = 不限时）。
    pub time_limit_seconds: i32,
    /// 本地收到 `S.ChangeQuest{Add}` 的时刻（Unix 毫秒）。
    ///
    /// 简化：协议里的 `ClientQuestProgress` **不带接取时间**（Crystal 的
    /// `StartDateTime` 只在服务端 `QuestProgressInfo` 里），所以剩余时间只能从
    /// 「客户端第一次看到这条任务」开始算；重登后会被重置成完整时长。
    pub accepted_at_ms: Option<u64>,
}

impl QuestView {
    /// 限时剩余秒数；不限时返回 `None`。
    ///
    /// `now_ms` 是客户端本地时钟（与 `accepted_at_ms` 同一时间轴）。
    #[must_use]
    pub fn remaining_seconds(&self, now_ms: u64) -> Option<i64> {
        if self.time_limit_seconds <= 0 {
            return None;
        }
        let elapsed_ms = self
            .accepted_at_ms
            .map_or(0, |accepted| now_ms.saturating_sub(accepted));
        let elapsed = i64::try_from(elapsed_ms / 1000).unwrap_or(i64::MAX);
        Some((i64::from(self.time_limit_seconds) - elapsed).max(0))
    }

    /// 是否应该高亮成「目标已达成、去交付」。
    #[must_use]
    pub const fn ready_to_return(&self) -> bool {
        self.completed && self.taken
    }
}

/// 一封邮件（`S.ReceiveMail` 里的 `ClientMail` 界面子集）。
///
/// 协议里**没有主题字段**（Crystal `Shared/Data/ClientData.cs:154` 的 `ClientMail`
/// 只有发件人/正文/标志/时间/金币/附件）；服务端 `mail` 表的 `subject` 列也不上线。
/// 所以列表的「主题」列显示正文首行，见 `ui::mail`。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MailView {
    /// 邮件编号（`C.ReadMail` / `C.CollectParcel` / `C.DeleteMail` / `C.LockMail` 用它）。
    pub mail_id: u64,
    /// 发件人角色名。
    pub sender_name: String,
    /// 正文。
    pub message: String,
    /// 是否已读。
    pub opened: bool,
    /// 是否锁定（锁定后不能删除）。
    pub locked: bool,
    /// 是否可回复。
    pub can_reply: bool,
    /// 附件是否已领取。
    pub collected: bool,
    /// 发送时间（`DateTime.ToBinary()`，见 `mir_core::DateTimeBinary`）。
    pub date_sent: mir_core::DateTimeBinary,
    /// 附带金币。
    pub gold: u32,
    /// 附带物品。
    pub items: Vec<InventoryItem>,
}

impl MailView {
    /// 从协议邮件里抽取界面字段。
    #[must_use]
    pub fn from_client_mail(mail: &ClientMail) -> Self {
        Self {
            mail_id: mail.mail_id,
            sender_name: mail.sender_name.clone(),
            message: mail.message.clone(),
            opened: mail.opened,
            locked: mail.locked,
            can_reply: mail.can_reply,
            collected: mail.collected,
            date_sent: mail.date_sent,
            gold: mail.gold,
            items: mail
                .items
                .iter()
                .map(InventoryItem::from_user_item)
                .collect(),
        }
    }

    /// 是否带附件（金币或物品）。
    #[must_use]
    pub fn is_parcel(&self) -> bool {
        self.gold > 0 || !self.items.is_empty()
    }

    /// 列表里的「主题」：正文第一行（协议没有主题字段，见类型文档）。
    #[must_use]
    pub fn subject_line(&self) -> &str {
        self.message
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("")
    }

    /// 是否可以领取附件（带附件、还没领过）。
    #[must_use]
    pub fn can_collect(&self) -> bool {
        !self.collected && (self.gold > 0 || !self.items.is_empty())
    }
}

/// 行会成员（`S.GuildMemberChange.Ranks[].Members[]`）。
///
/// 简化：协议里的 `GuildMember` **不带所在地图**（`Shared/Data/GuildData.cs:62-90` 只有
/// 名字/编号/最后登录/是否投票/是否在线），所以成员表的「地图」列恒为空。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GuildMemberView {
    /// 角色名。
    pub name: String,
    /// 角色编号（`GuildMember.id`）。
    pub id: i32,
    /// 最后登录时间。
    pub last_login: mir_core::DateTimeBinary,
    /// 是否在线。
    pub online: bool,
}

/// 行会职位（`S.GuildMemberChange.Ranks[]`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GuildRankView {
    /// 职位序号（`0` 是会长）。
    pub index: i32,
    /// 职位名。
    pub name: String,
    /// 权限位掩码（`GuildRankOptions`，逐位判断用
    /// [`mir_core::enums::GuildRankOptions::contains`]）。
    pub options: u8,
    /// 该职位下的成员。
    pub members: Vec<GuildMemberView>,
}

impl GuildRankView {
    /// 从协议职位里抽取界面字段。
    #[must_use]
    pub fn from_guild_rank(rank: &GuildRank) -> Self {
        Self {
            index: rank.index,
            name: rank.name.clone(),
            options: rank.options,
            members: rank
                .members
                .iter()
                .map(|member: &GuildMember| GuildMemberView {
                    name: member.name.clone(),
                    id: member.id,
                    last_login: member.last_login,
                    online: member.online,
                })
                .collect(),
        }
    }
}

/// 行会概况（`S.GuildStatus`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GuildView {
    /// 行会名（空串 = 没有行会）。
    pub name: String,
    /// 我的职位名。
    pub rank_name: String,
    /// 行会等级。
    pub level: u8,
    /// 当前经验。
    pub experience: i64,
    /// 升级所需经验。
    pub max_experience: i64,
    /// 行会资金（= 行会仓库金币，Crystal `GuildInfo.Gold`）。
    pub gold: u32,
    /// 可分配的升级点。
    pub spare_points: u8,
    /// 成员数。
    pub member_count: i32,
    /// 成员上限（`0` 表示不限）。
    pub max_members: i32,
    /// 是否在投票（本作服务端恒为 false）。
    pub voting: bool,
    /// 仓库已用格数。
    pub item_count: u8,
    /// 增益个数（Crystal 恒写 0）。
    pub buff_count: u8,
    /// 我的职位权限。
    pub my_options: mir_core::enums::GuildRankOptions,
    /// 我的职位序号（`256` 表示不是成员）。
    pub my_rank_id: i32,
}

impl GuildView {
    /// 我是否拥有某项职位权限。
    #[must_use]
    pub const fn allows(&self, option: mir_core::enums::GuildRankOptions) -> bool {
        self.my_options.contains(option)
    }

    /// 是否可以管理职位（升/降职）。
    #[must_use]
    pub const fn can_change_rank(&self) -> bool {
        self.allows(mir_core::enums::GuildRankOptions::CanChangeRank)
    }

    /// 是否可以踢人。
    #[must_use]
    pub const fn can_kick(&self) -> bool {
        self.allows(mir_core::enums::GuildRankOptions::CanKick)
    }

    /// 是否可以改公告。
    #[must_use]
    pub const fn can_change_notice(&self) -> bool {
        self.allows(mir_core::enums::GuildRankOptions::CanChangeNotice)
    }

    /// 是否可以激活增益。
    #[must_use]
    pub const fn can_activate_buff(&self) -> bool {
        self.allows(mir_core::enums::GuildRankOptions::CanActivateBuff)
    }
}

/// 行会增益定义（`S.GuildBuffList.GuildBuffs[]`，Crystal `GuildBuffInfo`）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GuildBuffInfoView {
    /// 增益编号。
    pub id: i32,
    /// 图标编号。
    pub icon: i32,
    /// 增益名。
    pub name: String,
    /// 解锁所需行会等级。
    pub level_requirement: u8,
    /// 解锁所需行会点数。
    pub points_requirement: u8,
    /// 生效时长（**分钟**，Crystal `GuildBuffInfo.TimeLimit`；`0` = 永久）。
    pub time_limit_minutes: i32,
    /// 激活消耗（行会资金）。
    pub activation_cost: i32,
    /// 提供的属性。
    pub stats: BTreeMap<Stat, i32>,
}

impl GuildBuffInfoView {
    /// 从协议定义里抽取界面字段。
    #[must_use]
    pub fn from_guild_buff_info(info: &GuildBuffInfo) -> Self {
        Self {
            id: info.id,
            icon: info.icon,
            name: info.name.clone(),
            level_requirement: info.level_requirement,
            points_requirement: info.points_requirement,
            time_limit_minutes: info.time_limit,
            activation_cost: info.activation_cost,
            stats: info.stats.values().clone(),
        }
    }
}

/// 行会已获得的增益（`S.GuildBuffList.ActiveBuffs[]`，Crystal `GuildBuff`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GuildBuffView {
    /// 增益编号（指向 [`GuildBuffInfoView`]）。
    pub id: i32,
    /// 是否正在生效。
    pub active: bool,
    /// 剩余生效时间（**分钟**，Crystal `GuildBuff.ActiveTimeRemaining`）。
    pub active_time_remaining_minutes: i32,
}

impl GuildBuffView {
    /// 从协议增益里抽取界面字段。
    #[must_use]
    pub const fn from_guild_buff(buff: &GuildBuff) -> Self {
        Self {
            id: buff.id,
            active: buff.active,
            active_time_remaining_minutes: buff.active_time_remaining,
        }
    }
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

/// 生命百分比（0..=100）：`HP / MaxHP × 100`，四舍五入到整数。
///
/// 逐行对应 Crystal `Client/MirObjects/UserObject.cs:176`：
/// `PercentHealth = (byte)(HP / (float)Stats[Stat.HP] * 100);`。
/// 最大生命未知（<= 0）时返回 0；HP 超过上限时夹到 100。
#[must_use]
fn health_percent(current: i32, maximum: i32) -> u8 {
    if maximum <= 0 {
        return 0;
    }
    let percent = i64::from(current.clamp(0, maximum)) * 100 / i64::from(maximum);
    u8::try_from(percent.clamp(0, 100)).unwrap_or(100)
}

/// 交易面板状态（Crystal `TradeDialog` + `GuestTradeDialog`，`Client/MirScenes/Dialogs/TradeDialogs.cs`）。
///
/// # 包 ↔ 状态
///
/// | 包 | 状态变化 |
/// |---|---|
/// | `S.TradeRequest{Name}` | [`Self::invitation`] = 邀请者，[`Self::invitation_version`] +1（UI 弹「XXX 请求与你交易」） |
/// | `S.TradeAccept{Name}` | [`Self::open`] = true、[`Self::partner`] = 对方名字、[`Self::open_version`] +1 |
/// | `S.DepositTradeItem{From,To,Success}` | 背包格 → [`Self::items`]，[`Self::version`] +1 |
/// | `S.RetrieveTradeItem{From,To,Success}` | [`Self::items`] → 背包格，[`Self::version`] +1 |
/// | `S.TradeItem{TradeItems[10]}` | 对方 10 格 → [`Self::partner_items`]，己方准备状态与对方准备状态一起清零 |
/// | `S.TradeGold{Amount}` | [`Self::partner_gold`] = Amount（**累计值**），同上清零 |
/// | `S.TradeConfirm{}` | 成交：`open` = false（Crystal `GameScene.cs:6329-6332` → `TradeReset`） |
/// | `S.TradeCancel{Unlock}` | `Unlock` = 解锁继续谈（`open` 不变），否则关窗（`GameScene.cs:6333-6346`） |
///
/// # 「对方已准备」
///
/// 协议**没有**「对方已准备」的包：Crystal 客户端只显示自己的锁定状态
/// （`TradeDialogs.cs:36-40` 的 `ConfirmButton` + `:122-140` 的 `ChangeLockState`）。
/// 本作按需求加了 [`Self::partner_ready`]，唯一的服务端信号是
/// **双方都锁定时才会发 `S.TradeCancel{Unlock = true}`**（`PlayerObject.cs:10817-10852`），
/// 所以收到 `unlock = true` 时把它置真；对方改报价（`S.TradeItem`/`S.TradeGold`）或
/// 交易重开时清回假。注入式测试可以直接写这个字段验证两种文案。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TradeView {
    /// 对方名字（`S.TradeAccept.Name`）。
    pub partner: String,
    /// 交易窗是否打开。
    pub open: bool,
    /// [`Self::open`] 每被打开一次 +1（UI 据此自动弹面板并重置本地准备状态）。
    pub open_version: u64,
    /// 自己的 10 格（`S.DepositTradeItem` / `S.RetrieveTradeItem` 的回执驱动）。
    pub items: Vec<Option<InventoryItem>>,
    /// 对方的 10 格（`S.TradeItem.TradeItems`）。
    pub partner_items: Vec<Option<InventoryItem>>,
    /// 自己放上的金币（累计；`C.TradeGold` 的增量在发包时加上，服务端不回声）。
    pub gold: u32,
    /// 对方的金币（`S.TradeGold.Amount` 是累计值）。
    pub partner_gold: u32,
    /// 自己已准备（`C.TradeConfirm{locked: true}`）。
    ///
    /// Crystal 把它放在客户端（`UserObject.TradeLocked`，`UserObject.cs:24` 附近），
    /// 所以 [`crate::ui::UiState`] 也留了一份即时状态；这里保存服务端包能改动的镜像。
    pub locked: bool,
    /// 对方已准备（见类型文档：协议只能间接推断）。
    pub partner_ready: bool,
    /// 交易内容每变一次 +1（对方改物品/金币、自己存取物品）。
    pub version: u64,
    /// 最近的交易邀请（`S.TradeRequest.Name`）；`None` 表示没有待处理邀请。
    pub invitation: Option<String>,
    /// 收到过多少次交易邀请（UI 据此弹一次窗）。
    pub invitation_version: u64,
}

impl TradeView {
    /// 新建 10 格空交易窗。
    #[must_use]
    pub fn new() -> Self {
        Self {
            items: vec![None; TRADE_SLOTS],
            partner_items: vec![None; TRADE_SLOTS],
            ..Self::default()
        }
    }

    /// 把两个格子列表补齐到 [`TRADE_SLOTS`]。
    fn normalize(&mut self) {
        self.items.resize(TRADE_SLOTS, None);
        self.partner_items.resize(TRADE_SLOTS, None);
    }
}

/// 一名队友（协议里只有名字/地图/坐标）。
///
/// 等级/职业/血量来自实体表（同图近距离时服务端会发 `S.ObjectPlayer` 与
/// `S.ObjectHealth`，`crates/mir-server/src/group.rs` 的 `group_join`），由
/// [`WorldSnapshot::group_member_infos`] 按名字关联。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupMember {
    /// 名字（`S.AddMember.Name`，顺序即入队顺序）。
    pub name: String,
    /// 所在地图标题（`S.GroupMembersMap.PlayerMap`）。
    pub map: String,
    /// 最近一次 `S.SendMemberLocation` 的坐标。
    pub location: Option<Point>,
}

/// 组队状态（Crystal `GroupDialog.GroupList` + `GroupMembersMap`，
/// `Client/MirScenes/Dialogs/GroupDialog.cs:12-14`）。
///
/// # 包 ↔ 状态
///
/// | 包 | 状态变化 |
/// |---|---|
/// | `S.SwitchGroup{AllowGroup}` | [`Self::allow_group`]；变假时清空队伍（`GameScene.cs:4970-4976`） |
/// | `S.AddMember{Name}` | 追加成员 + 组队频道提示（`GameScene.cs:5006-5010`） |
/// | `S.DeleteMember{Name}` | 移除成员 + 组队频道提示（`:4986-4992`） |
/// | `S.DeleteGroup{}` | 清空队伍（`:4978-4984`） |
/// | `S.GroupInvite{Name}` | [`Self::invitation`]（UI 弹「XXX 邀请你组队」，`:4994-5005`） |
/// | `S.GroupMembersMap{PlayerName,PlayerMap}` | 成员地图（`:5011-5020`） |
/// | `S.SendMemberLocation{MemberName,MemberLocation}` | 成员坐标（`:5021-5029`，Crystal 用来画大地图队友点） |
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupView {
    /// 成员（`members[0]` 是队长；服务端顺序照抄 Crystal `GroupMembers`）。
    pub members: Vec<GroupMember>,
    /// 是否允许组队（默认 `true`；服务端 `S.SwitchGroup` 同步）。
    pub allow_group: bool,
    /// 最近的组队邀请（`S.GroupInvite.Name`）。
    pub invitation: Option<String>,
    /// 收到过多少次组队邀请（UI 据此弹一次窗）。
    pub invitation_version: u64,
}

impl GroupView {
    /// 新建「默认允许组队、没有队伍」的状态。
    #[must_use]
    pub fn new() -> Self {
        Self {
            allow_group: true,
            ..Self::default()
        }
    }

    /// 是否在队伍里。
    #[must_use]
    pub fn has_members(&self) -> bool {
        !self.members.is_empty()
    }

    /// 队长名字（`members[0]`）。
    #[must_use]
    pub fn leader(&self) -> Option<&str> {
        self.members.first().map(|member| member.name.as_str())
    }

    /// 按名字找成员（大小写不敏感，与服务端 `string.Compare(..., OrdinalIgnoreCase)` 一致）。
    #[must_use]
    pub fn member(&self, name: &str) -> Option<&GroupMember> {
        self.members
            .iter()
            .find(|member| member.name.eq_ignore_ascii_case(name))
    }

    /// 名字是否是队友。
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.member(name).is_some()
    }

    /// 追加成员（已在队里时忽略，Crystal `GroupList.Add` 不会去重，这里按名字去重更安全）。
    fn add(&mut self, name: &str) {
        if name.is_empty() || self.contains(name) {
            return;
        }
        self.members.push(GroupMember {
            name: name.to_owned(),
            ..GroupMember::default()
        });
    }

    /// 移除成员并清掉地图/坐标缓存。
    fn remove(&mut self, name: &str) {
        self.members
            .retain(|member| !member.name.eq_ignore_ascii_case(name));
    }
}

/// 组队成员的展示信息（组队面板一行 / HUD 队友血条）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupMemberInfo {
    /// 名字。
    pub name: String,
    /// 等级（同图可见时来自 `S.ObjectPlayer.Level`，否则 0 = 未知）。
    pub level: u16,
    /// 职业（同图可见时来自 `S.ObjectPlayer.Class`）。
    pub class: Option<MirClass>,
    /// 所在地图标题（`S.GroupMembersMap`）。
    pub map: String,
    /// 是否在线：队友对象当前在自己的视野里（有 `S.ObjectPlayer` 且没被 `S.ObjectRemove`）。
    pub online: bool,
    /// 血量百分比（`S.ObjectHealth`；自己由 `HP / MaxHP` 算）。
    pub hp_percent: Option<u8>,
    /// 是否队长（`members[0]`）。
    pub is_leader: bool,
    /// 是否自己。
    pub is_self: bool,
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
    /// 最近的音效事件（尾部最新；播放器只消费新增的 `id`）。
    pub sound_events: Vec<SoundEvent>,
    /// 下一个音效事件编号。
    ///
    /// 从 1 开始：`0` 表示「还没消费过任何事件」，见 [`Self::sound_events_since`]。
    pub next_sound_id: u64,
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
    /// 任务定义（`S.NewQuestInfo`，按任务编号索引）。
    pub quest_infos: BTreeMap<i32, QuestInfoView>,
    /// 进行中任务（`S.ChangeQuest`，按接取顺序；`S.ChangeQuest{Remove}` 时移除）。
    pub quests: Vec<QuestView>,
    /// 已完成任务编号（`S.CompleteQuest`）。
    pub completed_quests: Vec<i32>,
    /// 仓库（`S.UserStorage`；长度 80，`None` 为空格）。没收到过时为空。
    pub storage: Vec<Option<InventoryItem>>,
    /// 仓库面板是否该打开（`S.NPCStorage`；离开 NPC 对话/切图时关闭）。
    pub storage_open: bool,
    /// 收到过多少次 `S.NPCStorage`（UI 用它判断「这是新的一次打开」）。
    pub storage_open_version: u64,
    /// 邮件列表（`S.ReceiveMail`，服务端按发送时间升序）。
    pub mails: Vec<MailView>,
    /// 服务端要求打开写信页（`S.MailSendRequest`）。
    pub mail_send_request: bool,
    /// 收到过多少次 `S.MailSendRequest`（版本号）。
    pub mail_send_request_version: u64,
    /// 最近一次邮费询价结果（`S.MailCost`）。
    pub mail_cost: Option<u32>,
    /// 最近一次发送结果（`S.MailSent.result`；`0` 成功）。
    pub mail_sent_result: Option<i8>,
    /// 最近一次领取回执（`S.ParcelCollected.result`）。
    pub parcel_collected_result: Option<i8>,
    /// 附件格锁定状态（`S.MailLockedItem`：物品唯一编号 → 是否锁定）。
    pub mail_locked_items: BTreeMap<u64, bool>,
    /// 行会概况（`S.GuildStatus`；`None` 表示没收到过）。
    pub guild: Option<GuildView>,
    /// 行会职位与成员（`S.GuildMemberChange`，按职位序号排序）。
    pub guild_ranks: Vec<GuildRankView>,
    /// 行会公告（`S.GuildNoticeChange`）。
    pub guild_notice: Vec<String>,
    /// 行会仓库（`S.GuildStorageList`；长度 112，`None` 为空格）。
    pub guild_storage: Vec<Option<InventoryItem>>,
    /// 全部可选行会增益（`S.GuildBuffList.GuildBuffs`）。
    pub guild_buff_infos: Vec<GuildBuffInfoView>,
    /// 已获得/激活的行会增益（`S.GuildBuffList.ActiveBuffs`）。
    pub guild_active_buffs: Vec<GuildBuffView>,
    /// 交易状态（`S.TradeRequest`/`S.TradeAccept`/`S.TradeItem`/`S.TradeGold`/
    /// `S.TradeConfirm`/`S.TradeCancel`，见 [`TradeView`]）。
    pub trade: TradeView,
    /// 组队状态（`S.AddMember`/`S.DeleteMember`/`S.DeleteGroup`/`S.SwitchGroup`/
    /// `S.GroupInvite`/`S.GroupMembersMap`/`S.SendMemberLocation`，见 [`GroupView`]）。
    pub group: GroupView,
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
            sound_events: Vec::new(),
            next_sound_id: 1,
            item_infos: BTreeMap::new(),
            magics: Vec::new(),
            npc_dialog: None,
            npc_goods: None,
            npc_panels: NpcPanels::default(),
            quest_infos: BTreeMap::new(),
            quests: Vec::new(),
            completed_quests: Vec::new(),
            storage: Vec::new(),
            storage_open: false,
            storage_open_version: 0,
            mails: Vec::new(),
            mail_send_request: false,
            mail_send_request_version: 0,
            mail_cost: None,
            mail_sent_result: None,
            parcel_collected_result: None,
            mail_locked_items: BTreeMap::new(),
            guild: None,
            guild_ranks: Vec::new(),
            guild_notice: Vec::new(),
            guild_storage: Vec::new(),
            guild_buff_infos: Vec::new(),
            guild_active_buffs: Vec::new(),
            trade: TradeView::new(),
            group: GroupView::new(),
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

    /// 取仓库格；越界或空格返回 `None`。
    #[must_use]
    pub fn storage_item(&self, slot: usize) -> Option<&InventoryItem> {
        self.storage.get(slot).and_then(Option::as_ref)
    }

    /// 取行会仓库格；越界或空格返回 `None`。
    #[must_use]
    pub fn guild_storage_item(&self, slot: usize) -> Option<&InventoryItem> {
        self.guild_storage.get(slot).and_then(Option::as_ref)
    }

    /// 按编号找进行中任务。
    #[must_use]
    pub fn quest(&self, quest_id: i32) -> Option<&QuestView> {
        self.quests.iter().find(|quest| quest.id == quest_id)
    }

    /// 按编号找邮件。
    #[must_use]
    pub fn mail(&self, mail_id: u64) -> Option<&MailView> {
        self.mails.iter().find(|mail| mail.mail_id == mail_id)
    }

    /// 按编号找行会增益定义。
    #[must_use]
    pub fn guild_buff_info(&self, buff_id: i32) -> Option<&GuildBuffInfoView> {
        self.guild_buff_infos.iter().find(|info| info.id == buff_id)
    }

    /// 按编号找行会已获得增益。
    #[must_use]
    pub fn guild_active_buff(&self, buff_id: i32) -> Option<&GuildBuffView> {
        self.guild_active_buffs
            .iter()
            .find(|buff| buff.id == buff_id)
    }

    /// 是否在队伍里。
    #[must_use]
    pub fn has_group(&self) -> bool {
        self.group.has_members()
    }

    /// 名字是否是队友（大小写不敏感）。
    #[must_use]
    pub fn is_group_member(&self, name: &str) -> bool {
        self.group.contains(name)
    }

    /// 自己是不是队长（`members[0]` 必须是自己）。
    #[must_use]
    pub fn is_group_leader(&self) -> bool {
        self.self_player.as_ref().is_some_and(|me| {
            self.group
                .leader()
                .is_some_and(|leader| leader.eq_ignore_ascii_case(&me.name))
        })
    }

    /// 组队成员的展示信息（名字/等级/职业/地图/在线/血量百分比）。
    ///
    /// 等级/职业/血量不在协议里：`S.AddMember`/`S.GroupMembersMap` 只带名字与地图，
    /// `S.SendMemberLocation` 只带坐标。同图近距离时服务端会为队友补发 `S.ObjectPlayer`
    /// 与 `S.ObjectHealth`（`crates/mir-server/src/group.rs` 的 `group_join`，Crystal
    /// `PlayerObject.cs:9454-9464`），所以这里按**名字**去实体表 + 自己的快照里关联；
    /// 视野外的队友只显示名字/地图（在线 = 假）。
    #[must_use]
    pub fn group_member_infos(&self) -> Vec<GroupMemberInfo> {
        let leader = self.group.leader().map(str::to_owned);
        let me = self.self_player.as_ref();
        self.group
            .members
            .iter()
            .map(|member| {
                let is_self = me.is_some_and(|me| me.name.eq_ignore_ascii_case(&member.name));
                let is_leader = leader
                    .as_deref()
                    .is_some_and(|leader| leader.eq_ignore_ascii_case(&member.name));
                if is_self {
                    let Some(me) = me else {
                        return GroupMemberInfo {
                            name: member.name.clone(),
                            map: member.map.clone(),
                            is_leader,
                            is_self,
                            ..GroupMemberInfo::default()
                        };
                    };
                    return GroupMemberInfo {
                        name: me.name.clone(),
                        level: me.level,
                        class: Some(me.class),
                        map: if member.map.is_empty() {
                            self.map
                                .as_ref()
                                .map_or_else(String::new, |map| map.title.clone())
                        } else {
                            member.map.clone()
                        },
                        online: true,
                        hp_percent: Some(health_percent(me.hp, me.hp_max)),
                        is_leader,
                        is_self,
                    };
                }
                let entity = self.entities.values().find(|entity| {
                    entity.kind == EntityKind::Player
                        && entity.name.eq_ignore_ascii_case(&member.name)
                });
                let online = entity.is_some_and(|entity| !entity.dead);
                GroupMemberInfo {
                    name: member.name.clone(),
                    level: entity.map_or(0, |entity| entity.level),
                    class: entity
                        .filter(|entity| entity.kind == EntityKind::Player)
                        .map(|entity| entity.class),
                    map: member.map.clone(),
                    online,
                    hp_percent: entity.and_then(|entity| entity.hp_percent),
                    is_leader,
                    is_self,
                }
            })
            .collect()
    }

    /// 设置自己的交易准备状态（发包时乐观更新，服务端不回显「自己的锁定」）。
    pub fn set_trade_locked(&mut self, locked: bool) {
        self.trade.locked = locked;
        self.trade.version = self.trade.version.saturating_add(1);
    }

    /// 累加自己放上的金币（`C.TradeGold.Amount` 是增量，Crystal `PlayerObject.cs:10755`）。
    pub fn add_trade_gold(&mut self, amount: u32) {
        self.trade.gold = self.trade.gold.saturating_add(amount);
        self.trade.version = self.trade.version.saturating_add(1);
    }

    /// 展平成「职位 + 成员」的成员表（成员表按职位序号、再按名字排序）。
    #[must_use]
    pub fn guild_members(&self) -> Vec<(&GuildRankView, &GuildMemberView)> {
        let mut rows = Vec::new();
        for rank in &self.guild_ranks {
            for member in &rank.members {
                rows.push((rank, member));
            }
        }
        rows
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
    /// 血条：自己的百分比由 `HP / MaxHP` 算（Crystal `UserObject.cs:176` 的
    /// `PercentHealth = (byte)(HP / (float)Stats[Stat.HP] * 100)`），其他人的取
    /// `S.ObjectHealth` 存进 [`WorldEntity::hp_percent`] 的值——也就是只有收到过
    /// `ObjectHealth`/`HealthChanged` 的实体才有血条（Crystal 的
    /// `MapObject.DrawHealth` 也只在 `ShouldDrawHealth()` 或刚掉过血时画，
    /// `Client/MirObjects/MapObject.cs:461-476`）。
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
                mount: me.looks.mount,
                wings: me.looks.wings,
                name: Some(me.name.clone()),
                dead: me.dead,
                hp_percent: Some(health_percent(me.hp, me.hp_max)),
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
                mount: entity.looks.mount,
                wings: entity.looks.wings,
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

    /// 记一条本地音效事件（自己的走/跑/攻击、点药、施法、服务器不回发的那些）。
    ///
    /// 与服务端包产生的 [`SoundEvent`] 走同一条队列，播放器不必区分来源。
    /// 自己的性别来自 [`Self::self_player`]（`S.UserInformation.gender`）。
    pub fn push_local_sound(&mut self, sound: SoundId, spell: Spell) {
        let female = self
            .self_player
            .as_ref()
            .is_some_and(|me| me.gender == MirGender::Female);
        self.push_sound(sound, spell, female);
    }

    /// 记一条音效事件（内部与测试用）。
    pub fn push_sound(&mut self, sound: SoundId, spell: Spell, female: bool) {
        let id = self.next_sound_id;
        self.next_sound_id = self.next_sound_id.saturating_add(1);
        self.sound_events.push(SoundEvent {
            id,
            sound,
            spell,
            female,
        });
        if self.sound_events.len() > SOUND_EVENT_LIMIT {
            let excess = self.sound_events.len() - SOUND_EVENT_LIMIT;
            self.sound_events.drain(..excess);
        }
    }

    /// 取出所有编号大于 `after` 的音效事件（播放器每帧调用一次）。
    ///
    /// 返回的第二个值是「已消费到的编号」，调用方存起来下次传回来
    /// （与伤害/光效事件一样用「严格大于」判定，避免快照整体克隆导致重复播放）。
    /// 编号从 1 开始，所以 `after = 0` 表示「一条都还没消费」。
    #[must_use]
    pub fn sound_events_since(&self, after: u64) -> (Vec<SoundEvent>, u64) {
        let mut last = after;
        let mut events = Vec::new();
        for event in &self.sound_events {
            if event.id > after {
                last = last.max(event.id);
                events.push(*event);
            }
        }
        (events, last)
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
        let mut looks = PlayerLooks::from_equipment(
            &self.equipment,
            &self.item_infos,
            me.class,
            me.gender,
            me.hair,
        );
        // 坐骑/翅膀不来自装备槽，重算纸娃娃时不能把它们丢掉。
        looks.mount = me.looks.mount;
        looks.wings = me.looks.wings;
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
        NetCommand::AbandonQuest { quest_index } => {
            ClientPacket::AbandonQuest(c::AbandonQuest { quest_index })
        }
        NetCommand::ReadMail { mail_id } => ClientPacket::ReadMail(c::ReadMail { mail_id }),
        NetCommand::CollectParcel { mail_id } => {
            ClientPacket::CollectParcel(c::CollectParcel { mail_id })
        }
        NetCommand::DeleteMail { mail_id } => ClientPacket::DeleteMail(c::DeleteMail { mail_id }),
        NetCommand::LockMail { mail_id, lock } => {
            ClientPacket::LockMail(c::LockMail { mail_id, lock })
        }
        NetCommand::SendMail {
            name,
            message,
            gold,
            items_idx,
            stamped,
        } => ClientPacket::SendMail(c::SendMail {
            name,
            message,
            gold,
            items_idx,
            stamped,
        }),
        NetCommand::MailCost {
            gold,
            items_idx,
            stamped,
        } => ClientPacket::MailCost(c::MailCost {
            gold,
            items_idx,
            stamped,
        }),
        NetCommand::StoreItem { from, to } => ClientPacket::StoreItem(c::StoreItem { from, to }),
        NetCommand::TakeBackItem { from, to } => {
            ClientPacket::TakeBackItem(c::TakeBackItem { from, to })
        }
        NetCommand::GuildStorageItemChange { type_, from, to } => {
            ClientPacket::GuildStorageItemChange(c::GuildStorageItemChange { type_, from, to })
        }
        NetCommand::GuildStorageGoldChange { type_, amount } => {
            ClientPacket::GuildStorageGoldChange(c::GuildStorageGoldChange { type_, amount })
        }
        NetCommand::GuildBuffUpdate { action, id } => {
            ClientPacket::GuildBuffUpdate(c::GuildBuffUpdate { action, id })
        }
        NetCommand::EditGuildMember {
            change_type,
            rank_index,
            name,
            rank_name,
        } => ClientPacket::EditGuildMember(c::EditGuildMember {
            change_type,
            rank_index,
            name,
            rank_name,
        }),
        NetCommand::EditGuildNotice { notice } => {
            ClientPacket::EditGuildNotice(c::EditGuildNotice { notice })
        }
        NetCommand::RequestGuildInfo { type_ } => {
            ClientPacket::RequestGuildInfo(c::RequestGuildInfo { type_ })
        }
        NetCommand::TradeRequest => {
            // 交易只在世界里有意义；服务端自己校验朝向/相邻/冷却（`PlayerObject.TradeRequest`）。
            ClientPacket::TradeRequest(c::TradeRequest {})
        }
        NetCommand::TradeReply { accept } => ClientPacket::TradeReply(c::TradeReply {
            accept_invite: accept,
        }),
        NetCommand::TradeItem { from, to } => {
            ClientPacket::DepositTradeItem(c::DepositTradeItem { from, to })
        }
        NetCommand::TradeTakeBack { from, to } => {
            ClientPacket::RetrieveTradeItem(c::RetrieveTradeItem { from, to })
        }
        NetCommand::TradeGold { amount } => {
            // 服务端只给**对方**发 `S.TradeGold`，自己这边不会回声，所以本地先累加
            // （Crystal `TradeDialogs.cs:90-93` 的 `User.TradeGoldAmount += Amount`）。
            if amount > 0 {
                snapshot.add_trade_gold(amount);
            }
            ClientPacket::TradeGold(c::TradeGold { amount })
        }
        NetCommand::TradeConfirm { locked } => {
            snapshot.set_trade_locked(locked);
            ClientPacket::TradeConfirm(c::TradeConfirm { locked })
        }
        NetCommand::TradeCancel => {
            // 取消后本地立即关窗（服务端会回 `S.TradeCancel{Unlock=false}`，但那是给双方的）。
            snapshot.trade.open = false;
            snapshot.trade.locked = false;
            snapshot.trade.partner_ready = false;
            ClientPacket::TradeCancel(c::TradeCancel {})
        }
        NetCommand::AddMember { name } => {
            if name.trim().is_empty() {
                return Ok(());
            }
            ClientPacket::AddMember(c::AddMember { name })
        }
        NetCommand::DelMember { name } => {
            if name.trim().is_empty() {
                return Ok(());
            }
            ClientPacket::DelMember(c::DelMember { name })
        }
        NetCommand::GroupReply { accept } => ClientPacket::GroupInvite(c::GroupInvite {
            accept_invite: accept,
        }),
        NetCommand::SwitchGroup { allow_group } => {
            ClientPacket::SwitchGroup(c::SwitchGroup { allow_group })
        }
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
        ServerPacket::UserStorage(_) => "UserStorage",
        ServerPacket::NPCStorage(_) => "NPCStorage",
        ServerPacket::StoreItem(_) => "StoreItem",
        ServerPacket::TakeBackItem(_) => "TakeBackItem",
        ServerPacket::NewQuestInfo(_) => "NewQuestInfo",
        ServerPacket::ChangeQuest(_) => "ChangeQuest",
        ServerPacket::CompleteQuest(_) => "CompleteQuest",
        ServerPacket::ShareQuest(_) => "ShareQuest",
        ServerPacket::ReceiveMail(_) => "ReceiveMail",
        ServerPacket::MailSendRequest(_) => "MailSendRequest",
        ServerPacket::MailCost(_) => "MailCost",
        ServerPacket::MailSent(_) => "MailSent",
        ServerPacket::ParcelCollected(_) => "ParcelCollected",
        ServerPacket::MailLockedItem(_) => "MailLockedItem",
        ServerPacket::GuildStatus(_) => "GuildStatus",
        ServerPacket::GuildMemberChange(_) => "GuildMemberChange",
        ServerPacket::GuildNoticeChange(_) => "GuildNoticeChange",
        ServerPacket::GuildStorageList(_) => "GuildStorageList",
        ServerPacket::GuildStorageItemChange(_) => "GuildStorageItemChange",
        ServerPacket::GuildStorageGoldChange(_) => "GuildStorageGoldChange",
        ServerPacket::GuildBuffList(_) => "GuildBuffList",
        ServerPacket::TradeRequest(_) => "TradeRequest",
        ServerPacket::TradeAccept(_) => "TradeAccept",
        ServerPacket::TradeGold(_) => "TradeGold",
        ServerPacket::TradeItem(_) => "TradeItem",
        ServerPacket::TradeConfirm(_) => "TradeConfirm",
        ServerPacket::TradeCancel(_) => "TradeCancel",
        ServerPacket::DepositTradeItem(_) => "DepositTradeItem",
        ServerPacket::RetrieveTradeItem(_) => "RetrieveTradeItem",
        ServerPacket::SwitchGroup(_) => "SwitchGroup",
        ServerPacket::AddMember(_) => "AddMember",
        ServerPacket::DeleteMember(_) => "DeleteMember",
        ServerPacket::DeleteGroup(_) => "DeleteGroup",
        ServerPacket::GroupInvite(_) => "GroupInvite",
        ServerPacket::GroupMembersMap(_) => "GroupMembersMap",
        ServerPacket::SendMemberLocation(_) => "SendMemberLocation",
        ServerPacket::ColourChanged(_) => "ColourChanged",
        ServerPacket::ObjectColourChanged(_) => "ObjectColourChanged",
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
                mini_map: info.mini_map,
                big_map: info.big_map,
                music: info.music,
            });
        }
        ServerPacket::NewMapInfo(info) => {
            let map = snapshot.map.get_or_insert_with(MapSnapshot::default);
            map.map_index = info.map_index;
            map.title.clone_from(&info.info.title);
            map.width = info.info.width;
            map.height = info.info.height;
            // `ClientMapInfo.BigMap` 是 i32（`ClientData.cs:324`），负数/超界按 0 处理。
            map.big_map = u16::try_from(info.info.big_map).unwrap_or(0);
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
                mini_map: changed.mini_map,
                big_map: changed.big_map,
                music: changed.music,
            });
            snapshot.entities.clear();
            // 换地图等于离开当前 NPC，UI 必须把对话/商店/仓库收起来。
            snapshot.npc_dialog = None;
            snapshot.npc_goods = None;
            snapshot.npc_panels = NpcPanels::default();
            snapshot.storage_open = false;
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
            // `S.UserInformation` 不带坐骑/翅膀；保留之前 `S.MountUpdate` / `S.ObjectPlayer`
            // 已经写入的值（登录链路通常先发 UserInformation，再发 MountUpdate）。
            let (mount, wings) = snapshot
                .self_player
                .as_ref()
                .map_or((0, 0), |me| (me.looks.mount, me.looks.wings));
            let mut looks = PlayerLooks::from_equipment(
                &snapshot.equipment,
                &snapshot.item_infos,
                info.class,
                info.gender,
                info.hair,
            );
            looks.mount = mount;
            looks.wings = wings;
            snapshot.self_player = Some(SelfPlayer {
                object_id: info.object_id,
                real_id: info.real_id,
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
            let mut looks = PlayerLooks::from_server(
                player.class,
                player.gender,
                player.hair,
                player.armour,
                player.weapon,
            );
            // `S.ObjectPlayer` 在 `Armour` 之后带 `WingEffect`、`MountType`、`RidingMount`
            // （`Shared/ServerPackets.cs`；Crystal 在 `PlayerObject.cs:141,145,176` 接收）。
            looks.wings = wing_layer(player.wing_effect);
            looks.mount = mount_layer(player.mount_type, player.riding_mount);
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
                    level: player.level,
                    class: player.class,
                    name_colour: Some(player.name_colour),
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
            // 走路音：Crystal 在步帧播 `WalkGroundL`（`PlayerObject.cs:3596-3599`）。
            snapshot.push_sound(SoundId::Walk, Spell::None, false);
            update_entity(snapshot, walk.object_id, |entity| {
                entity.position = walk.location;
                entity.direction = walk.direction;
                entity.set_action(ActorAction::Walking, now_ms);
            });
        }
        ServerPacket::ObjectRun(run) => {
            snapshot.push_sound(SoundId::Run, Spell::None, false);
            update_entity(snapshot, run.object_id, |entity| {
                entity.position = run.location;
                entity.direction = run.direction;
                entity.set_action(ActorAction::Running, now_ms);
            });
        }
        ServerPacket::ObjectAttack(attack) => {
            // 挥空/挥武器（`PlayerObject.cs:4779-4847`）＋施法音（若这一下是魔法，
            // `PlayerObject.cs:1538` 的 `20000 + Spell × 10`）。
            snapshot.push_sound(SoundId::Swing, attack.spell, false);
            update_entity(snapshot, attack.object_id, |entity| {
                entity.position = attack.location;
                entity.direction = attack.direction;
                entity.set_action(ActorAction::Attacking, now_ms);
            });
        }
        ServerPacket::ObjectStruck(struck) => {
            // 被打中的身体音（`PlayerObject.cs:4700-4762` 的 `StruckBodySword + add`）。
            snapshot.push_sound(SoundId::Hit, Spell::None, false);
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
        ServerPacket::Struck(_) => {
            // 自己被攻击（`S.Struck` 只发给挨打的人）：受击呻吟按性别分档
            // （`PlayerObject.cs:624-625` 的 `FlinchSound`）。
            snapshot.push_local_sound(SoundId::Struck, Spell::None);
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
            // 怪物/玩家死亡：Crystal 玩家用 `MaleDie/FemaleDie`（`PlayerObject.cs:4854-4855`），
            // 怪物用 `BaseSound + 9`（`MonsterObject.cs:4087`）；本作统一按性别取死亡音。
            snapshot.push_sound(SoundId::Die, Spell::None, false);
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
            // 自己阵亡：男女两档死亡音（`SoundList.cs:133-134`）。
            snapshot.push_local_sound(SoundId::Die, Spell::None);
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
            // 升级音（`GameScene.cs:3853` 的 `SoundList.LevelUp`）。
            snapshot.push_local_sound(SoundId::LevelUp, Spell::None);
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
            // 拾取音：Crystal 只有宠物拾取音（`MonsterObject.cs:4266` 的 `PetPickup`），
            // 简化：玩家捡东西也用它。
            snapshot.push_local_sound(SoundId::Pickup, Spell::None);
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
            // 金币音（`GameScene.cs:3300` 的 `GainedGold`；数量为 0 时 Crystal 直接 return，
            // 这里也跳过：`GainedGold` 的 `p.Gold == 0` 分支，`GameScene.cs:3297`）。
            if gained.gold > 0 {
                snapshot.push_local_sound(SoundId::Gold, Spell::None);
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
            // 失去金币也响同一个音（`GameScene.cs:3306` 的 `LoseGold`）。
            snapshot.push_local_sound(SoundId::Gold, Spell::None);
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
            // 仓库页是「页面 + S.NPCStorage」两步；先按非仓库页处理，随后到达的
            // `S.NPCStorage` 会重新打开面板（`NPCScript.cs:1002-1006`）。
            snapshot.storage_open = false;
            // NPC 对话音：（简化）Crystal 没有专用音效，面板按钮音是 `ButtonA`
            // （`SoundList.cs:41`，点击路径 `MirControl.cs:823-824`）。
            // 每收到一页非空正文都响一次（开窗与翻页，接近 Crystal 的「点按钮翻页」），
            // 空正文 = 关窗，不响。
            if !response.page.is_empty() {
                snapshot.push_local_sound(SoundId::NpcDialog, Spell::None);
            }
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
        ServerPacket::UserStorage(storage) => {
            // `Storage = null` 表示「客户端保留旧数据」（Crystal 语义，`PlayerObject.cs:8169`）。
            if let Some(slots) = &storage.storage {
                snapshot.storage = fit_inventory_slots(slots, STORAGE_SLOTS);
            }
        }
        ServerPacket::NPCStorage(_) => {
            // 服务端打开仓库面板（`NPCScript.cs:1002-1006`）：快照只记「该显示仓库」，
            // 由 UI 在版本变化时自动打开面板（`UiState::sync`）。
            snapshot.storage_open = true;
            snapshot.storage_open_version = snapshot.storage_open_version.saturating_add(1);
            if snapshot.storage.len() < STORAGE_SLOTS {
                snapshot.storage.resize(STORAGE_SLOTS, None);
            }
        }
        ServerPacket::StoreItem(stored) => {
            if stored.success {
                store_inventory_to_storage(snapshot, stored.from, stored.to);
            }
        }
        ServerPacket::TakeBackItem(taken) => {
            if taken.success {
                take_back_storage_to_inventory(snapshot, taken.from, taken.to);
            }
        }
        ServerPacket::NewQuestInfo(info) => {
            snapshot.quest_infos.insert(
                info.info.index,
                QuestInfoView::from_client_quest(&info.info),
            );
            // 任务名可能先于定义到达（登录包序是定义在前，但假包/切图重连不保证），
            // 定义到位后把已有进度的显示名刷新一次。
            for quest in &mut snapshot.quests {
                if let Some(definition) = snapshot.quest_infos.get(&quest.id) {
                    quest.name.clone_from(&definition.name);
                    quest.group.clone_from(&definition.group);
                    quest.time_limit_seconds = definition.time_limit_seconds;
                }
            }
        }
        ServerPacket::ChangeQuest(change) => {
            apply_quest_change(snapshot, &change.quest, change.quest_state, now_ms);
        }
        ServerPacket::CompleteQuest(complete) => {
            snapshot
                .completed_quests
                .clone_from(&complete.completed_quests);
        }
        ServerPacket::ShareQuest(share) => {
            let name = snapshot.quest_infos.get(&share.quest_index).map_or_else(
                || format!("任务 #{}", share.quest_index),
                |info| info.name.clone(),
            );
            push_chat(
                snapshot,
                ChatLine {
                    speaker: None,
                    text: format!("{} 分享了任务：{name}", share.sharer_name),
                    kind: ChatType::System,
                },
            );
        }
        ServerPacket::ReceiveMail(received) => {
            snapshot.mails = received
                .mail
                .iter()
                .map(MailView::from_client_mail)
                .collect();
        }
        ServerPacket::MailSendRequest(_) => {
            snapshot.mail_send_request = true;
            snapshot.mail_send_request_version =
                snapshot.mail_send_request_version.saturating_add(1);
        }
        ServerPacket::MailCost(cost) => {
            snapshot.mail_cost = Some(cost.cost);
        }
        ServerPacket::MailSent(sent) => {
            snapshot.mail_sent_result = Some(sent.result);
        }
        ServerPacket::ParcelCollected(collected) => {
            snapshot.parcel_collected_result = Some(collected.result);
        }
        ServerPacket::MailLockedItem(locked) => {
            snapshot
                .mail_locked_items
                .insert(locked.unique_id, locked.locked);
        }
        ServerPacket::GuildStatus(status) => {
            snapshot.guild = Some(GuildView {
                name: status.guild_name.clone(),
                rank_name: status.guild_rank_name.clone(),
                level: status.level,
                experience: status.experience,
                max_experience: status.max_experience,
                gold: status.gold,
                spare_points: status.spare_points,
                member_count: status.member_count,
                max_members: status.max_members,
                voting: status.voting,
                item_count: status.item_count,
                buff_count: status.buff_count,
                my_options: status.my_options,
                my_rank_id: status.my_rank_id,
            });
            if status.guild_name.is_empty() {
                // Crystal：`p.GuildName == ""` 时关窗并清空本地行会数据
                // （`GameScene.cs:5874-5877`）。
                snapshot.guild_ranks.clear();
                snapshot.guild_notice.clear();
                snapshot.guild_storage.clear();
                snapshot.guild_active_buffs.clear();
            }
        }
        ServerPacket::GuildMemberChange(change) => {
            apply_guild_member_change(snapshot, change);
        }
        ServerPacket::GuildNoticeChange(notice) => {
            if notice.update < 0 {
                // `update == -1` 是「公告变了，自己去拉」的通知（`GuildObject.cs:599`），
                // 客户端此时清空缓存；真正的公告正文由随后的 `S.GuildNoticeChange{update>=0}` 发。
                snapshot.guild_notice.clear();
            } else {
                snapshot.guild_notice.clone_from(&notice.notice);
            }
        }
        ServerPacket::GuildStorageList(list) => {
            snapshot.guild_storage = fit_guild_storage_slots(&list.items);
        }
        ServerPacket::GuildStorageItemChange(change) => {
            apply_guild_storage_item_change(snapshot, change);
        }
        ServerPacket::GuildStorageGoldChange(gold) => {
            if let Some(guild) = &mut snapshot.guild {
                match gold.type_ {
                    // 0 = 存入（行会资金增加），3 = 退还（Crystal `GameScene.cs:5912-5931`）。
                    0 | 3 => guild.gold = guild.gold.saturating_add(gold.amount),
                    // 1 = 取出，2 = 购买增益花掉：都减少，按 Crystal 的「不够就归零」。
                    _ => guild.gold = guild.gold.saturating_sub(gold.amount),
                }
            }
            // 行会频道提示：0 = 捐献、1 = 取出；2（买增益）/3（退还）不提示
            // （Crystal `GameScene.cs:5910-5931` 同样只对 0/1 发聊天）。
            let text = match gold.type_ {
                0 => Some(format!("{} 向行会捐献 {} 金币", gold.name, gold.amount)),
                1 => Some(format!("{} 从行会取出 {} 金币", gold.name, gold.amount)),
                _ => None,
            };
            if let Some(text) = text {
                push_chat(
                    snapshot,
                    ChatLine {
                        speaker: None,
                        text,
                        kind: ChatType::Guild,
                    },
                );
            }
        }
        ServerPacket::GuildBuffList(list) => {
            apply_guild_buff_list(snapshot, list);
        }
        ServerPacket::TradeRequest(request) => {
            // 收到邀请：UI 弹「XXX 请求与你交易」接受/拒绝（Crystal `GameScene.cs:6303-6311`）。
            snapshot.trade.invitation = Some(request.name.clone());
            snapshot.trade.invitation_version = snapshot.trade.invitation_version.saturating_add(1);
        }
        ServerPacket::TradeAccept(accept) => {
            // 双方都收到 `S.TradeAccept`（Crystal `PlayerObject.cs:10741-10742`）：
            // 打开交易窗，并像 Crystal 的 `TradeDialog.TradeAccept()` 一样把背包一起显示
            // （`TradeDialogs.cs:152-161`）。
            snapshot.trade.open = true;
            snapshot.trade.open_version = snapshot.trade.open_version.saturating_add(1);
            snapshot.trade.partner.clone_from(&accept.name);
            snapshot.trade.items = vec![None; TRADE_SLOTS];
            snapshot.trade.partner_items = vec![None; TRADE_SLOTS];
            snapshot.trade.gold = 0;
            snapshot.trade.partner_gold = 0;
            snapshot.trade.locked = false;
            snapshot.trade.partner_ready = false;
            snapshot.trade.version = snapshot.trade.version.saturating_add(1);
            snapshot.trade.invitation = None;
        }
        ServerPacket::DepositTradeItem(deposit) => {
            if deposit.success {
                trade_deposit(snapshot, deposit.from, deposit.to);
            }
        }
        ServerPacket::RetrieveTradeItem(retrieve) => {
            if retrieve.success {
                trade_retrieve(snapshot, retrieve.from, retrieve.to);
            }
        }
        ServerPacket::TradeItem(items) => {
            // 对方改了报价：服务端 `TradeUnlock()` 已经把**双方**的锁定清掉
            // （`PlayerObject.cs:10761-10786`），客户端同步清自己的准备状态
            // （Crystal `GameScene.cs:6323-6328` 的 `ChangeLockState(false)`）。
            snapshot.trade.partner_items = items
                .trade_items
                .iter()
                .map(|item| item.as_ref().map(InventoryItem::from_user_item))
                .collect();
            snapshot.trade.partner_items.resize(TRADE_SLOTS, None);
            snapshot.trade.locked = false;
            snapshot.trade.partner_ready = false;
            snapshot.trade.version = snapshot.trade.version.saturating_add(1);
        }
        ServerPacket::TradeGold(gold) => {
            snapshot.trade.partner_gold = gold.amount;
            snapshot.trade.locked = false;
            snapshot.trade.partner_ready = false;
            snapshot.trade.version = snapshot.trade.version.saturating_add(1);
        }
        ServerPacket::TradeConfirm(_) => {
            // 成交（空包）：Crystal 的 `TradeConfirm()` 直接重置并关窗（`GameScene.cs:6329-6332`）。
            snapshot.trade.open = false;
            snapshot.trade.partner_ready = snapshot.trade.locked;
            snapshot.trade.locked = false;
            snapshot.trade.version = snapshot.trade.version.saturating_add(1);
            push_chat(
                snapshot,
                ChatLine {
                    speaker: None,
                    text: "交易完成。".to_owned(),
                    kind: ChatType::System,
                },
            );
        }
        ServerPacket::TradeCancel(cancel) => {
            if cancel.unlock {
                // `Unlock = true` 只在双方都锁定后的失败分支里发（`PlayerObject.cs:10817-10852`）：
                // 这是「对方确实准备过」的唯一信号，见 [`TradeView::partner_ready`]。
                snapshot.trade.partner_ready = true;
                snapshot.trade.locked = false;
                snapshot.trade.version = snapshot.trade.version.saturating_add(1);
            } else {
                // 交易被取消：关窗（Crystal 弹「交易已取消」提示框，`GameScene.cs:6341-6345`）。
                snapshot.trade.open = false;
                snapshot.trade.locked = false;
                snapshot.trade.partner_ready = false;
                snapshot.trade.invitation = None;
                snapshot.trade.version = snapshot.trade.version.saturating_add(1);
                push_chat(
                    snapshot,
                    ChatLine {
                        speaker: None,
                        text: "交易已取消。".to_owned(),
                        kind: ChatType::System,
                    },
                );
            }
        }
        ServerPacket::SwitchGroup(switch) => {
            // Crystal `GameScene.cs:4970-4976`：关掉「允许组队」时本地直接解散队伍显示。
            snapshot.group.allow_group = switch.allow_group;
            if !switch.allow_group && snapshot.group.has_members() {
                snapshot.group.members.clear();
                push_chat(
                    snapshot,
                    ChatLine {
                        speaker: None,
                        text: "你已离开队伍。".to_owned(),
                        kind: ChatType::Group,
                    },
                );
            }
        }
        ServerPacket::AddMember(member) => {
            // `GroupList.Add(p.Name)` + 组队频道提示（`GameScene.cs:5006-5010`）。
            snapshot.group.add(&member.name);
            push_chat(
                snapshot,
                ChatLine {
                    speaker: None,
                    text: format!("{} 加入了队伍。", member.name),
                    kind: ChatType::Group,
                },
            );
        }
        ServerPacket::DeleteMember(member) => {
            snapshot.group.remove(&member.name);
            push_chat(
                snapshot,
                ChatLine {
                    speaker: None,
                    text: format!("{} 退出了队伍。", member.name),
                    kind: ChatType::Group,
                },
            );
        }
        ServerPacket::DeleteGroup(_) => {
            snapshot.group.members.clear();
            push_chat(
                snapshot,
                ChatLine {
                    speaker: None,
                    text: "你已离开队伍。".to_owned(),
                    kind: ChatType::Group,
                },
            );
        }
        ServerPacket::GroupInvite(invite) => {
            // 弹「XXX 邀请你组队」（Crystal `GameScene.cs:4994-5005`）。
            snapshot.group.invitation = Some(invite.name.clone());
            snapshot.group.invitation_version = snapshot.group.invitation_version.saturating_add(1);
        }
        ServerPacket::GroupMembersMap(map) => {
            if let Some(member) = snapshot
                .group
                .members
                .iter_mut()
                .find(|member| member.name.eq_ignore_ascii_case(&map.player_name))
            {
                member.map.clone_from(&map.player_map);
            }
        }
        ServerPacket::SendMemberLocation(location) => {
            if let Some(member) = snapshot
                .group
                .members
                .iter_mut()
                .find(|member| member.name.eq_ignore_ascii_case(&location.member_name))
            {
                member.location = Some(location.member_location);
            }
        }
        ServerPacket::ColourChanged(changed) => {
            // 自己的名字颜色（Crystal `PlayerObject.RefreshNameColour` → `S.ColourChanged`，
            // `Server/MirObjects/PlayerObject.cs:1838-1847`）。自己的头顶名字固定金色，
            // 所以这里只记下来供以后 HUD/聊天用；快照不做展示。
            tracing::debug!(colour = changed.name_colour.0, "自己的名字颜色变了");
        }
        ServerPacket::ObjectColourChanged(changed) => {
            update_entity(snapshot, changed.object_id, |entity| {
                entity.name_colour = Some(changed.name_colour);
            });
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
                entity.looks.wings = wing_layer(update.wing_effect);
                entity.image = entity.looks.armour;
            });
        }
        ServerPacket::MountUpdate(update) => {
            // `S.MountUpdate`（Crystal `PlayerObject.MountUpdate`，`PlayerObject.cs:202-219`）
            // 同时更新坐骑外观与是否骑乘；没有坐骑时 `MountType = -1`。
            update_entity(snapshot, update.object_id, |entity| {
                entity.looks.mount = mount_layer(update.mount_type, update.riding_mount);
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

/// 把协议里的槽位列表搬成界面物品并补齐到固定格数。
///
/// 协议允许 `null` 槽位；长度来自服务端（仓库 80、行会仓库 112），超过的截断、不足的补空，
/// 这样 UI 的格子下标永远等于协议下标。
fn fit_inventory_slots(slots: &[Option<UserItem>], size: usize) -> Vec<Option<InventoryItem>> {
    let mut out: Vec<Option<InventoryItem>> = slots
        .iter()
        .take(size)
        .map(|slot| slot.as_ref().map(InventoryItem::from_user_item))
        .collect();
    out.resize(size, None);
    out
}

/// 把 `S.GuildStorageList` 的槽位搬成界面物品（补齐到 112 格）。
fn fit_guild_storage_slots(items: &[Option<GuildStorageItem>]) -> Vec<Option<InventoryItem>> {
    let mut out: Vec<Option<InventoryItem>> = items
        .iter()
        .take(GUILD_STORAGE_SLOTS)
        .map(|slot| {
            slot.as_ref()
                .map(|stored| InventoryItem::from_user_item(&stored.item))
        })
        .collect();
    out.resize(GUILD_STORAGE_SLOTS, None);
    out
}

/// 背包 → 仓库（`S.StoreItem{success=true}`）。
///
/// 服务端只回「哪一格搬到哪一格」，物品数据仍以客户端背包里的为准
/// （Crystal `PlayerObject.StoreItem` 的 `Account.Storage[to] = temp`）。
fn store_inventory_to_storage(snapshot: &mut WorldSnapshot, from: i32, to: i32) {
    let (Ok(from), Ok(to)) = (usize::try_from(from), usize::try_from(to)) else {
        return;
    };
    if to >= STORAGE_SLOTS {
        return;
    }
    if snapshot.storage.len() < STORAGE_SLOTS {
        snapshot.storage.resize(STORAGE_SLOTS, None);
    }
    let Some(item) = snapshot.inventory.get_mut(from).and_then(Option::take) else {
        return;
    };
    snapshot.storage[to] = Some(item);
}

/// 仓库 → 背包（`S.TakeBackItem{success=true}`）。
fn take_back_storage_to_inventory(snapshot: &mut WorldSnapshot, from: i32, to: i32) {
    let (Ok(from), Ok(to)) = (usize::try_from(from), usize::try_from(to)) else {
        return;
    };
    let Some(item) = snapshot.storage.get_mut(from).and_then(Option::take) else {
        return;
    };
    if snapshot.inventory.len() <= to {
        snapshot.inventory.resize(to + 1, None);
    }
    snapshot.inventory[to] = Some(item);
}

/// 背包 → 交易格（`S.DepositTradeItem{success=true}`）。
///
/// 服务端只回「哪一格搬到哪一格」，物品数据仍以客户端背包里的为准
/// （Crystal `PlayerObject.DepositTradeItem` 的 `Info.Trade[to] = temp`，
/// `Server/MirObjects/PlayerObject.cs:10566-10576`）。
fn trade_deposit(snapshot: &mut WorldSnapshot, from: i32, to: i32) {
    let (Ok(from), Ok(to)) = (usize::try_from(from), usize::try_from(to)) else {
        return;
    };
    if to >= TRADE_SLOTS {
        return;
    }
    snapshot.trade.normalize();
    let Some(item) = snapshot.inventory.get_mut(from).and_then(Option::take) else {
        return;
    };
    snapshot.trade.items[to] = Some(item);
    snapshot.trade.version = snapshot.trade.version.saturating_add(1);
}

/// 交易格 → 背包（`S.RetrieveTradeItem{success=true}`）。
fn trade_retrieve(snapshot: &mut WorldSnapshot, from: i32, to: i32) {
    let (Ok(from), Ok(to)) = (usize::try_from(from), usize::try_from(to)) else {
        return;
    };
    snapshot.trade.normalize();
    let Some(item) = snapshot.trade.items.get_mut(from).and_then(Option::take) else {
        return;
    };
    if snapshot.inventory.len() <= to {
        snapshot.inventory.resize(to + 1, None);
    }
    snapshot.inventory[to] = Some(item);
    snapshot.trade.version = snapshot.trade.version.saturating_add(1);
}

/// 应用一条 `S.ChangeQuest`。
///
/// * `Add`：新接/重接任务，记录本地接取时刻（用于限时倒计时）；
/// * `Update`：替换进度文本，保留接取时刻；
/// * `Remove`：从进行中列表移除（交付/放弃/超时都由服务端发这一条）。
fn apply_quest_change(
    snapshot: &mut WorldSnapshot,
    progress: &ClientQuestProgress,
    state: QuestState,
    now_ms: u64,
) {
    let id = progress.id;
    match state {
        QuestState::Add => {
            let accepted_at_ms = snapshot
                .quest(id)
                .and_then(|quest| quest.accepted_at_ms)
                .or(Some(now_ms));
            let definition = snapshot.quest_infos.get(&id).cloned();
            let view = QuestView {
                id,
                // 定义缺失时留空串，由 UI 按当前语言显示「任务 #n」（不能在这里写死中文）。
                name: definition
                    .as_ref()
                    .map_or_else(String::new, |info| info.name.clone()),
                group: definition
                    .as_ref()
                    .map_or_else(String::new, |info| info.group.clone()),
                task_list: progress.task_list.clone(),
                taken: progress.taken,
                completed: progress.completed,
                new: progress.new,
                time_limit_seconds: definition
                    .as_ref()
                    .map_or(0, |info| info.time_limit_seconds),
                accepted_at_ms,
            };
            if let Some(existing) = snapshot.quests.iter_mut().find(|quest| quest.id == id) {
                *existing = view;
            } else {
                snapshot.quests.push(view);
            }
        }
        QuestState::Update => {
            if let Some(quest) = snapshot.quests.iter_mut().find(|quest| quest.id == id) {
                quest.task_list.clone_from(&progress.task_list);
                quest.taken = progress.taken;
                quest.completed = progress.completed;
                quest.new = progress.new;
            } else {
                // 进度包先于接取包到达（理论上不会，防御性补一条）。
                apply_quest_change(snapshot, progress, QuestState::Add, now_ms);
            }
        }
        QuestState::Remove => {
            snapshot.quests.retain(|quest| quest.id != id);
        }
    }
}

/// 从所有职位里摘掉一个成员。
fn take_guild_member(snapshot: &mut WorldSnapshot, name: &str) -> Option<GuildMemberView> {
    for rank in &mut snapshot.guild_ranks {
        if let Some(index) = rank.members.iter().position(|member| member.name == name) {
            return Some(rank.members.remove(index));
        }
    }
    None
}

/// 插入/替换一个职位快照，并把该职位里出现的成员从其它职位里摘掉。
///
/// Crystal 的 `GuildMemberChange` 一次只带**一个**职位的成员列表
/// （`GuildObject.cs:315-330`），被调动者的旧职位不会重发，所以客户端要自己摘。
fn upsert_guild_rank(snapshot: &mut WorldSnapshot, rank: GuildRankView) {
    let names: Vec<String> = rank
        .members
        .iter()
        .map(|member| member.name.clone())
        .collect();
    for other in &mut snapshot.guild_ranks {
        if other.index == rank.index {
            continue;
        }
        other.members.retain(|member| !names.contains(&member.name));
    }
    if let Some(existing) = snapshot
        .guild_ranks
        .iter_mut()
        .find(|existing| existing.index == rank.index)
    {
        *existing = rank;
    } else {
        snapshot.guild_ranks.push(rank);
    }
    snapshot.guild_ranks.sort_by_key(|rank| rank.index);
}

/// 应用一条 `S.GuildMemberChange`（状态码语义照 Crystal `GameScene.cs:5813-5868`）。
///
/// | `status` | 含义 | 客户端动作 |
/// |---|---|---|
/// | 0 | 成员下线 | 该成员 `online = false` |
/// | 1 | 成员上线 | 该成员 `online = true` + 行会频道消息 |
/// | 2 | 新成员入会 | 插入到 `rank_index` 职位（在线） |
/// | 3 | 被踢 | 从成员表移除 |
/// | 4 | 主动退会 | 从成员表移除 |
/// | 5 | 职位调动通知（给其他成员） | 把该成员挪到 `rank_index` |
/// | 6 | 新建职位 | `ranks[0]` 插入 |
/// | 7 | 职位改名/权限变化 | `ranks[0]` 替换同序号职位 |
/// | 8 | 我自己的职位变了 | `ranks[0]` 是我的新职位，同时更新 [`GuildView`] 的权限 |
/// | 255 | 完整成员表 | 整体替换 |
fn apply_guild_member_change(snapshot: &mut WorldSnapshot, change: &GuildMemberChange) {
    let name = change.name.clone();
    match change.status {
        0 => {
            if let Some(member) = snapshot
                .guild_ranks
                .iter_mut()
                .flat_map(|rank| rank.members.iter_mut())
                .find(|member| member.name == name)
            {
                member.online = false;
            }
        }
        1 => {
            let mut found = false;
            for member in snapshot
                .guild_ranks
                .iter_mut()
                .flat_map(|rank| rank.members.iter_mut())
            {
                if member.name == name {
                    member.online = true;
                    found = true;
                }
            }
            if found {
                push_chat(
                    snapshot,
                    ChatLine {
                        speaker: None,
                        text: format!("{name} 上线了"),
                        kind: ChatType::Guild,
                    },
                );
            }
        }
        2 => {
            push_chat(
                snapshot,
                ChatLine {
                    speaker: None,
                    text: format!("{name} 加入了行会"),
                    kind: ChatType::Guild,
                },
            );
            // 新成员可能已经在别的职位里（重连/重复包）：先摘出来再放到 `rank_index`。
            let existing = take_guild_member(snapshot, &name);
            let member = existing.unwrap_or_else(|| GuildMemberView {
                name,
                id: 0,
                last_login: mir_core::DateTimeBinary::default(),
                online: true,
            });
            let rank_index = i32::from(change.rank_index);
            if let Some(rank) = snapshot
                .guild_ranks
                .iter_mut()
                .find(|rank| rank.index == rank_index)
            {
                rank.members.push(member);
            }
            if let Some(guild) = &mut snapshot.guild {
                guild.member_count = guild.member_count.saturating_add(1);
            }
        }
        3 | 4 => {
            let verb = if change.status == 3 {
                "被移出行会"
            } else {
                "离开了行会"
            };
            if take_guild_member(snapshot, &name).is_some() {
                push_chat(
                    snapshot,
                    ChatLine {
                        speaker: None,
                        text: format!("{name} {verb}"),
                        kind: ChatType::Guild,
                    },
                );
                if let Some(guild) = &mut snapshot.guild {
                    guild.member_count = guild.member_count.saturating_sub(1).max(0);
                }
            }
        }
        5 | 8 => {
            // 调动：`status = 5` 是「别人被挪到 rank_index」，`status = 8` 带的是
            // 我自己的新职位快照（`ranks[0]`），成员表由它决定。
            if let Some(rank) = change.ranks.first() {
                upsert_guild_rank(snapshot, GuildRankView::from_guild_rank(rank));
                if change.status == 8 {
                    if let Some(guild) = &mut snapshot.guild {
                        guild.my_rank_id = rank.index;
                        guild.rank_name.clone_from(&rank.name);
                        guild.my_options =
                            mir_core::enums::GuildRankOptions::from_bits_truncate(rank.options);
                    }
                }
            } else if let Some(position) = snapshot
                .guild_ranks
                .iter()
                .position(|rank| rank.index == i32::from(change.rank_index))
            {
                if let Some(member) = take_guild_member(snapshot, &name) {
                    snapshot.guild_ranks[position].members.push(member);
                }
            }
        }
        6 | 7 => {
            if let Some(rank) = change.ranks.first() {
                upsert_guild_rank(snapshot, GuildRankView::from_guild_rank(rank));
            }
        }
        255 => {
            snapshot.guild_ranks = change
                .ranks
                .iter()
                .map(GuildRankView::from_guild_rank)
                .collect();
            snapshot.guild_ranks.sort_by_key(|rank| rank.index);
        }
        _ => {}
    }
}

/// 应用一条 `S.GuildStorageItemChange`（广播给全行会，`crates/mir-server/src/guild_storage.rs`）。
///
/// * `0` 存入：`Item` 是存入的物品，本地背包 `from` 清空（只有自己才有背包数据）；
/// * `1` 取回：**没有 `Item`**，从本地仓库格 `from` 取出放进背包 `to`；
/// * `2` 仓库内移动：`Item` 是移动后的目标格物品；
/// * `3..=5` 是失败回执（Crystal 的 `p.Type = 3 + type`），不改状态。
fn apply_guild_storage_item_change(snapshot: &mut WorldSnapshot, change: &GuildStorageItemChange) {
    let is_me = snapshot
        .self_player
        .as_ref()
        .is_some_and(|me| i64::from(me.real_id) == i64::from(change.user));
    let Ok(to) = usize::try_from(change.to) else {
        return;
    };
    let from = usize::try_from(change.from).ok();
    match change.change_type {
        0 => {
            let item = change
                .item
                .as_ref()
                .map(|stored| InventoryItem::from_user_item(&stored.item));
            if snapshot.guild_storage.len() < GUILD_STORAGE_SLOTS {
                snapshot.guild_storage.resize(GUILD_STORAGE_SLOTS, None);
            }
            if to < GUILD_STORAGE_SLOTS {
                snapshot.guild_storage[to] = item;
            }
            if is_me {
                if let Some(slot) = from.and_then(|from| snapshot.inventory.get_mut(from)) {
                    *slot = None;
                }
            }
        }
        1 => {
            let item =
                from.and_then(|from| snapshot.guild_storage.get_mut(from).and_then(Option::take));
            if is_me {
                if let Some(item) = item {
                    if snapshot.inventory.len() <= to {
                        snapshot.inventory.resize(to + 1, None);
                    }
                    snapshot.inventory[to] = Some(item);
                }
            }
        }
        2 => {
            let item = change
                .item
                .as_ref()
                .map(|stored| InventoryItem::from_user_item(&stored.item));
            if let Some(from) = from {
                if let Some(slot) = snapshot.guild_storage.get_mut(from) {
                    *slot = None;
                }
            }
            if to < GUILD_STORAGE_SLOTS {
                if snapshot.guild_storage.len() < GUILD_STORAGE_SLOTS {
                    snapshot.guild_storage.resize(GUILD_STORAGE_SLOTS, None);
                }
                snapshot.guild_storage[to] = item;
            }
        }
        _ => {}
    }
}

/// 应用一条 `S.GuildBuffList`（Crystal `GameScene.cs:6249-6285`）。
///
/// `GuildBuffs` 非空 = 全量可选列表（登录/请求时下发）；`remove == 1` 表示
/// `ActiveBuffs` 里的条目是「移除」（到期/被清掉）。
fn apply_guild_buff_list(snapshot: &mut WorldSnapshot, list: &GuildBuffList) {
    if !list.guild_buffs.is_empty() {
        snapshot.guild_buff_infos = list
            .guild_buffs
            .iter()
            .map(GuildBuffInfoView::from_guild_buff_info)
            .collect();
    }
    let remove = list.remove == 1;
    for buff in &list.active_buffs {
        let view = GuildBuffView::from_guild_buff(buff);
        if remove {
            snapshot
                .guild_active_buffs
                .retain(|existing| existing.id != view.id);
        } else if let Some(existing) = snapshot
            .guild_active_buffs
            .iter_mut()
            .find(|existing| existing.id == view.id)
        {
            *existing = view;
        } else {
            snapshot.guild_active_buffs.push(view);
        }
    }
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
            me.looks = temporary.looks;
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
    use mir_protocol::data::{
        ClientBuff, ClientMagic, ClientMail, ItemInfo, SelectInfo, Stats, UserItem,
    };
    use mir_protocol::packets::server::{
        AddBuff, DamageIndicator, DeleteItem, EquipItem, GuildStatus, MountUpdate, MoveItem,
        NPCGoods, NPCResponse, NPCStorage, NewItemInfo, ObjectEffect, ObjectItem, ObjectMonster,
        ObjectNPC, ObjectPlayer, ObjectStruck, ObjectWalk, PlayerUpdate, RemoveBuff, RemoveItem,
        UserInformation, UserLocation, UserStorage,
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
            wing_effect: 3,
            mount_type: 2,
            riding_mount: true,
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
        assert_eq!(view.wings, 3, "S.ObjectPlayer.wing_effect");
        assert_eq!(view.mount, 3, "mount_type 2 + 1（0 = 没有坐骑）");
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

    #[test]
    fn mount_update_sets_and_clears_mount_for_self_and_others() {
        let mut snapshot = WorldSnapshot::default();
        snapshot.apply_packet(&ServerPacket::UserInformation(UserInformation {
            object_id: 7,
            name: "自己".to_owned(),
            location: Point::new(5, 5),
            ..UserInformation::default()
        }));
        snapshot.apply_packet(&ServerPacket::MountUpdate(MountUpdate {
            object_id: 7,
            mount_type: 4,
            riding_mount: true,
        }));
        assert_eq!(
            snapshot.self_player.as_ref().expect("自己").looks.mount,
            5,
            "自己的坐骑来自 S.MountUpdate"
        );
        // 重新算纸娃娃（任意包都会触发）不能把坐骑丢掉。
        snapshot.apply_packet(&ServerPacket::HealthChanged(
            mir_protocol::packets::server::HealthChanged { hp: 9, mp: 9 },
        ));
        assert_eq!(snapshot.self_player.as_ref().expect("自己").looks.mount, 5);
        let view = snapshot
            .entity_views(0)
            .into_iter()
            .next()
            .expect("自己的视图");
        assert_eq!(view.mount, 5);

        snapshot.apply_packet(&ServerPacket::MountUpdate(MountUpdate {
            object_id: 7,
            mount_type: -1,
            riding_mount: false,
        }));
        assert_eq!(snapshot.self_player.as_ref().expect("自己").looks.mount, 0);

        // 其他玩家：ObjectPlayer 给出坐骑，MountUpdate 可以下马。
        snapshot.apply_packet(&ServerPacket::ObjectPlayer(ObjectPlayer {
            object_id: 8,
            name: "路人".to_owned(),
            location: Point::new(6, 5),
            mount_type: 1,
            riding_mount: true,
            ..ObjectPlayer::default()
        }));
        assert_eq!(snapshot.entities[&8].looks.mount, 2);
        snapshot.apply_packet(&ServerPacket::MountUpdate(MountUpdate {
            object_id: 8,
            mount_type: -1,
            riding_mount: false,
        }));
        assert_eq!(snapshot.entities[&8].looks.mount, 0);
    }

    #[test]
    fn map_packets_keep_the_music_index() {
        let mut snapshot = WorldSnapshot::default();
        snapshot.apply_packet(&ServerPacket::MapInformation(
            mir_protocol::packets::server::MapInformation {
                map_index: 1,
                file_name: "0".to_owned(),
                title: "比奇省".to_owned(),
                music: 30_001,
                ..mir_protocol::packets::server::MapInformation::default()
            },
        ));
        assert_eq!(snapshot.map.as_ref().expect("地图").music, 30_001);
        snapshot.apply_packet(&ServerPacket::MapChanged(
            mir_protocol::packets::server::MapChanged {
                map_index: 2,
                file_name: "D2083".to_owned(),
                title: "洞穴".to_owned(),
                music: 30_101,
                ..mir_protocol::packets::server::MapChanged::default()
            },
        ));
        assert_eq!(snapshot.map.as_ref().expect("地图").music, 30_101);
    }

    // -----------------------------------------------------------------------
    // 任务 / 仓库 / 邮件 / 行会快照
    // -----------------------------------------------------------------------

    fn quest_info(index: i32, name: &str, limit: i32) -> mir_protocol::data::ClientQuestInfo {
        mir_protocol::data::ClientQuestInfo {
            index,
            name: name.to_owned(),
            group: "新手村".to_owned(),
            time_limit_in_seconds: limit,
            ..mir_protocol::data::ClientQuestInfo::default()
        }
    }

    fn quest_change(
        id: i32,
        task_list: &[&str],
        completed: bool,
        state: QuestState,
    ) -> ServerPacket {
        ServerPacket::ChangeQuest(mir_protocol::packets::server::ChangeQuest {
            quest: ClientQuestProgress {
                id,
                task_list: task_list.iter().map(|line| (*line).to_owned()).collect(),
                taken: true,
                completed,
                new: false,
            },
            quest_state: state,
            track_quest: false,
        })
    }

    #[test]
    fn quest_packets_track_definitions_progress_removal_and_share_chat() {
        let mut snapshot = WorldSnapshot::default();
        snapshot.apply_packet(&ServerPacket::NewQuestInfo(
            mir_protocol::packets::server::NewQuestInfo {
                info: quest_info(5, "猎鹿", 600),
            },
        ));
        assert_eq!(
            snapshot.quest_infos.get(&5).map(|info| info.name.as_str()),
            Some("猎鹿")
        );

        snapshot.apply_packet_at(
            &quest_change(5, &["击杀 鹿 3/10"], false, QuestState::Add),
            1_000,
        );
        let quest = snapshot.quest(5).expect("进行中任务");
        assert_eq!(quest.name, "猎鹿");
        assert_eq!(quest.group, "新手村");
        assert_eq!(quest.time_limit_seconds, 600);
        assert_eq!(quest.remaining_seconds(61_000), Some(540));
        assert!(!quest.ready_to_return());

        snapshot.apply_packet_at(
            &quest_change(5, &["击杀 鹿 10/10"], true, QuestState::Update),
            90_000,
        );
        let quest = snapshot.quest(5).expect("进度更新");
        assert!(quest.ready_to_return());
        assert_eq!(quest.accepted_at_ms, Some(1_000), "Update 不重置接取时刻");

        snapshot.apply_packet(&ServerPacket::CompleteQuest(
            mir_protocol::packets::server::CompleteQuest {
                completed_quests: vec![5],
            },
        ));
        assert_eq!(snapshot.completed_quests, vec![5]);

        snapshot.apply_packet(&ServerPacket::ShareQuest(
            mir_protocol::packets::server::ShareQuest {
                quest_index: 5,
                sharer_name: "队长".to_owned(),
            },
        ));
        assert!(
            snapshot
                .chat
                .last()
                .is_some_and(|line| line.text.contains("猎鹿") && line.text.contains("队长"))
        );

        snapshot.apply_packet(&quest_change(5, &[], false, QuestState::Remove));
        assert!(snapshot.quests.is_empty(), "Remove 应移除进行中任务");
        assert_eq!(snapshot.packet_count("ChangeQuest"), 3);
    }

    fn guild_member(name: &str, online: bool) -> GuildMember {
        GuildMember {
            name: name.to_owned(),
            online,
            ..GuildMember::default()
        }
    }

    fn guild_rank(index: i32, name: &str, members: Vec<GuildMember>) -> GuildRank {
        GuildRank {
            name: name.to_owned(),
            index,
            options: 0,
            members,
        }
    }

    fn member_change(
        name: &str,
        status: u8,
        rank_index: u8,
        ranks: Vec<GuildRank>,
    ) -> ServerPacket {
        ServerPacket::GuildMemberChange(GuildMemberChange {
            name: name.to_owned(),
            rank_index,
            status,
            ranks,
        })
    }

    fn guild_status(options: u8) -> ServerPacket {
        ServerPacket::GuildStatus(GuildStatus {
            guild_name: "沙巴克".to_owned(),
            guild_rank_name: "成员".to_owned(),
            level: 3,
            gold: 5_000,
            member_count: 2,
            max_members: 20,
            my_options: mir_core::enums::GuildRankOptions(options),
            my_rank_id: 1,
            ..GuildStatus::default()
        })
    }

    #[test]
    fn guild_member_status_codes_rebuild_the_rank_table() {
        let mut snapshot = WorldSnapshot::default();
        snapshot.apply_packet(&guild_status(255));
        assert_eq!(snapshot.guild.as_ref().map(|guild| guild.level), Some(3));
        snapshot.apply_packet(&member_change(
            "会长",
            255,
            0,
            vec![
                guild_rank(0, "会长", vec![guild_member("会长", true)]),
                guild_rank(1, "成员", vec![guild_member("小明", false)]),
            ],
        ));
        assert_eq!(snapshot.guild_ranks.len(), 2);
        assert_eq!(snapshot.guild_members().len(), 2);
        assert!(!snapshot.guild_members()[1].1.online);

        // 1 = 上线。
        snapshot.apply_packet(&member_change("小明", 1, 1, Vec::new()));
        assert!(snapshot.guild_members()[1].1.online);
        assert!(
            snapshot
                .chat
                .iter()
                .any(|line| line.text.contains("小明 上线了"))
        );

        // 5 = 被挪到 0 号职位。
        snapshot.apply_packet(&member_change("小明", 5, 0, Vec::new()));
        assert_eq!(snapshot.guild_ranks[0].members.len(), 2);
        assert!(snapshot.guild_ranks[1].members.is_empty());

        // 2 = 新成员。
        snapshot.apply_packet(&member_change("新人", 2, 1, Vec::new()));
        assert_eq!(snapshot.guild_ranks[1].members.len(), 1);
        assert_eq!(
            snapshot.guild.as_ref().map(|guild| guild.member_count),
            Some(3),
            "新成员会让人数 +1"
        );

        // 3 = 被踢；4 = 主动退会。
        snapshot.apply_packet(&member_change("新人", 3, 1, Vec::new()));
        assert!(snapshot.guild_ranks[1].members.is_empty());
        snapshot.apply_packet(&member_change("小明", 4, 0, Vec::new()));
        assert_eq!(snapshot.guild_ranks[0].members.len(), 1);

        // 8 = 我的职位变了：`ranks[0]` 是新的职位快照。
        snapshot.apply_packet(&member_change(
            "会长",
            8,
            0,
            vec![GuildRank {
                name: "副会长".to_owned(),
                options: 64,
                index: 0,
                members: vec![guild_member("会长", true)],
            }],
        ));
        assert_rank(&snapshot, "副会长", 64);

        // 空名字的 GuildStatus = 没有行会（清空成员/仓库/增益）。
        snapshot.apply_packet(&ServerPacket::GuildStatus(GuildStatus::default()));
        assert!(snapshot.guild_ranks.is_empty());
        assert!(snapshot.guild_notice.is_empty());
        assert!(snapshot.guild_storage.is_empty());
    }

    fn assert_rank(snapshot: &WorldSnapshot, name: &str, options: u8) {
        let guild = snapshot.guild.as_ref().expect("行会");
        assert_eq!(guild.rank_name, name);
        assert_eq!(guild.my_options.value(), options);
    }

    #[test]
    fn guild_storage_gold_and_buff_packets_update_the_snapshot() {
        let mut snapshot = WorldSnapshot::default();
        snapshot.apply_packet(&guild_status(255));
        snapshot.apply_packet(&ServerPacket::GuildStorageList(
            mir_protocol::packets::server::GuildStorageList {
                items: vec![Some(GuildStorageItem {
                    item: test_item(41, 3, 9),
                    user_id: 1,
                })],
            },
        ));
        assert_eq!(snapshot.guild_storage.len(), GUILD_STORAGE_SLOTS);
        assert_eq!(
            snapshot.guild_storage_item(0).map(|item| item.unique_id),
            Some(41)
        );

        // 0 = 我存入：仓库格出现物品，背包对应格清空。
        let mut inventory = vec![None; 4];
        inventory[1] = Some(test_item(11, 1, 1));
        snapshot.apply_packet(&ServerPacket::UserInformation(UserInformation {
            inventory: Some(inventory),
            ..user_information()
        }));
        snapshot.apply_packet(&ServerPacket::GuildStorageItemChange(
            mir_protocol::packets::server::GuildStorageItemChange {
                user: 1,
                change_type: 0,
                to: 2,
                from: 1,
                item: Some(GuildStorageItem {
                    item: test_item(11, 1, 1),
                    user_id: 1,
                }),
            },
        ));
        assert!(snapshot.inventory_item(1).is_none());
        assert_eq!(
            snapshot.guild_storage_item(2).map(|item| item.unique_id),
            Some(11)
        );

        // 1 = 我取回：仓库格清空，物品出现在背包。
        snapshot.apply_packet(&ServerPacket::GuildStorageItemChange(
            mir_protocol::packets::server::GuildStorageItemChange {
                user: 1,
                change_type: 1,
                to: 3,
                from: 2,
                item: None,
            },
        ));
        assert!(snapshot.guild_storage_item(2).is_none());
        assert_eq!(
            snapshot.inventory_item(3).map(|item| item.unique_id),
            Some(11)
        );

        // 资金：0 存入、1 取出、2 买增益、3 退还。
        for (type_, expected) in [(0u8, 5_500u32), (1, 5_000), (2, 4_500), (3, 5_000)] {
            snapshot.apply_packet(&ServerPacket::GuildStorageGoldChange(
                mir_protocol::packets::server::GuildStorageGoldChange {
                    amount: 500,
                    type_,
                    name: "测试者".to_owned(),
                },
            ));
            assert_eq!(
                snapshot.guild.as_ref().map(|guild| guild.gold),
                Some(expected),
                "type = {type_}"
            );
        }

        // 增益：定义 + 获得 + 激活 + 移除。
        snapshot.apply_packet(&ServerPacket::GuildBuffList(GuildBuffList {
            remove: 0,
            active_buffs: Vec::new(),
            guild_buffs: vec![GuildBuffInfo {
                id: 3,
                name: "公会之力".to_owned(),
                time_limit: 60,
                activation_cost: 500,
                ..GuildBuffInfo::default()
            }],
        }));
        assert_eq!(snapshot.guild_buff_infos.len(), 1);
        snapshot.apply_packet(&ServerPacket::GuildBuffList(GuildBuffList {
            remove: 0,
            active_buffs: vec![GuildBuff {
                id: 3,
                active: true,
                active_time_remaining: 59,
            }],
            guild_buffs: Vec::new(),
        }));
        assert_eq!(
            snapshot
                .guild_active_buff(3)
                .map(|buff| buff.active_time_remaining_minutes),
            Some(59)
        );
        snapshot.apply_packet(&ServerPacket::GuildBuffList(GuildBuffList {
            remove: 1,
            active_buffs: vec![GuildBuff {
                id: 3,
                active: false,
                active_time_remaining: 0,
            }],
            guild_buffs: Vec::new(),
        }));
        assert!(snapshot.guild_active_buff(3).is_none());
    }

    #[test]
    fn mail_packets_track_list_flags_and_send_results() {
        let mut snapshot = WorldSnapshot::default();
        snapshot.apply_packet(&ServerPacket::ReceiveMail(
            mir_protocol::packets::server::ReceiveMail {
                mail: vec![ClientMail {
                    mail_id: 9,
                    sender_name: "张三".to_owned(),
                    message: "你好\n附件".to_owned(),
                    opened: false,
                    locked: false,
                    can_reply: true,
                    collected: false,
                    gold: 100,
                    items: vec![test_item(31, 1, 1)],
                    ..ClientMail::default()
                }],
            },
        ));
        let mail = snapshot.mail(9).expect("邮件");
        assert!(mail.can_collect());
        assert!(mail.is_parcel());
        assert_eq!(mail.subject_line(), "你好");
        assert_eq!(mail.items.len(), 1);

        snapshot.apply_packet(&ServerPacket::MailSendRequest(
            mir_protocol::packets::server::MailSendRequest {},
        ));
        assert!(snapshot.mail_send_request);
        assert_eq!(snapshot.mail_send_request_version, 1);
        snapshot.apply_packet(&ServerPacket::MailCost(
            mir_protocol::packets::server::MailCost { cost: 250 },
        ));
        assert_eq!(snapshot.mail_cost, Some(250));
        snapshot.apply_packet(&ServerPacket::MailSent(
            mir_protocol::packets::server::MailSent { result: 0 },
        ));
        assert_eq!(snapshot.mail_sent_result, Some(0));
        snapshot.apply_packet(&ServerPacket::ParcelCollected(
            mir_protocol::packets::server::ParcelCollected { result: 1 },
        ));
        assert_eq!(snapshot.parcel_collected_result, Some(1));
        snapshot.apply_packet(&ServerPacket::MailLockedItem(
            mir_protocol::packets::server::MailLockedItem {
                unique_id: 31,
                locked: true,
            },
        ));
        assert_eq!(snapshot.mail_locked_items.get(&31), Some(&true));
    }

    #[test]
    fn storage_packets_fill_eighty_slots_and_move_items() {
        let mut snapshot = WorldSnapshot::default();
        snapshot.apply_packet(&ServerPacket::UserStorage(UserStorage {
            storage: Some(vec![Some(test_item(21, 2, 3))]),
        }));
        assert_eq!(snapshot.storage.len(), STORAGE_SLOTS);
        assert_eq!(
            snapshot.storage_item(0).map(|item| item.unique_id),
            Some(21)
        );
        assert!(!snapshot.storage_open);
        snapshot.apply_packet(&ServerPacket::NPCStorage(NPCStorage {}));
        assert!(snapshot.storage_open);
        assert_eq!(snapshot.storage_open_version, 1);

        let mut inventory = vec![None; 3];
        inventory[0] = Some(test_item(11, 1, 1));
        snapshot.apply_packet(&ServerPacket::UserInformation(UserInformation {
            inventory: Some(inventory),
            ..user_information()
        }));
        snapshot.apply_packet(&ServerPacket::StoreItem(
            mir_protocol::packets::server::StoreItem {
                from: 0,
                to: 5,
                success: true,
            },
        ));
        assert!(snapshot.inventory_item(0).is_none());
        assert_eq!(
            snapshot.storage_item(5).map(|item| item.unique_id),
            Some(11)
        );
        snapshot.apply_packet(&ServerPacket::TakeBackItem(
            mir_protocol::packets::server::TakeBackItem {
                from: 0,
                to: 1,
                success: true,
            },
        ));
        assert!(snapshot.storage_item(0).is_none());
        assert_eq!(
            snapshot.inventory_item(1).map(|item| item.unique_id),
            Some(21)
        );

        // 失败回执不动状态。
        snapshot.apply_packet(&ServerPacket::StoreItem(
            mir_protocol::packets::server::StoreItem {
                from: 0,
                to: 7,
                success: false,
            },
        ));
        assert!(snapshot.storage_item(7).is_none());

        // 空 NPC 页会关掉仓库面板。
        snapshot.apply_packet(&ServerPacket::NPCResponse(NPCResponse { page: Vec::new() }));
        assert!(!snapshot.storage_open);
    }
}
