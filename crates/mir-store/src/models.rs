//! 账号与角色的数据模型。
//!
//! 字段名是 Crystal 字段的 `snake_case` 转写（`AccountInfo.AccountID` → `username` 之外
//! 全部一一对应，例外在字段文档里注明）。枚举类字段（职业、性别、发型、方向、攻击模式等）
//! 在这里一律存整数，取值与 Crystal `Shared/Enums.cs` 的枚举值相同；转换成
//! `mir-core` 的枚举由上层完成，存储层不依赖世界规则。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// 默认界面语言：简体中文。
///
/// 与 `migrations/0002_account_language.sql` 里 `accounts.language` 的 `DEFAULT 'zh-cn'`
/// 以及 `mir-core` 的 `Language::ZhCn` 保持一致；全部受支持的语言代码见
/// [`crate::normalize_language`]。
pub const DEFAULT_LANGUAGE: &str = "zh-cn";

/// 新账号的创建参数。
///
/// [`password`](Self::password) 是明文口令，只在 [`crate::create_account`] 内部参与散列，
/// 不会写入数据库；写完后的行由 [`Account`] 表示。
#[derive(Debug, Clone)]
pub struct NewAccount {
    /// 登录名（Crystal `AccountID`），大小写不敏感唯一。
    pub username: String,
    /// 明文口令；散列算法与 Crystal 完全一致，见 [`crate::hash_password`]。
    pub password: String,
    /// 邮箱（Crystal `EMailAddress`）。
    pub email: String,
    /// 注册 IP（Crystal `CreationIP`）；未知时传空串。
    pub creation_ip: String,
    /// 是否为 GM 账号（Crystal `AdminAccount`）。
    pub admin: bool,
    /// 界面语言代码，默认 [`DEFAULT_LANGUAGE`]（`zh-cn`，简体中文）。
    ///
    /// 接受 `zh-cn`/`zh`/`cn`/`chs`、`en`/`en-us` 及其大小写变体，
    /// 落库前由 [`crate::normalize_language`] 归一成 `zh-cn`/`en`；
    /// 无法识别的代码会让 [`crate::create_account`] 返回
    /// [`StoreError::InvalidLanguage`](crate::StoreError::InvalidLanguage)。
    pub language: String,
}

impl NewAccount {
    /// 用最少的参数构造：邮箱、注册 IP 为空，非 GM，界面语言取 [`DEFAULT_LANGUAGE`]。
    pub fn new(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            username: username.into(),
            password: password.into(),
            email: String::new(),
            creation_ip: String::new(),
            admin: false,
            language: DEFAULT_LANGUAGE.to_owned(),
        }
    }
}

/// 账号行（Crystal `AccountInfo`）。
///
/// 只包含「登录 + 账号状态」需要的字段；密保资料、仓库二级密码、扩建仓库等尚未迁移，
/// 见 crate 文档的「刻意延后」一节。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct Account {
    /// 账号主键（Crystal `AccountInfo.Index`）。
    pub id: i64,
    /// 登录名（Crystal `AccountID`）；唯一性不区分大小写。
    pub username: String,
    /// Crystal `password`：PBKDF2 结果的 UTF-8 替换解码串，再按标准 Base64 编码后的
    /// ASCII 文本（见 [`crate::encode_hash`]）。
    ///
    /// 旧版本直接把 lossy 字符串写进本列；当派生物含 NUL 时 PostgreSQL 报 22021，
    /// 迁移 `0010_password_hash_base64.sql` 已把旧行统一转成 Base64。新代码只会写 Base64。
    pub password_hash: String,
    /// 口令盐（Crystal `Salt`），24 字节。
    pub salt: Vec<u8>,
    /// 邮箱（Crystal `EMailAddress`）。
    pub email: String,
    /// 注册 IP（Crystal `CreationIP`）。
    pub creation_ip: String,
    /// 注册时间（Crystal `CreationDate`）。
    pub creation_date: DateTime<Utc>,
    /// 是否封禁（Crystal `Banned`）。
    pub banned: bool,
    /// 封禁原因（Crystal `BanReason`）。
    pub ban_reason: String,
    /// 封禁/锁定到期时间（Crystal `ExpiryDate`）；`None` 表示无到期时间。
    pub expiry_date: Option<DateTime<Utc>>,
    /// 下次登录是否必须改密（Crystal `RequirePasswordChange`）。
    pub require_password_change: bool,
    /// 连续错误口令次数（Crystal `WrongPasswordCount`；原版不落盘，这里落库）。
    pub wrong_password_count: i32,
    /// 最后登录 IP（Crystal `LastIP`）。
    pub last_ip: String,
    /// 最后登录时间（Crystal `LastDate`）。
    pub last_date: Option<DateTime<Utc>>,
    /// GM 账号标志（Crystal `AdminAccount`）。
    pub admin: bool,
    /// 界面语言的 BCP-47 小写代码（目前是 `zh-cn` 或 `en`），默认 [`DEFAULT_LANGUAGE`]。
    ///
    /// 账号级设置：与角色无关，登录后用来初始化客户端的语言目录，因此换角色或重登
    /// 都保留同一个选择。读出的行一定已归一化；改语言请走 [`crate::set_account_language`]。
    pub language: String,
}

impl Default for Account {
    /// 空账号，界面语言取 [`DEFAULT_LANGUAGE`]（`zh-cn`，简体中文），其余字段取空值。
    ///
    /// 主要给测试和「先建空行再逐字段填」的场景用；正常读账号走 [`crate::find_account`]。
    fn default() -> Self {
        Self {
            id: 0,
            username: String::new(),
            password_hash: String::new(),
            salt: Vec::new(),
            email: String::new(),
            creation_ip: String::new(),
            creation_date: DateTime::<Utc>::default(),
            banned: false,
            ban_reason: String::new(),
            expiry_date: None,
            require_password_change: false,
            wrong_password_count: 0,
            last_ip: String::new(),
            last_date: None,
            admin: false,
            language: DEFAULT_LANGUAGE.to_owned(),
        }
    }
}

impl Account {
    /// 账号此刻是否处于封禁期。
    ///
    /// 与 Crystal 登录流程一致：`Banned` 为真且 `ExpiryDate` 为空或晚于 `now` 时拒绝登录；
    /// 到期后调用方可以清除 `banned`。
    pub fn is_banned(&self, now: DateTime<Utc>) -> bool {
        self.banned && self.expiry_date.is_none_or(|expiry_date| expiry_date > now)
    }

    /// 登录门禁判断（纯函数，不改状态）。
    ///
    /// 对应 Crystal `Envir.cs` 登录流程里的两个检查：封禁 `Envir.cs:3855-3878` 附近，
    /// 强制改密 `Envir.cs:3905-3909`。连续错密码后的自动封禁是服务器策略，不在这里做。
    pub fn login_gate(&self, now: DateTime<Utc>) -> LoginGate {
        if self.is_banned(now) {
            LoginGate::Banned
        } else if self.require_password_change {
            LoginGate::PasswordChangeRequired
        } else {
            LoginGate::Allowed
        }
    }
}

/// 账号登录门禁结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoginGate {
    /// 允许登录。
    Allowed,
    /// 账号处于封禁期。
    Banned,
    /// 必须先完成改密流程。
    PasswordChangeRequired,
}

/// 新建角色的参数。
#[derive(Debug, Clone)]
pub struct NewCharacter {
    /// 归属账号主键。
    pub account_id: i64,
    /// 角色名，大小写不敏感唯一。
    pub name: String,
    /// 职业，取值同 Crystal `MirClass`。
    pub class: i16,
    /// 性别，取值同 Crystal `MirGender`。
    pub gender: i16,
    /// 发型，取值同 Crystal `Hair`。
    pub hair: i16,
    /// 建号 IP（Crystal `CreationIP`）。
    pub creation_ip: String,
}

impl NewCharacter {
    /// 构造一个发型为 0、注册 IP 为空的建号请求。
    ///
    /// 出生点、初始 HP/MP 等由服务器在创建后通过 [`crate::save_character`] 写入，
    /// 与 Crystal「建号只记录姓名/职业/性别」的流程一致（`Envir.cs:4062`）。
    pub fn new(account_id: i64, name: impl Into<String>, class: i16, gender: i16) -> Self {
        Self {
            account_id,
            name: name.into(),
            class,
            gender,
            hair: 0,
            creation_ip: String::new(),
        }
    }
}

/// 角色行（Crystal `CharacterInfo`）。
///
/// 位置用 `current_map_index` + `current_x`/`current_y` 平铺，等价于 Crystal 的
/// `CurrentMapIndex` + `CurrentLocation`（`Point`）。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, FromRow)]
pub struct Character {
    /// 角色主键（Crystal `CharacterInfo.Index`）。
    pub id: i64,
    /// 归属账号（Crystal `CharacterInfo.AccountInfo`）。
    pub account_id: i64,
    /// 角色名（Crystal `Name`），唯一性不区分大小写。
    pub name: String,
    /// 等级（Crystal `Level`，`ushort`）。
    pub level: i16,
    /// 职业（Crystal `Class`，取值同 `MirClass`）。
    pub class: i16,
    /// 性别（Crystal `Gender`，取值同 `MirGender`）。
    pub gender: i16,
    /// 发型（Crystal `Hair`）。
    pub hair: i16,
    /// 建号 IP（Crystal `CreationIP`）。
    pub creation_ip: String,
    /// 建号时间（Crystal `CreationDate`）。
    pub creation_date: DateTime<Utc>,
    /// 角色级封禁（Crystal `Banned`）。
    pub banned: bool,
    /// 封禁原因（Crystal `BanReason`）。
    pub ban_reason: String,
    /// 封禁到期时间（Crystal `ExpiryDate`）。
    pub expiry_date: Option<DateTime<Utc>>,
    /// 最后登录 IP（Crystal `LastIP`）。
    pub last_ip: String,
    /// 最后登录时间（Crystal `LastLoginDate`）。
    pub last_login_date: Option<DateTime<Utc>>,
    /// 最后登出时间（Crystal `LastLogoutDate`）。
    pub last_logout_date: Option<DateTime<Utc>>,
    /// 软删除标记（Crystal `Deleted`）。
    pub deleted: bool,
    /// 软删除时间（Crystal `DeleteDate`）。
    pub delete_date: Option<DateTime<Utc>>,
    /// 当前地图（Crystal `CurrentMapIndex`）。
    pub current_map_index: i32,
    /// 当前 X（Crystal `CurrentLocation.X`）。
    pub current_x: i32,
    /// 当前 Y（Crystal `CurrentLocation.Y`）。
    pub current_y: i32,
    /// 朝向（Crystal `Direction`，取值同 `MirDirection`）。
    pub direction: i16,
    /// 回城点地图（Crystal `BindMapIndex`）。
    pub bind_map_index: i32,
    /// 回城点 X（Crystal `BindLocation.X`）。
    pub bind_x: i32,
    /// 回城点 Y（Crystal `BindLocation.Y`）。
    pub bind_y: i32,
    /// 当前 HP（Crystal `HP`）。
    pub hp: i32,
    /// 当前 MP（Crystal `MP`）。
    pub mp: i32,
    /// 经验（Crystal `Experience`）。
    pub experience: i64,
    /// 金币（任务要求放在角色行；Crystal 实际存于 `AccountInfo.Gold`）。
    pub gold: i64,
    /// 充值点数（任务要求放在角色行；Crystal 实际存于 `AccountInfo.Credit`）。
    pub credit: i64,
    /// 攻击模式（Crystal `AMode`，取值同 `AttackMode`）。
    pub attack_mode: i16,
    /// 宠物模式（Crystal `PMode`，取值同 `PetMode`）。
    pub pet_mode: i16,
    /// PK 值（Crystal `PKPoints`）。
    pub pk_points: i32,
    /// 是否允许组队（Crystal `AllowGroup`）。
    pub allow_group: bool,
    /// 是否允许交易（Crystal `AllowTrade`）。
    pub allow_trade: bool,
    /// 是否允许观察（Crystal `AllowObserve`）。
    pub allow_observe: bool,
    /// 行会主键，`-1` 表示没有行会（Crystal `GuildIndex`）。
    pub guild_index: i32,
    /// 背包（Crystal `Inventory` 的 `UserItem[]` 二进制整块；空数组 = 零长度）。
    pub inventory: Vec<u8>,
    /// 装备（Crystal `Equipment` 的 `UserItem[]` 二进制整块）。
    pub equipment: Vec<u8>,
    /// 已学技能（Crystal `Magics` 的整块二进制；格式见
    /// `crates/mir-server/src/magic.rs` 的 `encode_magics`/`decode_magics`）。
    pub magics: Vec<u8>,
    /// 任务进度（Crystal `CurrentQuests` + `CompletedQuests` 的整块二进制；格式见
    /// `crates/mir-server/src/quests.rs` 的 `encode_quests`/`decode_quests`）。
    pub quests: Vec<u8>,
    /// 任务物品栏（Crystal `CharacterInfo.QuestInventory`，`CharacterInfo.cs:75` 的
    /// `UserItem[40]` 整块二进制；格式见 `crates/mir-server/src/quest_inventory.rs` 的
    /// `QuestInventory::encode`/`QuestInventory::decode`）。
    ///
    /// 空数组（没有任务物品）是零长度；Crystal 的存档在 `CharacterInfo.cs:449-456`。
    pub quest_inventory: Vec<u8>,
    /// 上次每日任务重置的时间（Unix 毫秒，迁移 0013）。
    ///
    /// 0 表示从未重置过；`quests::daily_reset_due` 按 24h 间隔惰性判定，登录/任务交互时补做。
    pub last_daily_reset_at: i64,
    /// 仓库（任务要求放在角色行；Crystal 实际存于 `AccountInfo.Storage`）。
    pub storage: Vec<u8>,
    /// 配偶角色主键，0 表示未婚（Crystal `Married`）。
    pub married: i32,
    /// 结婚时间（Crystal `MarriedDate`）。
    pub married_date: Option<DateTime<Utc>>,
    /// 师徒关系里的对方角色主键，0 表示没有（Crystal `Mentor`）。
    pub mentor: i32,
    /// 拜师/收徒时间（Crystal `MentorDate`）。
    pub mentor_date: Option<DateTime<Utc>>,
    /// 该角色是否为师傅方（Crystal `IsMentor`）。
    pub is_mentor: bool,
    /// 师徒经验池（Crystal `MentorExp`）。
    pub mentor_exp: i64,
}

impl Character {
    /// 角色此刻是否处于封禁期，语义同 [`Account::is_banned`]。
    pub fn is_banned(&self, now: DateTime<Utc>) -> bool {
        self.banned && self.expiry_date.is_none_or(|expiry_date| expiry_date > now)
    }

    /// 转成选角界面需要的精简视图。
    pub fn to_summary(&self) -> CharacterSummary {
        CharacterSummary {
            id: self.id,
            name: self.name.clone(),
            level: self.level,
            class: self.class,
            gender: self.gender,
            last_logout_date: self.last_logout_date,
        }
    }
}

// ---------------------------------------------------------------------------
// 行会（Crystal `GuildInfo` / `GuildRank` / `GuildMember`）
// ---------------------------------------------------------------------------

/// 行会（Crystal `GuildInfo`，`Server/MirDatabase/GuildInfo.cs:7-38`）。
///
/// Crystal 把整个行会（职位、成员、仓库、buff、公告、攻城租期）序列化成一个
/// `Guilds/<GuildIndex>.mgd` 文件（`GuildInfo.Save`，`:189-255`）。本作把它拆成
/// 五张表（`migrations/0005_guilds.sql` + `migrations/0006_guild_storage.sql`）：
///
/// | 本结构字段 | 表 | Crystal 来源 |
/// |---|---|---|
/// | `id` | `guilds.id` | `GuildIndex`（`:9`） |
/// | `name` | `guilds.name` | `Name`（`:10`） |
/// | `leader_character_id` | `guilds.leader_character_id` | rank 0 的第一个成员（`:54-57`） |
/// | `level` / `experience` / `gold` / `spare_points` | `guilds` 同名列 | `:11-14` |
/// | `notice` | `guilds.notice`（`\n` 连接） | `Notice`（`:24`） |
/// | [`GuildRankRow`] | `guild_ranks` | `Ranks`（`:21`） |
/// | [`GuildMemberRow`] | `guild_members` | `GuildRank.Members`（`Shared/Data/GuildData.cs:1-43`） |
/// | [`GuildStorageRow`] | `guild_storage` | `StoredItems`（`:22`，112 格） |
/// | [`GuildBuffRow`] | `guild_buffs` | `BuffList`（`:23`） |
///
/// 金币只有一份：Crystal 的 `GuildInfo.Gold` 既是行会资金也是「仓库金币」，
/// 本作的 `gold` 列同样一份（见 `migrations/0006_guild_storage.sql` 的说明）。
///
/// 投票（`:16-19`）、战旗与攻城租期（`:29-37`）**没有**迁移，
/// 见 `docs/world/guild.md` 的「未做」。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Guild {
    /// 行会主键（Crystal `GuildInfo.GuildIndex`）。
    pub id: i32,
    /// 行会名（唯一性不区分大小写）。
    pub name: String,
    /// 会长角色主键。
    pub leader_character_id: i64,
    /// 行会等级（Crystal `Level`，`byte`）。
    pub level: i16,
    /// 行会经验（Crystal `Experience`，`long`）。
    pub experience: i64,
    /// 行会资金（Crystal `Gold`，`uint`；这里用 `i64` 以免每处都要 `try_into`）。
    pub gold: i64,
    /// 行会升级点（Crystal `SparePoints`，`byte`）。
    pub spare_points: i16,
    /// 公告的每一行（Crystal `Notice`，`List<string>`）。
    pub notice: Vec<String>,
    /// 职位（按 `index` 升序；`index == 0` 是会长职位）。
    pub ranks: Vec<GuildRankRow>,
    /// 成员（跨职位平铺；每个角色的 `rank_index` 指回 [`GuildRankRow::rank_index`]）。
    pub members: Vec<GuildMemberRow>,
    /// 仓库（只存非空格子，按 `slot` 升序）。
    pub storage: Vec<GuildStorageRow>,
    /// 行会增益（按写入顺序；Crystal `List<GuildBuff>`）。
    pub buffs: Vec<GuildBuffRow>,
}

impl Guild {
    /// 公告全文：按行用 `\n` 连接（空行也保留，与 Crystal 的 `List<string>` 一致）。
    #[must_use]
    pub fn notice_text(&self) -> String {
        self.notice.join("\n")
    }

    /// 从公告全文拆行。
    ///
    /// 空串解成 0 行（Crystal 里没有公告就是空 `List`，而不是一个空行）。
    #[must_use]
    pub fn split_notice(text: &str) -> Vec<String> {
        if text.is_empty() {
            return Vec::new();
        }
        text.split('\n').map(str::to_owned).collect()
    }
}

/// 一行职位（`guild_ranks`）。
///
/// 对应 Crystal `GuildRank`（`Shared/Data/GuildData.cs:1`）的 `Index` / `Name` / `Options`；
/// 成员在 Crystal 里嵌在职位对象内，本作单独成表（见 [`GuildMemberRow`]）。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, FromRow)]
pub struct GuildRankRow {
    /// 所属行会。
    pub guild_id: i32,
    /// 职位序号（`0` = 会长）。
    pub rank_index: i16,
    /// 职位名。
    pub name: String,
    /// 权限位掩码（`GuildRankOptions`，`Shared/Enums.cs:1897-1908`）。
    pub options: i16,
}

/// 一行成员（`guild_members`）。
///
/// 角色名来自 `characters.name` 的联表查询（Crystal 把名字抄在 `GuildMember.Name` 里，
/// `Shared/Data/GuildData.cs:34`；本作不冗余存名字，改名后行会列表自动跟着变）。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, FromRow)]
pub struct GuildMemberRow {
    /// 所属行会。
    pub guild_id: i32,
    /// 角色主键。
    pub character_id: i64,
    /// 角色名（联表读出）。
    pub name: String,
    /// 所属职位序号。
    pub rank_index: i16,
    /// 入会时间（Crystal 没有这个字段，本作扩展；用来稳定成员列表顺序）。
    pub joined_at: DateTime<Utc>,
}

/// 一格行会仓库（`guild_storage`）。
///
/// 对应 Crystal `GuildStorageItem`（`Shared/Data/GuildData.cs:44-60`）：
/// `Item`（这里是 `encode_slots(&[Some(item)])` 的字节块，见
/// `migrations/0006_guild_storage.sql`）+ `UserId`。
/// `stored_at` 是本作扩展（Crystal 没有存放时间）。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, FromRow)]
pub struct GuildStorageRow {
    /// 所属行会。
    pub guild_id: i32,
    /// 格子序号（Crystal `GuildInfo.StoredItems` 的数组下标，0..=111）。
    pub slot: i16,
    /// `encode_slots(&[Some(UserItem)])` 字节块。
    pub item: Vec<u8>,
    /// 存放者的角色主键（Crystal `GuildStorageItem.UserId`）。
    pub user_id: i64,
    /// 存放时间（本作扩展）。
    pub stored_at: DateTime<Utc>,
}

/// 一条行会增益（`guild_buffs`）。
///
/// 对应 Crystal `GuildBuff`（`Shared/Data/GuildData.cs:217-243`）：
/// `Id` / `Active` / `ActiveTimeRemaining`。`Info` 不在库里，
/// 运行时按 `buff_id` 从 `GuildSettings.ini` 的 `[Buff-N]` 段（`Server/Settings.cs:1371-1375`）查。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, FromRow)]
pub struct GuildBuffRow {
    /// 所属行会。
    pub guild_id: i32,
    /// 增益编号（`GuildBuff.Id`）。
    pub buff_id: i32,
    /// 是否已激活（`GuildBuff.Active`）。
    pub active: bool,
    /// 剩余生效时间，单位**分钟**（`GuildBuff.ActiveTimeRemaining`）。
    pub active_time_remaining: i32,
}

// ---------------------------------------------------------------------------
// 邮件（Crystal `MailInfo`）
// ---------------------------------------------------------------------------

/// 一条邮件（`mail` 表；Crystal `MailInfo`，`Server/MirDatabase/MailInfo.cs:5-167`）。
///
/// Crystal 把收件箱挂在角色上（`CharacterInfo.Mail`，`CharacterInfo.cs:85`），
/// 落盘顺序见 `CharacterInfo.cs:500-503`（`i32 count` + 逐条 `MailInfo.Save`）。
/// 本作拆成一张表，字段对照见 `migrations/0007_mail.sql` 的文件头。
///
/// `items` 是 **`crates/mir-server/src/items.rs` 的 `encode_slots` 字节块**：
/// `encode_slots(&[Some(item), …])`，与背包/装备/行会仓库共用同一套 `UserItem[]` 编解码。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, FromRow)]
pub struct Mail {
    /// Crystal `MailInfo.MailID`（`ulong`，`++Envir.NextMailID`）。
    ///
    /// 数据库列是 `BIGSERIAL`，但服务器层会**显式写号**
    /// （`crates/mir-server/src/mail.rs` 的 `World::allocate_mail_id`），
    /// 与行会 ID 的做法一致；`create_mail` 会在显式写号后推进序列。
    pub id: i64,
    /// Crystal `MailInfo.RecipientIndex`：收件角色主键。
    pub character_id: i64,
    /// Crystal `MailInfo.Sender`：发件人角色名。
    pub sender_name: String,
    /// **本作扩展**：主题。Crystal 的 `C.SendMail` 只有收件人/正文
    /// （`Shared/ClientPackets.cs:2126-2128`），所以玩家邮件恒为空串。
    pub subject: String,
    /// Crystal `MailInfo.Message`。
    pub message: String,
    /// `encode_slots(&[Some(UserItem), …])` 字节块；空附件是零长度。
    pub items: Vec<u8>,
    /// Crystal `MailInfo.Gold`（`uint`）：附件金币，**不含**发送时扣掉的邮费。
    pub gold: i64,
    /// Crystal `MailInfo.DateSent`（发送时的 `Envir.Now`）。
    pub date_sent: DateTime<Utc>,
    /// Crystal `MailInfo.Opened`（`DateOpened > DateTime.MinValue`）。
    pub opened: bool,
    /// Crystal `MailInfo.Locked`。
    pub locked: bool,
    /// Crystal `MailInfo.Collected`：包裹是否已被邮局受理（见 `MailInfo.cs:99-143`）。
    pub collected: bool,
}

// ---------------------------------------------------------------------------
// 市场（Crystal `AuctionInfo`）
// ---------------------------------------------------------------------------

/// 一条寄售/拍卖挂单（`market_auctions` 表；Crystal `AuctionInfo`，
/// `Server/MirDatabase/AuctionInfo.cs:5-117`）。
///
/// Crystal 把挂单存在 **`MirADB` 全局段**（`Envir.Auctions`，`Server/MirEnvir/Envir.cs:131`；
/// 存档 `:2591-2594`、读档 `:3018-3029`），不是角色行——本表同样是全局表。
///
/// `item` 是 **`crates/mir-server/src/items.rs` 的 `encode_slots` 字节块**：
/// `encode_slots(&[Some(item)])`（单格），与背包/装备/邮件/行会仓库共用同一套
/// `UserItem[]` 编解码。`seller_name` / `current_buyer_name` 是名字快照
/// （Crystal 通过 `SellerInfo`/`CurrentBuyerInfo` 引用拿名字，`AuctionInfo.cs:19`、`:96`）。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, FromRow)]
pub struct MarketAuction {
    /// Crystal `AuctionInfo.AuctionID`（`ulong`，`++Envir.NextAuctionID`，`:12`、`:33`）。
    pub id: i64,
    /// Crystal `AuctionInfo.Item`（`:14`）：`encode_slots(&[Some(item)])` 单格字节块。
    pub item: Vec<u8>,
    /// Crystal `AuctionInfo.ConsignmentDate`（`:15`，`Envir.Now`）。
    pub consignment_date: DateTime<Utc>,
    /// Crystal `AuctionInfo.Price`（`:16`，`uint`）：寄售一口价 / 拍卖起拍价。
    pub price: i64,
    /// Crystal `AuctionInfo.CurrentBid`（`:16`，`uint`）：当前最高出价；寄售恒为 0。
    pub current_bid: i64,
    /// Crystal `AuctionInfo.SellerIndex`（`:18`）：卖家角色主键。
    pub seller_character_id: i64,
    /// Crystal `SellerInfo.Name`（`:19`、`:96`）：卖家名字快照。
    pub seller_name: String,
    /// Crystal `AuctionInfo.CurrentBuyerIndex`（`:18`）：当前最高出价者；`None` = 没人出价。
    pub current_buyer_character_id: Option<i64>,
    /// Crystal `CurrentBuyerInfo.Name`（`:19`）：最高出价者名字快照。
    pub current_buyer_name: Option<String>,
    /// Crystal `AuctionInfo.Expired`（`:21`）：是否已过期（`Envir.ProcessAuction`，
    /// `Envir.cs:2396`）。
    pub expired: bool,
    /// Crystal `AuctionInfo.Sold`（`:21`）：是否已成交。
    pub sold: bool,
    /// Crystal `AuctionInfo.ItemType`（`:23`）：0 `Consign` / 1 `Auction` / 2 `GameShop`。
    pub item_type: i16,
}

// ---------------------------------------------------------------------------
// 商城（Crystal `GameShopList` / `GameshopLog` / `GSpurchases`）
// ---------------------------------------------------------------------------

/// 一行「全服已购数量」（`gameshop_stock` 表；Crystal `Envir.GameshopLog`，
/// `Server/MirEnvir/Envir.cs:133`）。
///
/// 键是商品的 **`GIndex`**（`GameShopItem.GIndex`，不是物品模板 `ItemIndex`）。
/// 只在 `GameShopItem.Stock != 0` 且 `iStock == false`（全服库存）时参与剩余库存计算：
/// `剩余 = Stock - purchased`（`PlayerObject.GameShopStock`，`PlayerObject.cs:13712-13717`）。
///
/// `Stock == 0` 表示不限量（`PlayerObject.cs:13782-13785`），不会产生本表的行。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, FromRow)]
pub struct GameShopStock {
    /// Crystal `GameShopItem.GIndex`。
    pub g_index: i32,
    /// 全服已购数量（Crystal `GameshopLog[gIndex]`）。
    pub purchased: i64,
}

/// 一行「某个角色已购数量」（`gameshop_purchases` 表；Crystal
/// `CharacterInfo.GSpurchases`）。
///
/// 与 [`GameShopStock`] 的区别只在计数口径：`iStock == true` 的个人库存按角色算
/// （`PlayerObject.cs:13708-13711`、`:13838-13849`），全服库存按全服算。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, FromRow)]
pub struct GameShopPurchase {
    /// 购买者角色主键。
    pub character_id: i64,
    /// Crystal `GameShopItem.GIndex`。
    pub g_index: i32,
    /// 该角色已购数量。
    pub quantity: i64,
}

// ---------------------------------------------------------------------------
// 攻城（Crystal `ConquestGuildInfo`）
// ---------------------------------------------------------------------------

/// 一个攻城区的运行期状态（`conquests` 表；Crystal `ConquestGuildInfo`，
/// `Server/MirDatabase/ConquestGuildInfo.cs:7-92`）。
///
/// Crystal 把这份状态存成 `Conquests/<index>.mcd` 一个二进制文件
/// （`Envir.SaveConquests`，`Server/MirEnvir/Envir.cs:2734-2747`；
/// `Envir.LoadConquests`，`:3119-3170`）。字段对照与「为什么静态配表不进本表」
/// 见 `migrations/0008_conquests.sql` 的文件头。
///
/// 静态数据（名字、地图、时间表、守卫/城门/城墙坐标）仍然来自 `Server.MirDB` 的
/// `ConquestInfo` 段；本结构只存会变的那些。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, FromRow)]
pub struct Conquest {
    /// Crystal `ConquestInfo.Index`（也是 `.mcd` 的文件名）。
    pub conquest_index: i32,
    /// Crystal `ConquestGuildInfo.Owner`：城主行会，`None` 表示无主（C# 里是 `-1`）。
    pub owner_guild_id: Option<i32>,
    /// Crystal `ConquestGuildInfo.AttackerID`：已排队的攻打方，`None` 表示没人申请。
    pub attacker_guild_id: Option<i32>,
    /// Crystal `ConquestGuildInfo.GoldStorage`（`uint`）：城主税收池。
    pub gold_storage: i64,
    /// Crystal `ConquestGuildInfo.NPCRate`（`byte`）：城主设置的加价百分比。
    pub npc_rate: i16,
    /// Crystal `ConquestObject.WarIsOn`：是否正在开战。
    pub war_is_on: bool,
    /// Crystal `ConquestObject.WarStartTime`。
    pub war_start_time: Option<DateTime<Utc>>,
    /// Crystal `ConquestObject.WarEndTime`。
    pub war_end_time: Option<DateTime<Utc>>,
    /// Crystal `ConquestObject.StartType`（`ConquestType`）。
    pub start_type: i16,
    /// Crystal `ConquestObject.GameType`（`ConquestGame`）。
    pub game_type: i16,
    /// 时间表快照：Crystal `ConquestInfo.StartHour`。
    pub schedule_start_hour: i16,
    /// 时间表快照：Crystal `ConquestInfo.WarLength`（分钟）。
    pub schedule_war_length: i32,
    /// 时间表快照：Crystal `ConquestInfo.Monday`…`Sunday` 的位掩码（周一 = 位 0）。
    pub schedule_days: i16,
    /// Crystal `ConquestObject.KingPoints`（`ConquestObject.cs:77`）：`行会ID:分数` 逗号分隔。
    ///
    /// Crystal 用 `Dictionary<GuildObject, int>` 存运行期引用，落盘时没有对应字段；
    /// 本作为了「重启后王者之城的比分不丢」额外存一份文本，格式见
    /// `crates/mir-server/src/conquest.rs` 的 `encode_points`。
    pub king_points: String,
    /// Crystal `ConquestObject.ControlPoints` 的归属部分（`:52-56`）：`控制点ID:行会ID` 逗号分隔。
    pub flag_owners: String,
    /// **本作扩展**：下一次自动开战时刻（Crystal 每分钟现算，不落盘）。
    pub next_war_at: Option<DateTime<Utc>>,
}

/// `guilds` 表的一行（不加联表信息，[`Guild`] 是它的展开版）。
#[derive(Debug, Clone, FromRow)]
pub(crate) struct GuildRow {
    /// 主键。
    pub id: i32,
    /// 行会名。
    pub name: String,
    /// 会长角色主键。
    pub leader_character_id: i64,
    /// 等级。
    pub level: i16,
    /// 经验。
    pub experience: i64,
    /// 资金。
    pub gold: i64,
    /// 升级点。
    pub spare_points: i16,
    /// 公告全文（`\n` 分隔）。
    pub notice: String,
}

/// 选角列表的一行（Crystal `SelectInfo`）。
///
/// 只带客户端选角需要的字段，避免把三个物品大块一起读出来。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, FromRow)]
pub struct CharacterSummary {
    /// 角色主键。
    pub id: i64,
    /// 角色名。
    pub name: String,
    /// 等级。
    pub level: i16,
    /// 职业（取值同 `MirClass`）。
    pub class: i16,
    /// 性别（取值同 `MirGender`）。
    pub gender: i16,
    /// 最后登出时间，Crystal 用它当「最后访问时间」（`SelectInfo.LastAccess`）。
    pub last_logout_date: Option<DateTime<Utc>>,
}
