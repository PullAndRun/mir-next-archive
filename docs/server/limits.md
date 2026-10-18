# 抗刷与连接限制（第 6 轮新增）

本作在 Crystal 的 `Server.cs` 之外加了一层**抗刷/保护**：限制单 IP 并发、连接速率、
空闲连接与会话包速率。实现分布在 `crates/mir-server/src/{config,server,session}.rs`，
配置项都在 `ServerConfig`，环境变量见下表；**`0` 表示关闭该项限制**。

## 1. 配置项与默认值

| 配置字段 | 环境变量 | 默认 | 触发行为 |
|---|---|---|---|
| `max_connections_per_ip` | `MIR2_MAX_CONN_PER_IP` | **5** | 同一 IP 并发连接超限：**立刻关闭新连接**并计数（`tracing::warn!`），不进入会话 |
| `max_connections_per_second` | `MIR2_MAX_CONN_PER_SEC` | **10** | 同一 IP 每秒新建连接超限：同上（按 Unix 秒固定窗口） |
| `idle_timeout_secs` | `MIR2_IDLE_TIMEOUT_SECS` | **60** | 该秒数内收不到客户端**任何**包：断开连接（Crystal 客户端每分钟发一次 `C.KeepAlive`，所以「有包」=「还活着」） |
| `max_packets_per_second` | `MIR2_MAX_PACKETS_PER_SEC` | **200** | 单会话每秒超限的包**丢弃并记一条 warn**（每个窗口只记一次），**不踢人**（避免网络抖动误伤） |

配置读取：`ServerConfig::default()` 给出上表默认值，`ServerConfig::from_env()` 用环境变量覆盖；
`crates/mir-server/src/config.rs` 有对应单测。

## 2. 实现位置与语义

### 2.1 连接限流（`server.rs` 的 `ConnectionLimiter`）

* accept 之后、**创建会话之前**调用 `try_acquire(ip, unix_secs)`，返回
  `TooFast`（每秒速率超限）或 `TooManyConcurrent`（并发超限）就 `drop(stream)`；
* 先判定、后记账：被拒的连接**不占配额**（避免手滑重连把自己锁死一整秒）；
* 连接结束时 `release(ip)` 释放并发槽；每秒窗口不需要释放，按 Unix 秒自然滚动；
* 被拒计数累加在 `rejected`，停机日志会打印总数。

### 2.2 心跳/空闲超时（`server.rs` 读循环）

读循环用 `tokio::select!` 同时等「读到数据」「`@kick` 通知」「`idle_wait(idle_timeout_secs)`」
「`@reboot` 到点」；空闲分支命中即 `break` 断开，日志字段 `session / peer / secs`。
`idle_timeout_secs = 0` 时该分支永不触发（不限时空闲连接）。

### 2.3 单会话包速率（`session.rs` 的 `rate_allows_packet`）

* 用**单调时钟**做 1 秒窗口，窗口内计数超过 `max_packets_per_second` 的包直接丢弃；
* 每个窗口只记一条 `tracing::warn!`（`session / peer / limit`），防止被刷屏时日志压垮磁盘；
* **不踢线**：网络抖动时客户端会一次性补发积压包，踢掉反而误伤。

## 3. 与 Crystal 的差异

* Crystal 的 `Server.cs` 只做 `accept`，没有 IP 并发/连接速率/包速率限制；本作新增这一层
  用于挡端口扫描、单机多开刷号与简单洪水包；
* Crystal 没有服务端空闲超时（客户端自己做 KeepAlive）；本作把「超时」做成可配置的断开；
* 这些限制**只影响连接与包处理频率**，不改变任何游戏协议语义（正常客户端每秒包数远低于 200）。

## 4. 测试

* `crates/mir-server/tests/anti_flood.rs`：`ConnectionLimiter` 的并发/每秒窗口上限与释放；
* `crates/mir-server/tests/reboot.rs`：`@reboot` 倒计时与优雅停机（踢线 + 停止 accept）。
