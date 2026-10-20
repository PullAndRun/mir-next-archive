//! 抗刷（连接限流 / 心跳超时 / 包速率）的端到端测试。
//!
//! 每个用例都起一个真的 `serve_forever`（绑 `127.0.0.1:0`）并用真的 TCP 连接打，
//! 阈值通过 [`ServerConfig`] 调到「几秒内就能观察」的量级，不 sleep 长等。

use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use mir_protocol::codec::FrameBuffer;
use mir_protocol::packets::client as c;
use mir_protocol::packets::server as s;
use mir_protocol::{ClientPacket, ServerPacket};
use mir_server::server::{ConnReject, ConnectionLimiter};
use mir_server::{MemoryStore, ServerConfig, serve_forever};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// 一份「不校验版本、不读真实数据」的测试配置；阈值由各用例自己覆盖。
fn base_config() -> ServerConfig {
    ServerConfig {
        map_root: "resource/不存在的目录".to_owned(),
        load_game_data: false,
        idle_timeout_secs: 0,
        max_connections_per_ip: 0,
        max_connections_per_second: 0,
        max_packets_per_second: 0,
        ..ServerConfig::default()
    }
}

async fn start(config: ServerConfig) -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("绑定临时端口");
    let address = listener.local_addr().expect("取端口");
    tokio::spawn(serve_forever(
        listener,
        Arc::new(config),
        Arc::new(MemoryStore::new()),
    ));
    address
}

/// 连上并等到 `S.Connected`（说明服务端真的接住了这条连接）。
async fn connect(address: std::net::SocketAddr) -> (TcpStream, FrameBuffer) {
    let mut stream = TcpStream::connect(address).await.expect("连接服务端");
    stream.set_nodelay(true).ok();
    let mut frames = FrameBuffer::new();
    let mut chunk = [0u8; 4096];
    let read = tokio::time::timeout(Duration::from_secs(3), stream.read(&mut chunk))
        .await
        .expect("等待 Connected 超时")
        .expect("读取 Connected");
    assert!(read > 0, "服务端提前关闭连接");
    frames.push(&chunk[..read]);
    (stream, frames)
}

/// 立刻能读到 EOF/错误 = 服务端已经关掉这条连接。
async fn expect_closed_within(stream: &mut TcpStream, timeout: Duration) -> bool {
    let mut chunk = [0u8; 256];
    match tokio::time::timeout(timeout, stream.read(&mut chunk)).await {
        Ok(Ok(read)) => read == 0,
        Ok(Err(_)) => true,
        Err(_) => false,
    }
}

#[tokio::test]
async fn per_ip_concurrent_limit_closes_extra_connections() {
    let address = start(ServerConfig {
        max_connections_per_ip: 2,
        ..base_config()
    })
    .await;
    let (first, _f1) = connect(address).await;
    let (second, _f2) = connect(address).await;
    // 第三条同 IP 的连接会被 accept 后立刻关掉。
    let mut third = TcpStream::connect(address).await.expect("连接服务端");
    assert!(
        expect_closed_within(&mut third, Duration::from_secs(3)).await,
        "第三条同 IP 连接应被限流关闭"
    );
    drop((first, second));
}

#[tokio::test]
async fn per_second_limit_closes_burst_connections() {
    let address = start(ServerConfig {
        max_connections_per_second: 1,
        ..base_config()
    })
    .await;
    let (first, _f1) = connect(address).await;
    let mut second = TcpStream::connect(address).await.expect("连接服务端");
    assert!(
        expect_closed_within(&mut second, Duration::from_secs(3)).await,
        "同一秒的第二条连接应被限流关闭"
    );
    drop(first);
}

#[tokio::test]
async fn idle_client_is_disconnected_by_heartbeat_timeout() {
    let address = start(ServerConfig {
        idle_timeout_secs: 1,
        ..base_config()
    })
    .await;
    let (mut stream, _frames) = connect(address).await;
    // 什么都不发：1 秒后服务端应按心跳超时断开。
    assert!(
        expect_closed_within(&mut stream, Duration::from_secs(5)).await,
        "空闲连接应被心跳超时断开"
    );
}

#[tokio::test]
async fn packet_flood_is_dropped_but_keeps_the_connection() {
    let address = start(ServerConfig {
        max_packets_per_second: 3,
        ..base_config()
    })
    .await;
    let (mut stream, mut frames) = connect(address).await;
    // 客户端包要带版本阶段才处理：先发 `ClientVersion`，再灌 `KeepAlive`。
    stream
        .write_all(
            &ClientPacket::ClientVersion(c::ClientVersion {
                version_hash: Vec::new(),
            })
            .encode_frame(),
        )
        .await
        .expect("发送版本");
    for _ in 0..40 {
        stream
            .write_all(&ClientPacket::KeepAlive(c::KeepAlive { time: 0 }).encode_frame())
            .await
            .expect("发送 KeepAlive");
    }
    // 只处理 3 个（版本 + 2 个 KeepAlive），其余丢弃；关键是不踢线。
    let mut replies = 0;
    let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
    let mut chunk = [0u8; 4096];
    loop {
        while let Some((index, payload)) = frames.next_frame().expect("帧格式") {
            let _packet = ServerPacket::decode(index, &payload).expect("解析服务端包");
            replies += 1;
        }
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        match tokio::time::timeout(Duration::from_millis(100), stream.read(&mut chunk)).await {
            Ok(Ok(0)) => panic!("包速率超限不应该踢线"),
            Ok(Ok(read)) => frames.push(&chunk[..read]),
            Ok(Err(error)) => panic!("连接被关闭：{error}"),
            Err(_) => {}
        }
    }
    assert!(replies >= 1, "至少应收到版本回包，实际 {replies}");
    // 每秒窗口重置后还能继续通信（连接没被踢）。
    tokio::time::sleep(Duration::from_millis(1_100)).await;
    stream
        .write_all(&ClientPacket::KeepAlive(c::KeepAlive { time: 1 }).encode_frame())
        .await
        .expect("窗口重置后再发一个");
    let mut got_keepalive = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline && !got_keepalive {
        if let Some((index, payload)) = frames.next_frame().expect("帧格式") {
            let packet = ServerPacket::decode(index, &payload).expect("解析服务端包");
            got_keepalive = matches!(packet, ServerPacket::KeepAlive(_));
            continue;
        }
        match tokio::time::timeout(Duration::from_millis(200), stream.read(&mut chunk)).await {
            Ok(Ok(0)) => panic!("连接被关闭"),
            Ok(Ok(read)) => frames.push(&chunk[..read]),
            Ok(Err(error)) => panic!("连接被关闭：{error}"),
            Err(_) => {}
        }
    }
    assert!(got_keepalive, "窗口重置后应恢复处理（连接未踢）");
}

#[test]
fn connection_limiter_enforces_both_limits() {
    let ip: IpAddr = "203.0.113.7".parse().expect("IP");
    let mut limiter = ConnectionLimiter::new(2, 3);
    assert_eq!(limiter.try_acquire(ip, 100), None);
    assert_eq!(limiter.try_acquire(ip, 100), None);
    assert_eq!(limiter.concurrent_for(ip), 2);
    // 并发到顶（这一秒还剩 1 个配额，但并发先挡）
    assert!(limiter.try_acquire(ip, 100).is_some());
    limiter.release(ip);
    limiter.release(ip);
    assert_eq!(limiter.concurrent_for(ip), 0);
    // 并发归零后还能再进（本秒已用 2 个配额，第 3 个仍放行）
    assert_eq!(limiter.try_acquire(ip, 100), None);
    // 本秒配额用满 → 再同秒新建连接就是 TooFast
    assert_eq!(
        limiter.try_acquire(ip, 100),
        Some(ConnReject::TooFast),
        "同一秒第 4 条应超每秒上限"
    );
    // 下一秒窗口重置
    assert_eq!(limiter.try_acquire(ip, 101), None);
    // 别的 IP 不受影响
    let other: IpAddr = "203.0.113.8".parse().expect("IP");
    assert_eq!(limiter.try_acquire(other, 101), None);
}

#[tokio::test]
async fn zero_thresholds_disable_the_limits() {
    // ---- 1) 纯逻辑：并发/每秒都是 0 = 不限制，同 IP 连开 11 条都不被拒、也不记账。
    let ip: IpAddr = "198.51.100.9".parse().expect("IP");
    let mut limiter = ConnectionLimiter::new(0, 0);
    for attempt in 1..=11 {
        assert_eq!(
            limiter.try_acquire(ip, 100),
            None,
            "阈值 0 时同 IP 第 {attempt} 条连接不应被限流"
        );
    }
    assert_eq!(limiter.concurrent_for(ip), 0, "阈值 0 时不应记录并发数");

    // ---- 2) e2e：11 条真连接每条都能拿到 `S.Connected`（限流全关）。
    let address = start(base_config()).await;
    let mut sockets = Vec::new();
    for attempt in 1..=11 {
        let (stream, mut frames) = connect(address).await;
        let (index, _payload) = frames
            .next_frame()
            .expect("帧格式")
            .expect("应收到 Connected");
        assert_eq!(
            index,
            s::Connected::INDEX,
            "阈值 0 时第 {attempt} 条同 IP 连接应被正常接受"
        );
        sockets.push(stream);
    }

    // ---- 3) 包速率 0 = 不限制：300 个 `KeepAlive` 应几乎全部回包、且不断线。
    let (mut stream, mut frames) = connect(address).await;
    stream
        .write_all(
            &ClientPacket::ClientVersion(c::ClientVersion {
                version_hash: Vec::new(),
            })
            .encode_frame(),
        )
        .await
        .expect("发送版本");
    for _ in 0..300 {
        stream
            .write_all(&ClientPacket::KeepAlive(c::KeepAlive { time: 0 }).encode_frame())
            .await
            .expect("发送 KeepAlive");
    }
    let mut replies = 0_usize;
    let mut chunk = [0u8; 8192];
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline && replies < 250 {
        while let Some((index, payload)) = frames.next_frame().expect("帧格式") {
            let packet = ServerPacket::decode(index, &payload).expect("解析服务端包");
            if matches!(packet, ServerPacket::KeepAlive(_)) {
                replies += 1;
            }
        }
        match tokio::time::timeout(Duration::from_millis(200), stream.read(&mut chunk)).await {
            Ok(Ok(0)) => panic!("包速率上限为 0 时不应断开连接"),
            Ok(Ok(read)) => frames.push(&chunk[..read]),
            Ok(Err(error)) => panic!("连接被关闭：{error}"),
            Err(_) => {}
        }
    }
    assert!(
        replies >= 250,
        "包速率上限 0（不限制）时 300 个 KeepAlive 应几乎全部回包，实际只有 {replies}"
    );

    drop(sockets);
}

#[tokio::test]
async fn garbage_and_oversized_frames_keep_server_alive() {
    let address = start(ServerConfig {
        max_packets_per_second: 200,
        ..base_config()
    })
    .await;
    let (mut stream, _frames) = connect(address).await;
    // 8 字节垃圾：声明 65535 但只给 4 字节 body → 服务端必须等更多数据，不能 panic/断开。
    stream.write_all(&[0xFF; 8]).await.expect("写垃圾帧");
    // 再发一个长度 65535 的完整（但内容全随机）超大帧：不能 panic。
    let mut oversized = vec![0u8; usize::from(u16::MAX)];
    oversized[0..2].copy_from_slice(&u16::MAX.to_le_bytes());
    oversized[2..4].copy_from_slice(&42i16.to_le_bytes());
    stream.write_all(&oversized).await.expect("写超大帧");
    tokio::time::sleep(Duration::from_millis(100)).await;
    // 服务端仍存活：新连接能收到 Connected（不要求登录成功）。
    let (_fresh, _fresh_frames) = connect(address).await;
    drop(stream);
}

#[tokio::test]
async fn packet_rate_limit_at_200_per_second_drops_excess_and_keepalive_responds() {
    let address = start(ServerConfig {
        max_packets_per_second: 200,
        ..base_config()
    })
    .await;
    let (mut stream, mut frames) = connect(address).await;
    stream
        .write_all(
            &ClientPacket::ClientVersion(c::ClientVersion {
                version_hash: Vec::new(),
            })
            .encode_frame(),
        )
        .await
        .expect("版本");
    for _ in 0..240 {
        stream
            .write_all(&ClientPacket::KeepAlive(c::KeepAlive { time: 0 }).encode_frame())
            .await
            .expect("KeepAlive");
    }
    let mut replies = 0usize;
    let deadline = tokio::time::Instant::now() + Duration::from_millis(800);
    let mut chunk = [0u8; 8192];
    while tokio::time::Instant::now() < deadline {
        while let Some((index, payload)) = frames.next_frame().expect("帧格式") {
            let _ = ServerPacket::decode(index, &payload).expect("解析");
            replies += 1;
        }
        match tokio::time::timeout(Duration::from_millis(100), stream.read(&mut chunk)).await {
            Ok(Ok(0)) => panic!("200/s 限流不应踢线"),
            Ok(Ok(read)) => frames.push(&chunk[..read]),
            Ok(Err(error)) => panic!("连接被关闭：{error}"),
            Err(_) => {}
        }
    }
    println!("rate-200 replies={replies}");
    assert!(
        replies <= 220,
        "超过 200/s 的 KeepAlive 应被丢弃，实际回包 {replies}"
    );
    // 窗口重置后 KeepAlive 仍响应，证明连接存活。
    tokio::time::sleep(Duration::from_millis(1_100)).await;
    stream
        .write_all(&ClientPacket::KeepAlive(c::KeepAlive { time: 1 }).encode_frame())
        .await
        .expect("窗口后 KeepAlive");
    let mut got = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline && !got {
        if let Some((index, payload)) = frames.next_frame().expect("帧格式") {
            got = matches!(
                ServerPacket::decode(index, &payload).expect("解析"),
                ServerPacket::KeepAlive(_)
            );
            continue;
        }
        match tokio::time::timeout(Duration::from_millis(200), stream.read(&mut chunk)).await {
            Ok(Ok(0)) => panic!("连接被关闭"),
            Ok(Ok(read)) => frames.push(&chunk[..read]),
            Ok(Err(error)) => panic!("连接被关闭：{error}"),
            Err(_) => {}
        }
    }
    assert!(got, "限流窗口重置后 KeepAlive 必须仍响应");
}
