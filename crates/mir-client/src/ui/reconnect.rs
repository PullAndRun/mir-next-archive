//! 断线重连状态机（客户端）。
//!
//! 断线后按 1s / 2s / 4s 退避自动重连 3 次；成功回到 `Idle`，3 次失败进入
//! `Failed`（UI 显示「无法连接」并返回登录界面）。状态机不碰网络，只吃
//! 「断线/到点/成功/失败」事件，便于确定性测试。

use mir_core::Language;

use crate::ui::labels::Label;

/// 最多尝试次数。
pub const MAX_ATTEMPTS: u8 = 3;
/// 第 1/2/3 次重连前的退避（毫秒）。
pub const BACKOFF_MS: [u64; 3] = [1_000, 2_000, 4_000];

/// 重连阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconnectPhase {
    /// 正常在线。
    Idle,
    /// 等待退避到点，`attempt` 是即将进行的第几次尝试（1..=3）。
    Waiting {
        /// 第几次尝试。
        attempt: u8,
        /// 到点时刻（本地毫秒）。
        at_ms: u64,
    },
    /// 正在发起连接（已到点，还没收到成功/失败）。
    Connecting {
        /// 第几次尝试。
        attempt: u8,
    },
    /// 3 次失败，放弃。
    Failed,
}

/// 重连状态机。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconnectState {
    phase: ReconnectPhase,
}

impl Default for ReconnectState {
    fn default() -> Self {
        Self {
            phase: ReconnectPhase::Idle,
        }
    }
}

impl ReconnectState {
    /// 当前阶段。
    #[must_use]
    pub const fn phase(&self) -> ReconnectPhase {
        self.phase
    }

    /// 是否正在重连（等待或连接中）。
    #[must_use]
    pub const fn is_reconnecting(&self) -> bool {
        matches!(
            self.phase,
            ReconnectPhase::Waiting { .. } | ReconnectPhase::Connecting { .. }
        )
    }

    /// 是否已放弃（3 次失败）。
    #[must_use]
    pub const fn failed(&self) -> bool {
        matches!(self.phase, ReconnectPhase::Failed)
    }

    /// 断线：从第 1 次尝试开始（1s 后）。
    pub fn disconnected(&mut self, now_ms: u64) {
        if self.failed() || self.is_reconnecting() {
            return;
        }
        self.phase = ReconnectPhase::Waiting {
            attempt: 1,
            at_ms: now_ms.saturating_add(BACKOFF_MS[0]),
        };
    }

    /// 到点则返回要尝试的第几次（1..=3），并把阶段推进到 `Connecting`。
    pub fn due(&mut self, now_ms: u64) -> Option<u8> {
        let ReconnectPhase::Waiting { attempt, at_ms } = self.phase else {
            return None;
        };
        if now_ms < at_ms {
            return None;
        }
        self.phase = ReconnectPhase::Connecting { attempt };
        Some(attempt)
    }

    /// 连接成功：回到 `Idle`。
    pub fn succeeded(&mut self) {
        self.phase = ReconnectPhase::Idle;
    }

    /// 一次尝试失败：还有次数就排下一次退避，否则进入 `Failed`。
    pub fn attempt_failed(&mut self, now_ms: u64) {
        let ReconnectPhase::Connecting { attempt } = self.phase else {
            return;
        };
        if attempt >= MAX_ATTEMPTS {
            self.phase = ReconnectPhase::Failed;
            return;
        }
        let next = attempt + 1;
        let delay = BACKOFF_MS[usize::from(next - 1)];
        self.phase = ReconnectPhase::Waiting {
            attempt: next,
            at_ms: now_ms.saturating_add(delay),
        };
    }

    /// 重连提示文案（`None` = 不在重连）。
    #[must_use]
    pub fn message(&self, language: Language) -> Option<String> {
        match self.phase {
            ReconnectPhase::Idle => None,
            ReconnectPhase::Failed => Some(Label::ConnectFailed.text(language).to_owned()),
            ReconnectPhase::Waiting { attempt, .. } | ReconnectPhase::Connecting { attempt } => {
                Some(
                    Label::Reconnecting
                        .text(language)
                        .replace("{}", &attempt.to_string()),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn succeeds_on_first_attempt() {
        let mut state = ReconnectState::default();
        state.disconnected(1_000);
        assert_eq!(state.due(1_999), None);
        assert_eq!(state.due(2_000), Some(1));
        assert!(state.is_reconnecting());
        state.succeeded();
        assert_eq!(state.phase(), ReconnectPhase::Idle);
        assert!(state.message(Language::ZhCn).is_none());
    }

    #[test]
    fn retries_with_exponential_backoff_and_gives_up() {
        let mut state = ReconnectState::default();
        state.disconnected(0);
        assert_eq!(state.due(1_000), Some(1));
        state.attempt_failed(1_000); // 下一次 2s 后
        assert_eq!(state.due(2_999), None);
        assert_eq!(state.due(3_000), Some(2));
        state.attempt_failed(3_000); // 下一次 4s 后
        assert_eq!(state.due(6_999), None);
        assert_eq!(state.due(7_000), Some(3));
        state.attempt_failed(7_000);
        assert!(state.failed());
        assert!(!state.is_reconnecting());
        assert_eq!(
            state.message(Language::En),
            Some("Connection failed. Try again.".to_owned())
        );
    }

    #[test]
    fn message_reports_attempt_number() {
        let mut state = ReconnectState::default();
        state.disconnected(0);
        assert_eq!(
            state.message(Language::ZhCn),
            Some("连接已断开，正在重连（1/3）".to_owned())
        );
        state.due(1_000);
        state.attempt_failed(1_000);
        assert_eq!(
            state.message(Language::En),
            Some("Disconnected, reconnecting (2/3)".to_owned())
        );
    }

    #[test]
    fn duplicate_disconnect_does_not_reset_progress() {
        let mut state = ReconnectState::default();
        state.disconnected(0);
        state.due(1_000);
        state.attempt_failed(1_000);
        state.disconnected(1_500);
        assert_eq!(
            state.message(Language::ZhCn),
            Some("连接已断开，正在重连（2/3）".to_owned())
        );
    }
}
