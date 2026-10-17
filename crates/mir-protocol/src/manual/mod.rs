//! 生成器暂时表达不了、需要手写的包。
//!
//! 触发条件见 `tools/protocol-gen/report.md`：位标记打包、嵌套的「可选数组」、
//! 或者引用了尚未实现的数据类。每个手写包都必须在
//! [`crate::packets::client`] / [`crate::packets::server`] 里被 `pub use` 出来，
//! 以保证包枚举保持完整。

pub mod client;
pub mod server;
