//! Crystal `PlayerObject.GetFrame` 的攻击选招（只决定原生帧表，不改变服务端伤害）。
//!
//! 目标源码：`Crystal/Client/MirObjects/PlayerObject.cs:1035-1080`。
//! 这里把随机源作为可注入的 `roll(100)`，便于用固定序列做确定性测试；生产
//! 调用方传入同一个客户端 RNG。

use mir_core::enums::MirClass;

use crate::player_layout::NativeAction;

/// 服务端 `Attack4` 携带的 `Spell` 信息（当前协议没有字段时用 `Unavailable`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attack4Spell {
    /// 双龙斩。
    TwinDrakeBlade,
    /// 烈火剑法。
    FlamingSword,
    /// 其他/未知。
    Other,
    /// 当前 `S.ObjectAttack` 不携带 Spell，无法判定。
    Unavailable,
}

/// 选择一次 `Attack1` 的原生帧表。
///
/// * `roll` 只在真正需要的分支调用，且严格按 Crystal 的惰性顺序；
/// * `attack_variant` 是 R47 已有的 A1/A3 视觉变体，优先保留；
/// * `double_slash` 在协议未提供技能状态时由调用方显式传 `false`。
///
/// # Panics
/// 不 panic；`roll` 返回值不做取模（调用方必须返回 0..100）。
#[must_use]
pub fn select_attack1<R: FnMut(u32) -> u32>(
    class: MirClass,
    shift: bool,
    has_target: bool,
    attack_variant: Option<crate::animation::AttackVariant>,
    double_slash: bool,
    roll: &mut R,
) -> NativeAction {
    if let Some(variant) = attack_variant {
        return match variant {
            crate::animation::AttackVariant::Attack1 => NativeAction::Attack1,
            crate::animation::AttackVariant::Attack3 => NativeAction::Attack3,
        };
    }
    match class {
        MirClass::Archer => NativeAction::Attack1,
        MirClass::Assassin => {
            if double_slash {
                return NativeAction::Attack1;
            }
            if shift {
                let outer = roll(100);
                if outer >= 20 {
                    // Crystal: Next(100) > 40 ? Attack1 : Attack4
                    if roll(100) > 40 {
                        NativeAction::Attack1
                    } else {
                        NativeAction::Attack4
                    }
                } else {
                    // Crystal: Next(100) > 10 ? Attack2 : Attack3
                    if roll(100) > 10 {
                        NativeAction::Attack2
                    } else {
                        NativeAction::Attack3
                    }
                }
            } else if roll(100) >= 40 {
                NativeAction::Attack1
            } else {
                NativeAction::Attack4
            }
        }
        MirClass::Warrior | MirClass::Wizard | MirClass::Taoist => {
            if shift && !has_target {
                if roll(100) >= 20 {
                    NativeAction::Attack1
                } else {
                    NativeAction::Attack3
                }
            } else {
                NativeAction::Attack1
            }
        }
    }
}

/// `MirAction.Attack4` 的固定映射：双龙斩/烈火剑法→Attack1，其余→Attack4。
#[must_use]
pub const fn select_attack4(spell: Attack4Spell) -> NativeAction {
    match spell {
        Attack4Spell::TwinDrakeBlade | Attack4Spell::FlamingSword => NativeAction::Attack1,
        Attack4Spell::Other | Attack4Spell::Unavailable => NativeAction::Attack4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::AttackVariant;

    fn rolls(values: &[u32]) -> impl FnMut(u32) -> u32 + '_ {
        let mut index = 0;
        move |_| {
            let value = values[index];
            index += 1;
            value
        }
    }

    #[test]
    fn archer_never_rolls() {
        let mut calls = 0;
        let mut roll = |_| {
            calls += 1;
            0
        };
        assert_eq!(
            select_attack1(MirClass::Archer, true, false, None, false, &mut roll),
            NativeAction::Attack1
        );
        assert_eq!(calls, 0);
    }

    #[test]
    fn warrior_shift_without_target_rolls_once() {
        let mut roll = rolls(&[19]);
        assert_eq!(
            select_attack1(MirClass::Warrior, true, false, None, false, &mut roll),
            NativeAction::Attack3
        );
        let mut roll = rolls(&[20]);
        assert_eq!(
            select_attack1(MirClass::Warrior, true, false, None, false, &mut roll),
            NativeAction::Attack1
        );
    }

    #[test]
    fn warrior_with_target_or_no_shift_never_rolls() {
        for (shift, target) in [(true, true), (false, false), (false, true)] {
            let mut calls = 0;
            let mut roll = |_| {
                calls += 1;
                0
            };
            assert_eq!(
                select_attack1(MirClass::Warrior, shift, target, None, false, &mut roll),
                NativeAction::Attack1
            );
            assert_eq!(calls, 0);
        }
    }

    #[test]
    fn assassin_double_slash_has_priority_without_roll() {
        let mut calls = 0;
        let mut roll = |_| {
            calls += 1;
            99
        };
        assert_eq!(
            select_attack1(MirClass::Assassin, true, false, None, true, &mut roll),
            NativeAction::Attack1
        );
        assert_eq!(calls, 0);
    }

    #[test]
    fn assassin_shift_consumes_two_lazy_rolls() {
        assert_eq!(
            select_attack1(
                MirClass::Assassin,
                true,
                false,
                None,
                false,
                &mut rolls(&[20, 41])
            ),
            NativeAction::Attack1
        );
        assert_eq!(
            select_attack1(
                MirClass::Assassin,
                true,
                false,
                None,
                false,
                &mut rolls(&[20, 40])
            ),
            NativeAction::Attack4
        );
        assert_eq!(
            select_attack1(
                MirClass::Assassin,
                true,
                false,
                None,
                false,
                &mut rolls(&[19, 11])
            ),
            NativeAction::Attack2
        );
        assert_eq!(
            select_attack1(
                MirClass::Assassin,
                true,
                false,
                None,
                false,
                &mut rolls(&[19, 10])
            ),
            NativeAction::Attack3
        );
    }

    #[test]
    fn assassin_no_shift_consumes_one_roll() {
        assert_eq!(
            select_attack1(
                MirClass::Assassin,
                false,
                false,
                None,
                false,
                &mut rolls(&[39])
            ),
            NativeAction::Attack4
        );
        assert_eq!(
            select_attack1(
                MirClass::Assassin,
                false,
                false,
                None,
                false,
                &mut rolls(&[40])
            ),
            NativeAction::Attack1
        );
    }

    #[test]
    fn warrior_default_shift_no_target_observes_both_variants_over_40_samples() {
        let mut attack1 = 0;
        let mut attack3 = 0;
        for draw in 0u32..40 {
            let roll_value = draw * 5 % 100;
            let mut one = |_| roll_value;
            let action = select_attack1(MirClass::Warrior, true, false, None, false, &mut one);
            match action {
                NativeAction::Attack1 => attack1 += 1,
                NativeAction::Attack3 => attack3 += 1,
                other => panic!("默认职业空挥不应产生 {other:?}"),
            }
        }
        assert_eq!(attack1 + attack3, 40);
        assert!(attack1 > 0 && attack3 > 0, "40 次固定采样应同时覆盖 A1/A3");
    }

    #[test]
    fn attack4_spell_mapping() {
        assert_eq!(
            select_attack4(Attack4Spell::TwinDrakeBlade),
            NativeAction::Attack1
        );
        assert_eq!(
            select_attack4(Attack4Spell::FlamingSword),
            NativeAction::Attack1
        );
        assert_eq!(select_attack4(Attack4Spell::Other), NativeAction::Attack4);
        assert_eq!(
            select_attack4(Attack4Spell::Unavailable),
            NativeAction::Attack4
        );
    }

    #[test]
    fn existing_visual_variant_wins() {
        let mut calls = 0;
        let mut roll = |_| {
            calls += 1;
            0
        };
        assert_eq!(
            select_attack1(
                MirClass::Assassin,
                true,
                false,
                Some(AttackVariant::Attack3),
                false,
                &mut roll
            ),
            NativeAction::Attack3
        );
        assert_eq!(calls, 0);
    }
}
