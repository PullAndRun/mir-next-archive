//! 登出/重登装备落盘回归：snapshot 必须带 equipment，store 立即读回仍是同一块。

mod common;

use mir_core::enums::{MirClass, MirGender, MirGridType, PetMode};
use mir_server::AccountStore;
use mir_server::items::encode_slots;

#[test]
fn character_snapshot_copies_equipped_weapon() {
    let Some(data) = common::default_data() else {
        return;
    };
    let mut harness = common::Harness::new(0xE0_01);
    harness.world.set_game_data(data);
    let player = harness.add_player(1, "装备测试", 1, 0);
    let weapon = harness
        .world
        .starter_items_for(MirClass::Warrior)
        .into_iter()
        .find(|item| {
            harness
                .world
                .item_info(item.item_index)
                .is_some_and(|info| info.item_type == mir_core::enums::ItemType::Weapon)
        })
        .expect("cjlaaa 战士出生装里必须有武器");
    harness.world.player_mut(player).unwrap().inventory[0] = Some(weapon.clone());
    harness
        .world
        .equip_item(player, weapon.unique_id, MirGridType::Inventory, 0)
        .expect("应能装备武器");
    let player_ref = harness.world.player(player).unwrap();
    assert!(player_ref.equipment[0].is_some(), "武器必须在装备槽");

    let base = mir_server::Character::new(
        1,
        1,
        "装备测试",
        MirClass::Warrior,
        MirGender::Male,
        "0",
        (4, 9),
    );
    let snapshot = mir_server::market::character_snapshot(&base, player_ref, PetMode::Both);
    assert_eq!(
        snapshot.equipment,
        encode_slots(&player_ref.equipment),
        "snapshot.equipment 必须来自 player.equipment"
    );
    assert!(!snapshot.equipment.is_empty());
}

#[tokio::test]
async fn equipment_survives_immediate_store_roundtrip() {
    let Some(data) = common::default_data() else {
        return;
    };
    let mut harness = common::Harness::new(0xE0_02);
    harness.world.set_game_data(data);
    let player = harness.add_player(1, "装备测试", 1, 0);
    let weapon = harness
        .world
        .starter_items_for(MirClass::Warrior)
        .into_iter()
        .find(|item| {
            harness
                .world
                .item_info(item.item_index)
                .is_some_and(|info| info.item_type == mir_core::enums::ItemType::Weapon)
        })
        .expect("cjlaaa 战士出生装里必须有武器");
    harness.world.player_mut(player).unwrap().inventory[0] = Some(weapon.clone());
    harness
        .world
        .equip_item(player, weapon.unique_id, MirGridType::Inventory, 0)
        .expect("应能装备武器");
    let player_ref = harness.world.player(player).unwrap();
    let base = mir_server::Character::new(
        1,
        1,
        "装备测试",
        MirClass::Warrior,
        MirGender::Male,
        "0",
        (4, 9),
    );
    let snapshot = mir_server::market::character_snapshot(&base, player_ref, PetMode::Both);
    let equipment = snapshot.equipment.clone();

    let store = mir_server::MemoryStore::new();
    let account = store
        .create_account("equipuser", "secret", "")
        .await
        .expect("建账号");
    let mut character = store
        .create_character(
            account.id,
            "装备测试",
            MirClass::Warrior,
            MirGender::Male,
            "0",
            (4, 9),
        )
        .await
        .expect("建角色");
    character.equipment = equipment.clone();
    store.save_character(character.clone()).await.expect("落盘");
    let loaded = store
        .find_character(account.id, character.id)
        .await
        .expect("读库")
        .expect("角色存在");
    assert_eq!(loaded.equipment, equipment, "立即读回装备块必须一致");
}
