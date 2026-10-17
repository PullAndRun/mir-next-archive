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

    // 世界 tick：每 100ms 推进一次（跑步计数衰减、停顿清零；怪物 AI 之后也挂这里）。
    let tick_world = Arc::clone(&world);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            tick_world.lock().await.tick();
        }
    });

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
    let mut world = match mir_world::Map::load(&path) {
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
    };
    // 等级经验表：`config/ExpList.ini` → 素材索引里的 `Configs/ExpList.ini` → 近似曲线。
    // `World` 的默认表是兜底曲线（保证测试确定性），生产路径必须显式换成真表。
    world.set_exp_table(crate::exp::ExpTable::discover());
    if config.demo_monsters {
        spawn_demo_monsters(&mut world, config);
    }
    spawn_from_database(&mut world, config);
    world
}

/// 用 `Server.MirDB` 的刷怪点填充世界（Crystal 的 `MapRespawn` 数据来源）。
///
/// 优先级：`config.db_path`（或 `MIR2_DB_PATH`）→ 索引里的候选路径。找不到数据库时
/// 只保留 [`spawn_demo_monsters`] 的演示怪，服务器照常可跑。
///
/// 顺便把 `GameData` 挂到世界上（`World::set_game_data`）：物品模板查找与怪物掉落都要用它，
/// 对应 Crystal 的 `Envir.ItemInfoList` / `MonsterInfo.Drops`（`Envir.cs:5405-5419`）。
fn spawn_from_database(world: &mut World, config: &ServerConfig) {
    if !config.load_game_data {
        return;
    }
    let Some(data) = load_game_data(config) else {
        return;
    };
    let groups = data.spawn_groups(&config.start_map);
    let path = data.path().display().to_string();
    let monsters = data.monster_count();
    let items = data.item_count();
    let drop_dir = data
        .drop_dir()
        .map(|dir| dir.display().to_string())
        .unwrap_or_default();
    world.set_game_data(Arc::new(data));
    // NPC：Crystal 在建图时把 `MapIndex` 匹配的 `NPCInfo` 全刷出来
    // （`Map.cs:490-496`），本作世界只加载一张图，所以在同一个地方一次性刷出。
    let npcs = world.spawn_npcs(&config.start_map);
    tracing::info!(
        map = %config.start_map,
        npcs = npcs.len(),
        "已按数据库刷出 NPC"
    );
    if groups.is_empty() {
        tracing::warn!(
            map = %config.start_map,
            path = %path,
            "数据库里这张地图没有刷怪点（或怪物模板缺失）"
        );
        return;
    }
    let group_count = groups.len();
    let mut spawned = 0;
    for group in &groups {
        spawned += world.spawn_group(group).len();
    }
    tracing::info!(
        path = %path,
        monsters,
        items,
        drop_dir = %drop_dir,
        map = %config.start_map,
        groups = group_count,
        spawned,
        "已按数据库刷怪点刷新"
    );
}

fn load_game_data(config: &ServerConfig) -> Option<crate::gamedata::GameData> {
    if let Some(path) = &config.db_path {
        match crate::gamedata::GameData::load(path) {
            Ok(data) => return Some(data),
            Err(error) => {
                tracing::warn!(path, %error, "MIR2_DB_PATH 指定的数据库读取失败");
                return None;
            }
        }
    }
    match crate::gamedata::GameData::discover() {
        Ok(data) => Some(data),
        Err(error) => {
            tracing::info!(%error, "没有可用的 Server.MirDB，跳过数据库刷怪");
            None
        }
    }
}

/// 演示用刷怪：在出生点附近撒几只示例怪物。
///
/// 简化：Crystal 的刷怪节奏由 `MapRespawn`/`RespawnTimer` 驱动（见 `docs/world/tick-loop.md`
/// §6），数据来自数据库；`mir-db` 接入前用这个开关让「刷怪 → 索敌 → 追 → 打 → 死」在真实
/// 服务器上能看到。怪物数值是**占位值**（[`MonsterKind::scarecrow`] / [`MonsterKind::skeleton`]）。
/// 刷新节奏给 1~2 分钟，方便手动验证「打死 → 到点补一只」。
fn spawn_demo_monsters(world: &mut World, config: &ServerConfig) {
    let center = mir_core::Point::new(config.start_point.0, config.start_point.1);
    let groups = [
        (crate::monster::MonsterKind::scarecrow(), 4, 6),
        (crate::monster::MonsterKind::skeleton(), 2, 9),
    ];
    for (kind, count, spread) in groups {
        let spawned = world.register_spawn_group(
            crate::world::SpawnGroup::new(config.start_map.clone(), kind, center, count, spread)
                .with_delay(1, 0),
        );
        tracing::info!(count = spawned.len(), "演示怪物已刷新");
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
