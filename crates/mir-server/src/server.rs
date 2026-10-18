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
    // 行会：启动时把库里的行会灌进世界（Crystal `Envir.LoadGuilds`，`Envir.cs:3089-3117`）。
    match store.list_guilds().await {
        Ok(guilds) => {
            let mut world = world.lock().await;
            let loaded = world.load_guilds(guilds);
            tracing::info!(count = loaded, "已载入行会");
        }
        Err(error) => tracing::warn!(%error, "读取行会失败：从空行会列表启动"),
    }
    // 攻城：先按配表建出攻城区（Crystal `Envir.LoadConquests`，`Envir.cs:3119-3170`），
    // 再用库里的运行期状态覆盖（城主/攻打方/税收/战争状态），最后刷出箭塔。
    {
        let mut world = world.lock().await;
        let built = world.init_conquests();
        if built > 0 {
            let indices: Vec<i32> = world
                .conquests()
                .iter()
                .map(|conquest| conquest.info.index)
                .collect();
            let archers: usize = indices
                .into_iter()
                .map(|index| world.spawn_conquest_archers(index).len())
                .sum();
            tracing::info!(count = built, archers, "已载入攻城区域");
        }
    }
    match store.list_conquests().await {
        Ok(records) => {
            let mut world = world.lock().await;
            let loaded = world.load_conquests(records);
            tracing::info!(count = loaded, "已载入攻城状态");
        }
        Err(error) => tracing::warn!(%error, "读取攻城状态失败：从空状态启动"),
    }
    // 市场：把库里的挂单灌进世界，先结算停机期间到期的拍卖（有有效出价 → 自动成交），
    // 再清理剩下的过期挂单（物品/最高出价按系统邮件退回）。
    // 读取失败直接中止启动——绝不能让运行期用「空挂单」的快照把库里的行覆盖掉。
    let auctions = store
        .load_auctions()
        .await
        .context("读取市场挂单失败，拒绝以空市场启动")?;
    let (auction_count, sold, purged, market_cleanup) = {
        let mut world = world.lock().await;
        let loaded = world.load_auctions(auctions);
        let now_ms = world.wall_now();
        // Crystal 的 `ProcessAuction` 在停机期间没跑过：重启后先按同一语义补结算，
        // 否则「到期+有出价」的挂单会被清理路径当成普通过期挂单退款退货。
        let sold = world.process_auctions(now_ms);
        let purged = world.purge_expired_auctions(now_ms);
        // F2：邮件（成交/退回）与「删除挂单」必须在同一事务提交；否则崩溃在两者之间
        // 会重复退款或重复发货。出锁后由 `apply_market_update` 用一个事务落库。
        let cleanup = if sold.is_empty() && purged.is_empty() {
            None
        } else {
            Some(crate::store::MarketUpdate {
                character: None,
                mail_writes: world.take_mail_writes(),
                auctions: world.auction_snapshot(),
            })
        };
        (loaded, sold, purged, cleanup)
    };
    if !sold.is_empty() {
        tracing::info!(
            count = sold.len(),
            "启动结算：到期的拍卖已自动成交（物品寄买家、成交价−佣金寄卖家）"
        );
    }
    if let Some(cleanup) = market_cleanup {
        store
            .apply_market_update(cleanup)
            .await
            .context("过期市场挂单清理（成交/退回邮件+删除挂单）事务失败")?;
    }
    tracing::info!(
        loaded = auction_count,
        sold = sold.len(),
        purged = purged.len(),
        "已载入市场挂单"
    );
    let languages = Arc::new(build_language_library(&config));
    // 语言包注入世界层：World 里的玩家可见文本（市场/交易/邮件/NPC）按玩家语言渲染，
    // 语言包是运行时覆盖层（缺失时 `World::text/format` 回落内置模板）。
    {
        let mut world = world.lock().await;
        world.set_languages(Arc::clone(&languages));
    }
    let next_session = Arc::new(AtomicU64::new(1));

    // 世界 tick：每 100ms 推进一次（跑步计数衰减、停顿清零；怪物 AI 之后也挂这里）。
    let tick_world = Arc::clone(&world);
    let tick_store = Arc::clone(&store);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // 拍卖到期结算的闸门：Crystal 每 10 分钟一次（`Envir.cs:2354-2358`），本作 tick 是
        // 100ms，按 [`crate::market::AUCTION_SCAN_INTERVAL_MS`] 节流，避免每 tick 全表扫描。
        // 状态放在 tick 任务里而不是 `World` 上：不给世界加只服务一次扫描的字段。
        let mut next_auction_scan = 0_i64;
        // 成交结果已在内存里（挂单删除 + 邮件入队），落盘失败时必须继续重试，直到事务提交。
        let mut sale_flush_pending = false;
        loop {
            interval.tick().await;
            let (writes, conquest_writes, auction_sales) = {
                let mut world = tick_world.lock().await;
                world.tick();
                // 行会经验来自怪物击杀（tick 里结算），写入由这里定期落盘。
                // 攻城状态（开战/停战/城主/税收）同样在 tick 里变化。
                let now_ms = world.wall_now();
                let auction_sales = if now_ms >= next_auction_scan {
                    next_auction_scan =
                        now_ms.saturating_add(crate::market::AUCTION_SCAN_INTERVAL_MS);
                    let sales = world.process_auctions(now_ms);
                    if !sales.is_empty() {
                        sale_flush_pending = true;
                    }
                    sales
                } else {
                    Vec::new()
                };
                (
                    world.take_guild_writes(),
                    world.take_conquest_writes(),
                    auction_sales,
                )
            };
            if !writes.is_empty()
                && let Err(error) =
                    crate::guild::persist_guild_writes(tick_store.as_ref(), writes).await
            {
                tracing::warn!(%error, "tick 里的行会落盘失败");
            }
            if !conquest_writes.is_empty()
                && let Err(error) =
                    crate::conquest::persist_conquest_writes(tick_store.as_ref(), conquest_writes)
                        .await
            {
                tracing::warn!(%error, "tick 里的攻城状态落盘失败");
            }
            if sale_flush_pending {
                let count = auction_sales.len();
                match crate::market::flush_auction_sales(tick_store.as_ref(), tick_world.as_ref())
                    .await
                {
                    Ok(()) => sale_flush_pending = false,
                    Err(error) => {
                        // 事务一行未写、内存里的成交结果保留（见 `flush_auction_sales` 文档）：
                        // 5 秒后重试落盘，而不是等满一个扫描周期。
                        next_auction_scan =
                            tick_world.lock().await.wall_now().saturating_add(5_000);
                        tracing::warn!(%error, count, "tick 里的拍卖成交落盘失败，稍后重试");
                    }
                }
            }
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

/// 起世界：地图按需从 `{map_root}/{start_map}.map` 加载（`World::load_map`），
/// 出生地图在这里先初始化一次（刷 NPC + 注册刷怪点）。
fn build_world(config: &ServerConfig) -> World {
    let mut world = World::new(999);
    // 多地图：地图目录与缓存上限。Crystal `Settings.MapPath`，`Server/Settings.cs`。
    world.set_map_root(&config.map_root);
    // 等级经验表：`config/ExpList.ini` → 素材索引里的 `Configs/ExpList.ini` → 近似曲线。
    // `World` 的默认表是兜底曲线（保证测试确定性），生产路径必须显式换成真表。
    world.set_exp_table(crate::exp::ExpTable::discover());
    // 行会配置：`config/GuildSettings.ini` → 素材里的 `Configs/GuildSettings.ini` → 代码默认值。
    // 与经验表同理：测试用默认值保证确定性，生产路径显式 discover。
    world.set_guild_settings(crate::guild::GuildSettings::discover());
    attach_game_data(&mut world, config);
    if config.demo_monsters {
        spawn_demo_monsters(&mut world, config);
    }
    world
}

/// 把 `Server.MirDB` 挂到世界上，并初始化出生地图（Crystal 建图时一次做完的事：
/// `Map.Load` 读地形 + `MapRespawn` 注册 + `NPCObject` 刷出，`Server/MirEnvir/Map.cs:432-514`）。
///
/// 优先级：`config.db_path`（或 `MIR2_DB_PATH`）→ 索引里的候选路径。找不到数据库时
/// 只保留 [`spawn_demo_monsters`] 的演示怪，服务器照常可跑。
///
/// `GameData` 还要给物品模板查找与怪物掉落用（对应 Crystal 的 `Envir.ItemInfoList` /
/// `MonsterInfo.Drops`，`Envir.cs:5405-5419`）。
fn attach_game_data(world: &mut World, config: &ServerConfig) {
    if config.load_game_data {
        if let Some(data) = load_game_data(config) {
            let path = data.path().display().to_string();
            let monsters = data.monster_count();
            let items = data.item_count();
            let drop_dir = data
                .drop_dir()
                .map(|dir| dir.display().to_string())
                .unwrap_or_default();
            world.set_game_data(Arc::new(data));
            tracing::info!(
                path = %path,
                monsters,
                items,
                drop_dir = %drop_dir,
                "已加载 Server.MirDB"
            );
        }
    }
    // 出生地图：地形 + NPC + 刷怪点（`World::initialize_map`）。
    let report = world.initialize_map(&config.start_map);
    if report.initialized {
        tracing::info!(
            map = %config.start_map,
            npcs = report.npcs,
            groups = report.respawn_groups,
            spawned = report.spawned,
            "出生地图已初始化"
        );
    } else {
        tracing::warn!(
            map = %config.start_map,
            root = %config.map_root,
            "出生地图地形加载失败：使用无地形的兜底世界（只校验边界与占格）"
        );
    }
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
