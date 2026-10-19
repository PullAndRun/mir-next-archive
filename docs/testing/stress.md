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

## 2. 本机基线（dev profile，单机，2026-10-20 静默窗口）

环境：dev profile + 真实 `Server.MirDB`（比奇 1922 怪 / 46 NPC）+ 内存存储；每连接 4 pkt/s
（走动:攻击:聊天 = 2:2:1）；tick 100ms；每档**全新服务端**、60s。

| 档位 | tick p50 | p95 | p99 | max | 服务端 CPU | 整机 CPU 均值/峰值 | RSS 峰值 | DB 写 | 收到包数 |
|---|---|---|---|---|---|---|---|---|---|
| **0（空闲）** | 0.145 ms | 0.230 ms | **0.244 ms** | 0.35 ms | 0.3% | 16.1% / 24.0% | 26.4 MiB | 0 | 0 |
| **100** | 6.38 ms | 9.24 ms | **14.84 ms** | 18.1 ms | 138.5% | 22.5% / 26.7% | 47.1 MiB | 206 | 1.10 M |
| **300** | 58.8 ms | 183.8 ms | **232.3 ms** | 320 ms | 963% | 55.8% / 67.8% | 108.1 MiB | 628 | 7.46 M |

## 3. 结论与口径

* **空闲 p99 0.244 ms**（t93 时是 46.6 ms）——t95「怪物 AI 分帧 + 空闲短路」生效；
* **100 人 p99 14.8 ms，达标**（目标 <30 ms）；
* **300 人 p99 232 ms，未达标且波动大**：本机历史实测 **75 / 121 / 163 / 232 / 260 ms**
  都出现过，按**区间 ~75–260 ms** 记录，不写单点；根因是**同图视野扇出**（300 人挤出生点，
  收到包数 7.46 M/分钟），留给第 13 轮「写聚合/AOI 合批」；
* 三档均 **100% 握手 + 100% 进世界、0 断线、0 限流拒绝、0 丢包** → 连接/会话层不是瓶颈；
* **口径**：`/metrics` 的 tick 统计是最近 **512 次** tick 的环形窗口（100ms tick ≈ 51s），
  压测必须跑满 60 秒；`--clients 0` 的空闲档也要 60 秒，否则启动期样本会混进 p99；
* 这是**本机 dev profile** 基线，不是 release；对外表述请带「dev profile / 单机 / 同图 300 人」。

### 与 t93 基线的对比（仅数量级参考）

| 档位 | t93（t95 之前） | 现在（t95 之后） |
|---|---|---|
| 空闲 p99 | 46.6 ms | 0.244 ms |
| 100 人 p99 | 56.9 ms | 14.8 ms |
| 300 人 p99 | 107 ms | 75–260 ms（波动） |

t93 那组是另一时段/不同负载下测的，只能看数量级，不能逐点比较。

## 4. 工具说明

* **Windows 控制台编码**：脚本启动把 `stdout/stderr` 切到 `utf-8, errors=replace`，所有输出
  走 `_emit()` 兜底——默认 GBK 控制台下打印 `µs` 不再 `UnicodeEncodeError` 崩掉（刑部 F3）；
* **`--clients 0`**：空闲基线档，不建连接，只采 `/metrics` 与进程指标；必须跑满 60s
  （512 tick 环形窗口 ≈51s）；
* **整机 CPU** 用 `GetSystemTimes` 采样：`Win32_Processor.LoadPercentage` 在 32 核机器上
  抖动太大，不能当「机器空闲」证据；JSON 看 `host_cpu_percent_avg/peak`，并用空闲档
  **tick p50 ≈ 0.145ms** 作为「机器确实空闲」的硬证据；
* 压测服务端一律用 `tools/dev/serve.ps1` 隐藏启动、跑完 `-Stop`，避免残留进程与弹窗。
