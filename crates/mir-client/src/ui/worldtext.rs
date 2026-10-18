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

use crate::net::{DamageEvent, WorldSnapshot};
use crate::ui::UiText;
use crate::ui::hud::SIZE_SMALL;
use crate::ui::labels::Label;

/// 伤害数字存活时长（毫秒）。
pub const DAMAGE_LIFETIME_MS: i64 = 1000;
/// 伤害数字上飘的像素数。
pub const DAMAGE_RISE_PIXELS: i32 = 50;
/// 头顶名字相对格原点的垂直偏移（格原点是脚底，名字在头顶上方）。
pub const NAME_OFFSET_Y: i32 = -44;
/// 伤害数字的起始垂直偏移。
pub const DAMAGE_OFFSET_Y: i32 = -56;
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
        if Some(*object_id) == self_id || entity.kind == EntityKind::Item {
            continue;
        }
        let color = match entity.kind {
            EntityKind::Player => [190, 220, 255, 255],
            EntityKind::Monster => [255, 176, 160, 255],
            EntityKind::Npc => [170, 255, 190, 255],
            EntityKind::Item => continue,
        };
        draw_one_name(
            frame,
            scene,
            center,
            text,
            entity.position,
            &entity.name,
            color,
        );
    }
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
                hp_percent: None,
                action: mir_render::animation::ActorAction::Standing,
                action_started_ms: 0,
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
