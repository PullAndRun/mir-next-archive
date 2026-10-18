//! 大地图面板（`B`）无头测试：注入 `MapInformation` / `NewMapInfo` → `B` 渲染 → PNG。
//!
//! 流程：
//!
//! 1. 注入 `S.UserInformation`（自己）+ `S.MapInformation`（地图名/大地图号）+
//!    `S.NewMapInfo`（宽高/大地图号）；
//! 2. `UiState::toggle_bigmap()` 模拟 `B`（Crystal `KeybindOptions.Bigmap` →
//!    `BigMapDialog.Toggle()`，`GameScene.cs:773-774`）；
//! 3. 渲染整屏到 `target/client-bigmap.png`，断言面板区域像素变化、非空白、玩家点是黄色；
//! 4. `UiState::escape()` 模拟 `Esc`（窗口层 `app.rs` 就是这么调用的），断言面板关闭；
//! 5. 点面板不会变成「点地走路」；缺素材时视区是纯色占位 + 地图名。
//!
//! 有真实素材时用 `resource/map/0.map` + `resource/data/mmap.wzl`（302 号帧）；
//! 没有素材时用合成地图 + 合成大地图，测试照样跑。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mir_client::geom::Rect;
use mir_client::ui::{self, EscapeResult, UiState, UiText, bigmap};
use mir_core::Language;
use mir_core::Point;
use mir_core::enums::{LightSetting, MirClass, MirDirection, MirGender, WeatherSetting};
use mir_protocol::ServerPacket;
use mir_protocol::data::ClientMapInfo;
use mir_protocol::packets::server::{MapInformation, NewMapInfo, ObjectMonster, UserInformation};
use mir_render::{
    Frame, Image, ImageInfo, MapScene, MinimapScene, PLAYER_COLOR, TileLibraries, Viewport,
};
use mir_world::{Map, MapCell, MapVersion};

/// 仓库根目录。
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// 真实素材里的大地图帧号（`mmap.wzx` 有 3273 帧，302 号是真实地图）。
const REAL_BIG_MAP_FRAME: u16 = 302;

/// 合成地图（没有 `resource/map/0.map` 时用）。
fn synthetic_map() -> Map {
    let width = 256;
    let height = 256;
    Map {
        width,
        height,
        cells: vec![MapCell::default(); usize::try_from(width * height).unwrap_or(0)],
        file_name: "synthetic-bigmap".to_owned(),
        version: MapVersion::V0,
        doors: Vec::new(),
    }
}

/// 合成 300×200 的大地图底图（`from_images` 用）。
fn synthetic_big_map() -> BTreeMap<u16, Image> {
    let (width, height) = (300u16, 200u16);
    let mut pixels = Vec::with_capacity(usize::from(width) * usize::from(height) * 4);
    for y in 0..u32::from(height) {
        for x in 0..u32::from(width) {
            // 渐变底：任何像素都能看出「画了东西」，也方便看 PNG。
            pixels.extend_from_slice(&[
                u8::try_from((x / 2) % 200).unwrap_or(0),
                u8::try_from((y / 2) % 200).unwrap_or(0),
                70,
                255,
            ]);
        }
    }
    let mut images = BTreeMap::new();
    images.insert(
        REAL_BIG_MAP_FRAME,
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

/// 注入角色 + 地图信息（`MapInformation` + `NewMapInfo`）+ 一只怪。
fn snapshot_with_map(big_map: u16) -> mir_client::net::WorldSnapshot {
    let mut snapshot = mir_client::net::WorldSnapshot::default();
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
    snapshot.apply_packet(&ServerPacket::MapInformation(MapInformation {
        map_index: 0,
        file_name: "0".to_owned(),
        title: "比奇省".to_owned(),
        mini_map: 7,
        big_map,
        lights: LightSetting::Day,
        lightning: false,
        fire: false,
        map_dark_light: 0,
        music: 0,
        weather_particles: WeatherSetting::None,
    }));
    snapshot.apply_packet(&ServerPacket::NewMapInfo(NewMapInfo {
        map_index: 0,
        info: ClientMapInfo {
            title: "比奇省".to_owned(),
            width: 700,
            height: 700,
            big_map: i32::from(big_map),
            movements: Vec::new(),
            npcs: Vec::new(),
        },
    }));
    snapshot.apply_packet(&ServerPacket::ObjectMonster(ObjectMonster {
        object_id: 500,
        name: "鸡".to_owned(),
        location: Point::new(340, 270),
        direction: MirDirection::Left,
        image: mir_core::enums::Monster::Hen,
        ..Default::default()
    }));
    snapshot
}

/// 一个能画东西的场景：优先真实素材，缺了就合成。
fn scene_for_test(root: &Path) -> MapScene {
    match MapScene::load(root.join("resource/map/0.map"), root.join("resource/data")) {
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

/// 大地图场景：优先真实 `mmap.wzl` 的 302 号帧，缺了就合成。
fn big_map_scene_for_test(root: &Path) -> MinimapScene {
    let (mut scene, reason) = MinimapScene::open_dir(root.join("resource/data"));
    if scene.image_size(REAL_BIG_MAP_FRAME).is_some() {
        return scene;
    }
    eprintln!("没有真实 mmap.wzl 的 302 号帧（{reason:?}），改用合成大地图");
    MinimapScene::from_images(synthetic_big_map())
}

/// 统计两个帧在矩形内变化的像素数。
fn changed_pixels(before: &Frame, after: &Frame, rect: Rect) -> usize {
    let mut count = 0;
    for y in rect.y..rect.bottom() {
        for x in rect.x..rect.right() {
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
fn pressing_b_renders_the_big_map_panel_and_escape_closes_it() {
    let root = repo_root();
    let mut scene = scene_for_test(&root);
    let mut big_map_scene = big_map_scene_for_test(&root);
    let snapshot = snapshot_with_map(REAL_BIG_MAP_FRAME);
    let center = snapshot
        .self_player
        .as_ref()
        .map_or(Point::new(330, 270), |me| me.position);

    // 世界帧（对照组）。
    let mut frame = Frame::new(800, 600);
    let entities = snapshot.entity_views(0);
    scene.render_with_entities(&mut frame, center, &entities);
    let world_only = frame.clone();

    let text = UiText::load(None);
    let mut state = UiState::new(Language::ZhCn);
    state.minimap_scene = std::mem::replace(&mut big_map_scene, MinimapScene::empty());
    assert!(!state.show_bigmap, "默认关闭");
    state.toggle_bigmap(); // `B`
    assert!(state.show_bigmap, "B 应打开大地图");
    state.set_time(1_000);
    state.mouse = Point::new(20, 20);
    ui::draw(
        &mut state, &mut frame, &mut scene, &snapshot, &text, None, center,
    );

    let output = root.join("target/client-bigmap.png");
    std::fs::create_dir_all(root.join("target")).ok();
    frame
        .save_png(&output)
        .expect("写出 target/client-bigmap.png");
    let metadata = std::fs::metadata(&output).expect("PNG 应存在");
    assert!(metadata.len() > 4_096, "PNG 不应为空文件");

    // 面板居中、尺寸固定；面板区域被覆盖（与「只有世界」的帧比较）。
    let screen = Rect::screen(800, 600);
    let panel = bigmap::panel_rect(screen);
    assert_eq!(
        (panel.width, panel.height),
        (bigmap::PANEL_WIDTH, bigmap::PANEL_HEIGHT)
    );
    assert_eq!(panel.center(), screen.center(), "大地图面板必须居中");
    let changed = changed_pixels(&world_only, &frame, panel);
    assert!(
        changed > 50_000,
        "大地图面板应当覆盖世界画面，实际变化 {changed} 像素"
    );
    assert!(
        frame.distinct_color_count() >= 20,
        "整帧颜色太少：{}",
        frame.distinct_color_count()
    );

    // 视区里有玩家点（黄）与底图颜色（至少不是全黑）。
    let view = bigmap::view_rect(panel);
    let mut yellow = 0;
    let mut non_black = 0;
    for y in view.y..view.bottom() {
        for x in view.x..view.right() {
            let Some(pixel) =
                frame.pixel(u32::try_from(x).unwrap_or(0), u32::try_from(y).unwrap_or(0))
            else {
                continue;
            };
            if pixel == PLAYER_COLOR {
                yellow += 1;
            }
            if pixel != [0, 0, 0, 255] {
                non_black += 1;
            }
        }
    }
    assert!(yellow >= 4, "大地图上应画出黄色玩家点，实际 {yellow} 像素");
    assert!(
        non_black > 50_000,
        "大地图视区应当有图像内容，实际非黑 {non_black} 像素"
    );

    // `Esc`：窗口层（`app.rs`）调用 `UiState::escape()`，大地图最先关。
    let result = state.escape();
    assert_eq!(result, EscapeResult::Bigmap);
    assert!(!state.show_bigmap, "Esc 应关闭大地图");
    assert!(!state.bigmap_clicked(screen));

    eprintln!(
        "写出 {}：{}×{}，面板 {} 像素变化，玩家点 {} 像素，非黑 {} 像素",
        output.display(),
        frame.width,
        frame.height,
        changed,
        yellow,
        non_black
    );
}

#[test]
fn clicking_the_big_map_panel_does_not_walk_or_lock_targets() {
    let snapshot = snapshot_with_map(REAL_BIG_MAP_FRAME);
    let mut state = UiState::new(Language::ZhCn);
    state.toggle_bigmap();
    let screen = Rect::screen(800, 600);
    state.mouse = bigmap::panel_rect(screen).center();
    let action = state.on_click(
        &snapshot,
        &UiText::fallback(),
        screen,
        Some(Point::new(340, 270)), // 怪物格：如果吞点击失败会锁定/走路
    );
    assert!(action.is_none(), "点大地图面板不应产生任何动作：{action:?}");
    assert_eq!(state.target_object_id, None, "点面板不应锁定目标");
}

#[test]
fn missing_assets_draw_a_placeholder_with_the_map_name() {
    let snapshot = snapshot_with_map(REAL_BIG_MAP_FRAME);
    let mut scene = MapScene::from_parts(
        synthetic_map(),
        TileLibraries::empty("."),
        Viewport::DEFAULT,
    );
    let mut frame = Frame::new(800, 600);
    let text = UiText::fallback();
    let mut state = UiState::new(Language::ZhCn);
    // `UiText::fallback()` 的 3×5 点阵只有 ASCII；用英文标签保证「占位区里有文字」
    // 这条断言在没有 TrueType 字体的环境也成立（地图名本身来自服务端，可能是中文）。
    state.language = Language::En;
    // 故意不注入图库：`minimap_scene` 保持默认空场景。
    state.toggle_bigmap();
    state.set_time(1_000);
    let center = Point::new(330, 270);
    ui::draw(
        &mut state, &mut frame, &mut scene, &snapshot, &text, None, center,
    );

    let screen = Rect::screen(800, 600);
    let view = bigmap::view_rect(bigmap::panel_rect(screen));
    assert_eq!(
        frame.pixel(
            u32::try_from(view.x + 5).unwrap(),
            u32::try_from(view.y + 5).unwrap()
        ),
        Some(bigmap::PLACEHOLDER_COLOR),
        "缺素材时视区应当是纯色占位"
    );
    // 占位区里应有地图名/提示文字（亮色像素）。
    let mut light = 0;
    for y in view.y..view.bottom() {
        for x in view.x..view.right() {
            if let Some([r, g, b, _]) =
                frame.pixel(u32::try_from(x).unwrap_or(0), u32::try_from(y).unwrap_or(0))
            {
                if r > 150 && g > 150 && b > 150 {
                    light += 1;
                }
            }
        }
    }
    assert!(light > 20, "占位区域应画出地图名/提示，实际 {light} 像素");
}

#[test]
fn zero_big_map_index_still_opens_the_panel_with_a_placeholder() {
    // `MapInfo.BigMap == 0`：Crystal 的 `BigMapDialog.Show` 直接返回（`BigMapDialog.cs:289`），
    // 本作改为打开面板画占位（需求：无素材时画纯色占位 + 地图名）。
    let snapshot = snapshot_with_map(0);
    let mut scene = MapScene::from_parts(
        synthetic_map(),
        TileLibraries::empty("."),
        Viewport::DEFAULT,
    );
    let mut frame = Frame::new(800, 600);
    let mut state = UiState::new(Language::ZhCn);
    // 即使给了图库，0 号图也不该被当成大地图（与 `mmap.wzl` 的 0 号帧是同名陷阱）。
    state.minimap_scene = MinimapScene::from_images(synthetic_big_map());
    state.toggle_bigmap();
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
    let view = bigmap::view_rect(bigmap::panel_rect(screen));
    assert_eq!(
        frame.pixel(
            u32::try_from(view.x + 5).unwrap(),
            u32::try_from(view.y + 5).unwrap()
        ),
        Some(bigmap::PLACEHOLDER_COLOR),
        "big_map == 0 时应画占位而不是 302 号图"
    );
}
