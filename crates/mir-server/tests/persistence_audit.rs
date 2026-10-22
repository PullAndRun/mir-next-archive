//! 存档一致性审计：断线 / 崩溃窗口 / 写失败 / 并发 / 重启恢复。
//!
//! 这些测试只证明存储层与会话落盘路径的可观察语义；真实的「进程被 kill -9」
//! 用「只信任最后一次成功写库的行」来模拟，见 `crash_window_*`。

use std::sync::Arc;
use std::time::Duration;

use mir_core::enums::{MirClass, MirDirection, MirGender};
use mir_protocol::codec::FrameBuffer;
use mir_protocol::packets::client as c;
use mir_protocol::packets::server as s;
use mir_protocol::{ClientPacket, ServerPacket};
use mir_server::{
    AccountStore, Character, ConquestRecord, GuildRecord, MemoryStore, ServerConfig, World,
    serve_forever, serve_forever_with_world,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

struct TestClient {
    stream: TcpStream,
    frames: FrameBuffer,
}

impl TestClient {
    async fn connect(address: std::net::SocketAddr) -> Self {
        let stream = TcpStream::connect(address).await.expect("connect");
        stream.set_nodelay(true).ok();
        Self {
            stream,
            frames: FrameBuffer::new(),
        }
    }

    async fn send(&mut self, packet: ClientPacket) {
        self.stream
            .write_all(&packet.encode_frame())
            .await
            .expect("send");
    }

    async fn next(&mut self) -> ServerPacket {
        for _ in 0..40 {
            if let Some((index, payload)) = self.frames.next_frame().expect("frame") {
                return ServerPacket::decode(index, &payload).expect("decode");
            }
            let mut chunk = [0u8; 4096];
            let read = tokio::time::timeout(Duration::from_secs(5), self.stream.read(&mut chunk))
                .await
                .expect("read timeout")
                .expect("read");
            assert!(read > 0, "server closed");
            self.frames.push(&chunk[..read]);
        }
        panic!("packet timeout");
    }

    async fn expect_index(&mut self, index: i16) -> ServerPacket {
        let mut packet = self.next().await;
        while matches!(packet, ServerPacket::FriendUpdate(_)) {
            packet = self.next().await;
        }
        assert_eq!(packet.index(), index, "unexpected {packet:?}");
        packet
    }

    /// 等到指定索引的包为止，途中的世界广播（`ObjectPlayer`/`ObjectMonster`/...）忽略。
    ///
    /// 第二个账号登录时会收到第一个玩家的外观广播，`expect_index` 的严格断言会炸。
    async fn wait_for_index(&mut self, index: i16) -> ServerPacket {
        for _ in 0..80 {
            let packet = self.next().await;
            if packet.index() == index {
                return packet;
            }
        }
        panic!("等不到包 {index}");
    }

    async fn send_chat(&mut self, message: &str) {
        self.send(ClientPacket::Chat(c::Chat {
            message: message.to_owned(),
            linked_items: Vec::new(),
        }))
        .await;
    }
}

fn config() -> Arc<ServerConfig> {
    Arc::new(ServerConfig {
        map_root: "resource/不存在".to_owned(),
        load_game_data: false,
        gm_accounts: vec!["gmuser".to_owned()],
        ..ServerConfig::default()
    })
}

async fn start_server() -> (std::net::SocketAddr, Arc<MemoryStore>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let address = listener.local_addr().expect("addr");
    let store = Arc::new(MemoryStore::new());
    tokio::spawn(serve_forever(listener, config(), Arc::clone(&store)));
    (address, store)
}

/// 起一个**注入世界**的服务器：测试拿世界句柄用 `World::advance_clock` 快进世界时钟，
/// 从而确定性地驱动 autosave，不用 sleep 等 `autosave_secs` 秒。
async fn start_server_with_world(
    autosave_secs: u64,
) -> (
    std::net::SocketAddr,
    Arc<MemoryStore>,
    Arc<tokio::sync::Mutex<World>>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let address = listener.local_addr().expect("addr");
    let store = Arc::new(MemoryStore::new());
    let config = Arc::new(ServerConfig {
        autosave_secs,
        gm_accounts: vec!["gmuser".to_owned(), "busyuser".to_owned()],
        ..(*config()).clone()
    });
    let world = Arc::new(tokio::sync::Mutex::new(World::new(999)));
    {
        let mut world = world.lock().await;
        world.set_map_root(&config.map_root);
    }
    tokio::spawn(serve_forever_with_world(
        listener,
        Arc::clone(&config),
        Arc::clone(&store),
        Arc::clone(&world),
    ));
    (address, store, world)
}

/// 推进世界时钟并给服务器的 autosave 轮询留出发火机会。
///
/// 轮询粒度是 `AUTOSAVE_POLL_MS`（250ms），这里等 600ms 是把「世界时钟已到点」
/// 变成「读循环真的跑了一次 autosave」，仍然远小于用真实 30 秒周期。
async fn advance(world: &Arc<tokio::sync::Mutex<World>>, ms: i64) {
    {
        let mut world = world.lock().await;
        world.advance_clock(ms);
    }
    tokio::time::sleep(Duration::from_millis(600)).await;
}

/// 用指定账号名进世界（第一个账号用的 `gmuser` 会被别的测试复用；并发多账号时用这个）。
async fn enter_world_as(
    address: std::net::SocketAddr,
    account_name: &str,
    character_name: &str,
) -> (TestClient, i32, s::UserInformation) {
    let mut client = TestClient::connect(address).await;
    client.wait_for_index(s::Connected::INDEX).await;

    client
        .send(ClientPacket::ClientVersion(c::ClientVersion {
            version_hash: vec![0u8; 16],
        }))
        .await;
    client.wait_for_index(s::ClientVersion::INDEX).await;

    client
        .send(ClientPacket::NewAccount(c::NewAccount {
            account_id: account_name.to_owned(),
            password: "secret".to_owned(),
            birth_date: mir_core::DateTimeBinary(0),
            user_name: String::new(),
            secret_question: String::new(),
            secret_answer: String::new(),
            e_mail_address: String::new(),
        }))
        .await;
    client.wait_for_index(s::NewAccount::INDEX).await;

    client
        .send(ClientPacket::Login(c::Login {
            account_id: account_name.to_owned(),
            password: "secret".to_owned(),
        }))
        .await;
    let packet = client.wait_for_index(s::LoginSuccess::INDEX).await;
    let characters = match packet {
        ServerPacket::LoginSuccess(success) => success.characters,
        other => panic!("LoginSuccess expected, got {other:?}"),
    };
    let index = if let Some(existing) = characters.first() {
        existing.index
    } else {
        client
            .send(ClientPacket::NewCharacter(c::NewCharacter {
                name: character_name.to_owned(),
                gender: MirGender::Male,
                class: MirClass::Warrior,
            }))
            .await;
        match client.wait_for_index(s::NewCharacterSuccess::INDEX).await {
            ServerPacket::NewCharacterSuccess(success) => success.char_info.index,
            other => panic!("NewCharacterSuccess expected, got {other:?}"),
        }
    };

    client
        .send(ClientPacket::StartGame(c::StartGame {
            character_index: index,
        }))
        .await;
    client.wait_for_index(s::StartGame::INDEX).await;
    client.wait_for_index(s::MapInformation::INDEX).await;
    let info = match client.wait_for_index(s::UserInformation::INDEX).await {
        ServerPacket::UserInformation(info) => info,
        other => panic!("UserInformation expected, got {other:?}"),
    };
    (client, index, info)
}

/// 注册/登录/建角/进世界，返回 (客户端, 角色 index)。
async fn enter_world(address: std::net::SocketAddr) -> (TestClient, i32, s::UserInformation) {
    let mut client = TestClient::connect(address).await;
    client.expect_index(s::Connected::INDEX).await;

    client
        .send(ClientPacket::ClientVersion(c::ClientVersion {
            version_hash: vec![0u8; 16],
        }))
        .await;
    client.expect_index(s::ClientVersion::INDEX).await;

    client
        .send(ClientPacket::NewAccount(c::NewAccount {
            account_id: "gmuser".to_owned(),
            password: "secret".to_owned(),
            birth_date: mir_core::DateTimeBinary(0),
            user_name: String::new(),
            secret_question: String::new(),
            secret_answer: String::new(),
            e_mail_address: String::new(),
        }))
        .await;
    // 账号已存在（重连）时返回 0 或 8，都继续走登录。
    client.expect_index(s::NewAccount::INDEX).await;

    client
        .send(ClientPacket::Login(c::Login {
            account_id: "gmuser".to_owned(),
            password: "secret".to_owned(),
        }))
        .await;
    let packet = client.expect_index(s::LoginSuccess::INDEX).await;
    let characters = match packet {
        ServerPacket::LoginSuccess(success) => success.characters,
        other => panic!("LoginSuccess expected, got {other:?}"),
    };
    let index = if let Some(existing) = characters.first() {
        existing.index
    } else {
        client
            .send(ClientPacket::NewCharacter(c::NewCharacter {
                name: "AuditHero".to_owned(),
                gender: MirGender::Male,
                class: MirClass::Warrior,
            }))
            .await;
        let packet = client.expect_index(s::NewCharacterSuccess::INDEX).await;
        match packet {
            ServerPacket::NewCharacterSuccess(success) => success.char_info.index,
            other => panic!("NewCharacterSuccess expected, got {other:?}"),
        }
    };

    client
        .send(ClientPacket::StartGame(c::StartGame {
            character_index: index,
        }))
        .await;
    client.expect_index(s::StartGame::INDEX).await;
    client.expect_index(s::MapInformation::INDEX).await;
    let info = match client.expect_index(s::UserInformation::INDEX).await {
        ServerPacket::UserInformation(info) => info,
        other => panic!("UserInformation expected, got {other:?}"),
    };
    (client, index, info)
}

async fn login_again(address: std::net::SocketAddr) -> s::UserInformation {
    let (_, _, info) = enter_world(address).await;
    info
}

async fn grant_gold(client: &mut TestClient, amount: u32) {
    client.send_chat(&format!("@gold {amount}")).await;
    for _ in 0..20 {
        if let ServerPacket::Chat(chat) = client.next().await {
            if chat.message.contains("Gold +") {
                return;
            }
        }
    }
    panic!("GM @gold 没有回复");
}

// ---------------------------------------------------------------------------
// 1) 显式 LogOut 落盘
// ---------------------------------------------------------------------------
#[tokio::test]
async fn logout_saves_gold_before_next_login() {
    let (address, _store) = start_server().await;
    let (mut client, _index, info) = enter_world(address).await;
    assert_eq!(info.gold, 0);

    grant_gold(&mut client, 777).await;
    client
        .send(ClientPacket::LogOut(c::LogOut::default()))
        .await;
    client.expect_index(s::LogOutSuccess::INDEX).await;

    let after = login_again(address).await;
    assert_eq!(after.gold, 777, "LogOut 后重连金币必须一致");
}

// ---------------------------------------------------------------------------
// 2) 直接断 TCP（不走 LogOut）也要落盘
// ---------------------------------------------------------------------------
#[tokio::test]
async fn tcp_disconnect_saves_gold_before_next_login() {
    let (address, store) = start_server().await;
    let (mut client, index, _info) = enter_world(address).await;
    grant_gold(&mut client, 654).await;

    drop(client); // 直接断链：服务端靠读 EOF 触发 on_disconnected
    // 给 accept 循环一点时间把 on_disconnected 跑完。
    for _ in 0..50 {
        let stored = store
            .find_character(1, i64::from(index))
            .await
            .expect("find")
            .expect("character");
        if stored.gold == 654 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let after = login_again(address).await;
    assert_eq!(after.gold, 654, "TCP 直接断开也必须保存金币");
}

// ---------------------------------------------------------------------------
// 3) 崩溃窗口：有了周期 autosave，最多丢 `autosave_secs + 1` 秒的改动
// ---------------------------------------------------------------------------
//
// 吏部 t85 的 F1：Crystal 只在 LogOut 写角色行，进程崩溃 = 回档到登录时刻（RPO 无上限）。
// 本作用「世界时钟 + 脏行 autosave」把窗口压到 `autosave_secs + 1s`（+1s 是轮询粒度）。
// 断言都在**会话仍然连着**的时候做——这才等价于「进程被 kill -9，库里剩下什么」。
#[tokio::test]
async fn autosave_bounds_the_crash_window_to_autosave_secs() {
    const AUTOSAVE_SECS: u64 = 1;
    let (address, store, world) = start_server_with_world(AUTOSAVE_SECS).await;
    let (mut client, index, info) = enter_world(address).await;
    let account_id = store
        .find_account("gmuser")
        .await
        .expect("account")
        .expect("row")
        .id;

    // 登录时落过盘（建角 + 首登补属性），此刻库里是 0 金币。
    let before = store
        .find_character(account_id, i64::from(index))
        .await
        .expect("load")
        .expect("row");
    assert_eq!(before.gold, 0, "登录后的初始金币");

    // 世界里的金币变了，但**还没到 autosave 点**：库里仍是旧值（这就是崩溃窗口）。
    grant_gold(&mut client, 900).await;
    let in_window = store
        .find_character(account_id, i64::from(index))
        .await
        .expect("load")
        .expect("row");
    assert_eq!(
        in_window.gold, 0,
        "还没到 autosave 点，库里不该有新金币（说明脏值确实只在内存里）"
    );

    // 推进 1 秒 → autosave 到点 → 脏行落盘。
    advance(
        &world,
        i64::try_from(AUTOSAVE_SECS).expect("secs") * 1000 + 100,
    )
    .await;
    let after = store
        .find_character(account_id, i64::from(index))
        .await
        .expect("load")
        .expect("row");
    assert_eq!(
        after.gold, 900,
        "autosave 必须把金币写进库（崩溃窗口 ≤ autosave_secs + 1 秒）"
    );
    assert_eq!(
        (after.x, after.y),
        (info.location.x, info.location.y),
        "坐标也在同一行里，一起落盘"
    );

    // 会话还连着（没 LogOut/断线）：上面读到的就是「进程此刻被杀」后的库内容。
    drop(client);
}

// ---------------------------------------------------------------------------
// 3b) 只写脏角色：静止的玩家不会被周期存档空写
// ---------------------------------------------------------------------------
#[tokio::test]
async fn autosave_writes_only_dirty_characters() {
    let (address, store, world) = start_server_with_world(1).await;
    let (mut idle_client, idle_index, _) = enter_world(address).await;
    // 出生点是同一个格子：idle 先走开两步，空出 (330,270) 免得第二个角色进不来。
    for _ in 0..2 {
        idle_client
            .send(ClientPacket::Walk(c::Walk {
                direction: MirDirection::Right,
            }))
            .await;
        tokio::time::sleep(Duration::from_millis(700)).await;
    }
    // 第二个角色：另一个账号（同一个 MemoryStore）。
    let (mut busy_client, busy_index, _) = enter_world_as(address, "busyuser", "BusyHero").await;

    // idle 刚才走过两步（坐标变了=脏），先让它那轮 autosave 落完盘，把两人都变干净。
    advance(&world, 1_200).await;
    store.clear_save_history().await;

    // 只有 busy 改金币；idle 从此什么都不做。
    grant_gold(&mut busy_client, 777).await;
    advance(&world, 1_200).await;
    let saved = store.save_history().await;
    assert!(
        saved.contains(&i64::from(busy_index)),
        "脏角色必须被写：{saved:?}"
    );
    assert!(
        !saved.contains(&i64::from(idle_index)),
        "静止角色不该被空写（只写脏角色）：{saved:?}"
    );

    // 再推进一个周期：busy 也已经干净，谁都不该再写。
    store.clear_save_history().await;
    advance(&world, 1_200).await;
    assert!(
        store.save_history().await.is_empty(),
        "干净角色不该被周期存档重复写"
    );
}

// ---------------------------------------------------------------------------
// 3c) 写失败可观测 + 退避重试：脏标记保留，下一次 autosave 补上
// ---------------------------------------------------------------------------
#[tokio::test]
async fn autosave_retries_after_a_failed_write() {
    let (address, store, world) = start_server_with_world(1).await;
    let (mut client, index, _) = enter_world(address).await;
    let account_id = store
        .find_account("gmuser")
        .await
        .expect("account")
        .expect("row")
        .id;
    grant_gold(&mut client, 4321).await;

    let failures_before = mir_server::metrics::metrics().db_save_failed();
    let autosaves_before = mir_server::metrics::metrics().autosaves();

    // 注入一次落盘失败，正好命中本轮 autosave。
    store.fail_next_save_character().await;
    advance(&world, 1_200).await;

    let row = store
        .find_character(account_id, i64::from(index))
        .await
        .expect("load")
        .expect("row");
    assert_eq!(row.gold, 0, "写失败时库里保持上一次成功的值");
    assert!(
        mir_server::metrics::metrics().db_save_failed() > failures_before,
        "写失败必须体现在 mir2_db_save_failed_total"
    );
    assert!(
        mir_server::metrics::metrics().autosaves() > autosaves_before,
        "失败的那次也算一次 autosave 尝试"
    );

    // 失败后按 1s 退避：推进 1 秒再过一轮，脏标记还在 → 这次写成功。
    advance(&world, 1_200).await;
    let row = store
        .find_character(account_id, i64::from(index))
        .await
        .expect("load")
        .expect("row");
    assert_eq!(row.gold, 4321, "重试成功后金币补写进库（批次没有丢）");
}

// ---------------------------------------------------------------------------
// 4) 写失败：返回错误、旧行保持、内存值不被写坏
// ---------------------------------------------------------------------------
#[tokio::test]
async fn failed_character_save_keeps_previous_row_and_returns_error() {
    let store = MemoryStore::new();
    let account = store
        .create_account("gmuser", "secret", "")
        .await
        .expect("account");
    let mut character = store
        .create_character(
            account.id,
            "FailHero",
            MirClass::Warrior,
            MirGender::Male,
            "0",
            (330, 270),
        )
        .await
        .expect("character");
    character.gold = 42;
    store
        .save_character(character.clone())
        .await
        .expect("save v1");

    character.gold = 999;
    let snapshot = character.clone();
    store.fail_next_save_character().await;
    let result = store.save_character(character.clone()).await;
    assert!(result.is_err(), "注入的写失败必须冒泡，不能静默");
    assert_eq!(snapshot.gold, 999, "调用者的内存值不应被存储层改写");

    let row = store
        .find_character(account.id, character.id)
        .await
        .expect("load")
        .expect("row");
    assert_eq!(row.gold, 42, "失败后数据库仍是上一次成功的值");
}

// ---------------------------------------------------------------------------
// 5) 并发保存：串行化、最后提交者胜、行不撕裂
// ---------------------------------------------------------------------------
#[tokio::test]
async fn concurrent_saves_are_serialized_and_keep_a_consistent_row() {
    let store = Arc::new(MemoryStore::new());
    let account = store
        .create_account("gmuser", "secret", "")
        .await
        .expect("account");
    let mut character = store
        .create_character(
            account.id,
            "RaceHero",
            MirClass::Warrior,
            MirGender::Male,
            "0",
            (330, 270),
        )
        .await
        .expect("character");
    character.gold = 1;
    store.save_character(character.clone()).await.expect("base");

    let mut first = character.clone();
    first.gold = 11;
    let mut second = character.clone();
    second.gold = 22;

    let (a, b) = tokio::join!(store.save_character(first), store.save_character(second));
    a.expect("first save");
    b.expect("second save");

    let row = store
        .find_character(account.id, character.id)
        .await
        .expect("load")
        .expect("row");
    assert!(
        row.gold == 11 || row.gold == 22,
        "并发保存只能留下完整的一版，实际 {}",
        row.gold
    );
    assert_eq!(row.name, "RaceHero", "其它列不能被并发写坏");
}

// ---------------------------------------------------------------------------
// 6) 重启恢复：背包 / 装备 / 技能字节块
// ---------------------------------------------------------------------------
#[tokio::test]
async fn restart_restores_inventory_equipment_and_magics_bytes() {
    let store = MemoryStore::new();
    let account = store
        .create_account("gmuser", "secret", "")
        .await
        .expect("account");
    let mut character = store
        .create_character(
            account.id,
            "GearHero",
            MirClass::Warrior,
            MirGender::Male,
            "0",
            (330, 270),
        )
        .await
        .expect("character");
    character.inventory = vec![1, 2, 3, 4];
    character.equipment = vec![5, 6, 7];
    character.magics = vec![8, 9];
    character.storage = vec![10, 11];
    store.save_character(character.clone()).await.expect("save");

    let row = store
        .find_character(account.id, character.id)
        .await
        .expect("load")
        .expect("row");
    assert_eq!(row.inventory, vec![1, 2, 3, 4]);
    assert_eq!(row.equipment, vec![5, 6, 7]);
    assert_eq!(row.magics, vec![8, 9]);
    assert_eq!(row.storage, vec![10, 11]);
}

// ---------------------------------------------------------------------------
// 7) 重启恢复：任务进度 / 任务物品栏 / 每日重置时间
// ---------------------------------------------------------------------------
#[tokio::test]
async fn restart_restores_quests_and_quest_inventory() {
    let store = MemoryStore::new();
    let account = store
        .create_account("gmuser", "secret", "")
        .await
        .expect("account");
    let mut character = store
        .create_character(
            account.id,
            "QuestHero",
            MirClass::Warrior,
            MirGender::Male,
            "0",
            (330, 270),
        )
        .await
        .expect("character");
    character.quests = vec![21, 22, 23];
    character.quest_inventory = vec![24, 25];
    character.last_daily_reset_at = 1_700_000_000_000;
    store.save_character(character.clone()).await.expect("save");

    let row = store
        .find_character(account.id, character.id)
        .await
        .expect("load")
        .expect("row");
    assert_eq!(row.quests, vec![21, 22, 23]);
    assert_eq!(row.quest_inventory, vec![24, 25]);
    assert_eq!(row.last_daily_reset_at, 1_700_000_000_000);
}

// ---------------------------------------------------------------------------
// 8) 攻城区耐久/归属状态重启恢复
// ---------------------------------------------------------------------------
#[tokio::test]
async fn conquest_state_survives_restart() {
    let store = MemoryStore::new();
    let record = ConquestRecord {
        conquest_index: 7,
        owner: Some(3),
        attacker: Some(4),
        gold_storage: 12_345,
        war_is_on: true,
        war_start_time: 100,
        war_end_time: 200,
        ..ConquestRecord::default()
    };
    store.save_conquest(record.clone()).await.expect("save");

    let rows = store.list_conquests().await.expect("list");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0], record, "攻城区整行必须原样恢复");
}

// ---------------------------------------------------------------------------
// 9) 行会整份状态重启恢复
// ---------------------------------------------------------------------------
#[tokio::test]
async fn guild_state_survives_restart() {
    let store = MemoryStore::new();
    let mut guild = GuildRecord {
        name: "AuditGuild".to_owned(),
        leader_character_id: 1,
        level: 3,
        gold: 55_000,
        notice: vec!["notice".to_owned()],
        ..GuildRecord::default()
    };
    guild.id = store.save_guild(guild.clone()).await.expect("save");
    assert!(guild.id > 0, "存储层要分配主键");

    let rows = store.list_guilds().await.expect("list");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "AuditGuild");
    assert_eq!(rows[0].gold, 55_000);
    assert_eq!(rows[0].level, 3);
}

// ---------------------------------------------------------------------------
// 10) 角色行整行恢复：金币/坐标/HP/等级一起回来
// ---------------------------------------------------------------------------
#[tokio::test]
async fn character_row_roundtrips_all_audited_columns() {
    let store = MemoryStore::new();
    let account = store
        .create_account("gmuser", "secret", "")
        .await
        .expect("account");
    let mut character = store
        .create_character(
            account.id,
            "WholeHero",
            MirClass::Warrior,
            MirGender::Male,
            "0",
            (330, 270),
        )
        .await
        .expect("character");
    character.gold = 123;
    character.credit = 45;
    character.level = 30;
    character.hp = 239;
    character.mp = 260;
    character.x = 12;
    character.y = 34;
    store.save_character(character.clone()).await.expect("save");

    let row: Character = store
        .find_character(account.id, character.id)
        .await
        .expect("load")
        .expect("row");
    assert_eq!(row.gold, 123);
    assert_eq!(row.credit, 45);
    assert_eq!(row.level, 30);
    assert_eq!(row.hp, 239);
    assert_eq!(row.mp, 260);
    assert_eq!((row.x, row.y), (12, 34));
}

// ---------------------------------------------------------------------------
// 11) 版本化写：旧快照丢弃、并发串行、失败不推进版本
// ---------------------------------------------------------------------------

async fn versioned_fixture() -> (MemoryStore, mir_server::Account, Character) {
    let store = MemoryStore::new();
    let account = store
        .create_account("gmuser", "secret", "")
        .await
        .expect("account");
    let character = store
        .create_character(
            account.id,
            "VersionHero",
            MirClass::Warrior,
            MirGender::Male,
            "0",
            (330, 270),
        )
        .await
        .expect("character");
    (store, account, character)
}

#[tokio::test]
async fn versioned_save_rejects_older_snapshot() {
    let (store, account, mut character) = versioned_fixture().await;
    character.gold = 10;
    assert!(
        store
            .save_character_versioned(character.clone(), 10)
            .await
            .expect("save v10")
    );
    character.gold = 99;
    assert!(
        !store
            .save_character_versioned(character.clone(), 9)
            .await
            .expect("stale save"),
        "旧版本必须被拒绝"
    );
    let row = store
        .find_character(account.id, character.id)
        .await
        .expect("load")
        .expect("row");
    assert_eq!(row.gold, 10, "旧版本不能覆盖新版本");
}

#[tokio::test]
async fn concurrent_versioned_saves_are_serialized_and_latest_wins() {
    let (store, account, mut character) = versioned_fixture().await;
    let mut older = character.clone();
    older.gold = 1;
    let mut newer = character.clone();
    newer.gold = 2;
    let (a, b) = tokio::join!(
        store.save_character_versioned(older, 20),
        store.save_character_versioned(newer, 21)
    );
    a.expect("older request");
    b.expect("newer request");
    character = store
        .find_character(account.id, character.id)
        .await
        .expect("load")
        .expect("row");
    assert_eq!(character.gold, 2, "最终必须是更大版本 21 的快照");
}

#[tokio::test]
async fn failed_versioned_save_does_not_advance_version() {
    let (store, account, mut character) = versioned_fixture().await;
    character.gold = 1;
    store
        .save_character_versioned(character.clone(), 30)
        .await
        .expect("base");
    character.gold = 2;
    store.fail_next_save_character().await;
    assert!(
        store
            .save_character_versioned(character.clone(), 31)
            .await
            .is_err(),
        "注入失败必须冒泡"
    );
    // 失败没有推进版本：同版本重试应仍然允许（且写的是同一快照）。
    assert!(
        store
            .save_character_versioned(character.clone(), 31)
            .await
            .expect("retry"),
        "失败后同版本重试应成功"
    );
    let row = store
        .find_character(account.id, character.id)
        .await
        .expect("load")
        .expect("row");
    assert_eq!(row.gold, 2);
}

#[tokio::test]
async fn same_version_concurrent_saves_only_one_wins() {
    let (store, account, mut character) = versioned_fixture().await;
    let mut a = character.clone();
    a.gold = 7;
    let mut b = character.clone();
    b.gold = 8;
    let (ra, rb) = tokio::join!(
        store.save_character_versioned(a, 40),
        store.save_character_versioned(b, 40)
    );
    let ra = ra.expect("a");
    let rb = rb.expect("b");
    assert_ne!(ra, rb, "同一版本只能有一个成功");
    character = store
        .find_character(account.id, character.id)
        .await
        .expect("load")
        .expect("row");
    assert!(character.gold == 7 || character.gold == 8);
}

#[tokio::test]
async fn autosave_then_logout_keeps_the_latest_versioned_snapshot() {
    let (address, store, world) = start_server_with_world(1).await;
    let (mut client, index, _) = enter_world(address).await;
    let account_id = store
        .find_account("gmuser")
        .await
        .expect("account")
        .expect("row")
        .id;
    grant_gold(&mut client, 500).await;
    advance(&world, 1_100).await; // autosave 写 500
    grant_gold(&mut client, 111).await; // 再加 111，LogOut 写 611
    client
        .send(ClientPacket::LogOut(c::LogOut::default()))
        .await;
    client.expect_index(s::LogOutSuccess::INDEX).await;
    let row = store
        .find_character(account_id, i64::from(index))
        .await
        .expect("load")
        .expect("row");
    assert_eq!(
        row.gold, 611,
        "LogOut 的版本必须大于 autosave，不能回退到 500"
    );
}

#[tokio::test]
async fn versioned_saves_still_meet_the_autosave_rpo() {
    const AUTOSAVE_SECS: u64 = 1;
    let (address, store, world) = start_server_with_world(AUTOSAVE_SECS).await;
    let (mut client, index, _) = enter_world(address).await;
    let account_id = store
        .find_account("gmuser")
        .await
        .expect("account")
        .expect("row")
        .id;
    grant_gold(&mut client, 777).await;
    advance(
        &world,
        i64::try_from(AUTOSAVE_SECS).expect("secs") * 1000 + 100,
    )
    .await;
    let row = store
        .find_character(account_id, i64::from(index))
        .await
        .expect("load")
        .expect("row");
    assert_eq!(row.gold, 777, "加锁/版本化后 RPO 仍 ≤ autosave_secs + 1 秒");
}

#[tokio::test]
async fn immediate_logout_relogin_preserves_inventory_and_equipment() {
    let (address, store) = start_server().await;
    let account = store
        .create_account("relogineq", "secret", "")
        .await
        .expect("建账号");
    let mut character = store
        .create_character(
            account.id,
            "重登装备",
            MirClass::Warrior,
            MirGender::Male,
            "0",
            (330, 270),
        )
        .await
        .expect("建角色");
    character.level = 10;
    let inventory_item = mir_protocol::data::UserItem {
        unique_id: 7_001,
        item_index: 1,
        ..Default::default()
    };
    let equipment_item = mir_protocol::data::UserItem {
        unique_id: 7_002,
        item_index: 2,
        ..Default::default()
    };
    character.inventory = mir_server::items::encode_slots(&[Some(inventory_item.clone())]);
    character.equipment = mir_server::items::encode_slots(&[Some(equipment_item.clone())]);
    store.save_character(character.clone()).await.expect("预置");

    let (mut client, index, info) = enter_world_as(address, "relogineq", "重登装备").await;
    assert_eq!(index, i32::try_from(character.id).unwrap());
    assert!(
        info.inventory
            .as_ref()
            .is_some_and(|items| items.iter().flatten().any(|item| item.unique_id == 7_001)),
        "首次登录应读到预置背包：{info:?}"
    );
    assert!(
        info.equipment
            .as_ref()
            .is_some_and(|items| items.iter().flatten().any(|item| item.unique_id == 7_002)),
        "首次登录应读到预置装备：{info:?}"
    );

    client
        .send(ClientPacket::LogOut(c::LogOut::default()))
        .await;
    client.wait_for_index(s::LogOutSuccess::INDEX).await;

    // 立刻用新连接重登，不额外等待异步任务。
    let (_again, _index, after) = enter_world_as(address, "relogineq", "重登装备").await;
    assert!(
        after
            .inventory
            .as_ref()
            .is_some_and(|items| items.iter().flatten().any(|item| item.unique_id == 7_001)),
        "立刻重登背包必须一致：{after:?}"
    );
    assert!(
        after
            .equipment
            .as_ref()
            .is_some_and(|items| items.iter().flatten().any(|item| item.unique_id == 7_002)),
        "立刻重登装备必须一致：{after:?}"
    );
}
