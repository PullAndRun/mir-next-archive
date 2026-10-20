//! 新手引导追踪面板：读 [`crate::newbie::NewbieChain`]，用快照判断当前步骤。
//!
//! 只做提示，不改协议、不自动发任务包；玩家按提示自行对话/打怪/买装备。

use mir_core::Language;

use crate::geom::Rect;
use crate::net::WorldSnapshot;
use crate::newbie::NewbieChain;
use crate::ui::hud;
use crate::ui::labels::Label;
use crate::ui::text::UiText;

/// 面板宽。
pub const PANEL_WIDTH: u32 = 280;
/// 行高。
pub const ROW_HEIGHT: u32 = 16;
/// 面板内边距。
pub const PADDING: u32 = 6;

/// 面板矩形（左下角，避开顶部 HUD；`screen` 是整屏）。
#[must_use]
pub fn panel_rect(screen: Rect) -> Rect {
    let height = 24 + ROW_HEIGHT * 6 + PADDING * 2;
    let x = screen.x + 8;
    let y = (screen.bottom() - i32::try_from(height).unwrap_or(0) - 8).max(screen.y + 8);
    Rect::new(
        x,
        y,
        PANEL_WIDTH.min(screen.width.saturating_sub(16)),
        height,
    )
}

/// 画引导面板；全部完成时返回 `false`（不画）。
pub fn draw(
    frame: &mut mir_render::Frame,
    screen: Rect,
    snapshot: &WorldSnapshot,
    chain: &NewbieChain,
    text: &UiText,
    language: Language,
) -> bool {
    let done = chain.done_flags(snapshot);
    let current = chain.current_step(snapshot);
    if current >= chain.steps.len() {
        return false;
    }
    let panel = panel_rect(screen);
    crate::ui::draw_panel(frame, panel);
    let english = language == Language::En;
    text.draw_clipped(
        frame,
        panel.x + i32::try_from(PADDING).unwrap_or(0),
        panel.y + 4,
        Label::NewbieGuide.text(language),
        hud::SIZE_SMALL,
        hud::GOLD_COLOR,
        Rect::new(panel.x, panel.y, panel.width, 18),
    );
    let mut y = panel.y + 20;
    for (index, step) in chain.steps.iter().enumerate() {
        let marker = if done.get(index).copied().unwrap_or(false) {
            "[x]"
        } else if index == current {
            "[>]"
        } else {
            "[ ]"
        };
        let color = if index == current {
            [255, 236, 140, 255]
        } else if done.get(index).copied().unwrap_or(false) {
            hud::MUTED_COLOR
        } else {
            hud::BODY_COLOR
        };
        let title = format!("{marker} {}", step.title(english));
        text.draw_clipped(
            frame,
            panel.x + i32::try_from(PADDING).unwrap_or(0),
            y,
            &title,
            hud::SIZE_SMALL,
            color,
            Rect::new(panel.x, y, panel.width, ROW_HEIGHT),
        );
        y += i32::try_from(ROW_HEIGHT).unwrap_or(16);
    }
    if let Some(step) = chain.steps.get(current) {
        text.draw_clipped(
            frame,
            panel.x + i32::try_from(PADDING).unwrap_or(0),
            y + 2,
            step.hint(english),
            hud::SIZE_SMALL,
            hud::MUTED_COLOR,
            Rect::new(panel.x, y, panel.width.saturating_sub(PADDING * 2), 18),
        );
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::QuestView;

    #[test]
    fn panel_fits_screen() {
        let screen = Rect::screen(800, 600);
        let panel = panel_rect(screen);
        assert!(panel.right() <= screen.right());
        assert!(panel.bottom() <= screen.bottom());
    }

    #[test]
    fn draws_until_all_steps_done() {
        let mut snapshot = WorldSnapshot::default();
        let chain = NewbieChain::default();
        let mut frame = mir_render::Frame::new(800, 600);
        let text = UiText::fallback();
        assert!(draw(
            &mut frame,
            Rect::screen(800, 600),
            &snapshot,
            &chain,
            &text,
            Language::ZhCn
        ));
        snapshot.quests.push(QuestView {
            taken: true,
            completed: true,
            ..QuestView::default()
        });
        snapshot.magics.push(crate::net::MagicView {
            name: "火球术".to_owned(),
            spell: mir_core::enums::Spell::FireBall,
            level: 1,
            key: 0,
            base_cost: 0,
            icon: 0,
            range: 0,
        });
        snapshot.equipment = vec![
            Some(crate::net::InventoryItem {
                unique_id: 1,
                item_index: 1,
                count: 1,
                current_dura: 1,
                max_dura: 1,
            }),
            Some(crate::net::InventoryItem {
                unique_id: 2,
                item_index: 2,
                count: 1,
                current_dura: 1,
                max_dura: 1,
            }),
        ];
        assert!(!draw(
            &mut frame,
            Rect::screen(800, 600),
            &snapshot,
            &chain,
            &text,
            Language::ZhCn
        ));
    }
}
