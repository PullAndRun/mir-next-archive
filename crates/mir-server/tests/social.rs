//! t53：行会联盟 + 好友/黑名单（迁移 0015）。
//!
//! 覆盖：好友加删/重复/自加/备注/编解码往返、好友表下发；行会结盟/解除/上限/双向存储/
//! `GuildRecord` 往返、盟友 PK 豁免判定（`players_are_allied`，PK 扣分逻辑调用它）。

use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{MirClass, MirDirection, MirGender};
use mir_db::MirDb;
use mir_server::combat;
use mir_server::friends::{FriendEntry, MAX_FRIENDS, decode_friends, encode_friends};
use mir_server::guild::GuildAllyError;
use mir_server::guild::{Guild, GuildRef, GuildSettings, MAX_GUILD_ALLIES};
use mir_server::items;
use mir_server::npc::NpcState;
use mir_server::store::GuildRecord;
use mir_server::world::{Player, World};
use tokio::sync::mpsc;

const MAP: &str = "0";

fn world() -> World {
    let mut world = World::new(999);
    world.seed_rng(0x5EED);
    world.set_game_data(Arc::new(mir_server::gamedata::GameData::from_db(
        MirDb::default(),
        None,
    )));
    world
}

fn add_player(world: &mut World, object_id: u32, character_id: i64, guild_id: Option<i32>) {
    let stats = combat::player_base_stats(10, MirClass::Warrior);
    let (sender, _receiver) = mpsc::unbounded_channel();
    world.enter(Player {
        object_id,
        session_id: u64::from(object_id),
        character_id,
        name: format!("玩家{object_id}"),
        map: MAP.to_owned(),
        position: Point::new(100, 100),
        direction: MirDirection::Down,
        level: 10,
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
        visible_monsters: std::collections::BTreeSet::new(),
        visible_npcs: std::collections::BTreeSet::new(),
        movement_time: 0,
        inventory: items::new_inventory(),
        equipment: items::new_equipment(),
        storage: mir_server::storage::new_storage(),
        magics: Vec::new(),
        magic: mir_server::MagicState::default(),
        gold: 0,
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
        guild: guild_id.map(|guild_id| GuildRef {
            guild_id,
            rank_index: 0,
        }),
        trade: mir_server::TradeState::default(),
        trade_invitation: None,
        next_trade_time: 0,
        allow_trade: true,
        sender,
    });
}

fn guild_record(id: i32, name: &str, leader: i64) -> GuildRecord {
    GuildRecord {
        id,
        name: name.to_owned(),
        leader_character_id: leader,
        level: 1,
        experience: 0,
        gold: 0,
        spare_points: 0,
        notice: Vec::new(),
        ranks: vec![mir_server::store::GuildRankRecord {
            index: 0,
            name: "会长".to_owned(),
            options: 255,
        }],
        members: vec![mir_server::store::GuildMemberRecord {
            character_id: leader,
            name: format!("会长{leader}"),
            rank_index: 0,
            joined_at: 0,
        }],
        storage: Vec::new(),
        buffs: Vec::new(),
        allies: Vec::new(),
    }
}

#[test]
fn friends_codec_roundtrip_and_empty_default() {
    assert!(decode_friends(&[]).is_empty());
    let list = vec![
        FriendEntry {
            character_id: 7,
            name: "Alice".to_owned(),
            blocked: false,
            memo: "队友".to_owned(),
        },
        FriendEntry {
            character_id: 9,
            name: "Bob".to_owned(),
            blocked: true,
            memo: String::new(),
        },
    ];
    let bytes = encode_friends(&list);
    assert_eq!(decode_friends(&bytes), list);
    assert!(decode_friends(&[0xFF, 0x00]).is_empty(), "坏块按空列表处理");
}

#[test]
fn add_remove_and_block_friends() {
    let mut world = world();
    add_player(&mut world, 1, 10, None);
    add_player(&mut world, 2, 20, None);

    world.add_friend(1, 20, "玩家2", false).expect("加好友");
    world.add_friend(1, 30, "黑名单", true).expect("加黑名单");
    assert_eq!(world.friends_of(1).len(), 2);
    assert!(world.friends_of(1)[1].blocked);

    assert!(matches!(
        world.add_friend(1, 20, "玩家2", false),
        Err(mir_server::friends::FriendError::AlreadyAdded)
    ));
    assert!(matches!(
        world.add_friend(1, 10, "自己", false),
        Err(mir_server::friends::FriendError::CannotAddSelf)
    ));

    assert!(world.set_friend_memo(1, 20, "老朋友"));
    assert_eq!(world.friends_of(1)[0].memo, "老朋友");
    assert!(
        !world.set_friend_memo(1, 20, &"x".repeat(201)),
        "备注最多 200 字"
    );

    assert!(world.remove_friend(1, 20));
    assert!(!world.remove_friend(1, 20), "重复删除返回 false");
    assert_eq!(world.friends_of(1).len(), 1);
}

#[test]
fn friend_list_is_capped() {
    let mut world = world();
    add_player(&mut world, 1, 10, None);
    for index in 0..MAX_FRIENDS {
        world
            .add_friend(1, i64::try_from(index).unwrap() + 100, "好友", false)
            .expect("未满时都能加");
    }
    assert!(matches!(
        world.add_friend(1, 9_999, "溢出", false),
        Err(mir_server::friends::FriendError::ListFull)
    ));
}

#[test]
fn guild_alliance_is_bidirectional_and_persisted_in_record() {
    let mut world = world();
    world.load_guilds(vec![
        guild_record(1, "甲会", 10),
        guild_record(2, "乙会", 20),
    ]);
    assert!(!world.guilds_are_allied(1, 2));

    world.form_guild_alliance(1, 2).expect("结盟");
    assert!(world.guilds_are_allied(1, 2));
    assert!(world.guilds_are_allied(2, 1), "双向");
    assert_eq!(world.guild_allies(1), vec![2]);
    assert_eq!(world.guild_allies(2), vec![1]);

    // GuildRecord 往返（pg_store 用 encode_i32_list 落库）。
    let record = world.guild(1).expect("行会").to_record();
    let settings = GuildSettings::default();
    let restored = Guild::from_record(record.clone(), &settings);
    assert!(restored.allies.contains(&2), "从存档恢复联盟");
    let bytes = mir_store::encode_i32_list(&record.allies);
    assert_eq!(mir_store::decode_i32_list(&bytes), vec![2]);

    world.dissolve_guild_alliance(1, 2);
    assert!(!world.guilds_are_allied(1, 2));
    assert!(!world.guilds_are_allied(2, 1));
}

#[test]
fn guild_alliance_respects_max_limit() {
    let mut world = world();
    let mut records = vec![guild_record(1, "主会", 10)];
    for id in 2..=(MAX_GUILD_ALLIES as i32 + 2) {
        records.push(guild_record(id, &format!("友会{id}"), i64::from(id) * 10));
    }
    world.load_guilds(records);
    for id in 2..=(MAX_GUILD_ALLIES as i32 + 1) {
        world.form_guild_alliance(1, id).expect("未满时结盟");
    }
    assert_eq!(world.guild_allies(1).len(), MAX_GUILD_ALLIES);
    assert!(matches!(
        world.form_guild_alliance(1, MAX_GUILD_ALLIES as i32 + 2),
        Err(GuildAllyError::AllyLimitReached)
    ));
}

#[test]
fn allied_players_are_reported_as_allied() {
    let mut world = world();
    world.load_guilds(vec![
        guild_record(1, "甲会", 10),
        guild_record(2, "乙会", 20),
    ]);
    add_player(&mut world, 1, 10, Some(1));
    add_player(&mut world, 2, 20, Some(2));
    assert!(!world.players_are_allied(1, 2));
    world.form_guild_alliance(1, 2).expect("结盟");
    assert!(world.players_are_allied(1, 2), "盟友 PK 豁免判定");
    assert!(world.players_are_allied(2, 1));
}

#[test]
fn friend_update_packet_contains_saved_names_and_online_flags() {
    let mut world = world();
    // 手动建一个可读的接收端，验证 `S.FriendUpdate`。
    add_player(&mut world, 1, 10, None);
    let (sender, mut receiver) = mpsc::unbounded_channel();
    if let Some(player) = world.player_mut(1) {
        player.sender = sender;
    }
    world.add_friend(1, 20, "离线好友", false).expect("加好友");
    world.send_friends(1);
    let frame = receiver.try_recv().expect("应当下发 FriendUpdate");
    assert_eq!(i16::from_le_bytes([frame[2], frame[3]]), 245);
    let packet = mir_protocol::ServerPacket::decode(245, &frame[4..]).expect("解码");
    match packet {
        mir_protocol::ServerPacket::FriendUpdate(update) => {
            assert_eq!(update.friends.len(), 1);
            assert_eq!(update.friends[0].name, "离线好友");
            assert!(!update.friends[0].online);
        }
        other => panic!("期望 FriendUpdate，收到 {other:?}"),
    }
}

#[test]
fn friend_limit_is_configurable_and_zero_means_unlimited() {
    let mut world = world();
    add_player(&mut world, 1, 10, None);
    world.set_friend_limit(2);
    world.add_friend(1, 100, "A", false).expect("第 1 个");
    world.add_friend(1, 101, "B", false).expect("第 2 个");
    assert!(matches!(
        world.add_friend(1, 102, "C", false),
        Err(mir_server::friends::FriendError::ListFull)
    ));
    world.set_friend_limit(0);
    world
        .add_friend(1, 102, "C", false)
        .expect("0 = 不限（与 ServerConfig 其它阈值同口径）");
}

#[test]
fn guild_ally_limit_zero_means_unlimited() {
    let mut world = world();
    let mut records = vec![guild_record(1, "主会", 10)];
    for id in 2..=8 {
        records.push(guild_record(id, &format!("友会{id}"), i64::from(id) * 10));
    }
    world.load_guilds(records);
    world.set_guild_ally_limit(0);
    for id in 2..=8 {
        world
            .form_guild_alliance(1, id)
            .expect("0 = 不限，全部可结盟");
    }
    assert_eq!(world.guild_allies(1).len(), 7);
}
