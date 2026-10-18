//! 《传奇-Next》服务端。
//!
//! 当前里程碑打通了 Crystal 协议的「连接 → 版本校验 → 注册/登录 → 选角 →
//! 进入世界 → 转身/走跑/聊天 → 下线」全链路，并在世界层移植了
//! **物理战斗与怪物 AI**（见 [`combat`]、[`monster`] 与 `docs/world/combat.md`）、
//! **物品系统**（含使用消耗品与丢金币，见 [`items`] 与 `docs/world/items.md`）、
//! **NPC 与商店**（见 [`npc`]、[`mir_db::npc_script`] 与 `docs/world/npc.md`）、
//! **技能/魔法**（含 9 个增益技能，见 [`magic`] 与 `docs/world/magic.md`）、
//! **增益容器**（见 [`buffs`] 与 `docs/world/buffs.md`）、
//! **任务系统**（见 [`quests`]、[`mir_db::quest_script`] 与 `docs/world/quests.md`）、
//! **多地图与传送**（见 [`maps`] 与 `docs/world/maps.md`）、
//! **组队**（见 [`group`] 与 `docs/world/group.md`）、
//! **行会**（见 [`guild`]、[`guild_storage`]、[`guild_buffs`] 与 `docs/world/guild.md`；
//! 联盟/投票未做）、**玩家仓库**（见 [`storage`] 与 `docs/world/storage.md`）、
//! **邮件**（见 [`mail`] 与 `docs/world/mail.md`）、
//! **玩家对玩家（PK）**（攻击模式、PK 值/红棕名、死亡掉落与守卫，见 `docs/world/pk.md`）、
//! **攻城/城主任命**（见 [`conquest`] 与 `docs/world/conquest.md`；城门/城墙/旗子等建筑未做）
//! 以及 **玩家间交易**（见 [`trade`] 与 `docs/world/trade.md`）。

pub mod aoi;
pub mod buffs;
pub mod combat;
pub mod config;
pub mod conquest;
pub mod exp;
pub mod gamedata;
pub mod gm;
pub mod group;
pub mod guild;
pub mod guild_buffs;
pub mod guild_storage;
pub mod items;
pub mod magic;
pub mod mail;
pub mod maps;
pub mod market;
pub mod monster;
pub mod npc;
pub mod pg_store;
pub mod quest_inventory;
pub mod quests;
pub mod server;
pub mod session;
pub mod storage;
pub mod store;
pub mod trade;
pub mod world;

pub use buffs::{ActiveBuff, BuffKind, PendingBuff};
pub use combat::{CombatStats, HitResult};
pub use config::ServerConfig;
pub use conquest::{
    CONQUEST_PROCESS_INTERVAL_MS, CONQUEST_SCHEDULE_INTERVAL_MS, CONTROL_POINT_RANGE, Conquest,
    ConquestError, ConquestSettings, ConquestTickReport, ControlPointState, MAX_CONTROL_POINTS,
    MAX_KING_POINTS, MINUTES_PER_DAY, WarRequestReport, check_day, decode_points, encode_points,
    minute_of_day, weekday_index,
};
pub use exp::{ExpSource, ExpTable};
pub use gamedata::GameData;
pub use group::{
    GROUP_EXP_RANGE, GROUP_INVITE_DELAY_MS, GROUP_INVITE_EXPIRE_MS, Group, GroupError, GroupInvite,
    GroupJoinOutcome, MAX_GROUP, PARTY_EXP_RATE,
};
pub use guild::{
    GUILD_CREATION_GOLD_DEFAULT, GUILD_DEFAULT_RANK_NAME, GUILD_EXP_RATE_DEFAULT,
    GUILD_EXP_UPDATE_MS, GUILD_LEADER_OPTIONS, GUILD_LEADER_RANK_NAME, GUILD_MAX_RANKS,
    GUILD_NAME_MAX_LEN, GUILD_NAME_MIN_LEN, GUILD_NOTICE_MAX_LINES, GUILD_REQUIRED_LEVEL_DEFAULT,
    GUILD_STORAGE_SLOTS, Guild, GuildError, GuildJoinOutcome, GuildMemberEdit, GuildMemberState,
    GuildRef, GuildSettings, GuildStorageSlot, has_option, validate_guild_name,
};
pub use items::{
    GroundItem, ItemError, ItemUidGenerator, Looks, PickupReport, decode_equipment,
    decode_inventory, decode_slots, decode_slots_with_limit, encode_slots,
};
pub use magic::{
    CastReport, LearnedMagic, MagicError, MagicState, SpellKind, UserMagic, decode_magics,
    encode_magics, spell_kind,
};
pub use mail::{
    MAIL_ATTACHMENTS_WITHOUT_STAMP, MAIL_AUTO_SEND_GOLD, MAIL_AUTO_SEND_ITEMS, MAIL_CAPACITY,
    MAIL_COST_PER_1K_GOLD, MAIL_EXPIRE_DAYS, MAIL_FREE_WITH_STAMP, MAIL_ITEM_INSURANCE_PERCENT,
    MAIL_MAX_ATTACHMENTS, MAIL_RATE_MS, MAIL_RECIPIENT_LIMIT, MAIL_TEXT_MAX, Mail, MailError,
    MailRecipient, MailState, MailWrite, persist_mail_writes,
};
pub use maps::{
    DEFAULT_MAP_CACHE_LIMIT, MAP_CACHE_ENV, MOVEMENT_DELAY_MS, MapChangeError, MapChangeReport,
    MapInitReport, PendingMapMove, ViewCounts,
};
pub use market::{
    AUCTION_COST, Auction, COMMISSION, CONSIGN_KEY, CONSIGNMENT_COST, CONSIGNMENT_LENGTH_DAYS,
    MARKET_KEY, MARKET_PAGE_SIZE, MAX_CONSIGNMENT, MAX_STARTING_BID, MIN_CONSIGNMENT,
    MIN_STARTING_BID, MarketError, MarketFailReason, MarketSearchState, SEARCH_DELAY_MS,
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
pub use storage::{
    STORAGE_MAX_SIZE, STORAGE_SIZE, StorageError, decode_storage, encode_storage, new_storage,
};
pub use store::{
    Account, AccountStore, AuctionRecord, Character, ConquestRecord, GuildBuffRecord,
    GuildMemberRecord, GuildRankRecord, GuildRecord, GuildStorageRecord, MailRecord, MemoryStore,
    StoreError,
};
pub use trade::{
    TRADE_DELAY_MS, TRADE_RANGE, TRADE_SLOTS, TradeError, TradeReplyOutcome, TradeState,
    facing_each_other,
};
pub use world::{
    AttackReport, CombatError, GuildWrite, MoveError, Player, RespawnState, SpawnGroup, World,
};
