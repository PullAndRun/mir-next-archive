//! 攻城（conquest / 沙巴克攻城）的端到端测试。
//!
//! 全部确定性：
//! * 世界随机数固定种子（[`World::seed_rng`]）；
//! * **时间表**用 [`World::process_conquests_at`] / [`World::request_conquest_war_at`] 显式传入
//!   「墙上时刻」（Unix 毫秒），不读真实时钟；星期几由 [`weekday_index`] 的整数算法决定
//!   （`1970-01-01` 是星期四），常量 [`MONDAY_18`] 是「星期一 18:00 UTC」；
//! * 战区内计分用 [`World::tick`] + [`World::advance_clock`] 驱动（不 sleep）；
//! * 地形是内存里合成的 60×60 空地，地图/攻城配表在内存数据库里构造；
//! * 持久化用 [`MemoryStore`]（实现了 `AccountStore` 的攻城两件套）。
//!
//! 每个断言尽量对应 Crystal 的一处行为，注释里写了出处（`文件.cs:行号`）。关键源码：
//! `Server/MirObjects/ConquestObject.cs`（整份文件）、
//! `Server/MirDatabase/ConquestGuildInfo.cs`、
//! `Server/MirObjects/NPC/NPCSegment.cs:4223-4298`（攻城 NPC 动作）。

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{ChatType, ConquestGame, ConquestType, MirClass, MirDirection, MirGender};
use mir_db::{
    ConquestArcherInfo, ConquestFlagInfo, ConquestInfo, ConquestStructure, MapInfo, MirDb,
    MonsterInfo, NpcInfo,
};
use mir_protocol::ServerPacket;
use mir_server::conquest::persist_conquest_writes;
use mir_server::gamedata::GameData;
use mir_server::{
    ConquestError, ConquestSettings, GuildSettings, MemoryStore, Npc, NpcState, Player, World,
    combat, minute_of_day, weekday_index,
};
use mir_world::{Map, MapCell, MapVersion};
use tokio::sync::mpsc;

/// 攻城区主键（配表 `ConquestInfo.Index`）。
const CONQUEST: i32 = 1;
/// 攻城区所在地图（数据库主键 172 → 文件名 `3`，与真实数据一致）。
const MAP: &str = "3";
/// 地图数据库主键。
const MAP_INDEX: i32 = 172;
/// 皇宫地图（数据库主键 245 → 文件名 `D2116`）。
const PALACE: &str = "D2116";
/// 皇宫地图数据库主键。
const PALACE_INDEX: i32 = 245;
/// 攻城区中心：真实数据的沙巴克在 (641, 287)，但合成地图只有 60×60，
/// 所以测试用的攻城区把中心缩到地图里（半径 20 → 覆盖 10..50）。
const CONQUEST_CENTER: Point = Point::new(30, 30);
/// 攻城区半径（真实数据是 30；测试用 20 以便落在 60×60 的合成地图内）。
const CONQUEST_SIZE: u16 = 20;
/// 会长等级（行会要求 22）。
const LEADER_LEVEL: u16 = 30;
/// 建行会/申请攻城用的金币。
const GOLD: i64 = 2_000_000;
/// 「星期一 18:00 UTC」：`20003 * 86400_000 + 18 * 3600_000`。
///
/// `20003 + 3 = 20006 = 7 * 2858`，所以 `weekday_index` 算出 0（星期一）。
const MONDAY_18: i64 = 20_003 * 86_400_000 + 18 * 3_600_000;
/// 一天。
const DAY: i64 = 86_400_000;
/// 一分钟。
const MINUTE: i64 = 60_000;

// ---------------------------------------------------------------------------
// 脚手架
// ---------------------------------------------------------------------------

/// 行会配置：等级 22、金币 1,000,000、倍率 1%、每级 1 点升级点。
fn guild_settings() -> GuildSettings {
    GuildSettings {
        required_level: 22,
        creation_gold: 1_000_000,
        exp_rate: 0.01,
        point_per_level: 1,
        experience_list: vec![100, 200, -1],
        member_cap_list: vec![10, 10, 20],
        buff_list: Vec::new(),
    }
}

/// 60×60 的可走空地。
fn synthetic_map(file_name: &str) -> Arc<Map> {
    Map {
        width: 60,
        height: 60,
        cells: vec![MapCell::default(); 3600],
        file_name: file_name.to_owned(),
        version: MapVersion::V0,
        doors: Vec::new(),
    }
    .into()
}

/// 真实形状的攻城配表：18:00 开战、持续 30 分钟、`Classic` 玩法、每天开战。
///
/// 与 `resource/community/suprcode-crystal-database/Jev/Server.MirDB` 里的
/// 「Sabuk Wall」一致（守卫 12 个、城门 1 个、城墙 3 个、没有旗子/控制点）。
fn conquest_info() -> ConquestInfo {
    ConquestInfo {
        index: CONQUEST,
        full_map: false,
        location: CONQUEST_CENTER,
        size: CONQUEST_SIZE,
        name: "Sabuk Wall".to_owned(),
        map_index: MAP_INDEX,
        palace_index: PALACE_INDEX,
        guard_index: 12,
        gate_index: 1,
        wall_index: 3,
        siege_index: 0,
        flag_index: 0,
        guards: (1..=12)
            .map(|index| ConquestArcherInfo {
                index,
                location: Point::new(40 + index, 45 - index),
                mob_index: 499,
                name: "Guard".to_owned(),
                repair_cost: 1000,
            })
            .collect(),
        extra_maps: Vec::new(),
        gates: vec![ConquestStructure {
            index: 1,
            location: Point::new(50, 40),
            mob_index: 568,
            name: "SabukGate".to_owned(),
            repair_cost: 1000,
        }],
        walls: vec![
            ConquestStructure {
                index: 1,
                location: Point::new(20, 20),
                mob_index: 569,
                name: "LeftWall".to_owned(),
                repair_cost: 1000,
            },
            ConquestStructure {
                index: 2,
                location: Point::new(22, 20),
                mob_index: 570,
                name: "CenterWall".to_owned(),
                repair_cost: 1000,
            },
            ConquestStructure {
                index: 3,
                location: Point::new(24, 20),
                mob_index: 571,
                name: "RightWall".to_owned(),
                repair_cost: 1000,
            },
        ],
        sieges: Vec::new(),
        flags: Vec::new(),
        start_hour: 18,
        war_length: 30,
        // `ConquestType.Request` = 0（`Shared/Enums.cs:1883`）。
        conquest_type: 0,
        // `ConquestGame.Classic` = 3（`Shared/Enums.cs:1893`）。
        game: 3,
        monday: true,
        tuesday: true,
        wednesday: true,
        thursday: true,
        friday: true,
        saturday: true,
        sunday: true,
        king_location: Point::new(0, 0),
        king_size: 0,
        control_point_index: 0,
        control_points: Vec::new(),
    }
}

/// 最小数据库：两张地图 + 一条攻城配表 + 一个 AI 80 的箭塔模板。
fn base_db(info: ConquestInfo) -> MirDb {
    let mut stats = mir_protocol::data::Stats::new();
    stats.set(mir_core::enums::Stat::HP, 500);
    stats.set(mir_core::enums::Stat::MinDC, 5);
    stats.set(mir_core::enums::Stat::MaxDC, 10);
    stats.set(mir_core::enums::Stat::Accuracy, 20);
    MirDb {
        maps: vec![
            MapInfo {
                index: MAP_INDEX,
                file_name: MAP.to_owned(),
                title: "沙巴克".to_owned(),
                ..MapInfo::default()
            },
            MapInfo {
                index: PALACE_INDEX,
                file_name: PALACE.to_owned(),
                title: "沙巴克皇宫".to_owned(),
                ..MapInfo::default()
            },
        ],
        conquests: vec![info],
        monsters: vec![MonsterInfo {
            index: 499,
            name: "Guard".to_owned(),
            // AI 80 = `ConquestArcher`（`Server/MirObjects/MonsterObject.cs:189-190`）。
            ai: 80,
            level: 50,
            view_range: 7,
            attack_speed: 1000,
            move_speed: 1000,
            stats,
            ..MonsterInfo::default()
        }],
        ..MirDb::default()
    }
}

/// 测试世界 + 每个玩家的收包通道。
struct Harness {
    world: World,
    receivers: HashMap<u32, mpsc::UnboundedReceiver<Vec<u8>>>,
}

impl Harness {
    fn new(seed: u64) -> Self {
        Self::with_info(seed, conquest_info())
    }

    fn with_info(seed: u64, info: ConquestInfo) -> Self {
        let mut world = World::new(999);
        world.seed_rng(seed);
        world.insert_map(MAP, synthetic_map(MAP));
        world.insert_map(PALACE, synthetic_map(PALACE));
        world.set_game_data(Arc::new(GameData::from_db(base_db(info), None)));
        world.set_guild_settings(guild_settings());
        world.init_conquests();
        Self {
            world,
            receivers: HashMap::new(),
        }
    }

    fn add_player(&mut self, object_id: u32, name: &str, level: u16, gold: i64) -> u32 {
        let stats = combat::player_base_stats(level, MirClass::Warrior);
        let (sender, receiver) = mpsc::unbounded_channel();
        self.world.enter(Player {
            object_id,
            session_id: u64::from(object_id),
            character_id: i64::from(object_id),
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

    /// 让 `member` 加入 `inviter` 的行会。
    fn join(&mut self, inviter: u32, member: u32) {
        let name = self.name_of(member);
        self.world
            .guild_invite(inviter, &name)
            .unwrap_or_else(|error| panic!("邀请 {name} 应成功：{error}"));
        assert!(
            self.world.guild_join(member, true).is_ok(),
            "{name} 接受邀请应成功"
        );
    }

    /// 把某个玩家移到攻城区中心（或指定坐标）。
    ///
    /// 走 `World::place_player`：它在改坐标的同时同步 AOI 网格
    /// （否则广播/索敌会按旧坐标找人）。
    fn teleport(&mut self, object_id: u32, map: &str, point: Point) {
        assert!(
            self.world.place_player(object_id, map, point),
            "玩家 {object_id} 不在世界里"
        );
    }

    async fn flush(&mut self, store: &MemoryStore) {
        let writes = self.world.take_conquest_writes();
        persist_conquest_writes(store, writes)
            .await
            .expect("攻城落盘");
    }
}

/// 某个玩家收到的 `S.Chat` 文本。
fn chats(packets: &[ServerPacket]) -> Vec<(&str, ChatType)> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::Chat(chat) => Some((chat.message.as_str(), chat.type_)),
            _ => None,
        })
        .collect()
}

/// 新一轮「申请攻城」的常用前置：两个行会 + 各一个会长。
struct TwoGuilds {
    alpha_leader: u32,
    alpha: i32,
    beta_leader: u32,
    beta: i32,
}

fn two_guilds(harness: &mut Harness) -> TwoGuilds {
    let alpha_leader = harness.add_player(1, "甲会会长", LEADER_LEVEL, GOLD);
    let alpha = harness.create_guild(alpha_leader, "甲会联盟");
    let beta_leader = harness.add_player(2, "乙会会长", LEADER_LEVEL, GOLD);
    let beta = harness.create_guild(beta_leader, "乙会联盟");
    harness.clear(1);
    harness.clear(2);
    TwoGuilds {
        alpha_leader,
        alpha,
        beta_leader,
        beta,
    }
}

/// 直接把某个行会设成城主（绕过开战流程）。
///
/// Crystal 的 `TakeConquest` 在 `StartType == Request` 时要求「只有申请方能夺城」
/// （`ConquestObject.cs:580`），而新建的 `ConquestObject.StartType` 默认就是 `Request`
/// （C# 枚举零值），所以测试里要先把 `StartType` 改成 `Auto` 才能直接指定城主
/// ——这与「GM 用 `SCHEDULECONQUEST` 之外的手段指定城主」的语义一致。
fn set_owner(harness: &mut Harness, guild_id: i32) {
    if let Some(conquest) = harness.world.conquest_mut(CONQUEST) {
        conquest.start_type = ConquestType::Auto;
    }
    assert!(
        harness.world.take_conquest(CONQUEST, guild_id),
        "指定城主应成功"
    );
}

// ---------------------------------------------------------------------------
// 1. 真实数据冒烟
// ---------------------------------------------------------------------------

#[test]
fn real_database_exposes_a_complete_conquest_record() {
    let Ok(data) = GameData::discover() else {
        eprintln!("跳过：resource/ 里没有 Server.MirDB 样本");
        return;
    };
    let count = data.conquest_count();
    println!("db.conquests 数量 = {count}");
    assert_eq!(count, 1, "Jev 样本里只有一个攻城区");
    let info = data.conquest(CONQUEST).expect("主键 1 的攻城区");
    println!("{info:#?}");
    assert_eq!(info.name, "Sabuk Wall");
    assert_eq!(info.map_index, MAP_INDEX);
    assert_eq!(info.palace_index, PALACE_INDEX, "皇宫地图主键");
    assert_eq!(info.location, Point::new(641, 287), "沙巴克中心");
    assert_eq!(info.size, 30);
    assert!(!info.full_map, "不是整图攻城区");
    assert_eq!(info.start_hour, 18);
    assert_eq!(info.war_length, 30);
    assert_eq!(info.conquest_type, ConquestType::Request.value());
    assert_eq!(
        info.game,
        ConquestGame::Classic.value(),
        "真实数据用 Classic 玩法"
    );
    assert_eq!(info.guards.len(), 12, "12 座箭塔");
    assert_eq!(info.gates.len(), 1);
    assert_eq!(info.walls.len(), 3);
    assert!(
        info.flags.is_empty() && info.control_points.is_empty(),
        "没有旗子/控制点"
    );
    assert!(
        info.monday
            && info.tuesday
            && info.wednesday
            && info.thursday
            && info.friday
            && info.saturday
            && info.sunday,
        "每天开战"
    );
    // 箭塔模板真的存在，且 AI 是 80（`ConquestArcher`）。
    let guard = &info.guards[0];
    let kind = data
        .monster(guard.mob_index)
        .unwrap_or_else(|| panic!("箭塔模板 {0} 应该存在", guard.mob_index));
    assert!(kind.is_conquest_archer(), "AI 80 = ConquestArcher");
    assert!(kind.is_guard() || kind.ai == 80);
}

// ---------------------------------------------------------------------------
// 2. 装载：配表 → 运行期
// ---------------------------------------------------------------------------

#[test]
fn init_conquests_resolves_maps_and_starts_unowned() {
    let harness = Harness::new(1);
    let conquest = harness.world.conquest(CONQUEST).expect("攻城区应存在");
    assert_eq!(conquest.map.as_deref(), Some(MAP), "MapIndex 解析成文件名");
    assert_eq!(
        conquest.palace_map.as_deref(),
        Some(PALACE),
        "PalaceIndex 解析成文件名"
    );
    assert_eq!(
        conquest.owner, -1,
        "初始无主（ConquestGuildInfo.Owner = -1）"
    );
    assert_eq!(conquest.attacker, -1, "初始没人申请");
    assert!(!conquest.war_is_on, "初始未开战");
    assert_eq!(conquest.gold_storage, 0);
    assert_eq!(conquest.npc_rate, 0);
    assert_eq!(conquest.control_points.len(), 0);
    // 时间表：18:00 开、18:30 收（`ConquestObject.AutoSchedule`，`:467-468`）。
    assert_eq!(conquest.start_minute(), 18 * 60);
    assert_eq!(conquest.finish_minute(), 18 * 60 + 30);
    // 战区判定：中心点在内、半径外一点点不在（`Size = 20`）。
    assert!(conquest.covers(MAP, CONQUEST_CENTER));
    assert!(conquest.covers(
        MAP,
        Point::new(
            CONQUEST_CENTER.x + i32::from(CONQUEST_SIZE),
            CONQUEST_CENTER.y
        )
    ));
    assert!(!conquest.covers(
        MAP,
        Point::new(
            CONQUEST_CENTER.x + i32::from(CONQUEST_SIZE) + 1,
            CONQUEST_CENTER.y
        )
    ));
    assert!(
        !conquest.covers(PALACE, CONQUEST_CENTER),
        "别的地图不算攻城区"
    );
}

#[test]
fn weekday_and_minute_helpers_match_the_schedule() {
    // `1970-01-01` 是星期四 → 序号 3（周一 = 0）。
    assert_eq!(weekday_index(0), 3);
    assert_eq!(weekday_index(MONDAY_18), 0, "常量就是星期一");
    assert_eq!(weekday_index(MONDAY_18 + DAY), 1, "第二天是星期二");
    assert_eq!(minute_of_day(MONDAY_18), 18 * 60);
    assert_eq!(minute_of_day(MONDAY_18 + 30 * MINUTE), 18 * 60 + 30);
    // 负时间戳也不 panic（往正方向取模）。
    assert!(minute_of_day(-1) >= 0);
    assert!(minute_of_day(-DAY) >= 0);
}

// ---------------------------------------------------------------------------
// 3. 申请攻城：校验
// ---------------------------------------------------------------------------

#[test]
fn request_war_succeeds_and_queues_the_attacker() {
    let mut harness = Harness::new(2);
    let guilds = two_guilds(&mut harness);
    // 时间窗外申请：只排队，不开战（`ConquestObject.cs:482-503`）。
    let report = harness
        .world
        .request_conquest_war_at(guilds.alpha_leader, CONQUEST, MONDAY_18 - 60 * MINUTE)
        .expect("申请应成功");
    assert_eq!(report.guild_id, guilds.alpha);
    assert!(!report.war_started, "时间窗外只排队");
    assert_eq!(
        report.next_war_at, MONDAY_18,
        "下一次开战时刻 = 今天 18:00（Crystal 每分钟现算）"
    );
    let conquest = harness.world.conquest(CONQUEST).expect("攻城区");
    assert_eq!(conquest.attacker, guilds.alpha, "AttackerID 记成申请方");
    assert!(!conquest.war_is_on);

    // 时间窗内申请：下一轮时间表立刻开战。
    let report =
        harness
            .world
            .request_conquest_war_at(guilds.beta_leader, CONQUEST, MONDAY_18 + 5 * MINUTE);
    assert_eq!(
        report,
        Err(ConquestError::AlreadyScheduled),
        "已经有行会申请过了（CheckType.ConquestAvailable）"
    );
}

#[test]
fn request_war_inside_the_window_starts_immediately() {
    let mut harness = Harness::new(3);
    let guilds = two_guilds(&mut harness);
    let report = harness
        .world
        .request_conquest_war_at(guilds.alpha_leader, CONQUEST, MONDAY_18 + 5 * MINUTE)
        .expect("申请应成功");
    assert!(report.war_started, "时间窗内申请应立刻开战");
    let conquest = harness.world.conquest(CONQUEST).expect("攻城区");
    assert!(conquest.war_is_on);
    assert_eq!(
        conquest.game_type,
        ConquestGame::Classic,
        "GameType = Info.Game"
    );
}

#[test]
fn request_war_requires_guild_leader() {
    let mut harness = Harness::new(4);
    let leader = harness.add_player(1, "会长", LEADER_LEVEL, GOLD);
    let member = harness.add_player(2, "成员", LEADER_LEVEL, GOLD);
    let _guild = harness.create_guild(leader, "甲会联盟");
    harness.join(leader, member);
    assert_eq!(
        harness
            .world
            .request_conquest_war_at(member, CONQUEST, MONDAY_18),
        Err(ConquestError::NotLeader),
        "只有会长能申请（NPCScript.cs:1066-1069）"
    );
    // 没行会的人：`NotInGuild`。
    let outsider = harness.add_player(3, "散人", LEADER_LEVEL, GOLD);
    assert_eq!(
        harness
            .world
            .request_conquest_war_at(outsider, CONQUEST, MONDAY_18),
        Err(ConquestError::NotInGuild)
    );
}

#[test]
fn request_war_checks_guild_level() {
    let mut harness = Harness::new(5);
    let guilds = two_guilds(&mut harness);
    harness.world.set_conquest_settings(ConquestSettings {
        war_cost: 0,
        required_guild_level: 5,
    });
    // 新建行会等级是 0（`GuildInfo.Level` 从 0 起）。
    assert_eq!(
        harness
            .world
            .request_conquest_war_at(guilds.alpha_leader, CONQUEST, MONDAY_18),
        Err(ConquestError::LevelTooLow {
            required: 5,
            level: 0
        })
    );
    // 把行会升到 5 级就通过。
    if let Some(guild) = harness.world.guild_mut(guilds.alpha) {
        guild.level = 5;
    }
    assert!(
        harness
            .world
            .request_conquest_war_at(guilds.alpha_leader, CONQUEST, MONDAY_18)
            .is_ok()
    );
}

#[test]
fn request_war_checks_guild_gold_and_deducts_it() {
    let mut harness = Harness::new(6);
    let guilds = two_guilds(&mut harness);
    harness.world.set_conquest_settings(ConquestSettings {
        war_cost: 5_000,
        required_guild_level: 0,
    });
    // 行会资金在建会时被清空，这里先充 1000。
    if let Some(guild) = harness.world.guild_mut(guilds.alpha) {
        guild.gold = 1_000;
    }
    assert_eq!(
        harness
            .world
            .request_conquest_war_at(guilds.alpha_leader, CONQUEST, MONDAY_18),
        Err(ConquestError::NotEnoughGold {
            required: 5_000,
            gold: 1_000
        })
    );
    if let Some(guild) = harness.world.guild_mut(guilds.alpha) {
        guild.gold = 8_000;
    }
    let report = harness
        .world
        .request_conquest_war_at(guilds.alpha_leader, CONQUEST, MONDAY_18)
        .expect("资金够了应成功");
    assert_eq!(report.gold_spent, 5_000);
    assert_eq!(
        harness.world.guild(guilds.alpha).map(|guild| guild.gold),
        Some(3_000),
        "行会资金扣掉 5000"
    );
    // 成员收到 `S.GuildStorageGoldChange{Type = 2}`（Crystal 扣钱的包）。
    let packets = harness.packets(guilds.alpha_leader);
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::GuildStorageGoldChange(change)
                if change.type_ == 2 && change.amount == 5_000
        )),
        "应收到扣行会资金的包，实际 {packets:?}"
    );
}

#[test]
fn request_war_rejects_while_at_war_and_when_already_owner() {
    let mut harness = Harness::new(7);
    let guilds = two_guilds(&mut harness);
    // 先申请并开战。
    harness
        .world
        .request_conquest_war_at(guilds.alpha_leader, CONQUEST, MONDAY_18)
        .expect("申请应成功");
    assert!(
        harness
            .world
            .conquest(CONQUEST)
            .is_some_and(|c| c.war_is_on),
        "应该已经开战"
    );
    assert_eq!(
        harness
            .world
            .request_conquest_war_at(guilds.beta_leader, CONQUEST, MONDAY_18),
        Err(ConquestError::AtWar)
    );
    // 结束战争后把城主设成乙会，再申请 → `AlreadyOwner`。
    harness.world.end_war(CONQUEST);
    set_owner(&mut harness, guilds.beta);
    assert_eq!(
        harness
            .world
            .request_conquest_war_at(guilds.beta_leader, CONQUEST, MONDAY_18),
        Err(ConquestError::AlreadyOwner)
    );
    // 不存在的攻城区。
    assert_eq!(
        harness
            .world
            .request_conquest_war_at(guilds.alpha_leader, 999, MONDAY_18),
        Err(ConquestError::NotFound(999))
    );
}

// ---------------------------------------------------------------------------
// 4. 时间表：开战 / 停战 / 广播
// ---------------------------------------------------------------------------

#[test]
fn schedule_starts_the_war_and_broadcasts_a_system_message() {
    let mut harness = Harness::new(8);
    let guilds = two_guilds(&mut harness);
    // 没有攻打方时，`Request` 类型的攻城区不会自动开战（`ConquestObject.cs:486-494`）。
    let report = harness.world.process_conquests_at(MONDAY_18 - 60 * MINUTE);
    assert!(report.is_empty(), "没人申请不该开战");
    assert!(
        !harness
            .world
            .conquest(CONQUEST)
            .is_some_and(|c| c.war_is_on)
    );

    harness
        .world
        .request_conquest_war_at(guilds.alpha_leader, CONQUEST, MONDAY_18 - 60 * MINUTE)
        .expect("申请应成功");
    harness.clear(1);
    harness.clear(2);

    // 到点开战（`AutoSchedule` 的 `start <= now && finish > now && CheckDay()`）。
    let report = harness.world.process_conquests_at(MONDAY_18 + 5 * MINUTE);
    assert_eq!(report.started, vec![CONQUEST], "本轮应开战");
    let conquest = harness.world.conquest(CONQUEST).expect("攻城区");
    assert!(conquest.war_is_on);
    assert_eq!(conquest.start_type, ConquestType::Request);
    assert_eq!(
        conquest.game_type,
        ConquestGame::Classic,
        "开战时 GameType = Info.Game"
    );
    assert_eq!(
        conquest.war_start_time,
        MONDAY_18 + 5 * MINUTE,
        "本作记录开战时刻"
    );

    // 全服系统公告（Crystal 的 `ReceiveChat(WarStarted, ChatType.System)`，
    // `NPCSegment.cs:4261-4280`；定时开战在 Crystal 里只 `BroadcastInfo()`，
    // 本作两条路径都发公告，属于记录在案的简化）。
    for object_id in [1, 2] {
        let packets = harness.packets(object_id);
        let messages = chats(&packets);
        assert!(
            messages
                .iter()
                .any(|(text, type_)| *type_ == ChatType::System && text.contains("攻城战已经开始")),
            "玩家 {object_id} 应收到开战公告，实际 {messages:?}"
        );
    }
}

#[test]
fn schedule_ends_the_war_after_the_window() {
    let mut harness = Harness::new(9);
    let guilds = two_guilds(&mut harness);
    harness
        .world
        .request_conquest_war_at(guilds.alpha_leader, CONQUEST, MONDAY_18)
        .expect("申请应成功");
    assert!(
        harness
            .world
            .conquest(CONQUEST)
            .is_some_and(|c| c.war_is_on)
    );

    // 窗口里（18:29）还在打。
    let report = harness.world.process_conquests_at(MONDAY_18 + 29 * MINUTE);
    assert!(report.ended.is_empty(), "18:29 还没到停战时刻");
    assert!(
        harness
            .world
            .conquest(CONQUEST)
            .is_some_and(|c| c.war_is_on)
    );

    harness.clear(1);
    // 18:30 到点停战（`now > start && finish <= now`，`:478`）。
    let report = harness.world.process_conquests_at(MONDAY_18 + 30 * MINUTE);
    assert_eq!(report.ended, vec![CONQUEST], "本轮应停战");
    let conquest = harness.world.conquest(CONQUEST).expect("攻城区");
    assert!(!conquest.war_is_on);
    assert_eq!(
        conquest.attacker, -1,
        "Request 类型停战后清零 AttackerID（`:353-358`）"
    );
    assert_eq!(
        conquest.start_type,
        ConquestType::Request,
        "停战后 StartType 复位成配表类型（Crystal 不复位，见文档）"
    );
    let inbox = harness.packets(1);
    let messages = chats(&inbox);
    assert!(
        messages
            .iter()
            .any(|(text, _)| text.contains("攻城战已经结束")),
        "应收到停战公告，实际 {messages:?}"
    );
}

#[test]
fn schedule_skips_days_that_are_not_configured() {
    let mut info = conquest_info();
    info.monday = true;
    for day in [
        &mut info.tuesday,
        &mut info.wednesday,
        &mut info.thursday,
        &mut info.friday,
        &mut info.saturday,
        &mut info.sunday,
    ] {
        *day = false;
    }
    let mut harness = Harness::with_info(10, info);
    let guilds = two_guilds(&mut harness);
    harness
        .world
        .request_conquest_war_at(guilds.alpha_leader, CONQUEST, MONDAY_18 - MINUTE)
        .expect("申请应成功");
    // 星期二同一时刻：`CheckDay()` 为假，不开战（`ConquestObject.cs:442-463`）。
    let report = harness.world.process_conquests_at(MONDAY_18 + DAY);
    assert!(report.started.is_empty(), "星期二不该开战");
    assert!(
        !harness
            .world
            .conquest(CONQUEST)
            .is_some_and(|c| c.war_is_on)
    );
    // 下周一才开。
    let report = harness.world.process_conquests_at(MONDAY_18 + 7 * DAY);
    assert_eq!(report.started, vec![CONQUEST]);
}

#[test]
fn forced_war_ends_at_war_end_time() {
    let mut harness = Harness::new(11);
    let _guilds = two_guilds(&mut harness);
    // GM/NPC 的 `STARTCONQUEST`（`NPCSegment.cs:4250-4286`）。
    assert!(
        harness.world.toggle_conquest_war(CONQUEST).expect("开战"),
        "返回 true = 本次是开战"
    );
    let conquest = harness.world.conquest(CONQUEST).expect("攻城区");
    assert_eq!(conquest.start_type, ConquestType::Forced);
    assert!(conquest.war_is_on);
    assert_eq!(
        conquest.war_end_time,
        conquest.war_start_time + 30 * MINUTE,
        "Forced 用 WarLength 算停战时刻（`:320-325`）"
    );
    // 到点自动停战（`:471-474`）。
    let end = conquest.war_end_time;
    let report = harness.world.process_conquests_at(end + MINUTE);
    assert_eq!(report.ended, vec![CONQUEST]);
    assert!(
        !harness
            .world
            .conquest(CONQUEST)
            .is_some_and(|c| c.war_is_on)
    );
    // 再切一次就是「停战」语义的兜底：已经在停战状态，返回 true（重新开战）。
    assert!(
        harness
            .world
            .toggle_conquest_war(CONQUEST)
            .expect("再开一次")
    );
    assert!(
        !harness
            .world
            .toggle_conquest_war(CONQUEST)
            .expect("切回停战")
    );
}

// ---------------------------------------------------------------------------
// 5. 城主判定
// ---------------------------------------------------------------------------

#[test]
fn classic_captures_when_only_one_guild_holds_the_palace() {
    let mut harness = Harness::new(12);
    let guilds = two_guilds(&mut harness);
    harness
        .world
        .request_conquest_war_at(guilds.alpha_leader, CONQUEST, MONDAY_18)
        .expect("申请应成功");
    assert!(
        harness
            .world
            .conquest(CONQUEST)
            .is_some_and(|c| c.war_is_on)
    );

    // 皇宫里只有甲会的人 + 乙会的人也在 → 两个行会，不能夺城。
    let alpha_member = harness.add_player(3, "甲会成员", LEADER_LEVEL, GOLD);
    harness.join(guilds.alpha_leader, alpha_member);
    harness.teleport(guilds.alpha_leader, PALACE, Point::new(20, 20));
    harness.teleport(alpha_member, PALACE, Point::new(21, 20));
    harness.teleport(guilds.beta_leader, PALACE, Point::new(22, 20));
    let report = harness.world.process_conquests_at(MONDAY_18 + MINUTE);
    assert!(
        report.captured.is_empty(),
        "皇宫里有两个行会时不该夺城（`:873-891`）"
    );
    assert_eq!(harness.world.conquest(CONQUEST).map(|c| c.owner), Some(-1));

    // 乙会的人离开皇宫 → 只剩甲会，夺城（`guildCounter == 1`）。
    harness.teleport(guilds.beta_leader, MAP, Point::new(10, 10));
    harness.clear(1);
    let report = harness.world.process_conquests_at(MONDAY_18 + 2 * MINUTE);
    assert_eq!(
        report.captured,
        vec![(CONQUEST, guilds.alpha)],
        "只剩一个行会应夺城"
    );
    let conquest = harness.world.conquest(CONQUEST).expect("攻城区");
    assert_eq!(conquest.owner, guilds.alpha);
    assert!(
        conquest.war_is_on,
        "Classic 夺城**不**结束战争（只有 CapturePalace 会 `EndWar`，`:575`）"
    );
    let inbox = harness.packets(1);
    let messages = chats(&inbox);
    assert!(
        messages
            .iter()
            .any(|(text, _)| text.contains("新城主是 甲会联盟")),
        "城主变更要全服公告，实际 {messages:?}"
    );
}

#[test]
fn classic_requires_the_attacker_to_take_the_castle() {
    let mut harness = Harness::new(13);
    let guilds = two_guilds(&mut harness);
    harness
        .world
        .request_conquest_war_at(guilds.alpha_leader, CONQUEST, MONDAY_18)
        .expect("申请应成功");
    assert!(
        harness
            .world
            .conquest(CONQUEST)
            .is_some_and(|c| c.war_is_on)
    );
    // 乙会（不是申请方）占住皇宫：`Request` 类型下 `break`，不夺城（`:895`）。
    harness.teleport(guilds.beta_leader, PALACE, Point::new(20, 20));
    let report = harness.world.process_conquests_at(MONDAY_18 + MINUTE);
    assert!(report.captured.is_empty(), "非申请方不能夺城");
    assert_eq!(harness.world.conquest(CONQUEST).map(|c| c.owner), Some(-1));
    // 甲会（申请方）占住皇宫 → 夺城。
    harness.teleport(guilds.beta_leader, MAP, Point::new(10, 10));
    harness.teleport(guilds.alpha_leader, PALACE, Point::new(20, 20));
    let report = harness.world.process_conquests_at(MONDAY_18 + 2 * MINUTE);
    assert_eq!(report.captured, vec![(CONQUEST, guilds.alpha)]);
}

#[test]
fn capture_through_the_palace_map_for_the_capture_game() {
    // `CapturePalace` 玩法：走进皇宫就算占领（`PlayerObject.CheckConquest(true)`，`:4616`）。
    let mut info = conquest_info();
    info.game = ConquestGame::CapturePalace.value();
    let mut harness = Harness::with_info(14, info);
    let guilds = two_guilds(&mut harness);
    harness
        .world
        .request_conquest_war_at(guilds.alpha_leader, CONQUEST, MONDAY_18)
        .expect("申请应成功");
    assert!(
        harness
            .world
            .conquest(CONQUEST)
            .is_some_and(|c| c.war_is_on)
    );
    // 申请方的人切进皇宫 → 夺城 + 立刻停战（`TakeConquest` 的 `EndWar`，`:575`）。
    let direction = MirDirection::Down;
    harness
        .world
        .change_map(guilds.alpha_leader, PALACE, Point::new(20, 20), direction)
        .expect("切图应成功");
    let conquest = harness.world.conquest(CONQUEST).expect("攻城区");
    assert_eq!(conquest.owner, guilds.alpha);
    assert!(!conquest.war_is_on, "CapturePalace 夺城后立刻停战");
}

#[test]
fn king_of_hill_scores_and_captures() {
    let mut info = conquest_info();
    info.game = ConquestGame::KingOfHill.value();
    info.king_location = CONQUEST_CENTER;
    info.king_size = 5;
    let mut harness = Harness::with_info(15, info);
    let guilds = two_guilds(&mut harness);
    harness
        .world
        .request_conquest_war_at(guilds.alpha_leader, CONQUEST, MONDAY_18)
        .expect("申请应成功");
    // 甲会会长站进王座圈（攻城区中心附近，同时也在战区里）。
    harness.teleport(guilds.alpha_leader, MAP, CONQUEST_CENTER);
    let report = harness.world.process_conquests_at(MONDAY_18 + MINUTE);
    let conquest = harness.world.conquest(CONQUEST).expect("攻城区");
    let score = conquest
        .king_points
        .get(&guilds.alpha)
        .copied()
        .unwrap_or(0);
    assert_eq!(score, 1, "每轮 +1 分（`ConquestObject.cs:813-824`）");
    assert_eq!(
        report.captured,
        vec![(CONQUEST, guilds.alpha)],
        "分最高且不是城主应立刻夺城（`:861-869`）"
    );
    assert_eq!(
        harness.world.conquest(CONQUEST).map(|c| c.owner),
        Some(guilds.alpha)
    );
}

#[test]
fn control_points_change_owner_and_decide_the_castle() {
    let mut info = conquest_info();
    info.game = ConquestGame::ControlPoints.value();
    info.control_point_index = 7;
    info.control_points = vec![ConquestFlagInfo {
        index: 7,
        location: CONQUEST_CENTER,
        name: "城门控制点".to_owned(),
        file_name: "flag".to_owned(),
    }];
    let mut harness = Harness::with_info(16, info);
    let guilds = two_guilds(&mut harness);
    assert_eq!(
        harness
            .world
            .conquest(CONQUEST)
            .map(|c| c.control_points.len()),
        Some(1),
        "控制点从配表建出来"
    );
    harness
        .world
        .request_conquest_war_at(guilds.alpha_leader, CONQUEST, MONDAY_18)
        .expect("申请应成功");
    harness.teleport(guilds.alpha_leader, MAP, CONQUEST_CENTER);
    harness.clear(1);
    // 每轮 +1 分（上限 6，`MAX_CONTROL_POINTS`）。
    harness.world.process_conquests_at(MONDAY_18 + MINUTE);
    let conquest = harness.world.conquest(CONQUEST).expect("攻城区");
    assert_eq!(
        conquest.control_points[0].owner, guilds.alpha,
        "分最高者控制"
    );
    assert_eq!(
        conquest.control_points[0].points.get(&guilds.alpha),
        Some(&1)
    );
    let inbox = harness.packets(1);
    let messages = chats(&inbox);
    assert!(
        messages
            .iter()
            .any(|(text, _)| text.contains("占领了控制点")),
        "控制点易主要公告，实际 {messages:?}"
    );
    // 停战时按控制点数量选城主（`EndWar` 的 `ControlPoints` 分支，`:921-962`）。
    harness.world.end_war(CONQUEST);
    assert_eq!(
        harness.world.conquest(CONQUEST).map(|c| c.owner),
        Some(guilds.alpha),
        "控制点最多的行会当城主"
    );
}

// ---------------------------------------------------------------------------
// 6. 战区：名字颜色与 PK 保护
// ---------------------------------------------------------------------------

#[test]
fn war_zone_paints_names_green_and_protects_killers() {
    let mut harness = Harness::new(17);
    let guilds = two_guilds(&mut harness);
    assert_eq!(
        harness.world.name_colour_for(1),
        mir_server::world::NAME_COLOUR_WHITE,
        "战前是白名"
    );
    harness
        .world
        .request_conquest_war_at(guilds.alpha_leader, CONQUEST, MONDAY_18)
        .expect("申请应成功");
    // 走到攻城区中心：WarZone = true（`Map.GetConquest` + `EnterSabuk`）。
    harness.teleport(guilds.alpha_leader, MAP, CONQUEST_CENTER);
    assert!(harness.world.in_war_zone(guilds.alpha_leader), "在战区里");
    harness.world.tick();
    assert_eq!(
        harness.world.name_colour_for(guilds.alpha_leader),
        mir_server::world::NAME_COLOUR_WAR_ZONE,
        "战区里名字变绿（`PlayerObject.cs:1861-1870`）"
    );
    // 野外的人不受影响（半径 20，站到 (5,5) 就出去了）。
    harness.teleport(guilds.beta_leader, MAP, Point::new(5, 5));
    assert!(
        !harness.world.in_war_zone(guilds.beta_leader),
        "站在 (5,5) 不在攻城区半径内"
    );

    // 战区里杀死对方不加 PK 值，只给凶手一条「受到法律保护」的提示（`:603-606`）。
    harness.teleport(
        guilds.beta_leader,
        MAP,
        Point::new(CONQUEST_CENTER.x + 1, CONQUEST_CENTER.y),
    );
    harness.clear(1);
    // 受害者只剩 1 点血，凶手一击必杀；攻击模式设成「全体」才能打玩家
    // （`Peace` 恒为 false，`PlayerObject.cs:1370-1373`）。
    harness
        .world
        .change_attack_mode(guilds.alpha_leader, mir_core::enums::AttackMode::All);
    if let Some(attacker) = harness.world.player_mut(guilds.alpha_leader) {
        attacker.stats.min_dc = 100;
        attacker.stats.max_dc = 100;
        attacker.stats.accuracy = 100;
        attacker.action_time = 0;
        attacker.attack_time = 0;
    }
    if let Some(victim) = harness.world.player_mut(guilds.beta_leader) {
        victim.hp = 1;
        victim.stats.agility = 0;
        victim.stats.max_ac = 0;
        victim.regen_time = i64::MAX;
    }
    // 打几下（命中率不是 100%），直到倒下。
    let mut dead = false;
    for _ in 0..20 {
        if let Some(attacker) = harness.world.player_mut(guilds.alpha_leader) {
            attacker.action_time = 0;
            attacker.attack_time = 0;
        }
        let _ = harness
            .world
            .attack(guilds.alpha_leader, MirDirection::Right);
        harness.world.advance_clock(400);
        harness.world.tick();
        dead = harness
            .world
            .player(guilds.beta_leader)
            .is_some_and(|player| player.dead);
        if dead {
            break;
        }
    }
    assert!(dead, "乙会会长应该被击杀");
    assert_eq!(
        harness
            .world
            .player(guilds.alpha_leader)
            .map(|p| p.pk_points),
        Some(0),
        "战区里杀人**不加** PK 值"
    );
    let inbox = harness.packets(1);
    let messages = chats(&inbox);
    assert!(
        messages
            .iter()
            .any(|(text, _)| text.contains("你受到了法律的保护")),
        "凶手应收到保护提示，实际 {messages:?}"
    );
}

// ---------------------------------------------------------------------------
// 7. 城主收益：加价与税收池
// ---------------------------------------------------------------------------

#[test]
fn castle_tax_pool_and_price_rate() {
    let mut harness = Harness::new(18);
    let guilds = two_guilds(&mut harness);
    // 把甲会设成城主 + 加价 50%。
    set_owner(&mut harness, guilds.alpha);
    harness
        .world
        .set_conquest_rate(guilds.alpha_leader, CONQUEST, 50)
        .expect("城主可以设置加价（NPCSegment.cs:4237-4249）");
    assert_eq!(
        harness
            .world
            .conquest(CONQUEST)
            .map(|conquest| conquest.npc_rate),
        Some(50)
    );
    // 非城主行会不能设置。
    assert_eq!(
        harness
            .world
            .set_conquest_rate(guilds.beta_leader, CONQUEST, 10),
        Err(ConquestError::NotOwnerGuild)
    );

    // NPC 属于这个攻城区：`NpcInfo.Conquest == 1`（`ConquestObject.LoadNPCs`，`:265-306`）。
    let npc = Npc::from_info(
        &NpcInfo {
            index: 1,
            name: "沙巴克商人".to_owned(),
            file_name: "sabuk".to_owned(),
            rate: 200,
            conquest: CONQUEST,
            ..NpcInfo::default()
        },
        MAP,
        900,
        MirDirection::Down,
    );
    // 城主行会成员：`Rate / 100 = 2.0`（`NPCScript.cs:125-128`）。
    assert!(
        (harness
            .world
            .conquest_price_rate(&npc, guilds.alpha_leader, false)
            - 2.0)
            .abs()
            < f32::EPSILON,
        "城主行会按基础价"
    );
    // 其它人：`((Rate/100) * NPCRate + Rate) / 100 = (2.0 * 50 + 200) / 100 = 3.0`
    // （`NPCScript.cs:131`）。
    assert!(
        (harness
            .world
            .conquest_price_rate(&npc, guilds.beta_leader, false)
            - 3.0)
            .abs()
            < f32::EPSILON,
        "非城主行会吃 50% 加价"
    );
    // 基础价倍率忽略加价。
    assert!(
        (harness
            .world
            .conquest_price_rate(&npc, guilds.beta_leader, true)
            - 2.0)
            .abs()
            < f32::EPSILON,
        "base_rate 忽略加价"
    );

    // 税收池：`GoldStorage += cost - baseCost`（`NPCScript.cs:1211-1214`）。
    harness.world.add_conquest_tax(CONQUEST, 300);
    harness.world.add_conquest_tax(CONQUEST, 0);
    assert_eq!(harness.world.conquest_gold_storage(CONQUEST), 300);
    // 城主取钱：`TAKECONQUESTGOLD`（`:4223-4236`）。
    assert_eq!(
        harness
            .world
            .take_conquest_gold(guilds.beta_leader, CONQUEST),
        Err(ConquestError::NotOwnerGuild),
        "非城主不能取钱"
    );
    assert_eq!(
        harness
            .world
            .take_conquest_gold(guilds.alpha_leader, CONQUEST)
            .expect("城主取钱"),
        300
    );
    assert_eq!(harness.world.conquest_gold_storage(CONQUEST), 0);
    assert_eq!(
        harness.world.guild(guilds.alpha).map(|guild| guild.gold),
        Some(300),
        "钱进了行会资金"
    );
}

// ---------------------------------------------------------------------------
// 8. 守卫 AI
// ---------------------------------------------------------------------------

#[test]
fn conquest_archers_only_shoot_enemy_guilds_during_the_war() {
    let mut harness = Harness::new(19);
    let guilds = two_guilds(&mut harness);
    set_owner(&mut harness, guilds.alpha);
    // 刷箭塔（`ConquestGuildArcherInfo.Spawn`，`ConquestGuildInfo.cs:518-544`）。
    let archers = harness.world.spawn_conquest_archers(CONQUEST);
    assert_eq!(archers.len(), 12, "12 座箭塔都刷出来");
    assert!(
        harness
            .world
            .monster(archers[0])
            .is_some_and(|monster| monster.conquest == Some(CONQUEST)),
        "箭塔绑定到攻城区"
    );
    // 把甲会会长（城主）和乙会会长都放到第一座箭塔旁边。
    let anchor = harness
        .world
        .monster(archers[0])
        .map(|monster| monster.position)
        .expect("箭塔");
    harness.teleport(guilds.alpha_leader, MAP, Point::new(anchor.x + 1, anchor.y));
    harness.teleport(guilds.beta_leader, MAP, Point::new(anchor.x, anchor.y + 1));

    // 未开战时箭塔不索敌（`!Conquest.WarIsOn`，`ConquestArcher.cs:53`）。
    harness.world.advance_clock(10_000);
    harness.world.tick();
    assert!(
        harness
            .world
            .monster(archers[0])
            .is_some_and(|monster| monster.target.is_none()),
        "停战期间箭塔不打人"
    );

    // 开战：只锁定非城主行会的乙会会长。
    harness
        .world
        .toggle_conquest_war(CONQUEST)
        .expect("强制开战");
    harness.world.advance_clock(10_000);
    harness.world.tick();
    let target = harness
        .world
        .monster(archers[0])
        .and_then(|monster| monster.target);
    assert_eq!(
        target,
        Some(guilds.beta_leader),
        "箭塔只射非城主行会的玩家（`ConquestArcher.cs:53`）"
    );
    // 再跑一会儿，乙会会长应该掉血（箭塔 300ms 的延迟伤害，`TownArcher.Attack`）。
    harness.world.advance_clock(10_000);
    harness.world.tick();
    let beta_hp = harness
        .world
        .player(guilds.beta_leader)
        .map(|player| player.hp)
        .expect("乙会会长");
    assert!(
        beta_hp
            < harness
                .world
                .player(guilds.beta_leader)
                .map_or(0, |p| p.stats.hp),
        "应该已经被箭塔打过"
    );
}

// ---------------------------------------------------------------------------
// 9. 持久化与重启
// ---------------------------------------------------------------------------

#[tokio::test]
async fn persistence_roundtrip_and_restart_recovery() {
    let store = MemoryStore::new();
    let mut harness = Harness::new(20);
    let guilds = two_guilds(&mut harness);
    set_owner(&mut harness, guilds.alpha);
    if let Some(conquest) = harness.world.conquest_mut(CONQUEST) {
        conquest.gold_storage = 1_234;
        conquest.npc_rate = 25;
        conquest.dirty = true;
    }
    harness
        .world
        .request_conquest_war_at(guilds.beta_leader, CONQUEST, MONDAY_18)
        .expect("乙会申请攻城");
    harness.flush(&store).await;

    let rows = store.conquests().await;
    assert_eq!(
        rows.len(),
        1,
        "只写有变化的攻城区（`NeedSave`，`Envir.cs:2739`）"
    );
    let row = &rows[0];
    assert_eq!(row.conquest_index, CONQUEST);
    assert_eq!(row.owner, Some(guilds.alpha));
    assert_eq!(row.gold_storage, 1_234);
    assert_eq!(row.npc_rate, 25);
    assert_eq!(row.attacker, Some(guilds.beta));
    assert_eq!(row.schedule_start_hour, 18);
    assert_eq!(row.schedule_war_length, 30);
    assert_eq!(row.schedule_days, 0b0111_1111, "七个星期布尔全开");
    assert!(row.king_points.is_empty());

    // 「重启」：新世界 + 同一个库。
    let mut restarted = Harness::new(21);
    let loaded = restarted.world.load_conquests(rows);
    assert_eq!(loaded, 1);
    let conquest = restarted.world.conquest(CONQUEST).expect("攻城区");
    assert_eq!(conquest.owner, guilds.alpha, "城主恢复");
    assert_eq!(conquest.attacker, guilds.beta, "攻打方恢复");
    assert_eq!(conquest.gold_storage, 1_234, "税收池恢复");
    assert_eq!(conquest.npc_rate, 25, "加价恢复");
    assert_eq!(conquest.info.name, "Sabuk Wall", "静态配表没被改写");
    assert_eq!(conquest.info.guards.len(), 12);
    // 恢复后不该再有待写记录。
    assert!(restarted.world.take_conquest_writes().is_empty());
}

#[tokio::test]
async fn war_state_survives_a_restart() {
    let store = MemoryStore::new();
    let mut harness = Harness::new(22);
    let guilds = two_guilds(&mut harness);
    harness
        .world
        .toggle_conquest_war(CONQUEST)
        .expect("强制开战");
    let end = harness
        .world
        .conquest(CONQUEST)
        .map(|conquest| conquest.war_end_time)
        .expect("停战时刻");
    harness.flush(&store).await;
    let rows = store.conquests().await;
    assert!(rows[0].war_is_on, "开战状态落盘");

    let mut restarted = Harness::new(23);
    restarted.world.load_conquests(rows);
    let conquest = restarted.world.conquest(CONQUEST).expect("攻城区");
    assert!(conquest.war_is_on, "重启后仍在开战");
    assert_eq!(conquest.start_type, ConquestType::Forced);
    assert_eq!(conquest.war_end_time, end, "停战时刻保持");
    assert_eq!(
        restarted
            .world
            .guild_conquest(guilds.alpha)
            .map(|c| c.info.index),
        None,
        "还没夺城"
    );
}

// ---------------------------------------------------------------------------
// 10. 传送点：只有城主行会能过
// ---------------------------------------------------------------------------

#[test]
fn conquest_gated_movement_requires_the_owning_guild() {
    let mut harness = Harness::new(24);
    let guilds = two_guilds(&mut harness);
    // 在 (10,10) 放一个 `ConquestIndex = 1` 的传送点，目标图是皇宫。
    let source = Point::new(10, 10);
    harness.world.set_map_movements(
        MAP,
        vec![mir_db::MovementInfo {
            map_index: PALACE_INDEX,
            source,
            destination: Point::new(20, 20),
            need_hole: false,
            need_move: false,
            conquest_index: CONQUEST,
            show_on_big_map: false,
            icon: 0,
        }],
    );
    // 甲会会长走到传送点上：还没拥有这座城 → 不触发。
    harness.teleport(guilds.beta_leader, MAP, Point::new(50, 50));
    harness.teleport(guilds.alpha_leader, MAP, Point::new(9, 10));
    harness
        .world
        .walk(guilds.alpha_leader, MirDirection::Right)
        .expect("走进传送点");
    harness.world.tick();
    assert_eq!(
        harness
            .world
            .player(guilds.alpha_leader)
            .map(|player| player.map.clone()),
        Some(MAP.to_owned()),
        "不是城主时传送点不生效（`PlayerObject.cs:4489-4493`）"
    );
    // 把甲会设成城主 → 传送生效。
    set_owner(&mut harness, guilds.alpha);
    // 走一步有 600ms 动作冷却（`MOVE_DELAY_MS`），先快进。
    harness.world.advance_clock(1_000);
    harness.teleport(guilds.alpha_leader, MAP, Point::new(9, 10));
    harness
        .world
        .walk(guilds.alpha_leader, MirDirection::Right)
        .expect("再走进传送点");
    harness.world.tick();
    assert_eq!(
        harness
            .world
            .player(guilds.alpha_leader)
            .map(|player| player.map.clone()),
        Some(PALACE.to_owned()),
        "城主行会成员可以通过"
    );
}

// ---------------------------------------------------------------------------
// 11. 存档文本编码
// ---------------------------------------------------------------------------

#[test]
fn score_and_owner_text_roundtrip() {
    let mut points = BTreeMap::new();
    points.insert(3, 7);
    points.insert(1, 0);
    points.insert(9, -2);
    let text = mir_server::encode_points(&points);
    assert_eq!(text, "3:7,9:-2", "0 分不落盘、按 ID 升序");
    assert_eq!(mir_server::decode_points(&text), {
        let mut expected = BTreeMap::new();
        expected.insert(3, 7);
        expected.insert(9, -2);
        expected
    });
    // 坏数据不 panic。
    assert!(mir_server::decode_points("abc,,1:x,:5").is_empty());
}
