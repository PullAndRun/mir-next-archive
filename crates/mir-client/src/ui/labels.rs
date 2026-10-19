//! 界面文案表：中文优先，`--lang en` 切到英文。
//!
//! 只有几十个固定标签，用穷举 `match` 比加载语言包更省事，也不会漏翻。玩家/物品/地图
//! 之类的动态文本来自服务端，不在这里。

use mir_core::Language;
use mir_core::enums::{
    BuffType, ChatType, EquipmentSlot, ItemGrade, ItemType, LightSetting, PetMode, RequiredType,
    WeatherSetting,
};

use crate::net::{ChatLine, NetStatus, SystemChat};

/// 界面固定标签。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Label {
    /// 生命。
    Hp,
    /// 魔法。
    Mp,
    /// 金币。
    Gold,
    /// 地图。
    Map,
    /// 聊天。
    Chat,
    /// 背包。
    Inventory,
    /// 商店。
    Shop,
    /// 对话。
    Dialog,
    /// 技能。
    Skills,
    /// 关闭。
    Close,
    /// 出售。
    Sell,
    /// 修理。
    Repair,
    /// 丢弃。
    Drop,
    /// 等级。
    Level,
    /// 价格。
    Price,
    /// 倍率。
    Rate,
    /// 空。
    Empty,
    /// 尚未学习技能。
    NoSkills,
    /// 按 T 聊天。
    ChatHint,
    /// S 出售 · R 修理。
    ShopHint,
    /// 左键选中 · D 丢弃。
    InventoryHint,
    /// 未知物品。
    UnknownItem,
    /// 未知地图。
    UnknownMap,
    /// 可出售。
    SellAllowed,
    /// 可修理。
    RepairAllowed,
    /// 自定义键提示（NPC 对话）。
    NpcHint,
    /// 连接中。
    Connecting,
    /// 已断开。
    Disconnected,
    /// 施法键提示。
    CastHint,
    /// 装备面板标题。
    Equipment,
    /// 装备（动词）。
    Equip,
    /// 使用。
    Use,
    /// 类型。
    Type,
    /// 需求。
    Require,
    /// 持久度。
    Durability,
    /// 堆叠。
    Stack,
    /// 攻击（DC）。
    Attack,
    /// 防御（AC）。
    Defence,
    /// 魔防（MAC）。
    MagicDefence,
    /// 魔法（MC）。
    MagicAttack,
    /// 道术（SC）。
    Taoism,
    /// 准确。
    Accuracy,
    /// 敏捷。
    Agility,
    /// 攻击速度。
    AttackSpeed,
    /// 幸运。
    Luck,
    /// 属性汇总。
    Totals,
    /// 增益栏。
    Buffs,
    /// 秒。
    Seconds,
    /// 永久。
    Infinite,
    /// 暂停。
    Paused,
    /// 闪避。
    Miss,
    /// 暴击。
    Critical,
    /// 世界点击走路提示。
    WorldHint,
    /// 背包交互提示（双击/右键）。
    InventoryTooltipHint,
    /// 小地图面板标题。
    Minimap,
    /// 没有小地图素材时的占位文字。
    NoMinimap,
    /// 小地图面板底部的按键提示。
    MinimapHint,
    /// 没有大地图素材时的占位文字。
    NoBigmap,
    /// 大地图面板底部的按键提示。
    BigmapHint,
    /// 任务面板标题。
    Quests,
    /// 任务日志标题（Crystal `QuestDiaryDialog`）。
    QuestDiary,
    /// 任务面板底部提示。
    QuestHint,
    /// 任务状态「进行中」。
    QuestInProgress,
    /// 任务状态「已完成 / 可交付」。
    QuestComplete,
    /// 任务状态「可接取」。
    QuestAvailable,
    /// 放弃任务按钮。
    Abandon,
    /// 任务限时剩余。
    TimeLeft,
    /// 仓库面板标题。
    Storage,
    /// 仓库容量（`已用/总数`）。
    Capacity,
    /// 仓库面板底部提示。
    StorageHint,
    /// 邮件面板标题。
    Mail,
    /// 收件箱。
    Inbox,
    /// 写信。
    Compose,
    /// 发件人列。
    Sender,
    /// 主题列。
    Subject,
    /// 正文。
    Message,
    /// 日期列。
    Date,
    /// 附件。
    Attachment,
    /// 收取附件。
    Collect,
    /// 锁定。
    Lock,
    /// 解锁。
    Unlock,
    /// 删除。
    Delete,
    /// 发送。
    Send,
    /// 收件人。
    Recipient,
    /// 邮费。
    Postage,
    /// 询价。
    Inquiry,
    /// 贴邮票。
    Stamp,
    /// 返回。
    Back,
    /// 没有邮件。
    NoMail,
    /// 邮件面板底部提示。
    MailHint,
    /// 行会面板标题。
    Guild,
    /// 行会成员页。
    Members,
    /// 行会仓库页。
    GuildStorageTab,
    /// 行会增益页。
    GuildBuffsTab,
    /// 行会联盟页。
    GuildAllyTab,
    /// 盟友行会名（输入框）。
    GuildAllyName,
    /// 申请结盟。
    GuildAllyRequest,
    /// 同意结盟。
    GuildAllyAccept,
    /// 解除联盟。
    GuildAllyRemove,
    /// 暂无盟友。
    GuildAllyEmpty,
    /// 申请中。
    GuildAllyPending,
    /// 联盟操作提示。
    GuildAllyHint,
    /// 好友面板标题。
    Friends,
    /// 好友名字输入框。
    FriendName,
    /// 好友备注。
    FriendMemo,
    /// 黑名单标记。
    FriendBlocked,
    /// 好友列表为空。
    FriendEmpty,
    /// 添加好友。
    FriendAdd,
    /// 删除好友。
    FriendRemove,
    /// 保存备注。
    FriendSaveMemo,
    /// 刷新好友列表。
    FriendRefresh,
    /// 好友面板操作提示。
    FriendHint,
    /// 攻城进行中。
    ConquestWar,
    /// 剩余时间。
    ConquestRemaining,
    /// 当前归属。
    ConquestOwner,
    /// 控制点。
    ConquestControlPoints,
    /// 攻城结算。
    ConquestResult,
    /// 胜者。
    ConquestWinner,
    /// 城门。
    ConquestGate,
    /// 城墙。
    ConquestWall,
    /// 箭塔。
    ConquestArcher,
    /// 被摧毁。
    ConquestDestroyed,
    /// 城门已开。
    ConquestGateOpen,
    /// 盟友行会会长。
    GuildAllyLeader,
    /// 断线重连提示（参数：第几次/总次数）。
    Reconnecting,
    /// 小地图传送点悬停提示（参数：目标地图、坐标）。
    MinimapGateTip,
    /// 无归属。
    ConquestUnclaimed,
    /// 系统：攻城开始。
    SystemConquestStarted,
    /// 系统：攻城结束。
    SystemConquestEnded,
    /// 系统：攻城器械被摧毁。
    SystemConquestObjectDestroyed,
    /// 行会公告页。
    Notice,
    /// 行会资金。
    GuildFunds,
    /// 职位列。
    Rank,
    /// 在线。
    Online,
    /// 离线。
    Offline,
    /// 地图列。
    MapColumn,
    /// 升职。
    Promote,
    /// 降职。
    Demote,
    /// 踢人。
    Kick,
    /// 保存。
    Save,
    /// 激活增益。
    Activate,
    /// 购买增益。
    Purchase,
    /// 存入金币。
    Deposit,
    /// 取出金币。
    Withdraw,
    /// 金额输入。
    Amount,
    /// 未加入行会。
    NoGuild,
    /// 行会面板底部提示。
    GuildHint,
    /// 可用（增益状态）。
    Available,
    /// 已获得（增益状态）。
    Obtained,
    /// 已过期（增益状态）。
    Expired,
    /// 剩余时长（分钟）。
    MinutesLeft,
    /// 没有进行中的任务。
    NoQuests,
    /// 升级点（行会增益需求）。
    Points,
    /// 未读（邮件）。
    Unread,
    /// 已领取（邮件附件）。
    Collected,
    /// 邮件没有正文时的占位主题。
    NoBody,
    /// 升级（装备面板的「升级」按钮）。
    Upgrade,
    /// 写信页主题框的占位提示（协议没有主题字段）。
    NoSubjectField,
    /// 交易。
    Trade,
    /// 交易邀请文案后半句（「张三 请求与你交易」）。
    TradeRequestSuffix,
    /// 交易面板：自己这一栏的标题。
    TradeYourSide,
    /// 交易面板：对方金币行。
    TradePartnerGold,
    /// 「准备」。
    TradeReady,
    /// 「确认」。
    TradeConfirm,
    /// 「取消交易」。
    TradeCancel,
    /// 「放入金币」。
    TradeGoldAdd,
    /// 交易面板底部提示。
    TradeHint,
    /// 「未准备」。
    NotReady,
    /// 「已准备」。
    Ready,
    /// 交易面板：对方状态行前缀。
    TradePartnerState,
    /// 「接受」。
    Accept,
    /// 「拒绝」。
    Reject,
    /// 组队。
    Group,
    /// 组队邀请文案后半句（「张三 邀请你组队」）。
    GroupInviteSuffix,
    /// 「邀请」。
    Invite,
    /// 「离队」。
    Leave,
    /// 组队面板：邀请名字输入框的占位。
    InviteName,
    /// 「队长」。
    GroupLeader,
    /// 「职业」。
    Profession,
    /// 组队面板底部提示。
    GroupHint,
    /// 组队聊天的 `!!` 前缀提示。
    GroupChatHint,
    /// 「允许组队」开关。
    GroupAllow,
    /// 「开」。
    ToggleOn,
    /// 「关」。
    ToggleOff,
    /// 「未组队」。
    NoGroup,
    /// 「血量」（组队面板列头）。
    Health,
    /// 组队面板：踢人按钮。
    GroupKick,
    /// 市场面板标题。
    Market,
    /// 物品列（市场列表）。
    ItemColumn,
    /// 寄售页签。
    Consign,
    /// 拍卖页签。
    Auction,
    /// 搜索。
    Search,
    /// 刷新。
    Refresh,
    /// 卖家列。
    Seller,
    /// 到期列。
    Expiry,
    /// 买下（一口价）。
    Buy,
    /// 出价。
    Bid,
    /// 出价金额（输入框占位）。
    BidAmount,
    /// 寄售售价（输入框占位）。
    ConsignPrice,
    /// 拍卖起拍价（输入框占位）。
    StartingBid,
    /// 上架。
    ListItem,
    /// 取回。
    GetBack,
    /// 提前成交。
    SellNow,
    /// 一键收款。
    CollectSold,
    /// 上一页。
    PrevPage,
    /// 下一页。
    NextPage,
    /// 没有市场条目。
    NoListings,
    /// 市场面板底部提示。
    MarketHint,
    /// 寄售面板底部提示。
    ConsignHint,
    /// 拍卖面板底部提示。
    AuctionHint,
    /// 市场操作失败（未知原因）。
    MarketFailGeneric,
    /// 失败原因：角色死亡。
    MarketFailDead,
    /// 失败原因：不在市场 NPC 旁。
    MarketFailNoDialog,
    /// 失败原因：条目已售出。
    MarketFailSold,
    /// 失败原因：条目已过期。
    MarketFailExpired,
    /// 失败原因：背包空间不足。
    MarketFailNoSpace,
    /// 失败原因：不能购买自己的条目。
    MarketFailOwnItem,
    /// 失败原因：金币不足。
    MarketFailLowGold,
    /// 失败原因：条目不可用或距离太远。
    MarketFailTooFar,
    /// 失败原因：金币已达上限。
    MarketFailCannotGainGold,
    /// 失败原因：出价太低。
    MarketFailBidTooLow,
    /// 失败原因：拍卖已结束。
    MarketFailAuctionEnded,
    /// 上架成功。
    ConsignSuccess,
    /// 上架失败。
    ConsignFailed,
    /// 背包里没有选中物品。
    NoSelectedItem,
    /// 寄售表单里「上架物品：」前缀。
    ConsignSelected,
    /// 商城页签（Crystal `MarketPanelType.GameShop`）。
    GameShop,
    /// 元宝（商城价格单位；Crystal `ClientTextKeys.Credits`）。
    Credit,
    /// 库存列。
    Stock,
    /// 有货（商城列表的库存列文案：售罄条目由服务端直接剔除）。
    OnSale,
    /// 购买数量（输入框占位）。
    Quantity,
    /// 商城页签底部提示。
    GameShopHint,
    /// 商城操作行提示（选中商品后点购买）。
    GameShopPickHint,
    /// 商城没有商品。
    NoGameShopItems,
    /// 商品分类列。
    Category,
    /// 设置面板标题（Crystal `OptionDialog`，`KeyBindSettings.cs:221`）。
    Settings,
    /// 设置面板：音频分组。
    Audio,
    /// 设置面板：背景音乐。
    Music,
    /// 设置面板：音效。
    Sound,
    /// 设置面板：音量。
    Volume,
    /// 设置面板：语言。
    Language,
    /// 设置面板：显示。
    Display,
    /// 设置面板：分辨率。
    Resolution,
    /// 设置面板：缩放。
    UiScale,
    /// 设置面板：按键说明。
    KeyBindings,
    /// 设置面板底部提示。
    SettingsHint,
    /// 登录界面标题。
    Login,
    /// 服务器。
    Server,
    /// 服务器地址。
    ServerAddress,
    /// 账号。
    Account,
    /// 密码。
    Password,
    /// 连接按钮。
    Connect,
    /// 重试按钮。
    Retry,
    /// 选择角色。
    SelectCharacter,
    /// 创建角色按钮。
    CreateCharacter,
    /// 进入游戏。
    EnterGame,
    /// 角色名。
    CharacterName,
    /// 性别。
    Gender,
    /// 男。
    Male,
    /// 女。
    Female,
    /// 账号下没有角色时的引导。
    NoCharacters,
    /// 连接失败标题。
    ConnectionFailed,
    /// 上次登录时间。
    LastAccess,
    /// 从未登录。
    Never,
    /// 退出。
    Quit,
    /// 登录界面底部提示。
    LoginHint,
    /// 选角界面底部提示。
    CharSelectHint,
    /// 没有装备（角色选择界面的装备摘要占位）。
    NoEquipmentSummary,
    /// 选角界面：返回服务器选择。
    BackToServer,
    /// 设置面板按键表：大地图。
    Bigmap,
    /// 设置面板按键表：走路。
    KeyMove,
    /// 设置面板按键表：跑步。
    KeyRun,
    /// 设置面板按键表：攻击。
    KeyAttack,
    /// 设置面板按键表：拾取。
    KeyPickUp,
    /// 设置面板按键表：聊天输入。
    KeyChat,
    /// 设置面板按键表：关闭面板。
    KeyEscape,
    /// 窗口标题（离线地图）。
    WindowTitleMap,
    /// 窗口标题（登录窗口）。
    WindowTitleLogin,
    /// 窗口标题（联机世界）。
    WindowTitleLive,
    /// 连接状态：版本校验。
    StatusVerifying,
    /// 连接状态：登录中。
    StatusLoggingIn,
    /// 连接状态：选择角色。
    StatusSelectingCharacter,
    /// 连接状态：进入世界。
    StatusStartingGame,
    /// 连接状态：已进入世界。
    StatusInGame,
    /// 连接状态：失败。
    StatusFailed,
    /// 登录窗口详情：网络线程已退出。
    NetworkThreadExited,
    /// 登录窗口详情：登录线程已退出。
    LoginThreadExited,
    /// 客户端系统消息：获得金币。
    SystemGoldGained,
    /// 客户端系统消息：获得物品。
    SystemItemGained,
    /// 客户端系统消息：分享任务。
    SystemQuestShared,
    /// 客户端系统消息：向行会捐献金币。
    SystemGuildGoldDeposit,
    /// 客户端系统消息：从行会取出金币。
    SystemGuildGoldWithdraw,
    /// 客户端系统消息：交易完成。
    SystemTradeComplete,
    /// 客户端系统消息：交易已取消。
    SystemTradeCancelled,
    /// 客户端系统消息：自己离开队伍。
    SystemGroupLeft,
    /// 客户端系统消息：队员加入。
    SystemGroupJoined,
    /// 客户端系统消息：队员退出。
    SystemGroupMemberLeft,
    /// 客户端系统消息：行会成员上线。
    SystemGuildMemberOnline,
    /// 客户端系统消息：行会新成员。
    SystemGuildMemberJoined,
    /// 客户端系统消息：行会成员被移出。
    SystemGuildMemberKicked,
    /// 客户端系统消息：行会成员离开。
    SystemGuildMemberLeft,
    /// 任务名缺失时的占位（`任务 #12`）。
    QuestFallback,
    /// 连接失败：连不上服务器。
    ConnectUnreachable,
    /// 连接失败：账号或密码错误。
    ConnectCredentials,
    /// 连接失败：服务器拒绝连接。
    ConnectRefused,
    /// 连接失败：超时。
    ConnectTimeout,
    /// 连接失败：客户端/服务端版本不匹配。
    ConnectVersionMismatch,
    /// 连接失败：其它错误。
    ConnectFailed,
    /// 任务面板的「任务物品」区标题。
    QuestItems,
    /// 昼夜：黎明（`S.TimeOfDay` 的 `LightSetting::Dawn`）。
    PhaseDawn,
    /// 昼夜：白天（`Normal` / `Day`）。
    PhaseDay,
    /// 昼夜：黄昏。
    PhaseEvening,
    /// 昼夜：夜晚。
    PhaseNight,
    /// 天气：晴。
    WeatherClear,
    /// 天气：雾。
    WeatherFog,
    /// 天气：余烬。
    WeatherEmber,
    /// 天气：火星。
    WeatherFire,
    /// 天气：雪。
    WeatherSnow,
    /// 天气：雨。
    WeatherRain,
    /// 天气：落叶。
    WeatherLeaves,
    /// 客户端系统消息：获得任务物品。
    SystemQuestItemGained,
    /// 客户端系统消息：失去任务物品。
    SystemQuestItemLost,
    /// 宠物模式：跟随。
    PetModeFollow,
    /// 宠物模式：攻击。
    PetModeAttack,
    /// 宠物模式：休息。
    PetModeRest,
    /// 宠物模式切换提示（`宠物模式：跟随`）。
    PetModeChanged,
    /// 没有宠物时的提示。
    NoPetHint,
    /// 任务物品区翻页提示。
    QuestItemsScrollHint,
    /// 设置面板按键表：宠物模式。
    PetMode,
}

impl Label {
    /// 全部标签（枚举顺序）。
    ///
    /// 新增 `Label` 变体时**必须**同步这个数组：`Label::text` 的穷举 `match` 只保证
    /// 「每个变体都有分支」，非空/中英不同只能靠 `every_label_has_both_languages`
    /// 遍历这份列表兜住，数量不符会直接让测试失败。
    pub const ALL: &[Label] = &[
        Self::Hp,
        Self::Mp,
        Self::Gold,
        Self::Map,
        Self::Chat,
        Self::Inventory,
        Self::Shop,
        Self::Dialog,
        Self::Skills,
        Self::Close,
        Self::Sell,
        Self::Repair,
        Self::Drop,
        Self::Level,
        Self::Price,
        Self::Rate,
        Self::Empty,
        Self::NoSkills,
        Self::ChatHint,
        Self::ShopHint,
        Self::InventoryHint,
        Self::UnknownItem,
        Self::UnknownMap,
        Self::SellAllowed,
        Self::RepairAllowed,
        Self::NpcHint,
        Self::Connecting,
        Self::Disconnected,
        Self::CastHint,
        Self::Equipment,
        Self::Equip,
        Self::Use,
        Self::Type,
        Self::Require,
        Self::Durability,
        Self::Stack,
        Self::Attack,
        Self::Defence,
        Self::MagicDefence,
        Self::MagicAttack,
        Self::Taoism,
        Self::Accuracy,
        Self::Agility,
        Self::AttackSpeed,
        Self::Luck,
        Self::Totals,
        Self::Buffs,
        Self::Seconds,
        Self::Infinite,
        Self::Paused,
        Self::Miss,
        Self::Critical,
        Self::WorldHint,
        Self::InventoryTooltipHint,
        Self::Minimap,
        Self::NoMinimap,
        Self::MinimapHint,
        Self::NoBigmap,
        Self::BigmapHint,
        Self::Quests,
        Self::QuestDiary,
        Self::QuestHint,
        Self::QuestInProgress,
        Self::QuestComplete,
        Self::QuestAvailable,
        Self::Abandon,
        Self::TimeLeft,
        Self::Storage,
        Self::Capacity,
        Self::StorageHint,
        Self::Mail,
        Self::Inbox,
        Self::Compose,
        Self::Sender,
        Self::Subject,
        Self::Message,
        Self::Date,
        Self::Attachment,
        Self::Collect,
        Self::Lock,
        Self::Unlock,
        Self::Delete,
        Self::Send,
        Self::Recipient,
        Self::Postage,
        Self::Inquiry,
        Self::Stamp,
        Self::Back,
        Self::NoMail,
        Self::MailHint,
        Self::Guild,
        Self::Members,
        Self::GuildStorageTab,
        Self::GuildBuffsTab,
        Self::Notice,
        Self::GuildFunds,
        Self::Rank,
        Self::Online,
        Self::Offline,
        Self::MapColumn,
        Self::Promote,
        Self::Demote,
        Self::Kick,
        Self::Save,
        Self::Activate,
        Self::Purchase,
        Self::Deposit,
        Self::Withdraw,
        Self::Amount,
        Self::NoGuild,
        Self::GuildHint,
        Self::Available,
        Self::Obtained,
        Self::Expired,
        Self::MinutesLeft,
        Self::NoQuests,
        Self::Points,
        Self::Unread,
        Self::Collected,
        Self::NoBody,
        Self::Upgrade,
        Self::NoSubjectField,
        Self::Trade,
        Self::TradeRequestSuffix,
        Self::TradeYourSide,
        Self::TradePartnerGold,
        Self::TradeReady,
        Self::TradeConfirm,
        Self::TradeCancel,
        Self::TradeGoldAdd,
        Self::TradeHint,
        Self::NotReady,
        Self::Ready,
        Self::TradePartnerState,
        Self::Accept,
        Self::Reject,
        Self::Group,
        Self::GroupInviteSuffix,
        Self::Invite,
        Self::Leave,
        Self::InviteName,
        Self::GroupLeader,
        Self::Profession,
        Self::GroupHint,
        Self::GroupChatHint,
        Self::GroupAllow,
        Self::ToggleOn,
        Self::ToggleOff,
        Self::NoGroup,
        Self::Health,
        Self::GroupKick,
        Self::Market,
        Self::ItemColumn,
        Self::Consign,
        Self::Auction,
        Self::Search,
        Self::Refresh,
        Self::Seller,
        Self::Expiry,
        Self::Buy,
        Self::Bid,
        Self::BidAmount,
        Self::ConsignPrice,
        Self::StartingBid,
        Self::ListItem,
        Self::GetBack,
        Self::SellNow,
        Self::CollectSold,
        Self::PrevPage,
        Self::NextPage,
        Self::NoListings,
        Self::MarketHint,
        Self::ConsignHint,
        Self::AuctionHint,
        Self::MarketFailGeneric,
        Self::MarketFailDead,
        Self::MarketFailNoDialog,
        Self::MarketFailSold,
        Self::MarketFailExpired,
        Self::MarketFailNoSpace,
        Self::MarketFailOwnItem,
        Self::MarketFailLowGold,
        Self::MarketFailTooFar,
        Self::MarketFailCannotGainGold,
        Self::MarketFailBidTooLow,
        Self::MarketFailAuctionEnded,
        Self::ConsignSuccess,
        Self::ConsignFailed,
        Self::NoSelectedItem,
        Self::ConsignSelected,
        Self::GameShop,
        Self::Credit,
        Self::Stock,
        Self::OnSale,
        Self::Quantity,
        Self::GameShopHint,
        Self::GameShopPickHint,
        Self::NoGameShopItems,
        Self::Category,
        Self::Settings,
        Self::Audio,
        Self::Music,
        Self::Sound,
        Self::Volume,
        Self::Language,
        Self::Display,
        Self::Resolution,
        Self::UiScale,
        Self::KeyBindings,
        Self::SettingsHint,
        Self::Login,
        Self::Server,
        Self::ServerAddress,
        Self::Account,
        Self::Password,
        Self::Connect,
        Self::Retry,
        Self::SelectCharacter,
        Self::CreateCharacter,
        Self::EnterGame,
        Self::CharacterName,
        Self::Gender,
        Self::Male,
        Self::Female,
        Self::NoCharacters,
        Self::ConnectionFailed,
        Self::LastAccess,
        Self::Never,
        Self::Quit,
        Self::LoginHint,
        Self::CharSelectHint,
        Self::NoEquipmentSummary,
        Self::BackToServer,
        Self::Bigmap,
        Self::KeyMove,
        Self::KeyRun,
        Self::KeyAttack,
        Self::KeyPickUp,
        Self::KeyChat,
        Self::KeyEscape,
        Self::WindowTitleMap,
        Self::WindowTitleLogin,
        Self::WindowTitleLive,
        Self::StatusVerifying,
        Self::StatusLoggingIn,
        Self::StatusSelectingCharacter,
        Self::StatusStartingGame,
        Self::StatusInGame,
        Self::StatusFailed,
        Self::NetworkThreadExited,
        Self::LoginThreadExited,
        Self::SystemGoldGained,
        Self::SystemItemGained,
        Self::SystemQuestShared,
        Self::SystemGuildGoldDeposit,
        Self::SystemGuildGoldWithdraw,
        Self::SystemTradeComplete,
        Self::SystemTradeCancelled,
        Self::SystemGroupLeft,
        Self::SystemGroupJoined,
        Self::SystemGroupMemberLeft,
        Self::SystemGuildMemberOnline,
        Self::SystemGuildMemberJoined,
        Self::SystemGuildMemberKicked,
        Self::SystemGuildMemberLeft,
        Self::QuestFallback,
        Self::ConnectUnreachable,
        Self::ConnectCredentials,
        Self::ConnectRefused,
        Self::ConnectTimeout,
        Self::ConnectVersionMismatch,
        Self::ConnectFailed,
        Self::QuestItems,
        Self::PhaseDawn,
        Self::PhaseDay,
        Self::PhaseEvening,
        Self::PhaseNight,
        Self::WeatherClear,
        Self::WeatherFog,
        Self::WeatherEmber,
        Self::WeatherFire,
        Self::WeatherSnow,
        Self::WeatherRain,
        Self::WeatherLeaves,
        Self::SystemQuestItemGained,
        Self::SystemQuestItemLost,
        Self::PetModeFollow,
        Self::PetModeAttack,
        Self::PetModeRest,
        Self::PetModeChanged,
        Self::NoPetHint,
        Self::QuestItemsScrollHint,
        Self::PetMode,
        Self::GuildAllyTab,
        Self::GuildAllyName,
        Self::GuildAllyRequest,
        Self::GuildAllyAccept,
        Self::GuildAllyRemove,
        Self::GuildAllyEmpty,
        Self::GuildAllyPending,
        Self::GuildAllyHint,
        Self::Friends,
        Self::FriendName,
        Self::FriendMemo,
        Self::FriendBlocked,
        Self::FriendEmpty,
        Self::FriendAdd,
        Self::FriendRemove,
        Self::FriendSaveMemo,
        Self::FriendRefresh,
        Self::FriendHint,
        Self::ConquestWar,
        Self::ConquestRemaining,
        Self::ConquestOwner,
        Self::ConquestControlPoints,
        Self::ConquestResult,
        Self::ConquestWinner,
        Self::ConquestGate,
        Self::ConquestWall,
        Self::ConquestArcher,
        Self::ConquestDestroyed,
        Self::ConquestGateOpen,
        Self::GuildAllyLeader,
        Self::Reconnecting,
        Self::MinimapGateTip,
        Self::ConquestUnclaimed,
        Self::SystemConquestStarted,
        Self::SystemConquestEnded,
        Self::SystemConquestObjectDestroyed,
    ];
}

impl Label {
    /// 取当前语言下的文案。
    #[must_use]
    pub const fn text(self, language: Language) -> &'static str {
        match (self, language) {
            (Self::Hp, Language::ZhCn) => "生命",
            (Self::Hp | Self::Health, Language::En) => "HP",
            (Self::Mp | Self::MagicAttack, Language::ZhCn) => "魔法",
            (Self::Mp, Language::En) => "MP",
            (Self::Gold, Language::ZhCn) => "金币",
            (Self::Gold, Language::En) => "Gold",
            (Self::Map | Self::MapColumn, Language::ZhCn) => "地图",
            (Self::Map | Self::MapColumn, Language::En) => "Map",
            (Self::Chat, Language::ZhCn) => "聊天",
            (Self::Chat, Language::En) => "Chat",
            (Self::Inventory, Language::ZhCn) => "背包",
            (Self::Inventory, Language::En) => "Inventory",
            (Self::Shop, Language::ZhCn) => "商店",
            (Self::Shop | Self::GameShop, Language::En) => "Shop",
            (Self::Dialog, Language::ZhCn) => "对话",
            (Self::Dialog, Language::En) => "Dialog",
            (Self::Skills, Language::ZhCn) => "技能",
            (Self::Skills, Language::En) => "Skills",
            (Self::Close, Language::ZhCn) => "关闭",
            (Self::Close, Language::En) => "Close",
            (Self::Sell, Language::ZhCn) => "出售",
            (Self::Sell | Self::SellAllowed, Language::En) => "Sell",
            (Self::Repair, Language::ZhCn) => "修理",
            (Self::Repair | Self::RepairAllowed, Language::En) => "Repair",
            (Self::Drop, Language::ZhCn) => "丢弃",
            (Self::Drop, Language::En) => "Drop",
            (Self::Level, Language::ZhCn) => "等级",
            (Self::Level, Language::En) => "Lv",
            (Self::Price, Language::ZhCn) => "价格",
            (Self::Price | Self::ConsignPrice, Language::En) => "Price",
            (Self::Rate, Language::ZhCn) => "倍率",
            (Self::Rate, Language::En) => "Rate",
            (Self::Empty, Language::ZhCn) => "空",
            (Self::Empty, Language::En) => "-",
            (Self::NoSkills, Language::ZhCn) => "尚未学习技能",
            (Self::NoSkills, Language::En) => "No skills learned",
            (Self::ChatHint, Language::ZhCn) => "按 T 聊天",
            (Self::ChatHint, Language::En) => "Press T to chat",
            (Self::ShopHint, Language::ZhCn) => "S 出售 · R 修理",
            (Self::ShopHint, Language::En) => "S sell · R repair",
            (Self::InventoryHint, Language::ZhCn) => "左键选中 · D 丢弃",
            (Self::InventoryHint, Language::En) => "Click select · D drop",
            (Self::UnknownItem, Language::ZhCn) => "未知物品",
            (Self::UnknownItem, Language::En) => "Unknown item",
            (Self::UnknownMap, Language::ZhCn) => "未知地图",
            (Self::UnknownMap, Language::En) => "Unknown map",
            (Self::SellAllowed, Language::ZhCn) => "可出售",
            (Self::RepairAllowed, Language::ZhCn) => "可修理",
            (Self::NpcHint, Language::ZhCn) => "点击按钮继续 · Esc 关闭",
            (Self::NpcHint, Language::En) => "Click a button · Esc to close",
            (Self::Connecting, Language::ZhCn) => "连接中…",
            (Self::Connecting, Language::En) => "Connecting…",
            (Self::Disconnected, Language::ZhCn) => "已断开",
            (Self::Disconnected, Language::En) => "Disconnected",
            (Self::CastHint, Language::ZhCn) => "按数字键施法",
            (Self::CastHint, Language::En) => "Press number keys",
            (Self::Equipment | Self::Equip, Language::ZhCn) => "装备",
            (Self::Equipment, Language::En) => "Equipment",
            (Self::Equip, Language::En) => "Equip",
            (Self::Use, Language::ZhCn) => "使用",
            (Self::Use, Language::En) => "Use",
            (Self::Type, Language::ZhCn) => "类型",
            (Self::Type, Language::En) => "Type",
            (Self::Require, Language::ZhCn) => "需求",
            (Self::Require, Language::En) => "Require",
            (Self::Durability, Language::ZhCn) => "持久",
            (Self::Durability, Language::En) => "Durability",
            (Self::Stack, Language::ZhCn) => "堆叠",
            (Self::Stack, Language::En) => "Stack",
            (Self::Attack | Self::KeyAttack | Self::PetModeAttack, Language::ZhCn) => "攻击",
            (Self::Attack, Language::En) => "DC",
            (Self::Defence, Language::ZhCn) => "防御",
            (Self::Defence, Language::En) => "AC",
            (Self::MagicDefence, Language::ZhCn) => "魔防",
            (Self::MagicDefence, Language::En) => "MAC",
            (Self::MagicAttack, Language::En) => "MC",
            (Self::Taoism, Language::ZhCn) => "道术",
            (Self::Taoism, Language::En) => "SC",
            (Self::Accuracy, Language::ZhCn) => "准确",
            (Self::Accuracy, Language::En) => "Accuracy",
            (Self::Agility, Language::ZhCn) => "敏捷",
            (Self::Agility, Language::En) => "Agility",
            (Self::AttackSpeed, Language::ZhCn) => "攻速",
            (Self::AttackSpeed, Language::En) => "Atk Speed",
            (Self::Luck, Language::ZhCn) => "幸运",
            (Self::Luck, Language::En) => "Luck",
            (Self::Totals, Language::ZhCn) => "属性汇总",
            (Self::Totals, Language::En) => "Totals",
            (Self::Buffs | Self::GuildBuffsTab, Language::ZhCn) => "增益",
            (Self::Buffs | Self::GuildBuffsTab, Language::En) => "Buffs",
            (Self::Seconds, Language::ZhCn) => "秒",
            (Self::Seconds, Language::En) => "s",
            (Self::Infinite, Language::ZhCn) => "永久",
            (Self::Infinite, Language::En) => "infinite",
            (Self::Paused, Language::ZhCn) => "暂停",
            (Self::Paused, Language::En) => "paused",
            (Self::Miss, Language::ZhCn) => "闪避",
            (Self::Miss, Language::En) => "Miss",
            (Self::Critical, Language::ZhCn) => "暴击",
            (Self::Critical, Language::En) => "Crit",
            (Self::WorldHint, Language::ZhCn) => "点地走路 · 按住连续走",
            (Self::WorldHint, Language::En) => "Click ground to walk · hold to continue",
            (Self::InventoryTooltipHint, Language::ZhCn) => "双击使用 · 右键菜单 · 拖动装备",
            (Self::InventoryTooltipHint, Language::En) => {
                "Double-click use · right-click menu · drag to equip"
            }
            (Self::Minimap, Language::ZhCn) => "小地图",
            (Self::Minimap, Language::En) => "Minimap",
            (Self::NoMinimap, Language::ZhCn) => "无小地图素材",
            (Self::NoMinimap, Language::En) => "No minimap art",
            (Self::MinimapHint, Language::ZhCn) => "M 关闭",
            (Self::MinimapHint, Language::En) => "M to close",
            (Self::NoBigmap, Language::ZhCn) => "无大地图素材",
            (Self::NoBigmap, Language::En) => "No big map art",
            (Self::BigmapHint, Language::ZhCn) => "B / Esc 关闭",
            (Self::BigmapHint, Language::En) => "B / Esc to close",
            (Self::Quests | Self::QuestDiary, Language::ZhCn) => "任务",
            (Self::Quests, Language::En) => "Quests",
            (Self::QuestDiary, Language::En) => "Quest Diary",
            (Self::QuestHint, Language::ZhCn) => "Q 关闭 · 点击任务选中",
            (Self::QuestHint, Language::En) => "Q to close · click a quest",
            (Self::QuestInProgress, Language::ZhCn) => "进行中",
            (Self::QuestInProgress, Language::En) => "In progress",
            (Self::QuestComplete, Language::ZhCn) => "已完成 · 去交付",
            (Self::QuestComplete, Language::En) => "Completed · turn in",
            (Self::QuestAvailable, Language::ZhCn) => "可接取",
            (Self::QuestAvailable | Self::Available, Language::En) => "Available",
            (Self::Abandon, Language::ZhCn) => "放弃",
            (Self::Abandon, Language::En) => "Abandon",
            (Self::TimeLeft | Self::ConquestRemaining, Language::ZhCn) => "剩余",
            (Self::TimeLeft | Self::ConquestRemaining, Language::En) => "Left",
            (Self::Storage | Self::GuildStorageTab, Language::ZhCn) => "仓库",
            (Self::Storage | Self::GuildStorageTab, Language::En) => "Storage",
            (Self::Capacity, Language::ZhCn) => "容量",
            (Self::Capacity, Language::En) => "Capacity",
            (Self::StorageHint, Language::ZhCn) => "拖拽存取 · Esc 关闭",
            (Self::StorageHint, Language::En) => "Drag to store · Esc to close",
            (Self::Mail, Language::ZhCn) => "邮件",
            (Self::Mail, Language::En) => "Mail",
            (Self::Inbox, Language::ZhCn) => "收件箱",
            (Self::Inbox, Language::En) => "Inbox",
            (Self::Compose, Language::ZhCn) => "写信",
            (Self::Compose, Language::En) => "Compose",
            (Self::Sender, Language::ZhCn) => "发件人",
            (Self::Sender, Language::En) => "Sender",
            (Self::Subject, Language::ZhCn) => "主题",
            (Self::Subject, Language::En) => "Subject",
            (Self::Message, Language::ZhCn) => "正文",
            (Self::Message, Language::En) => "Message",
            (Self::Date, Language::ZhCn) => "日期",
            (Self::Date, Language::En) => "Date",
            (Self::Attachment, Language::ZhCn) => "附件",
            (Self::Attachment, Language::En) => "Attachment",
            (Self::Collect, Language::ZhCn) => "收取附件",
            (Self::Collect, Language::En) => "Collect",
            (Self::Lock, Language::ZhCn) => "锁定",
            (Self::Lock, Language::En) => "Lock",
            (Self::Unlock, Language::ZhCn) => "解锁",
            (Self::Unlock, Language::En) => "Unlock",
            (Self::Delete | Self::FriendRemove, Language::ZhCn) => "删除",
            (Self::Delete, Language::En) => "Delete",
            (Self::Send, Language::ZhCn) => "发送",
            (Self::Send, Language::En) => "Send",
            (Self::Recipient, Language::ZhCn) => "收件人",
            (Self::Recipient, Language::En) => "Recipient",
            (Self::Postage, Language::ZhCn) => "邮费",
            (Self::Postage, Language::En) => "Postage",
            (Self::Inquiry, Language::ZhCn) => "询价",
            (Self::Inquiry, Language::En) => "Quote",
            (Self::Stamp, Language::ZhCn) => "贴邮票",
            (Self::Stamp, Language::En) => "Stamp",
            (Self::Back, Language::ZhCn) => "返回",
            (Self::Back, Language::En) => "Back",
            (Self::NoMail, Language::ZhCn) => "没有邮件",
            (Self::NoMail, Language::En) => "No mail",
            (Self::MailHint, Language::ZhCn) => "N 关闭 · 点击邮件阅读",
            (Self::MailHint, Language::En) => "N to close · click to read",
            (Self::Guild, Language::ZhCn) => "行会",
            (Self::Guild, Language::En) => "Guild",
            (Self::Members, Language::ZhCn) => "成员",
            (Self::Members, Language::En) => "Members",
            (Self::Notice, Language::ZhCn) => "公告",
            (Self::Notice, Language::En) => "Notice",
            (Self::GuildFunds, Language::ZhCn) => "行会资金",
            (Self::GuildFunds, Language::En) => "Guild Funds",
            (Self::Rank, Language::ZhCn) => "职位",
            (Self::Rank, Language::En) => "Rank",
            (Self::Online, Language::ZhCn) => "在线",
            (Self::Online, Language::En) => "Online",
            (Self::Offline, Language::ZhCn) => "离线",
            (Self::Offline, Language::En) => "Offline",
            (Self::Promote, Language::ZhCn) => "升职",
            (Self::Promote, Language::En) => "Promote",
            (Self::Demote, Language::ZhCn) => "降职",
            (Self::Demote, Language::En) => "Demote",
            (Self::Kick, Language::ZhCn) => "踢人",
            (Self::Kick | Self::GroupKick, Language::En) => "Kick",
            (Self::Save, Language::ZhCn) => "保存",
            (Self::Save, Language::En) => "Save",
            (Self::Activate, Language::ZhCn) => "激活",
            (Self::Activate, Language::En) => "Activate",
            (Self::Purchase, Language::ZhCn) => "购买",
            (Self::Purchase, Language::En) => "Purchase",
            (Self::Deposit, Language::ZhCn) => "存入",
            (Self::Deposit, Language::En) => "Deposit",
            (Self::Withdraw, Language::ZhCn) => "取出",
            (Self::Withdraw, Language::En) => "Withdraw",
            (Self::Amount, Language::ZhCn) => "金额",
            (Self::Amount, Language::En) => "Amount",
            (Self::NoGuild, Language::ZhCn) => "未加入行会",
            (Self::NoGuild, Language::En) => "Not in a guild",
            (Self::GuildHint, Language::ZhCn) => "G 关闭 · Esc 收起",
            (Self::GuildHint, Language::En) => "G to close · Esc hides",
            (Self::Available, Language::ZhCn) => "可用",
            (Self::Obtained, Language::ZhCn) => "已获得",
            (Self::Obtained, Language::En) => "Obtained",
            (Self::Expired, Language::ZhCn) => "已过期",
            (Self::Expired, Language::En) => "Expired",
            (Self::MinutesLeft, Language::ZhCn) => "分钟",
            (Self::MinutesLeft, Language::En) => "min",
            (Self::NoQuests, Language::ZhCn) => "没有进行中的任务",
            (Self::NoQuests, Language::En) => "No active quests",
            (Self::Points, Language::ZhCn) => "点数",
            (Self::Points, Language::En) => "Points",
            (Self::Unread, Language::ZhCn) => "未读",
            (Self::Unread, Language::En) => "Unread",
            (Self::Collected, Language::ZhCn) => "已领取",
            (Self::Collected, Language::En) => "Collected",
            (Self::NoBody, Language::ZhCn) => "（无正文）",
            (Self::NoBody, Language::En) => "(no message)",
            (Self::Upgrade, Language::ZhCn) => "升级",
            (Self::Upgrade, Language::En) => "Upgrade",
            (Self::NoSubjectField, Language::ZhCn) => "协议无主题字段，仅本地显示",
            (Self::NoSubjectField, Language::En) => "no subject field on the wire",
            (Self::Trade, Language::ZhCn) => "交易",
            (Self::Trade, Language::En) => "Trade",
            (Self::TradeRequestSuffix, Language::ZhCn) => "请求与你交易",
            (Self::TradeRequestSuffix, Language::En) => "requests a trade with you",
            (Self::TradeYourSide, Language::ZhCn) => "自己",
            (Self::TradeYourSide, Language::En) => "You",
            (Self::TradePartnerGold, Language::ZhCn) => "对方金币",
            (Self::TradePartnerGold, Language::En) => "Their gold",
            (Self::TradeReady, Language::ZhCn) => "准备",
            (Self::TradeReady | Self::Ready, Language::En) => "Ready",
            (Self::TradeConfirm, Language::ZhCn) => "确认",
            (Self::TradeConfirm, Language::En) => "Confirm",
            (Self::TradeCancel, Language::ZhCn) => "取消交易",
            (Self::TradeCancel, Language::En) => "Cancel trade",
            (Self::TradeGoldAdd, Language::ZhCn) => "放入金币",
            (Self::TradeGoldAdd, Language::En) => "Add gold",
            (Self::TradeHint, Language::ZhCn) => "拖拽背包物品到交易格 · Esc 关闭",
            (Self::TradeHint, Language::En) => {
                "Drag inventory items into the trade grid · Esc closes"
            }
            (Self::NotReady, Language::ZhCn) => "未准备",
            (Self::NotReady, Language::En) => "Not ready",
            (Self::Ready, Language::ZhCn) => "已准备",
            (Self::TradePartnerState, Language::ZhCn) => "对方",
            (Self::TradePartnerState, Language::En) => "Partner",
            (Self::Accept, Language::ZhCn) => "接受",
            (Self::Accept | Self::GuildAllyAccept, Language::En) => "Accept",
            (Self::Reject, Language::ZhCn) => "拒绝",
            (Self::Reject, Language::En) => "Reject",
            (Self::Group, Language::ZhCn) => "组队",
            (Self::Group, Language::En) => "Group",
            (Self::GroupInviteSuffix, Language::ZhCn) => "邀请你组队",
            (Self::GroupInviteSuffix, Language::En) => "invites you to a group",
            (Self::Invite, Language::ZhCn) => "邀请",
            (Self::Invite, Language::En) => "Invite",
            (Self::Leave, Language::ZhCn) => "离队",
            (Self::Leave, Language::En) => "Leave",
            (Self::InviteName, Language::ZhCn) => "输入名字",
            (Self::InviteName, Language::En) => "Enter a name",
            (Self::GroupLeader, Language::ZhCn) => "队长",
            (Self::GroupLeader | Self::GuildAllyLeader, Language::En) => "Leader",
            (Self::Profession, Language::ZhCn) => "职业",
            (Self::Profession, Language::En) => "Class",
            (Self::GroupHint, Language::ZhCn) => "P 关闭 · 队长才能邀请/踢人",
            (Self::GroupHint, Language::En) => "P to close · leader can invite or kick",
            (Self::GroupChatHint, Language::ZhCn) => "聊天输入 !! 前缀发组队消息",
            (Self::GroupChatHint, Language::En) => "Prefix chat with !! for group chat",
            (Self::GroupAllow, Language::ZhCn) => "允许组队",
            (Self::GroupAllow, Language::En) => "Allow group",
            (Self::ToggleOn, Language::ZhCn) => "开",
            (Self::ToggleOn, Language::En) => "On",
            (Self::ToggleOff, Language::ZhCn) => "关",
            (Self::ToggleOff, Language::En) => "Off",
            (Self::NoGroup, Language::ZhCn) => "未组队",
            (Self::NoGroup, Language::En) => "Not in a group",
            (Self::Health, Language::ZhCn) => "血量",
            (Self::GroupKick, Language::ZhCn) => "踢出队伍",
            (Self::Market, Language::ZhCn) => "市场",
            (Self::Market, Language::En) => "Market",
            (Self::ItemColumn, Language::ZhCn) => "物品",
            (Self::ItemColumn, Language::En) => "Item",
            (Self::Consign, Language::ZhCn) => "寄售",
            (Self::Consign, Language::En) => "Consign",
            (Self::Auction, Language::ZhCn) => "拍卖",
            (Self::Auction, Language::En) => "Auction",
            (Self::Search, Language::ZhCn) => "搜索",
            (Self::Search, Language::En) => "Search",
            (Self::Refresh | Self::FriendRefresh, Language::ZhCn) => "刷新",
            (Self::Refresh | Self::FriendRefresh, Language::En) => "Refresh",
            (Self::Seller, Language::ZhCn) => "卖家",
            (Self::Seller, Language::En) => "Seller",
            (Self::Expiry, Language::ZhCn) => "到期",
            (Self::Expiry, Language::En) => "Expires",
            (Self::Buy, Language::ZhCn) => "买下",
            (Self::Buy, Language::En) => "Buy",
            (Self::Bid, Language::ZhCn) => "出价",
            (Self::Bid, Language::En) => "Bid",
            (Self::BidAmount, Language::ZhCn) => "出价金额",
            (Self::BidAmount, Language::En) => "Bid amount",
            (Self::ConsignPrice, Language::ZhCn) => "售价",
            (Self::StartingBid, Language::ZhCn) => "起拍价",
            (Self::StartingBid, Language::En) => "Starting bid",
            (Self::ListItem, Language::ZhCn) => "上架",
            (Self::ListItem, Language::En) => "List",
            (Self::GetBack, Language::ZhCn) => "取回",
            (Self::GetBack, Language::En) => "Take back",
            (Self::SellNow, Language::ZhCn) => "提前成交",
            (Self::SellNow, Language::En) => "Sell now",
            (Self::CollectSold, Language::ZhCn) => "一键收款",
            (Self::CollectSold, Language::En) => "Collect sold",
            (Self::PrevPage, Language::ZhCn) => "上一页",
            (Self::PrevPage, Language::En) => "Prev",
            (Self::NextPage, Language::ZhCn) => "下一页",
            (Self::NextPage, Language::En) => "Next",
            (Self::NoListings, Language::ZhCn) => "没有条目",
            (Self::NoListings, Language::En) => "No listings",
            (Self::MarketHint, Language::ZhCn) => "搜索 / 翻页 · Esc 关闭",
            (Self::MarketHint, Language::En) => "Search or page · Esc closes",
            (Self::ConsignHint, Language::ZhCn) => "在背包选中物品、填价后上架",
            (Self::ConsignHint, Language::En) => "Select a bag item, set a price, then list",
            (Self::AuctionHint, Language::ZhCn) => "在背包选中物品、填起拍价后上架",
            (Self::AuctionHint, Language::En) => "Select a bag item, set a starting bid, then list",
            (Self::MarketFailGeneric, Language::ZhCn) => "操作失败",
            (Self::MarketFailGeneric | Self::StatusFailed, Language::En) => "Failed",
            (Self::MarketFailDead, Language::ZhCn) => "角色已死亡",
            (Self::MarketFailDead, Language::En) => "You are dead",
            (Self::MarketFailNoDialog, Language::ZhCn) => "不在市场 NPC 旁",
            (Self::MarketFailNoDialog, Language::En) => "Not at the market NPC",
            (Self::MarketFailSold, Language::ZhCn) => "该条目已售出",
            (Self::MarketFailSold, Language::En) => "That listing is sold",
            (Self::MarketFailExpired, Language::ZhCn) => "该条目已过期",
            (Self::MarketFailExpired, Language::En) => "That listing has expired",
            (Self::MarketFailNoSpace, Language::ZhCn) => "背包空间不足",
            (Self::MarketFailNoSpace, Language::En) => "Not enough bag space",
            (Self::MarketFailOwnItem, Language::ZhCn) => "不能购买自己的条目",
            (Self::MarketFailOwnItem, Language::En) => "Cannot buy your own listing",
            (Self::MarketFailLowGold, Language::ZhCn) => "金币不足",
            (Self::MarketFailLowGold, Language::En) => "Not enough gold",
            (Self::MarketFailTooFar, Language::ZhCn) => "条目不可用或距离太远",
            (Self::MarketFailTooFar, Language::En) => "Listing unavailable or too far",
            (Self::MarketFailCannotGainGold, Language::ZhCn) => "金币已达上限",
            (Self::MarketFailCannotGainGold, Language::En) => "Gold limit reached",
            (Self::MarketFailBidTooLow, Language::ZhCn) => "出价太低",
            (Self::MarketFailBidTooLow, Language::En) => "Bid too low",
            (Self::MarketFailAuctionEnded, Language::ZhCn) => "拍卖已结束",
            (Self::MarketFailAuctionEnded, Language::En) => "Auction ended",
            (Self::ConsignSuccess, Language::ZhCn) => "上架成功",
            (Self::ConsignSuccess, Language::En) => "Listed",
            (Self::ConsignFailed, Language::ZhCn) => "上架失败",
            (Self::ConsignFailed, Language::En) => "Listing failed",
            (Self::NoSelectedItem, Language::ZhCn) => "未选中物品",
            (Self::NoSelectedItem, Language::En) => "No item selected",
            (Self::ConsignSelected, Language::ZhCn) => "上架物品",
            (Self::ConsignSelected, Language::En) => "Listing",
            (Self::GameShop, Language::ZhCn) => "商城",
            (Self::Credit, Language::ZhCn) => "元宝",
            (Self::Credit, Language::En) => "Credits",
            (Self::Stock, Language::ZhCn) => "库存",
            (Self::Stock, Language::En) => "Stock",
            (Self::OnSale, Language::ZhCn) => "在售",
            (Self::OnSale, Language::En) => "On sale",
            (Self::Quantity, Language::ZhCn) => "数量",
            (Self::Quantity, Language::En) => "Qty",
            (Self::GameShopHint, Language::ZhCn) => "选中商品后点购买 · 元宝结算 · Esc 关闭",
            (Self::GameShopHint, Language::En) => {
                "Select an item then Buy · paid in Credits · Esc closes"
            }
            (Self::GameShopPickHint, Language::ZhCn) => "选中商品后点购买",
            (Self::GameShopPickHint, Language::En) => "Select an item, then Buy",
            (Self::NoGameShopItems, Language::ZhCn) => "商城暂无商品",
            (Self::NoGameShopItems, Language::En) => "The shop has no items",
            (Self::Category, Language::ZhCn) => "分类",
            (Self::Category, Language::En) => "Category",
            (Self::Settings, Language::ZhCn) => "设置",
            (Self::Settings, Language::En) => "Settings",
            (Self::Audio, Language::ZhCn) => "音频",
            (Self::Audio, Language::En) => "Audio",
            (Self::Music, Language::ZhCn) => "背景音乐",
            (Self::Music, Language::En) => "Music",
            (Self::Sound, Language::ZhCn) => "音效",
            (Self::Sound, Language::En) => "Sound",
            (Self::Volume, Language::ZhCn) => "音量",
            (Self::Volume, Language::En) => "Volume",
            (Self::Language, Language::ZhCn) => "语言",
            (Self::Language, Language::En) => "Language",
            (Self::Display, Language::ZhCn) => "显示",
            (Self::Display, Language::En) => "Display",
            (Self::Resolution, Language::ZhCn) => "分辨率",
            (Self::Resolution, Language::En) => "Resolution",
            (Self::UiScale, Language::ZhCn) => "缩放",
            (Self::UiScale, Language::En) => "Scale",
            (Self::KeyBindings, Language::ZhCn) => "按键说明",
            (Self::KeyBindings, Language::En) => "Key bindings",
            (Self::SettingsHint, Language::ZhCn) => "O 关闭 · Esc 收起 · 改动即时生效并保存",
            (Self::SettingsHint, Language::En) => {
                "O to close · Esc hides · changes apply and save at once"
            }
            (Self::Login, Language::ZhCn) => "登录",
            (Self::Login, Language::En) => "Log in",
            (Self::Server, Language::ZhCn) => "服务器",
            (Self::Server, Language::En) => "Server",
            (Self::ServerAddress, Language::ZhCn) => "服务器地址",
            (Self::ServerAddress, Language::En) => "Server address",
            (Self::Account, Language::ZhCn) => "账号",
            (Self::Account, Language::En) => "Account",
            (Self::Password, Language::ZhCn) => "密码",
            (Self::Password, Language::En) => "Password",
            (Self::Connect, Language::ZhCn) => "连接",
            (Self::Connect, Language::En) => "Connect",
            (Self::Retry, Language::ZhCn) => "重试",
            (Self::Retry, Language::En) => "Retry",
            (Self::SelectCharacter | Self::StatusSelectingCharacter, Language::ZhCn) => "选择角色",
            (Self::SelectCharacter, Language::En) => "Select character",
            (Self::CreateCharacter, Language::ZhCn) => "创建角色",
            (Self::CreateCharacter, Language::En) => "Create character",
            (Self::EnterGame, Language::ZhCn) => "进入游戏",
            (Self::EnterGame, Language::En) => "Enter game",
            (Self::CharacterName, Language::ZhCn) => "角色名",
            (Self::CharacterName | Self::FriendName, Language::En) => "Name",
            (Self::Gender, Language::ZhCn) => "性别",
            (Self::Gender, Language::En) => "Gender",
            (Self::Male, Language::ZhCn) => "男",
            (Self::Male, Language::En) => "Male",
            (Self::Female, Language::ZhCn) => "女",
            (Self::Female, Language::En) => "Female",
            (Self::NoCharacters, Language::ZhCn) => "账号下还没有角色，先创建一个开始冒险",
            (Self::NoCharacters, Language::En) => "No characters yet — create one to start",
            (Self::ConnectionFailed, Language::ZhCn) => "连接失败",
            (Self::ConnectionFailed, Language::En) => "Connection failed",
            (Self::LastAccess, Language::ZhCn) => "上次登录",
            (Self::LastAccess, Language::En) => "Last access",
            (Self::Never, Language::ZhCn) => "从未登录",
            (Self::Never, Language::En) => "Never",
            (Self::Quit, Language::ZhCn) => "退出",
            (Self::Quit, Language::En) => "Quit",
            (Self::LoginHint, Language::ZhCn) => "填好地址与账号后点连接 · Esc 退出",
            (Self::LoginHint, Language::En) => {
                "Fill in the address and account, then Connect · Esc quits"
            }
            (Self::CharSelectHint, Language::ZhCn) => "点角色选中 · 回车进入游戏 · Esc 返回",
            (Self::CharSelectHint, Language::En) => "Click a character · Enter to play · Esc back",
            (Self::NoEquipmentSummary, Language::ZhCn) => "暂无装备",
            (Self::NoEquipmentSummary, Language::En) => "No equipment yet",
            (Self::BackToServer, Language::ZhCn) => "返回服务器",
            (Self::BackToServer, Language::En) => "Back to servers",
            (Self::Bigmap, Language::ZhCn) => "大地图",
            (Self::Bigmap, Language::En) => "Big map",
            (Self::KeyMove, Language::ZhCn) => "走 / 转身",
            (Self::KeyMove, Language::En) => "Walk / turn",
            (Self::KeyRun, Language::ZhCn) => "跑",
            (Self::KeyRun, Language::En) => "Run",
            (Self::KeyAttack | Self::PetModeAttack, Language::En) => "Attack",
            (Self::KeyPickUp, Language::ZhCn) => "拾取",
            (Self::KeyPickUp, Language::En) => "Pick up",
            (Self::KeyChat, Language::ZhCn) => "聊天输入",
            (Self::KeyChat, Language::En) => "Chat input",
            (Self::KeyEscape, Language::ZhCn) => "关闭面板",
            (Self::KeyEscape, Language::En) => "Close panels",
            (Self::WindowTitleMap, Language::ZhCn) => "传奇-Next · 地图",
            (Self::WindowTitleMap, Language::En) => "Legend of Mir Next · Map",
            (Self::WindowTitleLogin, Language::ZhCn) => "传奇-Next · 登录",
            (Self::WindowTitleLogin, Language::En) => "Legend of Mir Next · Login",
            (Self::WindowTitleLive, Language::ZhCn) => "传奇-Next · 联机",
            (Self::WindowTitleLive, Language::En) => "Legend of Mir Next · Online",
            (Self::StatusVerifying, Language::ZhCn) => "版本校验",
            (Self::StatusVerifying, Language::En) => "Verifying",
            (Self::StatusLoggingIn, Language::ZhCn) => "登录中",
            (Self::StatusLoggingIn, Language::En) => "Logging in",
            (Self::StatusSelectingCharacter, Language::En) => "Selecting character",
            (Self::StatusStartingGame, Language::ZhCn) => "进入世界",
            (Self::StatusStartingGame, Language::En) => "Entering world",
            (Self::StatusInGame, Language::ZhCn) => "已进入世界",
            (Self::StatusInGame, Language::En) => "In game",
            (Self::StatusFailed, Language::ZhCn) => "失败",
            (Self::NetworkThreadExited, Language::ZhCn) => "网络线程已退出",
            (Self::NetworkThreadExited, Language::En) => "Network thread exited",
            (Self::LoginThreadExited, Language::ZhCn) => "登录线程已退出",
            (Self::LoginThreadExited, Language::En) => "Login thread exited",
            (Self::SystemGoldGained, Language::ZhCn) => "获得金币 {}",
            (Self::SystemGoldGained, Language::En) => "Gained {} gold",
            (Self::SystemItemGained, Language::ZhCn) => "获得 {} ×{}",
            (Self::SystemItemGained, Language::En) => "Gained {} ×{}",
            (Self::SystemQuestShared, Language::ZhCn) => "{} 分享了任务：{}",
            (Self::SystemQuestShared, Language::En) => "{} shared a quest: {}",
            (Self::SystemGuildGoldDeposit, Language::ZhCn) => "{} 向行会捐献 {} 金币",
            (Self::SystemGuildGoldDeposit, Language::En) => "{} donated {} gold to the guild",
            (Self::SystemGuildGoldWithdraw, Language::ZhCn) => "{} 从行会取出 {} 金币",
            (Self::SystemGuildGoldWithdraw, Language::En) => "{} withdrew {} gold from the guild",
            (Self::SystemTradeComplete, Language::ZhCn) => "交易完成。",
            (Self::SystemTradeComplete, Language::En) => "Trade complete.",
            (Self::SystemTradeCancelled, Language::ZhCn) => "交易已取消。",
            (Self::SystemTradeCancelled, Language::En) => "Trade cancelled.",
            (Self::SystemGroupLeft, Language::ZhCn) => "你已离开队伍。",
            (Self::SystemGroupLeft, Language::En) => "You left the group.",
            (Self::SystemGroupJoined, Language::ZhCn) => "{} 加入了队伍。",
            (Self::SystemGroupJoined, Language::En) => "{} joined the group.",
            (Self::SystemGroupMemberLeft, Language::ZhCn) => "{} 退出了队伍。",
            (Self::SystemGroupMemberLeft, Language::En) => "{} left the group.",
            (Self::SystemGuildMemberOnline, Language::ZhCn) => "{} 上线了",
            (Self::SystemGuildMemberOnline, Language::En) => "{} is online",
            (Self::SystemGuildMemberJoined, Language::ZhCn) => "{} 加入了行会",
            (Self::SystemGuildMemberJoined, Language::En) => "{} joined the guild",
            (Self::SystemGuildMemberKicked, Language::ZhCn) => "{} 被移出行会",
            (Self::SystemGuildMemberKicked, Language::En) => "{} was removed from the guild",
            (Self::SystemGuildMemberLeft, Language::ZhCn) => "{} 离开了行会",
            (Self::SystemGuildMemberLeft, Language::En) => "{} left the guild",
            (Self::QuestFallback, Language::ZhCn) => "任务 #{}",
            (Self::QuestFallback, Language::En) => "Quest #{}",
            (Self::ConnectUnreachable, Language::ZhCn) => "无法连接服务器，请检查地址与端口。",
            (Self::ConnectUnreachable, Language::En) => {
                "Cannot reach the server. Check the address and port."
            }
            (Self::ConnectCredentials, Language::ZhCn) => "账号或密码错误，请重试。",
            (Self::ConnectCredentials, Language::En) => "Wrong account or password. Try again.",
            (Self::ConnectRefused, Language::ZhCn) => "服务器拒绝连接，请稍后重试。",
            (Self::ConnectRefused, Language::En) => {
                "The server refused the connection. Try again later."
            }
            (Self::ConnectTimeout, Language::ZhCn) => "连接超时，请重试。",
            (Self::ConnectTimeout, Language::En) => "Connection timed out. Try again.",
            (Self::ConnectVersionMismatch, Language::ZhCn) => {
                "客户端与服务端版本不匹配，请更新客户端。"
            }
            (Self::ConnectVersionMismatch, Language::En) => {
                "Client and server versions do not match. Update the client."
            }
            (Self::ConnectFailed, Language::ZhCn) => "连接失败，请重试。",
            (Self::ConnectFailed, Language::En) => "Connection failed. Try again.",
            (Self::QuestItems, Language::ZhCn) => "任务物品",
            (Self::QuestItems, Language::En) => "Quest items",
            (Self::PhaseDawn, Language::ZhCn) => "黎明",
            (Self::PhaseDawn, Language::En) => "Dawn",
            (Self::PhaseDay, Language::ZhCn) => "白天",
            (Self::PhaseDay, Language::En) => "Day",
            (Self::PhaseEvening, Language::ZhCn) => "黄昏",
            (Self::PhaseEvening, Language::En) => "Evening",
            (Self::PhaseNight, Language::ZhCn) => "夜晚",
            (Self::PhaseNight, Language::En) => "Night",
            (Self::WeatherClear, Language::ZhCn) => "晴",
            (Self::WeatherClear, Language::En) => "Clear",
            (Self::WeatherFog, Language::ZhCn) => "雾",
            (Self::WeatherFog, Language::En) => "Fog",
            (Self::WeatherEmber, Language::ZhCn) => "余烬",
            (Self::WeatherEmber, Language::En) => "Embers",
            (Self::WeatherFire, Language::ZhCn) => "火星",
            (Self::WeatherFire, Language::En) => "Fire particles",
            (Self::WeatherSnow, Language::ZhCn) => "雪",
            (Self::WeatherSnow, Language::En) => "Snow",
            (Self::WeatherRain, Language::ZhCn) => "雨",
            (Self::WeatherRain, Language::En) => "Rain",
            (Self::WeatherLeaves, Language::ZhCn) => "落叶",
            (Self::WeatherLeaves, Language::En) => "Leaves",
            (Self::SystemQuestItemGained, Language::ZhCn) => "获得任务物品 {} ×{}",
            (Self::SystemQuestItemGained, Language::En) => "Quest item gained: {} ×{}",
            (Self::SystemQuestItemLost, Language::ZhCn) => "失去任务物品 {} ×{}",
            (Self::SystemQuestItemLost, Language::En) => "Quest item lost: {} ×{}",
            (Self::PetModeFollow, Language::ZhCn) => "跟随",
            (Self::PetModeFollow, Language::En) => "Follow",
            (Self::PetModeRest, Language::ZhCn) => "休息",
            (Self::PetModeRest, Language::En) => "Rest",
            (Self::PetModeChanged, Language::ZhCn) => "宠物模式：{}",
            (Self::PetModeChanged, Language::En) => "Pet mode: {}",
            (Self::NoPetHint, Language::ZhCn) => "没有宠物，无法切换模式。",
            (Self::NoPetHint, Language::En) => "No pet to command.",
            (Self::QuestItemsScrollHint, Language::ZhCn) => "滚轮翻页",
            (Self::QuestItemsScrollHint, Language::En) => "Wheel to page",
            (Self::PetMode, Language::ZhCn) => "宠物模式",
            (Self::PetMode, Language::En) => "Pet mode",
            (Self::GuildAllyTab, Language::ZhCn) => "联盟",
            (Self::GuildAllyTab, Language::En) => "Allies",
            (Self::GuildAllyName, Language::ZhCn) => "盟友行会名",
            (Self::GuildAllyName, Language::En) => "Ally guild",
            (Self::GuildAllyRequest, Language::ZhCn) => "申请结盟",
            (Self::GuildAllyRequest, Language::En) => "Request ally",
            (Self::GuildAllyAccept, Language::ZhCn) => "同意",
            (Self::GuildAllyRemove, Language::ZhCn) => "解除",
            (Self::FriendRemove | Self::GuildAllyRemove, Language::En) => "Remove",
            (Self::GuildAllyEmpty, Language::ZhCn) => "暂无盟友（客户端本地记录）",
            (Self::GuildAllyEmpty, Language::En) => "No allies (local record)",
            (Self::GuildAllyPending, Language::ZhCn) => "申请中",
            (Self::GuildAllyPending, Language::En) => "Pending",
            (Self::GuildAllyHint, Language::ZhCn) => "命令：/ally request|accept|remove <行会名>",
            (Self::GuildAllyHint, Language::En) => "Command: /ally request|accept|remove <guild>",
            (Self::Friends, Language::ZhCn) => "好友",
            (Self::Friends, Language::En) => "Friends",
            (Self::FriendName, Language::ZhCn) => "名字",
            (Self::FriendMemo, Language::ZhCn) => "备注",
            (Self::FriendMemo, Language::En) => "Memo",
            (Self::FriendBlocked, Language::ZhCn) => "黑名单",
            (Self::FriendBlocked, Language::En) => "Block",
            (Self::FriendEmpty, Language::ZhCn) => "好友列表为空：输入名字后点「添加」",
            (Self::FriendEmpty, Language::En) => "No friends yet — enter a name and Add",
            (Self::FriendAdd, Language::ZhCn) => "添加",
            (Self::FriendAdd, Language::En) => "Add",
            (Self::FriendSaveMemo, Language::ZhCn) => "保存备注",
            (Self::FriendSaveMemo, Language::En) => "Save memo",
            (Self::FriendHint, Language::ZhCn) => "F 关闭 · 回车提交当前输入",
            (Self::FriendHint, Language::En) => "F closes · Enter submits the focused field",
            (Self::ConquestWar, Language::ZhCn) => "攻城进行中",
            (Self::ConquestWar, Language::En) => "Siege in progress",
            (Self::ConquestOwner, Language::ZhCn) => "归属",
            (Self::ConquestOwner, Language::En) => "Owner",
            (Self::ConquestControlPoints, Language::ZhCn) => "控制点",
            (Self::ConquestControlPoints, Language::En) => "Control points",
            (Self::ConquestResult, Language::ZhCn) => "攻城结束",
            (Self::ConquestResult, Language::En) => "Siege ended",
            (Self::ConquestWinner, Language::ZhCn) => "胜者",
            (Self::ConquestWinner, Language::En) => "Winner",
            (Self::ConquestGate, Language::ZhCn) => "城门",
            (Self::ConquestGate, Language::En) => "Gate",
            (Self::ConquestWall, Language::ZhCn) => "城墙",
            (Self::ConquestWall, Language::En) => "Wall",
            (Self::ConquestArcher, Language::ZhCn) => "箭塔",
            (Self::ConquestArcher, Language::En) => "Archer tower",
            (Self::ConquestDestroyed, Language::ZhCn) => "被摧毁",
            (Self::ConquestDestroyed, Language::En) => "Destroyed",
            (Self::ConquestGateOpen, Language::ZhCn) => "城门已开",
            (Self::ConquestGateOpen, Language::En) => "Gate open",
            (Self::GuildAllyLeader, Language::ZhCn) => "会长",
            (Self::Reconnecting, Language::ZhCn) => "连接已断开，正在重连（{}/3）",
            (Self::Reconnecting, Language::En) => "Disconnected, reconnecting ({}/3)",
            (Self::MinimapGateTip, Language::ZhCn) => "传送点：{} ({},{})",
            (Self::MinimapGateTip, Language::En) => "Teleport: {} ({},{})",
            (Self::ConquestUnclaimed, Language::ZhCn) => "无归属",
            (Self::ConquestUnclaimed, Language::En) => "Unclaimed",
            (Self::SystemConquestStarted, Language::ZhCn) => "攻城开始：{}",
            (Self::SystemConquestStarted, Language::En) => "Siege started: {}",
            (Self::SystemConquestEnded, Language::ZhCn) => "攻城结束：行会 {} 获胜",
            (Self::SystemConquestEnded, Language::En) => "Siege ended: guild {} wins",
            (Self::SystemConquestObjectDestroyed, Language::ZhCn) => "{} 被摧毁",
            (Self::SystemConquestObjectDestroyed, Language::En) => "{} destroyed",
        }
    }

    /// 便捷方法：`Label::Hp.text(language)` 的短写。
    #[must_use]
    pub const fn of(self, language: Language) -> &'static str {
        self.text(language)
    }
}

/// 中英对照的小工具：`(zh, en)` → 当前语言。
const fn pair(language: Language, zh: &'static str, en: &'static str) -> &'static str {
    match language {
        Language::ZhCn => zh,
        Language::En => en,
    }
}

/// 技能名（技能表现浮字用；只收录客户端特效相关的技能，未收录返回 `None`）。
#[must_use]
pub fn spell_name(spell: mir_core::enums::Spell, language: Language) -> Option<&'static str> {
    use mir_core::enums::Spell;
    let (zh, en) = match spell {
        Spell::FireWall => ("火墙", "Fire Wall"),
        Spell::ThunderBolt => ("雷电术", "Thunder Bolt"),
        Spell::ThunderStorm => ("地狱雷光", "Thunder Storm"),
        Spell::Lightning => ("疾光电影", "Lightning"),
        Spell::Teleport => ("瞬息移动", "Teleport"),
        Spell::Blink => ("瞬移", "Blink"),
        Spell::Hiding | Spell::MassHiding => ("隐身术", "Hiding"),
        Spell::SummonSkeleton => ("召唤骷髅", "Summon Skeleton"),
        Spell::SummonShinsu => ("召唤神兽", "Summon Shinsu"),
        Spell::Poisoning => ("施毒术", "Poisoning"),
        Spell::PoisonCloud => ("毒云", "Poison Cloud"),
        Spell::Healing | Spell::MassHealing => ("治愈术", "Healing"),
        Spell::MagicShield => ("魔法盾", "Magic Shield"),
        Spell::FireBall | Spell::GreatFireBall => ("火球术", "Fire Ball"),
        _ => return None,
    };
    Some(pair(language, zh, en))
}

/// 职业名（组队面板与以后的玩家信息面板用）。
#[must_use]
pub fn class_name(class: mir_core::enums::MirClass, language: Language) -> &'static str {
    use mir_core::enums::MirClass;
    let (zh, en) = match class {
        MirClass::Warrior => ("战士", "Warrior"),
        MirClass::Wizard => ("法师", "Wizard"),
        MirClass::Taoist => ("道士", "Taoist"),
        MirClass::Assassin => ("刺客", "Assassin"),
        MirClass::Archer => ("弓箭手", "Archer"),
    };
    pair(language, zh, en)
}

/// 物品类型名（tooltip 用；`ItemType` 的 43 个成员全部覆盖）。
#[must_use]
pub fn item_type_name(item_type: ItemType, language: Language) -> &'static str {
    let (zh, en) = match item_type {
        ItemType::Nothing => ("杂物", "Misc"),
        ItemType::Weapon => ("武器", "Weapon"),
        ItemType::Armour => ("衣服", "Armour"),
        ItemType::Helmet => ("头盔", "Helmet"),
        ItemType::Necklace => ("项链", "Necklace"),
        ItemType::Bracelet => ("手镯", "Bracelet"),
        ItemType::Ring => ("戒指", "Ring"),
        ItemType::Amulet => ("护身符", "Amulet"),
        ItemType::Belt => ("腰带", "Belt"),
        ItemType::Boots => ("靴子", "Boots"),
        ItemType::Stone => ("宝石", "Stone"),
        ItemType::Torch => ("火把", "Torch"),
        ItemType::Potion => ("药品", "Potion"),
        ItemType::Ore => ("矿石", "Ore"),
        ItemType::Meat => ("肉类", "Meat"),
        ItemType::CraftingMaterial => ("材料", "Material"),
        ItemType::Scroll => ("卷轴", "Scroll"),
        ItemType::Gem => ("宝石", "Gem"),
        ItemType::Mount => ("坐骑", "Mount"),
        ItemType::Book => ("技能书", "Skill book"),
        ItemType::Script => ("脚本", "Script"),
        ItemType::Reins => ("缰绳", "Reins"),
        ItemType::Bells => ("铃铛", "Bells"),
        ItemType::Saddle => ("马鞍", "Saddle"),
        ItemType::Ribbon => ("丝带", "Ribbon"),
        ItemType::Mask => ("面具", "Mask"),
        ItemType::Food => ("食物", "Food"),
        ItemType::Hook => ("鱼钩", "Hook"),
        ItemType::Float => ("浮漂", "Float"),
        ItemType::Bait => ("鱼饵", "Bait"),
        ItemType::Finder => ("探鱼器", "Finder"),
        ItemType::Reel => ("卷线器", "Reel"),
        ItemType::Fish => ("鱼", "Fish"),
        ItemType::Quest => ("任务物品", "Quest item"),
        ItemType::Awakening => ("觉醒材料", "Awakening"),
        ItemType::Pets => ("宠物物品", "Pet item"),
        ItemType::Transform => ("变身物品", "Transform"),
        ItemType::Deco => ("装饰", "Decoration"),
        ItemType::Socket => ("镶嵌", "Socket"),
        ItemType::MonsterSpawn => ("怪物刷新", "Monster spawn"),
        ItemType::SiegeAmmo => ("攻城弹药", "Siege ammo"),
        ItemType::SealedHero => ("封印英雄", "Sealed hero"),
    };
    pair(language, zh, en)
}

/// 装备槽名（纸娃娃面板与 tooltip 用）。
#[must_use]
pub fn equipment_slot_name(slot: EquipmentSlot, language: Language) -> &'static str {
    let (zh, en) = match slot {
        EquipmentSlot::Weapon => ("武器", "Weapon"),
        EquipmentSlot::Armour => ("衣服", "Armour"),
        EquipmentSlot::Helmet => ("头盔", "Helmet"),
        EquipmentSlot::Torch => ("火把", "Torch"),
        EquipmentSlot::Necklace => ("项链", "Necklace"),
        EquipmentSlot::BraceletL => ("左手镯", "L. Bracelet"),
        EquipmentSlot::BraceletR => ("右手镯", "R. Bracelet"),
        EquipmentSlot::RingL => ("左戒指", "L. Ring"),
        EquipmentSlot::RingR => ("右戒指", "R. Ring"),
        EquipmentSlot::Amulet => ("护身符", "Amulet"),
        EquipmentSlot::Belt => ("腰带", "Belt"),
        EquipmentSlot::Boots => ("靴子", "Boots"),
        EquipmentSlot::Stone => ("宝石", "Stone"),
        EquipmentSlot::Mount => ("坐骑", "Mount"),
    };
    pair(language, zh, en)
}

/// 需求属性名（tooltip 的「需求」行）。
#[must_use]
pub fn required_type_name(required: RequiredType, language: Language) -> &'static str {
    let (zh, en) = match required {
        RequiredType::Level => ("等级", "Level"),
        RequiredType::MaxAC => ("防御上限", "Max AC"),
        RequiredType::MaxMAC => ("魔防上限", "Max MAC"),
        RequiredType::MaxDC => ("攻击上限", "Max DC"),
        RequiredType::MaxMC => ("魔法上限", "Max MC"),
        RequiredType::MaxSC => ("道术上限", "Max SC"),
        RequiredType::MaxLevel => ("最高等级", "Max level"),
        RequiredType::MinAC => ("防御下限", "Min AC"),
        RequiredType::MinMAC => ("魔防下限", "Min MAC"),
        RequiredType::MinDC => ("攻击下限", "Min DC"),
        RequiredType::MinMC => ("魔法下限", "Min MC"),
        RequiredType::MinSC => ("道术下限", "Min SC"),
    };
    pair(language, zh, en)
}

/// 增益名；未收录的增益返回 `None`（调用方退化成 `增益 #n`）。
#[must_use]
pub fn buff_name(buff: BuffType, language: Language) -> Option<&'static str> {
    let (zh, en) = match buff {
        BuffType::None => ("无", "None"),
        BuffType::TemporalFlux => ("时空领域", "Temporal Flux"),
        BuffType::Hiding => ("隐身", "Hiding"),
        BuffType::Haste => ("加速", "Haste"),
        BuffType::SwiftFeet => ("疾风步", "Swift Feet"),
        BuffType::Fury => ("狂暴", "Fury"),
        BuffType::SoulShield => ("灵魂盾", "Soul Shield"),
        BuffType::BlessedArmour => ("神圣战甲", "Blessed Armour"),
        BuffType::LightBody => ("轻身术", "Light Body"),
        BuffType::UltimateEnhancer => ("强化术", "Ultimate Enhancer"),
        BuffType::ProtectionField => ("保护领域", "Protection Field"),
        BuffType::Rage => ("愤怒", "Rage"),
        BuffType::Curse => ("诅咒", "Curse"),
        BuffType::MoonLight => ("月光", "Moon Light"),
        BuffType::DarkBody => ("暗影体", "Dark Body"),
        BuffType::Concentration => ("专注", "Concentration"),
        BuffType::VampireShot => ("吸血箭", "Vampire Shot"),
        BuffType::PoisonShot => ("毒箭", "Poison Shot"),
        BuffType::CounterAttack => ("反击", "Counter Attack"),
        BuffType::MentalState => ("精神集中", "Mental State"),
        BuffType::EnergyShield => ("能量盾", "Energy Shield"),
        BuffType::MagicBooster => ("魔力强化", "Magic Booster"),
        BuffType::PetEnhancer => ("宠物强化", "Pet Enhancer"),
        BuffType::ImmortalSkin => ("不灭之躯", "Immortal Skin"),
        BuffType::MagicShield => ("魔法盾", "Magic Shield"),
        BuffType::ElementalBarrier => ("元素结界", "Elemental Barrier"),
        BuffType::Blindness => ("失明", "Blindness"),
        BuffType::GameMaster => ("管理员", "Game Master"),
        BuffType::General => ("将军", "General"),
        BuffType::Exp => ("经验加成", "EXP"),
        BuffType::Drop => ("掉落加成", "Drop"),
        BuffType::Gold => ("金币加成", "Gold"),
        BuffType::BagWeight => ("负重", "Bag Weight"),
        BuffType::Transform => ("变身", "Transform"),
        BuffType::Lover => ("恋人", "Lover"),
        BuffType::Mentee => ("徒弟", "Mentee"),
        _ => return None,
    };
    Some(pair(language, zh, en))
}

/// 品级颜色（tooltip 物品名）：无品级为普通白，越高越亮。
#[must_use]
pub const fn grade_color(grade: ItemGrade) -> [u8; 4] {
    match grade {
        ItemGrade::None => [236, 240, 246, 255],
        ItemGrade::Common => [180, 220, 255, 255],
        ItemGrade::Rare => [120, 220, 120, 255],
        ItemGrade::Legendary => [255, 200, 80, 255],
        ItemGrade::Mythical => [255, 140, 80, 255],
        ItemGrade::Heroic => [240, 120, 255, 255],
    }
}

/// 千位分隔（`12345` → `12,345`）。
#[must_use]
pub fn format_number(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    let offset = digits.len() % 3;
    for (index, character) in digits.chars().enumerate() {
        if index != 0 && index % 3 == offset {
            out.push(',');
        }
        out.push(character);
    }
    out
}

/// `金币 1,234` 之类的文案。
#[must_use]
pub fn gold_text(language: Language, gold: u32) -> String {
    format!(
        "{} {}",
        Label::Gold.text(language),
        format_number(u64::from(gold))
    )
}

/// `元宝 1,234` 之类的文案（状态栏余额；商城页签标题也用元宝）。
#[must_use]
pub fn credit_text(language: Language, credit: u32) -> String {
    format!(
        "{} {}",
        Label::Credit.text(language),
        format_number(u64::from(credit))
    )
}

/// 未知物品的占位名：`未知物品 #12`。
#[must_use]
pub fn unknown_item(language: Language, item_index: i32) -> String {
    format!("{} #{item_index}", Label::UnknownItem.text(language))
}

/// 把 `Label` 模板里的 `{}` 依次替换成参数（编译期 `&'static str` 只能存模板）。
#[must_use]
pub fn fill_template(template: &str, args: &[&str]) -> String {
    let mut out = template.to_owned();
    for arg in args {
        let Some(index) = out.find("{}") else {
            break;
        };
        out.replace_range(index..index + 2, arg);
    }
    out
}

/// 客户端合成的系统聊天消息 → 玩家语言文案。
///
/// 服务端下发的聊天（`S.Chat` / `S.ObjectChat` / `S.MarketSuccess`）已经由服务端
/// 按玩家语言翻译好，客户端**原样显示**；这里只处理客户端自己拼的消息（拾取金币/
/// 交易/组队/行会上下线等），模板在 [`Label`] 里中英各一份。
#[must_use]
pub fn system_chat_text(message: &SystemChat, language: Language) -> String {
    match message {
        SystemChat::GoldGained { gold } => fill_template(
            Label::SystemGoldGained.text(language),
            &[&format_number(u64::from(*gold))],
        ),
        SystemChat::ItemGained {
            name,
            item_index,
            count,
        } => {
            let name = name
                .clone()
                .unwrap_or_else(|| unknown_item(language, *item_index));
            fill_template(
                Label::SystemItemGained.text(language),
                &[&name, &count.to_string()],
            )
        }
        SystemChat::QuestShared {
            sharer,
            name,
            quest_index,
        } => {
            let name = name.clone().unwrap_or_else(|| {
                fill_template(
                    Label::QuestFallback.text(language),
                    &[&quest_index.to_string()],
                )
            });
            fill_template(Label::SystemQuestShared.text(language), &[sharer, &name])
        }
        SystemChat::GuildGold {
            name,
            amount,
            deposit,
        } => {
            let label = if *deposit {
                Label::SystemGuildGoldDeposit
            } else {
                Label::SystemGuildGoldWithdraw
            };
            fill_template(
                label.text(language),
                &[name, &format_number(u64::from(*amount))],
            )
        }
        SystemChat::TradeComplete => Label::SystemTradeComplete.text(language).to_owned(),
        SystemChat::TradeCancelled => Label::SystemTradeCancelled.text(language).to_owned(),
        SystemChat::GroupLeft => Label::SystemGroupLeft.text(language).to_owned(),
        SystemChat::GroupMemberJoined { name } => {
            fill_template(Label::SystemGroupJoined.text(language), &[name])
        }
        SystemChat::GroupMemberLeft { name } => {
            fill_template(Label::SystemGroupMemberLeft.text(language), &[name])
        }
        SystemChat::GuildMemberOnline { name } => {
            fill_template(Label::SystemGuildMemberOnline.text(language), &[name])
        }
        SystemChat::GuildMemberJoined { name } => {
            fill_template(Label::SystemGuildMemberJoined.text(language), &[name])
        }
        SystemChat::GuildMemberKicked { name } => {
            fill_template(Label::SystemGuildMemberKicked.text(language), &[name])
        }
        SystemChat::GuildMemberLeft { name } => {
            fill_template(Label::SystemGuildMemberLeft.text(language), &[name])
        }
        SystemChat::QuestItemGained {
            name,
            item_index,
            count,
        } => {
            let name = name
                .clone()
                .unwrap_or_else(|| unknown_item(language, *item_index));
            fill_template(
                Label::SystemQuestItemGained.text(language),
                &[&name, &count.to_string()],
            )
        }
        SystemChat::QuestItemLost {
            name,
            item_index,
            count,
        } => {
            let name = name
                .clone()
                .unwrap_or_else(|| unknown_item(language, *item_index));
            fill_template(
                Label::SystemQuestItemLost.text(language),
                &[&name, &count.to_string()],
            )
        }
        SystemChat::PetModeChanged { mode } => fill_template(
            Label::PetModeChanged.text(language),
            &[pet_mode_label(*mode).text(language)],
        ),
        SystemChat::NoPet => Label::NoPetHint.text(language).to_owned(),
        SystemChat::ConquestStarted { name } => {
            fill_template(Label::SystemConquestStarted.text(language), &[name])
        }
        SystemChat::ConquestEnded { name: _, winner } => fill_template(
            Label::SystemConquestEnded.text(language),
            &[&winner.to_string()],
        ),
        SystemChat::ConquestObjectDestroyed { kind } => fill_template(
            Label::SystemConquestObjectDestroyed.text(language),
            &[conquest_object_name(*kind, language)],
        ),
    }
}

/// 一条聊天记录的显示文本（服务端文本原样，客户端系统消息按语言渲染）。
#[must_use]
pub fn chat_line_text(line: &ChatLine, language: Language) -> String {
    let body = match &line.system {
        Some(message) => system_chat_text(message, language),
        None => line.text.clone(),
    };
    match &line.speaker {
        Some(speaker) => format!("[{speaker}] {body}"),
        None => body,
    }
}

/// 连接阶段的状态文案（HUD 在还没有自己的角色时显示）。
#[must_use]
pub const fn net_status_label(status: NetStatus, language: Language) -> &'static str {
    match status {
        NetStatus::Connecting => Label::Connecting.text(language),
        NetStatus::Verifying => Label::StatusVerifying.text(language),
        NetStatus::LoggingIn => Label::StatusLoggingIn.text(language),
        NetStatus::SelectingCharacter => Label::StatusSelectingCharacter.text(language),
        NetStatus::StartingGame => Label::StatusStartingGame.text(language),
        NetStatus::InGame => Label::StatusInGame.text(language),
        NetStatus::Closed => Label::Disconnected.text(language),
        NetStatus::Failed => Label::StatusFailed.text(language),
    }
}

/// 昼夜文案（`S.TimeOfDay`；`Normal` 与 `Day` 都按「白天」显示）。
#[must_use]
pub const fn light_label(setting: LightSetting) -> Label {
    match setting {
        LightSetting::Normal | LightSetting::Day => Label::PhaseDay,
        LightSetting::Dawn => Label::PhaseDawn,
        LightSetting::Evening => Label::PhaseEvening,
        LightSetting::Night => Label::PhaseNight,
    }
}

/// 天气文案（`S.MapChanged.weather`）。
#[must_use]
pub const fn weather_label(setting: WeatherSetting) -> Label {
    match setting {
        WeatherSetting::None => Label::WeatherClear,
        WeatherSetting::Fog => Label::WeatherFog,
        WeatherSetting::RedEmber | WeatherSetting::WhiteEmber | WeatherSetting::YellowEmber => {
            Label::WeatherEmber
        }
        WeatherSetting::FireParticle => Label::WeatherFire,
        WeatherSetting::Snow => Label::WeatherSnow,
        WeatherSetting::Rain => Label::WeatherRain,
        WeatherSetting::Leaves | WeatherSetting::FireyLeaves | WeatherSetting::PurpleLeaves => {
            Label::WeatherLeaves
        }
    }
}

/// 攻城器械类型名（`S.ConquestObject.kind`）。
#[must_use]
pub const fn conquest_object_name(
    kind: mir_protocol::packets::server::ConquestObjectType,
    language: Language,
) -> &'static str {
    use mir_protocol::packets::server::ConquestObjectType;
    match kind {
        ConquestObjectType::Gate => Label::ConquestGate.text(language),
        ConquestObjectType::Wall => Label::ConquestWall.text(language),
        ConquestObjectType::Archer => Label::ConquestArcher.text(language),
    }
}

/// 攻城状态文案（`S.ChangePMode` / `C.ChangePMode`）：
/// `Both` = 跟随、`AttackOnly` = 攻击、`None` = 休息；其余值按跟随显示。
#[must_use]
pub const fn pet_mode_label(mode: PetMode) -> Label {
    match mode {
        PetMode::AttackOnly => Label::PetModeAttack,
        PetMode::None => Label::PetModeRest,
        // `Both` / `MoveOnly` / `FocusMasterTarget` 都按「跟随」显示。
        _ => Label::PetModeFollow,
    }
}

/// 聊天行颜色（按频道）。
#[must_use]
pub const fn chat_color(kind: ChatType) -> [u8; 4] {
    match kind {
        ChatType::System | ChatType::System2 | ChatType::Hint => [255, 230, 120, 255],
        ChatType::Announcement => [255, 160, 80, 255],
        ChatType::Shout | ChatType::Shout2 | ChatType::Shout3 => [255, 120, 120, 255],
        ChatType::WhisperIn | ChatType::WhisperOut => [255, 160, 230, 255],
        ChatType::Group => [120, 200, 255, 255],
        ChatType::Guild => [140, 240, 150, 255],
        ChatType::LevelUp => [255, 220, 80, 255],
        ChatType::Trainer | ChatType::Mentor => [200, 180, 255, 255],
        ChatType::Relationship => [255, 180, 210, 255],
        ChatType::LineMessage => [220, 220, 220, 255],
        ChatType::Normal => [240, 240, 240, 255],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chinese_is_the_default_and_english_switches_labels() {
        assert_eq!(Label::Hp.text(Language::ZhCn), "生命");
        assert_eq!(Label::Hp.text(Language::En), "HP");
        assert_eq!(Label::Inventory.text(Language::ZhCn), "背包");
        assert_eq!(Label::Shop.text(Language::En), "Shop");
    }

    #[test]
    fn numbers_use_thousands_separators() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(1_000), "1,000");
        assert_eq!(format_number(12_345_678), "12,345,678");
        assert_eq!(gold_text(Language::ZhCn, 4_321), "金币 4,321");
        assert_eq!(credit_text(Language::ZhCn, 5_000), "元宝 5,000");
        assert_eq!(credit_text(Language::En, 250), "Credits 250");
    }

    #[test]
    fn chat_colors_differ_by_channel() {
        assert_ne!(
            chat_color(ChatType::System),
            chat_color(ChatType::Normal),
            "系统消息应与普通聊天不同色"
        );
        assert_ne!(chat_color(ChatType::Shout), chat_color(ChatType::Group));
    }

    #[test]
    fn unknown_items_are_labelled_with_the_template_index() {
        assert_eq!(unknown_item(Language::ZhCn, 7), "未知物品 #7");
        assert_eq!(unknown_item(Language::En, 7), "Unknown item #7");
    }

    #[test]
    fn item_type_slot_and_buff_names_are_localized() {
        assert_eq!(item_type_name(ItemType::Weapon, Language::ZhCn), "武器");
        assert_eq!(item_type_name(ItemType::Weapon, Language::En), "Weapon");
        assert_eq!(item_type_name(ItemType::Potion, Language::ZhCn), "药品");
        assert_eq!(
            equipment_slot_name(EquipmentSlot::RingR, Language::ZhCn),
            "右戒指"
        );
        assert_eq!(
            equipment_slot_name(EquipmentSlot::RingR, Language::En),
            "R. Ring"
        );
        assert_eq!(
            buff_name(BuffType::MagicShield, Language::ZhCn),
            Some("魔法盾")
        );
        assert_eq!(
            buff_name(BuffType::MagicShield, Language::En),
            Some("Magic Shield")
        );
        assert_eq!(buff_name(BuffType::HornedArcherBuff, Language::ZhCn), None);
        assert_eq!(
            required_type_name(RequiredType::Level, Language::ZhCn),
            "等级"
        );
        assert_ne!(grade_color(ItemGrade::Rare), grade_color(ItemGrade::None));
    }

    #[test]
    fn every_label_has_both_languages() {
        // `Label::ALL` 是手工维护的清单：数量对不上说明加了变体没同步。
        assert_eq!(Label::ALL.len(), 329, "新增 Label 后请同步 Label::ALL");
        for label in Label::ALL {
            let zh = label.text(Language::ZhCn);
            let en = label.text(Language::En);
            assert!(!zh.is_empty(), "{label:?} 缺中文");
            assert!(!en.is_empty(), "{label:?} 缺英文");
            assert!(
                !en.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
                "{label:?} 英文里混进中文：{en}"
            );
        }
    }

    #[test]
    fn new_ui_labels_have_both_languages() {
        for label in [
            Label::Equipment,
            Label::Equip,
            Label::Use,
            Label::Type,
            Label::Require,
            Label::Durability,
            Label::Stack,
            Label::Attack,
            Label::Defence,
            Label::MagicDefence,
            Label::MagicAttack,
            Label::Taoism,
            Label::Accuracy,
            Label::Agility,
            Label::Totals,
            Label::Buffs,
            Label::Miss,
            Label::Critical,
            Label::WorldHint,
            Label::InventoryTooltipHint,
            Label::Minimap,
            Label::NoMinimap,
            Label::MinimapHint,
            Label::NoBigmap,
            Label::BigmapHint,
            Label::Quests,
            Label::QuestDiary,
            Label::QuestHint,
            Label::QuestInProgress,
            Label::QuestComplete,
            Label::QuestAvailable,
            Label::Abandon,
            Label::TimeLeft,
            Label::Storage,
            Label::Capacity,
            Label::StorageHint,
            Label::Mail,
            Label::Inbox,
            Label::Compose,
            Label::Sender,
            Label::Subject,
            Label::Message,
            Label::Date,
            Label::Attachment,
            Label::Collect,
            Label::Lock,
            Label::Unlock,
            Label::Delete,
            Label::Send,
            Label::Recipient,
            Label::Postage,
            Label::Inquiry,
            Label::Stamp,
            Label::Back,
            Label::NoMail,
            Label::MailHint,
            Label::Guild,
            Label::Members,
            Label::GuildStorageTab,
            Label::GuildBuffsTab,
            Label::Notice,
            Label::GuildFunds,
            Label::Rank,
            Label::Online,
            Label::Offline,
            Label::MapColumn,
            Label::Promote,
            Label::Demote,
            Label::Kick,
            Label::Save,
            Label::Activate,
            Label::Purchase,
            Label::Deposit,
            Label::Withdraw,
            Label::Amount,
            Label::NoGuild,
            Label::GuildHint,
            Label::Available,
            Label::Obtained,
            Label::Expired,
            Label::MinutesLeft,
            Label::NoQuests,
            Label::Points,
            Label::Unread,
            Label::Collected,
            Label::NoBody,
            Label::NoSubjectField,
            Label::Market,
            Label::ItemColumn,
            Label::Consign,
            Label::Auction,
            Label::Search,
            Label::Refresh,
            Label::Seller,
            Label::Expiry,
            Label::Buy,
            Label::Bid,
            Label::BidAmount,
            Label::ConsignPrice,
            Label::StartingBid,
            Label::ListItem,
            Label::GetBack,
            Label::SellNow,
            Label::CollectSold,
            Label::PrevPage,
            Label::NextPage,
            Label::NoListings,
            Label::MarketHint,
            Label::ConsignHint,
            Label::AuctionHint,
            Label::MarketFailGeneric,
            Label::MarketFailDead,
            Label::MarketFailNoDialog,
            Label::MarketFailSold,
            Label::MarketFailExpired,
            Label::MarketFailNoSpace,
            Label::MarketFailOwnItem,
            Label::MarketFailLowGold,
            Label::MarketFailTooFar,
            Label::MarketFailCannotGainGold,
            Label::MarketFailBidTooLow,
            Label::MarketFailAuctionEnded,
            Label::ConsignSuccess,
            Label::ConsignFailed,
            Label::NoSelectedItem,
            Label::ConsignSelected,
            Label::GameShopPickHint,
            Label::WindowTitleMap,
            Label::WindowTitleLogin,
            Label::WindowTitleLive,
            Label::StatusVerifying,
            Label::StatusLoggingIn,
            Label::StatusSelectingCharacter,
            Label::StatusStartingGame,
            Label::StatusInGame,
            Label::StatusFailed,
            Label::NetworkThreadExited,
            Label::LoginThreadExited,
            Label::SystemGoldGained,
            Label::SystemItemGained,
            Label::SystemQuestShared,
            Label::SystemGuildGoldDeposit,
            Label::SystemGuildGoldWithdraw,
            Label::SystemTradeComplete,
            Label::SystemTradeCancelled,
            Label::SystemGroupLeft,
            Label::SystemGroupJoined,
            Label::SystemGroupMemberLeft,
            Label::SystemGuildMemberOnline,
            Label::SystemGuildMemberJoined,
            Label::SystemGuildMemberKicked,
            Label::SystemGuildMemberLeft,
            Label::QuestFallback,
            Label::ConnectUnreachable,
            Label::ConnectCredentials,
            Label::ConnectRefused,
            Label::ConnectTimeout,
            Label::ConnectVersionMismatch,
            Label::ConnectFailed,
            Label::QuestItems,
            Label::PhaseDawn,
            Label::PhaseDay,
            Label::PhaseEvening,
            Label::PhaseNight,
            Label::WeatherClear,
            Label::WeatherFog,
            Label::WeatherEmber,
            Label::WeatherFire,
            Label::WeatherSnow,
            Label::WeatherRain,
            Label::WeatherLeaves,
            Label::SystemQuestItemGained,
            Label::SystemQuestItemLost,
            Label::PetModeFollow,
            Label::PetModeAttack,
            Label::PetModeRest,
            Label::PetModeChanged,
            Label::NoPetHint,
            Label::QuestItemsScrollHint,
            Label::PetMode,
        ] {
            let zh = label.text(Language::ZhCn);
            let en = label.text(Language::En);
            assert!(!zh.is_empty(), "{label:?} 缺中文");
            assert!(!en.is_empty(), "{label:?} 缺英文");
            assert_ne!(zh, en, "{label:?} 中英文不应相同");
        }
    }
}
