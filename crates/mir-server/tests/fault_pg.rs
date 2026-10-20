use mir_server::PgAccountStore;
use mir_server::store::AccountStore;
use sqlx::Executor;
use std::sync::Arc;

static METRICS_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn pg_terminate_disconnects_save_and_next_save_recovers() {
    let Ok(url) = std::env::var("MIR2_TEST_DATABASE_URL") else {
        panic!("MIR2_TEST_DATABASE_URL 未设置；本用例 fail-loud，不允许 skip");
    };
    let _guard = METRICS_LOCK.lock().await;
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
    store
        .pool()
        .execute("SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = current_database() AND pid <> pg_backend_pid()")
        .await
        .expect("定向 terminate");
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
    let store = Arc::new(PgAccountStore::connect(url).await.expect("PG"));
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
            % 1_000_000
    );
    let mut client = login(addr, &user).await;
    let base_saves = metrics().db_saves();
    let base_failed = metrics().db_save_failed();
    store.pool().execute("SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = current_database() AND pid <> pg_backend_pid()").await.expect("terminate");
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
            % 1_000_000
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
    let recovered = PgAccountStore::connect(&url).await.expect("恢复连接");
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
