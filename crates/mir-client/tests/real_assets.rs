//! 客户端真实素材渲染回归：`resource/map/0.map` + `resource/data`。
//!
//! `resource/` 被 `.gitignore` 忽略（CI 上没有），缺素材时打印原因并跳过。
//! 有素材时验证：
//!
//! 1. 地图用真实 `tiles.wzl` 渲染（颜色数 > 256）；
//! 2. 玩家/怪物/NPC 精灵真的解码（实体缓存 > 0，玩家所在区域与空场景不同）；
//! 3. 动作换帧会改变角色像素（动画帧循环生效）；
//! 4. 截图写到 `target/client-render-world.png`。

use std::path::{Path, PathBuf};

use mir_client::net::WorldSnapshot;
use mir_core::Point;
use mir_core::enums::{MirClass, MirDirection, MirGender, Monster};
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::{ObjectMonster, ObjectNPC, ObjectPlayer, UserInformation};
use mir_render::{Frame, MapScene};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn assets() -> Option<(PathBuf, PathBuf)> {
    let root = repo_root();
    // 运行期只读自有格式：`game-assets/`（`.mnm` 地图 + 拆包 gfx）。
    let map = root.join("game-assets/maps/0.mnm");
    let data = root.join("game-assets/data");
    if !map.is_file() || !root.join("game-assets/library-map.json").is_file() {
        eprintln!(
            "跳过：缺 {} 或 game-assets/library-map.json（game-assets/ 被 .gitignore 忽略）",
            map.display()
        );
        return None;
    }
    Some((map, data))
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

/// 一个玩家 + 一只怪 + 一个 NPC 的快照。
fn snapshot() -> WorldSnapshot {
    let mut snapshot = WorldSnapshot::default();
    snapshot.apply_packet(&ServerPacket::UserInformation(UserInformation {
        object_id: 7,
        real_id: 7,
        name: "测试者".to_owned(),
        class: MirClass::Warrior,
        gender: MirGender::Male,
        level: 22,
        location: Point::new(330, 270),
        direction: MirDirection::Down,
        hp: 10,
        mp: 10,
        ..UserInformation::default()
    }));
    snapshot.apply_packet(&ServerPacket::ObjectPlayer(ObjectPlayer {
        object_id: 8,
        name: "路人".to_owned(),
        class: MirClass::Taoist,
        gender: MirGender::Female,
        location: Point::new(329, 271),
        direction: MirDirection::Left,
        ..ObjectPlayer::default()
    }));
    snapshot.apply_packet(&ServerPacket::ObjectMonster(ObjectMonster {
        object_id: 42,
        name: "鸡".to_owned(),
        image: Monster::Hen,
        location: Point::new(331, 271),
        direction: MirDirection::Left,
        ..ObjectMonster::default()
    }));
    snapshot.apply_packet(&ServerPacket::ObjectNPC(ObjectNPC {
        object_id: 43,
        name: "商人".to_owned(),
        image: 100,
        location: Point::new(329, 269),
        direction: MirDirection::Down,
        ..ObjectNPC::default()
    }));
    snapshot
}

/// 统计矩形里两帧不同的像素数（客户端的 `geom::Rect` 换成窗口坐标）。
fn changed_pixels(before: &Frame, after: &Frame, x0: i32, y0: i32, x1: i32, y1: i32) -> usize {
    let mut count = 0;
    for y in y0.max(0)..y1.min(i32::try_from(after.height).unwrap_or(0)) {
        for x in x0.max(0)..x1.min(i32::try_from(after.width).unwrap_or(0)) {
            let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
                continue;
            };
            if before.pixel(x, y) != after.pixel(x, y) {
                count += 1;
            }
        }
    }
    count
}

#[test]
fn renders_real_map_with_player_monster_and_npc() {
    let Some((map_path, data_dir)) = assets() else {
        return;
    };
    let mut scene = load_scene(&map_path, &data_dir);
    assert!(scene.libraries().len() >= 3, "至少 tiles/smtiles/objects");
    let center = Point::new(330, 270);

    let snapshot = snapshot();
    let views = snapshot.entity_views(0);
    assert!(views.len() >= 4, "玩家 + 路人 + 怪物 + NPC");

    let mut with_entities = Frame::new(800, 600);
    scene.render_with_entities_at(&mut with_entities, center, &views, 0);
    let colors = with_entities.distinct_color_count();
    assert!(colors >= 256, "真实地图颜色太少：{colors}");
    assert!(
        scene.entity_image_cache_len() > 0,
        "实体精灵没有解码（仍在画色块）"
    );
    let output = repo_root().join("target/client-render-world.png");
    std::fs::create_dir_all(output.parent().expect("target")).expect("创建 target");
    with_entities.save_png(&output).expect("写截图");
    println!(
        "真实地图渲染：{} 种颜色，实体缓存 {} 张 → {}",
        colors,
        scene.entity_image_cache_len(),
        output.display()
    );

    // 同一位置去掉实体：玩家所在格周围必须不同（角色精灵真的画上去了）。
    let mut terrain_only = Frame::new(800, 600);
    scene.render(&mut terrain_only, center);
    let (player_x, player_y) = scene.world_to_screen(center, Point::new(330, 270));
    let changed = changed_pixels(
        &terrain_only,
        &with_entities,
        player_x - 40,
        player_y - 96,
        player_x + 40,
        player_y + 16,
    );
    assert!(
        changed > 50,
        "玩家所在区域没有变化（精灵没画出来）：{changed} 像素"
    );

    // 截图也要能看到地图本身（非纯黑背景）。
    assert!(
        with_entities
            .pixels
            .chunks_exact(4)
            .filter(|pixel| pixel[3] != 0
                && u32::from(pixel[0]) + u32::from(pixel[1]) + u32::from(pixel[2]) > 40)
            .count()
            > 10_000,
        "画面大部分是空白"
    );
}

#[test]
fn player_animation_frames_advance_on_real_sprites() {
    let Some((map_path, data_dir)) = assets() else {
        return;
    };
    let mut scene = load_scene(&map_path, &data_dir);
    let center = Point::new(330, 270);
    let mut snapshot = snapshot();
    snapshot.apply_local_action(
        mir_render::animation::ActorAction::Walking,
        MirDirection::Down,
        1_000,
    );

    let first_views = snapshot.entity_views(1_000);
    let first = Frame::new(800, 600);
    let mut first = first;
    scene.render_with_entities_at(&mut first, center, &first_views, 1_000);

    // 走路动画每帧约 100ms：400ms 后应当换了若干帧。
    let later_views = snapshot.entity_views(1_400);
    let mut later = Frame::new(800, 600);
    scene.render_with_entities_at(&mut later, center, &later_views, 1_400);

    let (player_x, player_y) = scene.world_to_screen(center, Point::new(330, 270));
    let changed = changed_pixels(
        &first,
        &later,
        player_x - 40,
        player_y - 96,
        player_x + 40,
        player_y + 16,
    );
    assert!(changed > 10, "角色动画没有换帧：{changed} 像素");
    first
        .save_png(repo_root().join("target/client-render-animation-a.png"))
        .expect("写动画帧 A");
    later
        .save_png(repo_root().join("target/client-render-animation-b.png"))
        .expect("写动画帧 B");
}

#[test]
fn missing_assets_fall_back_to_placeholder_without_panicking() {
    // 不存在的素材目录：`open_dir` 报错，但空图库场景仍能渲染占位（不 panic）。
    let missing = repo_root().join("target/definitely-missing-assets");
    assert!(
        mir_render::TileLibraries::open_dir(&missing).is_err(),
        "缺目录应当报错"
    );
    let map = mir_world::Map {
        width: 32,
        height: 32,
        cells: vec![mir_world::MapCell::default(); 32 * 32],
        file_name: "fallback".to_owned(),
        version: mir_world::MapVersion::V0,
        doors: Vec::new(),
    };
    let mut scene = MapScene::from_parts(
        map,
        mir_render::TileLibraries::empty(&missing),
        mir_render::Viewport::DEFAULT,
    );
    let mut frame = Frame::new(320, 240);
    scene.render(&mut frame, Point::new(16, 16));
    assert!(frame.distinct_color_count() >= 1);
}
