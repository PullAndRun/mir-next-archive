//! 大地图面板（`B`）：`mmap.wzl` 的 `MapInfo.BigMap` 帧 + 玩家点。
//!
//! 布局照 Crystal `BigMapDialog`（`Client/MirScenes/Dialogs/BigMapDialog.cs`）：
//!
//! | 元素 | 本项目 | Crystal 出处 |
//! |---|---|---|
//! | 面板位置 | 屏幕居中 | `:98` `Location = Center` |
//! | 标题（地图名） | 面板内 `(19, 6)`，高 20 | `:225-234` `TitleLabel` |
//! | 地图视区 | 面板内 `(14, 52)`、568×380 | `:559`、`:650-653` |
//! | 坐标行 | 面板底部（y = 435） | `:217-223` `CoordinateLabel` |
//! | 图像号 | `S.MapInformation.big_map` / `ClientMapInfo.BigMap` | `:642`、`Shared/Data/ClientData.cs:324` |
//! | 图库 | `resource/data/mmap.wzl`（与小地图同一个库） | `Client/MirGraphics/MLibrary.cs:23` |
//!
//! 图像本身的整图缩放与点位投影在 `mir-render::minimap`（[`mir_render::MinimapScene::draw_big_map`]），
//! 这里只负责面板外观、标题与坐标行。
//!
//! # 与 Crystal 的简化
//!
//! * Crystal 的对话框尺寸来自 `Title.wzl` 的 820 号图（`BigMapDialog.cs:95-98` 的
//!   `Index = 820; Library = Libraries.Title`），本仓库的素材里**没有** `Title.wzl`，
//!   所以面板尺寸按控件坐标反推：宽 = 视区左边距 14 + 568 + 右边距 14 = 596（`:652`），
//!   高 = 视区上边距 52 + 380 = 432，再加底部坐标行（`:221` 的 y = 435）→ 460。
//!   面板用与其它面板相同的半透明底 + 边框（[`crate::ui::draw_panel`]）。
//! * 右侧的 NPC 列表、滚动条、底部按钮（世界地图/我的位置/传送/搜索）与「已探明区域」
//!   遮罩都没有做（Crystal 也没有遮罩，直接画整张 `mmap.wzl` 大图）。
//! * 坐标行显示**自己的**坐标；Crystal 跟随鼠标显示大地图坐标（`:603-609`、`:257-260`）。
//! * 点大地图在 Crystal 里会寻路（`:612-628`）；本作已有 A*，但还没把大地图坐标映射到
//!   世界格，所以 `UiState::bigmap_clicked`
//!   会把面板上的点击吃掉，避免误触发「点地走路」。

use mir_core::{Language, Point};
use mir_render::{
    BIG_MAP_OFFSET_X, BIG_MAP_OFFSET_Y, BIG_MAP_VIEW_HEIGHT, BIG_MAP_VIEW_WIDTH, BigMapProjection,
    Frame, MinimapScene, MinimapView,
};

use crate::geom::Rect;
use crate::ui::hud;
use crate::ui::labels::Label;
use crate::ui::text::UiText;

/// 面板宽（简化值，推导见模块文档）。
pub const PANEL_WIDTH: u32 = 596;
/// 面板高（简化值，推导见模块文档）。
pub const PANEL_HEIGHT: u32 = 460;
/// 标题行高（`BigMapDialog.cs:230` 的 `Size = new Size(699, 20)`）。
pub const TITLE_HEIGHT: u32 = 20;
/// 底部坐标行高（`CoordinateLabel` 的 `AutoSize`，按 18 像素留位）。
pub const FOOTER_HEIGHT: u32 = 18;
/// 底部坐标行的纵向位置（`BigMapDialog.cs:221` 的 `Location = new Point(519, 435)`）。
pub const FOOTER_TOP: u32 = 435;
/// 没有底图素材时的占位色（与小地图面板相同）。
pub const PLACEHOLDER_COLOR: [u8; 4] = crate::ui::minimap::PLACEHOLDER_COLOR;

/// 面板矩形：屏幕居中（`BigMapDialog.cs:98` 的 `Location = Center`）。
#[must_use]
pub fn panel_rect(screen: Rect) -> Rect {
    Rect::new(
        (screen.width.saturating_sub(PANEL_WIDTH) / 2) as i32,
        (screen.height.saturating_sub(PANEL_HEIGHT) / 2) as i32,
        PANEL_WIDTH,
        PANEL_HEIGHT,
    )
}

/// 地图视区矩形（面板内 `(14, 52)`、568×380，`BigMapDialog.cs:559,650-653`）。
#[must_use]
pub fn view_rect(panel: Rect) -> Rect {
    Rect::new(
        panel.x + i32::try_from(BIG_MAP_OFFSET_X).unwrap_or(0),
        panel.y + i32::try_from(BIG_MAP_OFFSET_Y).unwrap_or(0),
        BIG_MAP_VIEW_WIDTH,
        BIG_MAP_VIEW_HEIGHT,
    )
}

/// 标题行矩形（面板内 `(19, 6)`，`BigMapDialog.cs:231`）。
#[must_use]
pub fn title_rect(panel: Rect) -> Rect {
    Rect::new(
        panel.x + 19,
        panel.y + 6,
        panel.width.saturating_sub(38),
        TITLE_HEIGHT,
    )
}

/// 底部坐标行矩形。
#[must_use]
pub fn footer_rect(panel: Rect) -> Rect {
    Rect::new(
        panel.x + 14,
        panel.y + i32::try_from(FOOTER_TOP).unwrap_or(0),
        panel.width.saturating_sub(28),
        FOOTER_HEIGHT,
    )
}

/// 画整个大地图面板。
///
/// `title` 是地图名（`MapSnapshot.title`），`view` 是 [`crate::ui::minimap::big_view_for`]
/// 的结果（已经带 `big_map` 图像号、地图宽高与自己/其他对象的点位）。
///
/// 返回值：是否真的画出了 `mmap.wzl` 的大地图图像（`false` = 画了纯色占位）。
pub fn draw(
    frame: &mut Frame,
    screen: Rect,
    scene: &mut MinimapScene,
    view: &MinimapView,
    title: &str,
    text: &UiText,
    language: Language,
) -> bool {
    let panel = panel_rect(screen);
    crate::ui::draw_panel(frame, panel);

    // 标题：地图名（Crystal `TitleLabel.Text = currentRecord.MapInfo.Title`，
    // `BigMapDialog.cs:87`）。
    let map_title = if title.is_empty() {
        Label::UnknownMap.text(language)
    } else {
        title
    };
    text.draw_center(
        frame,
        title_rect(panel),
        map_title,
        hud::SIZE_BODY,
        hud::TITLE_COLOR,
    );

    // 地图视区：整图缩放 + 点位（`mir-render::minimap`）。
    let view_area = view_rect(panel);
    let drew = scene.draw_big_map(frame, view_area.x, view_area.y, view);
    if !drew {
        // 没有素材（`mmap.wzl` 缺失或 `big_map == 0`）：纯色占位 + 地图名 + 提示。
        frame.fill_rect(
            view_area.x,
            view_area.y,
            view_area.width,
            view_area.height,
            PLACEHOLDER_COLOR,
        );
        text.draw_center(
            frame,
            Rect::new(
                view_area.x,
                view_area.center_y() - 26,
                view_area.width,
                TITLE_HEIGHT,
            ),
            map_title,
            hud::SIZE_BODY,
            hud::TITLE_COLOR,
        );
        text.draw_center(
            frame,
            Rect::new(
                view_area.x,
                view_area.center_y(),
                view_area.width,
                TITLE_HEIGHT,
            ),
            Label::NoBigmap.text(language),
            hud::SIZE_BODY,
            hud::MUTED_COLOR,
        );
    }

    // 底部：自己的坐标（Crystal `CoordinateLabel` 的 `[ x, y ]` 格式，`:259`）。
    let footer = footer_rect(panel);
    text.draw_clipped(
        frame,
        footer.x,
        footer.y,
        &format!("[ {}, {} ]", view.player.x, view.player.y),
        hud::SIZE_SMALL,
        hud::BODY_COLOR,
        footer,
    );
    text.draw_right(
        frame,
        footer.right(),
        footer.y,
        Label::BigmapHint.text(language),
        hud::SIZE_SMALL,
        hud::MUTED_COLOR,
    );
    drew
}

/// 大地图点击后选中的目的地。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapDestination {
    /// 目标地图名（`ClientMovementInfo.title`，通常就是目的地地图名）。
    pub map: String,
    /// 目标坐标（传送点坐标）。
    pub cell: Point,
}

/// 大地图点击命中传送点：返回精确目的地（地图名 + 坐标）。
///
/// 只认 `S.NewMapInfo.movements`（玩家可见数据）；`view_area` 是
/// [`view_rect`]，`projection` 是 [`MinimapScene::big_map_projection`] 的结果。
/// 点击空白处返回 `None`（不把任意格当目的地）。
#[must_use]
pub fn destination_at(
    view_area: Rect,
    projection: &BigMapProjection,
    movements: &[mir_protocol::data::ClientMovementInfo],
    point: Point,
) -> Option<MapDestination> {
    for movement in movements {
        let (dx, dy) = projection.cell_to_view(movement.location);
        let screen_x = view_area.x + dx;
        let screen_y = view_area.y + dy;
        if (screen_x - point.x).abs() <= 5 && (screen_y - point.y).abs() <= 5 {
            return Some(MapDestination {
                map: movement.title.clone(),
                cell: movement.location,
            });
        }
    }
    None
}

/// 大地图点击：设置 [`crate::ui::UiState::bigmap_destination`] 并返回是否命中。
pub fn click_destination(
    state: &mut crate::ui::UiState,
    view_area: Rect,
    projection: &BigMapProjection,
    movements: &[mir_protocol::data::ClientMovementInfo],
) -> bool {
    let point = state.mouse;
    match destination_at(view_area, projection, movements, point) {
        Some(destination) => {
            state.bigmap_destination = Some(destination);
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use mir_core::Point;
    use mir_core::enums::{MirClass, MirDirection, MirGender};
    use mir_render::{Image, ImageInfo, PLAYER_COLOR};

    use super::*;
    use crate::net::{MapSnapshot, SelfPlayer, WorldSnapshot};

    fn text() -> UiText {
        UiText::fallback()
    }

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

    fn synthetic_scene() -> MinimapScene {
        let mut images = BTreeMap::new();
        images.insert(302, solid(300, 200, [30, 40, 50, 255]));
        MinimapScene::from_images(images)
    }

    fn snapshot() -> WorldSnapshot {
        WorldSnapshot {
            map: Some(MapSnapshot {
                map_index: 0,
                file_name: "0".to_owned(),
                title: "比奇省".to_owned(),
                width: 600,
                height: 400,
                mini_map: 7,
                big_map: 302,
                music: 0,
                movements: Vec::new(),
            }),
            self_player: Some(SelfPlayer {
                object_id: 7,
                real_id: 7,
                name: "测试者".to_owned(),
                position: Point::new(300, 200),
                direction: MirDirection::Down,
                hp: 30,
                mp: 20,
                hp_max: 40,
                mp_max: 40,
                level: 5,
                class: MirClass::Warrior,
                gender: MirGender::Male,
                hair: 1,
                gold: 0,
                credit: 0,
                dead: false,
                action: mir_render::ActorAction::Standing,
                action_started_ms: 0,
                attack_variant: None,
                looks: crate::net::PlayerLooks::default(),
            }),
            ..WorldSnapshot::default()
        }
    }

    #[test]
    fn click_destination_maps_point_and_starts_path_to_door_cell() {
        use crate::ui::{UiState, UiText};
        use mir_protocol::data::ClientMovementInfo;

        let mut snapshot = snapshot();
        let door = Point::new(305, 206);
        snapshot
            .map
            .as_mut()
            .expect("map")
            .movements
            .push(ClientMovementInfo {
                destination: 1,
                title: "奥玛洞穴".to_owned(),
                location: door,
                icon: 0,
            });
        let view = crate::ui::minimap::big_view_for(&snapshot, (600, 400));
        let screen = Rect::screen(800, 600);
        let view_area = view_rect(panel_rect(screen));
        let mut scene = synthetic_scene();
        let projection = scene.big_map_projection(&view).expect("投影");
        let point = Point::new(
            view_area.x + projection.cell_to_view(door).0,
            view_area.y + projection.cell_to_view(door).1,
        );

        let mut state = UiState::new(Language::ZhCn);
        state.show_bigmap = true;
        state.minimap_scene = synthetic_scene();
        state.mouse = point;
        let movements = &snapshot.map.as_ref().expect("map").movements;
        assert!(
            click_destination(&mut state, view_area, &projection, movements),
            "点到传送点应命中"
        );
        assert_eq!(
            state.bigmap_destination.as_ref().map(|dest| dest.cell),
            Some(door)
        );

        // 同一套 UI 点击：A* 路径终点必须是门格（首步由 WalkStep 发出）。
        let text = UiText::load(None);
        let action = state.on_click_with_path(&snapshot, &text, screen, None, Some(&|_x, _y| true));
        assert!(matches!(action, Some(crate::ui::UiAction::WalkStep { .. })));
        assert_eq!(state.walk_path.as_ref().map(|path| path.goal), Some(door));
        let path = state.walk_path.as_ref().expect("路径");
        let last = path
            .steps
            .iter()
            .fold(path.expected, |point, direction| point.moved(*direction, 1));
        assert_eq!(last, door, "路径终点必须是门格");
        assert_eq!(path.from, Point::new(300, 200), "起点是自身位置");
    }

    #[test]
    fn layout_matches_the_big_map_dialog_offsets() {
        let screen = Rect::screen(800, 600);
        let panel = panel_rect(screen);
        assert_eq!(panel.center(), screen.center(), "面板必须居中");
        assert_eq!((panel.width, panel.height), (PANEL_WIDTH, PANEL_HEIGHT));
        let view = view_rect(panel);
        assert_eq!(
            (view.x - panel.x, view.y - panel.y),
            (14, 52),
            "视区相对面板是 (14, 52)（BigMapDialog.cs:652-653）"
        );
        assert_eq!((view.width, view.height), (568, 380));
        assert!(panel.contains(view.center()));
        assert!(panel.contains(title_rect(panel).center()));
        assert!(panel.contains(footer_rect(panel).center()));
    }

    #[test]
    fn drawing_uses_the_map_image_and_marks_the_player() {
        let snapshot = snapshot();
        let view = crate::ui::minimap::big_view_for(&snapshot, (600, 400));
        assert_eq!(view.image_index, 302, "图像号来自 MapSnapshot.big_map");
        let screen = Rect::screen(800, 600);
        let panel = panel_rect(screen);
        let view_area = view_rect(panel);
        let mut scene = synthetic_scene();
        let mut frame = Frame::new(800, 600);
        frame.clear([0, 0, 0, 255]);
        let drew = draw(
            &mut frame,
            screen,
            &mut scene,
            &view,
            "比奇省",
            &text(),
            Language::ZhCn,
        );
        assert!(drew, "有素材时应当画出大地图");
        // 图像 300×200 居中在 568×380 里：偏移 (134, 90)。
        assert_eq!(
            frame.pixel(
                u32::try_from(view_area.x + 134 + 5).unwrap(),
                u32::try_from(view_area.y + 90 + 5).unwrap()
            ),
            Some([30, 40, 50, 255]),
            "视区里应当画满大地图底图"
        );
        // 自己 (300, 200) → 视区内 (134 + 150, 90 + 100) 的 2×2 黄点。
        let dot = (view_area.x + 284, view_area.y + 190);
        assert_eq!(
            frame.pixel(u32::try_from(dot.0).unwrap(), u32::try_from(dot.1).unwrap()),
            Some(PLAYER_COLOR),
            "玩家点应画成黄色"
        );
    }

    #[test]
    fn missing_assets_draw_a_placeholder_with_the_map_name() {
        let snapshot = snapshot();
        let view = crate::ui::minimap::big_view_for(&snapshot, (600, 400));
        let screen = Rect::screen(800, 600);
        let panel = panel_rect(screen);
        let view_area = view_rect(panel);
        let mut scene = MinimapScene::empty();
        let mut frame = Frame::new(800, 600);
        frame.clear([0, 0, 0, 255]);
        // 用英文标题/文案：`UiText::fallback()` 的 3×5 点阵只有 ASCII，
        // 中文要靠 TrueType 字体；这里要断言的只是「占位区里有文字」。
        let drew = draw(
            &mut frame,
            screen,
            &mut scene,
            &view,
            "Beech",
            &text(),
            Language::En,
        );
        assert!(!drew, "没有素材时返回 false");
        assert_eq!(
            frame.pixel(
                u32::try_from(view_area.x + 5).unwrap(),
                u32::try_from(view_area.y + 5).unwrap()
            ),
            Some(PLACEHOLDER_COLOR),
            "缺素材时视区应当是纯色占位"
        );
        // 占位里应当有文字像素（地图名 + 提示）。
        let mut light = 0;
        for y in view_area.y..view_area.bottom() {
            for x in view_area.x..view_area.right() {
                if let Some([r, g, b, _]) =
                    frame.pixel(u32::try_from(x).unwrap_or(0), u32::try_from(y).unwrap_or(0))
                {
                    if r > 150 && g > 150 && b > 150 {
                        light += 1;
                    }
                }
            }
        }
        assert!(light > 0, "占位区域应当画出地图名/提示文字");
    }
}
