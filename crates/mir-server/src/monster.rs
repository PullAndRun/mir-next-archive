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

use std::sync::Arc;

use mir_core::MirRng;
use mir_core::Point;
use mir_core::enums::{
    AttackMode, DamageType, DefenceType, MirDirection, Monster as MonsterImage, Spell,
};
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::{ObjectAttack, ObjectDied, ObjectTurn, ObjectWalk};

use crate::aoi::AoiKind;
use crate::combat::{CombatStats, PendingHit};
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
        self.remove_expired_corpses(now);

        for index in 0..self.monsters.len() {
            if self.monsters[index].dead {
                // Crystal 的死亡怪物仍会 `Process()`（等 `DeadTime`），但不再跑 AI。
                continue;
            }
            // `Envir.cs:2130`：`if (Time > current.Value.OperateTime)`（严格大于）。
            if now <= self.monsters[index].operate_time {
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
            self.process_guard_return(monster, now);
            self.process_target(monster, now);
            return;
        }
        // 攻城箭塔：`ConquestArcher.CanMove` 要求 `Route.Count > 0`（`ConquestArcher.cs:11-14`），
        // 本作没有路线表 → 站桩：只索敌 + 攻击，不漫游、不追击。
        if monster.kind.is_conquest_archer() {
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
        self.process_roam(monster, now);
        self.process_target(monster, now);
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

        if monster.kind.is_guard() || monster.kind.is_conquest_archer() {
            // `Guard.InAttackRange`：同图、不同格、切比雪夫距离 <= 视野半径
            // （`Monsters/Guard.cs:40-45`）。够不到就站着：`Guard.CanMove` 要求
            // `Route.Count > 0`，没有路线时不会追（`:14-17`）。
            //
            // 攻城箭塔同理（`TownArcher.AttackRange = 10`，`TownArcher.cs:10`、
            // `ConquestArcher.cs:11-14`），本作用视野半径代替固定 10。
            if same_map
                && target_position != monster.position
                && monster
                    .position
                    .in_range(target_position, monster.kind.view_range)
            {
                if self.players.contains_key(&target_id) {
                    self.guard_attack(monster, target_id, now);
                } else if !self.attack_monster(monster, target_id, now) {
                    monster.target = None;
                }
            }
            return;
        }

        if same_map
            && target_position != monster.position
            && monster.position.in_range(target_position, 1)
        {
            let attacked = if self.players.contains_key(&target_id) {
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
        if master_map != monster.map || !monster.position.in_range(master_position, VIEW_RANGE) {
            self.recall_pet_monster(monster);
            return;
        }

        if let Some(target_id) = monster.pet_target
            && !self.pet_target_valid(monster, master_id, target_id)
        {
            monster.pet_target = None;
        }
        if monster.pet_target.is_none() {
            monster.pet_target = self.find_pet_target(monster, master_id);
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
            self.monster_move_to(monster, target_position, now);
            return;
        }

        // `ProcessRoam`：`if (Master != null) { MoveTo(Master.Back); return; }`（`MonsterObject.cs:1821-1825`）。
        let back = master_position.moved(master_direction, -1);
        if monster.position != back {
            self.monster_move_to(monster, back, now);
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
    fn find_pet_target(&mut self, monster: &Monster, master_id: u32) -> Option<u32> {
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
    }
}
