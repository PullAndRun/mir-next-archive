//! 包定义。
//!
//! * `client` / `server`：由 `tools/protocol-gen/generate.py` 从 Crystal 的 C# 源码生成。
//! * 手写的包在 [`crate::manual`]，生成文件只把它们 `pub use` 进来，
//!   以保持包枚举与 Crystal 的编号表一致。

pub mod client;
pub mod server;
