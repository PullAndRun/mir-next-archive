# 运维手册（Runbook）

适用范围：Windows x64 上运行《传奇-Next》服务端（`mir-server.exe`）。基线 HEAD
`71cb363`（第 20 轮发布包重打后的版本；第 21 轮编写本手册）。发布包/端口/迁移细节见
[`docs/deploy.md`](../deploy.md)，指标字段完整清单见
[`docs/server/metrics.md`](../server/metrics.md)。

> 本机约定：**不要前台运行 `target\debug\mir-server.exe`**（会弹控制台到工作显示器）。
> 统一用隐藏启动器：`tools/dev/launcher.ps1`（新版，带 PID 文件/身份校验）或
> `tools/dev/serve.ps1`（旧版，`target/server-<端口>.pid`）。
> **压测/soak 期间**（`target/soak.lock` 存在）**禁止** `ci.ps1 -KillStale`，也**不要**
> 按进程名杀 `mir-server`/`soak-stability.exe`。

---

## 1. 快速命令卡

```powershell
# 首次启动（检查 .env/数据库/端口，成功后隐藏启动并写 PID 文件）
pwsh -NoProfile -File tools/dev/launcher.ps1 -FirstRun -Port 7000 -MetricsPort 9090

# 状态 / 停止（只按 PID 文件；停止前校验 exe 路径 + 启动时间 + 命令行）
pwsh -NoProfile -File tools/dev/launcher.ps1 -Status
pwsh -NoProfile -File tools/dev/launcher.ps1 -Stop

# 旧方式（无数据库、快速验证；日志/ PID 都在 target/）
pwsh -NoProfile -ExecutionPolicy Bypass -File tools/dev/serve.ps1 -Port 7031
pwsh -NoProfile -ExecutionPolicy Bypass -File tools/dev/serve.ps1 -Stop -Port 7031

# 本地门禁（不要加 -KillStale 如果 soak.lock 存在）
pwsh -NoProfile -File tools/dev/ci.ps1
pwsh -NoProfile -File tools/dev/ci.ps1 -SkipTests

# 抓指标
Invoke-WebRequest -UseBasicParsing http://127.0.0.1:9090/metrics
```

启动器会生成/校验 `target/mir-next.launcher.pid`（JSON：pid/exe/started/root/port/
metrics_port/client_pid），服务端日志在 `target/launcher-server.log` /
`target/launcher-server.err.log`。

## 2. 启动 / 停止 / 升级 / 回滚

### 2.1 启动

1. 确认没有跑 soak：`Test-Path target\soak.lock` 为 `False`；若为 `True`，先看锁内容，
   **不要杀进程**。
2. `launcher.ps1 -FirstRun -Port <端口> -MetricsPort <指标端口>`：
   - 检查 `.env`（`DATABASE_URL` 或 `MIR2_DEV_DATABASE_URL`）、数据库 TCP 可达、端口空闲；
   - 缺 `config/client.toml` 时生成默认文件；
   - 隐藏启动，写 PID 文件；20 秒内未监听则 `exit 2` 并打印日志路径。
3. 验证：`launcher.ps1 -Status` 应 `运行中：PID …`（exit 0；身份/时间校验见 §5.8）；
   或直接看端口：`Get-NetTCPConnection -State Listen -LocalPort 7000`。

### 2.2 停止

优先 `launcher.ps1 -Stop`；旧方式用 `serve.ps1 -Stop -Port <端口>`。两者都只按 PID 文件停，
不会按进程名误杀。**不要** `Get-Process mir-server | Stop-Process -Force`——soak 期间会
误杀别人的长跑服务端（第 20 轮真实事故，见 §4）。

### 2.3 升级（迁移不可回退）

**升级前必须先备份**。迁移只增不改，一旦新版本跑过迁移，旧版本不能直接跑在新库上。

```powershell
# 1) 备份（未实测：本机 5432 当前不可达，见 §8）
pg_dump --format=custom --file="mir_next_$(Get-Date -Format yyyyMMdd_HHmm).dump" $env:DATABASE_URL

# 2) 停服
pwsh -NoProfile -File tools/dev/launcher.ps1 -Stop

# 3) 替换 dist/ 二进制，保留 resource/、config/
# 4) 启动新版本，观察日志
pwsh -NoProfile -File tools/dev/launcher.ps1 -FirstRun -Port 7000 -MetricsPort 9090
Select-String -Path target\launcher-server.log -Pattern 'migration|failed|panic' -CaseSensitive:$false

# 5) 验证：客户端登录→进世界→背包/行会/邮件；/metrics 的 connections/tick 正常
```

### 2.4 回滚

1. 停新版本；换回旧二进制；
2. **用升级前的 dump 恢复数据库**（`pg_restore --clean --if-exists`）；
3. 启动并验证。没有备份就没有回滚路径——这是第 20 轮发布验收强调「先备份」的原因。

### 2.5 发布包校验

```powershell
pwsh -NoProfile -File tools/dev/package.ps1 -Verify
```

`-Verify` 校验文件数、逐文件 SHA-256、客户端 `--help` 冒烟；删除/篡改任一文件都会
`exit 1` 并打印 `MISSING:`/`TAMPERED:`。发布包生成后若代码又变了，必须**重打**
（第 20 轮真实事故：`dist` 15:33 生成、代码 17:33 冻结，SHA 不一致，最终在 `71cb363` 重打）。

## 3. 备份与恢复

* **数据库**：PostgreSQL 是唯一需要备份的持久化数据（角色/背包/装备/技能/任务/好友/
  行会/邮件/市场/攻城）。每日全量 + 升级前手动；保留 ≥7 天。
* **静态文件**：`config/`（语言包、`client.toml`）与 `resource/` 随版本归档。
* **MemoryStore**：没有可备份数据，重启即丢；生产必须设 `DATABASE_URL`。
* 恢复演练：`tools/dev/migrate_drill.ps1`（只认 `MIR2_DRILL_DATABASE_URL`，库名前缀必须是
  `mir2_drill`，否则拒绝）。**未实测**（本机无 PG，见 §8）。

## 4. 事故表（13 条；标「真实」的来自前 20 轮实际发生或第 21 轮本机实测）

| # | 症状 | 排查命令 | 修复 | 来源 |
|---|---|---|---|---|
| 1 | **soak/长跑中服务端无声消失**（无 panic、无 stderr） | `Get-Content target\soak.lock`；`Get-Process soak-stability,mir-server`；看服务端日志最后 20 行 | 队友跑 `ci.ps1 -KillStale` 按进程名误杀。已在 `ci.ps1` 加 `target/soak.lock`：锁存在时 `-KillStale` **拒绝执行并 exit 2**；soak 侧用改名副本（`soak-stability.exe`） | 真实·第 20 轮（`docs/testing/soak.md` 事故记录 #1；`ci.ps1` 第 66–81 行） |
| 2 | soak 跑满 1808.7s 但判决 FAIL：`RSS/句柄 缺少数据` | `Get-Content docs/testing/soak-data/<时间戳>/verdict.json`；`Get-Content target\server-<端口>.pid` | 服务端由 `Start-Process` 直起、没有 PID 文件，探针拿不到进程。现在要求走 `serve.ps1`/`launcher.ps1`（会写 PID），或给 soak 显式 `--pid` | 真实·第 20 轮（`soak-data/20261020-154802`；soak.md 事故记录 #2） |
| 3 | 发布包 SHA 与代码冻结不一致（15:33 打包 vs 17:33 冻结） | `pwsh tools/dev/package.ps1 -Verify`；`Get-FileHash dist\mir-next-0.2.0-win64\mir-server.exe -Algorithm SHA256`；对照 `manifest.sha256.json` | 代码冻结后**必须重打** dist；本次在 `71cb363` 重打并重新验收 | 真实·第 20 轮验收 low（`71cb363`） |
| 4 | 升级后想回滚，但新库结构旧程序不认 | 确认升级前 dump 存在且非 0 字节；`pg_restore --list <dump>` | 迁移**不可回退**：升级前 `pg_dump`；回滚 = 旧二进制 + 恢复升级前 dump；`docs/deploy.md` §6 已写 | 真实·第 20 轮（`docs/deploy.md` §6） |
| 5 | 本地脚本/CI 在服务端输出后**挂死不返回** | 看 `Get-Process pwsh`；检查启动命令是否 `| Out-Host`/管道捕获子进程 stdout | 管道把子进程 stdout 句柄留在父进程，脚本等不到 EOF。改为 `Start-Process -RedirectStandardOutput <文件>`（`serve.ps1`/`launcher.ps1` 的写法），**不要把服务端输出接管道** | 真实·第 17 轮 F1（收口 `9783098`） |
| 6 | `cargo test --workspace` 先红：真 PG 用例 FAILED（不是 skip） | `echo $env:MIR2_TEST_DATABASE_URL`；`Test-NetConnection 127.0.0.1 -Port 5432` | 真 PG 用例 fail-loud；`ci.ps1` 在**脚本开头**统一设 `MIR2_TEST_DATABASE_URL`（显式环境变量 → `.env` 的 `MIR2_DEV_DATABASE_URL` 换库名 → 本地默认），再跑测试 | 真实·第 19 轮（`ci.ps1` 第 46–64 行） |
| 7 | 攻城 `war_end` 测试偶发 flake | `cargo test -p mir-server --test conquest -- --nocapture` 复跑 | 用真实挂钟判定结束时间，跨秒/负载下抖动；改为**世界时钟投影**（`World::wall_now`） | 真实·第 16 轮（`5e97160`） |
| 8 | 新服务端启动成功，但 `/metrics` 连不上 | 看日志 `Select-String target\launcher-server.log -Pattern 'metrics .*失败'`；`Get-NetTCPConnection -State Listen -LocalPort 9090` | 指标端口被占（`os error 10048`），服务端**继续无指标运行**。换 `-MetricsPort` 或先停占用进程；本手册实测时 9090 被 soak-stability 占用 | **真实·第 21 轮本机实测**（见 §5.7） |
| 9 | （**已修复**）`launcher.ps1 -Status` 曾把运行中进程误报「未运行（相差 28800.4 秒）」 | `Get-Content target\mir-next.launcher.pid`；`Get-Process -Id <pid>` | 旧版对带 `Z` 的时间二次换算（28800s=8h）。修复：PID 文件加 `started_unix`，用 `[DateTimeOffset]` 统一 UTC 比较；反例（cmd.exe PID / 时间减 60s）均 `exit 3` 拒绝停止 | **真实·第 21 轮实测 + 工部修复**（证据 `target/launcher-refuse-*.out`；见 §5.8） |
| 10 | 启动报端口被占用（7000） | `Get-NetTCPConnection -State Listen -LocalPort 7000`；按 PID 查进程 | 可能是 soak-stability（改名副本）在跑。换端口（`-Port 7001`）或确认后再停；**不要按名杀** | **真实·第 21 轮本机实测**（§5.1） |
| 11 | PostgreSQL 连不上，`-FirstRun` 检查失败 | `Test-NetConnection 127.0.0.1 -Port 5432`；`Get-Service *postgres*` | 启动 PG、修正 `.env` 的 `DATABASE_URL`（host/port/密码/库名）；网络/防火墙放行 5432；本机当前 5432 不可达 | **真实·第 21 轮本机实测**（§5.6） |
| 12 | autosave 失败/告警，玩家进度可能回档 | `Select-String target\launcher-server.log -Pattern 'db_save_failed|autosave|backoff'`；`/metrics` 看 `mir2_db_save_failed_total`/`mir2_autosave_total` | 保留脏标记，退避 1s→30s 重试；先修 DB（连接/磁盘/锁），失败计数持续增长则人工介入并评估回档窗口（≤ `MIR2_AUTOSAVE_SECS`+1s） | 机制见 `docs/world/persistence.md` §4（**故障注入未实测**） |
| 13 | 磁盘满 / 日志暴涨 | `Get-PSDrive C`；`Get-ChildItem target\*.log | Sort-Object Length -Descending | Select-Object -First 10` | 清理旧日志/旧 soak 数据/旧 `dist`；数据库所在盘单独监控；设日志轮转（**未实测**：本机 C 盘余量 600GB，未制造满盘） | **未实测**（命令部分实测：`Get-PSDrive C` 见 §5.5） |

## 5. 常见故障（症状 → 排查 → 修复）

### 5.1 端口被占用

**症状**：`launcher.ps1 -FirstRun` 打印「端口 7000 已被占用（PID …）」并 exit 2；
或 `serve.ps1` 抛出「端口 7000 已被占用」。

**排查（实测输出，2026-10-20 18:13）**：

```text
PS> Get-NetTCPConnection -State Listen -LocalPort 7000 | Select-Object LocalAddress,LocalPort,OwningProcess

LocalAddress LocalPort OwningProcess
------------ --------- -------------
127.0.0.1         7000         47828
```

```text
PS> Get-Process -Id 47828 | Select-Object Id,ProcessName,Path

Id          : 47828
ProcessName : soak-stability
Path        : C:\Users\diana\Documents\github\mir-next\target\soak-run\soak-stability.exe
```

**修复**：那是 4h soak 的改名副本（锁文件：`target/soak.lock`，owner=hubu，
ends~22:18）。**不要杀**；换端口启动（`-Port 7001`）或等 soak 结束。若确认不是 soak，
按 PID 停：`Stop-Process -Id <pid>`（先核对 Path 属于本项目）。

### 5.2 进程残留 / exe 被锁（os error 5）

**症状**：`cargo build`/`ci.ps1` 报 `failed to remove file ... (os error 5)`。

**排查（实测输出，2026-10-20 18:13）**：

```text
PS> Get-Process -Name mir-server,soak-stability | Select-Object Id,ProcessName,StartTime,@{n='MB';e={[math]::Round($_.WorkingSet64/1MB,1)}}

   Id ProcessName    StartTime              MB
   -- -----------    ---------              --
32884 mir-server     2026/10/20 18:12:10 25.30
47828 soak-stability 2026/10/20 18:11:14 38.60
```

**修复**：优先 `launcher.ps1 -Stop` / `serve.ps1 -Stop`（按 PID 文件）。只有**确认**
不是长跑 soak 时才可 `ci.ps1 -KillStale`；`soak.lock` 存在时必须等锁释放。

### 5.3 PID 文件与进程

**排查（实测输出，2026-10-20 18:12）**：

```text
PS> (Get-Content target\server-7031.pid -Raw).Trim()
54164

PS> (Get-Process -Id 54164).ProcessName
mir-server
```

**修复**：PID 文件不存在 → 服务端不是由 `serve.ps1`/`launcher.ps1` 启动（soak 探针会
因此采不到 RSS，见 §4 #2），改用脚本启动；PID 文件存在但进程不在 → 残留文件，确认后删除
再启动。

### 5.4 日志定位

**日志路径（实测，2026-10-20 18:12）**：

```text
PS> (Get-Item target\server-7031.log).FullName
C:\Users\diana\Documents\github\mir-next\target\server-7031.log

PS> (Get-Item target\server-7031.log).Length
2661
```

| 启动方式 | stdout | stderr | PID |
|---|---|---|---|
| `launcher.ps1` | `target/launcher-server.log` | `target/launcher-server.err.log` | `target/mir-next.launcher.pid` |
| `serve.ps1` | `target/server-<端口>.log` | `target/server-<端口>.err.log` | `target/server-<端口>.pid` |
| 发布包手动 `Start-Process` | 由 `-RedirectStandardOutput` 指定（deploy.md 示例 `server.log`） | `server.err.log` | 无（建议改用 launcher） |

关键日志：

```powershell
Select-String -Path target\launcher-server.log -Pattern 'panic|ERROR|migration .*failed|metrics 端点启动失败' -CaseSensitive:$false
Select-String -Path target\launcher-server.log -Pattern 'metrics summary' | Select-Object -Last 3
```

`metrics summary` 每 30 秒一行（实测格式）：

```text
INFO mir_server::server: metrics summary metrics=players=0 maps=0 connections=0 rejected=0 ticks=1419 tick_last_us=248 tick_p99_us=285 db_saves=0 db_save_p95_us=0 db_save_failed=0 autosaves=0 db_flushes=0 db_flush_p95_us=0 mail_queue=0 dropped=0 write_failed=0
```

### 5.5 磁盘满

**排查（实测输出，2026-10-20 18:12）**：

```text
PS> Get-PSDrive C | Select-Object Name,@{n='FreeGB';e={[math]::Round($_.Free/1GB,1)}},@{n='UsedGB';e={[math]::Round($_.Used/1GB,1)}}

Name FreeGB  UsedGB
---- ------  ------
C    600.00 1306.60
```

**修复（未实测满盘场景）**：清理 `target/*.log`、旧 `target/stress-*.json`、旧
`docs/testing/soak-data`、旧 `dist`；数据库盘单独留 ≥20% 余量；不要删 `target/soak.lock`。

### 5.6 PostgreSQL 断连

**排查（实测输出，2026-10-20 18:13）**：

```text
PS> Test-NetConnection 127.0.0.1 -Port 5432 -InformationLevel Quiet
警告: TCP connect to (127.0.0.1 : 5432) failed
False
```

**修复**：本机当前没有 PG 在 5432；启动 PostgreSQL 或把 `.env` 的 `DATABASE_URL` 指到
可达实例，再 `launcher.ps1 -FirstRun`。检查 `Get-Service *postgres*`、防火墙、密码/库名。
**注意**：未配 `DATABASE_URL` 时服务端静默用 MemoryStore（重启丢档），日志有
`WARN ... 未配置数据库，使用内存存储`。

### 5.7 `/metrics` 端口冲突

**症状（实测，2026-10-20 18:12）**：服务端监听正常，但 `/metrics` 连接被拒；日志：

```text
WARN mir_server::server: metrics 端点启动失败，继续无指标运行 listen=127.0.0.1:9090 error=通常每个套接字地址(协议/网络地址/端口)只允许使用一次。 (os error 10048)
```

**排查**：`Get-NetTCPConnection -State Listen -LocalPort 9090 | Select-Object LocalPort,OwningProcess`。

**修复**：换 `-MetricsPort 9091` 重启；或停掉占用者（确认不是别人的 soak）。
`metrics` 是可选端点，冲突不会阻止游戏监听，但值班会失去指标（见 §8 判定）。

### 5.8 启动器 `-Status` 误报（**已修复**：`launcher.ps1` 增加 `started_unix`）

**历史症状（2026-10-20 18:13 实测，旧版）**：进程 32884 明明在跑，`launcher.ps1 -Status` 却报：

```text
未运行：PID 32884 的启动时间与 PID 文件相差 28800.4 秒（疑似 PID 复用）
如确认是残留 PID 文件，可删除 target/mir-next.launcher.pid 后重新启动。
exit=1
```

**原因**：PID 文件里的 `started` 是 UTC（`...Z`），旧检查里 `DateTime` 又做了一次本地/UTC
换算，差值正好 8 小时（28800s）。

**修复（工部，第 21 轮）**：PID 文件新增 **`started_unix`（epoch 秒）**，`Test-Identity`
用 `[DateTimeOffset]` 统一到 UTC 比较，不再对带 `Z` 的字符串二次换算；身份校验仍包含
**exe 路径 + 命令行项目路径**。修复后实测（soak 安静窗口内）：

```text
# 正例：launcher.ps1 -Status（exit 0）
运行中：PID 52072  端口 7061  已运行 4 秒
exe：C:\Users\diana\Documents\github\mir-next\target\release\mir-server.exe
PID 文件：C:\Users\diana\Documents\github\mir-next\target\mir-next.launcher.pid  启动时间：10/20/2026 10:21:54
metrics：mir2_players_online 0; mir2_connections_active 0; mir2_connections_total 0;
mir2_packets_dropped_total 0; mir2_write_failed_total 0; mir2_ticks_total 42;
mir2_db_save_total 0; mir2_db_save_failed_total 0

# 反例 1：把 cmd.exe PID 塞进 PID 文件 → exit 3，cmd 未被杀
REFUSE: 拒绝停止 PID 17528：PID 17528 的 exe 是 C:\Windows\System32\cmd.exe，
不是 C:\Users\diana\Documents\github\mir-next\target\release\mir-server.exe

# 反例 2：真实 server PID，started_unix 减 60s → exit 3，server 仍存活
REFUSE: 拒绝停止 PID 48976：PID 48976 的启动时间与 PID 文件相差 60 秒（疑似 PID 复用）
```

三个起停循环（7061/7062/7063）全部 `start_exit=0 / status_exit=0 / stop_exit=0`，端口与
PID 文件均无残留。证据文件：`target/launcher-refuse-cmd.out`、
`target/launcher-refuse-time.out`、`target/launcher-cycle{1,2,3}-{start,status,stop}.out`。

**运维口径**：现在 `-Status` 退出码 0 = 运行中，1 = 未运行（PID 文件不存在/进程不在），
`-Stop` 遇身份不符会 `exit 3` 并拒绝停止——**不要因为 REFUSE 就删 PID 文件**，先按提示核对
exe/启动时间。

### 5.9 autosave 失败

**症状**：日志出现 `db_save_failed`、`autosave` 退避告警；`mir2_db_save_failed_total` 上涨。

**排查**：

```powershell
Select-String -Path target\launcher-server.log -Pattern 'db_save_failed|backoff|autosave' | Select-Object -Last 20
(Invoke-WebRequest -UseBasicParsing http://127.0.0.1:9090/metrics).Content -split "`n" |
  Where-Object { $_ -match '^mir2_(db_save_failed|autosave|batch_write_failed)' }
```

**修复**：先修 DB（§5.6/§5.5）；退避会 1s→30s 自动重试，失败不推进脏标记。若持续失败，
按默认 `MIR2_AUTOSAVE_SECS=30` 评估最坏回档 ≤31s（`docs/world/persistence.md` §3），
必要时停服保护数据。**故障注入未实测**。

## 6. `/metrics` 字段表（≥5 项，真实抓取对照）

抓取方式（实测，2026-10-20 18:12，HTTP 200 / 2656 bytes）：

```powershell
Invoke-WebRequest -UseBasicParsing http://127.0.0.1:9090/metrics
```

> 说明：这次 9090 属于正在跑的 soak 服务端（`soak-stability`，100 连接）；我们另起的
> 7031 因 9090 被占而没开指标。字段名以真实输出为准，tick 分位是**微秒**
> （`mir2_tick_micros_*`，不是 `*_ms`）。

| 指标（真实输出） | 抓取值 | 含义 / 值班判定 |
|---|---|---|
| `mir2_players_online` | `99` | 在线玩家；与网关连接数对齐 |
| `mir2_connections_active` | `99` | 当前 TCP 连接 |
| `mir2_connections_total` | `100` | 启动以来接受连接（有 1 条已断） |
| `mir2_connections_rejected` | `0` | 限流拒绝；持续 >0 查 `MIR2_MAX_CONN_*` 与攻击 |
| `mir2_packets_dropped_total` | `0` | 包背压/限流丢弃；增长说明客户端刷包或服务端卡 |
| `mir2_write_failed_total` | `0` | writer 写失败/对端断开 |
| `mir2_tick_micros_p50` | `119`（=0.119ms） | tick 中位耗时 |
| `mir2_tick_micros_p99` | `771`（=0.771ms） | tick p99；**<80ms** 为健康（soak 口径） |
| `mir2_tick_micros_max` | `1004`（=1.004ms） | 本轮窗口最大 |
| `mir2_db_save_total` | `198` | 角色落盘次数 |
| `mir2_db_save_failed_total` | `0` | **>0 且增长需人工介入** |
| `mir2_autosave_total` | `99` | autosave 实际写库次数 |
| `mir2_batch_write_failed_total` | `0` | 行会/邮件批次失败（失败会回灌） |
| `mir2_db_pool_connections` / `mir2_db_pool_idle` | `5` / `5` | 连接池；长期打满/空闲 0 要查泄漏或长事务 |
| `mir2_mail_queue` | `0` | 待写邮件队列 |

完整字段（含 `mir2_players_on_map{map="0"}`、拍卖扫描、DB flush 耗时分位等）见
[`docs/server/metrics.md`](../server/metrics.md) §3。

## 7. 值班检查清单

### 7.1 每日（建议每 4–8 小时）

| 项 | 判定标准 |
|---|---|
| 进程/端口 | `launcher.ps1 -Status`（exit 0）或端口监听存在；身份/时间校验见 §5.8 |
| 在线数 | `mir2_players_online` 与网关连接数差 ≤ 5；掉到 0 且无人登录需查 |
| tick | `mir2_tick_micros_p99 < 80_000`（80ms，微秒口径）且 `p99` 无持续上升趋势 |
| 落盘失败 | `mir2_db_save_failed_total` **增量 = 0**；>0 即人工介入（查 DB/磁盘） |
| 批次/写失败 | `mir2_batch_write_failed_total`、`mir2_write_failed_total` 增量 = 0 |
| 丢包/限流 | `mir2_packets_dropped_total`、`mir2_connections_rejected` 增量 = 0；增长查刷包/限流配置 |
| 磁盘 | `Get-PSDrive C` 剩余 > 20%；日志目录不持续暴涨 |
| 日志 | 最近 30 分钟无 `panic`/`ERROR`/<br>`migration .*failed`/`metrics 端点启动失败` |
| soak 锁 | `target/soak.lock` 存在时：**不跑 `-KillStale`、不按名杀进程**，并确认锁里 owner/结束时间 |

### 7.2 每周

| 项 | 判定标准 |
|---|---|
| 数据库备份 | 每日 dump 存在、大小非 0；保留 ≥7 天；每周至少做一次恢复演练（`migrate_drill.ps1`，未实测） |
| 发布包 | `package.ps1 -Verify` exit 0；`manifest.sha256.json` 与当前发布 commit 一致 |
| 门禁 | `ci.ps1` 全绿（含 `verify_text_keys`、硬编码棘轮 67/15、check_links） |
| 指标趋势 | 7 天 `p99`/RSS/连接池无单调上升；RSS 周增幅 ≤10% |
| 日志轮转 | 旧日志已归档/清理；磁盘余量仍 >20% |
| 事故复盘 | 本周每条 `db_save_failed`/`panic`/soak 锁中断都有记录与修复项 |

## 8. 已知边界与未实测项

* **未实测**：`pg_dump`/`pg_restore`（本机 5432 不可达）、满盘处置、PG 断连恢复、autosave
  故障注入、迁移恢复演练。命令与步骤来自 `docs/deploy.md`/`persistence.md`，执行前先在
  测试库演练。
* **未实测但已实测部分**：端口占用/PID/日志/磁盘/进程/`/metrics`/启动器身份校验（本文件 §5
  贴的是本机真实输出）。
* `tools/dev/launcher.ps1` 为第 21 轮新增；`-Status` 的 UTC 误报已在同轮修复（`started_unix`
  + `[DateTimeOffset]`，见 §5.8），反例拒绝与 3 次起停循环均有证据文件。
* 本手册不覆盖 Linux 部署、Windows 服务化/NSSM、Prometheus/Grafana 告警规则（当前项目
  只提供 `/metrics` 文本端点）。
