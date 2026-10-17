//! 商店面板：货架列表、价格、选中/悬停高亮与买/卖/修提示。
//!
//! 数据来自 `S.NPCGoods`（`WorldSnapshot::npc_goods`）；点击某一行就买一件
//! （`C.BuyItem { item_index: 商店物品唯一编号, count: 1 }`）。出售/修理走背包里
//! 选中的物品（`S` / `R`），对应 `C.SellItem` / `C.RepairItem`。

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
/// 最多同时显示多少行（超出的货架用 `+N` 提示，滚动留到以后做）。
pub const MAX_SHOP_ROWS: usize = 11;

/// 商店面板布局。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShopLayout {
    /// 面板矩形。
    pub panel: Rect,
    /// 每一行（货架条目）的矩形。
    pub rows: Vec<Rect>,
    /// 底部提示区域。
    pub footer: Rect,
}

impl ShopLayout {
    /// 命中测试：点在哪个货架行上。
    #[must_use]
    pub fn row_at(&self, point: mir_core::Point) -> Option<usize> {
        self.rows.iter().position(|rect| rect.contains(point))
    }
}

/// 计算商店面板布局。
#[must_use]
pub fn shop_layout(frame: Rect, row_count: usize) -> ShopLayout {
    let visible = row_count.min(MAX_SHOP_ROWS);
    let height = TITLE_HEIGHT
        + u32::try_from(visible)
            .unwrap_or(0)
            .saturating_mul(ROW_HEIGHT)
        + FOOTER_HEIGHT
        + PADDING
        + 4;
    let panel = Rect::new(
        8,
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
    ShopLayout {
        panel,
        rows,
        footer,
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
    let mut text = entry.name.clone();
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
        let Some(entry) = goods.list.get(index) else {
            break;
        };
        let highlighted = selected == Some(index);
        let hovered = hover == Some(index);
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
        let mut label = entry.name.clone();
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

    if goods.list.len() > layout.rows.len() {
        text.draw_right(
            frame,
            layout.panel.right() - 10,
            layout.panel.y
                + i32::try_from(
                    TITLE_HEIGHT + 2 + ROW_HEIGHT * u32::try_from(layout.rows.len()).unwrap_or(0),
                )
                .unwrap_or(0),
            &format!("+{}", goods.list.len() - layout.rows.len()),
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
        let layout = shop_layout(Rect::screen(800, 600), 5);
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
        let layout = shop_layout(Rect::screen(800, 600), 40);
        assert_eq!(layout.rows.len(), MAX_SHOP_ROWS);
        assert!(layout.panel.bottom() < 422, "商店不应压住聊天面板");
        assert!(layout.footer.y > layout.rows.last().unwrap().y);
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
