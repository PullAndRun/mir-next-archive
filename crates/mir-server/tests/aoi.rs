//! AOI（网格索引）与视野半径的端到端测试。
//!
//! 覆盖三件事：
//! 1. 视野半径按 Crystal `Globals.DataRange = 16`（`Shared/Globals.cs:37`）判定；
//! 2. 广播/索敌/占格走 AOI 网格，且索引随「进图 / 走跑 / 切图 / 测试摆位」同步；
//! 3. 门状态机（开/关、5 秒自动关、`S.Opendoor` 线级包）。
//!
//! 网格本身的随机对拍在 `mir-server/src/aoi.rs` 的单元测试里
//! （200 个对象 × 1000 步随机增删移动，每一步与暴力扫描比对）。

use std::collections::BTreeSet;
use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{AttackMode, MirClass, MirDirection, MirGender};
use mir_protocol::ServerPacket;
use mir_server::combat;
use mir_server::world::MoveError;
use mir_server::{MonsterKind, NpcState, Player, TradeState, World};
use mir_world::Map;
use tokio::sync::mpsc;

/// 构造一个只有基础字段的 1 级战士（其余字段沿用各集成测试的写法）。
fn player(
    object_id: u32,
    name: &str,
    map: &str,
    position: Point,
) -> (Player, mpsc::UnboundedReceiver<Vec<u8>>) {
    let (sender, receiver) = mpsc::unbounded_channel();
    let stats = combat::player_base_stats(1, MirClass::Warrior);
    (
        Player {
            object_id,
            session_id: u64::from(object_id),
            character_id: i64::from(object_id),
            name: name.to_owned(),
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
            attack_mode: AttackMode::Peace,
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
            trade: TradeState::default(),
            trade_invitation: None,
            next_trade_time: 0,
            allow_trade: true,
            bind_map_index: 0,
            bind_point: mir_core::Point::default(),
            sender,
        },
        receiver,
    )
}

fn decode(frame: &[u8]) -> ServerPacket {
    let index = i16::from_le_bytes([frame[2], frame[3]]);
    ServerPacket::decode(index, &frame[4..]).expect("解码服务端包")
}

fn drain(receiver: &mut mpsc::UnboundedReceiver<Vec<u8>>) -> Vec<ServerPacket> {
    let mut packets = Vec::new();
    while let Ok(frame) = receiver.try_recv() {
        packets.push(decode(&frame));
    }
    packets
}

/// 两个玩家：甲在原点，乙在与怪物的对比位置。
fn two_players(
    map: &str,
    a: Point,
    b: Point,
) -> (
    World,
    mpsc::UnboundedReceiver<Vec<u8>>,
    mpsc::UnboundedReceiver<Vec<u8>>,
) {
    let mut world = World::new(999);
    let (first, first_rx) = player(1, "甲", map, a);
    world.enter(first);
    let (second, second_rx) = player(2, "乙", map, b);
    world.enter(second);
    (world, first_rx, second_rx)
}

#[test]
fn view_radius_is_crystal_data_range_sixteen() {
    let mut world = World::new(999);
    world.seed_rng(1);
    let (listener, mut listener_rx) = player(1, "甲", "0", Point::new(20, 20));
    world.enter(listener);
    drain(&mut listener_rx);

    // 距离 16 = `Globals.DataRange`：进入视野（`MapObject.cs:408-420`）。
    world.spawn_monster("0", MonsterKind::skeleton(), Point::new(36, 20));
    assert!(
        drain(&mut listener_rx)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::ObjectMonster(_))),
        "距离 16 的怪物应可见"
    );

    // 距离 17：超出视野，不发 `ObjectMonster`。
    world.spawn_monster("0", MonsterKind::skeleton(), Point::new(37, 21));
    assert!(
        !drain(&mut listener_rx)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::ObjectMonster(_))),
        "距离 17 的怪物不该可见"
    );
}

#[test]
fn broadcast_uses_grid_and_follows_placement() {
    let (mut world, mut first_rx, _second_rx) =
        two_players("0", Point::new(20, 20), Point::new(60, 60));
    drain(&mut first_rx);

    // 乙在 40 格外：聊天只有自己收到。
    assert_eq!(world.chat(1, "你好").expect("聊天"), 1);

    // 摆到 16 格处：进入广播半径。
    assert!(world.place_player(2, "0", Point::new(36, 20)));
    assert_eq!(world.chat(1, "你好").expect("聊天"), 2, "16 格内应收到");

    // 再摆到 17 格处：又收不到（网格随 `place_player` 更新，不能按旧坐标广播）。
    assert!(world.place_player(2, "0", Point::new(37, 21)));
    assert_eq!(world.chat(1, "你好").expect("聊天"), 1, "17 格外不该收到");
}

#[test]
fn player_enter_and_leave_view_is_diffed_both_ways() {
    // 甲 (20,20)、乙 (36,20)：距离 16，进场后互相可见。
    let (mut world, mut first_rx, mut second_rx) =
        two_players("0", Point::new(20, 20), Point::new(36, 20));
    drain(&mut first_rx);
    drain(&mut second_rx);

    // 甲向左走一格 → 距离 17：双方各收一条 `ObjectRemove`（Crystal `HumanObject.Remove`，
    // `HumanObject.cs:8440-8446`），且只发一次。
    world.walk(1, MirDirection::Left).expect("走一格");
    let removed_by_first = drain(&mut first_rx)
        .into_iter()
        .filter(
            |packet| matches!(packet, ServerPacket::ObjectRemove(remove) if remove.object_id == 2),
        )
        .count();
    let removed_by_second = drain(&mut second_rx)
        .into_iter()
        .filter(
            |packet| matches!(packet, ServerPacket::ObjectRemove(remove) if remove.object_id == 1),
        )
        .count();
    assert_eq!(removed_by_first, 1, "甲应恰好收到一条乙的 ObjectRemove");
    assert_eq!(removed_by_second, 1, "乙也应收到甲的 ObjectRemove");

    // 再走回原格 → 距离 16：双方各收一条 `ObjectPlayer`。
    world.advance_clock(700);
    world.walk(1, MirDirection::Right).expect("走回来");
    let added_by_first = drain(&mut first_rx)
        .into_iter()
        .filter(
            |packet| matches!(packet, ServerPacket::ObjectPlayer(object) if object.object_id == 2),
        )
        .count();
    let added_by_second = drain(&mut second_rx)
        .into_iter()
        .filter(
            |packet| matches!(packet, ServerPacket::ObjectPlayer(object) if object.object_id == 1),
        )
        .count();
    assert_eq!(added_by_first, 1, "甲应恰好收到一条乙的 ObjectPlayer");
    assert_eq!(added_by_second, 1, "乙应恰好收到一条甲的 ObjectPlayer");
}

#[test]
fn run_out_of_view_diffs_player_sets_both_ways() {
    // 甲 (20,20)、乙 (36,20)：距离 16，进场后互相可见。
    let (mut world, mut first_rx, mut second_rx) =
        two_players("0", Point::new(20, 20), Point::new(36, 20));
    drain(&mut first_rx);
    drain(&mut second_rx);

    // 先走一步积累 `StepCounter`（`CanRun` 要求刚走过一步），再向远处跑 2 格：21 → 19。
    world
        .walk(1, MirDirection::Right)
        .expect("walk one cell to arm the run counter");
    drain(&mut first_rx);
    drain(&mut second_rx);
    // 动作锁 600ms 已过，但要在 `cell_time + IDLE_RESET_MS`（500 + 700）前起跑，否则 `step_counter` 会被清零。
    world.advance_clock(900);
    assert_eq!(
        world.run(1, MirDirection::Left),
        Ok(Point::new(19, 20)),
        "real World::run must move two cells away from the observer"
    );

    let first_packets = drain(&mut first_rx);
    let second_packets = drain(&mut second_rx);
    let removed_by_first = first_packets
        .iter()
        .filter(
            |packet| matches!(packet, ServerPacket::ObjectRemove(remove) if remove.object_id == 2),
        )
        .count();
    let removed_by_second = second_packets
        .iter()
        .filter(
            |packet| matches!(packet, ServerPacket::ObjectRemove(remove) if remove.object_id == 1),
        )
        .count();
    assert_eq!(
        removed_by_first, 1,
        "running out of view must send the runner exactly one ObjectRemove for the observer"
    );
    assert_eq!(
        removed_by_second, 1,
        "running out of view must send the observer exactly one ObjectRemove for the runner"
    );
    let ghosts_by_first = first_packets
        .iter()
        .filter(
            |packet| matches!(packet, ServerPacket::ObjectPlayer(object) if object.object_id == 2),
        )
        .count();
    let ghosts_by_second = second_packets
        .iter()
        .filter(
            |packet| matches!(packet, ServerPacket::ObjectPlayer(object) if object.object_id == 1),
        )
        .count();
    assert_eq!(
        ghosts_by_first, 0,
        "a player leaving view must not be re-attached as a ghost ObjectPlayer to the runner"
    );
    assert_eq!(
        ghosts_by_second, 0,
        "a player leaving view must not be re-attached as a ghost ObjectPlayer to the observer"
    );

    // 距离 17：双方可见玩家集合都应与 AOI 一致（为空），且双向对称。
    assert_eq!(
        world.visible_players_of(1).cloned().unwrap_or_default(),
        BTreeSet::new(),
        "runner visible player set must match the AOI after running out of range"
    );
    assert_eq!(
        world.visible_players_of(2).cloned().unwrap_or_default(),
        BTreeSet::new(),
        "observer visible player set must match the AOI after the runner leaves range"
    );
}

#[test]
fn run_into_view_diffs_player_sets_both_ways() {
    // 甲 (18,20) 在乙 (36,20) 的 16 格视野外；先走到 19（仍 17 格），再向乙跑 2 格到 21（15 格内）。
    let (mut world, mut first_rx, mut second_rx) =
        two_players("0", Point::new(18, 20), Point::new(36, 20));
    drain(&mut first_rx);
    drain(&mut second_rx);
    assert_eq!(
        world.visible_players_of(1).cloned().unwrap_or_default(),
        BTreeSet::new(),
        "players 18 cells apart must not see each other before the run"
    );

    // 先走一步积累 `StepCounter`（`CanRun` 要求刚走过一步），再向观察者跑 2 格：19 → 21。
    world
        .walk(1, MirDirection::Right)
        .expect("walk one cell to arm the run counter");
    drain(&mut first_rx);
    drain(&mut second_rx);
    // 动作锁 600ms 已过，但要在 `cell_time + IDLE_RESET_MS`（500 + 700）前起跑，否则 `step_counter` 会被清零。
    world.advance_clock(900);
    assert_eq!(
        world.run(1, MirDirection::Right),
        Ok(Point::new(21, 20)),
        "real World::run must move two cells toward the observer"
    );

    let first_packets = drain(&mut first_rx);
    let second_packets = drain(&mut second_rx);
    let added_by_first = first_packets
        .iter()
        .filter(
            |packet| matches!(packet, ServerPacket::ObjectPlayer(object) if object.object_id == 2),
        )
        .count();
    let added_by_second = second_packets
        .iter()
        .filter(
            |packet| matches!(packet, ServerPacket::ObjectPlayer(object) if object.object_id == 1),
        )
        .count();
    assert_eq!(
        added_by_first, 1,
        "running into view must send the runner exactly one ObjectPlayer first frame for the observer"
    );
    assert_eq!(
        added_by_second, 1,
        "running into view must send the observer exactly one ObjectPlayer first frame for the runner"
    );
    let removals_by_first = first_packets
        .iter()
        .filter(
            |packet| matches!(packet, ServerPacket::ObjectRemove(remove) if remove.object_id == 2),
        )
        .count();
    let removals_by_second = second_packets
        .iter()
        .filter(
            |packet| matches!(packet, ServerPacket::ObjectRemove(remove) if remove.object_id == 1),
        )
        .count();
    assert_eq!(
        removals_by_first, 0,
        "entering view must not remove the observer from the runner"
    );
    assert_eq!(
        removals_by_second, 0,
        "entering view must not remove the runner from the observer"
    );

    // 距离 15：双方可见玩家集合都应与 AOI 一致，且双向对称。
    assert_eq!(
        world.visible_players_of(1).cloned().unwrap_or_default(),
        BTreeSet::from([2]),
        "runner visible player set must match the AOI after running into range"
    );
    assert_eq!(
        world.visible_players_of(2).cloned().unwrap_or_default(),
        BTreeSet::from([1]),
        "observer visible player set must match the AOI after the runner enters range"
    );
}

#[test]
fn melee_target_is_found_through_the_grid_after_placement() {
    let mut world = World::new(999);
    world.seed_rng(1);
    let (attacker, _rx) = player(1, "甲", "0", Point::new(20, 20));
    world.enter(attacker);
    let monster = world.spawn_monster("0", MonsterKind::skeleton(), Point::new(30, 20));

    if let Some(attacker) = world.player_mut(1) {
        attacker.stats.min_dc = 100;
        attacker.stats.max_dc = 100;
        attacker.stats.accuracy = 100;
    }
    // 用测试辅助摆到怪物左边一格：AOI 与业务字段一起更新，攻击才能在前方格找到目标。
    assert!(world.place_player(1, "0", Point::new(29, 20)));
    let report = world
        .attack(1, MirDirection::Right)
        .expect("攻击应被接受")
        .expect("前方格应有怪物");
    assert_eq!(report.target, monster);
    world.advance_clock(300);
    world.tick();
    assert!(
        world.monster(monster).is_none_or(|monster| monster.dead),
        "延迟伤害落地后怪物应已死亡"
    );
}

#[test]
fn monster_walk_updates_the_grid_and_player_view() {
    let mut world = World::new(999);
    world.seed_rng(7);
    let (listener, mut listener_rx) = player(1, "甲", "0", Point::new(30, 20));
    world.enter(listener);
    // 玩家在怪物索敌视野（7）内，怪物会朝他走；走位同时要维护 AOI 并广播 `ObjectWalk`。
    let monster = world.spawn_monster("0", MonsterKind::skeleton(), Point::new(34, 20));
    drain(&mut listener_rx);
    let before = world.monster(monster).expect("怪物还在").position;

    for _ in 0..8 {
        world.advance_clock(2_000);
        world.tick();
    }
    let after = world.monster(monster).expect("怪物还在").position;
    assert_ne!(before, after, "怪物应至少走动过一格");
    let walked = drain(&mut listener_rx)
        .into_iter()
        .filter(
            |packet| matches!(packet, ServerPacket::ObjectWalk(walk) if walk.object_id == monster),
        )
        .count();
    assert!(walked > 0, "怪物走动应经 AOI 广播给视野内玩家");
}

/// 合成一张 4×1 的 v0 地图，`(1, 0)` 带 1 号门（`Map.cs:142-143` 的门字节在 `o+8`）。
fn door_map() -> Map {
    const HEADER: usize = 52;
    const STRIDE: usize = 12;
    let width = 4usize;
    let height = 1usize;
    let mut data = vec![0_u8; HEADER + STRIDE * width * height];
    data[0..2].copy_from_slice(&(width as u16).to_le_bytes());
    data[2..4].copy_from_slice(&(height as u16).to_le_bytes());
    // 列优先：v0 第 (x, y) 格偏移 = HEADER + (x * Height + y) * 12（`Map.cs:123-151`）。
    // 这里取 x = 1、y = 0 → HEADER + height * 12。
    let cell = HEADER + height * STRIDE;
    data[cell + 8] = 0x81; // 门字节 > 0 → `AddDoor(0x81 & 0x7F = 1)`（`Map.cs:52`、`:142-143`）
    Map::from_bytes_named(&data, "door").expect("合成门地图应解析成功")
}

#[test]
fn door_state_machine_blocks_opens_and_auto_closes_with_wire_packets() {
    let mut world = World::new(999);
    world.insert_map("door", Arc::new(door_map()));
    let (first, mut first_rx) = player(1, "甲", "door", Point::new(0, 0));
    world.enter(first);
    let (second, mut second_rx) = player(2, "乙", "door", Point::new(3, 0));
    world.enter(second);
    drain(&mut first_rx);
    drain(&mut second_rx);

    // 1) 门关着：走上去被拦（`Map.CheckDoorOpen`，`Map.cs:646-651`；`HumanObject.cs:2420`）。
    assert_eq!(
        world.walk(1, MirDirection::Right),
        Err(MoveError::DoorClosed {
            x: 1,
            y: 0,
            index: 1
        })
    );

    // 2) 开门：本人与同图视野内玩家各收一条 `S.Opendoor{Close=false}`
    //    （`PlayerObject.Opendoor`，`PlayerObject.cs:7818-7826`）。
    assert!(world.open_door(1, 1), "1 号门应存在");
    assert!(world.door_is_open("door", 1));
    let first_events: Vec<bool> = drain(&mut first_rx)
        .into_iter()
        .filter_map(|packet| match packet {
            ServerPacket::Opendoor(door) => Some(door.close),
            _ => None,
        })
        .collect();
    let second_events: Vec<bool> = drain(&mut second_rx)
        .into_iter()
        .filter_map(|packet| match packet {
            ServerPacket::Opendoor(door) => Some(door.close),
            _ => None,
        })
        .collect();
    assert_eq!(first_events, vec![false], "开门者应收到 Opendoor(false)");
    assert_eq!(second_events, vec![false], "周围玩家应收到 Opendoor(false)");

    // 3) 门开了就能走过去。
    assert_eq!(
        world.walk(1, MirDirection::Right),
        Ok(Point::new(1, 0)),
        "门开着应能通过"
    );

    // 4) 5 秒后自动关并广播 `S.Opendoor{Close=true}`（`Map.cs:657-665`）。
    world.advance_clock(5_001);
    world.tick();
    assert!(!world.door_is_open("door", 1), "5 秒后门应自动关闭");
    let first_close = drain(&mut first_rx)
        .into_iter()
        .filter_map(|packet| match packet {
            ServerPacket::Opendoor(door) => Some((door.close, door.door_index)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let second_close = drain(&mut second_rx)
        .into_iter()
        .filter_map(|packet| match packet {
            ServerPacket::Opendoor(door) => Some((door.close, door.door_index)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(first_close, vec![(true, 1)], "关门广播应发给开门者");
    assert_eq!(second_close, vec![(true, 1)], "关门广播应发给周围玩家");

    // 5) 关回去以后又走不动了（把甲摆回门左边）。
    assert!(world.place_player(1, "door", Point::new(0, 0)));
    assert_eq!(
        world.walk(1, MirDirection::Right),
        Err(MoveError::DoorClosed {
            x: 1,
            y: 0,
            index: 1
        })
    );

    // 6) 不存在的地图/门：`open_door` 返回 false。
    assert!(!world.open_door(1, 99));
    assert!(
        world.door_is_open("door", 99),
        "未登记的门格按 Crystal 视为可通行"
    );
}

// =====================================================================
// R54-A: AOI five-route supplementary acceptance proof (appended tests).
// Existing cases above are intentionally untouched. New assertion and
// panic messages are English on purpose: the i18n ratchet scans test-only
// Chinese strings.
// =====================================================================

use mir_db::{MapInfo, MirDb, NpcInfo};
use mir_protocol::data::UserItem;
use mir_server::gamedata::GameData;

const AOI_MAP_A: &str = "0";
const AOI_MAP_B: &str = "1";

/// Decode one channel message that may carry several concatenated frames.
/// The production batching path (`flush_broadcast_batch`) writes exactly one
/// concatenated buffer per receiver per tick.
fn decode_batch(buf: &[u8]) -> Vec<ServerPacket> {
    let mut packets = Vec::new();
    let mut position = 0_usize;
    while position < buf.len() {
        assert!(
            position + 4 <= buf.len(),
            "frame header must fit in the batch buffer"
        );
        let total = u16::from_le_bytes([buf[position], buf[position + 1]]) as usize;
        assert!(total >= 4, "frame length must cover the 4-byte header");
        assert!(
            position + total <= buf.len(),
            "frame body must fit in the batch buffer"
        );
        let index = i16::from_le_bytes([buf[position + 2], buf[position + 3]]);
        packets.push(
            ServerPacket::decode(index, &buf[position + 4..position + total])
                .expect("decode a batched server packet"),
        );
        position += total;
    }
    assert_eq!(
        position,
        buf.len(),
        "the batch buffer must be consumed completely"
    );
    packets
}

fn drain_batches(receiver: &mut mpsc::UnboundedReceiver<Vec<u8>>) -> Vec<Vec<ServerPacket>> {
    let mut batches = Vec::new();
    while let Ok(frame) = receiver.try_recv() {
        batches.push(decode_batch(&frame));
    }
    batches
}

fn object_remove_ids(packets: &[ServerPacket]) -> Vec<u32> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::ObjectRemove(remove) => Some(remove.object_id),
            _ => None,
        })
        .collect()
}

/// Synthesize a width x height v0 map (52-byte header + 12 bytes per cell,
/// column-major cell order, same layout as `Map.cs`).
fn synthetic_v0_map(name: &str, width: usize, height: usize) -> Map {
    const HEADER: usize = 52;
    const STRIDE: usize = 12;
    let mut data = vec![0_u8; HEADER + STRIDE * width * height];
    data[0..2].copy_from_slice(&(width as u16).to_le_bytes());
    data[2..4].copy_from_slice(&(height as u16).to_le_bytes());
    Map::from_bytes_named(&data, name).expect("synthetic v0 map must parse")
}

/// In-memory production `GameData` for the enter/change-map routes: two maps and
/// three NPCs. No resource files are needed; `World::enter` drives the real
/// `initialize_map -> spawn_npcs` path with this data.
fn aoi_production_data() -> Arc<GameData> {
    let db = MirDb {
        maps: vec![
            MapInfo {
                index: 1,
                file_name: AOI_MAP_A.to_owned(),
                title: "aoi-route-map-a".to_owned(),
                ..MapInfo::default()
            },
            MapInfo {
                index: 2,
                file_name: AOI_MAP_B.to_owned(),
                title: "aoi-route-map-b".to_owned(),
                ..MapInfo::default()
            },
        ],
        npcs: vec![
            NpcInfo {
                index: 10,
                map_index: 1,
                name: "aoi-route-npc-0".to_owned(),
                file_name: "aoi-route-0.txt".to_owned(),
                location: Point::new(14, 10),
                ..NpcInfo::default()
            },
            NpcInfo {
                index: 11,
                map_index: 1,
                name: "aoi-route-npc-far".to_owned(),
                file_name: "aoi-route-1.txt".to_owned(),
                location: Point::new(28, 28),
                ..NpcInfo::default()
            },
            NpcInfo {
                index: 20,
                map_index: 2,
                name: "aoi-route-npc-b".to_owned(),
                file_name: "aoi-route-2.txt".to_owned(),
                location: Point::new(14, 10),
                ..NpcInfo::default()
            },
        ],
        ..MirDb::default()
    };
    Arc::new(GameData::from_db(db, None))
}

/// Flatten one first-frame packet into `(kind-order, object-id)` so the whole
/// ordered frame vector can be compared in one assertion.
fn first_frame_identity(packet: &ServerPacket) -> (u8, u32) {
    match packet {
        ServerPacket::ObjectPlayer(object) => (0, object.object_id),
        ServerPacket::ObjectMonster(object) => (1, object.object_id),
        ServerPacket::ObjectNPC(object) => (2, object.object_id),
        ServerPacket::ObjectItem(object) => (3, object.object_id),
        ServerPacket::ObjectGold(object) => (4, object.object_id),
        other => panic!("unexpected enter first-frame packet: {other:?}"),
    }
}

#[test]
fn aoi_acceptance_enter_snapshot() {
    let mut world = World::new(999);
    world.seed_rng(1);
    world.set_game_data(aoi_production_data());
    world.insert_map(AOI_MAP_A, Arc::new(synthetic_v0_map(AOI_MAP_A, 30, 30)));

    // The observer enters first: this runs the production
    // `initialize_map -> spawn_npcs` path, so the NPC first frame proven below
    // is produced by production code (never by `World::spawn_npc`).
    let (observer, mut observer_rx) = player(101, "observer", AOI_MAP_A, Point::new(12, 10));
    world.enter(observer);
    let spawned_npcs: Vec<(u32, String, Point)> = world
        .npcs_on(AOI_MAP_A)
        .into_iter()
        .map(|npc| (npc.object_id, npc.name.clone(), npc.position))
        .collect();
    assert_eq!(
        spawned_npcs,
        vec![
            (1, "aoi-route-npc-0".to_owned(), Point::new(14, 10)),
            (2, "aoi-route-npc-far".to_owned(), Point::new(28, 28)),
        ],
        "the real enter path must spawn NPCs through production spawn_npcs"
    );
    drain(&mut observer_rx);

    // Four kinds of objects around the main player's landing point.
    let near_monster_a =
        world.spawn_monster(AOI_MAP_A, MonsterKind::skeleton(), Point::new(13, 10));
    let near_monster_b = world.spawn_monster(AOI_MAP_A, MonsterKind::skeleton(), Point::new(9, 12));
    let far_monster = world.spawn_monster(AOI_MAP_A, MonsterKind::skeleton(), Point::new(28, 5));
    assert_eq!(
        vec![near_monster_a, near_monster_b, far_monster],
        vec![102, 103, 104],
        "monster object ids must be allocated in spawn order"
    );
    let item_user = UserItem {
        item_index: 100,
        ..UserItem::default()
    };
    let near_item_a = world.spawn_ground_item(
        AOI_MAP_A,
        Point::new(11, 10),
        Some(item_user.clone()),
        0,
        None,
    );
    let near_item_b =
        world.spawn_ground_item(AOI_MAP_A, Point::new(8, 11), Some(item_user), 0, None);
    let far_item = world.spawn_ground_item(AOI_MAP_A, Point::new(27, 8), None, 500, None);
    assert_eq!(
        vec![near_item_a, near_item_b, far_item],
        vec![105, 106, 107],
        "ground item object ids must be allocated in spawn order"
    );
    let (far_player, mut far_player_rx) = player(200, "far-player", AOI_MAP_A, Point::new(40, 40));
    world.enter(far_player);
    drain(&mut far_player_rx);

    // Main player enters for real; only its own channel is inspected here.
    let (main_player, mut main_rx) = player(100, "main-player", AOI_MAP_A, Point::new(10, 10));
    world.enter(main_player);
    let main_packets = drain(&mut main_rx);
    let frames: Vec<(u8, u32)> = main_packets.iter().map(first_frame_identity).collect();
    assert_eq!(
        frames,
        vec![
            (0, 101), // ObjectPlayer: only the in-range observer
            (1, 102), // ObjectMonster: ordered by (y, x, id)
            (1, 103),
            (2, 1),   // ObjectNPC: the production-spawned NPC
            (3, 105), // ObjectItem: ordered by (y, x, id)
            (3, 106),
        ],
        "enter first frames must be grouped by kind: players -> monsters -> NPCs -> items, each group in AOI order"
    );
    assert!(
        !frames.iter().any(|(_, object_id)| *object_id == 100),
        "a player must never receive its own first frame"
    );
    let unique_ids: BTreeSet<u32> = frames.iter().map(|(_, object_id)| *object_id).collect();
    assert_eq!(
        unique_ids.len(),
        frames.len(),
        "the enter snapshot must not contain duplicate object ids"
    );
    assert_eq!(
        unique_ids,
        BTreeSet::from([1, 101, 102, 103, 105, 106]),
        "the enter snapshot must expose exactly the in-range objects of all four kinds"
    );
    for excluded in [104_u32, 107, 200] {
        assert!(
            !frames.iter().any(|(_, object_id)| *object_id == excluded),
            "out-of-range object {excluded} must not appear in the enter snapshot"
        );
    }

    // The public view sets written by production enter must match the frames.
    let main = world
        .player(100)
        .expect("the main player must be in the world");
    assert_eq!(main.visible_monsters, BTreeSet::from([102, 103]));
    assert_eq!(main.visible_npcs, BTreeSet::from([1]));
    assert_eq!(main.visible_items, BTreeSet::from([105, 106]));
    assert_eq!(
        world.visible_players_of(100).cloned().unwrap_or_default(),
        BTreeSet::from([101]),
        "enter must record exactly the in-range players in visible_players"
    );
    assert_eq!(
        world.visible_players_of(101).cloned().unwrap_or_default(),
        BTreeSet::from([100]),
        "the observer must see the entering player (bidirectional first frame)"
    );
    for visible in [
        &main.visible_monsters,
        &main.visible_npcs,
        &main.visible_items,
    ] {
        assert!(
            !visible.contains(&100),
            "the entering player must exclude itself from every visible set"
        );
    }
}

#[test]
fn aoi_acceptance_leave_cleanup() {
    let mut world = World::new(999);
    world.seed_rng(3);
    let (observer, mut observer_rx) = player(100, "observer", AOI_MAP_A, Point::new(20, 20));
    world.enter(observer);
    let (leaver, mut leaver_rx) = player(200, "leaver", AOI_MAP_A, Point::new(24, 20));
    world.enter(leaver);
    let (far, mut far_rx) = player(300, "far", AOI_MAP_A, Point::new(60, 60));
    world.enter(far);
    drain(&mut observer_rx);
    drain(&mut leaver_rx);
    drain(&mut far_rx);

    assert_eq!(
        world.visible_players_of(100).cloned().unwrap_or_default(),
        BTreeSet::from([200]),
        "the observer must see the leaver before leave"
    );
    assert_eq!(
        world.visible_players_of(200).cloned().unwrap_or_default(),
        BTreeSet::from([100]),
        "the leaver must see the observer before leave"
    );
    assert_eq!(
        world.visible_players_of(300).cloned().unwrap_or_default(),
        BTreeSet::new(),
        "the far player must see nobody"
    );

    let removed = world
        .leave(200)
        .expect("the first leave must return the player");
    assert_eq!(removed.object_id, 200);
    assert_eq!(removed.map, AOI_MAP_A);
    assert_eq!(removed.position, Point::new(24, 20));

    let observer_packets = drain(&mut observer_rx);
    assert_eq!(
        object_remove_ids(&observer_packets),
        vec![200],
        "an in-range observer must receive exactly one ObjectRemove for the leaver"
    );
    assert_eq!(
        observer_packets.len(),
        1,
        "leave must not send any other packet to in-range observers"
    );
    assert!(
        drain(&mut leaver_rx).is_empty(),
        "the leaver must not receive packets for its own leave"
    );
    assert!(
        drain(&mut far_rx).is_empty(),
        "out-of-range players must not receive the leaver removal"
    );

    assert_eq!(
        world.visible_players_of(100).cloned().unwrap_or_default(),
        BTreeSet::new(),
        "leave must remove the leaver from the observer's visible_players"
    );
    assert_eq!(world.visible_players_of(200), None);
    assert_eq!(
        world.visible_players_of(300).cloned().unwrap_or_default(),
        BTreeSet::new()
    );

    assert!(
        world.leave(200).is_none(),
        "a second leave must return None"
    );
    assert!(
        drain(&mut observer_rx).is_empty(),
        "a second leave must not send a duplicate removal"
    );

    // Later broadcasts and moves must not reach the removed player.
    let receivers = world
        .chat(100, "after-leave")
        .expect("chat after leave must still be accepted");
    assert_eq!(
        receivers, 1,
        "chat after leave must only be delivered to its author"
    );
    let chat_packets = drain(&mut observer_rx);
    assert_eq!(chat_packets.len(), 1);
    assert!(
        matches!(&chat_packets[0], ServerPacket::ObjectChat(chat) if chat.object_id == 100 && chat.text == "after-leave"),
        "the in-range observer must still receive later chat: {chat_packets:?}"
    );
    assert!(
        drain(&mut leaver_rx).is_empty(),
        "a later chat must not reach the removed player"
    );
    assert_eq!(
        world.walk(200, MirDirection::Right),
        Err(MoveError::NotInWorld(200)),
        "a removed player must not be able to move"
    );

    let monster = world.spawn_monster(AOI_MAP_A, MonsterKind::scarecrow(), Point::new(24, 20));
    let spawn_packets = drain(&mut observer_rx);
    assert!(
        spawn_packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::ObjectMonster(object) if object.object_id == monster)),
        "the still-present observer must see a later monster spawn"
    );
    assert!(
        drain(&mut leaver_rx).is_empty(),
        "a later broadcast must not reach the removed player"
    );
}

#[test]
fn aoi_acceptance_change_map_sets() {
    let mut world = World::new(999);
    world.seed_rng(5);
    world.set_game_data(aoi_production_data());
    world.insert_map(AOI_MAP_A, Arc::new(synthetic_v0_map(AOI_MAP_A, 30, 30)));
    world.insert_map(AOI_MAP_B, Arc::new(synthetic_v0_map(AOI_MAP_B, 30, 30)));

    let (old_watcher, mut old_watcher_rx) = player(500, "old-watcher", AOI_MAP_A, Point::new(8, 5));
    world.enter(old_watcher);
    assert_eq!(
        world
            .npcs_on(AOI_MAP_A)
            .iter()
            .map(|npc| npc.object_id)
            .collect::<Vec<_>>(),
        vec![1, 2],
        "production spawn_npcs must fill the old map first"
    );
    let (mover, mut mover_rx) = player(100, "mover", AOI_MAP_A, Point::new(5, 5));
    world.enter(mover);
    let (far_old, mut far_old_rx) = player(600, "far-old", AOI_MAP_A, Point::new(28, 27));
    world.enter(far_old);
    let (new_watcher, mut new_watcher_rx) = player(700, "new-watcher", AOI_MAP_B, Point::new(8, 5));
    world.enter(new_watcher);
    assert_eq!(
        world
            .npcs_on(AOI_MAP_B)
            .iter()
            .map(|npc| npc.object_id)
            .collect::<Vec<_>>(),
        vec![601],
        "production spawn_npcs must fill the new map when it initializes"
    );
    let (far_new, mut far_new_rx) = player(800, "far-new", AOI_MAP_B, Point::new(28, 27));
    world.enter(far_new);
    for receiver in [
        &mut old_watcher_rx,
        &mut mover_rx,
        &mut far_old_rx,
        &mut new_watcher_rx,
        &mut far_new_rx,
    ] {
        drain(receiver);
    }

    assert_eq!(
        world.visible_players_of(100).cloned().unwrap_or_default(),
        BTreeSet::from([500]),
        "the mover must see exactly the old-map watcher before the change"
    );
    assert_eq!(
        world.player(100).expect("mover").visible_npcs,
        BTreeSet::from([1]),
        "the mover must see the old-map NPC before the change"
    );

    let report = world
        .change_map(100, AOI_MAP_B, Point::new(5, 5), MirDirection::Right)
        .expect("the real change_map must succeed");
    assert_eq!(report.from_map, AOI_MAP_A);
    assert_eq!(report.to_map, AOI_MAP_B);
    assert_eq!(report.position, Point::new(5, 5));

    // Mover channel: MapChanged first, then the new map's first frames.
    let mover_packets = drain(&mut mover_rx);
    assert_eq!(
        mover_packets.len(),
        3,
        "the mover must receive MapChanged plus the new-map first frames: {mover_packets:?}"
    );
    assert!(
        matches!(&mover_packets[0], ServerPacket::MapChanged(changed) if changed.file_name == AOI_MAP_B),
        "MapChanged must be the first packet on the mover channel: {mover_packets:?}"
    );
    assert!(
        matches!(&mover_packets[1], ServerPacket::ObjectPlayer(object) if object.object_id == 700),
        "the mover's player first frame must only contain the new-map watcher: {mover_packets:?}"
    );
    assert!(
        matches!(&mover_packets[2], ServerPacket::ObjectNPC(object) if object.object_id == 601),
        "the mover's NPC first frame must only contain the new-map NPC: {mover_packets:?}"
    );
    assert!(
        object_remove_ids(&mover_packets).is_empty(),
        "the mover must not receive ObjectRemove for its own change: {mover_packets:?}"
    );

    // Old-map observer: exactly one removal; new-map observer: exactly one attach.
    let old_packets = drain(&mut old_watcher_rx);
    assert_eq!(
        object_remove_ids(&old_packets),
        vec![100],
        "the old-map watcher must receive exactly one ObjectRemove for the mover"
    );
    assert_eq!(
        old_packets.len(),
        1,
        "the old-map watcher must receive no other packet"
    );
    let new_packets = drain(&mut new_watcher_rx);
    assert!(
        matches!(&new_packets[..], [ServerPacket::ObjectPlayer(object)] if object.object_id == 100),
        "the new-map watcher must receive exactly one attach frame for the mover: {new_packets:?}"
    );
    assert!(
        drain(&mut far_old_rx).is_empty(),
        "out-of-range old-map players must not receive a removal"
    );
    assert!(
        drain(&mut far_new_rx).is_empty(),
        "out-of-range new-map players must not receive an attach"
    );

    assert_eq!(
        world.visible_players_of(100).cloned().unwrap_or_default(),
        BTreeSet::from([700]),
        "the mover's visible_players must become exactly the new-map watcher"
    );
    assert_eq!(
        world.visible_players_of(500).cloned().unwrap_or_default(),
        BTreeSet::new(),
        "the old-map watcher must drop the mover"
    );
    assert_eq!(
        world.visible_players_of(600).cloned().unwrap_or_default(),
        BTreeSet::new()
    );
    assert_eq!(
        world.visible_players_of(700).cloned().unwrap_or_default(),
        BTreeSet::from([100]),
        "the new-map watcher must attach the mover"
    );
    assert_eq!(
        world.visible_players_of(800).cloned().unwrap_or_default(),
        BTreeSet::new()
    );

    let mover_view = world.player(100).expect("mover after change");
    assert_eq!(
        mover_view.visible_npcs,
        BTreeSet::from([601]),
        "the old-map NPC must be cleared and only the new-map NPC must remain visible"
    );
    assert!(
        !mover_view.visible_npcs.contains(&1),
        "the old-map NPC must not leak into the new visible set"
    );
    assert!(mover_view.visible_monsters.is_empty());
    assert!(mover_view.visible_items.is_empty());

    // Old-map broadcast negative: a chat on map A must not reach the mover.
    let old_receivers = world
        .chat(500, "old-map-ping")
        .expect("old-map chat must be accepted");
    assert_eq!(
        old_receivers, 1,
        "the old-map chat must only reach its author"
    );
    assert!(
        drain(&mut mover_rx).is_empty(),
        "a broadcast on the old map must not reach the moved player"
    );

    // New-map broadcast positive: the moved player is reachable on map B.
    let new_receivers = world
        .chat(700, "new-map-ping")
        .expect("new-map chat must be accepted");
    assert_eq!(new_receivers, 2);
    let mover_after_chat = drain(&mut mover_rx);
    assert!(
        matches!(&mover_after_chat[..], [ServerPacket::ObjectChat(chat)] if chat.object_id == 700 && chat.text == "new-map-ping"),
        "a broadcast on the new map must reach the moved player: {mover_after_chat:?}"
    );
}

#[test]
fn aoi_acceptance_recycle_all_kinds() {
    let mut world = World::new(999);
    world.seed_rng(9);
    world.set_item_expire_ms(1_000);
    let (player_object, mut player_rx) = player(100, "recycler", AOI_MAP_A, Point::new(20, 20));
    world.enter(player_object);
    drain(&mut player_rx);

    // [M1→A] Freeze the server clock before scheduling either ground item.
    //
    // `spawn_ground_item` stamps `expire_time = now() + item_expire_ms` from the
    // live wall clock on every call (`crates/mir-server/src/items.rs:1536/1544`).
    // When the two spawns straddle a real millisecond they get ExpireTime one
    // apart. The boundary walk below is derived from the first item only, so the
    // coin pile is still 1ms short after `advance_clock(1)` and the strict
    // `now > expire_time` recycle (`crates/mir-server/src/items.rs:1629`) never
    // removes it -> the test goes red at "the expired coin pile must be removed".
    // Freezing here makes the two spawns share one `now()`.
    //
    // Self-proof that this cannot silently regress: without this call the
    // explicit equality assertion below fails as soon as the two spawns observe
    // different wall-clock milliseconds. Reproduce deterministically by
    // commenting this line out and inserting a 2ms sleep between the two spawns
    // -> the run must be red at that equality assertion.
    world.advance_clock(0);

    // Normal item (Some(UserItem)) must be the item proof; the coin pile is a
    // separate ObjectGold subitem and must never stand in for it.
    let normal_item = world.spawn_ground_item(
        AOI_MAP_A,
        Point::new(22, 20),
        Some(UserItem {
            item_index: 100,
            ..UserItem::default()
        }),
        0,
        None,
    );
    let coin_pile = world.spawn_ground_item(AOI_MAP_A, Point::new(23, 20), None, 500, None);
    // [M1→A] Both ground items must carry the same ExpireTime: that is what the
    // clock freeze above buys, and it is what keeps the strict boundary walk
    // deterministic. Deleting the freeze must fail this assertion.
    let normal_expire_time = world
        .ground_item(normal_item)
        .expect("the normal item must exist")
        .expire_time;
    let coin_expire_time = world
        .ground_item(coin_pile)
        .expect("the coin pile must exist")
        .expire_time;
    assert!(
        normal_expire_time == coin_expire_time,
        "both ground items must share the same expire_time when the clock is frozen before spawning"
    );
    {
        let item_ground = world
            .ground_item(normal_item)
            .expect("the normal item must exist");
        assert!(
            item_ground.item.is_some(),
            "the normal-item proof requires GroundItem.item == Some(UserItem)"
        );
        assert!(!item_ground.is_gold(), "the normal item is not a coin pile");
        let gold_ground = world
            .ground_item(coin_pile)
            .expect("the coin pile must exist");
        assert!(
            gold_ground.item.is_none(),
            "the coin pile must not carry a UserItem"
        );
        assert!(
            gold_ground.is_gold(),
            "the coin pile must be a gold ground item"
        );
    }
    let spawn_packets = drain(&mut player_rx);
    assert_eq!(
        spawn_packets
            .iter()
            .filter(|packet| matches!(packet, ServerPacket::ObjectItem(object) if object.object_id == normal_item))
            .count(),
        1,
        "the normal item must be announced with one ObjectItem frame"
    );
    assert_eq!(
        spawn_packets
            .iter()
            .filter(|packet| matches!(packet, ServerPacket::ObjectGold(object) if object.object_id == coin_pile))
            .count(),
        1,
        "the coin pile must be announced with one ObjectGold frame"
    );
    assert_eq!(
        spawn_packets
            .iter()
            .filter(|packet| matches!(packet, ServerPacket::ObjectItem(object) if object.object_id == coin_pile))
            .count(),
        0,
        "coins must not be announced as ObjectItem"
    );

    // Strict `now > expire_time`: at the exact boundary both piles stay.
    let item_expire = world.ground_item(normal_item).expect("item").expire_time;
    let boundary_delta = item_expire - world.now();
    assert!(
        boundary_delta > 0 && boundary_delta <= 1_000,
        "the spawn must schedule expiry at now + the configured timeout"
    );
    world.advance_clock(boundary_delta);
    world.tick();
    assert!(
        world.ground_item(normal_item).is_some(),
        "a normal item must stay at now == ExpireTime (strict Time > ExpireTime)"
    );
    assert!(
        world.ground_item(coin_pile).is_some(),
        "a coin pile must stay at now == ExpireTime (strict Time > ExpireTime)"
    );
    assert!(
        object_remove_ids(&drain(&mut player_rx)).is_empty(),
        "no removal may be sent at the exact expiry boundary"
    );

    // One millisecond later both are due: exactly one removal each.
    world.advance_clock(1);
    world.tick();
    assert!(
        world.ground_item(normal_item).is_none(),
        "the expired item must be removed by the World tick"
    );
    assert!(
        world.ground_item(coin_pile).is_none(),
        "the expired coin pile must be removed by the World tick"
    );
    let expiry_removals = object_remove_ids(&drain(&mut player_rx));
    assert_eq!(
        expiry_removals,
        vec![normal_item, coin_pile],
        "the expiry tick must broadcast exactly one ObjectRemove per expired ground object"
    );
    let mut sorted_expiry = expiry_removals.clone();
    sorted_expiry.sort_unstable();
    assert_eq!(sorted_expiry, {
        let mut expected = vec![normal_item, coin_pile];
        expected.sort_unstable();
        expected
    });
    world.tick();
    assert!(
        object_remove_ids(&drain(&mut player_rx)).is_empty(),
        "a second tick must not emit duplicate ground-object removals"
    );

    // Real monster death -> corpse tick recycle.
    let monster = world.spawn_monster(AOI_MAP_A, MonsterKind::scarecrow(), Point::new(25, 20));
    drain(&mut player_rx);
    {
        let attacker = world.player_mut(100).expect("attacker");
        attacker.stats.min_dc = 100;
        attacker.stats.max_dc = 100;
        attacker.stats.accuracy = 100;
    }
    assert!(world.place_player(100, AOI_MAP_A, Point::new(24, 20)));
    drain(&mut player_rx);
    let report = world
        .attack(100, MirDirection::Right)
        .expect("the attack must be accepted")
        .expect("the corpse target must be adjacent");
    assert_eq!(report.target, monster);
    world.advance_clock(300);
    world.tick();
    let dies = drain(&mut player_rx);
    assert!(
        dies.iter()
            .any(|packet| matches!(packet, ServerPacket::ObjectDied(_))
                || matches!(packet, ServerPacket::GainExperience(_))),
        "the real attack must announce the kill: {dies:?}"
    );
    let dead_time = world
        .monster(monster)
        .expect("the dead monster must remain as a corpse")
        .dead_time;
    let corpse_delta = dead_time - world.now();
    assert!(corpse_delta > 0, "the corpse delay must be in the future");
    world.advance_clock(corpse_delta - 1);
    world.tick();
    assert!(
        world.monster(monster).is_some(),
        "the corpse must stay before now >= dead_time"
    );
    assert!(
        !object_remove_ids(&drain(&mut player_rx)).contains(&monster),
        "no corpse removal may be sent before the corpse delay elapses"
    );
    world.advance_clock(1);
    world.tick();
    assert!(
        world.monster(monster).is_none(),
        "the corpse tick must recycle the dead monster"
    );
    assert_eq!(
        object_remove_ids(&drain(&mut player_rx)),
        vec![monster],
        "corpse recycling must send exactly one ObjectRemove for the monster"
    );
    assert!(
        !world
            .player(100)
            .expect("recycler")
            .visible_monsters
            .contains(&monster),
        "the recycled corpse must be removed from the player view set"
    );
    world.tick();
    assert!(
        object_remove_ids(&drain(&mut player_rx)).is_empty(),
        "a second tick must not emit duplicate corpse removals"
    );
}

/// [M2→A] Deterministic millisecond-boundary regression for ground-item expiry.
///
/// `spawn_ground_item` stamps `expire_time = now() + item_expire_ms`
/// (`crates/mir-server/src/items.rs:1536/1544`) and the recycle predicate is the
/// strict `now > expire_time` (`crates/mir-server/src/items.rs:1629`). This test
/// freezes the clock, spawns one item, advances exactly 1ms, then spawns a second
/// item, so their `ExpireTime` differs by exactly 1 regardless of machine speed or
/// scheduler jitter. The test then walks three ticks: both items survive their
/// exact boundary, the first item is removed one millisecond past its boundary
/// while the second survives at its own boundary, and the second is removed one
/// millisecond past its boundary.
#[test]
fn ground_item_expiry_strict_ms_boundary() {
    let mut world = World::new(999);
    world.set_item_expire_ms(1_000);
    let (viewer, mut viewer_rx) = player(700, "expiry-viewer", AOI_MAP_A, Point::new(20, 20));
    world.enter(viewer);
    drain(&mut viewer_rx);

    // Freeze the clock first: from here on `now()` only moves when the test says so.
    world.advance_clock(0);
    let item_a = world.spawn_ground_item(
        AOI_MAP_A,
        Point::new(22, 20),
        Some(UserItem {
            item_index: 100,
            ..UserItem::default()
        }),
        0,
        None,
    );
    // Exactly one millisecond later the second spawn must be stamped one later.
    world.advance_clock(1);
    let item_b = world.spawn_ground_item(
        AOI_MAP_A,
        Point::new(23, 20),
        Some(UserItem {
            item_index: 101,
            ..UserItem::default()
        }),
        0,
        None,
    );
    drain(&mut viewer_rx);

    let expire_a = world.ground_item(item_a).expect("item a").expire_time;
    let expire_b = world.ground_item(item_b).expect("item b").expire_time;
    assert!(
        expire_b - expire_a == 1,
        "the two spawns must be exactly 1ms apart in ExpireTime"
    );

    // Moment 1: now == expire_a. Strict `now > expire_time` keeps both.
    world.advance_clock(expire_a - world.now());
    world.tick();
    assert!(
        world.ground_item(item_a).is_some(),
        "item a must stay at now == its ExpireTime (strict Time > ExpireTime)"
    );
    assert!(
        world.ground_item(item_b).is_some(),
        "item b must stay before its ExpireTime"
    );
    assert!(
        object_remove_ids(&drain(&mut viewer_rx)).is_empty(),
        "no ObjectRemove may be sent at the exact ExpireTime boundary"
    );

    // Moment 2: now == expire_a + 1 == expire_b. Only item a is due.
    world.advance_clock(1);
    world.tick();
    assert!(
        world.ground_item(item_a).is_none(),
        "item a must be removed one millisecond past its ExpireTime"
    );
    assert!(
        world.ground_item(item_b).is_some(),
        "item b must stay at now == its ExpireTime (strict Time > ExpireTime)"
    );
    assert_eq!(
        object_remove_ids(&drain(&mut viewer_rx)),
        vec![item_a],
        "the first expiry tick must broadcast exactly one ObjectRemove for item a"
    );

    // Moment 3: now == expire_b + 1. Only item b is due.
    world.advance_clock(1);
    world.tick();
    assert!(
        world.ground_item(item_b).is_none(),
        "item b must be removed one millisecond past its ExpireTime"
    );
    assert_eq!(
        object_remove_ids(&drain(&mut viewer_rx)),
        vec![item_b],
        "the second expiry tick must broadcast exactly one ObjectRemove for item b"
    );

    // A follow-up tick must not replay removals.
    world.tick();
    assert!(
        object_remove_ids(&drain(&mut viewer_rx)).is_empty(),
        "a tick after both removals must not emit duplicate ObjectRemove frames"
    );
}

#[test]
fn aoi_acceptance_broadcast_identity_order() {
    let mut world = World::new(999);
    world.seed_rng(11);
    let (actor, mut actor_rx) = player(500, "actor", AOI_MAP_A, Point::new(20, 20));
    world.enter(actor);
    let (near, mut near_rx) = player(501, "near", AOI_MAP_A, Point::new(36, 20));
    world.enter(near);
    let (far, mut far_rx) = player(502, "far", AOI_MAP_A, Point::new(37, 21));
    world.enter(far);
    let (cross_map, mut cross_map_rx) = player(503, "cross-map", AOI_MAP_B, Point::new(20, 20));
    world.enter(cross_map);
    for receiver in [&mut actor_rx, &mut near_rx, &mut far_rx, &mut cross_map_rx] {
        drain(receiver);
    }

    // Identity sets: same map near vs far, and cross-map with identical coords.
    assert_eq!(
        world.visible_players_of(500).cloned().unwrap_or_default(),
        BTreeSet::from([501]),
        "the actor must see exactly the near player (distance 16), not the far one (17)"
    );
    assert_eq!(
        world.visible_players_of(502).cloned().unwrap_or_default(),
        BTreeSet::from([501]),
        "the far player must only see the near player"
    );
    assert_eq!(
        world.visible_players_of(503).cloned().unwrap_or_default(),
        BTreeSet::new(),
        "a cross-map player at identical coordinates must not see map-A players"
    );

    // Default production path: direct send (batching is off by default).
    assert_eq!(
        world
            .chat(500, "hello")
            .expect("direct chat must be accepted"),
        2,
        "chat must reach the author and exactly the in-range receiver"
    );
    let actor_direct = drain(&mut actor_rx);
    let near_direct = drain(&mut near_rx);
    assert!(
        matches!(&actor_direct[..], [ServerPacket::ObjectChat(chat)] if chat.object_id == 500 && chat.text == "hello"),
        "the author must receive its own chat once, directly: {actor_direct:?}"
    );
    assert!(
        matches!(&near_direct[..], [ServerPacket::ObjectChat(chat)] if chat.object_id == 500 && chat.text == "hello"),
        "the in-range receiver must receive exactly the actor's chat: {near_direct:?}"
    );
    assert!(
        drain(&mut far_rx).is_empty(),
        "the far player must not receive the chat"
    );
    assert!(
        drain(&mut cross_map_rx).is_empty(),
        "the cross-map player must not receive the chat"
    );

    // Production batching path: broadcast is buffered per receiver and flushed
    // by the real World tick as one decoded sequence per receiver.
    world.set_broadcast_batching(true);
    drain(&mut actor_rx);
    assert_eq!(
        world
            .chat(500, "batched")
            .expect("batched chat must be accepted"),
        2,
        "batched chat must still count the author and the in-range receiver"
    );
    let actor_batched = drain_batches(&mut actor_rx);
    assert_eq!(
        actor_batched.len(),
        1,
        "the author's own chat is sent directly, not batched"
    );
    assert!(
        matches!(&actor_batched[0][..], [ServerPacket::ObjectChat(chat)] if chat.object_id == 500 && chat.text == "batched"),
        "the author's direct chat frame must be intact"
    );
    assert!(
        drain(&mut near_rx).is_empty(),
        "with batching on, the receiver must get nothing before the next tick flush"
    );
    world.tick();
    let near_batched = drain_batches(&mut near_rx);
    assert_eq!(
        near_batched.len(),
        1,
        "the real tick batch flush must write one concatenated buffer per receiver"
    );
    assert!(
        matches!(&near_batched[0][..], [ServerPacket::ObjectChat(chat)] if chat.object_id == 500 && chat.text == "batched"),
        "the flushed receiver sequence must decode to exactly the chat frame: {near_batched:?}"
    );
    assert!(
        drain(&mut far_rx).is_empty(),
        "the far player must not receive batched chat either"
    );
    assert!(
        drain(&mut cross_map_rx).is_empty(),
        "the cross-map player must not receive batched chat"
    );
    assert_eq!(
        world.visible_players_of(500).cloned().unwrap_or_default(),
        BTreeSet::from([501]),
        "chat/batching must not change the actor visible_players set"
    );

    // Production actor/exclude identity path: OpenDoor sends one frame to the
    // actor directly and broadcasts to the same map with exclude = actor and
    // actor = actor. The cross-map player at identical coordinates gets nothing.
    world.set_broadcast_batching(false);
    drain(&mut actor_rx);
    drain(&mut near_rx);
    world.insert_map("door", Arc::new(door_map()));
    let (door_actor, mut door_actor_rx) = player(801, "door-actor", "door", Point::new(0, 0));
    world.enter(door_actor);
    let (door_near, mut door_near_rx) = player(802, "door-near", "door", Point::new(3, 0));
    world.enter(door_near);
    let (door_cross, mut door_cross_rx) = player(803, "door-cross", AOI_MAP_A, Point::new(1, 0));
    world.enter(door_cross);
    drain(&mut door_actor_rx);
    drain(&mut door_near_rx);
    drain(&mut door_cross_rx);

    assert!(
        world.open_door(801, 1),
        "the real open_door production path must succeed"
    );
    let door_actor_packets = drain(&mut door_actor_rx);
    assert!(
        matches!(&door_actor_packets[..], [ServerPacket::Opendoor(door)] if !door.close && door.door_index == 1),
        "the actor must receive exactly one Opendoor(false) frame: {door_actor_packets:?}"
    );
    let door_near_packets = drain(&mut door_near_rx);
    assert!(
        matches!(&door_near_packets[..], [ServerPacket::Opendoor(door)] if !door.close && door.door_index == 1),
        "the same-map player must receive the broadcast exactly once: {door_near_packets:?}"
    );
    assert!(
        drain(&mut door_cross_rx).is_empty(),
        "the cross-map player must not receive the door broadcast"
    );
}
