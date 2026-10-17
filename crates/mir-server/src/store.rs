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
    /// 账号不存在。
    #[error("账号 {0} 不存在")]
    AccountMissing(String),
    /// 不支持的界面语言代码。
    #[error("不支持的界面语言代码 {0}（支持 zh-cn / en）")]
    InvalidLanguage(String),
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
    /// 背包（Crystal `CharacterInfo.Inventory` 的 `UserItem[]` 字节块，编解码见
    /// [`crate::items::encode_slots`] / [`crate::items::decode_inventory`]）。
    pub inventory: Vec<u8>,
    /// 装备（同样的字节块格式，见 [`crate::items::decode_equipment`]）。
    pub equipment: Vec<u8>,
    /// 已学技能（Crystal `CharacterInfo.Magics` 的字节块，编解码见
    /// [`crate::magic::encode_magics`] / [`crate::magic::decode_magics`]）。
    pub magics: Vec<u8>,
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
            inventory: Vec::new(),
            equipment: Vec::new(),
            magics: Vec::new(),
            last_access: DateTimeBinary(0),
        }
    }
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
    next_account_id: i64,
    next_character_id: i64,
}

impl MemoryStore {
    /// 新建空存储。
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: tokio::sync::Mutex::new(MemoryState {
                accounts: Vec::new(),
                characters: Vec::new(),
                next_account_id: 1,
                next_character_id: 1,
            }),
        }
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
