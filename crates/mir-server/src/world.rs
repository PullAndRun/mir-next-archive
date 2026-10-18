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
//!
//! # 玩家对玩家（PK）
//!
//! 攻击模式、PvP 结算、PK 值/红棕黄名、死亡掉落与守卫的完整规则见 `docs/world/pk.md`。
//! 本模块里对应的入口是 [`World::change_attack_mode`]、[`World::is_attack_target_player`]、
//! `damage_player_from_player`、`process_pk_points`、`refresh_name_colour` 与 `player_died`。

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Instant;

use mir_core::MirRng;
use mir_core::Point;
use mir_core::enums::{
    AttackMode, ChatType, DamageType, DefenceType, DoorState, MirClass, MirDirection, MirGender,
    MonsterType, PetMode, PoisonType, Spell, SpellEffect, Stat,
};
use mir_core::i18n::LanguageLibrary;
use mir_core::{Language, TextKey};
use mir_protocol::ServerPacket;
use mir_protocol::data::UserItem;
use mir_protocol::packets::server::{
    ChangeAMode, ColourChanged, DamageIndicator, Death, GainExperience, HealthChanged,
    LevelChanged, ObjectAttack, ObjectColourChanged, ObjectDied, ObjectEffect, ObjectHealth,
    ObjectLeveled, ObjectMonster, ObjectPlayer, ObjectRemove, ObjectRevived, ObjectRun,
    ObjectStruck, ObjectTurn, ObjectWalk, Opendoor, RefreshItem, Revived, Struck, UserLocation,
};
use tokio::sync::mpsc::UnboundedSender;

use crate::aoi::{AoiEntry, AoiGrid, AoiKind};
use crate::buffs::{ActiveBuff, BuffKind, MAGIC_SHIELD_MS_PER_DAMAGE};
use crate::combat::{self, CombatStats, PendingHit};
use crate::exp::ExpTable;
use crate::gamedata::GameData;
use crate::items::{GroundItem, ItemUidGenerator};
use crate::magic::{MagicState, PendingMagic, UserMagic};
use crate::maps::{PendingMapMove, map_cache_limit_from_env};
use crate::monster::{Monster, MonsterKind};
use crate::npc::{Npc, NpcState};

/// 视野半径：Crystal `Globals.DataRange = 16`（`Shared/Globals.cs:37`，源码注释写着 `Was 24`）。
///
/// 服务器上所有「看得见/广播得到」的判定都用它：
///
/// * 进图/切图补视野：`PlayerObject.GetObjects` 扫 `±DataRange`（`PlayerObject.cs:1741-1768`）；
/// * 移动后的进出视野差分：`HumanObject.RemoveObjects/AddObjects`（`HumanObject.cs:7864-8440`）；
/// * 广播：`MapObject.Broadcast` 用 `Functions.InRange(..., Globals.DataRange)`（`MapObject.cs:408-420`）；
/// * 怪物脱战距离：`MonsterObject.Process` 的 `Functions.InRange(..., Globals.DataRange)`（`MonsterObject.cs:1198`）。
///
/// 怪物自己的**索敌**视野是另一套：`MonsterKind::view_range`（Crystal `MonsterInfo.ViewRange`，
/// 默认 7，`Server/MirDatabase/MonsterInfo.cs:27`），与这个广播半径无关。
pub const VIEW_RANGE: i32 = 16;
/// 聊天广播半径：与 [`VIEW_RANGE`] 相同。
///
/// Crystal 的普通聊天走 `MapObject.Broadcast`（`PlayerObject.cs:2001-2013`），
/// 判定就是 `Functions.InRange(..., Globals.DataRange)`（`MapObject.cs:408-420`），
/// 所以聊天半径 = `DataRange` = 16（`Shared/Globals.cs:37`）。
/// 保留这个别名只为兼容早期公开 API；[`World::chat`] 走的是 [`World::broadcast`]（即 `VIEW_RANGE`）。
pub const CHAT_RANGE: i32 = VIEW_RANGE;
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
/// PK 值：击杀一个「非红名、非棕名」玩家给凶手加的点数
/// （Crystal `PlayerObject.cs:611`：`hitter.PKPoints = Math.Min(int.MaxValue, LastHitter.PKPoints + 100)`）。
pub const PK_POINTS_PER_KILL: i32 = 100;
/// 红名阈值：`PKPoints >= 200`（Crystal `PlayerObject.cs:1855`、`:4670`、`:4679`）。
pub const RED_NAME_PK_POINTS: i32 = 200;
/// 黄名阈值：`PKPoints >= 100`（Crystal `PlayerObject.cs:1886`）。
pub const YELLOW_NAME_PK_POINTS: i32 = 100;
/// PK 值自然衰减的间隔：Crystal `Settings.PKDelay = 12` 秒（`Server/Settings.cs:119`），
/// 每过一个间隔 `PKPoints--`（`Server/MirObjects/MapObject.cs:212-216`）。
pub const PK_DELAY_MS: i64 = 12_000;
/// 攻击白名玩家后攻方变棕名的时长：Crystal `attacker.BrownTime = Envir.Time + Settings.Minute`
/// （`Server/MirObjects/HumanObject.cs:7199-7200`）。
pub const BROWN_NAME_DURATION_MS: i64 = MINUTE_MS;
/// `LastHitter` 的保留时长：Crystal `LastHitTime = Envir.Time + 10000`
/// （`Server/MirObjects/HumanObject.cs:7195`，超时后 `MapObject.cs:218-221` 清空）。
pub const LAST_HITTER_TIMEOUT_MS: i64 = 10_000;
/// 白名（Crystal `Color.White.ToArgb() = -1`，`System.Drawing.Color.White`）。
pub const NAME_COLOUR_WHITE: mir_core::Color = mir_core::Color(-1);
/// 红名（Crystal `PlayerObject.cs:1856`、`Color.Red`）。
pub const NAME_COLOUR_RED: mir_core::Color = mir_core::Color::from_rgb(0xFF, 0x00, 0x00);
/// 棕名（Crystal `PlayerObject.cs:1859`、`Color.SaddleBrown`）。
pub const NAME_COLOUR_BROWN: mir_core::Color = mir_core::Color::from_rgb(0x8B, 0x45, 0x13);
/// 黄名（Crystal `PlayerObject.cs:1887`、`Color.Yellow`）。
pub const NAME_COLOUR_YELLOW: mir_core::Color = mir_core::Color::from_rgb(0xFF, 0xFF, 0x00);
/// 守卫的名字颜色（Crystal `Server/MirObjects/Monsters/Guard.cs:27`、`Color.SkyBlue`）。
/// 守卫名的颜色（Crystal `ObjectMonster` 的 `NameColour`，见 `World::monster_snapshot`）。
pub const NAME_COLOUR_GUARD: mir_core::Color = mir_core::Color::from_rgb(0x87, 0xCE, 0xEB);

/// 战区里的名字颜色：Crystal `Color.Green`（`PlayerObject.cs:1865`、`:1868`）。
///
/// Crystal 在战区里按**观察者**给色（同会/无行会 → 绿，敌对行会 → 橙，`:1861-1870`）；
/// 本作只发「自己视角」的颜色（`RefreshNameColour` 用自己当观察者，恒为绿），
/// 见 `docs/world/conquest.md`。
pub const NAME_COLOUR_WAR_ZONE: mir_core::Color = mir_core::Color::from_rgb(0x00, 0xFF, 0x00);
/// 刷怪失败（找不到空位）后的重试冷却：Crystal `respawn.RespawnTime = Envir.Time + 1 * Settings.Minute`
/// （`Server/MirEnvir/Map.cs:770`）。
pub const RESPAWN_FAIL_DELAY_MS: i64 = MINUTE_MS;
/// 刷怪点连续失败多少次后写一条日志：Crystal 在 `ErrorCount` 到 6 时打日志
/// （`Server/MirEnvir/Map.cs:771-785`）。
pub const RESPAWN_ERROR_LOG_THRESHOLD: u8 = 6;
/// 门打开后自动关闭的等待时间：Crystal `Map.Process` 里
/// `if ((Doors[i].DoorState == DoorState.Open) && (Doors[i].LastTick + 5000 < Envir.Time))`
/// （`Server/MirEnvir/Map.cs:657-665`）。
pub const DOOR_OPEN_MS: i64 = 5_000;

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
    /// 攻击模式（Crystal `HumanObject.AMode` → `CharacterInfo.AMode`，`HumanObject.cs:69-73`）。
    ///
    /// 由 `C.ChangeAMode` 修改（`MirConnection.cs:1432-1439`），判定见
    /// [`World::is_attack_target_player`]。默认 [`AttackMode::Peace`]（`AttackMode` 的判别值 0）。
    pub attack_mode: AttackMode,
    /// PK 值（Crystal `CharacterInfo.PKPoints`，`Server/MirDatabase/CharacterInfo.cs:66`）。
    ///
    /// * 击杀白名玩家 +100（`PlayerObject.cs:611`）；
    /// * 每 12 秒自然 -1（`MapObject.cs:212-216`，`Settings.PKDelay = 12`，`Settings.cs:119`）；
    /// * `>= 100` 黄名、`>= 200` 红名（`PlayerObject.cs:1855-1887`）；
    /// * 死亡不清零（`Info.PKPoints` 是存档字段，`CharacterInfo.cs:187`）。
    ///
    /// 简化：本作还没有把 PK 值写进 `mir-store`（不在本轮范围内），所以**重启后归零**；
    /// 同一进程内死亡/复活都保留。
    pub pk_points: i32,
    /// 棕名到期时刻（Crystal `MapObject.BrownTime`，`MapObject.cs:59-64`）。
    ///
    /// 攻击非红非棕的玩家时由受害者把攻方设成 `now + Settings.Minute`
    /// （`HumanObject.cs:7199-7200`）；死亡时清成当前时刻（`PlayerObject.cs:647`）。
    pub brown_time: i64,
    /// 下一次 PK 值自然衰减的时刻（Crystal `MapObject.PKPointTime`，`MapObject.cs:66`、`:212-216`）。
    pub pk_point_time: i64,
    /// 最后一个造成伤害的对象 ID（Crystal `MapObject.LastHitter`，`MapObject.cs:153`）。
    ///
    /// 死亡时用它区分「被玩家杀死」与「被怪杀死」（`PlayerObject.cs:599`），
    /// 并在 [`LAST_HITTER_TIMEOUT_MS`] 后清空（`MapObject.cs:218-221`）。
    pub last_hitter: Option<u32>,
    /// `last_hitter` 的保留截止时刻（Crystal `MapObject.LastHitTime`，`MapObject.cs:66`）。
    pub last_hit_time: i64,
    /// 当前名字颜色（Crystal `MapObject.NameColour`，`MapObject.cs:67`）。
    ///
    /// 由 [`World::refresh_name_colour`] 每个 tick 重算，变化时发
    /// `S.ColourChanged`（本人）与 `S.ObjectColourChanged`（周围玩家）
    /// （`PlayerObject.RefreshNameColour`，`PlayerObject.cs:1838-1847`）。
    pub name_colour: mir_core::Color,
    /// 自己**已经看见**的怪物，用于 `ObjectMonster`/`ObjectRemove` 的增量下发。
    pub visible_monsters: BTreeSet<u32>,
    /// 自己**已经看见**的 NPC（`S.ObjectNPC`）。目前只在进图/切图时整批下发，
    /// 移动后的增量刷新还没做（Crystal 的 `AddObjects/RemoveObjects` 会逐个比对，
    /// `HumanObject.cs:7864`、`:8152`）；切图时用它把旧图的 NPC 从视野里抹掉。
    pub visible_npcs: BTreeSet<u32>,
    /// 切图后的移动冷却截止时刻：`Envir.Time < MovementTime` 时不再检查传送点
    /// （Crystal `PlayerObject.MovementTime`，`Server/MirObjects/PlayerObject.cs:114`、
    /// 只由 `CompleteMapMovement` 设为 `Envir.Time + MovementDelay`，`:4564`）。
    pub movement_time: i64,
    /// 背包（Crystal `CharacterInfo.Inventory`，46 格；见 [`crate::items::INVENTORY_SIZE`]）。
    pub inventory: Vec<Option<UserItem>>,
    /// 装备（Crystal `CharacterInfo.Equipment`，14 槽；见 [`crate::items::EQUIPMENT_SLOTS`]）。
    pub equipment: Vec<Option<UserItem>>,
    /// 仓库（Crystal `AccountInfo.Storage = new UserItem[80]`，`Server/MirDatabase/AccountInfo.cs:69`；
    /// 本作落在角色行的 `characters.storage`，见 [`crate::storage`]）。
    pub storage: Vec<Option<UserItem>>,
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
    /// 邮箱状态（Crystal `CharacterInfo.Mail` + `PlayerObject.NextMailTime`/`NewMail`，
    /// `Server/MirDatabase/CharacterInfo.cs:85`、`PlayerObject.cs:21`、`:91`；见 [`crate::mail`]）。
    ///
    /// 登录时由会话从 `mail` 表装入（再过一遍过期过滤）；下线不写回这里——
    /// 邮件是独立的行，世界只产生 [`crate::mail::MailWrite`] 让会话落库。
    pub mail: crate::mail::MailState,
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
    /// 所在队伍的 ID（Crystal `MapObject.GroupMembers != null`，`Server/MirObjects/MapObject.cs:162`）。
    ///
    /// 队伍本体在 [`World::groups`] 里；组队是临时状态，不持久化
    /// （Crystal 的 `CharacterInfo` 存档里没有队伍块）。见 [`crate::group`] 与 `docs/world/group.md`。
    pub group_id: Option<u32>,
    /// 收到的组队邀请（Crystal `PlayerObject.GroupInvitation`，`PlayerObject.cs:173`）。
    ///
    /// Crystal 只在 `Process` 里检查邀请者是否还在线（`:480-481`），没有超时；
    /// 本作额外加了一条 TTL，见 [`crate::group::GROUP_INVITE_EXPIRE_MS`]。
    pub group_invitation: Option<crate::group::GroupInvite>,
    /// 下一次允许邀请的时刻（Crystal `PlayerObject.NextGroupInviteTime`，`PlayerObject.cs:15`、
    /// `Settings.GroupInviteDelay = 2000`，`Server/Settings.cs:336`）。
    pub group_invite_time: i64,
    /// 是否允许组队（Crystal `PlayerObject.AllowGroup` → `CharacterInfo.AllowGroup`，
    /// `Server/MirDatabase/CharacterInfo.cs:62`）。
    ///
    /// 简化：Crystal 把它存在角色存档里（`CharacterInfo.cs:249`、`:476`）；本作不改 `mir-store`，
    /// 所以不持久化，进世界时默认 `true`，可以用 `C.SwitchGroup` 关闭
    /// （偏差说明见 `docs/world/group.md`）。
    pub allow_group: bool,
    /// 所属行会与自己的职位序号（Crystal `PlayerObject.MyGuild` + `MyGuildRank`，
    /// `Server/MirObjects/PlayerObject.cs:232-235`、`:983-986`，见 [`crate::guild`]）。
    ///
    /// 进世界时由 [`World::guild_bind_player`] 按成员表校正（行会已被解散/自己已被踢
    /// 就清空），上线/下线的广播由 [`World::guild_player_online`] /
    /// [`World::guild_player_offline`] 负责。
    pub guild: Option<crate::guild::GuildRef>,
    /// 交易状态（Crystal `PlayerObject.TradePartner` / `TradeLocked` / `TradeGoldAmount`，
    /// `Server/MirObjects/PlayerObject.cs:177-179`；交易格 `Info.Trade`，`CharacterInfo.cs:75`）。
    ///
    /// 交易是**临时状态**，不持久化：下线、切图、走/跑、转身都会中止交易
    /// （Crystal 的挂点见 `docs/world/trade.md`）。见 [`crate::trade`]。
    pub trade: crate::trade::TradeState,
    /// 收到的交易邀请来自哪个对象（Crystal `PlayerObject.TradeInvitation`，
    /// `PlayerObject.cs:175`；接受后清空）。
    pub trade_invitation: Option<u32>,
    /// 下一次允许发起交易的时刻（Crystal `PlayerObject.NextTradeTime`，`PlayerObject.cs:14`；
    /// `Settings.TradeDelay = 2000`，`Server/Settings.cs:337`）。
    pub next_trade_time: i64,
    /// 是否允许别人向自己发起交易（Crystal `PlayerObject.AllowTrade` → `Info.AllowTrade`，
    /// `PlayerObject.cs:157-161`）。
    ///
    /// 简化：Crystal 把它存在 `CharacterInfo.AllowTrade` 里并随存档持久化
    /// （`CharacterInfo.cs:62`、`:249`）；本作不改 `mir-store`，所以进世界时默认 `true`、
    /// 不落盘（偏差说明见 `docs/world/trade.md`）。
    pub allow_trade: bool,
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

    /// 红毒造成的防御倍率（Crystal `ArmourRate`，`HumanObject.cs:714-715`）。
    ///
    /// Crystal 每中一次红毒 `ArmourRate -= 0.10F`；结算时 `armour * ArmourRate`
    /// （`HumanObject.cs:7241`）。本作把倍率夹到 `0.0..=1.0`，避免负防御。
    #[must_use]
    pub fn armour_rate(&self) -> f32 {
        let mut rate = 1.0_f32;
        for poison in &self.magic.poisons {
            if poison.kind == PoisonType::Red {
                rate -= 0.10;
            }
        }
        rate.clamp(0.0, 1.0)
    }

    /// 当前中毒类型（Crystal `MapObject.CurrentPoison`，用于 `S.ObjectPlayer.Poison`）。
    #[must_use]
    pub fn poison_type(&self) -> PoisonType {
        let mut kind = PoisonType::None;
        for poison in &self.magic.poisons {
            kind |= poison.kind;
        }
        kind
    }
}

/// 移动失败原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MoveError {
    /// 对象不在世界里。
    #[error("object {0} is not in the world")]
    NotInWorld(u32),
    /// 对象已死亡，不能行动。
    #[error("object {object_id} is dead")]
    Dead {
        /// 对象 ID。
        object_id: u32,
    },
    /// 上一个动作还没结束。
    #[error("action not finished; cannot move before {until} ms")]
    Busy {
        /// 可以开始下一个动作的时刻。
        until: i64,
    },
    /// 还没走过路，不能跑（Crystal `_stepCounter > 0`）。
    #[error("walk one step before running")]
    CannotRunYet,
    /// 目标格被占用。
    #[error("target cell ({x}, {y}) is occupied")]
    Occupied {
        /// 目标 X。
        x: i32,
        /// 目标 Y。
        y: i32,
    },
    /// 越界。
    #[error("target cell ({x}, {y}) is out of bounds")]
    OutOfBounds {
        /// 目标 X。
        x: i32,
        /// 目标 Y。
        y: i32,
    },
    /// 地形不可走（墙）。
    #[error("target cell ({x}, {y}) is a wall")]
    Blocked {
        /// 目标 X。
        x: i32,
        /// 目标 Y。
        y: i32,
    },
    /// 目标格上的门没开（Crystal `Map.CheckDoorOpen`，`Server/MirEnvir/Map.cs:646-651`）。
    #[error("door {index} at target cell ({x}, {y}) is closed")]
    DoorClosed {
        /// 目标 X。
        x: i32,
        /// 目标 Y。
        y: i32,
        /// 门索引。
        index: u8,
    },
}

/// 攻击失败原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CombatError {
    /// 对象不在世界里。
    #[error("object {0} is not in the world")]
    NotInWorld(u32),
    /// 对象已死亡。
    #[error("object {object_id} is dead and cannot be attacked")]
    Dead {
        /// 对象 ID。
        object_id: u32,
    },
    /// 动作或攻击冷却还没结束。
    #[error("action not finished; cannot attack before {until} ms")]
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

/// 一扇门的运行时状态。
///
/// Crystal 的 `Door` 带 `DoorState` / `LastTick`（`Server/MirEnvir/Map.cs:61-71`），
/// 状态枚举见 `Shared/Enums.cs:1910-1916`（`Closed=0, Opening=1, Open=2, Closing=3`）。
/// 地图文件里每格只记录 `door_index`（`MapCell::door_index`），运行时状态由本结构维护。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DoorRuntime {
    /// 该门第一次出现的格子（Crystal `AddDoor` 的 `Location`，`Map.cs:56`）。
    location: Point,
    /// 开关状态。
    state: DoorState,
    /// 打开时刻：`LastTick + 5000 < now` 时自动关（`Map.cs:659`）。
    last_tick: i64,
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
    /// 地图上的持续法术场（Crystal 的 `SpellObject`，`Server/MirObjects/SpellObject.cs`）。
    ///
    /// 本作没有 `ObjectType.Spell` 这一层对象，火墙用「中心 + 半径 + 到期 + 伤害间隔」
    /// 的轻量结构表示；每 tick 用 AOI 查半径内的玩家/怪物结算伤害（见 [`crate::magic`]）。
    pub(crate) spell_fields: Vec<crate::magic::SpellField>,
    /// 下一次火墙「施法实例」编号（Crystal `_fireWallCastSeq`，`HumanObject.cs:4178`）。
    ///
    /// 地图配置 `FireWallLimit/FireWallCount` 按施法实例计数，超限时一次回收最老实例的
    /// **全部格子**（`HumanObject.cs:4151-4175`、`:4183-4207`）。
    pub(crate) next_spell_cast_id: u32,
    /// 召唤物索引：主人（玩家对象 ID）→ 召唤物对象 ID 列表（按召唤顺序）。
    ///
    /// Crystal 把召唤物挂在 `HumanObject.Pets` 列表上（`HumanObject.cs:317-321`）；
    /// 本作单独维护一份索引，主人下线/死亡、召唤物死亡时 O(1) 找到需要清理的对象，
    /// 不必遍历全部怪物。
    pub(crate) pets: HashMap<u32, Vec<u32>>,
    /// 每个在线玩家的宠物命令（`object_id → PetMode`，Crystal `HumanObject.PMode`）。
    ///
    /// 不放在 [`Player`] 上（理由同 `player_languages`）：`C.ChangePMode` 由 `session.rs`
    /// 调 [`World::set_pet_mode`] 写入；召唤新宠时重置为 `PetMode::Both`。
    pub(crate) pet_modes: HashMap<u32, PetMode>,
    /// 每个在线玩家的当前语言（`object_id → Language`）。
    ///
    /// 不放在 [`Player`] 上是为了避免改动 30+ 处 `Player {}` 字面量（测试/会话层）；
    /// 登录与 `/language` 切换由 `session.rs` 调 [`World::set_player_language`] 写入，
    /// 玩家离开世界时在 [`World::leave`] 里移除，避免对象 ID 复用串语言。
    pub(crate) player_languages: HashMap<u32, Language>,
    /// 每个在线玩家的任务物品栏（`object_id → QuestInventory`，Crystal
    /// `CharacterInfo.QuestInventory`，40 格）。
    ///
    /// 与 [`World::player_languages`] 一样放在 `World` 侧表而不是 [`Player`] 上：避免改动
    /// 30+ 处 `Player {}` 字面量（测试/会话层）。登录时由 `session.rs` 从
    /// `characters.quest_inventory` 解码写入，下线时 [`World::take_quest_inventory`] 取走
    /// 并随角色存盘；[`World::leave`] 里兜底清理，避免对象 ID 复用串栏。
    pub(crate) quest_inventories: HashMap<u32, crate::quest_inventory::QuestInventory>,
    /// 服务器语言包（`config/lang/*.json`，由 `server.rs` 注入）。
    ///
    /// `None` 时 [`World::text`]/[`World::format`] 回落到 `mir_core::i18n` 的内置模板，
    /// 与 [`LanguageLibrary::text`]/[`LanguageLibrary::format`] 同口径。
    pub(crate) languages: Option<Arc<LanguageLibrary>>,
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
    /// 已加载的地形缓存：地图文件名 → 地图（Crystal `Envir.MapList`，
    /// `Server/MirEnvir/Envir.cs:4587-4590`）。按需加载，见 [`World::load_map`]。
    pub(crate) maps: HashMap<String, Arc<mir_world::Map>>,
    /// 走地形判定时的**兜底地图**：由 [`World::with_map`] 设置，代表「世界只有这一张图」
    /// （无数据库的服务器与绝大多数测试都是这种形态）。世界里有按名字加载的地图时，
    /// 兜底地图不再参与判定。
    pub(crate) fallback_map: Option<Arc<mir_world::Map>>,
    /// 地图文件根目录（Crystal `Settings.MapPath`，`Server/Settings.cs`）。
    pub(crate) map_root: Option<std::path::PathBuf>,
    /// 地图缓存上限（`MIR2_MAP_CACHE`，0 = 不限制）。超出时优先淘汰没有对象的地图。
    pub(crate) map_cache_limit: usize,
    /// 加载失败过的地图文件名：避免每次切图都重读一遍磁盘。
    pub(crate) missing_maps: BTreeSet<String>,
    /// 每张图的传送点（Crystal `MapInfo.Movements`，`Server/MirDatabase/MovementInfo.cs`）。
    ///
    /// 在地图首次进入时从 `Server.MirDB` 灌入（见 [`World::initialize_map`]），
    /// 也可以由测试/工具用 [`World::set_map_movements`] 直接设置。
    pub(crate) movements: HashMap<String, Arc<Vec<mir_db::MovementInfo>>>,
    /// 已经初始化过（刷 NPC + 注册刷怪点）的地图：Crystal 在 `Map.Load` 里一次性建好
    /// （`Server/MirEnvir/Map.cs:473-496`），本作推迟到「第一次有人进入这张图」。
    pub(crate) initialized_maps: BTreeSet<String>,
    /// 排队中的传送点触发：走/跑/转身命中 `MovementInfo.Source` 后**下一个 tick** 结算
    /// （Crystal 上游在 `HumanObject.Process` 里每 tick 调 `MapTime()`，见 `docs/world/maps.md`）。
    pub(crate) pending_map_moves: Vec<PendingMapMove>,
    /// 已挖开的洞（`need_hole` 传送点要求同格有洞才能通过，Crystal 检查同格的
    /// `Spell.DigOutZombie` / `Spell.DigOutArmadillo` 法术对象，`PlayerObject.cs:4480-4487`）。
    ///
    /// 简化：本作没有挖洞技能，用一个 `(地图, 坐标)` 集合表示「洞已打开」，
    /// 由 GM 命令/测试用 [`World::set_hole`] 打开。
    pub(crate) holes: BTreeSet<(String, Point)>,
    /// 服务器时钟起点（Crystal `Envir.Time` 的 0 点）。
    started: Instant,
    /// 世界创建时刻的 Unix 毫秒（[`World::wall_now`] 的基准）。
    started_unix_ms: i64,
    /// 测试用的时钟偏移（毫秒）。生产环境恒为 0，见 [`World::advance_clock`]。
    clock_offset: i64,
    /// 时钟是否被 [`World::advance_clock`] 冻结（冻结后不再叠加真实流逝时间）。
    clock_frozen: bool,
    /// 所有队伍（Crystal 里每支队伍就是一个被成员共享的 `List<PlayerObject>`，
    /// `Server/MirObjects/PlayerObject.cs:9437`；本作用「队伍 ID → 成员列表」表示，见 [`crate::group`]）。
    pub(crate) groups: HashMap<u32, crate::group::Group>,
    /// 下一个队伍 ID（Crystal 没有这个字段，靠对象引用区分）。
    pub(crate) next_group_id: u32,
    /// 所有行会（Crystal `Envir.Guilds`，`Server/MirEnvir/Envir.cs:147`；见 [`crate::guild`]）。
    pub(crate) guilds: HashMap<i32, crate::guild::Guild>,
    /// 下一个行会 ID（Crystal `Envir.NextGuildID`，`Envir.cs:126`）。
    pub(crate) next_guild_id: i32,
    /// 待接受的行会邀请：玩家对象 ID → 行会 ID
    /// （Crystal `PlayerObject.PendingGuildInvite`，`PlayerObject.cs:143`）。
    pub(crate) guild_invites: HashMap<u32, i32>,
    /// 行会相关配置（Crystal `Settings.Guild_*` + `Configs/GuildSettings.ini`）。
    pub(crate) guild_settings: crate::guild::GuildSettings,
    /// 已请求过「仓库列表」的玩家（Crystal `PlayerObject.GuildCanRequestItems == false`，
    /// `PlayerObject.cs:146`）。
    ///
    /// Crystal 把标志放在玩家对象上：请求一次后置 false，加入/创建行会时重置 true
    /// （`:9831`、`:10006`），下线即随对象销毁。本作同样不持久化，放在世界上按对象 ID 记，
    /// 避免给 [`Player`] 增加只给行会用的一次性字段（与 [`World::guild_invites`] 同一做法）。
    pub(crate) guild_storage_requested: BTreeSet<u32>,
    /// 已经收到过「行会 buff 可选列表」的玩家（Crystal `PlayerObject.RequestedGuildBuffInfo`，
    /// `PlayerObject.cs:147`）：登录时发一次（`:1223-1224`），之后 `C.GuildBuffUpdate{Action=0}`
    /// 不再回（`:10358`）。下线即失效（Crystal 里标志随对象销毁）。
    pub(crate) guild_buff_info_sent: BTreeSet<u32>,
    /// 下一次处理行会 buff 到期的时刻（Crystal `guildTime = Time + Settings.Minute`，
    /// `Envir.cs:2330-2337`；行会 `Process` 每分钟跑一次，`ActiveTimeRemaining` 每次减 1 分钟）。
    pub(crate) next_guild_buff_tick: i64,
    /// 待落盘的行会写入队列。
    ///
    /// 世界层是同步的，而存储层是异步的：行会操作只把「要写什么」记在这里，
    /// 由会话（[`crate::session::Session`]）在包处理完后取走写库；测试可以直接
    /// [`World::take_guild_writes`] 检查/应用。
    pub(crate) guild_writes: Vec<GuildWrite>,
    /// 已经把仓库列表（`S.UserStorage`）发给过的玩家。
    ///
    /// Crystal 用每个连接上的 `MirConnection.StorageSent`（`Server/MirNetwork/MirConnection.cs:70`）：
    /// `PlayerObject.SendStorage` 里有 `if (Connection.StorageSent) return;`（`PlayerObject.cs:8169-8170`），
    /// 所以**每个连接只下发一次**仓库列表。本作按对象 ID 记，下线即随对象销毁。
    pub(crate) storage_sent: BTreeSet<u32>,
    /// 待落盘的邮件写入队列（见 [`crate::mail::MailWrite`]）。
    pub(crate) mail_writes: Vec<crate::mail::MailWrite>,
    /// 下一个邮件编号（Crystal `Envir.NextMailID`，`Server/MirEnvir/Envir.cs:149`）。
    ///
    /// 第一次用到时按「Unix 毫秒 << 20」起跳，见 [`World::allocate_mail_id`]。
    pub(crate) next_mail_id: u64,
    /// 全部攻城区（Crystal `Envir.Conquests`，`Server/MirEnvir/Envir.cs:148`；
    /// 见 [`crate::conquest`]）。
    ///
    /// 由 [`World::init_conquests`] 从 `db.conquests` 建出来，再由
    /// [`World::load_conquests`] 用库里的状态覆盖。
    pub(crate) conquests: Vec<crate::conquest::Conquest>,
    /// 攻城配置（**本作扩展**，Crystal 没有攻城的等级/金币门槛，
    /// 见 [`crate::conquest::ConquestSettings`]）。
    pub(crate) conquest_settings: crate::conquest::ConquestSettings,
    /// 下一次跑攻城处理（`Envir.Process` 每 10 秒一次，`Envir.cs:2339-2346`）的时刻。
    pub(crate) next_conquest_tick: i64,
    /// 市场（摆摊/寄售/拍卖）条目，等价于 Crystal 的 `Envir.Auctions`
    /// （`Server/MirEnvir/Envir.cs:150`；最新的在**最前面**，源码用 `AddFirst`，
    /// `PlayerObject.cs:8302`）。见 [`crate::market`]。
    ///
    /// 简化：Crystal 把条目写进存档（`AuctionInfo.Save`，`AuctionInfo.cs:71-87`）；
    /// 本作只在内存里，重启即丢。
    pub(crate) auctions: Vec<crate::market::Auction>,
    /// 下一个拍卖编号（Crystal `Envir.NextAuctionID`，`AuctionInfo.cs:33`）。
    pub(crate) next_auction_id: u64,
    /// 每个玩家的市场浏览状态（Crystal `PlayerObject.Search`/`PageSent`/`MatchName` 等，
    /// `PlayerObject.cs:132-139`）：对象 ID → 状态，下线即清。
    pub(crate) market_searches: HashMap<u32, crate::market::MarketSearchState>,
    /// 每张地图的 AOI 网格（Crystal 每张 `Map` 的 `Cells[x, y].Objects`，
    /// `Server/MirEnvir/Map.cs:2475-2518`；见 [`crate::aoi`]）。
    ///
    /// 地图名 → 网格。玩家/怪物/NPC/地面物品的生成、移动、消失都必须同步维护，
    /// 否则查询会读到旧坐标（维护点见各 `aoi_*` 调用处）。
    pub(crate) aoi: HashMap<String, AoiGrid>,
    /// 玩家**已经看见**的其它玩家：对象 ID → 可见的玩家 ID 集合。
    ///
    /// Crystal 用 `AddObjects/RemoveObjects` 在移动时双向维护（`HumanObject.cs:7864-8440`），
    /// 进入视野的双方各收一次 `S.ObjectPlayer`、离开视野的双方各收一次 `S.ObjectRemove`
    /// （`HumanObject.Add/Remove`，`HumanObject.cs:8440-8457`）。
    ///
    /// 简化：Crystal 把可见集合隐含在客户端的对象表里，本作显式保存一份用于差分；
    /// 下线/切图时由 [`World::leave`] / `change_map` 清理，避免发重复的移除包。
    pub(crate) visible_players: HashMap<u32, BTreeSet<u32>>,
    /// 怪物 ID → 在 [`World::monsters`] 里的下标，避免按 ID 查询时线性扫描
    /// （地图上怪物数量远大于视野内数量）。
    pub(crate) monster_slots: HashMap<u32, usize>,
    /// 地面物品 ID → 在 [`World::ground_items`] 里的下标，理由同 [`World::monster_slots`]。
    pub(crate) ground_item_slots: HashMap<u32, usize>,
    /// 每张地图的门的运行时状态：地图名 → (门索引 → 状态)。
    ///
    /// 地图文件只给出每格的 `door_index`（[`mir_world::MapCell::door_index`]），
    /// 运行时状态照 Crystal `Map.Doors`（`Server/MirEnvir/Map.cs:50-71`）由世界维护；
    /// 见 [`World::register_map_doors`] / [`World::open_door`] / `tick_doors`。
    pub(crate) doors: HashMap<String, HashMap<u8, DoorRuntime>>,
}

/// 一条待落盘的行会变更（见 `World::guild_writes`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuildWrite {
    /// 整份保存一个行会（含职位、成员、仓库与行会增益）。
    Save(Box<crate::store::GuildRecord>),
    /// 删除一个行会（解散，或最后一个成员退出）。
    Delete {
        /// 行会 ID。
        guild_id: i32,
    },
    /// 写角色的行会归属（`-1` = 退出）。
    SetCharacterGuild {
        /// 角色 ID。
        character_id: i64,
        /// 行会 ID；`-1` 表示没有行会。
        guild_id: i32,
    },
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
            spell_fields: Vec::new(),
            next_spell_cast_id: 1,
            pets: HashMap::new(),
            pet_modes: HashMap::new(),
            player_languages: HashMap::new(),
            quest_inventories: HashMap::new(),
            languages: None,
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
            maps: HashMap::new(),
            fallback_map: None,
            map_root: None,
            map_cache_limit: map_cache_limit_from_env(),
            missing_maps: BTreeSet::new(),
            movements: HashMap::new(),
            initialized_maps: BTreeSet::new(),
            pending_map_moves: Vec::new(),
            holes: BTreeSet::new(),
            started: Instant::now(),
            started_unix_ms: unix_now_ms(),
            clock_offset: 0,
            clock_frozen: false,
            groups: HashMap::new(),
            next_group_id: 1,
            guilds: HashMap::new(),
            next_guild_id: 1,
            guild_invites: HashMap::new(),
            guild_settings: crate::guild::GuildSettings::default(),
            guild_storage_requested: BTreeSet::new(),
            guild_buff_info_sent: BTreeSet::new(),
            next_guild_buff_tick: 0,
            guild_writes: Vec::new(),
            storage_sent: BTreeSet::new(),
            mail_writes: Vec::new(),
            next_mail_id: 0,
            conquests: Vec::new(),
            conquest_settings: crate::conquest::ConquestSettings::default(),
            next_conquest_tick: 0,
            auctions: Vec::new(),
            next_auction_id: 0,
            market_searches: HashMap::new(),
            aoi: HashMap::new(),
            visible_players: HashMap::new(),
            monster_slots: HashMap::new(),
            ground_item_slots: HashMap::new(),
            doors: HashMap::new(),
        }
    }

    /// 用真实地图构造：行走判定会查地图的可走性。
    ///
    /// 这张地图是**兜底地图**（世界只有一张图时用它，见 [`World::fallback_map`]）。
    /// 多地图世界请改用 [`World::set_map_root`] + [`World::load_map`] / [`World::insert_map`]。
    #[must_use]
    pub fn with_map(map: Arc<mir_world::Map>) -> Self {
        let bound = map.width.max(map.height).saturating_sub(1);
        Self {
            players: HashMap::new(),
            monsters: Vec::new(),
            npcs: Vec::new(),
            pending_hits: Vec::new(),
            pending_magic: Vec::new(),
            spell_fields: Vec::new(),
            next_spell_cast_id: 1,
            pets: HashMap::new(),
            pet_modes: HashMap::new(),
            player_languages: HashMap::new(),
            quest_inventories: HashMap::new(),
            languages: None,
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
            maps: HashMap::new(),
            fallback_map: Some(map),
            map_root: None,
            map_cache_limit: map_cache_limit_from_env(),
            missing_maps: BTreeSet::new(),
            movements: HashMap::new(),
            initialized_maps: BTreeSet::new(),
            pending_map_moves: Vec::new(),
            holes: BTreeSet::new(),
            started: Instant::now(),
            started_unix_ms: unix_now_ms(),
            clock_offset: 0,
            clock_frozen: false,
            groups: HashMap::new(),
            next_group_id: 1,
            guilds: HashMap::new(),
            next_guild_id: 1,
            guild_invites: HashMap::new(),
            guild_settings: crate::guild::GuildSettings::default(),
            guild_storage_requested: BTreeSet::new(),
            guild_buff_info_sent: BTreeSet::new(),
            next_guild_buff_tick: 0,
            guild_writes: Vec::new(),
            storage_sent: BTreeSet::new(),
            mail_writes: Vec::new(),
            next_mail_id: 0,
            conquests: Vec::new(),
            conquest_settings: crate::conquest::ConquestSettings::default(),
            next_conquest_tick: 0,
            auctions: Vec::new(),
            next_auction_id: 0,
            market_searches: HashMap::new(),
            aoi: HashMap::new(),
            visible_players: HashMap::new(),
            monster_slots: HashMap::new(),
            ground_item_slots: HashMap::new(),
            doors: HashMap::new(),
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
    ///
    /// 只对「单地图世界」有意义：返回 [`World::with_map`] 设置的兜底地图。
    /// 多地图世界请用 [`World::map_of`] 按地图文件名取。
    #[must_use]
    pub fn map(&self) -> Option<&Arc<mir_world::Map>> {
        self.fallback_map.as_ref()
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

    /// 注入服务器语言包（`config/lang/*.json`，`server.rs` 启动时调用）。
    ///
    /// 不注入也能工作：[`World::text`]/[`World::format`] 会回落到 `mir_core::i18n`
    /// 的内置模板，只是不享受语言包覆盖层。
    pub fn set_languages(&mut self, languages: Arc<LanguageLibrary>) {
        self.languages = Some(languages);
    }

    /// 设置玩家当前语言：登录时按账号语言写入，`/language` 切换成功后同步。
    pub fn set_player_language(&mut self, object_id: u32, language: Language) {
        self.player_languages.insert(object_id, language);
    }

    /// 玩家当前语言；对象不在世界里时按服务器默认语言（简体中文）处理。
    #[must_use]
    pub fn player_language(&self, object_id: u32) -> Language {
        self.player_languages
            .get(&object_id)
            .copied()
            .unwrap_or(Language::ZhCn)
    }

    /// 按玩家语言取一条无参文本（语言包覆盖层优先，缺失时回落内置模板）。
    #[must_use]
    pub fn text(&self, object_id: u32, key: TextKey) -> String {
        let language = self.player_language(object_id);
        self.languages.as_ref().map_or_else(
            || mir_core::i18n::text(language, key).to_owned(),
            |library| library.text(language, key),
        )
    }

    /// 按玩家语言渲染一条带参文本（`{name}` 占位符，缺参原样保留）。
    #[must_use]
    pub fn format(&self, object_id: u32, key: TextKey, args: &[(&str, &str)]) -> String {
        let language = self.player_language(object_id);
        self.languages.as_ref().map_or_else(
            || mir_core::i18n::format_text(language, key, args),
            |library| library.format(language, key, args),
        )
    }

    /// 向某张地图上的所有玩家广播一条系统聊天，**逐收件人**按各自语言渲染
    /// （多语言项目不能只发一份默认语言；顺序按玩家对象 ID 升序，保证可回放）。
    pub fn broadcast_text(&mut self, map: &str, key: TextKey, args: &[(&str, &str)]) {
        let mut recipients: Vec<u32> = self
            .players
            .iter()
            .filter(|(_, player)| player.map == map)
            .map(|(object_id, _)| *object_id)
            .collect();
        recipients.sort_unstable();
        for object_id in recipients {
            let message = self.format(object_id, key, args);
            self.send_system_chat(object_id, &message);
        }
    }

    /// 按对象 ID 取怪物。
    #[must_use]
    pub fn monster(&self, object_id: u32) -> Option<&Monster> {
        let index = *self.monster_slots.get(&object_id)?;
        self.monsters
            .get(index)
            .filter(|monster| monster.object_id == object_id)
    }

    /// 按对象 ID 取怪物（可变）。
    pub fn monster_mut(&mut self, object_id: u32) -> Option<&mut Monster> {
        let index = *self.monster_slots.get(&object_id)?;
        self.monsters
            .get_mut(index)
            .filter(|monster| monster.object_id == object_id)
    }

    /// 重建「怪物 ID → 下标」表。
    ///
    /// 只在 `monsters` 删除元素（尸体到期）后调用：`Vec::remove` 会让后面的下标整体前移，
    /// 与其逐个修补，不如 O(N) 重建一次；删除远少于查询。
    pub(crate) fn rebuild_monster_slots(&mut self) {
        self.monster_slots.clear();
        for (index, monster) in self.monsters.iter().enumerate() {
            self.monster_slots.insert(monster.object_id, index);
        }
    }

    /// 重建「地面物品 ID → 下标」表（理由同 [`World::rebuild_monster_slots`]）。
    pub(crate) fn rebuild_ground_item_slots(&mut self) {
        self.ground_item_slots.clear();
        for (index, ground) in self.ground_items.iter().enumerate() {
            self.ground_item_slots.insert(ground.object_id, index);
        }
    }

    /// 把对象登记进某张图的 AOI 网格。
    pub(crate) fn aoi_insert(&mut self, map: &str, object_id: u32, kind: AoiKind, position: Point) {
        self.aoi
            .entry(map.to_owned())
            .or_default()
            .insert(AoiEntry {
                object_id,
                kind,
                position,
            });
    }

    /// 把对象从某张图的 AOI 网格里删除。
    pub(crate) fn aoi_remove(&mut self, map: &str, kind: AoiKind, object_id: u32) {
        if let Some(grid) = self.aoi.get_mut(map) {
            grid.remove(kind, object_id);
        }
    }

    /// 把对象在 AOI 网格里移动到新坐标；对象未登记时返回 `false`。
    pub(crate) fn aoi_move(
        &mut self,
        map: &str,
        kind: AoiKind,
        object_id: u32,
        position: Point,
    ) -> bool {
        self.aoi
            .get_mut(map)
            .is_some_and(|grid| grid.move_to(kind, object_id, position))
    }

    /// 查询切比雪夫半径内的全部对象（按 `(y, x, ID)` 排序，等价 Crystal 逐格扫描顺序）。
    #[must_use]
    pub(crate) fn aoi_in_radius(&self, map: &str, center: Point, radius: i32) -> Vec<AoiEntry> {
        self.aoi
            .get(map)
            .map_or_else(Vec::new, |grid| grid.query_radius(center, radius))
    }

    /// 查询某一格上的全部对象。
    #[must_use]
    pub(crate) fn aoi_in_cell(&self, map: &str, cell: Point) -> Vec<AoiEntry> {
        self.aoi
            .get(map)
            .map_or_else(Vec::new, |grid| grid.query_cell(cell))
    }

    /// 半径内某一类对象的 ID（升序，去重）。
    #[must_use]
    pub(crate) fn aoi_ids_in_radius(
        &self,
        map: &str,
        center: Point,
        radius: i32,
        kind: AoiKind,
    ) -> Vec<u32> {
        let mut ids: Vec<u32> = self
            .aoi_in_radius(map, center, radius)
            .into_iter()
            .filter(|entry| entry.kind == kind)
            .map(|entry| entry.object_id)
            .collect();
        ids.sort_unstable();
        ids
    }

    /// 半径内某一类对象的 ID（按 `(y, x, ID)` 排序）。
    #[must_use]
    pub(crate) fn aoi_ids_in_radius_spatial(
        &self,
        map: &str,
        center: Point,
        radius: i32,
        kind: AoiKind,
    ) -> Vec<u32> {
        self.aoi_in_radius(map, center, radius)
            .into_iter()
            .filter(|entry| entry.kind == kind)
            .map(|entry| entry.object_id)
            .collect()
    }

    /// 某一格上某一类对象的 ID（升序）。
    #[must_use]
    pub(crate) fn aoi_ids_in_cell(&self, map: &str, cell: Point, kind: AoiKind) -> Vec<u32> {
        let mut ids: Vec<u32> = self
            .aoi_in_cell(map, cell)
            .into_iter()
            .filter(|entry| entry.kind == kind)
            .map(|entry| entry.object_id)
            .collect();
        ids.sort_unstable();
        ids
    }

    /// 玩家已经看见的其它玩家（只读）。
    #[must_use]
    pub fn visible_players_of(&self, player_id: u32) -> Option<&BTreeSet<u32>> {
        self.visible_players.get(&player_id)
    }

    /// 测试/GM 辅助：把玩家瞬移到指定地图的指定坐标，并同步 AOI 网格。
    ///
    /// **不在玩家链路调用**——正常移动请走 [`World::walk`]/[`World::run`]/`change_map`，
    /// 它们会自己维护索引与视野。本函数只为测试直接摆位（以及 GM 命令）准备：
    /// `aoi_remove(old)` → 改 `map`/`position` → `aoi_insert(new)`，跨图也安全。
    ///
    /// 返回玩家是否存在于世界里。
    pub fn place_player(&mut self, player_id: u32, map: &str, position: Point) -> bool {
        let Some(old) = self
            .players
            .get(&player_id)
            .map(|player| (player.map.clone(), player.position))
        else {
            return false;
        };
        self.aoi_remove(&old.0, AoiKind::Player, player_id);
        if let Some(player) = self.players.get_mut(&player_id) {
            map.clone_into(&mut player.map);
            player.position = position;
        }
        self.aoi_insert(map, player_id, AoiKind::Player, position);
        true
    }

    /// 测试/GM 辅助：把怪物瞬移到指定地图的指定坐标，并同步 AOI 网格。
    ///
    /// **不在玩家链路调用**；返回怪物是否存在。
    pub fn place_monster(&mut self, monster_id: u32, map: &str, position: Point) -> bool {
        let Some(old) = self
            .monster(monster_id)
            .map(|monster| (monster.map.clone(), monster.position))
        else {
            return false;
        };
        self.aoi_remove(&old.0, AoiKind::Monster, monster_id);
        if let Some(monster) = self.monster_mut(monster_id) {
            map.clone_into(&mut monster.map);
            monster.position = position;
        }
        self.aoi_insert(map, monster_id, AoiKind::Monster, position);
        true
    }

    /// 把地图文件里的门表登记进运行时状态（Crystal `Map.Load` 里的 `AddDoor`，
    /// `Server/MirEnvir/Map.cs:50-59`、`:143`）。每张图只登记一次。
    pub(crate) fn register_map_doors(&mut self, map_file: &str) {
        let Some(map) = self.map_of(map_file) else {
            return;
        };
        let doors: Vec<(u8, Point)> = map
            .doors
            .iter()
            .map(|door| (door.index, door.location))
            .collect();
        let entry = self.doors.entry(map_file.to_owned()).or_default();
        for (index, location) in doors {
            entry.entry(index).or_insert(DoorRuntime {
                location,
                state: DoorState::Closed,
                last_tick: 0,
            });
        }
    }

    /// 某个门当前是否开着：Crystal `Map.CheckDoorOpen`（`Map.cs:646-651`）。
    ///
    /// 格子上没有登记门（`DoorIndex[x, y] == null`）时视为可通过（Crystal 返回 `true`）。
    #[must_use]
    pub fn door_is_open(&self, map_file: &str, door_index: u8) -> bool {
        self.doors
            .get(map_file)
            .and_then(|doors| doors.get(&door_index))
            .is_none_or(|door| door.state == DoorState::Open)
    }

    /// 某扇门的运行时状态（测试/运维观察用）。
    #[must_use]
    pub fn door_state_of(&self, map_file: &str, door_index: u8) -> Option<DoorState> {
        self.doors
            .get(map_file)
            .and_then(|doors| doors.get(&door_index))
            .map(|door| door.state)
    }

    /// 开一扇门：Crystal `PlayerObject.Opendoor`（`Server/MirObjects/PlayerObject.cs:7818-7826`）。
    ///
    /// ```csharp
    /// if (CurrentMap.OpenDoor(Doorindex))
    /// {
    ///     Enqueue(new S.Opendoor { DoorIndex = Doorindex });
    ///     Broadcast(new S.Opendoor { DoorIndex = Doorindex });
    /// }
    /// ```
    ///
    /// 本人收一条、同图视野内的其它玩家收一条；5 秒后由 `tick_doors` 自动关并广播
    /// `S.Opendoor { Close = true }`（`Map.cs:657-665`）。
    /// 返回是否找到了这扇门。
    pub fn open_door(&mut self, player_id: u32, door_index: u8) -> bool {
        let Some(player) = self.players.get(&player_id) else {
            return false;
        };
        let map = player.map.clone();
        let now = self.now();
        let Some(door) = self
            .doors
            .get_mut(&map)
            .and_then(|doors| doors.get_mut(&door_index))
        else {
            return false;
        };
        door.state = DoorState::Open;
        door.last_tick = now;
        let location = door.location;
        let packet = ServerPacket::Opendoor(Opendoor {
            close: false,
            door_index,
        })
        .encode_frame();
        self.send_to_player(player_id, &packet);
        self.broadcast(&map, location, Some(player_id), &packet, player_id);
        true
    }

    /// 门自动关闭：Crystal `Map.Process`（`Server/MirEnvir/Map.cs:657-665`）：
    /// `if ((Doors[i].DoorState == DoorState.Open) && (Doors[i].LastTick + 5000 < Envir.Time))`。
    fn tick_doors(&mut self, now: i64) {
        let mut closing: Vec<(String, u8, Point)> = Vec::new();
        for (map, doors) in &mut self.doors {
            for (index, door) in doors.iter_mut() {
                if door.state == DoorState::Open && door.last_tick + DOOR_OPEN_MS < now {
                    door.state = DoorState::Closed;
                    closing.push((map.clone(), *index, door.location));
                }
            }
        }
        for (map, door_index, location) in closing {
            let packet = ServerPacket::Opendoor(Opendoor {
                close: true,
                door_index,
            })
            .encode_frame();
            self.broadcast(&map, location, None, &packet, 0);
        }
    }

    /// 所有怪物（生成顺序）。
    #[must_use]
    pub fn monsters(&self) -> &[Monster] {
        &self.monsters
    }

    /// 把玩家放进世界，并把它的外观广播给同图可见玩家。
    ///
    /// 进入一张还没初始化过的地图时会先补齐地形/NPC/刷怪点
    /// （Crystal `Envir.GetMap` → `MapInfo.CreateMap` → `Map.Load`，
    /// `Server/MirDatabase/MapInfo.cs:197-215`、`Server/MirEnvir/Map.cs:432-514`）。
    pub fn enter(&mut self, player: Player) {
        let init = self.initialize_map(&player.map);
        if init.initialized {
            tracing::info!(
                map = %player.map,
                npcs = init.npcs,
                groups = init.respawn_groups,
                spawned = init.spawned,
                "首次进入地图：已补齐 NPC 与刷怪点"
            );
        }
        let now = self.now();
        let mut player = player;
        // `RefreshNameColour()` 的初值：新进世界时按 PK 值/棕名时间算一次
        // （Crystal 在 `Load` 之后的第一次 `Process` 里做，这里提前到广播外观之前，
        // 否则周围玩家会先看到一帧白色名字）。
        player.name_colour = self.computed_name_colour(&player, now);
        let appearance = self.appearance_packet(&player);
        // 先让新玩家看到周围的人，再把自己的外观发给别人
        // （Crystal 对视野内每个对象调用 `Add(player)` 下发 `GetInfo()`，`MapObject.cs:286-308`）。
        let view = self.gather_view(&player);
        for (frame, _) in &view.players {
            let _ = player.sender.send(frame.clone());
        }
        for (frame, _) in &view.monsters {
            let _ = player.sender.send(frame.clone());
        }
        for (frame, _) in &view.npcs {
            let _ = player.sender.send(frame.clone());
        }
        for (frame, _) in &view.items {
            let _ = player.sender.send(frame.clone());
        }

        // 周围玩家收到新玩家的外观（Crystal `MapObject.Add(HumanObject)` 的
        // `player.Enqueue(GetInfoEx(player))`，`HumanObject.cs:8447-8457`；用 AOI 网格取半径内玩家）。
        let nearby_players =
            self.aoi_ids_in_radius(&player.map, player.position, VIEW_RANGE, AoiKind::Player);
        for other_id in &nearby_players {
            if let Some(other) = self.players.get(other_id) {
                let _ = other.sender.send(appearance.clone());
            }
        }

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
        player.visible_monsters = view.monster_ids();
        player.visible_npcs = view.npc_ids();
        player.visible_items = view.item_ids();
        let visible_players = view.player_ids();
        // 让后续 `allocate_object_id` 不会撞上这个玩家的 ID
        // （生产路径是先分配再 `enter`，但测试会直接构造对象 ID）。
        self.next_object_id = self.next_object_id.max(player.object_id.wrapping_add(1));
        let object_id = player.object_id;
        let map_name = player.map.clone();
        let position = player.position;
        let has_equipment = player.equipment.iter().any(Option::is_some);
        self.players.insert(object_id, player);
        // 默认语言：登录后由 `session.rs` 按账号语言覆盖；测试/工具直接 `enter` 时保持简体中文。
        self.player_languages
            .entry(object_id)
            .or_insert(Language::ZhCn);
        // AOI：新玩家登记进网格，并把他写进周围玩家的「已看见玩家」集合
        // （Crystal 的 `AddObjects` 是双向的，`HumanObject.cs:8447-8457`）。
        self.aoi_insert(&map_name, object_id, AoiKind::Player, position);
        self.visible_players
            .insert(object_id, visible_players.clone());
        for other_id in visible_players {
            self.visible_players
                .entry(other_id)
                .or_default()
                .insert(object_id);
        }
        // 带着装备登录时要立刻把装备加成算进去（Crystal 登录/换装都会走
        // `RefreshStats`，`HumanObject.cs:1730-1769`）；空装备就不动，避免覆盖测试
        // 直接塞进 `Player::stats` 的自定义数值。
        if has_equipment {
            self.refresh_stats(object_id);
        }
        // 行会：接上成员状态（**不发包**：`S.GuildStatus` 要排在 `UserInformation`
        // 之后，由会话在 `on_start_game` 里调 `guild_player_online` 补发；
        // Crystal `PlayerObject.cs:1309`）。
        self.guild_bind_player(object_id);
    }

    /// 把玩家移出世界，并通知周围玩家移除。
    ///
    /// 顺序照抄 Crystal `PlayerObject.StopGame`：广播 `S.ObjectRemove`（`:403`）→ `LeaveGroup()`
    /// （`:406`，下线即退队，见 [`World::leave_group`]）→ 清理对象。
    pub fn leave(&mut self, object_id: u32) -> Option<Player> {
        let player = self.players.get(&object_id)?;
        let map = player.map.clone();
        let position = player.position;
        let removal = object_remove_frame(object_id);
        let nearby = self.aoi_ids_in_radius(&map, position, VIEW_RANGE, AoiKind::Player);
        for other_id in nearby {
            if other_id == object_id {
                continue;
            }
            if let Some(other) = self.players.get(&other_id) {
                let _ = other.sender.send(removal.clone());
            }
            // 对方已经收到 `ObjectRemove`，同步从「已看见」集合里删掉，避免下次移动重复发移除包。
            if let Some(seen) = self.visible_players.get_mut(&other_id) {
                seen.remove(&object_id);
            }
        }
        self.aoi_remove(&map, AoiKind::Player, object_id);
        self.visible_players.remove(&object_id);
        // 召唤物随主人下线消失（简化：不做 Crystal 的 PetSave/登录重召，见 docs 差异）。
        self.despawn_pets(object_id);
        // `Despawn(); LeaveGroup();`（`PlayerObject.cs:405-406`）。
        self.leave_group(object_id);
        // 行会成员标记离线（Crystal `PlayerObject.cs:397` 的 `MyGuild.PlayerLogged(this, false)`）。
        self.guild_player_offline(object_id);
        // `TradeCancel();`（`PlayerObject.cs:407`）：下线中止交易并把东西退回。
        self.trade_cancel(object_id);
        // 市场浏览状态随对象销毁（Crystal 的 `PlayerObject.Search` 等字段）。
        self.market_searches.remove(&object_id);
        // 语言映射随对象销毁：对象 ID 会被复用，不清掉会让新玩家继承旧语言。
        self.player_languages.remove(&object_id);
        self.pet_modes.remove(&object_id);
        self.players.remove(&object_id)
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
        // 转身也算一次移动检查：Crystal `PlayerObject.Turn` 在设完方向后调
        // `CheckMovement(CurrentLocation)`（`Server/MirObjects/PlayerObject.cs:4264-4273`），
        // 命中传送点时**不广播** `ObjectTurn`、也不回 `UserLocation`，直接切图。
        if self.queue_movement_trigger(object_id, position) {
            return Ok(());
        }
        // `if (TradePartner != null) TradeCancel();`（`PlayerObject.cs:4300-4301`）：
        // Crystal 在 `Turn` 里、广播 `ObjectTurn` **之前**中止交易。
        self.trade_cancel(object_id);
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
        // 落格前先更新朝向（Crystal `HumanObject.Walk` 的 `Direction = dir` 在
        // `CheckMovement(location)` 之前，`Server/MirObjects/HumanObject.cs:2465-2466`）。
        // 隐身也在这里解除（`:2460-2463`，在 `Direction = dir` 之前）。
        self.remove_hiding(object_id);
        if let Some(player) = self.players.get_mut(&object_id) {
            player.direction = direction;
        }
        // 传送点：整次走动作**作废**（不广播 `ObjectWalk`、不回 `UserLocation`），
        // 由下一个 tick 结算切图（Crystal 在 `Walk` 里同步 `CompleteMapMovement`）。
        if self.queue_movement_trigger(object_id, to) {
            return Ok(from);
        }

        if let Some(player) = self.players.get_mut(&object_id) {
            player.position = to;
            player.direction = direction;
            player.step_counter += 1;
            player.cell_time = now + CELL_GRACE_MS;
            player.action_time = now + MOVE_DELAY_MS;
        }
        // Crystal `GetCell(CurrentLocation).Remove(this); … GetCell(CurrentLocation).Add(this);`
        // （`HumanObject.cs:2468-2474`）：AOI 网格随落格同步移动。
        self.aoi_move(&map, AoiKind::Player, object_id, to);
        // `Moved()`（`PlayerObject.cs:568-574`）在 `S.UserLocation`/`S.ObjectWalk` 之前中止交易
        // （Crystal `HumanObject.cs:2491-2497`）。
        self.trade_cancel(object_id);
        let packet = ServerPacket::ObjectWalk(ObjectWalk {
            object_id,
            location: to,
            direction,
        })
        .encode_frame();
        self.broadcast(&map, to, Some(object_id), &packet, object_id);
        self.send_user_location(object_id);
        // 走完之后刷新自己的视野（Crystal `AddObjects/RemoveObjects`，
        // `HumanObject.cs:2470-2476`、`:7864`、`:8152`）：玩家 → 怪物 → 地面物品的进入/离开包。
        self.sync_player_players(object_id);
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
        // Crystal 在跑之前就把朝向改掉（`HumanObject.cs:2549`），逐格检查里命中
        // 传送点会整次作废（`:2584` 的 `if (CheckMovement(location)) return false;`）。
        // 隐身先解除（`:2540-2545`，`if (Hidden && !Sneaking)`）。
        self.remove_hiding(object_id);
        if let Some(player) = self.players.get_mut(&object_id) {
            player.direction = direction;
        }
        for step in 1..=steps {
            let cell = from.moved(direction, step);
            if let Err(error) = self.check_cell(&map, cell, object_id, now) {
                self.send_user_location(object_id);
                return Err(error);
            }
            if self.queue_movement_trigger(object_id, cell) {
                return Ok(from);
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
        // 跑动同样先摘旧格再挂新格（Crystal `HumanObject.cs:2596-2602`）。
        self.aoi_move(&map, AoiKind::Player, object_id, destination);
        // `Moved()`（`PlayerObject.cs:568-574`）在 `S.UserLocation`/`S.ObjectRun` 之前中止交易
        // （Crystal `HumanObject.cs:2547`）。
        self.trade_cancel(object_id);
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

    /// 尚未结算的延迟伤害队列（只读）。
    ///
    /// 对应 Crystal 挂在每个对象 `ActionList` 上的 `DelayedType.Damage`
    /// （`Server/MirObjects/MapObject.cs:228-233`）；测试与运维用它观察「动作已广播、
    /// 伤害还没落地」的中间状态。
    #[must_use]
    pub fn pending_hits(&self) -> &[PendingHit] {
        &self.pending_hits
    }

    /// 玩家物理攻击（Crystal `HumanObject.Attack` 的近战分支，`HumanObject.cs:2848-3204`）。
    ///
    /// 流程：`CanAttack` → 本人 `UserLocation` + 周围 `ObjectAttack` → 计时 →
    /// 抽伤害 → 前方一格取第一个可攻击对象 → 排入 300ms 后的延迟伤害。
    ///
    /// 目标可以是怪物（现有分支）或玩家（PK，见 [`World::is_attack_target_player`]），
    /// 选择顺序照抄 Crystal：遍历格子里的对象，第一个 `IsAttackTarget` 为真的对象被选中
    /// （`HumanObject.cs:3029-3035`、`:3201-3203`）；对象按创建顺序排列，等价于本作的
    /// 「对象 ID 升序」。守卫不可被攻击（`Guard.IsAttackTarget` 恒为 false，
    /// `Monsters/Guard.cs:54-55`）。
    ///
    /// 简化：不处理技能（`Spell`）、幸运/诅咒开关、`Fencing`/`MoonLight` 等加成。
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
        if !self.walkable_on(&map, target_cell) {
            return Ok(None);
        }
        let Some(target_id) = self.melee_target(object_id, &map, target_cell) else {
            return Ok(None);
        };
        // 记录最近一次攻击目标：召唤物优先打主人的目标（`MonsterObject.cs:1978-1988` 的
        // 反向——本作由 `pet_target` 读取这里）。
        if let Some(player) = self.players.get_mut(&object_id) {
            player.magic.last_target = Some(target_id);
        }
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

    /// 前方格子上第一个可攻击的对象（玩家或怪物）。
    ///
    /// Crystal 遍历 `cell.Objects` 并在第一个 `IsAttackTarget` 为真的对象上 `break`
    /// （`HumanObject.cs:3029-3203`）。对象在格子里按加入顺序排列，而对象 ID 是全局
    /// 递增分配的，所以这里按 **对象 ID 升序**合并玩家与怪物，等价于 Crystal 的格子顺序。
    ///
    /// * 玩家：交给 [`World::is_attack_target_player`]（安全区、攻击模式、红棕名等）；
    /// * 怪物：守卫永远不可攻击（`Monsters/Guard.cs:54-55`），其余野生怪可以。
    fn melee_target(&self, attacker_id: u32, map: &str, cell: Point) -> Option<u32> {
        // 只查目标格（Crystal `CurrentMap.GetCell(target).Objects`，`HumanObject.cs:3029`）。
        let mut candidates: Vec<(u32, bool)> = Vec::new();
        for entry in self.aoi_in_cell(map, cell) {
            match entry.kind {
                AoiKind::Player => {
                    if self
                        .players
                        .get(&entry.object_id)
                        .is_some_and(|player| !player.dead)
                    {
                        candidates.push((entry.object_id, true));
                    }
                }
                AoiKind::Monster => {
                    if self.monster_is_attack_target_of_player(attacker_id, entry.object_id) {
                        candidates.push((entry.object_id, false));
                    }
                }
                AoiKind::Npc | AoiKind::Item => {}
            }
        }
        candidates.sort_unstable();
        for (object_id, is_player) in candidates {
            if is_player {
                if self.is_attack_target_player(attacker_id, object_id) {
                    return Some(object_id);
                }
                continue;
            }
            // 怪物：守卫不可攻击；宠物按 Crystal `MonsterObject.IsAttackTarget(HumanObject)`
            // （`:2430-2463`）判定（主人/Peace/安全区/组队/行会）。
            if self.monster_is_attack_target_of_player(attacker_id, object_id) {
                return Some(object_id);
            }
        }
        None
    }

    /// 玩家能否攻击某个怪物（含玩家召唤物）：Crystal
    /// `MonsterObject.IsAttackTarget(HumanObject)`（`MonsterObject.cs:2430-2463`）。
    ///
    /// * 守卫：恒 false（`Monsters/Guard.cs:54-57`）；
    /// * 野生怪：true（现有行为不变）；
    /// * 宠物（`Master != null`）：
    ///   - 攻击者 Peace → false；
    ///   - 目标是自己的宠物 → 只有 `AttackMode::All` 才能打（`:2440`）；
    ///   - 攻击者或主人在安全区 → false（`:2441`）；
    ///   - `Group`/`Guild` 按同组/同会排除，`EnemyGuild` 未实现恒 false，
    ///     `RedBrown` 要求主人红名/棕名（`:2443-2462`）。
    #[must_use]
    pub fn monster_is_attack_target_of_player(&self, attacker_id: u32, target_id: u32) -> bool {
        let Some(attacker) = self.players.get(&attacker_id) else {
            return false;
        };
        let Some(target) = self.monster(target_id) else {
            return false;
        };
        if target.dead {
            return false;
        }
        if target.kind.is_guard() {
            return false;
        }
        let Some(master_id) = target.master else {
            return true;
        };
        if attacker.attack_mode == AttackMode::Peace {
            return false;
        }
        if master_id == attacker_id {
            return attacker.attack_mode == AttackMode::All;
        }
        let Some(master) = self.players.get(&master_id) else {
            return false;
        };
        if self.in_safe_zone(attacker_id) || self.in_safe_zone(master_id) {
            return false;
        }
        match attacker.attack_mode {
            AttackMode::All => true,
            AttackMode::Group => !same_group(attacker, master),
            AttackMode::Guild => !same_guild(attacker, master),
            // 行会战争未实现（与 [`World::is_attack_target_player`] 同口径）。
            AttackMode::EnemyGuild | AttackMode::Peace => false,
            AttackMode::RedBrown => {
                master.pk_points >= RED_NAME_PK_POINTS || self.now() < master.brown_time
            }
        }
    }

    /// 玩家能否攻击另一个玩家：Crystal `PlayerObject.IsAttackTarget(HumanObject)`
    /// （`Server/MirObjects/PlayerObject.cs:4650-4674`）。
    ///
    /// ```csharp
    /// if (Dead || InSafeZone || attacker.InSafeZone || attacker == this || GMGameMaster) return false;
    /// if (CurrentMap.Info.NoFight) return false;
    /// switch (attacker.AMode) {
    ///     case All:        return true;
    ///     case Group:      return GroupMembers == null || !GroupMembers.Contains(attacker);
    ///     case Guild:      return MyGuild == null || MyGuild != attacker.MyGuild;
    ///     case EnemyGuild: return MyGuild != null && MyGuild.IsEnemy(attacker.MyGuild);
    ///     case Peace:      return false;
    ///     case RedBrown:   return PKPoints >= 200 || Envir.Time < BrownTime;
    /// }
    /// ```
    ///
    /// * 安全区判定是**双方**都要在区外（`InSafeZone` 是目标，`attacker.InSafeZone` 是攻方）；
    /// * `NoFight` 地图同样禁止（`Server/MirDatabase/MapInfo.cs` 的 `NoFight`）；
    /// * 行会战争（`GuildObject` 的 `WarringGuilds` / `IsEnemy`）尚未实现，所以
    ///   `EnemyGuild` 恒为 false、`Guild` 只按「是否同一行会」判定——
    ///   见 `docs/world/pk.md` 的异常清单。
    #[must_use]
    pub fn is_attack_target_player(&self, attacker_id: u32, target_id: u32) -> bool {
        let Some(attacker) = self.players.get(&attacker_id) else {
            return false;
        };
        let Some(target) = self.players.get(&target_id) else {
            return false;
        };
        if target.dead || attacker_id == target_id {
            return false;
        }
        if self.in_safe_zone(attacker_id) || self.in_safe_zone(target_id) {
            return false;
        }
        if self.map_no_fight(&target.map) {
            return false;
        }
        match attacker.attack_mode {
            AttackMode::All => true,
            AttackMode::Group => !same_group(attacker, target),
            AttackMode::Guild => !same_guild(attacker, target),
            // 行会战争未实现（没有「敌对行会」可言）；和平模式恒 false。
            AttackMode::EnemyGuild | AttackMode::Peace => false,
            AttackMode::RedBrown => {
                target.pk_points >= RED_NAME_PK_POINTS || self.now() < target.brown_time
            }
        }
    }

    /// 世界 tick：玩家簿记 → 到期伤害 → 到期魔法 → 刷怪点 → 怪物 AI。
    ///
    /// 由服务端的定时任务每 100ms 调一次；顺序理由见模块文档的「tick 顺序」。
    pub fn tick(&mut self) {
        let now = self.now();
        // 传送点：走/跑/转身命中后在**下一个 tick** 结算（Crystal 上游在
        // `HumanObject.Process` 里每 tick 调 `MapTime()`；本仓库的 Crystal 分支改为在
        // `CheckMovement` 里同步执行，见 `docs/world/maps.md`）。
        self.resolve_pending_map_moves();
        self.tick_players(now);
        // 任务超时（Crystal 用 `DelayedAction` 排程，本作每 tick 比较；见 `crate::quests`）。
        self.tick_quests(self.wall_now());
        self.resolve_pending_hits(now);
        self.resolve_pending_magic(now);
        // 持续法术场（火墙）：Crystal 的 `SpellObject.Process`（`SpellObject.cs:51-122`）。
        self.tick_spell_fields(now);
        self.tick_respawns(now);
        self.tick_monsters(now);
        // 地面物品到期（Crystal `ItemObject.Process`，`ItemObject.cs:129-142`）。
        // 放在最后：本 tick 新生成/拾取的物品不会在同一 tick 里被判过期。
        self.tick_ground_items(now);
        // 行会 buff 到期：Crystal 在 `Envir.Process` 里每分钟跑一次 `GuildObject.Process`
        // （`Envir.cs:2330-2337`），见 [`crate::guild_buffs`]。
        self.tick_guild_buffs(now);
        // 攻城：Crystal 在 `Envir.Process` 里每 10 秒跑一次 `ConquestObject.Process`
        // （`Envir.cs:2339-2346`），见 [`crate::conquest`]。
        self.tick_conquests(now);
        // 门自动关闭：Crystal `Map.Process`（`Server/MirEnvir/Map.cs:657-665`）。
        self.tick_doors(now);
        // 寄售/拍卖过期：Crystal 在 `Envir.Process` 里逐条标记 `Expired`
        // （`Envir.cs:2396`），见 [`crate::market`]。
        self.tick_market();
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
            // （`HumanObject.cs:323-325`）。毒的 tick 在回复之后。
            self.process_player_regen(object_id, now);
            // 隐身到期（Crystal `ProcessBuffs` 的 `BuffType.Hiding` 分支，
            // `HumanObject.cs:462-468`）。
            self.tick_player_hiding(object_id, now);
            self.tick_player_poison(object_id, now);
            // Crystal 没有服务端复活计时器；到点自动复活是本作的扩展（见 `PLAYER_REVIVE_DELAY_MS`）。
            let due = self.players.get(&object_id).is_some_and(|player| {
                player.dead && player.revive_time > 0 && now >= player.revive_time
            });
            if due {
                self.revive_player(object_id);
            }
            // 新邮件提示：Crystal 在 `PlayerObject.Process` 里检查 `NewMail`
            // （`PlayerObject.cs:499-503`），本作跟着每 tick 的玩家簿记一起做。
            self.process_new_mail(object_id);
            // `MapObject.Process` 的 PK 值衰减与 `LastHitter` 过期（`MapObject.cs:212-221`），
            // 再 `RefreshNameColour()`（`HumanObject.cs:358-360`）。
            self.process_pk_points(object_id, now);
            self.refresh_name_colour(object_id);
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

    /// PK 值的自然衰减与 `LastHitter` 过期：Crystal `MapObject.Process`
    /// （`Server/MirObjects/MapObject.cs:212-221`）。
    ///
    /// ```csharp
    /// if (PKPoints > 0 && Envir.Time > PKPointTime) {
    ///     PKPointTime = Envir.Time + Settings.PKDelay * Settings.Second;   // 12 秒
    ///     PKPoints--;
    /// }
    /// if (LastHitter != null && Envir.Time > LastHitTime) LastHitter = null;
    /// ```
    fn process_pk_points(&mut self, object_id: u32, now: i64) {
        let Some(player) = self.players.get_mut(&object_id) else {
            return;
        };
        if player.pk_points > 0 && now > player.pk_point_time {
            player.pk_point_time = now + PK_DELAY_MS;
            player.pk_points -= 1;
        }
        if player.last_hitter.is_some() && now > player.last_hit_time {
            player.last_hitter = None;
        }
    }

    /// 计算玩家的名字颜色：Crystal `PlayerObject.GetNameColour`（`PlayerObject.cs:1849-1891`）。
    ///
    /// 判定顺序：红名（`PKPoints >= 200`）→ 棕名（`Envir.Time < BrownTime`）→
    /// 战区绿名（`WarZone`）→ 黄名（`PKPoints >= 100`）→ 白名。
    ///
    /// 战区分支的简化：Crystal 在 `WarZone` 里对**观察者**返回绿色（同会/无行会）或橙色
    /// （敌对行会）（`PlayerObject.cs:1861-1870`），而 `RefreshNameColour` 用自己当观察者
    /// 算出的缓存值恒为绿色（`:1840-1846`）。本作只发「自己视角」的颜色，因此战区里
    /// 一律是绿色，敌人看到的也是绿色（见 `docs/world/conquest.md`）。
    fn computed_name_colour(&self, player: &Player, now: i64) -> mir_core::Color {
        if player.pk_points >= RED_NAME_PK_POINTS {
            NAME_COLOUR_RED
        } else if now < player.brown_time {
            NAME_COLOUR_BROWN
        } else if self
            .conquests
            .iter()
            .any(|conquest| conquest.covers_player(player))
        {
            NAME_COLOUR_WAR_ZONE
        } else if player.pk_points >= YELLOW_NAME_PK_POINTS {
            NAME_COLOUR_YELLOW
        } else {
            NAME_COLOUR_WHITE
        }
    }

    /// 某个玩家当前应有的名字颜色（供登录/测试查询）。
    #[must_use]
    pub fn name_colour_for(&self, player_id: u32) -> mir_core::Color {
        self.players
            .get(&player_id)
            .map_or(NAME_COLOUR_WHITE, |player| {
                self.computed_name_colour(player, self.now())
            })
    }

    /// 重算并广播名字颜色：Crystal `PlayerObject.RefreshNameColour`（`PlayerObject.cs:1838-1847`）
    /// 与 `HumanObject.BroadcastColourChange`（`HumanObject.cs:2351-2363`）。
    ///
    /// 颜色变化时：
    ///
    /// * 本人收 `S.ColourChanged { NameColour }`（`PlayerObject.cs:1845`）；
    /// * 同图视野内的其它玩家收 `S.ObjectColourChanged { ObjectID, NameColour }`
    ///   （`HumanObject.cs:2361`）。
    ///
    /// 没有变化时什么也不发（Crystal 在 `prevColor == NameColour` 时直接 return）。
    pub(crate) fn refresh_name_colour(&mut self, object_id: u32) {
        let Some(player) = self.players.get(&object_id) else {
            return;
        };
        let colour = self.computed_name_colour(player, self.now());
        if player.name_colour == colour {
            return;
        }
        let map = player.map.clone();
        let position = player.position;
        if let Some(player) = self.players.get_mut(&object_id) {
            player.name_colour = colour;
        }
        self.send_to_player(
            object_id,
            &ServerPacket::ColourChanged(ColourChanged {
                name_colour: colour,
            })
            .encode_frame(),
        );
        let packet = ServerPacket::ObjectColourChanged(ObjectColourChanged {
            object_id,
            name_colour: colour,
        })
        .encode_frame();
        self.broadcast(&map, position, Some(object_id), &packet, object_id);
    }

    /// `C.ChangeAMode`（Crystal `Server/MirNetwork/MirConnection.cs:1432-1439`）。
    ///
    /// Crystal 只校验阶段（`Stage != GameStage.Game`），不校验模式值本身；
    /// 然后 `Player.AMode = p.Mode` 并给**本人**回一条 `S.ChangeAMode`。
    /// **不广播**给周围玩家——攻击模式是私有状态，源码里没有 `Broadcast`。
    ///
    /// 返回是否找到了玩家。
    pub fn change_attack_mode(&mut self, object_id: u32, mode: AttackMode) -> bool {
        let Some(player) = self.players.get_mut(&object_id) else {
            return false;
        };
        player.attack_mode = mode;
        let _ = player
            .sender
            .send(ServerPacket::ChangeAMode(ChangeAMode { mode }).encode_frame());
        true
    }

    /// 当前攻击模式（登录时补发 `S.ChangeAMode` 用，`PlayerObject.cs:1217`）。
    #[must_use]
    pub fn attack_mode_of(&self, object_id: u32) -> Option<AttackMode> {
        self.players
            .get(&object_id)
            .map(|player| player.attack_mode)
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
        // 目标不是怪物 ⇒ 玩家。攻方是玩家 ⇒ PK 分支（`HumanObject.Attacked(HumanObject)`），
        // 否则是怪物打玩家（`HumanObject.Attacked(MonsterObject)`）。
        if self.players.contains_key(&hit.attacker_id) {
            self.damage_player_from_player(hit, &attacker_stats);
        } else {
            self.damage_player(hit, &attacker_stats);
        }
    }

    /// 怪物被打：Crystal `MonsterObject.Attacked(HumanObject)`（`MonsterObject.cs:2569-2684`）。
    ///
    /// 返回实际扣掉的血量（`damage - armour`，未命中时 0）：魔法伤害要靠它决定
    /// 是否给技能加经验（`HumanObject.cs:5929` 的 `if (target.Attacked(...) > 0) LevelMagic(magic)`）。
    pub(crate) fn damage_monster(&mut self, hit: PendingHit, attacker_stats: &CombatStats) -> i32 {
        // 守卫不可被攻击：`Guard.IsAttackTarget(...)` 恒 false、`Guard.Attacked(...)` 直接抛异常
        // （`Server/MirObjects/Monsters/Guard.cs:54-57`）。物理攻击在 `melee_target` 里已经过滤，
        // 这里兜住技能/其它入口。
        if self
            .monster(hit.target_id)
            .is_some_and(|monster| monster.kind.is_guard())
        {
            return 0;
        }
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
        //
        // 宠物被攻击时把攻击者写回 `pet_target`（Crystal `MonsterObject.Attacked` 的
        // `Target = attacker`，`:2601-2610`；玩家攻击也走 `:2571-2574`）。是否真的能反击
        // 交给 `pet_target_valid`：主人 Peace/安全区/同组同会等会自然拒绝。
        let pet_retaliation = self.monster(hit.target_id).and_then(|pet| {
            let master_id = pet.master?;
            self.pet_target_valid(pet, master_id, hit.attacker_id)
                .then_some(hit.attacker_id)
        });
        let attacker_is_monster = self.monster(hit.attacker_id).is_some();
        if let Some(monster) = self.monster_mut(hit.target_id) {
            if monster.master.is_some() {
                if let Some(attacker_id) = pet_retaliation {
                    monster.pet_target = Some(attacker_id);
                    monster.operate_time = 0;
                }
            } else {
                // `Attacked(MonsterObject)` 是**无条件覆盖**（`MonsterObject.cs:2601-2610`）：
                // 野生怪已经有玩家目标时，被宠物咬也会立刻回身打宠物；玩家攻击则只在
                // 没有目标时设（`Attacked(HumanObject)` 的 `Target == null`，`:2571-2574`）。
                if attacker_is_monster || monster.target.is_none() {
                    monster.target = Some(hit.attacker_id);
                }
                if monster.expo_owner.is_none() {
                    monster.expo_owner = Some(hit.attacker_id);
                }
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
        let result = combat::resolve_monster_attack(
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

        // `LastHitter = attacker.Master ?? attacker; LastHitTime = now + 10000;
        //  RegenTime = now + RegenDelay;`（`HumanObject.cs:7293-7296`）。
        if let Some(player) = self.players.get_mut(&hit.target_id) {
            player.last_hitter = Some(hit.attacker_id);
            player.last_hit_time = now + LAST_HITTER_TIMEOUT_MS;
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

    /// 玩家被玩家打中：Crystal `HumanObject.Attacked(HumanObject)`
    /// （`Server/MirObjects/HumanObject.cs:7092-7221`）。
    ///
    /// 包序（照抄源码；若气功波触发，会先在第 1 步之前发一条 `S.HealthChanged`，
    /// `HumanObject.cs:7145-7154`）：
    ///
    /// | # | 包 | 收件人 | 出处 |
    /// |---|---|---|---|
    /// | 1 | `ObjectEffect{Critical}` | 周围玩家 | `:7158` |
    /// | 2 | `DamageIndicator{Critical, 0}` | 周围玩家 | `:7160` |
    /// | 3 | `Struck` | 受害者本人 | `:7214`（**无** 500ms 节流） |
    /// | 4 | `ObjectStruck` | 周围玩家（含攻击者） | `:7215` |
    /// | 5 | `DamageIndicator{Hit, armour - damage}` | 本人 + 周围玩家 | `:7217` |
    /// | 6 | `HealthChanged` | 本人（+ 同图队友的 `ObjectHealth`） | `ChangeHP` → `SendHealthChanged`（`:7219`） |
    ///
    /// 棕名（`:7199-7200`）：**受害者**既不是棕名也没有红名、且双方不在战争中时，
    /// 攻方 `BrownTime = now + Settings.Minute`。注意判定用的是受害者的
    /// `BrownTime` / `PKPoints`，不是攻方的。
    pub(crate) fn damage_player_from_player(
        &mut self,
        hit: PendingHit,
        attacker_stats: &CombatStats,
    ) {
        let now = self.now();
        let Some((stats, map, dead, position, direction)) = self.player_snapshot(hit.target_id)
        else {
            return;
        };
        if dead {
            return;
        }
        let pvp = combat::resolve_player_vs_player(
            attacker_stats,
            &stats,
            hit.damage,
            hit.defence,
            &mut self.rng,
        );
        if pvp.result.miss {
            self.emit_damage_indicator(&map, position, hit.target_id, 0, DamageType::Miss);
            return;
        }

        // 气功波（`:7145-7154`）：`if (HP + Gain >= Stats[HP]) SetHP(Stats[HP]) else ChangeHP(Gain)`。
        // 注意它在暴击**之前**，会先发一条 `S.HealthChanged`。
        if pvp.energy_shield {
            let gain = stats.energy_shield_hp_gain;
            if gain > 0 {
                let (hp, max_hp) = {
                    let player = &self.players[&hit.target_id];
                    (player.hp, player.stats.hp)
                };
                if hp + gain >= max_hp {
                    self.set_player_hp(hit.target_id, max_hp);
                } else {
                    self.change_player_hp(hit.target_id, gain);
                }
            }
        }

        // 暴击：`CurrentMap.Broadcast(new S.ObjectEffect { Effect = Critical })` +
        // `BroadcastDamageIndicator(DamageType.Critical)`（`HumanObject.cs:7156-7161`）。
        if pvp.result.critical {
            let effect = ServerPacket::ObjectEffect(ObjectEffect {
                object_id: hit.target_id,
                effect: SpellEffect::Critical,
                ..ObjectEffect::default()
            })
            .encode_frame();
            self.broadcast(&map, position, Some(hit.target_id), &effect, hit.target_id);
            self.emit_damage_indicator(&map, position, hit.target_id, 0, DamageType::Critical);
        }

        // 魔法盾：每点实际伤害扣 60ms（`:7163-7167`；ElementalBarrier 未实现）。
        let dealt = pvp.result.damage.max(0);
        self.shave_buff(
            hit.target_id,
            BuffKind::MagicShield,
            i64::from(dealt) * MAGIC_SHIELD_MS_PER_DAMAGE,
        );

        // `LastHitter = attacker; LastHitTime = now + 10000; RegenTime = now + RegenDelay;`
        // （`:7194-7197`）。
        if let Some(player) = self.players.get_mut(&hit.target_id) {
            player.last_hitter = Some(hit.attacker_id);
            player.last_hit_time = now + LAST_HITTER_TIMEOUT_MS;
            player.regen_time = now + PLAYER_REGEN_DELAY_MS;
        }

        // 棕名（`:7199-7200`）：`if (Envir.Time > BrownTime && PKPoints < 200 && !AtWar(attacker))`。
        let (victim_brown, victim_pk) = self
            .players
            .get(&hit.target_id)
            .map_or((0, 0), |player| (player.brown_time, player.pk_points));
        if now > victim_brown
            && victim_pk < RED_NAME_PK_POINTS
            && !self.at_war(hit.target_id, hit.attacker_id)
            && let Some(attacker) = self.players.get_mut(&hit.attacker_id)
        {
            attacker.brown_time = now + BROWN_NAME_DURATION_MS;
        }

        // 受害者本人 `S.Struck`（`HumanObject.cs:7214`，没有 StruckTime 节流）。
        self.send_to_player(
            hit.target_id,
            &ServerPacket::Struck(Struck {
                attacker_id: hit.attacker_id,
            })
            .encode_frame(),
        );

        // 周围玩家 `S.ObjectStruck`（`HumanObject.cs:7215`）：Broadcast 只排除受害者本人，
        // 所以攻方也能看到自己打中的动画。
        let struck = ServerPacket::ObjectStruck(ObjectStruck {
            object_id: hit.target_id,
            attacker_id: hit.attacker_id,
            location: position,
            direction,
        })
        .encode_frame();
        self.broadcast(&map, position, Some(hit.target_id), &struck, hit.target_id);

        // `BroadcastDamageIndicator(DamageType.Hit, armour - damage)`（`:7217`）。
        self.emit_damage_indicator(
            &map,
            position,
            hit.target_id,
            -pvp.result.damage,
            DamageType::Hit,
        );

        // `ChangeHP(armour - damage)`（`:7219`）。
        self.change_player_hp(hit.target_id, -pvp.result.damage);
    }

    /// 玩家死亡：Crystal `PlayerObject.Die`（`PlayerObject.cs:578-667`）。
    ///
    /// 顺序（照抄源码）：
    ///
    /// 1. 击杀者是玩家时结算 PK：`hitter.PKPoints = min(int.MaxValue, PKPoints + 100)`，
    ///    并按 `Random.Next(4) == 0` 给凶手的武器加诅咒（`:599-622`）；
    /// 2. 移除魔法盾/元素屏障（`:635-636`，本作只实现魔法盾）；
    /// 3. 掉落（`:638-641`）：`PKPoints > 200` → `RedDeathDrop`，
    ///    否则**不在安全区** → `DeathDrop`；
    /// 4. `HP = 0; Dead = true; LogTime = BrownTime = Envir.Time`（`:643-647`）；
    /// 5. 本人 `S.Death`（`:649`）、周围玩家 `S.ObjectDied`（`:650`）。
    ///
    /// 简化：复活戒指（`:580-597`）、宠物/英雄清理（`:624-633`）、`RemoveOnDeath` 增益全清
    /// （`:652-659`）、毒清理（`:661`）、`CallDefaultNPC(Die)` 与统计（`:664-666`）。
    /// 复活计时是本作扩展：Crystal 服务端没有自动复活（见 [`PLAYER_REVIVE_DELAY_MS`]）。
    fn player_died(&mut self, player_id: u32) {
        let now = self.now();
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        if player.dead {
            return;
        }
        let position = player.position;
        let direction = player.direction;
        let map = player.map.clone();
        let victim_pk = player.pk_points;
        let victim_brown = player.brown_time;
        let last_hitter = player.last_hitter;
        // 死亡掉落用 `!InSafeZone` 判定（`:640`），必须在 `Dead = true` 之前取。
        let was_in_safe_zone = self.in_safe_zone(player_id);
        // 死亡立刻现形：先发 `S.ObjectHidden{false}` / `S.RemoveBuff`，避免旁人客户端
        // 保留「尸体还在隐身」的状态。
        self.remove_hiding(player_id);

        // 1) PK（`PlayerObject.cs:599-622`）。
        //    注意判定用的是**受害者**的 `BrownTime` / `PKPoints`（源码里的无限定字段），
        //    所以杀死红名/棕名玩家不会给凶手加 PK 值。
        //
        //    攻城期间在战区里杀死对手**不加 PK 值**，只给凶手一条「受到法律保护」的提示
        //    （`if (AtWar(hitter) || WarZone) hitter.ReceiveChat(ProtectedByLaw)`，`:603-606`）。
        if let Some(hitter_id) = last_hitter
            && hitter_id != player_id
            && self.players.contains_key(&hitter_id)
        {
            if self.at_war(player_id, hitter_id) || self.in_war_zone(player_id) {
                let message = self.text(hitter_id, TextKey::PkProtectedByLaw);
                self.send_system_chat(hitter_id, &message);
            } else if now > victim_brown && victim_pk < RED_NAME_PK_POINTS {
                let mut cursed = None;
                if let Some(hitter) = self.players.get_mut(&hitter_id) {
                    hitter.pk_points = hitter.pk_points.saturating_add(PK_POINTS_PER_KILL);
                    // `if (weapon != null && weapon.AddedStats[Luck] > (Settings.MaxLuck * -1)
                    //      && Envir.Random.Next(4) == 0) { weapon.AddedStats[Luck]--; … }`
                    // （`PlayerObject.cs:609-620`）。
                    if let Some(weapon) = hitter.equipment.first_mut().and_then(Option::as_mut) {
                        let luck = weapon.added_stats.get(Stat::Luck);
                        if luck > -combat::MAX_LUCK && self.rng.next_bounded(4) == 0 {
                            weapon.added_stats.set(Stat::Luck, luck - 1);
                            cursed = Some(weapon.clone());
                        }
                    }
                }
                if let Some(item) = cursed {
                    self.send_to_player(
                        hitter_id,
                        &ServerPacket::RefreshItem(RefreshItem { item }).encode_frame(),
                    );
                }
            }
        }

        // 2) `RemoveBuff(BuffType.MagicShield); RemoveBuff(BuffType.ElementalBarrier);`（`:635-636`）。
        self.remove_buff(player_id, BuffKind::MagicShield);

        // 3) 死亡掉落（`:638-641`）。`PKPoints > 200` 是**严格大于**：等于 200 时仍走普通掉落。
        if victim_pk > RED_NAME_PK_POINTS {
            self.drop_player_death_loot(player_id, true);
        } else if !was_in_safe_zone {
            self.drop_player_death_loot(player_id, false);
        }

        // 4) `HP = 0; Dead = true; LogTime = Envir.Time; BrownTime = Envir.Time;`（`:643-647`）。
        if let Some(player) = self.players.get_mut(&player_id) {
            player.hp = 0;
            player.dead = true;
            player.death_time = now;
            player.revive_time = now + PLAYER_REVIVE_DELAY_MS;
            player.brown_time = now;
            // `PoisonList.Clear()`（`PlayerObject.cs:661`）；隐身状态也随死亡清除。
            player.magic.poisons.clear();
            player.magic.hidden = false;
        }
        // `for (int i = 0; i < Pets.Count; i++) Pets[i].Die();`（`PlayerObject.cs:629-633`）。
        self.kill_pets(player_id);
        // 本作扩展：死亡即中止交易并退货。Crystal 的 `Die()` 不清交易状态
        // （`PlayerObject.cs:578-667` 里没有 `TradeCancel`），只靠随后的移动/切图取消；
        // 需求要求死亡中断交易，见 `docs/world/trade.md` 的差异 3。
        self.trade_cancel(player_id);

        // 5) `S.Death` + `S.ObjectDied`（`:649-650`）。
        self.send_to_player(
            player_id,
            &ServerPacket::Death(Death {
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
        for other_id in self.aoi_ids_in_radius(&map, position, VIEW_RANGE, AoiKind::Player) {
            if other_id == player_id {
                continue;
            }
            let Some(other) = self.players.get(&other_id) else {
                continue;
            };
            let _ = other.sender.send(remove.clone());
            let _ = other.sender.send(revived.clone());
            if let Some(frame) = &appearance {
                let _ = other.sender.send(frame.clone());
            }
        }
        self.send_to_player(player_id, &ServerPacket::Revived(Revived {}).encode_frame());
        // `GroupMemberMapNameChanged(); GetPlayerLocation();`（`PlayerObject.cs:1389-1390`，
        // 原地复活同见 `:1448-1449`）：复活后把「我在哪张图/哪个坐标」重新同步给队友。
        self.group_member_map_name_changed(player_id);
        self.group_player_location(player_id);
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

    /// `HumanObject.SendHealthChanged`（`HumanObject.cs:870-874`）：先给本人发 `S.HealthChanged{HP,MP}`，
    /// 再走 `BroadcastHealthChange()`（`MapObject.cs:841-914`）。
    ///
    /// 玩家的 `BroadcastHealthChange` 只发给**同图 `Globals.DataRange` 内的队友**
    /// （`MapObject.cs:856-871`，包是 `S.ObjectHealth`），见
    /// [`World::broadcast_health_to_group`]；不在队伍里就什么也不发（Crystal 也不广播给周围人）。
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
        self.broadcast_health_to_group(object_id);
    }

    /// 击杀经验结算入口：Crystal `MonsterObject.Die` →
    /// `EXPOwner.WinExp(Experience, Level)`（`Server/MirObjects/MonsterObject.cs:982-984`）。
    ///
    /// `amount` 是怪物模板的经验（[`MonsterKind::experience`]），`target_level` 是怪物等级。
    /// Crystal 的 `WinExp` 先过 `ReduceExp` 与 `Settings.ExpRate`（`PlayerObject.cs:794-845`），
    /// 再按「是否组队」分流：
    ///
    /// * **不在队伍里**：`GainExp(expPoint)`（`:834-837`）；
    /// * **在队伍里**：按 [`World::group_exp_shares`] 算出的分配表逐个 `GainExp`
    ///   （`:807-833`）——同图、`Globals.DataRange = 16` 内、活着的成员按等级比例分，
    ///   并乘上人数加成 [`crate::group::PARTY_EXP_RATE`]。
    ///
    /// 简化：`Settings.ExpRate` 默认 1.0（`Server/Settings.cs:112`），没有接入配置；
    /// 英雄（`:839-844`）、地图 `NoExperience`、夫妻/师徒/行会加成仍未实现。
    pub fn win_experience(&mut self, object_id: u32, amount: u32, target_level: u16) {
        let Some(player) = self.players.get(&object_id) else {
            return;
        };
        let exp = reduce_exp(player.level, amount, target_level);
        let exp_point = u32::try_from(exp).unwrap_or(0);
        match self.group_exp_shares(object_id, exp_point) {
            Some(shares) => {
                // `for (...) player.GainExp(...)`（`:824-832`）：按队伍成员顺序逐个发。
                for (member_id, share) in shares {
                    self.gain_experience(member_id, share);
                }
            }
            None => self.gain_experience(object_id, exp_point),
        }
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

        // `if (MyGuild != null && MyGuild.Name != Settings.NewbieGuild) MyGuild.GainExp(amount);`
        // （`PlayerObject.cs:902-903`）：行会经验在「判定升级之前」结算，见 [`crate::guild`]。
        self.guild_gain_exp(object_id, amount);

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
            if !self.walkable_on(&group.map, cell) {
                continue;
            }
            // 占格判定只看这一格（沿用既有语义：有活怪就换一格）；用 AOI 网格
            // 代替遍历全部怪物。
            let occupied = self.aoi_in_cell(&group.map, cell).into_iter().any(|entry| {
                entry.kind == AoiKind::Monster
                    && self
                        .monster(entry.object_id)
                        .is_some_and(|monster| !monster.dead)
            });
            if occupied {
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
        // 不死系标记来自数据库的 `MonsterInfo.Undead`（Crystal `MapObject.Undead`），
        // 本作没有给 `MonsterKind` 加字段，刷怪时按名字回查一次（`ThunderBolt` 用，`HumanObject.cs:4126`）。
        let undead = self
            .game_data
            .as_ref()
            .and_then(|data| data.monster_info_by_name(&kind.name))
            .is_some_and(|info| info.undead);
        let mut monster =
            Monster::new(object_id, Arc::new(kind), map, position, now, &mut self.rng);
        monster.undead = undead;
        let packet = monster_info_frame(&monster);
        self.monsters.push(monster);
        let index = self.monsters.len() - 1;
        self.monster_slots.insert(object_id, index);
        // Crystal `Map.Load` 把怪物挂进地图（`Map.cs:473-488`）后再 `BroadcastInfo()`（`MapObject.cs:353`）。
        self.aoi_insert(map, object_id, AoiKind::Monster, position);
        self.sync_monster_view(object_id, map, None, position, &packet);
        object_id
    }

    /// 刷一只召唤物：怪物实例 + 主人归属（Crystal `SummonSkeleton`/`SummonShinsu`，
    /// `HumanObject.cs:4399-4440`、`:4448-4488`；`Map.cs:914-933` 的 `Spawn` + `Pets.Add`）。
    ///
    /// `pet_level` 是施法时的技能等级；属性加成与提速照 `MonsterObject.RefreshAll`
    /// （`MonsterObject.cs:811-827`）：
    /// `HP + 20L`、`AC/MAC + 2L`、`DC + L`，移动/攻击间隔各减 `MaxPetLevel × 130 / 70`（下限 400ms）。
    #[allow(clippy::too_many_arguments)] // 与 Crystal `Spawn` + `Pets.Add` 的入参一一对应，拆结构体反而更难读。
    pub(crate) fn spawn_pet(
        &mut self,
        map: &str,
        kind: MonsterKind,
        position: Point,
        master_id: u32,
        pet_level: u8,
        max_pet_level: u8,
        summon_spell: Spell,
    ) -> u32 {
        let now = self.now();
        let object_id = self.spawn_monster(map, kind, position);
        let level = i32::from(pet_level);
        let mut monster_frame = None;
        if let Some(monster) = self.monster_mut(object_id) {
            monster.master = Some(master_id);
            monster.pet_level = pet_level;
            monster.max_pet_level = max_pet_level;
            monster.summon_spell = Some(summon_spell);
            monster.stats.hp += level * 20;
            monster.stats.min_ac += level * 2;
            monster.stats.max_ac += level * 2;
            monster.stats.min_mac += level * 2;
            monster.stats.max_mac += level * 2;
            monster.stats.min_dc += level;
            monster.stats.max_dc += level;
            monster.hp = monster.stats.hp;
            let reduction = i64::from(max_pet_level);
            monster.move_speed_ms = (monster.move_speed_ms - reduction * 130).max(400);
            monster.attack_speed_ms = (monster.attack_speed_ms - reduction * 70).max(400);
            // `SummonSkeleton` 的 `ActionTime = Envir.Time + 1000`（`HumanObject.cs:4432`）。
            monster.action_time = now + 1_000;
            monster.move_time = now + 1_000;
            monster.attack_time = now + 1_000;
            monster.search_time = now;
            monster_frame = Some(monster_info_frame(monster));
        }
        self.pets.entry(master_id).or_default().push(object_id);
        // 新宠物的命令重置为默认「跟随 + 攻击」（任务要求：重召唤后重置）。
        self.pet_modes.insert(master_id, PetMode::Both);
        // 重新广播一次外观：`spawn_monster` 那一帧还不知道主人（`MasterObjectId = 0`）。
        if let Some(frame) = monster_frame {
            self.broadcast(map, position, None, &frame, object_id);
        }
        object_id
    }

    /// 设置玩家宠物命令（Crystal `MirConnection.ChangePMode` → `Player.PMode`，
    /// `MirNetwork/MirConnection.cs:1440-1447`）；玩家不在世界里返回 false。
    pub fn set_pet_mode(&mut self, master_id: u32, mode: PetMode) -> bool {
        if !self.players.contains_key(&master_id) {
            return false;
        }
        self.pet_modes.insert(master_id, mode);
        true
    }

    /// 玩家当前宠物命令；未设置时按 Crystal 默认 `PetMode::Both`（跟随 + 攻击）。
    #[must_use]
    pub fn pet_mode(&self, master_id: u32) -> PetMode {
        self.pet_modes
            .get(&master_id)
            .copied()
            .unwrap_or(PetMode::Both)
    }

    /// 某个主人的召唤物 ID 列表（按召唤顺序；测试/运维观察用）。
    #[must_use]
    pub fn pets_of(&self, master_id: u32) -> Vec<u32> {
        self.pets.get(&master_id).cloned().unwrap_or_default()
    }

    /// 从召唤物索引里摘掉一只（召唤物死亡/尸体到期时调用）。
    pub(crate) fn unregister_pet(&mut self, pet_id: u32) {
        self.pets.retain(|_, pets| {
            pets.retain(|id| *id != pet_id);
            !pets.is_empty()
        });
    }

    /// 主人死亡：所有召唤物一起死（Crystal `PlayerObject.cs:629-633`）。
    pub(crate) fn kill_pets(&mut self, master_id: u32) {
        let pets = self.pets_of(master_id);
        for pet_id in pets {
            self.monster_died(pet_id);
        }
        self.pets.remove(&master_id);
        self.pet_modes.remove(&master_id);
    }

    /// 主人下线/死亡时清掉全部召唤物（Crystal `PlayerObject.Die` 的宠物清理，
    /// `PlayerObject.cs:624-633`；`StopGame` 的 `Despawn` 同理）。
    pub(crate) fn despawn_pets(&mut self, master_id: u32) {
        self.pet_modes.remove(&master_id);
        let Some(pets) = self.pets.remove(&master_id) else {
            return;
        };
        if pets.is_empty() {
            return;
        }
        for pet_id in &pets {
            // 广播 `ObjectRemove` 并从 AOI 摘掉。
            self.forget_monster(*pet_id);
        }
        self.monsters
            .retain(|monster| !pets.contains(&monster.object_id));
        self.rebuild_monster_slots();
    }

    /// 召回召唤物：跨图/离主人太远时传送到主人身后一格（Crystal `PetRecall`，
    /// `MonsterObject.cs:1334-1375`；触发点 `:1713-1714`）。
    ///
    /// 落点取 `Master.Back`，不可走时退回主人脚下（`Teleport(Master.CurrentMap, Master.Back)`，
    /// 失败再 `Teleport(Master.CurrentMap, Master.CurrentLocation)`，`:1366-1367`）。
    pub(crate) fn recall_pet(&mut self, pet_id: u32) {
        let Some(mut monster) = self.monster(pet_id).cloned() else {
            return;
        };
        // 同名召唤的「召回」等同重新召唤：命令重置为默认（任务要求）。
        if let Some(master_id) = monster.master {
            self.pet_modes.insert(master_id, PetMode::Both);
        }
        self.recall_pet_monster(&mut monster);
        if let Some(slot) = self.monster_mut(pet_id) {
            *slot = monster;
        }
    }

    /// 召回一只召唤物（就地版本：调用方持有 `Monster` 副本时用，见
    /// `tick_monsters` 的「克隆处理再回写」）。
    pub(crate) fn recall_pet_monster(&mut self, monster: &mut Monster) {
        let Some(master_id) = monster.master else {
            return;
        };
        let Some((master_map, master_position, master_direction)) = self
            .players
            .get(&master_id)
            .map(|master| (master.map.clone(), master.position, master.direction))
        else {
            return;
        };
        let back = master_position.moved(master_direction, -1);
        let landing = if self.walkable_on(&master_map, back) {
            back
        } else {
            master_position
        };
        // 先让旧位置的玩家收到移除（`Teleport` 的 `ObjectRemove`，`MapObject.cs:784-786`）。
        self.forget_monster(monster.object_id);
        let now = self.now();
        monster.map.clone_from(&master_map);
        monster.position = landing;
        monster.target = None;
        monster.pet_target = None;
        monster.action_time = now + 300;
        monster.move_time = now + 300;
        monster.attack_time = now + 300;
        monster.search_time = now;
        let frame = monster_info_frame(monster);
        self.aoi_insert(&master_map, monster.object_id, AoiKind::Monster, landing);
        self.sync_monster_view(monster.object_id, &master_map, None, landing, &frame);
    }

    /// 地图上的持续法术场（火墙），测试/运维观察用。
    #[must_use]
    pub fn spell_fields(&self) -> &[crate::magic::SpellField] {
        &self.spell_fields
    }

    /// 怪物位置/可见性变化后，刷新受影响的玩家的视野。
    ///
    /// 等价于 Crystal 的 `AddObjects/RemoveObjects`（`HumanObject.cs:7864`、`:8152`）：
    /// 新进入视野发 `ObjectMonster`，离开视野发 `ObjectRemove`。
    ///
    /// 候选玩家取**旧位置与新位置两个半径矩形的并集**（Crystal 移动时扫的就是
    /// 进出视野的条带，`HumanObject.cs:2470-2476`）：只查这两个矩形，不遍历全服玩家。
    pub(crate) fn sync_monster_view(
        &mut self,
        object_id: u32,
        map: &str,
        old_position: Option<Point>,
        position: Point,
        frame: &[u8],
    ) {
        let removal = object_remove_frame(object_id);
        let mut candidates: BTreeSet<u32> = BTreeSet::new();
        if let Some(old) = old_position {
            candidates.extend(self.aoi_ids_in_radius(map, old, VIEW_RANGE, AoiKind::Player));
        }
        candidates.extend(self.aoi_ids_in_radius(map, position, VIEW_RANGE, AoiKind::Player));
        for player_id in candidates {
            let Some(player) = self.players.get_mut(&player_id) else {
                continue;
            };
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
    ///
    /// 调用方必须在把怪物从 [`World::monsters`] 移除**之前**调用；本函数顺带把它从 AOI 网格摘掉。
    pub(crate) fn forget_monster(&mut self, object_id: u32) {
        let Some((map, position)) = self
            .monster(object_id)
            .map(|monster| (monster.map.clone(), monster.position))
        else {
            return;
        };
        let removal = object_remove_frame(object_id);
        for player_id in self.aoi_ids_in_radius(&map, position, VIEW_RANGE, AoiKind::Player) {
            let Some(player) = self.players.get_mut(&player_id) else {
                continue;
            };
            if player.visible_monsters.remove(&object_id) {
                let _ = player.sender.send(removal.clone());
            }
        }
        self.aoi_remove(&map, AoiKind::Monster, object_id);
    }

    /// 玩家移动后刷新自己的怪物视野（先发移除、再发新增，顺序同 Crystal `Walk`）。
    ///
    /// 新可见集合由 AOI 网格按 `(y,x,ID)` 查出（等价 Crystal 的 33×33 逐格扫描），
    /// 再与上一轮集合做差分：只有真正进出视野的怪物才发一次包
    /// （`HumanObject.RemoveObjects/AddObjects`，`HumanObject.cs:2470-2476`）。
    fn sync_player_monsters(&mut self, player_id: u32) {
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        let map = player.map.clone();
        let position = player.position;
        let visible: BTreeSet<u32> = self
            .aoi_ids_in_radius(&map, position, VIEW_RANGE, AoiKind::Monster)
            .into_iter()
            .collect();
        let removals: Vec<u32> = player
            .visible_monsters
            .difference(&visible)
            .copied()
            .collect();
        let adds: Vec<Vec<u8>> = visible
            .difference(&player.visible_monsters)
            .filter_map(|object_id| self.monster(*object_id).map(monster_info_frame))
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

    /// 玩家移动后刷新「我能看见哪些玩家」，并做**双向**差分。
    ///
    /// Crystal `HumanObject.Add/Remove(HumanObject)`（`HumanObject.cs:8440-8457`）：
    /// 进入视野时双方各收一条 `S.ObjectPlayer`，离开视野时双方各收一条 `S.ObjectRemove`。
    /// 这里用 [`World::visible_players`] 保存上一轮集合，保证每个对象只发一次进出包。
    fn sync_player_players(&mut self, player_id: u32) {
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        let map = player.map.clone();
        let position = player.position;
        let self_frame = self.appearance_packet(player);
        let visible: BTreeSet<u32> = self
            .aoi_ids_in_radius(&map, position, VIEW_RANGE, AoiKind::Player)
            .into_iter()
            .filter(|object_id| *object_id != player_id)
            .collect();
        let previous = self
            .visible_players
            .get(&player_id)
            .cloned()
            .unwrap_or_default();
        let removals: Vec<u32> = previous.difference(&visible).copied().collect();
        let add_frames: Vec<(u32, Vec<u8>)> = visible
            .difference(&previous)
            .filter_map(|other_id| {
                self.players
                    .get(other_id)
                    .map(|other| (*other_id, self.appearance_packet(other)))
            })
            .collect();

        // 1) 本人：离开视野的玩家发 `ObjectRemove`，进入视野的玩家发 `ObjectPlayer`。
        if let Some(player) = self.players.get(&player_id) {
            for other_id in &removals {
                let _ = player.sender.send(object_remove_frame(*other_id));
            }
            for (_, frame) in &add_frames {
                let _ = player.sender.send(frame.clone());
            }
        }
        // 2) 对方：双向维护（Crystal 的 `Add/Remove` 两边各收一条）。
        for other_id in &removals {
            if let Some(other) = self.players.get(other_id) {
                let _ = other.sender.send(object_remove_frame(player_id));
            }
            if let Some(seen) = self.visible_players.get_mut(other_id) {
                seen.remove(&player_id);
            }
        }
        for (other_id, _) in &add_frames {
            if let Some(other) = self.players.get(other_id) {
                let _ = other.sender.send(self_frame.clone());
            }
            self.visible_players
                .entry(*other_id)
                .or_default()
                .insert(player_id);
        }
        self.visible_players.insert(player_id, visible);
    }

    /// 聊天广播，返回实际接收人数（含本人）。
    ///
    /// Crystal 的普通聊天是 `Enqueue(p)` + `Broadcast(p)`：本人与附近玩家都会收到
    /// `ObjectChat`，所以这里先把同一帧发给本人，再广播给其它人。
    ///
    /// 以 `!!` 开头的消息是**组队聊天**（Crystal `PlayerObject.Chat`，`PlayerObject.cs:2001-2013`），
    /// 只发给队员、不走本地图广播，见 [`World::group_chat`]（不在队伍里则静默丢弃，返回 0）。
    pub fn chat(&mut self, object_id: u32, message: &str) -> Result<usize, MoveError> {
        if !self.players.contains_key(&object_id) {
            return Err(MoveError::NotInWorld(object_id));
        }
        // `else if (message.StartsWith("!!"))`（`PlayerObject.cs:2001`）：
        // 这一支在 Crystal 的 `if/else if` 链里，所以**不会**再走到普通聊天广播。
        if message.starts_with("!!") {
            return Ok(self.group_chat(object_id, message));
        }
        // `else if (message.StartsWith("!~"))`（`PlayerObject.cs:2014`）：行会聊天，
        // 只发给在线成员，不走本地图广播（见 [`crate::guild`] 的 `guild_chat`）。
        if let Some(text) = message.strip_prefix("!~") {
            return Ok(self.guild_chat(object_id, text));
        }
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

    /// 目标格是否在这张地图内且可走（怪物的 `Walk` 只做这一层判定，
    /// `Server/MirObjects/MonsterObject.cs:2101` 的 `CurrentMap.ValidPoint(location)`）。    ///
    /// 地图名只是「这张图的键」：世界没加载过它时退回坐标上界 `bound`
    /// （无地形世界，绝大多数单元测试用的就是这种）。
    pub(crate) fn walkable_on(&self, map: &str, cell: Point) -> bool {
        let (max_x, max_y) = self.map_bounds(map);
        if cell.x < 0 || cell.y < 0 || cell.x > max_x || cell.y > max_y {
            return false;
        }
        self.map_of(map)
            .is_none_or(|data| data.walkable(cell.x, cell.y))
    }

    /// 地图是否禁止战斗：Crystal `MapInfo.NoFight`（`Server/MirDatabase/MapInfo.cs`），
    /// 在 `PlayerObject.IsAttackTarget(HumanObject)` 里检查（`PlayerObject.cs:4655`）。
    #[must_use]
    pub fn map_no_fight(&self, map: &str) -> bool {
        self.game_data()
            .and_then(|data| data.map_info(map))
            .is_some_and(|info| info.no_fight)
    }

    /// 地图是否强制行会战：Crystal `MapInfo.Fight`，
    /// 在 `PlayerObject.AtWar` 的第一行返回 true（`PlayerObject.cs:10331`）。
    #[must_use]
    pub fn map_fight(&self, map: &str) -> bool {
        self.game_data()
            .and_then(|data| data.map_info(map))
            .is_some_and(|info| info.fight)
    }

    /// 两个玩家是否处于「战争状态」：Crystal `PlayerObject.AtWar`
    /// （`PlayerObject.cs:10329-10343`）。
    ///
    /// ```csharp
    /// if (CurrentMap.Info.Fight) return true;
    /// if (MyGuild == null) return false;
    /// if (attacker is PlayerObject playerAttacker) {
    ///     if (attacker == null || playerAttacker.MyGuild == null) return false;
    ///     if (!MyGuild.WarringGuilds.Contains(playerAttacker.MyGuild)) return false;
    /// }
    /// return true;
    /// ```
    ///
    /// 简化：本作没有行会攻城战（`WarringGuilds` 未实现），所以只剩地图的 `Fight` 标记。
    #[must_use]
    fn at_war(&self, victim_id: u32, _attacker_id: u32) -> bool {
        let Some(victim) = self.players.get(&victim_id) else {
            return false;
        };
        // `MyGuild == null → false` / `WarringGuilds` 未实现 → 恒 false。
        self.map_fight(&victim.map)
    }

    /// 检查目标格是否可进入：边界、地形、占格。
    fn check_cell(&self, map: &str, cell: Point, mover: u32, now: i64) -> Result<(), MoveError> {
        let (max_x, max_y) = self.map_bounds(map);
        if cell.x < 0 || cell.y < 0 || cell.x > max_x || cell.y > max_y {
            return Err(MoveError::OutOfBounds {
                x: cell.x,
                y: cell.y,
            });
        }
        if let Some(map_data) = self.map_of(map) {
            if !map_data.walkable(cell.x, cell.y) {
                return Err(MoveError::Blocked {
                    x: cell.x,
                    y: cell.y,
                });
            }
            // 门：`CheckDoorOpen(location)`（`Map.cs:646-651`；走/跑各自在
            // `HumanObject.cs:2420`、`:2559` 判一次）。门格在地图文件里带 `door_index`。
            if let Some(index) = map_data
                .cell(cell.x, cell.y)
                .and_then(|map_cell| map_cell.door_index)
                && !self.door_is_open(map, index)
            {
                return Err(MoveError::DoorClosed {
                    x: cell.x,
                    y: cell.y,
                    index,
                });
            }
        }
        // 占格：只查目标格（Crystal `CurrentMap.GetCell(target).Objects`，
        // `HumanObject.cs:2427-2447`）。用 AOI 网格代替遍历全服玩家/怪物/NPC。
        for entry in self.aoi_in_cell(map, cell) {
            let blocked = match entry.kind {
                // 玩家：刚移动过的对象有 500ms 宽限（`CellTime >= Envir.Time` 时不算阻挡）。
                AoiKind::Player => {
                    entry.object_id != mover
                        && self
                            .players
                            .get(&entry.object_id)
                            .is_some_and(|other| other.position == cell && other.cell_time < now)
                }
                // 怪物也是阻挡物（Crystal `MapObject.Blocking`，怪物为 `!Dead`）。
                AoiKind::Monster => self
                    .monster(entry.object_id)
                    .is_some_and(|monster| !monster.dead && monster.position == cell),
                // NPC 也挡路：Crystal `NPCObject.Blocking => Visible`（`NPCObject.cs:418-421`）。
                AoiKind::Npc => self
                    .npc(entry.object_id)
                    .is_some_and(|npc| npc.map == map && npc.position == cell),
                AoiKind::Item => false,
            };
            if blocked {
                return Err(MoveError::Occupied {
                    x: cell.x,
                    y: cell.y,
                });
            }
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
        // 只查广播点 ±VIEW_RANGE 的玩家（Crystal `Broadcast` 用 `Functions.InRange`，
        // `MapObject.cs:408-420`；这里用 AOI 网格代替遍历全服玩家）。
        for entry in self.aoi_in_radius(map, position, VIEW_RANGE) {
            if entry.kind != AoiKind::Player {
                continue;
            }
            if exclude == Some(entry.object_id) || entry.object_id == actor {
                continue;
            }
            let Some(other) = self.players.get(&entry.object_id) else {
                continue;
            };
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
            let mut stats = player.stats;
            scale_armour(&mut stats, player.armour_rate());
            (
                stats,
                player.map.clone(),
                player.dead,
                player.position,
                player.direction,
            )
        })
    }

    /// 取一份怪物快照，理由同 [`World::player_snapshot`]。
    /// 红毒会按 `ArmourRate` 降低防御（`MonsterObject.cs:2581`）。
    fn monster_snapshot(
        &self,
        object_id: u32,
    ) -> Option<(CombatStats, bool, String, Point, MirDirection)> {
        self.monster(object_id).map(|monster| {
            let mut stats = monster.stats;
            scale_armour(&mut stats, monster.armour_rate());
            (
                stats,
                monster.dead,
                monster.map.clone(),
                monster.position,
                monster.direction,
            )
        })
    }
}

/// 按红毒倍率缩放四项防御（Crystal `armour * ArmourRate`，`HumanObject.cs:7241`）。
fn scale_armour(stats: &mut CombatStats, rate: f32) {
    if (rate - 1.0).abs() < f32::EPSILON {
        return;
    }
    stats.min_ac = (f64::from(stats.min_ac) * f64::from(rate)) as i32;
    stats.max_ac = (f64::from(stats.max_ac) * f64::from(rate)) as i32;
    stats.min_mac = (f64::from(stats.min_mac) * f64::from(rate)) as i32;
    stats.max_mac = (f64::from(stats.max_mac) * f64::from(rate)) as i32;
}

/// 两个玩家是否在同一支队伍里（Crystal `GroupMembers.Contains(attacker)`，`PlayerObject.cs:4662`）。
fn same_group(attacker: &Player, target: &Player) -> bool {
    matches!(
        (attacker.group_id, target.group_id),
        (Some(left), Some(right)) if left == right
    )
}

/// 两个玩家是否在同一行会（Crystal `MyGuild == attacker.MyGuild`，`PlayerObject.cs:4664`）。
fn same_guild(attacker: &Player, target: &Player) -> bool {
    matches!(
        (attacker.guild, target.guild),
        (Some(left), Some(right)) if left.guild_id == right.guild_id
    )
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
            name_colour: player.name_colour,
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
            hidden: player.magic.hidden,
            poison: player.poison_type(),
            buffs: if player.magic.hidden {
                vec![mir_core::enums::BuffType::Hiding]
            } else {
                Vec::new()
            },
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
        // Crystal `MapObject.NameColour = Color.White`（`MapObject.cs:67`）；
        // 守卫在构造函数里改成 `Color.SkyBlue`（`Monsters/Guard.cs:24-28`）。
        name_colour: if monster.kind.is_guard() {
            NAME_COLOUR_GUARD
        } else {
            NAME_COLOUR_WHITE
        },
        location: monster.position,
        image: monster.kind.image,
        direction: monster.direction,
        effect: 0,
        ai: monster.kind.ai,
        light: 0,
        dead: monster.dead,
        skeleton: false,
        poison: monster.poison_type(),
        hidden: false,
        extra: false,
        extra_byte: 0,
        shock_time: 0,
        binding_shot_center: false,
        master_object_id: monster.master.unwrap_or(0),
        rarity: MonsterType::Normal,
        buffs: Vec::new(),
    })
    .encode_frame()
}

/// `ObjectRemove` 帧（怪物消失、玩家下线、切图离开旧图都用它）。
pub(crate) fn object_remove_frame(object_id: u32) -> Vec<u8> {
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
                attack_mode: AttackMode::Peace,
                pk_points: 0,
                brown_time: 0,
                pk_point_time: 0,
                last_hitter: None,
                last_hit_time: 0,
                name_colour: NAME_COLOUR_WHITE,
                visible_monsters: BTreeSet::new(),
                visible_npcs: BTreeSet::new(),
                movement_time: 0,
                inventory: crate::items::new_inventory(),
                equipment: crate::items::new_equipment(),
                storage: crate::storage::new_storage(),
                magics: Vec::new(),
                magic: crate::magic::MagicState::default(),
                gold: 0,
                credit: 0,
                visible_items: BTreeSet::new(),
                quests: Vec::new(),
                completed_quests: BTreeSet::new(),
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
