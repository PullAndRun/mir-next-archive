//! `@reboot` 的端到端测试：倒计时公告 → 踢线 → 停止 accept。
//!
//! 单独一个测试目标（进程）：`@reboot` 用的是**进程级**通知，和别的网络用例放一起会
//! 互相打断，所以这里只跑重启链路。

use std::sync::Arc;
use std::time::Duration;

use mir_core::DateTimeBinary;
use mir_protocol::codec::FrameBuffer;
use mir_protocol::packets::client as c;
use mir_protocol::{ClientPacket, ServerPacket};
use mir_server::{MemoryStore, ServerConfig, serve_forever};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

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
