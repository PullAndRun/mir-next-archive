# 资源根冒烟：验证 `game-assets/` 与回落 `resource/` 的行为
#
# 覆盖三件事：
#   1. `MIR2_ASSET_DIR=game-assets` → 服务端日志里地图目录 = `game-assets/map`
#   2. `MIR2_ASSET_DIR=<不存在的目录>` → **warn** 并回落 `resource/map`
#   3. 不设环境变量 → 默认 `game-assets`（存在时）；不存在则 `info` 回落 `resource`
# 外加客户端资源根解析单测（`cargo test -p mir-client --lib assets::`）。
#
# 用法：
#   pwsh -NoProfile -File tools/dev/asset-smoke.ps1              # 默认用 target/debug/mir-server.exe
#   pwsh -NoProfile -File tools/dev/asset-smoke.ps1 -Seconds 8   # 每档多跑一会儿

param(
    [int]$Seconds = 5,
    [string]$Exe = "target/debug/mir-server.exe",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Push-Location $repoRoot
$env:RUST_LOG = "info"
$failures = @()

function Write-Step([string]$Text) {
    Write-Host ""
    Write-Host "== $Text ==" -ForegroundColor Cyan
}

function Invoke-Server([hashtable]$Env, [string]$Tag) {
    $log = Join-Path $repoRoot "target/asset-smoke-$Tag.log"
    $err = Join-Path $repoRoot "target/asset-smoke-$Tag.err.log"
    Remove-Item $log, $err -ErrorAction SilentlyContinue
    $start = @{
        FilePath = (Join-Path $repoRoot $Exe)
        ArgumentList = @()
        RedirectStandardOutput = $log
        RedirectStandardError = $err
        WindowStyle = "Hidden"
        PassThru = $true
    }
    $saved = @{}
    foreach ($key in $Env.Keys) {
        $saved[$key] = [Environment]::GetEnvironmentVariable($key, "Process")
        [Environment]::SetEnvironmentVariable($key, $Env[$key], "Process")
    }
    try {
        $process = Start-Process @start
        Start-Sleep -Seconds $Seconds
        if (-not $process.HasExited) {
            Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
        }
    } finally {
        foreach ($key in $saved.Keys) {
            [Environment]::SetEnvironmentVariable($key, $saved[$key], "Process")
        }
    }
    $text = ""
    foreach ($file in @($log, $err)) {
        if (Test-Path $file) { $text += (Get-Content $file -Raw) }
    }
    return $text
}

if (-not $SkipBuild -and -not (Test-Path (Join-Path $repoRoot $Exe))) {
    Write-Step "构建服务端（$Exe 不存在）"
    cargo build -p mir-server
    if ($LASTEXITCODE -ne 0) { throw "cargo build -p mir-server 失败" }
}

if (-not (Test-Path (Join-Path $repoRoot $Exe))) {
    throw "找不到 $Exe；先 cargo build -p mir-server，或用 -Exe 指定路径"
}

$baseEnv = @{
    MIR2_LISTEN = "127.0.0.1:0"          # 端口随意分配，只为跑起来打日志
    MIR2_LOAD_GAME_DATA = "0"            # 冒烟不加载真实刷怪，启动更快
    MIR2_METRICS_LISTEN = ""             # 不开 /metrics
}

# ---- 1) 显式指定 game-assets ----
Write-Step "1) MIR2_ASSET_DIR=game-assets"
$env1 = $baseEnv.Clone()
$env1["MIR2_ASSET_DIR"] = "game-assets"
$log1 = Invoke-Server $env1 "game-assets"
if ($log1 -match "map_root=game-assets/map" -or $log1 -match "root=game-assets/map") {
    Write-Host "  OK  地图目录 = game-assets/map" -ForegroundColor Green
} else {
    Write-Host "  FAIL 日志里没有 game-assets/map" -ForegroundColor Red
    $failures += "①MIR2_ASSET_DIR=game-assets 未生效"
}

# ---- 2) 显式指定不存在的目录 → warn + 回落 ----
Write-Step "2) MIR2_ASSET_DIR=<不存在的目录> → 应 warn 并回落 resource"
$env2 = $baseEnv.Clone()
$env2["MIR2_ASSET_DIR"] = "target/definitely-not-an-asset-dir"
$log2 = Invoke-Server $env2 "missing"
$hasWarn = $log2 -match "指定的资源目录不存在"
$hasFallback = ($log2 -match "map_root=resource/map") -or ($log2 -match "root=resource/map")
if ($hasWarn -and $hasFallback) {
    Write-Host "  OK  warn + 回落 resource/map" -ForegroundColor Green
} else {
    Write-Host "  FAIL warn=$hasWarn fallback=$hasFallback" -ForegroundColor Red
    $failures += "②不存在的资源目录没有 warn/回落"
}

# ---- 3) 不设环境变量 → 默认 game-assets ----
Write-Step "3) 不设 MIR2_ASSET_DIR → 默认 game-assets（存在时）"
$env3 = $baseEnv.Clone()
$log3 = Invoke-Server $env3 "default"
$gameAssetsExists = Test-Path (Join-Path $repoRoot "game-assets")
if ($gameAssetsExists) {
    if ($log3 -match "map_root=game-assets/map") {
        Write-Host "  OK  默认走 game-assets/map" -ForegroundColor Green
    } else {
        Write-Host "  FAIL 默认没有走 game-assets" -ForegroundColor Red
        $failures += "③默认资源根未生效"
    }
} else {
    if ($log3 -match "map_root=resource/map") {
        Write-Host "  OK  没有 game-assets/ → 回落 resource/map" -ForegroundColor Green
    } else {
        Write-Host "  FAIL 没有 game-assets/ 时未回落" -ForegroundColor Red
        $failures += "③缺少 game-assets/ 时未回落"
    }
}

# ---- 4) 客户端资源根解析单测 ----
Write-Step "4) 客户端资源根解析单测"
cargo test -q -p mir-client --lib assets:: 2>&1 | Select-Object -Last 3 | ForEach-Object { Write-Host "  $_" }
if ($LASTEXITCODE -ne 0) {
    Write-Host "  FAIL 客户端 assets 单测失败" -ForegroundColor Red
    $failures += "④客户端 assets 单测失败"
} else {
    Write-Host "  OK  客户端 assets 单测通过" -ForegroundColor Green
}

Write-Step "汇总"
if ($failures.Count -gt 0) {
    foreach ($item in $failures) { Write-Host "  FAIL $item" -ForegroundColor Red }
    Pop-Location
    exit 1
}
Write-Host "  全部通过（日志：target/asset-smoke-*.log）" -ForegroundColor Green
Pop-Location
exit 0
