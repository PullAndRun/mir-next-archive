//! TCP 监听与连接循环。

use std::collections::HashMap;
use std::net::IpAddr;
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

/// 汇总日志的间隔（30 秒）。
const METRICS_LOG_INTERVAL_MS: i64 = 30_000;
/// 单连接写任务的合批上限：一次 `write_all` 最多发送这么多字节。
const OUTBOX_BATCH_BYTES: usize = 64 * 1024;

/// 连接被拒的原因（只用于日志/测试断言）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnReject {
    /// 同一个 IP 的并发连接数已达上限。
    TooManyConcurrent,
    /// 同一个 IP 这一秒新建的连接数已达上限。
    TooFast,
}

/// 每 IP 的连接限流器（抗刷）：并发连接数 + 每秒新建连接数。
///
/// Crystal 的 `Server.cs` 只做 `AcceptSocket`，没有这一层；本作加上它是为了挡掉端口扫描
/// 与「一个脚本开几百条连接」的刷号行为。阈值来自 [`ServerConfig`]（`0` = 不限制）。
#[derive(Debug)]
pub struct ConnectionLimiter {
    max_per_ip: usize,
    max_per_second: usize,
    concurrent: HashMap<IpAddr, usize>,
    /// (Unix 秒, 该秒内新建的连接数)
    window: HashMap<IpAddr, (u64, usize)>,
}

impl ConnectionLimiter {
    /// 用配置里的两个上限建一个限流器。
    #[must_use]
    pub fn new(max_per_ip: usize, max_per_second: usize) -> Self {
        Self {
            max_per_ip,
            max_per_second,
            concurrent: HashMap::new(),
            window: HashMap::new(),
        }
    }

    /// 尝试占用一个连接槽；返回 `None` = 放行，`Some(reason)` = 拒绝。
    ///
    /// `second` 是当前的 Unix 秒（测试里显式传入，生产用 [`crate::gm::unix_now_secs`]）。
    pub fn try_acquire(&mut self, ip: IpAddr, second: u64) -> Option<ConnReject> {
        // 先判定、后记账：被拒的连接不占用配额（否则手滑重连几次就把自己锁死一整秒）。
        if self.max_per_second != 0
            && let Some(&(window, count)) = self.window.get(&ip)
            && window == second
            && count >= self.max_per_second
        {
            return Some(ConnReject::TooFast);
        }
        if self.max_per_ip != 0 && self.concurrent_for(ip) >= self.max_per_ip {
            return Some(ConnReject::TooManyConcurrent);
        }
        if self.max_per_second != 0 {
            let entry = self.window.entry(ip).or_insert((second, 0));
            if entry.0 == second {
                entry.1 = entry.1.saturating_add(1);
            } else {
                *entry = (second, 1);
            }
        }
        if self.max_per_ip != 0 {
            let count = self.concurrent.entry(ip).or_insert(0);
            *count = count.saturating_add(1);
        }
        None
    }

    /// 连接结束时释放并发槽（每秒窗口不需要释放）。
    pub fn release(&mut self, ip: IpAddr) {
        if let Some(count) = self.concurrent.get_mut(&ip) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.concurrent.remove(&ip);
            }
        }
    }

    /// 某个 IP 现在的并发连接数（测试/诊断用）。
    #[must_use]
    pub fn concurrent_for(&self, ip: IpAddr) -> usize {
        self.concurrent.get(&ip).copied().unwrap_or(0)
    }
}

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
    serve_forever_with_world(listener, config, store, world).await
}

/// [`serve_forever`] 的可注入世界版本：**测试**用它拿世界句柄（`World::advance_clock`
/// 快进世界时钟来驱动 autosave / 拍卖扫描，不用 sleep 等真实时间）。
///
/// 生产路径不调用它；除了「世界由调用方构造」之外，行为与 [`serve_forever`] 完全一致
/// （启动装载行会/攻城/拍卖、100ms tick、accept 循环都在这里）。
pub async fn serve_forever_with_world<S: AccountStore>(
    listener: TcpListener,
    config: Arc<ServerConfig>,
    store: Arc<S>,
    world: Arc<Mutex<World>>,
) -> anyhow::Result<()> {
    world.lock().await.set_broadcast_batching(true);
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
        .context("failed to load market listings; refusing to start with an empty market")?;
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
        store.apply_market_update(cleanup).await.context(
            "expired auction cleanup transaction failed (sale/return mail + listing delete)",
        )?;
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
        // 重试也有闸门（[`crate::market::SALE_FLUSH_RETRY_MS`]）：DB 故障时不能按 100ms tick
        // 的频率捶库，第一次尝试是即时的（初值 0），失败后按 5 秒退避。
        let mut sale_flush_pending = false;
        let mut next_sale_flush_at = 0_i64;
        // 指标：每 30 秒一条汇总日志（`metrics summary` 便于 grep）。
        let mut next_metrics_log_at = 0_i64;
        loop {
            interval.tick().await;
            let tick_started = std::time::Instant::now();
            let (now_ms, writes, conquest_writes) = {
                let mut world = tick_world.lock().await;
                world.tick();
                // 行会经验来自怪物击杀（tick 里结算），写入由这里定期落盘。
                // 攻城状态（开战/停战/城主/税收）同样在 tick 里变化。
                let now_ms = world.wall_now();
                if now_ms >= next_auction_scan {
                    next_auction_scan =
                        now_ms.saturating_add(crate::market::AUCTION_SCAN_INTERVAL_MS);
                    // 成交结果留在内存里（挂单删除 + 邮件入队），由下面的 flush 分支落盘。
                    let sold = world.process_auctions(now_ms).len();
                    crate::metrics::metrics()
                        .record_auction_scan(u64::try_from(sold).unwrap_or(u64::MAX));
                    if sold > 0 {
                        sale_flush_pending = true;
                    }
                }
                // `@reboot` 倒计时：到点时先全服公告再踢掉所有会话，accept 循环随后退出。
                if let Some(remaining) = crate::gm::poll_reboot(now_ms / 1000) {
                    let message =
                        crate::gm::reboot_message(u32::try_from(remaining).unwrap_or_default());
                    world.broadcast_system(&message);
                    let _ = remaining;
                    tracing::info!(remaining, "重启倒计时：读循环收到通知后将强制存盘再退出");
                }
                crate::metrics::metrics().record_tick(tick_started.elapsed());
                crate::metrics::metrics().set_mail_queue(world.pending_mail_writes());
                (
                    now_ms,
                    world.take_guild_writes(),
                    world.take_conquest_writes(),
                )
            };
            if now_ms >= next_metrics_log_at {
                next_metrics_log_at = now_ms.saturating_add(METRICS_LOG_INTERVAL_MS);
                let per_map = {
                    let world = tick_world.lock().await;
                    world.player_counts_by_map().len()
                };
                tracing::info!(
                    metrics = %crate::metrics::metrics().summary_line(per_map),
                    "metrics summary"
                );
            }
            if !writes.is_empty() {
                let count = writes.len();
                if let Err(error) =
                    crate::guild::persist_guild_writes(tick_store.as_ref(), writes.clone()).await
                {
                    // F3：批次已从世界取走，失败必须回灌，否则行会变更永久丢失。
                    {
                        let mut world = tick_world.lock().await;
                        let mut retry = writes;
                        retry.append(&mut world.guild_writes);
                        world.guild_writes = retry;
                    }
                    crate::metrics::metrics().record_batch_write_failed();
                    tracing::warn!(count, %error, "tick 里的行会落盘失败：批次已回灌队列");
                }
            }
            if !conquest_writes.is_empty()
                && let Err(error) =
                    crate::conquest::persist_conquest_writes(tick_store.as_ref(), conquest_writes)
                        .await
            {
                tracing::warn!(%error, "tick 里的攻城状态落盘失败");
            }
            if sale_flush_pending && now_ms >= next_sale_flush_at {
                match crate::market::flush_auction_sales(tick_store.as_ref(), tick_world.as_ref())
                    .await
                {
                    Ok(()) => {
                        sale_flush_pending = false;
                        next_sale_flush_at = 0;
                    }
                    Err(error) => {
                        // 事务一行未写、内存里的成交结果保留（见 `flush_auction_sales` 文档）：
                        // 按 [`crate::market::SALE_FLUSH_RETRY_MS`] 退避重试，不按 tick 频率捶库。
                        next_sale_flush_at =
                            now_ms.saturating_add(crate::market::SALE_FLUSH_RETRY_MS);
                        tracing::warn!(%error, "tick 里的拍卖成交落盘失败，稍后重试");
                    }
                }
            }
        }
    });

    // 可观测性：配了 `MIR2_METRICS_LISTEN` 才起 `/metrics`（默认关闭）。
    if let Some(listen) = config.metrics_listen {
        let pool_probe: crate::metrics::PoolProbe = {
            let store = Arc::clone(&store);
            Arc::new(move || store.pool_stats().map(|stats| (stats.size, stats.idle)))
        };
        match crate::metrics::bind_metrics(listen, Arc::clone(&world), pool_probe).await {
            Ok(address) => tracing::info!(%address, "metrics 端点已启动（GET /metrics）"),
            Err(error) => tracing::warn!(%listen, %error, "metrics 端点启动失败，继续无指标运行"),
        }
    }

    // 抗刷：每 IP 并发 + 每秒新建连接数，超限直接关连接并计数。
    let limiter = Arc::new(std::sync::Mutex::new(ConnectionLimiter::new(
        config.max_connections_per_ip,
        config.max_connections_per_second,
    )));
    let mut rejected = 0_u64;
    loop {
        let accepted = tokio::select! {
            accepted = listener.accept() => accepted,
            () = crate::gm::wait_for_reboot() => {
                tracing::info!("GM @reboot 到点：停止 accept，服务端退出");
                break;
            }
        };
        let (stream, peer) = match accepted {
            Ok(accepted) => accepted,
            Err(error) => {
                tracing::warn!(%error, "accept 失败");
                continue;
            }
        };
        let ip = peer.ip();
        let verdict = limiter
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .try_acquire(ip, u64::try_from(crate::gm::unix_now_secs()).unwrap_or(0));
        if let Some(reason) = verdict {
            rejected += 1;
            crate::metrics::metrics().connection_rejected();
            tracing::warn!(%ip, ?reason, rejected, "连接被限流拒绝");
            drop(stream);
            continue;
        }
        crate::metrics::metrics().connection_opened();
        let id = next_session.fetch_add(1, Ordering::Relaxed);
        let config = Arc::clone(&config);
        let store = Arc::clone(&store);
        let world = Arc::clone(&world);
        let languages = Arc::clone(&languages);
        let session_limiter = Arc::clone(&limiter);
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
            session_limiter
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .release(ip);
            crate::metrics::metrics().connection_closed();
        });
    }
    tracing::info!(rejected, "服务端已停止接受新连接");
    Ok(())
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

/// 心跳超时的等待器：`secs == 0` 表示不超时（永远 pending）。
///
/// 放在 `select!` 里是 cancel-safe 的：每位客户端的「无包时长」按每帧重新计时。
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
    // 关键优化：一次最多把 64KB 的帧合并到同一个 `write_all`，把每 tick 每玩家
    // 的多次 syscall 降到一次；超过阈值先写掉，避免单连接无限占内存。
    let writer_task = tokio::spawn(async move {
        while let Some(first) = out_rx.recv().await {
            let mut batch = first;
            while batch.len() < OUTBOX_BATCH_BYTES {
                match out_rx.try_recv() {
                    Ok(frame) => batch.extend_from_slice(&frame),
                    Err(_) => break,
                }
            }
            if writer.write_all(&batch).await.is_err() {
                crate::metrics::metrics().write_failed();
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
    // GM `@kick` 通道：`gm.rs` 通过登记表找到会话后发一个信号，读循环立刻断开
    // （Crystal `Connection.Disconnect()` 的等价物）。
    let (kick_tx, mut kick_rx) = mpsc::unbounded_channel::<()>();
    crate::gm::register_session(id, kick_tx);

    let mut frames = FrameBuffer::new();
    let mut chunk = [0u8; 8192];
    // 心跳截止时刻：每收到**任何**客户端数据就往后推（Crystal 的 `LastPacket` 语义）。
    // 不能每轮 `sleep(...)` 重建——autosave 分支每 250ms 醒一次，会被无限推后。
    let idle_timeout = std::time::Duration::from_secs(config.idle_timeout_secs);
    let mut idle_deadline = tokio::time::Instant::now() + idle_timeout;
    'read: loop {
        let read = tokio::select! {
            read = reader.read(&mut chunk) => match read {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) => {
                    tracing::debug!(session = id, %error, "读取失败");
                    break;
                }
            },
            Some(()) = kick_rx.recv() => {
                tracing::info!(session = id, %peer, "GM @kick：断开连接");
                break 'read;
            },
            // 心跳超时（抗刷）：`MIR2_IDLE_TIMEOUT_SECS` 秒内没收到**任何**客户端包就断开。
            // Crystal 客户端每分钟发一次 `C.KeepAlive`，所以「有包」=「客户端还活着」。
            () = tokio::time::sleep_until(idle_deadline), if config.idle_timeout_secs > 0 => {
                tracing::info!(
                    session = id,
                    %peer,
                    secs = config.idle_timeout_secs,
                    "心跳超时：断开空闲连接"
                );
                break 'read;
            },
            // 服务端重启：`@reboot` 到点时也把读循环收掉（踢线通道之外的第二条路径）。
            () = crate::gm::wait_for_reboot() => {
                tracing::info!(session = id, %peer, "服务端重启：强制存盘后断开连接");
                session.force_save_for_shutdown().await;
                break 'read;
            },
            // 周期自动存档（F1）：每 `AUTOSAVE_POLL_MS` 看一次「到点没」。
            // 周期本身由 `MIR2_AUTOSAVE_SECS` 决定，判定用世界时钟（测试可快进）。
            // `session` 此刻不在别的 `select!` 分支里，借用是安全的。
            () = tokio::time::sleep(std::time::Duration::from_millis(
                crate::session::AUTOSAVE_POLL_MS,
            )) => {
                if config.autosave_secs > 0 {
                    session.autosave_tick().await;
                }
                // 本分支不产生「读到的字节数」，用 continue 回到 select（帧缓冲保留）。
                continue 'read;
            }
        };
        // 收到了数据 → 心跳计时重新开始。
        if config.idle_timeout_secs > 0 {
            idle_deadline = tokio::time::Instant::now() + idle_timeout;
        }
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

    crate::gm::unregister_session(id);
    session.on_disconnected().await;
    drop(session);
    drop(out_tx);
    let _ = writer_task.await;
    tracing::info!(session = id, %peer, "连接关闭");
    Ok(())
}
