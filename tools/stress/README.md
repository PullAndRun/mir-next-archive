# 并发压测（`tools/stress`）

用真实 TCP 连接模拟 N 个玩家：每个连接走完整的「版本 → 注册/登录 → 建角/选角 → 进世界」
握手，然后按固定频率走动 / 攻击 / 发聊天，同时每秒抓一次服务端 `/metrics`
（Prometheus 文本）与进程 RSS/CPU。只依赖 Python 标准库（协议实现复用
[`tools/playtest/playtest.py`](../playtest/playtest.py)，避免两处协议口径漂移）。

## 用法

```powershell
# 1) 起服务端（**必须**隐藏窗口；/metrics 需要先设 MIR2_METRICS_LISTEN）
#    测之前先确认机器空闲：Get-CimInstance Win32_Processor | Select LoadPercentage
#    （本机基线要求 load ≲ 10% 且没有别人的 cargo/rustc/mir-server 在跑）
$env:MIR2_METRICS_LISTEN = '127.0.0.1:9110'
$env:MIR2_MAX_CONN_PER_IP = '2000'          # 300 个连接都来自 127.0.0.1
$env:MIR2_MAX_CONN_PER_SEC = '2000'
powershell -NoProfile -ExecutionPolicy Bypass -File tools/dev/serve.ps1 -Port 7042

# 2) 三档（空闲基线 / 100 / 300，各 60 秒）
py tools/stress/stress.py --server 127.0.0.1:7042 --metrics-port 9110 --clients 0   --duration 60 --out target/stress-0.json
py tools/stress/stress.py --server 127.0.0.1:7042 --metrics-port 9110 --clients 100 --duration 60 --out target/stress-100.json
py tools/stress/stress.py --server 127.0.0.1:7042 --metrics-port 9110 --clients 300 --duration 60 --out target/stress-300.json

# 3) 停服务端
powershell -NoProfile -ExecutionPolicy Bypass -File tools/dev/serve.ps1 -Stop -Port 7042
```

结果写到 `target/stress-<N>.json`（`--out` 可改），字段含义见 `report()`。

* `--clients 0` = **空闲基线**：不建连接，只采 `/metrics` 与进程指标（要跑满 60 秒，
  512 tick 的环形窗口才会只包含本次样本，否则 p99 会混进启动期）。
* **Windows 控制台编码**：脚本启动时会把 `stdout/stderr` 切到 `utf-8, errors=replace`，
  并通过统一出口 `_emit()` 打印。不这么做的话，默认 GBK 编码下表格表头的 `µs`
  会抛 `UnicodeEncodeError: 'gbk' codec can't encode '\xb5'`，整条命令 exit 1、
  JSON 写出来了但表格看不见（刑部第 12 轮 F3）。

## 判据

脚本最后打印判据（退出码 1 = 不达标）：

| 判据 | 阈值 | 参数 |
|---|---|---|
| 连接成功率 | ≥ 95% | 固定 |
| tick p99 | ≤ tick 预算 100ms | `--max-tick-p99` |
| tick p99 告警线 | > 50ms 只 `[WARN]` | `--warn-tick-p99` |
| 被限流拒绝的连接 | 0 | 固定 |

用法示例（对准不同目标时改判据，不用改代码）：

```
--max-tick-p99 60      # 「tick p99 必须 ≤ 60ms」当硬判据（超了 exit 1）
--warn-tick-p99 50     # 只告警不算失败的那条线（默认 50ms）
```

`--clients 0` 是空闲档：不建连接、只采 `/metrics` 与进程指标，**也要跑满 60 秒**
（512 tick 的环形窗口 ≈ 51 秒，短了会把启动期样本混进 p99）。

## 实测基线（本机，2026-10-20，**静默窗口**）

环境（三档都是「干跑」，启动前 `Get-Process mir-server,cargo,rustc` 计数 0）：

* 服务端：`target/debug`（dev profile，未开优化）+ 真实 `Server.MirDB`（比奇 1922 只怪、46 个 NPC）
  + 内存存储（`MemoryStore`，未配 `DATABASE_URL`）；tick 周期 100ms；`MIR2_AUTOSAVE_SECS=30` 默认。
* 客户端：每连接 4 pkt/s（走动:攻击:聊天 = 2:2:1），全部挤在比奇出生点（最坏情况的同图扇出）。
* 运行：每档**全新服务端**、60 秒（空闲档也跑满 60 秒，让 512 tick 的环形窗口只含本次样本）。
* 空闲度证据：空闲档的 **tick p50 = 0.145 ms**（机器被占时这个数会到几 ms）；
  `host CPU` 用 `GetSystemTimes` 采（`Win32_Processor.LoadPercentage` 在 32 核机器上抖得没法用）。

| 档位 | tick p50 | p95 | p99 | tick max | 服务端 CPU | 整机 CPU 均值/峰值 | RSS 峰值 | DB 写次数 | 收到包数 | 判据 |
|---|---|---|---|---|---|---|---|---|---|---|
| **0（空闲）** | **0.145 ms** | 0.230 ms | **0.244 ms** | 0.35 ms | 0.3% | 16.1% / 24.0% | 26.4 MiB | 0 | 0 | exit 0 |
| **100 连接** | **6.38 ms** | 9.24 ms | **14.84 ms** | 18.1 ms | 138.5% | 22.5% / 26.7% | 47.1 MiB | 206 | 1.10 M | exit 0 |
| **300 连接** | **58.8 ms** | 183.8 ms | **232.3 ms** | 320 ms | 963% | 55.8% / 67.8% | 108.1 MiB | 628 | 7.46 M | exit 1（p99 超 200ms 自定义判据） |

> 300 档的整机 CPU 55.8%/67.8% 是**压测自己**造成的（服务端 963% ≈ 10 个核），不是外部干扰。

JSON：`target/stress-0.json`、`target/stress-100.json`、`target/stress-300.json`
（含 `host_cpu_percent_avg/peak`、`packets_received`、`db_saves_delta`、`autosaves_delta` 等）。

### 结论

1. **空闲几乎零成本**：p99 **0.244 ms**（t95 之前的旧基线是 **46.6 ms**）—— 怪物 AI 分帧 + 空闲短路生效。
2. **100 人同图**：p99 **14.8 ms**，远低于「< 30 ms」目标 ✅；100% 握手 + 100% 进世界、0 断线、0 限流、0 丢包。
3. **300 人同图仍超预算**：p99 **232 ms**（目标 60/80 ms）❌，而且**波动极大** ——
   本机历史实测出现过 **75 / 121 / 163 / 232 / 260 ms**，随机器负载线性恶化。
   所以这一档请按**区间**看（`~75–260 ms`），不要拿单点当结论。
   根因是**同图视野扇出**（300 人挤出生点时收到包数 7.46 M/分钟），不是 AI 也不是连接处理；
   后续计划见第 13 轮「写聚合/AOI 合批」。
4. **连接/会话层不是瓶颈**：三档都是 100% 握手 + 100% 进世界、0 限流拒绝、0 丢包
   （`mir2_connections_rejected` / `mir2_packets_dropped_total` 全 0）。

### 与旧基线的对比（t93 → 现在的变化）

| 档位 | t93（t95 之前） | 现在（t95 之后） |
|---|---|---|
| 空闲 p99 | 46.6 ms | **0.244 ms** |
| 100 人 p99 | 56.9 ms | **14.8 ms** |
| 300 人 p99 | 107 ms | 75–260 ms（波动，负载相关） |

> 注意：t93 那组是在**另一台机器的不同负载**下测的（当时没有静默窗口概念），
> 只能看数量级；要横向比对请用同一静默窗口重跑。

