//! 与 Crystal `Shared/Enums.cs` 一一对应的枚举。
//!
//! `generated` 由 `tools/protocol-gen/generate.py` 从 Crystal 源码生成，
//! 不要手工修改；其余模块放针对这些枚举的手写实现。

mod direction;
mod generated;

pub use generated::*;
