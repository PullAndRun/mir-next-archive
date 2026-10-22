//! `.mnd` 数据包：老格式 → `.mnd` → `GameData` 读回，关键字段逐项一致。
//!
//! M2：DB 段用原生 bincode，Envir 段进内存 FS，不再落临时目录。

mod common;

use std::path::PathBuf;

use mir_db::MirDb;
use mir_db::mnd::{LEGACY_FORMAT_VERSION, MndFile, MndSection, SCHEMA_VERSION, TAG_DB, TAG_ENVIR};
use mir_server::gamedata::GameData;

fn temp_mnd_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "mir-next-mnd-test-{tag}-{}.mnd",
        std::process::id()
    ))
}

#[test]
fn mnd_roundtrip_matches_legacy_database() {
    let Some(db_path) = common::dataset_db_path("cjlaaa") else {
        eprintln!("skip: cjlaaa Server.MirDB not available under resource/");
        return;
    };
    let source = db_path.parent().expect("dataset dir").to_path_buf();
    let db_bytes = std::fs::read(&db_path).expect("read db");
    let legacy = MirDb::from_bytes(&db_bytes).expect("parse legacy db");

    let mut file = MndFile::new();
    // 显式构造一份 M1 包，验证只读兼容。
    file.format_version = LEGACY_FORMAT_VERSION;
    file.schema_version = 1;
    assert_eq!(SCHEMA_VERSION, 2);
    file.push(MndSection::new(TAG_DB, "db", db_bytes.clone()));

    // 抽两份脚本进包：一份 NPC 脚本、一份掉落表（都从老 Envir 读）。
    let npc_rel = "NPCs/BichonProvince/BorderVillage/0Helper-0.txt";
    let npc_path = source.join("Envir").join(npc_rel);
    if npc_path.is_file() {
        file.push(MndSection::new(
            TAG_ENVIR,
            format!("Envir/{npc_rel}"),
            std::fs::read(&npc_path).expect("read npc script"),
        ));
    }
    let drops_dir = source.join("Envir").join("Drops");
    let mut drop_name = None;
    if let Ok(entries) = std::fs::read_dir(&drops_dir) {
        for entry in entries.flatten() {
            if entry.path().is_file() {
                let name = entry.file_name().to_string_lossy().into_owned();
                file.push(MndSection::new(
                    TAG_ENVIR,
                    format!("Envir/Drops/{name}"),
                    std::fs::read(entry.path()).expect("read drop table"),
                ));
                drop_name = Some(name.trim_end_matches(".txt").to_owned());
                break;
            }
        }
    }

    let mnd_path = temp_mnd_path("roundtrip");
    file.write_to(&mnd_path).expect("write mnd");

    let loaded = GameData::load_mnd(&mnd_path).expect("load mnd");
    assert_eq!(
        loaded.path().extension().and_then(|ext| ext.to_str()),
        Some("mnd"),
        "GameData 路径应指向 .mnd"
    );
    // 逐段相等（MirDb 全量 PartialEq）。
    assert_eq!(loaded.db, legacy, "读回的 DB 必须与老库逐字段相等");

    if npc_path.is_file() {
        assert!(
            loaded
                .npc_script("BichonProvince/BorderVillage/0Helper-0")
                .is_some(),
            "NPC 脚本应从 .mnd 解包后可用"
        );
    }
    if let Some(name) = drop_name {
        assert!(
            loaded.drop_table(&name).is_some(),
            "掉落表 {name} 应从 .mnd 解包后可用"
        );
    }

    let _ = std::fs::remove_file(&mnd_path);
}

#[test]
fn mnd_sizes_and_section_counts_are_reported() {
    let Some(db_path) = common::dataset_db_path("cjlaaa") else {
        eprintln!("skip: cjlaaa Server.MirDB not available under resource/");
        return;
    };
    let db_bytes = std::fs::read(&db_path).expect("read db");
    let mut file = MndFile::new();
    file.push(MndSection::new(TAG_DB, "db", db_bytes));
    let bytes = file.to_bytes().expect("encode");
    let back = MndFile::from_bytes(&bytes).expect("decode");
    assert_eq!(back.sections.len(), 1);
    assert_eq!(back.get("db").expect("db").tag, TAG_DB);
}

/// 报告用的加载耗时对比（旧 `Server.MirDB` vs `.mnd` 数据包）。
///
/// 依赖已经导入好的 `game-assets/data/game.mnd`；没导入时跳过，不影响门禁。
#[test]
fn reports_legacy_vs_mnd_load_time() {
    let mnd_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../game-assets/data/game.mnd");
    let Some(db_path) = common::dataset_db_path("cjlaaa") else {
        eprintln!("skip: cjlaaa Server.MirDB not available under resource/");
        return;
    };
    if !mnd_path.is_file() {
        eprintln!("skip: game-assets/data/game.mnd 未导入");
        return;
    }
    let legacy_started = std::time::Instant::now();
    let legacy = GameData::load(&db_path).expect("legacy load");
    let legacy_ms = legacy_started.elapsed();
    let mnd_started = std::time::Instant::now();
    let mnd = GameData::load_mnd(&mnd_path).expect("mnd load");
    let mnd_ms = mnd_started.elapsed();
    assert_eq!(mnd.db, legacy.db);
    let size = std::fs::metadata(&mnd_path).expect("metadata").len();
    println!(
        "legacy Server.MirDB: {legacy_ms:?}；game.mnd: {mnd_ms:?}（{size} B，含 1139 份脚本解包）"
    );
}
