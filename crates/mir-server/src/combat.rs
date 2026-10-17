//! 战斗：战斗属性、命中/伤害结算与玩家基础属性。
//!
//! 规则逐条对应 Crystal（参考根目录 `C:\Users\diana\ref\Crystal`，下文只写相对路径）：
//!
//! | 本模块 | Crystal |
//! |---|---|
//! | [`CombatStats::attack_power`] | `MapObject.GetAttackPower`（`Server/MirObjects/MapObject.cs:243-260`） |
//! | [`CombatStats::defence_power`] | `MapObject.GetDefencePower`（`Server/MirObjects/MapObject.cs:274-280`） |
//! | [`CombatStats::get_armour`] | `MapObject.GetArmour`（`Server/MirObjects/MapObject.cs:460-507`） |
//! | [`resolve_player_attack`] | `MonsterObject.Attacked(HumanObject)`（`Server/MirObjects/MonsterObject.cs:2569-2684`） |
//! | [`resolve_monster_attack`] | `HumanObject.Attacked(MonsterObject)`（`Server/MirObjects/HumanObject.cs:7222-7315`） |
//! | [`player_base_stats`] | `Shared/BaseStats.cs` + `HumanObject.RefreshStats`（`Server/MirObjects/HumanObject.cs:1730-1769`） |
//! | [`player_attack_delay_ms`] | `HumanObject.RefreshStats` 里的 `AttackSpeed`（`Server/MirObjects/HumanObject.cs:1766-1768`） |
//!
//! # 随机数顺序
//!
//! Crystal 只有一个全局 `Envir.Random`（`Server/MirEnvir/Envir.cs:100`），mir-next 用
//! [`crate::world::World`] 里同样的单实例 [`MirRng`]，因此**每一次抽签的顺序都必须一致**，
//! 否则同一 seed 的回放结果会不同。一次物理攻击的抽签顺序固定为：
//!
//! 1. 攻方 `GetAttackPower`（幸运 > 0 或 < 0 时两次抽签，否则一次）；
//! 2. 守方 `GetArmour`：先掷闪避（`Random.Next(Agility + 1) > Accuracy`），再掷 `GetDefencePower`；
//! 3. 判定「护甲 >= 伤害」之后的暴击抽签（只有打怪物时才有，见 [`resolve_player_attack`]）。
//!
//! # 简化
//!
//! `// 简化：` 开头的注释标出了本模块与 Crystal 的差异，每条都写了原因。

use mir_core::MirRng;
use mir_core::enums::{DefenceType, MirClass};

/// 幸运上限：Crystal `Settings.MaxLuck`（`Server/Settings.cs:304`）。
pub const MAX_LUCK: i32 = 10;
/// 魔法闪避权重：Crystal `Settings.MagicResistWeight`（`Server/Settings.cs:296`）。
pub const MAGIC_RESIST_WEIGHT: u32 = 10;
/// 暴击率权重：Crystal `Settings.CriticalRateWeight`（`Server/Settings.cs:298`）。
pub const CRITICAL_RATE_WEIGHT: i32 = 5;
/// 暴击伤害权重：Crystal `Settings.CriticalDamageWeight`（`Server/Settings.cs:299`）。
pub const CRITICAL_DAMAGE_WEIGHT: i32 = 50;

/// 一条等待结算的攻击（Crystal `DelayedAction(DelayedType.Damage, ...)`，`Server/MirObjects/DelayedAction.cs`）。
///
/// Crystal 把伤害塞进攻击者自己的 `ActionList`，到点后由 `CompleteAttack` 调 `target.Attacked(...)`
/// （`Server/MirObjects/HumanObject.cs:6775-6792`、`MonsterObject.cs:1376-1385`）。
/// mir-next 用世界级的队列复现：顺序按入队顺序，与 Crystal 的「每个对象各自的队列」在单目标场景等价。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingHit {
    /// 结算时刻（毫秒）。`now >= at` 时落地。
    pub at: i64,
    /// 攻击者对象 ID（玩家或怪物）。
    pub attacker_id: u32,
    /// 目标对象 ID（玩家或怪物）。
    pub target_id: u32,
    /// 已经抽好的原始伤害（`GetAttackPower` 的结果）。
    pub damage: i32,
    /// 结算时使用的防御类型。
    pub defence: DefenceType,
}

/// 一次攻击的结算结果，对应 Crystal `Attacked` 的返回值 + `DamageIndicator` 的语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HitResult {
    /// 实际扣血值（Crystal `damage - armour`），`miss == true` 时为 0。
    pub damage: i32,
    /// 抽到的护甲值（Crystal `armour`）。
    pub armour: i32,
    /// 是否暴击（只有「攻击方是玩家、守方是怪物」的路径会暴击）。
    pub critical: bool,
    /// Crystal 是否广播了 `DamageIndicator{DamageType.Miss}`。
    pub miss: bool,
}

impl HitResult {
    /// 未命中：Crystal 会广播一条 `Miss` 指示。
    #[must_use]
    pub const fn missed() -> Self {
        Self {
            damage: 0,
            armour: 0,
            critical: false,
            miss: true,
        }
    }
}

/// 怪物/玩家的战斗属性。
///
/// 对应 Crystal `MapObject.Stats` 里参与物理战斗的那部分（`Shared/Stats.cs`）。
/// `hp`/`mp` 是**上限**（`Stats[Stat.HP]`/`Stats[Stat.MP]`），当前血量在
/// [`crate::world::Player::hp`] / [`crate::monster::Monster::hp`] 里。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CombatStats {
    /// `Stat.HP`：生命上限。
    pub hp: i32,
    /// `Stat.MP`：魔法上限。
    pub mp: i32,
    /// `Stat.MinDC`：最小物理攻击力。
    pub min_dc: i32,
    /// `Stat.MaxDC`：最大物理攻击力。
    pub max_dc: i32,
    /// `Stat.MinMC`：最小魔法攻击力。
    pub min_mc: i32,
    /// `Stat.MaxMC`：最大魔法攻击力。
    pub max_mc: i32,
    /// `Stat.MinSC`：最小道术攻击力。
    pub min_sc: i32,
    /// `Stat.MaxSC`：最大道术攻击力。
    pub max_sc: i32,
    /// `Stat.MinAC`：最小物理防御。
    pub min_ac: i32,
    /// `Stat.MaxAC`：最大物理防御。
    pub max_ac: i32,
    /// `Stat.MinMAC`：最小魔法防御。
    pub min_mac: i32,
    /// `Stat.MaxMAC`：最大魔法防御。
    pub max_mac: i32,
    /// `Stat.Accuracy`：准确（被攻击方的 `Agility` 与之对抗）。
    pub accuracy: i32,
    /// `Stat.Agility`：敏捷（决定闪避概率）。
    pub agility: i32,
    /// `Stat.Luck`：幸运（>0 更容易打出上限攻击力，<0 更容易打出下限）。
    pub luck: i32,
    /// `Stat.AttackSpeed`：攻速属性（**不是**毫秒），见 [`player_attack_delay_ms`]。
    pub attack_speed: i32,
    /// `Stat.CriticalRate`：暴击率（乘 `Settings.CriticalRateWeight` 后与 `Random.Next(100)` 比较）。
    pub critical_rate: i32,
    /// `Stat.CriticalDamage`：暴击伤害（除以 `Settings.CriticalDamageWeight` 再 ×10）。
    pub critical_damage: i32,
    /// `Stat.MagicResist`：魔法闪避（只在 `MAC`/`MACAgility` 防御类型里用）。
    pub magic_resist: i32,
    /// `Stat.Reflect`：伤害反射（本作没有装备，恒为 0，但抽签照做以保持随机序列一致）。
    pub reflect: i32,
}

impl CombatStats {
    /// Crystal `MapObject.GetAttackPower`（`Server/MirObjects/MapObject.cs:243-260`）。
    ///
    /// 幸运 > 0：有 `luck / MaxLuck` 的概率直接取上限；幸运 < 0：对称地取下限；
    /// 否则在 `[min, max]` 上均匀取值。注意幸运分支**先抽一次**再决定是否返回。
    pub fn attack_power(&self, min: i32, max: i32, rng: &mut MirRng) -> i32 {
        let min = min.max(0);
        let mut max = max;
        if min > max {
            max = min;
        }

        if self.luck > 0 {
            if self.luck > rng.next_range(0, MAX_LUCK - 1) {
                return max;
            }
        } else if self.luck < 0 && self.luck < -rng.next_range(0, MAX_LUCK - 1) {
            return min;
        }

        // 简化：Crystal 的 `Random.Next(min, max + 1)` 在 min == max 时也不抽签（.NET 实现会对
        // 相等区间直接返回），mir-next 的 `next_range` 同样不抽签，两边行为一致，故无需特判。
        rng.next_range(min, max)
    }

    /// `GetAttackPower(MinDC, MaxDC)`：一次物理攻击的原始伤害。
    pub fn physical_attack(&self, rng: &mut MirRng) -> i32 {
        self.attack_power(self.min_dc, self.max_dc, rng)
    }

    /// Crystal `MapObject.GetDefencePower`（`Server/MirObjects/MapObject.cs:274-280`）。
    pub fn defence_power(&self, min: i32, max: i32, rng: &mut MirRng) -> i32 {
        let min = min.max(0);
        let mut max = max;
        if min > max {
            max = min;
        }
        rng.next_range(min, max)
    }

    /// Crystal `MapObject.GetArmour`（`Server/MirObjects/MapObject.cs:460-507`）。
    ///
    /// 返回 `(护甲值, 是否命中)`。**未命中时护甲值仍然会被抽出来**（Crystal 也一样），
    /// 所以随机数消耗与命中与否无关。
    pub fn get_armour(
        &self,
        attacker: &Self,
        defence: DefenceType,
        rng: &mut MirRng,
    ) -> (i32, bool) {
        let mut hit = true;
        let armour = match defence {
            DefenceType::ACAgility => {
                if rng.next_range(0, self.agility) > attacker.accuracy {
                    hit = false;
                }
                self.defence_power(self.min_ac, self.max_ac, rng)
            }
            DefenceType::AC => self.defence_power(self.min_ac, self.max_ac, rng),
            DefenceType::MACAgility => {
                if rng.next_bounded(MAGIC_RESIST_WEIGHT) < self.magic_resist as u32 {
                    hit = false;
                }
                if rng.next_range(0, self.agility) > attacker.accuracy {
                    hit = false;
                }
                self.defence_power(self.min_mac, self.max_mac, rng)
            }
            DefenceType::MAC => {
                if rng.next_bounded(MAGIC_RESIST_WEIGHT) < self.magic_resist as u32 {
                    hit = false;
                }
                self.defence_power(self.min_mac, self.max_mac, rng)
            }
            DefenceType::Agility => {
                if rng.next_range(0, self.agility) > attacker.accuracy {
                    hit = false;
                }
                0
            }
            // Crystal 的 switch 没有 default：Repulsion/None 的护甲是 0、必中。
            DefenceType::Repulsion | DefenceType::None => 0,
        };
        (armour, hit)
    }
}

/// 玩家打怪物的伤害结算：Crystal `MonsterObject.Attacked(HumanObject)`
/// （`Server/MirObjects/MonsterObject.cs:2569-2684`）。
///
/// 顺序：`GetArmour` → 未命中/护甲够高则 Miss → **暴击抽签** → 扣血。
#[must_use]
pub fn resolve_player_attack(
    attacker: &CombatStats,
    defender: &CombatStats,
    damage: i32,
    defence: DefenceType,
    rng: &mut MirRng,
) -> HitResult {
    let (armour, hit) = defender.get_armour(attacker, defence, rng);
    if !hit || armour >= damage {
        return HitResult::missed();
    }

    let mut damage = damage;
    let mut critical = false;
    // `MonsterObject.cs:2594-2599`：`Random.Next(100) < CriticalRate * CriticalRateWeight`
    if (rng.next_bounded(100) as i32) < attacker.critical_rate * CRITICAL_RATE_WEIGHT {
        critical = true;
        // `damage += (int)Math.Floor(damage * ((CriticalDamage / CriticalDamageWeight) * 10))`
        let bonus = i64::from(damage) * i64::from(attacker.critical_damage) * 10
            / i64::from(CRITICAL_DAMAGE_WEIGHT);
        damage = i32::try_from(i64::from(damage) + bonus).unwrap_or(i32::MAX);
    }

    HitResult {
        damage: damage - armour,
        armour,
        critical,
        miss: false,
    }
}

/// 怪物打玩家的伤害结算：Crystal `HumanObject.Attacked(MonsterObject)`
/// （`Server/MirObjects/HumanObject.cs:7222-7315`）。
///
/// 与 [`resolve_player_attack`] 的两点不同（都是 Crystal 的原样行为）：
///
/// 1. 进门先掷**反射**（`HumanObject.cs:7231`），本作 `reflect` 恒为 0，但抽签照做；
/// 2. **没有暴击判定**——Crystal 只在「与玩家交锋」的 `Attacked(HumanObject)` 里判暴击
///    （`HumanObject.cs:7156`），怪物打玩家永远不暴击。
#[must_use]
pub fn resolve_monster_attack(
    attacker: &CombatStats,
    defender: &CombatStats,
    damage: i32,
    rng: &mut MirRng,
) -> HitResult {
    if (rng.next_bounded(100) as i32) < defender.reflect {
        // 反射：Crystal 会再对攻击方结算一次伤害并广播 `SpellEffect.Reflect`。
        // 简化：没有装备/魔法，`reflect` 恒为 0，这里只保留抽签位置。
        return HitResult::missed();
    }

    let (armour, hit) = defender.get_armour(attacker, DefenceType::ACAgility, rng);
    if !hit || armour >= damage {
        return HitResult::missed();
    }

    HitResult {
        damage: damage - armour,
        armour,
        critical: false,
        miss: false,
    }
}

/// 玩家攻速（毫秒）：Crystal `HumanObject.RefreshStats`（`Server/MirObjects/HumanObject.cs:1766-1768`）。
///
/// `AttackSpeed = 1400 - (AttackSpeedStat * 60 + min(370, Level * 14))`，下限 550。
#[must_use]
pub fn player_attack_delay_ms(level: u16, attack_speed: i32) -> i64 {
    let level_bonus = (i32::from(level) * 14).min(370);
    let delay = 1400 - (attack_speed * 60 + level_bonus);
    i64::from(delay.max(550))
}

/// 等级/职业推出玩家基础战斗属性。
///
/// 逐条移植 `Shared/BaseStats.cs:12-89` 的 `BaseStat` 表与 `BaseStat.Calculate`
/// （`Shared/BaseStats.cs:155-184`），以及 `HumanObject.RefreshStatCaps`
/// （`Server/MirObjects/HumanObject.cs:2229-2253`）里的三类上限与 `MinDC <= MaxDC` 夹取。
///
/// 简化：只算**基础值**（等级曲线），不含装备、技能、buff、行会加成——这些系统还没实现；
/// 等 `mir-db` 接入装备后由刷新流程重新算一遍即可。
#[must_use]
pub fn player_base_stats(level: u16, class: MirClass) -> CombatStats {
    let level_f = f64::from(level);
    let mut stats = CombatStats {
        // 各职业的 Agility / Accuracy 基础值（`Shared/BaseStats.cs:25-26,40-41,57-58,70-71,85-86`）。
        agility: match class {
            MirClass::Taoist => 18,
            MirClass::Assassin => 20,
            _ => 15,
        },
        accuracy: match class {
            MirClass::Archer => 8,
            _ => 5,
        },
        ..CombatStats::default()
    };

    match class {
        MirClass::Warrior => {
            stats.hp = health(level_f, 14, 4.0, 4.5, true);
            stats.mp = mana_plain(level_f, 11, 3.5, 0.0);
            stats.max_ac = stat_curve(level_f, 7.0);
            stats.min_dc = stat_curve(level_f, 5.0);
            stats.max_dc = stat_curve(level_f, 5.0);
        }
        MirClass::Wizard => {
            stats.hp = health(level_f, 14, 15.0, 1.8, false);
            stats.mp = mana_wizard(level_f, 13, 5.0, 0.0);
            stats.min_dc = stat_curve(level_f, 7.0);
            stats.max_dc = stat_curve(level_f, 7.0);
            stats.min_mc = stat_curve(level_f, 7.0);
            stats.max_mc = stat_curve(level_f, 7.0);
        }
        MirClass::Taoist => {
            stats.hp = health(level_f, 14, 6.0, 2.5, false);
            stats.mp = mana_taoist(level_f, 13, 8.0, 0.0);
            stats.min_mac = stat_curve(level_f, 12.0);
            stats.max_mac = stat_curve(level_f, 6.0);
            stats.min_dc = stat_curve(level_f, 7.0);
            stats.max_dc = stat_curve(level_f, 7.0);
            stats.min_sc = stat_curve(level_f, 7.0);
            stats.max_sc = stat_curve(level_f, 7.0);
        }
        MirClass::Assassin => {
            stats.hp = health(level_f, 14, 4.0, 3.25, false);
            stats.mp = mana_plain(level_f, 11, 5.0, 0.0);
            stats.min_dc = stat_curve(level_f, 8.0);
            stats.max_dc = stat_curve(level_f, 8.0);
        }
        MirClass::Archer => {
            stats.hp = health(level_f, 14, 4.0, 3.25, false);
            stats.mp = mana_plain(level_f, 11, 4.0, 0.0);
            stats.min_dc = stat_curve(level_f, 8.0);
            stats.max_dc = stat_curve(level_f, 8.0);
            stats.min_mc = stat_curve(level_f, 8.0);
            stats.max_mc = stat_curve(level_f, 8.0);
        }
    }

    // `RefreshStatCaps`：这些属性有职业上限（`Shared/BaseStats.cs:91-99`），基础值恒为 0。
    stats.critical_rate = stats.critical_rate.min(18);
    stats.critical_damage = stats.critical_damage.min(10);
    stats.magic_resist = stats.magic_resist.min(2);
    // `HumanObject.cs:2250`：`MinDC = Math.Min(MinDC, MaxDC)`（MC/SC 同理）。
    stats.min_dc = stats.min_dc.min(stats.max_dc);
    stats.min_mc = stats.min_mc.min(stats.max_mc);
    stats.min_sc = stats.min_sc.min(stats.max_sc);
    stats
}

/// `StatFormula.Health`（`Shared/BaseStats.cs:159-166`）。
fn health(level: f64, base: i32, gain: f64, gain_rate: f64, warrior: bool) -> i32 {
    let value = if warrior {
        level / gain + gain_rate + level / 20.0
    } else {
        level / gain + gain_rate
    };
    (f64::from(base) + value * level) as i32
}

/// 非法师/道士的 `StatFormula.Mana`（`Shared/BaseStats.cs:173`）。
fn mana_plain(level: f64, base: i32, gain: f64, gain_rate: f64) -> i32 {
    (f64::from(base) + level * gain + level * gain_rate) as i32
}

/// 法师的 `StatFormula.Mana`（`Shared/BaseStats.cs:171`）。
fn mana_wizard(level: f64, base: i32, gain: f64, gain_rate: f64) -> i32 {
    (f64::from(base) + (level / gain + 2.0) * 2.2 * level + level * gain_rate) as i32
}

/// 道士的 `StatFormula.Mana`（`Shared/BaseStats.cs:172`）。
fn mana_taoist(level: f64, base: i32, gain: f64, gain_rate: f64) -> i32 {
    (f64::from(base) + level / gain * 2.2 * level + level * gain_rate) as i32
}

/// `StatFormula.Stat`：`Base + level / Gain`（`Shared/BaseStats.cs:181`，整数除法由 `as i32` 截断复现）。
fn stat_curve(level: f64, gain: f64) -> i32 {
    (level / gain) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats() -> CombatStats {
        CombatStats::default()
    }

    #[test]
    fn attack_power_stays_inside_bounds() {
        let mut rng = MirRng::from_seed(11);
        let attacker = stats();
        for _ in 0..256 {
            let damage = attacker.attack_power(3, 9, &mut rng);
            assert!((3..=9).contains(&damage), "伤害 {damage} 越界");
        }
    }

    #[test]
    fn positive_luck_pins_max_and_negative_luck_pins_min() {
        let mut rng = MirRng::from_seed(7);
        // 幸运 10：`10 > Random.Next(10)` 恒成立（Next(10) ∈ [0, 9]）。
        let lucky = CombatStats {
            luck: MAX_LUCK,
            ..CombatStats::default()
        };
        // 幸运 -10：`-10 < -Random.Next(10)` 也恒成立（-Next ∈ [-9, 0]）。
        let cursed = CombatStats {
            luck: -MAX_LUCK,
            ..CombatStats::default()
        };
        for _ in 0..64 {
            assert_eq!(lucky.attack_power(2, 5, &mut rng), 5);
            assert_eq!(cursed.attack_power(2, 5, &mut rng), 2);
        }
    }

    #[test]
    fn attack_power_clamps_negative_and_inverted_ranges() {
        let mut rng = MirRng::from_seed(3);
        let attacker = stats();
        assert_eq!(attacker.attack_power(-5, -1, &mut rng), 0, "min 先夹到 0");
        assert_eq!(
            attacker.attack_power(7, 2, &mut rng),
            7,
            "min > max 时取 min"
        );
    }

    #[test]
    fn high_agility_dodges_and_zero_agility_never_dodges() {
        let mut rng = MirRng::from_seed(5);
        let weak_attacker = CombatStats {
            accuracy: 0,
            ..CombatStats::default()
        };
        let nimble = CombatStats {
            agility: 100,
            ..CombatStats::default()
        };
        let mut dodged = 0;
        for _ in 0..64 {
            let (_, hit) = nimble.get_armour(&weak_attacker, DefenceType::ACAgility, &mut rng);
            if !hit {
                dodged += 1;
            }
        }
        assert!(dodged > 0, "敏捷 100 vs 准确 0 必须出现闪避");

        let clumsy = CombatStats::default();
        let accurate = CombatStats {
            accuracy: 5,
            ..CombatStats::default()
        };
        for _ in 0..64 {
            let (_, hit) = clumsy.get_armour(&accurate, DefenceType::ACAgility, &mut rng);
            assert!(hit, "敏捷 0 时 Next(1) == 0，永远不会大于准确 5");
        }
    }

    #[test]
    fn monster_hit_damage_is_damage_minus_armour() {
        let mut rng = MirRng::from_seed(13);
        let attacker = CombatStats {
            min_dc: 10,
            max_dc: 10,
            accuracy: 50,
            ..CombatStats::default()
        };
        let defender = CombatStats {
            min_ac: 2,
            max_ac: 2,
            ..CombatStats::default()
        };
        let result = resolve_monster_attack(&attacker, &defender, 10, &mut rng);
        assert!(!result.miss);
        assert_eq!(result.armour, 2);
        assert_eq!(result.damage, 8);
        assert!(!result.critical, "怪物打玩家不判暴击（Crystal 原样行为）");
    }

    #[test]
    fn armour_at_or_above_damage_counts_as_miss() {
        let mut rng = MirRng::from_seed(17);
        let attacker = CombatStats {
            min_dc: 5,
            max_dc: 5,
            accuracy: 50,
            ..CombatStats::default()
        };
        let defender = CombatStats {
            min_ac: 5,
            max_ac: 5,
            ..CombatStats::default()
        };
        let result =
            resolve_player_attack(&attacker, &defender, 5, DefenceType::ACAgility, &mut rng);
        assert!(result.miss);
        assert_eq!(result.damage, 0);
    }

    #[test]
    fn critical_hit_adds_floor_of_ratio_times_ten() {
        // 暴击率 20 * 权重 5 = 100，`Random.Next(100) < 100` 恒成立。
        let mut rng = MirRng::from_seed(23);
        let attacker = CombatStats {
            accuracy: 99,
            critical_rate: 20,
            critical_damage: 5,
            ..CombatStats::default()
        };
        let defender = CombatStats::default();
        let result =
            resolve_player_attack(&attacker, &defender, 100, DefenceType::ACAgility, &mut rng);
        assert!(result.critical);
        // 100 + floor(100 * (5 / 50) * 10) = 100 + 100 = 200。
        assert_eq!(result.damage, 200);
    }

    #[test]
    fn zero_critical_rate_never_crits() {
        let mut rng = MirRng::from_seed(29);
        let attacker = CombatStats {
            accuracy: 99,
            ..CombatStats::default()
        };
        for _ in 0..64 {
            let result = resolve_player_attack(
                &attacker,
                &CombatStats::default(),
                10,
                DefenceType::ACAgility,
                &mut rng,
            );
            assert!(!result.critical);
        }
    }

    #[test]
    fn player_attack_delay_matches_crystal_formula() {
        // 1 级战士：1400 - (0 + min(370, 14)) = 1386。
        assert_eq!(player_attack_delay_ms(1, 0), 1386);
        // 50 级：min(370, 700) = 370 → 1030。
        assert_eq!(player_attack_delay_ms(50, 0), 1030);
        // 攻速属性 5 → 1400 - (300 + 14) = 1086。
        assert_eq!(player_attack_delay_ms(1, 5), 1086);
        // 下限 550。
        assert_eq!(player_attack_delay_ms(99, 20), 550);
    }

    #[test]
    fn base_stats_match_crystal_level_one() {
        let warrior = player_base_stats(1, MirClass::Warrior);
        // 14 + (1/4 + 4.5 + 1/20) * 1 = 18.8 → 18；11 + 1 * 3.5 = 14.5 → 14。
        assert_eq!((warrior.hp, warrior.mp), (18, 14));
        assert_eq!(warrior.accuracy, 5);
        assert_eq!(warrior.agility, 15);
        assert_eq!((warrior.min_dc, warrior.max_dc), (0, 0));

        let wizard = player_base_stats(1, MirClass::Wizard);
        // HP：14 + (1/15 + 1.8) = 15.87 → 15；MP：13 + (1/5 + 2) * 2.2 = 17.84 → 17。
        assert_eq!((wizard.hp, wizard.mp), (15, 17));
        assert_eq!(wizard.agility, 15);

        let taoist = player_base_stats(1, MirClass::Taoist);
        // HP：14 + (1/6 + 2.5) = 16.67 → 16；MP：13 + 1/8 * 2.2 = 13.27 → 13。
        assert_eq!((taoist.hp, taoist.mp), (16, 13));
        assert_eq!(taoist.agility, 18);
        assert_eq!(taoist.max_mac, 0);

        let assassin = player_base_stats(1, MirClass::Assassin);
        assert_eq!((assassin.hp, assassin.mp), (17, 16));
        assert_eq!(assassin.agility, 20);

        let archer = player_base_stats(1, MirClass::Archer);
        assert_eq!((archer.hp, archer.mp), (17, 15));
        assert_eq!(archer.accuracy, 8);
    }

    #[test]
    fn base_stats_grow_with_level_and_keep_min_dc_below_max() {
        let low = player_base_stats(10, MirClass::Warrior);
        let high = player_base_stats(40, MirClass::Warrior);
        assert!(high.hp > low.hp);
        assert!(high.max_dc >= low.max_dc);
        assert!(high.min_dc <= high.max_dc);
    }
}
