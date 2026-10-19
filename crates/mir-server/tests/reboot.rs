//! `@reboot` 的端到端测试：倒计时公告 → 踢线 → 停止 accept。
//!
//! 单独一个测试目标（进程）：`@reboot` 用的是**进程级**通知，和别的网络用例放一起会
//! 互相打断，所以这里只跑重启链路。

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use mir_core::DateTimeBinary;
use mir_protocol::codec::FrameBuffer;
use mir_protocol::packets::client as c;
use mir_protocol::{ClientPacket, ServerPacket};
use mir_server::{AccountStore, MemoryStore, ServerConfig, serve_forever};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// `gm` 的重启状态是进程级静态：同一测试二进制里的用例必须串行，否则会互相清掉倒计时。
async fn gm_guard() -> tokio::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}

struct Client {
    stream: TcpStream,
    frames: FrameBuffer,
}

impl Client {
    async fn connect(address: std::net::SocketAddr) -> Self {
        let stream = TcpStream::connect(address).await.expect("连接服务端");
        stream.set_nodelay(true).ok();
        Self {
            stream,
            frames: FrameBuffer::new(),
        }
    }

    async fn send(&mut self, packet: ClientPacket) {
        self.stream
            .write_all(&packet.encode_frame())
            .await
            .expect("发送");
    }

    /// 读到下一个包（最多等 `timeout`）。
    async fn next(&mut self, timeout: Duration) -> Option<ServerPacket> {
        let deadline = tokio::time::Instant::now() + timeout;
        let mut chunk = [0u8; 4096];
        loop {
            if let Some((index, payload)) = self.frames.next_frame().expect("帧格式") {
                return Some(ServerPacket::decode(index, &payload).expect("解析服务端包"));
            }
            if tokio::time::Instant::now() >= deadline {
                return None;
            }
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            match tokio::time::timeout(remaining, self.stream.read(&mut chunk)).await {
                Ok(Ok(0) | Err(_)) | Err(_) => return None,
                Ok(Ok(read)) => self.frames.push(&chunk[..read]),
            }
        }
    }
}

#[tokio::test]
async fn reboot_counts_down_then_disconnects_and_stops_the_server() {
    let _guard = gm_guard().await;
    mir_server::gm::clear_reboot();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("绑定临时端口");
    let address = listener.local_addr().expect("取端口");
    let config = Arc::new(ServerConfig {
        map_root: "resource/不存在的目录".to_owned(),
        load_game_data: false,
        gm_accounts: vec!["root".to_owned()],
        idle_timeout_secs: 60,
        ..ServerConfig::default()
    });
    let server = tokio::spawn(serve_forever(
        listener,
        Arc::clone(&config),
        Arc::new(MemoryStore::new()),
    ));

    // 注册 + 登录 + 建角 + 进世界（GM 账号在 `gm_accounts` 名单里）。
    let mut client = Client::connect(address).await;
    assert!(matches!(
        client.next(Duration::from_secs(3)).await,
        Some(ServerPacket::Connected(_))
    ));
    client
        .send(ClientPacket::ClientVersion(c::ClientVersion {
            version_hash: Vec::new(),
        }))
        .await;
    client.next(Duration::from_secs(3)).await;
    client
        .send(ClientPacket::NewAccount(c::NewAccount {
            account_id: "root".to_owned(),
            password: "secret".to_owned(),
            birth_date: DateTimeBinary(0),
            user_name: String::new(),
            secret_question: String::new(),
            secret_answer: String::new(),
            e_mail_address: String::new(),
        }))
        .await;
    client.next(Duration::from_secs(3)).await;
    client
        .send(ClientPacket::Login(c::Login {
            account_id: "root".to_owned(),
            password: "secret".to_owned(),
        }))
        .await;
    client.next(Duration::from_secs(3)).await;
    client
        .send(ClientPacket::NewCharacter(c::NewCharacter {
            name: "重启测试".to_owned(),
            gender: mir_core::enums::MirGender::Male,
            class: mir_core::enums::MirClass::Warrior,
        }))
        .await;
    let index = loop {
        match client.next(Duration::from_secs(3)).await {
            Some(ServerPacket::NewCharacterSuccess(success)) => break success.char_info.index,
            Some(_) => {}
            None => panic!("没收到 NewCharacterSuccess"),
        }
    };
    client
        .send(ClientPacket::StartGame(c::StartGame {
            character_index: index,
        }))
        .await;
    loop {
        match client.next(Duration::from_secs(3)).await {
            Some(ServerPacket::UserInformation(_)) => break,
            Some(_) => {}
            None => panic!("没进世界"),
        }
    }

    // GM 发 `@reboot 1`：应当先收到全服公告，再收到自己的回执。
    client
        .send(ClientPacket::Chat(c::Chat {
            message: "@reboot 1".to_owned(),
            linked_items: Vec::new(),
        }))
        .await;
    let mut messages = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        match client.next(Duration::from_millis(500)).await {
            Some(ServerPacket::Chat(chat)) => messages.push(chat.message),
            Some(_) => {}
            None => break,
        }
        if messages.iter().any(|m| m.contains("reboot scheduled")) {
            break;
        }
    }
    let joined = messages.join("\n");
    assert!(
        joined.contains("Server is rebooting in 1 second(s)"),
        "玩家应收到倒计时公告：{joined}"
    );
    assert!(
        joined.contains("reboot scheduled in 1 second(s)"),
        "GM 应收到回执：{joined}"
    );

    // 到点后连接被断开（读到 EOF）。
    let mut closed = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(6);
    while tokio::time::Instant::now() < deadline {
        if client.next(Duration::from_millis(300)).await.is_none() {
            closed = true;
            break;
        }
    }
    assert!(closed, "@reboot 到点应断开客户端连接");

    // accept 循环退出：`serve_forever` 返回 Ok。
    let outcome = tokio::time::timeout(Duration::from_secs(6), server)
        .await
        .expect("serve_forever 应在上限内退出")
        .expect("任务不应 panic");
    assert!(outcome.is_ok(), "serve_forever 应正常退出：{outcome:?}");
}

async fn start_server(
    store: Arc<MemoryStore>,
    gm_account: &str,
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    mir_server::gm::clear_reboot();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("绑定临时端口");
    let address = listener.local_addr().expect("取端口");
    let config = Arc::new(ServerConfig {
        map_root: "resource/不存在的目录".to_owned(),
        load_game_data: false,
        gm_accounts: vec![gm_account.to_owned()],
        idle_timeout_secs: 60,
        autosave_secs: 0,
        ..ServerConfig::default()
    });
    let server = tokio::spawn(async move {
        let _ = serve_forever(listener, config, store).await;
    });
    (address, server)
}

async fn enter(address: std::net::SocketAddr, account: &str, character: &str) -> (Client, i32) {
    let mut client = Client::connect(address).await;
    assert!(matches!(
        client.next(Duration::from_secs(3)).await,
        Some(ServerPacket::Connected(_))
    ));
    client
        .send(ClientPacket::ClientVersion(c::ClientVersion {
            version_hash: Vec::new(),
        }))
        .await;
    client.next(Duration::from_secs(3)).await;
    client
        .send(ClientPacket::NewAccount(c::NewAccount {
            account_id: account.to_owned(),
            password: "secret".to_owned(),
            birth_date: DateTimeBinary(0),
            user_name: String::new(),
            secret_question: String::new(),
            secret_answer: String::new(),
            e_mail_address: String::new(),
        }))
        .await;
    client.next(Duration::from_secs(3)).await;
    client
        .send(ClientPacket::Login(c::Login {
            account_id: account.to_owned(),
            password: "secret".to_owned(),
        }))
        .await;
    let characters = match client.next(Duration::from_secs(3)).await {
        Some(ServerPacket::LoginSuccess(success)) => success.characters,
        other => panic!("LoginSuccess expected, got {other:?}"),
    };
    let index = if let Some(existing) = characters.first() {
        existing.index
    } else {
        client
            .send(ClientPacket::NewCharacter(c::NewCharacter {
                name: character.to_owned(),
                gender: mir_core::enums::MirGender::Male,
                class: mir_core::enums::MirClass::Warrior,
            }))
            .await;
        loop {
            match client.next(Duration::from_secs(3)).await {
                Some(ServerPacket::NewCharacterSuccess(success)) => {
                    break success.char_info.index;
                }
                Some(_) => {}
                None => panic!("没收到 NewCharacterSuccess"),
            }
        }
    };
    client
        .send(ClientPacket::StartGame(c::StartGame {
            character_index: index,
        }))
        .await;
    loop {
        match client.next(Duration::from_secs(3)).await {
            Some(ServerPacket::UserInformation(_)) => break,
            Some(_) => {}
            None => panic!("没进世界"),
        }
    }
    (client, index)
}

async fn grant_gold(client: &mut Client, amount: u32) {
    client
        .send(ClientPacket::Chat(c::Chat {
            message: format!("@gold {amount}"),
            linked_items: Vec::new(),
        }))
        .await;
    for _ in 0..20 {
        if let Some(ServerPacket::Chat(chat)) = client.next(Duration::from_millis(300)).await
            && chat.message.contains("gold")
        {
            break;
        }
    }
}

#[tokio::test]
async fn reboot_force_saves_online_character() {
    let _guard = gm_guard().await;
    let store = Arc::new(MemoryStore::new());
    let (address, server) = start_server(Arc::clone(&store), "rebootforce").await;
    let (mut client, index) = enter(address, "rebootforce", "ForceHero").await;
    grant_gold(&mut client, 555).await;
    mir_server::gm::schedule_reboot(0);
    tokio::time::sleep(Duration::from_millis(900)).await;
    let account = store.find_account("rebootforce").await.unwrap().unwrap().id;
    let row = store
        .find_character(account, i64::from(index))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.gold, 555, "停机前必须强制存盘");
    let _ = tokio::time::timeout(Duration::from_secs(3), server).await;
    mir_server::gm::clear_reboot();
}

#[tokio::test]
async fn reboot_force_save_retries_once_after_failure() {
    let _guard = gm_guard().await;
    let store = Arc::new(MemoryStore::new());
    let (address, server) = start_server(Arc::clone(&store), "rebootretry").await;
    let (mut client, index) = enter(address, "rebootretry", "RetryHero").await;
    grant_gold(&mut client, 666).await;
    store.fail_next_save_character().await;
    mir_server::gm::schedule_reboot(0);
    tokio::time::sleep(Duration::from_millis(900)).await;
    let account = store.find_account("rebootretry").await.unwrap().unwrap().id;
    let row = store
        .find_character(account, i64::from(index))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.gold, 666, "失败后重试一次应写成功");
    let _ = tokio::time::timeout(Duration::from_secs(3), server).await;
    mir_server::gm::clear_reboot();
}

#[tokio::test]
async fn reboot_countdown_broadcasts_seconds() {
    let _guard = gm_guard().await;
    let store = Arc::new(MemoryStore::new());
    let (address, server) = start_server(Arc::clone(&store), "rebootcast").await;
    let (mut client, _index) = enter(address, "rebootcast", "CastHero").await;
    mir_server::gm::schedule_reboot(10);
    tokio::time::sleep(Duration::from_millis(500)).await;
    let mut texts = Vec::new();
    while let Some(packet) = client.next(Duration::from_millis(300)).await {
        if let ServerPacket::Chat(chat) = packet {
            texts.push(chat.message);
            if texts.iter().any(|text| text.contains("reboot")) {
                break;
            }
        }
    }
    assert!(
        texts.iter().any(|text| text.contains("reboot")),
        "倒计时应广播 S.Chat：{texts:?}"
    );
    mir_server::gm::clear_reboot();
    server.abort();
}

#[tokio::test]
async fn reconnect_within_grace_restores_last_character() {
    let _guard = gm_guard().await;
    let store = Arc::new(MemoryStore::new());
    let (address, server) = start_server(Arc::clone(&store), "rebootgrace").await;
    let (mut client, _index) = enter(address, "rebootgrace", "GraceHero").await;
    grant_gold(&mut client, 777).await;
    mir_server::gm::schedule_reboot(0);
    tokio::time::sleep(Duration::from_millis(900)).await;
    let _ = tokio::time::timeout(Duration::from_secs(3), server).await;
    mir_server::gm::clear_reboot();

    let (new_address, new_server) = start_server(Arc::clone(&store), "rebootgrace").await;
    let mut again = Client::connect(new_address).await;
    assert!(matches!(
        again.next(Duration::from_secs(3)).await,
        Some(ServerPacket::Connected(_))
    ));
    again
        .send(ClientPacket::ClientVersion(c::ClientVersion {
            version_hash: Vec::new(),
        }))
        .await;
    again.next(Duration::from_secs(3)).await;
    again
        .send(ClientPacket::Login(c::Login {
            account_id: "rebootgrace".to_owned(),
            password: "secret".to_owned(),
        }))
        .await;
    let mut automatic_start = false;
    for _ in 0..40 {
        match again.next(Duration::from_millis(300)).await {
            Some(ServerPacket::StartGame(start)) => {
                assert_eq!(
                    start.result, 4,
                    "宽限期内应直接进游戏（Crystal StartGame 成功=4）"
                );
                automatic_start = true;
                break;
            }
            Some(_) => {}
            None => break,
        }
    }
    assert!(automatic_start, "宽限期内登录应自动 StartGame 恢复原角色");
    new_server.abort();
}

#[tokio::test]
async fn reconnect_after_grace_falls_back_to_select() {
    let _guard = gm_guard().await;
    let store = Arc::new(MemoryStore::new());
    let (address, server) = start_server(Arc::clone(&store), "rebootexpire").await;
    let (client, index) = enter(address, "rebootexpire", "ExpireHero").await;
    drop(client);
    let account = store
        .find_account("rebootexpire")
        .await
        .unwrap()
        .unwrap()
        .id;
    mir_server::session::remember_reconnect_at(
        account,
        i64::from(index),
        mir_server::gm::unix_now_secs() - 300,
    );
    let mut again = Client::connect(address).await;
    assert!(matches!(
        again.next(Duration::from_secs(3)).await,
        Some(ServerPacket::Connected(_))
    ));
    again
        .send(ClientPacket::ClientVersion(c::ClientVersion {
            version_hash: Vec::new(),
        }))
        .await;
    again.next(Duration::from_secs(3)).await;
    again
        .send(ClientPacket::Login(c::Login {
            account_id: "rebootexpire".to_owned(),
            password: "secret".to_owned(),
        }))
        .await;
    loop {
        match again.next(Duration::from_secs(3)).await {
            Some(ServerPacket::LoginSuccess(_)) => break,
            Some(_) => {}
            None => panic!("登录超时"),
        }
    }
    assert!(
        again.next(Duration::from_millis(300)).await.is_none(),
        "超过宽限期不应自动 StartGame"
    );
    server.abort();
}
