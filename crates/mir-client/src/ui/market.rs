//! 市场 / 寄售 / 拍卖 / 商城面板（Crystal `TrustMerchantDialog`，
//! `Client/MirScenes/Dialogs/TrustMerchantDialog.cs`）。
//!
//! 四个页签对应协议里的 `MarketPanelType`：
//!
//! | 页签 | `C.MarketSearch.UserMode` | 列表内容 | 主按钮 |
//! |---|---|---|---|
//! | 市场 | `false` | 全部在售条目 | 寄售「买下」/ 拍卖「出价」（`C.MarketBuy`） |
//! | 寄售 | `true` | 自己挂的寄售 | 「取回」（`C.MarketGetBack{Any}`） |
//! | 拍卖 | `true` | 自己挂的拍卖 | 「取回」；选中已有人出价的条目可「提前成交」（`C.MarketSellNow`） |
//! | 商城 | `false` | 服务端商城目录（`S.NPCMarket`，`item_type = GameShop`） | 「购买」（`C.GameshopBuy`，元宝结算） |
//!
//! 布局参考 Crystal：顶部页签（`MarketButton` / `ConsignmentButton` / `AuctionButton`
//! / `GameShopButton`，`TrustMerchantDialog.cs:96-155`）、中间 10 行列表
//! （`AuctionRow`，`:593-605`）、底部搜索/刷新/翻页/买/卖/收款按钮
//! （`:182-232`、`:309-482`）。寄售页的「拖入物品 + 填价 + 上架」
//! （`:544-587`）简化成「在背包里选中物品 → 填价 → 上架」：市场窗口打开时
//! 背包也一起显示（Crystal `Show()` 同样把背包移到窗口旁边，`:1435-1436`）。
//!
//! **与 Crystal 的差异**：Crystal 的商城按钮默认隐藏（`Show()` 里
//! `GameShopButton.Visible = false`，`TrustMerchantDialog.cs:1426-1432`，只有
//! `Show(gameshop: true)` 才显示，而那个入口没被调用）；本作把它做成常显的第四个
//! 页签（需求「商城页签」）。商城页签仅支持**元宝**结算（`p_type = 0`），金币支付
//! 留给独立商城窗口（`GameshopDialog`）。列表**不显示实时库存**：售罄条目仍会列出，
//! 购买时由服务端回「库存不足」；「库存」列显示「在售」只是占位（与户部契约一致，
//! 见 `docs/world/gameshop.md` §7.3）。
//!
//! 与既有的 [`crate::ui::shop`] / [`crate::ui::trade`] 一样，布局函数与命中测试共用
//! 同一份矩形，所以无头测试可以在不开窗口的情况下点中每一个按钮。

use mir_core::enums::{MarketItemType, MarketPanelType};
use mir_core::{DateTimeBinary, Language, Point};
use mir_render::{Frame, Image, scale_to_fit};

use crate::geom::Rect;
use crate::net::{InventoryItem, MARKET_PAGE_SIZE, MarketListing};
use crate::ui::PanelContext;
use crate::ui::hud::{BODY_COLOR, GOLD_COLOR, MUTED_COLOR, SIZE_BODY, SIZE_SMALL, TITLE_COLOR};
use crate::ui::input::{self, TextField};
use crate::ui::labels::{self, Label};

/// 面板宽度。
pub const PANEL_WIDTH: u32 = 520;
/// 面板高度。
pub const PANEL_HEIGHT: u32 = 470;
/// 小窗口下的最小宽度（再窄就按屏幕宽度收边）。
pub const MIN_PANEL_WIDTH: u32 = 280;
/// 小窗口下的最小高度。
pub const MIN_PANEL_HEIGHT: u32 = 160;
/// 内边距。
pub const PADDING: u32 = 8;
/// 标题高度。
pub const TITLE_HEIGHT: u32 = 24;
/// 页签高度。
pub const TAB_HEIGHT: u32 = 22;
/// 工具栏（搜索 / 寄售表单）高度。
pub const TOOLBAR_HEIGHT: u32 = 24;
/// 列表表头高度。
pub const HEADER_HEIGHT: u32 = 18;
/// 每行高度。
pub const ROW_HEIGHT: u32 = 24;
/// 一页最多显示多少行（Crystal 固定 10 条一页，`PlayerObject.cs:8356`）。
pub const MAX_ROWS: usize = MARKET_PAGE_SIZE;
/// 按钮高度。
pub const BUTTON_HEIGHT: u32 = 22;
/// 翻页行高度。
pub const PAGER_HEIGHT: u32 = 20;
/// 底部提示行高度。
pub const FOOTER_HEIGHT: u32 = 18;
/// 行内图标最大边长。
pub const ROW_ICON_SIZE: u32 = 18;
/// 操作失败的红字颜色。
pub const FAIL_COLOR: [u8; 4] = [255, 132, 132, 255];
/// 操作成功的绿字颜色。
pub const SUCCESS_COLOR: [u8; 4] = [150, 226, 150, 255];
/// 高亮行的背景/左侧竖条。
pub const ROW_SELECTED_COLOR: [u8; 4] = [86, 74, 32, 235];
pub const ROW_HOVER_COLOR: [u8; 4] = [44, 56, 76, 235];
pub const ROW_STRIPE_COLOR: [u8; 4] = [26, 30, 38, 160];
/// 选中行左侧竖条颜色。
pub const ROW_MARK_COLOR: [u8; 4] = [255, 216, 96, 255];
/// 寄售有效期：`Globals.ConsignmentLength = 7` 天（`Shared/Globals.cs:42`）。
pub const CONSIGNMENT_LENGTH_SECONDS: i64 = 7 * 24 * 60 * 60;

/// 打开市场列表的 NPC 脚本页键：`NPCScript.MarketKey = "[@MARKET]"`（`NPCScript.cs:57`）。
pub const MARKET_KEY: &str = "[@MARKET]";

/// 页签顺序（Crystal 的 Market / Consign / Auction 三个按钮 + 手动打开的商城页签）。
///
/// 商城（`MarketPanelType::GameShop`）是市场面板里的第四个页签：Crystal 里它由
/// `GameShopButton` 打开（`TrustMerchantDialog.cs:143-155`），列表走
/// `S.NPCMarket`（`market_type = GameShop`），价格是元宝。
pub const TABS: [MarketPanelType; 4] = [
    MarketPanelType::Market,
    MarketPanelType::Consign,
    MarketPanelType::Auction,
    MarketPanelType::GameShop,
];

/// 页签文案。
#[must_use]
pub const fn tab_label(panel: MarketPanelType) -> Label {
    match panel {
        MarketPanelType::Consign => Label::Consign,
        MarketPanelType::Auction => Label::Auction,
        MarketPanelType::GameShop => Label::GameShop,
        MarketPanelType::Market => Label::Market,
    }
}

/// 该页签是否显示「自己挂单」列表（Crystal 打开寄售/拍卖页时发 `Usermode = true`，
/// `TrustMerchantDialog.cs:1210-1216`、`:1260-1266`）。
#[must_use]
pub const fn is_user_mode(panel: MarketPanelType) -> bool {
    !matches!(panel, MarketPanelType::Market | MarketPanelType::GameShop)
}

/// 该页签是否显示「上架表单」（选中背包物品 + 价格 + 上架）。
#[must_use]
pub const fn has_sell_form(panel: MarketPanelType) -> bool {
    matches!(panel, MarketPanelType::Consign | MarketPanelType::Auction)
}

/// 市场面板布局（绘制与命中测试共用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketLayout {
    /// 面板矩形。
    pub panel: Rect,
    /// 四个页签（顺序同 [`TABS`]）。
    pub tabs: Vec<Rect>,
    /// 关闭按钮（标题栏右侧）。
    pub close: Rect,
    /// 标题栏里的金币显示区域。
    pub gold: Rect,
    /// 工具栏：市场页是搜索框，寄售/拍卖页是「上架物品」显示区。
    pub toolbar: Rect,
    /// 搜索输入框（市场页）。
    pub search_field: Rect,
    /// 「搜索」按钮。
    pub find: Rect,
    /// 「刷新」按钮。
    pub refresh: Rect,
    /// 寄售/拍卖页价格输入框。
    pub price_field: Rect,
    /// 「上架」按钮。
    pub sell: Rect,
    /// 寄售/拍卖页「上架物品」显示区。
    pub consign_item: Rect,
    /// 列表表头。
    pub header: Rect,
    /// 表头四列。
    pub item_header: Rect,
    pub price_header: Rect,
    pub seller_header: Rect,
    pub expiry_header: Rect,
    /// 当前可见的列表行。
    pub rows: Vec<Rect>,
    /// 第一行的**列表绝对下标**（`page × 10 + scroll`）。
    pub first_row: usize,
    /// 翻页行。
    pub pager: Rect,
    /// 上一页按钮。
    pub prev: Rect,
    /// 页码显示。
    pub page_label: Rect,
    /// 下一页按钮。
    pub next: Rect,
    /// 操作行（市场页：出价输入 + 买下/出价；其它页：取回/提前成交/一键收款）。
    pub action: Rect,
    /// 市场页的出价输入框。
    pub bid_field: Rect,
    /// 主按钮：市场页「买下/出价」，寄售/拍卖页「取回」。
    pub primary: Rect,
    /// 「提前成交」（只在拍卖页有效）。
    pub sell_now: Rect,
    /// 「一键收款」（只在寄售/拍卖页有效）。
    pub collect: Rect,
    /// 底部提示/失败信息。
    pub footer: Rect,
}

impl MarketLayout {
    /// 命中页签（返回页签类型）。
    #[must_use]
    pub fn tab_at(&self, point: Point) -> Option<MarketPanelType> {
        self.tabs
            .iter()
            .position(|rect| rect.contains(point))
            .map(|index| TABS[index])
    }

    /// 命中列表行，返回**列表绝对下标**（已含当前页与滚动偏移）。
    #[must_use]
    pub fn row_at(&self, point: Point) -> Option<usize> {
        self.rows
            .iter()
            .position(|rect| rect.contains(point))
            .map(|index| self.first_row + index)
    }
}

/// 计算市场面板布局。
///
/// `listing_count` 是**已拿到的全部条目数**（翻页会追加），`page` 是当前页（0 起），
/// `scroll` 是页内滚动行；`row_at` 返回的绝对下标可以直接查 `listings`。
#[must_use]
pub fn market_layout(
    frame: Rect,
    panel: MarketPanelType,
    listing_count: usize,
    page: i32,
    scroll: usize,
) -> MarketLayout {
    let available_width = frame.width.saturating_sub(8);
    let width = PANEL_WIDTH
        .min(available_width)
        .max(MIN_PANEL_WIDTH.min(available_width));
    let available_height = frame.height.saturating_sub(8);
    let height = PANEL_HEIGHT
        .min(available_height)
        .max(MIN_PANEL_HEIGHT.min(available_height));
    let panel_rect = Rect::new(frame.x + 4, frame.y + 4, width, height);
    let content_x = panel_rect.x + i32::try_from(PADDING).unwrap_or(0);
    let content_width = width.saturating_sub(PADDING * 2);
    let content_right = content_x + i32::try_from(content_width).unwrap_or(0);

    let title_y = panel_rect.y + 4;
    let close = Rect::new(content_right - 18, title_y, 18, 18);
    let gold = Rect::new(
        content_x,
        title_y,
        content_width.saturating_sub(70),
        TITLE_HEIGHT,
    );

    let tabs_y = title_y + i32::try_from(TITLE_HEIGHT).unwrap_or(24);
    let tab_width =
        (content_width.saturating_sub(4 * (TABS.len() as u32 - 1)) / TABS.len() as u32).max(40);
    let tabs = (0..TABS.len())
        .map(|index| {
            Rect::new(
                content_x
                    + i32::try_from(index).unwrap_or(0)
                        * (i32::try_from(tab_width).unwrap_or(100) + 4),
                tabs_y,
                tab_width,
                TAB_HEIGHT,
            )
        })
        .collect();

    let toolbar_y = tabs_y + i32::try_from(TAB_HEIGHT + 4).unwrap_or(0);
    let toolbar = Rect::new(content_x, toolbar_y, content_width, TOOLBAR_HEIGHT);
    let find_width = 48;
    let refresh_width = 56;
    let search_width = content_width
        .saturating_sub(find_width + refresh_width + 8)
        .max(60);
    let search_field = Rect::new(content_x, toolbar_y, search_width, TOOLBAR_HEIGHT);
    let find = Rect::new(
        search_field.right() + 4,
        toolbar_y,
        find_width,
        TOOLBAR_HEIGHT,
    );
    let refresh = Rect::new(find.right() + 4, toolbar_y, refresh_width, TOOLBAR_HEIGHT);
    let sell_width = 56;
    let price_width = 82;
    let sell = Rect::new(
        content_right - i32::try_from(sell_width).unwrap_or(0),
        toolbar_y,
        sell_width,
        TOOLBAR_HEIGHT,
    );
    let price_field = Rect::new(
        sell.x - 4 - i32::try_from(price_width).unwrap_or(0),
        toolbar_y,
        price_width,
        TOOLBAR_HEIGHT,
    );
    let consign_item = Rect::new(
        content_x,
        toolbar_y,
        u32::try_from((price_field.x - 4 - content_x).max(40)).unwrap_or(40),
        TOOLBAR_HEIGHT,
    );

    let header_y = toolbar_y + i32::try_from(TOOLBAR_HEIGHT + 4).unwrap_or(0);
    let header = Rect::new(content_x, header_y, content_width, HEADER_HEIGHT);
    let columns = column_widths(content_width);
    let item_header = Rect::new(content_x, header_y, columns[0], HEADER_HEIGHT);
    let price_header = Rect::new(item_header.right(), header_y, columns[1], HEADER_HEIGHT);
    let seller_header = Rect::new(price_header.right(), header_y, columns[2], HEADER_HEIGHT);
    let expiry_header = Rect::new(seller_header.right(), header_y, columns[3], HEADER_HEIGHT);

    let footer = Rect::new(
        content_x,
        panel_rect.bottom() - i32::try_from(PADDING + FOOTER_HEIGHT).unwrap_or(0),
        content_width,
        FOOTER_HEIGHT,
    );
    let pager = Rect::new(
        content_x,
        footer.y - 4 - i32::try_from(PAGER_HEIGHT).unwrap_or(0),
        content_width,
        PAGER_HEIGHT,
    );
    let action = Rect::new(
        content_x,
        pager.y - 4 - i32::try_from(BUTTON_HEIGHT).unwrap_or(0),
        content_width,
        BUTTON_HEIGHT,
    );
    let prev = Rect::new(content_x, pager.y, 30, PAGER_HEIGHT);
    let page_label = Rect::new(prev.right() + 4, pager.y, 64, PAGER_HEIGHT);
    let next = Rect::new(page_label.right() + 4, pager.y, 30, PAGER_HEIGHT);

    let primary_width = 76;
    let primary = Rect::new(content_x, action.y, primary_width, BUTTON_HEIGHT);
    let sell_now = Rect::new(primary.right() + 6, action.y, 80, BUTTON_HEIGHT);
    let collect = Rect::new(
        if panel == MarketPanelType::Auction {
            sell_now.right() + 6
        } else {
            primary.right() + 6
        },
        action.y,
        84,
        BUTTON_HEIGHT,
    );
    let bid_width = 90;
    let bid_field = Rect::new(
        content_right - i32::try_from(bid_width).unwrap_or(0),
        action.y,
        bid_width,
        BUTTON_HEIGHT,
    );

    // 列表区域：表头下沿 → 操作行上沿。
    let rows_top = header.bottom() + 2;
    let rows_bottom = action.y - 4;
    let rows_area = rows_bottom.saturating_sub(rows_top);
    let capacity =
        usize::try_from(rows_area.max(0) / i32::try_from(ROW_HEIGHT).unwrap_or(24)).unwrap_or(0);

    let page_start = usize::try_from(page.max(0))
        .unwrap_or(0)
        .saturating_mul(MARKET_PAGE_SIZE);
    let on_page = listing_count
        .saturating_sub(page_start)
        .min(MARKET_PAGE_SIZE);
    let first_row_in_page = scroll.min(on_page.saturating_sub(1));
    let visible = on_page.saturating_sub(first_row_in_page).min(capacity);
    let rows = (0..visible)
        .map(|index| {
            Rect::new(
                content_x,
                rows_top
                    + i32::try_from(index).unwrap_or(0) * i32::try_from(ROW_HEIGHT).unwrap_or(0),
                content_width,
                ROW_HEIGHT - 1,
            )
        })
        .collect();

    MarketLayout {
        panel: panel_rect,
        tabs,
        close,
        gold,
        toolbar,
        search_field,
        find,
        refresh,
        price_field,
        sell,
        consign_item,
        header,
        item_header,
        price_header,
        seller_header,
        expiry_header,
        rows,
        first_row: page_start + first_row_in_page,
        pager,
        prev,
        page_label,
        next,
        action,
        bid_field,
        primary,
        sell_now,
        collect,
        footer,
    }
}

/// 四列宽度：物品 / 价格 / 卖家 / 到期。
fn column_widths(width: u32) -> [u32; 4] {
    let item = width * 42 / 100;
    let price = width * 20 / 100;
    let seller = width * 21 / 100;
    let expiry = width.saturating_sub(item + price + seller);
    [item, price, seller, expiry]
}

/// 主按钮文案：市场页按条目类型显示「买下」/「出价」，寄售/拍卖页显示「取回」。
#[must_use]
pub fn primary_label(panel: MarketPanelType, listing: Option<&MarketListing>) -> Label {
    if is_user_mode(panel) {
        return Label::GetBack;
    }
    // 商城页签固定「购买」（元宝结算），与是否选中条目无关。
    if panel == MarketPanelType::GameShop {
        return Label::Purchase;
    }
    match listing.map(|listing| listing.item_type) {
        Some(MarketItemType::Auction) => Label::Bid,
        Some(MarketItemType::GameShop) => Label::Purchase,
        _ => Label::Buy,
    }
}

/// 价格文案：寄售 `1,200 金币`，拍卖 `1,200 出价`，商城 `250 元宝`
/// （Crystal `AuctionRow.Update`，`TrustMerchantDialog.cs:1569`；
/// 商城页签用元宝结算是 `TrustMerchantDialog.cs:408-416` 的 `Credits` 分支）。
#[must_use]
pub fn price_text(listing: &MarketListing, language: Language) -> String {
    let unit = if listing.item_type == MarketItemType::GameShop {
        Label::Credit
    } else if listing.is_bid() {
        Label::Bid
    } else {
        Label::Gold
    };
    format!(
        "{} {}",
        labels::format_number(u64::from(listing.price)),
        unit.text(language)
    )
}

/// 到期时间文案（`MM-DD HH:MM`；Crystal 显示 `ConsignmentDate + 7 天`，
/// `TrustMerchantDialog.cs:1547`）。
#[must_use]
pub fn expiry_text(date: DateTimeBinary) -> String {
    if date.is_unset() {
        return "--".to_owned();
    }
    let seconds = date
        .to_unix_seconds()
        .saturating_add(CONSIGNMENT_LENGTH_SECONDS)
        .max(0);
    let days = seconds / 86_400;
    let minutes_of_day = (seconds % 86_400) / 60;
    let (_, month, day) = crate::ui::mail::civil_from_days(days);
    format!(
        "{month:02}-{day:02} {:02}:{:02}",
        minutes_of_day / 60,
        minutes_of_day % 60
    )
}

/// `S.MarketFail.Reason` → 本地化文案（Crystal `GameScene.MarketFail`，`GameScene.cs:5669-5707`）。
#[must_use]
pub const fn fail_label(reason: u8) -> Label {
    match reason {
        0 => Label::MarketFailDead,
        1 => Label::MarketFailNoDialog,
        2 => Label::MarketFailSold,
        3 => Label::MarketFailExpired,
        4 => Label::MarketFailLowGold,
        5 => Label::MarketFailNoSpace,
        6 => Label::MarketFailOwnItem,
        7 => Label::MarketFailTooFar,
        8 => Label::MarketFailCannotGainGold,
        9 => Label::MarketFailBidTooLow,
        10 => Label::MarketFailAuctionEnded,
        _ => Label::MarketFailGeneric,
    }
}

/// 画一个按钮（与商店/行会面板同款：深色底 + 1px 边框 + 小字）。
fn draw_button(frame: &mut Frame, text: &crate::ui::UiText, rect: Rect, label: &str) {
    frame.fill_rect(rect.x, rect.y, rect.width, rect.height, [34, 42, 54, 235]);
    frame.fill_rect(rect.x, rect.y, rect.width, 1, [92, 110, 136, 255]);
    frame.fill_rect(
        rect.x,
        rect.bottom() - 1,
        rect.width,
        1,
        [92, 110, 136, 255],
    );
    frame.fill_rect(rect.x, rect.y, 1, rect.height, [92, 110, 136, 255]);
    frame.fill_rect(
        rect.right() - 1,
        rect.y,
        1,
        rect.height,
        [92, 110, 136, 255],
    );
    text.draw_clipped(
        frame,
        rect.x + 6,
        rect.y + 2,
        label,
        SIZE_SMALL,
        BODY_COLOR,
        rect,
    );
}

/// 表头四列文案：市场/寄售/拍卖是「物品 / 价格 / 卖家 / 到期」；
/// 商城页签的 `seller` 恒为空、`ConsignmentDate` 不是到期时间，所以换成
/// 「物品 / 价格 / 分类 / 库存」（Crystal 商城页签也是 `TitleExpiryLabel` 不使用，
/// `TrustMerchantDialog.cs:1276-1300`）。
#[must_use]
pub fn header_labels(tab: MarketPanelType) -> [Label; 4] {
    if tab == MarketPanelType::GameShop {
        [
            Label::ItemColumn,
            Label::Price,
            Label::Category,
            Label::Stock,
        ]
    } else {
        [
            Label::ItemColumn,
            Label::Price,
            Label::Seller,
            Label::Expiry,
        ]
    }
}

/// 商城列表里「卖家」列显示的内容（服务端下发空串）。
#[must_use]
pub fn seller_text(listing: &MarketListing, language: Language) -> String {
    if listing.item_type == MarketItemType::GameShop {
        Label::GameShop.text(language).to_owned()
    } else {
        listing.seller.clone()
    }
}

/// 商城列表里「库存」列显示的内容：列表不显示实时库存（售罄条目仍会列出，
/// 购买时服务端回「库存不足」），「在售」只是占位文案。
#[must_use]
pub fn stock_text(listing: &MarketListing, language: Language) -> String {
    if listing.item_type == MarketItemType::GameShop {
        Label::OnSale.text(language).to_owned()
    } else {
        expiry_text(listing.consignment_date)
    }
}

/// 画市场面板。
///
/// 参数覆盖「布局 + 快照 + 三个输入框 + 选中/悬停 + 图标」，与 `trade::draw` 一样
/// 拆结构体只会增加样板。`fail` / `message` 由调用方按「是否已被页签切换清掉」
/// 算好（见 `UiState::market_notice_dismissed`），所以切页签不会残留上一页的提示。
#[allow(clippy::too_many_arguments)]
pub fn draw(
    frame: &mut Frame,
    layout: &MarketLayout,
    context: &PanelContext<'_>,
    tab: MarketPanelType,
    listings: &[MarketListing],
    page: i32,
    pages: i32,
    selected: Option<u64>,
    hover: Option<usize>,
    search: &TextField,
    price: &TextField,
    consign_item: Option<&InventoryItem>,
    focus: Option<crate::ui::FieldId>,
    fail: Option<u8>,
    message: Option<&str>,
    icon: &mut dyn FnMut(u16) -> Option<Image>,
) {
    let text = context.text;
    let language = context.language;
    let snapshot = context.snapshot;
    crate::ui::draw_panel(frame, layout.panel);

    // 标题 + 金币。
    let title = format!(
        "{} · {}",
        Label::Market.text(language),
        tab_label(tab).text(language)
    );
    text.draw_clipped(
        frame,
        layout.panel.x + 10,
        layout.panel.y + 4,
        &title,
        SIZE_BODY,
        TITLE_COLOR,
        Rect::new(
            layout.panel.x,
            layout.panel.y,
            layout.panel.width.saturating_sub(80),
            TITLE_HEIGHT,
        ),
    );
    let gold = snapshot
        .self_player
        .as_ref()
        .map_or(0, |player| player.gold);
    let credit = snapshot
        .self_player
        .as_ref()
        .map_or(0, |player| player.credit);
    // 商城页签用元宝结算，标题栏显示元宝余额（Crystal `TotalGold`，
    // `TrustMerchantDialog.cs:979`）。
    let (balance, unit) = if tab == MarketPanelType::GameShop {
        (credit, Label::Credit)
    } else {
        (gold, Label::Gold)
    };
    text.draw_right(
        frame,
        layout.close.x - 6,
        layout.panel.y + 5,
        &format!(
            "{} {}",
            labels::format_number(u64::from(balance)),
            unit.text(language)
        ),
        SIZE_SMALL,
        GOLD_COLOR,
    );
    draw_button(frame, text, layout.close, Label::Close.text(language));

    // 页签。
    for (index, rect) in layout.tabs.iter().enumerate() {
        let panel = TABS[index];
        let active = panel == tab;
        frame.fill_rect(
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            if active {
                [82, 68, 30, 240]
            } else {
                [30, 36, 46, 220]
            },
        );
        frame.fill_rect(
            rect.x,
            rect.bottom() - 2,
            rect.width,
            2,
            if active {
                [255, 216, 96, 255]
            } else {
                [70, 84, 104, 255]
            },
        );
        text.draw_clipped(
            frame,
            rect.x + 8,
            rect.y + 3,
            tab_label(panel).text(language),
            SIZE_SMALL,
            if active { TITLE_COLOR } else { MUTED_COLOR },
            *rect,
        );
    }

    // 工具栏：市场页是搜索，寄售/拍卖页是上架表单。
    if has_sell_form(tab) {
        input::draw(
            frame,
            text,
            layout.price_field,
            price,
            focus == Some(crate::ui::FieldId::MarketPrice),
            if tab == MarketPanelType::Auction {
                Label::StartingBid.text(language)
            } else {
                Label::ConsignPrice.text(language)
            },
        );
        draw_button(frame, text, layout.sell, Label::ListItem.text(language));
        let item_text = consign_item.map_or_else(
            || Label::NoSelectedItem.text(language).to_owned(),
            |item| {
                let name = snapshot
                    .item_info(item.item_index)
                    .map_or_else(|| item.item_index.to_string(), |info| info.name.clone());
                format!("{}: {}", Label::ConsignSelected.text(language), name)
            },
        );
        if let Some(item) = consign_item {
            if let Some(image) = snapshot.item_image(item.item_index).and_then(&mut *icon) {
                let image = scale_to_fit(&image, ROW_ICON_SIZE, ROW_ICON_SIZE);
                frame.blit(&image, layout.consign_item.x, layout.consign_item.y + 3);
            }
        }
        let item_area = Rect::new(
            layout.consign_item.x + 24,
            layout.consign_item.y,
            layout.consign_item.width.saturating_sub(24),
            TOOLBAR_HEIGHT,
        );
        text.draw_clipped(
            frame,
            item_area.x,
            item_area.y + 4,
            &item_text,
            SIZE_SMALL,
            BODY_COLOR,
            item_area,
        );
    } else {
        input::draw(
            frame,
            text,
            layout.search_field,
            search,
            focus == Some(crate::ui::FieldId::MarketSearch),
            Label::Search.text(language),
        );
        draw_button(frame, text, layout.find, Label::Search.text(language));
        draw_button(frame, text, layout.refresh, Label::Refresh.text(language));
    }

    // 表头（商城页签换成「物品 / 价格 / 分类 / 库存」）。
    let labels_for_tab = header_labels(tab);
    let headers = [
        (layout.item_header, labels_for_tab[0], false),
        (layout.price_header, labels_for_tab[1], true),
        (layout.seller_header, labels_for_tab[2], true),
        (layout.expiry_header, labels_for_tab[3], false),
    ];
    for (rect, label, right) in headers {
        if right {
            text.draw_right(
                frame,
                rect.right() - 4,
                rect.y + 2,
                label.text(language),
                SIZE_SMALL,
                MUTED_COLOR,
            );
        } else {
            text.draw_clipped(
                frame,
                rect.x + 4,
                rect.y + 2,
                label.text(language),
                SIZE_SMALL,
                MUTED_COLOR,
                rect,
            );
        }
    }

    // 列表行。
    let columns = column_widths(layout.panel.width.saturating_sub(PADDING * 2));
    for (index, row) in layout.rows.iter().enumerate() {
        let listing_index = layout.first_row + index;
        let Some(listing) = listings.get(listing_index) else {
            break;
        };
        let highlighted = selected == Some(listing.auction_id);
        let hovered = hover == Some(listing_index);
        if highlighted {
            frame.fill_rect(row.x, row.y, row.width, row.height, ROW_SELECTED_COLOR);
            frame.fill_rect(row.x, row.y, 2, row.height, ROW_MARK_COLOR);
        } else if hovered {
            frame.fill_rect(row.x, row.y, row.width, row.height, ROW_HOVER_COLOR);
        } else if index % 2 == 1 {
            frame.fill_rect(row.x, row.y, row.width, row.height, ROW_STRIPE_COLOR);
        }
        if let Some(image) = icon(listing.image) {
            let image = scale_to_fit(&image, ROW_ICON_SIZE, ROW_ICON_SIZE);
            let icon_y = row.y
                + (i32::try_from(ROW_HEIGHT).unwrap_or(24) - i32::from(image.info.height)) / 2;
            frame.blit(&image, row.x + 3, icon_y);
        }
        let item_col = Rect::new(row.x + 24, row.y, columns[0].saturating_sub(24), ROW_HEIGHT);
        let mut name = listing.name.clone();
        if listing.count > 1 {
            use std::fmt::Write as _;
            let _ = write!(name, " ×{}", listing.count);
        }
        text.draw_clipped(
            frame,
            item_col.x,
            row.y + 4,
            &name,
            SIZE_SMALL,
            if highlighted {
                [255, 255, 220, 255]
            } else {
                BODY_COLOR
            },
            item_col,
        );
        let price_col = Rect::new(
            row.x + i32::try_from(columns[0]).unwrap_or(0),
            row.y,
            columns[1],
            ROW_HEIGHT,
        );
        text.draw_right(
            frame,
            price_col.right() - 4,
            row.y + 4,
            &price_text(listing, language),
            SIZE_SMALL,
            GOLD_COLOR,
        );
        let seller_col = Rect::new(price_col.right(), row.y, columns[2], ROW_HEIGHT);
        text.draw_clipped(
            frame,
            seller_col.x + 4,
            row.y + 4,
            &seller_text(listing, language),
            SIZE_SMALL,
            BODY_COLOR,
            seller_col,
        );
        let expiry_col = Rect::new(seller_col.right(), row.y, columns[3], ROW_HEIGHT);
        text.draw_clipped(
            frame,
            expiry_col.x + 4,
            row.y + 4,
            &stock_text(listing, language),
            SIZE_SMALL,
            MUTED_COLOR,
            expiry_col,
        );
    }
    if layout.rows.is_empty() {
        let empty = if tab == MarketPanelType::GameShop {
            Label::NoGameShopItems
        } else {
            Label::NoListings
        };
        text.draw_clipped(
            frame,
            layout.header.x,
            layout.header.bottom() + 8,
            empty.text(language),
            SIZE_SMALL,
            MUTED_COLOR,
            Rect::new(
                layout.header.x,
                layout.header.bottom() + 4,
                layout.header.width,
                ROW_HEIGHT,
            ),
        );
    }

    // 翻页。
    draw_button(frame, text, layout.prev, "<");
    draw_button(frame, text, layout.next, ">");
    text.draw_clipped(
        frame,
        layout.page_label.x + 4,
        layout.page_label.y + 3,
        &format!("{}/{}", page.max(0) + 1, pages.max(1)),
        SIZE_SMALL,
        BODY_COLOR,
        layout.page_label,
    );
    text.draw_right(
        frame,
        layout.pager.right() - 4,
        layout.pager.y + 3,
        &format!(
            "{}: {}",
            Label::ItemColumn.text(language),
            labels::format_number(u64::try_from(listings.len()).unwrap_or(u64::MAX))
        ),
        SIZE_SMALL,
        MUTED_COLOR,
    );

    // 操作行。
    let selected_listing = selected.and_then(|auction_id| {
        listings
            .iter()
            .find(|listing| listing.auction_id == auction_id)
    });
    if is_user_mode(tab) {
        draw_button(
            frame,
            text,
            layout.primary,
            primary_label(tab, selected_listing).text(language),
        );
        if tab == MarketPanelType::Auction {
            draw_button(frame, text, layout.sell_now, Label::SellNow.text(language));
        }
        draw_button(
            frame,
            text,
            layout.collect,
            Label::CollectSold.text(language),
        );
    } else if tab == MarketPanelType::GameShop {
        // 商城：数量输入框（复用 bid_field 的位置）+「购买」（`C.GameshopBuy`）。
        input::draw(
            frame,
            text,
            layout.bid_field,
            price,
            focus == Some(crate::ui::FieldId::MarketPrice),
            Label::Quantity.text(language),
        );
        draw_button(
            frame,
            text,
            layout.primary,
            primary_label(tab, selected_listing).text(language),
        );
        text.draw_clipped(
            frame,
            layout.primary.right() + 8,
            layout.action.y + 3,
            Label::GameShopPickHint.text(language),
            SIZE_SMALL,
            MUTED_COLOR,
            Rect::new(
                layout.primary.right() + 8,
                layout.action.y,
                u32::try_from((layout.bid_field.x - 6 - layout.primary.right()).max(0))
                    .unwrap_or(0),
                BUTTON_HEIGHT,
            ),
        );
    } else {
        input::draw(
            frame,
            text,
            layout.bid_field,
            price,
            focus == Some(crate::ui::FieldId::MarketPrice),
            Label::BidAmount.text(language),
        );
        draw_button(
            frame,
            text,
            layout.primary,
            primary_label(tab, selected_listing).text(language),
        );
        text.draw_clipped(
            frame,
            layout.primary.right() + 8,
            layout.action.y + 3,
            Label::MarketHint.text(language),
            SIZE_SMALL,
            MUTED_COLOR,
            Rect::new(
                layout.primary.right() + 8,
                layout.action.y,
                u32::try_from((layout.bid_field.x - 6 - layout.primary.right()).max(0))
                    .unwrap_or(0),
                BUTTON_HEIGHT,
            ),
        );
    }

    // 底部：失败 > 成功 > 提示。
    let (footer_text, footer_color) = if let Some(reason) = fail {
        (fail_label(reason).text(language).to_owned(), FAIL_COLOR)
    } else if let Some(message) = message {
        (message.to_owned(), SUCCESS_COLOR)
    } else if tab == MarketPanelType::GameShop {
        (Label::GameShopHint.text(language).to_owned(), MUTED_COLOR)
    } else if has_sell_form(tab) {
        (
            if tab == MarketPanelType::Auction {
                Label::AuctionHint.text(language).to_owned()
            } else {
                Label::ConsignHint.text(language).to_owned()
            },
            MUTED_COLOR,
        )
    } else {
        (Label::MarketHint.text(language).to_owned(), MUTED_COLOR)
    };
    text.draw_clipped(
        frame,
        layout.footer.x,
        layout.footer.y,
        &footer_text,
        SIZE_SMALL,
        footer_color,
        layout.footer,
    );
}

/// 寄售表单里选中的物品是否可上架（对齐 Crystal：`Durability < 0` 的物品不能拖入，
/// `TrustMerchantDialog.cs:1387-1389`；这里只看是否有物品）。
#[must_use]
pub fn can_consign(item: Option<&InventoryItem>) -> bool {
    item.is_some()
}

/// 页内滚动上限（给 `UiState::scroll_market` 用）。
#[must_use]
pub fn max_scroll(listing_count: usize, page: i32, visible_rows: usize) -> usize {
    let page_start = usize::try_from(page.max(0))
        .unwrap_or(0)
        .saturating_mul(MARKET_PAGE_SIZE);
    listing_count
        .saturating_sub(page_start)
        .min(MARKET_PAGE_SIZE)
        .saturating_sub(visible_rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_cover_all_panels() {
        for panel in TABS {
            assert!(!tab_label(panel).text(Language::ZhCn).is_empty());
            assert!(!tab_label(panel).text(Language::En).is_empty());
        }
        assert!(is_user_mode(MarketPanelType::Consign));
        assert!(is_user_mode(MarketPanelType::Auction));
        assert!(!is_user_mode(MarketPanelType::Market));
        assert!(has_sell_form(MarketPanelType::Consign));
        assert!(!has_sell_form(MarketPanelType::Market));
    }

    #[test]
    fn fail_reasons_are_localized() {
        for reason in 0..=10 {
            let label = fail_label(reason);
            assert!(!label.text(Language::ZhCn).is_empty());
            assert!(!label.text(Language::En).is_empty());
        }
        assert_eq!(fail_label(4), Label::MarketFailLowGold);
        assert_eq!(fail_label(99), Label::MarketFailGeneric);
    }

    #[test]
    fn price_and_expiry_text_follow_crystal() {
        let listing = MarketListing {
            auction_id: 1,
            item_index: 1,
            count: 1,
            current_dura: 0,
            max_dura: 0,
            name: "木剑".to_owned(),
            image: 0,
            seller: "张三".to_owned(),
            price: 1_200,
            item_type: MarketItemType::Consign,
            consignment_date: DateTimeBinary::from_unix_seconds(1_704_067_200),
        };
        assert_eq!(price_text(&listing, Language::ZhCn), "1,200 金币");
        assert_eq!(
            primary_label(MarketPanelType::Market, Some(&listing)),
            Label::Buy
        );
        assert_eq!(
            expiry_text(DateTimeBinary::from_unix_seconds(1_704_067_200)),
            "01-08 00:00"
        );
        assert_eq!(expiry_text(DateTimeBinary::default()), "--");
        let auction = MarketListing {
            item_type: MarketItemType::Auction,
            ..listing
        };
        assert_eq!(price_text(&auction, Language::En), "1,200 Bid");
        assert_eq!(
            primary_label(MarketPanelType::Market, Some(&auction)),
            Label::Bid
        );
        assert_eq!(
            primary_label(MarketPanelType::Consign, Some(&auction)),
            Label::GetBack
        );
        // 商城页签固定「购买」（没选中条目时也是「购买」，不是「买下」）。
        assert_eq!(
            primary_label(MarketPanelType::GameShop, None),
            Label::Purchase
        );
        assert_eq!(
            primary_label(MarketPanelType::GameShop, Some(&auction)),
            Label::Purchase
        );
    }
}
