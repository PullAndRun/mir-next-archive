//! 《传奇-Next》服务端。
//!
//! 当前里程碑打通了 Crystal 协议的「连接 → 版本校验 → 注册/登录 → 选角 →
//! 进入世界 → 转身/走跑/聊天 → 下线」全链路，并在世界层移植了
//! **物理战斗与怪物 AI**（见 [`combat`]、[`monster`] 与 `docs/world/combat.md`）、
//! **物品系统**（见 [`items`] 与 `docs/world/items.md`）以及
//! **NPC 与商店**（见 [`npc`]、[`mir_db::npc_script`] 与 `docs/world/npc.md`）。

pub mod combat;
pub mod config;
pub mod exp;
pub mod gamedata;
pub mod items;
pub mod monster;
pub mod npc;
pub mod pg_store;
pub mod server;
pub mod session;
pub mod store;
pub mod world;

pub use combat::{CombatStats, HitResult};
pub use config::ServerConfig;
pub use exp::{ExpSource, ExpTable};
pub use gamedata::GameData;
pub use items::{
    GroundItem, ItemError, ItemUidGenerator, Looks, PickupReport, decode_equipment,
    decode_inventory, decode_slots, encode_slots,
};
pub use monster::{Monster, MonsterKind};
pub use npc::{BuyReport, Npc, NpcError, NpcReply, NpcState, RepairReport, SellReport};
pub use pg_store::PgAccountStore;
pub use server::{run, serve_forever};
pub use store::{Account, AccountStore, Character, MemoryStore, StoreError};
pub use world::{AttackReport, CombatError, MoveError, Player, RespawnState, SpawnGroup, World};
