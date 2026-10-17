//! 账号与角色的读写函数（仓库层）。
//!
//! 每个函数都接受 `sqlx` 的任意执行器（`&PgPool`、`&mut PgConnection` 或
//! `&mut PgTransaction`），因此可以在连接池上直接调用，也可以在调用方的事务里组合，
//! 集成测试正是用事务 + 回滚来避免污染数据库的。
//!
//! 单条 SQL 的语义如下（与 Crystal 对照）：
//!
//! | 函数 | 行为 |
//! |---|---|
//! | [`create_account`] | 生成 24 字节盐、按 Crystal 算法散列口令后插入 |
//! | [`find_account`] | 按登录名查账号，大小写不敏感 |
//! | [`save_account`] | 回写账号的可变状态（封禁、强制改密、错误计数、口令、最后登录） |
//! | [`create_character`] | 建角色；账号不存在返回 [`StoreError::NotFound`] |
//! | [`list_characters`] | 选角列表（未删除，按创建顺序） |
//! | [`load_character`] | 读完整角色；不存在或已软删除返回 [`StoreError::NotFound`] |
//! | [`save_character`] | 按主键回写可变字段（名字/账号/建号信息不可改） |
//! | [`delete_character`] | 软删除，写 `deleted`/`delete_date`，保留行 |
//! | [`mark_login`] | 记录最后登录时间并清零错误口令计数 |
//! | [`set_last_login_ip`] | 记录最后登录 IP |

use sqlx::PgExecutor;

use crate::error::{Result, StoreError};
use crate::models::{Account, Character, CharacterSummary, NewAccount, NewCharacter};
use crate::password::{generate_salt, hash_password};

/// `accounts` 表的列顺序；`Account` 的 `FromRow` 按名字匹配，这里的顺序只用于手写 SQL 的可读性。
const ACCOUNT_COLUMNS: &str = "id, username, password_hash, salt, email, creation_ip, creation_date, \
     banned, ban_reason, expiry_date, require_password_change, wrong_password_count, last_ip, \
     last_date, admin";

/// `characters` 表的列顺序。
const CHARACTER_COLUMNS: &str = "id, account_id, name, level, class, gender, hair, creation_ip, \
     creation_date, banned, ban_reason, expiry_date, last_ip, last_login_date, last_logout_date, \
     deleted, delete_date, current_map_index, current_x, current_y, direction, bind_map_index, \
     bind_x, bind_y, hp, mp, experience, gold, credit, attack_mode, pet_mode, pk_points, \
     allow_group, allow_trade, allow_observe, guild_index, inventory, equipment, storage, \
     married, married_date, mentor, mentor_date, is_mentor, mentor_exp";

/// 拼出账号查询：`SELECT <列> FROM accounts WHERE <filter>`。
fn account_select(filter: &str) -> String {
    format!("SELECT {ACCOUNT_COLUMNS} FROM accounts WHERE {filter}")
}

/// 拼出角色查询：`SELECT <列> FROM characters WHERE <filter>`。
fn character_select(filter: &str) -> String {
    format!("SELECT {CHARACTER_COLUMNS} FROM characters WHERE {filter}")
}

/// 新建账号。
///
/// 盐由 [`generate_salt`] 生成，口令按 Crystal 算法散列（见 [`crate::hash_password`]）；
/// 登录名冲突返回 [`StoreError::AccountExists`]。
pub async fn create_account<'e, E>(executor: E, new: &NewAccount) -> Result<Account>
where
    E: PgExecutor<'e>,
{
    let salt = generate_salt();
    let password_hash = hash_password(&new.password, &salt);
    let sql = format!(
        "INSERT INTO accounts (username, password_hash, salt, email, creation_ip, admin) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING {ACCOUNT_COLUMNS}"
    );

    sqlx::query_as::<_, Account>(&sql)
        .bind(new.username.as_str())
        .bind(password_hash)
        .bind(salt.as_slice())
        .bind(new.email.as_str())
        .bind(new.creation_ip.as_str())
        .bind(new.admin)
        .fetch_one(executor)
        .await
        .map_err(|error| {
            StoreError::map_insert(
                error,
                StoreError::AccountExists(new.username.clone()),
                "账号",
            )
        })
}

/// 按登录名查账号，大小写不敏感（Crystal `AccountExists` 用 `OrdinalIgnoreCase`）。
pub async fn find_account<'e, E>(executor: E, username: &str) -> Result<Option<Account>>
where
    E: PgExecutor<'e>,
{
    let sql = account_select("lower(username) = lower($1)");

    sqlx::query_as::<_, Account>(&sql)
        .bind(username)
        .fetch_optional(executor)
        .await
        .map_err(StoreError::Database)
}

/// 回写账号的可变状态。
///
/// 可改字段：邮箱、口令散列与盐、封禁（标志/原因/到期时间）、强制改密、错误口令计数、
/// 最后登录 IP/时间、GM 标志。登录名与建号信息不可改。
///
/// 改密时先用 [`Account::set_password`] 重新生成盐和散列，再调用本函数；封禁、
/// 解除封禁、清零错误计数同理。目标行不存在返回 [`StoreError::NotFound`]。
pub async fn save_account<'e, E>(executor: E, account: &Account) -> Result<()>
where
    E: PgExecutor<'e>,
{
    const SQL: &str = "UPDATE accounts SET \
         email = $2, password_hash = $3, salt = $4, banned = $5, ban_reason = $6, \
         expiry_date = $7, require_password_change = $8, wrong_password_count = $9, \
         last_ip = $10, last_date = $11, admin = $12 \
       WHERE id = $1";

    let result = sqlx::query(SQL)
        .bind(account.id)
        .bind(account.email.as_str())
        .bind(account.password_hash.as_str())
        .bind(account.salt.as_slice())
        .bind(account.banned)
        .bind(account.ban_reason.as_str())
        .bind(account.expiry_date)
        .bind(account.require_password_change)
        .bind(account.wrong_password_count)
        .bind(account.last_ip.as_str())
        .bind(account.last_date)
        .bind(account.admin)
        .execute(executor)
        .await
        .map_err(StoreError::Database)?;

    if result.rows_affected() == 0 {
        return Err(StoreError::NotFound("账号"));
    }

    Ok(())
}

/// 新建角色。
///
/// 只写入建号必需字段（姓名、职业、性别、发型、建号 IP），其余列取表默认值；
/// 角色名冲突返回 [`StoreError::CharacterNameTaken`]，账号不存在返回
/// [`StoreError::NotFound`]。
pub async fn create_character<'e, E>(executor: E, new: &NewCharacter) -> Result<Character>
where
    E: PgExecutor<'e>,
{
    let sql = format!(
        "INSERT INTO characters (account_id, name, class, gender, hair, creation_ip) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING {CHARACTER_COLUMNS}"
    );

    sqlx::query_as::<_, Character>(&sql)
        .bind(new.account_id)
        .bind(new.name.as_str())
        .bind(new.class)
        .bind(new.gender)
        .bind(new.hair)
        .bind(new.creation_ip.as_str())
        .fetch_one(executor)
        .await
        .map_err(|error| {
            StoreError::map_insert(
                error,
                StoreError::CharacterNameTaken(new.name.clone()),
                "账号",
            )
        })
}

/// 列出账号下未删除的角色，按创建顺序（主键升序）。
///
/// 对应 Crystal `AccountInfo.GetSelectInfo()`：已删除的角色不进选角列表。
pub async fn list_characters<'e, E>(executor: E, account_id: i64) -> Result<Vec<CharacterSummary>>
where
    E: PgExecutor<'e>,
{
    const SQL: &str = "SELECT id, name, level, class, gender, last_logout_date \
                       FROM characters WHERE account_id = $1 AND NOT deleted ORDER BY id";

    sqlx::query_as::<_, CharacterSummary>(SQL)
        .bind(account_id)
        .fetch_all(executor)
        .await
        .map_err(StoreError::Database)
}

/// 读一个完整角色。
///
/// 不存在或已被软删除都返回 [`StoreError::NotFound`]（Crystal 读档时同样不加载已删除角色）。
pub async fn load_character<'e, E>(executor: E, id: i64) -> Result<Character>
where
    E: PgExecutor<'e>,
{
    let sql = character_select("id = $1 AND NOT deleted");

    sqlx::query_as::<_, Character>(&sql)
        .bind(id)
        .fetch_optional(executor)
        .await
        .map_err(StoreError::Database)?
        .ok_or(StoreError::NotFound("角色"))
}

/// 回写角色的可变状态。
///
/// 可改字段：等级、封禁、登录/登出时间、位置与朝向、HP/MP/经验/货币、模式与权限、
/// 行会、三个物品大块、婚姻与师徒关系。姓名、归属账号、建号信息不可改。
/// 目标行不存在（例如已被硬删除）返回 [`StoreError::NotFound`]。
pub async fn save_character<'e, E>(executor: E, character: &Character) -> Result<()>
where
    E: PgExecutor<'e>,
{
    const SQL: &str = "UPDATE characters SET \
         level = $2, banned = $3, ban_reason = $4, expiry_date = $5, \
         last_ip = $6, last_login_date = $7, last_logout_date = $8, \
         deleted = $9, delete_date = $10, \
         current_map_index = $11, current_x = $12, current_y = $13, direction = $14, \
         bind_map_index = $15, bind_x = $16, bind_y = $17, \
         hp = $18, mp = $19, experience = $20, gold = $21, credit = $22, \
         attack_mode = $23, pet_mode = $24, pk_points = $25, \
         allow_group = $26, allow_trade = $27, allow_observe = $28, guild_index = $29, \
         inventory = $30, equipment = $31, storage = $32, \
         married = $33, married_date = $34, mentor = $35, mentor_date = $36, \
         is_mentor = $37, mentor_exp = $38 \
       WHERE id = $1";

    let result = sqlx::query(SQL)
        .bind(character.id)
        .bind(character.level)
        .bind(character.banned)
        .bind(character.ban_reason.as_str())
        .bind(character.expiry_date)
        .bind(character.last_ip.as_str())
        .bind(character.last_login_date)
        .bind(character.last_logout_date)
        .bind(character.deleted)
        .bind(character.delete_date)
        .bind(character.current_map_index)
        .bind(character.current_x)
        .bind(character.current_y)
        .bind(character.direction)
        .bind(character.bind_map_index)
        .bind(character.bind_x)
        .bind(character.bind_y)
        .bind(character.hp)
        .bind(character.mp)
        .bind(character.experience)
        .bind(character.gold)
        .bind(character.credit)
        .bind(character.attack_mode)
        .bind(character.pet_mode)
        .bind(character.pk_points)
        .bind(character.allow_group)
        .bind(character.allow_trade)
        .bind(character.allow_observe)
        .bind(character.guild_index)
        .bind(character.inventory.as_slice())
        .bind(character.equipment.as_slice())
        .bind(character.storage.as_slice())
        .bind(character.married)
        .bind(character.married_date)
        .bind(character.mentor)
        .bind(character.mentor_date)
        .bind(character.is_mentor)
        .bind(character.mentor_exp)
        .execute(executor)
        .await
        .map_err(StoreError::Database)?;

    if result.rows_affected() == 0 {
        return Err(StoreError::NotFound("角色"));
    }

    Ok(())
}

/// 软删除角色：置 `deleted`、写 `delete_date`，行本身保留给归档/宽限期使用。
///
/// `account_id` 必须与角色归属一致，避免越权删除；不匹配或已删除返回
/// [`StoreError::NotFound`]。
pub async fn delete_character<'e, E>(executor: E, id: i64, account_id: i64) -> Result<()>
where
    E: PgExecutor<'e>,
{
    const SQL: &str = "UPDATE characters SET deleted = TRUE, delete_date = now() \
                       WHERE id = $1 AND account_id = $2 AND NOT deleted";

    let result = sqlx::query(SQL)
        .bind(id)
        .bind(account_id)
        .execute(executor)
        .await
        .map_err(StoreError::Database)?;

    if result.rows_affected() == 0 {
        return Err(StoreError::NotFound("角色"));
    }

    Ok(())
}

/// 记录一次成功登录：更新最后登录时间并清零连续错误口令计数。
///
/// 对应 Crystal `Envir.cs:3903,3921`（`WrongPasswordCount = 0`、`LastDate = Now`）。
pub async fn mark_login<'e, E>(executor: E, account_id: i64) -> Result<()>
where
    E: PgExecutor<'e>,
{
    const SQL: &str =
        "UPDATE accounts SET last_date = now(), wrong_password_count = 0 WHERE id = $1";

    let result = sqlx::query(SQL)
        .bind(account_id)
        .execute(executor)
        .await
        .map_err(StoreError::Database)?;

    if result.rows_affected() == 0 {
        return Err(StoreError::NotFound("账号"));
    }

    Ok(())
}

/// 记录最后登录 IP（Crystal `Envir.cs:3922` 的 `account.LastIP = c.IPAddress`）。
///
/// 与 [`mark_login`] 分开是为了让「只改登录时间」和「顺带改 IP」都能各用一条 SQL。
pub async fn set_last_login_ip<'e, E>(executor: E, account_id: i64, ip: &str) -> Result<()>
where
    E: PgExecutor<'e>,
{
    const SQL: &str = "UPDATE accounts SET last_ip = $2 WHERE id = $1";

    let result = sqlx::query(SQL)
        .bind(account_id)
        .bind(ip)
        .execute(executor)
        .await
        .map_err(StoreError::Database)?;

    if result.rows_affected() == 0 {
        return Err(StoreError::NotFound("账号"));
    }

    Ok(())
}
