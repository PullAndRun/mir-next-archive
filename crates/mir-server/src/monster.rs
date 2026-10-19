//! 怪物：种类定义、运行时实例与 AI（`ProcessAI`/`ProcessSearch`/`ProcessRoam`/`ProcessTarget`/`Die`）。
//!
//! AI 必须同时读写「怪物表 + 玩家表 + 待结算伤害队列」，所以它的 `impl World` 写在本模块，
//! 由 [`crate::world::World::tick`] 调用；数据结构（[`Monster`]/[`MonsterKind`]）也在这里。
//!
//! # Crystal 对照
//!
//! | 本模块 | Crystal |
//! |---|---|
//! | [`Monster::new`] | `MonsterObject` 构造函数（`Server/MirObjects/MonsterObject.cs:666-683`）+ `Spawn`/`Spawned`（`:689-744`） |
//! | `World::tick_monsters` | `Envir` 的对象调度（`Server/MirEnvir/Envir.cs:2119-2138`）+ `MonsterObject.Process`（`MonsterObject.cs:1192-1230`） |
//! | `World::process_ai` | `MonsterObject.ProcessAI`（`MonsterObject.cs:1671-1733`） |
//! | `World::check_alone` | `MonsterObject.CheckAlone`（`MonsterObject.cs:1735-1763`） |
//! | `World::process_stacking` | `MonsterObject.ProcessStacking`（`MonsterObject.cs:1765-1802`） |
//! | `World::process_search` | `MonsterObject.ProcessSearch`（`MonsterObject.cs:1804-1813`） |
//! | `World::process_roam` | `MonsterObject.ProcessRoam`（`MonsterObject.cs:1815-1840`） |
//! | `World::process_target` | `MonsterObject.ProcessTarget`（`MonsterObject.cs:1842-1870`） |
//! | `World::find_target` | `MonsterObject.FindTarget`（`MonsterObject.cs:1918-1997`） |
//! | `World::monster_move_to` | `MonsterObject.MoveTo`（`MonsterObject.cs:2027-2071`） |
//! | `World::monster_walk` | `MonsterObject.Walk`（`MonsterObject.cs:2095-2149`） |
//! | `World::monster_turn` | `MonsterObject.Turn`（`MonsterObject.cs:2073-2093`） |
//! | `World::monster_attack` | `MonsterObject.Attack`（`MonsterObject.cs:2150-2173`） |
//! | `World::monster_died` | `MonsterObject.Die`（`MonsterObject.cs:966-1006`） |
//! | `World::monster_operate_time` | `MonsterObject.SetOperateTime`（`MonsterObject.cs:1232-1310`） |
//! | `World::process_monster_regen` | `MonsterObject.ProcessRegen`（`MonsterObject.cs:1414-1452`） |
//!
//! # 简化（每条都写了原因）
//!
//! * 只实现「野生敌对怪物」这一条 AI 分支：没有 `Master`（宠物）、`Route`（巡逻路线）、
//!   毒/麻痹/buff、`ShockTime`、`Rage`、`Hallucination`、隐身、召唤、传送、掉落与经验。
//!   这些都是后续里程碑的内容，且不影响「发现目标 → 追 → 打 → 死」的主链路。
//! * 目标只可能是玩家（怪物之间互相友好，Crystal 的 `IsAttackTarget(MonsterObject)`
//!   对两个野生怪物返回 `HallucinationTime`/`RageTime` 判断，见 `MonsterObject.cs:2537-2539`）。

use std::collections::BTreeSet;
use std::sync::Arc;

use mir_core::MirRng;
use mir_core::Point;
use mir_core::enums::{
    AttackMode, DamageType, DefenceType, MirDirection, Monster as MonsterImage, PetMode,
    PoisonType, Spell,
};
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::{
    ObjectAttack, ObjectDied, ObjectMagic, ObjectRangeAttack, ObjectTurn, ObjectWalk,
};

use crate::aoi::AoiKind;
use crate::combat::{CombatStats, PendingHit};
use crate::magic::{PoisonState, UserMagic};
use crate::world::{RED_NAME_PK_POINTS, VIEW_RANGE, World, monster_info_frame};

/// 隐身玩家对这只怪是否「不可见」：Crystal `ob.Hidden && (!CoolEye || Level < ob.Level)`
/// （`MonsterObject.cs:1974`）。
///
/// * 没隐身 → 可见；
/// * 隐身 + 怪没有破隐（`CoolEye`）→ 不可见；
/// * 隐身 + 破隐但怪等级低于玩家 → 不可见。
fn hidden_from(magic: &crate::magic::MagicState, monster: &Monster, player_level: u16) -> bool {
    magic.hidden && (!monster.cool_eye || monster.kind.level < player_level)
}

/// 搜索间隔：Crystal `MonsterObject.SearchDelay`（`Server/MirObjects/MonsterObject.cs:602`）。
pub const SEARCH_DELAY_MS: i64 = 3_000;
/// 漫游间隔：Crystal `MonsterObject.RoamDelay`（`Server/MirObjects/MonsterObject.cs:602`）。
pub const ROAM_DELAY_MS: i64 = 1_000;
/// 「附近没有玩家」检查间隔：Crystal `MonsterObject.AloneDelay`（`Server/MirObjects/MonsterObject.cs:602`）。
pub const ALONE_DELAY_MS: i64 = 3_000;
/// 自然回血间隔：Crystal `MonsterObject.RegenDelay`（`Server/MirObjects/MonsterObject.cs:602`）。
pub const REGEN_DELAY_MS: i64 = 10_000;
/// 怪物走一步后的动作锁定：Crystal `MonsterObject.Walk` 里的 `ActionTime = now + 300`
/// （`Server/MirObjects/MonsterObject.cs:2128`）。
pub const MONSTER_WALK_ACTION_MS: i64 = 300;
/// 怪物攻击后的动作锁定：Crystal `MonsterObject.Attack` 里的 `ActionTime = now + 300`
/// （`Server/MirObjects/MonsterObject.cs:2165`）。
pub const MONSTER_ATTACK_ACTION_MS: i64 = 300;
/// 怪物攻击到伤害结算之间的延迟：Crystal `DelayedAction(..., now + 300, ...)`
/// （`Server/MirObjects/MonsterObject.cs:2171`）。
pub const MONSTER_ATTACK_DAMAGE_DELAY_MS: i64 = 300;
/// 怪物移动后的格子宽限：Crystal `MonsterObject.Walk` 里的 `CellTime = now + 500`
/// （`Server/MirObjects/MonsterObject.cs:2127`）。
pub const MONSTER_CELL_GRACE_MS: i64 = 500;
/// 出生后的动作锁定：Crystal `MonsterObject.Spawned` 里的 `ActionTime = now + 2000`
/// （`Server/MirObjects/MonsterObject.cs:736`）。
pub const MONSTER_SPAWN_ACTION_MS: i64 = 2_000;
/// 守卫攻击后的动作锁定：Crystal `Monsters/Guard.cs:76`（`ActionTime = Envir.Time + 500`）。
pub const GUARD_ATTACK_ACTION_MS: i64 = 500;
/// 守卫攻击到伤害结算的延迟：Crystal `Monsters/Guard.cs:85`（`DelayedAction(..., now + 300, ...)`）。
pub const GUARD_ATTACK_DAMAGE_DELAY_MS: i64 = 300;
/// 对象调度上限：Crystal `SetOperateTime` 的 `Envir.Time + 2000`（`MonsterObject.cs:1234`）。
pub const MONSTER_OPERATE_DELAY_MS: i64 = 2_000;
/// 出生时随机打散的调度抖动：Crystal `MapObject.Spawned` 的 `OperateDelay = 100`
/// （`Server/MirObjects/MapObject.cs:155,351`）。
pub const MONSTER_OPERATE_JITTER_MS: u32 = 100;
/// 尸体停留时间：Crystal `MonsterObject.DeadDelay` 默认值（`Server/MirObjects/MonsterObject.cs:584-599`）。
pub const CORPSE_DELAY_MS: i64 = 180_000;
/// 默认视野：Crystal `MonsterInfo.ViewRange = 7`（`Server/MirDatabase/MonsterInfo.cs:27`）。
pub const DEFAULT_VIEW_RANGE: i32 = 7;
/// 默认攻击间隔（毫秒）：Crystal `MonsterInfo.AttackSpeed = 2500`（`Server/MirDatabase/MonsterInfo.cs:32`）。
pub const DEFAULT_ATTACK_SPEED_MS: i64 = 2_500;
/// 默认移动间隔（毫秒）：Crystal `MonsterInfo.MoveSpeed = 1800`（`Server/MirDatabase/MonsterInfo.cs:32`）。
pub const DEFAULT_MOVE_SPEED_MS: i64 = 1_800;

/// 怪物的魔法攻击形态（Crystal `MonsterMagic` 子类的简化统一模型）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonsterMagicShape {
    /// 单体指向（如 `HumanWizard` 的 `ThunderBolt`）。
    Single,
    /// 以目标/自身为中心的 AOE。
    Aoe {
        /// 半径（格）。
        radius: i32,
        /// `true` 以目标位置为中心，`false` 以施法者位置为中心。
        at_target: bool,
    },
    /// 朝目标方向的直线穿透。
    Line {
        /// 穿透长度（格）。
        length: i32,
    },
}

/// Crystal 子类的蛇形邻域判定（`BoneSpearman.cs:14-25`、`VenomSpider.cs:14-25`）：
/// `x<=2 && y<=2 && ((x<=1&&y<=1)||x==y||x%2==y%2)`。
fn in_snake_neighborhood(from: Point, to: Point) -> bool {
    let x = (to.x - from.x).abs();
    let y = (to.y - from.y).abs();
    x <= 2 && y <= 2 && ((x <= 1 && y <= 1) || x == y || x % 2 == y % 2)
}

/// 怪物的物理特殊攻击（Crystal 子类的 AOE/直线 `ObjectAttack`，**不是** `MonsterMagic`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonsterSpecialSpec {
    /// 触发射程。
    pub range: i32,
    /// 伤害形态。
    pub shape: MonsterMagicShape,
    /// 伤害延迟（毫秒）。
    pub delay_ms: i64,
    /// 是否站桩。
    pub stationary: bool,
    /// 是否按 Crystal 的蛇形邻域判定（`x<=2 && y<=2 && ((x<=1&&y<=1)||x==y||x%2==y%2)`）。
    pub snake: bool,
    /// 防御类型（AC / MAC）。
    pub defence: DefenceType,
    /// 命中后的附加效果（毒/麻痹/减速；Crystal `PoisonTarget`）。
    pub effects: &'static [MonsterHitEffect],
}

/// 物理特殊攻击命中后的附加效果（Crystal `PoisonTarget` 调用）。
///
/// * `chance` 是「1/N 概率」：`Random.Next(chance) == 0`（`MonsterObject.cs:3527-3545`）；
/// * `duration` 是**跳数**（Crystal `Poison.Duration`，不是毫秒）；
/// * `tick_ms` 是两次跳的间隔（Crystal `TickSpeed`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonsterHitEffect {
    /// 毒类型（`Green`/`Paralysis`/`Slow`）。
    pub kind: PoisonType,
    /// 触发概率：1/N。
    pub chance: u32,
    /// 持续跳数。
    pub duration: u32,
    /// 每跳间隔（毫秒）。
    pub tick_ms: i64,
}

/// 一个怪物魔法模板：Crystal 各 AI 子类的 `Spell`/射程/形态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonsterMagicSpec {
    /// 技能编号（`S.ObjectMagic.spell`）。
    pub spell: Spell,
    /// 施法射程（格）。
    pub range: i32,
    /// 冷却（毫秒，Crystal `AttackSpeed`）。
    pub cooldown_ms: i64,
    /// 伤害形态。
    pub shape: MonsterMagicShape,
    /// 贴身时是否先拉开距离（`HumanWizard` 的 kite 行为）。
    pub kite: bool,
    /// 是否站桩（`CanMove => false`）。
    pub stationary: bool,
    /// 用 MC 还是 DC 作为基础攻击力（Crystal 各子类的 `Stat.MinMC/MaxMC` vs `MinDC/MaxDC`）。
    pub use_magic_stats: bool,
}

/// 怪物种类：Crystal `MonsterInfo` 里影响 AI/战斗的那部分。
///
/// 真实数值将来由 `mir-db` 从数据库读出后填进来；本结构不依赖数据库。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonsterKind {
    /// 显示名（Crystal `MonsterInfo.Name`）。
    pub name: String,
    /// 客户端图像编号（Crystal `MonsterInfo.Image`，对应 `S.ObjectMonster.Image`）。
    pub image: MonsterImage,
    /// AI 编号（Crystal `MonsterInfo.AI`），只用于下发给客户端，本模块不按它分支。
    pub ai: u8,
    /// 等级（影响玩家经验/命中，本里程碑尚未使用）。
    pub level: u16,
    /// 视野半径（Crystal `MonsterInfo.ViewRange`，默认 7）。
    pub view_range: i32,
    /// 破隐身概率（Crystal `MonsterInfo.CoolEye`，百分比）。
    pub cool_eye: u8,
    /// 攻击间隔（毫秒，Crystal `MonsterInfo.AttackSpeed`）。
    pub attack_speed_ms: i64,
    /// 移动间隔（毫秒，Crystal `MonsterInfo.MoveSpeed`）。
    pub move_speed_ms: i64,
    /// 击杀经验（Crystal `MonsterInfo.Experience`）。
    pub experience: u32,
    /// 是否能被推动（Crystal `MonsterInfo.CanPush`）。
    pub can_push: bool,
    /// 战斗属性（Crystal `MonsterInfo.Stats`）。
    pub stats: CombatStats,
}

impl MonsterKind {
    /// 是否是守卫类怪物（Crystal `MonsterObject.GetMonster` 的 `Guard` / `ArcherGuard`，
    /// `Server/MirObjects/MonsterObject.cs:30-31`、`:141-142`、`:261-262`）。
    ///
    /// AI 编号：`6` / `58` = `Guard`，`113` = `ArcherGuard`（后者的父类也是 `Guard`，
    /// `Server/MirObjects/Monsters/ArcherGuard.cs:6`）。
    ///
    /// 守卫的行为与其他怪不同：
    /// * `Guard.IsAttackTarget` 恒 false → 玩家打不到它（`Monsters/Guard.cs:54-55`）；
    /// * 只把**红名**玩家（`PKPoints >= 200`）当目标（`PlayerObject.IsAttackTarget(MonsterObject)`，
    ///   `PlayerObject.cs:4679`）；
    /// * 攻击距离是视野半径、防御类型是 `AC`（`Monsters/Guard.cs:40-45`、`:85`）；
    /// * 攻击动作广播在目标的**背后一格**（`Monsters/Guard.cs:70-74`）。
    #[must_use]
    pub const fn is_guard(&self) -> bool {
        matches!(self.ai, 6 | 58 | 113)
    }

    /// 远程攻击射程（Crystal 各 AI 子类覆写的 `InAttackRange`）：
    ///
    /// * `AI 8` `AxeSkeleton` / `AI 164` `HornedArcher` / `AI 175` `ChieftainArcher` /
    ///   `AI 199` `FurbolgArcher`：6（各自 `AttackRange`）；
    /// * `AI 57` `TownArcher` / `AI 80` `ConquestArcher`：10（`TownArcher.cs:10`）；
    /// * `AI 31` `RightGuard` / `AI 32` `LeftGuard`：8，且**相邻时改用近战**
    ///   （`RightGuard.cs:10-53` 的 `ranged = !InRange(...,1)`）；
    /// * `AI 16` `RedThunderZuma`：9，同样相邻改近战（`RedThunderZuma.cs:8-56`）；
    /// * `AI 113` `ArcherGuard`：射程 = `ViewRange`（`ArcherGuard.cs:22-44`，
    ///   `InAttackRange` 继承 `Guard`）。真实 DB 里 113 也被部分版本用作 `Armadillo`，
    ///   因此按名字再确认一次，避免把土狼当弓手。
    ///
    /// 其余 AI 返回 `None`（走基类近战距离 1）。法师类 AI（10/55/93…）的魔法弹/直线
    /// 技能需要 `MonsterMagic` 表与 `ObjectMagic` 分支，本轮不做，见 docs 差异。
    #[must_use]
    pub fn ranged_attack_range(&self) -> Option<i32> {
        match self.ai {
            8 | 164 | 175 | 199 => Some(6),
            16 => Some(9),
            31 | 32 => Some(8),
            57 | 80 => Some(10),
            113 if self.is_archer_guard() => Some(self.view_range),
            _ => None,
        }
    }

    /// `AI 113` 是否是 `ArcherGuard`（站桩远程守卫）。
    ///
    /// Crystal 参考表把 113 映射到 `ArcherGuard`（`MonsterObject.cs:261-262`）；
    /// 但部分社区 DB（如 Jev）把 113 用作 `Armadillo`，所以用名字兜底确认。
    #[must_use]
    pub fn is_archer_guard(&self) -> bool {
        self.ai == 113 && (self.name.contains("ArcherGuard") || self.name.contains("弓箭守卫"))
    }

    /// 远程 AI 在贴身时是否改用近战（Crystal `RightGuard/LeftGuard/RedThunderZuma`）。
    #[must_use]
    pub const fn melee_when_adjacent(&self) -> bool {
        matches!(self.ai, 16 | 31 | 32)
    }

    /// 怪物魔法模板：当前 Crystal 参考版本里唯一走 `S.ObjectMagic` 的 AI。
    ///
    /// * `AI 55` `HumanWizard`/`Clone`：`ThunderBolt`，单体，射程 6，首轮 5 秒
    ///   `FearTime` 走位后施法（`Monsters/HumanWizard.cs:25-120`）。
    ///
    /// 其余 `ObjectMagic` 子类（SepWizard/AI 215+ 等）在采样 DB 中不存在；
    /// `AI 13/29/14/50/100` 是物理 AOE/直线（见 [`Self::monster_special`]）。
    #[must_use]
    pub fn monster_magic(&self) -> Option<MonsterMagicSpec> {
        let cooldown = self.attack_speed_ms;
        match self.ai {
            55 => Some(MonsterMagicSpec {
                spell: Spell::ThunderBolt,
                range: 6,
                cooldown_ms: cooldown,
                shape: MonsterMagicShape::Single,
                kite: true,
                stationary: false,
                use_magic_stats: true,
            }),
            // 下列 AI 在 Crystal 里是 `S.ObjectMagic` 子类（`SepWizard.cs:68-84`、
            // `SepTaoist.cs:92-97`、`SepHighWizard.cs:58-79`）。**注意**：采样 `Server.MirDB`
            // （cjlaaa/Jev/Daneo/airen/masmx86 等全部 12 份）里唯一真实存在的 ObjectMagic 是
            // AI 55 `Clone`（已实现）；这三个编号是按 Crystal 参考类预留，供导入新版 DB 时直接生效。
            215 => Some(MonsterMagicSpec {
                spell: Spell::GreatFireBall,
                range: 5,
                cooldown_ms: cooldown,
                shape: MonsterMagicShape::Single,
                kite: false,
                stationary: false,
                use_magic_stats: true,
            }),
            216 => Some(MonsterMagicSpec {
                spell: Spell::SoulFireBall,
                range: 5,
                cooldown_ms: cooldown,
                shape: MonsterMagicShape::Single,
                kite: false,
                stationary: false,
                use_magic_stats: true,
            }),
            220 => Some(MonsterMagicSpec {
                spell: Spell::FireBang,
                range: 6,
                cooldown_ms: cooldown,
                shape: MonsterMagicShape::Aoe {
                    radius: 1,
                    at_target: true,
                },
                kite: false,
                stationary: false,
                use_magic_stats: true,
            }),
            _ => None,
        }
    }

    /// 物理特殊攻击（Crystal 子类里的 AOE/直线，走 `S.ObjectAttack`，不是魔法）：
    ///
    /// * `AI 13` `RedMoonEvil`：站桩，视野内全体 AOE（`Monsters/RedMoonEvil.cs:30-60`）；
    /// * `AI 29` `BoneSpearman`：直线 2 格（`Monsters/BoneSpearman.cs:14-45`）。
    ///
    /// * `AI 14` `EvilCentipede`：站桩，自身半径 7 AOE + 绿毒/麻痹
    ///   （`Monsters/EvilCentipede.cs:74-84`）；
    /// * `AI 50` `GreatFoxSpirit`：站桩，距离 >2 时半径 7 AOE、否则半径 2，附 Slow/Paralysis
    ///   （`Monsters/GreatFoxSpirit.cs:100-133`）；
    /// * `AI 100` `VenomSpider`：蛇形邻域，直线 2 格 + 绿毒
    ///   （`Monsters/VenomSpider.cs:14-47`）。
    #[must_use]
    pub fn monster_special(&self) -> Option<MonsterSpecialSpec> {
        match self.ai {
            13 => Some(MonsterSpecialSpec {
                range: self.view_range,
                shape: MonsterMagicShape::Aoe {
                    radius: self.view_range,
                    at_target: false,
                },
                delay_ms: 300,
                stationary: true,
                snake: false,
                defence: DefenceType::ACAgility,
                effects: &[],
            }),
            29 => Some(MonsterSpecialSpec {
                range: 2,
                shape: MonsterMagicShape::Line { length: 2 },
                delay_ms: 250,
                stationary: false,
                snake: true,
                defence: DefenceType::ACAgility,
                effects: &[],
            }),
            // `EvilCentipede.cs:74-84`：`FindAllTargets(7, CurrentLocation)` → 逐目标
            // `Attack()`（`Attacked(..., MAC)`），命中后绿毒（5/15/2000）+ 麻痹（15/5/2000）。
            14 => Some(MonsterSpecialSpec {
                range: 7,
                shape: MonsterMagicShape::Aoe {
                    radius: 7,
                    at_target: false,
                },
                delay_ms: 500,
                stationary: true,
                snake: false,
                defence: DefenceType::MAC,
                effects: &[
                    MonsterHitEffect {
                        kind: PoisonType::Green,
                        chance: 5,
                        duration: 15,
                        tick_ms: 2_000,
                    },
                    MonsterHitEffect {
                        kind: PoisonType::Paralysis,
                        chance: 15,
                        duration: 5,
                        tick_ms: 2_000,
                    },
                ],
            }),
            // `GreatFoxSpirit.cs:100-133`：射程 7；距离 >2 时半径 7 的 AOE（远程形态），
            // 否则半径 2；命中后 Slow（5/15/1000）+ Paralysis（5/5/1000）。
            // 远程形态 Crystal 还广播 `S.ObjectRangeAttack` + `S.ObjectEffect`，本作统一用
            // `S.ObjectAttack`（没有 GreatFoxSpirit 特效包），见 docs 差异。
            50 => Some(MonsterSpecialSpec {
                range: 7,
                shape: MonsterMagicShape::Aoe {
                    radius: 7,
                    at_target: false,
                },
                delay_ms: 300,
                stationary: true,
                snake: false,
                defence: DefenceType::MAC,
                effects: &[
                    MonsterHitEffect {
                        kind: PoisonType::Slow,
                        chance: 5,
                        duration: 15,
                        tick_ms: 1_000,
                    },
                    MonsterHitEffect {
                        kind: PoisonType::Paralysis,
                        chance: 5,
                        duration: 5,
                        tick_ms: 1_000,
                    },
                ],
            }),
            // `VenomSpider.cs:14-47`：蛇形邻域判定，`LineAttack(damage, 2, 300, MACAgility)`；
            // 命中后绿毒（8/5/1000）。
            100 => Some(MonsterSpecialSpec {
                range: 2,
                shape: MonsterMagicShape::Line { length: 2 },
                delay_ms: 300,
                stationary: false,
                snake: true,
                defence: DefenceType::MACAgility,
                effects: &[MonsterHitEffect {
                    kind: PoisonType::Green,
                    chance: 8,
                    duration: 5,
                    tick_ms: 1_000,
                }],
            }),
            _ => None,
        }
    }

    /// 是否是站桩型弓箭手（Crystal `TownArcher.CanMove` 要求 `Route.Count > 0`，
    /// 无路线表时不移动，`Monsters/TownArcher.cs:52-120`）。
    #[must_use]
    pub const fn is_town_archer(&self) -> bool {
        self.ai == 57
    }

    /// 是否是 Tao Guard（Crystal AI 58）。
    ///
    /// 与普通守卫（AI 6/113）不同：Tao Guard 会主动打宠物，**除非**宠物的主人处于
    /// 和平模式（`MonsterObject.cs:2476-2479`）。
    pub const fn is_tao_guard(&self) -> bool {
        self.ai == 58
    }

    /// 是否是攻城箭塔（Crystal `ConquestArcher`，AI 80）。
    ///
    /// `Server/MirObjects/MonsterObject.cs:189-190` 把 AI 80 映射到 `ConquestArcher`；
    /// 它是 `TownArcher` 的子类（`Server/MirObjects/Monsters/ConquestArcher.cs:6`），
    /// 只在攻城期间射击**非城主行会**的玩家：
    ///
    /// * `FindTarget` 跳过「本行会已经拥有这座城」的玩家、跳过隐身、要求 `WarIsOn`
    ///   （`ConquestArcher.cs:53`）；
    /// * 攻击距离是 `TownArcher.AttackRange = 10`（`TownArcher.cs:10`）；
    /// * 站桩不动（`CanMove` 要求 `Route.Count > 0`，`ConquestArcher.cs:11-14`）。
    ///
    /// 简化：Crystal 走 `S.ObjectRangeAttack` + `ProjectileAttack`（`TownArcher.cs:39`、`:47`），
    /// 本作没有远程抛射物协议（`ObjectRangeAttack` 未移植），箭塔按普通近战广播结算，
    /// 伤害仍然按「同图 + 视野半径内」判定，见 `docs/world/conquest.md`。
    #[must_use]
    pub const fn is_conquest_archer(&self) -> bool {
        self.ai == 80
    }

    /// 用默认值构造一个种类（视野 7、攻击 2500ms、移动 1800ms）。
    #[must_use]
    pub fn new(name: impl Into<String>, image: MonsterImage) -> Self {
        Self {
            name: name.into(),
            image,
            ai: 0,
            level: 1,
            view_range: DEFAULT_VIEW_RANGE,
            cool_eye: 0,
            attack_speed_ms: DEFAULT_ATTACK_SPEED_MS,
            move_speed_ms: DEFAULT_MOVE_SPEED_MS,
            experience: 0,
            can_push: true,
            stats: CombatStats::default(),
        }
    }

    /// 示例怪物：骷髅。
    ///
    /// 简化：**占位数值**。Crystal 的怪物属性全部来自数据库（`MonsterInfo`），
    /// `mir-db` 尚未提供数据，这里给一组可玩的手感值，接入数据库后整条 `MonsterKind` 会被替换。
    #[must_use]
    pub fn skeleton() -> Self {
        Self {
            name: "骷髅".to_owned(),
            image: MonsterImage::Skeleton,
            ai: 5,
            level: 5,
            experience: 20,
            stats: CombatStats {
                hp: 25,
                min_dc: 1,
                max_dc: 4,
                max_ac: 1,
                accuracy: 5,
                agility: 3,
                ..CombatStats::default()
            },
            ..Self::new("骷髅", MonsterImage::Skeleton)
        }
    }

    /// 示例怪物：稻草人（更弱、更慢，适合当第一只被试手的怪）。
    ///
    /// 简化：**占位数值**，理由同 [`MonsterKind::skeleton`]。
    #[must_use]
    pub fn scarecrow() -> Self {
        Self {
            name: "稻草人".to_owned(),
            image: MonsterImage::Scarecrow,
            ai: 0,
            level: 1,
            experience: 5,
            move_speed_ms: 2_500,
            attack_speed_ms: 3_000,
            stats: CombatStats {
                hp: 10,
                min_dc: 0,
                max_dc: 2,
                accuracy: 3,
                agility: 0,
                ..CombatStats::default()
            },
            ..Self::new("稻草人", MonsterImage::Scarecrow)
        }
    }

    /// 召唤骷髅（`SummonSkeleton`）在数据库缺模板时的兜底种类。
    ///
    /// Crystal 从 `Settings.SkeletonName = "BoneFamiliar"` 的 `MonsterInfo` 取
    /// （`Settings.cs:162`、`HumanObject.cs:4422`）；本作优先查库，查不到才用这组占位数值。
    #[must_use]
    pub fn bone_familiar() -> Self {
        Self {
            name: "骷髅".to_owned(),
            image: MonsterImage::BoneFamiliar,
            ai: 5,
            level: 8,
            experience: 0,
            stats: CombatStats {
                hp: 45,
                min_dc: 2,
                max_dc: 6,
                min_ac: 1,
                max_ac: 2,
                accuracy: 6,
                agility: 3,
                ..CombatStats::default()
            },
            ..Self::new("骷髅", MonsterImage::BoneFamiliar)
        }
    }

    /// 召唤神兽（`SummonShinsu`）在数据库缺模板时的兜底种类。
    ///
    /// Crystal 从 `Settings.ShinsuName = "Shinsu"` 的 `MonsterInfo` 取（`Settings.cs:163`、
    /// `HumanObject.cs:4471`）；本作优先查库，查不到才用这组占位数值。
    #[must_use]
    pub fn shinsu() -> Self {
        Self {
            name: "神兽".to_owned(),
            image: MonsterImage::Shinsu,
            ai: 5,
            level: 12,
            experience: 0,
            stats: CombatStats {
                hp: 80,
                min_dc: 3,
                max_dc: 8,
                min_ac: 2,
                max_ac: 3,
                min_mac: 2,
                max_mac: 3,
                accuracy: 8,
                agility: 4,
                ..CombatStats::default()
            },
            ..Self::new("神兽", MonsterImage::Shinsu)
        }
    }
}

/// 一只有状态怪物。
#[derive(Debug, Clone)]
pub struct Monster {
    /// 对象 ID（与玩家共用同一个 ID 空间）。
    pub object_id: u32,
    /// 种类（多个实例共享同一份定义）。
    pub kind: Arc<MonsterKind>,
    /// 地图名。
    pub map: String,
    /// 坐标。
    pub position: Point,
    /// 出生点（守卫的「原位」）。
    ///
    /// Crystal 的守卫靠 `RespawnInfo.RoutePath` 巡逻并走回路线点（`Guard.CanMove` 要求
    /// `Route.Count > 0`，`Monsters/Guard.cs:14-17`、`MonsterObject.ProcessRoute`，
    /// `MonsterObject.cs:1999-2025`）。本作没有路线表，用出生点近似「唯一路线点」：
    /// 守卫没有目标且不在出生点时走回去，见 `World::process_guard`。
    pub home: Point,
    /// 朝向。
    pub direction: MirDirection,
    /// 当前血量（Crystal `MonsterObject.HP`）。
    pub hp: i32,
    /// 战斗属性（Crystal `RefreshBase` 之后的 `Stats`）。
    pub stats: CombatStats,
    /// 是否破隐身（Crystal `CoolEye`，构造时抽签决定，`MonsterObject.cs:674`）。
    pub cool_eye: bool,
    /// 是否已死亡（尸体仍在世界里，`ObjectDied` 已广播）。
    pub dead: bool,
    /// 尸体移除时刻（Crystal `DeadTime`）。
    pub dead_time: i64,
    /// 动作锁定截止（Crystal `ActionTime`）。
    pub action_time: i64,
    /// 移动冷却截止（Crystal `MoveTime`）。
    pub move_time: i64,
    /// 攻击冷却截止（Crystal `AttackTime`）。
    pub attack_time: i64,
    /// `HumanWizard` 的 `FearTime`：进入射程后的首轮 5 秒走位窗口。
    pub fear_time: i64,
    /// 下次搜索目标的时刻（Crystal `SearchTime`）。
    pub search_time: i64,
    /// 下次漫游的时刻（Crystal `RoamTime`）。
    pub roam_time: i64,
    /// 下次回血的时刻（Crystal `RegenTime`）。
    pub regen_time: i64,
    /// 刚移动过的宽限截止（Crystal `CellTime`）。
    pub cell_time: i64,
    /// 下次允许被 `Process()` 的时刻（Crystal `OperateTime`）。
    pub operate_time: i64,
    /// 当前目标（只可能是玩家）。
    pub target: Option<u32>,
    /// 首个攻击者的玩家 ID（Crystal `EXPOwner`，用于经验/血量包）。
    pub expo_owner: Option<u32>,
    /// 所属刷怪组在 [`crate::world::World`] 里的下标（Crystal `MonsterObject.Respawn`，
    /// `Server/MirObjects/MonsterObject.cs:504`）；手工刷出来的怪是 `None`。
    pub respawn_group: Option<usize>,
    /// 附近有没有玩家（Crystal `Alone`：没有玩家时整只怪跳过 AI）。
    pub alone: bool,
    /// 下次检查 `alone` 的时刻（Crystal `AloneTime`）。
    pub alone_time: i64,
    /// 所属攻城区（Crystal `ConquestArcher.Conquest`，`ConquestArcher.cs:8`）。
    ///
    /// `None` = 普通怪物。由 [`World::spawn_conquest_archer`] 设置；
    /// 它决定箭塔在开战时只射非城主行会的玩家。
    pub conquest: Option<i32>,
    /// 召唤物的主人（Crystal `MonsterObject.Master`，`Server/MirObjects/MonsterObject.cs:608`）。
    ///
    /// `None` = 野生怪物。主人的对象 ID 一定是玩家：召唤物跟随主人、攻击主人的目标、
    /// 主人下线/死亡时消失（见 `docs/world/magic.md` 的召唤物一节）。
    pub master: Option<u32>,
    /// 召唤物等级（Crystal `MonsterObject.PetLevel`，`MonsterObject.cs:609`）。
    ///
    /// 取施法时的技能等级；召唤物属性按这个值加成（`RefreshAll`，`:815-821`）。
    pub pet_level: u8,
    /// 召唤物能升到的最高等级（Crystal `MonsterObject.MaxPetLevel`，`MonsterObject.cs:611`）。
    ///
    /// 只用来算移动/攻击间隔（`RefreshAll`，`:825-826`）：骷髅 `4 + Level`、
    /// 神兽 `1 + Level * 2`（`HumanObject.cs:4431`、`:4480`）。
    pub max_pet_level: u8,
    /// 召唤物的当前目标（Crystal `MonsterObject.Target` 同时可以指向玩家或怪物）。
    ///
    /// 野生怪的 [`Monster::target`] 只可能是玩家，所以另开一个字段给召唤物用：
    /// 它可以是玩家的目标（`master.magic.last_target`），也可以是附近的野生怪。
    pub pet_target: Option<u32>,
    /// 召唤这只宠物的技能（`SummonSkeleton` / `SummonShinsu`）。
    ///
    /// 用来做「同名活宠只召回、不重复召唤」的判定（Crystal 比的是
    /// `Settings.SkeletonName`/`ShinsuName`，`HumanObject.cs:4408-4415`、`:4457-4464`）。
    pub summon_spell: Option<Spell>,
    /// 中毒状态（Crystal `MapObject.PoisonList`，`Server/MirObjects/MapObject.cs:74`）。
    ///
    /// **与 buffs 分开**：Crystal 的毒是独立列表，有自己的 tick / 时长 / 施毒者，
    /// 不参与 `Stats` 加成（红毒只改 `ArmourRate`，绿毒每 tick 直接扣血）。
    pub poisons: Vec<crate::magic::PoisonState>,
    /// 是否不死系（Crystal `MapObject.Undead`，`MapObject.cs:69`）。
    ///
    /// 由 `MonsterInfo.Undead` 推导（`World::spawn_monster` 查 `GameData`），
    /// 只影响 `ThunderBolt` 的 ×1.5 伤害（`HumanObject.cs:4126`）。
    pub undead: bool,
    /// 本实例的移动间隔（Crystal `MonsterObject.MoveSpeed`；默认取模板值，
    /// 召唤物按 `MaxPetLevel` 缩短，`MonsterObject.cs:825`）。
    pub move_speed_ms: i64,
    /// 本实例的攻击间隔（Crystal `MonsterObject.AttackSpeed`；召唤物加快，
    /// `MonsterObject.cs:826`）。
    pub attack_speed_ms: i64,
}

impl Monster {
    /// 创建并「出生」一只怪物。
    ///
    /// 抽签顺序与 Crystal 构造函数 + `Spawned()` 完全一致（同一 seed 才能回放）：
    /// `CoolEye` → `Direction` → `RegenTime` → `SearchTime` → `RoamTime` → `OperateTime`。
    pub fn new(
        object_id: u32,
        kind: Arc<MonsterKind>,
        map: impl Into<String>,
        position: Point,
        now: i64,
        rng: &mut MirRng,
    ) -> Self {
        // `MonsterObject.cs:674`：`CoolEye = info.CoolEye > Envir.Random.Next(100)`。
        let cool_eye = i32::from(kind.cool_eye) > rng.next_range(0, 99);
        // `MonsterObject.cs:675`：`Direction = (MirDirection)Envir.Random.Next(8)`。
        let direction = MirDirection::ALL[rng.next_bounded(8) as usize];
        // `MonsterObject.cs:680-682`：三个计时器先各抽一次，再落在 now 之后。
        let regen_time = now + i64::from(rng.next_bounded(REGEN_DELAY_MS as u32));
        let search_time = now + i64::from(rng.next_bounded(SEARCH_DELAY_MS as u32));
        let roam_time = now + i64::from(rng.next_bounded(ROAM_DELAY_MS as u32));
        // `MapObject.cs:351`：`OperateTime = Envir.Time + Envir.Random.Next(OperateDelay)`。
        let operate_time = now + i64::from(rng.next_bounded(MONSTER_OPERATE_JITTER_MS));
        let move_speed_ms = kind.move_speed_ms;
        let attack_speed_ms = kind.attack_speed_ms;

        Self {
            object_id,
            hp: kind.stats.hp,
            stats: kind.stats,
            kind,
            map: map.into(),
            position,
            home: position,
            direction,
            cool_eye,
            dead: false,
            dead_time: 0,
            // `MonsterObject.cs:736`：出生后 2 秒才开始行动。
            action_time: now + MONSTER_SPAWN_ACTION_MS,
            move_time: 0,
            attack_time: 0,
            fear_time: 0,
            search_time,
            roam_time,
            regen_time,
            cell_time: 0,
            operate_time,
            target: None,
            expo_owner: None,
            respawn_group: None,
            alone: false,
            alone_time: 0,
            conquest: None,
            master: None,
            pet_level: 0,
            max_pet_level: 0,
            pet_target: None,
            summon_spell: None,
            poisons: Vec::new(),
            undead: false,
            move_speed_ms,
            attack_speed_ms,
        }
    }

    /// 怪物是否还能行动：Crystal `MonsterObject.CanMove`（`MonsterObject.cs:634-649`）去掉毒/宠物条件。
    #[must_use]
    pub fn can_move(&self, now: i64) -> bool {
        !self.dead && now > self.move_time && now > self.action_time
    }

    /// 怪物是否能攻击：Crystal `MonsterObject.CanAttack`（`MonsterObject.cs:650-665`）去掉毒/宠物条件。
    #[must_use]
    pub fn can_attack(&self, now: i64) -> bool {
        !self.dead && now > self.attack_time && now > self.action_time
    }

    /// 血量百分比（Crystal `MonsterObject.HealthPercent`，`MonsterObject.cs:559-565`）。
    #[must_use]
    pub fn health_percent(&self) -> u8 {
        if self.stats.hp <= 0 {
            return 0;
        }
        let percent = self.hp * 100 / self.stats.hp;
        u8::try_from(percent.clamp(0, 100)).unwrap_or(0)
    }

    /// 是否是召唤物（Crystal `Master != null`）。
    #[must_use]
    pub const fn is_pet(&self) -> bool {
        self.master.is_some()
    }

    /// 红毒造成的防御倍率（Crystal `ArmourRate`，`MonsterObject.cs:1536-1537`）。
    ///
    /// Crystal 每中一次红毒 `ArmourRate -= 0.5F`，结算时 `armour * ArmourRate`
    /// （`MonsterObject.cs:2581`）。本作把倍率夹到 `0.0..=1.0`，避免负防御。
    #[must_use]
    pub fn armour_rate(&self) -> f32 {
        let mut rate = 1.0_f32;
        for poison in &self.poisons {
            if poison.kind == mir_core::enums::PoisonType::Red {
                rate -= 0.5;
            }
        }
        rate.clamp(0.0, 1.0)
    }

    /// 当前中毒类型（Crystal `MapObject.CurrentPoison`，用于 `S.ObjectMonster.Poison`）。
    #[must_use]
    pub fn poison_type(&self) -> mir_core::enums::PoisonType {
        let mut kind = mir_core::enums::PoisonType::None;
        for poison in &self.poisons {
            kind |= poison.kind;
        }
        kind
    }

    /// 下一次中毒结算时刻（`SetOperateTime` 要把毒 tick 算进调度，
    /// `MonsterObject.cs:1232-1310`）。
    #[must_use]
    pub fn next_poison_tick(&self) -> Option<i64> {
        self.poisons.iter().map(|poison| poison.tick_at).min()
    }
}

/// Crystal `Envir` 的对象调度 + `MonsterObject.Process`。
///
/// Crystal 把所有对象（玩家、怪物、物品、法术……）放在**同一个** `Envir.Objects` 链表里，
/// 按插入顺序（= 创建顺序）轮转，每个对象用 `OperateTime` 决定这一轮要不要 `Process()`
/// （`Server/MirEnvir/Envir.cs:2119-2138`）。mir-next 按 [`crate::world::World::tick`] 的
/// 文档化顺序：先玩家簿记，再怪物（`monsters` 用 `Vec` 保持生成顺序，等价于链表的插入顺序）。
impl World {
    /// 推进所有怪物：先清理到期的尸体，再按生成顺序跑 AI。
    pub(crate) fn tick_monsters(&mut self, now: i64) {
        const AI_BATCHES: u32 = 8;
        self.remove_expired_corpses(now);
        // 空闲短路：没有任何在线玩家时，所有非召唤怪都不需要索敌/AI。
        // Crystal 的 `Settings.MonsterProcessWhenAlone` 默认为 false，怪物在附近没有玩家时
        // 本来就不思考（`MonsterObject.cs` 的 `CheckAlone`）；这里把整张世界直接短路，
        // 避免每 tick 遍历 1900+ 只怪。
        if self.players.is_empty() && self.monsters.len() > 512 {
            return;
        }
        // 大地图（真实 MirDB 约 1900+ 只怪）按 object_id 分 4 批，每 tick 只跑一批；
        // 有目标/召唤物的怪仍然每 tick 处理，保证攻击节奏不变。小世界（测试/空世界）
        // 保持全量，避免把测试里的 AI 时序摊到 4 个 tick。
        let batch = u32::try_from((now / 100).rem_euclid(i64::from(AI_BATCHES))).unwrap_or(0);
        let batched = self.monsters.len() > 512;
        // 大世界预计算「玩家附近 16×16 大格」集合：怪物 AI 只处理玩家附近区域，
        // 避免 1900+ 只怪每 tick 做 AOI 索敌（Crystal `CheckAlone` 的口径）。
        let mut player_cells = std::collections::HashSet::new();
        if batched {
            for player in self.players.values() {
                if player.dead {
                    continue;
                }
                let cx = player.position.x.div_euclid(16);
                let cy = player.position.y.div_euclid(16);
                for dx in -1..=1 {
                    for dy in -1..=1 {
                        player_cells.insert((player.map.clone(), cx + dx, cy + dy));
                    }
                }
            }
        }
        if batched {
            self.broadcast_batch = Some(std::collections::HashMap::new());
        }

        for index in 0..self.monsters.len() {
            if self.monsters[index].dead {
                // Crystal 的死亡怪物仍会 `Process()`（等 `DeadTime`），但不再跑 AI。
                continue;
            }
            // `Envir.cs:2130`：`if (Time > current.Value.OperateTime)`（严格大于）。
            if now <= self.monsters[index].operate_time {
                continue;
            }
            // 只摊薄「无目标、非召唤物」的野生怪；有 AI 交互的怪保持每 tick 处理。
            let idle = self.monsters[index].target.is_none()
                && self.monsters[index].master.is_none()
                && !self.monsters[index].kind.is_guard();
            if batched && idle {
                let cell = (
                    self.monsters[index].map.clone(),
                    self.monsters[index].position.x.div_euclid(16),
                    self.monsters[index].position.y.div_euclid(16),
                );
                if !player_cells.contains(&cell) {
                    continue;
                }
            }
            if batched && idle && self.monsters[index].object_id % AI_BATCHES != batch {
                continue;
            }
            // 克隆一份出来处理，避免 `&mut Monster` 与 `&mut World`（广播、随机数、待结算队列）
            // 同时借用；世界是单线程的，处理期间没有其它代码会看到这份旧副本。
            let mut monster = self.monsters[index].clone();
            self.process_monster(&mut monster, now);
            let object_id = monster.object_id;
            self.monsters[index] = monster;
            // 毒（或其它不经过 `damage_monster` 的掉血）可能把血打到 0：在**回写之后**
            // 统一结算死亡，避免死亡状态被循环末尾的 `self.monsters[index] = monster` 覆盖。
            if self.monsters[index].hp <= 0 && !self.monsters[index].dead {
                self.monster_died(object_id);
            }
        }
        if batched {
            self.flush_broadcast_batch();
        }
    }

    /// 移除 `dead_time` 已到的尸体：Crystal `MonsterObject.Process`（`MonsterObject.cs:1205-1216`）。
    fn remove_expired_corpses(&mut self, now: i64) {
        let mut index = 0;
        while index < self.monsters.len() {
            if self.monsters[index].dead && now >= self.monsters[index].dead_time {
                let monster_id = self.monsters[index].object_id;
                // `forget_monster` 同时把怪物从 AOI 网格摘掉。
                self.forget_monster(monster_id);
                self.unregister_pet(monster_id);
                self.monsters.remove(index);
                self.rebuild_monster_slots();
                continue;
            }
            index += 1;
        }
    }

    /// 单只怪物的一次 `Process()`：目标失效清理 → AI → 回血 → 重算调度时刻。
    fn process_monster(&mut self, monster: &mut Monster, now: i64) {
        // `MonsterObject.cs:1198-1199`：目标跑出视野/死亡/换图就丢弃。
        // 简化：Crystal 用的是 `Globals.DataRange`（16），mir-next 用广播半径 `VIEW_RANGE`（10），
        // 因此「目标跑远后脱战」的距离比 Crystal 近 6 格；两者都远大于索敌视野（7），
        // 不影响「先发现、再追、再脱战」的先后顺序。
        if let Some(target_id) = monster.target {
            let still_valid = if self.players.contains_key(&target_id) {
                self.players.get(&target_id).is_some_and(|player| {
                    !player.dead
                        && player.map == monster.map
                        && monster.position.in_range(player.position, VIEW_RANGE)
                        // 隐身玩家不再被锁定（`MonsterObject.cs:1974`；破隐（CoolEye）
                        // 且等级不低于对方时仍可锁定）。
                        && !hidden_from(&player.magic, monster, player.level)
                        // 守卫只对红名动手（`PlayerObject.IsAttackTarget(MonsterObject)`，
                        // `PlayerObject.cs:4679`）：对方洗白后立即脱战。
                        && (!monster.kind.is_guard() || player.pk_points >= RED_NAME_PK_POINTS)
                })
            } else {
                // 怪物目标只可能是玩家召唤物（Crystal `FindTarget` 的 Monster 分支，
                // `MonsterObject.cs:1940-1963`；野生怪之间不会互殴）。
                self.monster(target_id).is_some_and(|target| {
                    self.can_target_pet(monster, target)
                        && !target.dead
                        && target.map == monster.map
                        && monster.position.in_range(target.position, VIEW_RANGE)
                })
            };
            if !still_valid {
                monster.target = None;
            }
        }

        self.process_ai(monster, now);
        self.process_monster_regen(monster, now);
        // Crystal `MonsterObject.Process`：`ProcessAI → ProcessBuffs → ProcessRegen → ProcessPoison`
        // （`MonsterObject.cs:1225-1229`）。
        self.tick_monster_poison(monster, now);
        monster.operate_time = Self::monster_operate_time(monster, now);
    }

    /// Crystal `MonsterObject.ProcessAI`（`MonsterObject.cs:1671-1733`）。
    fn process_ai(&mut self, monster: &mut Monster, now: i64) {
        // 召唤物（`Master != null`）：跟随主人、攻击主人的目标（`MonsterObject.cs:1704-1721`、
        // `:1821-1825`）。走路逻辑与野生怪共用，所以单独分支，不参与 `CheckAlone` 的省电。
        if monster.master.is_some() {
            monster.alone = false;
            self.process_pet_ai(monster, now);
            return;
        }
        // 守卫：`Guard.CheckAlone` 在 `Route.Count > 0` 时恒为 false
        // （`MonsterObject.cs:1741-1745`、`Monsters/Guard.cs:14-17`），所以不走「附近没人就不思考」
        // 的省电分支，也不随机漫游。
        if monster.kind.is_guard() {
            monster.alone = false;
            self.process_search(monster, now);
            // `ArcherGuard.CanMove => false`（`ArcherGuard.cs:12-16`）：站桩，不回巡逻线。
            if !monster.kind.is_archer_guard() {
                self.process_guard_return(monster, now);
            }
            self.process_target(monster, now);
            return;
        }
        // 站桩魔法怪（GreatFoxSpirit/RedMoonEvil/EvilCentipede）：只索敌 + 施法，不漫游。
        if monster
            .kind
            .monster_magic()
            .is_some_and(|spec| spec.stationary)
        {
            monster.alone = false;
            self.process_search(monster, now);
            self.process_target(monster, now);
            return;
        }
        // 攻城箭塔/城弓箭手：`CanMove` 要求 `Route.Count > 0`
        // （`ConquestArcher.cs:11-14`、`TownArcher.cs:52-120`），本作没有路线表 →
        // 站桩：只索敌 + 攻击，不漫游、不追击。
        if monster.kind.is_conquest_archer() || monster.kind.is_town_archer() {
            monster.alone = false;
            self.process_search(monster, now);
            self.process_target(monster, now);
            return;
        }
        self.check_alone(monster, now);
        // `Settings.MonsterProcessWhenAlone` 默认 false（`Server/Settings.cs:51`）：
        // 附近（`DataRange * 2`）没有玩家时，怪物完全不思考。
        if monster.alone {
            return;
        }
        self.process_stacking(monster, now);
        self.process_search(monster, now);
        // AI 2 `Deer/Sheep` 的低血逃跑：Crystal `Deer.ProcessTarget` 在 `_runAway`
        // 时远离目标（`Monsters/Deer.cs:36-55`）；本作以 HP < 30% 作为触发阈值。
        if monster.kind.ai == 2
            && monster.target.is_some()
            && monster.hp.saturating_mul(100) <= monster.stats.hp.saturating_mul(30)
        {
            self.process_flee(monster, now);
            return;
        }
        self.process_roam(monster, now);
        self.process_target(monster, now);
        self.process_return_home(monster, now);
    }

    /// 低血逃跑：Crystal `Deer.ProcessTarget` 的 `_runAway`（`Monsters/Deer.cs:36-55`）。
    fn process_flee(&mut self, monster: &mut Monster, now: i64) {
        let Some(target_id) = monster.target else {
            return;
        };
        let Some(target_position) = self.players.get(&target_id).map(|player| player.position)
        else {
            return;
        };
        let mut direction = target_position.direction_to(monster.position);
        if self.monster_walk(monster, direction, now) {
            return;
        }
        // 正向被挡：按 Crystal 的 NextDir/PreviousDir 各试 7 个方向。
        for _ in 0..7 {
            direction = direction.next();
            if self.monster_walk(monster, direction, now) {
                return;
            }
        }
        direction = target_position.direction_to(monster.position);
        for _ in 0..7 {
            direction = direction.previous();
            if self.monster_walk(monster, direction, now) {
                return;
            }
        }
    }

    /// 目标丢失后的「回位」：Crystal 基类只随机漫游，守卫沿 `Route` 回出生点
    /// （`MonsterObject.ProcessRoam`，`:1815-1825`）。本作没有路线表，用出生点
    /// [`Monster::home`] 兜底：只有追出视野半径外才走回，避免影响正常漫游。
    fn process_return_home(&mut self, monster: &mut Monster, now: i64) {
        if monster.target.is_some() || monster.position == monster.home {
            return;
        }
        if monster.position.max_distance(monster.home) > monster.kind.view_range {
            self.monster_move_to(monster, monster.home, now);
        }
    }

    /// Crystal `MonsterObject.CheckAlone`（`MonsterObject.cs:1735-1763`）。
    ///
    /// 简化：Crystal 的判断半径是 `Globals.DataRange * 2`（32 格），mir-next 用 `VIEW_RANGE * 2`（20 格）；
    /// 只影响「离得多远算没人」的省电优化，不影响战斗。
    fn check_alone(&mut self, monster: &mut Monster, now: i64) {
        if now < monster.alone_time {
            return;
        }
        monster.alone_time = now + ALONE_DELAY_MS;
        monster.alone = self
            .aoi_ids_in_radius(
                &monster.map,
                monster.position,
                VIEW_RANGE * 2,
                AoiKind::Player,
            )
            .iter()
            .all(|player_id| {
                // Crystal 只看「这张图上有没有活着的玩家」（`MonsterObject.cs:1735-1763`）。
                self.players
                    .get(player_id)
                    .is_none_or(|player| player.dead || player.map != monster.map)
            });
    }

    /// Crystal `MonsterObject.ProcessStacking`（`MonsterObject.cs:1765-1802`）。
    ///
    /// 与别的对象踩在同一格时，先朝当前方向走；走不动就按 `Random.Next(3)` 顺时针或逆时针转着找空格。
    fn process_stacking(&mut self, monster: &mut Monster, now: i64) {
        if !monster.can_move(now) || !self.monster_cell_stacked(monster) {
            return;
        }
        let direction = monster.direction;
        if self.monster_walk(monster, direction, now) {
            return;
        }
        if self.rng.next_bounded(3) == 0 {
            let mut dir = direction;
            for _ in 0..7 {
                dir = dir.next();
                if self.monster_walk(monster, dir, now) {
                    return;
                }
            }
        } else {
            let mut dir = direction;
            for _ in 0..7 {
                dir = dir.previous();
                if self.monster_walk(monster, dir, now) {
                    return;
                }
            }
        }
    }

    /// Crystal `MonsterObject.ProcessSearch`（`MonsterObject.cs:1804-1813`）。
    fn process_search(&mut self, monster: &mut Monster, now: i64) {
        if now < monster.search_time {
            return;
        }
        monster.search_time = now + SEARCH_DELAY_MS;
        // `Target == null || Envir.Random.Next(3) == 0`：短路求值，已有目标时才抽签。
        if monster.target.is_none() || self.rng.next_bounded(3) == 0 {
            self.find_target(monster);
        }
    }

    /// Crystal `MonsterObject.ProcessRoam`（`MonsterObject.cs:1815-1840`）。
    fn process_roam(&mut self, monster: &mut Monster, now: i64) {
        if monster.target.is_some() || now < monster.roam_time {
            return;
        }
        monster.roam_time = now + ROAM_DELAY_MS;
        // `if (Envir.Random.Next(10) != 0) return;`：10% 的漫游尝试才真的动。
        if self.rng.next_bounded(10) != 0 {
            return;
        }
        // `switch (Envir.Random.Next(3))`：1/3 原地转身，2/3 朝当前方向走一步。
        if self.rng.next_bounded(3) == 0 {
            let direction = MirDirection::ALL[self.rng.next_bounded(8) as usize];
            self.monster_turn(monster, direction, now);
        } else {
            let direction = monster.direction;
            self.monster_walk(monster, direction, now);
        }
    }

    /// 守卫没有目标时走回出生点。
    ///
    /// Crystal 的守卫沿 `Route` 巡逻（`Guard.CanMove` 要求 `Route.Count > 0`，
    /// `Monsters/Guard.cs:14-17`；`MonsterObject.ProcessRoute`，`MonsterObject.cs:1999-2025`）。
    /// 本作没有路线表，把出生点当作只有一站的路線：[`Monster::home`]。
    fn process_guard_return(&mut self, monster: &mut Monster, now: i64) {
        if monster.target.is_some() || monster.position == monster.home {
            return;
        }
        let home = monster.home;
        self.monster_move_to(monster, home, now);
    }

    /// Crystal `MonsterObject.ProcessTarget`（`MonsterObject.cs:1842-1870`）。
    ///
    /// 目标既可以是玩家（野生怪的常规目标），也可以是玩家的召唤物
    /// （`FindTarget` 的 `case ObjectType.Monster`，`MonsterObject.cs:1940-1963`）。
    fn process_target(&mut self, monster: &mut Monster, now: i64) {
        let Some(target_id) = monster.target else {
            return;
        };
        if !monster.can_attack(now) {
            return;
        }
        let (target_position, same_map) = if let Some(target) = self.players.get(&target_id) {
            (target.position, target.map == monster.map)
        } else if let Some(target) = self.monster(target_id) {
            if target.master.is_none() {
                monster.target = None;
                return;
            }
            (target.position, target.map == monster.map)
        } else {
            monster.target = None;
            return;
        };

        let ranged_range = monster.kind.ranged_attack_range();
        // AI 12 `BugBatMaggot`：攻击动作是召唤 BugBat（`BugBagMaggot.cs:30-48`），
        // 不做普通近战；上限/冷却由 `World::process_bugbat_summon` 控制。
        if monster.kind.ai == 12 {
            self.process_bugbat_summon(monster.object_id, target_id, now);
            return;
        }
        if let Some(spec) = monster.kind.monster_magic() {
            self.process_magic_target(monster, target_id, target_position, same_map, spec, now);
            return;
        }
        if let Some(spec) = monster.kind.monster_special() {
            self.process_special_target(monster, target_id, target_position, same_map, spec, now);
            return;
        }
        if monster.kind.is_guard() || monster.kind.is_conquest_archer() {
            // `Guard.InAttackRange`：同图、不同格、切比雪夫距离 <= 视野半径
            // （`Monsters/Guard.cs:40-45`）。攻城箭塔/TownArcher 用 `AttackRange = 10`
            // （`Monsters/TownArcher.cs:10`），走 `S.ObjectRangeAttack`。
            let range = ranged_range.unwrap_or(monster.kind.view_range);
            if same_map
                && target_position != monster.position
                && monster.position.in_range(target_position, range)
            {
                if ranged_range.is_some() {
                    if !self.ranged_attack(monster, target_id, now) {
                        monster.target = None;
                    }
                } else if self.players.contains_key(&target_id) {
                    self.guard_attack(monster, target_id, now);
                } else if !self.attack_monster(monster, target_id, now) {
                    monster.target = None;
                }
            }
            return;
        }

        let attack_range = ranged_range.unwrap_or(1);
        let adjacent = monster.position.in_range(target_position, 1);
        if same_map
            && target_position != monster.position
            && monster.position.in_range(target_position, attack_range)
        {
            let use_ranged =
                ranged_range.is_some() && !(adjacent && monster.kind.melee_when_adjacent());
            let attacked = if use_ranged {
                self.ranged_attack(monster, target_id, now)
            } else if self.players.contains_key(&target_id) {
                self.monster_attack(monster, target_id, now);
                true
            } else {
                self.attack_monster(monster, target_id, now)
            };
            if !attacked {
                monster.target = None;
            }
            return;
        }
        // TownArcher 无路线表 → 站桩；够不到也不追。
        if monster.kind.is_town_archer() {
            return;
        }
        self.monster_move_to(monster, target_position, now);
    }

    /// 召唤物 AI：跟随主人、攻击主人的目标（Crystal `MonsterObject.ProcessAI` 的
    /// `Master != null` 分支，`MonsterObject.cs:1704-1721`；跟随 `:1821-1825`；
    /// 目标选择与「优先打主人的目标」见 `:1974-1988`）。
    ///
    /// 与野生怪的区别：
    /// * 不做 `CheckAlone` 省电（主人就在附近，且召唤物必须持续跟随）；
    /// * 主人跨图/超出 `Globals.DataRange`（本作用 `VIEW_RANGE`）时召回
    ///   （`PetRecall`，`MonsterObject.cs:1334-1375`、`:1713-1714`）；
    /// * 没有目标时走回主人身后一格（`Master.Back`）。
    fn process_pet_ai(&mut self, monster: &mut Monster, now: i64) {
        let Some(master_id) = monster.master else {
            return;
        };
        let Some((master_map, master_position, master_direction, master_dead)) =
            self.players.get(&master_id).map(|master| {
                (
                    master.map.clone(),
                    master.position,
                    master.direction,
                    master.dead,
                )
            })
        else {
            return;
        };
        // 主人已死：`player_died` 会统一清掉召唤物；这里不再行动。
        if master_dead {
            return;
        }
        // `C.ChangePMode`：命令存在主人身上（Crystal `HumanObject.PMode`，
        // `MonsterObject.cs:1706`），宠物每 tick 读取。
        let mode = self.pet_mode(master_id);
        if mode == PetMode::None {
            // 休息：不索敌、不跟随（`ProcessAI` 的 None 分支 + `ProcessRoam` 返回）。
            monster.pet_target = None;
            return;
        }
        // 只有「会跟随」的命令在主人跨图/超出 DataRange 时召回
        // （`MonsterObject.cs:1710-1715`；AttackOnly/None 不召回）。
        if matches!(
            mode,
            PetMode::Both | PetMode::MoveOnly | PetMode::FocusMasterTarget
        ) && (master_map != monster.map
            || !monster.position.in_range(master_position, VIEW_RANGE))
        {
            self.recall_pet_monster(monster);
            return;
        }

        // MoveOnly / None 不攻击：目标清空（`MonsterObject.cs:1717-1720`）。
        if matches!(mode, PetMode::MoveOnly | PetMode::None) {
            monster.pet_target = None;
        } else {
            if let Some(target_id) = monster.pet_target
                && !self.pet_target_valid(monster, master_id, target_id)
            {
                monster.pet_target = None;
            }
            if monster.pet_target.is_none() {
                monster.pet_target =
                    self.find_pet_target(monster, master_id, mode == PetMode::FocusMasterTarget);
            }
        }

        if let Some(target_id) = monster.pet_target {
            if !monster.can_attack(now) {
                return;
            }
            let Some(target_position) = self.attackable_position(&monster.map, target_id) else {
                monster.pet_target = None;
                return;
            };
            if target_position != monster.position && monster.position.in_range(target_position, 1)
            {
                self.pet_attack(monster, target_id, now);
                return;
            }
            // AttackOnly 不能移动（Crystal `CanMove` 只放行 MoveOnly/Both/Focus，
            // `MonsterObject.cs:643`）：够不到就站着。
            if mode != PetMode::AttackOnly {
                self.monster_move_to(monster, target_position, now);
            }
            return;
        }

        // `ProcessRoam`：`if (Master != null) { MoveTo(Master.Back); return; }`（`MonsterObject.cs:1821-1825`）。
        // AttackOnly 不跟随（Crystal `CanMove` 不放行 AttackOnly，`:643`）。
        if matches!(
            mode,
            PetMode::Both | PetMode::MoveOnly | PetMode::FocusMasterTarget
        ) {
            let back = master_position.moved(master_direction, -1);
            if monster.position != back {
                self.monster_move_to(monster, back, now);
            }
        }
    }

    /// 可攻击对象的位置（玩家或怪物，必须同图且存活）。
    fn attackable_position(&self, map: &str, target_id: u32) -> Option<Point> {
        if let Some(player) = self.players.get(&target_id) {
            return (!player.dead && player.map == map).then_some(player.position);
        }
        self.monster(target_id)
            .filter(|monster| !monster.dead && monster.map == map)
            .map(|monster| monster.position)
    }

    /// 召唤物的目标是否仍然合法。
    pub(crate) fn pet_target_valid(
        &self,
        monster: &Monster,
        master_id: u32,
        target_id: u32,
    ) -> bool {
        // 召唤物永远不打自己的主人（即使主人的 `last_target` 指向自己）。
        if target_id == master_id {
            return false;
        }
        if let Some(player) = self.players.get(&target_id) {
            return !player.dead
                && player.map == monster.map
                && monster
                    .position
                    .in_range(player.position, monster.kind.view_range)
                && !hidden_from(&player.magic, monster, player.level)
                && self.is_attack_target_player(master_id, target_id);
        }
        self.monster(target_id).is_some_and(|target| {
            !target.dead
                && target.map == monster.map
                && target.master.is_none()
                && !target.kind.is_guard()
                && monster
                    .position
                    .in_range(target.position, monster.kind.view_range)
        })
    }

    /// 召唤物选目标：先打主人最近一次攻击/施法的目标，否则打视野内最近的野生怪
    /// （Crystal 的召唤物用 `FindTarget` 自己索敌，`MonsterObject.cs:1918-1997`；
    /// 这里加上「主人目标」优先，见任务书要求）。
    fn find_pet_target(
        &mut self,
        monster: &Monster,
        master_id: u32,
        focus_master: bool,
    ) -> Option<u32> {
        if let Some(last_target) = self
            .players
            .get(&master_id)
            .and_then(|master| master.magic.last_target)
            && last_target != master_id
            && last_target != monster.object_id
            && self.pet_target_valid(monster, master_id, last_target)
        {
            return Some(last_target);
        }
        // `FocusMasterTarget`：只打主人的目标，不回落到最近的野生怪
        // （Crystal `HumanObject.cs:2769/3038/3479` 的 PMode 分支）。
        if focus_master {
            return None;
        }
        let mut best: Option<(i32, i32, i32, u32)> = None;
        for target_id in self.aoi_ids_in_radius_spatial(
            &monster.map,
            monster.position,
            monster.kind.view_range,
            AoiKind::Monster,
        ) {
            let Some(target) = self.monster(target_id) else {
                continue;
            };
            if target.dead
                || target.map != monster.map
                || target.master.is_some()
                || target.kind.is_guard()
            {
                continue;
            }
            let distance = monster.position.max_distance(target.position);
            let key = (distance, target.position.y, target.position.x);
            if best.is_none_or(|(best_d, best_y, best_x, _)| key < (best_d, best_y, best_x)) {
                best = Some((key.0, key.1, key.2, target_id));
            }
        }
        best.map(|(_, _, _, object_id)| object_id)
    }

    /// 召唤物攻击（Crystal `MonsterObject.Attack`，`MonsterObject.cs:2150-2173`）。
    ///
    /// 与野生怪相同：广播 `ObjectAttack`、`ActionTime = now + 300`、
    /// 伤害延迟 300ms 结算。目标可以是野生怪或玩家。
    /// 怪物攻击另一个怪物（召唤物 ↔ 野生怪，Crystal `MonsterObject.Attack`，
    /// `MonsterObject.cs:2150-2173`）：广播 `ObjectAttack`、`ActionTime = now + 300`、
    /// 伤害延迟 300ms 结算。返回是否真的排入了伤害。
    ///
    /// 宠物打野怪时经验归主人（Crystal `GetAttacker` 会把宠物的主人当作 `EXPOwner`，
    /// `MonsterObject.cs:1008-1016`、`:2627-2630`）；野生怪打宠物时正常记在攻击者身上，
    /// 宠物死亡本来就跳过经验/掉落（`monster_died` 的 `master.is_some()` 分支）。
    fn attack_monster(&mut self, monster: &mut Monster, target_id: u32, now: i64) -> bool {
        let Some(target_position) = self.attackable_position(&monster.map, target_id) else {
            return false;
        };
        monster.direction = monster.position.direction_to(target_position);
        let packet = ServerPacket::ObjectAttack(ObjectAttack {
            object_id: monster.object_id,
            location: monster.position,
            direction: monster.direction,
            spell: Spell::None,
            level: 0,
            type_: 0,
        })
        .encode_frame();
        self.broadcast(
            &monster.map,
            monster.position,
            Some(monster.object_id),
            &packet,
            monster.object_id,
        );

        monster.action_time = now + MONSTER_ATTACK_ACTION_MS;
        monster.attack_time = now + monster.attack_speed_ms;
        let damage = monster.stats.physical_attack(&mut self.rng);
        if damage == 0 {
            return true;
        }
        if monster.master.is_some()
            && let Some(target) = self.monster_mut(target_id)
            && target.expo_owner.is_none()
        {
            target.expo_owner = monster.master;
        }
        self.pending_hits.push(PendingHit {
            at: now + MONSTER_ATTACK_DAMAGE_DELAY_MS,
            attacker_id: monster.object_id,
            target_id,
            damage,
            defence: DefenceType::ACAgility,
        });
        true
    }

    /// 物理特殊攻击（AOE/直线）：广播 `S.ObjectAttack`，DC 伤害，按 Crystal 子类延迟。
    fn process_special_target(
        &mut self,
        monster: &mut Monster,
        target_id: u32,
        target_position: Point,
        same_map: bool,
        spec: MonsterSpecialSpec,
        now: i64,
    ) {
        if !same_map || target_position == monster.position {
            return;
        }
        if spec.snake && !in_snake_neighborhood(monster.position, target_position) {
            if !spec.stationary {
                self.monster_move_to(monster, target_position, now);
            }
            return;
        }
        if monster.position.max_distance(target_position) > spec.range {
            if !spec.stationary {
                self.monster_move_to(monster, target_position, now);
            }
            return;
        }
        let distance = monster.position.max_distance(target_position);
        // GreatFoxSpirit：距离 >2 用半径 7 的远程 AOE，否则半径 2（`GreatFoxSpirit.cs:105-126`）。
        let shape = if monster.kind.ai == 50 {
            MonsterMagicShape::Aoe {
                radius: if distance > 2 { 7 } else { 2 },
                at_target: false,
            }
        } else {
            spec.shape
        };
        let targets = self.magic_targets(monster, target_id, target_position, shape);
        if targets.is_empty() {
            monster.target = None;
            return;
        }
        monster.direction = monster.position.direction_to(target_position);
        let packet = if monster.kind.ai == 50 && distance > 2 {
            ServerPacket::ObjectRangeAttack(ObjectRangeAttack {
                object_id: monster.object_id,
                location: monster.position,
                direction: monster.direction,
                target_id,
                target: target_position,
                type_: 0,
                spell: Spell::None,
                level: 0,
            })
        } else {
            ServerPacket::ObjectAttack(ObjectAttack {
                object_id: monster.object_id,
                location: monster.position,
                direction: monster.direction,
                spell: Spell::None,
                level: 0,
                type_: 0,
            })
        }
        .encode_frame();
        self.broadcast(
            &monster.map,
            monster.position,
            Some(monster.object_id),
            &packet,
            monster.object_id,
        );
        monster.action_time = now + MONSTER_ATTACK_ACTION_MS;
        monster.attack_time = now + monster.attack_speed_ms;
        let damage = monster.stats.physical_attack(&mut self.rng);
        if damage == 0 {
            return;
        }
        for &target in &targets {
            // Crystal `LineAttack`：每个目标的延迟 = 距离 * 50 + additionalDelay
            // （`MonsterObject.cs:3611-3645`；BoneSpearman additionalDelay=250）。
            let distance_delay = if matches!(spec.shape, MonsterMagicShape::Line { .. }) {
                self.players
                    .get(&target)
                    .map(|player| i64::from(monster.position.max_distance(player.position)) * 50)
                    .or_else(|| {
                        self.monster(target).map(|target| {
                            i64::from(monster.position.max_distance(target.position)) * 50
                        })
                    })
                    .unwrap_or(0)
            } else {
                0
            };
            self.pending_hits.push(PendingHit {
                at: now + distance_delay + spec.delay_ms,
                attacker_id: monster.object_id,
                target_id: target,
                damage,
                defence: spec.defence,
            });
        }
        if !spec.effects.is_empty() {
            // Crystal 在 `CompleteAttack` 里对每个命中目标调用 `PoisonTarget`
            // （`EvilCentipede.cs:82-84`、`GreatFoxSpirit.cs:127-129`、`VenomSpider.cs:40-44`）。
            // 本作无法在 `world.rs` 的延迟结算里挂效果（文件归兵部），因此在**出招时**按
            // 同一概率/跳数结算；绿毒数值沿用 `GetAttackPower(MinSC, MaxSC)`。
            let green_value = monster.stats.attack_power(
                monster.stats.min_sc,
                monster.stats.max_sc,
                &mut self.rng,
            );
            for target in &targets {
                self.apply_special_effects(
                    monster.object_id,
                    *target,
                    spec.effects,
                    green_value,
                    now,
                );
            }
        }
    }

    /// 物理特殊攻击的附加效果：按 Crystal `PoisonTarget` 的概率（`Random.Next(chance)==0`）
    /// 给目标挂毒；绿毒数值来自 `GetAttackPower(MinSC, MaxSC)`。
    fn apply_special_effects(
        &mut self,
        caster_id: u32,
        target_id: u32,
        effects: &[MonsterHitEffect],
        green_value: i32,
        now: i64,
    ) {
        for effect in effects {
            if effect.chance > 1 && self.rng.next_bounded(effect.chance) != 0 {
                continue;
            }
            let value = if effect.kind == PoisonType::Green {
                green_value.max(0)
            } else {
                0
            };
            let poison = PoisonState::new(effect.kind, value, effect.duration, now, caster_id);
            if self.monster(target_id).is_some() {
                self.apply_poison_to_monster(target_id, poison);
            } else {
                self.apply_poison_to_player(target_id, poison);
            }
        }
    }

    /// 处理魔法怪的施法决策：射程内施法；站桩怪不追；HumanWizard 贴身先拉开。
    fn process_magic_target(
        &mut self,
        monster: &mut Monster,
        target_id: u32,
        target_position: Point,
        same_map: bool,
        spec: MonsterMagicSpec,
        now: i64,
    ) {
        if !same_map || target_position == monster.position {
            return;
        }
        let distance = monster.position.max_distance(target_position);
        if distance > spec.range {
            if !spec.stationary {
                self.monster_move_to(monster, target_position, now);
            }
            return;
        }
        // HumanWizard：`dist < AttackRange → Walk(awayFromTarget)`（`HumanWizard.cs:80-115`）；
        // `FearTime` 首轮 5 秒只走位，窗口结束后才施法。
        if spec.kite && distance < spec.range {
            if monster.fear_time == 0 {
                monster.fear_time = now + 5_000;
                let away = target_position.direction_to(monster.position);
                if self.monster_walk(monster, away, now) {
                    return;
                }
            } else if now < monster.fear_time {
                let away = target_position.direction_to(monster.position);
                if self.monster_walk(monster, away, now) {
                    return;
                }
            }
        }
        if !monster.can_attack(now) {
            return;
        }
        if !self.cast_monster_magic(monster, target_id, target_position, spec, now) {
            monster.target = None;
        }
    }

    /// 怪物魔法施法：广播 `S.ObjectMagic`，用 `magic.rs` 的 `UserMagic::damage`
    /// 计算伤害（与玩家魔法同公式），按单体/直线/AOE 收集目标后延迟结算。
    fn cast_monster_magic(
        &mut self,
        monster: &mut Monster,
        target_id: u32,
        target_position: Point,
        spec: MonsterMagicSpec,
        now: i64,
    ) -> bool {
        let targets = self.magic_targets(monster, target_id, target_position, spec.shape);
        if targets.is_empty() {
            return false;
        }
        let direction = monster.position.direction_to(target_position);
        let secondary_target_ids = targets
            .iter()
            .copied()
            .filter(|id| *id != target_id)
            .collect();
        let packet = ServerPacket::ObjectMagic(ObjectMagic {
            object_id: monster.object_id,
            location: monster.position,
            direction,
            spell: spec.spell,
            target_id,
            target: target_position,
            cast: true,
            level: 3,
            self_broadcast: false,
            secondary_target_ids,
        })
        .encode_frame();
        self.broadcast(
            &monster.map,
            monster.position,
            Some(monster.object_id),
            &packet,
            monster.object_id,
        );

        monster.direction = direction;
        monster.action_time = now + MONSTER_ATTACK_ACTION_MS;
        monster.attack_time = now + spec.cooldown_ms;
        // 口径（任务书要求复用玩家魔法公式）：
        // `base = GetAttackPower(MinMC, MaxMC)` → `UserMagic::damage(ThunderBolt MagicInfo)`。
        // Crystal `HumanWizard.Attack`（`HumanWizard.cs:55-70`）是直接 `GetAttackPower`
        // 不套 MagicInfo；这里按任务书统一走 `magic.rs` 公式，差异在 docs 登记。
        let base = if spec.use_magic_stats {
            monster
                .stats
                .attack_power(monster.stats.min_mc, monster.stats.max_mc, &mut self.rng)
        } else {
            monster.stats.physical_attack(&mut self.rng)
        };
        if base == 0 {
            return true;
        }
        let magic_info = self.magic_info(spec.spell).cloned();
        let user_magic = UserMagic {
            spell: spec.spell,
            level: 3,
            key: 0,
            experience: 0,
        };
        let damage = magic_info.map_or(base, |info| user_magic.damage(&info, base, &mut self.rng));
        for target in targets {
            self.pending_hits.push(PendingHit {
                at: now + 500,
                attacker_id: monster.object_id,
                target_id: target,
                damage,
                defence: DefenceType::MAC,
            });
        }
        true
    }

    /// 怪物魔法的目标集合：单体 / 以目标或自身为中心的 AOE / 直线穿透。
    fn magic_targets(
        &self,
        monster: &Monster,
        target_id: u32,
        target_position: Point,
        shape: MonsterMagicShape,
    ) -> Vec<u32> {
        // 玩家：Crystal `PlayerObject.IsAttackTarget(MonsterObject)`——死亡/安全区/
        // `NoFight` 地图不可打；Peace/组队/行会由**玩家**攻击模式决定，怪物 AOE
        // 不按这些过滤（Crystal 同口径，见 docs 差异）。
        let valid_player = |id: u32| {
            id != monster.object_id
                && self.players.get(&id).is_some_and(|player| {
                    !player.dead
                        && player.map == monster.map
                        && !self.in_safe_zone(id)
                        && !self.map_no_fight(&player.map)
                })
        };
        // 召唤物（Crystal `FindAllTargets` 的 Monster/Hero；野生怪之间不互殴）。
        let valid_monster = |id: u32| {
            id != monster.object_id
                && self.monster(id).is_some_and(|target| {
                    target.master.is_some()
                        && !target.dead
                        && target.map == monster.map
                        && !target.kind.is_guard()
                })
        };
        let valid = |id: u32| valid_player(id) || valid_monster(id);
        let mut targets = BTreeSet::new();
        match shape {
            MonsterMagicShape::Single => {
                if valid(target_id) {
                    targets.insert(target_id);
                }
            }
            MonsterMagicShape::Aoe { radius, at_target } => {
                let center = if at_target {
                    target_position
                } else {
                    monster.position
                };
                for kind in [AoiKind::Player, AoiKind::Monster] {
                    for id in self.aoi_ids_in_radius(&monster.map, center, radius, kind) {
                        if valid(id) {
                            targets.insert(id);
                        }
                    }
                }
            }
            MonsterMagicShape::Line { length } => {
                let direction = monster.position.direction_to(target_position);
                let mut point = monster.position;
                for _ in 0..length {
                    point = point.moved(direction, 1);
                    if !self.walkable_on(&monster.map, point) {
                        // Crystal `LineAttack` 遇墙继续扫后续格，不截断整条线。
                        continue;
                    }
                    for entry in self.aoi_in_cell(&monster.map, point) {
                        if matches!(entry.kind, AoiKind::Player | AoiKind::Monster)
                            && valid(entry.object_id)
                        {
                            targets.insert(entry.object_id);
                        }
                    }
                }
            }
        }
        targets.into_iter().collect()
    }

    /// 远程物理攻击（Crystal `AxeSkeleton.Attack`/`TownArcher.Attack`，
    /// `Monsters/AxeSkeleton.cs:23-49`、`Monsters/TownArcher.cs:28-48`）：
    /// 广播 `S.ObjectRangeAttack`，伤害延迟 `max_distance * 50 + 500ms`
    /// （`MonsterObject.ProjectileAttack`）。
    fn ranged_attack(&mut self, monster: &mut Monster, target_id: u32, now: i64) -> bool {
        let Some(target_position) = self.attackable_position(&monster.map, target_id) else {
            return false;
        };
        monster.direction = monster.position.direction_to(target_position);
        let packet = ServerPacket::ObjectRangeAttack(ObjectRangeAttack {
            object_id: monster.object_id,
            location: monster.position,
            direction: monster.direction,
            target_id,
            target: target_position,
            type_: 0,
            spell: Spell::None,
            level: 0,
        })
        .encode_frame();
        self.broadcast(
            &monster.map,
            monster.position,
            Some(monster.object_id),
            &packet,
            monster.object_id,
        );

        monster.action_time = now + MONSTER_ATTACK_ACTION_MS;
        monster.attack_time = now + monster.attack_speed_ms;
        let damage = monster.stats.physical_attack(&mut self.rng);
        if damage == 0 {
            return true;
        }
        if monster.master.is_some()
            && let Some(target) = self.monster_mut(target_id)
            && target.expo_owner.is_none()
        {
            target.expo_owner = monster.master;
        }
        let distance = i64::from(monster.position.max_distance(target_position));
        self.pending_hits.push(PendingHit {
            at: now + distance * 50 + 500,
            attacker_id: monster.object_id,
            target_id,
            damage,
            defence: DefenceType::ACAgility,
        });
        true
    }

    /// 召唤物攻击：失败时清掉 `pet_target`（目标死亡/换图/消失）。
    fn pet_attack(&mut self, monster: &mut Monster, target_id: u32, now: i64) {
        if !self.attack_monster(monster, target_id, now) {
            monster.pet_target = None;
        }
    }

    /// Crystal `MonsterObject.FindTarget`（`MonsterObject.cs:1918-1997`）。
    ///
    /// 环形扫描：`d` 从 0 递增到视野半径，同环内按 `y`、`x` 升序取第一个可攻击对象。
    /// mir-next 直接对玩家列表求「(距离, y, x) 最小」的那个，与逐格扫描的先后顺序等价。
    fn find_target(&mut self, monster: &mut Monster) {
        // `Target ??= ob` / `if (Target != null) continue;`：已有目标就不再换。
        if monster.target.is_some() {
            return;
        }
        // 攻城箭塔：`ConquestArcher.cs:53` 的三条过滤（本会已拥城 / 隐身 / 未开战）。
        let archer_owner = monster
            .conquest
            .and_then(|index| self.conquest(index))
            .filter(|conquest| conquest.war_is_on)
            .map(|conquest| conquest.owner);
        if monster.kind.is_conquest_archer() && archer_owner.is_none() {
            // 没在开战的攻城箭塔不索敌（`!Conquest.WarIsOn → continue`，`ConquestArcher.cs:53`）。
            return;
        }
        let mut best: Option<(i32, i32, i32, u32)> = None;
        // Crystal `for (int d = 0; d <= Info.ViewRange; d++)` 逐环扫格子（`MonsterObject.cs:1922`）；
        // 这里用 AOI 网格取半径内的玩家，顺序按 `(y, x, ID)`，与逐格扫描一致。
        let candidates = self.aoi_ids_in_radius_spatial(
            &monster.map,
            monster.position,
            monster.kind.view_range,
            AoiKind::Player,
        );
        for player_id in candidates {
            let Some(player) = self.players.get(&player_id) else {
                continue;
            };
            if player.map != monster.map || player.dead {
                continue;
            }
            // 隐身：`MonsterObject.cs:1974` 的 `ob.Hidden && (!CoolEye || Level < ob.Level)`。
            if hidden_from(&player.magic, monster, player.level) {
                continue;
            }
            // 守卫只攻击红名（`PlayerObject.IsAttackTarget(MonsterObject)`，
            // `PlayerObject.cs:4679`：`AI == 6 || 58 || 113 → return PKPoints >= 200`）。
            if monster.kind.is_guard() && player.pk_points < RED_NAME_PK_POINTS {
                continue;
            }
            // TownArcher `FindTarget`：只锁定 PKPoints >= 200 的红名玩家
            // （`Monsters/TownArcher.cs:96-112`）。
            if monster.kind.is_town_archer() && player.pk_points < RED_NAME_PK_POINTS {
                continue;
            }
            if monster.kind.is_conquest_archer() {
                // `playerob.MyGuild != null && playerob.MyGuild.Conquest != null
                //  && Conquest.Info.Index == playerob.MyGuild.Conquest.Info.Index → continue`
                // （`ConquestArcher.cs:53`）：城主行会的人在城里不会被射。
                if let Some(owner) = archer_owner
                    && player
                        .guild
                        .is_some_and(|reference| reference.guild_id == owner)
                {
                    continue;
                }
            }
            let distance = monster.position.max_distance(player.position);
            if distance > monster.kind.view_range {
                continue;
            }
            let key = (distance, player.position.y, player.position.x);
            let better =
                best.is_none_or(|(best_d, best_y, best_x, _)| key < (best_d, best_y, best_x));
            if better {
                best = Some((key.0, key.1, key.2, player.object_id));
            }
        }
        // Crystal `FindTarget` 的 `case ObjectType.Monster`（`MonsterObject.cs:1940-1963`）：
        // 野生怪的候选里还有**玩家召唤物**。`IsAttackTarget(MonsterObject)` 的
        // 「Pet Attacked」分支（`:2481-2484`）对「野生怪 → 宠物」返回 true；
        // 守卫（AI 6/113）只打红名主人的宠物（`:2471-2475`），Tao Guard（AI 58）
        // 只排除和平模式主人的宠物（`:2476-2479`），攻城箭塔不把宠物当目标。
        if !monster.kind.is_conquest_archer() {
            for pet_id in self.aoi_ids_in_radius_spatial(
                &monster.map,
                monster.position,
                monster.kind.view_range,
                AoiKind::Monster,
            ) {
                if pet_id == monster.object_id {
                    continue;
                }
                let Some(pet) = self.monster(pet_id) else {
                    continue;
                };
                if pet.dead || pet.map != monster.map || !self.can_target_pet(monster, pet) {
                    continue;
                }
                let distance = monster.position.max_distance(pet.position);
                if distance > monster.kind.view_range {
                    continue;
                }
                let key = (distance, pet.position.y, pet.position.x);
                let better =
                    best.is_none_or(|(best_d, best_y, best_x, _)| key < (best_d, best_y, best_x));
                if better {
                    best = Some((key.0, key.1, key.2, pet.object_id));
                }
            }
        }
        if let Some((_, _, _, object_id)) = best {
            monster.target = Some(object_id);
        }
    }

    /// 怪物能否把某个召唤物当目标（Crystal `IsAttackTarget(MonsterObject)` 的
    /// Guard / Tao Guard / Pet Attacked 分支，`MonsterObject.cs:2465-2521`）。
    ///
    /// * 普通野生怪：任何召唤物都可以（`:2481-2484`）；
    /// * 守卫（AI 6/113）：只打红名主人的召唤物（`:2471-2475`）；
    /// * Tao Guard（AI 58）：和平模式主人的召唤物不打（`:2476-2479`）；
    /// * 攻城箭塔：不把召唤物当目标（`ConquestArcher.cs` 只射玩家）。
    fn can_target_pet(&self, attacker: &Monster, pet: &Monster) -> bool {
        let Some(master_id) = pet.master else {
            return false;
        };
        let master = self.players.get(&master_id);
        match attacker.kind.ai {
            58 => master.is_some_and(|master| master.attack_mode != AttackMode::Peace),
            6 | 113 => master.is_some_and(|master| master.pk_points >= RED_NAME_PK_POINTS),
            80 => false,
            _ => true,
        }
    }

    /// Crystal `MonsterObject.MoveTo`（`MonsterObject.cs:2027-2071`）。
    fn monster_move_to(&mut self, monster: &mut Monster, location: Point, now: i64) {
        if monster.position == location {
            return;
        }
        // 目标就在隔壁：Crystal 先 `ValidPoint` 再查占格，任一不满足就**原地不动**（不绕路）。
        if monster.position.in_range(location, 1)
            && (!self.walkable_on(&monster.map, location)
                || !self.monster_cell_free(monster, location))
        {
            return;
        }
        let mut direction = monster.position.direction_to(location);
        if self.monster_walk(monster, direction, now) {
            return;
        }
        // `switch (Envir.Random.Next(2))`：走不通就按顺时针/逆时针各试 7 个方向。
        if self.rng.next_bounded(2) == 0 {
            for _ in 0..7 {
                direction = direction.next();
                if self.monster_walk(monster, direction, now) {
                    return;
                }
            }
        } else {
            for _ in 0..7 {
                direction = direction.previous();
                if self.monster_walk(monster, direction, now) {
                    return;
                }
            }
        }
    }

    /// Crystal `MonsterObject.Walk`（`MonsterObject.cs:2095-2149`）。
    ///
    /// 怪物不会跑、不查门、不触发传送；走成功后广播 `ObjectWalk` 并刷新周围玩家的视野。
    fn monster_walk(&mut self, monster: &mut Monster, direction: MirDirection, now: i64) -> bool {
        if !monster.can_move(now) {
            return false;
        }
        let location = monster.position.adjacent(direction);
        if !self.walkable_on(&monster.map, location) || !self.monster_cell_free(monster, location) {
            return false;
        }
        let old_position = monster.position;
        monster.position = location;
        // AOI：Crystal `HumanObject/MonsterObject.Walk` 的 `GetCell(...).Remove/Add`
        // （`MonsterObject.cs:2103-2119`），这里同步搬格。
        self.aoi_move(&monster.map, AoiKind::Monster, monster.object_id, location);
        monster.direction = direction;
        monster.cell_time = now + MONSTER_CELL_GRACE_MS;
        monster.action_time = now + MONSTER_WALK_ACTION_MS;
        monster.move_time = now + monster.move_speed_ms;
        if monster.move_time > monster.attack_time {
            monster.attack_time = monster.move_time;
        }

        let packet = ServerPacket::ObjectWalk(ObjectWalk {
            object_id: monster.object_id,
            location,
            direction,
        })
        .encode_frame();
        self.broadcast(
            &monster.map,
            location,
            Some(monster.object_id),
            &packet,
            monster.object_id,
        );
        // 位置变了，重新算一次这只怪在其它玩家视野里的进出（Crystal `AddObjects/RemoveObjects`）。
        // 传旧坐标：候选玩家 = 旧位置与新位置两个视野矩形的并集，不会漏掉「刚走出视野」的人。
        let info = monster_info_frame(monster);
        self.sync_monster_view(
            monster.object_id,
            &monster.map,
            Some(old_position),
            location,
            &info,
        );
        true
    }

    /// Crystal `MonsterObject.Turn`（`MonsterObject.cs:2073-2093`）：只改方向，不动 `ActionTime`。
    fn monster_turn(&mut self, monster: &mut Monster, direction: MirDirection, now: i64) {
        if !monster.can_move(now) {
            return;
        }
        monster.direction = direction;
        let packet = ServerPacket::ObjectTurn(ObjectTurn {
            object_id: monster.object_id,
            location: monster.position,
            direction,
        })
        .encode_frame();
        self.broadcast(
            &monster.map,
            monster.position,
            Some(monster.object_id),
            &packet,
            monster.object_id,
        );
    }

    /// Crystal `MonsterObject.Attack`（`MonsterObject.cs:2150-2173`）。
    ///
    /// 伤害不会立刻结算：Crystal 把 `DelayedAction(DelayedType.Damage, now + 300, ...)`
    /// 塞进 `ActionList`，300ms 后才在 `CompleteAttack` 里调 `target.Attacked(...)`。
    /// mir-next 用 [`World::pending_hits`] 复现这一延迟。
    fn monster_attack(&mut self, monster: &mut Monster, target_id: u32, now: i64) {
        let Some(target) = self.players.get(&target_id) else {
            monster.target = None;
            return;
        };
        if target.dead || target.map != monster.map {
            monster.target = None;
            return;
        }
        let target_position = target.position;

        monster.direction = monster.position.direction_to(target_position);
        let packet = ServerPacket::ObjectAttack(ObjectAttack {
            object_id: monster.object_id,
            location: monster.position,
            direction: monster.direction,
            spell: Spell::None,
            level: 0,
            type_: 0,
        })
        .encode_frame();
        self.broadcast(
            &monster.map,
            monster.position,
            Some(monster.object_id),
            &packet,
            monster.object_id,
        );

        monster.action_time = now + MONSTER_ATTACK_ACTION_MS;
        monster.attack_time = now + monster.attack_speed_ms;

        let damage = monster.stats.physical_attack(&mut self.rng);
        // `MonsterObject.cs:2169`：`if (damage == 0) return;`（0 伤害连动作都不排）。
        if damage == 0 {
            return;
        }
        self.pending_hits.push(PendingHit {
            at: now + MONSTER_ATTACK_DAMAGE_DELAY_MS,
            attacker_id: monster.object_id,
            target_id,
            damage,
            defence: DefenceType::ACAgility,
        });
    }

    /// 守卫攻击：Crystal `Guard.Attack`（`Server/MirObjects/Monsters/Guard.cs:66-87`）。
    ///
    /// 三个与野生怪不同的地方，全部照抄：
    ///
    /// 1. 攻击动作的坐标是**目标的背后一格**（`Target.Back`，`:70`），
    ///    方向是从「背后」指向目标（也就是目标朝向的反方向，`:71`）；
    /// 2. 紧跟一条 `S.ObjectTurn` 把守卫自己的朝向/位置再同步一次（`:74`）；
    /// 3. 延迟 300ms、以 `DefenceType.AC` 结算（不掷闪避，`:85`）。
    ///
    /// 动作锁定 `ActionTime = now + 500`（`:76`），攻击冷却 `now + AttackSpeed`（`:77`）。
    fn guard_attack(&mut self, monster: &mut Monster, target_id: u32, now: i64) {
        let Some(target) = self.players.get(&target_id) else {
            monster.target = None;
            return;
        };
        if target.dead || target.map != monster.map {
            monster.target = None;
            return;
        }
        let target_position = target.position;
        let behind = target_position.moved(target.direction, -1);

        let packet = ServerPacket::ObjectAttack(ObjectAttack {
            object_id: monster.object_id,
            location: behind,
            direction: behind.direction_to(target_position),
            spell: Spell::None,
            level: 0,
            type_: 0,
        })
        .encode_frame();
        self.broadcast(
            &monster.map,
            monster.position,
            Some(monster.object_id),
            &packet,
            monster.object_id,
        );
        let turn = ServerPacket::ObjectTurn(ObjectTurn {
            object_id: monster.object_id,
            location: monster.position,
            direction: monster.direction,
        })
        .encode_frame();
        self.broadcast(
            &monster.map,
            monster.position,
            Some(monster.object_id),
            &turn,
            monster.object_id,
        );

        monster.action_time = now + GUARD_ATTACK_ACTION_MS;
        monster.attack_time = now + monster.attack_speed_ms;

        let damage = monster.stats.physical_attack(&mut self.rng);
        // `if (damage == 0) return;`（`Monsters/Guard.cs:83`）。
        if damage == 0 {
            return;
        }
        self.pending_hits.push(PendingHit {
            at: now + GUARD_ATTACK_DAMAGE_DELAY_MS,
            attacker_id: monster.object_id,
            target_id,
            damage,
            defence: DefenceType::AC,
        });
    }

    /// Crystal `MonsterObject.ProcessRegen`（`MonsterObject.cs:1414-1452`）。
    ///
    /// 守卫覆盖成空实现（`Monsters/Guard.cs:19-22`、`:47`）。
    fn process_monster_regen(&mut self, monster: &mut Monster, now: i64) {
        if monster.kind.is_guard() || monster.dead || now < monster.regen_time {
            return;
        }
        monster.regen_time = now + REGEN_DELAY_MS;
        if monster.hp >= monster.stats.hp {
            return;
        }
        // `healthRegen += (int)(Stats[Stat.HP] * 0.022F) + 1`。
        let gain = (f64::from(monster.stats.hp) * 0.022) as i32 + 1;
        monster.hp = (monster.hp + gain).min(monster.stats.hp);
        self.emit_damage_indicator(
            &monster.map,
            monster.position,
            monster.object_id,
            gain,
            DamageType::Hit,
        );
    }

    /// Crystal `MonsterObject.SetOperateTime`（`MonsterObject.cs:1232-1310`）：
    /// 取 `now + 2000` 与所有「还在未来」的 deadline 的最小值。
    fn monster_operate_time(monster: &Monster, now: i64) -> i64 {
        let mut time = now + MONSTER_OPERATE_DELAY_MS;
        for deadline in [
            monster.dead_time,
            monster.search_time,
            monster.roam_time,
            monster.alone_time,
            monster.regen_time,
            monster.action_time,
            monster.move_time,
            monster.attack_time,
        ] {
            if deadline < time && deadline > now {
                time = deadline;
            }
        }
        // 中毒：下一跳必须准时被调度，否则 `tick_monsters` 会隔 2 秒才轮一次。
        if let Some(poison_tick) = monster.next_poison_tick()
            && poison_tick < time
            && poison_tick > now
        {
            time = poison_tick;
        }
        time
    }

    /// 目标格上是否已有别的阻挡对象（怪物 `Walk` 的占格判定，`MonsterObject.cs:2105-2112`）。
    fn monster_cell_free(&self, monster: &Monster, cell: Point) -> bool {
        // 只查目标格（Crystal `CurrentMap.GetCell(location).Objects`，`MonsterObject.cs:2105-2112`）。
        for entry in self.aoi_in_cell(&monster.map, cell) {
            let blocked = match entry.kind {
                AoiKind::Player => self
                    .players
                    .get(&entry.object_id)
                    .is_some_and(|player| !player.dead && player.position == cell),
                AoiKind::Monster => {
                    entry.object_id != monster.object_id
                        && self
                            .monster(entry.object_id)
                            .is_some_and(|other| !other.dead && other.position == cell)
                }
                // 既有语义：怪物走位只被玩家与其它怪物挡住（NPC 由地图可走性负责）。
                AoiKind::Npc | AoiKind::Item => false,
            };
            if blocked {
                return false;
            }
        }
        true
    }

    /// 自己脚下是否还踩着别的对象（Crystal `CheckStacked`）。
    fn monster_cell_stacked(&self, monster: &Monster) -> bool {
        for entry in self.aoi_in_cell(&monster.map, monster.position) {
            let stacked = match entry.kind {
                AoiKind::Player => self
                    .players
                    .get(&entry.object_id)
                    .is_some_and(|player| !player.dead && player.position == monster.position),
                AoiKind::Monster => {
                    entry.object_id != monster.object_id
                        && self
                            .monster(entry.object_id)
                            .is_some_and(|other| !other.dead && other.position == monster.position)
                }
                AoiKind::Npc | AoiKind::Item => false,
            };
            if stacked {
                return true;
            }
        }
        false
    }

    /// 怪物死亡：Crystal `MonsterObject.Die`（`MonsterObject.cs:966-1006`）。
    ///
    /// 照抄的部分：`HP = 0`、`Dead = true`、`DeadTime = now + DeadDelay`、广播 `S.ObjectDied`、
    /// `EXPOwner.WinExp(Experience, Level)`、击杀任务计数（`EXPOwner.CheckGroupQuestKill`）、
    /// `Respawn.Count--`、`Drop()`（掉落见 [`World::drop_monster_loot`]，
    /// 对应 `:997-998` 的 `if (Master == null && EXPOwner != null) Drop();`）。
    ///
    /// 简化：`HasDieScript`（`:977-980`）、`Envir.MonsterCount` 统计
    /// （`:1003-1005`，本作按需扫描不维护计数）。
    pub(crate) fn monster_died(&mut self, monster_id: u32) {
        let now = self.now();
        // 用 `World::monster_mut`（monster_slots 下标）代替遍历全部怪物。
        let Some(monster) = self.monster_mut(monster_id) else {
            return;
        };
        if monster.dead {
            return;
        }
        monster.dead = true;
        monster.hp = 0;
        monster.dead_time = now + CORPSE_DELAY_MS;
        monster.target = None;
        monster.pet_target = None;
        let map = monster.map.clone();
        let position = monster.position;
        let direction = monster.direction;
        let respawn_group = monster.respawn_group;
        let expo_owner = monster.expo_owner;
        let experience = monster.kind.experience;
        let monster_level = monster.kind.level;
        let monster_name = monster.kind.name.clone();
        // 召唤物不产经验、不掉落、不算击杀任务（Crystal `Master == null` 三个条件，
        // `MonsterObject.cs:982`、`:997`）。
        let master = monster.master;
        monster.master = None;
        // 宠物死亡后命令重置为默认（下一次召唤 `spawn_pet` 也会重置）。
        if let Some(master_id) = master {
            self.pet_modes.remove(&master_id);
        }
        self.unregister_pet(monster_id);

        let packet = ServerPacket::ObjectDied(ObjectDied {
            object_id: monster_id,
            location: position,
            direction,
            type_: 0,
        })
        .encode_frame();
        self.broadcast(&map, position, Some(monster_id), &packet, monster_id);

        // `if (EXPOwner != null && ... ) EXPOwner.WinExp(Experience, Level);`（`MonsterObject.cs:982-992`）。
        // 简化：`EXPOwner.Race == ObjectType.Player` 恒成立（本作只有玩家能打怪）。
        if master.is_none()
            && let Some(owner) = expo_owner
        {
            self.win_experience(owner, experience, monster_level);
            // `playerObj.CheckGroupQuestKill(Info);`（`:986-990`）——击杀任务计数。
            // 组队时同图、`Globals.DataRange` 内、活着的队友一起记（`PlayerObject.cs:11494-11508`）。
            for member in self.quest_kill_recipients(owner) {
                self.advance_quest_kills(member, &monster_name);
            }
        }

        // `if (Respawn != null) Respawn.Count--;`（`MonsterObject.cs:994-995`）。
        if let Some(index) = respawn_group {
            self.on_respawn_monster_died(index, now);
        }

        // `if (Master == null && EXPOwner != null) Drop();`（`MonsterObject.cs:997-998`）——
        // 顺序照抄：经验 → 刷怪计数 → 掉落。
        // 「没有击杀者就不掉」这一条在 `drop_monster_loot` 里用 `expo_owner` 再判一次。
        if master.is_none() && expo_owner.is_some() {
            self.drop_monster_loot(monster_id);
        }
        // 召唤者死亡 → 自己的小怪一起消失（Crystal `SlaveList` 随主人 `Die()`）。
        let slaves = self.monster_slaves.remove(&monster_id);
        if let Some(slaves) = slaves {
            for slave_id in slaves {
                self.monster_died(slave_id);
            }
        }
        self.monster_slaves.retain(|_, slaves| {
            slaves.retain(|slave_id| *slave_id != monster_id);
            !slaves.is_empty()
        });
    }
}
