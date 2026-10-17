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
//! * **技能（魔法）**由 [`crate::magic`] 实现：施法时的 `ActionTime`/`SpellTime` 锁、
//!   延迟效果队列与治疗分段都在那边，本模块只负责 `Player` 上的字段与 tick 调度。
//!
//! # tick 顺序
//!
//! Crystal 没有「先处理怪物还是先处理玩家」的区分：所有对象在同一个 `Envir.Objects`
//! 链表里按插入顺序轮转（`Server/MirEnvir/Envir.cs:2119-2138`）。mir-next 把顺序固定成
//! **玩家簿记 → 到期伤害 → 到期魔法 → 刷怪点 → 怪物 AI**：玩家每 tick 都要处理（Crystal `HumanObject.SetOperateTime`
//! 把 `OperateTime` 设成 `Envir.Time`，`HumanObject.cs:392-395`），怪物则受 `operate_time`
//! 节流（最快 2 秒一次，`MonsterObject.cs:1232-1310`），所以先玩家后怪物与 Crystal 的可观测行为一致；
//! 刷怪点放在「到期伤害」之后，是为了让本 tick 结算出来的死亡数立刻反映到刷怪组的存活计数上。
//!
//! # 本模块里的四个「生命周期」系统
//!
//! | 系统 | 入口 | Crystal |
//! |---|---|---|
//! | 刷怪点重生 | [`World::register_spawn_group`]、`tick_respawns` | `MapRespawn`（`Server/MirEnvir/Map.cs:2532-2591`）+ `Map.ProcessRespawns`（`:740-790`）+ `RespawnTimer`（`Server/MirEnvir/RespawnTimer.cs:51-124`） |
//! | 经验/升级 | [`World::win_experience`] | `PlayerObject.WinExp`/`GainExp`/`LevelUp`（`Server/MirObjects/PlayerObject.cs:794-956`） |
//! | 死亡/复活 | [`World::revive_player`] | `PlayerObject.Die`（`:578-667`）+ `HumanObject.Revive`（`HumanObject.cs:1690-1721`） |
//! | 回血回蓝 | `process_player_regen` | `HumanObject.ProcessRegen`（`HumanObject.cs:550-649`） |
//!
//! 每个常量的出处见 `docs/world/combat.md` 的常量表。

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
use mir_protocol::data::UserItem;
use mir_protocol::packets::server::{
    DamageIndicator, Death, GainExperience, HealthChanged, LevelChanged, ObjectAttack, ObjectDied,
    ObjectEffect, ObjectHealth, ObjectLeveled, ObjectMonster, ObjectPlayer, ObjectRemove,
    ObjectRevived, ObjectRun, ObjectStruck, ObjectTurn, ObjectWalk, Revived, Struck, UserLocation,
};
use tokio::sync::mpsc::UnboundedSender;

use crate::buffs::{ActiveBuff, BuffKind, MAGIC_SHIELD_MS_PER_DAMAGE};
use crate::combat::{self, CombatStats, PendingHit};
use crate::exp::ExpTable;
use crate::gamedata::GameData;
use crate::items::{GroundItem, ItemUidGenerator};
use crate::magic::{MagicState, PendingMagic, UserMagic};
use crate::monster::{Monster, MonsterKind};
use crate::npc::{Npc, NpcState};

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
/// 一「分钟」的毫秒数：Crystal `Settings.Minute = 60 * Second`，`Second = 1000`
/// （`Server/Settings.cs:16`）。刷怪延迟用它换算（`Map.cs:760`）。
pub const MINUTE_MS: i64 = 60_000;
/// 玩家自然回血/回蓝的间隔：Crystal `HumanObject.RegenDelay = 10000`
/// （`Server/MirObjects/HumanObject.cs:186`）。
pub const PLAYER_REGEN_DELAY_MS: i64 = 10_000;
/// 每次自然回复的生命/魔法比例：Crystal `(int)(Stats[Stat.HP] * 0.03F) + 1`
/// （`Server/MirObjects/HumanObject.cs:562`，魔法同式见 `:568`）。
pub const PLAYER_REGEN_RATE: f64 = 0.03;
/// 药水回复池的分批间隔：Crystal `HumanObject.PotDelay = 200`（`HumanObject.cs:186`）。
pub const PLAYER_POT_DELAY_MS: i64 = 200;
/// 药水每批放出多少的基数：`int PerTickRegen = 5 + (Level / 10);`（`HumanObject.cs:577`）。
pub const POT_REGEN_BASE: i32 = 5;
/// 死亡后自动复活的等待时间（毫秒）。
///
/// Crystal **没有**服务端复活计时器：死亡后由客户端在死亡动画结束（`DeadTime`）时弹窗，
/// 玩家点「是」才发 `C.TownRevive`（`Client/MirScenes/GameScene.cs:1269-1281`
/// → `Server/MirNetwork/MirConnection.cs:1643-1648`）。mir-next 在服务端计时自动
/// **原地**复活（等价 `HumanObject.Revive`，`HumanObject.cs:1690-1721`），
/// 时长取客户端「死亡动画 + 弹窗」的量级 10 秒。
pub const PLAYER_REVIVE_DELAY_MS: i64 = 10_000;
/// 刷怪失败（找不到空位）后的重试冷却：Crystal `respawn.RespawnTime = Envir.Time + 1 * Settings.Minute`
/// （`Server/MirEnvir/Map.cs:770`）。
pub const RESPAWN_FAIL_DELAY_MS: i64 = MINUTE_MS;
/// 刷怪点连续失败多少次后写一条日志：Crystal 在 `ErrorCount` 到 6 时打日志
/// （`Server/MirEnvir/Map.cs:771-785`）。
pub const RESPAWN_ERROR_LOG_THRESHOLD: u8 = 6;

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
    /// 当前等级内已积累的经验（Crystal `PlayerObject.Experience`，升级时扣掉本级需求）。
    pub experience: i64,
    /// 升到下一级所需经验（Crystal `PlayerObject.MaxExperience`，
    /// `PlayerObject.cs:9627-9630`；由 [`World::set_exp_table`] 的表算出）。
    pub max_experience: i64,
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
    /// 下一次自然回血/回蓝的时刻（Crystal `HumanObject.RegenTime`，
    /// `HumanObject.cs:145-151` 的 `CanRegen`）。
    pub regen_time: i64,
    /// 死亡时刻（0 = 从未死亡）。
    pub death_time: i64,
    /// 到点自动复活的时刻（0 = 未安排复活）。
    pub revive_time: i64,
    /// 刚移动过的宽限截止时刻。
    pub cell_time: i64,
    /// 连续行走步数：转身或停顿后清零，跑步要求它大于 0。
    pub step_counter: i32,
    /// 跑步计数（体力）：超过上限会掉血。
    pub run_counter: i32,
    /// 跑步计数下一次自然衰减的时刻。
    pub run_time: i64,
    /// 是否已死亡（`Death` 包已发，等 [`World::revive_player`] 复活）。
    pub dead: bool,
    /// 自己**已经看见**的怪物，用于 `ObjectMonster`/`ObjectRemove` 的增量下发。
    pub visible_monsters: BTreeSet<u32>,
    /// 背包（Crystal `CharacterInfo.Inventory`，46 格；见 [`crate::items::INVENTORY_SIZE`]）。
    pub inventory: Vec<Option<UserItem>>,
    /// 装备（Crystal `CharacterInfo.Equipment`，14 槽；见 [`crate::items::EQUIPMENT_SLOTS`]）。
    pub equipment: Vec<Option<UserItem>>,
    /// 已学技能（Crystal `CharacterInfo.Magics`，见 [`crate::magic`]）。
    pub magics: Vec<UserMagic>,
    /// 施法相关的计时状态（`SpellTime`、每个技能的 `CastTime`、待生效治疗量）。
    pub magic: MagicState,
    /// 金币（Crystal `AccountInfo.Gold`；本作存在角色行的 `gold` 列上）。
    pub gold: i64,
    /// 元宝/点数（Crystal `AccountInfo.Credit`；同样挂在角色行上）。
    ///
    /// 任务奖励会走 [`World::gain_credit`]（`S.GainedCredit`），除此之外没有别的获取途径。
    pub credit: i64,
    /// 自己**已经看见**的地面物品，用于 `ObjectItem`/`ObjectGold`/`ObjectRemove` 的增量下发。
    pub visible_items: BTreeSet<u32>,
    /// 任务进度（Crystal `CharacterInfo.CurrentQuests`，`CharacterInfo.cs:91`；见 [`crate::quests`]）。
    ///
    /// 进行中与已完成的任务都在这里：完成的任务保留一条 `state = Completed` 的记录
    /// （Crystal 交付后会把进度从 `CurrentQuests` 删掉并塞进 `CompletedQuests`，
    /// 本作保留一条已完成记录，同时维护 [`Player::completed_quests`]）。
    pub quests: Vec<crate::quests::QuestProgress>,
    /// 已完成的任务编号（Crystal `CharacterInfo.CompletedQuests`，`CharacterInfo.cs:92`）。
    ///
    /// 可重复任务（`QuestType.Repeatable`）**不**计入这里（Crystal `PlayerObject.cs:11428-11432`）。
    pub completed_quests: BTreeSet<i32>,
    /// 生效中的增益（Crystal `MapObject.Buffs`，见 [`crate::buffs`]）。
    ///
    /// Crystal 把 `Buffs` 放在 `MapObject` 上（`Server/MirObjects/MapObject.cs:49`），
    /// 本作只有玩家能带 buff（怪物 buff 还没接），所以就放在这里。
    /// **不持久化**：Crystal 的 `CharacterInfo` 存档里没有 buff 块，下线即清。
    pub buffs: Vec<ActiveBuff>,
    /// 药水回复池：普通药水喝下去先攒在这里，再按 [`PLAYER_POT_DELAY_MS`] 分批放出来。
    ///
    /// Crystal `MapObject.PotHealthAmount` / `PotManaAmount`（`MapObject.cs:75`，`ushort`）。
    pub pot_health_amount: u16,
    /// 见 [`Player::pot_health_amount`]。
    pub pot_mana_amount: u16,
    /// 下一次放出药水回复的时刻：Crystal `HumanObject.PotTime`（`HumanObject.cs:187`）。
    pub pot_time: i64,
    /// 是否已解锁「卸下诅咒装备」：Crystal `HumanObject.UnlockCurse`
    /// （神秘药水 `PlayerObject.cs:5839-5848`）。本作没有诅咒装备，标记只用于提示去重。
    pub unlock_curse: bool,
    /// NPC 交互状态：当前对话的 NPC、页面、按钮白名单、标记位
    /// （Crystal 上是 `PlayerObject.NPCObjectID` / `NPCPage` / `NPCSuccess` / `Info.Flags` 几处分散的字段，
    /// 见 [`NpcState`]）。
    pub npc: NpcState,
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
/// 对应 Crystal `RespawnInfo`（`Server/MirDatabase/RespawnInfo.cs:6-52`）里影响刷怪节奏的字段，
/// 每个刷怪组在 [`World`] 里有一份运行状态（见 [`RespawnState`]）。
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
    /// 数量（Crystal `RespawnInfo.Count`）。
    pub count: usize,
    /// ± 半径。
    pub spread: i32,
    /// 刷新间隔（分钟，Crystal `RespawnInfo.Delay`）。
    pub delay_minutes: u16,
    /// 随机延迟（分钟，Crystal `RespawnInfo.RandomDelay`）：
    /// 实际间隔 = `delay - random + Random(0 .. 2*random)` 分钟（`Map.cs:759-760`）。
    pub random_delay_minutes: u16,
}

impl SpawnGroup {
    /// 构造一组刷怪点（默认间隔 1 分钟、无随机延迟）。
    #[must_use]
    pub fn new(
        map: impl Into<String>,
        kind: MonsterKind,
        center: Point,
        count: usize,
        spread: i32,
    ) -> Self {
        Self {
            map: map.into(),
            kind,
            center,
            count,
            spread,
            delay_minutes: 1,
            random_delay_minutes: 0,
        }
    }

    /// 设置刷新节奏（分钟）。
    #[must_use]
    pub fn with_delay(mut self, delay_minutes: u16, random_delay_minutes: u16) -> Self {
        self.delay_minutes = delay_minutes;
        self.random_delay_minutes = random_delay_minutes;
        self
    }
}

/// 刷怪组的**只读运行状态**快照（给测试、GM 命令与运维观察用）。
///
/// 字段一一对应 Crystal `MapRespawn` 的运行期成员（`Server/MirEnvir/Map.cs:2539-2545`）：
/// `Count` → [`Self::alive`]，`RespawnTime` → [`Self::next_spawn_at`]。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RespawnState {
    /// 地图名。
    pub map: String,
    /// 怪物名（Crystal `MapRespawn.Monster.Name`）。
    pub monster: String,
    /// 刷怪中心。
    pub center: Point,
    /// 目标数量（Crystal `Info.Count * SpawnMultiplier`，本作倍率恒为 1）。
    pub count: usize,
    /// 当前存活数量（Crystal `MapRespawn.Count`）。
    pub alive: usize,
    /// 下一次补怪的绝对时刻（毫秒）；`None` 表示不缺怪、没有排程。
    pub next_spawn_at: Option<i64>,
}

/// 刷怪组的运行状态（[`RespawnState`] 的内部版本，带模板与失败计数）。
#[derive(Debug, Clone)]
struct RespawnGroupState {
    /// 模板。
    group: SpawnGroup,
    /// 当前存活数量（Crystal `MapRespawn.Count`，死亡时 `--`、刷出时 `++`）。
    alive: usize,
    /// 下一次补怪的绝对时刻；`0` 表示没有排程。
    next_spawn_at: i64,
    /// 连续失败次数（Crystal `MapRespawn.ErrorCount`）。
    error_count: u8,
}

/// 世界状态。
#[derive(Debug)]
pub struct World {
    pub(crate) players: HashMap<u32, Player>,
    /// 怪物列表：**按生成顺序**存放，等价于 Crystal `Envir.Objects` 链表的插入顺序。
    pub(crate) monsters: Vec<Monster>,
    /// NPC 列表（Crystal `Envir.NPCs`，`Server/MirEnvir/Envir.cs:146`）。
    ///
    /// 与怪物不同，NPC 是建图时一次性刷出的静态对象（`Map.cs:490-496`），
    /// 本作不参与 tick，因此单独放一个列表。
    pub(crate) npcs: Vec<Npc>,
    /// 已到期但还没结算的延迟伤害（Crystal 每个对象 `ActionList` 里的 `DelayedType.Damage`）。
    pub(crate) pending_hits: Vec<PendingHit>,
    /// 已到期但还没结算的延迟魔法效果（Crystal `DelayedType.Magic`，见 [`crate::magic`]）。
    pub(crate) pending_magic: Vec<PendingMagic>,
    /// 刷怪组运行状态（Crystal 每张 `Map` 的 `Respawns` 列表，`Server/MirEnvir/Map.cs:473-488`）。
    respawn_groups: Vec<RespawnGroupState>,
    /// 地面物品（Crystal `Envir.Objects` 里 `ObjectType.Item` 的那部分，见 `crate::items`）。
    pub(crate) ground_items: Vec<GroundItem>,
    /// 游戏数据（物品模板 + 掉落表）；`None` 时物品系统降级成「只能搬运已有物品」。
    pub(crate) game_data: Option<Arc<GameData>>,
    /// 地面物品的存活时间（毫秒），见 [`crate::items::ITEM_EXPIRE_MS`]。
    pub(crate) item_expire_ms: i64,
    /// 拾取时是否检查背包负重（**本作扩展**，Crystal 只在奔跑时查重量）。
    pub(crate) enforce_bag_weight: bool,
    /// 物品 UUID 发生器（Crystal `Envir.NextUserItemID`，见 [`ItemUidGenerator`]）。
    pub(crate) item_uids: ItemUidGenerator,
    /// 等级经验表（Crystal `Settings.ExperienceList`，见 [`crate::exp`]）。
    exp: ExpTable,
    /// Crystal `Envir.RespawnTick` 的全局节奏倍率：`1.0` 等于 Crystal 默认
    /// （`RespawnTimer.BaseSpawnRate = 20` 分钟一个 tick，`CurrentDelay = 基准 × bonus`，
    /// `Server/MirEnvir/RespawnTimer.cs:112-123`）。
    respawn_tick_scale: f64,
    next_object_id: u32,
    pub(crate) rng: MirRng,
    /// 允许的坐标范围（0..=bound）。没有加载真实地图时用它兜底。
    pub bound: i32,
    /// 已加载的地图；有它时行走按真实地形的可走性判定。
    pub(crate) map: Option<Arc<mir_world::Map>>,
    /// 服务器时钟起点（Crystal `Envir.Time` 的 0 点）。
    started: Instant,
    /// 世界创建时刻的 Unix 毫秒（[`World::wall_now`] 的基准）。
    started_unix_ms: i64,
    /// 测试用的时钟偏移（毫秒）。生产环境恒为 0，见 [`World::advance_clock`]。
    clock_offset: i64,
    /// 时钟是否被 [`World::advance_clock`] 冻结（冻结后不再叠加真实流逝时间）。
    clock_frozen: bool,
}

impl Default for World {
    fn default() -> Self {
        Self::new(999)
    }
}

impl World {
    /// 用坐标上界构造（无地图，仅做边界与占格校验）。
    ///
    /// 经验表用 [`ExpTable::fallback`]（近似曲线），保证测试与「没有任何数据文件」时也能跑；
    /// 生产路径在 [`World::set_exp_table`] 里换成从 `ExpList.ini` 读到的真表。
    #[must_use]
    pub fn new(bound: i32) -> Self {
        Self {
            players: HashMap::new(),
            monsters: Vec::new(),
            npcs: Vec::new(),
            pending_hits: Vec::new(),
            pending_magic: Vec::new(),
            respawn_groups: Vec::new(),
            ground_items: Vec::new(),
            game_data: None,
            item_expire_ms: crate::items::ITEM_EXPIRE_MS,
            enforce_bag_weight: true,
            item_uids: ItemUidGenerator::default(),
            exp: ExpTable::fallback(),
            respawn_tick_scale: 1.0,
            next_object_id: 1,
            rng: MirRng::from_entropy(),
            bound,
            map: None,
            started: Instant::now(),
            started_unix_ms: unix_now_ms(),
            clock_offset: 0,
            clock_frozen: false,
        }
    }

    /// 用真实地图构造：行走判定会查地图的可走性。
    #[must_use]
    pub fn with_map(map: Arc<mir_world::Map>) -> Self {
        let bound = map.width.max(map.height).saturating_sub(1);
        Self {
            players: HashMap::new(),
            monsters: Vec::new(),
            npcs: Vec::new(),
            pending_hits: Vec::new(),
            pending_magic: Vec::new(),
            respawn_groups: Vec::new(),
            ground_items: Vec::new(),
            game_data: None,
            item_expire_ms: crate::items::ITEM_EXPIRE_MS,
            enforce_bag_weight: true,
            item_uids: ItemUidGenerator::default(),
            exp: ExpTable::fallback(),
            respawn_tick_scale: 1.0,
            next_object_id: 1,
            rng: MirRng::from_entropy(),
            bound,
            map: Some(map),
            started: Instant::now(),
            started_unix_ms: unix_now_ms(),
            clock_offset: 0,
            clock_frozen: false,
        }
    }

    /// 替换等级经验表（生产路径用它装入 `ExpList.ini`）。
    pub fn set_exp_table(&mut self, exp: ExpTable) {
        self.exp = exp;
        // 已经在线的人按新表刷新一次「下一级需求」
        // （Crystal 在 `RefreshMaxExperience` 里做，`PlayerObject.cs:9627`）。
        let levels: Vec<(u32, u16)> = self
            .players
            .values()
            .map(|player| (player.object_id, player.level))
            .collect();
        for (object_id, level) in levels {
            let max_experience = self.exp.exp_for_level(level);
            if let Some(player) = self.players.get_mut(&object_id) {
                player.max_experience = max_experience;
            }
        }
    }

    /// 当前经验表。
    #[must_use]
    pub fn exp_table(&self) -> &ExpTable {
        &self.exp
    }

    /// Crystal `Envir.RespawnTick` 的全局倍率（默认 `1.0`）。
    #[must_use]
    pub fn respawn_tick_scale(&self) -> f64 {
        self.respawn_tick_scale
    }

    /// 设置刷怪全局倍率。
    ///
    /// `1.0` = Crystal 默认节奏；`0.5` = 刷新间隔减半；`2.0` = 加倍。
    /// Crystal 里这个倍率来自 `RespawnTimer.CurrentDelay = BaseSpawnRate * 60000 * bonus`
    /// （`Server/MirEnvir/RespawnTimer.cs:112-123`，`bonus` 由在线人数档位决定），
    /// 本作先把它做成一个可直接设定的旋钮（GM / 测试用）。
    ///
    /// # Panics
    /// 传入非正数或非有限值时 panic（测试/配置错误应当立刻暴露）。
    pub fn set_respawn_tick_scale(&mut self, scale: f64) {
        assert!(scale.is_finite() && scale > 0.0, "刷怪倍率必须是正数");
        self.respawn_tick_scale = scale;
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
        if self.clock_frozen {
            // 冻结时钟：dvance_clock 之后时间只由偏移决定，读到的时间才是可复现的。
            self.clock_offset
        } else {
            self.started.elapsed().as_millis() as i64 + self.clock_offset
        }
    }

    /// 把世界时钟快进 `ms` 毫秒（**仅用于测试**：让 300ms 的伤害延迟、2 秒的怪物调度粒度、
    /// 3 分钟的尸体停留都能被确定性地驱动，不必真的 `sleep`）。
    /// 调用后时钟会**冻结**：`now()` 不再叠加真实流逝的毫秒数，否则「快进 60000ms 之后
    /// 立刻读到的间隔」会随机器快慢在 59999/60000 之间抖动，断言精确间隔的测试就会偶发失败。
    pub fn advance_clock(&mut self, ms: i64) {
        // 先把「当前时刻」固化进偏移再冻结，保证时间只向前走：
        // 冻结前 `now()` 里含真实流逝的毫秒（`started.elapsed()`），直接清零偏移会把它拨回去，
        // 于是「攻击时排的 305ms 后结算」在快进 300ms 后就永远到不了。
        let current = self.now();
        self.clock_frozen = true;
        self.clock_offset = current + ms;
    }

    /// 「墙上时钟」：世界创建时刻的 Unix 毫秒 + 单调时钟。
    ///
    /// 任务的接取时间/超时时间要**跨重启**保持意义（存档里存的就是它），所以不能用
    /// 单纯的单调时钟；但超时测试又必须能被 [`World::advance_clock`] 驱动，因此两条时间线
    /// 绑在一起：`advance_clock` 快进时墙上时钟也一起走。
    #[must_use]
    pub fn wall_now(&self) -> i64 {
        self.started_unix_ms + self.now()
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
        let now = self.now();
        let appearance = self.appearance_packet(&player);
        // 先让新玩家看到周围的人，再把自己的外观发给别人
        // （Crystal 对视野内每个对象调用 `Add(player)` 下发 `GetInfo()`，`MapObject.cs:286-308`）。
        let mut existing = Vec::new();
        for other in self.nearby(player.position, &player.map, VIEW_RANGE, None) {
            existing.push(other.clone());
        }
        for other in &existing {
            let _ = player.sender.send(self.appearance_packet(other));
        }

        // 视野内的怪物：`ObjectMonster`（Crystal `MonsterObject.GetInfo`）。
        let mut visible_monsters = BTreeSet::new();
        for monster in &self.monsters {
            if monster.map == player.map && monster.position.in_range(player.position, VIEW_RANGE) {
                visible_monsters.insert(monster.object_id);
                let _ = player.sender.send(monster_info_frame(monster));
            }
        }

        // 视野内的 NPC：`ObjectNPC`（Crystal `NPCObject.GetInfo`，`NPCObject.cs:381-395`；
        // `MapObject.Add` 会把视野内每个对象的外观发给新玩家，`MapObject.cs:286-308`）。
        self.send_visible_npcs(&player);

        // 视野内的地面物品：`ObjectItem`/`ObjectGold`（Crystal `ItemObject.GetInfo`，`ItemObject.cs:365-383`）。
        let mut visible_items = BTreeSet::new();
        for ground in &self.ground_items {
            if ground.map == player.map && ground.position.in_range(player.position, VIEW_RANGE) {
                visible_items.insert(ground.object_id);
                let _ = player.sender.send(self.ground_item_frame(ground));
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
        // 升级需求由经验表算出（Crystal `RefreshMaxExperience`，`PlayerObject.cs:9627-9630`；
        // 0 级在建角后不会进世界，这里仍然按 1 级取，避免 `ExperienceList[-1]` 那种越界）。
        player.max_experience = self.exp.exp_for_level(player.level.max(1));
        // 简化：Crystal 的 `RegenTime` 初值是 0（字段无初始化，`HumanObject.cs:187`），
        // 所以「HP 不满的玩家一登录就立刻回一次血」（`HumanObject.cs:145-151`）。本作改成
        // 「入场后满一个 `RegenDelay` 才首次回复」，原因：角色存档里的 HP 经常不是满的，
        // 登录瞬间凭空回血会让客户端开场动画里莫名飘字，也让登录链路的包序列不可稳定断言。
        player.regen_time = now + PLAYER_REGEN_DELAY_MS;
        player.visible_monsters = visible_monsters;
        player.visible_items = visible_items;
        // 让后续 `allocate_object_id` 不会撞上这个玩家的 ID
        // （生产路径是先分配再 `enter`，但测试会直接构造对象 ID）。
        self.next_object_id = self.next_object_id.max(player.object_id.wrapping_add(1));
        let object_id = player.object_id;
        let has_equipment = player.equipment.iter().any(Option::is_some);
        self.players.insert(object_id, player);
        // 带着装备登录时要立刻把装备加成算进去（Crystal 登录/换装都会走
        // `RefreshStats`，`HumanObject.cs:1730-1769`）；空装备就不动，避免覆盖测试
        // 直接塞进 `Player::stats` 的自定义数值。
        if has_equipment {
            self.refresh_stats(object_id);
        }
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
        // 走完之后刷新自己的怪物/地面物品视野（Crystal `AddObjects/RemoveObjects`，
        // `HumanObject.cs:2500-2510`、`:7864`、`:8152`）。
        self.sync_player_monsters(object_id);
        self.sync_player_ground_items(object_id);
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
        self.sync_player_ground_items(object_id);
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

        // 2) `AttackTime = now + AttackSpeed`、`ActionTime = now + 550`、`RegenTime = now + RegenDelay`
        //    （`HumanObject.cs:2974-2976`）——出手会把自然回复往后推一个完整周期。
        let attack_delay = combat::player_attack_delay_ms(level, stats.attack_speed);
        if let Some(player) = self.players.get_mut(&object_id) {
            player.attack_time = now + attack_delay;
            player.action_time = now + PLAYER_ATTACK_ACTION_MS;
            player.regen_time = now + PLAYER_REGEN_DELAY_MS;
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

    /// 世界 tick：玩家簿记 → 到期伤害 → 到期魔法 → 刷怪点 → 怪物 AI。
    ///
    /// 由服务端的定时任务每 100ms 调一次；顺序理由见模块文档的「tick 顺序」。
    pub fn tick(&mut self) {
        let now = self.now();
        self.tick_players(now);
        // 任务超时（Crystal 用 `DelayedAction` 排程，本作每 tick 比较；见 `crate::quests`）。
        self.tick_quests(self.wall_now());
        self.resolve_pending_hits(now);
        self.resolve_pending_magic(now);
        self.tick_respawns(now);
        self.tick_monsters(now);
        // 地面物品到期（Crystal `ItemObject.Process`，`ItemObject.cs:129-142`）。
        // 放在最后：本 tick 新生成/拾取的物品不会在同一 tick 里被判过期。
        self.tick_ground_items(now);
    }

    /// 玩家侧簿记：跑步计数衰减、停顿后清零 `step_counter`、自然回血回蓝、到点复活。
    ///
    /// 玩家按对象 ID 升序遍历（Crystal 按 `Envir.Objects` 的插入顺序，等价于创建顺序），
    /// 保证同一 seed 下发包顺序可回放。
    fn tick_players(&mut self, now: i64) {
        // `HumanObject.Process` 先 `ProcessBuffs()` 再 `ProcessRegen()`（`HumanObject.cs:323-324`），
        // 所以增益到期（可能改上限）必须排在natural回复之前。
        self.tick_buffs(now);
        let mut ids: Vec<u32> = self.players.keys().copied().collect();
        ids.sort_unstable();
        for object_id in ids {
            let Some(player) = self.players.get_mut(&object_id) else {
                continue;
            };
            if player.run_counter > 0 && now >= player.run_time {
                player.run_counter -= 1;
                player.run_time = now + RUN_COUNTER_DECAY_MS;
            }
            if player.cell_time + IDLE_RESET_MS < now {
                player.step_counter = 0;
            }
            // `HumanObject.Process` 的调用顺序：先 `ProcessRegen()`，再 `ProcessPoison()`
            // （`HumanObject.cs:323-325`）。毒还没实现，这里只做回复。
            self.process_player_regen(object_id, now);
            // Crystal 没有服务端复活计时器；到点自动复活是本作的扩展（见 `PLAYER_REVIVE_DELAY_MS`）。
            let due = self.players.get(&object_id).is_some_and(|player| {
                player.dead && player.revive_time > 0 && now >= player.revive_time
            });
            if due {
                self.revive_player(object_id);
            }
        }
    }

    /// 玩家自然回血/回蓝/药水回复：Crystal `HumanObject.ProcessRegen`（`HumanObject.cs:550-649`）。
    ///
    /// ```csharp
    /// if (Dead) return;                                                  // :552
    /// if (CanRegen) {                                                    // :556，即 Time >= RegenTime
    ///     RegenTime = Envir.Time + RegenDelay;                           // :558，10000ms
    ///     if (HP < Stats[HP]) healthRegen += (int)(Stats[HP] * 0.03F) + 1;  // :560-564
    ///     if (MP < Stats[MP]) manaRegen   += (int)(Stats[MP] * 0.03F) + 1;  // :566-570
    /// }
    /// if (Envir.Time > PotTime) { ... 药水池每 200ms 放 5 + Level/10 ... }  // :573-600
    /// if (Envir.Time > HealTime) { ... HealAmount 分段 ... }             // :602-617（治疗术的生效方式）
    /// if (healthRegen > 0) { ChangeHP(healthRegen); BroadcastDamageIndicator(Hit, healthRegen); }  // :635-639
    /// if (HP == Stats[HP]) { PotHealthAmount = 0; HealAmount = 0; }      // :641-645
    /// if (manaRegen > 0) ChangeMP(manaRegen);                            // :647
    /// if (MP == Stats[MP]) PotManaAmount = 0;                            // :648
    /// ```
    ///
    /// **药水分段与治疗分段都不受 `CanRegen` 节流**：Crystal 的 `PotTime`/`HealTime`
    /// 两个块在 `if (CanRegen)` 之外，各自按 200ms / 600ms 结算。
    ///
    /// 简化：`Stats[HealthRecovery]`/`Stats[SpellRecovery]`（装备加成，`:563`、`:569`）
    /// 与吸血计时器（`:619-633`）未实现——本作没有这两类装备属性与吸血，
    /// 加进来只会平白多抽随机数。回血/回蓝后的 `S.HealthChanged` 由
    /// [`World::change_player_hp`]/[`World::change_player_mp`] 发出（Crystal 的
    /// `SetHP`/`SetMP` 都走 `SendHealthChanged`，`HumanObject.cs:867-884`；
    /// Crystal 没有 `S.MPChanged` 这种包）。
    fn process_player_regen(&mut self, object_id: u32, now: i64) {
        let Some(player) = self.players.get(&object_id) else {
            return;
        };
        // `if (Dead) return;`（`HumanObject.cs:552`）。
        if player.dead {
            return;
        }
        let max_hp = player.stats.hp;
        let max_mp = player.stats.mp;
        let hp = player.hp;
        let mp = player.mp;
        let regen_time = player.regen_time;
        let level = i32::from(player.level);
        let map = player.map.clone();
        let position = player.position;

        // `CanRegen`（`HumanObject.cs:145-151`）。
        let can_regen = now >= regen_time;
        if can_regen {
            // `RegenTime = Envir.Time + RegenDelay;`（`:558`）——**只要进入这个分支就重置**，
            // 与本次是否真的回了血无关。
            if let Some(player) = self.players.get_mut(&object_id) {
                player.regen_time = now + PLAYER_REGEN_DELAY_MS;
            }
        }

        // 药水池（`:573-600`）：喝了普通药水之后每 200ms 放一小段，和自然回复相加后统一生效。
        let (pot_health, pot_mana) = self.take_pot_chunk(object_id, now, level);
        // 治疗术攒下的 `HealAmount`：每 600ms 放出一小段（`:602-617`）。
        let heal_regen = self.take_heal_chunk(object_id, now);

        let mut health_regen = pot_health + heal_regen;
        let mut mana_regen = pot_mana;
        if can_regen {
            if hp < max_hp {
                health_regen += (f64::from(max_hp) * PLAYER_REGEN_RATE) as i32 + 1;
            }
            if mp < max_mp {
                mana_regen += (f64::from(max_mp) * PLAYER_REGEN_RATE) as i32 + 1;
            }
        }

        if health_regen > 0 {
            self.change_player_hp(object_id, health_regen);
            // `BroadcastDamageIndicator(DamageType.Hit, healthRegen)`（`:638`）：
            // 自身与周围玩家都会看到一条「+N」飘字。
            self.emit_damage_indicator(&map, position, object_id, health_regen, DamageType::Hit);
            self.clear_heal_when_full(object_id);
        }
        if mana_regen > 0 {
            self.change_player_mp(object_id, mana_regen);
        }
        // `if (MP == Stats[MP]) PotManaAmount = 0;`（`:648`）。
        if let Some(player) = self.players.get_mut(&object_id) {
            if player.mp >= player.stats.mp {
                player.pot_mana_amount = 0;
            }
        }
    }

    /// 取走本 tick 的药水回复量：Crystal `ProcessRegen` 的 `PotTime` 块（`HumanObject.cs:573-600`）。
    ///
    /// ```csharp
    /// if (Envir.Time > PotTime) {
    ///     PotTime = Envir.Time + PotDelay;                     // 200ms
    ///     int PerTickRegen = 5 + (Level / 10);
    ///     if (PotHealthAmount > PerTickRegen) { healthRegen += PerTickRegen; PotHealthAmount -= …; }
    ///     else { healthRegen += PotHealthAmount; PotHealthAmount = 0; }
    ///     ... 魔法同式 ...
    /// }
    /// ```
    fn take_pot_chunk(&mut self, object_id: u32, now: i64, level: i32) -> (i32, i32) {
        let Some(player) = self.players.get_mut(&object_id) else {
            return (0, 0);
        };
        if now <= player.pot_time {
            return (0, 0);
        }
        player.pot_time = now + PLAYER_POT_DELAY_MS;
        let per_tick = POT_REGEN_BASE + level / 10;
        let health = take_ushort_chunk(&mut player.pot_health_amount, per_tick);
        let mana = take_ushort_chunk(&mut player.pot_mana_amount, per_tick);
        (health, mana)
    }

    /// `if (HP == Stats[HP]) { PotHealthAmount = 0; HealAmount = 0; }`
    /// （`HumanObject.cs:641-645`）：满血后丢弃剩余治疗量。
    fn clear_heal_when_full(&mut self, object_id: u32) {
        if let Some(player) = self.players.get_mut(&object_id) {
            if player.hp >= player.stats.hp {
                player.magic.heal_amount = 0;
                player.pot_health_amount = 0;
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
    ///
    /// 返回实际扣掉的血量（`damage - armour`，未命中时 0）：魔法伤害要靠它决定
    /// 是否给技能加经验（`HumanObject.cs:5929` 的 `if (target.Attacked(...) > 0) LevelMagic(magic)`）。
    pub(crate) fn damage_monster(&mut self, hit: PendingHit, attacker_stats: &CombatStats) -> i32 {
        let Some((stats, dead, map, position, direction)) = self.monster_snapshot(hit.target_id)
        else {
            return 0;
        };
        if dead {
            return 0;
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
            return 0;
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
        result.damage
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

        // 气功波（`Stat.EnergyShieldPercent`）：`if (Random.Next(100) < Percent) { …回血… }`
        // （`HumanObject.cs:7262-7271`）。抽签位置在「未命中」判定之后、`S.Struck` 之前。
        let shield_percent = stats.energy_shield_percent;
        if shield_percent > 0 && (self.rng.next_bounded(100) as i32) < shield_percent {
            let gain = stats.energy_shield_hp_gain;
            let (hp, max_hp) = {
                let player = &self.players[&hit.target_id];
                (player.hp, player.stats.hp)
            };
            if gain > 0 {
                if hp + gain >= max_hp {
                    // `if (HP + Gain >= Stats[HP]) SetHP(Stats[HP]); else ChangeHP(Gain);`
                    self.set_player_hp(hit.target_id, max_hp);
                } else {
                    self.change_player_hp(hit.target_id, gain);
                }
            }
        }

        // 魔法盾：每点实际伤害扣 60ms（`HumanObject.cs:7273-7277`）。
        let dealt = result.damage.max(0);
        self.shave_buff(
            hit.target_id,
            BuffKind::MagicShield,
            i64::from(dealt) * MAGIC_SHIELD_MS_PER_DAMAGE,
        );

        // `RegenTime = Envir.Time + RegenDelay;`（`HumanObject.cs:7295`，在 `S.Struck` 之前）。
        if let Some(player) = self.players.get_mut(&hit.target_id) {
            player.regen_time = now + PLAYER_REGEN_DELAY_MS;
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

        // `ChangeHP(armour - damage)`（`HumanObject.cs:7376`）：扣血、归零则 `Die()`，
        // 然后 `SendHealthChanged()`（`HumanObject.cs:885-907`）。
        self.change_player_hp(hit.target_id, -result.damage);
    }

    /// 玩家死亡：Crystal `PlayerObject.Die`（`PlayerObject.cs:578-667`）的 MVP 实现。
    ///
    /// 照抄的部分：`HP = 0`、`Dead = true`、`LogTime/BrownTime = Envir.Time`（`:643-647`）、
    /// 本人 `S.Death`（`:649`）、周围玩家 `S.ObjectDied`（`:650`）。
    ///
    /// 简化（都在源码里标了 `// 简化：`）：装备复活戒指（`:580-597`）、PK 红名与掉落
    /// （`:599-641`）、buff/毒清理（`:652-662`）、`CallDefaultNPC(Die)` 与统计（`:664-666`）。
    ///
    /// 复活计时是本作扩展：Crystal 服务端没有自动复活（见 [`PLAYER_REVIVE_DELAY_MS`]）。
    fn player_died(&mut self, player_id: u32) {
        let now = self.now();
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        if player.dead {
            return;
        }
        player.dead = true;
        player.hp = 0;
        player.death_time = now;
        player.revive_time = now + PLAYER_REVIVE_DELAY_MS;
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
    }

    /// 玩家原地复活：Crystal `PlayerObject.Revive` → `HumanObject.Revive`
    /// （`PlayerObject.cs:1379-1391`、`HumanObject.cs:1690-1721`）。
    ///
    /// 顺序与包：
    ///
    /// ```csharp
    /// if (!Dead) return;
    /// Dead = false;
    /// SetHP(hp);                                    // → S.HealthChanged（HumanObject.cs:858-869）
    /// CurrentMap.RemoveObject(this);
    /// Broadcast(new S.ObjectRemove { ObjectID });   // → 周围玩家
    /// CurrentMap = CurrentMap; CurrentLocation = CurrentLocation;   // 原地（源码里就是两行空操作）
    /// CurrentMap.AddObject(this);
    /// Enqueue(new S.MapChanged { ... });            // 见下：本作不发
    /// Enqueue(new S.Revived());                     // → 本人
    /// Broadcast(new S.ObjectRevived { ObjectID, Effect = effect });  // → 周围玩家
    /// ```
    ///
    /// 简化：`S.MapChanged` 不发——世界层没有地图标题/小地图/光照/音乐这些元数据
    /// （它们只在 `MapInformation` 里下发过一次），而复活本来就不换图；客户端靠
    /// `S.Revived` 就能把死亡动作切回来。
    ///
    /// 额外多一条 `S.ObjectPlayer`：Crystal 的 `AddObject` 只把对象塞回格子、不再广播
    /// `GetInfo()`，但我们的客户端把 `S.ObjectRemove` 当作彻底删除，不补外观包的话
    /// 复活后的角色在旁人客户端里会永久消失。Crystal 在 `HumanObject.cs:1697-1721` 里
    /// 只发 `ObjectRemove` + `ObjectRevived`。
    ///
    /// 返回是否真的复活了（原本就活着/不存在时返回 `false`）。
    pub fn revive_player(&mut self, player_id: u32) -> bool {
        let Some(player) = self.players.get(&player_id) else {
            return false;
        };
        if !player.dead {
            return false;
        }
        let max_hp = player.stats.hp;
        let max_mp = player.stats.mp;
        let map = player.map.clone();
        let position = player.position;

        if let Some(player) = self.players.get_mut(&player_id) {
            player.dead = false;
            player.death_time = 0;
            player.revive_time = 0;
            player.step_counter = 0;
        }
        // `SetHP(Stats[Stat.HP])` / `SetMP(...)`：`TownRevive` 走的是这条（`PlayerObject.cs:1414-1415`），
        // 原地复活（`HumanObject.Revive`）只调 `SetHP`；本作两条都补满，理由：
        // 自动复活没有回城补给的机会，只回血不回蓝会让法师复活即瘫痪。
        self.set_player_hp(player_id, max_hp);
        self.set_player_mp(player_id, max_mp);

        let remove = object_remove_frame(player_id);
        let revived = ServerPacket::ObjectRevived(ObjectRevived {
            object_id: player_id,
            effect: true,
        })
        .encode_frame();
        let appearance = self
            .players
            .get(&player_id)
            .map(|player| self.appearance_packet(player));
        for other in self.players.values() {
            if other.object_id == player_id
                || other.map != map
                || !other.position.in_range(position, VIEW_RANGE)
            {
                continue;
            }
            let _ = other.sender.send(remove.clone());
            let _ = other.sender.send(revived.clone());
            if let Some(frame) = &appearance {
                let _ = other.sender.send(frame.clone());
            }
        }
        self.send_to_player(player_id, &ServerPacket::Revived(Revived {}).encode_frame());
        true
    }

    /// `HumanObject.SetHP`（`HumanObject.cs:858-869`）：赋值 → 夹到上限 → 归零则死 → 同步血量。
    pub(crate) fn set_player_hp(&mut self, object_id: u32, amount: i32) {
        let Some(player) = self.players.get_mut(&object_id) else {
            return;
        };
        if player.hp == amount {
            return;
        }
        let max_hp = player.stats.hp;
        player.hp = amount.min(max_hp);
        let died = !player.dead && player.hp == 0;
        if died {
            self.player_died(object_id);
        }
        self.send_health_changed(object_id);
    }

    /// `HumanObject.SetMP`（`HumanObject.cs:875-884`）：与 `SetHP` 同构，但不会触发死亡。
    pub(crate) fn set_player_mp(&mut self, object_id: u32, amount: i32) {
        let Some(player) = self.players.get_mut(&object_id) else {
            return;
        };
        if player.mp == amount {
            return;
        }
        let max_mp = player.stats.mp;
        player.mp = amount.min(max_mp);
        self.send_health_changed(object_id);
    }

    /// `HumanObject.ChangeHP`（`HumanObject.cs:885-907`）。
    pub(crate) fn change_player_hp(&mut self, object_id: u32, amount: i32) {
        let Some(player) = self.players.get_mut(&object_id) else {
            return;
        };
        let max_hp = player.stats.hp;
        let mut amount = amount;
        if player.hp + amount > max_hp {
            amount = max_hp - player.hp;
        }
        if amount == 0 {
            return;
        }
        player.hp += amount;
        if player.hp < 0 {
            player.hp = 0;
        }
        let died = !player.dead && player.hp == 0;
        if died {
            self.player_died(object_id);
        }
        self.send_health_changed(object_id);
    }

    /// `HumanObject.ChangeMP`（`HumanObject.cs:912-926`）。
    pub(crate) fn change_player_mp(&mut self, object_id: u32, amount: i32) {
        let Some(player) = self.players.get_mut(&object_id) else {
            return;
        };
        let max_mp = player.stats.mp;
        let mut amount = amount;
        if player.mp + amount > max_mp {
            amount = max_mp - player.mp;
        }
        if amount == 0 {
            return;
        }
        player.mp += amount;
        if player.mp < 0 {
            player.mp = 0;
        }
        self.send_health_changed(object_id);
    }

    /// `HumanObject.SendHealthChanged`（`HumanObject.cs:870-874`）：只给本人发 `S.HealthChanged{HP,MP}`。
    ///
    /// 简化：Crystal 还会调 `BroadcastHealthChange()`（`MapObject.cs:841-914`），
    /// 把 `S.ObjectHealth` 发给队友/宠物主人；本作没有队伍与宠物，玩家侧这一路在
    /// `change`/`set` 里不发，只有**上限变化**时由
    /// [`World::broadcast_health_change`](crate::buffs::World::broadcast_health_change) 补一条。
    pub(crate) fn send_health_changed(&self, object_id: u32) {
        if let Some(player) = self.players.get(&object_id) {
            let _ = player.sender.send(
                ServerPacket::HealthChanged(HealthChanged {
                    hp: player.hp,
                    mp: player.mp,
                })
                .encode_frame(),
            );
        }
    }

    /// 击杀经验结算入口：Crystal `MonsterObject.Die` →
    /// `EXPOwner.WinExp(Experience, Level)`（`Server/MirObjects/MonsterObject.cs:982-984`）。
    ///
    /// `amount` 是怪物模板的经验（[`MonsterKind::experience`]），`target_level` 是怪物等级。
    /// Crystal 的 `WinExp` 还要过 `ReduceExp` 与 `Settings.ExpRate`（`PlayerObject.cs:794-845`）；
    /// 简化：组队分摊（`:807-833`）、英雄（`:839-844`）、地图 `NoExperience`、夫妻/师徒/行会加成
    /// 全部未实现——本作没有这些系统。`Settings.ExpRate` 默认 1.0（`Server/Settings.cs:112`）。
    pub fn win_experience(&mut self, object_id: u32, amount: u32, target_level: u16) {
        let Some(player) = self.players.get(&object_id) else {
            return;
        };
        let exp = reduce_exp(player.level, amount, target_level);
        self.gain_experience(object_id, u32::try_from(exp).unwrap_or(0));
    }

    /// Crystal `PlayerObject.GainExp`（`PlayerObject.cs:846-931`）。
    ///
    /// 流程：`Experience += amount` → 本人 `S.GainExperience` → 够阈值就跨级 →
    /// `LevelUp()`（见 [`World::player_level_up`]）。
    ///
    /// 任务奖励经验走的就是这条路（Crystal `GainExp(ExpReward * Settings.ExpRate)`，
    /// `PlayerObject.cs:11457`），不经过 [`World::win_experience`] 的等级差衰减。
    pub(crate) fn gain_experience(&mut self, object_id: u32, amount: u32) {
        // `if (amount == 0) return;`（`:850`）
        if amount == 0 {
            return;
        }
        let Some(player) = self.players.get(&object_id) else {
            return;
        };
        let mut level = player.level.max(1);
        let mut experience = player.experience.saturating_add(i64::from(amount));
        let mut max_experience = self.exp.exp_for_level(level);

        // `Enqueue(new S.GainExperience { Amount = amount });`（`:893`）——只给本人。
        self.send_to_player(
            object_id,
            &ServerPacket::GainExperience(GainExperience { amount }).encode_frame(),
        );
        if let Some(player) = self.players.get_mut(&object_id) {
            player.experience = experience;
            player.level = level;
            player.max_experience = max_experience;
        }

        // `if (Experience < MaxExperience) return; if (Level >= ushort.MaxValue) return;`（`:905-906`）
        if experience < max_experience || level == u16::MAX {
            return;
        }

        // ```csharp
        // while (experience >= MaxExperience) {                 // :911
        //     Level++; experience -= MaxExperience;             // :913-914
        //     RefreshLevelStats();                              // :916 → RefreshMaxExperience
        //     if (Level >= ushort.MaxValue) break;              // :918
        // }
        // ```
        //
        // 简化：多一个 `max_experience > 0` 的守卫。Crystal 在等级超出经验表（500 项）之后
        // `MaxExperience` 变成 0（`PlayerObject.cs:9629` 的三元表达式），于是
        // `while (experience >= 0)` 会一路升到 65535 级——本作遇到 0 就停（见 docs 的异常清单）。
        while experience >= max_experience && max_experience > 0 {
            level += 1;
            experience -= max_experience;
            // `RefreshLevelStats()` → `RefreshMaxExperience()`（`HumanObject.cs:1773-1781`）。
            max_experience = self.exp.exp_for_level(level);
            if level == u16::MAX {
                break;
            }
        }
        if let Some(player) = self.players.get_mut(&object_id) {
            // `Experience = experience;`（`:921`）
            player.experience = experience;
            player.level = level;
            player.max_experience = max_experience;
        }
        self.player_level_up(object_id);
    }

    /// Crystal `HumanObject.LevelUp`（`HumanObject.cs:845-852`）+ `PlayerObject.LevelUp`
    /// （`PlayerObject.cs:932-956`）。
    ///
    /// 发出的包（顺序照抄源码）：
    ///
    /// | # | 包 | 收件人 | 出处 |
    /// |---|---|---|---|
    /// | 1 | `HealthChanged` | 本人 | `SetHP(Stats[HP])` → `SendHealthChanged`（`HumanObject.cs:848`） |
    /// | 2 | `HealthChanged` | 本人 | `SetMP(Stats[MP])` → `SendHealthChanged`（`:849`） |
    /// | 3 | `ObjectLeveled` | **周围玩家**（不含本人） | `Broadcast`（`:851`） |
    /// | 4 | `LevelChanged` | 本人 | `Enqueue`（`PlayerObject.cs:938`）——注意不是广播 |
    ///
    /// 1 与 2 是**两条**内容不同的 `HealthChanged`：`SetHP` 先把 HP 补满（此时 MP 还是旧值），
    /// `SetMP` 再把 MP 补满。Crystal 就是这么发的，客户端以最后一条为准。
    fn player_level_up(&mut self, object_id: u32) {
        let Some(player) = self.players.get(&object_id) else {
            return;
        };
        let level = player.level;
        let map = player.map.clone();
        let position = player.position;
        let experience = player.experience;
        let max_experience = player.max_experience;

        // `RefreshStats()`（`HumanObject.cs:847`）：等级曲线 + 装备加成一起重算
        // （`RefreshLevelStats` → `RefreshEquipmentStats`，`:1740-1742`）。
        self.refresh_stats(object_id);
        let stats = self.players[&object_id].stats;
        // `SetHP(Stats[Stat.HP]); SetMP(Stats[Stat.MP]);`（`:848-849`）
        self.set_player_hp(object_id, stats.hp);
        self.set_player_mp(object_id, stats.mp);
        // `Broadcast(new S.ObjectLeveled { ObjectID = ObjectID });`（`:851`）
        let packet = ServerPacket::ObjectLeveled(ObjectLeveled { object_id }).encode_frame();
        self.broadcast(&map, position, Some(object_id), &packet, object_id);
        // `Enqueue(new S.LevelChanged { Level, Experience, MaxExperience });`（`PlayerObject.cs:938`）
        self.send_to_player(
            object_id,
            &ServerPacket::LevelChanged(LevelChanged {
                level,
                experience,
                max_experience,
            })
            .encode_frame(),
        );
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

    /// 登记一组刷怪点并**立即铺满** `count` 只，返回刷出来的对象 ID（生成顺序）。
    ///
    /// 对应 Crystal 建图时把 `RespawnInfo` 变成 `MapRespawn` 放进 `Map.Respawns`
    /// （`Server/MirEnvir/Map.cs:473-488`）：登记之后由 `Map.ProcessRespawns`
    /// 每过一个刷新周期把缺口补齐（`:740-790`）。
    ///
    /// 当前存活数与下次刷新时刻用 [`World::respawn_state`] 读。
    pub fn register_spawn_group(&mut self, group: SpawnGroup) -> Vec<u32> {
        let now = self.now();
        let index = self.respawn_groups.len();
        self.respawn_groups.push(RespawnGroupState {
            group,
            alive: 0,
            next_spawn_at: 0,
            error_count: 0,
        });
        // Crystal 建图并**不**立刻刷怪，而是等 `Map.ProcessRespawns`（首个 `RespawnTime` 是 0，
        // 所以第一次 `Map.Process` 就会补齐）。为了「进服务器就能看到怪」，这里立刻补齐，
        // 并把「补不满」的缺口排进第一次刷新。
        let count = self.respawn_groups[index].group.count;
        let mut spawned = Vec::new();
        for _ in 0..count {
            if let Some(object_id) = self.spawn_from_group(index) {
                spawned.push(object_id);
            }
        }
        let (alive, count, template) = {
            let state = &self.respawn_groups[index];
            (state.alive, state.group.count, state.group.clone())
        };
        if alive < count {
            // 没铺满（半径内找不到空位）：排一个周期后再补（Crystal 会给 1 分钟冷却，`Map.cs:770`）。
            let delay = self.respawn_delay_ms(&template);
            self.respawn_groups[index].next_spawn_at = now + delay;
        }
        spawned
    }

    /// 兼容旧入口：等价于 [`World::register_spawn_group`]（登记 + 立即铺满）。
    pub fn spawn_group(&mut self, group: &SpawnGroup) -> Vec<u32> {
        self.register_spawn_group(group.clone())
    }

    /// 从指定的刷怪组里刷**一只**怪（Crystal `MapRespawn.Spawn` → `MonsterObject.Spawn`，
    /// `Server/MirEnvir/Map.cs:2557-2568`、`MonsterObject.cs:706-732`）。
    ///
    /// 简化：Crystal 从地图预计算的 `WalkableCells`（已裁剪到刷新点 ± `Spread`）里
    /// 随机取一格；mir-next 在半径内撒点、校验可走与占格，最多重试 16 次。
    fn spawn_from_group(&mut self, index: usize) -> Option<u32> {
        let group = self.respawn_groups.get(index)?.group.clone();
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
        let cell = placed?;
        let object_id = self.spawn_monster(&group.map, group.kind.clone(), cell);
        if let Some(monster) = self.monster_mut(object_id) {
            monster.respawn_group = Some(index);
        }
        if let Some(state) = self.respawn_groups.get_mut(index) {
            state.alive += 1;
        }
        Some(object_id)
    }

    /// 下一次刷新要等多久（毫秒）。
    ///
    /// 逐字移植 `Map.ProcessRespawns` 的延迟公式（`Server/MirEnvir/Map.cs:759-760`）：
    ///
    /// ```csharp
    /// long delay = Math.Max(1, respawn.Info.Delay - respawn.Info.RandomDelay
    ///                          + Envir.Random.Next(respawn.Info.RandomDelay * 2));
    /// respawn.RespawnTime = Envir.Time + (delay * Settings.Minute);
    /// ```
    ///
    /// 注意三点（都照抄）：
    /// 1. `Random.Next(RandomDelay * 2)` 是**左闭右开** `[0, 2*RandomDelay)`，
    ///    所以真实间隔是 `Delay - RandomDelay ..= Delay + RandomDelay - 1` 分钟；
    ///    `RandomDelay == 0` 时 `.NET` 的 `Next(0)` 直接返回 0 且不抽签（`next_bounded(0)` 同）。
    /// 2. 下限是 **1 分钟**（`Math.Max(1, ...)`），即使配置写了 `Delay=0`。
    /// 3. 乘 `Settings.Minute = 60000`（`Server/Settings.cs:16`）。
    ///
    /// 全局倍率 [`World::respawn_tick_scale`] 乘在最后（Crystal 的对应物是
    /// `RespawnTimer` 的 `CurrentDelay`，`Server/MirEnvir/RespawnTimer.cs:112-123`）。
    fn respawn_delay_ms(&mut self, group: &SpawnGroup) -> i64 {
        let delay = i64::from(group.delay_minutes);
        let random = i64::from(group.random_delay_minutes);
        let bound = u32::try_from(random.saturating_mul(2)).unwrap_or(0);
        let jitter = i64::from(self.rng.next_bounded(bound));
        let minutes = i32::try_from((delay - random + jitter).max(1)).unwrap_or(i32::MAX);
        // `Settings.Minute = 60 * Second = 60000`（`Server/Settings.cs:16`）。
        let ms = f64::from(minutes) * 60_000.0 * self.respawn_tick_scale;
        (ms as i64).max(1)
    }

    /// 推进所有刷怪组（Crystal `Map.ProcessRespawns`，`Server/MirEnvir/Map.cs:740-790`）。
    ///
    /// 简化：Crystal 到点后**一次补齐**所有缺口（`:749-755` 的 `for (c < count) Spawn()`）；
    /// 本作每个刷新周期只补 **1 只**，缺口不为 0 就再排下一个周期。
    /// 原因：一次补齐会在到点的瞬间刷出一整群怪，逐只补更接近「打个不停」的手感，
    /// 也让 `respawn_state().next_spawn_at` 的语义始终是「下一只」。
    fn tick_respawns(&mut self, now: i64) {
        for index in 0..self.respawn_groups.len() {
            let (alive, count, next_spawn_at, group) = {
                let state = &self.respawn_groups[index];
                (
                    state.alive,
                    state.group.count,
                    state.next_spawn_at,
                    state.group.clone(),
                )
            };
            if alive >= count || next_spawn_at == 0 || now < next_spawn_at {
                continue;
            }
            let spawned = self.spawn_from_group(index).is_some();
            let delay = if spawned {
                self.respawn_delay_ms(&group)
            } else {
                RESPAWN_FAIL_DELAY_MS
            };
            let state = &mut self.respawn_groups[index];
            if spawned {
                state.error_count = 0;
                state.next_spawn_at = if state.alive < state.group.count {
                    now + delay
                } else {
                    0
                };
            } else {
                // Crystal：失败时给 1 分钟冷却，并累加 `ErrorCount`（`Map.cs:768-788`）。
                state.next_spawn_at = now + RESPAWN_FAIL_DELAY_MS;
                if state.error_count < RESPAWN_ERROR_LOG_THRESHOLD {
                    state.error_count += 1;
                }
                if state.error_count == RESPAWN_ERROR_LOG_THRESHOLD {
                    tracing::warn!(
                        map = %state.group.map,
                        monster = %state.group.kind.name,
                        center = ?state.group.center,
                        spread = state.group.spread,
                        "刷怪失败：半径内找不到可站立的空格（Crystal 同样会放弃这一轮）"
                    );
                }
            }
        }
    }

    /// 刷怪组的只读快照（测试 / GM 命令 / 运维观察）。
    #[must_use]
    pub fn respawn_state(&self) -> Vec<RespawnState> {
        self.respawn_groups
            .iter()
            .map(|state| RespawnState {
                map: state.group.map.clone(),
                monster: state.group.kind.name.clone(),
                center: state.group.center,
                count: state.group.count,
                alive: state.alive,
                next_spawn_at: (state.next_spawn_at != 0).then_some(state.next_spawn_at),
            })
            .collect()
    }

    /// 某个刷怪组少了一只：存活计数 `--`，并在没有排程时排出下一次刷新。
    ///
    /// Crystal `MonsterObject.Die` 里的 `if (Respawn != null) Respawn.Count--;`
    /// （`Server/MirObjects/MonsterObject.cs:994-995`）。
    pub(crate) fn on_respawn_monster_died(&mut self, index: usize, now: i64) {
        let Some(state) = self.respawn_groups.get_mut(index) else {
            return;
        };
        state.alive = state.alive.saturating_sub(1);
        // 缺口存在但还没有排程 → 排一个完整周期之后。
        let schedule = state.next_spawn_at == 0 && state.alive < state.group.count;
        let template = state.group.clone();
        if schedule {
            let delay = self.respawn_delay_ms(&template);
            if let Some(state) = self.respawn_groups.get_mut(index) {
                state.next_spawn_at = now + delay;
            }
        }
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
        // NPC 也挡路：Crystal `NPCObject.Blocking => Visible`（`NPCObject.cs:418-421`）。
        let blocked_by_npc = self
            .npcs
            .iter()
            .any(|npc| npc.map == map && npc.position == cell);
        if blocked_by_npc {
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
    pub(crate) fn send_user_location(&self, object_id: u32) {
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

/// 从药水池里取一批：Crystal `ProcessRegen` 的 `PotHealthAmount > PerTickRegen ? … : …`
/// （`HumanObject.cs:579-599`）。
///
/// 注意比较用的是 `>`：恰好等于一批时会把池子一次性清空（`else` 分支），
/// 数学上的结果相同，但池子必须归零，否则下一次又会被判成「还有剩」。
fn take_ushort_chunk(pool: &mut u16, per_tick: i32) -> i32 {
    let amount = i32::from(*pool);
    let taken = if amount > per_tick { per_tick } else { amount };
    *pool = u16::try_from(amount - taken).unwrap_or(0);
    taken
}

/// 当前 Unix 毫秒（[`World::wall_now`] 的基准；`SystemTime` 取不到时退回 0）。
fn unix_now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as i64)
}

/// Crystal `HumanObject.ReduceExp`（`Server/MirObjects/HumanObject.cs:2365-2381`）。
/// 高等级打低等级怪时经验衰减：
///
/// ```csharp
/// if (Level < targetLevel + 10 || !Settings.ExpMobLevelDifference)
///     expPoint = (int)amount;
/// else
///     expPoint = (int)amount - (int)Math.Round(Math.Max(amount / 15, 1) * ((double)Level - (targetLevel + 10)));
/// if (expPoint <= 0) expPoint = 1;
/// ```
///
/// * `Settings.ExpMobLevelDifference` 默认 `true`（`Server/Settings.cs:98`），本作固定按 true 走。
/// * `Math.Round` 在 .NET 里是**银行家舍入**（中点取偶），这里用 `round_ties_even` 对齐。
/// * `amount / 15` 是整数除法（`Math.Max` 的下限 1 在除法之后）。
fn reduce_exp(level: u16, amount: u32, target_level: u16) -> i32 {
    let level = i32::from(level);
    let target_level = i32::from(target_level);
    let amount = i32::try_from(amount).unwrap_or(i32::MAX);
    let exp = if level < target_level + 10 {
        amount
    } else {
        let step = (amount / 15).max(1);
        let over = level - (target_level + 10);
        let penalty = (f64::from(step) * f64::from(over)).round_ties_even() as i32;
        amount - penalty
    };
    exp.max(1)
}

impl World {
    /// 玩家外观包（Crystal `HumanObject.GetInfo`，对应 `S.ObjectPlayer`）。
    ///
    /// 武器/盔甲外观来自装备（Crystal `Looks_Weapon`/`Looks_Armour`/`Looks_Wings`/`Light`，
    /// `HumanObject.cs:164-165`、`:1838-1876`），由 [`World::looks`] 从装备表算出来；
    /// 换装后的**增量**外观包是 `S.PlayerUpdate`（`HumanObject.UpdateLooks`，`:7058-7061`）。
    pub(crate) fn appearance_packet(&self, player: &Player) -> Vec<u8> {
        let looks = self.looks(player.object_id);
        ServerPacket::ObjectPlayer(ObjectPlayer {
            object_id: player.object_id,
            name: player.name.clone(),
            class: player.class,
            gender: player.gender,
            level: player.level,
            location: player.position,
            direction: player.direction,
            hair: player.hair,
            light: looks.light,
            weapon: looks.weapon,
            weapon_effect: looks.weapon_effect,
            armour: looks.armour,
            wing_effect: looks.wings,
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
                experience: 0,
                max_experience: 0,
                class: MirClass::Warrior,
                gender: MirGender::Male,
                hair: 1,
                hp: 15,
                mp: 0,
                stats: combat::player_base_stats(1, MirClass::Warrior),
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
                dead: false,
                visible_monsters: BTreeSet::new(),
                inventory: crate::items::new_inventory(),
                equipment: crate::items::new_equipment(),
                magics: Vec::new(),
                magic: crate::magic::MagicState::default(),
                gold: 0,
                credit: 0,
                visible_items: BTreeSet::new(),
                quests: Vec::new(),
                completed_quests: BTreeSet::new(),
                buffs: Vec::new(),
                pot_health_amount: 0,
                pot_mana_amount: 0,
                pot_time: 0,
                unlock_curse: false,
                npc: crate::npc::NpcState::default(),
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
