//! AI 覆盖率巡检的稳定性测试：锁定 `ai_audit` 使用的状态口径，避免以后
//! 改 AI 分支时把真实存在的 AI 悄悄打回「未实现」而没人发现。

use mir_core::enums::Monster as MonsterImage;
use mir_server::MonsterKind;
use mir_server::combat::CombatStats;
use mir_server::monster::{AI_BEHAVIOURS, AiSupport, ai_behaviour};

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
    assert_eq!(kind(15).ai_support(), AiSupport::Full);
    assert_eq!(kind(99).ai_support(), AiSupport::Full);
    assert_eq!(kind(97).ai_support(), AiSupport::Full);
    assert_eq!(kind(102).ai_support(), AiSupport::Full);
    assert_eq!(kind(181).ai_support(), AiSupport::Full);
    assert_eq!(kind(210).ai_support(), AiSupport::Full);
    assert_eq!(kind(164).ai_support(), AiSupport::Full);
    assert_eq!(kind(72).ai_support(), AiSupport::Full);
    assert_eq!(kind(79).ai_support(), AiSupport::Full);
    assert_eq!(kind(116).ai_support(), AiSupport::Full);
    assert_eq!(kind(117).ai_support(), AiSupport::Full);
    assert_eq!(kind(163).ai_support(), AiSupport::Full);
    assert_eq!(kind(169).ai_support(), AiSupport::Full);
    assert_eq!(kind(170).ai_support(), AiSupport::Full);
    assert_eq!(kind(192).ai_support(), AiSupport::Full);
    assert_eq!(kind(4).ai_support(), AiSupport::Full);
    assert_eq!(kind(5).ai_support(), AiSupport::Partial);
    assert_eq!(kind(70).ai_support(), AiSupport::Full);
    assert_eq!(kind(165).ai_support(), AiSupport::Full);
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

#[test]
fn full_ai_table_entries_have_branch_and_named_test() {
    const TEST_SOURCES: &str = concat!(
        include_str!("monster_combat.rs"),
        include_str!("pk.rs"),
        include_str!("skills.rs")
    );
    assert!(
        AI_BEHAVIOURS.len() >= 44,
        "Full AI 表应有 >=44 项，实际 {}",
        AI_BEHAVIOURS.len()
    );
    let mut seen = std::collections::BTreeSet::new();
    for entry in AI_BEHAVIOURS {
        assert!(seen.insert(entry.ai), "重复 AI 号 {}", entry.ai);
        assert!(
            !entry.crystal.is_empty() && !entry.test_name.is_empty(),
            "AI {} 缺 Crystal 出处或测试名",
            entry.ai
        );
        assert!(
            TEST_SOURCES.contains(&format!("fn {}", entry.test_name)),
            "AI {} 的测试 {} 不存在",
            entry.ai,
            entry.test_name
        );
        assert_eq!(
            ai_behaviour(entry.ai),
            Some(entry.behaviour),
            "AI {} 的 dispatch 表不一致",
            entry.ai
        );
        assert_eq!(kind(entry.ai).ai_support(), AiSupport::Full);
    }
    for required in [2, 3, 47, 55, 97, 210] {
        assert!(
            AI_BEHAVIOURS.iter().any(|entry| entry.ai == required),
            "Full AI {required} 必须留在 AI_BEHAVIOURS 表内"
        );
    }
}
