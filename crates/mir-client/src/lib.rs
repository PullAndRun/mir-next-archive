//! 《传奇-Next》客户端库。
//!
//! 二进制入口（`main.rs`）负责窗口与命令行；真正的网络会话放在 [`net`]，
//! 这样端到端测试可以在没有窗口/图形环境的情况下直接驱动登录、走路与渲染。
//!
//! * [`net`]：阻塞 socket + 后台线程的 Crystal 登录/世界会话，维护 [`net::WorldSnapshot`]；
//! * [`ui`]：HUD、背包、商店、NPC 对话、技能面板与中文文字（[`ui::text`]）；
//! * [`geom`]：面板矩形与命中测试。
//!
//! 窗口与输入在二进制 crate 的 `app` 模块；本库不依赖 winit，所以无头测试可以直接
//! 画一帧 UI 到 PNG。

pub mod geom;
pub mod net;
pub mod ui;
