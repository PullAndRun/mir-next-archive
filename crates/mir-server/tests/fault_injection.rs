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
