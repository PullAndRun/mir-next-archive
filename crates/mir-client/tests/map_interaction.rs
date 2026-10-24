//! 第 18 轮：小/大地图悬停与点击（精确文本/坐标，合成夹具，不依赖 resource）。

use std::collections::BTreeMap;

use mir_client::geom::Rect;
use mir_client::net::{MapSnapshot, SelfPlayer, WorldEntity, WorldSnapshot};
use mir_client::ui::{UiState, bigmap, minimap};
use mir_core::enums::{MirClass, MirDirection, MirGender};
use mir_core::{Language, Point};
use mir_render::{EntityKind, Image, ImageInfo, MinimapScene, Viewport};

fn solid(width: u16, height: u16, color: [u8; 4]) -> Image {
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
        pixels: color
            .iter()
            .copied()
            .cycle()
            .take(usize::from(width) * usize::from(height) * 4)
            .collect(),
    }
}

fn map_scene() -> MinimapScene {
    let mut images = BTreeMap::new();
    images.insert(9, solid(120, 108, [40, 60, 80, 255]));
    images.insert(302, solid(300, 200, [30, 40, 50, 255]));
    MinimapScene::from_images(images)
}

#[allow(clippy::field_reassign_with_default)]
fn snapshot() -> WorldSnapshot {
    let mut snapshot = WorldSnapshot::default();
    snapshot.map = Some(MapSnapshot {
        map_index: 0,
        file_name: "0".to_owned(),
        title: "比奇省".to_owned(),
        width: 700,
        height: 700,
        mini_map: 9,
        big_map: 302,
        music: 0,
        movements: vec![mir_protocol::data::ClientMovementInfo {
            destination: 4,
            title: "比奇城".to_owned(),
            location: Point::new(330, 270),
            icon: 0,
        }],
    });
    snapshot.self_player = Some(SelfPlayer {
        object_id: 1,
        real_id: 1,
        name: "测试者".to_owned(),
        position: Point::new(320, 260),
        direction: MirDirection::Down,
        hp: 10,
        mp: 10,
        hp_max: 10,
        mp_max: 10,
        level: 1,
        class: MirClass::Warrior,
        gender: MirGender::Male,
        hair: 0,
        gold: 0,
        credit: 0,
        dead: false,
        action: mir_render::ActorAction::Standing,
        action_started_ms: 0,
        attack_variant: None,
        looks: mir_client::net::PlayerLooks::default(),
    });
    snapshot.entities.insert(
        77,
        WorldEntity::new(
            77,
            EntityKind::Npc,
            "商人张三".to_owned(),
            Point::new(300, 300),
            MirDirection::Down,
            1,
        ),
    );
    snapshot
}

fn screen() -> Rect {
    Rect::screen(800, 600)
}

#[test]
fn minimap_hover_teleport_shows_exact_name_and_coords() {
    let snapshot = snapshot();
    let mut scene = map_scene();
    let view = minimap::view_for(&snapshot, (700, 700));
    let mut scene_for_projection = map_scene();
    let projection = scene_for_projection.projection(&view).expect("小地图投影");
    let area = minimap::view_rect(minimap::panel_rect(screen()));
    let (dx, dy) = projection.cell_to_view(Point::new(330, 270));
    let mouse = Point::new(area.x + dx, area.y + dy);
    let hover = minimap::hover_text(
        &snapshot,
        &view,
        &mut scene,
        screen(),
        mouse,
        Language::ZhCn,
        true,
    );
    assert_eq!(hover.as_deref(), Some("传送点：比奇城 (330,270)"));
}

#[test]
fn minimap_hover_npc_shows_exact_name_and_type() {
    let snapshot = snapshot();
    let mut scene = map_scene();
    let view = minimap::view_for(&snapshot, (700, 700));
    let mut scene_for_projection = map_scene();
    let projection = scene_for_projection.projection(&view).expect("小地图投影");
    let area = minimap::view_rect(minimap::panel_rect(screen()));
    let (dx, dy) = projection.cell_to_view(Point::new(300, 300));
    let mouse = Point::new(area.x + dx, area.y + dy);
    let hover = minimap::hover_text(
        &snapshot,
        &view,
        &mut scene,
        screen(),
        mouse,
        Language::ZhCn,
        true,
    );
    assert_eq!(hover.as_deref(), Some("NPC：商人张三"));
}

#[test]
fn bigmap_click_known_cell_sets_destination() {
    let snapshot = snapshot();
    let mut scene = map_scene();
    let view = minimap::big_view_for(&snapshot, (700, 700));
    let projection = scene.big_map_projection(&view).expect("大地图投影");
    let area = bigmap::view_rect(bigmap::panel_rect(screen()));
    let (dx, dy) = projection.cell_to_view(Point::new(330, 270));
    let mut state = UiState::new(Language::ZhCn);
    state.show_bigmap = true;
    state.mouse = Point::new(area.x + dx, area.y + dy);
    let movements = &snapshot.map.as_ref().expect("map").movements;
    assert!(bigmap::click_destination(
        &mut state,
        area,
        &projection,
        movements
    ));
    assert_eq!(
        state.bigmap_destination,
        Some(bigmap::MapDestination {
            map: "比奇城".to_owned(),
            cell: Point::new(330, 270),
        })
    );
}

#[test]
fn minimap_hover_empty_cell_shows_no_tooltip() {
    let snapshot = snapshot();
    let mut scene = map_scene();
    let view = minimap::view_for(&snapshot, (700, 700));
    let mut scene_for_projection = map_scene();
    let projection = scene_for_projection.projection(&view).expect("小地图投影");
    let area = minimap::view_rect(minimap::panel_rect(screen()));
    let (dx, dy) = projection.cell_to_view(Point::new(400, 500));
    let mouse = Point::new(area.x + dx, area.y + dy);
    let hover = minimap::hover_text(
        &snapshot,
        &view,
        &mut scene,
        screen(),
        mouse,
        Language::ZhCn,
        true,
    );
    assert_eq!(hover, None);
}

#[test]
fn fixture_does_not_need_real_assets() {
    // 合成 120×108 图像即可投影；没有 `resource/` 也能跑。
    assert_eq!(Viewport::DEFAULT.width, 800);
    let mut scene = MinimapScene::empty();
    let snapshot = snapshot();
    let view = minimap::view_for(&snapshot, (700, 700));
    assert!(scene.projection(&view).is_none());
}
