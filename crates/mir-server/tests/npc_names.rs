//! NPC 名字按会话语言渲染（`config/lang/*.json` 的 `npc_names` 段）。
//!
//! 中文会话显示语言包里的规范化中文名，英文会话回退 DB 原名；未知 NPC 回退原名不 panic。
//! 覆盖 `S.ObjectNPC`（`World::npc_object_packet`）与 `S.NewMapInfo`（`map_entry_info_with_language`）。

mod common;

use std::sync::Arc;

use mir_core::Language;
use mir_core::i18n::LanguageLibrary;
use mir_server::World;
use mir_world::{Map, MapCell, MapVersion};

use common::Harness;

fn blank_map(name: &str, size: i32) -> Arc<Map> {
    let cells = usize::try_from(size * size).expect("map size");
    Map {
        width: size,
        height: size,
        cells: vec![MapCell::default(); cells],
        file_name: name.to_owned(),
        version: MapVersion::V0,
        doors: Vec::new(),
    }
    .into()
}

fn world_with_default_data() -> Option<(World, mir_server::Npc)> {
    let data = common::default_data()?;
    let mut harness = Harness::new(7);
    harness.world.set_game_data(data);
    harness
        .world
        .set_languages(Arc::new(LanguageLibrary::embedded()));
    harness.world.insert_map("0", blank_map("0", 700));
    harness.world.spawn_npcs("0");
    let jane = harness
        .world
        .npcs()
        .iter()
        .find(|npc| npc.name == "Assistant_Jane")
        .cloned()?;
    Some((harness.world, jane))
}

#[test]
fn chinese_session_shows_localized_npc_name_in_object_and_map_info() {
    let Some((mut world, jane)) = world_with_default_data() else {
        eprintln!("skip: cjlaaa Server.MirDB not available under resource/");
        return;
    };
    world.set_player_language(1, Language::ZhCn);

    let packet = world.npc_object_packet(1, &jane);
    assert_eq!(packet.name, "助理·简", "S.ObjectNPC 的中文名");

    let info = world
        .map_entry_info_with_language("0", "比奇省", Some(Language::ZhCn))
        .expect("进图资料");
    let jane_info = info
        .npcs
        .iter()
        .find(|npc| npc.index == jane.index)
        .expect("NewMapInfo 应包含 Jane");
    assert_eq!(jane_info.name, "助理·简", "S.NewMapInfo 的中文名");
}

#[test]
fn english_session_keeps_original_npc_name() {
    let Some((mut world, jane)) = world_with_default_data() else {
        eprintln!("skip: cjlaaa Server.MirDB not available under resource/");
        return;
    };
    world.set_player_language(2, Language::En);

    let packet = world.npc_object_packet(2, &jane);
    assert_eq!(packet.name, "Assistant_Jane", "英文会话保持 DB 原名");

    let info = world
        .map_entry_info_with_language("0", "BichonProvince", Some(Language::En))
        .expect("进图资料");
    let jane_info = info
        .npcs
        .iter()
        .find(|npc| npc.index == jane.index)
        .expect("NewMapInfo 应包含 Jane");
    assert_eq!(jane_info.name, "Assistant_Jane");
}

#[test]
fn unknown_npc_falls_back_to_database_name_without_panicking() {
    let Some((mut world, mut npc)) = world_with_default_data() else {
        eprintln!("skip: cjlaaa Server.MirDB not available under resource/");
        return;
    };
    world.set_player_language(3, Language::ZhCn);
    npc.name = "Unknown_NPC".to_owned();
    npc.file_name = "Unknown/NotInLanguagePack".to_owned();

    let packet = world.npc_object_packet(3, &npc);
    assert_eq!(packet.name, "Unknown_NPC", "未知 NPC 回退 DB 原名");
}
