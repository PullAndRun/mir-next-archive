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
    /// Crystal `password`：PBKDF2 结果的 UTF-8 替换解码串，原样保存、原样比较。
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
