//! 商城（GameShop）与元宝的端到端测试。
//!
//! 覆盖任务要求的五类用例：
//! 1. **购买成功**：扣元宝、`S.LoseCredit`、物品进背包（`S.GainedItem`）、库存计数推进；
//! 2. **元宝不足**：拒绝购买、余额与背包不变、静默（Crystal `canAfford == false` 的语义）；
//! 3. **背包满转邮件**：物品攒成一封商城邮件（Crystal `PlayerObject.cs:13900-13907`）、
//!    邮件与元宝同一个事务落盘；
//! 4. **重启后元宝与库存正确**：用 `MemoryStore` 落盘 → 重新读库/重建世界再买；
//!    PostgreSQL 用例用 `MIR2_TEST_DATABASE_URL` 指向的独立测试库（没设置就跳过）；
//! 5. **市场面板 `GameShop` 页签**：`S.NPCMarket` 每页 10 条、翻页、条目字段
//!    （`auction_id = GIndex`、`price = CreditPrice`、`item_type = GameShop`）。
//!
//! 另外两条与「游戏内怎么拿到元宝」有关（本轮户部同时负责 `npc.rs`）：
//! * `GIVECREDIT` / `TAKECREDIT` / `CHECKCREDIT` 与 `<$CREDIT>` 占位符
//!   （Crystal `NPCSegment.cs:488-497`、`:2158-2174`、`:3286-3306`、`:1495-1497`）；
//! * 真实 `Server.MirDB` 的商城段（105 条商品）能通过 `C.MarketSearch` 下发、
//!   并通过 `C.GameshopBuy` 买到。
//!
//! 全部确定性：世界随机数固定种子、时间用 `World::advance_clock` 快进、脚本与数据库在
//! 临时目录/内存里构造；只有最后一个 `real_database_*` 用例读 `resource/`，缺失时跳过。

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use mir_core::Point;
use mir_core::enums::{
    ItemType, MarketItemType, MarketPanelType, MirClass, MirDirection, MirGender, RequiredClass,
};
use mir_db::{GameShopItem, ItemInfo, MapInfo, MirDb, NpcInfo};
use mir_protocol::ServerPacket;
use mir_server::gamedata::GameData;
use mir_server::items;
use mir_server::market::{
    GAMESHOP_MAX_STACKS, GameShopError, GameShopPayment, GameShopPurchases, MARKET_PAGE_SIZE,
    SEARCH_DELAY_MS,
};
use mir_server::store::{AccountStore, GameShopUpdate};
use mir_server::world::{Player, World};
use mir_server::{MemoryStore, NpcState};
use tokio::sync::mpsc;

const MAP: &str = "0";
/// 药品模板：可堆叠 20，商城用元宝价 10 / 金币价 100 卖。
const ITEM_POTION: i32 = 100;
/// 武器模板：不可堆叠，限战士。
const ITEM_SWORD: i32 = 200;
/// 商城商品：不限量药品（`Stock == 0`，`iStock == false`）。
const G_POTION: i32 = 1;
/// 商城商品：限量 5 件、按角色计库存（`iStock == true`）、限战士。
const G_SWORD: i32 = 2;

// ---------------------------------------------------------------------------
// 测试脚手架
// ---------------------------------------------------------------------------

/// 每个用例一个独立的 `Envir/` 目录（放 NPC 脚本），退出时删除。
struct Envir {
    root: PathBuf,
}

impl Envir {
    fn new(tag: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("mir-next-gameshop-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("NPCs/Test")).expect("建 NPC 目录");
        Self { root }
    }

    fn script(&self, name: &str, text: &str) -> &Self {
        let path = self.root.join("NPCs").join(format!("{name}.txt"));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("建脚本目录");
        }
        std::fs::write(&path, text).expect("写脚本");
        self
    }

    /// [`GameData::from_db`] 的掉落目录参数（父目录正好是 `Envir/`）。
    fn drop_dir(&self) -> PathBuf {
        self.root.join("Drops")
    }
}

impl Drop for Envir {
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

/// 最小数据库：2 件物品、1 张图、1 个 NPC、2 件商城商品。
fn database() -> MirDb {
    MirDb {
        items: vec![
            item(ITEM_POTION, "金创药", ItemType::Potion, 20),
            ItemInfo {
                required_class: RequiredClass::Warrior,
                ..item(ITEM_SWORD, "木剑", ItemType::Weapon, 1)
            },
        ],
        maps: vec![MapInfo {
            index: 1,
            file_name: MAP.to_owned(),
            title: "比奇省".to_owned(),
            ..MapInfo::default()
        }],
        npcs: vec![NpcInfo {
            index: 1,
            map_index: 1,
            file_name: "Test/Credit".to_owned(),
            name: "测试钱庄".to_owned(),
            location: Point::new(100, 100),
            image: 5,
            rate: 100,
            ..NpcInfo::default()
        }],
        gameshop: vec![
            product(ITEM_POTION, G_POTION, 10),
            GameShopItem {
                // 限量 5 件、按角色计库存、限战士（`class` 是 Crystal 客户端的职业分组名）。
                stock: 5,
                i_stock: true,
                class: "Warrior".to_owned(),
                ..product(ITEM_SWORD, G_SWORD, 100)
            },
        ],
        ..MirDb::default()
    }
}

/// 元宝相关脚本：`@Give` 加 100 元宝、`@Take` 扣 40、`@Check` 用 `CHECKCREDIT` 分支，
/// 三个页面都把 `<$CREDIT>` 渲染出来（`call_npc` 只允许跳当前页按钮上的页面，所以要回 `@Main`）。
const CREDIT_SCRIPT: &str = "\
[@Main]\n\
#SAY\n\
你的元宝：<$CREDIT>\n\
<领取/@Give> <检查/@Check> <扣款/@Take>\n\
\n\
[@Give]\n\
#ACT\n\
GIVECREDIT 100\n\
#SAY\n\
已到账，当前元宝：<$CREDIT>\n\
<返回/@Main>\n\
\n\
[@Check]\n\
#IF\n\
CHECKCREDIT >= 50\n\
#SAY\n\
元宝充足。\n\
<返回/@Main>\n\
#ELSEACT\n\
GOTO @Poor\n\
\n\
[@Poor]\n\
#SAY\n\
元宝不够。\n\
<返回/@Main>\n\
\n\
[@Take]\n\
#ACT\n\
TAKECREDIT 40\n\
#SAY\n\
扣除后元宝：<$CREDIT>\n\
<返回/@Main>\n\
";

struct Harness {
    world: World,
    receivers: HashMap<u32, mpsc::UnboundedReceiver<Vec<u8>>>,
}

impl Harness {
    fn new(tag: &str) -> (Self, Envir) {
        let envir = Envir::new(tag);
        envir.script("Test/Credit", CREDIT_SCRIPT);
        let data = GameData::from_db(database(), Some(envir.drop_dir()));
        let mut world = World::new(999);
        world.seed_rng(0x5EED);
        world.set_game_data(Arc::new(data));
        let spawned = world.spawn_npcs(MAP);
        assert_eq!(spawned.len(), 1, "钱庄 NPC 应该刷出来");
        (
            Self {
                world,
                receivers: HashMap::new(),
            },
            envir,
        )
    }

    fn add_player(&mut self, object_id: u32, class: MirClass, level: u16, credit: i64) -> u32 {
        let stats = mir_server::combat::player_base_stats(level, class);
        let (sender, receiver) = mpsc::unbounded_channel();
        self.world.enter(Player {
            object_id,
            session_id: u64::from(object_id),
            character_id: i64::from(object_id),
            name: format!("玩家{object_id}"),
            map: MAP.to_owned(),
            position: Point::new(100, 101),
            direction: MirDirection::Down,
            level,
            experience: 0,
            max_experience: 0,
            class,
            gender: MirGender::Male,
            hair: 1,
            hp: stats.hp,
            mp: stats.mp,
            stats,
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
            attack_mode: mir_core::enums::AttackMode::Peace,
            pk_points: 0,
            brown_time: 0,
            pk_point_time: 0,
            last_hitter: None,
            last_hit_time: 0,
            name_colour: mir_core::Color(-1),
            visible_monsters: BTreeSet::new(),
            visible_npcs: BTreeSet::new(),
            movement_time: 0,
            inventory: items::new_inventory(),
            equipment: items::new_equipment(),
            storage: mir_server::storage::new_storage(),
            magics: Vec::new(),
            magic: mir_server::MagicState::default(),
            gold: 0,
            credit,
            visible_items: BTreeSet::new(),
            quests: Vec::new(),
            completed_quests: BTreeSet::new(),
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
            trade: mir_server::TradeState::default(),
            trade_invitation: None,
            next_trade_time: 0,
            allow_trade: true,
            sender,
        });
        self.receivers.insert(object_id, receiver);
        object_id
    }

    fn packets(&mut self, object_id: u32) -> Vec<ServerPacket> {
        let receiver = self.receivers.get_mut(&object_id).expect("玩家通道");
        let mut packets = Vec::new();
        while let Ok(frame) = receiver.try_recv() {
            let index = i16::from_le_bytes([frame[2], frame[3]]);
            packets.push(ServerPacket::decode(index, &frame[4..]).expect("解码服务端包"));
        }
        packets
    }

    fn credit_of(&self, object_id: u32) -> i64 {
        self.world.player(object_id).expect("玩家存在").credit
    }

    fn inventory_count(&self, object_id: u32, item_index: i32) -> u32 {
        self.world
            .player(object_id)
            .expect("玩家存在")
            .inventory
            .iter()
            .flatten()
            .filter(|item| item.item_index == item_index)
            .map(|item| u32::from(item.count))
            .sum()
    }

    /// 把背包塞满不可堆叠的物品（用来触发「背包满转邮件」）。
    fn fill_inventory(&mut self, object_id: u32) {
        let slots = self
            .world
            .player(object_id)
            .expect("玩家存在")
            .inventory
            .len();
        for slot in 0..slots {
            self.world.add_item_to_inventory(
                object_id,
                mir_protocol::data::UserItem {
                    unique_id: u64::try_from(slot + 1).unwrap_or(1),
                    item_index: ITEM_SWORD,
                    count: 1,
                    ..Default::default()
                },
            );
        }
        assert!(
            self.world
                .player(object_id)
                .expect("玩家存在")
                .inventory
                .iter()
                .all(Option::is_some),
            "背包应该已经塞满"
        );
    }
}

fn no_purchases() -> GameShopPurchases {
    GameShopPurchases::default()
}

// ---------------------------------------------------------------------------
// 一、目录加载
// ---------------------------------------------------------------------------

/// 商城目录来自数据库的商城段，并绑定物品模板；模板缺失的商品被丢弃（Crystal
/// `Main.BindGameShop`，`Envir.cs:4550-4561`）；没有商城段时是空列表而不是启动失败。
#[test]
fn catalog_is_loaded_and_bound_to_item_templates() {
    let (_harness, _envir) = Harness::new("catalog");
    let data = GameData::from_db(database(), None);
    assert_eq!(data.game_shop_count(), 2);
    let potion = data.game_shop_item(G_POTION).expect("药品在目录里");
    assert_eq!(potion.item_index, ITEM_POTION);
    assert_eq!(potion.info.name, "金创药", "Info 应绑定到物品模板");
    assert_eq!(potion.info.stack_size, 20);

    // 引用了不存在的物品模板：丢弃（Crystal `BindGameShop` 返回 false → 不加入列表）。
    let mut db = database();
    db.gameshop.push(product(9999, 3, 1));
    let data = GameData::from_db(db, None);
    assert_eq!(data.game_shop_count(), 2, "模板缺失的商品应被丢弃");
    assert!(data.game_shop_item(3).is_none());

    // 没有商城段：空列表，服务器照常。
    let empty = GameData::from_db(MirDb::default(), None);
    assert_eq!(empty.game_shop_count(), 0);
    assert!(empty.game_shop().is_empty());
}

// ---------------------------------------------------------------------------
// 二、购买成功 / 元宝不足
// ---------------------------------------------------------------------------

/// 购买成功：扣元宝（`S.LoseCredit`）→ 物品进背包（`S.GainedItem`）→ 计数推进。
#[test]
fn purchase_deducts_credit_and_delivers_item() {
    let (mut harness, _envir) = Harness::new("buy-ok");
    let player = harness.add_player(1, MirClass::Warrior, 10, 100);
    harness.packets(player);

    let report = harness
        .world
        .gameshop_buy(player, G_POTION, 2, 0, &no_purchases())
        .expect("元宝够，应该买成功");
    assert_eq!(report.g_index, G_POTION);
    assert_eq!(report.payment, GameShopPayment::Credit);
    assert_eq!(report.cost, 20, "10 元宝 × 2 份");
    assert_eq!(report.gained, 1, "可堆叠物品并成 1 堆");
    assert_eq!(report.mailed, 0);
    assert_eq!(report.server_purchased, 2, "Crystal 每次购买都推进全服计数");
    assert_eq!(report.individual_purchased, None, "不限量商品不记个人计数");
    assert_eq!(report.remaining, None, "Stock == 0 表示不限量");
    assert_eq!(harness.credit_of(player), 80);
    assert_eq!(harness.inventory_count(player, ITEM_POTION), 2);

    let packets = harness.packets(player);
    let lose = packets
        .iter()
        .find_map(|packet| match packet {
            ServerPacket::LoseCredit(packet) => Some(packet.credit),
            _ => None,
        })
        .expect("应发 S.LoseCredit");
    assert_eq!(lose, 20);
    let gained = packets
        .iter()
        .filter(|packet| matches!(packet, ServerPacket::GainedItem(_)))
        .count();
    assert_eq!(gained, 1, "一堆物品一条 S.GainedItem");
}

/// 元宝不足：拒绝购买，余额/背包不变，且**不发任何包**（Crystal `canAfford=false` 静默）。
#[test]
fn purchase_without_enough_credit_is_rejected_silently() {
    let (mut harness, _envir) = Harness::new("buy-poor");
    let player = harness.add_player(1, MirClass::Warrior, 10, 5);
    harness.packets(player);

    let error = harness
        .world
        .gameshop_buy(player, G_POTION, 1, 0, &no_purchases())
        .expect_err("元宝不够");
    assert_eq!(
        error,
        GameShopError::NotEnoughCredit {
            needed: 10,
            held: 5
        }
    );
    assert!(error.is_silent(), "余额不足在 Crystal 里是静默 return");
    assert_eq!(harness.credit_of(player), 5);
    assert_eq!(harness.inventory_count(player, ITEM_POTION), 0);
    assert!(harness.packets(player).is_empty(), "失败不该发任何包");
}

/// 数量越界与「一次最多 5 组」都是静默拒绝（Crystal `:13727`、`:13756`）。
#[test]
fn quantity_limits_are_enforced() {
    let (mut harness, _envir) = Harness::new("quantity");
    let player = harness.add_player(1, MirClass::Warrior, 10, 100_000);
    harness.packets(player);

    assert_eq!(
        harness
            .world
            .gameshop_buy(player, G_POTION, 0, 0, &no_purchases())
            .expect_err("数量 0"),
        GameShopError::InvalidQuantity(0)
    );
    // 木剑不可堆叠：买 6 把要 6 组 > 5 组上限。
    let error = harness
        .world
        .gameshop_buy(player, G_SWORD, 6, 0, &no_purchases())
        .expect_err("超过 5 组");
    assert!(matches!(error, GameShopError::TooManyStacks { stacks: 6 }));
    assert!(error.is_silent(), "组数上限在 Crystal 里是静默 return");
    assert_eq!(harness.credit_of(player), 100_000, "拒绝时不能扣钱");
}

// ---------------------------------------------------------------------------
// 三、背包满 → 邮件
// ---------------------------------------------------------------------------

/// 背包满时物品攒成一封商城邮件（Crystal 的 `Sender = "Gameshop"` 邮件），
/// 元宝照扣，邮件进待写队列（随后由 `flush_gameshop_update` 与角色行同事务落盘）。
#[test]
fn bag_full_purchase_goes_to_mailbox() {
    let (mut harness, _envir) = Harness::new("bag-full");
    let player = harness.add_player(1, MirClass::Warrior, 10, 100);
    harness.fill_inventory(player);
    harness.packets(player);

    let report = harness
        .world
        .gameshop_buy(player, G_POTION, 3, 0, &no_purchases())
        .expect("背包满也能买（转邮件）");
    assert_eq!(report.gained, 0, "背包满，一件都不该进背包");
    assert_eq!(
        report.mailed, 1,
        "3 份可堆叠物品并成 1 堆 → 1 封邮件的 1 个附件"
    );
    assert_eq!(harness.credit_of(player), 70);

    let mailbox = harness.world.mailbox(player).expect("玩家存在");
    assert_eq!(mailbox.len(), 1, "应生成一封商城邮件");
    // 发件人/正文按语言包取（zh-cn 模板），不在测试里写死两份文案。
    let pack = language_pack();
    assert_eq!(
        mailbox[0].sender_name,
        pack.text(
            mir_core::Language::ZhCn,
            mir_core::TextKey::MailGameshopSender
        )
    );
    assert_eq!(
        mailbox[0].message,
        pack.text(
            mir_core::Language::ZhCn,
            mir_core::TextKey::MailGameshopBody
        ),
        "Crystal 的致谢正文"
    );
    assert_eq!(mailbox[0].items.len(), 1);
    assert_eq!(mailbox[0].items[0].item_index, ITEM_POTION);
    assert_eq!(mailbox[0].items[0].count, 3);
    assert_eq!(harness.world.pending_mail_writes(), 1, "邮件进入待写队列");

    let packets = harness.packets(player);
    assert!(
        packets.iter().any(|packet| matches!(
            packet,
            ServerPacket::Chat(chat) if chat.message.contains("邮箱")
        )),
        "应提示玩家去邮箱取货"
    );
}

// ---------------------------------------------------------------------------
// 四、落盘失败的回滚（服务端状态必须与数据库一致）
// ---------------------------------------------------------------------------

/// 事务失败时的回滚：元宝、背包物品、商城邮件全部退回（客户端会看到物品「进来又消失」，
/// 但服务端与库一致）。聊天提示由会话层发，见 [`failed_flush_rolls_back_purchase`]。
#[test]
fn rollback_restores_credit_items_and_mail() {
    let (mut harness, _envir) = Harness::new("rollback");
    let player = harness.add_player(1, MirClass::Warrior, 10, 100);
    harness.packets(player);

    let report = harness
        .world
        .gameshop_buy(player, G_POTION, 2, 0, &no_purchases())
        .expect("买成功");
    assert_eq!(harness.credit_of(player), 80);
    assert_eq!(harness.inventory_count(player, ITEM_POTION), 2);
    harness.world.rollback_gameshop_purchase(player, &report);
    assert_eq!(harness.credit_of(player), 100, "元宝退回");
    assert_eq!(
        harness.inventory_count(player, ITEM_POTION),
        0,
        "背包物品收回"
    );
    let packets = harness.packets(player);
    assert!(
        packets.iter().any(
            |packet| matches!(packet, ServerPacket::GainedCredit(gained) if gained.credit == 20)
        ),
        "回滚要发退款包"
    );

    // 背包满走邮件的那条路：邮件与待写队列一起撤回。
    harness.fill_inventory(player);
    let report = harness
        .world
        .gameshop_buy(player, G_POTION, 1, 0, &no_purchases())
        .expect("背包满也能买（转邮件）");
    let mails = harness.world.mailbox(player).expect("玩家存在");
    assert_eq!(mails.len(), 1);
    assert_eq!(harness.world.pending_mail_writes(), 1);
    harness.world.rollback_gameshop_purchase(player, &report);
    let mails = harness.world.mailbox(player).expect("玩家存在");
    assert!(mails.is_empty(), "商城邮件撤回");
    assert_eq!(harness.world.pending_mail_writes(), 0, "待写邮件队列清掉");
    assert_eq!(harness.credit_of(player), 100, "元宝退回");
}

/// 回滚必须按「模板 + 数量」而不是 `UniqueID`：可堆叠物品会被 `add_item_to_inventory`
/// 并进购买前就有的堆，新物品的 `UniqueID` 根本不在背包里（否则退款后玩家净赚）。
#[test]
fn rollback_handles_merged_stacks() {
    let (mut harness, _envir) = Harness::new("rollback-merge");
    let player = harness.add_player(1, MirClass::Warrior, 10, 100);
    // 购买前背包里已有 5 瓶金创药（同一模板、可堆叠 20）。
    assert!(harness.world.add_item_to_inventory(
        player,
        mir_protocol::data::UserItem {
            unique_id: 777,
            item_index: ITEM_POTION,
            count: 5,
            ..Default::default()
        }
    ));
    assert_eq!(harness.inventory_count(player, ITEM_POTION), 5);

    let report = harness
        .world
        .gameshop_buy(player, G_POTION, 2, 0, &no_purchases())
        .expect("买 2 瓶");
    assert_eq!(
        harness.inventory_count(player, ITEM_POTION),
        7,
        "并进已有堆：5 + 2"
    );
    assert_eq!(harness.credit_of(player), 80);

    harness.world.rollback_gameshop_purchase(player, &report);
    assert_eq!(
        harness.inventory_count(player, ITEM_POTION),
        5,
        "按数量反向扣减，旧的 5 瓶不能被误删"
    );
    assert_eq!(harness.credit_of(player), 100, "元宝退回");
}

/// 会话层的失败路径：`apply_gameshop_update` 注入失败 → 会话回滚内存扣款/发货、
/// 提示玩家、清空邮件队列；**数据库一行未写**，世界与存储完全一致。
#[tokio::test]
async fn failed_flush_rolls_back_purchase() {
    use mir_protocol::packets::client as c;

    let store = Arc::new(MemoryStore::new());
    let account = store
        .create_account("rollback", "secret", "")
        .await
        .expect("建号");
    let mut character = store
        .create_character(
            account.id,
            "回滚者",
            MirClass::Warrior,
            MirGender::Male,
            MAP,
            (100, 101),
        )
        .await
        .expect("建角");
    character.level = 10;
    character.credit = 1_000;
    // 背包预置 5 瓶金创药（可堆叠 → 购买会并堆，回滚必须按数量扣）。
    let mut inventory = items::new_inventory();
    inventory[0] = Some(mir_protocol::data::UserItem {
        unique_id: 777,
        item_index: ITEM_POTION,
        count: 5,
        ..Default::default()
    });
    character.inventory = items::encode_slots(&inventory);
    store
        .save_character(character.clone())
        .await
        .expect("存角色");

    let envir = Envir::new("rollback-session");
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
            account_id: "rollback".to_owned(),
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
    let mut object_id = None;
    while let Ok(frame) = out_rx.try_recv() {
        let index = i16::from_le_bytes([frame[2], frame[3]]);
        if let ServerPacket::UserInformation(info) =
            ServerPacket::decode(index, &frame[4..]).expect("解码")
        {
            object_id = Some(info.object_id);
        }
    }
    let object_id = object_id.expect("进世界应下发 UserInformation");

    // 注入一次事务失败：购买成功后落盘失败 → 回滚。
    store.fail_next_gameshop_update().await;
    session
        .handle(mir_protocol::ClientPacket::GameshopBuy(c::GameshopBuy {
            g_index: G_POTION,
            quantity: 2,
            p_type: 0,
        }))
        .await
        .expect("处理购买");
    let mut packets = Vec::new();
    while let Ok(frame) = out_rx.try_recv() {
        let index = i16::from_le_bytes([frame[2], frame[3]]);
        packets.push(ServerPacket::decode(index, &frame[4..]).expect("解码"));
    }
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::LoseCredit(lose) if lose.credit == 20)),
        "购买时先扣款"
    );
    assert!(
        packets.iter().any(
            |packet| matches!(packet, ServerPacket::GainedCredit(gained) if gained.credit == 20)
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
        let potions: u32 = player
            .inventory
            .iter()
            .flatten()
            .filter(|item| item.item_index == ITEM_POTION)
            .map(|item| u32::from(item.count))
            .sum();
        assert_eq!(potions, 5, "合并进去的 2 瓶要扣回，旧的 5 瓶保留");
        assert_eq!(world.pending_mail_writes(), 0);
    }
    let reloaded = store
        .find_character(account.id, character.id)
        .await
        .expect("读角色")
        .expect("角色存在");
    assert_eq!(reloaded.credit, 1_000, "事务失败 → 库里一行未写");
    assert!(store.gameshop_stock().await.is_empty());

    // 第二阶段：背包塞满 → 购买转邮件 → 注入失败 → 邮件与待写队列一起撤回。
    {
        let mut world = world.lock().await;
        for slot in 0..items::new_inventory().len() {
            let _ = world.add_item_to_inventory(
                object_id,
                mir_protocol::data::UserItem {
                    unique_id: 1_000 + u64::try_from(slot).unwrap_or(0),
                    item_index: ITEM_SWORD,
                    count: 1,
                    ..Default::default()
                },
            );
        }
    }
    store.fail_next_gameshop_update().await;
    session
        .handle(mir_protocol::ClientPacket::GameshopBuy(c::GameshopBuy {
            g_index: G_POTION,
            quantity: 1,
            p_type: 0,
        }))
        .await
        .expect("处理购买");
    let world = world.lock().await;
    assert!(
        world.mailbox(object_id).expect("玩家存在").is_empty(),
        "回滚要撤回商城邮件"
    );
    assert_eq!(world.pending_mail_writes(), 0, "待写邮件队列清空");
    assert_eq!(
        world.player(object_id).expect("玩家").credit,
        1_000,
        "第二阶段也退回元宝"
    );
}

// ---------------------------------------------------------------------------
// 五、库存与职业
// ---------------------------------------------------------------------------
/// 限量 + 按角色计库存：买 3 件后剩 2，再买 3 件被拒；职业不符直接拒绝。
#[test]
fn stock_and_class_limits_are_enforced() {
    let (mut harness, _envir) = Harness::new("stock");
    let player = harness.add_player(1, MirClass::Warrior, 10, 1_000);
    let wizard = harness.add_player(2, MirClass::Wizard, 10, 1_000);
    harness.packets(player);

    assert_eq!(
        harness
            .world
            .gameshop_buy(wizard, G_SWORD, 1, 0, &no_purchases())
            .expect_err("法师不能买限战士的商品"),
        GameShopError::WrongClass {
            class: "Warrior".to_owned()
        }
    );

    let report = harness
        .world
        .gameshop_buy(player, G_SWORD, 3, 0, &no_purchases())
        .expect("战士可以买");
    assert_eq!(report.individual_purchased, Some(3), "iStock 按角色计");
    assert_eq!(report.remaining, Some(2));
    assert_eq!(report.gained, 3, "不可堆叠：3 把剑 3 堆");

    // 用刚落盘的计数再买：只剩 2 件，买 3 件被拒（Crystal `:13770-13780`）。
    let (purchases, stock) = report.to_records(i64::from(player));
    let counts = GameShopPurchases::from_records(&stock, &purchases);
    let error = harness
        .world
        .gameshop_buy(player, G_SWORD, 3, 0, &counts)
        .expect_err("库存只剩 2");
    assert_eq!(error, GameShopError::OutOfStock { remaining: 2 });
    assert!(!error.is_silent(), "库存不足要提示（Crystal `:13776`）");
    assert_eq!(harness.credit_of(player), 700, "拒绝时不能再扣钱");

    let again = harness
        .world
        .gameshop_buy(player, G_SWORD, 2, 0, &counts)
        .expect("买走最后 2 件");
    assert_eq!(again.remaining, Some(0));
    assert_eq!(again.individual_purchased, Some(5));
}

/// 支付方式非法 / 金币支付：非法 `PType` 拒绝，金币够时按金币价扣款并发 `S.LoseGold`。
#[test]
fn payment_flags_are_enforced() {
    let (mut harness, _envir) = Harness::new("payment");
    let player = harness.add_player(1, MirClass::Warrior, 10, 1_000);
    if let Some(player_state) = harness.world.player_mut(player) {
        player_state.gold = 500;
    }
    harness.packets(player);

    assert_eq!(
        harness
            .world
            .gameshop_buy(player, G_POTION, 1, 7, &no_purchases())
            .expect_err("非法支付方式"),
        GameShopError::UnsupportedPayment(7)
    );
    assert!(!GameShopError::UnsupportedPayment(7).is_silent());

    // 金币支付：G_POTION 的 GoldPrice = CreditPrice * 10 = 100。
    let report = harness
        .world
        .gameshop_buy(player, G_POTION, 1, 1, &no_purchases())
        .expect("金币够");
    assert_eq!(report.payment, GameShopPayment::Gold);
    assert_eq!(report.cost, 100);
    assert_eq!(harness.world.player(player).expect("玩家").gold, 400);
    assert_eq!(harness.credit_of(player), 1_000, "金币支付不动元宝");
    let packets = harness.packets(player);
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::LoseGold(lose) if lose.gold == 100)),
        "金币支付发 S.LoseGold"
    );
}

// ---------------------------------------------------------------------------
// 六、市场面板的 GameShop 页签
// ---------------------------------------------------------------------------

/// 商城页签：`S.NPCMarket` 每页 [`MARKET_PAGE_SIZE`] 条，条目字段
/// `auction_id = GIndex` / `price = CreditPrice` / `item_type = GameShop`；翻页按 `GIndex` 取。
#[test]
fn market_panel_lists_shop_items_and_pages_by_ten() {
    let envir = Envir::new("panel");
    let mut db = database();
    // 再加 10 件商品凑够 12 条，验证分页（10 + 2）。
    for index in 0..10 {
        let mut extra = product(
            ITEM_POTION,
            100 + index,
            1 + u32::try_from(index).unwrap_or(0),
        );
        extra.class = "All".to_owned();
        db.gameshop.push(extra);
    }
    let data = GameData::from_db(db, Some(envir.drop_dir()));
    let mut world = World::new(999);
    world.set_game_data(Arc::new(data));
    let mut harness = Harness {
        world,
        receivers: HashMap::new(),
    };
    let player = harness.add_player(1, MirClass::Warrior, 10, 100);
    harness.packets(player);

    harness
        .world
        .market_search(
            player,
            "",
            ItemType::Nothing,
            false,
            0,
            0,
            MarketPanelType::GameShop,
        )
        .expect("商城页签搜索");
    let packets = harness.packets(player);
    let market = packets
        .iter()
        .find_map(|packet| match packet {
            ServerPacket::NPCMarket(market) => Some(market.clone()),
            _ => None,
        })
        .expect("应回 S.NPCMarket");
    assert!(!market.user_mode, "商城页签不是「只看自己」模式");
    assert_eq!(market.listings.len(), MARKET_PAGE_SIZE, "第一页 10 条");
    assert_eq!(market.pages, 2, "(12 - 1) / 10 + 1 = 2");
    let first = &market.listings[0];
    assert_eq!(
        first.auction_id,
        u64::try_from(G_POTION).unwrap_or(0),
        "auction_id = GIndex"
    );
    assert_eq!(first.item_type, MarketItemType::GameShop);
    assert_eq!(first.price, 10, "price = CreditPrice");
    assert_eq!(
        first.item.item_index, ITEM_POTION,
        "item 带模板 ID 供客户端显示"
    );
    assert!(first.seller.is_empty());

    // 翻页：商城页签不受 `page <= PageSent` 限制，但同样要过 500ms 节流。
    harness.world.advance_clock(SEARCH_DELAY_MS);
    harness.world.market_page(player, 1).expect("第 2 页");
    let packets = harness.packets(player);
    let page = packets
        .iter()
        .find_map(|packet| match packet {
            ServerPacket::NPCMarketPage(page) => Some(page.clone()),
            _ => None,
        })
        .expect("应回 S.NPCMarketPage");
    assert_eq!(page.listings.len(), 2, "第二页剩 2 条");
    assert_eq!(page.listings[0].auction_id, 108, "第 11 条商品的 GIndex");
}

// ---------------------------------------------------------------------------
// 七、NPC 元宝动作（GIVECREDIT / TAKECREDIT / CHECKCREDIT / <$CREDIT>）
// ---------------------------------------------------------------------------

/// 游戏内拿元宝的通道：`GIVECREDIT` 加、`TAKECREDIT` 扣、`CHECKCREDIT` 分支、
/// `<$CREDIT>` 占位符（Crystal `NPCSegment.cs:1495-1497`、`:2158-2174`、`:3286-3306`）。
#[test]
fn npc_script_can_give_and_check_credit() {
    let (mut harness, _envir) = Harness::new("npc-credit");
    let player = harness.add_player(1, MirClass::Warrior, 10, 0);
    let npc = harness
        .world
        .npcs()
        .iter()
        .find(|npc| npc.name == "测试钱庄")
        .map(|npc| npc.object_id)
        .expect("钱庄 NPC");
    harness.packets(player);

    // `<$CREDIT>`：初始 0。
    let reply = harness
        .world
        .call_npc(player, npc, "@main")
        .expect("打开钱庄");
    assert_eq!(reply.page[0], "你的元宝：0");

    // `GIVECREDIT 100` → S.GainedCredit 100，余额 100。
    let reply = harness
        .world
        .call_npc(player, npc, "@give")
        .expect("领取元宝");
    assert_eq!(reply.page[0], "已到账，当前元宝：100");
    assert_eq!(harness.credit_of(player), 100);
    assert!(
        harness.packets(player).iter().any(
            |packet| matches!(packet, ServerPacket::GainedCredit(gained) if gained.credit == 100)
        ),
        "GIVECREDIT 发 S.GainedCredit"
    );

    // `CHECKCREDIT >= 50`：通过分支（先回 `@Main` 让按钮合法）。
    harness
        .world
        .call_npc(player, npc, "@main")
        .expect("回主页");
    let reply = harness
        .world
        .call_npc(player, npc, "@check")
        .expect("检查元宝");
    assert_eq!(reply.page[0], "元宝充足。");

    // `TAKECREDIT 40` → S.LoseCredit 40，余额 60。
    harness
        .world
        .call_npc(player, npc, "@main")
        .expect("回主页");
    let reply = harness
        .world
        .call_npc(player, npc, "@take")
        .expect("扣元宝");
    assert_eq!(reply.page[0], "扣除后元宝：60");
    assert_eq!(harness.credit_of(player), 60);
    assert!(
        harness
            .packets(player)
            .iter()
            .any(|packet| matches!(packet, ServerPacket::LoseCredit(lose) if lose.credit == 40)),
        "TAKECREDIT 发 S.LoseCredit"
    );
}

// ---------------------------------------------------------------------------
// 八、重启后元宝与库存正确（MemoryStore）
// ---------------------------------------------------------------------------

/// 购买落盘后重读存储：元宝、全服计数、按角色计数都在；重建世界后用同一份计数
/// 继续被库存限制（「重启后正确」）。
#[tokio::test]
async fn restart_keeps_credit_and_stock() {
    let store = Arc::new(MemoryStore::new());
    let account = store
        .create_account("shopkeep", "secret", "")
        .await
        .expect("建号");
    let mut character = store
        .create_character(
            account.id,
            "掌柜",
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

    let envir = Envir::new("restart");
    let data = || Arc::new(GameData::from_db(database(), Some(envir.drop_dir())));

    // 第一次运行：买 3 把限量剑，落盘。
    let world = tokio::sync::Mutex::new({
        let mut world = World::new(999);
        world.set_game_data(data());
        world
    });
    let player = {
        let mut world = world.lock().await;
        let object_id = world.allocate_object_id();
        world.enter(player_from(&character, object_id));
        object_id
    };
    let report = {
        let mut world = world.lock().await;
        world
            .gameshop_buy(player, G_SWORD, 3, 0, &no_purchases())
            .expect("买 3 把剑")
    };
    assert_eq!(report.cost, 300);
    let snapshot = {
        let world = world.lock().await;
        mir_server::market::character_snapshot(
            &character,
            world.player(player).expect("玩家"),
            world.pet_mode(player),
        )
    };
    let (purchases, stock) = report.to_records(character.id);
    mir_server::market::flush_gameshop_update(
        store.as_ref(),
        &world,
        Some(snapshot),
        GameShopUpdate {
            character: None,
            mail_writes: Vec::new(),
            purchases,
            stock,
        },
    )
    .await
    .expect("商城事务落盘");

    // —— 模拟重启：一切从库里读。
    let reloaded = store
        .find_character(account.id, character.id)
        .await
        .expect("读角色")
        .expect("角色存在");
    assert_eq!(reloaded.credit, 700, "元宝已扣并持久化");
    let stock_rows = store.load_game_shop_stock().await.expect("读全服计数");
    assert_eq!(
        stock_rows
            .iter()
            .find(|row| row.g_index == G_SWORD)
            .map(|row| row.purchased),
        Some(3)
    );
    let purchase_rows = store
        .load_game_shop_purchases(character.id)
        .await
        .expect("读个人计数");
    assert_eq!(
        purchase_rows
            .iter()
            .find(|row| row.g_index == G_SWORD)
            .map(|row| row.quantity),
        Some(3)
    );

    // 第二次运行：世界重建，玩家按库里的余额进服，库存继续生效。
    let mut world2 = World::new(999);
    world2.set_game_data(data());
    let object_id = world2.allocate_object_id();
    world2.enter(player_from(&reloaded, object_id));
    let counts = GameShopPurchases::from_records(&stock_rows, &purchase_rows);
    assert_eq!(
        world2
            .gameshop_buy(object_id, G_SWORD, 3, 0, &counts)
            .expect_err("只剩 2 件"),
        GameShopError::OutOfStock { remaining: 2 }
    );
    let last = world2
        .gameshop_buy(object_id, G_SWORD, 2, 0, &counts)
        .expect("买走最后 2 件");
    assert_eq!(last.remaining, Some(0));
    assert_eq!(world2.player(object_id).expect("玩家").credit, 500);
}

/// 用存储层的角色行造一个 `Player`（与 `session.rs` 的进世界字段口径一致的最小集合）。
fn player_from(character: &mir_server::Character, object_id: u32) -> Player {
    let stats = mir_server::combat::player_base_stats(character.level, character.class);
    let (sender, _receiver) = mpsc::unbounded_channel();
    Player {
        object_id,
        session_id: u64::from(object_id),
        character_id: character.id,
        name: character.name.clone(),
        map: character.map.clone(),
        position: Point::new(character.x, character.y),
        direction: character.direction,
        level: character.level,
        experience: character.experience,
        max_experience: 0,
        class: character.class,
        gender: character.gender,
        hair: character.hair,
        hp: stats.hp,
        mp: stats.mp,
        stats,
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
        attack_mode: mir_core::enums::AttackMode::Peace,
        pk_points: 0,
        brown_time: 0,
        pk_point_time: 0,
        last_hitter: None,
        last_hit_time: 0,
        name_colour: mir_core::Color(-1),
        visible_monsters: BTreeSet::new(),
        visible_npcs: BTreeSet::new(),
        movement_time: 0,
        inventory: items::new_inventory(),
        equipment: items::new_equipment(),
        storage: mir_server::storage::new_storage(),
        magics: Vec::new(),
        magic: mir_server::MagicState::default(),
        gold: character.gold,
        credit: character.credit,
        visible_items: BTreeSet::new(),
        quests: Vec::new(),
        completed_quests: BTreeSet::new(),
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
        trade: mir_server::TradeState::default(),
        trade_invitation: None,
        next_trade_time: 0,
        allow_trade: true,
        sender,
    }
}

// ---------------------------------------------------------------------------
// 九、真实协议：C.MarketSearch / C.GameshopBuy 的接线（真实 Server.MirDB）
// ---------------------------------------------------------------------------

/// 真实样本数据库（素材索引里的 v111 全量库，105 件商城商品），相对**仓库根**。
const SAMPLE_DB_RELATIVE: &str = "resource/community/suprcode-crystal-database/Jev/Server.MirDB";

/// 把相对**仓库根**的路径解析成绝对路径（仓库根 = `CARGO_MANIFEST_DIR/../..`）。
///
/// **不能**直接用相对路径：`cargo test -p mir-server` 的工作目录是 `crates/mir-server`，
/// 而 `resource/` 在仓库根；用相对路径会「找不到 → 打印跳过 → 计 passed」，真实链路
/// 其实没测到（刑部 t7 F1）。返回 `None` 表示路径无法 canonicalize（文件不存在）。
fn resolve_from_repo_root(relative: &str) -> Option<std::path::PathBuf> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(relative);
    std::fs::canonicalize(path).ok()
}

/// 真实样本库的绝对路径（见 [`SAMPLE_DB_RELATIVE`]）。
fn sample_db_path() -> Option<std::path::PathBuf> {
    resolve_from_repo_root(SAMPLE_DB_RELATIVE)
}

/// t7 F1 的回归：路径解析必须以 `CARGO_MANIFEST_DIR` 为基准（与 cwd 无关），
/// 且不存在的相对路径要返回 `None`（调用方据此打印明确的跳过原因）。
#[test]
fn sample_db_path_is_resolved_from_manifest_dir() {
    let Some(path) = sample_db_path() else {
        println!(
            "跳过 sample_db_path_is_resolved_from_manifest_dir：真实样本库不存在（按 CARGO_MANIFEST_DIR/../.. 解析）"
        );
        return;
    };
    assert!(path.is_absolute(), "必须是绝对路径：{}", path.display());
    assert!(
        path.ends_with("Jev/Server.MirDB"),
        "应解析到样本库：{}",
        path.display()
    );
    assert!(
        resolve_from_repo_root("resource/definitely-missing/Server.MirDB").is_none(),
        "不存在的库必须返回 None，让调用方打印跳过原因"
    );
}

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
        // 登录序列里还有好友列表（`S.FriendUpdate`，Crystal `GetFriends()`），
        // 与本文件关心的包序无关，跳过它。
        let mut packet = self.expect().await;
        while matches!(packet, ServerPacket::FriendUpdate(_)) {
            packet = self.expect().await;
        }
        assert_eq!(packet.index(), index, "收到的包编号不符：{packet:?}");
        packet
    }

    /// 读到指定编号的包为止，**忽略**期间收到的其它包。
    ///
    /// 进世界序列的包数随功能/数据变化（任务、行会 buff、邮件、欢迎语…），
    /// 逐个断言顺序会让用例和「当前有哪些系统」耦合；这里只关心目标包，
    /// 与 `market_flow.rs` 的做法一致但更宽容。
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

/// 端到端：登录 → 进世界 → `C.MarketSearch{GameShop}` 拿到目录 → `C.GameshopBuy` 扣元宝并到货，
/// 并且落盘后重读存储的元宝正确（KeepAlive 当屏障：会话按包序处理，回包时事务已完成）。
///
/// 这条断言是「`session.rs` 的商城分派断了」最直接的报警器——没有它，`World` 层单测全绿
/// 也可能客户端发了包被 `_ =>` 静默忽略。
#[tokio::test]
async fn gameshop_buy_round_trips_over_the_wire() {
    use mir_protocol::packets::client as c;
    use mir_protocol::packets::server as s;

    // 真实样本库按**仓库根**解析（`CARGO_MANIFEST_DIR/../..`），见 [`sample_db_path`]。
    // 缺数据时跳过，但必须打印明确的绝对路径，不允许「静默跳过还计 passed」。
    let Some(db_path) = sample_db_path() else {
        println!(
            "跳过 gameshop_buy_round_trips_over_the_wire：真实样本库不存在（期望 {}/{}）。\
             本用例验证真实 `C.MarketSearch`/`C.GameshopBuy` 链路，缺数据时无法覆盖——\
             请确认仓库根的 resource/ 素材目录已就绪。",
            env!("CARGO_MANIFEST_DIR"),
            SAMPLE_DB_RELATIVE
        );
        return;
    };
    let db_path = db_path.to_string_lossy().into_owned();

    let store = Arc::new(MemoryStore::new());
    let account = store
        .create_account("shopbuy", "secret", "")
        .await
        .expect("建号");
    let mut character = store
        .create_character(
            account.id,
            "商城客",
            MirClass::Warrior,
            MirGender::Male,
            "0",
            (330, 270),
        )
        .await
        .expect("建角");
    character.level = 1;
    character.credit = 1_000;
    store
        .save_character(character.clone())
        .await
        .expect("存角色");

    let config = Arc::new(mir_server::ServerConfig {
        map_root: "resource/不存在的目录".to_owned(),
        language_dir: "config/不存在的目录".to_owned(),
        load_game_data: true,
        db_path: Some(db_path.clone()),
        ..mir_server::ServerConfig::default()
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("绑定临时端口");
    let address = listener.local_addr().expect("取端口");
    tokio::spawn(mir_server::serve_forever(
        listener,
        config,
        Arc::clone(&store),
    ));

    let mut client = TestClient::connect(address).await;
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
            account_id: "shopbuy".to_owned(),
            password: "secret".to_owned(),
        }))
        .await;
    client.expect_index(s::LoginSuccess::INDEX).await;
    client
        .send(mir_protocol::ClientPacket::StartGame(c::StartGame {
            character_index: i32::try_from(character.id).unwrap_or(0),
        }))
        .await;
    client.expect_index(s::StartGame::INDEX).await;
    // 进世界序列：读到 `S.UserInformation` 说明角色已进世界；其余包（任务、行会 buff、
    // 邮件、欢迎语…）与本用例无关，后面的 `expect_until` 会一并跳过。
    let info = match client.expect_until(s::UserInformation::INDEX).await {
        ServerPacket::UserInformation(info) => info,
        other => panic!("期望 UserInformation，收到 {other:?}"),
    };
    assert!(info.object_id > 0, "进世界应分配对象 ID");

    // `C.MarketSearch{MarketType=GameShop}` → `S.NPCMarket`：目录来自真实数据库。
    client
        .send(mir_protocol::ClientPacket::MarketSearch(c::MarketSearch {
            match_text: String::new(),
            item_type: ItemType::Nothing,
            usermode: false,
            min_shape: 0,
            max_shape: 0,
            market_type: MarketPanelType::GameShop,
        }))
        .await;
    let market = match client.expect_until(s::NPCMarket::INDEX).await {
        ServerPacket::NPCMarket(market) => market,
        other => panic!("期望 NPCMarket，收到 {other:?}"),
    };
    assert_eq!(market.listings.len(), MARKET_PAGE_SIZE, "每页 10 条");
    assert!(
        market.pages >= 10,
        "105 件商品至少 11 页，实际 {}",
        market.pages
    );
    let first = market.listings[0].clone();
    assert_eq!(first.item_type, MarketItemType::GameShop);
    assert!(first.auction_id > 0, "auction_id 是商品 GIndex");
    let g_index = i32::try_from(first.auction_id).expect("GIndex 落在 i32");
    let price = first.price;

    // `C.GameshopBuy` → `S.LoseCredit` + 至少一条 `S.GainedItem`。
    client
        .send(mir_protocol::ClientPacket::GameshopBuy(c::GameshopBuy {
            g_index,
            quantity: 1,
            p_type: 0,
        }))
        .await;
    let mut lose = None;
    let mut gained = 0usize;
    for _ in 0..40 {
        // 进世界序列可能还有没读完的包（任务/邮件/欢迎语…），一律跳过。
        match client.expect().await {
            ServerPacket::LoseCredit(packet) => lose = Some(packet.credit),
            ServerPacket::GainedItem(_) => gained += 1,
            _ => {}
        }
        if lose.is_some() && gained > 0 {
            break;
        }
    }
    assert_eq!(lose, Some(price), "扣的元宝等于列表里的 CreditPrice");
    assert!(gained > 0, "物品应直接进背包并发 S.GainedItem");

    // KeepAlive 屏障：会话按包序处理，收到回包时商城事务已经提交。
    client
        .send(mir_protocol::ClientPacket::KeepAlive(c::KeepAlive {
            time: 1,
        }))
        .await;
    client.expect_until(s::KeepAlive::INDEX).await;
    let reloaded = store
        .find_character(account.id, character.id)
        .await
        .expect("读角色")
        .expect("角色存在");
    assert_eq!(
        reloaded.credit,
        1_000 - i64::from(price),
        "元宝扣款应与角色行一起落盘"
    );
    // 打印一条「真的跑了」的证据：`--nocapture` 下能看到它，用来区分「通过」与「静默跳过」。
    println!(
        "真实链路通过：g_index={g_index} price={price} pages={} 首页条目={} 余额={} 样本库={}",
        market.pages,
        market.listings.len(),
        reloaded.credit,
        db_path
    );
}

// ---------------------------------------------------------------------------
// 十、PostgreSQL：元宝 + 库存同事务持久化（独立测试库）
// ---------------------------------------------------------------------------

/// 需要真实 PostgreSQL（`MIR2_TEST_DATABASE_URL`，URL 见 `target/test-db-url.txt`）。
///
/// 走**生产路径**：`World::gameshop_buy`（与 session 相同的校验/扣款/发货）→
/// `flush_gameshop_update`（角色行 + 元宝 + 两组计数同一事务）→ 重新读库断言
/// 「重启后元宝与库存正确」；再验证计数是绝对值 upsert（不会把两次数量相加）。
#[tokio::test]
async fn postgres_gameshop_purchase_persists() {
    let Ok(url) = std::env::var("MIR2_TEST_DATABASE_URL") else {
        println!("跳过：未设置 MIR2_TEST_DATABASE_URL（需要一个可写的 PostgreSQL 测试库）");
        return;
    };
    let store = mir_server::PgAccountStore::connect(&url)
        .await
        .expect("连接测试库并迁移");

    // 唯一后缀：账号名 3..=15 位，只取时间戳后 9 位。
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let username = format!("gs{}", suffix % 1_000_000_000);
    let account = store
        .create_account(&username, "secret", "")
        .await
        .expect("建测试账号");
    let mut character = store
        .create_character(
            account.id,
            &format!("仓{}", &username[2..]),
            MirClass::Warrior,
            MirGender::Male,
            MAP,
            (100, 101),
        )
        .await
        .expect("建测试角色");
    character.level = 10;
    character.credit = 500;
    store
        .save_character(character.clone())
        .await
        .expect("存角色");

    // 每次运行用一个独立的 GIndex，避免与历史行冲突。
    let g_index = 900_000 + i32::try_from(suffix % 100_000).unwrap_or(0);

    // 合成一个只含本用例商品的数据库，走**世界层入口** `World::gameshop_buy`
    // （与 session 购买同一条校验/扣款/发货路径），再用生产用的
    // `flush_gameshop_update` 落盘（t7 F5：不再手搓 GameShopUpdate）。
    let envir = Envir::new("pg");
    let mut db = database();
    db.gameshop = vec![GameShopItem {
        stock: 5,
        i_stock: true,
        ..product(ITEM_SWORD, g_index, 100)
    }];
    let world = tokio::sync::Mutex::new({
        let mut world = World::new(999);
        world.set_game_data(Arc::new(GameData::from_db(db, Some(envir.drop_dir()))));
        world
    });
    let object_id = {
        let mut world = world.lock().await;
        let object_id = world.allocate_object_id();
        world.enter(player_from(&character, object_id));
        object_id
    };
    let report = {
        let mut world = world.lock().await;
        world
            .gameshop_buy(object_id, g_index, 2, 0, &no_purchases())
            .expect("World::gameshop_buy 买 2 件")
    };
    assert_eq!(report.cost, 200, "100 元宝 × 2");
    assert_eq!(report.individual_purchased, Some(2));
    let snapshot = {
        let world = world.lock().await;
        mir_server::market::character_snapshot(
            &character,
            world.player(object_id).expect("玩家"),
            world.pet_mode(object_id),
        )
    };
    let (purchases, stock) = report.to_records(character.id);
    mir_server::market::flush_gameshop_update(
        &store,
        &world,
        Some(snapshot),
        GameShopUpdate {
            character: None,
            mail_writes: Vec::new(),
            purchases: purchases.clone(),
            stock: stock.clone(),
        },
    )
    .await
    .expect("商城事务");

    let reloaded = store
        .find_character(account.id, character.id)
        .await
        .expect("读角色")
        .expect("角色存在");
    assert_eq!(reloaded.credit, 300, "元宝在角色行上持久化");
    let stock_rows = store.load_game_shop_stock().await.expect("读全服计数");
    assert_eq!(
        stock_rows
            .iter()
            .find(|row| row.g_index == g_index)
            .map(|row| row.purchased),
        Some(2)
    );
    let purchase_rows = store
        .load_game_shop_purchases(character.id)
        .await
        .expect("读个人计数");
    assert_eq!(
        purchase_rows
            .iter()
            .find(|row| row.g_index == g_index)
            .map(|row| row.quantity),
        Some(2)
    );

    // 再买一次：绝对值 upsert（不会把两次数量相加）。
    store
        .apply_gameshop_update(GameShopUpdate {
            character: None,
            mail_writes: Vec::new(),
            purchases: vec![mir_server::store::GameShopPurchaseRecord {
                character_id: character.id,
                g_index,
                quantity: 3,
            }],
            stock: vec![mir_server::store::GameShopStockRecord {
                g_index,
                purchased: 3,
            }],
        })
        .await
        .expect("第二次商城事务");
    let stock_rows = store.load_game_shop_stock().await.expect("再读全服计数");
    assert_eq!(
        stock_rows
            .iter()
            .find(|row| row.g_index == g_index)
            .map(|row| row.purchased),
        Some(3),
        "upsert 写绝对值"
    );
}

// ---------------------------------------------------------------------------
// 十一、i18n：文本键按玩家语言渲染（期望值全部取自 `config/lang/*.json`）
// ---------------------------------------------------------------------------

/// 真实语言包（`config/lang`，仓库根 = `CARGO_MANIFEST_DIR/../..`）。
///
/// 测试断言直接从语言包取模板，不写死中英文案——这样文案调整时测试不会变红，
/// 只有当「渲染走错语言/参数名写错」时才会失败。
fn language_pack() -> mir_core::i18n::LanguageLibrary {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("config/lang");
    mir_core::i18n::LanguageLibrary::load_dir(&dir)
        .unwrap_or_else(|error| panic!("读取语言包 {} 失败：{error}", dir.display()))
}

/// 商城错误提示：无参 + 两条带参模板，中英各渲染一次（`GameShopError::message`）。
#[test]
fn i18n_gameshop_errors_follow_language() {
    use mir_core::Language;
    use mir_core::TextKey;
    let pack = language_pack();
    let zh = Language::ZhCn;
    let en = Language::En;

    // 带参 ①：一次最多买 N 组（{max} / {stacks}）。
    let error = GameShopError::TooManyStacks { stacks: 6 };
    let args = [
        ("max", GAMESHOP_MAX_STACKS.to_string()),
        ("stacks", 6.to_string()),
    ];
    let args_ref: Vec<(&str, &str)> = args.iter().map(|(k, v)| (*k, v.as_str())).collect();
    assert_eq!(
        error.message(zh),
        pack.format(zh, TextKey::GameshopTooManyStacks, &args_ref)
    );
    assert_eq!(
        error.message(en),
        pack.format(en, TextKey::GameshopTooManyStacks, &args_ref)
    );

    // 带参 ②：等级不够（{required}）。
    let error = GameShopError::WrongLevel {
        required: 22,
        level: 10,
    };
    assert_eq!(
        error.message(en),
        pack.format(en, TextKey::GameshopWrongLevel, &[("required", "22")])
    );

    // 无参：库存不足。
    assert_eq!(
        GameShopError::OutOfStock { remaining: 0 }.message(en),
        pack.text(en, TextKey::GameshopOutOfStock)
    );
}

/// 市场过期清理的**系统邮件**按收件人语言渲染（买家 en、卖家 zh），
/// 覆盖 `market_mail_bid_refund_expired` / `market_mail_item_expired` 两条带参模板。
#[test]
fn i18n_market_mails_follow_language() {
    use mir_core::Language;
    use mir_core::TextKey;
    let (mut harness, _envir) = Harness::new("i18n-market");
    let seller = harness.add_player(1, MirClass::Warrior, 10, 0);
    let buyer = harness.add_player(2, MirClass::Warrior, 10, 0);
    let pack = language_pack();
    harness.world.set_languages(Arc::new(pack.clone()));
    harness.world.set_player_language(seller, Language::ZhCn);
    harness.world.set_player_language(buyer, Language::En);

    // 一条已过期、有 ·最高出价· 的寄售挂单（卖家角色 1、买家角色 2）。
    let item = mir_protocol::data::UserItem {
        unique_id: 5_001,
        item_index: ITEM_SWORD,
        count: 1,
        ..Default::default()
    };
    harness.world.load_auctions(vec![mir_server::AuctionRecord {
        id: 77,
        item: items::encode_slots(&[Some(item)]),
        consignment_date: mir_core::DateTimeBinary::default(),
        price: 5_000,
        current_bid: 6_000,
        seller_character_id: 1,
        seller_name: "卖家".to_owned(),
        current_buyer_character_id: Some(2),
        current_buyer_name: Some("买家".to_owned()),
        expired: true,
        sold: false,
        item_type: MarketItemType::Consign,
    }]);
    let now = 10_000_000_000_i64;
    let purged = harness.world.purge_expired_auctions(now);
    assert_eq!(purged, vec![77], "过期挂单应被清理");

    let mails: Vec<mir_server::store::MailRecord> = harness
        .world
        .take_mail_writes()
        .into_iter()
        .filter_map(|write| match write {
            mir_server::mail::MailWrite::Create(mail) => Some(*mail),
            _ => None,
        })
        .collect();
    assert_eq!(mails.len(), 2, "买家退款 + 卖家退货各一封");
    let item_name = mir_server::items::friendly_name("木剑");
    let buyer_mail = mails
        .iter()
        .find(|mail| mail.character_id == 2)
        .expect("买家邮件");
    assert_eq!(
        buyer_mail.message,
        pack.format(
            Language::En,
            TextKey::MarketMailBidRefundExpired,
            &[("item", &item_name), ("gold", "6000")]
        )
    );
    assert_eq!(
        buyer_mail.sender_name,
        pack.text(Language::En, TextKey::MailSystemSender),
        "系统邮件发件人按买家语言"
    );
    let seller_mail = mails
        .iter()
        .find(|mail| mail.character_id == 1)
        .expect("卖家邮件");
    assert_eq!(
        seller_mail.message,
        pack.format(
            Language::ZhCn,
            TextKey::MarketMailItemExpired,
            &[("item", &item_name)]
        )
    );
}

/// 商城邮件（发件人/正文）与 `MailError` 提示按玩家语言渲染。
#[test]
fn i18n_gameshop_mail_and_mail_errors_follow_language() {
    use mir_core::Language;
    use mir_core::TextKey;
    let (mut harness, _envir) = Harness::new("i18n-mail");
    let player = harness.add_player(1, MirClass::Warrior, 10, 100);
    let pack = language_pack();
    harness.world.set_languages(Arc::new(pack.clone()));
    harness.world.set_player_language(player, Language::En);

    // 背包塞满 → 购买转邮件，邮件按玩家语言（en）渲染。
    harness.fill_inventory(player);
    let report = harness
        .world
        .gameshop_buy(player, G_POTION, 1, 0, &no_purchases())
        .expect("背包满也能买");
    assert_eq!(report.mailed, 1);
    let mails: Vec<mir_server::store::MailRecord> = harness
        .world
        .take_mail_writes()
        .into_iter()
        .filter_map(|write| match write {
            mir_server::mail::MailWrite::Create(mail) => Some(*mail),
            _ => None,
        })
        .collect();
    assert_eq!(
        mails[0].sender_name,
        pack.text(Language::En, TextKey::MailGameshopSender)
    );
    assert_eq!(
        mails[0].message,
        pack.text(Language::En, TextKey::MailGameshopBody)
    );

    // `MailError` 的带参模板（{max}）也按语言取。
    let error = mir_server::mail::MailError::TextTooLong;
    assert_eq!(
        error.message(Language::En),
        pack.format(
            Language::En,
            TextKey::MailTextTooLong,
            &[("max", &mir_server::mail::MAIL_TEXT_MAX.to_string())]
        )
    );
    assert_eq!(
        error.message(Language::ZhCn),
        pack.format(
            Language::ZhCn,
            TextKey::MailTextTooLong,
            &[("max", &mir_server::mail::MAIL_TEXT_MAX.to_string())]
        )
    );
}

/// 会话层：进世界把账号语言写进世界；`/language en` 后世界层同步切换；
/// 商城页签（`C.MarketBuy`）的成功回执按新语言渲染（带参模板 `gameshop_bought_market_success`）。
#[tokio::test]
async fn i18n_session_syncs_language_and_renders_success() {
    use mir_core::Language;
    use mir_core::TextKey;
    use mir_protocol::packets::client as c;

    let pack = language_pack();
    let store = Arc::new(MemoryStore::new());
    let account = store
        .create_account("i18n", "secret", "")
        .await
        .expect("建号");
    let mut character = store
        .create_character(
            account.id,
            "多语言",
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

    let envir = Envir::new("i18n-session");
    let mut world = World::new(999);
    world.set_game_data(Arc::new(GameData::from_db(
        database(),
        Some(envir.drop_dir()),
    )));
    world.set_languages(Arc::new(pack.clone()));
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
        Arc::new(pack.clone()),
    );
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
            account_id: "i18n".to_owned(),
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
    let mut object_id = None;
    let drain = |rx: &mut mpsc::UnboundedReceiver<Vec<u8>>| {
        let mut packets = Vec::new();
        while let Ok(frame) = rx.try_recv() {
            let index = i16::from_le_bytes([frame[2], frame[3]]);
            packets.push(ServerPacket::decode(index, &frame[4..]).expect("解码"));
        }
        packets
    };
    for packet in drain(&mut out_rx) {
        if let ServerPacket::UserInformation(info) = packet {
            object_id = Some(info.object_id);
        }
    }
    let object_id = object_id.expect("进世界应下发 UserInformation");
    assert_eq!(
        world.lock().await.player_language(object_id),
        Language::ZhCn,
        "进世界时按账号语言（默认 zh-cn）写入世界层"
    );

    // 切到英文：世界层同步。
    session
        .handle(mir_protocol::ClientPacket::Chat(c::Chat {
            message: "/language en".to_owned(),
            linked_items: Vec::new(),
        }))
        .await
        .expect("切换语言");
    assert_eq!(
        world.lock().await.player_language(object_id),
        Language::En,
        "/language 切换后世界层必须同步，否则世界提示仍用旧语言"
    );

    // 商城页签：搜索 + 用 `C.MarketBuy` 购买，成功回执按 en 渲染。
    session
        .handle(mir_protocol::ClientPacket::MarketSearch(c::MarketSearch {
            match_text: String::new(),
            item_type: ItemType::Nothing,
            usermode: false,
            min_shape: 0,
            max_shape: 0,
            market_type: MarketPanelType::GameShop,
        }))
        .await
        .expect("搜索商城");
    let market = drain(&mut out_rx)
        .into_iter()
        .find_map(|packet| match packet {
            ServerPacket::NPCMarket(market) => Some(market),
            _ => None,
        })
        .expect("应回 S.NPCMarket");
    assert!(!market.listings.is_empty());

    session
        .handle(mir_protocol::ClientPacket::MarketBuy(c::MarketBuy {
            auction_id: u64::try_from(G_POTION).unwrap_or(0),
            bid_price: 0,
        }))
        .await
        .expect("商城页签购买");
    let packets = drain(&mut out_rx);
    let success = packets
        .iter()
        .find_map(|packet| match packet {
            ServerPacket::MarketSuccess(success) => Some(success.message.clone()),
            _ => None,
        })
        .expect("应回 S.MarketSuccess");
    assert_eq!(
        success,
        pack.format(
            Language::En,
            TextKey::GameshopBoughtMarketSuccess,
            &[("item", "金创药"), ("count", "1")]
        )
    );
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::LoseCredit(lose) if lose.credit == 10)),
        "元宝按商品价扣除"
    );
}

// ---------------------------------------------------------------------------
// 十二、任务物品栏（迁移 0012 / Crystal `CharacterInfo.QuestInventory`）
// ---------------------------------------------------------------------------

/// 任务物品栏必须**活过**商城/市场的角色行落盘（`Session::character_snapshot` 覆写），
/// 否则任意一次购买都会把 `characters.quest_inventory` 写回登录时的旧值（丢任务物品）。
#[tokio::test]
async fn quest_inventory_survives_gameshop_flush_and_relogin() {
    use mir_protocol::packets::client as c;

    const QUEST_ITEM: i32 = 300;
    const QUEST_ITEM_UID: u64 = 9_001;

    let store = Arc::new(MemoryStore::new());
    let account = store
        .create_account("qinv", "secret", "")
        .await
        .expect("建号");
    let mut character = store
        .create_character(
            account.id,
            "带任务物品",
            MirClass::Warrior,
            MirGender::Male,
            MAP,
            (100, 101),
        )
        .await
        .expect("建角");
    character.level = 10;
    character.credit = 1_000;
    // 角色行里预置 1 件任务物品（登录时由 `QuestInventory::decode` 装进世界侧表）。
    let mut quest_inventory = mir_server::quest_inventory::QuestInventory::new();
    let quest_item = mir_protocol::data::UserItem {
        unique_id: QUEST_ITEM_UID,
        item_index: QUEST_ITEM,
        count: 1,
        ..Default::default()
    };
    assert!(
        quest_inventory.add(&quest_item, 20),
        "任务物品栏应放得进这件物品"
    );
    character.quest_inventory = quest_inventory.encode();
    store
        .save_character(character.clone())
        .await
        .expect("存角色");

    let envir = Envir::new("quest-inventory");
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
    let pack = Arc::new(mir_core::i18n::LanguageLibrary::embedded());
    let (out, mut out_rx) = mpsc::unbounded_channel();
    let mut session = mir_server::session::Session::new(
        1,
        "test".to_owned(),
        config,
        Arc::clone(&store),
        Arc::clone(&world),
        out,
        Arc::clone(&pack),
    );
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
            account_id: "qinv".to_owned(),
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
    let mut object_id = None;
    while let Ok(frame) = out_rx.try_recv() {
        let index = i16::from_le_bytes([frame[2], frame[3]]);
        if let ServerPacket::UserInformation(info) =
            ServerPacket::decode(index, &frame[4..]).expect("解码")
        {
            // 登录下发的是**存档里的真实任务物品栏**，不是空栏。
            assert!(
                info.quest_inventory.as_ref().is_some_and(|slots| slots
                    .iter()
                    .flatten()
                    .any(|i| i.unique_id == QUEST_ITEM_UID || i.item_index == QUEST_ITEM)),
                "UserInformation 必须下发存档里的任务物品栏"
            );
            object_id = Some(info.object_id);
        }
    }
    let object_id = object_id.expect("进世界应下发 UserInformation");
    {
        let guard = world.lock().await;
        let loaded = guard
            .quest_inventory(object_id)
            .expect("登录应装载任务物品栏");
        assert!(
            loaded
                .slots()
                .iter()
                .flatten()
                .any(|item| item.unique_id == QUEST_ITEM_UID),
            "世界侧任务物品栏应装载存档内容"
        );
    }

    // 商城购买：走 `flush_gameshop` → `Session::character_snapshot`（角色行整行落盘）。
    session
        .handle(mir_protocol::ClientPacket::GameshopBuy(c::GameshopBuy {
            g_index: G_POTION,
            quantity: 1,
            p_type: 0,
        }))
        .await
        .expect("处理购买");

    // 直接读库：任务物品栏还在（没有被写回登录值/空栏）。
    let row = store
        .find_character(account.id, character.id)
        .await
        .expect("查角色")
        .expect("角色存在");
    let saved =
        mir_server::quest_inventory::QuestInventory::decode(&row.quest_inventory).expect("解码");
    assert!(
        saved
            .slots()
            .iter()
            .flatten()
            .any(|item| item.unique_id == QUEST_ITEM_UID),
        "商城落盘不能覆盖任务物品栏"
    );

    // 下线（再落一次角色行）→ 重新登录：任务物品仍在。
    session.leave_world().await;
    let row = store
        .find_character(account.id, character.id)
        .await
        .expect("查角色")
        .expect("角色存在");
    let saved =
        mir_server::quest_inventory::QuestInventory::decode(&row.quest_inventory).expect("解码");
    assert!(
        saved
            .slots()
            .iter()
            .flatten()
            .any(|item| item.unique_id == QUEST_ITEM_UID),
        "下线存盘不能丢任务物品"
    );

    // 重新登录：新会话把存档里的任务物品栏装回世界。
    let (out, mut out_rx) = mpsc::unbounded_channel();
    let mut again = mir_server::session::Session::new(
        2,
        "test".to_owned(),
        Arc::new(mir_server::ServerConfig {
            map_root: "resource/不存在的目录".to_owned(),
            language_dir: "config/不存在的目录".to_owned(),
            load_game_data: false,
            ..mir_server::ServerConfig::default()
        }),
        Arc::clone(&store),
        Arc::clone(&world),
        out,
        pack,
    );
    again
        .handle(mir_protocol::ClientPacket::ClientVersion(
            c::ClientVersion {
                version_hash: Vec::new(),
            },
        ))
        .await
        .expect("版本校验");
    again
        .handle(mir_protocol::ClientPacket::Login(c::Login {
            account_id: "qinv".to_owned(),
            password: "secret".to_owned(),
        }))
        .await
        .expect("登录");
    again
        .handle(mir_protocol::ClientPacket::StartGame(c::StartGame {
            character_index: i32::try_from(character.id).unwrap_or(0),
        }))
        .await
        .expect("再次进世界");
    let mut relogin_id = None;
    while let Ok(frame) = out_rx.try_recv() {
        let index = i16::from_le_bytes([frame[2], frame[3]]);
        if let ServerPacket::UserInformation(info) =
            ServerPacket::decode(index, &frame[4..]).expect("解码")
        {
            relogin_id = Some(info.object_id);
        }
    }
    let relogin_id = relogin_id.expect("重登应下发 UserInformation");
    let guard = world.lock().await;
    let reloaded = guard
        .quest_inventory(relogin_id)
        .expect("重登应装载任务物品栏");
    assert!(
        reloaded
            .slots()
            .iter()
            .flatten()
            .any(|item| item.unique_id == QUEST_ITEM_UID),
        "重登后任务物品栏必须还在"
    );
}
