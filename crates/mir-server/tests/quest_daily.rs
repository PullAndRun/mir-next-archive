//! t34：每日任务惰性重置（Crystal `Envir.ProcessNewDay` → `ClearDailyQuests`）。
//!
//! Crystal 在服务器**跨天（0 点）**时对所有角色（含离线）清 `QuestType.Daily` 的完成记录
//! （`Envir.cs:2311-2315`、`:4856-4866`、`:5009-5024`）。本作没有服务器定时器，改为按角色的
//! `last_daily_reset_at`（迁移 0013）在登录/任务交互时**惰性**补做，间隔 24h——
//! 这是明确差异：不是 0 点，而是「距上次 ≥24h」。

use std::collections::BTreeSet;
use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{MirClass, MirDirection, MirGender};
use mir_db::{MirDb, QuestInfo};
use mir_server::combat;
use mir_server::gamedata::GameData;
use mir_server::items;
use mir_server::npc::NpcState;
use mir_server::quests::{
    self, QuestProgress, QuestProgressState, QuestSave, decode_quests, encode_quests,
};
use mir_server::world::{Player, World};
use tokio::sync::mpsc;

const MAP: &str = "0";
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

fn quest(index: i32, name: &str, quest_type: u8) -> QuestInfo {
    QuestInfo {
        index,
        name: name.to_owned(),
        group: "测试".to_owned(),
        file_name: "Test/1".to_owned(),
        quest_type,
        ..QuestInfo::default()
    }
}

/// 最小世界：任务 1（普通）与 10（每日）。
fn world() -> World {
    let db = MirDb {
        quests: vec![quest(1, "普通", 0), quest(10, "每日", 1)],
        ..MirDb::default()
    };
    let mut world = World::new(999);
    world.seed_rng(0x5EED);
    world.set_game_data(Arc::new(GameData::from_db(db, None)));
    world
}

fn add_player(world: &mut World, object_id: u32) {
    let stats = combat::player_base_stats(10, MirClass::Warrior);
    let (sender, _receiver) = mpsc::unbounded_channel();
    world.enter(Player {
        object_id,
        session_id: u64::from(object_id),
        character_id: i64::from(object_id),
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
        visible_monsters: BTreeSet::new(),
        visible_npcs: BTreeSet::new(),
        movement_time: 0,
        inventory: items::new_inventory(),
        equipment: items::new_equipment(),
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
}

#[test]
fn daily_reset_due_uses_a_24h_window() {
    let now = 1_700_000_000_000_i64;
    assert!(
        !quests::daily_reset_due(now, now + DAY_MS - 60_000),
        "23:59 不重置"
    );
    assert!(
        quests::daily_reset_due(now, now + DAY_MS + 60_000),
        "24:01 重置"
    );
    assert!(
        quests::daily_reset_due(0, now),
        "从未重置过（新角色/旧存档）→ 到期（重置幂等）"
    );
}

#[test]
fn apply_daily_reset_rewrites_the_login_record() {
    let world = world();
    let mut character = mir_server::Character::new(
        1,
        1,
        "玩家",
        MirClass::Warrior,
        MirGender::Male,
        MAP,
        (100, 100),
    );
    let completed_daily = QuestProgress {
        quest_index: 10,
        state: QuestProgressState::Completed,
        ..QuestProgress::default()
    };
    let completed_normal = QuestProgress {
        quest_index: 1,
        state: QuestProgressState::Completed,
        ..QuestProgress::default()
    };
    character.quests = encode_quests(&QuestSave {
        quests: vec![completed_daily, completed_normal],
        completed_quests: BTreeSet::from([1, 10]),
    });

    let now = 1_700_000_000_000_i64;
    assert!(
        world.apply_daily_reset(&mut character, now),
        "last=0（旧存档）→ 登录时补做"
    );
    assert_eq!(character.last_daily_reset_at, now);
    let save = decode_quests(&character.quests).expect("每日重置后的任务块应当可解码");
    assert_eq!(
        save.completed_quests,
        BTreeSet::from([1]),
        "只清每日任务（10）的完成记录，普通任务（1）保留"
    );
    assert!(
        save.quests
            .iter()
            .all(|progress| progress.quest_index != 10),
        "每日任务的完成态进度条目也一并清掉（否则再接会 AlreadyCompleted）"
    );

    assert!(
        !world.apply_daily_reset(&mut character, now + DAY_MS - 60_000),
        "23:59 内不重复重置"
    );
    assert!(
        world.apply_daily_reset(&mut character, now + DAY_MS + 60_000),
        "跨过 24:01 再重置一次"
    );
}

#[test]
fn offline_cross_day_login_resets_daily_quests() {
    let world = world();
    let mut character = mir_server::Character::new(
        1,
        1,
        "玩家",
        MirClass::Warrior,
        MirGender::Male,
        MAP,
        (100, 100),
    );
    character.quests = encode_quests(&QuestSave {
        quests: vec![QuestProgress {
            quest_index: 10,
            state: QuestProgressState::Completed,
            ..QuestProgress::default()
        }],
        completed_quests: BTreeSet::from([10]),
    });
    let now = 1_700_000_000_000_i64;
    character.last_daily_reset_at = now - 25 * 60 * 60 * 1000;
    assert!(
        world.apply_daily_reset(&mut character, now),
        "离线跨天（25h）后登录应当重置"
    );
    let save = decode_quests(&character.quests).expect("解码");
    assert!(save.completed_quests.is_empty());
    assert!(save.quests.is_empty());
}

#[test]
fn reset_daily_quests_clears_player_daily_but_keeps_in_progress() {
    let mut world = world();
    add_player(&mut world, 1);
    {
        let player = world.player_mut(1).expect("玩家在世界里");
        player.completed_quests.extend([1, 10]);
        player.quests.push(QuestProgress {
            quest_index: 10,
            state: QuestProgressState::Completed,
            ..QuestProgress::default()
        });
        player.quests.push(QuestProgress {
            quest_index: 10,
            state: QuestProgressState::InProgress,
            ..QuestProgress::default()
        });
    }
    assert_eq!(world.reset_daily_quests(1), 1);
    let player = world.player(1).expect("玩家在世界里");
    assert!(player.completed_quests.contains(&1), "普通任务保留");
    assert!(!player.completed_quests.contains(&10), "每日任务被清");
    assert!(
        player
            .quests
            .iter()
            .any(|progress| progress.quest_index == 10
                && progress.state == QuestProgressState::InProgress),
        "进行中的每日任务不打断"
    );
    assert!(
        !player
            .quests
            .iter()
            .any(|progress| progress.quest_index == 10
                && progress.state == QuestProgressState::Completed),
        "完成态进度条目被清"
    );
}
