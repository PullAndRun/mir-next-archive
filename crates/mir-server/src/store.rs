//! 账号与角色的存储边界。
//!
//! 服务端只依赖 [`AccountStore`] 这个 trait；PostgreSQL 实现（`mir-store`）
//! 与测试用的内存实现可以互换。口令哈希复用 `mir-store` 的 Crystal 兼容实现
//! （PBKDF2-HMAC-SHA1 / 24 字节盐 / 50 次迭代 / 有损 UTF-8 解码）。

use std::future::Future;

use mir_core::DateTimeBinary;
use mir_core::enums::{MirClass, MirDirection, MirGender};
use mir_store::{generate_salt, hash_password};

/// 存储层错误。
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// 账号已存在。
    #[error("账号 {0} 已存在")]
    AccountExists(String),
    /// 角色名已存在。
    #[error("角色名 {0} 已存在")]
    CharacterExists(String),
    /// 行会名已存在。
    #[error("行会名 {0} 已存在")]
    GuildExists(String),
    /// 账号不存在。
    #[error("账号 {0} 不存在")]
    AccountMissing(String),
    /// 不支持的界面语言代码。
    #[error("不支持的界面语言代码 {0}（支持 zh-cn / en）")]
    InvalidLanguage(String),
    /// 目标记录不存在。
    #[error("找不到{0}")]
    NotFound(&'static str),
    /// 内部错误。
    #[error("存储错误：{0}")]
    Internal(String),
}

/// 账号记录（对应 Crystal `AccountInfo` 的 MVP 子集）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    /// 账号 ID。
    pub id: i64,
    /// 账号名（保留原始大小写）。
    pub username: String,
    /// 口令哈希（UTF-8 字符串，Crystal 语义）。
    pub password_hash: String,
    /// 盐。
    pub salt: Vec<u8>,
    /// 邮箱。
    pub email: String,
    /// 是否被封禁。
    pub banned: bool,
    /// 封禁原因。
    pub ban_reason: String,
    /// 封禁到期时间。
    pub ban_expiry: Option<DateTimeBinary>,
    /// 是否要求改密。
    pub require_password_change: bool,
    /// 连续输错次数。
    pub wrong_password_count: u32,
    /// 是否是管理员。
    pub admin: bool,
    /// 界面语言代码（默认 `zh-cn`；玩家在游戏内切换后会回写）。
    pub language: String,
}

impl Account {
    /// 用明文口令构造一个新账号。
    #[must_use]
    pub fn new_with_password(id: i64, username: &str, password: &str, email: &str) -> Self {
        let salt = generate_salt().to_vec();
        let password_hash = hash_password(password, &salt);
        Self {
            id,
            username: username.to_owned(),
            password_hash,
            salt,
            email: email.to_owned(),
            banned: false,
            ban_reason: String::new(),
            ban_expiry: None,
            require_password_change: false,
            wrong_password_count: 0,
            admin: false,
            // 项目要求默认全中文化。
            language: "zh-cn".to_owned(),
        }
    }

    /// 校验明文口令。
    #[must_use]
    pub fn verify_password(&self, password: &str) -> bool {
        let candidate = hash_password(password, &self.salt);
        // Crystal 用序数比较（`string.CompareOrdinal`），这里等价于逐字节比较。
        candidate.as_bytes() == self.password_hash.as_bytes()
    }
}

/// 角色记录（对应 Crystal `CharacterInfo` 的 MVP 子集）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Character {
    /// 角色 ID。
    pub id: i64,
    /// 所属账号。
    pub account_id: i64,
    /// 角色名。
    pub name: String,
    /// 等级。
    pub level: u16,
    /// 职业。
    pub class: MirClass,
    /// 性别。
    pub gender: MirGender,
    /// 发型。
    pub hair: u8,
    /// 所在地图。
    pub map: String,
    /// 坐标 X。
    pub x: i32,
    /// 坐标 Y。
    pub y: i32,
    /// 朝向。
    pub direction: MirDirection,
    /// 当前生命。
    pub hp: i32,
    /// 当前魔法。
    pub mp: i32,
    /// 经验。
    pub experience: i64,
    /// 金币（Crystal `AccountInfo.Gold`；本作落在角色行的 `gold` 列上，`i64`）。
    pub gold: i64,
    /// 元宝/点数（Crystal `AccountInfo.Credit`；同样落在角色行的 `credit` 列上）。
    pub credit: i64,
    /// 背包（Crystal `CharacterInfo.Inventory` 的 `UserItem[]` 字节块，编解码见
    /// [`crate::items::encode_slots`] / [`crate::items::decode_inventory`]）。
    pub inventory: Vec<u8>,
    /// 装备（同样的字节块格式，见 [`crate::items::decode_equipment`]）。
    pub equipment: Vec<u8>,
    /// 已学技能（Crystal `CharacterInfo.Magics` 的字节块，编解码见
    /// [`crate::magic::encode_magics`] / [`crate::magic::decode_magics`]）。
    pub magics: Vec<u8>,
    /// 任务进度 + 已完成任务（Crystal `CharacterInfo.CurrentQuests` / `CompletedQuests` 的字节块，
    /// 编解码见 [`crate::quests::encode_quests`] / [`crate::quests::decode_quests`]）。
    pub quests: Vec<u8>,
    /// 所属行会的主键，`-1` 表示没有行会（Crystal `CharacterInfo.GuildIndex`）。
    ///
    /// 行会本体走 [`GuildRecord`]（`guilds` / `guild_ranks` / `guild_members` 三张表），
    /// 这一列只是角色侧的反向指针，登录时用它（或行会成员表）决定要不要把玩家挂进行会。
    pub guild_index: i32,
    /// 最后登录时间。
    pub last_access: DateTimeBinary,
}

impl Character {
    /// 用一个新角色模板：等级 0、属性留待首次进世界生成（Crystal 的行为）。
    #[must_use]
    pub fn new(
        id: i64,
        account_id: i64,
        name: &str,
        class: MirClass,
        gender: MirGender,
        map: &str,
        point: (i32, i32),
    ) -> Self {
        Self {
            id,
            account_id,
            name: name.to_owned(),
            level: 0,
            class,
            gender,
            hair: 0,
            map: map.to_owned(),
            x: point.0,
            y: point.1,
            direction: MirDirection::Down,
            hp: 0,
            mp: 0,
            experience: 0,
            gold: 0,
            credit: 0,
            inventory: Vec::new(),
            equipment: Vec::new(),
            magics: Vec::new(),
            quests: Vec::new(),
            // Crystal 的 `CharacterInfo.GuildIndex` 默认 -1（没有行会）。
            guild_index: -1,
            last_access: DateTimeBinary(0),
        }
    }
}

/// 行会一行（与 `mir-store` 的 `Guild` 同构的存储视图）。
///
/// 服务端刻意不依赖 `mir-store` 的具体结构：`AccountStore` 只认这里的
/// [`GuildRecord`]，PostgreSQL 实现负责与 `mir_store::Guild` 互转
/// （[`PgAccountStore`](crate::PgAccountStore)），测试用的 [`MemoryStore`] 直接存。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GuildRecord {
    /// 行会主键；`<= 0` 表示「还没入库」，保存时由存储层分配。
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
    /// 公告行。
    pub notice: Vec<String>,
    /// 职位。
    pub ranks: Vec<GuildRankRecord>,
    /// 成员（跨职位平铺）。
    pub members: Vec<GuildMemberRecord>,
    /// 仓库格子（只含非空格子，按 `slot` 升序；Crystal `GuildInfo.StoredItems`，112 格）。
    pub storage: Vec<GuildStorageRecord>,
    /// 行会增益（Crystal `GuildInfo.BuffList`）。
    pub buffs: Vec<GuildBuffRecord>,
}

/// 行会里的一行职位。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GuildRankRecord {
    /// 职位序号（`0` = 会长）。
    pub index: u8,
    /// 职位名。
    pub name: String,
    /// 权限位掩码（`GuildRankOptions`）。
    pub options: u8,
}

/// 行会里的一名成员。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GuildMemberRecord {
    /// 角色主键。
    pub character_id: i64,
    /// 角色名。
    pub name: String,
    /// 所属职位序号。
    pub rank_index: u8,
    /// 入会时间（Unix 毫秒；本作扩展，用来稳定成员顺序）。
    pub joined_at: i64,
}

/// 一格行会仓库（Crystal `GuildStorageItem`，`Shared/Data/GuildData.cs:44-60`）。
///
/// `item` 是 [`crate::items::encode_slots`] 的「单格字节块」
/// （`encode_slots(&[Some(UserItem)])`），与角色背包/装备用同一套编解码。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GuildStorageRecord {
    /// 格子序号（0..=111，`GuildInfo.cs:22`）。
    pub slot: u8,
    /// 物品字节块。
    pub item: Vec<u8>,
    /// 存放者的角色主键（Crystal `GuildStorageItem.UserId`）。
    pub user_id: i64,
    /// 存放时间（Unix 毫秒；本作扩展，Crystal 无此字段）。
    pub stored_at: i64,
}

/// 一条行会增益（Crystal `GuildBuff`，`Shared/Data/GuildData.cs:217-243`）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GuildBuffRecord {
    /// 增益编号。
    pub id: i32,
    /// 是否已激活。
    pub active: bool,
    /// 剩余生效时间，单位**分钟**。
    pub active_time_remaining: i32,
}

/// 存储边界。
///
/// 返回类型写成 `impl Future + Send`（而不是裸 `async fn`）是为了让泛型调用方
/// 能证明 future 是 `Send`，否则 `tokio::spawn` 里的连接任务无法编译。
#[allow(clippy::manual_async_fn)] // 见上：必须显式声明 `+ Send`
pub trait AccountStore: Send + Sync + 'static {
    /// 建账号。
    fn create_account<'a>(
        &'a self,
        username: &'a str,
        password: &'a str,
        email: &'a str,
    ) -> impl Future<Output = Result<Account, StoreError>> + Send + 'a;

    /// 按账号名查找（忽略大小写）。
    fn find_account<'a>(
        &'a self,
        username: &'a str,
    ) -> impl Future<Output = Result<Option<Account>, StoreError>> + Send + 'a;

    /// 建角色。
    fn create_character<'a>(
        &'a self,
        account_id: i64,
        name: &'a str,
        class: MirClass,
        gender: MirGender,
        map: &'a str,
        point: (i32, i32),
    ) -> impl Future<Output = Result<Character, StoreError>> + Send + 'a;

    /// 列出某账号的全部角色。
    fn list_characters(
        &self,
        account_id: i64,
    ) -> impl Future<Output = Result<Vec<Character>, StoreError>> + Send + '_;

    /// 查找角色。
    fn find_character(
        &self,
        account_id: i64,
        character_id: i64,
    ) -> impl Future<Output = Result<Option<Character>, StoreError>> + Send + '_;

    /// 删除角色。
    fn delete_character(
        &self,
        account_id: i64,
        character_id: i64,
    ) -> impl Future<Output = Result<bool, StoreError>> + Send + '_;

    /// 保存角色。
    fn save_character(
        &self,
        character: Character,
    ) -> impl Future<Output = Result<(), StoreError>> + Send + '_;

    /// 保存账号的语言设置（游戏内切换后回写）。
    fn set_language(
        &self,
        account_id: i64,
        language: String,
    ) -> impl Future<Output = Result<(), StoreError>> + Send + '_;

    /// 写一个行会（职位与成员整份替换），返回存储层分配的主键。
    ///
    /// 对应 Crystal `GuildInfo.Save`（`Server/MirDatabase/GuildInfo.cs:189-255`）。
    fn save_guild(
        &self,
        guild: GuildRecord,
    ) -> impl Future<Output = Result<i32, StoreError>> + Send + '_;

    /// 删除一个行会（职位与成员一起删）。
    fn delete_guild(
        &self,
        guild_id: i32,
    ) -> impl Future<Output = Result<(), StoreError>> + Send + '_;

    /// 读出全部行会（服务端启动时灌进 [`World`](crate::World)）。
    fn list_guilds(&self)
    -> impl Future<Output = Result<Vec<GuildRecord>, StoreError>> + Send + '_;

    /// 写角色的行会归属（`-1` = 退出行会）。
    fn set_character_guild(
        &self,
        character_id: i64,
        guild_id: i32,
    ) -> impl Future<Output = Result<(), StoreError>> + Send + '_;
}

/// 内存实现：用于测试与无数据库的本地试玩。
#[derive(Debug, Default)]
pub struct MemoryStore {
    state: tokio::sync::Mutex<MemoryState>,
}

#[derive(Debug, Default)]
struct MemoryState {
    accounts: Vec<Account>,
    characters: Vec<Character>,
    /// 行会（本作扩展：`MemoryStore` 在本进程里扮演 `guilds`/`guild_ranks`/`guild_members` 三张表）。
    guilds: Vec<GuildRecord>,
    next_account_id: i64,
    next_character_id: i64,
    next_guild_id: i32,
}

impl MemoryStore {
    /// 新建空存储。
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: tokio::sync::Mutex::new(MemoryState {
                accounts: Vec::new(),
                characters: Vec::new(),
                guilds: Vec::new(),
                next_account_id: 1,
                next_character_id: 1,
                next_guild_id: 1,
            }),
        }
    }

    /// 行会表快照（测试用来断言「写库」的结果）。
    pub async fn guilds(&self) -> Vec<GuildRecord> {
        self.state.lock().await.guilds.clone()
    }

    /// 直接读某个角色的行会归属（等价于数据库里的 `characters.guild_index`）。
    pub async fn character_guild_index(&self, character_id: i64) -> Option<i32> {
        self.state
            .lock()
            .await
            .characters
            .iter()
            .find(|character| character.id == character_id)
            .map(|character| character.guild_index)
    }
}

impl AccountStore for MemoryStore {
    async fn create_account(
        &self,
        username: &str,
        password: &str,
        email: &str,
    ) -> Result<Account, StoreError> {
        let mut state = self.state.lock().await;
        if state
            .accounts
            .iter()
            .any(|account| account.username.eq_ignore_ascii_case(username))
        {
            return Err(StoreError::AccountExists(username.to_owned()));
        }
        let id = state.next_account_id;
        state.next_account_id += 1;
        let account = Account::new_with_password(id, username, password, email);
        state.accounts.push(account.clone());
        Ok(account)
    }

    async fn find_account(&self, username: &str) -> Result<Option<Account>, StoreError> {
        let state = self.state.lock().await;
        Ok(state
            .accounts
            .iter()
            .find(|account| account.username.eq_ignore_ascii_case(username))
            .cloned())
    }

    async fn create_character(
        &self,
        account_id: i64,
        name: &str,
        class: MirClass,
        gender: MirGender,
        map: &str,
        point: (i32, i32),
    ) -> Result<Character, StoreError> {
        let mut state = self.state.lock().await;
        if state
            .characters
            .iter()
            .any(|character| character.name.eq_ignore_ascii_case(name))
        {
            return Err(StoreError::CharacterExists(name.to_owned()));
        }
        let id = state.next_character_id;
        state.next_character_id += 1;
        let character = Character::new(id, account_id, name, class, gender, map, point);
        state.characters.push(character.clone());
        Ok(character)
    }

    async fn list_characters(&self, account_id: i64) -> Result<Vec<Character>, StoreError> {
        let state = self.state.lock().await;
        Ok(state
            .characters
            .iter()
            .filter(|character| character.account_id == account_id)
            .cloned()
            .collect())
    }

    async fn find_character(
        &self,
        account_id: i64,
        character_id: i64,
    ) -> Result<Option<Character>, StoreError> {
        let state = self.state.lock().await;
        Ok(state
            .characters
            .iter()
            .find(|character| character.account_id == account_id && character.id == character_id)
            .cloned())
    }

    async fn delete_character(
        &self,
        account_id: i64,
        character_id: i64,
    ) -> Result<bool, StoreError> {
        let mut state = self.state.lock().await;
        let before = state.characters.len();
        state.characters.retain(|character| {
            !(character.account_id == account_id && character.id == character_id)
        });
        Ok(state.characters.len() != before)
    }

    async fn save_character(&self, character: Character) -> Result<(), StoreError> {
        let mut state = self.state.lock().await;
        if let Some(existing) = state
            .characters
            .iter_mut()
            .find(|candidate| candidate.id == character.id)
        {
            *existing = character;
        }
        Ok(())
    }

    async fn set_language(&self, account_id: i64, language: String) -> Result<(), StoreError> {
        let mut state = self.state.lock().await;
        if let Some(account) = state
            .accounts
            .iter_mut()
            .find(|candidate| candidate.id == account_id)
        {
            account.language = language;
        }
        Ok(())
    }

    async fn save_guild(&self, guild: GuildRecord) -> Result<i32, StoreError> {
        let mut state = self.state.lock().await;
        // 行会名唯一（忽略大小写），与 `guilds_name_ci_key` 等价。
        if state.guilds.iter().any(|existing| {
            existing.id != guild.id && existing.name.eq_ignore_ascii_case(&guild.name)
        }) {
            return Err(StoreError::GuildExists(guild.name));
        }
        if guild.id > 0 {
            if let Some(existing) = state.guilds.iter_mut().find(|row| row.id == guild.id) {
                *existing = guild.clone();
                return Ok(guild.id);
            }
            // 有主键但库里没有：按「新建」处理，避免会话拿到半个行会。
        }
        let id = state.next_guild_id;
        state.next_guild_id += 1;
        let mut stored = guild;
        stored.id = id;
        state.guilds.push(stored);
        Ok(id)
    }

    async fn delete_guild(&self, guild_id: i32) -> Result<(), StoreError> {
        let mut state = self.state.lock().await;
        let before = state.guilds.len();
        state.guilds.retain(|guild| guild.id != guild_id);
        if state.guilds.len() == before {
            return Err(StoreError::NotFound("行会"));
        }
        // 外键级联的等价物：把还指着它的角色行清回 -1。
        for character in &mut state.characters {
            if character.guild_index == guild_id {
                character.guild_index = -1;
            }
        }
        Ok(())
    }

    async fn list_guilds(&self) -> Result<Vec<GuildRecord>, StoreError> {
        Ok(self.state.lock().await.guilds.clone())
    }

    async fn set_character_guild(
        &self,
        character_id: i64,
        guild_id: i32,
    ) -> Result<(), StoreError> {
        let mut state = self.state.lock().await;
        let Some(character) = state
            .characters
            .iter_mut()
            .find(|candidate| candidate.id == character_id)
        else {
            return Err(StoreError::NotFound("角色"));
        };
        character.guild_index = guild_id;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir_store::SALT_LEN;

    #[tokio::test]
    async fn account_and_character_lifecycle() {
        let store = MemoryStore::new();
        let account = store
            .create_account("diana", "secret", "diana@example.com")
            .await
            .expect("建号");
        assert_eq!(account.salt.len(), SALT_LEN);
        assert!(account.verify_password("secret"));
        assert!(!account.verify_password("wrong"));

        let character = store
            .create_character(
                account.id,
                "裁决",
                MirClass::Warrior,
                MirGender::Male,
                "0",
                (330, 270),
            )
            .await
            .expect("建角");
        assert_eq!(character.level, 0, "Crystal 建角时等级为 0");
        assert_eq!(store.list_characters(account.id).await.unwrap().len(), 1);
        assert!(
            store
                .delete_character(account.id, character.id)
                .await
                .unwrap()
        );
        assert!(store.list_characters(account.id).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn account_names_are_case_insensitive() {
        let store = MemoryStore::new();
        store.create_account("Diana", "secret", "").await.unwrap();
        assert!(store.find_account("diana").await.unwrap().is_some());
        assert!(store.create_account("dIANA", "secret", "").await.is_err());
    }
}
