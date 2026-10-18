//! 个人仓库面板（服务端 `S.NPCStorage` 到达时自动打开，Crystal `StorageDialog`）。
//!
//! Crystal 的仓库面板在 NPC 脚本页 `[@STORAGE]` 里打开：服务端先发一次 `S.UserStorage`
//! （80 格物品列表，`PlayerObject.cs:8169-8180`）再发 `S.NPCStorage`
//! （`NPCScript.cs:1002-1006`）。本作按同样顺序：`S.NPCStorage` 让快照把
//! [`crate::net::WorldSnapshot::storage_open`] 置真，`UiState::sync` 据此自动开面板。
//!
//! 交互：与背包之间**拖拽**存取。
//! * 背包格 → 仓库格：`C.StoreItem { from: 背包格, to: 仓库格 }`；
//! * 仓库格 → 背包格：`C.TakeBackItem { from: 仓库格, to: 背包格 }`；
//! * 仓库内部没有「移动」包（Crystal 的 `Account.Storage` 只有存/取两条路径）。
//!
//! 简化：
//! * Crystal 的仓库面板是 10×16 的滚动网格（`NPCDialogs.cs:2935`），其中只有前 80 格可用
//!   （`Globals.StorageGridSize = 80`）；本作没有扩充仓库，直接画 10×8 全 80 格，不滚动；
//! * Crystal 的仓库**没有金币功能**（`storage.rs` 模块文档差异 2），这里只显示物品与容量；
//! * 面板位置贴在背包左边（Crystal 的仓库是独立可拖动窗口，位置见 `NPCDialogs.cs:2798`）。

use mir_core::Point;
use mir_render::{Frame, Image};

use crate::geom::Rect;
use crate::net::{STORAGE_SLOTS, WorldSnapshot};
use crate::ui::hud::{GOLD_COLOR, MUTED_COLOR, SIZE_BODY, SIZE_SMALL, TITLE_COLOR};
use crate::ui::inventory;
use crate::ui::labels::Label;
use crate::ui::{PanelContext, grid};

/// 面板内边距。
pub const PADDING: u32 = 8;
/// 标题高度。
pub const TITLE_HEIGHT: u32 = 24;
/// 容量行高度。
pub const CAPACITY_HEIGHT: u32 = 17;
/// 底部提示高度。
pub const FOOTER_HEIGHT: u32 = 17;
/// 每格边长。
pub const CELL_SIZE: u32 = 30;
/// 格子间距。
pub const CELL_GAP: u32 = 4;
/// 网格列数（Crystal `StorageDialog` 的 10 列）。
pub const COLUMNS: usize = 10;
/// 网格行数（80 / 10）。
pub const ROWS: usize = STORAGE_SLOTS / COLUMNS;

/// 仓库面板布局。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageLayout {
    /// 面板矩形。
    pub panel: Rect,
    /// 格子网格。
    pub grid: grid::GridLayout,
    /// 容量行。
    pub capacity: Rect,
    /// 底部提示。
    pub footer: Rect,
}

/// 面板宽度。
#[must_use]
pub const fn panel_width() -> u32 {
    PADDING * 2 + COLUMNS as u32 * CELL_SIZE + (COLUMNS as u32 - 1) * CELL_GAP
}

/// 面板高度。
#[must_use]
pub const fn panel_height() -> u32 {
    TITLE_HEIGHT
        + CAPACITY_HEIGHT
        + ROWS as u32 * CELL_SIZE
        + (ROWS as u32 - 1) * CELL_GAP
        + FOOTER_HEIGHT
        + PADDING
        + 4
}

/// 计算仓库面板布局：贴在背包面板左边、状态栏下方。
#[must_use]
pub fn storage_layout(frame: Rect) -> StorageLayout {
    let width = panel_width().min(frame.width.saturating_sub(16)).max(180);
    let height = panel_height();
    let inventory = inventory::inventory_panel_rect(frame);
    let x = (inventory.x - i32::try_from(width).unwrap_or(0) - 8).max(8);
    let panel = Rect::new(x, 60, width, height);
    let grid_area = Rect::new(
        panel.x + i32::try_from(PADDING).unwrap_or(0),
        panel.y + i32::try_from(TITLE_HEIGHT + CAPACITY_HEIGHT).unwrap_or(0),
        width.saturating_sub(PADDING * 2),
        u32::try_from(ROWS).unwrap_or(0) * CELL_SIZE
            + u32::try_from(ROWS.saturating_sub(1)).unwrap_or(0) * CELL_GAP,
    );
    let grid = grid::grid_layout(grid_area, STORAGE_SLOTS, COLUMNS, CELL_SIZE, CELL_GAP, 0);
    let capacity = Rect::new(
        panel.x + i32::try_from(PADDING).unwrap_or(0),
        panel.y + i32::try_from(TITLE_HEIGHT).unwrap_or(0),
        width.saturating_sub(PADDING * 2),
        CAPACITY_HEIGHT,
    );
    let footer = Rect::new(
        panel.x + i32::try_from(PADDING).unwrap_or(0),
        panel.bottom() - i32::try_from(FOOTER_HEIGHT + 4).unwrap_or(0),
        width.saturating_sub(PADDING * 2),
        FOOTER_HEIGHT,
    );
    StorageLayout {
        panel,
        grid,
        capacity,
        footer,
    }
}

/// 仓库的已用格数。
#[must_use]
pub fn used_slots(snapshot: &WorldSnapshot) -> usize {
    snapshot
        .storage
        .iter()
        .filter(|slot| slot.is_some())
        .count()
}

/// 容量文案：`容量 12/80`。
#[must_use]
pub fn capacity_text(snapshot: &WorldSnapshot, language: mir_core::Language) -> String {
    format!(
        "{} {}/{}",
        Label::Capacity.text(language),
        used_slots(snapshot),
        STORAGE_SLOTS
    )
}

/// 命中测试：鼠标下的仓库格。
#[must_use]
pub fn slot_at(layout: &StorageLayout, point: Point) -> Option<usize> {
    layout.grid.slot_at(point)
}

/// 画仓库面板。
pub fn draw(
    frame: &mut Frame,
    layout: &StorageLayout,
    context: &PanelContext<'_>,
    hover: Option<usize>,
    drag: Option<usize>,
    icon: &mut dyn FnMut(u16) -> Option<Image>,
) {
    let text = context.text;
    let language = context.language;
    let snapshot = context.snapshot;
    crate::ui::draw_panel(frame, layout.panel);
    text.draw_clipped(
        frame,
        layout.panel.x + 10,
        layout.panel.y + 4,
        Label::Storage.text(language),
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
        layout.capacity.right(),
        layout.capacity.y,
        &capacity_text(snapshot, language),
        SIZE_SMALL,
        GOLD_COLOR,
    );
    grid::draw(
        frame,
        text,
        &layout.grid,
        &snapshot.storage,
        snapshot,
        None,
        hover,
        drag,
        icon,
    );
    text.draw_clipped(
        frame,
        layout.footer.x,
        layout.footer.y,
        Label::StorageHint.text(language),
        SIZE_SMALL,
        MUTED_COLOR,
        layout.footer,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_panel_holds_eighty_hit_testable_slots() {
        let frame = Rect::screen(800, 600);
        let layout = storage_layout(frame);
        assert_eq!(layout.grid.slots.len(), STORAGE_SLOTS);
        assert_eq!(layout.grid.max_scroll, 0);
        assert!(layout.panel.x >= 0);
        assert!(layout.panel.bottom() <= 600);
        let last = layout.grid.slots[STORAGE_SLOTS - 1];
        assert!(layout.panel.contains(last.center()));
        assert_eq!(slot_at(&layout, last.center()), Some(STORAGE_SLOTS - 1));
        assert_eq!(slot_at(&layout, Point::new(0, 0)), None);
    }

    #[test]
    fn storage_panel_sits_left_of_the_inventory() {
        let frame = Rect::screen(800, 600);
        let layout = storage_layout(frame);
        let inventory = inventory::inventory_panel_rect(frame);
        assert!(layout.panel.right() <= inventory.x);
        assert!(layout.panel.intersect(inventory).is_none());
    }
}
