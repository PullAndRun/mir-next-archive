//! 客户端 HUD / 面板状态与绘制入口。
//!
//! 这个模块把「服务器快照 + 鼠标键盘」翻译成画面和 [`UiAction`]：
//!
//! * [`hud`]：顶部状态栏、左下聊天面板（含输入行）、增益栏数据来自 [`buffs`]；
//! * [`inventory`]：46 格背包、金币、选中/悬停、拖拽幽灵与右键菜单；
//! * [`equipment`]：`E` 打开的纸娃娃面板（14 个装备槽 + 属性汇总）；
//! * [`tooltip`]：鼠标悬停物品的提示框（跟随鼠标并在屏幕内收边）；
//! * [`worldtext`]：实体头顶名字（描边）与伤害飘字；
//! * [`shop`]：NPC 商店货架与价格（支持滚轮/上下键滚动）；
//! * [`dialog`]：NPC 对话页与 `<文本/@跳转键>` 按钮；
//! * [`quests`]：`Q` 任务面板（进度/限时/放弃/完成高亮）；
//! * [`storage`]：个人仓库 80 格（服务端 `S.NPCStorage` 自动打开，与背包拖拽存取）；
//! * [`mail`]：`N` 邮件面板（收件箱/阅读/写信，`C.ReadMail`/`C.CollectParcel`/`C.SendMail`）；
//! * [`guild`]：`G` 行会面板（成员/仓库/增益/公告，112 格行会仓库）；
//! * [`trade`]：交易面板（`S.TradeAccept` 自动打开，左右两栏各 10 格 + 金币 + 准备/确认）；
//! * [`group`]：`P` 组队面板（成员表 + 邀请/离队/踢人 + `!!` 组队聊天提示）；
//! * [`notice`]：交易/组队邀请的两按钮弹窗；
//! * [`grid`] / [`input`]：上面三个面板共用的物品网格与单行输入框；
//! * [`skills`]：F3 技能列表与 1..9 施法目标选择；
//! * [`labels`]：中文/英文标签表。
//!
//! UI 不直接依赖网络层：点击产生的 [`UiAction`] 由 `app.rs` 映射成
//! [`crate::net::NetCommand`]。布局函数与命中测试共用同一份矩形，所以单元测试可以在
//! 不开窗口的情况下验证按钮位置。
//!
//! # 可注入时钟
//!
//! 伤害飘字与增益倒计时都用 [`UiState::time_millis`]（Unix 毫秒）。窗口模式每帧用
//! [`UiState::set_time`] 写入真实时间；无头测试直接写任意时刻，于是可以在不 sleep 的
//! 情况下断言「500 ms 内存在、2 s 后消失」。

pub mod bigmap;
pub mod buffs;
pub mod dialog;
pub mod equipment;
pub mod grid;
pub mod group;
pub mod guild;
pub mod hud;
pub mod input;
pub mod inventory;
pub mod labels;
pub mod mail;
pub mod market;
pub mod minimap;
pub mod notice;
pub mod quests;
pub mod select;
pub mod settings;
pub mod shop;
pub mod skills;
pub mod spellfx;
pub mod storage;
pub mod target;
pub mod text;
pub mod tooltip;
pub mod trade;
pub mod worldtext;

use std::collections::HashMap;

use mir_core::Point;
use mir_core::enums::{
    ItemType, MarketCollectionMode, MarketPanelType, MirDirection, MirGridType, Spell,
};
use mir_render::{Frame, Image, MapScene};

use crate::geom::Rect;
use crate::net::{InventoryItem, WorldEntity, WorldSnapshot};
use crate::ui::dialog::ParsedPage;
pub use crate::ui::text::UiText;
use mir_core::Language;

/// 面板绘制上下文（快照 + 文字 + 语言）。
#[derive(Clone, Copy)]
pub struct PanelContext<'a> {
    /// 世界快照。
    pub snapshot: &'a WorldSnapshot,
    /// 文字门面。
    pub text: &'a UiText,
    /// 界面语言。
    pub language: Language,
}

/// 一次点击/按键产生的界面动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiAction {
    /// 点 NPC 按钮（`C.CallNPC`）；`key = "@exit"` 表示关窗。
    CallNpc {
        /// NPC 对象 ID。
        object_id: u32,
        /// 脚本页键。
        key: String,
    },
    /// 购买商店物品（`C.BuyItem`，数量 1）。
    BuyItem {
        /// 商店物品唯一编号。
        unique_id: u64,
    },
    /// 施法（`C.Magic`）。
    CastMagic {
        /// 技能。
        spell: Spell,
        /// 朝向。
        direction: MirDirection,
        /// 目标对象 ID。
        target_id: u32,
        /// 目标格。
        location: Point,
    },
    /// 重新绑定技能快捷键（`C.MagicKey`）。
    BindMagic {
        /// 技能。
        spell: Spell,
        /// 新按键。
        key: u8,
        /// 旧按键。
        old_key: u8,
    },
    /// 使用背包物品（`C.UseItem`；双击或右键菜单「使用」）。
    UseItem {
        /// 物品唯一编号。
        unique_id: u64,
    },
    /// 丢弃背包物品（`C.DropItem`）。
    DropItem {
        /// 物品唯一编号。
        unique_id: u64,
        /// 数量。
        count: u16,
    },
    /// 拖拽物品（`C.MoveItem`）。
    ///
    /// `grid` 是拖拽**起点**网格、`to_grid` 是落点网格：
    /// * 背包内交换：`Inventory → Inventory`；
    /// * 背包 → 装备槽（穿戴）：`Inventory → Equipment`；
    /// * 装备槽 → 背包（脱下）：`Equipment → Inventory`。
    MoveItem {
        /// 起点网格。
        grid: MirGridType,
        /// 起点格。
        from: i32,
        /// 落点格。
        to: i32,
        /// 落点网格。
        to_grid: MirGridType,
    },
    /// 点地走路：`step = true` 走一步，`false` 只转身（见 [`walk_intent`]）。
    WorldMove {
        /// 方向。
        direction: MirDirection,
        /// 是否直接走（否则转身）。
        step: bool,
    },
    /// A* 路径的下一格（[`UiState::tick_path`] 产生）。
    ///
    /// 与 [`Self::WorldMove`] 分开：`WorldMove` 需要 `app.rs` 按当前朝向决定先转身，
    /// 而路径的每一步已经在寻路时确定了方向，直接发 `C.Walk`/`C.Run`。
    WalkStep {
        /// 方向。
        direction: MirDirection,
        /// 是否跑步（连续两格同向时）。
        run: bool,
    },
    /// 捡起脚下的地面物品（`C.PickUp`）。
    PickUp,
    /// 朝锁定目标攻击（`C.Attack`；目标相邻时由 [`UiState::attack_target`] 产生）。
    AttackTarget {
        /// 朝向目标的方向。
        direction: MirDirection,
    },
    /// 放弃任务（`C.AbandonQuest`）。
    AbandonQuest {
        /// 任务编号。
        quest_index: i32,
    },
    /// 阅读邮件（`C.ReadMail`；点邮件列表的一行时产生）。
    ReadMail {
        /// 邮件编号。
        mail_id: u64,
    },
    /// 领取邮件附件（`C.CollectParcel`）。
    CollectParcel {
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
    /// 删除邮件（`C.DeleteMail`）。
    DeleteMail {
        /// 邮件编号。
        mail_id: u64,
    },
    /// 发送邮件（`C.SendMail`）。
    SendMail {
        /// 收件人。
        name: String,
        /// 正文。
        message: String,
        /// 附带金币。
        gold: u32,
        /// 附件唯一编号（固定 5 格）。
        items_idx: [u64; 5],
        /// 是否贴邮票。
        stamped: bool,
    },
    /// 询邮费（`C.MailCost`）。
    MailCost {
        /// 附带金币。
        gold: u32,
        /// 附件唯一编号（固定 5 格）。
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
    GuildStorageItemChange {
        /// `0` 存入 / `1` 取回 / `2` 仓库内移动。
        type_: u8,
        /// 起点格。
        from: i32,
        /// 落点格。
        to: i32,
    },
    /// 行会资金存取（`C.GuildStorageGoldChange`）。
    GuildStorageGoldChange {
        /// `0` = 存入，其它 = 取出。
        type_: u8,
        /// 金额。
        amount: u32,
    },
    /// 行会增益（`C.GuildBuffUpdate`）。
    GuildBuffUpdate {
        /// `0` 请求列表 / `1` 购买 / `2` 激活。
        action: u8,
        /// 增益编号。
        id: i32,
    },
    /// 行会成员管理（`C.EditGuildMember`）。
    EditGuildMember {
        /// 变动类型。
        change_type: u8,
        /// 职位序号。
        rank_index: u8,
        /// 目标角色名。
        name: String,
        /// 职位名。
        rank_name: String,
    },
    /// 行会公告（`C.EditGuildNotice`）。
    EditGuildNotice {
        /// 公告行。
        notice: Vec<String>,
    },
    /// 请求行会公告/成员表（`C.RequestGuildInfo`）。
    RequestGuildInfo {
        /// `0` 公告 / `1` 成员表。
        type_: u8,
    },
    /// 发起交易（`C.TradeRequest`；`Y` 键或以后的右键菜单）。
    TradeRequest,
    /// 回应交易邀请（`C.TradeReply`）。
    TradeReply {
        /// `true` = 接受。
        accept: bool,
    },
    /// 背包物品拖进自己的交易格（`C.DepositTradeItem`）。
    TradeItem {
        /// 背包格。
        from: i32,
        /// 交易格。
        to: i32,
    },
    /// 交易格里的物品拖回背包（`C.RetrieveTradeItem`）。
    TradeTakeBack {
        /// 交易格。
        from: i32,
        /// 背包格。
        to: i32,
    },
    /// 放入金币（`C.TradeGold`）。
    TradeGold {
        /// 增量。
        amount: u32,
    },
    /// 准备 / 确认（`C.TradeConfirm`）。
    TradeConfirm {
        /// 是否锁定己方报价。
        locked: bool,
    },
    /// 取消交易（`C.TradeCancel`）。
    TradeCancel,
    /// 邀请入队（`C.AddMember`）。
    AddMember {
        /// 被邀请者名字。
        name: String,
    },
    /// 退队 / 踢人（`C.DelMember`）。
    DelMember {
        /// 目标名字（自己退队时是自己的名字）。
        name: String,
    },
    /// 回应组队邀请（`C.GroupInvite`）。
    GroupReply {
        /// `true` = 接受。
        accept: bool,
    },
    /// 开关「允许组队」（`C.SwitchGroup`）。
    SwitchGroup {
        /// 是否允许别人邀请自己。
        allow_group: bool,
    },
    /// 搜索市场（`C.MarketSearch`）。
    MarketSearch {
        /// 搜索关键字（空串 = 全部）。
        match_text: String,
        /// 物品类型过滤（当前 UI 恒为 `ItemType::Nothing`）。
        item_type: ItemType,
        /// 是否只看自己的挂单（寄售/拍卖页签）。
        user_mode: bool,
        /// 外观编号下限。
        min_shape: i16,
        /// 外观编号上限。
        max_shape: i16,
        /// 市场面板类型。
        panel: MarketPanelType,
    },
    /// 刷新市场列表（`C.MarketRefresh`）。
    MarketRefresh,
    /// 市场翻页（`C.MarketPage`）。
    MarketPage {
        /// 页码（0 起）。
        page: i32,
    },
    /// 买断寄售 / 拍卖出价（`C.MarketBuy`）。
    MarketBuy {
        /// 条目编号。
        auction_id: u64,
        /// 出价（寄售买断时服务端忽略）。
        bid_price: u32,
    },
    /// 拍卖提前成交（`C.MarketSellNow`）。
    MarketSellNow {
        /// 条目编号。
        auction_id: u64,
    },
    /// 下架 / 收款（`C.MarketGetBack`）。
    MarketGetBack {
        /// 收取模式：单条 / 所有已售出 / 所有已过期。
        mode: MarketCollectionMode,
        /// 条目编号（批量模式为 0）。
        auction_id: u64,
    },
    /// 上架寄售/拍卖（`C.ConsignItem`）。
    ConsignItem {
        /// 背包物品唯一编号。
        unique_id: u64,
        /// 一口价 / 起拍价。
        price: u32,
        /// 面板类型。
        panel: MarketPanelType,
    },
    /// 商城购买（`C.GameshopBuy`，包 133）。
    ///
    /// 商城页签里列表条目的 `auction_id` 就是商品 `GIndex`（户部契约），
    /// `p_type = 0` 用元宝结算、`1` 用金币结算。
    GameShopBuy {
        /// 商品编号（`MarketListing.auction_id`）。
        g_index: i32,
        /// 购买份数（1..=99）。
        quantity: u8,
        /// 结算货币：`0` 元宝 / `1` 金币。
        p_type: i32,
    },
    /// 设置面板动作（音量/语言/显示）；窗口层落实并写回设置文件，不发网络包。
    Settings(settings::SettingsAction),
}

impl UiAction {
    /// 纯数据动作 → 网络命令。
    ///
    /// `app.rs` 与无头测试共用这一份映射；NPC 对话、施法、绑定等需要额外本地状态
    /// （关闭面板、记录新键位、按当前朝向决定走/转）的动作返回 `None`，由 `app.rs` 处理。
    #[must_use]
    pub fn net_command(&self) -> Option<crate::net::NetCommand> {
        use crate::net::NetCommand;
        match self {
            Self::UseItem { unique_id } => Some(NetCommand::UseItem {
                unique_id: *unique_id,
            }),
            Self::DropItem { unique_id, count } => Some(NetCommand::DropItem {
                unique_id: *unique_id,
                count: *count,
            }),
            Self::MoveItem {
                grid,
                from,
                to,
                to_grid,
            } => Some(NetCommand::MoveItem {
                grid: *grid,
                from: *from,
                to: *to,
                to_grid: *to_grid,
            }),
            Self::BuyItem { unique_id } => Some(NetCommand::BuyItem {
                unique_id: *unique_id,
                count: 1,
            }),
            Self::AttackTarget { direction } => Some(NetCommand::Attack(*direction)),
            Self::AbandonQuest { quest_index } => Some(NetCommand::AbandonQuest {
                quest_index: *quest_index,
            }),
            Self::ReadMail { mail_id } => Some(NetCommand::ReadMail { mail_id: *mail_id }),
            Self::CollectParcel { mail_id } => {
                Some(NetCommand::CollectParcel { mail_id: *mail_id })
            }
            Self::LockMail { mail_id, lock } => Some(NetCommand::LockMail {
                mail_id: *mail_id,
                lock: *lock,
            }),
            Self::DeleteMail { mail_id } => Some(NetCommand::DeleteMail { mail_id: *mail_id }),
            Self::SendMail {
                name,
                message,
                gold,
                items_idx,
                stamped,
            } => Some(NetCommand::SendMail {
                name: name.clone(),
                message: message.clone(),
                gold: *gold,
                items_idx: *items_idx,
                stamped: *stamped,
            }),
            Self::MailCost {
                gold,
                items_idx,
                stamped,
            } => Some(NetCommand::MailCost {
                gold: *gold,
                items_idx: *items_idx,
                stamped: *stamped,
            }),
            Self::StoreItem { from, to } => Some(NetCommand::StoreItem {
                from: *from,
                to: *to,
            }),
            Self::TakeBackItem { from, to } => Some(NetCommand::TakeBackItem {
                from: *from,
                to: *to,
            }),
            Self::GuildStorageItemChange { type_, from, to } => {
                Some(NetCommand::GuildStorageItemChange {
                    type_: *type_,
                    from: *from,
                    to: *to,
                })
            }
            Self::GuildStorageGoldChange { type_, amount } => {
                Some(NetCommand::GuildStorageGoldChange {
                    type_: *type_,
                    amount: *amount,
                })
            }
            Self::GuildBuffUpdate { action, id } => Some(NetCommand::GuildBuffUpdate {
                action: *action,
                id: *id,
            }),
            Self::EditGuildMember {
                change_type,
                rank_index,
                name,
                rank_name,
            } => Some(NetCommand::EditGuildMember {
                change_type: *change_type,
                rank_index: *rank_index,
                name: name.clone(),
                rank_name: rank_name.clone(),
            }),
            Self::EditGuildNotice { notice } => Some(NetCommand::EditGuildNotice {
                notice: notice.clone(),
            }),
            Self::RequestGuildInfo { type_ } => {
                Some(NetCommand::RequestGuildInfo { type_: *type_ })
            }
            Self::TradeRequest => Some(NetCommand::TradeRequest),
            Self::TradeReply { accept } => Some(NetCommand::TradeReply { accept: *accept }),
            Self::TradeItem { from, to } => Some(NetCommand::TradeItem {
                from: *from,
                to: *to,
            }),
            Self::TradeTakeBack { from, to } => Some(NetCommand::TradeTakeBack {
                from: *from,
                to: *to,
            }),
            Self::TradeGold { amount } => Some(NetCommand::TradeGold { amount: *amount }),
            Self::TradeConfirm { locked } => Some(NetCommand::TradeConfirm { locked: *locked }),
            Self::TradeCancel => Some(NetCommand::TradeCancel),
            Self::AddMember { name } => Some(NetCommand::AddMember { name: name.clone() }),
            Self::DelMember { name } => Some(NetCommand::DelMember { name: name.clone() }),
            Self::GroupReply { accept } => Some(NetCommand::GroupReply { accept: *accept }),
            Self::SwitchGroup { allow_group } => Some(NetCommand::SwitchGroup {
                allow_group: *allow_group,
            }),
            Self::MarketSearch {
                match_text,
                item_type,
                user_mode,
                min_shape,
                max_shape,
                panel,
            } => Some(NetCommand::MarketSearch {
                match_text: match_text.clone(),
                item_type: *item_type,
                user_mode: *user_mode,
                min_shape: *min_shape,
                max_shape: *max_shape,
                panel: *panel,
            }),
            Self::MarketRefresh => Some(NetCommand::MarketRefresh),
            Self::MarketPage { page } => Some(NetCommand::MarketPage { page: *page }),
            Self::MarketBuy {
                auction_id,
                bid_price,
            } => Some(NetCommand::MarketBuy {
                auction_id: *auction_id,
                bid_price: *bid_price,
            }),
            Self::MarketSellNow { auction_id } => Some(NetCommand::MarketSellNow {
                auction_id: *auction_id,
            }),
            Self::MarketGetBack { mode, auction_id } => Some(NetCommand::MarketGetBack {
                mode: *mode,
                auction_id: *auction_id,
            }),
            Self::ConsignItem {
                unique_id,
                price,
                panel,
            } => Some(NetCommand::ConsignItem {
                unique_id: *unique_id,
                price: *price,
                panel: *panel,
            }),
            Self::GameShopBuy {
                g_index,
                quantity,
                p_type,
            } => Some(NetCommand::GameShopBuy {
                g_index: *g_index,
                quantity: *quantity,
                p_type: *p_type,
            }),
            Self::WalkStep { direction, run } => Some(if *run {
                NetCommand::Run(*direction)
            } else {
                NetCommand::Walk(*direction)
            }),
            Self::PickUp => Some(NetCommand::PickUp),
            _ => None,
        }
    }
}

/// 拖拽起点（背包格、装备槽、个人仓库格、行会仓库格、交易格）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragSource {
    /// 背包格。
    Inventory(usize),
    /// 装备槽。
    Equipment(usize),
    /// 个人仓库格（`C.StoreItem` / `C.TakeBackItem`）。
    Storage(usize),
    /// 行会仓库格（`C.GuildStorageItemChange`）。
    GuildStorage(usize),
    /// 自己的交易格（`C.DepositTradeItem` / `C.RetrieveTradeItem`）。
    Trade(usize),
}

impl DragSource {
    /// 起点网格（仓库走 `C.StoreItem`/`C.TakeBackItem`，交易走 `C.DepositTradeItem`，
    /// 都不经过 `C.MoveItem`，这里返回语义最接近的网格，仅供日志/诊断）。
    #[must_use]
    pub const fn grid(self) -> MirGridType {
        match self {
            Self::Inventory(_) | Self::Storage(_) | Self::GuildStorage(_) => MirGridType::Inventory,
            Self::Trade(_) => MirGridType::Trade,
            Self::Equipment(_) => MirGridType::Equipment,
        }
    }

    /// 起点下标。
    #[must_use]
    pub const fn slot(self) -> usize {
        match self {
            Self::Inventory(slot)
            | Self::Equipment(slot)
            | Self::Storage(slot)
            | Self::GuildStorage(slot)
            | Self::Trade(slot) => slot,
        }
    }
}

/// 正在拖拽的物品（用于画半透明幽灵）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DragState {
    /// 拖拽起点。
    pub source: DragSource,
    /// 物品模板号（取图标）。
    pub item_index: i32,
    /// 物品唯一编号。
    pub unique_id: u64,
    /// 堆叠数量（幽灵右下角显示）。
    pub count: u16,
}

/// 右键菜单里的一项动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextAction {
    /// 使用。
    Use,
    /// 穿到 `to` 号装备槽。
    Equip {
        /// 目标装备槽。
        to: usize,
    },
    /// 丢弃。
    Drop,
}

/// 右键菜单的一项（已本地化的文案）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextEntry {
    /// 动作。
    pub action: ContextAction,
    /// 文案。
    pub label: String,
}

/// 右键菜单状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextMenu {
    /// 物品所在位置。
    pub source: DragSource,
    /// 物品唯一编号。
    pub unique_id: u64,
    /// 物品模板号。
    pub item_index: i32,
    /// 堆叠数量。
    pub count: u16,
    /// 弹出位置（鼠标）。
    pub anchor: Point,
}

impl ContextMenu {
    /// 菜单项：使用 /（可装备时）装备 / 丢弃。
    #[must_use]
    pub fn entries(&self, snapshot: &WorldSnapshot, language: Language) -> Vec<ContextEntry> {
        let mut entries = vec![ContextEntry {
            action: ContextAction::Use,
            label: labels::Label::Use.text(language).to_owned(),
        }];
        if let Some(info) = snapshot.item_info(self.item_index) {
            if let Some(to) = equipment::equipment_slot_for_item(snapshot, info.item_type) {
                entries.push(ContextEntry {
                    action: ContextAction::Equip { to },
                    label: labels::Label::Equip.text(language).to_owned(),
                });
            }
        }
        entries.push(ContextEntry {
            action: ContextAction::Drop,
            label: labels::Label::Drop.text(language).to_owned(),
        });
        entries
    }
}

/// 点地走路的意图。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalkIntent {
    /// 朝目标的方向。
    pub direction: MirDirection,
    /// `true` = 直接走一步；`false` = 只转身。
    pub step: bool,
}

/// 根据「自己 → 目标格」算出走路意图。
///
/// 规则（不寻路）：目标与自己在同一行/列且相邻 → 直接走一步；否则朝目标方向——
/// 已经面向该方向就再走一步，否则先转身。按住左键由 `app.rs` 连续调用。
#[must_use]
pub fn walk_intent(snapshot: &WorldSnapshot, target: Point) -> Option<WalkIntent> {
    let me = snapshot.self_player.as_ref()?;
    if me.position == target {
        return None;
    }
    let direction = me.position.direction_to(target);
    let dx = (target.x - me.position.x).abs();
    let dy = (target.y - me.position.y).abs();
    let adjacent = dx.max(dy) == 1;
    let aligned = dx == 0 || dy == 0;
    let step = (adjacent && aligned) || direction == me.direction;
    Some(WalkIntent { direction, step })
}

/// 靠近锁定目标时最多连续走的步数（需求：最多 3 步后放弃）。
pub const APPROACH_STEPS: u8 = 3;
/// 靠近目标时每走一步的间隔：走动作本身的时长（`Frames.cs:159-166`，600 ms）。
pub const APPROACH_STEP_MS: i64 = 600;
/// 目标双击的判定窗口（与背包双击相同，`inventory::DOUBLE_CLICK_MS`）。
pub const TARGET_DOUBLE_CLICK_MS: i64 = inventory::DOUBLE_CLICK_MS;

/// 「靠近锁定目标」的进度。
///
/// 目标不在相邻格时，第一次攻击会变成「朝目标走一步」，并记下这里；之后每
/// [`APPROACH_STEP_MS`] 再走一步（[`UiState::tick_approach`]），走到相邻格就自动攻击，
/// 走满 [`APPROACH_STEPS`] 步还没到就放弃。
///
/// 对应 Crystal 的自动靠近：`CheckInput` 在目标超出 1 格时朝目标走
/// （`Client/MirScenes/GameScene.cs:11764-11773`），相邻时自动挥刀（`:11471-11509`）。
/// 靠近目标仍沿 `DirectionFromPoint` 的八方向走直线（A* 只用于点地走路，
/// 见 [`UiState::on_click_with_path`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Approach {
    /// 目标对象 ID。
    pub target_id: u32,
    /// 还要再走几步（0 表示下一步仍不相邻就放弃）。
    pub steps_left: u8,
    /// 下一步允许发出的时间（Unix 毫秒）。
    pub next_step_at: i64,
    /// `true` = 目标是地面物品（相邻时发 `C.PickUp`，不是攻击）。
    pub pickup: bool,
}

/// 一次 A* 点地走路的剩余路径。
///
/// 由 [`UiState::on_click_with_path`] 在点到空地时创建，[`UiState::tick_path`] 每
/// [`PATH_STEP_MS`] 取一步。`from`/`expected` 记录路径认为角色应该在的位置
/// （诊断/以后中断校验用，当前不阻塞发包：服务器回包慢时相当于输入缓冲）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkPath {
    /// 点击的目标格。
    pub goal: Point,
    /// 剩余方向（不含已经发出的第一步）。
    pub steps: Vec<MirDirection>,
    /// 下一步允许发出的时间（Unix 毫秒）。
    pub next_step_at: i64,
    /// 上一步的起点格。
    pub from: Point,
    /// 上一步的落点格（服务器确认后才会继续）。
    pub expected: Point,
    /// 是否跑步（下一段有两步同向时发 `C.Run`）。
    pub running: bool,
}

/// 点地走路每一步的间隔（毫秒）。
///
/// Walk 动作本身 6 帧 × 100 ms（`Frames.cs:158`）；跑步也是 600 ms 走两格
/// （`Frames.cs:159`），所以两者用同一个节拍，靠 `running` 决定发 `C.Walk` 还是 `C.Run`。
pub const PATH_STEP_MS: i64 = 600;

/// `Esc` 按顺序关面板的结果。
///
/// 顺序与 Crystal 的 `Closeall`/`Esc` 处理一致（`GameScene.cs:668-671,697`）：
/// 右键菜单 → 拖拽 → 大地图 → NPC 对话/商店 → 背包/装备/技能。聊天输入由窗口层先处理
/// （输入框在 `LiveState` 里，不在 [`UiState`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscapeResult {
    /// 关掉了大地图。
    Bigmap,
    /// 关掉了 NPC 对话/商店；调用方要给服务端发 `@exit`。
    Npc {
        /// 对话 NPC 的对象 ID（未知时为 0）。
        object_id: u32,
    },
    /// 关掉了交易窗；调用方要给服务端发 `C.TradeCancel`。
    Trade,
    /// 关掉了背包/装备/技能/右键菜单/拖拽。
    Panels,
    /// 没有可关的面板。
    Nothing,
}

impl EscapeResult {
    /// 是否真的关掉了什么（决定要不要重绘）。
    #[must_use]
    pub const fn closed(self) -> bool {
        !matches!(self, Self::Nothing)
    }
}

/// 正在编辑的输入框（写信页 / 行会公告 / 行会资金）。
///
/// 窗口层（`app.rs`）在 `UiState::focus` 非空时把字符键交给 [`UiState::type_char`]，
/// 这样同一套键盘事件既能聊天也能填表单。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldId {
    /// 写信页：收件人。
    MailRecipient,
    /// 写信页：主题（协议不发送，见 `ui::mail` 模块文档）。
    MailSubject,
    /// 写信页：附带金币。
    MailGold,
    /// 写信页：正文。
    MailMessage,
    /// 行会仓库：存取金额。
    GuildGold,
    /// 行会公告：输入一行公告。
    GuildNotice,
    /// 交易面板：要放入的金币。
    TradeGold,
    /// 组队面板：被邀请者名字。
    GroupName,
    /// 市场面板：搜索框。
    MarketSearch,
    /// 市场面板：寄售售价 / 拍卖出价。
    MarketPrice,
}

/// 界面状态：面板开关、选中项、鼠标位置、拖拽/右键菜单、飘字与物品图标缓存。
pub struct UiState {
    /// 界面语言。
    pub language: Language,
    /// 背包面板是否打开（`I`）。
    pub show_inventory: bool,
    /// 技能面板是否打开（`F3`）。
    pub show_skills: bool,
    /// 装备纸娃娃面板是否打开（`E`）。
    pub show_equipment: bool,
    /// 任务面板是否打开（`Q`）。
    pub show_quests: bool,
    /// 任务面板选中的任务编号。
    pub selected_quest: Option<i32>,
    /// 任务面板滚动行。
    pub quest_scroll: usize,
    /// 个人仓库面板是否打开（服务端 `S.NPCStorage` 自动打开）。
    pub show_storage: bool,
    /// 邮件面板是否打开（`N`）。
    pub show_mail: bool,
    /// 邮件页签（收件箱/写信）。
    pub mail_tab: mail::MailTab,
    /// 邮件列表页码（0 起）。
    pub mail_page: usize,
    /// 选中邮件的编号。
    pub mail_selected: Option<u64>,
    /// 写信页表单。
    pub mail_compose: mail::MailCompose,
    /// 行会面板是否打开（`G`）。
    pub show_guild: bool,
    /// 行会页签。
    pub guild_tab: guild::GuildTab,
    /// 行会成员表选中的行（展平下标）。
    pub guild_selected_member: Option<usize>,
    /// 行会成员表滚动行。
    pub guild_member_scroll: usize,
    /// 行会增益表滚动行。
    pub guild_buff_scroll: usize,
    /// 行会仓库的存取金额。
    pub guild_gold: input::TextField,
    /// 行会公告输入行。
    pub guild_notice: input::TextField,
    /// 交易面板是否打开（`S.TradeAccept` 自动打开，`S.TradeConfirm`/`S.TradeCancel` 关闭）。
    pub show_trade: bool,
    /// 自己是否已准备（Crystal `User.TradeLocked`，`UserObject.cs:24` 附近的客户端状态）。
    pub trade_locked: bool,
    /// 交易面板的金币输入框（`C.TradeGold` 的增量）。
    pub trade_gold: input::TextField,
    /// 待处理的交易邀请者名字（弹「XXX 请求与你交易」；`None` 表示没有弹窗）。
    pub trade_invite: Option<String>,
    /// 组队面板是否打开（`P`）。
    pub show_group: bool,
    /// 组队面板的邀请名字输入框（`C.AddMember`）。
    pub group_name: input::TextField,
    /// 待处理的组队邀请者名字（弹「XXX 邀请你组队」）。
    pub group_invite: Option<String>,
    /// 组队面板选中的成员行。
    pub group_selected: Option<usize>,
    /// 组队面板滚动行。
    pub group_scroll: usize,
    /// 市场面板是否打开（`S.NPCMarket` / `S.NPCConsign` 自动打开）。
    pub show_market: bool,
    /// 市场面板当前页签（市场/寄售/拍卖）。
    pub market_tab: MarketPanelType,
    /// 市场面板选中的条目编号。
    pub market_selected: Option<u64>,
    /// 市场搜索输入框。
    pub market_search: input::TextField,
    /// 市场寄售价格 / 出价输入框。
    pub market_price: input::TextField,
    /// 市场页内滚动行。
    pub market_scroll: usize,
    /// 市场当前页（0 起）。
    pub market_page: i32,
    /// 市场是否是从 `S.NPCConsign` 打开的「纯寄售」会话（NPC 页是 `[@CONSIGN]`，
    /// 此时服务端只接受上架，不接受搜索/翻页；切到其它页签要先 `@MARKET`）。
    pub market_consign_only: bool,
    /// 已被「切页签」清掉的提示版本：`fail_version` / `message_version` 不大于它时
    /// 不画底部红/绿字（避免切到商城页签还残留上一页的失败提示）。
    pub market_notice_dismissed: u64,
    /// 当前聚焦的输入框。
    pub focus: Option<FieldId>,
    /// 小地图面板是否打开（`M`）。
    pub show_minimap: bool,
    /// 大地图面板是否打开（`B`，Crystal `BigMapDialog.Toggle()`，`GameScene.cs:774`）。
    pub show_bigmap: bool,
    /// 设置面板是否打开（`O` / `F12`，Crystal `OptionDialog`）。
    pub show_settings: bool,
    /// 设置面板的可编辑状态（窗口层用设置文件初始化，每个动作写回文件）。
    pub settings: settings::SettingsState,
    /// 小地图渲染器（`resource/data/mmap.wzl`；缺素材时是空场景，画纯色占位）。
    pub minimap_scene: mir_render::MinimapScene,
    /// 鼠标位置（物理像素，与帧坐标一致）。
    pub mouse: Point,
    /// 锁定的目标对象 ID（Crystal `MapObject.TargetObjectID`，`MapObject.cs:34-44`）。
    pub target_object_id: Option<u32>,
    /// 朝锁定目标靠近的进度（见 [`Approach`]）。
    pub approach: Option<Approach>,
    /// 点地走路的 A* 剩余路径（见 [`WalkPath`]）。
    pub walk_path: Option<WalkPath>,
    /// 背包选中的格子。
    pub selected_slot: Option<usize>,
    /// 商店选中的货架下标。
    pub selected_goods: Option<usize>,
    /// 技能选中的行。
    pub selected_magic: Option<usize>,
    /// 当前解析后的 NPC 对话页（`None` 表示没有对话）。
    pub dialog: Option<ParsedPage>,
    /// 已同步的对话版本号。
    pub dialog_version: u64,
    /// 商店面板是否打开。
    pub shop_open: bool,
    /// 本次对话是否已被关掉（空页/`Esc`）；此时即使收到迟到的货架包也不再弹面板。
    pub shop_suppressed: bool,
    /// 已同步的货架版本号。
    pub goods_version: u64,
    /// 商店滚动位置（第一条可见货架的下标）。
    pub shop_scroll: usize,
    /// 对话 NPC 的对象 ID。
    pub npc_object_id: Option<u32>,
    /// 正在拖拽的物品。
    pub drag: Option<DragState>,
    /// 右键菜单。
    pub context_menu: Option<ContextMenu>,
    /// 客户端本地时钟（Unix 毫秒；伤害飘字与增益倒计时用）。
    pub time_millis: i64,
    /// 正在飘的伤害数字。
    pub damage_numbers: Vec<worldtext::DamageNumber>,
    /// 正在播放的短光效（`S.ObjectEffect`）。
    pub effects: Vec<buffs::ActiveEffect>,
    /// 已消费到的伤害事件编号。
    pub last_damage_id: u64,
    /// 已消费到的光效事件编号。
    pub last_effect_id: u64,
    /// `items.wzl` 图标缓存（`None` 表示该图像号没有素材）。
    pub icons: HashMap<u16, Option<Image>>,
    /// 上一次点击（双击判定）。
    last_click: Option<(i64, DragSource)>,
    /// 上一次点击的世界目标（双击判定：同一目标 `TARGET_DOUBLE_CLICK_MS` 内两次点击）。
    last_target_click: Option<(i64, u32)>,
    /// 已同步的仓库打开版本（`S.NPCStorage`，见 [`UiState::sync`]）。
    storage_open_version: u64,
    /// 已同步的写信页请求版本（`S.MailSendRequest`）。
    mail_send_request_version: u64,
    /// 已同步的交易打开版本（`S.TradeAccept`）。
    trade_open_version: u64,
    /// 已同步的交易内容版本（对方改报价时服务端会把双方解锁）。
    trade_version: u64,
    /// 已同步的交易邀请版本（`S.TradeRequest`）。
    trade_invite_version: u64,
    /// 已同步的组队邀请版本（`S.GroupInvite`）。
    group_invite_version: u64,
    /// 已同步的市场打开版本（`S.NPCMarket`）。
    market_open_version: u64,
    /// 已同步的寄售面板版本（`S.NPCConsign`）。
    market_consign_version: u64,
    /// 已同步的市场翻页版本（`S.NPCMarketPage`）。
    market_page_version: u64,
}

impl UiState {
    /// 新建界面状态。
    #[must_use]
    pub fn new(language: Language) -> Self {
        Self {
            language,
            show_inventory: false,
            show_skills: false,
            show_equipment: false,
            show_quests: false,
            selected_quest: None,
            quest_scroll: 0,
            show_storage: false,
            show_mail: false,
            mail_tab: mail::MailTab::Inbox,
            mail_page: 0,
            mail_selected: None,
            mail_compose: mail::MailCompose::default(),
            show_guild: false,
            guild_tab: guild::GuildTab::Members,
            guild_selected_member: None,
            guild_member_scroll: 0,
            guild_buff_scroll: 0,
            guild_gold: input::TextField::numeric(9),
            guild_notice: input::TextField::new(200),
            show_trade: false,
            trade_locked: false,
            trade_gold: input::TextField::numeric(9),
            trade_invite: None,
            show_group: false,
            group_name: input::TextField::new(12),
            group_invite: None,
            group_selected: None,
            group_scroll: 0,
            show_market: false,
            market_tab: MarketPanelType::Market,
            market_selected: None,
            market_search: input::TextField::new(20),
            market_price: input::TextField::numeric(9),
            market_scroll: 0,
            market_page: 0,
            market_consign_only: false,
            market_notice_dismissed: 0,
            focus: None,
            show_minimap: false,
            show_bigmap: false,
            show_settings: false,
            settings: settings::SettingsState::default(),
            minimap_scene: mir_render::MinimapScene::empty(),
            mouse: Point::new(0, 0),
            target_object_id: None,
            approach: None,
            walk_path: None,
            selected_slot: None,
            selected_goods: None,
            selected_magic: None,
            dialog: None,
            dialog_version: 0,
            shop_open: false,
            shop_suppressed: false,
            goods_version: 0,
            shop_scroll: 0,
            npc_object_id: None,
            drag: None,
            context_menu: None,
            time_millis: 0,
            damage_numbers: Vec::new(),
            effects: Vec::new(),
            last_damage_id: 0,
            last_effect_id: 0,
            icons: HashMap::new(),
            last_click: None,
            last_target_click: None,
            storage_open_version: 0,
            mail_send_request_version: 0,
            trade_open_version: 0,
            trade_version: 0,
            trade_invite_version: 0,
            group_invite_version: 0,
            market_open_version: 0,
            market_consign_version: 0,
            market_page_version: 0,
        }
    }

    /// 写入客户端时钟（窗口模式每帧调用；测试可直接写任意时刻）。
    pub fn set_time(&mut self, now_millis: i64) {
        self.time_millis = now_millis;
    }

    /// 跟随快照更新对话/商店/选中项，并消费新的伤害/光效事件。
    pub fn sync(&mut self, snapshot: &WorldSnapshot) {
        match &snapshot.npc_dialog {
            Some(dialog) if dialog.version != self.dialog_version => {
                self.dialog_version = dialog.version;
                if dialog.lines.is_empty() {
                    self.dialog = None;
                    self.shop_open = false;
                    self.selected_goods = None;
                    self.npc_object_id = None;
                    self.shop_suppressed = true;
                } else {
                    self.dialog = Some(ParsedPage::parse(&dialog.lines));
                    self.shop_suppressed = false;
                    if dialog.object_id.is_some() {
                        self.npc_object_id = dialog.object_id;
                    }
                }
            }
            Some(_) => {}
            None => {
                // 快照里没有对话时只收对话面板；商店由 `npc_goods` 自己的存在性决定
                // （假包注入、或服务器先发货架后发对话页时不会被误关）。
                self.dialog = None;
            }
        }
        match &snapshot.npc_goods {
            Some(goods) if goods.version != self.goods_version => {
                self.goods_version = goods.version;
                self.shop_open = !goods.is_empty() && !self.shop_suppressed;
                self.selected_goods = None;
                self.shop_scroll = 0;
            }
            Some(_) => {}
            None => {
                self.shop_open = false;
                self.selected_goods = None;
                self.shop_scroll = 0;
            }
        }
        // 仓库：`S.NPCStorage` 每来一次就打开一次（Crystal 的 `StorageDialog.Show()`，
        // `GameScene.cs:4256`）；离开仓库页后服务端不再发 `NPCStorage`，快照把
        // `storage_open` 置假，这里收面板。
        if snapshot.storage_open_version != self.storage_open_version {
            self.storage_open_version = snapshot.storage_open_version;
            self.show_storage = true;
            // 拖拽存取需要背包一起显示（Crystal 的仓库也是和背包并排用的）。
            self.show_inventory = true;
        } else if self.show_storage && !snapshot.storage_open {
            self.show_storage = false;
        }
        // 写信页：`S.MailSendRequest`（NPC 邮筒页）到达时自动打开写信页。
        if snapshot.mail_send_request_version != self.mail_send_request_version {
            self.mail_send_request_version = snapshot.mail_send_request_version;
            self.show_mail = true;
            self.mail_tab = mail::MailTab::Compose;
            self.mail_page = 0;
        }
        // 交易：`S.TradeAccept` 每来一次就打开一次面板（Crystal `TradeDialog.TradeAccept()`，
        // `TradeDialogs.cs:152-161`，连背包一起显示）；成交/取消时快照把 `open` 置假。
        if snapshot.trade.open_version != self.trade_open_version {
            self.trade_open_version = snapshot.trade.open_version;
            self.show_trade = snapshot.trade.open;
            if self.show_trade {
                self.show_inventory = true;
                self.trade_gold = input::TextField::numeric(9);
                self.trade_locked = snapshot.trade.locked;
            }
        }
        if !snapshot.trade.open {
            self.show_trade = false;
        }
        // 交易内容变化（对方改物品/金币）时服务端会把双方解锁（`PlayerObject.cs:10761-10786`）。
        if snapshot.trade.version != self.trade_version {
            self.trade_version = snapshot.trade.version;
            self.trade_locked = snapshot.trade.locked;
        }
        // 交易邀请（`S.TradeRequest`）→ 弹一次窗。
        if snapshot.trade.invitation_version != self.trade_invite_version {
            self.trade_invite_version = snapshot.trade.invitation_version;
            self.trade_invite.clone_from(&snapshot.trade.invitation);
        }
        // 组队邀请（`S.GroupInvite`）→ 弹一次窗。
        if snapshot.group.invitation_version != self.group_invite_version {
            self.group_invite_version = snapshot.group.invitation_version;
            self.group_invite.clone_from(&snapshot.group.invitation);
        }
        // 市场：`S.NPCMarket` 打开窗口并换一份新列表（Crystal `GameScene.NPCMarket`，
        // `GameScene.cs:5632-5643`）；`S.NPCConsign` 打开寄售页并拉自己的挂单。
        if snapshot.market.open_version != self.market_open_version {
            self.market_open_version = snapshot.market.open_version;
            // 从关闭状态打开时回到「市场」页签（Crystal `Show()` → `TMerchantDialog(Market)`，
            // `TrustMerchantDialog.cs:1420-1433`）；搜索/刷新回来的列表不重置页签。
            if !self.show_market {
                self.market_tab = MarketPanelType::Market;
                self.market_search = input::TextField::new(20);
            }
            self.show_market = snapshot.market.open;
            self.market_consign_only = false;
            self.market_page = 0;
            self.market_scroll = 0;
            self.market_selected = None;
            if self.show_market {
                // Crystal `TrustMerchantDialog.Show()` 会把背包一起摆出来（`:1435-1436`）。
                self.show_inventory = true;
            }
        }
        if snapshot.market.consign_open_version != self.market_consign_version {
            self.market_consign_version = snapshot.market.consign_open_version;
            self.show_market = true;
            self.market_tab = MarketPanelType::Consign;
            self.market_page = 0;
            self.market_scroll = 0;
            self.market_selected = None;
            self.market_price = input::TextField::numeric(9);
            self.market_consign_only = true;
            self.show_inventory = true;
        }
        if !snapshot.market.open {
            self.show_market = false;
        }
        // 翻页：Crystal 用 `(Listings.Count - 1) / 10` 算当前页（`GameScene.cs:5652`）。
        if snapshot.market.page_version != self.market_page_version {
            self.market_page_version = snapshot.market.page_version;
            self.market_page = snapshot
                .market
                .listings
                .len()
                .saturating_sub(1)
                .checked_div(crate::net::MARKET_PAGE_SIZE)
                .and_then(|page| i32::try_from(page).ok())
                .unwrap_or(0);
            self.market_scroll = 0;
        }
        // 市场选中项/页码/滚动收边。
        let market_pages = snapshot.market.pages.max(0);
        self.market_page = self
            .market_page
            .clamp(0, market_pages.saturating_sub(1).max(0));
        let market_scroll_max = market::max_scroll(
            snapshot.market.listings.len(),
            self.market_page,
            market::MAX_ROWS,
        );
        self.market_scroll = self.market_scroll.min(market_scroll_max);
        if self
            .market_selected
            .is_some_and(|auction_id| snapshot.market.listing(auction_id).is_none())
        {
            self.market_selected = None;
        }
        // 组队面板滚动/选中收边。
        let member_count = snapshot.group.members.len();
        self.group_scroll = self
            .group_scroll
            .min(member_count.saturating_sub(group::ROWS));
        if self
            .group_selected
            .is_some_and(|index| index >= member_count)
        {
            self.group_selected = None;
        }
        // 队伍解散后收起面板（Crystal 的 `GroupDialog` 也只在组队时才显示成员）。
        if member_count == 0 {
            self.group_selected = None;
            self.group_scroll = 0;
        }
        // 任务滚动/选中收边。
        let quest_scroll_max = snapshot.quests.len().saturating_sub(quests::MAX_QUEST_ROWS);
        self.quest_scroll = self.quest_scroll.min(quest_scroll_max);
        if self
            .selected_quest
            .is_some_and(|id| snapshot.quest(id).is_none())
        {
            self.selected_quest = None;
        }
        // 邮件页码/选中收边（`page_count` 至少为 1，所以 `- 1` 不会下溢）。
        self.mail_page = self
            .mail_page
            .min(mail::page_count(snapshot.mails.len()) - 1);
        if self
            .mail_selected
            .is_some_and(|mail_id| snapshot.mail(mail_id).is_none())
        {
            self.mail_selected = None;
        }
        // 行会成员表/增益表收边。
        let member_count = snapshot.guild_members().len();
        self.guild_member_scroll = self
            .guild_member_scroll
            .min(member_count.saturating_sub(guild::MEMBER_ROWS));
        if self
            .guild_selected_member
            .is_some_and(|index| index >= member_count)
        {
            self.guild_selected_member = None;
        }
        self.guild_buff_scroll = self.guild_buff_scroll.min(
            snapshot
                .guild_buff_infos
                .len()
                .saturating_sub(guild::BUFF_ROWS),
        );
        // 面板被关掉时丢掉对应的拖拽（不然幽灵图标会留在屏幕上）。
        if self.drag.as_ref().is_some_and(|drag| match drag.source {
            DragSource::Storage(_) => !self.show_storage,
            DragSource::GuildStorage(_) => {
                !(self.show_guild && self.guild_tab == guild::GuildTab::Storage)
            }
            _ => false,
        }) {
            self.drag = None;
        }
        if self
            .selected_slot
            .is_some_and(|slot| slot >= snapshot.inventory.len())
        {
            self.selected_slot = None;
        }
        if self.selected_goods.is_some_and(|index| {
            snapshot
                .npc_goods
                .as_ref()
                .is_none_or(|goods| index >= goods.list.len())
        }) {
            self.selected_goods = None;
        }
        if self
            .selected_magic
            .is_some_and(|index| index >= snapshot.magics.len())
        {
            self.selected_magic = None;
        }
        if self.drag.as_ref().is_some_and(|drag| match drag.source {
            DragSource::Inventory(slot) => snapshot.inventory_item(slot).is_none(),
            DragSource::Equipment(slot) => snapshot.equipment_item(slot).is_none(),
            DragSource::Storage(slot) => snapshot.storage_item(slot).is_none(),
            DragSource::GuildStorage(slot) => snapshot.guild_storage_item(slot).is_none(),
            DragSource::Trade(slot) => snapshot.trade.items.get(slot).is_none_or(Option::is_none),
        }) {
            self.drag = None;
        }
        // 目标死亡/离场后不再锁定（Crystal 在目标死亡时清 `TargetObjectID`，
        // `GameScene.cs:10485-10486`）。
        if self.target_object_id.is_some_and(|object_id| {
            snapshot
                .entities
                .get(&object_id)
                .is_none_or(|entity| entity.dead)
        }) {
            self.target_object_id = None;
            self.approach = None;
        }
        // 地面物品被捡走/消失后，靠近它的进度也作废（目标 ID 不在 `entities` 里）。
        if self
            .approach
            .as_ref()
            .is_some_and(|approach| !snapshot.entities.contains_key(&approach.target_id))
        {
            self.approach = None;
        }
        self.collect_world_events(snapshot);
        self.prune_world_events();
        self.clamp_shop_scroll(snapshot);
    }

    /// 消费新增的伤害/光效事件（快照会整体克隆，所以用递增 ID 去重）。
    fn collect_world_events(&mut self, snapshot: &WorldSnapshot) {
        let now = self.time_millis;
        for event in &snapshot.damage_events {
            if event.id <= self.last_damage_id {
                continue;
            }
            self.last_damage_id = event.id;
            if let Some(number) = worldtext::damage_from_event(event, snapshot, self.language, now)
            {
                self.damage_numbers.push(number);
            }
        }
        if self.damage_numbers.len() > 32 {
            let excess = self.damage_numbers.len() - 32;
            self.damage_numbers.drain(0..excess);
        }
        for event in &snapshot.effects {
            if event.id <= self.last_effect_id {
                continue;
            }
            self.last_effect_id = event.id;
            let time = if event.time == 0 {
                u32::try_from(buffs::DEFAULT_EFFECT_MS).unwrap_or(600)
            } else {
                event.time.clamp(200, 2_000)
            };
            self.effects.push(buffs::ActiveEffect {
                object_id: event.object_id,
                started: now,
                until: now.saturating_add(i64::from(time)),
                effect: event.effect,
            });
        }
        if self.effects.len() > 16 {
            let excess = self.effects.len() - 16;
            self.effects.drain(0..excess);
        }
    }

    /// 清掉过期的飘字与光效。
    fn prune_world_events(&mut self) {
        let now = self.time_millis;
        self.damage_numbers.retain(|number| !number.expired(now));
        self.effects.retain(|effect| !effect.expired(now));
    }

    /// 滚动位置收边到当前货架长度。
    fn clamp_shop_scroll(&mut self, snapshot: &WorldSnapshot) {
        let max_scroll = snapshot.npc_goods.as_ref().map_or(0, |goods| {
            goods.list.len().saturating_sub(shop::MAX_SHOP_ROWS)
        });
        self.shop_scroll = self.shop_scroll.min(max_scroll);
    }

    /// 关掉 NPC 对话与商店（`Esc` 用；调用方负责先发 `@exit`）。
    pub fn close_npc(&mut self) {
        self.dialog = None;
        self.shop_open = false;
        self.shop_suppressed = true;
        self.selected_goods = None;
        self.shop_scroll = 0;
        self.npc_object_id = None;
    }

    /// 关掉所有可切换面板。
    pub fn close_panels(&mut self) {
        self.show_inventory = false;
        self.show_skills = false;
        self.show_equipment = false;
        self.show_quests = false;
        self.show_storage = false;
        self.show_mail = false;
        self.show_guild = false;
        self.show_group = false;
        self.show_bigmap = false;
        self.show_settings = false;
        self.close_trade();
        self.close_market();
        self.focus = None;
        self.drag = None;
        self.context_menu = None;
        self.close_npc();
    }

    /// 开关任务面板（`Q`，Crystal `KeybindOptions.Quests` → `QuestLogDialog`，
    /// `GameScene.cs:660-664`）。
    pub fn toggle_quests(&mut self) {
        self.show_quests = !self.show_quests;
        if !self.show_quests {
            self.selected_quest = None;
        }
    }

    /// 开关邮件面板（`N`）。打开时回到收件箱；开关都清掉输入焦点。
    pub fn toggle_mail(&mut self) {
        self.show_mail = !self.show_mail;
        if self.show_mail {
            self.mail_tab = mail::MailTab::Inbox;
        }
        self.focus = None;
    }

    /// 开关行会面板（`G`，Crystal `KeybindOptions.Guilds` → `GuildDialog`，
    /// `GameScene.cs:648-655`）。
    pub fn toggle_guild(&mut self) {
        self.show_guild = !self.show_guild;
        if !self.show_guild {
            self.focus = None;
        }
    }

    /// 打开行会面板时要发的请求（Crystal `GuildDialog.Show()` 拉公告/成员/仓库/增益）。
    ///
    /// 窗口层在按键打开面板时把这些动作派发出去；无头测试可以直接断言。
    #[must_use]
    pub fn guild_open_requests() -> Vec<UiAction> {
        vec![
            UiAction::RequestGuildInfo { type_: 0 },
            UiAction::RequestGuildInfo { type_: 1 },
            UiAction::GuildStorageItemChange {
                type_: 3,
                from: 0,
                to: 0,
            },
            UiAction::GuildBuffUpdate { action: 0, id: 0 },
        ]
    }

    /// 开关组队面板（`P`，Crystal `KeybindOptions.Group = Keys.P`，
    /// `Client/KeyBindSettings.cs:225`、`GameScene.cs:717-719`）。
    pub fn toggle_group(&mut self) {
        self.show_group = !self.show_group;
        if !self.show_group {
            self.focus = None;
        }
    }

    /// 开关交易面板（`S.TradeAccept` 自动打开；也允许手动关掉，等同取消）。
    pub fn toggle_trade(&mut self) {
        self.show_trade = !self.show_trade;
        if !self.show_trade {
            self.focus = None;
        }
    }

    /// 关掉交易面板并清掉本地准备状态（`Esc`/取消按钮；调用方负责发 `C.TradeCancel`）。
    pub fn close_trade(&mut self) {
        self.show_trade = false;
        self.trade_locked = false;
        self.trade_gold = input::TextField::numeric(9);
        self.focus = None;
    }

    /// 关掉市场面板（本地关窗；Crystal 的 `TrustMerchantDialog.Hide()` 也不发包）。
    pub fn close_market(&mut self) {
        self.show_market = false;
        self.market_selected = None;
        self.market_scroll = 0;
        self.focus = None;
    }

    /// 开关小地图（`M`）。
    pub fn toggle_minimap(&mut self) {
        self.show_minimap = !self.show_minimap;
    }

    /// 开关大地图（`B`，Crystal `KeybindOptions.Bigmap` → `BigMapDialog.Toggle()`，
    /// `GameScene.cs:773-774`；`BigMapDialog.Show()` 在 `MapInfo.BigMap <= 0` 时直接返回，
    /// `BigMapDialog.cs:286-295`，本作改为显示纯色占位 + 地图名）。
    pub fn toggle_bigmap(&mut self) {
        self.show_bigmap = !self.show_bigmap;
    }

    /// 开关设置面板（`O`，Crystal `KeybindOptions.Options2` 的默认键，
    /// `Client/KeyBindSettings.cs:223-224`；`F12` 是同组的另一个默认键）。
    ///
    /// 打开时清掉输入焦点，避免上一次聊天/表单的焦点把字符键吃掉。
    pub fn toggle_settings(&mut self) {
        self.show_settings = !self.show_settings;
        self.focus = None;
        // 面板里的语言/音量显示与当前界面保持一致（窗口层每次打开前也会同步）。
        self.settings.language = self.language;
    }

    /// 鼠标是否落在大地图面板上（面板开着时吃掉点击，避免误点成走路）。
    #[must_use]
    pub fn bigmap_clicked(&self, frame: Rect) -> bool {
        self.show_bigmap && bigmap::panel_rect(frame).contains(self.mouse)
    }

    /// `Esc`：按「右键菜单 → 拖拽 → 大地图 → NPC 对话/商店 → 其它面板」的顺序关一层。
    ///
    /// 聊天输入不在这里（在窗口层的 `LiveState` 里，且窗口层在调用本方法前已经处理）。
    /// 返回值告诉调用方要不要发 `@exit`、要不要重绘。
    pub fn escape(&mut self) -> EscapeResult {
        if self.context_menu.is_some() {
            self.context_menu = None;
            return EscapeResult::Panels;
        }
        if self.drag.is_some() {
            self.drag = None;
            return EscapeResult::Panels;
        }
        // 设置面板是模态的，`Esc` 先收它（Crystal `OptionDialog.Hide()`）。
        if self.show_settings {
            self.show_settings = false;
            self.focus = None;
            return EscapeResult::Panels;
        }
        // 需求：`Esc` 关闭大地图（Crystal 的 `Closeall` 也会收起对话框，
        // `GameScene.cs:668-671,697`）。
        if self.show_bigmap {
            self.show_bigmap = false;
            return EscapeResult::Bigmap;
        }
        if self.dialog.is_some() || self.shop_open {
            let object_id = self.npc_object_id.unwrap_or(0);
            self.close_npc();
            return EscapeResult::Npc { object_id };
        }
        // 交易窗是「进行中的会话」，关窗等于取消（Crystal `CloseButton.Click → TradeCancel()`，
        // `TradeDialogs.cs:52-57`）。
        if self.show_trade {
            self.close_trade();
            return EscapeResult::Trade;
        }
        // 市场窗只是 NPC 窗口，关窗不发包（Crystal `Hide()`，`TrustMerchantDialog.cs:1398-1412`）。
        if self.show_market {
            self.close_market();
            return EscapeResult::Panels;
        }
        if self.show_inventory
            || self.show_skills
            || self.show_equipment
            || self.show_quests
            || self.show_storage
            || self.show_mail
            || self.show_guild
            || self.show_group
        {
            self.close_panels();
            return EscapeResult::Panels;
        }
        EscapeResult::Nothing
    }

    /// 鼠标是否落在小地图面板上（面板开着时吃掉点击，避免误点成走路）。
    #[must_use]
    pub fn minimap_clicked(&self, frame: Rect) -> bool {
        self.show_minimap && minimap::panel_rect(frame).contains(self.mouse)
    }

    /// 鼠标下的实体（悬停高亮用）；鼠标压在面板上时不算（Crystal 只在
    /// `MapControl` 是 `MouseControl` 时才更新 `MouseObject`）。
    #[must_use]
    pub fn hover_object<'a>(
        &self,
        snapshot: &'a WorldSnapshot,
        scene: &MapScene,
        text: &UiText,
        center: Point,
        frame: Rect,
    ) -> Option<&'a WorldEntity> {
        if self.pointer_over_panel(snapshot, text, frame) {
            return None;
        }
        let cell = scene.screen_to_cell(center, self.mouse);
        target::entity_at(snapshot, cell)
    }

    /// 鼠标下的地面物品（物品名/拾取用）；鼠标压在面板上时不算。
    #[must_use]
    pub fn hover_item<'a>(
        &self,
        snapshot: &'a WorldSnapshot,
        scene: &MapScene,
        text: &UiText,
        center: Point,
        frame: Rect,
    ) -> Option<&'a WorldEntity> {
        if self.pointer_over_panel(snapshot, text, frame) {
            return None;
        }
        let cell = scene.screen_to_cell(center, self.mouse);
        target::item_at(snapshot, cell)
    }

    /// 鼠标是否压在某个打开的面板上（tooltip/高亮/点地走路据此避让）。
    #[must_use]
    pub fn pointer_over_panel(&self, snapshot: &WorldSnapshot, text: &UiText, frame: Rect) -> bool {
        if self.invite_open() {
            return true;
        }
        if self.show_settings && settings::settings_layout(frame).panel.contains(self.mouse) {
            return true;
        }
        if self.show_bigmap && bigmap::panel_rect(frame).contains(self.mouse) {
            return true;
        }
        if self.minimap_clicked(frame) {
            return true;
        }
        if self.show_quests
            && quests::quest_layout(frame, snapshot.quests.len(), self.quest_scroll)
                .panel
                .contains(self.mouse)
        {
            return true;
        }
        if self.show_storage && storage::storage_layout(frame).panel.contains(self.mouse) {
            return true;
        }
        if self.show_mail
            && mail::mail_layout(
                frame,
                self.mail_tab,
                snapshot.mails.len(),
                self.mail_page,
                snapshot.inventory.len(),
            )
            .panel
            .contains(self.mouse)
        {
            return true;
        }
        if self.show_guild
            && guild::guild_layout(
                frame,
                snapshot.guild_members().len(),
                snapshot.guild_buff_infos.len(),
                self.guild_member_scroll,
                self.guild_buff_scroll,
            )
            .panel
            .contains(self.mouse)
        {
            return true;
        }
        if self.show_group
            && group::group_layout(frame, snapshot.group.members.len(), self.group_scroll)
                .panel
                .contains(self.mouse)
        {
            return true;
        }
        if self.show_market
            && market::market_layout(
                frame,
                self.market_tab,
                snapshot.market.listings.len(),
                self.market_page,
                self.market_scroll,
            )
            .panel
            .contains(self.mouse)
        {
            return true;
        }
        if self.show_trade && trade::trade_layout(frame).panel.contains(self.mouse) {
            return true;
        }
        if self.show_inventory && inventory::inventory_panel_rect(frame).contains(self.mouse) {
            return true;
        }
        if self.show_equipment
            && equipment::equipment_layout(frame)
                .panel
                .contains(self.mouse)
        {
            return true;
        }
        if self.show_skills
            && skills::skill_layout(frame, snapshot.magics.len())
                .panel
                .contains(self.mouse)
        {
            return true;
        }
        if self.shop_open {
            if let Some(goods) = &snapshot.npc_goods {
                let layout = shop::shop_layout(
                    frame,
                    goods.list.len(),
                    self.shop_scroll,
                    shop::shop_left(self.show_equipment),
                );
                if layout.panel.contains(self.mouse) {
                    return true;
                }
            }
        }
        if let Some(page) = &self.dialog {
            let layout =
                dialog::dialog_layout(frame, page, |label| text.text_width(label, hud::SIZE_BODY));
            if layout.panel.contains(self.mouse) {
                return true;
            }
        }
        false
    }

    /// 对锁定的目标发起一次攻击/靠近（`Space` 或双击目标）。
    ///
    /// * 目标相邻（切比雪夫距离 ≤ 1）→ [`UiAction::AttackTarget`]（`C.Attack`）；
    ///   对应 Crystal 相邻时自动挥刀（`GameScene.cs:11471-11509`）；
    /// * 否则朝目标走一步，并记下 [`Approach`]（最多 [`APPROACH_STEPS`] 步）；
    ///   对应 Crystal 目标超出 1 格时朝目标走（`:11764-11773`）。
    ///
    /// 没有锁定目标、目标已死或自己不在世界里时返回 `None`。
    pub fn attack_target(&mut self, snapshot: &WorldSnapshot) -> Option<UiAction> {
        let target_id = self.target_object_id?;
        let me = snapshot.self_player.as_ref()?;
        let target = snapshot.entities.get(&target_id)?;
        if target.dead {
            self.target_object_id = None;
            self.approach = None;
            return None;
        }
        let direction = me.position.direction_to(target.position);
        if me.position.in_range(target.position, 1) {
            self.approach = None;
            return Some(UiAction::AttackTarget { direction });
        }
        self.approach = Some(Approach {
            target_id,
            steps_left: APPROACH_STEPS.saturating_sub(1),
            next_step_at: self.time_millis.saturating_add(APPROACH_STEP_MS),
            pickup: false,
        });
        Some(UiAction::WorldMove {
            direction,
            step: true,
        })
    }

    /// 每帧推进「靠近目标」：到时间就走一步；相邻就自动攻击；步数用完就放弃。
    ///
    /// 由 `app.rs` 每个渲染节拍调用一次；无头测试直接拨 [`Self::time_millis`] 后调用。
    pub fn tick_approach(&mut self, snapshot: &WorldSnapshot) -> Option<UiAction> {
        let approach = self.approach?;
        if self.time_millis < approach.next_step_at {
            return None;
        }
        let Some(me) = snapshot.self_player.as_ref() else {
            self.approach = None;
            return None;
        };
        let Some(target) = snapshot
            .entities
            .get(&approach.target_id)
            .filter(|target| !target.dead)
        else {
            self.approach = None;
            return None;
        };
        if self.target_object_id != Some(approach.target_id) && !approach.pickup {
            self.approach = None;
            return None;
        }
        let direction = me.position.direction_to(target.position);
        if me.position.in_range(target.position, 1) {
            self.approach = None;
            return Some(if approach.pickup {
                UiAction::PickUp
            } else {
                UiAction::AttackTarget { direction }
            });
        }
        if approach.steps_left == 0 {
            // 走满 3 步还没到：放弃靠近（目标仍可重新点击/按 Space）。
            self.approach = None;
            return None;
        }
        self.approach = Some(Approach {
            steps_left: approach.steps_left - 1,
            next_step_at: self.time_millis.saturating_add(APPROACH_STEP_MS),
            ..approach
        });
        Some(UiAction::WorldMove {
            direction,
            step: true,
        })
    }

    /// 选中的背包物品。
    #[must_use]
    pub fn selected_inventory_item<'a>(
        &self,
        snapshot: &'a WorldSnapshot,
    ) -> Option<&'a InventoryItem> {
        snapshot.inventory_item(self.selected_slot?)
    }

    /// 滚轮：优先滚动最上层的面板（组队 → 任务 → 邮件 → 行会 → 商店），返回是否发生了变化。
    ///
    /// `delta > 0` 表示向下滚。
    pub fn on_wheel(&mut self, snapshot: &WorldSnapshot, delta: i32) -> bool {
        // 市场面板用「上一页/下一页」按钮翻页，滚轮不穿透到下面的商店/背包。
        if self.show_market {
            return false;
        }
        if self.show_group && self.scroll_group(snapshot, delta) {
            return true;
        }
        if self.show_quests && self.scroll_quests(snapshot, delta) {
            return true;
        }
        if self.show_mail && self.scroll_mail(snapshot, delta) {
            return true;
        }
        if self.show_guild && self.scroll_guild(snapshot, delta) {
            return true;
        }
        if !self.shop_open {
            return false;
        }
        self.scroll_shop(snapshot, delta.signum())
    }

    /// 滚动组队成员表（超过一屏 12 行时）。
    pub fn scroll_group(&mut self, snapshot: &WorldSnapshot, delta: i32) -> bool {
        if delta == 0 {
            return false;
        }
        let max_scroll = snapshot.group.members.len().saturating_sub(group::ROWS);
        if max_scroll == 0 {
            return false;
        }
        let before = self.group_scroll;
        self.group_scroll = if delta < 0 {
            self.group_scroll.saturating_sub(1)
        } else {
            self.group_scroll.saturating_add(1)
        }
        .min(max_scroll);
        self.group_scroll != before
    }

    /// 滚动任务列表。
    pub fn scroll_quests(&mut self, snapshot: &WorldSnapshot, delta: i32) -> bool {
        let max_scroll = snapshot.quests.len().saturating_sub(quests::MAX_QUEST_ROWS);
        if max_scroll == 0 {
            return false;
        }
        let before = self.quest_scroll;
        let step = usize::try_from(delta.abs()).unwrap_or(1).max(1);
        self.quest_scroll = if delta < 0 {
            self.quest_scroll.saturating_sub(step)
        } else {
            self.quest_scroll.saturating_add(step)
        }
        .min(max_scroll);
        self.quest_scroll != before
    }

    /// 翻邮件页（滚轮 / 上下键）。
    pub fn scroll_mail(&mut self, snapshot: &WorldSnapshot, delta: i32) -> bool {
        let pages = mail::page_count(snapshot.mails.len());
        if pages <= 1 || delta == 0 {
            return false;
        }
        let before = self.mail_page;
        self.mail_page = if delta < 0 {
            self.mail_page.saturating_sub(1)
        } else {
            self.mail_page.saturating_add(1)
        }
        .min(pages - 1);
        self.mail_page != before
    }

    /// 滚动行会成员/增益页（按当前页签）。
    pub fn scroll_guild(&mut self, snapshot: &WorldSnapshot, delta: i32) -> bool {
        if delta == 0 {
            return false;
        }
        match self.guild_tab {
            guild::GuildTab::Members => {
                let max_scroll = snapshot
                    .guild_members()
                    .len()
                    .saturating_sub(guild::MEMBER_ROWS);
                if max_scroll == 0 {
                    return false;
                }
                let before = self.guild_member_scroll;
                self.guild_member_scroll = if delta < 0 {
                    self.guild_member_scroll.saturating_sub(1)
                } else {
                    self.guild_member_scroll.saturating_add(1)
                }
                .min(max_scroll);
                self.guild_member_scroll != before
            }
            guild::GuildTab::Buffs => {
                let max_scroll = snapshot
                    .guild_buff_infos
                    .len()
                    .saturating_sub(guild::BUFF_ROWS);
                if max_scroll == 0 {
                    return false;
                }
                let before = self.guild_buff_scroll;
                self.guild_buff_scroll = if delta < 0 {
                    self.guild_buff_scroll.saturating_sub(1)
                } else {
                    self.guild_buff_scroll.saturating_add(1)
                }
                .min(max_scroll);
                self.guild_buff_scroll != before
            }
            _ => false,
        }
    }

    /// 上下键滚动商店（`delta` 为 +1 / -1）。
    pub fn scroll_shop(&mut self, snapshot: &WorldSnapshot, delta: i32) -> bool {
        let Some(goods) = &snapshot.npc_goods else {
            return false;
        };
        let max_scroll = goods.list.len().saturating_sub(shop::MAX_SHOP_ROWS);
        if max_scroll == 0 {
            return false;
        }
        let before = self.shop_scroll;
        let next = if delta < 0 {
            self.shop_scroll
                .saturating_sub(usize::try_from(-delta).unwrap_or(1))
        } else {
            self.shop_scroll
                .saturating_add(usize::try_from(delta).unwrap_or(1))
        };
        self.shop_scroll = next.min(max_scroll);
        self.shop_scroll != before
    }

    /// 按键施法：返回需要发送的动作（绑定 + 施法）。
    #[must_use]
    pub fn press_magic_key(&mut self, snapshot: &WorldSnapshot, key: u8) -> Vec<UiAction> {
        let Some(press) = skills::press_magic_key(snapshot, key) else {
            return Vec::new();
        };
        let mut actions = Vec::new();
        if let Some(bind) = press.bind {
            actions.push(UiAction::BindMagic {
                spell: bind.spell,
                key: bind.key,
                old_key: bind.old_key,
            });
        }
        actions.push(UiAction::CastMagic {
            spell: press.cast.spell,
            direction: press.cast.direction,
            target_id: press.cast.target_id,
            location: press.cast.location,
        });
        actions
    }

    /// 点击任务面板：选中任务或点「放弃」。
    ///
    /// 点在面板空白处只吞掉点击（返回 `None`），不会穿透成走路。
    fn click_quests(&mut self, snapshot: &WorldSnapshot, frame: Rect) -> Option<UiAction> {
        let layout = quests::quest_layout(frame, snapshot.quests.len(), self.quest_scroll);
        if !layout.panel.contains(self.mouse) {
            return None;
        }
        if let Some(index) = layout.abandon_at(self.mouse) {
            if let Some(quest) = snapshot.quests.get(index) {
                return Some(UiAction::AbandonQuest {
                    quest_index: quest.id,
                });
            }
        }
        if let Some(index) = layout.row_at(self.mouse) {
            if let Some(quest) = snapshot.quests.get(index) {
                self.selected_quest = Some(quest.id);
            }
        }
        None
    }

    /// 点击仓库面板：在仓库格上按下开始拖拽（松手时译成 `C.StoreItem`/`C.TakeBackItem`）。
    fn click_storage(&mut self, snapshot: &WorldSnapshot, frame: Rect) -> Option<UiAction> {
        let layout = storage::storage_layout(frame);
        if !layout.panel.contains(self.mouse) {
            return None;
        }
        if let Some(slot) = layout.grid.slot_at(self.mouse) {
            self.start_drag(snapshot, DragSource::Storage(slot));
        }
        None
    }

    /// 点击邮件面板：页签 / 列表行 / 阅读按钮 / 写信表单。
    fn click_mail(&mut self, snapshot: &WorldSnapshot, frame: Rect) -> Option<UiAction> {
        let layout = mail::mail_layout(
            frame,
            self.mail_tab,
            snapshot.mails.len(),
            self.mail_page,
            snapshot.inventory.len(),
        );
        if !layout.panel.contains(self.mouse) {
            return None;
        }
        if let Some(tab) = layout.tab_at(self.mouse) {
            self.mail_tab = if tab == 0 {
                mail::MailTab::Inbox
            } else {
                mail::MailTab::Compose
            };
            self.focus = None;
            return None;
        }
        match self.mail_tab {
            mail::MailTab::Inbox => {
                if layout.prev.contains(self.mouse) {
                    self.scroll_mail(snapshot, -1);
                    return None;
                }
                if layout.next.contains(self.mouse) {
                    self.scroll_mail(snapshot, 1);
                    return None;
                }
                if let Some(index) = layout.row_at(self.mouse) {
                    let mail = snapshot.mails.get(index)?;
                    self.mail_selected = Some(mail.mail_id);
                    // Crystal 点开一封邮件就发 `C.ReadMail`（`MailDialogs.cs:1098`），
                    // 服务端把 `Opened` 置真并重发整份列表。
                    return Some(UiAction::ReadMail {
                        mail_id: mail.mail_id,
                    });
                }
                let selected = self
                    .mail_selected
                    .and_then(|mail_id| snapshot.mail(mail_id));
                if layout.collect.contains(self.mouse) {
                    if let Some(mail) = selected {
                        if mail.can_collect() {
                            return Some(UiAction::CollectParcel {
                                mail_id: mail.mail_id,
                            });
                        }
                    }
                }
                if layout.lock.contains(self.mouse) {
                    if let Some(mail) = selected {
                        return Some(UiAction::LockMail {
                            mail_id: mail.mail_id,
                            lock: !mail.locked,
                        });
                    }
                }
                if layout.delete.contains(self.mouse) {
                    if let Some(mail) = selected {
                        if !mail.locked {
                            return Some(UiAction::DeleteMail {
                                mail_id: mail.mail_id,
                            });
                        }
                    }
                }
            }
            mail::MailTab::Compose => {
                for (rect, field) in [
                    (layout.recipient, FieldId::MailRecipient),
                    (layout.subject, FieldId::MailSubject),
                    (layout.gold, FieldId::MailGold),
                    (layout.message, FieldId::MailMessage),
                ] {
                    if rect.contains(self.mouse) {
                        self.focus = Some(field);
                        return None;
                    }
                }
                if layout.stamp.contains(self.mouse) {
                    self.mail_compose.stamped = !self.mail_compose.stamped;
                    return None;
                }
                for (index, rect) in layout.attachments.iter().enumerate() {
                    if rect.contains(self.mouse) {
                        self.mail_compose.attachments[index] = None;
                        return None;
                    }
                }
                if let Some(slot) = layout.pick_grid.slot_at(self.mouse) {
                    if let Some(item) = snapshot.inventory_item(slot) {
                        self.mail_compose.toggle_attachment(item.unique_id);
                    }
                    return None;
                }
                if layout.quote.contains(self.mouse) {
                    return Some(UiAction::MailCost {
                        gold: self.mail_compose.gold.number(),
                        items_idx: self.mail_compose.items_idx(),
                        stamped: self.mail_compose.stamped,
                    });
                }
                if layout.send.contains(self.mouse) {
                    if !self.mail_compose.recipient.is_empty() {
                        return Some(UiAction::SendMail {
                            name: self.mail_compose.recipient.value.clone(),
                            message: self.mail_compose.message.value.clone(),
                            gold: self.mail_compose.gold.number(),
                            items_idx: self.mail_compose.items_idx(),
                            stamped: self.mail_compose.stamped,
                        });
                    }
                    return None;
                }
                if layout.back.contains(self.mouse) {
                    self.mail_tab = mail::MailTab::Inbox;
                    self.focus = None;
                    return None;
                }
            }
        }
        None
    }

    /// 点击邀请弹窗（交易/组队）。弹窗是模态的：点弹窗外也算「拒绝」。
    fn click_invite(&mut self, frame: Rect) -> Option<UiAction> {
        let layout = notice::notice_layout(frame);
        if let Some(name) = self.trade_invite.clone() {
            if layout.accept.contains(self.mouse) {
                self.trade_invite = None;
                tracing::info!(partner = %name, "接受交易邀请");
                return Some(UiAction::TradeReply { accept: true });
            }
            if layout.reject.contains(self.mouse) {
                self.trade_invite = None;
                // 拒绝也要把快照里的邀请清掉，否则下一帧 `sync` 会再弹一次。
                tracing::info!(partner = %name, "拒绝交易邀请");
                return Some(UiAction::TradeReply { accept: false });
            }
            // 点在弹窗外：吃掉点击（Crystal 的 `MirMessageBox` 也是模态的）。
            return None;
        }
        if let Some(name) = self.group_invite.clone() {
            if layout.accept.contains(self.mouse) {
                self.group_invite = None;
                self.show_group = true;
                tracing::info!(inviter = %name, "接受组队邀请");
                return Some(UiAction::GroupReply { accept: true });
            }
            if layout.reject.contains(self.mouse) {
                self.group_invite = None;
                tracing::info!(inviter = %name, "拒绝组队邀请");
                return Some(UiAction::GroupReply { accept: false });
            }
            return None;
        }
        None
    }

    /// 打开市场/寄售面板后要发的请求（Crystal `TMerchantDialog(type)` 打开页签时都会
    /// 发一次 `C.MarketSearch`，`TrustMerchantDialog.cs:1160-1166`、`:1210-1216`）。
    ///
    /// `S.NPCMarket` 本身只带第一页（服务端 `GetMarket` 用的 `Min/MaxShapes` 还是 0），
    /// 所以客户端打开后要按当前页签再搜一次，才能拿到完整的在售/自己的挂单。
    #[must_use]
    pub fn market_open_requests(&self) -> Vec<UiAction> {
        if !self.show_market {
            return Vec::new();
        }
        vec![self.market_search_action()]
    }

    /// 商城购买数量：输入框夹到 `1..=99`，再按「5 组」上限夹一次。
    ///
    /// Crystal 在 `MirGameShopCell.cs:148-153` 这样算：
    /// `if ((decimal)(Quantity * Item.Count) / Item.Info.StackSize) > 5` 时把数量夹到
    /// `(5 * StackSize) / Count`；客户端先夹可以避免服务端对「超过 5 组」**静默丢弃**
    /// （Crystal 语义，户部实现一致）。列表条目不带库存（售罄条目直接不下发），
    /// 所以没有再按库存夹；模板缺失时只做 1..=99 的夹取。
    fn market_quantity(&self, snapshot: &WorldSnapshot, listing: &crate::net::MarketListing) -> u8 {
        let requested = self.market_price.number().clamp(1, 99);
        let per_purchase = u32::from(listing.count.max(1));
        let max_by_stacks = snapshot.item_info(listing.item_index).map_or(99, |info| {
            (5 * u32::from(info.stack_size.max(1)) / per_purchase).max(1)
        });
        u8::try_from(requested.min(max_by_stacks).clamp(1, 99)).unwrap_or(1)
    }

    /// 市场底部提示（失败原因 / 成功文案）。
    ///
    /// 切页签时会把 `market_notice_dismissed` 推到当时的版本号，于是上一页的提示
    /// 不再显示（新提示的版本号会更大）。返回 `(fail, message)` 交给 [`market::draw`]。
    #[must_use]
    pub fn market_notice(&self, snapshot: &WorldSnapshot) -> (Option<u8>, Option<String>) {
        let fail = if snapshot.market.fail_version > self.market_notice_dismissed {
            snapshot.market.fail
        } else {
            None
        };
        let message = if snapshot.market.message_version > self.market_notice_dismissed {
            snapshot.market.message.clone()
        } else {
            None
        };
        (fail, message)
    }

    /// 当前页签的搜索动作（关键字取搜索框；外观区间用 Crystal `MarketSearch` 的
    /// 缺省值 0..=5000，`Shared/ClientPackets.cs:1571`）。
    fn market_search_action(&self) -> UiAction {
        UiAction::MarketSearch {
            match_text: self.market_search.value.clone(),
            item_type: ItemType::Nothing,
            user_mode: market::is_user_mode(self.market_tab),
            min_shape: 0,
            max_shape: 5000,
            panel: self.market_tab,
        }
    }

    /// 点击市场面板：页签 / 搜索 / 刷新 / 翻页 / 选中行 / 买断-出价 / 取回 / 上架。
    fn click_market(&mut self, snapshot: &WorldSnapshot, frame: Rect) -> Option<UiAction> {
        let layout = market::market_layout(
            frame,
            self.market_tab,
            snapshot.market.listings.len(),
            self.market_page,
            self.market_scroll,
        );
        if !layout.panel.contains(self.mouse) {
            return None;
        }
        // 市场面板吃掉点击，顺便清掉从背包拖过来的幽灵（寄售物品靠「选中背包格」）。
        self.drag = None;
        if layout.close.contains(self.mouse) {
            self.close_market();
            return None;
        }
        if let Some(tab) = layout.tab_at(self.mouse) {
            if tab == self.market_tab {
                return None;
            }
            self.market_tab = tab;
            self.market_selected = None;
            self.market_page = 0;
            self.market_scroll = 0;
            // 切页签同时清掉上一页的失败/成功提示（新提示的版本号会比这里大）。
            self.market_notice_dismissed = snapshot
                .market
                .fail_version
                .max(snapshot.market.message_version);
            // Crystal 切页签会清空价格框（`PriceTextBox.Text = null`，
            // `TrustMerchantDialog.cs:1183`、`:1233`）；商城页签复用同一个输入框当
            // 「购买数量」，默认 1 份。
            self.market_price = if tab == MarketPanelType::GameShop {
                let mut field = input::TextField::numeric(2);
                field.set("1");
                field
            } else {
                input::TextField::numeric(9)
            };
            self.focus = None;
            // `S.NPCConsign` 的 NPC 页是 `[@CONSIGN]`：服务端只接受上架，搜索/翻页要先
            // 回到 `[@MARKET]` 页（`crates/mir-server/src/market.rs` 的 `check_market_npc`）。
            if self.market_consign_only && tab != MarketPanelType::Consign {
                self.market_consign_only = false;
                return Some(UiAction::CallNpc {
                    object_id: self.npc_object_id.unwrap_or(0),
                    key: market::MARKET_KEY.to_owned(),
                });
            }
            // 商城页签打开时发 `C.MarketSearch{MarketType=GameShop}`（Crystal
            // `TrustMerchantDialog.cs:1302-1309`），列表由服务端 `S.NPCMarket` 下发。
            return Some(self.market_search_action());
        }
        if market::has_sell_form(self.market_tab) {
            if layout.price_field.contains(self.mouse) {
                self.focus = Some(FieldId::MarketPrice);
                return None;
            }
            if layout.sell.contains(self.mouse) {
                let slot = self.selected_slot?;
                let item = snapshot.inventory_item(slot)?;
                let price = self.market_price.number();
                if price == 0 {
                    return None;
                }
                self.focus = None;
                return Some(UiAction::ConsignItem {
                    unique_id: item.unique_id,
                    price,
                    panel: self.market_tab,
                });
            }
        } else {
            if layout.search_field.contains(self.mouse) {
                self.focus = Some(FieldId::MarketSearch);
                return None;
            }
            if layout.find.contains(self.mouse) {
                self.focus = None;
                return Some(self.market_search_action());
            }
            if layout.refresh.contains(self.mouse) {
                self.focus = None;
                return Some(UiAction::MarketRefresh);
            }
            if layout.bid_field.contains(self.mouse) {
                self.focus = Some(FieldId::MarketPrice);
                return None;
            }
        }
        if let Some(index) = layout.row_at(self.mouse) {
            let listing = snapshot.market.listings.get(index)?;
            self.market_selected = Some(listing.auction_id);
            // 拍卖条目：按 Crystal 的 `MirAmountBox` 预填「当前价 + 1」
            // （`TrustMerchantDialog.cs:421`）；寄售是一口价，清掉残留的出价。
            // 商城页签的输入框是「数量」，选中商品时保持原值。
            if !market::is_user_mode(self.market_tab)
                && self.market_tab != MarketPanelType::GameShop
            {
                if listing.item_type == mir_core::enums::MarketItemType::Auction {
                    let minimum = listing.price.saturating_add(1);
                    if self.market_price.number() <= listing.price {
                        self.market_price.set(&minimum.to_string());
                    }
                    self.focus = Some(FieldId::MarketPrice);
                } else {
                    self.market_price = input::TextField::numeric(9);
                }
            }
            return None;
        }
        if layout.prev.contains(self.mouse) {
            self.market_page = self.market_page.saturating_sub(1);
            self.market_scroll = 0;
            self.market_selected = None;
            return None;
        }
        if layout.next.contains(self.mouse) {
            let next = self.market_page + 1;
            if next >= snapshot.market.pages {
                return None;
            }
            if next < snapshot.market.fetched_pages() {
                // 这一页已经在本地（Crystal 的列表也是累积的），直接翻。
                self.market_page = next;
                self.market_scroll = 0;
                self.market_selected = None;
                return None;
            }
            return Some(UiAction::MarketPage { page: next });
        }
        let selected = self
            .market_selected
            .and_then(|auction_id| snapshot.market.listing(auction_id));
        if layout.primary.contains(self.mouse) {
            let listing = selected?;
            if market::is_user_mode(self.market_tab) {
                return Some(UiAction::MarketGetBack {
                    mode: MarketCollectionMode::Any,
                    auction_id: listing.auction_id,
                });
            }
            // 商城页签：`C.GameshopBuy`（包 133），数量取输入框（1..=99），元宝结算。
            if self.market_tab == MarketPanelType::GameShop {
                let quantity = self.market_quantity(snapshot, listing);
                return Some(UiAction::GameShopBuy {
                    g_index: i32::try_from(listing.auction_id).unwrap_or(0),
                    quantity,
                    p_type: 0,
                });
            }
            // 拍卖出价：输入框空着就用当前价（服务端只拒绝 `current_bid > bid`）。
            let bid_price = self.market_price.number().max(listing.price);
            return Some(UiAction::MarketBuy {
                auction_id: listing.auction_id,
                bid_price,
            });
        }
        if layout.sell_now.contains(self.mouse) && self.market_tab == MarketPanelType::Auction {
            let listing = selected?;
            return Some(UiAction::MarketSellNow {
                auction_id: listing.auction_id,
            });
        }
        if layout.collect.contains(self.mouse) && market::is_user_mode(self.market_tab) {
            // Crystal「Collect Sold」：`C.MarketGetBack{Mode=Sold, AuctionID=0}`（`:474-480`）。
            return Some(UiAction::MarketGetBack {
                mode: MarketCollectionMode::Sold,
                auction_id: 0,
            });
        }
        None
    }

    /// 点击交易面板：金币输入 / 放入金币 / 准备-确认 / 取消 / 拖起自己的交易格。
    fn click_trade(&mut self, snapshot: &WorldSnapshot, frame: Rect) -> Option<UiAction> {
        let layout = trade::trade_layout(frame);
        if !layout.panel.contains(self.mouse) {
            return None;
        }
        if layout.gold_field.contains(self.mouse) {
            self.focus = Some(FieldId::TradeGold);
            return None;
        }
        if layout.gold_add.contains(self.mouse) {
            let amount = self.trade_gold.number();
            if amount > 0 {
                // 只有本地先加到快照里并不够（快照由网络线程持有），所以这里把输入框清空，
                // 累计值由 `NetCommand::TradeGold` 在发包时加进快照（见 `net::handle_command`）。
                self.trade_gold = input::TextField::numeric(9);
                self.focus = None;
                return Some(UiAction::TradeGold { amount });
            }
            return None;
        }
        if layout.ready.contains(self.mouse) {
            // 两段式：未准备 → 准备（锁住己方报价）；已准备 → 确认（再发一次，服务端幂等）。
            self.trade_locked = true;
            self.focus = None;
            return Some(UiAction::TradeConfirm { locked: true });
        }
        if layout.cancel.contains(self.mouse) {
            self.close_trade();
            return Some(UiAction::TradeCancel);
        }
        if let Some(slot) = layout.own_slot_at(self.mouse) {
            self.start_drag(snapshot, DragSource::Trade(slot));
        }
        None
    }

    /// 点击组队面板：允许组队开关 / 成员行 / 邀请 / 离队 / 踢人。
    fn click_group(&mut self, snapshot: &WorldSnapshot, frame: Rect) -> Option<UiAction> {
        let layout = group::group_layout(frame, snapshot.group.members.len(), self.group_scroll);
        if !layout.panel.contains(self.mouse) {
            return None;
        }
        if layout.switch.contains(self.mouse) {
            return Some(UiAction::SwitchGroup {
                allow_group: !snapshot.group.allow_group,
            });
        }
        if let Some(index) = layout.row_at(self.mouse) {
            self.group_selected = Some(index);
            return None;
        }
        if layout.invite_field.contains(self.mouse) {
            self.focus = Some(FieldId::GroupName);
            return None;
        }
        if layout.invite.contains(self.mouse) {
            let name = self.group_name.value.trim().to_owned();
            if name.is_empty() {
                return None;
            }
            // 只有队长（或还没组队）能邀请（Crystal `GroupDialog.cs:184-206`）。
            if !snapshot.is_group_leader() && snapshot.has_group() {
                return None;
            }
            self.group_name = input::TextField::new(12);
            self.focus = None;
            return Some(UiAction::AddMember { name });
        }
        if layout.leave.contains(self.mouse) {
            let Some(me) = &snapshot.self_player else {
                return None;
            };
            return Some(UiAction::DelMember {
                name: me.name.clone(),
            });
        }
        if layout.kick.contains(self.mouse) {
            if !snapshot.is_group_leader() {
                return None;
            }
            let index = self.group_selected?;
            let member = snapshot.group.members.get(index)?;
            return Some(UiAction::DelMember {
                name: member.name.clone(),
            });
        }
        None
    }

    /// 点击行会面板：页签 / 成员行 / 职位管理 / 仓库拖拽 / 资金 / 增益 / 公告。
    fn click_guild(&mut self, snapshot: &WorldSnapshot, frame: Rect) -> Option<UiAction> {
        let layout = guild::guild_layout(
            frame,
            snapshot.guild_members().len(),
            snapshot.guild_buff_infos.len(),
            self.guild_member_scroll,
            self.guild_buff_scroll,
        );
        if !layout.panel.contains(self.mouse) {
            return None;
        }
        if let Some(tab) = layout.tab_at(self.mouse) {
            self.guild_tab = guild::GuildTab::ALL[tab];
            self.focus = None;
            return None;
        }
        match self.guild_tab {
            guild::GuildTab::Members => {
                if let Some(index) = layout.member_at(self.mouse) {
                    self.guild_selected_member = Some(index);
                    return None;
                }
                let members = snapshot.guild_members();
                let selected = self
                    .guild_selected_member
                    .and_then(|index| members.get(index).copied());
                let max_rank = snapshot.guild_ranks.len().saturating_sub(1);
                if layout.promote.contains(self.mouse) {
                    if let Some((rank, member)) = selected {
                        if rank.index > 0 {
                            return Some(UiAction::EditGuildMember {
                                change_type: 2,
                                rank_index: u8::try_from(rank.index - 1).unwrap_or(0),
                                name: member.name.clone(),
                                rank_name: String::new(),
                            });
                        }
                    }
                }
                if layout.demote.contains(self.mouse) {
                    if let Some((rank, member)) = selected {
                        let index = usize::try_from(rank.index).unwrap_or(0);
                        if index < max_rank {
                            return Some(UiAction::EditGuildMember {
                                change_type: 2,
                                rank_index: u8::try_from(index + 1).unwrap_or(u8::MAX),
                                name: member.name.clone(),
                                rank_name: String::new(),
                            });
                        }
                    }
                }
                if layout.kick.contains(self.mouse) {
                    if let Some((_, member)) = selected {
                        return Some(UiAction::EditGuildMember {
                            change_type: 1,
                            rank_index: 0,
                            name: member.name.clone(),
                            rank_name: String::new(),
                        });
                    }
                }
            }
            guild::GuildTab::Storage => {
                if layout.gold_field.contains(self.mouse) {
                    self.focus = Some(FieldId::GuildGold);
                    return None;
                }
                let amount = self.guild_gold.number();
                if layout.deposit.contains(self.mouse) && amount > 0 {
                    return Some(UiAction::GuildStorageGoldChange { type_: 0, amount });
                }
                if layout.withdraw.contains(self.mouse) && amount > 0 {
                    return Some(UiAction::GuildStorageGoldChange { type_: 1, amount });
                }
                if let Some(slot) = layout.storage_grid.slot_at(self.mouse) {
                    self.start_drag(snapshot, DragSource::GuildStorage(slot));
                }
            }
            guild::GuildTab::Buffs => {
                if let Some(index) = layout.buff_button_at(self.mouse) {
                    if let Some(info) = snapshot.guild_buff_infos.get(index) {
                        // Crystal：未获得的走「购买」（Action=1），已获得但过期的走
                        // 「激活」（Action=2）（`crates/mir-server/src/guild_buffs.rs` 包序表）。
                        let action = if snapshot.guild_active_buff(info.id).is_some() {
                            2
                        } else {
                            1
                        };
                        return Some(UiAction::GuildBuffUpdate {
                            action,
                            id: info.id,
                        });
                    }
                }
            }
            guild::GuildTab::Notice => {
                if layout.notice_field.contains(self.mouse) {
                    self.focus = Some(FieldId::GuildNotice);
                    return None;
                }
                if layout.save_notice.contains(self.mouse) {
                    let notice = self.guild_notice.value.trim().to_owned();
                    if !notice.is_empty() {
                        return Some(UiAction::EditGuildNotice {
                            notice: vec![notice],
                        });
                    }
                }
            }
        }
        None
    }

    /// 鼠标左键按下（无地图可走性时的兼容入口）。
    ///
    /// 与 [`Self::on_click_with_path`] 完全相同，只是点击空地仍退回
    /// [`walk_intent`] 的「走一步/转身」；无头测试与旧调用方用这个入口。
    pub fn on_click(
        &mut self,
        snapshot: &WorldSnapshot,
        text: &UiText,
        frame: Rect,
        world_cell: Option<Point>,
    ) -> Option<UiAction> {
        self.on_click_with_path(snapshot, text, frame, world_cell, None)
    }

    /// 鼠标左键按下：按「右键菜单 > 对话 > 商店 > 装备 > 背包 > 技能 > 世界」的顺序派发。
    ///
    /// `world_cell` 是鼠标对应的地图格（由 `MapScene::screen_to_cell` 算好）。
    /// `walkable` 是地图可走性闭包：给出时点到空地会跑 A*（[`crate::ui::PATH_STEP_MS`]
    /// 节拍逐格发 [`UiAction::WalkStep`]）；`None` 或寻路失败时退回一步/转身。
    /// 在背包格按下会同时开始拖拽；同一格 `inventory::DOUBLE_CLICK_MS` 内再次按下
    /// 视为双击 → [`UiAction::UseItem`]。
    pub fn on_click_with_path(
        &mut self,
        snapshot: &WorldSnapshot,
        text: &UiText,
        frame: Rect,
        world_cell: Option<Point>,
        walkable: Option<&dyn Fn(i32, i32) -> bool>,
    ) -> Option<UiAction> {
        self.sync(snapshot);
        // 新点击一律取消旧路径/靠近；下面点到空地时会重新设置。
        self.walk_path = None;
        self.approach = None;
        if let Some(menu) = self.context_menu.clone() {
            let entries = menu.entries(snapshot, self.language);
            let panel = inventory::context_menu_rect(menu.anchor, entries.len(), frame);
            if let Some(index) = inventory::context_menu_entry_at(panel, entries.len(), self.mouse)
            {
                self.context_menu = None;
                let entry = entries.get(index)?;
                return Self::context_action(&menu, entry.action);
            }
            // 点在菜单外：关掉菜单并吃掉这次点击（与 Crystal 的右键菜单一致）。
            self.context_menu = None;
            return None;
        }
        // 邀请弹窗是模态的（交易/组队），最优先处理。
        if let Some(action) = self.click_invite(frame) {
            return Some(action);
        }
        if self.invite_open() {
            return None;
        }
        // 设置面板是模态的：打开时吃掉所有点击（面板外的点击也吞掉，`Esc` 或「关闭」退出），
        // 与 Crystal `OptionDialog`（`MainDialogs.cs` 的对话框基类）一致。
        if self.show_settings {
            let layout = settings::settings_layout(frame);
            if let Some(action) = settings::click(&layout, &mut self.settings, self.mouse) {
                if action == settings::SettingsAction::Close {
                    self.show_settings = false;
                    self.focus = None;
                    return None;
                }
                return Some(UiAction::Settings(action));
            }
            return None;
        }
        if let Some(page) = &self.dialog {
            let layout =
                dialog::dialog_layout(frame, page, |label| text.text_width(label, hud::SIZE_BODY));
            if layout.panel.contains(self.mouse) {
                if let Some(index) = layout.button_index_at(self.mouse) {
                    let button = page.buttons.get(index)?;
                    return Some(UiAction::CallNpc {
                        object_id: self.npc_object_id.unwrap_or(0),
                        key: button.key.clone(),
                    });
                }
                // 点在对话面板空白处：吃掉这次点击，避免误点到底下的面板。
                return None;
            }
        }
        // 小地图/大地图是覆盖层，画在其它面板之后，所以先吃掉点击。
        if self.minimap_clicked(frame) {
            return None;
        }
        // 大地图同理；Crystal 点大地图会寻路（`BigMapDialog.cs:612-628`），
        // 本作还没把大地图坐标映射到世界格，只吃掉点击（简化）。
        if self.bigmap_clicked(frame) {
            return None;
        }
        // 任务 / 邮件 / 行会 / 仓库（按「后画的先响应」顺序）：
        // 先问各面板要动作，再统一吞掉落在任一新面板上的点击。
        if self.show_trade {
            if let Some(action) = self.click_trade(snapshot, frame) {
                return Some(action);
            }
        }
        if self.show_market {
            if let Some(action) = self.click_market(snapshot, frame) {
                return Some(action);
            }
        }
        if self.show_group {
            if let Some(action) = self.click_group(snapshot, frame) {
                return Some(action);
            }
        }
        if self.show_guild {
            if let Some(action) = self.click_guild(snapshot, frame) {
                return Some(action);
            }
        }
        if self.show_mail {
            if let Some(action) = self.click_mail(snapshot, frame) {
                return Some(action);
            }
        }
        if self.show_quests {
            if let Some(action) = self.click_quests(snapshot, frame) {
                return Some(action);
            }
        }
        if self.show_storage {
            if let Some(action) = self.click_storage(snapshot, frame) {
                return Some(action);
            }
        }
        if self.panel_contains_mouse(snapshot, frame) {
            return None;
        }
        if self.shop_open {
            if let Some(goods) = &snapshot.npc_goods {
                let layout = shop::shop_layout(
                    frame,
                    goods.list.len(),
                    self.shop_scroll,
                    shop::shop_left(self.show_equipment),
                );
                if layout.panel.contains(self.mouse) {
                    if let Some(index) = layout.row_at(self.mouse) {
                        self.selected_goods = Some(index);
                        if let Some(entry) = goods.list.get(index) {
                            return Some(UiAction::BuyItem {
                                unique_id: entry.unique_id,
                            });
                        }
                    }
                    return None;
                }
            }
        }
        if self.show_equipment {
            let layout = equipment::equipment_layout(frame);
            if layout.panel.contains(self.mouse) {
                if let Some(slot) = layout.slot_at(self.mouse) {
                    self.start_drag(snapshot, DragSource::Equipment(slot));
                }
                return None;
            }
        }
        if self.show_inventory {
            let panel = inventory::inventory_panel_rect(frame);
            if panel.contains(self.mouse) {
                if let Some(slot) = inventory::inventory_slot_at(panel, self.mouse) {
                    self.selected_slot = Some(slot);
                    if let Some(item) = snapshot.inventory_item(slot) {
                        let source = DragSource::Inventory(slot);
                        let now = self.time_millis;
                        if self.last_click.is_some_and(|(time, last)| {
                            last == source && now.saturating_sub(time) <= inventory::DOUBLE_CLICK_MS
                        }) {
                            self.last_click = None;
                            self.drag = None;
                            return Some(UiAction::UseItem {
                                unique_id: item.unique_id,
                            });
                        }
                        self.last_click = Some((now, source));
                        self.drag = Some(DragState {
                            source,
                            item_index: item.item_index,
                            unique_id: item.unique_id,
                            count: item.count,
                        });
                    }
                }
                return None;
            }
        }
        if self.show_skills {
            let layout = skills::skill_layout(frame, snapshot.magics.len());
            if layout.panel.contains(self.mouse) {
                if let Some(index) = layout.row_at(self.mouse) {
                    self.selected_magic = Some(index);
                    let (spell, bound_key) = snapshot
                        .magics
                        .get(index)
                        .map(|magic| (magic.spell, magic.key))?;
                    if bound_key == 0 {
                        // 未绑定：按第 N 个技能临时绑到 N 号键再施放。
                        let key = u8::try_from(index + 1).unwrap_or(skills::SKILL_KEYS);
                        let actions = self.press_magic_key(snapshot, key.min(skills::SKILL_KEYS));
                        return actions.into_iter().next();
                    }
                    return skills::cast_request(snapshot, spell).map(|cast| UiAction::CastMagic {
                        spell: cast.spell,
                        direction: cast.direction,
                        target_id: cast.target_id,
                        location: cast.location,
                    });
                }
                return None;
            }
        }
        if let Some(cell) = world_cell {
            if let Some(npc) = snapshot.npc_at(cell) {
                let in_range = snapshot
                    .self_player
                    .as_ref()
                    .is_some_and(|me| me.position.in_range(npc.position, 8));
                if in_range {
                    self.npc_object_id = Some(npc.object_id);
                    return Some(UiAction::CallNpc {
                        object_id: npc.object_id,
                        key: "@main".to_owned(),
                    });
                }
            }
            // 点地面物品：相邻直接 `C.PickUp`；不相邻先靠近（复用 [`Approach`]，
            // 最多 [`APPROACH_STEPS`] 步，到了相邻再捡）。Crystal 的拾取在
            // `GameScene.cs:11627-11634`：鼠标格 == 自己所在格时发 `C.PickUp`。
            if let Some(item) = target::item_at(snapshot, cell) {
                let me = snapshot.self_player.as_ref()?;
                // 简化：Crystal 只在鼠标格 == 自己所在格时发 C.PickUp
                // （GameScene.cs:11627-11634）；需求要求相邻格即可拾取。
                if me.position.in_range(item.position, 1) {
                    self.target_object_id = None;
                    return Some(UiAction::PickUp);
                }
                let direction = me.position.direction_to(item.position);
                self.target_object_id = None;
                self.approach = Some(Approach {
                    target_id: item.object_id,
                    steps_left: APPROACH_STEPS.saturating_sub(1),
                    next_step_at: self.time_millis.saturating_add(APPROACH_STEP_MS),
                    pickup: true,
                });
                return Some(UiAction::WorldMove {
                    direction,
                    step: true,
                });
            }
            // 点在怪物/其他玩家上：锁定目标（`GameScene.cs:11442-11451`），
            // 同一目标 `TARGET_DOUBLE_CLICK_MS` 内再点一次 → 攻击/靠近。
            if let Some(object_id) = target::target_at(snapshot, cell) {
                return self.lock_target(snapshot, object_id);
            }
            // 点空地：清掉锁定目标（Crystal 的 `else TargetObjectID = 0`，
            // `GameScene.cs:11450-11451`），有可走性闭包时走 A*，否则走一步/转身。
            self.target_object_id = None;
            self.approach = None;
            if let Some(walkable) = walkable {
                if let Some(me) = snapshot.self_player.as_ref() {
                    if let Some(path) = mir_render::pathfind::find_path_with_steps(
                        me.position,
                        cell,
                        walkable,
                        mir_render::pathfind::DEFAULT_MAX_NODES,
                        mir_render::pathfind::MAX_PATH_STEPS,
                    ) {
                        if let Some((&first, rest)) = path.split_first() {
                            // 只有一步时直接发掉，不留下空路径。
                            self.walk_path = if rest.is_empty() {
                                None
                            } else {
                                Some(WalkPath {
                                    goal: cell,
                                    steps: rest.to_vec(),
                                    next_step_at: self.time_millis.saturating_add(PATH_STEP_MS),
                                    from: me.position,
                                    expected: me.position.moved(first, 1),
                                    running: false,
                                })
                            };
                            return Some(UiAction::WalkStep {
                                direction: first,
                                run: false,
                            });
                        }
                    }
                    // 寻路失败：退化为「朝目标方向走一步」（需求）。
                    if me.position != cell {
                        return Some(UiAction::WorldMove {
                            direction: me.position.direction_to(cell),
                            step: true,
                        });
                    }
                }
            }
            return walk_intent(snapshot, cell).map(|intent| UiAction::WorldMove {
                direction: intent.direction,
                step: intent.step,
            });
        }
        None
    }

    /// 设置当前路径是否用跑步（`Shift` 按住时由 `app.rs` 调用）。
    pub fn set_path_running(&mut self, running: bool) {
        if let Some(path) = &mut self.walk_path {
            path.running = running;
        }
    }

    /// 取消点地走路（键盘移动/攻击/点面板时调用）。
    pub fn cancel_path(&mut self) {
        self.walk_path = None;
    }

    /// 每帧推进 A* 路径：到时间就取下一格，返回要发的 `C.Walk`/`C.Run`。
    ///
    /// 按 [`PATH_STEP_MS`] 节拍逐格取，路径耗尽就停止（`walk_path` 清空）。
    /// 不依赖服务器回包：服务器还没确认上一步时继续按节拍发下一步，相当于输入缓冲；
    /// `from`/`expected` 只用于诊断（记录路径认为角色应该在的位置）。
    /// `snapshot` 保留在签名里，方便以后加入「被传送/攻击打断」的校验。
    pub fn tick_path(&mut self, _snapshot: &WorldSnapshot) -> Option<UiAction> {
        let next_at = self.walk_path.as_ref()?.next_step_at;
        if self.time_millis < next_at {
            return None;
        }
        let mut path = self.walk_path.take()?;
        if path.steps.is_empty() {
            return None;
        }
        let first = path.steps.remove(0);
        let run = path.running && path.steps.first().is_some_and(|second| *second == first);
        let steps = if run {
            path.steps.remove(0);
            2
        } else {
            1
        };
        path.from = path.expected;
        path.expected = path.from.moved(first, steps);
        path.next_step_at = self.time_millis.saturating_add(PATH_STEP_MS);
        // 最后一步已经发出：直接丢路径（到达后停止）。
        self.walk_path = if path.steps.is_empty() {
            None
        } else {
            Some(path)
        };
        Some(UiAction::WalkStep {
            direction: first,
            run,
        })
    }

    /// 左键点到可攻击目标：锁定它；同一目标短时间内第二次点击视为双击 → 攻击/靠近。
    ///
    /// Crystal 没有「双击攻击」：它按住左键，`CheckInput` 在目标相邻时持续挥刀
    /// （`GameScene.cs:11558-11625`）。本作没有输入缓冲，用双击表达「打这个」
    /// （简化），单击只锁定目标。
    fn lock_target(&mut self, snapshot: &WorldSnapshot, object_id: u32) -> Option<UiAction> {
        let now = self.time_millis;
        let double = self.last_target_click.is_some_and(|(time, last)| {
            last == object_id && now.saturating_sub(time) <= TARGET_DOUBLE_CLICK_MS
        });
        self.target_object_id = Some(object_id);
        self.last_target_click = Some((now, object_id));
        if !double {
            return None;
        }
        self.last_target_click = None;
        self.attack_target(snapshot)
    }

    /// 开始拖拽某个格子里的物品（空格不产生拖拽）。
    fn start_drag(&mut self, snapshot: &WorldSnapshot, source: DragSource) {
        let item = match source {
            DragSource::Inventory(slot) => snapshot.inventory_item(slot),
            DragSource::Equipment(slot) => snapshot.equipment_item(slot),
            DragSource::Storage(slot) => snapshot.storage_item(slot),
            DragSource::GuildStorage(slot) => snapshot.guild_storage_item(slot),
            DragSource::Trade(slot) => snapshot.trade.items.get(slot).and_then(Option::as_ref),
        };
        self.drag = item.map(|item| DragState {
            source,
            item_index: item.item_index,
            unique_id: item.unique_id,
            count: item.count,
        });
    }

    /// 鼠标左键释放：把拖拽落点翻译成 [`UiAction`]。
    pub fn on_mouse_release(&mut self, snapshot: &WorldSnapshot, frame: Rect) -> Option<UiAction> {
        self.sync(snapshot);
        let drag = self.drag.take()?;
        let equipment_slot = self
            .show_equipment
            .then(|| equipment::equipment_layout(frame).slot_at(self.mouse))
            .flatten();
        if let Some(to) = equipment_slot {
            return match drag.source {
                DragSource::Inventory(from) => Some(UiAction::MoveItem {
                    grid: MirGridType::Inventory,
                    from: i32::try_from(from).unwrap_or(0),
                    to: i32::try_from(to).unwrap_or(0),
                    to_grid: MirGridType::Equipment,
                }),
                DragSource::Equipment(slot) if slot == to => None,
                // 仓库/交易格 → 装备槽没有对应包（Crystal 也要先取回背包）。
                DragSource::Equipment(_)
                | DragSource::Storage(_)
                | DragSource::GuildStorage(_)
                | DragSource::Trade(_) => None,
            };
        }
        // 落在个人仓库格上：背包 → 仓库是 `C.StoreItem`；仓库内部没有移动包。
        if self.show_storage {
            let layout = storage::storage_layout(frame);
            if let Some(to) = layout.grid.slot_at(self.mouse) {
                return match drag.source {
                    DragSource::Inventory(from) => Some(UiAction::StoreItem {
                        from: i32::try_from(from).unwrap_or(0),
                        to: i32::try_from(to).unwrap_or(0),
                    }),
                    _ => None,
                };
            }
        }
        // 落在行会仓库格上：`C.GuildStorageItemChange` 的 0/2 两种类型。
        if self.show_guild && self.guild_tab == guild::GuildTab::Storage {
            let layout = guild::guild_layout(
                frame,
                snapshot.guild_members().len(),
                snapshot.guild_buff_infos.len(),
                self.guild_member_scroll,
                self.guild_buff_scroll,
            );
            if let Some(to) = layout.storage_grid.slot_at(self.mouse) {
                let to = i32::try_from(to).unwrap_or(0);
                return match drag.source {
                    DragSource::Inventory(from) => Some(UiAction::GuildStorageItemChange {
                        type_: 0,
                        from: i32::try_from(from).unwrap_or(0),
                        to,
                    }),
                    DragSource::GuildStorage(from) => {
                        let from = i32::try_from(from).unwrap_or(0);
                        (from != to).then_some(UiAction::GuildStorageItemChange {
                            type_: 2,
                            from,
                            to,
                        })
                    }
                    _ => None,
                };
            }
        }
        // 落在自己的交易格上：背包 → 交易格是 `C.DepositTradeItem`；交易格之间是
        // `C.MoveItem{Grid = Trade}`（Crystal `MirItemCell.cs:1507-1575`）。
        if self.show_trade {
            let layout = trade::trade_layout(frame);
            if let Some(to) = layout.own_slot_at(self.mouse) {
                let to_i32 = i32::try_from(to).unwrap_or(0);
                return match drag.source {
                    DragSource::Inventory(from) => Some(UiAction::TradeItem {
                        from: i32::try_from(from).unwrap_or(0),
                        to: to_i32,
                    }),
                    DragSource::Trade(from) if from != to => Some(UiAction::MoveItem {
                        grid: MirGridType::Trade,
                        from: i32::try_from(from).unwrap_or(0),
                        to: to_i32,
                        to_grid: MirGridType::Trade,
                    }),
                    _ => None,
                };
            }
            // 对方那一栏只读（Crystal 的 `GuestTradeDialog` 没有拖拽入口）。
            if layout.partner_slot_at(self.mouse).is_some() {
                return None;
            }
        }
        let inventory_slot = self
            .show_inventory
            .then(|| {
                inventory::inventory_slot_at(inventory::inventory_panel_rect(frame), self.mouse)
            })
            .flatten();
        let to = inventory_slot?;
        match drag.source {
            DragSource::Inventory(from) if from == to => None,
            DragSource::Inventory(from) => Some(UiAction::MoveItem {
                grid: MirGridType::Inventory,
                from: i32::try_from(from).unwrap_or(0),
                to: i32::try_from(to).unwrap_or(0),
                to_grid: MirGridType::Inventory,
            }),
            DragSource::Equipment(from) => Some(UiAction::MoveItem {
                grid: MirGridType::Equipment,
                from: i32::try_from(from).unwrap_or(0),
                to: i32::try_from(to).unwrap_or(0),
                to_grid: MirGridType::Inventory,
            }),
            DragSource::Storage(from) => Some(UiAction::TakeBackItem {
                from: i32::try_from(from).unwrap_or(0),
                to: i32::try_from(to).unwrap_or(0),
            }),
            DragSource::GuildStorage(from) => Some(UiAction::GuildStorageItemChange {
                type_: 1,
                from: i32::try_from(from).unwrap_or(0),
                to: i32::try_from(to).unwrap_or(0),
            }),
            DragSource::Trade(from) => Some(UiAction::TradeTakeBack {
                from: i32::try_from(from).unwrap_or(0),
                to: i32::try_from(to).unwrap_or(0),
            }),
        }
    }

    /// 鼠标右键：背包物品弹出「使用/装备/丢弃」菜单。
    pub fn on_right_click(&mut self, snapshot: &WorldSnapshot, frame: Rect) -> Option<UiAction> {
        self.sync(snapshot);
        self.context_menu = None;
        if !self.show_inventory {
            return None;
        }
        let panel = inventory::inventory_panel_rect(frame);
        let slot = inventory::inventory_slot_at(panel, self.mouse)?;
        let item = snapshot.inventory_item(slot)?;
        self.selected_slot = Some(slot);
        self.context_menu = Some(ContextMenu {
            source: DragSource::Inventory(slot),
            unique_id: item.unique_id,
            item_index: item.item_index,
            count: item.count,
            anchor: self.mouse,
        });
        None
    }

    /// 把右键菜单项变成动作。
    fn context_action(menu: &ContextMenu, action: ContextAction) -> Option<UiAction> {
        match action {
            ContextAction::Use => Some(UiAction::UseItem {
                unique_id: menu.unique_id,
            }),
            ContextAction::Equip { to } => {
                let from = menu.source.slot();
                // 只有背包里的物品可以「穿」；装备槽上右键没有装备项，防御性返回。
                if menu.source.grid() != MirGridType::Inventory {
                    return None;
                }
                Some(UiAction::MoveItem {
                    grid: MirGridType::Inventory,
                    from: i32::try_from(from).unwrap_or(0),
                    to: i32::try_from(to).unwrap_or(0),
                    to_grid: MirGridType::Equipment,
                })
            }
            ContextAction::Drop => Some(UiAction::DropItem {
                unique_id: menu.unique_id,
                count: 1,
            }),
        }
    }

    /// 鼠标悬停的背包格（tooltip 用）。
    #[must_use]
    pub fn hovered_inventory_slot(&self, frame: Rect) -> Option<usize> {
        if !self.show_inventory {
            return None;
        }
        inventory::inventory_slot_at(inventory::inventory_panel_rect(frame), self.mouse)
    }

    /// 鼠标悬停的装备槽（tooltip 用）。
    #[must_use]
    pub fn hovered_equipment_slot(&self, frame: Rect) -> Option<usize> {
        if !self.show_equipment {
            return None;
        }
        equipment::equipment_layout(frame).slot_at(self.mouse)
    }

    /// 鼠标悬停的个人仓库格（tooltip 用）。
    #[must_use]
    pub fn hovered_storage_slot(&self, frame: Rect) -> Option<usize> {
        if !self.show_storage {
            return None;
        }
        storage::slot_at(&storage::storage_layout(frame), self.mouse)
    }

    /// 鼠标悬停的行会仓库格（tooltip 用；只在仓库页签下有效）。
    #[must_use]
    pub fn hovered_guild_storage_slot(
        &self,
        snapshot: &WorldSnapshot,
        frame: Rect,
    ) -> Option<usize> {
        if !self.show_guild || self.guild_tab != guild::GuildTab::Storage {
            return None;
        }
        let layout = guild::guild_layout(
            frame,
            snapshot.guild_members().len(),
            snapshot.guild_buff_infos.len(),
            self.guild_member_scroll,
            self.guild_buff_scroll,
        );
        layout.storage_grid.slot_at(self.mouse)
    }

    /// 鼠标悬停的交易格（tooltip 用；自己/对方两栏都算）。
    #[must_use]
    pub fn hovered_trade_slot(&self, frame: Rect) -> Option<(bool, usize)> {
        if !self.show_trade {
            return None;
        }
        let layout = trade::trade_layout(frame);
        if let Some(slot) = layout.own_slot_at(self.mouse) {
            return Some((true, slot));
        }
        layout.partner_slot_at(self.mouse).map(|slot| (false, slot))
    }

    /// 鼠标是否落在新增的面板之一上（任务/仓库/邮件/行会/组队/交易）。
    ///
    /// `on_click_with_path` 用它吞掉「面板内但没命中按钮」的点击，避免穿透成走路。
    #[must_use]
    pub fn panel_contains_mouse(&self, snapshot: &WorldSnapshot, frame: Rect) -> bool {
        if self.show_quests
            && quests::quest_layout(frame, snapshot.quests.len(), self.quest_scroll)
                .panel
                .contains(self.mouse)
        {
            return true;
        }
        if self.show_storage && storage::storage_layout(frame).panel.contains(self.mouse) {
            return true;
        }
        if self.show_mail
            && mail::mail_layout(
                frame,
                self.mail_tab,
                snapshot.mails.len(),
                self.mail_page,
                snapshot.inventory.len(),
            )
            .panel
            .contains(self.mouse)
        {
            return true;
        }
        if self.show_guild
            && guild::guild_layout(
                frame,
                snapshot.guild_members().len(),
                snapshot.guild_buff_infos.len(),
                self.guild_member_scroll,
                self.guild_buff_scroll,
            )
            .panel
            .contains(self.mouse)
        {
            return true;
        }
        if self.show_group
            && group::group_layout(frame, snapshot.group.members.len(), self.group_scroll)
                .panel
                .contains(self.mouse)
        {
            return true;
        }
        if self.show_market
            && market::market_layout(
                frame,
                self.market_tab,
                snapshot.market.listings.len(),
                self.market_page,
                self.market_scroll,
            )
            .panel
            .contains(self.mouse)
        {
            return true;
        }
        if self.show_trade && trade::trade_layout(frame).panel.contains(self.mouse) {
            return true;
        }
        false
    }

    /// 是否有待处理的邀请弹窗（交易/组队；弹窗是模态的，吃掉所有点击）。
    #[must_use]
    pub fn invite_open(&self) -> bool {
        self.trade_invite.is_some() || self.group_invite.is_some()
    }

    /// 当前聚焦的输入框。
    #[must_use]
    pub const fn focused_field(&self) -> Option<FieldId> {
        self.focus
    }

    /// 向当前聚焦的输入框追加一个字符；返回是否消费了这次按键。
    pub fn type_char(&mut self, character: char) -> bool {
        let Some(field) = self.focus else {
            return false;
        };
        match field {
            FieldId::MailRecipient => self.mail_compose.recipient.insert(character),
            FieldId::MailSubject => self.mail_compose.subject.insert(character),
            FieldId::MailGold => self.mail_compose.gold.insert(character),
            FieldId::MailMessage => self.mail_compose.message.insert(character),
            FieldId::GuildGold => self.guild_gold.insert(character),
            FieldId::GuildNotice => self.guild_notice.insert(character),
            FieldId::TradeGold => self.trade_gold.insert(character),
            FieldId::GroupName => self.group_name.insert(character),
            FieldId::MarketSearch => self.market_search.insert(character),
            FieldId::MarketPrice => self.market_price.insert(character),
        }
        true
    }

    /// 当前聚焦的输入框退格；返回是否消费了这次按键。
    pub fn backspace(&mut self) -> bool {
        let Some(field) = self.focus else {
            return false;
        };
        match field {
            FieldId::MailRecipient => self.mail_compose.recipient.backspace(),
            FieldId::MailSubject => self.mail_compose.subject.backspace(),
            FieldId::MailGold => self.mail_compose.gold.backspace(),
            FieldId::MailMessage => self.mail_compose.message.backspace(),
            FieldId::GuildGold => self.guild_gold.backspace(),
            FieldId::GuildNotice => self.guild_notice.backspace(),
            FieldId::TradeGold => self.trade_gold.backspace(),
            FieldId::GroupName => self.group_name.backspace(),
            FieldId::MarketSearch => self.market_search.backspace(),
            FieldId::MarketPrice => self.market_price.backspace(),
        }
        true
    }

    /// 取消输入焦点（`Esc`/点面板空白处）。
    pub fn clear_focus(&mut self) {
        self.focus = None;
    }
}

/// 画所有面板与世界叠层。
///
/// 绘制顺序：世界文字（名字/伤害/光效）→ 光标目标高亮 → 状态栏 → 增益栏 → 聊天 →
/// 装备 → 背包 → 仓库 → 商店 → 任务 → 邮件 → 行会 → 技能 → 小地图 → 大地图 → 对话 →
/// tooltip → 右键菜单 → 拖拽幽灵。点击派发按相反顺序（后画的面板先响应）。
pub fn draw(
    state: &mut UiState,
    frame: &mut Frame,
    scene: &mut MapScene,
    snapshot: &WorldSnapshot,
    text: &UiText,
    chat_input: Option<&str>,
    center: Point,
) {
    state.sync(snapshot);
    let screen = Rect::screen(frame.width, frame.height);
    worldtext::draw_names(frame, snapshot, scene, center, text);
    // 鼠标下的地面物品名字（只画命中的那一件，Crystal `GameScene.cs:10619-10626`）。
    if let Some(item) = state.hover_item(snapshot, scene, text, center, screen) {
        worldtext::draw_item_name(
            frame,
            snapshot,
            scene,
            center,
            text,
            item.position,
            state.language,
        );
    }
    // 光标目标：悬停实体画 1px 黄框、锁定目标画 1px 红框（Crystal 用半透明重绘，
    // `GameScene.cs:10973-10980`；这里按需求简化成方框，见 [`target`]）。
    let hover_cell = state
        .hover_object(snapshot, scene, text, center, screen)
        .map(|entity| entity.position);
    target::draw_highlights(
        frame,
        scene,
        center,
        snapshot,
        hover_cell,
        state.target_object_id,
    );
    worldtext::draw_damage_numbers(
        frame,
        &state.damage_numbers,
        scene,
        center,
        text,
        state.time_millis,
    );
    buffs::draw_effects(
        frame,
        &state.effects,
        snapshot,
        scene,
        center,
        state.time_millis,
    );
    // 技能表现（`S.Magic` / `S.ObjectMagic` / `S.ObjectSpell`）：火墙的区域、
    // 雷电的路径、瞬息移动的落点、隐身/召唤的光环。
    spellfx::draw(
        frame,
        &snapshot.spell_events,
        snapshot,
        scene,
        center,
        text,
        state.language,
        state.time_millis,
    );
    let context = PanelContext {
        snapshot,
        text,
        language: state.language,
    };
    hud::draw_status(frame, snapshot, text, state.language);
    buffs::draw(
        frame,
        snapshot,
        text,
        state.language,
        state.time_millis,
        buffs::bar_left(state.show_equipment, state.shop_open),
    );
    hud::draw_chat(frame, snapshot, text, chat_input, state.language);
    // 队友血条（HUD）：队伍存在时贴在聊天面板上方（需求：HUD 上显示队友血条）。
    hud::draw_party(frame, snapshot, text, state.language);

    if state.show_equipment {
        let layout = equipment::equipment_layout(screen);
        let hover = layout.slot_at(state.mouse);
        let UiState { icons, .. } = state;
        let mut icon_fn = |image: u16| -> Option<Image> { load_icon(scene, icons, image) };
        equipment::draw(frame, &layout, &context, hover, &mut icon_fn);
    }

    if state.show_inventory {
        let panel = inventory::inventory_panel_rect(screen);
        let hover = inventory::inventory_slot_at(panel, state.mouse);
        let selected = state.selected_slot;
        let UiState { icons, .. } = state;
        let mut icon_fn = |image: u16| -> Option<Image> { load_icon(scene, icons, image) };
        inventory::draw(frame, panel, &context, selected, hover, &mut icon_fn);
    }

    if state.shop_open {
        if let Some(goods) = &snapshot.npc_goods {
            let layout = shop::shop_layout(
                screen,
                goods.list.len(),
                state.shop_scroll,
                shop::shop_left(state.show_equipment),
            );
            let hover = layout.row_at(state.mouse);
            let selected = state.selected_goods;
            let UiState { icons, .. } = state;
            let mut icon_fn = |image: u16| -> Option<Image> { load_icon(scene, icons, image) };
            shop::draw(
                frame,
                &layout,
                &context,
                goods,
                selected,
                hover,
                &mut icon_fn,
            );
        }
    }

    if state.show_storage {
        let layout = storage::storage_layout(screen);
        let hover = storage::slot_at(&layout, state.mouse);
        let drag = state.drag.as_ref().and_then(|drag| match drag.source {
            DragSource::Storage(slot) => Some(slot),
            _ => None,
        });
        let UiState { icons, .. } = state;
        let mut icon_fn = |image: u16| -> Option<Image> { load_icon(scene, icons, image) };
        storage::draw(frame, &layout, &context, hover, drag, &mut icon_fn);
    }

    if state.show_quests {
        let layout = quests::quest_layout(screen, snapshot.quests.len(), state.quest_scroll);
        let hover_index = layout.row_at(state.mouse);
        quests::draw(
            frame,
            &layout,
            &context,
            snapshot,
            state.selected_quest,
            hover_index,
            state.mouse,
            u64::try_from(state.time_millis).unwrap_or(0),
        );
    }

    if state.show_mail {
        let layout = mail::mail_layout(
            screen,
            state.mail_tab,
            snapshot.mails.len(),
            state.mail_page,
            snapshot.inventory.len(),
        );
        let tab = state.mail_tab;
        let page = state.mail_page;
        let selected = state.mail_selected;
        let compose = state.mail_compose.clone();
        let focus = state.focus;
        let mouse = state.mouse;
        let UiState { icons, .. } = state;
        let mut icon_fn = |image: u16| -> Option<Image> { load_icon(scene, icons, image) };
        mail::draw(
            frame,
            &layout,
            &context,
            tab,
            page,
            selected,
            &compose,
            focus,
            mouse,
            &mut icon_fn,
        );
    }

    if state.show_guild {
        let layout = guild::guild_layout(
            screen,
            snapshot.guild_members().len(),
            snapshot.guild_buff_infos.len(),
            state.guild_member_scroll,
            state.guild_buff_scroll,
        );
        let gold_field = state.guild_gold.clone();
        let notice_field = state.guild_notice.clone();
        let UiState {
            icons,
            guild_tab,
            guild_selected_member,
            focus,
            mouse,
            ..
        } = state;
        let guild_tab = *guild_tab;
        let selected_member = *guild_selected_member;
        let focus = *focus;
        let mouse = *mouse;
        let mut icon_fn = |image: u16| -> Option<Image> { load_icon(scene, icons, image) };
        guild::draw(
            frame,
            &layout,
            &context,
            guild_tab,
            selected_member,
            &gold_field,
            &notice_field,
            focus,
            mouse,
            &mut icon_fn,
        );
    }

    if state.show_group {
        let layout = group::group_layout(screen, snapshot.group.members.len(), state.group_scroll);
        let members = snapshot.group_member_infos();
        let selected = state.group_selected;
        let name_field = state.group_name.clone();
        let UiState { focus, mouse, .. } = state;
        let (focus, mouse) = (*focus, *mouse);
        group::draw(
            frame,
            &layout,
            &context,
            &members,
            selected,
            &name_field,
            focus,
            snapshot.group.allow_group,
            snapshot.is_group_leader(),
            mouse,
        );
    }

    if state.show_market {
        let layout = market::market_layout(
            screen,
            state.market_tab,
            snapshot.market.listings.len(),
            state.market_page,
            state.market_scroll,
        );
        let hover = layout.row_at(state.mouse);
        let tab = state.market_tab;
        let page = state.market_page;
        let pages = snapshot.market.pages;
        let search = state.market_search.clone();
        let price = state.market_price.clone();
        let selected = state.market_selected;
        let focus = state.focus;
        let consign_item = state
            .selected_slot
            .and_then(|slot| snapshot.inventory_item(slot));
        // 切页签时 `market_notice_dismissed` 会记下当时的版本，旧提示不再画。
        let (fail, message) = state.market_notice(snapshot);
        let UiState { icons, .. } = state;
        let mut icon_fn = |image: u16| -> Option<Image> { load_icon(scene, icons, image) };
        market::draw(
            frame,
            &layout,
            &context,
            tab,
            &snapshot.market.listings,
            page,
            pages,
            selected,
            hover,
            &search,
            &price,
            consign_item,
            focus,
            fail,
            message.as_deref(),
            &mut icon_fn,
        );
    }

    if state.show_trade {
        let layout = trade::trade_layout(screen);
        let trade = snapshot.trade.clone();
        let locked = state.trade_locked || snapshot.trade.locked;
        let gold_field = state.trade_gold.clone();
        let (focus, mouse) = (state.focus, state.mouse);
        let UiState { icons, .. } = state;
        let mut icon_fn = |image: u16| -> Option<Image> { load_icon(scene, icons, image) };
        trade::draw(
            frame,
            &layout,
            &context,
            &trade,
            locked,
            &gold_field,
            focus,
            mouse,
            &mut icon_fn,
        );
    }

    if state.show_skills {
        let layout = skills::skill_layout(screen, snapshot.magics.len());
        skills::draw(frame, &layout, &context, state.selected_magic);
    }

    // 小地图（右上角）。放在世界/面板之后、tooltip 之前：它盖住地图与其它面板的右上角，
    // 但下面的 tooltip/右键菜单仍然可以压在它上面（与 Crystal 的控件层级不同，简化）。
    if state.show_minimap {
        let view = minimap::view_for(snapshot, (scene.map.width, scene.map.height));
        let title = snapshot.map.as_ref().map_or("", |map| map.title.as_str());
        let UiState {
            minimap_scene,
            language,
            ..
        } = state;
        minimap::draw(frame, screen, minimap_scene, &view, title, text, *language);
    }

    // 大地图（`B`）：屏幕居中的对话框，盖住世界与小地图（Crystal 的
    // `BigMapDialog` 也是独立控件，`GameScene.cs:337`）。
    if state.show_bigmap {
        let view = minimap::big_view_for(snapshot, (scene.map.width, scene.map.height));
        let title = snapshot.map.as_ref().map_or("", |map| map.title.as_str());
        let UiState {
            minimap_scene,
            language,
            ..
        } = state;
        bigmap::draw(frame, screen, minimap_scene, &view, title, text, *language);
    }

    if let Some(page) = &state.dialog {
        draw_dialog(frame, screen, page, text, state.language, state.mouse);
    }

    // 设置面板是模态的：画在普通面板之上、邀请弹窗之下。
    if state.show_settings {
        let layout = settings::settings_layout(screen);
        let settings_state = state.settings.clone();
        settings::draw(frame, &layout, &settings_state, text);
    }

    // 邀请弹窗（交易/组队）压在其它面板之上；它们也是模态的（吃掉点击）。
    if let Some(name) = &state.trade_invite {
        let layout = notice::notice_layout(screen);
        let message = trade::invite_message(name, state.language);
        notice::draw(frame, text, &layout, &message, state.language, state.mouse);
    } else if let Some(name) = &state.group_invite {
        let layout = notice::notice_layout(screen);
        let message = group::invite_message(name, state.language);
        notice::draw(frame, text, &layout, &message, state.language, state.mouse);
    }

    if state.context_menu.is_none() && !state.show_settings {
        if let Some(item) = hovered_item(state, snapshot, screen) {
            let content = tooltip::content_for_item(snapshot, item, state.language);
            let layout = tooltip::layout(&content, text, screen, state.mouse);
            tooltip::draw(frame, text, &layout);
        }
    }

    if let Some(menu) = &state.context_menu {
        let entries = menu.entries(snapshot, state.language);
        let panel = inventory::context_menu_rect(menu.anchor, entries.len(), screen);
        let hover = inventory::context_menu_entry_at(panel, entries.len(), state.mouse);
        inventory::draw_context_menu(frame, text, panel, &entries, hover);
    }

    if let Some(drag) = &state.drag {
        let image = snapshot.item_image(drag.item_index).and_then(|image| {
            let UiState { icons, .. } = state;
            load_icon(scene, icons, image)
        });
        inventory::draw_drag_ghost(frame, text, image.as_ref(), drag.count, state.mouse);
    }
}

/// 当前鼠标下的物品（背包 → 装备槽 → 个人仓库 → 行会仓库 → 交易格，先命中先返回）。
fn hovered_item<'a>(
    state: &UiState,
    snapshot: &'a WorldSnapshot,
    screen: Rect,
) -> Option<&'a InventoryItem> {
    if let Some(slot) = state.hovered_inventory_slot(screen) {
        if let Some(item) = snapshot.inventory_item(slot) {
            return Some(item);
        }
    }
    if let Some(slot) = state.hovered_equipment_slot(screen) {
        if let Some(item) = snapshot.equipment_item(slot) {
            return Some(item);
        }
    }
    if let Some(slot) = state.hovered_storage_slot(screen) {
        if let Some(item) = snapshot.storage_item(slot) {
            return Some(item);
        }
    }
    if let Some((own, slot)) = state.hovered_trade_slot(screen) {
        let items = if own {
            &snapshot.trade.items
        } else {
            &snapshot.trade.partner_items
        };
        if let Some(item) = trade::slot_item(items, slot) {
            return Some(item);
        }
    }
    let slot = state.hovered_guild_storage_slot(snapshot, screen)?;
    snapshot.guild_storage_item(slot)
}

/// 取物品图标：先查 UI 缓存，没有就让地图场景从 `items.wzl` 解码一次。
fn load_icon(
    scene: &mut MapScene,
    icons: &mut HashMap<u16, Option<Image>>,
    image: u16,
) -> Option<Image> {
    if let Some(cached) = icons.get(&image) {
        return cached.clone();
    }
    let loaded = scene.item_icon(image);
    icons.insert(image, loaded.clone());
    loaded
}

/// 面板底板：半透明深色 + 1px 边框 + 淡阴影。
pub fn draw_panel(frame: &mut Frame, rect: Rect) {
    frame.fill_rect_alpha(
        rect.x + 2,
        rect.y + 2,
        rect.width,
        rect.height,
        [0, 0, 0, 90],
    );
    frame.fill_rect_alpha(
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        hud::PANEL_BACKGROUND,
    );
    frame.fill_rect(rect.x, rect.y, rect.width, 1, hud::PANEL_BORDER);
    frame.fill_rect(rect.x, rect.bottom() - 1, rect.width, 1, hud::PANEL_BORDER);
    frame.fill_rect(rect.x, rect.y, 1, rect.height, hud::PANEL_BORDER);
    frame.fill_rect(rect.right() - 1, rect.y, 1, rect.height, hud::PANEL_BORDER);
}

/// 画 NPC 对话面板。
fn draw_dialog(
    frame: &mut Frame,
    screen: Rect,
    page: &ParsedPage,
    text: &UiText,
    language: Language,
    mouse: Point,
) {
    let layout =
        dialog::dialog_layout(screen, page, |label| text.text_width(label, hud::SIZE_BODY));
    draw_panel(frame, layout.panel);
    text.draw_clipped(
        frame,
        layout.panel.x + 12,
        layout.panel.y + 4,
        labels::Label::Dialog.text(language),
        hud::SIZE_BODY,
        hud::TITLE_COLOR,
        Rect::new(
            layout.panel.x,
            layout.panel.y,
            layout.panel.width,
            dialog::TITLE_HEIGHT,
        ),
    );
    text.draw_right(
        frame,
        layout.panel.right() - 12,
        layout.panel.y + 6,
        labels::Label::NpcHint.text(language),
        hud::SIZE_SMALL,
        hud::MUTED_COLOR,
    );
    let mut y = layout.lines_area.y;
    for line in &page.lines {
        text.draw_clipped(
            frame,
            layout.lines_area.x,
            y,
            line,
            hud::SIZE_BODY,
            hud::BODY_COLOR,
            layout.lines_area,
        );
        y = y.saturating_add(i32::try_from(dialog::LINE_HEIGHT).unwrap_or(0));
    }
    for (index, rect) in layout.buttons.iter().enumerate() {
        let hovered = rect.contains(mouse);
        let fill = if hovered {
            [92, 82, 42, 255]
        } else {
            [44, 52, 66, 255]
        };
        frame.fill_rect(rect.x, rect.y, rect.width, rect.height, fill);
        frame.fill_rect(rect.x, rect.y, rect.width, 1, [140, 156, 180, 255]);
        frame.fill_rect(rect.x, rect.bottom() - 1, rect.width, 1, [36, 44, 56, 255]);
        let label = page
            .buttons
            .get(index)
            .map(|button| {
                if button.label.is_empty() {
                    dialog::close_button_label(language).to_owned()
                } else {
                    button.label.clone()
                }
            })
            .unwrap_or_default();
        text.draw_center(
            frame,
            *rect,
            &label,
            hud::SIZE_SMALL,
            if hovered {
                [255, 255, 220, 255]
            } else {
                [255, 240, 180, 255]
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::{
        ChatLine, DamageEvent, EffectEvent, InventoryItem, ItemInfoView, MagicView, MapSnapshot,
        NpcDialog, SelfPlayer, ShopEntry, ShopGoods, WorldEntity,
    };
    use mir_core::Language;
    use mir_core::enums::{ChatType, DamageType, MirClass, MirDirection, MirGender, Spell};
    use mir_render::EntityKind;

    fn text() -> UiText {
        UiText::fallback()
    }

    fn item(unique_id: u64, item_index: i32, count: u16) -> InventoryItem {
        InventoryItem {
            unique_id,
            item_index,
            count,
            current_dura: 100,
            max_dura: 100,
        }
    }

    fn snapshot() -> WorldSnapshot {
        let mut snapshot = WorldSnapshot {
            self_player: Some(SelfPlayer {
                object_id: 7,
                real_id: 7,
                name: "测试者".to_owned(),
                position: Point::new(10, 10),
                direction: MirDirection::Down,
                hp: 30,
                mp: 20,
                hp_max: 40,
                mp_max: 40,
                level: 5,
                class: MirClass::Warrior,
                gender: MirGender::Male,
                hair: 0,
                gold: 4_321,
                credit: 0,
                dead: false,
                action: mir_render::animation::ActorAction::Standing,
                action_started_ms: 0,
                looks: crate::net::PlayerLooks::default(),
            }),
            map: Some(MapSnapshot {
                title: "比奇省".to_owned(),
                file_name: "0".to_owned(),
                ..MapSnapshot::default()
            }),
            inventory: vec![Some(item(11, 1, 2)), None],
            magics: vec![MagicView {
                name: "治愈术".to_owned(),
                spell: Spell::Healing,
                level: 1,
                key: 1,
                base_cost: 5,
                icon: 0,
                range: 3,
            }],
            chat: vec![ChatLine {
                speaker: Some("甲".to_owned()),
                text: "你好".to_owned(),
                kind: ChatType::Normal,
                system: None,
            }],
            ..WorldSnapshot::default()
        };
        snapshot.item_infos.insert(
            1,
            ItemInfoView {
                index: 1,
                name: "金创药".to_owned(),
                image: 5,
                price: 50,
                stack_size: 100,
                ..ItemInfoView::default()
            },
        );
        snapshot
    }

    fn with_npc(mut snapshot: WorldSnapshot) -> WorldSnapshot {
        snapshot.entities.insert(
            42,
            WorldEntity {
                object_id: 42,
                kind: EntityKind::Npc,
                name: "商人".to_owned(),
                position: Point::new(11, 11),
                direction: MirDirection::Down,
                image: 1,
                looks: crate::net::PlayerLooks::default(),
                dead: false,
                hidden: false,
                hp_percent: None,
                level: 0,
                class: MirClass::Warrior,
                name_colour: None,
                action: mir_render::animation::ActorAction::Standing,
                action_started_ms: 0,
            },
        );
        snapshot
    }

    fn with_dialog(mut snapshot: WorldSnapshot) -> WorldSnapshot {
        snapshot.npc_dialog_version = 1;
        snapshot.npc_dialog = Some(NpcDialog {
            lines: vec![
                "欢迎光临".to_owned(),
                "<买东西/@BUY>".to_owned(),
                "<离开/@exit>".to_owned(),
            ],
            object_id: Some(42),
            version: 1,
        });
        snapshot
    }

    fn with_shop(mut snapshot: WorldSnapshot) -> WorldSnapshot {
        snapshot.npc_goods_version = 1;
        snapshot.npc_goods = Some(ShopGoods {
            list: vec![ShopEntry {
                unique_id: 99,
                item_index: 1,
                name: "金创药".to_owned(),
                image: 5,
                price: 50,
                count: 1,
                current_dura: 0,
                max_dura: 0,
            }],
            rate: 1.0,
            version: 1,
        });
        snapshot
    }

    #[test]
    fn clicking_an_inventory_slot_selects_it_without_network_action() {
        let snapshot = snapshot();
        let mut state = UiState::new(Language::ZhCn);
        state.show_inventory = true;
        state.sync(&snapshot);
        let frame = Rect::screen(800, 600);
        let panel = inventory::inventory_panel_rect(frame);
        let slot = inventory::inventory_slot_rect(panel, 0).unwrap();
        state.mouse = slot.center();
        let action = state.on_click(&snapshot, &text(), frame, None);
        assert!(action.is_none(), "选格子不应发网络包");
        assert_eq!(state.selected_slot, Some(0));
        assert_eq!(
            state
                .selected_inventory_item(&snapshot)
                .map(|item| item.unique_id),
            Some(11)
        );
        assert!(state.drag.is_some(), "按下有物品的格子应开始拖拽");
    }

    #[test]
    fn double_clicking_an_inventory_item_uses_it() {
        let snapshot = snapshot();
        let mut state = UiState::new(Language::ZhCn);
        state.show_inventory = true;
        state.set_time(1_000);
        let frame = Rect::screen(800, 600);
        let panel = inventory::inventory_panel_rect(frame);
        state.mouse = inventory::inventory_slot_rect(panel, 0).unwrap().center();
        assert!(state.on_click(&snapshot, &text(), frame, None).is_none());
        state.set_time(1_200);
        let action = state
            .on_click(&snapshot, &text(), frame, None)
            .expect("第二次点击应是双击");
        assert_eq!(action, UiAction::UseItem { unique_id: 11 });
        assert!(state.drag.is_none(), "双击使用不应继续拖拽");
    }

    #[test]
    fn dragging_inventory_to_equipment_emits_move_item_with_inventory_source() {
        let mut snapshot = snapshot();
        let mut info = snapshot.item_infos[&1].clone();
        info.item_type = mir_core::enums::ItemType::Potion;
        snapshot.item_infos.insert(1, info);
        snapshot.equipment = vec![None; crate::net::EQUIPMENT_SLOTS];
        let mut state = UiState::new(Language::ZhCn);
        state.show_inventory = true;
        state.show_equipment = true;
        let frame = Rect::screen(800, 600);
        let inv = inventory::inventory_panel_rect(frame);
        let equip = equipment::equipment_layout(frame);
        state.mouse = inventory::inventory_slot_rect(inv, 0).unwrap().center();
        assert!(state.on_click(&snapshot, &text(), frame, None).is_none());
        assert!(state.drag.is_some());
        state.mouse = equip.slots[0].center();
        let action = state
            .on_mouse_release(&snapshot, frame)
            .expect("拖到装备槽应产生 MoveItem");
        assert_eq!(
            action,
            UiAction::MoveItem {
                grid: MirGridType::Inventory,
                from: 0,
                to: 0,
                to_grid: MirGridType::Equipment
            }
        );
    }

    #[test]
    fn dragging_between_inventory_slots_swaps_and_equipment_to_inventory_unequips() {
        let mut snapshot = snapshot();
        snapshot.inventory = vec![Some(item(11, 1, 2)), Some(item(12, 1, 1))];
        snapshot.equipment = vec![None; crate::net::EQUIPMENT_SLOTS];
        snapshot.equipment[3] = Some(item(13, 1, 1));
        let mut state = UiState::new(Language::ZhCn);
        state.show_inventory = true;
        state.show_equipment = true;
        let frame = Rect::screen(800, 600);
        let inv = inventory::inventory_panel_rect(frame);
        let equip = equipment::equipment_layout(frame);
        // 背包内交换。
        state.mouse = inventory::inventory_slot_rect(inv, 0).unwrap().center();
        let _ = state.on_click(&snapshot, &text(), frame, None);
        state.mouse = inventory::inventory_slot_rect(inv, 1).unwrap().center();
        assert_eq!(
            state.on_mouse_release(&snapshot, frame),
            Some(UiAction::MoveItem {
                grid: MirGridType::Inventory,
                from: 0,
                to: 1,
                to_grid: MirGridType::Inventory
            })
        );
        // 装备槽拖回背包。
        state.mouse = equip.slots[3].center();
        let _ = state.on_click(&snapshot, &text(), frame, None);
        state.mouse = inventory::inventory_slot_rect(inv, 1).unwrap().center();
        assert_eq!(
            state.on_mouse_release(&snapshot, frame),
            Some(UiAction::MoveItem {
                grid: MirGridType::Equipment,
                from: 3,
                to: 1,
                to_grid: MirGridType::Inventory
            })
        );
    }

    #[test]
    fn right_click_inventory_opens_a_context_menu_and_equip_moves_to_a_slot() {
        let mut snapshot = snapshot();
        let mut info = snapshot.item_infos[&1].clone();
        info.item_type = mir_core::enums::ItemType::Ring;
        snapshot.item_infos.insert(1, info);
        snapshot.equipment = vec![None; crate::net::EQUIPMENT_SLOTS];
        let mut state = UiState::new(Language::ZhCn);
        state.show_inventory = true;
        let frame = Rect::screen(800, 600);
        let inv = inventory::inventory_panel_rect(frame);
        state.mouse = inventory::inventory_slot_rect(inv, 0).unwrap().center();
        assert!(state.on_right_click(&snapshot, frame).is_none());
        let menu = state.context_menu.clone().expect("右键应打开菜单");
        let entries = menu.entries(&snapshot, Language::ZhCn);
        assert_eq!(entries.len(), 3, "使用/装备/丢弃");
        let panel = inventory::context_menu_rect(menu.anchor, entries.len(), frame);
        state.mouse = inventory::context_menu_entry_rect(panel, 1).center();
        let action = state
            .on_click(&snapshot, &text(), frame, None)
            .expect("点「装备」应产生动作");
        assert_eq!(
            action,
            UiAction::MoveItem {
                grid: MirGridType::Inventory,
                from: 0,
                to: mir_core::enums::EquipmentSlot::RingL as i32,
                to_grid: MirGridType::Equipment
            }
        );
        assert!(state.context_menu.is_none());
    }

    #[test]
    fn clicking_a_dialog_button_returns_its_script_key() {
        let snapshot = with_npc(with_dialog(snapshot()));
        let mut state = UiState::new(Language::ZhCn);
        state.sync(&snapshot);
        assert!(state.dialog.is_some());
        let frame = Rect::screen(800, 600);
        let page = state.dialog.as_ref().unwrap();
        let layout = dialog::dialog_layout(frame, page, |label| text().text_width(label, 14));
        state.mouse = layout.buttons[0].center();
        let action = state
            .on_click(&snapshot, &text(), frame, None)
            .expect("按钮应产生动作");
        assert_eq!(
            action,
            UiAction::CallNpc {
                object_id: 42,
                key: "@BUY".to_owned()
            }
        );
    }

    #[test]
    fn clicking_a_shop_row_buys_one_item() {
        let snapshot = with_npc(with_shop(snapshot()));
        let mut state = UiState::new(Language::ZhCn);
        state.sync(&snapshot);
        assert!(state.shop_open);
        let frame = Rect::screen(800, 600);
        let layout = shop::shop_layout(frame, 1, 0, shop::SHOP_LEFT);
        state.mouse = layout.rows[0].center();
        let action = state
            .on_click(&snapshot, &text(), frame, None)
            .expect("点货架应产生动作");
        assert_eq!(
            action,
            UiAction::BuyItem { unique_id: 99 },
            "买的是商店物品唯一编号"
        );
        assert_eq!(state.selected_goods, Some(0));
    }

    #[test]
    fn shop_scrolls_with_the_wheel_and_clamps_to_the_last_page() {
        let mut snapshot = with_npc(snapshot());
        let list: Vec<ShopEntry> = (0..30_u64)
            .map(|index| ShopEntry {
                unique_id: index,
                item_index: 1,
                name: format!("商品 {index}"),
                image: 5,
                price: 10,
                count: 1,
                current_dura: 0,
                max_dura: 0,
            })
            .collect();
        snapshot.npc_goods = Some(ShopGoods {
            list,
            rate: 1.0,
            version: 1,
        });
        snapshot.npc_goods_version = 1;
        let mut state = UiState::new(Language::ZhCn);
        state.sync(&snapshot);
        assert!(state.shop_open);
        assert!(state.on_wheel(&snapshot, 1), "滚轮向下应滚动");
        assert_eq!(state.shop_scroll, 1);
        assert!(state.scroll_shop(&snapshot, 100));
        assert_eq!(state.shop_scroll, 30 - shop::MAX_SHOP_ROWS);
        assert!(!state.scroll_shop(&snapshot, 1), "到底后不再变化");
        assert!(state.scroll_shop(&snapshot, -100));
        assert_eq!(state.shop_scroll, 0);
        // 货架换版本后回到顶部。
        state.shop_scroll = 5;
        snapshot.npc_goods_version = 2;
        snapshot.npc_goods.as_mut().unwrap().version = 2;
        state.sync(&snapshot);
        assert_eq!(state.shop_scroll, 0);
    }

    #[test]
    fn clicking_an_npc_in_the_world_opens_the_dialog() {
        let snapshot = with_npc(snapshot());
        let mut state = UiState::new(Language::ZhCn);
        state.sync(&snapshot);
        let action = state
            .on_click(
                &snapshot,
                &text(),
                Rect::screen(800, 600),
                Some(Point::new(11, 11)),
            )
            .expect("点 NPC 应发起对话");
        assert_eq!(
            action,
            UiAction::CallNpc {
                object_id: 42,
                key: "@main".to_owned()
            }
        );
        assert_eq!(state.npc_object_id, Some(42));
    }

    #[test]
    fn clicking_the_ground_walks_or_turns_toward_the_target() {
        let snapshot = snapshot();
        // 同列相邻：直接走。
        assert_eq!(
            walk_intent(&snapshot, Point::new(10, 11)),
            Some(WalkIntent {
                direction: MirDirection::Down,
                step: true
            })
        );
        // 已经在朝向下：走一步。
        assert_eq!(
            walk_intent(&snapshot, Point::new(10, 13)),
            Some(WalkIntent {
                direction: MirDirection::Down,
                step: true
            })
        );
        // 方向不同：先转身。
        assert_eq!(
            walk_intent(&snapshot, Point::new(13, 10)),
            Some(WalkIntent {
                direction: MirDirection::Right,
                step: false
            })
        );
        // 原地点自己：没有动作。
        assert_eq!(walk_intent(&snapshot, Point::new(10, 10)), None);
        // 面板外点地：产生 WorldMove。
        let mut state = UiState::new(Language::ZhCn);
        let action = state
            .on_click(
                &snapshot,
                &text(),
                Rect::screen(800, 600),
                Some(Point::new(11, 11)),
            )
            .expect("点地应走路");
        assert_eq!(
            action,
            UiAction::WorldMove {
                direction: MirDirection::DownRight,
                step: false
            }
        );
    }

    #[test]
    fn closing_npc_hides_dialog_and_shop() {
        let snapshot = with_npc(with_shop(with_dialog(snapshot())));
        let mut state = UiState::new(Language::ZhCn);
        state.sync(&snapshot);
        assert!(state.dialog.is_some());
        assert!(state.shop_open);
        state.close_npc();
        assert!(state.dialog.is_none());
        assert!(!state.shop_open);
    }

    #[test]
    fn empty_dialog_page_closes_everything() {
        let mut snapshot = with_shop(with_dialog(snapshot()));
        snapshot.npc_dialog_version = 2;
        snapshot.npc_dialog = Some(NpcDialog {
            lines: Vec::new(),
            object_id: Some(42),
            version: 2,
        });
        let mut state = UiState::new(Language::ZhCn);
        state.sync(&snapshot);
        assert!(state.dialog.is_none());
        assert!(!state.shop_open);
        assert_eq!(state.dialog_version, 2);
    }

    #[test]
    fn pressing_a_number_key_casts_and_binds_unbound_magic() {
        let base = snapshot();
        let mut state = UiState::new(Language::ZhCn);
        let actions = state.press_magic_key(&base, 1);
        assert_eq!(actions.len(), 1, "已绑定键不产生绑定动作");
        assert!(matches!(actions[0], UiAction::CastMagic { .. }));

        let mut unbound = base;
        unbound.magics[0].key = 0;
        let actions = state.press_magic_key(&unbound, 1);
        assert_eq!(actions.len(), 2);
        assert!(matches!(actions[0], UiAction::BindMagic { key: 1, .. }));
        assert!(matches!(actions[1], UiAction::CastMagic { .. }));
    }

    #[test]
    fn selected_slot_is_cleared_when_the_inventory_shrinks() {
        let mut snapshot = snapshot();
        let mut state = UiState::new(Language::ZhCn);
        state.selected_slot = Some(1);
        state.sync(&snapshot);
        snapshot.inventory.clear();
        state.sync(&snapshot);
        assert_eq!(state.selected_slot, None);
    }

    #[test]
    fn clicking_outside_panels_does_nothing() {
        let snapshot = with_dialog(with_npc(snapshot()));
        let mut state = UiState::new(Language::ZhCn);
        state.sync(&snapshot);
        state.mouse = Point::new(400, 20);
        assert!(
            state
                .on_click(&snapshot, &text(), Rect::screen(800, 600), None)
                .is_none()
        );
    }

    #[test]
    fn new_damage_and_effect_events_are_consumed_once_with_the_injected_clock() {
        let mut snapshot = snapshot();
        let mut state = UiState::new(Language::ZhCn);
        state.set_time(10_000);
        state.sync(&snapshot);
        assert!(state.damage_numbers.is_empty());
        snapshot.damage_events.push(DamageEvent {
            id: 1,
            object_id: 7,
            damage: Some(12),
            damage_type: Some(DamageType::Hit),
            location: None,
        });
        snapshot.effects.push(EffectEvent {
            id: 1,
            object_id: 7,
            effect: mir_core::enums::SpellEffect::MagicShieldUp,
            time: 600,
        });
        state.sync(&snapshot);
        assert_eq!(state.damage_numbers.len(), 1);
        assert_eq!(state.effects.len(), 1);
        assert_eq!(state.damage_numbers[0].text, "12");
        // 同一事件不会被重复消费。
        state.sync(&snapshot);
        assert_eq!(state.damage_numbers.len(), 1);
        assert_eq!(state.effects.len(), 1);
        // 500ms 时还在，2s 后消失。
        state.set_time(10_500);
        state.sync(&snapshot);
        assert_eq!(state.damage_numbers.len(), 1);
        assert_eq!(state.effects.len(), 1);
        state.set_time(12_100);
        state.sync(&snapshot);
        assert!(state.damage_numbers.is_empty(), "2s 后飘字应消失");
        assert!(state.effects.is_empty(), "光效应结束");
    }

    /// 在快照里塞一只怪物（`ObjectMonster` 的界面子集）。
    fn with_monster(
        mut snapshot: WorldSnapshot,
        object_id: u32,
        cell: Point,
        hp_percent: u8,
    ) -> WorldSnapshot {
        snapshot.entities.insert(
            object_id,
            WorldEntity {
                object_id,
                kind: EntityKind::Monster,
                name: "鸡".to_owned(),
                position: cell,
                direction: MirDirection::Left,
                image: 1,
                looks: crate::net::PlayerLooks::default(),
                dead: false,
                hidden: false,
                hp_percent: Some(hp_percent),
                level: 0,
                class: MirClass::Warrior,
                name_colour: None,
                action: mir_render::animation::ActorAction::Standing,
                action_started_ms: 0,
            },
        );
        snapshot
    }

    #[test]
    fn clicking_a_monster_locks_it_and_double_clicking_attacks_it() {
        let snapshot = with_monster(snapshot(), 42, Point::new(11, 10), 50);
        let mut state = UiState::new(Language::ZhCn);
        state.set_time(1_000);
        let frame = Rect::screen(800, 600);
        // 单击：只锁定（Crystal `MouseObject` → `TargetObjectID`，GameScene.cs:11442-11451）。
        let action = state.on_click(&snapshot, &text(), frame, Some(Point::new(11, 10)));
        assert!(action.is_none(), "单击怪物只锁定，不应发网络包");
        assert_eq!(state.target_object_id, Some(42));
        // 350 ms 内再点同一目标：双击 → 相邻时直接攻击。
        state.set_time(1_100);
        let action = state
            .on_click(&snapshot, &text(), frame, Some(Point::new(11, 10)))
            .expect("双击相邻怪物应攻击");
        assert_eq!(
            action,
            UiAction::AttackTarget {
                direction: MirDirection::Right
            }
        );
        assert_eq!(
            action.net_command(),
            Some(crate::net::NetCommand::Attack(MirDirection::Right)),
            "双击相邻怪物应产生 NetCommand::Attack"
        );
        assert!(state.approach.is_none(), "相邻时不需要靠近");
    }

    #[test]
    fn double_clicking_a_distant_target_walks_and_gives_up_after_three_steps() {
        let snapshot = with_monster(snapshot(), 42, Point::new(13, 10), 50);
        let mut state = UiState::new(Language::ZhCn);
        let frame = Rect::screen(800, 600);
        state.set_time(1_000);
        assert!(
            state
                .on_click(&snapshot, &text(), frame, Some(Point::new(13, 10)))
                .is_none()
        );
        state.set_time(1_100);
        let action = state
            .on_click(&snapshot, &text(), frame, Some(Point::new(13, 10)))
            .expect("双击远处目标应先走一步");
        assert_eq!(
            action,
            UiAction::WorldMove {
                direction: MirDirection::Right,
                step: true
            }
        );
        assert_eq!(state.approach.map(|approach| approach.steps_left), Some(2));

        // 之后每 600 ms 走一步（帧间隔沿用走动作时长，Frames.cs:159-166）。
        state.set_time(1_700);
        assert_eq!(
            state.tick_approach(&snapshot),
            Some(UiAction::WorldMove {
                direction: MirDirection::Right,
                step: true
            })
        );
        assert_eq!(state.approach.map(|approach| approach.steps_left), Some(1));
        state.set_time(2_300);
        assert_eq!(
            state.tick_approach(&snapshot),
            Some(UiAction::WorldMove {
                direction: MirDirection::Right,
                step: true
            })
        );
        assert_eq!(state.approach.map(|approach| approach.steps_left), Some(0));
        // 第 3 步之后还没到相邻：放弃（需求：最多连续 3 步）。
        state.set_time(2_900);
        assert_eq!(state.tick_approach(&snapshot), None);
        assert!(state.approach.is_none(), "走满 3 步应放弃靠近");
        assert_eq!(state.target_object_id, Some(42), "放弃靠近不清目标");
    }

    #[test]
    fn approach_attacks_automatically_once_the_target_is_adjacent() {
        let mut snapshot = with_monster(snapshot(), 42, Point::new(13, 10), 50);
        let mut state = UiState::new(Language::ZhCn);
        let frame = Rect::screen(800, 600);
        state.set_time(1_000);
        assert!(
            state
                .on_click(&snapshot, &text(), frame, Some(Point::new(13, 10)))
                .is_none()
        );
        state.set_time(1_100);
        assert!(
            state
                .on_click(&snapshot, &text(), frame, Some(Point::new(13, 10)))
                .is_some()
        );
        // 服务器把目标走到了自己旁边（ObjectWalk 的界面子集）。
        snapshot.entities.get_mut(&42).unwrap().position = Point::new(11, 10);
        state.set_time(1_700);
        let action = state.tick_approach(&snapshot).expect("相邻后应自动攻击");
        assert_eq!(
            action,
            UiAction::AttackTarget {
                direction: MirDirection::Right
            }
        );
        assert!(state.approach.is_none());
    }

    #[test]
    fn clicking_the_ground_clears_the_locked_target() {
        let snapshot = with_monster(snapshot(), 42, Point::new(11, 10), 50);
        let mut state = UiState::new(Language::ZhCn);
        state.set_time(1_000);
        let frame = Rect::screen(800, 600);
        assert!(
            state
                .on_click(&snapshot, &text(), frame, Some(Point::new(11, 10)))
                .is_none()
        );
        assert_eq!(state.target_object_id, Some(42));
        state.set_time(2_000);
        let action = state
            .on_click(&snapshot, &text(), frame, Some(Point::new(13, 13)))
            .expect("点空地仍然走路");
        assert!(matches!(action, UiAction::WorldMove { .. }));
        assert_eq!(
            state.target_object_id, None,
            "点空地清目标（GameScene.cs:11450）"
        );
    }

    #[test]
    fn dead_targets_are_dropped_on_sync() {
        let mut snapshot = with_monster(snapshot(), 42, Point::new(11, 10), 50);
        let mut state = UiState::new(Language::ZhCn);
        state.target_object_id = Some(42);
        state.approach = Some(Approach {
            target_id: 42,
            steps_left: 2,
            next_step_at: 0,
            pickup: false,
        });
        state.sync(&snapshot);
        assert_eq!(state.target_object_id, Some(42), "活着的目标保留");
        snapshot.entities.get_mut(&42).unwrap().dead = true;
        state.sync(&snapshot);
        assert_eq!(state.target_object_id, None, "目标死亡后清空锁定");
        assert!(state.approach.is_none());
    }

    #[test]
    fn escape_closes_the_bigmap_before_other_panels() {
        let mut state = UiState::new(Language::ZhCn);
        state.show_bigmap = true;
        state.show_inventory = true;
        assert_eq!(state.escape(), EscapeResult::Bigmap);
        assert!(!state.show_bigmap);
        assert!(state.show_inventory, "一次 Esc 只关一层");
        let result = state.escape();
        assert_eq!(result, EscapeResult::Panels);
        assert!(!state.show_inventory);
        assert!(result.closed());
        assert_eq!(state.escape(), EscapeResult::Nothing);
        assert!(!EscapeResult::Nothing.closed());
    }

    #[test]
    fn clicking_the_bigmap_panel_does_not_walk() {
        let snapshot = with_monster(snapshot(), 42, Point::new(11, 10), 50);
        let mut state = UiState::new(Language::ZhCn);
        state.show_bigmap = true;
        let frame = Rect::screen(800, 600);
        let panel = bigmap::panel_rect(frame);
        state.mouse = panel.center();
        let action = state.on_click(&snapshot, &text(), frame, Some(Point::new(13, 13)));
        assert!(action.is_none(), "点大地图面板不应产生走路动作");
        assert!(state.bigmap_clicked(frame));
        assert_eq!(state.target_object_id, None, "点面板也不应锁定目标");
        // 面板外仍然可以点地走路。
        state.mouse = Point::new(10, 560);
        assert!(!state.bigmap_clicked(frame));
    }
}
