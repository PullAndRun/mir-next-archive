# 在**没有 `resource/`** 的目录里跑整套测试，模拟 CI（GitHub Actions 里没有素材库）。
#
# CI 上只有源码、没有 `resource/`（地图 / WZL / Server.MirDB 都没有）。本脚本的做法：
#   1) 用独立的 target 目录把**测试二进制**编出来（`cargo test --workspace --no-run`）；
#   2) 把这些二进制**放到一个没有 `resource/` 的目录里逐个执行**（工作目录决定了
#      `resource/...` 相对路径能不能找到）；
#   3) 任何二进制非 0 退出就整体失败，并打印它的 `test result:` 行。
#
# 依赖真实素材的用例应当「缺素材 → eprintln + return」（见 `docs/testing/ci-assets.md`），
# 所以整套在没有素材时应当**全绿**（只是跳过那些用例）。
#
# 用法：
#   pwsh -NoProfile -File tools/dev/test-no-assets.ps1
#   pwsh -NoProfile -File tools/dev/test-no-assets.ps1 -Jobs 4 -TargetDir target/ci-sim
#
# 注意：本文件存成 **UTF-8 with BOM**（兼容仍用 PS 5.1 的人；pwsh 7 无此问题）——Windows PowerShell 5.1 会按 ANSI(GBK) 读无 BOM
# 脚本，中文注释会把语法读坏（`Unexpected token`）。另外：PowerShell 变量名大小写不敏感，
# 参数 `$Jobs` 与局部变量 `$jobs` 是同一个变量，所以局部变量叫 `$running/$results`。
param(
    # 同时跑几个测试二进制（默认 4；调大更快但更吃 CPU）。
    [int]$Jobs = 4,
    # 单个二进制超时秒数（默认 600）。
    [int]$TimeoutSeconds = 600,
    # 编译用的 target 目录（默认 target/ci-sim，避开日常构建缓存）。
    [string]$TargetDir = "target/ci-sim"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location $root

$targetPath = Join-Path $root $TargetDir
$sandbox = Join-Path $targetPath "no-assets"
New-Item -ItemType Directory -Force -Path $sandbox | Out-Null
# 保险：沙箱里绝不能有 resource/（否则就测不出 CI 的缺失场景）
if (Test-Path (Join-Path $sandbox "resource")) {
    throw "沙箱目录里存在 resource/：$(Join-Path $sandbox 'resource')"
}

Write-Host "==> 构建测试二进制（CARGO_TARGET_DIR=$TargetDir）"
$env:CARGO_TARGET_DIR = $targetPath
$buildLog = Join-Path $targetPath "no-run.log"
# 用 cmd 包一层（兼容 PS 5.1；pwsh 7 下 `2>&1` 本身没问题，这样两边行为一致）：PS 5.1 在 $ErrorActionPreference=Stop 下会把原生命令写 stderr 的
# 正常输出（cargo 进度、tracing 日志）当成异常抛出。
cmd /c "cargo test --workspace --no-run > `"$buildLog`" 2>&1"
if ($LASTEXITCODE -ne 0) {
    Get-Content $buildLog -Tail 30 | Out-Host
    throw "cargo test --workspace --no-run 失败（exit $LASTEXITCODE，日志 $buildLog）"
}

$exes = @(Get-ChildItem (Join-Path $targetPath "debug/deps") -Filter *.exe |
    Where-Object { $_.BaseName -match '-[0-9a-f]{16}$' })
Write-Host "==> 共 $($exes.Count) 个测试二进制，工作目录=$sandbox（无 resource/）"

$runner = {
    param($path, $cwd, $logDir)
    Set-Location $cwd
    $log = Join-Path $logDir (($path -split '\\')[-1] + ".log")
    cmd /c "`"$path`" --test-threads=4 > `"$log`" 2>&1" | Out-Null
    $code = $LASTEXITCODE
    $lines = @(Get-Content $log -ErrorAction SilentlyContinue)
    $summary = ($lines | Select-String -Pattern '^test result:' | Select-Object -Last 1)
    [pscustomobject]@{
        Name    = ($path -split '\\')[-1]
        Code    = $code
        Summary = if ($summary) { ($summary -replace '\s+', ' ').Trim() } else { "(no test result line)" }
    }
}

$running = @()
$results = @()
foreach ($exe in $exes) {
    # 节流：同时最多 $Jobs 个
    while (@($running | Where-Object { $_.State -eq 'Running' }).Count -ge $Jobs) {
        foreach ($job in @($running | Where-Object { $_.State -ne 'Running' })) {
            $results += Receive-Job -Job $job -ErrorAction SilentlyContinue
            Remove-Job -Job $job -Force -ErrorAction SilentlyContinue
        }
        $running = @($running | Where-Object { $_.State -eq 'Running' })
        Start-Sleep -Milliseconds 200
    }
    $running += Start-Job -ScriptBlock $runner -ArgumentList $exe.FullName, $sandbox, $sandbox
}

$running | Wait-Job -Timeout $TimeoutSeconds | Out-Null
$results += $running | Receive-Job -ErrorAction SilentlyContinue
$running | Remove-Job -Force -ErrorAction SilentlyContinue

$failed = @($results | Where-Object { $_.Code -ne 0 })
$results | Sort-Object Name | ForEach-Object {
    $flag = if ($_.Code -eq 0) { "OK  " } else { "FAIL" }
    Write-Host ("{0} {1,-44} {2}" -f $flag, $_.Name, $_.Summary)
}
if ($failed.Count -gt 0) {
    Write-Host "==> 失败 $($failed.Count) / $($results.Count) 个测试二进制"
    exit 1
}
Write-Host "==> 全部通过：$($results.Count) 个测试二进制（无 resource/）"
exit 0
