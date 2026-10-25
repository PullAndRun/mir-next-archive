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
//! * 护身符/毒药消耗：**只有毒雾（`PoisonCloud`）** 已接上（符 5 个 + 绿毒 5 个，走
//!   [`crate::spell_consumables`] 的 `find_spell_material`/`consume_spell_material`，
//!   `HumanObject.cs:4543-4567`）；`SoulFireBall`、`SoulShield`、`Hiding`、`Summon*`、
//!   `Poisoning` 在 Crystal 里也要消耗（`HumanObject.cs:4382-4396`、`:4515`、`:4489-4498`、
//!   `:4419-4426`、`:4369-4377`），本作仍未接。
//! * 毒雾的 D3 偏差（PARTIAL）：施法者离线/换图时本作立刻清场并停伤；Crystal 的毒雾
//!   会留在图上直到 `ExpireTime`（`Caster = null` 后 `IsAttackTarget` 拒绝而停伤），
//!   异图强清只对火墙生效（`SpellObject.cs:47-51`、`:66`、`:78-86`）。
//! * 第 2 轮新增的 7 个技能见 [`SpellKind`]：`FireWall`/`Lightning`/`Teleport`/`Hiding`/
//!   `Summon`/`Poison`；逐条差异（无 bind 点的传送、绿→红的施毒策略、火墙的轻量场等）
//!   都写在各自方法文档里。

use std::collections::HashMap;

use mir_core::MirRng;
use mir_core::Point;
use mir_core::TextKey;
use mir_core::enums::{
    BuffType, DamageType, DefenceType, ItemType, MirClass, MirDirection, MirGridType, PoisonType,
    RequiredClass, Spell, SpellEffect, Stat,
};
use mir_db::MagicInfo;
use mir_protocol::data::{ClientBuff, ClientMagic, Stats, UserItem};
use mir_protocol::packets::server::{
    AddBuff, Magic, MagicCast, MagicDelay, MagicLeveled, NewMagic, ObjectDash, ObjectDashFail,
    ObjectEffect, ObjectHidden, ObjectMagic, ObjectPoisoned, ObjectPushed, ObjectRemove,
    ObjectSpell, ObjectTeleportIn, ObjectTeleportOut, Poisoned, Pushed, RemoveBuff, UseItem,
    UserDash, UserDashFail,
};
use mir_protocol::{PacketReader, PacketWriter, ProtocolError, ServerPacket};

use crate::aoi::AoiKind;
use crate::buffs::{
    BuffKind, ENERGY_SHIELD_MIN_CHANCE, PendingBuff, SECOND_MS, SHIELD_AREA_RADIUS,
};
use crate::combat::PendingHit;
use crate::monster::MonsterKind;
use crate::world::{
    MOVE_DELAY_MS, PLAYER_REGEN_DELAY_MS, Player, VIEW_RANGE, World, spell_field_view_location,
    spell_field_visible_to_client,
};
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

/// 瞬移的落地延迟与瞬息移动同为 200ms（`HumanObject.cs:3537-3539`
/// 的 `case Spell.Teleport: case Spell.Blink:`）。
pub const BLINK_DELAY_MS: i64 = TELEPORT_DELAY_MS;

/// 雷暴/狂风绝命自身落点的 5×5 雷击延迟：`Envir.Time + 500`（`HumanObject.cs:4268`、
/// `Map.cs:1307`）。
pub const THUNDER_STORM_DELAY_MS: i64 = 500;

/// 雷暴范围：落点 ±2 的 5×5（`Map.cs:1308-1309`）。
pub const THUNDER_STORM_RADIUS: i32 = 2;

/// 雷暴对**非亡灵**目标的伤害除数：`value / 10`（`Map.cs:1335`）。
pub const THUNDER_STORM_DAMAGE_DIVISOR: i32 = 10;

/// 狂风绝命的落点传送延迟：`ActionList.Add(new DelayedAction(DelayedType.Magic,
/// Envir.Time + 750, magic, location))`（`HumanObject.cs:6091-6093`）。
pub const STORM_ESCAPE_DELAY_MS: i64 = 750;

/// 冰咆哮（`FrostCrunch`）的飞行延迟：`MaxDistance * 50 + 500`ms，
/// 每格 50ms（`HumanObject.cs:3945`）。
pub const FROST_CRUNCH_STEP_MS: i64 = 50;

/// 冰咆哮的固定延迟：`MaxDistance * 50 + 500` 里的 500ms
/// （`HumanObject.cs:3945` 的 `Fireball` 延迟公式）。
pub const FROST_CRUNCH_BASE_DELAY_MS: i64 = 500;

/// 冲撞结束时写的 `CellTime = Envir.Time + 500`（`HumanObject.cs:5143`）。
pub const SHOULDER_DASH_CELL_TIME_MS: i64 = 500;

/// 被推一格后的 `MoveTime = Envir.Time + 500 * 格数`，
/// `ActionTime = Envir.Time + 300 * 格数`（`MonsterObject.cs:4855-4856`）。
pub const PUSH_MOVE_DELAY_MS: i64 = 500;

/// 被推一格后的 `ActionTime` 增量（`MonsterObject.cs:4856`）。
pub const PUSH_ACTION_DELAY_MS: i64 = 300;

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

/// 冰咆哮两种毒的每跳间隔：`TickSpeed = 1000`
/// （`HumanObject.cs:5974` 的 Slow、`:5986` 的 Frozen）。这是**独立于**
/// [`POISON_TICK_MS`] 的口径，Crystal 里同一个 `Poison` 结构按种类带不同的 `TickSpeed`。
pub const FROST_CRUNCH_POISON_TICK_MS: i64 = 1_000;

/// 毒雾（`PoisonCloud`）的落地延迟系数：`MaxDistance(CurrentLocation, location) * 50 + 500`
/// （`HumanObject.cs:4546`，每格 50ms）。
pub const POISON_CLOUD_STEP_MS: i64 = 50;

/// 毒雾（`PoisonCloud`）的固定落地延迟：延迟公式里的 `+ 500`（`HumanObject.cs:4546`）。
pub const POISON_CLOUD_BASE_DELAY_MS: i64 = 500;

/// 毒雾的存在时长：`ExpireTime = Envir.Time + 6000`（`Server/MirEnvir/Map.cs:1450`）。
pub const POISON_CLOUD_EXPIRE_MS: i64 = 6_000;

/// 毒雾每跳间隔：`TickSpeed = 1000`（`Map.cs:1451`）。
pub const POISON_CLOUD_TICK_MS: i64 = 1_000;

/// 毒雾每次命中附上的绿毒跳数：`new Poison { Duration = 12, PType = PoisonType.Green }`
/// （`Server/MirObjects/SpellObject.cs:166-174`）。
pub const POISON_CLOUD_POISON_DURATION: u32 = 12;

/// 毒雾的护身符形状：`GetAmulet(5)` 只认 `ItemType.Amulet`（`HumanObject.cs:6859`）。
pub const POISON_CLOUD_AMULET_SHAPE: u16 = 0;

/// 毒雾的毒药形状：`GetPoison(5, 1)` 的 `shape = 1`（绿毒，`HumanObject.cs:6873`）。
pub const POISON_CLOUD_POISON_SHAPE: u16 = 1;

/// 毒雾一次施法烧掉的护身符/毒药数量：`GetAmulet(5)` + `GetPoison(5, 1)`（`HumanObject.cs:4545-4549`）。
pub const POISON_CLOUD_MATERIAL_COUNT: u32 = 5;

/// 困魔咒（`TrapHexagon`）落地延迟：`DelayedAction(..., Envir.Time + 500, ...)`
/// （`HumanObject.cs:4634-4638`）。
pub const TRAP_HEXAGON_DELAY_MS: i64 = 500;

/// 困魔咒持续时间的等级步长：`duration = (magic.Level * 5 + 10) * 1000`
/// （`HumanObject.cs:4632`）。
pub const TRAP_HEXAGON_DURATION_STEP_MS: i64 = 5_000;

/// 困魔咒持续时间的基数：0 级 10 秒、3 级 25 秒（`HumanObject.cs:4632`）。
pub const TRAP_HEXAGON_BASE_DURATION_MS: i64 = 10_000;

/// 困魔咒耗材数量：`GetAmulet(1)`（`HumanObject.cs:4630`）。
pub const TRAP_HEXAGON_AMULET_COUNT: u32 = 1;

/// 困魔咒耗材形状：护身符（`Shape = 0`，与 `GetAmulet` 的 `FindItem` 参数一致）。
pub const TRAP_HEXAGON_AMULET_SHAPE: u16 = 0;

/// 困魔咒外围场对象的 `TickSpeed`（`Map.cs:1815`，`SpellObject{ TickSpeed = 100 }`）。
///
/// 本作的场没有周期伤害，`next_tick` 设为 [`i64::MAX`] 表示「不结算」，这个常量只做
/// 与 Crystal 的数值对照，实际不参与调度。
pub const TRAP_HEXAGON_FIELD_TICK_MS: i64 = 100;

/// 困魔咒中心怪等级上限差：`target.Level > player.Level + 2` 的怪不受影响
/// （`HumanObject.cs:4621`、`Map.cs:1782`）。
pub const TRAP_HEXAGON_LEVEL_MARGIN: u16 = 2;

/// 困魔咒八个外围场相对施法落点的坐标（`Map.cs:1797-1815`）：
/// `Up/Right/Down/Left` 各走 2 格，再沿 `Right`/`Up` 各偏移 1 格展开。
pub const TRAP_HEXAGON_FIELD_POINTS: [(i32, i32); 8] = [
    (1, -2),
    (-1, -2),
    (2, -1),
    (2, 1),
    (1, 2),
    (-1, 2),
    (-2, -1),
    (-2, 1),
];

/// 困魔咒持续时间（毫秒）：`(level * 5 + 10) * 1000`（`HumanObject.cs:4632`）。
#[must_use]
pub const fn trap_hexagon_duration_ms(level: u8) -> i64 {
    (level as i64 * TRAP_HEXAGON_DURATION_STEP_MS) + TRAP_HEXAGON_BASE_DURATION_MS
}

/// 怪物被 `PoisonType.Slow` 每跳叠加的移动/攻击间隔：`MoveSpeed += 100`、
/// `AttackSpeed += 100`（`MonsterObject.cs:1544-1553`）。
pub const MONSTER_SLOW_STEP_MS: i64 = 100;

/// 怪物被 Slow 叠成的速度上限：`if (MoveSpeed > 3500) MoveSpeed = 3500`
/// （`MonsterObject.cs:1548-1552`）。
pub const MONSTER_SLOW_MAX_MS: i64 = 3_500;

/// 玩家被推一格后的 `ActionTime = Envir.Time + 500`（`HumanObject.cs:2687-2694`）。
pub const PLAYER_PUSH_ACTION_MS: i64 = 500;

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

    /// 两参数兼容版本：只叠加 `ManaPenaltyPercent`（不叠加传送惩罚）。
    /// 第 51 轮给 [`Self::cost_with_penalties`] 增加传送惩罚参数后保留此入口，
    /// 供既有集成测试 `crates/mir-server/tests/use_items.rs` 继续使用。
    pub fn cost_with_penalty(&self, info: &MagicInfo, mana_penalty_percent: i32) -> i32 {
        self.cost_with_penalties(info, mana_penalty_percent, 0)
    }

    /// 带「魔法消耗惩罚」的施法耗蓝（Crystal `HumanObject.MagicCost`，`HumanObject.cs:3375-3398`）。
    ///
    /// ```csharp
    /// int cost = magic.Info.BaseCost + magic.Info.LevelCost * magic.Level;
    /// if (spell == Teleport || spell == Blink || spell == StormEscape)
    ///     if (Stats[Stat.TeleportManaPenaltyPercent] > 0) cost += (cost * percent) / 100;
    /// if (Stats[Stat.ManaPenaltyPercent] > 0) cost += (cost * Stats[Stat.ManaPenaltyPercent]) / 100;
    /// ```
    ///
    /// `TeleportManaPenaltyPercent` 来自 `TemporalFlux` 增益（30%，`HumanObject.cs:6106`）；
    /// `ManaPenaltyPercent` 来自魔力增幅（`BuffType.MagicBooster`，`6 + Level`，`:6234`）。
    pub fn cost_with_penalties(
        &self,
        info: &MagicInfo,
        mana_penalty_percent: i32,
        teleport_penalty_percent: i32,
    ) -> i32 {
        let mut cost = self.cost(info);
        if teleport_penalty_percent > 0
            && matches!(
                self.spell,
                Spell::Teleport | Spell::Blink | Spell::StormEscape
            )
        {
            cost += cost * teleport_penalty_percent / 100;
        }
        if mana_penalty_percent > 0 {
            cost += cost * mana_penalty_percent / 100;
        }
        cost
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
/// * `tick_speed_ms` → `TickSpeed`（Crystal 每条毒自带跳速：普通毒 2000ms、
///   冰咆哮的 Slow/Frozen 1000ms，见 `HumanObject.cs:5974`、`:5986`）；
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
    /// 每跳间隔（Crystal `Poison.TickSpeed`）。
    pub tick_speed_ms: i64,
    /// 施毒者对象 ID（0 = 无主）。
    pub owner_id: u32,
}

impl PoisonState {
    /// 新建一条毒；第一跳在 `tick_at` 时刻结算（Crystal `TickTime` 初值 0 → 首跳立即），
    /// 跳速默认 [`POISON_TICK_MS`]（Crystal 普通毒的 `TickSpeed = 2000`）。
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
            tick_speed_ms: POISON_TICK_MS,
            owner_id,
        }
    }

    /// 覆盖每跳间隔（Crystal `new Poison { TickSpeed = 1000 }`，`HumanObject.cs:5974`）。
    #[must_use]
    pub const fn with_tick_speed(mut self, tick_speed_ms: i64) -> Self {
        self.tick_speed_ms = tick_speed_ms;
        self
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
    /// 绿毒附伤（Crystal `SpellObject.BonusDmg`）：只有毒雾用，火墙恒 0。
    pub bonus_damage: i32,
    /// 施法落点（Crystal `SpellObject.CastLocation`）：只有毒雾的首个对象会广播它。
    pub cast_location: Point,
    /// 是否向客户端广播过（Crystal `SpellObject.Show`）：毒雾 9 格里只有
    /// 首个实际生成的对象为 `true`（`Map.cs:1452-1460`）。
    pub show: bool,
    /// 绑定的中心怪对象 ID（Crystal `SpellObject.Target`）：`0` = 无绑定
    /// （火墙/毒雾）；困魔咒的 8 个外围场填中心怪，中心怪 `ShockTime` 归零或死亡时
    /// 整组场提前撤销（`SpellObject.cs:60-70`、`Map.cs:1815`）。
    pub target_monster: u32,
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
    /// 冲撞（`HumanObject.cs:4975-5157`，`CompleteMagic` 里同步调用）。
    ShoulderDash,
    /// 瞬移（`HumanObject.cs:3537-3539` + `:6114-6128`）。
    Blink,
    /// 狂风绝命（`HumanObject.cs:3580-3588` + `:6079-6129`）：
    /// 先 500ms 落点雷击，再 750ms 传送（不是瞬息移动的别名）。
    StormEscape,
    /// 冰咆哮（`HumanObject.cs:3501-3504`、`:3939-3952`、`:5962-5995`）。
    FrostCrunch,
    /// 雷暴（`HumanObject.cs:4264-4270` + `Map.cs:1303-1341`）：自身脚下 5×5。
    ThunderStorm,
    /// 毒雾（`PoisonCloud`）：落点 3×3 铺毒云、`POISON_CLOUD_EXPIRE_MS` 内每
    /// `POISON_CLOUD_TICK_MS` 结算；命中先 `DefenceType.MAC` 伤害、目标存活再附
    /// 12 跳绿毒（`HumanObject.cs:4543-4567` + `Map.cs:1434-1486` +
    /// `SpellObject.cs:150-163`）。
    PoisonCloud,
    /// 困魔咒（`TrapHexagon`）：施法前 3×2 预检、延迟 500ms、落地 3×3 重检写
    /// `ShockTime`，再在落点周围生成 8 个绑定中心怪的场对象
    /// （`HumanObject.cs:4596-4644` + `Map.cs:1752-1832` + `SpellObject.cs:51-70`）。
    TrapHexagon,
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
        Spell::PoisonCloud => SpellKind::PoisonCloud,
        Spell::TrapHexagon => SpellKind::TrapHexagon,
        Spell::ShoulderDash => SpellKind::ShoulderDash,
        Spell::Blink => SpellKind::Blink,
        Spell::StormEscape => SpellKind::StormEscape,
        Spell::FrostCrunch => SpellKind::FrostCrunch,
        Spell::ThunderStorm => SpellKind::ThunderStorm,
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
    /// 瞬移落点（`HumanObject.cs:6114-6128`）：200ms 后校验落点、按 `1/(Level+1)` 抽签传送。
    Blink {
        /// 目标落点（客户端指定）。
        location: Point,
    },
    /// 狂风绝命的落点传送（`HumanObject.cs:6079-6093`）：750ms 后校验、抽签并传送。
    StormEscape {
        /// 目标落点（客户端指定）。
        location: Point,
    },
    /// 雷暴（`Map.cs:1303-1341`）：以 `location` 为中心的 5×5 范围伤害。
    ThunderStorm {
        /// **施法瞬间**施法者所在的地图。Crystal 把延迟动作挂在
        /// `CurrentMap.ActionList` 上（`HumanObject.cs:4268-4269`、`:3530-3533`），
        /// 结算发生在**施法地图**；施法后 500ms 内换图不能让旧坐标打到新图（P05）。
        map: String,
        /// 中心格（`HumanObject.cs:4268`：施法瞬间人物所在格）。
        location: Point,
        /// 施法时抽好的伤害（非亡灵目标结算时再 `/10`，`Map.cs:1335`）。
        damage: i32,
    },
    /// 冰咆哮（`HumanObject.cs:5962-5995`）：单体伤害 + 等级概率减速/冰冻。
    FrostCrunch {
        /// 目标对象 ID。
        target_id: u32,
        /// 施法时的落点（结算时目标必须仍在 ±2 格内，`:5967`）。
        location: Point,
        /// 施法时抽好的伤害。
        damage: i32,
    },
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
    /// 毒雾落地（`Map.cs:1434-1486`）：在 `location` 的 3×3 逐格生成毒云；
    /// 首个实际生成的对象广播 `S.ObjectSpell`（`SpellObject.cs:490-530` 的
    /// `GetInfo` 只在 `Show = true` 时返回），其余只登记为服务端场。
    PoisonCloud {
        /// 施法时的地图（Crystal `Map.CompleteMagic` 里的 `this`，即 `CurrentMap`）。
        map: String,
        /// 施法落点（毒云 3×3 的中心；也是客户端看到的 `CastLocation`）。
        location: Point,
        /// 施法瞬间抽好的 `DefenceType.MAC` 伤害（`HumanObject.cs:4547`）。
        damage: i32,
        /// 施法瞬间抽好的绿毒附伤（`SpellObject.BonusDmg`）：来自
        /// `Random.Next(Stats[Stat.PoisonAttack])`（`HumanObject.cs:4555`），由
        /// [`World::caster_poison_attack`] 汇总装备模板属性/附加属性/生效增益。
        /// `PoisonAttack <= 0` 时为 0 且不消耗随机数。
        bonus_damage: i32,
    },
    /// 困魔咒落地（`Map.cs:1752-1832`）：3×3 重检写 `ShockTime`，再在落点周围铺
    /// 8 个绑定中心怪的场对象。
    TrapHexagon {
        /// 施法时所在的地图（Crystal 的 `DelayedAction` 挂在施法时的 `Map.ActionList`，
        /// 落地用 `centerTarget.CurrentMap`；本作按施法图落地）。
        map: String,
        /// 施法落点（3×2 预检与 3×3 重检的中心）。
        location: Point,
        /// 持续时间（`(magic.Level * 5 + 10) * 1000`，`HumanObject.cs:4632`）。
        duration_ms: i64,
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

/// [`World::cast_magic`] 里「公共检查已通过、但耗材/预检失败」时回显 `cast = false`
/// 所需的施法上下文。
///
/// Crystal 的失败分支仍然照发 `S.Magic` 与 `S.ObjectMagic`（`HumanObject.cs:3773-3779`），
/// 但公共检查的副作用（MP、`ActionTime`、`AttackTime`、`SpellTime`、`Direction`）
/// 都已经发生，`cast = false` 只表示不写本技能 `CastTime`、不排延迟效果。
#[derive(Clone, Copy)]
struct RefusedCast<'a> {
    object_id: u32,
    spell: Spell,
    level: u8,
    target_id: u32,
    target_location: Point,
    position: Point,
    map: &'a str,
    direction: MirDirection,
    cost: i32,
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
        let language = self.player_language(object_id);
        player
            .magics
            .iter()
            .filter_map(|magic| {
                data.magic(magic.spell).map(|info| {
                    let mut client = magic.client_magic(info);
                    client.name = self.magic_name_for(language, info);
                    client
                })
            })
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

        // 冲刺是唯一不写 `ActionTime`、不回 `S.UserLocation` 的技能：Crystal 的
        // `if (spell != Spell.ShoulderDash) ActionTime = ...`（`:3428-3431`）与
        // `if (spell != ShoulderDash && ...) Enqueue(S.UserLocation)`（`:3456-3458`）。
        // 冲刺自己的 `ActionTime`/`S.MagicCast` 由 `ShoulderDash()` 末尾写（`:5145-5151`）。
        let is_dash = kind == SpellKind::ShoulderDash;

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
            SpellKind::FrostCrunch => {
                // `case FireBall/GreatFireBall/FrostCrunch: if (!Fireball(target, magic)) targetID = 0;`
                // （`HumanObject.cs:3501-3504`）。`Fireball` 要求目标可攻击且 `CanFly`
                // （`:3941`）；失败**不报错**，单纯没有目标（照常施法/扣蓝）。
                if target_id == 0 || target_id == object_id {
                    target_id = 0;
                } else if let Some(target_position) =
                    self.magic_attack_position(&map, object_id, target_id)
                {
                    if self.can_fly(&map, position, target_position) {
                        target_location = target_position;
                    } else {
                        target_id = 0;
                    }
                } else {
                    target_id = 0;
                }
            }
            SpellKind::Blink | SpellKind::StormEscape | SpellKind::ThunderStorm => {
                // Blink/StormEscape 用包里的落点；ThunderStorm 只以人物脚下为中心
                // （`HumanObject.cs:4268` 的 `CurrentLocation`）。
                if target_location.x == 0 && target_location.y == 0 {
                    target_location = position;
                }
                target_id = 0;
            }
            SpellKind::ShoulderDash => {
                // `ShoulderDash(magic)` 完全不用 target/location，只取方向
                // （`HumanObject.cs:3558-3560`、`:5013`）。
                target_id = 0;
                target_location = position;
            }
            SpellKind::FireWall => {
                // `FireWall(magic, spellTargetLock ? target.CurrentLocation : location)`（`:3565-3566`）。
                if target_location.x == 0 && target_location.y == 0 {
                    target_location = position;
                }
                target_id = 0;
            }
            SpellKind::PoisonCloud => {
                // `PoisonCloud(magic, location)`（`HumanObject.cs:4543`）：落点直接取包里的
                // `location`，没有目标（`targetID` 不被读取，`:3778` 回显 0）。
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
            SpellKind::TrapHexagon => {
                // `TrapHexagon(magic, spellTargetLock ? target.CurrentLocation : location, ...)`
                // （`HumanObject.cs:3656-3657`）：没有锁定目标就用包里的落点，为空时退回
                // 自己脚下；目标由 3×2 预检自己扫，不读 `target_id`。
                if target_location.x == 0 && target_location.y == 0 {
                    target_location = position;
                }
                target_id = 0;
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
            // 冲刺不在这里锁动作：`if (spell != Spell.ShoulderDash) ActionTime = ...`
            // （`HumanObject.cs:3428-3431`），它的 `ActionTime = now + MoveDelay` 在
            // `ShoulderDash()` 开头（`:4993`），末尾再写 `CellTime`（`:5150`）。
            if !is_dash {
                player.action_time = now + MAGIC_CAST_ACTION_MS;
            }
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
        // `MagicCost`（`:3375-3398`）：基础耗蓝 → 传送类（Teleport/Blink/StormEscape）
        // 先吃 `TeleportManaPenaltyPercent`（TemporalFlux，30%）→ 再吃
        // `ManaPenaltyPercent`（魔力增幅）。顺序与 Crystal 一致。
        let mana_penalty = self.players[&object_id].stats.mana_penalty_percent;
        // `TeleportManaPenaltyPercent` 来自 `TemporalFlux` 增益（30%，`HumanObject.cs:6106`）。
        // 第 51 轮返工 P06：改为读真实增益（`buff.values[0]`，见 `add_temporal_flux`），
        // 增益到期自动失效（Crystal 同一 `Stats[Stat.TeleportManaPenaltyPercent]` 路径）。
        let teleport_penalty = self
            .buff_value(object_id, BuffKind::TemporalFlux)
            .unwrap_or(0);
        let cost = magic.cost_with_penalties(&info, mana_penalty, teleport_penalty);
        let mp = self.players.get(&object_id).map_or(0, |player| player.mp);
        if cost > mp {
            self.send_user_location(object_id);
            return Err(MagicError::NotEnoughMana { cost, mp });
        }
        // `ChangeMP(-cost)`（`:3451`）→ `SendHealthChanged`。
        self.change_player_mp(object_id, -cost);

        // --- 施法回显（`HumanObject.cs:3456-3458`） ---------------------------------
        // `if (spell != ShoulderDash && spell != BackStep && spell != FlashDash)
        //     Enqueue(new S.UserLocation { ... })`：冲刺不回位置包。
        if !is_dash {
            self.send_user_location(object_id);
        }
        // 记下最近一次攻击目标（召唤物优先打主人的目标，本作扩展）。
        if target_id != 0
            && matches!(kind, SpellKind::Bolt | SpellKind::Poison)
            && let Some(player) = self.players.get_mut(&object_id)
        {
            player.magic.last_target = Some(target_id);
        }

        // --- 耗材（Crystal `PoisonCloud` 分支，`HumanObject.cs:4543-4567`） ----------
        // D2 口径：耗材检查放在**公共检查之后**——`CanCast`/射程/冷却/扣蓝都已完成，
        // 所以材料不足时这些计时器与 MP 都保留（Crystal 的 `return` 也回滚不了它们）。
        // 失败时 `cast` 保持 `false`（分支入口就置 false，只有两项材料都拿到才置 true）：
        //   * 不写本技能 `CastTime`（本作 `magic.casts`）、不排延迟效果；
        //   * 仍然发 `S.Magic` 与 `S.ObjectMagic`，只是 `Cast = false`
        //     （`HumanObject.cs:3773-3779`）。
        // 顺序也照抄：先 `GetAmulet(5)`、再 `GetPoison(5, 1)`，任一为 `null` 立即 return，
        // 所以「缺第二项」时第一项不会被先扣掉。
        if kind == SpellKind::PoisonCloud {
            let amulet = self.find_spell_material(
                object_id,
                POISON_CLOUD_MATERIAL_COUNT,
                POISON_CLOUD_AMULET_SHAPE,
            );
            let poison = self.find_spell_material(
                object_id,
                POISON_CLOUD_MATERIAL_COUNT,
                POISON_CLOUD_POISON_SHAPE,
            );
            let (Some(amulet_slot), Some(poison_slot)) = (amulet, poison) else {
                self.send_to_player(
                    object_id,
                    &ServerPacket::Magic(Magic {
                        spell,
                        target_id,
                        target: target_location,
                        cast: false,
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
                    cast: false,
                    level: magic.level,
                    self_broadcast: false,
                    secondary_target_ids: Vec::new(),
                })
                .encode_frame();
                self.broadcast(&map, position, Some(object_id), &packet, object_id);
                return Ok(CastReport {
                    spell,
                    level: magic.level,
                    target_id,
                    location: target_location,
                    damage: None,
                    heal: None,
                    buff: None,
                    cost,
                });
            };
            // 两项都通过预检才一起扣（B1 原语：`find` 只读 `&World`，`consume` 才拿
            // `&mut World`；中间没有 await/重入，两个槽位不会失效）。
            let consumed_amulet =
                self.consume_spell_material(object_id, amulet_slot, POISON_CLOUD_MATERIAL_COUNT);
            let consumed_poison =
                self.consume_spell_material(object_id, poison_slot, POISON_CLOUD_MATERIAL_COUNT);
            debug_assert!(
                consumed_amulet && consumed_poison,
                "find_spell_material 成功后 consume_spell_material 必须成功"
            );
        }

        // --- 困魔咒（`TrapHexagon`，`HumanObject.cs:4596-4644`） ----------------------
        // 顺序照抄 Crystal：先 3×2 预检（没有任何目标 → `cast = false` 直接 return），
        // 再 `GetAmulet(1)`（不足 → 同样 `cast = false`），再 `LevelMagic`、排 500ms
        // 延迟动作、扣耗材、`cast = true`。两条失败路径都在公共检查之后，所以 MP 已扣、
        // 动作/`SpellTime` 已锁，只是不写本技能 `CastTime`、不排效果。
        if kind == SpellKind::TrapHexagon {
            let caster_level = self.players[&object_id].level;
            let mut material = None;
            if self.trap_hexagon_precheck(&map, target_location, object_id, caster_level) {
                material = self.find_spell_material(
                    object_id,
                    TRAP_HEXAGON_AMULET_COUNT,
                    TRAP_HEXAGON_AMULET_SHAPE,
                );
            }
            let Some(amulet_slot) = material else {
                let report = self.refused_cast_report(RefusedCast {
                    object_id,
                    spell,
                    level: magic.level,
                    target_id,
                    target_location,
                    position,
                    map: &map,
                    direction,
                    cost,
                });
                return Ok(report);
            };
            let consumed =
                self.consume_spell_material(object_id, amulet_slot, TRAP_HEXAGON_AMULET_COUNT);
            debug_assert!(
                consumed,
                "find_spell_material 成功后 consume_spell_material 必须成功"
            );
            // `LevelMagic(magic)`（`HumanObject.cs:4632`）：施法成功即训练一次；
            // 落地 `train = true` 再训练一次（`Map.cs:2356`）。
            self.level_magic(object_id, spell);
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
            SpellKind::PoisonCloud => {
                // `PoisonCloud(magic, location)`（`HumanObject.cs:4543-4567`）：伤害
                // `magic.GetDamage(GetAttackPower(MinSC, MaxSC))` 在这里抽好；延迟
                // `MaxDistance(CurrentLocation, location) * 50 + 500`（`:4546`）；
                // 附伤 `(byte)Envir.Random.Next(Stats[Stat.PoisonAttack])`（`:4555`），
                // 抽签顺序在伤害之后（`DelayedAction(..., damage, location, bonus)`，
                // `:4556-4558`）。`PoisonAttack <= 0` 时 Crystal 的 `Random.Next(0)`
                // 返回 0 且**不消耗**随机数（见 [`Self::poison_attack_roll`]）。
                let base = stats.attack_power(stats.min_sc, stats.max_sc, &mut self.rng);
                let value = magic.damage(&info, base, &mut self.rng);
                damage = Some(value);
                let bonus_damage = self.poison_attack_roll(object_id);
                let travel = i64::from(position.max_distance(target_location))
                    * POISON_CLOUD_STEP_MS
                    + POISON_CLOUD_BASE_DELAY_MS;
                effects.push((
                    now + travel,
                    MagicEffect::PoisonCloud {
                        map: map.clone(),
                        location: target_location,
                        damage: value,
                        bonus_damage,
                    },
                ));
            }
            SpellKind::TrapHexagon => {
                // `TrapHexagon`：`DelayedAction(DelayedType.Magic, Envir.Time + 500, this,
                // magic, value, location)`（`HumanObject.cs:4634-4638`），`value =
                // (magic.Level * 5 + 10) * 1000`（`:4632`）。落地范围/中心怪重检在
                // [`Self::land_trap_hexagon`]。
                effects.push((
                    now + TRAP_HEXAGON_DELAY_MS,
                    MagicEffect::TrapHexagon {
                        map: map.clone(),
                        location: target_location,
                        duration_ms: trap_hexagon_duration_ms(magic.level),
                    },
                ));
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
            SpellKind::FrostCrunch => {
                // `Fireball(target, magic)`（`HumanObject.cs:3939-3952`）：伤害
                // `magic.GetDamage(GetAttackPower(MinMC, MaxMC))`，延迟
                // `MaxDistance * 50 + 500`；没有合法目标时照常施法但无效果。
                if target_id != 0 {
                    let base = stats.attack_power(stats.min_mc, stats.max_mc, &mut self.rng);
                    let value = magic.damage(&info, base, &mut self.rng);
                    damage = Some(value);
                    effects.push((
                        now + i64::from(position.max_distance(target_location))
                            * FROST_CRUNCH_STEP_MS
                            + FROST_CRUNCH_BASE_DELAY_MS,
                        MagicEffect::FrostCrunch {
                            target_id,
                            location: target_location,
                            damage: value,
                        },
                    ));
                }
            }
            SpellKind::ThunderStorm | SpellKind::StormEscape => {
                // `ThunderStorm(magic)`（`HumanObject.cs:4264-4270`）：以**施法时**
                // 的自己脚下为中心、500ms 后结算；`StormEscape` 复用同一段，
                // 另外在 750ms 后把落点处的传送排进队列（`:6091-6093`）。
                let base = stats.attack_power(stats.min_mc, stats.max_mc, &mut self.rng);
                let value = magic.damage(&info, base, &mut self.rng);
                damage = Some(value);
                effects.push((
                    now + THUNDER_STORM_DELAY_MS,
                    MagicEffect::ThunderStorm {
                        map: map.clone(),
                        location: position,
                        damage: value,
                    },
                ));
                if kind == SpellKind::StormEscape {
                    effects.push((
                        now + STORM_ESCAPE_DELAY_MS,
                        MagicEffect::StormEscape {
                            location: target_location,
                        },
                    ));
                }
            }
            SpellKind::Blink => {
                // `case Spell.Teleport: case Spell.Blink: ActionList.Add(new
                // DelayedAction(DelayedType.Magic, Envir.Time + 200, magic, location))`
                // （`HumanObject.cs:3537-3539`）；落点由 `CompleteMagic` 再校验。
                effects.push((
                    now + BLINK_DELAY_MS,
                    MagicEffect::Blink {
                        location: target_location,
                    },
                ));
            }
            SpellKind::ShoulderDash => {
                // `ShoulderDash(magic)` 是**同步**执行的：没有延迟动作，逐格移动与
                // 推送都在这里完成（`HumanObject.cs:4975-5157`）。
                // 前置失败（`!CanWalk`）时 Crystal 直接 `return;`，`Magic()` 也提前
                // 返回：不补 `S.MagicCast`、不记冷却（蓝已在 cost 处扣掉）。
                let Some(dealt) = self.cast_shoulder_dash(object_id, direction, magic.level, now)
                else {
                    return Ok(CastReport {
                        spell,
                        level: magic.level,
                        target_id: 0,
                        location: position,
                        damage: None,
                        heal: None,
                        buff: None,
                        cost,
                    });
                };
                damage = (dealt > 0).then_some(dealt);
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

        // 冲刺在 Crystal 里是 `Magic` 的**提前返回**分支：
        // `case Spell.ShoulderDash: ShoulderDash(magic); return;`（`HumanObject.cs:3558-3560`）
        // —— 所以既不发 `S.Magic` 也不发 `S.ObjectMagic`，只由 `ShoulderDash()` 末尾补
        // `magic.CastTime = now`（`:5152`）与 `S.MagicCast`（`:5153`）。
        if is_dash {
            if let Some(player) = self.players.get_mut(&object_id) {
                player.magic.casts.insert(spell, now);
            }
            self.send_to_player(
                object_id,
                &ServerPacket::MagicCast(MagicCast { spell }).encode_frame(),
            );
            return Ok(CastReport {
                spell,
                level: magic.level,
                target_id: 0,
                location: position,
                damage,
                heal: None,
                buff: None,
                cost,
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
            MagicEffect::PoisonCloud {
                map: field_map,
                location,
                damage,
                bonus_damage,
            } => {
                // `Map.cs:1434-1486`：`train = true`（落地即训练，不要求命中），逐格
                // 生成后 `CompleteMagic` 末尾 `if (train) LevelMagic(magic)`（`:2356`）。
                //
                // D3 偏差：Crystal 的 `DelayedAction` 挂在施法时那张图上，落点对象
                // 仍以 `Caster = player` 生成；玩家中途换图/下线时毒雾照常生成、留到
                // `ExpireTime` 才消失（`SpellObject.cs:47-51`、`:66`）。本作在 pending
                // 阶段就丢弃：施法者已离线（上游已 return）或已换图 → 什么都不做。
                let caster_on_map = self
                    .players
                    .get(&action.caster_id)
                    .is_some_and(|caster| caster.map == field_map);
                if !caster_on_map {
                    return;
                }
                self.level_magic(action.caster_id, action.spell);
                self.create_poison_cloud(
                    action.caster_id,
                    &field_map,
                    location,
                    damage,
                    bonus_damage,
                    now,
                );
            }
            MagicEffect::TrapHexagon {
                map: field_map,
                location,
                duration_ms,
            } => {
                // `Map.cs:1752-1832`：落地时以落点为中心再做一次 3×3 重检（施法前的
                // 预检是 3×2，不含 `y + 1` 那行），中心怪确定后才铺 8 个外围场。
                //
                // D3 偏差：Crystal 的 `DelayedAction` 挂在施法时那张图的 `ActionList`
                // 上，只查 `player.Node != null`（`Map.cs:856-866`），施法者中途换图时
                // 场仍会在旧图生成；本作沿用 B2 毒雾的口径：施法者已换图/离线则丢弃。
                let caster_on_map = self
                    .players
                    .get(&action.caster_id)
                    .is_some_and(|caster| caster.map == field_map);
                if caster_on_map {
                    self.land_trap_hexagon(
                        action.caster_id,
                        &field_map,
                        location,
                        duration_ms,
                        now,
                    );
                }
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
            MagicEffect::ThunderStorm {
                map: cast_map,
                location,
                damage,
            } => {
                // `Map.cs:1303-1341` 的 `Map.ThunderStorm`：落点 ±2 的 5×5，逐个目标
                // `value / 10`（非亡灵）；每次命中都 `LevelMagic`（`:1377`）。
                // 结算用**施法瞬间的地图**（Crystal 把该动作挂在 `CurrentMap.ActionList`，
                // `HumanObject.cs:4268-4269`），施法后换图也打不到新图（P05）。
                if self.resolve_thunder_storm(
                    action.caster_id,
                    &cast_map,
                    location,
                    damage,
                    action.spell,
                ) {
                    self.level_magic(action.caster_id, action.spell);
                }
            }
            MagicEffect::StormEscape { location } => {
                // `HumanObject.cs:6080-6092`：NoTeleport 提示 → `Random.Next(4) >= Level+1`
                // 直接无功而返 → 传送成功才广播 `S.ObjectEffect{StormEscape}`，
                // 并严格按原作顺序：**先 AddBuff(TemporalFlux, 30s) 再 LevelMagic**。
                if self.storm_escape_caster(action.caster_id, location) {
                    self.add_temporal_flux(action.caster_id);
                    self.level_magic(action.caster_id, action.spell);
                }
            }
            MagicEffect::Blink { location } => {
                // `HumanObject.cs:6114-6131`：与 StormEscape 同构，效果码是 `Teleport`；
                // 原作顺序是 **广播光效 → LevelMagic → AddBuff(TemporalFlux)**。
                if self.blink_caster(action.caster_id, location) {
                    self.level_magic(action.caster_id, action.spell);
                    self.add_temporal_flux(action.caster_id);
                }
            }
            MagicEffect::FrostCrunch {
                target_id,
                location,
                damage,
            } => {
                // `HumanObject.cs:5964-5995`：`Attacked(..., MAC)` 结算伤害，
                // 再按等级概率叠 Slow / Frozen 毒。
                if self.resolve_frost_crunch(
                    action.caster_id,
                    &map,
                    target_id,
                    location,
                    damage,
                    now,
                ) {
                    self.level_magic(action.caster_id, action.spell);
                }
            }
            MagicEffect::Teleport => {
                // `HumanObject.cs:6097-6110`：`MagicTeleport` 成功 → **先 AddBuff(TemporalFlux)
                // 再 LevelMagic**（只有 `Spell.Teleport` 走这个分支；Blink / StormEscape
                // 在各自的分支里按规范顺序调用 `add_temporal_flux`）。
                if self.teleport_caster(action.caster_id) {
                    self.add_temporal_flux(action.caster_id);
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
                bonus_damage: 0,
                cast_location: cell,
                show: true,
                // 火墙不绑怪（Crystal `SpellObject.Target` 为 null）。
                target_monster: 0,
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
    ///
    /// 毒雾（`PoisonCloud`）的 D3 偏差已登记：Crystal 里法师下线只会把 `Caster` 置
    /// `null`（`SpellObject.cs:47-51`），毒雾**留在场上直到 `ExpireTime`**、只是
    /// `IsAttackTarget(null)` 拒绝而停止伤害（`SpellObject.cs:150-163`）；换图强清也
    /// 只对火墙生效（`SpellObject.cs:78-86`）。本作对两者统一「施法者离线/换图即清场」，
    /// 这是 R52 安全化偏差（PARTIAL），与 Crystal 不完全等价。
    ///
    /// 困魔咒（`TrapHexagon`）不套用「施法者离线即清场」：Crystal 的
    /// `SpellObject.Process` 只在 `Caster == null` 时撤销火墙家族
    /// （`SpellObject.cs:66-72`），困魔咒的场留到 `ExpireTime`；提前撤销的唯一条件是
    /// 绑定的中心怪 `ShockTime == 0`（`SpellObject.cs:73-78`），本作据此判断中心怪
    /// 死亡/换图/Shock 到期。
    pub(crate) fn tick_spell_fields(&mut self, now: i64) {
        let mut index = 0;
        while index < self.spell_fields.len() {
            let field = self.spell_fields[index].clone();
            let caster_gone = field.spell != Spell::TrapHexagon
                && self
                    .players
                    .get(&field.caster_id)
                    .is_none_or(|caster| caster.map != field.map);
            // Crystal 到期用严格 `Envir.Time > ExpireTime`（`SpellObject.cs:66`）：
            // 毒雾 6000ms 那一跳仍然结算，6000 之后的下一个 world tick 才移除。
            let expired = if field.spell == Spell::PoisonCloud || field.spell == Spell::TrapHexagon
            {
                now > field.expire_at
            } else {
                now >= field.expire_at
            };
            // 困魔咒绑定中心怪：`Envir.Time < ExpireTime && ob.ShockTime != 0` 才留场
            // （`SpellObject.cs:73-78`），所以中心怪消失/死亡/换图/Shock 提前结束都撤销。
            let shock_released = field.spell == Spell::TrapHexagon
                && !self.monster(field.target_monster).is_some_and(|monster| {
                    !monster.dead && monster.map == field.map && now < monster.shock_time
                });
            if expired || caster_gone || shock_released {
                // 撤销广播只对「客户端真的收到过这个对象」的场发生：
                // * 火墙沿用 B2 行为，每格生成时都广播、到期逐格撤销；
                // * 毒雾 3×3 只有首个 `show == true` 的对象发给客户端，其余 8 个内部场
                //   不能广播撤销（否则客户端会收到未知 ID 的 `ObjectRemove`，对应
                //   Crystal `SpellObject.cs:509` 的 `Show` 门槛）；
                // * 困魔咒 8 个外围场全部 `show == true`。
                if field.spell == Spell::FireWall || spell_field_visible_to_client(&field) {
                    let packet = ServerPacket::ObjectRemove(ObjectRemove {
                        object_id: field.object_id,
                    })
                    .encode_frame();
                    // 撤销广播要发在**客户端可见位置**上：毒雾的可见对象在落点中心
                    // （`CastLocation`），不是它 3×3 里那个内部格（Crystal `SpellObject.cs:509`、
                    // `:522-530`）。
                    self.broadcast(
                        &field.map,
                        spell_field_view_location(&field),
                        None,
                        &packet,
                        field.object_id,
                    );
                }
                // 已见集合同步清掉：之后谁再进视野/原地移动都不会再补发或再撤销。
                self.forget_spell_field(field.object_id);
                self.spell_fields.remove(index);
                continue;
            }
            if now >= field.next_tick {
                self.spell_fields[index].next_tick = now + field.tick_ms;
                match field.spell {
                    Spell::PoisonCloud => self.apply_poison_cloud_tick(&field, now),
                    // 纯视觉/状态场，没有周期伤害（Crystal `SpellObject.ProcessSpell`
                    // 的 `TrapHexagon` 也是空实现）。
                    Spell::TrapHexagon => {}
                    _ => self.apply_fire_wall_tick(&field),
                }
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

    /// `cast = false` 的统一回显路径（材料不足 / 预检无目标）。
    ///
    /// Crystal 的耗材分支失败后 `cast` 保持 `false`，但 `S.Magic` 与 `S.ObjectMagic`
    /// 仍然照发（`HumanObject.cs:3773-3779`）；公共检查的副作用（MP、`ActionTime`、
    /// `AttackTime`、`SpellTime`、`Direction`）都已经发生，不在这里回滚。
    fn refused_cast_report(&mut self, refused: RefusedCast<'_>) -> CastReport {
        let RefusedCast {
            object_id,
            spell,
            level,
            target_id,
            target_location,
            position,
            map,
            direction,
            cost,
        } = refused;
        self.send_to_player(
            object_id,
            &ServerPacket::Magic(Magic {
                spell,
                target_id,
                target: target_location,
                cast: false,
                level,
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
            cast: false,
            level,
            self_broadcast: false,
            secondary_target_ids: Vec::new(),
        })
        .encode_frame();
        self.broadcast(map, position, Some(object_id), &packet, object_id);
        CastReport {
            spell,
            level,
            target_id,
            location: target_location,
            damage: None,
            heal: None,
            buff: None,
            cost,
        }
    }

    /// 困魔咒施法前的 **3×2** 预检（`HumanObject.cs:4596-4622`）。
    ///
    /// 范围：`x ∈ [location.X - 1, location.X + 1]`、`y ∈ [location.Y - 1, location.Y]`，
    /// 即 3 列 × 2 行（**不含 `y + 1` 那行**，第三行只可能在落地重检时被扫到）。
    /// 每格必须可走（`CurrentMap.ValidPoint`），格内怪物必须能被打
    /// （`IsAttackTarget(this)`）且 `Level <= caster.Level + 2`。
    fn trap_hexagon_precheck(
        &self,
        map: &str,
        location: Point,
        caster_id: u32,
        caster_level: u16,
    ) -> bool {
        for dx in -1..=1 {
            for dy in -1..=0 {
                let cell = Point::new(location.x + dx, location.y + dy);
                if !self.walkable_on(map, cell) {
                    continue;
                }
                for monster_id in self.monsters_on_cell(map, cell) {
                    let Some(monster) = self.monster(monster_id) else {
                        continue;
                    };
                    if monster.dead || monster.kind.level > caster_level + TRAP_HEXAGON_LEVEL_MARGIN
                    {
                        continue;
                    }
                    if self.monster_is_attack_target_of_player(caster_id, monster_id) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// 困魔咒落地（Crystal `Map.CompleteMagic` 的 `TrapHexagon` 区域，`Map.cs:1752-1832`）。
    ///
    /// * 以落点为中心 **3×3** 重检（外层 y、内层 x，越界跳过；格必须可走）；
    /// * 中心格上的**任何**怪先记作 `centerTarget`（Crystal 不检查可攻击/等级，
    ///   `:1770-1772`，同格后出现的对象覆盖前一个）；
    /// * 其他格：`IsAttackTarget(player)`、未死、`Level <= player.Level + 2` 的怪
    ///   写入 `ShockTime = Envir.Time + duration` 并清空 `Target`；`centerTarget` 还没
    ///   确定时取第一只这样的怪（`:1774-1791`）；
    /// * 没有任何中心怪 → 直接返回（不生成场、不训练，`:1793`）；
    /// * 以落点为中心生成 8 个外围场（`:1797-1829`），每个场绑定中心怪、
    ///   `ExpireTime = now + duration`；`Show` 恒 `true`（`SpellObject.GetInfo` 的
    ///   `default` 分支，`SpellObject.cs:553-560`）；
    /// * `train = true` → `LevelMagic`（`Map.cs:2356`）。
    ///
    /// D2 差异（已批准）：Crystal 的边界判断是 `spawnpoint.X <= 0 || X > Width`
    /// （`Map.cs:1808-1809`），会跳过 `x = 0`、放行 `x == Width`；本作按合法地图坐标
    /// 安全剪裁——`in_bounds` 不过的点直接跳过。
    fn land_trap_hexagon(
        &mut self,
        caster_id: u32,
        map: &str,
        location: Point,
        duration_ms: i64,
        now: i64,
    ) {
        let Some(caster_level) = self.players.get(&caster_id).map(|caster| caster.level) else {
            return;
        };
        let mut center: Option<u32> = None;
        let mut shocked: Vec<u32> = Vec::new();
        for dy in -1..=1 {
            for dx in -1..=1 {
                let cell = Point::new(location.x + dx, location.y + dy);
                if !self.walkable_on(map, cell) {
                    continue;
                }
                for monster_id in self.monsters_on_cell(map, cell) {
                    let Some(monster) = self.monster(monster_id) else {
                        continue;
                    };
                    if monster.dead || monster.map != map {
                        continue;
                    }
                    if dy == 0 && dx == 0 {
                        center = Some(monster_id);
                    }
                    if monster.kind.level > caster_level + TRAP_HEXAGON_LEVEL_MARGIN {
                        continue;
                    }
                    if !self.monster_is_attack_target_of_player(caster_id, monster_id) {
                        continue;
                    }
                    if center.is_none() {
                        center = Some(monster_id);
                    }
                    shocked.push(monster_id);
                }
            }
        }
        let Some(center_id) = center else {
            return;
        };
        for monster_id in shocked {
            if let Some(monster) = self.monster_mut(monster_id) {
                // `mobTarget.ShockTime = Envir.Time + value; mobTarget.Target = null;`
                // （`Map.cs:1785-1786`）。宠物用独立的 `pet_target`（`MonsterObject.cs:1928-1955`），
                // 落地这一刻同样要清，否则被 Shock 的宠物还留着旧仇恨目标。
                monster.shock_time = now + duration_ms;
                monster.target = None;
                monster.pet_target = None;
            }
        }
        // 八外围场：`for (i = 0; i < 8; i += 2)`（Up/Right/Down/Left）再
        // `for (j = 0; j <= 4; j += 4)`：`PointMove(location, i, 2)` 后沿
        // `Right`/`Up` 再走 1 格（`j` 加到方向枚举上：`Right + 4 = Left`、
        // `Up + 4 = Down`），展开即 [`TRAP_HEXAGON_FIELD_POINTS`]（`Map.cs:1797-1815`）。
        for (dx, dy) in TRAP_HEXAGON_FIELD_POINTS {
            let spawn = Point::new(location.x + dx, location.y + dy);
            if !self.in_bounds(map, spawn) {
                continue;
            }
            let object_id = self.allocate_object_id();
            let field = SpellField {
                object_id,
                map: map.to_owned(),
                center: spawn,
                radius: 0,
                damage: 0,
                caster_id,
                expire_at: now + duration_ms,
                // 没有周期伤害：`next_tick = i64::MAX` 让 tick 永远不结算
                // （Crystal 的 `ProcessSpell` 对 `TrapHexagon` 也是空实现）。
                next_tick: i64::MAX,
                tick_ms: TRAP_HEXAGON_FIELD_TICK_MS,
                spell: Spell::TrapHexagon,
                direction: MirDirection::Up,
                cast_id: 0,
                bonus_damage: 0,
                cast_location: location,
                show: true,
                target_monster: center_id,
            };
            let frame = ServerPacket::ObjectSpell(ObjectSpell {
                object_id,
                // `SpellObject.GetInfo` 的 `default` 分支用 `CurrentLocation`
                // （`SpellObject.cs:553-560`）：8 个场各自在自己的格子上可见。
                location: spawn,
                spell: Spell::TrapHexagon,
                direction: MirDirection::Up,
                param: false,
            })
            .encode_frame();
            self.broadcast(map, spawn, None, &frame, object_id);
            // 生成即同步「已见集合」：刚收到 ObjectSpell 的观察者不该在下次移动时
            // 再被补发一次（R2；C 批复用同一差分状态）。
            self.mark_spell_field_visible(&field);
            self.spell_fields.push(field);
        }
        self.level_magic(caster_id, Spell::TrapHexagon);
    }

    /// 毒雾落地（Crystal `Map.CompleteMagic` 的 `PoisonCloud` 区域，`Map.cs:1434-1486`）：
    /// 以落点为中心的 3×3 逐格生成毒云，`6000ms` 内每 `1000ms` 一跳（这里用
    /// [`POISON_CLOUD_EXPIRE_MS`] / [`POISON_CLOUD_TICK_MS`]）。同图同格已有同类
    /// （`Spell.PoisonCloud`、半径 0）的对象则跳过、不叠加（`Map.cs:1455-1466`）。
    ///
    /// `Show` 语义（Crystal `SpellObject.GetInfo`，`SpellObject.cs:490-530`）：9 格里只有
    /// **首个实际生成**的对象 `Show = true`，并且只有它广播一条 `S.ObjectSpell`
    /// （客户端看到的是 `CastLocation` 落点）；其余对象只登记为服务端场、不发包。
    fn create_poison_cloud(
        &mut self,
        caster_id: u32,
        map: &str,
        location: Point,
        damage: i32,
        bonus_damage: i32,
        now: i64,
    ) {
        // Crystal 用外层 y、内层 x 的遍历（`Map.cs:1447-1477`），首个真正生成的对象
        // 之后 `show` 立刻置 false（`:1461`），后续只创建不广播。
        let mut show = true;
        for dy in -1..=1 {
            for dx in -1..=1 {
                let cell = Point::new(location.x + dx, location.y + dy);
                // Crystal `if (!cell.Valid) continue;`（`Map.cs:1442-1444`），而
                // `Valid => Attribute == CellAttribute.Walk`（`Map.cs:2480-2482`）：
                // 不是单纯的坐标合法——**墙格不生成毒云**，首个有效格才 `Show = true`。
                // 与 [`Self::walkable_on`] 的区别：后者额外挡攻城对象
                // （`conquest_object_blocking`），那是 Crystal 的对象碰撞语义，
                // 不参与 `cell.Valid`，所以这里单独用 [`Self::poison_cloud_cell_valid`]。
                if !self.poison_cloud_cell_valid(map, cell) {
                    continue;
                }
                let occupied = self.spell_fields.iter().any(|field| {
                    field.map == map
                        && field.spell == Spell::PoisonCloud
                        && field.radius == 0
                        && field.center == cell
                });
                if occupied {
                    continue;
                }
                let object_id = self.allocate_object_id();
                let first = show;
                let field = SpellField {
                    object_id,
                    map: map.to_owned(),
                    center: cell,
                    radius: 0,
                    damage,
                    caster_id,
                    expire_at: now + POISON_CLOUD_EXPIRE_MS,
                    // Crystal 的 `TickTime` 初值 0 → 落场后第一次 `Process` 即首跳
                    // （`SpellObject.cs:111-112`）。
                    next_tick: now,
                    tick_ms: POISON_CLOUD_TICK_MS,
                    spell: Spell::PoisonCloud,
                    direction: MirDirection::Up,
                    cast_id: 0,
                    bonus_damage,
                    cast_location: location,
                    show: first,
                    // 毒雾不绑怪（Crystal `SpellObject.Target` 为 null）。
                    target_monster: 0,
                };
                if first {
                    let frame = ServerPacket::ObjectSpell(ObjectSpell {
                        object_id,
                        // `SpellObject.cs:509`、`:522-530`：Show 对象的位置是 `CastLocation`。
                        location,
                        spell: Spell::PoisonCloud,
                        direction: MirDirection::Up,
                        param: false,
                    })
                    .encode_frame();
                    self.broadcast(map, location, None, &frame, object_id);
                    show = false;
                }
                // 只有 `show == true` 的首对象对客户端可见；生成即登记已见集合，
                // 避免视野内玩家移动时重复补发（R2；B2 的 9 格内部场永不可见）。
                self.mark_spell_field_visible(&field);
                self.spell_fields.push(field);
            }
        }
        // `show` 字段落库时已逐个写好：首对象 true、其余 false（见上）。
    }

    /// Crystal `cell.Valid`（`Map.cs:2480-2482`）：坐标在界内且格属性为
    /// `CellAttribute::Walk`。毒雾落地的有效格过滤（`Map.cs:1442-1444`）用的就是它。
    ///
    /// 没有地图数据时退化为边界判定（与 [`Self::in_bounds`] 一致），
    /// 与 [`Self::walkable_on`] 的差异见 `create_poison_cloud` 的注释。
    fn poison_cloud_cell_valid(&self, map: &str, cell: Point) -> bool {
        if !self.in_bounds(map, cell) {
            return false;
        }
        self.map_of(map)
            .is_none_or(|data| data.walkable(cell.x, cell.y))
    }

    /// 毒雾一次 tick（Crystal `SpellObject.ProcessSpell` 的 `PoisonCloud` 分支，
    /// `SpellObject.cs:150-163`）：
    ///
    /// 1. 中心格内玩家/怪物先吃 `DefenceType.MAC` 伤害（`Value`，施法时抽好的值）；
    /// 2. 目标**存活**才再 `ApplyPoison` 绿毒，毒量 = `(当前 MinSC + MaxSC) / 2 + BonusDmg`
    ///    （`SpellObject.cs:166-174`），跳数 [`POISON_CLOUD_POISON_DURATION`]、
    ///    每跳间隔与毒云同（`TickSpeed`）。
    ///
    /// 玩家目标用 [`World::is_attack_target_player`]（PK/安全区/攻击模式），怪物目标用
    /// [`World::monster_is_attack_target_of_player`]（Crystal `MonsterObject.IsAttackTarget`，
    /// `MonsterObject.cs:2430-2463`）。施法者自己不会被自己的毒雾命中（Crystal 同格
    /// `Race == Player` 分支先判 `ob != Caster`）。
    fn apply_poison_cloud_tick(&mut self, field: &SpellField, now: i64) {
        let Some(caster_stats) = self
            .players
            .get(&field.caster_id)
            .map(|caster| caster.stats)
        else {
            return;
        };
        // Crystal `Value = (Caster.Stats[MinSC] + Caster.Stats[MaxSC]) / 2 + BonusDmg`
        // （`SpellObject.cs:173`）；整数除法向零截断，SC 恒为正，无符号差异。
        let poison_value = (caster_stats.min_sc + caster_stats.max_sc) / 2 + field.bonus_damage;
        for entry in self.aoi_in_radius(&field.map, field.center, field.radius) {
            match entry.kind {
                AoiKind::Player => {
                    let target_id = entry.object_id;
                    if target_id == field.caster_id {
                        continue;
                    }
                    let hittable = self.players.get(&target_id).is_some_and(|player| {
                        !player.dead && player.map == field.map && player.position == field.center
                    });
                    if !hittable || !self.is_attack_target_player(field.caster_id, target_id) {
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
                    // `if (!ob.Dead) ob.ApplyPoison(...)`（`SpellObject.cs:165-174`）。
                    let survived = self
                        .players
                        .get(&target_id)
                        .is_some_and(|player| !player.dead);
                    if survived {
                        let poison = PoisonState::new(
                            PoisonType::Green,
                            poison_value,
                            POISON_CLOUD_POISON_DURATION,
                            now,
                            field.caster_id,
                        )
                        .with_tick_speed(field.tick_ms);
                        self.apply_poison_to_player(target_id, poison);
                    }
                }
                AoiKind::Monster => {
                    let target_id = entry.object_id;
                    let hittable = self.monster(target_id).is_some_and(|monster| {
                        !monster.dead
                            && monster.map == field.map
                            && monster.position == field.center
                    }) && self
                        .monster_is_attack_target_of_player(field.caster_id, target_id);
                    if !hittable {
                        continue;
                    }
                    self.damage_magic_target(field.caster_id, target_id, field.damage);
                    let survived = self.monster(target_id).is_some_and(|monster| !monster.dead);
                    if survived {
                        let poison = PoisonState::new(
                            PoisonType::Green,
                            poison_value,
                            POISON_CLOUD_POISON_DURATION,
                            now,
                            field.caster_id,
                        )
                        .with_tick_speed(field.tick_ms);
                        self.apply_poison_to_monster(target_id, poison);
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
        true
    }

    // ==== 第 51 轮：Blink / StormEscape / FrostCrunch / ThunderStorm / ShoulderDash ====

    /// 飞行道具的弹道检查（Crystal `MapObject.CanFly`，`MapObject.cs:321-337`）。
    ///
    /// 从 `origin` 沿 `DirectionFromPoint` 逐格走到 `target`；出界或 `cell.Valid == false`
    /// 即失败。与 Crystal 一样**不检查**中间格上的对象（飞行道具会穿过人物/怪物）。
    fn can_fly(&self, map: &str, origin: Point, target: Point) -> bool {
        let mut location = origin;
        while location != target {
            location = location.moved(location.direction_to(target), 1);
            if !self.walkable_on(map, location) {
                return false;
            }
        }
        true
    }

    /// 目标格是否落在安全区（Crystal `Map.GetSafeZone(Point)`，`Map.cs:2386-2395`）。
    ///
    /// 与 [`World::in_safe_zone`] 同一套数据与判定（切比雪夫距离 ≤ `Size`），只是入参是格。
    fn safe_zone_at(&self, map: &str, cell: Point) -> bool {
        self.game_data()
            .and_then(|data| data.map_info(map))
            .is_some_and(|info| {
                info.safe_zones.iter().any(|zone| {
                    let size = i32::from(zone.size);
                    (cell.x - zone.location.x).abs() <= size
                        && (cell.y - zone.location.y).abs() <= size
                })
            })
    }

    /// 地图禁止传送时发系统提示并返回 `true`
    /// （Crystal `CurrentMap.Info.NoTeleport` → `YouCannotTeleportOnMap`，
    /// `HumanObject.cs:6082-6086`、`:6100-6104`、`:6117-6121`）。
    fn teleport_forbidden(&mut self, caster_id: u32, map: &str) -> bool {
        let forbidden = self
            .game_data()
            .and_then(|data| data.map_info(map))
            .is_some_and(|info| info.no_teleport);
        if forbidden {
            let message = self.text(caster_id, TextKey::MagicTeleportForbidden);
            self.send_system_chat(caster_id, &message);
        }
        forbidden
    }

    /// 把施法者传送到**指定落点**，广播 `S.ObjectTeleportOut/In`，成功后加 30 秒传送法力惩罚
    /// （Crystal `MapObject.Teleport`，`MapObject.cs:780-820`；惩罚见 `HumanObject.cs:6090`、`:6127`）。
    fn teleport_caster_to(&mut self, caster_id: u32, landing: Point) -> bool {
        let Some((map, position, direction)) = self
            .players
            .get(&caster_id)
            .map(|player| (player.map.clone(), player.position, player.direction))
        else {
            return false;
        };
        if !self.walkable_on(&map, landing) {
            return false;
        }
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
        true
    }

    /// 广播一次法术光效（Crystal `S.ObjectEffect`；`MapObject.cs:408-420`）。
    ///
    /// Crystal 的 `Map.Broadcast` 把包发给**视野内所有玩家，包括施法者本人**；
    /// 本作 `World::broadcast` 默认跳过 `self_id`，因此这里先单独给本人发一份
    /// （P06 审计点：之前漏发，客户端看不到自己身上的传送/雷击光效）。
    fn broadcast_spell_effect(
        &mut self,
        map: &str,
        position: Point,
        object_id: u32,
        effect: SpellEffect,
    ) {
        let packet = ServerPacket::ObjectEffect(ObjectEffect {
            object_id,
            effect,
            effect_type: 0,
            delay_time: 0,
            time: 0,
        })
        .encode_frame();
        self.send_to_player(object_id, &packet);
        self.broadcast(map, position, Some(object_id), &packet, 0);
    }

    /// 传送后的 `TemporalFlux` 增益（Crystal `AddBuff(BuffType.TemporalFlux, 30s)`，
    /// `HumanObject.cs:6106`、`:6127`、`:6135`）：30 秒内传送类技能多耗 30% 蓝
    /// （`MagicCost`，`:3379-3385`）。增益到期由 [`crate::buffs::World::tick_buffs`]
    /// 移除并同步 `RemoveBuff`，耗蓝随之恢复正常。
    fn add_temporal_flux(&mut self, object_id: u32) {
        if !self.add_buff(
            object_id,
            BuffKind::TemporalFlux,
            TELEPORT_PENALTY_MS,
            Stats::default(),
            object_id,
        ) {
            return;
        }
        let Some(player) = self.players.get_mut(&object_id) else {
            return;
        };
        // `Stat.TeleportManaPenaltyPercent` 的数值挂在增益上（`values[0]`），
        // `MagicCost` 通过 `buff_value` 读取（见 `cast_magic` 的蓝量段）。
        if let Some(buff) = player
            .buffs
            .iter_mut()
            .find(|buff| buff.kind == BuffKind::TemporalFlux)
        {
            buff.values = vec![TELEPORT_MANA_PENALTY_PERCENT];
        }
    }

    /// 闪现落地（Crystal `CompleteMagic` 的 `case Spell.Blink`，`HumanObject.cs:6114-6129`）：
    /// `NoTeleport` → 提示；`InRange(CurrentLocation, location, Info.Range)` 失败 → 静默；
    /// `!ValidPoint || Random.Next(4) >= Level + 1 || !Teleport(...)` → 静默；
    /// 成功才广播 `SpellEffect.Teleport` 并加 30 秒传送法力惩罚。返回是否落地。
    fn blink_caster(&mut self, caster_id: u32, location: Point) -> bool {
        let Some((map, position)) = self
            .players
            .get(&caster_id)
            .map(|player| (player.map.clone(), player.position))
        else {
            return false;
        };
        if self.teleport_forbidden(caster_id, &map) {
            return false;
        }
        let range = self
            .magic_info(Spell::Blink)
            .map_or(0, |info| i32::from(info.range));
        if !position.in_range(location, range) {
            return false;
        }
        let level = self
            .player_magic(caster_id, Spell::Blink)
            .map_or(0, |magic| i32::from(magic.level));
        if self.rng.next_range(0, 3) > level {
            return false;
        }
        if !self.teleport_caster_to(caster_id, location) {
            return false;
        }
        self.broadcast_spell_effect(&map, location, caster_id, SpellEffect::Teleport);
        true
    }

    /// 狂风绝命落地（Crystal `CompleteMagic` 的 `case Spell.StormEscape`，`HumanObject.cs:6079-6092`）：
    /// 与闪现同构，但光效是 `SpellEffect.StormEscape`；**不再**校验与施法点的距离。
    fn storm_escape_caster(&mut self, caster_id: u32, location: Point) -> bool {
        let Some(map) = self
            .players
            .get(&caster_id)
            .map(|player| player.map.clone())
        else {
            return false;
        };
        if self.teleport_forbidden(caster_id, &map) {
            return false;
        }
        let level = self
            .player_magic(caster_id, Spell::StormEscape)
            .map_or(0, |magic| i32::from(magic.level));
        if self.rng.next_range(0, 3) > level {
            return false;
        }
        if !self.teleport_caster_to(caster_id, location) {
            return false;
        }
        self.broadcast_spell_effect(&map, location, caster_id, SpellEffect::StormEscape);
        true
    }

    /// 施法者装备/增益汇总的 `Stat.Freezing`（Crystal `Stats[Stat.Freezing]`，
    /// 由 `HumanObject.RefreshStats` 把所有装备模板属性、装备附加属性与生效 buff
    /// 的同名属性加起来：`HumanObject.cs:1858-1859` 的
    /// `Stats.Add(realItem.Stats)` + `Stats.Add(temp.AddedStats)`（普通装备的模板 + 附加）、
    /// `:1965-1966` 的 `RefreshSocketStats` 插槽模板 + 附加、以及 `:2332` 的 `RefreshBuffs`）。
    ///
    /// 本作的 `CombatStats` 还没有 `freezing` 字段（`combat.rs` 不在本轮写范围），
    /// 所以这里按 `RefreshStats` 的加法顺序直接汇总：
    /// ① 每件装备的模板属性 `ItemInfo.stats`（`:1858`）→ ② 其附加属性
    /// `UserItem.added_stats`（同一 `:1859`）→ ③ 玩家当前生效增益的 `ActiveBuff.stats`
    /// （`:2332` RefreshBuffs）。缺省 0，语义与 Crystal 一致
    /// （`Random.Next(0)` 恒为 0 → 时长 5，`HumanObject.cs:5987`）。
    ///
    /// 未覆盖：行会/其它系统级加成（本作没有 Freezing 表示，保持现状）。
    fn caster_freezing(&self, caster_id: u32) -> i32 {
        self.players.get(&caster_id).map_or(0, |player| {
            let mut total = 0;
            for item in player.equipment.iter().flatten() {
                let Some(info) = self.item_info(item.item_index) else {
                    continue;
                };
                // 损坏装备不贡献任何属性：Crystal `HumanObject.cs:1836` 在叠
                // `Stats.Add(realItem.Stats)`/`Stats.Add(temp.AddedStats)`（`:1858-1859`）
                // **之前**就跳过 `CurrentDura == 0 && Info.Durability > 0` 的装备；
                // 与 `items.rs:912-915 apply_equipment_stats` 同一语义。
                if item.current_dura == 0 && info.durability > 0 {
                    continue;
                }
                // ① 装备模板属性（Crystal `realItem.Stats`，`HumanObject.cs:1858`）。
                total += info.stats.get(Stat::Freezing);
                // ② 附加属性（Crystal `temp.AddedStats`，`HumanObject.cs:1859`；
                //    插槽属性在 `:1965-1966` 的 `RefreshSocketStats`，不在此列）。
                total += item.added_stats.get(Stat::Freezing);
            }
            // ③ 生效增益属性（Crystal `RefreshBuffs`，`HumanObject.cs:2332`）。
            total += player
                .buffs
                .iter()
                .map(|buff| buff.stats.get(Stat::Freezing))
                .sum::<i32>();
            total
        })
    }

    /// 冰咆哮 Frozen 时长的额外随机：`Random.Next(Stats[Stat.Freezing])`
    /// （`HumanObject.cs:5987`）。Crystal 的 `Random.Next(0)` 恒返回 0 且不消耗随机数，
    /// 这里同样在 `freezing <= 0` 时不做抽取，保证 RNG 序列一致。
    fn freeze_roll(&mut self, caster_id: u32) -> i32 {
        let freezing = self.caster_freezing(caster_id);
        if freezing > 0 {
            self.rng.next_range(0, freezing - 1)
        } else {
            0
        }
    }

    /// 施法者装备/增益汇总的 `Stat.PoisonAttack`（Crystal `Stats[Stat.PoisonAttack]`，
    /// 由 `HumanObject.RefreshStats` 在 `RefreshEquipmentStats`（`:1825-1915`）与
    /// `RefreshBuffs`（`:2326-2333`）里把装备模板属性 + 附加属性 + 生效增益的同名属性
    /// 累加得到）。
    ///
    /// 与 [`Self::caster_freezing`] 同构，原因相同：本作的 `CombatStats` 没有
    /// `poison_attack` 字段（`combat.rs` 不在 B2b 写范围），所以按 `RefreshStats`
    /// 的加法顺序就地汇总：① 每件装备的模板属性 `ItemInfo.stats`（`:1858`）→
    /// ② 其附加属性 `UserItem.added_stats`（同一 `:1859`）→ ③ 玩家当前生效增益的
    /// `ActiveBuff.stats`（`:2332`）。损坏装备（`CurrentDura == 0 && Info.Durability > 0`）
    /// 不贡献属性（`:1836`，与 `items.rs:912-915` 同语义）。
    fn caster_poison_attack(&self, caster_id: u32) -> i32 {
        self.players.get(&caster_id).map_or(0, |player| {
            let mut total = 0;
            for item in player.equipment.iter().flatten() {
                let Some(info) = self.item_info(item.item_index) else {
                    continue;
                };
                if item.current_dura == 0 && info.durability > 0 {
                    continue;
                }
                total += info.stats.get(Stat::PoisonAttack);
                total += item.added_stats.get(Stat::PoisonAttack);
            }
            total += player
                .buffs
                .iter()
                .map(|buff| buff.stats.get(Stat::PoisonAttack))
                .sum::<i32>();
            total
        })
    }

    /// 毒雾附伤：`(byte)Envir.Random.Next(Stats[Stat.PoisonAttack])`（`HumanObject.cs:4555`）。
    ///
    /// Crystal 的 `Random.Next(0)` 返回 0 且**不推进随机数状态**，这里在
    /// `poison_attack <= 0` 时同样不抽取（[`MirRng::next_bounded`]/[`MirRng::next_range`]
    /// 的 `max <= min` 分支本来就不耗状态，但显式早退更清楚）。
    fn poison_attack_roll(&mut self, caster_id: u32) -> i32 {
        let poison_attack = self.caster_poison_attack(caster_id);
        if poison_attack > 0 {
            self.rng.next_range(0, poison_attack - 1)
        } else {
            0
        }
    }

    /// 冰咆哮：伤害 + 等级概率的 Slow/Frozen 毒（Crystal `HumanObject.cs:5962-5995`）。
    ///
    /// 1. 目标必须仍可命中（同图/未死/在落点 ±2 内，`:5967` 的 `InRange(..., 2)`）；
    /// 2. `Attacked(value, DefenceType.MAC)` 结算，伤害 > 0 才有后续；
    /// 3. `Level + (目标是否玩家 ? 2 : 10) >= target.Level` 才判定两种毒；
    ///    Slow：`Random.Next(玩家 ? 100 : 20) <= magic.Level`，时长玩家 4 / 怪 `5 + Random.Next(5)`；
    ///    Frozen：`Random.Next(玩家 ? 100 : 40) <= magic.Level`，时长玩家 2 / 怪 `5 + Random.Next(Freezing)`；
    ///    两种毒的 `TickSpeed` 都是 1000ms、`OperateTime = 0`；
    /// 4. 伤害 > 0 即 `LevelMagic`。返回是否训练到技能。
    fn resolve_frost_crunch(
        &mut self,
        caster_id: u32,
        map: &str,
        target_id: u32,
        location: Point,
        damage: i32,
        now: i64,
    ) -> bool {
        if !self.magic_target_hittable(map, target_id, location) {
            return false;
        }
        if self.damage_magic_target(caster_id, target_id, damage) <= 0 {
            return false;
        }
        let Some(caster_level) = self
            .players
            .get(&caster_id)
            .map(|player| i32::from(player.level))
        else {
            return false;
        };
        let magic_level = self
            .player_magic(caster_id, Spell::FrostCrunch)
            .map_or(0, |magic| i32::from(magic.level));
        // 目标类型：决定等级边界与随机分母（`:5970`、`:5982`）。
        let target = self
            .monster(target_id)
            .map(|monster| (i32::from(monster.kind.level), false))
            .or_else(|| {
                self.players
                    .get(&target_id)
                    .map(|player| (i32::from(player.level), true))
            });
        let Some((target_level, is_player)) = target else {
            return true;
        };
        if caster_level + if is_player { 2 } else { 10 } < target_level {
            return true;
        }
        // Slow：`Random.Next(玩家 ? 100 : 20) <= magic.Level`，时长玩家 4 秒 / 怪 `5 + Random.Next(5)`；
        // `TickSpeed = 1000`（`:5974`），所以第一跳在 +1000ms。
        if self.rng.next_range(0, if is_player { 100 } else { 20 } - 1) <= magic_level {
            let duration = if is_player {
                4
            } else {
                5 + self.rng.next_range(0, 4)
            };
            let poison = PoisonState::new(
                PoisonType::Slow,
                0,
                u32::try_from(duration).unwrap_or(1),
                now + FROST_CRUNCH_POISON_TICK_MS,
                caster_id,
            )
            .with_tick_speed(FROST_CRUNCH_POISON_TICK_MS);
            self.apply_frost_poison(target_id, poison);
        }
        // Frozen：`Random.Next(玩家 ? 100 : 40) <= magic.Level`，时长玩家 2 秒 /
        // 怪 `5 + Random.Next(施法者 Stats[Freezing])`（`:5987`，取施法者装备汇总，
        // 见 [`Self::caster_freezing`]）；`TickSpeed = 1000`（`:5986`）。
        if self.rng.next_range(0, if is_player { 100 } else { 40 } - 1) <= magic_level {
            let duration = if is_player {
                2
            } else {
                5 + self.freeze_roll(caster_id)
            };
            let poison = PoisonState::new(
                PoisonType::Frozen,
                0,
                u32::try_from(duration).unwrap_or(1),
                now + FROST_CRUNCH_POISON_TICK_MS,
                caster_id,
            )
            .with_tick_speed(FROST_CRUNCH_POISON_TICK_MS);
            self.apply_frost_poison(target_id, poison);
        }
        true
    }

    /// 按目标类型落到怪毒/人毒（FrostCrunch 的目标可能是怪物），并照 Crystal
    /// `target.OperateTime = 0`（`:5975`、`:5987`）让中毒的怪立刻重新思考。
    fn apply_frost_poison(&mut self, target_id: u32, poison: PoisonState) {
        let is_monster = self.monster(target_id).is_some();
        let applied = if is_monster {
            self.apply_poison_to_monster(target_id, poison)
        } else {
            self.apply_poison_to_player(target_id, poison)
        };
        if applied {
            if let Some(monster) = self.monster_mut(target_id) {
                monster.operate_time = 0;
            }
        }
    }

    /// 雷暴/狂风绝命的 5×5 范围伤害（Crystal `Map.cs:1303-1341` 的 `ThunderStorm` 分支）。
    ///
    /// 以 `center` 为心扫 `y-2..=y+2` × `x-2..=x+2`，逐格遍历对象：
    /// `Player`/`Monster` 且 `IsAttackTarget(caster)` 才 `Attacked(value, MAC)`；
    /// 伤害 = `spell == ThunderStorm && !Undead ? value / 10 : value`；
    /// 只要命中一个目标就返回 `true`（结算方据此 `LevelMagic` 一次）。
    fn resolve_thunder_storm(
        &mut self,
        caster_id: u32,
        map: &str,
        center: Point,
        damage: i32,
        spell: Spell,
    ) -> bool {
        let Some(caster_stats) = self.players.get(&caster_id).map(|caster| caster.stats) else {
            return false;
        };
        let divided = spell == Spell::ThunderStorm;
        let mut trained = false;
        for dy in -THUNDER_STORM_RADIUS..=THUNDER_STORM_RADIUS {
            for dx in -THUNDER_STORM_RADIUS..=THUNDER_STORM_RADIUS {
                let cell = Point::new(center.x + dx, center.y + dy);
                // Crystal 只处理 `cell.Valid` 的格子（`Map.cs:1315`、`:1325`），
                // 本作的等价判定是 `walkable_on`（边界 + 地形可走，见 `World::walkable_on`）。
                if !self.walkable_on(map, cell) {
                    continue;
                }
                for entry in self.aoi_in_cell(map, cell) {
                    match entry.kind {
                        AoiKind::Monster => {
                            let target_id = entry.object_id;
                            let Some(value) = self.monster(target_id).and_then(|monster| {
                                if monster.dead || monster.map != map {
                                    return None;
                                }
                                Some(if divided && !monster.undead {
                                    damage / THUNDER_STORM_DAMAGE_DIVISOR
                                } else {
                                    damage
                                })
                            }) else {
                                continue;
                            };
                            if !self.monster_is_attack_target_of_player(caster_id, target_id) {
                                continue;
                            }
                            self.damage_magic_target(caster_id, target_id, value);
                            trained = true;
                        }
                        AoiKind::Player => {
                            let target_id = entry.object_id;
                            if target_id == caster_id {
                                continue;
                            }
                            let attackable = self.players.get(&target_id).is_some_and(|player| {
                                !player.dead && player.map == map && player.position == cell
                            }) && self
                                .is_attack_target_player(caster_id, target_id);
                            if !attackable {
                                continue;
                            }
                            // 玩家永远不是亡灵 → `ThunderStorm` 也只有 1/10 伤害。
                            let value = if divided {
                                damage / THUNDER_STORM_DAMAGE_DIVISOR
                            } else {
                                damage
                            };
                            self.damage_player_from_player(
                                PendingHit {
                                    at: 0,
                                    attacker_id: caster_id,
                                    target_id,
                                    damage: value,
                                    defence: DefenceType::MAC,
                                },
                                &caster_stats,
                            );
                            trained = true;
                        }
                        AoiKind::Npc | AoiKind::Item => {}
                    }
                }
            }
        }
        trained
    }

    /// 把怪物沿 `direction` 推一格（Crystal `MonsterObject.Pushed`，`MonsterObject.cs:1035-1091`）：
    /// `Info.CanPush == false`、前方出界/不可走、或前方格上有 Blocking 对象时不动；
    /// 推成功时转身朝向推手、`ActionTime = now + 300`、`MoveTime = now + 500`，并广播 `S.ObjectPushed`。
    /// 返回是否真的移动了一格。
    fn push_monster_once(&mut self, monster_id: u32, direction: MirDirection) -> bool {
        let Some(monster) = self.monster(monster_id) else {
            return false;
        };
        if monster.dead || !monster.kind.can_push {
            return false;
        }
        let map = monster.map.clone();
        let forward = monster.position.moved(direction, 1);
        if !self.walkable_on(&map, forward) {
            return false;
        }
        let blocked = self
            .aoi_in_cell(&map, forward)
            .into_iter()
            .any(|entry| match entry.kind {
                AoiKind::Player => self
                    .players
                    .get(&entry.object_id)
                    .is_some_and(|player| !player.dead),
                AoiKind::Monster => self
                    .monster(entry.object_id)
                    .is_some_and(|monster| !monster.dead),
                AoiKind::Npc | AoiKind::Item => false,
            });
        if blocked {
            return false;
        }
        let now = self.now();
        // `Direction = ReverseDirection(dir)`（`:1041`、`:1062`）。
        let reverse = rotate_direction(direction, 4);
        self.aoi_move(&map, AoiKind::Monster, monster_id, forward);
        if let Some(monster) = self.monster_mut(monster_id) {
            monster.direction = reverse;
            monster.position = forward;
            monster.action_time = now + PUSH_ACTION_DELAY_MS;
            monster.move_time = now + PUSH_MOVE_DELAY_MS;
        }
        let packet = ServerPacket::ObjectPushed(ObjectPushed {
            object_id: monster_id,
            location: forward,
            direction: reverse,
        })
        .encode_frame();
        self.broadcast(&map, forward, Some(monster_id), &packet, monster_id);
        true
    }

    /// 推玩家一格（Crystal `HumanObject.Pushed`，`HumanObject.cs:2648-2745`）。
    ///
    /// 逐行对应：越界 → 停；目标格上任何 `Blocking` 对象 → 停；否则 `Remove/Add` 格、
    /// `Direction = ReverseDirection(dir)`、给本人 `S.Pushed`、向周围广播 `S.ObjectPushed`，
    /// 末 `ActionTime = now + 500`（`:2743`）。本作没有队伍与 `Concentration` 增益，
    /// 因此省略 `GetPlayerLocation` 与 `UpdateConcentration` 两段。
    fn push_player_once(&mut self, target_id: u32, direction: MirDirection) -> bool {
        let Some(player) = self.players.get(&target_id) else {
            return false;
        };
        let (map, position, dead) = (player.map.clone(), player.position, player.dead);
        if dead {
            return false;
        }
        let forward = position.moved(direction, 1);
        if !self.walkable_on(&map, forward) {
            return false;
        }
        let blocked = self
            .aoi_in_cell(&map, forward)
            .into_iter()
            .any(|entry| match entry.kind {
                AoiKind::Player => self
                    .players
                    .get(&entry.object_id)
                    .is_some_and(|player| !player.dead),
                AoiKind::Monster => self
                    .monster(entry.object_id)
                    .is_some_and(|monster| !monster.dead),
                AoiKind::Npc | AoiKind::Item => false,
            });
        if blocked {
            return false;
        }
        let now = self.now();
        let reverse = rotate_direction(direction, 4);
        self.aoi_move(&map, AoiKind::Player, target_id, forward);
        if let Some(player) = self.players.get_mut(&target_id) {
            player.position = forward;
            player.direction = reverse;
            // `ActionTime = Envir.Time + 500`（`HumanObject.cs:2743`）。
            player.action_time = now + PUSH_MOVE_DELAY_MS;
        }
        let pushed = ServerPacket::Pushed(Pushed {
            location: forward,
            direction: reverse,
        })
        .encode_frame();
        self.send_to_player(target_id, &pushed);
        let broadcast = ServerPacket::ObjectPushed(ObjectPushed {
            object_id: target_id,
            location: forward,
            direction: reverse,
        })
        .encode_frame();
        self.broadcast(&map, forward, Some(target_id), &broadcast, target_id);
        true
    }

    /// 冲撞走入一格后的火墙结算（Crystal `HumanObject.cs:5100-5121`）：对格上的
    /// `SpellObject` 调 `ProcessSpell`，火墙 `Attacked(...MAC...) > 0` 就打断冲撞。
    /// 返回是否被火墙打中（真 = 停止继续冲撞）。
    fn fire_wall_on_cell(&mut self, map: &str, cell: Point, caster_id: u32, now: i64) -> bool {
        let Some((wall_caster, wall_damage)) = self
            .spell_fields
            .iter()
            .find(|field| {
                field.spell == Spell::FireWall
                    && field.map == map
                    && field.center == cell
                    && field.caster_id != caster_id
                    && now < field.expire_at
            })
            .map(|field| (field.caster_id, field.damage))
        else {
            return false;
        };
        let Some(stats) = self.players.get(&wall_caster).map(|player| player.stats) else {
            return false;
        };
        let before = self.players.get(&caster_id).map_or(0, |player| player.hp);
        self.damage_player_from_player(
            PendingHit {
                at: now,
                attacker_id: wall_caster,
                target_id: caster_id,
                damage: wall_damage,
                defence: DefenceType::MAC,
            },
            &stats,
        );
        let after = self.players.get(&caster_id).map_or(0, |player| player.hp);
        after < before
    }

    /// 冲撞（Crystal `HumanObject.ShoulderDash`，`HumanObject.cs:4975-5157`）。
    ///
    /// **同步**执行（Crystal 在 `Magic` 里 `case ShoulderDash: ShoulderDash(magic); return;`）：
    /// 0. 前置 `InTrapRock || !CanWalk` → 直接返回（本作没有陷阱系统，做 `CanWalk` 等价校验）；
    /// 1. `dist = Random.Next(2) + Level + 2`；`ActionTime = now + MoveDelay`；
    /// 2. 逐格向前：出界/不可走/**安全区** → 停；第 0 格先找目标
    ///    （`Race==Player||Monster` 且 `IsAttackTarget(this)` 且 `Level < 本等级`），
    ///    没有目标时格上任何存活的玩家/怪物都算阻挡；
    /// 3. 有目标时每格推它一格（怪物走 `MonsterObject.Pushed`、玩家走
    ///    `HumanObject.Pushed`；`Pushed` 返回 0 视为阻挡并停）；每成功走一格发本人
    ///    `S.UserDash`、广播 `S.ObjectDash`；入格后结算火墙（`:5100-5121`），被打中即停；
    /// 4. **无论走没走**都 `CellTime = now + 500`、`_stepCounter = 0`（`:5155-5156`）；
    /// 5. 一格未走 → `S.UserDashFail` + `S.ObjectDashFail` + 系统聊天
    ///    （`InSafeZone ? NoPushingInSafezone : NotEnoughPushingPower`，`:5128-5143`）；
    /// 6. 走过路 → 对目标补一次 `magic.GetDamage(0)` 的 `DefenceType.None` 伤害
    ///    （怪物走 `Attacked`、玩家走 `PLAYER_ATTACK` 路径）+ `LevelMagic`。
    ///
    /// 返回 `None` 表示前置失败（Crystal `ShoulderDash()` 开头直接 `return;`，
    /// `Magic()` 随之提前返回：**不补** `S.MagicCast`、不记冷却，只留下已扣蓝）；
    /// `Some(0)` 表示已跑到函数末尾但没伤到目标（含零步失败分支），
    /// 其余 `Some(value)` 为对目标补的那次伤害。
    fn cast_shoulder_dash(
        &mut self,
        caster_id: u32,
        direction: MirDirection,
        magic_level: u8,
        now: i64,
    ) -> Option<i32> {
        // 前置（`HumanObject.cs:4977`）：`if (InTrapRock || !CanWalk) return;`
        // 本作没有陷阱（InTrapRock）系统 → 做 `CanWalk` 等价校验：存活、未冰冻、未麻痹、
        // 动作时间与格子时间都允许移动（`HumanObject.CanWalk`，`HumanObject.cs:121-133`）。
        let can_start = self.players.get(&caster_id).is_some_and(|player| {
            !player.dead
                && !player.is_frozen()
                && now >= player.action_time
                && now >= player.cell_time
        });
        if !can_start || self.has_buff(caster_id, BuffKind::Paralysis) {
            return None;
        }
        let (map, start, caster_level) = self
            .players
            .get(&caster_id)
            .map(|player| (player.map.clone(), player.position, i32::from(player.level)))?;
        let distance = self.rng.next_range(0, 1) + i32::from(magic_level) + 2;
        if let Some(player) = self.players.get_mut(&caster_id) {
            player.action_time = now + MOVE_DELAY_MS;
        }
        let mut position = start;
        let mut target_monster: Option<u32> = None;
        let mut target_player: Option<u32> = None;
        let mut blocking = false;
        let mut travelled = 0;
        for step in 0..distance {
            if blocking {
                break;
            }
            let next = position.moved(direction, 1);
            // `!ValidPoint(_nextLocation) || GetSafeZone(_nextLocation) != null` → break（`:5002`）。
            if !self.walkable_on(&map, next) || self.safe_zone_at(&map, next) {
                break;
            }
            // 第 0 格先找目标（`:5008-5043`）：
            // `(Race==Player||Monster) && IsAttackTarget(this) && Level < Level` 才推；
            // 其余存活目标是阻挡。
            if step == 0 && target_monster.is_none() && target_player.is_none() {
                for entry in self.aoi_in_cell(&map, next) {
                    match entry.kind {
                        AoiKind::Player => {
                            let Some(target) = self.players.get(&entry.object_id) else {
                                continue;
                            };
                            if target.dead {
                                continue;
                            }
                            if i32::from(target.level) < caster_level
                                && self.is_attack_target_player(caster_id, entry.object_id)
                            {
                                target_player = Some(entry.object_id);
                                break;
                            }
                            blocking = true;
                            break;
                        }
                        AoiKind::Monster => {
                            let Some(monster) = self.monster(entry.object_id) else {
                                continue;
                            };
                            if monster.dead {
                                continue;
                            }
                            if i32::from(monster.kind.level) < caster_level
                                && self
                                    .monster_is_attack_target_of_player(caster_id, entry.object_id)
                            {
                                target_monster = Some(entry.object_id);
                                break;
                            }
                            blocking = true;
                            break;
                        }
                        AoiKind::Npc | AoiKind::Item => {}
                    }
                }
                if blocking {
                    break;
                }
            }
            // 有目标：推它一格（怪物 `MonsterObject.Pushed` `:1035`；玩家
            // `HumanObject.Pushed` `:2648`）；没目标：格上不能有别的 Blocking（`:5045-5087`）。
            let can_dash = if let Some(target_id) = target_monster {
                self.push_monster_once(target_id, direction)
            } else if let Some(target_id) = target_player {
                self.push_player_once(target_id, direction)
            } else {
                !self
                    .aoi_in_cell(&map, next)
                    .into_iter()
                    .any(|entry| match entry.kind {
                        AoiKind::Player => self
                            .players
                            .get(&entry.object_id)
                            .is_some_and(|player| !player.dead),
                        AoiKind::Monster => self
                            .monster(entry.object_id)
                            .is_some_and(|monster| !monster.dead),
                        AoiKind::Npc | AoiKind::Item => false,
                    })
            };
            if !can_dash {
                break;
            }
            // 走一格：`S.UserDash` 只发本人、`S.ObjectDash` 广播周围（`:5091-5098`）。
            self.aoi_move(&map, AoiKind::Player, caster_id, next);
            if let Some(player) = self.players.get_mut(&caster_id) {
                player.position = next;
            }
            let user = ServerPacket::UserDash(UserDash {
                location: next,
                direction,
            })
            .encode_frame();
            self.send_to_player(caster_id, &user);
            let broadcast = ServerPacket::ObjectDash(ObjectDash {
                object_id: caster_id,
                location: next,
                direction,
            })
            .encode_frame();
            self.broadcast(&map, next, Some(caster_id), &broadcast, caster_id);
            position = next;
            travelled += 1;
            // 火墙打断（`:5100-5121`）：中招就不再继续往前冲。
            if self.fire_wall_on_cell(&map, next, caster_id, now) {
                blocking = true;
            }
        }
        // `CellTime = now + 500; _stepCounter = 0;`（`:5155-5156`）——**走没走都执行**（P04）。
        if let Some(player) = self.players.get_mut(&caster_id) {
            player.cell_time = now + SHOULDER_DASH_CELL_TIME_MS;
            player.step_counter = 0;
        }
        if travelled == 0 {
            // `S.UserDashFail` + `S.ObjectDashFail`（`:5128-5141`）。
            let fail = ServerPacket::UserDashFail(UserDashFail {
                location: position,
                direction,
            })
            .encode_frame();
            self.send_to_player(caster_id, &fail);
            let broadcast = ServerPacket::ObjectDashFail(ObjectDashFail {
                object_id: caster_id,
                location: position,
                direction,
            })
            .encode_frame();
            self.broadcast(&map, position, Some(caster_id), &broadcast, caster_id);
            // 一格未走 → 系统聊天（`:5140-5143`）：
            // `InSafeZone ? NoPushingInSafezone : NotEnoughPushingPower`。
            let key = if self.safe_zone_at(&map, position) {
                TextKey::NoPushingInSafezone
            } else {
                TextKey::NotEnoughPushingPower
            };
            let text = self.text(caster_id, key);
            self.send_system_chat(caster_id, &text);
            // 零步失败已走到 Crystal 的尾巴：`S.MagicCast` 由调用方补。
            return Some(0);
        }
        // `_target?.Attacked(this, magic.GetDamage(0), DefenceType.None, false); LevelMagic(magic);`
        // （`:5144-5145`）——即使没有目标，只要走成了也 `LevelMagic`。
        let mut damage = None;
        let caster_stats = self.players.get(&caster_id).map(|player| player.stats);
        let magic_damage = if let (Some(magic), Some(info)) = (
            self.player_magic(caster_id, Spell::ShoulderDash).copied(),
            self.magic_info(Spell::ShoulderDash).cloned(),
        ) {
            Some(magic.damage(&info, 0, &mut self.rng))
        } else {
            None
        };
        if let (Some(target_id), Some(stats), Some(value)) =
            (target_monster, caster_stats, magic_damage)
        {
            let dealt = self.damage_monster(
                PendingHit {
                    at: 0,
                    attacker_id: caster_id,
                    target_id,
                    damage: value,
                    defence: DefenceType::None,
                },
                &stats,
            );
            damage = Some(dealt);
        } else if let (Some(target_id), Some(stats), Some(value)) =
            (target_player, caster_stats, magic_damage)
        {
            if self.is_attack_target_player(caster_id, target_id) {
                let before = self.players.get(&target_id).map_or(0, |player| player.hp);
                self.damage_player_from_player(
                    PendingHit {
                        at: 0,
                        attacker_id: caster_id,
                        target_id,
                        damage: value,
                        defence: DefenceType::None,
                    },
                    &stats,
                );
                let after = self.players.get(&target_id).map_or(0, |player| player.hp);
                damage = Some((before - after).max(0));
            }
        }
        self.level_magic(caster_id, Spell::ShoulderDash);
        Some(damage.unwrap_or(0))
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
    /// （Crystal 还有末项 `Random.Next(Stats[Stat.PoisonAttack])`，
    /// `HumanObject.cs:6060`；本作 `CombatStats` 无该字段，Poisoning 仍按 R51 省去该项，
    /// 只有毒雾走 [`World::poison_attack_roll`] 的真实 `PoisonAttack` 聚合）。
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
                // 每条毒自带跳速：普通毒 2000ms、冰咆哮 Slow/Frozen 1000ms
                // （Crystal `Poison.TickSpeed`，`HumanObject.cs:6058`、`:5974`）。
                poison.tick_at = now + poison.tick_speed_ms;
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
                // 毒主下线 → 整条毒移除；Slow 要恢复原速（`MonsterObject.cs:1464-1474`）。
                if poison.kind == PoisonType::Slow {
                    Self::restore_monster_speed(monster, now);
                }
                monster.poisons.remove(index);
                changed = true;
                continue;
            }
            if now > poison.tick_at {
                let mut poison = poison;
                poison.ticks = poison.ticks.saturating_add(1);
                // 每条毒自带跳速（Crystal `Poison.TickSpeed`）：普通毒 2000ms、
                // 冰咆哮 Slow/Frozen 1000ms（`HumanObject.cs:6058`、`:5974`、`:5986`）。
                poison.tick_at = now + poison.tick_speed_ms;
                if poison.ticks >= poison.duration {
                    // 怪物口径：`Time >= Duration` 先移除、**最后一跳不结算**；
                    // Slow 同时恢复 `Info.MoveSpeed/AttackSpeed`（`MonsterObject.cs:1476-1491`）。
                    if poison.kind == PoisonType::Slow {
                        Self::restore_monster_speed(monster, now);
                    }
                    monster.poisons.remove(index);
                    changed = true;
                    continue;
                }
                monster.poisons[index] = poison;
                match poison.kind {
                    PoisonType::Green => {
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
                    // Slow 每跳再叠 100ms，封顶 3500（`MonsterObject.cs:1544-1553`）。
                    PoisonType::Slow => {
                        monster.move_speed_ms =
                            (monster.move_speed_ms + MONSTER_SLOW_STEP_MS).min(MONSTER_SLOW_MAX_MS);
                        monster.attack_speed_ms = (monster.attack_speed_ms + MONSTER_SLOW_STEP_MS)
                            .min(MONSTER_SLOW_MAX_MS);
                    }
                    // Frozen 由 `Monster::can_move/can_attack` 判定，无需改速度；
                    // 其余毒种（红毒减防）本轮不在怪物侧实现，和改动前一致。
                    _ => {}
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

    /// Slow 毒结束/毒主消失时，把怪物的移动与攻击间隔恢复成模板值
    /// （Crystal `MonsterObject.cs:1479-1490`、`:1550-1553`）。
    fn restore_monster_speed(monster: &mut crate::monster::Monster, now: i64) {
        monster.move_speed_ms = monster.kind.move_speed_ms;
        monster.attack_speed_ms = monster.kind.attack_speed_ms;
        monster.attack_time = now + monster.attack_speed_ms;
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
        let language = self.player_language(object_id);
        let mut client = magic.client_magic(info);
        client.name = self.magic_name_for(language, info);
        self.send_to_player(
            object_id,
            &ServerPacket::NewMagic(NewMagic {
                magic: client,
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

    use crate::gamedata::GameData;
    use mir_db::MirDb;
    use std::sync::Arc;

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
                bind_map_index: 0,
                bind_point: mir_core::Point::default(),
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
        // 第 51 轮补齐的 5 个技能都有独立分支。
        assert_eq!(spell_kind(Spell::Blink), SpellKind::Blink);
        assert_eq!(spell_kind(Spell::StormEscape), SpellKind::StormEscape);
        assert_eq!(spell_kind(Spell::FrostCrunch), SpellKind::FrostCrunch);
        assert_eq!(spell_kind(Spell::ThunderStorm), SpellKind::ThunderStorm);
        assert_eq!(spell_kind(Spell::ShoulderDash), SpellKind::ShoulderDash);
        // R52-B3：`TrapHexagon` 已接真实实现（3×2 预检 + 护身符 + 500ms 落地 +
        // 3×3 重检 + 8 外围场），不再是 `Unsupported`；行为断言见 integration 测试
        // `tests/round52_trap_hexagon.rs`。
        assert_eq!(spell_kind(Spell::TrapHexagon), SpellKind::TrapHexagon);
        // R52-B2：`PoisonCloud` 已接耗材（符 5 + 绿毒 5），不再是 `Unsupported`；
        // 真实施法行为见 integration 测试 `tests/round52_poison_cloud.rs`。
        assert_eq!(spell_kind(Spell::PoisonCloud), SpellKind::PoisonCloud);
        assert_eq!(spell_kind(Spell::Rage), SpellKind::Unsupported);
        // 增益技能不能和已实现的伤害/治疗技能重叠。
        assert!(!REGISTERED_UNSUPPORTED.contains(&Spell::MagicShield));
    }

    // -----------------------------------------------------------------------
    // 第 51 轮：Blink / ShoulderDash / StormEscape / FrostCrunch / ThunderStorm
    //
    // Crystal 对照：`Magic` 分发 `HumanObject.cs:3499-3780`；Blink/StormEscape
    // 落地 `:6079-6129`；ShoulderDash `:4975-5157`；FrostCrunch 结算
    // `:5962-5996`；ThunderStorm 5×5 `Map.cs:1303-1341`。
    // -----------------------------------------------------------------------

    /// 30×30 全可走测试地图（与 `world.rs` 的 `test_map` 一致）。
    fn round51_map(name: &str) -> Arc<mir_world::Map> {
        mir_world::Map {
            width: 30,
            height: 30,
            cells: vec![mir_world::MapCell::default(); 900],
            file_name: name.to_owned(),
            version: mir_world::MapVersion::V0,
            doors: Vec::new(),
        }
        .into()
    }

    /// 第 51 轮技能模板：`MPower=10`、`DefPower=4`、倍率 1.0。
    /// 3 级时 power = round(10/4*3 + 4) = 12，MC 100 → 伤害 112。
    fn round51_magic(spell: Spell, range: u8) -> MagicInfo {
        MagicInfo {
            name: format!("{spell:?}"),
            spell: spell.value(),
            base_cost: 1,
            level_cost: 1,
            icon: 0,
            level1: 1,
            level2: 2,
            level3: 3,
            need1: 0,
            need2: 0,
            need3: 0,
            delay_base: 1_000,
            delay_reduction: 0,
            power_base: 4,
            power_bonus: 0,
            m_power_base: 10,
            m_power_bonus: 0,
            range,
            multiplier_base: 1.0,
            multiplier_bonus: 0.0,
        }
    }

    fn round51_map_info(file_name: &str, no_teleport: bool) -> mir_db::MapInfo {
        mir_db::MapInfo {
            index: if no_teleport { 2 } else { 1 },
            file_name: file_name.to_owned(),
            title: file_name.to_owned(),
            no_teleport,
            ..mir_db::MapInfo::default()
        }
    }

    fn round51_world(magics: Vec<MagicInfo>, maps: Vec<mir_db::MapInfo>) -> World {
        let db = MirDb {
            magics,
            maps,
            ..MirDb::default()
        };
        let mut world = World::new(999);
        world.set_game_data(Arc::new(GameData::from_db(db, None)));
        world.insert_map("0", round51_map("0"));
        world.insert_map("nt", round51_map("nt"));
        // 冻结世界时钟到 0：Crystal 的时间断言（200/500/750/600ms）要求从 0 起算，
        // 否则首次 `advance_clock` 前真实流逝的几毫秒会污染精确断言。
        world.advance_clock(0);
        world
    }

    /// 与 [`round51_world`] 相同，但带一份物品模板库（用于验证装备**模板属性**
    /// 参与 `caster_freezing` 汇总）。
    fn round51_world_with_items(
        magics: Vec<MagicInfo>,
        maps: Vec<mir_db::MapInfo>,
        items: Vec<mir_db::ItemInfo>,
    ) -> World {
        let db = MirDb {
            magics,
            maps,
            items,
            ..MirDb::default()
        };
        let mut world = World::new(999);
        world.set_game_data(Arc::new(GameData::from_db(db, None)));
        world.insert_map("0", round51_map("0"));
        world.insert_map("nt", round51_map("nt"));
        world.advance_clock(0);
        world
    }

    fn round51_default_world(magics: Vec<MagicInfo>) -> World {
        round51_world(
            magics,
            vec![round51_map_info("0", false), round51_map_info("nt", true)],
        )
    }

    fn round51_add_player(
        world: &mut World,
        object_id: u32,
        map: &str,
        position: Point,
    ) -> mpsc::UnboundedReceiver<Vec<u8>> {
        let (mut player, receiver) = test_player(object_id, map, position, 20, false);
        player.mp = 10_000;
        player.stats.min_mc = 100;
        player.stats.max_mc = 100;
        world.enter(player);
        receiver
    }

    fn round51_learn(world: &mut World, object_id: u32, spell: Spell, level: u8) {
        world
            .player_mut(object_id)
            .expect("玩家")
            .magics
            .push(UserMagic {
                spell,
                level,
                key: 0,
                experience: 0,
            });
    }

    /// 刷一只不会自己行动的怪（`OperateTime/RegenTime = i64::MAX`），血量抬到
    /// 10000、物理防御归零，方便断言伤害数字。
    fn round51_spawn_monster(world: &mut World, kind: MonsterKind, position: Point) -> u32 {
        let object_id = world.spawn_monster("0", kind, position);
        if let Some(monster) = world.monster_mut(object_id) {
            monster.operate_time = i64::MAX;
            monster.regen_time = i64::MAX;
            monster.hp = 10_000;
            monster.stats.hp = 10_000;
            monster.stats.min_mac = 0;
            monster.stats.max_mac = 0;
        }
        object_id
    }

    fn round51_packets(receiver: &mut mpsc::UnboundedReceiver<Vec<u8>>) -> Vec<ServerPacket> {
        let mut packets = Vec::new();
        while let Ok(frame) = receiver.try_recv() {
            let index = i16::from_le_bytes([frame[2], frame[3]]);
            packets.push(ServerPacket::decode(index, &frame[4..]).expect("解码服务端包"));
        }
        packets
    }

    fn round51_resolve_after(world: &mut World, ms: i64) {
        world.advance_clock(ms);
        let now = world.now();
        world.resolve_pending_magic(now);
    }

    // --- Blink（`HumanObject.cs:6114-6129`）--------------------------------

    #[test]
    fn blink_lands_after_200ms_and_starts_30s_penalty() {
        let mut world = round51_default_world(vec![round51_magic(Spell::Blink, 9)]);
        let mut caster_rx = round51_add_player(&mut world, 1, "0", Point::new(10, 10));
        let mut observer_rx = round51_add_player(&mut world, 2, "0", Point::new(12, 12));
        round51_learn(&mut world, 1, Spell::Blink, 3);
        round51_packets(&mut caster_rx);
        round51_packets(&mut observer_rx);

        world
            .cast_magic(1, Spell::Blink, MirDirection::Down, 0, Point::new(15, 10))
            .expect("闪现应当施法成功");
        assert_eq!(
            world.player(1).expect("施法者").position,
            Point::new(10, 10),
            "Crystal queues the now+200 DelayedAction, so the caster must not move before landing"
        );
        round51_resolve_after(&mut world, 199);
        assert_eq!(
            world.player(1).expect("施法者").position,
            Point::new(10, 10),
            "199ms is not due yet"
        );
        round51_resolve_after(&mut world, 1);
        let landed_at = world.now();
        assert_eq!(
            world.player(1).expect("施法者").position,
            Point::new(15, 10),
            "after 200ms the caster lands on the client-supplied cell"
        );
        // P06：真实 `TemporalFlux` 增益（30 秒 + 30% 传送耗蓝），不是内部计时器。
        let buff = world
            .buffs(1)
            .iter()
            .find(|buff| buff.kind == BuffKind::TemporalFlux)
            .expect("落地后应加 TemporalFlux 增益");
        assert_eq!(
            buff.expire_at,
            landed_at + TELEPORT_PENALTY_MS,
            "TemporalFlux: teleport mana cost +30% for 30 seconds"
        );
        assert_eq!(
            buff.values,
            vec![TELEPORT_MANA_PENALTY_PERCENT],
            "Stat.TeleportManaPenaltyPercent = 30 挂在增益上（HumanObject.cs:6127）"
        );
        // P06：Crystal 的物体光效广播也发给本人（MapObject.cs:408-420）。
        let caster_packets = round51_packets(&mut caster_rx);
        assert!(
            caster_packets.iter().any(|packet| matches!(
                packet,
                ServerPacket::ObjectEffect(effect) if effect.effect == SpellEffect::Teleport
            )),
            "the caster must also receive their own S.ObjectEffect{{Teleport}} (P06)"
        );
        // P06 包序（Crystal `HumanObject.cs:6119-6128`）：先 `Teleport`（Out/In）→
        // 再广播 `S.ObjectEffect{Teleport}` → 最后 `AddBuff(TemporalFlux, 30s)`。
        // `S.ObjectTeleportOut/In` 是广播给**周围**的（本人靠 `ChangeMap`/`UserLocation`
        // 同步地图），所以 Out/In 的顺序在观察者包流上断言，光效→buff 的顺序在本人包流上断言。
        let effect_at = caster_packets.iter().position(|packet| {
            matches!(
                packet,
                ServerPacket::ObjectEffect(effect) if effect.effect == SpellEffect::Teleport
            )
        });
        let buff_at = caster_packets.iter().position(|packet| {
            matches!(
                packet,
                ServerPacket::AddBuff(add) if add.buff.buff_type == BuffType::TemporalFlux
            )
        });
        // 先断言四包存在，再比较顺序：`Option<usize>` 的 `None < Some(_)` 为真，
        // 若缺包直接比较会假通过（B4-2）。
        assert!(
            effect_at.is_some(),
            "must send S.ObjectEffect{{Teleport}} (None<Some would pass silently)"
        );
        assert!(
            buff_at.is_some(),
            "must send AddBuff(TemporalFlux) (None<Some would pass silently)"
        );
        assert!(
            effect_at < buff_at,
            "本人包序必须是 ObjectEffect{{Teleport}} < AddBuff(TemporalFlux)，\
             实际 Effect={effect_at:?} Buff={buff_at:?}"
        );
        let packets = round51_packets(&mut observer_rx);
        assert!(
            packets.iter().any(|packet| matches!(
                packet,
                ServerPacket::ObjectEffect(effect) if effect.effect == SpellEffect::Teleport
            )),
            "on landing, broadcast SpellEffect.Teleport to nearby observers"
        );
        assert!(
            packets
                .iter()
                .any(|packet| matches!(packet, ServerPacket::ObjectTeleportIn(_))),
            "teleport must send ObjectTeleportOut/In"
        );
        // 观察者包序：ObjectTeleportOut（旧格）先于 ObjectTeleportIn（新格），
        // 都先于落地的 `S.ObjectEffect{Teleport}`（Crystal `MapObject.Teleport` 的顺序）。
        let out_at = packets
            .iter()
            .position(|packet| matches!(packet, ServerPacket::ObjectTeleportOut(_)));
        let in_at = packets
            .iter()
            .position(|packet| matches!(packet, ServerPacket::ObjectTeleportIn(_)));
        let observer_effect_at = packets.iter().position(|packet| {
            matches!(
                packet,
                ServerPacket::ObjectEffect(effect) if effect.effect == SpellEffect::Teleport
            )
        });
        assert!(
            out_at.is_some(),
            "observer must receive ObjectTeleportOut (None<Some would pass silently)"
        );
        assert!(
            in_at.is_some(),
            "observer must receive ObjectTeleportIn (None<Some would pass silently)"
        );
        assert!(
            observer_effect_at.is_some(),
            "observer must receive S.ObjectEffect{{Teleport}} (None<Some would pass silently)"
        );
        assert!(
            out_at < in_at && in_at < observer_effect_at,
            "观察者包序必须是 TeleportOut < TeleportIn < ObjectEffect{{Teleport}}，\
             实际 Out={out_at:?} In={in_at:?} Effect={observer_effect_at:?}"
        );
    }

    #[test]
    fn blink_out_of_range_is_rejected_without_mana() {
        let mut world = round51_default_world(vec![round51_magic(Spell::Blink, 9)]);
        let _caster_rx = round51_add_player(&mut world, 1, "0", Point::new(5, 5));
        round51_learn(&mut world, 1, Spell::Blink, 3);
        let mp = world.player(1).expect("玩家").mp;
        let error = world
            .cast_magic(1, Spell::Blink, MirDirection::Down, 0, Point::new(20, 20))
            .expect_err("target cell beyond the 9-cell range must be rejected");
        assert_eq!(
            error,
            MagicError::OutOfRange {
                distance: 15,
                range: 9,
            }
        );
        assert_eq!(world.player(1).expect("玩家").mp, mp, "超程不扣蓝");
    }

    #[test]
    fn blink_is_refused_on_no_teleport_map_with_chat() {
        let mut world = round51_default_world(vec![round51_magic(Spell::Blink, 9)]);
        let mut caster_rx = round51_add_player(&mut world, 1, "nt", Point::new(10, 10));
        round51_learn(&mut world, 1, Spell::Blink, 3);
        round51_packets(&mut caster_rx);

        world
            .cast_magic(1, Spell::Blink, MirDirection::Down, 0, Point::new(12, 10))
            .expect("施法本身成功，禁传在落地时判定");
        round51_resolve_after(&mut world, BLINK_DELAY_MS);
        assert_eq!(
            world.player(1).expect("玩家").position,
            Point::new(10, 10),
            "禁传地图不落地"
        );
        assert!(
            !world
                .buffs(1)
                .iter()
                .any(|buff| buff.kind == BuffKind::TemporalFlux),
            "no landing means no TemporalFlux buff"
        );
        let packets = round51_packets(&mut caster_rx);
        assert!(
            packets
                .iter()
                .any(|packet| matches!(packet, ServerPacket::Chat(_))),
            "on no-teleport maps Crystal sends the YouCannotTeleportOnMap notice"
        );
    }

    // --- StormEscape（`HumanObject.cs:3580-3588`、`:6079-6128`）------------

    #[test]
    fn storm_escape_strikes_at_500ms_then_teleports_at_750ms() {
        let mut world = round51_default_world(vec![round51_magic(Spell::StormEscape, 9)]);
        let _caster_rx = round51_add_player(&mut world, 1, "0", Point::new(10, 10));
        let monster =
            round51_spawn_monster(&mut world, MonsterKind::skeleton(), Point::new(11, 10));
        round51_learn(&mut world, 1, Spell::StormEscape, 3);

        world
            .cast_magic(
                1,
                Spell::StormEscape,
                MirDirection::Down,
                0,
                Point::new(14, 10),
            )
            .expect("狂风绝命施法成功");
        round51_resolve_after(&mut world, 500);
        let struck = world.monster(monster).expect("怪物").hp;
        assert!(
            struck <= 10_000 - 20,
            "500ms later lightning strikes under the caster's feet; StormEscape deals full damage (112), actual loss {}",
            10_000 - struck
        );
        assert_eq!(
            world.player(1).expect("施法者").position,
            Point::new(10, 10),
            "the caster must not teleport during the strike phase"
        );
        round51_resolve_after(&mut world, 250);
        assert_eq!(
            world.player(1).expect("施法者").position,
            Point::new(14, 10),
            "750ms later the caster teleports to the client-supplied cell"
        );
        assert!(
            world
                .buffs(1)
                .iter()
                .any(|buff| buff.kind == BuffKind::TemporalFlux),
            "teleport applies the TemporalFlux buff (HumanObject.cs:6089)"
        );
    }

    // --- FrostCrunch（`HumanObject.cs:3501-3504`、`:3939-3952`、`:5962-5996`）-

    #[test]
    fn frost_crunch_damages_target_and_gates_poison_by_level() {
        let mut world = round51_default_world(vec![round51_magic(Spell::FrostCrunch, 8)]);
        let _caster_rx = round51_add_player(&mut world, 1, "0", Point::new(10, 10));
        let target =
            round51_spawn_monster(&mut world, MonsterKind::scarecrow(), Point::new(12, 10));
        let mut high_kind = MonsterKind::scarecrow();
        high_kind.level = 100;
        let high_level = round51_spawn_monster(&mut world, high_kind, Point::new(8, 10));
        round51_learn(&mut world, 1, Spell::FrostCrunch, 3);

        world
            .cast_magic(
                1,
                Spell::FrostCrunch,
                MirDirection::Right,
                target,
                Point::new(12, 10),
            )
            .expect("冰咆哮施法成功");
        round51_resolve_after(&mut world, 599);
        assert_eq!(
            world.monster(target).expect("怪物").hp,
            10_000,
            "distance 2 cells -> delay MaxDistance*50+500 = 600ms; 599ms is not enough"
        );
        round51_resolve_after(&mut world, 1);
        let hit = world.monster(target).expect("怪物");
        assert!(hit.hp < 10_000, "600ms 结算 MAC 伤害");
        for poison in &hit.poisons {
            assert!(
                poison.kind == PoisonType::Slow || poison.kind == PoisonType::Frozen,
                "FrostCrunch may only apply Slow/Frozen"
            );
            assert_eq!(
                poison.tick_at,
                world.now() + SECOND_MS,
                "TickSpeed = 1000ms"
            );
        }

        // 目标等级 100 >> 施法者 20 + 10 → 概率门先失败，不叠任何毒。
        // 先把 SpellsTime(1800) / 冷却(1000) 走过去。
        round51_resolve_after(&mut world, 1_300);
        world
            .cast_magic(
                1,
                Spell::FrostCrunch,
                MirDirection::Left,
                high_level,
                Point::new(8, 10),
            )
            .expect("等级高的目标也照常施法");
        round51_resolve_after(&mut world, 600);
        let high = world.monster(high_level).expect("怪物");
        assert!(high.hp < 10_000, "伤害照常结算");
        assert!(high.poisons.is_empty(), "Level+10 < 目标等级 → 不叠毒");
    }

    // --- ThunderStorm（`HumanObject.cs:4264-4270`、`Map.cs:1303-1341`）-----

    #[test]
    fn thunder_storm_hits_5x5_and_divides_non_undead_damage() {
        let mut world = round51_default_world(vec![round51_magic(Spell::ThunderStorm, 0)]);
        let _caster_rx = round51_add_player(&mut world, 1, "0", Point::new(10, 10));
        let near = round51_spawn_monster(&mut world, MonsterKind::skeleton(), Point::new(9, 9));
        let undead = round51_spawn_monster(&mut world, MonsterKind::skeleton(), Point::new(11, 11));
        world.monster_mut(undead).expect("怪物").undead = true;
        let outside =
            round51_spawn_monster(&mut world, MonsterKind::skeleton(), Point::new(13, 10));
        round51_learn(&mut world, 1, Spell::ThunderStorm, 3);

        world
            .cast_magic(
                1,
                Spell::ThunderStorm,
                MirDirection::Down,
                0,
                Point::new(0, 0),
            )
            .expect("雷暴施法成功（Range=0，落点忽略）");
        round51_resolve_after(&mut world, 500);
        let near_drop = 10_000 - world.monster(near).expect("怪物").hp;
        let undead_drop = 10_000 - world.monster(undead).expect("怪物").hp;
        assert!(
            (1..=30).contains(&near_drop),
            "non-undead take 1/10 damage (112/10=11), actual loss {near_drop}"
        );
        assert!(
            undead_drop >= 50,
            "undead take full damage (112), actual loss {undead_drop}"
        );
        assert_eq!(
            world.monster(outside).expect("怪物").hp,
            10_000,
            "monsters outside the 5x5 area are unharmed"
        );
    }

    // --- ShoulderDash（`HumanObject.cs:4975-5157`）-------------------------

    #[test]
    fn shoulder_dash_travels_3_to_4_cells_and_ends_with_magic_cast() {
        let mut world = round51_default_world(vec![round51_magic(Spell::ShoulderDash, 0)]);
        let mut caster_rx = round51_add_player(&mut world, 1, "0", Point::new(10, 10));
        round51_learn(&mut world, 1, Spell::ShoulderDash, 3);
        round51_packets(&mut caster_rx);
        let start = world.player(1).expect("玩家").position;

        world
            .cast_magic(
                1,
                Spell::ShoulderDash,
                MirDirection::Down,
                0,
                Point::new(0, 0),
            )
            .expect("冲撞施法成功");
        let after = world.player(1).expect("玩家");
        let travelled = after.position.y - start.y;
        assert!(
            (5..=6).contains(&travelled),
            "Crystal dist = Next(2)+Level+2, level 3 should be 5..=6 cells, actual {travelled}"
        );
        assert_eq!(after.position.x, start.x, "只沿面向直冲");
        assert_eq!(
            after.cell_time,
            world.now() + SHOULDER_DASH_CELL_TIME_MS,
            "CellTime+500"
        );
        assert_eq!(after.step_counter, 0, "冲撞后 StepCounter 归零");
        assert_eq!(
            after.action_time,
            world.now() + MOVE_DELAY_MS,
            "ActionTime = now+MoveDelay"
        );

        let packets = round51_packets(&mut caster_rx);
        let dashes = packets
            .iter()
            .filter(|packet| matches!(packet, ServerPacket::UserDash(_)))
            .count();
        assert_eq!(
            dashes, travelled as usize,
            "one S.UserDash per travelled cell (Location = new cell)"
        );
        assert!(
            matches!(packets.last(), Some(ServerPacket::MagicCast(_))),
            "dash ends with S.MagicCast as the final packet"
        );
        assert!(
            !packets
                .iter()
                .any(|packet| matches!(packet, ServerPacket::Magic(_))),
            "a dash is not a normal cast and must not send S.Magic"
        );
    }

    #[test]
    fn shoulder_dash_fails_when_blocked_and_sends_dash_fail() {
        let mut world = round51_default_world(vec![round51_magic(Spell::ShoulderDash, 0)]);
        let mut caster_rx = round51_add_player(&mut world, 1, "0", Point::new(10, 10));
        let mut observer_rx = round51_add_player(&mut world, 2, "0", Point::new(5, 5));
        let mut blocker_kind = MonsterKind::skeleton();
        blocker_kind.level = 100;
        let blocker = round51_spawn_monster(&mut world, blocker_kind, Point::new(10, 11));
        round51_learn(&mut world, 1, Spell::ShoulderDash, 3);
        round51_packets(&mut caster_rx);
        round51_packets(&mut observer_rx);

        world
            .cast_magic(
                1,
                Spell::ShoulderDash,
                MirDirection::Down,
                0,
                Point::new(0, 0),
            )
            .expect("被挡住也返回成功（Crystal 只发失败包）");
        assert_eq!(
            world.player(1).expect("玩家").position,
            Point::new(10, 10),
            "zero cells travelled"
        );
        assert_eq!(
            world.monster(blocker).expect("怪物").position,
            Point::new(10, 11),
            "a higher-level monster cannot be pushed or attacked -> it blocks in place"
        );

        let caster_packets = round51_packets(&mut caster_rx);
        assert!(
            caster_packets
                .iter()
                .any(|packet| matches!(packet, ServerPacket::UserDashFail(_))),
            "zero movement must send S.UserDashFail"
        );
        // 第 51 轮返工 P04：失败提示走真实 i18n 键（`HumanObject.cs:5141-5143`）。
        let expected = world.text(1, TextKey::NotEnoughPushingPower);
        assert!(
            caster_packets.iter().any(|packet| matches!(
                packet,
                ServerPacket::Chat(chat) if chat.message.contains(&expected)
            )),
            "zero movement sends the NotEnoughPushingPower chat"
        );
        assert!(
            matches!(caster_packets.last(), Some(ServerPacket::MagicCast(_))),
            "the failure flow still ends with S.MagicCast"
        );
        let observer_packets = round51_packets(&mut observer_rx);
        assert!(
            observer_packets
                .iter()
                .any(|packet| matches!(packet, ServerPacket::ObjectDashFail(_))),
            "zero movement broadcasts S.ObjectDashFail to nearby observers"
        );
    }

    #[test]
    fn shoulder_dash_stops_at_safe_zone() {
        let mut city = round51_map_info("0", false);
        city.safe_zones.push(mir_db::SafeZoneInfo {
            location: Point::new(10, 11),
            size: 0,
            start_point: false,
        });
        let mut world = round51_world(vec![round51_magic(Spell::ShoulderDash, 0)], vec![city]);
        let mut caster_rx = round51_add_player(&mut world, 1, "0", Point::new(10, 10));
        round51_learn(&mut world, 1, Spell::ShoulderDash, 3);
        round51_packets(&mut caster_rx);

        world
            .cast_magic(
                1,
                Spell::ShoulderDash,
                MirDirection::Down,
                0,
                Point::new(0, 0),
            )
            .expect("施法成功");
        assert_eq!(
            world.player(1).expect("玩家").position,
            Point::new(10, 10),
            "next cell is a safe zone -> zero movement (GetSafeZone != null breaks)"
        );
        assert!(
            round51_packets(&mut caster_rx)
                .iter()
                .any(|packet| matches!(packet, ServerPacket::UserDashFail(_))),
            "a safe-zone block also sends S.UserDashFail"
        );
    }

    #[test]
    fn shoulder_dash_pushes_monster_and_damages_it() {
        let mut world = round51_default_world(vec![round51_magic(Spell::ShoulderDash, 0)]);
        let _caster_rx = round51_add_player(&mut world, 1, "0", Point::new(10, 10));
        let mut observer_rx = round51_add_player(&mut world, 2, "0", Point::new(5, 5));
        let monster =
            round51_spawn_monster(&mut world, MonsterKind::skeleton(), Point::new(10, 11));
        round51_learn(&mut world, 1, Spell::ShoulderDash, 3);
        round51_packets(&mut observer_rx);

        world
            .cast_magic(
                1,
                Spell::ShoulderDash,
                MirDirection::Down,
                0,
                Point::new(0, 0),
            )
            .expect("冲撞施法成功");
        let travelled = world.player(1).expect("玩家").position.y - 10;
        assert!((5..=6).contains(&travelled), "冲撞距离 5~6 格");
        let pushed = world.monster(monster).expect("怪物");
        assert_eq!(
            pushed.position,
            Point::new(10, 11 + travelled),
            "one MonsterObject.Pushed(direction, 1) per travelled cell"
        );
        assert_eq!(
            pushed.direction,
            rotate_direction(MirDirection::Down, 4),
            "the pushed monster turns to the opposite direction"
        );
        assert!(pushed.hp < 10_000, "撞到目标后结算 GetDamage(0) 伤害");

        let packets = round51_packets(&mut observer_rx);
        assert!(
            packets
                .iter()
                .any(|packet| matches!(packet, ServerPacket::ObjectDash(_))),
            "dash broadcasts S.ObjectDash per cell"
        );
        assert!(
            packets
                .iter()
                .any(|packet| matches!(packet, ServerPacket::ObjectPushed(_))),
            "push broadcasts S.ObjectPushed per cell"
        );
    }

    // --- R52-B3：TrapHexagon 已接真实实现 ---------------------------------
    //
    // 行为断言全部搬到 integration 测试 `crates/mir-server/tests/round52_trap_hexagon.rs`；
    // 这里只保留「不再是 `Unsupported`」和「预检无目标时 `cast = false`、不排效果」两条。

    #[test]
    fn trap_hexagon_is_implemented_and_refuses_without_targets() {
        let mut world = round51_default_world(vec![
            round51_magic(Spell::PoisonCloud, 8),
            round51_magic(Spell::TrapHexagon, 8),
        ]);
        let _caster_rx = round51_add_player(&mut world, 1, "0", Point::new(10, 10));
        assert_eq!(spell_kind(Spell::PoisonCloud), SpellKind::PoisonCloud);
        let spell = Spell::TrapHexagon;
        assert_eq!(spell_kind(spell), SpellKind::TrapHexagon);
        round51_learn(&mut world, 1, spell, 1);
        let (mp, action_time, spell_time, casts) = {
            let player = world.player(1).expect("玩家");
            (
                player.mp,
                player.action_time,
                player.magic.spell_time,
                player.magic.casts.clone(),
            )
        };
        let report = world
            .cast_magic(1, spell, MirDirection::Down, 0, Point::new(10, 10))
            .expect("预检无目标不是错误：只是 cast = false");
        assert_eq!(report.spell, spell);
        {
            let player = world.player(1).expect("玩家");
            // 公共检查（动作/SpellTime/冷却/MP）已经发生，不因耗材段落失败回滚。
            assert!(player.mp < mp, "公共检查的 MP 已经扣掉");
            assert!(player.action_time > action_time, "动作锁定保留");
            assert!(player.magic.spell_time > spell_time, "SpellTime 保留");
            assert_eq!(
                player.magic.casts, casts,
                "cast = false 不写本技能 CastTime"
            );
        }
        assert!(world.pending_magic().is_empty(), "不排延迟效果");
        assert!(world.spell_fields().is_empty(), "没有落地就不生成场");
        world
            .player_mut(1)
            .expect("玩家")
            .magics
            .retain(|magic| magic.spell != spell);
    }

    // -----------------------------------------------------------------------
    // 第 51 轮返工（R51-B2）：审计 P03–P06 的行为断言
    // -----------------------------------------------------------------------

    #[test]
    fn frost_crunch_control_poisons_freeze_and_slow_monsters() {
        let mut world = round51_default_world(vec![]);
        let _rx = round51_add_player(&mut world, 1, "0", Point::new(10, 10));
        let monster =
            round51_spawn_monster(&mut world, MonsterKind::skeleton(), Point::new(12, 10));
        let now = world.now();
        // Frozen（Crystal `HumanObject.cs:5986`：TickSpeed=1000、时长 2 跳）。
        let frozen = PoisonState {
            kind: PoisonType::Frozen,
            value: 0,
            duration: 2,
            ticks: 0,
            tick_at: now + FROST_CRUNCH_POISON_TICK_MS,
            tick_speed_ms: FROST_CRUNCH_POISON_TICK_MS,
            owner_id: 1,
        };
        assert!(world.apply_poison_to_monster(monster, frozen));
        assert!(
            !world.monster(monster).expect("怪物").can_move(now),
            "Frozen blocks monster movement (MonsterObject.cs:640)"
        );
        assert!(
            !world.monster(monster).expect("怪物").can_attack(now),
            "Frozen blocks monster attacks (MonsterObject.cs:654)"
        );

        // Slow（Crystal `MonsterObject.cs:1544-1553`：每跳 MoveSpeed/AttackSpeed +100，封顶 3500）。
        let base_move = world.monster(monster).expect("怪物").move_speed_ms;
        let base_attack = world.monster(monster).expect("怪物").attack_speed_ms;
        let slow = PoisonState {
            kind: PoisonType::Slow,
            value: 0,
            duration: 3,
            ticks: 0,
            tick_at: now + FROST_CRUNCH_POISON_TICK_MS,
            tick_speed_ms: FROST_CRUNCH_POISON_TICK_MS,
            owner_id: 1,
        };
        assert!(world.apply_poison_to_monster(monster, slow));

        world.advance_clock(FROST_CRUNCH_POISON_TICK_MS + 10);
        let now = world.now();
        let mut ticked = world.monster(monster).expect("怪物").clone();
        world.tick_monster_poison(&mut ticked, now);
        *world.monster_mut(monster).expect("怪物") = ticked;
        let ticked = world.monster(monster).expect("怪物");
        assert_eq!(
            ticked.move_speed_ms,
            base_move + MONSTER_SLOW_STEP_MS,
            "Slow adds 100ms to move speed per tick"
        );
        assert_eq!(
            ticked.attack_speed_ms,
            base_attack + MONSTER_SLOW_STEP_MS,
            "Slow adds 100ms to attack speed per tick"
        );
        assert!(
            !ticked.can_move(world.now()),
            "still frozen before the second tick, cannot move"
        );

        // 毒到期后恢复原始速度（`MonsterObject.cs:1551-1553`）。
        for _ in 0..6 {
            world.advance_clock(FROST_CRUNCH_POISON_TICK_MS);
            let now = world.now();
            let mut ticked = world.monster(monster).expect("怪物").clone();
            world.tick_monster_poison(&mut ticked, now);
            *world.monster_mut(monster).expect("怪物") = ticked;
        }
        let expired = world.monster(monster).expect("怪物");
        assert_eq!(
            expired.move_speed_ms, base_move,
            "Slow 到期恢复 Info.MoveSpeed"
        );
        assert_eq!(
            expired.attack_speed_ms, base_attack,
            "Slow expiry restores Info.AttackSpeed"
        );
        assert!(expired.can_move(world.now()), "Frozen 到期后恢复移动");
    }

    #[test]
    fn player_frozen_blocks_moves_and_slow_doubles_action_delay() {
        let mut world = round51_default_world(vec![]);
        let _rx = round51_add_player(&mut world, 1, "0", Point::new(10, 10));
        let now = world.now();
        let frozen = PoisonState {
            kind: PoisonType::Frozen,
            value: 0,
            duration: 2,
            ticks: 0,
            tick_at: now + FROST_CRUNCH_POISON_TICK_MS,
            tick_speed_ms: FROST_CRUNCH_POISON_TICK_MS,
            owner_id: 2,
        };
        assert!(world.apply_poison_to_player(1, frozen));
        assert!(
            world.walk(1, MirDirection::Down).is_err(),
            "a frozen player cannot walk (HumanObject.cs:121-131)"
        );
        assert!(
            world.run(1, MirDirection::Down).is_err(),
            "a frozen player cannot run"
        );

        let slow = PoisonState {
            kind: PoisonType::Slow,
            value: 0,
            duration: 2,
            ticks: 0,
            tick_at: now + FROST_CRUNCH_POISON_TICK_MS,
            tick_speed_ms: FROST_CRUNCH_POISON_TICK_MS,
            owner_id: 2,
        };
        assert!(world.apply_poison_to_player(1, slow));
        let player = world.player(1).expect("玩家");
        assert_eq!(
            player.action_delay(MOVE_DELAY_MS),
            MOVE_DELAY_MS * 2,
            "Slow doubles the player GetDelayTime (HumanObject.cs:249)"
        );
    }

    #[test]
    fn shoulder_dash_zero_step_still_sets_cell_time_and_chats() {
        let mut world = round51_default_world(vec![round51_magic(Spell::ShoulderDash, 0)]);
        let mut caster_rx = round51_add_player(&mut world, 1, "0", Point::new(10, 29));
        round51_learn(&mut world, 1, Spell::ShoulderDash, 3);
        round51_packets(&mut caster_rx);
        let now = world.now();
        // 关键：先放一个**非零**的 StepCounter，才能证明零步失败路径把它清零
        // （Crystal `HumanObject.cs:5155-5156` 无条件 `CellTime = now+500; _stepCounter = 0`）。
        world.player_mut(1).expect("玩家").step_counter = 3;

        world
            .cast_magic(
                1,
                Spell::ShoulderDash,
                MirDirection::Down,
                0,
                Point::new(0, 0),
            )
            .expect("冲撞施法成功");
        let caster = world.player(1).expect("玩家");
        assert_eq!(caster.position, Point::new(10, 29), "地图边缘一格未走");
        assert_eq!(
            caster.cell_time,
            now + SHOULDER_DASH_CELL_TIME_MS,
            "zero-step failure still sets CellTime = now+500 (HumanObject.cs:5152)"
        );
        assert_eq!(caster.step_counter, 0, "StepCounter 归零");
        let expected = world.text(1, TextKey::NotEnoughPushingPower);
        let packets = round51_packets(&mut caster_rx);
        assert!(
            packets.iter().any(|packet| matches!(
                packet,
                ServerPacket::Chat(chat) if chat.message.contains(&expected)
            )),
            "zero-step failure sends the NotEnoughPushingPower chat (HumanObject.cs:5141-5143)"
        );
        assert!(
            packets
                .iter()
                .any(|packet| matches!(packet, ServerPacket::UserDashFail(_))),
            "still emits S.UserDashFail"
        );
    }

    #[test]
    fn shoulder_dash_failure_chat_uses_the_safezone_text() {
        let mut city = round51_map_info("0", false);
        city.safe_zones.push(mir_db::SafeZoneInfo {
            location: Point::new(10, 29),
            size: 3,
            start_point: false,
        });
        let mut world = round51_world(vec![round51_magic(Spell::ShoulderDash, 0)], vec![city]);
        let mut caster_rx = round51_add_player(&mut world, 1, "0", Point::new(10, 29));
        round51_learn(&mut world, 1, Spell::ShoulderDash, 3);
        round51_packets(&mut caster_rx);

        world
            .cast_magic(
                1,
                Spell::ShoulderDash,
                MirDirection::Down,
                0,
                Point::new(0, 0),
            )
            .expect("冲撞施法成功");
        let expected = world.text(1, TextKey::NoPushingInSafezone);
        assert!(
            round51_packets(&mut caster_rx)
                .iter()
                .any(|packet| matches!(
                    packet,
                    ServerPacket::Chat(chat) if chat.message.contains(&expected)
                )),
            "safe-zone failure sends the NoPushingInSafezone chat"
        );
    }

    #[test]
    fn shoulder_dash_pushes_lower_level_players() {
        let mut world = round51_default_world(vec![round51_magic(Spell::ShoulderDash, 0)]);
        let _caster_rx = round51_add_player(&mut world, 1, "0", Point::new(10, 10));
        let (mut target, mut target_rx) = test_player(2, "0", Point::new(10, 11), 1, false);
        target.attack_mode = mir_core::enums::AttackMode::All;
        world.enter(target);
        world.player_mut(1).expect("施法者").attack_mode = mir_core::enums::AttackMode::All;
        round51_learn(&mut world, 1, Spell::ShoulderDash, 3);
        round51_packets(&mut target_rx);

        world
            .cast_magic(
                1,
                Spell::ShoulderDash,
                MirDirection::Down,
                0,
                Point::new(0, 0),
            )
            .expect("冲撞施法成功");
        let caster = world.player(1).expect("施法者").position;
        let pushed = world.player(2).expect("目标").position;
        assert!(caster.y > 10, "低等级玩家被推走后施法者跟进");
        assert!(
            pushed.y >= 12,
            "low-level player is pushed cell by cell (HumanObject.Pushed), got {pushed:?}"
        );
        assert_eq!(pushed.x, 10, "只沿冲撞方向推");
        let packets = round51_packets(&mut target_rx);
        assert!(
            packets
                .iter()
                .any(|packet| matches!(packet, ServerPacket::Pushed(_))),
            "the pushed player receives S.Pushed (HumanObject.cs:2712)"
        );
    }

    #[test]
    fn shoulder_dash_is_blocked_by_same_level_players() {
        let mut world = round51_default_world(vec![round51_magic(Spell::ShoulderDash, 0)]);
        let _caster_rx = round51_add_player(&mut world, 1, "0", Point::new(10, 10));
        let _target_rx = round51_add_player(&mut world, 2, "0", Point::new(10, 11));
        world.player_mut(1).expect("施法者").attack_mode = mir_core::enums::AttackMode::All;
        world.player_mut(2).expect("目标").attack_mode = mir_core::enums::AttackMode::All;
        round51_learn(&mut world, 1, Spell::ShoulderDash, 3);

        world
            .cast_magic(
                1,
                Spell::ShoulderDash,
                MirDirection::Down,
                0,
                Point::new(0, 0),
            )
            .expect("冲撞施法成功");
        assert_eq!(
            world.player(1).expect("施法者").position,
            Point::new(10, 10),
            "target level < caster level is required, so the player blocks and dash travels zero cells"
        );
        assert_eq!(
            world.player(2).expect("目标").position,
            Point::new(10, 11),
            "same-level player cannot be pushed"
        );
    }

    #[test]
    fn thunder_storm_resolves_against_the_map_it_was_cast_on() {
        let mut world = round51_default_world(vec![round51_magic(Spell::ThunderStorm, 0)]);
        let _caster_rx = round51_add_player(&mut world, 1, "0", Point::new(10, 10));
        let old_map_monster =
            round51_spawn_monster(&mut world, MonsterKind::skeleton(), Point::new(11, 10));
        let nt_monster = world.spawn_monster("nt", MonsterKind::skeleton(), Point::new(10, 10));
        if let Some(monster) = world.monster_mut(nt_monster) {
            monster.operate_time = i64::MAX;
            monster.hp = 10_000;
            monster.stats.hp = 10_000;
        }
        round51_learn(&mut world, 1, Spell::ThunderStorm, 3);

        world
            .cast_magic(
                1,
                Spell::ThunderStorm,
                MirDirection::Down,
                0,
                Point::new(0, 0),
            )
            .expect("雷暴施法成功");
        // 500ms 延迟动作期间玩家换图（Crystal 动作记录挂在施法时的 CurrentMap.ActionList）。
        {
            let caster = world.player_mut(1).expect("玩家");
            caster.map = "nt".to_owned();
            caster.position = Point::new(20, 20);
        }
        round51_resolve_after(&mut world, THUNDER_STORM_DELAY_MS);
        assert!(
            world.monster(old_map_monster).expect("怪物").hp < 10_000,
            "lightning resolves on the map cast on (0), not the caster current map"
        );
        assert_eq!(
            world.monster(nt_monster).expect("怪物").hp,
            10_000,
            "a monster at the same cell on another map is unaffected"
        );
    }

    #[test]
    fn storm_escape_keeps_lightning_damage_when_teleport_is_forbidden() {
        let mut world = round51_default_world(vec![round51_magic(Spell::StormEscape, 9)]);
        let _caster_rx = round51_add_player(&mut world, 1, "nt", Point::new(10, 10));
        let monster = world.spawn_monster("nt", MonsterKind::skeleton(), Point::new(11, 10));
        if let Some(monster) = world.monster_mut(monster) {
            monster.operate_time = i64::MAX;
            monster.hp = 10_000;
            monster.stats.hp = 10_000;
            monster.stats.min_mac = 0;
            monster.stats.max_mac = 0;
        }
        round51_learn(&mut world, 1, Spell::StormEscape, 3);

        world
            .cast_magic(
                1,
                Spell::StormEscape,
                MirDirection::Down,
                0,
                Point::new(14, 10),
            )
            .expect("狂风绝命施法成功");
        round51_resolve_after(&mut world, THUNDER_STORM_DELAY_MS);
        assert!(
            world.monster(monster).expect("怪物").hp < 10_000,
            "teleport failure keeps the lightning damage already dealt at 500ms"
        );
        round51_resolve_after(&mut world, STORM_ESCAPE_DELAY_MS - THUNDER_STORM_DELAY_MS);
        assert_eq!(
            world.player(1).expect("施法者").position,
            Point::new(10, 10),
            "on a no-teleport map the teleport fails and position is unchanged"
        );
        assert!(
            !world
                .buffs(1)
                .iter()
                .any(|buff| buff.kind == BuffKind::TemporalFlux),
            "teleport failure does not apply TemporalFlux"
        );
    }

    #[test]
    fn teleport_penalty_is_a_real_temporal_flux_buff() {
        let mut magic = round51_magic(Spell::Blink, 9);
        magic.delay_base = 0;
        let mut world = round51_default_world(vec![magic]);
        let _rx = round51_add_player(&mut world, 1, "0", Point::new(10, 10));
        // `round51_add_player` 给 10_000 蓝，但 `change_player_mp` 会截到上限；
        // 先回满，差值才等于真实消耗。
        let max_mp = world.player(1).expect("玩家").stats.mp;
        world.player_mut(1).expect("玩家").mp = max_mp;
        round51_learn(&mut world, 1, Spell::Blink, 3);
        let base_cost = 4; // BaseCost 1 + LevelCost 1 * Level 3
        let penalty_cost = base_cost + base_cost * TELEPORT_MANA_PENALTY_PERCENT / 100;

        let mp = world.player(1).expect("玩家").mp;
        world
            .cast_magic(1, Spell::Blink, MirDirection::Down, 0, Point::new(10, 13))
            .expect("第一次瞬移");
        assert_eq!(
            mp - world.player(1).expect("玩家").mp,
            base_cost,
            "no penalty without TemporalFlux"
        );
        round51_resolve_after(&mut world, BLINK_DELAY_MS);
        let buff = world
            .buffs(1)
            .iter()
            .find(|buff| buff.kind == BuffKind::TemporalFlux)
            .expect("落地后获得 30s TemporalFlux");
        assert_eq!(
            buff.expire_at,
            world.now() + TELEPORT_PENALTY_MS,
            "TemporalFlux 30s"
        );

        world.advance_clock(1_000);
        // 第一次施法把 ActionTime/SpellTime 推到 now+1800；跳过法术硬直再测第二次耗蓝。
        world.player_mut(1).expect("玩家").action_time = 0;
        world.player_mut(1).expect("玩家").magic.spell_time = 0;
        let mp = world.player(1).expect("玩家").mp;
        world
            .cast_magic(1, Spell::Blink, MirDirection::Down, 0, Point::new(10, 10))
            .expect("第二次瞬移");
        assert_eq!(
            mp - world.player(1).expect("玩家").mp,
            penalty_cost,
            "blink costs 30% more mana while TemporalFlux is active"
        );

        world.advance_clock(TELEPORT_PENALTY_MS);
        world.tick_buffs(world.now());
        assert!(
            !world
                .buffs(1)
                .iter()
                .any(|buff| buff.kind == BuffKind::TemporalFlux),
            "TemporalFlux expires after 30s"
        );
        let mp = world.player(1).expect("玩家").mp;
        world
            .cast_magic(1, Spell::Blink, MirDirection::Down, 0, Point::new(10, 13))
            .expect("第三次瞬移");
        assert_eq!(
            mp - world.player(1).expect("玩家").mp,
            base_cost,
            "no penalty once the buff expires"
        );
    }

    #[test]
    fn frost_crunch_freezing_added_stats_source_extends_the_frozen_roll() {
        // 本用例**只覆盖附加属性来源**（`UserItem.added_stats`，Crystal
        // `HumanObject.cs:1965` 的 `Stats.Add(temp.AddedStats)`）；模板属性与 buff
        // 属性来源见 `caster_freezing_sums_template_added_and_buff_stats`。
        // 用一个存在但 Freezing=0、无耐久的模板，确保测试点在附加属性而非模板查表。
        let plain = mir_db::ItemInfo {
            index: 1,
            name: "PlainRing".to_owned(),
            ..Default::default()
        };
        let mut world =
            round51_world_with_items(vec![], vec![round51_map_info("0", false)], vec![plain]);
        let _rx = round51_add_player(&mut world, 1, "0", Point::new(10, 10));
        assert_eq!(world.caster_freezing(1), 0, "无 Freezing 装备时为 0");
        let mut item = mir_protocol::data::UserItem {
            item_index: 1,
            current_dura: 10_000,
            ..Default::default()
        };
        item.added_stats.set(Stat::Freezing, 17);
        world.player_mut(1).expect("玩家").equipment[0] = Some(item);
        assert_eq!(
            world.caster_freezing(1),
            17,
            "读取装备附加属性的 Freezing（item_index 默认模板无 Freezing）"
        );
        for _ in 0..64 {
            let bonus = world.freeze_roll(1);
            assert!(
                (0..=16).contains(&bonus),
                "Freezing=17 → Random.Next(17) ∈ 0..=16（HumanObject.cs:5987）"
            );
        }
        world.player_mut(1).expect("玩家").equipment[0] = None;
        assert_eq!(world.freeze_roll(1), 0, "Freezing=0 时不抽取、额外时长 0");
    }

    /// 反复施放 FrostCrunch（每次重置法术硬直/冷却/蓝量与目标血量），直到目标真的
    /// 吃到 Frozen 毒；返回那条毒。固定种子（`World::new(999)`）保证结果确定可复现。
    fn round51_cast_frost_until_frozen(world: &mut World, monster: u32) -> PoisonState {
        let monster_position = world.monster(monster).expect("怪物").position;
        for _ in 0..400 {
            {
                let player = world.player_mut(1).expect("玩家");
                player.action_time = 0;
                player.magic.spell_time = 0;
                player.magic.casts.remove(&Spell::FrostCrunch);
                player.mp = player.stats.mp;
            }
            {
                let target = world.monster_mut(monster).expect("怪物");
                target.hp = 10_000;
                target.stats.hp = 10_000;
            }
            world
                .cast_magic(
                    1,
                    Spell::FrostCrunch,
                    MirDirection::Right,
                    monster,
                    monster_position,
                )
                .expect("冰咆哮施法成功");
            // 飞行延迟 = MaxDistance*50 + 500（Crystal `HumanObject.cs:3939-3952`）。
            let distance = world
                .player(1)
                .expect("玩家")
                .position
                .max_distance(monster_position);
            world.advance_clock(i64::from(distance) * 50 + 500);
            world.resolve_pending_magic(world.now());
            if let Some(poison) = world
                .monster(monster)
                .expect("怪物")
                .poisons
                .iter()
                .find(|poison| poison.kind == PoisonType::Frozen)
            {
                return *poison;
            }
        }
        panic!("400 次施法都没吃到 Frozen 毒（固定种子下不应发生）");
    }

    #[test]
    fn caster_freezing_sums_template_added_and_buff_stats() {
        // Crystal `RefreshStats`：装备模板属性（`HumanObject.cs:1858`）+
        // 附加属性（`:1965`）+ 生效 buff 属性（`:2332`），最终 `:5987` 读总和。
        let template = mir_db::ItemInfo {
            index: 1,
            name: "FreezingRing".to_owned(),
            stats: {
                let mut stats = Stats::default();
                stats.set(Stat::Freezing, 7);
                stats
            },
            ..Default::default()
        };
        let mut world = round51_world_with_items(
            vec![round51_magic(Spell::FrostCrunch, 9)],
            vec![round51_map_info("0", false), round51_map_info("nt", true)],
            vec![template],
        );
        let _rx = round51_add_player(&mut world, 1, "0", Point::new(10, 10));
        round51_learn(&mut world, 1, Spell::FrostCrunch, 3);
        let monster =
            round51_spawn_monster(&mut world, MonsterKind::skeleton(), Point::new(12, 10));

        // ① 装备模板属性 7。
        let item = mir_protocol::data::UserItem {
            item_index: 1,
            ..Default::default()
        };
        world.player_mut(1).expect("玩家").equipment[0] = Some(item);
        assert_eq!(world.caster_freezing(1), 7, "模板属性 Freezing=7");
        // ② 附加属性 +5。
        world.player_mut(1).expect("玩家").equipment[0]
            .as_mut()
            .expect("装备")
            .added_stats
            .set(Stat::Freezing, 5);
        assert_eq!(world.caster_freezing(1), 12, "模板 7 + 附加 5 = 12");
        // ③ 生效 buff 属性 +3。
        let mut buff_stats = Stats::default();
        buff_stats.set(Stat::Freezing, 3);
        assert!(world.add_buff(1, BuffKind::TemporalFlux, 30_000, buff_stats, 1));
        assert_eq!(
            world.caster_freezing(1),
            15,
            "模板 7 + 附加 5 + buff 3 = 15"
        );

        // `Random.Next(15)` ∈ 0..=14；固定种子下多次抽取必定出现 >0（真正消耗了 RNG）。
        let mut saw_positive = false;
        for _ in 0..64 {
            let bonus = world.freeze_roll(1);
            assert!((0..=14).contains(&bonus), "Freezing=15 → 额外时长 ∈ 0..=14");
            saw_positive |= bonus > 0;
        }
        assert!(saw_positive, "Freezing=15 时 Random.Next 应至少抽到一次 >0");

        // 真实施法：目标 Frozen 毒时长 = 5 + roll ∈ 5..=19（`HumanObject.cs:5987`）。
        let frozen = round51_cast_frost_until_frozen(&mut world, monster);
        assert!(
            (5..=19).contains(&frozen.duration),
            "Frozen 时长 = 5 + Random.Next(15) ∈ 5..=19，实际 {}",
            frozen.duration
        );
    }

    #[test]
    fn frozen_duration_is_five_without_any_freezing_source() {
        // 三个来源全 0 → Crystal `Random.Next(0)` 恒 0 → 时长 5，且不消耗随机数。
        let mut world = round51_default_world(vec![round51_magic(Spell::FrostCrunch, 9)]);
        let _rx = round51_add_player(&mut world, 1, "0", Point::new(10, 10));
        round51_learn(&mut world, 1, Spell::FrostCrunch, 3);
        let monster =
            round51_spawn_monster(&mut world, MonsterKind::skeleton(), Point::new(12, 10));
        assert_eq!(world.caster_freezing(1), 0);
        let frozen = round51_cast_frost_until_frozen(&mut world, monster);
        assert_eq!(frozen.duration, 5, "无 Freezing 来源时 Frozen 固定 5 跳");
        assert_eq!(world.freeze_roll(1), 0, "Freezing=0 时额外时长恒 0");
    }

    #[test]
    fn damaged_equipment_does_not_contribute_freezing() {
        // B4-1（Lead 指定参数）：Crystal `HumanObject.cs:1836` 在叠
        // `Stats.Add(realItem.Stats)` / `Stats.Add(temp.AddedStats)`（`:1858-1859`，
        // 普通装备的模板 + 附加）**之前**就跳过 `CurrentDura == 0 && Info.Durability > 0`
        // 的损坏装备；本作 `items.rs:912-915 apply_equipment_stats` 同款。
        // 注：`:1965-1966` 是 `RefreshSocketStats` 的插槽模板/附加，不是普通 AddedStats，
        // 这里只引 `:1858-1859`。
        let template = mir_db::ItemInfo {
            index: 1,
            name: "FreezingRing".to_owned(),
            durability: 10_000,
            stats: {
                let mut stats = Stats::default();
                stats.set(Stat::Freezing, 7);
                stats
            },
            ..Default::default()
        };
        let mut world = round51_world_with_items(
            vec![round51_magic(Spell::FrostCrunch, 9)],
            vec![round51_map_info("0", false)],
            vec![template],
        );
        let _rx = round51_add_player(&mut world, 1, "0", Point::new(10, 10));
        round51_learn(&mut world, 1, Spell::FrostCrunch, 3);
        let monster =
            round51_spawn_monster(&mut world, MonsterKind::skeleton(), Point::new(12, 10));

        // ① 损坏（current_dura == 0 且模板 durability > 0）：模板 7 + 附加 5 都不生效。
        let mut damaged = mir_protocol::data::UserItem {
            item_index: 1,
            current_dura: 0,
            ..Default::default()
        };
        damaged.added_stats.set(Stat::Freezing, 5);
        world.player_mut(1).expect("玩家").equipment[0] = Some(damaged);
        assert_eq!(
            world.caster_freezing(1),
            0,
            "broken item template 7 + added 5 must not contribute Freezing (HumanObject.cs:1836)"
        );
        let frozen = round51_cast_frost_until_frozen(&mut world, monster);
        assert_eq!(
            frozen.duration, 5,
            "broken item -> same as no source, Frozen lasts exactly 5 ticks"
        );
        assert_eq!(
            world.freeze_roll(1),
            0,
            "broken item -> Freezing=0, consumes no rng"
        );

        // ② 修复（current_dura > 0）：模板 7 + 附加 5 = 12 生效（同一测试正反差）。
        world.player_mut(1).expect("玩家").equipment[0]
            .as_mut()
            .expect("装备")
            .current_dura = 10_000;
        assert_eq!(
            world.caster_freezing(1),
            12,
            "intact item template 7 + added 5 = 12 takes effect (HumanObject.cs:1858-1859)"
        );
        let mut saw_positive = false;
        for _ in 0..64 {
            let bonus = world.freeze_roll(1);
            assert!(
                (0..=11).contains(&bonus),
                "Freezing=12 -> bonus duration in 0..=11"
            );
            saw_positive |= bonus > 0;
        }
        assert!(saw_positive, "Freezing=12 时应至少抽到一次 >0");
    }

    #[test]
    fn no_durability_equipment_still_contributes_freezing() {
        // B4 补充边界（Lead）：Crystal `HumanObject.cs:1836` 的跳过条件是
        // `CurrentDura == 0 && Info.Durability > 0`；`Info.Durability == 0` 的模板
        // **不**受该条件影响——与 `items.rs:908-925 apply_equipment_stats` 的
        // `info.durability > 0` 判断一致。本用例证明损坏过滤没有过度过滤。
        let template = mir_db::ItemInfo {
            index: 1,
            name: "NoDuraRing".to_owned(),
            durability: 0,
            stats: {
                let mut stats = Stats::default();
                stats.set(Stat::Freezing, 5);
                stats
            },
            ..Default::default()
        };
        let mut world = round51_world_with_items(
            vec![round51_magic(Spell::FrostCrunch, 9)],
            vec![round51_map_info("0", false)],
            vec![template],
        );
        let _rx = round51_add_player(&mut world, 1, "0", Point::new(10, 10));
        let item = mir_protocol::data::UserItem {
            item_index: 1,
            current_dura: 0,
            ..Default::default()
        };
        world.player_mut(1).expect("玩家").equipment[0] = Some(item);
        // 模板耐久为 0 → 即使 current_dura == 0 也照常贡献属性。
        assert_eq!(
            world.caster_freezing(1),
            5,
            "equipment with durability == 0 still contributes Freezing=5 (HumanObject.cs:1836 does not apply)"
        );
        let mut saw_positive = false;
        for _ in 0..64 {
            let bonus = world.freeze_roll(1);
            assert!((0..=4).contains(&bonus), "Freezing=5 → 额外时长 ∈ 0..=4");
            saw_positive |= bonus > 0;
        }
        assert!(saw_positive, "Freezing=5 时应至少抽到一次 >0");
    }

    #[test]
    fn buff_freezing_alone_extends_the_frozen_roll() {
        // B4 补充边界（Lead）：buff 是独立来源（`HumanObject.cs:2332` RefreshBuffs 的
        // `Stats.Add(buff.Stats)`）。本用例不穿任何 Freezing 装备，只挂一个
        // Freezing=3 的生效增益：`caster_freezing == 3`，roll 上限 = Freezing-1。
        let mut world = round51_default_world(vec![round51_magic(Spell::FrostCrunch, 9)]);
        let _rx = round51_add_player(&mut world, 1, "0", Point::new(10, 10));
        round51_learn(&mut world, 1, Spell::FrostCrunch, 3);
        let monster =
            round51_spawn_monster(&mut world, MonsterKind::skeleton(), Point::new(12, 10));
        assert_eq!(world.caster_freezing(1), 0, "无装备无 buff 时为 0");
        let mut buff_stats = Stats::default();
        buff_stats.set(Stat::Freezing, 3);
        assert!(world.add_buff(1, BuffKind::TemporalFlux, 30_000, buff_stats.clone(), 1));
        assert_eq!(
            world.caster_freezing(1),
            3,
            "buff alone contributes Freezing=3 (HumanObject.cs:2332)"
        );
        let mut saw_positive = false;
        for _ in 0..64 {
            let bonus = world.freeze_roll(1);
            assert!((0..=2).contains(&bonus), "Freezing=3 → 额外时长 ∈ 0..=2");
            saw_positive |= bonus > 0;
        }
        assert!(saw_positive, "Freezing=3 时应至少抽到一次 >0");
        // 真实施法：Frozen 时长 = 5 + roll ∈ 5..=7。
        let frozen = round51_cast_frost_until_frozen(&mut world, monster);
        assert!(
            (5..=7).contains(&frozen.duration),
            "buff-only Freezing=3 -> Frozen duration in 5..=7, actual {}",
            frozen.duration
        );

        // B4-3 边界（Lead）：buff 移除后贡献归 0。
        assert!(world.remove_buff(1, BuffKind::TemporalFlux));
        assert_eq!(
            world.caster_freezing(1),
            0,
            "Freezing contribution is 0 after the buff is removed (buffs.rs:387 remove_buff)"
        );

        // B4-3 边界（Lead）：buff 到期（tick_buffs 清理）后贡献归 0。
        assert!(world.add_buff(1, BuffKind::TemporalFlux, 1_000, buff_stats.clone(), 1));
        assert_eq!(world.caster_freezing(1), 3, "重新挂上 buff 后恢复 3");
        world.advance_clock(1_000);
        let now = world.now();
        world.tick_buffs(now);
        assert_eq!(
            world.caster_freezing(1),
            0,
            "Freezing contribution is 0 after the buff expires (buffs.rs:454 tick_buffs)"
        );
    }

    #[test]
    fn frost_crunch_real_cast_uses_the_freezing_roll() {
        // B4-4（Lead/T3 明确缺口）：既有施毒测试只断言 duration ∈ 5..19，恒写 5 也会绿。
        // 本用例固定 rng 种子、让模板 Freezing=7，真实施法得到的 Frozen 毒时长必须
        // 落在 5..=12 且**至少一个种子 > 5**——把“5 + roll”写死成 5 的实现会失败。
        let mut rolled_above_five = Vec::new();
        for seed in 0..=23u64 {
            let template = mir_db::ItemInfo {
                index: 1,
                name: "FreezingRing".to_owned(),
                durability: 0,
                stats: {
                    let mut stats = Stats::default();
                    stats.set(Stat::Freezing, 7);
                    stats
                },
                ..Default::default()
            };
            let mut world = round51_world_with_items(
                vec![round51_magic(Spell::FrostCrunch, 9)],
                vec![round51_map_info("0", false)],
                vec![template],
            );
            world.seed_rng(seed);
            let _rx = round51_add_player(&mut world, 1, "0", Point::new(10, 10));
            round51_learn(&mut world, 1, Spell::FrostCrunch, 3);
            let item = mir_protocol::data::UserItem {
                item_index: 1,
                current_dura: 10_000,
                ..Default::default()
            };
            world.player_mut(1).expect("玩家").equipment[0] = Some(item);
            let monster =
                round51_spawn_monster(&mut world, MonsterKind::skeleton(), Point::new(12, 10));
            let frozen = round51_cast_frost_until_frozen(&mut world, monster);
            assert!(
                (5..=12).contains(&frozen.duration),
                "seed {seed}: template Freezing=7 -> Frozen duration in 5..=12, actual {}",
                frozen.duration
            );
            if frozen.duration > 5 {
                rolled_above_five.push((seed, frozen.duration));
            }
        }
        // 若真实施法恒为 5，该断言失败；Freezing=7 时 roll ∈ 0..=6，24 个种子里
        // 至少一个抽到 >0（固定种子序列完全确定，无随机不稳定）。
        assert!(
            !rolled_above_five.is_empty(),
            "a real cast with Freezing=7 must produce a Frozen duration > 5 at least once; actual {rolled_above_five:?}"
        );
    }

    #[test]
    fn frost_crunch_without_freezing_always_lasts_five_ticks() {
        // B4-4 边界（Lead）：Freezing=0 时真实施法恒 5 跳，与 rng 种子/状态无关
        // （`HumanObject.cs:5987` 的 `Duration = 5 + Random.Next(Freezing)`，参数 0 →
        // 不消耗随机数，恒 5）。
        for seed in [0u64, 12_345] {
            let mut world = round51_default_world(vec![round51_magic(Spell::FrostCrunch, 9)]);
            world.seed_rng(seed);
            let _rx = round51_add_player(&mut world, 1, "0", Point::new(10, 10));
            round51_learn(&mut world, 1, Spell::FrostCrunch, 3);
            let monster =
                round51_spawn_monster(&mut world, MonsterKind::skeleton(), Point::new(12, 10));
            let frozen = round51_cast_frost_until_frozen(&mut world, monster);
            assert_eq!(
                frozen.duration, 5,
                "seed {seed}: Frozen lasts exactly 5 ticks when Freezing=0"
            );
        }
    }

    #[test]
    fn shoulder_dash_aborts_before_packets_when_the_caster_cannot_walk() {
        let mut world = round51_default_world(vec![round51_magic(Spell::ShoulderDash, 0)]);
        let mut rx = round51_add_player(&mut world, 1, "0", Point::new(10, 10));
        round51_learn(&mut world, 1, Spell::ShoulderDash, 3);
        let now = world.now();
        // `HumanObject.CanWalk` 含 Frozen（`HumanObject.cs:121-131`）；`ShoulderDash()`
        // 开头 `if (InTrapRock || !CanWalk) return;`（`:4977-4979`）——`Magic()` 随之直接
        // 返回：一个包都不发（连末尾的 `S.MagicCast` 都不补，也不记 CastTime）。
        assert!(world.apply_poison_to_player(
            1,
            PoisonState {
                kind: PoisonType::Frozen,
                value: 0,
                duration: 2,
                ticks: 0,
                tick_at: now + FROST_CRUNCH_POISON_TICK_MS,
                tick_speed_ms: FROST_CRUNCH_POISON_TICK_MS,
                owner_id: 1,
            }
        ));
        let start = world.player(1).expect("施法者").position;
        let _ = round51_packets(&mut rx);
        // 玩家冻结后客户端照旧能发施法请求（`CanCast` 不拦 Frozen），但 ShoulderDash
        // 的前置会立即返回。
        let report = world
            .cast_magic(
                1,
                Spell::ShoulderDash,
                MirDirection::Down,
                0,
                Point::new(0, 0),
            )
            .expect("冻结时 Magic() 仍走完通用校验并返回 Ok");
        assert!(report.damage.is_none(), "前置失败没有伤害");
        let packets = round51_packets(&mut rx);
        // 蓝在 cost 段已扣（`ChangeMP(-cost)`，`HumanObject.cs:3452`），客户端会收到
        // `S.HealthChanged`；除此之外**一个技能包都没有**（`S.Magic`/`S.MagicCast`/
        // `S.UserDash`/`S.ObjectDash`/`S.ObjectMagic` 全不发）。
        assert!(
            packets
                .iter()
                .all(|packet| matches!(packet, ServerPacket::HealthChanged(_))),
            "a failed prerequisite only echoes the already-paid mana cost, got {packets:?}"
        );
        assert_eq!(
            world.player(1).expect("施法者").position,
            start,
            "frozen caster does not dash"
        );
        assert!(
            !world
                .player(1)
                .expect("施法者")
                .magic
                .casts
                .contains_key(&Spell::ShoulderDash),
            "a failed prerequisite does not set the cooldown (Crystal returns early)"
        );
    }
}
