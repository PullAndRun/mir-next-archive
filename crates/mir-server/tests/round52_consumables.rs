//! R52 B1：施法耗材（护身符/毒药）在**生产存档链**上的往返。
//!
//! 原语本身（`World::find_spell_material` / `World::consume_spell_material`）是
//! `pub(crate)`，由 `crates/mir-server/src/spell_consumables.rs` 的模块内单测直接覆盖；
//! 本文件只走公开真实链，验证 M01 的存档部分：
//!
//! 真实 `Session` 登录/下线（下线时 `session::character_snapshot` → 既有
//! `characters.equipment` 字节块）→ 用生产 `decode_equipment` 读回 → **重登**后
//! 装备栏逐槽全等。其中「消耗后的装备栏」指部分扣除后剩余 `count = 3`、耐久 0 的
//! 护身符仍在槽 6，毒药耗尽后槽 9 为 `None`——正是原语产出的两种形状。
//!
//! Crystal 对照：`Server/MirObjects/HumanObject.cs:6859`（`GetAmulet`）、`:6873`
//! （`GetPoison`）、`:1346`（`ConsumeItem`）；装备整块的编解码见
//! `CharacterInfo.cs:431-447`，本作对应 `crates/mir-server/src/items.rs` 的
//! `encode_slots` / `decode_equipment`。

mod common;

use std::sync::Arc;

use mir_core::enums::{ItemType, MirClass, MirGender};
use mir_core::i18n::LanguageLibrary;
use mir_db::{MapInfo, MirDb};
use mir_protocol::data::{ItemInfo, UserItem};
use mir_protocol::packets::client as c;
use mir_protocol::{ClientPacket, ServerPacket};
use mir_server::gamedata::GameData;
use mir_server::items;
use mir_server::session::Session;
use mir_server::store::AccountStore;
use mir_server::{MemoryStore, ServerConfig, World};
use tokio::sync::{Mutex, mpsc};

/// 测试地图文件名（与 `common::MAP` 一致，合成 60×60 空地）。
const MAP: &str = common::MAP;
/// 测试护身符模板编号（`Shape = 0`）。
const AMULET_INDEX: i32 = 9001;
/// 测试绿毒模板编号（`Shape = 1`）。
const POISON_INDEX: i32 = 9002;
/// 部分扣除后留在槽 6 的护身符 UUID。
const AMULET_UID: u64 = 60_001;

const ACCOUNT: &str = "tester";
const PASSWORD: &str = "secret";
const CHARACTER: &str = "耗材甲";

/// 合成数据：两种耗材 + 地图主键（`common::synthetic_map` 与文件名对齐）。
fn consumables_db() -> MirDb {
    MirDb {
        maps: vec![MapInfo {
            index: common::MAP_INDEX,
            file_name: MAP.to_owned(),
            title: "耗材测试".to_owned(),
            ..MapInfo::default()
        }],
        items: vec![
            ItemInfo {
                index: AMULET_INDEX,
                name: "护身符".to_owned(),
                item_type: ItemType::Amulet,
                shape: 0,
                stack_size: 300,
                durability: 10,
                ..ItemInfo::default()
            },
            ItemInfo {
                index: POISON_INDEX,
                name: "绿毒".to_owned(),
                item_type: ItemType::Amulet,
                shape: 1,
                stack_size: 200,
                durability: 5,
                ..ItemInfo::default()
            },
        ],
        ..MirDb::default()
    }
}

fn build_world(seed: u64) -> World {
    let mut world = World::new(999);
    world.seed_rng(seed);
    world.insert_map(MAP, common::synthetic_map(MAP));
    world.set_game_data(Arc::new(GameData::from_db(consumables_db(), None)));
    world.set_languages(Arc::new(LanguageLibrary::embedded()));
    world.advance_clock(0);
    world
}

/// 用生产 [`World::create_fresh_item`] 造一件真实物品，再钉 UUID/数量。
fn make_item(world: &World, index: i32, count: u16, unique_id: u64) -> UserItem {
    let info = world.item_info(index).expect("物品模板").clone();
    let mut item = world.create_fresh_item(&info);
    item.unique_id = unique_id;
    item.count = count;
    item
}

fn drain(rx: &mut mpsc::UnboundedReceiver<Vec<u8>>) -> Vec<ServerPacket> {
    let mut packets = Vec::new();
    while let Ok(frame) = rx.try_recv() {
        let index = i16::from_le_bytes([frame[2], frame[3]]);
        packets.push(ServerPacket::decode(index, &frame[4..]).expect("解码服务端包"));
    }
    packets
}

/// 生产登录流程：版本 → 登录 → 选角进世界（与 `storage_mail.rs` 的 `Live::login` 同构）。
async fn login_into(
    world: Arc<Mutex<World>>,
    store: Arc<MemoryStore>,
    session_id: u64,
    character_id: i64,
    out: mpsc::UnboundedSender<Vec<u8>>,
) -> Session<MemoryStore> {
    let config = Arc::new(ServerConfig {
        load_game_data: false,
        map_root: "resource/不存在的目录".to_owned(),
        start_map: MAP.to_owned(),
        ..ServerConfig::default()
    });
    let mut session = Session::new(
        session_id,
        "test".to_owned(),
        config,
        store,
        world,
        out,
        Arc::new(LanguageLibrary::embedded()),
    );
    session
        .handle(ClientPacket::ClientVersion(c::ClientVersion {
            version_hash: Vec::new(),
        }))
        .await
        .expect("版本校验");
    session
        .handle(ClientPacket::Login(c::Login {
            account_id: ACCOUNT.to_owned(),
            password: PASSWORD.to_owned(),
        }))
        .await
        .expect("登录");
    session
        .handle(ClientPacket::StartGame(c::StartGame {
            character_index: i32::try_from(character_id).expect("角色 ID 适合 i32"),
        }))
        .await
        .expect("进世界");
    session
}

/// 从世界锁里取玩家当前的整块装备栏。
async fn equipment_of(world: &Arc<Mutex<World>>, name: &str) -> Vec<Option<UserItem>> {
    let world = world.lock().await;
    let object_id = world
        .player_by_name(name)
        .unwrap_or_else(|| panic!("{name} 应在世界上"))
        .object_id;
    world.player(object_id).expect("玩家").equipment.clone()
}

#[tokio::test]
async fn round52_consumables_save_reload_equipment_count() {
    // --- 1) 账号 + 角色 + 世界 + 真实 Session 登录。
    let store = Arc::new(MemoryStore::new());
    let account = store
        .create_account(ACCOUNT, PASSWORD, "")
        .await
        .expect("建号");
    let created = store
        .create_character(
            account.id,
            CHARACTER,
            MirClass::Taoist,
            MirGender::Male,
            MAP,
            (10, 10),
        )
        .await
        .expect("建角");

    let world = Arc::new(Mutex::new(build_world(7)));
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut session = login_into(Arc::clone(&world), Arc::clone(&store), 1, created.id, tx).await;
    let _ = drain(&mut rx);
    let object_id = {
        let world = world.lock().await;
        world
            .player_by_name(CHARACTER)
            .expect("登录后玩家应在世界上")
            .object_id
    };

    // --- 2) 摆出「原语消耗之后」的装备栏：槽 6 护身符剩 3 张（0 耐久），槽 9 毒耗光摘槽。
    //     原语本身的扣数/摘槽/发包由 `spell_consumables.rs` 模块内单测证明；这里只负责
    //     把这种状态交给生产存档链。
    let expected_equipment = {
        let mut world = world.lock().await;
        // 0 耐久：Crystal 的 `GetAmulet` 不判断耐久，这种剩符照样能烧，落档也要原样保留。
        let mut amulet = make_item(&world, AMULET_INDEX, 3, AMULET_UID);
        amulet.current_dura = 0;
        let player = world.player_mut(object_id).expect("玩家");
        player.equipment[6] = Some(amulet); // 槽 6 = `BraceletR`，`slot_accepts` 也收护身符
        player.equipment[9] = None; // 槽 9 = `Amulet`，毒药耗尽后已摘槽
        player.equipment.clone()
    };
    assert_eq!(
        expected_equipment[6].as_ref().map(|item| item.count),
        Some(3),
        "部分扣除后剩 3 张"
    );
    assert!(expected_equipment[9].is_none(), "毒药耗尽后槽位是 None");

    // --- 3) 生产下线：`leave_world` → `session::character_snapshot` → `save_character`。
    session.leave_world().await;
    drop(session);

    let saved = store
        .find_character(account.id, created.id)
        .await
        .expect("读档")
        .expect("角色行存在");
    let saved_equipment =
        items::decode_equipment(&saved.equipment).expect("解码 characters.equipment");
    assert_eq!(
        saved_equipment, expected_equipment,
        "生产快照写进 characters.equipment 的逐槽内容（UUID/模板/数量/耐久）"
    );
    let saved_amulet = saved_equipment[6].as_ref().expect("槽 6 护身符");
    assert_eq!(
        (
            saved_amulet.unique_id,
            saved_amulet.item_index,
            saved_amulet.count,
            saved_amulet.current_dura
        ),
        (AMULET_UID, AMULET_INDEX, 3, 0),
        "0 耐久的护身符连同数量原样落档"
    );
    assert!(saved_equipment[9].is_none(), "耗尽的毒药不能复活");

    // --- 4) 新建世界 + 新会话重登：装备栏逐槽全等。
    let reloaded_world = Arc::new(Mutex::new(build_world(8)));
    let (reloaded_tx, mut reloaded_rx) = mpsc::unbounded_channel();
    let mut reloaded_session = login_into(
        Arc::clone(&reloaded_world),
        Arc::clone(&store),
        2,
        created.id,
        reloaded_tx,
    )
    .await;
    let _ = drain(&mut reloaded_rx);
    let reloaded_equipment = equipment_of(&reloaded_world, CHARACTER).await;
    assert_eq!(
        reloaded_equipment, expected_equipment,
        "重登后 equipment 逐槽全等（UUID/模板/count/耐久）"
    );
    assert_eq!(
        reloaded_equipment[6].as_ref().expect("槽 6 护身符").count,
        3,
        "部分扣除后的 count 经重登仍然正确"
    );
    assert!(
        reloaded_equipment[8].is_none() && reloaded_equipment[9].is_none(),
        "空槽与摘掉的槽都不能复活"
    );

    // --- 5) 二次下线仍然一致（反复登录/下线的幂等性）。
    reloaded_session.leave_world().await;
    drop(reloaded_session);
    let resaved = store
        .find_character(account.id, created.id)
        .await
        .expect("再读档")
        .expect("角色行存在");
    assert_eq!(
        items::decode_equipment(&resaved.equipment).expect("再解码"),
        expected_equipment,
        "二次下线后 characters.equipment 依旧逐槽全等"
    );
}
