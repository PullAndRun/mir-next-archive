use mir_server::PgAccountStore;
use mir_server::store::AccountStore;
use sqlx::Row;
use std::sync::Arc;

static METRICS_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn tagged_database_url(url: &str) -> String {
    let separator = if url.contains('?') { '&' } else { '?' };
    format!("{url}{separator}application_name=mir2_fault_test")
}

#[tokio::test]
#[ignore = "需要 pg_proxy.py/python 与真实 PG；手工跑: cargo test -p mir-server --test fault_pg -- --ignored --test-threads=1"]
async fn pg_terminate_disconnects_save_and_next_save_recovers() {
    let Ok(url) = std::env::var("MIR2_TEST_DATABASE_URL") else {
        panic!("MIR2_TEST_DATABASE_URL 未设置；本用例 fail-loud，不允许 skip");
    };
    let _guard = METRICS_LOCK.lock().await;
    let url = tagged_database_url(&url);
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("时钟")
        .as_nanos();
    let account_name = format!("fault-pg-{suffix}");
    let character_name = format!("断连角色{suffix}");
    let store = Arc::new(PgAccountStore::connect(&url).await.expect("连接测试库"));
    let account = store
        .create_account(&account_name, "secret", "")
        .await
        .expect("建号");
    let character = store
        .create_character(
            account.id,
            &character_name,
            mir_core::enums::MirClass::Warrior,
            mir_core::enums::MirGender::Male,
            "0",
            (10, 10),
        )
        .await
        .expect("建角");
    store
        .save_character(character.clone())
        .await
        .expect("首次成功");
    let mut admin = store.pool().acquire().await.expect("管理连接");
    let pids: Vec<i32> = sqlx::query(
        "SELECT pid FROM pg_stat_activity \
         WHERE datname = current_database() \
           AND application_name = 'mir2_fault_test' \
           AND pid <> pg_backend_pid()",
    )
    .fetch_all(&mut *admin)
    .await
    .expect("查测试连接")
    .into_iter()
    .map(|row| row.get("pid"))
    .collect();
    for pid in &pids {
        sqlx::query("SELECT pg_terminate_backend($1)")
            .bind(pid)
            .execute(&mut *admin)
            .await
            .expect("定向 terminate");
    }
    drop(admin);
    println!("terminated PIDs: {pids:?}（count={}）", pids.len());
    assert!(!pids.is_empty(), "至少应 terminate 本用例的连接");
    // sqlx 在 acquire 时可能自动重连；再关闭本用例的池，确保第一笔 save 真的失败。
    store.pool().close().await;
    let character_id = character.id;
    let mut dirty = character;
    dirty.gold = 777;
    let failed = store.save_character(dirty.clone()).await;
    assert!(
        failed.is_err(),
        "terminate 后下一次 save 必须失败，实际 {failed:?}"
    );
    // 关闭池后该 store 不可复用；新 store 验证恢复路径（terminate 已触发第一笔失败）。
    let store = Arc::new(PgAccountStore::connect(&url).await.expect("重连测试库"));
    let mut retry = dirty;
    retry.gold = 778;
    store
        .save_character(retry.clone())
        .await
        .expect("下一笔 save 自动恢复");
    let loaded = store
        .find_character(account.id, character_id)
        .await
        .expect("读库")
        .expect("存在");
    assert_eq!(loaded.gold, 778);
}
use mir_protocol::codec::FrameBuffer;
use mir_protocol::packets::client as c;

use mir_protocol::{ClientPacket, ServerPacket};
use mir_server::metrics::metrics;
use mir_server::{ServerConfig, serve_forever};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

struct Client {
    stream: TcpStream,
    frames: FrameBuffer,
}
impl Client {
    async fn connect(addr: std::net::SocketAddr) -> Self {
        let stream = TcpStream::connect(addr).await.expect("连接");
        stream.set_nodelay(true).ok();
        Self {
            frames: FrameBuffer::new(),
            stream,
        }
    }
    async fn send(&mut self, packet: ClientPacket) {
        self.stream
            .write_all(&packet.encode_frame())
            .await
            .expect("发送");
    }

    async fn still_alive(&mut self) {
        let mut chunk = [0u8; 256];
        match tokio::time::timeout(
            std::time::Duration::from_millis(500),
            self.stream.read(&mut chunk),
        )
        .await
        {
            Ok(Ok(0)) => panic!("客户端连接被服务端关闭"),
            Ok(Ok(_)) | Err(_) => {}
            Ok(Err(error)) => panic!("客户端连接读取失败：{error}"),
        }
    }
    async fn expect(&mut self) -> ServerPacket {
        for _ in 0..80 {
            if let Some((i, p)) = self.frames.next_frame().expect("帧") {
                let packet = ServerPacket::decode(i, &p).expect("解码");

                return packet;
            }
            let mut chunk = [0u8; 4096];
            let n = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                self.stream.read(&mut chunk),
            )
            .await
            .expect("超时")
            .expect("读");
            assert!(n > 0, "提前关闭");
            self.frames.push(&chunk[..n]);
        }
        panic!("等待服务端包超时");
    }
}

async fn login(addr: std::net::SocketAddr, user: &str) -> Client {
    let mut client = Client::connect(addr).await;

    let _ = client.expect().await;
    client
        .send(ClientPacket::ClientVersion(c::ClientVersion {
            version_hash: vec![0u8; 16],
        }))
        .await;

    let _ = client.expect().await;
    client
        .send(ClientPacket::NewAccount(c::NewAccount {
            account_id: user.to_owned(),
            password: "secret".to_owned(),
            birth_date: mir_core::DateTimeBinary(0),
            user_name: String::new(),
            secret_question: String::new(),
            secret_answer: String::new(),
            e_mail_address: String::new(),
        }))
        .await;

    let _ = client.expect().await;
    client
        .send(ClientPacket::Login(c::Login {
            account_id: user.to_owned(),
            password: "secret".to_owned(),
        }))
        .await;

    let _ = client.expect().await;
    client
        .send(ClientPacket::NewCharacter(c::NewCharacter {
            name: format!("{user}侠"),
            gender: mir_core::enums::MirGender::Male,
            class: mir_core::enums::MirClass::Warrior,
        }))
        .await;
    let packet = client.expect().await;
    let ServerPacket::NewCharacterSuccess(success) = packet else {
        panic!("建角失败")
    };
    client
        .send(ClientPacket::StartGame(c::StartGame {
            character_index: success.char_info.index,
        }))
        .await;
    for _ in 0..20 {
        let packet = client.expect().await;
        if matches!(packet, ServerPacket::UserInformation(_)) {
            return client;
        }
    }
    panic!("未收到 UserInformation");
}

async fn wait_metric<F: Fn(u64) -> bool>(base: u64, f: F, label: &str) -> u64 {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(8);
    loop {
        let value = if label == "saves" {
            metrics().db_saves()
        } else {
            metrics().db_save_failed()
        };
        if value >= base && f(value - base) {
            return value;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "等待 {label} 超时：base={base} current={value}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

async fn start_pg_server(
    url: &str,
) -> (
    std::net::SocketAddr,
    Arc<PgAccountStore>,
    tokio::task::JoinHandle<Result<(), anyhow::Error>>,
) {
    let url = tagged_database_url(url);
    let store = Arc::new(PgAccountStore::connect(&url).await.expect("PG"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("绑定");
    let addr = listener.local_addr().expect("addr");
    let config = ServerConfig {
        map_root: "resource/不存在的目录".to_owned(),
        load_game_data: false,
        autosave_secs: 1,
        idle_timeout_secs: 0,
        max_connections_per_ip: 0,
        max_connections_per_second: 0,
        max_packets_per_second: 0,
        ..ServerConfig::default()
    };
    let handle = tokio::spawn(serve_forever(
        listener,
        Arc::new(config),
        Arc::clone(&store) as Arc<_>,
    ));
    (addr, store, handle)
}

#[tokio::test]
#[ignore = "需要 pg_proxy.py/python 与真实 PG；手工跑: cargo test -p mir-server --test fault_pg -- --ignored --test-threads=1"]
async fn transient_terminate_is_transparent_and_metrics_show_success_not_failure() {
    let Ok(url) = std::env::var("MIR2_TEST_DATABASE_URL") else {
        panic!("MIR2_TEST_DATABASE_URL 未设置；fail-loud")
    };
    let _guard = METRICS_LOCK.lock().await;
    let (addr, store, handle) = start_pg_server(&url).await;
    let user = format!(
        "fa{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("时钟")
            .as_nanos()
    );
    let mut client = login(addr, &user).await;
    let base_saves = metrics().db_saves();
    let base_failed = metrics().db_save_failed();
    let mut admin = store.pool().acquire().await.expect("管理连接");
    let pids: Vec<i32> = sqlx::query("SELECT pid FROM pg_stat_activity WHERE datname = current_database() AND application_name = 'mir2_fault_test' AND pid <> pg_backend_pid()").fetch_all(&mut *admin).await.expect("查测试连接").into_iter().map(|row| row.get("pid")).collect();
    for pid in &pids {
        sqlx::query("SELECT pg_terminate_backend($1)")
            .bind(pid)
            .execute(&mut *admin)
            .await
            .expect("terminate");
    }
    drop(admin);
    println!("terminated PIDs: {pids:?}（count={}）", pids.len());
    client
        .send(ClientPacket::Walk(c::Walk {
            direction: mir_core::enums::MirDirection::Right,
        }))
        .await;
    let saves = wait_metric(base_saves, |d| d >= 1, "saves").await;
    assert_eq!(
        metrics().db_save_failed(),
        base_failed,
        "瞬时 terminate 不应记失败"
    );
    client.still_alive().await;
    handle.abort();
    println!(
        "transient saves {base_saves}->{saves}, failed={}",
        metrics().db_save_failed()
    );
}

#[tokio::test]
#[ignore = "需要 pg_proxy.py/python 与真实 PG；手工跑: cargo test -p mir-server --test fault_pg -- --ignored --test-threads=1"]
async fn sustained_pool_close_records_failure_and_reconnects_new_store() {
    let Ok(url) = std::env::var("MIR2_TEST_DATABASE_URL") else {
        panic!("MIR2_TEST_DATABASE_URL 未设置；fail-loud")
    };
    let _guard = METRICS_LOCK.lock().await;
    let (addr, store, handle) = start_pg_server(&url).await;
    let user = format!(
        "fb{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("时钟")
            .as_nanos()
    );
    let mut client = login(addr, &user).await;
    let base_failed = metrics().db_save_failed();
    let base_saves = metrics().db_saves();
    store.pool().close().await;
    client
        .send(ClientPacket::Walk(c::Walk {
            direction: mir_core::enums::MirDirection::Right,
        }))
        .await;
    let failed = wait_metric(base_failed, |d| d >= 1, "failed").await;
    assert_eq!(metrics().db_saves(), base_saves, "失败不应增加成功计数");
    client.still_alive().await;

    // 恢复：新 store 能重连并写回；断言 DB 内容等于这次写入的内存快照。
    let recovered = PgAccountStore::connect(&tagged_database_url(&url))
        .await
        .expect("恢复连接");
    let account = recovered
        .find_account(&user)
        .await
        .expect("查账号")
        .expect("账号存在");
    let mut character = recovered
        .list_characters(account.id)
        .await
        .expect("角色列表")
        .into_iter()
        .next()
        .expect("角色存在");
    character.gold = 4242;
    recovered
        .save_character(character.clone())
        .await
        .expect("恢复后保存成功");
    let loaded = recovered
        .find_character(account.id, character.id)
        .await
        .expect("读回")
        .expect("角色存在");
    assert_eq!(loaded, character, "恢复后 DB == 内存快照");
    handle.abort();
    println!("sustained failed {base_failed}->{failed}, saves unchanged {base_saves}");
}

#[tokio::test]
#[ignore = "需要 pg_proxy.py/python 与真实 PG；手工跑: cargo test -p mir-server --test fault_pg -- --ignored --test-threads=1"]
async fn saturated_test_pool_waits_then_succeeds_after_release() {
    let Ok(url) = std::env::var("MIR2_TEST_DATABASE_URL") else {
        panic!("MIR2_TEST_DATABASE_URL 未设置；fail-loud")
    };
    let _guard = METRICS_LOCK.lock().await;
    let url = tagged_database_url(&url);
    let store = Arc::new(
        PgAccountStore::connect_with_max_connections(&url, 2)
            .await
            .expect("测试池 max=2"),
    );
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("时钟")
        .as_nanos();
    let account = store
        .create_account(&format!("sat{suffix}"), "secret", "")
        .await
        .expect("建号");
    let character = store
        .create_character(
            account.id,
            &format!("池满{suffix}"),
            mir_core::enums::MirClass::Warrior,
            mir_core::enums::MirGender::Male,
            "0",
            (10, 10),
        )
        .await
        .expect("建角");

    let first = store.pool().acquire().await.expect("占第1条");
    let second = store.pool().acquire().await.expect("占第2条");
    let mut dirty = character.clone();
    dirty.gold = 5150;
    let task = tokio::spawn({
        let store = Arc::clone(&store);
        async move { store.save_character(dirty).await }
    });
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    assert!(!task.is_finished(), "池已满时 save 应排队而不是立刻成功");
    drop(first);
    drop(second);
    let result = tokio::time::timeout(std::time::Duration::from_secs(8), task)
        .await
        .expect("释放连接后 save 应在超时内完成")
        .expect("join");
    result.expect("第 3 次 acquire 释放后应成功");
    let loaded = store
        .find_character(account.id, character.id)
        .await
        .expect("读库")
        .expect("存在");
    assert_eq!(loaded.gold, 5150, "排队后写入必须落库");
    println!("saturated pool: 2 held, save queued, release -> success, gold=5150");
}

fn split_db_endpoint(url: &str) -> (String, u16) {
    let host_part = url.split('@').nth(1).expect("URL 需要 user@host");
    let host_port = host_part.split('/').next().expect("host");
    let (host, port) = host_port.rsplit_once(':').expect("host:port");
    (host.to_owned(), port.parse().expect("port"))
}

fn proxied_database_url(url: &str) -> String {
    let (start, rest) = url.split_once('@').expect("URL 需要 @");
    let after = rest.split_once('/').map_or(rest, |(_, tail)| tail);
    format!("{start}@127.0.0.1:15432/{after}")
}

async fn wait_proxy_ready() {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        match TcpStream::connect("127.0.0.1:15432").await {
            Ok(_) => return,
            Err(_) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            Err(error) => panic!("代理未就绪：{error}"),
        }
    }
}

#[tokio::test]
#[ignore = "需要 pg_proxy.py/python 与真实 PG；手工跑: cargo test -p mir-server --test fault_pg -- --ignored --test-threads=1"]
async fn proxy_outage_fails_save_then_recovers_after_resume() {
    let Ok(url) = std::env::var("MIR2_TEST_DATABASE_URL") else {
        panic!("MIR2_TEST_DATABASE_URL 未设置；fail-loud")
    };
    let _guard = METRICS_LOCK.lock().await;
    let (host, port) = split_db_endpoint(&url);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let control = root.join("target/pg_proxy.control");
    std::fs::write(&control, "resume").expect("写控制文件");
    let script = root.join("tools/dev/pg_proxy.py");
    let mut proxy = std::process::Command::new("py")
        .arg(&script)
        .env("PG_PROXY_PORT", "15432")
        .env("PG_PROXY_TARGET_HOST", &host)
        .env("PG_PROXY_TARGET_PORT", port.to_string())
        .env("PG_PROXY_CONTROL", &control)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("启动 pg_proxy.py");
    wait_proxy_ready().await;

    let proxy_url = proxied_database_url(&url);
    let (addr, store, handle) = start_pg_server(&proxy_url).await;
    let user = format!(
        "fp{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("时钟")
            .as_nanos()
    );
    let mut client = login(addr, &user).await;
    let base_saves = metrics().db_saves();
    let base_failed = metrics().db_save_failed();

    let mut admin = store.pool().acquire().await.expect("管理连接");
    let pids: Vec<i32> = sqlx::query(
        "SELECT pid FROM pg_stat_activity WHERE datname = current_database() \
         AND application_name = 'mir2_fault_test' AND pid <> pg_backend_pid()",
    )
    .fetch_all(&mut *admin)
    .await
    .expect("查连接")
    .into_iter()
    .map(|row| row.get("pid"))
    .collect();
    for pid in &pids {
        sqlx::query("SELECT pg_terminate_backend($1)")
            .bind(pid)
            .execute(&mut *admin)
            .await
            .expect("断连");
    }
    std::fs::write(&control, "pause").expect("暂停代理");
    drop(admin);
    println!("proxy paused; terminated PIDs={pids:?}");

    client
        .send(ClientPacket::Walk(c::Walk {
            direction: mir_core::enums::MirDirection::Right,
        }))
        .await;
    let failed = wait_metric(base_failed, |d| d >= 1, "failed").await;
    assert_eq!(metrics().db_saves(), base_saves, "断期不应增加成功计数");
    client.still_alive().await;

    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    std::fs::write(&control, "resume").expect("恢复代理");
    let recovered = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        wait_metric(failed, |_| metrics().db_saves() > base_saves, "saves"),
    )
    .await
    .expect("恢复后 20s 内必须存盘成功");
    client.still_alive().await;
    println!("proxy resumed; saves {base_saves} -> {recovered}");
    handle.abort();
    let _ = handle.await;
    proxy.kill().ok();
    let _ = proxy.wait();
    std::fs::remove_file(&control).ok();
    assert!(recovered > base_saves);
}

#[tokio::test]
#[ignore = "需要 pg_proxy.py/python 与真实 PG；手工跑: cargo test -p mir-server --test fault_pg -- --ignored --test-threads=1"]
async fn durability_survives_a_real_postgres_round_trip() {
    let Ok(url) = std::env::var("MIR2_TEST_DATABASE_URL") else {
        panic!("MIR2_TEST_DATABASE_URL 未设置；本用例 fail-loud，不允许 skip");
    };
    let _guard = METRICS_LOCK.lock().await;
    let url = tagged_database_url(&url);
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("时钟")
        .as_nanos();
    let account_name = format!("dura-pg-{suffix}");
    let store = Arc::new(PgAccountStore::connect(&url).await.expect("连接测试库"));
    let account = store
        .create_account(&account_name, "secret", "")
        .await
        .expect("建号");
    let mut character = store
        .create_character(
            account.id,
            &format!("耐久角色{suffix}"),
            mir_core::enums::MirClass::Warrior,
            mir_core::enums::MirGender::Male,
            "0",
            (10, 10),
        )
        .await
        .expect("建角");
    let items = vec![Some(mir_protocol::data::UserItem {
        unique_id: 77,
        item_index: 1,
        current_dura: 1_234,
        max_dura: 5_000,
        ..mir_protocol::data::UserItem::default()
    })];
    character.inventory = mir_server::items::encode_slots(&items);
    store.save_character(character.clone()).await.expect("落盘");

    let reloaded = PgAccountStore::connect(&url).await.expect("重连");
    let row = reloaded
        .find_character(account.id, character.id)
        .await
        .expect("查角色")
        .expect("角色存在");
    let decoded = mir_server::items::decode_inventory(&row.inventory).expect("解码背包");
    let item = decoded[0].as_ref().expect("物品存在");
    assert_eq!(item.current_dura, 1_234, "DB 往返后 current_dura 必须一致");
    assert_eq!(item.max_dura, 5_000, "DB 往返后 max_dura 必须一致");
}
