//! 服务端可观测性指标 + 极简 `/metrics` HTTP 端点。
//!
//! 设计约束（t51）：
//!
//! * **采集不能拖慢 tick**：全部计数用无锁原子，耗时用固定大小的环形缓冲
//!   （[`Ring`]，`Vec<AtomicU32>` + 原子写游标），`record_*` 里**不分配、不加锁**；
//!   `/metrics` 请求来时再做一次快照（这时分配无所谓）。
//! * **不引 HTTP 框架**：手写一个只认 `GET /metrics` 的极小服务（[`serve_http`]），
//!   其它路径回 404，读请求最多 8 KB 就断开，避免被拿来当通用文件服务器。
//! * **默认关闭**：只有配了 [`crate::config::ServerConfig::metrics_listen`]
//!   （`MIR2_METRICS_LISTEN`）才监听。
//!
//! 输出是 Prometheus 文本格式（`# TYPE` + `name value`），字段表见模块末尾
//! [`Metrics::render`]。

use std::fmt::Write as _;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use crate::world::World;

/// 环形缓冲的槽数（512 个样本：100ms tick 约覆盖 51 秒）。
pub const RING_CAPACITY: usize = 512;

/// 固定容量环形缓冲：`Vec<AtomicU32>` + 原子写游标。
///
/// 只用来存「最近 N 次的耗时（微秒）」，容量固定 → 写入永不分配；
/// 读快照时把 `len` 个槽复制出来再排序算分位。
#[derive(Debug)]
pub struct Ring {
    slots: Vec<AtomicU32>,
    /// 写游标（也等于「已写入的样本数」，超过容量后按容量取模）。
    next: AtomicU32,
    /// 已写入样本总数（用于算「够不够 `len` 个」与速率）。
    total: AtomicU64,
}

impl Ring {
    /// 建一个容量 [`RING_CAPACITY`] 的环。
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(RING_CAPACITY)
    }

    /// 建一个指定容量的环（测试用）。
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        let mut slots = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            slots.push(AtomicU32::new(0));
        }
        Self {
            slots,
            next: AtomicU32::new(0),
            total: AtomicU64::new(0),
        }
    }

    /// 记一个样本（微秒）。写入路径没有分配、没有锁。
    pub fn push(&self, value: u32) {
        let index = self.next.fetch_add(1, Ordering::Relaxed) as usize % self.slots.len();
        if let Some(slot) = self.slots.get(index) {
            slot.store(value, Ordering::Relaxed);
        }
        self.total.fetch_add(1, Ordering::Relaxed);
    }

    /// 已写入的样本总数。
    #[must_use]
    pub fn total(&self) -> u64 {
        self.total.load(Ordering::Relaxed)
    }

    /// 最近 `min(total, capacity)` 个样本的快照（升序排列）。
    #[must_use]
    pub fn snapshot(&self) -> Vec<u32> {
        let usable = usize::try_from(self.total())
            .unwrap_or(usize::MAX)
            .min(self.slots.len());
        let mut values: Vec<u32> = self
            .slots
            .iter()
            .take(usable)
            .map(|slot| slot.load(Ordering::Relaxed))
            .collect();
        values.sort_unstable();
        values
    }

    /// 分位值（`quantile` 取 0.0-1.0）；没有样本时返回 `None`。
    #[must_use]
    pub fn quantile(&self, quantile: f64) -> Option<u32> {
        let values = self.snapshot();
        if values.is_empty() {
            return None;
        }
        let last = values.len() - 1;
        let last_f = f64::from(u32::try_from(last).unwrap_or(u32::MAX));
        let position = (quantile.clamp(0.0, 1.0) * last_f).round();
        let position = usize::try_from(position as u32).unwrap_or(last);
        values.get(position.min(last)).copied()
    }

    /// 平均值（微秒，四舍五入）；没有样本时返回 `None`。
    #[must_use]
    pub fn average(&self) -> Option<u32> {
        let values = self.snapshot();
        if values.is_empty() {
            return None;
        }
        let sum: u64 = values.iter().map(|value| u64::from(*value)).sum();
        u32::try_from(sum / values.len() as u64).ok()
    }

    /// 最大值（微秒）；没有样本时返回 `None`。
    #[must_use]
    pub fn max(&self) -> Option<u32> {
        self.snapshot().last().copied()
    }
}

impl Default for Ring {
    fn default() -> Self {
        Self::new()
    }
}

/// 服务端指标全集。
///
/// 所有字段都是原子量：`record_*` 可以从任何任务里随时调，不会被 tick 或会话阻塞。
#[derive(Debug, Default)]
pub struct Metrics {
    /// 当前连接数（`serve` 进出各加减一次）。
    connections_active: AtomicU64,
    /// 累计接受过的连接数。
    connections_total: AtomicU64,
    /// 被限流拒绝的连接数（t44 的 `ConnectionLimiter`）。
    connections_rejected: AtomicU64,
    /// 当前在线玩家数（进世界 +1、离世界 -1）。
    players_online: AtomicU64,
    /// 累计 tick 次数。
    ticks_total: AtomicU64,
    /// tick 耗时（微秒，最近 [`RING_CAPACITY`] 次）。
    tick_micros: Ring,
    /// 最近一次 tick 耗时（微秒）。
    last_tick_micros: AtomicU32,
    /// 累计拍卖扫描次数。
    auction_scans: AtomicU64,
    /// 累计拍卖成交数。
    auctions_sold: AtomicU64,
    /// 邮件队列长度（待写库的邮件数，每个 tick 更新）。
    mail_queue: AtomicU64,
    /// 累计 `save_character` 次数。
    db_saves_total: AtomicU64,
    /// 角色落盘**失败**次数（含 autosave 与下线存盘）。
    ///
    /// 与 `db_saves_total` 分开记：失败不增加成功计数，Prometheus 上
    /// `rate(mir2_db_save_failed_total[5m])` 直接就是「写库现在坏没坏」。
    db_save_failed_total: AtomicU64,
    /// autosave 实际写库次数（含失败），用于区分「周期存档在工作」与「只是下线存盘」。
    autosave_total: AtomicU64,
    /// 行会/邮件**批次**写库失败次数（失败即回灌队列重试，见 `session::flush_*_writes`）。
    batch_write_failed_total: AtomicU64,
    /// `save_character` 耗时（微秒，最近 N 次）。
    db_save_micros: Ring,
    /// 累计 `flush_market_update` / `flush_gameshop_update` 次数。
    db_flushes_total: AtomicU64,
    /// 市场落盘耗时（微秒，最近 N 次）。
    db_flush_micros: Ring,
    /// 因包速率超限/背压被丢弃的客户端包数。
    packets_dropped: AtomicU64,
    /// 连接 writer 写失败/断开的次数（与限流丢包分开统计）。
    write_failed: AtomicU64,
}

/// 全局指标（进程内唯一；测试也可以自己 [`Metrics::new`] 造一个）。
fn global() -> &'static Metrics {
    static GLOBAL: OnceLock<Metrics> = OnceLock::new();
    GLOBAL.get_or_init(Metrics::new)
}

/// 取全局指标。
#[must_use]
pub fn metrics() -> &'static Metrics {
    global()
}

impl Metrics {
    /// 新建一份指标（测试用；服务端用 [`metrics`] 的全局实例）。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 连接建立。
    pub fn connection_opened(&self) {
        self.connections_active.fetch_add(1, Ordering::Relaxed);
        self.connections_total.fetch_add(1, Ordering::Relaxed);
    }

    /// 连接关闭。
    pub fn connection_closed(&self) {
        decrement(&self.connections_active);
    }

    /// 被限流拒绝一条连接。
    pub fn connection_rejected(&self) {
        self.connections_rejected.fetch_add(1, Ordering::Relaxed);
    }

    /// 玩家进世界。
    pub fn player_entered(&self) {
        self.players_online.fetch_add(1, Ordering::Relaxed);
    }

    /// 玩家离开世界。
    pub fn player_left(&self) {
        decrement(&self.players_online);
    }

    /// 记一次 tick 的耗时。
    pub fn record_tick(&self, elapsed: Duration) {
        let micros = u32::try_from(elapsed.as_micros()).unwrap_or(u32::MAX);
        self.ticks_total.fetch_add(1, Ordering::Relaxed);
        self.last_tick_micros.store(micros, Ordering::Relaxed);
        self.tick_micros.push(micros);
    }

    /// 记一次拍卖扫描（`scanned` 条挂单里成交了 `sold` 条）。
    pub fn record_auction_scan(&self, sold: u64) {
        self.auction_scans.fetch_add(1, Ordering::Relaxed);
        self.auctions_sold.fetch_add(sold, Ordering::Relaxed);
    }

    /// 更新邮件队列长度。
    pub fn set_mail_queue(&self, len: usize) {
        self.mail_queue
            .store(u64::try_from(len).unwrap_or(u64::MAX), Ordering::Relaxed);
    }

    /// 记一次角色落盘失败（`save_character` 返回错误）。
    pub fn record_db_save_failed(&self) {
        self.db_save_failed_total.fetch_add(1, Ordering::Relaxed);
    }

    /// 记一次行会/邮件批次落盘失败（批次已回灌队列，下一次 flush 重试）。
    pub fn record_batch_write_failed(&self) {
        self.batch_write_failed_total
            .fetch_add(1, Ordering::Relaxed);
    }

    /// 行会/邮件批次落盘失败累计次数。
    #[must_use]
    pub fn batch_write_failures(&self) -> u64 {
        self.batch_write_failed_total.load(Ordering::Relaxed)
    }

    /// 记一次 autosave 真正发起写库（不含「快照没变化所以跳过的」那一类）。
    pub fn record_autosave(&self) {
        self.autosave_total.fetch_add(1, Ordering::Relaxed);
    }

    /// 角色落盘成功累计次数。
    #[must_use]
    pub fn db_saves(&self) -> u64 {
        self.db_saves_total.load(Ordering::Relaxed)
    }

    /// 角色落盘失败累计次数。
    #[must_use]
    pub fn db_save_failed(&self) -> u64 {
        self.db_save_failed_total.load(Ordering::Relaxed)
    }

    /// autosave 写库累计次数。
    #[must_use]
    pub fn autosaves(&self) -> u64 {
        self.autosave_total.load(Ordering::Relaxed)
    }

    /// 记一次 `save_character` 的耗时。
    pub fn record_db_save(&self, elapsed: Duration) {
        self.db_saves_total.fetch_add(1, Ordering::Relaxed);
        self.db_save_micros
            .push(u32::try_from(elapsed.as_micros()).unwrap_or(u32::MAX));
    }

    /// 记一次市场/商城落盘的耗时。
    pub fn record_db_flush(&self, elapsed: Duration) {
        self.db_flushes_total.fetch_add(1, Ordering::Relaxed);
        self.db_flush_micros
            .push(u32::try_from(elapsed.as_micros()).unwrap_or(u32::MAX));
    }

    /// 记一个因包速率超限被丢弃的包。
    pub fn packet_dropped(&self) {
        self.packets_dropped.fetch_add(1, Ordering::Relaxed);
    }

    /// writer 写失败或对端断开（连接关闭）一次。
    pub fn write_failed(&self) {
        self.write_failed.fetch_add(1, Ordering::Relaxed);
    }

    /// 累计 writer 写失败/断开次数（测试/日志用）。
    #[must_use]
    pub fn write_failures(&self) -> u64 {
        self.write_failed.load(Ordering::Relaxed)
    }

    /// 当前在线玩家数（日志/测试用）。
    #[must_use]
    pub fn players_online(&self) -> u64 {
        self.players_online.load(Ordering::Relaxed)
    }

    /// 当前连接数（日志/测试用）。
    #[must_use]
    pub fn connections_active(&self) -> u64 {
        self.connections_active.load(Ordering::Relaxed)
    }

    /// 累计拒绝数（日志/测试用）。
    #[must_use]
    pub fn connections_rejected(&self) -> u64 {
        self.connections_rejected.load(Ordering::Relaxed)
    }

    /// 累计 tick 次数（日志/测试用）。
    #[must_use]
    pub fn ticks_total(&self) -> u64 {
        self.ticks_total.load(Ordering::Relaxed)
    }

    /// 最近一次 tick 耗时（微秒）。
    #[must_use]
    pub fn last_tick_micros(&self) -> u32 {
        self.last_tick_micros.load(Ordering::Relaxed)
    }

    /// 渲染 Prometheus 文本。
    ///
    /// 字段一览（`# TYPE` 行已标注类型）：
    ///
    /// | 指标 | 含义 |
    /// |---|---|
    /// | `mir2_players_online` | 当前在线玩家数 |
    /// | `mir2_players_on_map{map="0"}` | 每地图在线人数 |
    /// | `mir2_connections_active` | 当前连接数 |
    /// | `mir2_db_pool_connections` / `mir2_db_pool_idle` | 数据库连接池：当前管理/空闲连接数（soak 判据用，内存实现不发） |
    /// | `mir2_internal_records{kind=...}` | 内部集合长度（`kind` 编译期固定 5 项，慢泄漏观测；见 `INTERNAL_COLLECTION_KINDS`） |
    /// | `mir2_connections_total` | 累计连接数 |
    /// | `mir2_connections_rejected` | 被限流拒绝的连接数 |
    /// | `mir2_packets_dropped_total` | 包速率超限丢弃的包数 |
    /// | `mir2_ticks_total` | 累计 tick 次数 |
    /// | `mir2_tick_micros_last` | 最近一次 tick 耗时（微秒） |
    /// | `mir2_tick_micros_avg` / `_p50` / `_p95` / `_p99` / `_max` | tick 耗时统计 |
    /// | `mir2_tick_samples` | tick 耗时的样本窗口长度 |
    /// | `mir2_auction_scans_total` | 拍卖扫描次数 |
    /// | `mir2_auctions_sold_total` | 拍卖成交次数 |
    /// | `mir2_mail_queue` | 邮件队列长度 |
    /// | `mir2_db_save_total` / `mir2_db_save_micros_{avg,p95,max}` | 角色落盘次数/耗时 |
    /// | `mir2_db_save_failed_total` | 角色落盘失败次数（含 autosave/下线存盘） |
    /// | `mir2_autosave_total` | 周期自动存档实际写库次数 |
    /// | `mir2_batch_write_failed_total` | 行会/邮件批次写库失败次数（失败即回灌重试） |
    /// | `mir2_autosave_failures_total` | 同 `mir2_db_save_failed_total` 的语义别名（按「autosave 失败」取名） |
    /// | `mir2_db_flush_total` / `mir2_db_flush_micros_{avg,p95,max}` | 市场落盘次数/耗时 |
    #[must_use]
    pub fn render(
        &self,
        per_map: &[(String, usize)],
        pool: Option<(u32, u32)>,
        internal: &[(&'static str, usize)],
    ) -> String {
        let mut out = String::with_capacity(2048);
        push_gauge(
            &mut out,
            "mir2_players_online",
            "current players in world",
            self.players_online(),
        );
        let _ = writeln!(out, "# TYPE mir2_players_on_map gauge");
        for (map, players) in per_map {
            let _ = writeln!(
                out,
                "mir2_players_on_map{{map=\"{}\"}} {players}",
                escape_label(map)
            );
        }
        if let Some((size, idle)) = pool {
            push_gauge(
                &mut out,
                "mir2_db_pool_connections",
                "DB pool connections currently managed",
                u64::from(size),
            );
            push_gauge(
                &mut out,
                "mir2_db_pool_idle",
                "DB pool idle connections",
                u64::from(idle),
            );
        }
        if !internal.is_empty() {
            let _ = writeln!(
                out,
                "# HELP mir2_internal_records lengths of internal collections (slow-leak watch)"
            );
            let _ = writeln!(out, "# TYPE mir2_internal_records gauge");
            for (kind, len) in internal {
                // `kind` 只能来自 INTERNAL_COLLECTION_KINDS（编译期固定集合）。
                // 断言文案是**开发者可见**的（`debug_assert!` 只在 debug 构建触发，
                // 玩家看不到），所以用英文，不占玩家可见硬编码棘轮的名额。
                debug_assert!(
                    INTERNAL_COLLECTION_KINDS.contains(kind),
                    "unknown internal collection kind: {kind}"
                );
                let _ = writeln!(out, "mir2_internal_records{{kind=\"{kind}\"}} {len}");
            }
        }
        push_gauge(
            &mut out,
            "mir2_connections_active",
            "current TCP connections",
            self.connections_active(),
        );
        push_counter(
            &mut out,
            "mir2_connections_total",
            "connections accepted since start",
            self.connections_total.load(Ordering::Relaxed),
        );
        push_counter(
            &mut out,
            "mir2_connections_rejected",
            "connections rejected by the rate limiter",
            self.connections_rejected(),
        );
        push_counter(
            &mut out,
            "mir2_packets_dropped_total",
            "client packets dropped by the per-session rate limit or outbox backpressure",
            self.packets_dropped.load(Ordering::Relaxed),
        );
        push_counter(
            &mut out,
            "mir2_write_failed_total",
            "connection writer failures or peer disconnects",
            self.write_failed.load(Ordering::Relaxed),
        );
        push_counter(
            &mut out,
            "mir2_ticks_total",
            "world tick count",
            self.ticks_total(),
        );
        out.push_str("# TYPE mir2_tick_micros_last gauge\n");
        let _ = writeln!(out, "mir2_tick_micros_last {}", self.last_tick_micros());
        out.push_str("# TYPE mir2_tick_samples gauge\n");
        let _ = writeln!(
            out,
            "mir2_tick_samples {}",
            self.tick_micros.snapshot().len()
        );
        push_ring(&mut out, "mir2_tick_micros", &self.tick_micros);
        push_counter(
            &mut out,
            "mir2_auction_scans_total",
            "auction expiry scans",
            self.auction_scans.load(Ordering::Relaxed),
        );
        push_counter(
            &mut out,
            "mir2_auctions_sold_total",
            "auctions sold on expiry",
            self.auctions_sold.load(Ordering::Relaxed),
        );
        push_gauge(
            &mut out,
            "mir2_mail_queue",
            "pending mail writes",
            self.mail_queue.load(Ordering::Relaxed),
        );
        push_counter(
            &mut out,
            "mir2_db_save_total",
            "character saves",
            self.db_saves_total.load(Ordering::Relaxed),
        );
        push_ring(&mut out, "mir2_db_save_micros", &self.db_save_micros);
        let _ = writeln!(
            out,
            "# TYPE mir2_db_save_failed_total counter\nmir2_db_save_failed_total {}",
            self.db_save_failed_total.load(Ordering::Relaxed),
        );
        let _ = writeln!(
            out,
            "# TYPE mir2_autosave_total counter\nmir2_autosave_total {}",
            self.autosave_total.load(Ordering::Relaxed),
        );
        let _ = writeln!(
            out,
            "# TYPE mir2_batch_write_failed_total counter\nmir2_batch_write_failed_total {}",
            self.batch_write_failed_total.load(Ordering::Relaxed),
        );
        // 别名（队长口径）：与 `mir2_db_save_failed_total` 同值，按语义命名更直观。
        let _ = writeln!(
            out,
            "# TYPE mir2_autosave_failures_total counter\nmir2_autosave_failures_total {}",
            self.db_save_failed_total.load(Ordering::Relaxed),
        );
        push_counter(
            &mut out,
            "mir2_db_flush_total",
            "market/gameshop flushes",
            self.db_flushes_total.load(Ordering::Relaxed),
        );
        push_ring(&mut out, "mir2_db_flush_micros", &self.db_flush_micros);
        out
    }

    /// 30 秒汇总日志用的紧凑一行（字段化，便于 grep `metrics summary`）。
    #[must_use]
    pub fn summary_line(&self, per_map: usize) -> String {
        format!(
            "players={} maps={} connections={} rejected={} ticks={} tick_last_us={} tick_p99_us={} db_saves={} db_save_p95_us={} db_save_failed={} autosaves={} db_flushes={} db_flush_p95_us={} mail_queue={} dropped={} write_failed={}",
            self.players_online(),
            per_map,
            self.connections_active(),
            self.connections_rejected(),
            self.ticks_total(),
            self.last_tick_micros(),
            self.tick_micros.quantile(0.99).unwrap_or(0),
            self.db_saves_total.load(Ordering::Relaxed),
            self.db_save_micros.quantile(0.95).unwrap_or(0),
            self.db_save_failed_total.load(Ordering::Relaxed),
            self.autosave_total.load(Ordering::Relaxed),
            self.db_flushes_total.load(Ordering::Relaxed),
            self.db_flush_micros.quantile(0.95).unwrap_or(0),
            self.mail_queue.load(Ordering::Relaxed),
            self.packets_dropped.load(Ordering::Relaxed),
            self.write_failed.load(Ordering::Relaxed),
        )
    }
}

/// 原子减一但不允许变负（CAS 循环；MSRV 1.85 没有 `try_update`）。
fn decrement(counter: &AtomicU64) {
    let mut current = counter.load(Ordering::Relaxed);
    loop {
        let next = current.saturating_sub(1);
        match counter.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(actual) => current = actual,
        }
    }
}

/// 内部集合的**编译期固定** `kind` 集合（禁止运行时动态 label：基数必须可枚举）。
///
/// 慢泄漏观测（第 24 轮）：4h soak 的 RSS 判据对「每天 +4.6MB」这类慢增长不敏感
/// （4h 只涨 800KB，淹没在噪声里），所以把关键集合的长度做成 gauge 单独观测。
pub const INTERNAL_COLLECTION_KINDS: [&str; 5] = [
    "db_save",           // pg_store::DB_SAVE_RECORDS —— 环形 cap=1024（有界）
    "mail_writes",       // world::mail_writes —— 每包 drain（take_mail_writes），失败保留
    "pending_hits",      // world::pending_hits —— 300ms 后 resolve_pending_hits 结算
    "pending_map_moves", // world::pending_map_moves —— 每 tick resolve_pending_map_moves
    "market_searches",   // world::market_searches —— 每个在线玩家至多 1 条，离开即 remove
];

fn push_gauge(out: &mut String, name: &str, help: &str, value: u64) {
    let _ = writeln!(out, "# HELP {name} {help}");
    let _ = writeln!(out, "# TYPE {name} gauge");
    let _ = writeln!(out, "{name} {value}");
}

fn push_counter(out: &mut String, name: &str, help: &str, value: u64) {
    let _ = writeln!(out, "# HELP {name} {help}");
    let _ = writeln!(out, "# TYPE {name} counter");
    let _ = writeln!(out, "{name} {value}");
}

fn push_ring(out: &mut String, name: &str, ring: &Ring) {
    let _ = writeln!(out, "# TYPE {name} gauge");
    let avg = ring.average().unwrap_or(0);
    let p50 = ring.quantile(0.50).unwrap_or(0);
    let p95 = ring.quantile(0.95).unwrap_or(0);
    let p99 = ring.quantile(0.99).unwrap_or(0);
    let max = ring.max().unwrap_or(0);
    let _ = writeln!(out, "{name}_avg {avg}");
    let _ = writeln!(out, "{name}_p50 {p50}");
    let _ = writeln!(out, "{name}_p95 {p95}");
    let _ = writeln!(out, "{name}_p99 {p99}");
    let _ = writeln!(out, "{name}_max {max}");
}

/// Prometheus label 转义：只需要处理 `\`、`"`、换行。
fn escape_label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// 绑定并后台跑 `/metrics` HTTP 服务，返回**实际**绑定的地址（端口传 0 时有用）。
///
/// # Errors
/// 绑定失败（端口被占用/无权限）时返回错误，调用方决定是放弃指标还是拒绝启动。
pub type PoolProbe = Arc<dyn Fn() -> Option<(u32, u32)> + Send + Sync>;

pub async fn bind_metrics(
    listen: SocketAddr,
    world: Arc<Mutex<World>>,
    pool: PoolProbe,
) -> std::io::Result<SocketAddr> {
    let listener = TcpListener::bind(listen).await?;
    let address = listener.local_addr()?;
    tokio::spawn(async move {
        if let Err(error) = serve_http(listener, world, pool).await {
            tracing::warn!(%error, "metrics 服务退出");
        }
    });
    Ok(address)
}

/// 极简 HTTP：只认 `GET /metrics`（及 `/metrics?…`），其它回 404。
async fn serve_http(
    listener: TcpListener,
    world: Arc<Mutex<World>>,
    pool: PoolProbe,
) -> std::io::Result<()> {
    loop {
        let (mut stream, peer) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(error) => {
                tracing::debug!(%error, "metrics accept 失败");
                continue;
            }
        };
        let world = Arc::clone(&world);
        let pool = Arc::clone(&pool);
        tokio::spawn(async move {
            let mut buffer = [0u8; 8192];
            // 只读一次：请求行 + 头，够判断路径了；读不到就直接关。
            let Ok(read) = stream.read(&mut buffer).await else {
                return;
            };
            let request = String::from_utf8_lossy(&buffer[..read]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");
            let (status, body) = if path == "/metrics" || path.starts_with("/metrics?") {
                let metrics = metrics();
                let (per_map, internal) = {
                    let world = world.lock().await;
                    let per_map = world.player_counts_by_map();
                    // 慢泄漏观测：同一把锁里取内部集合长度，保证与 per_map 是同一时刻的快照。
                    let internal: Vec<(&'static str, usize)> = vec![
                        ("db_save", crate::pg_store::db_save_record_count()),
                        ("mail_writes", world.pending_mail_writes()),
                        ("pending_hits", world.pending_hits().len()),
                        ("pending_map_moves", world.pending_map_moves().len()),
                        ("market_searches", world.market_search_count()),
                    ];
                    (per_map, internal)
                };
                ("200 OK", metrics.render(&per_map, pool(), &internal))
            } else if path == "/healthz" {
                ("200 OK", "ok\n".to_owned())
            } else {
                ("404 Not Found", "not found\n".to_owned())
            };
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: text/plain; version=0.0.4; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.shutdown().await;
            tracing::debug!(%peer, path, status, "metrics 请求");
        });
    }
}

impl World {
    /// 每张地图的在线玩家数（按地图名排序，供 `/metrics` 与 30 秒汇总日志用）。
    ///
    /// 只读 `pub(crate) players`，不改 `world.rs`（该文件归兵部/吏部）。
    #[must_use]
    pub fn player_counts_by_map(&self) -> Vec<(String, usize)> {
        let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
        for player in self.players.values() {
            *counts.entry(player.map.as_str()).or_insert(0) += 1;
        }
        counts
            .into_iter()
            .map(|(map, count)| (map.to_owned(), count))
            .collect()
    }
}

/// 便于测试/工具构造一个 `Arc<Metrics>`。
#[must_use]
pub fn shared() -> Arc<Metrics> {
    Arc::new(Metrics::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_move_up_and_down_without_going_negative() {
        let metrics = Metrics::new();
        metrics.connection_opened();
        metrics.connection_opened();
        assert_eq!(metrics.connections_active(), 2);
        metrics.connection_closed();
        assert_eq!(metrics.connections_active(), 1);
        // 多减也不会变负（边界保护）。
        metrics.connection_closed();
        metrics.connection_closed();
        assert_eq!(metrics.connections_active(), 0);
        metrics.player_entered();
        assert_eq!(metrics.players_online(), 1);
        metrics.player_left();
        assert_eq!(metrics.players_online(), 0);
    }

    #[test]
    fn ring_keeps_only_the_latest_samples() {
        let ring = Ring::with_capacity(4);
        for value in [10_u32, 20, 30, 40, 50] {
            ring.push(value);
        }
        assert_eq!(ring.total(), 5);
        // 容量 4：最早的 10 被覆盖，快照是升序的 [20,30,40,50]。
        assert_eq!(ring.snapshot(), vec![20, 30, 40, 50]);
        assert_eq!(ring.max(), Some(50));
        assert_eq!(ring.average(), Some(35));
    }

    #[test]
    fn ring_quantiles_on_a_known_series() {
        let ring = Ring::with_capacity(100);
        for value in 1..=100_u32 {
            ring.push(value);
        }
        // 最近秩分位：index = round(q * (n-1))，n=100 时 p50→51、p95→95、p99→99。
        assert_eq!(ring.quantile(0.50), Some(51));
        assert_eq!(ring.quantile(0.95), Some(95));
        assert_eq!(ring.quantile(0.99), Some(99));
        assert_eq!(ring.quantile(0.0), Some(1));
        assert_eq!(ring.quantile(1.0), Some(100));
    }

    #[test]
    fn empty_ring_reports_nothing() {
        let ring = Ring::new();
        assert_eq!(ring.snapshot(), Vec::<u32>::new());
        assert_eq!(ring.quantile(0.5), None);
        assert_eq!(ring.average(), None);
        assert_eq!(ring.max(), None);
    }

    #[test]
    fn tick_and_db_samples_land_in_separate_rings() {
        let metrics = Metrics::new();
        metrics.record_tick(Duration::from_micros(1_500));
        metrics.record_db_save(Duration::from_micros(700));
        metrics.record_db_flush(Duration::from_micros(2_000));
        assert_eq!(metrics.ticks_total(), 1);
        assert_eq!(metrics.last_tick_micros(), 1_500);
        assert_eq!(metrics.db_saves_total.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.db_save_failed(), 0);
        assert_eq!(metrics.autosaves(), 0);
        assert_eq!(metrics.batch_write_failures(), 0);
        metrics.record_db_save_failed();
        metrics.record_autosave();
        metrics.record_batch_write_failed();
        assert_eq!(metrics.db_save_failed(), 1);
        assert_eq!(metrics.autosaves(), 1);
        assert_eq!(metrics.batch_write_failures(), 1);
        assert_eq!(metrics.db_flushes_total.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.tick_micros.max(), Some(1_500));
        assert_eq!(metrics.db_save_micros.max(), Some(700));
        assert_eq!(metrics.db_flush_micros.max(), Some(2_000));
    }

    #[test]
    fn write_failures_and_packet_drops_are_independent() {
        let metrics = Metrics::new();
        metrics.packet_dropped();
        assert_eq!(metrics.write_failures(), 0);
        metrics.write_failed();
        assert_eq!(metrics.write_failures(), 1);
        let body = metrics.render(&[], None, &[]);
        assert!(body.contains("mir2_packets_dropped_total 1"), "{body}");
        assert!(body.contains("mir2_write_failed_total 1"), "{body}");
        assert!(
            metrics.summary_line(1).contains("dropped=1 write_failed=1"),
            "{}",
            metrics.summary_line(1)
        );
    }

    #[test]
    fn render_contains_the_documented_fields() {
        let metrics = Metrics::new();
        metrics.connection_opened();
        metrics.connection_rejected();
        metrics.player_entered();
        metrics.record_tick(Duration::from_micros(900));
        metrics.record_auction_scan(2);
        metrics.set_mail_queue(3);
        metrics.record_db_save(Duration::from_micros(120));
        metrics.record_db_flush(Duration::from_micros(340));
        metrics.packet_dropped();
        metrics.write_failed();
        let body = metrics.render(&[("0".to_owned(), 1)], None, &[]);
        for needle in [
            "mir2_players_online 1",
            "mir2_players_on_map{map=\"0\"} 1",
            "mir2_connections_active 1",
            "mir2_connections_rejected 1",
            "mir2_packets_dropped_total 1",
            "mir2_write_failed_total 1",
            "mir2_ticks_total 1",
            "mir2_tick_micros_last 900",
            "mir2_tick_micros_p50 900",
            "mir2_auction_scans_total 1",
            "mir2_auctions_sold_total 2",
            "mir2_mail_queue 3",
            "mir2_db_save_total 1",
            "mir2_db_flush_total 1",
        ] {
            assert!(body.contains(needle), "缺少 {needle}：\n{body}");
        }
        assert!(body.contains("# TYPE mir2_ticks_total counter"));
    }

    #[test]
    fn label_escaping_covers_quotes_and_backslashes() {
        assert_eq!(escape_label("a\"b\\c"), "a\\\"b\\\\c");
        assert_eq!(escape_label("m\nn"), "m\\nn");
    }

    #[test]
    fn summary_line_is_delimited_by_spaces() {
        let metrics = Metrics::new();
        metrics.record_tick(Duration::from_micros(2_000));
        let line = metrics.summary_line(1);
        assert!(line.contains("ticks=1"), "{line}");
        assert!(line.contains("tick_last_us=2000"), "{line}");
        assert!(line.contains("players=0"), "{line}");
    }
}
