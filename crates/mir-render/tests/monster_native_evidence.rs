//! R49 D：原生怪物/玩家双模式证据。
//!
//! 默认 `game-assets`（auto→dev/unpacked）和 release `packs/` 用同一张真实地图、
//! 同一组 `EntityView` 渲染；逐 PNG 比较两模式 RGBA。还会落 005/022/039/023
//! 的 Standing/Walking/Attack1/Struck/Die/Dead/Revive 画面和 5 职业男女玩家。
//! 真实素材不存在时跳过（CI 不跑素材用例），但会明确打印原因。

use std::path::{Path, PathBuf};

use mir_asset::{LibraryMap, PackSet};
use mir_core::Point;
use mir_core::enums::{MirClass, MirDirection, MirGender};
use mir_render::entities::{EntityKind, EntityView};
use mir_render::player_layout::{NativeAction, PlayerLayout};
use mir_render::{ActorAction, EntityLibraries, Frame, MapScene};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .to_path_buf()
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mir2-r49d-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn player(
    class: MirClass,
    gender: MirGender,
    alt: bool,
    weapon_shape: u16,
    action: NativeAction,
    position: Point,
) -> EntityView {
    let mut view = EntityView::new(position, MirDirection::Down, EntityKind::Player, 0);
    view.armour = 0;
    view.hair = 0;
    view.weapon = u16::MAX;
    view.weapon_shape = weapon_shape;
    view.class = class;
    view.gender = gender;
    view.player_layout = Some(PlayerLayout::new(class, gender, alt, weapon_shape));
    assert_eq!(
        view.player_layout.expect("layout").alt_anim,
        alt,
        "player fixture altAnim"
    );
    view.native_action = Some(action);
    view.action = if action == NativeAction::Attack1 {
        ActorAction::Attacking
    } else {
        ActorAction::Standing
    };
    view.action_started_ms = 0;
    view
}

fn monster(image: u16, action: ActorAction, position: Point) -> EntityView {
    let mut view = EntityView::new(position, MirDirection::Down, EntityKind::Monster, image);
    view.action = action;
    view.action_started_ms = 0;
    view
}

fn render(scene: &mut MapScene, views: &[EntityView], center: Point, now: u64) -> Frame {
    let mut frame = Frame::new(800, 600);
    scene.render_with_entities_at(&mut frame, center, views, now);
    frame
}

fn changed_pixels(frame: &Frame, background: &Frame) -> usize {
    frame
        .pixels
        .chunks_exact(4)
        .zip(background.pixels.chunks_exact(4))
        .filter(|(left, right)| left != right)
        .count()
}

fn save(frame: &Frame, dir: &Path, name: &str) -> String {
    let path = dir.join(name);
    frame.save_png(&path).expect("write png");
    format!("docs/testing/monster-native/{name}")
}

#[test]
#[ignore = "需要真实 game-assets 与 packs；生成 docs/testing/monster-native 证据"]
fn native_monsters_and_players_render_in_dev_and_release_modes() {
    let root = repo_root().join("game-assets");
    let map_path = root.join("maps/0.mnm");
    let packs_dir = root.join("packs");
    assert!(
        map_path.is_file() && packs_dir.join("manifest.packs.json").is_file(),
        "R49 D 证据需要真实素材：缺 {} 或 {}（本测试带 #[ignore]，只有显式 --ignored 才跑；\
         缺素材必须失败而不是空过）",
        map_path.display(),
        packs_dir.join("manifest.packs.json").display()
    );
    let dev_map = LibraryMap::load(root.join("library-map.json")).expect("dev map");
    let mut dev_scene = MapScene::load_with_sources(
        &map_path,
        root.join("data"),
        Some(&root.join("gfx")),
        Some(&root),
        Some(&dev_map),
        None,
        false,
    )
    .expect("dev scene");
    let mut dev_libraries = EntityLibraries::open_with_sources(
        root.join("data"),
        Some(&root.join("gfx")),
        Some(&root),
        Some(&dev_map),
        None,
        false,
    );

    let packs_map = LibraryMap::load(packs_dir.join("library-map.json")).expect("packs map");
    let packs = PackSet::open_unverified(&packs_dir).expect("open packs");
    let empty_data = temp_dir("empty-data");
    let empty_root = temp_dir("empty-root");
    let mut release_scene = MapScene::load_with_sources(
        &map_path,
        &empty_data,
        None,
        Some(&empty_root),
        Some(&packs_map),
        Some(&packs),
        false,
    )
    .expect("release scene");
    let mut release_libraries = EntityLibraries::open_with_sources(
        &empty_data,
        None,
        Some(&empty_root),
        Some(&packs_map),
        Some(&packs),
        false,
    );

    // R50-F：out_dir 可用 `MIR_NATIVE_EVIDENCE_DIR` 覆盖（相对路径按仓库根解析），
    // 用于把重跑产物只写 `target/round50/**` 而不覆写冻结的 `docs/testing/monster-native`；
    // 未设置或为空时保持默认路径，默认行为不变。
    let out_dir = match std::env::var_os("MIR_NATIVE_EVIDENCE_DIR") {
        Some(value) if !value.is_empty() => {
            let path = PathBuf::from(value);
            if path.is_absolute() {
                path
            } else {
                repo_root().join(path)
            }
        }
        _ => repo_root().join("docs/testing/monster-native"),
    };
    std::fs::create_dir_all(out_dir.join("png")).expect("evidence png dir");
    let png_dir = out_dir.join("png");
    for entry in std::fs::read_dir(&png_dir).expect("read png dir") {
        let path = entry.expect("entry").path();
        if path.extension().is_some_and(|ext| ext == "png") {
            std::fs::remove_file(path).expect("remove stale png");
        }
    }

    let background = render(&mut dev_scene, &[], Point::new(330, 270), 0);
    let mut samples = Vec::new();
    let mut pngs = Vec::new();

    let player_cases = [
        ("warrior-male", MirClass::Warrior, MirGender::Male, false, 0),
        (
            "warrior-female",
            MirClass::Warrior,
            MirGender::Female,
            false,
            0,
        ),
        ("wizard-male", MirClass::Wizard, MirGender::Male, false, 0),
        (
            "wizard-female",
            MirClass::Wizard,
            MirGender::Female,
            false,
            0,
        ),
        ("taoist-male", MirClass::Taoist, MirGender::Male, false, 0),
        (
            "taoist-female",
            MirClass::Taoist,
            MirGender::Female,
            false,
            0,
        ),
        (
            "assassin-male-alt",
            MirClass::Assassin,
            MirGender::Male,
            true,
            100,
        ),
        (
            "assassin-female-alt",
            MirClass::Assassin,
            MirGender::Female,
            true,
            100,
        ),
        (
            "assassin-male-plain",
            MirClass::Assassin,
            MirGender::Male,
            false,
            0,
        ),
        (
            "archer-male-alt",
            MirClass::Archer,
            MirGender::Male,
            true,
            200,
        ),
        (
            "archer-female-alt",
            MirClass::Archer,
            MirGender::Female,
            true,
            200,
        ),
        (
            "archer-male-plain",
            MirClass::Archer,
            MirGender::Male,
            false,
            0,
        ),
    ];
    for (name, class, gender, alt, shape) in player_cases {
        let standing = player(
            class,
            gender,
            alt,
            shape,
            NativeAction::Standing,
            Point::new(330, 270),
        );
        let attack = player(
            class,
            gender,
            alt,
            shape,
            NativeAction::Attack1,
            Point::new(330, 270),
        );
        let let_stand = render(&mut dev_scene, &[standing], Point::new(330, 270), 0);
        let attack_frame = render(&mut dev_scene, &[attack], Point::new(330, 270), 0);
        let changed = changed_pixels(&let_stand, &background);
        assert!(changed > 100, "{name} 站立应有可见像素，实际 {changed}");
        assert!(
            changed_pixels(&attack_frame, &let_stand) > 20,
            "{name} Attack1 不能回退为 Standing"
        );
        pngs.push(save(&let_stand, &png_dir, &format!("{name}-standing.png")));
        pngs.push(save(
            &attack_frame,
            &png_dir,
            &format!("{name}-attack1.png"),
        ));
        samples.push(serde_json::json!({
            "kind": "player",
            "name": name,
            "class": format!("{class:?}"),
            "gender": format!("{gender:?}"),
            "alt_anim": alt,
            "weapon_shape": shape,
            "native_action": "Attack1",
            "standing_png": format!("docs/testing/monster-native/png/{name}-standing.png"),
            "attack_png": format!("docs/testing/monster-native/png/{name}-attack1.png"),
            "changed_pixels": changed,
        }));
        let body_key = format!(
            "{}-00",
            if alt && matches!(class, MirClass::Assassin) {
                "aarmour"
            } else if alt && matches!(class, MirClass::Archer) {
                "ararmour"
            } else {
                "carmour"
            }
        );
        assert_eq!(
            dev_libraries.source_kind(&body_key),
            Some("unpacked"),
            "{name} dev body source"
        );
        assert_eq!(
            release_libraries.source_kind(&body_key),
            Some("pack"),
            "{name} release body source"
        );
    }

    let monster_actions = [
        ("standing", ActorAction::Standing, 0),
        ("walking", ActorAction::Walking, 100),
        ("attack1", ActorAction::Attacking, 0),
        ("struck", ActorAction::Struck, 0),
        ("dying", ActorAction::Dying, 0),
        ("dead", ActorAction::Dead, 1000),
    ];
    for image in [5u16, 22, 39, 23] {
        let key = format!("monster-{image:03}");
        assert_eq!(
            dev_libraries.source_kind(&key),
            Some("unpacked"),
            "dev {key}"
        );
        assert_eq!(
            release_libraries.source_kind(&key),
            Some("pack"),
            "release {key}"
        );
        let standing = monster(image, ActorAction::Standing, Point::new(330, 270));
        let standing_frame = render(&mut dev_scene, &[standing], Point::new(330, 270), 0);
        let visible = changed_pixels(&standing_frame, &background);
        assert!(visible > 100, "{key} 站立应有可见像素，实际 {visible}");
        for (action_name, action, now) in monster_actions {
            let view = monster(image, action, Point::new(330, 270));
            let pixel_offset = view.pixel_offset;
            let sort_y = view.sort_y;
            let frame = render(&mut dev_scene, &[view], Point::new(330, 270), now);
            let changed = changed_pixels(&frame, &background);
            assert!(
                changed > 50,
                "{key} {action_name} 应有可见像素，实际 {changed}"
            );
            let name = format!("{key}-{action_name}.png");
            pngs.push(save(&frame, &png_dir, &name));
            samples.push(serde_json::json!({
                "kind": "monster",
                "library_key": key,
                "image": image,
                "action": action_name,
                "action_started_ms": 0,
                "sample_ms": now,
                "object_id": 1000 + u32::from(image),
                "direction": "Down",
                "from": [330, 270],
                "to": [330, 270],
                "started_ms": 0,
                "classification": "native-frames-json",
                "asset_mode": "dev/unpacked",
                "motion_tick_ms": 100,
                "pixel_offset": [pixel_offset.0, pixel_offset.1],
                "sort_y": sort_y,
                "png": format!("docs/testing/monster-native/png/{name}"),
                "changed_pixels": changed,
            }));
        }
    }

    let mixed_views = [
        player(
            MirClass::Warrior,
            MirGender::Male,
            false,
            0,
            NativeAction::Standing,
            Point::new(328, 267),
        ),
        player(
            MirClass::Archer,
            MirGender::Female,
            true,
            200,
            NativeAction::WalkingBow,
            Point::new(328, 270),
        ),
        player(
            MirClass::Assassin,
            MirGender::Male,
            true,
            100,
            NativeAction::Attack1,
            Point::new(332, 267),
        ),
        monster(5, ActorAction::Walking, Point::new(332, 270)),
        monster(22, ActorAction::Attacking, Point::new(330, 273)),
    ];
    let mut mixed_results = Vec::new();
    for (label, center) in [
        ("self-still", Point::new(330, 270)),
        ("self-moved", Point::new(331, 270)),
    ] {
        let dev = render(&mut dev_scene, &mixed_views, center, 100);
        let release = render(&mut release_scene, &mixed_views, center, 100);
        let diff = dev
            .pixels
            .chunks_exact(4)
            .zip(release.pixels.chunks_exact(4))
            .filter(|(left, right)| left != right)
            .count();
        assert_eq!(diff, 0, "{label}: dev/release 必须逐像素一致");
        assert!(
            changed_pixels(&dev, &background) > 1000,
            "{label} 场景不能为空"
        );
        let _dev_path = save(&dev, &png_dir, &format!("{label}-dev.png"));
        let _release_path = save(&release, &png_dir, &format!("{label}-release.png"));
        mixed_results.push(serde_json::json!({
            "label": label,
            "camera_center": [center.x, center.y],
            "dev_png": format!("docs/testing/monster-native/png/{label}-dev.png"),
            "release_png": format!("docs/testing/monster-native/png/{label}-release.png"),
            "pixel_diff": diff,
            "source_kind_dev": "unpacked",
            "source_kind_release": "pack",
        }));
    }

    let raw = serde_json::json!({
        "test": "native_monsters_and_players_render_in_dev_and_release_modes",
        "map": "game-assets/maps/0.mnm",
        "generated_at_unix_ms": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_millis(),
        "asset_mode": "dev(unpacked) vs release(pack-only)",
        "player_cases": player_cases.len(),
        "samples": samples,
        "mixed": mixed_results,
        "png": pngs,
    });
    std::fs::write(
        out_dir.join("raw-manifest.json"),
        serde_json::to_vec_pretty(&raw).expect("raw manifest"),
    )
    .expect("write raw manifest");
    println!("monster-native evidence: {}", out_dir.display());
    std::fs::remove_dir_all(&empty_data).ok();
    std::fs::remove_dir_all(&empty_root).ok();
}
