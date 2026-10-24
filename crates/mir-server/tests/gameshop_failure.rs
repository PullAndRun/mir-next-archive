//! R50-B：商城失败/回滚路径的「零库存包」断言（会话层，与真实连接同一条代码路径）。
//!
//! `Session::new` + `MemoryStore` 注入一次 `apply_gameshop_update` 事务失败，验证：
//! 1. **失败零包**：落盘失败时一个 `S.GameShopStock` 都不发（不能出现「库存减了但没扣款」）；
//!    同时按已有语义退款（`S.GainedCredit`）、提示玩家（Chat 含「失败」）、库里一行未写；
//! 2. **回滚不刷**：失败回滚后库存计数仍是完整值，紧接着重试购买成功，
//!    只降一次库存（`StockLevel = 4`）且只落一行计数。

use std::sync::Arc;

use mir_core::enums::{ItemType, MirClass, MirGender};
use mir_db::{GameShopItem, ItemInfo, MapInfo, MirDb};
use mir_protocol::ServerPacket;
use mir_server::MemoryStore;
use mir_server::gamedata::GameData;
use mir_server::store::AccountStore;
use mir_server::world::World;
use tokio::sync::mpsc;

const MAP: &str = "0";
/// 药品模板：不限量商品用它（`Stock == 0` → 登录发 0、购买后不发库存包）。
const ITEM_POTION: i32 = 100;
/// 武器模板：限量商品用它（`Stock != 0` → 购买成功发刷新后的库存）。
const ITEM_SWORD: i32 = 200;
/// 不限量商品（`Stock == 0`）。
const G_POTION: i32 = 1;
/// 限量 5、按角色计数（`iStock == true`）、限战士。
const G_SWORD: i32 = 2;

// ---------------------------------------------------------------------------
// 测试脚手架
// ---------------------------------------------------------------------------

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

fn product(item_index: i32, g_index: i32, credit_price: u32) -> GameShopItem {
    GameShopItem {
        item_index,
        g_index,
        credit_price,
        gold_price: credit_price.saturating_mul(10),
        count: 1,
        class: "All".to_owned(),
        category: "Potion".to_owned(),
        can_buy_credit: true,
        can_buy_gold: true,
        ..GameShopItem::default()
    }
}

/// 最小数据库：2 件物品、1 张图、2 件商城商品（不限量药品 + 限量 5 件木剑）。
fn database() -> MirDb {
    MirDb {
        items: vec![
            item(ITEM_POTION, "金创药", ItemType::Potion, 20),
            ItemInfo {
                required_class: mir_core::enums::RequiredClass::Warrior,
                ..item(ITEM_SWORD, "木剑", ItemType::Weapon, 1)
            },
        ],
        maps: vec![MapInfo {
            index: 1,
            file_name: MAP.to_owned(),
            title: "比奇省".to_owned(),
            ..MapInfo::default()
        }],
        gameshop: vec![
            product(ITEM_POTION, G_POTION, 10),
            GameShopItem {
                stock: 5,
                i_stock: true,
                ..product(ITEM_SWORD, G_SWORD, 100)
            },
        ],
        ..MirDb::default()
    }
}

/// 临时目录（`GameData::from_db` 的掉落目录父目录），退出时删除。
struct Envir {
    root: std::path::PathBuf,
}

impl Envir {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "mir-next-r50b-failure-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("建临时目录");
        Self { root }
    }

    fn drop_dir(&self) -> std::path::PathBuf {
        self.root.join("Drops")
    }
}

impl Drop for Envir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// 把会话输出帧解码成服务端包（帧格式 = u16 长度 + i16 包编号 + 载荷）。
fn drain(out_rx: &mut mpsc::UnboundedReceiver<Vec<u8>>) -> Vec<ServerPacket> {
    let mut packets = Vec::new();
    while let Ok(frame) = out_rx.try_recv() {
        let index = i16::from_le_bytes([frame[2], frame[3]]);
        packets.push(ServerPacket::decode(index, &frame[4..]).expect("解码服务端包"));
    }
    packets
}

// ---------------------------------------------------------------------------
// 失败零包 + 回滚不刷
// ---------------------------------------------------------------------------

/// 落盘失败 → 不发 `S.GameShopStock`；退款、提示、库零写入；重试成功只降一次库存。
#[tokio::test]
async fn failed_flush_sends_no_stock_packet_and_retry_decrements_once() {
    use mir_protocol::packets::client as c;

    let store = Arc::new(MemoryStore::new());
    let account = store
        .create_account("failure", "secret", "")
        .await
        .expect("建号");
    let mut character = store
        .create_character(
            account.id,
            "失败回滚者",
            MirClass::Warrior,
            MirGender::Male,
            MAP,
            (100, 101),
        )
        .await
        .expect("建角");
    character.level = 10;
    character.credit = 1_000;
    store
        .save_character(character.clone())
        .await
        .expect("存角色");

    let envir = Envir::new("zero-packet");
    let mut world = World::new(999);
    world.set_game_data(Arc::new(GameData::from_db(
        database(),
        Some(envir.drop_dir()),
    )));
    let world = Arc::new(tokio::sync::Mutex::new(world));

    let config = Arc::new(mir_server::ServerConfig {
        map_root: "resource/不存在的目录".to_owned(),
        language_dir: "config/不存在的目录".to_owned(),
        load_game_data: false,
        ..mir_server::ServerConfig::default()
    });
    let (out, mut out_rx) = mpsc::unbounded_channel();
    let mut session = mir_server::session::Session::new(
        1,
        "test".to_owned(),
        config,
        Arc::clone(&store),
        Arc::clone(&world),
        out,
        Arc::new(mir_core::i18n::LanguageLibrary::embedded()),
    );
    // 走完版本 → 登录 → 进世界（与真实连接同一条代码路径）。
    session
        .handle(mir_protocol::ClientPacket::ClientVersion(
            c::ClientVersion {
                version_hash: Vec::new(),
            },
        ))
        .await
        .expect("版本校验");
    session
        .handle(mir_protocol::ClientPacket::Login(c::Login {
            account_id: "failure".to_owned(),
            password: "secret".to_owned(),
        }))
        .await
        .expect("登录");
    session
        .handle(mir_protocol::ClientPacket::StartGame(c::StartGame {
            character_index: i32::try_from(character.id).unwrap_or(0),
        }))
        .await
        .expect("进世界");
    let login_packets = drain(&mut out_rx);
    let mut object_id = None;
    let mut login_stock_levels = std::collections::HashMap::new();
    for packet in &login_packets {
        match packet {
            ServerPacket::UserInformation(info) => object_id = Some(info.object_id),
            ServerPacket::GameShopInfo(info) => {
                login_stock_levels.insert(info.item.g_index, info.stock_level);
            }
            _ => {}
        }
    }
    let object_id = object_id.expect("进世界应下发 UserInformation");
    assert_eq!(
        login_stock_levels.get(&G_SWORD),
        Some(&5),
        "登录链应下发限量商品的剩余库存 5"
    );
    assert_eq!(
        login_stock_levels.get(&G_POTION),
        Some(&0),
        "不限量商品登录发 StockLevel = 0"
    );

    // ---- 阶段一：注入事务失败 → 零库存包 + 回滚 ----
    store.fail_next_gameshop_update().await;
    session
        .handle(mir_protocol::ClientPacket::GameshopBuy(c::GameshopBuy {
            g_index: G_SWORD,
            quantity: 1,
            p_type: 0,
        }))
        .await
        .expect("处理购买");
    let packets = drain(&mut out_rx);
    assert!(
        !packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::GameShopStock(_))),
        "落盘失败时一个 S.GameShopStock 都不能发（失败零包）：{packets:?}"
    );
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::LoseCredit(lose) if lose.credit == 100)),
        "购买时先扣款"
    );
    assert!(
        packets.iter().any(
            |packet| matches!(packet, ServerPacket::GainedCredit(gained) if gained.credit == 100)
        ),
        "落盘失败要退款"
    );
    assert!(
        packets.iter().any(
            |packet| matches!(packet, ServerPacket::Chat(chat) if chat.message.contains("失败"))
        ),
        "落盘失败要给玩家提示"
    );
    {
        let world = world.lock().await;
        let player = world.player(object_id).expect("玩家在世界上");
        assert_eq!(player.credit, 1_000, "元宝退回");
        assert_eq!(
            world.game_shop_item(G_SWORD).expect("商品存在").stock,
            5,
            "回滚不刷：世界层商品库存仍是 5"
        );
    }
    assert!(
        store.gameshop_stock().await.is_empty(),
        "事务失败 → 全服计数一行未写"
    );
    assert!(
        store.gameshop_purchases().await.is_empty(),
        "事务失败 → 按角色计数一行未写"
    );

    // ---- 阶段二：不注入失败，重试同一件商品 ----
    // 回滚必须把库存/计数完整退回，否则这里会因「剩余不足」失败或发送错误的 StockLevel。
    session
        .handle(mir_protocol::ClientPacket::GameshopBuy(c::GameshopBuy {
            g_index: G_SWORD,
            quantity: 1,
            p_type: 0,
        }))
        .await
        .expect("处理购买");
    let packets = drain(&mut out_rx);
    let stocks: Vec<(i32, i32)> = packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::GameShopStock(stock) => Some((stock.g_index, stock.stock_level)),
            _ => None,
        })
        .collect();
    assert_eq!(
        stocks,
        vec![(G_SWORD, 4)],
        "重试成功：只降一次库存（5 - 1 = 4）且 GIndex 是商品 GIndex"
    );
    assert_eq!(
        store.gameshop_stock().await,
        vec![mir_server::store::GameShopStockRecord {
            g_index: G_SWORD,
            purchased: 1,
        }],
        "失败那次不能计数，成功这次才落一行"
    );
    assert_eq!(
        store.gameshop_purchases().await,
        vec![mir_server::store::GameShopPurchaseRecord {
            character_id: character.id,
            g_index: G_SWORD,
            quantity: 1,
        }],
        "按角色计数同样只记成功的一次"
    );
    {
        let world = world.lock().await;
        assert_eq!(
            world.player(object_id).expect("玩家").credit,
            900,
            "只扣成功那次的 100 元宝"
        );
    }
}
