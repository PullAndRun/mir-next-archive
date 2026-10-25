//! 多语言文本与语言包。
//!
//! 项目要求「默认全中文化，并能在游戏内切换其他语言」。这里分三层：
//!
//! 1. **编译期内置**：每个 [`TextKey`] 都有一份简体中文与一份英文模板（[`text`]），
//!    用穷举 `match` 保证任何语言都不会漏翻，也让默认值（简体中文）永远可用。
//! 2. **运行时语言包**：[`LanguageLibrary`] 从 `config/lang/*.json` 读取语言包，
//!    可以覆盖内置模板、补充地图名等内容文本，无需改代码重新编译。
//! 3. **会话级设置**：[`LanguageCatalog`] 保存每个玩家的当前语言，
//!    服务端在游戏内用 `/language <代码>` 切换（Crystal 协议没有对应包，
//!    这是我们自己的扩展，见 `docs/i18n.md`）。
//!
//! # 参数化文本
//!
//! 第 2 轮起 [`TextKey`] 覆盖服务端全部玩家可见提示（`docs/i18n.md` §7.2），
//! 其中大部分键带参数。约定：
//!
//! * 模板里的占位符写成 `{snake_case}`，例如 `"你以 {gold} 金币买下了 {item}。"`；
//! * 用 [`format_text`] 传入 `&[("gold", "100"), ("item", "炼狱")]` 得到成品文本；
//! * 没有参数的键直接 [`text`]（返回 `&'static str`，零分配）；
//! * 语言包里存的也是**模板**，[`LanguageLibrary::format`] 先取覆盖模板再渲染，
//!   与 [`format_text`] 同口径。
//!
//! 渲染规则（缺参保留、多参忽略、`{{`/`}}` 转义）见 [`format_text`] 文档。
//!
//! # 词表以外的两档（门下裁决 2）
//!
//! 「不是 UI 提示文本」的字符串按语义分成两档，都**不进** [`TextKey`] / 语言包：
//!
//! * **显示名词表**：[`MonsterTemplate`] / [`monster_display_name`] 给「数据库缺项时的
//!   模板怪物」内置中英显示名；数据名/匹配名仍由调用方保留（本作沿用 cjlaaa 的中文名），
//!   显示边界查不到词表时回退数据名。
//! * **匹配别名**：[`MatchAliases`] 从 `config/i18n-aliases.json` 加载职业名/怪物名的
//!   「DB 名 / 中文名 / 英文名」三列，只用于匹配（`GameShopItem.Class`、`AI 113` 的
//!   `ArcherGuard` 兜底），**不得**当渲染词表用。

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;

use serde::Deserialize;

/// 支持的语言。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Language {
    /// 简体中文（默认）。
    #[default]
    ZhCn,
    /// English.
    En,
}

impl Language {
    /// 全部语言。
    pub const ALL: &'static [Self] = &[Self::ZhCn, Self::En];

    /// 语言代码（BCP-47 的小写形式，指令里用它切换）。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::ZhCn => "zh-cn",
            Self::En => "en",
        }
    }

    /// 从代码解析（大小写不敏感，接受 `zh`/`zh-cn`/`cn`/`chs`/`en`/`en-us`）。
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code.trim().to_ascii_lowercase().as_str() {
            "zh" | "zh-cn" | "cn" | "zh_cn" | "chs" => Some(Self::ZhCn),
            "en" | "en-us" | "en_us" => Some(Self::En),
            _ => None,
        }
    }

    /// 语言的母语名（用于语言选择界面）。
    #[must_use]
    pub const fn native_name(self) -> &'static str {
        match self {
            Self::ZhCn => "简体中文",
            Self::En => "English",
        }
    }
}

/// 模板怪物（数据库缺项时的兜底种类）的**显示名**词表。
///
/// 门下裁决 2 要求「数据名 / 匹配名」与「显示名」分离：数据库与输入匹配继续用数据名
/// （`MonsterKind::name`，本作沿用 cjlaaa 的中文名），显示时另查词表；缺词表时回退数据名
/// （[`LanguageLibrary::monster_name`] 已有这个回退）。
///
/// 这 4 个模板怪物不在数据库里（示例沙盒或 `SummonSkeleton`/`SummonShinsu` 的兜底），
/// 没有 `MonsterInfo.Index` 可以查 `LanguageLibrary::monster_names`，所以内置一份中英显示名。
/// 它们不是 UI 提示文本（不进 [`TextKey`]），也不是匹配别名（不进
/// `config/i18n-aliases.json`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MonsterTemplate {
    /// 示例怪物：骷髅（`MonsterKind::skeleton`）。
    Skeleton,
    /// 示例怪物：稻草人（`MonsterKind::scarecrow`）。
    Scarecrow,
    /// `SummonSkeleton` 的兜底（Crystal `Settings.SkeletonName = "BoneFamiliar"`）。
    BoneFamiliar,
    /// `SummonShinsu` 的兜底（Crystal `Settings.ShinsuName = "Shinsu"`）。
    Shinsu,
}

impl MonsterTemplate {
    /// 全部模板（测试与校验用）。
    pub const ALL: &'static [Self] = &[
        Self::Skeleton,
        Self::Scarecrow,
        Self::BoneFamiliar,
        Self::Shinsu,
    ];

    /// 稳定键名（ASCII，日志/配置对照用）。
    ///
    /// 刻意不命名为 `key()`：`tools/i18n/verify_text_keys.py` 会搜索 `TextKey` 那段
    /// `pub const fn key` 前缀来定位中英键表，文件里更靠前的同名方法会让它解析错表，
    /// 所以这里用 `stable_key()`。
    #[must_use]
    pub const fn stable_key(self) -> &'static str {
        match self {
            Self::Skeleton => "skeleton",
            Self::Scarecrow => "scarecrow",
            Self::BoneFamiliar => "bone_familiar",
            Self::Shinsu => "shinsu",
        }
    }

    /// 从稳定键名解析。
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|template| template.stable_key() == key)
    }
}

/// 取模板怪物的显示名（内置中英两份）。
///
/// 中文名同时是这些兜底种类的**数据名**（与 cjlaaa `MonsterInfo.Name` 一致，
/// `World::monster_name_for` 按数据名查数据库）；英文名只用于显示。
#[must_use]
pub const fn monster_display_name(language: Language, template: MonsterTemplate) -> &'static str {
    match (language, template) {
        (Language::ZhCn, MonsterTemplate::Skeleton | MonsterTemplate::BoneFamiliar) => "骷髅",
        (Language::ZhCn, MonsterTemplate::Scarecrow) => "稻草人",
        (Language::ZhCn, MonsterTemplate::Shinsu) => "神兽",
        (Language::En, MonsterTemplate::Skeleton) => "Skeleton",
        (Language::En, MonsterTemplate::Scarecrow) => "Scarecrow",
        (Language::En, MonsterTemplate::BoneFamiliar) => "Bone Familiar",
        (Language::En, MonsterTemplate::Shinsu) => "Shinsu",
    }
}

/// 玩家可见文本键。
///
/// * 变体名是 [`TextKey::key`]（`config/lang/*.json` 里 `server` 段的键名）的 CamelCase；
/// * 带参数的键用 [`TextKey::args`] 查参数名，再用 [`format_text`] 渲染；
/// * 第 1 轮的 11 个键保持无参、文案不变（见 `round_one_keys_keep_their_wording`）；
/// * **同一句文案在不同场景保留独立键**（如 `conquest_not_in_guild` 与 `guild_not_in_guild`、
///   `conquest_not_in_world` 与 `guild_not_in_world`、`gameshop_not_in_world` 与
///   `mail_not_in_world`、`gameshop_wrong_level` 与 `item_level_too_low`）：这是按
///   `docs/i18n.md` §7.2 的「建议 TextKey」1:1 保留的，允许以后按场景分别调整文案，
///   不是漏合并。
///
/// 新增键的步骤：① 在枚举里加变体（穷举 `match` 会强制补中英模板）；
/// ② 登记 [`TextKey::ALL`]；③ 在 `config/lang/zh-cn.json` 与 `config/lang/en.json`
/// 的 `server` 段补同一份模板；④ 跑 `cargo test -p mir-core`
/// （两份语言包的键集合与内置模板必须完全一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextKey {
    // ---- 第 1 轮：11 个无参键（文案冻结，防回归）----
    /// 进入世界时的欢迎语。
    Welcome,
    /// 新角色进游戏的第一条新手提示（出生装 + 书店学技能）。
    NewbieGuide,
    /// 账号被封禁。
    AccountBanned,
    /// 口令错误。
    WrongPassword,
    /// 角色过多。
    TooManyCharacters,
    /// 语言切换成功。
    LanguageChanged,
    /// 未知的语言代码。
    UnknownLanguage,
    /// 服务器人数已满。
    ServerFull,
    /// 不支持的操作。
    UnsupportedCommand,
    /// 角色名已被占用。
    CharacterNameTaken,
    /// 角色名不合法。
    CharacterNameInvalid,
    /// 登录成功。
    LoginSuccess,
    // ---- 第 2 轮：`docs/i18n.md` §7.2 的 185 个键（按源文件分组，与文档同序）----
    // 另有 4 个 `guild_not_allowed_*` 扩展键（不在 §7.2 内，见各自文档注释），
    // 用于替换 `GuildError::Permission` 里会泄漏给玩家的内部标识符。
    /// 场景：`ConquestError::NotInWorld`（`conquest.rs:269`）；模板：`对象 {object_id} 不在世界里`；参数：`{object_id}`。
    ConquestNotInWorld,
    /// 场景：`ConquestError::NotInGuild`（`conquest.rs:270`）；模板：`你不在行会中`；参数：无。
    ConquestNotInGuild,
    /// 场景：`ConquestError::NotLeader`（`conquest.rs:271`）；模板：`只有会长才能申请攻城`；参数：无。
    ConquestNotLeader,
    /// 场景：`ConquestError::NotFound`（`conquest.rs:272`）；模板：`找不到攻城区域 {area}`；参数：`{area}`。
    ConquestAreaNotFound,
    /// 场景：`ConquestError::AlreadyOwner`（`conquest.rs:273`）；模板：`你的行会已经是这座城的城主`；参数：无。
    ConquestAlreadyOwner,
    /// 场景：`ConquestError::AtWar`（`conquest.rs:274`）；模板：`这座城正在开战，无法申请`；参数：无。
    ConquestAtWar,
    /// 场景：`ConquestError::AlreadyScheduled`（`conquest.rs:275`）；模板：`已经有一个行会申请了这座城的攻城`；参数：无。
    ConquestAlreadyScheduled,
    /// 场景：`ConquestError::LevelTooLow`（`conquest.rs:276`）；模板：`申请攻城需要行会等级 {required}（当前 {level}）`；参数：`{required}`、`{level}`。
    ConquestLevelTooLow,
    /// 场景：`ConquestError::NotEnoughGold`（`conquest.rs:277`）；模板：`申请攻城需要 {required} 金币（行会现有 {gold}）`；参数：`{required}`、`{gold}`。
    ConquestNotEnoughGold,
    /// 场景：`ConquestError::NotOwnerGuild`（`conquest.rs:278`）；模板：`只有城主行会才能执行这个操作`；参数：无。
    ConquestNotOwnerGuild,
    /// 场景：`start_war 全服公告`（`conquest.rs:279`）；模板：`【攻城】{name} 攻城战已经开始！`；参数：`{name}`。
    ConquestWarStarted,
    /// 场景：`end_war 全服公告`（`conquest.rs:280`）；模板：`【攻城】{name} 攻城战已经结束。`；参数：`{name}`。
    ConquestWarEnded,
    /// 场景：`控制点公告里的行会名占位`（`conquest.rs:281`）；模板：`（无行会）`；参数：无。
    ConquestNoGuild,
    /// 场景：`score_control_points 全服公告`（`conquest.rs:282`）；模板：`【攻城】{guild} 占领了控制点 {point}。`；参数：`{guild}`、`{point}`。
    ConquestPointCaptured,
    /// 场景：`take_conquest 全服公告`（`conquest.rs:283`）；模板：`【攻城】{conquest} 的新城主是 {guild}。`；参数：`{conquest}`、`{guild}`。
    ConquestNewOwner,
    /// 场景：`group_invite`（`group.rs:289`）；模板：`你不是队长，不能邀请队员。`；参数：无。
    GroupInviteNotLeader,
    /// 场景：`group_invite`（`group.rs:290`）；模板：`你的队伍人数已满。`；参数：无。
    GroupInviteFull,
    /// 场景：`group_invite`（`group.rs:291`）；模板：`当前地图禁止组队，无法邀请。`；参数：无。
    GroupInviteMapForbidden,
    /// 场景：`group_invite`（`group.rs:292`）；模板：`找不到玩家 {name}。`；参数：`{name}`。
    GroupInviteTargetMissing,
    /// 场景：`group_invite`（`group.rs:293`）；模板：`不能邀请自己加入队伍。`；参数：无。
    GroupInviteSelf,
    /// 场景：`group_invite`（`group.rs:294`）；模板：`{target_name} 不允许组队。`；参数：`{target_name}`。
    GroupInviteTargetDisallowed,
    /// 场景：`group_invite`（`group.rs:295`）；模板：`{target_name} 已经在队伍中。`；参数：`{target_name}`。
    GroupInviteTargetInGroup,
    /// 场景：`group_invite`（`group.rs:296`）；模板：`{target_name} 正在接收其他人的组队邀请。`；参数：`{target_name}`。
    GroupInviteTargetBusy,
    /// 场景：`group_invite`（`group.rs:297`）；模板：`{target_name} 所在的地图禁止组队。`；参数：`{target_name}`。
    GroupInviteTargetMapForbidden,
    /// 场景：`group_invite`（`group.rs:298`）；模板：`当前地图禁止组队，无法接受邀请。`；参数：无。
    GroupJoinMapForbidden,
    /// 场景：`group_join`（`group.rs:299`）；模板：`你没有收到组队邀请。`；参数：无。
    GroupJoinNoInvite,
    /// 场景：`group_join`（`group.rs:300`）；模板：`{invitee_name} 拒绝了你的组队邀请。`；参数：`{invitee_name}`。
    GroupJoinDeclined,
    /// 场景：`group_join`（`group.rs:301`）；模板：`组队邀请已过期。`；参数：无。
    GroupJoinExpired,
    /// 场景：`group_join`（`group.rs:302`）；模板：`你已经在队伍中，无法加入 {inviter_name} 的队伍。`；参数：`{inviter_name}`。
    GroupJoinAlreadyInGroup,
    /// 场景：`group_join`（`group.rs:303`）；模板：`邀请者已经下线。`；参数：无。
    GroupJoinInviterOffline,
    /// 场景：`group_join`（`group.rs:304`）；模板：`{inviter_name} 已经不是队长了。`；参数：`{inviter_name}`。
    GroupJoinInviterNotLeader,
    /// 场景：`group_join`（`group.rs:305`）；模板：`{inviter_name} 的队伍人数已满。`；参数：`{inviter_name}`。
    GroupJoinInviterFull,
    /// 场景：`group_join`（`group.rs:306`）；模板：`{inviter_name} 不再允许组队。`；参数：`{inviter_name}`。
    GroupJoinInviterDisallowed,
    /// 场景：`group_join`（`group.rs:307`）；模板：`{inviter_name} 在地图禁止组队的区域。`；参数：`{inviter_name}`。
    GroupJoinInviterMapForbidden,
    /// 场景：`group_join`（`group.rs:308`）；模板：`当前地图禁止组队。`；参数：无。
    GroupMapForbidden,
    /// 场景：`group_kick`（`group.rs:309`）；模板：`你不在队伍中。`；参数：无。
    GroupKickNotInGroup,
    /// 场景：`group_kick`（`group.rs:310`）；模板：`你不是队长，不能移除队员。`；参数：无。
    GroupKickNotLeader,
    /// 场景：`group_kick`（`group.rs:311`）；模板：`{name} 不在你的队伍中。`；参数：`{name}`。
    GroupKickNotMember,
    /// 场景：`GuildError::NotInWorld`（`guild.rs:317`）；模板：`对象 {object_id} 不在世界里`；参数：`{object_id}`。
    GuildNotInWorld,
    /// 场景：`GuildError::NotInGuild`（`guild.rs:318`）；模板：`你不在行会中`；参数：无。
    GuildNotInGuild,
    /// 场景：`GuildError::AlreadyInGuild`（`guild.rs:319`）；模板：`你已经在行会中`；参数：无。
    GuildAlreadyInGuild,
    /// 场景：`GuildError::NameTaken`（`guild.rs:320`）；模板：`行会 {name} 已经存在`；参数：`{name}`。
    GuildNameTaken,
    /// 场景：`GuildError::NameLength`（`guild.rs:321`）；模板：`行会名长度必须是 {min}-{max} 个字符`；参数：`{min}`、`{max}`。
    GuildNameLength,
    /// 场景：`GuildError::NameInvalid`（`guild.rs:322`）；模板：`行会名不能包含反斜杠或换行`；参数：无。
    GuildNameInvalid,
    /// 场景：`GuildError::LevelTooLow`（`guild.rs:323`）；模板：`创建行会需要 {required} 级（你当前 {level} 级）`；参数：`{required}`、`{level}`。
    GuildCreateLevelTooLow,
    /// 场景：`GuildError::NotEnoughGold`（`guild.rs:324`）；模板：`创建行会需要 {required} 金币（你只有 {gold}）`；参数：`{required}`、`{gold}`。
    GuildCreateNotEnoughGold,
    /// 场景：`GuildError::Permission`（`guild.rs:325`）；模板：`你的职位没有 {permission} 权限`；参数：`{permission}`。
    ///
    /// `{permission}` 必须是**本地化后的权限名**（如「招收成员」/"recruit members"），
    /// 不能直接塞内部标识符（`CanRecruit` 等）——用 [`guild_permission_name`] 转换。
    GuildNoPermission,
    /// 扩展键：职位没有招收成员的权限（Crystal `ServerTextKeys.NotAllowedRecruitMembers`）。
    ///
    /// 无参；`guild.rs` 的 `CanRecruit` 分支用它。`guild_no_permission` 保留作未知权限兜底。
    GuildNotAllowedRecruit,
    /// 扩展键：职位没有踢出成员的权限（Crystal `ServerTextKeys.NotAllowedKickMembers`）。无参。
    GuildNotAllowedKick,
    /// 扩展键：职位没有修改成员职位的权限（Crystal `ServerTextKeys.NotAllowedChangeRank`）。无参。
    GuildNotAllowedChangeRank,
    /// 扩展键：职位没有修改行会公告的权限（Crystal `ServerTextKeys.NotAllowedChangeNotice`）。无参。
    GuildNotAllowedChangeNotice,
    /// 场景：`GuildError::MemberNotFound`（`guild.rs:326`）；模板：`行会里没有成员 {name}`；参数：`{name}`。
    GuildMemberNotFound,
    /// 场景：`GuildError::TargetAlreadyInGuild`（`guild.rs:327`）；模板：`{name} 已经在行会中`；参数：`{name}`。
    GuildTargetAlreadyInGuild,
    /// 场景：`GuildError::TargetAlreadyInvited`（`guild.rs:328`）；模板：`{name} 已经收到一个行会邀请`；参数：`{name}`。
    GuildTargetAlreadyInvited,
    /// 场景：`GuildError::CannotInviteSelf`（`guild.rs:329`）；模板：`不能邀请自己`；参数：无。
    GuildCannotInviteSelf,
    /// 场景：`GuildError::GuildFull`（`guild.rs:330`）；模板：`行会人数已满`；参数：无。
    GuildFull,
    /// 场景：`GuildError::NoInvitation`（`guild.rs:331`）；模板：`没有待接受的行会邀请`；参数：无。
    GuildNoInvitation,
    /// 场景：`GuildError::LeaderLevelTooLow`（`guild.rs:332`）；模板：`对方等级不足以成为会长（需要 {required} 级）`；参数：`{required}`。
    GuildLeaderLevelTooLow,
    /// 场景：`GuildError::GuildNeedsTwoLeaders`（`guild.rs:333`）；模板：`行会至少要有两个会长才能降职`；参数：无。
    GuildNeedsTwoLeaders,
    /// 场景：`GuildError::NeedOneLeaderOnline`（`guild.rs:334`）；模板：`需要至少一个会长在线`；参数：无。
    GuildNeedOneLeaderOnline,
    /// 场景：`GuildError::RankNotFound`（`guild.rs:335`）；模板：`职位 {rank} 不存在`；参数：`{rank}`。
    ///
    /// `{rank}` 是内部职位序号（数字），不是职位名。
    GuildRankNotFound,
    /// 场景：`GuildError::RankNameTooShort`（`guild.rs:336`）；模板：`职位名至少 {min} 个字符`；参数：`{min}`。
    GuildRankNameTooShort,
    /// 场景：`GuildError::RankNameTooLong`（`guild.rs:337`）；模板：`职位名最多 {max} 个字符`；参数：`{max}`。
    GuildRankNameTooLong,
    /// 场景：`GuildError::CannotChangeOwnRankOptions`（`guild.rs:338`）；模板：`不能修改自己职位的权限`；参数：无。
    GuildCannotChangeOwnRank,
    /// 场景：`GuildError::TooManyRanks`（`guild.rs:339`）；模板：`职位数量已达上限`；参数：无。
    GuildTooManyRanks,
    /// 场景：`GuildError::RankNotAdequate`（`guild.rs:340`）；模板：`你的职位不够高`；参数：无。
    GuildRankNotAdequate,
    /// 场景：`GuildError::NoticeTooLong`（`guild.rs:341`）；模板：`公告最多 {lines} 行`；参数：`{lines}`。
    GuildNoticeTooLong,
    /// 场景：`GuildError::NotInSafeZone`（`guild.rs:342`）；模板：`只能在安全区内使用行会仓库`；参数：无。
    GuildStorageOutsideSafezone,
    /// 场景：`GuildError::NoStorePermission`（`guild.rs:343`）；模板：`你的职位没有往行会仓库存物品的权限`；参数：无。
    GuildStorageNoStorePermission,
    /// 场景：`GuildError::NoRetrievePermission`（`guild.rs:344`）；模板：`你的职位没有从行会仓库取物品的权限`；参数：无。
    GuildStorageNoRetrievePermission,
    /// 场景：`GuildError::NoStorageMovePermission`（`guild.rs:345`）；模板：`你的职位没有整理行会仓库的权限`；参数：无。
    GuildStorageNoMovePermission,
    /// 场景：`GuildError::InvalidStorageSlot`（`guild.rs:346`）；模板：`格子序号不在范围内`；参数：无。
    GuildStorageInvalidSlot,
    /// 场景：`GuildError::EmptySlot`（`guild.rs:347`）；模板：`格子里没有物品`；参数：无。
    GuildStorageEmptySlot,
    /// 场景：`GuildError::TargetSlotNotEmpty`（`guild.rs:348`）；模板：`目标格子不是空的`；参数：无。
    GuildStorageTargetSlotNotEmpty,
    /// 场景：`GuildError::StorageRequestThrottled`（`guild.rs:349`）；模板：`仓库列表请求过于频繁`；参数：无。
    GuildStorageThrottled,
    /// 场景：`GuildError::ItemCannotBeStored`（`guild.rs:350`）；模板：`这件物品不能存入行会仓库`；参数：无。
    GuildStorageItemCannotStore,
    /// 场景：`GuildError::GuildGoldLimitReached`（`guild.rs:351`）；模板：`行会资金已达上限`；参数：无。
    GuildGoldLimitReached,
    /// 场景：`GuildError::PlayerGoldLimitReached`（`guild.rs:352`）；模板：`你的金币已达上限`；参数：无。
    GuildPlayerGoldLimitReached,
    /// 场景：`GuildError::GuildBankNotEnough`（`guild.rs:353`）；模板：`行会资金不足（需要 {required}，现有 {gold}）`；参数：`{required}`、`{gold}`。
    GuildBankNotEnoughGold,
    /// 场景：`GuildError::LeaderOnly`（`guild.rs:354`）；模板：`只有会长可以从行会仓库取金币`；参数：无。
    GuildBankLeaderOnly,
    /// 场景：`GuildError::NoBuffPermission`（`guild.rs:355`）；模板：`你的职位没有激活行会增益的权限`；参数：无。
    GuildBuffNoPermission,
    /// 场景：`GuildError::BuffNotExist`（`guild.rs:356`）；模板：`行会增益 {buff} 不存在`；参数：`{buff}`。
    ///
    /// `{buff}` 是内部增益编号。
    GuildBuffNotExist,
    /// 场景：`GuildError::BuffAlreadyObtained`（`guild.rs:357`）；模板：`行会已经拥有增益 {buff}`；参数：`{buff}`。
    GuildBuffAlreadyObtained,
    /// 场景：`GuildError::BuffNotObtained`（`guild.rs:358`）；模板：`行会还没有购买增益 {buff}`；参数：`{buff}`。
    GuildBuffNotObtained,
    /// 场景：`GuildError::GuildLevelTooLow`（`guild.rs:359`）；模板：`行会等级不足（需要 {required} 级，当前 {level} 级）`；参数：`{required}`、`{level}`。
    GuildLevelTooLow,
    /// 场景：`GuildError::SparePointsInsufficient`（`guild.rs:360`）；模板：`行会升级点不足（需要 {required} 点，现有 {remaining} 点）`；参数：`{required}`、`{remaining}`。
    GuildPointsInsufficient,
    /// 场景：`GuildError::LastLeaderCannotLeave`（`guild.rs:361`）；模板：`你是最后一个会长：请先转让会长或解散行会`；参数：无。
    GuildLastLeaderCannotLeave,
    /// 场景：`guild_bind_player`（`guild.rs:362`）；模板：`你已经被移出行会。`；参数：无。
    GuildRemoved,
    /// 场景：`guild_kick_or_leave`（`guild.rs:363`）；模板：`你已经解散了行会。`；参数：无。
    GuildDisbanded,
    /// 场景：`guild_remove_member`（`guild.rs:364`）；模板：`你离开了行会。`；参数：无。
    GuildLeft,
    /// 场景：`guild_remove_member`（`guild.rs:365`）；模板：`你被移出了行会。`；参数：无。
    GuildKicked,
    /// 场景：`guild_buff_buy 行会频道`（`guild_buffs.rs:371`）；模板：`「{player}」花费 {points} 升级点与 {gold} 金币为行会购买了增益「{buff}」。`；参数：`{player}`、`{points}`、`{gold}`、`{buff}`。
    GuildBuffBoughtPointsGold,
    /// 场景：`guild_buff_buy 行会频道`（`guild_buffs.rs:372`）；模板：`「{player}」花费 {gold} 金币为行会购买了增益「{buff}」。`；参数：`{player}`、`{gold}`、`{buff}`。
    GuildBuffBoughtGold,
    /// 场景：`guild_buff_buy 行会频道`（`guild_buffs.rs:373`）；模板：`「{player}」花费 {points} 升级点为行会购买了增益「{buff}」。`；参数：`{player}`、`{points}`、`{buff}`。
    GuildBuffBoughtPoints,
    /// 场景：`guild_buff_buy 行会频道`（`guild_buffs.rs:374`）；模板：`「{player}」为行会购买了增益「{buff}」。`；参数：`{player}`、`{buff}`。
    GuildBuffBoughtFree,
    /// 场景：`S.ObjectItem.name 兜底`（`items.rs:380`）；模板：`<未知物品 {item_index}>`；参数：`{item_index}`。
    ItemUnknownPlaceholder,
    /// 场景：`use_item`（`items.rs:381`）；模板：`「{item}」的效果尚未实现，物品没有被消耗。`；参数：`{item}`。
    ItemEffectNotImplemented,
    /// 场景：`use_item`（`items.rs:382`）；模板：`本作还不能骑乘，食物暂时用不上。`；参数：无。
    ItemMountNotImplemented,
    /// 场景：`use_potion`（`items.rs:383`）；模板：`生命和魔法都是满的，药水没有效果。`；参数：无。
    ItemPotionNoEffectFull,
    /// 场景：`use_potion`（`items.rs:385`）；模板：`你已经可以卸下诅咒装备了。`；参数：无。
    ItemCurseLifted,
    /// 场景：`use_potion`（`items.rs:386`）；模板：`你现在可以卸下诅咒装备了。`；参数：无。
    ItemCurseLiftedNow,
    /// 场景：`can_use_requirements`（`items.rs:387`）；模板：`你的性别无法使用这件物品。`；参数：无。
    ItemWrongGender,
    /// 场景：`can_use_requirements`（`items.rs:388`）；模板：`你的职业无法使用这件物品。`；参数：无。
    ItemWrongClass,
    /// 场景：`can_use_requirements`（`items.rs:389`）；模板：`等级不够：需要 {required} 级。`；参数：`{required}`。
    ItemLevelTooLow,
    /// 场景：`can_use_requirements`（`items.rs:390`）；模板：`等级过高：最多 {max} 级可用。`；参数：`{max}`。
    ItemLevelTooHigh,
    /// 场景：`can_use_requirements`（`items.rs:391`）；模板：`你还没有死亡，不能使用复活卷轴。`；参数：无。
    ItemReviveNotDead,
    /// 场景：`use_book`（`magic.rs:397`）；模板：`你已经学会了「{spell}」，不需要再学一次。`；参数：`{spell}`。
    MagicAlreadyLearned,
    /// 场景：`create_fire_wall 系统聊天`（`magic.rs:398`）；模板：`火墙数量已达上限，请稍后再试。`；参数：无。
    MagicFirewallLimit,
    /// 场景：`teleport_caster 系统聊天`（`magic.rs:399`）；模板：`这张地图不能使用瞬息移动。`；参数：无。
    MagicTeleportForbidden,
    /// 场景：`summon_pet 系统聊天`（`magic.rs:400`）；模板：`这张地图不能召唤宠物。`；参数：无。
    MagicSummonForbidden,
    /// 场景：`cast_shoulder_dash 系统聊天`（冲撞一格未走且施法者站在安全区；
    /// Crystal `HumanObject.cs:5140`）；模板：`安全区内无法冲撞。`；参数：无。
    NoPushingInSafezone,
    /// 场景：`cast_shoulder_dash 系统聊天`（冲撞一格未走；Crystal
    /// `HumanObject.cs:5142`）；模板：`冲撞力量不足。`；参数：无。
    NotEnoughPushingPower,
    /// 场景：`MailError::message`（`mail.rs:406`）；模板：`你不在游戏世界里。`；参数：无。
    MailNotInWorld,
    /// 场景：`MailError::message`（`mail.rs:407`）；模板：`邮件正文不能超过 {max} 字。`；参数：`{max}`。
    MailTextTooLong,
    /// 场景：`MailError::message`（`mail.rs:408`）；模板：`找不到该玩家。`；参数：无。
    MailRecipientMissing,
    /// 场景：`MailError::message`（`mail.rs:409`）；模板：`收件人的邮箱已满。`；参数：无。
    MailMailboxFull,
    /// 场景：`MailError::message`（`mail.rs:410`）；模板：`金币不足：需要 {needed}，你只有 {held}。`；参数：`{needed}`、`{held}`。
    MailNotEnoughGold,
    /// 场景：`MailError::message`（`mail.rs:411`）；模板：`这件物品不能邮寄。`；参数：无。
    MailItemCannotMail,
    /// 场景：`MailError::message`（`mail.rs:412`）；模板：`找不到这封邮件。`；参数：无。
    MailNotFound,
    /// 场景：`MailError::message`（`mail.rs:413`）；模板：`请先到邮局受理包裹。`；参数：无。
    MailNotCollected,
    /// 场景：`MailError::message`（`mail.rs:414`）；模板：`背包已满，无法领取附件。`；参数：无。
    MailInventoryFull,
    /// 场景：`MailError::message`（`mail.rs:415`）；模板：`邮件已锁定，不能删除。`；参数：无。
    MailLocked,
    /// 场景：`MailError::message`（`mail.rs:416`）；模板：`附件还没领取，不能删除。`；参数：无。
    MailUncollectedAttachments,
    /// 场景：`MailError::message`（`mail.rs:417`）；模板：`邮件附件数据损坏。`；参数：无。
    MailAttachmentCorrupted,
    /// 场景：`process_new_mail`（`mail.rs:418`）；模板：`你有新邮件。`；参数：无。
    MailNewMail,
    /// 场景：`商城邮件发件人（Crystal "Gameshop"）`（`mail.rs:419`）；模板：`商城`；参数：无。
    MailGameshopSender,
    /// 场景：`商城邮件正文`（`mail.rs:420`）；模板：`感谢您购买商城商品。`；参数：无。
    MailGameshopBody,
    /// 场景：`系统邮件发件人（Crystal "Bichon Administrator"）`（`mail.rs:421`）；模板：`比奇管理员`；参数：无。
    MailSystemSender,
    /// 场景：`交易背包满转邮件正文`（`mail.rs:422`）；模板：`由于以下原因，物品已通过邮件寄给你：\r\n\r\n交易结束后无法放回背包。`；参数：无。
    MailTradeItemReturned,
    /// 场景：`GameShopError::message`（`market.rs:428`）；模板：`你不在游戏世界里。`；参数：无。
    GameshopNotInWorld,
    /// 场景：`GameShopError::message`（`market.rs:429`）；模板：`商城暂未开放。`；参数：无。
    GameshopUnavailable,
    /// 场景：`GameShopError::message`（`market.rs:430`）；模板：`商城里没有这件商品。`；参数：无。
    GameshopProductNotFound,
    /// 场景：`GameShopError::message`（`market.rs:431`）；模板：`购买数量不对。`；参数：无。
    GameshopInvalidQuantity,
    /// 场景：`GameShopError::message`（`market.rs:432`）；模板：`一次最多买 {max} 组物品（这次要 {stacks} 组）。`；参数：`{max}`、`{stacks}`。
    GameshopTooManyStacks,
    /// 场景：`GameShopError::message`（`market.rs:433`）；模板：`你的职业不能购买这件商品。`；参数：无。
    GameshopWrongClass,
    /// 场景：`GameShopError::message`（`market.rs:434`）；模板：`等级不够：需要 {required} 级。`；参数：`{required}`。
    GameshopWrongLevel,
    /// 场景：`GameShopError::message`（`market.rs:435`）；模板：`库存不足。`；参数：无。
    GameshopOutOfStock,
    /// 场景：`GameShopError::message`（`market.rs:436`）；模板：`这件商品不能用这种方式购买。`；参数：无。
    GameshopPaymentNotAllowed,
    /// 场景：`GameShopError::message`（`market.rs:437`）；模板：`元宝不足。`；参数：无。
    GameshopNotEnoughCredit,
    /// 场景：`GameShopError::message`（`market.rs:438`）；模板：`金币不足。`；参数：无。
    GameshopNotEnoughGold,
    /// 场景：`GameShopError::message`（`market.rs:439`）；模板：`不支持的支付方式。`；参数：无。
    GameshopUnsupportedPayment,
    /// 场景：`GameShopError::message`（`market.rs:440`）；模板：`背包放不下。`；参数：无。
    GameshopCannotGain,
    /// 场景：`ClientAuction.seller`（`market.rs:441`）；模板：`已售出`；参数：无。
    MarketLabelSold,
    /// 场景：`ClientAuction.seller`（`market.rs:442`）；模板：`已过期`；参数：无。
    MarketLabelExpired,
    /// 场景：`ClientAuction.seller`（`market.rs:443`）；模板：`出售中`；参数：无。
    ///
    /// 英文与客户端 `Label::OnSale` 一致用 "On sale"（Crystal 原版用 "For Sale"）；
    /// 若以后怀疑「打折」歧义，需同时改客户端 label 与本键。
    MarketLabelOnSale,
    /// 场景：`ClientAuction.seller`（`market.rs:446`）；模板：`已有出价`；参数：无。
    MarketLabelHasBid,
    /// 场景：`ClientAuction.seller`（`market.rs:447`）；模板：`无人出价`；参数：无。
    MarketLabelNoBid,
    /// 场景：`S.MarketSuccess/邮件`（`market.rs:448`）；模板：`你在 {item} 上的出价已退回：拍卖已过期（{gold} 金币）。`；参数：`{item}`、`{gold}`。
    MarketMailBidRefundExpired,
    /// 场景：`系统邮件`（`market.rs:449`）；模板：`你挂出的 {item} 已过期，物品已通过邮件退回。`；参数：`{item}`。
    MarketMailItemExpired,
    /// 场景：`系统邮件`（`market.rs:450`）；模板：`你挂出的 {item} 已售出，收入 {gold} 金币。`；参数：`{item}`、`{gold}`。
    MarketMailSold,
    /// 场景：`S.MarketSuccess`（`market.rs:451`）；模板：`你以 {gold} 金币买下了 {item}。`；参数：`{gold}`、`{item}`。
    MarketBought,
    /// 场景：`系统邮件`（`market.rs:452`）；模板：`你在 {item} 上的出价被超过，{gold} 金币已通过邮件退回。`；参数：`{item}`、`{gold}`。
    MarketMailOutbidRefund,
    /// 场景：`系统邮件`（`market.rs:453`）；模板：`有人为 {item} 出价 {bid} 金币。`；参数：`{item}`、`{bid}`。
    MarketMailNewBid,
    /// 场景：`S.MarketSuccess`（`market.rs:454`）；模板：`你为 {item} 出价 {bid} 金币。`；参数：`{item}`、`{bid}`。
    MarketBidPlaced,
    /// 场景：`系统邮件`（`market.rs:455`）；模板：`你在拍卖中赢得了 {item}，成交价 {cost} 金币。`；参数：`{item}`、`{cost}`。
    MarketMailAuctionWon,
    /// 场景：`S.MarketSuccess`（`market.rs:456`）；模板：`你以 {cost} 金币卖出了 {item}，扣除佣金后收入 {gold} 金币。`；参数：`{cost}`、`{item}`、`{gold}`。
    MarketSoldNow,
    /// 场景：`系统邮件`（`market.rs:457`）；模板：`你挂出的 {item} 已被卖家下架，{gold} 金币已通过邮件退回。`；参数：`{item}`、`{gold}`。
    MarketMailItemWithdrawn,
    /// 场景：`S.MarketSuccess`（`market.rs:458`）；模板：`你卖出了 {item}，成交价 {cost}，扣除佣金后收入 {gold} 金币。`；参数：`{item}`、`{cost}`、`{gold}`。
    MarketMailSoldCommission,
    /// 场景：`gameshop_buy 系统聊天`（`market.rs:459`）；模板：`背包放不下的 {count} 份物品已寄到邮箱，请到邮局领取。`；参数：`{count}`。
    GameshopMailedItems,
    /// 场景：`run_npc_action`（`npc.rs:465`）；模板：`行会 {name} 创建成功。`；参数：`{name}`。
    NpcGuildCreated,
    /// 场景：`run_map_action`（`npc.rs:466`）；模板：`城里没有可收取的金币。`；参数：无。
    NpcNoConquestGold,
    /// 场景：`NPC 脚本装备变量兜底值`（`npc.rs:467`）；模板：`无`；参数：无。
    NpcNone,
    /// 场景：`sell_item`（`npc.rs:468`）；模板：`这个 NPC 不收购这种物品。`；参数：无。
    NpcWontBuyType,
    /// 场景：`repair_item`（`npc.rs:469`）；模板：`这件物品无法修理。`；参数：无。
    NpcCannotRepair,
    /// 场景：`repair_item`（`npc.rs:470`）；模板：`这里不能修理这种物品。`；参数：无。
    NpcRepairTypeNotAllowed,
    /// 场景：`repair_item`；模板：`这件物品的耐久是满的，不需要修理。`；参数：无。
    NpcRepairFull,
    /// 场景：`repair_item`；模板：`修理这件物品需要 {cost} 金币，你只有 {gold}。`；参数：`{cost}`、`{gold}`。
    NpcRepairNoGold,
    /// 场景：`damage_weapon`/`damage_armour`（战斗磨损）；模板：`{name} 的耐久耗尽，已损坏。`；参数：`{name}`。
    ItemBroken,
    /// 场景：配偶传送（`lover_recall_allowed`）；模板：`你已允许配偶传送过来。`。
    LoverRecallAllowed,
    /// 场景：配偶传送（`lover_recall_blocked`）；模板：`你已禁止配偶传送过来。`。
    LoverRecallBlocked,
    /// 场景：配偶传送（`spouse_recall_denied`）；模板：`对方关闭了配偶传送。`。
    SpouseRecallDenied,
    /// 场景：配偶传送（`spouse_ring_given`）；模板：`你们的结婚戒指已经放进背包。`。
    SpouseRingGiven,
    /// 场景：配偶传送（`spouse_not_married`）；模板：`你还没有结婚，无法召唤配偶。`。
    SpouseNotMarried,
    /// 场景：配偶传送（`spouse_offline`）；模板：`你的配偶不在线。`。
    SpouseOffline,
    /// 场景：配偶传送（`spouse_dead`）；模板：`你的配偶已经死亡，无法召唤。`。
    SpouseDead,
    /// 场景：配偶传送（`spouse_teleported`）；模板：`你已传送到 {name} 身边。`。
    SpouseTeleported,
    /// 场景：配偶传送（`spouse_recall_needs_ring`）；模板：`需要戴上结婚戒指才能召唤配偶。`。
    SpouseRecallNeedsRing,
    /// 场景：配偶传送（`SocialError::PartnerRing`）；模板：`对方的结婚戒指不见了（或与你不匹配），无法传送。`。
    SpouseRecallPartnerRing,
    /// 场景：平滑重启倒计时（`server.rs`）；模板：`服务器将在 {seconds} 秒后重启，请安全下线。`；参数：`{seconds}`。
    RebootCountdown,
    /// 场景：婚姻/师徒（`world.rs`）；模板：`求婚已发出，等待对方回应。`。
    MarriageProposalSent,
    /// 场景：婚姻/师徒（`world.rs`）；模板：`{name} 拒绝了你的求婚。`。
    MarriageRefused,
    /// 场景：婚姻/师徒（`world.rs`）；模板：`你和 {name} 结为夫妻。`。
    MarriageDone,
    /// 场景：婚姻/师徒（`world.rs`）；模板：`你和 {name} 解除了婚姻。`。
    DivorceDone,
    /// 场景：婚姻/师徒（`world.rs`）；模板：`你已经出师，师傅是 {name}。`。
    MentorGraduated,
    /// 场景：`run_script_action`（`npc.rs`）；模板：`没有可交出的物品。`；参数：无。
    NpcNoCheckedItem,
    /// 场景：`run_script_action`（`npc.rs`）；模板：`已交出 {name} ×{count}。`；参数：`{name}`、`{count}`。
    NpcItemTaken,
    /// 场景：`run_script_action`（`npc.rs`）；模板：`请从下列名字中选择，或直接输入名字。`；参数：无。
    NpcNameListPrompt,
    /// 场景：`run_script_action`（`npc.rs`）；模板：`名字列表为空。`；参数：无。
    NpcNameListEmpty,
    /// 场景：`run_script_action`（`npc.rs`）；模板：`未知的增益类型：{buff}。`；参数：`{buff}`。
    NpcUnknownBuff,
    /// 场景：`run_script_action`（`npc.rs`）；模板：`增益时长必须是 1~2592000 之间的秒数，收到：{value}。`；参数：`{value}`。
    NpcBuffBadDuration,
    /// 场景：`run_script_action`（`npc.rs`）；模板：`获得增益：{buff}，持续 {seconds} 秒。`；参数：`{buff}`、`{seconds}`。
    NpcBuffGiven,
    /// 场景：`share_quest`（`quests.rs:476`）；模板：`组队并且附近有队友时才能分享任务。`；参数：无。
    QuestShareNeedsParty,
    /// 场景：`on_accept_quest` 失败（过多任务）；模板：`你同时接取的任务太多了。`；参数：无。
    QuestTooMany,
    /// 场景：`on_accept_quest` 其它失败；模板：`接取任务失败。`；参数：无。
    QuestAcceptFailed,
    /// 场景：`on_finish_quest` 失败；模板：`交付任务失败。`；参数：无。
    QuestFinishFailed,
    /// 场景：`on_pickup` 失败；模板：`无法拾取。`；参数：无。
    PickupFailed,
    /// 场景：`on_buy_item` 失败（含背包满）；模板：`购买失败。`；参数：无。
    BuyFailed,
    /// 场景：`S.SendOutputMessage{Quest}`（`quests.rs:477`）；模板：`你杀死了 {monster}`；参数：`{monster}`。
    QuestKilledMonster,
    /// 场景：`任务进度文本后缀`（`quests.rs:478`）；模板：`（已完成）`；参数：无。
    ///
    /// **片段键**：只用于拼在 [`TextKey::QuestTaskFlag`] 的 `{number}` 后面
    /// （`激活标记 {number}{done_suffix}`），不要单独渲染。英文 " (done)" 带前导空格
    /// 是刻意的，拼出来是 "Activate flag 3 (done)"。
    QuestTaskDoneSuffix,
    /// 场景：`任务进度文本`（`quests.rs:479`）；模板：`击杀 {monster} {count}/{total}`；参数：`{monster}`、`{count}`、`{total}`。
    QuestTaskKill,
    /// 场景：`任务进度文本`（`quests.rs:480`）；模板：`收集 {item} {count}/{total}`；参数：`{item}`、`{count}`、`{total}`。
    QuestTaskItem,
    /// 场景：`任务进度文本`（`quests.rs:481`）；模板：`激活标记 {number}{done_suffix}`；参数：`{number}`、`{done_suffix}`。
    QuestTaskFlag,
    /// 场景：`check_need_quest_item` 截胡任务物品（`quest_inventory.rs`，Crystal
    /// `CheckNeedQuestItem` → `S.SendOutputMessage{Type=Quest}`，`PlayerObject.cs:11549-11550`）；
    /// 模板：`你找到了 {item}`；参数：`{item}`。
    QuestItemFound,
    /// 场景：任务导航（TP-1，`S.ChangeQuest.task_list`）；模板：`前往 {map}({file})`；
    /// 参数：`{map}`（地图标题）、`{file}`（地图文件名）。
    QuestNavigationGoTo,
    /// 场景：任务没有可导航目标；模板：`该任务无导航目标`；参数：无。
    QuestNavigationNone,
    /// 场景：`/ally` 用法提示（`session.rs` 联盟命令，本作扩展）；模板：`用法：/ally request|accept|remove <行会名或ID>`；参数：无。
    AllyUsage,
    /// 场景：`/ally` 找不到目标行会；模板：`找不到该行会。`；参数：无。
    AllyGuildNotFound,
    /// 场景：`/ally request` 成功；模板：`结盟申请已发送，等待对方会长同意。`；参数：无。
    AllyRequested,
    /// 场景：`/ally accept` 成功；模板：`结盟成功。`；参数：无。
    AllyAccepted,
    /// 场景：`/ally remove` 成功；模板：`已解除联盟。`；参数：无。
    AllyRemoved,
    /// 场景：`/ally` 操作失败；模板：`联盟操作失败。`；参数：无（详细错误只进日志）。
    AllyFailed,
    /// 场景：`on_gameshop_buy 读计数失败`（`session.rs:487`）；模板：`商城暂时不可用，请稍后再试。`；参数：无。
    GameshopStoreUnavailable,
    /// 场景：`on_gameshop_buy 市场页签成功回执`（`session.rs:488`）；模板：`你购买了 {item} × {count}。`；参数：`{item}`、`{count}`。
    GameshopBoughtMarketSuccess,
    /// 场景：`on_gameshop_buy 落盘失败回滚`（`session.rs:489`）；模板：`商城购买失败：已退回支付的货币与物品，请稍后再试。`；参数：无。
    GameshopRolledBack,
    /// 场景：`trade_request`（`trade.rs:495`）；模板：`你已经在交易中。`；参数：无。
    TradeAlreadyTrading,
    /// 场景：`trade_request`（`trade.rs:496`）；模板：`请面对你要交易的对象。`；参数：无。
    TradeNotFacing,
    /// 场景：`trade_invite`（`trade.rs:500`）；模板：`不能和自己交易。`；参数：无。
    TradeSelf,
    /// 场景：`trade_invite`（`trade.rs:501`）；模板：`死亡状态不能交易。`；参数：无。
    TradeDead,
    /// 场景：`trade_invite`（`trade.rs:502`）；模板：`{target_name} 已经收到了交易邀请。`；参数：`{target_name}`。
    TradeTargetAlreadyInvited,
    /// 场景：`trade_invite`（`trade.rs:503`）；模板：`{target_name} 不允许交易。`；参数：`{target_name}`。
    TradeTargetDisallowed,
    /// 场景：`trade_invite`（`trade.rs:504`）；模板：`{target_name} 不在交易范围内。`；参数：`{target_name}`。
    TradeTargetOutOfRange,
    /// 场景：`trade_invite`（`trade.rs:505`）；模板：`{target_name} 已经在交易中。`；参数：`{target_name}`。
    TradeTargetAlreadyTrading,
    /// 场景：`trade_reply`（`trade.rs:506`）；模板：`{invitee_name} 拒绝交易。`；参数：`{invitee_name}`。
    TradeDeclined,
    /// 场景：`trade_reply`（`trade.rs:508`）；模板：`{inviter_name} 已经在交易中。`；参数：`{inviter_name}`。
    TradeInviterAlreadyTrading,
    /// 场景：`trade_confirm`（`trade.rs:509`）；模板：`{name} 正在等待你确认交易。`；参数：`{name}`。
    TradePartnerWaiting,
    /// 场景：`trade_confirm`（`trade.rs:510`）；模板：`交易成功。`；参数：无。
    TradeSuccess,
    /// 场景：`trade_reject_swap`（`trade.rs:511`）；模板：`你的交易对方无法接受更多金币。`；参数：无。
    TradePartnerGoldOverflow,
    /// 场景：`trade_reject_swap`（`trade.rs:512`）；模板：`你无法接受更多金币。`；参数：无。
    TradeGoldOverflow,
    /// 场景：`trade_reject_swap`（`trade.rs:513`）；模板：`你的交易对方无法接受全部物品。`；参数：无。
    TradePartnerItemsOverflow,
    /// 场景：`trade_reject_swap`（`trade.rs:514`）；模板：`你无法接受全部物品。`；参数：无。
    TradeItemsOverflow,
    /// 场景：`交战区击杀不红名提示`（`world.rs:520`）；模板：`你受到了法律的保护。`；参数：无。
    PkProtectedByLaw,
}

impl TextKey {
    /// 全部文本键（语言包校验、参数核对与测试用）。
    ///
    /// 必须与枚举变体一一对应；`text_key_all_is_complete` 用数量守住这一步。
    pub const ALL: &'static [Self] = &[
        Self::Welcome,
        Self::NewbieGuide,
        Self::AccountBanned,
        Self::WrongPassword,
        Self::TooManyCharacters,
        Self::LanguageChanged,
        Self::UnknownLanguage,
        Self::ServerFull,
        Self::UnsupportedCommand,
        Self::CharacterNameTaken,
        Self::CharacterNameInvalid,
        Self::LoginSuccess,
        Self::ConquestNotInWorld,
        Self::ConquestNotInGuild,
        Self::ConquestNotLeader,
        Self::ConquestAreaNotFound,
        Self::ConquestAlreadyOwner,
        Self::ConquestAtWar,
        Self::ConquestAlreadyScheduled,
        Self::ConquestLevelTooLow,
        Self::ConquestNotEnoughGold,
        Self::ConquestNotOwnerGuild,
        Self::ConquestWarStarted,
        Self::ConquestWarEnded,
        Self::ConquestNoGuild,
        Self::ConquestPointCaptured,
        Self::ConquestNewOwner,
        Self::GroupInviteNotLeader,
        Self::GroupInviteFull,
        Self::GroupInviteMapForbidden,
        Self::GroupInviteTargetMissing,
        Self::GroupInviteSelf,
        Self::GroupInviteTargetDisallowed,
        Self::GroupInviteTargetInGroup,
        Self::GroupInviteTargetBusy,
        Self::GroupInviteTargetMapForbidden,
        Self::GroupJoinMapForbidden,
        Self::GroupJoinNoInvite,
        Self::GroupJoinDeclined,
        Self::GroupJoinExpired,
        Self::GroupJoinAlreadyInGroup,
        Self::GroupJoinInviterOffline,
        Self::GroupJoinInviterNotLeader,
        Self::GroupJoinInviterFull,
        Self::GroupJoinInviterDisallowed,
        Self::GroupJoinInviterMapForbidden,
        Self::GroupMapForbidden,
        Self::GroupKickNotInGroup,
        Self::GroupKickNotLeader,
        Self::GroupKickNotMember,
        Self::GuildNotInWorld,
        Self::GuildNotInGuild,
        Self::GuildAlreadyInGuild,
        Self::GuildNameTaken,
        Self::GuildNameLength,
        Self::GuildNameInvalid,
        Self::GuildCreateLevelTooLow,
        Self::GuildCreateNotEnoughGold,
        Self::GuildNoPermission,
        Self::GuildNotAllowedRecruit,
        Self::GuildNotAllowedKick,
        Self::GuildNotAllowedChangeRank,
        Self::GuildNotAllowedChangeNotice,
        Self::GuildMemberNotFound,
        Self::GuildTargetAlreadyInGuild,
        Self::GuildTargetAlreadyInvited,
        Self::GuildCannotInviteSelf,
        Self::GuildFull,
        Self::GuildNoInvitation,
        Self::GuildLeaderLevelTooLow,
        Self::GuildNeedsTwoLeaders,
        Self::GuildNeedOneLeaderOnline,
        Self::GuildRankNotFound,
        Self::GuildRankNameTooShort,
        Self::GuildRankNameTooLong,
        Self::GuildCannotChangeOwnRank,
        Self::GuildTooManyRanks,
        Self::GuildRankNotAdequate,
        Self::GuildNoticeTooLong,
        Self::GuildStorageOutsideSafezone,
        Self::GuildStorageNoStorePermission,
        Self::GuildStorageNoRetrievePermission,
        Self::GuildStorageNoMovePermission,
        Self::GuildStorageInvalidSlot,
        Self::GuildStorageEmptySlot,
        Self::GuildStorageTargetSlotNotEmpty,
        Self::GuildStorageThrottled,
        Self::GuildStorageItemCannotStore,
        Self::GuildGoldLimitReached,
        Self::GuildPlayerGoldLimitReached,
        Self::GuildBankNotEnoughGold,
        Self::GuildBankLeaderOnly,
        Self::GuildBuffNoPermission,
        Self::GuildBuffNotExist,
        Self::GuildBuffAlreadyObtained,
        Self::GuildBuffNotObtained,
        Self::GuildLevelTooLow,
        Self::GuildPointsInsufficient,
        Self::GuildLastLeaderCannotLeave,
        Self::GuildRemoved,
        Self::GuildDisbanded,
        Self::GuildLeft,
        Self::GuildKicked,
        Self::GuildBuffBoughtPointsGold,
        Self::GuildBuffBoughtGold,
        Self::GuildBuffBoughtPoints,
        Self::GuildBuffBoughtFree,
        Self::ItemUnknownPlaceholder,
        Self::ItemEffectNotImplemented,
        Self::ItemMountNotImplemented,
        Self::ItemPotionNoEffectFull,
        Self::ItemCurseLifted,
        Self::ItemCurseLiftedNow,
        Self::ItemWrongGender,
        Self::ItemWrongClass,
        Self::ItemLevelTooLow,
        Self::ItemLevelTooHigh,
        Self::ItemReviveNotDead,
        Self::MagicAlreadyLearned,
        Self::MagicFirewallLimit,
        Self::MagicTeleportForbidden,
        Self::MagicSummonForbidden,
        Self::NoPushingInSafezone,
        Self::NotEnoughPushingPower,
        Self::MailNotInWorld,
        Self::MailTextTooLong,
        Self::MailRecipientMissing,
        Self::MailMailboxFull,
        Self::MailNotEnoughGold,
        Self::MailItemCannotMail,
        Self::MailNotFound,
        Self::MailNotCollected,
        Self::MailInventoryFull,
        Self::MailLocked,
        Self::MailUncollectedAttachments,
        Self::MailAttachmentCorrupted,
        Self::MailNewMail,
        Self::MailGameshopSender,
        Self::MailGameshopBody,
        Self::MailSystemSender,
        Self::MailTradeItemReturned,
        Self::GameshopNotInWorld,
        Self::GameshopUnavailable,
        Self::GameshopProductNotFound,
        Self::GameshopInvalidQuantity,
        Self::GameshopTooManyStacks,
        Self::GameshopWrongClass,
        Self::GameshopWrongLevel,
        Self::GameshopOutOfStock,
        Self::GameshopPaymentNotAllowed,
        Self::GameshopNotEnoughCredit,
        Self::GameshopNotEnoughGold,
        Self::GameshopUnsupportedPayment,
        Self::GameshopCannotGain,
        Self::MarketLabelSold,
        Self::MarketLabelExpired,
        Self::MarketLabelOnSale,
        Self::MarketLabelHasBid,
        Self::MarketLabelNoBid,
        Self::MarketMailBidRefundExpired,
        Self::MarketMailItemExpired,
        Self::MarketMailSold,
        Self::MarketBought,
        Self::MarketMailOutbidRefund,
        Self::MarketMailNewBid,
        Self::MarketBidPlaced,
        Self::MarketMailAuctionWon,
        Self::MarketSoldNow,
        Self::MarketMailItemWithdrawn,
        Self::MarketMailSoldCommission,
        Self::GameshopMailedItems,
        Self::NpcGuildCreated,
        Self::NpcNoConquestGold,
        Self::NpcNone,
        Self::NpcWontBuyType,
        Self::NpcCannotRepair,
        Self::NpcRepairTypeNotAllowed,
        Self::NpcRepairFull,
        Self::NpcRepairNoGold,
        Self::ItemBroken,
        Self::LoverRecallAllowed,
        Self::LoverRecallBlocked,
        Self::SpouseRecallDenied,
        Self::SpouseRingGiven,
        Self::SpouseNotMarried,
        Self::SpouseOffline,
        Self::SpouseDead,
        Self::SpouseTeleported,
        Self::SpouseRecallNeedsRing,
        Self::SpouseRecallPartnerRing,
        Self::RebootCountdown,
        Self::MarriageProposalSent,
        Self::MarriageRefused,
        Self::MarriageDone,
        Self::DivorceDone,
        Self::MentorGraduated,
        Self::NpcNoCheckedItem,
        Self::NpcItemTaken,
        Self::NpcNameListPrompt,
        Self::NpcNameListEmpty,
        Self::NpcUnknownBuff,
        Self::NpcBuffBadDuration,
        Self::NpcBuffGiven,
        Self::QuestShareNeedsParty,
        Self::QuestTooMany,
        Self::QuestAcceptFailed,
        Self::QuestFinishFailed,
        Self::PickupFailed,
        Self::BuyFailed,
        Self::QuestKilledMonster,
        Self::QuestTaskDoneSuffix,
        Self::QuestTaskKill,
        Self::QuestTaskItem,
        Self::QuestTaskFlag,
        Self::QuestItemFound,
        Self::QuestNavigationGoTo,
        Self::QuestNavigationNone,
        Self::AllyUsage,
        Self::AllyGuildNotFound,
        Self::AllyRequested,
        Self::AllyAccepted,
        Self::AllyRemoved,
        Self::AllyFailed,
        Self::GameshopStoreUnavailable,
        Self::GameshopBoughtMarketSuccess,
        Self::GameshopRolledBack,
        Self::TradeAlreadyTrading,
        Self::TradeNotFacing,
        Self::TradeSelf,
        Self::TradeDead,
        Self::TradeTargetAlreadyInvited,
        Self::TradeTargetDisallowed,
        Self::TradeTargetOutOfRange,
        Self::TradeTargetAlreadyTrading,
        Self::TradeDeclined,
        Self::TradeInviterAlreadyTrading,
        Self::TradePartnerWaiting,
        Self::TradeSuccess,
        Self::TradePartnerGoldOverflow,
        Self::TradeGoldOverflow,
        Self::TradePartnerItemsOverflow,
        Self::TradeItemsOverflow,
        Self::PkProtectedByLaw,
    ];

    /// 语言包里的键名（`config/lang/*.json` 的 `server` 段用这个名字）。
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Welcome => "welcome",
            Self::NewbieGuide => "newbie_guide",
            Self::AccountBanned => "account_banned",
            Self::WrongPassword => "wrong_password",
            Self::TooManyCharacters => "too_many_characters",
            Self::LanguageChanged => "language_changed",
            Self::UnknownLanguage => "unknown_language",
            Self::ServerFull => "server_full",
            Self::UnsupportedCommand => "unsupported_command",
            Self::CharacterNameTaken => "character_name_taken",
            Self::CharacterNameInvalid => "character_name_invalid",
            Self::LoginSuccess => "login_success",
            Self::ConquestNotInWorld => "conquest_not_in_world",
            Self::ConquestNotInGuild => "conquest_not_in_guild",
            Self::ConquestNotLeader => "conquest_not_leader",
            Self::ConquestAreaNotFound => "conquest_area_not_found",
            Self::ConquestAlreadyOwner => "conquest_already_owner",
            Self::ConquestAtWar => "conquest_at_war",
            Self::ConquestAlreadyScheduled => "conquest_already_scheduled",
            Self::ConquestLevelTooLow => "conquest_level_too_low",
            Self::ConquestNotEnoughGold => "conquest_not_enough_gold",
            Self::ConquestNotOwnerGuild => "conquest_not_owner_guild",
            Self::ConquestWarStarted => "conquest_war_started",
            Self::ConquestWarEnded => "conquest_war_ended",
            Self::ConquestNoGuild => "conquest_no_guild",
            Self::ConquestPointCaptured => "conquest_point_captured",
            Self::ConquestNewOwner => "conquest_new_owner",
            Self::GroupInviteNotLeader => "group_invite_not_leader",
            Self::GroupInviteFull => "group_invite_full",
            Self::GroupInviteMapForbidden => "group_invite_map_forbidden",
            Self::GroupInviteTargetMissing => "group_invite_target_missing",
            Self::GroupInviteSelf => "group_invite_self",
            Self::GroupInviteTargetDisallowed => "group_invite_target_disallowed",
            Self::GroupInviteTargetInGroup => "group_invite_target_in_group",
            Self::GroupInviteTargetBusy => "group_invite_target_busy",
            Self::GroupInviteTargetMapForbidden => "group_invite_target_map_forbidden",
            Self::GroupJoinMapForbidden => "group_join_map_forbidden",
            Self::GroupJoinNoInvite => "group_join_no_invite",
            Self::GroupJoinDeclined => "group_join_declined",
            Self::GroupJoinExpired => "group_join_expired",
            Self::GroupJoinAlreadyInGroup => "group_join_already_in_group",
            Self::GroupJoinInviterOffline => "group_join_inviter_offline",
            Self::GroupJoinInviterNotLeader => "group_join_inviter_not_leader",
            Self::GroupJoinInviterFull => "group_join_inviter_full",
            Self::GroupJoinInviterDisallowed => "group_join_inviter_disallowed",
            Self::GroupJoinInviterMapForbidden => "group_join_inviter_map_forbidden",
            Self::GroupMapForbidden => "group_map_forbidden",
            Self::GroupKickNotInGroup => "group_kick_not_in_group",
            Self::GroupKickNotLeader => "group_kick_not_leader",
            Self::GroupKickNotMember => "group_kick_not_member",
            Self::GuildNotInWorld => "guild_not_in_world",
            Self::GuildNotInGuild => "guild_not_in_guild",
            Self::GuildAlreadyInGuild => "guild_already_in_guild",
            Self::GuildNameTaken => "guild_name_taken",
            Self::GuildNameLength => "guild_name_length",
            Self::GuildNameInvalid => "guild_name_invalid",
            Self::GuildCreateLevelTooLow => "guild_create_level_too_low",
            Self::GuildCreateNotEnoughGold => "guild_create_not_enough_gold",
            Self::GuildNoPermission => "guild_no_permission",
            Self::GuildNotAllowedRecruit => "guild_not_allowed_recruit",
            Self::GuildNotAllowedKick => "guild_not_allowed_kick",
            Self::GuildNotAllowedChangeRank => "guild_not_allowed_change_rank",
            Self::GuildNotAllowedChangeNotice => "guild_not_allowed_change_notice",
            Self::GuildMemberNotFound => "guild_member_not_found",
            Self::GuildTargetAlreadyInGuild => "guild_target_already_in_guild",
            Self::GuildTargetAlreadyInvited => "guild_target_already_invited",
            Self::GuildCannotInviteSelf => "guild_cannot_invite_self",
            Self::GuildFull => "guild_full",
            Self::GuildNoInvitation => "guild_no_invitation",
            Self::GuildLeaderLevelTooLow => "guild_leader_level_too_low",
            Self::GuildNeedsTwoLeaders => "guild_needs_two_leaders",
            Self::GuildNeedOneLeaderOnline => "guild_need_one_leader_online",
            Self::GuildRankNotFound => "guild_rank_not_found",
            Self::GuildRankNameTooShort => "guild_rank_name_too_short",
            Self::GuildRankNameTooLong => "guild_rank_name_too_long",
            Self::GuildCannotChangeOwnRank => "guild_cannot_change_own_rank",
            Self::GuildTooManyRanks => "guild_too_many_ranks",
            Self::GuildRankNotAdequate => "guild_rank_not_adequate",
            Self::GuildNoticeTooLong => "guild_notice_too_long",
            Self::GuildStorageOutsideSafezone => "guild_storage_outside_safezone",
            Self::GuildStorageNoStorePermission => "guild_storage_no_store_permission",
            Self::GuildStorageNoRetrievePermission => "guild_storage_no_retrieve_permission",
            Self::GuildStorageNoMovePermission => "guild_storage_no_move_permission",
            Self::GuildStorageInvalidSlot => "guild_storage_invalid_slot",
            Self::GuildStorageEmptySlot => "guild_storage_empty_slot",
            Self::GuildStorageTargetSlotNotEmpty => "guild_storage_target_slot_not_empty",
            Self::GuildStorageThrottled => "guild_storage_throttled",
            Self::GuildStorageItemCannotStore => "guild_storage_item_cannot_store",
            Self::GuildGoldLimitReached => "guild_gold_limit_reached",
            Self::GuildPlayerGoldLimitReached => "guild_player_gold_limit_reached",
            Self::GuildBankNotEnoughGold => "guild_bank_not_enough_gold",
            Self::GuildBankLeaderOnly => "guild_bank_leader_only",
            Self::GuildBuffNoPermission => "guild_buff_no_permission",
            Self::GuildBuffNotExist => "guild_buff_not_exist",
            Self::GuildBuffAlreadyObtained => "guild_buff_already_obtained",
            Self::GuildBuffNotObtained => "guild_buff_not_obtained",
            Self::GuildLevelTooLow => "guild_level_too_low",
            Self::GuildPointsInsufficient => "guild_points_insufficient",
            Self::GuildLastLeaderCannotLeave => "guild_last_leader_cannot_leave",
            Self::GuildRemoved => "guild_removed",
            Self::GuildDisbanded => "guild_disbanded",
            Self::GuildLeft => "guild_left",
            Self::GuildKicked => "guild_kicked",
            Self::GuildBuffBoughtPointsGold => "guild_buff_bought_points_gold",
            Self::GuildBuffBoughtGold => "guild_buff_bought_gold",
            Self::GuildBuffBoughtPoints => "guild_buff_bought_points",
            Self::GuildBuffBoughtFree => "guild_buff_bought_free",
            Self::ItemUnknownPlaceholder => "item_unknown_placeholder",
            Self::ItemEffectNotImplemented => "item_effect_not_implemented",
            Self::ItemMountNotImplemented => "item_mount_not_implemented",
            Self::ItemPotionNoEffectFull => "item_potion_no_effect_full",
            Self::ItemCurseLifted => "item_curse_lifted",
            Self::ItemCurseLiftedNow => "item_curse_lifted_now",
            Self::ItemWrongGender => "item_wrong_gender",
            Self::ItemWrongClass => "item_wrong_class",
            Self::ItemLevelTooLow => "item_level_too_low",
            Self::ItemLevelTooHigh => "item_level_too_high",
            Self::ItemReviveNotDead => "item_revive_not_dead",
            Self::MagicAlreadyLearned => "magic_already_learned",
            Self::MagicFirewallLimit => "magic_firewall_limit",
            Self::MagicTeleportForbidden => "magic_teleport_forbidden",
            Self::MagicSummonForbidden => "magic_summon_forbidden",
            Self::NoPushingInSafezone => "no_pushing_in_safezone",
            Self::NotEnoughPushingPower => "not_enough_pushing_power",
            Self::MailNotInWorld => "mail_not_in_world",
            Self::MailTextTooLong => "mail_text_too_long",
            Self::MailRecipientMissing => "mail_recipient_missing",
            Self::MailMailboxFull => "mail_mailbox_full",
            Self::MailNotEnoughGold => "mail_not_enough_gold",
            Self::MailItemCannotMail => "mail_item_cannot_mail",
            Self::MailNotFound => "mail_not_found",
            Self::MailNotCollected => "mail_not_collected",
            Self::MailInventoryFull => "mail_inventory_full",
            Self::MailLocked => "mail_locked",
            Self::MailUncollectedAttachments => "mail_uncollected_attachments",
            Self::MailAttachmentCorrupted => "mail_attachment_corrupted",
            Self::MailNewMail => "mail_new_mail",
            Self::MailGameshopSender => "mail_gameshop_sender",
            Self::MailGameshopBody => "mail_gameshop_body",
            Self::MailSystemSender => "mail_system_sender",
            Self::MailTradeItemReturned => "mail_trade_item_returned",
            Self::GameshopNotInWorld => "gameshop_not_in_world",
            Self::GameshopUnavailable => "gameshop_unavailable",
            Self::GameshopProductNotFound => "gameshop_product_not_found",
            Self::GameshopInvalidQuantity => "gameshop_invalid_quantity",
            Self::GameshopTooManyStacks => "gameshop_too_many_stacks",
            Self::GameshopWrongClass => "gameshop_wrong_class",
            Self::GameshopWrongLevel => "gameshop_wrong_level",
            Self::GameshopOutOfStock => "gameshop_out_of_stock",
            Self::GameshopPaymentNotAllowed => "gameshop_payment_not_allowed",
            Self::GameshopNotEnoughCredit => "gameshop_not_enough_credit",
            Self::GameshopNotEnoughGold => "gameshop_not_enough_gold",
            Self::GameshopUnsupportedPayment => "gameshop_unsupported_payment",
            Self::GameshopCannotGain => "gameshop_cannot_gain",
            Self::MarketLabelSold => "market_label_sold",
            Self::MarketLabelExpired => "market_label_expired",
            Self::MarketLabelOnSale => "market_label_on_sale",
            Self::MarketLabelHasBid => "market_label_has_bid",
            Self::MarketLabelNoBid => "market_label_no_bid",
            Self::MarketMailBidRefundExpired => "market_mail_bid_refund_expired",
            Self::MarketMailItemExpired => "market_mail_item_expired",
            Self::MarketMailSold => "market_mail_sold",
            Self::MarketBought => "market_bought",
            Self::MarketMailOutbidRefund => "market_mail_outbid_refund",
            Self::MarketMailNewBid => "market_mail_new_bid",
            Self::MarketBidPlaced => "market_bid_placed",
            Self::MarketMailAuctionWon => "market_mail_auction_won",
            Self::MarketSoldNow => "market_sold_now",
            Self::MarketMailItemWithdrawn => "market_mail_item_withdrawn",
            Self::MarketMailSoldCommission => "market_mail_sold_commission",
            Self::GameshopMailedItems => "gameshop_mailed_items",
            Self::NpcGuildCreated => "npc_guild_created",
            Self::NpcNoConquestGold => "npc_no_conquest_gold",
            Self::NpcNone => "npc_none",
            Self::NpcWontBuyType => "npc_wont_buy_type",
            Self::NpcCannotRepair => "npc_cannot_repair",
            Self::NpcRepairTypeNotAllowed => "npc_repair_type_not_allowed",
            Self::NpcRepairFull => "npc_repair_full",
            Self::NpcRepairNoGold => "npc_repair_no_gold",
            Self::ItemBroken => "item_broken",
            Self::LoverRecallAllowed => "lover_recall_allowed",
            Self::LoverRecallBlocked => "lover_recall_blocked",
            Self::SpouseRecallDenied => "spouse_recall_denied",
            Self::SpouseRingGiven => "spouse_ring_given",
            Self::SpouseNotMarried => "spouse_not_married",
            Self::SpouseOffline => "spouse_offline",
            Self::SpouseDead => "spouse_dead",
            Self::SpouseTeleported => "spouse_teleported",
            Self::SpouseRecallNeedsRing => "spouse_recall_needs_ring",
            Self::SpouseRecallPartnerRing => "spouse_recall_partner_ring",
            Self::RebootCountdown => "reboot_countdown",
            Self::MarriageProposalSent => "marriage_proposal_sent",
            Self::MarriageRefused => "marriage_refused",
            Self::MarriageDone => "marriage_done",
            Self::DivorceDone => "divorce_done",
            Self::MentorGraduated => "mentor_graduated",
            Self::NpcNoCheckedItem => "npc_no_checked_item",
            Self::NpcItemTaken => "npc_item_taken",
            Self::NpcNameListPrompt => "npc_name_list_prompt",
            Self::NpcNameListEmpty => "npc_name_list_empty",
            Self::NpcUnknownBuff => "npc_unknown_buff",
            Self::NpcBuffBadDuration => "npc_buff_bad_duration",
            Self::NpcBuffGiven => "npc_buff_given",
            Self::QuestShareNeedsParty => "quest_share_needs_party",
            Self::QuestTooMany => "quest_too_many",
            Self::QuestAcceptFailed => "quest_accept_failed",
            Self::QuestFinishFailed => "quest_finish_failed",
            Self::PickupFailed => "pickup_failed",
            Self::BuyFailed => "buy_failed",
            Self::QuestKilledMonster => "quest_killed_monster",
            Self::QuestTaskDoneSuffix => "quest_task_done_suffix",
            Self::QuestTaskKill => "quest_task_kill",
            Self::QuestTaskItem => "quest_task_item",
            Self::QuestTaskFlag => "quest_task_flag",
            Self::QuestItemFound => "quest_item_found",
            Self::QuestNavigationGoTo => "quest_navigation_go_to",
            Self::QuestNavigationNone => "quest_navigation_none",
            Self::AllyUsage => "ally_usage",
            Self::AllyGuildNotFound => "ally_guild_not_found",
            Self::AllyRequested => "ally_requested",
            Self::AllyAccepted => "ally_accepted",
            Self::AllyRemoved => "ally_removed",
            Self::AllyFailed => "ally_failed",
            Self::GameshopStoreUnavailable => "gameshop_store_unavailable",
            Self::GameshopBoughtMarketSuccess => "gameshop_bought_market_success",
            Self::GameshopRolledBack => "gameshop_rolled_back",
            Self::TradeAlreadyTrading => "trade_already_trading",
            Self::TradeNotFacing => "trade_not_facing",
            Self::TradeSelf => "trade_self",
            Self::TradeDead => "trade_dead",
            Self::TradeTargetAlreadyInvited => "trade_target_already_invited",
            Self::TradeTargetDisallowed => "trade_target_disallowed",
            Self::TradeTargetOutOfRange => "trade_target_out_of_range",
            Self::TradeTargetAlreadyTrading => "trade_target_already_trading",
            Self::TradeDeclined => "trade_declined",
            Self::TradeInviterAlreadyTrading => "trade_inviter_already_trading",
            Self::TradePartnerWaiting => "trade_partner_waiting",
            Self::TradeSuccess => "trade_success",
            Self::TradePartnerGoldOverflow => "trade_partner_gold_overflow",
            Self::TradeGoldOverflow => "trade_gold_overflow",
            Self::TradePartnerItemsOverflow => "trade_partner_items_overflow",
            Self::TradeItemsOverflow => "trade_items_overflow",
            Self::PkProtectedByLaw => "pk_protected_by_law",
        }
    }

    /// 该键模板里的参数名（顺序以中文模板为准；英文语序可能不同，参数集合相同）。
    ///
    /// 无参键返回空切片。测试会核对它和中英两份模板里的占位符完全一致，
    /// 所以新增参数时三处（枚举、模板、语言包）必须同步。
    #[must_use]
    #[allow(clippy::match_same_arms)] // 大多数字符串键确实没有参数，返回同一个 `&[]`。
    pub const fn args(self) -> &'static [&'static str] {
        match self {
            Self::Welcome => &[],
            Self::NewbieGuide => &[],
            Self::AccountBanned => &[],
            Self::WrongPassword => &[],
            Self::TooManyCharacters => &[],
            Self::LanguageChanged => &[],
            Self::UnknownLanguage => &[],
            Self::ServerFull => &[],
            Self::UnsupportedCommand => &[],
            Self::CharacterNameTaken => &[],
            Self::CharacterNameInvalid => &[],
            Self::LoginSuccess => &[],
            Self::ConquestNotInWorld => &["object_id"],
            Self::ConquestNotInGuild => &[],
            Self::ConquestNotLeader => &[],
            Self::ConquestAreaNotFound => &["area"],
            Self::ConquestAlreadyOwner => &[],
            Self::ConquestAtWar => &[],
            Self::ConquestAlreadyScheduled => &[],
            Self::ConquestLevelTooLow => &["required", "level"],
            Self::ConquestNotEnoughGold => &["required", "gold"],
            Self::ConquestNotOwnerGuild => &[],
            Self::ConquestWarStarted => &["name"],
            Self::ConquestWarEnded => &["name"],
            Self::ConquestNoGuild => &[],
            Self::ConquestPointCaptured => &["guild", "point"],
            Self::ConquestNewOwner => &["conquest", "guild"],
            Self::GroupInviteNotLeader => &[],
            Self::GroupInviteFull => &[],
            Self::GroupInviteMapForbidden => &[],
            Self::GroupInviteTargetMissing => &["name"],
            Self::GroupInviteSelf => &[],
            Self::GroupInviteTargetDisallowed => &["target_name"],
            Self::GroupInviteTargetInGroup => &["target_name"],
            Self::GroupInviteTargetBusy => &["target_name"],
            Self::GroupInviteTargetMapForbidden => &["target_name"],
            Self::GroupJoinMapForbidden => &[],
            Self::GroupJoinNoInvite => &[],
            Self::GroupJoinDeclined => &["invitee_name"],
            Self::GroupJoinExpired => &[],
            Self::GroupJoinAlreadyInGroup => &["inviter_name"],
            Self::GroupJoinInviterOffline => &[],
            Self::GroupJoinInviterNotLeader => &["inviter_name"],
            Self::GroupJoinInviterFull => &["inviter_name"],
            Self::GroupJoinInviterDisallowed => &["inviter_name"],
            Self::GroupJoinInviterMapForbidden => &["inviter_name"],
            Self::GroupMapForbidden => &[],
            Self::GroupKickNotInGroup => &[],
            Self::GroupKickNotLeader => &[],
            Self::GroupKickNotMember => &["name"],
            Self::GuildNotInWorld => &["object_id"],
            Self::GuildNotInGuild => &[],
            Self::GuildAlreadyInGuild => &[],
            Self::GuildNameTaken => &["name"],
            Self::GuildNameLength => &["min", "max"],
            Self::GuildNameInvalid => &[],
            Self::GuildCreateLevelTooLow => &["required", "level"],
            Self::GuildCreateNotEnoughGold => &["required", "gold"],
            Self::GuildNoPermission => &["permission"],
            Self::GuildNotAllowedRecruit => &[],
            Self::GuildNotAllowedKick => &[],
            Self::GuildNotAllowedChangeRank => &[],
            Self::GuildNotAllowedChangeNotice => &[],
            Self::GuildMemberNotFound => &["name"],
            Self::GuildTargetAlreadyInGuild => &["name"],
            Self::GuildTargetAlreadyInvited => &["name"],
            Self::GuildCannotInviteSelf => &[],
            Self::GuildFull => &[],
            Self::GuildNoInvitation => &[],
            Self::GuildLeaderLevelTooLow => &["required"],
            Self::GuildNeedsTwoLeaders => &[],
            Self::GuildNeedOneLeaderOnline => &[],
            Self::GuildRankNotFound => &["rank"],
            Self::GuildRankNameTooShort => &["min"],
            Self::GuildRankNameTooLong => &["max"],
            Self::GuildCannotChangeOwnRank => &[],
            Self::GuildTooManyRanks => &[],
            Self::GuildRankNotAdequate => &[],
            Self::GuildNoticeTooLong => &["lines"],
            Self::GuildStorageOutsideSafezone => &[],
            Self::GuildStorageNoStorePermission => &[],
            Self::GuildStorageNoRetrievePermission => &[],
            Self::GuildStorageNoMovePermission => &[],
            Self::GuildStorageInvalidSlot => &[],
            Self::GuildStorageEmptySlot => &[],
            Self::GuildStorageTargetSlotNotEmpty => &[],
            Self::GuildStorageThrottled => &[],
            Self::GuildStorageItemCannotStore => &[],
            Self::GuildGoldLimitReached => &[],
            Self::GuildPlayerGoldLimitReached => &[],
            Self::GuildBankNotEnoughGold => &["required", "gold"],
            Self::GuildBankLeaderOnly => &[],
            Self::GuildBuffNoPermission => &[],
            Self::GuildBuffNotExist => &["buff"],
            Self::GuildBuffAlreadyObtained => &["buff"],
            Self::GuildBuffNotObtained => &["buff"],
            Self::GuildLevelTooLow => &["required", "level"],
            Self::GuildPointsInsufficient => &["required", "remaining"],
            Self::GuildLastLeaderCannotLeave => &[],
            Self::GuildRemoved => &[],
            Self::GuildDisbanded => &[],
            Self::GuildLeft => &[],
            Self::GuildKicked => &[],
            Self::GuildBuffBoughtPointsGold => &["player", "points", "gold", "buff"],
            Self::GuildBuffBoughtGold => &["player", "gold", "buff"],
            Self::GuildBuffBoughtPoints => &["player", "points", "buff"],
            Self::GuildBuffBoughtFree => &["player", "buff"],
            Self::ItemUnknownPlaceholder => &["item_index"],
            Self::ItemEffectNotImplemented => &["item"],
            Self::ItemMountNotImplemented => &[],
            Self::ItemPotionNoEffectFull => &[],
            Self::ItemCurseLifted => &[],
            Self::ItemCurseLiftedNow => &[],
            Self::ItemWrongGender => &[],
            Self::ItemWrongClass => &[],
            Self::ItemLevelTooLow => &["required"],
            Self::ItemLevelTooHigh => &["max"],
            Self::ItemReviveNotDead => &[],
            Self::MagicAlreadyLearned => &["spell"],
            Self::MagicFirewallLimit => &[],
            Self::MagicTeleportForbidden => &[],
            Self::MagicSummonForbidden => &[],
            Self::NoPushingInSafezone => &[],
            Self::NotEnoughPushingPower => &[],
            Self::MailNotInWorld => &[],
            Self::MailTextTooLong => &["max"],
            Self::MailRecipientMissing => &[],
            Self::MailMailboxFull => &[],
            Self::MailNotEnoughGold => &["needed", "held"],
            Self::MailItemCannotMail => &[],
            Self::MailNotFound => &[],
            Self::MailNotCollected => &[],
            Self::MailInventoryFull => &[],
            Self::MailLocked => &[],
            Self::MailUncollectedAttachments => &[],
            Self::MailAttachmentCorrupted => &[],
            Self::MailNewMail => &[],
            Self::MailGameshopSender => &[],
            Self::MailGameshopBody => &[],
            Self::MailSystemSender => &[],
            Self::MailTradeItemReturned => &[],
            Self::GameshopNotInWorld => &[],
            Self::GameshopUnavailable => &[],
            Self::GameshopProductNotFound => &[],
            Self::GameshopInvalidQuantity => &[],
            Self::GameshopTooManyStacks => &["max", "stacks"],
            Self::GameshopWrongClass => &[],
            Self::GameshopWrongLevel => &["required"],
            Self::GameshopOutOfStock => &[],
            Self::GameshopPaymentNotAllowed => &[],
            Self::GameshopNotEnoughCredit => &[],
            Self::GameshopNotEnoughGold => &[],
            Self::GameshopUnsupportedPayment => &[],
            Self::GameshopCannotGain => &[],
            Self::MarketLabelSold => &[],
            Self::MarketLabelExpired => &[],
            Self::MarketLabelOnSale => &[],
            Self::MarketLabelHasBid => &[],
            Self::MarketLabelNoBid => &[],
            Self::MarketMailBidRefundExpired => &["item", "gold"],
            Self::MarketMailItemExpired => &["item"],
            Self::MarketMailSold => &["item", "gold"],
            Self::MarketBought => &["gold", "item"],
            Self::MarketMailOutbidRefund => &["item", "gold"],
            Self::MarketMailNewBid => &["item", "bid"],
            Self::MarketBidPlaced => &["item", "bid"],
            Self::MarketMailAuctionWon => &["item", "cost"],
            Self::MarketSoldNow => &["cost", "item", "gold"],
            Self::MarketMailItemWithdrawn => &["item", "gold"],
            Self::MarketMailSoldCommission => &["item", "cost", "gold"],
            Self::GameshopMailedItems => &["count"],
            Self::NpcGuildCreated => &["name"],
            Self::NpcNoConquestGold => &[],
            Self::NpcNone => &[],
            Self::NpcWontBuyType => &[],
            Self::NpcCannotRepair => &[],
            Self::NpcRepairTypeNotAllowed => &[],
            Self::NpcRepairFull => &[],
            Self::NpcRepairNoGold => &["cost", "gold"],
            Self::ItemBroken => &["name"],
            Self::LoverRecallAllowed => &[],
            Self::LoverRecallBlocked => &[],
            Self::SpouseRecallDenied => &[],
            Self::SpouseRingGiven => &[],
            Self::SpouseNotMarried => &[],
            Self::SpouseOffline => &[],
            Self::SpouseDead => &[],
            Self::SpouseTeleported => &["name"],
            Self::SpouseRecallNeedsRing => &[],
            Self::SpouseRecallPartnerRing => &[],
            Self::RebootCountdown => &["seconds"],
            Self::MarriageProposalSent => &[],
            Self::MarriageRefused => &["name"],
            Self::MarriageDone => &["name"],
            Self::DivorceDone => &["name"],
            Self::MentorGraduated => &["name"],
            Self::NpcNoCheckedItem => &[],
            Self::NpcItemTaken => &["name", "count"],
            Self::NpcNameListPrompt => &[],
            Self::NpcNameListEmpty => &[],
            Self::NpcUnknownBuff => &["buff"],
            Self::NpcBuffBadDuration => &["value"],
            Self::NpcBuffGiven => &["buff", "seconds"],
            Self::QuestShareNeedsParty => &[],
            Self::QuestTooMany => &[],
            Self::QuestAcceptFailed => &[],
            Self::QuestFinishFailed => &[],
            Self::PickupFailed => &[],
            Self::BuyFailed => &[],
            Self::QuestKilledMonster => &["monster"],
            Self::QuestTaskDoneSuffix => &[],
            Self::QuestTaskKill => &["monster", "count", "total"],
            Self::QuestTaskItem => &["item", "count", "total"],
            Self::QuestTaskFlag => &["number", "done_suffix"],
            Self::QuestItemFound => &["item"],
            Self::QuestNavigationGoTo => &["map", "file"],
            Self::QuestNavigationNone => &[],
            Self::AllyUsage => &[],
            Self::AllyGuildNotFound => &[],
            Self::AllyRequested => &[],
            Self::AllyAccepted => &[],
            Self::AllyRemoved => &[],
            Self::AllyFailed => &[],
            Self::GameshopStoreUnavailable => &[],
            Self::GameshopBoughtMarketSuccess => &["item", "count"],
            Self::GameshopRolledBack => &[],
            Self::TradeAlreadyTrading => &[],
            Self::TradeNotFacing => &[],
            Self::TradeSelf => &[],
            Self::TradeDead => &[],
            Self::TradeTargetAlreadyInvited => &["target_name"],
            Self::TradeTargetDisallowed => &["target_name"],
            Self::TradeTargetOutOfRange => &["target_name"],
            Self::TradeTargetAlreadyTrading => &["target_name"],
            Self::TradeDeclined => &["invitee_name"],
            Self::TradeInviterAlreadyTrading => &["inviter_name"],
            Self::TradePartnerWaiting => &["name"],
            Self::TradeSuccess => &[],
            Self::TradePartnerGoldOverflow => &[],
            Self::TradeGoldOverflow => &[],
            Self::TradePartnerItemsOverflow => &[],
            Self::TradeItemsOverflow => &[],
            Self::PkProtectedByLaw => &[],
        }
    }
}

/// 取某个语言下的内置文本模板（语言包缺失或没覆盖时用它兜底）。
///
/// 返回的是**模板**：带参数的键里含 `{name}` 占位符，要用 [`format_text`] 渲染；
/// 无参键可以直接当成品文本用。`TextKey` 是穷举 `match`，新增键时会强制补全
/// 所有语言，不会出现「某种语言漏翻」。
#[must_use]
pub const fn text(language: Language, key: TextKey) -> &'static str {
    match language {
        Language::ZhCn => text_zh(key),
        Language::En => text_en(key),
    }
}

/// 用命名实参渲染某个键的内置模板。
///
/// 语义：
///
/// * 模板里的 `{name}` 被 `args` 里同名实参替换；同一个占位符出现多次会全部替换；
/// * **没给**的占位符**原样保留**（例如 `{held}`），不会 panic —— 便于测试和渐进迁移
///   发现漏参；
/// * `args` 里**多余的**实参被忽略（同一组实参可以喂给多条模板）；
/// * `{{` / `}}` 渲染成字面大括号 `{` / `}`；空占位符 `{}` 不是合法占位符，原样保留。
///
/// 无参键等价于 [`text`]，只是返回 `String`：
///
/// ```
/// use mir_core::i18n::{format_text, text, Language, TextKey};
///
/// let rendered = format_text(
///     Language::ZhCn,
///     TextKey::MailNotEnoughGold,
///     &[("needed", "100"), ("held", "40")],
/// );
/// assert_eq!(rendered, "金币不足：需要 100，你只有 40。");
///
/// // 漏了一个实参：未提供的占位符原样保留，方便发现漏参。
/// let partial = format_text(Language::ZhCn, TextKey::MailNotEnoughGold, &[("needed", "100")]);
/// assert_eq!(partial, "金币不足：需要 100，你只有 {held}。");
///
/// // 无参键与 `text()` 等价；多余实参被忽略。
/// assert_eq!(
///     format_text(Language::En, TextKey::TradeSuccess, &[("gold", "1")]),
///     text(Language::En, TextKey::TradeSuccess),
/// );
/// ```
#[must_use]
pub fn format_text(language: Language, key: TextKey, args: &[(&str, &str)]) -> String {
    render_template(text(language, key), args)
}

/// 行会权限的内部标识（`GuildError::Permission` 里的 `"CanRecruit"` 等）→ 本地化名称。
///
/// [`TextKey::GuildNoPermission`] 的 `{permission}` 不能直接塞内部 CamelCase 标识符，
/// 否则玩家会看到「你的职位没有 `CanRecruit` 权限」。本函数按语言返回与模板吻合的词性：
///
/// * 中文返回名词短语（模板：「你的职位没有 {permission} 权限」）：
///   「你的职位没有招收成员权限」；
/// * 英文返回动词短语（模板："Your rank does not have permission to {permission}."）：
///   "Your rank does not have permission to recruit members."
///
/// 未知标识返回 `None`，调用方应回退到通用提示或原始标识：
///
/// ```
/// use mir_core::i18n::{format_text, guild_permission_name, Language, TextKey};
///
/// let language = Language::En;
/// let permission = guild_permission_name(language, "CanKick").unwrap_or("CanKick");
/// assert_eq!(
///     format_text(language, TextKey::GuildNoPermission, &[("permission", permission)]),
///     "Your rank does not have permission to kick members."
/// );
/// ```
#[must_use]
pub fn guild_permission_name(language: Language, permission: &str) -> Option<&'static str> {
    match (language, permission) {
        (Language::ZhCn, "CanRecruit") => Some("招收成员"),
        (Language::ZhCn, "CanKick") => Some("踢出成员"),
        (Language::ZhCn, "CanChangeRank") => Some("修改职位"),
        (Language::ZhCn, "CanChangeNotice") => Some("修改公告"),
        (Language::En, "CanRecruit") => Some("recruit members"),
        (Language::En, "CanKick") => Some("kick members"),
        (Language::En, "CanChangeRank") => Some("change ranks"),
        (Language::En, "CanChangeNotice") => Some("change the guild notice"),
        _ => None,
    }
}

/// 渲染模板：替换 `{name}`、保留未知占位符、`{{`/`}}` 转义为字面大括号。
fn render_template(template: &str, args: &[(&str, &str)]) -> String {
    let mut rendered = String::with_capacity(template.len());
    let mut rest = template;
    while !rest.is_empty() {
        // 先处理转义，否则 `{{` 会被当成占位符开头。
        if let Some(after) = rest.strip_prefix("{{") {
            rendered.push('{');
            rest = after;
            continue;
        }
        if let Some(after) = rest.strip_prefix("}}") {
            rendered.push('}');
            rest = after;
            continue;
        }
        if let Some(after) = rest.strip_prefix('{') {
            if let Some(close) = after.find('}') {
                let name = &after[..close];
                if name.is_empty() || name.contains('{') {
                    // `{}` / 畸形占位符：按字面量处理。
                    rendered.push('{');
                    rendered.push_str(name);
                    rendered.push('}');
                } else if let Some((_, value)) = args.iter().find(|(argument, _)| *argument == name)
                {
                    rendered.push_str(value);
                } else {
                    // 没提供这个占位符的实参：原样保留，方便测试发现漏参。
                    rendered.push('{');
                    rendered.push_str(name);
                    rendered.push('}');
                }
                rest = &after[close + 1..];
                continue;
            }
        }
        // 普通字符：原样搬一个 `char`（模板是 UTF-8，不能按字节切）。
        let Some(character) = rest.chars().next() else {
            break;
        };
        rendered.push(character);
        rest = &rest[character.len_utf8()..];
    }
    rendered
}

/// 简体中文模板。
#[allow(clippy::match_same_arms)] // 不同键可能共用一句文案（例如行会/攻城都提示「你不在行会中」）。
const fn text_zh(key: TextKey) -> &'static str {
    match key {
        TextKey::Welcome => "欢迎来到传奇-Next。",
        TextKey::NewbieGuide => {
            "新手提示：背包里已有出生装；到书店买技能书，双击阅读即可学会第一个技能。"
        }
        TextKey::AccountBanned => "该账号已被封禁。",
        TextKey::WrongPassword => "账号或密码错误。",
        TextKey::TooManyCharacters => "角色数量已达上限。",
        TextKey::LanguageChanged => "语言已切换为简体中文。",
        TextKey::UnknownLanguage => "未知的语言代码，可用：zh-cn、en。",
        TextKey::ServerFull => "服务器人数已满，请稍后再试。",
        TextKey::UnsupportedCommand => "不支持的命令。",
        TextKey::CharacterNameTaken => "该角色名已被占用。",
        TextKey::CharacterNameInvalid => "角色名不合法：3-15 个汉字、字母、数字或下划线。",
        TextKey::LoginSuccess => "登录成功。",
        TextKey::ConquestNotInWorld => "对象 {object_id} 不在世界里",
        TextKey::ConquestNotInGuild => "你不在行会中",
        TextKey::ConquestNotLeader => "只有会长才能申请攻城",
        TextKey::ConquestAreaNotFound => "找不到攻城区域 {area}",
        TextKey::ConquestAlreadyOwner => "你的行会已经是这座城的城主",
        TextKey::ConquestAtWar => "这座城正在开战，无法申请",
        TextKey::ConquestAlreadyScheduled => "已经有一个行会申请了这座城的攻城",
        TextKey::ConquestLevelTooLow => "申请攻城需要行会等级 {required}（当前 {level}）",
        TextKey::ConquestNotEnoughGold => "申请攻城需要 {required} 金币（行会现有 {gold}）",
        TextKey::ConquestNotOwnerGuild => "只有城主行会才能执行这个操作",
        TextKey::ConquestWarStarted => "【攻城】{name} 攻城战已经开始！",
        TextKey::ConquestWarEnded => "【攻城】{name} 攻城战已经结束。",
        TextKey::ConquestNoGuild => "（无行会）",
        TextKey::ConquestPointCaptured => "【攻城】{guild} 占领了控制点 {point}。",
        TextKey::ConquestNewOwner => "【攻城】{conquest} 的新城主是 {guild}。",
        TextKey::GroupInviteNotLeader => "你不是队长，不能邀请队员。",
        TextKey::GroupInviteFull => "你的队伍人数已满。",
        TextKey::GroupInviteMapForbidden => "当前地图禁止组队，无法邀请。",
        TextKey::GroupInviteTargetMissing => "找不到玩家 {name}。",
        TextKey::GroupInviteSelf => "不能邀请自己加入队伍。",
        TextKey::GroupInviteTargetDisallowed => "{target_name} 不允许组队。",
        TextKey::GroupInviteTargetInGroup => "{target_name} 已经在队伍中。",
        TextKey::GroupInviteTargetBusy => "{target_name} 正在接收其他人的组队邀请。",
        TextKey::GroupInviteTargetMapForbidden => "{target_name} 所在的地图禁止组队。",
        TextKey::GroupJoinMapForbidden => "当前地图禁止组队，无法接受邀请。",
        TextKey::GroupJoinNoInvite => "你没有收到组队邀请。",
        TextKey::GroupJoinDeclined => "{invitee_name} 拒绝了你的组队邀请。",
        TextKey::GroupJoinExpired => "组队邀请已过期。",
        TextKey::GroupJoinAlreadyInGroup => "你已经在队伍中，无法加入 {inviter_name} 的队伍。",
        TextKey::GroupJoinInviterOffline => "邀请者已经下线。",
        TextKey::GroupJoinInviterNotLeader => "{inviter_name} 已经不是队长了。",
        TextKey::GroupJoinInviterFull => "{inviter_name} 的队伍人数已满。",
        TextKey::GroupJoinInviterDisallowed => "{inviter_name} 不再允许组队。",
        TextKey::GroupJoinInviterMapForbidden => "{inviter_name} 在地图禁止组队的区域。",
        TextKey::GroupMapForbidden => "当前地图禁止组队。",
        TextKey::GroupKickNotInGroup => "你不在队伍中。",
        TextKey::GroupKickNotLeader => "你不是队长，不能移除队员。",
        TextKey::GroupKickNotMember => "{name} 不在你的队伍中。",
        TextKey::GuildNotInWorld => "对象 {object_id} 不在世界里",
        TextKey::GuildNotInGuild => "你不在行会中",
        TextKey::GuildAlreadyInGuild => "你已经在行会中",
        TextKey::GuildNameTaken => "行会 {name} 已经存在",
        TextKey::GuildNameLength => "行会名长度必须是 {min}-{max} 个字符",
        TextKey::GuildNameInvalid => "行会名不能包含反斜杠或换行",
        TextKey::GuildCreateLevelTooLow => "创建行会需要 {required} 级（你当前 {level} 级）",
        TextKey::GuildCreateNotEnoughGold => "创建行会需要 {required} 金币（你只有 {gold}）",
        TextKey::GuildNoPermission => "你的职位没有 {permission} 权限",
        TextKey::GuildNotAllowedRecruit => "你的职位没有招收成员的权限",
        TextKey::GuildNotAllowedKick => "你的职位没有踢出成员的权限",
        TextKey::GuildNotAllowedChangeRank => "你的职位没有修改成员职位的权限",
        TextKey::GuildNotAllowedChangeNotice => "你的职位没有修改行会公告的权限",
        TextKey::GuildMemberNotFound => "行会里没有成员 {name}",
        TextKey::GuildTargetAlreadyInGuild => "{name} 已经在行会中",
        TextKey::GuildTargetAlreadyInvited => "{name} 已经收到一个行会邀请",
        TextKey::GuildCannotInviteSelf => "不能邀请自己",
        TextKey::GuildFull => "行会人数已满",
        TextKey::GuildNoInvitation => "没有待接受的行会邀请",
        TextKey::GuildLeaderLevelTooLow => "对方等级不足以成为会长（需要 {required} 级）",
        TextKey::GuildNeedsTwoLeaders => "行会至少要有两个会长才能降职",
        TextKey::GuildNeedOneLeaderOnline => "需要至少一个会长在线",
        TextKey::GuildRankNotFound => "职位 {rank} 不存在",
        TextKey::GuildRankNameTooShort => "职位名至少 {min} 个字符",
        TextKey::GuildRankNameTooLong => "职位名最多 {max} 个字符",
        TextKey::GuildCannotChangeOwnRank => "不能修改自己职位的权限",
        TextKey::GuildTooManyRanks => "职位数量已达上限",
        TextKey::GuildRankNotAdequate => "你的职位不够高",
        TextKey::GuildNoticeTooLong => "公告最多 {lines} 行",
        TextKey::GuildStorageOutsideSafezone => "只能在安全区内使用行会仓库",
        TextKey::GuildStorageNoStorePermission => "你的职位没有往行会仓库存物品的权限",
        TextKey::GuildStorageNoRetrievePermission => "你的职位没有从行会仓库取物品的权限",
        TextKey::GuildStorageNoMovePermission => "你的职位没有整理行会仓库的权限",
        TextKey::GuildStorageInvalidSlot => "格子序号不在范围内",
        TextKey::GuildStorageEmptySlot => "格子里没有物品",
        TextKey::GuildStorageTargetSlotNotEmpty => "目标格子不是空的",
        TextKey::GuildStorageThrottled => "仓库列表请求过于频繁",
        TextKey::GuildStorageItemCannotStore => "这件物品不能存入行会仓库",
        TextKey::GuildGoldLimitReached => "行会资金已达上限",
        TextKey::GuildPlayerGoldLimitReached => "你的金币已达上限",
        TextKey::GuildBankNotEnoughGold => "行会资金不足（需要 {required}，现有 {gold}）",
        TextKey::GuildBankLeaderOnly => "只有会长可以从行会仓库取金币",
        TextKey::GuildBuffNoPermission => "你的职位没有激活行会增益的权限",
        TextKey::GuildBuffNotExist => "行会增益 {buff} 不存在",
        TextKey::GuildBuffAlreadyObtained => "行会已经拥有增益 {buff}",
        TextKey::GuildBuffNotObtained => "行会还没有购买增益 {buff}",
        TextKey::GuildLevelTooLow => "行会等级不足（需要 {required} 级，当前 {level} 级）",
        TextKey::GuildPointsInsufficient => {
            "行会升级点不足（需要 {required} 点，现有 {remaining} 点）"
        }
        TextKey::GuildLastLeaderCannotLeave => "你是最后一个会长：请先转让会长或解散行会",
        TextKey::GuildRemoved => "你已经被移出行会。",
        TextKey::GuildDisbanded => "你已经解散了行会。",
        TextKey::GuildLeft => "你离开了行会。",
        TextKey::GuildKicked => "你被移出了行会。",
        TextKey::GuildBuffBoughtPointsGold => {
            "「{player}」花费 {points} 升级点与 {gold} 金币为行会购买了增益「{buff}」。"
        }
        TextKey::GuildBuffBoughtGold => "「{player}」花费 {gold} 金币为行会购买了增益「{buff}」。",
        TextKey::GuildBuffBoughtPoints => {
            "「{player}」花费 {points} 升级点为行会购买了增益「{buff}」。"
        }
        TextKey::GuildBuffBoughtFree => "「{player}」为行会购买了增益「{buff}」。",
        TextKey::ItemUnknownPlaceholder => "<未知物品 {item_index}>",
        TextKey::ItemEffectNotImplemented => "「{item}」的效果尚未实现，物品没有被消耗。",
        TextKey::ItemMountNotImplemented => "本作还不能骑乘，食物暂时用不上。",
        TextKey::ItemPotionNoEffectFull => "生命和魔法都是满的，药水没有效果。",
        TextKey::ItemCurseLifted => "你已经可以卸下诅咒装备了。",
        TextKey::ItemCurseLiftedNow => "你现在可以卸下诅咒装备了。",
        TextKey::ItemWrongGender => "你的性别无法使用这件物品。",
        TextKey::ItemWrongClass => "你的职业无法使用这件物品。",
        TextKey::ItemLevelTooLow => "等级不够：需要 {required} 级。",
        TextKey::ItemLevelTooHigh => "等级过高：最多 {max} 级可用。",
        TextKey::ItemReviveNotDead => "你还没有死亡，不能使用复活卷轴。",
        TextKey::MagicAlreadyLearned => "你已经学会了「{spell}」，不需要再学一次。",
        TextKey::MagicFirewallLimit => "火墙数量已达上限，请稍后再试。",
        TextKey::MagicTeleportForbidden => "这张地图不能使用瞬息移动。",
        TextKey::MagicSummonForbidden => "这张地图不能召唤宠物。",
        TextKey::NoPushingInSafezone => "安全区内无法冲撞。",
        TextKey::NotEnoughPushingPower => "冲撞力量不足。",
        TextKey::MailNotInWorld => "你不在游戏世界里。",
        TextKey::MailTextTooLong => "邮件正文不能超过 {max} 字。",
        TextKey::MailRecipientMissing => "找不到该玩家。",
        TextKey::MailMailboxFull => "收件人的邮箱已满。",
        TextKey::MailNotEnoughGold => "金币不足：需要 {needed}，你只有 {held}。",
        TextKey::MailItemCannotMail => "这件物品不能邮寄。",
        TextKey::MailNotFound => "找不到这封邮件。",
        TextKey::MailNotCollected => "请先到邮局受理包裹。",
        TextKey::MailInventoryFull => "背包已满，无法领取附件。",
        TextKey::MailLocked => "邮件已锁定，不能删除。",
        TextKey::MailUncollectedAttachments => "附件还没领取，不能删除。",
        TextKey::MailAttachmentCorrupted => "邮件附件数据损坏。",
        TextKey::MailNewMail => "你有新邮件。",
        TextKey::MailGameshopSender => "商城",
        TextKey::MailGameshopBody => "感谢您购买商城商品。",
        TextKey::MailSystemSender => "比奇管理员",
        TextKey::MailTradeItemReturned => {
            "由于以下原因，物品已通过邮件寄给你：\r\n\r\n交易结束后无法放回背包。"
        }
        TextKey::GameshopNotInWorld => "你不在游戏世界里。",
        TextKey::GameshopUnavailable => "商城暂未开放。",
        TextKey::GameshopProductNotFound => "商城里没有这件商品。",
        TextKey::GameshopInvalidQuantity => "购买数量不对。",
        TextKey::GameshopTooManyStacks => "一次最多买 {max} 组物品（这次要 {stacks} 组）。",
        TextKey::GameshopWrongClass => "你的职业不能购买这件商品。",
        TextKey::GameshopWrongLevel => "等级不够：需要 {required} 级。",
        TextKey::GameshopOutOfStock => "库存不足。",
        TextKey::GameshopPaymentNotAllowed => "这件商品不能用这种方式购买。",
        TextKey::GameshopNotEnoughCredit => "元宝不足。",
        TextKey::GameshopNotEnoughGold => "金币不足。",
        TextKey::GameshopUnsupportedPayment => "不支持的支付方式。",
        TextKey::GameshopCannotGain => "背包放不下。",
        TextKey::MarketLabelSold => "已售出",
        TextKey::MarketLabelExpired => "已过期",
        TextKey::MarketLabelOnSale => "出售中",
        TextKey::MarketLabelHasBid => "已有出价",
        TextKey::MarketLabelNoBid => "无人出价",
        TextKey::MarketMailBidRefundExpired => {
            "你在 {item} 上的出价已退回：拍卖已过期（{gold} 金币）。"
        }
        TextKey::MarketMailItemExpired => "你挂出的 {item} 已过期，物品已通过邮件退回。",
        TextKey::MarketMailSold => "你挂出的 {item} 已售出，收入 {gold} 金币。",
        TextKey::MarketBought => "你以 {gold} 金币买下了 {item}。",
        TextKey::MarketMailOutbidRefund => {
            "你在 {item} 上的出价被超过，{gold} 金币已通过邮件退回。"
        }
        TextKey::MarketMailNewBid => "有人为 {item} 出价 {bid} 金币。",
        TextKey::MarketBidPlaced => "你为 {item} 出价 {bid} 金币。",
        TextKey::MarketMailAuctionWon => "你在拍卖中赢得了 {item}，成交价 {cost} 金币。",
        TextKey::MarketSoldNow => "你以 {cost} 金币卖出了 {item}，扣除佣金后收入 {gold} 金币。",
        TextKey::MarketMailItemWithdrawn => {
            "你挂出的 {item} 已被卖家下架，{gold} 金币已通过邮件退回。"
        }
        TextKey::MarketMailSoldCommission => {
            "你卖出了 {item}，成交价 {cost}，扣除佣金后收入 {gold} 金币。"
        }
        TextKey::GameshopMailedItems => "背包放不下的 {count} 份物品已寄到邮箱，请到邮局领取。",
        TextKey::NpcGuildCreated => "行会 {name} 创建成功。",
        TextKey::NpcNoConquestGold => "城里没有可收取的金币。",
        TextKey::NpcNone => "无",
        TextKey::NpcWontBuyType => "这个 NPC 不收购这种物品。",
        TextKey::NpcCannotRepair => "这件物品无法修理。",
        TextKey::NpcRepairTypeNotAllowed => "这里不能修理这种物品。",
        TextKey::NpcRepairFull => "这件物品的耐久是满的，不需要修理。",
        TextKey::NpcRepairNoGold => "修理这件物品需要 {cost} 金币，你只有 {gold}。",
        TextKey::ItemBroken => "{name} 的耐久耗尽，已损坏。",
        TextKey::LoverRecallAllowed => "你已允许配偶传送过来。",
        TextKey::LoverRecallBlocked => "你已禁止配偶传送过来。",
        TextKey::SpouseRecallDenied => "对方关闭了配偶传送。",
        TextKey::SpouseRingGiven => "你们的结婚戒指已经放进背包。",
        TextKey::SpouseNotMarried => "你还没有结婚，无法召唤配偶。",
        TextKey::SpouseOffline => "你的配偶不在线。",
        TextKey::SpouseDead => "你的配偶已经死亡，无法召唤。",
        TextKey::SpouseTeleported => "你已传送到 {name} 身边。",
        TextKey::SpouseRecallNeedsRing => "需要戴上结婚戒指才能召唤配偶。",
        TextKey::SpouseRecallPartnerRing => "对方的结婚戒指不见了（或与你不匹配），无法传送。",
        TextKey::RebootCountdown => "服务器将在 {seconds} 秒后重启，请安全下线。",
        TextKey::MarriageProposalSent => "求婚已发出，等待对方回应。",
        TextKey::MarriageRefused => "{name} 拒绝了你的求婚。",
        TextKey::MarriageDone => "你和 {name} 结为夫妻。",
        TextKey::DivorceDone => "你和 {name} 解除了婚姻。",
        TextKey::MentorGraduated => "你已经出师，师傅是 {name}。",
        TextKey::NpcNoCheckedItem => "没有可交出的物品。",
        TextKey::NpcItemTaken => "已交出 {name} ×{count}。",
        TextKey::NpcNameListPrompt => "请从下列名字中选择，或直接输入名字。",
        TextKey::NpcNameListEmpty => "名字列表为空。",
        TextKey::NpcUnknownBuff => "未知的增益类型：{buff}。",
        TextKey::NpcBuffBadDuration => "增益时长必须是 1~2592000 之间的秒数，收到：{value}。",
        TextKey::NpcBuffGiven => "获得增益：{buff}，持续 {seconds} 秒。",
        TextKey::QuestShareNeedsParty => "组队并且附近有队友时才能分享任务。",
        TextKey::QuestTooMany => "你同时接取的任务太多了。",
        TextKey::QuestAcceptFailed => "接取任务失败。",
        TextKey::QuestFinishFailed => "交付任务失败。",
        TextKey::PickupFailed => "无法拾取。",
        TextKey::BuyFailed => "购买失败。",
        TextKey::QuestKilledMonster => "你杀死了 {monster}",
        TextKey::QuestTaskDoneSuffix => "（已完成）",
        TextKey::QuestTaskKill => "击杀 {monster} {count}/{total}",
        TextKey::QuestTaskItem => "收集 {item} {count}/{total}",
        TextKey::QuestTaskFlag => "激活标记 {number}{done_suffix}",
        TextKey::QuestItemFound => "你找到了 {item}",
        TextKey::QuestNavigationGoTo => "前往 {map}({file})",
        TextKey::QuestNavigationNone => "该任务无导航目标",
        TextKey::AllyUsage => "用法：/ally request|accept|remove <行会名或ID>",
        TextKey::AllyGuildNotFound => "找不到该行会。",
        TextKey::AllyRequested => "结盟申请已发送，等待对方会长同意。",
        TextKey::AllyAccepted => "结盟成功。",
        TextKey::AllyRemoved => "已解除联盟。",
        TextKey::AllyFailed => "联盟操作失败。",
        TextKey::GameshopStoreUnavailable => "商城暂时不可用，请稍后再试。",
        TextKey::GameshopBoughtMarketSuccess => "你购买了 {item} × {count}。",
        TextKey::GameshopRolledBack => "商城购买失败：已退回支付的货币与物品，请稍后再试。",
        TextKey::TradeAlreadyTrading => "你已经在交易中。",
        TextKey::TradeNotFacing => "请面对你要交易的对象。",
        TextKey::TradeSelf => "不能和自己交易。",
        TextKey::TradeDead => "死亡状态不能交易。",
        TextKey::TradeTargetAlreadyInvited => "{target_name} 已经收到了交易邀请。",
        TextKey::TradeTargetDisallowed => "{target_name} 不允许交易。",
        TextKey::TradeTargetOutOfRange => "{target_name} 不在交易范围内。",
        TextKey::TradeTargetAlreadyTrading => "{target_name} 已经在交易中。",
        TextKey::TradeDeclined => "{invitee_name} 拒绝交易。",
        TextKey::TradeInviterAlreadyTrading => "{inviter_name} 已经在交易中。",
        TextKey::TradePartnerWaiting => "{name} 正在等待你确认交易。",
        TextKey::TradeSuccess => "交易成功。",
        TextKey::TradePartnerGoldOverflow => "你的交易对方无法接受更多金币。",
        TextKey::TradeGoldOverflow => "你无法接受更多金币。",
        TextKey::TradePartnerItemsOverflow => "你的交易对方无法接受全部物品。",
        TextKey::TradeItemsOverflow => "你无法接受全部物品。",
        TextKey::PkProtectedByLaw => "你受到了法律的保护。",
    }
}

/// 英文模板。
#[allow(clippy::match_same_arms)] // 同 `text_zh`：不同键可能共用一句文案。
const fn text_en(key: TextKey) -> &'static str {
    match key {
        TextKey::Welcome => "Welcome to Mir-Next.",
        TextKey::NewbieGuide => {
            "Newbie tip: your starter equipment is in the bag; buy a skill book from the bookshop and double-click it to learn your first skill."
        }
        TextKey::AccountBanned => "This account is banned.",
        TextKey::WrongPassword => "Incorrect account or password.",
        TextKey::TooManyCharacters => "You have too many characters.",
        TextKey::LanguageChanged => "Language switched to English.",
        TextKey::UnknownLanguage => "Unknown language code. Available: zh-cn, en.",
        TextKey::ServerFull => "The server is full, please try again later.",
        TextKey::UnsupportedCommand => "Unsupported command.",
        TextKey::CharacterNameTaken => "That character name is already taken.",
        TextKey::CharacterNameInvalid => {
            "Invalid character name: 3-15 CJK characters, letters, digits or underscores."
        }
        TextKey::LoginSuccess => "Login successful.",
        TextKey::ConquestNotInWorld => "Object {object_id} is not in the world.",
        TextKey::ConquestNotInGuild => "You are not in a guild.",
        TextKey::ConquestNotLeader => "Only the guild leader can request a siege.",
        TextKey::ConquestAreaNotFound => "Conquest area {area} could not be found.",
        TextKey::ConquestAlreadyOwner => "Your guild already owns this castle.",
        TextKey::ConquestAtWar => "This castle is at war; you cannot apply for it.",
        TextKey::ConquestAlreadyScheduled => {
            "Another guild has already applied to lay siege to this castle."
        }
        TextKey::ConquestLevelTooLow => {
            "A siege request requires guild level {required} (currently {level})."
        }
        TextKey::ConquestNotEnoughGold => {
            "A siege request costs {required} Gold (the guild has {gold})."
        }
        TextKey::ConquestNotOwnerGuild => "Only the owning guild can do that.",
        TextKey::ConquestWarStarted => "[Siege] The siege of {name} has begun!",
        TextKey::ConquestWarEnded => "[Siege] The siege of {name} has ended.",
        TextKey::ConquestNoGuild => "(No guild)",
        TextKey::ConquestPointCaptured => "[Siege] {guild} has captured control point {point}.",
        TextKey::ConquestNewOwner => "[Siege] The new owner of {conquest} is {guild}.",
        TextKey::GroupInviteNotLeader => "You are not the group leader and cannot invite members.",
        TextKey::GroupInviteFull => "Your group is full.",
        TextKey::GroupInviteMapForbidden => {
            "Grouping is not allowed on this map; you cannot invite."
        }
        TextKey::GroupInviteTargetMissing => "Player {name} could not be found.",
        TextKey::GroupInviteSelf => "You cannot invite yourself to a group.",
        TextKey::GroupInviteTargetDisallowed => "{target_name} does not allow grouping.",
        TextKey::GroupInviteTargetInGroup => "{target_name} is already in a group.",
        TextKey::GroupInviteTargetBusy => {
            "{target_name} is already receiving a group invitation from someone else."
        }
        TextKey::GroupInviteTargetMapForbidden => {
            "{target_name} is on a map where grouping is not allowed."
        }
        TextKey::GroupJoinMapForbidden => {
            "Grouping is not allowed on this map; you cannot accept the invitation."
        }
        TextKey::GroupJoinNoInvite => "You have not received a group invitation.",
        TextKey::GroupJoinDeclined => "{invitee_name} declined your group invitation.",
        TextKey::GroupJoinExpired => "The group invitation has expired.",
        TextKey::GroupJoinAlreadyInGroup => {
            "You are already in a group and cannot join {inviter_name}'s group."
        }
        TextKey::GroupJoinInviterOffline => "The inviter has gone offline.",
        TextKey::GroupJoinInviterNotLeader => "{inviter_name} is no longer the group leader.",
        TextKey::GroupJoinInviterFull => "{inviter_name}'s group is full.",
        TextKey::GroupJoinInviterDisallowed => "{inviter_name} no longer allows grouping.",
        TextKey::GroupJoinInviterMapForbidden => {
            "{inviter_name} is on a map where grouping is not allowed."
        }
        TextKey::GroupMapForbidden => "Grouping is not allowed on this map.",
        TextKey::GroupKickNotInGroup => "You are not in a group.",
        TextKey::GroupKickNotLeader => "You are not the group leader and cannot remove members.",
        TextKey::GroupKickNotMember => "{name} is not in your group.",
        TextKey::GuildNotInWorld => "Object {object_id} is not in the world.",
        TextKey::GuildNotInGuild => "You are not in a guild.",
        TextKey::GuildAlreadyInGuild => "You are already in a guild.",
        TextKey::GuildNameTaken => "Guild {name} already exists.",
        TextKey::GuildNameLength => "Guild names must be {min}-{max} characters long.",
        TextKey::GuildNameInvalid => "Guild names cannot contain backslashes or line breaks.",
        TextKey::GuildCreateLevelTooLow => {
            "Creating a guild requires level {required} (you are level {level})."
        }
        TextKey::GuildCreateNotEnoughGold => {
            "Creating a guild costs {required} Gold (you only have {gold})."
        }
        TextKey::GuildNoPermission => "Your rank does not have permission to {permission}.",
        TextKey::GuildNotAllowedRecruit => "Your rank is not allowed to recruit members.",
        TextKey::GuildNotAllowedKick => "Your rank is not allowed to remove members.",
        TextKey::GuildNotAllowedChangeRank => "Your rank is not allowed to change member ranks.",
        TextKey::GuildNotAllowedChangeNotice => {
            "Your rank is not allowed to change the guild notice."
        }
        TextKey::GuildMemberNotFound => "There is no member named {name} in the guild.",
        TextKey::GuildTargetAlreadyInGuild => "{name} is already in a guild.",
        TextKey::GuildTargetAlreadyInvited => "{name} already has a pending guild invitation.",
        TextKey::GuildCannotInviteSelf => "You cannot invite yourself.",
        TextKey::GuildFull => "The guild is full.",
        TextKey::GuildNoInvitation => "You have no pending guild invitation.",
        TextKey::GuildLeaderLevelTooLow => {
            "That player's level is too low to become leader (requires level {required})."
        }
        TextKey::GuildNeedsTwoLeaders => {
            "The guild needs at least two leaders before one can be demoted."
        }
        TextKey::GuildNeedOneLeaderOnline => "At least one leader must be online.",
        TextKey::GuildRankNotFound => "Rank {rank} does not exist.",
        TextKey::GuildRankNameTooShort => "Rank names must be at least {min} characters long.",
        TextKey::GuildRankNameTooLong => "Rank names can be at most {max} characters long.",
        TextKey::GuildCannotChangeOwnRank => "You cannot change the permissions of your own rank.",
        TextKey::GuildTooManyRanks => "The maximum number of ranks has been reached.",
        TextKey::GuildRankNotAdequate => "Your rank is not high enough.",
        TextKey::GuildNoticeTooLong => "The guild notice can be at most {lines} line(s) long.",
        TextKey::GuildStorageOutsideSafezone => {
            "The guild storage can only be used in a safe zone."
        }
        TextKey::GuildStorageNoStorePermission => {
            "Your rank does not have permission to store items in the guild storage."
        }
        TextKey::GuildStorageNoRetrievePermission => {
            "Your rank does not have permission to take items from the guild storage."
        }
        TextKey::GuildStorageNoMovePermission => {
            "Your rank does not have permission to rearrange the guild storage."
        }
        TextKey::GuildStorageInvalidSlot => "That slot number is out of range.",
        TextKey::GuildStorageEmptySlot => "There is no item in that slot.",
        TextKey::GuildStorageTargetSlotNotEmpty => "The target slot is not empty.",
        TextKey::GuildStorageThrottled => "Storage list requests are being sent too quickly.",
        TextKey::GuildStorageItemCannotStore => "This item cannot be stored in the guild storage.",
        TextKey::GuildGoldLimitReached => "The guild funds have reached their limit.",
        TextKey::GuildPlayerGoldLimitReached => "You have reached the Gold limit.",
        TextKey::GuildBankNotEnoughGold => {
            "Not enough guild funds (requires {required}, available {gold})."
        }
        TextKey::GuildBankLeaderOnly => {
            "Only the guild leader can withdraw Gold from the guild storage."
        }
        TextKey::GuildBuffNoPermission => {
            "Your rank does not have permission to activate guild buffs."
        }
        TextKey::GuildBuffNotExist => "Guild buff {buff} does not exist.",
        TextKey::GuildBuffAlreadyObtained => "The guild already has buff {buff}.",
        TextKey::GuildBuffNotObtained => "The guild has not purchased buff {buff} yet.",
        TextKey::GuildLevelTooLow => {
            "Guild level too low (requires level {required}, currently level {level})."
        }
        TextKey::GuildPointsInsufficient => {
            "Not enough guild upgrade points (requires {required}, available {remaining})."
        }
        TextKey::GuildLastLeaderCannotLeave => {
            "You are the last leader: transfer leadership or disband the guild first."
        }
        TextKey::GuildRemoved => "You have been removed from your guild.",
        TextKey::GuildDisbanded => "You have disbanded the guild.",
        TextKey::GuildLeft => "You have left the guild.",
        TextKey::GuildKicked => "You have been kicked out of the guild.",
        TextKey::GuildBuffBoughtPointsGold => {
            "{player} purchased guild buff \"{buff}\", spending {points} points and {gold} Gold."
        }
        TextKey::GuildBuffBoughtGold => {
            "{player} purchased guild buff \"{buff}\", spending {gold} Gold."
        }
        TextKey::GuildBuffBoughtPoints => {
            "{player} purchased guild buff \"{buff}\", spending {points} points."
        }
        TextKey::GuildBuffBoughtFree => "{player} purchased guild buff \"{buff}\" for the guild.",
        TextKey::ItemUnknownPlaceholder => "<Unknown item {item_index}>",
        TextKey::ItemEffectNotImplemented => {
            "The effect of \"{item}\" is not implemented yet, so the item was not consumed."
        }
        TextKey::ItemMountNotImplemented => "You cannot mount yet, so food is useless for now.",
        TextKey::ItemPotionNoEffectFull => {
            "Health and Mana are already full; the potion has no effect."
        }
        TextKey::ItemCurseLifted => "You can already unequip cursed equipment.",
        TextKey::ItemCurseLiftedNow => "You can now unequip cursed equipment.",
        TextKey::ItemWrongGender => "Your gender cannot use this item.",
        TextKey::ItemWrongClass => "Your class cannot use this item.",
        TextKey::ItemLevelTooLow => "Level requirement not met: requires level {required}.",
        TextKey::ItemLevelTooHigh => "Level too high: usable up to level {max}.",
        TextKey::ItemReviveNotDead => "You are not dead yet, so you cannot use a revival scroll.",
        TextKey::MagicAlreadyLearned => {
            "You have already learned \"{spell}\" and do not need to learn it again."
        }
        TextKey::MagicFirewallLimit => {
            "The maximum number of Fire Walls has been reached. Please try again later."
        }
        TextKey::MagicTeleportForbidden => "Teleport cannot be used on this map.",
        TextKey::MagicSummonForbidden => "Pets cannot be summoned on this map.",
        TextKey::NoPushingInSafezone => "You cannot use Shoulder Dash inside a safe zone.",
        TextKey::NotEnoughPushingPower => "You do not have enough pushing power.",
        TextKey::MailNotInWorld => "You are not in the game world.",
        TextKey::MailTextTooLong => "The mail body cannot exceed {max} characters.",
        TextKey::MailRecipientMissing => "That player could not be found.",
        TextKey::MailMailboxFull => "The recipient's mailbox is full.",
        TextKey::MailNotEnoughGold => "Not enough Gold: requires {needed}, you have {held}.",
        TextKey::MailItemCannotMail => "This item cannot be mailed.",
        TextKey::MailNotFound => "That mail could not be found.",
        TextKey::MailNotCollected => "Please accept the parcel at the post office first.",
        TextKey::MailInventoryFull => "Your bag is full, so the attachment cannot be collected.",
        TextKey::MailLocked => "This mail is locked and cannot be deleted.",
        TextKey::MailUncollectedAttachments => {
            "The attachments have not been collected, so this mail cannot be deleted."
        }
        TextKey::MailAttachmentCorrupted => "The mail attachment data is corrupted.",
        TextKey::MailNewMail => "You have new mail.",
        TextKey::MailGameshopSender => "Shop",
        TextKey::MailGameshopBody => "Thank you for your purchase from the shop.",
        TextKey::MailSystemSender => "Bichon Administrator",
        TextKey::MailTradeItemReturned => {
            "For the following reason, the item was sent to you by mail:\r\n\r\nIt could not be put back into your bag after the trade ended."
        }
        TextKey::GameshopNotInWorld => "You are not in the game world.",
        TextKey::GameshopUnavailable => "The shop is not available yet.",
        TextKey::GameshopProductNotFound => "The shop does not carry this item.",
        TextKey::GameshopInvalidQuantity => "Invalid purchase quantity.",
        TextKey::GameshopTooManyStacks => {
            "You can buy at most {max} stacks at a time ({stacks} requested)."
        }
        TextKey::GameshopWrongClass => "Your class cannot buy this item.",
        TextKey::GameshopWrongLevel => "Level requirement not met: requires level {required}.",
        TextKey::GameshopOutOfStock => "Out of stock.",
        TextKey::GameshopPaymentNotAllowed => "This item cannot be bought this way.",
        TextKey::GameshopNotEnoughCredit => "Not enough Credits.",
        TextKey::GameshopNotEnoughGold => "Not enough Gold.",
        TextKey::GameshopUnsupportedPayment => "Unsupported payment method.",
        TextKey::GameshopCannotGain => "Your bag is full.",
        TextKey::MarketLabelSold => "Sold",
        TextKey::MarketLabelExpired => "Expired",
        TextKey::MarketLabelOnSale => "On sale",
        TextKey::MarketLabelHasBid => "Has bids",
        TextKey::MarketLabelNoBid => "No bids",
        TextKey::MarketMailBidRefundExpired => {
            "Your bid on {item} has been refunded: the auction expired ({gold} Gold)."
        }
        TextKey::MarketMailItemExpired => {
            "Your listing of {item} has expired; the item has been returned by mail."
        }
        TextKey::MarketMailSold => "Your listing of {item} has sold for {gold} Gold.",
        TextKey::MarketBought => "You bought {item} for {gold} Gold.",
        TextKey::MarketMailOutbidRefund => {
            "You were outbid on {item}; {gold} Gold has been returned to you by mail."
        }
        TextKey::MarketMailNewBid => "Someone bid {bid} Gold on {item}.",
        TextKey::MarketBidPlaced => "You bid {bid} Gold on {item}.",
        TextKey::MarketMailAuctionWon => "You won {item} in the auction for {cost} Gold.",
        TextKey::MarketSoldNow => {
            "You sold {item} for {cost} Gold and received {gold} Gold after commission."
        }
        TextKey::MarketMailItemWithdrawn => {
            "Your listing of {item} was withdrawn by the seller; {gold} Gold has been returned to you by mail."
        }
        TextKey::MarketMailSoldCommission => {
            "You sold {item} for {cost} and received {gold} Gold after commission."
        }
        TextKey::GameshopMailedItems => {
            "{count} item(s) that did not fit in your bag have been mailed to you. Please collect them at the post office."
        }
        TextKey::NpcGuildCreated => "Guild {name} has been created.",
        TextKey::NpcNoConquestGold => "There is no Gold to collect in this castle.",
        TextKey::NpcNone => "None",
        TextKey::NpcWontBuyType => "This NPC does not buy that kind of item.",
        TextKey::NpcCannotRepair => "This item cannot be repaired.",
        TextKey::NpcRepairTypeNotAllowed => "This type of item cannot be repaired here.",
        TextKey::NpcRepairFull => "This item is already at full durability.",
        TextKey::NpcRepairNoGold => {
            "Repairing this item costs {cost} gold, but you only have {gold}."
        }
        TextKey::ItemBroken => "{name} has worn out and is broken.",
        TextKey::LoverRecallAllowed => "You now allow your spouse to recall to you.",
        TextKey::LoverRecallBlocked => "You no longer allow your spouse to recall to you.",
        TextKey::SpouseRecallDenied => "Your spouse has disabled spouse recall.",
        TextKey::RebootCountdown => {
            "The server will reboot in {seconds} second(s). Please log out safely."
        }
        TextKey::SpouseRingGiven => "Your wedding rings have been placed in your inventory.",
        TextKey::SpouseNotMarried => "You are not married, so you cannot recall a spouse.",
        TextKey::SpouseOffline => "Your spouse is not online.",
        TextKey::SpouseDead => "Your spouse is dead and cannot be recalled.",
        TextKey::SpouseTeleported => "You have been teleported next to {name}.",
        TextKey::SpouseRecallNeedsRing => {
            "You need to wear your wedding ring to recall your spouse."
        }
        TextKey::SpouseRecallPartnerRing => {
            "Your spouse is not wearing a matching wedding ring, so the recall is refused."
        }
        TextKey::MarriageProposalSent => "Proposal sent, waiting for the answer.",
        TextKey::MarriageRefused => "{name} turned down your proposal.",
        TextKey::MarriageDone => "You and {name} are now married.",
        TextKey::DivorceDone => "Your marriage to {name} has ended.",
        TextKey::MentorGraduated => "You have graduated from {name}'s mentorship.",
        TextKey::NpcNoCheckedItem => "There is no item to hand over.",
        TextKey::NpcItemTaken => "Handed over {name} x{count}.",
        TextKey::NpcNameListPrompt => "Pick a name from the list, or type one.",
        TextKey::NpcNameListEmpty => "The name list is empty.",
        TextKey::NpcUnknownBuff => "Unknown buff type: {buff}.",
        TextKey::NpcBuffBadDuration => {
            "The buff duration must be a number of seconds between 1 and 2592000, got: {value}."
        }
        TextKey::NpcBuffGiven => "Gained buff {buff} for {seconds} seconds.",
        TextKey::QuestShareNeedsParty => {
            "You can only share a quest while in a group with a member nearby."
        }
        TextKey::QuestTooMany => "You are already on too many quests.",
        TextKey::QuestAcceptFailed => "Failed to accept the quest.",
        TextKey::QuestFinishFailed => "Failed to finish the quest.",
        TextKey::PickupFailed => "You cannot pick that up.",
        TextKey::BuyFailed => "Purchase failed.",
        TextKey::QuestKilledMonster => "You killed {monster}",
        TextKey::QuestTaskDoneSuffix => " (done)",
        TextKey::QuestTaskKill => "Kill {monster} {count}/{total}",
        TextKey::QuestTaskItem => "Collect {item} {count}/{total}",
        TextKey::QuestTaskFlag => "Activate flag {number}{done_suffix}",
        TextKey::QuestItemFound => "You found {item}",
        TextKey::QuestNavigationGoTo => "Go to {map} ({file})",
        TextKey::QuestNavigationNone => "This quest has no navigation target",
        TextKey::AllyUsage => "Usage: /ally request|accept|remove <guild name or id>",
        TextKey::AllyGuildNotFound => "Guild not found.",
        TextKey::AllyRequested => {
            "Ally request sent; waiting for the other guild leader's approval."
        }
        TextKey::AllyAccepted => "Alliance formed.",
        TextKey::AllyRemoved => "Alliance dissolved.",
        TextKey::AllyFailed => "Ally operation failed.",
        TextKey::GameshopStoreUnavailable => {
            "The shop is temporarily unavailable. Please try again later."
        }
        TextKey::GameshopBoughtMarketSuccess => "You bought {item} × {count}.",
        TextKey::GameshopRolledBack => {
            "Purchase failed: your payment and items have been refunded. Please try again later."
        }
        TextKey::TradeAlreadyTrading => "You are already trading.",
        TextKey::TradeNotFacing => "Please face the player you want to trade with.",
        TextKey::TradeSelf => "You cannot trade with yourself.",
        TextKey::TradeDead => "You cannot trade while dead.",
        TextKey::TradeTargetAlreadyInvited => "{target_name} has already received a trade request.",
        TextKey::TradeTargetDisallowed => "{target_name} does not allow trading.",
        TextKey::TradeTargetOutOfRange => "{target_name} is out of trading range.",
        TextKey::TradeTargetAlreadyTrading => "{target_name} is already trading.",
        TextKey::TradeDeclined => "{invitee_name} declined the trade.",
        TextKey::TradeInviterAlreadyTrading => "{inviter_name} is already trading.",
        TextKey::TradePartnerWaiting => "{name} is waiting for you to confirm the trade.",
        TextKey::TradeSuccess => "Trade successful.",
        TextKey::TradePartnerGoldOverflow => "Your trading partner cannot accept any more Gold.",
        TextKey::TradeGoldOverflow => "You cannot accept any more Gold.",
        TextKey::TradePartnerItemsOverflow => {
            "Your trading partner cannot accept all of the items."
        }
        TextKey::TradeItemsOverflow => "You cannot accept all of the items.",
        TextKey::PkProtectedByLaw => "You are protected by the law.",
    }
}

/// 语言包解析/加载错误。
#[derive(Debug, thiserror::Error)]
pub enum LanguagePackError {
    /// 读取文件失败。
    #[error("读取语言包 {path} 失败：{source}")]
    Io {
        /// 文件路径。
        path: String,
        /// 原始错误。
        source: std::io::Error,
    },
    /// JSON 解析失败。
    #[error("语言包 {path} 解析失败：{source}")]
    Parse {
        /// 文件路径。
        path: String,
        /// 原始错误。
        source: serde_json::Error,
    },
    /// 语言代码不认识。
    #[error("语言包 {path} 里的语言代码 {code} 不受支持")]
    UnknownLanguage {
        /// 文件路径。
        path: String,
        /// 语言代码。
        code: String,
    },
}

/// 一份语言包（`config/lang/<code>.json`）。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LanguagePack {
    /// 语言代码，例如 `zh-cn`。
    pub language: String,
    /// 服务端文本模板：`TextKey::key()` → 模板（含 `{name}` 占位符）。
    #[serde(default)]
    pub server: BTreeMap<String, String>,
    /// 内容文本：地图文件名的键（如 `"0"`）→ 地图标题。
    #[serde(default)]
    pub maps: BTreeMap<String, String>,
    /// NPC 显示名：NPC 脚本文件名（或 `脚本名|英文名` 复合键）→ 本地化名字。
    ///
    /// 由 `tools` 从 `resource/.../npcs.ini` + cjlaaa 数据生成，见
    /// `docs/world/data-languages.md` §A+。键按脚本名对齐 `Envir/NPCs/*.txt`；
    /// 同一脚本承载多个不同名字的 NPC 时用 `脚本名|英文名` 复合键（例如
    /// `PrajnaIsland/8Piac|Merchant_Grace`）。
    #[serde(default)]
    pub npc_names: BTreeMap<String, String>,
    /// 物品显示名：`ItemInfo.Index` 的字符串形式 → 本地化名字。
    ///
    /// 键用 index 而不是英文名（物品可能重名/改名）；缺项时回退 DB 原名。
    #[serde(default)]
    pub item_names: BTreeMap<String, String>,
    /// 怪物显示名：`MonsterInfo.Index` 的字符串形式 → 本地化名字；缺项回退 DB 原名。
    #[serde(default)]
    pub monster_names: BTreeMap<String, String>,
    /// 技能显示名：`MagicInfo.Spell` 的字符串形式 → 本地化名字；缺项回退 DB 原名。
    #[serde(default)]
    pub skill_names: BTreeMap<String, String>,
    /// 任务显示名：`QuestInfo.Index` 的字符串形式 → 本地化名字；缺项回退 DB 原名。
    #[serde(default)]
    pub quest_names: BTreeMap<String, String>,
}

impl LanguagePack {
    /// 解析一份语言包。
    ///
    /// # Errors
    /// JSON 非法或语言代码不受支持时返回错误。
    pub fn from_json(source: &str) -> Result<Self, String> {
        let pack: Self = serde_json::from_str(source).map_err(|error| error.to_string())?;
        if Language::from_code(&pack.language).is_none() {
            return Err(format!("不支持的语言代码：{}", pack.language));
        }
        Ok(pack)
    }

    /// 该语言包对应的语言。
    #[must_use]
    pub fn language(&self) -> Language {
        Language::from_code(&self.language).unwrap_or_default()
    }
}

/// 全部语言包。
#[derive(Debug, Clone, Default)]
pub struct LanguageLibrary {
    packs: BTreeMap<String, LanguagePack>,
}

impl LanguageLibrary {
    /// 内置语言包（编译进二进制，保证开箱即用且默认中文）。
    ///
    /// # Panics
    /// 仅当仓库里的 `config/lang/*.json` 被改坏时才会 panic；这些文件由测试覆盖。
    #[must_use]
    pub fn embedded() -> Self {
        let mut library = Self::default();
        for source in [
            include_str!("../../../config/lang/zh-cn.json"),
            include_str!("../../../config/lang/en.json"),
        ] {
            let pack = LanguagePack::from_json(source).expect("内置语言包必须合法");
            library.packs.insert(pack.language.clone(), pack);
        }
        library
    }

    /// 从一个目录加载全部 `*.json` 语言包，覆盖同名的内置语言包。
    ///
    /// # Errors
    /// 目录读取失败、JSON 非法或语言代码不受支持时返回错误。
    pub fn load_dir(directory: &Path) -> Result<Self, LanguagePackError> {
        let mut library = Self::embedded();
        let entries = match std::fs::read_dir(directory) {
            Ok(entries) => entries,
            // 目录不存在时不报错：内置语言包已经够用，便于部署时精简。
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(library),
            Err(source) => {
                return Err(LanguagePackError::Io {
                    path: directory.display().to_string(),
                    source,
                });
            }
        };
        for entry in entries {
            let entry = entry.map_err(|source| LanguagePackError::Io {
                path: directory.display().to_string(),
                source,
            })?;
            let path = entry.path();
            if path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            let source =
                std::fs::read_to_string(&path).map_err(|source| LanguagePackError::Io {
                    path: path.display().to_string(),
                    source,
                })?;
            let pack = serde_json::from_str::<LanguagePack>(&source).map_err(|source| {
                LanguagePackError::Parse {
                    path: path.display().to_string(),
                    source,
                }
            })?;
            if Language::from_code(&pack.language).is_none() {
                return Err(LanguagePackError::UnknownLanguage {
                    path: path.display().to_string(),
                    code: pack.language.clone(),
                });
            }
            library.packs.insert(pack.language.clone(), pack);
        }
        Ok(library)
    }

    /// 已加载的语言代码。
    #[must_use]
    pub fn languages(&self) -> Vec<Language> {
        self.packs
            .values()
            .map(LanguagePack::language)
            .collect::<Vec<_>>()
    }

    /// 取某个语言包；缺语言包时返回 `None`。
    #[must_use]
    pub fn pack(&self, language: Language) -> Option<&LanguagePack> {
        self.packs.get(language.code())
    }

    /// 取某个键的模板：优先语言包里的覆盖模板，缺失时退回编译期内置模板。
    #[must_use]
    pub fn template(&self, language: Language, key: TextKey) -> &str {
        self.pack(language)
            .and_then(|pack| pack.server.get(key.key()))
            .map_or_else(|| text(language, key), String::as_str)
    }

    /// 取服务端成品文本：优先语言包，缺失时退回编译期内置文本。
    ///
    /// 参数化键请用 [`LanguageLibrary::format`]，否则会拿到带 `{name}` 的模板。
    #[must_use]
    pub fn text(&self, language: Language, key: TextKey) -> String {
        self.template(language, key).to_owned()
    }

    /// 取服务端成品文本并渲染参数：模板来源同 [`LanguageLibrary::text`]，
    /// 渲染规则同 [`format_text`]。
    #[must_use]
    pub fn format(&self, language: Language, key: TextKey, args: &[(&str, &str)]) -> String {
        render_template(self.template(language, key), args)
    }

    /// 取地图标题：优先语言包，缺失时用地图文件名兜底。
    #[must_use]
    pub fn map_title(&self, language: Language, map_file: &str) -> String {
        if let Some(value) = self
            .pack(language)
            .or_else(|| self.pack(Language::ZhCn))
            .and_then(|pack| pack.maps.get(map_file))
        {
            return value.clone();
        }
        map_file.to_owned()
    }

    /// 取 NPC 显示名：按「复合键 → 脚本名 → 英文名」查语言包，全部未命中时回退英文原名。
    ///
    /// `script_name` 是 `NpcInfo.FileName`（相对 `Envir/NPCs/`，不含 `.txt`）；
    /// `english_name` 是 `NpcInfo.Name`（DB 原名）。回退顺序保证：
    /// 语言包没配某个 NPC 时不会 panic，也不会显示空白。
    #[must_use]
    pub fn npc_name(&self, language: Language, script_name: &str, english_name: &str) -> String {
        let pack = self.pack(language).or_else(|| self.pack(Language::ZhCn));
        if let Some(pack) = pack {
            let composite = format!("{script_name}|{english_name}");
            if let Some(value) = pack.npc_names.get(&composite) {
                return value.clone();
            }
            if let Some(value) = pack.npc_names.get(script_name) {
                return value.clone();
            }
            if let Some(value) = pack.npc_names.get(english_name) {
                return value.clone();
            }
        }
        english_name.to_owned()
    }

    /// 取物品显示名：按 `ItemInfo.Index` 查语言包，缺项回退 DB 原名。
    ///
    /// 键是十进制 index 的字符串；这样客户端/服务端都不依赖物品英文名。
    #[must_use]
    pub fn item_name(&self, language: Language, index: i32, database_name: &str) -> String {
        let key = index.to_string();
        self.pack(language)
            .or_else(|| self.pack(Language::ZhCn))
            .and_then(|pack| pack.item_names.get(&key))
            .cloned()
            .unwrap_or_else(|| database_name.to_owned())
    }

    /// 取怪物显示名：按 `MonsterInfo.Index` 查语言包，缺项回退 DB 原名。
    #[must_use]
    pub fn monster_name(&self, language: Language, index: i32, database_name: &str) -> String {
        let key = index.to_string();
        self.pack(language)
            .or_else(|| self.pack(Language::ZhCn))
            .and_then(|pack| pack.monster_names.get(&key))
            .cloned()
            .unwrap_or_else(|| database_name.to_owned())
    }

    /// 取技能显示名：按 `MagicInfo.Spell` 查语言包，缺项回退 DB 原名。
    #[must_use]
    pub fn skill_name(&self, language: Language, spell: u8, database_name: &str) -> String {
        let key = spell.to_string();
        self.pack(language)
            .or_else(|| self.pack(Language::ZhCn))
            .and_then(|pack| pack.skill_names.get(&key))
            .cloned()
            .unwrap_or_else(|| database_name.to_owned())
    }

    /// 取任务显示名：按 `QuestInfo.Index` 查语言包，缺项回退 DB 原名。
    #[must_use]
    pub fn quest_name(&self, language: Language, index: i32, database_name: &str) -> String {
        let key = index.to_string();
        self.pack(language)
            .or_else(|| self.pack(Language::ZhCn))
            .and_then(|pack| pack.quest_names.get(&key))
            .cloned()
            .unwrap_or_else(|| database_name.to_owned())
    }
}

/// 一行匹配别名：同一个游戏概念的「DB 名 / 中文名 / 英文名」三列。
///
/// 只用于**匹配**（判断 `GameShopItem.Class` 里的职业写法、怪物名兜底是否命中），
/// 不是 UI 文案：玩家可见文案一律走 [`TextKey`] / 语言包，**禁止**把它当渲染词表用。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct NameAlias {
    /// 数据库里的原始名（英文，例如 `Warrior`、`ArcherGuard`）。
    pub db_name: String,
    /// 简体中文名（社区中文数据库使用，例如 `战士`、`弓箭守卫`）。
    #[serde(rename = "zh-cn")]
    pub zh_cn: String,
    /// 英文名（通常与 `db_name` 相同）。
    pub en: String,
}

impl NameAlias {
    /// 名字是否等于本行的任一名（`db_name` / `en` 大小写不敏感，`zh-cn` 精确比较）。
    #[must_use]
    pub fn matches(&self, name: &str) -> bool {
        self.db_name.eq_ignore_ascii_case(name)
            || self.en.eq_ignore_ascii_case(name)
            || self.zh_cn == name
    }

    /// 名字是否**包含**本行的任一名（怪物名兜底做 `contains` 匹配时用）。
    #[must_use]
    pub fn any_contains(&self, name: &str) -> bool {
        name.contains(&self.db_name) || name.contains(&self.en) || name.contains(&self.zh_cn)
    }
}

/// 匹配别名表（`config/i18n-aliases.json`）。
///
/// 语义三分里的「匹配别名」一档：`market.rs` 的五个职业名与 `monster.rs` 的
/// `ArcherGuard` 名字兜底共 6 条。它们不翻译给玩家看，只用来识别数据里的不同写法，
/// 所以单独放一份可替换配置、由代码加载；**不得塞进 UI 词表 / 语言包**。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MatchAliases {
    /// 配置版本（当前为 1；改结构时递增）。
    #[serde(default)]
    pub version: u32,
    /// 人读说明（这份表不是 UI 文案）。
    #[serde(default)]
    pub description: String,
    /// 游戏商店 `GameShopItem.Class` 的五个职业。
    #[serde(default)]
    pub game_shop_classes: Vec<NameAlias>,
    /// 怪物名匹配兜底（如 `AI 113` 的 `ArcherGuard`）。
    #[serde(default)]
    pub monster_names: Vec<NameAlias>,
}

impl MatchAliases {
    /// 解析一份别名表。
    ///
    /// # Errors
    /// JSON 非法时返回错误文本。
    pub fn from_json(source: &str) -> Result<Self, String> {
        serde_json::from_str(source).map_err(|error| error.to_string())
    }

    /// 内置别名表（编译进二进制，与 `LanguageLibrary::embedded` 同风格）。
    ///
    /// # Panics
    /// 仅当仓库里的 `config/i18n-aliases.json` 被改坏时才会 panic；该文件由测试覆盖。
    #[must_use]
    pub fn embedded() -> &'static Self {
        static ALIASES: OnceLock<MatchAliases> = OnceLock::new();
        ALIASES.get_or_init(|| {
            Self::from_json(include_str!("../../../config/i18n-aliases.json"))
                .expect("内置匹配别名表必须合法")
        })
    }

    /// 按英文职业名找别名行；找不到返回 `None`（调用方保持原有匹配语义）。
    #[must_use]
    pub fn game_shop_class(&self, english_class: &str) -> Option<&NameAlias> {
        self.game_shop_classes.iter().find(|alias| {
            alias.db_name.eq_ignore_ascii_case(english_class)
                || alias.en.eq_ignore_ascii_case(english_class)
        })
    }
}

/// 一个玩家的语言设置。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LanguageCatalog {
    language: Language,
}

impl LanguageCatalog {
    /// 用默认语言（简体中文）构造。
    #[must_use]
    pub const fn new() -> Self {
        Self {
            language: Language::ZhCn,
        }
    }

    /// 当前语言。
    #[must_use]
    pub const fn language(&self) -> Language {
        self.language
    }

    /// 直接用枚举设置。
    pub fn set(&mut self, language: Language) {
        self.language = language;
    }

    /// 切换语言；代码不认识时返回 `false`，保持原语言。
    pub fn switch(&mut self, code: &str) -> bool {
        match Language::from_code(code) {
            Some(language) => {
                self.language = language;
                true
            }
            None => false,
        }
    }

    /// 取当前语言下的内置文本（不走语言包）。
    ///
    /// 参数化键返回的是模板，用 [`LanguageCatalog::format_with`] 渲染。
    #[must_use]
    pub const fn text(&self, key: TextKey) -> &'static str {
        text(self.language, key)
    }

    /// 取当前语言下的文本，优先用语言包里的覆盖值。
    #[must_use]
    pub fn text_with(&self, library: &LanguageLibrary, key: TextKey) -> String {
        library.text(self.language, key)
    }

    /// 取当前语言下的成品文本并渲染参数，优先用语言包里的覆盖模板。
    #[must_use]
    pub fn format_with(
        &self,
        library: &LanguageLibrary,
        key: TextKey,
        args: &[(&str, &str)],
    ) -> String {
        library.format(self.language, key, args)
    }

    /// 解析游戏内的 `/language` 指令。
    ///
    /// 支持 `/language`（显示当前语言）与 `/language en`（切换）。
    /// 返回值是「切换后的语言」，`None` 表示这条消息不是语言指令。
    pub fn parse_command(&mut self, command: &str) -> Option<Result<Language, ()>> {
        let rest = command.strip_prefix("/language")?;
        let rest = rest.trim();
        if rest.is_empty() {
            return Some(Ok(self.language));
        }
        if self.switch(rest) {
            Some(Ok(self.language))
        } else {
            Some(Err(()))
        }
    }

    /// 处理游戏内的 `/language` 指令，返回要回给玩家的内置文本。
    pub fn handle_command(&mut self, command: &str) -> Option<&'static str> {
        match self.parse_command(command)? {
            Ok(_) => Some(self.text(TextKey::LanguageChanged)),
            Err(()) => Some(self.text(TextKey::UnknownLanguage)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashSet};

    use super::*;

    /// 模板里的命名占位符（按出现顺序；`{{`/`}}` 不算，空占位符不算）。
    fn placeholders(template: &str) -> Vec<&str> {
        let mut found = Vec::new();
        let mut rest = template;
        while let Some(open) = rest.find('{') {
            if let Some(after) = rest[open..].strip_prefix("{{") {
                rest = after;
                continue;
            }
            let after = &rest[open + 1..];
            let Some(close) = after.find('}') else {
                break;
            };
            let name = &after[..close];
            if !name.is_empty() && !name.contains('{') {
                found.push(name);
            }
            rest = &after[close + 1..];
        }
        found
    }

    #[test]
    fn default_language_is_simplified_chinese() {
        let catalog = LanguageCatalog::default();
        assert_eq!(catalog.language(), Language::ZhCn);
        assert_eq!(catalog.text(TextKey::Welcome), "欢迎来到传奇-Next。");
    }

    #[test]
    fn every_key_is_translated_in_every_language() {
        for language in Language::ALL {
            for key in TextKey::ALL {
                assert!(!text(*language, *key).is_empty(), "{language:?} {key:?}");
            }
        }
    }

    #[test]
    fn every_key_has_a_key_name() {
        for key in TextKey::ALL {
            assert!(!key.key().is_empty());
            assert!(key.key().is_ascii(), "语言包键名用 ASCII：{}", key.key());
        }
    }

    #[test]
    fn text_key_names_are_unique() {
        let mut names = BTreeSet::new();
        for key in TextKey::ALL {
            assert!(names.insert(key.key()), "重复的键名：{}", key.key());
        }
        assert_eq!(names.len(), TextKey::ALL.len());
    }

    #[test]
    fn text_key_all_has_no_duplicates() {
        let mut keys = HashSet::new();
        for key in TextKey::ALL {
            assert!(keys.insert(*key), "ALL 里重复：{}", key.key());
        }
        assert_eq!(keys.len(), TextKey::ALL.len());
    }

    #[test]
    fn text_key_all_is_complete() {
        // 第 1 轮 11 个键 + `docs/i18n.md` §7.2 的 192 个键 + 4 个行会权限扩展键
        // （`guild_not_allowed_*`，Crystal `ServerTextKeys.NotAllowed*`；§7.2 里
        // `guild_no_permission` 那一行仍保留，作未知权限兜底）+ 7 个 NPC 脚本动作键
        // （`npc_*`：`TAKECHECKITEM` / `GIVENAMELIST` / `GIVEBUFF` 的玩家可见提示）。
        //
        // 新增变体时必须同时登记 `TextKey::ALL`，否则这条会挡住——`ALL` 是
        // 语言包校验与参数核对的唯一入口，漏登记等于漏测试。
        //
        // 第 24 轮又追加 2 个「任务接取失败」键（`quest_too_many` /
        // `quest_accept_failed`，对齐 Crystal `AcceptQuest` 的 `ReceiveChat`），故 234 → 236。
        // 第 25 轮再追加 3 个玩家可见失败反馈键（`pickup_failed` / `buy_failed` /
        // `quest_finish_failed`），236 → 239；第 26 轮 TP-1 追加
        // `quest_navigation_go_to` / `quest_navigation_none`，239 → 241。
        assert_eq!(TextKey::ALL.len(), 241, "新增/删除键时同步更新数量");
    }

    #[test]
    fn every_key_has_same_placeholders_in_every_language() {
        for key in TextKey::ALL {
            let zh = placeholders(text(Language::ZhCn, *key));
            let en = placeholders(text(Language::En, *key));
            assert_eq!(
                zh.iter().collect::<BTreeSet<_>>().len(),
                zh.len(),
                "{} 的中文模板有重复占位符",
                key.key()
            );
            assert_eq!(
                en.iter().collect::<BTreeSet<_>>().len(),
                en.len(),
                "{} 的英文模板有重复占位符",
                key.key()
            );
            let mut zh_sorted = zh.clone();
            zh_sorted.sort_unstable();
            let mut en_sorted = en.clone();
            en_sorted.sort_unstable();
            let mut declared = key.args().to_vec();
            declared.sort_unstable();
            assert_eq!(zh_sorted, en_sorted, "{} 的中英占位符不一致", key.key());
            assert_eq!(
                zh_sorted,
                declared,
                "{} 的 args() 与模板占位符不一致",
                key.key()
            );
        }
    }

    #[test]
    fn placeholder_arguments_are_snake_case() {
        for key in TextKey::ALL {
            for name in key.args() {
                assert!(!name.is_empty(), "{} 有空的参数名", key.key());
                assert!(
                    name.bytes().all(|byte| byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || byte == b'_'),
                    "{} 的参数 {name} 必须是 ASCII snake_case",
                    key.key()
                );
                assert!(
                    !name.starts_with('_') && !name.ends_with('_'),
                    "{} 的参数 {name} 不能以下划线开头或结尾",
                    key.key()
                );
            }
        }
    }

    #[test]
    fn format_text_replaces_every_declared_placeholder() {
        for key in TextKey::ALL {
            let names = key.args();
            let values: Vec<String> = names.iter().map(|name| format!("<{name}>")).collect();
            let args: Vec<(&str, &str)> = names
                .iter()
                .copied()
                .zip(values.iter().map(String::as_str))
                .collect();
            for language in Language::ALL {
                let rendered = format_text(*language, *key, &args);
                assert!(!rendered.is_empty(), "{} {language:?}", key.key());
                for name in names {
                    assert!(
                        !rendered.contains(&format!("{{{name}}}")),
                        "{} {language:?} 漏替换了 {{{name}}}",
                        key.key()
                    );
                }
                for value in &values {
                    assert!(
                        rendered.contains(value.as_str()),
                        "{} {language:?} 渲染结果里没有 {value}",
                        key.key()
                    );
                }
            }
        }
    }

    #[test]
    fn format_text_keeps_missing_placeholders() {
        let key = TextKey::MailNotEnoughGold;
        assert_eq!(
            format_text(Language::ZhCn, key, &[("needed", "100")]),
            "金币不足：需要 100，你只有 {held}。"
        );
        assert_eq!(
            format_text(Language::En, key, &[("needed", "100")]),
            "Not enough Gold: requires 100, you have {held}."
        );
        // 一个实参都不给时就是模板本身。
        assert_eq!(
            format_text(Language::ZhCn, key, &[]),
            text(Language::ZhCn, key)
        );
    }

    #[test]
    fn format_text_ignores_extra_arguments() {
        assert_eq!(
            format_text(
                Language::ZhCn,
                TextKey::TradeSuccess,
                &[("gold", "1"), ("name", "x")]
            ),
            text(Language::ZhCn, TextKey::TradeSuccess)
        );
        // 参数化键的多余实参同样被忽略。
        assert_eq!(
            format_text(
                Language::ZhCn,
                TextKey::MarketBought,
                &[("gold", "100"), ("item", "炼狱"), ("unused", "ignored")]
            ),
            "你以 100 金币买下了 炼狱。"
        );
    }

    #[test]
    fn format_text_renders_literal_braces() {
        assert_eq!(render_template("{{gold}}", &[("gold", "1")]), "{gold}");
        assert_eq!(render_template("{gold}}", &[("gold", "1")]), "1}");
        assert_eq!(render_template("{}", &[("gold", "1")]), "{}");
        assert_eq!(render_template("{unknown}", &[("gold", "1")]), "{unknown}");
        assert_eq!(render_template("", &[]), "");
        assert_eq!(
            render_template("没有占位符", &[("gold", "1")]),
            "没有占位符"
        );
    }

    #[test]
    fn format_text_without_arguments_matches_text() {
        for key in TextKey::ALL {
            if !key.args().is_empty() {
                continue;
            }
            for language in Language::ALL {
                assert_eq!(
                    format_text(*language, *key, &[]),
                    text(*language, *key),
                    "{} {language:?}",
                    key.key()
                );
                // 多余实参也不该改变无参键的结果。
                assert_eq!(
                    format_text(*language, *key, &[("unused", "x")]),
                    text(*language, *key),
                    "{} {language:?}",
                    key.key()
                );
            }
        }
    }

    #[test]
    fn round_one_keys_keep_their_wording() {
        // 第 1 轮验收过的 11 条文案：本轮不改一个字（防回归）。
        let expected = [
            (
                TextKey::Welcome,
                "欢迎来到传奇-Next。",
                "Welcome to Mir-Next.",
            ),
            (
                TextKey::AccountBanned,
                "该账号已被封禁。",
                "This account is banned.",
            ),
            (
                TextKey::WrongPassword,
                "账号或密码错误。",
                "Incorrect account or password.",
            ),
            (
                TextKey::TooManyCharacters,
                "角色数量已达上限。",
                "You have too many characters.",
            ),
            (
                TextKey::LanguageChanged,
                "语言已切换为简体中文。",
                "Language switched to English.",
            ),
            (
                TextKey::UnknownLanguage,
                "未知的语言代码，可用：zh-cn、en。",
                "Unknown language code. Available: zh-cn, en.",
            ),
            (
                TextKey::ServerFull,
                "服务器人数已满，请稍后再试。",
                "The server is full, please try again later.",
            ),
            (
                TextKey::UnsupportedCommand,
                "不支持的命令。",
                "Unsupported command.",
            ),
            (
                TextKey::CharacterNameTaken,
                "该角色名已被占用。",
                "That character name is already taken.",
            ),
            (
                TextKey::CharacterNameInvalid,
                "角色名不合法：3-15 个汉字、字母、数字或下划线。",
                "Invalid character name: 3-15 CJK characters, letters, digits or underscores.",
            ),
            (TextKey::LoginSuccess, "登录成功。", "Login successful."),
        ];
        for (key, zh, en) in expected {
            assert_eq!(text(Language::ZhCn, key), zh);
            assert_eq!(text(Language::En, key), en);
            assert!(key.args().is_empty(), "{} 不该有参数", key.key());
        }
    }

    #[test]
    fn embedded_language_packs_cover_every_key() {
        let library = LanguageLibrary::embedded();
        assert_eq!(library.languages().len(), 2, "内置中英两份语言包");
        for language in Language::ALL {
            let pack = library.pack(*language).expect("语言包存在");
            assert_eq!(pack.language(), *language);
            for key in TextKey::ALL {
                assert!(
                    pack.server.contains_key(key.key()),
                    "{language:?} 语言包缺少 {}",
                    key.key()
                );
                assert!(
                    !pack.server[key.key()].is_empty(),
                    "{language:?} 语言包 {} 的值为空",
                    key.key()
                );
            }
        }
    }

    #[test]
    fn embedded_pack_matches_builtin_text() {
        // 语言包是内置模板的镜像；改一边忘了另一边时这条会拦住。
        let library = LanguageLibrary::embedded();
        for language in Language::ALL {
            for key in TextKey::ALL {
                assert_eq!(
                    library.text(*language, *key),
                    text(*language, *key),
                    "{language:?} {key:?}"
                );
            }
        }
    }

    #[test]
    fn language_packs_have_identical_key_sets() {
        let library = LanguageLibrary::embedded();
        let zh = library.pack(Language::ZhCn).expect("中文语言包存在");
        let en = library.pack(Language::En).expect("英文语言包存在");
        let zh_keys: Vec<&str> = zh.server.keys().map(String::as_str).collect();
        let en_keys: Vec<&str> = en.server.keys().map(String::as_str).collect();
        assert_eq!(zh_keys, en_keys, "中英语言包的键集合必须完全一致");
    }

    #[test]
    fn language_packs_have_no_unknown_keys() {
        let library = LanguageLibrary::embedded();
        let mut declared: Vec<&str> = TextKey::ALL.iter().map(|key| key.key()).collect();
        declared.sort_unstable();
        for language in Language::ALL {
            let pack = library.pack(*language).expect("语言包存在");
            let keys: Vec<&str> = pack.server.keys().map(String::as_str).collect();
            assert_eq!(
                keys, declared,
                "{language:?} 语言包里有未登记的键（或漏了键）"
            );
        }
    }

    #[test]
    fn guild_permission_names_render_naturally_in_both_languages() {
        // `{permission}` 不能露出内部标识符；未知标识返回 None 让调用方兜底。
        for permission in ["CanRecruit", "CanKick", "CanChangeRank", "CanChangeNotice"] {
            for language in Language::ALL {
                let name = guild_permission_name(*language, permission).expect("已知权限有名称");
                let rendered = format_text(
                    *language,
                    TextKey::GuildNoPermission,
                    &[("permission", name)],
                );
                assert!(
                    !rendered.contains("Can"),
                    "{language:?} 渲染出了内部标识符：{rendered}"
                );
            }
        }
        // 兜底模板保留 §7.2 原文里的空格（`没有 {permission} 权限`）；
        // 已知权限走 4 个 `guild_not_allowed_*` 扩展键，文案里没有空格。
        assert_eq!(
            format_text(
                Language::ZhCn,
                TextKey::GuildNoPermission,
                &[(
                    "permission",
                    guild_permission_name(Language::ZhCn, "CanRecruit").expect("已知")
                )]
            ),
            "你的职位没有 招收成员 权限"
        );
        assert_eq!(
            format_text(
                Language::En,
                TextKey::GuildNoPermission,
                &[(
                    "permission",
                    guild_permission_name(Language::En, "CanKick").expect("已知")
                )]
            ),
            "Your rank does not have permission to kick members."
        );
        assert_eq!(guild_permission_name(Language::En, "Unknown"), None);
    }

    #[test]
    fn guild_permission_extension_keys_are_specific() {
        // 4 个扩展键：具体场景、无参、中英文案与 Crystal 的 `NotAllowed*` 对齐。
        let expected = [
            (
                TextKey::GuildNotAllowedRecruit,
                "你的职位没有招收成员的权限",
                "Your rank is not allowed to recruit members.",
            ),
            (
                TextKey::GuildNotAllowedKick,
                "你的职位没有踢出成员的权限",
                "Your rank is not allowed to remove members.",
            ),
            (
                TextKey::GuildNotAllowedChangeRank,
                "你的职位没有修改成员职位的权限",
                "Your rank is not allowed to change member ranks.",
            ),
            (
                TextKey::GuildNotAllowedChangeNotice,
                "你的职位没有修改行会公告的权限",
                "Your rank is not allowed to change the guild notice.",
            ),
        ];
        for (key, zh, en) in expected {
            assert!(key.args().is_empty(), "{} 不该有参数", key.key());
            assert_eq!(text(Language::ZhCn, key), zh, "{}", key.key());
            assert_eq!(text(Language::En, key), en, "{}", key.key());
            assert_eq!(format_text(Language::ZhCn, key, &[]), zh, "{}", key.key());
        }
    }

    #[test]
    fn language_library_formats_like_builtin_templates() {
        let library = LanguageLibrary::embedded();
        let args = [("item", "炼狱"), ("gold", "100")];
        for key in TextKey::ALL {
            for language in Language::ALL {
                assert_eq!(
                    library.format(*language, *key, &args),
                    format_text(*language, *key, &args),
                    "{} {language:?}",
                    key.key()
                );
            }
        }
    }

    #[test]
    fn language_catalog_formats_in_current_language() {
        let library = LanguageLibrary::embedded();
        let mut catalog = LanguageCatalog::new();
        assert_eq!(
            catalog.format_with(
                &library,
                TextKey::MarketBought,
                &[("gold", "1"), ("item", "A")]
            ),
            "你以 1 金币买下了 A。"
        );
        assert!(catalog.switch("en"));
        assert_eq!(
            catalog.format_with(
                &library,
                TextKey::MarketBought,
                &[("gold", "1"), ("item", "A")]
            ),
            "You bought A for 1 Gold."
        );
    }

    #[test]
    fn missing_pack_falls_back_to_builtin_text() {
        let library = LanguageLibrary::default();
        assert_eq!(
            library.text(Language::En, TextKey::Welcome),
            "Welcome to Mir-Next."
        );
    }

    #[test]
    fn map_titles_are_localized() {
        let library = LanguageLibrary::embedded();
        assert_eq!(library.map_title(Language::ZhCn, "0"), "比奇省");
        assert_eq!(library.map_title(Language::En, "0"), "Bichon Province");
        // 未收录的地图用文件名兜底，不会 panic。
        assert_eq!(
            library.map_title(Language::En, "unknown-map"),
            "unknown-map"
        );
    }

    #[test]
    fn switching_by_code_works_and_rejects_garbage() {
        let mut catalog = LanguageCatalog::new();
        assert!(catalog.switch("EN"));
        assert_eq!(catalog.language(), Language::En);
        assert!(catalog.switch("zh-cn"));
        assert_eq!(catalog.language(), Language::ZhCn);
        assert!(!catalog.switch("klingon"));
        assert_eq!(catalog.language(), Language::ZhCn, "非法代码不应改变语言");
    }

    #[test]
    fn in_game_command_switches_language() {
        let mut catalog = LanguageCatalog::new();
        assert_eq!(
            catalog.handle_command("/language en"),
            Some("Language switched to English.")
        );
        assert_eq!(catalog.language(), Language::En);
        assert_eq!(
            catalog.handle_command("/language"),
            Some("Language switched to English.")
        );
        assert_eq!(
            catalog.handle_command("/language tlh"),
            Some("Unknown language code. Available: zh-cn, en.")
        );
        assert_eq!(catalog.handle_command("/say hi"), None);
    }

    #[test]
    fn language_pack_rejects_unknown_code() {
        let source = r#"{"language":"tlh","server":{},"maps":{}}"#;
        assert!(LanguagePack::from_json(source).is_err());
    }

    #[test]
    fn generated_name_packs_are_localized_and_fallback() {
        let library = LanguageLibrary::embedded();
        assert_eq!(library.monster_name(Language::ZhCn, 13, "Clone"), "分身");
        assert_eq!(library.monster_name(Language::En, 13, "Clone"), "Clone");
        assert_eq!(library.skill_name(Language::ZhCn, 54, "Blink"), "瞬息移动");
        assert_eq!(library.skill_name(Language::En, 54, "Blink"), "Blink");
        assert_eq!(
            library.quest_name(Language::ZhCn, 152, "AncientOmaCavern"),
            "远古奥玛洞穴"
        );
        // 未知 index 必须回退 DB 原名，不能 panic。
        assert_eq!(
            library.monster_name(Language::ZhCn, 999_999, "UnknownMob"),
            "UnknownMob"
        );
        assert_eq!(
            library.skill_name(Language::ZhCn, 255, "UnknownSkill"),
            "UnknownSkill"
        );
        assert_eq!(
            library.quest_name(Language::ZhCn, -1, "UnknownQuest"),
            "UnknownQuest"
        );
    }

    #[test]
    fn monster_template_display_names_are_complete_and_unique() {
        assert_eq!(MonsterTemplate::ALL.len(), 4);
        let mut keys = BTreeSet::new();
        for template in MonsterTemplate::ALL {
            assert!(
                keys.insert(template.stable_key()),
                "模板键必须唯一：{template:?}"
            );
            assert_eq!(
                MonsterTemplate::from_key(template.stable_key()),
                Some(*template),
                "模板键必须能解析回来：{}",
                template.stable_key()
            );
            let zh = monster_display_name(Language::ZhCn, *template);
            let en = monster_display_name(Language::En, *template);
            assert!(!zh.is_empty(), "模板 {template:?} 缺中文显示名");
            assert!(!en.is_empty(), "模板 {template:?} 缺英文显示名");
            assert_ne!(zh, en, "模板 {template:?} 中英显示名不能相同");
            assert!(
                en.is_ascii(),
                "模板 {template:?} 的英文名必须全 ASCII：{en}"
            );
        }
        assert_eq!(MonsterTemplate::from_key("no_such_template"), None);
    }

    #[test]
    fn monster_template_data_names_match_the_cjlaaa_database() {
        // 数据名（中文）必须与 cjlaaa `MonsterInfo.Name` 一致：数据名参与查库与匹配，
        // 显示名另走词表；这 4 个模板没有 DB 行，所以内置一份中文数据名。
        assert_eq!(
            monster_display_name(Language::ZhCn, MonsterTemplate::Skeleton),
            "骷髅"
        );
        assert_eq!(
            monster_display_name(Language::ZhCn, MonsterTemplate::BoneFamiliar),
            "骷髅"
        );
        assert_eq!(
            monster_display_name(Language::ZhCn, MonsterTemplate::Scarecrow),
            "稻草人"
        );
        assert_eq!(
            monster_display_name(Language::ZhCn, MonsterTemplate::Shinsu),
            "神兽"
        );
    }

    #[test]
    fn embedded_match_aliases_cover_the_six_known_aliases() {
        let aliases = MatchAliases::embedded();
        assert_eq!(aliases.version, 1);
        assert!(!aliases.description.is_empty());
        assert_eq!(aliases.game_shop_classes.len(), 5);
        assert_eq!(aliases.monster_names.len(), 1);
        let expected = [
            ("Warrior", "战士", "Warrior"),
            ("Wizard", "法师", "Wizard"),
            ("Taoist", "道士", "Taoist"),
            ("Assassin", "刺客", "Assassin"),
            ("Archer", "弓箭手", "Archer"),
        ];
        for (db_name, zh_cn, en) in expected {
            let row = aliases
                .game_shop_class(db_name)
                .unwrap_or_else(|| panic!("缺少职业别名 {db_name}"));
            assert_eq!(row.db_name, db_name);
            assert_eq!(row.zh_cn, zh_cn);
            assert_eq!(row.en, en);
            for name in [row.db_name.as_str(), row.zh_cn.as_str(), row.en.as_str()] {
                assert!(row.matches(name), "{db_name} 的三列都必须能自匹配：{name}");
            }
            assert!(!row.matches("不存在的职业"));
        }
        // 英文职业名大小写不敏感（保持 game_shop_class_allows 旧语义）。
        assert_eq!(
            aliases
                .game_shop_class("wArRiOr")
                .map(|row| row.zh_cn.as_str()),
            Some("战士")
        );
        let archer_guard = &aliases.monster_names[0];
        assert_eq!(archer_guard.db_name, "ArcherGuard");
        assert_eq!(archer_guard.zh_cn, "弓箭守卫");
        assert!(archer_guard.any_contains("ArcherGuard"));
        assert!(archer_guard.any_contains("弓箭守卫"));
        assert!(archer_guard.any_contains("EliteArcherGuard"));
        assert!(!archer_guard.any_contains("神兽"));
    }

    #[test]
    fn match_alias_rows_are_unique_and_complete() {
        let aliases = MatchAliases::embedded();
        let mut db_names = BTreeSet::new();
        let mut zh_names = BTreeSet::new();
        let mut count = 0;
        for row in aliases
            .game_shop_classes
            .iter()
            .chain(aliases.monster_names.iter())
        {
            count += 1;
            assert!(!row.db_name.trim().is_empty(), "DB 名列不得为空");
            assert!(
                !row.zh_cn.trim().is_empty(),
                "中文名列不得为空：{}",
                row.db_name
            );
            assert!(
                !row.en.trim().is_empty(),
                "英文名列不得为空：{}",
                row.db_name
            );
            assert!(
                db_names.insert(row.db_name.to_ascii_lowercase()),
                "DB 名重复：{}",
                row.db_name
            );
            assert!(
                zh_names.insert(row.zh_cn.clone()),
                "中文名重复：{}",
                row.zh_cn
            );
        }
        assert_eq!(count, 6, "6 条匹配别名：五职业 + ArcherGuard");
    }

    #[test]
    fn match_aliases_are_not_ui_text_keys() {
        let aliases = MatchAliases::embedded();
        let text_keys: BTreeSet<&str> = TextKey::ALL.iter().map(|key| key.key()).collect();
        let library = LanguageLibrary::embedded();
        for row in aliases
            .game_shop_classes
            .iter()
            .chain(aliases.monster_names.iter())
        {
            for name in [&row.db_name, &row.zh_cn, &row.en] {
                assert!(
                    !text_keys.contains(name.as_str()),
                    "{name} 不得出现在 TextKey：匹配别名严禁当 UI 词表"
                );
                for language in Language::ALL {
                    let pack = library.pack(*language).expect("内置语言包必须存在");
                    assert!(
                        !pack.server.values().any(|template| template == name),
                        "{name} 不得出现在 {language:?} 语言包 server 段"
                    );
                }
            }
        }
    }

    #[test]
    fn match_aliases_json_requires_the_kebab_case_chinese_column() {
        let source = r#"{"version":1,"description":"","game_shop_classes":[{"db_name":"Warrior","zh-cn":"战士","en":"Warrior"}],"monster_names":[]}"#;
        let parsed = MatchAliases::from_json(source).expect("合法别名表");
        assert_eq!(parsed.game_shop_classes[0].zh_cn, "战士");
        assert!(MatchAliases::from_json("{ 不是 json }").is_err());
        assert!(
            MatchAliases::from_json(r#"{"game_shop_classes":[{"db_name":"X"}]}"#).is_err(),
            "缺 zh-cn/en 列必须报错"
        );
    }
}
