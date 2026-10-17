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
use mir_core::enums::{DamageType, DefenceType, MirDirection, Monster as MonsterImage, Spell};
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::{ObjectAttack, ObjectDied, ObjectTurn, ObjectWalk};

use crate::combat::{CombatStats, PendingHit};
use crate::world::{VIEW_RANGE, World, monster_info_frame};

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
    /// 附近有没有玩家（Crystal `Alone`：没有玩家时整只怪跳过 AI）。
    pub alone: bool,
    /// 下次检查 `alone` 的时刻（Crystal `AloneTime`）。
    pub alone_time: i64,
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

        Self {
            object_id,
            hp: kind.stats.hp,
            stats: kind.stats,
            kind,
            map: map.into(),
            position,
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
            alone: false,
            alone_time: 0,
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
            self.monsters[index] = monster;
        }
    }

    /// 移除 `dead_time` 已到的尸体：Crystal `MonsterObject.Process`（`MonsterObject.cs:1205-1216`）。
    fn remove_expired_corpses(&mut self, now: i64) {
        let mut index = 0;
        while index < self.monsters.len() {
            if self.monsters[index].dead && now >= self.monsters[index].dead_time {
                let monster_id = self.monsters[index].object_id;
                self.forget_monster(monster_id);
                self.monsters.remove(index);
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
            let still_valid = self.players.get(&target_id).is_some_and(|player| {
                !player.dead
                    && player.map == monster.map
                    && monster.position.in_range(player.position, VIEW_RANGE)
            });
            if !still_valid {
                monster.target = None;
            }
        }

        self.process_ai(monster, now);
        self.process_monster_regen(monster, now);
        monster.operate_time = Self::monster_operate_time(monster, now);
    }

    /// Crystal `MonsterObject.ProcessAI`（`MonsterObject.cs:1671-1733`）。
    fn process_ai(&mut self, monster: &mut Monster, now: i64) {
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
        monster.alone = !self.players.values().any(|player| {
            player.map == monster.map
                && !player.dead
                && monster.position.in_range(player.position, VIEW_RANGE * 2)
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

    /// Crystal `MonsterObject.ProcessTarget`（`MonsterObject.cs:1842-1870`）。
    fn process_target(&mut self, monster: &mut Monster, now: i64) {
        let Some(target_id) = monster.target else {
            return;
        };
        if !monster.can_attack(now) {
            return;
        }
        let Some(target) = self.players.get(&target_id) else {
            monster.target = None;
            return;
        };
        let target_position = target.position;
        let same_map = target.map == monster.map;

        if same_map
            && target_position != monster.position
            && monster.position.in_range(target_position, 1)
        {
            self.monster_attack(monster, target_id, now);
            return;
        }
        self.monster_move_to(monster, target_position, now);
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
        let mut best: Option<(i32, i32, i32, u32)> = None;
        for player in self.players.values() {
            if player.map != monster.map || player.dead {
                continue;
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
        if let Some((_, _, _, object_id)) = best {
            monster.target = Some(object_id);
        }
    }

    /// Crystal `MonsterObject.MoveTo`（`MonsterObject.cs:2027-2071`）。
    fn monster_move_to(&mut self, monster: &mut Monster, location: Point, now: i64) {
        if monster.position == location {
            return;
        }
        // 目标就在隔壁：Crystal 先 `ValidPoint` 再查占格，任一不满足就**原地不动**（不绕路）。
        if monster.position.in_range(location, 1)
            && (!self.walkable(location) || !self.monster_cell_free(monster, location))
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
        if !self.walkable(location) || !self.monster_cell_free(monster, location) {
            return false;
        }
        monster.position = location;
        monster.direction = direction;
        monster.cell_time = now + MONSTER_CELL_GRACE_MS;
        monster.action_time = now + MONSTER_WALK_ACTION_MS;
        monster.move_time = now + monster.kind.move_speed_ms;
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
        let info = monster_info_frame(monster);
        self.sync_monster_view(monster.object_id, &monster.map, location, &info);
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
        monster.attack_time = now + monster.kind.attack_speed_ms;

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

    /// Crystal `MonsterObject.ProcessRegen`（`MonsterObject.cs:1414-1452`）。
    fn process_monster_regen(&mut self, monster: &mut Monster, now: i64) {
        if monster.dead || now < monster.regen_time {
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
        time
    }

    /// 目标格上是否已有别的阻挡对象（怪物 `Walk` 的占格判定，`MonsterObject.cs:2105-2112`）。
    fn monster_cell_free(&self, monster: &Monster, cell: Point) -> bool {
        if self
            .players
            .values()
            .any(|player| player.map == monster.map && !player.dead && player.position == cell)
        {
            return false;
        }
        !self.monsters.iter().any(|other| {
            other.map == monster.map
                && !other.dead
                && other.object_id != monster.object_id
                && other.position == cell
        })
    }

    /// 自己脚下是否还踩着别的对象（Crystal `CheckStacked`）。
    fn monster_cell_stacked(&self, monster: &Monster) -> bool {
        self.players.values().any(|player| {
            player.map == monster.map && !player.dead && player.position == monster.position
        }) || self.monsters.iter().any(|other| {
            other.map == monster.map
                && !other.dead
                && other.object_id != monster.object_id
                && other.position == monster.position
        })
    }

    /// 怪物死亡：Crystal `MonsterObject.Die`（`MonsterObject.cs:966-1006`）。
    ///
    /// 只做「标死 + 广播 `ObjectDied` + 设置尸体到期时间」；
    /// 经验、掉落、`HasDieScript` 与重生留给后续里程碑（`// TODO`）。
    pub(crate) fn monster_died(&mut self, monster_id: u32) {
        let now = self.now();
        let Some(monster) = self.monsters.iter_mut().find(|m| m.object_id == monster_id) else {
            return;
        };
        if monster.dead {
            return;
        }
        monster.dead = true;
        monster.hp = 0;
        monster.dead_time = now + CORPSE_DELAY_MS;
        monster.target = None;
        let map = monster.map.clone();
        let position = monster.position;
        let direction = monster.direction;

        let packet = ServerPacket::ObjectDied(ObjectDied {
            object_id: monster_id,
            location: position,
            direction,
            type_: 0,
        })
        .encode_frame();
        self.broadcast(&map, position, Some(monster_id), &packet, monster_id);
        // TODO: 经验结算（`EXPOwner.WinExp`）与掉落（`Drop()`），等 `mir-db` 的物品/经验表接入。
    }
}
