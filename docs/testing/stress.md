# 压测（`tools/stress/stress.py`，第 9/12 轮）

用途：用真实协议、真实服务端拉 N 个并发连接，测握手/进世界成功率、tick 延迟、CPU/RSS、
DB 写次数与限流/丢包，作为容量基线。脚本纯 stdlib，协议复用 `tools/playtest/playtest.py`；
用法与参数见 [`tools/stress/README.md`](../../tools/stress/README.md)。

## 1. 复现

```powershell
# 先隐藏起服务端（见 docs/testing/playtest.md 的环境段）
powershell -NoProfile -ExecutionPolicy Bypass -File tools/dev/serve.ps1 -Port 7006 -GmAccounts playtest
python tools/stress/stress.py --server 127.0.0.1:7006 --clients 100 --seconds 60
python tools/stress/stress.py --server 127.0.0.1:7006 --clients 0 --seconds 60   # 空闲基线
```

结果 JSON：`target/stress-0.json` / `stress-100.json` / `stress-300.json`（含
`host_cpu_percent_avg/peak`、`packets_received`、`db_saves_delta`、`autosaves_delta`）。

判据：`--max-tick-p99 60`（硬判据，超了 exit 1）/ `--warn-tick-p99 50`（只告警）。

## 2. 本机基线（第 13 轮，兵部/刑部两次测量）

**测量条件（重要）**：必须在**空载机器**上测——`LoadPercentage ≲ 10%`，且没有其他
`cargo`/`rustc`/`mir-server` 在跑；否则 p99 会被放大数倍。每档全新服务端、60s；
`--clients 0` 空闲档也须跑满 60s。

环境：dev profile + 真实 `Server.MirDB`（比奇 1922 怪 / 46 NPC）+ 内存存储；tick 100ms；
`MIR2_AUTOSAVE_SECS=30`；每连接 4 pkt/s。

| 场景 | tick p99 | 说明 |
|---|---|---|
| **空闲**（`--clients 0`） | **0.24 ~ 1.8 ms** | 随机器负载变化；0.244 ms 是 t100 静默窗口值 |
| **100 连接，默认 mix（2:2:1）** | **3.9 ~ 11.7 ms** | 两次测量区间 |
| **300 连接，默认 mix（2:2:1）** | **14 ~ 17 ms** | 优化前 **95.6 ms** |
| **300 连接，`--mix 1:0:0`**（只走动） | **14 ~ 17 ms** | 优化前 **1054 ms** |
| **300 连接，`--spread`** | **11.6 ~ 16.4 ms** | 各客户端走不同方向 |

三档均 **100% 握手 + 100% 进世界、0 断线、0 限流拒绝、0 丢包** → 连接/会话层不是瓶颈。

## 3. 结论与口径

* **写聚合/AOI 合批已落地（`6351d5d`）**：300 同图 p99 从 95.6ms（默认 mix）/1054ms（只走动）
  降到 **14 ~ 17ms**，目标（60/80ms）达成；
* **100 人 p99 3.9 ~ 11.7ms**，低于「<30ms」目标；
* **空闲 p99 0.24 ~ 1.8ms**，随机器负载变化；0.244ms 是 t100 静默窗口值；
* **必须空载测量**：机器被占用时同一场景 p99 会放大数倍，区间会漂；
* **口径**：`/metrics` 的 tick 统计是最近 **512 次** tick 的环形窗口（100ms tick ≈ 51s），
  压测必须跑满 60 秒；
* 这是**本机 dev profile** 基线，不是 release；对外表述请带「dev profile / 单机 / 同图 300 人」。

### 与旧基线的对比（仅数量级参考）

| 场景 | t93（t95 之前） | t100/13 轮区间 |
|---|---|---|
| 空闲 p99 | 46.6 ms | 0.24 ~ 1.8 ms |
| 100 人 p99 | 56.9 ms | 3.9 ~ 11.7 ms |
| 300 人默认 mix p99 | 107 ms | 14 ~ 17 ms（优化前 95.6 ms） |
| 300 人只走动 p99 | — | 14 ~ 17 ms（优化前 1054 ms） |

不同时段/负载下测的，只能看数量级，不能逐点比较。

## 4. 工具说明

* **Windows 控制台编码**：脚本启动把 `stdout/stderr` 切到 `utf-8, errors=replace`，所有输出
  走 `_emit()` 兜底——默认 GBK 控制台下打印 `µs` 不再 `UnicodeEncodeError` 崩掉（刑部 F3）；
* **`--clients 0`**：空闲基线档，不建连接，只采 `/metrics` 与进程指标；必须跑满 60s
  （512 tick 环形窗口 ≈51s）；
* **`--mix walk:attack:chat`** 与 `--spread`：前者改走动/攻击/聊天比例（`1:0:0` 只走动），
  后者让每个客户端走不同方向，用于区分「挤出生点」与「散开」的 AOI 差异；
* **整机 CPU** 用 `GetSystemTimes` 采样：`Win32_Processor.LoadPercentage` 在 32 核机器上
  抖动太大，不能当「机器空闲」证据；JSON 看 `host_cpu_percent_avg/peak`，并用空闲档
  tick p50 作为「机器确实空闲」的硬证据；
* 压测服务端一律用 `tools/dev/serve.ps1` 隐藏启动、跑完 `-Stop`，避免残留进程与弹窗。
