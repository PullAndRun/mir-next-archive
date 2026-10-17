//! 存储层错误类型。

use thiserror::Error;

/// 存储层统一错误。
///
/// 所有数据库交互都返回 [`Result`]；库代码不会 panic，唯一索引冲突、外键缺失这类
/// 「预期内」的失败会被翻译成有语义的变体，其余原样包在 [`StoreError::Database`] 里。
#[derive(Debug, Error)]
pub enum StoreError {
    /// 账号名已存在（`accounts_username_ci_key` 冲突）。
    #[error("账号名 {0} 已存在")]
    AccountExists(String),

    /// 角色名已被占用（`characters_name_ci_key` 冲突）。
    #[error("角色名 {0} 已被占用")]
    CharacterNameTaken(String),

    /// 目标记录不存在，或已被标记删除；参数是实体名（例如「账号」「角色」）。
    #[error("找不到{0}：记录不存在或已被标记删除")]
    NotFound(&'static str),

    /// 底层数据库错误。
    #[error("数据库错误：{0}")]
    Database(#[from] sqlx::Error),

    /// 迁移执行失败。
    #[error("数据库迁移失败：{0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
}

/// 存储层结果别名。
pub type Result<T> = std::result::Result<T, StoreError>;

impl StoreError {
    /// 翻译 `INSERT` 的两类预期失败：唯一索引冲突 → `on_unique`，
    /// 外键缺失 → `NotFound(missing_entity)`；其他数据库错误保持原样。
    ///
    /// 目前数据库里只有 `characters.account_id` 一条外键，因此 23503 只可能表示
    /// 建角色时账号不存在。
    pub(crate) fn map_insert(
        error: sqlx::Error,
        on_unique: Self,
        missing_entity: &'static str,
    ) -> Self {
        if error
            .as_database_error()
            .is_some_and(sqlx::error::DatabaseError::is_unique_violation)
        {
            return on_unique;
        }

        if error
            .as_database_error()
            .is_some_and(sqlx::error::DatabaseError::is_foreign_key_violation)
        {
            return Self::NotFound(missing_entity);
        }

        Self::Database(error)
    }
}
