//! 网格坐标与 Crystal `Shared/Functions/Functions.cs` 中的点运算。

use serde::{Deserialize, Serialize};

use crate::enums::MirDirection;

/// 与 Crystal `System.Drawing.Point` 对齐的整数网格坐标。
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct Point {
    /// 横向坐标（向右为正）。
    pub x: i32,
    /// 纵向坐标（向下为正）。
    pub y: i32,
}

impl Point {
    /// 新建坐标。
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// `InRange`：两点的切比雪夫距离是否不超过 `radius`。
    #[must_use]
    pub fn in_range(self, other: Self, radius: i32) -> bool {
        (self.x - other.x).abs() <= radius && (self.y - other.y).abs() <= radius
    }

    /// `MaxDistance`：切比雪夫距离。
    #[must_use]
    pub fn max_distance(self, other: Self) -> i32 {
        (self.x - other.x).abs().max((self.y - other.y).abs())
    }

    /// `DirectionFromPoint`：从 `self` 指向 `dest` 的八方向。
    ///
    /// Crystal 的实现只在两个轴上都做比较，斜向的判定与严格角度无关，
    /// 因此这里是逐行移植，保持行为一致。
    #[must_use]
    pub const fn direction_to(self, dest: Self) -> MirDirection {
        if self.x < dest.x {
            if self.y < dest.y {
                return MirDirection::DownRight;
            }
            if self.y > dest.y {
                return MirDirection::UpRight;
            }
            return MirDirection::Right;
        }
        if self.x > dest.x {
            if self.y < dest.y {
                return MirDirection::DownLeft;
            }
            if self.y > dest.y {
                return MirDirection::UpLeft;
            }
            return MirDirection::Left;
        }
        if self.y < dest.y {
            MirDirection::Down
        } else {
            MirDirection::Up
        }
    }

    /// `PointMove`：沿方向前进 `distance` 格。
    #[must_use]
    pub const fn moved(self, direction: MirDirection, distance: i32) -> Self {
        let (dx, dy) = direction.delta();
        Self {
            x: self.x + dx * distance,
            y: self.y + dy * distance,
        }
    }

    /// `Left`：方向左手边相邻格。
    #[must_use]
    pub const fn left(self, direction: MirDirection) -> Self {
        let (dx, dy) = direction.left_delta();
        Self {
            x: self.x + dx,
            y: self.y + dy,
        }
    }

    /// `Right`：方向右手边相邻格。
    #[must_use]
    pub const fn right(self, direction: MirDirection) -> Self {
        let (dx, dy) = direction.right_delta();
        Self {
            x: self.x + dx,
            y: self.y + dy,
        }
    }

    /// 方向上的相邻格（不乘步长），等价于 `moved(direction, 1)`。
    #[must_use]
    pub const fn adjacent(self, direction: MirDirection) -> Self {
        self.moved(direction, 1)
    }
}

impl core::ops::Add for Point {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl core::ops::Sub for Point {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl core::fmt::Display for Point {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}, {}", self.x, self.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_matches_crystal_table() {
        let origin = Point::new(10, 10);
        let cases = [
            (Point::new(10, 9), MirDirection::Up),
            (Point::new(11, 9), MirDirection::UpRight),
            (Point::new(11, 10), MirDirection::Right),
            (Point::new(11, 11), MirDirection::DownRight),
            (Point::new(10, 11), MirDirection::Down),
            (Point::new(9, 11), MirDirection::DownLeft),
            (Point::new(9, 10), MirDirection::Left),
            (Point::new(9, 9), MirDirection::UpLeft),
        ];
        for (dest, expected) in cases {
            assert_eq!(origin.direction_to(dest), expected, "dest {dest}");
        }
    }

    #[test]
    fn moving_then_back_is_identity() {
        let start = Point::new(5, 7);
        for direction in MirDirection::ALL {
            let moved = start.moved(*direction, 1);
            assert_eq!(moved.direction_to(start), direction.reverse());
            assert_eq!(start.moved(*direction, 0), start);
        }
    }

    #[test]
    fn range_uses_chebyshev_distance() {
        let a = Point::new(0, 0);
        assert!(a.in_range(Point::new(3, 3), 3));
        assert!(!a.in_range(Point::new(4, 0), 3));
        assert_eq!(a.max_distance(Point::new(-2, 5)), 5);
    }
}
