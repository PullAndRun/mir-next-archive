//! 《传奇-Next》服务端。
//!
//! 当前里程碑打通了 Crystal 协议的「连接 → 版本校验 → 注册/登录 → 选角 →
//! 进入世界 → 转身/走跑/聊天 → 下线」全链路，世界层是最小实现（单地图、
//! 视野广播、格占用校验），后续按 `docs/world/` 的规则接入真实地图与战斗。

pub mod config;
pub mod pg_store;
pub mod server;
pub mod session;
pub mod store;
pub mod world;

pub use config::ServerConfig;
pub use pg_store::PgAccountStore;
pub use server::{run, serve_forever};
pub use store::{Account, AccountStore, Character, MemoryStore, StoreError};
