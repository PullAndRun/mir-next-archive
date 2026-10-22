//! 战斗：战斗属性、命中/伤害结算与玩家基础属性。
//!
//! 规则逐条对应 Crystal（参考根目录 `github\Crystal`，下文只写相对路径）：
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
/// 魔法闪避的职业上限：Crystal `Caps[Stat.MagicResist] = 2`（`Shared/BaseStats.cs:91`）。
pub const MAGIC_RESIST_CAP: i32 = 2;
/// 暴击率的职业上限：Crystal `Caps[Stat.CriticalRate] = 18`（`Shared/BaseStats.cs:93`）。
pub const CRITICAL_RATE_CAP: i32 = 18;
/// 暴击伤害的职业上限：Crystal `Caps[Stat.CriticalDamage] = 10`（`Shared/BaseStats.cs:94`）。
pub const CRITICAL_DAMAGE_CAP: i32 = 10;

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
    /// `Stat.Reflect`：伤害反射（本作没有反射类装备，恒为 0，但抽签照做以保持随机序列一致）。
    pub reflect: i32,
    /// `Stat.BagWeight`：背包负重上限。
    ///
    /// 基础值来自 `Shared/BaseStats.cs:18`（战士 `Base = 50, Gain = 3F`，公式见
    /// `weight_cap`）；装备可以用 `Stat.BagWeight` 叠加（`HumanObject.cs:1858` 的 `Stats.Add`）。
    pub bag_weight: i32,
    /// `Stat.WearWeight`：除武器/火把之外的装备重量上限（`Shared/BaseStats.cs:19`）。
    pub wear_weight: i32,
    /// `Stat.HandWeight`：武器/火把的重量上限（`Shared/BaseStats.cs:20`）。
    pub hand_weight: i32,
    /// `Stat.DamageReductionPercent`：伤害减免百分比（`Shared/Data/Stat.cs:144`）。
    ///
    /// 唯一的来源是魔法盾（`BuffType.MagicShield`，`(Level + 2) * 10`，
    /// `HumanObject.cs:6203`）；受击时先按它削减伤害再减护甲
    /// （`HumanObject.cs:7126-7130`、`:7244-7248`）。
    pub damage_reduction_percent: i32,
    /// `Stat.EnergyShieldPercent`：气功波每次受击的回血触发概率（`Stat.cs:145`）。
    ///
    /// 由 `Spell.EnergyShield` 写入：`(int)Math.Round((1 / (decimal)chance) * 100)`
    /// （`HumanObject.cs:4766`），受击时 `Random.Next(100) < 该值` 就回血（`:7262-7271`）。
    pub energy_shield_percent: i32,
    /// `Stat.EnergyShieldHPGain`：气功波触发时回复的生命（`Stat.cs:146`、`HumanObject.cs:4767`）。
    pub energy_shield_hp_gain: i32,
    /// `Stat.ManaPenaltyPercent`：技能耗蓝惩罚百分比（`Stat.cs:147`）。
    ///
    /// 来源是魔力增幅（`BuffType.MagicBooster`，`6 + Level`，`HumanObject.cs:6234`）；
    /// 耗蓝公式见 [`crate::magic::UserMagic::cost_with_penalty`]（`HumanObject.cs:3387-3390`）。
    pub mana_penalty_percent: i32,
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

    /// Crystal `HumanObject.RefreshStatCaps`（`Server/MirObjects/HumanObject.cs:2229-2253`）。
    ///
    /// 装备叠加完之后必须再过一遍：先按职业上限截断（`Settings.ClassBaseStats[..].Caps`，
    /// `Shared/BaseStats.cs:91-99`），再把 HP/MP/攻击/防御夹到非负，最后 `MinDC <= MaxDC`
    /// （MC/SC 同理）。
    ///
    /// 简化：只截断本结构建模的属性。Crystal 还会截断毒抗/冰冻/中毒攻击/回血回蓝
    /// （`BaseStats.cs:92-99`），那几项本作尚未接入战斗结算，留着也观察不到。
    pub fn apply_caps(&mut self) {
        self.magic_resist = self.magic_resist.min(MAGIC_RESIST_CAP);
        self.critical_rate = self.critical_rate.min(CRITICAL_RATE_CAP);
        self.critical_damage = self.critical_damage.min(CRITICAL_DAMAGE_CAP);

        self.hp = self.hp.max(0);
        self.mp = self.mp.max(0);
        self.min_ac = self.min_ac.max(0);
        self.max_ac = self.max_ac.max(0);
        self.min_mac = self.min_mac.max(0);
        self.max_mac = self.max_mac.max(0);
        self.min_dc = self.min_dc.max(0);
        self.max_dc = self.max_dc.max(0);
        self.min_mc = self.min_mc.max(0);
        self.max_mc = self.max_mc.max(0);
        self.min_sc = self.min_sc.max(0);
        self.max_sc = self.max_sc.max(0);

        self.min_dc = self.min_dc.min(self.max_dc);
        self.min_mc = self.min_mc.min(self.max_mc);
        self.min_sc = self.min_sc.min(self.max_sc);
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
/// 与 [`resolve_player_attack`] 的三点不同（都是 Crystal 的原样行为）：
///
/// 1. **没有暴击判定**——Crystal 只在「与玩家交锋」的 `Attacked(HumanObject)` 里判暴击
///    （`HumanObject.cs:7156`），怪物打玩家永远不暴击；
/// 2. 护甲之前先过一次**魔法盾减伤**：`damage -= (damage * DamageReductionPercent) / 100`
///    （`HumanObject.cs:7244-7248`）；
/// 3. 抽签顺序是 `GetArmour`（`:7224`）→ **反射**（`:7231`）→ 护甲比较（`:7250`）。
///    反射在伤害减免之前，但**在命中判定之后**。
///
/// 气功波（`Stat.EnergyShieldPercent`）要改血量、还要抽一次 `Random.Next(100)`，
/// 所以不在这里做，交给 `World::damage_player`（顺序保持一致：在「未命中」判定之后）。
///
/// `defence` 由攻击方指定：野生怪用 `ACAgility`（`MonsterObject.cs:2169` 附近），
/// 守卫用 `AC`（`Server/MirObjects/Monsters/Guard.cs:85`）。
#[must_use]
pub fn resolve_monster_attack(
    attacker: &CombatStats,
    defender: &CombatStats,
    damage: i32,
    defence: DefenceType,
    rng: &mut MirRng,
) -> HitResult {
    let (armour, hit) = defender.get_armour(attacker, defence, rng);
    if !hit {
        return HitResult::missed();
    }

    if (rng.next_bounded(100) as i32) < defender.reflect {
        // 反射：Crystal 会再对攻击方结算一次伤害并广播 `SpellEffect.Reflect`。
        // 简化：没有装备/魔法，`reflect` 恒为 0，这里只保留抽签位置。
        return HitResult::missed();
    }

    let mut damage = damage;
    // `if (Stats[Stat.DamageReductionPercent] != 0) damage -= (damage * …) / 100;`
    // （`HumanObject.cs:7245-7248`，整数除法向零截断，与 C# 一致）。
    if defender.damage_reduction_percent != 0 {
        damage -= damage * defender.damage_reduction_percent / 100;
    }

    if armour >= damage {
        return HitResult::missed();
    }

    HitResult {
        damage: damage - armour,
        armour,
        critical: false,
        miss: false,
    }
}

/// 玩家打玩家的结算结果（Crystal `HumanObject.Attacked(HumanObject)` 的返回值语义）。
///
/// 与 [`HitResult`] 的区别只有 `energy_shield`：气功波的回血要落到世界里的真实血量，
/// 但它的**抽签位置**必须在暴击之前（`HumanObject.cs:7145-7161`），所以由本函数判定，
/// 再由 `World::damage_player_from_player` 执行回血。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PvpHit {
    /// 伤害结算结果（`damage` 已是 `damage - armour`）。
    pub result: HitResult,
    /// 气功波是否触发（`Random.Next(100) < Stats[Stat.EnergyShieldPercent]`）。
    pub energy_shield: bool,
}

/// 玩家打玩家的伤害结算：Crystal `HumanObject.Attacked(HumanObject)`
/// （`Server/MirObjects/HumanObject.cs:7092-7221`）。
///
/// 抽签顺序（与 Crystal 逐行一致）：
///
/// 1. `GetArmour`：先掷闪避（`Random.Next(Agility + 1) > attacker.Accuracy`），再掷护甲
///    （`:7101`，实现见 [`CombatStats::get_armour`]）；
/// 2. 未命中直接返回（`:7103-7106`）；
/// 3. **反射**：`Random.Next(100) < defender.Reflect`（`:7116`）——无论 `reflect` 是否为 0
///    都会抽签，保持随机序列；
/// 4. 魔法盾减伤：`damage -= damage * DamageReductionPercent / 100`（`:7127-7130`）；
/// 5. 护甲够高 → Miss（`:7132-7136`）；
/// 6. 气功波：`Random.Next(100) < EnergyShieldPercent`（`:7145-7154`，只有 `> 0` 才抽）；
/// 7. **暴击**：`Random.Next(100) < CriticalRate * CriticalRateWeight`（`:7156-7161`）。
///
/// 简化（都在 `docs/world/pk.md` 的异常清单里）：
/// * `ArmourRate` / `DamageRate`（`:7108-7109`）恒为 1，不建模；
/// * `Stats[Stat.AttackBonus]`（`:7114`）本作没有这个装备属性；
/// * `DamageWeapon` / `HPDrainRatePercent` / 毒与麻痹（`:7112`、`:7175-7192`、`:7204`）
///   未接入，因此不抽它们对应的随机数；
/// * 反射伤害与 `SpellEffect.Reflect` 只保留抽签（`reflect` 恒为 0）。
#[must_use]
pub fn resolve_player_vs_player(
    attacker: &CombatStats,
    defender: &CombatStats,
    damage: i32,
    defence: DefenceType,
    rng: &mut MirRng,
) -> PvpHit {
    let (armour, hit) = defender.get_armour(attacker, defence, rng);
    if !hit {
        return PvpHit {
            result: HitResult::missed(),
            energy_shield: false,
        };
    }

    if (rng.next_bounded(100) as i32) < defender.reflect {
        // 反射：Crystal 会对攻击方再结算一次 `attacker.Attacked(this, damage, type, false)`。
        // 简化：`reflect` 恒为 0（没有反射类装备），这里只保留抽签位置。
        return PvpHit {
            result: HitResult::missed(),
            energy_shield: false,
        };
    }

    let mut damage = damage;
    // `if (Stats[Stat.DamageReductionPercent] > 0) damage -= (damage * …) / 100;`（`:7127-7130`）
    if defender.damage_reduction_percent > 0 {
        damage -= damage * defender.damage_reduction_percent / 100;
    }

    if armour >= damage {
        return PvpHit {
            result: HitResult::missed(),
            energy_shield: false,
        };
    }

    // 气功波（`:7145-7154`）：只有 `> 0` 才会抽 `Random.Next(100)`。
    let mut energy_shield = false;
    if defender.energy_shield_percent > 0 {
        energy_shield = (rng.next_bounded(100) as i32) < defender.energy_shield_percent;
    }

    // 暴击（`:7156-7161`）：与 `MonsterObject.Attacked` 的公式相同。
    let mut critical = false;
    if (rng.next_bounded(100) as i32) < attacker.critical_rate * CRITICAL_RATE_WEIGHT {
        critical = true;
        let bonus = i64::from(damage) * i64::from(attacker.critical_damage) * 10
            / i64::from(CRITICAL_DAMAGE_WEIGHT);
        damage = i32::try_from(i64::from(damage) + bonus).unwrap_or(i32::MAX);
    }

    PvpHit {
        result: HitResult {
            damage: damage - armour,
            armour,
            critical,
            miss: false,
        },
        energy_shield,
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
/// 这里算的是**基础值**（等级曲线 + 三个负重上限），不含装备、技能、buff、行会加成。
/// 装备加成由 `World::refresh_stats` 在基础值之上叠加
/// （对应 Crystal `HumanObject.RefreshStats` 里 `RefreshLevelStats` → `RefreshEquipmentStats`
/// 的两步，`HumanObject.cs:1730-1769`）。
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
            // 负重三项：`Shared/BaseStats.cs:18-20`。
            set_weights(&mut stats, level_f, (3.0, 20.0, 13.0));
        }
        MirClass::Wizard => {
            stats.hp = health(level_f, 14, 15.0, 1.8, false);
            stats.mp = mana_wizard(level_f, 13, 5.0, 0.0);
            stats.min_dc = stat_curve(level_f, 7.0);
            stats.max_dc = stat_curve(level_f, 7.0);
            stats.min_mc = stat_curve(level_f, 7.0);
            stats.max_mc = stat_curve(level_f, 7.0);
            // `Shared/BaseStats.cs:33-35`。
            set_weights(&mut stats, level_f, (5.0, 100.0, 90.0));
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
            // `Shared/BaseStats.cs:48-50`。
            set_weights(&mut stats, level_f, (4.0, 50.0, 42.0));
        }
        MirClass::Assassin => {
            stats.hp = health(level_f, 14, 4.0, 3.25, false);
            stats.mp = mana_plain(level_f, 11, 5.0, 0.0);
            stats.min_dc = stat_curve(level_f, 8.0);
            stats.max_dc = stat_curve(level_f, 8.0);
            // `Shared/BaseStats.cs:65-67`。
            set_weights(&mut stats, level_f, (3.5, 33.0, 30.0));
        }
        MirClass::Archer => {
            stats.hp = health(level_f, 14, 4.0, 3.25, false);
            stats.mp = mana_plain(level_f, 11, 4.0, 0.0);
            stats.min_dc = stat_curve(level_f, 8.0);
            stats.max_dc = stat_curve(level_f, 8.0);
            stats.min_mc = stat_curve(level_f, 8.0);
            stats.max_mc = stat_curve(level_f, 8.0);
            // `Shared/BaseStats.cs:78-80`。
            set_weights(&mut stats, level_f, (4.0, 33.0, 30.0));
        }
    }

    // `RefreshStatCaps`：职业上限 + 非负 + `MinDC <= MaxDC`（`HumanObject.cs:2229-2253`）。
    stats.apply_caps();
    stats
}

/// 三个负重上限的 `Gain`（`BagWeight` / `WearWeight` / `HandWeight`）。
///
/// 三项的 `Base` 都是常数 50 / 15 / 12（`Shared/BaseStats.cs:18-20` 等），只有 `Gain` 随职业变，
/// 所以这里只传 `Gain` 三元组。
fn set_weights(stats: &mut CombatStats, level: f64, gains: (f64, f64, f64)) {
    stats.bag_weight = weight_cap(level, 50, gains.0);
    stats.wear_weight = weight_cap(level, 15, gains.1);
    stats.hand_weight = weight_cap(level, 12, gains.2);
}

/// `StatFormula.Weight`（`Shared/BaseStats.cs:180`）：`Base + (Level / Gain) * Level`。
///
/// `Gain` 在 C# 里是 `float`，所以是**浮点除法**再截断——1 级战士的背包负重是
/// `50 + (1/3)*1 = 50.33 → 50`，而不是整数除法会得到的 50（两者在这里巧合相同，
/// 但 2 级就分叉：`50 + (2/3)*2 = 51.33 → 51`，整数除法会算成 `50 + 0*2 = 50`）。
fn weight_cap(level: f64, base: i32, gain: f64) -> i32 {
    (f64::from(base) + (level / gain) * level) as i32
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
        let result =
            resolve_monster_attack(&attacker, &defender, 10, DefenceType::ACAgility, &mut rng);
        assert!(!result.miss);
        assert_eq!(result.armour, 2);
        assert_eq!(result.damage, 8);
        assert!(!result.critical, "怪物打玩家不判暴击（Crystal 原样行为）");
    }

    #[test]
    fn chicken_hit_rate_with_accuracy_seven_and_agility_ten_is_about_73_percent() {
        let attacker = CombatStats {
            accuracy: 7,
            min_dc: 2,
            max_dc: 4,
            ..CombatStats::default()
        };
        let defender = CombatStats {
            agility: 10,
            ..CombatStats::default()
        };

        // 20 刀只断言「明显不是全 0/全中」的区间，避免把某个种子钉死。
        let mut rng = MirRng::from_seed(2026);
        let mut hits = 0;
        for _ in 0..20 {
            let result =
                resolve_player_attack(&attacker, &defender, 3, DefenceType::ACAgility, &mut rng);
            if result.miss {
                assert_eq!(result.damage, 0, "Miss 的 DamageIndicator 值必须是 0");
            } else {
                hits += 1;
                assert_eq!(result.damage, 3, "AC=0 时命中应扣 3 点");
            }
        }
        assert!((10..=19).contains(&hits), "20 刀命中数 {hits} 超出合理区间");

        // 期望命中率 = 8/11 ≈ 72.7%（Next(0..=10) > 7 的只有 8/9/10）。
        let mut rng = MirRng::from_seed(2027);
        let mut hits = 0;
        for _ in 0..10_000 {
            let result =
                resolve_player_attack(&attacker, &defender, 3, DefenceType::ACAgility, &mut rng);
            if !result.miss {
                hits += 1;
            }
        }
        let rate = f64::from(hits) / 10_000.0;
        assert!((0.70..=0.76).contains(&rate), "命中率 {rate:.3} 偏离 8/11");
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
