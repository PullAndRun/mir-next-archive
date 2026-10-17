//! TCP 监听与连接循环。

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Context;
use mir_core::i18n::LanguageLibrary;
use mir_protocol::ClientPacket;
use mir_protocol::codec::FrameBuffer;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, mpsc};

use crate::config::ServerConfig;
use crate::session::Session;
use crate::store::AccountStore;
use crate::world::World;

/// 启动服务端，永久运行。
///
/// # Errors
/// 监听失败（端口被占用、无权限等）时返回错误。
pub async fn run<S: AccountStore>(config: Arc<ServerConfig>, store: Arc<S>) -> anyhow::Result<()> {
    let listener = TcpListener::bind(config.listen)
        .await
        .with_context(|| format!("监听 {} 失败", config.listen))?;
    tracing::info!(address = %config.listen, "传奇-Next 网关已启动");
    serve_forever(listener, config, store).await
}

/// 在已经绑定的监听器上跑服务端（测试用：可以先绑 `127.0.0.1:0` 再取端口）。
///
/// # Errors
/// accept 出现不可恢复错误时返回错误；普通连接错误只记日志并继续。
pub async fn serve_forever<S: AccountStore>(
    listener: TcpListener,
    config: Arc<ServerConfig>,
    store: Arc<S>,
) -> anyhow::Result<()> {
    let world = Arc::new(Mutex::new(build_world(&config)));
    let languages = Arc::new(build_language_library(&config));
    let next_session = Arc::new(AtomicU64::new(1));

    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(error) => {
                tracing::warn!(%error, "accept 失败");
                continue;
            }
        };
        let id = next_session.fetch_add(1, Ordering::Relaxed);
        let config = Arc::clone(&config);
        let store = Arc::clone(&store);
        let world = Arc::clone(&world);
        let languages = Arc::clone(&languages);
        tokio::spawn(async move {
            if let Err(error) = serve(
                stream,
                peer.to_string(),
                id,
                config,
                store,
                world,
                languages,
            )
            .await
            {
                tracing::warn!(session = id, %peer, %error, "连接结束");
            }
        });
    }
}

/// 加载语言包：目录不存在或解析失败时退回内置语言包（默认中文仍然可用）。
fn build_language_library(config: &ServerConfig) -> LanguageLibrary {
    match LanguageLibrary::load_dir(std::path::Path::new(&config.language_dir)) {
        Ok(library) => {
            tracing::info!(
                dir = %config.language_dir,
                languages = library.languages().len(),
                "已加载语言包"
            );
            library
        }
        Err(error) => {
            tracing::warn!(dir = %config.language_dir, %error, "语言包加载失败，使用内置语言包");
            LanguageLibrary::embedded()
        }
    }
}

/// 起世界：能读到 `{map_root}/{start_map}.map` 就用真实地形，否则退化成无边界的测试网格。
fn build_world(config: &ServerConfig) -> World {
    let path = std::path::Path::new(&config.map_root).join(format!("{}.map", config.start_map));
    match mir_world::Map::load(&path) {
        Ok(map) => {
            tracing::info!(
                map = %path.display(),
                width = map.width,
                height = map.height,
                "已加载地图"
            );
            World::with_map(Arc::new(map))
        }
        Err(error) => {
            tracing::warn!(
                map = %path.display(),
                %error,
                "地图加载失败，使用无地形的兜底世界（只校验边界与占格）"
            );
            World::default()
        }
    }
}

async fn serve<S: AccountStore>(
    stream: TcpStream,
    peer: String,
    id: u64,
    config: Arc<ServerConfig>,
    store: Arc<S>,
    world: Arc<Mutex<World>>,
    languages: Arc<LanguageLibrary>,
) -> anyhow::Result<()> {
    stream.set_nodelay(true).ok();
    let (mut reader, mut writer) = stream.into_split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Vec<u8>>();

    // 写任务：所有发包（含世界广播）都走同一条通道，保证帧不交错。
    let writer_task = tokio::spawn(async move {
        while let Some(frame) = out_rx.recv().await {
            if writer.write_all(&frame).await.is_err() {
                break;
            }
        }
        let _ = writer.shutdown().await;
    });

    tracing::info!(session = id, %peer, "接受连接");
    let mut session = Session::new(
        id,
        peer.clone(),
        Arc::clone(&config),
        store,
        Arc::clone(&world),
        out_tx.clone(),
        languages,
    );
    session.on_connected();

    let mut frames = FrameBuffer::new();
    let mut chunk = [0u8; 8192];
    'read: loop {
        let read = match reader.read(&mut chunk).await {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) => {
                tracing::debug!(session = id, %error, "读取失败");
                break;
            }
        };
        frames.push(&chunk[..read]);
        loop {
            let frame = match frames.next_frame() {
                Ok(Some(frame)) => frame,
                Ok(None) => break,
                Err(error) => {
                    tracing::warn!(session = id, %error, "帧格式非法，断开连接");
                    break 'read;
                }
            };
            let (index, payload) = frame;
            let packet = match ClientPacket::decode(index, &payload) {
                Ok(packet) => packet,
                Err(error) => {
                    // Crystal 对未知编号是「清空接收缓冲」，这里直接断开更安全。
                    tracing::warn!(session = id, index, %error, "无法解析的客户端包，断开连接");
                    break 'read;
                }
            };
            if let Err(error) = session.handle(packet).await {
                tracing::warn!(session = id, %error, "处理客户端包失败");
                break 'read;
            }
        }
    }

    session.on_disconnected().await;
    drop(session);
    drop(out_tx);
    let _ = writer_task.await;
    tracing::info!(session = id, %peer, "连接关闭");
    Ok(())
}
