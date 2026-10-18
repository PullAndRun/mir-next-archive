//! 交易面板（`Crystal TradeDialog` + `GuestTradeDialog`，
//! `Client/MirScenes/Dialogs/TradeDialogs.cs`）。
//!
//! 布局照 Crystal：左右两栏各 `5 × 2` 格（`TradeDialog.Grid = new MirItemCell[5 * 2]`，
//! `TradeDialogs.cs:104`；对方的 `GuestGrid` 同样 5 × 2，`:237`），每栏顶部是名字、
//! 底部是金币；本作把两个窗口拼成一个面板，左栏「自己」、右栏「对方」。
//!
//! | 元素 | 本项目 | Crystal 出处 |
//! |---|---|---|
//! | 自己的名字 | 面板标题「交易 (对方：名字)」+ 左栏标题 | `NameLabel.Text = User.Name`（`TradeDialogs.cs:144`） |
//! | 自己的金币 | 左栏底部「金币 1,234」+ 输入框 + 放入 | `GoldLabel` + `MirAmountBox`（`:71-100`） |
//! | 对方的金币 | 右栏底部「金币 1,234」 | `GuestGoldLabel`（`:224-233`）、`S.TradeGold` |
//! | 准备 / 确认 | 一个按钮，未准备时显示「准备」、已准备显示「确认」 | `ConfirmButton`（`:25-40`）+ `ChangeLockState`（`:122-140`） |
//! | 取消 | 「取消交易」按钮发 `C.TradeCancel` | `CloseButton.Click → TradeCancel()`（`:52-57`、`:179-182`） |
//! | 对方状态 | 「对方：未准备 / 已准备」 | **Crystal 没有这一行**，见 [`crate::net::TradeView::partner_ready`] |
//!
//! 简化（原因）：
//!
//! * 面板不可拖动，固定屏幕居中、状态栏下方（Crystal 的两个窗口 `Movable = true`）；
//! * 金币用面板内的输入框 + 「放入金币」按钮，没有 Crystal 的 `MirAmountBox` 弹窗；
//! * 「准备」和「确认」是同一个按钮的两个阶段（Crystal 只有一个锁定开关），
//!   两次点击都发 `C.TradeConfirm{locked: true}`（服务端幂等）。

use mir_core::{Language, Point};
use mir_render::{Frame, Image};

use crate::geom::Rect;
use crate::net::{InventoryItem, TRADE_SLOTS, TradeView};
use crate::ui::PanelContext;
use crate::ui::UiText;
use crate::ui::grid::{self, GridLayout};
use crate::ui::hud::{BODY_COLOR, GOLD_COLOR, MUTED_COLOR, SIZE_BODY, SIZE_SMALL, TITLE_COLOR};
use crate::ui::input::{self, TextField};
use crate::ui::labels::{self, Label};

/// 面板宽度（两栏 + 三条内边距）。
pub const PANEL_WIDTH: u32 = 500;
/// 面板高度。
pub const PANEL_HEIGHT: u32 = 246;
/// 内边距。
pub const PADDING: u32 = 10;
/// 标题高度。
pub const TITLE_HEIGHT: u32 = 24;
/// 每栏标题行高度。
pub const HEADER_HEIGHT: u32 = 20;
/// 每栏宽度。
pub const COLUMN_WIDTH: u32 = 230;
/// 格子边长（Crystal 的 `MirItemCell` 是 36×32，`TradeDialogs.cs:115`）。
pub const CELL: u32 = 32;
/// 格子间距。
pub const GAP: u32 = 3;
/// 每行格数。
pub const COLUMNS: usize = 5;
/// 按钮高度。
pub const BUTTON_HEIGHT: u32 = 24;
/// 已准备的名字颜色（与组队蓝一致）。
pub const READY_COLOR: [u8; 4] = [120, 200, 255, 255];

/// 交易面板的布局（绘制与命中测试共用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradeLayout {
    /// 面板。
    pub panel: Rect,
    /// 左栏标题（自己的名字 + 金币）。
    pub own_header: Rect,
    /// 自己的 10 格。
    pub own_grid: GridLayout,
    /// 自己金币行（标签）。
    pub own_gold: Rect,
    /// 金币输入框。
    pub gold_field: Rect,
    /// 「放入金币」按钮。
    pub gold_add: Rect,
    /// 右栏标题（对方名字）。
    pub partner_header: Rect,
    /// 对方的 10 格。
    pub partner_grid: GridLayout,
    /// 对方金币行。
    pub partner_gold: Rect,
    /// 对方状态行（未准备/已准备）。
    pub partner_state: Rect,
    /// 准备/确认按钮。
    pub ready: Rect,
    /// 取消交易按钮。
    pub cancel: Rect,
    /// 底部提示。
    pub footer: Rect,
}

impl TradeLayout {
    /// 命中自己的交易格（绝对下标）。
    #[must_use]
    pub fn own_slot_at(&self, point: Point) -> Option<usize> {
        self.own_grid.slot_at(point)
    }

    /// 命中对方的交易格（只看不拖）。
    #[must_use]
    pub fn partner_slot_at(&self, point: Point) -> Option<usize> {
        self.partner_grid.slot_at(point)
    }
}

/// 计算交易面板布局（靠左、状态栏下方）。
///
/// 靠左（与行会/仓库面板一致）而不是居中：面板不可拖动，居中会盖住右侧的背包面板，
/// 「背包 → 交易格」的拖拽就点不到了（Crystal 把两个交易窗放在屏幕中间偏左，
/// 同时把背包窗口移到最右侧，`TradeDialogs.cs:21,154`）。
#[must_use]
pub fn trade_layout(frame: Rect) -> TradeLayout {
    // 小窗口（如 320×240 的测试基准）下必须先让到屏幕内，再谈「最小可读宽度」：
    // 先夹到 `frame`，`min` 里的 `360` 只在屏幕够宽时才起作用。
    let available_width = frame.width.saturating_sub(16);
    let width = PANEL_WIDTH
        .min(available_width)
        .max(360.min(available_width));
    let available_height = frame.height.saturating_sub(60);
    let height = PANEL_HEIGHT
        .min(available_height)
        .max(180.min(available_height));
    let panel = Rect::new(frame.x + 8, frame.y + 60, width, height);
    let content_x = panel.x + i32::try_from(PADDING).unwrap_or(0);
    let content_width = width.saturating_sub(PADDING * 2);
    let column_width = COLUMN_WIDTH
        .min(content_width.saturating_sub(COLUMN_WIDTH) / 2)
        .max(160);
    let left_x = content_x;
    let right_x = panel.right()
        - i32::try_from(PADDING).unwrap_or(0)
        - i32::try_from(column_width).unwrap_or(0);
    let header_y = panel.y + i32::try_from(TITLE_HEIGHT + 2).unwrap_or(0);
    let own_header = Rect::new(left_x, header_y, column_width, HEADER_HEIGHT);
    let partner_header = Rect::new(right_x, header_y, column_width, HEADER_HEIGHT);
    let grid_y = header_y + i32::try_from(HEADER_HEIGHT + 2).unwrap_or(0);
    let grid_height = 2 * CELL + GAP;
    let own_grid = grid::grid_layout(
        Rect::new(left_x, grid_y, column_width, grid_height),
        TRADE_SLOTS,
        COLUMNS,
        CELL.saturating_sub(2),
        GAP,
        0,
    );
    let partner_grid = grid::grid_layout(
        Rect::new(right_x, grid_y, column_width, grid_height),
        TRADE_SLOTS,
        COLUMNS,
        CELL.saturating_sub(2),
        GAP,
        0,
    );
    let gold_y = grid_y + i32::try_from(grid_height + 6).unwrap_or(0);
    let own_gold = Rect::new(left_x, gold_y, column_width, 20);
    let partner_gold = Rect::new(right_x, gold_y, column_width, 20);
    let field_y = gold_y + 22;
    let gold_field = Rect::new(left_x, field_y, 100, 22);
    let gold_add = Rect::new(left_x + 106, field_y, 84, 22);
    let partner_state = Rect::new(right_x, gold_y + 4, column_width, SIZE_SMALL + 6);
    let button_y = panel.bottom() - i32::try_from(BUTTON_HEIGHT + 22).unwrap_or(0);
    let ready = Rect::new(left_x.max(content_x), button_y, 96, BUTTON_HEIGHT);
    let cancel = Rect::new(ready.right() + 8, button_y, 104, BUTTON_HEIGHT);
    let footer = Rect::new(
        content_x,
        panel.bottom() - i32::try_from(BUTTON_HEIGHT - 4).unwrap_or(0),
        content_width,
        BUTTON_HEIGHT - 4,
    );
    TradeLayout {
        panel,
        own_header,
        own_grid,
        own_gold,
        gold_field,
        gold_add,
        partner_header,
        partner_grid,
        partner_gold,
        partner_state,
        ready,
        cancel,
        footer,
    }
}

/// 画一个按钮（与行会面板同款样式）。
// 参数覆盖「布局 + 快照 + 金币输入 + 焦点 + 鼠标 + 图标」，拆结构体只会增加样板。
#[allow(clippy::too_many_arguments)]
pub fn draw(
    frame: &mut Frame,
    layout: &TradeLayout,
    context: &PanelContext<'_>,
    trade: &TradeView,
    locked: bool,
    gold_field: &TextField,
    focus: Option<crate::ui::FieldId>,
    mouse: Point,
    icon: &mut dyn FnMut(u16) -> Option<Image>,
) {
    let text = context.text;
    let language = context.language;
    let snapshot = context.snapshot;
    crate::ui::draw_panel(frame, layout.panel);
    let title = if trade.partner.is_empty() {
        Label::Trade.text(language).to_owned()
    } else {
        format!(
            "{} · {}：{}",
            Label::Trade.text(language),
            Label::TradePartnerState.text(language),
            trade.partner
        )
    };
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
            layout.panel.width,
            TITLE_HEIGHT,
        ),
    );

    // 左栏：自己的名字 + 金币。
    text.draw_clipped(
        frame,
        layout.own_header.x + 2,
        layout.own_header.y + 2,
        &format!(
            "{} · {} {}",
            Label::TradeYourSide.text(language),
            labels::format_number(u64::from(trade.gold)),
            Label::Gold.text(language)
        ),
        SIZE_SMALL,
        BODY_COLOR,
        layout.own_header,
    );
    // 右栏：对方名字 + 金币。
    text.draw_clipped(
        frame,
        layout.partner_header.x + 2,
        layout.partner_header.y + 2,
        &if trade.partner.is_empty() {
            Label::Empty.text(language).to_owned()
        } else {
            format!(
                "{} · {} {}",
                trade.partner,
                labels::format_number(u64::from(trade.partner_gold)),
                Label::Gold.text(language)
            )
        },
        SIZE_SMALL,
        BODY_COLOR,
        layout.partner_header,
    );

    grid::draw(
        frame,
        text,
        &layout.own_grid,
        &trade.items,
        snapshot,
        None,
        None,
        None,
        icon,
    );
    grid::draw(
        frame,
        text,
        &layout.partner_grid,
        &trade.partner_items,
        snapshot,
        None,
        None,
        None,
        icon,
    );

    // 自己的金币行：标签 + 输入框 + 放入按钮。
    text.draw_clipped(
        frame,
        layout.own_gold.x + 2,
        layout.own_gold.y + 3,
        &format!(
            "{} {}",
            Label::Gold.text(language),
            labels::format_number(u64::from(trade.gold))
        ),
        SIZE_SMALL,
        GOLD_COLOR,
        layout.own_gold,
    );
    input::draw(
        frame,
        text,
        layout.gold_field,
        gold_field,
        focus == Some(crate::ui::FieldId::TradeGold),
        Label::Amount.text(language),
    );
    draw_button(
        frame,
        text,
        layout.gold_add,
        Label::TradeGoldAdd.text(language),
        mouse,
        !gold_field.is_empty(),
    );

    // 对方金币 + 状态。
    text.draw_clipped(
        frame,
        layout.partner_gold.x + 2,
        layout.partner_gold.y + 3,
        &format!(
            "{} {}",
            Label::TradePartnerGold.text(language),
            labels::format_number(u64::from(trade.partner_gold))
        ),
        SIZE_SMALL,
        GOLD_COLOR,
        layout.partner_gold,
    );
    let (state, color) = if trade.partner_ready {
        (Label::Ready.text(language), READY_COLOR)
    } else {
        (Label::NotReady.text(language), MUTED_COLOR)
    };
    text.draw_clipped(
        frame,
        layout.partner_state.x + 2,
        layout.partner_state.y + 3,
        &format!("{}：{state}", Label::TradePartnerState.text(language)),
        SIZE_SMALL,
        color,
        layout.partner_state,
    );

    // 准备 / 确认 / 取消。
    let ready_label = if locked {
        Label::TradeConfirm.text(language)
    } else {
        Label::TradeReady.text(language)
    };
    draw_button(frame, text, layout.ready, ready_label, mouse, true);
    if locked {
        frame.fill_rect(
            layout.ready.x,
            layout.ready.y,
            2,
            layout.ready.height,
            READY_COLOR,
        );
    }
    draw_button(
        frame,
        text,
        layout.cancel,
        Label::TradeCancel.text(language),
        mouse,
        true,
    );
    text.draw_clipped(
        frame,
        layout.footer.x,
        layout.footer.y,
        Label::TradeHint.text(language),
        SIZE_SMALL,
        MUTED_COLOR,
        layout.footer,
    );
}

/// 画交易邀请弹窗的正文（「XXX 请求与你交易」；按钮由 [`crate::ui::notice`] 画）。
#[must_use]
pub fn invite_message(name: &str, language: Language) -> String {
    format!("{name} {}", Label::TradeRequestSuffix.text(language))
}

/// 画一个按钮（与行会面板同款样式）。
fn draw_button(
    frame: &mut Frame,
    text: &UiText,
    rect: Rect,
    label: &str,
    mouse: Point,
    enabled: bool,
) {
    let hovered = enabled && rect.contains(mouse);
    let fill = if !enabled {
        [34, 38, 44, 220]
    } else if hovered {
        [92, 82, 42, 255]
    } else {
        [44, 52, 66, 255]
    };
    frame.fill_rect(rect.x, rect.y, rect.width, rect.height, fill);
    frame.fill_rect(rect.x, rect.y, rect.width, 1, [140, 156, 180, 255]);
    frame.fill_rect(rect.x, rect.bottom() - 1, rect.width, 1, [36, 44, 56, 255]);
    text.draw_center(
        frame,
        rect,
        label,
        SIZE_SMALL,
        if enabled {
            [255, 240, 180, 255]
        } else {
            [120, 126, 134, 255]
        },
    );
}

/// 交易格的物品（测试与 tooltip 用）。
#[must_use]
pub fn slot_item(items: &[Option<InventoryItem>], slot: usize) -> Option<&InventoryItem> {
    items.get(slot).and_then(Option::as_ref)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_has_two_ten_slot_columns_inside_the_panel() {
        let frame = Rect::screen(800, 600);
        let layout = trade_layout(frame);
        assert_eq!(layout.own_grid.slots.len(), TRADE_SLOTS);
        assert_eq!(layout.partner_grid.slots.len(), TRADE_SLOTS);
        for (index, rect) in layout.own_grid.slots.iter().enumerate() {
            assert!(layout.panel.contains(rect.center()));
            assert_eq!(layout.own_slot_at(rect.center()), Some(index));
        }
        for (index, rect) in layout.partner_grid.slots.iter().enumerate() {
            assert!(layout.panel.contains(rect.center()));
            assert_eq!(layout.partner_slot_at(rect.center()), Some(index));
        }
        // 两栏不重叠。
        assert!(
            layout
                .own_grid
                .area
                .intersect(layout.partner_grid.area)
                .is_none()
        );
        assert!(layout.ready.y < layout.panel.bottom());
        assert!(layout.cancel.x > layout.ready.x);
    }

    #[test]
    fn small_windows_still_produce_a_usable_panel() {
        let layout = trade_layout(Rect::screen(320, 240));
        assert!(layout.panel.width <= 320);
        assert!(layout.own_grid.slots.len() <= TRADE_SLOTS);
    }
}
