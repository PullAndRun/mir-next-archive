//! 世界实体命中测试与目标准星（鼠标悬停 / 锁定目标）。
//!
//! Crystal 的鼠标目标逻辑在 `MapControl` 里：
//!
//! * 每帧扫描鼠标格周围 5×5 的格子，取第一个命中精灵的对象作为 `MouseObject`
//!   （`Client/MirScenes/GameScene.cs:10492-10535`）；
//! * 名字只为非物品对象绘制（`GameScene.cs:10605-10606`），并且只画鼠标下的那一个
//!   （本作的名字是常显的，见 [`crate::ui::worldtext`]）；
//! * 高亮用半透明重绘：鼠标对象与锁定目标各重绘一遍 `DrawBlend()`
//!   （`GameScene.cs:10973-10980`，`Settings.HighlightTarget`）；
//! * 左键按下时把 `MouseObject` 记成 `TargetObjectID`，但死物、地面物品、NPC、
//!   `AI == 64`/`70` 的怪物除外，其余情况清空（`GameScene.cs:11442-11451`）。
//!
//! 本作的简化（原因见括号）：
//!
//! * 命中测试按**格子**做（`MapScene::screen_to_cell` 已经把屏幕坐标换算成格，
//!   逐像素判断精灵矩形要重放每一层的图像偏移，收益很小）；
//! * 高亮不用半透明重绘，而是围绕该格画一个 1px 方框（需求）；
//! * 同格多个对象按「怪物 > NPC > 玩家 > 地面物品」取一个（Crystal 取格子对象表里
//!   最后画的那个，顺序由绘制排序决定）。

use mir_core::Point;
use mir_render::{CELL_HEIGHT, CELL_WIDTH, EntityKind, Frame, MapScene};

use crate::geom::Rect;
use crate::net::{WorldEntity, WorldSnapshot};

/// 鼠标悬停高亮色（1px 方框）。
pub const HOVER_COLOR: [u8; 4] = [255, 240, 120, 255];
/// 锁定目标的高亮色（1px 方框）。
pub const TARGET_COLOR: [u8; 4] = [255, 64, 64, 255];

/// 鼠标下的实体（悬停名字/高亮用）：跳过地面物品与尸体（`GameScene.cs:10507-10517`、
/// `:10605-10606`），也跳过自己（`:10507` 的 `ob == MapObject.User`）。
#[must_use]
pub fn entity_at(snapshot: &WorldSnapshot, cell: Point) -> Option<&WorldEntity> {
    let self_id = snapshot.self_player.as_ref().map(|me| me.object_id);
    snapshot
        .entities
        .values()
        .filter(|entity| {
            entity.position == cell
                && entity.kind != EntityKind::Item
                && !entity.dead
                && !entity.hidden
                && Some(entity.object_id) != self_id
        })
        .min_by_key(|entity| (priority(entity.kind), entity.object_id))
}

/// 鼠标下的地面物品（拾取/物品名用）。
///
/// Crystal 的 `ItemObject.MouseOver` 在鼠标格与物品格相同时命中，并在
/// `GameScene.cs:10619-10626` 只给鼠标下的物品画名字（其余物品只在 `DropView` 时画）。
/// 本作按格命中，同格多件物品取对象 ID 最小的那件。
#[must_use]
pub fn item_at(snapshot: &WorldSnapshot, cell: Point) -> Option<&WorldEntity> {
    snapshot
        .entities
        .values()
        .filter(|entity| entity.kind == EntityKind::Item && entity.position == cell)
        .min_by_key(|entity| entity.object_id)
}

/// 可以锁定的目标：活着的怪物或其他玩家（`GameScene.cs:11442-11451` 排除了 NPC/物品/
/// 尸体；本作还没有宠物 AI，所以没有 `AI == 64/70` 的判断）。
#[must_use]
pub fn target_at(snapshot: &WorldSnapshot, cell: Point) -> Option<u32> {
    let self_id = snapshot.self_player.as_ref().map(|me| me.object_id);
    snapshot
        .entities
        .values()
        .filter(|entity| {
            entity.position == cell
                && !entity.dead
                && Some(entity.object_id) != self_id
                && matches!(entity.kind, EntityKind::Monster | EntityKind::Player)
        })
        .map(|entity| entity.object_id)
        .next()
}

/// 同格实体的优先级（越小越先被选中）。
const fn priority(kind: EntityKind) -> u8 {
    match kind {
        EntityKind::Monster => 0,
        EntityKind::Npc => 1,
        EntityKind::Player => 2,
        EntityKind::Item => 3,
    }
}

/// 一个地图格在屏幕上的矩形（48×32，`GameScene.cs:10292-10293`）。
///
/// `MapScene::world_to_screen` 返回的是格的左上角（`screen_to_cell` 的逆运算），
/// 所以 `[x, x+48) × [y, y+32)` 正好覆盖这一格。
#[must_use]
pub fn cell_rect(scene: &MapScene, center: Point, cell: Point) -> Rect {
    let (x, y) = scene.world_to_screen(center, cell);
    Rect::new(
        x,
        y,
        u32::try_from(CELL_WIDTH).unwrap_or(48),
        u32::try_from(CELL_HEIGHT).unwrap_or(32),
    )
}

/// 围绕一个格子画 1px 方框（需求指定的「目标准星」；Crystal 是半透明重绘，
/// `GameScene.cs:10973-10980`）。
pub fn draw_cell_box(
    frame: &mut Frame,
    scene: &MapScene,
    center: Point,
    cell: Point,
    color: [u8; 4],
) {
    let rect = cell_rect(scene, center, cell);
    if rect.right() < 0
        || rect.bottom() < 0
        || rect.x >= i32::try_from(frame.width).unwrap_or(i32::MAX)
        || rect.y >= i32::try_from(frame.height).unwrap_or(i32::MAX)
    {
        return;
    }
    frame.fill_rect(rect.x, rect.y, rect.width, 1, color);
    frame.fill_rect(rect.x, rect.bottom() - 1, rect.width, 1, color);
    frame.fill_rect(rect.x, rect.y, 1, rect.height, color);
    frame.fill_rect(rect.right() - 1, rect.y, 1, rect.height, color);
}

/// 画悬停与锁定目标的高亮（锁定目标的框后画，保证压在悬停框上面）。
pub fn draw_highlights(
    frame: &mut Frame,
    scene: &MapScene,
    center: Point,
    snapshot: &WorldSnapshot,
    hover_cell: Option<Point>,
    target_object_id: Option<u32>,
) {
    if let Some(cell) = hover_cell {
        draw_cell_box(frame, scene, center, cell, HOVER_COLOR);
    }
    if let Some(entity) = target_object_id.and_then(|id| snapshot.entities.get(&id)) {
        if !entity.dead {
            draw_cell_box(frame, scene, center, entity.position, TARGET_COLOR);
        }
    }
}

#[cfg(test)]
mod tests {
    use mir_core::enums::MirDirection;
    use mir_render::{EntityKind, TileLibraries, Viewport};

    use super::*;

    fn monster(id: u32, cell: Point) -> WorldEntity {
        WorldEntity::new(
            id,
            EntityKind::Monster,
            "鸡".to_owned(),
            cell,
            MirDirection::Down,
            1,
        )
    }

    fn snapshot() -> WorldSnapshot {
        let mut snapshot = WorldSnapshot {
            self_player: Some(crate::net::SelfPlayer {
                object_id: 1,
                real_id: 1,
                name: "测试者".to_owned(),
                position: Point::new(10, 10),
                direction: MirDirection::Down,
                hp: 10,
                mp: 10,
                hp_max: 10,
                mp_max: 10,
                level: 1,
                class: mir_core::enums::MirClass::Warrior,
                gender: mir_core::enums::MirGender::Male,
                hair: 0,
                gold: 0,
                credit: 0,
                dead: false,
                action: mir_render::ActorAction::Standing,
                action_started_ms: 0,
                attack_variant: None,
                native_action: None,
                looks: crate::net::PlayerLooks::default(),
            }),
            ..WorldSnapshot::default()
        };
        snapshot.entities.insert(7, monster(7, Point::new(11, 10)));
        snapshot.entities.insert(
            8,
            WorldEntity::new(
                8,
                EntityKind::Item,
                "金币 10".to_owned(),
                Point::new(12, 10),
                MirDirection::Down,
                113,
            ),
        );
        snapshot
    }

    fn scene() -> MapScene {
        MapScene::from_parts(
            mir_world::Map {
                width: 64,
                height: 64,
                cells: vec![mir_world::MapCell::default(); 64 * 64],
                file_name: "synthetic-target".to_owned(),
                version: mir_world::MapVersion::V0,
                doors: Vec::new(),
            },
            TileLibraries::empty("."),
            Viewport::DEFAULT,
        )
    }

    #[test]
    fn hit_testing_prefers_monsters_and_skips_items_and_self() {
        let snapshot = snapshot();
        assert_eq!(
            entity_at(&snapshot, Point::new(11, 10)).map(|entity| entity.object_id),
            Some(7)
        );
        // 地面物品不进入悬停（`GameScene.cs:10605-10606` 不为物品画名字）。
        assert!(entity_at(&snapshot, Point::new(12, 10)).is_none());
        // 自己不算悬停对象（`GameScene.cs:10507`）。
        assert!(entity_at(&snapshot, Point::new(10, 10)).is_none());
        assert!(entity_at(&snapshot, Point::new(30, 30)).is_none());
    }

    #[test]
    fn ground_items_are_hoverable_but_not_targetable() {
        let snapshot = snapshot();
        let item = item_at(&snapshot, Point::new(12, 10)).expect("金币应能被悬停");
        assert_eq!(item.name, "金币 10");
        assert_eq!(item.kind, EntityKind::Item);
        assert!(item_at(&snapshot, Point::new(11, 10)).is_none());
        assert_eq!(target_at(&snapshot, Point::new(12, 10)), None);
    }

    #[test]
    fn only_living_monsters_and_players_are_targetable() {
        let snapshot = snapshot();
        assert_eq!(target_at(&snapshot, Point::new(11, 10)), Some(7));
        assert_eq!(target_at(&snapshot, Point::new(12, 10)), None);
        // 死掉的怪物不能锁定（`GameScene.cs:11442` 的 `!MouseObject.Dead`）。
        let mut dead = snapshot.clone();
        dead.entities.get_mut(&7).unwrap().dead = true;
        assert_eq!(target_at(&dead, Point::new(11, 10)), None);
    }

    #[test]
    fn cell_boxes_are_drawn_around_the_cell_and_clipped_at_frame_edges() {
        let scene = scene();
        let center = Point::new(10, 10);
        let rect = cell_rect(&scene, center, Point::new(9, 9));
        assert_eq!((rect.width, rect.height), (48, 32));
        let mut frame = Frame::new(400, 300);
        frame.clear([0, 0, 0, 255]);
        draw_cell_box(&mut frame, &scene, center, Point::new(9, 9), HOVER_COLOR);
        let (x, y) = scene.world_to_screen(center, Point::new(9, 9));
        assert_eq!(
            frame.pixel(u32::try_from(x).unwrap(), u32::try_from(y).unwrap()),
            Some(HOVER_COLOR)
        );
        // 框外（格子右下角之外）没有画东西。
        assert_eq!(
            frame.pixel(
                u32::try_from(x + 48).unwrap(),
                u32::try_from(y + 16).unwrap()
            ),
            Some([0, 0, 0, 255])
        );
        // 帧外不 panic。
        draw_cell_box(
            &mut frame,
            &scene,
            center,
            Point::new(999, 999),
            HOVER_COLOR,
        );
    }
}
