# 一键启动器：首次运行检查 / 启动 / 状态 / 按 PID 文件安全停止。
#
# 用法：
#   pwsh -NoProfile -File tools/dev/launcher.ps1 -FirstRun
#   pwsh -NoProfile -File tools/dev/launcher.ps1 -Status
#   pwsh -NoProfile -File tools/dev/launcher.ps1 -Stop
#   pwsh -NoProfile -File tools/dev/launcher.ps1 -Port 7000 -MetricsPort 9090
#   pwsh -NoProfile -File tools/dev/launcher.ps1 -FirstRun -WithClient   # 可选：同时拉起客户端
#
# 安全约定（第 20 轮 -KillStale 误杀 soak 的教训）：
#   - 只按 target/mir-next.launcher.pid 里的 PID 停止；
#   - 停止前校验「exe 路径 + 启动时间 + 命令行包含项目根目录」，任一不符即拒绝；
#   - 代码里只按 PID 停止，不做进程名匹配。
param(
    [switch]$FirstRun,
    [switch]$Status,
    [switch]$Stop,
    [switch]$WithClient,
    [string]$Root = "",
    [int]$Port = 0,
    [int]$MetricsPort = 0
)

$ErrorActionPreference = 'Stop'
if (-not $Root) {
    $Root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
}
if (-not (Test-Path -LiteralPath $Root)) {
    Write-Host "FAIL: 项目根目录不存在：$Root" -ForegroundColor Red
    exit 2
}
$root = (Resolve-Path -LiteralPath $Root).Path
$pidFile = Join-Path $root 'target\mir-next.launcher.pid'
$releaseExe = Join-Path $root 'target\release\mir-server.exe'
$debugExe = Join-Path $root 'target\debug\mir-server.exe'
$clientExe = Join-Path $root 'target\release\mir-client.exe'
if (-not (Test-Path -LiteralPath $clientExe)) {
    $clientExe = Join-Path $root 'target\debug\mir-client.exe'
}

function Write-Checklist([string[]]$items) {
    Write-Host '启动前检查未通过，按下面清单处理后重跑：' -ForegroundColor Yellow
    $index = 1
    foreach ($item in $items) {
        Write-Host ("  {0}. {1}" -f $index, $item)
        $index += 1
    }
    exit 2
}

function Read-DotEnv([string]$path) {
    $values = @{}
    if (-not (Test-Path -LiteralPath $path)) { return $values }
    foreach ($line in Get-Content -LiteralPath $path) {
        $trimmed = $line.Trim()
        if (-not $trimmed -or $trimmed.StartsWith('#')) { continue }
        $eq = $trimmed.IndexOf('=')
        if ($eq -lt 1) { continue }
        $key = $trimmed.Substring(0, $eq).Trim()
        $value = $trimmed.Substring($eq + 1).Trim().Trim('"').Trim("'")
        if ($key) { $values[$key] = $value }
    }
    return $values
}

function Get-PortFromUrl([string]$url) {
    # postgres://user:pass@host:5432/db → @{host=...; port=...}；无端口时用 5432。
    $match = [regex]::Match($url, '^[a-zA-Z0-9+]+://(?:[^@/]+@)?([^/:?]+)(?::(\d+))?')
    if (-not $match.Success) { return $null }
    $port = if ($match.Groups[2].Success) { [int]$match.Groups[2].Value } else { 5432 }
    return @{ host = $match.Groups[1].Value; port = $port }
}

function Test-DbReachable([string]$url) {
    $target = Get-PortFromUrl $url
    if (-not $target) { return @{ ok = $false; reason = '连接串格式无法解析（期望 postgres://user:pass@host:port/db）' } }
    $client = New-Object System.Net.Sockets.TcpClient
    try {
        $task = $client.ConnectAsync($target.host, $target.port)
        if (-not $task.Wait(2500)) {
            return @{ ok = $false; reason = "连接 $($target.host):$($target.port) 超时" }
        }
        return @{ ok = $true; reason = "TCP $($target.host):$($target.port) 可达" }
    } catch {
        return @{ ok = $false; reason = "连接 $($target.host):$($target.port) 失败：$($_.Exception.Message)" }
    } finally {
        $client.Dispose()
    }
}

function Test-PortListening([int]$port) {
    if ($port -le 0) { return $false }
    return [bool](Get-NetTCPConnection -State Listen -LocalPort $port -ErrorAction SilentlyContinue)
}

function Get-ProcessIdentity([int]$processId) {
    $process = Get-Process -Id $processId -ErrorAction SilentlyContinue
    if (-not $process) { return $null }
    $cim = Get-CimInstance Win32_Process -Filter "ProcessId = $processId" -ErrorAction SilentlyContinue
    return [pscustomobject]@{
        process      = $process
        path         = $process.Path
        command_line = if ($cim) { $cim.CommandLine } else { '' }
        started      = $process.StartTime
    }
}

function Test-Identity([object]$record) {
    if (-not $record -or -not $record.pid) { return @{ ok = $false; reason = 'PID 文件缺少 pid' } }
    $identity = Get-ProcessIdentity ([int]$record.pid)
    if (-not $identity) { return @{ ok = $false; reason = "PID $($record.pid) 不存在" } }
    if (-not $identity.path -or ($identity.path -ne $record.exe)) {
        return @{ ok = $false; reason = "PID $($record.pid) 的 exe 是 $($identity.path)，不是 $($record.exe)" }
    }
    if ($identity.command_line -and $identity.command_line -notlike "*$root*") {
        return @{ ok = $false; reason = "PID $($record.pid) 的命令行不含项目路径 $root" }
    }
    try {
        $expectedUnix = if ($record.started_unix) {
            [int64]$record.started_unix
        } else {
            ([DateTimeOffset]::Parse($record.started, [Globalization.CultureInfo]::InvariantCulture, [Globalization.DateTimeStyles]::RoundtripKind)).ToUnixTimeSeconds()
        }
        $actualUnix = ([DateTimeOffset]$identity.started.ToUniversalTime()).ToUnixTimeSeconds()
        $delta = [math]::Abs($actualUnix - $expectedUnix)
        if ($delta -gt 2) {
            return @{ ok = $false; reason = "PID $($record.pid) 的启动时间与 PID 文件相差 $delta 秒（疑似 PID 复用）" }
        }
    } catch {
        return @{ ok = $false; reason = 'PID 文件里的 started 无法解析' }
    }
    return @{ ok = $true; reason = '身份校验通过' }
}

function Read-PidFile {
    if (-not (Test-Path -LiteralPath $pidFile)) { return $null }
    try {
        return Get-Content -Raw -LiteralPath $pidFile | ConvertFrom-Json
    } catch {
        Write-Host "警告：PID 文件损坏（$pidFile），按未运行处理" -ForegroundColor Yellow
        return $null
    }
}

function Write-PidFile([int]$serverPid, [datetime]$started, [int]$serverPort, [int]$serverMetricsPort, [int]$clientPid, [string]$serverExePath) {
    $directory = Split-Path -Parent $pidFile
    if (-not (Test-Path -LiteralPath $directory)) {
        New-Item -ItemType Directory -Path $directory -Force | Out-Null
    }
    $record = [ordered]@{
        pid          = $serverPid
        exe          = (Resolve-Path -LiteralPath $serverExePath).Path
        started      = $started.ToUniversalTime().ToString('o')
        started_unix = ([DateTimeOffset]$started.ToUniversalTime()).ToUnixTimeSeconds()
        root         = $root
        port         = $serverPort
        metrics_port = $serverMetricsPort
        client_pid   = $clientPid
    }
    $record | ConvertTo-Json | Set-Content -LiteralPath $pidFile -Encoding UTF8
}

function Get-MetricsSummary([int]$metricsPort) {
    if ($metricsPort -le 0) { return '未启用（启动时加 -MetricsPort <端口>）' }
    try {
        $response = Invoke-WebRequest -UseBasicParsing -Uri "http://127.0.0.1:$metricsPort/metrics" -TimeoutSec 2
        $wanted = @(
            'mir2_players_online', 'mir2_connections_active', 'mir2_connections_total',
            'mir2_ticks_total', 'mir2_packets_dropped_total', 'mir2_db_save_failed_total',
            'mir2_db_save_total', 'mir2_write_failed_total'
        )
        $lines = @()
        foreach ($line in ($response.Content -split "`n")) {
            foreach ($name in $wanted) {
                if ($line.StartsWith($name + ' ')) { $lines += $line.Trim(); break }
            }
        }
        if ($lines.Count -eq 0) { return "已连接 http://127.0.0.1:$metricsPort/metrics（无已知指标）" }
        return ($lines -join '; ')
    } catch {
        return "不可达（http://127.0.0.1:$metricsPort/metrics）：$($_.Exception.Message)"
    }
}

function Show-Status {
    $record = Read-PidFile
    if (-not $record) {
        Write-Host '未运行：找不到 target/mir-next.launcher.pid' -ForegroundColor Yellow
        Write-Host "启动：pwsh -NoProfile -File tools/dev/launcher.ps1 -FirstRun"
        exit 1
    }
    $check = Test-Identity $record
    if (-not $check.ok) {
        Write-Host "未运行：$($check.reason)" -ForegroundColor Yellow
        Write-Host '如确认是残留 PID 文件，可删除 target/mir-next.launcher.pid 后重新启动。'
        exit 1
    }
    $identity = Get-ProcessIdentity ([int]$record.pid)
    $uptime = [math]::Round(((Get-Date) - $identity.started).TotalSeconds)
    Write-Host "运行中：PID $($record.pid)  端口 $($record.port)  已运行 $uptime 秒" -ForegroundColor Green
    Write-Host "exe：$($record.exe)"
    Write-Host "PID 文件：$pidFile  启动时间：$($record.started)"
    if ($record.client_pid -and [int]$record.client_pid -gt 0) {
        Write-Host "客户端 PID：$($record.client_pid)"
    }
    Write-Host "metrics：$(Get-MetricsSummary ([int]$record.metrics_port))"
    exit 0
}

function Stop-ByPidFile {
    $record = Read-PidFile
    if (-not $record) {
        Write-Host '没有可停止的服务：PID 文件不存在' -ForegroundColor Yellow
        exit 1
    }
    $check = Test-Identity $record
    if (-not $check.ok) {
        Write-Host "REFUSE: 拒绝停止 PID $($record.pid)：$($check.reason)" -ForegroundColor Red
        Write-Host '为安全起见未终止任何进程；请人工确认后删除 target/mir-next.launcher.pid。'
        exit 3
    }
    $identity = Get-ProcessIdentity ([int]$record.pid)
    Write-Host "停止 PID $($record.pid)（$($identity.path)）..."
    Stop-Process -Id ([int]$record.pid) -Force
    # 客户端也按 PID 文件里的 PID 停（同样先校验）。
    if ($record.client_pid -and [int]$record.client_pid -gt 0) {
        $clientPid = [int]$record.client_pid
        $client = Get-ProcessIdentity $clientPid
        if ($client -and $client.path -and $client.path -like "*$root*") {
            Stop-Process -Id $clientPid -Force
            Write-Host "已停止客户端 PID $clientPid"
        } else {
            Write-Host "跳过客户端 PID $clientPid（路径不匹配或已退出）" -ForegroundColor Yellow
        }
    }
    $deadline = (Get-Date).AddSeconds(5)
    while ((Get-Date) -lt $deadline) {
        if (-not (Get-Process -Id ([int]$record.pid) -ErrorAction SilentlyContinue)) { break }
        Start-Sleep -Milliseconds 100
    }
    Remove-Item -LiteralPath $pidFile -ErrorAction SilentlyContinue
    Write-Host '已停止，PID 文件已清理。' -ForegroundColor Green
    exit 0
}

function Start-Server([object]$envValues) {
    $serverPort = if ($Port -gt 0) { $Port } else { 7000 }
    $metrics = if ($MetricsPort -gt 0) { $MetricsPort } else { 0 }
    if (Test-PortListening $serverPort) {
        Write-Checklist @(
            "端口 $serverPort 已被占用（PID $((Get-NetTCPConnection -State Listen -LocalPort $serverPort | Select-Object -First 1).OwningProcess)）",
            "下一步：pwsh -NoProfile -File tools/dev/launcher.ps1 -Status   # 查看是否已有实例",
            "或换端口：pwsh -NoProfile -File tools/dev/launcher.ps1 -FirstRun -Port 7001"
        )
    }
    $exe = if (Test-Path -LiteralPath $releaseExe) { $releaseExe } elseif (Test-Path -LiteralPath $debugExe) { $debugExe } else {
        Write-Checklist @(
            "缺少服务端可执行文件：$releaseExe（或 $debugExe）",
            "下一步：cargo build --release -p mir-server -p mir-client"
        )
    }
    $log = Join-Path $root 'target\launcher-server.log'
    $err = Join-Path $root 'target\launcher-server.err.log'
    $savedListen = $env:MIR2_LISTEN
    $savedMetrics = $env:MIR2_METRICS_LISTEN
    $savedDb = $env:DATABASE_URL
    if ($envValues.ContainsKey('DATABASE_URL')) { $env:DATABASE_URL = $envValues['DATABASE_URL'] }
    $env:MIR2_LISTEN = "127.0.0.1:$serverPort"
    if ($metrics -gt 0) { $env:MIR2_METRICS_LISTEN = "127.0.0.1:$metrics" }
    try {
        $process = Start-Process -FilePath $exe -WindowStyle Hidden -PassThru -RedirectStandardOutput $log -RedirectStandardError $err
    } finally {
        $env:MIR2_LISTEN = $savedListen
        $env:MIR2_METRICS_LISTEN = $savedMetrics
        $env:DATABASE_URL = $savedDb
    }
    $deadline = (Get-Date).AddSeconds(20)
    while ((Get-Date) -lt $deadline) {
        if (Test-PortListening $serverPort) { break }
        if ($process.HasExited) { break }
        Start-Sleep -Milliseconds 100
    }
    if (-not (Test-PortListening $serverPort)) {
        if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force }
        Write-Host "FAIL: 服务端未能在 20 秒内监听 127.0.0.1:$serverPort" -ForegroundColor Red
        Write-Host "日志：$log / $err"
        Write-Host '下一步：检查数据库连接串与 target/launcher-server.err.log 后重试。'
        exit 2
    }
    $clientPid = 0
    if ($WithClient -and (Test-Path -LiteralPath $clientExe)) {
        $clientLog = Join-Path $root 'target\launcher-client.log'
        $client = Start-Process -FilePath $clientExe -WindowStyle Hidden -PassThru -RedirectStandardOutput $clientLog -RedirectStandardError ($clientLog + '.err')
        $clientPid = $client.Id
        Write-Host "客户端已启动：PID $clientPid（日志 $clientLog）"
    }
    Write-PidFile $process.Id $process.StartTime $serverPort $metrics $clientPid $exe
    Write-Host "服务端已启动：PID $($process.Id)  监听 127.0.0.1:$serverPort" -ForegroundColor Green
    Write-Host "日志：$log"
    Write-Host "状态：pwsh -NoProfile -File tools/dev/launcher.ps1 -Status"
    Write-Host "停止：pwsh -NoProfile -File tools/dev/launcher.ps1 -Stop"
    exit 0
}

function Invoke-FirstRun {
    $envPath = Join-Path $root '.env'
    if (-not (Test-Path -LiteralPath $envPath)) {
        Write-Checklist @(
            "缺少环境文件：$envPath",
            "下一步：新建 .env 并写入数据库连接串，例如： Set-Content -Path .env -Value 'DATABASE_URL=postgres://postgres:password@127.0.0.1:5432/mir_next'",
            '最少需要一行：DATABASE_URL=postgres://user:pass@host:5432/mir_next',
            '然后重跑：pwsh -NoProfile -File tools/dev/launcher.ps1 -FirstRun'
        )
    }
    $envValues = Read-DotEnv $envPath
    $databaseUrl = if ($envValues.ContainsKey('DATABASE_URL')) { $envValues['DATABASE_URL'] } elseif ($envValues.ContainsKey('MIR2_DEV_DATABASE_URL')) { $envValues['MIR2_DEV_DATABASE_URL'] } else { '' }
    if (-not $databaseUrl) {
        Write-Checklist @(
            "环境文件里缺少 DATABASE_URL（或 MIR2_DEV_DATABASE_URL）：$envPath",
            '示例：DATABASE_URL=postgres://postgres:密码@127.0.0.1:5432/mir_next',
            '修改后重跑：pwsh -NoProfile -File tools/dev/launcher.ps1 -FirstRun'
        )
    }
    $db = Test-DbReachable $databaseUrl
    if (-not $db.ok) {
        Write-Checklist @(
            "数据库不可达：$($db.reason)",
            '检查 PostgreSQL 是否启动、主机/端口/密码是否正确（连接串见 .env 的 DATABASE_URL）',
            '排查：Test-NetConnection <host> -Port <port>',
            '改好后重跑：pwsh -NoProfile -File tools/dev/launcher.ps1 -FirstRun'
        )
    }
    # 端口检查放到 Start-Server 前，先给出可执行提示。
    $serverPort = if ($Port -gt 0) { $Port } else { 7000 }
    if (Test-PortListening $serverPort) {
        Write-Checklist @(
            "端口 $serverPort 已被占用（PID $((Get-NetTCPConnection -State Listen -LocalPort $serverPort | Select-Object -First 1).OwningProcess)）",
            "下一步：pwsh -NoProfile -File tools/dev/launcher.ps1 -Status",
            '或换端口：pwsh -NoProfile -File tools/dev/launcher.ps1 -FirstRun -Port 7001'
        )
    }
    $configDir = Join-Path $root 'config'
    $clientConfig = Join-Path $configDir 'client.toml'
    if (-not (Test-Path -LiteralPath $clientConfig)) {
        if (-not (Test-Path -LiteralPath $configDir)) { New-Item -ItemType Directory -Path $configDir -Force | Out-Null }
        @"
# 由 launcher -FirstRun 生成的客户端统一设置。
language = "zh-cn"
master_volume = 0.500
sound_volume = 0.500
music_volume = 0.500
window_mode = "windowed"
scale = 1.0
show_minimap_gates = true

[keys]
attack = "Space"
magic = "F1"
inventory = "I"
group = "P"
friends = "F"
settings = "O"
"@ | Set-Content -LiteralPath $clientConfig -Encoding UTF8
        Write-Host "已生成客户端配置：$clientConfig"
    }
    Write-Host "环境检查通过：$($db.reason)" -ForegroundColor Green
    # 统一用检查通过的那条连接串（兼容 .env 只写 MIR2_DEV_DATABASE_URL 的情况）。
    $envValues['DATABASE_URL'] = $databaseUrl
    Start-Server $envValues
}

if ($Status -and $Stop) {
    Write-Host 'FAIL: -Status 与 -Stop 不能同时使用' -ForegroundColor Red
    exit 2
}
if ($Status) { Show-Status }
if ($Stop) { Stop-ByPidFile }
if ($FirstRun -or (-not $Status -and -not $Stop)) { Invoke-FirstRun }
