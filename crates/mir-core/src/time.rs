//! 服务器时间抽象。
//!
//! Crystal 用 `long Envir.Time`（毫秒、服务器启动后单调递增）做所有计时；
//! 这里保留同样的毫秒语义，避免浮点与系统时钟回拨带来的行为漂移。

use serde::{Deserialize, Serialize};

/// 毫秒时间跨度（与 Crystal 的 `long` 毫秒字段一致）。
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct Timespan(pub i64);

impl Timespan {
    /// 零长度。
    pub const ZERO: Self = Self(0);
    /// 一秒。
    pub const SECOND: Self = Self(1_000);
    /// 一分钟。
    pub const MINUTE: Self = Self(60 * 1_000);

    /// 由秒构造。
    #[must_use]
    pub const fn from_secs(seconds: i64) -> Self {
        Self(seconds * 1_000)
    }

    /// 由毫秒构造。
    #[must_use]
    pub const fn from_millis(millis: i64) -> Self {
        Self(millis)
    }

    /// 毫秒数。
    #[must_use]
    pub const fn as_millis(self) -> i64 {
        self.0
    }

    /// 该时间跨度是否已经过去（`now` 为当前时刻）。
    #[must_use]
    pub const fn is_due_at(self, now: Self) -> bool {
        now.0 >= self.0
    }
}

impl core::ops::Add for Timespan {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl core::ops::AddAssign for Timespan {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}
