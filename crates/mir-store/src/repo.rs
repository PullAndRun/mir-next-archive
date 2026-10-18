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
//! | [`save_account`] | 回写账号的可变状态（封禁、强制改密、错误计数、口令、最后登录、界面语言） |
//! | [`set_account_language`] | 校验并归一化语言代码后写 `accounts.language` |
//! | [`create_character`] | 建角色；账号不存在返回 [`StoreError::NotFound`] |
//! | [`list_characters`] | 选角列表（未删除，按创建顺序） |
//! | [`load_character`] | 读完整角色；不存在或已软删除返回 [`StoreError::NotFound`] |
//! | [`save_character`] | 按主键回写可变字段（名字/账号/建号信息不可改） |
//! | [`delete_character`] | 软删除，写 `deleted`/`delete_date`，保留行 |
//! | [`mark_login`] | 记录最后登录时间并清零错误口令计数 |
//! | [`set_last_login_ip`] | 记录最后登录 IP |
//! | [`find_character_by_name`] | 按角色名（大小写不敏感）查角色，邮件收件人允许离线 |
//! | [`create_mail`] / [`load_mail`] / [`list_mail`] / [`save_mail`] / [`delete_mail`] | 邮件表读写（Crystal `MailInfo`） |
//! | [`list_conquests`] / [`load_conquest`] / [`save_conquest`] | 攻城区运行期状态（Crystal `ConquestGuildInfo`） |
//! | [`list_gameshop_stock`] / [`save_gameshop_stock`] | 商城全服已购计数（Crystal `Envir.GameshopLog`） |
//! | [`list_gameshop_purchases`] / [`save_gameshop_purchases`] | 商城按角色已购计数（Crystal `CharacterInfo.GSpurchases`） |
//!
//! 界面语言（`accounts.language`）的校验与归一化是纯函数，不查库：
//! [`normalize_language`] 把 `zh`/`cn`/`chs`/`en-us` 之类的别名归一成 `zh-cn`/`en`，
//! [`is_supported_language`] 是它的布尔包装。写库的两个入口
//! （[`create_account`] 与 [`set_account_language`]）都会先归一化再落库。

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgExecutor};

use crate::error::{Result, StoreError};
use crate::models::{
    Account, Character, CharacterSummary, Conquest, DEFAULT_LANGUAGE, GameShopPurchase,
    GameShopStock, Guild, GuildBuffRow, GuildMemberRow, GuildRankRow, GuildRow, GuildStorageRow,
    Mail, MarketAuction, NewAccount, NewCharacter,
};
use crate::password::{encode_hash, generate_salt, hash_password};

/// `accounts` 表的列顺序；`Account` 的 `FromRow` 按名字匹配，这里的顺序只用于手写 SQL 的可读性。
const ACCOUNT_COLUMNS: &str = "id, username, password_hash, salt, email, creation_ip, creation_date, \
     banned, ban_reason, expiry_date, require_password_change, wrong_password_count, last_ip, \
     last_date, admin, language";

/// `characters` 表的列顺序。
const CHARACTER_COLUMNS: &str = "id, account_id, name, level, class, gender, hair, creation_ip, \
     creation_date, banned, ban_reason, expiry_date, last_ip, last_login_date, last_logout_date, \
     deleted, delete_date, current_map_index, current_x, current_y, direction, bind_map_index, \
     bind_x, bind_y, hp, mp, experience, gold, credit, attack_mode, pet_mode, pk_points, \
     allow_group, allow_trade, allow_observe, guild_index, inventory, equipment, magics, quests, \
     storage, married, married_date, mentor, mentor_date, is_mentor, mentor_exp, quest_inventory, \
     last_daily_reset_at, friends";

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
///
/// `new.language` 会先经 [`normalize_language`] 归一化再落库，因此传 `en-US` 也写成 `en`；
/// 无法识别的代码返回 [`StoreError::InvalidLanguage`]（在 SQL 之前就返回，不会留下半行数据）。
pub async fn create_account<'e, E>(executor: E, new: &NewAccount) -> Result<Account>
where
    E: PgExecutor<'e>,
{
    let salt = generate_salt();
    // 存储口径：Crystal 的 lossy 字符串 → 标准 Base64（见 `password::encode_hash` 与迁移 0010）。
    // 24 个随机字节里出现 NUL 不会再把 TEXT 写入打挂。
    let password_hash = encode_hash(&hash_password(&new.password, &salt));
    let language = normalize_language(&new.language)
        .ok_or_else(|| StoreError::InvalidLanguage(new.language.clone()))?;
    let sql = format!(
        "INSERT INTO accounts (username, password_hash, salt, email, creation_ip, admin, language) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING {ACCOUNT_COLUMNS}"
    );

    sqlx::query_as::<_, Account>(&sql)
        .bind(new.username.as_str())
        .bind(password_hash)
        .bind(salt.as_slice())
        .bind(new.email.as_str())
        .bind(new.creation_ip.as_str())
        .bind(new.admin)
        .bind(language)
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
/// 最后登录 IP/时间、GM 标志、界面语言。登录名与建号信息不可改。
///
/// 改密时先用 [`Account::set_password`] 重新生成盐和散列，再调用本函数；封禁、
/// 解除封禁、清零错误计数同理。界面语言先经 [`normalize_language`] 归一化再写，
/// 无法识别的代码返回 [`StoreError::InvalidLanguage`]；只改语言时用
/// [`set_account_language`] 更省事。目标行不存在返回 [`StoreError::NotFound`]。
pub async fn save_account<'e, E>(executor: E, account: &Account) -> Result<()>
where
    E: PgExecutor<'e>,
{
    const SQL: &str = "UPDATE accounts SET \
         email = $2, password_hash = $3, salt = $4, banned = $5, ban_reason = $6, \
         expiry_date = $7, require_password_change = $8, wrong_password_count = $9, \
         last_ip = $10, last_date = $11, admin = $12, language = $13 \
       WHERE id = $1";

    let language = normalize_language(&account.language)
        .ok_or_else(|| StoreError::InvalidLanguage(account.language.clone()))?;

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
        .bind(language)
        .execute(executor)
        .await
        .map_err(StoreError::Database)?;

    if result.rows_affected() == 0 {
        return Err(StoreError::NotFound("账号"));
    }

    Ok(())
}

/// 修改账号的界面语言（游戏内切换语言时调用）。
///
/// `language` 可以是 [`normalize_language`] 认识的任何别名及其大小写变体
/// （例如 `EN`、`en-US`、`chs`），落库前统一归一成 `zh-cn`/`en`；
/// 无法识别时返回 [`StoreError::InvalidLanguage`]，该检查发生在 SQL 之前，
/// 因此不会触碰数据库。账号不存在返回 [`StoreError::NotFound`]。
pub async fn set_account_language<'e, E>(executor: E, account_id: i64, language: &str) -> Result<()>
where
    E: PgExecutor<'e>,
{
    const SQL: &str = "UPDATE accounts SET language = $2 WHERE id = $1";

    let normalized = normalize_language(language)
        .ok_or_else(|| StoreError::InvalidLanguage(language.to_owned()))?;

    let result = sqlx::query(SQL)
        .bind(account_id)
        .bind(normalized)
        .execute(executor)
        .await
        .map_err(StoreError::Database)?;

    if result.rows_affected() == 0 {
        return Err(StoreError::NotFound("账号"));
    }

    Ok(())
}

/// 把语言代码归一成受支持的 BCP-47 小写代码，无法识别时返回 `None`。
///
/// 接受大小写不敏感、允许首尾空白的别名：
///
/// | 输入 | 归一化结果 |
/// |---|---|
/// | `zh-cn`、`zh`、`cn`、`chs`、`zh_cn` | `zh-cn`（[`DEFAULT_LANGUAGE`]） |
/// | `en`、`en-us`、`en_us` | `en` |
/// | 其他（`fr`、`zh-tw`、空串等） | `None` |
///
/// 别名表与 `mir-core` 的 `Language::from_code` 保持一致，避免两层对同一输入给出不同结论。
pub fn normalize_language(code: &str) -> Option<&'static str> {
    /// 简体中文的别名（比较前已 trim，比较时大小写不敏感）。
    const CHINESE_ALIASES: [&str; 5] = ["zh-cn", "zh", "cn", "chs", "zh_cn"];
    /// 英文的别名。
    const ENGLISH_ALIASES: [&str; 3] = ["en", "en-us", "en_us"];

    let code = code.trim();

    if CHINESE_ALIASES
        .iter()
        .any(|alias| code.eq_ignore_ascii_case(alias))
    {
        Some(DEFAULT_LANGUAGE)
    } else if ENGLISH_ALIASES
        .iter()
        .any(|alias| code.eq_ignore_ascii_case(alias))
    {
        Some("en")
    } else {
        None
    }
}

/// 语言代码是否受支持；等价于 [`normalize_language`] 返回 `Some`。
pub fn is_supported_language(code: &str) -> bool {
    normalize_language(code).is_some()
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

/// 按角色名查一个完整角色，大小写不敏感。
///
/// 对应 Crystal `Envir.GetCharacterInfo(string name)`（`Server/MirEnvir/Envir.cs:4712-4719`）：
/// `string.Compare(Name, name, OrdinalIgnoreCase) == 0`，**不看是否在线**——邮件收件人
/// 允许离线。已软删除的角色不返回（Crystal 的 `CharacterList` 里也没有它们）。
pub async fn find_character_by_name<'e, E>(executor: E, name: &str) -> Result<Option<Character>>
where
    E: PgExecutor<'e>,
{
    let sql = character_select("lower(name) = lower($1) AND NOT deleted");

    sqlx::query_as::<_, Character>(&sql)
        .bind(name)
        .fetch_optional(executor)
        .await
        .map_err(StoreError::Database)
}

/// 回写角色的可变状态。
///
/// 可改字段：等级、封禁、登录/登出时间、位置与朝向、HP/MP/经验/货币、模式与权限、
/// 行会、背包/装备/仓库/技能/任务/任务物品栏六个二进制块、婚姻与师徒关系。
/// 姓名、归属账号、建号信息不可改。目标行不存在（例如已被硬删除）返回 [`StoreError::NotFound`]。
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
         is_mentor = $37, mentor_exp = $38, magics = $39, quests = $40, \
         quest_inventory = $41, last_daily_reset_at = $42, friends = $43 \
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
        .bind(character.magics.as_slice())
        .bind(character.quests.as_slice())
        .bind(character.quest_inventory.as_slice())
        .bind(character.last_daily_reset_at)
        .bind(character.friends.as_slice())
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

// ---------------------------------------------------------------------------
// 行会（Crystal `GuildInfo`）
// ---------------------------------------------------------------------------
//
// 这些函数**不用** `PgExecutor`，而是显式收 `&mut PgConnection`：它们都是
// 「一条主表语句 + 两张子表的批量替换」，需要调用方把整组语句放进同一个事务
// （`&mut *tx` 可以直接传进来）。单个 `PgExecutor` 每调用一次就被消费，
// 无法承载多条语句。`PgAccountStore` 里的实现就是这么包的。

/// `guilds` 表的列（[`GuildRow`] 的 `FromRow` 按名字匹配）。
const GUILD_COLUMNS: &str =
    "id, name, leader_character_id, level, experience, gold, spare_points, notice, allies";

/// 建一个行会：写 `guilds` 一行 + 全部职位 + 全部成员，返回带主键的 [`Guild`]。
///
/// 对应 Crystal `new GuildInfo(owner, name)`（`Server/MirDatabase/GuildInfo.cs:50-79`）：
/// 创建时 `Level = 0`，只含一个 `Leader` 职位（`Options = 255`，`:54`）。行会名唯一
/// （大小写不敏感）冲突返回 [`StoreError::GuildExists`]；会长角色不存在返回
/// [`StoreError::NotFound`]。
///
/// `guild.id` 会被忽略（以数据库自增为准），其余字段原样落库；返回值的 `id`
/// 是数据库分配的，调用方要把它写回 `characters.guild_index`（[`set_character_guild`]）。
pub async fn create_guild(conn: &mut PgConnection, guild: &Guild) -> Result<Guild> {
    let columns =
        "name, leader_character_id, level, experience, gold, spare_points, notice, allies";
    // `id > 0` 时按世界层分配的号写（对应 Crystal `Envir.NextGuildID`，`Envir.cs:126`），
    // 否则交给 `GENERATED BY DEFAULT AS IDENTITY` 分配。
    let (sql, explicit_id) = if guild.id > 0 {
        (
            format!(
                "INSERT INTO guilds (id, {columns}) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING {GUILD_COLUMNS}"
            ),
            Some(guild.id),
        )
    } else {
        (
            format!(
                "INSERT INTO guilds ({columns}) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING {GUILD_COLUMNS}"
            ),
            None,
        )
    };

    let mut query = sqlx::query_as::<_, GuildRow>(&sql);
    if let Some(id) = explicit_id {
        query = query.bind(id);
    }
    let row: GuildRow = query
        .bind(guild.name.as_str())
        .bind(guild.leader_character_id)
        .bind(guild.level)
        .bind(guild.experience)
        .bind(guild.gold)
        .bind(guild.spare_points)
        .bind(guild.notice_text())
        .bind(guild.allies.as_slice())
        .fetch_one(&mut *conn)
        .await
        .map_err(|error| {
            StoreError::map_insert(error, StoreError::GuildExists(guild.name.clone()), "角色")
        })?;

    let id = row.id;
    if explicit_id.is_some() {
        // 显式写入 ID 不会推进 identity 序列；把它推到当前最大值，避免之后自动分配撞号。
        sqlx::query(
            "SELECT setval(pg_get_serial_sequence('guilds', 'id'), \
             GREATEST((SELECT COALESCE(MAX(id), 0) FROM guilds), $1))",
        )
        .bind(id)
        .execute(&mut *conn)
        .await
        .map_err(StoreError::Database)?;
    }

    for rank in &guild.ranks {
        insert_rank(&mut *conn, id, rank.rank_index, &rank.name, rank.options).await?;
    }
    for member in &guild.members {
        insert_member(
            &mut *conn,
            id,
            member.character_id,
            member.rank_index,
            member.joined_at,
        )
        .await?;
    }
    for item in &guild.storage {
        insert_storage(&mut *conn, id, item).await?;
    }
    for buff in &guild.buffs {
        insert_buff(&mut *conn, id, buff).await?;
    }

    Ok(Guild {
        id,
        name: row.name,
        leader_character_id: row.leader_character_id,
        level: row.level,
        experience: row.experience,
        gold: row.gold,
        spare_points: row.spare_points,
        notice: Guild::split_notice(&row.notice),
        allies: row.allies,
        // 返回值的 `guild_id` 要填成新主键（入参里的那两列只是占位）。
        ranks: guild
            .ranks
            .iter()
            .map(|rank| GuildRankRow {
                guild_id: id,
                ..rank.clone()
            })
            .collect(),
        members: guild
            .members
            .iter()
            .map(|member| GuildMemberRow {
                guild_id: id,
                ..member.clone()
            })
            .collect(),
        storage: guild
            .storage
            .iter()
            .map(|item| GuildStorageRow {
                guild_id: id,
                ..item.clone()
            })
            .collect(),
        buffs: guild
            .buffs
            .iter()
            .map(|buff| GuildBuffRow {
                guild_id: id,
                ..buff.clone()
            })
            .collect(),
    })
}

/// 插入一行职位。
async fn insert_rank(
    conn: &mut PgConnection,
    guild_id: i32,
    rank_index: i16,
    name: &str,
    options: i16,
) -> Result<()> {
    const SQL: &str =
        "INSERT INTO guild_ranks (guild_id, rank_index, name, options) VALUES ($1, $2, $3, $4)";

    sqlx::query(SQL)
        .bind(guild_id)
        .bind(rank_index)
        .bind(name)
        .bind(options)
        .execute(conn)
        .await
        .map_err(StoreError::Database)?;
    Ok(())
}

/// 插入一行成员。
async fn insert_member(
    conn: &mut PgConnection,
    guild_id: i32,
    character_id: i64,
    rank_index: i16,
    joined_at: DateTime<Utc>,
) -> Result<()> {
    const SQL: &str = "INSERT INTO guild_members (character_id, guild_id, rank_index, joined_at) \
                       VALUES ($1, $2, $3, $4)";

    sqlx::query(SQL)
        .bind(character_id)
        .bind(guild_id)
        .bind(rank_index)
        .bind(joined_at)
        .execute(conn)
        .await
        .map_err(|error| {
            StoreError::map_insert(error, StoreError::GuildMemberExists(character_id), "角色")
        })?;
    Ok(())
}

/// 插入一行仓库格子。
///
/// `item` 是 `encode_slots(&[Some(UserItem)])` 的字节块（见
/// `migrations/0006_guild_storage.sql`）；`guild_id` 由调用方给（入参里的那份只作占位）。
async fn insert_storage(
    conn: &mut PgConnection,
    guild_id: i32,
    item: &GuildStorageRow,
) -> Result<()> {
    const SQL: &str = "INSERT INTO guild_storage (guild_id, slot, item, user_id, stored_at) \
                       VALUES ($1, $2, $3, $4, $5)";

    sqlx::query(SQL)
        .bind(guild_id)
        .bind(item.slot)
        .bind(item.item.as_slice())
        .bind(item.user_id)
        .bind(item.stored_at)
        .execute(conn)
        .await
        .map_err(StoreError::Database)?;
    Ok(())
}

/// 插入一行行会增益。
async fn insert_buff(conn: &mut PgConnection, guild_id: i32, buff: &GuildBuffRow) -> Result<()> {
    const SQL: &str = "INSERT INTO guild_buffs (guild_id, buff_id, active, active_time_remaining) \
                       VALUES ($1, $2, $3, $4)";

    sqlx::query(SQL)
        .bind(guild_id)
        .bind(buff.buff_id)
        .bind(buff.active)
        .bind(buff.active_time_remaining)
        .execute(conn)
        .await
        .map_err(StoreError::Database)?;
    Ok(())
}

/// 整份回写一个行会（`guilds` 一行 + 职位/成员/仓库/增益全量替换）。
///
/// 对应 Crystal `GuildInfo.Save`（`Server/MirDatabase/GuildInfo.cs:189-255`）。
///
/// **简化**：Crystal 只保存「有成员的职位」（`:197-203`）；本作全量保存，
/// 因为客户端可以新建空职位（`GuildObject.NewRank`，`Server/MirObjects/GuildObject.cs:334-352`），
/// 丢掉它们会让玩家刚建好的职位消失。
///
/// 目标行不存在返回 [`StoreError::NotFound`]。调用方应把本函数放进事务。
pub async fn save_guild(conn: &mut PgConnection, guild: &Guild) -> Result<()> {
    const SQL: &str = "UPDATE guilds SET name = $2, leader_character_id = $3, level = $4, \
                       experience = $5, gold = $6, spare_points = $7, notice = $8, \
                       allies = $9, updated_at = now() WHERE id = $1";

    let result = sqlx::query(SQL)
        .bind(guild.id)
        .bind(guild.name.as_str())
        .bind(guild.leader_character_id)
        .bind(guild.level)
        .bind(guild.experience)
        .bind(guild.gold)
        .bind(guild.spare_points)
        .bind(guild.notice_text())
        .bind(guild.allies.as_slice())
        .execute(&mut *conn)
        .await
        .map_err(|error| {
            StoreError::map_insert(error, StoreError::GuildExists(guild.name.clone()), "角色")
        })?;

    if result.rows_affected() == 0 {
        return Err(StoreError::NotFound("行会"));
    }

    sqlx::query("DELETE FROM guild_ranks WHERE guild_id = $1")
        .bind(guild.id)
        .execute(&mut *conn)
        .await
        .map_err(StoreError::Database)?;
    sqlx::query("DELETE FROM guild_members WHERE guild_id = $1")
        .bind(guild.id)
        .execute(&mut *conn)
        .await
        .map_err(StoreError::Database)?;
    sqlx::query("DELETE FROM guild_storage WHERE guild_id = $1")
        .bind(guild.id)
        .execute(&mut *conn)
        .await
        .map_err(StoreError::Database)?;
    sqlx::query("DELETE FROM guild_buffs WHERE guild_id = $1")
        .bind(guild.id)
        .execute(&mut *conn)
        .await
        .map_err(StoreError::Database)?;

    for rank in &guild.ranks {
        insert_rank(
            &mut *conn,
            guild.id,
            rank.rank_index,
            &rank.name,
            rank.options,
        )
        .await?;
    }
    for member in &guild.members {
        insert_member(
            &mut *conn,
            guild.id,
            member.character_id,
            member.rank_index,
            member.joined_at,
        )
        .await?;
    }
    for item in &guild.storage {
        insert_storage(&mut *conn, guild.id, item).await?;
    }
    for buff in &guild.buffs {
        insert_buff(&mut *conn, guild.id, buff).await?;
    }

    Ok(())
}

/// 删除一个行会（职位与成员由外键级联删除）。
///
/// 对应 Crystal `Envir.DeleteGuild`（`Server/MirEnvir/Envir.cs:5478-5485`）：
/// 那边只是从内存列表里移除，下一次 `SaveGuilds` 不再写文件；本作直接删行。
/// 目标行不存在返回 [`StoreError::NotFound`]。
pub async fn delete_guild(conn: &mut PgConnection, guild_id: i32) -> Result<()> {
    let result = sqlx::query("DELETE FROM guilds WHERE id = $1")
        .bind(guild_id)
        .execute(&mut *conn)
        .await
        .map_err(StoreError::Database)?;

    if result.rows_affected() == 0 {
        return Err(StoreError::NotFound("行会"));
    }

    Ok(())
}

/// 按主键读一个行会（含职位与成员）。
///
/// 不存在返回 [`StoreError::NotFound`]。
pub async fn load_guild(conn: &mut PgConnection, guild_id: i32) -> Result<Guild> {
    let sql = format!("SELECT {GUILD_COLUMNS} FROM guilds WHERE id = $1");

    let row: GuildRow = sqlx::query_as(&sql)
        .bind(guild_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(StoreError::Database)?
        .ok_or(StoreError::NotFound("行会"))?;

    let ranks = load_ranks(&mut *conn, guild_id).await?;
    let members = load_members(&mut *conn, guild_id).await?;
    let storage = load_storage(&mut *conn, guild_id).await?;
    let buffs = load_buffs(&mut *conn, guild_id).await?;

    Ok(Guild {
        id: row.id,
        name: row.name,
        leader_character_id: row.leader_character_id,
        level: row.level,
        experience: row.experience,
        gold: row.gold,
        spare_points: row.spare_points,
        notice: Guild::split_notice(&row.notice),
        allies: row.allies,
        ranks,
        members,
        storage,
        buffs,
    })
}

/// 按名字读行会，语义照抄 Crystal `Envir.GetGuild(string)`（`Envir.cs:4832-4841`）：
/// **去掉空格 + 大小写不敏感** 比较（`Guilds[i].Name.Replace(" ", "")`）。
pub async fn load_guild_by_name(conn: &mut PgConnection, name: &str) -> Result<Option<Guild>> {
    let sql = format!(
        "SELECT {GUILD_COLUMNS} FROM guilds \
         WHERE lower(replace(name, ' ', '')) = lower(replace($1, ' ', ''))"
    );

    let row: Option<GuildRow> = sqlx::query_as(&sql)
        .bind(name)
        .fetch_optional(&mut *conn)
        .await
        .map_err(StoreError::Database)?;
    let Some(row) = row else {
        return Ok(None);
    };

    let ranks = load_ranks(&mut *conn, row.id).await?;
    let members = load_members(&mut *conn, row.id).await?;
    let storage = load_storage(&mut *conn, row.id).await?;
    let buffs = load_buffs(&mut *conn, row.id).await?;

    Ok(Some(Guild {
        id: row.id,
        name: row.name,
        leader_character_id: row.leader_character_id,
        level: row.level,
        experience: row.experience,
        gold: row.gold,
        spare_points: row.spare_points,
        notice: Guild::split_notice(&row.notice),
        allies: row.allies,
        ranks,
        members,
        storage,
        buffs,
    }))
}

/// 读出全部行会（启动时灌进世界，对应 Crystal `Envir.LoadGuilds`，`Envir.cs:3089-3117`）。
///
/// 五条查询（行会 / 职位 / 成员 / 仓库 / 增益）后在内存里拼装，避免 O(行会数) 次往返。
pub async fn list_guilds(conn: &mut PgConnection) -> Result<Vec<Guild>> {
    let sql = format!("SELECT {GUILD_COLUMNS} FROM guilds ORDER BY id");

    let rows: Vec<GuildRow> = sqlx::query_as(&sql)
        .fetch_all(&mut *conn)
        .await
        .map_err(StoreError::Database)?;

    let rank_rows: Vec<GuildRankRow> = sqlx::query_as(
        "SELECT guild_id, rank_index, name, options FROM guild_ranks \
         ORDER BY guild_id, rank_index",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(StoreError::Database)?;

    let member_rows: Vec<GuildMemberRow> = sqlx::query_as(
        "SELECT m.guild_id, m.character_id, COALESCE(c.name, '') AS name, m.rank_index, \
                m.joined_at \
         FROM guild_members m LEFT JOIN characters c ON c.id = m.character_id \
         ORDER BY m.guild_id, m.joined_at, m.character_id",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(StoreError::Database)?;

    let storage_rows: Vec<GuildStorageRow> = sqlx::query_as(
        "SELECT guild_id, slot, item, user_id, stored_at FROM guild_storage \
         ORDER BY guild_id, slot",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(StoreError::Database)?;

    let buff_rows: Vec<GuildBuffRow> = sqlx::query_as(
        "SELECT guild_id, buff_id, active, active_time_remaining FROM guild_buffs \
         ORDER BY guild_id, buff_id",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(StoreError::Database)?;

    let mut guilds: Vec<Guild> = rows
        .into_iter()
        .map(|row| Guild {
            id: row.id,
            name: row.name,
            leader_character_id: row.leader_character_id,
            level: row.level,
            experience: row.experience,
            gold: row.gold,
            spare_points: row.spare_points,
            notice: Guild::split_notice(&row.notice),
            allies: row.allies,
            ranks: Vec::new(),
            members: Vec::new(),
            storage: Vec::new(),
            buffs: Vec::new(),
        })
        .collect();

    for rank in rank_rows {
        if let Some(guild) = guilds.iter_mut().find(|guild| guild.id == rank.guild_id) {
            guild.ranks.push(rank);
        }
    }
    for member in member_rows {
        if let Some(guild) = guilds.iter_mut().find(|guild| guild.id == member.guild_id) {
            guild.members.push(member);
        }
    }
    for item in storage_rows {
        if let Some(guild) = guilds.iter_mut().find(|guild| guild.id == item.guild_id) {
            guild.storage.push(item);
        }
    }
    for buff in buff_rows {
        if let Some(guild) = guilds.iter_mut().find(|guild| guild.id == buff.guild_id) {
            guild.buffs.push(buff);
        }
    }

    Ok(guilds)
}

/// 读某行会的职位。
async fn load_ranks(conn: &mut PgConnection, guild_id: i32) -> Result<Vec<GuildRankRow>> {
    sqlx::query_as(
        "SELECT guild_id, rank_index, name, options FROM guild_ranks \
         WHERE guild_id = $1 ORDER BY rank_index",
    )
    .bind(guild_id)
    .fetch_all(conn)
    .await
    .map_err(StoreError::Database)
}

/// 读某行会的成员（名字联 `characters`）。
async fn load_members(conn: &mut PgConnection, guild_id: i32) -> Result<Vec<GuildMemberRow>> {
    sqlx::query_as(
        "SELECT m.guild_id, m.character_id, COALESCE(c.name, '') AS name, m.rank_index, \
                m.joined_at \
         FROM guild_members m LEFT JOIN characters c ON c.id = m.character_id \
         WHERE m.guild_id = $1 ORDER BY m.joined_at, m.character_id",
    )
    .bind(guild_id)
    .fetch_all(conn)
    .await
    .map_err(StoreError::Database)
}

/// 读某行会的仓库格子（按 `slot` 升序）。
async fn load_storage(conn: &mut PgConnection, guild_id: i32) -> Result<Vec<GuildStorageRow>> {
    sqlx::query_as(
        "SELECT guild_id, slot, item, user_id, stored_at FROM guild_storage \
         WHERE guild_id = $1 ORDER BY slot",
    )
    .bind(guild_id)
    .fetch_all(conn)
    .await
    .map_err(StoreError::Database)
}

/// 读某行会的增益。
async fn load_buffs(conn: &mut PgConnection, guild_id: i32) -> Result<Vec<GuildBuffRow>> {
    sqlx::query_as(
        "SELECT guild_id, buff_id, active, active_time_remaining FROM guild_buffs \
         WHERE guild_id = $1 ORDER BY buff_id",
    )
    .bind(guild_id)
    .fetch_all(conn)
    .await
    .map_err(StoreError::Database)
}

/// 写角色的 [`Character::guild_index`]（`-1` = 没有行会）。
///
/// 对应 Crystal 的 `CharacterInfo.GuildIndex` 落盘（`Server/MirDatabase/CharacterInfo.cs`）。
/// 只改这一列：成员的所属行会由 [`save_guild`] 写 `guild_members` 负责。
/// 角色不存在返回 [`StoreError::NotFound`]。
pub async fn set_character_guild(
    conn: &mut PgConnection,
    character_id: i64,
    guild_id: i32,
) -> Result<()> {
    let result = sqlx::query("UPDATE characters SET guild_index = $2 WHERE id = $1")
        .bind(character_id)
        .bind(guild_id)
        .execute(&mut *conn)
        .await
        .map_err(StoreError::Database)?;

    if result.rows_affected() == 0 {
        return Err(StoreError::NotFound("角色"));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// 邮件（Crystal `MailInfo`）
// ---------------------------------------------------------------------------
//
// 与行会函数同一套约定：显式收 `&mut PgConnection`，调用方负责事务
// （`create_mail` 需要「插入 + 推进序列」两条语句）。

/// `mail` 表的列（[`Mail`] 的 `FromRow` 按名字匹配）。
const MAIL_COLUMNS: &str = "id, character_id, sender_name, subject, message, items, gold, \
                            date_sent, opened, locked, collected";

/// 插入一封邮件，返回落库后的行。
///
/// 对应 Crystal `MailInfo.Send` 把邮件 `Add` 进收件箱（`Server/MirDatabase/MailInfo.cs:135`）。
/// 发件箱不存在——Crystal 只有收件人的 `CharacterInfo.Mail`。
///
/// `mail.id > 0` 时按调用方给的号写入（对应 Crystal 的 `++Envir.NextMailID`，
/// `PlayerObject.cs:11835`），并像 [`create_guild`] 一样把 `mail_id_seq` 推到当前最大值，
/// 避免之后自动分配撞号；`id <= 0` 交给 `BIGSERIAL` 分配。
pub async fn create_mail(conn: &mut PgConnection, mail: &Mail) -> Result<Mail> {
    let columns = "character_id, sender_name, subject, message, items, gold, date_sent, \
                   opened, locked, collected";
    let (sql, explicit_id) = if mail.id > 0 {
        (
            format!(
                "INSERT INTO mail (id, {columns}) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) RETURNING {MAIL_COLUMNS}"
            ),
            Some(mail.id),
        )
    } else {
        (
            format!(
                "INSERT INTO mail ({columns}) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING {MAIL_COLUMNS}"
            ),
            None,
        )
    };

    let mut query = sqlx::query_as::<_, Mail>(&sql);
    if let Some(id) = explicit_id {
        query = query.bind(id);
    }
    let row = query
        .bind(mail.character_id)
        .bind(mail.sender_name.as_str())
        .bind(mail.subject.as_str())
        .bind(mail.message.as_str())
        .bind(mail.items.as_slice())
        .bind(mail.gold)
        .bind(mail.date_sent)
        .bind(mail.opened)
        .bind(mail.locked)
        .bind(mail.collected)
        .fetch_one(&mut *conn)
        .await
        .map_err(|error| StoreError::map_insert(error, StoreError::MailExists(mail.id), "角色"))?;

    if explicit_id.is_some() {
        sqlx::query(
            "SELECT setval(pg_get_serial_sequence('mail', 'id'), \
             GREATEST((SELECT COALESCE(MAX(id), 0) FROM mail), $1))",
        )
        .bind(row.id)
        .execute(&mut *conn)
        .await
        .map_err(StoreError::Database)?;
    }

    Ok(row)
}

/// 读一封邮件；不存在返回 [`StoreError::NotFound`]。
pub async fn load_mail(conn: &mut PgConnection, mail_id: i64) -> Result<Mail> {
    let sql = format!("SELECT {MAIL_COLUMNS} FROM mail WHERE id = $1");

    sqlx::query_as::<_, Mail>(&sql)
        .bind(mail_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(StoreError::Database)?
        .ok_or(StoreError::NotFound("邮件"))
}

/// 列出某个角色的全部邮件，按 `MailID` 升序（= Crystal 的 `List<MailInfo>` 追加序）。
///
/// 过期过滤由服务端做（Crystal 没有邮件过期，见 `migrations/0007_mail.sql` 的文件头）。
pub async fn list_mail(conn: &mut PgConnection, character_id: i64) -> Result<Vec<Mail>> {
    let sql = format!("SELECT {MAIL_COLUMNS} FROM mail WHERE character_id = $1 ORDER BY id");

    sqlx::query_as::<_, Mail>(&sql)
        .bind(character_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(StoreError::Database)
}

/// 回写一封邮件的可变状态（已读、锁定、包裹受理、附件被领走后的空附件）。
///
/// 可改字段：`opened` / `locked` / `collected` / `items` / `gold`。收件人与发件人不可改
/// （Crystal 的 `MailInfo` 也没有转发语义）。目标行不存在返回 [`StoreError::NotFound`]。
pub async fn save_mail(conn: &mut PgConnection, mail: &Mail) -> Result<()> {
    const SQL: &str = "UPDATE mail SET items = $2, gold = $3, opened = $4, locked = $5, \
                       collected = $6, message = $7, subject = $8 \
                       WHERE id = $1";

    let result = sqlx::query(SQL)
        .bind(mail.id)
        .bind(mail.items.as_slice())
        .bind(mail.gold)
        .bind(mail.opened)
        .bind(mail.locked)
        .bind(mail.collected)
        .bind(mail.message.as_str())
        .bind(mail.subject.as_str())
        .execute(&mut *conn)
        .await
        .map_err(StoreError::Database)?;

    if result.rows_affected() == 0 {
        return Err(StoreError::NotFound("邮件"));
    }

    Ok(())
}

/// 删除一封邮件（领取完附件后由玩家主动删除，或过期清理）。
pub async fn delete_mail(conn: &mut PgConnection, mail_id: i64) -> Result<()> {
    sqlx::query("DELETE FROM mail WHERE id = $1")
        .bind(mail_id)
        .execute(&mut *conn)
        .await
        .map_err(StoreError::Database)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 市场（Crystal `AuctionInfo`）
// ---------------------------------------------------------------------------
//
// Crystal 把挂单写进 MirADB 的全局段（`Envir.SaveAccounts`，`Server/MirEnvir/Envir.cs:2591-2594`；
// 读入 `:3018-3029`），不是角色行——本表同样是全局表。字段对照见
// `migrations/0009_market.sql`。

/// `market_auctions` 表的列（[`MarketAuction`] 的 `FromRow` 按名字匹配）。
const MARKET_AUCTION_COLUMNS: &str = "id, item, consignment_date, price, current_bid, \
                                      seller_character_id, seller_name, \
                                      current_buyer_character_id, current_buyer_name, \
                                      expired, sold, item_type";

/// 列出全部市场挂单，按 `id` 降序（新 → 旧，等价于 Crystal `Envir.Auctions` 的 `AddFirst` 顺序，
/// `PlayerObject.cs:8302`）。
///
/// 对应 Crystal `Envir.LoadAccounts` 里按存档顺序重建 `Auctions` 的那段
/// （`Envir.cs:3018-3029`）。服务端启动时一次读全表，再灌进 `World`。
pub async fn list_auctions(conn: &mut PgConnection) -> Result<Vec<MarketAuction>> {
    let sql = format!("SELECT {MARKET_AUCTION_COLUMNS} FROM market_auctions ORDER BY id DESC");

    sqlx::query_as::<_, MarketAuction>(&sql)
        .fetch_all(conn)
        .await
        .map_err(StoreError::Database)
}

/// 用给定快照**整表替换**市场挂单（先 `DELETE` 再逐条 `INSERT`）。
///
/// 服务端世界层持有全部挂单（`World::auctions`），所以每次落盘都是一次全量快照：
/// 新增/改价/成交/下架/过期清理都只需传当前快照，不需要增量写入队列。
/// `save_auctions(&mut tx, &[])` 等价于清空表。
///
/// 对应 Crystal 每次 `SaveAccounts` 重写整个 `Auctions` 段（`Envir.cs:2591-2594`）。
/// 调用方应把本函数放进事务（`&mut *tx` 可以直接传进来），避免删完还没插入时进程退出。
pub async fn save_auctions(conn: &mut PgConnection, auctions: &[MarketAuction]) -> Result<()> {
    const SQL: &str = "INSERT INTO market_auctions (id, item, consignment_date, price, \
                       current_bid, seller_character_id, seller_name, \
                       current_buyer_character_id, current_buyer_name, expired, sold, item_type) \
                       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)";

    sqlx::query("DELETE FROM market_auctions")
        .execute(&mut *conn)
        .await
        .map_err(StoreError::Database)?;

    for auction in auctions {
        sqlx::query(SQL)
            .bind(auction.id)
            .bind(auction.item.as_slice())
            .bind(auction.consignment_date)
            .bind(auction.price)
            .bind(auction.current_bid)
            .bind(auction.seller_character_id)
            .bind(auction.seller_name.as_str())
            .bind(auction.current_buyer_character_id)
            .bind(auction.current_buyer_name.as_deref())
            .bind(auction.expired)
            .bind(auction.sold)
            .bind(auction.item_type)
            .execute(&mut *conn)
            .await
            .map_err(StoreError::Database)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// 商城（Crystal `GameshopLog` / `CharacterInfo.GSpurchases`）
// ---------------------------------------------------------------------------
//
// Crystal 把「全服已购」放进 MirADB 存档的全局段（`Envir.GameshopLog`，
// `Server/MirEnvir/Envir.cs:2598-2599` 写、`:3054` 读），把「按角色已购」随角色存档
// （`CharacterInfo.GSpurchases`）。本作落两张表，字段对照见
// `migrations/0011_gameshop.sql`。元宝余额不在这里——它沿用 `characters.credit`。

/// `gameshop_stock` 表的列（[`GameShopStock`] 的 `FromRow` 按名字匹配）。
const GAMESHOP_STOCK_COLUMNS: &str = "g_index, purchased";

/// `gameshop_purchases` 表的列。
const GAMESHOP_PURCHASE_COLUMNS: &str = "character_id, g_index, quantity";

/// 读出**全服**已购计数（Crystal `Envir.GameshopLog`，`Envir.cs:3054`）。
///
/// 服务端每次商城购买前读一次：剩余库存 = `GameShopItem.Stock - purchased`
/// （`PlayerObject.cs:13712-13717`）。表为空表示没有任何限量商品被买过。
pub async fn list_gameshop_stock<'e, E>(executor: E) -> Result<Vec<GameShopStock>>
where
    E: PgExecutor<'e>,
{
    let sql = format!("SELECT {GAMESHOP_STOCK_COLUMNS} FROM gameshop_stock ORDER BY g_index");

    sqlx::query_as::<_, GameShopStock>(&sql)
        .fetch_all(executor)
        .await
        .map_err(StoreError::Database)
}

/// 读出某个角色的**个人**已购计数（Crystal `CharacterInfo.GSpurchases`）。
pub async fn list_gameshop_purchases<'e, E>(
    executor: E,
    character_id: i64,
) -> Result<Vec<GameShopPurchase>>
where
    E: PgExecutor<'e>,
{
    let sql = format!(
        "SELECT {GAMESHOP_PURCHASE_COLUMNS} FROM gameshop_purchases \
         WHERE character_id = $1 ORDER BY g_index"
    );

    sqlx::query_as::<_, GameShopPurchase>(&sql)
        .bind(character_id)
        .fetch_all(executor)
        .await
        .map_err(StoreError::Database)
}

/// 写入（插入或更新）一批全服已购计数。
///
/// 用 `ON CONFLICT ... DO UPDATE`：同一件商品反复购买只更新一行，
/// 与 Crystal 直接改 `Dictionary` 再整段重写存档等价。调用方应放进事务。
pub async fn save_gameshop_stock(conn: &mut PgConnection, rows: &[GameShopStock]) -> Result<()> {
    const SQL: &str = "INSERT INTO gameshop_stock (g_index, purchased) VALUES ($1, $2) \
                       ON CONFLICT (g_index) DO UPDATE SET \
                       purchased = EXCLUDED.purchased, updated_at = now()";

    for row in rows {
        sqlx::query(SQL)
            .bind(row.g_index)
            .bind(row.purchased)
            .execute(&mut *conn)
            .await
            .map_err(StoreError::Database)?;
    }

    Ok(())
}

/// 写入（插入或更新）一批按角色的已购计数。
pub async fn save_gameshop_purchases(
    conn: &mut PgConnection,
    rows: &[GameShopPurchase],
) -> Result<()> {
    const SQL: &str = "INSERT INTO gameshop_purchases (character_id, g_index, quantity) \
                       VALUES ($1, $2, $3) \
                       ON CONFLICT (character_id, g_index) DO UPDATE SET \
                       quantity = EXCLUDED.quantity, updated_at = now()";

    for row in rows {
        sqlx::query(SQL)
            .bind(row.character_id)
            .bind(row.g_index)
            .bind(row.quantity)
            .execute(&mut *conn)
            .await
            .map_err(StoreError::Database)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// 攻城（Crystal `ConquestGuildInfo`）
// ---------------------------------------------------------------------------
//
// Crystal 把每个攻城区的运行期状态写成 `Conquests/<index>.mcd` 一个二进制文件
// （`Envir.SaveConquests`，`Server/MirEnvir/Envir.cs:2734-2747`），读入见
// `Envir.LoadConquests`（`:3119-3170`）。本作落一张 `conquests` 表，
// 字段对照见 `migrations/0008_conquests.sql`。

/// `conquests` 表的列（[`Conquest`] 的 `FromRow` 按名字匹配）。
const CONQUEST_COLUMNS: &str = "conquest_index, owner_guild_id, attacker_guild_id, gold_storage, \
                                npc_rate, war_is_on, war_start_time, war_end_time, start_type, \
                                game_type, schedule_start_hour, schedule_war_length, \
                                schedule_days, king_points, flag_owners, next_war_at";

/// 列出全部攻城区的运行期状态（服务端启动时灌进世界）。
///
/// 对应 Crystal `Envir.LoadConquests`（`Envir.cs:3119-3170`）遍历 `ConquestInfoList`
/// 逐个读 `<index>.mcd` 的做法；本作一次查完全表。
pub async fn list_conquests(conn: &mut PgConnection) -> Result<Vec<Conquest>> {
    let sql = format!("SELECT {CONQUEST_COLUMNS} FROM conquests ORDER BY conquest_index");

    sqlx::query_as::<_, Conquest>(&sql)
        .fetch_all(&mut *conn)
        .await
        .map_err(StoreError::Database)
}

/// 读一个攻城区的状态；不存在返回 [`StoreError::NotFound`]。
pub async fn load_conquest(conn: &mut PgConnection, conquest_index: i32) -> Result<Conquest> {
    let sql = format!("SELECT {CONQUEST_COLUMNS} FROM conquests WHERE conquest_index = $1");

    sqlx::query_as::<_, Conquest>(&sql)
        .bind(conquest_index)
        .fetch_optional(&mut *conn)
        .await
        .map_err(StoreError::Database)?
        .ok_or(StoreError::NotFound("攻城"))
}

/// 写入（插入或整行更新）一个攻城区的状态。
///
/// 用 `ON CONFLICT ... DO UPDATE` 而不是「先查再更新」：Crystal 的 `SaveConquests`
/// 只写 `NeedSave` 的那些（`Envir.cs:2739`），新建的攻城区第一次落盘时文件还不存在
/// （`:3156` 的 `NeedSave = true`），本作把两种情况合并成一条语句。
pub async fn save_conquest(conn: &mut PgConnection, conquest: &Conquest) -> Result<()> {
    const SQL: &str = "INSERT INTO conquests (conquest_index, owner_guild_id, attacker_guild_id, \
                       gold_storage, npc_rate, war_is_on, war_start_time, war_end_time, \
                       start_type, game_type, schedule_start_hour, schedule_war_length, \
                       schedule_days, king_points, flag_owners, next_war_at) \
                       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, \
                               $15, $16) \
                       ON CONFLICT (conquest_index) DO UPDATE SET \
                       owner_guild_id = EXCLUDED.owner_guild_id, \
                       attacker_guild_id = EXCLUDED.attacker_guild_id, \
                       gold_storage = EXCLUDED.gold_storage, \
                       npc_rate = EXCLUDED.npc_rate, \
                       war_is_on = EXCLUDED.war_is_on, \
                       war_start_time = EXCLUDED.war_start_time, \
                       war_end_time = EXCLUDED.war_end_time, \
                       start_type = EXCLUDED.start_type, \
                       game_type = EXCLUDED.game_type, \
                       schedule_start_hour = EXCLUDED.schedule_start_hour, \
                       schedule_war_length = EXCLUDED.schedule_war_length, \
                       schedule_days = EXCLUDED.schedule_days, \
                       king_points = EXCLUDED.king_points, \
                       flag_owners = EXCLUDED.flag_owners, \
                       next_war_at = EXCLUDED.next_war_at, \
                       updated_at = now()";

    sqlx::query(SQL)
        .bind(conquest.conquest_index)
        .bind(conquest.owner_guild_id)
        .bind(conquest.attacker_guild_id)
        .bind(conquest.gold_storage)
        .bind(conquest.npc_rate)
        .bind(conquest.war_is_on)
        .bind(conquest.war_start_time)
        .bind(conquest.war_end_time)
        .bind(conquest.start_type)
        .bind(conquest.game_type)
        .bind(conquest.schedule_start_hour)
        .bind(conquest.schedule_war_length)
        .bind(conquest.schedule_days)
        .bind(conquest.king_points.as_str())
        .bind(conquest.flag_owners.as_str())
        .bind(conquest.next_war_at)
        .execute(&mut *conn)
        .await
        .map_err(StoreError::Database)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 全部受支持的别名，以及它们归一化后的规范代码。
    const ACCEPTED: &[(&str, &str)] = &[
        ("zh-cn", "zh-cn"),
        ("zh", "zh-cn"),
        ("cn", "zh-cn"),
        ("chs", "zh-cn"),
        ("zh_cn", "zh-cn"),
        ("en", "en"),
        ("en-us", "en"),
        ("en_us", "en"),
    ];

    /// 看上去像语言、但不受支持的代码。
    const REJECTED: &[&str] = &[
        "",
        "  ",
        "fr",
        "fr-fr",
        "en-gb",
        "zh-tw",
        "zh-hk",
        "ch",
        "cnn",
        "en-usss",
        "zhcn",
        "english",
        "简体中文",
        "zh cn",
    ];

    #[test]
    fn normalize_language_accepts_every_alias() {
        for (input, expected) in ACCEPTED {
            assert_eq!(
                normalize_language(input),
                Some(*expected),
                "{input} 应当归一化成 {expected}",
            );
        }
    }

    #[test]
    fn normalize_language_ignores_case_and_surrounding_space() {
        for (input, expected) in ACCEPTED {
            let variants = [
                input.to_ascii_uppercase(),
                input.to_ascii_uppercase().replace('-', "_"),
                format!("  {input}\t"),
                format!("\n{}\r\n", input.to_ascii_lowercase()),
            ];
            for variant in &variants {
                assert_eq!(
                    normalize_language(variant),
                    Some(*expected),
                    "{variant:?} 应当归一化成 {expected}",
                );
            }
        }

        assert_eq!(normalize_language("EN"), Some("en"));
        assert_eq!(normalize_language("ChS"), Some("zh-cn"));
        assert_eq!(normalize_language("En-Us"), Some("en"));
    }

    #[test]
    fn normalize_language_rejects_unsupported_codes() {
        for input in REJECTED {
            assert_eq!(normalize_language(input), None, "{input:?} 不应被接受");
        }
    }

    #[test]
    fn is_supported_language_matches_normalize_language() {
        for (input, _) in ACCEPTED {
            assert!(is_supported_language(input), "{input} 应当受支持");
        }
        for input in REJECTED {
            assert!(!is_supported_language(input), "{input:?} 不应受支持");
        }

        assert!(is_supported_language("EN"));
        assert!(!is_supported_language("fr"));
    }

    #[test]
    fn new_account_and_default_account_use_simplified_chinese() {
        assert_eq!(DEFAULT_LANGUAGE, "zh-cn");

        let new = NewAccount::new("demo", "secret");
        assert_eq!(new.language, "zh-cn");
        assert_eq!(new.language, DEFAULT_LANGUAGE);

        // 空账号的语言不是空串，避免把「未设置」当成一种语言写进库里。
        let account = Account::default();
        assert_eq!(account.language, "zh-cn");
        assert!(is_supported_language(&account.language));
    }
}
