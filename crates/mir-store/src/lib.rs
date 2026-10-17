//! 《传奇-Next》的 PostgreSQL 持久化层：账号与角色。
//!
//! 对应 Crystal 的 `Server/MirDatabase/AccountInfo.cs` 与 `CharacterInfo.cs`。
//! 本 crate 只做「数据库行 ↔ 结构体」的映射，不含游戏规则：连续错口令是否封号、
//! 单账号最多几个角色、建号 IP 限流等策略都在服务器层实现。
//!
//! # 表结构
//!
//! `migrations/0001_init.sql` 建两张表，`migrations/0002_account_language.sql` 再给账号
//! 加上界面语言列，`migrations/0003_character_magics.sql` 给角色加上技能字节块，
//! `migrations/0004_character_quests.sql` 加上任务字节块（迁移是只追加的，旧文件不再改动）：
//!
//! - `accounts`：登录名（大小写不敏感唯一）、口令散列与 24 字节盐、邮箱、注册 IP/时间、
//!   封禁（标志 + 原因 + 到期时间）、强制改密、错误口令计数、最后登录 IP/时间、GM 标志、
//!   界面语言（`language`，BCP-47 小写代码，默认 `zh-cn` = 简体中文，可在游戏内切换）。
//! - `characters`：归属账号（外键，账号删除时级联）、角色名（大小写不敏感唯一）、
//!   等级/职业/性别/发型、建号信息、封禁、登录/登出时间、软删除、位置与朝向、回城点、
//!   HP/MP/经验/金币/点数、模式与权限、行会、五个字节块（背包/装备/技能/任务/仓库，`BYTEA`）、
//!   婚姻与师徒关系。
//!
//! # 与 Crystal 的取舍
//!
//! - 口令算法完全照抄：PBKDF2-HMAC-SHA1、50 次迭代、24 字节盐、24 字节输出，
//!   并按 Crystal 的方式把结果当 UTF-8 解码成字符串（见 [`hash_password`]）。
//! - 时间统一为 `timestamptz`；Crystal 的 `DateTime.MinValue` 落成 `NULL`。
//! - 物品在 MVP 阶段是 `BYTEA` 整块，尚未拆成独立的 `user_items` 表。
//! - 金币、点数、仓库按任务要求放在角色行；Crystal 实际把它们挂在账号上，
//!   多角色共享仓库/钱包的语义要等拆表时再补。
//! - 界面语言是账号级扩展：Crystal 的语言跟着客户端设置走，本项目要求「默认全中文、
//!   游戏内可切换」，因此落库到 `accounts.language`，换角色/重登都保留同一个选择。
//!   别名（`zh`/`cn`/`chs`/`en-us` 等，大小写不敏感）由 [`normalize_language`] 归一化。
//!
//! # 示例
//!
//! ```no_run
//! # async fn demo() -> Result<(), mir_store::StoreError> {
//! let pool = mir_store::connect("postgres://postgres:secret@localhost/mir2").await?;
//! mir_store::migrate(&pool).await?;
//!
//! let account = mir_store::create_account(&pool, &mir_store::NewAccount::new("demo", "secret")).await?;
//! // 新账号默认简体中文；玩家在游戏内切换语言时调用（`EN` 会归一化成 `en`）。
//! assert_eq!(account.language, mir_store::DEFAULT_LANGUAGE);
//! mir_store::set_account_language(&pool, account.id, "en").await?;
//!
//! let character =
//!     mir_store::create_character(&pool, &mir_store::NewCharacter::new(account.id, "Hero", 0, 0))
//!         .await?;
//! mir_store::save_character(&pool, &character).await?;
//! let characters = mir_store::list_characters(&pool, account.id).await?;
//! # let _ = characters;
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]

mod error;
mod models;
mod password;
mod repo;

pub use error::{Result, StoreError};
pub use models::{
    Account, Character, CharacterSummary, DEFAULT_LANGUAGE, LoginGate, NewAccount, NewCharacter,
};
pub use password::{HASH_LEN, ITERATIONS, SALT_LEN, generate_salt, hash_password, verify_password};
pub use repo::{
    create_account, create_character, delete_character, find_account, is_supported_language,
    list_characters, load_character, mark_login, normalize_language, save_account, save_character,
    set_account_language, set_last_login_ip,
};
pub use sqlx::PgPool;

use std::time::Duration;

use sqlx::migrate::Migrator;
use sqlx::postgres::PgPoolOptions;

/// 由 `migrations/` 目录嵌入的迁移集合（编译期读取）。
pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// 连接池的默认上限。
pub const DEFAULT_MAX_CONNECTIONS: u32 = 16;

/// 每条语句的默认超时，连接建立后通过 `SET statement_timeout` 下发。
pub const DEFAULT_STATEMENT_TIMEOUT: Duration = Duration::from_secs(30);

/// 建立连接池。
///
/// 配置固定（不读任何 `MIR2_STORE_*` 环境变量）：最多 [`DEFAULT_MAX_CONNECTIONS`] 条连接、
/// 获取连接 5 秒超时、空闲 10 分钟回收、最长存活 30 分钟，并在每条连接建立后把
/// `statement_timeout` 设为 [`DEFAULT_STATEMENT_TIMEOUT`]，避免慢查询把连接池拖死。
pub async fn connect(database_url: &str) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(DEFAULT_MAX_CONNECTIONS)
        .acquire_timeout(Duration::from_secs(5))
        .idle_timeout(Duration::from_secs(600))
        .max_lifetime(Duration::from_secs(1800))
        .after_connect(|connection, _metadata| {
            Box::pin(async move {
                let sql = format!(
                    "SET statement_timeout = {}",
                    DEFAULT_STATEMENT_TIMEOUT.as_millis()
                );
                sqlx::query(&sql).execute(connection).await?;
                Ok(())
            })
        })
        .connect(database_url)
        .await?;

    tracing::debug!(
        max_connections = DEFAULT_MAX_CONNECTIONS,
        "已建立 PostgreSQL 连接池"
    );

    Ok(pool)
}

/// 执行嵌入的迁移，可重复调用（已应用的版本会跳过）。
pub async fn migrate(pool: &PgPool) -> Result<()> {
    MIGRATOR.run(pool).await?;
    let latest_version = MIGRATOR
        .iter()
        .last()
        .map_or(0, |migration| migration.version);
    tracing::debug!(version = latest_version, "数据库迁移已完成");
    Ok(())
}
