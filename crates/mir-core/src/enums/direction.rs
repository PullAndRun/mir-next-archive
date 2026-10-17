//! `MirDirection` 的几何辅助实现，对应 Crystal `Shared/Functions/Functions.cs`。

use crate::enums::MirDirection;

impl MirDirection {
    /// 该方向在网格上的位移（单位格）。
    #[must_use]
    pub const fn delta(self) -> (i32, i32) {
        match self {
            Self::Up => (0, -1),
            Self::UpRight => (1, -1),
            Self::Right => (1, 0),
            Self::DownRight => (1, 1),
            Self::Down => (0, 1),
            Self::DownLeft => (-1, 1),
            Self::Left => (-1, 0),
            Self::UpLeft => (-1, -1),
        }
    }

    /// 方向左手边相邻格的位移，对应 Crystal `Functions.Left`。
    #[must_use]
    pub const fn left_delta(self) -> (i32, i32) {
        match self {
            Self::Up => (-1, 0),
            Self::UpRight => (-1, -1),
            Self::Right => (0, -1),
            Self::DownRight => (1, -1),
            Self::Down => (1, 0),
            Self::DownLeft => (1, 1),
            Self::Left => (0, 1),
            Self::UpLeft => (-1, 1),
        }
    }

    /// 方向右手边相邻格的位移，对应 Crystal `Functions.Right`。
    #[must_use]
    pub const fn right_delta(self) -> (i32, i32) {
        match self {
            Self::Up => (1, 0),
            Self::UpRight => (1, 1),
            Self::Right => (0, 1),
            Self::DownRight => (-1, 1),
            Self::Down => (-1, 0),
            Self::DownLeft => (-1, -1),
            Self::Left => (0, -1),
            Self::UpLeft => (1, -1),
        }
    }

    /// `ShiftDirection`：按顺时针偏移 `steps` 个方向。
    #[must_use]
    pub const fn shift(self, steps: i8) -> Self {
        let index = (self as i8 + steps).rem_euclid(8) as u8;
        match Self::from_value(index) {
            Some(direction) => direction,
            None => Self::Up,
        }
    }

    /// `NextDir`：顺时针下一个方向。
    #[must_use]
    pub const fn next(self) -> Self {
        self.shift(1)
    }

    /// `PreviousDir`：逆时针上一个方向。
    #[must_use]
    pub const fn previous(self) -> Self {
        self.shift(-1)
    }

    /// `ReverseDirection`：反方向。
    #[must_use]
    pub const fn reverse(self) -> Self {
        self.shift(4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverse_is_opposite_delta() {
        for direction in MirDirection::ALL {
            let (dx, dy) = direction.delta();
            let (rx, ry) = direction.reverse().delta();
            assert_eq!((dx, dy), (-rx, -ry), "{direction:?}");
            assert_eq!(direction.reverse().reverse(), *direction);
        }
    }

    #[test]
    fn left_and_right_are_mirrored() {
        for direction in MirDirection::ALL {
            let (lx, ly) = direction.left_delta();
            let (rx, ry) = direction.right_delta();
            assert_eq!((lx, ly), (-rx, -ry), "{direction:?}");
            assert_eq!(direction.next(), direction.shift(1));
            assert_eq!(direction.next().previous(), *direction);
        }
    }
}
