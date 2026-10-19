# 本地全量门禁 —— 与 `.github/workflows/ci.yml` 的 5 个 job 一一对应。
#
# 为什么需要它：仓库改为**私有**后，GitHub Actions 需要付费额度（当前账号的
# spending limit / 账单未就绪，Action 会在 runner 分配前失败）。所有者随时可能恢复，
# 但**门禁不能靠别人的账单**，所以把同样的检查做成本地一条命令。
#
# 用法（PowerShell 7）：
#   pwsh -NoProfile -File tools/dev/ci.ps1              # 全跑
#   pwsh -NoProfile -File tools/dev/ci.ps1 -SkipTests   # 只跑静态检查（快）
#   pwsh -NoProfile -File tools/dev/ci.ps1 -NoAssets    # 额外跑「无 resource/」沙箱（慢）
#
# 退出码 0 = 全绿。
param(
    [switch]$SkipTests,
    [switch]$NoAssets,
    # 跳过 PostgreSQL 集成测试（没有本地 PG 时用）。
    [switch]$SkipPostgres
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
Write-Host "== 本地 CI（与 .github/workflows/ci.yml 对应；python=$python）==" -ForegroundColor Cyan

# 1) Rust：fmt / clippy / test
Invoke-Gate "cargo fmt --all --check" "cargo fmt --all --check"
Invoke-Gate "cargo clippy --workspace --all-targets -D warnings" "cargo clippy --workspace --all-targets -- -D warnings"
if (-not $SkipTests) {
    # duo_smoke 串行跑（与 CI 一致：两个客户端抢同一个战斗槽，并行会 flake）
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

# 5) PostgreSQL 集成测试
if (-not $SkipTests -and -not $SkipPostgres) {
    $env:MIR2_TEST_DATABASE_URL = if ($env:MIR2_TEST_DATABASE_URL) { $env:MIR2_TEST_DATABASE_URL } else { "postgres://postgres:postgres@127.0.0.1:5432/mir2_test" }
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
