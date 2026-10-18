//! 小型「确认 / 取消」弹窗：交易邀请与组队邀请共用。
//!
//! Crystal 用 `MirMessageBox`（`Client/MirControls/MirMessageBox.cs`）弹系统提示，例如
//! `GameScene.cs:6305-6310`（交易邀请）与 `:4994-5005`（组队邀请）。
//! 本作没有通用控件系统，只做这一种两按钮弹窗：一行正文 + 接受 + 拒绝。

use mir_core::{Language, Point};
use mir_render::Frame;

use crate::geom::Rect;
use crate::ui::UiText;
use crate::ui::hud::{PANEL_BACKGROUND, SIZE_BODY, SIZE_SMALL, TITLE_COLOR};
use crate::ui::labels::Label;

/// 弹窗宽度。
pub const NOTICE_WIDTH: u32 = 360;
/// 弹窗高度。
pub const NOTICE_HEIGHT: u32 = 108;
/// 按钮高度。
pub const BUTTON_HEIGHT: u32 = 24;

/// 弹窗布局。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoticeLayout {
    /// 弹窗矩形。
    pub panel: Rect,
    /// 接受按钮。
    pub accept: Rect,
    /// 拒绝按钮。
    pub reject: Rect,
}

/// 计算弹窗布局（屏幕居中；Crystal 的 `MirMessageBox` 也是居中）。
#[must_use]
pub fn notice_layout(frame: Rect) -> NoticeLayout {
    let width = NOTICE_WIDTH.min(frame.width.saturating_sub(24)).max(220);
    let height = NOTICE_HEIGHT.min(frame.height.saturating_sub(24)).max(90);
    let x =
        frame.x + (i32::try_from(frame.width).unwrap_or(0) - i32::try_from(width).unwrap_or(0)) / 2;
    let y = frame.y
        + (i32::try_from(frame.height).unwrap_or(0) - i32::try_from(height).unwrap_or(0)) / 2;
    let panel = Rect::new(x, y, width, height);
    let button_width = (width.saturating_sub(36) / 2).max(70);
    let button_y = panel.bottom() - 36;
    let accept = Rect::new(panel.x + 12, button_y, button_width, BUTTON_HEIGHT);
    let reject = Rect::new(
        panel.right() - 12 - i32::try_from(button_width).unwrap_or(0),
        button_y,
        button_width,
        BUTTON_HEIGHT,
    );
    NoticeLayout {
        panel,
        accept,
        reject,
    }
}

/// 画弹窗：正文 + 接受/拒绝。
pub fn draw(
    frame: &mut Frame,
    text: &UiText,
    layout: &NoticeLayout,
    message: &str,
    language: Language,
    mouse: Point,
) {
    crate::ui::draw_panel(frame, layout.panel);
    frame.fill_rect(
        layout.panel.x,
        layout.panel.y,
        layout.panel.width,
        1,
        PANEL_BACKGROUND,
    );
    text.draw_center(
        frame,
        Rect::new(
            layout.panel.x,
            layout.panel.y + 18,
            layout.panel.width,
            SIZE_BODY + 2,
        ),
        message,
        SIZE_BODY,
        TITLE_COLOR,
    );
    draw_button(
        frame,
        text,
        layout.accept,
        Label::Accept.text(language),
        mouse,
    );
    draw_button(
        frame,
        text,
        layout.reject,
        Label::Reject.text(language),
        mouse,
    );
}

/// 画一个按钮（与行会/交易面板同款样式）。
fn draw_button(frame: &mut Frame, text: &UiText, rect: Rect, label: &str, mouse: Point) {
    let hovered = rect.contains(mouse);
    let fill = if hovered {
        [92, 82, 42, 255]
    } else {
        [44, 52, 66, 255]
    };
    frame.fill_rect(rect.x, rect.y, rect.width, rect.height, fill);
    frame.fill_rect(rect.x, rect.y, rect.width, 1, [140, 156, 180, 255]);
    frame.fill_rect(rect.x, rect.bottom() - 1, rect.width, 1, [36, 44, 56, 255]);
    text.draw_center(frame, rect, label, SIZE_SMALL, [255, 240, 180, 255]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notice_is_centered_and_buttons_hit_testable() {
        let frame = Rect::screen(800, 600);
        let layout = notice_layout(frame);
        assert_eq!(layout.panel.center_x(), 400);
        assert_eq!(layout.panel.center_y(), 300);
        assert!(layout.panel.contains(layout.accept.center()));
        assert!(layout.panel.contains(layout.reject.center()));
        assert!(layout.reject.x > layout.accept.x);
        let small = notice_layout(Rect::screen(320, 240));
        assert_eq!(small.panel.center_x(), 160);
        assert!(small.panel.width <= 320);
    }
}
