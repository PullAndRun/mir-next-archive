# 5 分钟上手（Windows x64）

目标：从零启动一个本地服务端 + 客户端，并学会用 launcher 查看/停止。
所有命令都在仓库根目录执行（PowerShell）。

## 双击启动（最省事，推荐）

仓库根目录的 **`启动传奇Next.cmd`** 双击即可：

1. 弹出「传奇-Next 一键启动器」窗口，自动检查 `.env` / 数据库 / `game-assets/` / 可执行文件；
2. 点 **“开始游戏（服务端+客户端）”**：自动起服务端（隐藏窗口）→ 等端口就绪 → 起客户端；
   - 首次缺 `mir-server.exe` / `mir-client.exe` 时会提示并自动 `cargo build --release`；
   - 数据库不可达、端口被占、缺 `.env` 等情况会弹窗说明原因，并显示日志位置；
3. 点 **“停止服务端”** 停止（复用 `launcher.ps1` 的 PID 身份校验，不按进程名杀）；
4. “打开日志目录”直达 `target/`；状态区绿色=可用、红色=需要修复。

> 启动器源码：`tools/dev/launcher-gui.ps1`（WinForms，兼容 Windows PowerShell 5.1）。
> 排错/自检：`powershell -NoProfile -ExecutionPolicy Bypass -File tools/dev/launcher-gui.ps1 -SelfTest`
> （另有 `-Smoke` 只构造界面、`-E2E -Port 7142` 起停服务端做端到端自检）。

## 前置依赖

| 依赖 | 版本/说明 | 检查命令 |
|---|---|---|
| Windows | x64，PowerShell 7（`pwsh`）或 Windows PowerShell 5.1 | `$PSVersionTable.PSVersion` |
| Rust | 1.99+（含 cargo） | `cargo --version` |
| PostgreSQL | 14+，已建库（可用现有 `mir2`） | `psql --version` |
| 素材 | 仓库内 `resource/`（地图/图库/音效/字体）；缺素材也能启动，画面退化占位 | `Test-Path resource` |
| 环境文件 | 仓库根目录 `.env`，至少一行 `DATABASE_URL` | `Test-Path .env` |

`.env` 示例（把密码换成自己的）：

```powershell
Set-Content -Path .env -Value 'DATABASE_URL=postgres://postgres:password@127.0.0.1:5432/mir_next'
```

## 三条命令

```powershell
# 1) 构建（首次/改代码后；约 1–2 分钟）
cargo build --release -p mir-server -p mir-client

# 2) 首次启动：检查 .env / 数据库 / 端口 → 生成 config/client.toml → 隐藏窗口起服务端
pwsh -NoProfile -File tools/dev/launcher.ps1 -FirstRun

# 3) 查看状态 / 停止（停止只按 PID 文件 + 身份校验，不按进程名杀）
pwsh -NoProfile -File tools/dev/launcher.ps1 -Status
pwsh -NoProfile -File tools/dev/launcher.ps1 -Stop
```

启动客户端（可选；默认只起服务端，避免弹窗）：

```powershell
pwsh -NoProfile -File tools/dev/launcher.ps1 -FirstRun -WithClient
# 或直接运行 target\release\mir-client.exe，服务器地址填 127.0.0.1:7000
```

换端口/开 metrics（多人开发或端口冲突时）：

```powershell
pwsh -NoProfile -File tools/dev/launcher.ps1 -FirstRun -Port 7001 -MetricsPort 9090
pwsh -NoProfile -File tools/dev/launcher.ps1 -Status
```

## 三条命令各自做什么

1. `cargo build --release ...`：产出 `target/release/mir-server.exe`、`mir-client.exe`。
2. `launcher.ps1 -FirstRun`：
   - 检查 `.env`、`DATABASE_URL`、数据库 TCP 可达、游戏端口空闲；
   - 缺 `config/client.toml` 时按默认值生成；
   - 把服务端以隐藏窗口启动，stdout/stderr 写 `target/launcher-server.log` / `.err.log`；
   - PID 写入 `target/mir-next.launcher.pid`（含 exe、启动时间、端口、metrics 端口、项目根目录）。
3. `-Status` / `-Stop`：
   - Status 打印 PID、端口、运行时长与 `/metrics` 摘要；
   - Stop 先校验 PID 的 exe 路径、启动时间（±2s）与命令行项目路径，全对才 `Stop-Process -Id`（精确 PID）；任一不符 → `REFUSE`，exit 3，不动进程。

## 常见报错与对应命令

| 现象 | 原因 | 对应命令 |
|---|---|---|
| `启动前检查未通过` + `缺少环境文件 .env`，exit 2 | 没有 `.env` | `Set-Content .env 'DATABASE_URL=postgres://...'` 后重跑 `-FirstRun` |
| `数据库不可达：连接 host:port ...`，exit 2 | PostgreSQL 未启动/地址密码错 | `Test-NetConnection <host> -Port 5432`；改 `.env` 后重跑 |
| `端口 7000 已被占用（PID ...）`，exit 2 | 已有服务端/别的程序 | `launcher.ps1 -Status`；或 `-FirstRun -Port 7001` |
| `未运行：找不到 target/mir-next.launcher.pid`，exit 1 | 没启动或已停止 | `launcher.ps1 -FirstRun` |
| `未运行：PID ... 不存在 / 启动时间相差 N 秒`，exit 1 | 进程已退出或 PID 被复用 | 确认无该服务后删 `target/mir-next.launcher.pid`，再 `-FirstRun` |
| `REFUSE: 拒绝停止 PID ...`，exit 3 | PID 文件里的进程不是本项目服务端 | **不要强删进程**；人工核对后删 PID 文件 |
| `服务端未能在 20 秒内监听`，exit 2 | 数据库迁移/端口/日志异常 | 看 `target/launcher-server.err.log`，修正后重跑 |
| `-Status` 的 metrics 显示 `未启用` | 启动时没加 `-MetricsPort` | 停掉后 `-FirstRun -MetricsPort 9090` |
| `Start-Process` 启动后命令看起来不返回 | 子进程仍持有父控制台句柄（终端/管道的已知现象） | 另开一个终端跑 `-Status`/`-Stop`；脚本本身已退出并写好 PID 文件 |

## 安全说明（第 20 轮事故教训）

- launcher **不做进程名匹配**，只按 PID 文件 + 身份校验停止。
- 手动清理时也请用 `Stop-Process -Id <PID>`；不要按进程名批量停止（会误杀别人的 soak/联调）。
- 生产/压测期间不要运行 `ci.ps1 -KillStale`；`target/soak.lock` 存在时 CI 会自行拒绝。
- 日志：服务端 `target/launcher-server.log`、客户端 `target/launcher-client.log`（`-WithClient` 时）。
