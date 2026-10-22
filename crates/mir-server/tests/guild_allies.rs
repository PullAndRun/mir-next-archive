//! 行会盟友名单包（`S.GuildAllies`，本项目扩展，索引 303）的端到端测试。
//!
//! 覆盖任务 t72 的四条要求：
//! 1. 结盟 / 解除 / 行会解散后，给**双方在线成员**整表下发；
//! 2. 登录/进世界时，有行会的玩家补发一次；
//! 3. 没有行会的玩家**不**下发；
//! 4. 包本身的 encode/decode 往返（另外两条在 `mir-protocol` 的 manual 单测里）。
//!
//! 全部确定性：内存合成地图 + 内存数据库 + 固定随机种子，不依赖 `resource/`。

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{MirClass, MirDirection, MirGender};
use mir_db::{MapInfo, MirDb};
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::GuildAllies;
use mir_server::gamedata::GameData;
use mir_server::{GuildSettings, NpcState, Player, World, combat};
use mir_world::{Map, MapCell, MapVersion};
use tokio::sync::mpsc;

const MAP: &str = "0";
const MAP_INDEX: i32 = 10;
const LEADER_LEVEL: u16 = 30;
const CREATION_GOLD: i64 = 1_000_000;

fn test_settings() -> GuildSettings {
    GuildSettings {
        required_level: 22,
        creation_gold: 1_000_000,
        exp_rate: 0.01,
        point_per_level: 1,
        experience_list: vec![100, 200, -1],
        member_cap_list: vec![5, 5, 10],
        buff_list: Vec::new(),
    }
}

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

fn base_db() -> MirDb {
    MirDb {
        maps: vec![MapInfo {
            index: MAP_INDEX,
            file_name: MAP.to_owned(),
            title: "比奇省".to_owned(),
            ..MapInfo::default()
        }],
        ..MirDb::default()
    }
}

struct Harness {
    world: World,
    receivers: HashMap<u32, mpsc::UnboundedReceiver<Vec<u8>>>,
}

impl Harness {
    fn new() -> Self {
        let mut world = World::new(999);
        world.seed_rng(0x5EED);
        world.insert_map(MAP, synthetic_map());
        world.set_game_data(Arc::new(GameData::from_db(base_db(), None)));
        world.set_guild_settings(test_settings());
        Self {
            world,
            receivers: HashMap::new(),
        }
    }

    fn add_player(&mut self, object_id: u32, name: &str) -> u32 {
        let stats = combat::player_base_stats(LEADER_LEVEL, MirClass::Warrior);
        let (sender, receiver) = mpsc::unbounded_channel();
        self.world.enter(Player {
            object_id,
            session_id: u64::from(object_id),
            character_id: i64::from(object_id),
            name: name.to_owned(),
            map: MAP.to_owned(),
            position: Point::new(10, 10),
            direction: MirDirection::Down,
            level: LEADER_LEVEL,
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
            gold: CREATION_GOLD,
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

    /// 只取 `S.GuildAllies`（同一用例里别的包不关心）。
    fn allies(&mut self, object_id: u32) -> Vec<GuildAllies> {
        self.packets(object_id)
            .into_iter()
            .filter_map(|packet| match packet {
                ServerPacket::GuildAllies(allies) => Some(allies),
                _ => None,
            })
            .collect()
    }

    fn create_guild(&mut self, leader: u32, name: &str) -> i32 {
        self.world
            .create_guild(leader, name)
            .unwrap_or_else(|error| panic!("创建行会 {name} 应成功：{error}"))
    }
}

#[test]
fn forming_an_alliance_broadcasts_the_same_table_to_both_guilds() {
    let mut harness = Harness::new();
    let a = harness.add_player(1, "甲会长");
    let b = harness.add_player(2, "乙会长");
    let guild_a = harness.create_guild(a, "沙巴克");
    let guild_b = harness.create_guild(b, "比奇城");
    harness.clear(a);
    harness.clear(b);

    harness
        .world
        .form_guild_alliance(guild_a, guild_b)
        .expect("结盟应成功");

    let a_tables = harness.allies(a);
    let b_tables = harness.allies(b);
    assert_eq!(a_tables.len(), 1, "甲方成员应收到 1 次 GuildAllies");
    assert_eq!(b_tables.len(), 1, "乙方成员应收到 1 次 GuildAllies");
    // 两边看到的是同一份「对方」信息。
    assert_eq!(a_tables[0].allies.len(), 1);
    assert_eq!(a_tables[0].allies[0].index, guild_b);
    assert_eq!(a_tables[0].allies[0].name, "比奇城");
    assert_eq!(a_tables[0].allies[0].leader, "乙会长");
    assert_eq!(b_tables[0].allies.len(), 1);
    assert_eq!(b_tables[0].allies[0].index, guild_a);
    assert_eq!(b_tables[0].allies[0].name, "沙巴克");
    assert_eq!(b_tables[0].allies[0].leader, "甲会长");
}

#[test]
fn removing_an_alliance_broadcasts_an_empty_table() {
    let mut harness = Harness::new();
    let a = harness.add_player(1, "甲会长");
    let b = harness.add_player(2, "乙会长");
    let guild_a = harness.create_guild(a, "沙巴克");
    let guild_b = harness.create_guild(b, "比奇城");
    harness
        .world
        .form_guild_alliance(guild_a, guild_b)
        .expect("结盟");
    harness.clear(a);
    harness.clear(b);

    harness
        .world
        .remove_guild_ally(a, guild_b)
        .expect("解除联盟应成功");

    assert_eq!(harness.allies(a), vec![GuildAllies::default()]);
    assert_eq!(harness.allies(b), vec![GuildAllies::default()]);
    // 双向链接都清掉了。
    assert!(harness.world.guild_allies(guild_a).is_empty());
    assert!(harness.world.guild_allies(guild_b).is_empty());
}

#[test]
fn accepting_an_ally_request_broadcasts_to_both_guilds() {
    let mut harness = Harness::new();
    let a = harness.add_player(1, "甲会长");
    let b = harness.add_player(2, "乙会长");
    let guild_a = harness.create_guild(a, "沙巴克");
    let guild_b = harness.create_guild(b, "比奇城");
    harness
        .world
        .request_guild_ally(a, guild_b)
        .expect("申请结盟");
    harness.clear(a);
    harness.clear(b);

    harness.world.accept_guild_ally(b).expect("同意结盟");

    // 申请-同意这条路也要走到同一个广播点（两边各一份，且互为对方）。
    let a_tables = harness.allies(a);
    let b_tables = harness.allies(b);
    assert_eq!(a_tables.len(), 1);
    assert_eq!(b_tables.len(), 1);
    assert_eq!(a_tables[0].allies[0].index, guild_b);
    assert_eq!(b_tables[0].allies[0].index, guild_a);
}

#[test]
fn login_with_a_guild_sends_the_allies_table() {
    let mut harness = Harness::new();
    let a = harness.add_player(1, "甲会长");
    let b = harness.add_player(2, "乙会长");
    let guild_a = harness.create_guild(a, "沙巴克");
    let guild_b = harness.create_guild(b, "比奇城");
    harness
        .world
        .form_guild_alliance(guild_a, guild_b)
        .expect("结盟");
    harness.clear(a);
    harness.clear(b);

    // 登录发包点：`guild_player_online`（并断言 `guild_bind_player` 只接状态、不发包）。
    harness.world.guild_bind_player(a);
    assert!(
        harness.allies(a).is_empty(),
        "guild_bind_player 只接状态，不该发包"
    );
    harness.world.guild_player_online(a);
    let tables = harness.allies(a);
    assert_eq!(tables.len(), 1, "有行会的玩家登录应收到一次 GuildAllies");
    assert_eq!(tables[0].allies.len(), 1);
    assert_eq!(tables[0].allies[0].index, guild_b);
}

#[test]
fn login_without_a_guild_sends_no_allies_packet() {
    let mut harness = Harness::new();
    let loner = harness.add_player(1, "散人");
    harness.clear(loner);

    harness.world.guild_player_online(loner);

    assert!(
        harness.allies(loner).is_empty(),
        "没有行会的玩家不应收到 GuildAllies"
    );
}

#[test]
fn dissolving_a_guild_clears_the_ally_backlink_and_notifies_the_survivor() {
    let mut harness = Harness::new();
    let a = harness.add_player(1, "甲会长");
    let b = harness.add_player(2, "乙会长");
    let guild_a = harness.create_guild(a, "沙巴克");
    let guild_b = harness.create_guild(b, "比奇城");
    harness
        .world
        .form_guild_alliance(guild_a, guild_b)
        .expect("结盟");
    harness.clear(a);
    harness.clear(b);

    // 乙方唯一成员（会长）退会 → 行会解散。
    harness.world.guild_leave(b).expect("退会并解散行会");

    // 甲方仍然在线，收到一份「已经没有盟友了」的名单。
    let tables = harness.allies(a);
    assert!(
        tables.iter().any(|table| table.allies.is_empty()),
        "行会解散后盟友应收到空名单：{tables:?}"
    );
    // 反向链接也清掉了（否则甲方会永远挂着一个不存在的行会）。
    assert!(harness.world.guild_allies(guild_a).is_empty());
    // 解散的那一方自己也在退会时下过线/清空，这里不再断言其收包。
    assert!(harness.world.guild_allies(guild_b).is_empty());
}
