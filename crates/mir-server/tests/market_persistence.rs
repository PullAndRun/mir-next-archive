//! 市场/拍卖挂单持久化的端到端测试（t4）。
//!
//! 覆盖「世界层 → `AccountStore` → 重建 `World`」这条链路：
//!
//! 1. `market_auction_survives_restart`：上架（`World::consign_item`）→ 全量快照落盘 →
//!    重建 `World` + `World::load_auctions` → 条目仍在、能被 `World::market_search` 搜到，
//!    且 `next_auction_id` 继续递增；
//! 2. `market_buyout_survives_restart`：买断 → 重启 → `Sold` 状态仍在，卖家重启后仍能收款，
//!    收款后的挂单删除也持久化；
//! 3. `market_bid_attribution_survives_restart`：拍卖出价 → 重启 → `current_bid` 与买家归属
//!    不变，卖家可提前成交（物品按系统邮件寄给买家）；
//! 4. `expired_auctions_are_purged_on_restart`：停机期间过期的挂单在启动清理时被移除，
//!    物品退回卖家、最高出价退回买家，清理结果再次重启仍为空。
//!
//! `MemoryStore` 与 PostgreSQL 走**同一组断言**（`run_*` 帮助函数）；PG 用例在没有
//! `MIR2_TEST_DATABASE_URL` 时跳过，与 `postgres_guild.rs` / `postgres_login.rs` 一致。
//!
//! Crystal 对照：`Server/MirDatabase/AuctionInfo.cs:12-23`（字段）与 `:71-87`（存档）；
//! `Server/MirEnvir/Envir.cs:2591-2594`（写全局 `Auctions` 段）、`:3018-3029`（读入并重绑）、
//! `:2396`（过期判定）。本作的持久化设计见 `crates/mir-server/src/market.rs` 与
//! `crates/mir-store/migrations/0009_market.sql`。

use std::sync::Arc;

use mir_core::enums::{
    AttackMode, BindMode, ItemType, MarketCollectionMode, MarketItemType, MarketPanelType,
    MirClass, MirDirection, MirGender, Stat,
};
use mir_core::{DateTimeBinary, Point};
use mir_db::{MirDb, NpcInfo};
use mir_protocol::data::{ItemInfo, Stats, UserItem};
use mir_server::gamedata::GameData;
use mir_server::mail::MailWrite;
use mir_server::store::{AuctionRecord, MarketUpdate};
use mir_server::{
    AccountStore, Auction, CONSIGN_KEY, MARKET_KEY, MagicState, MailRecord, MailState, MemoryStore,
    NpcState, PgAccountStore, Player, TradeState, World, combat, items,
};
use mir_world::{Map, MapCell, MapVersion};
use tokio::sync::mpsc;

/// 测试地图文件名。
const MAP: &str = "0";
/// 市场 NPC 的数据库主键。
const NPC_INDEX: i32 = 1;
/// 可寄售/可拍卖的物品模板。
const ITEM_INDEX: i32 = 1;

/// 卖家对象 ID。
const SELLER: u32 = 1;
/// 买家对象 ID。
const BUYER: u32 = 2;

// ---------------------------------------------------------------------------
// 世界与玩家脚手架
// ---------------------------------------------------------------------------

fn stats(pairs: &[(Stat, i32)]) -> Stats {
    Stats::from_iter(pairs.iter().copied())
}

fn item_info(index: i32, name: &str) -> ItemInfo {
    ItemInfo {
        index,
        name: name.to_owned(),
        item_type: ItemType::Weapon,
        bind: BindMode::None,
        weight: 1,
        image: u16::try_from(index).unwrap_or(0),
        durability: 0,
        stack_size: 1,
        price: 100,
        shape: 0,
        stats: stats(&[(Stat::MaxDC, 1)]),
        ..ItemInfo::default()
    }
}

fn npc_info() -> NpcInfo {
    NpcInfo {
        index: NPC_INDEX,
        map_index: 0,
        file_name: "Test/Market".to_owned(),
        name: "市场管理员".to_owned(),
        location: Point::new(10, 10),
        image: 5,
        rate: 100,
        ..NpcInfo::default()
    }
}

fn database() -> MirDb {
    MirDb {
        items: vec![item_info(ITEM_INDEX, "测试之剑")],
        npcs: vec![npc_info()],
        ..MirDb::default()
    }
}

fn synthetic_map() -> Arc<Map> {
    Map {
        width: 60,
        height: 60,
        cells: vec![MapCell::default(); 3600],
        file_name: MAP.to_owned(),
        version: MapVersion::V0,
        doors: Vec::new(),
    }
    .into()
}

/// 建一个带市场 NPC 的确定性世界。
fn build_world() -> (World, u32) {
    let mut world = World::new(999);
    world.seed_rng(7);
    world.insert_map(MAP, synthetic_map());
    world.set_game_data(Arc::new(GameData::from_db(database(), None)));
    let npc_id = world.spawn_npc(MAP, &npc_info()).expect("市场 NPC 应刷出");
    (world, npc_id)
}

#[allow(clippy::too_many_arguments)]
fn add_player(
    world: &mut World,
    object_id: u32,
    character_id: i64,
    name: &str,
    gold: i64,
    npc_id: u32,
) {
    let base = combat::player_base_stats(10, MirClass::Warrior);
    let (sender, _receiver) = mpsc::unbounded_channel();
    world.enter(Player {
        object_id,
        session_id: u64::from(object_id),
        character_id,
        name: name.to_owned(),
        map: MAP.to_owned(),
        position: Point::new(10, 10),
        direction: MirDirection::Down,
        level: 10,
        experience: 0,
        max_experience: 0,
        class: MirClass::Warrior,
        gender: MirGender::Male,
        hair: 1,
        hp: base.hp,
        mp: base.mp,
        stats: base,
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
        movement_time: 0,
        inventory: items::new_inventory(),
        equipment: items::new_equipment(),
        storage: mir_server::storage::new_storage(),
        magics: Vec::new(),
        magic: MagicState::default(),
        gold,
        credit: 0,
        visible_items: std::collections::BTreeSet::new(),
        quests: Vec::new(),
        completed_quests: std::collections::BTreeSet::new(),
        mail: MailState::default(),
        buffs: Vec::new(),
        pot_health_amount: 0,
        pot_mana_amount: 0,
        pot_time: 0,
        unlock_curse: false,
        npc: NpcState {
            npc_object_id: npc_id,
            ..NpcState::default()
        },
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
    });
}

fn set_page(world: &mut World, object_id: u32, key: &str) {
    let player = world.player_mut(object_id).expect("玩家应在世界里");
    key.clone_into(&mut player.npc.page_key);
}

/// 给玩家背包放一件可寄售物品，并打开寄售页。
fn give_item(world: &mut World, object_id: u32, unique_id: u64) {
    let player = world.player_mut(object_id).expect("玩家应在世界里");
    player.inventory[0] = Some(UserItem {
        unique_id,
        item_index: ITEM_INDEX,
        count: 1,
        ..UserItem::default()
    });
    player.gold = 100_000;
}

/// 上架一件物品，返回挂单 ID。
fn consign(
    world: &mut World,
    object_id: u32,
    unique_id: u64,
    price: u32,
    panel: MarketPanelType,
) -> u64 {
    give_item(world, object_id, unique_id);
    set_page(world, object_id, CONSIGN_KEY);
    world
        .consign_item(object_id, unique_id, price, panel)
        .expect("上架应成功")
}

/// 按真实协议路径搜索一次市场（`World::market_search`）。
fn search_market(world: &mut World, object_id: u32) {
    set_page(world, object_id, MARKET_KEY);
    world
        .market_search(
            object_id,
            "",
            ItemType::Nothing,
            false,
            0,
            0,
            MarketPanelType::Market,
        )
        .expect("搜索市场应成功");
}

async fn flush_mails<S: AccountStore>(store: &S, world: &mut World) {
    let writes = world.take_mail_writes();
    if !writes.is_empty() {
        mir_server::mail::persist_mail_writes(store, writes)
            .await
            .expect("退回邮件应能落库");
    }
}

/// 构造一条退款/退货系统邮件（F2 用例）。
fn sample_mail(character_id: i64, gold: u32) -> MailRecord {
    MailRecord {
        id: 0,
        character_id,
        sender_name: "比奇管理员".to_owned(),
        subject: String::new(),
        message: "测试退款".to_owned(),
        items: Vec::new(),
        gold,
        date_sent: DateTimeBinary::default(),
        opened: false,
        locked: false,
        collected: false,
    }
}

/// 构造一条只有外键会失败的挂单（F2/F3 回滚用例）。
fn invalid_auction(id: u64, seller_character_id: i64) -> AuctionRecord {
    AuctionRecord {
        id,
        item: Vec::new(),
        consignment_date: DateTimeBinary::default(),
        price: 5_000,
        current_bid: 0,
        seller_character_id,
        seller_name: "坏挂单".to_owned(),
        current_buyer_character_id: None,
        current_buyer_name: None,
        expired: false,
        sold: false,
        item_type: MarketItemType::Consign,
    }
}

/// 建一个账号和卖/买两个角色（返回账号 ID 与角色 ID）。
async fn seed_characters<S: AccountStore>(store: &S, tag: &str) -> (i64, i64, i64) {
    let account = store
        .create_account(&format!("mkt{tag}"), "secret", "")
        .await
        .expect("建测试账号");
    let seller = store
        .create_character(
            account.id,
            &format!("卖家{tag}"),
            MirClass::Warrior,
            MirGender::Male,
            MAP,
            (10, 10),
        )
        .await
        .expect("建卖家");
    let buyer = store
        .create_character(
            account.id,
            &format!("买家{tag}"),
            MirClass::Warrior,
            MirGender::Male,
            MAP,
            (10, 10),
        )
        .await
        .expect("建买家");
    (account.id, seller.id, buyer.id)
}

// ---------------------------------------------------------------------------
// 场景 1：上架 → 重启 → 仍在
// ---------------------------------------------------------------------------

async fn run_consign_survives_restart<S: AccountStore>(store: &S, tag: &str) {
    let (_account_id, seller_id, buyer_id) = seed_characters(store, tag).await;

    let (mut world, npc_id) = build_world();
    add_player(&mut world, SELLER, seller_id, "卖家", 100_000, npc_id);
    let auction_id = consign(&mut world, SELLER, 1, 5_000, MarketPanelType::Consign);
    store
        .save_auctions(world.auction_snapshot())
        .await
        .expect("上架后落盘");

    // 重启：重新建 World，再从存储层加载。
    let (mut restarted, npc_id) = build_world();
    let records = store.load_auctions().await.expect("读取挂单");
    assert_eq!(restarted.load_auctions(records), 1, "一条挂单应被载入");

    let auction = restarted.auction(auction_id).expect("重启后挂单仍应存在");
    assert_eq!(auction.price, 5_000);
    assert_eq!(auction.item.item_index, ITEM_INDEX);
    assert_eq!(auction.item.unique_id, 1);
    assert_eq!(auction.seller_character_id, seller_id);
    assert_eq!(auction.seller_name, "卖家");
    assert_eq!(auction.item_type, MarketItemType::Consign);
    assert!(!auction.sold && !auction.expired);

    // 买家通过真实搜索路径能看到它。
    add_player(&mut restarted, BUYER, buyer_id, "买家", 100_000, npc_id);
    search_market(&mut restarted, BUYER);
    let listings = &restarted
        .market_state_of(BUYER)
        .expect("搜索状态应在")
        .listings;
    assert!(
        listings.contains(&auction_id),
        "重启后的挂单应出现在搜索列表里：{listings:?}"
    );

    // 续号：再上架一件，新 ID 必须大于已载入的最大 ID（Crystal 的 `++NextAuctionID`）。
    add_player(&mut restarted, SELLER, seller_id, "卖家", 100_000, npc_id);
    let second = consign(&mut restarted, SELLER, 2, 5_000, MarketPanelType::Consign);
    assert!(second > auction_id, "重启后拍卖号应继续递增");

    store.save_auctions(Vec::new()).await.expect("清空测试挂单");
}

// ---------------------------------------------------------------------------
// 场景 2：买断 → 重启 → 已售 + 卖家可收款
// ---------------------------------------------------------------------------

async fn run_buyout_survives_restart<S: AccountStore>(store: &S, tag: &str) {
    let (_account_id, seller_id, buyer_id) = seed_characters(store, tag).await;

    let (mut world, npc_id) = build_world();
    add_player(&mut world, SELLER, seller_id, "卖家", 100_000, npc_id);
    add_player(&mut world, BUYER, buyer_id, "买家", 100_000, npc_id);
    let auction_id = consign(&mut world, SELLER, 1, 5_000, MarketPanelType::Consign);
    search_market(&mut world, BUYER);
    world
        .market_buy(BUYER, auction_id, 5_000)
        .expect("买断应成功");
    assert!(
        world.auction(auction_id).expect("挂单还在").sold,
        "买断后 `Sold` 应为真"
    );
    assert!(
        world
            .player(BUYER)
            .expect("买家在线")
            .inventory
            .iter()
            .flatten()
            .any(|item| item.item_index == ITEM_INDEX),
        "物品应进买家背包"
    );
    store
        .save_auctions(world.auction_snapshot())
        .await
        .expect("买断后落盘");

    // 重启：`Sold` 状态与卖家归属都要在。
    let (mut restarted, npc_id) = build_world();
    let records = store.load_auctions().await.expect("读取挂单");
    assert_eq!(restarted.load_auctions(records), 1);
    let auction = restarted.auction(auction_id).expect("已售挂单应在");
    assert!(auction.sold, "重启后 `Sold` 不应丢");
    assert_eq!(auction.seller_character_id, seller_id);
    assert_eq!(auction.price, 5_000);
    assert!(
        auction.current_buyer_character_id.is_none(),
        "寄售没有买家字段"
    );

    // 卖家重启后收款：金币按 Crystal 佣金公式入账，挂单删除。
    add_player(&mut restarted, SELLER, seller_id, "卖家", 95_000, npc_id);
    set_page(&mut restarted, SELLER, MARKET_KEY);
    let before = restarted.player(SELLER).expect("卖家在线").gold;
    let count = restarted
        .market_get_back(SELLER, MarketCollectionMode::Any, auction_id)
        .expect("卖家应能收款");
    assert_eq!(count, 1);
    assert!(restarted.auction(auction_id).is_none(), "收款后挂单应删除");
    let after = restarted.player(SELLER).expect("卖家在线").gold;
    assert_eq!(
        after - before,
        i64::from(Auction::after_commission(5_000)),
        "卖家应拿到扣除 5% 佣金后的金币"
    );
    store
        .save_auctions(restarted.auction_snapshot())
        .await
        .expect("收款后落盘");

    // 再重启一次：删除也要持久化。
    let (mut final_world, _) = build_world();
    assert_eq!(
        final_world.load_auctions(store.load_auctions().await.expect("读取挂单")),
        0
    );
    store.save_auctions(Vec::new()).await.expect("清空测试挂单");
}

// ---------------------------------------------------------------------------
// 场景 3：拍卖出价 → 重启 → 出价/买家归属不变
// ---------------------------------------------------------------------------

async fn run_bid_attribution_survives_restart<S: AccountStore>(store: &S, tag: &str) {
    let (_account_id, seller_id, buyer_id) = seed_characters(store, tag).await;

    let (mut world, npc_id) = build_world();
    add_player(&mut world, SELLER, seller_id, "卖家", 100_000, npc_id);
    add_player(&mut world, BUYER, buyer_id, "买家", 100_000, npc_id);
    let auction_id = consign(&mut world, SELLER, 1, 5_000, MarketPanelType::Auction);
    search_market(&mut world, BUYER);
    world
        .market_buy(BUYER, auction_id, 6_000)
        .expect("拍卖出价应成功");
    store
        .save_auctions(world.auction_snapshot())
        .await
        .expect("出价后落盘");

    let (mut restarted, npc_id) = build_world();
    assert_eq!(
        restarted.load_auctions(store.load_auctions().await.expect("读取挂单")),
        1
    );
    let auction = restarted.auction(auction_id).expect("拍卖应在");
    assert_eq!(auction.item_type, MarketItemType::Auction);
    assert_eq!(auction.price, 5_000);
    assert_eq!(auction.current_bid, 6_000);
    assert_eq!(auction.current_buyer_character_id, Some(buyer_id));
    assert_eq!(auction.current_buyer_name.as_deref(), Some("买家"));

    // 卖家重启后提前成交：物品按系统邮件寄给买家，卖家拿佣金后的成交价。
    add_player(&mut restarted, SELLER, seller_id, "卖家", 95_000, npc_id);
    set_page(&mut restarted, SELLER, MARKET_KEY);
    restarted
        .market_sell_now(SELLER, auction_id)
        .expect("提前成交应成功");
    assert!(restarted.auction(auction_id).is_none());
    flush_mails(store, &mut restarted).await;
    let mails = store.list_mail(buyer_id).await.expect("读买家邮箱");
    assert!(
        mails.iter().any(|mail| !mail.items.is_empty()),
        "成交物品应通过系统邮件寄给买家"
    );
    store
        .save_auctions(restarted.auction_snapshot())
        .await
        .expect("成交后落盘");
    store.save_auctions(Vec::new()).await.expect("清空测试挂单");
}

// ---------------------------------------------------------------------------
// 场景 4：过期 → 重启 → 启动清理（**出价不高于起拍价**，不满足自动成交条件）
// ---------------------------------------------------------------------------

async fn run_expired_auctions_are_purged<S: AccountStore>(store: &S, tag: &str) {
    let (_account_id, seller_id, buyer_id) = seed_characters(store, tag).await;

    let (mut world, npc_id) = build_world();
    add_player(&mut world, SELLER, seller_id, "卖家", 100_000, npc_id);
    add_player(&mut world, BUYER, buyer_id, "买家", 100_000, npc_id);
    let auction_id = consign(&mut world, SELLER, 1, 5_000, MarketPanelType::Auction);
    search_market(&mut world, BUYER);
    // 出价**等于**起拍价：Crystal 的自动成交条件是 `CurrentBid > Price`（`Envir.cs:2398`），
    // 相等不成交，所以这类挂单到期后仍走「置过期 → 启动清理退款退货」。
    world
        .market_buy(BUYER, auction_id, 5_000)
        .expect("拍卖出价应成功");

    // 把上架时间改成 8 天前，模拟停机期间过期（`Globals.ConsignmentLength = 7`）。
    let mut records = world.auction_snapshot();
    let old_ms = world.wall_now() - 8 * 24 * 60 * 60 * 1000;
    records[0].consignment_date = DateTimeBinary::from_unix_seconds(old_ms / 1000);
    store.save_auctions(records).await.expect("写回过期挂单");

    // 重启 + 启动清理（server.rs 启动路径的同款调用：先结算成交，再清理剩余的过期挂单）。
    let (mut restarted, _) = build_world();
    assert_eq!(
        restarted.load_auctions(store.load_auctions().await.expect("读取挂单")),
        1
    );
    let now = restarted.wall_now();
    assert!(
        restarted.process_auctions(now).is_empty(),
        "出价等于起拍价不满足 > 条件，不能自动成交"
    );
    let purged = restarted.purge_expired_auctions(now);
    assert_eq!(purged, vec![auction_id], "过期挂单应被启动清理");
    assert!(restarted.auction(auction_id).is_none());
    flush_mails(store, &mut restarted).await;

    let seller_mails = store.list_mail(seller_id).await.expect("读卖家邮箱");
    assert!(
        seller_mails.iter().any(|mail| !mail.items.is_empty()),
        "过期物品应退回卖家邮箱"
    );
    let buyer_mails = store.list_mail(buyer_id).await.expect("读买家邮箱");
    assert!(
        buyer_mails.iter().any(|mail| mail.gold == 5_000),
        "最高出价应退回买家邮箱"
    );

    // 清理结果落盘，再重启仍为空。
    store
        .save_auctions(restarted.auction_snapshot())
        .await
        .expect("清理后落盘");
    let (mut final_world, _) = build_world();
    assert_eq!(
        final_world.load_auctions(store.load_auctions().await.expect("读取挂单")),
        0
    );
    store.save_auctions(Vec::new()).await.expect("清空测试挂单");
}

// ---------------------------------------------------------------------------
// 场景 5：市场变更与角色行同事务（F3）
// ---------------------------------------------------------------------------

async fn run_market_update_is_atomic<S: AccountStore>(store: &S, tag: &str) {
    let (account_id, seller_id, buyer_id) = seed_characters(store, tag).await;

    let (mut world, npc_id) = build_world();
    add_player(&mut world, SELLER, seller_id, "卖家", 100_000, npc_id);
    add_player(&mut world, BUYER, buyer_id, "买家", 100_000, npc_id);
    let auction_id = consign(&mut world, SELLER, 1, 5_000, MarketPanelType::Consign);
    search_market(&mut world, BUYER);
    world
        .market_buy(BUYER, auction_id, 5_000)
        .expect("买断应成功");

    // 模拟「买断后立刻崩溃」：不经过下线存盘，只做市场原子落盘。
    let base = store
        .find_character(account_id, buyer_id)
        .await
        .expect("查买家")
        .expect("买家存在");
    let buyer_player = world.player(BUYER).expect("买家在线");
    let character = mir_server::market::character_snapshot(&base, buyer_player);
    store
        .apply_market_update(MarketUpdate {
            character: Some(character),
            mail_writes: world.take_mail_writes(),
            auctions: world.auction_snapshot(),
        })
        .await
        .expect("角色行 + 挂单应同事务落盘");

    // 直接读库：金币、背包、挂单三处都必须已经持久化。
    let reloaded = store
        .find_character(account_id, buyer_id)
        .await
        .expect("查买家")
        .expect("买家存在");
    assert_eq!(reloaded.gold, 95_000, "买断扣款必须和挂单同事务落库");
    let slots = mir_server::decode_slots(&reloaded.inventory).expect("背包字节块");
    assert!(
        slots
            .iter()
            .flatten()
            .any(|item| item.item_index == ITEM_INDEX),
        "买到的东西必须已经落库（崩溃窗口不丢）",
    );
    let auctions = store.load_auctions().await.expect("读取挂单");
    assert!(
        auctions
            .iter()
            .find(|auction| auction.id == auction_id)
            .is_some_and(|auction| auction.sold),
        "挂单已售状态必须已经落库",
    );

    // 失败回滚：角色行改金币 + 无效卖家挂单（外键必失败），必须整组回滚。
    let mut tampered = reloaded.clone();
    tampered.gold = 1;
    let result = store
        .apply_market_update(MarketUpdate {
            character: Some(tampered),
            mail_writes: Vec::new(),
            auctions: vec![invalid_auction(auction_id + 1, -1)],
        })
        .await;
    assert!(result.is_err(), "无效挂单必须让整个事务失败");
    let after = store
        .find_character(account_id, buyer_id)
        .await
        .expect("查买家")
        .expect("买家存在");
    assert_eq!(after.gold, 95_000, "失败事务不能改动角色行");
    assert_eq!(
        store.load_auctions().await.expect("读取挂单").len(),
        1,
        "失败事务不能改动挂单"
    );

    store.save_auctions(Vec::new()).await.expect("清空测试挂单");
}

// ---------------------------------------------------------------------------
// 场景 6：启动清理「退款至多一次」（F2）
// ---------------------------------------------------------------------------

async fn run_market_cleanup_is_atomic<S: AccountStore>(store: &S, tag: &str) {
    let (_account_id, seller_id, _buyer_id) = seed_characters(store, tag).await;

    // 清理中途失败：退款邮件 + 无效挂单 → 整个事务回滚，邮件不能落库。
    let failed = store
        .apply_market_update(MarketUpdate {
            character: None,
            mail_writes: vec![MailWrite::Create(Box::new(sample_mail(seller_id, 6_000)))],
            auctions: vec![invalid_auction(900, -1)],
        })
        .await;
    assert!(failed.is_err(), "无效挂单必须让清理事务失败");
    assert!(
        store
            .list_mail(seller_id)
            .await
            .expect("读邮箱")
            .iter()
            .all(|mail| mail.gold != 6_000),
        "失败清理不能留下退款邮件（否则重试会重复退款）",
    );

    // 成功：退款邮件 + 删除挂单同一事务提交。
    store
        .apply_market_update(MarketUpdate {
            character: None,
            mail_writes: vec![MailWrite::Create(Box::new(sample_mail(seller_id, 6_000)))],
            auctions: Vec::new(),
        })
        .await
        .expect("清理事务应成功");
    let mails = store.list_mail(seller_id).await.expect("读邮箱");
    assert_eq!(
        mails.iter().filter(|mail| mail.gold == 6_000).count(),
        1,
        "退款邮件必须恰好一封",
    );
    assert!(
        store.load_auctions().await.expect("读取挂单").is_empty(),
        "清理成功后挂单必须为空；重启不会再触发同一批退款",
    );
}

// ---------------------------------------------------------------------------
// 用例：MemoryStore（全跑）与 PostgreSQL（有环境变量才跑）
// ---------------------------------------------------------------------------

#[tokio::test]
async fn market_auction_survives_restart() {
    let store = MemoryStore::new();
    run_consign_survives_restart(&store, "mem-consign").await;
}

#[tokio::test]
async fn market_buyout_survives_restart() {
    let store = MemoryStore::new();
    run_buyout_survives_restart(&store, "mem-buyout").await;
}

#[tokio::test]
async fn market_bid_attribution_survives_restart() {
    let store = MemoryStore::new();
    run_bid_attribution_survives_restart(&store, "mem-bid").await;
}

#[tokio::test]
async fn expired_auctions_are_purged_on_restart() {
    let store = MemoryStore::new();
    run_expired_auctions_are_purged(&store, "mem-expire").await;
}

#[tokio::test]
async fn market_update_persists_character_and_auctions_atomically() {
    let store = MemoryStore::new();
    run_market_update_is_atomic(&store, "mem-atomic").await;
}

#[tokio::test]
async fn market_cleanup_refund_is_at_most_once() {
    let store = MemoryStore::new();
    run_market_cleanup_is_atomic(&store, "mem-cleanup").await;
}

/// 回归：重启载入后 `next_auction_id` 必须是 `max(id) + 1`，不能回到 1。
///
/// 否则下一次上架会用旧号覆盖库里另一条挂单（丢玩家物品）。解码失败的记录也要推进
/// 计数：Crystal 读档时 `BindItem` 失败会跳过条目（`Envir.cs:3025`），但 `NextAuctionID`
/// 已在存档里单独保存（`:2591`），本作没有计数表，必须从所有记录（含坏行）的 max id 续号。
#[tokio::test]
async fn reload_advances_next_auction_id_from_max_id() {
    use mir_server::encode_slots;
    use mir_server::store::AuctionRecord;

    let (mut world, npc_id) = build_world();
    add_player(&mut world, SELLER, 1, "卖家", 100_000, npc_id);

    let good_item = UserItem {
        unique_id: 9,
        item_index: ITEM_INDEX,
        count: 1,
        ..UserItem::default()
    };
    let records = vec![
        AuctionRecord {
            id: 100,
            item: encode_slots(&[Some(good_item)]),
            seller_character_id: 1,
            seller_name: "卖家".to_owned(),
            price: 5_000,
            item_type: MarketItemType::Consign,
            ..AuctionRecord::default()
        },
        // 坏物品块：载入时跳过，但 id=120 仍要参与续号。
        AuctionRecord {
            id: 120,
            item: vec![0xFF, 0xFF, 0xFF, 0xFF],
            seller_character_id: 1,
            seller_name: "卖家".to_owned(),
            ..AuctionRecord::default()
        },
    ];

    assert_eq!(world.load_auctions(records), 1, "坏行应被跳过");
    let new_id = consign(&mut world, SELLER, 10, 5_000, MarketPanelType::Consign);
    assert_eq!(new_id, 121, "续号必须从 max(id)+1 开始，不能回 1");

    // 空库/空记录才允许从 1 开始（显式的全新市场）。
    let mut empty = build_world().0;
    assert_eq!(empty.load_auctions(Vec::new()), 0);
}

#[tokio::test]
async fn postgres_market_persistence_roundtrip() {
    let Ok(url) = std::env::var("MIR2_TEST_DATABASE_URL") else {
        println!("跳过：未设置 MIR2_TEST_DATABASE_URL（需要一个可写的 PostgreSQL 测试库）");
        return;
    };
    let store = PgAccountStore::connect(&url)
        .await
        .expect("连接测试库并迁移");
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos() % 1_000_000_000);
    let tag = format!("pg{suffix}");
    run_consign_survives_restart(&store, &format!("{tag}a")).await;
    run_buyout_survives_restart(&store, &format!("{tag}b")).await;
    run_bid_attribution_survives_restart(&store, &format!("{tag}c")).await;
    run_expired_auctions_are_purged(&store, &format!("{tag}d")).await;
    run_market_update_is_atomic(&store, &format!("{tag}e")).await;
    run_market_cleanup_is_atomic(&store, &format!("{tag}f")).await;
    // 拍卖到期自动成交（Crystal `Envir.ProcessAuction`）：成交/只置过期/重复扫描/崩溃窗口。
    run_auto_sale_of_expired_auction(&store, &format!("{tag}g")).await;
    run_expired_auction_without_bid_is_marked(&store, &format!("{tag}h")).await;
    run_repeated_scan_sells_once(&store, &format!("{tag}i")).await;
    run_auto_sale_crash_window_is_consistent(&store, &format!("{tag}j")).await;
    store.save_auctions(Vec::new()).await.expect("清空测试挂单");
}

/// t12：`apply_market_update` 事务失败时，已 drain 的邮件必须回灌队列；
/// 修复故障后重试必须还能把它写进去，且成功后不能重复写。
#[tokio::test]
async fn flush_failure_keeps_mail_queue_for_retry() {
    let store = MemoryStore::new();
    let (mut world, npc_id) = build_world();
    add_player(&mut world, SELLER, 1, "卖家", 100_000, npc_id);
    add_player(&mut world, BUYER, 2, "买家", 100_000, npc_id);
    let auction_id = consign(&mut world, SELLER, 1, 5_000, MarketPanelType::Auction);
    search_market(&mut world, BUYER);
    world
        .market_buy(BUYER, auction_id, 6_000)
        .expect("拍卖出价应成功");
    set_page(&mut world, SELLER, MARKET_KEY);
    world
        .market_sell_now(SELLER, auction_id)
        .expect("提前成交应成功");

    // 此时成交物品邮件已在世界队列，但 MemoryStore 里还没有角色（id 1/2），事务必失败。
    let world = tokio::sync::Mutex::new(world);
    assert_eq!(
        world.lock().await.pending_mail_writes(),
        1,
        "提前成交应产生 1 封待写邮件",
    );
    let failed = mir_server::market::flush_market_update(&store, &world, None).await;
    assert!(failed.is_err(), "缺少角色时事务应失败（注入 StoreError）");
    assert_eq!(
        world.lock().await.pending_mail_writes(),
        1,
        "事务失败后邮件必须回灌队列，不能丢",
    );
    assert!(
        store.list_mail(2).await.expect("读邮箱").is_empty(),
        "失败事务不能落库",
    );

    // 补齐角色（MemoryStore 依次分配 id 1、2，与世界里的一致），重试 flush。
    let account = store
        .create_account("flush-retry", "secret", "")
        .await
        .expect("建测试账号");
    let seller = store
        .create_character(
            account.id,
            "卖家重试",
            MirClass::Warrior,
            MirGender::Male,
            MAP,
            (10, 10),
        )
        .await
        .expect("建卖家");
    let buyer = store
        .create_character(
            account.id,
            "买家重试",
            MirClass::Warrior,
            MirGender::Male,
            MAP,
            (10, 10),
        )
        .await
        .expect("建买家");
    assert_eq!((seller.id, buyer.id), (1, 2), "角色主键应与世界一致");

    mir_server::market::flush_market_update(&store, &world, None)
        .await
        .expect("故障修复后重试应成功");
    let mails = store.list_mail(2).await.expect("读邮箱");
    assert_eq!(mails.len(), 1, "失败前 drain 的邮件必须在重试时写入");
    assert!(!mails[0].items.is_empty(), "成交物品应作为附件寄出");
    assert_eq!(
        world.lock().await.pending_mail_writes(),
        0,
        "事务成功后队列才清空",
    );

    // 再 flush 一次：成功路径已清空队列，不能重复写同一封邮件。
    mir_server::market::flush_market_update(&store, &world, None)
        .await
        .expect("再次 flush");
    assert_eq!(
        store.list_mail(2).await.expect("读邮箱").len(),
        1,
        "成功后队列已清空，不能重复写",
    );
}

// ---------------------------------------------------------------------------
// 场景 7：拍卖到期自动成交（Crystal `Envir.ProcessAuction`，`Envir.cs:2388-2418`）
// ---------------------------------------------------------------------------

/// 把唯一一条挂单的上架时间改成 8 天前（`Globals.ConsignmentLength = 7`），世界与库同步。
async fn backdate_only_auction<S: AccountStore>(store: &S, world: &mut World) -> AuctionRecord {
    let mut records = world.auction_snapshot();
    assert_eq!(records.len(), 1, "用例只处理一条挂单");
    let old_seconds = world.wall_now() / 1000 - 8 * 24 * 60 * 60;
    records[0].consignment_date = DateTimeBinary::from_unix_seconds(old_seconds);
    store
        .save_auctions(records.clone())
        .await
        .expect("写回过期挂单");
    assert_eq!(world.load_auctions(records.clone()), 1);
    records.remove(0)
}

/// 邮箱里**物品总件数**（跨所有邮件、按附件槽位累计）。用来断言「没有双发」。
async fn mailbox_item_count<S: AccountStore>(store: &S, character_id: i64) -> usize {
    store
        .list_mail(character_id)
        .await
        .expect("读邮箱")
        .iter()
        .flat_map(|mail| {
            mir_server::decode_slots(&mail.items)
                .expect("邮件附件字节块")
                .into_iter()
                .flatten()
        })
        .count()
}

/// 邮箱里**金币总额**（跨所有邮件）。用来断言卖家没有收两次钱。
async fn mailbox_gold<S: AccountStore>(store: &S, character_id: i64) -> u32 {
    store
        .list_mail(character_id)
        .await
        .expect("读邮箱")
        .iter()
        .map(|mail| mail.gold)
        .sum()
}

/// 走到「拍卖已有高于起拍价的出价」这一步，并把出价那一笔按真实链路落盘。
async fn bid_on_only_auction<S: AccountStore>(
    store: &S,
    world: &mut World,
    account_id: i64,
    buyer_character_id: i64,
    auction_id: u64,
) {
    search_market(world, BUYER);
    world
        .market_buy(BUYER, auction_id, 6_000)
        .expect("拍卖出价应成功");
    let base = store
        .find_character(account_id, buyer_character_id)
        .await
        .expect("查买家")
        .expect("买家存在");
    let buyer = world.player(BUYER).expect("买家在线");
    store
        .apply_market_update(MarketUpdate {
            character: Some(mir_server::market::character_snapshot(&base, buyer)),
            mail_writes: world.take_mail_writes(),
            auctions: world.auction_snapshot(),
        })
        .await
        .expect("出价（买家扣款 + 挂单）应同事务落盘");
}

/// 场景 7a：到期 + 有高于起拍价的出价 → 自动成交。
///
/// * 物品按系统邮件寄给最高出价者（`market_mail_auction_won`）；
/// * 卖家收到「成交价 − 佣金」金币邮件（`market_mail_sold`）；
/// * 挂单从世界与库里一起消失，重启后不再出现；
/// * 重复扫描不会二次成交。
async fn run_auto_sale_of_expired_auction<S: AccountStore>(store: &S, tag: &str) {
    let (account_id, seller_id, buyer_id) = seed_characters(store, tag).await;
    let (mut world, npc_id) = build_world();
    add_player(&mut world, SELLER, seller_id, "卖家", 100_000, npc_id);
    add_player(&mut world, BUYER, buyer_id, "买家", 100_000, npc_id);
    let auction_id = consign(&mut world, SELLER, 1, 5_000, MarketPanelType::Auction);
    bid_on_only_auction(store, &mut world, account_id, buyer_id, auction_id).await;
    let record = backdate_only_auction(store, &mut world).await;
    assert_eq!(record.current_bid, 6_000);
    let seller_row_before = store
        .find_character(account_id, seller_id)
        .await
        .expect("查卖家")
        .expect("卖家存在");

    let now = world.wall_now();
    let sold = world.process_auctions(now);
    assert_eq!(sold.len(), 1, "到期且有更高出价的拍卖应自动成交");
    assert!(world.auction(auction_id).is_none(), "成交后挂单应清出列表");
    assert_eq!(
        world.pending_mail_writes(),
        2,
        "买家物品邮件 + 卖家金币邮件"
    );

    let world = tokio::sync::Mutex::new(world);
    mir_server::market::flush_auction_sales(store, &world)
        .await
        .expect("成交应原子落盘");

    // 买家：一件物品、无金币。
    let buyer_mails = store.list_mail(buyer_id).await.expect("读买家邮箱");
    let won = buyer_mails
        .iter()
        .find(|mail| !mail.items.is_empty())
        .expect("买家应收到物品邮件");
    let attachments = mir_server::decode_slots(&won.items).expect("邮件附件字节块");
    assert!(
        attachments
            .iter()
            .flatten()
            .any(|item| item.item_index == ITEM_INDEX),
        "邮件附件应是成交的那件物品"
    );
    assert_eq!(won.gold, 0);
    assert_eq!(
        won.message,
        mir_core::i18n::format_text(
            mir_core::Language::ZhCn,
            mir_core::TextKey::MarketMailAuctionWon,
            &[("item", "测试之剑"), ("cost", "6000")],
        ),
        "成交邮件正文按语言包渲染",
    );

    // 卖家：成交价 − 佣金（5%）。
    let payout = Auction::after_commission(6_000);
    assert_eq!(payout, 5_700);
    let seller_mails = store.list_mail(seller_id).await.expect("读卖家邮箱");
    let paid = seller_mails
        .iter()
        .find(|mail| mail.gold > 0)
        .expect("卖家应收到金币邮件");
    assert_eq!(paid.gold, payout, "卖家拿到成交价 − 佣金");
    assert!(
        mir_server::decode_slots(&paid.items)
            .expect("邮件字节块")
            .iter()
            .flatten()
            .next()
            .is_none(),
        "金币邮件不带物品附件",
    );
    assert_eq!(
        paid.message,
        mir_core::i18n::format_text(
            mir_core::Language::ZhCn,
            mir_core::TextKey::MarketMailSoldCommission,
            &[("item", "测试之剑"), ("cost", "6000"), ("gold", "5700")],
        ),
        "卖家邮件要同时写清成交价与扣佣金后到手",
    );

    // 库里挂单已删除；卖家角色行没有被跨会话改写（金币只走邮件）。
    assert!(
        store.load_auctions().await.expect("读取挂单").is_empty(),
        "成交后库里不能残留挂单"
    );
    let seller_row = store
        .find_character(account_id, seller_id)
        .await
        .expect("查卖家")
        .expect("卖家存在");
    assert_eq!(
        seller_row.gold, seller_row_before.gold,
        "卖家角色行只能由卖家自己的会话写（本作把金币寄成邮件）"
    );
    let _ = &seller_row_before;

    // 幂等：再扫一遍不会二次成交，也不会再产生邮件。
    let again = world.lock().await.process_auctions(now + 60_000);
    assert!(again.is_empty(), "同一拍卖只能成交一次");
    assert_eq!(world.lock().await.pending_mail_writes(), 0);

    // 重启：挂单不回来，邮件仍在。
    let (mut restarted, _) = build_world();
    assert_eq!(
        restarted.load_auctions(store.load_auctions().await.expect("读取挂单")),
        0
    );
    assert!(restarted.process_auctions(now).is_empty());
    assert_eq!(
        store.list_mail(buyer_id).await.expect("读买家邮箱").len(),
        1
    );
    assert_eq!(
        store.list_mail(seller_id).await.expect("读卖家邮箱").len(),
        1
    );
}

/// 场景 7b：到期但**没有**高于起拍价的出价 → 只置 `Expired`，不成交。
async fn run_expired_auction_without_bid_is_marked<S: AccountStore>(store: &S, tag: &str) {
    let (_account_id, seller_id, buyer_id) = seed_characters(store, tag).await;
    let (mut world, npc_id) = build_world();
    add_player(&mut world, SELLER, seller_id, "卖家", 100_000, npc_id);
    add_player(&mut world, BUYER, buyer_id, "买家", 100_000, npc_id);
    let auction_id = consign(&mut world, SELLER, 1, 5_000, MarketPanelType::Auction);
    backdate_only_auction(store, &mut world).await;

    let now = world.wall_now();
    let sold = world.process_auctions(now);
    assert!(sold.is_empty(), "没有出价不能成交");
    let auction = world.auction(auction_id).expect("挂单仍在");
    assert!(auction.expired && !auction.sold, "只置 Expired");
    assert_eq!(world.pending_mail_writes(), 0, "不成交就不发邮件");

    // 之后的取回/清理路径与原来一致：启动清理把物品退回卖家、把出价退回买家。
    assert_eq!(world.purge_expired_auctions(now), vec![auction_id]);
    let writes = world.take_mail_writes();
    assert!(
        writes.iter().any(|write| matches!(
            write,
            MailWrite::Create(mail) if mail.character_id == seller_id && !mail.items.is_empty()
        )),
        "过期物品退回卖家"
    );
}

/// 场景 7c：同一批发多次扫描只成交一次（重复 tick 不双发）。
async fn run_repeated_scan_sells_once<S: AccountStore>(store: &S, tag: &str) {
    let (account_id, seller_id, buyer_id) = seed_characters(store, tag).await;
    let (mut world, npc_id) = build_world();
    add_player(&mut world, SELLER, seller_id, "卖家", 100_000, npc_id);
    add_player(&mut world, BUYER, buyer_id, "买家", 100_000, npc_id);
    let auction_id = consign(&mut world, SELLER, 1, 5_000, MarketPanelType::Auction);
    bid_on_only_auction(store, &mut world, account_id, buyer_id, auction_id).await;
    backdate_only_auction(store, &mut world).await;

    let now = world.wall_now();
    let first = world.process_auctions(now);
    assert_eq!(first.len(), 1);
    for tick in 1..=5 {
        assert!(
            world.process_auctions(now + tick * 1_000).is_empty(),
            "第 {tick} 次重复扫描不能再成交"
        );
    }
    assert_eq!(world.pending_mail_writes(), 2, "重复扫描不能重复产生邮件");

    let world = tokio::sync::Mutex::new(world);
    mir_server::market::flush_auction_sales(store, &world)
        .await
        .expect("落盘");
    // 落盘后再扫：成交过的挂单已不在世界/库里，不能再发一次。
    assert!(world.lock().await.process_auctions(now + 10_000).is_empty());
    assert_eq!(
        store.list_mail(buyer_id).await.expect("读买家邮箱").len(),
        1
    );
    assert_eq!(
        store.list_mail(seller_id).await.expect("读卖家邮箱").len(),
        1
    );
}

/// 场景 7d：事务失败（崩溃窗口）→ 内存成交结果保留、邮件回灌队列；重试落盘成功且不双发。
async fn run_failed_sale_flush_keeps_memory_and_retries(store: &MemoryStore) {
    let (account_id, seller_id, buyer_id) = seed_characters(store, "mem-sale-fail").await;
    let (mut world, npc_id) = build_world();
    add_player(&mut world, SELLER, seller_id, "卖家", 100_000, npc_id);
    add_player(&mut world, BUYER, buyer_id, "买家", 100_000, npc_id);
    let auction_id = consign(&mut world, SELLER, 1, 5_000, MarketPanelType::Auction);
    bid_on_only_auction(store, &mut world, account_id, buyer_id, auction_id).await;
    backdate_only_auction(store, &mut world).await;

    let now = world.wall_now();
    let sold = world.process_auctions(now);
    assert_eq!(sold.len(), 1);
    let world = tokio::sync::Mutex::new(world);

    // 注入一次事务失败：DB 一行未写。
    store.fail_next_market_update().await;
    let failed = mir_server::market::flush_auction_sales(store, &world).await;
    assert!(failed.is_err(), "注入的事务失败应返回错误");
    {
        let guard = world.lock().await;
        assert!(
            guard.auction(auction_id).is_none(),
            "内存里的成交结果保留（挂单已删），落盘失败只重试事务，不回滚"
        );
        assert_eq!(guard.pending_mail_writes(), 2, "邮件必须回灌队列，不能丢");
    }
    assert!(
        store.load_auctions().await.expect("读取挂单").len() == 1,
        "失败事务不能改动库里的挂单"
    );
    assert!(
        store
            .list_mail(buyer_id)
            .await
            .expect("读买家邮箱")
            .is_empty()
    );

    // 重试同一个 flush（不重新扫描、不重新成交）→ 成功，且只发一次。
    assert!(
        world.lock().await.process_auctions(now + 1_000).is_empty(),
        "重试期间不能再成交一次"
    );
    mir_server::market::flush_auction_sales(store, &world)
        .await
        .expect("重试应成功");
    assert!(store.load_auctions().await.expect("读取挂单").is_empty());
    assert_eq!(
        world.lock().await.pending_mail_writes(),
        0,
        "提交成功后队列清空"
    );
    // 关键断言（中书省 t27）：买家**只收到 1 件**、卖家**只收到 1 笔**款。
    assert_eq!(
        mailbox_item_count(store, buyer_id).await,
        1,
        "买家只能收到 1 件物品"
    );
    assert_eq!(
        mailbox_gold(store, seller_id).await,
        Auction::after_commission(6_000),
        "卖家只能收到 1 笔成交价 − 佣金"
    );

    // 再 flush 一次（tick 任务的重试是幂等的）：不会重复写。
    mir_server::market::flush_auction_sales(store, &world)
        .await
        .expect("重复 flush");
    assert_eq!(mailbox_item_count(store, buyer_id).await, 1);
    assert_eq!(
        mailbox_gold(store, seller_id).await,
        Auction::after_commission(6_000)
    );
}

/// 场景 7f（中书省 t27 点名的并发入口）：tick 出锁后、会话的 `flush_market_update`
/// 先把成交落库，随后 tick 的 flush 失败——**不能**把挂单放回世界再成交一次。
///
/// 本作实现是「失败不回滚挂单、只重试事务」，所以这个交错天然安全：
/// 会话已经把「两封邮件 + 挂单删除」提交了，tick 的失败只影响它自己那次空事务。
async fn run_session_commit_then_tick_flush_failure(store: &MemoryStore) {
    let (account_id, seller_id, buyer_id) = seed_characters(store, "mem-sale-race").await;
    let (mut world, npc_id) = build_world();
    add_player(&mut world, SELLER, seller_id, "卖家", 100_000, npc_id);
    add_player(&mut world, BUYER, buyer_id, "买家", 100_000, npc_id);
    let auction_id = consign(&mut world, SELLER, 1, 5_000, MarketPanelType::Auction);
    bid_on_only_auction(store, &mut world, account_id, buyer_id, auction_id).await;
    backdate_only_auction(store, &mut world).await;

    let now = world.wall_now();
    assert_eq!(world.process_auctions(now).len(), 1);
    let world = tokio::sync::Mutex::new(world);

    // 会话侧（卖家的刷新/取回等操作）先把整批落盘：邮件 + 挂单删除同一事务。
    mir_server::market::flush_market_update(store, &world, None)
        .await
        .expect("会话 flush 应提交成交");
    assert_eq!(mailbox_item_count(store, buyer_id).await, 1);
    assert_eq!(
        mailbox_gold(store, seller_id).await,
        Auction::after_commission(6_000)
    );

    // 随后 tick 的 flush 失败（注入）：此时待写邮件已被会话 drain 空，事务是空写。
    store.fail_next_market_update().await;
    assert!(
        mir_server::market::flush_auction_sales(store, &world)
            .await
            .is_err(),
        "注入的事务失败应返回错误"
    );

    // 不双发：库里仍只有 1 件 + 1 笔，世界没有挂单，再扫也不会成交。
    assert_eq!(
        mailbox_item_count(store, buyer_id).await,
        1,
        "买家不能收两次"
    );
    assert_eq!(
        mailbox_gold(store, seller_id).await,
        Auction::after_commission(6_000),
        "卖家不能收两次钱"
    );
    assert!(store.load_auctions().await.expect("读取挂单").is_empty());
    assert!(
        world.lock().await.process_auctions(now + 60_000).is_empty(),
        "已成交的挂单不能因 tick flush 失败而复活"
    );

    // 恢复后再 flush 一次（tick 任务的 5s 重试路径）：仍然只有一份。
    mir_server::market::flush_auction_sales(store, &world)
        .await
        .expect("恢复后 flush");
    assert_eq!(mailbox_item_count(store, buyer_id).await, 1);
    assert_eq!(
        mailbox_gold(store, seller_id).await,
        Auction::after_commission(6_000)
    );
}

/// 场景 7e：崩溃窗口「内存成交了、事务没提交」→ 重启后由启动结算补跑，不会双发。
async fn run_auto_sale_crash_window_is_consistent<S: AccountStore>(store: &S, tag: &str) {
    let (account_id, seller_id, buyer_id) = seed_characters(store, tag).await;
    let (mut world, npc_id) = build_world();
    add_player(&mut world, SELLER, seller_id, "卖家", 100_000, npc_id);
    add_player(&mut world, BUYER, buyer_id, "买家", 100_000, npc_id);
    let auction_id = consign(&mut world, SELLER, 1, 5_000, MarketPanelType::Auction);
    bid_on_only_auction(store, &mut world, account_id, buyer_id, auction_id).await;
    backdate_only_auction(store, &mut world).await;

    // 成交只在内存里发生，随后进程崩溃（不落盘）。
    assert_eq!(world.process_auctions(world.wall_now()).len(), 1);
    drop(world);

    // 重启：库里挂单仍在且已过期；启动路径先结算成交、再清理剩余的过期挂单。
    let (mut restarted, _) = build_world();
    assert_eq!(
        restarted.load_auctions(store.load_auctions().await.expect("读取挂单")),
        1,
        "崩溃窗口里未提交的成交，挂单仍在库里"
    );
    let now = restarted.wall_now();
    let sold = restarted.process_auctions(now);
    assert_eq!(sold.len(), 1, "重启后补结算，仍应成交一次");
    let purged = restarted.purge_expired_auctions(now);
    assert!(purged.is_empty(), "已成交的挂单不应再走过期清理");

    let world = tokio::sync::Mutex::new(restarted);
    mir_server::market::flush_auction_sales(store, &world)
        .await
        .expect("重启结算落盘");
    assert_eq!(
        store.list_mail(buyer_id).await.expect("读买家邮箱").len(),
        1
    );
    assert_eq!(
        store.list_mail(seller_id).await.expect("读卖家邮箱").len(),
        1,
        "崩溃重启后也不能给卖家发两次金币"
    );
    assert!(store.load_auctions().await.expect("读取挂单").is_empty());
}

#[tokio::test]
async fn expired_auction_with_bid_sells_automatically() {
    let store = MemoryStore::new();
    run_auto_sale_of_expired_auction(&store, "mem-sale").await;
}

#[tokio::test]
async fn expired_auction_without_bid_only_expires() {
    let store = MemoryStore::new();
    run_expired_auction_without_bid_is_marked(&store, "mem-sale-none").await;
}

#[tokio::test]
async fn repeated_scan_sells_an_auction_only_once() {
    let store = MemoryStore::new();
    run_repeated_scan_sells_once(&store, "mem-sale-repeat").await;
}

#[tokio::test]
async fn failed_sale_flush_retries_without_double_delivery() {
    let store = MemoryStore::new();
    run_failed_sale_flush_keeps_memory_and_retries(&store).await;
}

#[tokio::test]
async fn sale_committed_by_session_flush_survives_tick_failure() {
    let store = MemoryStore::new();
    run_session_commit_then_tick_flush_failure(&store).await;
}

#[tokio::test]
async fn auto_sale_crash_window_is_consistent() {
    let store = MemoryStore::new();
    run_auto_sale_crash_window_is_consistent(&store, "mem-sale-crash").await;
}
