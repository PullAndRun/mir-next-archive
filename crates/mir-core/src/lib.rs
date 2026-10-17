//! 《传奇-Next》基础类型。
//!
//! 本 crate 只放与引擎无关的领域类型：坐标、方向、与 Crystal `Shared/Enums.cs`
//! 对齐的枚举，以及服务器时钟与随机数。协议编解码、世界规则与存储都不在这里。

pub mod color;
pub mod datetime;
pub mod enums;
pub mod i18n;
pub mod point;
pub mod rng;
pub mod time;

pub use color::Color;
pub use datetime::DateTimeBinary;
pub use enums::*;
pub use i18n::{Language, LanguageCatalog, TextKey, text};
pub use point::Point;
pub use rng::MirRng;
pub use time::Timespan;

/// 未知枚举值：`TryFrom` 转换失败时返回。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownEnumValue {
    /// 枚举类型名（与 Crystal 一致）。
    pub type_name: &'static str,
    /// 无法识别的原始值。
    pub value: i64,
}

impl core::fmt::Display for UnknownEnumValue {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} 没有取值为 {} 的成员", self.type_name, self.value)
    }
}

impl std::error::Error for UnknownEnumValue {}
