# 本地全量门禁 —— 本项目**不使用远端 CI**，这是唯一的自动化门禁入口。
#
# 全部检查在本地跑（无远端 CI）。改代码时常跑 `-SkipTests`，收口前跑全量。
#
# 用法（PowerShell 7）：
#   pwsh -NoProfile -File tools/dev/ci.ps1              # 全跑（10 项）
#   pwsh -NoProfile -File tools/dev/ci.ps1 -SkipTests   # 只跑静态检查（快）
#   pwsh -NoProfile -File tools/dev/ci.ps1 -NoAssets    # 额外跑「无 resource/」沙箱（慢）
#
# 退出码 0 = 全绿。
param(
    [switch]$SkipTests,
    [switch]$NoAssets,
    # 跳过 PostgreSQL 集成测试（没有本地 PG 时用）。
    [switch]$SkipPostgres,
    # 构建前强制结束残留的 mir-server / mir-client（它们会锁住 target\debug\*.exe，
    # 表现为 `failed to remove file ... (os error 5)`）。
    [switch]$KillStale
)

$ErrorActionPreference = "Continue"
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location $root

$results = @()
function Invoke-Gate {
    param([string]$Name, [string]$Command)
    Write-Host ""
    Write-Host "==> $Name" -ForegroundColor Cyan
    $started = Get-Date
    cmd /c "$Command"
    $code = $LASTEXITCODE
    $elapsed = [int]((Get-Date) - $started).TotalSeconds
    $script:results += [pscustomobject]@{ Name = $Name; Code = $code; Seconds = $elapsed }
    if ($code -eq 0) {
        Write-Host "    OK   ($elapsed s)" -ForegroundColor Green
    } else {
        Write-Host "    FAIL ($elapsed s)" -ForegroundColor Red
    }
}

# Python 解释器：Windows 上 `python` 常是 Microsoft Store 的占位 stub，优先用 `py`。
$python = "python"
if (Get-Command py -ErrorAction SilentlyContinue) { $python = "py" }

# 测试库连接串：**必须在跑 `cargo test --workspace` 之前就设好**。
# 现在有一批「真 PG」用例（如 `conquest_reload_from_postgres_*`）是 fail-loud 的：
# 没有 `MIR2_TEST_DATABASE_URL` 会直接 FAILED 而不是 skip —— 若只在 PG 那一项里设置，
# 前面的 workspace 测试门禁就会红。
# 优先级：显式环境变量 → 仓库根 `.env` 的 `MIR2_DEV_DATABASE_URL` 换库名 → 本地默认。
if (-not $env:MIR2_TEST_DATABASE_URL) {
    $envFile = Join-Path $root ".env"
    if (Test-Path $envFile) {
        $devLine = Get-Content $envFile | Where-Object { $_ -match '^\s*MIR2_DEV_DATABASE_URL\s*=' } | Select-Object -First 1
        if ($devLine) {
            $devUrl = ($devLine -split '=', 2)[1].Trim().Trim('"')
            $env:MIR2_TEST_DATABASE_URL = $devUrl -replace '/[^/]+$', '/mir2_test'
        }
    }
}
if (-not $env:MIR2_TEST_DATABASE_URL) {
    $env:MIR2_TEST_DATABASE_URL = "postgres://postgres:postgres@127.0.0.1:5432/mir2_test"
}
Write-Host "    测试库（workspace 测试与 PG 门禁共用）：$($env:MIR2_TEST_DATABASE_URL -replace ':[^:@]+@', ':***@')" -ForegroundColor DarkGray

# 预检：残留的 mir-server / mir-client 会锁住 target\debug\*.exe，构建直接失败。
$stale = @(Get-Process -Name mir-server, mir-client -ErrorAction SilentlyContinue)
if ($stale.Count -gt 0) {
    $list = ($stale | ForEach-Object { "$($_.Id)@$($_.StartTime.ToString('HH:mm:ss'))" }) -join ", "
    if ($KillStale) {
        $stale | ForEach-Object { Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue }
        Write-Host "==> 已结束残留进程：$list" -ForegroundColor Yellow
        Start-Sleep -Seconds 1
    } else {
        Write-Host "==> 警告：发现残留进程 $list" -ForegroundColor Yellow
        Write-Host "    它们会锁住 target\debug\*.exe 导致构建失败（os error 5）。" -ForegroundColor Yellow
        Write-Host "    加 -KillStale 自动结束，或手动：Get-Process mir-server,mir-client | Stop-Process -Force" -ForegroundColor Yellow
    }
}

Write-Host "== 本地门禁（无远端 CI；python=$python）==" -ForegroundColor Cyan

# 1) Rust：fmt / clippy / test
Invoke-Gate "cargo fmt --all --check" "cargo fmt --all --check"
Invoke-Gate "cargo clippy --workspace --all-targets -D warnings" "cargo clippy --workspace --all-targets -- -D warnings"
if (-not $SkipTests) {
    # duo_smoke 必须串行：两个客户端抢同一个战斗槽，并行会 flake
    Invoke-Gate "cargo test -p mir-server --test duo_smoke（串行）" "cargo test -p mir-server --test duo_smoke -- --test-threads=1"
    Invoke-Gate "cargo test --workspace" "cargo test --workspace"
}

# 2) 协议生成器自检（需要 github/Crystal）
if (Test-Path "github/Crystal") {
    Invoke-Gate "协议生成器 --check" "$python tools/protocol-gen/generate.py --crystal github/Crystal --check"
} else {
    Write-Host ""
    Write-Host "==> 协议生成器 --check 跳过：github/Crystal 不存在" -ForegroundColor Yellow
    $results += [pscustomobject]@{ Name = "协议生成器 --check"; Code = 0; Seconds = 0 }
}

# 3) 文档链接与包编号
Invoke-Gate "文档链接与包编号" "$python tools/docs/check_links.py"

# 4) i18n：TextKey 完整 + 硬编码棘轮
Invoke-Gate "TextKey 完整性" "$python tools/i18n/verify_text_keys.py"
Invoke-Gate "硬编码棘轮（player-visible，阈值 70）" "$python tools/i18n/scan_hardcoded.py --player-visible --strict --fail-above 70"
Invoke-Gate "硬编码棘轮（no-test，阈值 15）" "$python tools/i18n/scan_hardcoded.py --no-test --player-visible --strict --fail-above 15"

# 5) PostgreSQL 集成测试（连接串已在脚本开头统一设好，这里只跑门禁）
if (-not $SkipTests -and -not $SkipPostgres) {
    Invoke-Gate "PostgreSQL 集成测试（mir-store 串行）" "cargo test -p mir-store -- --test-threads=1"
}

# 6) 可选：无 resource/ 沙箱（模拟 CI 没有素材）
if ($NoAssets) {
    Write-Host ""
    Write-Host "==> 无 resource/ 沙箱（tools/dev/test-no-assets.ps1）" -ForegroundColor Cyan
    pwsh -NoProfile -File tools/dev/test-no-assets.ps1 -Jobs 4 -TargetDir target/ci-sim
    $code = $LASTEXITCODE
    $results += [pscustomobject]@{ Name = "无 resource/ 沙箱"; Code = $code; Seconds = 0 }
    if ($code -eq 0) { Write-Host "    OK" -ForegroundColor Green } else { Write-Host "    FAIL" -ForegroundColor Red }
}

Write-Host ""
Write-Host "== 汇总 ==" -ForegroundColor Cyan
$results | ForEach-Object {
    $flag = if ($_.Code -eq 0) { "OK  " } else { "FAIL" }
    $color = if ($_.Code -eq 0) { "Green" } else { "Red" }
    Write-Host ("{0} {1,-52} {2,4}s" -f $flag, $_.Name, $_.Seconds) -ForegroundColor $color
}
$failed = @($results | Where-Object { $_.Code -ne 0 })
if ($failed.Count -gt 0) {
    Write-Host "==> 失败 $($failed.Count) 项" -ForegroundColor Red
    exit 1
}
Write-Host "==> 全部通过（$($results.Count) 项）" -ForegroundColor Green
exit 0
