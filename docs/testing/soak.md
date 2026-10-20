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

## 长跑趋势（4h · 第 21 轮 JJC-20261020-008）

4 小时长跑（`--duration 14400`，100 连接，10s 采样）额外要求**趋势斜率**，并把中间判决落成
**8 份 chunk**（每 1800s 一份）。判据与短跑相同的部分不再重复，这里只讲增量：

| 项 | 判据 | 说明 |
|---|---|---|
| RSS 斜率 | **≤2 MB/h** | 对 `t`（小时）与 `rss_bytes` 做最小二乘回归 |
| 句柄斜率 | **≤2 个/h** | 同上，`handles` |
| 时长自证 | `requested ≥ 14400` 且 `elapsed ≥ requested` | `verdict.json` 里 `duration_ok: true` |
| 采样点 | `≥ requested / sample_interval`（4h@10s → **1440**） | 由请求时长派生，不写死 |
| 反作弊 | `--duration < 14400` → **exit 2** | 要短跑必须 `--allow-short`，产物带 `-short` 后缀且 `meta.short_run = true` |
| chunk fail-fast | 每 1800s 一份 `verdict-chunk-<n>.json` | **任一 FAIL → 立即 exit 1**，写 `"verdict": "FAIL_EARLY"`，**已产出 chunk 全部保留** |

### 两个 RSS 窗口口径（**复算时别混**）

第 20 轮踩过一次「基线口径不一致 → 同一份数据算出 +7.06% 与 +4.16%」的坑，这里把两个窗口写死：

| 用途 | 窗口 | 计算方式 | 本机实测（第 21 轮 chunk-1 / 4h 总） |
|---|---|---|---|
| **chunk 区间判据** | 该 chunk 的 1800s 区间内，且 `t ≥ warmup(180s)` | `(区间内 max - 区间内首点) / 区间内首点` | chunk-1：39.1 → 40.3MB = **+3.13%** |
| **4h 总判据** | `t ≥ slope_skip_seconds(1800s)` 起，到结束 | `(全程 max - 该窗口首点) / 该窗口首点` | 结束时回填 |
| **4h 总斜率** | `t ≥ slope_skip_seconds(1800s)` 起 | 对 `t`(小时) 与 `rss_bytes` 做最小二乘回归 | 结束时回填（`slope_window_seconds` 写进 verdict） |

> 结论：**区间增长看 chunk，总增长/总斜率看跳过前 30 分钟的窗口**；两个数字都对，只是窗口不同。
> `verdict.json` 的 `stats.slope_window_seconds` 会明确写出实际参与回归的区间（自检里是 `[1807, 14400]`），
> 复算时以它为准。

### 连接数口径（`clients_requested` / `clients_entered` / `active_min`）

第 21 轮实测：服务端 **accepted 100、entered 99、rejected 0**（1 个虚拟客户端握手后自行关闭，
服务端日志只有 `连接关闭 session=85`，没有拒绝/踢人记录）。所以连接判据拆成四段：

| 判据 | 要求 | 说明 |
|---|---|---|
| `clients_requested` | **== 100** | 脚本参数 |
| `clients_entered` | **== 100** | 100 个客户端都进入世界 |
| `active_min`（逐点最小 `connections_active`） | **≥ 95** | 允许偶发掉线，但不许「掉一半还在跑」 |
| `connections_rejected` | **== 0** | 服务端没有拒绝连接（拒绝 ≠ 客户端主动退出） |

verdict 的 `stats` 里同时给这三个值和 `active_max`；`samples.json` 有逐点 `connections_active` 序列。

### 运行记录与「多久跑一次 / 什么条件必须重跑」

**运行记录**（每次都要写：时间戳 / commit / 环境 / 结果或作废根因 / 原始数据路径）：

| # | 时间戳（本地 +08:00） | commit | 环境 | 结果 | 作废根因 / 说明 | 原始数据 |
|---|---|---|---|---|---|---|
| 1 | 2026-10-20 18:13:03 → 18:43 | `9d7dad1` 前一轮树 | release + nas:3001 + 100 连接 | **作废**（+1807s，FAIL_EARLY） | 未跑到的 chunk 被当失败（chunk 2..8 空窗口 `采样非空 FAIL`） | `soak-data/20261020-181303-aborted-by-chunk-bug/` |
| 2 | 2026-10-20 18:47:56 → 19:19 | 同上 | 同上 | **作废**（+1810s，FAIL_EARLY） | 当前**未跑满**的窗口被评（只有 1 点）；且只落盘最后一个 chunk | `soak-data/20261020-184756-aborted-by-chunk-incomplete/` |
| 3 | 2026-10-20 19:21:15 → 20:00 | 同上 | 同上 | **作废**（+3600s，chunk-2 FAIL） | 整跑级判据逐窗口判（粒度错配）：句柄 373↔380、池 2→3、p99 连续 7 点 | `soak-data/20261020-192115-aborted-by-chunk-strict-criteria/` |
| 4 | 2026-10-20 20:29:04 → 22:29 | 同上 | 同上 | **PARTIAL**（+7200s，chunk-4 FAIL） | **修复后验证**：真实单步台阶 RSS +5.2MB@6154s（其后零增长），判据未区分台阶/斜坡 | `soak-data/20261020-202904/`（**缺 `samples.json`，见「已知缺口」**） |
| 5 | 2026-10-20 16:21:48 → 16:51 | `5e97160` 后 | 同上（1800s 短跑） | **PASS**（历史基线） | 第 20 轮 1800s 短跑，用于校准判据 | `soak-data/20261020-162148/` |
| 6 | 2026-10-20 15:48:02 → 16:18 | `5e97160` 后 | 同上 | **作废**（RSS/句柄全 `None`） | 服务端直起无 pid 文件，探针拿不到进程 | `soak-data/20261020-154802/` |

**多久跑一次 / 什么条件必须重跑**：

| 触发条件 | 动作 |
|---|---|
| **发版前（每次 release tag 之前）** | 必跑 `--duration 14400` 完整 4h；`duration_ok=true` 且 8 份 chunk 全 PASS 才允许发版 |
| **改了服务端热路径**（`world.rs` tick / `monster.rs` AI / `session.rs` 收发 / `pg_store.rs` 落盘） | 必跑 4h；改的是同一批文件但只动注释/文档时可只跑 `--duration 1800` 并在回执注明 |
| **改了 `soak.py` 判据本身** | 先跑 `--self-check`（10 个用例必须全绿）+ `--duration 600 --allow-short` 冒烟，再排 4h；**判据改动未自检不得排长跑** |
| **依赖/运行时升级**（rustc、sqlx、tokio 大版本） | 必跑 4h，并在运行记录里写升级前后的 commit |
| **线上/验收环境报告掉帧或内存增长** | 用同一 commit 复现 4h；若复现则按「台阶 vs 斜坡」表判定是泄漏还是台阶 |
| **距上次成功 4h 超过 30 天** | 排一次 4h（判据可能已经随代码漂移） |

> 原则：**判据没验证过就不排长跑；长跑失败先修工具，不重试同一个脚本**（第 21 轮就是靠这条避免了第 5 次盲跑）。

### 早退字段修复（第 22 轮，刑部 medium）

第 21 轮的 `FAIL_EARLY` verdict 里出现过自相矛盾：`clients_entered = 0` 却 `active_min = 100`
（早退路径没汇总「进入世界」人数，却被写成 0；`active_min` 又照常从采样算）。

**修复前后对比**（用夹具跑同一份「早退」数据）：

| | `stats.clients_entered` | `stats.active_min` | 一致性检查 |
|---|---|---|---|
| **修复前** | `0` | `100.0` | ❌ `clients_entered=0 却 active_min=100` |
| **修复后（早退未采集）** | `None` | `None` | ✅ 空（两字段一起标未采集） |
| **修复后（早退已汇总 entered=99）** | `99` | `100.0` | ✅ 空，且正常判 `clients_entered == 100` FAIL |

改动点：
1. 早退（`FAIL_EARLY`）与中止（服务端不可达）两条路径，**评估前**回填 `meta["clients_entered"] = 已进入世界人数`；
2. 若确实无法汇总 → `clients_entered` 与 `active_min`/`active_max` **一起写 `None`**，判据里标「未采集」而不是判 FAIL；
3. 新增 `client_fields_consistency(stats)`：显式抓「`clients_entered=0` + `active_min>0`」；
4. **自检第 10 用例**：构造「chunk-2 FAIL 早退」的三种数据（已回填 / 未采集 / 旧 bug），断言前两者无矛盾、且校验器能抓到旧 bug：
```
[早退字段] 已回填：entered=100 active_min=100.0 矛盾=[]
[早退字段] 未采集：entered=None active_min=None 矛盾=[]
[早退字段] 校验器对旧 bug 数据：['clients_entered=0 却 active_min=100：早退时两字段必须一起标未采集']（期望非空）
```

### `--self-check` 的 16 个用例（每个判据都有自己的反例）

| # | 用例 | 构造的坏数据 | 期望 | 实际命中判据 |
|---|---|---|---|---|
| 0 | 对照：健康 4h | 无（p50 3.5ms、p99 锯齿、RSS 平、句柄平、池 5） | **PASS** | — |
| 1 | RSS 翻倍 | 预热后 RSS 线性涨到 2× | FAIL | `RSS 增长 ≤10%`（+99.88%）+ `趋势：RSS 斜率 ≤2MB/h`（+30.38MB/h） |
| 2 | p99 单调上升 | 每个采样点 p99 +0.02ms | FAIL | `后 1/4 ≤ 前 1/4 ×1.2`（1.898）+ `无连续 6 点上升`（1432 点） |
| 3 | 单点 p99=90ms | 一个采样点 p99=90 | FAIL | `绝对：每点 p99 ≤80ms`（相对判据全绿，只有绝对阈值能抓） |
| 4 | 时长造假 | `requested=14400` 但 `elapsed=1800`、样本 180 | FAIL | `elapsed ≥ requested`（−12600s）+ `采样点 ≥1440` |
| 5 | 第 3 个 chunk FAIL | 8 个 chunk 中第 3 个塞 p99=90 | **FAIL_EARLY** | chunk 门禁 → `FAIL_EARLY`，首个失败 chunk=3 |
| 6 | 增量安全 | 只跑 1 个 chunk（181 点） | PASS | 未跑到的窗口不算失败 |
| 7 | 逐边界模拟 | 8 个边界逐个用 `require_complete` 评 | 全部通过 | 实时路径与窗口完成度一致 |
| 8 | 事后校验 | 篡改 chunk-3 的实时结论为 PASS | 抓到不一致 | `crosscheck.ok=False`，列出 mismatch |
| 9 | 抗噪 | 句柄波动 7 / 池 2→3 / p99 连续 7 点 | **PASS** | chunk 门禁只看绝对延迟 + 区间 RSS（粒度错配的回归） |
| 10 | 早退字段 + 落盘 | chunk-2 早退的三种 meta（已回填/未采集/旧 bug） | 前两者无矛盾、旧 bug 被抓；且 `samples.json`/`.csv` 存在且点数一致 | `client_fields_consistency` + `write_samples` |
| 11 | 台阶**持续**高位 | 基线压平 + t=10800 起 +5.5MB 常驻 | **WARN 1 条、verdict 仍 PASS** | `detect_rss_steps`（仅观察，不参与 FAIL） |
| 12 | 涨后回落 | 短暂 +8MB 后 600s 内回落 | **0 条 WARN、verdict PASS** | 同上（防 GC/分配器归还导致的告警疲劳） |
| 13 | chunk-1 预热块爬坡 | 前 30 分钟 RSS 线性 +15% | **PASS**（只记 `observe_only` WARN） | chunk-1 的区间 RSS 判据是**仅观察**（整跑判据本来就跳过前 1800s） |
| 14 | chunk-2 硬判 | 第 2 段 RSS 30 分钟 +10MB | **FAIL** | 证明软化**只在 chunk-1**，不是全局放水 |
| 15 | 锯齿抗噪 | chunk 内 44→36→49 摆动、中位数平稳、末点≈首点 | **PASS** | 噪声地板（8MB）让判据高于测量噪声 |
| 16 | 真泄漏仍红 | 30 分钟单调 +20MB | **FAIL** | 加了地板仍能抓真泄漏 |

跑法：`py tools/stress/soak.py --self-check`（退出码 **1 属预期**：坏样本本该判 FAIL，这里把「会红」当证据）。

### chunk-1 是「预热块」：区间 RSS 只观察（第 23 轮第 7 项）

第 23 轮起跑后 **30 分钟就 FAIL_EARLY**（客户端按设计退出，不是崩溃）：
```
verdict-chunk-1.json: FAIL
   FAIL RSS 增长 ≤10%（区间） — 区间基线 44.6MB 峰值 51.6MB 增长=+15.64%
   PASS 绝对 p50 ≤30ms — max 0.578ms
   PASS 绝对 p99 ≤80ms — max 1.064ms   | 句柄波动 3 | DB 池 5/5
```
服务端**从起服到起跑只隔 2 分钟**，这 30 分钟里含地图加载、1922 只怪刷出、100 个玩家陆续进场 ——
**是启动爬坡，不是泄漏**（延迟/句柄/池全部正常）。实测即使把预热窗口挪到 600s，段内仍 +13.6%，
说明**挪阈值解决不了问题**（爬坡持续到约 17 分钟）。

**修法（原则化，不是调阈值）**：

| 判据 | 窗口 | 处理 |
|---|---|---|
| chunk-1 区间 RSS 增长 | [0s, 1800s) | **仅观察**（`observe_only: true` + `observe_warnings[]`），不参与 FAIL |
| chunk 2..8 区间 RSS 增长 | 各 1800s | **硬判 ≤10%**（此时已稳态，泄漏会立刻暴露） |
| 整跑 RSS 增长 / 斜率 | `t ≥ 1800s`（`slope_skip_seconds`） | **硬判**（不软化） |

理由：整跑判据本来就跳过前 1800 秒（正好等于 chunk-1 的长度），那 chunk-1 的 RSS 自然不该当硬判据。
自检第 13/14 例锁定这个边界：**chunk-1 +15% → PASS（WARN）、chunk-2 +15% → FAIL**。

### 慢泄漏观测：`mir2_internal_records{kind=...}`（第 24 轮）

RSS 判据对「每天 +4.6MB」这类慢增长**不敏感**（4h 只涨 ~800KB，淹没在噪声里）。
所以把内部集合的**长度**单独做成 gauge，与 RSS 对照看：

```
mir2_internal_records{kind="db_save"} 1024
mir2_internal_records{kind="mail_writes"} 0
mir2_internal_records{kind="pending_hits"} 3
mir2_internal_records{kind="pending_map_moves"} 0
mir2_internal_records{kind="market_searches"} 100
```
`kind` 是**编译期固定**的 5 项（`metrics::INTERNAL_COLLECTION_KINDS`），禁止动态 label 污染基数。
每个集合的「有界/无界、上限、代码位置、证据」见 [`docs/architecture/internal-collections.md`](../architecture/internal-collections.md)。

**soak 侧**（`samples.json` / `samples.csv`，每采样点一行）：
`internal_db_save`、`internal_mail_writes`、`internal_pending_hits`、
`internal_pending_map_moves`、`internal_market_searches`。

**判据（与 RSS 对照，不替代既有阈值）**：

| gauge | RSS | 判读 | 动作 |
|---|---|---|---|
| 斜率 ≈ 0 | 平 | 正常 | — |
| 线性上涨，**到上限收敛** | 平 | 有界缓冲（如 `db_save → 1024`） | 记录上限即正常 |
| 线性上涨 | 平 | 缓冲在涨但被回收 | 观察是否收敛 |
| **线性上涨** | **同步涨**（>2MB/h） | **真泄漏** | 提 P0，定位该集合清理路径 |

**不改既有阈值**：RSS ≤10%、p50 ≤30ms、p99 ≤80ms、时长 ≥14400s、chunk 判据全部保持原样。

**复算（PowerShell 7，无 heredoc）**：
```powershell
py -c "import json,statistics;d=json.load(open(r'docs/testing/soak-data/<ts>/samples.json',encoding='utf-8'));s=d['samples'];keys=['internal_db_save','internal_mail_writes','internal_pending_hits','internal_pending_map_moves','internal_market_searches'];sl=lambda k:(lambda v:(round(sum((a-statistics.fmean([x for x,_ in v]))*(b-statistics.fmean([y for _,y in v])) for a,b in v)/sum((a-statistics.fmean([x for x,_ in v]))**2 for a,_ in v),4) if len(v)>1 else None))([(x['t']/3600.0,x[k]) for x in s if isinstance(x.get(k),(int,float))]);print({k:sl(k) for k in keys})"
```

> 这些字段是第 24 轮才加的：更早的 `samples.json` 里没有，复算得到 `None` 属正常。

### chunk 区间 RSS：为什么要有 8MB 绝对地板（第 23 轮第 8 项）

第 23 轮第 2 次跑到 **chunk-2 被判 +10.06% FAIL**（差 **0.06 个百分点**），但从 `samples.json` 复算：

| 统计 | chunk-1 | chunk-2 |
|---|---|---|
| 首点 / 中位数 / p90 / 峰值 / 末点 | 37.1 / 44.3 / 44.7 / 44.8 / 44.8MB | 44.8 / **37.8** / 49.3 / 49.3 / 49.3MB |
| 段内形态 | 平滑爬升 | **锯齿**：44.8 → 44.9 → **35.8** → 35.8 → 38.5 → … → 49.3 |

**chunk-2 的中位数比 chunk-1 还低 6.5MB、累计斜率 −0.866MB/h（RSS 在降）**，p99 1.15ms、句柄与池正常
→ 这不是泄漏，是 **Windows working set trimming** 造成的 ±7–9MB 锯齿。

而 10% 判据在 ~40MB 基线下只有 **4.7MB**，**低于测量噪声**，于是"首点 vs 峰值"的算法被抖动支配（差 0.06pp 就翻红）。

**修法（保持硬判，不做全局放水）**：

```
chunk 区间 RSS：FAIL 当且仅当  growth_bytes > max(10% × 基线, 8MB)
```

| 样本 | 增长 | 判据 | 结果 |
|---|---|---|---|
| 真实 chunk-2（锯齿） | 4.73MB | max(4.69MB, 8MB) = 8MB | **PASS**（复算验证：chunk-1/chunk-2 均 PASS，`首个失败 chunk = None`） |
| 自检 15（锯齿 44→36→49） | — | 8MB 地板 | **PASS** |
| 自检 16（30 分钟单调 +20MB） | 20MB | 8MB | **FAIL**（真泄漏仍能抓） |

**整跑判据一个字没改**：RSS ≤10%、斜率 ≤2MB/h、句柄、池、连续上升、p99 四分位全部保持原样 ——
那些是 4 小时尺度，不受 30 分钟锯齿影响。chunk-1 仍**仅观察**。

> 原则：**判据阈值必须高于测量噪声**。否则"工具会红"就退化成"工具会随机红"，比不测更糟。

### 环境噪声记录（长跑期间他人活动）

长跑要求「空载机器」，但共享开发机上总有人干活。**诚实记录噪声比假装干净更有用**：

| 时间窗（本地 +08:00） | 活动 | 是否跑 cargo | 预期影响 |
|---|---|---|---|
| 18:12:04 – 18:14:35 | 礼部起了一个 debug 服务端（端口 **7031**，无数据库，加载 1922 怪）实测启动/停止/日志命令；18:14:35 用 `serve.ps1 -Stop` **精确停**（未按进程名杀） | 否 | 极小：不同端口、独立进程；仅对 9090 做过一次只读 `/metrics` 抓取 |
| 23:37:41 – 23:39 | 户部为了判断『他人是否修完』跑了一次 `cargo clippy --workspace --all-targets`（**已退出**，非常驻） | **是**（一次 workspace clippy） | 无（此时没有 soak 在跑）。**教训**：非队长安排不自跑 workspace 级编译；有静默窗口公告时一律不编译；判断状态用 `Get-Process`/`git status --porcelain`（零成本），不要用分钟级编译去轮询 |

> 噪声窗口与 soak 起跑（18:13:03）重叠约 1.5 分钟；该区间内的采样点仍全部合规（见 chunk-1）。

### 时间基已核（避免 UTC 二次换算）

- `samples.json` 的 `timestamp` 用 `time.strftime("%Y-%m-%dT%H:%M:%S")` —— **本地时间**，
  与 `PowerShell Get-Date` 逐秒一致（实测 `2026-10-20T18:25:47` == `18:25:47`），**没有**再做一次时区换算；
- `t` 是**单调递增**的「相对起跑秒数」（`time.time() - started`），与 `timestamp` 同源；
- 复核命令：
  ```powershell
  py -c "import json;d=json.load(open(r'docs/testing/soak-data/<ts>/samples.json',encoding='utf-8'));t=[s['t'] for s in d['samples']];print('单调', all(b>a for a,b in zip(t,t[1:])), t[0], t[-1])"
  ```

### chunk 门禁的「最小充分集」（第 21 轮第 4 条修复）

chunk（1800s）**只判两件事**：每个采样点的绝对延迟（p50≤30ms / p99≤80ms）+ **区间 RSS 增长 ≤10%**。
句柄波动、DB 池、连续上升、趋势斜率**只在整跑（4h）判**——它们的抖动必须长窗口才能与噪声区分。

> 第 3 次真跑就是被「整跑判据逐窗口判」误杀的：同一份数据里句柄 373↔380、池 2→3、p99 有 7 点连续上升，
> 拿来判 30 分钟窗口必然 FAIL。**修法不是放宽阈值，是把判据放到正确的窗口上。**
> 自检第 9 例「抗噪」就是这条的回归：造出上述"噪声三连"，chunk 门禁必须 PASS。

### 台阶 vs 斜坡（第 21 轮第 5 项，待修）

第 4 次真跑在 **t≈6154s** 抓到 **RSS 41.4 → 46.6 MB 的单步跳变（+5.2MB / +12.56%）**，其后 **~1000s 零增长**：

| 时刻 | RSS | 说明 |
|---|---|---|
| 0–6000s | 38.0 → 41.4 MB | 100 分钟共 +3.4MB（≈2.0MB/h），**斜坡** |
| **6154s（≈22:11:34）** | **41.4 → 46.6 MB** | **一个 10s 采样内的台阶**（720 个采样里只有这一次 >0.4MB） |
| 6154s–7200s | 46.6 MB 持平 | **零增长**，p99 仍 0.89ms、句柄仍 374 |
| 服务端日志 | `WARN=0 / ERROR=0` | 只有正常 autosave（`db_save_p95 30–44ms`） |

**判读**：这是**一次性大块分配后常驻**，不是「每 tick/每请求」的泄漏（后者是持续斜坡）。
当前判据把「区间增长 >10%」一律判 FAIL，于是它把台阶和斜坡混为一谈 —— **这是判据缺陷，不是误报**（台阶本身是真实可测的）。

第 22 轮拟定的修法（**本轮未实施**，避免中途改脚本导致"数据与判据不同版本"）：

| 情形 | 处理 |
|---|---|
| 单步 >5MB **且** 其后 600s 增长 ≤0.5MB | 记入 `warnings[]`，**不 FAIL** |
| 区间 RSS 增长 >10% **且** 窗口内至少 3 个 300s 分段都在涨 | **FAIL**（持续增长才是泄漏） |
| 任何 `warnings` | 必须出现在 `verdict.json`，且整跑结束时再报一次（不许静默吞掉） |

### 工具可信度时间线（第 21 轮，4 次起跑）

| 次 | 起跑 | 结束 | 结果 | 根因 | 改动点 | 回归用例 |
|---|---|---|---|---|---|---|
| 1 | 18:13:03 | 18:43（+1807s） | **作废**（FAIL_EARLY） | 未跑到的 chunk 被当失败：第一个边界就把 chunk 2..8 的空窗口评成 `采样非空 FAIL` | `evaluate_chunks(elapsed=)` 只评已覆盖窗口；空窗口跳过 | 自检第 6 例「只跑 1 个 chunk 不许误判」 |
| 2 | 18:47:56 | 19:19（+1810s） | **作废**（FAIL_EARLY） | 当前**未跑满**的窗口被评：只有 1 个采样点 → `预热后样本≥3` 失败；且只落盘最后一个 chunk | `require_complete=True`；触发时把本轮新完成的所有 chunk 逐个落盘 | 自检第 7 例「逐边界模拟」 |
| 3 | 19:21:15 | 20:00（+3600s） | **作废**（chunk-2 FAIL） | **整跑级判据逐窗口判**（粒度错配）：句柄 373↔380=7、池 2→3、p99 连续上升 7 点 | chunk 门禁改为最小充分集（绝对延迟 + 区间 RSS） | 自检第 9 例「抗噪」 |
| 4 | 20:29:04 | 22:29（+7200s） | **PARTIAL**（chunk-4 FAIL） | 真实台阶：RSS 单步 +5.2MB@6154s，其后零增长 → 判据未区分「台阶/斜坡」 | 本轮**不改**（纪律）；第 22 轮按上表实施 | 待补：台阶 WARN + 持续增长 FAIL 双用例 |

> 这张表比「最终 PASS」更能说明工具是怎么被信任起来的：**每一次事故都留下一条回归用例**（9 个自检案例）。

### 已知缺口：fail-fast 路径没有落原始采样（第 6 项，待修）

`docs/testing/soak-data/20261020-202904/` 里**只有** `verdict-chunk-1..4.json` 与 `verdict.json`，
**没有 `samples.json` / `samples.csv`** —— 因为 `FAIL_EARLY` 是在采样循环里直接 `return` 的，跳过了收尾落盘：

| 文件 | 是否存在 |
|---|---|
| `verdict.json`（含 `FAIL_EARLY`） | ✅ |
| `verdict-chunk-1..4.json` | ✅ |
| `samples.json` / `samples.csv`（720 个采样点） | ❌ **缺失** |

后果：**队长与刑部无法用原始数据独立复算那个 +5.2MB 台阶**，只能看文档叙述。这违反「失败也是数据」的原则。
第 22 轮修法：`FAIL_EARLY` 退出前必须落 `samples{suffix}.json` / `.csv`（**已采到的全部点**）+ `verdict.json`，
并加自检用例：**构造 chunk-2 FAIL → 断言 `samples.json` 存在且 `len(samples) == 已采点数`**。

> 本轮可部分补偿：`target/soak4h-20261020-202904.log` 的逐行日志里保留了全部 720 个采样点（`t=`/`p50`/`p99`/`rss`/`handles`/`pool`），
> 台阶时刻与前后值已在上表逐条列出；但日志不是机器可复算的产物，**不能替代 `samples.json`**。

## 怎么复算文档里的数字

> **本仓文档里的命令一律按 PowerShell 7 写（`pwsh -NoProfile`），不支持 bash heredoc（`py - <<'PY'` 在 pwsh 里跑不了）。**

**方式 A（推荐）：用脚本自带的 `--analyze` 重算**
```powershell
py tools/stress/soak.py --analyze docs/testing/soak-data/<时间戳>/samples.json
```
它会用**同一套判据**重算：绝对阈值、相对判据、趋势斜率、8 个 chunk，并写出
`verdict-recomputed.json`（与实时 `verdict.json` 对照），最后打印 `slope_window_seconds`。

**方式 B：一条命令打印关键数字**
```powershell
py -c "import json;d=json.load(open(r'docs/testing/soak-data/<时间戳>/samples.json',encoding='utf-8'));s,m=d['samples'],d['meta'];w=[x for x in s if x['t']>=m['warmup_seconds']];q=max(1,len(w)//4);p=[x['tick_p99_ms'] for x in w];import statistics;print('elapsed',m['elapsed_seconds'],'samples',len(s),'warm',len(w));print('p50max',max(x['tick_p50_ms'] for x in s),'p99max',max(x['tick_p99_ms'] for x in s));print('rss first/max',w[0]['rss_bytes'],max(x['rss_bytes'] for x in w),'growth%',round((max(x['rss_bytes'] for x in w)-w[0]['rss_bytes'])/w[0]['rss_bytes']*100,2));print('handles swing',max(x['handles'] for x in w)-min(x['handles'] for x in w));print('dbpool first/max',w[0]['db_pool_connections'],max(x['db_pool_connections'] for x in w));print('p99 head/tail',statistics.fmean(p[:q]),statistics.fmean(p[-q:]));print('write_failed',s[-1]['write_failed_total'],'dropped',s[-1]['packets_dropped_total'])"
```

**方式 C：只看结论**
`verdict.json` 里每条判据都有 `ok` 与 `detail`（含具体数字），`warnings[]` 是台阶类告警（仅观察、不参与 FAIL）。

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
