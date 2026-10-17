//! 增益栏（HUD 顶部）+ `S.ObjectEffect` 的短光效。
//!
//! 增益数据来自 [`WorldSnapshot::buffs`]（`S.AddBuff` / `S.RemoveBuff`）；剩余时间用
//! **客户端本地时钟**算（`ClientBuff.ExpireTime` 是 Unix 毫秒），不依赖服务器再发一次包。
//! 永久/暂停的增益分别显示「永久」「暂停」。
//!
//! 光效是可选提示：收到 `S.ObjectEffect`（魔法盾、元素结界等）时在目标脚下画一圈
//! 半透明圆环，约 600 ms 内淡出。状态存在 [`crate::ui::UiState::effects`]。

use mir_core::enums::SpellEffect;
use mir_core::{Language, Point};
use mir_render::{Frame, MapScene};

use crate::geom::Rect;
use crate::net::{BuffView, WorldSnapshot};
use crate::ui::UiText;
use crate::ui::hud::{PANEL_BORDER, SIZE_SMALL, STATUS_HEIGHT};
use crate::ui::labels::{self, Label};

/// 增益栏第一条的左边距（没有面板时的默认值）。
pub const BAR_LEFT: i32 = 10;
/// 增益栏顶边（紧贴状态栏下方）。
pub const BAR_TOP: u32 = STATUS_HEIGHT + 5;
/// 单个增益条目高度。
pub const ENTRY_HEIGHT: u32 = 20;
/// 条目间距。
pub const ENTRY_GAP: u32 = 4;
/// 图标边长。
pub const ICON_SIZE: u32 = 16;
/// 单个条目的宽度（图标 + 名称/秒数）。
pub const ENTRY_WIDTH: u32 = ICON_SIZE + 6 + 9 * SIZE_SMALL;
/// 最多显示多少条（其余显示 `+N`）。
pub const MAX_VISIBLE: usize = 6;
/// 光效默认时长（服务器 `ObjectEffect.time = 0` 时用）。
pub const DEFAULT_EFFECT_MS: i64 = 600;
/// 光效圆环半径。
pub const RING_RADIUS: i32 = 22;

/// 增益栏左边界：避开左侧的装备/商店面板。
///
/// 装备面板（272px）与商店面板（312px）都从 `x = 8` 开始；装备打开时商店还会右移，
/// 所以这里按「装备 + 商店 + 间距」逐级右移，保证 buff 不会被面板盖住。
#[must_use]
pub fn bar_left(equipment_open: bool, shop_open: bool) -> i32 {
    let equipment_width = i32::try_from(crate::ui::equipment::panel_width()).unwrap_or(0);
    let shop_width = i32::try_from(crate::ui::shop::SHOP_WIDTH).unwrap_or(0);
    match (equipment_open, shop_open) {
        (true, true) => BAR_LEFT + equipment_width + shop_width + 16,
        (true, false) => BAR_LEFT + equipment_width + 8,
        (false, true) => BAR_LEFT + shop_width + 8,
        (false, false) => BAR_LEFT,
    }
}

/// 一条当前光效。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveEffect {
    /// 目标对象。
    pub object_id: u32,
    /// 开始时间（Unix 毫秒）。
    pub started: i64,
    /// 结束时间（Unix 毫秒）。
    pub until: i64,
    /// 特效类型（决定颜色）。
    pub effect: SpellEffect,
}

impl ActiveEffect {
    /// 是否已经过期。
    #[must_use]
    pub const fn expired(self, now: i64) -> bool {
        now >= self.until
    }

    /// 剩余比例 0.0..=1.0（用整数百分比避免浮点告警）。
    #[must_use]
    pub fn alpha(self, now: i64) -> u8 {
        let total = self.until.saturating_sub(self.started).max(1);
        let left = self.until.saturating_sub(now).clamp(0, total);
        u8::try_from(left * 255 / total).unwrap_or(255)
    }
}

/// 增益显示文本：`魔法盾 8s` / `魔法盾 永久` / `魔法盾 暂停`。
#[must_use]
pub fn buff_text(buff: &BuffView, now: i64, language: Language) -> String {
    let name = labels::buff_name(buff.buff_type, language).map_or_else(
        || {
            format!(
                "{} #{}",
                Label::Buffs.text(language),
                buff.buff_type.value()
            )
        },
        str::to_owned,
    );
    if buff.paused {
        return format!("{name} {}", Label::Paused.text(language));
    }
    match buff.remaining_seconds(now) {
        Some(seconds) => format!("{name} {seconds}{}", Label::Seconds.text(language)),
        None => format!("{name} {}", Label::Infinite.text(language)),
    }
}

/// 增益条目的矩形（从 `left` 起连续排布，最多 [`MAX_VISIBLE`] 条）。
#[must_use]
pub fn entry_rects(_frame: Rect, count: usize, left: i32) -> Vec<Rect> {
    (0..count.min(MAX_VISIBLE))
        .map(|index| {
            Rect::new(
                left + i32::try_from(index).unwrap_or(0)
                    * i32::try_from(ENTRY_WIDTH + ENTRY_GAP).unwrap_or(0),
                i32::try_from(BAR_TOP + 2).unwrap_or(0),
                ENTRY_WIDTH,
                ENTRY_HEIGHT,
            )
        })
        .collect()
}

/// 给定左边界与帧宽，最多能放下几条（至少 1 条）。
#[must_use]
pub fn capacity(frame: Rect, left: i32) -> usize {
    let available = frame.right().saturating_sub(left).saturating_sub(8).max(0);
    let slot = i32::try_from(ENTRY_WIDTH + ENTRY_GAP).unwrap_or(1);
    usize::try_from(available / slot)
        .unwrap_or(1)
        .clamp(1, MAX_VISIBLE)
}

/// 图标底色（按增益类型区分，未收录的用固定色）。
#[must_use]
pub const fn buff_color(buff: mir_core::enums::BuffType) -> [u8; 4] {
    use mir_core::enums::BuffType as B;
    match buff {
        B::Hiding => [110, 110, 200, 255],
        B::Haste | B::SwiftFeet => [110, 200, 240, 255],
        B::Fury | B::Rage => [240, 90, 70, 255],
        B::SoulShield | B::MagicShield | B::ElementalBarrier | B::EnergyShield => {
            [120, 170, 250, 255]
        }
        B::BlessedArmour | B::ProtectionField | B::ImmortalSkin => [220, 200, 110, 255],
        B::Curse | B::Blindness => [160, 90, 200, 255],
        B::Exp | B::Drop | B::Gold => [240, 200, 80, 255],
        _ => [170, 190, 210, 255],
    }
}

/// 画增益栏；没有可见增益时什么也不画。`left` 由 [`bar_left`] 计算。
pub fn draw(
    frame: &mut Frame,
    snapshot: &WorldSnapshot,
    text: &UiText,
    language: Language,
    now: i64,
    left: i32,
) {
    let visible: Vec<&BuffView> = snapshot.buffs.iter().filter(|buff| buff.visible).collect();
    if visible.is_empty() {
        return;
    }
    let screen = Rect::screen(frame.width, frame.height);
    let visible_count = visible.len().min(capacity(screen, left));
    let rects = entry_rects(screen, visible_count, left);
    for (index, rect) in rects.iter().enumerate() {
        let Some(buff) = visible.get(index) else {
            break;
        };
        frame.fill_rect_alpha(rect.x, rect.y, rect.width, rect.height, [8, 12, 20, 190]);
        frame.fill_rect(rect.x, rect.y, rect.width, 1, PANEL_BORDER);
        frame.fill_rect(rect.x, rect.bottom() - 1, rect.width, 1, PANEL_BORDER);
        let icon = Rect::new(rect.x + 2, rect.y + 2, ICON_SIZE, ICON_SIZE);
        frame.fill_rect(
            icon.x,
            icon.y,
            icon.width,
            icon.height,
            buff_color(buff.buff_type),
        );
        frame.fill_rect(
            icon.x + 3,
            icon.y + 3,
            icon.width.saturating_sub(6),
            icon.height.saturating_sub(6),
            [255, 255, 255, 60],
        );
        // 没有真正的 buff 图标素材：用增益名的首字当图标，色块区分类型。
        let glyph = labels::buff_name(buff.buff_type, language)
            .and_then(|name| name.chars().next())
            .map_or_else(|| "#".to_owned(), |character| character.to_string());
        let glyph_width = text.text_width(&glyph, SIZE_SMALL);
        text.draw_clipped(
            frame,
            icon.x
                + (i32::try_from(icon.width).unwrap_or(0)
                    - i32::try_from(glyph_width).unwrap_or(0))
                    / 2,
            icon.y + 1,
            &glyph,
            SIZE_SMALL,
            [18, 24, 34, 255],
            icon,
        );
        let label = buff_text(buff, now, language);
        text.draw_clipped(
            frame,
            icon.right() + 4,
            rect.y + 4,
            &label,
            SIZE_SMALL,
            [235, 240, 250, 255],
            *rect,
        );
    }
    let hidden = visible.len().saturating_sub(rects.len());
    if hidden > 0 {
        if let Some(last) = rects.last() {
            text.draw_right(
                frame,
                last.right() + 24,
                last.y + 4,
                &format!("+{hidden}"),
                SIZE_SMALL,
                [200, 208, 220, 255],
            );
        }
    }
}

/// 光效颜色。
#[must_use]
pub const fn effect_color(effect: SpellEffect) -> [u8; 4] {
    match effect {
        SpellEffect::MagicShieldUp | SpellEffect::MagicShieldDown => [110, 170, 255, 255],
        SpellEffect::ElementalBarrierUp | SpellEffect::ElementalBarrierDown => [120, 240, 220, 255],
        SpellEffect::Healing => [120, 240, 140, 255],
        SpellEffect::Critical | SpellEffect::FatalSword => [255, 110, 90, 255],
        SpellEffect::Reflect | SpellEffect::Stunned => [255, 220, 110, 255],
        _ => [230, 230, 170, 255],
    }
}

/// 画所有未过期的光效圆环。
///
/// 圆环画在目标脚下：`world_to_screen` 的格原点向右 24、向下 10 像素是脚底中心；
/// 用 `blend_pixel` 逐像素画环，随时间淡出。
pub fn draw_effects(
    frame: &mut Frame,
    effects: &[ActiveEffect],
    snapshot: &WorldSnapshot,
    scene: &MapScene,
    center: Point,
    now: i64,
) {
    for effect in effects.iter().filter(|effect| !effect.expired(now)) {
        let Some(position) = snapshot.object_position(effect.object_id) else {
            continue;
        };
        let (cell_x, cell_y) = scene.world_to_screen(center, position);
        let center_x = cell_x + 24;
        let center_y = cell_y + 10;
        let fade = effect.alpha(now);
        if fade == 0 {
            continue;
        }
        let color = effect_color(effect.effect);
        for y in -RING_RADIUS..=RING_RADIUS {
            for x in -RING_RADIUS..=RING_RADIUS {
                let distance_sq = x * x + y * y;
                let inner = (RING_RADIUS - 3) * (RING_RADIUS - 3);
                let outer = RING_RADIUS * RING_RADIUS;
                if distance_sq < inner || distance_sq > outer {
                    continue;
                }
                frame.blend_pixel(center_x + x, center_y + y, color, fade / 2);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir_core::enums::BuffType;

    fn buff(kind: BuffType, expire: i64) -> BuffView {
        BuffView {
            buff_type: kind,
            visible: true,
            object_id: 7,
            expire_time: expire,
            infinite: false,
            paused: false,
        }
    }

    #[test]
    fn remaining_seconds_count_down_with_the_local_clock() {
        let buff = buff(BuffType::MagicShield, 10_000);
        assert_eq!(buff.remaining_seconds(2_000), Some(8));
        assert_eq!(buff.remaining_seconds(9_500), Some(1), "向上取整");
        assert_eq!(buff.remaining_seconds(11_000), Some(0));
        assert_eq!(buff_text(&buff, 2_000, Language::ZhCn), "魔法盾 8秒");
        assert_eq!(buff_text(&buff, 2_000, Language::En), "Magic Shield 8s");
    }

    #[test]
    fn infinite_and_paused_buffs_do_not_count_down() {
        let mut infinite = buff(BuffType::Haste, 0);
        infinite.infinite = true;
        assert_eq!(infinite.remaining_seconds(1), None);
        assert!(buff_text(&infinite, 1, Language::ZhCn).contains("永久"));
        let mut paused = buff(BuffType::Haste, 10_000);
        paused.paused = true;
        assert!(buff_text(&paused, 1, Language::ZhCn).contains("暂停"));
    }

    #[test]
    fn entry_rects_are_capped_and_do_not_overlap() {
        let rects = entry_rects(Rect::screen(800, 600), 20, BAR_LEFT);
        assert_eq!(rects.len(), MAX_VISIBLE);
        for pair in rects.windows(2) {
            assert!(pair[0].right() <= pair[1].x || pair[0].bottom() <= pair[1].y);
        }
        assert!(rects[0].y >= STATUS_HEIGHT as i32);
        assert_eq!(rects[0].x, BAR_LEFT);
    }

    #[test]
    fn bar_moves_right_past_the_left_panels_and_capacity_shrinks() {
        let screen = Rect::screen(800, 600);
        assert_eq!(bar_left(false, false), BAR_LEFT);
        let equipment = bar_left(true, false);
        assert!(equipment > BAR_LEFT);
        assert!(equipment >= i32::try_from(crate::ui::equipment::panel_width()).unwrap_or(0));
        let both = bar_left(true, true);
        assert!(both > equipment);
        assert!(capacity(screen, both) >= 1);
        assert!(capacity(Rect::screen(300, 600), BAR_LEFT) < MAX_VISIBLE);
    }

    #[test]
    fn effect_ring_fades_out_over_its_lifetime() {
        let effect = ActiveEffect {
            object_id: 7,
            started: 1_000,
            until: 1_600,
            effect: SpellEffect::MagicShieldUp,
        };
        assert!(!effect.expired(1_100));
        assert!(effect.expired(1_700));
        assert!(effect.alpha(1_100) > effect.alpha(1_500));
        assert_eq!(effect.alpha(2_000), 0);
        assert_ne!(
            effect_color(SpellEffect::MagicShieldUp),
            effect_color(SpellEffect::Healing)
        );
    }
}
