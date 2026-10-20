# 长时 soak（稳定性压测）`tools/stress/soak.py`

> 目的：证明服务端在**持续负载**下不漂移——RSS 不涨、句柄不漏、DB 连接不涨、tick 延迟不劣化，
> 且**绝对延迟**始终在可接受范围内。发版前跑一次 1800s。

## 一句话用法

```powershell
# 1) 起服务端（开 /metrics；soak 专用：关掉单 IP 连接上限，否则第 6 个连接就被拒）
$env:MIR2_LISTEN = "127.0.0.1:7000"
$env:MIR2_METRICS_LISTEN = "127.0.0.1:9090"
$env:MIR2_MAX_CONN_PER_IP = "0"      # soak 需要 100 条同源连接
$env:MIR2_MAX_CONN_PER_SEC = "0"
pwsh -NoProfile -File tools/dev/serve.ps1 -Port 7000 -Exe target/release/mir-server.exe

# 2) 跑 1800s（100 连接、10s 采样、走动/攻击/聊天 2:2:1）
py tools/stress/soak.py --duration 1800 --clients 100 --sample-interval 10 \
    --out-dir docs/testing/soak-data/<时间戳>

# 3) 负向自测（合成坏样本必须被判 FAIL；退出码 1 属预期）
py tools/stress/soak.py --self-check
```

退出码：`0` = verdict PASS；`1` = verdict FAIL（含 `--self-check` 的预期失败）；`2` = 参数/环境错误
（比如抓不到 `/metrics`）。

## 规格（脚本自断言，不达标直接非 0 退出）

| 项 | 要求 |
|---|---|
| 时长 | **≥1800s**（指定更短会打 WARN，判决也必定 FAIL —— **不许缩时长**） |
| 连接数 | **== 100** |
| 采样 | 每 **10s** 一点，**≥180** 点 |
| 负载 | 走动/攻击/聊天 mix（默认 `2,2,1`），每客户端 1 包/秒 |

## 判据（相对 + 绝对，缺一不可）

**绝对**（每个采样点都必须满足，任一超限即 FAIL）：

- `p50 ≤ 30ms`
- `p99 ≤ 80ms`

**相对**（只看预热 `180s` 之后的窗口）：

- RSS 增长 **≤10%**。基线口径**必须写清**：`first` = **预热（180s）之后**的第一个采样点，
  不是进程启动时的第一个点 —— 预热期包含地图/NPC/怪物首次加载与 JIT 预热，用启动基线会把
  「初始化开销」误算成「泄漏」（本机实测：启动基线算出来 +7.06%，预热后基线是 +4.16%）。
  判据实现见 `evaluate()` 的 `rss = _series(warm, "rss_bytes")`（`warm` = `t >= 180s` 的样本）。
- 句柄数波动 **≤5**（`max - min`）
- DB 连接数**不增长**（`max(pool) ≤ first(pool)`）
- 后 1/4 窗口 p99 均值 **≤ 前 1/4 ×1.2**
- **无连续 6 个采样点单调上升**（p99 严格递增段 < 6）

数据来源全部是真实观测：tick p50/p99、连接数、**DB 连接池**、`packets_dropped_total`、
`write_failed_total` 来自服务端 `/metrics`；RSS 与句柄数来自操作系统。
「分配速率」用 `alloc_bytes_per_sec = ΔRSS / Δt` 作为**代理指标**记账（按采样点记录，不做判据）。

## 负向自测 `--self-check`（防「恒绿」）

`evaluate()` 是纯函数，`--self-check` 用**合成样本**走同一套判据：

| 案例 | 期望 | 实际命中判据 |
|---|---|---|
| 对照：健康样本 | PASS | — |
| 坏样本 1：RSS 翻倍 | **FAIL** | `RSS 增长 ≤10%`（112.35%） |
| 坏样本 2：p99 单调上升 | **FAIL** | `后 1/4 窗口 p99 ≤ 前 1/4 ×1.2`（ratio 1.993）**且** `无连续 6 点单调上升`（最长 183） |
| 坏样本 3：单点 p99=90ms | **FAIL** | `绝对：每个采样点 p99 ≤80ms`（超限点 1 个） |

> 案例 3 专门验证**绝对阈值真的生效**：它的相对判据全绿，只有绝对阈值能抓住它。
> `--self-check` 以 **1 退出**属预期——坏样本本该判 FAIL，这里把「会红」当证据。

## 实测记录（第 20 轮 · JJC-20261020-007-sub-e9c3f17d）

**判决：PASS**（数据目录 `docs/testing/soak-data/20261020-162148/`）

| 项 | 值 |
|---|---|
| 时间窗（本地 +08:00） | `2026-10-20 16:21:48` → `2026-10-20 16:51:56`（`duration_seconds = 1808.9`） |
| 采样点 | **181**（每 10s；预热 180s 后仍有 163 点） |
| 连接数 | **100 / 100 进入世界**（`mir2_connections_active = 100`，`connections_rejected = 0`） |
| 服务端 | release 构建（`mir-server.exe` 的改名副本 `soak-stability.exe`，PID 48720；改名是为了躲开 `ci.ps1 -KillStale` 的按名清理） |
| 硬件 | AMD Ryzen 9 7945HX（16 物理核 / 32 逻辑核）、63.7 GB RAM、Windows 11 Pro 10.0.26300 |
| 空载确认 | 跑前 `cargo`/`rustc`/`mir-server` 均无残留、整机 CPU 0%；常驻 `rust-analyzer` 跑前 6 秒 CPU 增量 0s（空闲）；本机同时刻无其他压测 |
| **绝对判据** | `p50` 最大 **0.64ms**（阈值 30ms）、`p99` 最大 **1.29ms**（阈值 80ms）—— **181/181 个采样点全部合规** |
| **RSS** | 预热后基线 **44.8 MB → 峰 46.7 MB，增长 +4.16%**（阈值 ≤10%）。单位是 `1 MB = 10^6 字节`；换算成 MiB 是 42.7 → 44.5 MiB（比值相同，`samples.json` 里是原始字节数） |
| **句柄** | 375 ~ 380，波动 **5**（阈值 ≤5，**恰好压线通过**） |
| **DB 连接池** | 首 5 / 峰 5 / 末 2 —— **全程不增长**（收尾阶段客户端断开归池） |
| **p99 前后 1/4** | 前 1.12ms → 后 1.10ms，比值 **0.986**（阈值 ≤1.2） |
| **最长连续上升** | **5** 个采样点（阈值 <6） |
| `/metrics` 终值 | `write_failed_total = 0`、`packets_dropped_total = 0` |
| 客户端异常 | 0（`connections_rejected = 0`，100/100 连接全程在线） |

原始数据：`docs/testing/soak-data/20261020-162148/`（`samples.json` 90KB、`samples.csv` 17KB、`verdict.json`），
已入库；文档里每个数字都能用它复算（见下）。

> 另有一份**被判 FAIL 的历史数据** `docs/testing/soak-data/20261020-154802/`：那次的 p50/p99/DB 池全部合规，
> 但服务端是 `Start-Process` 直起的、没有 pid 文件，导致 RSS/句柄全程为 `None` 而被判 FAIL。
> 保留它是为了说明「判据宁红不假绿」——不是把它当成功证据。

## 怎么复算文档里的数字

```powershell
py - <<'PY'
import json, statistics
d = json.load(open("docs/testing/soak-data/<时间戳>/samples.json", encoding="utf-8"))
s, meta = d["samples"], d["meta"]
warm = [x for x in s if x["t"] >= meta["warmup_seconds"]]
p99 = [x["tick_p99_ms"] for x in warm]
q = max(1, len(p99) // 4)
print("duration", meta["duration_seconds"], "samples", len(s), "warm", len(warm))
print("p50 max", max(x["tick_p50_ms"] for x in s), "p99 max", max(x["tick_p99_ms"] for x in s))
print("rss growth %", (max(x["rss_bytes"] for x in warm) - warm[0]["rss_bytes"]) / warm[0]["rss_bytes"] * 100)
print("handles swing", max(x["handles"] for x in warm) - min(x["handles"] for x in warm))
print("db pool first/max", warm[0]["db_pool_connections"], max(x["db_pool_connections"] for x in warm))
print("p99 head/tail", statistics.fmean(p99[:q]), statistics.fmean(p99[-q:]))
print("write_failed", s[-1]["write_failed_total"], "dropped", s[-1]["packets_dropped_total"])
PY
```

或者直接看 `verdict.json`：里面是每条判据的 `ok` 与 `detail`（含具体数字），与 `summary` 一致。

## 与 `check-routine` 的关系

- `pwsh -NoProfile -File tools/dev/check-routine.ps1 -Soak` 会跑**完整 1800s** 版本；
- **不并入** `-Quick` / `-Full` 的日常路径（30 分钟太慢），文档与脚本里都写明；
- `-Soak` 前置要求：`MIR2_DRILL_DATABASE_URL` 不必设，但**服务端与 `/metrics` 必须先起来**，
  脚本里的步骤会自己校验 `/metrics` 可达。

## 已知边界

- tick p50/p99 取自服务端 `/metrics` 的**滚动窗口**统计（`mir2_tick_micros_p50/p99`），
  不是客户端侧 RTT；客户端侧延迟受本机 Python GIL 影响，不适合当判据。
- 「分配速率」是 RSS 增速代理，不是分配器级数据；它只记账、不参与判决。
- 绝对阈值（30/80ms）**依赖空载机器**：跑前必须确认无 `cargo`/`rustc`/`mir-server` 残留，
  并把硬件与时段写进本文件（否则数字不可比）。
- **服务端 PID**：RSS / 句柄 依赖拿到服务端进程。脚本按 `--pid` → `target/server-<port>.pid`
  （`serve.ps1` 会写）→ `netstat -ano` 反查监听端口 的顺序解析；**直接 `Start-Process` 起的
  服务端没有 pid 文件**，第 1 次实测就因此让 RSS/句柄全程为 `None` 并被判 FAIL（见下「事故记录」），
  所以现在起服要么走 `serve.ps1`，要么显式传 `--pid`。
- **别让 `-KillStale` 打断 soak**：`ci.ps1 -KillStale` 按进程名杀 `mir-server`，会把 soak 的服务端
  一起带走。压测期间请在 `target/soak.lock` 里写窗口（`ci.ps1` 见到锁会拒绝清理），
  或者用**改名副本**起服（例如 `target/soak-run/soak-stability.exe`）。

## 事故记录（第 20 轮，全部已修）

| # | 现象 | 根因 | 处置 |
|---|---|---|---|
| 1 | 首次跑 100 秒后服务端无声消失（无 panic、无 stderr） | 队友执行 `ci.ps1 -KillStale`（按名杀 `mir-server`）误杀 soak 服务端 | 队长在 `ci.ps1` 加 `target/soak.lock` 机制；soak 侧改用改名副本 + 锁 |
| 2 | 第 2 次跑满 1808.7s 但判决 FAIL：`RSS/句柄 缺少数据` | 服务端由 `Start-Process` 直起，没有 pid 文件 → 探针 PID 为空 | `soak.py` 增加 `--pid` + `netstat` 反查；补 90s 冒烟确认 `rss=48.6MB / handles=384` 可采 |
| 3 | 客户端线程异常刷出 2 万行 traceback | `Worker.run` 未捕获 socket 异常 | `QuietWorker` 收进 `error` 字段；服务端不可达 30s 即 `exit 1`（不白跑 29 分钟） |
