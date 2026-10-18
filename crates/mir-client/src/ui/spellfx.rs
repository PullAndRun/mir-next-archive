//! 技能表现层：把 `S.Magic` / `S.ObjectMagic` / `S.ObjectSpell` 画到地图格上。
//!
//! Crystal 的技能表现由 `SpellObject`（`Client/MirObjects/SpellObject.cs`）承担：
//! 火墙沿施法方向铺开若干格、雷电从施法者连到目标、瞬息移动在落点留一圈光、
//! 隐身/召唤在施法者脚下起环。本作没有逐帧贴图素材，所以按「格 + 颜色 + 名字」
//! 画简化表现（与 [`crate::ui::buffs::draw_effects`] 的圆环同一思路）：
//!
//! * **区域类**（火墙/毒云）：火墙由服务端 5 条逐格 `S.ObjectSpell` 定位（中心 + 上下
//!   左右，`Map.cs:1145-1181` 的十字），客户端一格一条、不再铺开；毒云等没有逐格
//!   对象包的法术按施法方向铺 3 格近似；
//! * **指向类**（雷电/火球）：施法者 → 落点画一条带抖动的线（没有施法者就画落点）；
//! * **自身类**（隐身/魔法盾/召唤）：落点画一圈光；
//! * 所有表现都会在 `duration` 内淡出，并在格子上方浮出技能名（走 `ui::labels`）。
//!
//! 数据来源见 `crate::net::SpellEvent`；服务端的伤害/受击仍走 `S.DamageIndicator`
//! 与 `S.ObjectStruck`，这里只负责「看得见」。

use mir_core::enums::{MirDirection, Spell};
use mir_core::{Language, Point};
use mir_render::{Frame, MapScene};

use crate::geom::Rect;
use crate::net::{SpellEvent, WorldSnapshot};
use crate::ui::labels;
use crate::ui::text::UiText;

/// 瞬发技能表现的时长（毫秒）：雷电/火球/瞬息移动。
pub const CAST_MS: i64 = 700;
/// 区域持续技能的表现时长（毫秒）：火墙/毒云（伤害节奏由服务端决定）。
pub const AREA_MS: i64 = 4_000;
/// 自身增益/召唤的表现时长（毫秒）。
pub const AURA_MS: i64 = 1_200;
/// 格子色块宽度。
pub const CELL_WIDTH: u32 = 34;
/// 格子色块高度。
pub const CELL_HEIGHT: u32 = 20;

/// 该技能表现持续多久（毫秒）。UI 用它判断事件是否还该画。
#[must_use]
pub const fn duration_ms(spell: Spell) -> i64 {
    match spell {
        Spell::FireWall
        | Spell::PoisonCloud
        | Spell::Plague
        | Spell::ThunderStorm
        | Spell::Blizzard
        | Spell::IceStorm
        | Spell::MeteorShower
        | Spell::MeteorStrike => AREA_MS,
        Spell::Hiding
        | Spell::MassHiding
        | Spell::MagicShield
        | Spell::SummonSkeleton
        | Spell::SummonShinsu
        | Spell::SummonHolyDeva
        | Spell::SummonVampire
        | Spell::SummonToad
        | Spell::SummonSnakes
        | Spell::Mirroring => AURA_MS,
        _ => CAST_MS,
    }
}

/// 技能表现的颜色（区域/指向/自身三类用不同色）。
#[must_use]
pub const fn color(spell: Spell) -> [u8; 4] {
    match spell {
        Spell::FireWall | Spell::HellFire | Spell::FireBurst | Spell::FireBang => {
            [255, 132, 48, 255]
        }
        Spell::ThunderBolt | Spell::ThunderStorm | Spell::Lightning => [255, 240, 120, 255],
        Spell::Poisoning | Spell::PoisonCloud | Spell::PoisonShot | Spell::Plague => {
            [140, 220, 96, 255]
        }
        Spell::Teleport | Spell::Blink | Spell::StormEscape => [120, 220, 255, 255],
        Spell::Hiding | Spell::MassHiding => [170, 150, 255, 255],
        Spell::SummonSkeleton | Spell::SummonShinsu | Spell::SummonHolyDeva => [240, 234, 200, 255],
        Spell::Healing | Spell::MassHealing | Spell::HealingCircle => [120, 240, 140, 255],
        Spell::MagicShield => [110, 170, 255, 255],
        Spell::FireBall | Spell::GreatFireBall | Spell::SoulFireBall => [255, 170, 90, 255],
        _ => [220, 220, 220, 255],
    }
}

/// 该技能是否沿施法方向铺开（区域类）。
#[must_use]
pub const fn is_area(spell: Spell) -> bool {
    matches!(
        spell,
        Spell::FireWall
            | Spell::PoisonCloud
            | Spell::Plague
            | Spell::ThunderStorm
            | Spell::Blizzard
            | Spell::IceStorm
            | Spell::MeteorShower
            | Spell::MeteorStrike
            | Spell::FlameField
    )
}

/// 该技能是否从施法者连向落点（指向类）。
#[must_use]
pub const fn is_bolt(spell: Spell) -> bool {
    matches!(
        spell,
        Spell::ThunderBolt
            | Spell::Lightning
            | Spell::FireBall
            | Spell::GreatFireBall
            | Spell::SoulFireBall
            | Spell::FireBurst
            | Spell::NapalmShot
            | Spell::VampireShot
            | Spell::PoisonShot
            | Spell::ElementalShot
    )
}

/// 方向 → 单位格偏移（与 Crystal `MirDirection` 的八方向一致）。
fn direction_step(direction: MirDirection) -> (i32, i32) {
    match direction {
        MirDirection::Up => (0, -1),
        MirDirection::UpRight => (1, -1),
        MirDirection::Right => (1, 0),
        MirDirection::DownRight => (1, 1),
        MirDirection::Down => (0, 1),
        MirDirection::DownLeft => (-1, 1),
        MirDirection::Left => (-1, 0),
        MirDirection::UpLeft => (-1, -1),
    }
}

/// 画一格高亮（半透明色块 + 1px 上边）。
fn draw_cell(frame: &mut Frame, scene: &MapScene, center: Point, cell: Point, color: [u8; 4]) {
    let (x, y) = scene.world_to_screen(center, cell);
    frame.fill_rect_alpha(x + 7, y, CELL_WIDTH, CELL_HEIGHT, color);
    frame.fill_rect(x + 7, y, CELL_WIDTH, 1, color);
}

/// 画所有未过期的技能表现。
///
/// `now_ms` 是本地时钟（Unix 毫秒）；`snapshot` 只用来找施法者位置（指向类技能）。
#[allow(clippy::too_many_arguments)]
pub fn draw(
    frame: &mut Frame,
    events: &[SpellEvent],
    snapshot: &WorldSnapshot,
    scene: &MapScene,
    center: Point,
    text: &UiText,
    language: Language,
    now_ms: i64,
) {
    for event in events {
        let duration = duration_ms(event.spell);
        let age = now_ms.saturating_sub(event.started_ms);
        if age < 0 || age >= duration {
            continue;
        }
        let left = duration.saturating_sub(age).clamp(0, duration);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let alpha = ((left * 255) / duration.max(1)) as u8;
        let mut tint = color(event.spell);
        tint[3] = alpha.clamp(40, 220);

        if is_area(event.spell) {
            // 区域类：`S.ObjectSpell` 是**逐格**下发的（火墙一次 5 格十字，每格一条，
            // 见服务端 `magic.rs::create_fire_wall`），所以只画事件自己那一格；
            // 只有没有逐格对象包的法术（如毒云）才按朝向铺 3 格做一个近似。
            let per_cell = event.from_object_spell || event.spell == Spell::FireWall;
            if per_cell {
                draw_cell(frame, scene, center, event.location, tint);
            } else {
                let (dx, dy) = direction_step(event.direction);
                for step in 0..3 {
                    let cell =
                        Point::new(event.location.x + dx * step, event.location.y + dy * step);
                    draw_cell(frame, scene, center, cell, tint);
                }
            }
        } else if is_bolt(event.spell) {
            // 指向类：从施法者格子画一条线到落点（没有施法者就只画落点）。
            if let Some(from) = snapshot.object_position(event.object_id) {
                let steps = ((from.x - event.location.x).abs())
                    .max((from.y - event.location.y).abs())
                    .max(1);
                for step in 0..=steps {
                    let cell = Point::new(
                        from.x + (event.location.x - from.x) * step / steps,
                        from.y + (event.location.y - from.y) * step / steps,
                    );
                    draw_cell(
                        frame,
                        scene,
                        center,
                        cell,
                        [tint[0], tint[1], tint[2], alpha / 2],
                    );
                }
            } else {
                draw_cell(frame, scene, center, event.location, tint);
            }
        } else {
            // 自身/召唤/治疗：落点一圈光。
            draw_cell(frame, scene, center, event.location, tint);
        }

        // 技能名浮字（Crystal 在法术位置飘 `Spell` 名，这里贴在格子上方）。
        if let Some(name) = labels::spell_name(event.spell, language) {
            let (x, y) = scene.world_to_screen(center, event.location);
            let area = Rect::new(x.saturating_sub(24), y.saturating_sub(16), 96, 16);
            text.draw_clipped(
                frame,
                area.x,
                area.y,
                name,
                11,
                [255, 255, 255, alpha.max(120)],
                area,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_split_into_cast_area_and_aura() {
        assert_eq!(duration_ms(Spell::ThunderBolt), CAST_MS);
        assert_eq!(duration_ms(Spell::FireWall), AREA_MS);
        assert_eq!(duration_ms(Spell::SummonSkeleton), AURA_MS);
        assert!(duration_ms(Spell::FireWall) > duration_ms(Spell::Teleport));
    }

    #[test]
    fn spell_visual_categories_cover_the_round_two_skills() {
        assert!(is_area(Spell::FireWall));
        assert!(is_bolt(Spell::ThunderBolt));
        assert!(!is_area(Spell::ThunderBolt));
        assert!(!is_bolt(Spell::FireWall));
        assert!(!is_area(Spell::Hiding));
        assert!(!is_bolt(Spell::Teleport));
        // 每个技能都要有可区分的颜色。
        assert_ne!(color(Spell::FireWall), color(Spell::ThunderBolt));
        assert_ne!(color(Spell::Poisoning), color(Spell::Hiding));
    }

    #[test]
    fn spell_names_are_localized() {
        assert_eq!(
            labels::spell_name(Spell::FireWall, Language::ZhCn),
            Some("火墙")
        );
        assert_eq!(
            labels::spell_name(Spell::ThunderBolt, Language::En),
            Some("Thunder Bolt")
        );
        assert_eq!(
            labels::spell_name(Spell::SummonSkeleton, Language::ZhCn),
            Some("召唤骷髅")
        );
        assert_eq!(labels::spell_name(Spell::Fencing, Language::ZhCn), None);
    }

    #[test]
    fn direction_steps_match_the_eight_way_enum() {
        assert_eq!(direction_step(MirDirection::Up), (0, -1));
        assert_eq!(direction_step(MirDirection::DownRight), (1, 1));
        assert_eq!(direction_step(MirDirection::Left), (-1, 0));
    }
}
