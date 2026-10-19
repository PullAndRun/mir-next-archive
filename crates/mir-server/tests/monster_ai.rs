//! AI 覆盖率巡检的稳定性测试：锁定 `ai_audit` 使用的状态口径，避免以后
//! 改 AI 分支时把真实存在的 AI 悄悄打回「未实现」而没人发现。

use mir_core::enums::Monster as MonsterImage;
use mir_server::MonsterKind;
use mir_server::combat::CombatStats;
use mir_server::monster::AiSupport;

fn kind(ai: u8) -> MonsterKind {
    MonsterKind {
        name: format!("AI-{ai}"),
        image: MonsterImage::Oma,
        ai,
        level: 1,
        view_range: 7,
        cool_eye: 0,
        attack_speed_ms: 1_000,
        move_speed_ms: 1_000,
        experience: 0,
        can_push: false,
        stats: CombatStats::default(),
    }
}

#[test]
fn audit_support_labels_and_key_ais_are_stable() {
    assert_eq!(kind(47).ai_support(), AiSupport::Full);
    assert_eq!(kind(3).ai_support(), AiSupport::Full);
    assert_eq!(kind(56).ai_support(), AiSupport::Full);
    assert_eq!(kind(12).ai_support(), AiSupport::Full);
    assert_eq!(kind(2).ai_support(), AiSupport::Full);

    assert_eq!(
        kind(0).ai_support(),
        AiSupport::Partial,
        "base melee is still partial"
    );
    assert_eq!(kind(53).ai_support(), AiSupport::Stub, "AI53 has no branch");

    assert_eq!(AiSupport::Full.label(), "implemented");
    assert_eq!(AiSupport::Partial.label(), "partial");
    assert_eq!(AiSupport::Stub.label(), "missing");
}
