//! 任务系统：接取、目标推进、交付领奖、放弃、超时与持久化。
//!
//! 移植自 Crystal 的四个文件：
//!
//! | 本模块 | Crystal | 内容 |
//! |---|---|---|
//! | [`World::accept_quest`] | `PlayerObject.AcceptQuest`（`PlayerObject.cs:11251-11348`） | 前置校验、发携带物品、`S.ChangeQuest{Add}` |
//! | [`World::finish_quest`] | `PlayerObject.FinishQuest`（`:11350-11461`） | 目标校验、发奖励、`S.CompleteQuest` + `S.ChangeQuest{Remove}` |
//! | [`World::abandon_quest`] | `PlayerObject.AbandonQuest`（`:11462-11471`） | 清进度 |
//! | [`World::share_quest`] | `PlayerObject.ShareQuest`（`:11472-11492`） | 组队分享 |
//! | [`World::advance_quest_kills`] | `PlayerObject.CheckNeedQuestKill`（`:11578-11590`）+ `QuestProgressInfo.ProcessKill`（`QuestProgressInfo.cs:385-396`） | 击杀计数 |
//! | [`World::refresh_quest_items`] | `PlayerObject.CheckNeedQuestItem`（`:11539-11561`）+ `ProcessItem`（`QuestProgressInfo.cs:398-408`） | 收集计数 |
//! | `tick_quests` | `QuestProgressInfo.SetTimer` + `PlayerObject.CompleteQuest`（`:538-573`、`:4364-4385`） | 超时 → 放弃 |
//! | `quest_login_packets` | `PlayerObject.GetQuestInfo`/`GetCompletedQuests`（`:1185-1188`、`:1727-1731`、`:11662-11668`） | 进世界时下发任务表 |
//! | [`encode_quests`] / [`decode_quests`] | `CharacterInfo.Save`/`Load` 的 quest 段（`CharacterInfo.cs:262-277`、`:488-492`、`:514-518`） | 存档 |
//!
//! # 与 Crystal 的三处结构性差异
//!
//! 1. **任务物品栏**：Crystal 有一块独立的 `QuestInventory`（40 格，`CharacterInfo.cs:75`），
//!    怪物掉落/挖矿时命中任务目标的物品会被**截胡**进这块背包（`MonsterObject.cs:1130-1138`、
//!    `HumanObject.cs:947`），收集进度也从它统计（`QuestProgressInfo.cs:398-408`）。
//!    本作改用**普通背包**：收集进度 = 背包里该物品的实时数量，交付时从背包扣除。
//!    原因：任务书要求「按任务要求实时统计背包数量即可」，且 40 格任务物品栏牵连
//!    `S.GainedQuestItem` / `S.DeleteQuestItem` / `UserInformation.QuestInventory` 三个包的语义。
//! 2. **`NpcIndex` / `FinishNpcIndex`**：Crystal 的 `QuestInfo` 有这两个字段，但它们**不在数据库里**
//!    （`QuestInfo.cs:85-110` 没有读它们），而是 `NPCScript.ParseQuests` 从 NPC 脚本的
//!    `[Quests]` 段回填的（`NPCScript.cs:685-721`：正数=发布 NPC、负数=交付 NPC）。
//!    本作同时认数据库的 `NpcInfo.CollectQuestIndexes` / `FinishQuestIndexes`
//!    （任务书要求）和 NPC 脚本的 `[Quests]` 段（真实数据 178 份脚本在用）。
//! 3. **完成态**：Crystal 交付后把进度从 `CurrentQuests` 删掉、把编号塞进 `CompletedQuests`。
//!    本作保留一条 `state = Completed` 的进度（可存档、可查询），同时维护同语义的
//!    [`Player::completed_quests`]。可重复任务的完成记录会被下一次接取覆盖（Crystal 的
//!    `QuestType.Repeatable` 不写 `CompletedQuests`，`:11428-11432`）。
//!
//! # 时钟
//!
//! 任务的时间戳用 [`World::wall_now`]（世界创建时刻的 Unix 毫秒 + 单调时钟），
//! 这样 `advance_clock` 快进时超时也会跟着触发，而存档里存的是真实时间戳。

use std::collections::BTreeSet;

use mir_core::enums::{OutputMessageType, QuestState, QuestType, RequiredClass};
use mir_core::i18n::{TextKey, format_text, text};
use mir_db::QuestInfo;
use mir_db::quest_script::QuestScript;
use mir_protocol::data::{ClientQuestInfo, ClientQuestProgress, QuestItemReward};
use mir_protocol::packets::server::{
    ChangeQuest, CompleteQuest, DeleteItem, GainedCredit, GainedItem, NewQuestInfo,
    SendOutputMessage, ShareQuest,
};
use mir_protocol::{PacketReader, PacketWriter, ProtocolError, ServerPacket};

use crate::gamedata::normalize_item_name;
use crate::npc::{NPC_TALK_RANGE, Npc};
use crate::world::{Player, World};

/// 同时能接多少个任务：Crystal `Globals.MaxConcurrentQuests = 20`（`Shared/Globals.cs:33`）。
pub const MAX_CONCURRENT_QUESTS: usize = 20;

/// 每日任务每天最多完成几次（**本作扩展**：Crystal 的 `QuestType.Daily` 没有完成次数上限，
/// 只有跨天 `ClearDailyQuests`；这里加一个可预期的防刷上限，超出后当天不能再接每日任务）。
pub const MAX_DAILY_COMPLETIONS: usize = 10;

/// 「新任务」高亮窗口：Crystal `New => StartDateTime > Envir.Now.AddDays(-1)`
/// （`QuestProgressInfo.cs:40-43`）。
pub const QUEST_NEW_WINDOW_MS: i64 = 24 * 60 * 60 * 1000;

/// 每日任务的重置间隔：24 小时（毫秒）。
///
/// Crystal 在服务器**跨天（0 点）**时对全部角色执行 `ClearDailyQuests`（`Envir.cs:2311-2315`、
/// `:4856-4866`）；本作改为按角色的 `last_daily_reset_at` 惰性补做，间隔 24h。
pub const DAILY_RESET_INTERVAL_MS: i64 = 24 * 60 * 60 * 1000;

/// 是否该做每日重置：距上次重置 ≥24h，或从未重置过（`last <= 0`，新角色/旧存档）。
///
/// 纯函数，方便在 session 侧（登录/任务交互）判边界；重置本身是幂等的，所以
/// 「从未重置过」直接视为到期不会有副作用。
#[must_use]
pub fn daily_reset_due(last_reset_at_ms: i64, now_ms: i64) -> bool {
    last_reset_at_ms <= 0 || now_ms.saturating_sub(last_reset_at_ms) >= DAILY_RESET_INTERVAL_MS
}

/// 一毫秒对应的秒数换算用的常量（`time_limit_in_seconds` → 毫秒）。
const SECOND_MS: i64 = 1_000;

// ---------------------------------------------------------------------------
// 任务进度
// ---------------------------------------------------------------------------

/// 一条任务进度的状态。
///
/// Crystal 没有枚举表示它，而是用两个 `DateTime` 的哨兵值表达
/// （`QuestProgressInfo.cs:30-38`）：`StartDateTime > MinValue` 即「已接」、
/// `EndDateTime < MaxValue` 即「已完成」。本作把它显式化成三个状态。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QuestProgressState {
    /// 没接过（只在「查一个不在列表里的任务」时出现）。
    #[default]
    NotStarted,
    /// 已接、进行中（Crystal `Taken && !Completed`）。
    InProgress,
    /// 已交付完成（Crystal `Completed`）。
    Completed,
}

impl QuestProgressState {
    /// 存档用的字节值。
    #[must_use]
    pub const fn value(self) -> u8 {
        match self {
            Self::NotStarted => 0,
            Self::InProgress => 1,
            Self::Completed => 2,
        }
    }

    /// 从存档字节还原；未知值返回 `None`。
    #[must_use]
    pub const fn from_value(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::NotStarted),
            1 => Some(Self::InProgress),
            2 => Some(Self::Completed),
            _ => None,
        }
    }
}

/// 一条任务的运行期进度（Crystal `QuestProgressInfo`，`Server/MirDatabase/QuestProgressInfo.cs:6`）。
///
/// * `kill_count` / `item_collected` 的键都是**任务脚本里的模板名归一化之后**的形式
///   （小写 + 去空白，见 [`normalize_item_name`]），这样怪物名/物品名的大小写与空格差异
///   不会让计数错位（Crystal 用 `MonsterID`/`ItemID` 直接对齐模板，本作没有稳定 ID）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuestProgress {
    /// 任务编号（`QuestInfo.Index`）。
    pub quest_index: i32,
    /// 状态。
    pub state: QuestProgressState,
    /// **目标是否已经全部达成**（Crystal `QuestProgressInfo.Completed`，`QuestProgressInfo.cs:35-38`）。
    ///
    /// 它与 [`Self::state`] 是两件事：`objectives_met` 一旦达成就为真（客户端据此显示
    /// “可以交任务”的图标，`ClientQuestProgress.Completed` 读的就是它），而 `state` 要等到
    /// 真正交付才变 `Completed`。
    pub objectives_met: bool,
    /// 接取时刻（Unix 毫秒；Crystal `StartDateTime`）。
    pub taken_at: i64,
    /// 超时时刻（Unix 毫秒；`0` = 不限时）。Crystal 用 `DelayedAction` 排程，本作每次 tick 比较。
    pub expires_at: i64,
    /// 击杀计数：`(归一化怪物名, 已击杀数)`。
    pub kill_count: Vec<(String, i32)>,
    /// 收集计数：`(归一化物品名, 背包里现有数量)`。
    pub item_collected: Vec<(String, u16)>,
    /// 已点亮的标记位（Crystal `QuestFlagTaskProgress.State` 为 true 的那几个）。
    pub flags: BTreeSet<i32>,
}

impl QuestProgress {
    /// 新建一条「刚接取」的进度：按脚本预置三个目标列表的空计数
    /// （Crystal `QuestProgressInfo(int index)` 构造函数的做法，`QuestProgressInfo.cs:45-83`）。
    #[must_use]
    pub fn new(
        quest_index: i32,
        taken_at: i64,
        time_limit_in_seconds: i32,
        script: &QuestScript,
    ) -> Self {
        let expires_at = if time_limit_in_seconds > 0 {
            taken_at + i64::from(time_limit_in_seconds) * SECOND_MS
        } else {
            0
        };
        Self {
            quest_index,
            state: QuestProgressState::InProgress,
            objectives_met: false,
            taken_at,
            expires_at,
            kill_count: script
                .kill_tasks
                .iter()
                .map(|task| (normalize_item_name(&task.monster), 0))
                .collect(),
            item_collected: script
                .item_tasks
                .iter()
                .map(|task| (normalize_item_name(&task.item), 0))
                .collect(),
            flags: BTreeSet::new(),
        }
    }

    /// 是否进行中。
    #[must_use]
    pub fn is_in_progress(&self) -> bool {
        self.state == QuestProgressState::InProgress
    }

    /// 是否已完成。
    #[must_use]
    pub fn is_completed(&self) -> bool {
        self.state == QuestProgressState::Completed
    }

    /// 某个（归一化）怪物名已经击杀多少。
    #[must_use]
    pub fn kill_count_of(&self, monster: &str) -> i32 {
        let key = normalize_item_name(monster);
        self.kill_count
            .iter()
            .find(|(name, _)| name == &key)
            .map_or(0, |(_, count)| *count)
    }

    /// 某个（归一化）物品名现有多少。
    #[must_use]
    pub fn item_count_of(&self, item: &str) -> u16 {
        let key = normalize_item_name(item);
        self.item_collected
            .iter()
            .find(|(name, _)| name == &key)
            .map_or(0, |(_, count)| *count)
    }
}

// ---------------------------------------------------------------------------
// 错误与报告
// ---------------------------------------------------------------------------

/// 任务操作失败的原因。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum QuestError {
    /// 玩家不在世界里。
    #[error("player {0} is not in the world")]
    NotInWorld(u32),
    /// 玩家已死亡（Crystal 的 NPC 入口一律先 `if (Dead) return;`）。
    #[error("player {player} is dead")]
    Dead {
        /// 玩家对象 ID。
        player: u32,
    },
    /// 数据库里没有这个任务。
    #[error("quest {0} does not exist")]
    NoSuchQuest(i32),
    /// 已经接过这个任务。
    #[error("quest {0} is already in progress")]
    AlreadyTaken(i32),
    /// 已经完成过这个任务（非可重复任务）。
    #[error("quest {0} has already been completed")]
    AlreadyCompleted(i32),
    /// 没接这个任务（交付/放弃时）。
    #[error("quest {0} is not accepted")]
    NotTaken(i32),
    /// 目标还没达成。
    #[error("quest {0} objectives are not complete")]
    NotCompleted(i32),
    /// 等级不够。
    #[error("level {level} does not meet the quest requirement ({min}..={max})")]
    LevelOutOfRange {
        /// 玩家等级。
        level: u16,
        /// 需求下限。
        min: i32,
        /// 需求上限。
        max: i32,
    },
    /// 职业不符。
    #[error("class {class} cannot accept this quest")]
    WrongClass {
        /// 玩家职业。
        class: &'static str,
    },
    /// 前置任务没完成。
    #[error("prerequisite quest {0} is not complete")]
    PrerequisiteMissing(i32),
    /// 同时进行的任务太多。
    #[error("at most {limit} quests can be active")]
    TooManyQuests {
        /// 上限。
        limit: usize,
    },
    /// 这个 NPC 不发布该任务。
    #[error("NPC {npc} does not offer quest {quest}")]
    NotAQuestPublisher {
        /// NPC 对象 ID。
        npc: u32,
        /// 任务编号。
        quest: i32,
    },
    /// 这个 NPC 不交付该任务。
    #[error("NPC {npc} cannot finish quest {quest}")]
    NotAQuestFinisher {
        /// NPC 对象 ID。
        npc: u32,
        /// 任务编号。
        quest: i32,
    },
    /// 附近没有能交付这个任务的 NPC。
    #[error("no NPC nearby can finish quest {0}")]
    NoFinisherNearby(i32),
    /// NPC 不存在或不在同图。
    #[error("NPC {0} is not on the current map")]
    NoSuchNpc(u32),
    /// 离 NPC 太远。
    #[error("too far from NPC {object_id} (max {range} cells)")]
    OutOfRange {
        /// NPC 对象 ID。
        object_id: u32,
        /// 距离上限。
        range: i32,
    },
    /// 当天完成的每日任务数已达上限（**本作扩展**；Crystal 无此上限）。
    #[error("daily quest limit reached ({limit} per day)")]
    DailyLimit {
        /// 每日完成上限。
        limit: usize,
    },
    /// 背包放不下携带物品或奖励。
    #[error("quest items do not fit in the inventory")]
    InventoryFull,
    /// 没有可以分享的对象：不在队伍里或附近没有队友
    /// （Crystal `ServerTextKeys.QuestNotShared`，`PlayerObject.cs:11488-11491`）。
    #[error("quest cannot be shared (requires a nearby group member on the same map)")]
    NotShared,
}

/// 一次任务操作的结果（同时已经把包发给玩家）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuestReport {
    /// 任务编号。
    pub quest_index: i32,
    /// 任务名。
    pub name: String,
    /// 目标是否全部达成。
    pub completed: bool,
    /// 任务进度文本（`ClientQuestProgress.task_list`）。
    pub task_list: Vec<String>,
    /// 发出去的金币。
    pub gold: u32,
    /// 发出去的经验。
    pub exp: u32,
    /// 发出去的元宝。
    pub credit: u32,
    /// 发出去的物品（模板名 + 数量）。
    pub items: Vec<(String, u16)>,
}

// ---------------------------------------------------------------------------
// 任务定义
// ---------------------------------------------------------------------------

/// 一份「可执行」的任务定义 = 数据库头部（[`QuestInfo`]）+ 脚本正文（[`QuestScript`]）。
#[derive(Debug, Clone)]
pub struct QuestDefinition {
    /// 数据库里的头部（编号、名字、分组、等级/职业/前置、时限、四条提示）。
    pub info: QuestInfo,
    /// `Envir/Quests/<file_name>.txt` 的解析结果。文件缺失时是空脚本
    /// （Crystal `LoadInfo` 失败后同样是空的，`QuestInfo.cs:145`）。
    pub script: std::sync::Arc<QuestScript>,
}

impl QuestDefinition {
    /// 时限（秒），`0` = 不限时。
    #[must_use]
    pub fn time_limit_seconds(&self) -> i32 {
        self.info.time_limit_in_seconds
    }

    /// 组装客户端用的任务定义（Crystal `QuestInfo.CreateClientQuestInfo`，`QuestInfo.cs:385-415`）。
    ///
    /// `npc_index` / `finish_npc_index` 是 Crystal 里由 NPC 脚本回填的两个字段
    /// （`NPCInfo` 里没有），本作按世界里的 NPC 现查，查不到就是 0。
    #[must_use]
    pub fn client_info(&self, npc_index: u32, finish_npc_index: u32) -> ClientQuestInfo {
        let reward = |line: &mir_db::quest_script::QuestRewardLine| QuestItemReward {
            // Crystal 这里挂的是 `ItemInfo` 引用（`QuestInfo.cs:273`）；本作只解析名字，
            // 所以先填一个占位模板（`index = 0`），真正的模板在 `World::client_quest_info` 里补。
            item: mir_protocol::data::ItemInfo {
                name: line.item.clone(),
                ..mir_protocol::data::ItemInfo::default()
            },
            count: line.count,
        };
        ClientQuestInfo {
            index: self.info.index,
            npc_index,
            name: self.info.name.clone(),
            group: self.info.group.clone(),
            description: self.script.description.clone(),
            task_description: self.script.task_description.clone(),
            return_description: self.script.return_description.clone(),
            completion_description: self.script.completion_description.clone(),
            min_level_needed: self.info.required_min_level,
            max_level_needed: self.info.effective_required_max_level(),
            quest_needed: self.info.required_quest,
            class_needed: RequiredClass::from_value(self.info.required_class)
                .unwrap_or(RequiredClass::None),
            quest_type: QuestType::from_value(self.info.quest_type).unwrap_or_default(),
            time_limit_in_seconds: self.info.time_limit_in_seconds,
            reward_gold: self.script.gold_reward,
            reward_exp: self.script.exp_reward,
            reward_credit: self.script.credit_reward,
            rewards_fixed_item: self.script.fixed_rewards.iter().map(reward).collect(),
            rewards_select_item: self.script.select_rewards.iter().map(reward).collect(),
            finish_npc_index,
        }
    }
}

// ---------------------------------------------------------------------------
// 名字匹配
// ---------------------------------------------------------------------------

/// 怪物名是否命中任务的击杀目标。
///
/// 逐字移植 Crystal `QuestProgressInfo.NeedKill`（`QuestProgressInfo.cs:371-374`）：
/// `mInfo.Name.StartsWith(task.Info.Monster.Name, StringComparison.OrdinalIgnoreCase)`。
/// 这里先把两边都归一化（小写 + 去空白）再做前缀比较，等价于「忽略大小写」并且额外容忍空格差异
/// （任务书要求的归一化）。
#[must_use]
pub fn monster_matches_task(task_name: &str, monster_name: &str) -> bool {
    let task = normalize_item_name(task_name);
    let monster = normalize_item_name(monster_name);
    !task.is_empty() && monster.starts_with(&task)
}

/// 一件物品是否命中任务的收集目标（Crystal 比的是模板引用，本作比模板名）。
#[must_use]
pub fn item_matches_task(task_name: &str, item_name: &str) -> bool {
    normalize_item_name(task_name) == normalize_item_name(item_name)
}

/// 目标是否全部达成（Crystal `QuestProgressInfo.CheckCompleted`，`QuestProgressInfo.cs:321-362`）。
///
/// 注意 Crystal 比较的是 `Count >= Info.Count`，所以**坏数据里数量为 0 的目标天然算完成**；
/// 本作照抄（`need <= 0` 时不拦）。
#[must_use]
pub fn is_objectives_complete(script: &QuestScript, progress: &QuestProgress) -> bool {
    script
        .kill_tasks
        .iter()
        .all(|task| progress.kill_count_of(&task.monster) >= task.count)
        && script
            .item_tasks
            .iter()
            .all(|task| i32::from(progress.item_count_of(&task.item)) >= i32::from(task.count))
        && script
            .flag_tasks
            .iter()
            .all(|task| progress.flags.contains(&task.number))
}

// ---------------------------------------------------------------------------
// World：定义查询
// ---------------------------------------------------------------------------

impl World {
    /// 全部任务编号（按数据库顺序）。
    #[must_use]
    pub fn quest_indexes(&self) -> Vec<i32> {
        self.game_data()
            .map(|data| data.db.quests.iter().map(|info| info.index).collect())
            .unwrap_or_default()
    }

    /// 任务总数。
    #[must_use]
    pub fn quest_count(&self) -> usize {
        self.game_data().map_or(0, |data| data.quest_count())
    }

    /// 取一份任务定义（头部 + 脚本）。没有游戏数据时恒为 `None`。
    #[must_use]
    pub fn quest_definition(&self, quest_index: i32) -> Option<QuestDefinition> {
        let data = self.game_data()?;
        let info = data.quest(quest_index)?.clone();
        let script = data
            .quest_script(&info.file_name)
            .unwrap_or_else(|| std::sync::Arc::new(QuestScript::default()));
        Some(QuestDefinition { info, script })
    }

    /// 客户端用的任务定义（Crystal `QuestInfo.CreateClientQuestInfo`，`QuestInfo.cs:385-415`）。
    ///
    /// 与 Crystal 的差异：奖励条目的 `ItemInfo` 用**真实模板**补齐（Crystal 直接持引用，
    /// 因此线上天然是完整模板），查不到模板的奖励行会被跳过并记日志。
    #[must_use]
    pub fn client_quest_info(
        &self,
        quest_index: i32,
        language: mir_core::Language,
    ) -> Option<ClientQuestInfo> {
        let definition = self.quest_definition(quest_index)?;
        let publisher = self.quest_publisher_npc(quest_index);
        let finisher = self.quest_finisher_npc(quest_index);
        let mut info = definition.client_info(
            publisher.map_or(0, |npc| npc.object_id),
            finisher.map_or(0, |npc| npc.object_id),
        );
        if let Some(library) = self.languages.as_ref() {
            info.name = library.quest_name(language, info.index, &info.name);
        }
        info.rewards_fixed_item = self.resolve_rewards(&definition.script.fixed_rewards);
        info.rewards_select_item = self.resolve_rewards(&definition.script.select_rewards);
        Some(info)
    }

    /// 把奖励行解析成客户端奖励（`QuestItemReward` + 真实模板）。
    fn resolve_rewards(
        &self,
        lines: &[mir_db::quest_script::QuestRewardLine],
    ) -> Vec<QuestItemReward> {
        lines
            .iter()
            .filter_map(|line| {
                let info = self.item_info_by_name(&line.item)?;
                Some(QuestItemReward {
                    item: info.clone(),
                    count: line.count,
                })
            })
            .collect()
    }

    /// 世界里的第一个**发布**该任务的 NPC（Crystal `QuestInfo.NpcIndex`，由 `[Quests]` 回填）。
    #[must_use]
    pub fn quest_publisher_npc(&self, quest_index: i32) -> Option<&Npc> {
        self.npcs
            .iter()
            .find(|npc| self.npc_gives_quest(npc, quest_index))
    }

    /// 世界里的第一个**交付**该任务的 NPC（Crystal `QuestInfo.FinishNpcIndex`）。
    ///
    /// Crystal 的 `FinishNpcIndex` 有个默认值：`_finishNpcIndex == 0 ? NpcIndex : _finishNpcIndex`
    /// （`QuestInfo.cs:31-35`）——**没有单独的交付 NPC 时，发布者自己就是交付者**。
    /// 本作按同样的规则回退。
    #[must_use]
    pub fn quest_finisher_npc(&self, quest_index: i32) -> Option<&Npc> {
        self.npcs
            .iter()
            .find(|npc| self.npc_finishes_quest(npc, quest_index))
            .or_else(|| self.quest_publisher_npc(quest_index))
    }

    /// 这个 NPC 能否发布任务 `quest_index`。
    ///
    /// 两个来源都认：数据库的 `NpcInfo.CollectQuestIndexes`（`:NPCInfo.cs:40`）与
    /// 脚本的 `[Quests]` 段里的**正数**（`NPCScript.ParseQuests`，`NPCScript.cs:711-712`）。
    #[must_use]
    pub fn npc_gives_quest(&self, npc: &Npc, quest_index: i32) -> bool {
        npc.collect_quest_indexes.contains(&quest_index)
            || self
                .npc_script(&npc.file_name)
                .is_some_and(|script| script.publishes_quest(quest_index))
    }

    /// 这个 NPC 能否交付任务 `quest_index`（`NpcInfo.FinishQuestIndexes` 或脚本 `[Quests]` 的负数）。
    #[must_use]
    pub fn npc_finishes_quest(&self, npc: &Npc, quest_index: i32) -> bool {
        npc.finish_quest_indexes.contains(&quest_index)
            || self
                .npc_script(&npc.file_name)
                .is_some_and(|script| script.finishes_quest(quest_index))
    }

    /// 同图且在对话距离内的 NPC（Crystal `Functions.InRange(..., Globals.DataRange)`，`DataRange = 16`）。
    fn nearby_npcs(&self, player: &Player) -> Vec<Npc> {
        self.npcs
            .iter()
            .filter(|npc| {
                npc.map == player.map && npc.position.in_range(player.position, NPC_TALK_RANGE)
            })
            .cloned()
            .collect()
    }

    /// 玩家身上某个任务的进度。
    #[must_use]
    pub fn quest_progress(&self, player_id: u32, quest_index: i32) -> Option<&QuestProgress> {
        self.players
            .get(&player_id)?
            .quests
            .iter()
            .find(|progress| progress.quest_index == quest_index)
    }
}

// ---------------------------------------------------------------------------
// World：接取 / 交付 / 放弃 / 分享
// ---------------------------------------------------------------------------

impl World {
    /// 接任务（`C.AcceptQuest`，Crystal `PlayerObject.AcceptQuest`，`PlayerObject.cs:11251-11348`）。
    ///
    /// 判定顺序照抄 Crystal：
    ///
    /// 1. 已接同一个任务 → 直接返回（本作返回 [`QuestError::AlreadyTaken`]）；
    /// 2. 任务不存在 → 返回（Crystal 会在 `info.NpcIndex` 上 NRE，见 `docs/world/quests.md`）；
    /// 3. 当前地图里找 NPC，比较对象 ID 并做距离检查（`:11259-11269`）；
    /// 4. `info.CanAccept(this)`：等级区间、职业位掩码、前置任务链（`:11271`、`:350-383`）；
    /// 5. 并发任务数上限 [`MAX_CONCURRENT_QUESTS`]（`:11276-11280`）；
    /// 6. 已完成过的任务不能再接（`:11282-11286`，可重复任务由第 1 步之外的清理放行）；
    /// 7. 前置任务链（`:11288-11305`）；
    /// 8. 发放 `CarryItems`（`:11307-11339`）；
    /// 9. `SendUpdateQuest(quest, QuestState.Add, true)`（`:11341-11345`）。
    ///
    /// # Errors
    /// 见 [`QuestError`]。
    pub fn accept_quest(
        &mut self,
        player_id: u32,
        npc_object_id: u32,
        quest_index: i32,
    ) -> Result<QuestReport, QuestError> {
        let now = self.wall_now();
        let Some(player) = self.players.get(&player_id) else {
            return Err(QuestError::NotInWorld(player_id));
        };
        if player.dead {
            return Err(QuestError::Dead { player: player_id });
        }
        let definition = self
            .quest_definition(quest_index)
            .ok_or(QuestError::NoSuchQuest(quest_index))?;
        let repeatable = definition.info.quest_type == QuestType::Repeatable.value();
        if let Some(existing) = player
            .quests
            .iter()
            .find(|progress| progress.quest_index == quest_index)
        {
            match existing.state {
                QuestProgressState::InProgress => {
                    return Err(QuestError::AlreadyTaken(quest_index));
                }
                QuestProgressState::Completed if !repeatable => {
                    return Err(QuestError::AlreadyCompleted(quest_index));
                }
                _ => {}
            }
        }

        // 3) NPC：必须是同图、在对话距离内，而且确实发布这个任务。
        let npc = self
            .npcs
            .iter()
            .find(|npc| npc.object_id == npc_object_id && npc.map == player.map)
            .cloned()
            .ok_or(QuestError::NoSuchNpc(npc_object_id))?;
        if !npc.position.in_range(player.position, NPC_TALK_RANGE) {
            return Err(QuestError::OutOfRange {
                object_id: npc_object_id,
                range: NPC_TALK_RANGE,
            });
        }
        if !self.npc_gives_quest(&npc, quest_index) {
            return Err(QuestError::NotAQuestPublisher {
                npc: npc_object_id,
                quest: quest_index,
            });
        }

        // 4~7) 前置校验。
        self.check_can_accept(&definition, player_id, quest_index)?;

        // 8) 携带物品：Crystal 把 `[@CARRYITEMS]` 发进**任务物品栏**（`GainQuestItem`，
        //    `PlayerObject.cs:11307-11339`）；先算清单再一次性检查任务物品栏，
        //    避免「发了一半」的中间态。放不下就拒绝接取（Crystal 同样返回）。
        let carry = self.resolve_carry_items(&definition.script);
        if !carry.is_empty() {
            if !self.can_gain_quest_items(player_id, &carry) {
                return Err(QuestError::InventoryFull);
            }
            for (info, count) in &carry {
                let item = mir_protocol::data::UserItem {
                    count: *count,
                    ..self.create_fresh_item(info)
                };
                self.gain_quest_item(player_id, &item);
            }
        }

        // 9) 建进度、进列表。
        let progress = QuestProgress::new(
            quest_index,
            now,
            definition.time_limit_seconds(),
            &definition.script,
        );
        let Some(player) = self.players.get_mut(&player_id) else {
            return Err(QuestError::NotInWorld(player_id));
        };
        player
            .quests
            .retain(|existing| existing.quest_index != quest_index);
        player.quests.push(progress);
        // 收集类目标按当前背包重算一次（Crystal 在 `SendUpdateQuest` 里 `CheckCompleted`）；
        // 这里用**不发包**的版本，避免在 `S.ChangeQuest{Add}` 之前先发一条 Update。
        self.sync_quest_items(player_id);
        let report = self.send_quest_change(player_id, quest_index, QuestState::Add, true);
        Ok(report.unwrap_or_else(|| QuestReport {
            quest_index,
            name: definition.info.name.clone(),
            ..QuestReport::default()
        }))
    }

    /// 接取前置校验（Crystal `QuestInfo.CanAccept`，`QuestInfo.cs:350-383` + `PlayerObject.cs:11276-11305`）。
    fn check_can_accept(
        &self,
        definition: &QuestDefinition,
        player_id: u32,
        quest_index: i32,
    ) -> Result<(), QuestError> {
        let Some(player) = self.players.get(&player_id) else {
            return Err(QuestError::NotInWorld(player_id));
        };
        let level = i32::from(player.level);
        let min = definition.info.required_min_level;
        let max = definition.info.effective_required_max_level();
        if level < min || level > max {
            return Err(QuestError::LevelOutOfRange {
                level: player.level,
                min,
                max,
            });
        }
        let required = RequiredClass::from_value(definition.info.required_class)
            .unwrap_or(RequiredClass::None);
        if !required.contains(class_flag(player.class)) {
            return Err(QuestError::WrongClass {
                class: crate::npc::class_name(player.class),
            });
        }
        if player.quests.iter().filter(|p| p.is_in_progress()).count() >= MAX_CONCURRENT_QUESTS {
            return Err(QuestError::TooManyQuests {
                limit: MAX_CONCURRENT_QUESTS,
            });
        }
        // 每日任务完成上限（本作扩展）。
        if definition.info.quest_type == QuestType::Daily.value()
            && self.daily_completed_count(player_id) >= MAX_DAILY_COMPLETIONS
        {
            return Err(QuestError::DailyLimit {
                limit: MAX_DAILY_COMPLETIONS,
            });
        }
        // 前置任务链：Crystal 从 `RequiredQuest` 一路往上查（`PlayerObject.cs:11288-11299`）。
        // 这里同样走一遍，只是加了步数上限防止数据成环。
        let mut needed = definition.info.required_quest;
        let mut hops = 0;
        while needed > 0 && hops < 64 {
            if !player.completed_quests.contains(&needed) {
                return Err(QuestError::PrerequisiteMissing(needed));
            }
            let Some(next) = self.game_data().and_then(|data| data.quest(needed)) else {
                break;
            };
            needed = next.required_quest;
            hops += 1;
        }
        let _ = quest_index;
        Ok(())
    }

    /// 把 `[@CARRYITEMS]` 解析成 (模板, 数量) 清单，按堆叠上限拆分。
    fn resolve_carry_items(
        &self,
        script: &QuestScript,
    ) -> Vec<(mir_protocol::data::ItemInfo, u16)> {
        let mut out = Vec::new();
        for task in &script.carry_items {
            let Some(info) = self.item_info_by_name(&task.item).cloned() else {
                tracing::debug!(item = %task.item, "任务携带物品的模板不存在，跳过");
                continue;
            };
            let mut remaining = task.count;
            while remaining > 0 {
                let stack = info.stack_size.max(1);
                let count = remaining.min(stack);
                out.push((info.clone(), count));
                remaining -= count;
            }
        }
        out
    }

    /// 这些物品能否全部塞进背包（Crystal `CanGainItems`，`PlayerObject.cs` 里的批量版）。
    ///
    /// 简化：不模拟「放进去之后再算下一件」，而是逐件用 [`World::can_gain_item`] 判定。
    /// 只要背包还有空格，`can_gain_item` 就恒真，所以只有在背包已满时才可能误判。
    #[must_use]
    pub fn can_gain_items(
        &self,
        player_id: u32,
        items: &[(mir_protocol::data::ItemInfo, u16)],
    ) -> bool {
        let Some(player) = self.players.get(&player_id) else {
            return false;
        };
        // 按 Crystal `HumanObject.CanGainItems`（`HumanObject.cs:7629-7660`）批量模拟：
        // 先填已有堆叠，再逐件占用空格；前面的奖励占掉的槽位会影响后面的判定。
        // 背包快照：`(item_index, count)`，`None` = 空格。
        let mut slots: Vec<Option<(i32, u16)>> = player
            .inventory
            .iter()
            .map(|slot| slot.as_ref().map(|item| (item.item_index, item.count)))
            .collect();
        for (info, count) in items {
            if *count == 0 {
                continue;
            }
            let stack = info.stack_size.max(1);
            let mut remaining = *count;
            if stack > 1 {
                for slot in slots.iter_mut().flatten() {
                    if remaining == 0 {
                        break;
                    }
                    if slot.0 == info.index && slot.1 < stack {
                        let room = stack - slot.1;
                        let add = remaining.min(room);
                        slot.1 += add;
                        remaining -= add;
                    }
                }
            }
            while remaining > 0 {
                let Some(position) = slots.iter().position(Option::is_none) else {
                    return false;
                };
                let add = remaining.min(stack);
                slots[position] = Some((info.index, add));
                remaining -= add;
            }
        }
        true
    }

    /// 交任务（`C.FinishQuest`，Crystal `PlayerObject.FinishQuest`，`PlayerObject.cs:11350-11461`）。
    ///
    /// `selected_item_index` 是**任选奖励**的下标（Crystal 的同名参数，`:11350`；
    /// 协议里 `C.FinishQuest.SelectedItemIndex` 就是这个字段，所以不需要额外的
    /// `C.SelectQuestReward` 包）。`< 0` 表示不选。
    ///
    /// 发的包与顺序（照抄 `:11428-11460`）：
    ///
    /// | # | 包 | 说明 |
    /// |---|---|---|
    /// | 1 | `S.CompleteQuest` | 非可重复任务才发（`:11430-11431`） |
    /// | 2 | `S.ChangeQuest{Remove}` | `:11434` |
    /// | 3 | `S.DeleteItem` | 扣掉携带物品与收集目标（`:11436-11447`），本作走普通背包 |
    /// | 4 | `S.GainedItem` | 每件奖励一条（`GainItem` → `:11449-11452`） |
    /// | 5 | `S.GainedGold` | `GoldReward * DropRate`（`:11456`，`DropRate` 默认 1.0） |
    /// | 6 | `S.GainExperience` | `ExpReward * ExpRate`（`:11457`，`ExpRate` 默认 1.0） |
    /// | 7 | `S.GainedCredit` | `CreditReward`（`:11458`） |
    ///
    /// # Errors
    /// 未接取、目标未达成、附近没有可交付的 NPC、背包放不下奖励时返回 [`QuestError`]。
    pub fn finish_quest(
        &mut self,
        player_id: u32,
        quest_index: i32,
        selected_item_index: i32,
    ) -> Result<QuestReport, QuestError> {
        let nearby = {
            let Some(player) = self.players.get(&player_id) else {
                return Err(QuestError::NotInWorld(player_id));
            };
            if player.dead {
                return Err(QuestError::Dead { player: player_id });
            }
            self.nearby_npcs(player)
        };
        let definition = self
            .quest_definition(quest_index)
            .ok_or(QuestError::NoSuchQuest(quest_index))?;
        // 交付前把收集类目标按背包重算一次。
        self.refresh_quest_items(player_id);
        let Some(progress) = self.quest_progress(player_id, quest_index).cloned() else {
            return Err(QuestError::NotTaken(quest_index));
        };
        if progress.state == QuestProgressState::Completed {
            return Err(QuestError::NotTaken(quest_index));
        }
        if !is_objectives_complete(&definition.script, &progress) {
            return Err(QuestError::NotCompleted(quest_index));
        }

        // 交付 NPC：Crystal 按 `quest.Info.FinishNpcIndex` 在当前地图里找（`:11356-11366`）。
        // 本作没有那个字段，改成「附近任意一个能交付这个任务的 NPC」；都没有时退回发布者
        // （Crystal 的 `FinishNpcIndex` 默认就指向 `NpcIndex`，`QuestInfo.cs:31-35`）。
        let finisher = nearby
            .iter()
            .find(|npc| self.npc_finishes_quest(npc, quest_index))
            .or_else(|| {
                nearby
                    .iter()
                    .find(|npc| self.npc_gives_quest(npc, quest_index))
            })
            .cloned();
        let Some(finisher) = finisher else {
            return Err(QuestError::NoFinisherNearby(quest_index));
        };
        tracing::debug!(
            player = player_id,
            quest = quest_index,
            npc = finisher.object_id,
            "任务交付 NPC"
        );

        // 奖励清单：固定奖励 + 选中的可选奖励（Crystal `:11368-11420`）。
        let mut rewards: Vec<(mir_protocol::data::ItemInfo, u16)> =
            self.resolve_carry_items(&QuestScript {
                carry_items: definition
                    .script
                    .fixed_rewards
                    .iter()
                    .map(|line| mir_db::quest_script::QuestItemTask {
                        item: line.item.clone(),
                        count: line.count,
                        message: String::new(),
                    })
                    .collect(),
                ..QuestScript::default()
            });
        if selected_item_index >= 0 {
            if let Some(line) = definition
                .script
                .select_rewards
                .get(usize::try_from(selected_item_index).unwrap_or(usize::MAX))
            {
                if let Some(info) = self.item_info_by_name(&line.item).cloned() {
                    let mut remaining = line.count;
                    while remaining > 0 {
                        let stack = info.stack_size.max(1);
                        let count = remaining.min(stack);
                        rewards.push((info.clone(), count));
                        remaining -= count;
                    }
                }
            }
        }
        if !rewards.is_empty() && !self.can_gain_items(player_id, &rewards) {
            return Err(QuestError::InventoryFull);
        }

        // 1) 非可重复任务记入 `CompletedQuests`，并下发完整列表（`:11428-11432`）。
        if definition.info.quest_type != QuestType::Repeatable.value() {
            if let Some(player) = self.players.get_mut(&player_id) {
                player.completed_quests.insert(quest_index);
            }
            self.send_completed_quests(player_id);
        }

        // 2) 标记完成 + `S.ChangeQuest{Remove}`（`:11428-11434`）。
        if let Some(player) = self.players.get_mut(&player_id) {
            if let Some(entry) = player
                .quests
                .iter_mut()
                .find(|entry| entry.quest_index == quest_index)
            {
                entry.state = QuestProgressState::Completed;
            }
        }
        let report = self.send_quest_change(player_id, quest_index, QuestState::Remove, false);

        // 3) 扣携带物品与收集目标（`:11436-11447`）。Crystal 从任务物品栏扣
        //    （`TakeQuestItem` → `S.DeleteQuestItem`）；本作先扣任务物品栏，不够再从普通背包
        //    补扣（兼容旧存档/交易来的物品），见 `crate::quest_inventory` 的模块文档。
        for task in &definition.script.carry_items {
            self.take_quest_items(player_id, &task.item, task.count);
        }
        for task in &definition.script.item_tasks {
            self.take_quest_items(player_id, &task.item, task.count);
        }

        // 4) 发奖励物品（`:11449-11452`）。
        let mut given = Vec::new();
        for (info, count) in &rewards {
            let item = mir_protocol::data::UserItem {
                count: *count,
                ..self.create_fresh_item(info)
            };
            self.send_to_player(
                player_id,
                &ServerPacket::GainedItem(GainedItem { item: item.clone() }).encode_frame(),
            );
            self.add_item_to_inventory(player_id, item);
            given.push((info.name.clone(), *count));
        }

        // 5~7) 金币 / 经验 / 元宝（`:11456-11458`）。
        // `Settings.DropRate` / `Settings.ExpRate` 默认都是 1.0（`Server/Settings.cs:112`），
        // 本作固定 1.0，因此直接按配置值发放（`(uint)(reward * 1.0F)` == `reward`）。
        let gold = definition.script.gold_reward;
        let exp = definition.script.exp_reward;
        let credit = definition.script.credit_reward;
        if gold > 0 {
            self.gain_gold(player_id, gold);
        }
        if exp > 0 {
            self.gain_experience(player_id, exp);
        }
        if credit > 0 {
            self.gain_credit(player_id, credit);
        }

        // `RecalculateQuestBag()`（`:11454`）：任务结束后清掉不再被任何进行中任务需要的
        // 任务物品（含携带物品与收集目标），每清一格发 `S.DeleteQuestItem`。
        self.recalculate_quest_bag(player_id);

        // 扣完收集物品后，其它进行中任务的计数也要跟着降。
        self.refresh_quest_items(player_id);

        Ok(QuestReport {
            quest_index,
            name: definition.info.name.clone(),
            completed: true,
            task_list: report.map(|report| report.task_list).unwrap_or_default(),
            gold,
            exp,
            credit,
            items: given,
        })
    }

    /// 放弃任务（`C.AbandonQuest`，Crystal `PlayerObject.AbandonQuest`，`PlayerObject.cs:11462-11471`）。
    ///
    /// Crystal 只做两件事：`SendUpdateQuest(Remove)` + `RecalculateQuestBag()`（把不再需要的
    /// 任务物品从任务背包里清掉）。本作对应「删进度 + 回收不再需要的任务物品」。
    ///
    /// # Errors
    /// 没接过这个任务时返回 [`QuestError::NotTaken`]。
    pub fn abandon_quest(&mut self, player_id: u32, quest_index: i32) -> Result<(), QuestError> {
        let Some(player) = self.players.get(&player_id) else {
            return Err(QuestError::NotInWorld(player_id));
        };
        if self.quest_definition(quest_index).is_none() {
            return Err(QuestError::NoSuchQuest(quest_index));
        }
        let Some(index) = player
            .quests
            .iter()
            .position(|progress| progress.quest_index == quest_index && progress.is_in_progress())
        else {
            return Err(QuestError::NotTaken(quest_index));
        };
        let removed = self.players[&player_id].quests[index].clone();
        if let Some(player) = self.players.get_mut(&player_id) {
            player.quests.remove(index);
        }
        // `SendUpdateQuest(quest, QuestState.Remove)`（`:11468`）：先按**移除前**的进度组包。
        self.send_quest_change_for(player_id, &removed, QuestState::Remove, false);
        // Crystal 只做 `SendUpdateQuest(Remove)` + `RecalculateQuestBag()`：不再需要的任务物品
        // 从任务物品栏清掉（携带物品也在其中），普通背包里的旧物品不主动收。
        self.recalculate_quest_bag(player_id);
        Ok(())
    }

    /// 分享任务（`C.ShareQuest`，Crystal `PlayerObject.ShareQuest`，`:11472-11492`）。
    ///
    /// 只发给队伍里**同图 + `Globals.DataRange = 16` 内 + 活着 + 不是自己**的成员
    /// （`:11476-11486`），包是 `S.ShareQuest{QuestIndex, SharerName}`。
    /// 一个都没发出去时给本人一条系统提示（`ServerTextKeys.QuestNotShared`，`:11488-11491`）。
    ///
    /// 返回实际分享到的人数。
    ///
    /// # Errors
    /// 没在队伍里或附近没有队友时返回 [`QuestError::NotShared`]；玩家不在世界返回 [`QuestError::NotInWorld`]。
    pub fn share_quest(&mut self, player_id: u32, quest_index: i32) -> Result<usize, QuestError> {
        let Some(player) = self.players.get(&player_id) else {
            return Err(QuestError::NotInWorld(player_id));
        };
        let sharer_name = player.name.clone();
        let map = player.map.clone();
        let position = player.position;
        let members = self.group_members(player_id).to_vec();
        let recipients: Vec<u32> = members
            .into_iter()
            .filter(|member_id| *member_id != player_id)
            .filter(|member_id| {
                self.players.get(member_id).is_some_and(|member| {
                    member.map == map
                        && member
                            .position
                            .in_range(position, crate::group::GROUP_EXP_RANGE)
                        && !member.dead
                })
            })
            .collect();
        if recipients.is_empty() {
            let message = self.text(player_id, mir_core::i18n::TextKey::QuestShareNeedsParty);
            self.send_system_chat(player_id, &message);
            return Err(QuestError::NotShared);
        }
        let frame = ServerPacket::ShareQuest(ShareQuest {
            quest_index,
            sharer_name,
        })
        .encode_frame();
        for member_id in &recipients {
            self.send_to_player(*member_id, &frame);
        }
        Ok(recipients.len())
    }

    /// 加元宝（Crystal `PlayerObject.GainCredit`；包是 `S.GainedCredit`，`ServerPackets.cs`）。
    pub fn gain_credit(&mut self, player_id: u32, credit: u32) {
        if credit == 0 {
            return;
        }
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        player.credit += i64::from(credit);
        let _ = player
            .sender
            .send(ServerPacket::GainedCredit(GainedCredit { credit }).encode_frame());
    }

    /// 从背包里扣掉 `count` 个模板名为 `item_name` 的物品（Crystal `TakeQuestItem`，
    /// `PlayerObject.cs:7778-7803`）。
    ///
    /// 返回实际扣掉的数量；不够时能扣多少扣多少（Crystal 的行为）。
    pub fn take_inventory_items(&mut self, player_id: u32, item_name: &str, count: u16) -> u16 {
        let Some(info) = self.item_info_by_name(item_name).cloned() else {
            return 0;
        };
        let Some(player) = self.players.get_mut(&player_id) else {
            return 0;
        };
        let mut remaining = count;
        let mut removed = 0;
        for slot in &mut player.inventory {
            if remaining == 0 {
                break;
            }
            let Some(entry) = slot else { continue };
            if entry.item_index != info.index {
                continue;
            }
            let taken = entry.count.min(remaining);
            let unique_id = entry.unique_id;
            let left = entry.count - taken;
            if left == 0 {
                *slot = None;
            } else {
                entry.count = left;
            }
            remaining -= taken;
            removed += taken;
            let _ = player.sender.send(
                ServerPacket::DeleteItem(DeleteItem {
                    unique_id,
                    count: taken,
                })
                .encode_frame(),
            );
        }
        removed
    }
}

// ---------------------------------------------------------------------------
// World：进度推进
// ---------------------------------------------------------------------------

impl World {
    /// 击杀推进：怪物死亡时给击杀者推进所有进行中任务的击杀目标
    /// （Crystal `MonsterObject.Die` → `EXPOwner.CheckGroupQuestKill(Info)`，`MonsterObject.cs:986-990`；
    /// 计数逻辑在 `QuestProgressInfo.ProcessKill`，`QuestProgressInfo.cs:385-396`）。
    ///
    /// 返回被推进的任务个数。
    ///
    /// 简化：Crystal 的 `ProcessKill` 只会给**第一个**名字匹配的任务条目 `++`，即使那一条已经完成
    /// （`NeedKill` 只看「有没有未完成的匹配项」，`ProcessKill` 却不看完成度）；任务里有两条
    /// 同怪物的击杀目标时，第二条永远涨不上去。本作改成「给第一个**未完成**的匹配项 `++`」，
    /// 这样两条目标都会推进——见 `docs/world/quests.md` 的异常清单。
    pub fn advance_quest_kills(&mut self, player_id: u32, monster_name: &str) -> usize {
        let Some(player) = self.players.get(&player_id) else {
            return 0;
        };
        let candidates: Vec<i32> = player
            .quests
            .iter()
            .filter(|progress| progress.is_in_progress())
            .map(|progress| progress.quest_index)
            .collect();
        let mut advanced = Vec::new();
        for quest_index in candidates {
            let Some(definition) = self.quest_definition(quest_index) else {
                continue;
            };
            let Some(player) = self.players.get_mut(&player_id) else {
                return advanced.len();
            };
            let Some(progress) = player
                .quests
                .iter_mut()
                .find(|progress| progress.quest_index == quest_index)
            else {
                continue;
            };
            let mut changed = false;
            for task in &definition.script.kill_tasks {
                if !monster_matches_task(&task.monster, monster_name) {
                    continue;
                }
                let key = normalize_item_name(&task.monster);
                let Some((_, count)) = progress
                    .kill_count
                    .iter_mut()
                    .find(|(name, _)| name == &key)
                else {
                    continue;
                };
                if *count < task.count {
                    *count += 1;
                    changed = true;
                }
                break;
            }
            if !changed {
                continue;
            }
            let completed = is_objectives_complete(&definition.script, progress);
            progress.objectives_met = completed;
            advanced.push((quest_index, completed));
        }
        if advanced.is_empty() {
            return 0;
        }
        // `Enqueue(new S.SendOutputMessage { Message = YouKilled(GameName), Type = Quest });`
        // （`PlayerObject.cs:11586`）——每推进一次发一条「你杀死了 X」。
        let message = self.format(
            player_id,
            mir_core::i18n::TextKey::QuestKilledMonster,
            &[("monster", monster_name)],
        );
        self.send_to_player(
            player_id,
            &ServerPacket::SendOutputMessage(SendOutputMessage {
                message,
                type_: OutputMessageType::Quest,
            })
            .encode_frame(),
        );
        let count = advanced.len();
        for (quest_index, _) in advanced {
            self.send_quest_change(player_id, quest_index, QuestState::Update, false);
        }
        count
    }

    /// 收集目标重算：按**任务物品栏**（外加普通背包，兼容旧存档）里该物品的当前数量刷新
    /// 进行中任务的 `item_collected`（Crystal `CheckNeedQuestItem` → `ProcessItem(Info.QuestInventory)`，
    /// `PlayerObject.cs:11539-11561`、`QuestProgressInfo.cs:398-408`）。
    ///
    /// 返回计数发生变化的任务个数；有变化时会各发一条 `S.ChangeQuest{Update}`。
    pub fn refresh_quest_items(&mut self, player_id: u32) -> usize {
        let counts = self.quest_item_counts(player_id);
        let Some(player) = self.players.get_mut(&player_id) else {
            return 0;
        };
        let mut changed = Vec::new();
        let targets: Vec<i32> = player
            .quests
            .iter()
            .filter(|progress| progress.is_in_progress() && !progress.item_collected.is_empty())
            .map(|progress| progress.quest_index)
            .collect();
        for quest_index in targets {
            let Some(progress) = player
                .quests
                .iter_mut()
                .find(|progress| progress.quest_index == quest_index)
            else {
                continue;
            };
            let mut touched = false;
            for (key, value) in &mut progress.item_collected {
                let current = counts.get(key).copied().unwrap_or(0);
                if *value != current {
                    *value = current;
                    touched = true;
                }
            }
            if touched {
                changed.push(quest_index);
            }
        }
        let count = changed.len();
        for quest_index in &changed {
            self.update_objectives(player_id, *quest_index);
        }
        for quest_index in changed {
            self.send_quest_change(player_id, quest_index, QuestState::Update, false);
        }
        count
    }

    /// 背包里每个物品模板（归一化名字）的总数量。
    #[must_use]
    pub fn inventory_item_counts(&self, player_id: u32) -> std::collections::BTreeMap<String, u16> {
        let mut counts: std::collections::BTreeMap<String, u16> = std::collections::BTreeMap::new();
        let Some(player) = self.players.get(&player_id) else {
            return counts;
        };
        for item in player.inventory.iter().flatten() {
            let Some(info) = self.item_info(item.item_index) else {
                continue;
            };
            let entry = counts.entry(normalize_item_name(&info.name)).or_insert(0);
            *entry = entry.saturating_add(item.count);
        }
        counts
    }

    /// 把玩家身上的任务进度按当前任务物品栏（+普通背包）重算一次（**不发包**，登录/存档前用）。
    pub fn sync_quest_items(&mut self, player_id: u32) {
        let counts = self.quest_item_counts(player_id);
        let indexes: Vec<i32> = self
            .players
            .get(&player_id)
            .map(|player| {
                player
                    .quests
                    .iter()
                    .map(|progress| progress.quest_index)
                    .collect()
            })
            .unwrap_or_default();
        if let Some(player) = self.players.get_mut(&player_id) {
            for progress in &mut player.quests {
                for (key, value) in &mut progress.item_collected {
                    *value = counts.get(key).copied().unwrap_or(0);
                }
            }
        }
        for quest_index in indexes {
            self.update_objectives(player_id, quest_index);
        }
    }

    /// 所有 `QuestType.Daily` 的任务编号（Crystal `QuestInfoList.Where(Type == Daily)`）。
    fn daily_quest_indexes(&self) -> Vec<i32> {
        self.game_data()
            .map(|data| {
                data.db
                    .quests
                    .iter()
                    .filter(|quest| {
                        QuestType::from_value(quest.quest_type) == Some(QuestType::Daily)
                    })
                    .map(|quest| quest.index)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 玩家今天已完成（且尚未被每日重置清掉）的每日任务数量。
    #[must_use]
    pub fn daily_completed_count(&self, player_id: u32) -> usize {
        let Some(player) = self.players.get(&player_id) else {
            return 0;
        };
        let daily = self.daily_quest_indexes();
        player
            .completed_quests
            .iter()
            .filter(|index| daily.contains(index))
            .count()
    }

    /// 当前角色**可接**的每日任务编号（按等级/职业/前置/是否已完成/是否已接筛选）。
    ///
    /// 登录时 `S.NewQuestInfo` 只用这个列表下发 Daily 任务（见 [`World::quest_login_packets`]），
    /// 这样客户端不会看到当天已完成的每日任务。
    #[must_use]
    pub fn daily_quest_candidates(&self, player_id: u32) -> Vec<i32> {
        if !self.players.contains_key(&player_id) {
            return Vec::new();
        }
        self.daily_quest_indexes()
            .into_iter()
            .filter(|quest_index| {
                let Some(definition) = self.quest_definition(*quest_index) else {
                    return false;
                };
                self.check_can_accept(&definition, player_id, *quest_index)
                    .is_ok()
            })
            .collect()
    }

    /// 惰性每日重置（Crystal `Envir.ProcessNewDay` → `ClearDailyQuests`，`Envir.cs:4856-4866`、
    /// `:5009-5024`）：把 `QuestType.Daily` 的完成记录清掉，让每日任务可以再接。
    ///
    /// 与 Crystal 的差异：
    /// * Crystal 在服务器**跨天（0 点）**时对所有角色（含离线）执行；本作按角色
    ///   `last_daily_reset_at` 在**登录/任务交互时**惰性补做，间隔 24h（[`daily_reset_due`]）；
    /// * 本作交付后保留一条 `state = Completed` 的进度（Crystal 直接删 `CurrentQuests`），
    ///   所以这里连这条进度一起删，否则再接会被 `AlreadyCompleted` 拒绝。
    ///
    /// 进行中的每日任务不打断（Crystal 只清 `CompletedQuests`）。清完发 `S.CompleteQuest` 整表
    /// （Crystal `GetCompletedQuests`）。返回清掉的任务数。
    pub fn reset_daily_quests(&mut self, player_id: u32) -> usize {
        let daily = self.daily_quest_indexes();
        if daily.is_empty() {
            return 0;
        }
        let Some(player) = self.players.get_mut(&player_id) else {
            return 0;
        };
        let mut removed = 0;
        for quest_index in daily {
            let had_completed = player.completed_quests.remove(&quest_index);
            let before = player.quests.len();
            player.quests.retain(|progress| {
                !(progress.quest_index == quest_index
                    && progress.state == QuestProgressState::Completed)
            });
            let removed_progress = player.quests.len() != before;
            if had_completed || removed_progress {
                removed += 1;
            }
        }
        if removed > 0 {
            self.send_completed_quests(player_id);
        }
        removed
    }

    /// 登录时的每日重置（在 `world.enter` **之前**改角色记录，session 补丁调用）。
    ///
    /// 与 [`World::reset_daily_quests`] 同一份判定与清理逻辑，但作用在**存档记录**的
    /// `quests` 字节块上：登录流程会先解码 `character.quests` 再 `enter`，所以必须在解码前调用；
    /// `last_daily_reset_at` 随记录一起落盘（`market::character_snapshot` 克隆 base 时自带）。
    /// 返回是否真的做了重置（调用方据此决定是否记日志/立即存盘）。
    pub fn apply_daily_reset(&self, character: &mut crate::store::Character, now_ms: i64) -> bool {
        if !daily_reset_due(character.last_daily_reset_at, now_ms) {
            return false;
        }
        character.last_daily_reset_at = now_ms;
        let daily = self.daily_quest_indexes();
        if daily.is_empty() {
            return true;
        }
        let Ok(mut save) = decode_quests(&character.quests) else {
            // 坏块交给登录流程按空存档处理；这里只更新时间戳，避免每次登录反复重试。
            return true;
        };
        save.completed_quests.retain(|index| !daily.contains(index));
        save.quests.retain(|progress| {
            !(daily.contains(&progress.quest_index)
                && progress.state == QuestProgressState::Completed)
        });
        character.quests = encode_quests(&save);
        true
    }

    /// 重新计算一个任务「目标是否全部达成」（不发包）。
    ///
    /// 对应 Crystal `SendUpdateQuest` 每次都会调的 `quest.CheckCompleted()`
    /// （`PlayerObject.cs:11634`、`QuestProgressInfo.cs:321-362`）。
    pub fn update_objectives(&mut self, player_id: u32, quest_index: i32) {
        let Some(definition) = self.quest_definition(quest_index) else {
            return;
        };
        if let Some(player) = self.players.get_mut(&player_id) {
            if let Some(progress) = player
                .quests
                .iter_mut()
                .find(|progress| progress.quest_index == quest_index)
            {
                progress.objectives_met = is_objectives_complete(&definition.script, progress);
            }
        }
    }

    /// 超时检查：到点把任务当成放弃（Crystal `QuestProgressInfo.SetTimer` 排的
    /// `DelayedType.Quest` → `CompleteQuest(TimeExpired)` → `AbandonQuest`，
    /// `QuestProgressInfo.cs:538-559`、`PlayerObject.cs:4364-4385`）。
    ///
    /// 返回本次超时的任务个数。
    pub fn tick_quests(&mut self, now: i64) -> usize {
        let mut ids: Vec<u32> = self.players.keys().copied().collect();
        ids.sort_unstable();
        let mut expired_total = 0;
        for player_id in ids {
            let Some(player) = self.players.get(&player_id) else {
                continue;
            };
            let expired: Vec<i32> = player
                .quests
                .iter()
                .filter(|progress| {
                    progress.is_in_progress()
                        && progress.expires_at > 0
                        && now >= progress.expires_at
                })
                .map(|progress| progress.quest_index)
                .collect();
            if expired.is_empty() {
                continue;
            }
            for quest_index in expired {
                tracing::debug!(
                    player = player_id,
                    quest = quest_index,
                    "任务超时，按放弃处理"
                );
                let _ = self.abandon_quest(player_id, quest_index);
                expired_total += 1;
            }
        }
        expired_total
    }
}

// ---------------------------------------------------------------------------
// World：下发
// ---------------------------------------------------------------------------

impl World {
    /// 进世界时该发的任务包（Crystal `PlayerObject` 的登录序列）：
    ///
    /// 1. `S.NewQuestInfo` × 全库任务（`GetQuestInfo`，`:1185`、`:1727-1731`）——
    ///    Crystal 把**所有**任务定义都推给客户端，客户端自己按 NPC/等级过滤；
    /// 2. `S.CompleteQuest`（`GetCompletedQuests`，`:1188`、`:11662-11668`）；
    /// 3. `S.ChangeQuest{Add}` × 已接任务（`:1207-1212`，注意这里 `trackQuest` 是 `false`）。
    ///
    /// 简化：**没有任务数据时一个包都不发**（`load_game_data = false` 的兜底世界、
    /// 或者数据库里一条任务都没有）。Crystal 在真实服务器上总会把全库任务推一遍，
    /// 但给空世界塞 `S.CompleteQuest{[]}` 只会让「进世界」的包序列无谓地变长。
    #[must_use]
    pub fn quest_login_packets(&self, player_id: u32) -> Vec<ServerPacket> {
        let mut packets = Vec::new();
        let Some(player) = self.players.get(&player_id) else {
            return packets;
        };
        if self.quest_count() == 0 && player.quests.is_empty() && player.completed_quests.is_empty()
        {
            return packets;
        }
        // 每日任务按角色筛选（本作扩展）：只下发当天还能接的 Daily 任务；已接的仍要下发，
        // 因为客户端要靠定义渲染 `S.ChangeQuest{Add}`（Crystal 全量下发，客户端自己过滤）。
        let daily_candidates: std::collections::BTreeSet<i32> =
            self.daily_quest_candidates(player_id).into_iter().collect();
        let in_progress: std::collections::BTreeSet<i32> = player
            .quests
            .iter()
            .filter(|progress| progress.is_in_progress())
            .map(|progress| progress.quest_index)
            .collect();
        let language = self.player_language(player_id);
        for quest_index in self.quest_indexes() {
            let is_daily = self
                .quest_definition(quest_index)
                .is_some_and(|definition| definition.info.quest_type == QuestType::Daily.value());
            if is_daily
                && !daily_candidates.contains(&quest_index)
                && !in_progress.contains(&quest_index)
            {
                continue;
            }
            if let Some(info) = self.client_quest_info(quest_index, language) {
                packets.push(ServerPacket::NewQuestInfo(NewQuestInfo { info }));
            }
        }
        packets.push(ServerPacket::CompleteQuest(CompleteQuest {
            completed_quests: player.completed_quests.iter().copied().collect(),
        }));
        for progress in &player.quests {
            if !progress.is_in_progress() {
                continue;
            }
            if let Some(packet) = self.quest_change_packet(player, progress, QuestState::Add, false)
            {
                packets.push(packet);
            }
        }
        packets
    }

    /// 下发给本人的「已完成任务列表」（`S.CompleteQuest`）。
    pub fn send_completed_quests(&mut self, player_id: u32) {
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        let completed_quests: Vec<i32> = player.completed_quests.iter().copied().collect();
        let _ = player
            .sender
            .send(ServerPacket::CompleteQuest(CompleteQuest { completed_quests }).encode_frame());
    }

    /// 发一条 `S.ChangeQuest`（按任务编号现查进度），返回这次操作的报告。
    fn send_quest_change(
        &mut self,
        player_id: u32,
        quest_index: i32,
        state: QuestState,
        track: bool,
    ) -> Option<QuestReport> {
        let progress = self
            .players
            .get(&player_id)?
            .quests
            .iter()
            .find(|progress| progress.quest_index == quest_index)?
            .clone();
        self.send_quest_change_for(player_id, &progress, state, track)
    }

    /// 发一条 `S.ChangeQuest`（进度按引用传入，供「已经删掉进度」的 Remove 用）。
    fn send_quest_change_for(
        &mut self,
        player_id: u32,
        progress: &QuestProgress,
        state: QuestState,
        track: bool,
    ) -> Option<QuestReport> {
        let player = self.players.get(&player_id)?;
        let packet = self.quest_change_packet(player, progress, state, track)?;
        let report = QuestReport {
            quest_index: progress.quest_index,
            name: self
                .quest_definition(progress.quest_index)
                .map_or_else(String::new, |definition| definition.info.name),
            completed: progress.is_completed(),
            task_list: match &packet {
                ServerPacket::ChangeQuest(change) => change.quest.task_list.clone(),
                _ => Vec::new(),
            },
            ..QuestReport::default()
        };
        if let Some(player) = self.players.get(&player_id) {
            let _ = player.sender.send(packet.encode_frame());
        }
        Some(report)
    }

    /// 组装一条 `S.ChangeQuest`（Crystal `SendUpdateQuest`，`PlayerObject.cs:11632-11660`）。
    fn quest_change_packet(
        &self,
        player: &Player,
        progress: &QuestProgress,
        state: QuestState,
        track: bool,
    ) -> Option<ServerPacket> {
        let definition = self.quest_definition(progress.quest_index)?;
        let now = self.wall_now();
        let language = self.player_language(player.object_id);
        let quest = self.client_progress(&definition, progress, now, language);
        Some(ServerPacket::ChangeQuest(ChangeQuest {
            quest,
            quest_state: state,
            track_quest: track,
        }))
    }

    /// 从 `[@TaskDescription]` 的 `{名称/颜色}` 标记里提取**可反查地图标题**的目标。
    ///
    /// 任务导航行：优先用 `[@TaskDescription]` 里能反查 `MapInfo.title` 的显式标记；
    /// 没有显式地图标记时，按**交付 NPC（无则发布 NPC）所在地图**推导。
    /// 两者都没有才输出“无导航目标”。
    fn task_navigation_lines(
        &self,
        definition: &QuestDefinition,
        language: mir_core::Language,
    ) -> Vec<String> {
        let maps = self
            .game_data
            .as_ref()
            .map_or(&[][..], |data| data.db.maps.as_slice());
        let mut lines = navigation_lines_from_task_description(
            &definition.script.task_description,
            maps,
            language,
            |file, fallback| self.localized_map_title(file, fallback, language),
        );
        if lines.is_empty()
            && let Some(line) = self.quest_npc_navigation_line(definition.info.index, language)
        {
            lines.push(line);
        }
        if lines.is_empty() {
            lines.push(text(language, TextKey::QuestNavigationNone).to_owned());
        }
        lines
    }

    /// 地图可读名：优先语言包 `maps` 覆盖（`LanguageLibrary::map_title`），
    /// 未收录时退回数据库 `MapInfo.title`，最后才用文件名。
    fn localized_map_title(
        &self,
        map_file: &str,
        data_title: &str,
        language: mir_core::Language,
    ) -> String {
        let Some(library) = self.languages.as_ref() else {
            return data_title.to_owned();
        };
        let localized = library.map_title(language, map_file);
        if localized == map_file {
            data_title.to_owned()
        } else {
            localized
        }
    }

    /// 由交付 NPC（缺失则发布 NPC）的 `map` 反查 `MapInfo`，生成导航行。
    fn quest_npc_navigation_line(
        &self,
        quest_index: i32,
        language: mir_core::Language,
    ) -> Option<String> {
        let npc = self
            .quest_finisher_npc(quest_index)
            .or_else(|| self.quest_publisher_npc(quest_index))?;
        let data = self.game_data.as_ref()?;
        let info = data.map_info(&npc.map)?;
        if info.title.trim().is_empty() {
            return None;
        }
        let title = self.localized_map_title(&info.file_name, info.title.trim(), language);
        Some(format_text(
            language,
            TextKey::QuestNavigationGoTo,
            &[("map", &title), ("file", &info.file_name)],
        ))
    }

    /// 组装客户端进度（Crystal `QuestProgressInfo.CreateClientQuestProgress`，
    /// `QuestProgressInfo.cs:576-586`，进度文本见 `UpdateTasks`，`:428-532`）。
    #[must_use]
    pub fn client_progress(
        &self,
        definition: &QuestDefinition,
        progress: &QuestProgress,
        now: i64,
        language: mir_core::Language,
    ) -> ClientQuestProgress {
        ClientQuestProgress {
            id: progress.quest_index,
            task_list: {
                let mut lines = task_list(definition, progress, language);
                lines.extend(self.task_navigation_lines(definition, language));
                lines
            },
            taken: progress.state != QuestProgressState::NotStarted,
            // Crystal 的 `Completed` 是「目标达成」（`EndDateTime` 被写），不是「已交付」。
            completed: progress.objectives_met,
            new: progress.taken_at > now - QUEST_NEW_WINDOW_MS,
        }
    }
}

/// 从 `[@TaskDescription]` 的 `{名称/颜色}` 标记提取导航行；纯函数便于单测。
///
/// 只有 `X` 与 `MapInfo.title`（大小写不敏感、去首尾空白）完全相等时才转成
/// `前往 <title>(<file_name>)`；NPC/物品名反查不到地图，直接忽略。
#[must_use]
pub fn navigation_lines_from_task_description<F>(
    task_description: &[String],
    maps: &[mir_db::MapInfo],
    language: mir_core::Language,
    mut title_for: F,
) -> Vec<String>
where
    F: FnMut(&str, &str) -> String,
{
    let raws: Vec<&str> = task_description
        .iter()
        .map(String::as_str)
        .filter(|line| !line.trim().is_empty())
        .collect();
    if raws.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    for raw in raws {
        let mut rest = raw;
        while let Some(open) = rest.find('{') {
            let tail = &rest[open + 1..];
            let Some(close) = tail.find('}') else {
                break;
            };
            let token = &tail[..close];
            let name = token.split('/').next().unwrap_or(token).trim();
            if !name.is_empty()
                && let Some(map) = maps.iter().find(|info| {
                    !info.title.trim().is_empty() && info.title.trim().eq_ignore_ascii_case(name)
                })
            {
                let title = title_for(&map.file_name, map.title.trim());
                let line = format_text(
                    language,
                    TextKey::QuestNavigationGoTo,
                    &[("map", &title), ("file", &map.file_name)],
                );
                if !lines.contains(&line) {
                    lines.push(line);
                }
            }
            rest = &tail[close + 1..];
        }
    }
    lines
}

/// 任务进度文本（Crystal `QuestProgressInfo.UpdateTasks`，`QuestProgressInfo.cs:428-532`）。
///
/// Crystal 的文案来自语言表（`ServerTextKeys.KillCountProgress` 等）；本作用
/// `TextKey::QuestTask*` 按 `language` 渲染。脚本里给了 `KillMessage` / `ItemMessage` /
/// `FlagMessage` 时用脚本文案（数据，来自 `Server.MirDB`；Crystal 的同名分支，
/// `:440-452`、`:470-482`、`:500-512`）。
#[must_use]
pub fn task_list(
    definition: &QuestDefinition,
    progress: &QuestProgress,
    language: mir_core::Language,
) -> Vec<String> {
    use mir_core::i18n::{TextKey, format_text, text};
    let info = &definition.info;
    let script = &definition.script;
    let mut lines = Vec::new();
    let done = |flag: bool| {
        if flag {
            text(language, TextKey::QuestTaskDoneSuffix)
        } else {
            ""
        }
    };

    if !script.kill_tasks.is_empty() {
        let all_done = script
            .kill_tasks
            .iter()
            .all(|task| progress.kill_count_of(&task.monster) >= task.count);
        if info.kill_message.is_empty() {
            for task in &script.kill_tasks {
                let count = progress.kill_count_of(&task.monster);
                if task.message.is_empty() {
                    lines.push(format_text(
                        language,
                        TextKey::QuestTaskKill,
                        &[
                            ("monster", &task.monster),
                            ("count", &count.min(task.count).to_string()),
                            ("total", &task.count.to_string()),
                        ],
                    ));
                } else {
                    lines.push(format!("{}{}", task.message, done(count >= task.count)));
                }
            }
        } else {
            lines.push(format!("{}{}", info.kill_message, done(all_done)));
        }
    }

    if !script.item_tasks.is_empty() {
        let all_done = script
            .item_tasks
            .iter()
            .all(|task| i32::from(progress.item_count_of(&task.item)) >= i32::from(task.count));
        if info.item_message.is_empty() {
            for task in &script.item_tasks {
                let count = progress.item_count_of(&task.item);
                if task.message.is_empty() {
                    lines.push(format_text(
                        language,
                        TextKey::QuestTaskItem,
                        &[
                            ("item", &task.item),
                            ("count", &count.min(task.count).to_string()),
                            ("total", &task.count.to_string()),
                        ],
                    ));
                } else {
                    lines.push(format!("{}{}", task.message, done(count >= task.count)));
                }
            }
        } else {
            lines.push(format!("{}{}", info.item_message, done(all_done)));
        }
    }

    if !script.flag_tasks.is_empty() {
        let all_done = script
            .flag_tasks
            .iter()
            .all(|task| progress.flags.contains(&task.number));
        if info.flag_message.is_empty() {
            for task in &script.flag_tasks {
                if task.message.is_empty() {
                    lines.push(format_text(
                        language,
                        TextKey::QuestTaskFlag,
                        &[
                            ("number", &task.number.to_string()),
                            ("done_suffix", done(progress.flags.contains(&task.number))),
                        ],
                    ));
                } else {
                    lines.push(format!(
                        "{}{}",
                        task.message,
                        done(progress.flags.contains(&task.number))
                    ));
                }
            }
        } else {
            lines.push(format!("{}{}", info.flag_message, done(all_done)));
        }
    }

    // `UpdateGotoTask`（`:527-532`）：目标全部达成后追加「去交任务」的提示。
    if progress.objectives_met && !info.goto_message.is_empty() {
        lines.push(info.goto_message.clone());
    }
    lines
}

/// 职业对应的 `RequiredClass` 位（Crystal `RequiredClass` 的取值，`Shared/Enums.cs:1068-1077`）。
#[must_use]
pub fn class_flag(class: mir_core::enums::MirClass) -> RequiredClass {
    use mir_core::enums::MirClass;
    match class {
        MirClass::Warrior => RequiredClass::Warrior,
        MirClass::Wizard => RequiredClass::Wizard,
        MirClass::Taoist => RequiredClass::Taoist,
        MirClass::Assassin => RequiredClass::Assassin,
        MirClass::Archer => RequiredClass::Archer,
    }
}

// ---------------------------------------------------------------------------
// 存档编解码
// ---------------------------------------------------------------------------

/// 存档编解码错误。
#[derive(Debug, thiserror::Error)]
pub enum QuestCodecError {
    /// 字节流不符合格式。
    #[error("failed to parse quest blob: {0}")]
    Protocol(#[from] ProtocolError),
    /// 任务条数超过上限。
    #[error("quest count {count} exceeds limit {limit}")]
    TooManyQuests {
        /// 存档里的条数。
        count: usize,
        /// 允许的上限。
        limit: usize,
    },
    /// 状态字节非法。
    #[error("invalid quest state byte {0}")]
    InvalidState(u8),
}

/// 存档里最多存多少条任务进度（Crystal 只受内存限制；这里按「每任务 1 条 + 余量」给个上限）。
pub const MAX_STORED_QUESTS: usize = 256;

/// 一个角色的任务存档（`characters.quests` 解出来的两个部分）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuestSave {
    /// 任务进度（Crystal `CharacterInfo.CurrentQuests`，本作含已完成记录）。
    pub quests: Vec<QuestProgress>,
    /// 已完成的任务编号（Crystal `CharacterInfo.CompletedQuests`，`CharacterInfo.cs:92`）。
    pub completed_quests: BTreeSet<i32>,
}

/// 编码任务字节块（`characters.quests`）。
///
/// 布局**仿照** Crystal `CharacterInfo.Save` 的 quest 段（`CharacterInfo.cs:488-492`、
/// `:514-518`）与 `QuestProgressInfo.Save`（`QuestProgressInfo.cs:291-318`），
/// 但把「模板 ID」换成模板名（本作没有 Crystal 那套全局模板 ID 的稳定映射）：
///
/// ```text
/// i32    任务条数
/// 每条：
///   i32    task 编号（QuestInfo.Index）
///   u8     状态（0 = NotStarted / 1 = InProgress / 2 = Completed）
///   u8     objectives_met（Crystal `Completed`：目标是否已达成）
///   i64    taken_at（Unix 毫秒）
///   i64    expires_at（Unix 毫秒，0 = 不限时）
///   i32    击杀计数条数 + 每条 { string 归一化怪物名, i32 计数 }
///   i32    收集计数条数 + 每条 { string 归一化物品名, u16 计数 }
///   i32    标记条数     + 每条 { i32 标记号 }
/// i32    已完成任务条数 + 每条 i32 任务编号
/// ```
///
/// 空块（新角色）写成 4 字节的「0 条 + 0 条」；解码时零长度 → 空存档。
#[must_use]
pub fn encode_quests(save: &QuestSave) -> Vec<u8> {
    let mut writer = PacketWriter::with_capacity(8 + save.quests.len() * 24);
    writer.write_count(save.quests.len());
    for quest in &save.quests {
        writer.write_i32(quest.quest_index);
        writer.write_u8(quest.state.value());
        writer.write_u8(u8::from(quest.objectives_met));
        writer.write_i64(quest.taken_at);
        writer.write_i64(quest.expires_at);
        writer.write_count(quest.kill_count.len());
        for (name, count) in &quest.kill_count {
            writer.write_string(name);
            writer.write_i32(*count);
        }
        writer.write_count(quest.item_collected.len());
        for (name, count) in &quest.item_collected {
            writer.write_string(name);
            writer.write_u16(*count);
        }
        writer.write_count(quest.flags.len());
        for number in &quest.flags {
            writer.write_i32(*number);
        }
    }
    writer.write_count(save.completed_quests.len());
    for quest_index in &save.completed_quests {
        writer.write_i32(*quest_index);
    }
    writer.into_inner()
}

/// 解码任务字节块，见 [`encode_quests`]。
pub fn decode_quests(bytes: &[u8]) -> Result<QuestSave, QuestCodecError> {
    if bytes.is_empty() {
        return Ok(QuestSave::default());
    }
    let mut reader = PacketReader::new(bytes);
    let count = reader.read_count()?;
    if count > MAX_STORED_QUESTS {
        return Err(QuestCodecError::TooManyQuests {
            count,
            limit: MAX_STORED_QUESTS,
        });
    }
    let mut quests = Vec::with_capacity(count);
    for _ in 0..count {
        let quest_index = reader.read_i32()?;
        let state_raw = reader.read_u8()?;
        let state = QuestProgressState::from_value(state_raw)
            .ok_or(QuestCodecError::InvalidState(state_raw))?;
        let objectives_met = reader.read_u8()? != 0;
        let taken_at = reader.read_i64()?;
        let expires_at = reader.read_i64()?;
        let kills = reader.read_count()?;
        let mut kill_count = Vec::with_capacity(kills.min(64));
        for _ in 0..kills {
            kill_count.push((reader.read_string()?, reader.read_i32()?));
        }
        let items = reader.read_count()?;
        let mut item_collected = Vec::with_capacity(items.min(64));
        for _ in 0..items {
            item_collected.push((reader.read_string()?, reader.read_u16()?));
        }
        let flags = reader.read_count()?;
        let mut flag_set = BTreeSet::new();
        for _ in 0..flags {
            flag_set.insert(reader.read_i32()?);
        }
        quests.push(QuestProgress {
            quest_index,
            state,
            objectives_met,
            taken_at,
            expires_at,
            kill_count,
            item_collected,
            flags: flag_set,
        });
    }
    let completed = reader.read_count()?;
    let mut completed_quests = BTreeSet::new();
    for _ in 0..completed {
        completed_quests.insert(reader.read_i32()?);
    }
    Ok(QuestSave {
        quests,
        completed_quests,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script() -> QuestScript {
        let lines: Vec<String> = "[@KILLTASKS]\nHen 2\n[@ITEMTASKS]\nEgg 3\n"
            .split('\n')
            .map(str::to_owned)
            .collect();
        QuestScript::parse("Test/1", &lines)
    }

    #[test]
    fn codec_roundtrip_keeps_every_field() {
        let mut progress = QuestProgress::new(42, 1_700_000_000_000, 3_600, &script());
        progress.kill_count[0].1 = 1;
        progress.item_collected[0].1 = 2;
        progress.flags.extend([7, 9]);
        let save = QuestSave {
            quests: vec![progress],
            completed_quests: BTreeSet::from([3, 5]),
        };
        let encoded = encode_quests(&save);
        let decoded = decode_quests(&encoded).expect("解码成功");
        assert_eq!(decoded, save);
    }

    #[test]
    fn empty_block_decodes_to_no_quests() {
        assert_eq!(decode_quests(&[]).unwrap(), QuestSave::default());
        assert_eq!(
            decode_quests(&encode_quests(&QuestSave::default())).unwrap(),
            QuestSave::default()
        );
    }

    #[test]
    fn objective_matching_ignores_case_and_spaces() {
        assert!(monster_matches_task("ForestYeti", "forestyeti"));
        assert!(monster_matches_task("Forest Yeti", "forestyeti"));
        // Crystal 是 `StartsWith`：怪物名更长也算命中。
        assert!(monster_matches_task("ForestYeti", "ForestYeti2"));
        assert!(!monster_matches_task("ForestYeti", "Yeti"));
        assert!(!monster_matches_task("", "anything"));
        assert!(item_matches_task("Bat Teeth", "batteeth"));
    }

    #[test]
    fn objectives_complete_only_when_every_task_is_done() {
        let script = script();
        let mut progress = QuestProgress::new(1, 0, 0, &script);
        assert!(!is_objectives_complete(&script, &progress));
        progress.kill_count[0].1 = 2;
        assert!(!is_objectives_complete(&script, &progress));
        progress.item_collected[0].1 = 3;
        assert!(is_objectives_complete(&script, &progress));
    }

    #[test]
    fn navigation_only_converts_real_map_titles() {
        let maps = vec![mir_db::MapInfo {
            file_name: "D401".to_owned(),
            title: "DeathValley".to_owned(),
            ..mir_db::MapInfo::default()
        }];
        let lines = navigation_lines_from_task_description(
            &["Go to {DeathValley/Yellow} and kill {Scyther/Crimson}, find {MasterMage} and carry {CannibalLeaves/LightSteelBlue}".to_owned()],
            &maps,
            mir_core::Language::En,
            |_, title| title.to_owned(),
        );
        assert_eq!(lines, vec!["Go to DeathValley (D401)".to_owned()]);
    }

    #[test]
    fn real_jev_quest_5_navigation_uses_handin_npc_map() {
        let Ok(data) = crate::gamedata::GameData::discover() else {
            return;
        };
        let mut world = World::new(1);
        world.set_game_data(std::sync::Arc::new(data));
        world.set_languages(std::sync::Arc::new(
            mir_core::i18n::LanguageLibrary::embedded(),
        ));
        world.spawn_npcs("0");
        let Some(definition) = world.quest_definition(5) else {
            return;
        };
        let Some(npc) = world.quest_finisher_npc(5) else {
            return;
        };
        let Some(info) = world
            .game_data
            .as_ref()
            .and_then(|data| data.map_info(&npc.map))
        else {
            return;
        };
        let progress = QuestProgress::new(5, 0, 0, &definition.script);
        let client = world.client_progress(&definition, &progress, 0, mir_core::Language::ZhCn);
        let title =
            world.localized_map_title(&info.file_name, info.title.trim(), mir_core::Language::ZhCn);
        let expected = mir_core::i18n::format_text(
            mir_core::Language::ZhCn,
            mir_core::i18n::TextKey::QuestNavigationGoTo,
            &[("map", &title), ("file", &info.file_name)],
        );
        assert!(
            client.task_list.contains(&expected),
            "quest 5 should derive navigation from its handin NPC map: {expected:?} / {:?}",
            client.task_list
        );
    }

    #[test]
    fn empty_task_description_has_no_task_navigation_line() {
        assert!(
            navigation_lines_from_task_description(
                &[],
                &[],
                mir_core::Language::ZhCn,
                |_, title| title.to_owned()
            )
            .is_empty()
        );
        // 非空、但没有可反查地图标记：不能误报，也不能塞 NPC/物品名。
        assert!(
            navigation_lines_from_task_description(
                &["Carry {CannibalLeaves/LightSteelBlue} to {YoungJane/LimeGreen}".to_owned()],
                &[],
                mir_core::Language::En,
                |_, title| title.to_owned(),
            )
            .is_empty()
        );
    }

    #[test]
    fn no_npc_or_map_gets_explicit_no_navigation_line() {
        let world = World::new(1);
        let definition = QuestDefinition {
            info: QuestInfo::default(),
            script: std::sync::Arc::new(QuestScript::default()),
        };
        let progress = QuestProgress::new(0, 0, 0, &definition.script);
        let client = world.client_progress(&definition, &progress, 0, mir_core::Language::ZhCn);
        assert_eq!(
            client.task_list,
            vec![
                mir_core::i18n::text(
                    mir_core::Language::ZhCn,
                    mir_core::i18n::TextKey::QuestNavigationNone,
                )
                .to_owned()
            ]
        );
    }

    #[test]
    fn state_values_roundtrip() {
        for state in [
            QuestProgressState::NotStarted,
            QuestProgressState::InProgress,
            QuestProgressState::Completed,
        ] {
            assert_eq!(QuestProgressState::from_value(state.value()), Some(state));
        }
        assert_eq!(QuestProgressState::from_value(9), None);
    }
}
