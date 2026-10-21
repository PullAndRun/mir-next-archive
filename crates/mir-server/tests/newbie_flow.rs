//! 非 GM 新号端到端：注册 → 登录 → 建角 → 进世界（出生装 + 新手提示）
//! → 走到比奇书店 → 买技能书 → `C.UseItem` 读书学会技能、书消耗。
//!
//! 走真实 TCP + 真实 `Server.MirDB` / 地图；数据目录缺失时显式 skip（不红 workspace）。

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use mir_core::Point;
use mir_core::enums::{MirClass, MirDirection, MirGender, MirGridType, PanelType};
use mir_db::MirDb;
use mir_protocol::codec::FrameBuffer;
use mir_protocol::manual::server::UserInformation;
use mir_protocol::packets::client as c;
use mir_protocol::{ClientPacket, ServerPacket};
use mir_server::gamedata::GameData;
use mir_server::store::AccountStore;
use mir_server::world::{Player, World};
use mir_server::{MemoryStore, NpcState, ServerConfig, combat, items, serve_forever};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const WAIT: Duration = Duration::from_secs(15);
/// 真实 Jev 库里的 `Fencing`（战士 7 级书）与若干 `StartItem` 索引。
const FENCING_BOOK: i32 = 973;
const STARTER_ITEM_INDICES: [i32; 5] = [221, 317, 318, 658, 722];

struct TestClient {
    stream: TcpStream,
    frames: FrameBuffer,
    object_id: u32,
}

impl TestClient {
    async fn connect(address: std::net::SocketAddr) -> Self {
        let stream = TcpStream::connect(address).await.expect("连接服务端");
        stream.set_nodelay(true).ok();
        Self {
            stream,
            frames: FrameBuffer::new(),
            object_id: 0,
        }
    }

    async fn send(&mut self, packet: ClientPacket) {
        self.stream
            .write_all(&packet.encode_frame())
            .await
            .expect("发送");
    }

    async fn next(&mut self, timeout: Duration) -> Option<ServerPacket> {
        let deadline = tokio::time::Instant::now() + timeout;
        let mut chunk = [0u8; 8192];
        loop {
            if let Some((index, payload)) = self.frames.next_frame().expect("帧格式") {
                return Some(ServerPacket::decode(index, &payload).expect("解析服务端包"));
            }
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return None;
            }
            match tokio::time::timeout(remaining, self.stream.read(&mut chunk)).await {
                Ok(Ok(0) | Err(_)) | Err(_) => return None,
                Ok(Ok(read)) => self.frames.push(&chunk[..read]),
            }
        }
    }

    async fn wait_for(&mut self, what: &str, pred: impl Fn(&ServerPacket) -> bool) -> ServerPacket {
        let deadline = tokio::time::Instant::now() + WAIT;
        let mut seen = Vec::new();
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            assert!(!remaining.is_zero(), "等待「{what}」超时；最近包：{seen:?}");
            let Some(packet) = self.next(remaining).await else {
                panic!("等待「{what}」时连接关闭；最近包：{seen:?}");
            };
            if pred(&packet) {
                return packet;
            }
            if seen.len() < 20 {
                seen.push(packet.index());
            }
        }
    }
}

fn real_config() -> Option<ServerConfig> {
    let db = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../resource/community/clgforever910-mir2database/Daneo1989/Server.MirDB");
    let map = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resource/map/0104.map");
    if !db.exists() || !map.exists() {
        eprintln!(
            "跳过 newbie_flow：缺少真实数据（db={} map={}）",
            db.display(),
            map.display()
        );
        return None;
    }
    Some(ServerConfig {
        start_map: "0104".to_owned(),
        // 比奇书店 `Librarian_Steven` 在 (5,8)；从相邻的 (7,8) 走一步即可对话。
        start_point: (7, 8),
        map_root: std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../resource/map")
            .display()
            .to_string(),
        db_path: Some(db.display().to_string()),
        load_game_data: true,
        gm_accounts: Vec::new(),
        autosave_secs: 0,
        idle_timeout_secs: 0,
        max_connections_per_ip: 0,
        max_connections_per_second: 0,
        max_packets_per_second: 0,
        metrics_listen: None,
        ..ServerConfig::default()
    })
}

async fn boot(config: ServerConfig, store: Arc<MemoryStore>) -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("绑定临时端口");
    let address = listener.local_addr().expect("取端口");
    tokio::spawn(serve_forever(listener, Arc::new(config), store));
    address
}

async fn enter_new_character(
    address: std::net::SocketAddr,
    account: &str,
) -> (TestClient, UserInformation, u32) {
    let mut client = TestClient::connect(address).await;
    client
        .wait_for("S.Connected", |packet| {
            matches!(packet, ServerPacket::Connected(_))
        })
        .await;
    client
        .send(ClientPacket::ClientVersion(c::ClientVersion {
            version_hash: Vec::new(),
        }))
        .await;
    client
        .wait_for("S.ClientVersion", |packet| {
            matches!(packet, ServerPacket::ClientVersion(_))
        })
        .await;
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
    client
        .wait_for("S.NewAccount", |packet| {
            matches!(packet, ServerPacket::NewAccount(_))
        })
        .await;
    client
        .send(ClientPacket::Login(c::Login {
            account_id: account.to_owned(),
            password: "secret".to_owned(),
        }))
        .await;
    client
        .wait_for("S.LoginSuccess", |packet| {
            matches!(packet, ServerPacket::LoginSuccess(_))
        })
        .await;
    client
        .send(ClientPacket::NewCharacter(c::NewCharacter {
            name: format!("{account}hero"),
            gender: MirGender::Male,
            class: MirClass::Warrior,
        }))
        .await;
    let index = match client
        .wait_for("S.NewCharacterSuccess", |packet| {
            matches!(packet, ServerPacket::NewCharacterSuccess(_))
        })
        .await
    {
        ServerPacket::NewCharacterSuccess(success) => success.char_info.index,
        _ => unreachable!(),
    };
    client
        .send(ClientPacket::StartGame(c::StartGame {
            character_index: index,
        }))
        .await;
    let mut info = None;
    let mut hint = None;
    let mut bookshop_id = 0;
    let mut item_info_indexes = BTreeSet::new();
    let mut seen_packets = 0usize;
    loop {
        let Some(packet) = client.next(WAIT).await else {
            panic!("进世界后连接提前关闭；已收 {seen_packets} 包");
        };
        seen_packets += 1;
        match &packet {
            ServerPacket::UserInformation(user) => {
                client.object_id = user.object_id;
                info = Some(user.clone());
            }
            ServerPacket::Chat(chat) if hint.is_none() => {
                assert_eq!(chat.type_, mir_core::enums::ChatType::System);
                hint = Some(chat.message.clone());
            }
            ServerPacket::ObjectNPC(npc)
                if npc.name.contains("Librarian") || npc.name.contains("图书管理员") =>
            {
                bookshop_id = npc.object_id;
            }
            ServerPacket::ObjectNPC(npc) => {
                eprintln!("newbie npc: {} at {:?}", npc.name, npc.location);
            }
            ServerPacket::NewItemInfo(item) => {
                assert!(!item.info.name.is_empty(), "物品模板名不能为空");
                item_info_indexes.insert(item.info.index);
            }
            ServerPacket::NewMapInfo(map) => {
                eprintln!(
                    "newbie map npcs: {:?}",
                    map.info
                        .npcs
                        .iter()
                        .map(|npc| &npc.name)
                        .collect::<Vec<_>>()
                );
            }
            _ => {}
        }
        if hint.is_some() {
            break;
        }
    }
    let Some(info) = info else {
        panic!("进世界后没有收到 UserInformation");
    };
    let Some(hint) = hint else {
        panic!("进世界后没有收到系统提示");
    };
    assert!(hint.contains("新手"), "第一条系统提示应含新手引导：{hint}");
    // 登录时必须把背包/装备里每个 unique item_index 的模板都下发（否则客户端只显示编号）。
    let expected: BTreeSet<i32> = info
        .inventory
        .iter()
        .flatten()
        .flatten()
        .chain(info.equipment.iter().flatten().flatten())
        .map(|item| item.item_index)
        .collect();
    assert!(
        expected.is_subset(&item_info_indexes),
        "背包/装备模板缺失：expected={expected:?} got={item_info_indexes:?}"
    );
    (client, info, bookshop_id)
}

/// 用已有账号/角色重新登录并进世界（不做 GM，不新建角色）。
async fn relogin_existing(
    address: std::net::SocketAddr,
    account: &str,
    character_index: i32,
) -> (TestClient, UserInformation, u32) {
    let mut client = TestClient::connect(address).await;
    client
        .wait_for("S.Connected", |packet| {
            matches!(packet, ServerPacket::Connected(_))
        })
        .await;
    client
        .send(ClientPacket::ClientVersion(c::ClientVersion {
            version_hash: Vec::new(),
        }))
        .await;
    client
        .wait_for("S.ClientVersion", |packet| {
            matches!(packet, ServerPacket::ClientVersion(_))
        })
        .await;
    client
        .send(ClientPacket::Login(c::Login {
            account_id: account.to_owned(),
            password: "secret".to_owned(),
        }))
        .await;
    client
        .wait_for("S.LoginSuccess", |packet| {
            matches!(packet, ServerPacket::LoginSuccess(_))
        })
        .await;
    client
        .send(ClientPacket::StartGame(c::StartGame { character_index }))
        .await;
    let mut info = None;
    let mut hint = None;
    let mut bookshop_id = 0;
    loop {
        let Some(packet) = client.next(WAIT).await else {
            panic!("重新登录后连接提前关闭");
        };
        match &packet {
            ServerPacket::UserInformation(user) => {
                client.object_id = user.object_id;
                info = Some(user.clone());
            }
            ServerPacket::Chat(chat) if hint.is_none() => {
                assert_eq!(chat.type_, mir_core::enums::ChatType::System);
                hint = Some(chat.message.clone());
            }
            ServerPacket::ObjectNPC(npc)
                if npc.name.contains("Librarian") || npc.name.contains("图书管理员") =>
            {
                bookshop_id = npc.object_id;
            }
            _ => {}
        }
        if hint.is_some() {
            break;
        }
    }
    let info = info.expect("重新登录应收到 UserInformation");
    assert!(
        hint.expect("重新登录应收到欢迎语").contains("新手"),
        "欢迎语仍应带新手引导"
    );
    (client, info, bookshop_id)
}

#[tokio::test]
async fn new_player_gets_starter_items_and_learns_from_bookshop() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("mir_server=debug")
        .try_init();
    let Some(config) = real_config() else {
        return;
    };
    let store = Arc::new(MemoryStore::new());
    let address = boot(config, store.clone()).await;
    let (mut client, info, _bookshop_id) = enter_new_character(address, "newbie").await;

    // 1) 出生装：真实 DB 的 StartItem（战士至少拿 WoodenSword/BaseDress/HP 药/蜡烛。
    let inventory = info.inventory.expect("登录包应带背包");
    let starter: Vec<i32> = inventory
        .iter()
        .flatten()
        .map(|item| item.item_index)
        .filter(|index| STARTER_ITEM_INDICES.contains(index))
        .collect();
    assert!(
        !starter.is_empty(),
        "新角色应有 StartItem 出生装，实际背包：{inventory:?}"
    );
    // F1：用真实 C.EquipItem 穿上出生木剑（之前 session 未分派 EquipItem，客户端发来即被忽略）。
    let sword = inventory
        .iter()
        .flatten()
        .find(|item| item.item_index == 221)
        .expect("出生装应有 WoodenSword");
    client
        .send(ClientPacket::EquipItem(c::EquipItem {
            grid: MirGridType::Inventory,
            unique_id: sword.unique_id,
            to: 0,
        }))
        .await;
    let ServerPacket::EquipItem(equipped) = client
        .wait_for("S.EquipItem", |packet| {
            matches!(packet, ServerPacket::EquipItem(_))
        })
        .await
    else {
        unreachable!()
    };
    assert!(equipped.success, "出生木剑应能穿上：{equipped:?}");
    assert_eq!(equipped.unique_id, sword.unique_id);

    // 2) store 夹具：角色已通过正常协议建好；先下线让服务端把世界状态存盘，
    //    再把 level/gold 写回存储（不是 GM 命令），重新登录后走书店。
    client.send(ClientPacket::LogOut(c::LogOut {})).await;
    let ServerPacket::LogOutSuccess(logout) = client
        .wait_for("S.LogOutSuccess", |packet| {
            matches!(packet, ServerPacket::LogOutSuccess(_))
        })
        .await
    else {
        unreachable!()
    };
    let character_index = logout.characters.first().expect("下线包应带角色槽位").index;
    drop(client);
    let account = store
        .find_account("newbie")
        .await
        .expect("注册账号应存在")
        .expect("账号");
    let mut character = store
        .list_characters(account.id)
        .await
        .expect("列角色")
        .into_iter()
        .next()
        .expect("角色已建");
    character.level = 7;
    character.gold = 1_000;
    store
        .save_character(character.clone())
        .await
        .expect("store 夹具写回");
    let (mut client, info, bookshop_id) =
        relogin_existing(address, "newbie", character_index).await;
    assert_eq!(info.level, 7, "重新登录应读到 store 里的 7 级");
    assert_eq!(info.gold, 1_000, "重新登录应读到 store 里的 1000 金币");
    assert!(
        info.equipment.as_ref().is_some_and(|equipment| equipment
            .iter()
            .flatten()
            .any(|item| item.item_index == 221)),
        "重新登录后木剑应仍在装备栏"
    );
    assert_ne!(bookshop_id, 0, "重新登录后书店 NPC 必须在视野里");

    // 3) 走到书店：从 (7,8) 向左一格到 (6,8)，再与 (5,8) 的 NPC 对话。
    client
        .send(ClientPacket::Walk(c::Walk {
            direction: MirDirection::Left,
        }))
        .await;
    client
        .wait_for("S.UserLocation", |packet| {
            matches!(packet, ServerPacket::UserLocation(_))
        })
        .await;
    client
        .send(ClientPacket::CallNPC(c::CallNPC {
            object_id: bookshop_id,
            key: "@main".to_owned(),
        }))
        .await;
    client
        .wait_for("S.NPCResponse", |packet| {
            matches!(packet, ServerPacket::NPCResponse(_))
        })
        .await;
    client
        .send(ClientPacket::CallNPC(c::CallNPC {
            object_id: bookshop_id,
            key: "@BuySell".to_owned(),
        }))
        .await;
    let ServerPacket::NPCGoods(goods) = client
        .wait_for("S.NPCGoods", |packet| {
            matches!(packet, ServerPacket::NPCGoods(_))
        })
        .await
    else {
        unreachable!()
    };
    let book = goods
        .list
        .iter()
        .find(|item| item.item_index == FENCING_BOOK)
        .unwrap_or_else(|| panic!("书店应卖 Fencing 技能书，实际货架：{goods:?}"));
    let book_uid = book.unique_id;

    // 3) 买书 → 收到 GainedItem。
    client
        .send(ClientPacket::BuyItem(c::BuyItem {
            item_index: book_uid,
            count: 1,
            type_: PanelType::Buy,
        }))
        .await;
    let mut gained = None;
    let mut lost = None;
    while gained.is_none() || lost.is_none() {
        let Some(packet) = client.next(WAIT).await else {
            panic!("买书后连接关闭；gained={gained:?} lost={lost:?}");
        };
        match packet {
            ServerPacket::GainedItem(item) => gained = Some(item),
            ServerPacket::LoseGold(gold) => lost = Some(gold),
            _ => {}
        }
    }
    let gained = gained.expect("S.GainedItem");
    let lost = lost.expect("S.LoseGold");
    assert_eq!(gained.item.item_index, FENCING_BOOK);
    assert_eq!(lost.gold, 500, "买 Fencing 应扣 500 金币");
    let item_uid = gained.item.unique_id;

    // 4) 读书 → S.NewMagic；再读同一本应失败（书已消耗）。
    client
        .send(ClientPacket::UseItem(c::UseItem {
            unique_id: item_uid,
            grid: MirGridType::Inventory,
        }))
        .await;
    let mut used = false;
    let mut learned = false;
    while !used || !learned {
        let Some(packet) = client.next(WAIT).await else {
            panic!("读书后连接关闭；used={used} learned={learned}");
        };
        match packet {
            ServerPacket::UseItem(use_item)
                if use_item.unique_id == item_uid && use_item.success =>
            {
                used = true;
            }
            ServerPacket::NewMagic(_) => learned = true,
            _ => {}
        }
    }

    client
        .send(ClientPacket::UseItem(c::UseItem {
            unique_id: item_uid,
            grid: MirGridType::Inventory,
        }))
        .await;
    client
        .wait_for("S.UseItem 失败（书已消耗）", |packet| {
            matches!(packet, ServerPacket::UseItem(use_item) if use_item.unique_id == item_uid && !use_item.success)
        })
        .await;
}

/// 真实数据下不经过 socket 的同一链路（World 层）：出生装 → 书店货架 → 买书 → 读书。
/// socket 用例负责验证登录/提示/背包；这条补上真实书店脚本与 `use_book` 的完整闭环。
#[test]
fn non_gm_bookshop_purchase_and_reading_path() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../resource/community/clgforever910-mir2database/Daneo1989");
    let db_path = root.join("Server.MirDB");
    if !db_path.exists() {
        eprintln!("跳过 newbie_flow World 层：缺少真实 Server.MirDB");
        return;
    }
    let db = MirDb::from_bytes(&std::fs::read(&db_path).expect("读数据库")).expect("解析数据库");
    let data = GameData::from_db(db, Some(root.join("Envir/Drops")));
    let map_file = data
        .map_info_by_index(6)
        .map_or_else(|| "6".to_owned(), |info| info.file_name.clone());
    eprintln!("newbie world map file: {map_file}");
    let mut world = World::new(999);
    world.set_game_data(Arc::new(data));
    world.set_map_root(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resource/map"));
    world.load_map(&map_file).expect("加载 BichonWall 地图");
    world.spawn_npcs(&map_file);
    let npc = world
        .npcs()
        .iter()
        .find(|npc| npc.file_name.contains("BookStore"))
        .cloned()
        .expect("真实 BichonWall 应有书店 NPC");
    let stats = combat::player_base_stats(1, MirClass::Warrior);
    let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel();
    let player_id = 10_000;
    world.enter(Player {
        object_id: player_id,
        session_id: 1,
        character_id: 1,
        name: "newbie".to_owned(),
        map: map_file.clone(),
        position: Point::new(npc.position.x, npc.position.y + 1),
        direction: MirDirection::Up,
        level: 1,
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
        regen_time: 0,
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
        credit: 0,
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
    assert!(
        world
            .player(player_id)
            .expect("玩家")
            .inventory
            .iter()
            .flatten()
            .any(|item| STARTER_ITEM_INDICES.contains(&item.item_index)),
        "1 级空背包进世界应有出生装"
    );
    // 测试夹具：新角色金库为 0，这里只补足买一本技能书的金币；不经过 GM 命令。
    // 真实技能书有等级要求（Fencing 7 级），测试把角色视为已经练到 7 级。
    if let Some(player) = world.player_mut(player_id) {
        player.gold = 1_000;
        player.level = 7;
    }
    world
        .call_npc(player_id, npc.object_id, "@main")
        .expect("打开书店主页");
    world
        .call_npc(player_id, npc.object_id, "@BuySell")
        .expect("打开书店买卖页");
    let goods = world.npc_goods(&npc);
    let book = goods
        .iter()
        .find(|item| item.item_index == FENCING_BOOK)
        .expect("书店应卖 Fencing");
    let bought = world
        .buy_item(player_id, book.unique_id, 1, PanelType::Buy)
        .expect("买书");
    let report = world
        .use_item(player_id, bought.item.unique_id)
        .expect("读书");
    assert!(report.learned.is_some(), "读书应学会技能");
    assert!(report.remaining.is_none(), "单本书应被消耗");
}

/// `C.RequestItemInfo`：按需补一条 `S.NewItemInfo`；同一模板第二次请求按 Crystal
/// `CheckItemInfo` 去重，不应重复下发。
#[tokio::test]
async fn request_item_info_sends_template_once() {
    let Some(config) = real_config() else {
        return;
    };
    let db_path = config.db_path.clone().expect("newbie config 带 db_path");
    let db = MirDb::load(&db_path).expect("加载真实 MirDB");
    let store = Arc::new(MemoryStore::new());
    let address = boot(config, store.clone()).await;
    let (mut client, info, _) = enter_new_character(address, "iteminfo").await;

    let used: BTreeSet<i32> = info
        .inventory
        .iter()
        .flatten()
        .flatten()
        .chain(info.equipment.iter().flatten().flatten())
        .map(|item| item.item_index)
        .collect();
    let index = db
        .items
        .iter()
        .map(|item| item.index)
        .find(|index| !used.contains(index))
        .expect("应有一个未在背包里的模板");

    client
        .send(ClientPacket::RequestItemInfo(c::RequestItemInfo {
            item_index: index,
        }))
        .await;
    let packet = client
        .wait_for(
            "S.NewItemInfo",
            |packet| matches!(packet, ServerPacket::NewItemInfo(info) if info.info.index == index),
        )
        .await;
    let ServerPacket::NewItemInfo(packet) = packet else {
        unreachable!()
    };
    assert_eq!(packet.info.index, index);
    assert!(!packet.info.name.is_empty(), "模板名不能为空");

    // 重复请求：已发过 → 不再有 NewItemInfo。
    client
        .send(ClientPacket::RequestItemInfo(c::RequestItemInfo {
            item_index: index,
        }))
        .await;
    let extra = client.next(Duration::from_millis(300)).await;
    assert!(extra.is_none(), "重复请求不应重复下发：{extra:?}");
}
