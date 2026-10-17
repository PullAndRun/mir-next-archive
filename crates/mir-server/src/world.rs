//! 世界：在线玩家、怪物、视野广播、移动/战斗判定、动作计时与主 tick。
//!
//! 规则按 Crystal 的服务器权威逻辑逐条移植（见 `docs/world/movement.md` 与 `docs/world/combat.md`）：
//!
//! * 服务器时钟是「启动以来的单调毫秒数」（Crystal `Envir.Time`）。
//! * 每个对象有 `action_time`：下一次允许开始动作的最早时刻；
//!   玩家走/跑一次动作 **600ms**、转身 **350ms**，与职业、护甲重量无关。
//! * 走一步 `step_counter + 1`；**转身或停顿 700ms 以上清零**，
//!   而「跑」要求 `step_counter > 0`——所以站桩或转身后必须先走一步才能跑。
//! * 移动成功时：本人收到 `UserLocation`，周围玩家收到 `ObjectWalk`/`ObjectRun`；
//!   移动失败时：只给本人回一条 `UserLocation` 把角色拉回服务器坐标。
//! * 玩家的物理攻击是**延迟结算**的：动作包立刻广播，伤害 300ms 后才落地
//!   （Crystal `DelayedAction`，`Server/MirObjects/HumanObject.cs:3201`），
//!   怪物的攻击同理（`Server/MirObjects/MonsterObject.cs:2171`）。
//!
//! # tick 顺序
//!
//! Crystal 没有「先处理怪物还是先处理玩家」的区分：所有对象在同一个 `Envir.Objects`
//! 链表里按插入顺序轮转（`Server/MirEnvir/Envir.cs:2119-2138`）。mir-next 把顺序固定成
//! **玩家簿记 → 到期伤害 → 怪物 AI**：玩家每 tick 都要处理（Crystal `HumanObject.SetOperateTime`
//! 把 `OperateTime` 设成 `Envir.Time`，`HumanObject.cs:392-395`），怪物则受 `operate_time`
//! 节流（最快 2 秒一次，`MonsterObject.cs:1232-1310`），所以先玩家后怪物与 Crystal 的可观测行为一致。

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Instant;

use mir_core::MirRng;
use mir_core::Point;
use mir_core::enums::{
    ChatType, DamageType, DefenceType, MirClass, MirDirection, MirGender, MonsterType, PoisonType,
    Spell, SpellEffect,
};
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::{
    DamageIndicator, Death, HealthChanged, ObjectAttack, ObjectDied, ObjectEffect, ObjectHealth,
    ObjectMonster, ObjectPlayer, ObjectRemove, ObjectRun, ObjectStruck, ObjectTurn, ObjectWalk,
    Struck, UserLocation,
};
use tokio::sync::mpsc::UnboundedSender;

use crate::combat::{self, CombatStats, PendingHit};
use crate::monster::{Monster, MonsterKind};

/// 视野半径。
///
/// 简化：Crystal 的 `Globals.DataRange` 是 **16**（`Shared/Globals.cs:37`），
/// mir-next 沿用既有实现的 10，以免改动已经通过的移动/聊天测试；
/// 怪物自己的索敌视野是 `MonsterKind::view_range`（Crystal `MonsterInfo.ViewRange`，默认 7），
/// 与这个广播半径无关。
pub const VIEW_RANGE: i32 = 10;
/// 聊天广播半径。
pub const CHAT_RANGE: i32 = 20;
/// 走/跑一次动作的耗时（Crystal `HumanObject.MoveDelay`）。
pub const MOVE_DELAY_MS: i64 = 600;
/// 转身耗时（Crystal `Turn` 里的 350ms）。
pub const TURN_DELAY_MS: i64 = 350;
/// 刚移动过的宽限：期间不阻挡别人（Crystal `CellTime = now + 500`）。
pub const CELL_GRACE_MS: i64 = 500;
/// 停顿多久算「站住了」，清零 `step_counter`（Crystal `CellTime + 700 < Time`）。
pub const IDLE_RESET_MS: i64 = 700;
/// 跑步计数上限：超过就扣 1 点血并回退 8（Crystal `_runCounter`）。
pub const RUN_COUNTER_LIMIT: i32 = 10;
/// 跑步计数每 1500ms 自然减 1。
pub const RUN_COUNTER_DECAY_MS: i64 = 1_500;
/// 玩家攻击后的动作锁定：Crystal `HumanObject.Attack` 的 `ActionTime = Envir.Time + 550`
/// （`Server/MirObjects/HumanObject.cs:2975`）。
pub const PLAYER_ATTACK_ACTION_MS: i64 = 550;
/// 玩家攻击到伤害结算的延迟：Crystal `HumanObject.Attack` 的 `DelayedAction(..., now + 300, ...)`
/// （`Server/MirObjects/HumanObject.cs:3201`）。
pub const PLAYER_ATTACK_DAMAGE_DELAY_MS: i64 = 300;
/// 玩家受击动画的节流：Crystal `HumanObject.Attacked(MonsterObject)` 的 `StruckTime = now + 500`
/// （`Server/MirObjects/HumanObject.cs:7304-7309`）。
pub const PLAYER_STRUCK_DELAY_MS: i64 = 500;
/// 怪物 `ObjectHealth` 的 `Expire` 字段：Crystal 取 `max(5, (RevTime - now) / 1000)`，
/// 没有复活计时时恒为 5（`Server/MirObjects/MapObject.cs:845`）。
pub const OBJECT_HEALTH_EXPIRE_SECONDS: u8 = 5;

/// 一个在线玩家。
#[derive(Debug, Clone)]
pub struct Player {
    /// 对象 ID（世界内唯一）。
    pub object_id: u32,
    /// 连接 ID。
    pub session_id: u64,
    /// 角色 ID。
    pub character_id: i64,
    /// 角色名。
    pub name: String,
    /// 地图名。
    pub map: String,
    /// 坐标。
    pub position: Point,
    /// 朝向。
    pub direction: MirDirection,
    /// 等级。
    pub level: u16,
    /// 职业。
    pub class: MirClass,
    /// 性别。
    pub gender: MirGender,
    /// 发型。
    pub hair: u8,
    /// 生命。
    pub hp: i32,
    /// 魔法。
    pub mp: i32,
    /// 战斗属性（等级/职业推出的基础值；装备接入后由刷新流程更新）。
    pub stats: CombatStats,
    /// 下一次可以开始动作的最早时刻（毫秒，服务器单调时钟）。
    pub action_time: i64,
    /// 下一次可以攻击的最早时刻（Crystal `HumanObject.AttackTime`）。
    pub attack_time: i64,
    /// 受击动画节流截止（Crystal `HumanObject.StruckTime`）。
    pub struck_time: i64,
    /// 刚移动过的宽限截止时刻。
    pub cell_time: i64,
    /// 连续行走步数：转身或停顿后清零，跑步要求它大于 0。
    pub step_counter: i32,
    /// 跑步计数（体力）：超过上限会掉血。
    pub run_counter: i32,
    /// 跑步计数下一次自然衰减的时刻。
    pub run_time: i64,
    /// 是否已死亡（`Death` 包已发；重生/幽灵状态见 TODO）。
    pub dead: bool,
    /// 自己**已经看见**的怪物，用于 `ObjectMonster`/`ObjectRemove` 的增量下发。
    pub visible_monsters: BTreeSet<u32>,
    /// 发包通道。
    pub sender: UnboundedSender<Vec<u8>>,
}

impl Player {
    /// 是否可以开始新动作（`CanMove`）。
    #[must_use]
    pub fn can_move(&self, now: i64) -> bool {
        now >= self.action_time
    }

    /// 是否允许跑（`CanRun`）：动作已结束且刚走过至少一步。
    #[must_use]
    pub fn can_run(&self, now: i64) -> bool {
        self.can_move(now) && self.step_counter > 0
    }

    /// 是否可以攻击：Crystal `HumanObject.CanAttack`（`HumanObject.cs:117-159`）去掉毒/坐骑条件。
    #[must_use]
    pub fn can_attack(&self, now: i64) -> bool {
        !self.dead && now >= self.action_time && now >= self.attack_time
    }
}

/// 移动失败原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MoveError {
    /// 对象不在世界里。
    #[error("对象 {0} 不在世界里")]
    NotInWorld(u32),
    /// 对象已死亡，不能行动。
    #[error("对象 {object_id} 已死亡")]
    Dead {
        /// 对象 ID。
        object_id: u32,
    },
    /// 上一个动作还没结束。
    #[error("动作未结束，最早 {until}ms 之后才能动")]
    Busy {
        /// 可以开始下一个动作的时刻。
        until: i64,
    },
    /// 还没走过路，不能跑（Crystal `_stepCounter > 0`）。
    #[error("需要先走一步才能跑")]
    CannotRunYet,
    /// 目标格被占用。
    #[error("目标格 ({x}, {y}) 已被占用")]
    Occupied {
        /// 目标 X。
        x: i32,
        /// 目标 Y。
        y: i32,
    },
    /// 越界。
    #[error("目标格 ({x}, {y}) 越界")]
    OutOfBounds {
        /// 目标 X。
        x: i32,
        /// 目标 Y。
        y: i32,
    },
    /// 地形不可走（墙）。
    #[error("目标格 ({x}, {y}) 是墙")]
    Blocked {
        /// 目标 X。
        x: i32,
        /// 目标 Y。
        y: i32,
    },
}

/// 攻击失败原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CombatError {
    /// 对象不在世界里。
    #[error("对象 {0} 不在世界里")]
    NotInWorld(u32),
    /// 对象已死亡。
    #[error("对象 {object_id} 已死亡，不能攻击")]
    Dead {
        /// 对象 ID。
        object_id: u32,
    },
    /// 动作或攻击冷却还没结束。
    #[error("动作未结束，最早 {until}ms 之后才能攻击")]
    Busy {
        /// 可以攻击的最早时刻。
        until: i64,
    },
}

/// 一次攻击的**即时**结果；伤害在 `PLAYER_ATTACK_DAMAGE_DELAY_MS` 之后才落地。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttackReport {
    /// 被打的目标对象 ID。
    pub target: u32,
    /// 已抽好的原始伤害（扣护甲前，Crystal `GetAttackPower`）。
    pub damage: i32,
}

/// 一组刷怪点：在 `center ± spread` 里随机找空位刷 `count` 只。
///
/// 简化：Crystal 从地图预计算的 `WalkableCells` 里随机取格（`MonsterObject.cs:706-732`），
/// mir-next 直接在半径内撒点并校验可走/未被占，最多重试 16 次。
#[derive(Debug, Clone, PartialEq)]
pub struct SpawnGroup {
    /// 地图名。
    pub map: String,
    /// 怪物种类。
    pub kind: MonsterKind,
    /// 中心点。
    pub center: Point,
    /// 数量。
    pub count: usize,
    /// ± 半径。
    pub spread: i32,
}

/// 世界状态。
#[derive(Debug)]
pub struct World {
    pub(crate) players: HashMap<u32, Player>,
    /// 怪物列表：**按生成顺序**存放，等价于 Crystal `Envir.Objects` 链表的插入顺序。
    pub(crate) monsters: Vec<Monster>,
    /// 已到期但还没结算的延迟伤害（Crystal 每个对象 `ActionList` 里的 `DelayedType.Damage`）。
    pub(crate) pending_hits: Vec<PendingHit>,
    next_object_id: u32,
    pub(crate) rng: MirRng,
    /// 允许的坐标范围（0..=bound）。没有加载真实地图时用它兜底。
    pub bound: i32,
    /// 已加载的地图；有它时行走按真实地形的可走性判定。
    pub(crate) map: Option<Arc<mir_world::Map>>,
    /// 服务器时钟起点（Crystal `Envir.Time` 的 0 点）。
    started: Instant,
    /// 测试用的时钟偏移（毫秒）。生产环境恒为 0，见 [`World::advance_clock`]。
    clock_offset: i64,
}

impl Default for World {
    fn default() -> Self {
        Self::new(999)
    }
}

impl World {
    /// 用坐标上界构造（无地图，仅做边界与占格校验）。
    #[must_use]
    pub fn new(bound: i32) -> Self {
        Self {
            players: HashMap::new(),
            monsters: Vec::new(),
            pending_hits: Vec::new(),
            next_object_id: 1,
            rng: MirRng::from_entropy(),
            bound,
            map: None,
            started: Instant::now(),
            clock_offset: 0,
        }
    }

    /// 用真实地图构造：行走判定会查地图的可走性。
    #[must_use]
    pub fn with_map(map: Arc<mir_world::Map>) -> Self {
        let bound = map.width.max(map.height).saturating_sub(1);
        Self {
            players: HashMap::new(),
            monsters: Vec::new(),
            pending_hits: Vec::new(),
            next_object_id: 1,
            rng: MirRng::from_entropy(),
            bound,
            map: Some(map),
            started: Instant::now(),
            clock_offset: 0,
        }
    }

    /// 当前地图（如果有）。
    #[must_use]
    pub fn map(&self) -> Option<&Arc<mir_world::Map>> {
        self.map.as_ref()
    }

    /// 服务器时钟（毫秒，单调递增）。
    ///
    /// Crystal 的 `Envir.Time` 是 `Stopwatch.ElapsedMilliseconds`（`Server/MirEnvir/Envir.cs:73-75`），
    /// 这里加上 `clock_offset` 只是为了测试能快进时间（见 [`World::advance_clock`]）。
    #[must_use]
    pub fn now(&self) -> i64 {
        self.started.elapsed().as_millis() as i64 + self.clock_offset
    }

    /// 把世界时钟快进 `ms` 毫秒（**仅用于测试**：让 300ms 的伤害延迟、2 秒的怪物调度粒度、
    /// 3 分钟的尸体停留都能被确定性地驱动，不必真的 `sleep`）。
    pub fn advance_clock(&mut self, ms: i64) {
        self.clock_offset += ms;
    }

    /// 用固定种子替换随机数发生器，让测试可回放
    /// （Crystal 的 `Envir.Random` 也是单实例全局随机源，`Envir.cs:100`）。
    pub fn seed_rng(&mut self, seed: u64) {
        self.rng = MirRng::from_seed(seed);
    }

    /// 在线人数。
    #[must_use]
    pub fn len(&self) -> usize {
        self.players.len()
    }

    /// 是否空世界。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.players.is_empty()
    }

    /// 分配一个对象 ID。
    pub fn allocate_object_id(&mut self) -> u32 {
        let id = self.next_object_id;
        self.next_object_id = self.next_object_id.wrapping_add(1).max(1);
        id
    }

    /// 按对象 ID 取玩家。
    #[must_use]
    pub fn player(&self, object_id: u32) -> Option<&Player> {
        self.players.get(&object_id)
    }

    /// 按对象 ID 取玩家（可变）。
    pub fn player_mut(&mut self, object_id: u32) -> Option<&mut Player> {
        self.players.get_mut(&object_id)
    }

    /// 按对象 ID 取怪物。
    #[must_use]
    pub fn monster(&self, object_id: u32) -> Option<&Monster> {
        self.monsters.iter().find(|m| m.object_id == object_id)
    }

    /// 按对象 ID 取怪物（可变）。
    pub fn monster_mut(&mut self, object_id: u32) -> Option<&mut Monster> {
        self.monsters.iter_mut().find(|m| m.object_id == object_id)
    }

    /// 所有怪物（生成顺序）。
    #[must_use]
    pub fn monsters(&self) -> &[Monster] {
        &self.monsters
    }

    /// 把玩家放进世界，并把它的外观广播给同图可见玩家。
    pub fn enter(&mut self, player: Player) {
        let appearance = appearance_packet(&player);
        let now = self.now();

        // 先让新玩家看到周围的人，再把自己的外观发给别人
        // （Crystal 对视野内每个对象调用 `Add(player)` 下发 `GetInfo()`，`MapObject.cs:286-308`）。
        let mut existing = Vec::new();
        for other in self.nearby(player.position, &player.map, VIEW_RANGE, None) {
            existing.push(other.clone());
        }
        for other in &existing {
            let _ = player.sender.send(appearance_packet(other));
        }

        // 视野内的怪物：`ObjectMonster`（Crystal `MonsterObject.GetInfo`）。
        let mut visible_monsters = BTreeSet::new();
        for monster in &self.monsters {
            if monster.map == player.map && monster.position.in_range(player.position, VIEW_RANGE) {
                visible_monsters.insert(monster.object_id);
                let _ = player.sender.send(monster_info_frame(monster));
            }
        }

        for other in self.players.values() {
            if other.map == player.map
                && other.position.in_range(player.position, VIEW_RANGE)
                && other.object_id != player.object_id
            {
                let _ = other.sender.send(appearance.clone());
            }
        }

        let mut player = player;
        player.action_time = now;
        // 刚进世界还没移动过，`cell_time` 保持 0 → 会正常阻挡别人。
        player.cell_time = 0;
        player.visible_monsters = visible_monsters;
        // 让后续 `allocate_object_id` 不会撞上这个玩家的 ID
        // （生产路径是先分配再 `enter`，但测试会直接构造对象 ID）。
        self.next_object_id = self.next_object_id.max(player.object_id.wrapping_add(1));
        self.players.insert(player.object_id, player);
    }

    /// 把玩家移出世界，并通知周围玩家移除。
    pub fn leave(&mut self, object_id: u32) -> Option<Player> {
        let player = self.players.remove(&object_id)?;
        let removal = object_remove_frame(object_id);
        for other in self.players.values() {
            if other.map == player.map && other.position.in_range(player.position, VIEW_RANGE) {
                let _ = other.sender.send(removal.clone());
            }
        }
        Some(player)
    }

    /// 转身。
    ///
    /// 对应 Crystal `PlayerObject.Turn`：`_stepCounter` 先清零（转身后不能立刻跑），
    /// 动作没结束时不改方向，但**无论如何**都回一条 `UserLocation` 给本人。
    pub fn turn(&mut self, object_id: u32, direction: MirDirection) -> Result<(), MoveError> {
        let now = self.now();
        self.refresh_idle(object_id, now);
        let (map, position, can_move, dead) = {
            let player = self
                .players
                .get_mut(&object_id)
                .ok_or(MoveError::NotInWorld(object_id))?;
            player.step_counter = 0;
            (
                player.map.clone(),
                player.position,
                player.can_move(now),
                player.dead,
            )
        };
        if dead {
            self.send_user_location(object_id);
            return Err(MoveError::Dead { object_id });
        }
        if !can_move {
            self.send_user_location(object_id);
            return Err(MoveError::Busy {
                until: self.players[&object_id].action_time,
            });
        }
        if let Some(player) = self.players.get_mut(&object_id) {
            player.direction = direction;
            player.action_time = now + TURN_DELAY_MS;
        }
        let packet = ServerPacket::ObjectTurn(ObjectTurn {
            object_id,
            location: position,
            direction,
        })
        .encode_frame();
        self.broadcast(&map, position, Some(object_id), &packet, object_id);
        self.send_user_location(object_id);
        Ok(())
    }

    /// 走一步。
    ///
    /// 判定顺序与 Crystal `HumanObject.Walk` 一致：动作时间 → 边界/地形 → 占格。
    /// 任何一步失败都只把本人拉回服务器坐标。
    pub fn walk(&mut self, object_id: u32, direction: MirDirection) -> Result<Point, MoveError> {
        let now = self.now();
        self.refresh_idle(object_id, now);
        let Some(player) = self.players.get(&object_id) else {
            return Err(MoveError::NotInWorld(object_id));
        };
        if player.dead {
            self.send_user_location(object_id);
            return Err(MoveError::Dead { object_id });
        }
        if !player.can_move(now) {
            self.send_user_location(object_id);
            return Err(MoveError::Busy {
                until: player.action_time,
            });
        }
        let map = player.map.clone();
        let from = player.position;
        let to = from.adjacent(direction);
        if let Err(error) = self.check_cell(&map, to, object_id, now) {
            self.send_user_location(object_id);
            return Err(error);
        }

        if let Some(player) = self.players.get_mut(&object_id) {
            player.position = to;
            player.direction = direction;
            player.step_counter += 1;
            player.cell_time = now + CELL_GRACE_MS;
            player.action_time = now + MOVE_DELAY_MS;
        }
        let packet = ServerPacket::ObjectWalk(ObjectWalk {
            object_id,
            location: to,
            direction,
        })
        .encode_frame();
        self.broadcast(&map, to, Some(object_id), &packet, object_id);
        self.send_user_location(object_id);
        // 走完之后刷新自己的怪物视野（Crystal `AddObjects/RemoveObjects`，`HumanObject.cs:2500-2510`）。
        self.sync_player_monsters(object_id);
        Ok(to)
    }

    /// 跑（一次动作跨 2 格）。
    ///
    /// Crystal 语义：`step_counter > 0` 才允许跑；**每一格**都要单独校验，
    /// 任一格不通过则整次跑失败、人物留在原地。骑乘与疾风步尚未实现，
    /// 因此固定 2 格（`steps = 2`）。
    pub fn run(&mut self, object_id: u32, direction: MirDirection) -> Result<Point, MoveError> {
        let now = self.now();
        self.refresh_idle(object_id, now);
        let Some(player) = self.players.get(&object_id) else {
            return Err(MoveError::NotInWorld(object_id));
        };
        if player.dead {
            self.send_user_location(object_id);
            return Err(MoveError::Dead { object_id });
        }
        if !player.can_move(now) {
            self.send_user_location(object_id);
            return Err(MoveError::Busy {
                until: player.action_time,
            });
        }
        if !player.can_run(now) {
            self.send_user_location(object_id);
            return Err(MoveError::CannotRunYet);
        }
        let map = player.map.clone();
        let from = player.position;
        let steps = 2;
        let mut destination = from;
        for step in 1..=steps {
            let cell = from.moved(direction, step);
            if let Err(error) = self.check_cell(&map, cell, object_id, now) {
                self.send_user_location(object_id);
                return Err(error);
            }
            destination = cell;
        }

        if let Some(player) = self.players.get_mut(&object_id) {
            player.position = destination;
            player.direction = direction;
            player.cell_time = now + CELL_GRACE_MS;
            player.action_time = now + MOVE_DELAY_MS;
            player.run_counter += 1;
            if player.run_counter > RUN_COUNTER_LIMIT {
                player.run_counter -= 8;
                // Crystal：`_runCounter > 10 → -= 8 且 ChangeHP(-1)`。
                player.hp -= 1;
            }
            if player.run_time == 0 {
                player.run_time = now + RUN_COUNTER_DECAY_MS;
            }
        }
        let packet = ServerPacket::ObjectRun(ObjectRun {
            object_id,
            location: destination,
            direction,
        })
        .encode_frame();
        self.broadcast(&map, destination, Some(object_id), &packet, object_id);
        self.send_user_location(object_id);
        self.sync_player_monsters(object_id);
        Ok(destination)
    }

    /// 玩家物理攻击（Crystal `HumanObject.Attack` 的近战分支，`HumanObject.cs:2848-3204`）。
    ///
    /// 流程：`CanAttack` → 本人 `UserLocation` + 周围 `ObjectAttack` → 计时 →
    /// 抽伤害 → 前方一格取第一个可攻击对象 → 排入 300ms 后的延迟伤害。
    ///
    /// 简化：不处理技能（`Spell`）、幸运/诅咒开关、`Fencing`/`MoonLight` 等加成；
    /// PK 未实现，所以近战**只打怪物**（Crystal 还会按 PK 规则打玩家）。
    ///
    /// # Errors
    /// 对象不在世界里、已死亡或还在动作/攻击冷却里时返回 [`CombatError`]。
    pub fn attack(
        &mut self,
        object_id: u32,
        direction: MirDirection,
    ) -> Result<Option<AttackReport>, CombatError> {
        let now = self.now();
        let Some(player) = self.players.get(&object_id) else {
            return Err(CombatError::NotInWorld(object_id));
        };
        if player.dead {
            return Err(CombatError::Dead { object_id });
        }
        if !player.can_attack(now) {
            return Err(CombatError::Busy {
                until: player.action_time.max(player.attack_time),
            });
        }
        let map = player.map.clone();
        let position = player.position;
        let stats = player.stats;
        let level = player.level;

        // 1) `Enqueue(S.UserLocation)` + `Broadcast(S.ObjectAttack)`（`HumanObject.cs:2971-2972`）。
        self.send_to_player(
            object_id,
            &ServerPacket::UserLocation(UserLocation {
                location: position,
                direction,
            })
            .encode_frame(),
        );
        let packet = ServerPacket::ObjectAttack(ObjectAttack {
            object_id,
            location: position,
            direction,
            spell: Spell::None,
            level: 0,
            type_: 0,
        })
        .encode_frame();
        self.broadcast(&map, position, Some(object_id), &packet, object_id);

        // 2) `AttackTime = now + AttackSpeed`、`ActionTime = now + 550`（`HumanObject.cs:2974-2975`）。
        let attack_delay = combat::player_attack_delay_ms(level, stats.attack_speed);
        if let Some(player) = self.players.get_mut(&object_id) {
            player.attack_time = now + attack_delay;
            player.action_time = now + PLAYER_ATTACK_ACTION_MS;
        }

        // 3) 伤害在这里就抽好（`HumanObject.cs:2981`）：即使前方没有目标也会消耗随机数。
        let damage = stats.physical_attack(&mut self.rng);

        // 4) 前方一格（`HumanObject.cs:2978-3026`）。
        let target_cell = position.adjacent(direction);
        if !self.walkable(target_cell) {
            return Ok(None);
        }
        let Some(target_id) = self.melee_target(&map, target_cell) else {
            return Ok(None);
        };
        self.pending_hits.push(PendingHit {
            at: now + PLAYER_ATTACK_DAMAGE_DELAY_MS,
            attacker_id: object_id,
            target_id,
            damage,
            defence: DefenceType::ACAgility,
        });
        Ok(Some(AttackReport {
            target: target_id,
            damage,
        }))
    }

    /// 前方格子上第一个可攻击的怪物。
    ///
    /// Crystal 遍历 `cell.Objects` 并在第一个可攻击对象上 `break`（`HumanObject.cs:3029-3203`）。
    /// 简化：PK 未实现，这里只挑怪物（按生成顺序取第一个）。
    fn melee_target(&self, map: &str, cell: Point) -> Option<u32> {
        self.monsters
            .iter()
            .find(|monster| monster.map == map && !monster.dead && monster.position == cell)
            .map(|monster| monster.object_id)
    }

    /// 世界 tick：玩家簿记 → 到期伤害结算 → 怪物 AI。
    ///
    /// 由服务端的定时任务每 100ms 调一次；顺序理由见模块文档的「tick 顺序」。
    pub fn tick(&mut self) {
        let now = self.now();
        self.tick_players(now);
        self.resolve_pending_hits(now);
        self.tick_monsters(now);
    }

    /// 玩家侧簿记：跑步计数衰减、停顿后清零 `step_counter`。
    fn tick_players(&mut self, now: i64) {
        for player in self.players.values_mut() {
            if player.run_counter > 0 && now >= player.run_time {
                player.run_counter -= 1;
                player.run_time = now + RUN_COUNTER_DECAY_MS;
            }
            if player.cell_time + IDLE_RESET_MS < now {
                player.step_counter = 0;
            }
        }
    }

    /// 结算到期的延迟伤害（Crystal 每个对象 `ActionList` 里的 `DelayedType.Damage`，
    /// `MapObject.cs:228-233`：`if (Envir.Time < ActionList[i].Time) continue;`）。
    fn resolve_pending_hits(&mut self, now: i64) {
        let mut index = 0;
        while index < self.pending_hits.len() {
            if self.pending_hits[index].at > now {
                index += 1;
                continue;
            }
            let hit = self.pending_hits.remove(index);
            self.apply_pending_hit(hit);
        }
    }

    /// 把一条延迟伤害落到具体对象上。
    fn apply_pending_hit(&mut self, hit: PendingHit) {
        // 攻方可能是玩家或怪物；攻方已消失时丢弃——Crystal 的 `Despawn` 会清空 `ActionList`
        // （`MapObject.cs:370`），延迟伤害自然不会再生效。
        let attacker = self
            .players
            .get(&hit.attacker_id)
            .map(|player| player.stats)
            .or_else(|| self.monster(hit.attacker_id).map(|monster| monster.stats));
        let Some(attacker_stats) = attacker else {
            return;
        };

        if self.monster(hit.target_id).is_some() {
            self.damage_monster(hit, &attacker_stats);
            return;
        }
        // 简化：PK 未实现（玩家不会排入打玩家的延迟伤害），走到这里的必然是「怪物打玩家」。
        self.damage_player(hit, &attacker_stats);
    }

    /// 怪物被打：Crystal `MonsterObject.Attacked(HumanObject)`（`MonsterObject.cs:2569-2684`）。
    fn damage_monster(&mut self, hit: PendingHit, attacker_stats: &CombatStats) {
        let Some((stats, dead, map, position, direction)) = self.monster_snapshot(hit.target_id)
        else {
            return;
        };
        if dead {
            return;
        }
        let result = combat::resolve_player_attack(
            attacker_stats,
            &stats,
            hit.damage,
            hit.defence,
            &mut self.rng,
        );
        if result.miss {
            self.emit_damage_indicator(&map, position, hit.target_id, 0, DamageType::Miss);
            return;
        }

        // 暴击：先广播 `ObjectEffect{Critical}`，再发 `DamageIndicator{Critical, 0}`
        // （`MonsterObject.cs:2594-2599`）。
        if result.critical {
            let effect = ServerPacket::ObjectEffect(ObjectEffect {
                object_id: hit.target_id,
                effect: SpellEffect::Critical,
                ..ObjectEffect::default()
            })
            .encode_frame();
            self.broadcast(&map, position, Some(hit.target_id), &effect, hit.target_id);
            self.emit_damage_indicator(&map, position, hit.target_id, 0, DamageType::Critical);
        }

        // 仇恨与经验归属（`MonsterObject.cs:2571-2574`、`:2627-2633`）。
        if let Some(monster) = self.monster_mut(hit.target_id) {
            if monster.target.is_none() {
                monster.target = Some(hit.attacker_id);
            }
            if monster.expo_owner.is_none() {
                monster.expo_owner = Some(hit.attacker_id);
            }
        }

        let struck = ServerPacket::ObjectStruck(ObjectStruck {
            object_id: hit.target_id,
            attacker_id: hit.attacker_id,
            location: position,
            direction,
        })
        .encode_frame();
        // 攻击者本人也能看到这一条：Crystal 的 `Broadcast` 是从**怪物**的视角发出的，
        // 只排除怪物自己（`actor` 传怪物 ID），所以打怪的玩家会收到 `ObjectStruck`。
        self.broadcast(&map, position, Some(hit.target_id), &struck, hit.target_id);
        // `DamageIndicator` 的数值是 `armour - damage`（负数），Crystal 原样发（`MonsterObject.cs:2680`）。
        self.emit_damage_indicator(
            &map,
            position,
            hit.target_id,
            -result.damage,
            DamageType::Hit,
        );

        let mut died = false;
        let mut percent = 0;
        let mut owner = None;
        if let Some(monster) = self.monster_mut(hit.target_id) {
            monster.hp = (monster.hp - result.damage).max(0);
            died = monster.hp == 0;
            percent = monster.health_percent();
            owner = monster.expo_owner;
        }
        // `BroadcastHealthChange`：怪物只把 `ObjectHealth` 发给 `EXPOwner`
        // （`Server/MirObjects/MapObject.cs:841-914`）。
        if let Some(owner_id) = owner {
            self.send_to_player(
                owner_id,
                &ServerPacket::ObjectHealth(ObjectHealth {
                    object_id: hit.target_id,
                    percent,
                    expire: OBJECT_HEALTH_EXPIRE_SECONDS,
                })
                .encode_frame(),
            );
        }
        if died {
            self.monster_died(hit.target_id);
        }
    }

    /// 玩家被打：Crystal `HumanObject.Attacked(MonsterObject)`（`HumanObject.cs:7222-7315`）。
    fn damage_player(&mut self, hit: PendingHit, attacker_stats: &CombatStats) {
        let now = self.now();
        let Some((stats, map, dead, position, direction)) = self.player_snapshot(hit.target_id)
        else {
            return;
        };
        if dead {
            return;
        }
        let result =
            combat::resolve_monster_attack(attacker_stats, &stats, hit.damage, &mut self.rng);
        if result.miss {
            self.emit_damage_indicator(&map, position, hit.target_id, 0, DamageType::Miss);
            return;
        }

        // 周围的玩家收到 `ObjectStruck`（`HumanObject.cs:7307`）。
        let struck = ServerPacket::ObjectStruck(ObjectStruck {
            object_id: hit.target_id,
            attacker_id: hit.attacker_id,
            location: position,
            direction,
        })
        .encode_frame();
        self.broadcast(
            &map,
            position,
            Some(hit.target_id),
            &struck,
            hit.attacker_id,
        );

        // 本人收到 `S.Struck`，500ms 节流（`HumanObject.cs:7304-7309`）。
        if self
            .players
            .get(&hit.target_id)
            .is_some_and(|player| player.struck_time <= now)
        {
            if let Some(player) = self.players.get_mut(&hit.target_id) {
                player.struck_time = now + PLAYER_STRUCK_DELAY_MS;
                let _ = player.sender.send(
                    ServerPacket::Struck(Struck {
                        attacker_id: hit.attacker_id,
                    })
                    .encode_frame(),
                );
            }
        }

        // `DamageIndicator` 的数值是 `armour - damage`（负数，`HumanObject.cs:7311`）。
        self.emit_damage_indicator(
            &map,
            position,
            hit.target_id,
            -result.damage,
            DamageType::Hit,
        );

        // 扣血并同步 `HealthChanged`（`HumanObject.ChangeHP` → `SendHealthChanged`，
        // `HumanObject.cs:885-900`、`:870-874`）。
        let mut died = false;
        if let Some(player) = self.players.get_mut(&hit.target_id) {
            player.hp = (player.hp - result.damage).max(0);
            died = player.hp == 0;
            let hp = player.hp;
            let mp = player.mp;
            let _ = player
                .sender
                .send(ServerPacket::HealthChanged(HealthChanged { hp, mp }).encode_frame());
        }
        if died {
            self.player_died(hit.target_id);
        }
    }

    /// 玩家死亡：Crystal `PlayerObject.Die`（`PlayerObject.cs:578-667`）的最小实现。
    ///
    /// 只做「标死 + 给自己发 `Death` + 给别人广播 `ObjectDied`」；
    /// 掉落、红名、复活与幽灵状态见 TODO。
    fn player_died(&mut self, player_id: u32) {
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        if player.dead {
            return;
        }
        player.dead = true;
        player.hp = 0;
        let position = player.position;
        let direction = player.direction;
        let map = player.map.clone();
        let _ = player.sender.send(
            ServerPacket::Death(Death {
                location: position,
                direction,
            })
            .encode_frame(),
        );

        let died = ServerPacket::ObjectDied(ObjectDied {
            object_id: player_id,
            location: position,
            direction,
            type_: 0,
        })
        .encode_frame();
        self.broadcast(&map, position, Some(player_id), &died, player_id);
        // TODO: 死亡掉落、红名/PK 点、复活与幽灵状态（`PlayerObject.cs:599-667`）。
    }

    /// 广播一条伤害指示：本人（如果是玩家）自己收一份，再发给周围玩家
    /// （Crystal `MapObject.BroadcastDamageIndicator`，`MapObject.cs:916-926`）。
    pub(crate) fn emit_damage_indicator(
        &mut self,
        map: &str,
        position: Point,
        object_id: u32,
        damage: i32,
        type_: DamageType,
    ) {
        let packet = ServerPacket::DamageIndicator(DamageIndicator {
            damage,
            type_,
            object_id,
        })
        .encode_frame();
        self.send_to_player(object_id, &packet);
        self.broadcast(map, position, Some(object_id), &packet, object_id);
    }

    /// 给某个对象（只有玩家有连接）发一帧。
    pub(crate) fn send_to_player(&self, object_id: u32, frame: &[u8]) {
        if let Some(player) = self.players.get(&object_id) {
            let _ = player.sender.send(frame.to_vec());
        }
    }

    /// 在某张地图上刷一组怪。
    pub fn spawn_group(&mut self, group: &SpawnGroup) -> Vec<u32> {
        let mut spawned = Vec::new();
        for _ in 0..group.count {
            let mut placed = None;
            for _ in 0..16 {
                let dx = self.rng.next_range(-group.spread, group.spread);
                let dy = self.rng.next_range(-group.spread, group.spread);
                let cell = Point::new((group.center.x + dx).max(0), (group.center.y + dy).max(0));
                if !self.walkable(cell) {
                    continue;
                }
                if self.monsters.iter().any(|monster| {
                    monster.map == group.map && !monster.dead && monster.position == cell
                }) {
                    continue;
                }
                placed = Some(cell);
                break;
            }
            let Some(cell) = placed else {
                continue;
            };
            spawned.push(self.spawn_monster(&group.map, group.kind.clone(), cell));
        }
        spawned
    }

    /// 在指定地图上刷若干只怪，返回它们的对象 ID（生成顺序）。
    pub fn spawn_monsters(&mut self, map: &str, spawns: &[(MonsterKind, Point)]) -> Vec<u32> {
        spawns
            .iter()
            .map(|(kind, position)| self.spawn_monster(map, kind.clone(), *position))
            .collect()
    }

    /// 刷一只怪：分配 ID、构造（含 Crystal 的 6 次随机抽签）、广播 `ObjectMonster`。
    pub fn spawn_monster(&mut self, map: &str, kind: MonsterKind, position: Point) -> u32 {
        let now = self.now();
        let object_id = self.allocate_object_id();
        let monster = Monster::new(object_id, Arc::new(kind), map, position, now, &mut self.rng);
        let packet = monster_info_frame(&monster);
        self.monsters.push(monster);
        // Crystal `Spawn()` → `Spawned()` → `BroadcastInfo()`（`MapObject.cs:353`）。
        self.sync_monster_view(object_id, map, position, &packet);
        object_id
    }

    /// 怪物位置/可见性变化后，刷新所有玩家对**这一只**怪的视野。
    ///
    /// 等价于 Crystal 的 `AddObjects/RemoveObjects`（`HumanObject.cs:7864`、`:8152`）：
    /// 新进入视野发 `ObjectMonster`，离开视野发 `ObjectRemove`。
    pub(crate) fn sync_monster_view(
        &mut self,
        object_id: u32,
        map: &str,
        position: Point,
        frame: &[u8],
    ) {
        let removal = object_remove_frame(object_id);
        for player in self.players.values_mut() {
            let visible = player.map == map && player.position.in_range(position, VIEW_RANGE);
            if visible && !player.visible_monsters.contains(&object_id) {
                player.visible_monsters.insert(object_id);
                let _ = player.sender.send(frame.to_vec());
            } else if !visible && player.visible_monsters.remove(&object_id) {
                let _ = player.sender.send(removal.clone());
            }
        }
    }

    /// 怪物消失（尸体到期）：给所有「看见过它」的玩家发 `ObjectRemove`
    /// （Crystal `Despawn` 里的 `Broadcast(S.ObjectRemove)`，`MapObject.cs:363`）。
    pub(crate) fn forget_monster(&mut self, object_id: u32) {
        let removal = object_remove_frame(object_id);
        for player in self.players.values_mut() {
            if player.visible_monsters.remove(&object_id) {
                let _ = player.sender.send(removal.clone());
            }
        }
    }

    /// 玩家移动后刷新自己的怪物视野（先发移除、再发新增，顺序同 Crystal `Walk`）。
    fn sync_player_monsters(&mut self, player_id: u32) {
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        let map = player.map.clone();
        let position = player.position;
        let visible: BTreeSet<u32> = self
            .monsters
            .iter()
            .filter(|monster| monster.map == map && monster.position.in_range(position, VIEW_RANGE))
            .map(|monster| monster.object_id)
            .collect();
        let removals: Vec<u32> = player
            .visible_monsters
            .difference(&visible)
            .copied()
            .collect();
        let adds: Vec<Vec<u8>> = self
            .monsters
            .iter()
            .filter(|monster| {
                visible.contains(&monster.object_id)
                    && !player.visible_monsters.contains(&monster.object_id)
            })
            .map(monster_info_frame)
            .collect();

        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        for object_id in removals {
            let _ = player.sender.send(object_remove_frame(object_id));
        }
        for frame in adds {
            let _ = player.sender.send(frame);
        }
        player.visible_monsters = visible;
    }

    /// 聊天广播，返回实际接收人数（含本人）。
    ///
    /// Crystal 的普通聊天是 `Enqueue(p)` + `Broadcast(p)`：本人与附近玩家都会收到
    /// `ObjectChat`，所以这里先把同一帧发给本人，再广播给其它人。
    pub fn chat(&mut self, object_id: u32, message: &str) -> Result<usize, MoveError> {
        let (map, position) = {
            let player = self
                .players
                .get(&object_id)
                .ok_or(MoveError::NotInWorld(object_id))?;
            (player.map.clone(), player.position)
        };
        let packet = ServerPacket::ObjectChat(mir_protocol::packets::server::ObjectChat {
            object_id,
            text: message.to_owned(),
            type_: ChatType::Normal,
        })
        .encode_frame();
        let mut receivers = 0;
        if let Some(player) = self.players.get(&object_id) {
            if player.sender.send(packet.clone()).is_ok() {
                receivers += 1;
            }
        }
        Ok(receivers + self.broadcast(&map, position, None, &packet, object_id))
    }

    /// 给新入场的玩家随机一个出生点微偏移，避免叠在一起。
    pub fn jitter(&mut self, point: Point, radius: i32) -> Point {
        let dx = self.rng.next_range(-radius, radius);
        let dy = self.rng.next_range(-radius, radius);
        Point::new((point.x + dx).max(0), (point.y + dy).max(0))
    }

    /// 随机数发生器（战斗/掉落会用到）。
    pub fn rng(&mut self) -> &mut MirRng {
        &mut self.rng
    }

    /// 目标格是否在地图内且可走（怪物的 `Walk` 只做这一层判定，`MonsterObject.cs:2101`）。
    pub(crate) fn walkable(&self, cell: Point) -> bool {
        if cell.x < 0 || cell.y < 0 || cell.x > self.bound || cell.y > self.bound {
            return false;
        }
        self.map
            .as_ref()
            .is_none_or(|map| map.walkable(cell.x, cell.y))
    }

    /// 检查目标格是否可进入：边界、地形、占格。
    fn check_cell(&self, map: &str, cell: Point, mover: u32, now: i64) -> Result<(), MoveError> {
        if cell.x < 0 || cell.y < 0 || cell.x > self.bound || cell.y > self.bound {
            return Err(MoveError::OutOfBounds {
                x: cell.x,
                y: cell.y,
            });
        }
        if let Some(map_data) = &self.map {
            if !map_data.walkable(cell.x, cell.y) {
                return Err(MoveError::Blocked {
                    x: cell.x,
                    y: cell.y,
                });
            }
        }
        // Crystal：刚移动过的对象有 500ms 宽限（`CellTime >= Envir.Time` 时不算阻挡）。
        let occupied = self.players.values().any(|other| {
            other.map == map
                && other.position == cell
                && other.object_id != mover
                && other.cell_time < now
        });
        if occupied {
            return Err(MoveError::Occupied {
                x: cell.x,
                y: cell.y,
            });
        }
        // 怪物也是阻挡物（Crystal `MapObject.Blocking`，怪物为 `!Dead`）。
        let blocked_by_monster = self
            .monsters
            .iter()
            .any(|monster| monster.map == map && !monster.dead && monster.position == cell);
        if blocked_by_monster {
            return Err(MoveError::Occupied {
                x: cell.x,
                y: cell.y,
            });
        }
        Ok(())
    }

    /// 停顿检查：`cell_time + 700 < now` 时清零 `step_counter`。
    fn refresh_idle(&mut self, object_id: u32, now: i64) {
        if let Some(player) = self.players.get_mut(&object_id) {
            if player.cell_time + IDLE_RESET_MS < now {
                player.step_counter = 0;
            }
        }
    }

    /// 给本人发一条坐标回显（成功与失败都发，与 Crystal 一致）。
    fn send_user_location(&self, object_id: u32) {
        if let Some(player) = self.players.get(&object_id) {
            let packet = ServerPacket::UserLocation(UserLocation {
                location: player.position,
                direction: player.direction,
            })
            .encode_frame();
            let _ = player.sender.send(packet);
        }
    }

    fn nearby(&self, position: Point, map: &str, range: i32, exclude: Option<u32>) -> Vec<&Player> {
        self.players
            .values()
            .filter(|other| {
                other.map == map
                    && other.position.in_range(position, range)
                    && exclude != Some(other.object_id)
            })
            .collect()
    }

    /// 同图、视野内的**其它**玩家接收 `packet`（Crystal `Broadcast` 不发给本人）。
    pub(crate) fn broadcast(
        &self,
        map: &str,
        position: Point,
        exclude: Option<u32>,
        packet: &[u8],
        actor: u32,
    ) -> usize {
        let mut sent = 0;
        for other in self.players.values() {
            if other.map != map || !other.position.in_range(position, VIEW_RANGE) {
                continue;
            }
            if exclude == Some(other.object_id) || other.object_id == actor {
                continue;
            }
            if other.sender.send(packet.to_vec()).is_ok() {
                sent += 1;
            }
        }
        sent
    }

    /// 取一份玩家快照，避免在结算过程中长时间占用 `&mut self`。
    fn player_snapshot(
        &self,
        object_id: u32,
    ) -> Option<(CombatStats, String, bool, Point, MirDirection)> {
        self.players.get(&object_id).map(|player| {
            (
                player.stats,
                player.map.clone(),
                player.dead,
                player.position,
                player.direction,
            )
        })
    }

    /// 取一份怪物快照，理由同 [`World::player_snapshot`]。
    fn monster_snapshot(
        &self,
        object_id: u32,
    ) -> Option<(CombatStats, bool, String, Point, MirDirection)> {
        self.monster(object_id).map(|monster| {
            (
                monster.stats,
                monster.dead,
                monster.map.clone(),
                monster.position,
                monster.direction,
            )
        })
    }
}

fn appearance_packet(player: &Player) -> Vec<u8> {
    ServerPacket::ObjectPlayer(ObjectPlayer {
        object_id: player.object_id,
        name: player.name.clone(),
        class: player.class,
        gender: player.gender,
        level: player.level,
        location: player.position,
        direction: player.direction,
        hair: player.hair,
        light: 0,
        dead: player.dead,
        hidden: false,
        extra: false,
        riding_mount: false,
        fishing: false,
        level_effects: mir_core::enums::LevelEffects::None,
        ..ObjectPlayer::default()
    })
    .encode_frame()
}

/// 怪物外观包（Crystal `MonsterObject.GetInfo`，对应 `S.ObjectMonster`）。
pub(crate) fn monster_info_frame(monster: &Monster) -> Vec<u8> {
    ServerPacket::ObjectMonster(ObjectMonster {
        object_id: monster.object_id,
        name: monster.kind.name.clone(),
        // Crystal `MapObject.NameColour = Color.White`（`MapObject.cs:67`）。
        name_colour: mir_core::Color(-1),
        location: monster.position,
        image: monster.kind.image,
        direction: monster.direction,
        effect: 0,
        ai: monster.kind.ai,
        light: 0,
        dead: monster.dead,
        skeleton: false,
        poison: PoisonType::None,
        hidden: false,
        extra: false,
        extra_byte: 0,
        shock_time: 0,
        binding_shot_center: false,
        master_object_id: 0,
        rarity: MonsterType::Normal,
        buffs: Vec::new(),
    })
    .encode_frame()
}

/// `ObjectRemove` 帧（怪物消失、玩家下线都用它）。
fn object_remove_frame(object_id: u32) -> Vec<u8> {
    ServerPacket::ObjectRemove(ObjectRemove { object_id }).encode_frame()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir_protocol::ServerPacket;
    use tokio::sync::mpsc;

    fn player(
        object_id: u32,
        name: &str,
        position: Point,
        map: &str,
    ) -> (Player, mpsc::UnboundedReceiver<Vec<u8>>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (
            Player {
                object_id,
                session_id: u64::from(object_id),
                character_id: i64::from(object_id),
                name: name.to_owned(),
                map: map.to_owned(),
                position,
                direction: MirDirection::Down,
                level: 1,
                class: MirClass::Warrior,
                gender: MirGender::Male,
                hair: 1,
                hp: 15,
                mp: 0,
                stats: combat::player_base_stats(1, MirClass::Warrior),
                action_time: 0,
                attack_time: 0,
                struck_time: 0,
                cell_time: 0,
                step_counter: 0,
                run_counter: 0,
                run_time: 0,
                dead: false,
                visible_monsters: BTreeSet::new(),
                sender,
            },
            receiver,
        )
    }

    /// 把 `Vec<u8>` 解成 `ServerPacket`，方便断言。
    fn decode(frame: &[u8]) -> ServerPacket {
        let index = i16::from_le_bytes([frame[2], frame[3]]);
        ServerPacket::decode(index, &frame[4..]).expect("解码服务端包")
    }

    fn drain(receiver: &mut mpsc::UnboundedReceiver<Vec<u8>>) -> Vec<ServerPacket> {
        let mut packets = Vec::new();
        while let Ok(frame) = receiver.try_recv() {
            packets.push(decode(&frame));
        }
        packets
    }

    #[test]
    fn entering_world_notifies_both_directions() {
        let mut world = World::new(999);
        let (first, mut first_rx) = player(1, "甲", Point::new(10, 10), "0");
        world.enter(first);
        let (second, mut second_rx) = player(2, "乙", Point::new(11, 10), "0");
        world.enter(second);

        assert!(first_rx.try_recv().is_ok(), "先入场的人应该看到后来者");
        assert!(second_rx.try_recv().is_ok(), "后入场的人应该看到先入场的人");
    }

    #[test]
    fn walking_moves_and_sends_user_location_and_object_walk() {
        let mut world = World::new(999);
        let (walker, mut walker_rx) = player(1, "甲", Point::new(10, 10), "0");
        world.enter(walker);
        let (observer, mut observer_rx) = player(2, "乙", Point::new(12, 10), "0");
        world.enter(observer);
        drain(&mut walker_rx);
        drain(&mut observer_rx);

        let position = world.walk(1, MirDirection::Right).expect("走路");
        assert_eq!(position, Point::new(11, 10));
        assert_eq!(world.player(1).unwrap().position, Point::new(11, 10));

        // 本人收到 UserLocation（新坐标），旁观者收到 ObjectWalk。
        match decode(&walker_rx.try_recv().expect("本人应收 UserLocation")) {
            ServerPacket::UserLocation(location) => {
                assert_eq!(location.location, Point::new(11, 10));
                assert_eq!(location.direction, MirDirection::Right);
            }
            other => panic!("期望 UserLocation，收到 {other:?}"),
        }
        match decode(&observer_rx.try_recv().expect("旁观者应收 ObjectWalk")) {
            ServerPacket::ObjectWalk(walk) => {
                assert_eq!(walk.object_id, 1);
                assert_eq!(walk.location, Point::new(11, 10));
                assert_eq!(walk.direction, MirDirection::Right);
            }
            other => panic!("期望 ObjectWalk，收到 {other:?}"),
        }
    }

    #[test]
    fn cannot_walk_onto_occupied_cell() {
        let mut world = World::new(999);
        let (first, _rx1) = player(1, "甲", Point::new(10, 10), "0");
        world.enter(first);
        let (second, _rx2) = player(2, "乙", Point::new(11, 10), "0");
        world.enter(second);
        // 服务器时钟是毫秒级：等 2ms，让 `cell_time < now` 成立（刚进世界时 cell_time=0）。
        std::thread::sleep(std::time::Duration::from_millis(2));

        assert_eq!(
            world.walk(1, MirDirection::Right),
            Err(MoveError::Occupied { x: 11, y: 10 })
        );
    }

    #[test]
    fn cannot_walk_out_of_bounds() {
        let mut world = World::new(5);
        let (walker, _rx) = player(1, "甲", Point::new(5, 5), "0");
        world.enter(walker);
        assert_eq!(
            world.walk(1, MirDirection::Right),
            Err(MoveError::OutOfBounds { x: 6, y: 5 })
        );
    }

    #[test]
    fn walk_then_run_is_allowed_but_run_from_standstill_is_not() {
        let mut world = World::new(999);
        let (walker, mut rx) = player(1, "甲", Point::new(10, 10), "0");
        world.enter(walker);
        drain(&mut rx);

        // 站桩状态：不能跑。
        assert_eq!(
            world.run(1, MirDirection::Right),
            Err(MoveError::CannotRunYet)
        );

        // 走一步之后可以跑（时间要等到 ActionTime 之后）。
        world.walk(1, MirDirection::Right).expect("走一步");
        let now = world.now();
        if let Some(player) = world.players.get_mut(&1) {
            player.action_time = now; // 模拟 600ms 已过
        }
        let destination = world.run(1, MirDirection::Right).expect("跑两格");
        assert_eq!(destination, Point::new(13, 10));
    }

    #[test]
    fn turn_resets_step_counter_so_run_needs_a_fresh_walk() {
        let mut world = World::new(999);
        let (walker, mut rx) = player(1, "甲", Point::new(10, 10), "0");
        world.enter(walker);
        drain(&mut rx);
        world.walk(1, MirDirection::Right).expect("走一步");
        assert_eq!(world.player(1).unwrap().step_counter, 1);
        let now = world.now();
        if let Some(player) = world.players.get_mut(&1) {
            player.action_time = now;
        }

        world.turn(1, MirDirection::Up).expect("转身");
        assert_eq!(world.player(1).unwrap().step_counter, 0, "转身清零");
        assert_eq!(world.player(1).unwrap().direction, MirDirection::Up);

        let now = world.now();
        if let Some(player) = world.players.get_mut(&1) {
            player.action_time = now;
        }
        assert_eq!(
            world.run(1, MirDirection::Up),
            Err(MoveError::CannotRunYet),
            "转身后不能立刻跑"
        );
    }

    #[test]
    fn action_time_enforces_move_delay() {
        let mut world = World::new(999);
        let (walker, mut rx) = player(1, "甲", Point::new(10, 10), "0");
        world.enter(walker);
        drain(&mut rx);

        world.walk(1, MirDirection::Right).expect("第一次走");
        // 立刻再走：动作未结束，应该被拒绝并且收到回拉包。
        match world.walk(1, MirDirection::Right) {
            Err(MoveError::Busy { .. }) => {}
            other => panic!("期望 Busy，收到 {other:?}"),
        }
        let packets = drain(&mut rx);
        assert!(
            packets
                .iter()
                .any(|packet| matches!(packet, ServerPacket::UserLocation(_))),
            "被拒绝时也应给本人回 UserLocation"
        );
        assert_eq!(
            world.player(1).unwrap().position,
            Point::new(11, 10),
            "被拒绝不应移动"
        );
    }

    #[test]
    fn run_stamina_penalty_costs_hp_after_ten_runs() {
        let mut world = World::new(999);
        let (walker, _rx) = player(1, "甲", Point::new(10, 10), "0");
        world.enter(walker);
        // 直接置成「刚走过一步」，然后连续向下跑 11 次（每次 2 格，远未到边界）。
        if let Some(player) = world.players.get_mut(&1) {
            player.step_counter = 1;
        }
        for _ in 0..11 {
            let now = world.now();
            if let Some(player) = world.players.get_mut(&1) {
                player.action_time = now;
            }
            world.run(1, MirDirection::Down).expect("跑");
        }
        assert_eq!(world.player(1).unwrap().position, Point::new(10, 32));
        assert!(
            world.player(1).unwrap().hp < 15,
            "跑满 10 次以上应扣血，实际 HP = {}",
            world.player(1).unwrap().hp
        );
        assert_eq!(world.player(1).unwrap().run_counter, 3, "11 - 8 = 3");
    }

    #[test]
    fn tick_decays_run_counter_and_clears_steps_when_idle() {
        let mut world = World::new(999);
        let (mut walker, _rx) = player(1, "甲", Point::new(10, 10), "0");
        walker.step_counter = 3;
        walker.run_counter = 2;
        walker.run_time = 0;
        world.enter(walker);
        // 模拟「上次移动已经是 1 秒前」：`enter` 会把 cell_time 归零，这里直接调。
        let now = world.now();
        if let Some(player) = world.players.get_mut(&1) {
            player.step_counter = 3;
            player.run_counter = 2;
            player.run_time = 0;
            player.cell_time = now - 1_000;
        }
        world.tick();
        assert_eq!(world.player(1).unwrap().step_counter, 0, "停顿后清零");
        assert_eq!(world.player(1).unwrap().run_counter, 1, "跑步计数衰减 1");
    }

    #[test]
    fn chat_reaches_people_in_range_only() {
        let mut world = World::new(999);
        let (speaker, mut speaker_rx) = player(1, "甲", Point::new(10, 10), "0");
        world.enter(speaker);
        let (near, mut near_rx) = player(2, "乙", Point::new(12, 10), "0");
        world.enter(near);
        let (far, mut far_rx) = player(3, "丙", Point::new(90, 90), "0");
        world.enter(far);
        drain(&mut speaker_rx);
        drain(&mut near_rx);
        drain(&mut far_rx);

        let receivers = world.chat(1, "你好").expect("聊天");
        assert_eq!(receivers, 2, "自己与近处的人各收到一条");
        assert!(near_rx.try_recv().is_ok());
        assert!(far_rx.try_recv().is_err(), "远处的人不该收到");
    }

    #[test]
    fn leaving_notifies_observers() {
        let mut world = World::new(999);
        let (a, _rx1) = player(1, "甲", Point::new(10, 10), "0");
        world.enter(a);
        let (b, mut b_rx) = player(2, "乙", Point::new(11, 10), "0");
        world.enter(b);
        drain(&mut b_rx);
        assert!(world.leave(1).is_some());
        assert!(b_rx.try_recv().is_ok(), "乙应收到 ObjectRemove");
        assert_eq!(world.len(), 1);
    }

    #[test]
    fn spawning_monster_broadcasts_object_monster_to_players_in_range() {
        let mut world = World::new(999);
        world.seed_rng(1);
        let (near, mut near_rx) = player(1, "甲", Point::new(20, 20), "0");
        world.enter(near);
        let (far, mut far_rx) = player(2, "乙", Point::new(90, 90), "0");
        world.enter(far);
        drain(&mut near_rx);
        drain(&mut far_rx);

        let id = world.spawn_monster("0", MonsterKind::skeleton(), Point::new(22, 22));
        assert!(world.monster(id).is_some());
        match decode(&near_rx.try_recv().expect("近处玩家应收到 ObjectMonster")) {
            ServerPacket::ObjectMonster(monster) => {
                assert_eq!(monster.object_id, id);
                assert_eq!(monster.name, "骷髅");
                assert_eq!(monster.location, Point::new(22, 22));
                assert!(!monster.dead);
            }
            other => panic!("期望 ObjectMonster，收到 {other:?}"),
        }
        assert!(far_rx.try_recv().is_err(), "远处玩家不该收到");
    }
}
