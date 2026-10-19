# 在后台（隐藏窗口）启动本地 mir-server，供手工验证/playtest 用。
#
# 为什么要这个脚本：直接双击或在前台运行 `target\debug\mir-server.exe` 会弹出一个
# 控制台窗口，它会出现在**主显示器**上，打断正在工作的人。这里统一用
# `-WindowStyle Hidden` 启动，并把 stdout/stderr 重定向到日志文件。
#
# 目标运行时：**PowerShell 7（pwsh）**；仍兼容 Windows PowerShell 5.1（因此不用 `Start-Process -Environment`）。
#
# 用法：
#   pwsh -NoProfile -File tools/dev/serve.ps1 -Port 7000
#   pwsh -NoProfile -File tools/dev/serve.ps1 -Port 7000 -GmAccounts playtest
#   pwsh -NoProfile -File tools/dev/serve.ps1 -Port 7000 -Exe target\release\mir-server.exe
#   pwsh -NoProfile -File tools/dev/serve.ps1 -Stop -Port 7000
param(
    [int]$Port = 7000,
    [string]$Exe = "target\debug\mir-server.exe",
    [string]$GmAccounts = "",
    [string]$DataDir = "",
    [switch]$Stop,
    # 需要看服务端控制台时用 -Visible；窗口会被强制搬到 -Display 指定的显示器（默认 2）。
    [switch]$Visible,
    [int]$Display = 0  # 0 = 自动：显示器 2 开着用它，关了就用显示器 1
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$pidFile = Join-Path $root "target\server-$Port.pid"
$logFile = Join-Path $root "target\server-$Port.log"
$errFile = Join-Path $root "target\server-$Port.err.log"

if ($Stop) {
    if (Test-Path $pidFile) {
        $serverPid = (Get-Content $pidFile -Raw).Trim()
        if ($serverPid -and (Get-Process -Id $serverPid -ErrorAction SilentlyContinue)) {
            Stop-Process -Id $serverPid -Force
            Write-Host "已停止 mir-server（PID $serverPid，端口 $Port）"
        } else {
            Write-Host "PID $serverPid 已不在运行"
        }
        Remove-Item $pidFile -ErrorAction SilentlyContinue
    } else {
        Write-Host "没有记录到端口 $Port 的服务端 PID（$pidFile）"
    }
    return
}

$exePath = if ([System.IO.Path]::IsPathRooted($Exe)) { $Exe } else { Join-Path $root $Exe }
if (-not (Test-Path $exePath)) {
    throw "找不到服务端可执行文件：$exePath（先跑 cargo build -p mir-server）"
}

$inUse = Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue
if ($inUse) {
    throw "端口 $Port 已被占用（PID $(($inUse | Select-Object -First 1).OwningProcess)）；换一个 -Port 或先停止它"
}

# 子进程继承父进程环境变量（PS 5.1 没有 Start-Process -Environment）。
$savedListen = $env:MIR2_LISTEN
$savedGm = $env:MIR2_GM_ACCOUNTS
$env:MIR2_LISTEN = "127.0.0.1:$Port"
if ($GmAccounts) { $env:MIR2_GM_ACCOUNTS = $GmAccounts }

$startArgs = @{
    FilePath               = $exePath
    WindowStyle            = if ($Visible) { 'Normal' } else { 'Hidden' }
    PassThru               = $true
    RedirectStandardOutput = $logFile
    RedirectStandardError  = $errFile
}
if ($DataDir) { $startArgs['ArgumentList'] = @('--data-dir', $DataDir) }

try {
    $process = Start-Process @startArgs
} finally {
    $env:MIR2_LISTEN = $savedListen
    $env:MIR2_GM_ACCOUNTS = $savedGm
}

if ($Visible) {
    # 用户要求：可见窗口一律放显示器 2，不能弹在显示器 1 上。
    # 注意：控制台窗口属于 conhost，`MainWindowHandle` 拿不到它，脚本通常搬不动；
    # 所以**默认还是隐藏模式**，`-Visible` 只是调试时的临时手段。
    . (Join-Path $PSScriptRoot 'display.ps1')
    if (-not (Wait-WindowOnDisplay -ProcessId $process.Id -Display $Display -TimeoutSeconds 5)) {
        Write-Warning "控制台窗口通常无法被脚本搬动（属于 conhost）。若它出现在显示器 1 上，请改用默认隐藏模式。"
    }
}

$targetDir = Split-Path -Parent $pidFile
if (-not (Test-Path $targetDir)) {
    New-Item -ItemType Directory -Path $targetDir -Force | Out-Null
}
$process.Id | Set-Content $pidFile

$deadline = (Get-Date).AddSeconds(10)
while ((Get-Date) -lt $deadline) {
    if (Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue) {
        Write-Host "mir-server 已在 127.0.0.1:$Port 监听（PID $($process.Id)，隐藏窗口）"
        Write-Host "日志：$logFile"
        return
    }
    Start-Sleep -Milliseconds 200
}
Write-Host "警告：PID $($process.Id) 已启动但 10 秒内没有监听 $Port，看日志 $logFile / $errFile"
