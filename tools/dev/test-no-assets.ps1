<#
.SYNOPSIS
在没有 `resource/` 的目录里跑整套测试，模拟 CI（GitHub Actions 里没有素材库）。

.DESCRIPTION
CI 上只有源码、没有 `resource/`（地图 / WZL / Server.MirDB 都没有）。
本脚本的做法：
  1) 用独立的 target 目录把**测试二进制**编出来（`cargo test --workspace --no-run`）；
  2) 把这些二进制**放到一个没有 `resource/` 的目录里逐个执行**（工作目录决定了
     `resource/...` 相对路径能不能找到）；
  3) 任何二进制非 0 退出就整体失败，并打印它的 `test result:` 行。

依赖真实素材的用例应当「缺素材 → eprintln + return」（见 `docs/testing/ci-assets.md`），
所以整套在没有素材时应当**全绿**（只是跳过那些用例）。

.EXAMPLE
powershell -NoProfile -File tools/dev/test-no-assets.ps1
powershell -NoProfile -File tools/dev/test-no-assets.ps1 -Jobs 4 -TargetDir target/ci-sim
#>

[CmdletBinding()]
param(
    [string]$TargetDir = "target/ci-sim",
    [int]$Jobs = 4,
    [int]$TimeoutSeconds = 1800
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location $root

$targetPath = Join-Path $root $TargetDir
$sandbox = Join-Path $targetPath "no-assets"
New-Item -ItemType Directory -Force -Path $sandbox | Out-Null
# 保险：沙箱里绝不能有 resource/（否则就测不出 CI 的缺失场景）
$strayResource = Join-Path $sandbox "resource"
if (Test-Path $strayResource) { throw "沙箱目录里存在 resource/：$strayResource" }

Write-Host "==> 构建测试二进制（CARGO_TARGET_DIR=$TargetDir）"
$env:CARGO_TARGET_DIR = $targetPath
cargo test --workspace --no-run 2>&1 | Out-Host
if ($LASTEXITCODE -ne 0) { throw "cargo test --workspace --no-run 失败（exit $LASTEXITCODE）" }

$exes = Get-ChildItem (Join-Path $targetPath "debug/deps") -Filter *.exe |
    Where-Object { $_.BaseName -match '-[0-9a-f]{16}$' }
Write-Host "==> 共 $($exes.Count) 个测试二进制，工作目录=$sandbox（无 resource/）"

$jobs = foreach ($exe in $exes) {
    Start-Job -ScriptBlock {
        param($path, $cwd)
        Set-Location $cwd
        $out = & $path --test-threads=4 2>&1 | Out-String
        [pscustomobject]@{
            Name    = ($path -split '\\')[-1]
            Code    = $LASTEXITCODE
            Summary = (($out -split "`n" | Select-String -Pattern '^test result:' | Select-Object -Last 1) -replace '\s+', ' ').Trim()
        }
    } -ArgumentList $exe.FullName, $sandbox
}

$jobs | Wait-Job -Timeout $TimeoutSeconds | Out-Null
$results = $jobs | Receive-Job
$jobs | Remove-Job -Force

$failed = $results | Where-Object { $_.Code -ne 0 }
$results | Sort-Object Name | ForEach-Object {
    $flag = if ($_.Code -eq 0) { "OK  " } else { "FAIL" }
    Write-Host ("{0} {1,-42} {2}" -f $flag, $_.Name, $_.Summary)
}
if ($failed) {
    Write-Host "==> 失败 $($failed.Count) / $($results.Count) 个测试二进制"
    exit 1
}
Write-Host "==> 全部通过：$($results.Count) 个测试二进制（无 resource/）"
exit 0
