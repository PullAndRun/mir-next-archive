#![allow(
    clippy::unused_async,
    clippy::option_map_unit_fn,
    clippy::manual_let_else
)]
//! 故障注入：Redis 无依赖证明 + 存储写失败不丢数据（第一增量）。
//!
//! 强杀 RPO 用例见后续单独提交；本文件先覆盖可独立验证的两条。

use std::sync::Arc;

use mir_core::enums::{MirClass, MirGender};
use mir_protocol::codec::FrameBuffer;
use mir_protocol::packets::client as c;
use mir_protocol::{ClientPacket, ServerPacket};
use mir_server::store::AccountStore;
use mir_server::{MemoryStore, ServerConfig, serve_forever};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

static RPO_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn redis_absent_does_not_break_server_startup_or_protocol_handshake() {
    // 本项目没有任何 Redis 客户端依赖；REDIS_URL 置空只影响运维环境，不应改变启动。
    let config = ServerConfig {
        map_root: "resource/不存在的目录".to_owned(),
        load_game_data: false,
        idle_timeout_secs: 0,
        max_connections_per_ip: 0,
        max_connections_per_second: 0,
        max_packets_per_second: 0,
        ..ServerConfig::default()
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("绑定临时端口");
    let address = listener.local_addr().expect("端口");
    tokio::spawn(serve_forever(
        listener,
        Arc::new(config),
        Arc::new(MemoryStore::new()),
    ));

    let mut stream = TcpStream::connect(address).await.expect("连接服务端");
    let mut frames = FrameBuffer::new();
    let mut chunk = [0u8; 2048];
    let read = tokio::time::timeout(std::time::Duration::from_secs(3), stream.read(&mut chunk))
        .await
        .expect("等待 S.Connected 超时")
        .expect("读取 S.Connected");
    assert!(read > 0, "服务端提前关闭");
    frames.push(&chunk[..read]);
    let (index, payload) = frames
        .next_frame()
        .expect("帧格式")
        .expect("S.Connected 帧");
    assert!(matches!(
        ServerPacket::decode(index, &payload).expect("解码"),
        ServerPacket::Connected(_)
    ));
    // 版本握手也必须在不依赖 Redis 的情况下正常工作。
    stream
        .write_all(
            &ClientPacket::ClientVersion(c::ClientVersion {
                version_hash: Vec::new(),
            })
            .encode_frame(),
        )
        .await
        .expect("发送版本");
    let read = tokio::time::timeout(std::time::Duration::from_secs(3), stream.read(&mut chunk))
        .await
        .expect("等待 S.ClientVersion 超时")
        .expect("读取 S.ClientVersion");
    assert!(read > 0);
}

#[tokio::test]
async fn save_failure_keeps_old_row_and_next_success_persists_new_row() {
    let store = Arc::new(MemoryStore::new());
    let account = store
        .create_account("fault-write", "secret", "")
        .await
        .expect("建号");
    let character = store
        .create_character(
            account.id,
            "故障写角色",
            MirClass::Warrior,
            MirGender::Male,
            "0",
            (10, 10),
        )
        .await
        .expect("建角");
    let id = character.id;

    let mut first = character;
    first.gold = 111;
    store.save_character(first.clone()).await.expect("首次成功");
    let before = store
        .find_character(account.id, id)
        .await
        .expect("读库")
        .expect("角色存在");
    assert_eq!(before.gold, 111);

    let mut dirty = before.clone();
    dirty.gold = 999;
    store.fail_next_save_character().await;
    assert!(
        store.save_character(dirty.clone()).await.is_err(),
        "注入失败必须返回 Err"
    );
    let after_failure = store
        .find_character(account.id, id)
        .await
        .expect("读库")
        .expect("角色存在");
    assert_eq!(after_failure.gold, 111, "失败后数据库必须保留旧行");
    assert_eq!(dirty.gold, 999, "内存快照的脏值仍在");

    store.save_character(dirty.clone()).await.expect("重试成功");
    let after_success = store
        .find_character(account.id, id)
        .await
        .expect("读库")
        .expect("角色存在");
    assert_eq!(after_success.gold, 999, "第 N+1 次成功后 DB == 内存快照");
    assert_eq!(after_success, dirty);
}

// ---------------------------------------------------------------------------
// RPO：真实独立 server 进程 + TerminateProcess 强杀 + DB 持久化核对
// ---------------------------------------------------------------------------

use std::process::{Child, Command};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct RpoServer {
    child: Option<Child>,
    pid: u32,
    dir: std::path::PathBuf,
}

impl Drop for RpoServer {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-Command",
                    &format!(
                        "Stop-Process -Id {} -Force -ErrorAction SilentlyContinue",
                        self.pid
                    ),
                ])
                .status();
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("时钟")
        .as_millis()
}

async fn wait_port(port: u16, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        if TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "端口 {port} 未在 {timeout:?} 内监听"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn latest_db_save_log(dir: &std::path::Path) -> Option<(u64, i64)> {
    let text = std::fs::read_to_string(dir.join("server.err.log")).ok()?;
    let mut best: Option<(u64, i64)> = None;
    for line in text.lines() {
        let Some(rest) = line.split("db_save ok ").nth(1) else {
            continue;
        };
        let mut seq = None;
        let mut t = None;
        for token in rest.split_whitespace() {
            if let Some(value) = token.strip_prefix("seq=") {
                seq = value.parse::<u64>().ok();
            } else if let Some(value) = token.strip_prefix("t=") {
                t = value.parse::<i64>().ok();
            }
        }
        if let (Some(seq), Some(t)) = (seq, t)
            && best.is_none_or(|(old, _)| seq > old)
        {
            best = Some((seq, t));
        }
    }
    best
}

async fn db_gold(url: &str, character_name: &str) -> Option<u64> {
    let pool = sqlx::PgPool::connect(url).await.ok()?;
    let value: Option<i64> = sqlx::query_scalar("SELECT gold FROM characters WHERE name = $1")
        .bind(character_name)
        .fetch_optional(&pool)
        .await
        .ok()
        .flatten();
    pool.close().await;
    value.and_then(|gold| u64::try_from(gold).ok())
}

struct RpoClient {
    stream: TcpStream,
    frames: FrameBuffer,
}

impl RpoClient {
    async fn connect(port: u16) -> Self {
        let stream = TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("连接 server");
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
    async fn expect(&mut self) -> ServerPacket {
        for _ in 0..100 {
            if let Some((index, payload)) = self.frames.next_frame().expect("帧") {
                return ServerPacket::decode(index, &payload).expect("解码");
            }
            let mut chunk = [0u8; 4096];
            let n = tokio::time::timeout(Duration::from_secs(5), self.stream.read(&mut chunk))
                .await
                .expect("读超时")
                .expect("读失败");
            assert!(n > 0, "server 提前关闭");
            self.frames.push(&chunk[..n]);
        }
        panic!("等待服务端包超时");
    }
    async fn login_new(&mut self, user: &str) -> u32 {
        let _ = self.expect().await;
        self.send(ClientPacket::ClientVersion(c::ClientVersion {
            version_hash: vec![0; 16],
        }))
        .await;
        let _ = self.expect().await;
        self.send(ClientPacket::NewAccount(c::NewAccount {
            account_id: user.to_owned(),
            password: "secret".to_owned(),
            birth_date: mir_core::DateTimeBinary(0),
            user_name: String::new(),
            secret_question: String::new(),
            secret_answer: String::new(),
            e_mail_address: String::new(),
        }))
        .await;
        let _ = self.expect().await;
        self.send(ClientPacket::Login(c::Login {
            account_id: user.to_owned(),
            password: "secret".to_owned(),
        }))
        .await;
        let _ = self.expect().await;
        self.send(ClientPacket::NewCharacter(c::NewCharacter {
            name: format!("rpo{user}"),
            gender: MirGender::Male,
            class: MirClass::Warrior,
        }))
        .await;
        let packet = self.expect().await;
        let ServerPacket::NewCharacterSuccess(success) = packet else {
            panic!("建角失败：{packet:?}")
        };
        self.send(ClientPacket::StartGame(c::StartGame {
            character_index: success.char_info.index,
        }))
        .await;
        let mut info_index = 0;
        for _ in 0..30 {
            let packet = self.expect().await;
            if let ServerPacket::UserInformation(info) = packet {
                info_index = info.object_id;
                break;
            }
        }
        assert_ne!(info_index, 0, "未进入世界");
        info_index
    }
    async fn login_existing(&mut self, user: &str) -> u64 {
        let _ = self.expect().await;
        self.send(ClientPacket::ClientVersion(c::ClientVersion {
            version_hash: vec![0; 16],
        }))
        .await;
        let _ = self.expect().await;
        self.send(ClientPacket::Login(c::Login {
            account_id: user.to_owned(),
            password: "secret".to_owned(),
        }))
        .await;
        let packet = self.expect().await;
        let ServerPacket::LoginSuccess(success) = packet else {
            panic!("登录失败：{packet:?}")
        };
        let index = success.characters.first().expect("角色存在").index;
        self.send(ClientPacket::StartGame(c::StartGame {
            character_index: index,
        }))
        .await;
        for _ in 0..30 {
            let packet = self.expect().await;
            if let ServerPacket::UserInformation(info) = packet {
                return u64::from(info.gold);
            }
        }
        panic!("重启后未收到 UserInformation");
    }
}

fn spawn_rpo_server(
    exe: &std::path::Path,
    dir: &std::path::Path,
    port: u16,
    metrics_port: u16,
    url: &str,
    gm: &str,
) -> RpoServer {
    std::fs::create_dir_all(dir).expect("建临时目录");
    let copy = dir.join("fault-server.exe");
    if !copy.exists() {
        std::fs::copy(exe, &copy).expect("复制 server");
    }
    let log = std::fs::File::create(dir.join("server.err.log")).expect("创建 server 日志");
    let child = Command::new(&copy)
        .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .env("MIR2_LISTEN", format!("127.0.0.1:{port}"))
        .env("MIR2_METRICS_LISTEN", format!("127.0.0.1:{metrics_port}"))
        .env("MIR2_AUTOSAVE_SECS", "1")
        .env("MIR2_LOAD_GAME_DATA", "0")
        .env("MIR2_GM_ACCOUNTS", gm)
        .env("MIR2_DATABASE_URL", url)
        .env("RUST_LOG", "info")
        .stdout(std::process::Stdio::from(log))
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("启动 server");
    let pid = child.id();
    RpoServer {
        child: Some(child),
        pid,
        dir: dir.to_owned(),
    }
}

#[tokio::test]

async fn rpo_bounded_by_autosave_after_hard_kill() {
    let Ok(url) = std::env::var("MIR2_TEST_DATABASE_URL") else {
        panic!("MIR2_TEST_DATABASE_URL 未设置；fail-loud")
    };
    let _guard = RPO_LOCK.lock().await;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let exe = root.join("target/debug/mir-server.exe");
    assert!(exe.exists(), "先 cargo build -p mir-server");
    let stamp = now_millis();
    let port = 7051 + u16::try_from(stamp % 20).unwrap_or(0);
    let metrics_port = 9095;
    let user = format!("rpo{}", stamp % 1_000_000);
    let dir = root.join(format!("target/rpo-{stamp}"));
    let tagged = format!(
        "{}?application_name=rpo_test",
        url.split('?').next().unwrap_or(&url)
    );
    let mut server = spawn_rpo_server(&exe, &dir, port, metrics_port, &tagged, &user);
    wait_port(port, Duration::from_secs(15)).await;
    let mut client = RpoClient::connect(port).await;
    let _ = client.login_new(&user).await;
    let character_name = format!("rpo{user}");
    // 1/2/4/8/16 是测试装置而非玩家 gate：幂次和（二进制位）唯一，强杀后
    // 从最终 gold 反解“哪些写持久化”，丢失集合不会与某次写混淆。
    let markers: [u64; 5] = [1, 2, 4, 8, 16];
    let base_seq = latest_db_save_log(&dir).map_or(0, |(seq, _)| seq);
    println!("base save seq={base_seq}");

    // 每次写一个唯一幂次标记：DB 最终 gold 的二进制位可直接解码“哪些写持久化了”。
    let mut writes: Vec<(u128, u64)> = Vec::new();
    // writes.0 是每次 @gold 的实际执行时刻（t_confirm 的 wall-clock 对照），writes.1 是幂次标记。
    for marker in markers {
        client
            .send(ClientPacket::Chat(c::Chat {
                message: format!("@gold {marker}"),
                linked_items: Vec::new(),
            }))
            .await;
        writes.push((now_millis(), marker));
        tokio::time::sleep(Duration::from_millis(1_100)).await;
    }
    // 轮询 DB，记录每个唯一标记被实际观察到的时刻；未确认的写允许在强杀后丢失。
    let mut confirmations: Vec<Option<u128>> = vec![None; markers.len()];
    let confirm_deadline = Instant::now() + Duration::from_secs(12);
    while Instant::now() < confirm_deadline && confirmations.iter().any(Option::is_none) {
        if let Some(gold) = db_gold(&tagged, &character_name).await {
            let now = now_millis();
            for (index, marker) in markers.iter().enumerate() {
                if gold & marker != 0 && confirmations[index].is_none() {
                    confirmations[index] = Some(now);
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    // T_save：读服务端日志里 max(seq) 的 t（不依赖 /metrics，也不取文件尾行）。
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut t_save = None;
    while Instant::now() < deadline {
        if let Some((seq, t)) = latest_db_save_log(&dir)
            && seq > base_seq
        {
            t_save = Some(t);
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let t_save = match t_save {
        Some(t) => t,
        None => {
            panic!(
                "10s 内没有观察到 db_save 日志；latest={:?}",
                latest_db_save_log(&dir)
            )
        }
    };
    let t_save_ms = u128::try_from(t_save).expect("t_save");
    let offset = 300 + (stamp % 600);
    tokio::time::sleep(Duration::from_millis(u64::try_from(offset).unwrap())).await;
    let kill_wall = now_millis();

    let pid = server.pid;
    let kill_status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!("Stop-Process -Id {pid} -Force"),
        ])
        .status()
        .expect("Stop-Process");
    assert!(kill_status.success(), "强制终止失败");
    // 等旧进程退出、端口释放。
    let _ = server.child.take().and_then(|mut child| child.wait().ok());
    tokio::time::sleep(Duration::from_millis(500)).await;

    let mut restarted = spawn_rpo_server(&exe, &dir, port, metrics_port, &tagged, &user);
    wait_port(port, Duration::from_secs(15)).await;
    let mut after = RpoClient::connect(port).await;
    let persisted_gold = after.login_existing(&user).await;
    // 停止/清理必须在断言前完成（失败也不留进程）。
    restarted.child.take().map(|mut child| {
        let _ = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!("Stop-Process -Id {} -Force", restarted.pid),
            ])
            .status();
        let _ = child.wait();
    });
    drop(restarted);
    let persisted_bits: Vec<bool> = markers
        .iter()
        .map(|marker| persisted_gold & marker != 0)
        .collect();
    let persisted = persisted_bits.iter().filter(|bit| **bit).count();
    let lost: Vec<u64> = markers
        .iter()
        .zip(&persisted_bits)
        .filter(|(_, kept)| !**kept)
        .map(|(marker, _)| *marker)
        .collect();
    let confirmed_before_kill: Vec<usize> = confirmations
        .iter()
        .enumerate()
        .filter_map(|(index, at)| (*at).filter(|t| *t + 2_000 <= kill_wall).map(|_| index))
        .collect();
    let max_confirm = confirmations.iter().flatten().max().copied();
    let window_ms = max_confirm.map_or(0, |at| kill_wall.saturating_sub(at));
    let kill_minus_save = kill_wall.saturating_sub(t_save_ms);
    println!(
        "RPO K=5 P={persisted} gold={persisted_gold} lost={lost:?} writes={writes:?} \
         T_save={t_save} kill={kill_wall} kill_minus_save_ms={kill_minus_save} \
         confirmations={confirmations:?} window_ms={window_ms}"
    );
    assert!(kill_wall > t_save_ms, "kill 应在 T_save 后发生");
    for (index, confirmed_at) in confirmations.iter().enumerate() {
        if confirmed_at.as_ref().is_some_and(|at| *at <= kill_wall) {
            assert!(
                persisted_bits[index],
                "已确认的写必须保留：marker={} confirmations={confirmations:?}",
                markers[index]
            );
        }
    }
    for index in confirmed_before_kill {
        assert!(
            persisted_bits[index],
            "kill-2s 前已确认的写必须保留：marker={} confirmations={confirmations:?}",
            markers[index]
        );
    }
    for marker in &lost {
        let index = markers
            .iter()
            .position(|candidate| candidate == marker)
            .expect("marker");
        assert!(
            confirmations[index].is_none(),
            "丢失的写不能是已确认的：marker={marker} confirmations={confirmations:?}"
        );
    }
    assert!(
        window_ms <= 2_000,
        "kill 距最近已确认写必须 ≤2s，实际 {window_ms}ms（confirmations={confirmations:?}）"
    );
    assert!(kill_minus_save <= 2_000, "kill 距 T_save 必须 ≤2s");
}
