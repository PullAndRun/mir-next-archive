//! GM 命令（`@level` / `@item` / `@map` / `@kick` …）的确定性测试。
//!
//! 这些用例直接驱动 [`World::run_gm_command`]（权限门在 `session.rs`，见
//! `tests/login_flow.rs` 的 e2e 用例）；世界只用到 `mir-server` 自己的确定性时钟，
//! 不 sleep、不依赖真实网络。

use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{AttackMode, BindMode, ItemType, MirClass, MirDirection, MirGender, Stat};
use mir_db::{ItemInfo, MapInfo, MirDb};
use mir_protocol::ServerPacket;
use mir_protocol::data::{ItemInfo as ProtoItemInfo, Stats};
use mir_protocol::packets::server::Chat;
use mir_server::combat::{self, CombatStats};
use mir_server::gamedata::GameData;
use mir_server::items;
use mir_server::world::{Player, World};
use mir_server::{MagicState, NpcState, TradeState};
use tokio::sync::mpsc;

const MAP: &str = "0";
/// 可堆叠药品模板（`stack_size = 20`）。
const POTION: i32 = 100;
/// 武器模板。
const SWORD: i32 = 200;

fn stats(pairs: &[(Stat, i32)]) -> Stats {
    Stats::from_iter(pairs.iter().copied())
}

fn item_info(index: i32, name: &str, stack_size: u16) -> ItemInfo {
    ItemInfo {
        index,
        name: name.to_owned(),
        item_type: if stack_size > 1 {
            ItemType::Potion
        } else {
            ItemType::Weapon
        },
        bind: BindMode::None,
        weight: 1,
        image: u16::try_from(index).unwrap_or(0),
        stack_size,
        price: 100,
        stats: stats(&[(Stat::MaxDC, 1)]),
        ..ProtoItemInfo::default()
    }
}

fn database() -> MirDb {
    MirDb {
        items: vec![item_info(POTION, "金创药", 20), item_info(SWORD, "木剑", 1)],
        // @map <索引> 走 MapInfo，所以测试库要有一张地图（文件名与 MAP 一致）。
        maps: vec![MapInfo {
            index: 0,
            file_name: MAP.to_owned(),
            title: "测试地图".to_owned(),
            ..MapInfo::default()
        }],
        ..MirDb::default()
    }
}

fn build_world() -> World {
    let mut world = World::new(999);
    world.seed_rng(11);
    world.set_game_data(Arc::new(GameData::from_db(database(), None)));
    world
}

/// 建一个玩家；返回其系统聊天接收端（GM 回复都走 `S.Chat`）。
fn add_player(world: &mut World, object_id: u32, name: &str) -> mpsc::UnboundedReceiver<Vec<u8>> {
    let base: CombatStats = combat::player_base_stats(10, MirClass::Warrior);
    let (sender, receiver) = mpsc::unbounded_channel();
    world.enter(Player {
        object_id,
        session_id: u64::from(object_id),
        character_id: i64::from(object_id),
        name: name.to_owned(),
        map: MAP.to_owned(),
        position: Point::new(10, 10),
        direction: MirDirection::Down,
        level: 10,
        experience: 0,
        max_experience: 0,
        class: MirClass::Warrior,
        gender: MirGender::Male,
        hair: 1,
        hp: base.hp,
        mp: base.mp,
        stats: base,
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
        visible_monsters: std::collections::BTreeSet::new(),
        visible_npcs: std::collections::BTreeSet::new(),
        movement_time: 0,
        inventory: items::new_inventory(),
        equipment: items::new_equipment(),
        storage: mir_server::storage::new_storage(),
        magics: Vec::new(),
        magic: MagicState::default(),
        gold: 1_000,
        credit: 0,
        visible_items: std::collections::BTreeSet::new(),
        quests: Vec::new(),
        completed_quests: std::collections::BTreeSet::new(),
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
        sender,
    });
    receiver
}

/// 收取玩家收到的所有系统聊天文本。
fn chat_lines(receiver: &mut mpsc::UnboundedReceiver<Vec<u8>>) -> Vec<String> {
    let mut lines = Vec::new();
    while let Ok(frame) = receiver.try_recv() {
        let index = i16::from_le_bytes([frame[2], frame[3]]);
        if let Ok(ServerPacket::Chat(Chat { message, .. })) =
            ServerPacket::decode(index, &frame[4..])
        {
            lines.push(message);
        }
    }
    lines
}

/// 收取玩家收到的所有包（用于断言 `S.Death` 这类非聊天包）。
fn packets(receiver: &mut mpsc::UnboundedReceiver<Vec<u8>>) -> Vec<ServerPacket> {
    let mut packets = Vec::new();
    while let Ok(frame) = receiver.try_recv() {
        let index = i16::from_le_bytes([frame[2], frame[3]]);
        if let Ok(packet) = ServerPacket::decode(index, &frame[4..]) {
            packets.push(packet);
        }
    }
    packets
}

fn anyone(receiver: &mut mpsc::UnboundedReceiver<Vec<u8>>) -> String {
    chat_lines(receiver).into_iter().next().unwrap_or_default()
}

#[test]
fn level_command_updates_level_stats_and_max_experience() {
    let mut world = build_world();
    let mut rx = add_player(&mut world, 1, "gm");
    assert!(world.run_gm_command(1, "@level 50"));
    let player = world.player(1).expect("玩家在");
    assert_eq!(player.level, 50);
    assert_eq!(
        player.max_experience,
        world.exp_table().exp_for_level(50),
        "@level 必须按新等级重算下一级经验（Crystal RefreshMaxExperience）"
    );
    assert!(player.hp > 0 && player.mp >= 0);
    let reply = anyone(&mut rx);
    assert!(reply.contains("Level set to 50"), "回复：{reply}");
}

#[test]
fn gold_and_credit_commands_add_currency() {
    let mut world = build_world();
    let mut rx = add_player(&mut world, 1, "gm");
    assert!(world.run_gm_command(1, "@gold 500"));
    assert!(world.run_gm_command(1, "@credit 77"));
    let player = world.player(1).expect("玩家在");
    assert_eq!(player.gold, 1_500, "金币在原有 1000 上累加");
    assert_eq!(player.credit, 77);
    let lines = chat_lines(&mut rx);
    assert!(lines.iter().any(|line| line.contains("Gold +500")));
    assert!(lines.iter().any(|line| line.contains("Credits +77")));
}

#[test]
fn item_command_gives_items_by_index_and_by_name() {
    let mut world = build_world();
    let mut rx = add_player(&mut world, 1, "gm");
    assert!(world.run_gm_command(1, "@item 100 5"), "按索引发药");
    assert!(world.run_gm_command(1, "@item 木剑"), "按名字发武器");
    let player = world.player(1).expect("玩家在");
    let potions: u16 = player
        .inventory
        .iter()
        .flatten()
        .filter(|item| item.item_index == POTION)
        .map(|item| item.count)
        .sum();
    assert_eq!(potions, 5);
    assert!(
        player
            .inventory
            .iter()
            .flatten()
            .any(|item| item.item_index == SWORD),
        "按名字应发出木剑"
    );
    let lines = chat_lines(&mut rx);
    assert!(lines.iter().any(|line| line.contains("Gave 5 x 金创药")));
}

#[test]
fn item_command_reports_unknown_item() {
    let mut world = build_world();
    let mut rx = add_player(&mut world, 1, "gm");
    assert!(world.run_gm_command(1, "@item no_such_item"));
    let reply = anyone(&mut rx);
    assert!(reply.contains("Item not found"), "回复：{reply}");
}

#[test]
fn map_command_teleports_to_given_coordinates() {
    let mut world = build_world();
    let mut rx = add_player(&mut world, 1, "gm");
    assert!(world.run_gm_command(1, "@map 0 33 44"), "地图索引 + 坐标");
    let player = world.player(1).expect("玩家在");
    assert_eq!(player.position, Point::new(33, 44));
    assert_eq!(player.map, MAP);
    let reply = anyone(&mut rx);
    assert!(reply.contains("Teleported to 0"), "回复：{reply}");
}

#[test]
fn heal_command_restores_hp_and_revives() {
    let mut world = build_world();
    let mut rx = add_player(&mut world, 1, "gm");
    {
        let player = world.player_mut(1).expect("玩家在");
        player.hp = 1;
    }
    assert!(world.run_gm_command(1, "@heal"));
    let player = world.player(1).expect("玩家在");
    assert_eq!(player.hp, player.stats.hp, "回满 HP");
    assert!(anyone(&mut rx).contains("Healed"));
}

#[test]
fn kill_command_kills_named_player() {
    let mut world = build_world();
    let _gm_rx = add_player(&mut world, 1, "gm");
    let mut victim_rx = add_player(&mut world, 2, "victim");
    assert!(world.run_gm_command(1, "@kill victim"));
    let victim = world.player(2).expect("目标在");
    assert!(victim.dead && victim.hp == 0, "@kill 目标应死亡");
    assert!(victim.revive_time > 0, "应设置自动复活计时");
    let packets = packets(&mut victim_rx);
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::Death(_))),
        "目标应收到 S.Death：{packets:?}"
    );
}

#[test]
fn who_lists_players_exactly_once() {
    let mut world = build_world();
    let mut rx = add_player(&mut world, 1, "gm");
    add_player(&mut world, 2, "other");
    assert!(world.run_gm_command(1, "@who"));
    let lines = chat_lines(&mut rx);
    let joined = lines.join("\n");
    assert!(joined.contains("2 player(s) online"), "{joined}");
    assert_eq!(
        joined.matches("other").count(),
        1,
        "每个玩家只应出现一次（不能合并一条再逐行重复）：{joined}"
    );
}

#[test]
fn kick_command_reports_missing_target() {
    let mut world = build_world();
    let mut rx = add_player(&mut world, 1, "gm");
    assert!(world.run_gm_command(1, "@kick ghost"));
    let reply = anyone(&mut rx);
    assert!(reply.contains("Player not found"), "回复：{reply}");
}

#[test]
fn invalid_arguments_report_usage_and_change_nothing() {
    let mut world = build_world();
    let mut rx = add_player(&mut world, 1, "gm");
    assert!(world.run_gm_command(1, "@level abc"));
    assert!(world.run_gm_command(1, "@gold"));
    let player = world.player(1).expect("玩家在");
    assert_eq!(player.level, 10, "非法参数不能改等级");
    assert_eq!(player.gold, 1_000, "非法参数不能改金币");
    let lines = chat_lines(&mut rx);
    assert!(lines.iter().any(|line| line.contains("Usage: @level")));
    assert!(lines.iter().any(|line| line.contains("Usage: @gold")));
}

#[test]
fn unknown_command_hints_at_help() {
    let mut world = build_world();
    let mut rx = add_player(&mut world, 1, "gm");
    assert!(world.run_gm_command(1, "@nope"));
    let reply = anyone(&mut rx);
    assert!(reply.contains("@help"), "回复：{reply}");
}

#[test]
fn recall_moves_target_to_gm() {
    let mut world = build_world();
    let _gm_rx = add_player(&mut world, 1, "gm");
    let _other_rx = add_player(&mut world, 2, "scout");
    {
        let gm = world.player_mut(1).expect("GM 在");
        gm.position = Point::new(20, 21);
    }
    assert!(world.run_gm_command(1, "@recall scout"));
    let scout = world.player(2).expect("目标在");
    assert_eq!(scout.position, Point::new(20, 21));
}

#[test]
fn non_command_chat_is_not_consumed() {
    let mut world = build_world();
    let _rx = add_player(&mut world, 1, "gm");
    assert!(!world.run_gm_command(1, "hello world"), "普通聊天不被消费");
    assert!(!world.run_gm_command(1, "@"), "单独的 @ 不是命令");
}
