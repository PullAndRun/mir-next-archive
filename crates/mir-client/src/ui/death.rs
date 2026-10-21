//! 死亡弹窗：`S.Death` 后等死亡动画播完，再问「是否回城复活？」。
//!
//! 时序（与兵部服务端对齐）：
//! * `S.Death` → `SelfPlayer.dead = true`、`action = Dying`、记录 `action_started_ms`；
//! * 动画播完（`HUMAN_DYING.duration_ms()`；素材缺失时 600ms 兜底）→ 弹窗；
//! * 「是」→ 恰好一个 `C.TownRevive`（index 68，空 payload）；「否」→ 只关窗；
//! * `S.Revived` / `S.UserLocation` 清 `dead`，弹窗自动消失。
//!
//! 本模块只有本地状态，不新增协议；时钟由调用方传入（Unix 毫秒），测试可直接注入。

use mir_core::{Language, Point};
use mir_render::Frame;

use crate::geom::Rect;
use crate::net::SelfPlayer;
use crate::ui::UiText;
use crate::ui::hud::{PANEL_BACKGROUND, SIZE_BODY, SIZE_SMALL, TITLE_COLOR};
use crate::ui::labels::Label;

/// 死亡动画的兜底时长（素材/帧表不可用时；正常用 `HUMAN_DYING.duration_ms()`）。
pub const DEATH_FALLBACK_MS: u64 = 600;
/// 死亡弹窗宽度。
pub const DEATH_WIDTH: u32 = 360;
/// 死亡弹窗高度。
pub const DEATH_HEIGHT: u32 = 108;
/// 按钮高度。
pub const DEATH_BUTTON_HEIGHT: u32 = 24;

/// 死亡动画时长：优先用 `mir-render` 的 actor 帧表，异常时 600ms 兜底。
#[must_use]
pub fn death_animation_ms() -> u64 {
    let duration = mir_render::HUMAN_DYING.duration_ms();
    if duration == 0 {
        DEATH_FALLBACK_MS
    } else {
        duration
    }
}

/// 死亡弹窗状态（只记录“本次死亡是否已关闭/已确认”，不保存网络连接）。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DeathPrompt {
    /// 已关闭/已确认的死亡动作起点；新的死亡会换 `action_started_ms`，从而重新弹出。
    dismissed_started_ms: Option<u64>,
}

impl DeathPrompt {
    /// 动画是否已经播完（`Dead` 视为播完；Dying 按帧表时长）。
    #[must_use]
    pub fn animation_finished(player: &SelfPlayer, now_ms: u64) -> bool {
        if !player.dead {
            return false;
        }
        if player.action == mir_render::animation::ActorAction::Dead {
            return true;
        }
        now_ms
            >= player
                .action_started_ms
                .saturating_add(death_animation_ms())
    }

    /// 现在是否应显示弹窗。
    #[must_use]
    pub fn visible(&self, player: &SelfPlayer, now_ms: u64) -> bool {
        player.dead
            && Self::animation_finished(player, now_ms)
            && self.dismissed_started_ms != Some(player.action_started_ms)
    }

    /// 点「否」：关闭本次弹窗；不发包。
    pub fn dismiss(&mut self, player: &SelfPlayer) {
        self.dismissed_started_ms = Some(player.action_started_ms);
    }

    /// 点「是」：标记已确认，保证重复点击不会重复发 `C.TownRevive`。
    pub fn confirm(&mut self, player: &SelfPlayer) {
        self.dismissed_started_ms = Some(player.action_started_ms);
    }

    /// 复活/离世状态变化：活着时清掉旧标记。
    pub fn sync(&mut self, player: &SelfPlayer) {
        if !player.dead {
            self.dismissed_started_ms = None;
        }
    }
}

/// 弹窗布局（屏幕居中）。
#[must_use]
pub fn death_layout(frame: Rect) -> crate::ui::notice::NoticeLayout {
    crate::ui::notice::notice_layout(frame)
}

/// 画弹窗：提示 + 「是」/「否」。
pub fn draw(
    frame: &mut Frame,
    text: &UiText,
    layout: &crate::ui::notice::NoticeLayout,
    language: Language,
    mouse: Point,
) {
    let message = Label::RevivePrompt.text(language);
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
    draw_button(frame, text, layout.accept, Label::Yes.text(language), mouse);
    draw_button(frame, text, layout.reject, Label::No.text(language), mouse);
}

fn draw_button(frame: &mut Frame, text: &UiText, rect: Rect, label: &str, mouse: Point) {
    let fill = if rect.contains(mouse) {
        [92, 82, 42, 255]
    } else {
        [44, 52, 66, 255]
    };
    frame.fill_rect(rect.x, rect.y, rect.width, rect.height, fill);
    frame.fill_rect(rect.x, rect.y, rect.width, 1, [140, 156, 180, 255]);
    text.draw_center(frame, rect, label, SIZE_SMALL, [255, 240, 180, 255]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir_render::animation::ActorAction;

    fn dead_player(started: u64) -> SelfPlayer {
        SelfPlayer {
            object_id: 1,
            real_id: 1,
            name: "tester".to_owned(),
            position: Point::default(),
            direction: mir_core::enums::MirDirection::Down,
            hp: 0,
            mp: 0,
            hp_max: 10,
            mp_max: 10,
            level: 1,
            class: mir_core::enums::MirClass::Warrior,
            gender: mir_core::enums::MirGender::Male,
            hair: 0,
            gold: 0,
            credit: 0,
            dead: true,
            action: ActorAction::Dying,
            action_started_ms: started,
            looks: crate::net::PlayerLooks::default(),
        }
    }

    #[test]
    fn prompt_appears_only_after_death_animation() {
        let player = dead_player(1_000);
        let prompt = DeathPrompt::default();
        assert!(!prompt.visible(&player, 1_000), "动画开始时不弹");
        assert!(!prompt.visible(&player, 1_000 + death_animation_ms() - 1));
        assert!(
            prompt.visible(&player, 1_000 + death_animation_ms()),
            "播完才弹"
        );
    }

    #[test]
    fn dead_action_is_already_finished() {
        let mut player = dead_player(1_000);
        player.action = ActorAction::Dead;
        assert!(DeathPrompt::default().visible(&player, 0));
    }

    #[test]
    fn no_click_sends_nothing_for_twelve_seconds() {
        let player = dead_player(1_000);
        let prompt = DeathPrompt::default();
        // 12s 内没有任何点击 → 状态机不会产生动作（发包由 UiAction 层决定）。
        for second in 1..=12 {
            assert!(prompt.visible(&player, 1_000 + second * 1_000));
        }
    }

    #[test]
    fn reject_dismisses_without_packet_and_stays_dismissed() {
        let player = dead_player(1_000);
        let mut prompt = DeathPrompt::default();
        assert!(prompt.visible(&player, 2_000));
        prompt.dismiss(&player);
        assert!(!prompt.visible(&player, 2_000), "点否后关闭");
        assert!(!prompt.visible(&player, 3_000), "不重复弹");
    }

    #[test]
    fn accept_is_idempotent_for_same_death() {
        let player = dead_player(1_000);
        let mut prompt = DeathPrompt::default();
        prompt.confirm(&player);
        assert!(!prompt.visible(&player, 2_000), "确认后不再弹");
        // 新的死亡动作起点不同 → 重新可弹（服务端复活后再次死亡）。
        let mut second = dead_player(5_000);
        second.action_started_ms = 5_000;
        assert!(prompt.visible(&second, 6_000));
    }

    #[test]
    fn alive_clears_state() {
        let player = dead_player(1_000);
        let mut prompt = DeathPrompt::default();
        prompt.dismiss(&player);
        let mut alive = player;
        alive.dead = false;
        prompt.sync(&alive);
        assert!(!prompt.visible(&alive, 2_000));
    }

    #[test]
    fn s_death_injection_waits_for_animation_end() {
        let mut snapshot = crate::net::WorldSnapshot::default();
        let mut player = dead_player(0);
        player.dead = false;
        player.action = ActorAction::Standing;
        snapshot.self_player = Some(player);
        snapshot.apply_packet_at(
            &mir_protocol::ServerPacket::Death(mir_protocol::packets::server::Death {
                location: Point::new(3, 4),
                direction: mir_core::enums::MirDirection::Down,
            }),
            10_000,
        );
        let player = snapshot.self_player.expect("自己");
        assert!(player.dead);
        let prompt = DeathPrompt::default();
        assert!(!prompt.visible(&player, 10_000), "收到 S.Death 当帧不弹");
        assert!(prompt.visible(&player, 10_000 + death_animation_ms()));
    }

    #[test]
    fn click_yes_sends_once_and_click_no_sends_nothing() {
        let mut snapshot = crate::net::WorldSnapshot::default();
        let mut player = dead_player(1_000);
        player.dead = false;
        player.action = ActorAction::Standing;
        snapshot.self_player = Some(player);
        snapshot.apply_packet_at(
            &mir_protocol::ServerPacket::Death(mir_protocol::packets::server::Death {
                location: Point::new(3, 4),
                direction: mir_core::enums::MirDirection::Down,
            }),
            1_000,
        );
        let text = crate::ui::UiText::load(None);
        let frame = Rect::screen(800, 600);
        let layout = death_layout(frame);
        let mut state = crate::ui::UiState::new(Language::ZhCn);
        state.time_millis = i64::try_from(1_000 + death_animation_ms()).unwrap();

        // 「是」→ 恰好一个 TownRevive；重复点击不再发。
        state.mouse = layout.accept.center();
        let first = state.on_click(&snapshot, &text, frame, None);
        assert_eq!(first, Some(crate::ui::UiAction::TownRevive));
        assert_eq!(
            first.expect("动作").net_command(),
            Some(crate::net::NetCommand::TownRevive)
        );
        let second = state.on_click(&snapshot, &text, frame, None);
        assert_ne!(second, Some(crate::ui::UiAction::TownRevive));

        // 「否」→ 只吃点击，零网络命令。
        let mut other = state;
        other.death_prompt = DeathPrompt::default();
        other.mouse = layout.reject.center();
        let reject = other.on_click(&snapshot, &text, frame, None);
        assert_eq!(reject, Some(crate::ui::UiAction::DismissDeath));
        assert_eq!(reject.expect("动作").net_command(), None);
    }

    #[test]
    fn yes_maps_to_exactly_one_town_revive_packet() {
        assert_eq!(
            crate::ui::UiAction::TownRevive.net_command(),
            Some(crate::net::NetCommand::TownRevive)
        );
        assert_eq!(mir_protocol::packets::client::TownRevive::INDEX, 68);
        // 「否」只吃点击，不产生命令。
        assert_eq!(crate::ui::UiAction::DismissDeath.net_command(), None);
    }

    #[test]
    fn prompt_is_i18n() {
        assert_ne!(
            Label::RevivePrompt.text(Language::ZhCn),
            Label::RevivePrompt.text(Language::En)
        );
        assert_ne!(
            Label::Yes.text(Language::ZhCn),
            Label::Yes.text(Language::En)
        );
        assert_ne!(Label::No.text(Language::ZhCn), Label::No.text(Language::En));
    }
}
