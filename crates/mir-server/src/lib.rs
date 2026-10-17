//! 《传奇-Next》服务端。
//!
//! 当前里程碑打通了 Crystal 协议的「连接 → 版本校验 → 注册/登录 → 选角 →
//! 进入世界 → 转身/走跑/聊天 → 下线」全链路，并在世界层移植了
//! **物理战斗与怪物 AI**（见 [`combat`]、[`monster`] 与 `docs/world/combat.md`）、
//! **物品系统**（含使用消耗品与丢金币，见 [`items`] 与 `docs/world/items.md`）、
//! **NPC 与商店**（见 [`npc`]、[`mir_db::npc_script`] 与 `docs/world/npc.md`）、
//! **技能/魔法**（含 9 个增益技能，见 [`magic`] 与 `docs/world/magic.md`）、
//! **增益容器**（见 [`buffs`] 与 `docs/world/buffs.md`）以及
//! **任务系统**（见 [`quests`]、[`mir_db::quest_script`] 与 `docs/world/quests.md`）。

pub mod buffs;
pub mod combat;
pub mod config;
pub mod exp;
pub mod gamedata;
pub mod items;
pub mod magic;
pub mod monster;
pub mod npc;
pub mod pg_store;
pub mod quests;
pub mod server;
pub mod session;
pub mod store;
pub mod world;

pub use buffs::{ActiveBuff, BuffKind, PendingBuff};
pub use combat::{CombatStats, HitResult};
pub use config::ServerConfig;
pub use exp::{ExpSource, ExpTable};
pub use gamedata::GameData;
pub use items::{
    GroundItem, ItemError, ItemUidGenerator, Looks, PickupReport, decode_equipment,
    decode_inventory, decode_slots, encode_slots,
};
pub use magic::{
    CastReport, LearnedMagic, MagicError, MagicState, SpellKind, UserMagic, decode_magics,
    encode_magics, spell_kind,
};
pub use monster::{Monster, MonsterKind};
pub use npc::{BuyReport, Npc, NpcError, NpcReply, NpcState, RepairReport, SellReport};
pub use pg_store::PgAccountStore;
pub use quests::{
    MAX_CONCURRENT_QUESTS, QuestCodecError, QuestDefinition, QuestError, QuestProgress,
    QuestProgressState, QuestReport, QuestSave, decode_quests, encode_quests,
    is_objectives_complete, monster_matches_task,
};
pub use server::{run, serve_forever};
pub use store::{Account, AccountStore, Character, MemoryStore, StoreError};
pub use world::{AttackReport, CombatError, MoveError, Player, RespawnState, SpawnGroup, World};
