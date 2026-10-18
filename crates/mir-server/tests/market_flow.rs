//! 市场链路的端到端协议测试：真的起一个服务端，用真的 TCP 连接把「搜索 → 翻页 → 刷新 →
//! 买不存在的挂单 → 取回不存在的挂单 → 提前成交不存在的拍卖 → 上架被拒」走一遍。
//!
//! 为什么单独要有这个测试：市场（寄售/拍卖）曾经只有 `World` 层方法、`session.rs` 里
//! **一个分派都没有**，客户端发上来的包全部落进 `_ =>` 被静默忽略，而单元测试仍然全绿。
//! 这里的每条断言都是「客户端视角收到的服务端包」，接线一断就会立刻变红。
//!
//! 用商城页签（`MarketPanelType::GameShop`）打开市场：按 Crystal 的语义它会跳过
//! 「必须站在 NPC 前」的检查（`PlayerObject.cs:8385-8390`），测试世界不需要铺 NPC。
//! 非商城页签的路径（必须 `[@MARKET]` 页 + NPC 在范围内）由 `market.rs` 的单元测试覆盖。

use std::sync::Arc;
use std::time::Duration;

use mir_core::enums::{
    ItemType, MarketCollectionMode, MarketPanelType, MirClass, MirDirection, MirGender,
};
use mir_protocol::codec::FrameBuffer;
use mir_protocol::packets::client as c;
use mir_protocol::packets::server as s;
use mir_protocol::{ClientPacket, ServerPacket};
use mir_server::{MemoryStore, ServerConfig, serve_forever};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// `MarketFailReason::TooFar`：搜索结果里没有这条挂单 / 找不到 NPC。
const MARKET_FAIL_TOO_FAR: u8 = 7;
/// `MarketFailReason::NoDialog`：没有通过 NPC 打开市场面板（`PlayerObject.cs:8489-8493`）。
const MARKET_FAIL_NO_DIALOG: u8 = 1;

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
        // 登录序列里还有好友列表（`S.FriendUpdate`，Crystal `GetFriends()`），
        // 与本文件关心的包序无关，跳过它。
        let mut packet = self.expect().await;
        while matches!(packet, ServerPacket::FriendUpdate(_)) {
            packet = self.expect().await;
        }
        assert_eq!(packet.index(), index, "收到的包编号不符：{packet:?}");
        packet
    }

    /// 走完「连上 → 版本 → 注册 → 登录 → 建角 → 进世界」，把进世界那串包读完，
    /// 停在欢迎语之后。返回 `UserInformation.ObjectID`。
    async fn enter_world(&mut self) -> u32 {
        // 1. 连上先收到 Connected（`MirConnection.cs:97`）。
        self.expect_index(s::Connected::INDEX).await;

        // 2. 版本校验：回 Result = 1。
        self.send(ClientPacket::ClientVersion(c::ClientVersion {
            version_hash: vec![0u8; 16],
        }))
        .await;
        let packet = self.expect_index(s::ClientVersion::INDEX).await;
        match packet {
            ServerPacket::ClientVersion(response) => assert_eq!(response.result, 1),
            other => panic!("期望 ClientVersion，收到 {other:?}"),
        }

        // 3. 注册：成功码 8。
        self.send(ClientPacket::NewAccount(c::NewAccount {
            account_id: "market".to_owned(),
            password: "secret".to_owned(),
            birth_date: mir_core::DateTimeBinary(0),
            user_name: String::new(),
            secret_question: String::new(),
            secret_answer: String::new(),
            e_mail_address: String::new(),
        }))
        .await;
        let packet = self.expect_index(s::NewAccount::INDEX).await;
        match packet {
            ServerPacket::NewAccount(response) => assert_eq!(response.result, 8),
            other => panic!("期望 NewAccount，收到 {other:?}"),
        }

        // 4. 登录。
        self.send(ClientPacket::Login(c::Login {
            account_id: "market".to_owned(),
            password: "secret".to_owned(),
        }))
        .await;
        let packet = self.expect_index(s::LoginSuccess::INDEX).await;
        let characters = match packet {
            ServerPacket::LoginSuccess(success) => success.characters,
            other => panic!("期望 LoginSuccess，收到 {other:?}"),
        };
        assert!(characters.is_empty(), "新账号没有角色");

        // 5. 建角色。
        self.send(ClientPacket::NewCharacter(c::NewCharacter {
            name: "行商客".to_owned(),
            gender: MirGender::Male,
            class: MirClass::Warrior,
        }))
        .await;
        let packet = self.expect_index(s::NewCharacterSuccess::INDEX).await;
        let character = match packet {
            ServerPacket::NewCharacterSuccess(success) => success.char_info,
            other => panic!("期望 NewCharacterSuccess，收到 {other:?}"),
        };

        // 6. 进世界：StartGame → MapInformation → UserInformation → … → 欢迎语。
        self.send(ClientPacket::StartGame(c::StartGame {
            character_index: character.index,
        }))
        .await;
        let packet = self.expect_index(s::StartGame::INDEX).await;
        match packet {
            ServerPacket::StartGame(response) => assert_eq!(response.result, 4, "4 = 成功"),
            other => panic!("期望 StartGame，收到 {other:?}"),
        }

        let mut object_id = None;
        // 进世界序列的包数随功能增减变化，这里按「读到欢迎语为止」消费，最多 20 个。
        for _ in 0..20 {
            let packet = self.expect().await;
            match packet {
                ServerPacket::UserInformation(info) => object_id = Some(info.object_id),
                ServerPacket::Chat(chat) => {
                    assert!(chat.message.contains("欢迎"), "进世界欢迎语");
                    break;
                }
                _ => {}
            }
        }
        object_id.expect("进世界序列里应有 UserInformation")
    }
}

fn test_config() -> Arc<ServerConfig> {
    Arc::new(ServerConfig {
        // 测试不依赖 resource/ 下的真实地图与数据库。
        map_root: "resource/不存在的目录".to_owned(),
        load_game_data: false,
        ..ServerConfig::default()
    })
}

async fn start_server() -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("绑定临时端口");
    let address = listener.local_addr().expect("取端口");
    let config = test_config();
    let store = Arc::new(MemoryStore::new());
    tokio::spawn(serve_forever(listener, config, store));
    address
}

/// 市场搜索（商城页签）：服务端必须回 `S.NPCMarket`，而不是把包吞掉。
#[tokio::test]
async fn market_search_round_trips_over_the_wire() {
    let address = start_server().await;
    let mut client = TestClient::connect(address).await;
    client.enter_world().await;

    let search = ClientPacket::MarketSearch(c::MarketSearch {
        match_text: String::new(),
        item_type: ItemType::Nothing,
        usermode: false,
        min_shape: 0,
        max_shape: 0,
        market_type: MarketPanelType::GameShop,
    });

    // `C.MarketSearch`（71）→ `S.NPCMarket`（155）：本作没有商城数据，列表为空、
    // 页码按 `(count - 1) / 10 + 1` 算出来是 1。
    client.send(search).await;
    let packet = client.expect_index(s::NPCMarket::INDEX).await;
    match packet {
        ServerPacket::NPCMarket(market) => {
            assert!(market.listings.is_empty(), "没有挂单/商城数据");
            assert_eq!(market.pages, 1);
            assert!(!market.user_mode, "默认不是按玩家名搜索");
        }
        other => panic!("期望 NPCMarket，收到 {other:?}"),
    }
}

/// 翻页与刷新：都必须有回包。
#[tokio::test]
async fn market_page_and_refresh_round_trip() {
    let address = start_server().await;
    let mut client = TestClient::connect(address).await;
    client.enter_world().await;

    client
        .send(ClientPacket::MarketSearch(c::MarketSearch {
            match_text: String::new(),
            item_type: ItemType::Nothing,
            usermode: false,
            min_shape: 0,
            max_shape: 0,
            market_type: MarketPanelType::GameShop,
        }))
        .await;
    client.expect_index(s::NPCMarket::INDEX).await;

    // 翻页同样要过 500ms 节流（`SEARCH_DELAY_MS`，`Globals.SearchDelay`，`Globals.cs:41`）：
    // `market_page` 先比 `SearchTime` 再判面板（`PlayerObject.cs:8327-8374`）。
    tokio::time::sleep(Duration::from_millis(700)).await;

    // `C.MarketPage`（73）→ `S.NPCMarketPage`（156）。商城页签不受「页码必须递增」限制，
    // 第 2 页为空列表（`PlayerObject.cs:8356-8360`）。
    client
        .send(ClientPacket::MarketPage(c::MarketPage { page: 1 }))
        .await;
    let packet = client.expect_index(s::NPCMarketPage::INDEX).await;
    match packet {
        ServerPacket::NPCMarketPage(page) => assert!(page.listings.is_empty()),
        other => panic!("期望 NPCMarketPage，收到 {other:?}"),
    }

    // `C.MarketRefresh`（72）→ `S.NPCMarket`（155）。刷新有 500ms 节流
    // （`Globals.SearchDelay`，`Globals.cs:41`），等过它再断言。
    tokio::time::sleep(Duration::from_millis(700)).await;
    client
        .send(ClientPacket::MarketRefresh(c::MarketRefresh {}))
        .await;
    client.expect_index(s::NPCMarket::INDEX).await;
}

/// 买/取回/提前成交一条不存在的挂单：服务端必须回 `S.MarketFail{TooFar}`，
/// 而不是静默丢包——这条断言是「市场接线断了」最直接的报警器。
#[tokio::test]
async fn market_operations_on_missing_listing_report_failure() {
    let address = start_server().await;
    let mut client = TestClient::connect(address).await;
    client.enter_world().await;

    // 先搜索一次，让服务端记住「当前面板 = 商城」，后面三个操作才跳过 NPC 检查
    // （`PlayerObject.cs:8489-8500`）。
    client
        .send(ClientPacket::MarketSearch(c::MarketSearch {
            match_text: String::new(),
            item_type: ItemType::Nothing,
            usermode: false,
            min_shape: 0,
            max_shape: 0,
            market_type: MarketPanelType::GameShop,
        }))
        .await;
    client.expect_index(s::NPCMarket::INDEX).await;

    // `C.MarketBuy`（74）→ `S.MarketFail`（158）：搜索结果里没有这条 ID（`:8502`）。
    client
        .send(ClientPacket::MarketBuy(c::MarketBuy {
            auction_id: 424_242,
            bid_price: 0,
        }))
        .await;
    let packet = client.expect_index(s::MarketFail::INDEX).await;
    match packet {
        ServerPacket::MarketFail(fail) => assert_eq!(fail.reason, MARKET_FAIL_TOO_FAR),
        other => panic!("期望 MarketFail，收到 {other:?}"),
    }

    // `C.MarketGetBack{Mode=Any}`（75）：取回**必须**先通过 `[@MARKET]` 的 NPC 面板，
    // 这里没和 NPC 对话过，所以按 Crystal `:8737-8747` 回 `NoDialog`（1），
    // 而不是「挂单不存在」。这条断言同时证明包 75 真的被分派了。
    client
        .send(ClientPacket::MarketGetBack(c::MarketGetBack {
            mode: MarketCollectionMode::Any,
            auction_id: 424_242,
        }))
        .await;
    let packet = client.expect_index(s::MarketFail::INDEX).await;
    match packet {
        ServerPacket::MarketFail(fail) => assert_eq!(fail.reason, MARKET_FAIL_NO_DIALOG),
        other => panic!("期望 MarketFail，收到 {other:?}"),
    }

    // `C.MarketSellNow`（76）：同一条 NPC 面板前置（`:8599-8609`）。
    client
        .send(ClientPacket::MarketSellNow(c::MarketSellNow {
            auction_id: 424_242,
        }))
        .await;
    let packet = client.expect_index(s::MarketFail::INDEX).await;
    match packet {
        ServerPacket::MarketFail(fail) => assert_eq!(fail.reason, MARKET_FAIL_NO_DIALOG),
        other => panic!("期望 MarketFail，收到 {other:?}"),
    }

    // 三个操作都不该改变角色在世界的状态：还能正常走路（服务端回 UserLocation）。
    client
        .send(ClientPacket::Walk(c::Walk {
            direction: MirDirection::Right,
        }))
        .await;
    let packet = client.expect_index(s::UserLocation::INDEX).await;
    match packet {
        ServerPacket::UserLocation(location) => {
            assert_eq!(location.direction, MirDirection::Right);
        }
        other => panic!("期望 UserLocation，收到 {other:?}"),
    }
}

/// 上架：没有和 NPC 对话过时，必须回 `S.ConsignItem{Success=false}`。
#[tokio::test]
async fn consign_without_npc_dialog_is_rejected() {
    let address = start_server().await;
    let mut client = TestClient::connect(address).await;
    client.enter_world().await;

    // `C.ConsignItem`（70）→ `S.ConsignItem`（157），`Success = false`
    // （Crystal `PlayerObject.cs:8222-8226`：`NPCPage == null`）。
    client
        .send(ClientPacket::ConsignItem(c::ConsignItem {
            unique_id: 1,
            price: 5_000,
            type_: MarketPanelType::Consign,
        }))
        .await;
    let packet = client.expect_index(s::ConsignItem::INDEX).await;
    match packet {
        ServerPacket::ConsignItem(ack) => {
            assert!(!ack.success, "没和 NPC 对话不能上架");
            assert_eq!(ack.unique_id, 1);
        }
        other => panic!("期望 ConsignItem，收到 {other:?}"),
    }
}
