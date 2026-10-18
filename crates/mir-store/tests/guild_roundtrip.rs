//! 行会仓储的集成测试：需要一个可写的 PostgreSQL 库。
//!
//! 通过环境变量 `MIR2_TEST_DATABASE_URL` 指定库地址；未设置时打印跳过提示并直接返回，
//! 因此没有数据库的机器上 `cargo test -p mir-store` 依然会通过。
//!
//! 与 `store_roundtrip.rs` 一样，整个用例在**一个事务里完成并回滚**，不会留下任何
//! 账号/角色/行会行；迁移本身会留下 `_sqlx_migrations` 与表结构，所以建议指向专用测试库。

// `&mut *tx` 是必需的：`create_account` 之类的函数收 `E: PgExecutor`，
// 而 `&mut Transaction` 没有实现该 trait，只能先自动解引用成 `&mut PgConnection`。
// 同一文件里行会函数收的是具体的 `&mut PgConnection`，clippy 会建议省掉 `*`，故整体放行。
#![allow(clippy::explicit_auto_deref)]

use chrono::{DateTime, Utc};
use mir_store::{
    Guild, GuildBuffRow, GuildMemberRow, GuildRankRow, GuildStorageRow, NewAccount, NewCharacter,
    StoreError, create_account, create_character, create_guild, delete_guild, list_guilds,
    load_guild, load_guild_by_name, save_guild, set_character_guild,
};

#[tokio::test]
async fn guild_repository_roundtrip_rolls_back() {
    let Ok(database_url) = std::env::var("MIR2_TEST_DATABASE_URL") else {
        println!("跳过集成测试：未设置 MIR2_TEST_DATABASE_URL");
        return;
    };

    let pool = mir_store::connect(&database_url)
        .await
        .expect("应能连接 MIR2_TEST_DATABASE_URL 指向的数据库");
    mir_store::migrate(&pool).await.expect("迁移应当成功");

    let mut tx = pool.begin().await.expect("应能开启事务");

    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("系统时间应晚于 UNIX_EPOCH")
        .as_nanos();
    let account = create_account(
        &mut *tx,
        &NewAccount::new(format!("guild_{suffix}"), "secret123"),
    )
    .await
    .expect("建号");
    let leader = create_character(
        &mut *tx,
        &NewCharacter::new(account.id, format!("Leader{suffix}"), 0, 0),
    )
    .await
    .expect("建会长角色");
    let member = create_character(
        &mut *tx,
        &NewCharacter::new(account.id, format!("Member{suffix}"), 0, 0),
    )
    .await
    .expect("建成员角色");
    // 精确到微秒的固定时刻：PostgreSQL 的 `timestamptz` 只有微秒精度。
    let joined_at =
        DateTime::<Utc>::from_timestamp(1_700_000_000, 123_456_000).expect("固定时间戳合法");

    // ---- 建 ----
    let guild = Guild {
        id: 0,
        name: format!("测试行会{suffix}"),
        leader_character_id: leader.id,
        level: 0,
        experience: 0,
        gold: 0,
        spare_points: 0,
        notice: Vec::new(),
        ranks: vec![GuildRankRow {
            guild_id: 0,
            rank_index: 0,
            name: "Leader".to_owned(),
            options: 255,
        }],
        members: vec![GuildMemberRow {
            guild_id: 0,
            character_id: leader.id,
            name: leader.name.clone(),
            rank_index: 0,
            joined_at,
        }],
        // 仓库格子：`item` 列对仓储层是透明字节块（世界层用 `encode_slots` 生成）。
        storage: vec![GuildStorageRow {
            guild_id: 0,
            slot: 3,
            item: vec![1, 0, 0, 0, 1, 0xAA],
            user_id: leader.id,
            stored_at: joined_at,
        }],
        // 行会增益：按 `buff_id` 升序（读库时 ORDER BY buff_id）。
        buffs: vec![
            GuildBuffRow {
                guild_id: 0,
                buff_id: 1,
                active: true,
                active_time_remaining: 42,
            },
            GuildBuffRow {
                guild_id: 0,
                buff_id: 7,
                active: false,
                active_time_remaining: 0,
            },
        ],
        allies: Vec::new(),
    };
    let created = create_guild(&mut *tx, &guild).await.expect("建行会");
    assert!(created.id > 0, "应由数据库分配主键");
    assert_eq!(created.name, guild.name);
    assert_eq!(created.ranks.len(), 1, "会长职位应落库");
    assert_eq!(
        created.ranks[0].guild_id, created.id,
        "guild_id 应填成新主键"
    );
    assert_eq!(created.ranks[0].options, 255);
    assert_eq!(created.members.len(), 1);
    assert_eq!(created.members[0].character_id, leader.id);

    // 重名（大小写不敏感）被唯一索引拒绝。
    sqlx::query("SAVEPOINT expect_guild_unique")
        .execute(&mut *tx)
        .await
        .expect("创建保存点");
    let duplicate = create_guild(
        &mut *tx,
        &Guild {
            name: guild.name.to_uppercase(),
            ..guild.clone()
        },
    )
    .await;
    sqlx::query("ROLLBACK TO SAVEPOINT expect_guild_unique")
        .execute(&mut *tx)
        .await
        .expect("回滚到保存点");
    assert!(
        matches!(duplicate, Err(StoreError::GuildExists(_))),
        "重名行会应返回 GuildExists，实际 {duplicate:?}"
    );

    // 显式 ID 的插入路径（世界层自己分配 `Envir.NextGuildID`）。
    let explicit_id = 2_000_000 + i32::try_from(suffix % 100_000).unwrap_or(0);
    let explicit = create_guild(
        &mut *tx,
        &Guild {
            id: explicit_id,
            name: format!("显式ID行会{suffix}"),
            // 成员表主键是 `character_id`：一个角色只能在一个行会里，所以这条不带成员。
            members: Vec::new(),
            ..guild.clone()
        },
    )
    .await
    .expect("按显式 ID 建行会");
    assert_eq!(explicit.id, explicit_id, "显式 ID 应原样落库");

    // ---- 读 ----
    let loaded = load_guild(&mut *tx, created.id).await.expect("读行会");
    assert_eq!(loaded, created);
    assert_eq!(loaded.storage.len(), 1, "仓库格子应落库");
    assert_eq!(loaded.storage[0].slot, 3);
    assert_eq!(loaded.storage[0].user_id, leader.id);
    assert_eq!(loaded.storage[0].item, vec![1, 0, 0, 0, 1, 0xAA]);
    assert_eq!(loaded.buffs.len(), 2, "行会增益应落库");
    assert!(loaded.buffs[0].active, "激活状态应原样往返");
    assert_eq!(loaded.buffs[0].active_time_remaining, 42);
    // `Envir.GetGuild` 的比较语义：去空格 + 忽略大小写（`Envir.cs:4836`）。
    let by_name = load_guild_by_name(&mut *tx, &format!(" {}", guild.name.to_uppercase()))
        .await
        .expect("按名字读");
    assert_eq!(by_name.map(|row| row.id), Some(created.id));
    assert!(
        load_guild_by_name(&mut *tx, "不存在的行会")
            .await
            .expect("查询成功")
            .is_none()
    );

    // ---- 整份回写：改名、加职位、加成员、写公告 ----
    let mut updated = loaded;
    updated.name = format!("改名行会{suffix}");
    updated.notice = vec!["第一行".to_owned(), "第二行".to_owned()];
    updated.level = 3;
    updated.experience = 1234;
    updated.gold = 99_999;
    updated.spare_points = 2;
    updated.ranks.push(GuildRankRow {
        guild_id: updated.id,
        rank_index: 1,
        name: "Members".to_owned(),
        options: 0,
    });
    updated.members.push(GuildMemberRow {
        guild_id: updated.id,
        character_id: member.id,
        name: member.name.clone(),
        rank_index: 1,
        joined_at,
    });
    // 仓库与增益也是整份替换：新增一格、把第一条增益改成未激活。
    updated.storage.push(GuildStorageRow {
        guild_id: updated.id,
        slot: 9,
        item: vec![1, 0, 0, 0, 0],
        user_id: member.id,
        stored_at: joined_at,
    });
    updated.buffs[0].active = false;
    save_guild(&mut *tx, &updated).await.expect("回写行会");

    let reloaded = load_guild(&mut *tx, updated.id).await.expect("重读");
    assert_eq!(reloaded, updated);
    assert_eq!(reloaded.storage.len(), 2, "新增的仓库格子应写入");
    assert_eq!(reloaded.storage[1].slot, 9);
    assert!(!reloaded.buffs[0].active, "增益的激活状态应被回写");
    assert_eq!(
        reloaded.notice,
        vec!["第一行".to_owned(), "第二行".to_owned()]
    );

    // 全部行会（启动时灌进世界）。
    let all = list_guilds(&mut *tx).await.expect("列出全部行会");
    assert!(all.iter().any(|row| row.id == updated.id));
    assert!(all.iter().any(|row| row.id == explicit.id));

    // ---- 角色的 guild_index ----
    set_character_guild(&mut *tx, leader.id, updated.id)
        .await
        .expect("写角色的行会归属");
    let leader_row = mir_store::load_character(&mut *tx, leader.id)
        .await
        .expect("读角色");
    assert_eq!(leader_row.guild_index, updated.id);
    set_character_guild(&mut *tx, leader.id, -1)
        .await
        .expect("清空行会归属");
    let cleared = mir_store::load_character(&mut *tx, leader.id)
        .await
        .expect("读角色");
    assert_eq!(cleared.guild_index, -1);
    assert!(
        matches!(
            set_character_guild(&mut *tx, -1, updated.id).await,
            Err(StoreError::NotFound(_))
        ),
        "不存在的角色应返回 NotFound"
    );

    // ---- 删 ----
    delete_guild(&mut *tx, created.id).await.expect("删行会");
    assert!(
        matches!(
            load_guild(&mut *tx, created.id).await,
            Err(StoreError::NotFound(_))
        ),
        "删除后读不到"
    );
    assert!(
        matches!(
            delete_guild(&mut *tx, created.id).await,
            Err(StoreError::NotFound(_))
        ),
        "重复删除返回 NotFound"
    );

    // 回滚：整段事务不留任何行。
    tx.rollback().await.expect("回滚应当成功");
}
