//! 技能（魔法）：数据、学习、施法流程、伤害/治疗结算与持久化。
//!
//! 规则逐条对应 Crystal（参考根目录 `github\Crystal`，下文写相对路径）：
//!
//! | 本模块 | Crystal |
//! |---|---|
//! | [`UserMagic`] | `UserMagic`（`Server/MirDatabase/MagicInfo.cs:88-223`） |
//! | [`UserMagic::power`] / [`UserMagic::damage`] | `UserMagic.GetPower`/`GetDamage`（`MagicInfo.cs:180-217`） |
//! | [`UserMagic::delay_ms`] | `UserMagic.GetDelay`（`MagicInfo.cs:219-222`） |
//! | [`UserMagic::cost`] | `HumanObject.MagicCost`（`Server/MirObjects/HumanObject.cs:3375-3398`） |
//! | [`World::cast_magic`] | `HumanObject.Magic`（`HumanObject.cs:3400-3780`） |
//! | [`World::use_book`] | `PlayerObject.UseItem` 的 `ItemType.Book` 分支（`Server/MirObjects/PlayerObject.cs:6077-6089`） |
//! | `World::cast_buff_spell` | `Magic` 的增益分支（`HumanObject.cs:3544-3674`）+ `CompleteMagic`（`:6149-6242`）+ `Map.cs:1046-1097` |
//! | `World::level_magic` | `HumanObject.LevelMagic`（`HumanObject.cs:6903-6980`） |
//! | [`World::set_magic_key`] | `MirConnection.MagicKey`（`Server/MirNetwork/MirConnection.cs:1543-1566`） |
//! | 延迟效果队列 | `DelayedType.Magic` → `Map.CompleteMagic`（`Server/MirEnvir/Map.cs:856-`）/ `HumanObject.CompleteMagic`（`HumanObject.cs:5903-`） |
//! | 治疗分段生效 | `HumanObject.ProcessRegen` 的 `HealAmount` 块（`HumanObject.cs:602-617`） |
//!
//! # 一次施法的完整流程（`HumanObject.Magic`）
//!
//! ```csharp
//! if (!CanCast) { Enqueue(S.UserLocation); return; }               // :3402-3406
//! magic = GetMagic(spell); if (magic == null) { ...; return; }      // :3408-3414
//! if (location != 0 && magic.Info.Range != 0
//!     && !InRange(CurrentLocation, location, magic.Info.Range)) return;   // :3416
//! AttackTime = now + MoveDelay; SpellTime = now + 1800;             // :3424-3425
//! ActionTime = now + MoveDelay;                                     // :3429
//! if (now < magic.CastTime + magic.GetDelay()) { UserLocation; return; }   // :3434-3440
//! cost = MagicCost(magic); if (cost > MP) { UserLocation; return; } // :3442-3448
//! RegenTime = now + RegenDelay; ChangeMP(-cost);                    // :3450-3451
//! Direction = dir; Enqueue(S.UserLocation);                         // :3456-3458
//! ...按 Spell 分派，伤害/治疗排进延迟队列...                        // :3499-3771
//! if (cast) magic.CastTime = now;                                   // :3773-3776
//! Enqueue(S.Magic); Broadcast(S.ObjectMagic);                        // :3778-3779
//! ```
//!
//! 几个容易看漏的点：
//!
//! * **冷却检查在「写计时器」之后**：一次因冷却/缺蓝被拒的施法，仍然会锁
//!   `ActionTime`/`SpellTime` 各 600/1800ms（Crystal 原样行为）。
//! * **范围检查失败时什么都不发**（连 `UserLocation` 都没有）。
//! * 每次成功施法都会给本人发 `S.Magic`、给周围玩家广播 `S.ObjectMagic`；
//!   即使这一次没有打中任何目标也一样。
//! * `S.MagicDelay` **不是每次施法都发**——只有 `LevelMagic` 升级时发一次，
//!   用来把「缩短后的冷却」告诉客户端（`HumanObject.cs:6972-6976`）。
//!
//! # 简化与差异（与 Crystal 的已知分歧）
//!
//! 每条都在调用点标了 `// 简化：`。
//!
//! * `spell_target_lock`（`C.Magic.SpellTargetLock`）未实现：落点始终取包里的 `location`。
//! * `CanFly`（目标格是否允许飞行道具）未实现——地图层没有飞行属性，只按距离判定。
//! * PK：`Bolt` 一族仍只认怪物（既有简化）；`FireWall`/`Lightning`/`Poisoning`
//!   用 [`World::is_attack_target_player`] 判定玩家目标，与物理攻击的 PK 规则一致。
//! * 目标合法性在扣蓝之前严格校验（Crystal 是扣蓝后才发现目标无效，仍算一次施法）。
//! * 增益类技能由 [`SpellKind::Buff`] 实现（9 个），效果、时长与包序见 [`crate::buffs`]；
//!   仍然「只登记、不生效」的少数技能见 [`REGISTERED_UNSUPPORTED`]。
//! * 施法者死亡后已排入队列的延迟效果仍会结算（Crystal 只检查对象是否还在图上，
//!   同样不检查 `Dead`）。
//! * 护身符/毒药消耗未实现（`SoulFireBall`、`SoulShield`、`Hiding`、`Summon*`、`Poisoning`
//!   在 Crystal 里都要消耗护身符/毒药，`HumanObject.cs:4382-4396`、`:4515`、`:4489-4498`、
//!   `:4419-4426`、`:4369-4377`），原因是本作的护身符/毒药还没有接到技能上。
//! * 第 2 轮新增的 7 个技能见 [`SpellKind`]：`FireWall`/`Lightning`/`Teleport`/`Hiding`/
//!   `Summon`/`Poison`；逐条差异（无 bind 点的传送、绿→红的施毒策略、火墙的轻量场等）
//!   都写在各自方法文档里。

use std::collections::HashMap;

use mir_core::MirRng;
use mir_core::Point;
use mir_core::TextKey;
use mir_core::enums::{
    BuffType, DamageType, DefenceType, ItemType, MirClass, MirDirection, MirGridType, PoisonType,
    RequiredClass, Spell, Stat,
};
use mir_db::MagicInfo;
use mir_protocol::data::{ClientBuff, ClientMagic, Stats, UserItem};
use mir_protocol::packets::server::{
    AddBuff, Magic, MagicDelay, MagicLeveled, NewMagic, ObjectHidden, ObjectMagic, ObjectPoisoned,
    ObjectRemove, ObjectSpell, ObjectTeleportIn, ObjectTeleportOut, Poisoned, RemoveBuff, UseItem,
};
use mir_protocol::{PacketReader, PacketWriter, ProtocolError, ServerPacket};

use crate::aoi::AoiKind;
use crate::buffs::{
    BuffKind, ENERGY_SHIELD_MIN_CHANCE, PendingBuff, SECOND_MS, SHIELD_AREA_RADIUS,
};
use crate::combat::PendingHit;
use crate::monster::MonsterKind;
use crate::world::{PLAYER_REGEN_DELAY_MS, Player, VIEW_RANGE, World};
/// 技能最高等级：Crystal 的 `MagicInfo` 只有 `Level1`/`Level2`/`Level3` 三档
/// （`MagicInfo.cs:16`），`LevelMagic` 的 `default:` 分支对 3 级直接返回
/// （`HumanObject.cs:6968-6969`）。
pub const MAGIC_LEVEL_MAX: u8 = 3;

/// 一次施法锁住的「动作时间」：Crystal `HumanObject.MoveDelay = 600`
/// （`HumanObject.cs:186`），`Magic` 里 `ActionTime = Envir.Time + MoveDelay`
/// （`:3429`）、`AttackTime = Envir.Time + MoveDelay`（`:3424`）。
pub const MAGIC_CAST_ACTION_MS: i64 = 600;

/// 施法后的全局技能间隔：Crystal `SpellTime = Envir.Time + 1800`（`HumanObject.cs:3425`）。
pub const MAGIC_SPELL_TIME_MS: i64 = 1_800;

/// `LevelMagic` 单次经验抽取的上限：`Envir.Random.Next(3) + 1`（`HumanObject.cs:6905`）。
pub const MAGIC_EXP_ROLL_MAX: u8 = 3;

/// `HellFire` 一次施法推进的格数：`DelayedAction(..., 4)`（`HumanObject.cs:4107`）。
pub const HELLFIRE_RANGE: i32 = 4;

/// `HellFire` 每格的推进间隔：`Envir.Time + 100`（`Server/MirEnvir/Map.cs:885`）。
pub const HELLFIRE_STEP_MS: i64 = 100;

/// 区域技能（`FireBang`/`MassHealing`）的半径：地图层遍历 `location ± 1`
/// （`Map.cs:956-961`、`:1259-1264`），即 3×3。
pub const AREA_RADIUS: i32 = 1;

/// 治疗分段间隔：Crystal `HumanObject.HealDelay = 600`（`HumanObject.cs:186`），
/// 由 `ProcessRegen` 的 `HealAmount` 块使用（`:602-617`）。
pub const HEAL_DELAY_MS: i64 = 600;

/// 按对象 ID 找目标的搜索半径：`FindObject(targetID, 10)`（`HumanObject.cs:3468`）。
pub const MAGIC_TARGET_SEARCH_RANGE: i32 = 10;

/// 延迟伤害结算时允许目标偏离「施法时落点」的格数：
/// `InRange(target.CurrentLocation, targetLocation, 2)`（`HumanObject.cs:5928`）。
pub const MAGIC_HIT_TOLERANCE: i32 = 2;

/// 火墙的伤害间隔：Crystal `SpellObject.TickSpeed = 2000`（`Map.cs:1134`）。
pub const FIREWALL_TICK_MS: i64 = 2_000;

/// 火墙基础持续秒数：`ExpireTime = now + (10 + Value / 2) * 1000`（`Map.cs:1133`）。
pub const FIREWALL_BASE_SECONDS: i64 = 10;

/// 一次火墙施法铺开的格子数：落点 + 上/右/下/左（`Map.cs:1145-1149`）。
pub const FIREWALL_CELLS: usize = 5;

/// 雷电的贯穿格数：`for (int i = 0; i < 6; i++)`（`Map.cs:1194`）。
pub const LIGHTNING_RANGE: i32 = 6;

/// 雷电的固定延迟：`DelayedAction(..., Envir.Time + 500, ...)`（`HumanObject.cs:4213`）。
pub const LIGHTNING_DELAY_MS: i64 = 500;

/// 瞬息移动的延迟：`DelayedAction(..., Envir.Time + 200, ...)`（`HumanObject.cs:3539`）。
pub const TELEPORT_DELAY_MS: i64 = 200;

/// 瞬息移动最多尝试的随机落点数（`PlayerObject.cs:4639`）。
pub const TELEPORT_ATTEMPTS: u32 = 200;

/// 瞬息移动后的「传送法力惩罚」时长：30 秒（`HumanObject.cs:6106`）。
pub const TELEPORT_PENALTY_MS: i64 = 30_000;

/// 传送法力惩罚百分比（`HumanObject.cs:6106` 的 `TemporalFlux` → `TeleportManaPenaltyPercent = 30`）。
pub const TELEPORT_MANA_PENALTY_PERCENT: i32 = 30;

/// 隐身生效延迟（`HumanObject.cs:4496`）。
pub const HIDING_DELAY_MS: i64 = 500;

/// 召唤生效延迟（`HumanObject.cs:4437`、`:4486`）。
pub const SUMMON_DELAY_MS: i64 = 500;

/// 一个主人最多同时拥有的怪物类召唤物数量（`HumanObject.cs:4417`、`:4466`）。
pub const MAX_MONSTER_PETS: usize = 2;

/// 施毒的生效延迟（`HumanObject.cs:4374`）。
pub const POISON_DELAY_MS: i64 = 500;

/// 中毒每跳间隔：`TickSpeed = 2000`（`HumanObject.cs:6058`、`:6068`）。
pub const POISON_TICK_MS: i64 = 2_000;

/// 雷电术对不死系的伤害倍率：`damage * 1.5F`（`HumanObject.cs:4126`）。
pub const UNDEAD_DAMAGE_PERCENT: i32 = 150;

/// 刚读档、还没施过法时的 `CastTime`：Crystal 读档后强制写成 `int.MinValue`
/// （`Server/MirDatabase/CharacterInfo.cs:232`），因此第一次施法永远不触发冷却。
pub const INITIAL_CAST_TIME: i64 = i32::MIN as i64;

/// 一条技能存档记录（`UserMagic.Save`，`MagicInfo.cs:135-144`）的字节数：
/// `u8 spell + u8 level + u8 key + u16 experience + bool IsTempSpell + i64 CastTime`。
pub const MAGIC_RECORD_LEN: usize = 14;

/// 本作允许的角色技能条数上限（读取存档时的防御性上限；Crystal 的客户端技能栏是 24 格）。
pub const MAX_MAGICS: usize = 64;

/// 已登记但**本作尚未实现效果**的技能。
///
/// 这些技能在 Crystal 里会 `AddBuff`（`HumanObject.cs:4964-4973`、`:6171-6182` 等），
/// 而本作还没有对应的效果实现，因此只登记（`MagicInfo` 能查到、能学、能进客户端技能栏），
/// 施法时返回 [`MagicError::UnsupportedSpell`]，**不扣蓝、不进冷却**。
///
/// 曾经在这个列表里的 9 个增益技能（`ProtectionField`/`Haste`/`MagicShield`/`SoulShield`/
/// `BlessedArmour`/`EnergyShield`/`LightBody`/`Fury`/`MagicBooster`）已经由
/// [`SpellKind::Buff`] 实现，见 [`crate::buffs`] 与 `docs/world/magic.md` 的支持矩阵。
pub const REGISTERED_UNSUPPORTED: &[Spell] = &[
    // 战士：狂暴 / 金刚不坏（`HumanObject.cs:4964-4973`、`:6171-6182`）。
    Spell::Rage,
    Spell::ImmortalSkin,
    // 道士：强化术 / 神兽强化。
    Spell::UltimateEnhancer,
    Spell::PetEnhancer,
    // 法师：元素护盾。
    Spell::ElementalBarrier,
];

/// 玩家已学的一个技能（Crystal `UserMagic`，`Server/MirDatabase/MagicInfo.cs:88-101`）。
///
/// 字段与 Crystal 一一对应；`Info` 不入存档，运行时按 `spell` 去
/// [`crate::gamedata::GameData::magic`] 查。`IsTempSpell`/`CastTime` 两个字段没有建模：
/// 前者是「临时技能」（GM 授予、下线回收），后者读档时会被强制重置
/// （`CharacterInfo.cs:232`），当前没有任何可观察行为。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserMagic {
    /// 技能编号（Crystal `UserMagic.Spell`）。
    pub spell: Spell,
    /// 技能等级 0..=3（Crystal `UserMagic.Level`）。
    pub level: u8,
    /// 客户端快捷键（Crystal `UserMagic.Key`；`0` = 未绑定）。
    pub key: u8,
    /// 当前修炼值（Crystal `UserMagic.Experience` 是 `ushort`，这里放宽到 `u32`；
    /// 发给客户端时截断成 `u16`，见 [`Self::client_magic`]）。
    pub experience: u32,
}

impl UserMagic {
    /// 新学会的技能：0 级、修炼值 0、未绑定快捷键
    /// （`new UserMagic(spell)` 用字段默认值，`MagicInfo.cs:114-119`）。
    pub const fn new(spell: Spell) -> Self {
        Self {
            spell,
            level: 0,
            key: 0,
            experience: 0,
        }
    }

    /// 施法耗蓝：`Info.BaseCost + Info.LevelCost * Level`（`HumanObject.cs:3377`）。
    ///
    /// 简化：`Stat.TeleportManaPenaltyPercent`（`:3379-3385`）与 `Spell.Plague` 的特殊算法
    /// （`:3392-3395`）不参与——本作没有传送类技能，plague 也不在支持清单里；
    /// `Stat.ManaPenaltyPercent` 见 [`Self::cost_with_penalty`]。
    pub fn cost(&self, info: &MagicInfo) -> i32 {
        i32::from(info.base_cost) + i32::from(info.level_cost) * i32::from(self.level)
    }

    /// 带「魔法消耗惩罚」的施法耗蓝（Crystal `HumanObject.MagicCost`，`HumanObject.cs:3375-3398`）。
    ///
    /// ```csharp
    /// int cost = magic.Info.BaseCost + magic.Info.LevelCost * magic.Level;
    /// if (Stats[Stat.ManaPenaltyPercent] > 0) cost += (cost * Stats[Stat.ManaPenaltyPercent]) / 100;
    /// ```
    ///
    /// 唯一会写入 `Stat.ManaPenaltyPercent` 的是魔力增幅
    /// （`BuffType.MagicBooster`，`6 + Level`，`HumanObject.cs:6234`）。
    pub fn cost_with_penalty(&self, info: &MagicInfo, mana_penalty_percent: i32) -> i32 {
        let cost = self.cost(info);
        if mana_penalty_percent > 0 {
            cost + cost * mana_penalty_percent / 100
        } else {
            cost
        }
    }

    /// 冷却时长：`Info.DelayBase - Level * Info.DelayReduction`（`MagicInfo.cs:219-222`）。
    ///
    /// 注意这是 `long` 差值，理论上可以变成负数（数据里 `DelayReduction` 远小于
    /// `DelayBase / 3`，实际不会），负值等价于「没有冷却」。
    pub fn delay_ms(&self, info: &MagicInfo) -> i64 {
        i64::from(info.delay_base) - i64::from(info.delay_reduction) * i64::from(self.level)
    }

    /// 威力倍率：`MultiplierBase + Level * MultiplierBonus`（`MagicInfo.cs:185-188`）。
    pub fn multiplier(&self, info: &MagicInfo) -> f64 {
        f64::from(info.multiplier_base) + f64::from(self.level) * f64::from(info.multiplier_bonus)
    }

    /// 技能基础威力：`(int)Math.Round((MPower() / 4F) * (Level + 1) + DefPower())`
    /// （`MagicInfo.cs:190-193`）。
    ///
    /// `Math.Round(double)` 默认是**银行家舍入**（中点取偶），这里用 `round_ties_even` 对齐。
    /// 抽签顺序：先 `MPower()` 再 `DefPower()`（C# 从左到右求值）。
    pub fn power(&self, info: &MagicInfo, rng: &mut MirRng) -> i32 {
        let m_power = roll_u16(info.m_power_base, info.m_power_bonus, rng);
        let def_power = roll_u16(info.power_base, info.power_bonus, rng);
        let value =
            f64::from(m_power) / 4.0 * f64::from(u16::from(self.level) + 1) + f64::from(def_power);
        value.round_ties_even() as i32
    }

    /// 最终数值：`(int)((DamageBase + GetPower()) * GetMultiplier())`
    /// （`MagicInfo.cs:180-183`）。
    ///
    /// `DamageBase` 由调用方先抽好（通常是 `GetAttackPower(MinMC, MaxMC)`），
    /// 所以**攻方的攻击力抽签发生在技能威力抽签之前**。
    pub fn damage(&self, info: &MagicInfo, base: i32, rng: &mut MirRng) -> i32 {
        let power = self.power(info, rng);
        (f64::from(base.saturating_add(power)) * self.multiplier(info)) as i32
    }

    /// `GetPower(int power)`（`MagicInfo.cs:214-217`）：
    /// `(int)Math.Round(power / 4F * (Level + 1) + DefPower())`。
    ///
    /// 与 [`Self::power`] 的区别：这里**不抽 `MPower`**，只抽 `DefPower`
    /// （`PowerBase .. PowerBase + PowerBonus - 1`，`:204-212`）。魔法盾/气功波/幽灵盾
    /// 都用这个重载来算「时长」或「回血量」。
    pub fn power_with_base(&self, info: &MagicInfo, power: i32, rng: &mut MirRng) -> i32 {
        let def_power = roll_u16(info.power_base, info.power_bonus, rng);
        let value =
            f64::from(power) / 4.0 * f64::from(u16::from(self.level) + 1) + f64::from(def_power);
        value.round_ties_even() as i32
    }

    /// 组装下发给客户端的技能视图（`UserMagic.CreateClientMagic`，`MagicInfo.cs:155-178`）。
    ///
    /// 与 Crystal 的两处差异：
    /// * `Experience` 截断到 `u16`（Crystal 字段本身就是 `ushort`）；
    /// * `CastTime` 恒发 0。Crystal 发的是 `CastTime - Envir.Time`，而读档时 `CastTime`
    ///   被重置成 `int.MinValue`（`CharacterInfo.cs:232`），会发出一个极大的负数；
    ///   客户端只关心「还有多久转好」的正数，0 与那个负数在语义上等价。
    pub fn client_magic(&self, info: &MagicInfo) -> ClientMagic {
        ClientMagic {
            name: info.name.clone(),
            spell: self.spell,
            base_cost: info.base_cost,
            level_cost: info.level_cost,
            icon: info.icon,
            level1: info.level1,
            level2: info.level2,
            level3: info.level3,
            need1: info.need1,
            need2: info.need2,
            need3: info.need3,
            level: self.level,
            key: self.key,
            experience: u16::try_from(self.experience).unwrap_or(u16::MAX),
            delay: self.delay_ms(info),
            range: info.range,
            cast_time: 0,
        }
    }
}

/// `MagicInfo` 里的 `MPowerBase/MPowerBonus`（或 `PowerBase/PowerBonus`）取一个值：
/// `Random.Next(Base, Bonus + Base)`，`Bonus == 0` 时直接返回 `Base`
/// （`MagicInfo.cs:195-212`）。
fn roll_u16(base: u16, bonus: u16, rng: &mut MirRng) -> i32 {
    if bonus > 0 {
        let min = i32::from(base);
        // `.NET Random.Next(min, max)` 右开 → 上限减 1。
        let max = i32::from(base) + i32::from(bonus) - 1;
        rng.next_range(min, max)
    } else {
        i32::from(base)
    }
}

/// 技能施法期的运行时状态（Crystal 上分散在 `HumanObject`/`UserMagic` 的几个字段）。
#[derive(Debug, Clone, Default)]
pub struct MagicState {
    /// 下一次可以施法的时刻：Crystal `HumanObject.SpellTime`（`HumanObject.cs:187`）。
    pub spell_time: i64,
    /// 每个技能的「上次施法时刻」：Crystal `UserMagic.CastTime`。
    ///
    /// 单独放一张表而不是塞进 [`UserMagic`]，是为了让 `UserMagic` 与存档格式一一对应
    /// （`CastTime` 读档时反正会被重置，见 [`INITIAL_CAST_TIME`]）。
    pub casts: HashMap<Spell, i64>,
    /// 待生效的治疗量：Crystal `MapObject.HealAmount`（`Server/MirObjects/MapObject.cs:75`）。
    pub heal_amount: u32,
    /// 下一次结算治疗分段的时刻：Crystal `HumanObject.HealTime`（`HumanObject.cs:187`）。
    pub heal_time: i64,
    /// 是否隐身：Crystal `MapObject.Hidden`（`Server/MirObjects/MapObject.cs:78-92`）。
    ///
    /// 放在 [`MagicState`] 而不是 [`crate::world::Player`] 上，避免给玩家结构加字段
    ///（17 个测试与 `session.rs` 都直接构造 `Player`）。
    pub hidden: bool,
    /// 隐身到期时刻（Crystal `Buff.ExpireTime`，`BuffType.Hiding`）。
    pub hidden_until: i64,
    /// 传送法力惩罚到期时刻（Crystal `BuffType.TemporalFlux`，
    /// `HumanObject.cs:6106`；耗蓝公式见 `:3379-3385`）。
    pub teleport_penalty_until: i64,
    /// 玩家身上的毒（Crystal `HumanObject.PoisonList`，`MapObject.cs:74`）。
    ///
    /// **与 buffs 分开**：毒有独立的 tick / 时长 / 施毒者，不参与 `Stats` 加成。
    pub poisons: Vec<PoisonState>,
    /// 最近一次物理攻击/攻击魔法的目标（本作扩展）：召唤物优先打主人的目标
    /// （Crystal 由召唤物自己 `FindTarget`，见 `MonsterObject.cs:1918-1997`）。
    pub last_target: Option<u32>,
}

/// 一条中毒状态（Crystal `Poison`，`Server/MirObjects/MapObject.cs:74`）。
///
/// 字段与 Crystal 一一对应：
/// * `kind` → `PoisonType`（本作只做 `Green`/`Red`）；
/// * `value` → `Value`（绿毒每跳扣血；红毒为 0）；
/// * `duration`/`ticks` → `Duration`/`Time`（**单位是跳数**，不是毫秒）；
/// * `tick_at` → `TickTime`（本作存绝对时刻，Crystal 存下一次 tick 的时刻）；
/// * `owner_id` → `Owner`（毒主下线后整条毒移除，`MonsterObject.cs:1464-1474`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoisonState {
    /// 毒类型。
    pub kind: PoisonType,
    /// 绿毒每跳伤害（红毒为 0）。
    pub value: i32,
    /// 总跳数。
    pub duration: u32,
    /// 已经跳过的次数。
    pub ticks: u32,
    /// 下一次结算时刻。
    pub tick_at: i64,
    /// 施毒者对象 ID（0 = 无主）。
    pub owner_id: u32,
}

impl PoisonState {
    /// 新建一条毒；第一跳在 `tick_at` 时刻结算（Crystal `TickTime` 初值 0 → 首跳立即）。
    #[must_use]
    pub const fn new(
        kind: PoisonType,
        value: i32,
        duration: u32,
        tick_at: i64,
        owner_id: u32,
    ) -> Self {
        Self {
            kind,
            value,
            duration,
            ticks: 0,
            tick_at,
            owner_id,
        }
    }
}

/// 地图上的一片持续法术场（Crystal `SpellObject`，`Server/MirObjects/SpellObject.cs`）。
///
/// 本作没有 `ObjectType.Spell` 这一层对象：火墙用「中心 + 半径 + 到期 + 伤害间隔」表示。
/// Crystal 一次火墙施法铺 5 格（`Map.cs:1145-1181`），本作对应 5 个半径 0 的场；
/// 每 tick 用 AOI 查半径内的玩家/怪物结算伤害。
#[derive(Debug, Clone, PartialEq)]
pub struct SpellField {
    /// 客户端对象 ID（`S.ObjectSpell.ObjectID`）。
    pub object_id: u32,
    /// 地图名。
    pub map: String,
    /// 中心格。
    pub center: Point,
    /// 作用半径（火墙为 0：只烧同格）。
    pub radius: i32,
    /// 每次结算的伤害（Crystal `SpellObject.Value`）。
    pub damage: i32,
    /// 施法者对象 ID（下线/换图后整片火墙消失，`SpellObject.cs:57`、`:82-90`）。
    pub caster_id: u32,
    /// 到期时刻（`ExpireTime`）。
    pub expire_at: i64,
    /// 下一次结算时刻（`TickTime`）。
    pub next_tick: i64,
    /// 结算间隔（`TickSpeed = 2000`，`Map.cs:1134`）。
    pub tick_ms: i64,
    /// 法术编号（目前只有 `FireWall`）。
    pub spell: Spell,
    /// 客户端朝向（`S.ObjectSpell.Direction`）。
    pub direction: MirDirection,
    /// 施法实例号（Crystal `CastInstanceId`，用于 `FireWallLimit` 按实例回收，
    /// `HumanObject.cs:4151-4175`、`:4183-4207`）。
    pub cast_id: u32,
}

/// 技能类别：决定施法时走哪条效果分支。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpellKind {
    /// 单体攻击魔法（`FireBall`/`GreatFireBall`/`ThunderBolt`/`SoulFireBall`）：
    /// `HumanObject.cs:3939-3952`、`:4120-4131`、`:4379-4398`。
    Bolt,
    /// 前方贯穿（`HellFire`）：`HumanObject.cs:4103-4119` + `Map.cs:875-910`。
    HellFire,
    /// 落点 3×3 范围伤害（`FireBang`/`IceStorm`）：`HumanObject.cs:4142-4148` + `Map.cs:951-991`。
    FireBang,
    /// 单体治疗（`Healing`）：`HumanObject.cs:4355-4364` + `:6018-6027`。
    Heal,
    /// 3×3 范围治疗（`MassHealing`）：`HumanObject.cs:4526-4532` + `Map.cs:1255-1296`。
    MassHealing,
    /// 增益类技能（套 buff）：`HumanObject.cs:3544-3674` + `CompleteMagic` 的各分支。
    ///
    /// 包括 `ProtectionField`/`MagicShield`/`SoulShield`/`BlessedArmour`/`EnergyShield`/
    /// `Haste`/`LightBody`/`Fury`/`MagicBooster`，效果见 [`crate::buffs`]。
    Buff,
    /// 火墙（`HumanObject.cs:4149-4182` + `Map.cs:1101-1183` + `SpellObject.cs:128-135`）。
    FireWall,
    /// 雷电（`HumanObject.cs:4209-4215` + `Map.cs:1187-1218`）：前方 6 格逐格命中。
    Lightning,
    /// 瞬息移动（`HumanObject.cs:3537-3539` + `:6095-6109` + `PlayerObject.cs:4633-4648`）。
    Teleport,
    /// 隐身（`HumanObject.cs:4489-4498` + `:6133-6145`）。
    Hiding,
    /// 召唤骷髅/神兽（`HumanObject.cs:4399-4488` + `Map.cs:914-933`）。
    Summon,
    /// 施毒（`HumanObject.cs:4365-4378` + `:6041-6077`）。
    Poison,
    /// 已登记、但本作没有效果（见 [`REGISTERED_UNSUPPORTED`]）。
    Unsupported,
}

/// 某个技能属于哪一类（决定施法分支）。
pub const fn spell_kind(spell: Spell) -> SpellKind {
    match spell {
        Spell::FireBall | Spell::GreatFireBall | Spell::ThunderBolt | Spell::SoulFireBall => {
            SpellKind::Bolt
        }
        Spell::HellFire => SpellKind::HellFire,
        Spell::FireBang => SpellKind::FireBang,
        Spell::Healing => SpellKind::Heal,
        Spell::MassHealing => SpellKind::MassHealing,
        Spell::ProtectionField
        | Spell::MagicShield
        | Spell::SoulShield
        | Spell::BlessedArmour
        | Spell::EnergyShield
        | Spell::Haste
        | Spell::LightBody
        | Spell::Fury
        | Spell::MagicBooster => SpellKind::Buff,
        Spell::FireWall => SpellKind::FireWall,
        Spell::Lightning => SpellKind::Lightning,
        Spell::Teleport => SpellKind::Teleport,
        Spell::Hiding => SpellKind::Hiding,
        Spell::SummonSkeleton | Spell::SummonShinsu => SpellKind::Summon,
        Spell::Poisoning => SpellKind::Poison,
        _ => SpellKind::Unsupported,
    }
}

/// 一条已经排进延迟队列的魔法效果（Crystal 的 `DelayedAction(DelayedType.Magic, ...)`）。
#[derive(Debug, Clone, PartialEq)]
pub enum MagicEffect {
    /// 单体伤害（`FireBall` 一族）。`location` 是施法时的落点，结算时目标必须还在它 ±2 格内
    /// （`HumanObject.cs:5928`）。
    Damage {
        /// 目标对象 ID。
        target_id: u32,
        /// 施法时的落点。
        location: Point,
        /// 已经抽好的伤害。
        damage: i32,
    },
    /// 落点 3×3 的范围伤害（`FireBang`，`Map.cs:951-991`）。
    AreaDamage {
        /// 区域中心。
        location: Point,
        /// 已经抽好的伤害。
        damage: i32,
    },
    /// 前方贯穿（`HellFire`）：从 `location` 沿 `direction` 每 100ms 推进一格。
    HellFire {
        /// 上一格的位置（第一格是施法者脚下，`Map.cs:878` 的 `PointMove(..., 1)`）。
        location: Point,
        /// 推进方向。
        direction: MirDirection,
        /// 已经抽好的伤害。
        damage: i32,
        /// 还要推进几格。
        remaining: u8,
    },
    /// 单体治疗（`Healing`）：结算时把 `amount` 累加进目标的 `HealAmount`。
    Heal {
        /// 目标玩家对象 ID。
        target_id: u32,
        /// 治疗量。
        amount: i32,
    },
    /// 落点 3×3 的范围治疗（`MassHealing`）。
    AreaHeal {
        /// 区域中心。
        location: Point,
        /// 每次结算的治疗量。
        amount: i32,
    },
    /// 给单个目标套增益（`CompleteMagic` 里的 `AddBuff`，`HumanObject.cs:6149-6242`）。
    ///
    /// 数值在施法瞬间就抽好了（见 [`PendingBuff`]），这里只负责「到点写入」。
    Buff {
        /// 目标玩家对象 ID。
        target_id: u32,
        /// 待写入的增益。
        buff: PendingBuff,
    },
    /// 7×7 范围内的友方增益（`SoulShield`/`BlessedArmour`，`Map.cs:1046-1097`）。
    AreaBuff {
        /// 区域中心（施法落点）。
        location: Point,
        /// 增益种类。
        kind: BuffKind,
        /// 持续时长（毫秒）。
        duration_ms: i64,
        /// 加成落在哪个防御属性上（`Stat.MaxAC` 或 `Stat.MaxMAC`，`Map.cs:1082`）。
        stat: Stat,
    },
    /// 火墙落地（`Map.cs:1101-1183`）：在 `location` 周围铺 5 格持续伤害场。
    FireWall {
        /// 施法时的地图（施法者 500ms 内换图时仍落在原图，Crystal 的 `ActionList` 挂在地图上）。
        map: String,
        /// 落点（五格的中心）。
        location: Point,
        /// 每格每次结算的伤害。
        damage: i32,
        /// 施法实例号（`FireWallLimit` 回收用）。
        cast_id: u32,
    },
    /// 雷电（`Map.cs:1187-1218`）：从 `origin` 沿 `direction` 推进 6 格，每格打第一个可攻击对象。
    Lightning {
        /// 施法者施法时所在格。
        origin: Point,
        /// 方向。
        direction: MirDirection,
        /// 已经抽好的伤害。
        damage: i32,
    },
    /// 瞬息移动（`HumanObject.cs:6095-6109`）。
    Teleport,
    /// 隐身（`HumanObject.cs:6133-6145`）。
    Hiding {
        /// 持续时长（毫秒）：`GetAttackPower(MinSC, MaxSC) + (Level + 1) * 5` **秒**。
        duration_ms: i64,
    },
    /// 召唤骷髅/神兽（`Map.cs:914-933`）。
    Summon {
        /// 召唤技能（决定模板与上限）。
        spell: Spell,
    },
    /// 施毒（`HumanObject.cs:6041-6077`）。
    Poison {
        /// 目标对象 ID。
        target_id: u32,
        /// `magic.GetDamage(GetAttackPower(MinSC, MaxSC))` 的结果。
        power: i32,
    },
}

/// 延迟魔法效果（Crystal 里每个对象/地图各有一条 `ActionList`，本作合并成世界级队列）。
#[derive(Debug, Clone, PartialEq)]
pub struct PendingMagic {
    /// 结算时刻（毫秒）。
    pub at: i64,
    /// 施法者对象 ID。
    pub caster_id: u32,
    /// 技能编号。
    pub spell: Spell,
    /// 效果。
    pub effect: MagicEffect,
}

/// 技能系统错误。
///
/// 施法的「失败包」按 Crystal 的位置发：`Busy`/`NotLearned`/`CoolingDown`/`NotEnoughMana`
/// 会发 `S.UserLocation`；`OutOfRange` 什么都不发（`HumanObject.cs:3416`）。
/// 学习失败会发 `S.UseItem{Success=false}`（`PlayerObject.cs:5814-5824`）。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MagicError {
    /// 对象不在世界里。
    #[error("object {0} is not in the world")]
    NotInWorld(u32),
    /// 对象已死亡（施法与用书都要求活着，`HumanObject.cs:156`、`PlayerObject.cs:5820`）。
    #[error("object {object_id} is dead")]
    Dead {
        /// 对象 ID。
        object_id: u32,
    },
    /// 动作时间或技能间隔还没结束（`CanCast`，`HumanObject.cs:152-158`）。
    #[error("action not finished; cannot cast before {until} ms")]
    Busy {
        /// 可以施法的最早时刻。
        until: i64,
    },
    /// 数据库里没有这个技能的模板（`magic.Info == null`，`HumanObject.cs:3410-3414`）。
    #[error("spell {0:?} has no template")]
    UnknownSpell(Spell),
    /// 没有学会这个技能。
    #[error("spell {0:?} not learned")]
    NotLearned(Spell),
    /// 落点超出 `MagicInfo.Range`（`HumanObject.cs:3416`）。
    #[error("target distance {distance} exceeds cast range {range}")]
    OutOfRange {
        /// 实际距离（切比雪夫）。
        distance: i32,
        /// 技能射程。
        range: i32,
    },
    /// 冷却中：`now < CastTime + GetDelay()`（`HumanObject.cs:3436-3440`）。
    #[error("spell on cooldown; cannot cast again before {until} ms")]
    CoolingDown {
        /// 冷却结束时刻。
        until: i64,
    },
    /// 蓝不够：`cost > MP`（`HumanObject.cs:3444-3448`，注意等于时允许，MP 会变成 0）。
    #[error("insufficient mana: requires {cost}, current {mp}")]
    NotEnoughMana {
        /// 本次耗蓝。
        cost: i32,
        /// 当前魔法。
        mp: i32,
    },
    /// 目标不合法（死亡/不在同图/攻击类技能打到玩家等）。
    #[error("invalid target")]
    InvalidTarget,
    /// 已登记但本作没有效果（见 [`REGISTERED_UNSUPPORTED`]）。
    #[error("spell {0:?} effect is not implemented")]
    UnsupportedSpell(Spell),
    /// 不支持的物品网格（Crystal 的 `UseItem` 只处理角色背包）。
    #[error("unsupported item grid {0:?}")]
    UnsupportedGrid(MirGridType),
    /// 背包里没有这个 UUID 的物品。
    #[error("no item with UUID {unique_id} in the inventory")]
    ItemNotFound {
        /// 物品 UUID。
        unique_id: u64,
    },
    /// 物品模板缺失。
    #[error("item template {0} does not exist")]
    UnknownItem(i32),
    /// 用的不是书。
    #[error("item {item_index} is not a spell book")]
    NotABook {
        /// 物品模板编号。
        item_index: i32,
    },
    /// 已经学过这个技能。
    ///
    /// 简化：Crystal 的书籍分支**不查重**（`PlayerObject.cs:6077-6089` 直接
    /// `Info.Magics.Add(magic)`），连用两本同样的书会得到两条同 `Spell` 的记录。
    /// 本作拒绝并提示，见 `docs/world/magic.md` 的「与 Crystal 差异」。
    #[error("spell {0:?} already learned")]
    AlreadyLearned(Spell),
    /// 职业不符（Crystal `CanUseItem`，`HumanObject.cs:1042-1079`）。
    #[error("class mismatch: requires {required:?}, current {class:?}")]
    WrongClass {
        /// 物品要求的职业位。
        required: RequiredClass,
        /// 玩家职业。
        class: MirClass,
    },
    /// 性别不符（`HumanObject.cs:1024-1040`）。
    #[error("gender mismatch")]
    WrongGender,
    /// 等级不够（`HumanObject.cs:1081-1089`）。
    #[error("level too low: requires {required}, current {level}")]
    LowLevel {
        /// 需要的等级。
        required: u8,
        /// 当前等级。
        level: u16,
    },
}

/// 一次施法的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CastReport {
    /// 技能编号。
    pub spell: Spell,
    /// 施法时的技能等级。
    pub level: u8,
    /// 主目标对象 ID（没有目标时为 0，治疗默认指向自己）。
    pub target_id: u32,
    /// 落点。
    pub location: Point,
    /// 本次抽到的伤害（单体/范围/贯穿都有；治疗类为 `None`）。
    pub damage: Option<i32>,
    /// 本次抽到的治疗量（治疗类技能才有）。
    pub heal: Option<i32>,
    /// 本次套上的增益种类（增益类技能才有，见 [`crate::buffs::BuffKind`]）。
    ///
    /// 注意它表示「这次施法成功登记了增益」，延迟生效的 6 个技能要到
    /// 500ms（幽灵盾是「距离 × 50 + 500」ms）之后才真正出现在玩家身上。
    pub buff: Option<BuffKind>,
    /// 本次扣掉的魔法。
    pub cost: i32,
}

/// 一次「用书学会技能」的结果。
#[derive(Debug, Clone, PartialEq)]
pub struct LearnedMagic {
    /// 学会的技能。
    pub magic: UserMagic,
    /// 被消耗的书（数量已经扣减；`None` 表示这一格被清空）。
    pub book: Option<UserItem>,
}

/// 技能存档的编解码错误。
#[derive(Debug, thiserror::Error)]
pub enum MagicCodecError {
    /// 协议层错误（读到文件尾、计数非法……）。
    #[error("failed to parse spell blob: {0}")]
    Protocol(#[from] ProtocolError),
    /// 技能条数超过上限。
    #[error("spell count {count} exceeds limit {limit}")]
    TooManyMagics {
        /// 声明的条数。
        count: usize,
        /// 允许的上限。
        limit: usize,
    },
}

/// 编码 Crystal 的魔法列表整块：`int32 条数` + 逐条 `UserMagic.Save`。
///
/// 逐字对照 `CharacterInfo.Save`（`CharacterInfo.cs:458-463`）与
/// `UserMagic.Save`（`MagicInfo.cs:135-144`）：
///
/// ```text
/// i32  count
/// per magic: u8 Spell, u8 Level, u8 Key, u16 Experience, bool IsTempSpell, i64 CastTime
/// ```
///
/// `IsTempSpell` 恒写 `false`、`CastTime` 恒写 0：两者都不参与本作的任何判定
/// （见 [`UserMagic`] 的说明与 [`INITIAL_CAST_TIME`]）。
pub fn encode_magics(magics: &[UserMagic]) -> Vec<u8> {
    let mut writer = PacketWriter::with_capacity(4 + magics.len() * MAGIC_RECORD_LEN);
    writer.write_count(magics.len());
    for magic in magics {
        writer.write_u8(magic.spell.value());
        writer.write_u8(magic.level);
        writer.write_u8(magic.key);
        writer.write_u16(u16::try_from(magic.experience).unwrap_or(u16::MAX));
        writer.write_bool(false); // IsTempSpell
        writer.write_i64(0); // CastTime
    }
    writer.into_inner()
}

/// 解码 Crystal 的魔法列表整块，见 [`encode_magics`]。
///
/// 空块（新角色）得到空列表。与 Crystal 的差异：Crystal 读档时对**查不到模板**的技能
/// 直接 `continue`（`CharacterInfo.cs:230`），本作把过滤留给上层——解码器只保证字节层
/// 正确，组装客户端视图时再跳过 [`crate::gamedata::GameData::magic`] 返回 `None` 的条目。
pub fn decode_magics(bytes: &[u8]) -> Result<Vec<UserMagic>, MagicCodecError> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let mut reader = PacketReader::new(bytes);
    let count = reader.read_count()?;
    if count > MAX_MAGICS {
        return Err(MagicCodecError::TooManyMagics {
            count,
            limit: MAX_MAGICS,
        });
    }
    let mut magics = Vec::with_capacity(count);
    for _ in 0..count {
        let spell = Spell::from_value(reader.read_u8()?).unwrap_or_default();
        let level = reader.read_u8()?;
        let key = reader.read_u8()?;
        let experience = u32::from(reader.read_u16()?);
        let _is_temp_spell = reader.read_bool()?;
        let _cast_time = reader.read_i64()?;
        magics.push(UserMagic {
            spell,
            level,
            key,
            experience,
        });
    }
    Ok(magics)
}

impl World {
    /// 按技能编号查模板（Crystal `Envir.MagicInfoList` 的线性查找）。
    pub fn magic_info(&self, spell: Spell) -> Option<&MagicInfo> {
        self.game_data.as_ref()?.magic(spell)
    }

    /// 取玩家已学的某个技能。
    pub fn player_magic(&self, object_id: u32, spell: Spell) -> Option<&UserMagic> {
        self.players
            .get(&object_id)?
            .magics
            .iter()
            .find(|magic| magic.spell == spell)
    }

    /// 组装某个玩家此刻的客户端技能列表（登录时 `S.UserInformation.Magics` 用）。
    ///
    /// 查不到模板的技能被跳过（Crystal 读档时就直接丢弃这类记录，
    /// `CharacterInfo.cs:230`）。
    pub fn client_magics(&self, object_id: u32) -> Vec<ClientMagic> {
        let Some(player) = self.players.get(&object_id) else {
            return Vec::new();
        };
        let Some(data) = self.game_data.as_ref() else {
            return Vec::new();
        };
        player
            .magics
            .iter()
            .filter_map(|magic| data.magic(magic.spell).map(|info| magic.client_magic(info)))
            .collect()
    }

    /// 学会一个技能（不涉及物品，GM 命令与用书都会走这里）。
    pub fn learn_magic(&mut self, object_id: u32, spell: Spell) -> Result<UserMagic, MagicError> {
        if self.magic_info(spell).is_none() {
            return Err(MagicError::UnknownSpell(spell));
        }
        let Some(player) = self.players.get_mut(&object_id) else {
            return Err(MagicError::NotInWorld(object_id));
        };
        if player.magics.iter().any(|magic| magic.spell == spell) {
            return Err(MagicError::AlreadyLearned(spell));
        }
        let magic = UserMagic::new(spell);
        player.magics.push(magic);
        // `SendMagicInfo(magic)`（`PlayerObject.cs:6087`）。
        self.send_new_magic(object_id, magic);
        Ok(magic)
    }

    /// 用一本书学技能（Crystal `PlayerObject.UseItem` 的 `ItemType.Book` 分支，
    /// `PlayerObject.cs:6077-6089`）。
    ///
    /// 流程：查物品 → `CanUseItem`（性别/职业/等级，`HumanObject.cs:1020-1160`）→
    /// 书里的 `Shape` 就是 `Spell`（`PlayerObject.cs:6078`）→ 查模板 → 学会 →
    /// 发 `S.NewMagic` → 扣掉一本 → 发 `S.UseItem{Success=true}`。
    ///
    /// 失败时按 Crystal 的位置发 `S.UseItem{Success=false}`，并在
    /// 「职业/等级/已学」这些需要解释的情况下补一条系统聊天。
    pub fn use_book(&mut self, object_id: u32, unique_id: u64) -> Result<LearnedMagic, MagicError> {
        let Some(player) = self.players.get(&object_id) else {
            return Err(MagicError::NotInWorld(object_id));
        };
        if player.dead {
            // `if (Dead && !(Type == Scroll && Shape == 6)) { Enqueue(p); return; }`（`:5820-5824`）。
            self.use_item_failure(object_id, unique_id);
            return Err(MagicError::Dead { object_id });
        }
        let Some((index, item)) = self.find_inventory_item(object_id, unique_id) else {
            self.use_item_failure(object_id, unique_id);
            return Err(MagicError::ItemNotFound { unique_id });
        };
        let Some(info) = self.item_info(item.item_index).cloned() else {
            self.use_item_failure(object_id, unique_id);
            return Err(MagicError::UnknownItem(item.item_index));
        };
        if info.item_type != ItemType::Book {
            self.use_item_failure(object_id, unique_id);
            return Err(MagicError::NotABook {
                item_index: info.index,
            });
        }
        if let Err(error) = self.check_item_requirements(object_id, &info) {
            self.use_item_failure(object_id, unique_id);
            return Err(error);
        }
        let Some(spell) = u8::try_from(info.shape).ok().and_then(Spell::from_value) else {
            // `UserMagic((Spell)item.Info.Shape)` 之后 `magic.Info == null`（`:6078-6084`）。
            self.use_item_failure(object_id, unique_id);
            return Err(MagicError::UnknownSpell(Spell::None));
        };
        if self.magic_info(spell).is_none() {
            self.use_item_failure(object_id, unique_id);
            return Err(MagicError::UnknownSpell(spell));
        }
        if let Some(existing) = self.player_magic(object_id, spell).copied() {
            if existing.level >= MAGIC_LEVEL_MAX {
                let message = self.format(
                    object_id,
                    TextKey::MagicAlreadyLearned,
                    &[("spell", &info.name)],
                );
                self.send_system_chat(object_id, &message);
                self.use_item_failure(object_id, unique_id);
                return Err(MagicError::AlreadyLearned(spell));
            }
            // 本作扩展：真实 `Server.MirDB` 没有独立的 `MagicLevel` 书字段，
            // 已会技能再读同 Shape 的书按 Crystal 技能升级书处理（+1 级，封顶 3 级）。
            let mut upgraded = existing;
            upgraded.level = upgraded.level.saturating_add(1);
            upgraded.experience = 0;
            let mut book = None;
            if let Some(player) = self.players.get_mut(&object_id) {
                let magic_index = player.magics.iter().position(|magic| magic.spell == spell);
                if let Some(slot) = magic_index.and_then(|i| player.magics.get_mut(i)) {
                    *slot = upgraded;
                }
                if let Some(slot) = player.inventory.get_mut(index) {
                    if let Some(stack) = slot {
                        if stack.count > 1 {
                            stack.count -= 1;
                            book = Some(stack.clone());
                        } else {
                            *slot = None;
                        }
                    }
                }
            }
            self.send_to_player(
                object_id,
                &ServerPacket::MagicLeveled(MagicLeveled {
                    object_id,
                    spell,
                    level: upgraded.level,
                    experience: u16::try_from(upgraded.experience).unwrap_or(u16::MAX),
                })
                .encode_frame(),
            );
            self.send_to_player(
                object_id,
                &ServerPacket::UseItem(UseItem {
                    unique_id,
                    success: true,
                    grid: MirGridType::Inventory,
                })
                .encode_frame(),
            );
            return Ok(LearnedMagic {
                magic: upgraded,
                book,
            });
        }

        let magic = self.learn_magic(object_id, spell)?;
        // 扣书：`if (item.Count > 1) item.Count--; else Info.Inventory[index] = null;`
        // （`PlayerObject.cs:6330-6331`）。
        let mut book = None;
        if let Some(player) = self.players.get_mut(&object_id) {
            if let Some(slot) = player.inventory.get_mut(index) {
                if let Some(stack) = slot {
                    if stack.count > 1 {
                        stack.count -= 1;
                        book = Some(stack.clone());
                    } else {
                        *slot = None;
                    }
                }
            }
        }
        // `p.Success = true; Enqueue(p);`（`:6336-6337`）。
        self.send_to_player(
            object_id,
            &ServerPacket::UseItem(UseItem {
                unique_id,
                success: true,
                grid: MirGridType::Inventory,
            })
            .encode_frame(),
        );
        Ok(LearnedMagic { magic, book })
    }

    /// `HumanObject.CanUseItem` 里对书有意义的那几项：性别、职业、等级
    /// （`HumanObject.cs:1020-1160`）。
    ///
    /// 实现在物品系统（[`World::can_use_requirements`]）——喝药水/用卷轴走的是同一条判定
    /// （`PlayerObject.cs:5814` 的 `CanUseItem(item)`），这里只把
    /// [`crate::items::ItemError`] 翻译回 [`MagicError`]，保持 [`World::use_book`] 的签名。
    fn check_item_requirements(
        &self,
        object_id: u32,
        info: &mir_protocol::data::ItemInfo,
    ) -> Result<(), MagicError> {
        use crate::items::ItemError;

        self.can_use_requirements(object_id, info)
            .map_err(|error| match error {
                ItemError::WrongGender => MagicError::WrongGender,
                ItemError::WrongClass { required, class } => {
                    MagicError::WrongClass { required, class }
                }
                ItemError::LowLevel { required, level } => MagicError::LowLevel { required, level },
                ItemError::NotInWorld(object_id) => MagicError::NotInWorld(object_id),
                // 复活卷轴那条「必须处于死亡状态」的检查与书无关；其余分支书本走不到。
                _ => MagicError::UnknownItem(info.index),
            })
    }

    /// `C.MagicKey`：改技能快捷键（`MirConnection.cs:1543-1566`）。
    ///
    /// 先把手上的其它技能里占用了同一个键的清零，再给目标技能绑定
    /// （Crystal 逐条遍历 `Info.Magics`，这里等价实现）。
    pub fn set_magic_key(&mut self, object_id: u32, spell: Spell, key: u8) -> bool {
        let Some(player) = self.players.get_mut(&object_id) else {
            return false;
        };
        if !player.magics.iter().any(|magic| magic.spell == spell) {
            return false;
        }
        for magic in &mut player.magics {
            if magic.spell == spell {
                magic.key = key;
            } else if magic.key == key {
                magic.key = 0;
            }
        }
        true
    }

    /// 施法（Crystal `HumanObject.Magic`，`HumanObject.cs:3400-3780`）。
    ///
    /// 判定顺序见模块文档；成功时扣蓝、写计时器、发 `S.Magic`/`S.ObjectMagic`，
    /// 并把伤害/治疗排进延迟队列（`World::resolve_pending_magic` 每 tick 结算）。
    pub fn cast_magic(
        &mut self,
        object_id: u32,
        spell: Spell,
        direction: MirDirection,
        target_id: u32,
        location: Point,
    ) -> Result<CastReport, MagicError> {
        let now = self.now();
        let Some(player) = self.players.get(&object_id) else {
            return Err(MagicError::NotInWorld(object_id));
        };
        if player.dead {
            return Err(MagicError::Dead { object_id });
        }
        // `CanCast`（`HumanObject.cs:152-158`）：`!Dead && now >= ActionTime && now >= SpellTime`。
        if now < player.action_time || now < player.magic.spell_time {
            self.send_user_location(object_id);
            return Err(MagicError::Busy {
                until: player.action_time.max(player.magic.spell_time),
            });
        }
        let Some(magic) = player.magics.iter().find(|magic| magic.spell == spell) else {
            self.send_user_location(object_id);
            return Err(MagicError::NotLearned(spell));
        };
        let magic = *magic;
        let Some(info) = self.magic_info(spell).cloned() else {
            self.send_user_location(object_id);
            return Err(MagicError::UnknownSpell(spell));
        };
        let kind = spell_kind(spell);
        if kind == SpellKind::Unsupported {
            self.send_user_location(object_id);
            return Err(MagicError::UnsupportedSpell(spell));
        }

        let position = player.position;
        let map = player.map.clone();
        let range = i32::from(info.range);
        let mut target_id = target_id;
        let mut target_location = location;

        // --- 目标解析与范围（`HumanObject.cs:3416`、`:3460-3474`） ------------------
        match kind {
            SpellKind::Bolt => {
                if target_id == 0 || target_id == object_id {
                    return Err(MagicError::InvalidTarget);
                }
                let Some(target_position) = self.attack_target_position(&map, object_id, target_id)
                else {
                    return Err(MagicError::InvalidTarget);
                };
                // `FindObject(targetID, 10)` 的搜索半径（`:3468`）。
                if position.max_distance(target_position) > MAGIC_TARGET_SEARCH_RANGE {
                    return Err(MagicError::InvalidTarget);
                }
                if target_location.x == 0 && target_location.y == 0 {
                    target_location = target_position;
                }
            }
            SpellKind::Heal => {
                // `case Spell.Healing: if (target == null) { target = DefaultMagicTarget; targetID = ObjectID; }`
                // （`HumanObject.cs:3506-3512`，`DefaultMagicTarget => this`，`:3399`）。
                if target_id == 0 {
                    target_id = object_id;
                }
                let Some(target) = self.players.get(&target_id) else {
                    return Err(MagicError::InvalidTarget);
                };
                if target.dead || target.map != map {
                    return Err(MagicError::InvalidTarget);
                }
                target_location = target.position;
            }
            SpellKind::FireBang | SpellKind::MassHealing | SpellKind::HellFire => {
                // 落点为空/在原点时退回施法者脚下（Crystal 会照原样用 `(0,0)` 施法，
                // 见 `HumanObject.cs:3416` 对 `location == 0` 的放行）。
                if target_location.x == 0 && target_location.y == 0 {
                    target_location = position;
                }
                target_id = 0;
            }
            SpellKind::Poison => {
                // `Poisoning(target, magic)`：目标必须是可攻击对象（`HumanObject.cs:4367`）。
                if target_id == 0 || target_id == object_id {
                    return Err(MagicError::InvalidTarget);
                }
                let Some(target_position) = self.magic_attack_position(&map, object_id, target_id)
                else {
                    return Err(MagicError::InvalidTarget);
                };
                if position.max_distance(target_position) > MAGIC_TARGET_SEARCH_RANGE {
                    return Err(MagicError::InvalidTarget);
                }
                if target_location.x == 0 && target_location.y == 0 {
                    target_location = target_position;
                }
            }
            SpellKind::FireWall => {
                // `FireWall(magic, spellTargetLock ? target.CurrentLocation : location)`（`:3565-3566`）。
                if target_location.x == 0 && target_location.y == 0 {
                    target_location = position;
                }
                target_id = 0;
            }
            SpellKind::Lightning => {
                // `Lightning(magic)` 只用到方向（`HumanObject.cs:3568-3569`、`:4209-4215`）。
                if target_location.x == 0 && target_location.y == 0 {
                    target_location = position;
                }
                target_id = 0;
            }
            SpellKind::Teleport | SpellKind::Hiding | SpellKind::Summon => {
                // `Teleport/Blink` 用包里的 location 只做射程预检（`:3537-3539`）；
                // 隐身/召唤都作用在自己身上（`:3534-3542`、`:3605-3606`）。
                if target_location.x == 0 && target_location.y == 0 {
                    target_location = position;
                }
                if matches!(kind, SpellKind::Hiding | SpellKind::Summon) {
                    target_id = object_id;
                } else {
                    target_id = 0;
                }
            }
            SpellKind::Buff => {
                // 增益的落点为空时退回自己脚下（同样按 `:3416` 的放行规则）。
                if target_location.x == 0 && target_location.y == 0 {
                    target_location = position;
                }
                // 目标：`EnergyShield` 可以给友方，其余 8 个都作用在自己身上
                // （`HumanObject.cs:4755` 的 `target = this`；`Haste`/`LightBody`/`Fury`/
                // `MagicShield`/`MagicBooster`/`ProtectionField` 的 `AddBuff(..., this, ...)`）。
                // 简化：没有队伍系统，「友方」只认同图存活的玩家。
                let valid_friendly = target_id != 0
                    && self
                        .players
                        .get(&target_id)
                        .is_some_and(|target| !target.dead && target.map == map);
                if !matches!(
                    spell,
                    Spell::EnergyShield | Spell::SoulShield | Spell::BlessedArmour
                ) || !valid_friendly
                {
                    target_id = 0;
                }
            }
            SpellKind::Unsupported => unreachable!("already dispatched above"),
        }
        // `if ((location.X != 0) && (location.Y != 0) && magic.Info.Range != 0
        //      && !Functions.InRange(CurrentLocation, location, magic.Info.Range)) return;`
        // （`HumanObject.cs:3416`）。注意 Crystal 在这里**什么都不发**。
        if range != 0 && !position.in_range(target_location, range) {
            return Err(MagicError::OutOfRange {
                distance: position.max_distance(target_location),
                range,
            });
        }

        // --- 计时器（`HumanObject.cs:3424-3432`） -----------------------------------
        // 注意：Crystal 在检查冷却与蓝量**之前**就把这三个时间写掉了，
        // 因此被拒绝的施法同样会锁动作 600ms、锁技能 1800ms。
        if let Some(player) = self.players.get_mut(&object_id) {
            player.attack_time = now + MAGIC_CAST_ACTION_MS;
            player.magic.spell_time = now + MAGIC_SPELL_TIME_MS;
            player.action_time = now + MAGIC_CAST_ACTION_MS;
            player.regen_time = now + PLAYER_REGEN_DELAY_MS;
            player.direction = direction;
        }

        // --- 冷却（`HumanObject.cs:3434-3440`） -------------------------------------
        let delay = magic.delay_ms(&info);
        let cast_at = self
            .players
            .get(&object_id)
            .and_then(|player| player.magic.casts.get(&spell).copied())
            .unwrap_or(INITIAL_CAST_TIME);
        let ready_at = cast_at.saturating_add(delay);
        if now < ready_at {
            self.send_user_location(object_id);
            return Err(MagicError::CoolingDown { until: ready_at });
        }

        // --- 蓝量（`HumanObject.cs:3442-3448`） -------------------------------------
        // `MagicCost`：基础耗蓝 + `Stat.ManaPenaltyPercent` 惩罚（`:3375-3398`）。
        let mana_penalty = self.players[&object_id].stats.mana_penalty_percent;
        let mut cost = magic.cost_with_penalty(&info, mana_penalty);
        // 传送类技能的额外惩罚：`if (spell == Teleport && Stats[TeleportManaPenaltyPercent] > 0)
        //     cost += cost * percent / 100;`（`:3379-3385`；值来自 TemporalFlux，`:6106`）。
        if spell == Spell::Teleport && now < self.players[&object_id].magic.teleport_penalty_until {
            cost += cost * TELEPORT_MANA_PENALTY_PERCENT / 100;
        }
        let mp = self.players.get(&object_id).map_or(0, |player| player.mp);
        if cost > mp {
            self.send_user_location(object_id);
            return Err(MagicError::NotEnoughMana { cost, mp });
        }
        // `ChangeMP(-cost)`（`:3451`）→ `SendHealthChanged`。
        self.change_player_mp(object_id, -cost);

        // --- 施法回显（`HumanObject.cs:3456-3458`） ---------------------------------
        self.send_user_location(object_id);
        // 记下最近一次攻击目标（召唤物优先打主人的目标，本作扩展）。
        if target_id != 0
            && matches!(kind, SpellKind::Bolt | SpellKind::Poison)
            && let Some(player) = self.players.get_mut(&object_id)
        {
            player.magic.last_target = Some(target_id);
        }

        // --- 效果（`HumanObject.cs:3499-3771` + `Map.cs:856-`） ---------------------
        let stats = self.players[&object_id].stats;
        let level = self.players[&object_id].level;
        let mut damage = None;
        let mut heal = None;
        let mut buff = None;
        let mut effects: Vec<(i64, MagicEffect)> = Vec::new();
        match kind {
            SpellKind::Bolt => {
                // `magic.GetDamage(GetAttackPower(Stats[MinMC], Stats[MaxMC]))`
                // （`Fireball`，`HumanObject.cs:3943`；`ThunderBolt`，`:4124`）。
                let base = stats.attack_power(stats.min_mc, stats.max_mc, &mut self.rng);
                let mut value = magic.damage(&info, base, &mut self.rng);
                // `if (target.Undead) damage = (int)(damage * 1.5F);`（`:4126`）。
                if spell == Spell::ThunderBolt
                    && self
                        .monster(target_id)
                        .is_some_and(|monster| monster.undead)
                {
                    // `(int)(damage * 1.5F)` 的整数等价（正伤害截断一致）。
                    value = value.saturating_mul(UNDEAD_DAMAGE_PERCENT) / 100;
                }
                damage = Some(value);
                // `ThunderBolt` 固定 500ms（`:4128`）；其余单体魔法是
                // `delay = MaxDistance(CurrentLocation, target.CurrentLocation) * 50 + 500`
                // （`HumanObject.cs:3945`，注释「50 MS per Step」）。
                let travel = if spell == Spell::ThunderBolt {
                    LIGHTNING_DELAY_MS
                } else {
                    i64::from(position.max_distance(target_location)) * 50 + 500
                };
                effects.push((
                    now + travel,
                    MagicEffect::Damage {
                        target_id,
                        location: target_location,
                        damage: value,
                    },
                ));
            }
            SpellKind::HellFire => {
                // `DelayedAction(..., Envir.Time + 500, this, magic, damage, CurrentLocation, Direction, 4)`
                // （`HumanObject.cs:4103-4108`）。
                let base = stats.attack_power(stats.min_mc, stats.max_mc, &mut self.rng);
                let value = magic.damage(&info, base, &mut self.rng);
                damage = Some(value);
                let mut directions = vec![direction];
                if magic.level == MAGIC_LEVEL_MAX {
                    // 3 级同时向左右各偏 45° 各放一道（`HumanObject.cs:4110-4119`）。
                    directions.push(rotate_direction(direction, 1));
                    directions.push(rotate_direction(direction, -1));
                }
                for dir in directions {
                    effects.push((
                        now + 500,
                        MagicEffect::HellFire {
                            location: position,
                            direction: dir,
                            damage: value,
                            remaining: u8::try_from(HELLFIRE_RANGE).unwrap_or(4),
                        },
                    ));
                }
            }
            SpellKind::FireBang => {
                // `FireBang`：`DelayedAction(..., Envir.Time + 500, this, magic, damage, location)`
                // （`HumanObject.cs:4142-4148`），区域 3×3 在 `Map.cs:951-991`。
                let base = stats.attack_power(stats.min_mc, stats.max_mc, &mut self.rng);
                let value = magic.damage(&info, base, &mut self.rng);
                damage = Some(value);
                effects.push((
                    now + 500,
                    MagicEffect::AreaDamage {
                        location: target_location,
                        damage: value,
                    },
                ));
            }
            SpellKind::Heal => {
                // `health = magic.GetDamage(GetAttackPower(MinSC, MaxSC) * 2) + Level`
                // （`HumanObject.cs:4359`）。
                let base = stats
                    .attack_power(stats.min_sc, stats.max_sc, &mut self.rng)
                    .saturating_mul(2);
                let value = magic
                    .damage(&info, base, &mut self.rng)
                    .saturating_add(i32::from(level));
                heal = Some(value);
                effects.push((
                    now + 500,
                    MagicEffect::Heal {
                        target_id,
                        amount: value,
                    },
                ));
            }
            SpellKind::MassHealing => {
                // `value = magic.GetDamage(GetAttackPower(MinSC, MaxSC))`（`HumanObject.cs:4528`）。
                let base = stats.attack_power(stats.min_sc, stats.max_sc, &mut self.rng);
                let value = magic.damage(&info, base, &mut self.rng);
                heal = Some(value);
                effects.push((
                    now + 500,
                    MagicEffect::AreaHeal {
                        location: target_location,
                        amount: value,
                    },
                ));
            }
            SpellKind::Buff => {
                // 增益的数值在**这里**抽好（Crystal 也是施法时就 `GetPower(...)`，
                // 只是写入 buff 的时刻延后，见 `HumanObject.cs:3591`）。
                buff = self.cast_buff_spell(
                    &BuffCast {
                        object_id,
                        spell,
                        magic,
                        info: &info,
                        stats,
                        position,
                        target_location,
                        target_id,
                        now,
                    },
                    &mut effects,
                );
            }
            SpellKind::FireWall => {
                // `FireWall(magic, location)`（`HumanObject.cs:4149-4182`）：
                // 伤害在这里抽好，500ms 后由 `Map.CompleteMagic` 铺 5 格火墙。
                let base = stats.attack_power(stats.min_mc, stats.max_mc, &mut self.rng);
                let value = magic.damage(&info, base, &mut self.rng);
                damage = Some(value);
                // `FireWallLimit` 在施法瞬间检查：超限且无法回收 → `ReceiveChat` 后
                // 不排延迟动作（`:4151-4175`）。Crystal 此时 `cast` 仍为 true，
                // 所以蓝已扣、`S.Magic` 照发、`CastTime` 照记。
                if self.can_cast_fire_wall(&map, object_id) {
                    let cast_id = self.next_spell_cast_id;
                    self.next_spell_cast_id = self.next_spell_cast_id.wrapping_add(1).max(1);
                    effects.push((
                        now + 500,
                        MagicEffect::FireWall {
                            map: map.clone(),
                            location: target_location,
                            damage: value,
                            cast_id,
                        },
                    ));
                }
            }
            SpellKind::Lightning => {
                // `Lightning(magic)`：`DelayedAction(..., now + 500, this, magic, damage,
                // CurrentLocation, Direction)`（`HumanObject.cs:4209-4215`）。
                let base = stats.attack_power(stats.min_mc, stats.max_mc, &mut self.rng);
                let value = magic.damage(&info, base, &mut self.rng);
                damage = Some(value);
                effects.push((
                    now + LIGHTNING_DELAY_MS,
                    MagicEffect::Lightning {
                        origin: position,
                        direction,
                        damage: value,
                    },
                ));
            }
            SpellKind::Teleport => {
                // `case Spell.Teleport: ActionList.Add(DelayedAction(Magic, now + 200, magic, location))`
                // （`HumanObject.cs:3537-3539`）；实际落点由 `CompleteMagic` 决定（`:6095-6109`）。
                effects.push((now + TELEPORT_DELAY_MS, MagicEffect::Teleport));
            }
            SpellKind::Hiding => {
                // `Hiding(magic)`：`DelayedAction(..., now + 500, magic,
                //     GetAttackPower(MinSC, MaxSC) + (magic.Level + 1) * 5)`（`:4489-4498`）。
                let seconds = stats
                    .attack_power(stats.min_sc, stats.max_sc, &mut self.rng)
                    .saturating_add((i32::from(magic.level) + 1) * 5)
                    .max(0);
                effects.push((
                    now + HIDING_DELAY_MS,
                    MagicEffect::Hiding {
                        duration_ms: i64::from(seconds) * SECOND_MS,
                    },
                ));
            }
            SpellKind::Summon => {
                // `SummonSkeleton/SummonShinsu` 在施法阶段只做「能不能召唤」的判定，
                // 真正的落点/生成延迟 500ms（`HumanObject.cs:4437`、`:4486`）。
                effects.push((now + SUMMON_DELAY_MS, MagicEffect::Summon { spell }));
            }
            SpellKind::Poison => {
                // `int power = magic.GetDamage(GetAttackPower(MinSC, MaxSC));`
                // 延迟 500ms（`HumanObject.cs:4372-4374`）。
                let base = stats.attack_power(stats.min_sc, stats.max_sc, &mut self.rng);
                let value = magic.damage(&info, base, &mut self.rng);
                effects.push((
                    now + POISON_DELAY_MS,
                    MagicEffect::Poison {
                        target_id,
                        power: value,
                    },
                ));
            }
            SpellKind::Unsupported => unreachable!("already dispatched above"),
        }
        for (at, effect) in effects {
            self.pending_magic.push(PendingMagic {
                at,
                caster_id: object_id,
                spell,
                effect,
            });
        }

        // `if (cast) magic.CastTime = Envir.Time;`（`HumanObject.cs:3773-3776`）。
        if let Some(player) = self.players.get_mut(&object_id) {
            player.magic.casts.insert(spell, now);
        }

        // `Enqueue(new S.Magic {...}); Broadcast(new S.ObjectMagic {...});`（`:3778-3779`）。
        self.send_to_player(
            object_id,
            &ServerPacket::Magic(Magic {
                spell,
                target_id,
                target: target_location,
                cast: true,
                level: magic.level,
                secondary_target_ids: Vec::new(),
            })
            .encode_frame(),
        );
        let packet = ServerPacket::ObjectMagic(ObjectMagic {
            object_id,
            location: position,
            direction,
            spell,
            target_id,
            target: target_location,
            cast: true,
            level: magic.level,
            self_broadcast: false,
            secondary_target_ids: Vec::new(),
        })
        .encode_frame();
        self.broadcast(&map, position, Some(object_id), &packet, object_id);

        Ok(CastReport {
            spell,
            level: magic.level,
            target_id,
            location: target_location,
            damage,
            heal,
            buff,
            cost,
        })
    }

    /// 增益类技能的副作用（Crystal `Magic` 的 `switch` 分支 + `CompleteMagic` 的对应分支）。
    ///
    /// 9 个技能的公式与发包时机（逐条对照 `HumanObject.cs` / `Map.cs`）：
    ///
    /// | 技能 | 加成 | 时长 | 生效时机 |
    /// |---|---|---|---|
    /// | `ProtectionField` | `MinAC/MaxAC = round(MaxAC × (0.2 + 0.03L))`（`:4958`） | `45 + 15L` 秒（`:4957`） | 立刻（`:3650-3651` 直接调用） |
    /// | `Haste` | `AttackSpeed = 2L + 2`（`:6151`） | `25 + 15L` 秒（`:6151`） | 500ms（`:3544-3546`） |
    /// | `LightBody` | `Agility = 2(L+1)`（`:6189`） | `30(L+1)` 秒（`:6189`） | 500ms |
    /// | `Fury` | `AttackSpeed = 4`（`:6162`） | `60 + 10L` 秒（`:6162`） | 500ms |
    /// | `MagicShield` | `DamageReductionPercent = 10(L+2)`（`:6203`） | `GetPower(GetAttackPower(MinMC,MaxMC)+15)` 秒（`:3591`） | 500ms |
    /// | `MagicBooster` | `MinMC = MaxMC = 6 + 6L`、`ManaPenaltyPercent = 6 + L`（`:6232-6235`） | 60 秒（`:6237`） | 500ms |
    /// | `EnergyShield` | `EnergyShieldPercent = round(100 / chance)`、`EnergyShieldHPGain = GetPower(GetAttackPower(MinSC,MaxSC))`（`:4757-4768`） | `30 + 50L` 秒（`:4757`） | 立刻（`:3672-3673`） |
    /// | `SoulShield` | `MaxMAC = Level / 7 + 4`（按**受术者**等级，`Map.cs:1082`） | `GetAttackPower(MinSC,MaxSC) × 4 + 50(L+1)` 秒（`:4521`） | `距离 × 50 + 500` ms，落点 7×7 |
    /// | `BlessedArmour` | `MaxAC = Level / 7 + 4`（同上） | 同上 | 同上 |
    ///
    /// `L` 是技能等级（0..=3）。`chance = max(2, 10 - (Luck / 3 + L + 1))`（`:4760-4762`）。
    ///
    /// 简化：`SoulShield`/`BlessedArmour` 在 Crystal 里要消耗 1 个护身符
    /// （`GetAmulet(1)`，`:4515`），本作的消耗品还没接到技能上，这里不扣。
    fn cast_buff_spell(
        &mut self,
        cast: &BuffCast<'_>,
        effects: &mut Vec<(i64, MagicEffect)>,
    ) -> Option<BuffKind> {
        let BuffCast {
            object_id,
            spell,
            magic,
            info,
            stats,
            position,
            target_location,
            target_id,
            now,
        } = *cast;
        let level = i32::from(magic.level);
        match spell {
            Spell::ProtectionField => {
                let duration = i64::from(45 + 15 * level) * SECOND_MS;
                let add = (f64::from(stats.max_ac) * (0.2 + 0.03 * f64::from(level)))
                    .round_ties_even() as i32;
                let mut delta = Stats::default();
                delta.set(Stat::MaxAC, add);
                delta.set(Stat::MinAC, add);
                self.add_buff(
                    object_id,
                    BuffKind::ProtectionField,
                    duration,
                    delta,
                    object_id,
                );
                self.level_magic(object_id, spell);
                Some(BuffKind::ProtectionField)
            }
            Spell::Haste => {
                let duration = i64::from(25 + 15 * level) * SECOND_MS;
                let mut delta = Stats::default();
                delta.set(Stat::AttackSpeed, level * 2 + 2);
                let buff = PendingBuff::new(BuffKind::Haste, duration, delta);
                effects.push((
                    now + 500,
                    MagicEffect::Buff {
                        target_id: object_id,
                        buff,
                    },
                ));
                Some(BuffKind::Haste)
            }
            Spell::LightBody => {
                // `(magic.Level + 1) * (Settings.Second * 30)`，`Agility = (Level + 1) * 2`（`:6189`）。
                let duration = i64::from((level + 1) * 30) * SECOND_MS;
                let mut delta = Stats::default();
                delta.set(Stat::Agility, (level + 1) * 2);
                let buff = PendingBuff::new(BuffKind::LightBody, duration, delta);
                effects.push((
                    now + 500,
                    MagicEffect::Buff {
                        target_id: object_id,
                        buff,
                    },
                ));
                Some(BuffKind::LightBody)
            }
            Spell::Fury => {
                // `(Settings.Second * 60) + (magic.Level * 10000)`（`:6162`）——每级多 10 秒。
                let duration = 60 * SECOND_MS + i64::from(level) * 10_000;
                let mut delta = Stats::default();
                delta.set(Stat::AttackSpeed, 4);
                let buff = PendingBuff::new(BuffKind::Fury, duration, delta);
                effects.push((
                    now + 500,
                    MagicEffect::Buff {
                        target_id: object_id,
                        buff,
                    },
                ));
                Some(BuffKind::Fury)
            }
            Spell::MagicShield => {
                // `DelayedAction(Magic, now + 500, magic,
                //      magic.GetPower(GetAttackPower(Stats[MinMC], Stats[MaxMC]) + 15))`（`:3591`）。
                // 抽签顺序：先 `GetAttackPower`，再 `GetPower` 里的 `DefPower`。
                let power = stats.attack_power(stats.min_mc, stats.max_mc, &mut self.rng) + 15;
                let seconds = magic.power_with_base(info, power, &mut self.rng).max(0);
                let duration = i64::from(seconds) * SECOND_MS;
                let mut delta = Stats::default();
                delta.set(Stat::DamageReductionPercent, (level + 2) * 10);
                let buff = PendingBuff::new(BuffKind::MagicShield, duration, delta);
                effects.push((
                    now + 500,
                    MagicEffect::Buff {
                        target_id: object_id,
                        buff,
                    },
                ));
                Some(BuffKind::MagicShield)
            }
            Spell::MagicBooster => {
                // `int bonus = 6 + magic.Level * 6;`（`:4347`），500ms 后
                // `AddBuff(BuffType.MagicBooster, this, Settings.Second * 60,
                //          { MinMC = bonus, MaxMC = bonus, ManaPenaltyPercent = 6 + Level })`（`:6230-6237`）。
                let bonus = 6 + level * 6;
                let mut delta = Stats::default();
                delta.set(Stat::MinMC, bonus);
                delta.set(Stat::MaxMC, bonus);
                delta.set(Stat::ManaPenaltyPercent, 6 + level);
                let buff = PendingBuff::new(BuffKind::MagicBooster, 60 * SECOND_MS, delta);
                effects.push((
                    now + 500,
                    MagicEffect::Buff {
                        target_id: object_id,
                        buff,
                    },
                ));
                Some(BuffKind::MagicBooster)
            }
            Spell::EnergyShield => {
                // `int duration = 30 + 50 * magic.Level;`
                // `int power = magic.GetPower(GetAttackPower(Stats[MinSC], Stats[MaxSC]));`
                // `int chance = (10 - (Stats[Stat.Luck] / 3 + magic.Level + 1)); if (chance < 2) chance = 2;`
                // `EnergyShieldPercent = round((1 / (decimal)chance) * 100)`（`:4757-4768`）。
                let base = stats.attack_power(stats.min_sc, stats.max_sc, &mut self.rng);
                let power = magic.power_with_base(info, base, &mut self.rng);
                let chance = (10 - (stats.luck / 3 + level + 1)).max(ENERGY_SHIELD_MIN_CHANCE);
                let percent = (100.0 / f64::from(chance)).round_ties_even() as i32;
                let duration = i64::from(30 + 50 * level) * SECOND_MS;
                let mut delta = Stats::default();
                delta.set(Stat::EnergyShieldPercent, percent);
                delta.set(Stat::EnergyShieldHPGain, power);
                // `if (target == null || target.Node == null || !target.IsFriendlyTarget(this)) target = this;`
                // （`:4755`）——没有目标就套在自己身上。
                let target = if target_id == 0 { object_id } else { target_id };
                self.add_buff(target, BuffKind::EnergyShield, duration, delta, object_id);
                self.level_magic(object_id, spell);
                Some(BuffKind::EnergyShield)
            }
            Spell::SoulShield | Spell::BlessedArmour => {
                // `GetAttackPower(Stats[MinSC], Stats[MaxSC]) * 4 + (magic.Level + 1) * 50`
                // 作为「秒」用（`HumanObject.cs:4521`），延迟 `MaxDistance * 50 + 500`（`:4519`）。
                let base = stats.attack_power(stats.min_sc, stats.max_sc, &mut self.rng);
                let seconds = base * 4 + (level + 1) * 50;
                let duration = i64::from(seconds.max(0)) * SECOND_MS;
                let delay = i64::from(position.max_distance(target_location)) * 50 + 500;
                let (kind, stat) = if spell == Spell::SoulShield {
                    (BuffKind::SoulShield, Stat::MaxMAC)
                } else {
                    (BuffKind::BlessedArmour, Stat::MaxAC)
                };
                effects.push((
                    now + delay,
                    MagicEffect::AreaBuff {
                        location: target_location,
                        kind,
                        duration_ms: duration,
                        stat,
                    },
                ));
                Some(kind)
            }
            _ => None,
        }
    }

    /// 攻击类技能（Bolt）能命中的怪物位置：必须按 Crystal
    /// `MonsterObject.IsAttackTarget(HumanObject)` 可攻击（宠物受主人/Peace/安全区限制）。
    fn attack_target_position(&self, map: &str, caster_id: u32, target_id: u32) -> Option<Point> {
        self.monsters
            .iter()
            .find(|monster| {
                monster.object_id == target_id
                    && !monster.dead
                    && monster.map == map
                    && self.monster_is_attack_target_of_player(caster_id, target_id)
            })
            .map(|monster| monster.position)
    }

    /// 攻击类技能（含玩家目标的 `Poisoning`）能命中的目标位置：
    /// 未死亡的同图怪物，或按 PK 规则可以攻击的玩家（Crystal `IsAttackTarget`）。
    fn magic_attack_position(&self, map: &str, caster_id: u32, target_id: u32) -> Option<Point> {
        if let Some(position) = self.attack_target_position(map, caster_id, target_id) {
            return Some(position);
        }
        self.players
            .get(&target_id)
            .filter(|player| {
                !player.dead
                    && player.map == map
                    && self.is_attack_target_player(caster_id, target_id)
            })
            .map(|player| player.position)
    }

    /// 结算到期的延迟魔法效果（Crystal `DelayedType.Magic`，`Map.cs:730-737` 的
    /// `ActionList` 循环 + `Map.CompleteMagic`/`HumanObject.CompleteMagic`）。
    pub(crate) fn resolve_pending_magic(&mut self, now: i64) {
        let mut index = 0;
        while index < self.pending_magic.len() {
            if self.pending_magic[index].at > now {
                index += 1;
                continue;
            }
            let action = self.pending_magic.remove(index);
            self.apply_pending_magic(action, now);
        }
    }

    /// 把一条延迟魔法效果落到具体对象上。
    fn apply_pending_magic(&mut self, action: PendingMagic, now: i64) {
        let Some(caster) = self.players.get(&action.caster_id) else {
            // 施法者已经下线：Crystal 的 `Despawn` 会清空 `ActionList`（`MapObject.cs:370`）。
            return;
        };
        let map = caster.map.clone();
        if !caster
            .magics
            .iter()
            .any(|magic| magic.spell == action.spell)
        {
            return;
        }
        match action.effect {
            MagicEffect::Damage {
                target_id,
                location,
                damage,
            } => {
                if self.magic_target_hittable(&map, target_id, location) {
                    let dealt = self.damage_magic_target(action.caster_id, target_id, damage);
                    if dealt > 0 {
                        self.level_magic(action.caster_id, action.spell);
                    }
                }
            }
            MagicEffect::AreaDamage { location, damage } => {
                let targets = self.monsters_in_area(&map, location);
                let mut trained = false;
                for target in targets {
                    if self.damage_magic_target(action.caster_id, target, damage) > 0 {
                        trained = true;
                    }
                }
                if trained {
                    self.level_magic(action.caster_id, action.spell);
                }
            }
            MagicEffect::HellFire {
                location,
                direction,
                damage,
                remaining,
            } => {
                let cell = location.moved(direction, 1);
                if !self.in_bounds(&map, cell) {
                    return;
                }
                if remaining > 1 {
                    // `if (count > 0) ActionList.Add(now + 100, ..., location, dir, count)`（`Map.cs:883-887`）。
                    self.pending_magic.push(PendingMagic {
                        at: now + HELLFIRE_STEP_MS,
                        caster_id: action.caster_id,
                        spell: action.spell,
                        effect: MagicEffect::HellFire {
                            location: cell,
                            direction,
                            damage,
                            remaining: remaining - 1,
                        },
                    });
                }
                let targets = self.monsters_on_cell(&map, cell);
                let mut trained = false;
                for target in targets {
                    if self.damage_magic_target(action.caster_id, target, damage) > 0 {
                        trained = true;
                    }
                }
                if trained {
                    self.level_magic(action.caster_id, action.spell);
                }
            }
            MagicEffect::Heal { target_id, amount } => {
                let healed = self.queue_heal(target_id, &map, amount);
                if healed {
                    self.level_magic(action.caster_id, action.spell);
                }
            }
            MagicEffect::AreaHeal { location, amount } => {
                let targets = self.players_in_area(&map, location);
                let mut trained = false;
                for target in targets {
                    if self.queue_heal(target, &map, amount) {
                        trained = true;
                    }
                }
                if trained {
                    self.level_magic(action.caster_id, action.spell);
                }
            }
            MagicEffect::Buff { target_id, buff } => {
                // `CompleteMagic` 的增益分支：`AddBuff(...)` 之后 `LevelMagic(magic)`
                // （`HumanObject.cs:6151-6153`、`:6202-6203` 等）。
                if self.apply_pending_buff(target_id, action.caster_id, &buff) {
                    self.level_magic(action.caster_id, action.spell);
                }
            }
            MagicEffect::AreaBuff {
                location,
                kind,
                duration_ms,
                stat,
            } => {
                // `Map.cs:1052-1095`：落点 ±3 的 7×7，逐个友方 `AddBuff`，
                // 加成是**受术者**的 `Level / 7 + 4`（`:1082`）。
                let targets = self.area_buff_targets(&map, location);
                let mut trained = false;
                for (target_id, level) in targets {
                    let mut delta = Stats::default();
                    delta.set(stat, i32::from(level) / 7 + 4);
                    if self.add_buff(target_id, kind, duration_ms, delta, action.caster_id) {
                        trained = true;
                    }
                }
                if trained {
                    self.level_magic(action.caster_id, action.spell);
                }
            }
            MagicEffect::FireWall {
                map: field_map,
                location,
                damage,
                cast_id,
            } => {
                // `Map.cs:1101-1183`：先 `LevelMagic`（`:1110`，落点非法也升级），
                // 再在落点 + 上/右/下/左铺 5 格（`:1145-1149`）。
                self.level_magic(action.caster_id, action.spell);
                self.create_fire_wall(action.caster_id, &field_map, location, damage, cast_id, now);
            }
            MagicEffect::Lightning {
                origin,
                direction,
                damage,
            } => {
                if self.resolve_lightning(action.caster_id, &map, origin, direction, damage) {
                    self.level_magic(action.caster_id, action.spell);
                }
            }
            MagicEffect::Teleport => {
                if self.teleport_caster(action.caster_id) {
                    self.level_magic(action.caster_id, action.spell);
                }
            }
            MagicEffect::Hiding { duration_ms } => {
                self.start_hiding(action.caster_id, duration_ms, now);
                self.level_magic(action.caster_id, action.spell);
            }
            MagicEffect::Summon { spell } => {
                if self.summon_pet(action.caster_id, spell).is_some() {
                    self.level_magic(action.caster_id, action.spell);
                }
            }
            MagicEffect::Poison { target_id, power } => {
                if self.apply_poisoning(action.caster_id, &map, target_id, power, action.spell, now)
                {
                    self.level_magic(action.caster_id, action.spell);
                }
            }
        }
    }

    /// 目标是否还能吃这一发单体魔法：同图、未死亡、且还在施法落点 ±2 格内
    /// （`HumanObject.cs:5928`）。
    fn magic_target_hittable(&self, map: &str, target_id: u32, location: Point) -> bool {
        // 只查落点 ±MAGIC_HIT_TOLERANCE 内的怪物（Crystal `HumanObject.cs:5928`），
        // 用 AOI 网格代替遍历全部怪物。
        self.aoi_ids_in_radius(map, location, MAGIC_HIT_TOLERANCE, AoiKind::Monster)
            .into_iter()
            .any(|object_id| {
                object_id == target_id
                    && self
                        .monster(object_id)
                        .is_some_and(|monster| !monster.dead && monster.map == map)
            })
    }

    /// 对一只怪结算一次魔法伤害，返回实际扣血值。
    ///
    /// 走 `DefenceType::MAC`（Crystal `target.Attacked(this, value, DefenceType.MAC, false)`，
    /// `HumanObject.cs:5929`），因此魔法闪避（`Stat.MagicResist`）与魔法防御
    /// （`MinMAC/MaxMAC`）参与判定，暴击仍按攻方属性抽签（`MonsterObject.cs:2594-2599`）。
    fn damage_magic_target(&mut self, caster_id: u32, target_id: u32, damage: i32) -> i32 {
        let Some(stats) = self.players.get(&caster_id).map(|player| player.stats) else {
            return 0;
        };
        let hit = PendingHit {
            at: 0,
            attacker_id: caster_id,
            target_id,
            damage,
            defence: DefenceType::MAC,
        };
        self.damage_monster(hit, &stats)
    }

    /// 3×3 区域里的怪物（`Map.cs:956-987`：外层 y、内层 x，逐格取对象）。
    fn monsters_in_area(&self, map: &str, center: Point) -> Vec<u32> {
        let mut targets = Vec::new();
        for dy in -AREA_RADIUS..=AREA_RADIUS {
            for dx in -AREA_RADIUS..=AREA_RADIUS {
                let cell = Point::new(center.x + dx, center.y + dy);
                if !self.in_bounds(map, cell) {
                    continue;
                }
                targets.extend(self.monsters_on_cell(map, cell));
            }
        }
        targets
    }

    /// 某一格上的全部存活怪物（生成顺序；用 AOI 网格查单格）。
    fn monsters_on_cell(&self, map: &str, cell: Point) -> Vec<u32> {
        self.aoi_ids_in_cell(map, cell, AoiKind::Monster)
            .into_iter()
            .filter(|object_id| {
                self.monster(*object_id)
                    .is_some_and(|monster| !monster.dead && monster.position == cell)
            })
            .collect()
    }

    /// 群体增益（`MagicEffect::AreaBuff`）的作用目标：同图、未死亡、在落点
    /// `±SHIELD_AREA_RADIUS` 内的玩家，按对象 ID 升序（= 旧暴力扫描后 `sort_unstable`
    /// 的结果，保证增益包序可回放）。
    ///
    /// Crystal `Map.cs:1052-1095` 是外层 y、内层 x 逐格取对象；本作用 AOI 网格取
    /// 落点半径内的玩家（`aoi_ids_in_radius`，等价 7×7 扫描），判定阈值不变。
    fn area_buff_targets(&self, map: &str, location: Point) -> Vec<(u32, u16)> {
        let mut targets: Vec<(u32, u16)> = self
            .aoi_ids_in_radius(map, location, SHIELD_AREA_RADIUS, AoiKind::Player)
            .into_iter()
            .filter_map(|object_id| {
                self.players
                    .get(&object_id)
                    .filter(|player| player.map == map && !player.dead)
                    .map(|player| (player.object_id, player.level))
            })
            .collect();
        targets.sort_unstable();
        targets
    }

    /// 3×3 区域里的玩家（`Map.cs:1259-1290`），按对象 ID 升序保证可回放。
    fn players_in_area(&self, map: &str, center: Point) -> Vec<u32> {
        let mut targets: Vec<u32> = self
            .aoi_ids_in_radius(map, center, AREA_RADIUS, AoiKind::Player)
            .into_iter()
            .filter(|object_id| {
                self.players
                    .get(object_id)
                    .is_some_and(|player| !player.dead && player.map == map)
            })
            .collect();
        targets.sort_unstable();
        targets
    }

    /// 把治疗量记到目标身上：`target.HealAmount = min(ushort.MaxValue, HealAmount + value)`
    /// （`HumanObject.cs:6024`、`Map.cs:1281`）。
    ///
    /// 返回是否真的记上了（满血/死亡/不同图都算失败，Crystal 满血直接 `continue`）。
    fn queue_heal(&mut self, target_id: u32, map: &str, amount: i32) -> bool {
        if amount <= 0 {
            return false;
        }
        let Some(player) = self.players.get_mut(&target_id) else {
            return false;
        };
        if player.dead || player.map != map || player.hp >= player.stats.hp {
            return false;
        }
        player.magic.heal_amount = player
            .magic
            .heal_amount
            .saturating_add(u32::try_from(amount).unwrap_or(0))
            .min(u32::from(u16::MAX));
        true
    }

    /// 取走本 tick 的「治疗分段」：Crystal `HumanObject.ProcessRegen` 的 `HealAmount` 块
    /// （`HumanObject.cs:602-617`）。
    ///
    /// ```csharp
    /// if (Envir.Time > HealTime) {
    ///     HealTime = Envir.Time + HealDelay;                 // 600ms
    ///     int incHeal = (Level / 10) + (HealAmount / 10);
    ///     if (HealAmount > (5 + incHeal)) {
    ///         healthRegen += (5 + incHeal);
    ///         HealAmount -= (5 + incHeal);
    ///     } else {
    ///         healthRegen += HealAmount;
    ///         HealAmount = 0;
    ///     }
    /// }
    /// ```
    ///
    /// 返回值会并进调用方（[`World`] 的自然回复）本 tick 的 `healthRegen`，一起经
    /// `ChangeHP` 生效并发 `S.HealthChanged`（Crystal 同样是先累加、最后统一 `ChangeHP`，
    /// `:635-639`）。
    pub(crate) fn take_heal_chunk(&mut self, object_id: u32, now: i64) -> i32 {
        let Some(player) = self.players.get_mut(&object_id) else {
            return 0;
        };
        if player.magic.heal_amount == 0 || now <= player.magic.heal_time {
            return 0;
        }
        player.magic.heal_time = now + HEAL_DELAY_MS;
        let level = i32::from(player.level);
        let heal_amount = i32::try_from(player.magic.heal_amount).unwrap_or(i32::MAX);
        let inc_heal = level / 10 + heal_amount / 10;
        let chunk = 5 + inc_heal;
        let taken = heal_amount.min(chunk);
        player.magic.heal_amount = u32::try_from(heal_amount - taken).unwrap_or(0);
        taken
    }

    /// 技能经验增长与升级（Crystal `HumanObject.LevelMagic`，`HumanObject.cs:6903-6980`）。
    ///
    /// 流程：
    ///
    /// ```csharp
    /// byte exp = (byte)(Envir.Random.Next(3) + 1);                 // :6905
    /// exp *= min(255, Stats[Stat.SkillGainMultiplier]);            // :6924（本作恒为 1）
    /// if (Level == ushort.MaxValue) exp = byte.MaxValue;           // :6926
    /// switch (magic.Level) {
    ///   case 0: if (Level < Info.Level1) return; exp += ...; >= Need1 → Level++  // :6932-6943
    ///   case 1: if (Level < Info.Level2) return; ... >= Need2 → Level++          // :6944-6955
    ///   case 2: if (Level < Info.Level3) return; ... >= Need3 → Level++、exp=0   // :6956-6967
    ///   default: return;                                                          // :6968-6969
    /// }
    /// if (oldLevel != magic.Level) Enqueue(S.MagicDelay { Delay = magic.GetDelay() }); // :6972-6976
    /// Enqueue(S.MagicLeveled { Spell, Level, Experience });                            // :6978
    /// ```
    ///
    /// 注意两点（都是 Crystal 的原样行为）：
    /// * 角色等级不够时**连 `S.MagicLeveled` 都不发**（`:6933-6934` 的提前 return）；
    /// * 升级不消耗额外资源，也不改角色属性——`RefreshStats()`（`:6941`）在本作里是空操作，
    ///   因为没有依赖技能等级的属性。
    pub(crate) fn level_magic(&mut self, object_id: u32, spell: Spell) {
        // `Envir.Random.Next(3) + 1`（`:6905`）。
        let roll = u8::try_from(self.rng.next_bounded(u32::from(MAGIC_EXP_ROLL_MAX)))
            .unwrap_or(0)
            .saturating_add(1);
        let Some(player) = self.players.get(&object_id) else {
            return;
        };
        let Some(index) = player.magics.iter().position(|magic| magic.spell == spell) else {
            return;
        };
        let Some(info) = self.magic_info(spell).cloned() else {
            return;
        };
        let char_level = player.level;
        let mut magic = player.magics[index];
        // `if (Level == ushort.MaxValue) exp = byte.MaxValue;`（`:6926`）。
        let exp = if char_level == u16::MAX {
            u32::from(u8::MAX)
        } else {
            u32::from(roll)
        };
        let old_level = magic.level;
        let (required_level, need) = match magic.level {
            0 => (info.level1, u32::from(info.need1)),
            1 => (info.level2, u32::from(info.need2)),
            2 => (info.level3, u32::from(info.need3)),
            _ => return,
        };
        // `if (Level < magic.Info.Level1) return;`（`:6933`）。
        if char_level < u16::from(required_level) {
            return;
        }
        magic.experience = magic.experience.saturating_add(exp);
        if magic.experience >= need {
            magic.level = magic.level.saturating_add(1);
            // 0/1 级扣掉本次所需修炼值，2 级升满后清零（`:6940`、`:6952`、`:6964`）。
            magic.experience = if magic.level >= MAGIC_LEVEL_MAX {
                0
            } else {
                magic.experience - need
            };
        }
        if let Some(player) = self.players.get_mut(&object_id) {
            if let Some(slot) = player.magics.get_mut(index) {
                *slot = magic;
            }
        }
        // `if (oldLevel != magic.Level) { Enqueue(new S.MagicDelay {...}); }`（`:6972-6976`）。
        if old_level != magic.level {
            self.send_to_player(
                object_id,
                &ServerPacket::MagicDelay(MagicDelay {
                    object_id,
                    spell: magic.spell,
                    delay: magic.delay_ms(&info),
                })
                .encode_frame(),
            );
        }
        // `Enqueue(new S.MagicLeveled {...});`（`:6978`）。
        self.send_to_player(
            object_id,
            &ServerPacket::MagicLeveled(MagicLeveled {
                object_id,
                spell: magic.spell,
                level: magic.level,
                experience: u16::try_from(magic.experience).unwrap_or(u16::MAX),
            })
            .encode_frame(),
        );
    }

    /// 火墙施法瞬间的上限检查（Crystal `HumanObject.cs:4151-4175`）。
    ///
    /// `FireWallLimit` 为真且 `FireWallCount > 0` 时，按**施法实例**（`CastInstanceId`）
    /// 统计当前仍在场上的火墙；达到上限则回收最老一次施法的全部格子
    /// （`TryReplaceOldestFireWallCast`，`:4183-4207`）；没有可回收的才返回 `false`
    /// （调用方据此不排延迟动作，但蓝/冷却/`S.Magic` 已经发生，与 Crystal 一致）。
    fn can_cast_fire_wall(&mut self, map: &str, caster_id: u32) -> bool {
        let Some((true, count)) = self
            .game_data()
            .and_then(|data| data.map_info(map))
            .map(|info| (info.firewall_limit, info.firewall_count))
        else {
            return true;
        };
        if count <= 0 {
            return true;
        }
        let mut casts: Vec<u32> = self
            .spell_fields
            .iter()
            .filter(|field| {
                field.map == map && field.spell == Spell::FireWall && field.caster_id == caster_id
            })
            .map(|field| field.cast_id)
            .collect();
        casts.sort_unstable();
        casts.dedup();
        if casts.len() < usize::try_from(count).unwrap_or(usize::MAX) {
            return true;
        }
        if self.recycle_oldest_fire_wall(map, caster_id) {
            true
        } else {
            let message = self.text(caster_id, TextKey::MagicFirewallLimit);
            self.send_system_chat(caster_id, &message);
            false
        }
    }

    /// 铺一片火墙（Crystal `Map.cs:1101-1183` + `SpellObject` 构造）。
    ///
    /// 落点 + 上/右/下/左共 5 格（`:1145-1149`），每格独立到期（`ExpireTime =
    /// now + (10 + damage / 2) * 1000`，`:1133`）、独立 tick（`TickSpeed = 2000`，`:1134`）。
    /// 同格已有火墙则跳过（`:1116-1126`、`:1154-1166`）。
    ///
    /// 每格都先做 `ValidPoint`（`:1112-1113`、`:1151`）：边界 + 地形可走
    /// （`Map.cs:637-640`），本作对应 [`World::walkable_on`]。
    fn create_fire_wall(
        &mut self,
        caster_id: u32,
        map: &str,
        location: Point,
        damage: i32,
        cast_id: u32,
        now: i64,
    ) {
        let expire_at = now + (FIREWALL_BASE_SECONDS + i64::from(damage.max(0) / 2)) * SECOND_MS;
        let mut direction = MirDirection::Up;
        for index in 0..FIREWALL_CELLS {
            let cell = if index == 0 {
                location
            } else {
                direction = if index == 1 {
                    MirDirection::Up
                } else {
                    rotate_direction(direction, 2)
                };
                location.moved(direction, 1)
            };
            if !self.walkable_on(map, cell) {
                continue;
            }
            let occupied = self.spell_fields.iter().any(|field| {
                field.map == map
                    && field.spell == Spell::FireWall
                    && field.radius == 0
                    && field.center == cell
            });
            if occupied {
                continue;
            }
            let object_id = self.allocate_object_id();
            let frame = ServerPacket::ObjectSpell(ObjectSpell {
                object_id,
                location: cell,
                spell: Spell::FireWall,
                direction: MirDirection::Up,
                param: false,
            })
            .encode_frame();
            self.broadcast(map, cell, None, &frame, object_id);
            self.spell_fields.push(SpellField {
                object_id,
                map: map.to_owned(),
                center: cell,
                radius: 0,
                damage,
                caster_id,
                expire_at,
                // Crystal 的 `TickTime` 初值 0 → 首跳立即（`SpellObject.cs:111-112`）。
                next_tick: now,
                tick_ms: FIREWALL_TICK_MS,
                spell: Spell::FireWall,
                direction: MirDirection::Up,
                cast_id,
            });
        }
    }

    /// 回收最老的一次火墙施法（Crystal `TryReplaceOldestFireWallCast`，
    /// `HumanObject.cs:4183-4207`）。
    fn recycle_oldest_fire_wall(&mut self, map: &str, caster_id: u32) -> bool {
        let Some(oldest) = self
            .spell_fields
            .iter()
            .filter(|field| {
                field.map == map && field.spell == Spell::FireWall && field.caster_id == caster_id
            })
            .map(|field| field.cast_id)
            .min()
        else {
            return false;
        };
        let mut index = 0;
        while index < self.spell_fields.len() {
            let field = &self.spell_fields[index];
            if field.map == map && field.caster_id == caster_id && field.cast_id == oldest {
                let packet = ServerPacket::ObjectRemove(ObjectRemove {
                    object_id: field.object_id,
                })
                .encode_frame();
                let (field_map, center, object_id) =
                    (field.map.clone(), field.center, field.object_id);
                self.broadcast(&field_map, center, None, &packet, object_id);
                self.spell_fields.remove(index);
                continue;
            }
            index += 1;
        }
        true
    }

    /// 每 tick 推进持续法术场：到期/施法者换图或下线就移除；到点用 AOI 结算伤害
    /// （Crystal `SpellObject.Process`，`SpellObject.cs:51-122`）。
    pub(crate) fn tick_spell_fields(&mut self, now: i64) {
        let mut index = 0;
        while index < self.spell_fields.len() {
            let field = self.spell_fields[index].clone();
            let caster_gone = self
                .players
                .get(&field.caster_id)
                .is_none_or(|caster| caster.map != field.map);
            if now >= field.expire_at || caster_gone {
                let packet = ServerPacket::ObjectRemove(ObjectRemove {
                    object_id: field.object_id,
                })
                .encode_frame();
                self.broadcast(&field.map, field.center, None, &packet, field.object_id);
                self.spell_fields.remove(index);
                continue;
            }
            if now >= field.next_tick {
                self.spell_fields[index].next_tick = now + field.tick_ms;
                self.apply_fire_wall_tick(&field);
            }
            index += 1;
        }
    }

    /// 火墙一次 tick 的伤害（Crystal `SpellObject.ProcessSpell` 的 `FireWall` 分支，
    /// `SpellObject.cs:128-135`）：同格内玩家/怪物，`DefenceType.MAC`。
    fn apply_fire_wall_tick(&mut self, field: &SpellField) {
        let Some(caster_stats) = self
            .players
            .get(&field.caster_id)
            .map(|caster| caster.stats)
        else {
            return;
        };
        // 半径 0 = 只烧中心格；用 AOI 查这一格（Crystal `GetCell(...).Objects`，`SpellObject.cs:114-119`）。
        for entry in self.aoi_in_radius(&field.map, field.center, field.radius) {
            match entry.kind {
                AoiKind::Player => {
                    let target_id = entry.object_id;
                    if target_id == field.caster_id {
                        continue;
                    }
                    let alive = self.players.get(&target_id).is_some_and(|player| {
                        !player.dead && player.map == field.map && player.position == field.center
                    });
                    if !alive || !self.is_attack_target_player(field.caster_id, target_id) {
                        continue;
                    }
                    self.damage_player_from_player(
                        PendingHit {
                            at: 0,
                            attacker_id: field.caster_id,
                            target_id,
                            damage: field.damage,
                            defence: DefenceType::MAC,
                        },
                        &caster_stats,
                    );
                }
                AoiKind::Monster => {
                    let target_id = entry.object_id;
                    let hittable = self.monster(target_id).is_some_and(|monster| {
                        !monster.dead
                            && monster.map == field.map
                            && monster.position == field.center
                            && monster.master != Some(field.caster_id)
                    });
                    if hittable {
                        self.damage_magic_target(field.caster_id, target_id, field.damage);
                    }
                }
                AoiKind::Npc | AoiKind::Item => {}
            }
        }
    }

    /// 雷电结算（Crystal `Map.CompleteMagic` 的 `Lightning` 分支，`Map.cs:1187-1218`）：
    /// 从施法者位置沿方向推进最多 6 格，每格命中**第一个**可攻击对象（内层 `break`），
    /// 然后继续下一格；`ValidPoint` 失败或空格只是 `continue`，**没有遇阻停止**。
    ///
    /// 返回是否至少打出了有效伤害（决定 `LevelMagic`，`:1210-1211`）。
    fn resolve_lightning(
        &mut self,
        caster_id: u32,
        map: &str,
        origin: Point,
        direction: MirDirection,
        damage: i32,
    ) -> bool {
        let Some(caster_stats) = self.players.get(&caster_id).map(|caster| caster.stats) else {
            return false;
        };
        let mut trained = false;
        let mut location = origin;
        for _ in 0..LIGHTNING_RANGE {
            location = location.moved(direction, 1);
            // Crystal `ValidPoint` = 边界 + 地形可走（`Map.cs:637-640`）；失败只是 continue。
            if !self.walkable_on(map, location) {
                continue;
            }
            // 单格对象按 (y, x, 对象 ID) 排序，等价 Crystal `cell.Objects` 的加入顺序。
            for entry in self.aoi_in_cell(map, location) {
                match entry.kind {
                    AoiKind::Monster => {
                        let target_id = entry.object_id;
                        let attackable = self.monster(target_id).is_some_and(|monster| {
                            !monster.dead
                                && monster.map == map
                                && monster.position == location
                                && !monster.kind.is_guard()
                        });
                        if !attackable {
                            continue;
                        }
                        if self.damage_magic_target(caster_id, target_id, damage) > 0 {
                            trained = true;
                        }
                        break;
                    }
                    AoiKind::Player => {
                        let target_id = entry.object_id;
                        let attackable = self.players.get(&target_id).is_some_and(|player| {
                            !player.dead
                                && player.map == map
                                && player.position == location
                                && target_id != caster_id
                        });
                        if !attackable || !self.is_attack_target_player(caster_id, target_id) {
                            continue;
                        }
                        let before = self.players.get(&target_id).map_or(0, |player| player.hp);
                        self.damage_player_from_player(
                            PendingHit {
                                at: 0,
                                attacker_id: caster_id,
                                target_id,
                                damage,
                                defence: DefenceType::MAC,
                            },
                            &caster_stats,
                        );
                        let after = self.players.get(&target_id).map_or(0, |player| player.hp);
                        if after < before {
                            trained = true;
                        }
                        break;
                    }
                    AoiKind::Npc | AoiKind::Item => {}
                }
            }
        }
        trained
    }

    /// 瞬息移动（Crystal `PlayerObject.MagicTeleport`，`PlayerObject.cs:4633-4648`）。
    ///
    /// **偏差**：Crystal 以 `BindMapIndex` + `BindLocation` 为中心；本作的 `Player` 没有
    /// 建模 bind 点（`session.rs` 归户部），改用**当前地图 + 当前坐标**为中心，
    /// 矩形半径仍是 `Width / (Level + 1)`、`Height / (Level + 1)`，最多 200 次，
    /// 只校验可走点（`MapObject.cs:780-782`）。
    ///
    /// 返回是否成功传送；成功时加 30 秒传送法力惩罚（`HumanObject.cs:6106`）。
    fn teleport_caster(&mut self, caster_id: u32) -> bool {
        let Some((map, position, direction, level)) = self.players.get(&caster_id).map(|player| {
            (
                player.map.clone(),
                player.position,
                player.direction,
                player.level,
            )
        }) else {
            return false;
        };
        if self
            .game_data()
            .and_then(|data| data.map_info(&map))
            .is_some_and(|info| info.no_teleport)
        {
            let message = self.text(caster_id, TextKey::MagicTeleportForbidden);
            self.send_system_chat(caster_id, &message);
            return false;
        }
        let (max_x, max_y) = self.map_bounds(&map);
        let divisor = i32::from(level) + 1;
        let size_x = ((max_x + 1) / divisor).max(1);
        let size_y = ((max_y + 1) / divisor).max(1);
        let mut landing = None;
        for _ in 0..TELEPORT_ATTEMPTS {
            let cell = Point::new(
                position.x + self.rng.next_range(-size_x, size_x),
                position.y + self.rng.next_range(-size_y, size_y),
            );
            if self.walkable_on(&map, cell) {
                landing = Some(cell);
                break;
            }
        }
        let Some(landing) = landing else {
            return false;
        };
        let now = self.now();
        let out = ServerPacket::ObjectTeleportOut(ObjectTeleportOut {
            object_id: caster_id,
            type_: 0,
        })
        .encode_frame();
        self.broadcast(&map, position, Some(caster_id), &out, caster_id);
        if self
            .change_map(caster_id, &map, landing, direction)
            .is_err()
        {
            return false;
        }
        let enter = ServerPacket::ObjectTeleportIn(ObjectTeleportIn {
            object_id: caster_id,
            type_: 0,
        })
        .encode_frame();
        self.broadcast(&map, landing, Some(caster_id), &enter, caster_id);
        if let Some(player) = self.players.get_mut(&caster_id) {
            player.magic.teleport_penalty_until = now + TELEPORT_PENALTY_MS;
        }
        true
    }

    /// 进入隐身（Crystal `AddBuff(BuffType.Hiding, ...)` + `MapObject.Hidden` setter，
    /// `HumanObject.cs:7460-7488`、`MapObject.cs:80-92`、`:662-666`）。
    ///
    /// 包序：
    /// 1. `S.ObjectHidden{true}` 广播给周围玩家（`Hidden` setter，`MapObject.cs:90`；
    ///    `Broadcast` 不发给本人，`MapObject.cs:408-420`）；
    /// 2. `S.AddBuff` **只发本人**（`Hiding` 的 `BuffInfo.Visible == false`，
    ///    `BuffInfo.cs:20`、`HumanObject.cs:7474-7481`）；
    /// 3. 让范围内正锁定自己的怪脱战（`HideFromTargets`，`MapObject.cs:737-763`）。
    fn start_hiding(&mut self, player_id: u32, duration_ms: i64, now: i64) {
        let Some((map, position, level, already_hidden)) =
            self.players.get(&player_id).map(|player| {
                (
                    player.map.clone(),
                    player.position,
                    player.level,
                    player.magic.hidden,
                )
            })
        else {
            return;
        };
        if let Some(player) = self.players.get_mut(&player_id) {
            player.magic.hidden = true;
            player.magic.hidden_until = now + duration_ms.max(0);
        }
        // 已经隐身：`BuffStackType.ResetDuration` 只重置时长，不再重发包
        // （`MapObject.cs:591-595`；`Hidden` setter 在值未变时也直接 return，`MapObject.cs:88`）。
        if already_hidden {
            return;
        }
        let frame = ServerPacket::ObjectHidden(ObjectHidden {
            object_id: player_id,
            hidden: true,
        })
        .encode_frame();
        self.broadcast(&map, position, Some(player_id), &frame, player_id);
        // `HideFromTargets`：`if (ob.Target == this && (!ob.CoolEye || ob.Level < Level))
        //     ob.Target = null;`（`MapObject.cs:754-760`）。
        for monster_id in self.aoi_ids_in_radius(&map, position, VIEW_RANGE, AoiKind::Monster) {
            let Some(monster) = self.monster_mut(monster_id) else {
                continue;
            };
            if monster.target == Some(player_id)
                && (!monster.cool_eye || monster.kind.level < level)
            {
                monster.target = None;
            }
        }
        let buff = ClientBuff {
            buff_type: BuffType::Hiding,
            visible: false,
            object_id: player_id,
            expire_time: duration_ms.max(0),
            infinite: false,
            paused: false,
            stats: Stats::default(),
            values: Vec::new(),
        };
        self.send_to_player(
            player_id,
            &ServerPacket::AddBuff(AddBuff { buff }).encode_frame(),
        );
    }

    /// 解除隐身（Crystal `RemoveBuff(BuffType.Hiding)` → `Hidden = false` 广播，
    /// `HumanObject.cs:462-468`、`MapObject.cs:672-694`）。
    pub(crate) fn remove_hiding(&mut self, player_id: u32) {
        let Some((map, position, hidden)) = self
            .players
            .get(&player_id)
            .map(|player| (player.map.clone(), player.position, player.magic.hidden))
        else {
            return;
        };
        if !hidden {
            return;
        }
        if let Some(player) = self.players.get_mut(&player_id) {
            player.magic.hidden = false;
            player.magic.hidden_until = 0;
        }
        let frame = ServerPacket::ObjectHidden(ObjectHidden {
            object_id: player_id,
            hidden: false,
        })
        .encode_frame();
        self.broadcast(&map, position, Some(player_id), &frame, player_id);
        // 本人收到 `S.RemoveBuff` 清掉客户端 buff 列表（`HumanObject.cs:452-458`）。
        self.send_to_player(
            player_id,
            &ServerPacket::RemoveBuff(RemoveBuff {
                type_: BuffType::Hiding,
                object_id: player_id,
            })
            .encode_frame(),
        );
    }

    /// 隐身到期（Crystal `ProcessBuffs` 里 `BuffType.Hiding` 到期 → `Hidden = false`，
    /// `HumanObject.cs:462-468`）。
    pub(crate) fn tick_player_hiding(&mut self, player_id: u32, now: i64) {
        let expired = self
            .players
            .get(&player_id)
            .is_some_and(|player| player.magic.hidden && now >= player.magic.hidden_until);
        if expired {
            self.remove_hiding(player_id);
        }
    }

    /// 召唤骷髅/神兽（Crystal `SummonSkeleton`/`SummonShinsu` + `Map.CompleteMagic`，
    /// `HumanObject.cs:4399-4488`、`Map.cs:914-933`）。
    ///
    /// 规则：
    /// * 地图 `NoPets` → 拒绝（`:4401-4405`、`:4450-4454`）；
    /// * 同名活宠 → 只召回、不重复召唤（`:4408-4415`、`:4457-4464`）；
    /// * 存活的怪物类召唤物最多 2 只（`:4417`、`:4466`；Crystal 的 `Pets` 列表含未移除的尸体，
    ///   本作在召唤物死亡时就从索引摘掉，所以口径是「存活上限 2」）；
    /// * 落点 Front 不合法时落主人脚下；主人已死则放弃（`Map.cs:925-932`）。
    ///
    /// 简化：护身符消耗未实现（Crystal 骷髅 1 符、神兽 5 符，`:4419-4426`、`:4468-4475`）。
    fn summon_pet(&mut self, caster_id: u32, spell: Spell) -> Option<u32> {
        let (map, position, direction, pet_level, master_dead) =
            self.players.get(&caster_id).map(|player| {
                let level = self
                    .player_magic(caster_id, spell)
                    .map_or(0, |magic| magic.level);
                (
                    player.map.clone(),
                    player.position,
                    player.direction,
                    level,
                    player.dead,
                )
            })?;
        // `Map.cs:925`：`if (monster.Master.Dead) return;`——500ms 内主人死了就不召唤。
        if master_dead {
            return None;
        }
        if self
            .game_data()
            .and_then(|data| data.map_info(&map))
            .is_some_and(|info| info.no_pets)
        {
            let message = self.text(caster_id, TextKey::MagicSummonForbidden);
            self.send_system_chat(caster_id, &message);
            return None;
        }
        // 同名活宠：只召回（Crystal 比较 `Settings.SkeletonName`/`ShinsuName`）。
        if let Some(pet_id) = self.pets_of(caster_id).into_iter().find(|pet_id| {
            self.monster(*pet_id)
                .is_some_and(|pet| !pet.dead && pet.summon_spell == Some(spell))
        }) {
            self.recall_pet(pet_id);
            return None;
        }
        if self.pets_of(caster_id).len() >= MAX_MONSTER_PETS {
            return None;
        }
        let (template_name, fallback, max_pet_level) = match spell {
            Spell::SummonSkeleton => ("BoneFamiliar", MonsterKind::bone_familiar(), 4 + pet_level),
            Spell::SummonShinsu => ("Shinsu", MonsterKind::shinsu(), 1 + pet_level * 2),
            _ => return None,
        };
        let kind = self
            .game_data()
            .and_then(|data| data.monster_info_by_name(template_name))
            .map_or(fallback, crate::gamedata::kind_from_monster_info);
        let front = position.moved(direction, 1);
        let at = if self.walkable_on(&map, front) {
            front
        } else {
            position
        };
        Some(self.spawn_pet(&map, kind, at, caster_id, pet_level, max_pet_level, spell))
    }

    /// 施毒落地（Crystal `CompleteMagic` 的 `Poisoning` 分支，`HumanObject.cs:6041-6077`）。
    ///
    /// **本作玩法决策**：Crystal 由装备栏毒药物品的 `Shape`（1 绿 / 2 红）决定毒型
    /// （`:6050-6070`）；本作没有毒药消耗品，改为「目标没有绿毒就施绿毒，已经有绿毒就施红毒」，
    /// 两段都能在测试里确定性复现。绿毒公式逐字照抄：
    /// `Duration = power * 2 + (Level + 1) * 7`、`Value = power / 15 + Level + 1`
    /// （`PoisonAttack` 属性本作没有，省略该项）。
    fn apply_poisoning(
        &mut self,
        caster_id: u32,
        map: &str,
        target_id: u32,
        power: i32,
        spell: Spell,
        now: i64,
    ) -> bool {
        if self
            .magic_attack_position(map, caster_id, target_id)
            .is_none()
        {
            return false;
        }
        let level = i32::from(
            self.player_magic(caster_id, spell)
                .map_or(0, |magic| magic.level),
        );
        let duration = u32::try_from(power.max(0) * 2 + (level + 1) * 7).unwrap_or(u32::MAX);
        let (has_green, has_red) = if let Some(monster) = self.monster(target_id) {
            (
                monster.poisons.iter().any(|p| p.kind == PoisonType::Green),
                monster.poisons.iter().any(|p| p.kind == PoisonType::Red),
            )
        } else if let Some(player) = self.players.get(&target_id) {
            (
                player
                    .magic
                    .poisons
                    .iter()
                    .any(|p| p.kind == PoisonType::Green),
                player
                    .magic
                    .poisons
                    .iter()
                    .any(|p| p.kind == PoisonType::Red),
            )
        } else {
            return false;
        };
        let poison = if has_green && !has_red {
            PoisonState::new(PoisonType::Red, 0, duration, now, caster_id)
        } else {
            let value = power / 15 + level + 1;
            PoisonState::new(PoisonType::Green, value, duration, now, caster_id)
        };
        let applied = if self.monster(target_id).is_some() {
            self.apply_poison_to_monster(target_id, poison)
        } else {
            self.apply_poison_to_player(target_id, poison)
        };
        if applied {
            // `target.OperateTime = 0;`（`:6072`）：中毒的怪立刻重新思考。
            if let Some(monster) = self.monster_mut(target_id) {
                monster.operate_time = 0;
                if monster.target.is_none() {
                    monster.target = Some(caster_id);
                }
            }
        }
        // `LevelMagic(magic)` 在目标有效时总会执行（`HumanObject.cs:6074`），
        // 即使新毒被「更强/更久的旧毒」挡住。
        true
    }

    /// 给怪物上毒（Crystal `MonsterObject.ApplyPoison`，`MonsterObject.cs:2782-2836`）。
    ///
    /// 同类型毒：绿毒只接受更高伤害、红毒只接受更长时长，否则忽略（`:2805-2813`）。
    pub(crate) fn apply_poison_to_monster(&mut self, target_id: u32, poison: PoisonState) -> bool {
        let Some((map, position)) = self
            .monster(target_id)
            .map(|monster| (monster.map.clone(), monster.position))
        else {
            return false;
        };
        let Some(monster) = self.monster_mut(target_id) else {
            return false;
        };
        if monster.dead {
            return false;
        }
        if let Some(existing) = monster
            .poisons
            .iter_mut()
            .find(|existing| existing.kind == poison.kind)
        {
            if poison.kind == PoisonType::Green && existing.value > poison.value {
                return false;
            }
            if poison.kind != PoisonType::Green
                && existing.duration.saturating_sub(existing.ticks) > poison.duration
            {
                return false;
            }
            *existing = poison;
        } else {
            monster.poisons.push(poison);
        }
        self.sync_monster_poison(&map, target_id, position);
        true
    }

    /// 给玩家上毒（Crystal `HumanObject.ApplyPoison`，`HumanObject.cs:7380-7425`）。
    pub(crate) fn apply_poison_to_player(&mut self, target_id: u32, poison: PoisonState) -> bool {
        let Some((map, position)) = self
            .players
            .get(&target_id)
            .map(|player| (player.map.clone(), player.position))
        else {
            return false;
        };
        let Some(player) = self.players.get_mut(&target_id) else {
            return false;
        };
        if player.dead {
            return false;
        }
        if let Some(existing) = player
            .magic
            .poisons
            .iter_mut()
            .find(|existing| existing.kind == poison.kind)
        {
            if poison.kind == PoisonType::Green && existing.value > poison.value {
                return false;
            }
            if poison.kind != PoisonType::Green
                && existing.duration.saturating_sub(existing.ticks) > poison.duration
            {
                return false;
            }
            *existing = poison;
        } else {
            player.magic.poisons.push(poison);
        }
        self.sync_player_poison(target_id, &map, position);
        true
    }

    /// 广播某一格的毒类型变化（Crystal `S.ObjectPoisoned`，`MonsterObject.cs:1568`、
    /// `HumanObject.cs:730`）。
    fn sync_monster_poison(&mut self, map: &str, object_id: u32, position: Point) {
        let Some(kind) = self
            .monster(object_id)
            .map(crate::monster::Monster::poison_type)
        else {
            return;
        };
        let frame = ServerPacket::ObjectPoisoned(ObjectPoisoned {
            object_id,
            poison: kind,
        })
        .encode_frame();
        self.broadcast(map, position, Some(object_id), &frame, object_id);
    }

    /// 玩家毒类型变化：本人 `S.Poisoned` + 周围 `S.ObjectPoisoned`
    /// （Crystal `HumanObject.cs:727-732`）。
    fn sync_player_poison(&mut self, object_id: u32, map: &str, position: Point) {
        let Some(kind) = self.players.get(&object_id).map(Player::poison_type) else {
            return;
        };
        self.send_to_player(
            object_id,
            &ServerPacket::Poisoned(Poisoned { poison: kind }).encode_frame(),
        );
        let frame = ServerPacket::ObjectPoisoned(ObjectPoisoned {
            object_id,
            poison: kind,
        })
        .encode_frame();
        self.broadcast(map, position, Some(object_id), &frame, object_id);
    }

    /// 玩家中毒每 tick 结算（Crystal `HumanObject.ProcessPoison`，`HumanObject.cs:650-733`）。
    ///
    /// 人类口径：**最后一跳仍然结算**（先扣血再移除，`:668-693`）。
    pub(crate) fn tick_player_poison(&mut self, player_id: u32, now: i64) {
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        if player.dead || player.magic.poisons.is_empty() {
            return;
        }
        let map = player.map.clone();
        let position = player.position;
        let mut changed = false;
        let mut index = 0;
        while index
            < self
                .players
                .get(&player_id)
                .map_or(0, |player| player.magic.poisons.len())
        {
            let poison = self.players[&player_id].magic.poisons[index];
            // 毒主下线 → 整条毒移除（`MonsterObject.cs:1464-1474` 的等价语义）。
            if poison.owner_id != 0 && !self.players.contains_key(&poison.owner_id) {
                self.players
                    .get_mut(&player_id)
                    .map(|player| player.magic.poisons.remove(index));
                changed = true;
                continue;
            }
            if now > poison.tick_at {
                let mut poison = poison;
                poison.ticks = poison.ticks.saturating_add(1);
                poison.tick_at = now + POISON_TICK_MS;
                if let Some(player) = self.players.get_mut(&player_id) {
                    if let Some(slot) = player.magic.poisons.get_mut(index) {
                        *slot = poison;
                    }
                }
                if poison.kind == PoisonType::Green {
                    if let Some(player) = self.players.get_mut(&player_id) {
                        player.last_hitter = (poison.owner_id != 0).then_some(poison.owner_id);
                        player.last_hit_time = now + 10_000;
                        player.regen_time = now + PLAYER_REGEN_DELAY_MS;
                    }
                    self.change_player_hp(player_id, -poison.value);
                    self.emit_damage_indicator(
                        &map,
                        position,
                        player_id,
                        -poison.value,
                        DamageType::Hit,
                    );
                    if self
                        .players
                        .get(&player_id)
                        .is_some_and(|player| player.dead)
                    {
                        break;
                    }
                }
                if poison.ticks >= poison.duration {
                    self.players
                        .get_mut(&player_id)
                        .map(|player| player.magic.poisons.remove(index));
                    changed = true;
                    continue;
                }
            }
            index += 1;
        }
        if changed {
            self.sync_player_poison(player_id, &map, position);
        }
    }

    /// 怪物中毒每 tick 结算（Crystal `MonsterObject.ProcessPoison`，`MonsterObject.cs:1453-1569`）。
    ///
    /// 怪物口径：`Time++` 后若 `Time >= Duration` 先移除再 `continue`，
    /// **最后一跳不结算**（`:1476-1491`）。
    pub(crate) fn tick_monster_poison(&mut self, monster: &mut crate::monster::Monster, now: i64) {
        if monster.dead || monster.poisons.is_empty() {
            return;
        }
        let map = monster.map.clone();
        let position = monster.position;
        let object_id = monster.object_id;
        let mut changed = false;
        let mut index = 0;
        while index < monster.poisons.len() {
            let poison = monster.poisons[index];
            if poison.owner_id != 0 && !self.players.contains_key(&poison.owner_id) {
                monster.poisons.remove(index);
                changed = true;
                continue;
            }
            if now > poison.tick_at {
                let mut poison = poison;
                poison.ticks = poison.ticks.saturating_add(1);
                poison.tick_at = now + POISON_TICK_MS;
                if poison.ticks >= poison.duration {
                    monster.poisons.remove(index);
                    changed = true;
                    continue;
                }
                monster.poisons[index] = poison;
                if poison.kind == PoisonType::Green {
                    if monster.expo_owner.is_none() && poison.owner_id != 0 {
                        monster.expo_owner = Some(poison.owner_id);
                    }
                    monster.hp = (monster.hp - poison.value).max(0);
                    self.emit_damage_indicator(
                        &map,
                        position,
                        object_id,
                        -poison.value,
                        DamageType::Hit,
                    );
                    if monster.hp == 0 {
                        // 死亡由 `tick_monsters` 在回写后统一结算（避免覆盖世界里的副本）。
                        break;
                    }
                }
            }
            index += 1;
        }
        if changed {
            let kind = monster.poison_type();
            let frame = ServerPacket::ObjectPoisoned(ObjectPoisoned {
                object_id,
                poison: kind,
            })
            .encode_frame();
            self.broadcast(&map, position, Some(object_id), &frame, object_id);
        }
    }

    /// 目标格是否在地图边界内（**既有简化**，只给 `HellFire` 用）。
    ///
    /// Crystal 的 `Map.ValidPoint`（`Map.cs:637-640`）是「边界 **且** `Cell.Valid`（地形可走）」；
    /// 本作的 `HellFire` 从 milestone 1 起就只做边界判定（火焰可以推进到不能站的格子上），
    /// 这是有意保留的偏差。新写的技能（火墙/雷电）一律用 [`World::walkable_on`] 对齐
    /// `ValidPoint`。
    fn in_bounds(&self, map: &str, cell: Point) -> bool {
        let (max_x, max_y) = self.map_bounds(map);
        cell.x >= 0 && cell.y >= 0 && cell.x <= max_x && cell.y <= max_y
    }

    /// 发一条 `S.NewMagic`（Crystal `HumanObject.SendMagicInfo`，`HumanObject.cs:2275-2278`）。
    fn send_new_magic(&self, object_id: u32, magic: UserMagic) {
        let Some(info) = self.magic_info(magic.spell) else {
            return;
        };
        self.send_to_player(
            object_id,
            &ServerPacket::NewMagic(NewMagic {
                magic: magic.client_magic(info),
                hero: false,
            })
            .encode_frame(),
        );
    }
}

/// 以 8 个方向为一圈旋转 `steps` 格（Crystal `(MirDirection)(((int)Direction + 1) % 8)`，
/// `HumanObject.cs:4112`）。
fn rotate_direction(direction: MirDirection, steps: i32) -> MirDirection {
    let value = (direction as i32 + steps).rem_euclid(8);
    u8::try_from(value)
        .ok()
        .and_then(MirDirection::from_value)
        .unwrap_or(direction)
}

/// 一次增益施法的上下文（参数太多，打包成结构体；字段全部是值语义）。
struct BuffCast<'a> {
    /// 施法者对象 ID。
    object_id: u32,
    /// 技能编号。
    spell: Spell,
    /// 施法者的技能记录（等级决定时长与加成）。
    magic: UserMagic,
    /// 技能模板（`GetPower` 要用 `PowerBase/PowerBonus`）。
    info: &'a MagicInfo,
    /// 施法瞬间的战斗属性（`MaxAC`、`Luck`、`MinSC/MaxSC` 等）。
    stats: crate::combat::CombatStats,
    /// 施法者所在格。
    position: Point,
    /// 施法落点。
    target_location: Point,
    /// 目标（0 = 自己）。
    target_id: u32,
    /// 施法时刻（毫秒）。
    now: i64,
}

/// 技能模块私有的发包工具。
impl World {
    /// 用物品失败的 `S.UseItem`（Crystal `PlayerObject.UseItem` 的 `p` 参数）——
    /// 实现已挪到物品系统（[`World::send_use_item_failure`]），这里保留别名方便阅读。
    fn use_item_failure(&self, object_id: u32, unique_id: u64) {
        self.send_use_item_failure(object_id, unique_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    fn info() -> MagicInfo {
        MagicInfo {
            name: "火球术".to_owned(),
            spell: Spell::FireBall.value(),
            base_cost: 5,
            level_cost: 2,
            icon: 10,
            level1: 7,
            level2: 9,
            level3: 12,
            need1: 100,
            need2: 200,
            need3: 300,
            delay_base: 1_800,
            delay_reduction: 100,
            power_base: 4,
            power_bonus: 0,
            m_power_base: 6,
            m_power_bonus: 0,
            range: 8,
            multiplier_base: 1.0,
            multiplier_bonus: 0.25,
        }
    }

    #[test]
    fn magic_record_matches_crystal_layout() {
        let magic = UserMagic {
            spell: Spell::FireBall,
            level: 2,
            key: 5,
            experience: 1234,
        };
        let bytes = encode_magics(&[magic]);
        // `int32 count` + 14 字节记录（`MagicInfo.cs:135-144`）。
        assert_eq!(bytes.len(), 4 + MAGIC_RECORD_LEN);
        assert_eq!(&bytes[..4], &1i32.to_le_bytes());
        assert_eq!(bytes[4], Spell::FireBall.value());
        assert_eq!(bytes[5], 2);
        assert_eq!(bytes[6], 5);
        assert_eq!(&bytes[7..9], &1234u16.to_le_bytes());
        assert_eq!(bytes[9], 0, "IsTempSpell = false");
        assert_eq!(&bytes[10..18], &0i64.to_le_bytes(), "CastTime = 0");

        let decoded = decode_magics(&bytes).expect("解码应当成功");
        assert_eq!(decoded, vec![magic]);
    }

    #[test]
    fn empty_blob_is_empty_list_and_garbage_is_an_error() {
        assert!(decode_magics(&[]).expect("空块").is_empty());
        assert!(decode_magics(&[1, 2, 3]).is_err(), "截断的记录应当报错");
        let too_many = i32::try_from(MAX_MAGICS + 1).unwrap().to_le_bytes();
        assert!(matches!(
            decode_magics(&too_many),
            Err(MagicCodecError::TooManyMagics { .. })
        ));
    }

    #[test]
    fn cost_delay_and_multiplier_follow_crystal() {
        let magic = UserMagic {
            spell: Spell::FireBall,
            level: 2,
            key: 0,
            experience: 0,
        };
        let info = info();
        // `BaseCost + LevelCost * Level`（`HumanObject.cs:3377`）。
        assert_eq!(magic.cost(&info), 5 + 2 * 2);
        // `DelayBase - Level * DelayReduction`（`MagicInfo.cs:221`）。
        assert_eq!(magic.delay_ms(&info), 1_800 - 2 * 100);
        // `MultiplierBase + Level * MultiplierBonus`（`:187`）。
        assert!((magic.multiplier(&info) - 1.5).abs() < f64::EPSILON);
    }

    #[test]
    fn power_uses_banker_rounding_and_rolls() {
        let mut rng = MirRng::from_seed(3);
        let magic = UserMagic {
            spell: Spell::FireBall,
            level: 1,
            key: 0,
            experience: 0,
        };
        // MPower = 6（无 bonus，不抽签）、DefPower = 4 → round((6/4)*2 + 4) = round(7.0) = 7。
        assert_eq!(magic.power(&info(), &mut rng), 7);
        // `GetDamage(base)` = (base + power) * multiplier，level 1 → 1.25。
        // base 10 → (10 + 7) * 1.25 = 21.25 → 21（C# 的 `(int)` 截断）。
        assert_eq!(magic.damage(&info(), 10, &mut rng), 21);
    }

    #[test]
    fn client_magic_clamps_experience() {
        let magic = UserMagic {
            spell: Spell::FireBall,
            level: 1,
            key: 3,
            experience: u32::from(u16::MAX) + 5,
        };
        let client = magic.client_magic(&info());
        assert_eq!(client.experience, u16::MAX);
        assert_eq!(client.key, 3);
        assert_eq!(client.range, 8);
        assert_eq!(client.delay, 1_700);
        assert_eq!(client.cast_time, 0);
    }

    /// 测试用玩家（字段与 `world.rs` 的测试辅助一致）。
    fn test_player(
        object_id: u32,
        map: &str,
        position: Point,
        level: u16,
        dead: bool,
    ) -> (crate::world::Player, mpsc::UnboundedReceiver<Vec<u8>>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        let stats = crate::combat::player_base_stats(level, MirClass::Warrior);
        (
            crate::world::Player {
                object_id,
                session_id: u64::from(object_id),
                character_id: i64::from(object_id),
                name: format!("玩家{object_id}"),
                map: map.to_owned(),
                position,
                direction: MirDirection::Down,
                level,
                experience: 0,
                max_experience: 0,
                class: MirClass::Warrior,
                gender: mir_core::enums::MirGender::Male,
                hair: 1,
                hp: stats.hp,
                mp: stats.mp,
                stats,
                action_time: 0,
                attack_time: 0,
                struck_time: 0,
                regen_time: 0,
                death_time: 0,
                revive_time: 0,
                cell_time: 0,
                step_counter: 0,
                run_counter: 0,
                run_time: 0,
                dead,
                attack_mode: mir_core::enums::AttackMode::Peace,
                pk_points: 0,
                brown_time: 0,
                pk_point_time: 0,
                last_hitter: None,
                last_hit_time: 0,
                name_colour: mir_core::Color(-1),
                visible_monsters: std::collections::BTreeSet::new(),
                visible_npcs: std::collections::BTreeSet::new(),
                movement_time: 0,
                inventory: crate::items::new_inventory(),
                equipment: crate::items::new_equipment(),
                storage: crate::storage::new_storage(),
                magics: Vec::new(),
                magic: MagicState::default(),
                gold: 0,
                credit: 0,
                visible_items: std::collections::BTreeSet::new(),
                quests: Vec::new(),
                completed_quests: std::collections::BTreeSet::new(),
                mail: crate::mail::MailState::default(),
                buffs: Vec::new(),
                pot_health_amount: 0,
                pot_mana_amount: 0,
                pot_time: 0,
                unlock_curse: false,
                npc: crate::npc::NpcState::default(),
                group_id: None,
                group_invitation: None,
                group_invite_time: 0,
                allow_group: true,
                guild: None,
                trade: crate::trade::TradeState::default(),
                trade_invitation: None,
                next_trade_time: 0,
                allow_trade: true,
                sender,
            },
            receiver,
        )
    }

    /// 对拍：AreaBuff 的新 AOI 选择结果必须与旧的 `players.values() + in_range`
    /// 暴力扫描（含「同图 / 存活 / 半径内」三条判定）完全一致，
    /// 且输出顺序（按对象 ID 升序）不变。
    #[test]
    fn area_buff_targets_match_the_old_bruteforce_scan() {
        let mut world = World::new(999);
        let location = Point::new(20, 20);
        let boundary = Point::new(20 + SHIELD_AREA_RADIUS, 20);
        let outside = Point::new(20 + SHIELD_AREA_RADIUS + 1, 20);
        let cases: [(u32, &str, Point, u16, bool); 5] = [
            // 半径内、同图、存活 → 命中。
            (1, "0", location, 10, false),
            // 恰好落在半径边界（切比雪夫 ≤ radius）→ 命中。
            (2, "0", boundary, 20, false),
            // 半径外 → 不命中。
            (3, "0", outside, 30, false),
            // 同格但已死亡 → 不命中。
            (4, "0", location, 40, true),
            // 同坐标但另一张地图 → 不命中。
            (5, "1", location, 50, false),
        ];
        // 收包端只用于保活（`enter` 会往 sender 写外观包），不参与断言。
        let mut receivers = Vec::new();
        for (object_id, map, position, level, dead) in cases {
            let (player, receiver) = test_player(object_id, map, position, level, dead);
            receivers.push(receiver);
            world.enter(player);
        }

        // 新实现：AOI 网格取落点半径内的玩家。
        let new_targets = world.area_buff_targets("0", location);
        // 旧实现（t3 之前的暴力扫描）原样重算，作为对拍基准。
        let mut old_targets: Vec<(u32, u16)> = world
            .players
            .values()
            .filter(|player| {
                player.map == "0"
                    && !player.dead
                    && player.position.in_range(location, SHIELD_AREA_RADIUS)
            })
            .map(|player| (player.object_id, player.level))
            .collect();
        old_targets.sort_unstable();

        assert_eq!(
            new_targets, old_targets,
            "AOI 查询结果应与人物列表暴力扫描一致"
        );
        assert_eq!(
            new_targets,
            vec![(1, 10), (2, 20)],
            "只命中同图、存活、切比雪夫半径内的玩家（含边界）"
        );
    }

    #[test]
    fn spell_kinds_cover_the_supported_subset() {
        assert_eq!(spell_kind(Spell::FireBall), SpellKind::Bolt);
        assert_eq!(spell_kind(Spell::GreatFireBall), SpellKind::Bolt);
        assert_eq!(spell_kind(Spell::ThunderBolt), SpellKind::Bolt);
        assert_eq!(spell_kind(Spell::SoulFireBall), SpellKind::Bolt);
        assert_eq!(spell_kind(Spell::HellFire), SpellKind::HellFire);
        assert_eq!(spell_kind(Spell::FireBang), SpellKind::FireBang);
        assert_eq!(spell_kind(Spell::Healing), SpellKind::Heal);
        assert_eq!(spell_kind(Spell::MassHealing), SpellKind::MassHealing);
        // 9 个增益技能现在都有实现（`crate::buffs`），不再是 `Unsupported`。
        for spell in [
            Spell::ProtectionField,
            Spell::MagicShield,
            Spell::SoulShield,
            Spell::BlessedArmour,
            Spell::EnergyShield,
            Spell::Haste,
            Spell::LightBody,
            Spell::Fury,
            Spell::MagicBooster,
        ] {
            assert_eq!(spell_kind(spell), SpellKind::Buff, "{spell:?} 应是增益技能");
        }
        // 登记但还没实现的那些，仍然走 `Unsupported`。
        for spell in REGISTERED_UNSUPPORTED {
            assert_eq!(spell_kind(*spell), SpellKind::Unsupported);
        }
        // 第 2 轮补齐的 7 个技能（火墙/雷电/瞬息移动/隐身/召唤/施毒）。
        for spell in [
            Spell::FireWall,
            Spell::Lightning,
            Spell::Teleport,
            Spell::Hiding,
            Spell::SummonSkeleton,
            Spell::SummonShinsu,
            Spell::Poisoning,
        ] {
            assert_ne!(
                spell_kind(spell),
                SpellKind::Unsupported,
                "{spell:?} 应该已经有实现"
            );
        }
        assert_eq!(spell_kind(Spell::FireWall), SpellKind::FireWall);
        assert_eq!(spell_kind(Spell::Lightning), SpellKind::Lightning);
        assert_eq!(spell_kind(Spell::Teleport), SpellKind::Teleport);
        assert_eq!(spell_kind(Spell::Hiding), SpellKind::Hiding);
        assert_eq!(spell_kind(Spell::SummonSkeleton), SpellKind::Summon);
        assert_eq!(spell_kind(Spell::SummonShinsu), SpellKind::Summon);
        assert_eq!(spell_kind(Spell::Poisoning), SpellKind::Poison);
        assert_eq!(spell_kind(Spell::Rage), SpellKind::Unsupported);
        // 增益技能不能和已实现的伤害/治疗技能重叠。
        assert!(!REGISTERED_UNSUPPORTED.contains(&Spell::MagicShield));
    }
}
