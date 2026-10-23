# 一键启动器（图形界面）：双击即玩。
#
# 目标：不懂命令行的玩家也能启动《传奇-Next》。
#   - 自动检查 .env / 数据库 / 素材 / 端口 / 可执行文件；
#   - 缺可执行文件时提示并自动 cargo build；
#   - 一键：启动服务端 -> 等待端口 -> 启动客户端；
#   - 停止服务端复用 launcher.ps1 的 PID 身份校验（不按进程名杀）；
#   - 失败时弹窗并显示日志位置。
#
# 用法（一般由根目录 启动传奇Next.cmd 调用）：
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools/dev/launcher-gui.ps1
# 自检（无界面，CI/排错用）：
#   pwsh -NoProfile -File tools/dev/launcher-gui.ps1 -SelfTest
#
# 兼容 Windows PowerShell 5.1（因此不用三元/??/.NET 新语法）。
param(
    [int]$Port = 7000,
    [switch]$SelfTest,
    [switch]$Smoke,
    [switch]$E2E,
    [string]$Root = ""
)

$ErrorActionPreference = 'Stop'

if (-not $Root) {
    $Root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
}
$root = (Resolve-Path -LiteralPath $Root).Path
$launcher = Join-Path $root 'tools\dev\launcher.ps1'
$serverExe = Join-Path $root 'target\release\mir-server.exe'
$serverExeDebug = Join-Path $root 'target\debug\mir-server.exe'
$clientExe = Join-Path $root 'target\release\mir-client.exe'
$clientExeDebug = Join-Path $root 'target\debug\mir-client.exe'
$pidFile = Join-Path $root 'target\mir-next.launcher.pid'
$serverLog = Join-Path $root 'target\launcher-server.log'
$serverErr = Join-Path $root 'target\launcher-server.err.log'
$envFile = Join-Path $root '.env'
$assetDir = Join-Path $root 'game-assets'

function Get-EnvValue([string]$path, [string]$key) {
    if (-not (Test-Path -LiteralPath $path)) { return '' }
    foreach ($line in Get-Content -LiteralPath $path) {
        $t = $line.Trim()
        if (-not $t -or $t.StartsWith('#')) { continue }
        $eq = $t.IndexOf('=')
        if ($eq -lt 1) { continue }
        if ($t.Substring(0, $eq).Trim() -eq $key) {
            return $t.Substring($eq + 1).Trim().Trim('"').Trim("'")
        }
    }
    return ''
}

function Test-DbReachable([string]$url) {
    if (-not $url) { return @{ ok = $false; reason = '缺少 DATABASE_URL' } }
    $m = [regex]::Match($url, '^[a-zA-Z0-9+]+://(?:[^@/]+@)?([^/:?]+)(?::(\d+))?')
    if (-not $m.Success) { return @{ ok = $false; reason = 'DATABASE_URL 格式无法解析' } }
    $hostName = $m.Groups[1].Value
    $dbPort = 5432
    if ($m.Groups[2].Success) { $dbPort = [int]$m.Groups[2].Value }
    try {
        $c = New-Object System.Net.Sockets.TcpClient
        $iar = $c.BeginConnect($hostName, $dbPort, $null, $null)
        $done = $iar.AsyncWaitHandle.WaitOne(3000)
        if (-not $done) { $c.Close(); return @{ ok = $false; reason = "连接 $hostName`:$dbPort 超时" } }
        $c.EndConnect($iar)
        $c.Close()
        return @{ ok = $true; reason = "$hostName`:$dbPort 可达" }
    } catch {
        return @{ ok = $false; reason = "连接 $hostName`:$dbPort 失败：$($_.Exception.Message)" }
    }
}

function Test-PortListening([int]$port) {
    try {
        $c = New-Object System.Net.Sockets.TcpClient
        $iar = $c.BeginConnect('127.0.0.1', $port, $null, $null)
        $done = $iar.AsyncWaitHandle.WaitOne(500)
        if (-not $done) { $c.Close(); return $false }
        $c.EndConnect($iar)
        $c.Close()
        return $true
    } catch {
        return $false
    }
}

function Get-ServerPidInfo {
    if (-not (Test-Path -LiteralPath $pidFile)) { return $null }
    try {
        $info = Get-Content -LiteralPath $pidFile -Raw | ConvertFrom-Json
        if (-not $info.pid) { return $null }
        $proc = Get-Process -Id ([int]$info.pid) -ErrorAction SilentlyContinue
        if (-not $proc) { return $null }
        return $info
    } catch {
        return $null
    }
}

function Get-LogTail([string]$path, [int]$lines) {
    if (-not (Test-Path -LiteralPath $path)) { return '' }
    return (Get-Content -LiteralPath $path -Tail $lines -ErrorAction SilentlyContinue) -join [Environment]::NewLine
}

function Write-SelfTestResult([string]$key, [bool]$ok, [string]$detail) {
    $flag = 'FAIL'
    if ($ok) { $flag = 'PASS' }
    Write-Output ("{0}  {1}  {2}" -f $flag, $key, $detail)
    if (-not $ok) { $script:selfTestFailures = $script:selfTestFailures + 1 }
}

# ---------------------------------------------------------------------------
# 自检模式（无窗口）
# ---------------------------------------------------------------------------
if ($SelfTest) {
    $script:selfTestFailures = 0
    Write-Output "root=$root"
    Write-SelfTestResult 'root' (Test-Path -LiteralPath $root) $root
    Write-SelfTestResult '.env' (Test-Path -LiteralPath $envFile) $envFile
    $dbUrl = Get-EnvValue $envFile 'DATABASE_URL'
    $db = Test-DbReachable $dbUrl
    Write-SelfTestResult 'database' $db.ok ('DATABASE_URL ' + $db.reason)
    Write-SelfTestResult 'assets' (Test-Path -LiteralPath $assetDir) $assetDir
    $hasServer = (Test-Path -LiteralPath $serverExe) -or (Test-Path -LiteralPath $serverExeDebug)
    $hasClient = (Test-Path -LiteralPath $clientExe) -or (Test-Path -LiteralPath $clientExeDebug)
    Write-SelfTestResult 'server_exe' $hasServer ("release=" + (Test-Path -LiteralPath $serverExe) + " debug=" + (Test-Path -LiteralPath $serverExeDebug))
    Write-SelfTestResult 'client_exe' $hasClient ("release=" + (Test-Path -LiteralPath $clientExe) + " debug=" + (Test-Path -LiteralPath $clientExeDebug))
    $info = Get-ServerPidInfo
    Write-Output ("INFO  server_running  running=" + ($null -ne $info) + " port=" + $Port + " listening=" + (Test-PortListening $Port))
    Write-SelfTestResult 'port_usable' ((-not (Test-PortListening $Port)) -or ($null -ne $info)) ("port=" + $Port)
    if ($script:selfTestFailures -gt 0) {
        Write-Output ("SELFTEST RESULT=FAIL failures=" + $script:selfTestFailures)
        exit 1
    }
    Write-Output 'SELFTEST RESULT=PASS'
    exit 0
}

# ---------------------------------------------------------------------------
# 图形界面
# ---------------------------------------------------------------------------
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

[System.Windows.Forms.Application]::EnableVisualStyles()

$form = New-Object System.Windows.Forms.Form
$form.Text = '传奇-Next 一键启动器'
$form.Size = New-Object System.Drawing.Size(720, 560)
$form.StartPosition = 'CenterScreen'
$form.MinimumSize = New-Object System.Drawing.Size(620, 480)
$form.Font = New-Object System.Drawing.Font('Microsoft YaHei UI', 9)

$title = New-Object System.Windows.Forms.Label
$title.Text = '传奇-Next'
$title.Font = New-Object System.Drawing.Font('Microsoft YaHei UI', 20, [System.Drawing.FontStyle]::Bold)
$title.AutoSize = $true
$title.Location = New-Object System.Drawing.Point(20, 12)
$form.Controls.Add($title)

$subtitle = New-Object System.Windows.Forms.Label
$subtitle.Text = '双击即玩：自动检查 -> 启动服务端 -> 启动客户端'
$subtitle.AutoSize = $true
$subtitle.ForeColor = [System.Drawing.Color]::DimGray
$subtitle.Location = New-Object System.Drawing.Point(24, 52)
$form.Controls.Add($subtitle)

$statusBox = New-Object System.Windows.Forms.GroupBox
$statusBox.Text = '运行状态'
$statusBox.Location = New-Object System.Drawing.Point(20, 82)
$statusBox.Size = New-Object System.Drawing.Size(664, 120)
$form.Controls.Add($statusBox)

$lblServer = New-Object System.Windows.Forms.Label
$lblServer.Text = '服务端：检查中…'
$lblServer.AutoSize = $true
$lblServer.Location = New-Object System.Drawing.Point(14, 26)
$statusBox.Controls.Add($lblServer)

$lblClient = New-Object System.Windows.Forms.Label
$lblClient.Text = '客户端：检查中…'
$lblClient.AutoSize = $true
$lblClient.Location = New-Object System.Drawing.Point(14, 50)
$statusBox.Controls.Add($lblClient)

$lblDb = New-Object System.Windows.Forms.Label
$lblDb.Text = '数据库：检查中…'
$lblDb.AutoSize = $true
$lblDb.Location = New-Object System.Drawing.Point(14, 74)
$statusBox.Controls.Add($lblDb)

$lblAssets = New-Object System.Windows.Forms.Label
$lblAssets.Text = '素材：检查中…'
$lblAssets.AutoSize = $true
$lblAssets.Location = New-Object System.Drawing.Point(360, 26)
$statusBox.Controls.Add($lblAssets)

$lblPort = New-Object System.Windows.Forms.Label
$lblPort.Text = "端口：$Port"
$lblPort.AutoSize = $true
$lblPort.Location = New-Object System.Drawing.Point(360, 50)
$statusBox.Controls.Add($lblPort)

$lblHint = New-Object System.Windows.Forms.Label
$lblHint.Text = '提示：绿色=可用，红色=需要修复（见下方日志/弹窗）。'
$lblHint.AutoSize = $true
$lblHint.ForeColor = [System.Drawing.Color]::DimGray
$lblHint.Location = New-Object System.Drawing.Point(360, 74)
$statusBox.Controls.Add($lblHint)

$actionBox = New-Object System.Windows.Forms.GroupBox
$actionBox.Text = '操作'
$actionBox.Location = New-Object System.Drawing.Point(20, 212)
$actionBox.Size = New-Object System.Drawing.Size(664, 86)
$form.Controls.Add($actionBox)

$btnPlay = New-Object System.Windows.Forms.Button
$btnPlay.Text = '开始游戏（服务端+客户端）'
$btnPlay.Size = New-Object System.Drawing.Size(220, 40)
$btnPlay.Location = New-Object System.Drawing.Point(14, 28)
$actionBox.Controls.Add($btnPlay)

$btnServer = New-Object System.Windows.Forms.Button
$btnServer.Text = '只启动服务端'
$btnServer.Size = New-Object System.Drawing.Size(140, 40)
$btnServer.Location = New-Object System.Drawing.Point(244, 28)
$actionBox.Controls.Add($btnServer)

$btnStop = New-Object System.Windows.Forms.Button
$btnStop.Text = '停止服务端'
$btnStop.Size = New-Object System.Drawing.Size(120, 40)
$btnStop.Location = New-Object System.Drawing.Point(394, 28)
$actionBox.Controls.Add($btnStop)

$btnLogs = New-Object System.Windows.Forms.Button
$btnLogs.Text = '打开日志目录'
$btnLogs.Size = New-Object System.Drawing.Size(120, 40)
$btnLogs.Location = New-Object System.Drawing.Point(524, 28)
$actionBox.Controls.Add($btnLogs)

$logLabel = New-Object System.Windows.Forms.Label
$logLabel.Text = '最近日志（服务端 stderr 尾部，失败时看这里）：'
$logLabel.AutoSize = $true
$logLabel.Location = New-Object System.Drawing.Point(24, 308)
$form.Controls.Add($logLabel)

$logBox = New-Object System.Windows.Forms.TextBox
$logBox.Multiline = $true
$logBox.ScrollBars = 'Vertical'
$logBox.ReadOnly = $true
$logBox.WordWrap = $false
$logBox.Location = New-Object System.Drawing.Point(20, 332)
$logBox.Size = New-Object System.Drawing.Size(664, 150)
$logBox.Anchor = 'Top,Left,Right,Bottom'
$logBox.Font = New-Object System.Drawing.Font('Consolas', 8.5)
$form.Controls.Add($logBox)

$btnClose = New-Object System.Windows.Forms.Button
$btnClose.Text = '关闭'
$btnClose.Size = New-Object System.Drawing.Size(90, 30)
$btnClose.Location = New-Object System.Drawing.Point(594, 490)
$btnClose.Anchor = 'Bottom,Right'
$form.Controls.Add($btnClose)

function Set-Color([object]$label, [bool]$ok) {
    if ($ok) { $label.ForeColor = [System.Drawing.Color]::ForestGreen }
    else { $label.ForeColor = [System.Drawing.Color]::Firebrick }
}

function Refresh-Ui {
    $info = Get-ServerPidInfo
    $running = $null -ne $info
    $listening = Test-PortListening $Port
    if ($running -and $listening) {
        $lblServer.Text = "服务端：运行中 (PID $($info.pid), 端口 $Port)"
        Set-Color $lblServer $true
    } elseif ($listening) {
        $lblServer.Text = "服务端：端口 $Port 被占用（可能不是本启动器起的）"
        Set-Color $lblServer $false
    } else {
        $lblServer.Text = '服务端：未运行'
        Set-Color $lblServer $false
    }

    $clientRelease = Test-Path -LiteralPath $clientExe
    $clientDebug = Test-Path -LiteralPath $clientExeDebug
    if ($clientRelease) {
        $lblClient.Text = '客户端：已构建 (release)'
        Set-Color $lblClient $true
    } elseif ($clientDebug) {
        $lblClient.Text = '客户端：已构建 (debug)'
        Set-Color $lblClient $true
    } else {
        $lblClient.Text = '客户端：未构建（点"开始游戏"会自动构建）'
        Set-Color $lblClient $false
    }

    $dbUrl = Get-EnvValue $envFile 'DATABASE_URL'
    $db = Test-DbReachable $dbUrl
    $lblDb.Text = '数据库：' + $db.reason
    Set-Color $lblDb $db.ok

    if (Test-Path -LiteralPath $assetDir) {
        $lblAssets.Text = '素材：game-assets 可用'
        Set-Color $lblAssets $true
    } else {
        $lblAssets.Text = '素材：缺 game-assets（会用 resource 回落）'
        Set-Color $lblAssets $false
    }

    $tail = Get-LogTail $serverErr 12
    if (-not $tail) { $tail = '（暂无错误日志）' }
    $logBox.Text = $tail
    $logBox.SelectionStart = $logBox.Text.Length
    $logBox.ScrollToCaret()

    $btnPlay.Enabled = $true
    $btnServer.Enabled = $true
    $btnStop.Enabled = $running
}

function Show-Error([string]$message, [string]$detail) {
    $text = $message
    if ($detail) { $text = $text + [Environment]::NewLine + [Environment]::NewLine + $detail }
    $text = $text + [Environment]::NewLine + [Environment]::NewLine + "日志：" + $serverLog + [Environment]::NewLine + $serverErr
    [void][System.Windows.Forms.MessageBox]::Show($text, '传奇-Next 启动器', 'OK', 'Error')
}

function Start-LauncherProcess([string[]]$launcherArgs) {
    $pwsh = Get-Command pwsh -ErrorAction SilentlyContinue
    $exe = 'powershell'
    if ($pwsh) { $exe = $pwsh.Source }
    $argList = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $launcher) + $launcherArgs
    # 不 -Wait：launcher 的子进程可能继承控制台句柄，同步等待会看起来“卡住”（项目已知现象）。
    return Start-Process -FilePath $exe -ArgumentList $argList -WindowStyle Hidden -PassThru
}

function Wait-Exit([object]$process, [int]$milliseconds) {
    if ($process.HasExited) { return $true }
    return $process.WaitForExit($milliseconds)
}

function Wait-ServerPid([int]$milliseconds) {
    $deadline = [DateTime]::UtcNow.AddMilliseconds($milliseconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        $info = Get-ServerPidInfo
        if ($info) { return $info }
        Start-Sleep -Milliseconds 200
    }
    return $null
}

function Stop-ServerSafe {
    $info = Wait-ServerPid 5000
    if (-not $info) {
        # 没有 PID 文件：端口也没在监听，视为已停止；否则尝试 launcher -Stop 兜底
        if (-not (Test-PortListening $Port)) { return $true }
        $fallback = Start-LauncherProcess @('-Stop')
        [void](Wait-Exit $fallback 10000)
        Start-Sleep -Milliseconds 500
        return (-not (Test-PortListening $Port))
    }
    $p = Start-LauncherProcess @('-Stop')
    if (Wait-Exit $p 15000) {
        Start-Sleep -Milliseconds 500
        if (-not (Test-PortListening $Port)) { return $true }
    }
    # launcher 自身卡住/未生效时，按 PID 文件里的已校验 PID 直接停止
    try {
        Stop-Process -Id ([int]$info.pid) -Force -ErrorAction Stop
        Remove-Item -LiteralPath $pidFile -ErrorAction SilentlyContinue
        Start-Sleep -Milliseconds 500
        return (-not (Test-PortListening $Port))
    } catch {
        return $false
    }
}

function Ensure-Build {
    $needServer = -not ((Test-Path -LiteralPath $serverExe) -or (Test-Path -LiteralPath $serverExeDebug))
    $needClient = -not ((Test-Path -LiteralPath $clientExe) -or (Test-Path -LiteralPath $clientExeDebug))
    if (-not ($needServer -or $needClient)) { return $true }

    $answer = [System.Windows.Forms.MessageBox]::Show(
        "缺少可执行文件（server=$needServer client=$needClient）。" + [Environment]::NewLine +
        '现在启动一次构建（cargo build --release，约 1-3 分钟）吗？',
        '需要先构建', 'YesNo', 'Question')
    if ($answer -ne 'Yes') { return $false }

    $form.Cursor = [System.Windows.Forms.Cursors]::WaitCursor
    $btnPlay.Enabled = $false
    $btnServer.Enabled = $false
    $logBox.Text = '正在构建，请稍候…（构建窗口会弹出，完成后本窗口自动继续）'
    [System.Windows.Forms.Application]::DoEvents()

    $buildCmd = 'cargo build --release -p mir-server -p mir-client; Write-Host ""; Write-Host "构建结束。按任意键关闭此窗口。"; [void][Console]::ReadKey($true)'
    $build = Start-Process -FilePath 'powershell' -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-NoExit', '-Command', $buildCmd) -WorkingDirectory $root -PassThru -Wait

    $form.Cursor = [System.Windows.Forms.Cursors]::Default
    $btnPlay.Enabled = $true
    $btnServer.Enabled = $true
    Refresh-Ui
    $ok = (Test-Path -LiteralPath $serverExe) -and (Test-Path -LiteralPath $clientExe)
    if (-not $ok) {
        Show-Error '构建未完成或失败，请查看构建窗口里的报错。' $('mir-server.exe=' + (Test-Path -LiteralPath $serverExe) + ' mir-client.exe=' + (Test-Path -LiteralPath $clientExe))
    }
    return $ok
}

function Start-ServerChecked {
    if (Get-ServerPidInfo) { return $true }
    if (Test-PortListening $Port) {
        Show-Error "端口 $Port 已被占用，可能已有别的服务在跑。" '先停止占用端口的程序，或用 launcher.ps1 -Port 改端口。'
        return $false
    }
    $envExists = Test-Path -LiteralPath $envFile
    if (-not $envExists) {
        Show-Error '缺少 .env（至少一行 DATABASE_URL）。' '示例：DATABASE_URL=postgres://postgres:password@127.0.0.1:5432/mir_next'
        return $false
    }
    $db = Test-DbReachable (Get-EnvValue $envFile 'DATABASE_URL')
    if (-not $db.ok) {
        Show-Error ('数据库不可达：' + $db.reason) '请确认 PostgreSQL 已启动、.env 里的 DATABASE_URL 正确。'
        return $false
    }

    $btnPlay.Enabled = $false
    $btnServer.Enabled = $false
    $form.Cursor = [System.Windows.Forms.Cursors]::WaitCursor
    $logBox.Text = '正在启动服务端…'
    [System.Windows.Forms.Application]::DoEvents()

    $p = Start-LauncherProcess @('-FirstRun', '-Port', "$Port")
    $waited = 0
    while ($waited -lt 80 -and -not (Test-PortListening $Port)) {
        Start-Sleep -Milliseconds 500
        $waited = $waited + 1
        [System.Windows.Forms.Application]::DoEvents()
        if ($p.HasExited -and -not (Test-PortListening $Port)) { break }
    }
    $form.Cursor = [System.Windows.Forms.Cursors]::Default
    $btnPlay.Enabled = $true
    $btnServer.Enabled = $true
    Refresh-Ui
    if (-not (Test-PortListening $Port)) {
        $exitText = '仍在运行'
        if ($p.HasExited) { $exitText = '退出码 ' + $p.ExitCode }
        Show-Error ('服务端启动失败（launcher ' + $exitText + '）。') (Get-LogTail $serverErr 12)
        return $false
    }
    return $true
}

function Start-Client {
    $exe = $clientExe
    if (-not (Test-Path -LiteralPath $exe)) { $exe = $clientExeDebug }
    if (-not (Test-Path -LiteralPath $exe)) {
        Show-Error '找不到客户端可执行文件。' '先点"开始游戏"自动构建，或运行 cargo build --release -p mir-client。'
        return $false
    }
    $args = @('--login', '--server', ('127.0.0.1:' + $Port))
    Start-Process -FilePath $exe -ArgumentList $args -WorkingDirectory $root | Out-Null
    return $true
}

if ($E2E) {
    $db = Test-DbReachable (Get-EnvValue $envFile 'DATABASE_URL')
    if (-not $db.ok) {
        Write-Output ('E2E RESULT=FAIL database ' + $db.reason)
        exit 1
    }
    if (Test-PortListening $Port) {
        Write-Output ('E2E RESULT=FAIL port_in_use port=' + $Port)
        exit 1
    }
    $p = Start-LauncherProcess @('-FirstRun', '-Port', "$Port")
    $waited = 0
    while ($waited -lt 80 -and -not (Test-PortListening $Port)) {
        Start-Sleep -Milliseconds 500
        $waited = $waited + 1
        if ($p.HasExited -and -not (Test-PortListening $Port)) { break }
    }
    if (-not (Test-PortListening $Port)) {
        Write-Output ('E2E RESULT=FAIL server_not_listening waited=' + $waited + 's')
        Write-Output (Get-LogTail $serverErr 8)
        exit 1
    }
    $pidInfo = Wait-ServerPid 5000
    if (-not $pidInfo) {
        Write-Output 'E2E RESULT=FAIL pid_file_missing'
        exit 1
    }
    Write-Output ('E2E PASS server_up port=' + $Port + ' pid=' + $pidInfo.pid + ' waited=' + $waited + 's')
    $stopped = Stop-ServerSafe
    Start-Sleep -Seconds 2
    $stillListening = Test-PortListening $Port
    if ($stopped -and -not $stillListening) {
        Write-Output 'E2E PASS server_stopped'
        Write-Output 'E2E RESULT=PASS'
        exit 0
    }
    Write-Output ('E2E RESULT=FAIL stop failed=' + (-not $stopped) + ' still_listening=' + $stillListening)
    exit 1
}

$btnPlay.Add_Click({
    if (-not (Ensure-Build)) { return }
    if (-not (Start-ServerChecked)) { return }
    if (-not (Start-Client)) { return }
    Refresh-Ui
    [void][System.Windows.Forms.MessageBox]::Show('服务端已就绪，客户端已启动。' + [Environment]::NewLine + '在登录窗口输入账号密码（不存在会自动注册），选角后进入游戏。', '可以开始玩了', 'OK', 'Information')
})

$btnServer.Add_Click({
    if (-not (Ensure-Build)) { return }
    [void](Start-ServerChecked)
    Refresh-Ui
})

$btnStop.Add_Click({
    if (-not (Get-ServerPidInfo)) {
        [void][System.Windows.Forms.MessageBox]::Show('服务端没有在运行。', '提示', 'OK', 'Information')
        return
    }
    $ok = Stop-ServerSafe
    Refresh-Ui
    if (-not $ok) {
        Show-Error '停止失败。' '请手动查看 target\launcher-server.err.log 与 target\mir-next.launcher.pid。'
    }
})

$btnLogs.Add_Click({
    Start-Process -FilePath 'explorer.exe' -ArgumentList $root\target | Out-Null
})

$btnClose.Add_Click({ $form.Close() })

$timer = New-Object System.Windows.Forms.Timer
$timer.Interval = 3000
$timer.Add_Tick({ Refresh-Ui })
$timer.Start()

$form.Add_Shown({ Refresh-Ui })

if ($Smoke) {
    Refresh-Ui
    $controls = $form.Controls.Count
    Write-Output ("SMOKE RESULT=PASS controls=" + $controls + " title=" + $form.Text)
    exit 0
}

[void]$form.ShowDialog()
$timer.Stop()
