//! 独立商城面板（Crystal `GameshopDialog`，`Client/MirScenes/GameshopDialog.cs`）。
//!
//! # 与市场面板「商城页签」的区别
//!
//! 市场面板的第四个页签（[`crate::ui::market`]）展示的是服务端市场协议
//! `S.NPCMarket{ItemType = GameShop}`，仅支持元宝结算、列表不显示实时库存。
//! 本面板是 Crystal 真正的独立商城窗口：数据源是登录链的 `S.GameShopInfo`
//! （逐商品目录 + 库存档位，见 [`crate::net::GameShopState`]）与购买后的
//! `S.GameShopStock`（`GIndex` + 新库存），支持**元宝/金币双币**、分类/职业/
//! 新品/促销/置顶筛选、搜索与 8 格网格分页。两者数据源、选中、筛选状态完全隔离。
//!
//! # Crystal 对照
//!
//! * 4 列 × 2 行 = 8 格商品网格（`GameshopDialog.cs` 构造函数 `Grid = new GameShopCell[4 * 2]`）；
//! * 职业筛选按钮 `ALL / War / Sin / Tao / Wiz / Arch`（`:474-497`，`ClassFilter` 取
//!   `"Show All"` / `"Warrior"` / `"Assassin"` / `"Taoist"` / `"Wizard"` / `"Archer"`；
//!   商品 `Class == "All"` 或等于当前职业时通过，见 `GetCategories` 与 `UpdateFilter`）；
//! * 分区筛选 `AllItems / TopItems / DealItems / NewItems`（`:26-28`，新品 = 上架时间在
//!   7 天内，`GameScene.cs:6678-6684`）；
//! * 分类列表先加 `Show All`，再按出现顺序去重（`GetCategories`）；搜索结果按
//!   物品名小写包含匹配（搜索框 `GameshopDialog.cs` 的 `SearchTextBox`）；列表按
//!   物品名排序（`OrderBy(x => x.Info.FriendlyName)`）；
//! * 支付方式单选：金币默认勾选、元宝可切换（`PaymentTypeGold.Checked = true`，
//!   `MirGameShopCell.cs:59-88` 的 `PType_Clicked` 互斥）；购买前检查
//!   `CanBuyCredit` / `CanBuyGold` 与余额（`BuyProduct`，`:648-760`）；
//! * 库存显示：`>= 99` 显示 `99+`、`0` 显示 `∞`、售罄（本作保留条目）显示「售罄」
//!   （`MirGameShopCell.UpdateText`，`:189-299`）；
//! * 购买份数夹取：`1..=99`，且不超过 5 组（`StackSize`）或剩余库存
//!   （`MirGameShopCell.cs:148-153`）。
//!
//! # 与 Crystal 的差异（已在 R50 契约/裁决中登记）
//!
//! * Crystal 收到 `S.GameShopStock{StockLevel = 0}` 会把商品从列表删除
//!   （`GameScene.cs:6686-6697`）；本作保留条目并显示「售罄」、禁止购买
//!   （R50 §1.3「库存零可见」）。
//! * Crystal 商城按钮走 `KeybindOptions.GameShop` 可重绑；本作先固定热键 `K`
//!   （R51 再开放重绑），只在 live 态、无输入焦点时生效。
//! * 缺 `docs/assets/gameshop-manifest.json`（R50-D 在产）时商品图标用占位方块 +
//!   `partial` 标记，**不伪造图标素材**；有清单时仍是同一绘制路径（清单里的自定义
//!   图标待 D 交付后接入，本面板不回退成假图标）。
//!
//! 布局函数与命中测试共用同一份矩形，无头测试可以在不开窗口的情况下点中每个按钮。

use std::path::Path;

use mir_core::enums::MirClass;
use mir_core::{Language, Point};
use mir_render::{Frame, Image, scale_to_fit};

use crate::geom::Rect;
use crate::net::{GameShopEntry, GameShopState};
use crate::ui::PanelContext;
use crate::ui::hud::{BODY_COLOR, GOLD_COLOR, MUTED_COLOR, SIZE_BODY, SIZE_SMALL, TITLE_COLOR};
use crate::ui::input::{self, TextField};
use crate::ui::labels;

/// 面板宽度。
pub const PANEL_WIDTH: u32 = 680;
/// 面板高度。
pub const PANEL_HEIGHT: u32 = 500;
/// 小窗口下的最小宽度。
pub const MIN_PANEL_WIDTH: u32 = 300;
/// 小窗口下的最小高度。
pub const MIN_PANEL_HEIGHT: u32 = 180;
/// 内边距。
pub const PADDING: u32 = 8;
/// 标题高度。
pub const TITLE_HEIGHT: u32 = 24;
/// 筛选按钮行高。
pub const ROW_HEIGHT: u32 = 20;
/// 网格列数（Crystal 4 列）。
pub const GRID_COLUMNS: usize = 4;
/// 网格行数（Crystal 2 行）。
pub const GRID_ROWS: usize = 2;
/// 一页商品数：Crystal 固定 8 格（`GameShopCell[4 * 2]`）。
pub const PAGE_SIZE: usize = GRID_COLUMNS * GRID_ROWS;
/// 商品格之间的间距。
pub const CELL_GAP: u32 = 6;
/// 商品格最大高度（内容少时不再拉高）。
pub const CELL_MAX_HEIGHT: u32 = 108;
/// 分类栏宽度。
pub const CATEGORY_WIDTH: u32 = 132;
/// 分类行高。
pub const CATEGORY_ROW: u32 = 18;
/// 分类栏标题高度。
pub const CATEGORY_HEADER: u32 = 18;
/// 支付/数量行高。
pub const PAY_ROW_HEIGHT: u32 = 26;
/// 翻页行高。
pub const PAGER_HEIGHT: u32 = 20;
/// 底部提示行高。
pub const FOOTER_HEIGHT: u32 = 18;
/// 按钮行高（筛选/关闭）。
pub const BUTTON_HEIGHT: u32 = 20;
/// 搜索框宽度。
pub const SEARCH_WIDTH: u32 = 170;
/// 商品图标的显示边长。
pub const ICON_SIZE: u32 = 32;

/// 元宝结算（Crystal `PType = 0`）。
pub const PAY_CREDIT: u8 = 0;
/// 金币结算（Crystal `PType = 1`）。
pub const PAY_GOLD: u8 = 1;
/// 购买份数上限（Crystal `ushort Quantity`，`MirGameShopCell.cs:148`）。
pub const MAX_QUANTITY: u8 = 99;
/// 新品窗口：上架 7 天内（Crystal `GameScene.cs:6680` 的 `AddDays(-7)`）。
pub const NEW_WINDOW_SECONDS: i64 = 7 * 24 * 60 * 60;

/// 面板底色 / 交互色。
pub const CELL_COLOR: [u8; 4] = [26, 30, 38, 235];
/// 选中格底色。
pub const SELECTED_COLOR: [u8; 4] = [86, 74, 32, 235];
/// 悬停格底色。
pub const HOVER_COLOR: [u8; 4] = [44, 56, 76, 235];
/// 普通按钮底色。
pub const BUTTON_COLOR: [u8; 4] = [48, 54, 66, 255];
/// 激活按钮底色。
pub const BUTTON_ACTIVE_COLOR: [u8; 4] = [96, 84, 44, 255];
/// 禁用按钮底色。
pub const BUTTON_DISABLED_COLOR: [u8; 4] = [32, 34, 40, 255];
/// 失败提示红字。
pub const FAIL_COLOR: [u8; 4] = [255, 132, 132, 255];
/// 成功/正向提示绿字。
pub const SUCCESS_COLOR: [u8; 4] = [150, 226, 150, 255];
/// 售罄/不可用的灰字。
pub const DIMMED_COLOR: [u8; 4] = [140, 146, 156, 255];

/// 商城面板自己的双语文案。
///
/// 不放 [`crate::ui::labels::Label`]：R50-C 的写范围不含 `ui/labels.rs`，而新增标签
/// 要求 `cargo test -p mir-core embedded_pack_matches_builtin_text` 绿（服务端文本包
/// 同步由 R51 处理）。这里双语文案同样由单测覆盖「每种语言都非空、中英不同」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameShopLabel {
    /// 面板标题。
    Title,
    /// 关闭按钮。
    Close,
    /// 分类栏标题。
    Category,
    /// 分类栏「全部分类」。
    AllCategories,
    /// 搜索框占位文案。
    SearchHint,
    /// 搜索框氪标。
    Search,
    /// 支付方式：元宝。
    PayCredit,
    /// 支付方式：金币。
    PayGold,
    /// 数量。
    Quantity,
    /// 购买按钮。
    Buy,
    /// 库存。
    Stock,
    /// 售罄。
    SoldOut,
    /// 不限量库存。
    Unlimited,
    /// 新品分区。
    New,
    /// 促销分区（Crystal `DealItems`）。
    Deal,
    /// 置顶分区（Crystal `TopItems`）。
    Top,
    /// 全部分区（Crystal `AllItems`）。
    AllItems,
    /// 上一页。
    Prev,
    /// 下一页。
    Next,
    /// 页码格式里的「第 / 页」。
    Page,
    /// 空目录。
    Empty,
    /// 筛选无结果。
    NoMatch,
    /// 未连接服务器。
    Disconnected,
    /// 未选择商品。
    NoSelection,
    /// 该商品不支持所选货币。
    PayTypeUnavailable,
    /// 金币不足。
    NotEnoughGold,
    /// 元宝不足。
    NotEnoughCredit,
    /// 库存不足（勾选份数超过剩余）。
    OutOfStock,
    /// 缺图标清单（R50-D 未交付）时的 `partial` 标记。
    Partial,
    /// 底部操作提示。
    Hint,
    /// 职业：战士。
    Warrior,
    /// 职业：法师。
    Wizard,
    /// 职业：道士。
    Taoist,
    /// 职业：刺客。
    Assassin,
    /// 职业：弓箭手。
    Archer,
    /// 职业筛选：全部。
    AllClasses,
}

impl GameShopLabel {
    /// 所有成员（单测遍历，保证两种语言都覆盖）。
    pub const ALL: &'static [Self] = &[
        Self::Title,
        Self::Close,
        Self::Category,
        Self::AllCategories,
        Self::SearchHint,
        Self::Search,
        Self::PayCredit,
        Self::PayGold,
        Self::Quantity,
        Self::Buy,
        Self::Stock,
        Self::SoldOut,
        Self::Unlimited,
        Self::New,
        Self::Deal,
        Self::Top,
        Self::AllItems,
        Self::Prev,
        Self::Next,
        Self::Page,
        Self::Empty,
        Self::NoMatch,
        Self::Disconnected,
        Self::NoSelection,
        Self::PayTypeUnavailable,
        Self::NotEnoughGold,
        Self::NotEnoughCredit,
        Self::OutOfStock,
        Self::Partial,
        Self::Hint,
        Self::Warrior,
        Self::Wizard,
        Self::Taoist,
        Self::Assassin,
        Self::Archer,
        Self::AllClasses,
    ];

    /// 文案（中 / 英）。
    #[must_use]
    pub const fn text(self, language: Language) -> &'static str {
        let zh = self.zh();
        let en = self.en();
        if matches!(language, Language::ZhCn) {
            zh
        } else {
            en
        }
    }

    const fn zh(self) -> &'static str {
        match self {
            Self::Title => "商城",
            Self::Close => "关闭",
            Self::Category => "分类",
            Self::AllCategories => "全部分类",
            Self::SearchHint => "搜索物品名",
            Self::Search => "搜索",
            Self::PayCredit => "元宝",
            Self::PayGold => "金币",
            Self::Quantity => "数量",
            Self::Buy => "买下",
            Self::Stock => "库存",
            Self::SoldOut => "售罄",
            Self::Unlimited => "不限量",
            Self::New => "新品",
            Self::Deal => "促销",
            Self::Top => "置顶",
            Self::AllItems | Self::AllClasses => "全部",
            Self::Prev => "上一页",
            Self::Next => "下一页",
            Self::Page => "页",
            Self::Empty => "商城暂无商品",
            Self::NoMatch => "没有符合条件的商品",
            Self::Disconnected => "未连接服务器",
            Self::NoSelection => "先选中一件商品",
            Self::PayTypeUnavailable => "该商品不支持所选货币",
            Self::NotEnoughGold => "金币不足",
            Self::NotEnoughCredit => "元宝不足",
            Self::OutOfStock => "库存不足",
            Self::Partial => "图标待补 partial",
            Self::Hint => "K 关闭 · 选中商品后点买下",
            Self::Warrior => "战士",
            Self::Wizard => "法师",
            Self::Taoist => "道士",
            Self::Assassin => "刺客",
            Self::Archer => "弓箭手",
        }
    }

    const fn en(self) -> &'static str {
        match self {
            Self::Title => "Shop",
            Self::Close => "Close",
            Self::Category => "Category",
            Self::AllCategories => "All categories",
            Self::SearchHint => "Search items",
            Self::Search => "Search",
            Self::PayCredit => "Credits",
            Self::PayGold => "Gold",
            Self::Quantity => "Qty",
            Self::Buy => "Buy",
            Self::Stock => "Stock",
            Self::SoldOut => "Sold out",
            Self::Unlimited => "Unlimited",
            Self::New => "New",
            Self::Deal => "Deals",
            Self::Top => "Top",
            Self::AllItems | Self::AllClasses => "All",
            Self::Prev => "Prev",
            Self::Next => "Next",
            Self::Page => "Page",
            Self::Empty => "The shop has no items",
            Self::NoMatch => "No matching items",
            Self::Disconnected => "Not connected",
            Self::NoSelection => "Select an item first",
            Self::PayTypeUnavailable => "This item cannot be bought with that currency",
            Self::NotEnoughGold => "Not enough gold",
            Self::NotEnoughCredit => "Not enough credits",
            Self::OutOfStock => "Out of stock",
            Self::Partial => "icons pending (partial)",
            Self::Hint => "K close · select an item, then Buy",
            Self::Warrior => "Warrior",
            Self::Wizard => "Wizard",
            Self::Taoist => "Taoist",
            Self::Assassin => "Assassin",
            Self::Archer => "Archer",
        }
    }
}

/// Crystal 的职业筛选（`GameshopDialog.cs:474-497`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClassFilter {
    /// `ALL` 按钮 / `ClassFilter = "Show All"`。
    #[default]
    All,
    /// 战士。
    Warrior,
    /// 刺客。
    Assassin,
    /// 道士。
    Taoist,
    /// 法师。
    Wizard,
    /// 弓箭手。
    Archer,
}

impl ClassFilter {
    /// 按钮顺序（Crystal：ALL / War / Sin / Tao / Wiz / Arch）。
    pub const ALL_FILTERS: [Self; 6] = [
        Self::All,
        Self::Warrior,
        Self::Assassin,
        Self::Taoist,
        Self::Wizard,
        Self::Archer,
    ];

    /// 按钮文案。
    #[must_use]
    pub const fn label(self) -> GameShopLabel {
        match self {
            Self::All => GameShopLabel::AllClasses,
            Self::Warrior => GameShopLabel::Warrior,
            Self::Assassin => GameShopLabel::Assassin,
            Self::Taoist => GameShopLabel::Taoist,
            Self::Wizard => GameShopLabel::Wizard,
            Self::Archer => GameShopLabel::Archer,
        }
    }

    /// 商品 `Class` 字段里的 Crystal 字符串（`Shared/Data/ItemData.cs` 的
    /// `GameShopItem.Class` 与 `GameScene.User.Class.ToString()` 对齐）。
    #[must_use]
    pub const fn crystal_name(self) -> &'static str {
        match self {
            Self::All => "Show All",
            Self::Warrior => "Warrior",
            Self::Assassin => "Assassin",
            Self::Taoist => "Taoist",
            Self::Wizard => "Wizard",
            Self::Archer => "Archer",
        }
    }

    /// 商品是否通过职业筛选。
    ///
    /// Crystal `GetCategories`/`UpdateFilter` 的判定：`ClassFilter == "Show All"`、
    /// 商品 `Class == "All"` 或等于当前职业三者之一。
    #[must_use]
    pub fn matches(self, item_class: &str) -> bool {
        matches!(self, Self::All) || item_class == self.crystal_name() || item_class == "All"
    }

    /// 打开面板时的默认筛选：Crystal `Show()` 把 `ClassFilter` 设为玩家职业
    /// （`GameshopDialog.cs:508`）；没有角色信息时退回「全部」。
    #[must_use]
    pub const fn from_class(class: Option<MirClass>) -> Self {
        match class {
            Some(MirClass::Warrior) => Self::Warrior,
            Some(MirClass::Wizard) => Self::Wizard,
            Some(MirClass::Taoist) => Self::Taoist,
            Some(MirClass::Assassin) => Self::Assassin,
            Some(MirClass::Archer) => Self::Archer,
            None => Self::All,
        }
    }
}

/// Crystal 的分区筛选（`GameshopDialog.cs:26-28` 的三个按钮 + 「全部」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SectionFilter {
    /// `AllItems`。
    #[default]
    All,
    /// `TopItems`：`TopItem == true`。
    Top,
    /// `DealItems`：`Deal == true`。
    Deal,
    /// `NewItems`：上架 7 天内（`Date > now - 7d`）。
    New,
}

impl SectionFilter {
    /// 按钮顺序。
    pub const ALL_SECTIONS: [Self; 4] = [Self::All, Self::Top, Self::Deal, Self::New];

    /// 按钮文案。
    #[must_use]
    pub const fn label(self) -> GameShopLabel {
        match self {
            Self::All => GameShopLabel::AllItems,
            Self::Top => GameShopLabel::Top,
            Self::Deal => GameShopLabel::Deal,
            Self::New => GameShopLabel::New,
        }
    }

    /// 商品是否通过分区筛选。`now_sec` 是当前 Unix 秒。
    #[must_use]
    pub fn matches(self, entry: &GameShopEntry, now_sec: i64) -> bool {
        match self {
            Self::All => true,
            Self::Top => entry.item.top_item,
            Self::Deal => entry.item.deal,
            Self::New => is_new(entry, now_sec),
        }
    }
}

/// 商品是否「新品」：上架时间在 7 天内。
///
/// Crystal `GameScene.cs:6678-6684`：`p.Item.Date > DateTime.Now.AddDays(-7)` 时
/// 把 `NewButton.Visible` 置真；未设置日期（`DateTimeBinary` 0）不算新品。
#[must_use]
pub fn is_new(entry: &GameShopEntry, now_sec: i64) -> bool {
    if entry.item.date.is_unset() {
        return false;
    }
    entry.item.date.to_unix_seconds() > now_sec - NEW_WINDOW_SECONDS
}

/// 目录里是否有新品（决定「新品」分区按钮是否可用；Crystal 用按钮可见性表达）。
#[must_use]
pub fn has_new_items(entries: &[GameShopEntry], now_sec: i64) -> bool {
    entries.iter().any(|entry| is_new(entry, now_sec))
}

/// 商品显示名：`S.GameShopInfo` 自带完整 `ItemInfo`，直接用物品名
/// （Crystal 用 `Info.FriendlyName`；协议里没有更友好的名字，`name` 即服务端本地化名）。
#[must_use]
pub fn display_name(entry: &GameShopEntry) -> &str {
    &entry.item.info.name
}

/// 分类列表（Crystal `GetCategories()`）：先放「全部分类」占位（`None`），
/// 然后按出现顺序去重；已按当前职业/分区/搜索过滤。
#[must_use]
pub fn category_names(
    entries: &[GameShopEntry],
    class: ClassFilter,
    section: SectionFilter,
    search: &str,
    now_sec: i64,
) -> Vec<String> {
    let needle = search.trim().to_lowercase();
    let mut out: Vec<String> = Vec::new();
    for entry in entries {
        if !class.matches(&entry.item.class) || !section.matches(entry, now_sec) {
            continue;
        }
        if !needle.is_empty() && !display_name(entry).to_lowercase().contains(&needle) {
            continue;
        }
        let category = entry.item.category.trim();
        if category.is_empty() {
            continue;
        }
        if !out.iter().any(|known| known == category) {
            out.push(category.to_owned());
        }
    }
    out
}

/// 当前筛选结果（Crystal `GetCategories` 末段的 `OrderBy(Info.FriendlyName)`）。
///
/// 过滤顺序：职业 → 分区 → 分类 → 搜索；结果按物品名排序（同名的按 `GIndex` 稳定排序，
/// 保证两页之间顺序不抖动）。
#[must_use]
pub fn visible_entries<'a>(
    entries: &'a [GameShopEntry],
    class: ClassFilter,
    section: SectionFilter,
    category: Option<&str>,
    search: &str,
    now_sec: i64,
) -> Vec<&'a GameShopEntry> {
    let needle = search.trim().to_lowercase();
    let mut out: Vec<&GameShopEntry> = entries
        .iter()
        .filter(|entry| {
            class.matches(&entry.item.class)
                && section.matches(entry, now_sec)
                && category.is_none_or(|wanted| entry.item.category.trim() == wanted.trim())
                && (needle.is_empty() || display_name(entry).to_lowercase().contains(&needle))
        })
        .collect();
    out.sort_by(|left, right| {
        display_name(left)
            .to_lowercase()
            .cmp(&display_name(right).to_lowercase())
            .then_with(|| left.item.g_index.cmp(&right.item.g_index))
    });
    out
}

/// 一页显示的商品数（至少 1 页）。
#[must_use]
pub fn page_count(filtered: usize) -> usize {
    filtered.div_ceil(PAGE_SIZE).max(1)
}

/// 取某一页的商品（越界返回空）。
#[must_use]
pub fn page_slice<'a>(filtered: &'a [&'a GameShopEntry], page: usize) -> &'a [&'a GameShopEntry] {
    let start = page.saturating_mul(PAGE_SIZE);
    if start >= filtered.len() {
        return &[];
    }
    let end = start.saturating_add(PAGE_SIZE).min(filtered.len());
    &filtered[start..end]
}

/// 库存显示（Crystal `MirGameShopCell.UpdateText`，`MirGameShopCell.cs:189-299`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StockDisplay {
    /// 售罄（本作保留条目显示；Crystal 直接删条目）。
    SoldOut,
    /// 不限量（Crystal 的 `0 → "∞"`；只在商品原始 `item.stock == 0` 时出现）。
    Unlimited,
    /// `>= 99` 显示 `99+`。
    AtLeast99,
    /// 具体剩余。
    Count(i32),
}

/// 计算库存显示。
///
/// 是否不限量只看商品原始 `item.stock`（Crystal `GameShopItem.Stock` 上架总量）：
/// `item.stock == 0` → `∞`（服务端给这种商品下发 `StockLevel = 0`，见
/// `crates/mir-server/src/market.rs::game_shop_info_packets`）；`item.stock > 0` 是
/// 限量，按剩余 `stock_level` 显示，剩余 0 显示「售罄」。`i_stock` 不参与判定——
/// 它只是个人/全服计数模式标记（服务端 `purchased`），限量商品 `i_stock == true`
/// 仍按剩余库存显示。
#[must_use]
pub fn stock_display(entry: &GameShopEntry) -> StockDisplay {
    if entry.sold_out {
        return StockDisplay::SoldOut;
    }
    if entry.item.stock <= 0 {
        return StockDisplay::Unlimited;
    }
    if entry.stock_level <= 0 {
        return StockDisplay::SoldOut;
    }
    if entry.stock_level >= 99 {
        return StockDisplay::AtLeast99;
    }
    StockDisplay::Count(entry.stock_level)
}

/// 库存文案（`99+` / `∞` / 数字 / 「售罄」）。
#[must_use]
pub fn stock_text(entry: &GameShopEntry, language: Language) -> String {
    match stock_display(entry) {
        StockDisplay::SoldOut => GameShopLabel::SoldOut.text(language).to_owned(),
        StockDisplay::Unlimited => "∞".to_owned(),
        StockDisplay::AtLeast99 => "99+".to_owned(),
        StockDisplay::Count(count) => labels::format_number(u64::try_from(count).unwrap_or(0)),
    }
}

/// 单价（按结算货币）。元宝 = `CreditPrice`，金币 = `GoldPrice`。
#[must_use]
pub fn unit_price(entry: &GameShopEntry, pay_type: u8) -> u64 {
    if pay_type == PAY_CREDIT {
        u64::from(entry.item.credit_price)
    } else {
        u64::from(entry.item.gold_price)
    }
}

/// 总价 = 单价 × 份数（`u64` 防溢出；份数已被夹到 `<= 99`）。
#[must_use]
pub fn total_price(entry: &GameShopEntry, quantity: u8, pay_type: u8) -> u64 {
    unit_price(entry, pay_type).saturating_mul(u64::from(quantity.max(1)))
}

/// 夹取购买份数：`1..=99`，不超过 5 组（堆叠上限 / 每次数量）与剩余库存。
///
/// Crystal `MirGameShopCell.cs:148-153`：
/// `if ((decimal)(Quantity * Item.Count) / Item.Info.StackSize) > 5` 时夹到
/// `(5 * StackSize) / Count`；随后 `if (Item.Stock != 0 && Quantity > Item.Stock)`
/// 夹到库存（`Stock == 0` = 不限量，不夹）。这里用整数运算等价实现。
///
/// R50-C2：限量判定用商品原始 `item.stock`（Crystal `Stock`）；剩余取
/// `stock_level`，剩余 0 时份数收边到下限 1，购买由 [`check_buy`] 拦截。
#[must_use]
pub fn clamp_quantity(entry: &GameShopEntry, quantity: u8) -> u8 {
    let mut clamped = quantity.clamp(1, MAX_QUANTITY);
    let per_purchase = u32::from(entry.item.count.max(1));
    let stack_size = u32::from(entry.item.info.stack_size.max(1));
    let max_by_stacks = (5 * stack_size / per_purchase).max(1);
    let max_by_stacks =
        u8::try_from(max_by_stacks.min(u32::from(MAX_QUANTITY))).unwrap_or(MAX_QUANTITY);
    clamped = clamped.min(max_by_stacks);
    // 限量（`item.stock > 0`）按剩余库存夹取；`item.stock == 0` 是不限量，不夹。
    if entry.item.stock > 0 {
        let remaining = u8::try_from(entry.stock_level.max(0)).unwrap_or(MAX_QUANTITY);
        clamped = clamped.min(remaining.max(1));
    }
    clamped
}

/// 购买检查失败的原因（本地拦截，不发包；与 Crystal `BuyProduct` 的检查一一对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuyProblem {
    /// 未连接服务器（断线态）。
    Disconnected,
    /// 没选中商品。
    NoSelection,
    /// 商品售罄（本作保留条目，禁用购买）。
    SoldOut,
    /// 该商品不支持所选货币（`CanBuyCredit` / `CanBuyGold` 为假）。
    PayTypeUnavailable,
    /// 金币不足。
    NotEnoughGold,
    /// 元宝不足。
    NotEnoughCredit,
    /// 库存不足（勾选份数超过剩余）。
    OutOfStock,
}

impl BuyProblem {
    /// 对应文案。
    #[must_use]
    pub const fn label(self) -> GameShopLabel {
        match self {
            Self::Disconnected => GameShopLabel::Disconnected,
            Self::NoSelection => GameShopLabel::NoSelection,
            Self::SoldOut => GameShopLabel::SoldOut,
            Self::PayTypeUnavailable => GameShopLabel::PayTypeUnavailable,
            Self::NotEnoughGold => GameShopLabel::NotEnoughGold,
            Self::NotEnoughCredit => GameShopLabel::NotEnoughCredit,
            Self::OutOfStock => GameShopLabel::OutOfStock,
        }
    }
}

/// 通过检查的购买请求（映射到 `C.GameshopBuy{GIndex, Quantity, PType}`，包 133）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuyOrder {
    /// 商品编号。
    pub g_index: i32,
    /// 购买份数。
    pub quantity: u8,
    /// 结算货币：`0` 元宝 / `1` 金币。
    pub p_type: i32,
}

/// 点击「买下」时的本地检查（Crystal `BuyProduct`，`MirGameShopCell.cs:648-760`）。
///
/// * 未连接、未选中、售罄 → 对应原因；
/// * 按当前货币检查 `CanBuyCredit` / `CanBuyGold`；
/// * 按「单价 × 份数」检查金币/元宝余额（Crystal 用 `GameScene.Gold` / `Credit`）；
/// * 通过后把份数夹到 `clamp_quantity` 再发包。
pub fn check_buy(
    entry: Option<&GameShopEntry>,
    quantity: u8,
    pay_type: u8,
    gold: u32,
    credit: u32,
    live: bool,
) -> Result<BuyOrder, BuyProblem> {
    if !live {
        return Err(BuyProblem::Disconnected);
    }
    let entry = entry.ok_or(BuyProblem::NoSelection)?;
    if entry.sold_out {
        return Err(BuyProblem::SoldOut);
    }
    // 限量（`item.stock > 0`）且剩余 0：即使 `sold_out` 未及时置位的防御分支也禁购，
    // 不能把「首登已卖空」的限量商品当不限量放行（R50-C2）。
    if entry.item.stock > 0 && entry.stock_level <= 0 {
        return Err(BuyProblem::SoldOut);
    }
    let quantity = clamp_quantity(entry, quantity);
    if entry.item.stock > 0 && i32::from(quantity) > entry.stock_level {
        return Err(BuyProblem::OutOfStock);
    }
    match pay_type {
        PAY_CREDIT => {
            if !entry.item.can_buy_credit {
                return Err(BuyProblem::PayTypeUnavailable);
            }
            if total_price(entry, quantity, pay_type) > u64::from(credit) {
                return Err(BuyProblem::NotEnoughCredit);
            }
        }
        PAY_GOLD => {
            if !entry.item.can_buy_gold {
                return Err(BuyProblem::PayTypeUnavailable);
            }
            if total_price(entry, quantity, pay_type) > u64::from(gold) {
                return Err(BuyProblem::NotEnoughGold);
            }
        }
        _ => return Err(BuyProblem::PayTypeUnavailable),
    }
    Ok(BuyOrder {
        g_index: entry.item.g_index,
        quantity,
        p_type: i32::from(pay_type),
    })
}

/// 缺 `docs/assets/gameshop-manifest.json`（R50-D 在产）时返回 `true`：
/// 面板画占位图标方块 + `partial` 标记，**不伪造图标**。
#[must_use]
pub fn assets_partial(workspace_root: &Path) -> bool {
    !manifest_path(workspace_root).is_file()
}

/// 商城图标清单路径（R50-D 的交付物）。
#[must_use]
pub fn manifest_path(workspace_root: &Path) -> std::path::PathBuf {
    workspace_root
        .join("docs")
        .join("assets")
        .join("gameshop-manifest.json")
}

/// 工作区根：`crates/mir-client` 往上两级（dev 树 / `cargo test` 都从工作区运行）。
#[must_use]
pub fn workspace_root() -> Option<&'static Path> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
}

/// 缺图标清单时返回 `true`。
///
/// 发布包不带 `docs/`，所以按编译期 crate 路径回推工作区根；回推失败同样视为
/// 缺清单（占了方块 + `partial` 标记，**不伪造图标**）。
#[must_use]
pub fn assets_partial_default() -> bool {
    workspace_root().is_none_or(assets_partial)
}

/// 从 R50-D 的 `gameshop-manifest.json` 读 `GIndex -> icon_image`（`items.wzl` 帧号，
/// 等价 Crystal `Libraries.Items.Draw(Item.Info.Image, ...)` 的入参）。
///
/// 客户端不引入 `serde_json`（不在 R50-C 写范围），这里按 `"gindex"` 切块手写扫描：
/// 每块里先取 `gindex` 整数、再取 `"icon_image"` 整数；块里 `"partial": true` 的条目
/// 视为缺图标（跳过，回退占位）。解析失败返回空表，调用方回退包内 `info.image`。
#[must_use]
pub fn manifest_icon_images(path: &Path) -> Vec<(i32, u16)> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut icons = Vec::new();
    let mut parts = text.split("\"gindex\"");
    let _ = parts.next(); // 第一个 gindex 之前的头部
    for part in parts {
        let Some(g_index) = leading_int(part) else {
            continue;
        };
        if part.contains("\"partial\": true") || part.contains("\"partial\":true") {
            continue;
        }
        let Some(icon) = int_after(part, "\"icon_image\"") else {
            continue;
        };
        if let Ok(icon) = u16::try_from(icon) {
            icons.push((g_index, icon));
        }
    }
    icons
}

/// 进程内缓存清单图标（面板每帧调用，只读一次文件）。
fn cached_manifest_icons() -> &'static [(i32, u16)] {
    static CACHE: std::sync::OnceLock<Vec<(i32, u16)>> = std::sync::OnceLock::new();
    CACHE
        .get_or_init(|| {
            workspace_root()
                .map_or_else(Vec::new, |root| manifest_icon_images(&manifest_path(root)))
        })
        .as_slice()
}

/// 商品图标帧号：优先包内 `Info.Image`（Crystal 同源），缺省时查 R50-D 清单。
#[must_use]
pub fn icon_image_for(item: &mir_protocol::data::GameShopItem) -> u16 {
    if item.info.image != 0 {
        return item.info.image;
    }
    cached_manifest_icons()
        .iter()
        .find(|(g_index, _)| *g_index == item.g_index)
        .map_or(0, |(_, image)| *image)
}

/// 解析 `": 123 ..."` 形式的前导整数（`gindex` 值紧跟键）。
fn leading_int(text: &str) -> Option<i32> {
    let text = text.trim_start().strip_prefix(':')?.trim_start();
    let end = text
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(text.len());
    if end == 0 {
        return None;
    }
    text.get(..end)?.parse().ok()
}

/// 在文本里找 `key` 之后的整数（`"icon_image": 380`）。
fn int_after(text: &str, key: &str) -> Option<i32> {
    let start = text.find(key)? + key.len();
    let text = text
        .get(start..)?
        .trim_start()
        .strip_prefix(':')?
        .trim_start();
    let end = text
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(text.len());
    if end == 0 {
        return None;
    }
    text.get(..end)?.parse().ok()
}

/// 商城面板布局（绘制与命中测试共用）。
#[derive(Debug, Clone, PartialEq)]
pub struct GameShopLayout {
    /// 面板。
    pub panel: Rect,
    /// 关闭按钮。
    pub close: Rect,
    /// 职业筛选按钮（顺序见 [`ClassFilter::ALL_FILTERS`]）。
    pub class_buttons: Vec<Rect>,
    /// 分区筛选按钮（顺序见 [`SectionFilter::ALL_SECTIONS`]）。
    pub section_buttons: Vec<Rect>,
    /// 搜索框。
    pub search: Rect,
    /// 分类栏标题。
    pub category_header: Rect,
    /// 分类栏区域（不含标题；行高 [`CATEGORY_ROW`]）。
    pub category_area: Rect,
    /// 商品格（8 个，按页内顺序；`None` 的格子也占位）。
    pub cells: Vec<Rect>,
    /// 「元宝支付」复选框。
    pub pay_credit: Rect,
    /// 「金币支付」复选框。
    pub pay_gold: Rect,
    /// 数量减一。
    pub qty_minus: Rect,
    /// 数量显示框。
    pub qty_value: Rect,
    /// 数量加一。
    pub qty_plus: Rect,
    /// 买下按钮。
    pub buy: Rect,
    /// 上一页。
    pub prev: Rect,
    /// 页码显示区。
    pub page_label: Rect,
    /// 下一页。
    pub next: Rect,
    /// 底部提示行。
    pub footer: Rect,
}

impl GameShopLayout {
    /// 点中的商品格（页内下标 0..8）。
    #[must_use]
    pub fn cell_at(&self, point: Point) -> Option<usize> {
        self.cells.iter().position(|rect| rect.contains(point))
    }

    /// 点中的分类行：`0` = 全部分类，`1..` = `category_names()` 的下标。
    #[must_use]
    pub fn category_at(&self, point: Point) -> Option<usize> {
        if !self.category_area.contains(point) {
            return None;
        }
        let row = usize::try_from((point.y - self.category_area.y) / CATEGORY_ROW as i32).ok()?;
        if row == 0 {
            return Some(0);
        }
        Some(row)
    }

    /// 点中的职业筛选按钮。
    #[must_use]
    pub fn class_at(&self, point: Point) -> Option<ClassFilter> {
        self.class_buttons
            .iter()
            .position(|rect| rect.contains(point))
            .and_then(|index| ClassFilter::ALL_FILTERS.get(index).copied())
    }

    /// 点中的分区筛选按钮。
    #[must_use]
    pub fn section_at(&self, point: Point) -> Option<SectionFilter> {
        self.section_buttons
            .iter()
            .position(|rect| rect.contains(point))
            .and_then(|index| SectionFilter::ALL_SECTIONS.get(index).copied())
    }
}

/// 计算面板布局。`categories` 是 [`category_names`] 的长度（不含「全部分类」行）。
#[must_use]
pub fn gameshop_layout(frame: Rect, categories: usize) -> GameShopLayout {
    let width = PANEL_WIDTH
        .min(frame.width.saturating_sub(8))
        .max(MIN_PANEL_WIDTH.min(frame.width));
    let height = PANEL_HEIGHT
        .min(frame.height.saturating_sub(8))
        .max(MIN_PANEL_HEIGHT.min(frame.height));
    let panel = Rect::new(
        frame.x + i32::try_from(frame.width.saturating_sub(width) / 2).unwrap_or(0),
        frame.y + i32::try_from(frame.height.saturating_sub(height) / 2).unwrap_or(0),
        width,
        height,
    );
    let left = panel.x + i32::try_from(PADDING).unwrap_or(8);
    let right = panel.right() - i32::try_from(PADDING).unwrap_or(8);
    let inner_width = u32::try_from((right - left).max(1)).unwrap_or(1);
    let pad = i32::try_from(PADDING).unwrap_or(8);

    let mut y = panel.y + pad + i32::try_from(TITLE_HEIGHT).unwrap_or(24);
    let close = Rect::new(right - 56, panel.y + 4, 54, 18.min(panel.height));

    // 职业筛选行：6 个等宽按钮。
    let class_width = inner_width
        .saturating_sub(5 * 4)
        .checked_div(ClassFilter::ALL_FILTERS.len() as u32)
        .unwrap_or(1)
        .max(1);
    let class_buttons = ClassFilter::ALL_FILTERS
        .iter()
        .enumerate()
        .map(|(index, _)| {
            Rect::new(
                left + i32::try_from(index as u32 * (class_width + 4)).unwrap_or(0),
                y,
                class_width,
                BUTTON_HEIGHT,
            )
        })
        .collect();
    y += i32::try_from(ROW_HEIGHT + 4).unwrap_or(24);

    // 分区筛选行：4 个按钮 + 右侧搜索框。
    let section_width = 70
        .min(inner_width.saturating_sub(SEARCH_WIDTH + 10) / 4)
        .max(24);
    let section_buttons = SectionFilter::ALL_SECTIONS
        .iter()
        .enumerate()
        .map(|(index, _)| {
            Rect::new(
                left + i32::try_from(index as u32 * (section_width + 4)).unwrap_or(0),
                y,
                section_width,
                BUTTON_HEIGHT,
            )
        })
        .collect();
    let search = Rect::new(
        right - i32::try_from(SEARCH_WIDTH).unwrap_or(170),
        y,
        SEARCH_WIDTH.min(inner_width),
        BUTTON_HEIGHT,
    );
    y += i32::try_from(ROW_HEIGHT + 6).unwrap_or(26);
    let content_top = y;

    // 从底部往上排：提示行 → 翻页 → 支付/数量行 → 网格。
    let bottom = panel.bottom() - pad;
    let footer = Rect::new(
        left,
        bottom - i32::try_from(FOOTER_HEIGHT).unwrap_or(18),
        inner_width,
        FOOTER_HEIGHT,
    );
    let pager = Rect::new(
        left,
        footer.y - i32::try_from(PAGER_HEIGHT).unwrap_or(20) - 2,
        inner_width,
        PAGER_HEIGHT,
    );
    let pay_y = pager.y - i32::try_from(PAY_ROW_HEIGHT).unwrap_or(26) - 2;
    let content_bottom = pay_y - 6;
    let content_height = u32::try_from((content_bottom - content_top).max(1)).unwrap_or(1);

    let category_area = Rect::new(left, content_top, CATEGORY_WIDTH, content_height);
    let category_header = Rect::new(
        left,
        content_top.saturating_sub(i32::try_from(CATEGORY_HEADER).unwrap_or(18)),
        CATEGORY_WIDTH,
        CATEGORY_HEADER,
    );
    let _ = categories;
    let grid_x = left + i32::try_from(CATEGORY_WIDTH + 8).unwrap_or(140);
    let grid_width = u32::try_from((right - grid_x).max(1)).unwrap_or(1);
    let cell_width = grid_width
        .saturating_sub((GRID_COLUMNS as u32 - 1) * CELL_GAP)
        .checked_div(GRID_COLUMNS as u32)
        .unwrap_or(1)
        .max(1);
    let cell_height = content_height
        .saturating_sub(GRID_ROWS as u32 * CELL_GAP)
        .checked_div(GRID_ROWS as u32)
        .unwrap_or(24)
        .clamp(24, CELL_MAX_HEIGHT);
    let cells = (0..PAGE_SIZE)
        .map(|index| {
            let column = index % GRID_COLUMNS;
            let row = index / GRID_COLUMNS;
            Rect::new(
                grid_x + i32::try_from(column as u32 * (cell_width + CELL_GAP)).unwrap_or(0),
                content_top + i32::try_from(row as u32 * (cell_height + CELL_GAP)).unwrap_or(0),
                cell_width,
                cell_height,
            )
        })
        .collect();

    // 支付/数量/买下。
    let pay_height = 22.ui32();
    let credit = Rect::new(left, pay_y + 2, 96, pay_height);
    let gold = Rect::new(left + 100, pay_y + 2, 96, pay_height);
    let qty_minus = Rect::new(gold.right() + 16, pay_y + 2, 20, pay_height);
    let qty_value = Rect::new(qty_minus.right() + 2, pay_y + 2, 36, pay_height);
    let qty_plus = Rect::new(qty_value.right() + 2, pay_y + 2, 20, pay_height);
    let buy = Rect::new(right - 90, pay_y + 2, 88, pay_height);
    let prev = Rect::new(left, pager.y + 2, 46, PAGER_HEIGHT.saturating_sub(4).max(1));
    let next = Rect::new(
        right - 46,
        pager.y + 2,
        46,
        PAGER_HEIGHT.saturating_sub(4).max(1),
    );
    let page_label = Rect::new(
        prev.right() + 4,
        pager.y + 2,
        120,
        PAGER_HEIGHT.saturating_sub(4),
    );

    GameShopLayout {
        panel,
        close,
        class_buttons,
        section_buttons,
        search,
        category_header,
        category_area,
        cells,
        pay_credit: credit,
        pay_gold: gold,
        qty_minus,
        qty_value,
        qty_plus,
        buy,
        prev,
        page_label,
        next,
        footer,
    }
}

/// `u32` 字面量辅助（保持布局代码可读）。
trait Ui32Ext {
    /// 转 `u32`。
    fn ui32(self) -> u32;
}

impl Ui32Ext for u32 {
    fn ui32(self) -> u32 {
        self
    }
}

/// 一帧里要画的所有商城数据（绘制与点击共用同一份筛选结果）。
pub struct GameShopView<'a> {
    /// 当前筛选结果（[`visible_entries`] 的输出，已排序）。
    pub entries: &'a [&'a GameShopEntry],
    /// 全部分类名（[`category_names`] 的输出；不含「全部分类」行）。
    pub categories: &'a [String],
    /// 目录总条目数（区分「空目录」与「筛选无结果」）。
    pub catalog_len: usize,
    /// 当前页（0 起）。
    pub page: usize,
    /// 选中的 `GIndex`。
    pub selected: Option<i32>,
    /// 购买份数。
    pub quantity: u8,
    /// 结算货币（[`PAY_CREDIT`] / [`PAY_GOLD`]）。
    pub pay_type: u8,
    /// 搜索框。
    pub search: &'a TextField,
    /// 当前聚焦的输入框。
    pub focus: Option<crate::ui::FieldId>,
    /// 悬停的页内格下标。
    pub hover: Option<usize>,
    /// 当前职业筛选。
    pub class: ClassFilter,
    /// 当前分区筛选。
    pub section: SectionFilter,
    /// 当前分类筛选（`None` = 全部分类）。
    pub category: Option<&'a str>,
    /// 本地购买检查失败提示。
    pub notice: Option<BuyProblem>,
    /// 自己的金币。
    pub gold: u32,
    /// 自己的元宝。
    pub credit: u32,
    /// 是否已连接（断线时禁用购买并提示）。
    pub live: bool,
    /// 目录里是否有新品（无新品时「新品」按钮灰显）。
    pub has_new: bool,
    /// 缺图标清单（显示 `partial` 标记）。
    pub partial: bool,
    /// 当前 Unix 秒（新品徽标与 7 天窗口用；无头渲染可直接给固定值）。
    pub now_sec: i64,
}

/// 画商城面板。
pub fn draw(
    frame: &mut Frame,
    layout: &GameShopLayout,
    context: &PanelContext<'_>,
    view: &GameShopView<'_>,
    icon_fn: &mut dyn FnMut(u16) -> Option<Image>,
) {
    let language = context.language;
    let language_label = |label: GameShopLabel| label.text(language);
    crate::ui::draw_panel(frame, layout.panel);

    // 标题 + 关闭。
    context.text.draw(
        frame,
        layout.panel.x + 8,
        layout.panel.y + 5,
        language_label(GameShopLabel::Title),
        SIZE_BODY,
        TITLE_COLOR,
    );
    if view.partial {
        context.text.draw(
            frame,
            layout.panel.x + 8 + 60,
            layout.panel.y + 5,
            language_label(GameShopLabel::Partial),
            SIZE_SMALL,
            MUTED_COLOR,
        );
    }
    draw_button(
        frame,
        context.text,
        layout.close,
        language_label(GameShopLabel::Close),
        false,
        MUTED_COLOR,
    );

    // 职业筛选按钮。
    for (index, rect) in layout.class_buttons.iter().enumerate() {
        let Some(filter) = ClassFilter::ALL_FILTERS.get(index).copied() else {
            continue;
        };
        draw_button(
            frame,
            context.text,
            *rect,
            filter.label().text(language),
            view.class == filter,
            if view.class == filter {
                GOLD_COLOR
            } else {
                BODY_COLOR
            },
        );
    }

    // 分区筛选按钮（新品无数据时灰显）。
    for (index, rect) in layout.section_buttons.iter().enumerate() {
        let Some(section) = SectionFilter::ALL_SECTIONS.get(index).copied() else {
            continue;
        };
        let enabled = section != SectionFilter::New || view.has_new;
        let active = view.section == section;
        let color = if !enabled {
            DIMMED_COLOR
        } else if active {
            GOLD_COLOR
        } else {
            BODY_COLOR
        };
        draw_button(
            frame,
            context.text,
            *rect,
            section.label().text(language),
            active,
            color,
        );
    }

    // 搜索框。
    input::draw(
        frame,
        context.text,
        layout.search,
        view.search,
        view.focus == Some(crate::ui::FieldId::GameShopSearch),
        language_label(GameShopLabel::SearchHint),
    );

    // 分类栏。
    context.text.draw(
        frame,
        layout.category_header.x,
        layout.category_header.y,
        language_label(GameShopLabel::Category),
        SIZE_SMALL,
        MUTED_COLOR,
    );
    frame.fill_rect(
        layout.category_area.x,
        layout.category_area.y,
        layout.category_area.width,
        layout.category_area.height,
        CELL_COLOR,
    );
    let category_rows = usize::try_from(layout.category_area.height / CATEGORY_ROW).unwrap_or(1);
    for row in 0..category_rows.min(view.categories.len() + 1) {
        let rect = Rect::new(
            layout.category_area.x,
            layout.category_area.y + i32::try_from(row as u32 * CATEGORY_ROW).unwrap_or(0),
            layout.category_area.width,
            CATEGORY_ROW,
        );
        let (text, active) = if row == 0 {
            (
                language_label(GameShopLabel::AllCategories),
                view.category.is_none(),
            )
        } else {
            let Some(category) = view.categories.get(row - 1) else {
                break;
            };
            (
                category.as_str(),
                view.category.is_some_and(|wanted| wanted == category),
            )
        };
        if active {
            frame.fill_rect(
                rect.x,
                rect.y,
                rect.width,
                rect.height.saturating_sub(1),
                BUTTON_ACTIVE_COLOR,
            );
        }
        context.text.draw_clipped(
            frame,
            rect.x + 4,
            rect.y + 2,
            text,
            SIZE_SMALL,
            if active { GOLD_COLOR } else { BODY_COLOR },
            rect,
        );
    }

    // 商品网格。
    let page_entries = page_slice(view.entries, view.page);
    for (index, rect) in layout.cells.iter().enumerate() {
        let entry = page_entries.get(index).copied();
        draw_cell(frame, context, *rect, entry, view, index, &mut *icon_fn);
    }

    // 空态。
    if view.entries.is_empty() {
        let label = if !view.live {
            GameShopLabel::Disconnected
        } else if view.catalog_len == 0 {
            GameShopLabel::Empty
        } else {
            GameShopLabel::NoMatch
        };
        let grid_top = layout
            .cells
            .first()
            .map_or(layout.panel.y + 60, |cell| cell.y);
        context.text.draw(
            frame,
            layout
                .cells
                .first()
                .map_or(layout.panel.x + 160, |cell| cell.x + 8),
            grid_top + 12,
            label.text(language),
            SIZE_BODY,
            MUTED_COLOR,
        );
    }

    // 支付方式与数量。
    draw_check(
        frame,
        context.text,
        layout.pay_credit,
        view.pay_type == PAY_CREDIT,
        &format!(
            "{} {}",
            language_label(GameShopLabel::PayCredit),
            labels::format_number(u64::from(view.credit))
        ),
        GOLD_COLOR,
    );
    draw_check(
        frame,
        context.text,
        layout.pay_gold,
        view.pay_type == PAY_GOLD,
        &format!(
            "{} {}",
            language_label(GameShopLabel::PayGold),
            labels::format_number(u64::from(view.gold))
        ),
        GOLD_COLOR,
    );
    draw_button(
        frame,
        context.text,
        layout.qty_minus,
        "-",
        false,
        BODY_COLOR,
    );
    draw_button(
        frame,
        context.text,
        layout.qty_value,
        &view.quantity.to_string(),
        false,
        BODY_COLOR,
    );
    draw_button(frame, context.text, layout.qty_plus, "+", false, BODY_COLOR);
    let selected = view.selected.and_then(|g_index| {
        view.entries
            .iter()
            .find(|entry| entry.item.g_index == g_index)
            .copied()
    });
    let total = selected.map(|entry| total_price(entry, view.quantity, view.pay_type));
    let buy_label = match total {
        Some(total) => format!(
            "{} {}",
            language_label(GameShopLabel::Buy),
            labels::format_number(total)
        ),
        None => language_label(GameShopLabel::Buy).to_owned(),
    };
    let buy_enabled = selected.is_some_and(|entry| !entry.sold_out) && view.live;
    draw_button(
        frame,
        context.text,
        layout.buy,
        &buy_label,
        false,
        if buy_enabled {
            GOLD_COLOR
        } else {
            DIMMED_COLOR
        },
    );

    // 翻页。
    let pages = page_count(view.entries.len());
    draw_button(
        frame,
        context.text,
        layout.prev,
        language_label(GameShopLabel::Prev),
        false,
        BODY_COLOR,
    );
    draw_button(
        frame,
        context.text,
        layout.next,
        language_label(GameShopLabel::Next),
        false,
        BODY_COLOR,
    );
    context.text.draw_center(
        frame,
        layout.page_label,
        &format!(
            "{} {} / {}",
            language_label(GameShopLabel::Page),
            view.page + 1,
            pages
        ),
        SIZE_SMALL,
        MUTED_COLOR,
    );

    // 底部提示：本地检查失败优先，其次操作提示/断线。
    let footer_text = if let Some(problem) = view.notice {
        problem.label().text(language).to_owned()
    } else if !view.live {
        GameShopLabel::Disconnected.text(language).to_owned()
    } else {
        GameShopLabel::Hint.text(language).to_owned()
    };
    let footer_color = if view.notice.is_some() {
        FAIL_COLOR
    } else if view.live {
        MUTED_COLOR
    } else {
        FAIL_COLOR
    };
    context.text.draw_clipped(
        frame,
        layout.footer.x,
        layout.footer.y,
        &footer_text,
        SIZE_SMALL,
        footer_color,
        layout.footer,
    );
}

/// 画一个商品格（Crystal `MirGameShopCell.UpdateText`，`MirGameShopCell.cs:189-299`）。
#[allow(clippy::too_many_arguments)]
fn draw_cell(
    frame: &mut Frame,
    context: &PanelContext<'_>,
    rect: Rect,
    entry: Option<&GameShopEntry>,
    view: &GameShopView<'_>,
    index: usize,
    icon_fn: &mut dyn FnMut(u16) -> Option<Image>,
) {
    let language = context.language;
    let selected = entry.is_some_and(|entry| view.selected == Some(entry.item.g_index));
    let hover = view.hover == Some(index);
    let background = if selected {
        SELECTED_COLOR
    } else if hover {
        HOVER_COLOR
    } else {
        CELL_COLOR
    };
    frame.fill_rect(rect.x, rect.y, rect.width, rect.height, background);
    frame.fill_rect(
        rect.x,
        rect.y,
        rect.width,
        1,
        if selected {
            GOLD_COLOR
        } else {
            [70, 76, 90, 255]
        },
    );
    let Some(entry) = entry else {
        return;
    };
    let text = context.text;
    let inner = rect.inset(4);
    let dimmed = entry.sold_out;

    // 图标：有素材就画，缺素材画占位方块并标 `?`（不伪造图标）。
    let icon_rect = Rect::new(
        rect.x + 4,
        rect.y + 4,
        ICON_SIZE.min(rect.width.saturating_sub(8)),
        ICON_SIZE.min(rect.height.saturating_sub(8)),
    );
    let icon_image = icon_image_for(&entry.item);
    let icon = if icon_image == 0 {
        None
    } else {
        icon_fn(icon_image)
    };
    if let Some(image) = icon {
        let scaled = scale_to_fit(&image, icon_rect.width.max(1), icon_rect.height.max(1));
        frame.blit(&scaled, icon_rect.x, icon_rect.y);
    } else {
        frame.fill_rect(
            icon_rect.x,
            icon_rect.y,
            icon_rect.width,
            icon_rect.height,
            [40, 44, 52, 255],
        );
        text.draw_center(frame, icon_rect, "?", SIZE_BODY, DIMMED_COLOR);
    }

    // 物品名（品级色）。
    let name_color = if dimmed {
        DIMMED_COLOR
    } else {
        labels::grade_color(entry.item.info.grade)
    };
    let name_rect = Rect::new(
        icon_rect.right() + 4,
        rect.y + 4,
        u32::try_from(rect.right() - icon_rect.right() - 8).unwrap_or(1),
        16,
    );
    text.draw_clipped(
        frame,
        name_rect.x,
        name_rect.y,
        display_name(entry),
        SIZE_SMALL,
        name_color,
        name_rect,
    );

    // 价格行：元宝 / 金币（双币显示，`CanBuy*` 为假时灰显）。
    let mut price_y = rect.y + 24;
    let credit_color = if entry.item.can_buy_credit && !dimmed {
        GOLD_COLOR
    } else {
        DIMMED_COLOR
    };
    text.draw_clipped(
        frame,
        rect.x + 6,
        price_y,
        &format!(
            "{} {}",
            GameShopLabel::PayCredit.text(language),
            labels::format_number(u64::from(entry.item.credit_price))
        ),
        SIZE_SMALL,
        credit_color,
        Rect::new(rect.x + 6, price_y, rect.width.saturating_sub(12), 14),
    );
    price_y += 16;
    let gold_color = if entry.item.can_buy_gold && !dimmed {
        GOLD_COLOR
    } else {
        DIMMED_COLOR
    };
    text.draw_clipped(
        frame,
        rect.x + 6,
        price_y,
        &format!(
            "{} {}",
            GameShopLabel::PayGold.text(language),
            labels::format_number(u64::from(entry.item.gold_price))
        ),
        SIZE_SMALL,
        gold_color,
        Rect::new(rect.x + 6, price_y, rect.width.saturating_sub(12), 14),
    );

    // 库存 + 每次数量。
    price_y += 16;
    let stock_color = if dimmed { FAIL_COLOR } else { MUTED_COLOR };
    let stock = format!(
        "{} {}",
        GameShopLabel::Stock.text(language),
        stock_text(entry, language)
    );
    text.draw_clipped(
        frame,
        rect.x + 6,
        price_y,
        &stock,
        SIZE_SMALL,
        stock_color,
        Rect::new(rect.x + 6, price_y, rect.width.saturating_sub(12), 14),
    );
    if entry.item.count > 1 {
        text.draw_right(
            frame,
            rect.right() - 6,
            price_y,
            &format!("x{}", entry.item.count),
            SIZE_SMALL,
            MUTED_COLOR,
        );
    }

    // 分区徽标（新品/促销/置顶）。
    let mut badge_x = rect.right() - 6;
    let badges: [(&str, [u8; 4]); 3] = [
        (GameShopLabel::New.text(language), SUCCESS_COLOR),
        (GameShopLabel::Deal.text(language), [255, 140, 80, 255]),
        (GameShopLabel::Top.text(language), GOLD_COLOR),
    ];
    let flags = [
        entry.item.deal,
        entry.item.top_item,
        is_new(entry, view.now_sec),
    ];
    let _ = text;
    for (flag, (badge, color)) in flags.into_iter().zip(badges) {
        if !flag {
            continue;
        }
        let width = context.text.text_width(badge, SIZE_SMALL).max(12);
        let rect = Rect::new(
            badge_x - i32::try_from(width).unwrap_or(0),
            rect.y + 4,
            width,
            12,
        );
        frame.fill_rect(rect.x, rect.y, rect.width, rect.height, background);
        context
            .text
            .draw(frame, rect.x, rect.y, badge, SIZE_SMALL, color);
        badge_x = rect.x - 2;
    }

    // 售罄蒙层。
    if dimmed {
        let width = u32::try_from((rect.width as i32 - 8).max(1)).unwrap_or(1);
        let overlay_y = rect.y + i32::try_from(rect.height.saturating_sub(22)).unwrap_or(0);
        frame.fill_rect_alpha(rect.x + 4, overlay_y, width, 18, [0, 0, 0, 170]);
        context.text.draw_center(
            frame,
            Rect::new(rect.x + 4, overlay_y, width, 18),
            GameShopLabel::SoldOut.text(language),
            SIZE_BODY,
            FAIL_COLOR,
        );
    }
    let _ = inner;
}

/// 画一个复选按钮（支付方式）。
fn draw_check(
    frame: &mut Frame,
    text: &crate::ui::UiText,
    rect: Rect,
    checked: bool,
    label: &str,
    color: [u8; 4],
) {
    frame.fill_rect(rect.x, rect.y, rect.width, rect.height, BUTTON_COLOR);
    text.draw_clipped(
        frame,
        rect.x + 4,
        rect.y + 3,
        if checked { "◉" } else { "○" },
        SIZE_BODY,
        color,
        rect,
    );
    text.draw_clipped(
        frame,
        rect.x + 20,
        rect.y + 3,
        label,
        SIZE_SMALL,
        if checked { color } else { BODY_COLOR },
        Rect::new(
            rect.x + 20,
            rect.y,
            rect.width.saturating_sub(22),
            rect.height,
        ),
    );
}

/// 画一个按钮（底板 + 居中文字）。
fn draw_button(
    frame: &mut Frame,
    text: &crate::ui::UiText,
    rect: Rect,
    label: &str,
    active: bool,
    color: [u8; 4],
) {
    frame.fill_rect(
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        if active {
            BUTTON_ACTIVE_COLOR
        } else {
            BUTTON_COLOR
        },
    );
    text.draw_center(frame, rect, label, SIZE_SMALL, color);
}

/// 从快照的商城目录取某个 `GIndex` 的条目。
#[must_use]
pub fn selected_entry(gameshop: &GameShopState, selected: Option<i32>) -> Option<&GameShopEntry> {
    selected.and_then(|g_index| gameshop.entry(g_index))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use mir_core::DateTimeBinary;
    use mir_core::enums::ItemGrade;
    use mir_protocol::data::{GameShopItem, ItemInfo};

    use super::*;
    use crate::ui::{FieldId, UiState};

    fn entry(g_index: i32, name: &str, class: &str, category: &str) -> GameShopEntry {
        let info = ItemInfo {
            name: name.to_owned(),
            image: u16::try_from(g_index).unwrap_or(0),
            grade: ItemGrade::None,
            stack_size: 10,
            ..ItemInfo::default()
        };
        GameShopEntry {
            item: GameShopItem {
                item_index: g_index,
                g_index,
                info,
                gold_price: 100,
                credit_price: 10,
                count: 1,
                class: class.to_owned(),
                category: category.to_owned(),
                can_buy_credit: true,
                can_buy_gold: true,
                ..GameShopItem::default()
            },
            stock_level: 0,
            sold_out: false,
        }
    }

    fn catalogue() -> Vec<GameShopEntry> {
        let mut deal = entry(2, "炼狱药水", "All", "药品");
        deal.item.deal = true;
        deal.item.credit_price = 5;
        let mut top = entry(3, "屠龙", "Warrior", "武器");
        top.item.top_item = true;
        top.stock_level = 3;
        let mut fresh = entry(4, "鹿肉", "All", "药品");
        fresh.item.date = DateTimeBinary::from_unix_seconds(1_700_000_000);
        let mut old = entry(5, "金创药", "Taoist", "药品");
        old.item.date = DateTimeBinary::from_unix_seconds(100);
        vec![entry(1, "魔法盾", "Wizard", "技能"), deal, top, fresh, old]
    }

    #[test]
    fn labels_cover_all_variants_in_both_languages() {
        for label in GameShopLabel::ALL {
            let zh = label.text(Language::ZhCn);
            let en = label.text(Language::En);
            assert!(!zh.is_empty(), "{label:?} 缺中文");
            assert!(!en.is_empty(), "{label:?} 缺英文");
            assert_ne!(zh, en, "{label:?} 中英不应相同");
        }
        assert_eq!(
            GameShopLabel::Title.text(Language::ZhCn),
            "商城",
            "标题必须是中文（默认中文化）"
        );
        assert_eq!(
            GameShopLabel::SoldOut.text(Language::ZhCn),
            "售罄",
            "库存零可见必须显示「售罄」"
        );
        assert_eq!(GameShopLabel::SoldOut.text(Language::En), "Sold out");
        assert!(
            GameShopLabel::Partial
                .text(Language::En)
                .contains("partial"),
            "缺图标清单时要打 partial 标记（契约原文）"
        );
        assert_eq!(GameShopLabel::Buy.text(Language::ZhCn), "买下");
        assert_eq!(GameShopLabel::Close.text(Language::En), "Close");
    }

    #[test]
    fn class_filter_follows_crystal_names_and_matching() {
        assert_eq!(ClassFilter::All.crystal_name(), "Show All");
        assert_eq!(ClassFilter::Warrior.crystal_name(), "Warrior");
        assert_eq!(ClassFilter::Assassin.crystal_name(), "Assassin");
        assert_eq!(ClassFilter::Taoist.crystal_name(), "Taoist");
        assert_eq!(ClassFilter::Wizard.crystal_name(), "Wizard");
        assert_eq!(ClassFilter::Archer.crystal_name(), "Archer");
        // Crystal：Class == "All" 或 == 当前职业通过；Show All 全通过。
        assert!(ClassFilter::All.matches("Warrior"));
        assert!(ClassFilter::Warrior.matches("Warrior"));
        assert!(ClassFilter::Warrior.matches("All"));
        assert!(!ClassFilter::Warrior.matches("Wizard"));
        assert_eq!(
            ClassFilter::from_class(Some(MirClass::Taoist)),
            ClassFilter::Taoist
        );
        assert_eq!(ClassFilter::from_class(None), ClassFilter::All);
        assert_eq!(
            ClassFilter::from_class(Some(MirClass::Archer)),
            ClassFilter::Archer
        );
    }

    #[test]
    fn section_filter_matches_flags_and_7_day_new_window() {
        let now = 1_700_000_000;
        let mut deal = entry(1, "A", "All", "药品");
        deal.item.deal = true;
        let mut top = entry(2, "B", "All", "药品");
        top.item.top_item = true;
        let mut fresh = entry(3, "C", "All", "药品");
        fresh.item.date = DateTimeBinary::from_unix_seconds(now - 3 * 24 * 60 * 60);
        let mut stale = entry(4, "D", "All", "药品");
        stale.item.date = DateTimeBinary::from_unix_seconds(now - 8 * 24 * 60 * 60);
        let unset = entry(5, "E", "All", "药品");

        assert!(SectionFilter::All.matches(&deal, now));
        assert!(SectionFilter::Deal.matches(&deal, now));
        assert!(!SectionFilter::Deal.matches(&top, now));
        assert!(SectionFilter::Top.matches(&top, now));
        assert!(!SectionFilter::Top.matches(&deal, now));
        assert!(SectionFilter::New.matches(&fresh, now));
        assert!(
            !SectionFilter::New.matches(&stale, now),
            "8 天前的商品不算新品"
        );
        assert!(
            !SectionFilter::New.matches(&unset, now),
            "未设置日期不算新品（Crystal DateTime 0 早于窗口）"
        );
        assert!(has_new_items(&[fresh.clone()], now));
        assert!(!has_new_items(&[stale, unset], now));
    }

    #[test]
    fn visible_entries_filters_combine_and_sort_by_name() {
        let entries = catalogue();
        let now = 1_700_000_000 + 60;
        let all = visible_entries(
            &entries,
            ClassFilter::All,
            SectionFilter::All,
            None,
            "",
            now,
        );
        assert_eq!(all.len(), 5);
        // 按物品名排序：金创药 / 炼狱药水 / 鹿肉 / 魔法盾 / 屠龙（UTF-8 字节序）。
        let names: Vec<&str> = all.iter().map(|entry| display_name(entry)).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted, "列表必须按物品名排序");

        let warrior = visible_entries(
            &entries,
            ClassFilter::Warrior,
            SectionFilter::All,
            None,
            "",
            now,
        );
        assert_eq!(
            warrior.len(),
            3,
            "Warrior 筛选 = 职业 Warrior + Class=All 的商品（Crystal：Class==All 也通过）"
        );
        let deals = visible_entries(
            &entries,
            ClassFilter::All,
            SectionFilter::Deal,
            None,
            "",
            now,
        );
        assert_eq!(deals.len(), 1);
        assert_eq!(display_name(deals[0]), "炼狱药水");
        let meds = visible_entries(
            &entries,
            ClassFilter::All,
            SectionFilter::All,
            Some("药品"),
            "",
            now,
        );
        assert_eq!(meds.len(), 3, "分类筛选只留药品");
        let searched = visible_entries(
            &entries,
            ClassFilter::All,
            SectionFilter::All,
            None,
            " 药水 ",
            now,
        );
        assert_eq!(searched.len(), 1, "搜索按名字包含匹配并 trim");
        assert_eq!(display_name(searched[0]), "炼狱药水");
        let none = visible_entries(
            &entries,
            ClassFilter::All,
            SectionFilter::All,
            Some("不存在的分类"),
            "",
            now,
        );
        assert!(none.is_empty(), "组合筛选无结果时要能返回空（空态）");
    }

    #[test]
    fn category_names_dedupe_in_server_order_and_follow_filters() {
        let entries = catalogue();
        let now = 1_700_000_000;
        let categories = category_names(&entries, ClassFilter::All, SectionFilter::All, "", now);
        assert_eq!(
            categories,
            vec!["技能".to_owned(), "药品".to_owned(), "武器".to_owned()],
            "分类按商品顺序去重（Crystal GetCategories）"
        );
        let warrior = category_names(&entries, ClassFilter::Warrior, SectionFilter::All, "", now);
        assert_eq!(warrior, vec!["药品".to_owned(), "武器".to_owned()]);
        let searched = category_names(&entries, ClassFilter::All, SectionFilter::All, "药", now);
        assert_eq!(searched, vec!["药品".to_owned()]);
    }

    #[test]
    fn page_count_and_slice_cover_8_cells_per_page() {
        assert_eq!(page_count(0), 1, "空列表也要有一页，页码显示 1/1");
        assert_eq!(page_count(1), 1);
        assert_eq!(page_count(PAGE_SIZE), 1);
        assert_eq!(page_count(PAGE_SIZE + 1), 2);
        let entries = catalogue();
        let filtered = visible_entries(
            &entries,
            ClassFilter::All,
            SectionFilter::All,
            None,
            "",
            1_700_000_000,
        );
        assert_eq!(page_slice(&filtered, 0).len(), 5);
        assert!(
            page_slice(&filtered, 1).is_empty(),
            "越界页返回空而不是 panic"
        );
    }

    #[test]
    fn stock_display_uses_original_stock_and_ignores_i_stock() {
        // R50-C2（R50-T 审计 finding 1）：不限量只在 `item.stock == 0`；限量按剩余
        // `stock_level` 显示，剩余 0 显示「售罄」而不是 ∞。`i_stock` 只是个人/全服
        // 计数模式标记（`crates/mir-server/src/market.rs` 的 `purchased`），不参与
        // 不限量判定。
        let mut entry = entry(1, "A", "All", "药品");
        assert_eq!(stock_display(&entry), StockDisplay::Unlimited);
        // 不限量商品即使带着正的剩余档位也显示 ∞（剩余只对限量商品有意义）。
        entry.stock_level = 7;
        assert_eq!(stock_display(&entry), StockDisplay::Unlimited);
        // 限量：`item.stock > 0`，按剩余显示。
        entry.item.stock = 20;
        assert_eq!(stock_display(&entry), StockDisplay::Count(7));
        entry.stock_level = 99;
        assert_eq!(stock_display(&entry), StockDisplay::AtLeast99);
        entry.stock_level = 0;
        assert_eq!(
            stock_display(&entry),
            StockDisplay::SoldOut,
            "限量商品剩余 0 必须显示售罄（不能误判为不限量 ∞）"
        );
        entry.sold_out = true;
        assert_eq!(
            stock_display(&entry),
            StockDisplay::SoldOut,
            "售罄条目必须保留并显示售罄（R50 §1.3）"
        );
        assert_eq!(stock_text(&entry, Language::ZhCn), "售罄");
        // `i_stock == true` 但上架总量 > 0：仍是限量，显示剩余而不是 ∞。
        let mut individual = entry.clone();
        individual.sold_out = false;
        individual.item.i_stock = true;
        individual.stock_level = 3;
        assert_eq!(stock_display(&individual), StockDisplay::Count(3));
    }

    #[test]
    fn clamp_quantity_uses_stock_and_five_stack_limit() {
        // 不限量（`item.stock == 0`）只受 5 组堆叠夹取。
        let mut entry = entry(1, "A", "All", "药品");
        entry.item.count = 2;
        entry.item.info.stack_size = 10;
        // 5 组 × 10 / 每次 2 = 25 份上限。
        assert_eq!(clamp_quantity(&entry, 99), 25);
        assert_eq!(clamp_quantity(&entry, 0), 1);
        // 限量（`item.stock > 0`）再按剩余库存夹取。
        entry.item.stock = 8;
        entry.stock_level = 5;
        assert_eq!(
            clamp_quantity(&entry, 99),
            5,
            "限量库存夹取（Crystal Stock 判断）"
        );
        // 限量售罄：份数保留下限 1，购买由 `check_buy` 拦截（不发包）。
        entry.stock_level = 0;
        assert_eq!(
            clamp_quantity(&entry, 99),
            1,
            "限量剩余 0 时份数收边到下限 1"
        );
        // 不限量：剩余档位为 0 也不参与夹取。
        entry.item.count = 1;
        entry.item.info.stack_size = 1;
        entry.item.stock = 0;
        // 5 组 / 每次 1 = 5 份上限（不可堆叠也按 5 组夹）。
        assert_eq!(clamp_quantity(&entry, 99), 5);
    }

    #[test]
    fn check_buy_matrix_matches_crystal_buy_product() {
        // 限量售罄（`item.stock > 0` 且剩余 0，未带 `sold_out` 标记的防御分支）：
        // 必须本地拦截，不能放行成「不限量」。
        let mut exhausted = entry(9, "B", "All", "药品");
        exhausted.item.stock = 3;
        exhausted.stock_level = 0;
        exhausted.item.count = 1;
        exhausted.item.info.stack_size = 100;
        assert_eq!(
            check_buy(Some(&exhausted), 1, PAY_GOLD, 1_000, 1_000, true),
            Err(BuyProblem::SoldOut),
            "限量商品剩余 0 视为售罄、不得放行"
        );
        // 不限量商品（`item.stock == 0`）不受剩余档位限制。
        let mut unlimited_zero = entry(10, "C", "All", "药品");
        unlimited_zero.item.stock = 0;
        unlimited_zero.stock_level = 0;
        let order =
            check_buy(Some(&unlimited_zero), 3, PAY_GOLD, 1_000, 1_000, true).expect("不限量可买");
        assert_eq!(order.quantity, 3);
        let mut entry = entry(7, "A", "All", "药品");
        entry.item.gold_price = 100;
        entry.item.credit_price = 10;
        // 断线：不发包。
        assert_eq!(
            check_buy(Some(&entry), 1, PAY_GOLD, 100, 100, false),
            Err(BuyProblem::Disconnected)
        );
        // 未选中。
        assert_eq!(
            check_buy(None, 1, PAY_GOLD, 100, 100, true),
            Err(BuyProblem::NoSelection)
        );
        // 售罄。
        entry.sold_out = true;
        assert_eq!(
            check_buy(Some(&entry), 1, PAY_GOLD, 100, 100, true),
            Err(BuyProblem::SoldOut)
        );
        entry.sold_out = false;
        // 货币不支持。
        entry.item.can_buy_gold = false;
        assert_eq!(
            check_buy(Some(&entry), 1, PAY_GOLD, 100, 100, true),
            Err(BuyProblem::PayTypeUnavailable)
        );
        entry.item.can_buy_gold = true;
        // 余额不足。
        assert_eq!(
            check_buy(Some(&entry), 2, PAY_GOLD, 199, 1000, true),
            Err(BuyProblem::NotEnoughGold)
        );
        assert_eq!(
            check_buy(Some(&entry), 1, PAY_CREDIT, 1000, 9, true),
            Err(BuyProblem::NotEnoughCredit)
        );
        // 通过：份数夹取 + 总价。
        entry.item.info.stack_size = 10;
        entry.item.count = 1;
        let order = check_buy(Some(&entry), 99, PAY_GOLD, 100_000, 1000, true).expect("可买");
        assert_eq!(
            order,
            BuyOrder {
                g_index: 7,
                quantity: 50,
                p_type: 1
            },
            "份数上限 = 5 × StackSize / Count = 50（MirGameShopCell.cs:151）"
        );
        assert_eq!(total_price(&entry, 50, PAY_GOLD), 5000);
        let credit = check_buy(Some(&entry), 2, PAY_CREDIT, 0, 100, true).expect("元宝可买");
        assert_eq!(credit.p_type, 0);
        assert_eq!(total_price(&entry, 2, PAY_CREDIT), 20);
        // 未知货币。
        assert_eq!(
            check_buy(Some(&entry), 1, 9, 100, 100, true),
            Err(BuyProblem::PayTypeUnavailable)
        );
    }

    #[test]
    fn layout_hit_tests_are_consistent_with_drawn_rects() {
        let frame = Rect::screen(800, 600);
        let layout = gameshop_layout(frame, 3);
        assert_eq!(layout.cells.len(), PAGE_SIZE, "Crystal 8 格网格");
        for (index, cell) in layout.cells.iter().enumerate() {
            assert!(
                layout.panel.contains(cell.center()),
                "格子 {index} 应在面板内"
            );
            assert_eq!(layout.cell_at(cell.center()), Some(index));
        }
        // 点空白处不误判。
        assert_eq!(layout.cell_at(Point::new(0, 0)), None);
        for (index, button) in layout.class_buttons.iter().enumerate() {
            assert_eq!(
                layout.class_at(button.center()),
                Some(ClassFilter::ALL_FILTERS[index])
            );
        }
        for (index, button) in layout.section_buttons.iter().enumerate() {
            assert_eq!(
                layout.section_at(button.center()),
                Some(SectionFilter::ALL_SECTIONS[index])
            );
        }
        // 分类行 0 = 全部分类，1.. = 分类。
        assert_eq!(
            layout.category_at(Point::new(
                layout.category_area.x + 4,
                layout.category_area.y + 2
            )),
            Some(0)
        );
        assert_eq!(
            layout.category_at(Point::new(
                layout.category_area.x + 4,
                layout.category_area.y + CATEGORY_ROW as i32 * 2 + 2
            )),
            Some(2)
        );
        assert!(layout.close.contains(layout.close.center()));
        assert!(layout.buy.contains(layout.buy.center()));
    }

    #[test]
    fn layout_survives_tiny_frames() {
        for (width, height) in [(320, 240), (300, 180), (1280, 720)] {
            let frame = Rect::screen(width, height);
            let layout = gameshop_layout(frame, 1);
            assert!(layout.panel.x >= frame.x, "{width}x{height} 面板出左界");
            assert!(layout.panel.y >= frame.y, "{width}x{height} 面板出上界");
            assert!(
                layout.panel.right() <= frame.right(),
                "{width}x{height} 面板出右界"
            );
            assert!(
                layout.panel.bottom() <= frame.bottom(),
                "{width}x{height} 面板出下界"
            );
            for cell in &layout.cells {
                assert!(cell.width > 0 && cell.height > 0, "格子尺寸不能为 0");
            }
        }
    }

    #[test]
    fn hotkey_toggle_is_guarded_by_input_focus_and_resets_on_open() {
        let mut state = UiState::new(Language::ZhCn);
        assert!(!state.show_gameshop);
        assert!(
            state.toggle_gameshop(Some(MirClass::Taoist)),
            "live 态、无焦点时 K 应打开面板"
        );
        assert!(state.show_gameshop);
        assert_eq!(
            state.gameshop_class,
            ClassFilter::Taoist,
            "Crystal Show() 按玩家职业初始化 ClassFilter"
        );
        assert_eq!(state.gameshop_pay_type, PAY_GOLD, "Crystal 默认金币支付");
        assert_eq!(state.gameshop_quantity, 1);
        assert_eq!(state.gameshop_page, 0);
        // 面板内改了状态后再开要复位。
        state.gameshop_pay_type = PAY_CREDIT;
        state.gameshop_search.set("屠龙");
        state.toggle_gameshop(Some(MirClass::Taoist));
        assert!(!state.show_gameshop, "再按一次 K 关闭");
        state.toggle_gameshop(Some(MirClass::Taoist));
        assert_eq!(state.gameshop_pay_type, PAY_GOLD, "重开恢复默认支付方式");
        assert!(state.gameshop_search.value.is_empty(), "重开清空搜索");
        // 输入框聚焦时热键不得触发（app.rs 文本早退之外的二道防线）。
        state.focus = Some(FieldId::GameShopSearch);
        assert!(
            !state.toggle_gameshop(Some(MirClass::Taoist)),
            "焦点在输入框时 K 不得开关面板"
        );
        assert!(state.show_gameshop, "焦点守卫不能误关面板");
        state.focus = None;
        assert!(state.toggle_gameshop(None), "无角色信息时也能开关");
        assert!(!state.show_gameshop);
    }

    #[test]
    fn quantity_and_pay_type_reset_on_new_catalogue_selection() {
        let mut state = UiState::new(Language::ZhCn);
        state.toggle_gameshop(Some(MirClass::Warrior));
        state.gameshop_quantity = 12;
        state.gameshop_selected = Some(3);
        state.select_gameshop_entry(Some(4));
        assert_eq!(state.gameshop_quantity, 1, "切换商品恢复份数 1");
        assert_eq!(state.gameshop_notice, None);
        state.select_gameshop_entry(None);
        assert_eq!(state.gameshop_selected, None);
    }

    #[test]
    fn manifest_partial_flag_tracks_d_file() {
        // 用工作区 target/round50/r50-c 下的临时目录（`std::env::temp_dir` 在本机不可写）。
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("工作区根");
        let root = workspace.join(format!(
            "target/round50/r50-c/manifest-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("建临时目录");
        assert!(
            assets_partial(&root),
            "没有 docs/assets/gameshop-manifest.json 时必须是 partial"
        );
        fs::create_dir_all(root.join("docs/assets")).expect("建目录");
        fs::write(manifest_path(&root), "{}").expect("写清单");
        assert!(!assets_partial(&root), "清单存在后不再标 partial");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn selected_entry_lookup_is_by_gindex() {
        let entries = catalogue();
        let mut state = GameShopState::default();
        for entry in &entries {
            state.upsert(&entry.item, entry.stock_level);
        }
        let found = selected_entry(&state, Some(3)).expect("GIndex 3");
        assert_eq!(display_name(found), "屠龙");
        assert!(selected_entry(&state, Some(999)).is_none());
        assert!(selected_entry(&state, None).is_none());
    }
}
