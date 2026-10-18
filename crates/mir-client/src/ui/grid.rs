//! 通用物品格子：个人仓库（80）、行会仓库（112）与写信页的附件选择共用。
//!
//! 布局是纯函数（[`grid_layout`]），绘制与命中测试共用同一份格子矩形，单元测试可以只验证
//! 矩形与命中。格子里的物品由调用方传入切片（`&[Option<InventoryItem>]`），所以同一份代码
//! 能画背包/仓库/行会仓库。
//!
//! 图标走 `mir-render` 的 `items.wzl` 精灵（`MapScene::item_icon`）；没有素材时在格子里画
//! 模板编号占位，与背包面板（`ui::inventory`）一致。

use mir_core::Point;
use mir_render::{Frame, Image, scale_to_fit};

use crate::geom::Rect;
use crate::net::{InventoryItem, WorldSnapshot};
use crate::ui::UiText;
use crate::ui::hud::{MUTED_COLOR, SIZE_SMALL};

/// 格子背景色。
pub const SLOT_BACKGROUND: [u8; 4] = [34, 40, 50, 235];
/// 空格子的边框色。
pub const SLOT_BORDER: [u8; 4] = [62, 72, 88, 255];
/// 悬停边框色。
pub const SLOT_HOVER_BORDER: [u8; 4] = [168, 208, 255, 255];
/// 选中边框色。
pub const SLOT_SELECTED_BORDER: [u8; 4] = [255, 216, 96, 255];
/// 拖拽起点边框色。
pub const SLOT_DRAG_BORDER: [u8; 4] = [255, 160, 80, 255];
/// 堆叠数量文字色。
pub const COUNT_COLOR: [u8; 4] = [255, 240, 180, 255];

/// 一块可滚动物品网格的布局。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridLayout {
    /// 网格外框（只包含格子，不含标题/金币行）。
    pub area: Rect,
    /// 可见格子的矩形（下标 `0` 对应 `first_slot`）。
    pub slots: Vec<Rect>,
    /// 第一条可见格子的绝对下标。
    pub first_slot: usize,
    /// 列数。
    pub columns: usize,
    /// 每行格数 × 行数（可见格子数）。
    pub visible: usize,
    /// 最大滚动行数（0 = 不需要滚动）。
    pub max_scroll: usize,
}

impl GridLayout {
    /// 命中测试：返回**绝对**格子下标。
    #[must_use]
    pub fn slot_at(&self, point: Point) -> Option<usize> {
        self.slots
            .iter()
            .position(|rect| rect.contains(point))
            .map(|index| self.first_slot + index)
    }
}

/// 计算物品网格布局。
///
/// * `area`：可放格子的区域（左上角对齐）；
/// * `total`：格子总数（仓库 80 / 行会仓库 112 / 背包 46）；
/// * `columns`：列数；
/// * `cell` / `gap`：格子边长与间距；
/// * `scroll`：滚动行数（会被夹到 `max_scroll`）。
#[must_use]
pub fn grid_layout(
    area: Rect,
    total: usize,
    columns: usize,
    cell: u32,
    gap: u32,
    scroll: usize,
) -> GridLayout {
    let columns = columns.max(1);
    let pitch = cell.saturating_add(gap);
    let rows = usize::try_from((area.height.saturating_add(gap)) / pitch.max(1)).unwrap_or(0);
    let rows = rows.max(1);
    let visible = rows.saturating_mul(columns).min(total.max(1));
    let total_rows = total.div_ceil(columns);
    let max_scroll = total_rows.saturating_sub(rows);
    let first_row = scroll.min(max_scroll);
    let first_slot = first_row.saturating_mul(columns);
    let mut slots = Vec::with_capacity(visible);
    for index in 0..visible {
        if first_slot + index >= total {
            break;
        }
        let column = index % columns;
        let row = index / columns;
        slots.push(Rect::new(
            area.x + i32::try_from(u32::try_from(column).unwrap_or(0) * pitch).unwrap_or(0),
            area.y + i32::try_from(u32::try_from(row).unwrap_or(0) * pitch).unwrap_or(0),
            cell,
            cell,
        ));
    }
    GridLayout {
        area,
        slots,
        first_slot,
        columns,
        visible,
        max_scroll,
    }
}

/// 画一块物品网格。
///
/// `items` 是与格子同序的物品切片；`drag` 是拖拽起点（画橙色框），`hover` 是鼠标悬停的
/// 绝对下标。
// 9 个参数把「画什么/画成什么样」都显式列出来，比再造一个只在这里用的结构体清楚
//（与 `ui::text::draw_clipped`、`ui::worldtext` 的既有写法一致）。
#[allow(clippy::too_many_arguments)]
pub fn draw(
    frame: &mut Frame,
    text: &UiText,
    layout: &GridLayout,
    items: &[Option<InventoryItem>],
    snapshot: &WorldSnapshot,
    selected: Option<usize>,
    hover: Option<usize>,
    drag: Option<usize>,
    icon: &mut dyn FnMut(u16) -> Option<Image>,
) {
    for (index, rect) in layout.slots.iter().enumerate() {
        let slot_index = layout.first_slot + index;
        let border = if drag == Some(slot_index) {
            SLOT_DRAG_BORDER
        } else if selected == Some(slot_index) {
            SLOT_SELECTED_BORDER
        } else if hover == Some(slot_index) {
            SLOT_HOVER_BORDER
        } else {
            SLOT_BORDER
        };
        frame.fill_rect(rect.x, rect.y, rect.width, rect.height, [0, 0, 0, 255]);
        frame.fill_rect(
            rect.x + 1,
            rect.y + 1,
            rect.width.saturating_sub(2),
            rect.height.saturating_sub(2),
            SLOT_BACKGROUND,
        );
        frame.fill_rect(rect.x, rect.y, rect.width, 1, border);
        frame.fill_rect(rect.x, rect.bottom() - 1, rect.width, 1, border);
        frame.fill_rect(rect.x, rect.y, 1, rect.height, border);
        frame.fill_rect(rect.right() - 1, rect.y, 1, rect.height, border);

        let Some(item) = items.get(slot_index).and_then(Option::as_ref) else {
            continue;
        };
        let loaded = snapshot.item_image(item.item_index).and_then(&mut *icon);
        if let Some(image) = loaded {
            let image = scale_to_fit(
                &image,
                rect.width.saturating_sub(4),
                rect.height.saturating_sub(6),
            );
            let x =
                rect.x + (i32::try_from(rect.width).unwrap_or(0) - i32::from(image.info.width)) / 2;
            let y = rect.y
                + (i32::try_from(rect.height).unwrap_or(0) - i32::from(image.info.height)) / 2;
            frame.blit(&image, x, y);
        } else {
            text.draw_clipped(
                frame,
                rect.x + 3,
                rect.y + 4,
                &item.item_index.to_string(),
                SIZE_SMALL,
                MUTED_COLOR,
                *rect,
            );
        }
        if item.count > 1 {
            text.draw_right(
                frame,
                rect.right() - 2,
                rect.bottom() - 13,
                &format!("×{}", item.count),
                SIZE_SMALL,
                COUNT_COLOR,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_slots_are_inside_the_area_and_hit_testable() {
        let area = Rect::new(10, 20, 340, 276);
        let layout = grid_layout(area, 80, 10, 30, 4, 0);
        assert_eq!(layout.columns, 10);
        assert_eq!(layout.first_slot, 0);
        assert_eq!(layout.visible, 80);
        assert_eq!(layout.max_scroll, 0);
        for (index, rect) in layout.slots.iter().enumerate() {
            assert!(area.contains(rect.center()));
            assert_eq!(layout.slot_at(rect.center()), Some(index));
        }
        assert_eq!(layout.slot_at(Point::new(0, 0)), None);
    }

    #[test]
    fn scrolling_moves_the_visible_window() {
        let area = Rect::new(0, 0, 8 * 30, 3 * 30);
        let layout = grid_layout(area, 112, 8, 26, 4, 1);
        assert_eq!(layout.max_scroll, 14 - 3);
        assert_eq!(layout.first_slot, 8);
        assert_eq!(layout.slots.len(), 24);
        assert_eq!(layout.slot_at(layout.slots[0].center()), Some(8));
        // 滚过头夹到最后一行。
        let clamped = grid_layout(area, 112, 8, 26, 4, 99);
        assert_eq!(clamped.first_slot, (14 - 3) * 8);
    }

    #[test]
    fn narrow_areas_still_produce_at_least_one_row() {
        let layout = grid_layout(Rect::new(0, 0, 100, 10), 46, 10, 30, 4, 0);
        assert_eq!(layout.slots.len(), 10);
        assert_eq!(layout.max_scroll, 4, "46 格 10 列 = 5 行，只显示 1 行");
    }
}
