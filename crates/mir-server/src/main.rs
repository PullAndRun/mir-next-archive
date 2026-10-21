//! 《传奇-Next》服务端入口。

use std::sync::Arc;

use anyhow::Result;
use mir_server::{MemoryStore, ServerConfig, run};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = Arc::new(ServerConfig::from_env());
    tracing::info!(?config.listen, "启动传奇-Next 服务端");
    tracing::info!(
        asset_root = %config.asset_root,
        source = config.asset_root_source.as_str(),
        map_root = %config.map_root,
        db_path = config.db_path.as_deref().unwrap_or("<未配置>"),
        "资源根解析结果"
    );

    // 存储层：设置了 `DATABASE_URL`（或 `MIR2_DATABASE_URL`）时用 PostgreSQL，
    // 否则退回内存实现，保证没有数据库也能本地试玩。
    let database_url = std::env::var("MIR2_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .ok();
    if let Some(url) = database_url {
        tracing::info!("使用 PostgreSQL 存储");
        let store = Arc::new(mir_server::PgAccountStore::connect(&url).await?);
        run(config, store).await
    } else {
        tracing::warn!("未配置数据库，使用内存存储（重启即丢失）");
        let store = Arc::new(MemoryStore::new());
        run(config, store).await
    }
}
