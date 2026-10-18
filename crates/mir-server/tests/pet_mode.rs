//! t45：宠物命令模式随角色存档（Crystal `CharacterInfo.PMode`，`characters.pet_mode`，迁移 0001）。
//!
//! 运行时模式在 `World.pet_modes`（按主人 object id）；下线时写回角色行 `pet_mode: i16`，
//! 登录时解码写回世界侧表。召回/死亡的重置语义由
//! `tests/skills.rs::pet_mode_resets_on_recall_and_pet_death` 覆盖（本文件验证数值映射与
//! 「下线→重登」往返）。真正的会话级落盘由户部的 `session.rs` 接线 + PG 用例验证。

use std::collections::BTreeSet;
use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{MirClass, MirDirection, MirGender, PetMode};
use mir_db::MirDb;
use mir_server::combat;
use mir_server::gamedata::GameData;
use mir_server::items;
use mir_server::npc::NpcState;
use mir_server::world::{Player, World};
use tokio::sync::mpsc;

const MAP: &str = "0";

fn world() -> World {
    let mut world = World::new(999);
    world.seed_rng(0x5EED);
    world.set_game_data(Arc::new(GameData::from_db(MirDb::default(), None)));
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
fn pet_mode_values_match_crystal_byte_column() {
    assert_eq!(PetMode::Both.value(), 0, "Crystal 默认 PMode = Both");
    assert_eq!(PetMode::MoveOnly.value(), 1);
    assert_eq!(PetMode::AttackOnly.value(), 2);
    assert_eq!(PetMode::None.value(), 3);
    assert_eq!(PetMode::FocusMasterTarget.value(), 4);
    assert_eq!(PetMode::from_value(2), Some(PetMode::AttackOnly));
    assert_eq!(PetMode::from_value(200), None, "越界值由调用方回退 Both");
}

#[test]
fn pet_mode_survives_character_logout_and_login() {
    let mut first = world();
    add_player(&mut first, 1);
    assert_eq!(first.pet_mode(1), PetMode::Both, "默认跟随+攻击");
    assert!(first.set_pet_mode(1, PetMode::AttackOnly));

    // 下线：会话把世界侧模式写进 `Character.pet_mode: i16`。
    let saved: i16 = i16::from(first.pet_mode(1).value());
    assert_eq!(saved, 2);

    // 重登：新世界 + 同一角色行 → `PetMode::from_value(saved as u8)` 写回。
    let mut relogged = world();
    add_player(&mut relogged, 1);
    relogged.set_pet_mode(
        1,
        PetMode::from_value(u8::try_from(saved).unwrap_or(0)).unwrap_or(PetMode::Both),
    );
    assert_eq!(
        relogged.pet_mode(1),
        PetMode::AttackOnly,
        "重登后仍是攻击模式"
    );
}

#[test]
fn old_characters_default_to_both() {
    let mut world = world();
    add_player(&mut world, 1);
    // 旧存档列默认 0 = Both。
    assert_eq!(PetMode::from_value(0), Some(PetMode::Both));
    assert_eq!(world.pet_mode(1), PetMode::Both);
}
