//! 界面文案表：中文优先，`--lang en` 切到英文。
//!
//! 只有几十个固定标签，用穷举 `match` 比加载语言包更省事，也不会漏翻。玩家/物品/地图
//! 之类的动态文本来自服务端，不在这里。

use mir_core::Language;
use mir_core::enums::{BuffType, ChatType, EquipmentSlot, ItemGrade, ItemType, RequiredType};

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
            (Self::Shop, Language::En) => "Shop",
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
            (Self::Price, Language::En) => "Price",
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
            (Self::Attack, Language::ZhCn) => "攻击",
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
            (Self::TimeLeft, Language::ZhCn) => "剩余",
            (Self::TimeLeft, Language::En) => "Left",
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
            (Self::Delete, Language::ZhCn) => "删除",
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
            (Self::Accept, Language::En) => "Accept",
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
            (Self::GroupLeader, Language::En) => "Leader",
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

/// 未知物品的占位名：`未知物品 #12`。
#[must_use]
pub fn unknown_item(language: Language, item_index: i32) -> String {
    format!("{} #{item_index}", Label::UnknownItem.text(language))
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
        ] {
            let zh = label.text(Language::ZhCn);
            let en = label.text(Language::En);
            assert!(!zh.is_empty(), "{label:?} 缺中文");
            assert!(!en.is_empty(), "{label:?} 缺英文");
            assert_ne!(zh, en, "{label:?} 中英文不应相同");
        }
    }
}
