//! 原生玩家库键的加载回归：同一个 `native-one` 键分别从拆包目录与小包读取。
//!
//! 测试只造 1×1 PNG，不依赖 `game-assets`；键名故意不含旧 `file_stem` 的 `00` 形态，
//! 防止实现又退回“按文件名/目录名猜库”。

use std::path::{Path, PathBuf};

use mir_asset::{ImageInfo, LibraryMap, PackSet, PackWriter};
use mir_core::enums::{MirClass, MirDirection, MirGender};
use mir_render::EntityLibraries;
use mir_render::player_layout::{
    NativeAction, NativeFrameTable, NativeLayer, NativeLibrary, NativeLibraryId, frame_index,
};

fn temp_dir(tag: &str) -> PathBuf {
    let base = std::env::var("CARGO_TARGET_TMPDIR")
        .map_or_else(|_| PathBuf::from("target/tmp-native-tests"), PathBuf::from);
    let dir = base.join(format!(
        "mir2-native-library-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn write_png(path: &Path) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create png dir");
    }
    let mut frame = mir_render::Frame::new(1, 1);
    frame.clear([255, 0, 0, 255]);
    frame.save_png(path).expect("save png");
}

fn write_index(dir: &Path, lib: &str) {
    std::fs::create_dir_all(dir).expect("create index dir");
    let index = serde_json::json!({
        "lib": lib,
        "images": [{
            "index": 0,
            "png": "0000.png",
            "width": 1,
            "height": 1,
            "offset_x": 0,
            "offset_y": 0,
            "format_code": 255,
            "empty": false
        }]
    });
    std::fs::write(
        dir.join("index.json"),
        serde_json::to_string_pretty(&index).expect("index json"),
    )
    .expect("write index.json");
}

#[test]
fn native_library_loads_by_stable_key_from_unpacked_and_pack() {
    let root = temp_dir("sources");
    let unpacked_gfx = root.join("gfx/sprites/player-native/native-one");
    write_png(&unpacked_gfx.join("0000.png"));
    write_index(&unpacked_gfx, "native-one.Lib");
    let unpacked_map = serde_json::json!({
        "version": 1,
        "libraries": {
            "native-one": {
                "lib": "native-one.Lib",
                "dirs": ["gfx/sprites/player-native/native-one"],
                "prefix": "gfx/sprites/player-native/native-one/"
            }
        }
    });
    std::fs::write(
        root.join("library-map.json"),
        serde_json::to_string(&unpacked_map).expect("map json"),
    )
    .expect("write map");
    let map = LibraryMap::load(root.join("library-map.json")).expect("parse map");
    let empty_data = root.join("data");
    std::fs::create_dir_all(&empty_data).expect("data dir");
    let gfx = root.join("gfx");
    let mut libraries = EntityLibraries::open_with_sources(
        &empty_data,
        Some(&gfx),
        Some(&root),
        Some(&map),
        None,
        false,
    );
    let source = NativeLibrary::open(&mut libraries, "native-one").expect("unpacked library");
    assert!(source.is_unpacked_dirs(), "散装库应走 UnpackedDirs 变体");
    assert_eq!(source.len(), 1);

    // 小包：同一逻辑键，但只有 `.mnp`，不走 gfx 目录。
    let packs_dir = root.join("packs");
    std::fs::create_dir_all(&packs_dir).expect("packs dir");
    let pack_path = packs_dir.join("sprites-000.mnp");
    let mut writer = PackWriter::create(&pack_path, false).expect("pack writer");
    writer
        .add_image(
            "gfx/sprites/player-native/native-one/0000.png",
            &ImageInfo {
                width: 1,
                height: 1,
                format: 0xFF,
                ..ImageInfo::default()
            },
            &[255, 0, 0, 255],
        )
        .expect("add image");
    let summary = writer.finish("sprites").expect("finish pack");
    let manifest = serde_json::json!({
        "version": 1,
        "generated_at": "test",
        "chunk_mb": 16,
        "generator": "test",
        "deterministic": true,
        "packs": [summary]
    });
    std::fs::write(
        packs_dir.join("manifest.packs.json"),
        serde_json::to_string_pretty(&manifest).expect("packs manifest"),
    )
    .expect("write packs manifest");
    let pack_map = serde_json::json!({
        "version": 1,
        "libraries": {
            "native-one": {
                "lib": "native-one.Lib",
                "dirs": [],
                "prefix": "gfx/sprites/player-native/native-one/"
            }
        }
    });
    let pack_map_path = root.join("packs/library-map.json");
    std::fs::write(
        &pack_map_path,
        serde_json::to_string(&pack_map).expect("pack map"),
    )
    .expect("write pack map");
    let pack_map = LibraryMap::load(&pack_map_path).expect("parse pack map");
    let packs = PackSet::open_unverified(&packs_dir).expect("open pack");
    let mut release_libraries = EntityLibraries::open_with_sources(
        root.join("no-gfx-data"),
        None,
        Some(&root),
        Some(&pack_map),
        Some(&packs),
        false,
    );
    let source = NativeLibrary::open(&mut release_libraries, "native-one").expect("pack library");
    assert!(source.is_pack(), "只有小包时应走 Pack 变体");
    assert_eq!(source.len(), 1);

    std::fs::remove_dir_all(root).ok();
}

#[test]
#[ignore = "需要先生成 game-assets 原生玩家拆包（R48 A2）"]
fn classic_a2_alt_and_plain_offsets_hit_real_armour_frames() {
    let workspace = workspace_root();
    let root = workspace.join("game-assets");
    let map = LibraryMap::load(root.join("library-map.json")).expect("dev library map");
    let mut libraries = EntityLibraries::open_with_sources(
        root.join("data"),
        Some(&root.join("gfx")),
        Some(&root),
        Some(&map),
        None,
        false,
    );

    // 刺客/弓手的身体域随 altAnim：alt 才用 A/AR，否则回 C 系。
    // Attack2 起始帧 = 184 + 方向*6 + armour offset。
    let cases = [
        (MirClass::Assassin, true, "aarmour-00", 696u32),
        (MirClass::Assassin, false, "carmour-00", 992u32),
        (MirClass::Archer, true, "ararmour-00", 536u32),
        (MirClass::Archer, false, "carmour-00", 992u32),
    ];
    for (class, alt_anim, expected_key, expected_frame) in cases {
        let id = NativeLibraryId::new(class, MirGender::Female, alt_anim);
        let key = id.library_key(NativeLayer::Armour, 0).expect("armour key");
        assert_eq!(key, expected_key, "{class:?} alt_anim={alt_anim}");
        let index = frame_index(
            NativeFrameTable::ATTACK2,
            MirDirection::Up,
            0,
            id.armour_offset(),
        );
        assert_eq!(index, expected_frame, "{class:?} alt_anim={alt_anim}");
        let frame = usize::try_from(index).expect("frame index fits usize");
        let source = NativeLibrary::open(&mut libraries, &key).expect("real armour source");
        let info = source.info(frame).expect("Attack2 frame info");
        assert!(!info.is_empty(), "{key} 的 Attack2 起始帧不应为空");
        let image = source.image(frame).expect("decode Attack2 frame");
        assert_eq!(
            image.pixels.len(),
            usize::from(image.info.width) * usize::from(image.info.height) * 4
        );
    }

    // 武器：刺客 alt 才用 AWeaponsR/L，弓手 alt 才用 ARWeaponsS；普通域回落 CWeapons。
    let a = NativeLibraryId::new(MirClass::Assassin, MirGender::Female, true);
    assert_eq!(
        a.library_key(
            NativeLayer::Weapon(mir_render::player_layout::NativeHand::Main),
            100
        )
        .as_deref(),
        Some("aweapon-00-r")
    );
    let ar = NativeLibraryId::new(MirClass::Archer, MirGender::Female, true);
    let ar_main = ar.library_key(
        NativeLayer::Weapon(mir_render::player_layout::NativeHand::Main),
        200,
    );
    assert_eq!(ar_main, None, "弓手 WeaponLibrary1 必须为空");
    assert_eq!(
        ar.library_key(
            NativeLayer::Weapon(mir_render::player_layout::NativeHand::Off),
            200
        )
        .as_deref(),
        Some("arweapon-00-s")
    );
    let ar_plain = NativeLibraryId::new(MirClass::Archer, MirGender::Female, false);
    assert_eq!(
        ar_plain.library_key(
            NativeLayer::Weapon(mir_render::player_layout::NativeHand::Main),
            200
        ),
        None,
        "普通弓手 WeaponLibrary1 也必须为空"
    );
    assert_eq!(
        ar_plain
            .library_key(
                NativeLayer::Weapon(mir_render::player_layout::NativeHand::Off),
                200
            )
            .as_deref(),
        Some("arweapon-00")
    );
}

#[test]
#[ignore = "需要先生成 game-assets 原生玩家拆包（R48 A2）"]
fn classic_a1_to_a4_body_frames_are_non_empty() {
    let workspace = workspace_root();
    let root = workspace.join("game-assets");
    let map = LibraryMap::load(root.join("library-map.json")).expect("dev library map");
    let mut libraries = EntityLibraries::open_with_sources(
        root.join("data"),
        Some(&root.join("gfx")),
        Some(&root),
        Some(&map),
        None,
        false,
    );

    let cases = [
        (MirClass::Warrior, NativeAction::Attack1),
        (MirClass::Warrior, NativeAction::Attack2),
        (MirClass::Warrior, NativeAction::Attack3),
        (MirClass::Warrior, NativeAction::Attack4),
        (MirClass::Wizard, NativeAction::Attack1),
        (MirClass::Wizard, NativeAction::Attack2),
        (MirClass::Wizard, NativeAction::Attack3),
        (MirClass::Wizard, NativeAction::Attack4),
        (MirClass::Taoist, NativeAction::Attack1),
        (MirClass::Taoist, NativeAction::Attack2),
        (MirClass::Taoist, NativeAction::Attack3),
        (MirClass::Taoist, NativeAction::Attack4),
        (MirClass::Assassin, NativeAction::Attack1),
        (MirClass::Assassin, NativeAction::Attack2),
        (MirClass::Assassin, NativeAction::Attack3),
        (MirClass::Assassin, NativeAction::Attack4),
        (MirClass::Archer, NativeAction::Attack1),
        (MirClass::Archer, NativeAction::Attack2),
        (MirClass::Archer, NativeAction::Attack3),
    ];
    for (class, action) in cases {
        for gender in [MirGender::Male, MirGender::Female] {
            let alt_anim = matches!(class, MirClass::Assassin | MirClass::Archer);
            let id = NativeLibraryId::new(class, gender, alt_anim);
            let key = id
                .library_key(NativeLayer::Armour, 0)
                .expect("body library key");
            let source = NativeLibrary::open(&mut libraries, &key).expect("body source");
            let sequence = NativeFrameTable::get(action);
            for direction in [
                MirDirection::Up,
                MirDirection::UpRight,
                MirDirection::Right,
                MirDirection::DownRight,
                MirDirection::Down,
                MirDirection::DownLeft,
                MirDirection::Left,
                MirDirection::UpLeft,
            ] {
                for frame in 0..sequence.count {
                    let index = frame_index(sequence, direction, frame, id.armour_offset());
                    let info = source.info(index as usize).unwrap_or_else(|err| {
                        panic!("{key} {action:?} {direction:?} {frame} 越界: {err}")
                    });
                    assert!(
                        !info.is_empty(),
                        "{key} {action:?} {direction:?} frame={frame} index={index} 不应为空"
                    );
                }
            }
        }
    }
}

#[test]
#[ignore = "需要先生成 game-assets 原生玩家拆包（R48 A2）"]
fn classic_level_offsets_are_absent_from_native_player_keys() {
    let workspace = workspace_root();
    let root = workspace.join("game-assets");
    let map = LibraryMap::load(root.join("library-map.json")).expect("dev library map");
    let mut libraries = EntityLibraries::open_with_sources(
        root.join("data"),
        Some(&root.join("gfx")),
        Some(&root),
        Some(&map),
        None,
        false,
    );
    for key in [
        "carmour-00",
        "carmour-24",
        "aarmour-00",
        "ararmour-00",
        "cweapon-00",
        "aweapon-00-r",
        "arweapon-00-s",
    ] {
        let source = NativeLibrary::open(&mut libraries, key).expect("native source");
        let count = source.len();
        assert!(
            !source
                .info(count.saturating_sub(1))
                .expect("last frame")
                .is_empty(),
            "{key} 末帧不应为空"
        );
    }
    for key in ["carmour-1616", "aarmour-1024", "ararmour-704"] {
        assert!(
            NativeLibrary::open(&mut libraries, key).is_none(),
            "{key} 是经典扁平库名，原生域不应按层级猜库"
        );
    }
}

#[test]
#[ignore = "需要先生成 game-assets/packs（R48 原生玩家导入）"]
fn release_pack_native_armour_decodes() {
    let workspace = workspace_root();
    let root = workspace.join("game-assets");
    let map = LibraryMap::load(root.join("packs/library-map.json")).expect("pack map");
    let packs = PackSet::open_unverified(root.join("packs")).expect("pack set");
    let mut libraries = EntityLibraries::open_with_sources(
        root.join("data"),
        None,
        Some(&root.join("packs")),
        Some(&map),
        Some(&packs),
        false,
    );
    for key in ["carmour-00", "aarmour-00", "ararmour-00"] {
        let source = libraries.native_library(key).expect("native armour pack");
        assert!(source.is_pack(), "{key} 必须从 release pack 加载");
        let info = source.info(0).expect("frame info");
        assert!(!info.is_empty(), "{key} 首帧不应为空");
        let image = source.image(0).expect("decode frame");
        assert_eq!(
            image.pixels.len(),
            usize::from(image.info.width) * usize::from(image.info.height) * 4,
            "{key} PNG zlib 解码后的 RGBA 长度应与宽高一致"
        );
    }
}
