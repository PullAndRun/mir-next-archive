# 服务端指标与 `/metrics`（第 7 轮新增）

实现：`crates/mir-server/src/metrics.rs`（采集 + 极简 HTTP 端点），
30 秒汇总日志在 `server.rs` 的 tick 任务里（`METRICS_LOG_INTERVAL_MS = 30_000`）。

## 1. 设计约束

* **采集不能拖慢 tick**：计数用无锁原子；耗时用固定大小环形缓冲
  （`RING_CAPACITY = 512` 个样本，100ms tick 约覆盖 51 秒），`record_*` 内**不分配、不加锁**；
  `/metrics` 请求到来时才做快照（此时分配无所谓）。
* **不引 HTTP 框架**：手写只认 `GET /metrics` 的极小服务，其它路径 404，读请求最多 8 KB。
* **默认关闭**：只有配置了监听地址才启动。

## 2. 启用

```powershell
# 只读端点，不鉴权：生产请绑内网/回环
$env:MIR2_METRICS_LISTEN = "127.0.0.1:9100"
cargo run -p mir-server
```

* 配置项：`ServerConfig::metrics_listen: Option<SocketAddr>`；`MIR2_METRICS_LISTEN`
  解析失败会 warn 并保持关闭；
* 监听地址 `0.0.0.0:7000` 是游戏端口，metrics 用**另一个端口**（示例 `9100`）。

## 3. 指标字段

输出为 Prometheus 文本格式（`# TYPE` + `name value`）：

| 指标 | 类型 | 含义 |
|---|---|---|
| `mir2_players_online` | gauge | 当前在线玩家数 |
| `mir2_players_on_map{map="0"}` | gauge | 每张地图的在线人数（按地图名排序） |
| `mir2_connections_active` | gauge | 当前 TCP 连接数 |
| `mir2_connections_total` | counter | 启动以来接受的连接数 |
| `mir2_connections_rejected` | counter | 被抗刷限流拒绝的连接数 |
| `mir2_packets_dropped_total` | counter | 单会话包速率超限丢弃的包数 |
| `mir2_ticks_total` | counter | 世界 tick 次数 |
| `mir2_tick_micros_last` | gauge | 最近一次 tick 耗时（微秒） |
| `mir2_tick_samples` | gauge | tick 耗时环形缓冲当前样本数 |
| `mir2_tick_micros_{avg,p50,p95,p99,max}` | gauge | tick 耗时分位数/平均/最大（微秒） |
| `mir2_auction_scans_total` | counter | 拍卖到期扫描次数 |
| `mir2_auctions_sold_total` | counter | 到期自动成交次数 |
| `mir2_mail_queue` | gauge | 待写邮件队列长度 |
| `mir2_db_save_total` | counter | 角色落盘次数 |
| `mir2_db_save_failed_total` | counter | 角色落盘失败次数（含 autosave 与下线存盘） |
| `mir2_autosave_total` | counter | 周期自动存档**实际写库**次数（快照没变被跳过的不计） |
| `mir2_autosave_failures_total` | counter | `mir2_db_save_failed_total` 的语义别名（同值） |
| `mir2_batch_write_failed_total` | counter | 行会/邮件批次写库失败次数（失败即回灌重试） |
| `mir2_db_save_micros_{avg,p50,p95,p99,max}` | gauge | 角色落盘耗时（微秒） |
| `mir2_db_flush_total` | counter | 市场/商城落盘次数 |
| `mir2_db_flush_micros_{avg,p50,p95,p99,max}` | gauge | 市场/商城落盘耗时（微秒） |

## 4. Prometheus 抓取示例

```yaml
# prometheus.yml
scrape_configs:
  - job_name: mir2
    scrape_interval: 15s
    static_configs:
      - targets: ["127.0.0.1:9100"]
```

```bash
curl -s http://127.0.0.1:9100/metrics | head
```

## 5. 30 秒汇总日志

tick 任务每 30 秒打一条 `tracing::info!`，`metrics` 字段是空格分隔的紧凑键值：

```text
INFO metrics summary metrics="players=12 maps=3 connections=15 rejected=0 ticks=18342 tick_last_us=412 tick_p99_us=980 db_saves=57 db_save_p95_us=3200 db_save_failed=0 autosaves=12 db_flushes=4 db_flush_p95_us=5100 mail_queue=1"
```

字段：`players / maps / connections / rejected / ticks / tick_last_us / tick_p99_us /
db_saves / db_save_p95_us / db_save_failed / autosaves / db_flushes / db_flush_p95_us /
mail_queue`。

## 6. 与 Crystal 的差异

Crystal 没有指标端点与汇总日志（只有 `Envir` 内部计数）；本作新增这一层用于观测
tick 抖动、DB 落盘耗时、限流拒绝与邮件队列积压，不改变任何游戏协议。
