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
