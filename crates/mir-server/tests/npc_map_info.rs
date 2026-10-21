//! `S.NewMapInfo` 的 NPC/movements 过滤与「数据缺 bigmap flags」回退（`npc.rs::map_entry_info`）。
//!
//! 背景：默认数据集 cjlaaa v105 全库 `ShowOnBigMap = false`（数据事实，见
//! `docs/world/data-languages.md`），严格按 Crystal 过滤会让客户端收到 0 个 NPC / 0 条传送点；
//! 服务端因此按图回退为全量。Jev v111 有 flags，输出必须保持 Crystal 语义不变。

mod common;

use std::path::PathBuf;
use std::sync::Arc;

use mir_core::Point;
use mir_db::{MapInfo, MirDb, MovementInfo, NpcInfo};
use mir_server::gamedata::GameData;
use mir_world::{Map, MapCell, MapVersion};

use common::Harness;

/// 空地图（默认格子可走），尺寸按数据集需要给足。
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

fn temp_envir(tag: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("mir-next-npc-mapinfo-{tag}-{}", std::process::id()));
    let drops = root.join("Envir").join("Drops");
    std::fs::create_dir_all(&drops).expect("create temp envir");
    drops
}

#[test]
fn new_map_info_falls_back_to_all_when_bigmap_flags_missing() {
    // 两个 NPC + 两条 movements，全部 show_on_big_map = false（模拟 cjlaaa v105）。
    let db = MirDb {
        maps: vec![
            MapInfo {
                index: 1,
                file_name: "0".to_owned(),
                title: "测试省".to_owned(),
                movements: vec![
                    MovementInfo {
                        map_index: 2,
                        source: Point::new(10, 10),
                        destination: Point::new(5, 5),
                        show_on_big_map: false,
                        ..MovementInfo::default()
                    },
                    MovementInfo {
                        map_index: 2,
                        source: Point::new(11, 10),
                        destination: Point::new(6, 5),
                        show_on_big_map: false,
                        ..MovementInfo::default()
                    },
                ],
                ..MapInfo::default()
            },
            MapInfo {
                index: 2,
                file_name: "1".to_owned(),
                title: "目标图".to_owned(),
                ..MapInfo::default()
            },
        ],
        npcs: vec![
            NpcInfo {
                index: 1,
                map_index: 1,
                file_name: "Test/Shop".to_owned(),
                name: "测试商人".to_owned(),
                location: Point::new(20, 20),
                show_on_big_map: false,
                ..NpcInfo::default()
            },
            NpcInfo {
                index: 2,
                map_index: 1,
                file_name: "Test/Shop".to_owned(),
                name: "测试铁匠".to_owned(),
                location: Point::new(21, 20),
                show_on_big_map: false,
                ..NpcInfo::default()
            },
        ],
        ..MirDb::default()
    };
    let data = Arc::new(GameData::from_db(db, Some(temp_envir("fallback"))));
    let mut harness = Harness::new(1);
    harness.world.set_game_data(Arc::clone(&data));
    harness.world.insert_map("0", blank_map("0", 60));
    let spawned = harness.world.spawn_npcs("0");
    assert_eq!(spawned.len(), 2, "两个 NPC 都应刷出");

    let info = harness
        .world
        .map_entry_info("0", "测试省")
        .expect("应有进图资料");
    assert_eq!(
        info.npcs.len(),
        2,
        "flags 全 false 时应回退为全部 NPC，实际 {}",
        info.npcs.len()
    );
    assert_eq!(
        info.movements.len(),
        2,
        "flags 全 false 时应回退为全部 movements，实际 {}",
        info.movements.len()
    );
    assert!(
        info.movements
            .iter()
            .all(|movement| movement.title == "目标图"),
        "回退的 movements 仍应解析目的地标题"
    );
}

#[test]
fn new_map_info_keeps_crystal_filter_when_flags_present() {
    // Jev v111 真实数据：map0 有 bigmap flags → 必须保持 Crystal 过滤结果 40 NPC / 6 movements。
    let Some(data) = common::jev_data() else {
        eprintln!("skip: Jev Server.MirDB not available under resource/");
        return;
    };
    let mut harness = Harness::new(2);
    harness.world.set_game_data(Arc::clone(&data));
    harness.world.insert_map("0", blank_map("0", 700));
    let spawned = harness.world.spawn_npcs("0");
    assert_eq!(spawned.len(), 46, "Jev map0 应有 46 个 NPC");

    let info = harness
        .world
        .map_entry_info("0", "BichonProvince")
        .expect("应有进图资料");
    assert_eq!(
        info.npcs.len(),
        40,
        "Jev 有 bigmap flags，NPC 应保持 Crystal 过滤结果"
    );
    assert_eq!(
        info.movements.len(),
        6,
        "Jev 有 bigmap flags，movements 应保持 Crystal 过滤结果"
    );
}
