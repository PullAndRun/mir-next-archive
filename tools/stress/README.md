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

## 实测基线（第 13 轮，兵部/刑部两次测量）

**测量条件（重要）**：必须在**空载机器**上测——`Get-CimInstance Win32_Processor | Select
LoadPercentage` **≲ 10%**，且没有别人的 `cargo`/`rustc`/`mir-server` 在跑；否则 p99 会被
放大数倍，数字不可比。每档全新服务端、跑满 60 秒（512 tick 环形窗口 ≈51s）；空闲档的
tick p50 是「机器确实空闲」的硬证据。

环境：dev profile + 真实 `Server.MirDB`（比奇 1922 怪 / 46 NPC）+ 内存存储；tick 100ms；
`MIR2_AUTOSAVE_SECS=30`；每连接 4 pkt/s。

| 场景 | tick p99 | 说明 |
|---|---|---|
| **空闲**（`--clients 0`） | **0.24 ~ 1.8 ms** | 随机器负载变化；**0.244 ms** 是 t100 静默窗口的值 |
| **100 连接，默认 mix（2:2:1）** | **3.9 ~ 11.7 ms** | 两次测量区间 |
| **300 连接，默认 mix（2:2:1）** | **14 ~ 17 ms** | 优化前 **95.6 ms**；同图扇出已由写聚合/AOI 合批压下来 |
| **300 连接，`--mix 1:0:0`**（只走动） | **14 ~ 17 ms** | 优化前 **1054 ms** |
| **300 连接，`--spread`**（各走各的方向） | **11.6 ~ 16.4 ms** | 散开 vs 挤出生点的对比 |

三档连接层：**100% 握手 + 100% 进世界、0 断线、0 限流拒绝、0 丢包** →
连接/会话层不是瓶颈。

复现（把 `--mix`/`--spread` 按场景加上）：

```powershell
py tools/stress/stress.py --server 127.0.0.1:7042 --metrics-port 9110 --clients 300 --duration 60 --mix 1:0:0
py tools/stress/stress.py --server 127.0.0.1:7042 --metrics-port 9110 --clients 300 --duration 60 --spread
```

JSON 字段：`host_cpu_percent_avg/peak`、`packets_received`、`db_saves_delta`、
`autosaves_delta` 等（`target/stress-<N>.json`）。

### 结论

1. **写聚合/AOI 合批已落地（`6351d5d`）**：300 同图 p99 从优化前 95.6ms（默认 mix）/
   1054ms（只走动）降到 **14~17ms**，目标（60/80ms）达成；
2. **100 人同图** p99 **3.9~11.7ms**，远低于「<30ms」；
3. **空闲** p99 **0.24~1.8ms**（随负载）；t100 静默窗口值 0.244ms；
4. 仍然要**空载测量**：机器被占用时同一场景的 p99 会放大数倍，区间会漂；
5. 历史对比：t93（t95 之前）空闲 46.6ms / 100 人 56.9ms / 300 人 107ms——不同时段与负载，
   只能看数量级。

