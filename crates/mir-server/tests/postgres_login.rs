//! 需要真实 PostgreSQL 的端到端测试。
//!
//! 没有设置 `MIR2_TEST_DATABASE_URL` 时直接跳过（CI 默认跳过），
//! 以免把开发机的生产库当成测试库。

use std::sync::Arc;
use std::time::Duration;

use mir_protocol::codec::FrameBuffer;
use mir_protocol::packets::client as c;
use mir_protocol::packets::server as s;
use mir_protocol::{ClientPacket, ServerPacket};
use mir_server::{AccountStore, PgAccountStore, ServerConfig, serve_forever};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

struct TestClient {
    stream: TcpStream,
    frames: FrameBuffer,
}

impl TestClient {
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

    async fn expect(&mut self) -> ServerPacket {
        for _ in 0..40 {
            if let Some((index, payload)) = self.frames.next_frame().expect("帧格式") {
                return ServerPacket::decode(index, &payload).expect("解析服务端包");
            }
            let mut chunk = [0u8; 4096];
            let read = tokio::time::timeout(Duration::from_secs(10), self.stream.read(&mut chunk))
                .await
                .expect("读取超时")
                .expect("读取");
            assert!(read > 0, "服务端提前关闭连接");
            self.frames.push(&chunk[..read]);
        }
        panic!("等待服务端包超时");
    }

    async fn expect_index(&mut self, index: i16) -> ServerPacket {
        let packet = self.expect().await;
        assert_eq!(packet.index(), index, "收到的包编号不符：{packet:?}");
        packet
    }
}

fn test_config() -> Arc<ServerConfig> {
    Arc::new(ServerConfig {
        map_root: "resource/不存在的目录".to_owned(),
        language_dir: "config/不存在的目录".to_owned(),
        ..ServerConfig::default()
    })
}

#[tokio::test]
async fn postgres_backed_login_and_language_persistence() {
    let Ok(url) = std::env::var("MIR2_TEST_DATABASE_URL") else {
        println!("跳过：未设置 MIR2_TEST_DATABASE_URL（需要一个可写的 PostgreSQL 测试库）");
        return;
    };
    let store = Arc::new(
        PgAccountStore::connect(&url)
            .await
            .expect("连接测试库并迁移"),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("绑定临时端口");
    let address = listener.local_addr().expect("取端口");
    tokio::spawn(serve_forever(listener, test_config(), Arc::clone(&store)));

    // 用时间戳保证账号名唯一，测试可重复跑；账号名上限 15 位，只取后 10 位。
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let username = format!("smk{}", suffix % 10_000_000_000);

    let mut client = TestClient::connect(address).await;
    client.expect_index(s::Connected::INDEX).await;
    client
        .send(ClientPacket::ClientVersion(c::ClientVersion {
            version_hash: Vec::new(),
        }))
        .await;
    client.expect_index(s::ClientVersion::INDEX).await;

    client
        .send(ClientPacket::NewAccount(c::NewAccount {
            account_id: username.clone(),
            password: "secret".to_owned(),
            birth_date: mir_core::DateTimeBinary(0),
            user_name: String::new(),
            secret_question: String::new(),
            secret_answer: String::new(),
            e_mail_address: String::new(),
        }))
        .await;
    match client.expect_index(s::NewAccount::INDEX).await {
        ServerPacket::NewAccount(response) => assert_eq!(response.result, 8, "建号成功"),
        other => panic!("期望 NewAccount，收到 {other:?}"),
    }

    client
        .send(ClientPacket::Login(c::Login {
            account_id: username.clone(),
            password: "secret".to_owned(),
        }))
        .await;
    client.expect_index(s::LoginSuccess::INDEX).await;

    // 游戏内切到英文，服务端应写回 accounts.language。
    client
        .send(ClientPacket::Chat(c::Chat {
            message: "/language en".to_owned(),
            linked_items: Vec::new(),
        }))
        .await;
    match client.expect_index(s::Chat::INDEX).await {
        ServerPacket::Chat(chat) => assert_eq!(chat.message, "Language switched to English."),
        other => panic!("期望 Chat，收到 {other:?}"),
    }

    // 直接查库确认落盘（等价于「重启后仍然记住」）。
    let account = store
        .as_ref()
        .find_account(&username)
        .await
        .expect("查账号")
        .expect("账号存在");
    assert_eq!(account.language, "en", "语言设置应写回数据库");
}
