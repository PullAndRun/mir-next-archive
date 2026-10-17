//! PostgreSQL 存储实现（把 `mir-store` 的仓储适配成服务端的 [`AccountStore`]）。

#[cfg(test)]
use chrono::TimeZone;
use chrono::{DateTime, Utc};
use mir_core::DateTimeBinary;
use mir_core::enums::{MirClass, MirDirection, MirGender};
use mir_store::PgPool;

use crate::store::{Account, AccountStore, Character, StoreError};

/// 基于 PostgreSQL 的账号/角色存储。
#[derive(Debug, Clone)]
pub struct PgAccountStore {
    pool: PgPool,
}

impl PgAccountStore {
    /// 连接数据库并跑完迁移。
    ///
    /// # Errors
    /// 连接失败或迁移失败时返回错误。
    pub async fn connect(database_url: &str) -> Result<Self, StoreError> {
        let pool = mir_store::connect(database_url)
            .await
            .map_err(|error| StoreError::Internal(error.to_string()))?;
        mir_store::migrate(&pool)
            .await
            .map_err(|error| StoreError::Internal(error.to_string()))?;
        Ok(Self { pool })
    }

    /// 底层连接池（后台任务、工具会用到）。
    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

fn map_store_error(error: mir_store::StoreError) -> StoreError {
    match error {
        mir_store::StoreError::AccountExists(username) => StoreError::AccountExists(username),
        mir_store::StoreError::CharacterNameTaken(name) => StoreError::CharacterExists(name),
        mir_store::StoreError::InvalidLanguage(code) => StoreError::InvalidLanguage(code),
        other => StoreError::Internal(other.to_string()),
    }
}

#[cfg(test)]
fn to_datetime(value: DateTimeBinary) -> Option<DateTime<Utc>> {
    if value.is_unset() {
        return None;
    }
    // 用 `timestamp_opt` 而不是 `timestamp`：越界值（比如存档里的 0）不应该 panic。
    Utc.timestamp_opt(value.to_unix_seconds(), 0).single()
}

fn from_datetime(value: Option<DateTime<Utc>>) -> DateTimeBinary {
    value.map_or(DateTimeBinary(0), |date| {
        DateTimeBinary::from_unix_seconds(date.timestamp())
    })
}

fn from_store_account(account: mir_store::Account) -> Account {
    Account {
        id: account.id,
        username: account.username,
        password_hash: account.password_hash,
        salt: account.salt,
        email: account.email,
        banned: account.banned,
        ban_reason: account.ban_reason,
        ban_expiry: account.expiry_date.map(|date| from_datetime(Some(date))),
        require_password_change: account.require_password_change,
        wrong_password_count: u32::try_from(account.wrong_password_count).unwrap_or(0),
        admin: account.admin,
        language: account.language,
    }
}

fn from_store_character(character: mir_store::Character) -> Character {
    Character {
        id: character.id,
        account_id: character.account_id,
        name: character.name,
        level: u16::try_from(character.level).unwrap_or(0),
        class: MirClass::from_value(u8::try_from(character.class).unwrap_or(0)).unwrap_or_default(),
        gender: MirGender::from_value(u8::try_from(character.gender).unwrap_or(0))
            .unwrap_or_default(),
        hair: u8::try_from(character.hair).unwrap_or(0),
        map: character.current_map_index.to_string(),
        x: character.current_x,
        y: character.current_y,
        direction: MirDirection::from_value(u8::try_from(character.direction).unwrap_or(4))
            .unwrap_or_default(),
        hp: character.hp,
        mp: character.mp,
        experience: character.experience,
        gold: character.gold,
        inventory: character.inventory,
        equipment: character.equipment,
        magics: character.magics,
        last_access: from_datetime(character.last_login_date),
    }
}

/// 把服务端的角色视图写回存储行。
fn apply_to_store_character(character: &Character, target: &mut mir_store::Character) {
    target.name.clone_from(&character.name);
    target.level = i16::try_from(character.level).unwrap_or(1);
    target.class = i16::from(character.class as u8);
    target.gender = i16::from(character.gender as u8);
    target.hair = i16::from(character.hair);
    target.current_map_index = character
        .map
        .parse::<i32>()
        .unwrap_or(target.current_map_index);
    target.current_x = character.x;
    target.current_y = character.y;
    target.direction = i16::from(character.direction as u8);
    target.hp = character.hp;
    target.mp = character.mp;
    target.experience = character.experience;
    target.gold = character.gold;
    target.inventory.clone_from(&character.inventory);
    target.equipment.clone_from(&character.equipment);
    target.magics.clone_from(&character.magics);
}

// 返回类型写成 `impl Future + Send` 是为了满足 trait 的 Send 契约（见 `store.rs`），
// 因此这里不能用 `async fn` 语法糖。
#[allow(clippy::manual_async_fn)]
impl AccountStore for PgAccountStore {
    fn create_account<'a>(
        &'a self,
        username: &'a str,
        password: &'a str,
        email: &'a str,
    ) -> impl std::future::Future<Output = Result<Account, StoreError>> + Send + 'a {
        async move {
            let mut new = mir_store::NewAccount::new(username, password);
            new.email = email.to_owned();
            let account = mir_store::create_account(&self.pool, &new)
                .await
                .map_err(map_store_error)?;
            Ok(from_store_account(account))
        }
    }

    fn find_account<'a>(
        &'a self,
        username: &'a str,
    ) -> impl std::future::Future<Output = Result<Option<Account>, StoreError>> + Send + 'a {
        async move {
            let account = mir_store::find_account(&self.pool, username)
                .await
                .map_err(map_store_error)?;
            Ok(account.map(from_store_account))
        }
    }

    fn create_character<'a>(
        &'a self,
        account_id: i64,
        name: &'a str,
        class: MirClass,
        gender: MirGender,
        map: &'a str,
        point: (i32, i32),
    ) -> impl std::future::Future<Output = Result<Character, StoreError>> + Send + 'a {
        async move {
            let new = mir_store::NewCharacter::new(
                account_id,
                name,
                i16::from(class as u8),
                i16::from(gender as u8),
            );
            let mut row = mir_store::create_character(&self.pool, &new)
                .await
                .map_err(map_store_error)?;
            // 出生点由服务器决定：建号接口只写姓名/职业/性别（与 Crystal 一致）。
            row.current_map_index = map.parse::<i32>().unwrap_or(0);
            row.current_x = point.0;
            row.current_y = point.1;
            row.level = 1;
            row.hp = 15;
            row.mp = 15;
            row.direction = i16::from(MirDirection::Down as u8);
            mir_store::save_character(&self.pool, &row)
                .await
                .map_err(map_store_error)?;
            Ok(from_store_character(row))
        }
    }

    fn list_characters(
        &self,
        account_id: i64,
    ) -> impl std::future::Future<Output = Result<Vec<Character>, StoreError>> + Send + '_ {
        async move {
            let summaries = mir_store::list_characters(&self.pool, account_id)
                .await
                .map_err(map_store_error)?;
            let mut characters = Vec::with_capacity(summaries.len());
            for summary in summaries {
                if let Ok(row) = mir_store::load_character(&self.pool, summary.id).await {
                    characters.push(from_store_character(row));
                }
            }
            Ok(characters)
        }
    }

    fn find_character(
        &self,
        account_id: i64,
        character_id: i64,
    ) -> impl std::future::Future<Output = Result<Option<Character>, StoreError>> + Send + '_ {
        async move {
            match mir_store::load_character(&self.pool, character_id).await {
                Ok(row) if row.account_id == account_id => Ok(Some(from_store_character(row))),
                Ok(_) | Err(mir_store::StoreError::NotFound(_)) => Ok(None),
                Err(error) => Err(map_store_error(error)),
            }
        }
    }

    fn delete_character(
        &self,
        account_id: i64,
        character_id: i64,
    ) -> impl std::future::Future<Output = Result<bool, StoreError>> + Send + '_ {
        async move {
            match mir_store::delete_character(&self.pool, character_id, account_id).await {
                Ok(()) => Ok(true),
                Err(mir_store::StoreError::NotFound(_)) => Ok(false),
                Err(error) => Err(map_store_error(error)),
            }
        }
    }

    fn save_character(
        &self,
        character: Character,
    ) -> impl std::future::Future<Output = Result<(), StoreError>> + Send + '_ {
        async move {
            let mut row = match mir_store::load_character(&self.pool, character.id).await {
                Ok(row) => row,
                Err(mir_store::StoreError::NotFound(_)) => {
                    return Err(StoreError::Internal(format!(
                        "角色 {} 不存在，无法保存",
                        character.id
                    )));
                }
                Err(error) => return Err(map_store_error(error)),
            };
            apply_to_store_character(&character, &mut row);
            mir_store::save_character(&self.pool, &row)
                .await
                .map_err(map_store_error)
        }
    }

    fn set_language(
        &self,
        account_id: i64,
        language: String,
    ) -> impl std::future::Future<Output = Result<(), StoreError>> + Send + '_ {
        async move {
            mir_store::set_account_language(&self.pool, account_id, &language)
                .await
                .map_err(map_store_error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn datetime_roundtrip_handles_unset() {
        assert!(to_datetime(DateTimeBinary(0)).is_none());
        let value = DateTimeBinary::from_unix_seconds(1_704_067_200);
        let date = to_datetime(value).expect("合法时间");
        assert_eq!(date.timestamp(), 1_704_067_200);
        assert_eq!(from_datetime(Some(date)), value);
    }
}
