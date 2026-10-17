//! 背包面板：46 格网格、物品图标、堆叠数量、选中/悬停高亮与金币。
//!
//! 布局是纯函数（[`inventory_panel_rect`] / [`inventory_slot_rect`] /
//! [`inventory_slot_at`]），绘制与鼠标命中测试共用，单元测试可以只验证矩形。
//! 图标走 `mir-render` 的物品精灵路径（`items.wzl`）；没有素材时用物品模板号占位。

use mir_render::{Frame, Image, scale_to_fit};

use crate::geom::Rect;
use crate::ui::PanelContext;
use crate::ui::hud::{
    BODY_COLOR, GOLD_COLOR, MUTED_COLOR, SIZE_BODY, SIZE_SMALL, STATUS_HEIGHT, TITLE_COLOR,
};
use crate::ui::labels::{self, Label};

/// 背包格子数（Crystal `Globals.InventorySize = 46`）。
pub const INVENTORY_SLOTS: usize = 46;
/// 每行格子数。
pub const INVENTORY_COLUMNS: usize = 6;
/// 单个格子的边长。
pub const SLOT_SIZE: u32 = 38;
/// 格子间距。
pub const SLOT_GAP: u32 = 4;
/// 面板内边距。
pub const PANEL_PADDING: u32 = 10;
/// 标题高度。
pub const PANEL_TITLE_HEIGHT: u32 = 24;
/// 金币行高度。
pub const PANEL_GOLD_HEIGHT: u32 = 20;
/// 操作提示行高度。
pub const PANEL_HINT_HEIGHT: u32 = 16;
/// 物品图标的显示上限（像素，超出按比例缩到格子里）。
pub const MAX_ICON_SIZE: u32 = 34;
/// 双击判定窗口（毫秒）。
pub const DOUBLE_CLICK_MS: i64 = 350;
/// 右键菜单宽度。
pub const MENU_WIDTH: u32 = 104;
/// 右键菜单每项高度。
pub const MENU_ENTRY_HEIGHT: u32 = 22;
/// 右键菜单内边距。
pub const MENU_PADDING: u32 = 3;
/// 拖拽幽灵图标的边长。
pub const GHOST_SIZE: i32 = 36;
/// 拖拽幽灵图标的不透明度（0..=255）。
pub const GHOST_ALPHA: u32 = 160;

/// 面板宽度。
#[must_use]
pub const fn panel_width() -> u32 {
    PANEL_PADDING * 2
        + INVENTORY_COLUMNS as u32 * SLOT_SIZE
        + (INVENTORY_COLUMNS as u32 - 1) * SLOT_GAP
}

/// 网格行数。
#[must_use]
pub const fn grid_rows() -> u32 {
    (INVENTORY_SLOTS as u32).div_ceil(INVENTORY_COLUMNS as u32)
}

/// 网格高度。
#[must_use]
pub const fn grid_height() -> u32 {
    grid_rows() * SLOT_SIZE + (grid_rows() - 1) * SLOT_GAP
}

/// 面板高度。
#[must_use]
pub const fn panel_height() -> u32 {
    PANEL_TITLE_HEIGHT + PANEL_PADDING + grid_height() + PANEL_PADDING + PANEL_HINT_HEIGHT + 6
}

/// 背包面板矩形（右侧，状态栏下方）。
#[must_use]
pub fn inventory_panel_rect(frame: Rect) -> Rect {
    let width = panel_width();
    let height = panel_height();
    let x = frame.width.saturating_sub(width + 8) as i32;
    let preferred = 60;
    let lowest = frame.height as i32 - height as i32 - 8;
    let highest = STATUS_HEIGHT as i32 + 8;
    let y = preferred.min(lowest.max(highest));
    Rect::new(x, y, width, height)
}

/// 第 `index` 个格子的矩形；越界返回 `None`。
#[must_use]
pub fn inventory_slot_rect(panel: Rect, index: usize) -> Option<Rect> {
    if index >= INVENTORY_SLOTS {
        return None;
    }
    let columns = INVENTORY_COLUMNS as u32;
    let column = index as u32 % columns;
    let row = index as u32 / columns;
    let x = panel.x
        + i32::try_from(PANEL_PADDING).unwrap_or(0)
        + i32::try_from(column * (SLOT_SIZE + SLOT_GAP)).unwrap_or(0);
    let y = panel.y
        + i32::try_from(PANEL_TITLE_HEIGHT + PANEL_PADDING).unwrap_or(0)
        + i32::try_from(row * (SLOT_SIZE + SLOT_GAP)).unwrap_or(0);
    Some(Rect::new(x, y, SLOT_SIZE, SLOT_SIZE))
}

/// 命中测试：点在哪个格子上。
#[must_use]
pub fn inventory_slot_at(panel: Rect, point: mir_core::Point) -> Option<usize> {
    (0..INVENTORY_SLOTS)
        .find(|index| inventory_slot_rect(panel, *index).is_some_and(|rect| rect.contains(point)))
}

/// 金币显示区域（与标题同一行，右半边）。
#[must_use]
pub fn inventory_gold_area(panel: Rect) -> Rect {
    Rect::new(
        panel.x + i32::try_from(panel.width / 2).unwrap_or(0),
        panel.y + 4,
        panel.width / 2 - PANEL_PADDING,
        18,
    )
}

/// 操作提示区域。
#[must_use]
pub fn inventory_hint_area(panel: Rect) -> Rect {
    Rect::new(
        panel.x + i32::try_from(PANEL_PADDING).unwrap_or(0),
        panel.bottom() - i32::try_from(PANEL_HINT_HEIGHT + 4).unwrap_or(0),
        panel.width.saturating_sub(PANEL_PADDING * 2),
        PANEL_HINT_HEIGHT,
    )
}

/// 画背包面板。
///
/// `icon` 是「物品模板号 → 图标」的取图回调，由 `UiState` 的缓存 + `MapScene` 提供；
/// 返回 `None` 时在格子里画模板号占位。
pub fn draw(
    frame: &mut Frame,
    panel: Rect,
    context: &PanelContext<'_>,
    selected: Option<usize>,
    hover: Option<usize>,
    icon: &mut dyn FnMut(u16) -> Option<Image>,
) {
    let snapshot = context.snapshot;
    let text = context.text;
    let language = context.language;
    crate::ui::draw_panel(frame, panel);
    text.draw_clipped(
        frame,
        panel.x + 10,
        panel.y + 4,
        &format!("{} (I)", Label::Inventory.text(language)),
        SIZE_BODY,
        TITLE_COLOR,
        Rect::new(panel.x, panel.y, panel.width, PANEL_TITLE_HEIGHT),
    );

    for index in 0..INVENTORY_SLOTS {
        let Some(rect) = inventory_slot_rect(panel, index) else {
            continue;
        };
        let highlighted = selected == Some(index);
        let hovered = hover == Some(index);
        let border = if highlighted {
            [255, 216, 96, 255]
        } else if hovered {
            [168, 208, 255, 255]
        } else {
            [62, 72, 88, 255]
        };
        frame.fill_rect(rect.x, rect.y, rect.width, rect.height, [0, 0, 0, 255]);
        frame.fill_rect(
            rect.x + 1,
            rect.y + 1,
            rect.width.saturating_sub(2),
            rect.height.saturating_sub(2),
            [34, 40, 50, 235],
        );
        draw_border(frame, rect, border);

        let Some(item) = snapshot.inventory_item(index) else {
            continue;
        };
        let image = snapshot.item_image(item.item_index);
        let loaded = match image {
            Some(image) => icon(image),
            None => None,
        };
        if let Some(image) = loaded {
            let image = scale_to_fit(&image, SLOT_SIZE - 4, SLOT_SIZE - 8);
            let (x, y) = centered(rect, image.info.width, image.info.height);
            frame.blit(&image, x, y);
        } else {
            text.draw_clipped(
                frame,
                rect.x + 4,
                rect.y + 10,
                &item.item_index.to_string(),
                SIZE_SMALL,
                MUTED_COLOR,
                rect,
            );
        }
        if item.count > 1 {
            text.draw_right(
                frame,
                rect.right() - 3,
                rect.bottom() - 15,
                &format!("×{}", item.count),
                SIZE_SMALL,
                [255, 240, 180, 255],
            );
        }
        draw_durability(frame, rect, item.current_dura, item.max_dura);
    }

    let gold_area = inventory_gold_area(panel);
    text.draw_right(
        frame,
        gold_area.right(),
        gold_area.y,
        &labels::gold_text(
            language,
            snapshot.self_player.as_ref().map_or(0, |me| me.gold),
        ),
        SIZE_BODY,
        GOLD_COLOR,
    );
    let hint_area = inventory_hint_area(panel);
    text.draw_clipped(
        frame,
        hint_area.x,
        hint_area.y,
        Label::InventoryTooltipHint.text(language),
        SIZE_SMALL,
        MUTED_COLOR,
        hint_area,
    );
    if let Some(selected) = selected {
        if let Some(item) = snapshot.inventory_item(selected) {
            let name = snapshot.item_info(item.item_index).map_or_else(
                || labels::unknown_item(language, item.item_index),
                |info| info.name.clone(),
            );
            text.draw_right(
                frame,
                hint_area.right(),
                hint_area.y,
                &name,
                SIZE_SMALL,
                BODY_COLOR,
            );
        }
    }
}

/// 把 `width×height` 的图居中放进格子，必要时不做缩放（只保证不左上溢出）。
fn centered(rect: Rect, width: u16, height: u16) -> (i32, i32) {
    let x = rect.x + (i32::try_from(rect.width).unwrap_or(0) - i32::from(width)) / 2;
    let y = rect.y + (i32::try_from(rect.height).unwrap_or(0) - i32::from(height)) / 2;
    (x, y)
}

/// 右键菜单矩形：以鼠标为锚点，贴边时翻到另一侧。
#[must_use]
pub fn context_menu_rect(anchor: mir_core::Point, entries: usize, screen: Rect) -> Rect {
    let height = MENU_PADDING * 2
        + u32::try_from(entries)
            .unwrap_or(0)
            .saturating_mul(MENU_ENTRY_HEIGHT);
    let width = MENU_WIDTH;
    let mut x = anchor.x + 4;
    let mut y = anchor.y + 4;
    if x + i32::try_from(width).unwrap_or(0) > screen.right() - 4 {
        x = anchor.x - i32::try_from(width).unwrap_or(0) - 4;
    }
    if y + i32::try_from(height).unwrap_or(0) > screen.bottom() - 4 {
        y = anchor.y - i32::try_from(height).unwrap_or(0) - 4;
    }
    Rect::new(x.max(4), y.max(4), width, height)
}

/// 右键菜单第 `index` 项的矩形。
#[must_use]
pub fn context_menu_entry_rect(panel: Rect, index: usize) -> Rect {
    Rect::new(
        panel.x + i32::try_from(MENU_PADDING).unwrap_or(0),
        panel.y
            + i32::try_from(MENU_PADDING).unwrap_or(0)
            + i32::try_from(index).unwrap_or(0) * i32::try_from(MENU_ENTRY_HEIGHT).unwrap_or(0),
        panel.width.saturating_sub(MENU_PADDING * 2),
        MENU_ENTRY_HEIGHT,
    )
}

/// 右键菜单命中测试。
#[must_use]
pub fn context_menu_entry_at(panel: Rect, entries: usize, point: mir_core::Point) -> Option<usize> {
    (0..entries).find(|index| context_menu_entry_rect(panel, *index).contains(point))
}

/// 画右键菜单（使用/装备/丢弃）。
pub fn draw_context_menu(
    frame: &mut Frame,
    text: &crate::ui::UiText,
    panel: Rect,
    entries: &[crate::ui::ContextEntry],
    hover: Option<usize>,
) {
    crate::ui::draw_panel(frame, panel);
    for (index, entry) in entries.iter().enumerate() {
        let rect = context_menu_entry_rect(panel, index);
        if hover == Some(index) {
            frame.fill_rect(rect.x, rect.y, rect.width, rect.height, [56, 68, 92, 245]);
        }
        text.draw_clipped(
            frame,
            rect.x + 8,
            rect.y + 3,
            &entry.label,
            SIZE_BODY,
            BODY_COLOR,
            rect,
        );
    }
}

/// 画拖拽幽灵：半透明物品图标跟随鼠标，右下角带堆叠数量。
pub fn draw_drag_ghost(
    frame: &mut Frame,
    text: &crate::ui::UiText,
    image: Option<&Image>,
    count: u16,
    mouse: mir_core::Point,
) {
    let x = mouse.x - GHOST_SIZE / 2;
    let y = mouse.y - GHOST_SIZE / 2;
    frame.fill_rect_alpha(
        x - 2,
        y - 2,
        u32::try_from(GHOST_SIZE + 4).unwrap_or(0),
        u32::try_from(GHOST_SIZE + 4).unwrap_or(0),
        [20, 26, 38, 120],
    );
    if let Some(image) = image {
        let scaled = scale_to_fit(image, MAX_ICON_SIZE, MAX_ICON_SIZE);
        let mut faded = scaled.clone();
        for pixel in faded.pixels.chunks_exact_mut(4) {
            pixel[3] = u8::try_from(u32::from(pixel[3]) * GHOST_ALPHA / 255).unwrap_or(pixel[3]);
        }
        let icon_x = x + (GHOST_SIZE - i32::from(faded.info.width)) / 2;
        let icon_y = y + (GHOST_SIZE - i32::from(faded.info.height)) / 2;
        frame.blit(&faded, icon_x, icon_y);
    }
    if count > 1 {
        text.draw_right(
            frame,
            x + GHOST_SIZE,
            y + GHOST_SIZE - 14,
            &format!("×{count}"),
            SIZE_SMALL,
            [255, 240, 180, 255],
        );
    }
}

/// 画格子的四条边。
fn draw_border(frame: &mut Frame, rect: Rect, color: [u8; 4]) {
    frame.fill_rect(rect.x, rect.y, rect.width, 1, color);
    frame.fill_rect(rect.x, rect.bottom() - 1, rect.width, 1, color);
    frame.fill_rect(rect.x, rect.y, 1, rect.height, color);
    frame.fill_rect(rect.right() - 1, rect.y, 1, rect.height, color);
}

/// 持久度条（只在有持久度上限时画）。
fn draw_durability(frame: &mut Frame, rect: Rect, current: u16, maximum: u16) {
    if maximum == 0 {
        return;
    }
    let width = rect.width.saturating_sub(6);
    let ratio = f32::from(current.min(maximum)) / f32::from(maximum);
    let filled = width * u32::from(current.min(maximum)) / u32::from(maximum);
    let color = if ratio > 0.5 {
        [96, 208, 96, 255]
    } else if ratio > 0.2 {
        [240, 208, 72, 255]
    } else {
        [232, 72, 72, 255]
    };
    let y = rect.bottom() - 4;
    frame.fill_rect(rect.x + 3, y, width, 2, [0, 0, 0, 255]);
    if filled > 0 {
        frame.fill_rect(rect.x + 3, y, filled, 2, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir_core::Point;

    #[test]
    fn slot_rects_are_inside_the_panel_and_do_not_overlap() {
        let panel = inventory_panel_rect(Rect::screen(800, 600));
        assert!(panel.x > 0);
        assert!(panel.bottom() <= 600);
        let first = inventory_slot_rect(panel, 0).expect("0 号格");
        let last = inventory_slot_rect(panel, INVENTORY_SLOTS - 1).expect("45 号格");
        assert!(panel.contains(first.center()));
        assert!(panel.contains(last.center()));
        assert!(inventory_slot_rect(panel, INVENTORY_SLOTS).is_none());
        assert!(first.intersect(last).is_none());
        // 6 列：第 5 个与第 6 个同排，第 6 个换行。
        let sixth = inventory_slot_rect(panel, 5).unwrap();
        let seventh = inventory_slot_rect(panel, 6).unwrap();
        assert_eq!(sixth.y, first.y);
        assert!(seventh.y > sixth.y);
        assert_eq!(seventh.x, first.x);
    }

    #[test]
    fn slot_hit_testing_matches_the_rects() {
        let panel = inventory_panel_rect(Rect::screen(800, 600));
        for index in [0usize, 5, 7, 23, 45] {
            let rect = inventory_slot_rect(panel, index).unwrap();
            assert_eq!(inventory_slot_at(panel, rect.center()), Some(index));
        }
        assert_eq!(
            inventory_slot_at(panel, Point::new(panel.x - 1, panel.y)),
            None
        );
        // 格子之间的缝不命中任何格。
        let gap = inventory_slot_rect(panel, 0).unwrap().right() + 1;
        assert_eq!(
            inventory_slot_at(
                panel,
                Point::new(gap, inventory_slot_rect(panel, 0).unwrap().y)
            ),
            None
        );
    }

    #[test]
    fn panel_rect_stays_below_the_status_bar() {
        let panel = inventory_panel_rect(Rect::screen(800, 600));
        assert!(panel.y >= STATUS_HEIGHT as i32);
        let narrow = inventory_panel_rect(Rect::screen(320, 240));
        assert!(narrow.x >= 0);
        assert_eq!(narrow.width, panel_width());
    }

    #[test]
    fn gold_and_hint_areas_are_inside_the_panel() {
        let panel = inventory_panel_rect(Rect::screen(800, 600));
        assert!(panel.contains(inventory_gold_area(panel).center()));
        assert!(panel.contains(inventory_hint_area(panel).center()));
    }
}
