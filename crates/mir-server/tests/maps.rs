//! 多地图/切图/传送点的端到端测试（Crystal `PlayerObject.CompleteMapMovement` 等）。
//!
//! 全部确定性：
//! * 世界随机数固定种子（[`World::seed_rng`]）；
//! * 时间用 [`World::advance_clock`] 快进（不 sleep）；
//! * 地形是内存里合成的 60×60 空地图（[`MapCell::default`] 可走），
//!   传送点/刷怪点/`MapInfo` 都在内存数据库里构造；
//! * 只有最后的 `real_database_*` 用例读 `resource/` 里的真实数据，缺失时跳过。
//!
//! 每个断言尽量对应 Crystal 的一处行为，注释里写了出处（`文件.cs:行号`）。

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{
    LightSetting, MirClass, MirDirection, MirGender, Monster as MonsterImage, Stat, WeatherSetting,
};
use mir_db::{MapInfo, MirDb, MonsterInfo, MovementInfo, NpcInfo, RespawnInfo};
use mir_protocol::ServerPacket;
use mir_protocol::data::Stats;
use mir_server::combat;
use mir_server::gamedata::GameData;
use mir_server::maps::{MOVEMENT_DELAY_MS, MapChangeError};
use mir_server::{MonsterKind, NpcState, Player, World};
use mir_world::{Map, MapCell, MapVersion};
use tokio::sync::mpsc;

/// 地图 A（测试的「旧图」）。
const MAP_A: &str = "0";
/// 地图 B（测试的「新图」）。
const MAP_B: &str = "1";
/// 地图 A 的数据库主键（故意不是 0：0 在传送点里表示「同图」）。
const INDEX_A: i32 = 10;
/// 地图 B 的数据库主键。
const INDEX_B: i32 = 20;

// ---------------------------------------------------------------------------
// 脚手架
// ---------------------------------------------------------------------------

/// 每张图 60×60 的可走空地（无图块，只影响可走性）。
fn synthetic_map(name: &str) -> Arc<Map> {
    let width = 60;
    let height = 60;
    Map {
        width,
        height,
        cells: vec![MapCell::default(); usize::try_from(width * height).unwrap_or(0)],
        file_name: name.to_owned(),
        version: MapVersion::V0,
        doors: Vec::new(),
    }
    .into()
}

/// 测试怪物模板（HP 10、无攻击力）。
fn monster_info(index: i32, name: &str) -> MonsterInfo {
    MonsterInfo {
        index,
        name: name.to_owned(),
        image: u16::from(MonsterImage::Oma as u8),
        ai: 0,
        level: 1,
        view_range: 7,
        attack_speed: 2_500,
        move_speed: 1_800,
        experience: 5,
        stats: Stats::from_iter([(Stat::HP, 10), (Stat::MinDC, 0), (Stat::MaxDC, 0)]),
        ..MonsterInfo::default()
    }
}

/// 最小数据库：两张地图（`0` / `1`）+ 一只怪物模板，没有刷怪点/NPC。
fn base_db() -> MirDb {
    MirDb {
        maps: vec![
            MapInfo {
                index: INDEX_A,
                file_name: MAP_A.to_owned(),
                title: "比奇省".to_owned(),
                ..MapInfo::default()
            },
            MapInfo {
                index: INDEX_B,
                file_name: MAP_B.to_owned(),
                title: "边界村".to_owned(),
                ..MapInfo::default()
            },
        ],
        monsters: vec![monster_info(1, "测试怪")],
        ..MirDb::default()
    }
}

/// 每个用例一个独立的 `Envir/` 目录（NPC 脚本用），退出时删除。
struct Envir {
    root: PathBuf,
}

impl Envir {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "mir-next-maps-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("NPCs/Test")).expect("建 NPC 目录");
        Self { root }
    }

    fn script(&self, name: &str, text: &str) {
        let path = self.root.join("NPCs").join(format!("{name}.txt"));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("建脚本目录");
        }
        std::fs::write(&path, text).expect("写脚本");
    }

    fn drop_dir(&self) -> PathBuf {
        self.root.join("Drops")
    }
}

impl Drop for Envir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// 测试世界 + 每个玩家的收包通道（+ 可选的临时 `Envir/`，保活到用例结束）。
struct Harness {
    world: World,
    receivers: HashMap<u32, mpsc::UnboundedReceiver<Vec<u8>>>,
    /// 只为保活临时目录（`Drop` 时删除脚本）；内容不参与断言。
    #[allow(dead_code)]
    envir: Option<Envir>,
}

impl Harness {
    /// 空世界：两张合成地图、没有 `GameData`。
    fn new(seed: u64) -> Self {
        let mut world = World::new(999);
        world.seed_rng(seed);
        world.insert_map(MAP_A, synthetic_map(MAP_A));
        world.insert_map(MAP_B, synthetic_map(MAP_B));
        Self {
            world,
            receivers: HashMap::new(),
            envir: None,
        }
    }

    /// 用真实地图目录构造：没有内存数据库，地形按需从磁盘加载。
    fn real(seed: u64, map_root: &Path) -> Self {
        let mut world = World::new(999);
        world.seed_rng(seed);
        world.set_map_root(map_root);
        Self {
            world,
            receivers: HashMap::new(),
            envir: None,
        }
    }

    /// 带内存数据库的世界（`envir` 用于 NPC 脚本目录）。
    fn with_db(seed: u64, db: MirDb, envir: Option<Envir>) -> Self {
        let mut harness = Self::new(seed);
        let drop_dir = envir.as_ref().map(Envir::drop_dir);
        harness
            .world
            .set_game_data(Arc::new(GameData::from_db(db, drop_dir)));
        harness.envir = envir;
        harness
    }

    /// 放一个 1 级战士进世界。`full=false` 时只给 1 点血（保证不会被自然回血干扰断言）。
    fn add_player(&mut self, object_id: u32, map: &str, position: Point) -> u32 {
        let stats = combat::player_base_stats(1, MirClass::Warrior);
        let (sender, receiver) = mpsc::unbounded_channel();
        self.world.enter(Player {
            object_id,
            session_id: u64::from(object_id),
            character_id: i64::from(object_id),
            name: format!("玩家{object_id}"),
            map: map.to_owned(),
            position,
            direction: MirDirection::Down,
            level: 1,
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
            regen_time: 0,
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
            gold: 0,
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

    /// 把一个玩家挪到指定格并把动作计时清零（测试里等同于「已经站稳」）。
    fn ready(&mut self, object_id: u32) {
        if let Some(player) = self.world.player_mut(object_id) {
            player.action_time = 0;
            player.attack_time = 0;
        }
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

    fn has(&mut self, object_id: u32, matcher: impl Fn(&ServerPacket) -> bool) -> bool {
        self.packets(object_id).iter().any(matcher)
    }
}

/// 一条「走到 source 就传送」的传送点。
fn movement(map_index: i32, source: Point, destination: Point) -> MovementInfo {
    MovementInfo {
        map_index,
        source,
        destination,
        need_hole: false,
        need_move: false,
        conquest_index: 0,
        show_on_big_map: false,
        icon: 0,
    }
}

// ---------------------------------------------------------------------------
// 1. 同图传送 / 跨图可见性
// ---------------------------------------------------------------------------

#[test]
fn same_map_teleport_moves_the_player_and_sends_map_changed() {
    let mut harness = Harness::with_db(1, base_db(), None);
    let player = harness.add_player(1, MAP_A, Point::new(10, 10));
    harness.packets(player);

    let report = harness
        .world
        .change_map(player, MAP_A, Point::new(30, 30), MirDirection::Left)
        .expect("同图传送应当成功");

    assert!(!report.map_changed, "同图传送 map_changed = false");
    assert_eq!(report.from_map, MAP_A);
    assert_eq!(report.to_map, MAP_A);
    assert_eq!(report.map_index, INDEX_A);
    assert_eq!(report.position, Point::new(30, 30));
    let me = harness.world.player(player).expect("玩家还在");
    assert_eq!(me.position, Point::new(30, 30));
    assert_eq!(me.direction, MirDirection::Left);

    let packets = harness.packets(player);
    let changed = packets
        .iter()
        .find_map(|packet| match packet {
            ServerPacket::MapChanged(changed) => Some(changed),
            _ => None,
        })
        .expect("本人应收到 S.MapChanged（PlayerObject.cs:4566-4579）");
    assert_eq!(changed.map_index, INDEX_A);
    assert_eq!(changed.file_name, MAP_A);
    assert_eq!(changed.title, "比奇省");
    assert_eq!(changed.location, Point::new(30, 30));
    assert_eq!(changed.direction, MirDirection::Left);
}

#[test]
fn cross_map_change_removes_from_old_map_and_reveals_the_new_map() {
    let mut harness = Harness::with_db(2, base_db(), None);
    let mover = harness.add_player(1, MAP_A, Point::new(20, 20));
    let old_observer = harness.add_player(2, MAP_A, Point::new(21, 20));
    let new_observer = harness.add_player(3, MAP_B, Point::new(20, 21));
    let monster_id = harness.world.spawn_monster(
        MAP_B,
        MonsterKind {
            name: "测试怪".to_owned(),
            level: 1,
            view_range: 7,
            ..MonsterKind::scarecrow()
        },
        Point::new(21, 21),
    );
    harness.packets(mover);
    harness.packets(old_observer);
    harness.packets(new_observer);

    let report = harness
        .world
        .change_map(mover, MAP_B, Point::new(20, 20), MirDirection::Down)
        .expect("跨图应当成功");
    assert!(report.map_changed);
    assert_eq!(report.visible.monsters, 1, "新图上应看见 1 只怪");

    // 旧图的旁观者收到 ObjectRemove（Crystal `PlayerObject.cs:4506-4507`）。
    assert!(
        harness.has(old_observer, |packet| matches!(
            packet,
            ServerPacket::ObjectRemove(remove) if remove.object_id == mover
        )),
        "旧图玩家应收到 ObjectRemove"
    );
    // 新图的旁观者收到自己的外观（`CurrentMap.AddObject(this)`，`:4562`）。
    assert!(
        harness.has(new_observer, |packet| matches!(
            packet,
            ServerPacket::ObjectPlayer(player) if player.object_id == mover
        )),
        "新图玩家应收到 ObjectPlayer"
    );
    // 本人看到新图的怪（`GetObjects`，`:4583`）。
    assert!(
        harness.has(mover, |packet| matches!(
            packet,
            ServerPacket::ObjectMonster(monster) if monster.object_id == monster_id
        )),
        "切图后本人应收到新图的 ObjectMonster"
    );
}

// ---------------------------------------------------------------------------
// 2. 传送点触发（walk / turn）与延迟执行
// ---------------------------------------------------------------------------

#[test]
fn walking_onto_a_movement_source_teleports_on_the_next_tick() {
    let mut harness = Harness::with_db(3, base_db(), None);
    harness.world.set_map_movements(
        MAP_A,
        vec![movement(INDEX_B, Point::new(20, 20), Point::new(5, 5))],
    );
    let player = harness.add_player(1, MAP_A, Point::new(19, 20));
    harness.packets(player);

    // 走到 (20,20)：这一次走动作**作废**（Crystal `HumanObject.cs:2466` 命中传送点后返回），
    // 传送要等下一次 tick（本作的「延迟到下一个动作」）。
    let position = harness
        .world
        .walk(player, MirDirection::Right)
        .expect("走路本身不报错");
    assert_eq!(
        position,
        Point::new(19, 20),
        "命中传送点时人物留在原地，不广播 ObjectWalk"
    );
    assert_eq!(harness.world.pending_map_moves().len(), 1, "传送请求已排队");
    assert!(
        !harness.has(player, |packet| matches!(
            packet,
            ServerPacket::MapChanged(_)
        )),
        "本 tick 不该收到 MapChanged"
    );

    harness.world.tick();
    assert!(harness.world.pending_map_moves().is_empty(), "tick 已结算");
    let me = harness.world.player(player).expect("玩家还在");
    assert_eq!(me.map, MAP_B);
    assert_eq!(me.position, Point::new(5, 5));
    assert!(
        harness.has(
            player,
            |packet| matches!(packet, ServerPacket::MapChanged(changed) if changed.map_index == INDEX_B)
        ),
        "下一 tick 应收到 MapChanged"
    );
}

#[test]
fn turning_on_a_movement_source_counts_as_movement() {
    let mut harness = Harness::with_db(4, base_db(), None);
    harness.world.set_map_movements(
        MAP_A,
        vec![movement(INDEX_B, Point::new(20, 20), Point::new(6, 6))],
    );
    let player = harness.add_player(1, MAP_A, Point::new(20, 20));
    harness.packets(player);

    // Crystal `PlayerObject.Turn` 在设完方向后检查 `CurrentLocation`（`:4264-4273`）。
    harness.world.turn(player, MirDirection::Up).expect("转身");
    assert_eq!(harness.world.pending_map_moves().len(), 1);
    harness.world.tick();
    let me = harness.world.player(player).expect("玩家还在");
    assert_eq!(me.map, MAP_B);
    assert_eq!(me.direction, MirDirection::Up);
}

#[test]
fn need_hole_movement_point_requires_an_open_hole() {
    let mut harness = Harness::with_db(5, base_db(), None);
    let mut hole = movement(INDEX_B, Point::new(20, 20), Point::new(7, 7));
    hole.need_hole = true;
    harness.world.set_map_movements(MAP_A, vec![hole]);
    let player = harness.add_player(1, MAP_A, Point::new(19, 20));

    // 洞没开：走路照常完成，不触发传送（Crystal `PlayerObject.cs:4480-4487`）。
    let position = harness
        .world
        .walk(player, MirDirection::Right)
        .expect("走路");
    assert_eq!(position, Point::new(20, 20));
    assert!(harness.world.pending_map_moves().is_empty(), "洞没开不传送");
    assert_eq!(harness.world.player(player).unwrap().map, MAP_A);

    // 挖开洞后，站在这格上转身就能触发（`turn` 检查的是 `CurrentLocation`）。
    harness.world.set_hole(MAP_A, Point::new(20, 20), true);
    harness.ready(player);
    harness
        .world
        .turn(player, MirDirection::Right)
        .expect("转身");
    assert_eq!(harness.world.pending_map_moves().len(), 1, "洞开了才传送");
    harness.world.tick();
    assert_eq!(harness.world.player(player).unwrap().map, MAP_B);
}

#[test]
fn need_move_movement_point_records_the_destination_for_entermap() {
    let envir = Envir::new("needmove");
    envir.script(
        "Test/Teleport",
        "[@Main]\n\
         #ACT\n\
         ENTERMAP\n\
         #SAY\n\
         已送你过去。\n",
    );
    let mut db = base_db();
    db.npcs.push(NpcInfo {
        index: 1,
        map_index: INDEX_A,
        file_name: "Test/Teleport".to_owned(),
        name: "传送员".to_owned(),
        location: Point::new(19, 19),
        ..NpcInfo::default()
    });
    let mut harness = Harness::with_db(6, db, Some(envir));
    let npc = harness.world.spawn_npcs(MAP_A);
    assert_eq!(npc.len(), 1, "传送员应刷出来");
    let mut need_move = movement(INDEX_B, Point::new(20, 20), Point::new(8, 8));
    need_move.need_move = true;
    harness.world.set_map_movements(MAP_A, vec![need_move]);
    let player = harness.add_player(1, MAP_A, Point::new(19, 20));

    // `NeedMove` 只记目的地、不切图，走路照常完成（Crystal `PlayerObject.cs:4495-4500`）。
    let position = harness
        .world
        .walk(player, MirDirection::Right)
        .expect("走路");
    assert_eq!(position, Point::new(20, 20));
    assert!(harness.world.pending_map_moves().is_empty());
    let state = &harness.world.player(player).unwrap().npc;
    assert_eq!(state.move_map.as_deref(), Some(MAP_B));
    assert_eq!(state.move_coord, Some(Point::new(8, 8)));

    // NPC 的 ENTERMAP 动作消费它（`NPCSegment.cs:4065-4074`）。
    harness
        .world
        .call_npc(player, npc[0], "@Main")
        .expect("NPC 对话应执行");
    let me = harness.world.player(player).expect("玩家还在");
    assert_eq!(me.map, MAP_B);
    assert_eq!(me.position, Point::new(8, 8));
    let state = &me.npc;
    assert_eq!(state.move_map, None, "ENTERMAP 会清掉目的地");
    assert_eq!(state.move_coord, None);
}

// ---------------------------------------------------------------------------
// 3. 首次进入：刷怪点 / NPC / 地面物品
// ---------------------------------------------------------------------------

#[test]
fn first_entry_registers_respawns_and_spawns_monsters_only_once() {
    let mut db = base_db();
    db.maps[1].respawns.push(RespawnInfo {
        monster_index: 1,
        location: Point::new(10, 10),
        count: 2,
        spread: 2,
        delay: 1,
        ..RespawnInfo::default()
    });
    let mut harness = Harness::with_db(7, db, None);
    let player = harness.add_player(1, MAP_A, Point::new(20, 20));

    let report = harness
        .world
        .change_map(player, MAP_B, Point::new(10, 10), MirDirection::Down)
        .expect("切图");
    assert!(report.first_entry, "第一次进入应初始化");
    assert_eq!(report.visible.monsters, 2);
    let groups: Vec<_> = harness
        .world
        .respawn_state()
        .into_iter()
        .filter(|state| state.map == MAP_B)
        .collect();
    assert_eq!(groups.len(), 1, "B 图应注册 1 组刷怪点");
    assert_eq!(groups[0].alive, 2);
    assert_eq!(
        harness
            .world
            .monsters()
            .iter()
            .filter(|monster| monster.map == MAP_B)
            .count(),
        2,
        "刷怪点应铺满 count"
    );

    // 回 A 再去 B：不重复注册（Crystal 建图只建一次）。
    harness
        .world
        .change_map(player, MAP_A, Point::new(20, 20), MirDirection::Down)
        .expect("回 A");
    let report = harness
        .world
        .change_map(player, MAP_B, Point::new(10, 10), MirDirection::Down)
        .expect("再去 B");
    assert!(!report.first_entry, "第二次进入不重新初始化");
    assert_eq!(harness.world.respawn_state().len(), 1, "刷怪组没有变多");
    assert_eq!(
        harness
            .world
            .monsters()
            .iter()
            .filter(|monster| monster.map == MAP_B)
            .count(),
        2,
        "怪物没有翻倍"
    );
}

#[test]
fn ground_items_and_npcs_are_visible_after_map_change() {
    let envir = Envir::new("visible");
    let mut db = base_db();
    db.npcs.push(NpcInfo {
        index: 1,
        map_index: INDEX_B,
        file_name: "Test/Shop".to_owned(),
        name: "边界商人".to_owned(),
        location: Point::new(21, 20),
        ..NpcInfo::default()
    });
    let mut harness = Harness::with_db(8, db, Some(envir));
    let npc = harness.world.spawn_npcs(MAP_B);
    assert_eq!(npc.len(), 1);
    let item = harness
        .world
        .spawn_ground_item(MAP_B, Point::new(20, 21), None, 123, None);
    let player = harness.add_player(1, MAP_A, Point::new(20, 20));
    harness.packets(player);

    harness
        .world
        .change_map(player, MAP_B, Point::new(20, 20), MirDirection::Down)
        .expect("切图");

    let packets = harness.packets(player);
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::ObjectNPC(object) if object.object_id == npc[0]
        )),
        "切图后应看到新图的 NPC，实际 {packets:?}"
    );
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::ObjectGold(gold) if gold.object_id == item && gold.gold == 123
        )),
        "切图后应看到新图的金币堆，实际 {packets:?}"
    );
    let me = harness.world.player(player).unwrap();
    assert!(me.visible_npcs.contains(&npc[0]));
    assert!(me.visible_items.contains(&item));
}

#[test]
fn map_change_clears_the_old_visibility_sets() {
    let envir = Envir::new("clear");
    let mut db = base_db();
    db.npcs.push(NpcInfo {
        index: 1,
        map_index: INDEX_B,
        file_name: "Test/Shop".to_owned(),
        name: "边界商人".to_owned(),
        location: Point::new(21, 20),
        ..NpcInfo::default()
    });
    let mut harness = Harness::with_db(9, db, Some(envir));
    let old_monster =
        harness
            .world
            .spawn_monster(MAP_A, MonsterKind::scarecrow(), Point::new(21, 20));
    let old_item = harness
        .world
        .spawn_ground_item(MAP_A, Point::new(20, 21), None, 7, None);
    let new_monster =
        harness
            .world
            .spawn_monster(MAP_B, MonsterKind::scarecrow(), Point::new(20, 21));
    let npcs = harness.world.spawn_npcs(MAP_B);
    let player = harness.add_player(1, MAP_A, Point::new(20, 20));
    let before = harness.world.player(player).unwrap();
    assert!(before.visible_monsters.contains(&old_monster));
    assert!(before.visible_items.contains(&old_item));

    harness
        .world
        .change_map(player, MAP_B, Point::new(20, 20), MirDirection::Down)
        .expect("切图");

    let after = harness.world.player(player).unwrap();
    assert!(
        !after.visible_monsters.contains(&old_monster),
        "旧图怪物应从可见集合里清掉"
    );
    assert!(!after.visible_items.contains(&old_item), "旧图物品应清掉");
    assert_eq!(
        after.visible_monsters,
        BTreeSet::from([new_monster]),
        "只剩下新图的怪物"
    );
    assert!(after.visible_items.is_empty());
    assert_eq!(after.visible_npcs, npcs.into_iter().collect());
}

#[test]
fn two_map_changes_in_a_row_do_not_leak_monsters() {
    let mut db = base_db();
    db.maps[1].respawns.push(RespawnInfo {
        monster_index: 1,
        location: Point::new(10, 10),
        count: 3,
        spread: 2,
        delay: 1,
        ..RespawnInfo::default()
    });
    let mut harness = Harness::with_db(10, db, None);
    let player = harness.add_player(1, MAP_A, Point::new(20, 20));
    for round in 0..2 {
        harness
            .world
            .change_map(player, MAP_B, Point::new(10, 10), MirDirection::Down)
            .expect("去 B");
        assert_eq!(
            harness
                .world
                .monsters()
                .iter()
                .filter(|monster| monster.map == MAP_B)
                .count(),
            3,
            "第 {} 轮：B 图怪物数量应稳定在 3",
            round + 1
        );
        harness
            .world
            .change_map(player, MAP_A, Point::new(20, 20), MirDirection::Down)
            .expect("回 A");
        assert_eq!(
            harness
                .world
                .monsters()
                .iter()
                .filter(|monster| monster.map == MAP_A)
                .count(),
            0,
            "A 图没有刷怪点"
        );
    }
    assert_eq!(harness.world.respawn_state().len(), 1, "刷怪组始终只有一组");
}

// ---------------------------------------------------------------------------
// 4. NPC 动作 MAPMOVE
// ---------------------------------------------------------------------------

#[test]
fn npc_move_actions_teleport_by_file_name_and_by_index() {
    let envir = Envir::new("npc-move");
    // `MAPMOVE <地图文件名> <x> <y>`（任务要求的写法，等价 Crystal 的 NPC 动作 `MOVE`）。
    envir.script(
        "Test/Teleport",
        "[@Main]\n\
         #ACT\n\
         MAPMOVE 1 8 9\n\
         #SAY\n\
         走了。\n",
    );
    // `MOVE <地图主键> <x> <y>`：数字参数先按 `MapInfo.Index` 查（本作扩展）。
    envir.script(
        "Test/TeleportIndex",
        "[@Main]\n\
         #ACT\n\
         MOVE 20 9 8\n\
         #SAY\n\
         按主键走的。\n",
    );
    let mut db = base_db();
    db.npcs.push(NpcInfo {
        index: 1,
        map_index: INDEX_A,
        file_name: "Test/Teleport".to_owned(),
        name: "传送员".to_owned(),
        location: Point::new(19, 19),
        ..NpcInfo::default()
    });
    db.npcs.push(NpcInfo {
        index: 2,
        map_index: INDEX_A,
        file_name: "Test/TeleportIndex".to_owned(),
        name: "传送员二号".to_owned(),
        location: Point::new(19, 21),
        ..NpcInfo::default()
    });
    let mut harness = Harness::with_db(11, db, Some(envir));
    let npcs = harness.world.spawn_npcs(MAP_A);
    assert_eq!(npcs.len(), 2);
    let player = harness.add_player(1, MAP_A, Point::new(19, 20));
    harness.packets(player);

    harness
        .world
        .call_npc(player, npcs[0], "@Main")
        .expect("MAPMOVE 应执行");
    let me = harness.world.player(player).expect("玩家还在");
    assert_eq!(me.map, MAP_B);
    assert_eq!(me.position, Point::new(8, 9));
    assert!(
        harness.has(
            player,
            |packet| matches!(packet, ServerPacket::MapChanged(changed) if changed.file_name == MAP_B)
        ),
        "NPC 切图后本人应收到 MapChanged"
    );

    harness
        .world
        .change_map(player, MAP_A, Point::new(19, 20), MirDirection::Down)
        .expect("回到 A");
    harness
        .world
        .call_npc(player, npcs[1], "@Main")
        .expect("MOVE 应执行");
    let me = harness.world.player(player).expect("玩家还在");
    assert_eq!(me.map, MAP_B);
    assert_eq!(me.position, Point::new(9, 8));
}

// ---------------------------------------------------------------------------
// 5. 落点校正 / 冷却 / 错误
// ---------------------------------------------------------------------------

#[test]
fn out_of_bounds_destination_falls_back_inside_the_map() {
    let mut harness = Harness::with_db(12, base_db(), None);
    let player = harness.add_player(1, MAP_A, Point::new(20, 20));

    let report = harness
        .world
        .change_map(player, MAP_B, Point::new(999, -5), MirDirection::Down)
        .expect("越界目标点应回退而不是失败");
    assert_eq!(
        report.fallback_from,
        Some(Point::new(999, -5)),
        "原始请求点要记录下来"
    );
    assert_eq!(report.position, Point::new(59, 0), "夹进 60×60 地图的边界");
    let me = harness.world.player(player).expect("玩家还在");
    assert_eq!(me.map, MAP_B);
    assert_eq!(me.position, Point::new(59, 0));
}

#[test]
fn movement_delay_blocks_an_immediate_retrigger() {
    let mut harness = Harness::with_db(13, base_db(), None);
    harness.world.set_map_movements(
        MAP_A,
        vec![movement(INDEX_B, Point::new(20, 20), Point::new(5, 5))],
    );
    harness.world.set_map_movements(
        MAP_B,
        vec![movement(INDEX_A, Point::new(5, 5), Point::new(20, 20))],
    );
    let player = harness.add_player(1, MAP_A, Point::new(19, 20));
    // 冻结时钟：下面要精确断言 `MovementTime = now + MovementDelay`。
    harness.world.advance_clock(0);
    harness
        .world
        .walk(player, MirDirection::Right)
        .expect("走到传送点");
    harness.world.tick();
    assert_eq!(harness.world.player(player).unwrap().map, MAP_B);
    assert_eq!(
        harness.world.player(player).unwrap().position,
        Point::new(5, 5)
    );
    let movement_time = harness.world.player(player).unwrap().movement_time;
    assert_eq!(
        movement_time,
        harness.world.now() + MOVEMENT_DELAY_MS,
        "MovementTime = now + MovementDelay（PlayerObject.cs:4564）"
    );

    // 冷却中：站在 B 的传送点上转身也不触发（`PlayerObject.cs:4461`）。
    harness.ready(player);
    harness.world.turn(player, MirDirection::Up).expect("转身");
    assert!(
        harness.world.pending_map_moves().is_empty(),
        "冷却期内不触发"
    );

    // 冷却结束：再转身就触发。
    harness.world.advance_clock(MOVEMENT_DELAY_MS);
    harness.ready(player);
    harness
        .world
        .turn(player, MirDirection::Down)
        .expect("转身");
    assert_eq!(harness.world.pending_map_moves().len(), 1);
    harness.world.tick();
    assert_eq!(harness.world.player(player).unwrap().map, MAP_A);
}

#[test]
fn movement_points_come_from_the_database_on_first_entry() {
    let mut db = base_db();
    db.maps[0]
        .movements
        .push(movement(INDEX_B, Point::new(20, 20), Point::new(11, 12)));
    let mut harness = Harness::with_db(14, db, None);
    let player = harness.add_player(1, MAP_A, Point::new(19, 20));
    // `enter` → `initialize_map` 已把 `MapInfo.Movements` 灌进世界。
    assert_eq!(harness.world.movements_on(MAP_A).len(), 1);

    harness
        .world
        .walk(player, MirDirection::Right)
        .expect("走路");
    harness.world.tick();
    let me = harness.world.player(player).expect("玩家还在");
    assert_eq!(me.map, MAP_B);
    assert_eq!(me.position, Point::new(11, 12));
}

#[test]
fn unknown_map_change_reports_an_error_and_keeps_the_player_in_place() {
    let mut harness = Harness::with_db(15, base_db(), None);
    let player = harness.add_player(1, MAP_A, Point::new(20, 20));
    let error = harness
        .world
        .change_map(player, "不存在的图", Point::new(1, 1), MirDirection::Down)
        .expect_err("不存在的图应失败");
    assert_eq!(
        error,
        MapChangeError::MapNotFound {
            map: "不存在的图".to_owned()
        }
    );
    let me = harness.world.player(player).expect("玩家还在");
    assert_eq!(me.map, MAP_A);
    assert_eq!(me.position, Point::new(20, 20));
}

#[test]
fn map_changed_packet_carries_the_full_map_info() {
    let mut db = base_db();
    let info = &mut db.maps[1];
    info.title = "龙之谷".to_owned();
    info.mini_map = 3;
    info.big_map = 9;
    info.light = 2;
    info.music = 8;
    info.map_dark_light = 4;
    info.weather_particles = 2;
    let mut harness = Harness::with_db(16, db, None);
    let player = harness.add_player(1, MAP_A, Point::new(20, 20));
    harness.packets(player);

    harness
        .world
        .change_map(player, MAP_B, Point::new(1, 2), MirDirection::UpLeft)
        .expect("切图");

    let packets = harness.packets(player);
    let changed = packets
        .iter()
        .find_map(|packet| match packet {
            ServerPacket::MapChanged(changed) => Some(changed.clone()),
            _ => None,
        })
        .expect("应收到 MapChanged");
    assert_eq!(changed.map_index, INDEX_B);
    assert_eq!(changed.file_name, MAP_B);
    assert_eq!(changed.title, "龙之谷");
    assert_eq!(changed.mini_map, 3);
    assert_eq!(changed.big_map, 9);
    assert_eq!(changed.lights, LightSetting::Day);
    assert_eq!(changed.location, Point::new(1, 2));
    assert_eq!(changed.direction, MirDirection::UpLeft);
    assert_eq!(changed.map_dark_light, 4);
    assert_eq!(changed.music, 8);
    assert_eq!(changed.weather, WeatherSetting::RedEmber);
}

// ---------------------------------------------------------------------------
// 6. 真实数据冒烟
// ---------------------------------------------------------------------------

#[test]
fn real_database_has_movement_points_that_resolve_to_maps() {
    let Ok(data) = GameData::discover() else {
        eprintln!("跳过：resource/ 里没有 Server.MirDB 样本");
        return;
    };
    let Some(best) = data
        .db
        .maps
        .iter()
        .max_by_key(|info| info.movements.len())
        .filter(|info| !info.movements.is_empty())
    else {
        eprintln!("跳过：样本数据库里没有任何传送点");
        return;
    };
    assert!(
        best.movements.len() >= 3,
        "{}（{}）应该有至少 3 个传送点，实际 {}",
        best.file_name,
        best.title,
        best.movements.len()
    );
    println!(
        "真实数据冒烟：地图 {}（{}）有 {} 个传送点",
        best.file_name,
        best.title,
        best.movements.len()
    );
    // 至少有一条能解析出目标地图（同图传送 map_index == 0，其余按主键查）。
    let mut resolved = 0;
    for movement in best.movements.iter().take(5) {
        let target = if movement.map_index == 0 {
            format!("同图 {}", best.file_name)
        } else {
            match data.map_info_by_index(movement.map_index) {
                Some(info) => {
                    resolved += 1;
                    format!("{}（{}）", info.file_name, info.title)
                }
                None => format!("未知地图 #{}", movement.map_index),
            }
        };
        println!(
            "  ({}, {}) → {} 的 ({}, {}) · need_hole={} need_move={}",
            movement.source.x,
            movement.source.y,
            target,
            movement.destination.x,
            movement.destination.y,
            movement.need_hole,
            movement.need_move
        );
    }
    assert!(resolved > 0, "至少有一条跨图传送点能按主键查到目标地图");
}

/// 真实 `.map` 文件冒烟：按数据库里的一条跨图传送点，把玩家从真实地形切到真实目标图。
///
/// 不建内存数据库：`change_map` 只按文件名找地形，正好验证 `resource/map/*.map` 能读、
/// 落点校正对真实地形有效、`S.MapChanged` 的 `FileName` 正确。
#[test]
fn real_map_files_load_and_change_map_moves_between_them() {
    let Ok(data) = GameData::discover() else {
        eprintln!("跳过：resource/ 里没有 Server.MirDB 样本");
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let map_root = root.join("resource/map");
    if !map_root.is_dir() {
        eprintln!("跳过：{} 不存在", map_root.display());
        return;
    }
    let Some((from_file, movement, target_file)) = data.db.maps.iter().find_map(|info| {
        if !map_root.join(format!("{}.map", info.file_name)).is_file() {
            return None;
        }
        info.movements.iter().find_map(|movement| {
            if movement.map_index <= 0 {
                return None;
            }
            let target = data.map_info_by_index(movement.map_index)?;
            map_root
                .join(format!("{}.map", target.file_name))
                .is_file()
                .then(|| {
                    (
                        info.file_name.clone(),
                        movement.clone(),
                        target.file_name.clone(),
                    )
                })
        })
    }) else {
        eprintln!("跳过：样本里没有指向已有 .map 文件的跨图传送点");
        return;
    };

    let mut harness = Harness::real(21, &map_root);
    let player = harness.add_player(1, &from_file, movement.source);
    harness.packets(player);
    assert!(
        harness.world.map_of(&from_file).is_some(),
        "进入地图时应该把 {from_file} 的真地形读进来"
    );
    let (from_max_x, from_max_y) = harness.world.map_bounds(&from_file);
    assert!(from_max_x > 0 && from_max_y > 0, "真实地图尺寸应非零");

    let report = harness
        .world
        .change_map(
            player,
            &target_file,
            movement.destination,
            MirDirection::Down,
        )
        .expect("真实地图切图应成功");
    assert!(report.map_changed);
    assert_eq!(report.to_map, target_file);
    assert!(
        harness.world.map_of(&target_file).is_some(),
        "目标地形应已加载"
    );
    let (max_x, max_y) = harness.world.map_bounds(&target_file);
    assert!(
        (0..=max_x).contains(&report.position.x) && (0..=max_y).contains(&report.position.y),
        "落点 ({}, {}) 应在 {}×{} 的地图内",
        report.position.x,
        report.position.y,
        max_x + 1,
        max_y + 1
    );
    assert!(
        harness.has(player, |packet| matches!(
            packet,
            ServerPacket::MapChanged(changed) if changed.file_name == target_file
        )),
        "本人应收到目标地图的 MapChanged"
    );
    println!(
        "真实地图切图：{} ({}, {}) → {} ({}, {}) · 地形 {}×{}",
        from_file,
        movement.source.x,
        movement.source.y,
        target_file,
        report.position.x,
        report.position.y,
        max_x + 1,
        max_y + 1
    );
}

// ---------------------------------------------------------------------------
// 7. 自然传送点：比奇广场能不能**走进**一个传送点（t93）
// ---------------------------------------------------------------------------
//
// 背景（t93）：playtest 之前只能靠 GM `@map` 兜底，理由是「最近的自然传送点在 274 格外」。
// 那个 274 格是**客户端可见**的传送点（`S.NewMapInfo.movements`）——Crystal 的
// `PlayerObject.CheckMapInfo`（`Server/MirObjects/PlayerObject.cs:1009-1056`）只把
// `MovementInfo.ShowOnBigMap` 的那些发给客户端（大地图图标），本作照抄。
// 真正的传送点表是 `MapInfo.Movements`（`Shared/MapInfo.cs`），比奇有 72 条，
// 其中有好几条就在广场 30 格内。下面两个用例分别锁住「数据在」与「走得过去」。

/// 起始点（`ServerConfig.start_point` 默认值，比奇广场）。
const BICHON_START: Point = Point { x: 330, y: 270 };

/// 自然传送点：`MapInfo.Movements` 里**能走进去**（不需要挖洞/不需要 `need_move`）、
/// 目标地图存在且 `.map` 文件在磁盘上的那些。
fn walkable_movements_of_bichon(
    data: &GameData,
    map_root: &Path,
) -> Vec<(String, MovementInfo, String)> {
    let Some(info) = data.map_info("0") else {
        return Vec::new();
    };
    info.movements
        .iter()
        .filter(|movement| movement.map_index > 0 && !movement.need_hole && !movement.need_move)
        .filter_map(|movement| {
            let target = data.map_info_by_index(movement.map_index)?;
            map_root
                .join(format!("{}.map", target.file_name))
                .is_file()
                .then(|| {
                    (
                        info.file_name.clone(),
                        movement.clone(),
                        target.file_name.clone(),
                    )
                })
        })
        .collect()
}

/// 数据侧：比奇的传送点表来自真实 `Server.MirDB`，且**广场附近就有**可用的。
#[test]
fn bichon_has_a_walkable_movement_point_near_the_plaza() {
    let Ok(data) = GameData::discover() else {
        eprintln!("跳过：resource/ 里没有 Server.MirDB 样本");
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let map_root = root.join("resource/map");
    let Some(info) = data.map_info("0") else {
        eprintln!("跳过：样本里没有比奇（map \"0\"）");
        return;
    };
    assert!(
        info.movements.len() >= 20,
        "比奇应该有几十条传送点（真实数据 72 条），实际 {}",
        info.movements.len()
    );

    // 客户端可见的（`ShowOnBigMap`）只是大地图图标那一小撮——这正是 playtest 之前
    // 「最近传送点 274 格」的原因，不是解析缺失。
    let big_map = info.movements.iter().filter(|m| m.show_on_big_map).count();
    assert!(
        big_map < info.movements.len(),
        "真实数据里 ShowOnBigMap 只占少数（大地图图标），不该等于全部"
    );
    println!(
        "比奇传送点：{} 条（其中 ShowOnBigMap {} 条；客户端 `S.NewMapInfo` 只收后者）",
        info.movements.len(),
        big_map
    );

    let walkable = walkable_movements_of_bichon(&data, &map_root);
    assert!(
        !walkable.is_empty(),
        "比奇至少要有 1 条不需要挖洞、目标图存在的传送点"
    );
    let (_, nearest, target) = walkable
        .iter()
        .min_by_key(|(_, movement, _)| {
            let dx = (movement.source.x - BICHON_START.x).abs();
            let dy = (movement.source.y - BICHON_START.y).abs();
            dx.max(dy)
        })
        .expect("非空");
    let distance = (nearest.source.x - BICHON_START.x)
        .abs()
        .max((nearest.source.y - BICHON_START.y).abs());
    println!(
        "广场 {BICHON_START:?} 最近的可走传送点：{:?} → 地图 {} 的 {:?}（{} 格）",
        nearest.source, target, nearest.destination, distance
    );
    assert!(
        distance <= 40,
        "广场 40 格内应该有一条自然传送点（实际最近 {distance} 格），否则玩家没法自然换图"
    );
}

/// 端到端：从广场一步步走进最近的传送点 → 下一个 tick 完成换图。
///
/// 用真实地形（`resource/map/*.map`）+ 真实传送点表，玩家按贪心方向走（撞墙换方向），
/// 断言「踩上传送点 → `world.tick()` 结算 → 地图/坐标变成传送点的目的地」。
#[test]
fn player_can_walk_from_the_plaza_into_a_natural_teleport_point() {
    let Ok(data) = GameData::discover() else {
        eprintln!("跳过：resource/ 里没有 Server.MirDB 样本");
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let map_root = root.join("resource/map");
    if !map_root.is_dir() {
        eprintln!("跳过：{} 不存在", map_root.display());
        return;
    }
    let data = Arc::new(data);
    let mut walkable = walkable_movements_of_bichon(&data, &map_root);
    if walkable.is_empty() {
        eprintln!("跳过：比奇没有可走的跨图传送点");
        return;
    }
    walkable.sort_by_key(|(_, movement, _)| {
        (movement.source.x - BICHON_START.x)
            .abs()
            .max((movement.source.y - BICHON_START.y).abs())
    });
    let (_, movement, target_file) = walkable.remove(0);
    let target_index = movement.map_index;

    let mut harness = Harness::real(7, &map_root);
    // 传送点解析要按 `MapIndex` 查 `MapInfo`（`maps::resolve_movement_target`），
    // 所以世界必须挂上真实 `GameData`；否则那条传送点会被当成「目标图不存在」跳过。
    harness.world.set_game_data(Arc::clone(&data));
    // 传送点表必须**在地形加载之后**设置：`World` 第一次走到某张图时才会载入 `.map`
    // 并注册 `MapInfo.Movements`，先设置会被那次注册覆盖。这里先走生产路径
    // （`initialize_map` 载入地形 + NPC + 刷怪点），再把真实传送点表覆写一遍。
    let report = harness.world.initialize_map("0");
    assert!(
        report.initialized,
        "比奇地形应该能加载（resource/map/0.map）"
    );
    harness
        .world
        .set_map_movements("0", data.map_info("0").expect("比奇").movements.clone());
    let player = harness.add_player(1, "0", BICHON_START);
    harness.packets(player);

    // NPC 与**当前**存活怪物都是占格对象：不同数据集（cjlaaa/Jev）的 NPC/刷怪布局不同，
    // 必须从 BFS 里排除，否则会规划出「穿过 NPC/站桩怪」的路径、真走时被连续拒绝
    // （cjlaaa 的布局就会触发）。怪物会移动，真走时仍用 tick 重试兜底。
    let map = harness.world.map_of("0").expect("比奇地形").clone();
    let mut occupied: std::collections::HashSet<(i32, i32)> = harness
        .world
        .npcs()
        .iter()
        .filter(|npc| npc.map == "0")
        .map(|npc| (npc.position.x, npc.position.y))
        .collect();
    occupied.extend(
        harness
            .world
            .monsters()
            .iter()
            .filter(|monster| monster.map == "0" && !monster.dead)
            .map(|monster| (monster.position.x, monster.position.y)),
    );
    let mut queue = std::collections::VecDeque::from([BICHON_START]);
    let mut came_from: HashMap<(i32, i32), Point> = HashMap::new();
    let mut seen = std::collections::HashSet::from([(BICHON_START.x, BICHON_START.y)]);
    let mut found = false;
    while let Some(cell) = queue.pop_front() {
        if cell == movement.source {
            found = true;
            break;
        }
        for (dx, dy) in [(1, 0), (0, 1), (-1, 0), (0, -1)] {
            let next = Point::new(cell.x + dx, cell.y + dy);
            if !map.walkable(next.x, next.y)
                || occupied.contains(&(next.x, next.y))
                || !seen.insert((next.x, next.y))
            {
                continue;
            }
            came_from.insert((next.x, next.y), cell);
            queue.push_back(next);
        }
    }
    assert!(
        found,
        "比奇地形上从 {BICHON_START:?} 走不到传送点 {:?}（BFS 无路；占格对象 {} 个）",
        movement.source,
        occupied.len()
    );

    // 回溯出路径（不含起点）。
    let mut path = Vec::new();
    let mut cursor = movement.source;
    while let Some(previous) = came_from.get(&(cursor.x, cursor.y)) {
        path.push(cursor);
        cursor = *previous;
    }
    path.reverse();
    println!(
        "自然换图：{BICHON_START:?} → {:?} 共 {} 步",
        movement.source,
        path.len()
    );

    // 沿路径真走（每步清冷却并推进世界时钟，模拟 600ms 的走位间隔）。
    for cell in &path {
        let position = harness.world.player(player).expect("玩家还在").position;
        let direction = match (cell.x - position.x, cell.y - position.y) {
            (1, 0) => MirDirection::Right,
            (0, 1) => MirDirection::Down,
            (-1, 0) => MirDirection::Left,
            (0, -1) => MirDirection::Up,
            other => panic!("路径上出现非四邻格 {other:?}（{position:?} → {cell:?}）"),
        };
        // 真实图上有怪：被临时占格时推进 tick 再试（怪物会走位）。
        let mut walked = false;
        for _ in 0..6 {
            harness.ready(player);
            harness
                .world
                .advance_clock(mir_server::world::MOVE_DELAY_MS);
            if harness.world.walk(player, direction).is_ok() {
                walked = true;
                break;
            }
            harness.world.tick();
        }
        assert!(
            walked,
            "沿 BFS 路径走 {position:?} → {cell:?} 连续 6 次被拒"
        );
        harness.world.tick();
        if harness.world.player(player).expect("玩家还在").map != "0" {
            // 踩上传送点 → tick 已经把人物换到目标图，后面的路不用走了。
            break;
        }
    }

    // 踩上传送点那一步会被作废、换图排队到下一个 tick（`walking_onto_a_movement_source_*`），
    // 上面每走一步都 `tick()` 过一次，所以走到传送点后地图已经切换。
    let me = harness.world.player(player).expect("玩家还在");
    assert_eq!(me.map, target_file, "应该换到传送点的目标地图");
    assert_eq!(
        (me.position.x, me.position.y),
        (movement.destination.x, movement.destination.y),
        "落点应该是传送点的目的地"
    );
    assert!(
        harness.has(
            player,
            |packet| matches!(packet, ServerPacket::MapChanged(changed) if changed.map_index == target_index)
        ),
        "本人应收到目标地图的 S.MapChanged"
    );
    println!(
        "自然换图：比奇 {:?} → 地图 {target_file}（#{target_index}）{:?}",
        movement.source, movement.destination
    );
}
