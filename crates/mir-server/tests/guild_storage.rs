//! 行会仓库（物品 + 金币）与行会增益（guild buff）的端到端测试。
//!
//! 全部确定性：
//! * 世界随机数固定种子（`World::seed_rng`）；时间用 `World::advance_clock` 快进；
//! * 地形是内存里合成的 60×60 空地；`MapInfo.SafeZones` 在内存数据库里构造（安全区判定用）；
//! * 物品模板、行会配置（含 `buff_list`）在每个用例里显式构造，不依赖 `resource/`
//!   （唯一的例外是最后一个「真实数据冒烟」用例）；
//! * 持久化用 [`MemoryStore`]（实现了 `AccountStore` 的行会函数）：世界只产生
//!   [`GuildWrite`] 队列，测试用 `persist_guild_writes` 把它写进 `MemoryStore`，
//!   再用 [`World::load_guilds`] 读回来断言。**选型说明**：真实 PostgreSQL 的仓库往返
//!   由 `crates/mir-store/tests/guild_roundtrip.rs` 与 `crates/mir-server/tests/postgres_guild.rs`
//!   覆盖（需要 `MIR2_TEST_DATABASE_URL`，默认跳过）；这里用 `MemoryStore` 是为了让
//!   「世界视图 ↔ `GuildRecord`（含 `storage`/`buffs`）↔ `UserItem` 字节块」的编解码
//!   在**没有数据库**的环境里也能被确定性断言，且不引入临时 Postgres 依赖。
//!
//! 关键 Crystal 源码：`Server/MirObjects/PlayerObject.cs:10061-10436`（仓库 + buff 入口）、
//! `Server/MirObjects/GuildObject.cs:796-1028`（buff 购买/激活/到期）、
//! `Server/MirDatabase/GuildInfo.cs:22-23`（`StoredItems[112]` / `BuffList`）、
//! `Server/Settings.cs:1331-1376`（`GuildSettings.ini` 的 `[Buff-N]`）。

use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{BindMode, ChatType, ItemType, MirClass, MirDirection, MirGender, Stat};
use mir_core::i18n::{Language, TextKey, format_text};
use mir_db::{MapInfo, MirDb, SafeZoneInfo};
use mir_protocol::ServerPacket;
use mir_protocol::data::{GuildBuffInfo, ItemInfo, Stats, UserItem};
use mir_protocol::packets::server::{
    GuildBuffList as GuildBuffListPacket, GuildStorageGoldChange as GuildStorageGoldChangePacket,
    GuildStorageItemChange as GuildStorageItemChangePacket,
    GuildStorageList as GuildStorageListPacket,
};
use mir_server::gamedata::GameData;
use mir_server::guild::persist_guild_writes;
use mir_server::{
    AccountStore, GuildError, GuildJoinOutcome, GuildMemberEdit, GuildSettings, MemoryStore,
    NpcState, Player, World, combat,
};
use mir_world::{Map, MapCell, MapVersion};
use tokio::sync::mpsc;

/// 测试地图（文件名 = 数据库里的 `file_name`）。
const MAP: &str = "0";
/// 地图数据库主键。
const MAP_INDEX: i32 = 10;
/// 会长等级（Crystal `Guild_RequiredLevel = 22`）。
const LEADER_LEVEL: u16 = 30;
/// 新建行会的金币花费。
const CREATION_GOLD: u32 = 1_000_000;
/// 普通物品模板编号。
const ITEM_INDEX: i32 = 1;
/// 绑定 `BindMode.DontStore` 的物品模板编号。
const BOUND_ITEM_INDEX: i32 = 2;
/// 行会仓库的格子数（`GuildInfo.cs:22` 的 `new GuildStorageItem[112]`）。
const STORAGE_SLOTS: usize = 112;
/// 背包格数（`crates/mir-server/src/items.rs` 的 `INVENTORY_SIZE`）。
const INVENTORY_SLOTS: usize = 46;

// ---------------------------------------------------------------------------
// 数据与配置
// ---------------------------------------------------------------------------

/// `Stats` 的小工具：`&[(Stat, i32)]` → `Stats`。
fn stats(pairs: &[(Stat, i32)]) -> Stats {
    Stats::from_iter(pairs.iter().copied())
}

/// 行会配置：5 级经验表（每级 100）、每级 1 个升级点、三个测试增益。
///
/// 增益设计：
/// * `1`：`等级 0 / 1 点 / 2 分钟 / 500 金币 / +50 HP`（可到期、可重新激活）；
/// * `2`：`等级 3 / 2 点 / 永久 / 免费 / +5 MaxDC`（等级与点数校验）；
/// * `3`：`等级 0 / 6 点`（点数不足）。
fn test_settings() -> GuildSettings {
    GuildSettings {
        required_level: 22,
        creation_gold: CREATION_GOLD,
        // 1:1 结算，便于用 `guild_gain_exp` 精确控制等级与升级点。
        exp_rate: 1.0,
        point_per_level: 1,
        experience_list: vec![100; 5],
        member_cap_list: vec![10; 30],
        buff_list: vec![
            GuildBuffInfo {
                id: 1,
                icon: 10,
                name: "体力祝福".to_owned(),
                level_requirement: 0,
                points_requirement: 1,
                // 单位是分钟（`GuildBuffInfo.TimeLimit`）。
                time_limit: 2,
                activation_cost: 500,
                stats: stats(&[(Stat::HP, 50)]),
            },
            GuildBuffInfo {
                id: 2,
                icon: 11,
                name: "高级力量".to_owned(),
                level_requirement: 3,
                points_requirement: 2,
                time_limit: 0,
                activation_cost: 0,
                stats: stats(&[(Stat::MaxDC, 5)]),
            },
            GuildBuffInfo {
                id: 3,
                icon: 12,
                name: "昂贵之力".to_owned(),
                level_requirement: 0,
                points_requirement: 6,
                time_limit: 0,
                activation_cost: 0,
                stats: stats(&[(Stat::MaxDC, 1)]),
            },
        ],
    }
}

/// 物品模板。
fn item_info(index: i32, name: &str, bind: BindMode) -> ItemInfo {
    ItemInfo {
        index,
        name: name.to_owned(),
        item_type: ItemType::Weapon,
        bind,
        weight: 10,
        image: u16::try_from(index).unwrap_or(0),
        durability: 5_000,
        ..ItemInfo::default()
    }
}

/// 60×60 的可走空地。
fn synthetic_map() -> Arc<Map> {
    Map {
        width: 60,
        height: 60,
        cells: vec![MapCell::default(); 3600],
        file_name: MAP.to_owned(),
        version: MapVersion::V0,
        doors: Vec::new(),
    }
    .into()
}

/// 最小数据库：一张带安全区（中心 10,10，半径 10）的地图 + 两件物品模板。
fn base_db() -> MirDb {
    MirDb {
        maps: vec![MapInfo {
            index: MAP_INDEX,
            file_name: MAP.to_owned(),
            title: "比奇省".to_owned(),
            safe_zones: vec![SafeZoneInfo {
                location: Point::new(10, 10),
                size: 10,
                start_point: true,
            }],
            ..MapInfo::default()
        }],
        items: vec![
            item_info(ITEM_INDEX, "测试之剑", BindMode::None),
            item_info(BOUND_ITEM_INDEX, "绑定之剑", BindMode::DontStore),
        ],
        ..MirDb::default()
    }
}

// ---------------------------------------------------------------------------
// 脚手架
// ---------------------------------------------------------------------------

struct Harness {
    world: World,
    receivers: HashMap<u32, mpsc::UnboundedReceiver<Vec<u8>>>,
    next_uid: u64,
}

impl Harness {
    fn new(seed: u64) -> Self {
        Self::with_settings(seed, test_settings())
    }

    fn with_settings(seed: u64, settings: GuildSettings) -> Self {
        let mut world = World::new(999);
        world.seed_rng(seed);
        world.insert_map(MAP, synthetic_map());
        world.set_game_data(Arc::new(GameData::from_db(base_db(), None)));
        world.set_guild_settings(settings);
        Self {
            world,
            receivers: HashMap::new(),
            next_uid: 1,
        }
    }

    fn add_player(&mut self, object_id: u32, name: &str, level: u16, gold: i64) -> u32 {
        self.add_player_as(object_id, i64::from(object_id), name, level, gold)
    }

    fn add_player_as(
        &mut self,
        object_id: u32,
        character_id: i64,
        name: &str,
        level: u16,
        gold: i64,
    ) -> u32 {
        let stats = combat::player_base_stats(level, MirClass::Warrior);
        let (sender, receiver) = mpsc::unbounded_channel();
        self.world.enter(Player {
            object_id,
            session_id: u64::from(object_id),
            character_id,
            name: name.to_owned(),
            map: MAP.to_owned(),
            position: Point::new(10, 10),
            direction: MirDirection::Down,
            level,
            experience: 0,
            max_experience: 0,
            class: MirClass::Warrior,
            gender: MirGender::Male,
            hair: 1,
            hp: stats.hp,
            mp: stats.mp,
            stats,
            action_time: 0,
            attack_time: 0,
            struck_time: 0,
            regen_time: i64::MAX,
            death_time: 0,
            revive_time: 0,
            cell_time: 0,
            step_counter: 0,
            run_counter: 0,
            run_time: 0,
            dead: false,
            attack_mode: mir_core::enums::AttackMode::Peace,
            pk_points: 0,
            brown_time: 0,
            pk_point_time: 0,
            last_hitter: None,
            last_hit_time: 0,
            name_colour: mir_core::Color(-1),
            visible_monsters: BTreeSet::new(),
            visible_npcs: BTreeSet::new(),
            movement_time: 0,
            inventory: mir_server::items::new_inventory(),
            equipment: mir_server::items::new_equipment(),
            storage: mir_server::storage::new_storage(),
            magics: Vec::new(),
            magic: mir_server::MagicState::default(),
            gold,
            credit: 0,
            visible_items: BTreeSet::new(),
            quests: Vec::new(),
            completed_quests: BTreeSet::new(),
            mail: mir_server::MailState::default(),
            buffs: Vec::new(),
            pot_health_amount: 0,
            pot_mana_amount: 0,
            pot_time: 0,
            unlock_curse: false,
            npc: NpcState::default(),
            group_id: None,
            group_invitation: None,
            group_invite_time: 0,
            allow_group: true,
            guild: None,
            trade: mir_server::TradeState::default(),
            trade_invitation: None,
            next_trade_time: 0,
            allow_trade: true,
            bind_map_index: 0,
            bind_point: mir_core::Point::default(),
            sender,
        });
        self.receivers.insert(object_id, receiver);
        object_id
    }

    fn name_of(&self, object_id: u32) -> String {
        self.world
            .player(object_id)
            .map_or_else(|| panic!("玩家 {object_id} 不在世界里"), |p| p.name.clone())
    }

    fn packets(&mut self, object_id: u32) -> Vec<ServerPacket> {
        let receiver = self.receivers.get_mut(&object_id).expect("玩家通道");
        let mut packets = Vec::new();
        while let Ok(frame) = receiver.try_recv() {
            let index = i16::from_le_bytes([frame[2], frame[3]]);
            packets.push(ServerPacket::decode(index, &frame[4..]).expect("解码服务端包"));
        }
        packets
    }

    fn clear(&mut self, object_id: u32) {
        let _ = self.packets(object_id);
    }

    fn create_guild(&mut self, leader: u32, name: &str) -> i32 {
        self.world
            .create_guild(leader, name)
            .unwrap_or_else(|error| panic!("创建行会 {name} 应成功：{error}"))
    }

    fn invite_and_join(&mut self, inviter: u32, member: u32) -> u8 {
        let name = self.name_of(member);
        self.world
            .guild_invite(inviter, &name)
            .unwrap_or_else(|error| panic!("邀请 {name} 应成功：{error}"));
        match self
            .world
            .guild_join(member, true)
            .unwrap_or_else(|error| panic!("{name} 接受邀请应成功：{error}"))
        {
            GuildJoinOutcome::Joined(_) => {}
            GuildJoinOutcome::Declined => panic!("不该走到拒绝分支"),
        }
        self.world
            .guild_ref_of(member)
            .expect("加入后应有行会")
            .rank_index
    }

    /// 打开/关闭某个职位的 `GuildRankOptions` 位（`C.EditGuildMember{ChangeType=5}`）。
    fn set_rank_option(&mut self, leader: u32, rank_index: u8, option: u8, enabled: bool) {
        let edit = GuildMemberEdit {
            change_type: 5,
            rank_index,
            name: if enabled { "true" } else { "false" }.to_owned(),
            rank_name: option.to_string(),
        };
        self.world
            .guild_edit_member(leader, &edit)
            .unwrap_or_else(|error| {
                panic!("设置职位 {rank_index} 的权限位 {option} 应成功：{error}")
            });
    }

    /// 用行会经验把行会升到 5 级（每级 100 经验、每级 1 点升级点）。
    fn level_guild(&mut self, leader: u32) {
        self.world.guild_gain_exp(leader, 1_000);
        let guild = self.world.guild_of(leader).expect("行会");
        assert_eq!(guild.level, 5, "1000 经验应升到 5 级（每级 100）");
        assert_eq!(guild.spare_points, 5, "5 级应累计 5 个升级点");
    }

    /// 往背包指定格子里放一件物品。
    fn give_item(&mut self, object_id: u32, slot: usize, item_index: i32) -> u64 {
        let uid = self.next_uid;
        self.next_uid += 1;
        let item = UserItem {
            unique_id: uid,
            item_index,
            current_dura: 1_000,
            max_dura: 5_000,
            count: 1,
            identified: true,
            ..UserItem::default()
        };
        let player = self.world.player_mut(object_id).expect("玩家");
        player.inventory[slot] = Some(item);
        uid
    }

    /// 把玩家挪出安全区（测试地图的安全区在 (10,10)，半径 10）。
    ///
    /// 走 `World::place_player`：同步 AOI 网格，避免查询按旧坐标找人。
    fn move_outside_safe_zone(&mut self, object_id: u32) {
        let map = self.world.player(object_id).expect("玩家").map.clone();
        assert!(self.world.place_player(object_id, &map, Point::new(50, 50)));
    }

    fn move_into_safe_zone(&mut self, object_id: u32) {
        let map = self.world.player(object_id).expect("玩家").map.clone();
        assert!(self.world.place_player(object_id, &map, Point::new(10, 10)));
    }

    /// 把世界积压的行会写入落到 `MemoryStore`。
    async fn flush(&mut self, store: &MemoryStore) {
        let writes = self.world.take_guild_writes();
        persist_guild_writes(store, writes)
            .await
            .expect("行会落盘应成功");
    }
}

// ---------------------------------------------------------------------------
// 包断言小工具
// ---------------------------------------------------------------------------

fn chats(packets: &[ServerPacket]) -> Vec<(&str, ChatType)> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::Chat(chat) => Some((chat.message.as_str(), chat.type_)),
            _ => None,
        })
        .collect()
}

fn item_changes(packets: &[ServerPacket]) -> Vec<&GuildStorageItemChangePacket> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::GuildStorageItemChange(change) => Some(change),
            _ => None,
        })
        .collect()
}

fn storage_lists(packets: &[ServerPacket]) -> Vec<&GuildStorageListPacket> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::GuildStorageList(list) => Some(list),
            _ => None,
        })
        .collect()
}

fn gold_changes(packets: &[ServerPacket]) -> Vec<&GuildStorageGoldChangePacket> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::GuildStorageGoldChange(change) => Some(change),
            _ => None,
        })
        .collect()
}

fn buff_lists(packets: &[ServerPacket]) -> Vec<&GuildBuffListPacket> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::GuildBuffList(list) => Some(list),
            _ => None,
        })
        .collect()
}

fn statuses(packets: &[ServerPacket]) -> Vec<&mir_protocol::packets::server::GuildStatus> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::GuildStatus(status) => Some(status),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 1. 存入物品：安全区 / 权限 / 目标格
// ---------------------------------------------------------------------------

#[test]
fn store_item_checks_safe_zone_permission_and_slot() {
    let mut harness = Harness::new(1);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 2_000_000);
    let bob = harness.add_player(2, "Bob", 10, 0);
    let guild_id = harness.create_guild(leader, "测试行会");
    harness.invite_and_join(leader, bob);
    harness.clear(leader);
    harness.clear(bob);

    // ---- 不在安全区：`Enqueue(p); ReceiveChat(GuildStorageOutsideSafezone)`（`:10131-10136`）----
    harness.move_outside_safe_zone(bob);
    let uid = harness.give_item(bob, 0, ITEM_INDEX);
    assert_eq!(
        harness.world.guild_storage_item_change(bob, 0, 0, 5),
        Err(GuildError::NotInSafeZone)
    );
    let packets = harness.packets(bob);
    let failures = item_changes(&packets);
    assert_eq!(failures.len(), 1, "应收到一条失败回执，实际 {packets:?}");
    assert_eq!(failures[0].change_type, 3, "失败类型是 3 + 0");
    assert_eq!((failures[0].from, failures[0].to), (0, 5));
    assert!(failures[0].item.is_none());
    assert!(
        chats(&packets)
            .iter()
            .any(|(text, kind)| *kind == ChatType::System
                && text == &format_text(Language::ZhCn, TextKey::GuildStorageOutsideSafezone, &[])),
        "应提示只能在安全区使用，实际 {packets:?}"
    );
    assert_eq!(
        harness
            .world
            .guild(guild_id)
            .and_then(|guild| guild.storage_slot(5)),
        None,
        "失败不应改动仓库"
    );

    // ---- 安全区里但职位没有 `CanStoreItem`（Bob 的 `Members` 职位 options = 0）----
    harness.move_into_safe_zone(bob);
    harness.clear(bob);
    assert_eq!(
        harness.world.guild_storage_item_change(bob, 0, 0, 5),
        Err(GuildError::NoStorePermission)
    );
    let packets = harness.packets(bob);
    assert_eq!(item_changes(&packets).len(), 1);
    assert_eq!(item_changes(&packets)[0].change_type, 3);
    assert!(
        chats(&packets)
            .iter()
            .any(|(text, kind)| *kind == ChatType::System
                && text
                    == &format_text(Language::ZhCn, TextKey::GuildStorageNoStorePermission, &[])),
        "应提示没有存物品权限，实际 {packets:?}"
    );

    // ---- 打开权限位：`CanStoreItem = 8` → `1 << 3` ----
    harness.set_rank_option(leader, 1, 3, true);
    harness.clear(leader);
    harness.clear(bob);
    assert_eq!(
        harness.world.guild_storage_item_change(bob, 0, 0, 5),
        Ok(())
    );

    // `StoredItems[to] = new GuildStorageItem { Item = Info.Inventory[from], UserId = Info.Index }`（`:10178`）。
    let guild = harness.world.guild(guild_id).expect("行会");
    let slot = guild.storage_slot(5).expect("第 5 格应有物品");
    assert_eq!(slot.item.item.unique_id, uid);
    assert_eq!(slot.item.item.item_index, ITEM_INDEX);
    assert_eq!(
        slot.item.user_id,
        i64::from(bob),
        "UserId 是存放者的角色 ID"
    );
    assert!(slot.stored_at > 0, "本作扩展的存放时间应被记录");
    assert_eq!(
        harness
            .world
            .player(bob)
            .and_then(|player| player.inventory[0].clone()),
        None,
        "背包格子应清空"
    );

    // `MyGuild.SendServerPacket(new S.GuildStorageItemChange { Type = 0, User, Item, To, From })`（`:10182`）。
    for who in [leader, bob] {
        let packets = harness.packets(who);
        let changes = item_changes(&packets);
        assert_eq!(
            changes.len(),
            1,
            "玩家 {who} 应收到一条成功回执，实际 {packets:?}"
        );
        let change = changes[0];
        assert_eq!(change.change_type, 0);
        assert_eq!(change.user, i32::try_from(bob).unwrap());
        assert_eq!((change.from, change.to), (0, 5));
        assert_eq!(
            change.item.as_ref().map(|item| item.item.unique_id),
            Some(uid)
        );
    }
}

#[test]
fn store_item_rejects_dont_store_bind_and_occupied_slot() {
    let mut harness = Harness::new(2);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 2_000_000);
    let guild_id = harness.create_guild(leader, "测试行会");
    harness.clear(leader);

    // `BindMode.DontStore`（`:10162-10166`）。
    harness.give_item(leader, 0, BOUND_ITEM_INDEX);
    assert_eq!(
        harness.world.guild_storage_item_change(leader, 0, 0, 0),
        Err(GuildError::ItemCannotBeStored)
    );
    let packets = harness.packets(leader);
    assert_eq!(item_changes(&packets)[0].change_type, 3);
    assert!(
        harness
            .world
            .guild(guild_id)
            .is_some_and(|guild| guild.storage_slot(0).is_none()),
        "绑定物品不应进仓库"
    );

    // 占位：`if (MyGuild.StoredItems[to] != null) { ReceiveChat(TargetSlotNotEmpty); Enqueue(p); }`（`:10172-10177`）。
    harness.give_item(leader, 0, ITEM_INDEX);
    assert_eq!(
        harness.world.guild_storage_item_change(leader, 0, 0, 0),
        Ok(())
    );
    let first_uid = harness
        .world
        .guild(guild_id)
        .and_then(|guild| guild.storage_slot(0))
        .map(|slot| slot.item.item.unique_id)
        .expect("仓库第 0 格");
    harness.give_item(leader, 0, ITEM_INDEX);
    harness.clear(leader);
    assert_eq!(
        harness.world.guild_storage_item_change(leader, 0, 0, 0),
        Err(GuildError::TargetSlotNotEmpty)
    );
    let packets = harness.packets(leader);
    assert_eq!(item_changes(&packets)[0].change_type, 3);
    assert!(
        chats(&packets)
            .iter()
            .any(|(text, kind)| *kind == ChatType::System
                && text
                    == &format_text(Language::ZhCn, TextKey::GuildStorageTargetSlotNotEmpty, &[])),
        "应提示目标格非空，实际 {packets:?}"
    );
    assert_eq!(
        harness
            .world
            .guild(guild_id)
            .and_then(|guild| guild.storage_slot(0))
            .map(|slot| slot.item.item.unique_id),
        Some(first_uid),
        "失败不应覆盖原有物品"
    );
    assert!(
        harness
            .world
            .player(leader)
            .is_some_and(|player| player.inventory[0].is_some()),
        "失败的物品应留在背包"
    );
}

#[test]
fn full_storage_rejects_new_items() {
    let mut harness = Harness::new(3);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 2_000_000);
    let guild_id = harness.create_guild(leader, "测试行会");

    // 填满 112 格（Crystal 的仓库就是 112 格数组，没有「仓库容量」配置项）。
    for slot in 0..STORAGE_SLOTS {
        harness.give_item(leader, 0, ITEM_INDEX);
        harness
            .world
            .guild_storage_item_change(leader, 0, 0, i32::try_from(slot).expect("格子序号"))
            .unwrap_or_else(|error| panic!("填第 {slot} 格应成功：{error}"));
        if slot % 16 == 0 {
            harness.clear(leader);
        }
    }
    harness.clear(leader);
    let guild = harness.world.guild(guild_id).expect("行会");
    assert_eq!(guild.storage.len(), STORAGE_SLOTS);
    assert!(
        guild.storage.iter().all(Option::is_some),
        "112 格应全部占满"
    );

    // 再存一件：任意格子都非空 → `TargetSlotNotEmpty`。
    harness.give_item(leader, 0, ITEM_INDEX);
    assert_eq!(
        harness.world.guild_storage_item_change(leader, 0, 0, 0),
        Err(GuildError::TargetSlotNotEmpty)
    );
    let packets = harness.packets(leader);
    assert_eq!(item_changes(&packets)[0].change_type, 3);
    assert!(
        harness
            .world
            .player(leader)
            .is_some_and(|player| player.inventory[0].is_some())
    );
}

// ---------------------------------------------------------------------------
// 2. 取回物品：目标格 / 权限（Crystal 的失败包例外）
// ---------------------------------------------------------------------------

#[test]
fn retrieve_item_requires_an_empty_inventory_slot() {
    let mut harness = Harness::new(4);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 2_000_000);
    let guild_id = harness.create_guild(leader, "测试行会");
    harness.give_item(leader, 0, ITEM_INDEX);
    harness
        .world
        .guild_storage_item_change(leader, 0, 0, 3)
        .expect("存入");
    let stored_uid = harness
        .world
        .guild(guild_id)
        .and_then(|guild| guild.storage_slot(3))
        .map(|slot| slot.item.item.unique_id)
        .expect("仓库第 3 格");

    // `if (Info.Inventory[to] != null) { ReceiveChat(TargetSlotNotEmpty); Enqueue(p); }`（`:10202-10207`）。
    harness.give_item(leader, 10, ITEM_INDEX);
    let blocker_uid = harness.world.player(leader).expect("玩家").inventory[10]
        .as_ref()
        .map(|item| item.unique_id)
        .expect("背包 10 号格");
    harness.clear(leader);
    assert_eq!(
        harness.world.guild_storage_item_change(leader, 1, 3, 10),
        Err(GuildError::TargetSlotNotEmpty)
    );
    let packets = harness.packets(leader);
    let failures = item_changes(&packets);
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].change_type, 4, "失败类型是 3 + 1");
    assert_eq!((failures[0].from, failures[0].to), (3, 10));
    assert!(
        harness
            .world
            .guild(guild_id)
            .is_some_and(|guild| guild.storage_slot(3).is_some()),
        "失败不应移走仓库物品"
    );
    assert_eq!(
        harness.world.player(leader).expect("玩家").inventory[10]
            .as_ref()
            .map(|item| item.unique_id),
        Some(blocker_uid)
    );

    // 腾出格子后成功：`Info.Inventory[to] = StoredItems[from].Item; StoredItems[from] = null;`（`:10218-10219`）。
    harness.world.player_mut(leader).expect("玩家").inventory[10] = None;
    harness.clear(leader);
    assert_eq!(
        harness.world.guild_storage_item_change(leader, 1, 3, 10),
        Ok(())
    );
    let packets = harness.packets(leader);
    let changes = item_changes(&packets);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].change_type, 1);
    assert!(changes[0].item.is_none(), "取回包不带 Item（`:10220`）");
    assert_eq!(
        harness.world.player(leader).expect("玩家").inventory[10]
            .as_ref()
            .map(|item| item.unique_id),
        Some(stored_uid)
    );
    assert!(
        harness
            .world
            .guild(guild_id)
            .is_some_and(|guild| guild.storage_slot(3).is_none())
    );
}

#[test]
fn retrieve_without_permission_sends_chat_but_no_failure_packet() {
    let mut harness = Harness::new(5);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 2_000_000);
    let bob = harness.add_player(2, "Bob", 10, 0);
    let guild_id = harness.create_guild(leader, "测试行会");
    harness.invite_and_join(leader, bob);

    // Bob 只有存物品权限（`CanStoreItem`），没有取物品权限（`CanRetrieveItem`）。
    harness.set_rank_option(leader, 1, 3, true);
    harness.give_item(bob, 0, ITEM_INDEX);
    harness
        .world
        .guild_storage_item_change(bob, 0, 0, 7)
        .expect("Bob 应能存入");
    harness.clear(bob);

    // Crystal 在取物品的权限分支里**只发聊天、不发失败包**（`:10186-10191`）。
    assert_eq!(
        harness.world.guild_storage_item_change(bob, 1, 7, 1),
        Err(GuildError::NoRetrievePermission)
    );
    let packets = harness.packets(bob);
    assert!(
        item_changes(&packets).is_empty(),
        "取物品权限失败时不应有 S.GuildStorageItemChange，实际 {packets:?}"
    );
    assert!(
        chats(&packets)
            .iter()
            .any(|(text, kind)| *kind == ChatType::System
                && text
                    == &format_text(
                        Language::ZhCn,
                        TextKey::GuildStorageNoRetrievePermission,
                        &[]
                    )),
        "应提示没有取物品权限，实际 {packets:?}"
    );
    assert!(
        harness
            .world
            .guild(guild_id)
            .is_some_and(|guild| guild.storage_slot(7).is_some()),
        "物品应留在仓库"
    );

    // 打开 `CanRetrieveItem = 16`（`1 << 4`）后可以取回。
    harness.set_rank_option(leader, 1, 4, true);
    harness.clear(bob);
    assert_eq!(
        harness.world.guild_storage_item_change(bob, 1, 7, 1),
        Ok(())
    );
    let packets = harness.packets(bob);
    assert_eq!(item_changes(&packets).len(), 1);
    assert_eq!(item_changes(&packets)[0].change_type, 1);
    assert!(
        harness
            .world
            .guild(guild_id)
            .is_some_and(|guild| guild.storage_slot(7).is_none())
    );
}

#[test]
fn retrieve_when_inventory_is_full_is_rejected() {
    let mut harness = Harness::new(21);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 2_000_000);
    let guild_id = harness.create_guild(leader, "测试行会");
    harness.give_item(leader, 0, ITEM_INDEX);
    harness
        .world
        .guild_storage_item_change(leader, 0, 0, 2)
        .expect("存入");
    let stored_uid = harness
        .world
        .guild(guild_id)
        .and_then(|guild| guild.storage_slot(2))
        .map(|slot| slot.item.item.unique_id)
        .expect("仓库第 2 格");
    assert_eq!(
        harness.world.free_space(leader),
        INVENTORY_SLOTS,
        "存完后背包全空"
    );
    // 把 46 格全部塞满（Crystal `Info.Inventory.Length`，默认 46）。
    for slot in 0..INVENTORY_SLOTS {
        if harness.world.player(leader).expect("玩家").inventory[slot].is_none() {
            harness.give_item(leader, slot, ITEM_INDEX);
        }
    }
    assert_eq!(harness.world.free_space(leader), 0);
    harness.clear(leader);

    // `if (to < 0 || to >= Info.Inventory.Length)` 之后的空位检查：
    // `if (Info.Inventory[to] != null) { ReceiveChat(TargetSlotNotEmpty); Enqueue(p); }`（`:10202-10207`）。
    assert_eq!(
        harness.world.guild_storage_item_change(leader, 1, 2, 0),
        Err(GuildError::TargetSlotNotEmpty)
    );
    let packets = harness.packets(leader);
    let failures = item_changes(&packets);
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].change_type, 4);
    assert!(
        chats(&packets)
            .iter()
            .any(|(text, kind)| *kind == ChatType::System
                && text
                    == &format_text(Language::ZhCn, TextKey::GuildStorageTargetSlotNotEmpty, &[]))
    );
    assert!(
        harness
            .world
            .guild(guild_id)
            .is_some_and(|guild| guild.storage_slot(2).is_some()),
        "背包满时物品应留在仓库"
    );

    // 腾出 45 号格后可以取回。
    harness.world.player_mut(leader).expect("玩家").inventory[45] = None;
    assert_eq!(
        harness.world.guild_storage_item_change(leader, 1, 2, 45),
        Ok(())
    );
    assert_eq!(
        harness.world.player(leader).expect("玩家").inventory[45]
            .as_ref()
            .map(|item| item.unique_id),
        Some(stored_uid)
    );
}

// ---------------------------------------------------------------------------
// 3. 仓库内移动
// ---------------------------------------------------------------------------

#[test]
fn storage_move_swaps_items_and_requires_permission() {
    let mut harness = Harness::new(22);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 2_000_000);
    let bob = harness.add_player(2, "Bob", 10, 0);
    let guild_id = harness.create_guild(leader, "测试行会");
    harness.invite_and_join(leader, bob);
    harness.give_item(leader, 0, ITEM_INDEX);
    harness
        .world
        .guild_storage_item_change(leader, 0, 0, 0)
        .expect("存入 A");
    harness.give_item(leader, 0, ITEM_INDEX);
    harness
        .world
        .guild_storage_item_change(leader, 0, 0, 1)
        .expect("存入 B");
    let uid_a = harness
        .world
        .guild(guild_id)
        .and_then(|guild| guild.storage_slot(0))
        .map(|slot| slot.item.item.unique_id)
        .expect("第 0 格 A");
    let uid_b = harness
        .world
        .guild(guild_id)
        .and_then(|guild| guild.storage_slot(1))
        .map(|slot| slot.item.item.unique_id)
        .expect("第 1 格 B");

    // Bob 的职位没有 `CanStoreItem` → `NoGuildStorageMovePermission` + `Type=5` 失败包（`:10226-10231`）。
    harness.clear(bob);
    assert_eq!(
        harness.world.guild_storage_item_change(bob, 2, 0, 1),
        Err(GuildError::NoStorageMovePermission)
    );
    let packets = harness.packets(bob);
    let failures = item_changes(&packets);
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].change_type, 5);
    assert!(
        chats(&packets)
            .iter()
            .any(|(text, kind)| *kind == ChatType::System
                && text
                    == &format_text(Language::ZhCn, TextKey::GuildStorageNoMovePermission, &[]))
    );

    // 会长可以交换：`StoredItems[to] = StoredItems[from]; StoredItems[from] = q`（`:10252-10258`）。
    harness.clear(leader);
    assert_eq!(
        harness.world.guild_storage_item_change(leader, 2, 0, 1),
        Ok(())
    );
    let guild = harness.world.guild(guild_id).expect("行会");
    assert_eq!(
        guild.storage_slot(0).map(|slot| slot.item.item.unique_id),
        Some(uid_b),
        "目标格原来的物品被换到来源格"
    );
    assert_eq!(
        guild.storage_slot(1).map(|slot| slot.item.item.unique_id),
        Some(uid_a),
        "来源格的物品移动到目标格"
    );
    let packets = harness.packets(leader);
    let changes = item_changes(&packets);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].change_type, 2);
    assert_eq!(
        changes[0].item.as_ref().map(|item| item.item.unique_id),
        Some(uid_a),
        "移动包带的是「移动后目标格」的物品（`:10264`）"
    );
}

// ---------------------------------------------------------------------------
// 4. 仓库列表与节流
// ---------------------------------------------------------------------------

#[test]
fn storage_list_request_is_throttled_until_rejoin() {
    let mut harness = Harness::new(6);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 2_000_000);
    let bob = harness.add_player(2, "Bob", 10, 0);
    let guild_id = harness.create_guild(leader, "测试行会");
    harness.give_item(leader, 0, ITEM_INDEX);
    harness
        .world
        .guild_storage_item_change(leader, 0, 0, 0)
        .expect("存入");

    // `case 3: if (!GuildCanRequestItems) return; GuildCanRequestItems = false; …… Enqueue(new S.GuildStorageList ……);`
    // （`:10267-10279`）。
    harness.clear(leader);
    assert_eq!(
        harness.world.guild_storage_item_change(leader, 3, 0, 0),
        Ok(())
    );
    let packets = harness.packets(leader);
    let lists = storage_lists(&packets);
    assert_eq!(
        lists.len(),
        1,
        "应收到 S.GuildStorageList，实际 {packets:?}"
    );
    assert_eq!(lists[0].items.len(), STORAGE_SLOTS);
    assert!(lists[0].items[0].is_some(), "第 0 格应显示刚存入的物品");
    assert!(lists[0].items[1..].iter().all(Option::is_none));

    // 同一次登录内再次拉取 → 静默丢弃（没有包）。
    harness.clear(leader);
    assert_eq!(
        harness.world.guild_storage_item_change(leader, 3, 0, 0),
        Err(GuildError::StorageRequestThrottled)
    );
    assert!(harness.packets(leader).is_empty(), "节流期间不应有任何回包");

    // 加入行会会重置 `GuildCanRequestItems = true`（`:10006`）；而且拉列表**不要求安全区**（`:10131`）。
    harness.invite_and_join(leader, bob);
    harness.move_outside_safe_zone(bob);
    harness.clear(bob);
    assert_eq!(
        harness.world.guild_storage_item_change(bob, 3, 0, 0),
        Ok(())
    );
    assert_eq!(
        storage_lists(&harness.packets(bob)).len(),
        1,
        "安全区外也能拉仓库列表"
    );
    assert!(
        harness
            .world
            .guild(guild_id)
            .is_some_and(|guild| guild.storage_slot(0).is_some())
    );
}

// ---------------------------------------------------------------------------
// 5. 金币：存入 / 取出 / 上限
// ---------------------------------------------------------------------------

#[test]
fn gold_deposit_and_withdrawal_broadcast_to_the_guild() {
    let mut harness = Harness::new(7);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 2_000_000);
    let bob = harness.add_player(2, "Bob", 10, 50_000);
    let guild_id = harness.create_guild(leader, "测试行会");
    harness.invite_and_join(leader, bob);
    harness.clear(leader);
    harness.clear(bob);

    // 存入：`Account.Gold -= amount; MyGuild.Gold += amount; Enqueue(S.LoseGold); SendServerPacket(Type=0)`（`:10089-10093`）。
    assert_eq!(
        harness.world.guild_storage_gold_change(bob, 0, 10_000),
        Ok(())
    );
    assert_eq!(
        harness.world.player(bob).map(|player| player.gold),
        Some(40_000)
    );
    assert_eq!(
        harness.world.guild(guild_id).map(|guild| guild.gold),
        Some(10_000)
    );
    let packets = harness.packets(bob);
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::LoseGold(gold) if gold.gold == 10_000)),
        "存入者应收到 S.LoseGold，实际 {packets:?}"
    );
    let changes = gold_changes(&packets);
    assert_eq!(changes.len(), 1, "应广播一条金币变动，实际 {packets:?}");
    assert_eq!((changes[0].type_, changes[0].amount), (0, 10_000));
    assert_eq!(changes[0].name, "Bob");
    // 会长也收到广播（`MyGuild.SendServerPacket` 发给全部在线成员）。
    assert_eq!(gold_changes(&harness.packets(leader)).len(), 1);

    // 非会长取钱：`if (MyGuildRank.Index != 0) ReceiveChat(InsufficientRank)`（`:10109-10113`）。
    harness.clear(bob);
    assert_eq!(
        harness.world.guild_storage_gold_change(bob, 1, 1_000),
        Err(GuildError::LeaderOnly)
    );
    let packets = harness.packets(bob);
    assert!(
        gold_changes(&packets).is_empty(),
        "失败时不应广播金币变动，实际 {packets:?}"
    );
    assert!(
        chats(&packets)
            .iter()
            .any(|(text, kind)| *kind == ChatType::System
                && text == &format_text(Language::ZhCn, TextKey::GuildBankLeaderOnly, &[])),
        "应提示只有会长能取，实际 {packets:?}"
    );
    assert_eq!(
        harness.world.guild(guild_id).map(|guild| guild.gold),
        Some(10_000)
    );

    // 会长取钱：`MyGuild.Gold -= amount; GainGold(amount); SendServerPacket(Type=1)`（`:10115-10118`）。
    harness.clear(leader);
    harness.clear(bob);
    assert_eq!(
        harness.world.guild_storage_gold_change(leader, 1, 4_000),
        Ok(())
    );
    assert_eq!(
        harness.world.guild(guild_id).map(|guild| guild.gold),
        Some(6_000)
    );
    assert_eq!(
        harness.world.player(leader).map(|player| player.gold),
        Some(1_000_000 + 4_000),
        "建会扣掉 1,000,000 后又取回 4,000"
    );
    let packets = harness.packets(leader);
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::GainedGold(gold) if gold.gold == 4_000)),
        "取钱者应收到 S.GainedGold，实际 {packets:?}"
    );
    assert_eq!(gold_changes(&packets)[0].type_, 1, "广播的 Type=1 表示取回");
    assert_eq!(gold_changes(&harness.packets(bob))[0].name, "阿甲");

    // 个人金币不足（`:10077-10081`）。
    harness.clear(bob);
    assert_eq!(
        harness.world.guild_storage_gold_change(bob, 0, 100_000),
        Err(GuildError::NotEnoughGold {
            required: 100_000,
            gold: 40_000
        })
    );
    // 行会资金不足（`:10097-10101`）。
    assert!(matches!(
        harness.world.guild_storage_gold_change(bob, 1, 7_000),
        Err(GuildError::GuildBankNotEnough { .. })
    ));
}

#[test]
fn gold_deposit_is_capped_at_uint_max() {
    let mut harness = Harness::new(8);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 2_000_000);
    let guild_id = harness.create_guild(leader, "测试行会");
    harness.world.player_mut(leader).expect("玩家").gold = i64::from(u32::MAX);

    // `if ((MyGuild.Gold + (ulong)amount) > uint.MaxValue)`（`:10083-10087`）：正好等于上限允许。
    assert_eq!(
        harness.world.guild_storage_gold_change(leader, 0, u32::MAX),
        Ok(())
    );
    assert_eq!(
        harness.world.guild(guild_id).map(|guild| guild.gold),
        Some(u32::MAX)
    );
    assert_eq!(
        harness.world.player(leader).map(|player| player.gold),
        Some(0)
    );

    // 再存 1 金币就超过上限 → `GuildGoldLimitReached`。
    harness.world.player_mut(leader).expect("玩家").gold = 1;
    harness.clear(leader);
    assert_eq!(
        harness.world.guild_storage_gold_change(leader, 0, 1),
        Err(GuildError::GuildGoldLimitReached)
    );
    assert!(
        chats(&harness.packets(leader))
            .iter()
            .any(|(text, kind)| *kind == ChatType::System
                && text == &format_text(Language::ZhCn, TextKey::GuildGoldLimitReached, &[]))
    );
    assert_eq!(
        harness.world.guild(guild_id).map(|guild| guild.gold),
        Some(u32::MAX)
    );
}

#[test]
fn gold_withdrawal_respects_the_player_gold_cap() {
    let mut harness = Harness::new(9);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 2_000_000);
    let guild_id = harness.create_guild(leader, "测试行会");
    assert_eq!(
        harness.world.guild_storage_gold_change(leader, 0, 100),
        Ok(())
    );

    // `if (!CanGainGold(amount))` → `(ulong)gold + Account.Gold <= uint.MaxValue`（`:10103-10107`、`:7638-7641`）。
    harness.world.player_mut(leader).expect("玩家").gold = i64::from(u32::MAX) - 5;
    harness.clear(leader);
    assert_eq!(
        harness.world.guild_storage_gold_change(leader, 1, 10),
        Err(GuildError::PlayerGoldLimitReached)
    );
    assert!(
        chats(&harness.packets(leader))
            .iter()
            .any(|(text, kind)| *kind == ChatType::System
                && text == &format_text(Language::ZhCn, TextKey::GuildPlayerGoldLimitReached, &[]))
    );
    assert_eq!(
        harness.world.guild(guild_id).map(|guild| guild.gold),
        Some(100),
        "失败不应扣行会资金"
    );
}

// ---------------------------------------------------------------------------
// 6. 行会 buff：权限 / 等级 / 点数 / 金币
// ---------------------------------------------------------------------------

#[test]
fn guild_buff_purchase_requires_permission_and_a_known_buff() {
    let mut harness = Harness::new(10);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 2_000_000);
    let bob = harness.add_player(2, "Bob", 10, 0);
    harness.create_guild(leader, "测试行会");
    harness.invite_and_join(leader, bob);
    harness.level_guild(leader);
    harness.clear(bob);

    // `if (!MyGuildRank.Options.HasFlag(GuildRankOptions.CanActivateBuff)) ReceiveChat(NoCorrectGuildRank)`
    // （`:10362-10366`）。
    assert_eq!(
        harness.world.guild_buff_update(bob, 1, 1),
        Err(GuildError::NoBuffPermission)
    );
    assert!(
        chats(&harness.packets(bob))
            .iter()
            .any(|(text, kind)| *kind == ChatType::System
                && text == &format_text(Language::ZhCn, TextKey::GuildBuffNoPermission, &[]))
    );

    // 打开 `CanActivateBuff = 128`（`1 << 7`）后，未知编号 → `BuffNotExist`（`:10367-10372`）。
    harness.set_rank_option(leader, 1, 7, true);
    harness.clear(bob);
    assert_eq!(
        harness.world.guild_buff_update(bob, 1, 999),
        Err(GuildError::BuffNotExist(999))
    );
    assert!(
        chats(&harness.packets(bob))
            .iter()
            .any(|(text, kind)| *kind == ChatType::System
                && text
                    == &format_text(
                        Language::ZhCn,
                        TextKey::GuildBuffNotExist,
                        &[("buff", "999")]
                    ))
    );
}

#[test]
fn guild_buff_purchase_checks_level_points_and_bank_gold() {
    let mut harness = Harness::new(11);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 2_000_000);
    let guild_id = harness.create_guild(leader, "测试行会");

    // 等级不足（`if (MyGuild.Info.Level < BuffInfo.LevelRequirement)`，`:10378-10382`）。
    assert_eq!(
        harness.world.guild_buff_update(leader, 1, 2),
        Err(GuildError::GuildLevelTooLow {
            required: 3,
            level: 0
        })
    );

    // 升到 5 级：5 个升级点。点数不足（`:10383-10387`）。
    harness.level_guild(leader);
    assert_eq!(
        harness.world.guild_buff_update(leader, 1, 3),
        Err(GuildError::SparePointsInsufficient {
            required: 6,
            remaining: 5
        })
    );

    // 金币不足（`requiresGold = TimeLimit > 0 && ActivationCost > 0`，`:10389-10398`）。
    assert_eq!(
        harness.world.guild_buff_update(leader, 1, 1),
        Err(GuildError::GuildBankNotEnough {
            required: 500,
            gold: 0
        })
    );

    // 存入 500 行会资金后购买成功。
    assert_eq!(
        harness.world.guild_storage_gold_change(leader, 0, 500),
        Ok(())
    );
    harness.clear(leader);
    assert_eq!(harness.world.guild_buff_update(leader, 1, 1), Ok(()));
    let guild = harness.world.guild(guild_id).expect("行会");
    assert!(
        guild.buff(1).is_some_and(|buff| buff.active),
        "购买后增益应立刻激活"
    );
    assert_eq!(guild.spare_points, 4, "扣掉 1 点升级点");
    assert_eq!(guild.gold, 0, "扣掉 500 金币");

    // 包序：`S.GuildStorageGoldChange{Type=2}` → `S.GuildBuffList{ActiveBuffs}` → `S.GuildStatus`
    // （`GuildObject.cs:952-978`）。
    let packets = harness.packets(leader);
    let gold_index = packets
        .iter()
        .position(|packet| matches!(packet, ServerPacket::GuildStorageGoldChange(_)))
        .expect("应有金币包");
    let buff_index = packets
        .iter()
        .position(|packet| matches!(packet, ServerPacket::GuildBuffList(_)))
        .expect("应有增益包");
    let status_index = packets
        .iter()
        .position(|packet| matches!(packet, ServerPacket::GuildStatus(_)))
        .expect("应有行会状态包");
    assert!(
        gold_index < buff_index && buff_index < status_index,
        "包序应为 金币 → 增益列表 → 行会状态，实际 {packets:?}"
    );
    let lists = buff_lists(&packets);
    assert_eq!(lists[0].active_buffs.len(), 1);
    assert_eq!(lists[0].active_buffs[0].id, 1);
    assert!(lists[0].active_buffs[0].active);
    assert_eq!(lists[0].active_buffs[0].active_time_remaining, 2);
    assert_eq!(statuses(&packets)[0].spare_points, 4);
    // 成功消息走**行会频道**（`MyGuild.SendMessage`，`:10404`）。
    assert!(
        chats(&packets)
            .iter()
            .any(|(text, kind)| *kind == ChatType::Guild
                && text
                    == &format_text(
                        Language::ZhCn,
                        TextKey::GuildBuffBoughtPointsGold,
                        &[
                            ("player", "阿甲"),
                            ("points", "1"),
                            ("gold", "500"),
                            ("buff", "体力祝福")
                        ]
                    )),
        "应广播购买成功消息，实际 {packets:?}"
    );

    // 重复购买 → `BuffAlreadyObtained`（`:10373-10377`）。
    assert_eq!(
        harness.world.guild_buff_update(leader, 1, 1),
        Err(GuildError::BuffAlreadyObtained(1))
    );

    // 免费的增益（永久、无金币）也可以买，升级点照扣。
    assert_eq!(harness.world.guild_buff_update(leader, 1, 2), Ok(()));
    assert_eq!(
        harness
            .world
            .guild(guild_id)
            .map(|guild| guild.spare_points),
        Some(2)
    );
}

#[test]
fn guild_level_up_awards_spare_points_that_buffs_consume() {
    let mut harness = Harness::new(12);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 2_000_000);
    let guild_id = harness.create_guild(leader, "测试行会");
    assert_eq!(
        harness
            .world
            .guild(guild_id)
            .map(|guild| guild.spare_points),
        Some(0)
    );

    // `Info.SparePoints = (byte)Math.Min(byte.MaxValue, Info.SparePoints + Settings.Guild_PointPerLevel)`（`:660`）。
    harness.level_guild(leader);
    let guild = harness.world.guild(guild_id).expect("行会");
    assert_eq!(guild.level, 5);
    assert_eq!(guild.spare_points, 5);
    assert_eq!(guild.experience, 500, "1000 - 5 × 100 的余数");
    assert_eq!(guild.max_experience, 0, "经验表到 5 级结束");

    // 已达到经验表顶：再加经验不会升级也不会加分（`if (Info.MaxExperience <= 0) return;`，`:647`）。
    harness.world.guild_gain_exp(leader, 1_000_000);
    let guild = harness.world.guild(guild_id).expect("行会");
    assert_eq!(guild.level, 5);
    assert_eq!(guild.spare_points, 5);

    // 买两个增益消耗 1 + 2 点。
    assert_eq!(harness.world.guild_buff_update(leader, 1, 2), Ok(()));
    assert_eq!(
        harness
            .world
            .guild(guild_id)
            .map(|guild| guild.spare_points),
        Some(3)
    );
}

// ---------------------------------------------------------------------------
// 7. 行会 buff：全员加成 / 到期 / 重新激活
// ---------------------------------------------------------------------------

#[test]
fn guild_buff_applies_stats_to_all_online_members_and_expires() {
    let mut harness = Harness::new(13);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 2_000_000);
    let bob = harness.add_player(2, "Bob", 10, 0);
    let guild_id = harness.create_guild(leader, "测试行会");
    harness.invite_and_join(leader, bob);
    harness.level_guild(leader);
    assert_eq!(
        harness.world.guild_storage_gold_change(leader, 0, 500),
        Ok(())
    );
    let leader_base_hp = combat::player_base_stats(LEADER_LEVEL, MirClass::Warrior).hp;
    let bob_base_hp = combat::player_base_stats(10, MirClass::Warrior).hp;
    harness.clear(leader);
    harness.clear(bob);

    assert_eq!(harness.world.guild_buff_update(leader, 1, 1), Ok(()));
    // `RefreshGuildBuffs`（`:1824-1836`）→ 所有在线成员 +50 HP。
    assert_eq!(
        harness.world.player(leader).map(|player| player.stats.hp),
        Some(leader_base_hp + 50)
    );
    assert_eq!(
        harness.world.player(bob).map(|player| player.stats.hp),
        Some(bob_base_hp + 50)
    );
    // 上线成员都应收到「新增益」列表。
    let packets = harness.packets(bob);
    let lists = buff_lists(&packets);
    assert_eq!(lists.len(), 1, "Bob 应收到增益列表，实际 {packets:?}");
    assert_eq!(lists[0].active_buffs.len(), 1);
    assert_eq!(lists[0].active_buffs[0].id, 1);
    assert!(lists[0].active_buffs[0].active);
    assert_eq!(lists[0].guild_buffs.len(), 0, "激活包不带定义列表");
    assert_eq!(
        statuses(&packets).last().map(|status| status.spare_points),
        Some(4)
    );

    // 每分钟 `ActiveTimeRemaining -= 1`，`< 0` 才失效（`GuildObject.cs:806-814`）；TimeLimit = 2。
    assert_eq!(harness.world.process_guild_buffs(), 0, "第 1 分钟仍激活");
    harness.clear(leader);
    harness.clear(bob);
    assert_eq!(harness.world.process_guild_buffs(), 0, "第 2 分钟仍激活");
    assert!(
        harness
            .world
            .guild(guild_id)
            .and_then(|guild| guild.buff(1))
            .is_some_and(|buff| buff.active && buff.active_time_remaining == 0)
    );
    harness.clear(leader);
    harness.clear(bob);
    assert_eq!(harness.world.process_guild_buffs(), 1, "第 3 分钟到期");

    let guild = harness.world.guild(guild_id).expect("行会");
    assert!(
        guild
            .buff(1)
            .is_some_and(|buff| !buff.active && buff.active_time_remaining == -1)
    );
    // 到期广播 `S.GuildBuffList{ActiveBuffs=[Active=false]}`（`:817-825`）。
    for who in [leader, bob] {
        let packets = harness.packets(who);
        let lists = buff_lists(&packets);
        assert_eq!(
            lists.len(),
            1,
            "玩家 {who} 应收到到期广播，实际 {packets:?}"
        );
        assert_eq!(lists[0].active_buffs.len(), 1);
        assert_eq!(lists[0].active_buffs[0].id, 1);
        assert!(!lists[0].active_buffs[0].active);
    }
    // `RefreshAllStats()` 撤掉加成（`:824`）。
    assert_eq!(
        harness.world.player(leader).map(|player| player.stats.hp),
        Some(leader_base_hp)
    );
    assert_eq!(
        harness.world.player(bob).map(|player| player.stats.hp),
        Some(bob_base_hp)
    );
    // 本作修正：到期会排一条落盘（Crystal `Process` 不置 NeedSave）。
    let writes = harness.world.take_guild_writes();
    assert!(
        writes.iter().any(|write| matches!(
            write,
            mir_server::GuildWrite::Save(record)
                if record.id == guild_id && record.buffs.iter().any(|buff| !buff.active)
        )),
        "到期状态应排入落盘队列，实际 {writes:?}"
    );
}

#[test]
fn guild_buff_reactivation_charges_gold_again() {
    let mut harness = Harness::new(14);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 2_000_000);
    let guild_id = harness.create_guild(leader, "测试行会");
    harness.level_guild(leader);
    assert_eq!(
        harness.world.guild_storage_gold_change(leader, 0, 500),
        Ok(())
    );
    assert_eq!(harness.world.guild_buff_update(leader, 1, 1), Ok(()));
    // 让它到期（TimeLimit = 2 → 第 3 次结算失效）。
    for _ in 0..3 {
        harness.world.process_guild_buffs();
    }
    assert!(
        harness
            .world
            .guild(guild_id)
            .and_then(|guild| guild.buff(1))
            .is_some_and(|buff| !buff.active)
    );

    // 金币不足：静默返回（`if ((MyGuild.Gold < Buff.Info.ActivationCost) || (Buff.Active)) return;`，`:10432`）。
    harness.clear(leader);
    assert_eq!(harness.world.guild_buff_update(leader, 2, 1), Ok(()));
    assert!(
        harness.packets(leader).is_empty(),
        "金币不足时不应有任何回包"
    );
    assert!(
        harness
            .world
            .guild(guild_id)
            .and_then(|guild| guild.buff(1))
            .is_some_and(|buff| !buff.active)
    );

    // 补足金币后重新激活：`S.GuildBuffList` → `S.GuildStorageGoldChange`（`:1011-1012`）。
    assert_eq!(
        harness.world.guild_storage_gold_change(leader, 0, 500),
        Ok(())
    );
    harness.clear(leader);
    assert_eq!(harness.world.guild_buff_update(leader, 2, 1), Ok(()));
    let guild = harness.world.guild(guild_id).expect("行会");
    assert!(
        guild
            .buff(1)
            .is_some_and(|buff| buff.active && buff.active_time_remaining == 2)
    );
    assert_eq!(guild.gold, 0);
    let packets = harness.packets(leader);
    let buff_index = packets
        .iter()
        .position(|packet| matches!(packet, ServerPacket::GuildBuffList(_)))
        .expect("应有增益列表");
    let gold_index = packets
        .iter()
        .position(|packet| matches!(packet, ServerPacket::GuildStorageGoldChange(_)))
        .expect("应有金币包");
    assert!(buff_index < gold_index, "激活包的顺序是先列表、后金币");
    assert_eq!(gold_changes(&packets)[0].type_, 2, "激活的花费走 Type=2");

    // 已激活时再激活：静默返回（`:10432`）。
    harness.clear(leader);
    assert_eq!(harness.world.guild_buff_update(leader, 2, 1), Ok(()));
    assert!(harness.packets(leader).is_empty());

    // 没买过的增益：`BuffNotObtained`（`:10426-10431`）。
    assert_eq!(
        harness.world.guild_buff_update(leader, 2, 3),
        Err(GuildError::BuffNotObtained(3))
    );
}

// ---------------------------------------------------------------------------
// 8. 登录 / 加入行会时的增益列表
// ---------------------------------------------------------------------------

#[test]
fn guild_buff_info_list_is_pushed_at_login_and_on_demand() {
    let mut harness = Harness::new(15);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 2_000_000);
    let bob = harness.add_player(2, "Bob", 10, 0);
    let guild_id = harness.create_guild(leader, "测试行会");
    harness.invite_and_join(leader, bob);

    // 登录：Crystal `StartGame` 无条件发 `S.GuildBuffList{GuildBuffs=全部}`（`:1223-1224`）。
    harness.clear(leader);
    harness.world.guild_player_online(leader);
    let packets = harness.packets(leader);
    let lists = buff_lists(&packets);
    assert_eq!(lists.len(), 1, "登录应收到可选列表，实际 {packets:?}");
    let ids: Vec<i32> = lists[0].guild_buffs.iter().map(|info| info.id).collect();
    assert_eq!(ids, vec![1, 2, 3]);
    assert_eq!(lists[0].guild_buffs[0].name, "体力祝福");
    assert_eq!(lists[0].guild_buffs[0].time_limit, 2);
    assert_eq!(lists[0].guild_buffs[0].activation_cost, 500);
    assert_eq!(
        lists[0].guild_buffs[0].stats.get(Stat::HP),
        50,
        "增益定义里的属性要原样下发"
    );
    assert!(lists[0].active_buffs.is_empty(), "行会还没有激活的增益");

    // `RequestedGuildBuffInfo` 已经置 true（`:1224`），`Action = 0` 不再回（`:10358`）。
    harness.clear(leader);
    assert_eq!(harness.world.guild_buff_update(leader, 0, 0), Ok(()));
    assert!(harness.packets(leader).is_empty());

    // Bob 是加入行会的（没走登录流程），`Action = 0` 会回列表。
    harness.clear(bob);
    assert_eq!(harness.world.guild_buff_update(bob, 0, 0), Ok(()));
    assert_eq!(buff_lists(&harness.packets(bob)).len(), 1);

    // 没有行会的玩家登录也会收到可选列表（Crystal 在 StartGame 里无条件发）。
    let carol = harness.add_player(3, "Carol", 10, 0);
    harness.clear(carol);
    harness.world.guild_player_online(carol);
    let packets = harness.packets(carol);
    let lists = buff_lists(&packets);
    assert_eq!(lists.len(), 1, "无行会玩家也应收到可选列表");
    assert_eq!(lists[0].guild_buffs.len(), 3);
    assert!(
        harness.world.guild_of(carol).is_none(),
        "只是发了列表，没有加入行会"
    );

    // id < 0 静默返回（`:10354`）。
    harness.clear(bob);
    assert_eq!(harness.world.guild_buff_update(bob, 1, -1), Ok(()));
    assert!(harness.packets(bob).is_empty());
    assert_eq!(
        harness.world.guild(guild_id).map(|guild| guild.buffs.len()),
        Some(0)
    );
}

#[test]
fn joining_a_guild_pushes_its_active_buffs() {
    let mut harness = Harness::new(16);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 2_000_000);
    let bob = harness.add_player(2, "Bob", 10, 0);
    let guild_id = harness.create_guild(leader, "测试行会");
    harness.level_guild(leader);
    // 免费的永久增益：+5 MaxDC。
    assert_eq!(harness.world.guild_buff_update(leader, 1, 2), Ok(()));

    harness.clear(bob);
    harness.invite_and_join(leader, bob);
    let packets = harness.packets(bob);
    let lists = buff_lists(&packets);
    assert_eq!(lists.len(), 1, "加入行会应下发已有增益，实际 {packets:?}");
    assert_eq!(lists[0].active_buffs.len(), 1);
    assert_eq!(lists[0].active_buffs[0].id, 2);
    // `RefreshStats(); if (MyGuild.BuffList.Count > 0) Enqueue(...)`（`:10008-10010`）。
    let base = combat::player_base_stats(10, MirClass::Warrior).max_dc;
    assert_eq!(
        harness.world.player(bob).map(|player| player.stats.max_dc),
        Some(base + 5)
    );
    assert!(
        harness
            .world
            .guild(guild_id)
            .and_then(|guild| guild.buff(2))
            .is_some()
    );
}

// ---------------------------------------------------------------------------
// 9. 被踢出后仓库物品的归属
// ---------------------------------------------------------------------------

#[test]
fn kicked_member_cannot_retrieve_but_another_member_can() {
    let mut harness = Harness::new(17);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 2_000_000);
    let bob = harness.add_player(2, "Bob", 10, 0);
    let guild_id = harness.create_guild(leader, "测试行会");
    harness.invite_and_join(leader, bob);
    harness.set_rank_option(leader, 1, 3, true);
    harness.give_item(bob, 0, ITEM_INDEX);
    assert_eq!(
        harness.world.guild_storage_item_change(bob, 0, 0, 4),
        Ok(())
    );
    let stored_uid = harness
        .world
        .guild(guild_id)
        .and_then(|guild| guild.storage_slot(4))
        .map(|slot| slot.item.item.unique_id)
        .expect("仓库第 4 格");
    assert_eq!(
        harness
            .world
            .guild(guild_id)
            .and_then(|guild| guild.storage_slot(4))
            .map(|slot| slot.item.user_id),
        Some(i64::from(bob)),
        "物品仍记 Bob 为存放者"
    );

    // 踢人：`ChangeType = 1`、`Name = Bob`（`PlayerObject.cs:9849-9855` → `GuildObject.DeleteMember`）。
    let kick = GuildMemberEdit {
        change_type: 1,
        rank_index: 0,
        name: "Bob".to_owned(),
        rank_name: String::new(),
    };
    harness
        .world
        .guild_edit_member(leader, &kick)
        .expect("踢人");
    harness.clear(bob);
    assert!(harness.world.guild_of(bob).is_none(), "被踢后没有行会");

    // 被踢者取物品：`if ((MyGuild == null) || (MyGuildRank == null)) Enqueue(p); ReceiveChat(NotPartOfGuild)`
    // （`:10124-10129`）。
    assert_eq!(
        harness.world.guild_storage_item_change(bob, 1, 4, 1),
        Err(GuildError::NotInGuild(bob))
    );
    let packets = harness.packets(bob);
    let failures = item_changes(&packets);
    assert_eq!(failures.len(), 1, "应收到失败回执，实际 {packets:?}");
    assert_eq!(failures[0].change_type, 4);
    assert!(
        chats(&packets)
            .iter()
            .any(|(text, kind)| *kind == ChatType::System
                && text == &format_text(Language::ZhCn, TextKey::GuildNotInGuild, &[])),
        "应提示不在行会，实际 {packets:?}"
    );
    assert!(
        harness
            .world
            .guild(guild_id)
            .is_some_and(|guild| guild.storage_slot(4).is_some()),
        "被踢后被存物品仍留在行会仓库"
    );

    // Crystal **不检查存放者**：任何有 `CanRetrieveItem` 的成员都能取走（会长 options = 255）。
    harness.clear(leader);
    assert_eq!(
        harness.world.guild_storage_item_change(leader, 1, 4, 5),
        Ok(())
    );
    assert_eq!(
        harness.world.player(leader).expect("玩家").inventory[5]
            .as_ref()
            .map(|item| item.unique_id),
        Some(stored_uid)
    );
    assert!(
        harness
            .world
            .guild(guild_id)
            .is_some_and(|guild| guild.storage_slot(4).is_none())
    );
}

// ---------------------------------------------------------------------------
// 10. 重启：仓库与增益从存储恢复（MemoryStore + 编解码往返）
// ---------------------------------------------------------------------------

#[tokio::test]
async fn storage_and_buffs_survive_a_store_roundtrip() {
    let store = MemoryStore::new();
    let account = store
        .create_account("tester", "secret", "")
        .await
        .expect("建号");
    let character = store
        .create_character(
            account.id,
            "阿甲",
            MirClass::Warrior,
            MirGender::Male,
            "0",
            (10, 10),
        )
        .await
        .expect("建角");

    let mut harness = Harness::new(18);
    let leader = harness.add_player_as(1, character.id, "阿甲", LEADER_LEVEL, 2_000_000);
    let guild_id = harness.create_guild(leader, "测试行会");
    harness.give_item(leader, 0, ITEM_INDEX);
    let stored_uid = harness.world.player(leader).expect("玩家").inventory[0]
        .as_ref()
        .map(|item| item.unique_id)
        .expect("背包物品");
    assert_eq!(
        harness.world.guild_storage_item_change(leader, 0, 0, 7),
        Ok(())
    );
    harness.level_guild(leader);
    assert_eq!(harness.world.guild_buff_update(leader, 1, 2), Ok(()));
    harness.flush(&store).await;

    // 落库视图：一格仓库（字节块非空）+ 一条永久增益。
    let rows = store.guilds().await;
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.id, guild_id);
    assert_eq!(row.storage.len(), 1);
    assert_eq!(row.storage[0].slot, 7);
    assert_eq!(row.storage[0].user_id, character.id);
    assert!(!row.storage[0].item.is_empty(), "物品字节块应写入");
    assert_eq!(row.buffs.len(), 1);
    assert_eq!(row.buffs[0].id, 2);
    assert!(row.buffs[0].active);
    assert_eq!(
        store.character_guild_index(character.id).await,
        Some(guild_id)
    );

    // 「重启」：新的世界从同一份记录启动（`Envir.LoadGuilds`，`Envir.cs:3089-3117`）。
    let mut restarted = Harness::new(19);
    assert_eq!(restarted.world.load_guilds(rows), 1);
    let leader = restarted.add_player_as(1, character.id, "阿甲", LEADER_LEVEL, 0);
    // 登录流程：`guild_bind_player` + `RefreshStats`（Crystal 构造里的 `RefreshStats`，`:232-240`）。
    restarted.world.guild_player_online(leader);
    assert_eq!(
        restarted
            .world
            .guild_ref_of(leader)
            .map(|reference| reference.guild_id),
        Some(guild_id)
    );

    let guild = restarted.world.guild(guild_id).expect("行会");
    let slot = guild.storage_slot(7).expect("仓库第 7 格应恢复");
    assert_eq!(slot.item.item.unique_id, stored_uid);
    assert_eq!(slot.item.item.item_index, ITEM_INDEX);
    assert_eq!(slot.item.user_id, character.id);
    assert!(slot.stored_at > 0, "stored_at 也应恢复");
    assert!(
        guild.buff(2).is_some_and(|buff| buff.active),
        "增益应从存储恢复"
    );
    let base = combat::player_base_stats(LEADER_LEVEL, MirClass::Warrior).max_dc;
    assert_eq!(
        restarted
            .world
            .player(leader)
            .map(|player| player.stats.max_dc),
        Some(base + 5),
        "恢复的增益要参与属性计算"
    );
}

// ---------------------------------------------------------------------------
// 11. `[Buff-N]` 解析（合成文本 + 真实数据冒烟）
// ---------------------------------------------------------------------------

#[test]
fn guild_settings_parses_buff_sections_with_crystal_defaults() {
    // `Settings.LoadGuildSettings`：`TotalBuffs` 决定条数，缺段也照样 `Add(default)`
    // （`Server/Settings.cs:1371-1375`）；`PointsReq` 缺省是 1（`Shared/Data/GuildData.cs:98`）。
    let text = "[Guilds]\nMinimumLevel=22\nTotalBuffs=2\n\
                [Buff-0]\nId=7\nIcon=42\nName=DC Buff\nLevelReq=3\nPointsReq=2\n\
                TimeLimit=60\nActivationCost=1234\nBuffDc=3\nBuffMaxHp=100\n\
                BuffDcX=9\n[Buff-9]\nId=9\n";
    let settings = GuildSettings::parse_ini(text);
    assert_eq!(settings.buff_list.len(), 2);
    let first = &settings.buff_list[0];
    assert_eq!(first.id, 7);
    assert_eq!(first.icon, 42);
    assert_eq!(first.name, "DC Buff");
    assert_eq!(first.level_requirement, 3);
    assert_eq!(first.points_requirement, 2);
    assert_eq!(first.time_limit, 60, "单位是分钟，原样保存");
    assert_eq!(first.activation_cost, 1234);
    assert_eq!(first.stats.get(Stat::MaxDC), 3);
    assert_eq!(first.stats.get(Stat::HP), 100);
    // 不认识的键 `BuffDcX` 被忽略。
    assert_eq!(first.stats.len(), 2);
    // `[Buff-1]` 缺段 → Crystal 仍会加一条默认项；本作照抄。
    let second = &settings.buff_list[1];
    assert_eq!(second.id, 0);
    assert_eq!(second.name, "");
    assert_eq!(second.points_requirement, 1);
    assert!(second.stats.is_empty());
    // 按编号查定义（`Envir.FindGuildBuffInfo`）。
    assert_eq!(
        settings.guild_buff_info(7).map(|info| info.name.as_str()),
        Some("DC Buff")
    );
    assert!(
        settings.guild_buff_info(9).is_none(),
        "[Buff-9] 不在 0..TotalBuffs 里"
    );
}

// ---------------------------------------------------------------------------
// 12. 真实数据冒烟：Configs/GuildSettings.ini 的 buff 列表
// ---------------------------------------------------------------------------

#[test]
fn real_guild_settings_buff_list_can_be_activated() {
    // 与 `GuildSettings::discover` 相同的候选顺序。`cargo test` 的工作目录是
    // `crates/mir-server`，所以先按仓库根（`CARGO_MANIFEST_DIR/../..`）解析一遍。
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = mir_server::guild::GUILD_SETTINGS_CANDIDATES
        .iter()
        .flat_map(|candidate| {
            let relative = Path::new(candidate);
            [relative.to_path_buf(), workspace.join(relative)]
        })
        .find(|path| path.is_file());
    // CI 上没有 `resource/`（被 gitignore），真实数据冒烟按约定跳过而不是失败。
    let Some(path) = path else {
        eprintln!("跳过：仓库里没有 GuildSettings.ini（resource/ 被 gitignore）");
        return;
    };
    let settings =
        GuildSettings::load(&path).unwrap_or_else(|| panic!("{} 应能解析", path.display()));
    assert!(
        !settings.buff_list.is_empty(),
        "{} 里应解析出至少一个 [Buff-N]",
        path.display()
    );
    println!(
        "{}: {} 个行会 buff（MinimumLevel={} ExpRate={} PointPerLevel={}）",
        path.display(),
        settings.buff_list.len(),
        settings.required_level,
        settings.exp_rate,
        settings.point_per_level
    );
    for info in &settings.buff_list {
        println!(
            "  id={} icon={} name={:?} level={} points={} time={} cost={} stats={:?}",
            info.id,
            info.icon,
            info.name,
            info.level_requirement,
            info.points_requirement,
            info.time_limit,
            info.activation_cost,
            info.stats.values()
        );
    }
    assert!(
        settings.buff_list.iter().any(|info| !info.stats.is_empty()),
        "真实数据里应至少有一个带属性的 buff"
    );
    // 至少有一个「等级 0、1 个升级点、带属性」的可激活 buff：行会 1 级就有 1 点。
    let pick = settings
        .buff_list
        .iter()
        .find(|info| {
            info.level_requirement == 0 && info.points_requirement <= 1 && !info.stats.is_empty()
        })
        .cloned()
        .expect("真实数据里应有 1 点即可激活且带属性的 buff");

    let mut harness = Harness::with_settings(20, settings);
    let leader = harness.add_player(1, "阿甲", LEADER_LEVEL, 2_000_000);
    let guild_id = harness.create_guild(leader, "冒烟行会");
    // 真实经验表 Level-0=1000000：打 1000001 经验刚好升 1 级，`PointPerLevel=1` 给 1 点。
    harness.world.guild_gain_exp(leader, 1_000_001);
    let guild = harness.world.guild(guild_id).expect("行会");
    assert_eq!(guild.level, 1);
    assert_eq!(guild.spare_points, 1);

    assert_eq!(
        harness.world.guild_buff_update(leader, 1, pick.id),
        Ok(()),
        "真实数据里的 1 点 buff 应可购买"
    );
    let guild = harness.world.guild(guild_id).expect("行会");
    assert!(
        guild.buff(pick.id).is_some_and(|buff| buff.active),
        "购买后增益应激活"
    );
    assert_eq!(guild.spare_points, 0);
    println!("已激活真实数据 buff: id={} name={:?}", pick.id, pick.name);
}
