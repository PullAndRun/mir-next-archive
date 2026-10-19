# 压测（`tools/stress/stress.py`，第 9 轮）

用途：用真实协议、真实服务端拉 N 个并发连接，测握手/进世界成功率、tick 延迟、CPU/RSS、
DB 写次数与限流/丢包，作为容量基线。脚本纯 stdlib，协议复用 `tools/playtest/playtest.py`；
用法与参数见 [`tools/stress/README.md`](../../tools/stress/README.md)。

## 1. 复现

```powershell
# 先隐藏起服务端（见 docs/testing/playtest.md 的环境段）
powershell -NoProfile -ExecutionPolicy Bypass -File tools/dev/serve.ps1 -Port 7006 -GmAccounts playtest
python tools/stress/stress.py --server 127.0.0.1:7006 --connections 100 --seconds 60
```

结果 JSON：`target/stress-100.json`、`target/stress-300.json`（20 连接冒烟为
`target/stress-smoke.json`）。

## 2. 本机基线（dev profile，单机）

环境：dev profile + 真实 `Server.MirDB`（比奇 1922 只怪 / 46 NPC）+ 内存存储（未配
`DATABASE_URL`）；每连接 4 pkt/s（走动:攻击:聊天 = 2:2:1）；tick 100ms；`MIR2_AUTOSAVE_SECS=30`。

| 连接数 | 进世界成功率 | tick p50 | tick p95 | tick p99 | tick max | CPU 均值 | RSS 峰值 | DB 写次数 | DB 写 p95 | 限流拒绝 | 丢包 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 0（空闲基线） | - | 0.37 ms | 14.0 ms | 46.6 ms | 62.9 ms | - | 26.3 MiB | 0 | - | 0 | 0 |
| 20（冒烟） | 100% | 0.42 ms | 38.7 ms | 67.3 ms | 189.8 ms | 9.9% | 35.5 MiB | 60 | 1.69 ms | 0 | 0 |
| **100** | 100% | 0.81 ms | 22.5 ms | **56.9 ms** | 68.6 ms | 126.7% | 47.3 MiB | 200 | 12.97 ms | 0 | 0 |
| **300** | 100% | 7.60 ms | 67.5 ms | **107.1 ms** | 197.2 ms | 1069.1% | 97.1 MiB | 651 | 1.34 ms | 0 | 0 |

## 3. 口径说明（避免误读）

* `/metrics` 的 tick 统计是**最近 512 次 tick** 的环形窗口；60 秒 × 100ms = 600 tick，
  所以 60 秒压测已只覆盖压测窗口；空闲基线同样跑满 60 秒再读；
* **连接/会话处理不是瓶颈**：300 连接 100% 握手 + 进世界、0 断线、0 限流、0 丢包、
  RSS 97 MiB；
* **tick 尾部延迟来自世界 tick 本身**：0 玩家时 p99 已 46.6 ms（比奇 1922 只怪 AI +
  `World::tick()` 单线程），300 同图玩家推到 107 ms，**超过 100ms 预算**；
* 这是**本机 dev profile** 基线，不是 release；对外表述请带上
  「dev profile / 单机 / 同图 300 人」。

## 4. 结论与后续

* 会话层可线性扩到 300 连接（0 失败/0 限流）；
* 世界 tick 是下一阶段的瓶颈：同图 300 人 p99 已超 100ms，需要把怪物 AI/刷怪扫描拆分或
  分批处理（后续任务）；
* 压测服务端一律用 `tools/dev/serve.ps1` 隐藏启动、跑完 `-Stop`，避免残留进程与弹窗。
