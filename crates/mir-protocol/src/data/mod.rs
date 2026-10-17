//! 手写的线格式数据类型。
//!
//! `packets/` 下的包结构由 `tools/protocol-gen/generate.py` 从 Crystal 的 C# 源码生成；
//! 生成器读不懂的那部分**嵌套数据类型**放在这里，按 Crystal 的
//! `BinaryReader` / `BinaryWriter` 调用顺序逐字节手写。

pub mod client_data;
pub mod select_info;
pub mod user_item;

pub use client_data::*;
pub use select_info::SelectInfo;
pub use user_item::{Awake, ExpireInfo, RentalInformation, SealedInfo, Stats, UserItem};
