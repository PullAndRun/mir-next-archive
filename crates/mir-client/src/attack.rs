//! R47 自机战士普通空挥选招（纯函数，便于 A1 边界测试）。
//!
//! Crystal `PlayerObject.cs:1055-1075` 的 default 分支只在
//! `CMain.Shift && TargetObject == null` 时抽 `Next(100) >= 20 ? Attack1 : Attack3`；
//! 非 Shift/有目标仍 Attack1。本作把「入围」限定为
//! `Warrior + 未骑乘 + Spell.None`，非入围职业/骑乘/技能返回 `None`，继续走旧
//! Attack1/50ms 行为。
//!
//! 选择只发生在动作建立时；返回的变体随后沿 `NetCommand::Attack` →
//! `WorldSnapshot::apply_local_action_with_variant` → `SelfPlayer.attack_variant` →
//! `EntityView.attack_variant` → renderer 流动，render/snapshot 不会重抽。

use mir_core::enums::{MirClass, Spell};
use mir_render::animation::AttackVariant;

/// 一次本地攻击选择需要的上下文。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttackContext {
    /// 职业。
    pub class: MirClass,
    /// 是否按住 Shift。
    pub shift: bool,
    /// 是否有锁定目标（严格 `target_object_id.is_some()`；无效 ID 也算有目标）。
    pub has_target: bool,
    /// 本次攻击的 Spell；普通空挥必须是 [`Spell::None`]。
    pub spell: Spell,
    /// 是否骑乘（`SelfPlayer.looks.mount > 0`）。
    pub riding: bool,
}

/// 把一次 `Random.Next(100)` 的抽签值映射为攻击变体。
///
/// `[0, 19]` → Attack3，`[20, 99]` → Attack1；`>= 100` 是非法值，
/// 返回 `None`（生产 RNG 只产生 0..=99，测试用它锁非法策略）。
#[must_use]
pub const fn variant_for_roll(roll: u8) -> Option<AttackVariant> {
    if roll >= 100 {
        return None;
    }
    Some(if roll < 20 {
        AttackVariant::Attack3
    } else {
        AttackVariant::Attack1
    })
}

/// 是否属于 R47 入围状态（职业/骑乘/技能三条门槛）。
const fn is_eligible(context: AttackContext) -> bool {
    matches!(context.class, MirClass::Warrior)
        && !context.riding
        && matches!(context.spell, Spell::None)
}

/// 选招：只有入围职业/状态才会返回变体；非 Shift/有目标固定 Attack1，
/// Shift+无目标才使用 `roll` 抽 Attack1/Attack3。
#[must_use]
pub const fn select_attack_variant(context: &AttackContext, roll: u8) -> Option<AttackVariant> {
    if !is_eligible(*context) {
        return None;
    }
    if context.has_target || !context.shift {
        return Some(AttackVariant::Attack1);
    }
    variant_for_roll(roll)
}

/// splitmix64 混合函数；输入所有 64 位都参与，避免 epoch 的 u32 截断。
const fn mix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

/// 由 epoch 毫秒与进程 ID 生成生产种子。
///
/// 不能用 `u32::try_from(epoch_ms)`：现代 epoch 毫秒已超过 `u32::MAX`，
/// 截断会在相邻会话里反复得到同一个值（旧实现还进一步 `unwrap_or(1)` 成 1）。
#[must_use]
pub fn seed_from_epoch_and_process(epoch_ms: u64, process_id: u32) -> u64 {
    // 进程 ID 放高位、epoch 全 64 位参与混合；同一进程的相邻毫秒也必不同。
    let hashed = mix64(epoch_ms ^ (u64::from(process_id) << 32) ^ u64::from(process_id));
    if hashed == 1 {
        0x9e37_79b9_7f4a_7c15
    } else {
        hashed
    }
}

/// 生产种子：epoch 毫秒 + 当前进程 ID。
#[must_use]
pub fn seed_from_epoch(epoch_ms: u64) -> u64 {
    seed_from_epoch_and_process(epoch_ms, std::process::id())
}

/// 生产选招 RNG（splitmix64；每动作最多调用一次 `next_roll`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttackRng {
    state: u64,
    draws: u32,
}

impl AttackRng {
    /// 用完整 64 位种子构造；再做一次混合，`0` 也不会退化。
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        let state = mix64(seed) | 1;
        Self {
            state: if state == 1 {
                0x9e37_79b9_7f4a_7c15
            } else {
                state
            },
            draws: 0,
        }
    }

    /// 生产初始化：epoch 毫秒 + 进程 ID，全 64 位混合。
    #[must_use]
    pub fn from_epoch(epoch_ms: u64) -> Self {
        Self::new(seed_from_epoch(epoch_ms))
    }

    /// 下一次 `Random.Next(100)` 等价抽签，范围 `0..=99`。
    pub fn next_roll(&mut self) -> u8 {
        self.draws = self.draws.saturating_add(1);
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^= value >> 31;
        u8::try_from(value % 100).unwrap_or(0)
    }

    /// 已经抽签的次数（测试用；每个动作最多 +1）。
    #[must_use]
    pub const fn draws(self) -> u32 {
        self.draws
    }
}

/// 生产入口：只有 Crystal 的 `Shift && TargetObject == null` 随机分支才调用一次
/// `Next(100)`；非 Shift/有目标/非入围不消耗随机数，避免改变生产随机序列。
pub fn select_attack_variant_with_rng(
    context: &AttackContext,
    rng: &mut AttackRng,
) -> Option<AttackVariant> {
    if !is_eligible(*context) || context.has_target || !context.shift {
        return select_attack_variant(context, 0);
    }
    select_attack_variant(context, rng.next_roll())
}
