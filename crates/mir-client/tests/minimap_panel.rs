//! 小地图面板（`M`）无头测试：注入 `MapInformation` / `NewMapInfo` → 面板状态 → PNG。
//!
//! 流程：
//!
//! 1. 注入 `S.UserInformation`（自己）+ `S.MapInformation`（地图名/小地图号）+
//!    `S.NewMapInfo`（宽高/大地图号）+ 若干 `S.ObjectMonster`；
//! 2. 断言 [`UiState::minimap_scene`] 的图库与 `view_for` 的取数（`MiniMap` 号、宽高、点位）；
//! 3. 打开 `state.show_minimap`，渲染地图 + UI 到 `target/client-minimap.png`；
//! 4. 断言：面板确实覆盖了地图右上角、玩家点是黄色、点面板不会变成走路。
//!
//! 有真实素材时用 `resource/map/0.map` + `resource/data/mmap.wzl`（真实小地图，
//! 帧号换成真实存在的 7 号帧）；没有素材时用合成地图 + 合成小地图图库，测试照样跑。

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mir_client::geom::Rect;
use mir_client::net::{WorldEntity, WorldSnapshot};
use mir_client::ui::minimap::{PANEL_HEIGHT, PANEL_WIDTH, panel_rect, view_for, view_rect};
use mir_client::ui::{self, UiState, UiText};
use mir_core::enums::{LightSetting, MirClass, MirDirection, MirGender, WeatherSetting};
use mir_core::{Language, Point};
use mir_protocol::ServerPacket;
use mir_protocol::data::ClientMapInfo;
use mir_protocol::packets::server::{MapInformation, NewMapInfo, ObjectMonster, UserInformation};
use mir_render::{
    EntityKind, Frame, Image, ImageInfo, MapScene, MinimapScene, PLAYER_COLOR, TileLibraries,
    Viewport,
};
use mir_world::{Map, MapCell, MapVersion};

/// 仓库根目录。
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// 真实素材里可用的 mmap 帧号（`resource/data/mmap.wzl` 的 7 号帧是 300×199 的小地图）。
const REAL_MINIMAP_FRAME: u16 = 7;

/// 合成地图（没有 `resource/map/0.map` 时用）。
fn synthetic_map() -> Map {
    let width = 256;
    let height = 256;
    Map {
        width,
        height,
        cells: vec![MapCell::default(); usize::try_from(width * height).unwrap_or(0)],
        file_name: "synthetic-minimap".to_owned(),
        version: MapVersion::V0,
        doors: Vec::new(),
    }
}

/// 合成 300×199 的小地图底图（`from_images` 用）。
fn synthetic_minimap() -> BTreeMap<u16, Image> {
    let (width, height) = (300u16, 199u16);
    let mut pixels = Vec::with_capacity(usize::from(width) * usize::from(height) * 4);
    for y in 0..u32::from(height) {
        for x in 0..u32::from(width) {
            // 渐变底，任何像素都能看出「画了东西」。
            pixels.extend_from_slice(&[
                u8::try_from(x % 200).unwrap_or(0),
                u8::try_from(y % 200).unwrap_or(0),
                60,
                255,
            ]);
        }
    }
    let mut images = BTreeMap::new();
    images.insert(
        REAL_MINIMAP_FRAME,
        Image {
            info: ImageInfo {
                width,
                height,
                offset_x: 0,
                offset_y: 0,
                format: 0,
                shadow_x: 0,
                shadow_y: 0,
                reserved: [0; 3],
                has_mask: false,
                data_offset: 0,
                data_len: 0,
            },
            pixels,
        },
    );
    images
}

/// 注入角色 + 地图信息 + 怪物。
fn snapshot_with_map(mini_map: u16) -> WorldSnapshot {
    let mut snapshot = WorldSnapshot::default();
    snapshot.apply_packet(&ServerPacket::UserInformation(UserInformation {
        object_id: 7,
        name: "测试者".to_owned(),
        class: MirClass::Warrior,
        gender: MirGender::Male,
        level: 12,
        location: Point::new(330, 270),
        direction: MirDirection::Down,
        hair: 1,
        hp: 50,
        mp: 30,
        ..Default::default()
    }));
    // `S.MapInformation`：地图名 + 小地图号（`Shared/ServerPackets.cs:479-491`）。
    snapshot.apply_packet(&ServerPacket::MapInformation(MapInformation {
        map_index: 0,
        file_name: "0".to_owned(),
        title: "比奇省".to_owned(),
        mini_map,
        big_map: 302,
        lights: LightSetting::Day,
        lightning: false,
        fire: false,
        map_dark_light: 0,
        music: 0,
        weather_particles: WeatherSetting::None,
    }));
    // `S.NewMapInfo`：宽高 + 大地图号（`ClientData.cs:320-336`）。
    snapshot.apply_packet(&ServerPacket::NewMapInfo(NewMapInfo {
        map_index: 0,
        info: ClientMapInfo {
            title: "比奇省".to_owned(),
            width: 700,
            height: 700,
            big_map: 302,
            movements: Vec::new(),
            npcs: Vec::new(),
        },
    }));
    // 一只怪（小地图上应当是红点）。
    snapshot.apply_packet(&ServerPacket::ObjectMonster(ObjectMonster {
        object_id: 500,
        name: "鸡".to_owned(),
        location: Point::new(340, 270),
        direction: MirDirection::Left,
        image: mir_core::enums::Monster::Hen,
        ..Default::default()
    }));
    snapshot.entities.insert(
        600,
        WorldEntity::new(
            600,
            EntityKind::Npc,
            "药店老板".to_owned(),
            Point::new(320, 270),
            MirDirection::Down,
            3,
        ),
    );
    snapshot
}

/// 一个能画东西的场景：优先真实素材，缺了就合成。
fn scene_for_test(root: &Path) -> MapScene {
    let map_path = root.join("resource/map/0.map");
    let data_dir = root.join("resource/data");
    match MapScene::load(&map_path, &data_dir) {
        Ok(scene) => scene,
        Err(error) => {
            eprintln!("没有真实地图素材（{error}），改用合成地图");
            MapScene::from_parts(
                synthetic_map(),
                TileLibraries::empty("."),
                Viewport::DEFAULT,
            )
        }
    }
}

/// 一个小地图场景：优先真实 `mmap.wzl`，缺了就合成。
fn minimap_scene_for_test(root: &Path) -> MinimapScene {
    let (scene, reason) = MinimapScene::open_dir(root.join("resource/data"));
    if scene.is_available() {
        return scene;
    }
    eprintln!("没有真实 mmap.wzl（{reason:?}），改用合成小地图");
    MinimapScene::from_images(synthetic_minimap())
}

#[test]
fn injected_map_information_drives_the_minimap_panel() {
    let snapshot = snapshot_with_map(REAL_MINIMAP_FRAME);
    let view = view_for(&snapshot, (700, 700));
    assert_eq!(
        view.image_index, REAL_MINIMAP_FRAME,
        "图像号来自 MapInformation"
    );
    assert_eq!(
        (view.map_width, view.map_height),
        (700, 700),
        "宽高来自 NewMapInfo"
    );
    assert_eq!(view.player, Point::new(330, 270));
    // 怪物进点位、物品不进；NPC 也在。
    assert_eq!(view.markers.len(), 2, "怪物 + NPC：{:?}", view.markers);
    assert!(
        view.markers
            .iter()
            .any(|marker| marker.cell == Point::new(340, 270)),
        "怪物应当有一个点"
    );

    // `M` 键（`app.rs`）最终只是翻这个开关；这里直接验证状态与命中测试。
    let mut state = UiState::new(Language::ZhCn);
    assert!(!state.show_minimap, "默认关闭");
    state.toggle_minimap();
    assert!(state.show_minimap);
    let screen = Rect::screen(800, 600);
    let panel = panel_rect(screen);
    state.mouse = Point::new(panel.x + 5, panel.y + 5);
    assert!(state.minimap_clicked(screen), "鼠标在面板上");
    // 面板上的点击不会被解释成走路。
    let action = state.on_click(
        &snapshot,
        &UiText::fallback(),
        screen,
        Some(Point::new(340, 270)),
    );
    assert!(action.is_none(), "点小地图面板不应产生任何动作：{action:?}");
}

#[test]
fn renders_minimap_panel_to_png() {
    let root = repo_root();
    let mut scene = scene_for_test(&root);
    let mut minimap_scene = minimap_scene_for_test(&root);
    // 真实素材：把快照的小地图号换成真实存在的帧；合成素材本来就用了这个帧。
    let snapshot = snapshot_with_map(REAL_MINIMAP_FRAME);
    let center = snapshot
        .self_player
        .as_ref()
        .map_or(Point::new(330, 270), |me| me.position);

    let mut frame = Frame::new(800, 600);
    let entities = snapshot.entity_views(0);
    scene.render_with_entities(&mut frame, center, &entities);
    let world_only = frame.clone();

    let text = common::load_text();
    let mut state = UiState::new(Language::ZhCn);
    state.minimap_scene = std::mem::replace(&mut minimap_scene, MinimapScene::empty());
    state.show_minimap = true;
    state.set_time(1_000);
    state.mouse = Point::new(400, 300);
    ui::draw(
        &mut state, &mut frame, &mut scene, &snapshot, &text, None, center,
    );

    let output = root.join("target/client-minimap.png");
    std::fs::create_dir_all(root.join("target")).ok();
    frame
        .save_png(&output)
        .expect("写出 target/client-minimap.png");
    let metadata = std::fs::metadata(&output).expect("PNG 应存在");
    assert!(metadata.len() > 4_096, "PNG 不应为空文件");

    // 面板覆盖了右上角（与「只有世界」的帧比较）。
    let screen = Rect::screen(800, 600);
    let panel = panel_rect(screen);
    assert_eq!((panel.width, panel.height), (PANEL_WIDTH, PANEL_HEIGHT));
    let mut changed = 0;
    let mut yellow = 0;
    for y in panel.y..panel.bottom() {
        for x in panel.x..panel.right() {
            let pixel = frame
                .pixel(u32::try_from(x).unwrap(), u32::try_from(y).unwrap())
                .expect("面板内像素");
            let before = world_only
                .pixel(u32::try_from(x).unwrap(), u32::try_from(y).unwrap())
                .expect("世界帧像素");
            if pixel != before {
                changed += 1;
            }
            if pixel == PLAYER_COLOR {
                yellow += 1;
            }
        }
    }
    assert!(
        changed > 5_000,
        "小地图面板应当覆盖右上角，实际变化 {changed} 像素"
    );
    assert!(
        yellow >= 4,
        "玩家点（黄）应当画在视窗里，实际 {yellow} 像素"
    );

    // 视窗矩形必须落在面板内。
    let view = view_rect(panel);
    assert!(panel.intersect(view).is_some());
    eprintln!(
        "写出 {}：{}×{}，面板变化 {changed} 像素，玩家点 {yellow} 像素",
        output.display(),
        frame.width,
        frame.height
    );
}

#[test]
fn missing_assets_render_a_placeholder_panel() {
    let root = repo_root();
    let mut scene = MapScene::from_parts(
        synthetic_map(),
        TileLibraries::empty("."),
        Viewport::DEFAULT,
    );
    let snapshot = snapshot_with_map(REAL_MINIMAP_FRAME);
    let mut frame = Frame::new(800, 600);
    let text = UiText::fallback();
    let mut state = UiState::new(Language::ZhCn);
    // 故意不注入图库：`minimap_scene` 保持默认空场景。
    state.show_minimap = true;
    state.set_time(1_000);
    let center = Point::new(330, 270);
    ui::draw(
        &mut state, &mut frame, &mut scene, &snapshot, &text, None, center,
    );

    let screen = Rect::screen(800, 600);
    let view = view_rect(panel_rect(screen));
    let placeholder = frame
        .pixel(
            u32::try_from(view.x + 5).unwrap(),
            u32::try_from(view.y + 5).unwrap(),
        )
        .expect("占位像素");
    assert_eq!(
        placeholder,
        mir_client::ui::minimap::PLACEHOLDER_COLOR,
        "缺素材时视窗应当是纯色占位"
    );
    let _ = root;
}

#[test]
fn zero_minimap_index_means_no_image() {
    // `MiniMap == 0`：Crystal 认为这张图没有小地图（`MainDialogs.cs:1898`）。
    let snapshot = snapshot_with_map(0);
    let view = view_for(&snapshot, (700, 700));
    assert_eq!(view.image_index, 0);
    let mut scene = MapScene::from_parts(
        synthetic_map(),
        TileLibraries::empty("."),
        Viewport::DEFAULT,
    );
    let mut frame = Frame::new(800, 600);
    let mut state = UiState::new(Language::ZhCn);
    // 即使塞了一张 0 号帧的图，也不该被画出来。
    let mut images = BTreeMap::new();
    images.insert(0, synthetic_minimap().remove(&REAL_MINIMAP_FRAME).unwrap());
    state.minimap_scene = MinimapScene::from_images(images);
    state.show_minimap = true;
    state.set_time(1_000);
    ui::draw(
        &mut state,
        &mut frame,
        &mut scene,
        &snapshot,
        &UiText::fallback(),
        None,
        Point::new(330, 270),
    );
    let screen = Rect::screen(800, 600);
    let view_rect = view_rect(panel_rect(screen));
    assert_eq!(
        frame.pixel(
            u32::try_from(view_rect.x + 5).unwrap(),
            u32::try_from(view_rect.y + 5).unwrap()
        ),
        Some(mir_client::ui::minimap::PLACEHOLDER_COLOR),
        "0 号图应当被当成「没有小地图」而画占位"
    );
}
