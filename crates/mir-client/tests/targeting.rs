//! 鼠标目标 / 头顶血条的无头测试（注入式，不连服务器）。
//!
//! 覆盖需求：
//!
//! 1. 注入 `S.UserInformation` + 两只 `S.ObjectMonster` + `S.ObjectHealth`，
//!    把鼠标移到怪物格上：`UiState::hover_object` 返回该 object id（命中测试）；
//! 2. 渲染后怪物头顶有 32×4 血条（`MapObject.cs:506`），颜色按百分比；
//! 3. 左键单击怪物 → `UiState::target_object_id` 被设置（`GameScene.cs:11442-11451`）；
//! 4. 双击相邻怪物 → `UiAction::AttackTarget` 且 `net_command()` 是 `NetCommand::Attack`；
//! 5. 双击远处怪物 → 先走一步（`NetCommand::Walk`），最多连续 3 步后放弃；
//! 6. 点背包/面板/小地图不会变成走路（既有行为保持）。
//!
//! 输出 `target/client-target.png`（悬停高亮 + 血条 + 锁定目标框同时可见）。

use std::path::{Path, PathBuf};

use mir_client::geom::Rect;
use mir_client::net::{NetCommand, WorldSnapshot};
use mir_client::ui::{self, UiAction, UiState, UiText, inventory};
use mir_core::Language;
use mir_core::Point;
use mir_core::enums::{MirClass, MirDirection, MirGender};
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::{ObjectHealth, ObjectMonster, UserInformation};
use mir_render::{
    CELL_HEIGHT, CELL_WIDTH, Frame, HEALTH_YELLOW, MapScene, TileLibraries, Viewport,
};
use mir_world::{Map, MapCell, MapVersion};

/// 远处的怪物（2 格）。
const FAR_MONSTER_ID: u32 = 90_100;
/// 相邻的怪物（1 格）。
const NEAR_MONSTER_ID: u32 = 90_101;
/// 自己站在这个格子上。
const SELF_CELL: Point = Point::new(330, 270);

/// 仓库根目录。
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// 合成地图（测试不依赖 `resource/`，渲染结果确定）。
fn synthetic_map() -> Map {
    let width = 64;
    let height = 64;
    Map {
        width,
        height,
        cells: vec![MapCell::default(); usize::try_from(width * height).unwrap_or(0)],
        file_name: "synthetic-targeting".to_owned(),
        version: MapVersion::V0,
        doors: Vec::new(),
    }
}

/// 注入角色 + 两只怪物 + 两条 `ObjectHealth`。
fn snapshot() -> WorldSnapshot {
    let mut snapshot = WorldSnapshot::default();
    snapshot.apply_packet(&ServerPacket::UserInformation(UserInformation {
        object_id: 7,
        name: "测试者".to_owned(),
        class: MirClass::Warrior,
        gender: MirGender::Male,
        level: 10,
        location: SELF_CELL,
        direction: MirDirection::Down,
        hp: 40,
        mp: 20,
        ..Default::default()
    }));
    snapshot.apply_packet(&ServerPacket::ObjectMonster(ObjectMonster {
        object_id: FAR_MONSTER_ID,
        name: "稻草人".to_owned(),
        image: mir_core::enums::Monster::Scarecrow,
        location: SELF_CELL.moved(MirDirection::Right, 2),
        direction: MirDirection::Left,
        ..ObjectMonster::default()
    }));
    snapshot.apply_packet(&ServerPacket::ObjectHealth(ObjectHealth {
        object_id: FAR_MONSTER_ID,
        percent: 40,
        expire: 5,
    }));
    snapshot.apply_packet(&ServerPacket::ObjectMonster(ObjectMonster {
        object_id: NEAR_MONSTER_ID,
        name: "鸡".to_owned(),
        image: mir_core::enums::Monster::Hen,
        location: SELF_CELL.moved(MirDirection::Right, 1),
        direction: MirDirection::Left,
        ..ObjectMonster::default()
    }));
    snapshot.apply_packet(&ServerPacket::ObjectHealth(ObjectHealth {
        object_id: NEAR_MONSTER_ID,
        percent: 90,
        expire: 5,
    }));
    snapshot
}

/// 合成场景。
fn scene() -> MapScene {
    MapScene::from_parts(
        synthetic_map(),
        TileLibraries::empty("."),
        Viewport::DEFAULT,
    )
}

/// 某个格子的屏幕中心。
fn cell_center(scene: &MapScene, cell: Point) -> Point {
    let (x, y) = scene.world_to_screen(SELF_CELL, cell);
    Point::new(x + CELL_WIDTH / 2, y + CELL_HEIGHT / 2)
}

#[test]
fn hovering_a_monster_hits_it_and_draws_an_overhead_health_bar() {
    let snapshot = snapshot();
    let mut scene = scene();
    let text = UiText::fallback();
    let screen = Rect::screen(800, 600);
    let far_cell = snapshot.entities[&FAR_MONSTER_ID].position;

    let mut state = UiState::new(Language::ZhCn);
    state.set_time(1_000);
    state.mouse = cell_center(&scene, far_cell);
    assert_eq!(
        state
            .hover_object(&snapshot, &scene, &text, SELF_CELL, screen)
            .map(|entity| entity.object_id),
        Some(FAR_MONSTER_ID),
        "鼠标在怪物格上时命中测试应返回该 object id"
    );
    // 鼠标在空地上没有命中。
    state.mouse = cell_center(&scene, Point::new(330, 274));
    assert!(
        state
            .hover_object(&snapshot, &scene, &text, SELF_CELL, screen)
            .is_none()
    );

    // 画世界 + UI 一帧。
    let mut frame = Frame::new(800, 600);
    let entities = snapshot.entity_views(0);
    scene.render_with_entities(&mut frame, SELF_CELL, &entities);
    // 血条并不依赖 UI：渲染层按 `hp_percent` 画（`EntityRenderer::render_at`）。
    state.mouse = cell_center(&scene, far_cell);
    ui::draw(
        &mut state, &mut frame, &mut scene, &snapshot, &text, None, SELF_CELL,
    );
    let (origin_x, origin_y) = scene.world_to_screen(SELF_CELL, far_cell);
    // 没素材时实体是占位方块（`draw_entity` 返回 `None`），血条画在格原点上方 6 px。
    let bar_y = origin_y - 6;
    let bar = Rect::new(
        origin_x + 8,
        bar_y,
        u32::try_from(40 * 32 / 100).unwrap_or(0),
        4,
    );
    let mut bar_pixels = 0;
    for y in bar.y..bar.bottom() {
        for x in bar.x..bar.right() {
            if let Some(pixel) =
                frame.pixel(u32::try_from(x).unwrap_or(0), u32::try_from(y).unwrap_or(0))
            {
                if pixel == HEALTH_YELLOW {
                    bar_pixels += 1;
                }
            }
        }
    }
    assert!(
        bar_pixels > 0,
        "40% 血的怪物头顶应有黄色血条像素，实际 {bar_pixels}"
    );

    // 悬停高亮：格左上角应当被画成悬停色（`ui::target::HOVER_COLOR`）。
    assert_eq!(
        frame.pixel(
            u32::try_from(origin_x).unwrap(),
            u32::try_from(origin_y).unwrap()
        ),
        Some(mir_client::ui::target::HOVER_COLOR),
        "悬停实体的格子应画出 1px 黄框"
    );

    let output = repo_root().join("target/client-target.png");
    std::fs::create_dir_all(repo_root().join("target")).ok();
    frame
        .save_png(&output)
        .expect("写出 target/client-target.png");
    // 合成地图是纯黑底，PNG 压缩后不大；只要不是空文件即可。
    let size = std::fs::metadata(&output).expect("PNG 应存在").len();
    assert!(size > 512, "PNG 不应为空文件，实际 {size} 字节");
    assert!(
        frame.distinct_color_count() >= 5,
        "整帧颜色太少：{}",
        frame.distinct_color_count()
    );
}

#[test]
fn clicking_a_monster_locks_it_and_double_clicking_an_adjacent_one_attacks() {
    let snapshot = snapshot();
    let text = UiText::fallback();
    let screen = Rect::screen(800, 600);
    let mut state = UiState::new(Language::ZhCn);
    state.set_time(1_000);

    // 单击远处怪物：只锁定（`GameScene.cs:11442-11451`）。
    let far_cell = snapshot.entities[&FAR_MONSTER_ID].position;
    let action = state.on_click(&snapshot, &text, screen, Some(far_cell));
    assert!(action.is_none(), "单击怪物不应发网络包");
    assert_eq!(state.target_object_id, Some(FAR_MONSTER_ID));

    // 双击相邻的鸡：直接攻击。
    let near_cell = snapshot.entities[&NEAR_MONSTER_ID].position;
    state.set_time(2_000);
    assert!(
        state
            .on_click(&snapshot, &text, screen, Some(near_cell))
            .is_none()
    );
    assert_eq!(state.target_object_id, Some(NEAR_MONSTER_ID));
    state.set_time(2_100);
    let action = state
        .on_click(&snapshot, &text, screen, Some(near_cell))
        .expect("双击相邻怪物应攻击");
    assert_eq!(
        action,
        UiAction::AttackTarget {
            direction: MirDirection::Right
        }
    );
    assert_eq!(
        action.net_command(),
        Some(NetCommand::Attack {
            direction: MirDirection::Right,
            variant: None,
            native_action: None,
        }),
        "双击相邻怪物应产生 NetCommand::Attack"
    );
}

#[test]
fn double_clicking_a_distant_monster_walks_up_to_three_steps_then_gives_up() {
    let snapshot = snapshot();
    let text = UiText::fallback();
    let screen = Rect::screen(800, 600);
    let far_cell = snapshot.entities[&FAR_MONSTER_ID].position;
    let mut state = UiState::new(Language::ZhCn);
    state.set_time(1_000);
    assert!(
        state
            .on_click(&snapshot, &text, screen, Some(far_cell))
            .is_none()
    );
    state.set_time(1_100);
    let first = state
        .on_click(&snapshot, &text, screen, Some(far_cell))
        .expect("双击远处怪物应先走一步");
    assert_eq!(
        first,
        UiAction::WorldMove {
            direction: MirDirection::Right,
            step: true
        }
    );
    // `WorldMove` 的 `step` 由 `app.rs` 按当前朝向折算成 `C.Walk`/`C.Turn`
    // （与点地走路共用一条路径），所以这里不直接断言 `net_command()`。
    assert!(first.net_command().is_none());
    // 600 ms 一步，共 3 步（含上面这一步）后放弃。
    state.set_time(1_700);
    assert!(matches!(
        state.tick_approach(&snapshot),
        Some(UiAction::WorldMove { step: true, .. })
    ));
    state.set_time(2_300);
    assert!(matches!(
        state.tick_approach(&snapshot),
        Some(UiAction::WorldMove { step: true, .. })
    ));
    state.set_time(2_900);
    assert_eq!(state.tick_approach(&snapshot), None, "走满 3 步后放弃");
    assert!(state.approach.is_none());
    assert_eq!(state.target_object_id, Some(FAR_MONSTER_ID));
}

#[test]
fn clicking_panels_or_the_minimap_never_walks() {
    let snapshot = snapshot();
    let text = UiText::fallback();
    let screen = Rect::screen(800, 600);
    let mut state = UiState::new(Language::ZhCn);
    state.set_time(1_000);
    // 背包面板。
    state.show_inventory = true;
    let panel = inventory::inventory_panel_rect(screen);
    state.mouse = panel.center();
    assert!(
        state
            .on_click(&snapshot, &text, screen, Some(Point::new(333, 273)))
            .is_none(),
        "点背包面板不应走路"
    );
    state.show_inventory = false;
    // 小地图面板。
    state.toggle_minimap();
    let minimap_panel = mir_client::ui::minimap::panel_rect(screen);
    state.mouse = minimap_panel.center();
    assert!(
        state
            .on_click(&snapshot, &text, screen, Some(Point::new(333, 273)))
            .is_none(),
        "点小地图面板不应走路"
    );
}
