//! 世界内文字：实体头顶名字（带描边）与伤害飘字。
//!
//! * 名字：玩家/怪物/NPC 的 `WorldEntity.name`，画在精灵头顶正中，黑边 + 亮色；
//! * 伤害数字：`S.ObjectStruck` / `S.DamageIndicator` 产生的事件，向上飘约 50 px 并在
//!   1 秒内淡出（Crystal `Client/MirObjects/Damage.cs` 的 `Distance = 50`、`duration = 1000`）。
//!
//! 时间全部来自 [`crate::ui::UiState::time_millis`]（可注入），模块本身不做 sleep，
//! 因此无头测试可以在 500 ms / 2000 ms 两个时刻各画一帧断言存在/消失。

use mir_core::enums::DamageType;
use mir_core::{Language, Point};
use mir_render::{EntityKind, Frame, MapScene};

use crate::net::{DamageEvent, WorldEntity, WorldSnapshot};
use crate::ui::UiText;
use crate::ui::hud::SIZE_SMALL;
use crate::ui::labels::{self, Label};

/// 伤害数字存活时长（毫秒）。
pub const DAMAGE_LIFETIME_MS: i64 = 1000;
/// 伤害数字上飘的像素数。
pub const DAMAGE_RISE_PIXELS: i32 = 50;
/// 头顶名字相对格原点的垂直偏移（格原点是脚底，名字在头顶上方）。
pub const NAME_OFFSET_Y: i32 = -44;
/// 伤害数字的起始垂直偏移。
pub const DAMAGE_OFFSET_Y: i32 = -56;
/// 地面物品名字相对格原点的垂直偏移（物品图标比人物小，名字贴近地面）。
pub const ITEM_NAME_OFFSET_Y: i32 = -24;
/// 地面物品名字的颜色（Crystal 的物品标签默认亮黄色，`ItemObject.DrawName`）。
pub const ITEM_NAME_COLOR: [u8; 4] = [240, 220, 120, 255];
/// 伤害数字字号。
pub const DAMAGE_SIZE: u32 = SIZE_SMALL;

/// 一个正在飘的伤害数字。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DamageNumber {
    /// 受击对象（调试/去重用）。
    pub object_id: u32,
    /// 锚点格。
    pub position: Point,
    /// 文本（数字、`闪避` 或 `✕`）。
    pub text: String,
    /// 颜色。
    pub color: [u8; 4],
    /// 开始时间（Unix 毫秒）。
    pub started: i64,
    /// 结束时间（Unix 毫秒）。
    pub until: i64,
}

impl DamageNumber {
    /// 是否已过期。
    #[must_use]
    pub const fn expired(&self, now: i64) -> bool {
        now >= self.until
    }

    /// 不透明度（0..=255），随时间线性淡出。
    #[must_use]
    pub fn alpha(&self, now: i64) -> u8 {
        let total = self.until.saturating_sub(self.started).max(1);
        let left = self.until.saturating_sub(now).clamp(0, total);
        u8::try_from(left * 255 / total).unwrap_or(255)
    }

    /// 已经上飘的像素数（0..=[`DAMAGE_RISE_PIXELS`]）。
    #[must_use]
    pub fn rise(&self, now: i64) -> i32 {
        let total = self.until.saturating_sub(self.started).max(1);
        let elapsed = (now - self.started).clamp(0, total);
        i32::try_from(elapsed * i64::from(DAMAGE_RISE_PIXELS) / total).unwrap_or(0)
    }
}

/// 把一条伤害事件转成飘字；没有位置或事件无效时返回 `None`。
#[must_use]
pub fn damage_from_event(
    event: &DamageEvent,
    snapshot: &WorldSnapshot,
    language: Language,
    now: i64,
) -> Option<DamageNumber> {
    let position = event
        .location
        .or_else(|| snapshot.object_position(event.object_id))?;
    let kind = snapshot
        .entities
        .get(&event.object_id)
        .map(|entity| entity.kind);
    let is_player = kind == Some(EntityKind::Player)
        || snapshot
            .self_player
            .as_ref()
            .is_some_and(|me| me.object_id == event.object_id);
    let (text, color) = match (event.damage, event.damage_type) {
        (Some(_), Some(DamageType::Miss)) => (
            Label::Miss.text(language).to_owned(),
            if is_player {
                [255, 160, 150, 255]
            } else {
                [210, 210, 210, 255]
            },
        ),
        (Some(damage), Some(DamageType::Critical)) => (
            format!("{damage}!"),
            if is_player {
                [255, 90, 80, 255]
            } else {
                [255, 120, 90, 255]
            },
        ),
        (Some(damage), _) => (
            format!("{}", damage.max(0)),
            if is_player {
                [255, 90, 90, 255]
            } else {
                [255, 255, 255, 255]
            },
        ),
        // `S.ObjectStruck` 不带伤害值：画一个红色受击标记。
        (None, _) => ("×".to_owned(), [255, 96, 96, 255]),
    };
    Some(DamageNumber {
        object_id: event.object_id,
        position,
        text,
        color,
        started: now,
        until: now.saturating_add(DAMAGE_LIFETIME_MS),
    })
}

/// 带黑描边的文字：先画 8 个方向的偏移，再画本体。
#[allow(clippy::too_many_arguments)]
pub fn draw_outlined(
    text: &UiText,
    frame: &mut Frame,
    x: i32,
    y: i32,
    value: &str,
    size: u32,
    color: [u8; 4],
    outline: [u8; 4],
) {
    for (dx, dy) in [
        (-1, 0),
        (1, 0),
        (0, -1),
        (0, 1),
        (-1, -1),
        (1, -1),
        (-1, 1),
        (1, 1),
    ] {
        text.draw(frame, x + dx, y + dy, value, size, outline);
    }
    text.draw(frame, x, y, value, size, color);
}

/// 画所有实体头顶名字（自己也是实体之一）；名字来自服务端，不随界面语言变化。
///
/// 颜色规则（需求：队友蓝名，与 PK 的红/黄/棕/白区分）：
///
/// * 自己：金色；
/// * 队友：队伍蓝 [`crate::ui::group::GROUP_NAME_COLOR`]（Crystal 的组队蓝：
///   小地图队友点 `MainDialogs.cs:1947-1948`、组队血条 `MapObject.cs:485`）；
/// * 其他玩家：用服务端给的名字颜色（`S.ObjectPlayer.NameColour` / `S.ObjectColourChanged`，
///   服务端 `PlayerObject.GetNameColour` 的 PK 红/黄/棕/白，`Server/MirObjects/PlayerObject.cs:1849-1890`）；
///   没收到过或收到白色时退回默认的浅蓝；
/// * 怪物 / NPC：默认色（Crystal 也给它们下发名字颜色，本作只对玩家生效，简化）。
pub fn draw_names(
    frame: &mut Frame,
    snapshot: &WorldSnapshot,
    scene: &MapScene,
    center: Point,
    text: &UiText,
) {
    if let Some(me) = &snapshot.self_player {
        draw_one_name(
            frame,
            scene,
            center,
            text,
            me.position,
            &me.name,
            [255, 236, 160, 255],
        );
    }
    let self_id = snapshot.self_player.as_ref().map(|me| me.object_id);
    for (object_id, entity) in &snapshot.entities {
        if Some(*object_id) == self_id || entity.kind == EntityKind::Item || entity.hidden {
            // 隐身对象的名字也不画（Crystal `GameScene.ObjectHidden`）。
            continue;
        }
        draw_one_name(
            frame,
            scene,
            center,
            text,
            entity.position,
            &entity.name,
            name_color(snapshot, entity),
        );
    }
}

/// 其他实体的名字颜色（见 [`draw_names`] 的规则）。
#[must_use]
pub fn name_color(snapshot: &WorldSnapshot, entity: &WorldEntity) -> [u8; 4] {
    if snapshot.is_group_member(&entity.name) {
        return crate::ui::group::GROUP_NAME_COLOR;
    }
    if entity.kind == EntityKind::Player {
        if let Some(colour) = entity.name_colour {
            // 服务端默认发白色（`NAME_COLOUR_WHITE`）；白色没有区分度，仍用默认浅蓝。
            if colour.alpha() != 0
                && (colour.red(), colour.green(), colour.blue()) != (255, 255, 255)
            {
                return [colour.red(), colour.green(), colour.blue(), 255];
            }
        }
    }
    match entity.kind {
        EntityKind::Player => [190, 220, 255, 255],
        EntityKind::Monster => [255, 176, 160, 255],
        EntityKind::Npc => [170, 255, 190, 255],
        EntityKind::Item => [240, 220, 120, 255],
    }
}

/// 画鼠标下地面物品的名字（`GameScene.cs:10619-10626` 只画鼠标命中的那一件）。
///
/// 复用 [`draw_one_name`] 的描边/居中逻辑，只是垂直偏移更贴近地面、颜色固定为
/// [`ITEM_NAME_COLOR`]。`cell` 是物品所在格（屏幕坐标由 `scene.world_to_screen` 换算）。
pub fn draw_item_name(
    frame: &mut Frame,
    snapshot: &WorldSnapshot,
    scene: &MapScene,
    center: Point,
    text: &UiText,
    cell: Point,
    language: Language,
) {
    let Some(item) = snapshot
        .entities
        .values()
        .filter(|entity| entity.kind == EntityKind::Item && entity.position == cell)
        .min_by_key(|entity| entity.object_id)
    else {
        return;
    };
    // 金币堆的实体名是空的（金额在 `gold_piles`），按玩家语言渲染 `金币 500`。
    let name = snapshot.gold_piles.get(&item.object_id).map_or_else(
        || item.name.clone(),
        |gold| labels::gold_text(language, *gold),
    );
    if name.is_empty() {
        return;
    }
    let (cell_x, cell_y) = scene.world_to_screen(center, cell);
    let width = text.text_width(&name, SIZE_SMALL);
    let x = cell_x + 24 - i32::try_from(width).unwrap_or(0) / 2;
    let y = cell_y + ITEM_NAME_OFFSET_Y;
    if x + i32::try_from(width).unwrap_or(0) < 0
        || x >= i32::try_from(frame.width).unwrap_or(i32::MAX)
        || y + 12 < 0
        || y >= i32::try_from(frame.height).unwrap_or(i32::MAX)
    {
        return;
    }
    draw_outlined(
        text,
        frame,
        x,
        y,
        &name,
        SIZE_SMALL,
        ITEM_NAME_COLOR,
        [0, 0, 0, 220],
    );
}

/// 画一个名字：居中于格原点，带黑描边。
#[allow(clippy::too_many_arguments)]
fn draw_one_name(
    frame: &mut Frame,
    scene: &MapScene,
    center: Point,
    text: &UiText,
    position: Point,
    name: &str,
    color: [u8; 4],
) {
    if name.is_empty() {
        return;
    }
    let (cell_x, cell_y) = scene.world_to_screen(center, position);
    let width = text.text_width(name, SIZE_SMALL);
    let x = cell_x + 24 - i32::try_from(width).unwrap_or(0) / 2;
    let y = cell_y + NAME_OFFSET_Y;
    if x + i32::try_from(width).unwrap_or(0) < 0
        || x >= i32::try_from(frame.width).unwrap_or(i32::MAX)
        || y + 12 < 0
        || y >= i32::try_from(frame.height).unwrap_or(i32::MAX)
    {
        return;
    }
    draw_outlined(text, frame, x, y, name, SIZE_SMALL, color, [0, 0, 0, 220]);
}

/// 画所有未过期的伤害数字。
pub fn draw_damage_numbers(
    frame: &mut Frame,
    numbers: &[DamageNumber],
    scene: &MapScene,
    center: Point,
    text: &UiText,
    now: i64,
) {
    for number in numbers.iter().filter(|number| !number.expired(now)) {
        let alpha = number.alpha(now);
        if alpha == 0 {
            continue;
        }
        let (cell_x, cell_y) = scene.world_to_screen(center, number.position);
        let width = text.text_width(&number.text, DAMAGE_SIZE);
        let x = cell_x + 24 - i32::try_from(width).unwrap_or(0) / 2;
        let y = cell_y + DAMAGE_OFFSET_Y - number.rise(now);
        let [red, green, blue, _] = number.color;
        draw_outlined(
            text,
            frame,
            x,
            y,
            &number.text,
            DAMAGE_SIZE,
            [red, green, blue, alpha],
            [0, 0, 0, alpha.saturating_mul(2) / 3],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> WorldSnapshot {
        let mut snapshot = WorldSnapshot::default();
        snapshot.entities.insert(
            8,
            crate::net::WorldEntity {
                object_id: 8,
                kind: EntityKind::Monster,
                name: "稻草人".to_owned(),
                position: Point::new(10, 10),
                direction: mir_core::enums::MirDirection::Down,
                image: 0,
                looks: crate::net::PlayerLooks::default(),
                dead: false,
                hidden: false,
                hp_percent: None,
                level: 0,
                class: mir_core::enums::MirClass::Warrior,
                name_colour: None,
                action: mir_render::animation::ActorAction::Standing,
                action_started_ms: 0,
                attack_variant: None,
            },
        );
        snapshot
    }

    fn event(damage: Option<i32>, kind: Option<DamageType>) -> DamageEvent {
        DamageEvent {
            id: 1,
            object_id: 8,
            damage,
            damage_type: kind,
            location: None,
        }
    }

    #[test]
    fn damage_numbers_expire_after_one_second_and_rise_upward() {
        let snapshot = snapshot();
        let number = damage_from_event(
            &event(Some(17), Some(DamageType::Hit)),
            &snapshot,
            Language::ZhCn,
            1_000,
        )
        .expect("有位置");
        assert_eq!(number.text, "17");
        assert!(!number.expired(1_500), "500ms 时仍在");
        assert!(number.expired(3_000), "2s 后消失");
        assert!(number.rise(1_500) > 0, "应向上飘");
        assert!(number.rise(1_500) < DAMAGE_RISE_PIXELS);
        assert!(number.alpha(1_500) < 255);
        assert_eq!(number.alpha(2_100), 0);
    }

    #[test]
    fn miss_and_struck_get_text_and_colors() {
        let snapshot = snapshot();
        let miss = damage_from_event(
            &event(Some(0), Some(DamageType::Miss)),
            &snapshot,
            Language::ZhCn,
            0,
        )
        .unwrap();
        assert_eq!(miss.text, "闪避");
        let critical = damage_from_event(
            &event(Some(42), Some(DamageType::Critical)),
            &snapshot,
            Language::En,
            0,
        )
        .unwrap();
        assert_eq!(critical.text, "42!");
        let struck = damage_from_event(&event(None, None), &snapshot, Language::ZhCn, 0).unwrap();
        assert_eq!(struck.text, "×");
        assert_ne!(miss.color, critical.color);
    }

    #[test]
    fn events_without_a_known_object_are_skipped() {
        let snapshot = WorldSnapshot::default();
        assert!(damage_from_event(&event(Some(1), None), &snapshot, Language::ZhCn, 0).is_none());
    }
}
