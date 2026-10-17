//! 与 Crystal `System.Drawing.Color` 对齐的颜色值（线上就是 ARGB 整数）。

use serde::{Deserialize, Serialize};

/// ARGB 颜色，线路上以 `int` 传输，对应 Crystal `Color.FromArgb` / `ToArgb`。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Color(pub i32);

impl Color {
    /// 由 ARGB 整数构造。
    #[must_use]
    pub const fn from_argb(value: i32) -> Self {
        Self(value)
    }

    /// 由 RGB 分量构造不透明颜色。
    #[must_use]
    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self(i32::from_le_bytes([b, g, r, 0xFF]))
    }

    /// ARGB 整数（`Color.ToArgb()`）。
    #[must_use]
    pub const fn argb(self) -> i32 {
        self.0
    }

    /// 不透明度分量。
    #[must_use]
    pub const fn alpha(self) -> u8 {
        (self.0 >> 24) as u8
    }

    /// 红色分量。
    #[must_use]
    pub const fn red(self) -> u8 {
        (self.0 >> 16) as u8
    }

    /// 绿色分量。
    #[must_use]
    pub const fn green(self) -> u8 {
        (self.0 >> 8) as u8
    }

    /// 蓝色分量。
    #[must_use]
    pub const fn blue(self) -> u8 {
        self.0 as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_roundtrip() {
        let color = Color::from_rgb(0x12, 0x34, 0x56);
        assert_eq!(color.red(), 0x12);
        assert_eq!(color.green(), 0x34);
        assert_eq!(color.blue(), 0x56);
        assert_eq!(color.alpha(), 0xFF);
        assert_eq!(Color::from_argb(color.argb()), color);
    }
}
