//! 临时审计（跑完即删）：地图兼容性与 cjlaaa 英文残留样本。

use std::path::Path;

use mir_db::MirDb;

fn has_cjk(text: &str) -> bool {
    text.chars()
        .any(|ch| ('\u{4E00}'..='\u{9FFF}').contains(&ch))
}

#[test]
fn audit_maps_and_leftovers() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");

    for (label, path) in [
        ("resource/map/0.map", root.join("resource/map/0.map")),
        (
            "cjlaaa/Maps/0.map",
            root.join("resource/community/suprcode-crystal-database/cjlaaa/Maps/0.map"),
        ),
        (
            "Jev/Maps/0.map",
            root.join("resource/community/suprcode-crystal-database/Jev/Maps/0.map"),
        ),
    ] {
        match mir_world::Map::load(&path) {
            Ok(map) => println!(
                "{label}: {}x{} walkable={}",
                map.width,
                map.height,
                map.walkable_cells().count()
            ),
            Err(error) => println!("{label}: load failed: {error}"),
        }
    }

    for name in ["0103", "0104", "D001A", "bonguk1"] {
        let path = root
            .join("resource/community/suprcode-crystal-database/cjlaaa/Maps")
            .join(format!("{name}.map"));
        match mir_world::Map::load(&path) {
            Ok(map) => println!("cjlaaa {name}.map: {}x{} ok", map.width, map.height),
            Err(error) => println!("cjlaaa {name}.map: FAILED {error}"),
        }
    }

    let cj = MirDb::load(root.join(
        "resource/community/suprcode-crystal-database/cjlaaa/Server.MirDB",
    ))
    .expect("cjlaaa");
    let jev = MirDb::load(root.join(
        "resource/community/suprcode-crystal-database/Jev/Server.MirDB",
    ))
    .expect("Jev");

    for (label, db) in [("cjlaaa", &cj), ("Jev", &jev)] {
        let (mut cjk_items, mut total_items) = (0, 0);
        let mut english_items = Vec::new();
        for item in &db.items {
            total_items += 1;
            if has_cjk(&item.name) {
                cjk_items += 1;
            } else if english_items.len() < 12 {
                english_items.push(item.name.as_str());
            }
        }
        let (mut cjk_mon, mut total_mon) = (0, 0);
        let mut english_mon = Vec::new();
        for mon in &db.monsters {
            total_mon += 1;
            if has_cjk(&mon.name) {
                cjk_mon += 1;
            } else if english_mon.len() < 12 {
                english_mon.push(mon.name.as_str());
            }
        }
        let cjk_magic = db
            .magics
            .iter()
            .filter(|m| has_cjk(&m.name))
            .count();
        println!(
            "{label}: items cjk={cjk_items}/{total_items}; monsters cjk={cjk_mon}/{total_mon}; magics cjk={cjk_magic}/{}, sample magic={:?}",
            db.magics.len(),
            db.magics
                .iter()
                .take(4)
                .map(|m| m.name.as_str())
                .collect::<Vec<_>>()
        );
        println!("  english item sample: {english_items:?}");
        println!("  english monster sample: {english_mon:?}");
    }
}
