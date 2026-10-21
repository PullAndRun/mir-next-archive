//! 撞墙/移动被拒的本地提示（**不新增协议、不发送任何包**）。
//!
//! 服务端对不可走格会回 `S.UserLocation` 同坐标（移动被拒）；客户端在该事件上
//! 调 [`WallBumpFeedback::inject`]，面板显示短暂提示。节流 150 ms，避免连续撞墙
//! 刷屏；单次被拒也会显示一次。

use crate::ui::labels::Label;

/// 两次提示之间的最小间隔（毫秒）。
pub const WALL_BUMP_INTERVAL_MS: u64 = 150;
/// 提示保持可见的时长（毫秒）。
pub const WALL_BUMP_VISIBLE_MS: u64 = 900;

/// 撞墙提示状态。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct WallBumpFeedback {
    /// 上次显示提示的时刻（本地单调毫秒）。
    last_shown_ms: Option<u64>,
    /// 已显示的提示次数（测试/诊断用）。
    shown: u64,
    /// 当前提示到期时刻。
    visible_until_ms: u64,
}

impl WallBumpFeedback {
    /// 收到一次“移动被拒/回拉”事件；返回是否真的显示了提示。
    pub fn inject(&mut self, now_ms: u64) -> bool {
        if let Some(last) = self.last_shown_ms {
            if now_ms.saturating_sub(last) < WALL_BUMP_INTERVAL_MS {
                return false;
            }
        }
        self.last_shown_ms = Some(now_ms);
        self.shown = self.shown.saturating_add(1);
        self.visible_until_ms = now_ms.saturating_add(WALL_BUMP_VISIBLE_MS);
        true
    }

    /// 当前是否应显示提示。
    #[must_use]
    pub fn visible(&self, now_ms: u64) -> bool {
        now_ms < self.visible_until_ms
    }

    /// 已显示次数。
    #[must_use]
    pub fn shown(&self) -> u64 {
        self.shown
    }

    /// 提示文字（中文“前方受阻”/英文“Blocked”）。
    #[must_use]
    pub fn message(language: mir_core::Language) -> &'static str {
        Label::WallBump.text(language)
    }

    /// 提示的矩形（绘制文字用）。
    #[must_use]
    pub fn rect(screen: crate::geom::Rect) -> crate::geom::Rect {
        let width = 120;
        let x = screen.x + (screen.width.saturating_sub(width) / 2) as i32;
        crate::geom::Rect::new(x, screen.y + 70, width, 18)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ten_rejections_within_one_second_show_at_least_two_with_gaps() {
        let mut feedback = WallBumpFeedback::default();
        let mut times = Vec::new();
        for i in 0..10u64 {
            let now = i * 100; // 10 次 ≤ 1s
            if feedback.inject(now) {
                times.push(now);
            }
        }
        assert!(times.len() >= 2, "10 次被拒应至少提示 2 次");
        for pair in times.windows(2) {
            assert!(
                pair[1] - pair[0] >= WALL_BUMP_INTERVAL_MS,
                "提示间隔必须 ≥150ms: {times:?}"
            );
        }
        assert_eq!(feedback.shown(), times.len() as u64);
    }

    #[test]
    fn single_rejection_shows_once() {
        let mut feedback = WallBumpFeedback::default();
        assert!(feedback.inject(1_000));
        assert!(feedback.visible(1_100));
        assert!(!feedback.visible(2_000));
        assert!(!feedback.inject(1_100), "150ms 内不重复提示");
        assert!(feedback.inject(1_150), "到点后允许");
        assert_eq!(feedback.shown(), 2);
    }

    #[test]
    fn no_protocol_packets_are_needed() {
        // 这个模块只有本地状态；注入 100 次也不产生任何网络动作。
        let mut feedback = WallBumpFeedback::default();
        for i in 0..100u64 {
            feedback.inject(i * 200);
        }
        assert_eq!(feedback.shown(), 100);
    }
}
