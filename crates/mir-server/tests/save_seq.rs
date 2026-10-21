//! B：`db_save ok` 序号/时间戳回归（并发 + 模拟重启）。
//!
//! 本用例需要 `MIR2_TEST_DATABASE_URL`（真 PG）；未设置时 **fail-loud 而非跳过**。
//! `ci.ps1` 会自动设置它，手动运行请先导出该环境变量。
//!
//! 序号是进程级全局 `AtomicU64`，只在写库成功后分配；这里用产品记录 API
//! `db_save_records()` 做证据，避免测试自己维护一套计数器。

use std::sync::Arc;

use mir_core::enums::{MirClass, MirGender};
use mir_server::PgAccountStore;
use mir_server::pg_store::{db_save_records, reset_db_save_seq};
use mir_server::store::AccountStore;

static SEQ_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn name(prefix: &str) -> String {
    // 唯一性：纳秒全量 + 进程内自增，避免「6 位取模」在历史数据累积后撞名
    // （曾出现 `建角: CharacterExists("序号376005")` 的门禁假红）。
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("时钟")
        .as_nanos();
    format!("{prefix}{suffix}{counter}")
}

/// 建角：撞名（历史测试数据残留）时换一个后缀重试，而不是让门禁随机红。
async fn create_character_retry(
    store: &PgAccountStore,
    account_id: i64,
    prefix: &str,
    index: usize,
    point: (i32, i32),
) -> mir_server::store::Character {
    for attempt in 0..8 {
        let character_name = format!("{}{index}{attempt}", name(prefix));
        match store
            .create_character(
                account_id,
                &character_name,
                MirClass::Warrior,
                MirGender::Male,
                "0",
                point,
            )
            .await
        {
            Ok(character) => return character,
            Err(mir_server::store::StoreError::CharacterExists(_)) => {}
            Err(error) => panic!("建角失败：{error}"),
        }
    }
    panic!("建角重试 8 次仍然撞名");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_saves_have_unique_monotonic_seq_and_time() {
    let Ok(url) = std::env::var("MIR2_TEST_DATABASE_URL") else {
        panic!("MIR2_TEST_DATABASE_URL 未设置；fail-loud")
    };
    let _guard = SEQ_LOCK.lock().await;
    reset_db_save_seq();
    let base = db_save_records().len();
    let store = Arc::new(PgAccountStore::connect(&url).await.expect("连接测试库"));
    let account = store
        .create_account(&name("seqacc"), "secret", "")
        .await
        .expect("建号");
    let mut characters = Vec::new();
    for index in 0..8 {
        let offset = i32::try_from(index).expect("index fits i32");
        characters.push(
            create_character_retry(&store, account.id, "idx", index, (10 + offset, 10)).await,
        );
    }
    let mut tasks = Vec::new();
    for mut character in characters {
        character.gold = 100;
        let store = Arc::clone(&store);
        tasks.push(tokio::spawn(async move {
            store.save_character(character).await
        }));
    }
    for task in tasks {
        task.await.expect("join").expect("并发保存");
    }
    let records = db_save_records();
    let recent = &records[base..];
    assert_eq!(recent.len(), 8, "应记录 8 次成功保存");
    let mut seqs: Vec<u64> = recent.iter().map(|(seq, _, _)| *seq).collect();
    seqs.sort_unstable();
    assert_eq!(seqs, (1..=8).collect::<Vec<_>>(), "seq 必须唯一且从 1 连续");
    let mut by_seq = recent.to_vec();
    by_seq.sort_by_key(|(seq, _, _)| *seq);
    for pair in by_seq.windows(2) {
        assert!(
            pair[0].1 <= pair[1].1,
            "按 seq 排序的 t 必须非降：{by_seq:?}"
        );
    }
    assert!(
        recent
            .iter()
            .all(|(_, t, character)| *t > 0 && *character > 0)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn seq_resets_to_one_after_simulated_restart() {
    let Ok(url) = std::env::var("MIR2_TEST_DATABASE_URL") else {
        panic!("MIR2_TEST_DATABASE_URL 未设置；fail-loud")
    };
    let _guard = SEQ_LOCK.lock().await;
    let store = Arc::new(PgAccountStore::connect(&url).await.expect("连接测试库"));
    let account = store
        .create_account(&name("seqrst"), "secret", "")
        .await
        .expect("建号");
    let character = create_character_retry(&store, account.id, "rst", 0, (20, 20)).await;
    store
        .save_character(character.clone())
        .await
        .expect("保存一次");
    reset_db_save_seq();
    let base = db_save_records().len();
    store.save_character(character).await.expect("重启后再保存");
    let records = db_save_records();
    let recent = &records[base..];
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].0, 1, "模拟重启后 seq 必须从 1 开始");
    assert!(recent[0].1 > 0);
}
