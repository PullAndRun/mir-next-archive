//! 商店面板：货架列表、价格、选中/悬停高亮与买/卖/修提示。
//!
//! 数据来自 `S.NPCGoods`（`WorldSnapshot::npc_goods`）；点击某一行就买一件
//! （`C.BuyItem { item_index: 商店物品唯一编号, count: 1 }`）。出售/修理走背包里
//! 选中的物品（`S` / `R`），对应 `C.SellItem` / `C.RepairItem`。
//!
//! 商品多于可见行时用 [`shop_layout`] 的 `scroll` 参数滚动（鼠标滚轮/上下键由
//! `UiState::on_wheel` / `UiState::scroll_shop` 维护），右侧画一条比例滚动条。
//! `ShopLayout::row_at` 返回**货架绝对下标**（已加 `first_row`），调用方不必再换算。

use mir_render::{Frame, Image, scale_to_fit};

use crate::geom::Rect;
use crate::net::ShopGoods;
use crate::ui::PanelContext;
use crate::ui::hud::{BODY_COLOR, GOLD_COLOR, MUTED_COLOR, SIZE_BODY, SIZE_SMALL, TITLE_COLOR};
use crate::ui::labels::{self, Label};
use mir_core::Language;

/// 商店面板宽度。
pub const SHOP_WIDTH: u32 = 312;
/// 每行高度。
pub const ROW_HEIGHT: u32 = 26;
/// 行内图标最大边长。
pub const ROW_ICON_SIZE: u32 = 20;
/// 标题高度。
pub const TITLE_HEIGHT: u32 = 24;
/// 底部提示高度。
pub const FOOTER_HEIGHT: u32 = 22;
/// 内边距。
pub const PADDING: u32 = 8;
/// 一屏最多显示多少行（超出的部分滚动查看）。
pub const MAX_SHOP_ROWS: usize = 11;
/// 商店面板离屏幕左边的默认距离。
pub const SHOP_LEFT: i32 = 8;
/// 滚动条宽度。
pub const SCROLLBAR_WIDTH: u32 = 4;
/// 滚动条离面板右边的距离。
pub const SCROLLBAR_MARGIN: u32 = 4;

/// 商店面板左边距：装备面板同时打开时右移，避免两块左侧面板重叠。
#[must_use]
pub fn shop_left(equipment_open: bool) -> i32 {
    if equipment_open {
        SHOP_LEFT + i32::try_from(crate::ui::equipment::panel_width()).unwrap_or(0) + 8
    } else {
        SHOP_LEFT
    }
}

/// 商店面板布局。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShopLayout {
    /// 面板矩形。
    pub panel: Rect,
    /// 当前可见的每一行（货架条目）矩形。
    pub rows: Vec<Rect>,
    /// 底部提示区域。
    pub footer: Rect,
    /// 第一条可见货架的下标（滚动位置）。
    pub first_row: usize,
    /// 滚动条轨道（没有可滚动内容时为 `None`）。
    pub scrollbar: Option<Rect>,
}

impl ShopLayout {
    /// 命中测试：点在哪个货架行上（返回**货架绝对下标**）。
    #[must_use]
    pub fn row_at(&self, point: mir_core::Point) -> Option<usize> {
        self.rows
            .iter()
            .position(|rect| rect.contains(point))
            .map(|index| self.first_row + index)
    }

    /// 滚动条滑块矩形（`total` 是货架总条数）。
    #[must_use]
    pub fn scroll_thumb(&self, total: usize) -> Option<Rect> {
        let track = self.scrollbar?;
        if total == 0 {
            return None;
        }
        let visible = self.rows.len();
        if visible == 0 {
            return None;
        }
        let height = (track.height * u32::try_from(visible).unwrap_or(0)
            / u32::try_from(total).unwrap_or(1))
        .max(10)
        .min(track.height);
        let max_offset = track.height.saturating_sub(height);
        let max_scroll = total.saturating_sub(visible);
        let top = if max_scroll == 0 {
            0
        } else {
            max_offset * u32::try_from(self.first_row.min(max_scroll)).unwrap_or(0)
                / u32::try_from(max_scroll).unwrap_or(1)
        };
        Some(Rect::new(
            track.x,
            track.y + i32::try_from(top).unwrap_or(0),
            track.width,
            height,
        ))
    }
}

/// 计算商店面板布局。
#[must_use]
pub fn shop_layout(frame: Rect, row_count: usize, scroll: usize, left: i32) -> ShopLayout {
    let visible = row_count.min(MAX_SHOP_ROWS);
    let max_scroll = row_count.saturating_sub(visible);
    let first_row = scroll.min(max_scroll);
    let height = TITLE_HEIGHT
        + u32::try_from(visible)
            .unwrap_or(0)
            .saturating_mul(ROW_HEIGHT)
        + FOOTER_HEIGHT
        + PADDING
        + 4;
    let panel = Rect::new(
        left,
        60,
        SHOP_WIDTH.min(frame.width.saturating_sub(16)).max(160),
        height,
    );
    let mut rows = Vec::with_capacity(visible);
    for index in 0..visible {
        rows.push(Rect::new(
            panel.x + i32::try_from(PADDING).unwrap_or(0),
            panel.y
                + i32::try_from(TITLE_HEIGHT + 2).unwrap_or(0)
                + i32::try_from(index).unwrap_or(0) * i32::try_from(ROW_HEIGHT).unwrap_or(0),
            panel.width.saturating_sub(PADDING * 2),
            ROW_HEIGHT,
        ));
    }
    let footer = Rect::new(
        panel.x + i32::try_from(PADDING).unwrap_or(0),
        panel.bottom() - i32::try_from(FOOTER_HEIGHT + 4).unwrap_or(0),
        panel.width.saturating_sub(PADDING * 2),
        FOOTER_HEIGHT,
    );
    let scrollbar = if max_scroll > 0 {
        let rows_top = panel.y + i32::try_from(TITLE_HEIGHT + 2).unwrap_or(0);
        let rows_height = u32::try_from(visible)
            .unwrap_or(0)
            .saturating_mul(ROW_HEIGHT)
            .saturating_sub(4);
        Some(Rect::new(
            panel.right() - i32::try_from(SCROLLBAR_MARGIN + SCROLLBAR_WIDTH).unwrap_or(0),
            rows_top,
            SCROLLBAR_WIDTH,
            rows_height,
        ))
    } else {
        None
    };
    ShopLayout {
        panel,
        rows,
        footer,
        first_row,
        scrollbar,
    }
}

/// 单价文案：`1,200 金币`。
#[must_use]
pub fn price_text(price: u32, language: Language) -> String {
    format!(
        "{} {}",
        labels::format_number(u64::from(price)),
        Label::Gold.text(language)
    )
}

/// 一行商品的完整文案（名字 + 数量 + 价格，测试与绘制共用）。
#[must_use]
pub fn row_text(entry: &crate::net::ShopEntry, language: Language) -> String {
    use std::fmt::Write as _;
    // 模板缺失时 `name` 是空的：按语言渲染占位（服务端不发模板才走到这里）。
    let mut text = if entry.name.is_empty() {
        labels::unknown_item(language, entry.item_index)
    } else {
        entry.name.clone()
    };
    if entry.count > 1 {
        let _ = write!(text, " ×{}", entry.count);
    }
    text.push_str("  ");
    text.push_str(&price_text(entry.price, language));
    text
}

/// 价格倍率文案：`倍率 100%`。
#[must_use]
pub fn rate_text(rate: f32, language: Language) -> String {
    let percent = if rate.is_finite() {
        (rate * 100.0).round() as i32
    } else {
        100
    };
    format!("{} {percent}%", Label::Rate.text(language))
}

/// 画商店面板。
pub fn draw(
    frame: &mut Frame,
    layout: &ShopLayout,
    context: &PanelContext<'_>,
    goods: &ShopGoods,
    selected: Option<usize>,
    hover: Option<usize>,
    icon: &mut dyn FnMut(u16) -> Option<Image>,
) {
    let text = context.text;
    let language = context.language;
    crate::ui::draw_panel(frame, layout.panel);
    text.draw_clipped(
        frame,
        layout.panel.x + 10,
        layout.panel.y + 4,
        &format!("{} (NPC)", Label::Shop.text(language)),
        SIZE_BODY,
        TITLE_COLOR,
        Rect::new(
            layout.panel.x,
            layout.panel.y,
            layout.panel.width,
            TITLE_HEIGHT,
        ),
    );
    text.draw_right(
        frame,
        layout.panel.right() - 10,
        layout.panel.y + 6,
        &rate_text(goods.rate, language),
        SIZE_SMALL,
        MUTED_COLOR,
    );

    for (index, row) in layout.rows.iter().enumerate() {
        let item_index = layout.first_row + index;
        let Some(entry) = goods.list.get(item_index) else {
            break;
        };
        let highlighted = selected == Some(item_index);
        let hovered = hover == Some(item_index);
        if highlighted {
            frame.fill_rect(row.x, row.y, row.width, row.height, [86, 74, 32, 235]);
            frame.fill_rect(row.x, row.y, 2, row.height, [255, 216, 96, 255]);
        } else if hovered {
            frame.fill_rect(row.x, row.y, row.width, row.height, [44, 56, 76, 235]);
            frame.fill_rect(row.x, row.y, 2, row.height, [128, 176, 240, 255]);
        } else if index % 2 == 1 {
            frame.fill_rect(row.x, row.y, row.width, row.height, [26, 30, 38, 160]);
        }
        let loaded = icon(entry.image);
        if let Some(image) = loaded {
            let image = scale_to_fit(&image, ROW_ICON_SIZE, ROW_ICON_SIZE);
            let icon_x = row.x + 3;
            let icon_y =
                row.y + (i32::try_from(ROW_HEIGHT).unwrap_or(0) - i32::from(image.info.height)) / 2;
            frame.blit(&image, icon_x, icon_y);
        }
        let name_area = Rect::new(row.x + 30, row.y, row.width.saturating_sub(114), ROW_HEIGHT);
        let mut label = if entry.name.is_empty() {
            labels::unknown_item(language, entry.item_index)
        } else {
            entry.name.clone()
        };
        if entry.count > 1 {
            use std::fmt::Write as _;
            let _ = write!(label, " ×{}", entry.count);
        }
        text.draw_clipped(
            frame,
            name_area.x,
            row.y + 5,
            &label,
            SIZE_BODY,
            if highlighted {
                [255, 255, 220, 255]
            } else {
                BODY_COLOR
            },
            name_area,
        );
        text.draw_right(
            frame,
            row.right() - 6,
            row.y + 5,
            &price_text(entry.price, language),
            SIZE_BODY,
            GOLD_COLOR,
        );
    }

    if let Some(track) = layout.scrollbar {
        frame.fill_rect_alpha(
            track.x,
            track.y,
            track.width,
            track.height,
            [40, 48, 60, 200],
        );
        if let Some(thumb) = layout.scroll_thumb(goods.list.len()) {
            frame.fill_rect(
                thumb.x,
                thumb.y,
                thumb.width,
                thumb.height,
                [128, 176, 240, 255],
            );
        }
        text.draw_right(
            frame,
            layout.panel.right() - 10,
            layout.footer.y,
            &format!("{}/{}", layout.first_row + 1, goods.list.len()),
            SIZE_SMALL,
            MUTED_COLOR,
        );
    }

    let mut hint = Label::ShopHint.text(language).to_owned();
    if context.snapshot.npc_panels.sell {
        hint.push_str(" · ");
        hint.push_str(Label::SellAllowed.text(language));
    }
    if context.snapshot.npc_panels.repair || context.snapshot.npc_panels.special_repair {
        hint.push_str(" · ");
        hint.push_str(Label::RepairAllowed.text(language));
    }
    text.draw_clipped(
        frame,
        layout.footer.x,
        layout.footer.y,
        &hint,
        SIZE_SMALL,
        MUTED_COLOR,
        layout.footer,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::ShopEntry;

    fn entry(name: &str, price: u32, count: u16) -> ShopEntry {
        ShopEntry {
            unique_id: 7,
            item_index: 1,
            name: name.to_owned(),
            image: 2,
            price,
            count,
            current_dura: 0,
            max_dura: 0,
        }
    }

    #[test]
    fn rows_are_hit_testable_and_inside_the_panel() {
        let layout = shop_layout(Rect::screen(800, 600), 5, 0, SHOP_LEFT);
        assert_eq!(layout.rows.len(), 5);
        for (index, row) in layout.rows.iter().enumerate() {
            assert!(layout.panel.contains(row.center()));
            assert_eq!(layout.row_at(row.center()), Some(index));
        }
        assert!(layout.row_at(layout.panel.center()).is_some() || layout.rows.is_empty());
        assert_eq!(layout.row_at(mir_core::Point::new(0, 0)), None);
    }

    #[test]
    fn rows_are_capped_and_stay_above_the_chat_panel() {
        let layout = shop_layout(Rect::screen(800, 600), 40, 0, SHOP_LEFT);
        assert_eq!(layout.rows.len(), MAX_SHOP_ROWS);
        assert!(layout.panel.bottom() < 422, "商店不应压住聊天面板");
        assert!(layout.footer.y > layout.rows.last().unwrap().y);
    }

    #[test]
    fn scrolling_moves_the_visible_window_and_the_scrollbar_thumb() {
        let top = shop_layout(Rect::screen(800, 600), 40, 0, SHOP_LEFT);
        let scrolled = shop_layout(Rect::screen(800, 600), 40, 5, SHOP_LEFT);
        assert_eq!(top.first_row, 0);
        assert_eq!(scrolled.first_row, 5);
        assert_eq!(
            scrolled.row_at(scrolled.rows[0].center()),
            Some(5),
            "row_at 返回货架绝对下标"
        );
        let top_thumb = top.scroll_thumb(40).expect("可滚动");
        let scrolled_thumb = scrolled.scroll_thumb(40).expect("可滚动");
        assert!(scrolled_thumb.y > top_thumb.y, "滚动条应下移");
        assert!(scrolled_thumb.bottom() <= scrolled.scrollbar.unwrap().bottom());
        // 超出范围时收边到最后一条。
        let clamped = shop_layout(Rect::screen(800, 600), 40, 999, SHOP_LEFT);
        assert_eq!(clamped.first_row, 40 - MAX_SHOP_ROWS);
        assert!(
            shop_layout(Rect::screen(800, 600), 3, 0, SHOP_LEFT)
                .scrollbar
                .is_none()
        );
    }

    #[test]
    fn row_and_price_text_are_localized() {
        let entry = entry("金创药", 1_200, 5);
        assert_eq!(row_text(&entry, Language::ZhCn), "金创药 ×5  1,200 金币");
        assert_eq!(price_text(50, Language::En), "50 Gold");
        assert_eq!(rate_text(1.0, Language::ZhCn), "倍率 100%");
        assert_eq!(rate_text(0.85, Language::En), "Rate 85%");
    }
}
