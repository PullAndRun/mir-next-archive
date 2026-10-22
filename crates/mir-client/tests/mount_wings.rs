//! 坐骑 / 翅膀渲染的无头集成测试，产出 `target/client-mount.png`。
//!
//! 不连服务器：直接用 [`EntityView`] 描述一个骑乘坐骑、带翅膀的玩家，在同一张地图上
//! 分别渲染「无坐骑/翅膀」与「有坐骑/翅膀」两帧，断言：
//!
//! * 有坐骑/翅膀的帧与没有时不同（真实素材缺失时退化为占位方块，差异为 0，测试跳过）；
//! * 层顺序是 Crystal `PlayerObject.Draw` 的顺序：坐骑 → 身体 → 头发 → 翅膀
//!   （`PlayerObject.cs:4884-4911`，本作按要求把翅膀固定在头发之后）；
//! * PNG 非空。

use std::path::{Path, PathBuf};

use mir_core::Point;
use mir_core::enums::MirDirection;
use mir_render::{
    ActorAction, EntityKind, EntityLibraries, EntityView, Frame, MapScene, PlayerLayer,
    TileLibraries, Viewport, player_layer_plan_at,
};
use mir_world::{Map, MapCell, MapVersion};

/// 玩家所在格。
const CENTER: Point = Point::new(50, 50);

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn map() -> Map {
    let width = 128;
    let height = 128;
    Map {
        width,
        height,
        cells: vec![MapCell::default(); usize::try_from(width * height).unwrap_or(0)],
        file_name: "synthetic-mount".to_owned(),
        version: MapVersion::V0,
        doors: Vec::new(),
    }
}

fn player(mounted: bool, center: Point) -> EntityView {
    let mut view = EntityView::new(center, MirDirection::Down, EntityKind::Player, 0);
    view.armour = 0;
    view.hair = 2;
    view.weapon = 3;
    view.weapon_shape = 3;
    if mounted {
        view.mount = 2; // 外观 2（S.ObjectPlayer.mount_type = 1 + 1）
        view.wings = 1; // S.ObjectPlayer.wing_effect = 1
        view.action = ActorAction::Walking;
        view.action_started_ms = 0;
    }
    view
}

fn changed_pixels(left: &Frame, right: &Frame) -> usize {
    left.pixels
        .chunks_exact(4)
        .zip(right.pixels.chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count()
}

#[test]
fn mounted_player_renders_mount_and_wings_to_a_png() {
    let root = repo_root();
    // 运行期只读自有格式：`game-assets/`（拆包 gfx + `.mnm` 地图）。
    let assets_root = root.join("game-assets");
    let data_dir = assets_root.join("data");
    let gfx_dir = assets_root.join("gfx");
    let has_assets = assets_root.join("library-map.json").is_file()
        && gfx_dir.join("sprites/player/mount-1").is_dir();
    if !has_assets {
        eprintln!(
            "跳过：{} 下没有 library-map.json / gfx/sprites/player/mount-1（game-assets/ 被 gitignore）",
            assets_root.display()
        );
        return;
    }
    let library_map = mir_asset::LibraryMap::load(assets_root.join("library-map.json")).ok();

    // 有 `.mnm` 就用它当背景，没有则用合成空地图；两者都只依赖本地素材。
    let (mut scene, center) = match MapScene::load_with_sources(
        assets_root.join("maps/0.mnm"),
        &data_dir,
        Some(gfx_dir.as_path()),
        Some(assets_root.as_path()),
        library_map.as_ref(),
        None,
        false,
    ) {
        Ok(scene) => (scene, Point::new(330, 270)),
        Err(error) => {
            eprintln!("没有可用的 0.mnm，改用合成地图：{error}");
            (
                MapScene::from_parts_with_entities(
                    map(),
                    TileLibraries::empty("."),
                    Viewport::new(800, 600),
                    EntityLibraries::open_with_sources(
                        &data_dir,
                        Some(gfx_dir.as_path()),
                        Some(assets_root.as_path()),
                        library_map.as_ref(),
                        None,
                        false,
                    ),
                ),
                CENTER,
            )
        }
    };

    let plain = player(false, center);
    let mounted = player(true, center);
    // 先断言层顺序（纯数据，不依赖素材）：坐骑最先，翅膀在头发之后。
    let plan = player_layer_plan_at(&mounted, 0);
    let layers: Vec<PlayerLayer> = plan.layers().iter().map(|item| item.layer).collect();
    assert_eq!(layers[0], PlayerLayer::Mount, "坐骑必须画在最下面");
    let hair = layers
        .iter()
        .position(|layer| *layer == PlayerLayer::Hair)
        .expect("有头发层");
    let wing = layers
        .iter()
        .position(|layer| *layer == PlayerLayer::Wing)
        .expect("有翅膀层");
    assert!(wing > hair, "翅膀必须在头发之后：{layers:?}");

    let mut plain_frame = Frame::new(800, 600);
    scene.render_with_entities_at(&mut plain_frame, center, std::slice::from_ref(&plain), 0);
    let mut mounted_frame = Frame::new(800, 600);
    scene.render_with_entities_at(
        &mut mounted_frame,
        center,
        std::slice::from_ref(&mounted),
        0,
    );

    let changed = changed_pixels(&plain_frame, &mounted_frame);
    assert!(
        changed > 200,
        "有坐骑/翅膀的帧必须明显不同，实际 {changed} 像素"
    );
    assert!(
        mounted_frame.distinct_color_count() > 4,
        "帧里应有地形/人物/坐骑/翅膀的颜色，实际 {}",
        mounted_frame.distinct_color_count()
    );

    let output = root.join("target/client-mount.png");
    std::fs::create_dir_all(root.join("target")).ok();
    mounted_frame
        .save_png(&output)
        .expect("写出 target/client-mount.png");
    let size = std::fs::metadata(&output).expect("PNG 应存在").len();
    assert!(size > 1_024, "PNG 不应为空文件，实际 {size} 字节");
    println!(
        "client-mount.png：{} · {}×{} · 素材=真实 · 颜色 {} 种",
        output.display(),
        mounted_frame.width,
        mounted_frame.height,
        mounted_frame.distinct_color_count()
    );
}
