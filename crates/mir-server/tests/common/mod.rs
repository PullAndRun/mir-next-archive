#![allow(dead_code)]

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{ChatType, ConquestType, MirClass, MirDirection, MirGender};
use mir_db::{ConquestArcherInfo, ConquestInfo, ConquestStructure, MapInfo, MirDb, MonsterInfo};
use mir_protocol::ServerPacket;
use mir_server::conquest::persist_conquest_writes;
use mir_server::gamedata::GameData;
use mir_server::{GuildSettings, MemoryStore, NpcState, Player, World, combat};
use mir_world::{Map, MapCell, MapVersion};
use tokio::sync::mpsc;

// ---------------------------------------------------------------------------
// 数据集（`docs/world/data-languages.md`）
// ---------------------------------------------------------------------------

/// `resource/community/suprcode-crystal-database`（各 variant 的父目录）。
#[must_use]
pub fn dataset_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("resource/community/suprcode-crystal-database")
}

/// 某个 variant 的 `Server.MirDB`；缺失（CI 没有 `resource/`）时返回 `None`。
#[must_use]
pub fn dataset_db_path(variant: &str) -> Option<PathBuf> {
    let path = dataset_root().join(variant).join("Server.MirDB");
    path.is_file().then_some(path)
}

/// 加载某个 variant 的完整 `GameData`（DB + 同目录 `Envir/`）。
#[must_use]
pub fn load_dataset(variant: &str) -> Option<Arc<GameData>> {
    GameData::load(dataset_db_path(variant)?).ok().map(Arc::new)
}

/// 默认数据集（`cjlaaa`，简体中文）：生产 `GameData::discover()` 的首选项。
#[must_use]
pub fn default_data() -> Option<Arc<GameData>> {
    load_dataset("cjlaaa")
}

/// Jev（英文）数据集：**只给「Jev 数据契约」用例显式钉住**。
///
/// 默认数据集已切 cjlaaa（中文），依赖英文名/英文任务脚本/英文 NPC 的用例如果
/// 还想验证 Jev 的数据契约，必须走这个入口；否则会随默认数据集漂移。
#[must_use]
pub fn jev_data() -> Option<Arc<GameData>> {
    load_dataset("Jev")
}

/// 攻城区主键（配表 `ConquestInfo.Index`）。
pub const CONQUEST: i32 = 1;
/// 攻城区所在地图（数据库主键 172 → 文件名 `3`，与真实数据一致）。
pub const MAP: &str = "3";
/// 地图数据库主键。
pub const MAP_INDEX: i32 = 172;
/// 皇宫地图（数据库主键 245 → 文件名 `D2116`）。
pub const PALACE: &str = "D2116";
/// 皇宫地图数据库主键。
pub const PALACE_INDEX: i32 = 245;
/// 攻城区中心：真实数据的沙巴克在 (641, 287)，但合成地图只有 60×60，
/// 所以测试用的攻城区把中心缩到地图里（半径 20 → 覆盖 10..50）。
pub const CONQUEST_CENTER: Point = Point::new(30, 30);
/// 攻城区半径（真实数据是 30；测试用 20 以便落在 60×60 的合成地图内）。
pub const CONQUEST_SIZE: u16 = 20;
/// 会长等级（行会要求 22）。
pub const LEADER_LEVEL: u16 = 30;
/// 建行会/申请攻城用的金币。
pub const GOLD: i64 = 2_000_000;
/// 「星期一 18:00 UTC」：`20003 * 86400_000 + 18 * 3600_000`。
///
/// `20003 + 3 = 20006 = 7 * 2858`，所以 `weekday_index` 算出 0（星期一）。
pub const MONDAY_18: i64 = 20_003 * 86_400_000 + 18 * 3_600_000;
/// 一天。
pub const DAY: i64 = 86_400_000;
/// 一分钟。
pub const MINUTE: i64 = 60_000;

// ---------------------------------------------------------------------------
// 脚手架
// ---------------------------------------------------------------------------

/// 行会配置：等级 22、金币 1,000,000、倍率 1%、每级 1 点升级点。
pub fn guild_settings() -> GuildSettings {
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
pub fn synthetic_map(file_name: &str) -> Arc<Map> {
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
pub fn conquest_info() -> ConquestInfo {
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
pub fn base_db(info: ConquestInfo) -> MirDb {
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
pub struct Harness {
    pub world: World,
    pub receivers: HashMap<u32, mpsc::UnboundedReceiver<Vec<u8>>>,
}

impl Harness {
    pub fn new(seed: u64) -> Self {
        Self::with_info(seed, conquest_info())
    }

    pub fn with_info(seed: u64, info: ConquestInfo) -> Self {
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

    pub fn add_player(&mut self, object_id: u32, name: &str, level: u16, gold: i64) -> u32 {
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
            bind_map_index: 0,
            bind_point: Point::default(),
            sender,
        });
        self.receivers.insert(object_id, receiver);
        object_id
    }

    pub fn name_of(&self, object_id: u32) -> String {
        self.world
            .player(object_id)
            .map_or_else(|| panic!("玩家 {object_id} 不在世界里"), |p| p.name.clone())
    }

    pub fn packets(&mut self, object_id: u32) -> Vec<ServerPacket> {
        let receiver = self.receivers.get_mut(&object_id).expect("玩家通道");
        let mut packets = Vec::new();
        while let Ok(frame) = receiver.try_recv() {
            let index = i16::from_le_bytes([frame[2], frame[3]]);
            packets.push(ServerPacket::decode(index, &frame[4..]).expect("解码服务端包"));
        }
        packets
    }

    pub fn clear(&mut self, object_id: u32) {
        let _ = self.packets(object_id);
    }

    pub fn create_guild(&mut self, leader: u32, name: &str) -> i32 {
        self.world
            .create_guild(leader, name)
            .unwrap_or_else(|error| panic!("创建行会 {name} 应成功：{error}"))
    }

    /// 让 `member` 加入 `inviter` 的行会。
    pub fn join(&mut self, inviter: u32, member: u32) {
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
    pub fn teleport(&mut self, object_id: u32, map: &str, point: Point) {
        assert!(
            self.world.place_player(object_id, map, point),
            "玩家 {object_id} 不在世界里"
        );
    }

    pub async fn flush(&mut self, store: &MemoryStore) {
        let writes = self.world.take_conquest_writes();
        persist_conquest_writes(store, writes)
            .await
            .expect("攻城落盘");
    }
}

/// 某个玩家收到的 `S.Chat` 文本。
pub fn chats(packets: &[ServerPacket]) -> Vec<(&str, ChatType)> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::Chat(chat) => Some((chat.message.as_str(), chat.type_)),
            _ => None,
        })
        .collect()
}

/// 新一轮「申请攻城」的常用前置：两个行会 + 各一个会长。
pub struct TwoGuilds {
    pub alpha_leader: u32,
    pub alpha: i32,
    pub beta_leader: u32,
    pub beta: i32,
}

pub fn two_guilds(harness: &mut Harness) -> TwoGuilds {
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
pub fn set_owner(harness: &mut Harness, guild_id: i32) {
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
