# 新手链路 3 连跑 gate（t123 第 17 轮）
#
# 用法（在仓库根目录）：
#   pwsh -NoProfile -File tools/dev/newbie-gate.ps1
#   pwsh -NoProfile -File tools/dev/newbie-gate.ps1 -Port 7019 -Exe target/libu-playtest-server.exe
#
# 硬门：每次 `--newbie` 必须 exit 0、0 个 [SKIP]、脚本自报墙钟 ≤30:00，且三次全部通过。
# 任何一项不满足本脚本 exit 1。

param(
    [int]$Port = 7019,
    [string]$Exe = "target/libu-playtest-server.exe",
    [double]$Timeout = 15,
    [string]$Python = "py",
    [switch]$SelfCheck
)

$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
Set-Location $root
$server = "127.0.0.1:$Port"
$logDir = Join-Path $root "target"
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$startedHere = $false
$env:PYTHONIOENCODING = "utf-8"

if ($SelfCheck) {
    # 只验证 gate 的三连跑/退出码/0 SKIP 判定逻辑，不连服务端。
    $ok = $true
    for ($i = 1; $i -le 3; $i++) {
        & $Python "tools/playtest/playtest.py" --self-check *> (Join-Path $logDir "newbie-gate-self-$i.log")
        $exit = $LASTEXITCODE
        $text = Get-Content (Join-Path $logDir "newbie-gate-self-$i.log") -Raw -Encoding utf8
        $skips = ([regex]::Matches($text, "\[SKIP\]")).Count
        $runOk = ($exit -eq 0) -and ($skips -eq 0)
        if (-not $runOk) { $ok = $false }
        Write-Host ("self-check run {0}: exit={1} skips={2} -> {3}" -f $i, $exit, $skips, ($(if ($runOk) { "OK" } else { "FAIL" })))
    }
    if ($ok) {
        Write-Host "[OK] newbie gate 逻辑自检：3/3 exit 0、0 SKIP（未连服务端）"
        exit 0
    }
    Write-Host "[FAIL] newbie gate 逻辑自检失败"
    exit 1
}

function Test-Port {
    param([int]$p)
    try {
        $client = [System.Net.Sockets.TcpClient]::new()
        $client.Connect("127.0.0.1", $p)
        $client.Close()
        return $true
    } catch {
        return $false
    }
}

if (-not (Test-Port $Port)) {
    if (-not (Test-Path $Exe)) {
        Write-Host "[FAIL] 服务端 exe 不存在：$Exe（先 cargo build -p mir-server 并复制）"
        exit 1
    }
    & pwsh -NoProfile -ExecutionPolicy Bypass -File "tools/dev/serve.ps1" -Port $Port -Exe $Exe | Out-Host
    $startedHere = $true
    for ($i = 0; $i -lt 40 -and -not (Test-Port $Port); $i++) {
        Start-Sleep -Milliseconds 250
    }
    if (-not (Test-Port $Port)) {
        Write-Host "[FAIL] 服务端未在 $server 监听"
        exit 1
    }
}

$allOk = $true
for ($run = 1; $run -le 3; $run++) {
    $log = Join-Path $logDir ("newbie-gate-{0}.log" -f $run)
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    & $Python "tools/playtest/playtest.py" --server $server --newbie --timeout $Timeout *> $log
    $exit = $LASTEXITCODE
    $sw.Stop()
    $text = Get-Content $log -Raw -Encoding utf8
    $skips = ([regex]::Matches($text, "\[SKIP\]")).Count
    $timeline = [regex]::Matches($text, "t=\+(\d{2}):(\d{2})") |
        ForEach-Object { [int]$_.Groups[1].Value * 60 + [int]$_.Groups[2].Value } |
        Measure-Object -Maximum
    $stampedMax = if ($timeline.Count -gt 0) { $timeline.Maximum } else { 0 }
    $wall = [math]::Round($sw.Elapsed.TotalSeconds, 1)
    $ok = ($exit -eq 0) -and ($skips -eq 0) -and ($stampedMax -le 1800) -and ($wall -le 1800)
    if (-not $ok) { $allOk = $false }
    Write-Host ("run {0}: exit={1} skips={2} stampedMax={3}s wall={4}s -> {5}" -f `
        $run, $exit, $skips, $stampedMax, $wall, ($(if ($ok) { "OK" } else { "FAIL" })))
    if (-not $ok) {
        Write-Host "----- $log (tail) -----"
        Get-Content $log -Tail 20 | ForEach-Object { Write-Host $_ }
    }
}

if ($startedHere) {
    & pwsh -NoProfile -ExecutionPolicy Bypass -File "tools/dev/serve.ps1" -Stop -Port $Port | Out-Null
}
if ($allOk) {
    Write-Host "[OK] newbie gate: 3/3 次 exit 0、0 SKIP、≤30:00"
    exit 0
}
Write-Host "[FAIL] newbie gate 未通过（见上方 run 明细）"
exit 1
