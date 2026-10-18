//! A* 点地走路与地面物品拾取的无头测试（注入式，不连服务器）。
//!
//! 覆盖需求：
//!
//! 1. 点到空地：A* 返回多步路径，`UiState` 每 600 ms 发出顺序正确的 `C.Walk`；
//! 2. 绕障碍：路径每一步都可走（A* 单元测试覆盖细节，这里验证客户端真的用它）；
//! 3. 不可达：退化为「朝目标方向走一步」；
//! 4. 悬停地面物品：`hover_item` 返回物品名，渲染帧里出现物品名文字；
//! 5. 点击相邻物品：`UiAction::PickUp` → `NetCommand::PickUp`；
//! 6. 点击远处物品：先靠近（`WorldMove`），相邻后自动 `PickUp`。
//!
//! 输出 `target/client-pickup.png`（悬停物品名 + 角色 + 物品图标）。

use std::path::{Path, PathBuf};

use mir_client::geom::Rect;
use mir_client::net::{NetCommand, WorldSnapshot};
use mir_client::ui::{self, UiAction, UiState, UiText};
use mir_core::Language;
use mir_core::Point;
use mir_core::enums::{MirClass, MirDirection, MirGender};
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::{ObjectItem, UserInformation};
use mir_render::{CELL_HEIGHT, CELL_WIDTH, Frame, MapScene, TileLibraries, Viewport};
use mir_world::{Map, MapCell, MapVersion};

/// 自己站的位置。
const SELF_CELL: Point = Point::new(10, 10);
/// 相邻的物品。
const NEAR_ITEM_ID: u32 = 70_001;
/// 远处的物品。
const FAR_ITEM_ID: u32 = 70_002;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn synthetic_map() -> Map {
    let width = 64;
    let height = 64;
    Map {
        width,
        height,
        cells: vec![MapCell::default(); usize::try_from(width * height).unwrap_or(0)],
        file_name: "synthetic-path".to_owned(),
        version: MapVersion::V0,
        doors: Vec::new(),
    }
}

fn scene() -> MapScene {
    MapScene::from_parts(
        synthetic_map(),
        TileLibraries::empty("."),
        Viewport::DEFAULT,
    )
}

fn snapshot() -> WorldSnapshot {
    let mut snapshot = WorldSnapshot::default();
    snapshot.apply_packet(&ServerPacket::UserInformation(UserInformation {
        object_id: 7,
        name: "走路的人".to_owned(),
        class: MirClass::Warrior,
        gender: MirGender::Male,
        level: 10,
        location: SELF_CELL,
        direction: MirDirection::Down,
        hp: 40,
        mp: 20,
        ..UserInformation::default()
    }));
    snapshot.apply_packet(&ServerPacket::ObjectItem(ObjectItem {
        object_id: NEAR_ITEM_ID,
        // 点阵兜底字体只认 ASCII；用英文名保证无字体环境也能断言名字被画出来。
        name: "Potion".to_owned(),
        location: SELF_CELL.moved(MirDirection::Right, 1),
        image: 0,
        ..ObjectItem::default()
    }));
    snapshot
}

/// 屏幕格中心（鼠标位置）。
fn cell_center(scene: &MapScene, cell: Point) -> Point {
    let (x, y) = scene.world_to_screen(SELF_CELL, cell);
    Point::new(x + CELL_WIDTH / 2, y + CELL_HEIGHT / 2)
}

#[test]
fn clicking_empty_ground_walks_a_multi_step_path_in_order() {
    let snapshot = snapshot();
    let text = UiText::fallback();
    let screen = Rect::screen(800, 600);
    let scene = scene();
    let goal = SELF_CELL.moved(MirDirection::Right, 4);
    let mut state = UiState::new(Language::ZhCn);
    state.set_time(1_000);
    state.mouse = cell_center(&scene, goal);
    // 全部可走（64×64 合成地图）。
    let walkable = |x: i32, y: i32| (0..64).contains(&x) && (0..64).contains(&y);

    let first = state
        .on_click_with_path(&snapshot, &text, screen, Some(goal), Some(&walkable))
        .expect("点空地应产生第一步");
    assert_eq!(
        first,
        UiAction::WalkStep {
            direction: MirDirection::Right,
            run: false
        }
    );
    assert_eq!(
        first.net_command(),
        Some(NetCommand::Walk(MirDirection::Right)),
        "路径第一步必须是 C.Walk"
    );

    // 模拟服务器逐格回包：每 600 ms 把角色挪到上一步的落点，再取下一步。
    let mut commands = vec![first.net_command().expect("第一步应有网络命令")];
    let mut position = SELF_CELL.moved(MirDirection::Right, 1);
    let mut snapshot = snapshot;
    snapshot.self_player.as_mut().expect("自己").position = position;
    let mut now = 1_600;
    loop {
        state.set_time(now);
        let Some(action) = state.tick_path(&snapshot) else {
            break;
        };
        let command = action.net_command().expect("路径每一步都有网络命令");
        commands.push(command);
        assert!(
            matches!(
                action,
                UiAction::WalkStep {
                    direction: MirDirection::Right,
                    run: false
                }
            ),
            "路径应一直向右：{action:?}"
        );
        position = position.moved(MirDirection::Right, 1);
        snapshot.self_player.as_mut().expect("自己").position = position;
        now += 600;
        assert!(commands.len() <= 8, "路径没有在 5 步内结束");
    }
    assert_eq!(
        commands,
        vec![NetCommand::Walk(MirDirection::Right); 4],
        "应逐格发 4 个 C.Walk，顺序正确且到达后停止"
    );
    assert!(state.walk_path.is_none(), "到达目标后路径应清空");
}

#[test]
fn path_ticks_do_not_require_server_position_updates() {
    // 无头/注入式测试常常不模拟 `UserLocation` 回包：路径仍应按时间逐格发出。
    let snapshot = snapshot();
    let text = UiText::fallback();
    let screen = Rect::screen(800, 600);
    let scene = scene();
    let goal = SELF_CELL.moved(MirDirection::Right, 3);
    let mut state = UiState::new(Language::ZhCn);
    state.set_time(1_000);
    state.mouse = cell_center(&scene, goal);
    let walkable = |x: i32, y: i32| (0..64).contains(&x) && (0..64).contains(&y);

    let first = state
        .on_click_with_path(&snapshot, &text, screen, Some(goal), Some(&walkable))
        .expect("第一步");
    assert_eq!(
        first.net_command(),
        Some(NetCommand::Walk(MirDirection::Right))
    );
    let mut now = 1_600;
    let mut count = 1;
    loop {
        state.set_time(now);
        let Some(action) = state.tick_path(&snapshot) else {
            break;
        };
        assert_eq!(
            action.net_command(),
            Some(NetCommand::Walk(MirDirection::Right))
        );
        count += 1;
        now += 600;
        assert!(count <= 3, "3 格路径不应超过 3 步");
    }
    assert_eq!(count, 3, "不更新快照坐标也应发完 3 步");
}

#[test]
fn path_running_sends_run_for_two_same_direction_steps() {
    let snapshot = snapshot();
    let text = UiText::fallback();
    let screen = Rect::screen(800, 600);
    let scene = scene();
    let goal = SELF_CELL.moved(MirDirection::Right, 4);
    let mut state = UiState::new(Language::ZhCn);
    state.set_time(1_000);
    state.mouse = cell_center(&scene, goal);
    let walkable = |x: i32, y: i32| (0..64).contains(&x) && (0..64).contains(&y);

    let first = state
        .on_click_with_path(&snapshot, &text, screen, Some(goal), Some(&walkable))
        .expect("第一步");
    assert_eq!(
        first.net_command(),
        Some(NetCommand::Walk(MirDirection::Right))
    );
    let mut snapshot = snapshot;
    snapshot.self_player.as_mut().expect("自己").position = SELF_CELL.moved(MirDirection::Right, 1);
    state.set_time(1_600);
    state.set_path_running(true);
    let second = state.tick_path(&snapshot).expect("第二步");
    assert_eq!(
        second.net_command(),
        Some(NetCommand::Run(MirDirection::Right)),
        "按住 Shift 时连续两格同向应发 C.Run"
    );
    assert!(state.walk_path.is_some(), "跑步消耗两格后还应剩一格");
}

#[test]
fn unreachable_ground_falls_back_to_one_step_towards_the_target() {
    let snapshot = snapshot();
    let text = UiText::fallback();
    let screen = Rect::screen(800, 600);
    let scene = scene();
    let goal = SELF_CELL.moved(MirDirection::Right, 4);
    let mut state = UiState::new(Language::ZhCn);
    state.set_time(1_000);
    state.mouse = cell_center(&scene, goal);
    // 目标格不可走（A* 会直接拒绝），客户端应退化成一格。
    let walkable = |_x: i32, _y: i32| false;

    let action = state
        .on_click_with_path(&snapshot, &text, screen, Some(goal), Some(&walkable))
        .expect("不可达也要有兜底动作");
    assert_eq!(
        action,
        UiAction::WorldMove {
            direction: MirDirection::Right,
            step: true
        }
    );
    assert!(state.walk_path.is_none(), "寻路失败不应留下半截路径");
}

#[test]
fn hovering_a_ground_item_shows_its_name_and_adjacent_click_picks_it_up() {
    let snapshot = snapshot();
    let text = UiText::fallback();
    let screen = Rect::screen(800, 600);
    let mut scene = scene();
    let item_cell = SELF_CELL.moved(MirDirection::Right, 1);
    let mut state = UiState::new(Language::ZhCn);
    state.set_time(1_000);
    state.mouse = cell_center(&scene, item_cell);

    let item = state
        .hover_item(&snapshot, &scene, &text, SELF_CELL, screen)
        .expect("鼠标在物品格上应命中");
    assert_eq!(item.name, "Potion");

    // 相邻物品：直接 C.PickUp。
    let action = state
        .on_click_with_path(&snapshot, &text, screen, Some(item_cell), None)
        .expect("相邻物品应拾取");
    assert_eq!(action, UiAction::PickUp);
    assert_eq!(action.net_command(), Some(NetCommand::PickUp));

    // 悬停物品名确实画进了帧里（与鼠标不在物品格上的帧不同）。
    let entities = snapshot.entity_views(0);
    let mut hovered = Frame::new(800, 600);
    scene.render_with_entities(&mut hovered, SELF_CELL, &entities);
    // 渲染会按帧尺寸更新 `MapScene` 的视口，鼠标坐标要在这之后算。
    state.mouse = cell_center(&scene, item_cell);
    ui::draw(
        &mut state,
        &mut hovered,
        &mut scene,
        &snapshot,
        &text,
        None,
        SELF_CELL,
    );
    let mut plain = Frame::new(800, 600);
    scene.render_with_entities(&mut plain, SELF_CELL, &entities);
    state.mouse = Point::new(2, 2);
    ui::draw(
        &mut state, &mut plain, &mut scene, &snapshot, &text, None, SELF_CELL,
    );
    let changed = hovered
        .pixels
        .chunks_exact(4)
        .zip(plain.pixels.chunks_exact(4))
        .filter(|(left, right)| left != right)
        .count();
    assert!(
        changed > 20,
        "悬停物品名应在帧里可见，实际差异 {changed} 像素"
    );

    let output = repo_root().join("target/client-pickup.png");
    std::fs::create_dir_all(repo_root().join("target")).ok();
    hovered.save_png(&output).expect("写出 client-pickup.png");
    let size = std::fs::metadata(&output).expect("PNG 应存在").len();
    assert!(size > 512, "PNG 不应为空文件，实际 {size} 字节");
}

#[test]
fn clicking_a_far_item_approaches_then_picks_up() {
    let mut snapshot = snapshot();
    snapshot.apply_packet(&ServerPacket::ObjectItem(ObjectItem {
        object_id: FAR_ITEM_ID,
        name: "祝福油".to_owned(),
        location: SELF_CELL.moved(MirDirection::Right, 2),
        image: 1,
        ..ObjectItem::default()
    }));
    let text = UiText::fallback();
    let screen = Rect::screen(800, 600);
    let scene = scene();
    let item_cell = SELF_CELL.moved(MirDirection::Right, 2);
    let mut state = UiState::new(Language::ZhCn);
    state.set_time(1_000);
    state.mouse = cell_center(&scene, item_cell);

    let first = state
        .on_click_with_path(&snapshot, &text, screen, Some(item_cell), None)
        .expect("远处物品应先靠近");
    assert_eq!(
        first,
        UiAction::WorldMove {
            direction: MirDirection::Right,
            step: true
        }
    );
    assert!(state.approach.is_some(), "应记录靠近进度");
    // 走到相邻格：下一步 tick 自动捡取。
    snapshot.self_player.as_mut().expect("自己").position = SELF_CELL.moved(MirDirection::Right, 1);
    state.set_time(1_600);
    assert_eq!(
        state.tick_approach(&snapshot),
        Some(UiAction::PickUp),
        "相邻后应发 C.PickUp"
    );
    assert!(state.approach.is_none());
}
