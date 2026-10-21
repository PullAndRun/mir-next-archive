//! `S.NewItemInfo` 下发：登录模板补齐、按连接去重、语言包名字。
//!
//! 背景：`UserItem`/`NPCGoods`/`GainedItem` 只有 `item_index`，客户端必须收到模板
//! 才能显示名字/图标/价格。Crystal 在 `MirConnection.CheckItemInfo`（`:2274-2290`）
//! 里按连接去重下发 `S.NewItemInfo`。

mod common;

use std::sync::Arc;

use mir_core::Language;
use mir_core::i18n::LanguageLibrary;
use mir_protocol::ServerPacket;

use common::Harness;

fn test_harness() -> Option<Harness> {
    let data = common::default_data()?;
    let mut harness = Harness::new(1);
    harness.world.set_game_data(data);
    harness
        .world
        .set_languages(Arc::new(LanguageLibrary::embedded()));
    Some(harness)
}

#[test]
fn login_item_info_packets_cover_unique_inventory_and_equipment_indices() {
    let Some(mut harness) = test_harness() else {
        eprintln!("skip: cjlaaa Server.MirDB not available under resource/");
        return;
    };
    let player_id = harness.add_player(1, "tester", 10, 0);

    // 模拟登录时背包 + 装备的 unique item_index（含重复与非法 0）。
    let packets = harness
        .world
        .item_info_packets_for_indices(player_id, [221, 317, 221, 0]);
    assert_eq!(packets.len(), 2, "重复/非法 index 只应下发一次");
    for packet in &packets {
        let ServerPacket::NewItemInfo(packet) = packet else {
            panic!("只应生成 S.NewItemInfo，实际 {packet:?}");
        };
        assert!(!packet.info.name.is_empty(), "名字不能为空");
        let template = harness
            .world
            .item_info(packet.info.index)
            .expect("模板应存在");
        assert_eq!(packet.info.image, template.image, "图标号必须与模板一致");
        assert_eq!(packet.info.price, template.price, "价格必须与模板一致");
    }

    // 再收集一次同一批 index：已发过 → 不再生成。
    assert!(
        harness
            .world
            .item_info_packets_for_indices(player_id, [221, 317])
            .is_empty(),
        "重复收集不应重复下发"
    );
}

#[test]
fn item_name_is_localized_per_session_language() {
    let Some(mut harness) = test_harness() else {
        eprintln!("skip: cjlaaa Server.MirDB not available under resource/");
        return;
    };
    let player_id = harness.add_player(1, "tester", 10, 0);

    harness.world.set_player_language(player_id, Language::ZhCn);
    let zh = harness
        .world
        .item_info_packets_for_indices(player_id, [1])
        .pop()
        .expect("中文模板");
    let ServerPacket::NewItemInfo(zh) = zh else {
        panic!("应为 NewItemInfo");
    };
    assert_eq!(zh.info.name, "祈祷之剑");

    // 另一个会话（独立的已发集合）用英文，验证同一 index 的名字不同。
    let Some(mut harness2) = test_harness() else {
        eprintln!("skip: cjlaaa Server.MirDB not available under resource/");
        return;
    };
    let player_id = harness2.add_player(2, "tester-en", 10, 0);
    harness2.world.set_player_language(player_id, Language::En);
    let en = harness2
        .world
        .item_info_packets_for_indices(player_id, [1])
        .pop()
        .expect("英文模板");
    let ServerPacket::NewItemInfo(en) = en else {
        panic!("应为 NewItemInfo");
    };
    assert_eq!(en.info.name, "SpiritBlade");
    assert_ne!(zh.info.name, en.info.name);
}

#[test]
fn send_item_info_dedups_and_ignores_unknown_templates() {
    let Some(mut harness) = test_harness() else {
        eprintln!("skip: cjlaaa Server.MirDB not available under resource/");
        return;
    };
    let player_id = harness.add_player(1, "tester", 10, 0);

    assert!(harness.world.send_item_info(player_id, 100), "首次应发送");
    assert!(
        !harness.world.send_item_info(player_id, 100),
        "同一模板第二次不应重复发送"
    );
    assert!(
        !harness.world.send_item_info(player_id, 999_999),
        "未知模板不应发送、也不应 panic"
    );
}
