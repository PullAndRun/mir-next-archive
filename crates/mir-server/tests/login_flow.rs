//! 端到端协议测试：真的起一个服务端，用真的 TCP 连接把登录链路走一遍。
//!
//! 这比单元测试更能证明「协议实现与 Crystal 一致」这件事没有在接线时走样：
//! 每个断言都是客户端视角收到的包。

use std::sync::Arc;
use std::time::Duration;

use mir_core::enums::{MirClass, MirDirection, MirGender};
use mir_protocol::codec::FrameBuffer;
use mir_protocol::packets::client as c;
use mir_protocol::packets::server as s;
use mir_protocol::{ClientPacket, ServerPacket};
use mir_server::{AccountStore, MemoryStore, ServerConfig, serve_forever};
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
        let frame = packet.encode_frame();
        self.stream.write_all(&frame).await.expect("发送");
    }

    async fn expect(&mut self) -> ServerPacket {
        for _ in 0..40 {
            if let Some((index, payload)) = self.frames.next_frame().expect("帧格式") {
                return ServerPacket::decode(index, &payload).expect("解析服务端包");
            }
            let mut chunk = [0u8; 4096];
            let read = tokio::time::timeout(Duration::from_secs(5), self.stream.read(&mut chunk))
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
        // 测试不依赖 resource/ 下的真实地图，走无地形的兜底世界。
        map_root: "resource/不存在的目录".to_owned(),
        ..ServerConfig::default()
    })
}

#[tokio::test]
async fn full_login_character_and_world_flow() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("绑定临时端口");
    let address = listener.local_addr().expect("取端口");
    let config = test_config();
    let store = Arc::new(MemoryStore::new());
    tokio::spawn(serve_forever(
        listener,
        Arc::clone(&config),
        Arc::clone(&store),
    ));

    let mut client = TestClient::connect(address).await;

    // 1. 连上就先收到 Connected（MirConnection.cs:97）。
    client.expect_index(s::Connected::INDEX).await;

    // 2. 版本校验：回 Result = 1，然后进入登录阶段。
    client
        .send(ClientPacket::ClientVersion(c::ClientVersion {
            version_hash: vec![0u8; 16],
        }))
        .await;
    let packet = client.expect_index(s::ClientVersion::INDEX).await;
    match packet {
        ServerPacket::ClientVersion(response) => assert_eq!(response.result, 1),
        other => panic!("期望 ClientVersion，收到 {other:?}"),
    }

    // 3. 注册账号：成功码是 8。
    client
        .send(ClientPacket::NewAccount(c::NewAccount {
            account_id: "diana".to_owned(),
            password: "secret".to_owned(),
            birth_date: mir_core::DateTimeBinary(0),
            user_name: String::new(),
            secret_question: String::new(),
            secret_answer: String::new(),
            e_mail_address: String::new(),
        }))
        .await;
    let packet = client.expect_index(s::NewAccount::INDEX).await;
    match packet {
        ServerPacket::NewAccount(response) => assert_eq!(response.result, 8),
        other => panic!("期望 NewAccount，收到 {other:?}"),
    }

    // 4. 登录：服务端直接回 LoginSuccess（空角色列表）。
    client
        .send(ClientPacket::Login(c::Login {
            account_id: "diana".to_owned(),
            password: "secret".to_owned(),
        }))
        .await;
    let packet = client.expect_index(s::LoginSuccess::INDEX).await;
    let characters = match packet {
        ServerPacket::LoginSuccess(success) => success.characters,
        other => panic!("期望 LoginSuccess，收到 {other:?}"),
    };
    assert!(characters.is_empty(), "新账号没有角色");

    // 5. 建角色：回 NewCharacterSuccess + 选角信息。
    client
        .send(ClientPacket::NewCharacter(c::NewCharacter {
            name: "裁决者".to_owned(),
            gender: MirGender::Male,
            class: MirClass::Warrior,
        }))
        .await;
    let packet = client.expect_index(s::NewCharacterSuccess::INDEX).await;
    let character = match packet {
        ServerPacket::NewCharacterSuccess(success) => success.char_info,
        other => panic!("期望 NewCharacterSuccess，收到 {other:?}"),
    };
    assert_eq!(character.name, "裁决者");
    assert_eq!(character.level, 0, "Crystal 建角时等级是 0，进世界才补 1");

    // 6. 进世界：StartGame{Result=4} → MapInformation → UserInformation → 欢迎语。
    client
        .send(ClientPacket::StartGame(c::StartGame {
            character_index: character.index,
        }))
        .await;
    let packet = client.expect_index(s::StartGame::INDEX).await;
    match packet {
        ServerPacket::StartGame(response) => {
            assert_eq!(response.result, 4, "4 = 成功");
            assert_eq!(response.resolution, config.allowed_resolution);
        }
        other => panic!("期望 StartGame，收到 {other:?}"),
    }
    let packet = client.expect_index(s::MapInformation::INDEX).await;
    match packet {
        ServerPacket::MapInformation(info) => {
            assert_eq!(info.file_name, "0");
            assert_eq!(info.title, "比奇省", "地图标题默认走中文语言包");
        }
        other => panic!("期望 MapInformation，收到 {other:?}"),
    }
    let packet = client.expect_index(s::UserInformation::INDEX).await;
    let object_id = match packet {
        ServerPacket::UserInformation(info) => {
            assert_eq!(info.name, "裁决者");
            assert_eq!(info.level, 1, "首次进世界补到 1 级");
            assert_eq!(info.mp, 15);
            info.object_id
        }
        other => panic!("期望 UserInformation，收到 {other:?}"),
    };
    let packet = client.expect_index(s::Chat::INDEX).await;
    match packet {
        ServerPacket::Chat(chat) => {
            assert_eq!(chat.type_, mir_core::enums::ChatType::System);
            assert!(
                chat.message.contains("欢迎"),
                "默认中文欢迎语：{}",
                chat.message
            );
        }
        other => panic!("期望 Chat，收到 {other:?}"),
    }

    // 7. 走路：服务端回 ObjectWalk（移动者本人也会收到）。
    client
        .send(ClientPacket::Walk(c::Walk {
            direction: MirDirection::Right,
        }))
        .await;
    let packet = client.expect_index(s::ObjectWalk::INDEX).await;
    match packet {
        ServerPacket::ObjectWalk(walk) => {
            assert_eq!(walk.object_id, object_id);
            assert_eq!(walk.direction, MirDirection::Right);
        }
        other => panic!("期望 ObjectWalk，收到 {other:?}"),
    }

    // 8. 游戏内切换语言：`/language en` 之后收到英文提示。
    client
        .send(ClientPacket::Chat(c::Chat {
            message: "/language en".to_owned(),
            linked_items: Vec::new(),
        }))
        .await;
    let packet = client.expect_index(s::Chat::INDEX).await;
    match packet {
        ServerPacket::Chat(chat) => {
            assert_eq!(chat.message, "Language switched to English.");
        }
        other => panic!("期望 Chat，收到 {other:?}"),
    }

    // 9. 退出登录：服务端回 LogOutSuccess 与该账号的角色列表。
    client.send(ClientPacket::LogOut(c::LogOut {})).await;
    let packet = client.expect_index(s::LogOutSuccess::INDEX).await;
    match packet {
        ServerPacket::LogOutSuccess(success) => {
            assert_eq!(success.characters.len(), 1);
            assert_eq!(success.characters[0].name, "裁决者");
        }
        other => panic!("期望 LogOutSuccess，收到 {other:?}"),
    }

    // 10. 语言随账号持久化：重新连一条连接并登录，`/language` 应回英文。
    let mut again = TestClient::connect(address).await;
    again.expect_index(s::Connected::INDEX).await;
    again
        .send(ClientPacket::ClientVersion(c::ClientVersion {
            version_hash: Vec::new(),
        }))
        .await;
    again.expect_index(s::ClientVersion::INDEX).await;
    again
        .send(ClientPacket::Login(c::Login {
            account_id: "diana".to_owned(),
            password: "secret".to_owned(),
        }))
        .await;
    again.expect_index(s::LoginSuccess::INDEX).await;
    again
        .send(ClientPacket::Chat(c::Chat {
            message: "/language".to_owned(),
            linked_items: Vec::new(),
        }))
        .await;
    let packet = again.expect_index(s::Chat::INDEX).await;
    match packet {
        ServerPacket::Chat(chat) => assert_eq!(
            chat.message, "Language switched to English.",
            "上次选的英文应该被带回来；聊天指令仍只在 Select 阶段生效"
        ),
        other => panic!("期望 Chat，收到 {other:?}"),
    }
}

#[tokio::test]
async fn wrong_password_is_rejected_with_result_4() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("绑定临时端口");
    let address = listener.local_addr().expect("取端口");
    let store = Arc::new(MemoryStore::new());
    tokio::spawn(serve_forever(listener, test_config(), Arc::clone(&store)));

    let mut client = TestClient::connect(address).await;
    client.expect_index(s::Connected::INDEX).await;
    client
        .send(ClientPacket::ClientVersion(c::ClientVersion {
            version_hash: Vec::new(),
        }))
        .await;
    client.expect_index(s::ClientVersion::INDEX).await;

    store
        .as_ref()
        .create_account("diana", "secret", "")
        .await
        .expect("准备账号");

    client
        .send(ClientPacket::Login(c::Login {
            account_id: "diana".to_owned(),
            password: "wrong".to_owned(),
        }))
        .await;
    let packet = client.expect_index(s::Login::INDEX).await;
    match packet {
        ServerPacket::Login(response) => assert_eq!(response.result, 4, "4 = 账号或密码错误"),
        other => panic!("期望 Login，收到 {other:?}"),
    }
}
