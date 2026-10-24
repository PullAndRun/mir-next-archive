//! R49 B 原生怪物真实素材证据（ignored）。
//!
//! 直接读默认 `game-assets` 的原生目录，渲染 005 的站立/行走/Attack1/Struck，
//! 断言每一帧都有非背景像素，并把 PNG 落到 `target/r49/t41/`。缺素材时跳过，
//! 不让普通 CI 因为资源目录缺失而红。

use std::path::{Path, PathBuf};

use mir_core::Point;
use mir_core::enums::MirDirection;
use mir_render::entities::{EntityKind, EntityView};
use mir_render::{ActorAction, EntityLibraries, EntityRenderer, Frame, Viewport};
use serde_json::json;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn game_assets() -> PathBuf {
    repo_root().join("game-assets")
}

fn render(action: ActorAction, now_ms: u64) -> (Frame, u64) {
    let root = game_assets();
    assert!(
        root.join("library-map.json").is_file(),
        "R49 证据需要真实 game-assets：缺 {}（本测试带 #[ignore]，缺素材必须失败而不是空过）",
        root.join("library-map.json").display()
    );
    let map = mir_asset::LibraryMap::load(root.join("library-map.json")).expect("library-map");
    let mut renderer = EntityRenderer::new(EntityLibraries::open_with_sources(
        root.join("data"),
        Some(&root.join("gfx")),
        Some(&root),
        Some(&map),
        None,
        false,
    ));
    let mut entity = EntityView::new(Point::new(6, 6), MirDirection::Down, EntityKind::Monster, 5);
    entity.action = action;
    entity.action_started_ms = 0;
    let mut frame = Frame::new(220, 280);
    frame.clear([0, 0, 0, 255]);
    renderer.render(
        &mut frame,
        Viewport::new(220, 280),
        Point::new(6, 6),
        std::slice::from_ref(&entity),
        u32::try_from(now_ms).unwrap_or(u32::MAX),
    );
    let non_background = frame
        .pixels
        .chunks_exact(4)
        .filter(|pixel| pixel[..3] != [0, 0, 0])
        .count() as u64;
    (frame, non_background)
}

#[test]
#[ignore = "读真实 game-assets，按任务要求单独跑"]
fn monster_005_native_actions_have_pixels() {
    let out = repo_root().join("target/r49/t41");
    std::fs::create_dir_all(&out).expect("创建 target/r49/t41");
    for (name, action, now_ms, expected_min) in [
        ("standing", ActorAction::Standing, 250, 500u64),
        ("walking", ActorAction::Walking, 250, 500),
        ("attack1", ActorAction::Attacking, 250, 500),
        ("struck", ActorAction::Struck, 50, 500),
    ] {
        // 缺素材不能空过：本测试带 #[ignore]，只有显式 --ignored 才跑。
        let (frame, non_background) = render(action, now_ms);
        assert!(
            non_background >= expected_min,
            "{name} 原生帧非背景像素 {non_background} < {expected_min}，不能回退占位"
        );
        assert!(
            frame.distinct_color_count() >= 8,
            "{name} 只有 {} 种颜色，疑似占位色块",
            frame.distinct_color_count()
        );
        let path = out.join(format!("monster-005-{name}.png"));
        frame.save_png(&path).expect("写 PNG");
        assert!(path.is_file());
    }

    // 005 的 frames.json 没有 Running；原生表存在但动作缺表时必须安静跳过，
    // 不能借 Standing/Attack1 的帧，也不能退回占位方块。
    let (_frame, non_background) = render(ActorAction::Running, 250);
    assert_eq!(
        non_background, 0,
        "缺 Running 动作时不得画占位或借其它动作的帧"
    );
}

#[test]
#[ignore = "读真实 game-assets 与 packs；生成 docs/testing/monster-motion stride=0 证据"]
fn stride_zero_monsters_render_standing_pixels_in_dev_and_pack() {
    let root = game_assets();
    let frames_path = root.join("gfx/sprites/monster-native/monster-010/frames.json");
    assert!(
        frames_path.is_file(),
        "R49 证据需要原生拆包：缺 {}（本测试带 #[ignore]，缺素材必须失败而不是空过）",
        frames_path.display()
    );
    let packs_dir = root.join("packs");
    assert!(
        packs_dir.join("manifest.packs.json").is_file(),
        "R49 证据需要发布包，缺 {}",
        packs_dir.display()
    );
    let dev_map = mir_asset::LibraryMap::load(root.join("library-map.json")).expect("dev map");
    let pack_map =
        mir_asset::LibraryMap::load(packs_dir.join("library-map.json")).expect("pack map");
    let packs = mir_asset::PackSet::open_unverified(&packs_dir).expect("open packs");
    let empty_data = repo_root().join("target/r49/t48/empty-data");
    let empty_root = repo_root().join("target/r49/t48/empty-root");
    std::fs::create_dir_all(&empty_data).expect("empty data");
    std::fs::create_dir_all(&empty_root).expect("empty root");

    let directions = [
        MirDirection::Up,
        MirDirection::UpRight,
        MirDirection::Right,
        MirDirection::DownRight,
        MirDirection::Down,
        MirDirection::DownLeft,
        MirDirection::Left,
        MirDirection::UpLeft,
    ];
    let stems = ["010", "041", "043", "062"];
    let docs = repo_root().join("docs/testing/monster-motion");
    let png_dir = docs.join("stride-zero");
    std::fs::create_dir_all(&png_dir).expect("stride zero png dir");
    for entry in std::fs::read_dir(&png_dir).expect("read stride png dir") {
        let path = entry.expect("entry").path();
        if path.extension().is_some_and(|ext| ext == "png") {
            std::fs::remove_file(path).expect("remove stale stride png");
        }
    }

    let mut evidence = Vec::new();
    for (mode, libraries) in [
        (
            "dev",
            EntityLibraries::open_with_sources(
                root.join("data"),
                Some(&root.join("gfx")),
                Some(&root),
                Some(&dev_map),
                None,
                false,
            ),
        ),
        (
            "pack",
            EntityLibraries::open_with_sources(
                &empty_data,
                None,
                Some(&empty_root),
                Some(&pack_map),
                Some(&packs),
                false,
            ),
        ),
    ] {
        let mut libraries = libraries;
        let expected_source = if mode == "dev" { "unpacked" } else { "pack" };
        let sources: Vec<(String, &'static str, mir_asset::FrameSet)> = stems
            .iter()
            .map(|stem| {
                let key = format!("monster-{stem}");
                let source = libraries.source_kind(&key).expect("图库来源");
                assert_eq!(
                    source, expected_source,
                    "{key} 在 {mode} 模式必须从对应来源加载，不能静默回退"
                );
                let frames = libraries
                    .monster_frames(&key)
                    .unwrap_or_else(|| panic!("{key} 应有 frames.json"));
                (key, source, (*frames).clone())
            })
            .collect();
        let mut renderer = EntityRenderer::new(libraries);

        for (stem, (key, source, frames)) in stems.into_iter().zip(sources) {
            let image: u16 = stem.parse().expect("monster image");
            let standing = frames.get(0).expect("Standing 帧表记录");

            let mut standing_pixels = Vec::new();
            let mut images = Vec::new();
            for direction in directions {
                let frame =
                    render_monster(&mut renderer, image, direction, ActorAction::Standing, 250);
                let pixels = non_background(&frame);
                assert!(
                    pixels > 0,
                    "{mode} {key} {direction:?} Standing 非背景像素为 0，stride=0 被误拒"
                );
                images.push(frame.pixels.clone());
                let range = standing
                    .frame_indices(direction as usize)
                    .expect("Standing 方向范围");
                standing_pixels.push(json!({
                    "direction": format!("{direction:?}"),
                    "non_background": pixels,
                    "distinct_colors": frame.distinct_color_count(),
                    "frame_range": [range.start, range.end],
                }));
            }
            assert!(
                images.windows(2).all(|pair| pair[0] == pair[1]),
                "{mode} {key} Count+Skip==0 时 8 方向必须共享同一 Standing 帧"
            );

            // 真实 010/041/043/062 的 frames.json 没有 Walking(1)/Running(2)；
            // Crystal MonsterObject.cs:373-378/511-524 在 Frame==null 时不位移、
            // 不借帧。这里断言安静跳过（0 像素），而不是伪造 6 帧默认动作。
            let mut walking = Vec::new();
            for action in [ActorAction::Walking, ActorAction::Running] {
                let mut max_pixels = 0;
                for direction in directions {
                    let frame = render_monster(&mut renderer, image, direction, action, 250);
                    max_pixels = max_pixels.max(non_background(&frame));
                }
                assert_eq!(
                    max_pixels, 0,
                    "{mode} {key} 缺 {action:?} 时必须安静跳过，不能占位/借帧"
                );
                walking.push(json!({
                    "action": format!("{action:?}"),
                    "present_in_frames_json": false,
                    "non_background": max_pixels,
                    "crystal_behavior": "Frame==null → OffSetMove=Empty, Movement=CurrentLocation",
                }));
            }

            if mode == "dev" {
                let down = render_monster(
                    &mut renderer,
                    image,
                    MirDirection::Down,
                    ActorAction::Standing,
                    250,
                );
                down.save_png(png_dir.join(format!("{stem}-standing-down.png")))
                    .expect("write stride png");
            }

            evidence.push(json!({
                "image": key,
                "mode": mode,
                "source": source,
                "standing_frame": {
                    "start": standing.start,
                    "count": standing.count,
                    "skip": standing.skip,
                    "interval_ms": standing.interval,
                },
                "standing": standing_pixels,
                "missing_actions": walking,
            }));
        }
    }

    // Count=1/Skip=-1（041/043/062 的 Dead=raw22）是另一种合法负 Skip：
    // stride=0，8 方向也只能取同一帧。
    let pack_map =
        mir_asset::LibraryMap::load(packs_dir.join("library-map.json")).expect("pack map");
    let packs = mir_asset::PackSet::open_unverified(&packs_dir).expect("open packs");
    let mut renderer = EntityRenderer::new(EntityLibraries::open_with_sources(
        &empty_data,
        None,
        Some(&empty_root),
        Some(&pack_map),
        Some(&packs),
        false,
    ));
    let mut dead_frames = Vec::new();
    for direction in directions {
        let frame = render_monster(&mut renderer, 41, direction, ActorAction::Dead, 100);
        assert!(
            non_background(&frame) > 0,
            "041 Dead Count=1/Skip=-1 是合法帧，8 方向都应有像素"
        );
        dead_frames.push(frame.pixels.clone());
    }
    assert!(
        dead_frames.windows(2).all(|pair| pair[0] == pair[1]),
        "Count=1 时 8 方向共享同一帧"
    );

    let generated = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64);
    let manifest = json!({
        "test": "stride_zero_monsters_render_standing_pixels_in_dev_and_pack",
        "batch": iso_utc(generated),
        "generated_at_unix_ms": generated,
        "crystal_reference": "github/Crystal Client/MirObjects/MonsterObject.cs:511-524,633-642",
        "note": "010/041/043/062 的 frames.json 只有 Standing/Attack/Struck/Die/Dead/Revive，没有 Walking/Running；Crystal 对 Frame==null 的移动动作不位移、不借帧，故这两个动作非背景像素必须为 0。",
        "monsters": evidence,
        "count1_negative_skip": {
            "image": "monster-041",
            "action": "Dead",
            "raw_action": 22,
            "count": 1,
            "skip": -1,
            "eight_directions_identical": true,
        },
    });
    std::fs::write(
        docs.join("stride-zero-render.json"),
        serde_json::to_vec_pretty(&manifest).expect("json"),
    )
    .expect("write stride evidence");
    println!("stride-zero evidence: {}", docs.display());
}

fn render_monster(
    renderer: &mut EntityRenderer,
    image: u16,
    direction: MirDirection,
    action: ActorAction,
    elapsed_ms: u64,
) -> Frame {
    let mut view = EntityView::new(Point::new(8, 8), direction, EntityKind::Monster, image);
    view.action = action;
    view.action_started_ms = 0;
    let mut frame = Frame::new(400, 400);
    frame.clear([0, 0, 0, 255]);
    renderer.render(
        &mut frame,
        Viewport::new(400, 400),
        Point::new(8, 8),
        std::slice::from_ref(&view),
        u32::try_from(elapsed_ms).unwrap_or(u32::MAX),
    );
    frame
}

fn non_background(frame: &Frame) -> usize {
    frame
        .pixels
        .chunks_exact(4)
        .filter(|pixel| pixel[..3] != [0, 0, 0])
        .count()
}

fn iso_utc(unix_ms: u64) -> String {
    fn civil_from_days(days: i64) -> (i64, u32, u32) {
        let z = days + 719_468;
        let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let year = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        (if month <= 2 { year + 1 } else { year }, month, day)
    }
    let seconds = (unix_ms / 1000) as i64;
    let days = seconds.div_euclid(86_400);
    let rem = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        (rem % 3_600) / 60,
        rem % 60
    )
}

#[test]
#[ignore = "读真实 game-assets，按任务要求单独跑"]
fn native_durations_derive_from_frames_json() {
    for (stem, expected) in [
        ("005", [(0u8, 2_000u64), (9, 600), (18, 400), (21, 1_000)]),
        ("022", [(0, 2_000), (9, 600), (18, 400), (21, 1_000)]),
        ("039", [(0, 2_000), (9, 600), (18, 400), (21, 1_000)]),
        ("023", [(0, 2_000), (9, 600), (18, 400), (21, 1_000)]),
    ] {
        let path = game_assets().join(format!(
            "gfx/sprites/monster-native/monster-{stem}/frames.json"
        ));
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("读 frames.json"))
                .expect("解析 frames.json");
        let actions = json["actions"].as_array().expect("actions[]");
        for (raw, duration) in expected {
            let action = actions
                .iter()
                .find(|action| action["raw_action"].as_u64() == Some(u64::from(raw)))
                .unwrap_or_else(|| panic!("{stem} 缺 action {raw}"));
            let count = action["count"].as_u64().expect("count");
            let interval = action["interval"].as_u64().expect("interval");
            assert_eq!(
                count * interval,
                duration,
                "{stem} action {raw} 的 Count×Interval 不符"
            );
            if count > 1 && action["skip"].as_i64() == Some(-(count as i64)) {
                let directions = action["frames_by_direction"]
                    .as_array()
                    .expect("directions");
                assert_eq!(directions.len(), 8);
                for direction in &directions[1..] {
                    assert_eq!(direction, &directions[0], "{stem} action {raw} stride=0");
                }
            }
        }
    }
}
