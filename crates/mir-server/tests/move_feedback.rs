//! 移动拒绝反馈：撞墙/占用等失败时回 `S.UserLocation`(23) 的真实坐标。
//!
//! 只用现有协议，成功路径不发额外包；200ms 节流 + 去重避免连按撞墙弹跳。

use std::sync::Arc;
use std::time::Duration;

use mir_core::enums::{MirClass, MirDirection, MirGender};
use mir_core::{CellAttribute, Point};
use mir_protocol::codec::FrameBuffer;
use mir_protocol::packets::client as c;
use mir_protocol::{ClientPacket, ServerPacket};
use mir_server::world::World;
use mir_server::{MemoryStore, ServerConfig, serve_forever_with_world};
use mir_world::{Map, MapCell, MapVersion};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

struct Client {
    stream: TcpStream,
    frames: FrameBuffer,
}

impl Client {
    async fn connect(address: std::net::SocketAddr) -> Self {
        let stream = TcpStream::connect(address).await.expect("连接");
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
            .expect("发送");
    }
    async fn next(&mut self, timeout: Duration) -> Option<ServerPacket> {
        let deadline = tokio::time::Instant::now() + timeout;
        let mut chunk = [0u8; 8192];
        loop {
            if let Some((index, payload)) = self.frames.next_frame().expect("帧") {
                return Some(ServerPacket::decode(index, &payload).expect("解码"));
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
    async fn wait_for(&mut self, pred: impl Fn(&ServerPacket) -> bool) -> ServerPacket {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let Some(packet) = self.next(remaining).await else {
                panic!("等待服务端包超时")
            };
            if pred(&packet) {
                return packet;
            }
        }
    }
    async fn login(&mut self, account: &str, name: &str) {
        self.wait_for(|p| matches!(p, ServerPacket::Connected(_)))
            .await;
        self.send(ClientPacket::ClientVersion(c::ClientVersion {
            version_hash: Vec::new(),
        }))
        .await;
        self.wait_for(|p| matches!(p, ServerPacket::ClientVersion(_)))
            .await;
        self.send(ClientPacket::NewAccount(c::NewAccount {
            account_id: account.to_owned(),
            password: "secret".to_owned(),
            birth_date: mir_core::DateTimeBinary(0),
            user_name: String::new(),
            secret_question: String::new(),
            secret_answer: String::new(),
            e_mail_address: String::new(),
        }))
        .await;
        self.wait_for(|p| matches!(p, ServerPacket::NewAccount(_)))
            .await;
        self.send(ClientPacket::Login(c::Login {
            account_id: account.to_owned(),
            password: "secret".to_owned(),
        }))
        .await;
        self.wait_for(|p| matches!(p, ServerPacket::LoginSuccess(_)))
            .await;
        self.send(ClientPacket::NewCharacter(c::NewCharacter {
            name: name.to_owned(),
            gender: MirGender::Male,
            class: MirClass::Warrior,
        }))
        .await;
        let packet = self
            .wait_for(|p| matches!(p, ServerPacket::NewCharacterSuccess(_)))
            .await;
        let ServerPacket::NewCharacterSuccess(success) = packet else {
            unreachable!()
        };
        self.send(ClientPacket::StartGame(c::StartGame {
            character_index: success.char_info.index,
        }))
        .await;
        self.wait_for(|p| matches!(p, ServerPacket::UserInformation(_)))
            .await;
    }
}

fn map_with(width: i32, height: i32, blocked: &[Point]) -> Arc<Map> {
    let mut cells = vec![MapCell::default(); usize::try_from(width * height).unwrap()];
    for point in blocked {
        let index = usize::try_from(point.x * height + point.y).expect("格索引");
        cells[index].attribute = CellAttribute::HighWall;
    }
    Arc::new(Map {
        width,
        height,
        cells,
        file_name: "0".to_owned(),
        version: MapVersion::V0,
        doors: Vec::new(),
    })
}

async fn boot(start: Point, blocked: &[Point]) -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("绑定");
    let address = listener.local_addr().expect("地址");
    let mut world = World::new(7);
    world.insert_map("0", map_with(400, 400, blocked));
    let config = ServerConfig {
        start_map: "0".to_owned(),
        start_point: (start.x, start.y),
        map_root: "resource/不存在的目录".to_owned(),
        load_game_data: false,
        autosave_secs: 0,
        idle_timeout_secs: 0,
        max_connections_per_ip: 0,
        max_connections_per_second: 0,
        max_packets_per_second: 0,
        ..ServerConfig::default()
    };
    let world = Arc::new(Mutex::new(world));
    tokio::spawn(serve_forever_with_world(
        listener,
        Arc::new(config),
        Arc::new(MemoryStore::new()),
        world,
    ));
    address
}

fn location(packet: &ServerPacket) -> Option<(Point, MirDirection)> {
    match packet {
        ServerPacket::UserLocation(location) => Some((location.location, location.direction)),
        _ => None,
    }
}

async fn collect_rejections(client: &mut Client, duration: Duration) -> Vec<(Point, MirDirection)> {
    let deadline = tokio::time::Instant::now() + duration;
    let mut out = Vec::new();
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if let Some(packet) = client.next(remaining).await {
            if let Some(location) = location(&packet) {
                out.push(location);
            }
        }
    }
    out
}

#[tokio::test]
async fn wall_rejection_sends_real_location() {
    let address = boot(Point::new(0, 0), &[Point::new(1, 0)]).await;
    let mut client = Client::connect(address).await;
    client.login("movewall", "移墙者").await;
    client
        .send(ClientPacket::Walk(c::Walk {
            direction: MirDirection::Right,
        }))
        .await;
    let packet = client.wait_for(|p| location(p).is_some()).await;
    assert_eq!(
        location(&packet),
        Some((Point::new(0, 0), MirDirection::Down))
    );
}

#[tokio::test]
async fn busy_retry_success_does_not_send_rejection() {
    let address = boot(Point::new(0, 0), &[]).await;
    let mut client = Client::connect(address).await;
    client.login("movebusy", "移动忙者").await;
    client
        .send(ClientPacket::Walk(c::Walk {
            direction: MirDirection::Down,
        }))
        .await;
    client
        .send(ClientPacket::Walk(c::Walk {
            direction: MirDirection::Down,
        }))
        .await;
    let got = collect_rejections(&mut client, Duration::from_millis(1_200)).await;
    assert!(
        got.len() <= 2,
        "Busy 重试成功后不得有额外回拉，实际 {got:?}"
    );
    assert_eq!(
        got.last().copied(),
        Some((Point::new(0, 2), MirDirection::Down)),
        "重试成功应到 (0,2)：{got:?}"
    );
}

#[tokio::test]
async fn ten_wall_bumps_are_throttled_and_do_not_bounce() {
    let address = boot(Point::new(0, 0), &[Point::new(1, 0)]).await;
    let mut client = Client::connect(address).await;
    client.login("movebump", "撞墙者").await;
    for _ in 0..10 {
        client
            .send(ClientPacket::Walk(c::Walk {
                direction: MirDirection::Right,
            }))
            .await;
    }
    let got = collect_rejections(&mut client, Duration::from_millis(1_000)).await;
    println!("wall_bumps packets={} locations={got:?}", got.len());
    assert!(got.len() <= 6, "10 次撞墙回拉应 ≤6，实际 {}", got.len());
    assert!(!got.is_empty(), "至少应收到一次真实坐标回拉");
    assert!(
        got.iter().all(
            |(point, direction)| *point == Point::new(0, 0) && *direction == MirDirection::Down
        ),
        "回拉坐标必须始终是真实坐标 (0,0)，不得弹跳：{got:?}"
    );
}

#[tokio::test]
async fn successful_walk_does_not_send_extra_user_location() {
    let address = boot(Point::new(0, 0), &[]).await;
    let mut client = Client::connect(address).await;
    client.login("moveok", "移动成功者").await;
    client
        .send(ClientPacket::Walk(c::Walk {
            direction: MirDirection::Down,
        }))
        .await;
    let got = collect_rejections(&mut client, Duration::from_millis(500)).await;
    assert_eq!(
        got,
        vec![(Point::new(0, 1), MirDirection::Down)],
        "成功路径只应有正常 UserLocation，不能有额外回拉"
    );
}

#[tokio::test]
async fn blocked_370_310_pulls_back_exact() {
    let address = boot(Point::new(370, 310), &[Point::new(371, 310)]).await;
    let mut client = Client::connect(address).await;
    client.login("move370", "坐标者").await;
    client
        .send(ClientPacket::Walk(c::Walk {
            direction: MirDirection::Right,
        }))
        .await;
    let packet = client.wait_for(|p| location(p).is_some()).await;
    assert_eq!(
        location(&packet),
        Some((Point::new(370, 310), MirDirection::Down))
    );
}

#[test]
fn real_map_cell_370_310_diagnostic() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resource/map/0.map");
    if !path.exists() {
        eprintln!("skip: {} 不存在", path.display());
        return;
    }
    let map = Map::load(&path).expect("加载真实地图");
    for point in [Point::new(370, 310), Point::new(371, 310)] {
        let cell = &map.cells[usize::try_from(point.x * map.height + point.y).expect("index")];
        println!(
            "map=0 point=({},{}) attribute={:?} door={:?}",
            point.x, point.y, cell.attribute, cell.door_index
        );
    }
}

#[tokio::test]
async fn second_wall_episode_after_leaving_and_returning_gets_feedback() {
    let address = boot(Point::new(0, 0), &[Point::new(1, 0)]).await;
    let mut client = Client::connect(address).await;
    client.login("moveagain", "再撞者").await;

    client
        .send(ClientPacket::Walk(c::Walk {
            direction: MirDirection::Right,
        }))
        .await;
    let first = client.wait_for(|p| location(p).is_some()).await;
    assert_eq!(
        location(&first),
        Some((Point::new(0, 0), MirDirection::Down))
    );

    tokio::time::sleep(Duration::from_millis(700)).await;
    client
        .send(ClientPacket::Walk(c::Walk {
            direction: MirDirection::Down,
        }))
        .await;
    let _ = collect_rejections(&mut client, Duration::from_millis(200)).await;
    tokio::time::sleep(Duration::from_millis(700)).await;
    client
        .send(ClientPacket::Walk(c::Walk {
            direction: MirDirection::Up,
        }))
        .await;
    let _ = collect_rejections(&mut client, Duration::from_millis(200)).await;
    tokio::time::sleep(Duration::from_millis(2_100)).await;

    client
        .send(ClientPacket::Walk(c::Walk {
            direction: MirDirection::Right,
        }))
        .await;
    let again = collect_rejections(&mut client, Duration::from_millis(400)).await;
    assert_eq!(
        again.len(),
        1,
        "离开再回来后的新撞墙 episode 必须仍收到一次回拉：{again:?}"
    );
    assert_eq!(
        again[0],
        (Point::new(0, 0), MirDirection::Up),
        "回拉坐标/朝向必须是真实状态"
    );
}
