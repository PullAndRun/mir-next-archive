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
        // 也不读真实数据库：这样断言只面对测试自己造出来的包。
        load_game_data: false,
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
            account_id: "tester".to_owned(),
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
            account_id: "tester".to_owned(),
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

    // 6. 进世界：StartGame{Result=4} → MapInformation → UserInformation → SwitchGroup → 欢迎语。
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
    let _object_id = match packet {
        ServerPacket::UserInformation(info) => {
            assert_eq!(info.name, "裁决者");
            assert_eq!(info.level, 1, "首次进世界补到 1 级");
            assert_eq!(info.mp, 15);
            info.object_id
        }
        other => panic!("期望 UserInformation，收到 {other:?}"),
    };
    // 攻击模式：Crystal 在进世界序列里补一条 `S.ChangeAMode { Mode = AMode }`
    // （`Server/MirObjects/PlayerObject.cs:1217`）。新角色默认和平模式。
    let packet = client.expect_index(s::ChangeAMode::INDEX).await;
    match packet {
        ServerPacket::ChangeAMode(changed) => {
            assert_eq!(changed.mode, mir_core::enums::AttackMode::Peace);
        }
        other => panic!("期望 ChangeAMode，收到 {other:?}"),
    }
    // 组队开关的初值（Crystal `Enqueue(new S.SwitchGroup { AllowGroup = AllowGroup });`，
    // `PlayerObject.cs:1219`）。本作不持久化 `AllowGroup`，进世界默认允许组队。
    let packet = client.expect_index(s::SwitchGroup::INDEX).await;
    match packet {
        ServerPacket::SwitchGroup(switch) => assert!(switch.allow_group),
        other => panic!("期望 SwitchGroup，收到 {other:?}"),
    }
    // 行会 buff 的可选列表：Crystal `StartGame` 无条件发（`PlayerObject.cs:1223-1224`）；
    // 这个测试的世界用默认行会配置（没有 `[Buff-N]`），所以列表是空的。
    let packet = client.expect_index(s::GuildBuffList::INDEX).await;
    match packet {
        ServerPacket::GuildBuffList(list) => {
            assert!(list.guild_buffs.is_empty());
            assert!(list.active_buffs.is_empty());
        }
        other => panic!("期望 GuildBuffList，收到 {other:?}"),
    }
    // 邮箱：Crystal 在 `GetCompletedQuests()` 之后调 `GetMail()`
    // （`Server/MirObjects/PlayerObject.cs:1190`），新角色邮箱是空的。
    let packet = client.expect_index(s::ReceiveMail::INDEX).await;
    match packet {
        ServerPacket::ReceiveMail(mail) => assert!(mail.mail.is_empty(), "新角色没有邮件"),
        other => panic!("期望 ReceiveMail，收到 {other:?}"),
    }
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

    // 7. 走路：本人收到 UserLocation（新坐标），周围玩家才收 ObjectWalk。
    client
        .send(ClientPacket::Walk(c::Walk {
            direction: MirDirection::Right,
        }))
        .await;
    let packet = client.expect_index(s::UserLocation::INDEX).await;
    match packet {
        ServerPacket::UserLocation(location) => {
            assert_eq!(location.location, mir_core::Point::new(331, 270));
            assert_eq!(location.direction, MirDirection::Right);
        }
        other => panic!("期望 UserLocation，收到 {other:?}"),
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
            account_id: "tester".to_owned(),
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
        .create_account("tester", "secret", "")
        .await
        .expect("准备账号");

    client
        .send(ClientPacket::Login(c::Login {
            account_id: "tester".to_owned(),
            password: "wrong".to_owned(),
        }))
        .await;
    let packet = client.expect_index(s::Login::INDEX).await;
    match packet {
        ServerPacket::Login(response) => assert_eq!(response.result, 4, "4 = 账号或密码错误"),
        other => panic!("期望 Login，收到 {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// GM 命令的权限门（t33）：`@...` 只有 GM 账号才被消费
// ---------------------------------------------------------------------------

/// 等 `S.LogOutSuccess`：进世界序列里还会夹着 `ChangeAMode` / `SwitchGroup` 等包，跳过即可。
async fn expect_logout(client: &mut TestClient) {
    for _ in 0..40 {
        if let ServerPacket::LogOutSuccess(_) = client.expect().await {
            return;
        }
    }
    panic!("没收到 LogOutSuccess");
}

/// 起一个测试服务器，`gm_accounts` 里放 `"gmuser"`。
fn gm_config() -> Arc<ServerConfig> {
    Arc::new(ServerConfig {
        map_root: "resource/不存在的目录".to_owned(),
        load_game_data: false,
        gm_accounts: vec!["gmuser".to_owned()],
        ..ServerConfig::default()
    })
}

/// 只做「连接 → 版本 → 登录 → 进世界」，账号/角色由测试自己预先建好。
///
/// `character_index` 是角色**主键**（服务端 `find_character(account, id)` 用它）。
async fn login_existing(
    address: std::net::SocketAddr,
    account: &str,
    character_index: i32,
) -> TestClient {
    let mut client = TestClient::connect(address).await;
    client.expect_index(s::Connected::INDEX).await;
    client
        .send(ClientPacket::ClientVersion(c::ClientVersion {
            version_hash: Vec::new(),
        }))
        .await;
    client.expect_index(s::ClientVersion::INDEX).await;
    client
        .send(ClientPacket::Login(c::Login {
            account_id: account.to_owned(),
            password: "secret".to_owned(),
        }))
        .await;
    client.expect_index(s::LoginSuccess::INDEX).await;
    client
        .send(ClientPacket::StartGame(c::StartGame { character_index }))
        .await;
    loop {
        match client.expect().await {
            ServerPacket::StartGame(response) => assert_eq!(response.result, 4, "进世界应成功"),
            ServerPacket::UserInformation(_) => break,
            _ => {}
        }
    }
    client
}

/// 完整走「连接 → 版本 → 注册 → 登录 → 建角 → 进世界」，返回在游戏里的客户端。
async fn enter_world(address: std::net::SocketAddr, account: &str) -> TestClient {
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
            account_id: account.to_owned(),
            password: "secret".to_owned(),
            birth_date: mir_core::DateTimeBinary(0),
            user_name: String::new(),
            secret_question: String::new(),
            secret_answer: String::new(),
            e_mail_address: String::new(),
        }))
        .await;
    client.expect_index(s::NewAccount::INDEX).await;
    client
        .send(ClientPacket::Login(c::Login {
            account_id: account.to_owned(),
            password: "secret".to_owned(),
        }))
        .await;
    client.expect_index(s::LoginSuccess::INDEX).await;
    client
        .send(ClientPacket::NewCharacter(c::NewCharacter {
            name: format!("{account}_char"),
            gender: MirGender::Male,
            class: MirClass::Warrior,
        }))
        .await;
    let character_index = match client.expect_index(s::NewCharacterSuccess::INDEX).await {
        // `char_info.index` 是角色**主键**（服务端 `find_character(account, id)` 用它），
        // 不是角色列表下标。
        ServerPacket::NewCharacterSuccess(success) => success.char_info.index,
        other => panic!("期望 NewCharacterSuccess，收到 {other:?}"),
    };
    client
        .send(ClientPacket::StartGame(c::StartGame { character_index }))
        .await;
    // 进世界序列：`StartGame{result=4}` 之后还有 `NewItemInfo`/`MapInformation`/`ObjectPlayer`…
    // 这里只挑关键的两条断言（成功码 + `UserInformation`），其余跳过。
    loop {
        match client.expect().await {
            ServerPacket::StartGame(response) => assert_eq!(response.result, 4, "进世界应成功"),
            ServerPacket::UserInformation(_) => break,
            _ => {}
        }
    }
    client
}

#[tokio::test]
async fn gm_chat_is_silent_for_normal_accounts_and_works_for_gm() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("绑定临时端口");
    let address = listener.local_addr().expect("取端口");
    tokio::spawn(serve_forever(
        listener,
        gm_config(),
        Arc::new(MemoryStore::new()),
    ));

    // ---- 非 GM：`@level 50` 按普通聊天走，不生效、不报错、不泄露命令存在 ----
    let mut plain = enter_world(address, "plainuser").await;
    plain
        .send(ClientPacket::Chat(c::Chat {
            message: "@level 50".to_owned(),
            linked_items: Vec::new(),
        }))
        .await;
    // 普通聊天广播回自己时是 `S.ObjectChat`（Crystal 的本地图广播），
    // 说明这条 `@...` 没被当成命令消费。
    loop {
        match plain.expect().await {
            ServerPacket::ObjectChat(chat) => {
                assert_eq!(chat.text, "@level 50", "非 GM 的 @... 原样广播");
                break;
            }
            ServerPacket::LevelChanged(_) => panic!("非 GM 的 @level 不能生效"),
            _ => {}
        }
    }

    // ---- GM：同一句命令生效，回英文提示 + 等级变化 ----
    let mut gm = enter_world(address, "gmuser").await;
    gm.send(ClientPacket::Chat(c::Chat {
        message: "@level 50".to_owned(),
        linked_items: Vec::new(),
    }))
    .await;
    let mut reply = None;
    let mut leveled = None;
    for _ in 0..20 {
        let packet = gm.expect().await;
        match packet {
            ServerPacket::Chat(chat) => {
                if chat.message.starts_with("Level set to 50") {
                    reply = Some(chat.message);
                }
            }
            ServerPacket::LevelChanged(change) => {
                leveled = Some(change);
            }
            _ => {}
        }
        if reply.is_some() && leveled.is_some() {
            break;
        }
    }
    assert!(reply.is_some(), "GM 应收到英文回执");
    let leveled = leveled.expect("GM 应收到 S.LevelChanged");
    assert_eq!(leveled.level, 50);
    assert!(leveled.max_experience > 0, "下一级经验上限要按新等级重算");
}

// ---------------------------------------------------------------------------
// 登录每日重置的落盘（t40 / 刑部第 5 轮 F1）
// ---------------------------------------------------------------------------

/// 登录命中每日重置时，必须在 `world.enter` 之前把角色行写库（不等下线）。
#[tokio::test]
async fn daily_reset_persists_before_entering_world() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("绑定临时端口");
    let address = listener.local_addr().expect("取端口");
    let store = Arc::new(MemoryStore::new());
    tokio::spawn(serve_forever(listener, test_config(), Arc::clone(&store)));

    let account = store
        .create_account("resetuser", "secret", "")
        .await
        .expect("建账号");
    let mut character = store
        .create_character(
            account.id,
            "重置者",
            MirClass::Warrior,
            MirGender::Male,
            "0",
            (330, 270),
        )
        .await
        .expect("建角色");
    // `0` = 从未重置过 → 下次登录必然命中每日重置。
    character.last_daily_reset_at = 0;
    store
        .save_character(character.clone())
        .await
        .expect("预置角色");

    let _client = login_existing(address, "resetuser", i32::try_from(character.id).unwrap()).await;

    // 关键断言：还在游戏里（未下线、未走任何快照路径），库里已经是新时间戳。
    let row = store
        .find_character(account.id, character.id)
        .await
        .expect("查角色")
        .expect("角色存在");
    assert!(
        row.last_daily_reset_at > 0,
        "登录命中每日重置后应立即落盘（不等下线），实际 {}",
        row.last_daily_reset_at
    );
}

/// 落盘失败不能把玩家挡在门外：登录仍成功，库里保持旧值（下次登录幂等重做）。
#[tokio::test]
async fn daily_reset_save_failure_does_not_block_login() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("绑定临时端口");
    let address = listener.local_addr().expect("取端口");
    let store = Arc::new(MemoryStore::new());
    tokio::spawn(serve_forever(listener, test_config(), Arc::clone(&store)));

    let account = store
        .create_account("failuser", "secret", "")
        .await
        .expect("建账号");
    let mut character = store
        .create_character(
            account.id,
            "失败者",
            MirClass::Warrior,
            MirGender::Male,
            "0",
            (330, 270),
        )
        .await
        .expect("建角色");
    character.last_daily_reset_at = 0;
    // 等级 ≥ 1：跳过 on_start_game 里「首次建角补 1 级」的那次 save_character，
    // 保证注入的失败正好落在每日重置那次写入上。
    character.level = 10;
    // 预置一件普通背包内容，用于断言资源没被清掉。
    character.inventory = mir_server::items::encode_slots(&[Some(mir_protocol::data::UserItem {
        unique_id: 4_321,
        item_index: 1,
        count: 1,
        ..Default::default()
    })]);
    store
        .save_character(character.clone())
        .await
        .expect("预置角色");

    // 让登录时那一次 `save_character` 失败（`login_existing` 之前先武装）。
    store.fail_next_save_character().await;
    let _client = login_existing(address, "failuser", i32::try_from(character.id).unwrap()).await;

    // 登录成功（能走到这里说明收到了 UserInformation）且库里的角色行没被写坏。
    let row = store
        .find_character(account.id, character.id)
        .await
        .expect("查角色")
        .expect("角色存在");
    assert_eq!(
        row.last_daily_reset_at, 0,
        "落盘失败时库里保持旧值（下次登录幂等重做）"
    );
    let slots = mir_server::decode_slots(&row.inventory).expect("背包字节块");
    assert!(
        slots.iter().flatten().any(|item| item.unique_id == 4_321),
        "落盘失败不能丢背包内容"
    );
}

/// `market::character_snapshot` 必须透传 `last_daily_reset_at`（否则市场/商城落盘会写回旧值）。
#[test]
fn character_snapshot_carries_daily_reset_timestamp() {
    use mir_core::Point;
    use mir_core::enums::{AttackMode, MirDirection};
    use mir_server::world::Player;
    use mir_server::{MagicState, NpcState, TradeState};

    let base = mir_server::store::Character {
        last_daily_reset_at: 1_700_000_000_123,
        ..mir_server::store::Character::new(
            7,
            1,
            "快照者",
            MirClass::Warrior,
            MirGender::Male,
            "0",
            (10, 10),
        )
    };
    let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel();
    let player = Player {
        object_id: 1,
        session_id: 1,
        character_id: 7,
        name: "快照者".to_owned(),
        map: "0".to_owned(),
        position: Point::new(10, 10),
        direction: MirDirection::Down,
        level: 10,
        experience: 0,
        max_experience: 0,
        class: MirClass::Warrior,
        gender: MirGender::Male,
        hair: 1,
        hp: 10,
        mp: 10,
        stats: mir_server::combat::player_base_stats(10, MirClass::Warrior),
        action_time: 0,
        attack_time: 0,
        struck_time: 0,
        regen_time: i64::MAX,
        death_time: 0,
        revive_time: 0,
        cell_time: 0,
        step_counter: 0,
        run_counter: 0,
        run_time: 0,
        dead: false,
        attack_mode: AttackMode::Peace,
        pk_points: 0,
        brown_time: 0,
        pk_point_time: 0,
        last_hitter: None,
        last_hit_time: 0,
        name_colour: mir_core::Color(-1),
        visible_monsters: std::collections::BTreeSet::new(),
        visible_npcs: std::collections::BTreeSet::new(),
        visible_items: std::collections::BTreeSet::new(),
        movement_time: 0,
        inventory: mir_server::items::new_inventory(),
        equipment: mir_server::items::new_equipment(),
        storage: mir_server::storage::new_storage(),
        magics: Vec::new(),
        magic: MagicState::default(),
        gold: 0,
        credit: 0,
        quests: Vec::new(),
        completed_quests: std::collections::BTreeSet::new(),
        mail: mir_server::MailState::default(),
        buffs: Vec::new(),
        pot_health_amount: 0,
        pot_mana_amount: 0,
        pot_time: 0,
        unlock_curse: false,
        npc: NpcState::default(),
        group_id: None,
        group_invitation: None,
        group_invite_time: 0,
        allow_group: true,
        guild: None,
        trade: TradeState::default(),
        trade_invitation: None,
        next_trade_time: 0,
        allow_trade: true,
        sender,
    };
    let snapshot =
        mir_server::market::character_snapshot(&base, &player, mir_core::enums::PetMode::Both);
    assert_eq!(
        snapshot.last_daily_reset_at, 1_700_000_000_123,
        "市场/商城的角色行快照必须带上每日重置时间戳"
    );
}

// ---------------------------------------------------------------------------
// 宠物命令模式的登录/下线往返（t44/t45：`characters.pet_mode`，迁移 0001）
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pet_mode_survives_logout_and_relogin() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("绑定临时端口");
    let address = listener.local_addr().expect("取端口");
    let store = Arc::new(MemoryStore::new());
    tokio::spawn(serve_forever(listener, test_config(), Arc::clone(&store)));

    // 第一次进世界：把宠物命令切到 `MoveOnly`（Crystal `C.ChangePMode`）。
    let mut client = enter_world(address, "petuser").await;
    client
        .send(ClientPacket::ChangePMode(c::ChangePMode {
            mode: mir_core::enums::PetMode::MoveOnly,
        }))
        .await;
    // 服务端会无条件回一条 `S.ChangePMode`（`MirConnection.cs:1441-1448`）。
    loop {
        if let ServerPacket::ChangePMode(response) = client.expect().await {
            assert_eq!(response.mode, mir_core::enums::PetMode::MoveOnly);
            break;
        }
    }
    client.send(ClientPacket::LogOut(c::LogOut {})).await;
    expect_logout(&mut client).await;

    // 下线存盘：角色行的 `pet_mode` 应该是 `MoveOnly`（1）。
    let account = store
        .find_account("petuser")
        .await
        .expect("查账号")
        .expect("账号存在");
    let saved = store.list_characters(account.id).await.expect("角色列表");
    let character = saved.first().expect("有一个角色").clone();
    assert_eq!(
        character.pet_mode,
        i16::from(mir_core::enums::PetMode::MoveOnly.value()),
        "下线应把宠物命令写进 characters.pet_mode"
    );

    // 重登再下线：若登录没有恢复存档里的模式，这次快照会把它写回 Both（0）。
    let mut again = login_existing(
        address,
        "petuser",
        i32::try_from(character.id).unwrap_or_default(),
    )
    .await;
    again.send(ClientPacket::LogOut(c::LogOut {})).await;
    expect_logout(&mut again).await;
    let reloaded = store
        .find_character(account.id, character.id)
        .await
        .expect("查角色")
        .expect("角色存在");
    assert_eq!(
        reloaded.pet_mode,
        i16::from(mir_core::enums::PetMode::MoveOnly.value()),
        "重登后应恢复存档里的宠物命令，而不是回落 Both"
    );
}
