//! R50-B：商城（GameShop）真实包链——登录 `S.GameShopInfo` + 购买 `S.GameShopStock`。
//!
//! 这里**不**用静态 fixture 冒充包链：测试自己编码一份最小 `.mnd` 数据包（`db.bin` 段是
//! [`mir_db::schema`] 的原生格式），由真实服务端通过 `GameData::load_mnd` 加载；再用真实
//! TCP 客户端包（`C.ClientVersion` / `C.Login` / `C.StartGame` / `C.GameshopBuy`）走完整会话链，
//! 最后按线格式解码服务端回包、断言 `S.GameShopInfo`（INDEX 250）与 `S.GameShopStock`
//! （INDEX 251）的库存字段。
//!
//! 覆盖的 B2 断言：
//! 1. `Stock != 0` → 发剩余 `Stock - 已购`（全服计数与按角色计数各一条）；
//! 2. 剩余为负 → **不发**（Crystal `Stock - purchased >= 0` 为假）；
//! 3. `Stock == 0`（不限量）→ 发 `StockLevel = 0`；
//! 4. 购买事务提交成功 → 发刷新后的 `S.GameShopStock{StockLevel = 剩余}`，
//!    且发送时机在 `S.LoseCredit` / `S.GainedItem` 之后、落库完成之后（用 `KeepAlive` 屏障验库）。
//!
//! 断言都对着「换成错误实现会红」写：例如把 `Stock - 已购` 写成 `Stock`、把超卖商品也发出去、
//! 把不限量商品写成 `StockLevel = Stock`、或购买后不发/发送旧值，都会让这里失败。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use mir_core::enums::{ItemType, MirClass, MirGender};
use mir_db::mnd::{MndFile, MndSection, TAG_DB_BIN};
use mir_db::{GameShopItem, ItemInfo, MapInfo, MirDb};
use mir_protocol::ServerPacket;
use mir_server::MemoryStore;
use mir_server::store::{
    AccountStore, GameShopPurchaseRecord, GameShopStockRecord, GameShopUpdate,
};

const MAP: &str = "0";
/// 可堆叠药品模板（商城用它测不限量/全服限量）。
const ITEM_POTION: i32 = 100;
/// 不可堆叠武器模板（商城用它测按角色限量）。
const ITEM_SWORD: i32 = 200;

/// 不限量商品（`Stock == 0`）：登录应发 `StockLevel = 0`。
const G_UNLIMITED: i32 = 10;
/// 限量 5、全服计数、已购 2：登录应发 `StockLevel = 3`。
const G_SERVER_LIMITED: i32 = 11;
/// 限量 5、按角色计数、本角色已购 4：登录应发 `StockLevel = 1`。
const G_INDIVIDUAL_LIMITED: i32 = 12;
/// 限量 3、全服已购 5（超卖）：登录**不应发**这条。
const G_OVERSOLD: i32 = 13;
/// 限量 5、全服计数、无已购：登录发 `StockLevel = 5`，买 1 件后发 `StockLevel = 4`。
const G_BUYABLE: i32 = 14;

// ---------------------------------------------------------------------------
// 测试脚手架：临时目录 + 最小 .mnd 数据包
// ---------------------------------------------------------------------------

/// 每个用例一个独立的临时目录（放 `.mnd`），退出时删除。
struct TempData {
    root: PathBuf,
}

impl TempData {
    fn new(tag: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("mir-next-r50b-wire-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("建临时目录");
        Self { root }
    }

    /// 把数据库编码成真实 `.mnd` 文件（走服务端 `load_mnd` 的生产加载路径），返回路径。
    fn write_mnd(&self, db: &MirDb) -> PathBuf {
        let payload = mir_db::schema::encode_db(db).expect("编码 db.bin");
        let mut mnd = MndFile::new();
        mnd.push(MndSection::new(TAG_DB_BIN, "db.bin", payload));
        let bytes = mnd.to_bytes().expect("编码 .mnd");
        let path = self.root.join("game.mnd");
        std::fs::write(&path, bytes).expect("写 .mnd");
        path
    }
}

impl Drop for TempData {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn item(index: i32, name: &str, item_type: ItemType, stack_size: u16) -> ItemInfo {
    ItemInfo {
        index,
        name: name.to_owned(),
        item_type,
        image: u16::try_from(index).unwrap_or(0),
        durability: 1_000,
        stack_size,
        price: 10,
        ..ItemInfo::default()
    }
}

/// 商城商品：`stock` 为 0 表示不限量；`i_stock` 为真时按角色计数。
fn product(
    item_index: i32,
    g_index: i32,
    stock: i32,
    i_stock: bool,
    credit_price: u32,
) -> GameShopItem {
    GameShopItem {
        item_index,
        g_index,
        credit_price,
        gold_price: credit_price.saturating_mul(10),
        count: 1,
        class: "All".to_owned(),
        category: "Potion".to_owned(),
        stock,
        i_stock,
        can_buy_credit: true,
        can_buy_gold: true,
        ..GameShopItem::default()
    }
}

/// 最小数据库：2 件物品、1 张图、5 件商城商品（覆盖六条库存断言的全部形态）。
fn database() -> MirDb {
    MirDb {
        items: vec![
            item(ITEM_POTION, "金创药", ItemType::Potion, 20),
            item(ITEM_SWORD, "木剑", ItemType::Weapon, 1),
        ],
        maps: vec![MapInfo {
            index: 1,
            file_name: MAP.to_owned(),
            title: "比奇省".to_owned(),
            ..MapInfo::default()
        }],
        gameshop: vec![
            product(ITEM_POTION, G_UNLIMITED, 0, false, 10),
            product(ITEM_POTION, G_SERVER_LIMITED, 5, false, 10),
            product(ITEM_SWORD, G_INDIVIDUAL_LIMITED, 5, true, 100),
            product(ITEM_POTION, G_OVERSOLD, 3, false, 10),
            product(ITEM_POTION, G_BUYABLE, 5, false, 10),
        ],
        ..MirDb::default()
    }
}

/// 建号（战士 10 级、1000 元宝）并预置商城已购计数：
/// `G_SERVER_LIMITED` 全服 2 件、`G_INDIVIDUAL_LIMITED` 本角色 4 件、`G_OVERSOLD` 全服 5 件（超卖）。
async fn seeded_store(tag: &str) -> (Arc<MemoryStore>, mir_server::store::Account, i64) {
    let store = Arc::new(MemoryStore::new());
    let account = store.create_account(tag, "secret", "").await.expect("建号");
    let mut character = store
        .create_character(
            account.id,
            "商城顾客",
            MirClass::Warrior,
            MirGender::Male,
            MAP,
            (330, 270),
        )
        .await
        .expect("建角");
    character.level = 10;
    character.credit = 1_000;
    store
        .save_character(character.clone())
        .await
        .expect("存角色");
    store
        .apply_gameshop_update(GameShopUpdate {
            stock: vec![
                GameShopStockRecord {
                    g_index: G_SERVER_LIMITED,
                    purchased: 2,
                },
                GameShopStockRecord {
                    g_index: G_OVERSOLD,
                    purchased: 5,
                },
            ],
            purchases: vec![GameShopPurchaseRecord {
                character_id: character.id,
                g_index: G_INDIVIDUAL_LIMITED,
                quantity: 4,
            }],
            ..GameShopUpdate::default()
        })
        .await
        .expect("预置已购计数");
    (store, account, character.id)
}

fn config_for(db_path: &std::path::Path) -> Arc<mir_server::ServerConfig> {
    Arc::new(mir_server::ServerConfig {
        map_root: "resource/不存在的目录".to_owned(),
        language_dir: "config/不存在的目录".to_owned(),
        load_game_data: true,
        db_path: Some(db_path.to_string_lossy().into_owned()),
        ..mir_server::ServerConfig::default()
    })
}

/// 起一个真实服务端，返回客户端连接地址。
async fn start_server(db_path: &std::path::Path, store: Arc<MemoryStore>) -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("绑定临时端口");
    let address = listener.local_addr().expect("取端口");
    tokio::spawn(mir_server::serve_forever(
        listener,
        config_for(db_path),
        store,
    ));
    address
}

// ---------------------------------------------------------------------------
// 真实 TCP 客户端（与 `gameshop.rs` 的 TestClient 同构：登录包序 + 宽容读包）
// ---------------------------------------------------------------------------

struct TestClient {
    stream: tokio::net::TcpStream,
    frames: mir_protocol::codec::FrameBuffer,
}

impl TestClient {
    async fn connect(address: std::net::SocketAddr) -> Self {
        let stream = tokio::net::TcpStream::connect(address)
            .await
            .expect("连接服务端");
        stream.set_nodelay(true).ok();
        Self {
            stream,
            frames: mir_protocol::codec::FrameBuffer::new(),
        }
    }

    async fn send(&mut self, packet: mir_protocol::ClientPacket) {
        use tokio::io::AsyncWriteExt;
        self.stream
            .write_all(&packet.encode_frame())
            .await
            .expect("发送");
    }

    async fn expect(&mut self) -> ServerPacket {
        use tokio::io::AsyncReadExt;
        for _ in 0..40 {
            if let Some((index, payload)) = self.frames.next_frame().expect("帧格式") {
                return ServerPacket::decode(index, &payload).expect("解析服务端包");
            }
            let mut chunk = [0u8; 4096];
            let read = tokio::time::timeout(
                std::time::Duration::from_secs(10),
                self.stream.read(&mut chunk),
            )
            .await
            .expect("读取超时")
            .expect("读取");
            assert!(read > 0, "服务端提前关闭连接");
            self.frames.push(&chunk[..read]);
        }
        panic!("等待服务端包超时");
    }

    async fn expect_index(&mut self, index: i16) -> ServerPacket {
        // 登录序列里还有好友列表（`S.FriendUpdate`），与本用例关心的包序无关，跳过它。
        let mut packet = self.expect().await;
        while matches!(packet, ServerPacket::FriendUpdate(_)) {
            packet = self.expect().await;
        }
        assert_eq!(packet.index(), index, "收到的包编号不符：{packet:?}");
        packet
    }

    /// 读到指定编号的包为止，**忽略**期间收到的其它包。
    async fn expect_until(&mut self, index: i16) -> ServerPacket {
        for _ in 0..200 {
            let packet = self.expect().await;
            if packet.index() == index {
                return packet;
            }
        }
        panic!("等待包 {index} 超时（已读 200 个其它包）");
    }
}

/// 走完版本 → 登录 → 进世界（与真实客户端同一条会话代码路径）。
async fn logged_in(client: &mut TestClient, account: &str, character_id: i64) {
    use mir_protocol::packets::client as c;
    use mir_protocol::packets::server as s;

    client.expect_index(s::Connected::INDEX).await;
    client
        .send(mir_protocol::ClientPacket::ClientVersion(
            c::ClientVersion {
                version_hash: Vec::new(),
            },
        ))
        .await;
    client.expect_index(s::ClientVersion::INDEX).await;
    client
        .send(mir_protocol::ClientPacket::Login(c::Login {
            account_id: account.to_owned(),
            password: "secret".to_owned(),
        }))
        .await;
    client.expect_index(s::LoginSuccess::INDEX).await;
    client
        .send(mir_protocol::ClientPacket::StartGame(c::StartGame {
            character_index: i32::try_from(character_id).unwrap_or(0),
        }))
        .await;
    client.expect_index(s::StartGame::INDEX).await;
    let info = match client.expect_until(s::UserInformation::INDEX).await {
        ServerPacket::UserInformation(info) => info,
        other => panic!("期望 UserInformation，收到 {other:?}"),
    };
    assert!(info.object_id > 0, "进世界应分配对象 ID");
}

// ---------------------------------------------------------------------------
// 断言 1–3：登录链 GetGameShop 逐商品发 S.GameShopInfo
// ---------------------------------------------------------------------------

/// 登录后按 `S.GameShopList` 顺序逐商品下发 `S.GameShopInfo`，且库存语义照抄 Crystal
/// `PlayerObject.GetGameShop`（`PlayerObject.cs:13913-13944`）：
/// `Stock != 0` 发 `Stock - 已购`（全服/按角色两种计数），剩余为负不发，`Stock == 0` 发 0。
#[tokio::test]
async fn login_sends_gameshop_info_with_crystal_stock_rules() {
    let data = TempData::new("login");
    let (store, account, character_id) = seeded_store("login").await;
    let db_path = data.write_mnd(&database());
    let address = start_server(&db_path, store).await;

    let mut client = TestClient::connect(address).await;
    logged_in(&mut client, &account.username, character_id).await;

    // `S.GameShopInfo` 紧跟 `UserInformation` + 物品模板之后、`ChangeAMode` 之前。
    // 读到 `ChangeAMode` 为止，把这段登录链里的全部 `GameShopInfo` 收集起来。
    let mut info_packets = Vec::new();
    for _ in 0..200 {
        match client.expect().await {
            ServerPacket::GameShopInfo(info) => info_packets.push(info),
            ServerPacket::ChangeAMode(_) => break,
            _ => {}
        }
    }

    let by_g_index: HashMap<i32, i32> = info_packets
        .iter()
        .map(|info| (info.item.g_index, info.stock_level))
        .collect();
    assert_eq!(
        info_packets.len(),
        4,
        "5 件商品里超卖的 {G_OVERSOLD} 不下发，其余 4 件各一条；实际：{by_g_index:?}"
    );
    assert_eq!(
        by_g_index.get(&G_UNLIMITED),
        Some(&0),
        "Stock == 0（不限量）发 StockLevel = 0"
    );
    assert_eq!(
        by_g_index.get(&G_SERVER_LIMITED),
        Some(&3),
        "限量 5 - 全服已购 2 = 剩余 3"
    );
    assert_eq!(
        by_g_index.get(&G_INDIVIDUAL_LIMITED),
        Some(&1),
        "限量 5 - 本角色已购 4 = 剩余 1（iStock 按角色计数）"
    );
    assert_eq!(
        by_g_index.get(&G_BUYABLE),
        Some(&5),
        "限量 5、无已购 = 剩余 5"
    );
    assert!(
        !by_g_index.contains_key(&G_OVERSOLD),
        "剩余 3 - 已购 5 = -2：Crystal 的 Stock - purchased >= 0 为假，不下发超卖商品"
    );

    // 顺序 = `GameShopList` 顺序（Crystal 逐条 Enqueue）。
    let order: Vec<i32> = info_packets.iter().map(|info| info.item.g_index).collect();
    assert_eq!(
        order,
        vec![
            G_UNLIMITED,
            G_SERVER_LIMITED,
            G_INDIVIDUAL_LIMITED,
            G_BUYABLE
        ],
        "应严格按 GameShopList 顺序逐件下发"
    );
    // 包里的 `Item` 是商品原样（`Stock` 保持数据库值），库存只走 `StockLevel`——
    // 客户端 Crystal 的 `GameShopUpdate` 会把 `Item.Stock = StockLevel` 再展示。
    let limited = info_packets
        .iter()
        .find(|info| info.item.g_index == G_SERVER_LIMITED)
        .expect("限量商品应在列表里");
    assert_eq!(limited.item.stock, 5, "Item.Stock 是数据库原值 5");
    assert_eq!(limited.stock_level, 3, "剩余库存只在 StockLevel 里");
}

// ---------------------------------------------------------------------------
// 断言 4：购买成功后发刷新后的 S.GameShopStock
// ---------------------------------------------------------------------------

/// `C.GameshopBuy` 购买限量商品成功 → 先 `S.LoseCredit`/`S.GainedItem`，事务提交成功后
/// 再发 `S.GameShopStock{GIndex, StockLevel = 剩余}`；用 `KeepAlive` 屏障确认库存已落库、
/// 且只发一次。
#[tokio::test]
async fn successful_buy_sends_refreshed_gameshop_stock() {
    use mir_protocol::packets::client as c;

    let data = TempData::new("buy");
    let (store, account, character_id) = seeded_store("buy").await;
    let db_path = data.write_mnd(&database());
    let address = start_server(&db_path, store.clone()).await;

    let mut client = TestClient::connect(address).await;
    logged_in(&mut client, &account.username, character_id).await;

    client
        .send(mir_protocol::ClientPacket::GameshopBuy(c::GameshopBuy {
            g_index: G_BUYABLE,
            quantity: 1,
            p_type: 0,
        }))
        .await;

    // 登录链剩下的包（GameShopInfo/SwitchGroup/任务/欢迎语…）与后面的回包混在一起，
    // 这里按类型收集：扣款、到货、库存刷新。
    let mut lose_credit = None;
    let mut gained_items = 0usize;
    let mut stock_packet = None;
    for _ in 0..200 {
        match client.expect().await {
            ServerPacket::LoseCredit(lose) => lose_credit = Some(lose.credit),
            ServerPacket::GainedItem(_) => gained_items += 1,
            ServerPacket::GameShopStock(stock) => {
                stock_packet = Some(stock);
                break;
            }
            _ => {}
        }
    }
    assert_eq!(lose_credit, Some(10), "扣的元宝 = 商品 CreditPrice");
    assert!(gained_items > 0, "购买成功应发 S.GainedItem");
    let stock = stock_packet.expect("购买成功必须发 S.GameShopStock");
    assert_eq!(
        stock.g_index, G_BUYABLE,
        "GIndex 必须是商品 GIndex（中书决策 1：不用 Crystal 的 Info.Index）"
    );
    assert_eq!(
        stock.stock_level, 4,
        "限量 5 - 本次购买 1 = 剩余 4，必须发刷新后的值"
    );

    // KeepAlive 屏障：会话按包序处理，收到回包时商城事务已提交；
    // 同时确认购买只发一次库存包（不会在后续包流里重复）。
    client
        .send(mir_protocol::ClientPacket::KeepAlive(c::KeepAlive {
            time: 1,
        }))
        .await;
    let mut extra_stock_packets = 0usize;
    for _ in 0..40 {
        match client.expect().await {
            ServerPacket::GameShopStock(_) => extra_stock_packets += 1,
            ServerPacket::KeepAlive(_) => break,
            _ => {}
        }
    }
    assert_eq!(extra_stock_packets, 0, "一次购买只发一次库存刷新");

    // 预置计数（11 全服 2、13 超卖 5）必须原样保留；本次购买只新增 14 一行。
    let stored = store.gameshop_stock().await;
    assert_eq!(
        stored,
        vec![
            GameShopStockRecord {
                g_index: G_SERVER_LIMITED,
                purchased: 2,
            },
            GameShopStockRecord {
                g_index: G_OVERSOLD,
                purchased: 5,
            },
            GameShopStockRecord {
                g_index: G_BUYABLE,
                purchased: 1,
            },
        ],
        "库存计数应已提交落库，且不能覆盖预置行"
    );
    assert_eq!(
        store.gameshop_purchases().await,
        vec![GameShopPurchaseRecord {
            character_id,
            g_index: G_INDIVIDUAL_LIMITED,
            quantity: 4,
        }],
        "全服计数商品不写按角色计数，预置行不能丢"
    );
}

// ---------------------------------------------------------------------------
// 断言 1 的正面计数：真实样本库登录到底发了几条 S.GameShopInfo
// ---------------------------------------------------------------------------

/// 真实样本库绝对路径（Crystal 官方 Jev 库，与 `gameshop.rs` 用的是同一份）。
fn sample_db() -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = root.join("resource/community/suprcode-crystal-database/Jev/Server.MirDB");
    if path.is_file() {
        std::fs::canonicalize(&path).ok().or(Some(path))
    } else {
        None
    }
}

/// 真实样本库（Jev）登录链的**正面计数**断言：`S.GameShopInfo` 条数 == 商城目录条数，
/// `GIndex`/`StockLevel` 与 Crystal `GetGameShop` 语义逐条一致。
///
/// 为什么需要它：`gameshop.rs` 的真实链路用例为兼容进世界序列的增长用 `expect_until`
/// 跳过其它包（容差常量），它能证明「买成功了」，却看不见登录到底发了几条商城包；
/// 这里把「逐商品各一条、不重不漏」钉死：漏发/重发/被 `Stock - 已购 >= 0` 规则误伤、
/// 把不限量商品的 `StockLevel` 写成 `Stock` 都会让这条用例红。
#[tokio::test]
async fn real_database_login_sends_one_gameshop_info_per_product() {
    let Some(db_path) = sample_db() else {
        println!("跳过 real_database_login_sends_one_gameshop_info_per_product：样本库不存在");
        return;
    };

    // 期望值不硬编码：按服务端同一条加载路径（`GameData::load` → `MirDb::load`，
    // 兼容 Crystal 原始 `Server.MirDB` 与 `.mnd`）读同一个库，数出商城目录条数。
    let mirrored = MirDb::load(&db_path).expect("加载样本库");
    // 新角色没有任何已购计数：`Stock != 0` 时剩余 = Stock，`Stock == 0` 发 0。
    // 因此会下发的商品 = `Stock >= 0` 的商品，`StockLevel` 恒等于 `Stock`（含 0）。
    let expected: Vec<(i32, i32)> = mirrored
        .gameshop
        .iter()
        .filter(|product| product.stock >= 0)
        .map(|product| (product.g_index, product.stock))
        .collect();
    assert!(!expected.is_empty(), "样本库应带商城目录");

    let store = Arc::new(MemoryStore::new());
    let account = store
        .create_account("realcount", "secret", "")
        .await
        .expect("建号");
    let character = store
        .create_character(
            account.id,
            "真实库顾客",
            MirClass::Warrior,
            MirGender::Male,
            MAP,
            (330, 270),
        )
        .await
        .expect("建角");
    let address = start_server(&db_path, store).await;

    let mut client = TestClient::connect(address).await;
    logged_in(&mut client, &account.username, character.id).await;

    // `UserInformation` 已被 `logged_in` 消费；登录链在 `GotUserInformation` 之后、
    // `ChangeAMode` 之前恰好下发一遍 `S.GameShopInfo`，这里逐包读到 `ChangeAMode` 为止。
    let mut received: Vec<(i32, i32)> = Vec::new();
    let mut saw_change_amode = false;
    for _ in 0..(expected.len() + 1_024) {
        match client.expect().await {
            ServerPacket::GameShopInfo(info) => {
                received.push((info.item.g_index, info.stock_level));
            }
            ServerPacket::ChangeAMode(_) => {
                saw_change_amode = true;
                break;
            }
            _ => {}
        }
    }
    assert!(saw_change_amode, "登录链应继续到 ChangeAMode");
    assert_eq!(
        received.len(),
        expected.len(),
        "登录链必须逐商品各发一条 S.GameShopInfo（条数 = 商城目录条数）"
    );
    assert_eq!(
        received, expected,
        "GIndex/StockLevel 必须与 Crystal GetGameShop 语义一致，且顺序 = 目录顺序"
    );
}
