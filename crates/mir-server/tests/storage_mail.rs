//! 玩家仓库（`C.StoreItem` / `C.TakeBackItem`）与邮件（发送/阅读/领取/锁定/删除/过期）
//! 的端到端测试。
//!
//! 全部确定性：
//! * 世界随机数固定种子（`World::seed_rng`），邮件限速用 `World::advance_clock` 快进；
//! * 地形是内存里合成的 60×60 空地，NPC 与脚本在每次用例的独立临时目录里构造
//!   （`Envir::new`，退出时删除）；
//! * 时间相关只出现在「邮件过期」用例里：直接构造一封 40 天前的邮件，不依赖时钟节拍；
//! * 持久化用 [`MemoryStore`]（实现了 `AccountStore` 的仓库/邮件函数）：世界只产生
//!   `MailWrite` 队列，测试用 `persist_mail_writes` 写进 `MemoryStore` 再读回来断言；
//!   仓库则走 `characters.storage` 字节块，由会话在登录/下线时编解码（见
//!   `session_login_round_trips_storage`）。
//!
//! 关键 Crystal 源码：`Server/MirObjects/PlayerObject.cs:5376-5526`（仓库存取）、
//! `:11674-12000`（邮件）、`Server/MirDatabase/AccountInfo.cs:69`（仓库 80 格）、
//! `Server/MirObjects/NPC/NPCScript.cs:1002-1098`（仓库/写信/邮局页面）、
//! `Server/MirDatabase/MailInfo.cs`（邮件实体）。

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use mir_core::enums::{BindMode, ChatType, ItemType, MirClass, MirDirection, MirGender, Stat};
use mir_core::i18n::LanguageLibrary;
use mir_core::{DateTimeBinary, Point};
use mir_db::{MapInfo, MirDb, NpcInfo};
use mir_protocol::data::{ClientMail, ItemInfo, Stats, UserItem};
use mir_protocol::packets::client as c;
use mir_protocol::{ClientPacket, ServerPacket};
use mir_server::gamedata::GameData;
use mir_server::mail::{Mail, MailError, MailRecipient, MailState, MailWrite, persist_mail_writes};
use mir_server::npc::{COLLECT_PARCEL_KEY, SEND_PARCEL_KEY, STORAGE_KEY};
use mir_server::session::Session;
use mir_server::storage::StorageError;
use mir_server::{AccountStore, MemoryStore, NpcState, Player, ServerConfig, World, combat, items};
use mir_world::{Map, MapCell, MapVersion};
use tokio::sync::{Mutex, mpsc};

/// 测试地图（文件名 = 数据库里的 `file_name`）。
const MAP: &str = "0";
/// 地图数据库主键。
const MAP_INDEX: i32 = 10;
/// 仓库 NPC 的数据库主键。
const NPC_INDEX: i32 = 1;
/// 普通物品（可存/可邮寄）。
const ITEM_INDEX: i32 = 1;
/// `BindMode::DontStore` 的物品。
const BOUND_ITEM_INDEX: i32 = 2;
/// `BindMode::DontTrade` 的物品。
const NO_TRADE_ITEM_INDEX: i32 = 3;
/// `BindMode::NoMail` 的物品。
const NO_MAIL_ITEM_INDEX: i32 = 4;
/// 邮票：`ItemType.Nothing` + `Shape = 1`（`PlayerObject.cs:11776`）。
const STAMP_ITEM_INDEX: i32 = 5;
/// 定价 1000 的物品（邮件保险费用例用）。
const PRICEY_ITEM_INDEX: i32 = 6;

// ---------------------------------------------------------------------------
// 临时 Envir 目录与数据
// ---------------------------------------------------------------------------

/// 每个用例一个独立的 `Envir/` 目录，退出时删除（脚本按需惰性加载）。
struct Envir {
    root: PathBuf,
}

impl Envir {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "mir-next-storage-mail-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
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

    fn drop_dir(&self) -> PathBuf {
        self.root.join("Drops")
    }
}

impl Drop for Envir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// 仓库 NPC 的脚本：主页暴露仓库/写信/邮局三个按钮。
const NPC_SCRIPT: &str = "\
[@Main]\n\
#SAY\n\
这里是仓库管理员。\n\
<仓库/@Storage> <写信/@SendParcel> <邮局/@CollectParcel>\n\
\n\
[@Storage]\n\
#SAY\n\
仓库\n\
\n\
[@SendParcel]\n\
#SAY\n\
写信\n\
\n\
[@CollectParcel]\n\
#SAY\n\
邮局\n";

fn stats(pairs: &[(Stat, i32)]) -> Stats {
    Stats::from_iter(pairs.iter().copied())
}

fn item_info(index: i32, name: &str, item_type: ItemType, price: u32, bind: BindMode) -> ItemInfo {
    ItemInfo {
        index,
        name: name.to_owned(),
        item_type,
        bind,
        weight: 1,
        image: u16::try_from(index).unwrap_or(0),
        durability: 0,
        stack_size: 1,
        price,
        shape: 0,
        stats: stats(&[(Stat::MaxDC, 1)]),
        ..ItemInfo::default()
    }
}

fn database() -> MirDb {
    let mut stamp = item_info(
        STAMP_ITEM_INDEX,
        "邮票",
        ItemType::Nothing,
        10,
        BindMode::None,
    );
    stamp.shape = 1;
    MirDb {
        maps: vec![MapInfo {
            index: MAP_INDEX,
            file_name: MAP.to_owned(),
            title: "比奇省".to_owned(),
            ..MapInfo::default()
        }],
        npcs: vec![NpcInfo {
            index: NPC_INDEX,
            map_index: MAP_INDEX,
            file_name: "Test/Storage".to_owned(),
            name: "仓库管理员".to_owned(),
            location: Point::new(10, 10),
            image: 5,
            rate: 100,
            ..NpcInfo::default()
        }],
        items: vec![
            item_info(
                ITEM_INDEX,
                "测试之剑",
                ItemType::Weapon,
                100,
                BindMode::None,
            ),
            item_info(
                BOUND_ITEM_INDEX,
                "绑定之剑",
                ItemType::Weapon,
                100,
                BindMode::DontStore,
            ),
            item_info(
                NO_TRADE_ITEM_INDEX,
                "禁交易之剑",
                ItemType::Weapon,
                100,
                BindMode::DontTrade,
            ),
            item_info(
                NO_MAIL_ITEM_INDEX,
                "禁邮寄之剑",
                ItemType::Weapon,
                100,
                BindMode::NoMail,
            ),
            stamp,
            item_info(
                PRICEY_ITEM_INDEX,
                "昂贵之剑",
                ItemType::Weapon,
                1_000,
                BindMode::None,
            ),
        ],
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

/// 建世界：合地图 + 内存数据库 + 一个带脚本的仓库 NPC。
fn build_world(seed: u64, tag: &str) -> (World, Envir) {
    let envir = Envir::new(tag);
    envir.script("Test/Storage", NPC_SCRIPT);
    let data = GameData::from_db(database(), Some(envir.drop_dir()));
    let mut world = World::new(999);
    world.seed_rng(seed);
    world.insert_map(MAP, synthetic_map());
    world.set_game_data(Arc::new(data));
    let spawned = world.spawn_npcs(MAP);
    assert_eq!(spawned.len(), 1, "仓库 NPC 应该刷出来");
    (world, envir)
}

// ---------------------------------------------------------------------------
// 世界级脚手架
// ---------------------------------------------------------------------------

struct Harness {
    world: World,
    receivers: HashMap<u32, mpsc::UnboundedReceiver<Vec<u8>>>,
    npc_id: u32,
    next_uid: u64,
    _envir: Envir,
}

impl Harness {
    fn new(seed: u64, tag: &str) -> Self {
        let (world, envir) = build_world(seed, tag);
        let npc_id = world.npcs_on(MAP)[0].object_id;
        Self {
            world,
            receivers: HashMap::new(),
            npc_id,
            next_uid: 1,
            _envir: envir,
        }
    }

    fn add_player(&mut self, object_id: u32, character_id: i64, name: &str, gold: i64) -> u32 {
        let stats = combat::player_base_stats(10, MirClass::Warrior);
        let (sender, receiver) = mpsc::unbounded_channel();
        self.world.enter(Player {
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
            gold,
            credit: 0,
            visible_items: BTreeSet::new(),
            quests: Vec::new(),
            completed_quests: BTreeSet::new(),
            mail: MailState::default(),
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

    fn give_item(&mut self, object_id: u32, slot: usize, item_index: i32) -> u64 {
        let uid = self.next_uid;
        self.next_uid += 1;
        let item = UserItem {
            unique_id: uid,
            item_index,
            current_dura: 0,
            max_dura: 0,
            count: 1,
            identified: true,
            ..UserItem::default()
        };
        let player = self.world.player_mut(object_id).expect("玩家");
        player.inventory[slot] = Some(item);
        uid
    }

    /// 直接把玩家置于 `[@STORAGE]` 页面（跳过 NPC 脚本，等价于 `call_npc` 的后半段）。
    fn open_storage_dialog(&mut self, object_id: u32) {
        let npc_id = self.npc_id;
        let player = self.world.player_mut(object_id).expect("玩家");
        player.npc.npc_object_id = npc_id;
        STORAGE_KEY.clone_into(&mut player.npc.page_key);
    }
}

fn store_acks(packets: &[ServerPacket]) -> Vec<&mir_protocol::packets::server::StoreItem> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::StoreItem(ack) => Some(ack),
            _ => None,
        })
        .collect()
}

fn take_back_acks(packets: &[ServerPacket]) -> Vec<&mir_protocol::packets::server::TakeBackItem> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::TakeBackItem(ack) => Some(ack),
            _ => None,
        })
        .collect()
}

fn chats(packets: &[ServerPacket]) -> Vec<(&str, ChatType)> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::Chat(chat) => Some((chat.message.as_str(), chat.type_)),
            _ => None,
        })
        .collect()
}

fn mail_sent(packets: &[ServerPacket]) -> Vec<i8> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::MailSent(sent) => Some(sent.result),
            _ => None,
        })
        .collect()
}

fn delete_items(packets: &[ServerPacket]) -> Vec<(u64, u16)> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::DeleteItem(item) => Some((item.unique_id, item.count)),
            _ => None,
        })
        .collect()
}

fn lose_gold(packets: &[ServerPacket]) -> Vec<u32> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::LoseGold(lose) => Some(lose.gold),
            _ => None,
        })
        .collect()
}

fn receive_mails(packets: &[ServerPacket]) -> Vec<&Vec<ClientMail>> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::ReceiveMail(mail) => Some(&mail.mail),
            _ => None,
        })
        .collect()
}

fn parcel_collected(packets: &[ServerPacket]) -> Vec<i8> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::ParcelCollected(result) => Some(result.result),
            _ => None,
        })
        .collect()
}

fn gained_items(packets: &[ServerPacket]) -> Vec<u64> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::GainedItem(item) => Some(item.item.unique_id),
            _ => None,
        })
        .collect()
}

fn sample_item(unique_id: u64, item_index: i32) -> UserItem {
    UserItem {
        unique_id,
        item_index,
        current_dura: 0,
        max_dura: 0,
        count: 1,
        identified: true,
        ..UserItem::default()
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}

/// 构造一封测试邮件。
fn sample_mail(
    id: u64,
    character_id: i64,
    items: Vec<UserItem>,
    gold: u32,
    collected: bool,
) -> Mail {
    Mail {
        id,
        character_id,
        sender_name: "系统".to_owned(),
        subject: String::new(),
        message: "测试邮件".to_owned(),
        items,
        gold,
        date_sent: DateTimeBinary::from_unix_seconds(now_unix()),
        opened: false,
        locked: false,
        collected,
    }
}

// 调用点统一传 `Option<MailRecipient>`（「收件人不存在」用 `None`），助手故意包一层。
#[allow(clippy::unnecessary_wraps)]
fn recipient(character_id: i64, name: &str) -> Option<MailRecipient> {
    Some(MailRecipient {
        character_id,
        name: name.to_owned(),
        mail_count: 0,
    })
}

// ---------------------------------------------------------------------------
// 1~2. 仓库存取
// ---------------------------------------------------------------------------

#[test]
fn store_item_moves_into_storage_and_acks() {
    let mut harness = Harness::new(1, "store");
    let player = harness.add_player(1, 1, "阿甲", 0);
    harness.open_storage_dialog(player);
    let uid = harness.give_item(player, 3, ITEM_INDEX);

    assert_eq!(
        harness.world.storage_store_item(player, 3, 7),
        Ok(()),
        "仓库页内应能存物品"
    );

    let packets = harness.packets(player);
    let acks = store_acks(&packets);
    assert_eq!(acks.len(), 1, "应收到一条 S.StoreItem，实际 {packets:?}");
    assert!(acks[0].success);
    assert_eq!((acks[0].from, acks[0].to), (3, 7));

    let player_state = harness.world.player(player).expect("玩家");
    assert!(player_state.inventory[3].is_none(), "背包格应清空");
    assert_eq!(
        player_state.storage[7].as_ref().map(|item| item.unique_id),
        Some(uid),
        "仓库第 7 格应是刚存的物品"
    );
    assert_eq!(harness.world.storage(player).map(<[_]>::len), Some(80));
}

#[test]
fn store_item_requires_storage_dialog_npc_and_range() {
    let mut harness = Harness::new(2, "store-gates");
    let player = harness.add_player(1, 1, "阿甲", 0);
    harness.give_item(player, 0, ITEM_INDEX);

    // ---- 不在任何 NPC 对话里（`NPCPage == null`，`PlayerObject.cs:5380-5384`）----
    assert_eq!(
        harness.world.storage_store_item(player, 0, 0),
        Err(StorageError::NoStorageDialog)
    );
    let packets = harness.packets(player);
    assert_eq!(store_acks(&packets).len(), 1, "失败也要回 S.StoreItem");
    assert!(!store_acks(&packets)[0].success);

    // ---- 对话页不是 `[@STORAGE]`（比如买卖页）----
    {
        let npc_id = harness.npc_id;
        let player_state = harness.world.player_mut(player).expect("玩家");
        player_state.npc.npc_object_id = npc_id;
        player_state.npc.page_key = "[@BUYSELL]".to_owned();
    }
    harness.packets(player);
    assert_eq!(
        harness.world.storage_store_item(player, 0, 0),
        Err(StorageError::NoStorageDialog)
    );

    // ---- 在仓库页，但玩家离开了 NPC 的可交互范围（`Functions.InRange`，`:5393`）----
    harness.open_storage_dialog(player);
    harness.world.player_mut(player).expect("玩家").position = Point::new(50, 50);
    harness.packets(player);
    assert_eq!(
        harness.world.storage_store_item(player, 0, 0),
        Err(StorageError::OutOfRange {
            object_id: harness.npc_id,
            range: 16,
        })
    );

    // ---- 在仓库页、在范围内，但 NPCObjectID 指向一个不存在的 NPC ----
    harness.world.player_mut(player).expect("玩家").position = Point::new(10, 10);
    harness
        .world
        .player_mut(player)
        .expect("玩家")
        .npc
        .npc_object_id = 999;
    harness.packets(player);
    assert_eq!(
        harness.world.storage_store_item(player, 0, 0),
        Err(StorageError::NoSuchNpc { object_id: 999 })
    );

    // 失败不应动背包。
    assert_eq!(
        harness
            .world
            .player(player)
            .and_then(|state| state.inventory[0].as_ref())
            .map(|item| item.item_index),
        Some(ITEM_INDEX)
    );
}

#[test]
fn store_item_rejects_out_of_range_and_occupied_slots() {
    let mut harness = Harness::new(3, "store-slots");
    let player = harness.add_player(1, 1, "阿甲", 0);
    harness.open_storage_dialog(player);
    let uid = harness.give_item(player, 0, ITEM_INDEX);

    // `to >= Account.Storage.Length`（80 格，`AccountInfo.cs:69`）。
    assert_eq!(
        harness.world.storage_store_item(player, 0, 80),
        Err(StorageError::StorageIndex {
            index: 80,
            size: 80,
        })
    );
    harness.packets(player);
    assert_eq!(
        harness.world.storage_store_item(player, 0, 200),
        Err(StorageError::StorageIndex {
            index: 200,
            size: 80,
        })
    );

    // 目标格已被占用（`PlayerObject.cs:5443-5455`）。
    {
        let player_state = harness.world.player_mut(player).expect("玩家");
        player_state.storage[5] = Some(sample_item(999, ITEM_INDEX));
    }
    harness.packets(player);
    assert_eq!(
        harness.world.storage_store_item(player, 0, 5),
        Err(StorageError::TargetOccupied { index: 5 })
    );
    assert!(harness.world.player(player).expect("玩家").inventory[0].is_some());

    // 越界/占用的回执都是 Success=false。
    let packets = harness.packets(player);
    let acks = store_acks(&packets);
    assert_eq!(acks.len(), 1, "最后一次失败回执，实际 {packets:?}");
    assert!(!acks[0].success);

    // 背包下标越界。
    assert_eq!(
        harness.world.storage_store_item(player, 46, 0),
        Err(StorageError::InventoryIndex {
            index: 46,
            size: 46,
        })
    );

    // 正数控制：换到空仓库格就成功。
    assert_eq!(harness.world.storage_store_item(player, 0, 0), Ok(()));
    assert_eq!(
        harness
            .world
            .storage(player)
            .and_then(|slots| slots[0].as_ref())
            .map(|item| item.unique_id),
        Some(uid)
    );
}

#[test]
fn store_item_rejects_bound_items() {
    let mut harness = Harness::new(4, "store-bound");
    let player = harness.add_player(1, 1, "阿甲", 0);
    harness.open_storage_dialog(player);
    let uid = harness.give_item(player, 0, BOUND_ITEM_INDEX);

    assert_eq!(
        harness.world.storage_store_item(player, 0, 0),
        Err(StorageError::CannotStore { unique_id: uid })
    );
    let packets = harness.packets(player);
    assert!(!store_acks(&packets)[0].success);
    assert!(
        harness.world.player(player).expect("玩家").inventory[0].is_some(),
        "被拒后物品留在背包"
    );
}

// ---------------------------------------------------------------------------
// 3~4. 取回
// ---------------------------------------------------------------------------

#[test]
fn take_back_item_returns_to_inventory_and_rejects_full_bag() {
    let mut harness = Harness::new(5, "take-back");
    let player = harness.add_player(1, 1, "阿甲", 0);
    harness.open_storage_dialog(player);
    let uid = harness.give_item(player, 0, ITEM_INDEX);
    assert_eq!(harness.world.storage_store_item(player, 0, 9), Ok(()));
    harness.packets(player);

    // 成功的取回：`S.TakeBackItem{Success=true}`（`PlayerObject.cs:5519-5521`）。
    assert_eq!(harness.world.storage_take_back_item(player, 9, 5), Ok(()));
    let packets = harness.packets(player);
    let acks = take_back_acks(&packets);
    assert_eq!(acks.len(), 1, "实际 {packets:?}");
    assert!(acks[0].success);
    assert_eq!((acks[0].from, acks[0].to), (9, 5));
    assert_eq!(
        harness
            .world
            .player(player)
            .and_then(|state| state.inventory[5].as_ref())
            .map(|item| item.unique_id),
        Some(uid)
    );
    assert!(harness.world.storage(player).expect("仓库")[9].is_none());

    // 目标背包格被占用 → 拒绝（`if (Info.Inventory[to] == null)`，`:5512-5525`）。
    let other = harness.give_item(player, 6, ITEM_INDEX);
    {
        let player_state = harness.world.player_mut(player).expect("玩家");
        player_state.storage[9] = Some(sample_item(uid, ITEM_INDEX));
    }
    assert_eq!(
        harness.world.storage_take_back_item(player, 9, 6),
        Err(StorageError::TargetOccupied { index: 6 })
    );
    let packets = harness.packets(player);
    assert!(!take_back_acks(&packets)[0].success);
    assert_eq!(
        harness
            .world
            .player(player)
            .and_then(|state| state.inventory[6].as_ref())
            .map(|item| item.unique_id),
        Some(other)
    );

    // 源格是空的。
    assert_eq!(
        harness.world.storage_take_back_item(player, 11, 0),
        Err(StorageError::EmptySlot {
            grid: "仓库",
            index: 11,
        })
    );
}

// ---------------------------------------------------------------------------
// 5. 仓库列表下发（`S.UserStorage` 每个连接只发一次）
// ---------------------------------------------------------------------------

#[test]
fn storage_page_sends_user_storage_once_then_only_npc_storage() {
    let mut harness = Harness::new(6, "storage-page");
    let player = harness.add_player(1, 1, "阿甲", 0);
    harness.open_storage_dialog(player);
    let uid = harness.give_item(player, 0, ITEM_INDEX);
    assert_eq!(harness.world.storage_store_item(player, 0, 2), Ok(()));
    harness.packets(player);

    // 第一次打开：`S.UserStorage{Storage=80 格}` + `S.NPCStorage`（`PlayerObject.cs:8180`、
    // `NPCScript.cs:1004-1005`）。
    assert_eq!(harness.world.open_storage(player), Ok(()));
    let packets = harness.packets(player);
    let user_storage: Vec<_> = packets
        .iter()
        .filter_map(|packet| match packet {
            ServerPacket::UserStorage(storage) => Some(storage),
            _ => None,
        })
        .collect();
    assert_eq!(user_storage.len(), 1, "实际 {packets:?}");
    let slots = user_storage[0].storage.as_ref().expect("应有仓库数组");
    assert_eq!(slots.len(), 80);
    assert_eq!(slots[2].as_ref().map(|item| item.unique_id), Some(uid));
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::NPCStorage(_))),
        "应发 S.NPCStorage"
    );

    // 第二次打开：`if (Connection.StorageSent) return;`（`:8169-8170`）→ 只有 `S.NPCStorage`。
    assert_eq!(harness.world.open_storage(player), Ok(()));
    let packets = harness.packets(player);
    assert!(
        !packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::UserStorage(_))),
        "同一连接不应重复发仓库列表"
    );
    assert_eq!(
        packets
            .iter()
            .filter(|packet| matches!(packet, ServerPacket::NPCStorage(_)))
            .count(),
        1
    );
}

// ---------------------------------------------------------------------------
// 6. NPC 页面接线：`[@STORAGE]` / `[@SENDPARCEL]` / `[@COLLECTPARCEL]`
// ---------------------------------------------------------------------------

#[test]
fn npc_pages_open_storage_and_mail_panels() {
    let mut harness = Harness::new(7, "npc-pages");
    let player = harness.add_player(1, 1, "阿甲", 0);
    let npc_id = harness.npc_id;

    // `[@Main]` 先展开三个按钮（Crystal 的按钮白名单校验，`NPCScript.cs:833-856`）。
    harness
        .world
        .call_npc(player, npc_id, "[@MAIN]")
        .expect("主页应能打开");
    harness.packets(player);

    harness
        .world
        .call_npc(player, npc_id, STORAGE_KEY)
        .expect("仓库页应能打开");
    let packets = harness.packets(player);
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::NPCStorage(_))),
        "仓库页应发 S.NPCStorage，实际 {packets:?}"
    );

    harness
        .world
        .call_npc(player, npc_id, "[@MAIN]")
        .expect("主页应能再次打开");
    harness
        .world
        .call_npc(player, npc_id, SEND_PARCEL_KEY)
        .expect("写信页应能打开");
    let packets = harness.packets(player);
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::MailSendRequest(_))),
        "写信页应发 S.MailSendRequest，实际 {packets:?}"
    );

    harness
        .world
        .call_npc(player, npc_id, "[@MAIN]")
        .expect("主页应能再次打开");
    harness
        .world
        .call_npc(player, npc_id, COLLECT_PARCEL_KEY)
        .expect("邮局页应能打开");
    let packets = harness.packets(player);
    // 没有待受理的包裹 → `Result = -1`，但 `GetMail()` 照样下发（`NPCScript.cs:1085-1097`）。
    assert_eq!(parcel_collected(&packets), vec![-1]);
    assert_eq!(
        receive_mails(&packets).len(),
        1,
        "邮局页应补发 S.ReceiveMail"
    );
}

// ---------------------------------------------------------------------------
// 7. 邮件发送：成功路径与包序
// ---------------------------------------------------------------------------

#[test]
fn send_mail_with_attachment_and_gold_uses_stamp_and_acks() {
    let mut harness = Harness::new(8, "mail-send");
    let player = harness.add_player(1, 1, "阿甲", 10_000);
    let stamp_uid = harness.give_item(player, 0, STAMP_ITEM_INDEX);
    let item_uid = harness.give_item(player, 1, PRICEY_ITEM_INDEX);

    assert_eq!(
        harness.world.mail_send(
            player,
            recipient(2, "Bob"),
            "你好，附件是屠龙。",
            1_000,
            &[item_uid, 0, 0, 0, 0],
            true,
        ),
        Ok(())
    );
    harness.world.tick();

    let packets = harness.packets(player);
    // 包序：邮票 `S.DeleteItem{Count=1}` → 附件 `S.DeleteItem{Count=物品数}`
    // → `S.LoseGold{附件+邮费}` → `S.MailSent{1}`（`PlayerObject.cs:11769-11844`）。
    assert_eq!(
        delete_items(&packets),
        vec![(stamp_uid, 1), (item_uid, 1)],
        "实际 {packets:?}"
    );
    // 贴了邮票且 `MailFreeWithStamp = true`，邮费为 0，只扣附件里的 1000 金。
    assert_eq!(lose_gold(&packets), vec![1_000]);
    assert_eq!(mail_sent(&packets), vec![1]);

    assert_eq!(harness.world.player(player).expect("玩家").gold, 9_000);
    assert!(harness.world.player(player).expect("玩家").inventory[0].is_none());
    assert!(harness.world.player(player).expect("玩家").inventory[1].is_none());

    // 收件人离线：邮件只进待落盘队列（`MailInfo.Send` → `RecipientInfo.Mail.Add`，`MailInfo.cs:135`）。
    let writes = harness.world.take_mail_writes();
    assert_eq!(writes.len(), 1, "实际 {writes:?}");
    let MailWrite::Create(record) = &writes[0] else {
        panic!("第一笔应是新建邮件：{writes:?}");
    };
    assert_eq!(record.character_id, 2);
    assert_eq!(record.sender_name, "阿甲");
    assert_eq!(record.gold, 1_000, "邮件里的 Gold 只记附件金币，不含邮费");
    assert!(!record.items.is_empty(), "附件应编码进字节块");
    assert!(!record.collected, "包裹默认要先去邮局受理");
    let decoded = Mail::from_record(record).expect("附件解码");
    assert_eq!(decoded.items.len(), 1);
    assert_eq!(decoded.items[0].unique_id, item_uid);
}

#[test]
fn send_mail_unstamped_takes_only_the_first_attachment() {
    let mut harness = Harness::new(9, "mail-unstamped");
    let player = harness.add_player(1, 1, "阿甲", 1_000);
    let first = harness.give_item(player, 0, PRICEY_ITEM_INDEX);
    let second = harness.give_item(player, 1, PRICEY_ITEM_INDEX);

    assert_eq!(
        harness.world.mail_send(
            player,
            recipient(2, "Bob"),
            "两件",
            0,
            &[first, second, 0, 0, 0],
            false,
        ),
        Ok(())
    );
    let packets = harness.packets(player);
    // 只动了第一件（`for (int j = 0; j < (hasStamp ? 5 : 1); j++)`，`PlayerObject.cs:11790`）。
    assert_eq!(delete_items(&packets), vec![(first, 1)]);
    // 没贴邮票 → 收 5% 保险费：floor(1000 * 5 / 100) = 50（`GetMailCost`，`:11937-11950`）。
    assert_eq!(lose_gold(&packets), vec![50]);
    assert_eq!(mail_sent(&packets), vec![1]);

    let writes = harness.world.take_mail_writes();
    let MailWrite::Create(record) = &writes[0] else {
        panic!("应新建邮件");
    };
    let decoded = Mail::from_record(record).expect("解码");
    assert_eq!(decoded.items.len(), 1);
    assert_eq!(decoded.items[0].unique_id, first);
    assert!(
        harness.world.player(player).expect("玩家").inventory[1].is_some(),
        "第二件应留在背包"
    );

    // 限速窗口内先等过去，再验附件数量防线。
    harness.world.advance_clock(10_000);
    // 本作 API 的防线：附件超过协议上限（`ItemsIdx[5]`）直接拒绝。
    assert_eq!(
        harness.world.mail_send(
            player,
            recipient(2, "Bob"),
            "六件",
            0,
            &[1, 2, 3, 4, 5, 6],
            true,
        ),
        Err(MailError::TooManyAttachments { count: 6 })
    );
}

#[test]
fn send_mail_rejects_missing_recipient_and_insufficient_gold() {
    let mut harness = Harness::new(10, "mail-reject");
    let player = harness.add_player(1, 1, "阿甲", 500);

    // 收件人不存在（`ServerTextKeys.CouldNotFindPlayer`，`PlayerObject.cs:11743-11749`）。
    assert_eq!(
        harness
            .world
            .mail_send(player, None, "你好", 0, &[0; 5], false),
        Err(MailError::RecipientMissing)
    );
    let packets = harness.packets(player);
    assert!(
        chats(&packets)
            .iter()
            .any(|(text, kind)| *kind == ChatType::System && text.contains("找不到该玩家")),
        "实际 {packets:?}"
    );
    assert!(
        mail_sent(&packets).is_empty(),
        "收件人不存在不发 S.MailSent"
    );

    // 每次发送都会重置 `NextMailTime`（10 秒），校验下一项前先把时钟推过去。
    harness.world.advance_clock(10_000);

    // 金币不够：附件 1000 + 邮费 floor(1000/1000)*100 = 1100 > 500
    // （`if (Account.Gold < totalGold ...) Enqueue(new S.MailSent { Result = -1 })`，`:11763-11767`）。
    assert_eq!(
        harness
            .world
            .mail_send(player, recipient(2, "Bob"), "给你钱", 1_000, &[0; 5], false),
        Err(MailError::NotEnoughGold {
            needed: 1_100,
            held: 500,
        })
    );
    let packets = harness.packets(player);
    assert_eq!(mail_sent(&packets), vec![-1]);
    assert!(lose_gold(&packets).is_empty(), "钱不够不应该扣钱");

    // 收件人邮箱满：Crystal 的判定是 `> 50`（`:11751`）。
    harness.world.advance_clock(10_000);
    let mut full = recipient(2, "Bob");
    if let Some(entry) = full.as_mut() {
        entry.mail_count = 51;
    }
    assert_eq!(
        harness
            .world
            .mail_send(player, full, "满了", 0, &[0; 5], false),
        Err(MailError::RecipientMailboxFull)
    );
    let packets = harness.packets(player);
    assert!(
        chats(&packets)
            .iter()
            .any(|(text, _)| text.contains("邮箱已满")),
        "实际 {packets:?}"
    );
}

#[test]
fn send_mail_rejects_unmailable_items_and_rate_limits() {
    let mut harness = Harness::new(11, "mail-bind");
    let player = harness.add_player(1, 1, "阿甲", 10_000);
    let no_mail = harness.give_item(player, 0, NO_MAIL_ITEM_INDEX);
    let no_trade = harness.give_item(player, 1, NO_TRADE_ITEM_INDEX);

    // `BindMode.NoMail`：提示 + `S.MailSent{-1}`（`PlayerObject.cs:11806-11811`）。
    assert_eq!(
        harness.world.mail_send(
            player,
            recipient(2, "Bob"),
            "禁邮寄",
            0,
            &[no_mail, 0, 0, 0, 0],
            false
        ),
        Err(MailError::CannotMail { unique_id: no_mail })
    );
    let packets = harness.packets(player);
    assert_eq!(mail_sent(&packets), vec![-1]);
    assert!(
        chats(&packets)
            .iter()
            .any(|(text, _)| text.contains("不能邮寄")),
        "实际 {packets:?}"
    );
    assert!(
        harness.world.player(player).expect("玩家").inventory[0].is_some(),
        "被拒后物品应留在背包"
    );

    // `BindMode.DontTrade`：只提示，不发 `S.MailSent`（`:11800-11804`）。
    harness.world.advance_clock(10_000);
    assert_eq!(
        harness.world.mail_send(
            player,
            recipient(2, "Bob"),
            "禁交易",
            0,
            &[no_trade, 0, 0, 0, 0],
            false
        ),
        Err(MailError::CannotMail {
            unique_id: no_trade
        })
    );
    let packets = harness.packets(player);
    assert!(mail_sent(&packets).is_empty(), "实际 {packets:?}");

    // 限速：`NextMailTime = Envir.Time + 10000`，期间静默丢弃（`:11676-11680`）。
    harness.world.advance_clock(10_000);
    assert_eq!(
        harness
            .world
            .mail_send(player, recipient(2, "Bob"), "第一封", 0, &[0; 5], false),
        Ok(())
    );
    harness.packets(player);
    assert_eq!(
        harness
            .world
            .mail_send(player, recipient(2, "Bob"), "太快了", 0, &[0; 5], false),
        Err(MailError::RateLimited)
    );
    let packets = harness.packets(player);
    assert!(
        packets.is_empty(),
        "限速时 Crystal 连错误都不回，实际 {packets:?}"
    );

    // 等过 10 秒就恢复。
    harness.world.advance_clock(10_000);
    assert_eq!(
        harness
            .world
            .mail_send(player, recipient(2, "Bob"), "第二封", 0, &[0; 5], false),
        Ok(())
    );
}

#[test]
fn letter_without_attachments_is_collected_and_notifies_online_recipient() {
    let mut harness = Harness::new(12, "mail-letter");
    let sender = harness.add_player(1, 1, "阿甲", 100);
    let bob = harness.add_player(2, 2, "Bob", 0);
    harness.packets(sender);

    assert_eq!(
        harness
            .world
            .mail_send(sender, recipient(2, "Bob"), "只聊天", 0, &[0; 5], false),
        Ok(())
    );
    let packets = harness.packets(sender);
    assert!(delete_items(&packets).is_empty(), "纯信件没有 S.DeleteItem");
    assert!(lose_gold(&packets).is_empty(), "纯信件没有 S.LoseGold");
    assert_eq!(mail_sent(&packets), vec![1]);

    let writes = harness.world.take_mail_writes();
    let MailWrite::Create(record) = &writes[0] else {
        panic!("应新建邮件");
    };
    assert!(
        record.collected,
        "非包裹的 Collected 初值为 true（MailInfo.cs:99-143）"
    );

    // 在线收件人：`RecipientInfo.Player.NewMail = true`（`MailInfo.cs:137-140`），
    // 下一 tick 收到系统聊天 + `S.ReceiveMail`（`PlayerObject.cs:499-503`）。
    assert_eq!(harness.world.mailbox(bob).map(<[_]>::len), Some(1));
    harness.world.tick();
    let packets = harness.packets(bob);
    assert!(
        chats(&packets)
            .iter()
            .any(|(text, kind)| *kind == ChatType::System && text.contains("新邮件")),
        "实际 {packets:?}"
    );
    let lists = receive_mails(&packets);
    assert_eq!(lists.len(), 1);
    assert_eq!(lists[0].len(), 1);
    assert_eq!(lists[0][0].sender_name, "阿甲");
    assert_eq!(lists[0][0].message, "只聊天");
}

// ---------------------------------------------------------------------------
// 8. 阅读：`C.ReadMail` 只做「标记已读 + 重发列表」
// ---------------------------------------------------------------------------

#[test]
fn read_mail_marks_opened_and_resends_the_list() {
    let mut harness = Harness::new(13, "mail-read");
    let player = harness.add_player(1, 1, "阿甲", 0);
    let mail = sample_mail(41, 1, Vec::new(), 0, true);
    harness.world.insert_mail(player, mail);

    assert_eq!(harness.world.mail_read(player, 41), Ok(()));
    let packets = harness.packets(player);
    let lists = receive_mails(&packets);
    assert_eq!(lists.len(), 1, "实际 {packets:?}");
    assert!(lists[0][0].opened, "ReadMail 应把 Opened 置真（:11853）");
    assert_eq!(lists[0][0].mail_id, 41);
    assert_eq!(lists[0][0].message, "测试邮件");

    let writes = harness.world.take_mail_writes();
    assert_eq!(writes.len(), 1);
    assert!(
        matches!(&writes[0], MailWrite::Save(record) if record.opened),
        "应把已读状态排进落盘队列"
    );

    // 不存在的邮件：Crystal 静默返回（`:11851`）。
    assert_eq!(
        harness.world.mail_read(player, 999),
        Err(MailError::NotFound { mail_id: 999 })
    );
    assert!(harness.packets(player).is_empty());
}

#[test]
fn mailbox_list_keeps_only_the_last_hundred_mails() {
    let mut harness = Harness::new(14, "mail-capacity");
    let player = harness.add_player(1, 1, "阿甲", 0);
    for id in 1..=105 {
        harness
            .world
            .insert_mail(player, sample_mail(id, 1, Vec::new(), 0, true));
    }
    let packet = harness.world.mail_list_packet(player).expect("邮箱包");
    let ServerPacket::ReceiveMail(receive) = packet else {
        panic!("应是 S.ReceiveMail");
    };
    assert_eq!(
        receive.mail.len(),
        100,
        "MailCapacity = 100（Settings.cs:247）"
    );
    assert_eq!(receive.mail[0].mail_id, 6, "只保留最后 100 封");
    assert_eq!(receive.mail[99].mail_id, 105);
    // `insert_mail` 应按 MailID 升序维护列表。
    assert_eq!(
        harness
            .world
            .mailbox(player)
            .map(|mails| mails.iter().map(|mail| mail.id).collect::<Vec<_>>()),
        Some((1..=105).collect())
    );
}

// ---------------------------------------------------------------------------
// 9. 包裹受理与领取附件
// ---------------------------------------------------------------------------

#[test]
fn collect_requires_post_office_acceptance() {
    let mut harness = Harness::new(15, "mail-collect-gate");
    let player = harness.add_player(1, 1, "阿甲", 0);
    let item = sample_item(700, ITEM_INDEX);
    harness
        .world
        .insert_mail(player, sample_mail(51, 1, vec![item], 0, false));

    assert_eq!(
        harness.world.mail_collect(player, 51),
        Err(MailError::NotCollected)
    );
    let packets = harness.packets(player);
    assert!(
        chats(&packets)
            .iter()
            .any(|(text, _)| text.contains("邮局受理")),
        "实际 {packets:?}"
    );
    assert!(gained_items(&packets).is_empty());
    assert!(harness.world.player(player).expect("玩家").inventory[0].is_none());
}

#[test]
fn post_office_page_accepts_parcels_then_collect_moves_them_to_bag() {
    let mut harness = Harness::new(16, "mail-collect");
    let player = harness.add_player(1, 1, "阿甲", 0);
    let item_uid = 701;
    harness.world.insert_mail(
        player,
        sample_mail(61, 1, vec![sample_item(item_uid, ITEM_INDEX)], 250, false),
    );
    // 纯信件不参与「受理」（Crystal 的 `if (mail.Parcel)`，`NPCScript.cs:1091-1094`）。
    harness
        .world
        .insert_mail(player, sample_mail(62, 1, Vec::new(), 0, true));

    // 邮局页：有包裹 → `Result = 0`，并把包裹标成已受理。
    assert_eq!(harness.world.mail_collect_parcel_page(player), Ok(0));
    let packets = harness.packets(player);
    assert_eq!(parcel_collected(&packets), vec![0]);
    assert_eq!(receive_mails(&packets).len(), 1);
    assert!(
        harness
            .world
            .mail(player, 61)
            .is_some_and(|mail| mail.collected),
        "包裹应被受理"
    );

    // 再次进邮局页：没有待受理的 → `Result = -1`。
    assert_eq!(harness.world.mail_collect_parcel_page(player), Ok(-1));
    harness.packets(player);

    // 领取：`S.GainedItem` → `S.GainedGold` → `S.ParcelCollected{1}` → `S.ReceiveMail`
    // （`PlayerObject.cs:11878-11901`）。
    assert_eq!(harness.world.mail_collect(player, 61), Ok(()));
    let packets = harness.packets(player);
    assert_eq!(gained_items(&packets), vec![item_uid]);
    assert_eq!(
        packets
            .iter()
            .filter_map(|packet| match packet {
                ServerPacket::GainedGold(gain) => Some(gain.gold),
                _ => None,
            })
            .collect::<Vec<_>>(),
        vec![250]
    );
    assert_eq!(parcel_collected(&packets), vec![1]);
    assert_eq!(receive_mails(&packets).len(), 1);

    let player_state = harness.world.player(player).expect("玩家");
    assert_eq!(player_state.gold, 250, "附件金币进背包");
    assert!(
        player_state
            .inventory
            .iter()
            .flatten()
            .any(|item| item.unique_id == item_uid),
        "附件物品进背包"
    );
    let mail = harness.world.mail(player, 61).expect("邮件还在");
    assert!(mail.items.is_empty() && mail.gold == 0, "附件应清空");
    assert!(mail.collected, "领取后仍是已受理状态");

    // 领取后这封邮件就可以删了（`DeleteMail`，`:11904-11913`）。
    assert_eq!(harness.world.mail_delete(player, 61), Ok(()));
    assert!(
        harness
            .world
            .mailbox(player)
            .expect("邮箱")
            .iter()
            .all(|mail| mail.id != 61)
    );
}

#[test]
fn collect_rejects_when_bag_is_full() {
    let mut harness = Harness::new(17, "mail-collect-full");
    let player = harness.add_player(1, 1, "阿甲", 0);
    // 46 格背包全部塞满（`INVENTORY_SIZE`）。
    for slot in 0..items::INVENTORY_SIZE {
        harness.give_item(player, slot, ITEM_INDEX);
    }
    harness.world.insert_mail(
        player,
        sample_mail(71, 1, vec![sample_item(800, ITEM_INDEX)], 0, true),
    );

    assert_eq!(
        harness.world.mail_collect(player, 71),
        Err(MailError::InventoryFull)
    );
    let packets = harness.packets(player);
    assert!(
        chats(&packets)
            .iter()
            .any(|(text, _)| text.contains("背包已满")),
        "实际 {packets:?}"
    );
    assert!(
        harness
            .world
            .mail(player, 71)
            .is_some_and(|mail| mail.items.len() == 1),
        "领取失败不应清空附件"
    );
}

// ---------------------------------------------------------------------------
// 10. 锁定与删除保护
// ---------------------------------------------------------------------------

#[test]
fn locked_mail_cannot_be_deleted_until_unlocked() {
    let mut harness = Harness::new(18, "mail-lock");
    let player = harness.add_player(1, 1, "阿甲", 0);
    harness
        .world
        .insert_mail(player, sample_mail(81, 1, Vec::new(), 0, true));

    assert_eq!(harness.world.mail_lock(player, 81, true), Ok(()));
    let packets = harness.packets(player);
    let lists = receive_mails(&packets);
    assert_eq!(lists.len(), 1, "LockMail 会重发列表（`:11921-11923`）");
    assert!(lists[0][0].locked);
    harness.packets(player);

    assert_eq!(
        harness.world.mail_delete(player, 81),
        Err(MailError::Locked)
    );
    let packets = harness.packets(player);
    assert!(
        chats(&packets)
            .iter()
            .any(|(text, _)| text.contains("已锁定")),
        "实际 {packets:?}"
    );
    assert!(harness.world.mail(player, 81).is_some());

    // 解锁后可以删。
    assert_eq!(harness.world.mail_lock(player, 81, false), Ok(()));
    harness.packets(player);
    assert_eq!(harness.world.mail_delete(player, 81), Ok(()));
    let packets = harness.packets(player);
    assert_eq!(receive_mails(&packets)[0].len(), 0);
    let writes = harness.world.take_mail_writes();
    assert!(
        writes
            .iter()
            .any(|write| matches!(write, MailWrite::Delete { mail_id } if *mail_id == 81)),
        "删除应排队落盘：{writes:?}"
    );
}

#[test]
fn mail_with_uncollected_attachments_cannot_be_deleted() {
    let mut harness = Harness::new(19, "mail-delete-guard");
    let player = harness.add_player(1, 1, "阿甲", 0);
    harness.world.insert_mail(
        player,
        sample_mail(91, 1, vec![sample_item(900, ITEM_INDEX)], 0, false),
    );

    // 还没去邮局受理。
    assert_eq!(
        harness.world.mail_delete(player, 91),
        Err(MailError::UncollectedAttachments)
    );
    // 受理之后附件还在，同样不能删。
    assert_eq!(harness.world.mail_collect_parcel_page(player), Ok(0));
    harness.packets(player);
    assert_eq!(
        harness.world.mail_delete(player, 91),
        Err(MailError::UncollectedAttachments)
    );
    // 领取之后就可以删。
    assert_eq!(harness.world.mail_collect(player, 91), Ok(()));
    harness.packets(player);
    assert_eq!(harness.world.mail_delete(player, 91), Ok(()));
}

// ---------------------------------------------------------------------------
// 11. 询价 / 附件格子锁回显
// ---------------------------------------------------------------------------

#[test]
fn mail_cost_request_matches_crystal_formula() {
    let mut harness = Harness::new(20, "mail-cost");
    let player = harness.add_player(1, 1, "阿甲", 10_000);
    let uid = harness.give_item(player, 0, PRICEY_ITEM_INDEX);

    // 附件金币 2500 → floor(2500/1000) * 100 = 200；物品 1000 → floor(1000*5/100) = 50。
    harness
        .world
        .mail_cost_request(player, &[uid, 0, 0, 0, 0], 2_500, false);
    let packet = harness.packets(player).pop().expect("应有 S.MailCost");
    match packet {
        ServerPacket::MailCost(cost) => assert_eq!(cost.cost, 250),
        other => panic!("期望 MailCost，收到 {other:?}"),
    }

    // 贴邮票且 MailFreeWithStamp → 0（`GetMailCost` 的第一行，`:11930`）。
    harness
        .world
        .mail_cost_request(player, &[uid, 0, 0, 0, 0], 2_500, true);
    let packet = harness.packets(player).pop().expect("应有 S.MailCost");
    match packet {
        ServerPacket::MailCost(cost) => assert_eq!(cost.cost, 0),
        other => panic!("期望 MailCost，收到 {other:?}"),
    }

    // 999 金不足一千 → 只算物品保险；空附件时免费。
    harness.world.mail_cost_request(player, &[0; 5], 999, false);
    let packet = harness.packets(player).pop().expect("应有 S.MailCost");
    match packet {
        ServerPacket::MailCost(cost) => assert_eq!(cost.cost, 0),
        other => panic!("期望 MailCost，收到 {other:?}"),
    }
}

#[test]
fn mail_locked_item_is_echoed() {
    let mut harness = Harness::new(21, "mail-locked-item");
    let player = harness.add_player(1, 1, "阿甲", 0);
    harness.world.mail_locked_item(player, 12345, true);
    let packet = harness.packets(player).pop().expect("应有回显");
    match packet {
        ServerPacket::MailLockedItem(item) => {
            assert_eq!(item.unique_id, 12_345);
            assert!(item.locked);
        }
        other => panic!("期望 MailLockedItem，收到 {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 12. 落盘：`MailWrite` → `MemoryStore`
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mail_writes_persist_and_reload_through_memory_store() {
    let store = MemoryStore::new();
    let account = store
        .create_account("diana", "secret", "")
        .await
        .expect("建号");
    let recipient_character = store
        .create_character(
            account.id,
            "Bob",
            MirClass::Warrior,
            MirGender::Male,
            MAP,
            (10, 10),
        )
        .await
        .expect("建角");

    let mut harness = Harness::new(22, "mail-persist");
    let sender = harness.add_player(1, 1, "阿甲", 10_000);
    let uid = harness.give_item(sender, 0, PRICEY_ITEM_INDEX);
    assert_eq!(
        harness.world.mail_send(
            sender,
            recipient(recipient_character.id, "Bob"),
            "附件给你",
            500,
            &[uid, 0, 0, 0, 0],
            false,
        ),
        Ok(())
    );

    let writes = harness.world.take_mail_writes();
    persist_mail_writes(&store, writes).await.expect("邮件落盘");

    let rows = store.mails().await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].character_id, recipient_character.id);
    assert_eq!(rows[0].sender_name, "阿甲");
    assert_eq!(rows[0].message, "附件给你");
    assert_eq!(rows[0].gold, 500);
    assert!(!rows[0].collected);
    let decoded = Mail::from_record(&rows[0]).expect("附件解码");
    assert_eq!(decoded.items.len(), 1);
    assert_eq!(decoded.items[0].unique_id, uid);

    // 收件人离线时不会插进任何运行期邮箱；换上线的世界后按 `list_mail` 装载。
    let loaded = store
        .list_mail(recipient_character.id)
        .await
        .expect("读邮箱");
    assert_eq!(loaded.len(), 1);

    // 已读/锁定状态经 `save_mail` 往返。
    let mut updated = rows[0].clone();
    updated.opened = true;
    updated.locked = true;
    store.save_mail(updated.clone()).await.expect("回写邮件");
    let reloaded = store.list_mail(recipient_character.id).await.expect("再读");
    assert!(reloaded[0].opened && reloaded[0].locked);
    store.delete_mail(reloaded[0].id).await.expect("删除");
    assert!(store.mails().await.is_empty());
}

// ---------------------------------------------------------------------------
// 13. 会话级：登录/重登/过期/按名查收件人
// ---------------------------------------------------------------------------

/// 会话级脚手架：真的 `Session::handle` 走 版本 → 登录 → 进世界。
struct Live {
    session: Session<MemoryStore>,
    world: Arc<Mutex<World>>,
    store: Arc<MemoryStore>,
    rx: mpsc::UnboundedReceiver<Vec<u8>>,
    sender_id: i64,
    recipient_id: i64,
    object_id: u32,
    _envir: Envir,
}

impl Live {
    /// 建账号 + 两个角色（阿甲 / Bob），完成登录但**还没**进世界。
    async fn login(seed: u64, tag: &str) -> Self {
        let (world, envir) = build_world(seed, tag);
        let store = Arc::new(MemoryStore::new());
        let account = store
            .create_account("diana", "secret", "")
            .await
            .expect("建号");
        let sender = store
            .create_character(
                account.id,
                "阿甲",
                MirClass::Warrior,
                MirGender::Male,
                MAP,
                (10, 10),
            )
            .await
            .expect("建角（发送者）");
        let recipient = store
            .create_character(
                account.id,
                "Bob",
                MirClass::Warrior,
                MirGender::Male,
                MAP,
                (11, 10),
            )
            .await
            .expect("建角（收件人）");
        let world = Arc::new(Mutex::new(world));
        let (tx, rx) = mpsc::unbounded_channel();
        let config = Arc::new(ServerConfig {
            load_game_data: false,
            map_root: "resource/不存在的目录".to_owned(),
            ..ServerConfig::default()
        });
        let mut session = Session::new(
            1,
            "test".to_owned(),
            config,
            Arc::clone(&store),
            Arc::clone(&world),
            tx,
            Arc::new(LanguageLibrary::embedded()),
        );
        // 测试配置不校验版本，但流程仍要先过 `ClientVersion` 才进 Login 阶段。
        session
            .handle(ClientPacket::ClientVersion(c::ClientVersion {
                version_hash: Vec::new(),
            }))
            .await
            .expect("版本");
        session
            .handle(ClientPacket::Login(c::Login {
                account_id: "diana".to_owned(),
                password: "secret".to_owned(),
            }))
            .await
            .expect("登录");
        Self {
            session,
            world,
            store,
            rx,
            sender_id: sender.id,
            recipient_id: recipient.id,
            object_id: 0,
            _envir: envir,
        }
    }

    /// `C.StartGame`：进入世界，返回对象 ID。
    async fn start_game(&mut self) -> u32 {
        self.session
            .handle(ClientPacket::StartGame(c::StartGame {
                character_index: i32::try_from(self.sender_id).expect("角色 ID 适合 i32"),
            }))
            .await
            .expect("进世界");
        let world = self.world.lock().await;
        let object_id = world
            .player_by_name("阿甲")
            .map(|player| player.object_id)
            .expect("进世界后应有玩家");
        drop(world);
        self.object_id = object_id;
        object_id
    }

    fn packets(&mut self) -> Vec<ServerPacket> {
        let mut packets = Vec::new();
        while let Ok(frame) = self.rx.try_recv() {
            let index = i16::from_le_bytes([frame[2], frame[3]]);
            packets.push(ServerPacket::decode(index, &frame[4..]).expect("解码服务端包"));
        }
        packets
    }

    async fn give_item(&self, slot: usize, item_index: i32) -> u64 {
        let mut world = self.world.lock().await;
        let uid = 50_000 + u64::try_from(slot).unwrap_or(0);
        world.player_mut(self.object_id).expect("玩家").inventory[slot] =
            Some(sample_item(uid, item_index));
        uid
    }

    async fn logout(&mut self) {
        self.session
            .handle(ClientPacket::LogOut(c::LogOut {}))
            .await
            .expect("退出登录");
    }
}

#[tokio::test]
async fn session_login_round_trips_storage() {
    let mut live = Live::login(30, "live-storage").await;
    let object_id = live.start_game().await;

    // 打开仓库对话框（直接置 NPC 页面状态，脚本接线已在 `npc_pages_open_storage_and_mail_panels` 覆盖）。
    {
        let mut world = live.world.lock().await;
        let npc_id = world.npcs_on(MAP)[0].object_id;
        let player = world.player_mut(object_id).expect("玩家");
        player.npc.npc_object_id = npc_id;
        STORAGE_KEY.clone_into(&mut player.npc.page_key);
    }
    let uid = live.give_item(0, ITEM_INDEX).await;
    {
        let mut world = live.world.lock().await;
        assert_eq!(world.storage_store_item(object_id, 0, 12), Ok(()));
    }
    let packets = live.packets();
    assert!(
        packets
            .iter()
            .any(|packet| matches!(packet, ServerPacket::StoreItem(ack) if ack.success))
    );

    // 下线：会话把仓库编码进 `characters.storage` 并整行存盘。
    live.logout().await;
    let stored = live
        .store
        .character_storage(live.sender_id)
        .await
        .expect("角色应有仓库字节块");
    assert!(!stored.is_empty(), "仓库字节块应非空（-1 点下线已落盘）");
    let decoded = mir_server::storage::decode_storage(&stored).expect("解码");
    assert_eq!(
        decoded[12].as_ref().map(|item| item.unique_id),
        Some(uid),
        "落盘的第 12 格应是刚存的物品"
    );

    // 重登：`StartGame` 重新读角色行并解码仓库。
    let object_id = live.start_game().await;
    let world = live.world.lock().await;
    let player = world.player(object_id).expect("玩家");
    assert_eq!(
        player.storage[12].as_ref().map(|item| item.unique_id),
        Some(uid),
        "重登后仓库应还在"
    );
    assert_eq!(player.storage.len(), 80);
}

#[tokio::test]
async fn session_login_sends_mail_list_and_filters_expired_mail() {
    let mut live = Live::login(31, "live-mail").await;
    // 登录前先往收件箱里放两封：一封新的、一封 40 天前的（扩展的 30 天过期）。
    let fresh = sample_mail(1_000, live.sender_id, Vec::new(), 0, true);
    let mut old = sample_mail(999, live.sender_id, Vec::new(), 0, true);
    old.date_sent = DateTimeBinary::from_unix_seconds(now_unix() - 40 * 24 * 60 * 60);
    live.store
        .create_mail(fresh.to_record())
        .await
        .expect("存新邮件");
    live.store
        .create_mail(old.to_record())
        .await
        .expect("存旧邮件");
    assert_eq!(live.store.mails().await.len(), 2);

    let _object_id = live.start_game().await;
    let packets = live.packets();
    // 登录包序：`UserInformation` 之后才有 `S.ReceiveMail`（Crystal `PlayerObject.cs:1190`）。
    let user_info = packets
        .iter()
        .position(|packet| matches!(packet, ServerPacket::UserInformation(_)))
        .expect("应有 UserInformation");
    let mail_at = packets
        .iter()
        .position(|packet| matches!(packet, ServerPacket::ReceiveMail(_)))
        .expect("应有 ReceiveMail");
    assert!(
        user_info < mail_at,
        "邮箱包应排在 UserInformation 之后：{packets:?}"
    );
    let lists = receive_mails(&packets);
    assert_eq!(lists.len(), 1);
    assert_eq!(lists[0].len(), 1, "过期邮件不应出现在收件箱");
    assert_eq!(lists[0][0].mail_id, 1_000);

    // 过期邮件应被顺手删除。
    let rows = live.store.mails().await;
    assert_eq!(rows.len(), 1, "过期邮件应从 mail 表删除");
    assert_eq!(rows[0].id, 1_000);
}

#[tokio::test]
async fn session_send_mail_to_offline_character_by_name() {
    let mut live = Live::login(32, "live-send").await;
    let object_id = live.start_game().await;
    {
        let mut world = live.world.lock().await;
        world.player_mut(object_id).expect("玩家").gold = 10_000;
    }
    let uid = live.give_item(0, PRICEY_ITEM_INDEX).await;
    live.packets();

    live.session
        .handle(ClientPacket::SendMail(c::SendMail {
            name: "bob".to_owned(), // 大小写不敏感（`Envir.GetCharacterInfo`，`Envir.cs:4712-4719`）
            message: "离线也能收".to_owned(),
            gold: 100,
            items_idx: [uid, 0, 0, 0, 0],
            stamped: false,
        }))
        .await
        .expect("发送邮件");
    let packets = live.packets();
    assert_eq!(mail_sent(&packets), vec![1], "实际 {packets:?}");
    // 扣的是「附件金币 + 邮费」：附件 1000 的保险费 50 + 附件金币 100
    // （`totalGold = gold + parcelCost`，`PlayerObject.cs:11759-11767`、`:11826-11830`）。
    assert_eq!(lose_gold(&packets), vec![150]);

    let rows = live.store.mails().await;
    assert_eq!(rows.len(), 1, "邮件应落进收件人的 mail 行");
    assert_eq!(rows[0].character_id, live.recipient_id);
    assert_eq!(rows[0].sender_name, "阿甲");
    assert_eq!(rows[0].gold, 100);

    // 第二次发送前推过限速窗口。
    {
        let mut world = live.world.lock().await;
        world.advance_clock(10_000);
    }

    // 收件人不存在：系统聊天提示，不落库。
    live.session
        .handle(ClientPacket::SendMail(c::SendMail {
            name: "查无此人".to_owned(),
            message: "在吗".to_owned(),
            gold: 0,
            items_idx: [0; 5],
            stamped: false,
        }))
        .await
        .expect("发送邮件");
    let packets = live.packets();
    assert!(
        chats(&packets)
            .iter()
            .any(|(text, kind)| *kind == ChatType::System && text.contains("找不到该玩家")),
        "实际 {packets:?}"
    );
    assert_eq!(live.store.mails().await.len(), 1);
}

// ---------------------------------------------------------------------------
// 14. 真实数据冒烟：解析带 `[@STORAGE]` 的脚本
// ---------------------------------------------------------------------------

#[test]
fn real_npc_scripts_with_storage_page_parse() {
    // `GameData::discover` 与 `npc.rs` 的真实数据冒烟同一入口（会按工作区/清单目录解析
    // `resource/...` 路径）。
    let Ok(data) = GameData::discover() else {
        eprintln!("跳过：resource/ 里没有 Server.MirDB 样本（真实数据冒烟）");
        return;
    };
    let Some(envir) = data.envir_dir().map(PathBuf::from) else {
        eprintln!("跳过：数据库旁边没有 Envir 目录");
        return;
    };
    let npc_dir = envir.join("NPCs");
    if !npc_dir.is_dir() {
        eprintln!("跳过：{} 下没有 NPCs 目录", envir.display());
        return;
    }

    let mut scripts = 0usize;
    let mut with_storage = 0usize;
    let mut storage_files = Vec::new();
    let mut stack = vec![npc_dir.clone()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_none_or(|extension| extension != "txt") {
                continue;
            }
            // `NpcScript::load` 需要相对 `Envir/NPCs` 的文件名（不含 `.txt`）。
            let Ok(relative) = path.strip_prefix(&npc_dir) else {
                continue;
            };
            let name = relative
                .with_extension("")
                .to_string_lossy()
                .replace('\\', "/");
            let Some(script) = data.npc_script(&name) else {
                continue;
            };
            scripts += 1;
            if script.section(STORAGE_KEY).is_some() {
                with_storage += 1;
                storage_files.push(name);
            }
        }
    }

    storage_files.sort();
    eprintln!(
        "真实脚本 {scripts} 份，其中含 [@STORAGE] 页面的 {with_storage} 份（前 5 份：{:?}）",
        storage_files.iter().take(5).collect::<Vec<_>>()
    );
    assert!(scripts > 0, "至少应该解析出一些真实 NPC 脚本");
    assert!(
        with_storage > 0,
        "真实数据里应当存在带 [@STORAGE] 页面的仓库 NPC 脚本"
    );
}
