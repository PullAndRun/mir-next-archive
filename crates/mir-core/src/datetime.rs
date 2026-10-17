//! .NET `DateTime.ToBinary()` / `FromBinary()` 的线格式。

use serde::{Deserialize, Serialize};

/// .NET `DateTime` 的二进制表示，与 Crystal 在协议与存档中使用的 `long` 一致。
///
/// 布局与 .NET 相同：低 62 位是自 `0001-01-01` 起的 100ns tick 数，
/// 高 2 位是 `DateTimeKind`（0 = Unspecified，1 = Utc，2 = Local）。
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct DateTimeBinary(pub i64);

impl DateTimeBinary {
    /// 每 tick 的 100ns 精度。
    const TICKS_PER_SECOND: i64 = 10_000_000;
    /// `1970-01-01T00:00:00Z` 对应的 tick 数。
    const UNIX_EPOCH_TICKS: i64 = 621_355_968_000_000_000;
    const KIND_MASK: u64 = 0b11 << 62;
    const TICKS_MASK: i64 = 0x3FFF_FFFF_FFFF_FFFF;

    /// 构造一个 UTC 值（`DateTimeKind.Utc`）。
    #[must_use]
    pub const fn from_unix_seconds(seconds: i64) -> Self {
        let ticks = Self::UNIX_EPOCH_TICKS + seconds * Self::TICKS_PER_SECOND;
        // kind = 1 (Utc) 放在最高两位。
        Self(ticks | (1i64 << 62))
    }

    /// 转换为 Unix 时间戳（秒）；`Unspecified` 与 `Local` 按 UTC 处理。
    #[must_use]
    pub const fn to_unix_seconds(self) -> i64 {
        ((self.0 & Self::TICKS_MASK) - Self::UNIX_EPOCH_TICKS) / Self::TICKS_PER_SECOND
    }

    /// `DateTimeKind`：0 未指定，1 UTC，2 本地。
    #[must_use]
    pub const fn kind(self) -> u8 {
        (((self.0 as u64) & Self::KIND_MASK) >> 62) as u8
    }

    /// 未设置的时间（数值 0）。
    #[must_use]
    pub const fn is_unset(self) -> bool {
        self.0 == 0
    }
}

impl From<chrono::DateTime<chrono::Utc>> for DateTimeBinary {
    fn from(value: chrono::DateTime<chrono::Utc>) -> Self {
        Self::from_unix_seconds(value.timestamp())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_epoch_roundtrip() {
        let value = DateTimeBinary::from_unix_seconds(0);
        assert_eq!(value.to_unix_seconds(), 0);
        assert_eq!(value.kind(), 1);
    }

    #[test]
    fn known_dotnet_value() {
        // 2024-01-01T00:00:00Z = 1704067200
        let value = DateTimeBinary::from_unix_seconds(1_704_067_200);
        assert_eq!(value.to_unix_seconds(), 1_704_067_200);
        // tick 数应与 .NET 计算一致：621355968000000000 + 1704067200 * 10^7
        assert_eq!(
            value.0 & DateTimeBinary::TICKS_MASK,
            621_355_968_000_000_000 + 1_704_067_200 * 10_000_000
        );
    }

    #[test]
    fn unset_is_zero() {
        assert!(DateTimeBinary::default().is_unset());
        assert!(!DateTimeBinary::from_unix_seconds(0).is_unset());
    }
}
