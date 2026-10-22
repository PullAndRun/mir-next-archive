//! `.mnd` 数据包：老格式 → `.mnd` → `GameData` 读回，关键字段逐项一致。
//!
//! M2：DB 段用原生 bincode，Envir 段进内存 FS，不再落临时目录。

mod common;

use std::path::PathBuf;

use mir_db::MirDb;
use mir_db::mnd::{
    LEGACY_FORMAT_VERSION, MndFile, MndSection, SCHEMA_VERSION, TAG_CONFIG, TAG_DB, TAG_DB_BIN,
    TAG_ENVIR, TAG_ENVIR_BUNDLE,
};
use mir_db::schema::{encode_db, encode_envir};
use mir_server::exp::ExpTable;
use mir_server::gamedata::GameData;
use mir_server::guild::GuildSettings;

fn temp_mnd_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "mir-next-mnd-test-{tag}-{}.mnd",
        std::process::id()
    ))
}

fn assert_db_content_eq(left: &MirDb, right: &MirDb) {
    assert_eq!(left.header, right.header, "header");
    assert_eq!(left.maps, right.maps, "maps");
    assert_eq!(left.items, right.items, "items");
    assert_eq!(left.monsters, right.monsters, "monsters");
    assert_eq!(left.npcs, right.npcs, "npcs");
    assert_eq!(left.quests, right.quests, "quests");
    assert_eq!(left.dragon, right.dragon, "dragon");
    assert_eq!(left.magics, right.magics, "magics");
    assert_eq!(left.gameshop, right.gameshop, "gameshop");
    assert_eq!(left.conquests, right.conquests, "conquests");
    assert_eq!(left.respawn, right.respawn, "respawn");
    assert_eq!(left.gt_maps, right.gt_maps, "gt_maps");
}

fn collect_files(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read dir") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out);
        } else if path.is_file() {
            out.push(path);
        }
    }
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
fn m2_native_db_and_memory_fs() {
    let Some(db_path) = common::dataset_db_path("cjlaaa") else {
        eprintln!("skip: cjlaaa Server.MirDB not available under resource/");
        return;
    };
    let source = db_path.parent().expect("dataset dir").to_path_buf();
    let legacy = MirDb::from_bytes(&std::fs::read(&db_path).expect("read db")).expect("parse db");

    let mut envir_files = Vec::new();
    collect_files(&source.join("Envir"), &mut envir_files);
    envir_files.sort();
    assert!(
        envir_files.len() > 1000,
        "cjlaaa Envir 应有一千多份脚本，实际 {}",
        envir_files.len()
    );

    let mut file = MndFile::new();
    assert_eq!(file.format_version, 2);
    assert_eq!(file.schema_version, SCHEMA_VERSION);
    file.push(MndSection::new(
        TAG_DB_BIN,
        "db.bin",
        encode_db(&legacy).expect("encode native db"),
    ));
    let mut relative_paths = Vec::new();
    let mut scripts = Vec::new();
    for path in &envir_files {
        let relative = path
            .strip_prefix(source.join("Envir"))
            .expect("envir prefix")
            .to_string_lossy()
            .replace('\\', "/");
        relative_paths.push(relative.clone());
        scripts.push((relative, std::fs::read(path).expect("read script")));
    }
    file.push(MndSection::new(
        TAG_ENVIR_BUNDLE,
        "envir.bin",
        encode_envir(&scripts).expect("encode envir bundle"),
    ));
    let mut configs = Vec::new();
    for name in ["ExpList.ini", "GuildSettings.ini"] {
        let path = source.join("Configs").join(name);
        let text = std::fs::read_to_string(&path).expect("read config");
        configs.push((name, text.clone()));
        file.push(MndSection::new(
            TAG_CONFIG,
            format!("Configs/{name}"),
            text.into_bytes(),
        ));
    }

    let mnd_path = temp_mnd_path("m2");
    let before = temp_mnd_dirs();
    file.write_to(&mnd_path).expect("write m2 mnd");
    let db_started = std::time::Instant::now();
    let decoded_db = mir_db::schema::decode_db(&file.sections[0].data).expect("decode db");
    let db_ms = db_started.elapsed();
    let envir_started = std::time::Instant::now();
    let decoded_envir = mir_db::schema::decode_envir(&file.sections[1].data).expect("decode envir");
    let envir_ms = envir_started.elapsed();
    assert_eq!(decoded_envir.len(), relative_paths.len());
    let load_started = std::time::Instant::now();
    let loaded = GameData::load_mnd(&mnd_path).expect("load m2 mnd");
    let load_ms = load_started.elapsed();
    let after = temp_mnd_dirs();
    assert_eq!(before, after, "M2 load_mnd 不应创建临时目录");
    assert!(loaded.envir_dir().is_none(), "M2 不应有 envir_dir");
    for (name, text) in &configs {
        assert_eq!(
            loaded.read_config(&format!("Configs/{name}")),
            Some(text.as_str()),
            "Configs/{name} 应从 .mnd 读回"
        );
    }
    let exp_text = loaded
        .read_config("Configs/ExpList.ini")
        .expect(".mnd 里的 ExpList");
    assert_eq!(
        ExpTable::from_ini(exp_text),
        ExpTable::from_ini(&configs[0].1),
        "经验表逐项相等"
    );
    let guild_text = loaded
        .read_config("Configs/GuildSettings.ini")
        .expect(".mnd 里的 GuildSettings");
    assert_eq!(
        GuildSettings::parse_ini(guild_text),
        GuildSettings::parse_ini(&configs[1].1),
        "行会配置逐项相等"
    );
    assert!(loaded.read_config("Configs/Missing.ini").is_none());
    assert_db_content_eq(&decoded_db, &legacy);

    assert_db_content_eq(&loaded.db, &legacy);
    for relative in &relative_paths {
        assert!(
            loaded.read_script(relative).is_some(),
            "内存 FS 应能读 {relative}"
        );
        assert!(
            loaded.read_script(&format!("Envir/{relative}")).is_some(),
            "内存 FS 应能读 Envir/{relative}"
        );
    }

    let npc = relative_paths
        .iter()
        .find(|path| path.starts_with("NPCs/"))
        .expect("至少一份 NPC 脚本");
    let npc_key = npc
        .strip_prefix("NPCs/")
        .expect("npc prefix")
        .trim_end_matches(".txt");
    assert!(loaded.npc_script(npc_key).is_some(), "NPC 脚本应可解析");

    let drop = relative_paths
        .iter()
        .find(|path| path.starts_with("Drops/"))
        .expect("至少一份掉落表");
    let drop_key = drop
        .strip_prefix("Drops/")
        .expect("drop prefix")
        .trim_end_matches(".txt");
    assert!(loaded.drop_table(drop_key).is_some(), "掉落表应可解析");

    let size = std::fs::metadata(&mnd_path).expect("metadata").len();
    println!(
        "M2 game.mnd：{size} B，{} 份脚本，read_script 全部命中；DB 原生段 {} 字节；加载 {load_ms:?}（decode_db {db_ms:?} / decode_envir {envir_ms:?}）",
        relative_paths.len(),
        file.sections[0].data.len()
    );
    let _ = std::fs::remove_file(&mnd_path);
}

fn temp_mnd_dirs() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(std::env::temp_dir())
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
                .filter_map(|entry| {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    name.starts_with("mir-next-mnd-").then_some(name)
                })
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
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
    assert_db_content_eq(&mnd.db, &legacy.db);
    let size = std::fs::metadata(&mnd_path).expect("metadata").len();
    println!(
        "legacy Server.MirDB: {legacy_ms:?}；game.mnd: {mnd_ms:?}（{size} B，M1 只读兼容/内存 FS）"
    );
}
