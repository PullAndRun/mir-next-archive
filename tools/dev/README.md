# 本地开发脚本

## 显示器约定（重要）

本机有两块显示器：**显示器 1 是用户工作用的，显示器 2 是测试用的**。

- 任何**可见窗口**都必须出现在显示器 2，绝不能弹到显示器 1（否则会打断用户工作）。
- 服务端一律**隐藏窗口**启动（不需要可见窗口）。
- 客户端用 `play.ps1` 启动，它会等窗口出现后把窗口搬到显示器 2（已实测：窗口落在 `X=2560`，
  即显示器 2 的起点）。
- 移动窗口的工具函数在 `display.ps1`（`Wait-WindowOnDisplay -ProcessId <pid> -Display 2`），
  可被其它脚本 dot-source 复用。

> 注意：**控制台窗口搬不动**（它属于 conhost，`MainWindowHandle` 拿不到），所以服务端不要用
> `-Visible`；要看日志直接 `Get-Content target/server-<port>.log -Wait`。

## `serve.ps1` —— 隐藏窗口启动本地服务端

**为什么需要它**：直接运行 `target\debug\mir-server.exe` 会分配一个控制台窗口，它会出现在
**主显示器**上；在多显示器环境里这会把正在工作的人打断（本项目在 1920×1080 的显示器 2 上
跑测试，窗口却弹到显示器 1）。本脚本统一用 `-WindowStyle Hidden` 启动，并把输出重定向到
日志文件，实测进程 `MainWindowHandle = 0`（无可见窗口）。

### 用法

```powershell
# 启动（默认 127.0.0.1:7000，debug 构建）
powershell -NoProfile -ExecutionPolicy Bypass -File tools/dev/serve.ps1 -Port 7000

# 带 GM 账号（playtest 的 --gm 需要）
powershell -NoProfile -ExecutionPolicy Bypass -File tools/dev/serve.ps1 -Port 7004 -GmAccounts playtest

# 指定构建/数据目录
powershell -NoProfile -ExecutionPolicy Bypass -File tools/dev/serve.ps1 `
    -Port 7000 -Exe target\release\mir-server.exe -DataDir resource/data

# 停止（按端口找 PID 文件）
powershell -NoProfile -ExecutionPolicy Bypass -File tools/dev/serve.ps1 -Stop -Port 7000
```

产物：

| 文件 | 内容 |
|---|---|
| `target/server-<port>.log` | stdout（`tracing` 日志） |
| `target/server-<port>.err.log` | stderr |
| `target/server-<port>.pid` | 进程号，供 `-Stop` 使用 |

行为约定：

- 端口已被占用时**直接报错**（不会重复起两个服务端抢同一端口）；
- 启动后最多等 10 秒确认端口真的在监听，否则打印「已启动但没监听」并给出日志路径；
- 不修改任何仓库文件，只写 `target/`。

### 配合 playtest

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/dev/serve.ps1 -Port 7004 -GmAccounts playtest
python tools/playtest/playtest.py --server 127.0.0.1:7004 --gm playtest --timeout 15
powershell -NoProfile -ExecutionPolicy Bypass -File tools/dev/serve.ps1 -Stop -Port 7004
```

### 注意

- 用完**立刻**停掉：残留进程会占住 `target\debug\mir-server.exe`，让下一次 `cargo build`/
  `cargo test` 报 `failed to remove file (os error 5)`。
- 脚本存为 **UTF-8 with BOM**：Windows PowerShell 5.1 按 ANSI(GBK) 读无 BOM 脚本，中文注释
  会导致解析失败。
