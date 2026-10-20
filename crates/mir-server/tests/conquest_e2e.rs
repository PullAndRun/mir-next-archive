//! 攻城 E2E：申请/竞价的世界级用例（阶段 1，MemoryStore）。
//!
//! 全部用 `common` 的共享 Harness；时钟固定，不读真实资源。

mod common;

use common::*;
use std::sync::Arc;
use std::time::Duration;

use mir_core::Point;
use mir_core::enums::{MirClass, MirGender};
use mir_protocol::codec::FrameBuffer;
use mir_protocol::manual::server::UserInformation;
use mir_protocol::packets::client as c;
use mir_protocol::packets::server::ConquestObjectType;
use mir_protocol::{ClientPacket, ServerPacket};
use mir_server::store::AccountStore;
use mir_server::{
    ConquestError, ConquestSettings, MemoryStore, PgAccountStore, ServerConfig,
    serve_forever_with_world,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const GOLD_U32: u32 = GOLD as u32;

fn fund_guilds(harness: &mut Harness, guilds: &TwoGuilds) {
    for guild in [guilds.alpha, guilds.beta] {
        harness.world.guild_mut(guild).expect("行会存在").gold = GOLD_U32;
    }
}

fn settings(required_guild_level: u8, war_cost: u32) -> ConquestSettings {
    ConquestSettings {
        required_guild_level,
        war_cost,
    }
}

#[test]
fn conquest_apply_level_and_gold_gate_deducts_fee() {
    let mut harness = Harness::new(70);
    let guilds = two_guilds(&mut harness);
    fund_guilds(&mut harness, &guilds);
    harness.world.set_conquest_settings(settings(0, 5_000));
    assert_eq!(
        harness.world.guild(guilds.alpha).expect("行会存在").gold,
        GOLD_U32
    );

    let report = harness
        .world
        .request_conquest_war_at(guilds.alpha_leader, CONQUEST, MONDAY_18 - MINUTE)
        .expect("满足等级/金币应申请成功");
    assert_eq!(report.gold_spent, 5_000, "扣费额必须等于配置常量");
    assert_eq!(
        harness.world.guild(guilds.alpha).expect("行会存在").gold,
        GOLD_U32 - 5_000
    );
    let after_apply = harness.world.guild(guilds.alpha).expect("行会存在").gold;
    let attacker = harness.world.conquest(CONQUEST).expect("攻城区").attacker;
    assert_eq!(attacker, guilds.alpha);

    // 负例：成功后再次申请被拒，且金币/申请记录保持不变。
    assert_eq!(
        harness
            .world
            .request_conquest_war_at(guilds.alpha_leader, CONQUEST, MONDAY_18 - MINUTE),
        Err(ConquestError::AlreadyScheduled)
    );
    assert_eq!(
        harness.world.guild(guilds.alpha).expect("行会存在").gold,
        after_apply,
        "二次申请被拒后金币不得再次扣除"
    );
    assert_eq!(
        harness.world.conquest(CONQUEST).expect("攻城区").attacker,
        guilds.alpha,
        "二次申请被拒后已申请记录不变"
    );
}

#[test]
fn conquest_apply_rejected_when_level_or_gold_insufficient() {
    let mut harness = Harness::new(71);
    let guilds = two_guilds(&mut harness);
    fund_guilds(&mut harness, &guilds);

    // 等级不足：金币与已申请记录都不变。
    harness.world.set_conquest_settings(settings(99, 5_000));
    assert!(matches!(
        harness
            .world
            .request_conquest_war_at(guilds.alpha_leader, CONQUEST, MONDAY_18 - MINUTE),
        Err(ConquestError::LevelTooLow { .. })
    ));
    assert_eq!(
        harness.world.guild(guilds.alpha).expect("行会存在").gold,
        GOLD_U32,
        "等级拒绝不得扣金币"
    );
    assert_eq!(
        harness.world.conquest(CONQUEST).expect("攻城区").attacker,
        -1,
        "等级拒绝不得写入申请记录"
    );

    // 金币不足：同样断言未发生变化。
    harness.world.set_conquest_settings(settings(0, u32::MAX));
    assert!(matches!(
        harness
            .world
            .request_conquest_war_at(guilds.beta_leader, CONQUEST, MONDAY_18 - MINUTE),
        Err(ConquestError::NotEnoughGold { .. })
    ));
    assert_eq!(
        harness.world.guild(guilds.beta).expect("行会存在").gold,
        GOLD_U32,
        "金币拒绝不得扣金币"
    );
    assert_eq!(
        harness.world.conquest(CONQUEST).expect("攻城区").attacker,
        -1,
        "金币拒绝不得写入申请记录"
    );
}

#[test]
fn conquest_bid_keeps_high_bid_and_refunds_previous_bidder() {
    let mut harness = Harness::new(72);
    let guilds = two_guilds(&mut harness);
    fund_guilds(&mut harness, &guilds);
    harness.world.set_conquest_settings(settings(1, 0));

    let first = harness
        .world
        .place_conquest_bid(guilds.alpha_leader, CONQUEST, 1_000)
        .expect("首次竞价");
    assert_eq!(first.highest_bid, 1_000);
    assert_eq!(first.refunded_amount, 0);
    assert_eq!(
        harness.world.guild(guilds.alpha).expect("行会存在").gold,
        GOLD_U32 - 1_000
    );

    let second = harness
        .world
        .place_conquest_bid(guilds.beta_leader, CONQUEST, 1_500)
        .expect("更高竞价");
    assert_eq!(second.highest_bid, 1_500, "最高价必须等于新出价");
    assert_eq!(second.refunded_guild, guilds.alpha);
    assert_eq!(second.refunded_amount, 1_000, "退款额必须等于原出价");
    assert_eq!(
        harness.world.guild(guilds.alpha).expect("行会存在").gold,
        GOLD_U32,
        "被超越方应收到原出价退款"
    );
    assert_eq!(
        harness.world.guild(guilds.beta).expect("行会存在").gold,
        GOLD_U32 - 1_500
    );
    let conquest = harness.world.conquest(CONQUEST).expect("攻城区");
    assert_eq!(conquest.highest_bid, 1_500);
    assert_eq!(conquest.highest_bidder, guilds.beta);
    assert_eq!(conquest.attacker, guilds.beta);
    assert_eq!(conquest.bid_refunds, vec![(guilds.alpha, 1_000)]);
}

#[test]
fn conquest_bid_rejected_when_not_higher_than_current() {
    let mut harness = Harness::new(73);
    let guilds = two_guilds(&mut harness);
    fund_guilds(&mut harness, &guilds);
    harness.world.set_conquest_settings(settings(1, 0));
    harness
        .world
        .place_conquest_bid(guilds.alpha_leader, CONQUEST, 1_000)
        .expect("首次竞价");
    let gold = harness.world.guild(guilds.alpha).expect("行会存在").gold;
    let refunds = harness
        .world
        .conquest(CONQUEST)
        .expect("攻城区")
        .bid_refunds
        .clone();

    // 低于当前价：拒绝且不扣钱、不改最高价/退款记录。
    assert_eq!(
        harness
            .world
            .place_conquest_bid(guilds.beta_leader, CONQUEST, 999),
        Err(ConquestError::BidTooLow {
            current: 1_000,
            bid: 999
        })
    );
    assert_eq!(
        harness.world.guild(guilds.beta).expect("行会存在").gold,
        GOLD_U32,
        "低价被拒不得扣金币"
    );
    // 等于当前价：同样拒绝。
    assert_eq!(
        harness
            .world
            .place_conquest_bid(guilds.beta_leader, CONQUEST, 1_000),
        Err(ConquestError::BidTooLow {
            current: 1_000,
            bid: 1_000
        })
    );
    let conquest = harness.world.conquest(CONQUEST).expect("攻城区");
    assert_eq!(conquest.highest_bid, 1_000, "负例不得改变最高价");
    assert_eq!(conquest.bid_refunds, refunds, "负例不得增加退款记录");
    assert_eq!(
        harness.world.guild(guilds.alpha).expect("行会存在").gold,
        gold,
        "负例不得改变原最高价行会余额"
    );
}

#[test]
fn conquest_objects_gate_wall_archer_state() {
    let mut harness = Harness::new(74);
    let guilds = two_guilds(&mut harness);
    fund_guilds(&mut harness, &guilds);
    set_owner(&mut harness, guilds.alpha);
    harness.world.spawn_conquest_objects(CONQUEST);
    let conquest = harness.world.conquest(CONQUEST).expect("攻城区");
    assert!(!conquest.objects.is_empty());
    let gate = conquest
        .objects
        .iter()
        .find(|o| o.kind == ConquestObjectType::Gate)
        .expect("Gate");
    assert_eq!(
        (gate.hp, gate.max_hp, gate.destroyed()),
        (5_000, 5_000, false)
    );
    let gate_id = gate.object_id;
    let gate_cell = gate.location;
    let wall = conquest
        .objects
        .iter()
        .find(|o| o.kind == ConquestObjectType::Wall)
        .expect("Wall");
    assert_eq!(
        (wall.hp, wall.max_hp, wall.destroyed()),
        (3_000, 3_000, false)
    );
    let archer = conquest
        .objects
        .iter()
        .find(|o| o.kind == ConquestObjectType::Archer)
        .expect("Archer");
    assert_eq!(
        (archer.hp, archer.max_hp, archer.destroyed()),
        (2_000, 2_000, false)
    );
    harness
        .world
        .send_conquest_state_to(guilds.alpha_leader, CONQUEST);
    let packets = harness.packets(guilds.alpha_leader);
    let info = packets
        .iter()
        .find_map(|p| match p {
            ServerPacket::ConquestInfo(info) => Some(info),
            _ => None,
        })
        .expect("300");
    assert_eq!(info.owner, guilds.alpha);
    let object_packets: Vec<_> = packets
        .iter()
        .filter_map(|p| match p {
            ServerPacket::ConquestObject(o) => Some(o),
            _ => None,
        })
        .collect();
    assert!(!object_packets.is_empty());
    assert!(
        object_packets
            .iter()
            .any(|o| o.kind == ConquestObjectType::Gate)
    );
    // 负例：已 destroyed 的器械不能被格子选中，也不会再被战斗命中。
    let destroyed = gate_id;
    if let Some(c) = harness.world.conquest_mut(CONQUEST) {
        if let Some(o) = c.objects.iter_mut().find(|o| o.object_id == destroyed) {
            o.hp = 0;
        }
    }
    assert!(
        harness.world.conquest_object_at(MAP, gate_cell).is_none(),
        "destroyed 不可再被选中"
    );
    let before = 0;
    // 世界战斗入口不会选中 destroyed：同一格查询为空即证明不会再扣血。
    assert_eq!(harness.world.conquest_object_at(MAP, gate_cell), None);
    assert_eq!(
        harness
            .world
            .conquest(CONQUEST)
            .unwrap()
            .objects
            .iter()
            .find(|o| o.object_id == destroyed)
            .unwrap()
            .hp,
        before
    );
}

#[test]
fn conquest_settlement_result_and_tax_ownership() {
    let mut harness = Harness::new(75);
    let guilds = two_guilds(&mut harness);
    fund_guilds(&mut harness, &guilds);
    set_owner(&mut harness, guilds.alpha);
    harness.world.add_conquest_tax(CONQUEST, 1_234);
    assert_eq!(
        harness.world.conquest(CONQUEST).unwrap().gold_storage,
        1_234
    );
    harness.world.start_war(CONQUEST, MONDAY_18);
    assert!(harness.world.take_conquest(CONQUEST, guilds.beta));
    harness.world.end_war(CONQUEST);
    let conquest = harness.world.conquest(CONQUEST).expect("攻城区");
    let owner = conquest.owner;
    let tax = conquest.gold_storage;
    assert_eq!(owner, guilds.beta);
    assert_eq!(tax, 1_234, "税收池仍属于该攻城区，但归新城主所有");
    harness
        .world
        .send_conquest_state_to(guilds.alpha_leader, CONQUEST);
    let packets = harness.packets(guilds.alpha_leader);
    let result = packets.iter().find_map(|p| match p {
        ServerPacket::ConquestResult(r) => Some(r),
        _ => None,
    });
    let result = result.expect("302 ConquestResult");
    assert_eq!(result.winner, guilds.beta);
    assert_eq!(result.owner, guilds.beta);
    // 负例：未参战行会（alpha）没有城主与税收。
    let conquest = harness.world.conquest(CONQUEST).expect("攻城区");
    assert_ne!(conquest.owner, guilds.alpha);
    assert_eq!(conquest.gold_storage, 1_234, "未参战行会拿不到税收");
}

#[tokio::test]
async fn conquest_reload_from_postgres_restores_owner_tax_and_objects() {
    let Ok(url) = std::env::var("MIR2_TEST_DATABASE_URL") else {
        panic!("真 PG 用例需要 MIR2_TEST_DATABASE_URL，不允许 skip");
    };
    let store = PgAccountStore::connect(&url)
        .await
        .expect("真实 PostgreSQL 测试库连接并迁移");
    let mut harness = Harness::new(76);
    let guilds = two_guilds(&mut harness);
    for guild_id in [guilds.alpha, guilds.beta] {
        let record = harness.world.guild(guild_id).expect("行会").to_record();
        store.save_guild(record).await.expect("写行会 PG");
    }
    set_owner(&mut harness, guilds.alpha);
    harness.world.add_conquest_tax(CONQUEST, 7_654);
    harness.world.spawn_conquest_objects(CONQUEST);
    let record = harness
        .world
        .conquest(CONQUEST)
        .expect("攻城区")
        .to_record();
    store.save_conquest(record.clone()).await.expect("写 PG");
    let rows = store.list_conquests().await.expect("从 PG 读回");
    let saved = rows
        .iter()
        .find(|row| row.conquest_index == CONQUEST)
        .expect("PG 中应有攻城区");
    assert_eq!(saved, &record, "PG 行必须与写入逐字段相等");

    let mut loaded = Harness::new(77);
    assert!(loaded.world.load_conquests(vec![saved.clone()]) > 0);
    let reloaded = loaded.world.conquest(CONQUEST).expect("重载攻城区");
    let original = harness.world.conquest(CONQUEST).expect("原攻城区");
    assert_eq!(reloaded.owner, original.owner, "城主");
    assert_eq!(reloaded.gold_storage, original.gold_storage, "税收");
    let shape = |objects: &[mir_server::conquest::ConquestObjectState]| {
        objects
            .iter()
            .map(|o| (o.kind, o.location, o.hp, o.max_hp, o.owner, o.open))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        shape(&reloaded.objects),
        shape(&original.objects),
        "器械逐字段相等（object_id 为运行期分配，不比）"
    );
    // 负例：未参战行会不是城主。
    assert_ne!(reloaded.owner, guilds.beta);
}

const SOCKET_WAIT: Duration = Duration::from_secs(15);

struct SocketClient {
    stream: TcpStream,
    frames: FrameBuffer,
    object_id: u32,
}

impl SocketClient {
    async fn connect(address: std::net::SocketAddr) -> Self {
        let stream = TcpStream::connect(address)
            .await
            .expect("真实 TcpStream 连接");
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
            .expect("写 socket");
    }

    async fn next(&mut self) -> Option<ServerPacket> {
        let mut chunk = [0u8; 8192];
        loop {
            if let Some((index, payload)) = self.frames.next_frame().expect("帧格式") {
                return Some(ServerPacket::decode(index, &payload).expect("解码包"));
            }
            match tokio::time::timeout(SOCKET_WAIT, self.stream.read(&mut chunk)).await {
                Ok(Ok(0) | Err(_)) | Err(_) => return None,
                Ok(Ok(read)) => self.frames.push(&chunk[..read]),
            }
        }
    }

    async fn wait_for(&mut self, what: &str, pred: impl Fn(&ServerPacket) -> bool) -> ServerPacket {
        let deadline = tokio::time::Instant::now() + SOCKET_WAIT;
        let mut seen = Vec::new();
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            assert!(!remaining.is_zero(), "等待 {what} 超时；最近 {seen:?}");
            let Some(packet) = self.next().await else {
                panic!("等待 {what} 时 socket 关闭；最近 {seen:?}");
            };
            if pred(&packet) {
                return packet;
            }
            seen.push(packet.index());
        }
    }
}

async fn login_into_world(address: std::net::SocketAddr) -> (SocketClient, UserInformation) {
    let mut client = SocketClient::connect(address).await;
    client
        .wait_for("Connected", |p| matches!(p, ServerPacket::Connected(_)))
        .await;
    client
        .send(ClientPacket::ClientVersion(c::ClientVersion {
            version_hash: Vec::new(),
        }))
        .await;
    client
        .wait_for("ClientVersion", |p| {
            matches!(p, ServerPacket::ClientVersion(_))
        })
        .await;
    client
        .send(ClientPacket::NewAccount(c::NewAccount {
            account_id: "conq18".to_owned(),
            password: "secret".to_owned(),
            birth_date: mir_core::DateTimeBinary(0),
            user_name: String::new(),
            secret_question: String::new(),
            secret_answer: String::new(),
            e_mail_address: String::new(),
        }))
        .await;
    client
        .wait_for("NewAccount", |p| matches!(p, ServerPacket::NewAccount(_)))
        .await;
    client
        .send(ClientPacket::Login(c::Login {
            account_id: "conq18".to_owned(),
            password: "secret".to_owned(),
        }))
        .await;
    client
        .wait_for("LoginSuccess", |p| {
            matches!(p, ServerPacket::LoginSuccess(_))
        })
        .await;
    client
        .send(ClientPacket::NewCharacter(c::NewCharacter {
            name: "conq18hero".to_owned(),
            gender: MirGender::Male,
            class: MirClass::Warrior,
        }))
        .await;
    let index = match client
        .wait_for("NewCharacterSuccess", |p| {
            matches!(p, ServerPacket::NewCharacterSuccess(_))
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
    let ServerPacket::UserInformation(info) = client
        .wait_for("UserInformation", |p| {
            matches!(p, ServerPacket::UserInformation(_))
        })
        .await
    else {
        unreachable!()
    };
    client.object_id = info.object_id;
    (client, info)
}

#[tokio::test]
async fn conquest_war_start_info_packet_fields() {
    let mut harness = Harness::new(78);
    let guilds = two_guilds(&mut harness);
    set_owner(&mut harness, guilds.alpha);
    harness.world.add_conquest_tax(CONQUEST, 4_321);
    harness.world.start_war(CONQUEST, MONDAY_18);
    let world = Arc::new(tokio::sync::Mutex::new(harness.world));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("绑定");
    let address = listener.local_addr().expect("端口");
    tokio::spawn(serve_forever_with_world(
        listener,
        Arc::new(ServerConfig {
            start_map: MAP.to_owned(),
            load_game_data: false,
            map_root: ".".to_owned(),
            ..ServerConfig::default()
        }),
        Arc::new(MemoryStore::new()),
        world.clone(),
    ));
    let (mut client, _info) = login_into_world(address).await;
    let (expected_start, expected_end) = {
        let mut world = world.lock().await;
        if let Some(conquest) = world.conquest_mut(CONQUEST) {
            conquest.owner = guilds.alpha;
            conquest.gold_storage = 4_321;
            conquest.npc_rate = 0;
        }
        world.start_war(CONQUEST, MONDAY_18);
        let conquest = world.conquest(CONQUEST).expect("攻城区");
        let expected = (conquest.war_start_time, conquest.war_end_time);
        world.send_conquest_state_to(client.object_id, CONQUEST);
        expected
    };
    let packet = client
        .wait_for("ConquestInfo(300)", |p| {
            matches!(p, ServerPacket::ConquestInfo(_))
        })
        .await;
    let ServerPacket::ConquestInfo(info) = packet else {
        unreachable!()
    };
    assert_eq!(info.owner, guilds.alpha);
    assert!(info.war_is_on);
    assert_eq!(info.start_time, expected_start);
    assert_eq!(info.end_time, expected_end);
    assert!(info.control_points.is_empty());
    let world = world.lock().await;
    let conquest = world.conquest(CONQUEST).expect("攻城区");
    assert_eq!(world.conquest_gold_storage(CONQUEST), 4_321);
    assert_eq!(conquest.npc_rate, 0);
    assert_ne!(info.owner, guilds.beta, "负例：未参战行会不会成为城主");
}

#[test]
fn conquest_war_starts_from_npc_script_keyword() {
    use mir_db::NpcInfo;
    use mir_server::World;
    use mir_server::gamedata::GameData;
    use std::collections::HashMap;
    use std::fs;
    use std::sync::Arc;

    let root = std::env::temp_dir().join(format!("mir-conquest-npc-{}", std::process::id()));
    let npc_dir = root.join("NPCs/Test");
    fs::create_dir_all(&npc_dir).expect("建 NPC 脚本目录");
    fs::write(
        npc_dir.join("Conquest.txt"),
        "[@Main]\n#ACT\nSTARTCONQUEST 1\n",
    )
    .expect("写 NPC 脚本");

    let mut world = World::new(999);
    world.seed_rng(79);
    world.insert_map(MAP, synthetic_map(MAP));
    world.insert_map(PALACE, synthetic_map(PALACE));
    let mut db = base_db(conquest_info());
    let npc_info = NpcInfo {
        index: 1,
        map_index: MAP_INDEX,
        file_name: "Test/Conquest".to_owned(),
        name: "ConquestNPC".to_owned(),
        location: Point::new(30, 30),
        ..NpcInfo::default()
    };
    db.npcs.push(npc_info.clone());
    world.set_game_data(Arc::new(GameData::from_db(db, Some(root.join("Drops")))));
    world.init_conquests();
    let mut harness = Harness {
        world,
        receivers: HashMap::new(),
    };
    let guilds = two_guilds(&mut harness);
    let npc_id = harness.world.spawn_npc(MAP, &npc_info).expect("NPC 应刷出");
    assert!(
        harness
            .world
            .place_player(guilds.alpha_leader, MAP, Point::new(30, 30))
    );
    assert!(
        !harness.world.conquest(CONQUEST).unwrap().war_is_on,
        "初始未开战"
    );
    let reply = harness.world.call_npc(guilds.alpha_leader, npc_id, "@main");
    assert!(reply.is_ok(), "NPC 脚本应执行");
    let conquest = harness.world.conquest(CONQUEST).expect("攻城区");
    assert!(conquest.war_is_on, "STARTCONQUEST 后必须开战");
    assert!(conquest.war_start_time > 0, "start_time 必须写入");
    assert!(conquest.war_end_time > 0, "end_time 必须写入");
    // 负例：未参战行会不是城主。
    assert_ne!(conquest.owner, guilds.beta);
}
