# 部署与运维（Windows x64 发布包）

发布版本 **0.2.0**（`Cargo.toml` = `dist/mir-next-0.2.0-win64/` = `CHANGELOG.md` 第 20 轮）。
发布包由 `tools/dev/package.ps1` 生成，包内只有 `mir-server.exe`、`mir-client.exe`、`config/` 示例与 `resources/README.txt`；
`resource/` 素材**不随包分发**，需要单独拷贝（见第 2 节）。

## 1. 环境变量

服务端全部通过环境变量配置；未设置时使用括号内默认值。

> **`/metrics` 默认关闭**：必须显式设置 `MIR2_METRICS_LISTEN=127.0.0.1:9090` 才会启动；
> 未设置时没有 HTTP 端点（不是端口冲突/防火墙）。生产建议只绑 `127.0.0.1` 或内网，
> 不要暴露公网；soak/监控脚本必须显式配置该变量。

> **必须捕获服务端 stdout**：`tracing_subscriber` 的日志默认写 **stdout**（不是 stderr）。
> 部署/测试若把 stdout 丢弃（`Stdio::null()` / 未重定向），`db_save ok character=... seq=... t=...`
> 等结构化日志等于没有；RPO/审计也无法计算 `T_save`。请把 stdout 重定向到日志文件或采集器，
> 并同时保留 stderr。

| 名称 | 必填 | 默认值 | 说明 |
|---|---|---|---|
| `DATABASE_URL` | 否 | 未设置 → MemoryStore | PostgreSQL 连接串（如 `postgres://user:pass@host:5432/mir_next`）。设置后使用 PostgreSQL 并**自动执行 `migrations/`**；不设置则内存存储，重启丢档（仅开发/冒烟）。 |
| `MIR2_DATABASE_URL` | 否 | — | 同 `DATABASE_URL` 的别名，优先级更高。 |
| `MIR2_LISTEN` | 否 | `0.0.0.0:7000` | 游戏网关监听地址；解析失败会告警并沿用默认。 |
| `MIR2_METRICS_LISTEN` | 否 | 关闭 | `/metrics` 监听地址（如 `127.0.0.1:9000`）；只建议绑本机/内网。 |
| `MIR2_START_MAP` | 否 | `0` | 新角色起始地图文件名（不含 `.map`）。 |
| `MIR2_DB_PATH` | 否 | 无 | 兼容旧参数：覆盖数据库文件路径（当前实现以 PostgreSQL/MemoryStore 为主）。 |
| `MIR2_LOAD_GAME_DATA` | 否 | 开启 | 设为 `0/false/no` 时不加载 `Server.MirDB` 游戏数据（仅调试启动用）。 |
| `MIR2_GM_ACCOUNTS` | 否 | 空 | 逗号分隔的 GM 账号名（启用 `@` 命令）。 |
| `MIR2_CHECK_VERSION` | 否 | `false` | 是否校验客户端版本；需要 `Configs/Setup.ini` 的客户端 exe 配置。 |
| `MIR2_ALLOW_START_GAME` | 否 | `true` | 是否允许进入世界（生产环境可按需关闭维护）。 |
| `MIR2_AUTOSAVE_SECS` | 否 | `30` | 周期自动存档间隔（秒）；`0` = 关闭。 |
| `MIR2_MAX_CONN_PER_IP` | 否 | `5` | 单 IP 最大连接数。 |
| `MIR2_MAX_CONN_PER_SEC` | 否 | `10` | 全服每秒新建连接上限。 |
| `MIR2_IDLE_TIMEOUT_SECS` | 否 | `60` | 空闲超时（秒）。 |
| `MIR2_MAX_PACKETS_PER_SEC` | 否 | `200` | 单连接包速率上限。 |
| `MIR2_MAX_FRIENDS` | 否 | `100` | 好友/黑名单上限。 |
| `REDIS_URL` | 否 | 未设置 | **预留**：当前版本服务端不读 Redis（用于后续跨进程缓存/会话共享）；设置也不会报错。 |

发布包解压后建议在 `dist/mir-next-<版本>-win64/` 下启动，并把 `resource/`、`config/` 放在同级目录。

## 2. 首次启动（建库 + 迁移）

1. 准备 PostgreSQL 14+，创建空库（示例）：
   ```sql
   CREATE DATABASE mir_next;
   ```
2. 设置连接串（PowerShell 示例）：
   ```powershell
   $env:DATABASE_URL = "postgres://mir:secret@127.0.0.1:5432/mir_next"
   $env:MIR2_LISTEN = "0.0.0.0:7000"
   ```
3. 从开发仓库复制素材与配置（只需一次）：
   ```powershell
   Copy-Item -Recurse <repo>/resource .\resource
   Copy-Item <repo>/config/lang/* .\config\lang\
   Copy-Item .\config\client.toml.example .\config\client.toml   # 客户端统一设置
   ```
   没有 `resource/` 也能启动：客户端地图/精灵退化为纯色占位，不 panic。
4. 启动（隐藏窗口，避免弹到工作显示器）：
   ```powershell
   Start-Process .\mir-server.exe -WindowStyle Hidden `
     -RedirectStandardOutput .\server.log -RedirectStandardError .\server.err.log
   ```
   看到 `传奇-Next 网关已启动 address=0.0.0.0:7000` 即监听成功。服务端在 `DATABASE_URL` 存在时会**自动执行 `migrations/`**（sqlx），无需手工建表。
5. 客户端：
   ```powershell
   .\mir-client.exe --login
   ```
   服务器地址填 `127.0.0.1:7000`，注册账号后建角色进世界。

## 3. 端口与防火墙

| 端口 | 用途 | 建议 |
|---|---|---|
| `7000/tcp`（`MIR2_LISTEN`） | 游戏网关（登录/世界协议） | 仅对玩家网段开放；公网需在防火墙放行 TCP 入站。 |
| `9000/tcp`（`MIR2_METRICS_LISTEN`，可选） | Prometheus `/metrics` | **只绑 127.0.0.1 或内网**，不要暴露公网。 |
| `5432/tcp` | PostgreSQL | 只允许服务端主机访问，不对公网开放。 |

Windows 放行示例（管理员 PowerShell）：
```powershell
New-NetFirewallRule -DisplayName "mir-next gateway" -Direction Inbound -Protocol TCP -LocalPort 7000 -Action Allow
```
NAT/云安全组需同步放行 7000/tcp；反向代理不适用于本 TCP 协议。

## 4. 日志位置

- 服务端默认 tracing 输出到 **stdout/stderr**；用 `Start-Process -RedirectStandardOutput/-RedirectStandardError` 落盘（见第 2 节示例的 `server.log` / `server.err.log`）。
- 开发机的 `tools/dev/serve.ps1` 固定写到：
  - `target/server-<端口>.log`（stdout）
  - `target/server-<端口>.err.log`（stderr）
- 客户端按 `--connect`/`--login` 启动，诊断信息在控制台；截图用 `F2` 保存到 `build/screenshot-<n>.png`。

## 5. 备份建议

- **数据库**：只备份 PostgreSQL 即可覆盖角色/背包/装备/技能/任务/好友/行会/邮件/市场/攻城全部持久化数据。
  ```powershell
  pg_dump --format=custom --file=mir_next_$(Get-Date -Format yyyyMMdd_HHmm).dump $env:DATABASE_URL
  ```
  建议每日全量 + 关键更新前手动一次，保留 ≥7 天；恢复用 `pg_restore --clean --if-exists`。
- **配置/素材**：`config/`（语言包、`client.toml`）与 `resource/` 属于静态文件，随版本归档即可。
- MemoryStore 模式没有可备份数据（重启即丢），**生产必须设置 `DATABASE_URL`**。

## 6. 升级步骤（迁移不可回退，先备份）

> **重要：数据库迁移不可回退。升级前必须先备份数据库；回滚只能靠备份恢复。**

1. **备份**：按第 5 节执行 `pg_dump`，确认 dump 文件大小非 0。
2. **停服**：停止旧 `mir-server.exe`，等待现有连接断开（或先摘流量）。
3. **替换程序**：解压新 `mir-next-<新版本>-win64`，保留原 `resource/` 与 `config/`（如有自定义 `client.toml`/语言包）。
4. **启动新服务端**：新进程会自动执行新增迁移（`migrations/` 只增不改）。观察日志无 `migration ... failed` 后，确认端口监听。
5. **验证**：客户端登录、进世界、打开背包/行会/邮件各一次；`/metrics` 的 `connections_*`、`tick_*` 正常。
6. **回滚**：若必须回退——
   - 停止新服务端；
   - 恢复旧二进制；
   - **用第 1 步的 dump 恢复数据库**（迁移已改表结构，旧程序不能直接跑在新库上）；
   - 重新启动并验证。
7. 清理旧版本目录；保留最近 2 个版本的发布包与对应数据库 dump。

### 发布前手动打包（不并入 CI）

```powershell
pwsh -NoProfile -File tools/dev/package.ps1                 # 构建 + 生成 dist/
pwsh -NoProfile -File tools/dev/package.ps1 -Verify         # 校验文件数 + 逐文件 SHA-256 + 客户端 --help 冒烟
```
`package.ps1` **不加入 `ci.ps1` 的 10 项门禁**（release 构建是分钟级），只在发版前手动执行。

> **体积约定**：dist/ 是本地构建产物，**不入库**（.gitignore 已忽略）；需要发布包时按上面命令现场生成。详见 [process/repo-hygiene.md](process/repo-hygiene.md)。
删除任一产物文件或改动任一字节，`-Verify` 都会以非 0 退出并打印 `MISSING:` / `TAMPERED:` 文件名。

### 本机实测（2026-10-20，Windows x64，release）

| 指标 | 数值 |
|---|---|
| `mir-server.exe` | 8,743,936 字节 |
| `mir-client.exe` | 4,516,864 字节 |
| 冷启动到监听（3 次：705 / 570 / 527 ms，**中位 570 ms**，MemoryStore） | ≤ 10 s ✅ |
| `cargo build --release`（增量，两二进制） | 57.4 s |
