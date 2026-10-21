#!/usr/bin/env pwsh
<#
.SYNOPSIS
    仓库体积清理（默认只清“可再生成的临时产物”，不动源码与 resource/）。

.DESCRIPTION
    约定（2026-10-22 起）：仓库只保留“当前正在用或以后要用”的产物。
    - `target/` 里除 `release/`、`debug/`、`mapgrid-*.bin` 之外的成员验证副本（`*-verify/`、`ci-sim/`、`t*-verify/`）一律是**一次性**产物；
    - 日志只保留最近 `-KeepDays` 天（默认 3 天）；
    - 编译缓存的“深清”要显式加 `-Deep`（会触发全量重建，耗时 5–10 分钟）。

.PARAMETER Deep
    运行 `cargo clean` 并删除整个 `target/`（之后需要重新构建）。
    **不常规执行**：仅在磁盘余量告急（默认 < 120 GB）或怀疑产物损坏时用；
    磁盘充足时加 `-Deep` 会被拒绝，需要 `-Force` 显式覆盖（避免无谓重编译写盘）。

.PARAMETER Force
    允许在磁盘充足时仍执行 `-Deep`。

.PARAMETER KeepDays
    保留多少天内的 `target/*.log` 与 `tools/dev/evidence/`（默认 3）。

.PARAMETER DryRun
    只打印将删除的内容，不实际删除。

.EXAMPLE
    pwsh -NoProfile -File tools/dev/clean.ps1 -DryRun
    pwsh -NoProfile -File tools/dev/clean.ps1
    pwsh -NoProfile -File tools/dev/clean.ps1 -Deep
#>
[CmdletBinding()]
param(
    [switch]$Deep,
    [switch]$Force,
    [int]$MinFreeGb = 120,
    [int]$KeepDays = 3,
    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location $root

function Get-DirSizeMb([string]$path) {
    if (-not (Test-Path $path)) { return 0 }
    $sum = (Get-ChildItem $path -Recurse -File -ErrorAction SilentlyContinue | Measure-Object Length -Sum).Sum
    return [math]::Round($sum / 1MB, 1)
}

function Remove-Target([string]$path, [string]$reason) {
    if (-not (Test-Path $path)) { return 0 }
    $mb = Get-DirSizeMb $path
    if ($DryRun) {
        Write-Host "[dry-run] 将删除 $path （$mb MB，$reason）"
    } else {
        Remove-Item $path -Recurse -Force -ErrorAction SilentlyContinue
        Write-Host "[clean] 已删除 $path （$mb MB，$reason）"
    }
    return $mb
}

# 0) 安全闸：有编译/服务端在跑就不要清
$busy = Get-Process -Name cargo, rustc, mir-server -ErrorAction SilentlyContinue
if ($busy) {
    Write-Host "[abort] 检测到 cargo/rustc/mir-server 正在运行，先停掉再清理：" -ForegroundColor Yellow
    $busy | Select-Object Id, ProcessName | Format-Table | Out-String | Write-Host
    exit 2
}

$total = 0

# 1) 成员的一次性验证副本（每个都是整份 target 拷贝，几十 GB）
$target = Join-Path $root 'target'
if (Test-Path $target) {
    Get-ChildItem $target -Directory -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -match '(-verify|^ci-sim$|^t\d+$|^tmp$|^drill$|^soak-smoke$|^pkg-selftest)' } |
        ForEach-Object {
            $total += Remove-Target $_.FullName '成员一次性验证副本/临时目录'
        }
}

# 2) 过期日志（target 根 + tools/dev/evidence）
$cut = (Get-Date).AddDays(-$KeepDays)
foreach ($dir in @($target, (Join-Path $root 'tools/dev/evidence'))) {
    if (-not (Test-Path $dir)) { continue }
    Get-ChildItem $dir -File -Filter '*.log' -ErrorAction SilentlyContinue |
        Where-Object { $_.LastWriteTime -lt $cut } |
        ForEach-Object {
            if ($DryRun) { Write-Host "[dry-run] 将删除过期日志 $($_.Name) （$([math]::Round($_.Length/1MB,2)) MB）" }
            else { $script:total += [math]::Round($_.Length / 1MB, 2); Remove-Item $_.FullName -Force }
        }
}

# 3) 临时/一次性可执行文件与备份（target 根目录）
if (Test-Path $target) {
    Get-ChildItem $target -File -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -match '\.(exe|bundle|bak)$' } |
        ForEach-Object {
            if ($DryRun) { Write-Host "[dry-run] 将删除临时产物 $($_.Name) （$([math]::Round($_.Length/1MB,1)) MB）" }
            else { $script:total += [math]::Round($_.Length / 1MB, 2); Remove-Item $_.FullName -Force }
        }
}

# 4) python 缓存
Get-ChildItem $root -Recurse -Directory -Filter '__pycache__' -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -notmatch '\\resource\\|\\github\\' } |
    ForEach-Object { $total += Remove-Target $_.FullName 'python 缓存' }

# 5) 深清：整个 target（**不常规**；磁盘充足时拒绝，避免无谓重编译写盘）
if ($Deep -and (Test-Path $target)) {
    $freeGb = [math]::Round((Get-PSDrive C).Free / 1GB, 1)
    if ($freeGb -ge $MinFreeGb -and -not $Force) {
        Write-Host ("[skip] 磁盘尚有 {0} GB（阈值 {1} GB），不做 -Deep：全量重编译会额外写入数 GB。" -f $freeGb, $MinFreeGb) -ForegroundColor Yellow
        Write-Host "       确实要清就加 -Force（例如怀疑产物损坏）。" -ForegroundColor Yellow
    } else {
        Write-Host "==> -Deep：将删除整个 target（之后需要 cargo build 重建）" -ForegroundColor Yellow
        if ($DryRun) {
            Write-Host "[dry-run] 将删除 target/（$((Get-DirSizeMb $target)) MB）"
        } else {
            $mb = Get-DirSizeMb $target
            Remove-Item $target -Recurse -Force -ErrorAction SilentlyContinue
            $total += $mb
            Write-Host "[clean] 已删除 target/（$mb MB）"
        }
    }
}

$free = [math]::Round((Get-PSDrive C).Free / 1GB, 1)
Write-Host ""
Write-Host ("==> 完成：本次回收 ≈ {0:N1} MB；C 盘可用 {1} GB" -f $total, $free) -ForegroundColor Green
Write-Host "提示：清理后首次构建会重新编译（release 约 2–4 分钟，全量测试约 5–8 分钟）。"
