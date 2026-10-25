//! 第 51 轮 P07：**真实服务端 World → 线上编码帧 → 客户端解码 → 快照状态**。
//!
//! 刑部终审要求「客户端状态链：真实 World 输出 → 编码帧 → decode → 快照状态断言」，
//! 覆盖冲撞五连包 150/151/152/153 + `MagicCast`(122) 与 `Pushed`(127)/`ObjectPushed`(128)，
//! 并回归 `ObjectWalk`(28)/`ObjectRun`(29)。本测试在进程内起真实 [`World`]，学真技能、
//! 真施法，从玩家收包通道取**服务端已经编码好的帧**，再用 `ServerPacket::decode` +
//! [`WorldSnapshot::apply_packet_at`] 驱动客户端快照，最后断言快照状态。

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{AttackMode, MirClass, MirDirection, MirGender, Spell};
use mir_db::{MagicInfo, MapInfo, MirDb};
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::UserInformation;
use mir_server::gamedata::GameData;
use mir_server::{Player, World, combat};
use tokio::sync::mpsc;

use mir_client::net::WorldSnapshot;

/// 给快照补一条「已进入世界」的 `S.UserInformation`（真实联网时由登录链路下发；
/// 本测试直接起 `World`，用同一客户端 apply 入口注入，见 `WorldSnapshot::apply_packet` 注释）。
fn seed_self(
    snapshot: &mut WorldSnapshot,
    object_id: u32,
    name: &str,
    position: Point,
    level: u16,
) {
    let info = UserInformation {
        object_id,
        real_id: object_id,
        name: name.to_owned(),
        class: MirClass::Warrior,
        gender: MirGender::Male,
        level,
        location: position,
        direction: MirDirection::Down,
        hair: 1,
        hp: 100,
        mp: 100,
        ..UserInformation::default()
    };
    snapshot.apply_packet_at(&ServerPacket::UserInformation(info), 0);
}

/// 30×30 全可走地图（与 `crates/mir-server/tests/common/mod.rs` 的合成图一致）。
fn synthetic_map(name: &str) -> Arc<mir_world::Map> {
    mir_world::Map {
        width: 30,
        height: 30,
        cells: vec![mir_world::MapCell::default(); 900],
        file_name: name.to_owned(),
        version: mir_world::MapVersion::V0,
        doors: Vec::new(),
    }
    .into()
}

/// 第 51 轮技能模板（与 `crates/mir-server/src/magic.rs` 的 `round51_magic` 一致）。
fn magic_info(spell: Spell) -> MagicInfo {
    MagicInfo {
        name: format!("{spell:?}"),
        spell: spell.value(),
        base_cost: 1,
        level_cost: 1,
        icon: 0,
        level1: 1,
        level2: 2,
        level3: 3,
        need1: 0,
        need2: 0,
        need3: 0,
        delay_base: 1_000,
        delay_reduction: 0,
        power_base: 4,
        power_bonus: 0,
        m_power_base: 10,
        m_power_bonus: 0,
        range: 0,
        multiplier_base: 1.0,
        multiplier_bonus: 0.0,
    }
}

fn map_info(index: i32, file_name: &str) -> MapInfo {
    MapInfo {
        index,
        file_name: file_name.to_owned(),
        title: file_name.to_owned(),
        ..MapInfo::default()
    }
}

/// 真实世界：最小配表 + 两张合成图 + 时钟冻结到 0。
fn wire_world() -> World {
    let db = MirDb {
        magics: vec![magic_info(Spell::ShoulderDash)],
        maps: vec![map_info(1, "0"), map_info(2, "nt")],
        ..MirDb::default()
    };
    let mut world = World::new(999);
    world.set_game_data(Arc::new(GameData::from_db(db, None)));
    world.insert_map("0", synthetic_map("0"));
    world.insert_map("nt", synthetic_map("nt"));
    world.advance_clock(0);
    world.seed_rng(51);
    world
}

/// 真玩家 + 收包通道（字段与 `crates/mir-server/tests/common/mod.rs` 同步）。
fn add_player(
    world: &mut World,
    object_id: u32,
    level: u16,
    map: &str,
    position: Point,
) -> mpsc::UnboundedReceiver<Vec<u8>> {
    let stats = combat::player_base_stats(level, MirClass::Warrior);
    let (sender, receiver) = mpsc::unbounded_channel();
    world.enter(Player {
        object_id,
        session_id: u64::from(object_id),
        character_id: i64::from(object_id),
        name: format!("玩家{object_id}"),
        map: map.to_owned(),
        position,
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
        npc: mir_server::NpcState::default(),
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
    receiver
}

fn learn(world: &mut World, object_id: u32, spell: Spell, level: u8) {
    let magic = world.learn_magic(object_id, spell).expect("学技能应成功");
    if let Some(player) = world.player_mut(object_id) {
        if let Some(entry) = player.magics.iter_mut().find(|m| m.spell == magic.spell) {
            entry.level = level;
        }
    }
}

/// 取出收包通道里的全部原始帧（服务端 `encode_frame` 的输出，含 4 字节头）。
fn frames(receiver: &mut mpsc::UnboundedReceiver<Vec<u8>>) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    while let Ok(frame) = receiver.try_recv() {
        out.push(frame);
    }
    out
}

/// 把服务端帧解码成 `ServerPacket`（`decode` 来自协议 crate，与网络线程同一入口）。
fn decode(raw: Vec<Vec<u8>>) -> Vec<ServerPacket> {
    raw.into_iter()
        .map(|frame| {
            let index = i16::from_le_bytes([frame[2], frame[3]]);
            ServerPacket::decode(index, &frame[4..]).expect("服务端帧必须能被客户端解码")
        })
        .collect()
}

/// 按真实时间顺序把包喂给快照（`apply_packet_at` 是网络线程与无头 UI 共用入口）。
fn apply(snapshot: &mut WorldSnapshot, packets: &[ServerPacket]) -> HashMap<String, u32> {
    let mut counts = HashMap::new();
    for (index, packet) in packets.iter().enumerate() {
        let now_ms = 1_000 + u64::try_from(index).unwrap_or(0) * 10;
        snapshot.apply_packet_at(packet, now_ms);
        *counts.entry(packet_name(packet).to_owned()).or_insert(0) += 1;
    }
    counts
}

fn packet_name(packet: &ServerPacket) -> &'static str {
    match packet {
        ServerPacket::UserDash(_) => "UserDash",
        ServerPacket::ObjectDash(_) => "ObjectDash",
        ServerPacket::UserDashFail(_) => "UserDashFail",
        ServerPacket::ObjectDashFail(_) => "ObjectDashFail",
        ServerPacket::Pushed(_) => "Pushed",
        ServerPacket::ObjectPushed(_) => "ObjectPushed",
        ServerPacket::MagicCast(_) => "MagicCast",
        ServerPacket::ObjectWalk(_) => "ObjectWalk",
        ServerPacket::ObjectRun(_) => "ObjectRun",
        ServerPacket::ObjectPlayer(_) => "ObjectPlayer",
        ServerPacket::UserInformation(_) => "UserInformation",
        ServerPacket::Chat(_) => "Chat",
        _ => "Other",
    }
}

#[test]
fn dash_wire_chain_updates_observer_caster_and_pushed_player_snapshots() {
    let mut world = wire_world();
    let mut caster_rx = add_player(&mut world, 1, 20, "0", Point::new(10, 10));
    let mut observer_rx = add_player(&mut world, 2, 20, "0", Point::new(14, 10));
    let mut pushed_rx = add_player(&mut world, 3, 5, "0", Point::new(10, 11));
    // 冲撞要推同图低等级玩家：施法者必须处在可攻击模式（`IsAttackTarget`，
    // Crystal `PlayerObject.IsAttackTarget` / `HumanObject.cs:5016`）。
    world.player_mut(1).expect("施法者").attack_mode = AttackMode::All;

    // 先让 2 号走一步、再跑一步（回归 ObjectWalk 28 / ObjectRun 29 的客户端消费）。
    world.walk(2, MirDirection::Right).expect("2 号走一步");
    world.advance_clock(700);
    world.run(2, MirDirection::Right).expect("2 号跑一步");

    learn(&mut world, 1, Spell::ShoulderDash, 3);
    world
        .cast_magic(
            1,
            Spell::ShoulderDash,
            MirDirection::Down,
            0,
            Point::new(0, 0),
        )
        .expect("冲撞施法成功");

    let caster_final = world.player(1).expect("施法者").position;
    let observer_final = world.player(2).expect("观察者").position;
    let pushed_final = world.player(3).expect("被推者").position;
    // Crystal 的冲撞循环每一格都会对同一个目标再调一次 `Pushed(this, dir, 1)`
    // （`HumanObject.cs:5047-5061`），所以被推者会被连续顶开 travelled 格。
    let travelled = caster_final.y - 10;
    assert_eq!(
        pushed_final,
        Point::new(10, 11 + travelled),
        "5 级玩家被 20 级施法者连续顶开 travelled 格（HumanObject.Pushed）"
    );
    assert!(caster_final.y > 11, "level 3 冲撞至少走 level+2 = 5 格");

    // ---- 观察者（2 号）快照：ObjectDash 逐格推进远端实体，ObjectPushed 更新被推者 ----
    let observer_packets = decode(frames(&mut observer_rx));
    let mut observer = WorldSnapshot::default();
    let counts = apply(&mut observer, &observer_packets);
    assert_eq!(
        counts.get("ObjectDash").copied().unwrap_or(0),
        u32::try_from(caster_final.y - 10).unwrap_or(0),
        "每走一格一条 S.ObjectDash（151）"
    );
    let caster_entity = observer.entities.get(&1).expect("观察者快照里有施法者");
    assert_eq!(
        caster_entity.position, caster_final,
        "ObjectDash 把施法者推到服务端权威落点"
    );
    let pushed_entity = observer.entities.get(&3).expect("观察者快照里有被推玩家");
    assert_eq!(
        pushed_entity.position, pushed_final,
        "ObjectPushed（128）把被推者推进一格"
    );

    // ---- 施法者（1 号）快照：UserDash 逐格推进自机，MagicCast 收尾 ----
    let caster_packets = decode(frames(&mut caster_rx));
    let mut caster = WorldSnapshot::default();
    seed_self(&mut caster, 1, "玩家1", Point::new(10, 10), 20);
    let counts = apply(&mut caster, &caster_packets);
    assert_eq!(
        counts.get("UserDash").copied().unwrap_or(0),
        u32::try_from(travelled).unwrap_or(0),
        "每走一格一条 S.UserDash（150）"
    );
    assert_eq!(
        counts.get("MagicCast").copied().unwrap_or(0),
        1,
        "冲撞结束必发 S.MagicCast（122，HumanObject.cs:5154）"
    );
    assert_eq!(
        counts.get("ObjectPushed").copied().unwrap_or(0),
        u32::try_from(travelled).unwrap_or(0),
        "施法者不在被推者本人位置，但会收到每一格的 S.ObjectPushed（128）"
    );
    let me = caster.self_player.as_ref().expect("施法者快照有自己");
    assert_eq!(me.position, caster_final, "UserDash 推进自机落点");
    assert_eq!(me.direction, MirDirection::Down, "冲撞不转向");
    assert!(
        caster
            .sound_events
            .iter()
            .any(|event| matches!(event.spell, Spell::ShoulderDash)),
        "客户端为冲撞收尾的 MagicCast 响了施法音"
    );
    assert_eq!(
        caster.entities.get(&2).expect("走/跑玩家").position,
        observer_final,
        "ObjectWalk/ObjectRun 回归：远端走跑照常更新快照"
    );

    // ---- 被推者（3 号）快照：Pushed（127）推进自机一格 ----
    let pushed_packets = decode(frames(&mut pushed_rx));
    let mut pushed = WorldSnapshot::default();
    seed_self(&mut pushed, 3, "玩家3", Point::new(10, 11), 5);
    let counts = apply(&mut pushed, &pushed_packets);
    assert_eq!(
        counts.get("Pushed").copied().unwrap_or(0),
        u32::try_from(travelled).unwrap_or(0),
        "被推者每格收到一条 S.Pushed（127）"
    );
    let me = pushed.self_player.as_ref().expect("被推者快照有自己");
    assert_eq!(
        me.position, pushed_final,
        "Pushed 把被推者推进一格且朝向为反方向"
    );
}

#[test]
fn dash_zero_step_chain_sends_fail_and_chat_to_the_caster() {
    let mut world = wire_world();
    // 地图边缘（30×30）：面向 Down 的下一格 y=30 越界 → 零步失败。
    let mut caster_rx = add_player(&mut world, 1, 20, "0", Point::new(12, 29));
    let mut observer_rx = add_player(&mut world, 2, 20, "0", Point::new(14, 29));
    learn(&mut world, 1, Spell::ShoulderDash, 3);
    world
        .cast_magic(
            1,
            Spell::ShoulderDash,
            MirDirection::Down,
            0,
            Point::new(0, 0),
        )
        .expect("冲撞施法成功（服务端内部判定失败）");
    let start = Point::new(12, 29);
    assert_eq!(
        world.player(1).expect("施法者").position,
        start,
        "零移动不改坐标"
    );

    let caster_packets = decode(frames(&mut caster_rx));
    let mut caster = WorldSnapshot::default();
    seed_self(&mut caster, 1, "玩家1", start, 20);
    let counts = apply(&mut caster, &caster_packets);
    assert_eq!(
        counts.get("UserDashFail").copied().unwrap_or(0),
        1,
        "零移动必发 S.UserDashFail（152，HumanObject.cs:5128-5130）"
    );
    assert_eq!(
        counts.get("MagicCast").copied().unwrap_or(0),
        1,
        "失败流程也以 S.MagicCast 收尾"
    );
    assert_eq!(
        caster.self_player.as_ref().expect("自己").position,
        start,
        "UserDashFail 拉回权威格（同格）"
    );
    assert!(
        caster
            .chat
            .iter()
            .any(|line| line.text.contains("冲撞力量不足")),
        "失败聊天走 i18n 键 NotEnoughPushingPower，客户端快照里能看到"
    );

    let observer_packets = decode(frames(&mut observer_rx));
    let mut observer = WorldSnapshot::default();
    let counts = apply(&mut observer, &observer_packets);
    assert_eq!(
        counts.get("ObjectDashFail").copied().unwrap_or(0),
        1,
        "周边玩家收到 S.ObjectDashFail（153）"
    );
    assert_eq!(
        observer.entities.get(&1).expect("施法者实体").position,
        start,
        "ObjectDashFail 把远端实体钉在权威格"
    );
}
