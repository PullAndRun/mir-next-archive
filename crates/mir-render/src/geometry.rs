//! Crystal 的等距地图几何：每格 48×32 像素，铺地瓦片 96×64（2×2 格）。
//!
//! # 常量出处
//!
//! | 常量 | 值 | Crystal 出处 |
//! |---|---|---|
//! | [`CELL_WIDTH`] | 48 | `MapControl.CellWidth`，`GameScene.cs:10292` |
//! | [`CELL_HEIGHT`] | 32 | `MapControl.CellHeight`，`GameScene.cs:10293` |
//! | [`TILE_WIDTH`] | 96 | 后层瓦片宽度，由「只画偶数格 + 每格 48」推得（`GameScene.cs:10703`），实测 `tiles.wzl` 第 0 张是 96×64 |
//! | [`TILE_HEIGHT`] | 64 | 同上；实测 `tiles.wzl` 第 0 张是 96×64 |
//! | [`VIEW_MARGIN`] | 6 | `ViewRangeX = OffSetX + 6`、`ViewRangeY = OffSetY + 6`，`GameScene.cs:10401-10402` |
//! | [`OBJECT_ROW_MARGIN`] | 25 | 对象层向下多扫 25 行，`GameScene.cs:10798` |
//!
//! # 三个锚点
//!
//! Crystal 里同一个格子有三种屏幕坐标，本模块分别给出：
//!
//! ```
//! use mir_core::Point;
//! use mir_render::geometry::{self, Viewport};
//!
//! let viewport = Viewport::new(800, 600);
//! let center = Point::new(330, 270);
//! let cell = Point::new(330, 270);
//! assert_eq!(geometry::world_to_screen(viewport, center, cell), (384, 256));
//! // 铺地瓦片/前层图像的 x 再左移 OffSetX（GameScene.cs:10680、:10820）。
//! assert_eq!(geometry::tile_origin(viewport, center, cell), (376, 256));
//! // 高精灵的底边在下一格（GameScene.cs:10814）。
//! assert_eq!(geometry::sprite_anchor(viewport, center, cell), (376, 288));
//! ```
//!
//! * [`world_to_screen`] = `(OffSetX*48 + (x-cx)*48, OffSetY*32 + (y-cy)*32)`，
//!   与 `PlayerObject.DrawLocation`（`PlayerObject.cs:847`）和鼠标拾取
//!   `MapControl.MapLocation`（`GameScene.cs:10323`）一致，也是
//!   [`crate::MapScene::world_to_screen`] 的返回值。
//! * [`tile_origin`] = 格原点 `.x - OffSetX`，用于铺地瓦片左上角与高精灵的 `drawX`
//!   （两处公式相同，见 `GameScene.cs:10680` 与 `GameScene.cs:10820`）。
//! * [`sprite_anchor`] = `(tile_origin.x, 格原点.y + 32)`，高精灵以它为底边向上画
//!   （`GameScene.cs:10814` 的 `+ 1` 格与 `GameScene.cs:10820` 的 `- OffSetX`）。
//!
//! 所以角色（`DrawLocation`，格原点）与地图前层精灵（`sprite_anchor`）在 x 上相差
//! `OffSetX` 像素，这是 Crystal 自带的不一致，M6 原样移植。
//!
//! 注意 `- OffSetX` 与 `+1 格` 只在 Crystal 的渲染代码里出现，拾取公式（[`screen_to_cell`]）
//! 并不含它们；这是 Crystal 自带的偏差，M6 原样移植，见 crate 文档。

use std::ops::RangeInclusive;

use mir_core::Point;

/// 一格在屏幕上的宽度（`GameScene.cs:10292`）。
pub const CELL_WIDTH: i32 = 48;

/// 一格在屏幕上的高度（`GameScene.cs:10293`）。
pub const CELL_HEIGHT: i32 = 32;

/// 后层瓦片的宽度：2×2 格（`GameScene.cs:10703` 只在偶数 x/y 上画）。
pub const TILE_WIDTH: i32 = CELL_WIDTH * 2;

/// 后层瓦片的高度：2×2 格。
pub const TILE_HEIGHT: i32 = CELL_HEIGHT * 2;

/// `ViewRange` 相对 `OffSet` 的额外格数（`GameScene.cs:10401-10402`）。
pub const VIEW_MARGIN: i32 = 6;

/// 对象层向下多扫的行数（`GameScene.cs:10798` 的 `+ 25`）。
///
/// 高精灵的底边在格子锚点上、向上生长，所以屏幕下方若干行的精灵仍可能伸进画面，
/// Crystal 用固定的 25 行覆盖这个范围；地图库中最高的一批前层图（约 438 px）
/// 只需要 14 行，25 行是安全上限。
pub const OBJECT_ROW_MARGIN: i32 = 25;

/// 渲染视口（帧尺寸）。
///
/// `OffSetX` / `OffSetY` 由帧尺寸决定（`GameScene.cs:10398-10399`）：
///
/// ```text
/// OffSetX = width  / 2 / CELL_WIDTH
/// OffSetY = height / 2 / CELL_HEIGHT - 1
/// ```
///
/// 800×600 时是 `OffSetX = 8`、`OffSetY = 8`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Viewport {
    /// 帧宽（像素）。
    pub width: u32,
    /// 帧高（像素）。
    pub height: u32,
}

impl Viewport {
    /// 里程碑默认视口：800×600。
    pub const DEFAULT: Self = Self::new(800, 600);

    /// 新建视口。
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// `Settings.ScreenWidth / 2 / CellWidth`（`GameScene.cs:10398`）。
    #[must_use]
    pub const fn offset_x(self) -> i32 {
        (self.width as i32) / 2 / CELL_WIDTH
    }

    /// `Settings.ScreenHeight / 2 / CellHeight - 1`（`GameScene.cs:10399`）。
    #[must_use]
    pub const fn offset_y(self) -> i32 {
        (self.height as i32) / 2 / CELL_HEIGHT - 1
    }

    /// `ViewRangeX = OffSetX + 6`（`GameScene.cs:10401`）。
    #[must_use]
    pub const fn view_range_x(self) -> i32 {
        self.offset_x() + VIEW_MARGIN
    }

    /// `ViewRangeY = OffSetY + 6`（`GameScene.cs:10402`）。
    #[must_use]
    pub const fn view_range_y(self) -> i32 {
        self.offset_y() + VIEW_MARGIN
    }

    /// 中心格（`center`）的格原点：`(OffSetX * 48, OffSetY * 32)`。
    #[must_use]
    pub const fn center_origin(self) -> (i32, i32) {
        (self.offset_x() * CELL_WIDTH, self.offset_y() * CELL_HEIGHT)
    }
}

impl Default for Viewport {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// 地图坐标 → 屏幕坐标（格原点）。
///
/// 对应 `PlayerObject.DrawLocation`（`PlayerObject.cs:847`）与
/// `MapControl.ToMouseLocation`（`GameScene.cs:10326-10329`）：
///
/// ```text
/// screen.x = (cell.x - center.x + OffSetX) * 48
/// screen.y = (cell.y - center.y + OffSetY) * 32
/// ```
#[must_use]
pub fn world_to_screen(viewport: Viewport, center: Point, cell: Point) -> (i32, i32) {
    let (origin_x, origin_y) = viewport.center_origin();
    (
        origin_x + (cell.x - center.x) * CELL_WIDTH,
        origin_y + (cell.y - center.y) * CELL_HEIGHT,
    )
}

/// 铺地瓦片左上角 / 高精灵的 `drawX`：格原点再左移 `OffSetX`
/// （`GameScene.cs:10680`、`GameScene.cs:10820`）。
#[must_use]
pub fn tile_origin(viewport: Viewport, center: Point, cell: Point) -> (i32, i32) {
    let (x, y) = world_to_screen(viewport, center, cell);
    (x - viewport.offset_x(), y)
}

/// 高精灵的底边锚点：`x` 与铺地瓦片相同，`y` 是格原点下移一格
/// （`GameScene.cs:10814` 的 `+ 1` 与 `GameScene.cs:10820` 的 `- OffSetX`）。
///
/// 前层/中层图像绘制时是从这个 y 减去图像高度得到的顶部
/// （`MLibrary.cs:809` 的 `y -= mi.Height`、`GameScene.cs:10935`）。
#[must_use]
pub fn sprite_anchor(viewport: Viewport, center: Point, cell: Point) -> (i32, i32) {
    let (x, _) = tile_origin(viewport, center, cell);
    let (_, y) = world_to_screen(viewport, center, cell);
    (x, y + CELL_HEIGHT)
}

/// 屏幕坐标 → 地图坐标，鼠标拾取用。
///
/// 逐行移植 `MapControl.MapLocation`（`GameScene.cs:10321-10324`）：
///
/// ```text
/// cell.x = mouse.x / 48 - OffSetX + center.x
/// cell.y = mouse.y / 32 - OffSetY + center.y
/// ```
///
/// 除法与 C# 一样向零取整（`-1 / 48 == 0`）。
#[must_use]
pub fn screen_to_cell(viewport: Viewport, center: Point, screen: Point) -> Point {
    Point::new(
        screen.x / CELL_WIDTH - viewport.offset_x() + center.x,
        screen.y / CELL_HEIGHT - viewport.offset_y() + center.y,
    )
}

/// 一帧需要遍历的格子范围（闭区间，可能越出地图边界，调用方用 `Map::cell` 过滤）。
///
/// * x：`center.x ± ViewRangeX`（`GameScene.cs:10671-10674`、`:10802`）；
/// * y：上方多 1 行补偿菱形瓦片的上半部分，下方多 `ViewRangeY + 25` 行，
///   与对象层循环的上界一致（`GameScene.cs:10798`）。
#[must_use]
pub fn cell_bounds(
    viewport: Viewport,
    center: Point,
) -> (RangeInclusive<i32>, RangeInclusive<i32>) {
    let range_x = viewport.view_range_x();
    let range_y = viewport.view_range_y();
    (
        center.x - range_x..=center.x + range_x,
        center.y - range_y - 1..=center.y + range_y + OBJECT_ROW_MARGIN,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_offsets_match_crystal() {
        let viewport = Viewport::new(800, 600);
        assert_eq!(viewport.offset_x(), 8); // 800 / 2 / 48
        assert_eq!(viewport.offset_y(), 8); // 600 / 2 / 32 - 1
        assert_eq!(viewport.view_range_x(), 14);
        assert_eq!(viewport.view_range_y(), 14);
        assert_eq!(viewport.center_origin(), (384, 256));

        // 1920×1080：OffSetX = 20，OffSetY = 1080/2/32 - 1 = 15。
        let full_hd = Viewport::new(1920, 1080);
        assert_eq!(full_hd.offset_x(), 20);
        assert_eq!(full_hd.offset_y(), 15);
    }

    #[test]
    fn world_to_screen_uses_cell_origin() {
        let viewport = Viewport::new(800, 600);
        let center = Point::new(330, 270);
        // OffSetX * 48 = 384、OffSetY * 32 = 256（PlayerObject.cs:847）。
        assert_eq!(world_to_screen(viewport, center, center), (384, 256));
    }

    #[test]
    fn relative_cells_move_by_48_and_32() {
        let viewport = Viewport::new(800, 600);
        let center = Point::new(330, 270);
        let origin = world_to_screen(viewport, center, center);
        assert_eq!(origin, (384, 256));
        assert_eq!(
            world_to_screen(viewport, center, Point::new(331, 270)),
            (origin.0 + CELL_WIDTH, origin.1)
        );
        assert_eq!(
            world_to_screen(viewport, center, Point::new(330, 271)),
            (origin.0, origin.1 + CELL_HEIGHT)
        );
        assert_eq!(
            world_to_screen(viewport, center, Point::new(329, 269)),
            (origin.0 - CELL_WIDTH, origin.1 - CELL_HEIGHT)
        );
    }

    #[test]
    fn tile_and_sprite_anchors_differ_as_crystal() {
        let viewport = Viewport::new(800, 600);
        let center = Point::new(330, 270);
        let cell = Point::new(330, 270);
        // 格原点。
        assert_eq!(world_to_screen(viewport, center, cell), (384, 256));
        // 瓦片/前层 x 左移 OffSetX。
        assert_eq!(tile_origin(viewport, center, cell), (376, 256));
        // 精灵 x 与瓦片相同、底边下移一格。
        assert_eq!(sprite_anchor(viewport, center, cell), (376, 288));
    }

    #[test]
    fn screen_to_cell_inverts_the_object_origin() {
        let viewport = Viewport::new(800, 600);
        let center = Point::new(330, 270);
        let cell = Point::new(335, 265);
        let (x, y) = world_to_screen(viewport, center, cell);
        // 格原点落在格内时，拾取得到同一格。
        for (dx, dy) in [(0, 0), (47, 31), (24, 16)] {
            assert_eq!(
                screen_to_cell(viewport, center, Point::new(x + dx, y + dy)),
                cell
            );
        }
        // 屏幕中心格：400/48 = 8，300/32 = 9（向零取整）。
        assert_eq!(
            screen_to_cell(viewport, center, Point::new(400, 300)),
            Point::new(330, 271)
        );
        // 负数坐标也向零取整（与 C# 相同）。
        assert_eq!(
            screen_to_cell(viewport, center, Point::new(-1, -1)),
            Point::new(322, 262)
        );
    }

    #[test]
    fn cell_bounds_cover_view_range_and_tall_sprites() {
        let viewport = Viewport::new(800, 600);
        let center = Point::new(330, 270);
        let (xs, ys) = cell_bounds(viewport, center);
        assert_eq!(xs.start(), &316);
        assert_eq!(xs.end(), &344);
        assert_eq!(ys.start(), &255); // 270 - 14 - 1
        assert_eq!(ys.end(), &309); // 270 + 14 + 25
    }
}
