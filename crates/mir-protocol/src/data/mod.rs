//! 手写的线格式数据类型。
//!
//! `packets/` 下的包结构由 `tools/protocol-gen/generate.py` 从 Crystal 的 C# 源码生成；
//! 生成器读不懂的那部分**嵌套数据类型**放在这里，按 Crystal 的
//! `BinaryReader` / `BinaryWriter` 调用顺序逐字节手写。
//!
//! 每个类型都提供 `decode(&mut PacketReader) -> Result<Self, ProtocolError>` 与
//! `encode(&self, &mut PacketWriter)`，与生成代码的调用约定一致。

pub mod base_stats;
pub mod client_auction;
pub mod client_buff;
pub mod client_data;
pub mod client_friend;
pub mod client_gt_map;
pub mod client_hero_information;
pub mod client_mail;
pub mod client_map_info;
pub mod client_monster_info;
pub mod client_movement_info;
pub mod client_npc_info;
pub mod client_quest_info;
pub mod client_quest_progress;
pub mod client_recipe_info;
pub mod game_shop_item;
pub mod guild_buff;
pub mod guild_rank;
pub mod guild_storage_item;
pub mod item_info;
pub mod item_rental_information;
pub mod notice;
pub mod quest_item_reward;
pub mod rank_character_info;
pub mod select_info;
pub mod user_item;
pub mod world_map_icon;
pub mod world_map_setup;

pub use base_stats::BaseStats;
pub use client_auction::ClientAuction;
pub use client_buff::ClientBuff;
pub use client_data::*;
pub use client_friend::ClientFriend;
pub use client_gt_map::ClientGTMap;
pub use client_hero_information::ClientHeroInformation;
pub use client_mail::ClientMail;
pub use client_map_info::ClientMapInfo;
pub use client_monster_info::ClientMonsterInfo;
pub use client_movement_info::ClientMovementInfo;
pub use client_npc_info::ClientNPCInfo;
pub use client_quest_info::ClientQuestInfo;
pub use client_quest_progress::ClientQuestProgress;
pub use client_recipe_info::ClientRecipeInfo;
pub use game_shop_item::GameShopItem;
pub use guild_buff::{GuildBuff, GuildBuffInfo};
pub use guild_rank::GuildRank;
pub use guild_storage_item::GuildStorageItem;
pub use item_info::ItemInfo;
pub use item_rental_information::ItemRentalInformation;
pub use notice::Notice;
pub use quest_item_reward::QuestItemReward;
pub use rank_character_info::RankCharacterInfo;
pub use select_info::SelectInfo;
pub use user_item::{Awake, ExpireInfo, RentalInformation, SealedInfo, Stats, UserItem};
pub use world_map_icon::WorldMapIcon;
pub use world_map_setup::WorldMapSetup;
