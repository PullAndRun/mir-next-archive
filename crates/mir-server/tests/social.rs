//! t53：行会联盟 + 好友/黑名单（迁移 0015）。
//!
//! 覆盖：好友加删/重复/自加/备注/编解码往返、好友表下发；行会结盟/解除/上限/双向存储/
//! `GuildRecord` 往返、盟友 PK 豁免判定（`players_are_allied`，PK 扣分逻辑调用它）。

use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{ItemType, MirClass, MirDirection, MirGender, RequiredClass};
use mir_db::MirDb;
use mir_protocol::ServerPacket;
use mir_protocol::data::{ItemInfo, UserItem};
use mir_server::combat;
use mir_server::friends::{FriendEntry, MAX_FRIENDS, decode_friends, encode_friends};
use mir_server::guild::GuildAllyError;
use mir_server::guild::{Guild, GuildSettings, MAX_GUILD_ALLIES};
use mir_server::items;
use mir_server::npc::NpcState;
use mir_server::store::Character;
use mir_server::store::GuildRecord;
use mir_server::world::{MARRIAGE_LEVEL_REQUIRED, MENTOR_LEVEL_GAP, Player, SocialError, World};
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
    let (sender, _receiver) = mpsc::unbounded_channel();
    let _ = guild_id;
    add_player_with_channel(
        world,
        object_id,
        character_id,
        10,
        Point::new(100, 100),
        MAP,
        sender,
    );
}

#[allow(clippy::too_many_arguments)]
fn add_player_with_channel(
    world: &mut World,
    object_id: u32,
    character_id: i64,
    level: u16,
    position: Point,
    map: &str,
    sender: mpsc::UnboundedSender<Vec<u8>>,
) {
    let stats = combat::player_base_stats(level, MirClass::Warrior);
    world.enter(Player {
        object_id,
        session_id: u64::from(object_id),
        character_id,
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
        guild: None,
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

#[test]
fn ally_replies_are_localized() {
    use mir_core::i18n::{Language, TextKey};

    let mut world = world();
    add_player(&mut world, 1, 10, None);
    world.set_player_language(1, Language::ZhCn);
    assert_eq!(
        world.text(1, TextKey::AllyUsage),
        "用法：/ally request|accept|remove <行会名或ID>"
    );
    assert_eq!(world.text(1, TextKey::AllyGuildNotFound), "找不到该行会。");
    assert_eq!(
        world.text(1, TextKey::AllyRequested),
        "结盟申请已发送，等待对方会长同意。"
    );
    assert_eq!(world.text(1, TextKey::AllyAccepted), "结盟成功。");
    assert_eq!(world.text(1, TextKey::AllyRemoved), "已解除联盟。");
    assert_eq!(world.text(1, TextKey::AllyFailed), "联盟操作失败。");

    world.set_player_language(1, Language::En);
    assert_eq!(
        world.text(1, TextKey::AllyUsage),
        "Usage: /ally request|accept|remove <guild name or id>"
    );
    assert_eq!(
        world.text(1, TextKey::AllyGuildNotFound),
        "Guild not found."
    );
    assert_eq!(
        world.text(1, TextKey::AllyRequested),
        "Ally request sent; waiting for the other guild leader's approval."
    );
    assert_eq!(world.text(1, TextKey::AllyAccepted), "Alliance formed.");
    assert_eq!(world.text(1, TextKey::AllyRemoved), "Alliance dissolved.");
    assert_eq!(world.text(1, TextKey::AllyFailed), "Ally operation failed.");
}

// ---------------------------------------------------------------------------
// 8. 婚姻与师徒（t109）
// ---------------------------------------------------------------------------

/// 建一个能收包的玩家：`add_player` 会把接收端丢掉，这里改成返回接收端。
fn add_player_with_receiver(
    world: &mut World,
    object_id: u32,
    character_id: i64,
    level: u16,
    position: Point,
    map: &str,
) -> tokio::sync::mpsc::UnboundedReceiver<Vec<u8>> {
    let (sender, receiver) = mpsc::unbounded_channel();
    add_player_with_channel(world, object_id, character_id, level, position, map, sender);
    receiver
}

fn drain_packets(
    receiver: &mut tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
) -> Vec<ServerPacket> {
    let mut packets = Vec::new();
    while let Ok(frame) = receiver.try_recv() {
        let index = i16::from_le_bytes([frame[2], frame[3]]);
        packets.push(ServerPacket::decode(index, &frame[4..]).expect("解码服务端包"));
    }
    packets
}

/// 造两个（或更多）玩家并让他们站在同一格附近，返回 `object_id` 列表。
fn couple(world: &mut World, levels: &[u16]) -> Vec<u32> {
    let mut ids = Vec::new();
    for (index, level) in levels.iter().enumerate() {
        let object_id = u32::try_from(index + 1).unwrap();
        let character_id = i64::try_from(index + 1).unwrap();
        add_player(world, object_id, character_id, None);
        if let Some(player) = world.player_mut(object_id) {
            player.level = *level;
            player.position = Point::new(100 + i32::try_from(index).unwrap(), 100);
        }
        ids.push(object_id);
    }
    ids
}

#[test]
fn marriage_request_and_accept_links_both_sides() {
    let mut world = world();
    let ids = couple(&mut world, &[20, 20]);
    world
        .marriage_request(ids[0], ids[1])
        .expect("同图相邻、双方 20 级，可以求婚");
    let proposer = world.marriage_reply(ids[1], true).expect("接受求婚");
    assert_eq!(proposer, ids[0]);
    let (a, b) = (world.player(ids[0]).unwrap(), world.player(ids[1]).unwrap());
    assert_eq!(world.spouse_of(a.character_id), Some(b.character_id));
    assert_eq!(world.spouse_of(b.character_id), Some(a.character_id));
    assert!(world.married_since(a.character_id).is_some());
}

#[test]
fn marriage_request_rejects_underlevel_and_self() {
    let mut world = world();
    let ids = couple(&mut world, &[5, 20]);
    assert!(matches!(
        world.marriage_request(ids[0], ids[1]),
        Err(SocialError::LevelTooLow {
            required: MARRIAGE_LEVEL_REQUIRED,
            ..
        })
    ));
    let ids = couple(&mut world, &[20, 20]);
    assert_eq!(
        world.marriage_request(ids[1], ids[1]),
        Err(SocialError::SelfTarget)
    );
}

#[test]
fn marriage_reply_declined_keeps_both_single() {
    let mut world = world();
    let ids = couple(&mut world, &[20, 20]);
    world.marriage_request(ids[0], ids[1]).expect("求婚");
    world.marriage_reply(ids[1], false).expect("拒绝");
    assert!(
        world
            .spouse_of(world.player(ids[0]).unwrap().character_id)
            .is_none()
    );
    assert!(
        world
            .spouse_of(world.player(ids[1]).unwrap().character_id)
            .is_none()
    );
    // 拒绝后没有待回应求婚，再回应会报错。
    assert_eq!(
        world.marriage_reply(ids[1], true),
        Err(SocialError::NoProposal)
    );
}

#[test]
fn divorce_clears_both_sides() {
    let mut world = world();
    let ids = couple(&mut world, &[20, 20]);
    world.marriage_request(ids[0], ids[1]).unwrap();
    world.marriage_reply(ids[1], true).unwrap();
    let spouse = world.divorce(ids[0]).expect("离婚");
    assert_eq!(spouse, world.player(ids[1]).unwrap().character_id);
    assert!(
        world
            .spouse_of(world.player(ids[0]).unwrap().character_id)
            .is_none()
    );
    assert!(
        world
            .spouse_of(world.player(ids[1]).unwrap().character_id)
            .is_none()
    );
    assert!(world.divorce(ids[0]).is_err(), "已经离了不能再离");
}

#[test]
fn spouse_name_is_lookupable_after_relogin_style_restore() {
    // 「配偶名显示」的数据来源：`spouse_of(character_id)` → 再按 character_id 找名字。
    let mut world = world();
    let ids = couple(&mut world, &[20, 20]);
    world.marriage_request(ids[0], ids[1]).unwrap();
    world.marriage_reply(ids[1], true).unwrap();
    let character = world.player(ids[0]).unwrap().character_id;
    let spouse_character = world.spouse_of(character).expect("已婚");
    let spouse_name = world
        .player_of_character(spouse_character)
        .map(|player| player.name.clone())
        .expect("配偶在线时能查到名字");
    assert_eq!(spouse_name, world.player(ids[1]).unwrap().name);
}

#[test]
fn mentor_request_enforces_level_gap_and_class() {
    let mut world = world();
    // 徒弟 5 级、师傅 20 级：差 15 >= 10，可以；师傅 12 级则不行（差 7 < 10）。
    let ids = couple(&mut world, &[5, 20]);
    world.set_allow_mentor(ids[1], true);
    world
        .mentor_request(ids[0], ids[1])
        .expect("等级差 15 满足");
    world.mentor_reply(ids[1], true).expect("接受");

    let mut world = self::world();
    let ids = couple(&mut world, &[5, 12]);
    world.set_allow_mentor(ids[1], true);
    assert!(matches!(
        world.mentor_request(ids[0], ids[1]),
        Err(SocialError::MentorLevelGap {
            gap: MENTOR_LEVEL_GAP
        })
    ));
}

#[test]
fn mentee_experience_banks_for_the_mentor() {
    let mut world = world();
    let ids = couple(&mut world, &[5, 20]);
    world.set_allow_mentor(ids[1], true);
    world.mentor_request(ids[0], ids[1]).unwrap();
    world.mentor_reply(ids[1], true).unwrap();
    world.gain_experience(ids[0], 1_000);
    let link = world.mentorship_of(ids[0]).expect("师徒关系");
    // `MenteeExpBank = 5` → 1000 * 5 / 100 = 50。
    assert_eq!(link.mentee_exp, 50);
}

#[test]
fn mentoring_ends_and_mentor_collects_the_bank() {
    let mut world = world();
    let ids = couple(&mut world, &[5, 20]);
    world.set_allow_mentor(ids[1], true);
    world.mentor_request(ids[0], ids[1]).unwrap();
    world.mentor_reply(ids[1], true).unwrap();
    world.gain_experience(ids[0], 2_000);
    let before = world.player(ids[1]).unwrap().experience;
    let link = world.mentor_break(ids[0]).expect("解除师徒");
    assert_eq!(link.mentee_exp, 100);
    assert!(world.mentorship_of(ids[0]).is_none());
    let after = world.player(ids[1]).unwrap().experience;
    assert_eq!(after, before + 100, "师傅一次性领走攒下的徒弟经验");
}

#[test]
fn mentee_graduates_automatically_when_level_gap_closes() {
    // Crystal `PlayerObject.cs:940-945`：升级后 `Level + MentorLevelGap > Mentor.Level` → `MentorBreak()`。
    let mut world = world();
    // 徒弟 5 级、师傅 14 级（差 9 < 10 才允许拜师？——差 9 不满足 `5 + 10 <= 14`，所以用 15）。
    let ids = couple(&mut world, &[5, 15]);
    world.set_allow_mentor(ids[1], true);
    world
        .mentor_request(ids[0], ids[1])
        .expect("等级差 10 满足");
    world.mentor_reply(ids[1], true).expect("接受");
    world.gain_experience(ids[0], 1_000);
    let banked = world.mentorship_of(ids[0]).unwrap().mentee_exp;
    assert_eq!(banked, 50);

    // 徒弟升到 6 级：6 + 10 > 15 → 自动出师。
    world.player_mut(ids[0]).unwrap().level = 6;
    let mentor_before = world.player(ids[1]).unwrap().experience;
    world.player_level_up(ids[0]);
    assert!(
        world.mentorship_of(ids[0]).is_none(),
        "等级差不再满足时应自动出师"
    );
    let mentor_after = world.player(ids[1]).unwrap().experience;
    assert_eq!(
        mentor_after,
        mentor_before + banked,
        "出师时师傅领走攒下的经验"
    );
}

#[test]
fn social_snapshot_carries_marriage_and_mentorship_for_persistence() {
    let mut world = world();
    let ids = couple(&mut world, &[20, 20]);
    world.marriage_request(ids[0], ids[1]).unwrap();
    world.marriage_reply(ids[1], true).unwrap();
    let character = world.player(ids[0]).unwrap().character_id;
    let spouse_character = world.player(ids[1]).unwrap().character_id;

    let snapshot = world.social_snapshot(ids[0]).expect("快照");
    assert_eq!(snapshot.married, i32::try_from(spouse_character).unwrap());
    assert!(snapshot.married_date.is_some(), "结婚时刻要进快照");

    // 把快照并进角色行（`Character::merge_social`），再把行读回来 —— 等价于「落库 → 重登」。
    let mut row = Character::new(
        1,
        1,
        "甲",
        MirClass::Warrior,
        MirGender::Male,
        "0",
        (100, 100),
    );
    row.merge_social(snapshot);
    assert_eq!(row.spouse, spouse_character);
    assert_eq!(row.mentor, 0);
    assert!(!row.is_mentor);

    // 师徒方向：徒弟的 mentor 字段、师傅的 is_mentor + mentee_exp。
    let mut world = self::world();
    let ids = couple(&mut world, &[5, 20]);
    world.set_allow_mentor(ids[1], true);
    world.mentor_request(ids[0], ids[1]).unwrap();
    world.mentor_reply(ids[1], true).unwrap();
    world.gain_experience(ids[0], 1_000);
    let mentee_snapshot = world.social_snapshot(ids[0]).expect("徒弟快照");
    assert_eq!(
        mentee_snapshot.mentor,
        i32::try_from(world.player(ids[1]).unwrap().character_id).unwrap()
    );
    assert_eq!(mentee_snapshot.mentor_exp, 50);

    let mentor_snapshot = world.social_snapshot(ids[1]).expect("师傅快照");
    assert!(mentor_snapshot.is_mentor, "带徒弟的人 is_mentor 要为真");
    assert_eq!(mentor_snapshot.mentor, 0);
    let _ = character;
}

// ---------------------------------------------------------------------------
// 9. 配偶传送（t114）：`/spouse` 命令 + 结婚戒指 `UseItem`
// ---------------------------------------------------------------------------

/// 同图 + 已结婚的一对（返回 [丈夫, 妻子]）。
fn married_couple(world: &mut World) -> Vec<u32> {
    let ids = couple(world, &[20, 20]);
    world.marriage_request(ids[0], ids[1]).expect("求婚");
    world.marriage_reply(ids[1], true).expect("结婚");
    ids
}

#[test]
fn spouse_teleport_moves_next_to_the_spouse_same_map() {
    let mut world = world();
    let ids = married_couple(&mut world);
    // 把配偶挪远一点，确认传送后**挨着**她。
    if let Some(spouse) = world.player_mut(ids[1]) {
        spouse.position = Point::new(110, 110);
    }
    let report = world
        .spouse_teleport(ids[0])
        .expect("已婚且配偶在线，应能传送");
    assert_eq!(report.spouse, ids[1]);
    assert!(!report.map_changed, "同图传送不应标记换图");
    let wife = world.player(ids[1]).unwrap().position;
    let me = world.player(ids[0]).unwrap().position;
    assert!(
        me.in_range(wife, 1),
        "应落在配偶相邻格：我 {me:?} / 配偶 {wife:?}"
    );
    assert_ne!(me, wife, "不要叠在配偶身上");
}

#[test]
fn spouse_teleport_rejects_unmarried_and_offline_spouse() {
    let mut world = world();
    let ids = couple(&mut world, &[20, 20]);
    assert!(matches!(
        world.spouse_teleport(ids[0]),
        Err(SocialError::NotMarried(_))
    ));

    // 结婚后让配偶下线 → 提示「配偶不在线」。
    world.marriage_request(ids[0], ids[1]).unwrap();
    world.marriage_reply(ids[1], true).unwrap();
    world.leave(ids[1]);
    assert!(matches!(
        world.spouse_teleport(ids[0]),
        Err(SocialError::SpouseOffline(_))
    ));
}

#[test]
fn spouse_teleport_crosses_maps_when_the_spouse_is_elsewhere() {
    // 用真实地形：比奇 `0` 与 `0108` 都能加载，跨图传送必须发 `S.MapChanged`。
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = World::new(999);
    world.seed_rng(0x5EED);
    world.set_map_root(root.join("resource/map"));
    world.set_game_data(Arc::new(mir_server::gamedata::GameData::from_db(
        MirDb::default(),
        None,
    )));
    let mut rx = add_player_with_receiver(&mut world, 1, 1, 20, Point::new(100, 100), "0");
    add_player_with_receiver(&mut world, 2, 2, 20, Point::new(101, 100), "0");
    world.marriage_request(1, 2).unwrap();
    world.marriage_reply(2, true).unwrap();
    // 婚后把配偶挪到另一张图（真实地形：比奇 0 → 0108）。
    if let Some(spouse) = world.player_mut(2) {
        spouse.map = "0108".to_owned();
        spouse.position = Point::new(8, 12);
    }
    let _ = drain_packets(&mut rx);
    let report = world.spouse_teleport(1).expect("跨图配偶传送");
    assert!(report.map_changed, "跨图应标记换图");
    assert_eq!(report.map, "0108");
    let packet_list = drain_packets(&mut rx);
    assert!(
        packet_list
            .iter()
            .any(|packet| matches!(packet, ServerPacket::MapChanged(_))),
        "跨图传送要发 S.MapChanged：{packet_list:?}"
    );
    let me = world.player(1).unwrap();
    assert_eq!(me.map, "0108");
    assert!(
        me.position.in_range(Point::new(8, 12), 1),
        "落点应是配偶相邻格"
    );
}

#[test]
fn wedding_ring_use_teleports_to_the_spouse() {
    // 结婚戒指 = 带 `WeddingRing` 的戒指（Crystal `UserItem.WeddingRing`），
    // 本作把「戴戒指」简化成「用它」：`C.UseItem` 触发配偶传送。
    let mut world = world();
    // 自建一件「任何职业都能戴的戒指」模板（真实 DB 里戒指带职业限制，测试不该赌）。
    let mut ring_info = ItemInfo {
        index: 9_001,
        name: "WeddingRing".to_owned(),
        item_type: ItemType::Ring,
        durability: 1_000,
        price: 1_000,
        stack_size: 1,
        ..ItemInfo::default()
    };
    ring_info.required_class = RequiredClass::Warrior;
    let mut db = MirDb::default();
    db.items.push(ring_info);
    world.set_game_data(Arc::new(mir_server::gamedata::GameData::from_db(db, None)));
    let ids = married_couple(&mut world);
    if let Some(spouse) = world.player_mut(ids[1]) {
        spouse.position = Point::new(120, 120);
    }
    let mut ring = UserItem {
        unique_id: 700_001,
        item_index: 9_001,
        current_dura: 1_000,
        max_dura: 1_000,
        count: 1,
        ..UserItem::default()
    };
    ring.wedding_ring = 2; // 配偶的 character_id
    if let Some(player) = world.player_mut(ids[0]) {
        player.inventory[0] = Some(ring.clone());
    }
    let report = world.use_item(ids[0], ring.unique_id).expect("戴戒指传送");
    let (map, position) = report.teleported.expect("应记录传送落点");
    assert_eq!(map, world.player(ids[0]).unwrap().map);
    assert!(position.in_range(Point::new(120, 120), 1));
}
