//! 攻城（沙巴克攻城 / castle siege）：`ConquestInfo` 的运行期状态、时间表、计分与城主收益。
//!
//! Crystal 侧的关键源码（全部按行号引用）：
//!
//! | 主题 | 源码 |
//! | --- | --- |
//! | 静态配表 | `Server/MirDatabase/ConquestInfo.cs:6-51`（读 `:53-141`，写 `:143-218`） |
//! | 运行期状态与存档 | `Server/MirDatabase/ConquestGuildInfo.cs:7-92` |
//! | 时间表 / 开战 / 停战 / 计分 | `Server/MirObjects/ConquestObject.cs`（整份文件） |
//! | 装载 / 存盘 / 处理节拍 | `Server/MirEnvir/Envir.cs:3119-3170`、`:2734-2749`、`:2339-2346` |
//! | 「申请攻城」 | 没有客户端包；NPC 脚本动作 `SCHEDULECONQUEST <index>`（解析 `Server/MirObjects/NPC/NPCSegment.cs:1077-1080`，执行 `:4287-4298`） |
//! | 城主收益 | `NPCScript.PriceRate`（`Server/MirObjects/NPC/NPCScript.cs:111-133`）+ `GoldStorage`（`:1211-1214`、`PlayerObject.cs:8150`）+ `TAKECONQUESTGOLD`（`NPCSegment.cs:4223-4236`） |
//! | 战区判定 | `Map.GetConquest`（`Server/MirEnvir/Map.cs:2412-2422`）、`PlayerObject.CheckConquest`（`PlayerObject.cs:13949-13979`） |
//!
//! # 与 Crystal 的取舍
//!
//! * **没有新建协议包**：Crystal 就没有攻城相关的包（`GuildRequestWar` 是「行会战申请」的
//!   服务端弹窗，`Shared/ServerPackets.cs:4709`；`GuildWarReturn` 是行会战名字回填，
//!   `Shared/ClientPackets.cs:1787`）。攻城的交互全部走 NPC 脚本 + `S.Chat` 系统公告。
//! * **建筑/旗帜没做**：守卫（`ConquestGuildArcherInfo`）、城门、城墙、器械、旗子、控制点
//!   NPC 的实体、血量、修理都没实现；见 `docs/world/conquest.md` 的「未做」。
//!   计分只需要坐标，所以 `Classic` / `KingOfHill` / `ControlPoints` 三种玩法都能跑。
//! * **时间**：Crystal 用 `Envir.Now`（墙上时钟的 `DateTime`）算时间表、用 `Envir.Time`
//!   （单调毫秒）算节流。本作把墙上时钟统一成「Unix 毫秒」，由
//!   [`World::wall_now`](crate::World::wall_now) 提供；测试可以显式传入任意时刻。

use std::collections::BTreeMap;

use mir_core::Point;
use mir_core::enums::{ChatType, ConquestGame, ConquestType};
use mir_core::i18n::TextKey;
use mir_db::ConquestInfo;
use mir_protocol::ServerPacket;
use mir_protocol::packets::server::{
    Chat, ConquestControlPoint, ConquestInfo as ConquestInfoPacket,
    ConquestObject as ConquestObjectPacket, ConquestObjectType,
    ConquestResult as ConquestResultPacket,
};

use crate::npc::Npc;
use crate::store::ConquestRecord;
use crate::world::{Player, World};

/// 「王者之城」（`ConquestGame.KingOfHill`）的满分：Crystal `MAX_KING_POINTS = 18`，
/// 注释写「3 minutes」——`Process` 每 10 秒加一分（`Envir.cs:2341`），
/// 18 分 = 180 秒（`ConquestObject.cs:74`）。
pub const MAX_KING_POINTS: i32 = 18;

/// 控制点的满分：Crystal `MAX_CONTROL_POINTS = 6`（`ConquestObject.cs:75`），
/// 6 分 × 10 秒 = 60 秒。
pub const MAX_CONTROL_POINTS: i32 = 6;

/// 控制点的争夺半径：Crystal `Functions.InRange(controlFlag.Info.Location, …, 3)`
/// （`ConquestObject.cs:740`）。
pub const CONTROL_POINT_RANGE: i32 = 3;

/// 攻城处理的周期：Crystal `conquestTime = Time + Settings.Second * 10`（`Envir.cs:2341`）。
pub const CONQUEST_PROCESS_INTERVAL_MS: i64 = 10_000;

/// 时间表检查周期：Crystal `ScheduleTimer = Envir.Time + Settings.Minute`
/// （`ConquestObject.cs:505`）。
pub const CONQUEST_SCHEDULE_INTERVAL_MS: i64 = 60_000;

/// 一天的分钟数（时间表按「当天第几分钟」比较，`ConquestObject.cs:467-469`）。
pub const MINUTES_PER_DAY: i64 = 24 * 60;

/// 一毫秒等于多少……
const MS_PER_MINUTE: i64 = 60_000;
/// 一天的毫秒数。
const MS_PER_DAY: i64 = 24 * 60 * MS_PER_MINUTE;

/// 攻城相关的服务器配置。
///
/// **Crystal 没有这些配置**：`Settings.cs` 里只有行会战的 `Guild_WarCost`（`:329`，
/// 配置项 `[Guilds] WarCost`，`:1344`），攻城完全由 NPC 脚本控制，
/// `SCHEDULECONQUEST` 既不查等级也不收钱（`NPCSegment.cs:4287-4298`）。
/// 本作把默认值都设成 0（= Crystal 行为），只在运营需要时才配上门槛。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ConquestSettings {
    /// 申请攻城要交的金币；**本作扩展**，默认 0（Crystal 无此费用）。
    pub war_cost: u32,
    /// 申请攻城要求的行会等级；**本作扩展**，默认 0（Crystal 无此门槛）。
    pub required_guild_level: u8,
}

/// 一个控制点的运行期状态。
///
/// 对应 Crystal `ConquestGuildInfo.ControlPoints` 的键与值
/// （`Dictionary<ConquestGuildFlagInfo, Dictionary<GuildObject, int>>`，
/// `ConquestGuildInfo.cs:15`）：这里拆成「归属」+「各会分数」。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlPointState {
    /// Crystal `ConquestFlagInfo.Index`。
    pub index: i32,
    /// Crystal `ConquestFlagInfo.Name`（广播里用）。
    pub name: String,
    /// Crystal `ConquestFlagInfo.Location`。
    pub location: Point,
    /// 当前归属行会（`ConquestGuildFlagInfo.Guild`），`-1` = 无主。
    pub owner: i32,
    /// 各会分数（Crystal 的 `Dictionary<GuildObject, int>`）。
    pub points: BTreeMap<i32, i32>,
}

/// 攻城器械运行期状态（**本作扩展**：Crystal 用 `ConquestGuildGateInfo` 等
/// 存档结构 + `MonsterObject` 实体，本作用独立的轻量对象表示）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConquestObjectState {
    /// 对象 ID。
    pub object_id: u32,
    /// 类型。
    pub kind: ConquestObjectType,
    /// 位置。
    pub location: Point,
    /// 朝向。
    pub direction: mir_core::enums::MirDirection,
    /// 当前 HP。
    pub hp: i32,
    /// 最大 HP。
    pub max_hp: i32,
    /// 归属行会。
    pub owner: i32,
    /// 下次射击时刻（仅箭塔用，Unix 毫秒）。
    pub next_shot: i64,
}

impl ConquestObjectState {
    /// 是否已摧毁。
    #[must_use]
    pub const fn destroyed(&self) -> bool {
        self.hp <= 0
    }
}

/// 一个攻城区的运行期状态（Crystal `ConquestObject`，
/// `Server/MirObjects/ConquestObject.cs:7`）。
#[derive(Debug, Clone)]
pub struct Conquest {
    /// 静态配表（Crystal `ConquestObject.Info`）。
    pub info: ConquestInfo,
    /// 攻城区所在地图文件名（`ConquestInfo.MapIndex` 解析结果；`None` = 配表指向不存在的
    /// 地图，Crystal 在 `Envir.LoadConquests` 里直接跳过，`:3130-3132`）。
    pub map: Option<String>,
    /// 皇宫地图文件名（`ConquestInfo.PalaceIndex`；`Classic` / `CapturePalace` 的判定场）。
    pub palace_map: Option<String>,
    /// 附加地图文件名（`ConquestInfo.ExtraMaps`）。
    pub extra_maps: Vec<String>,
    /// 城主行会 ID（`ConquestGuildInfo.Owner`），`-1` = 无主。
    pub owner: i32,
    /// 已排队的攻打方行会 ID（`ConquestGuildInfo.AttackerID`），`-1` = 没人申请。
    pub attacker: i32,
    /// 城主税收池（`ConquestGuildInfo.GoldStorage`）。
    pub gold_storage: u32,
    /// 城主设置的加价百分比（`ConquestGuildInfo.NPCRate`）。
    pub npc_rate: u8,
    /// 是否正在开战（`ConquestObject.WarIsOn`）。
    pub war_is_on: bool,
    /// 本次战争的类型（`ConquestObject.StartType`）。
    pub start_type: ConquestType,
    /// 本次战争的玩法（`ConquestObject.GameType`）。
    pub game_type: ConquestGame,
    /// 开战时刻（Unix 毫秒；`ConquestObject.WarStartTime`）。
    pub war_start_time: i64,
    /// 停战时刻（Unix 毫秒；`ConquestObject.WarEndTime`，Crystal **只对 `Forced` 赋值**，
    /// `:320-325`）。`0` = 未设置。
    pub war_end_time: i64,
    /// 下一次时间表检查的时刻（Unix 毫秒；`ConquestObject.ScheduleTimer`）。
    pub schedule_timer: i64,
    /// **本作扩展**：下一次自动开战的时刻（Unix 毫秒，`0` = 未知）。
    pub next_war_at: i64,
    /// 「王者之城」计分（`ConquestObject.KingPoints`，`:77`）。
    pub king_points: BTreeMap<i32, i32>,
    /// 控制点列表（`ConquestObject.ControlPoints` 的键与归属）。
    pub control_points: Vec<ControlPointState>,
    /// 攻城器械（城门/城墙/箭塔），仅运行期；停战时清空。
    pub objects: Vec<ConquestObjectState>,
    /// 需要落盘（Crystal `ConquestGuildInfo.NeedSave`，`:24`）。
    pub dirty: bool,
}

impl Conquest {
    /// 用配表构造一个「无主、未开战、时间表待跑」的攻城区。
    #[must_use]
    pub fn new(info: ConquestInfo) -> Self {
        let control_points = info
            .control_points
            .iter()
            .map(|point| ControlPointState {
                index: point.index,
                name: point.name.clone(),
                location: point.location,
                owner: -1,
                points: BTreeMap::new(),
            })
            .collect();
        Self {
            info,
            map: None,
            palace_map: None,
            extra_maps: Vec::new(),
            owner: -1,
            attacker: -1,
            gold_storage: 0,
            npc_rate: 0,
            war_is_on: false,
            start_type: ConquestType::Request,
            game_type: ConquestGame::CapturePalace,
            war_start_time: 0,
            war_end_time: 0,
            // `i64::MIN` 让第一次 `process` 一定跑一遍时间表。
            schedule_timer: i64::MIN,
            next_war_at: 0,
            king_points: BTreeMap::new(),
            control_points,
            objects: Vec::new(),
            dirty: false,
        }
    }

    /// 攻城区主键（`ConquestInfo.Index`）。
    #[must_use]
    pub fn index(&self) -> i32 {
        self.info.index
    }

    /// 配表里的名字（`ConquestInfo.Name`）。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.info.name
    }

    /// 地图上某个坐标是否落在攻城区内：Crystal `Map.GetConquest` 的内层判断
    /// （`Server/MirEnvir/Map.cs:2418`）——`FullMap` 时整张图都算，否则按半径。
    #[must_use]
    pub fn covers(&self, map: &str, point: Point) -> bool {
        self.map.as_deref() == Some(map)
            && (self.info.full_map
                || self
                    .info
                    .location
                    .in_range(point, i32::from(self.info.size)))
    }

    /// 这张图是否属于攻城区（皇宫 / 附加地图的玩家在开战时一律算战区，
    /// 见 `ConquestObject.NPCVisibility`，`:383-421`）。
    #[must_use]
    pub fn owns_map(&self, map: &str) -> bool {
        self.map.as_deref() == Some(map)
            || self.palace_map.as_deref() == Some(map)
            || self.extra_maps.iter().any(|extra| extra == map)
    }

    /// 玩家是否在战区内：Crystal `PlayerObject.CheckConquest` 的两条 `EnterSabuk` 分支
    /// （`PlayerObject.cs:13949-13965`）。
    ///
    /// * 主地图：坐标落在 `Location ± Size` 内（或 `FullMap`）且正在开战；
    /// * 皇宫 / 附加地图：开战时**整张图**都算（`CheckConquest` 的 `tempConquest` 分支）。
    #[must_use]
    pub fn covers_player(&self, player: &Player) -> bool {
        if !self.war_is_on {
            return false;
        }
        if self.covers(&player.map, player.position) {
            return true;
        }
        self.palace_map.as_deref() == Some(player.map.as_str())
            || self.extra_maps.iter().any(|extra| extra == &player.map)
    }

    /// 时间表里的「开战分钟数」（`ConquestObject.cs:467`）。
    #[must_use]
    pub const fn start_minute(&self) -> i64 {
        self.info.start_hour as i64 * 60
    }

    /// 时间表里的「停战分钟数」（`ConquestObject.cs:468`）。
    #[must_use]
    pub const fn finish_minute(&self) -> i64 {
        self.start_minute() + self.info.war_length as i64
    }

    /// 星期掩码（周一 = 位 0），对应 `ConquestInfo` 的七个布尔
    /// （`ConquestObject.CheckDay`，`:442-463`）。
    #[must_use]
    pub fn day_mask(&self) -> u8 {
        u8::from(self.info.monday)
            | (u8::from(self.info.tuesday) << 1)
            | (u8::from(self.info.wednesday) << 2)
            | (u8::from(self.info.thursday) << 3)
            | (u8::from(self.info.friday) << 4)
            | (u8::from(self.info.saturday) << 5)
            | (u8::from(self.info.sunday) << 6)
    }

    /// 配表里的攻城类型（`ConquestInfo.Type`）。
    #[must_use]
    pub fn conquest_type(&self) -> ConquestType {
        ConquestType::from_value(self.info.conquest_type).unwrap_or(ConquestType::Request)
    }

    /// 配表里的玩法（`ConquestInfo.Game`）。
    #[must_use]
    pub fn configured_game(&self) -> ConquestGame {
        ConquestGame::from_value(self.info.game).unwrap_or(ConquestGame::CapturePalace)
    }

    /// 下一次时间窗开启的时刻（Unix 毫秒，`0` = 未来 8 天内都不开战）。
    ///
    /// **本作扩展**：Crystal 不存这个值，只在 `AutoSchedule` 里每分钟现算
    /// （`ConquestObject.cs:465-506`）。这里做成可查询的纯函数，供 GM 面板/日志使用。
    #[must_use]
    pub fn next_window_start(&self, now_wall: i64) -> i64 {
        let start_ms = self.start_minute() * MS_PER_MINUTE;
        // 已经在一轮窗口里：就是「现在」。
        let now_minute = minute_of_day(now_wall);
        if self.start_minute() <= now_minute
            && self.finish_minute() > now_minute
            && check_day(self, now_wall)
        {
            return now_wall;
        }
        let today = now_wall.div_euclid(MS_PER_DAY);
        for offset in 0..=7_i64 {
            let candidate = (today + offset) * MS_PER_DAY + start_ms;
            if candidate < now_wall {
                continue;
            }
            if check_day(self, candidate) {
                return candidate;
            }
        }
        0
    }
}

/// 当天第几分钟（`ConquestObject.AutoSchedule` 的 `now = Hour * 60 + Minute`，`:469`）。
///
/// 用整数运算而不是引入日期库：一天 1440 分钟，`Unix 毫秒 / 60000 % 1440`。
#[must_use]
pub fn minute_of_day(unix_ms: i64) -> i64 {
    let minutes = unix_ms.div_euclid(MS_PER_MINUTE);
    // 往正方向取模，负时间戳（1970 前）也不会算出负数。
    minutes.rem_euclid(MINUTES_PER_DAY)
}

/// 星期序号（周一 = 0，周日 = 6）。
///
/// `1970-01-01` 是**星期四**，所以「周一起算的序号 = (天数 + 3) % 7」。
#[must_use]
pub fn weekday_index(unix_ms: i64) -> u8 {
    let days = unix_ms.div_euclid(MS_PER_DAY);
    u8::try_from((days + 3).rem_euclid(7)).unwrap_or(0)
}

/// 时间表上的某一天是否开战：Crystal `ConquestObject.CheckDay`（`:442-463`）。
///
/// C# 的 `switch` 没有 `default` 时，控制流会落到最后的 `return true`——
/// 也就是说「星期几无法识别」时一律算开战日。本作的星期序号永远在 0..=6，
/// 所以这里等价于「查当天的布尔」。
#[must_use]
pub fn check_day(conquest: &Conquest, unix_ms: i64) -> bool {
    (conquest.day_mask() >> weekday_index(unix_ms)) & 1 == 1
}

/// 攻城操作失败原因。
///
/// 每个变体都对应 Crystal 的一处校验/提示；[`ConquestError::message`] 给中文文案
/// （Crystal 走 `GameLanguage.ServerTextMap`，本作的 i18n 词条还没覆盖攻城）。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConquestError {
    /// 对象不在世界里。
    #[error("object {0} is not in the world")]
    NotInWorld(u32),
    /// 自己不在行会（`CheckType.ConquestAvailable` 要求 `player.MyGuild != null`，
    /// `NPCSegment.cs:2764-2767`）。
    #[error("you are not in a guild")]
    NotInGuild,
    /// 不是会长（NPC 脚本 `[@REQUESTWAR]` 的门槛，`NPCScript.cs:1066-1069`）。
    #[error("only the guild leader can request a conquest")]
    NotLeader,
    /// 找不到这个攻城区（`ServerTextKeys.ConquestNotFound`，`NPCSegment.cs:1438`）。
    #[error("conquest area {0} not found")]
    NotFound(i32),
    /// 自己已经是城主（`NPCSegment.cs:4293` 的 `Guildindex != Owner`）。
    #[error("your guild already owns this city")]
    AlreadyOwner,
    /// 已经在开战中（`NPCSegment.cs:4293` 的 `!conquest.WarIsOn`）。
    #[error("a war is in progress for this city")]
    AtWar,
    /// 已经有人申请了（`CheckType.ConquestAvailable` 把 `AttackerID != -1` 判为不可用，
    /// `NPCSegment.cs:2764-2765`）。
    #[error("another guild has already requested this city's conquest")]
    AlreadyScheduled,
    /// 行会等级不够（**本作扩展**的门槛，Crystal 无此校验）。
    #[error("requesting a conquest requires guild level {required} (current {level})")]
    LevelTooLow {
        /// 需要的行会等级。
        required: u8,
        /// 现在的行会等级。
        level: u8,
    },
    /// 行会资金不够（**本作扩展**的收费，Crystal 无此校验）。
    #[error("requesting a conquest requires {required} gold (guild has {gold})")]
    NotEnoughGold {
        /// 需要的金币。
        required: u32,
        /// 行会现有金币。
        gold: u32,
    },
    /// 不是城主行会（`NPCSegment.cs:4229`、`:4244` 的 `Guildindex == Owner`）。
    #[error("only the owning guild can perform this action")]
    NotOwnerGuild,
}

impl ConquestError {
    /// 给玩家看的提示，按玩家当前语言渲染（Crystal 用 `GameLanguage.ServerTextMap`
    /// 的对应词条；本作对应 `TextKey::Conquest*`）。
    ///
    /// `#[error]` 上的中文只用于 `tracing` 日志（开发者可见）。
    #[must_use]
    pub fn message(&self, language: mir_core::Language) -> String {
        use mir_core::i18n::{TextKey, format_text};
        use std::string::ToString;
        match self {
            Self::NotInWorld(object_id) => format_text(
                language,
                TextKey::ConquestNotInWorld,
                &[("object_id", &object_id.to_string())],
            ),
            Self::NotInGuild => format_text(language, TextKey::ConquestNotInGuild, &[]),
            Self::NotLeader => format_text(language, TextKey::ConquestNotLeader, &[]),
            Self::NotFound(area) => format_text(
                language,
                TextKey::ConquestAreaNotFound,
                &[("area", &area.to_string())],
            ),
            Self::AlreadyOwner => format_text(language, TextKey::ConquestAlreadyOwner, &[]),
            Self::AtWar => format_text(language, TextKey::ConquestAtWar, &[]),
            Self::AlreadyScheduled => format_text(language, TextKey::ConquestAlreadyScheduled, &[]),
            Self::LevelTooLow { required, level } => format_text(
                language,
                TextKey::ConquestLevelTooLow,
                &[
                    ("required", &required.to_string()),
                    ("level", &level.to_string()),
                ],
            ),
            Self::NotEnoughGold { required, gold } => format_text(
                language,
                TextKey::ConquestNotEnoughGold,
                &[
                    ("required", &required.to_string()),
                    ("gold", &gold.to_string()),
                ],
            ),
            Self::NotOwnerGuild => format_text(language, TextKey::ConquestNotOwnerGuild, &[]),
        }
    }
}

/// 一次「申请攻城」的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WarRequestReport {
    /// 攻城区主键。
    pub conquest_index: i32,
    /// 申请方的行会 ID。
    pub guild_id: i32,
    /// 扣掉的金币（本作扩展的门槛，默认 0）。
    pub gold_spent: u32,
    /// 申请之后是否**立刻**开战（Crystal 的 `AutoSchedule` 只在时间窗内才开战，
    /// 其余情况只是排队，`:482-503`）。
    pub war_started: bool,
    /// 下一次自动开战的时刻（Unix 毫秒，0 = 未知）。
    pub next_war_at: i64,
}

/// 一轮攻城处理的结果（[`World::process_conquests_at`] 的返回值，测试与日志用）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConquestTickReport {
    /// 本轮开战的攻城区。
    pub started: Vec<i32>,
    /// 本轮停战的攻城区。
    pub ended: Vec<i32>,
    /// 本轮换城主的 `(攻城区, 新城主行会)`。
    pub captured: Vec<(i32, i32)>,
}

impl ConquestTickReport {
    /// 本轮是否有任何变化。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.started.is_empty() && self.ended.is_empty() && self.captured.is_empty()
    }
}

// ---------------------------------------------------------------------------
// 存档视图 ↔ 运行期
// ---------------------------------------------------------------------------

/// 把「行会 ID → 分数」编码成落盘文本（`12:3,15:1`，按 ID 升序）。
#[must_use]
pub fn encode_points(points: &BTreeMap<i32, i32>) -> String {
    points
        .iter()
        .filter(|(_, score)| **score != 0)
        .map(|(guild, score)| format!("{guild}:{score}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// [`encode_points`] 的逆运算；坏数据直接跳过（不 panic）。
#[must_use]
pub fn decode_points(text: &str) -> BTreeMap<i32, i32> {
    let mut points = BTreeMap::new();
    for item in text.split(',').filter(|item| !item.is_empty()) {
        let Some((key, value)) = item.split_once(':') else {
            continue;
        };
        let (Ok(key), Ok(value)) = (key.trim().parse::<i32>(), value.trim().parse::<i32>()) else {
            continue;
        };
        points.insert(key, value);
    }
    points
}

impl Conquest {
    /// 转成落盘视图。
    #[must_use]
    pub fn to_record(&self) -> ConquestRecord {
        ConquestRecord {
            conquest_index: self.info.index,
            // Crystal 用 `-1` 表示无主/无人申请，落库用 `NULL`。
            owner: (self.owner >= 0).then_some(self.owner),
            attacker: (self.attacker >= 0).then_some(self.attacker),
            gold_storage: self.gold_storage,
            npc_rate: self.npc_rate,
            war_is_on: self.war_is_on,
            war_start_time: self.war_start_time,
            war_end_time: self.war_end_time,
            start_type: u8::from(self.start_type),
            game_type: u8::from(self.game_type),
            schedule_start_hour: self.info.start_hour,
            schedule_war_length: self.info.war_length,
            schedule_days: self.day_mask(),
            king_points: encode_points(&self.king_points),
            flag_owners: encode_points(
                &self
                    .control_points
                    .iter()
                    .filter(|point| point.owner >= 0)
                    .map(|point| (point.index, point.owner))
                    .collect(),
            ),
            next_war_at: self.next_war_at,
        }
    }

    /// 用落盘视图覆盖运行期状态（静态配表不动）。
    ///
    /// 对应 Crystal `Envir.LoadConquests`：先从 `.mcd` 读 `ConquestGuildInfo`
    /// （`:3134-3140`），再按 `Owner` 把行会与攻城区互相绑定（`:3147-3152`）。
    pub fn apply_record(&mut self, record: &ConquestRecord) {
        self.owner = record.owner.unwrap_or(-1);
        self.attacker = record.attacker.unwrap_or(-1);
        self.gold_storage = record.gold_storage;
        self.npc_rate = record.npc_rate;
        self.war_is_on = record.war_is_on;
        self.war_start_time = record.war_start_time;
        self.war_end_time = record.war_end_time;
        self.start_type =
            ConquestType::from_value(record.start_type).unwrap_or(ConquestType::Request);
        self.game_type =
            ConquestGame::from_value(record.game_type).unwrap_or(ConquestGame::CapturePalace);
        self.king_points = decode_points(&record.king_points);
        self.next_war_at = record.next_war_at;
        // 控制点归属：`flag_owners` 是 `控制点ID:行会ID` 的列表。
        let owners = decode_points(&record.flag_owners);
        for point in &mut self.control_points {
            point.owner = owners.get(&point.index).copied().unwrap_or(-1);
        }
        self.dirty = false;
    }
}

// ---------------------------------------------------------------------------
// World：装载 / 申请 / 时间表 / 计分 / 城主收益
// ---------------------------------------------------------------------------

impl World {
    /// 攻城配置。
    #[must_use]
    pub fn conquest_settings(&self) -> ConquestSettings {
        self.conquest_settings
    }

    /// 设置攻城配置（服务器启动时从配置读，测试里直接构造）。
    pub fn set_conquest_settings(&mut self, settings: ConquestSettings) {
        self.conquest_settings = settings;
    }

    /// 全部攻城区（按配表顺序）。
    #[must_use]
    pub fn conquests(&self) -> &[Conquest] {
        &self.conquests
    }

    /// 按主键取攻城区（Crystal `Envir.Conquests.FirstOrDefault(x => x.Info.Index == …)`，
    /// `NPCSegment.cs:4240` 等）。
    #[must_use]
    pub fn conquest(&self, index: i32) -> Option<&Conquest> {
        self.conquests
            .iter()
            .find(|conquest| conquest.info.index == index)
    }

    /// 按主键取攻城区（可变）。
    pub fn conquest_mut(&mut self, index: i32) -> Option<&mut Conquest> {
        self.conquests
            .iter_mut()
            .find(|conquest| conquest.info.index == index)
    }

    /// 从已加载的 `Server.MirDB` 建出攻城区列表（Crystal `Envir.LoadConquests`，
    /// `Envir.cs:3119-3170`）。
    ///
    /// Crystal 对「配表指向的地图不存在」的攻城区直接跳过（`:3130-3132`）；
    /// 本作同样跳过，但保留一条 `warn` 日志。返回实际建立的攻城区数量。
    pub fn init_conquests(&mut self) -> usize {
        let Some(data) = self.game_data.clone() else {
            self.conquests.clear();
            return 0;
        };
        let infos = data.db.conquests.clone();
        let mut built = Vec::with_capacity(infos.len());
        for info in infos {
            let map = data
                .map_info_by_index(info.map_index)
                .map(|map| map.file_name.clone());
            let Some(map) = map else {
                tracing::warn!(
                    conquest = info.index,
                    map_index = info.map_index,
                    "攻城区引用的地图不存在，已跳过（ConquestObject 装载时会 continue）"
                );
                continue;
            };
            let palace_map = data
                .map_info_by_index(info.palace_index)
                .map(|map| map.file_name.clone());
            let extra_maps = info
                .extra_maps
                .iter()
                .filter_map(|index| {
                    data.map_info_by_index(*index)
                        .map(|map| map.file_name.clone())
                })
                .collect();
            let mut conquest = Conquest::new(info);
            conquest.map = Some(map);
            conquest.palace_map = palace_map;
            conquest.extra_maps = extra_maps;
            built.push(conquest);
        }
        self.conquests = built;
        self.conquests.len()
    }

    /// 把库里的攻城区状态灌进来（Crystal `Envir.LoadConquests` 读 `.mcd` 的那一段，
    /// `Envir.cs:3134-3163`）。
    ///
    /// 库里没有的攻城区保持「无主、未开战」；返回命中的条数。
    pub fn load_conquests(&mut self, records: Vec<ConquestRecord>) -> usize {
        if self.conquests.is_empty() {
            self.init_conquests();
        }
        let mut loaded = 0;
        for record in records {
            if let Some(conquest) = self.conquest_mut(record.conquest_index) {
                conquest.apply_record(&record);
                loaded += 1;
            }
        }
        loaded
    }

    /// 取走需要落盘的攻城区记录（Crystal `SaveConquests` 只写 `NeedSave` 的那些，
    /// `Envir.cs:2739`）。
    pub fn take_conquest_writes(&mut self) -> Vec<ConquestRecord> {
        let mut writes = Vec::new();
        for conquest in &mut self.conquests {
            if !conquest.dirty {
                continue;
            }
            conquest.dirty = false;
            writes.push(conquest.to_record());
        }
        writes
    }

    /// 标记某个攻城区需要落盘。
    pub fn mark_conquest_dirty(&mut self, index: i32) {
        if let Some(conquest) = self.conquest_mut(index) {
            conquest.dirty = true;
        }
    }

    /// 玩家是否在战区里：Crystal `PlayerObject.WarZone`（`PlayerObject.cs:24`），
    /// 由 `CheckConquest` / `EnterSabuk` / `LeaveSabuk` 维护（`:13949-13979`）。
    ///
    /// 本作不缓存这个布尔，按当前时间/位置现算（与 Crystal 每次移动后重算的 `WarZone`
    /// 等价），因此不需要给 [`Player`] 加字段。
    #[must_use]
    pub fn in_war_zone(&self, object_id: u32) -> bool {
        self.players
            .get(&object_id)
            .is_some_and(|player| self.conquests.iter().any(|c| c.covers_player(player)))
    }

    /// 某个行会拥有的攻城区（Crystal `GuildObject.Conquest`，`GuildObject.cs:80`）。
    #[must_use]
    pub fn guild_conquest(&self, guild_id: i32) -> Option<&Conquest> {
        self.conquests
            .iter()
            .find(|conquest| conquest.owner == guild_id)
    }

    /// 城主税收池（`ConquestGuildInfo.GoldStorage`）。
    #[must_use]
    pub fn conquest_gold_storage(&self, index: i32) -> u32 {
        self.conquest(index)
            .map_or(0, |conquest| conquest.gold_storage)
    }

    /// 申请攻城：Crystal 里由 NPC 脚本动作 `SCHEDULECONQUEST <index>` 完成
    /// （`NPCSegment.cs:4287-4298`）。
    ///
    /// ```csharp
    /// var conquest = Envir.Conquests.FirstOrDefault(z => z.Info.Index == tempInt);
    /// if (conquest == null) return;
    /// if (player.MyGuild != null && player.MyGuild.Guildindex != conquest.GuildInfo.Owner
    ///     && !conquest.WarIsOn)
    ///     conquest.GuildInfo.AttackerID = player.MyGuild.Guildindex;
    /// ```
    ///
    /// 本作在同样的三条校验之外加了两道**可配置**的门槛（等级/金币，默认 0 = 关闭），
    /// 见 [`ConquestSettings`]。Crystal 里「会长才能申请」「等级/金币」这些条件写在各服的
    /// NPC 脚本 `#IF` 段里，服务器代码本身不查。
    ///
    /// # Errors
    /// 见 [`ConquestError`]：每个变体对应上面的一段校验。
    pub fn request_conquest_war(
        &mut self,
        player_id: u32,
        conquest_index: i32,
    ) -> Result<WarRequestReport, ConquestError> {
        self.request_conquest_war_at(player_id, conquest_index, self.wall_now())
    }

    /// 同 [`World::request_conquest_war`]，但时间表按调用方给的**墙上时刻**判断
    /// （测试与 GM 工具用；生产路径传 [`World::wall_now`]）。
    ///
    /// # Errors
    /// 见 [`ConquestError`]。
    pub fn request_conquest_war_at(
        &mut self,
        player_id: u32,
        conquest_index: i32,
        now_wall: i64,
    ) -> Result<WarRequestReport, ConquestError> {
        let Some(player) = self.players.get(&player_id) else {
            return Err(ConquestError::NotInWorld(player_id));
        };
        let Some(reference) = player.guild else {
            return Err(ConquestError::NotInGuild);
        };
        if reference.rank_index != 0 {
            return Err(ConquestError::NotLeader);
        }
        let guild_id = reference.guild_id;
        let Some((guild_level, guild_gold)) =
            self.guild(guild_id).map(|guild| (guild.level, guild.gold))
        else {
            return Err(ConquestError::NotInGuild);
        };
        let settings = self.conquest_settings;
        if guild_level < settings.required_guild_level {
            return Err(ConquestError::LevelTooLow {
                required: settings.required_guild_level,
                level: guild_level,
            });
        }
        if guild_gold < settings.war_cost {
            return Err(ConquestError::NotEnoughGold {
                required: settings.war_cost,
                gold: guild_gold,
            });
        }
        let Some((at_war, owner, attacker)) = self
            .conquest(conquest_index)
            .map(|conquest| (conquest.war_is_on, conquest.owner, conquest.attacker))
        else {
            return Err(ConquestError::NotFound(conquest_index));
        };
        if at_war {
            return Err(ConquestError::AtWar);
        }
        if owner == guild_id {
            return Err(ConquestError::AlreadyOwner);
        }
        if attacker >= 0 {
            return Err(ConquestError::AlreadyScheduled);
        }

        // 收费（本作扩展）：走行会资金，与 `Guild_WarCost` 扣行会资金的口径一致
        // （`PlayerObject.cs:10324`）。
        if settings.war_cost > 0 {
            let player_name = player.name.clone();
            if let Some(guild) = self.guilds.get_mut(&guild_id) {
                guild.gold = guild.gold.saturating_sub(settings.war_cost);
            }
            self.mark_guild_dirty(guild_id);
            self.send_guild_gold_change(guild_id, 2, settings.war_cost, &player_name);
        }

        let inside_window = self.conquest(conquest_index).is_some_and(|conquest| {
            let now = minute_of_day(now_wall);
            conquest.start_minute() <= now && conquest.finish_minute() > now
        });
        if let Some(conquest) = self.conquest_mut(conquest_index) {
            conquest.attacker = guild_id;
            conquest.next_war_at = conquest.next_window_start(now_wall);
            conquest.dirty = true;
        }
        if inside_window {
            // 时间窗内申请：Crystal 的下一轮 `AutoSchedule`（最多 60 秒后）就会开战
            // （`ConquestObject.cs:482-503`）；本作立刻跑一轮，省掉最多一分钟的空窗。
            let _ = self.run_auto_schedule(conquest_index, now_wall);
        }
        let (war_started, next_war_at) = self
            .conquest(conquest_index)
            .map_or((false, 0), |conquest| {
                (conquest.war_is_on, conquest.next_war_at)
            });
        Ok(WarRequestReport {
            conquest_index,
            guild_id,
            gold_spent: settings.war_cost,
            war_started,
            next_war_at,
        })
    }

    /// 强制开战 / 停战：Crystal NPC 动作 `STARTCONQUEST <index>`（`NPCSegment.cs:4250-4286`）。
    ///
    /// ```csharp
    /// if (!conquest.WarIsOn) { conquest.StartType = ConquestType.Forced;
    ///                          conquest.StartWar(conquest.GameType); … WarStarted }
    /// else { conquest.WarIsOn = false; … WarStopped }
    /// ```
    ///
    /// 返回 `true` = 本次是**开战**，`false` = 本次是**停战**。
    ///
    /// # Errors
    /// 找不到攻城区时返回 [`ConquestError::NotFound`]。
    pub fn toggle_conquest_war(&mut self, conquest_index: i32) -> Result<bool, ConquestError> {
        let Some(won) = self.conquest(conquest_index).map(|c| c.war_is_on) else {
            return Err(ConquestError::NotFound(conquest_index));
        };
        let now_wall = self.wall_now();
        if won {
            self.end_war(conquest_index);
            Ok(false)
        } else {
            if let Some(conquest) = self.conquest_mut(conquest_index) {
                conquest.start_type = ConquestType::Forced;
            }
            self.start_war(conquest_index, now_wall);
            Ok(true)
        }
    }

    /// 取走城主税收：Crystal NPC 动作 `TAKECONQUESTGOLD <index>`（`NPCSegment.cs:4223-4236`）。
    ///
    /// ```csharp
    /// if (player.MyGuild != null && player.MyGuild.Guildindex == conquest.GuildInfo.Owner) {
    ///     player.MyGuild.Gold += conquest.GuildInfo.GoldStorage;
    ///     player.MyGuild.SendServerPacket(new S.GuildStorageGoldChange { Type = 3, Amount = … });
    ///     conquest.GuildInfo.GoldStorage = 0;
    /// }
    /// ```
    ///
    /// 返回取走的金额。
    ///
    /// # Errors
    /// 不在行会 / 不是城主行会 / 找不到攻城区。
    pub fn take_conquest_gold(
        &mut self,
        player_id: u32,
        conquest_index: i32,
    ) -> Result<u32, ConquestError> {
        let Some(player) = self.players.get(&player_id) else {
            return Err(ConquestError::NotInWorld(player_id));
        };
        let Some(reference) = player.guild else {
            return Err(ConquestError::NotInGuild);
        };
        let name = player.name.clone();
        let Some((owner, amount)) = self
            .conquest(conquest_index)
            .map(|conquest| (conquest.owner, conquest.gold_storage))
        else {
            return Err(ConquestError::NotFound(conquest_index));
        };
        if owner != reference.guild_id {
            return Err(ConquestError::NotOwnerGuild);
        }
        if amount == 0 {
            return Ok(0);
        }
        if let Some(guild) = self.guilds.get_mut(&reference.guild_id) {
            guild.gold = guild.gold.saturating_add(amount);
        }
        self.mark_guild_dirty(reference.guild_id);
        if let Some(conquest) = self.conquest_mut(conquest_index) {
            conquest.gold_storage = 0;
            conquest.dirty = true;
        }
        self.send_guild_gold_change(reference.guild_id, 3, amount, &name);
        Ok(amount)
    }

    /// 设置城主加价：Crystal NPC 动作 `SETCONQUESTRATE <index> <percent>`
    /// （`NPCSegment.cs:4237-4249`）。
    ///
    /// # Errors
    /// 不在行会 / 不是城主行会 / 找不到攻城区。
    pub fn set_conquest_rate(
        &mut self,
        player_id: u32,
        conquest_index: i32,
        rate: u8,
    ) -> Result<(), ConquestError> {
        let Some(player) = self.players.get(&player_id) else {
            return Err(ConquestError::NotInWorld(player_id));
        };
        let Some(reference) = player.guild else {
            return Err(ConquestError::NotInGuild);
        };
        let Some(conquest) = self.conquest_mut(conquest_index) else {
            return Err(ConquestError::NotFound(conquest_index));
        };
        if conquest.owner != reference.guild_id {
            return Err(ConquestError::NotOwnerGuild);
        }
        conquest.npc_rate = rate;
        conquest.dirty = true;
        Ok(())
    }

    /// 攻城箭塔的生成：Crystal `ConquestGuildArcherInfo.Spawn`（`ConquestGuildInfo.cs:518-544`）。
    ///
    /// Crystal 在 `ConquestObject.Bind → LoadArchers`（`:203`、`:225-231`）里把
    /// `Info.ConquestGuards` 的每条记录刷成一只 `ConquestArcher`（AI 必须正好是 80，
    /// 否则 `return`，`:525`）。
    ///
    /// 本作简化的地方：
    ///
    /// * 只刷箭塔（`info.guards`），城门/城墙/器械（AI 81/82/72）没有对应实现；
    /// * 不存血量（`ConquestGuildArcherInfo.Alive`），重启后一律满血重刷；
    /// * 只在攻城区的地图已经加载时刷（没有地形就落不了点）。
    ///
    /// 返回刷出的箭塔对象 ID。
    pub fn spawn_conquest_archers(&mut self, index: i32) -> Vec<u32> {
        let Some((map, guards)) = self
            .conquest(index)
            .map(|conquest| (conquest.map.clone(), conquest.info.guards.clone()))
        else {
            return Vec::new();
        };
        let Some(map) = map else {
            return Vec::new();
        };
        let mut spawned = Vec::with_capacity(guards.len());
        for guard in guards {
            let Some(kind) = self
                .game_data
                .as_ref()
                .and_then(|data| data.monster(guard.mob_index))
                .cloned()
            else {
                tracing::debug!(
                    conquest = index,
                    mob_index = guard.mob_index,
                    "箭塔模板不存在，跳过（Crystal 的 GetMonsterInfo 返回 null 后 return）"
                );
                continue;
            };
            if !kind.is_conquest_archer() {
                tracing::debug!(
                    conquest = index,
                    mob_index = guard.mob_index,
                    ai = kind.ai,
                    "箭塔模板的 AI 不是 80，跳过（ConquestGuildArcherInfo.cs:525）"
                );
                continue;
            }
            let object_id = self.spawn_monster(&map, kind, guard.location);
            if let Some(monster) = self.monster_mut(object_id) {
                monster.conquest = Some(index);
            }
            spawned.push(object_id);
        }
        spawned
    }

    /// 攻城器械：按配表的城门/城墙/箭塔位置生成（无配表数据时用中心默认布点）。
    ///
    /// 简化：Crystal 用 `ConquestGuildGateInfo` 等存档结构 + 怪物实体；本作只在开战
    /// 期间维护轻量对象，停战清空，不持久化耐久。
    pub fn spawn_conquest_objects(&mut self, index: i32) -> Vec<u32> {
        let Some(conquest) = self.conquest(index) else {
            return Vec::new();
        };
        if !conquest.war_is_on {
            return Vec::new();
        }
        let Some(map) = conquest.map.clone() else {
            return Vec::new();
        };
        let owner = conquest.owner;
        let mut specs: Vec<(ConquestObjectType, Point, i32)> = Vec::new();
        for gate in &conquest.info.gates {
            specs.push((ConquestObjectType::Gate, gate.location, 5_000));
        }
        for wall in &conquest.info.walls {
            specs.push((ConquestObjectType::Wall, wall.location, 3_000));
        }
        for guard in &conquest.info.guards {
            specs.push((ConquestObjectType::Archer, guard.location, 2_000));
        }
        if specs.is_empty() {
            let base = conquest.info.location;
            specs.push((ConquestObjectType::Gate, base, 5_000));
            specs.push((
                ConquestObjectType::Wall,
                Point {
                    x: base.x,
                    y: base.y + 1,
                },
                3_000,
            ));
            specs.push((
                ConquestObjectType::Archer,
                Point {
                    x: base.x + 1,
                    y: base.y,
                },
                2_000,
            ));
        }
        let mut ids = Vec::with_capacity(specs.len());
        for (kind, location, max_hp) in specs {
            let object_id = self.allocate_object_id();
            if let Some(conquest) = self.conquest_mut(index) {
                conquest.objects.push(ConquestObjectState {
                    object_id,
                    kind,
                    location,
                    direction: mir_core::enums::MirDirection::Down,
                    hp: max_hp,
                    max_hp,
                    owner,
                    next_shot: 0,
                });
            }
            ids.push(object_id);
        }
        self.broadcast_conquest_state(index);
        let _ = map;
        ids
    }

    /// 某个格子是否被未摧毁的城门/城墙挡住。
    #[must_use]
    pub fn conquest_object_blocking(&self, map: &str, cell: Point) -> bool {
        self.conquests.iter().any(|conquest| {
            conquest.map.as_deref() == Some(map)
                && conquest.objects.iter().any(|object| {
                    !object.destroyed()
                        && matches!(
                            object.kind,
                            ConquestObjectType::Gate | ConquestObjectType::Wall
                        )
                        && object.location == cell
                })
        })
    }

    /// 格子上的攻城器械对象 ID（供近战目标选择）。
    #[must_use]
    pub fn conquest_object_at(&self, map: &str, cell: Point) -> Option<u32> {
        self.conquests.iter().find_map(|conquest| {
            if conquest.map.as_deref() != Some(map) {
                return None;
            }
            conquest
                .objects
                .iter()
                .find(|object| !object.destroyed() && object.location == cell)
                .map(|object| object.object_id)
        })
    }

    /// 器械受伤：广播 `S.ConquestObject`；HP 归零后 `destroyed = true`（城门/城墙不再挡路）。
    pub(crate) fn damage_conquest_object(&mut self, object_id: u32, damage: i32) -> i32 {
        let Some((index, map, actual, destroyed)) = self
            .conquests
            .iter()
            .find_map(|conquest| {
                let object = conquest
                    .objects
                    .iter()
                    .find(|object| object.object_id == object_id && !object.destroyed())?;
                Some((
                    conquest.info.index,
                    conquest.map.clone(),
                    object.hp,
                    object.kind,
                ))
            })
            .and_then(|(index, map, hp, _kind)| {
                let actual = damage.max(0).min(hp);
                if actual == 0 {
                    return None;
                }
                let object = self
                    .conquest_mut(index)?
                    .objects
                    .iter_mut()
                    .find(|object| object.object_id == object_id)?;
                object.hp = (object.hp - actual).max(0);
                Some((index, map, actual, object.destroyed()))
            })
        else {
            return 0;
        };
        self.broadcast_conquest_object(index, object_id);
        let _ = (map, destroyed);
        actual
    }

    /// 箭塔的伤害属性（走 `combat::resolve_player_attack` 的 MAC/AC 规则）。
    #[must_use]
    pub(crate) fn conquest_object_attack_stats(
        &self,
        object_id: u32,
    ) -> Option<crate::combat::CombatStats> {
        self.conquests
            .iter()
            .flat_map(|conquest| &conquest.objects)
            .find(|object| object.object_id == object_id && !object.destroyed())
            .map(|_| crate::combat::CombatStats {
                min_dc: 40,
                max_dc: 60,
                accuracy: 100,
                ..crate::combat::CombatStats::default()
            })
    }

    /// 显式时刻驱动箭塔（测试/运维用；不读墙上时钟）。
    pub fn process_conquest_towers_at(&mut self, now: i64) {
        self.process_conquest_towers(now);
    }

    /// 箭塔每 2 秒射击最近的敌对行会玩家（`owner == -1` 时所有玩家敌对）。
    pub(crate) fn process_conquest_towers(&mut self, now: i64) {
        let indices: Vec<i32> = self.conquests.iter().map(|c| c.info.index).collect();
        for index in indices {
            let Some(conquest) = self.conquest(index) else {
                continue;
            };
            if !conquest.war_is_on {
                continue;
            }
            let Some(map) = conquest.map.clone() else {
                continue;
            };
            let archers: Vec<(u32, Point, i32)> = conquest
                .objects
                .iter()
                .filter(|object| {
                    object.kind == ConquestObjectType::Archer
                        && !object.destroyed()
                        && object.next_shot <= now
                })
                .map(|object| (object.object_id, object.location, object.owner))
                .collect();
            for (object_id, location, owner) in archers {
                let target = self
                    .players
                    .iter()
                    .filter(|(_, player)| {
                        !player.dead
                            && player.map == map
                            && player.position.max_distance(location) <= 10
                            && player
                                .guild
                                .map(|guild| guild.guild_id)
                                .is_none_or(|guild| guild != owner)
                    })
                    .min_by_key(|(_, player)| player.position.max_distance(location))
                    .map(|(id, _)| *id);
                if let Some(target) = target {
                    self.pending_hits.push(crate::combat::PendingHit {
                        at: now + 500,
                        attacker_id: object_id,
                        target_id: target,
                        damage: 50,
                        defence: mir_core::enums::DefenceType::ACAgility,
                    });
                }
                if let Some(object) = self.conquest_mut(index).and_then(|conquest| {
                    conquest
                        .objects
                        .iter_mut()
                        .find(|object| object.object_id == object_id)
                }) {
                    object.next_shot = now + 2_000;
                }
            }
        }
    }

    fn conquest_info_packet(&self, index: i32) -> Option<ConquestInfoPacket> {
        let conquest = self.conquest(index)?;
        Some(ConquestInfoPacket {
            index,
            name: conquest.info.name.clone(),
            owner: conquest.owner,
            war_is_on: conquest.war_is_on,
            start_time: conquest.war_start_time,
            end_time: conquest.war_end_time,
            control_points: conquest
                .control_points
                .iter()
                .map(|point| ConquestControlPoint {
                    index: point.index,
                    owner: point.owner,
                    points: point.points.values().copied().sum(),
                })
                .collect(),
        })
    }

    fn conquest_object_packet(index: i32, object: &ConquestObjectState) -> ConquestObjectPacket {
        ConquestObjectPacket {
            object_id: object.object_id,
            conquest_index: index,
            kind: object.kind,
            location: object.location,
            direction: object.direction,
            hp: object.hp,
            max_hp: object.max_hp,
            owner: object.owner,
            destroyed: object.destroyed(),
        }
    }

    fn conquest_recipients(&self, index: i32) -> Vec<u32> {
        let Some(conquest) = self.conquest(index) else {
            return Vec::new();
        };
        self.players
            .iter()
            .filter(|(_, player)| conquest.owns_map(&player.map))
            .map(|(id, _)| *id)
            .collect()
    }

    /// 给一个玩家下发 `S.ConquestInfo` + 当前可见器械。
    pub fn send_conquest_state_to(&self, player_id: u32, index: i32) {
        let Some(info) = self.conquest_info_packet(index) else {
            return;
        };
        self.send_to_player(player_id, &ServerPacket::ConquestInfo(info).encode_frame());
        let Some(conquest) = self.conquest(index) else {
            return;
        };
        let Some(map) = conquest.map.clone() else {
            return;
        };
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        if player.map != map {
            return;
        }
        for object in &conquest.objects {
            let packet = Self::conquest_object_packet(index, object);
            self.send_to_player(
                player_id,
                &ServerPacket::ConquestObject(packet).encode_frame(),
            );
        }
    }

    fn broadcast_conquest_state(&self, index: i32) {
        for player_id in self.conquest_recipients(index) {
            self.send_conquest_state_to(player_id, index);
        }
    }

    fn broadcast_conquest_object(&self, index: i32, object_id: u32) {
        let Some(conquest) = self.conquest(index) else {
            return;
        };
        let Some(object) = conquest
            .objects
            .iter()
            .find(|object| object.object_id == object_id)
        else {
            return;
        };
        let Some(map) = conquest.map.clone() else {
            return;
        };
        let packet = ServerPacket::ConquestObject(Self::conquest_object_packet(index, object))
            .encode_frame();
        for (player_id, player) in &self.players {
            if player.map == map {
                self.send_to_player(*player_id, &packet);
            }
        }
    }

    fn broadcast_conquest_result(&self, index: i32, winner: i32, owner: i32) {
        let Some(conquest) = self.conquest(index) else {
            return;
        };
        let packet = ConquestResultPacket {
            index,
            winner,
            owner,
            control_points: conquest
                .control_points
                .iter()
                .map(|point| ConquestControlPoint {
                    index: point.index,
                    owner: point.owner,
                    points: point.points.values().copied().sum(),
                })
                .collect(),
        };
        let frame = ServerPacket::ConquestResult(packet).encode_frame();
        for player_id in self.conquest_recipients(index) {
            self.send_to_player(player_id, &frame);
        }
    }

    /// `callingNPC.Conq.GuildInfo.GoldStorage += (cost - baseCost)`（`:1211-1214`）
    /// 与 `PlayerObject.RepairItem` 的同名逻辑（`:8150`）。
    pub fn add_conquest_tax(&mut self, conquest_index: i32, amount: u32) {
        if amount == 0 {
            return;
        }
        if let Some(conquest) = self.conquest_mut(conquest_index) {
            conquest.gold_storage = conquest.gold_storage.saturating_add(amount);
            conquest.dirty = true;
        }
    }

    /// 某个 NPC 属于哪个攻城区（Crystal `NPCObject.Conq`，`NPCObject.cs:429`；
    /// 绑定见 `ConquestObject.LoadNPCs`，`:265-306`）。
    ///
    /// Crystal 靠 `NpcInfo.Conquest == ConquestInfo.Index` 认；本作同样用配表字段。
    #[must_use]
    pub fn npc_conquest(&self, npc: &Npc) -> Option<&Conquest> {
        if npc.conquest <= 0 {
            return None;
        }
        self.conquest(npc.conquest)
    }

    /// NPC 的价格倍率：Crystal `NPCScript.PriceRate(player, baseRate)`（`:111-133`）。
    ///
    /// ```csharp
    /// if (callingNPC == null) return 1F;
    /// if (callingNPC.Conq == null || baseRate) return callingNPC.Info.Rate / 100F;
    /// if (player.MyGuild != null && player.MyGuild.Guildindex == callingNPC.Conq.GuildInfo.Owner)
    ///     return callingNPC.Info.Rate / 100F;
    /// return (((Rate / 100F) * Conq.GuildInfo.NPCRate) + Rate) / 100F;
    /// ```
    ///
    /// * `base_rate = true` 时忽略城主加价（用来算「基础价」，差价进城主金库）；
    /// * 城主行会成员按基础价买东西。
    #[must_use]
    pub fn conquest_price_rate(&self, npc: &Npc, player_id: u32, base_rate: bool) -> f32 {
        let rate = f32::from(npc.rate) / 100.0;
        if base_rate {
            return rate;
        }
        let Some(conquest) = self.npc_conquest(npc) else {
            return rate;
        };
        let same_guild = self
            .players
            .get(&player_id)
            .and_then(|player| player.guild)
            .is_some_and(|reference| reference.guild_id == conquest.owner);
        if same_guild {
            return rate;
        }
        ((rate * f32::from(conquest.npc_rate)) + f32::from(npc.rate)) / 100.0
    }

    /// 每 10 秒一次的攻城处理：Crystal `Envir.Process` 的
    /// `conquestTime = Time + Settings.Second * 10`（`Envir.cs:2341`）。
    pub(crate) fn tick_conquests(&mut self, now: i64) {
        if self.conquests.is_empty() {
            return;
        }
        self.process_conquest_towers(now);
        if now < self.next_conquest_tick {
            return;
        }
        self.next_conquest_tick = now + CONQUEST_PROCESS_INTERVAL_MS;
        let wall = self.wall_now();
        self.process_conquests_at(wall);
    }

    /// 按给定的**墙上时刻**跑一轮攻城处理（`ConquestObject.Process`，`:508-512`）：
    ///
    /// ```csharp
    /// if (ScheduleTimer < Envir.Time) AutoSchedule();
    /// if (WarIsOn && (GameType == KingOfHill || GameType == Classic || GameType == ControlPoints))
    ///     ScorePoints();
    /// ```
    ///
    /// 测试与 GM 工具直接调用它，就能在任意时刻确定性地驱动时间表与计分。
    pub fn process_conquests_at(&mut self, now_wall: i64) -> ConquestTickReport {
        let mut report = ConquestTickReport::default();
        let indices: Vec<i32> = self.conquests.iter().map(|c| c.info.index).collect();
        for index in indices {
            let due = self
                .conquest(index)
                .is_some_and(|conquest| conquest.schedule_timer <= now_wall);
            if due {
                if let Some(conquest) = self.conquest_mut(index) {
                    conquest.schedule_timer = now_wall + CONQUEST_SCHEDULE_INTERVAL_MS;
                }
                match self.run_auto_schedule(index, now_wall) {
                    Some(WarEvent::Started) => report.started.push(index),
                    Some(WarEvent::Ended) => report.ended.push(index),
                    None => {}
                }
            }
            let scoring = self.conquest(index).is_some_and(|conquest| {
                conquest.war_is_on
                    && matches!(
                        conquest.game_type,
                        ConquestGame::KingOfHill
                            | ConquestGame::Classic
                            | ConquestGame::ControlPoints
                    )
            });
            if scoring && let Some(winner) = self.score_conquest_points(index) {
                report.captured.push((index, winner));
            }
        }
        report
    }

    /// 时间表：Crystal `ConquestObject.AutoSchedule`（`:465-506`）。
    ///
    /// ```csharp
    /// int start  = StartHour * 60;
    /// int finish = StartHour * 60 + WarLength;
    /// int now    = Envir.Now.Hour * 60 + Envir.Now.Minute;
    ///
    /// if (WarIsOn && StartType == Forced && WarEndTime <= Envir.Now) EndWar(Info.Game);
    ///
    /// if (StartType != Forced) {
    ///     if (WarIsOn && (now > start && finish <= now)) EndWar(Info.Game);
    ///     else if (start <= now && finish > now && CheckDay()) {
    ///         if (!WarIsOn) {
    ///             if (Info.Type == Request) { if (AttackerID != -1) { …; StartWar(Info.Game); } }
    ///             else { …; StartWar(Info.Game); }
    ///         }
    ///     }
    /// }
    /// ```
    fn run_auto_schedule(&mut self, index: i32, now_wall: i64) -> Option<WarEvent> {
        let conquest = self.conquest(index)?;
        // `WarEndTime <= Envir.Now`（`:471`）：只有 `Forced` 会赋值 `WarEndTime`。
        if conquest.war_is_on
            && conquest.start_type == ConquestType::Forced
            && conquest.war_end_time > 0
            && conquest.war_end_time <= now_wall
        {
            self.end_war(index);
            return Some(WarEvent::Ended);
        }
        if conquest.start_type == ConquestType::Forced {
            return None;
        }
        let start = conquest.start_minute();
        let finish = conquest.finish_minute();
        let now = minute_of_day(now_wall);
        if conquest.war_is_on && (now > start && finish <= now) {
            self.end_war(index);
            return Some(WarEvent::Ended);
        }
        if start <= now && finish > now && check_day(conquest, now_wall) && !conquest.war_is_on {
            let request = conquest.conquest_type() == ConquestType::Request;
            if !request || conquest.attacker >= 0 {
                self.start_war(index, now_wall);
                return Some(WarEvent::Started);
            }
        }
        None
    }

    /// 开战：Crystal `ConquestObject.StartWar`（`:907-913`）+ `AtWarChanged`（`:316-373`）。
    ///
    /// ```csharp
    /// WarIsOn = true;                    // → AtWarChanged()
    /// foreach (var pl in Envir.Players) pl.BroadcastInfo();
    /// ```
    ///
    /// `AtWarChanged` 在 `StartType == Forced` 时用 `Info.WarLength` 算起止时刻（`:320-325`）。
    ///
    /// 简化：Crystal 只 `BroadcastInfo()`（重发 `S.ObjectPlayer` 让客户端刷新名字颜色），
    /// **没有**聊天公告；只有 GM 的 `STARTCONQUEST`（`NPCSegment.cs:4261-4280`）
    /// 才 `ReceiveChat(WarStarted, ChatType.System)`。本作两条路径都发一条 `S.Chat`
    /// 系统公告，并刷新战区玩家的名字颜色（见 `docs/world/conquest.md`）。
    pub fn start_war(&mut self, index: i32, now_wall: i64) {
        let Some(conquest) = self.conquest_mut(index) else {
            return;
        };
        if conquest.war_is_on {
            return;
        }
        conquest.game_type = conquest.configured_game();
        // 简化：Crystal 在非 Forced 路径上**不写** `WarStartTime`（`:482-503` 只设
        // `GameType` / `StartType`）；本作补一个观测用的开战时刻，`WarEndTime` 仍按原样留 0。
        conquest.war_start_time = now_wall;
        if conquest.start_type == ConquestType::Forced {
            conquest.war_end_time = now_wall + i64::from(conquest.info.war_length) * MS_PER_MINUTE;
        } else {
            conquest.war_end_time = 0;
            conquest.next_war_at = now_wall;
        }
        conquest.war_is_on = true;
        conquest.dirty = true;
        let name = conquest.info.name.clone();
        // 战区玩家的名字颜色从「白」变「绿」（`PlayerObject.cs:1861-1870`）。
        self.refresh_war_zone_colours();
        self.broadcast_system_key(
            mir_core::i18n::TextKey::ConquestWarStarted,
            &[("name", &name)],
        );
        self.spawn_conquest_objects(index);
    }

    /// 停战：Crystal `ConquestObject.EndWar`（`:915-967`）+ `AtWarChanged` 的 else 分支
    /// （`:343-372`）。
    ///
    /// `ControlPoints` 玩法会在停战时按「各会控制点数量」选出新城主（`:921-962`）；
    /// 其余玩法的城主早已在 `ScorePoints` 里定好。停战后还要：
    ///
    /// * `ConquestType.Request` 时把 `AttackerID` 复位成 `-1`（`:353-358`）；
    /// * 清空王者之城/控制点的比分（Crystal 没清，但下一场开战带着旧分会立刻易主，
    ///   本作显式清空）。
    pub fn end_war(&mut self, index: i32) {
        let Some(conquest) = self.conquest_mut(index) else {
            return;
        };
        if !conquest.war_is_on {
            return;
        }
        let game = conquest.game_type;
        conquest.war_is_on = false;
        let name = conquest.info.name.clone();
        if game == ConquestGame::ControlPoints
            && let Some(winner) = self.control_point_winner(index)
        {
            self.take_conquest(index, winner);
        }
        if let Some(conquest) = self.conquest_mut(index) {
            if conquest.start_type == ConquestType::Request {
                conquest.attacker = -1;
            }
            // 简化：Crystal 停战后**不**复位 `StartType`，于是 GM 用 `STARTCONQUEST` 强制开过
            // 一次之后，`AutoSchedule` 的 `if (StartType != Forced)` 永远不成立、定时攻城
            // 就此失效（`ConquestObject.cs:476-503`）。本作在停战时复位成配表的类型。
            conquest.start_type = conquest.conquest_type();
            conquest.king_points.clear();
            for point in &mut conquest.control_points {
                point.points.clear();
            }
            conquest.dirty = true;
        }
        let wall = self.wall_now();
        if let Some(conquest) = self.conquest_mut(index) {
            conquest.next_war_at = conquest.next_window_start(wall);
        }
        self.refresh_war_zone_colours();
        self.broadcast_system_key(
            mir_core::i18n::TextKey::ConquestWarEnded,
            &[("name", &name)],
        );
        // 停战：器械先以 hp=0/destroyed 广播，再清空（简化：不保留耐久）。
        let object_ids: Vec<u32> = self
            .conquest(index)
            .map(|conquest| {
                conquest
                    .objects
                    .iter()
                    .map(|object| object.object_id)
                    .collect()
            })
            .unwrap_or_default();
        for object_id in object_ids {
            if let Some(conquest) = self.conquest_mut(index) {
                if let Some(object) = conquest
                    .objects
                    .iter_mut()
                    .find(|object| object.object_id == object_id)
                {
                    object.hp = 0;
                }
            }
            self.broadcast_conquest_object(index, object_id);
        }
        if let Some(conquest) = self.conquest_mut(index) {
            conquest.objects.clear();
        }
        let owner_after = self.conquest(index).map_or(-1, |conquest| conquest.owner);
        let winner = owner_after;
        self.broadcast_conquest_result(index, winner, owner_after);
        self.broadcast_conquest_state(index);
    }

    /// 计分：Crystal `ConquestObject.ScorePoints`（`:716-905`）。
    ///
    /// 返回本轮是否换了城主（`Some(新城主行会)`）。
    fn score_conquest_points(&mut self, index: i32) -> Option<i32> {
        let game = self.conquest(index)?.game_type;
        match game {
            ConquestGame::Classic => self.score_classic(index),
            ConquestGame::KingOfHill => self.score_king_of_hill(index),
            ConquestGame::ControlPoints => self.score_control_points(index),
            // `CapturePalace` 靠玩家走进皇宫触发（`CheckConquest(true)`，`PlayerObject.cs:4617`）；
            // `Random` 在 Crystal 里没有分支（`ScorePoints` 的 `default: return;`，`:902-904`）。
            ConquestGame::CapturePalace | ConquestGame::Random => None,
        }
    }

    /// `Classic`：皇宫里只剩一个行会的人时夺城（`ConquestObject.cs:873-900`）。
    ///
    /// ```csharp
    /// int guildCounter = 0; GuildObject takingGuild = null;
    /// for (每个皇宫里活着的玩家) {
    ///     if (MyGuild != null) { if (takingGuild == null || takingGuild != MyGuild) guildCounter++;
    ///                            takingGuild = MyGuild; }
    ///     else guildCounter++;
    /// }
    /// if (guildCounter == 1 && takingGuild != Guild) {
    ///     if (StartType == Request && takingGuild.Guildindex != GuildInfo.AttackerID) return;
    ///     TakeConquest(null, takingGuild);
    /// }
    /// ```
    ///
    /// `guildCounter` 数的是**行会切换次数**而不是人数：同一行会的多个人算 1，
    /// 行会不同（或被无行会玩家打断）就继续加。也就是说「皇宫里只剩一个行会且没有散人」
    /// 才算占领。
    ///
    /// 简化：Crystal 在 `takingGuild == null`（皇宫里唯一的人是散人）且
    /// `StartType == Request` 时会空引用崩掉（`:895`）；本作直接跳过。
    fn score_classic(&mut self, index: i32) -> Option<i32> {
        let (palace, owner) = {
            let conquest = self.conquest(index)?;
            (conquest.palace_map.clone()?, conquest.owner)
        };
        let (counter, taking) = {
            let mut ids: Vec<u32> = self
                .players
                .values()
                .filter(|player| player.map == palace && !player.dead)
                .map(|player| player.object_id)
                .collect();
            ids.sort_unstable();
            let mut counter = 0_i32;
            let mut taking: Option<i32> = None;
            for id in ids {
                let Some(player) = self.players.get(&id) else {
                    continue;
                };
                match player.guild.map(|reference| reference.guild_id) {
                    Some(guild) => {
                        if taking != Some(guild) {
                            counter += 1;
                        }
                        taking = Some(guild);
                    }
                    None => counter += 1,
                }
            }
            (counter, taking)
        };
        if counter != 1 {
            return None;
        }
        let winner = taking?;
        if winner == owner {
            return None;
        }
        self.take_conquest(index, winner).then_some(winner)
    }

    /// `KingOfHill`：王座范围内每 10 秒加 1 分（18 分满），分最高的行会立刻夺城
    /// （`ConquestObject.cs:801-871`）。
    fn score_king_of_hill(&mut self, index: i32) -> Option<i32> {
        let (map, king, size, attacker, request, owner, mut points) = {
            let conquest = self.conquest(index)?;
            (
                conquest.map.clone()?,
                conquest.info.king_location,
                i32::from(conquest.info.king_size),
                conquest.attacker,
                conquest.start_type == ConquestType::Request,
                conquest.owner,
                conquest.king_points.clone(),
            )
        };
        let mut player_ids: Vec<u32> = self
            .players
            .values()
            .filter(|player| player.map == map && !player.dead)
            .filter(|player| player.position.in_range(king, size))
            .map(|player| player.object_id)
            .collect();
        player_ids.sort_unstable();
        let mut changed = false;
        for player_id in player_ids {
            if !self.in_war_zone(player_id) {
                continue;
            }
            let Some(guild) = self
                .players
                .get(&player_id)
                .and_then(|player| player.guild)
                .map(|reference| reference.guild_id)
            else {
                continue;
            };
            if request && guild != attacker {
                continue;
            }
            if self.guild_conquest_other(guild, index) {
                continue;
            }
            let score = points.entry(guild).or_insert(0);
            if *score < MAX_KING_POINTS {
                *score += 1;
            }
            let others: Vec<i32> = points.keys().copied().collect();
            for other in others {
                if other == guild {
                    continue;
                }
                if let Some(score) = points.get_mut(&other) {
                    *score -= 1;
                }
            }
            changed = true;
        }
        if !changed {
            return None;
        }
        if let Some(conquest) = self.conquest_mut(index) {
            conquest.king_points.clone_from(&points);
            conquest.dirty = true;
        }
        // 找分最高的行会（Crystal 从当前城主起算，`:844-859`）。
        let mut best = (owner >= 0).then_some(owner);
        let mut best_score = best.and_then(|g| points.get(&g).copied()).unwrap_or(0);
        let mut guild_ids: Vec<i32> = self.guilds.keys().copied().collect();
        guild_ids.sort_unstable();
        for guild in guild_ids {
            let score = points.get(&guild).copied().unwrap_or(0);
            if score > best_score {
                best = Some(guild);
                best_score = score;
            }
        }
        let winner = best?;
        if winner == owner {
            return None;
        }
        self.take_conquest(index, winner).then_some(winner)
    }

    /// `ControlPoints`：控制点争夺（`ConquestObject.cs:722-800`）。
    ///
    /// 每个控制点各自维护「各会分数」：站在控制点 3 格内的活人给本会 +1（上限 6），
    /// 同时把其它会各 -1；分最高的会拿到这个控制点。停战时控制点最多的会当城主
    /// （[`Conquest::control_point_winner`]）。
    ///
    /// 本作只实现**计分与归属**；控制点的旗子 NPC 对象（`ConquestGuildFlagInfo.Spawn`，
    /// `ConquestGuildInfo.cs:218-243`）没做，见 `docs/world/conquest.md`。
    fn score_control_points(&mut self, index: i32) -> Option<i32> {
        let (map, request, attacker, mut points) = {
            let conquest = self.conquest(index)?;
            (
                conquest.map.clone()?,
                conquest.start_type == ConquestType::Request,
                conquest.attacker,
                conquest.control_points.clone(),
            )
        };
        let mut player_ids: Vec<u32> = self
            .players
            .values()
            .filter(|player| player.map == map && !player.dead)
            .map(|player| player.object_id)
            .collect();
        player_ids.sort_unstable();
        let mut changed_owners = Vec::new();
        for point in &mut points {
            let mut changed = false;
            for player_id in &player_ids {
                let Some(player) = self.players.get(player_id) else {
                    continue;
                };
                if !player
                    .position
                    .in_range(point.location, CONTROL_POINT_RANGE)
                {
                    continue;
                }
                let Some(guild) = player.guild.map(|reference| reference.guild_id) else {
                    continue;
                };
                if request && guild != attacker {
                    continue;
                }
                if !self.in_war_zone(*player_id) {
                    continue;
                }
                let score = point.points.entry(guild).or_insert(0);
                if *score < MAX_CONTROL_POINTS {
                    *score += 1;
                }
                let others: Vec<i32> = point.points.keys().copied().collect();
                for other in others {
                    if other == guild {
                        continue;
                    }
                    if let Some(score) = point.points.get_mut(&other)
                        && *score > 0
                    {
                        *score -= 1;
                    }
                }
                changed = true;
            }
            if !changed {
                continue;
            }
            let mut best = point.owner;
            let mut best_score = point.points.get(&point.owner).copied().unwrap_or(0);
            let mut guild_ids: Vec<i32> = self.guilds.keys().copied().collect();
            guild_ids.sort_unstable();
            for guild in guild_ids {
                let score = point.points.get(&guild).copied().unwrap_or(0);
                if score > best_score {
                    best = guild;
                    best_score = score;
                }
            }
            if best != point.owner {
                point.owner = best;
                changed_owners.push((point.name.clone(), best));
            }
        }
        if let Some(conquest) = self.conquest_mut(index) {
            conquest.control_points.clone_from(&points);
            conquest.dirty = true;
        }
        for (point_name, guild) in changed_owners {
            let guild_name = self.guild(guild).map(|guild| guild.name.clone());
            // 「（无行会）」也是本地化文本：占位时按收件人语言渲染。
            let mut ids: Vec<u32> = self.players.keys().copied().collect();
            ids.sort_unstable();
            for object_id in ids {
                let label = guild_name
                    .clone()
                    .unwrap_or_else(|| self.text(object_id, TextKey::ConquestNoGuild));
                let message = self.format(
                    object_id,
                    TextKey::ConquestPointCaptured,
                    &[("guild", &label), ("point", &point_name)],
                );
                self.send_system_chat(object_id, &message);
            }
        }
        None
    }

    /// 停战时按「谁控制点最多」选城主：Crystal `EndWar` 的 `ControlPoints` 分支
    /// （`ConquestObject.cs:921-962`）。
    fn control_point_winner(&self, index: i32) -> Option<i32> {
        let conquest = self.conquest(index)?;
        let mut counts: BTreeMap<i32, i32> = BTreeMap::new();
        for point in &conquest.control_points {
            if point.owner < 0 {
                continue;
            }
            *counts.entry(point.owner).or_insert(0) += 1;
        }
        let mut best = (conquest.owner >= 0).then_some(conquest.owner);
        let mut best_count = best.and_then(|g| counts.get(&g).copied()).unwrap_or(0);
        for (guild, count) in counts {
            if count > best_count {
                best = Some(guild);
                best_count = count;
            }
        }
        best
    }

    /// 某个行会是否已经拥有**别的**攻城区
    /// （Crystal `winningGuild.Conquest != null → return`，`ConquestObject.cs:554`）。
    fn guild_conquest_other(&self, guild_id: i32, index: i32) -> bool {
        self.conquests
            .iter()
            .any(|conquest| conquest.owner == guild_id && conquest.info.index != index)
    }

    /// 夺城：Crystal `ConquestObject.TakeConquest`（`:551-632`）。
    ///
    /// ```csharp
    /// if (winningGuild != null && winningGuild.Conquest != null) return;
    ///
    /// case CapturePalace / KingOfHill / Classic:
    ///     if (StartType == Request) if (winningGuild.Guildindex != GuildInfo.AttackerID) break;
    ///     if (Guild != null) { tmpPrevious = Guild; Guild.Conquest = null;
    ///                          GuildInfo.AttackerID = tmpPrevious.Guildindex; }
    ///     GuildInfo.Owner = winningGuild.Guildindex;
    ///     Guild = winningGuild; Guild.Conquest = this;
    /// ```
    ///
    /// `CapturePalace` 夺城后还会立刻 `EndWar(GameType)`（`:575`）。
    ///
    /// 返回是否真的换了城主（被校验拦下时返回 `false`，Crystal 是静默 `return`/`break`）。
    pub fn take_conquest(&mut self, index: i32, guild_id: i32) -> bool {
        if guild_id < 0 || self.guild(guild_id).is_none() {
            return false;
        }
        if self.guild_conquest_other(guild_id, index) {
            return false;
        }
        let Some((request, attacker, owner, game)) = self.conquest(index).map(|conquest| {
            (
                conquest.start_type == ConquestType::Request,
                conquest.attacker,
                conquest.owner,
                conquest.game_type,
            )
        }) else {
            return false;
        };
        // Crystal `break`：`Request` 时只有申请方有资格夺城（`:563`、`:580`、`:895`）。
        if request && attacker != guild_id {
            return false;
        }
        if owner == guild_id {
            return false;
        }
        if let Some(conquest) = self.conquest_mut(index) {
            conquest.owner = guild_id;
            // 上一任城主自动成为下一次的申请方（`:569`、`:586`）。
            if owner >= 0 {
                conquest.attacker = owner;
            }
            conquest.dirty = true;
        }
        let name = self
            .conquest(index)
            .map_or_else(String::new, |conquest| conquest.info.name.clone());
        let guild_name = self
            .guild(guild_id)
            .map_or_else(String::new, |guild| guild.name.clone());
        self.broadcast_system_key(
            TextKey::ConquestNewOwner,
            &[("conquest", &name), ("guild", &guild_name)],
        );
        // 城主的行会成员要刷新一次 `S.GuildStatus`（Crystal `UpdatePlayers`，`:634-650`）。
        self.send_guild_status_to_all(guild_id);
        self.refresh_war_zone_colours();
        if game == ConquestGame::CapturePalace {
            self.end_war(index);
        }
        true
    }

    /// 切图后检查「走进皇宫就算占领」：Crystal `PlayerObject.CheckConquest(true)`
    /// （`PlayerObject.cs:13949-13966`，由 `CompleteMapMovement` 在 `:4617` 调用）——
    /// 只有 `CapturePalace` 玩法在这一步夺城。
    pub(crate) fn check_conquest_after_map_change(&mut self, player_id: u32) {
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        let map = player.map.clone();
        // 进入攻城区相关地图时先下发配置 + 器械状态（本项目扩展包）。
        if let Some(index) = self
            .conquests
            .iter()
            .find(|conquest| conquest.owns_map(&map))
            .map(|conquest| conquest.info.index)
        {
            self.send_conquest_state_to(player_id, index);
        }
        let Some(guild) = player.guild.map(|reference| reference.guild_id) else {
            return;
        };
        let target = self.conquests.iter().find(|conquest| {
            conquest.war_is_on
                && conquest.game_type == ConquestGame::CapturePalace
                && conquest.palace_map.as_deref() == Some(map.as_str())
        });
        let Some(index) = target.map(|conquest| conquest.info.index) else {
            return;
        };
        self.take_conquest(index, guild);
    }

    /// 重新计算所有玩家的名字颜色（Crystal 开战/停战时的 `BroadcastInfo()`，
    /// `ConquestObject.cs:911-913`、`:965-966`）。
    fn refresh_war_zone_colours(&mut self) {
        let mut ids: Vec<u32> = self.players.keys().copied().collect();
        ids.sort_unstable();
        for object_id in ids {
            self.refresh_name_colour(object_id);
        }
    }

    /// 全服系统公告（Crystal 用 `pl.ReceiveChat(..., ChatType.System)` 遍历
    /// `Envir.Players`，`NPCSegment.cs:4271-4283`）。
    ///
    /// 多语言项目里**逐收件人按各自语言**渲染：先取 `TextKey` + 参数，再给每个
    /// 在线玩家 `format` 一次（同一语言只渲染一次的开销可以忽略；公告频率很低）。
    pub fn broadcast_system_key(&self, key: mir_core::i18n::TextKey, args: &[(&str, &str)]) {
        let mut ids: Vec<u32> = self.players.keys().copied().collect();
        ids.sort_unstable();
        for object_id in ids {
            let message = self.format(object_id, key, args);
            self.send_system_chat(object_id, &message);
        }
    }

    /// 全服系统公告（已渲染文本；内部/兼容用，玩家可见公告请走
    /// [`Self::broadcast_system_key`]）。
    pub fn broadcast_system(&self, message: &str) {
        let packet = ServerPacket::Chat(Chat {
            message: message.to_owned(),
            type_: ChatType::System,
        })
        .encode_frame();
        for player in self.players.values() {
            let _ = player.sender.send(packet.clone());
        }
    }

    /// 给行会在线成员发 `S.GuildStorageGoldChange`
    /// （Crystal `GuildObject.SendServerPacket`，`GuildObject.cs:602-615`）。
    fn send_guild_gold_change(&self, guild_id: i32, type_: u8, amount: u32, name: &str) {
        let packet = ServerPacket::GuildStorageGoldChange(
            mir_protocol::packets::server::GuildStorageGoldChange {
                type_,
                amount,
                name: name.to_owned(),
            },
        )
        .encode_frame();
        self.send_to_guild(guild_id, &packet);
    }
}

/// 时间表产生的事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WarEvent {
    /// 本轮开战。
    Started,
    /// 本轮停战。
    Ended,
}

/// 把世界里积压的攻城写入落到存储（Crystal `Envir.SaveConquests`，`Envir.cs:2734-2749`）。
///
/// # Errors
/// 任意一条写库失败时返回第一个错误（剩余记录已经丢掉了，与
/// [`crate::guild::persist_guild_writes`] 的口径一致：调用方只记日志）。
pub async fn persist_conquest_writes<S: crate::store::AccountStore>(
    store: &S,
    writes: Vec<ConquestRecord>,
) -> Result<usize, crate::store::StoreError> {
    let mut saved = 0;
    for record in writes {
        store.save_conquest(record).await?;
        saved += 1;
    }
    Ok(saved)
}
