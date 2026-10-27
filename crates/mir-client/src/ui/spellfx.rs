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

/// 技能命中闪光用的帧数（`magic.wzl` 前 10 帧，Crystal 的 `Effect(Magic, start, 10, ...)`）。
pub const MAGIC_FX_FRAMES: i64 = 10;
/// 命中闪光每帧时长（10 帧 × 60 ms = 600 ms，覆盖一次施法的可见命中窗口）。
pub const MAGIC_FX_FRAME_MS: i64 = 60;

/// 149 `S.ObjectSpell` 来源 `FireWall` 的起始帧（Crystal `SpellObject.cs:49`：
/// `Libraries.Magic`、`DrawFrame = 1630`）。
///
/// 有限切片：只迁移六帧 elapsed 循环 + 格锚点 + 当帧 signed offset 一次；Crystal 的
/// `rate = 1` 与 `src * 0.8 * alpha` 调光（`DrawBlend` 子集未实施）、`Light`、
/// `DrawColour`、严格 `NextMotion = CMain.Time + FrameInterval`（`SpellObject.cs:302`）
/// 与对象寿命均未迁移。
pub const FIRE_WALL_BASE_INDEX: u32 = 1_630;
/// `FireWall` 帧数（Crystal `SpellObject.cs:49` 的 `FrameCount = 6`）。
pub const FIRE_WALL_FRAME_COUNT: i64 = 6;
/// `FireWall` 每帧时长（Crystal `SpellObject.cs:49` 的 `FrameInterval = 120` 毫秒）。
pub const FIRE_WALL_FRAME_MS: i64 = 120;

/// 149 `S.ObjectSpell` 来源 `PoisonCloud` 的起始帧（Crystal `SpellObject.cs:57-64`：
/// `Libraries.Magic2`、`DrawFrame = 1650`、`FrameCount = 20`、`FrameInterval = 120`）。
///
/// 有限切片：只迁移二十帧 elapsed 循环 + 格锚点 + 当帧 signed offset 一次；Crystal 的
/// `DrawBlend(..., true, 0.8F)` 调光（`src * 0.8 * alpha`）、`Light3`、`DrawColour`、
/// 严格 `NextMotion`（`SpellObject.cs:288-302`）与对象寿命均未迁移。
pub const POISON_CLOUD_BASE_INDEX: u32 = 1_650;
/// `PoisonCloud` 帧数（Crystal `SpellObject.cs:57-64` 的 `FrameCount = 20`）。
pub const POISON_CLOUD_FRAME_COUNT: i64 = 20;
/// `PoisonCloud` 每帧时长（Crystal `SpellObject.cs:57-64` 的 `FrameInterval = 120` 毫秒）。
pub const POISON_CLOUD_FRAME_MS: i64 = 120;

/// 画技能命中闪光（`magic.wzl` 精灵）。
///
/// 与 [`draw`] 的色块叠加并存：色块给出范围/路径，精灵给出「打中了」的反馈。
/// 缺 `magic.wzl` / 缺帧时静默跳过（[`MapScene::effect_image`] 返回 `None`）。
///
/// 149 `S.ObjectSpell` 来源的 `FireWall` 是例外：走 Crystal `SpellObject.cs:49` 的有限
/// 切片（`magic.wzl[1630..=1635]`、120 ms/帧、六帧 elapsed 循环），落点为格锚点并在
/// `draw_effect_blend` 里消费当帧 i16 signed offset 一次；其余技能/来源仍走通用闪光路径。
///
/// 149 `S.ObjectSpell` 来源的 `PoisonCloud` 同理走 Crystal `SpellObject.cs:57-64` 的有限
/// 切片（`Magic2[1650..=1669]`、120 ms/帧、二十帧 elapsed 循环），复用 `FxKind::Swing`
/// 的零起点寻址与当帧 signed offset；非 149 来源保持通用路径（其他 26 技能映射 GAP-5）。
pub fn draw_sprites(
    frame: &mut Frame,
    events: &[SpellEvent],
    scene: &mut MapScene,
    center: Point,
    now_ms: i64,
) {
    for event in events {
        let duration = duration_ms(event.spell);
        let age = now_ms.saturating_sub(event.started_ms);
        if age < 0 || age >= duration {
            continue;
        }
        // 149 `S.ObjectSpell` 来源的 FireWall：Crystal `SpellObject.cs:49` 的有限切片——
        // `magic.wzl[1630..=1635]` 六帧 elapsed 循环（120 ms/帧），落点是格锚点，
        // 当帧 i16 signed offset 由 `draw_effect_blend` 叠一次（Add 混合）。
        // 未迁移：`DrawBlend` 的 `rate = 1` 与 `src * 0.8 * alpha` 调光、`Light`、
        // `DrawColour`、严格 `NextMotion`（`SpellObject.cs:302`
        // `NextMotion = CMain.Time + FrameInterval`）与对象寿命；Crystal 起手帧 1620
        // 也不在本切片内，非 149 来源保持旧通用路径。
        if event.spell == Spell::FireWall && event.from_object_spell {
            let phase = (age / FIRE_WALL_FRAME_MS) % FIRE_WALL_FRAME_COUNT;
            let index = FIRE_WALL_BASE_INDEX + u32::try_from(phase).unwrap_or(0);
            let (x, y) = scene.world_to_screen(center, event.location);
            scene.draw_effect_blend(frame, mir_render::FxKind::Magic, index, x, y, true);
            continue;
        }
        // 149 `S.ObjectSpell` 来源的 PoisonCloud：Crystal `SpellObject.cs:57-64` 的有限
        // 切片——`Magic2` 帧 `1650..=1669`、`FrameInterval = 120`、`FrameCount = 20` 的
        // elapsed 循环（`phase = (age / 120) % 20`、`index = 1650 + phase`）。落点是格
        // 锚点（`SpellObject.cs:323-339`：`DrawLocation` 为格坐标 + 显示偏移
        // `GlobalDisplayLocationOffset`/`User.OffSetMove`），当帧 i16 signed offset 由
        // `draw_effect_blend` 按 `MLibrary.cs:692-709` 的「先 offset 后裁剪、恰好一次」
        // 消费（Add 混合 = `DrawBlend(true, 0.8F)` 的 CPU 子集）。`FxKind::Swing` 仅作为
        // `Magic2` + `start_frame() = 0` 的零起点帧号寻址复用，**仅限本 149 毒云分支**，
        // 不改变 `Swing` 的公共语义。
        // PARTIAL（未实施）：`SpellObject.cs:288-302` 的严格 `NextMotion`
        // （`NextMotion = CMain.Time + FrameInterval`、初始 100ms 取整、每个 `Process`
        // 至多一步）与对象寿命；`DrawBlend(true, 0.8F)` 的 `rate = 0.8`、
        // `src * 0.8 * alpha` 调光、`Light3`、`DrawColour`。`AREA_MS = 4000` 是产品瞬态
        // 窗（`duration_ms`），不是 Crystal 对象寿命。其他 26 技能映射仍 GAP-5；非 149
        // 来源/其他 `ObjectSpell` 与 FireWall（R71）行为不变，通用路径不改。
        if event.spell == Spell::PoisonCloud && event.from_object_spell {
            let phase = (age / POISON_CLOUD_FRAME_MS) % POISON_CLOUD_FRAME_COUNT;
            let index = POISON_CLOUD_BASE_INDEX + u32::try_from(phase).unwrap_or(0);
            let (x, y) = scene.world_to_screen(center, event.location);
            scene.draw_effect_blend(frame, mir_render::FxKind::Swing, index, x, y, true);
            continue;
        }
        let frame_index = (age / MAGIC_FX_FRAME_MS).clamp(0, MAGIC_FX_FRAMES - 1);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let frame_index = frame_index as u32;
        // 雷电用 `magic2.wzl` 的闪电束，其它技能用 `magic.wzl` 的火环。
        let kind = if matches!(event.spell, Spell::ThunderBolt | Spell::Lightning) {
            mir_render::FxKind::Bolt
        } else {
            mir_render::FxKind::Magic
        };
        let Some((width, height)) = scene.effect_size(kind, frame_index) else {
            continue;
        };
        let (x, y) = scene.world_to_screen(center, event.location);
        // 扁平语料的 `magic*.wzl` 没有可靠的逐帧锚点：把特效按格居中，
        // 视觉上落在命中格中央（雷电束 48×100 居中后不再飘到上一格）。
        let x = x + (mir_render::CELL_WIDTH - i32::try_from(width).unwrap_or(0)) / 2;
        let y = y + (mir_render::CELL_HEIGHT - i32::try_from(height).unwrap_or(0)) / 2;
        scene.draw_effect(frame, kind, frame_index, x, y);
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
