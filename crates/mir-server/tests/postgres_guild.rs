//! 行会持久化的端到端测试（真实 PostgreSQL）。
//!
//! 没有设置 `MIR2_TEST_DATABASE_URL` 时直接跳过（与 `postgres_login.rs` 一致），
//! 以免把开发机的库当成测试库。
//!
//! 覆盖 [`PgAccountStore`] 的四件套（`save_guild` / `delete_guild` / `list_guilds` /
//! `set_character_guild`）与 `Character.guild_index` 的整行往返——也就是
//! 「世界层 → [`GuildWrite`] → 存储层」这条链路里存储侧的那一半。

use std::sync::Arc;

use mir_core::enums::{MirClass, MirGender};
use mir_server::{AccountStore, GuildMemberRecord, GuildRankRecord, GuildRecord, PgAccountStore};

#[tokio::test]
async fn postgres_guild_persistence_roundtrip() {
    let Ok(url) = std::env::var("MIR2_TEST_DATABASE_URL") else {
        println!("跳过：未设置 MIR2_TEST_DATABASE_URL（需要一个可写的 PostgreSQL 测试库）");
        return;
    };
    let store = Arc::new(
        PgAccountStore::connect(&url)
            .await
            .expect("连接测试库并迁移"),
    );

    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let username = format!("g{}", suffix % 1_000_000_000);
    let account = store
        .create_account(&username, "secret", "")
        .await
        .expect("建号");
    let character = store
        .create_character(
            account.id,
            &format!("Gd{suffix}"),
            MirClass::Warrior,
            MirGender::Male,
            "0",
            (10, 10),
        )
        .await
        .expect("建角");
    assert_eq!(character.guild_index, -1, "新角色没有行会");

    // ---- 建行会（id = 0 → 数据库分配） ----
    let guild = GuildRecord {
        id: 0,
        name: format!("PG行会{suffix}"),
        leader_character_id: character.id,
        level: 0,
        experience: 0,
        gold: 0,
        spare_points: 0,
        notice: Vec::new(),
        ranks: vec![GuildRankRecord {
            index: 0,
            name: "Leader".to_owned(),
            options: 255,
        }],
        members: vec![GuildMemberRecord {
            character_id: character.id,
            name: character.name.clone(),
            rank_index: 0,
            joined_at: suffix_to_millis(suffix),
        }],
    };
    let guild_id = store.save_guild(guild.clone()).await.expect("写行会");
    assert!(guild_id > 0, "应由数据库分配主键");

    // ---- 读回来 ----
    let guilds = store.list_guilds().await.expect("列出行会");
    let stored = guilds
        .iter()
        .find(|row| row.id == guild_id)
        .expect("刚写的行会应能读出来");
    assert_eq!(stored.name, guild.name);
    assert_eq!(stored.ranks.len(), 1);
    assert_eq!(stored.ranks[0].options, 255);
    assert_eq!(stored.members.len(), 1);
    assert_eq!(stored.members[0].character_id, character.id);

    // ---- 角色的 guild_index ----
    store
        .set_character_guild(character.id, guild_id)
        .await
        .expect("写角色归属");
    let reloaded = store
        .find_character(account.id, character.id)
        .await
        .expect("查角色")
        .expect("角色存在");
    assert_eq!(reloaded.guild_index, guild_id, "guild_index 应落库");

    // 整行存档也带上 guild_index（重新登录后据此找回行会）。
    store.save_character(reloaded).await.expect("存档");
    let after_save = store
        .find_character(account.id, character.id)
        .await
        .expect("查角色")
        .expect("角色存在");
    assert_eq!(after_save.guild_index, guild_id);

    // ---- 回写（改等级/经验/公告） ----
    let mut updated = stored.clone();
    updated.level = 2;
    updated.experience = 345;
    updated.notice = vec!["公告一".to_owned(), "公告二".to_owned()];
    store.save_guild(updated.clone()).await.expect("回写行会");
    let guilds = store.list_guilds().await.expect("列出行会");
    let stored = guilds
        .iter()
        .find(|row| row.id == guild_id)
        .expect("行会还在");
    assert_eq!(stored.level, 2);
    assert_eq!(stored.experience, 345);
    assert_eq!(
        stored.notice,
        vec!["公告一".to_owned(), "公告二".to_owned()]
    );

    // ---- 删 ----
    store.delete_guild(guild_id).await.expect("删行会");
    let guilds = store.list_guilds().await.expect("列出行会");
    assert!(
        !guilds.iter().any(|row| row.id == guild_id),
        "删除后不应再列出"
    );
    // 行会删除后把角色归属清掉，模拟「世界层解散行会」的收尾写入。
    store
        .set_character_guild(character.id, -1)
        .await
        .expect("清空归属");
    let cleared = store
        .find_character(account.id, character.id)
        .await
        .expect("查角色")
        .expect("角色存在");
    assert_eq!(cleared.guild_index, -1);

    // 清理：软删除测试角色（账号行保留，与 `postgres_login.rs` 的做法一致）。
    let _ = store.delete_character(account.id, character.id).await;
}

/// 纳秒时间戳 → 毫秒（`joined_at` 用毫秒）。
fn suffix_to_millis(suffix: u128) -> i64 {
    i64::try_from((suffix / 1_000_000) % i64::MAX as u128).unwrap_or(0)
}
