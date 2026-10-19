# 并发压测（`tools/stress`）

用真实 TCP 连接模拟 N 个玩家：每个连接走完整的「版本 → 注册/登录 → 建角/选角 → 进世界」
握手，然后按固定频率走动 / 攻击 / 发聊天，同时每秒抓一次服务端 `/metrics`
（Prometheus 文本）与进程 RSS/CPU。只依赖 Python 标准库（协议实现复用
[`tools/playtest/playtest.py`](../playtest/playtest.py)，避免两处协议口径漂移）。

## 用法

```powershell
# 1) 起服务端（**必须**隐藏窗口；/metrics 需要先设 MIR2_METRICS_LISTEN）
$env:MIR2_METRICS_LISTEN = '127.0.0.1:9110'
$env:MIR2_MAX_CONN_PER_IP = '2000'          # 300 个连接都来自 127.0.0.1
$env:MIR2_MAX_CONN_PER_SEC = '2000'
powershell -NoProfile -ExecutionPolicy Bypass -File tools/dev/serve.ps1 -Port 7042

# 2) 压测（N=100 / 300，各 60 秒）
py tools/stress/stress.py --server 127.0.0.1:7042 --metrics-port 9110 --clients 100 --duration 60
py tools/stress/stress.py --server 127.0.0.1:7042 --metrics-port 9110 --clients 300 --duration 60

# 3) 停服务端
powershell -NoProfile -ExecutionPolicy Bypass -File tools/dev/serve.ps1 -Stop -Port 7042
```

结果写到 `target/stress-<N>.json`（`--out` 可改），字段含义见 `report()`。

## 判据

脚本最后打印判据（退出码 1 = 不达标）：

| 判据 | 阈值 | 参数 |
|---|---|---|
| 连接成功率 | ≥ 95% | 固定 |
| tick p99 | ≤ tick 预算 100ms | `--max-tick-p99` |
| tick p99 告警线 | > 50ms 只 `[WARN]` | `--warn-tick-p99` |
| 被限流拒绝的连接 | 0 | 固定 |

## 实测基线（本机，2026-10-19）

服务端：`target/debug` 优化关闭（dev profile）+ 真实 `Server.MirDB`（比奇 1922 只怪、46 个 NPC）
+ 内存存储（`MemoryStore`，没有配 `DATABASE_URL`）；每连接 4 pkt/s（走动:攻击:聊天 = 2:2:1）；
tick 周期 100ms；`MIR2_AUTOSAVE_SECS=30` 默认。

| 连接数 | 进世界成功率 | tick p50 | tick p95 | tick p99 | tick max | CPU 均值 | RSS 峰值 | DB 写次数 | DB 写 p95 | 限流拒绝 | 丢包 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 0（空闲基线） | - | 0.37 ms | 14.0 ms | 46.6 ms | 62.9 ms | - | 26.3 MiB | 0 | - | 0 | 0 |
| 20（冒烟） | 100% | 0.42 ms | 38.7 ms | 67.3 ms | 189.8 ms | 9.9% | 35.5 MiB | 60 | 1.69 ms | 0 | 0 |
| **100** | 100% | 0.81 ms | 22.5 ms | **56.9 ms** | 68.6 ms | 126.7% | 47.3 MiB | 200 | 12.97 ms | 0 | 0 |
| **300** | 100% | 7.60 ms | 67.5 ms | **107.1 ms** | 197.2 ms | 1069.1% | 97.1 MiB | 651 | 1.34 ms | 0 | 0 |

### 结论

1. **连接/会话处理不是瓶颈**：300 连接全部握手成功、进世界成功、0 断线、0 限流拒绝、0 丢包；
   RSS 97 MiB、DB 写 651 次平均 0.45 ms。
2. **tick 尾部延迟来自世界 tick 本身，不是连接数**：空闲（0 玩家）时 p99 就已经 46.6 ms、
   max 62.9 ms —— 比奇一张图 1922 只怪的怪物 AI 是主要开销（`World::tick()` 单线程、全局一把锁）。
   100 连接把 p95 从 14 ms 抬到 22 ms（AOI 广播），p99 仍与空闲同量级。
3. **300 连接开始超过 tick 预算**（p99 107 ms > 100 ms）：p50 从 0.81 ms 涨到 7.6 ms（约 9×），
   说明同图 300 人的视野广播是超线性增长的主因。要支撑 300+ 同图玩家，下一步该做的是
   怪物 AI 分帧/分片（按地图或按象限摊到多个 tick）与 AOI 广播合批，而不是加连接处理线程。
4. `20` 与应用例只用于验证工具本身（时长短、含启动期 sample，p99 不具参考性）。

> 说明：`/metrics` 的 tick 统计是**最近 512 次 tick** 的环形窗口（`RING_CAPACITY`），
> 60 秒 × 100ms = 600 tick，所以 60 秒跑的 p99 已经只覆盖压测窗口本身；空闲基线同样跑满
> 60 秒再读。DB 写次数 = 登录首存 + 下线存 + 30s autosave（含 `mir2_autosave_total`）。

## 复现

```text
py tools/stress/stress.py --server 127.0.0.1:7042 --metrics-port 9110 --clients 100 --duration 60
py tools/stress/stress.py --server 127.0.0.1:7042 --metrics-port 9110 --clients 300 --duration 60
```

JSON：`target/stress-100.json`、`target/stress-300.json`。
