//! 攻击/技能特效与截图：真实 `effect.wzl` / `magic.wzl` + `resource/map/0.map`。
//!
//! 缺素材（CI 的 `resource/` 被忽略）时全部跳过，不 panic。

mod common;

use std::path::{Path, PathBuf};

use mir_client::net::WorldSnapshot;
use mir_client::ui::{self, UiState};
use mir_core::Language;
use mir_core::Point;
use mir_core::enums::{MirClass, MirDirection, MirGender, Spell};
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::{Magic, UserInformation};
use mir_render::animation::ActorAction;
use mir_render::{Frame, MapScene};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// 用自有格式（`game-assets/`）加载地图：library-map + 拆包目录。
fn load_scene(map_path: &Path, data_dir: &Path) -> MapScene {
    let root = data_dir.parent().expect("asset root");
    let map = mir_asset::LibraryMap::load(root.join("library-map.json")).ok();
    let gfx = root.join("gfx");
    MapScene::load_with_sources(
        map_path,
        data_dir,
        Some(gfx.as_path()),
        Some(root),
        map.as_ref(),
        None,
        false,
    )
    .expect("加载真实地图")
}

fn assets() -> Option<(PathBuf, PathBuf)> {
    let root = repo_root();
    // 运行期只读自有格式：地图 `.mnm` + 拆包 gfx（`game-assets/`）。
    let map = root.join("game-assets/maps/0.mnm");
    let data = root.join("game-assets/data");
    if !map.is_file() || !data.is_dir() {
        eprintln!(
            "跳过：缺 {} 或 {}（game-assets/ 被 .gitignore 忽略）",
            map.display(),
            data.display()
        );
        return None;
    }
    Some((map, data))
}

fn snapshot_at(now: u64) -> WorldSnapshot {
    let mut snapshot = WorldSnapshot::default();
    snapshot.apply_packet(&ServerPacket::UserInformation(UserInformation {
        object_id: 7,
        real_id: 7,
        name: "特效测试".to_owned(),
        class: MirClass::Warrior,
        gender: MirGender::Male,
        level: 30,
        location: Point::new(330, 270),
        direction: MirDirection::Right,
        hp: 10,
        mp: 10,
        ..UserInformation::default()
    }));
    // 自己的攻击（本地预测）：画面应当叠挥砍光效。
    snapshot.apply_local_action(ActorAction::Attacking, MirDirection::Right, now);
    // 技能命中：雷电落在右边一格，`spellfx::draw_sprites` 叠 `magic.wzl` 闪光。
    snapshot.apply_packet_at(
        &ServerPacket::Magic(Magic {
            spell: Spell::ThunderBolt,
            target_id: 0,
            target: Point::new(332, 270),
            cast: true,
            level: 3,
            secondary_target_ids: Vec::new(),
        }),
        now,
    );
    snapshot
}

#[test]
fn attack_swing_and_spell_flash_render_with_real_assets() {
    let Some((map_path, data_dir)) = assets() else {
        return;
    };
    let now = 1_700_000_000_000u64;
    let mut scene = load_scene(&map_path, &data_dir);
    let snapshot = snapshot_at(now);
    let text = common::load_text();
    let mut state = UiState::new(Language::ZhCn);
    state.set_time(i64::try_from(now).unwrap_or(i64::MAX) + 120);

    let mut frame = Frame::new(800, 600);
    // 与 app 的渲染顺序一致：先地形 + 实体，再 HUD/特效叠层。
    let views = snapshot.entity_views(now + 120);
    scene.render_with_entities_at(&mut frame, Point::new(330, 270), &views, now + 120);
    ui::draw(
        &mut state,
        &mut frame,
        &mut scene,
        &snapshot,
        &text,
        None,
        Point::new(330, 270),
    );
    let colors = frame.distinct_color_count();
    assert!(colors >= 256, "真实地图 + 特效颜色太少：{colors}");
    let cache = scene.entity_image_cache_len();
    assert!(cache > 0, "攻击/技能特效精灵应当解码进缓存");
    let output = repo_root().join("target/client-fx-attack-magic.png");
    std::fs::create_dir_all(output.parent().expect("target")).expect("创建 target");
    frame.save_png(&output).expect("写特效截图");
    println!(
        "特效渲染：{colors} 种颜色，实体/特效缓存 {cache} 张 → {}",
        output.display()
    );

    // 去掉技能事件再渲染：命中闪光区域的像素必须变化（色块 + magic.wzl 精灵）。
    let mut plain = snapshot.clone();
    plain.spell_events.clear();
    plain.apply_local_action(ActorAction::Standing, MirDirection::Right, now);
    let mut plain_frame = Frame::new(800, 600);
    scene.render_with_entities_at(
        &mut plain_frame,
        Point::new(330, 270),
        &plain.entity_views(now + 120),
        now + 120,
    );
    ui::draw(
        &mut state,
        &mut plain_frame,
        &mut scene,
        &plain,
        &text,
        None,
        Point::new(330, 270),
    );
    let (target_x, target_y) = scene.world_to_screen(Point::new(330, 270), Point::new(332, 270));
    let changed = (0..90)
        .flat_map(|dy| (0..110).map(move |dx| (dx, dy)))
        .filter(|(dx, dy)| {
            let x = u32::try_from(target_x - 24 + dx).ok();
            let y = u32::try_from(target_y - 70 + dy).ok();
            match (x, y) {
                (Some(x), Some(y)) => frame.pixel(x, y) != plain_frame.pixel(x, y),
                _ => false,
            }
        })
        .count();
    assert!(changed > 20, "技能命中区域没有变化：{changed} 像素");
    plain_frame
        .save_png(repo_root().join("target/client-fx-plain.png"))
        .expect("写对照截图");
}

#[test]
fn attack_swing_effect_renders_on_the_player() {
    let Some((map_path, data_dir)) = assets() else {
        return;
    };
    let now = 1_700_000_000_000u64;
    let mut scene = load_scene(&map_path, &data_dir);
    let snapshot = {
        let mut snapshot = WorldSnapshot::default();
        snapshot.apply_packet(&ServerPacket::UserInformation(UserInformation {
            object_id: 7,
            real_id: 7,
            name: "挥砍测试".to_owned(),
            class: MirClass::Warrior,
            gender: MirGender::Male,
            level: 30,
            location: Point::new(330, 270),
            direction: MirDirection::Right,
            hp: 10,
            mp: 10,
            ..UserInformation::default()
        }));
        snapshot.apply_local_action(ActorAction::Attacking, MirDirection::Right, now);
        snapshot
    };
    let views = snapshot.entity_views(now + 100);
    let mut frame = Frame::new(800, 600);
    scene.render_with_entities_at(&mut frame, Point::new(330, 270), &views, now + 100);
    let output = repo_root().join("target/client-fx-attack.png");
    std::fs::create_dir_all(output.parent().expect("target")).expect("创建 target");
    frame.save_png(&output).expect("写挥砍截图");

    // 与站立帧相比，挥砍光效必须带来像素变化。
    let mut standing = snapshot.clone();
    standing.apply_local_action(ActorAction::Standing, MirDirection::Right, now);
    let mut plain = Frame::new(800, 600);
    scene.render_with_entities_at(
        &mut plain,
        Point::new(330, 270),
        &standing.entity_views(now + 100),
        now + 100,
    );
    let changed = frame
        .pixels
        .chunks_exact(4)
        .zip(plain.pixels.chunks_exact(4))
        .filter(|(left, right)| left != right)
        .count();
    assert!(changed > 20, "攻击动作应当叠出挥砍光效：{changed} 像素");
    println!("挥砍光效变化 {changed} 像素 → {}", output.display());
}

#[test]
fn spell_flash_is_silent_without_assets() {
    // 空图库场景：`draw_sprites` 必须静默跳过，不 panic、不画像素。
    let map = mir_world::Map {
        width: 32,
        height: 32,
        cells: vec![mir_world::MapCell::default(); 32 * 32],
        file_name: "fx-empty".to_owned(),
        version: mir_world::MapVersion::V0,
        doors: Vec::new(),
    };
    let mut scene = MapScene::from_parts(
        map,
        mir_render::TileLibraries::empty("."),
        mir_render::Viewport::DEFAULT,
    );
    let events = vec![mir_client::net::SpellEvent {
        id: 1,
        object_id: 0,
        spell: Spell::ThunderBolt,
        location: Point::new(16, 16),
        direction: MirDirection::Right,
        param: true,
        from_object_spell: false,
        started_ms: 1_000,
    }];
    let mut frame = Frame::new(200, 200);
    frame.clear([0, 0, 0, 255]);
    ui::spellfx::draw_sprites(&mut frame, &events, &mut scene, Point::new(16, 16), 1_020);
    assert_eq!(
        frame.distinct_color_count(),
        1,
        "缺 magic.wzl 时命中闪光必须静默跳过"
    );
}
