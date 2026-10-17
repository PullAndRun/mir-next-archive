//! 界面用的小几何类型：屏幕像素矩形与命中测试。
//!
//! 只有 `mir-client` 的 UI 需要它；地图坐标仍用 `mir_core::Point`。
//! 所有面板布局函数都返回 [`Rect`]，命中测试与绘制共用同一份布局，避免「看到的
//! 按钮」和「点得到的按钮」错位。

use mir_core::Point;

/// 屏幕像素矩形（左上角 +宽高，右下角为开区间）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Rect {
    /// 左边界（像素）。
    pub x: i32,
    /// 上边界（像素）。
    pub y: i32,
    /// 宽（像素）。
    pub width: u32,
    /// 高（像素）。
    pub height: u32,
}

impl Rect {
    /// 建一个矩形。
    #[must_use]
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// 整帧矩形。
    #[must_use]
    pub const fn screen(width: u32, height: u32) -> Self {
        Self::new(0, 0, width, height)
    }

    /// 右边界（不含）。
    #[must_use]
    pub const fn right(self) -> i32 {
        self.x.saturating_add(self.width as i32)
    }

    /// 下边界（不含）。
    #[must_use]
    pub const fn bottom(self) -> i32 {
        self.y.saturating_add(self.height as i32)
    }

    /// 是否包含某个像素坐标。
    #[must_use]
    pub fn contains_xy(self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    /// 是否包含一个点。
    #[must_use]
    pub fn contains(self, point: Point) -> bool {
        self.contains_xy(point.x, point.y)
    }

    /// 内缩 `padding` 像素（宽高不足时退化为空矩形）。
    #[must_use]
    pub fn inset(self, padding: i32) -> Self {
        let width = self
            .width
            .saturating_sub(u32::try_from(padding.max(0) * 2).unwrap_or(0));
        let height = self
            .height
            .saturating_sub(u32::try_from(padding.max(0) * 2).unwrap_or(0));
        Self {
            x: self.x.saturating_add(padding),
            y: self.y.saturating_add(padding),
            width,
            height,
        }
    }

    /// 水平中心。
    #[must_use]
    pub const fn center_x(self) -> i32 {
        self.x + self.width as i32 / 2
    }

    /// 垂直中心。
    #[must_use]
    pub const fn center_y(self) -> i32 {
        self.y + self.height as i32 / 2
    }

    /// 中心点。
    #[must_use]
    pub fn center(self) -> Point {
        Point::new(self.center_x(), self.center_y())
    }

    /// 两个矩形的交集；没有交集时返回 `None`。
    #[must_use]
    pub fn intersect(self, other: Self) -> Option<Self> {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        if left >= right || top >= bottom {
            return None;
        }
        Some(Self::new(
            left,
            top,
            u32::try_from(right - left).unwrap_or(0),
            u32::try_from(bottom - top).unwrap_or(0),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_uses_half_open_bounds() {
        let rect = Rect::new(10, 20, 30, 40);
        assert!(rect.contains(Point::new(10, 20)));
        assert!(rect.contains(Point::new(39, 59)));
        assert!(!rect.contains(Point::new(40, 59)));
        assert!(!rect.contains(Point::new(39, 60)));
        assert!(!rect.contains(Point::new(9, 20)));
    }

    #[test]
    fn inset_and_intersection_clamp_to_empty() {
        let rect = Rect::new(0, 0, 10, 10);
        assert_eq!(rect.inset(3), Rect::new(3, 3, 4, 4));
        assert_eq!(rect.inset(8).width, 0);
        assert_eq!(
            Rect::new(0, 0, 10, 10).intersect(Rect::new(5, 5, 10, 10)),
            Some(Rect::new(5, 5, 5, 5))
        );
        assert_eq!(
            Rect::new(0, 0, 10, 10).intersect(Rect::new(20, 20, 5, 5)),
            None
        );
    }
}
