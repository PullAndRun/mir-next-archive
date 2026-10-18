//! 行会（guild / 行会）。
//!
//! Crystal 的行会分成两层：
//!
//! * `GuildInfo`（`Server/MirDatabase/GuildInfo.cs:7-38`）：纯数据，序列化到
//!   `Guilds/<GuildIndex>.mgd`，字段有等级/经验/资金/升级点/职位/成员/仓库/buff/公告/投票/攻城租期。
//! * `GuildObject`（`Server/MirObjects/GuildObject.cs:7-1029`）：运行期包装，负责发包、
//!   在线状态、经验与等级、聊天、攻城。
//!
//! 本作把两者合并成 [`Guild`]，放进 `World::guilds`；`PlayerObject.MyGuild` +
//! `MyGuildRank` 合成 [`crate::world::Player::guild`]（[`GuildRef`]：行会 ID + 职位序号）。
//!
//! | Crystal | 本作 |
//! |---|---|
//! | `Envir.Guilds`（`List<GuildObject>`，`Envir.cs:147`） | `World::guilds`：`HashMap<行会 ID, Guild>` |
//! | `GuildObject.Ranks`（`List<GuildRank>`，成员嵌在职位里） | [`Guild::ranks`]（只管职位）+ [`Guild::members`]（角色 ID → 成员），**单一事实来源** |
//! | `PlayerObject.PendingGuildInvite`（`PlayerObject.cs:143`） | `World::guild_invites`：对象 ID → 行会 ID |
//! | `GuildInfo.NeedSave` | 每次变更都往 `World::guild_writes` 里塞一条「待落盘」 |
//!
//! # 包序
//!
//! 每个流程都照抄 Crystal 的 `Enqueue` 顺序（行号见各方法文档）。用到的包：
//!
//! | 语义 | 客户端包 | 服务端包 |
//! |---|---|---|
//! | 创建（NPC 要名字） | — | `S.GuildNameRequest`（171，`NPCScript.cs:1058`） |
//! | 创建（拿到名字） | `C.GuildNameReturn{Name}`（82） | `S.GuildMemberChange{Status=2}` + `S.GuildStatus`（`PlayerObject.cs:9829-9835`） |
//! | 邀请 | `C.EditGuildMember{ChangeType=0,Name}`（79） | `S.GuildInvite{Name=行会名}`（169，`PlayerObject.cs:9886`） |
//! | 接受/拒绝 | `C.GuildInvite{AcceptInvite}`（81） | `S.GuildMemberChange{Status=2}` + `S.GuildStatus`（`:9995-10003`） |
//! | 踢人/退会 | `C.EditGuildMember{ChangeType=1,Name}` | `S.GuildMemberChange{Status=3/4}` + 被踢者 `S.GuildStatus`（空）（`GuildObject.cs:539-564`） |
//! | 升/降职 | `C.EditGuildMember{ChangeType=2,RankIndex,Name}` | `S.GuildMemberChange{Status=8}`（本人）+ `{Status=5,RankIndex}`（其他成员）（`GuildObject.cs:315-330`） |
//! | 改职位名 | `C.EditGuildMember{ChangeType=3,RankIndex,RankName}` | `S.GuildMemberChange{Status=7,Ranks}`（`GuildObject.cs:407-427`） |
//! | 新职位 | `C.EditGuildMember{ChangeType=4}` | `S.GuildMemberChange{Status=6,Ranks}`（`:345-349`） |
//! | 职位权限 | `C.EditGuildMember{ChangeType=5,RankIndex,Name=bool,RankName=option}` | `S.GuildMemberChange{Status=7,Ranks}`（`:370-376`） |
//! | 公告 | `C.EditGuildNotice{Notice}`（80） | `S.GuildNoticeChange{update=-1}`（166，`GuildObject.cs:599`） |
//! | 拉取公告/成员 | `C.RequestGuildInfo{Type}`（83） | `S.GuildNoticeChange{Notice}` / `S.GuildMemberChange{Status=255}`（`PlayerObject.cs:10012-10028`） |
//! | 行会聊天 | `C.Chat{"!~…"}` | `S.Chat{ChatType.Guild}`（`:2014-2024`） |
//! | 行会经验 | 成员 `GainExp` | `S.GuildExpGain{Amount}`（170，`GuildObject.cs:701`）/ 升级时 `S.GuildStatus`（`:685-694`） |
//!
//! # 未做（见 `docs/world/guild.md`）
//!
//! 攻城（`C.GuildRequestWar` / `C.PurchaseGuildTerritory` / `C.GuildTerritoryPage`）、
//! 行会仓库（物品 + 金币，`C.GuildStorageGoldChange` / `C.GuildStorageItemChange`）、
//! 行会 buff（`C.GuildBuffUpdate`）、行会联盟/敌对、行会投票（`GuildInfo.Votes`）。
//! 这些包收到后只记日志，不改变任何状态。

use std::collections::HashMap;

use mir_core::DateTimeBinary;
use mir_core::enums::{ChatType, GuildRankOptions};
use mir_protocol::ServerPacket;
use mir_protocol::data::guild_rank::{GuildMember, GuildRank};
use mir_protocol::packets::server::{
    Chat, GuildExpGain, GuildInvite as GuildInvitePacket, GuildMemberChange, GuildNameRequest,
    GuildNoticeChange, GuildStatus,
};

use crate::store::{GuildMemberRecord, GuildRankRecord, GuildRecord};
use crate::world::World;

// ---------------------------------------------------------------------------
// 常量（全部来自 Crystal 源码）
// ---------------------------------------------------------------------------

/// 行会名长度下限：`if ((Name.Length < 3) || (Name.Length > 20))`（`PlayerObject.cs:10033`）。
pub const GUILD_NAME_MIN_LEN: usize = 3;

/// 行会名长度上限（同上）。
pub const GUILD_NAME_MAX_LEN: usize = 20;

/// 职位名长度下限：`if ((RankName == "") || (RankName.Length < 3))`（`PlayerObject.cs:9917`）。
pub const GUILD_RANK_NAME_MIN_LEN: usize = 3;

/// 职位名长度上限：`if (RankName.Contains("\\") || RankName.Length > 20)`（`PlayerObject.cs:9922`）。
pub const GUILD_RANK_NAME_MAX_LEN: usize = 20;

/// 公告行数上限：`if (notice.Count > 200)`（`PlayerObject.cs:9971`）。
pub const GUILD_NOTICE_MAX_LINES: usize = 200;

/// 职位数量上限：`if (Ranks.Count >= byte.MaxValue)`（`GuildObject.cs:336`）。
pub const GUILD_MAX_RANKS: usize = 255;

/// 会长职位的序号：`Index = 0`（`GuildInfo.cs:54`）。
pub const GUILD_LEADER_RANK_INDEX: u8 = 0;

/// 会长职位的权限位：`Options = (GuildRankOptions)255`（`GuildInfo.cs:54`）。
pub const GUILD_LEADER_OPTIONS: u8 = 255;

/// 会长职位名：`new GuildRank { Name = "Leader", … }`（`GuildInfo.cs:54`）。
pub const GUILD_LEADER_RANK_NAME: &str = "Leader";

/// 加入行会时自动补出的最低职位名（`GuildObject.cs:226`）。
pub const GUILD_DEFAULT_RANK_NAME: &str = "Members";

/// 新职位的名字（`ServerTextKeys.RankNum` = `"Rank-{0}"`，`Shared/Language.cs:2483`）。
pub const GUILD_NEW_RANK_NAME: &str = "Rank-";

/// 行会仓库格数：`GuildStorageItem[] StoredItems = new GuildStorageItem[112]`（`GuildInfo.cs:22`）。
///
/// 仓库本身没实现（见模块文档），这个值仍然要填进 `S.GuildStatus.ItemCount`。
pub const GUILD_STORAGE_SLOTS: usize = 112;

/// 两次 `S.GuildExpGain` 之间的最短间隔：`NextExpUpdate = Envir.Time + 10000`
/// （`GuildObject.cs:683`、`:700`）。
pub const GUILD_EXP_UPDATE_MS: i64 = 10_000;

/// 默认的行会经验倍率：`Guild_ExpRate = 0.01f`（`Server/Settings.cs:328`）。
pub const GUILD_EXP_RATE_DEFAULT: f64 = 0.01;

/// 默认的创建行会所需等级：`Guild_RequiredLevel = 22`（`Server/Settings.cs:327`）。
pub const GUILD_REQUIRED_LEVEL_DEFAULT: u16 = 22;

/// 默认的创建行会所需金币：`Guild_CreationCostList.Add(new GuildItemVolume(){Amount = 1000000})`
/// （`Server/Settings.cs:1335`）。
pub const GUILD_CREATION_GOLD_DEFAULT: u32 = 1_000_000;

/// 示例数据里 `[Exp]`/`[Cap]` 的级数（Crystal `Settings.LoadGuildSettings` 固定读 255 项，
/// `Server/Settings.cs:1358-1369`）。
pub const GUILD_LEVEL_COUNT: usize = 255;

/// 行会名里不允许出现的字符：Crystal 只拒绝反斜杠（`PlayerObject.cs:10039`、`:9922`）。
///
/// 本作额外拒绝换行/制表等控制字符（**本作扩展**）：它们会被客户端当成两行或破坏
/// `S.GuildMemberChange` 的显示，而 Crystal 因为 `string` 长度前缀的写法不会崩。
pub const GUILD_NAME_FORBIDDEN: [char; 2] = ['\\', '\n'];

/// 行会相关设置（Crystal `Settings.Guild_*` + `Configs/GuildSettings.ini`）。
///
/// 对应 Crystal：
///
/// ```ini
/// [Guilds]
/// MinimumLevel=22      ; Guild_RequiredLevel，Settings.cs:1340
/// ExpRate=0.01         ; Guild_ExpRate，:1341
/// PointPerLevel=1      ; Guild_PointPerLevel，:1342
/// [Exp] Level-0=45000  ; Guild_ExperienceList，:1362
/// [Cap] Level-0=5      ; Guild_MembercapList，:1368
/// [Required-0] Amount=1000000 ; Guild_CreationCostList，:1351
/// ```
///
/// 代码里的默认值与 `Server/Settings.cs:327-334` 一致：等级 22、倍率 0.01、
/// 升级点 0；**经验表与人数表默认为全 0**（Crystal 代码里两者都是空列表，
/// 只有读到 `GuildSettings.ini` 才有值——没有配置文件时行会既不升级也不限人数）。
/// 生产路径用 [`GuildSettings::discover`] 装入素材里的真实配置。
#[derive(Debug, Clone, PartialEq)]
pub struct GuildSettings {
    /// 创建行会的最低等级（`Guild_RequiredLevel`）。
    pub required_level: u16,
    /// 创建行会需要的金币（`Guild_CreationCostList` 里 `ItemName` 为空的那一项）。
    ///
    /// **简化**：Crystal 还能要求物品（默认 1 个 `WoomaHorn`，`Settings.cs:1336`）；
    /// 本作只扣金币，物品要求见 `docs/world/guild.md` 的「未做」。
    pub creation_gold: u32,
    /// 行会经验倍率（`Guild_ExpRate`）：成员获得 `amount` 经验时行会得到
    /// `(uint)(amount * ExpRate)`（`GuildObject.cs:649`）。
    pub exp_rate: f64,
    /// 每升一级给的升级点（`Guild_PointPerLevel`，`GuildObject.cs:660`）。
    pub point_per_level: u8,
    /// 每级升级所需经验（`Guild_ExperienceList`，`GuildObject.cs:663-666`）。
    ///
    /// 下标 = 当前等级；`<= 0` 表示「不能再升」。
    pub experience_list: Vec<i64>,
    /// 每级的人数上限（`Guild_MembercapList`，`GuildObject.cs:678-681`）。
    ///
    /// `0` 表示不限（`HasRoom` 只在非 0 时比较，`GuildObject.cs:632-643`）。
    pub member_cap_list: Vec<i32>,
}

impl Default for GuildSettings {
    fn default() -> Self {
        Self {
            required_level: GUILD_REQUIRED_LEVEL_DEFAULT,
            creation_gold: GUILD_CREATION_GOLD_DEFAULT,
            exp_rate: GUILD_EXP_RATE_DEFAULT,
            // Crystal `Guild_PointPerLevel = 0`（`Settings.cs:327`）。
            point_per_level: 0,
            // Crystal 代码默认是空列表；这里用 255 个 0 保持 `Level < Count` 的判定形状
            // （`GuildInfo.cs:63`、`:159`），实际效果同样是「不升级」。
            experience_list: vec![0; GUILD_LEVEL_COUNT],
            member_cap_list: vec![0; GUILD_LEVEL_COUNT],
        }
    }
}

impl GuildSettings {
    /// 从 `GuildSettings.ini` 的文本解析。
    ///
    /// 只认本作需要的几项（`[Guilds]` 的 `MinimumLevel`/`ExpRate`/`PointPerLevel`、
    /// `[Exp]`/`[Cap]` 的 `Level-N`、`[Required-*]` 里 `ItemName` 为空的金币项）；
    /// 缺失项沿用 [`GuildSettings::default`]。
    #[must_use]
    pub fn parse_ini(text: &str) -> Self {
        let mut settings = Self::default();
        // 先把两张表清空：文件里有 [Exp] 就以文件为准（Crystal 每次都是整表重建）。
        let mut exp: HashMap<usize, i64> = HashMap::new();
        let mut cap: HashMap<usize, i32> = HashMap::new();
        let mut section = String::new();
        let mut creation_gold: Option<u32> = None;
        let mut current_required_item = false;

        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }
            if let Some(name) = line
                .strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
            {
                name.trim().clone_into(&mut section);
                // `[Required-N]` 段里 `ItemName=` 为空才表示「要金币」。
                current_required_item = section.to_ascii_lowercase().starts_with("required-");
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();
            let lower_section = section.to_ascii_lowercase();
            match lower_section.as_str() {
                "guilds" => match key.to_ascii_lowercase().as_str() {
                    "minimumlevel" => {
                        if let Ok(level) = value.parse::<u16>() {
                            settings.required_level = level;
                        }
                    }
                    "exprate" => {
                        if let Ok(rate) = value.parse::<f64>() {
                            settings.exp_rate = rate;
                        }
                    }
                    "pointperlevel" => {
                        if let Ok(points) = value.parse::<u8>() {
                            settings.point_per_level = points;
                        }
                    }
                    _ => {}
                },
                "exp" => {
                    if let Some(index) = ini_level_index(key) {
                        if let Ok(amount) = value.parse::<i64>() {
                            exp.insert(index, amount);
                        }
                    }
                }
                "cap" => {
                    if let Some(index) = ini_level_index(key) {
                        if let Ok(amount) = value.parse::<i32>() {
                            cap.insert(index, amount);
                        }
                    }
                }
                _ => {
                    if current_required_item {
                        if key.eq_ignore_ascii_case("itemname") && value.is_empty() {
                            // 这一项是「金币」；金额在同一个段的 Amount 里。
                            creation_gold = Some(0);
                        } else if key.eq_ignore_ascii_case("amount") && creation_gold == Some(0) {
                            if let Ok(amount) = value.parse::<u32>() {
                                creation_gold = Some(amount);
                            }
                        }
                    }
                }
            }
        }

        if !exp.is_empty() {
            settings.experience_list = (0..GUILD_LEVEL_COUNT)
                .map(|index| exp.get(&index).copied().unwrap_or(0))
                .collect();
        }
        if !cap.is_empty() {
            settings.member_cap_list = (0..GUILD_LEVEL_COUNT)
                .map(|index| cap.get(&index).copied().unwrap_or(0))
                .collect();
        }
        if let Some(gold) = creation_gold.filter(|gold| *gold > 0) {
            settings.creation_gold = gold;
        }

        settings
    }

    /// 读一个 `GuildSettings.ini`；文件读不出来返回 `None`。
    #[must_use]
    pub fn load(path: impl AsRef<std::path::Path>) -> Option<Self> {
        let text = std::fs::read_to_string(path).ok()?;
        Some(Self::parse_ini(&text))
    }

    /// 按候选路径依次尝试，全部失败时用 [`GuildSettings::default`] 并打一条 `warn!`。
    ///
    /// 候选顺序：本作自己的 `config/GuildSettings.ini`，然后是素材索引里社区数据库自带的
    /// `Configs/GuildSettings.ini`（与 [`crate::exp::EXP_CANDIDATES`] 同一批目录）。
    #[must_use]
    pub fn discover() -> Self {
        for candidate in GUILD_SETTINGS_CANDIDATES {
            if !std::path::Path::new(candidate).is_file() {
                continue;
            }
            if let Some(settings) = Self::load(candidate) {
                tracing::info!(
                    path = candidate,
                    required_level = settings.required_level,
                    creation_gold = settings.creation_gold,
                    "已加载行会配置"
                );
                return settings;
            }
            tracing::warn!(path = candidate, "行会配置读取失败，尝试下一个候选");
        }
        tracing::warn!("没有可用的 GuildSettings.ini，行会使用代码默认值（不升级、不限人数）");
        Self::default()
    }

    /// `level` 级升到下一级所需行会经验；表里没有该等级时返回 0（= 不能再升，
    /// `GuildInfo.cs:63-66` 的三元语义）。
    #[must_use]
    pub fn max_experience(&self, level: u8) -> i64 {
        self.experience_list
            .get(usize::from(level))
            .copied()
            .unwrap_or(0)
    }

    /// `level` 级的人数上限；表里没有该等级时返回 0（= 不限，`GuildObject.cs:632-643`）。
    #[must_use]
    pub fn member_cap(&self, level: u8) -> i32 {
        self.member_cap_list
            .get(usize::from(level))
            .copied()
            .unwrap_or(0)
    }
}

/// `GuildSettings.ini` 的候选路径（按优先级）。
pub const GUILD_SETTINGS_CANDIDATES: &[&str] = &[
    "config/GuildSettings.ini",
    "config/Guildsettings.ini",
    "resource/community/suprcode-crystal-database/cjlaaa/Configs/GuildSettings.ini",
    "resource/community/suprcode-crystal-database/Jev/Configs/GuildSettings.ini",
    "resource/community/cjlaaa-mir2-server-release/Configs/GuildSettings.ini",
];

/// 解析 `Level-<n>` / `Level<n>` 里的等级号。
fn ini_level_index(key: &str) -> Option<usize> {
    let digits = key
        .strip_prefix("Level-")
        .or_else(|| key.strip_prefix("level-"))
        .or_else(|| key.strip_prefix("Level"))
        .or_else(|| key.strip_prefix("level"))?;
    digits.trim().parse::<usize>().ok()
}

// ---------------------------------------------------------------------------
// 状态
// ---------------------------------------------------------------------------

/// 玩家所属行会的引用（Crystal `PlayerObject.MyGuild` + `MyGuildRank`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuildRef {
    /// 行会 ID（Crystal `MyGuild.Guildindex`）。
    pub guild_id: i32,
    /// 自己的职位序号（Crystal `MyGuildRank.Index`）。
    pub rank_index: u8,
}

/// 行会里的一名成员（Crystal `GuildMember`，`Shared/Data/GuildData.cs:62-95`）。
///
/// Crystal 把 `Player`（在线对象引用）挂在成员上；本作存对象 ID
/// （`online: Option<u32>`），因为对象 ID 在每个世界实例内唯一、并且玩家下线后
/// 对象会被销毁（Crystal 里 `Player = null`，`GuildObject.PlayerLogged`，`:177-181`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuildMemberState {
    /// 角色 ID（`GuildMember.Id`，Crystal 是 `int`）。
    pub character_id: i64,
    /// 角色名。
    pub name: String,
    /// 职位序号。
    pub rank_index: u8,
    /// 等级（Crystal 提升会长时查 `Character.Level`，`GuildObject.cs:258`；
    /// 本作不读库，改为缓存登录时的等级）。
    pub level: u16,
    /// 入会时间（Unix 毫秒；**本作扩展**，只用于稳定成员列表顺序）。
    pub joined_at: i64,
    /// 最后登录时间（Crystal `GuildMember.LastLogin`，下线时写，`:177`）。
    pub last_login: DateTimeBinary,
    /// 在线对象 ID；`None` = 离线。
    pub online: Option<u32>,
}

/// 一个行会（Crystal `GuildInfo` + `GuildObject` 的合并视图）。
///
/// 只派生 `PartialEq`（不派生 `Eq`）：`ranks` 里是协议层的 [`GuildRank`]，
/// 它为了逐字节保真只实现了 `PartialEq`。
#[derive(Debug, Clone, PartialEq)]
pub struct Guild {
    /// 行会 ID（Crystal `GuildIndex`）。
    pub id: i32,
    /// 行会名。
    pub name: String,
    /// 会长角色 ID（Crystal 的 rank 0 成员；`GuildObject` 没有这个字段，本作加上便于对账）。
    pub leader: i64,
    /// 等级（`GuildInfo.Level`）。
    pub level: u8,
    /// 当前等级内已积累的经验（`GuildInfo.Experience`）。
    pub experience: i64,
    /// 资金（`GuildInfo.Gold`）。
    pub gold: u32,
    /// 升级点（`GuildInfo.SparePoints`）。
    pub spare_points: u8,
    /// 公告行（`GuildInfo.Notice`）。
    pub notice: Vec<String>,
    /// 职位（按序号升序；`index == 0` 是会长职位）。`members` 字段恒为空，
    /// 真正成员在 [`Guild::members`]，发包时由 [`Guild::rank_snapshot`] 拼装。
    pub ranks: Vec<GuildRank>,
    /// 成员（角色 ID → 成员）。
    pub members: HashMap<i64, GuildMemberState>,
    /// 升到下一级所需经验（`GuildInfo.MaxExperience`，由 [`Guild::refresh_limits`] 维护）。
    pub max_experience: i64,
    /// 人数上限（`GuildInfo.MemberCap`；0 = 不限）。
    pub member_cap: i32,
    /// 下一次允许发 `S.GuildExpGain` 的时刻（`GuildObject.NextExpUpdate`，`:58`）。
    pub next_exp_update: i64,
}

impl Guild {
    /// 建一个只有会长的行会（Crystal `new GuildInfo(owner, name)`，`GuildInfo.cs:50-79`）。
    ///
    /// 只建一个 `Leader` 职位（`Options = 255`）；`Members` 职位等第一个人加入时再补
    /// （`GuildObject.NewMember`，`GuildObject.cs:224-227`）。
    #[must_use]
    pub fn new(
        id: i32,
        name: &str,
        leader_character_id: i64,
        leader_name: &str,
        leader_level: u16,
        joined_at: i64,
        settings: &GuildSettings,
    ) -> Self {
        let mut members = HashMap::new();
        members.insert(
            leader_character_id,
            GuildMemberState {
                character_id: leader_character_id,
                name: leader_name.to_owned(),
                rank_index: GUILD_LEADER_RANK_INDEX,
                level: leader_level,
                joined_at,
                last_login: DateTimeBinary(0),
                online: None,
            },
        );
        let mut guild = Self {
            id,
            name: name.to_owned(),
            leader: leader_character_id,
            level: 0,
            experience: 0,
            gold: 0,
            spare_points: 0,
            notice: Vec::new(),
            ranks: vec![GuildRank {
                name: GUILD_LEADER_RANK_NAME.to_owned(),
                options: GUILD_LEADER_OPTIONS,
                index: i32::from(GUILD_LEADER_RANK_INDEX),
                members: Vec::new(),
            }],
            members,
            max_experience: 0,
            member_cap: 0,
            next_exp_update: 0,
        };
        guild.refresh_limits(settings);
        guild
    }

    /// 按当前设置刷新 `MaxExperience` / `MemberCap`（Crystal `GuildInfo.cs:63-76`、`:159-171`）。
    ///
    /// `NewbieGuild` 的特殊处理（等级拉满 21、人数上限用 `NewbieGuildMaxSize`）没做，
    /// 见 `docs/world/guild.md` 的「未做」。
    pub fn refresh_limits(&mut self, settings: &GuildSettings) {
        self.max_experience = settings.max_experience(self.level);
        self.member_cap = settings.member_cap(self.level);
    }

    /// 某个职位。
    #[must_use]
    pub fn rank(&self, index: u8) -> Option<&GuildRank> {
        self.ranks.get(usize::from(index))
    }

    /// 是否有这个职位序号（`index < Ranks.Count`，`GuildObject.cs:241`）。
    #[must_use]
    pub fn has_rank(&self, index: u8) -> bool {
        usize::from(index) < self.ranks.len()
    }

    /// 按角色名找成员（Crystal 里职位名/成员名都是精确比较，`GuildObject.cs:246`）。
    ///
    /// 大小写：Crystal 用 `==`（序数比较）。本作先精确匹配，找不到再忽略 ASCII 大小写
    /// （**本作扩展**，避免客户端输入 `bob` 找不到 `Bob`）。
    #[must_use]
    pub fn member_by_name(&self, name: &str) -> Option<&GuildMemberState> {
        self.members
            .values()
            .find(|member| member.name == name)
            .or_else(|| {
                self.members
                    .values()
                    .find(|member| member.name.eq_ignore_ascii_case(name))
            })
    }

    /// 成员列表（按入会时间、角色 ID 稳定排序）。
    fn sorted_members(&self) -> Vec<&GuildMemberState> {
        let mut members: Vec<&GuildMemberState> = self.members.values().collect();
        members.sort_by_key(|member| (member.joined_at, member.character_id));
        members
    }

    /// 拼一份职位的线上快照（`GuildRank` + 该职位的成员）。
    ///
    /// Crystal 的 `GuildRank.Members` 就是数据本体；本作的成员在 [`Guild::members`]
    /// 里，这里按 `rank_index` 过滤后填进快照。
    #[must_use]
    pub fn rank_snapshot(&self, index: u8) -> Option<GuildRank> {
        let rank = self.rank(index)?;
        let mut snapshot = rank.clone();
        snapshot.index = i32::from(index);
        snapshot.members = self
            .sorted_members()
            .into_iter()
            .filter(|member| member.rank_index == index)
            .map(|member| GuildMember {
                name: member.name.clone(),
                id: i32::try_from(member.character_id).unwrap_or(0),
                last_login: member.last_login,
                has_voted: false,
                online: member.online.is_some(),
            })
            .collect();
        Some(snapshot)
    }

    /// 全部职位的线上快照（`S.GuildMemberChange{Status=255}` 用，`PlayerObject.cs:10025`）。
    #[must_use]
    pub fn ranks_snapshot(&self) -> Vec<GuildRank> {
        (0..self.ranks.len())
            .filter_map(|index| self.rank_snapshot(u8::try_from(index).ok()?))
            .collect()
    }

    /// 会长职位上的第一个成员的角色名（Crystal `Ranks[0].Members[0].Name`，`GuildObject.cs:307`）。
    #[must_use]
    pub fn leader_name(&self) -> String {
        self.rank_snapshot(GUILD_LEADER_RANK_INDEX)
            .and_then(|rank| rank.members.first().map(|member| member.name.clone()))
            .unwrap_or_default()
    }

    /// 在线成员的对象 ID（Crystal `GetOnlinePlayers`，`GuildObject.cs:143-160`）。
    #[must_use]
    pub fn online_members(&self) -> Vec<u32> {
        self.sorted_members()
            .into_iter()
            .filter_map(|member| member.online)
            .collect()
    }

    /// 人数是否已满（Crystal `HasRoom`，`GuildObject.cs:632-643`）。
    ///
    /// `MemberCap == 0` 表示不限。
    #[must_use]
    pub fn has_room(&self) -> bool {
        self.member_cap == 0
            || i32::try_from(self.members.len()).unwrap_or(i32::MAX) < self.member_cap
    }

    /// 一份 `S.GuildStatus`（Crystal `GuildObject.SendGuildStatus`，`:194-220`）。
    ///
    /// `rank_index` 是收件人的职位；`None` 表示「已经不是成员了」，此时
    /// 职位名留空、`MyRankId = 256`（Crystal 用 `MyGuildRank != null ? Index : 256`）。
    #[must_use]
    pub fn status(&self, rank_index: Option<u8>) -> GuildStatus {
        let rank = rank_index.and_then(|index| self.rank(index));
        GuildStatus {
            guild_name: self.name.clone(),
            guild_rank_name: rank.map_or_else(String::new, |rank| rank.name.clone()),
            level: self.level,
            experience: self.experience,
            max_experience: self.max_experience,
            gold: self.gold,
            spare_points: self.spare_points,
            member_count: i32::try_from(self.members.len()).unwrap_or(i32::MAX),
            max_members: self.member_cap,
            // 投票系统没做（`GuildInfo.Voting`，`:18`）。
            voting: false,
            item_count: u8::try_from(GUILD_STORAGE_SLOTS).unwrap_or(u8::MAX),
            // buff 没做（`GuildInfo.BuffList`，`:23`）。
            buff_count: 0,
            my_options: GuildRankOptions(rank.map_or(0, |rank| rank.options)),
            my_rank_id: rank_index.map_or(256, i32::from),
        }
    }

    /// 转成存储视图。
    #[must_use]
    pub fn to_record(&self) -> GuildRecord {
        GuildRecord {
            id: self.id,
            name: self.name.clone(),
            leader_character_id: self.leader,
            level: i16::from(self.level),
            experience: self.experience,
            gold: i64::from(self.gold),
            spare_points: i16::from(self.spare_points),
            notice: self.notice.clone(),
            ranks: self
                .ranks
                .iter()
                .map(|rank| GuildRankRecord {
                    index: u8::try_from(rank.index).unwrap_or(0),
                    name: rank.name.clone(),
                    options: rank.options,
                })
                .collect(),
            members: self
                .sorted_members()
                .into_iter()
                .map(|member| GuildMemberRecord {
                    character_id: member.character_id,
                    name: member.name.clone(),
                    rank_index: member.rank_index,
                    joined_at: member.joined_at,
                })
                .collect(),
        }
    }

    /// 从存储视图还原（启动时灌入）。
    ///
    /// 职位按 `index` 排序后重建，保证 `ranks[i].index == i`；成员的职位序号超出范围时
    /// 落到最低职位（数据修好后写回）。
    #[must_use]
    pub fn from_record(record: GuildRecord, settings: &GuildSettings) -> Self {
        let mut ranks: Vec<GuildRank> = record
            .ranks
            .iter()
            .map(|rank| GuildRank {
                name: rank.name.clone(),
                options: rank.options,
                index: i32::from(rank.index),
                members: Vec::new(),
            })
            .collect();
        ranks.sort_by_key(|rank| rank.index);
        if ranks.is_empty() {
            ranks.push(GuildRank {
                name: GUILD_LEADER_RANK_NAME.to_owned(),
                options: GUILD_LEADER_OPTIONS,
                index: 0,
                members: Vec::new(),
            });
        }
        for (position, rank) in ranks.iter_mut().enumerate() {
            rank.index = i32::try_from(position).unwrap_or(0);
        }

        let last_rank = u8::try_from(ranks.len().saturating_sub(1)).unwrap_or(0);
        let mut members = HashMap::with_capacity(record.members.len());
        for member in &record.members {
            let rank_index = if usize::from(member.rank_index) < ranks.len() {
                member.rank_index
            } else {
                last_rank
            };
            members.insert(
                member.character_id,
                GuildMemberState {
                    character_id: member.character_id,
                    name: member.name.clone(),
                    rank_index,
                    level: 1,
                    joined_at: member.joined_at,
                    last_login: DateTimeBinary(0),
                    online: None,
                },
            );
        }

        let mut guild = Self {
            id: record.id,
            name: record.name,
            leader: record.leader_character_id,
            level: u8::try_from(record.level).unwrap_or(0),
            experience: record.experience,
            gold: u32::try_from(record.gold).unwrap_or(u32::MAX),
            spare_points: u8::try_from(record.spare_points).unwrap_or(0),
            notice: record.notice,
            ranks,
            members,
            max_experience: 0,
            member_cap: 0,
            next_exp_update: 0,
        };
        guild.refresh_limits(settings);
        guild
    }
}

/// 一次行会成员编辑（`C.EditGuildMember`，`Server/MirNetwork/MirConnection.cs:1256-1261`）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GuildMemberEdit {
    /// 变动类型（Crystal `ChangeType`，`PlayerObject.cs:9847-9955`）：
    ///
    /// * `0` 加人（发邀请）
    /// * `1` 踢人
    /// * `2` 升/降职
    /// * `3` 改职位名
    /// * `4` 新职位
    /// * `5` 改职位权限
    pub change_type: u8,
    /// 目标职位序号（`0` = 会长）。
    pub rank_index: u8,
    /// 目标角色名（`ChangeType=5` 时是 `"true"`/`"false"`）。
    pub name: String,
    /// 职位名（`ChangeType=5` 时是权限位序号）。
    pub rank_name: String,
}

/// `C.GuildInvite` 的处理结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuildJoinOutcome {
    /// 已加入（值是行会 ID）。
    Joined(i32),
    /// 明确拒绝。
    Declined,
}

/// 行会操作失败原因。
///
/// 每个变体都对应 Crystal 的一次 `ReceiveChat(ServerTextKeys.…)` + `return`，
/// 行号见各变体的注释。[`GuildError::message`] 给出中文提示（Crystal 走
/// `GameLanguage.ServerTextMap`；本作的 i18n 词条还没覆盖行会，见文档）。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GuildError {
    /// 对象不在世界里。
    #[error("对象 {0} 不在世界里")]
    NotInWorld(u32),
    /// 自己不在行会（`ServerTextKeys.NotInGuild`，`PlayerObject.cs:9844`）。
    #[error("你不在行会中")]
    NotInGuild(u32),
    /// 已经在一个行会里（`CreateGuild` 的 `if ((MyGuild != null) || (Info.GuildIndex != -1))`，
    /// `PlayerObject.cs:9710`）。
    #[error("你已经在行会中")]
    AlreadyInGuild,
    /// 行会名已被占用（`ServerTextKeys.GuildAlreadyExists`，`:10051-10055`）。
    #[error("行会 {0} 已经存在")]
    NameTaken(String),
    /// 行会名太短/太长（`ServerTextKeys.GuildNameTooLong` / `GuildNameRestrictedLength`，
    /// `:10033-10037`）。
    #[error("行会名长度必须是 {GUILD_NAME_MIN_LEN}-{GUILD_NAME_MAX_LEN} 个字符")]
    NameLength,
    /// 行会名含非法字符（`Name.Contains('\\')`，`:10039-10043`）。
    #[error("行会名不能包含反斜杠或换行")]
    NameInvalid,
    /// 等级不够创建（`ServerTextKeys.LevelNotEnoughCreateGuild`，`:9713-9717`）。
    #[error("创建行会需要 {required} 级（你当前 {level} 级）")]
    LevelTooLow {
        /// 需要的等级。
        required: u16,
        /// 当前等级。
        level: u16,
    },
    /// 金币不够创建（`ServerTextKeys.InsufficientGoldCreateGuild`，`:9733-9736`）。
    #[error("创建行会需要 {required} 金币（你只有 {gold}）")]
    NotEnoughGold {
        /// 需要的金币。
        required: u32,
        /// 当前金币。
        gold: i64,
    },
    /// 自己的职位没有这项权限（`ServerTextKeys.NotAllowed*`，`PlayerObject.cs:9850-9947`）。
    #[error("你的职位没有 {0} 权限")]
    Permission(&'static str),
    /// 找不到这个成员（`GuildObject.ChangeRank`/`DeleteMember` 返回 false，`:254`、`:456`）。
    #[error("行会里没有成员 {0}")]
    MemberNotFound(String),
    /// 目标已经在行会里（`ServerTextKeys.AlreadyInGuild`，`:9864-9868`）。
    #[error("{0} 已经在行会中")]
    TargetAlreadyInGuild(String),
    /// 目标正被邀请（`ServerTextKeys.GuildInvitePending`，`:9874-9878`）。
    #[error("{0} 已经收到一个行会邀请")]
    TargetAlreadyInvited(String),
    /// 不能邀请自己（**本作扩展**：Crystal 只会在接受时因为已入会而失败）。
    #[error("不能邀请自己")]
    CannotInviteSelf,
    /// 行会人数已满（`ServerTextKeys.GuildInviteFull`，`:9990-9994`）。
    #[error("行会人数已满")]
    GuildFull,
    /// 没有待接受的行会邀请（`ServerTextKeys.GuildNotInvited`，`:9980-9984`）。
    #[error("没有待接受的行会邀请")]
    NoInvitation,
    /// 升成会长需要目标达到行会要求等级（`ServerTextKeys.GuildLeaderMinLevel`，`GuildObject.cs:258-262`）。
    #[error("对方等级不足以成为会长（需要 {required} 级）")]
    LeaderLevelTooLow {
        /// 需要的等级。
        required: u16,
    },
    /// 会长职位至少要保留两个会长（`ServerTextKeys.GuildNeedsTwoLeaders`，`GuildObject.cs:267-271`）。
    #[error("行会至少要有两个会长才能降职")]
    GuildNeedsTwoLeaders,
    /// 降职会长时必须还有一个会长在线（`ServerTextKeys.NeedOneLeaderOnline`，`:272-279`）。
    #[error("需要至少一个会长在线")]
    NeedOneLeaderOnline,
    /// 职位序号不存在（`ServerTextKeys.RankNotFound`，`GuildObject.cs:356-360`）。
    #[error("职位 {0} 不存在")]
    RankNotFound(u8),
    /// 职位名太短（`ServerTextKeys.RankNameTooShort`，`PlayerObject.cs:9917-9921`）。
    #[error("职位名至少 {GUILD_RANK_NAME_MIN_LEN} 个字符")]
    RankNameTooShort,
    /// 职位名太长（`:9922-9925`）。
    #[error("职位名最多 {GUILD_RANK_NAME_MAX_LEN} 个字符")]
    RankNameTooLong,
    /// 不能改自己职位的权限（`ServerTextKeys.CannotChangeOwnRankOptions`，`GuildObject.cs:361-365`）。
    #[error("不能修改自己职位的权限")]
    CannotChangeOwnRankOptions,
    /// 职位数量已达上限（`ServerTextKeys.NoMoreRankSlotsAvailable`，`PlayerObject.cs:9935-9939`）。
    #[error("职位数量已达上限")]
    TooManyRanks,
    /// 职位不够高（`ServerTextKeys.YourRankNotAdequate`，`GuildObject.cs:395-399`、`:457-461`）。
    #[error("你的职位不够高")]
    RankNotAdequate,
    /// 公告行数超限（`ServerTextKeys.GuildNoticeMaxLines`，`PlayerObject.cs:9971-9975`）。
    #[error("公告最多 {GUILD_NOTICE_MAX_LINES} 行")]
    NoticeTooLong,
    /// 会长退出/被删时行会里没有别的会长可接管（`ServerTextKeys.YouNeedLastLeaderToDisbandGuild`，
    /// `GuildObject.cs:474`）。
    #[error("你是最后一个会长：请先转让会长或解散行会")]
    LastLeaderCannotLeave,
}

impl GuildError {
    /// 给玩家看的中文提示（Crystal 用 `GameLanguage.ServerTextMap` 的 `ServerTextKeys`）。
    #[must_use]
    pub fn message(&self) -> String {
        self.to_string()
    }
}

// ---------------------------------------------------------------------------
// 帧构造
// ---------------------------------------------------------------------------

/// `S.GuildStatus`（包号 168）。
fn status_frame(status: &GuildStatus) -> Vec<u8> {
    ServerPacket::GuildStatus(status.clone()).encode_frame()
}

/// `S.GuildMemberChange`（167）。
fn member_change_frame(name: &str, status: u8, rank_index: u8, ranks: Vec<GuildRank>) -> Vec<u8> {
    ServerPacket::GuildMemberChange(GuildMemberChange {
        name: name.to_owned(),
        rank_index,
        status,
        ranks,
    })
    .encode_frame()
}

/// `S.GuildNoticeChange`（166）。
fn notice_frame(update: i32, notice: Vec<String>) -> Vec<u8> {
    ServerPacket::GuildNoticeChange(GuildNoticeChange { update, notice }).encode_frame()
}

/// 判断职位权限位是否包含 `option`（C# `Options.HasFlag(option)`）。
#[must_use]
pub fn has_option(options: u8, option: GuildRankOptions) -> bool {
    options & option.value() != 0
}

/// 判断行会名是否合法（长度 + 非法字符）。
///
/// 对应 Crystal `PlayerObject.GuildNameReturn`（`:10033-10043`）。
pub fn validate_guild_name(name: &str) -> Result<(), GuildError> {
    let length = name.chars().count();
    if !(GUILD_NAME_MIN_LEN..=GUILD_NAME_MAX_LEN).contains(&length) {
        return Err(GuildError::NameLength);
    }
    if name
        .chars()
        .any(|ch| GUILD_NAME_FORBIDDEN.contains(&ch) || ch.is_control())
    {
        return Err(GuildError::NameInvalid);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// World：查询
// ---------------------------------------------------------------------------

impl World {
    /// 行会配置（`Settings.Guild_*`）。
    #[must_use]
    pub fn guild_settings(&self) -> &GuildSettings {
        &self.guild_settings
    }

    /// 替换行会配置，并按新表刷新所有行会的 `MaxExperience`/`MemberCap`。
    pub fn set_guild_settings(&mut self, settings: GuildSettings) {
        self.guild_settings = settings;
        let settings = &self.guild_settings;
        for guild in self.guilds.values_mut() {
            guild.refresh_limits(settings);
        }
    }

    /// 按 ID 找行会。
    #[must_use]
    pub fn guild(&self, guild_id: i32) -> Option<&Guild> {
        self.guilds.get(&guild_id)
    }

    /// 全部行会（按 ID 升序，便于确定性测试与日志）。
    pub fn guilds(&self) -> Vec<&Guild> {
        let mut guilds: Vec<&Guild> = self.guilds.values().collect();
        guilds.sort_by_key(|guild| guild.id);
        guilds
    }

    /// 按名字找行会，语义照抄 Crystal `Envir.GetGuild(string)`（`Envir.cs:4832-4841`）：
    /// **去空格 + 忽略大小写**。
    #[must_use]
    pub fn guild_by_name(&self, name: &str) -> Option<&Guild> {
        let normalized = name.replace(' ', "").to_lowercase();
        self.guilds
            .values()
            .find(|guild| guild.name.replace(' ', "").to_lowercase() == normalized)
    }

    /// 某个在线玩家的行会。
    #[must_use]
    pub fn guild_of(&self, player_id: u32) -> Option<&Guild> {
        let guild_ref = self.players.get(&player_id)?.guild?;
        self.guilds.get(&guild_ref.guild_id)
    }

    /// 某个在线玩家的行会引用。
    #[must_use]
    pub fn guild_ref_of(&self, player_id: u32) -> Option<GuildRef> {
        self.players.get(&player_id)?.guild
    }

    /// 按角色 ID 反查行会引用（登录时用；成员表是唯一事实来源）。
    #[must_use]
    pub fn guild_ref_of_character(&self, character_id: i64) -> Option<GuildRef> {
        self.guilds.values().find_map(|guild| {
            guild.members.get(&character_id).map(|member| GuildRef {
                guild_id: guild.id,
                rank_index: member.rank_index,
            })
        })
    }

    /// 某个玩家的待接受邀请（行会 ID）。
    #[must_use]
    pub fn guild_invitation_of(&self, player_id: u32) -> Option<i32> {
        self.guild_invites.get(&player_id).copied()
    }

    /// 取走待落盘的行会写入（会话在每个包处理完之后调用并写库）。
    pub fn take_guild_writes(&mut self) -> Vec<crate::world::GuildWrite> {
        std::mem::take(&mut self.guild_writes)
    }

    /// 启动时把数据库里的行会灌进世界（Crystal `Envir.LoadGuilds`，`Envir.cs:3089-3117`）。
    ///
    /// 重复 ID 会被覆盖（以最后一次为准），返回载入条数。
    pub fn load_guilds(&mut self, records: Vec<GuildRecord>) -> usize {
        let settings = self.guild_settings.clone();
        let mut loaded = 0;
        for record in records {
            let guild = Guild::from_record(record, &settings);
            self.next_guild_id = self.next_guild_id.max(guild.id.saturating_add(1));
            self.guilds.insert(guild.id, guild);
            loaded += 1;
        }
        loaded
    }

    /// 找一个行会，顺便校验「我在行会里」并返回（行会 ID、自己的职位序号）。
    fn my_guild(&self, player_id: u32) -> Result<(i32, u8), GuildError> {
        if !self.players.contains_key(&player_id) {
            return Err(GuildError::NotInWorld(player_id));
        }
        let guild_ref = self
            .players
            .get(&player_id)
            .and_then(|player| player.guild)
            .ok_or(GuildError::NotInGuild(player_id))?;
        if !self.guilds.contains_key(&guild_ref.guild_id) {
            return Err(GuildError::NotInGuild(player_id));
        }
        Ok((guild_ref.guild_id, guild_ref.rank_index))
    }

    /// 某个玩家的职位权限位（不在行会返回 `None`）。
    #[must_use]
    pub fn guild_options_of(&self, player_id: u32) -> Option<u8> {
        let guild_ref = self.players.get(&player_id)?.guild?;
        let guild = self.guilds.get(&guild_ref.guild_id)?;
        guild.rank(guild_ref.rank_index).map(|rank| rank.options)
    }

    /// 给行会的**在线**成员发一帧（Crystal `GuildObject.SendServerPacket`，`:602-615`）。
    fn send_to_guild(&self, guild_id: i32, frame: &[u8]) {
        if let Some(guild) = self.guilds.get(&guild_id) {
            for object_id in guild.online_members() {
                self.send_to_player(object_id, frame);
            }
        }
    }

    /// 给某个人发行会状态；`rank_index` 用他自己的职位。
    fn send_guild_status(&self, player_id: u32) {
        let Some(guild_ref) = self.players.get(&player_id).and_then(|player| player.guild) else {
            return;
        };
        let Some(guild) = self.guilds.get(&guild_ref.guild_id) else {
            return;
        };
        self.send_to_player(
            player_id,
            &status_frame(&guild.status(Some(guild_ref.rank_index))),
        );
    }

    /// 把行会状态发给所有在线成员（Crystal `GuildObject.cs:685-694`）。
    fn send_guild_status_to_all(&self, guild_id: i32) {
        let Some(guild) = self.guilds.get(&guild_id) else {
            return;
        };
        for object_id in guild.online_members() {
            let rank_index = guild
                .members
                .values()
                .find(|member| member.online == Some(object_id))
                .map(|member| member.rank_index);
            self.send_to_player(object_id, &status_frame(&guild.status(rank_index)));
        }
    }

    /// 给某个玩家发一条系统提示。
    fn guild_error_to(&self, player_id: u32, error: &GuildError) {
        self.send_system_chat(player_id, &error.message());
    }

    /// 给某个玩家发一条**行会频道**消息（Crystal `ReceiveChat(..., ChatType.Guild)`）。
    fn send_guild_chat_to(&self, player_id: u32, message: &str) {
        self.send_to_player(
            player_id,
            &ServerPacket::Chat(Chat {
                message: message.to_owned(),
                type_: ChatType::Guild,
            })
            .encode_frame(),
        );
    }

    /// 记一条「该行会要落盘」。
    fn mark_guild_dirty(&mut self, guild_id: i32) {
        let Some(guild) = self.guilds.get(&guild_id) else {
            return;
        };
        let record = guild.to_record();
        self.guild_writes
            .push(crate::world::GuildWrite::Save(Box::new(record)));
    }
}

// ---------------------------------------------------------------------------
// World：上线 / 下线
// ---------------------------------------------------------------------------

impl World {
    /// 玩家进世界时**静默**接上行会状态（不发包）。
    ///
    /// 由 [`World::enter`] 调用：`Player` 一进去世界就要有正确的 `guild`，
    /// 但那时进世界的包序还没走到 `UserInformation`，不能插 `S.GuildStatus`
    /// （Crystal 在 `PlayerObject.cs:1309` 才发，晚于 `UserInformation`）。
    ///
    /// 校验照 `PlayerObject.cs:1153-1170`：
    /// 没有 `Player.guild` 时按角色 ID 反查成员表；行会不存在/自己不在成员表里 → 清空。
    pub fn guild_bind_player(&mut self, object_id: u32) {
        let Some(player) = self.players.get(&object_id) else {
            return;
        };
        let character_id = player.character_id;
        let name = player.name.clone();
        let level = player.level;
        let mut guild_ref = player
            .guild
            .or_else(|| self.guild_ref_of_character(character_id));

        if let Some(reference) = guild_ref {
            let valid = self
                .guilds
                .get(&reference.guild_id)
                .and_then(|guild| guild.members.get(&character_id))
                .map(|member| member.rank_index);
            if let Some(rank_index) = valid {
                guild_ref = Some(GuildRef {
                    guild_id: reference.guild_id,
                    rank_index,
                });
            } else {
                // Crystal：`Info.GuildIndex = -1; ReceiveChat(RemoveGuild)`（`:1157-1158`）。
                guild_ref = None;
                self.send_system_chat(object_id, "你已经被移出行会。");
                self.guild_writes
                    .push(crate::world::GuildWrite::SetCharacterGuild {
                        character_id,
                        guild_id: -1,
                    });
            }
        }

        if let Some(player) = self.players.get_mut(&object_id) {
            player.guild = guild_ref;
        }
        let Some(reference) = guild_ref else {
            return;
        };
        let mut changed = false;
        if let Some(guild) = self.guilds.get_mut(&reference.guild_id) {
            if let Some(member) = guild.members.get_mut(&character_id) {
                // 只有在线状态/名字/等级真的变了才落盘（Crystal 也只在登出时置 `NeedSave`，
                // `GuildObject.cs:180`）。
                if member.online != Some(object_id) || member.name != name || member.level != level
                {
                    changed = true;
                }
                member.online = Some(object_id);
                member.name.clone_from(&name);
                member.level = level;
            }
        }
        if changed {
            self.mark_guild_dirty(reference.guild_id);
        }
    }

    /// 玩家进世界后广播行会上线（Crystal `PlayerObject.cs:1309-1316` 的 `PlayerLogged`）。
    ///
    /// 必须排在 `S.UserInformation` 之后（会话在 `on_start_game` 里显式调用）：
    ///
    /// 1. 先 [`World::guild_bind_player`] 把状态接上；
    /// 2. 给所有在线成员发 `S.GuildMemberChange{Status=1}`，再给本人发 `S.GuildStatus`。
    pub fn guild_player_online(&mut self, object_id: u32) {
        self.guild_bind_player(object_id);
        let Some(reference) = self.players.get(&object_id).and_then(|player| player.guild) else {
            return;
        };
        let Some(name) = self
            .players
            .get(&object_id)
            .map(|player| player.name.clone())
        else {
            return;
        };
        let frame = member_change_frame(&name, 1, reference.rank_index, Vec::new());
        self.send_to_guild(reference.guild_id, &frame);
        self.send_guild_status(object_id);
    }

    /// 玩家离开世界：成员标记离线（Crystal `PlayerLogged(member, false)`，`GuildObject.cs:162-192`）。
    pub fn guild_player_offline(&mut self, object_id: u32) {
        let Some(player) = self.players.get(&object_id) else {
            return;
        };
        let Some(reference) = player.guild else {
            return;
        };
        let character_id = player.character_id;
        let name = player.name.clone();
        let now = DateTimeBinary::from_unix_seconds(self.wall_now() / 1000);

        if let Some(guild) = self.guilds.get_mut(&reference.guild_id) {
            if let Some(member) = guild.members.get_mut(&character_id) {
                member.online = None;
                member.last_login = now;
            }
        }
        // `:186`：`Status = 0` 是「下线」。
        let frame = member_change_frame(&name, 0, reference.rank_index, Vec::new());
        self.send_to_guild(reference.guild_id, &frame);
        self.mark_guild_dirty(reference.guild_id);
    }
}

// ---------------------------------------------------------------------------
// World：创建
// ---------------------------------------------------------------------------

impl World {
    /// 分配一个新的行会 ID（Crystal `Envir.NextGuildID`，`Envir.cs:126`）。
    fn allocate_guild_id(&mut self) -> i32 {
        let id = self.next_guild_id.max(1);
        self.next_guild_id = id.saturating_add(1);
        id
    }

    /// 创建行会（Crystal `PlayerObject.CreateGuild`，`PlayerObject.cs:9708-9838`）。
    ///
    /// 校验顺序照抄源码：
    ///
    /// 1. 已有行会 → 拒绝（`:9710`）；
    /// 2. 重名 → 拒绝（`:9711`）；
    /// 3. 等级不够 → 拒绝（`:9713-9717`）；
    /// 4. 金币不够 → 拒绝（`:9733-9736`）；
    /// 5. 扣钱（`:9773-9783`）；
    /// 6. 建行会 + 会长职位，把自己设为会长（`:9820-9831`）；
    /// 7. 广播 `GuildMemberChange{Status=2}` + `GuildStatus`（`:9833-9835`）。
    ///
    /// 名字长度/非法字符的校验在 [`validate_guild_name`]（对应 `GuildNameReturn`，
    /// `:10033-10043`）；`C.GuildNameReturn` 与 NPC 动作 `CREATEGUILD` 都走这里。
    ///
    /// # 简化
    ///
    /// * **简化：不做 `NewbieGuild` 的 GM 限制**（Crystal `:9719-9723`）——原因：本作的
    ///   `Player` 没有管理员标志，也没有这个特殊行会（它还会把等级拉满 21，`GuildInfo.cs:68-72`）。
    /// * **简化：创建费用只扣金币**（`Guild_CreationCostList` 里 `ItemName` 为空的那一项）——
    ///   原因：Crystal 默认还要求 1 个 `WoomaHorn`（`Settings.cs:1336`），本作的创建流程
    ///   暂时不做物品扣除（见 `docs/world/guild.md` 的「未做」）。
    pub fn create_guild(&mut self, player_id: u32, name: &str) -> Result<i32, GuildError> {
        let Some(player) = self.players.get(&player_id) else {
            return Err(GuildError::NotInWorld(player_id));
        };
        if player.guild.is_some() || self.guild_ref_of_character(player.character_id).is_some() {
            return Err(GuildError::AlreadyInGuild);
        }
        validate_guild_name(name)?;
        if self.guild_by_name(name).is_some() {
            return Err(GuildError::NameTaken(name.to_owned()));
        }
        let settings = self.guild_settings.clone();
        if player.level < settings.required_level {
            return Err(GuildError::LevelTooLow {
                required: settings.required_level,
                level: player.level,
            });
        }
        if player.gold < i64::from(settings.creation_gold) {
            return Err(GuildError::NotEnoughGold {
                required: settings.creation_gold,
                gold: player.gold,
            });
        }

        // `Enqueue(new S.LoseGold { Gold = Required.Amount });`（`:9781`）。
        let _ = self.lose_gold(player_id, settings.creation_gold);

        let (character_id, player_name, level) = {
            let player = &self.players[&player_id];
            (player.character_id, player.name.clone(), player.level)
        };
        let id = self.allocate_guild_id();
        let joined_at = self.wall_now();
        let guild = Guild::new(
            id,
            name,
            character_id,
            &player_name,
            level,
            joined_at,
            &settings,
        );
        let rank_index = GUILD_LEADER_RANK_INDEX;
        self.guilds.insert(id, guild);
        if let Some(player) = self.players.get_mut(&player_id) {
            player.guild = Some(GuildRef {
                guild_id: id,
                rank_index,
            });
        }
        if let Some(guild) = self.guilds.get_mut(&id) {
            if let Some(member) = guild.members.get_mut(&character_id) {
                member.online = Some(player_id);
            }
        }

        // 写库：整份行会 + 角色的 `guild_index`。
        self.mark_guild_dirty(id);
        self.guild_writes
            .push(crate::world::GuildWrite::SetCharacterGuild {
                character_id,
                guild_id: id,
            });

        // `PlayerLogged(newMember, true, New: true)` → `GuildMemberChange{Status=2}`（`:186`）。
        let frame = member_change_frame(&player_name, 2, rank_index, Vec::new());
        self.send_to_guild(id, &frame);
        // `MyGuild.SendGuildStatus(this)`（`:9835`）。
        self.send_guild_status(player_id);
        tracing::info!(guild = id, name, player = %player_name, "已创建行会");
        Ok(id)
    }
}

// ---------------------------------------------------------------------------
// World：邀请 / 加入
// ---------------------------------------------------------------------------

impl World {
    /// 邀请某人入会（`C.EditGuildMember{ChangeType=0}`，
    /// Crystal `PlayerObject.EditGuildMember` 的 `case 0`，`PlayerObject.cs:9849-9888`）。
    ///
    /// 包序：只给被邀请者发 `S.GuildInvite{Name=行会名}`（`:9886`），邀请者没有任何回包。
    pub fn guild_invite(&mut self, player_id: u32, target_name: &str) -> Result<u32, GuildError> {
        let (guild_id, rank_index) = self.my_guild(player_id)?;
        let options = self
            .guilds
            .get(&guild_id)
            .and_then(|guild| guild.rank(rank_index))
            .map_or(0, |rank| rank.options);
        if !has_option(options, GuildRankOptions::CanRecruit) {
            return Err(GuildError::Permission("CanRecruit"));
        }
        let Some(target) = self.player_by_name(target_name) else {
            return Err(GuildError::MemberNotFound(target_name.to_owned()));
        };
        let target_id = target.object_id;
        let target_character = target.character_id;
        let target_name = target.name.clone();
        if target_character == self.players[&player_id].character_id {
            return Err(GuildError::CannotInviteSelf);
        }
        if self.guild_invites.contains_key(&target_id)
            || self.guild_ref_of_character(target_character).is_some()
        {
            if self.guild_invites.contains_key(&target_id) {
                return Err(GuildError::TargetAlreadyInvited(target_name));
            }
            return Err(GuildError::TargetAlreadyInGuild(target_name));
        }
        let guild_name = self.guilds[&guild_id].name.clone();
        self.send_to_player(
            target_id,
            &ServerPacket::GuildInvite(GuildInvitePacket { name: guild_name }).encode_frame(),
        );
        self.guild_invites.insert(target_id, guild_id);
        Ok(target_id)
    }

    /// 接受/拒绝行会邀请（`C.GuildInvite`，Crystal `PlayerObject.GuildInvite`，
    /// `PlayerObject.cs:9978-10011`）。
    ///
    /// 接受时的包序（照抄 `:9995-10003`）：
    /// 1. 补最低职位（若只有 `Leader`，`GuildObject.cs:224-227`）；
    /// 2. 成员入会，广播 `S.GuildMemberChange{Status=2}`；
    /// 3. 给本人 `S.GuildStatus`。
    pub fn guild_join(
        &mut self,
        player_id: u32,
        accept: bool,
    ) -> Result<GuildJoinOutcome, GuildError> {
        if !self.players.contains_key(&player_id) {
            return Err(GuildError::NotInWorld(player_id));
        }
        let guild_id = self
            .guild_invites
            .get(&player_id)
            .copied()
            .ok_or(GuildError::NoInvitation)?;
        if !accept {
            self.guild_invites.remove(&player_id);
            return Ok(GuildJoinOutcome::Declined);
        }
        // `if (!PendingGuildInvite.HasRoom())`（`:9990`）：满员时邀请保留。
        let has_room = self.guilds.get(&guild_id).is_some_and(Guild::has_room);
        if !has_room {
            return Err(GuildError::GuildFull);
        }
        let (character_id, name, level) = {
            let player = &self.players[&player_id];
            (player.character_id, player.name.clone(), player.level)
        };
        if self.guild_ref_of_character(character_id).is_some() {
            self.guild_invites.remove(&player_id);
            return Err(GuildError::AlreadyInGuild);
        }

        let joined_at = self.wall_now();
        let Some(guild) = self.guilds.get_mut(&guild_id) else {
            self.guild_invites.remove(&player_id);
            return Err(GuildError::NotInGuild(player_id));
        };
        // `GuildObject.NewMember`：只有 Leader 职位时补一个 `Members`（`:224-227`）。
        if guild.ranks.len() < 2 {
            guild.ranks.push(GuildRank {
                name: GUILD_DEFAULT_RANK_NAME.to_owned(),
                options: 0,
                index: i32::try_from(guild.ranks.len()).unwrap_or(1),
                members: Vec::new(),
            });
        }
        let rank_index = u8::try_from(guild.ranks.len().saturating_sub(1)).unwrap_or(0);
        guild.members.insert(
            character_id,
            GuildMemberState {
                character_id,
                name: name.clone(),
                rank_index,
                level,
                joined_at,
                last_login: DateTimeBinary(0),
                online: Some(player_id),
            },
        );

        self.guild_invites.remove(&player_id);
        if let Some(player) = self.players.get_mut(&player_id) {
            player.guild = Some(GuildRef {
                guild_id,
                rank_index,
            });
        }
        // `PlayerLogged(this, true, New: true)`（`:186`）。
        let frame = member_change_frame(&name, 2, rank_index, Vec::new());
        self.send_to_guild(guild_id, &frame);
        self.send_guild_status(player_id);
        self.mark_guild_dirty(guild_id);
        self.guild_writes
            .push(crate::world::GuildWrite::SetCharacterGuild {
                character_id,
                guild_id,
            });
        Ok(GuildJoinOutcome::Joined(guild_id))
    }
}

// ---------------------------------------------------------------------------
// World：成员编辑（升/降职、踢人、职位名、权限）
// ---------------------------------------------------------------------------

impl World {
    /// `C.EditGuildMember`（Crystal `PlayerObject.EditGuildMember`，`PlayerObject.cs:9840-9957`）。
    ///
    /// 分派到下面几个私有函数，`ChangeType` 取值见 [`GuildMemberEdit::change_type`]。
    pub fn guild_edit_member(
        &mut self,
        player_id: u32,
        edit: &GuildMemberEdit,
    ) -> Result<(), GuildError> {
        if !self.players.contains_key(&player_id) {
            return Err(GuildError::NotInWorld(player_id));
        }
        let (guild_id, rank_index) = self.my_guild(player_id)?;
        let options = self
            .guilds
            .get(&guild_id)
            .and_then(|guild| guild.rank(rank_index))
            .map_or(0, |rank| rank.options);
        let result = match edit.change_type {
            0 => {
                if !has_option(options, GuildRankOptions::CanRecruit) {
                    Err(GuildError::Permission("CanRecruit"))
                } else if edit.name.is_empty() {
                    Ok(())
                } else {
                    self.guild_invite(player_id, &edit.name).map(|_| ())
                }
            }
            1 => {
                if !has_option(options, GuildRankOptions::CanKick) {
                    Err(GuildError::Permission("CanKick"))
                } else if edit.name.is_empty() {
                    Ok(())
                } else {
                    self.guild_kick_or_leave(player_id, &edit.name)
                }
            }
            2 => {
                if !has_option(options, GuildRankOptions::CanChangeRank) {
                    Err(GuildError::Permission("CanChangeRank"))
                } else if edit.name.is_empty() {
                    Ok(())
                } else {
                    self.guild_change_rank(player_id, &edit.name, edit.rank_index)
                }
            }
            3 => {
                if has_option(options, GuildRankOptions::CanChangeRank) {
                    self.guild_change_rank_name(player_id, edit.rank_index, &edit.rank_name)
                } else {
                    Err(GuildError::Permission("CanChangeRank"))
                }
            }
            4 => {
                if has_option(options, GuildRankOptions::CanChangeRank) {
                    self.guild_new_rank(player_id)
                } else {
                    Err(GuildError::Permission("CanChangeRank"))
                }
            }
            5 => {
                if has_option(options, GuildRankOptions::CanChangeRank) {
                    match edit.rank_name.trim().parse::<u8>() {
                        Ok(option) => self.guild_change_rank_option(
                            player_id,
                            edit.rank_index,
                            option,
                            &edit.name,
                        ),
                        // `int.TryParse` 失败时 Crystal 直接 return（`:9950-9953`）。
                        Err(_) => Ok(()),
                    }
                } else {
                    Err(GuildError::Permission("CanChangeRank"))
                }
            }
            _ => Ok(()),
        };
        if let Err(error) = &result {
            self.guild_error_to(player_id, error);
        }
        result
    }

    /// 升/降职（Crystal `GuildObject.ChangeRank`，`GuildObject.cs:238-332`）。
    ///
    /// 校验顺序：
    /// 1. 目标职位存在（`:241`）；
    /// 2. 找到成员（`:242-254`）；
    /// 3. 升成会长要求目标等级 ≥ `Guild_RequiredLevel`（`:258-262`）；
    /// 4. 不能动比自己高的职位（会长例外，`:264`）；
    /// 5. 降职会长时：会长少于 3 个 → 拒绝；没有别的会长在线 → 拒绝（`:265-279`）；
    /// 6. 移动成员并发包（`:282-331`）。
    fn guild_change_rank(
        &mut self,
        player_id: u32,
        member_name: &str,
        rank_index: u8,
    ) -> Result<(), GuildError> {
        let (guild_id, my_rank) = self.my_guild(player_id)?;
        let required_level = self.guild_settings.required_level;
        let guild = self
            .guilds
            .get(&guild_id)
            .ok_or(GuildError::NotInGuild(player_id))?;
        if !guild.has_rank(rank_index) {
            return Err(GuildError::RankNotFound(rank_index));
        }
        let member = guild
            .member_by_name(member_name)
            .cloned()
            .ok_or_else(|| GuildError::MemberNotFound(member_name.to_owned()))?;
        if rank_index == GUILD_LEADER_RANK_INDEX && member.level < required_level {
            return Err(GuildError::LeaderLevelTooLow {
                required: required_level,
            });
        }
        if member.rank_index >= my_rank && my_rank != GUILD_LEADER_RANK_INDEX {
            return Err(GuildError::RankNotAdequate);
        }
        if member.rank_index == GUILD_LEADER_RANK_INDEX {
            let leaders: Vec<&GuildMemberState> = guild
                .members
                .values()
                .filter(|candidate| candidate.rank_index == GUILD_LEADER_RANK_INDEX)
                .collect();
            if leaders.len() <= 2 {
                return Err(GuildError::GuildNeedsTwoLeaders);
            }
            let another_leader_online = leaders.iter().any(|leader| {
                leader.character_id != member.character_id && leader.online.is_some()
            });
            if !another_leader_online {
                return Err(GuildError::NeedOneLeaderOnline);
            }
        }

        let member_character_id = member.character_id;
        let member_name = member.name.clone();
        let old_rank = member.rank_index;
        let old_rank_name = guild
            .rank(old_rank)
            .map_or_else(String::new, |rank| rank.name.clone());

        let (snapshot, _leader) = {
            let guild = self
                .guilds
                .get_mut(&guild_id)
                .ok_or(GuildError::NotInGuild(player_id))?;
            if let Some(member) = guild.members.get_mut(&member_character_id) {
                member.rank_index = rank_index;
            }
            if rank_index == GUILD_LEADER_RANK_INDEX {
                guild.leader = member_character_id;
            }
            (
                guild.rank_snapshot(rank_index).unwrap_or_default(),
                guild.leader,
            )
        };
        // 被调动的人自己在线上时：`GuildMemberChange{Name = 操作者名, Status=8, Ranks=[新职位]}`（`:319`）。
        let operator = self.players[&player_id].name.clone();
        if let Some(object_id) = self
            .guilds
            .get(&guild_id)
            .and_then(|guild| guild.members.get(&member_character_id))
            .and_then(|member| member.online)
        {
            if let Some(player) = self.players.get_mut(&object_id) {
                if let Some(reference) = &mut player.guild {
                    reference.rank_index = rank_index;
                }
            }
            self.send_to_player(
                object_id,
                &member_change_frame(&operator, 8, rank_index, vec![snapshot.clone()]),
            );
        }
        // 其他人：`GuildMemberChange{Name = 被调动者, Status=5, RankIndex}`（`:328`）。
        let others = self
            .guilds
            .get(&guild_id)
            .map(Guild::online_members)
            .unwrap_or_default();
        for object_id in others {
            let is_member = self.guilds[&guild_id]
                .members
                .get(&member_character_id)
                .is_some_and(|member| member.online == Some(object_id));
            if is_member {
                continue;
            }
            self.send_to_player(
                object_id,
                &member_change_frame(&member_name, 5, rank_index, Vec::new()),
            );
        }
        tracing::debug!(
            guild = guild_id,
            member = %member_name,
            from = old_rank,
            from_name = %old_rank_name,
            to = rank_index,
            "行会职位变动"
        );
        self.mark_guild_dirty(guild_id);
        Ok(())
    }

    /// 踢人 / 自己退会（Crystal `GuildObject.DeleteMember`，`GuildObject.cs:433-509`）。
    ///
    /// `target_name` 等于自己的名字时就是「退会」（`PlayerObject.cs:3261` 的 `LEAVEGUILD`
    /// 与客户端「离开行会」按钮走同一条路）。
    ///
    /// 最后一个会长退出且行会没有别人 → 解散并删库（`:463-509` 的 `LeaderOk`）。
    pub fn guild_kick_or_leave(
        &mut self,
        player_id: u32,
        target_name: &str,
    ) -> Result<(), GuildError> {
        let (guild_id, my_rank) = self.my_guild(player_id)?;
        let my_name = self.players[&player_id].name.clone();
        let is_self = my_name == target_name;
        let guild = self
            .guilds
            .get(&guild_id)
            .ok_or(GuildError::NotInGuild(player_id))?;
        let member = guild
            .member_by_name(target_name)
            .cloned()
            .ok_or_else(|| GuildError::MemberNotFound(target_name.to_owned()))?;
        // `if ((Kicker.MyGuildRank.Index >= MemberRank.Index) && (Kicker.MyGuildRank.Index != 0)
        //      && (Kicker.Info.Name != membername))`（`:457-461`）。
        if my_rank >= member.rank_index && my_rank != GUILD_LEADER_RANK_INDEX && !is_self {
            return Err(GuildError::RankNotAdequate);
        }
        let member_count = guild.members.len();
        let leaders = guild
            .members
            .values()
            .filter(|candidate| candidate.rank_index == GUILD_LEADER_RANK_INDEX)
            .count();
        // 会长退出：`if (MemberRank.Members.Count < 2 && Info.Membercount < 2) goto LeaderOk;`
        // `else if (MemberRank.Members.Count > 1) goto AllOk;`（`:463-476`）。
        let mut disband = false;
        if member.rank_index == GUILD_LEADER_RANK_INDEX {
            if leaders < 2 && member_count < 2 {
                disband = true;
            } else if leaders <= 1 {
                return Err(GuildError::LastLeaderCannotLeave);
            }
        }
        self.guild_remove_member(guild_id, member.character_id, is_self, disband)?;
        if disband {
            self.send_system_chat(player_id, "你已经解散了行会。");
        }
        Ok(())
    }

    /// 退会（`C.LeaveGuild` 没有独立包：客户端发的是
    /// `C.EditGuildMember{ChangeType=1, Name=自己}`，见 `docs/world/guild.md`）。
    pub fn guild_leave(&mut self, player_id: u32) -> Result<(), GuildError> {
        let name = self
            .players
            .get(&player_id)
            .map(|player| player.name.clone())
            .ok_or(GuildError::NotInWorld(player_id))?;
        self.guild_kick_or_leave(player_id, &name)
    }

    /// `GuildObject.MemberDeleted`（`GuildObject.cs:539-564`）+ 删除成员 + 按需解散。
    fn guild_remove_member(
        &mut self,
        guild_id: i32,
        character_id: i64,
        self_left: bool,
        disband: bool,
    ) -> Result<(), GuildError> {
        let Some((member_name, former_online)) = self.guilds.get(&guild_id).and_then(|guild| {
            guild
                .members
                .get(&character_id)
                .map(|member| (member.name.clone(), member.online))
        }) else {
            return Err(GuildError::MemberNotFound(character_id.to_string()));
        };

        // 先给剩下的在线成员发 `Status=3`（被踢）/`Status=4`（自己退会）（`:548`）。
        let frame = member_change_frame(&member_name, if self_left { 4 } else { 3 }, 0, Vec::new());
        for object_id in self
            .guilds
            .get(&guild_id)
            .map(Guild::online_members)
            .unwrap_or_default()
        {
            if Some(object_id) == former_online {
                continue;
            }
            self.send_to_player(object_id, &frame);
        }

        // 被移除者：清掉行会引用、发空 `GuildStatus`（`:554-563`）。
        if let Some(object_id) = former_online {
            if let Some(player) = self.players.get_mut(&object_id) {
                player.guild = None;
            }
            self.send_to_player(object_id, &status_frame(&Guild::placeholder_status()));
            // `ReceiveChat(kickSelf ? YouHaveLeftGuild : YouRemovedFromGuild, ChatType.Guild)`
            // （`GuildObject.cs:559`）：**行会频道**，不是系统频道。
            self.send_guild_chat_to(
                object_id,
                if self_left {
                    "你离开了行会。"
                } else {
                    "你被移出了行会。"
                },
            );
            self.guild_writes
                .push(crate::world::GuildWrite::SetCharacterGuild {
                    character_id,
                    guild_id: -1,
                });
        } else {
            // 离线成员：也要把 `characters.guild_index` 清掉。
            self.guild_writes
                .push(crate::world::GuildWrite::SetCharacterGuild {
                    character_id,
                    guild_id: -1,
                });
        }

        let Some(guild) = self.guilds.get_mut(&guild_id) else {
            return Ok(());
        };
        guild.members.remove(&character_id);
        let leader_rank = GUILD_LEADER_RANK_INDEX;
        if guild.leader == character_id {
            guild.leader = guild
                .members
                .values()
                .find(|member| member.rank_index == leader_rank)
                .map_or(0, |member| member.character_id);
        }

        if disband {
            self.guilds.remove(&guild_id);
            self.guild_writes
                .push(crate::world::GuildWrite::Delete { guild_id });
            tracing::info!(guild = guild_id, "行会已解散");
            return Ok(());
        }

        self.mark_guild_dirty(guild_id);
        Ok(())
    }

    /// 改职位名（Crystal `GuildObject.ChangeRankName`，`GuildObject.cs:380-431`）。
    fn guild_change_rank_name(
        &mut self,
        player_id: u32,
        rank_index: u8,
        rank_name: &str,
    ) -> Result<(), GuildError> {
        let (guild_id, my_rank) = self.my_guild(player_id)?;
        let length = rank_name.chars().count();
        if length < GUILD_RANK_NAME_MIN_LEN {
            return Err(GuildError::RankNameTooShort);
        }
        if length > GUILD_RANK_NAME_MAX_LEN || rank_name.contains('\\') {
            return Err(GuildError::RankNameTooLong);
        }
        let guild = self
            .guilds
            .get(&guild_id)
            .ok_or(GuildError::NotInGuild(player_id))?;
        if !guild.has_rank(rank_index) {
            return Err(GuildError::RankNotFound(rank_index));
        }
        // `if (SelfRankIndex > RankIndex)`（`:395`）：允许改自己这一级及更低的职位名。
        if my_rank > rank_index {
            return Err(GuildError::RankNotAdequate);
        }
        let snapshot = {
            let guild = self
                .guilds
                .get_mut(&guild_id)
                .ok_or(GuildError::NotInGuild(player_id))?;
            if let Some(rank) = guild.ranks.get_mut(usize::from(rank_index)) {
                rank_name.clone_into(&mut rank.name);
            }
            guild.rank_snapshot(rank_index).unwrap_or_default()
        };
        let operator = self.players[&player_id].name.clone();
        let frame = member_change_frame(&operator, 7, rank_index, vec![snapshot]);
        self.send_to_guild(guild_id, &frame);
        self.mark_guild_dirty(guild_id);
        Ok(())
    }

    /// 新职位（Crystal `GuildObject.NewRank`，`GuildObject.cs:334-352`）。
    ///
    /// 插入位置照抄：`NewIndex = Ranks.Count > 1 ? Ranks.Count - 1 : 1`（插在最低职位之前）。
    fn guild_new_rank(&mut self, player_id: u32) -> Result<(), GuildError> {
        let (guild_id, _) = self.my_guild(player_id)?;
        let guild = self
            .guilds
            .get(&guild_id)
            .ok_or(GuildError::NotInGuild(player_id))?;
        if guild.ranks.len() >= GUILD_MAX_RANKS {
            return Err(GuildError::TooManyRanks);
        }
        let insert_at = if guild.ranks.len() > 1 {
            guild.ranks.len() - 1
        } else {
            1
        };
        let new_index = u8::try_from(insert_at).unwrap_or(1);
        let name = format!("{GUILD_NEW_RANK_NAME}{new_index}");
        let snapshot = {
            let guild = self
                .guilds
                .get_mut(&guild_id)
                .ok_or(GuildError::NotInGuild(player_id))?;
            guild.ranks.insert(
                insert_at,
                GuildRank {
                    name,
                    options: 0,
                    index: i32::from(new_index),
                    members: Vec::new(),
                },
            );
            // 插入后重排所有序号，并同步成员的职位序号（Crystal 只改最后一个职位的
            // `Index`，成员靠对象引用跟着走；本作存序号，必须一起搬）。
            for (position, rank) in guild.ranks.iter_mut().enumerate() {
                rank.index = i32::try_from(position).unwrap_or(0);
            }
            for member in guild.members.values_mut() {
                if usize::from(member.rank_index) >= insert_at {
                    member.rank_index = member.rank_index.saturating_add(1);
                }
            }
            guild.rank_snapshot(new_index).unwrap_or_default()
        };
        let operator = self.players[&player_id].name.clone();
        let frame = member_change_frame(&operator, 6, new_index, vec![snapshot]);
        self.send_to_guild(guild_id, &frame);
        // 序号整体后移：所有在线成员的 `Player.guild.rank_index` 同步。
        self.guild_resync_online_ranks(guild_id);
        self.mark_guild_dirty(guild_id);
        Ok(())
    }

    /// 改职位权限（Crystal `GuildObject.ChangeRankOption`，`GuildObject.cs:354-379`）。
    ///
    /// `option` 是 `GuildRankOptions` 的位序号 `0..=7`（`1 << option`）。
    /// **简化**：Crystal 关权限用的是 `Options ^= 1 << Option`（异或，位本来就是 0 时会
    /// **打开**权限，是源码 bug）；本作用「清零」语义。
    fn guild_change_rank_option(
        &mut self,
        player_id: u32,
        rank_index: u8,
        option: u8,
        enabled: &str,
    ) -> Result<(), GuildError> {
        let (guild_id, my_rank) = self.my_guild(player_id)?;
        let guild = self
            .guilds
            .get(&guild_id)
            .ok_or(GuildError::NotInGuild(player_id))?;
        if !guild.has_rank(rank_index) || option > 7 {
            return Err(GuildError::RankNotFound(rank_index));
        }
        // `if (Self.MyGuildRank.Index >= RankIndex)`（`:361`）：不能改自己这一级。
        if my_rank >= rank_index {
            return Err(GuildError::CannotChangeOwnRankOptions);
        }
        let on = match enabled {
            "true" => true,
            "false" => false,
            _ => return Ok(()),
        };
        let snapshot = {
            let guild = self
                .guilds
                .get_mut(&guild_id)
                .ok_or(GuildError::NotInGuild(player_id))?;
            if let Some(rank) = guild.ranks.get_mut(usize::from(rank_index)) {
                let bit = 1_u8 << option;
                if on {
                    rank.options |= bit;
                } else {
                    rank.options &= !bit;
                }
            }
            guild.rank_snapshot(rank_index).unwrap_or_default()
        };
        let operator = self.players[&player_id].name.clone();
        let frame = member_change_frame(&operator, 7, rank_index, vec![snapshot]);
        self.send_to_guild(guild_id, &frame);
        self.mark_guild_dirty(guild_id);
        Ok(())
    }

    /// 职位序号变化后，把在线成员的 `Player.guild.rank_index` 重新对齐。
    fn guild_resync_online_ranks(&mut self, guild_id: i32) {
        let Some(guild) = self.guilds.get(&guild_id) else {
            return;
        };
        let online: Vec<(u32, u8)> = guild
            .members
            .values()
            .filter_map(|member| {
                member
                    .online
                    .map(|object_id| (object_id, member.rank_index))
            })
            .collect();
        for (object_id, rank_index) in online {
            if let Some(player) = self.players.get_mut(&object_id) {
                if let Some(reference) = &mut player.guild {
                    if reference.guild_id == guild_id {
                        reference.rank_index = rank_index;
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// World：公告 / 信息
// ---------------------------------------------------------------------------

impl World {
    /// 改公告（`C.EditGuildNotice`，Crystal `PlayerObject.EditGuildNotice`，
    /// `PlayerObject.cs:9958-9977` + `GuildObject.NewNotice`，`:582-600`）。
    ///
    /// 包序：先给所有在线成员发 `S.GuildNoticeChange{update = -1}`（`:599`），
    /// 客户端收到后自己发 `C.RequestGuildInfo{Type=0}` 拉正文。
    pub fn guild_edit_notice(
        &mut self,
        player_id: u32,
        notice: Vec<String>,
    ) -> Result<(), GuildError> {
        let (guild_id, rank_index) = self.my_guild(player_id)?;
        let options = self
            .guilds
            .get(&guild_id)
            .and_then(|guild| guild.rank(rank_index))
            .map_or(0, |rank| rank.options);
        if !has_option(options, GuildRankOptions::CanChangeNotice) {
            let error = GuildError::Permission("CanChangeNotice");
            self.guild_error_to(player_id, &error);
            return Err(error);
        }
        if notice.len() > GUILD_NOTICE_MAX_LINES {
            let error = GuildError::NoticeTooLong;
            self.guild_error_to(player_id, &error);
            return Err(error);
        }
        if let Some(guild) = self.guilds.get_mut(&guild_id) {
            guild.notice = notice;
        }
        let frame = notice_frame(-1, Vec::new());
        self.send_to_guild(guild_id, &frame);
        self.mark_guild_dirty(guild_id);
        Ok(())
    }

    /// `C.RequestGuildInfo{Type}`（Crystal `PlayerObject.RequestGuildInfo`，`PlayerObject.cs:10012-10028`）。
    ///
    /// * `0` 公告：回 `S.GuildNoticeChange{notice}`；
    /// * `1` 成员表：回 `S.GuildMemberChange{Status=255, Ranks=全部职位}`。
    ///
    /// **简化**：Crystal 用 `GuildNoticeChanged` / `GuildMembersChanged` 两个标志做去重
    /// （`:10019`、`:10024`）；本作每次都回（幂等，客户端只在打开行会窗口时请求）。
    pub fn guild_request_info(&mut self, player_id: u32, type_: u8) -> Result<(), GuildError> {
        let (guild_id, rank_index) = self.my_guild(player_id)?;
        let guild = self
            .guilds
            .get(&guild_id)
            .ok_or(GuildError::NotInGuild(player_id))?;
        match type_ {
            0 => {
                let frame = notice_frame(
                    i32::try_from(guild.notice.len()).unwrap_or(0),
                    guild.notice.clone(),
                );
                self.send_to_player(player_id, &frame);
            }
            1 => {
                let snapshot = guild.ranks_snapshot();
                let name = self.players[&player_id].name.clone();
                let frame = member_change_frame(&name, 255, rank_index, snapshot);
                self.send_to_player(player_id, &frame);
            }
            _ => {}
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// World：聊天 / 经验
// ---------------------------------------------------------------------------

impl World {
    /// 行会聊天（`!~` 前缀，Crystal `PlayerObject.Chat`，`PlayerObject.cs:2014-2024`）。
    ///
    /// 只发给**在线成员**（`MyGuild.SendMessage` → `GuildObject.SendMessage`，
    /// `GuildObject.cs:113-126`），不走本地图广播。文本格式是 `"{角色名}: {正文}"`。
    /// 不在行会里时静默丢弃（`:2016` 的 `if (MyGuild == null) return;`）。
    ///
    /// 返回收到的人数（含自己）。
    pub fn guild_chat(&mut self, player_id: u32, message: &str) -> usize {
        let Some(player) = self.players.get(&player_id) else {
            return 0;
        };
        let name = player.name.clone();
        let Some(reference) = player.guild else {
            return 0;
        };
        let text = format!("{name}: {message}");
        let frame = ServerPacket::Chat(Chat {
            message: text,
            type_: ChatType::Guild,
        })
        .encode_frame();
        let members = self
            .guilds
            .get(&reference.guild_id)
            .map(Guild::online_members)
            .unwrap_or_default();
        for object_id in &members {
            self.send_to_player(*object_id, &frame);
        }
        members.len()
    }

    /// 成员获得经验时给行会加经验（Crystal `PlayerObject.GainExp`，`PlayerObject.cs:902-903`
    /// → `GuildObject.GainExp`，`GuildObject.cs:644-704`）。
    ///
    /// 流程照抄：
    ///
    /// 1. `MaxExperience <= 0` 就不加（`:647`）；
    /// 2. `expAmount = (uint)(amount * Guild_ExpRate)`，为 0 也不加（`:649-650`）；
    /// 3. `Experience += expAmount`（`:652`）；
    /// 4. `while (experience > MaxExperience)` 升级：`Level++`、加升级点、换新经验表
    ///    （`:656-674`）；升级后刷新人数上限并广播 `S.GuildStatus`（`:676-695`）；
    /// 5. 没升级时，每 [`GUILD_EXP_UPDATE_MS`] 广播一次 `S.GuildExpGain`（`:696-703`）。
    ///
    /// **本作修正的 Crystal 异常行为**：Crystal 的循环用的是局部变量 `experience`
    /// （`:654`），扣完不回写 `Info.Experience`，于是**每次加经验都会再升一级**。
    /// 本作把余数写回（见文档「Crystal 异常行为」）。
    ///
    /// `NewbieGuild` 不加经验（`:902`）。
    pub fn guild_gain_exp(&mut self, player_id: u32, amount: u32) {
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        let Some(reference) = player.guild else {
            return;
        };
        if amount == 0 {
            return;
        }
        let settings = self.guild_settings.clone();
        let now = self.now();
        let Some(guild) = self.guilds.get_mut(&reference.guild_id) else {
            return;
        };
        if guild.name == "NewbieGuild" {
            return;
        }
        if guild.max_experience <= 0 {
            return;
        }
        let exp_amount = (f64::from(amount) * settings.exp_rate).floor();
        if exp_amount < 1.0 {
            return;
        }
        let exp_amount = u32::try_from(exp_amount as u64).unwrap_or(u32::MAX);
        guild.experience = guild.experience.saturating_add(i64::from(exp_amount));

        let mut leveled = false;
        // `while (experience > Info.MaxExperience)`（`:656`，注意是严格大于）。
        while guild.max_experience > 0 && guild.experience > guild.max_experience {
            guild.level = guild.level.saturating_add(1);
            guild.spare_points = guild.spare_points.saturating_add(settings.point_per_level);
            guild.experience -= guild.max_experience;
            guild.max_experience = settings.max_experience(guild.level);
            leveled = true;
            if guild.max_experience == 0 || guild.level == u8::MAX {
                break;
            }
        }

        if leveled {
            guild.member_cap = settings.member_cap(guild.level);
            guild.next_exp_update = now + GUILD_EXP_UPDATE_MS;
            let guild_id = guild.id;
            self.send_guild_status_to_all(guild_id);
        } else if guild.next_exp_update < now {
            guild.next_exp_update = now + GUILD_EXP_UPDATE_MS;
            let guild_id = guild.id;
            let frame =
                ServerPacket::GuildExpGain(GuildExpGain { amount: exp_amount }).encode_frame();
            self.send_to_guild(guild_id, &frame);
        }
        self.mark_guild_dirty(reference.guild_id);
    }
}

impl Guild {
    /// 一张「你已经没有行会」的 `S.GuildStatus`（`GuildObject.cs:561`
    /// 只填了 `GuildName = ""`、`GuildRankName = ""`、`MyOptions = 0`，其余是结构体默认值）。
    #[must_use]
    pub fn placeholder_status() -> GuildStatus {
        GuildStatus::default()
    }
}

// 请求行会名字（NPC 面板 `[@CREATEGUILD]` 用）。
impl World {
    /// 发 `S.GuildNameRequest`（Crystal `NPCScript.cs:1058`），让客户端弹输入框。
    ///
    /// 对应 `NPCScript.ProcessSpecial` 的 `case GuildCreateKey`（`NPCScript.cs:1050-1062`）：
    /// 等级不够时只提示、不弹框；已在行会里也只提示。
    pub fn request_guild_name(&self, player_id: u32) -> Result<(), GuildError> {
        let Some(player) = self.players.get(&player_id) else {
            return Err(GuildError::NotInWorld(player_id));
        };
        let required = self.guild_settings.required_level;
        if player.level < required {
            return Err(GuildError::LevelTooLow {
                required,
                level: player.level,
            });
        }
        if player.guild.is_some() {
            return Err(GuildError::AlreadyInGuild);
        }
        self.send_to_player(
            player_id,
            &ServerPacket::GuildNameRequest(GuildNameRequest {}).encode_frame(),
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 落盘
// ---------------------------------------------------------------------------

/// 把 [`World::take_guild_writes`] 取出的队列写进存储层。
///
/// 世界层是同步的、存储层是异步的，所以两者之间的桥就是这条队列：会话在每次处理完
/// 行会包之后调用，服务端的 100ms tick 也会定期调用（行会经验可能来自怪物击杀）。
///
/// 单条写入失败只记 `warn!` 并继续下一条（一条坏行不应该拖垮整个行会系统的其它行），
/// 返回最近一次错误供调用方记日志。
pub async fn persist_guild_writes<S: crate::store::AccountStore>(
    store: &S,
    writes: Vec<crate::world::GuildWrite>,
) -> Result<(), crate::store::StoreError> {
    let mut last_error = None;
    for write in writes {
        let result = match write {
            crate::world::GuildWrite::Save(record) => {
                let expected = record.id;
                store.save_guild(*record).await.map(|id| {
                    if id != expected {
                        tracing::warn!(
                            expected,
                            actual = id,
                            "行会入库后主键与内存里的不一致（可能导致重登后行会丢失）"
                        );
                    }
                })
            }
            crate::world::GuildWrite::Delete { guild_id } => {
                match store.delete_guild(guild_id).await {
                    // 已经不存在就当删成功（重启后内存里可能残留已删除的行会 ID）。
                    Err(crate::store::StoreError::NotFound(_)) => Ok(()),
                    other => other,
                }
            }
            crate::world::GuildWrite::SetCharacterGuild {
                character_id,
                guild_id,
            } => store.set_character_guild(character_id, guild_id).await,
        };
        if let Err(error) = result {
            tracing::warn!(%error, "行会写库失败");
            last_error = Some(error);
        }
    }
    match last_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}
