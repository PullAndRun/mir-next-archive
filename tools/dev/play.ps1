# 启动客户端（mir-client）用于人工试玩，并把窗口固定到指定显示器（默认显示器 2）。
#
# 背景：显示器 1 是用户工作用的，显示器 2 是测试用的；客户端窗口绝不能弹到显示器 1。
# 服务端仍用 `serve.ps1` 以**隐藏窗口**方式启动（服务端不需要可见窗口）。
#
# 用法：
#   pwsh -NoProfile -File tools/dev/play.ps1
#   pwsh -NoProfile -File tools/dev/play.ps1 -Display 2 -Server 127.0.0.1:7000
#   pwsh -NoProfile -File tools/dev/play.ps1 -Exe target\release\mir-client.exe
param(
    [int]$Display = 0  # 0 = 自动：显示器 2 开着用它，关了就用显示器 1,
    [string]$Server = "",
    [string]$Exe = "target\debug\mir-client.exe",
    [string]$DataDir = ""
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'display.ps1')

$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$exePath = if ([System.IO.Path]::IsPathRooted($Exe)) { $Exe } else { Join-Path $root $Exe }
if (-not (Test-Path $exePath)) {
    throw "找不到客户端可执行文件：$exePath（先跑 cargo build -p mir-client）"
}

$gameArgs = @()
if ($Server) { $gameArgs += @('--server', $Server) }
if ($DataDir) { $gameArgs += @('--data-dir', $DataDir) }

$startArgs = @{
    FilePath    = $exePath
    PassThru    = $true
    WindowStyle = 'Normal'
}
if ($gameArgs.Count -gt 0) { $startArgs['ArgumentList'] = $gameArgs }

$process = Start-Process @startArgs
Write-Host "客户端已启动（PID $($process.Id)），正在把它移到显示器 $Display…"

if (-not (Wait-WindowOnDisplay -ProcessId $process.Id -Display $Display -TimeoutSeconds 15 -Resize)) {
    Write-Warning "没能在 15 秒内把窗口移到显示器 $Display；如果它出现在显示器 1 上，请手动拖过去并告诉我。"
}

$process.WaitForExit() | Out-Null
Write-Host "客户端已退出（PID $($process.Id)）"
