//! 小地图面板（`M`）：右上角半透明底 + `mmap.wzl` 底图 + 玩家/怪物点。
//!
//! 布局照 Crystal 的 `MiniMapDialog`（`Client/MirScenes/Dialogs/MainDialogs.cs:1764-1880`）：
//!
//! | 元素 | 本项目 | Crystal 出处 |
//! |---|---|---|
//! | 面板贴右上角 | `x = 屏宽 - 126`、`y = 0` | `:1780` `Location = new Point(Settings.ScreenWidth - 126, 0)` |
//! | 标题（地图名） | 面板顶部 18 像素高 | `:1786-1793` `MapNameLabel.Size = (120, 18)` |
//! | 视窗 | 面板内偏移 `(3, 22)`、120×108 | `:1912-1914` |
//! | 底部坐标行 | 面板底部 18 像素高 | `:1795-1802` `LocationLabel` |
//!
//! 画面本体（裁剪、缩放、点位颜色）在 `mir-render::minimap`，这里只负责面板外观与文字。
//!
//! # 缺素材
//!
//! `mmap.wzl` 不存在（CI）或 `MapInfo.MiniMap == 0` 时，视窗画一块纯色占位并写上
//! 地图名/「无小地图素材」，**不 panic**（Crystal 的做法是把面板切成无图的小档，
//! `MainDialogs.cs:1898-1901,2046-2058`；本作简化成一个占位块）。

use mir_core::Language;
use mir_core::Point;
use mir_render::{
    EntityKind, Frame, MarkerKind, MinimapMarker, MinimapScene, MinimapView, VIEW_HEIGHT,
    VIEW_WIDTH,
};

use crate::geom::Rect;
use crate::net::{WorldEntity, WorldSnapshot};
use crate::ui::hud;
use crate::ui::labels::Label;
use crate::ui::text::UiText;

/// 面板贴右边缘时的宽度（`MainDialogs.cs:1780` 的 126）。
pub const PANEL_WIDTH: u32 = 126;
/// 标题行高（`MainDialogs.cs:1790` 的 `(120, 18)`）。
pub const TITLE_HEIGHT: u32 = 18;
/// 视窗相对面板左上角的 X 偏移（`MainDialogs.cs:1913-1914` 的 `Offset(3, 22)`）。
pub const VIEW_OFFSET_X: u32 = 3;
/// 视窗相对面板左上角的 Y 偏移。
pub const VIEW_OFFSET_Y: u32 = 22;
/// 底部坐标行高（`MainDialogs.cs:1799` 的 `(56, 18)`）。
pub const FOOTER_HEIGHT: u32 = 18;
/// 面板总高：22 + 108 + 18。
pub const PANEL_HEIGHT: u32 = VIEW_OFFSET_Y + VIEW_HEIGHT + FOOTER_HEIGHT;
/// 没有底图时的占位色（暗蓝灰，与面板底色区分）。
pub const PLACEHOLDER_COLOR: [u8; 4] = [26, 34, 46, 255];

/// 小地图面板矩形：贴屏幕右上角（`MainDialogs.cs:1780`）。
#[must_use]
pub fn panel_rect(screen: Rect) -> Rect {
    Rect::new(
        screen.right() - PANEL_WIDTH as i32,
        screen.y,
        PANEL_WIDTH,
        PANEL_HEIGHT,
    )
}

/// 视窗矩形（面板内 `(3, 22)` 处的 120×108，`MainDialogs.cs:1912-1914`）。
#[must_use]
pub fn view_rect(panel: Rect) -> Rect {
    Rect::new(
        panel.x + VIEW_OFFSET_X as i32,
        panel.y + VIEW_OFFSET_Y as i32,
        VIEW_WIDTH,
        VIEW_HEIGHT,
    )
}

/// 标题行矩形。
#[must_use]
pub fn title_rect(panel: Rect) -> Rect {
    Rect::new(
        panel.x + VIEW_OFFSET_X as i32,
        panel.y + 2,
        VIEW_WIDTH,
        TITLE_HEIGHT,
    )
}

/// 底部坐标行矩形。
#[must_use]
pub fn footer_rect(panel: Rect) -> Rect {
    Rect::new(
        panel.x + VIEW_OFFSET_X as i32,
        panel.y + VIEW_OFFSET_Y as i32 + VIEW_HEIGHT as i32,
        VIEW_WIDTH,
        FOOTER_HEIGHT,
    )
}

/// 从快照里拼出「这一帧要画什么」。
///
/// * 图像号取 `S.MapInformation.MiniMap`（[`MapSnapshot::mini_map`]）；
/// * 地图宽高优先用 `S.NewMapInfo`（`ClientMapInfo.Width/Height`），没有时退回本地地形尺寸
///   （Crystal 用的是 `MapControl.Width/Height`，`MainDialogs.cs:1917-1918`）；
/// * 点位规则见 `mir-render::minimap` 模块文档（跳过物品/尸体，`MainDialogs.cs:1941`）。
///
/// [`MapSnapshot::mini_map`]: crate::net::MapSnapshot::mini_map
#[must_use]
pub fn view_for(snapshot: &WorldSnapshot, terrain: (i32, i32)) -> MinimapView {
    let map = snapshot.map.as_ref();
    let image_index = map.map_or(0, |map| map.mini_map);
    view_with_image(snapshot, terrain, image_index)
}

/// 大地图用的视图：与 [`view_for`] 相同，但图像号换成 `MapInfo.BigMap`
/// （`Client/MirScenes/Dialogs/BigMapDialog.cs:642` 的 `currentRecord.MapInfo.BigMap`）。
///
/// 两个面板共用同一份 `mmap.wzl`（`Client/MirGraphics/MLibrary.cs:23`），
/// 只是取的帧号不同（Crystal 叫 `MiniMap` 与 `BigMap`）。
#[must_use]
pub fn big_view_for(snapshot: &WorldSnapshot, terrain: (i32, i32)) -> MinimapView {
    let image_index = snapshot.map.as_ref().map_or(0, |map| map.big_map);
    view_with_image(snapshot, terrain, image_index)
}

/// 按给定图像号拼视图（小地图/大地图共用）。
fn view_with_image(snapshot: &WorldSnapshot, terrain: (i32, i32), image_index: u16) -> MinimapView {
    let map = snapshot.map.as_ref();
    let (width, height) = map.map_or((0, 0), |map| (map.width, map.height));
    let map_width = u32::try_from(if width > 0 { width } else { terrain.0 }).unwrap_or(0);
    let map_height = u32::try_from(if height > 0 { height } else { terrain.1 }).unwrap_or(0);
    let player = snapshot
        .self_player
        .as_ref()
        .map_or_else(|| Point::new(0, 0), |me| me.position);
    let self_id = snapshot.self_player.as_ref().map(|me| me.object_id);
    let markers = snapshot
        .entities
        .iter()
        .filter(|(object_id, entity)| {
            // 自己已经在 `player` 里画了（`entity_views` 会跳过重复的自己）。
            Some(**object_id) != self_id && !entity.dead && marker_kind(entity).is_some()
        })
        .filter_map(|(_, entity)| {
            marker_kind(entity).map(|kind| MinimapMarker {
                cell: entity.position,
                kind,
            })
        })
        .collect();
    MinimapView {
        image_index,
        map_width,
        map_height,
        player,
        markers,
    }
}

/// 对象 → 点位类别；`None` 表示不画（物品/尸体，`MainDialogs.cs:1941`）。
fn marker_kind(entity: &WorldEntity) -> Option<MarkerKind> {
    match entity.kind {
        // 地面物品与金币不画（`ob.Race == ObjectType.Item`，`MainDialogs.cs:1941`）。
        EntityKind::Item => None,
        EntityKind::Npc => Some(MarkerKind::Npc),
        EntityKind::Player => Some(MarkerKind::Player),
        EntityKind::Monster => Some(MarkerKind::Monster),
    }
}

/// 画整个小地图面板。
///
/// `title` 是地图名（`MapSnapshot.title`），坐标行用 `view.player`。
/// 返回 `false` 表示没有底图素材（画了占位），`true` 表示画出了 `mmap.wzl` 的图像。
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
    // 半透明底 + 边框（需求：右上角、半透明底）。
    crate::ui::draw_panel(frame, panel);

    // 标题：地图名（Crystal `MapNameLabel.Text = map.Title`，`MainDialogs.cs:2079`）。
    let title_area = title_rect(panel);
    text.draw_center(
        frame,
        title_area,
        if title.is_empty() {
            Label::UnknownMap.text(language)
        } else {
            title
        },
        hud::SIZE_SMALL,
        hud::TITLE_COLOR,
    );

    // 视窗：底图 + 点位。
    //
    // `image_index == 0` 表示这张图**没有**小地图（Crystal `MainDialogs.cs:1898-1907`
    // 也是这么判断的），此时不查图库——`mmap.wzl` 的 0 号帧是另一张真实地图，
    // 不是「空图」，查了会画错。
    let view_area = view_rect(panel);
    let drew = if view.image_index == 0 {
        false
    } else {
        scene.draw(frame, view_area.x, view_area.y, view)
    };
    if !drew {
        // 没有底图：纯色占位 + 「无小地图素材」文字（不 panic）。
        frame.fill_rect(
            view_area.x,
            view_area.y,
            view_area.width,
            view_area.height,
            PLACEHOLDER_COLOR,
        );
        text.draw_center(
            frame,
            view_area,
            Label::NoMinimap.text(language),
            hud::SIZE_SMALL,
            hud::MUTED_COLOR,
        );
    }

    // 底部：坐标（Crystal `LocationLabel`），右侧是 M 关闭提示。
    let footer = footer_rect(panel);
    let coordinates = format!("({}, {})", view.player.x, view.player.y);
    text.draw_clipped(
        frame,
        footer.x,
        footer.y + 2,
        &coordinates,
        hud::SIZE_SMALL,
        hud::BODY_COLOR,
        Rect::new(footer.x, footer.y, footer.width, footer.height),
    );
    text.draw_right(
        frame,
        footer.right(),
        footer.y + 2,
        Label::MinimapHint.text(language),
        hud::SIZE_SMALL,
        hud::MUTED_COLOR,
    );
    drew
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use mir_core::enums::{MirClass, MirDirection, MirGender};
    use mir_render::{EntityKind, Image, ImageInfo, PLAYER_COLOR};

    use super::*;
    use crate::net::{MapSnapshot, SelfPlayer, WorldEntity, WorldSnapshot};

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
        images.insert(9, solid(300, 199, [30, 40, 50, 255]));
        MinimapScene::from_images(images)
    }

    fn snapshot() -> WorldSnapshot {
        let mut snapshot = WorldSnapshot {
            map: Some(MapSnapshot {
                map_index: 3,
                file_name: "0".to_owned(),
                title: "比奇省".to_owned(),
                width: 700,
                height: 700,
                mini_map: 9,
                big_map: 302,
                music: 0,
            }),
            self_player: Some(SelfPlayer {
                object_id: 7,
                name: "测试者".to_owned(),
                position: Point::new(330, 270),
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
                dead: false,
                action: mir_render::ActorAction::Standing,
                action_started_ms: 0,
                looks: crate::net::PlayerLooks::default(),
            }),
            ..WorldSnapshot::default()
        };
        snapshot.entities.insert(
            42,
            WorldEntity::new(
                42,
                EntityKind::Monster,
                "鸡".to_owned(),
                Point::new(334, 270),
                MirDirection::Down,
                1,
            ),
        );
        snapshot.entities.insert(
            43,
            WorldEntity::new(
                43,
                EntityKind::Item,
                "金币".to_owned(),
                Point::new(331, 271),
                MirDirection::Down,
                113,
            ),
        );
        snapshot
    }

    #[test]
    fn layout_matches_crystal_geometry() {
        let screen = Rect::screen(800, 600);
        let panel = panel_rect(screen);
        assert_eq!(panel.x, 800 - 126);
        assert_eq!(panel.y, 0);
        assert_eq!((panel.width, panel.height), (126, 148));
        let view = view_rect(panel);
        assert_eq!((view.x, view.y), (panel.x + 3, 22));
        assert_eq!((view.width, view.height), (VIEW_WIDTH, VIEW_HEIGHT));
        assert_eq!(footer_rect(panel).y, 22 + 108);
        assert_eq!(title_rect(panel).y, 2);
    }

    #[test]
    fn view_uses_map_information_and_skips_items() {
        let snapshot = snapshot();
        let view = view_for(&snapshot, (700, 700));
        assert_eq!(view.image_index, 9, "图像号来自 MapInformation.MiniMap");
        assert_eq!((view.map_width, view.map_height), (700, 700));
        assert_eq!(view.player, Point::new(330, 270));
        // 怪物在，地面物品不在（`MainDialogs.cs:1941`）。
        assert_eq!(view.markers.len(), 1);
        assert_eq!(view.markers[0].cell, Point::new(334, 270));
        assert_eq!(view.markers[0].kind, MarkerKind::Monster);
    }

    #[test]
    fn view_falls_back_to_terrain_size() {
        let mut snapshot = snapshot();
        if let Some(map) = &mut snapshot.map {
            map.width = 0;
            map.height = 0;
        }
        let view = view_for(&snapshot, (1024, 768));
        assert_eq!((view.map_width, view.map_height), (1024, 768));
    }

    #[test]
    fn draws_map_image_and_player_dot() {
        let screen = Rect::screen(800, 600);
        let panel = panel_rect(screen);
        let view = view_for(&snapshot(), (700, 700));
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
        assert!(drew, "有素材时应当画出底图");
        // 视窗里有底图颜色（不是黑）。
        let view_area = view_rect(panel);
        let mut non_black = 0;
        for y in view_area.y..view_area.bottom() {
            for x in view_area.x..view_area.right() {
                if frame.pixel(u32::try_from(x).unwrap(), u32::try_from(y).unwrap())
                    != Some([0, 0, 0, 255])
                {
                    non_black += 1;
                }
            }
        }
        assert!(non_black > 10_000, "视窗里应当画满底图，实际 {non_black}");
        // 玩家点（黄）必须出现在视窗内。
        let yellow = (view_area.y..view_area.bottom())
            .flat_map(|y| (view_area.x..view_area.right()).map(move |x| (x, y)))
            .filter(|(x, y)| {
                frame.pixel(u32::try_from(*x).unwrap(), u32::try_from(*y).unwrap())
                    == Some(PLAYER_COLOR)
            })
            .count();
        assert!(yellow >= 4, "玩家点应当有 4 个像素，实际 {yellow}");
    }

    #[test]
    fn missing_assets_draw_placeholder_instead_of_panicking() {
        let screen = Rect::screen(800, 600);
        let panel = panel_rect(screen);
        let view = view_for(&snapshot(), (700, 700));
        let mut scene = MinimapScene::empty();
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
        assert!(!drew, "没有素材时返回 false");
        // 视窗被纯色占位涂满。
        let view_area = view_rect(panel);
        assert_eq!(
            frame.pixel(
                u32::try_from(view_area.x + 5).unwrap(),
                u32::try_from(view_area.y + 5).unwrap()
            ),
            Some(PLACEHOLDER_COLOR)
        );
    }
}
