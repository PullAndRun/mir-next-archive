//! 集成测试：需要一个可写的 PostgreSQL 库。
//!
//! 通过环境变量 `MIR2_TEST_DATABASE_URL` 指定库地址；未设置时打印跳过提示并直接返回，
//! 因此没有数据库的机器上 `cargo test -p mir-store` 依然会通过。
//!
//! 数据操作全部在同一个事务里完成并回滚，不会留下任何账号或角色行；
//! 迁移本身会留下 `_sqlx_migrations` 与两张表，所以建议指向专用的测试库。
//!
//! 预期会失败的写操作（唯一索引冲突、外键缺失）包在 `SAVEPOINT` 里：PostgreSQL 中
//! 任何语句报错都会把事务标记为 aborted，必须回滚到保存点才能继续使用同一个事务。

use chrono::{DateTime, Utc};
use mir_store::{
    LoginGate, NewAccount, NewCharacter, SALT_LEN, StoreError, create_account, create_character,
    delete_character, find_account, list_characters, load_character, mark_login, save_account,
    save_character, set_last_login_ip, verify_password,
};

#[tokio::test]
async fn account_and_character_lifecycle_rolls_back() {
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
    let username = format!("it_{suffix}");
    let character_name = format!("It{suffix}");
    // 固定时刻且精确到微秒：PostgreSQL 的 timestamptz 只有微秒精度，用 now() 会破坏相等比较。
    let fixed_time =
        DateTime::<Utc>::from_timestamp(1_700_000_000, 123_456_000).expect("固定时间戳应当合法");

    // ---- 账号 ----
    let account = create_account(&mut *tx, &NewAccount::new(username.clone(), "secret123"))
        .await
        .expect("创建账号应当成功");
    assert!(account.id > 0, "账号应当拿到自增主键");
    assert_eq!(account.username, username);
    assert_eq!(account.salt.len(), SALT_LEN);
    assert!(account.creation_ip.is_empty());
    assert!(verify_password(&account, "secret123"));
    assert!(!verify_password(&account, "secret124"));
    assert_eq!(account.login_gate(Utc::now()), LoginGate::Allowed);

    // 登录名大小写不敏感（Crystal `AccountExists` 用 OrdinalIgnoreCase）。
    let found = find_account(&mut *tx, &username.to_uppercase())
        .await
        .expect("查询账号应当成功")
        .expect("账号应当存在");
    assert_eq!(found.id, account.id);

    // 重名账号被唯一索引拒绝。
    sqlx::query("SAVEPOINT expect_unique")
        .execute(&mut *tx)
        .await
        .expect("创建保存点应当成功");
    let duplicate = create_account(
        &mut *tx,
        &NewAccount::new(username.to_uppercase(), "secret123"),
    )
    .await;
    sqlx::query("ROLLBACK TO SAVEPOINT expect_unique")
        .execute(&mut *tx)
        .await
        .expect("回滚到保存点应当成功");
    assert!(
        matches!(duplicate, Err(StoreError::AccountExists(_))),
        "重名账号应当返回 AccountExists，实际为 {duplicate:?}"
    );

    // ---- 角色 ----
    let mut character = create_character(
        &mut *tx,
        &NewCharacter::new(account.id, character_name.clone(), 0, 0),
    )
    .await
    .expect("创建角色应当成功");
    assert_eq!(character.account_id, account.id);
    assert_eq!(character.level, 1);
    assert!(!character.deleted);
    assert!(character.inventory.is_empty() && character.equipment.is_empty());
    assert_eq!(character.guild_index, -1);

    // 角色名全局唯一（大小写不敏感）。
    sqlx::query("SAVEPOINT expect_unique")
        .execute(&mut *tx)
        .await
        .expect("创建保存点应当成功");
    let duplicate_name = create_character(
        &mut *tx,
        &NewCharacter::new(account.id, character_name.to_uppercase(), 1, 1),
    )
    .await;
    sqlx::query("ROLLBACK TO SAVEPOINT expect_unique")
        .execute(&mut *tx)
        .await
        .expect("回滚到保存点应当成功");
    assert!(
        matches!(duplicate_name, Err(StoreError::CharacterNameTaken(_))),
        "重名角色应当返回 CharacterNameTaken，实际为 {duplicate_name:?}"
    );

    // 账号不存在时外键报错被翻译成 NotFound。
    sqlx::query("SAVEPOINT expect_fk")
        .execute(&mut *tx)
        .await
        .expect("创建保存点应当成功");
    let orphan = create_character(&mut *tx, &NewCharacter::new(-1, "OrphanHero", 0, 0)).await;
    sqlx::query("ROLLBACK TO SAVEPOINT expect_fk")
        .execute(&mut *tx)
        .await
        .expect("回滚到保存点应当成功");
    assert!(
        matches!(orphan, Err(StoreError::NotFound(_))),
        "不存在的账号应当返回 NotFound，实际为 {orphan:?}"
    );

    // 选角列表只包含未删除角色。
    let summaries = list_characters(&mut *tx, account.id)
        .await
        .expect("选角列表应当可读");
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].name, character_name);
    assert_eq!(summaries[0].level, 1);
    assert_eq!(summaries[0].last_logout_date, None);

    // 读回来的行与创建结果完全一致。
    let loaded = load_character(&mut *tx, character.id)
        .await
        .expect("角色应当可以读出");
    assert_eq!(loaded, character);

    // ---- 存档回写 ----
    character.level = 12;
    character.hp = 33;
    character.mp = 7;
    character.experience = 1_234_567;
    character.gold = 9_999;
    character.credit = 8;
    character.current_map_index = 3;
    character.current_x = 100;
    character.current_y = 200;
    character.direction = 4;
    character.bind_map_index = 1;
    character.pk_points = 2;
    character.allow_group = true;
    character.guild_index = 42;
    character.inventory = vec![1, 2, 3];
    character.equipment = vec![4, 5];
    character.storage = vec![6];
    character.last_login_date = Some(fixed_time);
    character.last_logout_date = Some(fixed_time);
    save_character(&mut *tx, &character)
        .await
        .expect("存档应当成功");

    let reloaded = load_character(&mut *tx, character.id)
        .await
        .expect("存档后应当可以读出");
    assert_eq!(reloaded, character);

    // 不存在的角色回写返回 NotFound（UPDATE 0 行不是数据库错误，不需要保存点）。
    let missing_save = {
        let mut missing = character.clone();
        missing.id = -1;
        save_character(&mut *tx, &missing).await
    };
    assert!(
        matches!(missing_save, Err(StoreError::NotFound(_))),
        "回写不存在的角色应当返回 NotFound，实际为 {missing_save:?}"
    );

    // ---- 登录记录 ----
    mark_login(&mut *tx, account.id)
        .await
        .expect("记录登录应当成功");
    set_last_login_ip(&mut *tx, account.id, "203.0.113.7")
        .await
        .expect("记录登录 IP 应当成功");
    let refreshed = find_account(&mut *tx, &username)
        .await
        .expect("查询账号应当成功")
        .expect("账号应当存在");
    assert!(refreshed.last_date.is_some(), "登录后应写入最后登录时间");
    assert_eq!(refreshed.last_ip, "203.0.113.7");
    assert_eq!(refreshed.wrong_password_count, 0);
    assert_eq!(refreshed.login_gate(Utc::now()), LoginGate::Allowed);

    let missing_account = mark_login(&mut *tx, -1).await;
    assert!(
        matches!(missing_account, Err(StoreError::NotFound(_))),
        "不存在的账号应当返回 NotFound，实际为 {missing_account:?}"
    );

    // ---- 账号状态回写：改密、封禁、强制改密 ----
    let mut managed = refreshed.clone();
    managed.set_password("changed123");
    managed.banned = true;
    managed.ban_reason = "测试封禁".to_owned();
    // 远未来的到期时间 → 处于封禁期。
    managed.expiry_date = DateTime::<Utc>::from_timestamp(4_000_000_000, 0);
    managed.require_password_change = true;
    save_account(&mut *tx, &managed)
        .await
        .expect("回写账号状态应当成功");

    let banned = find_account(&mut *tx, &username)
        .await
        .expect("查询账号应当成功")
        .expect("账号应当存在");
    assert!(verify_password(&banned, "changed123"));
    assert!(!verify_password(&banned, "secret123"));
    assert!(banned.is_banned(Utc::now()), "到期时间未到应当算封禁");
    assert_eq!(banned.login_gate(Utc::now()), LoginGate::Banned);

    // 到期时间已过 → 封禁失效，但仍要求改密。
    managed.expiry_date = Some(fixed_time);
    save_account(&mut *tx, &managed)
        .await
        .expect("回写到期时间应当成功");
    let expired = find_account(&mut *tx, &username)
        .await
        .expect("查询账号应当成功")
        .expect("账号应当存在");
    assert!(!expired.is_banned(Utc::now()), "到期后封禁不应再生效");
    assert_eq!(
        expired.login_gate(Utc::now()),
        LoginGate::PasswordChangeRequired
    );

    // 取消封禁与强制改密 → 允许登录。
    managed.banned = false;
    managed.expiry_date = None;
    managed.require_password_change = false;
    save_account(&mut *tx, &managed)
        .await
        .expect("解除封禁应当成功");
    let cleared = find_account(&mut *tx, &username)
        .await
        .expect("查询账号应当成功")
        .expect("账号应当存在");
    assert_eq!(cleared.login_gate(Utc::now()), LoginGate::Allowed);

    // 不存在的账号回写返回 NotFound。
    let missing_account_save = {
        let mut missing = managed.clone();
        missing.id = -1;
        save_account(&mut *tx, &missing).await
    };
    assert!(
        matches!(missing_account_save, Err(StoreError::NotFound(_))),
        "回写不存在的账号应当返回 NotFound，实际为 {missing_account_save:?}"
    );

    // ---- 删除 ----
    let wrong_owner = delete_character(&mut *tx, character.id, account.id + 1_000_000).await;
    assert!(
        matches!(wrong_owner, Err(StoreError::NotFound(_))),
        "账号不匹配时不允许删除，实际为 {wrong_owner:?}"
    );

    delete_character(&mut *tx, character.id, account.id)
        .await
        .expect("软删除应当成功");
    assert!(
        matches!(
            load_character(&mut *tx, character.id).await,
            Err(StoreError::NotFound(_))
        ),
        "软删除后不应再读出角色"
    );
    assert!(
        list_characters(&mut *tx, account.id)
            .await
            .expect("选角列表应当可读")
            .is_empty(),
        "软删除后选角列表应为空"
    );
    assert!(
        matches!(
            delete_character(&mut *tx, character.id, account.id).await,
            Err(StoreError::NotFound(_))
        ),
        "重复删除应当返回 NotFound"
    );

    // ---- 回滚：整个过程不留任何行 ----
    tx.rollback().await.expect("回滚应当成功");
    assert!(
        find_account(&pool, &username)
            .await
            .expect("回滚后查询账号应当成功")
            .is_none(),
        "回滚后账号不应存在于数据库中"
    );
}
