# 巡检例程：迁移演练 + 无素材沙箱 + 关键 PG 用例（第 18 轮 · JJC-20261020-005-sub-f3c95bd6）。
#
# 为什么单独一条脚本：这三步慢到不能塞进 `ci.ps1` 的 10 项门禁（见 `docs/testing/check-routine.md`），
# 但发版前必须确认。发版前 `-Full`，日常 `-Quick`，改过本脚本用 `-SelfTest` 自证「真失败会红」。
#
# 退出码：0 = 全部 PASS；1 = 至少一步 FAIL（`-SelfTest` 也是 1，因为被注入那次本来就该红）；
#         2 = 参数/环境错误（缺 MIR2_DRILL_DATABASE_URL、drill 自身 exit 2、缺可执行文件）。
#
# 无人值守：脚本里没有任何 `Read-Host`；PG 连接串从仓库根的 `.env` 读（`DATABASE_URL` → 换库名 `mir2_test`）。
#
# 注意：本文件存成 **UTF-8 with BOM**（兼容仍用 PS 5.1 的人）。PowerShell 变量名大小写不敏感，
# 所以参数与局部变量刻意取不同名字（例如参数 `$Mode` 与局部 `$mode` 不会同时出现）。

[CmdletBinding(DefaultParameterSetName = 'Full')]
param(
    # 全跑：迁移演练 + 无素材沙箱 + 关键 PG 用例。
    [Parameter(ParameterSetName = 'Full')]
    [switch]$Full,
    # 日常：跳过迁移演练（summary 里标 SKIPPED），只跑无素材沙箱 + 关键 PG 用例。
    [Parameter(ParameterSetName = 'Quick')]
    [switch]$Quick,
    # 自证会红：注入一次**真实失败**（关键 PG 用例指向不存在的库），然后确认正常 `-Quick` 能恢复。
    [Parameter(ParameterSetName = 'SelfTest')]
    [switch]$SelfTest,
    # 长时 soak（**1800s 完整版**）：只在显式开关时跑，绝不并入 -Quick/-Full 的日常路径。
    [Parameter(ParameterSetName = 'Soak')]
    [switch]$Soak,
    # soak 用的 /metrics 地址（-Soak 时必填，默认 127.0.0.1:9090）。
    [string]$SoakMetricsUrl = "http://127.0.0.1:9090/metrics",
    # soak 的服务端地址。
    [string]$SoakServer = "127.0.0.1:7000",
    # soak 时长（秒）。**默认 1800；调小会让 soak 判决必定 FAIL**（不许缩时长）。
    [int]$SoakSeconds = 1800,
    # 证据目录（默认 tools/dev/evidence/check-routine/<yyyyMMdd-HHmmss>）。
    [string]$EvidenceDir = "",
    # 无素材沙箱的编译 target 目录（透传给 test-no-assets.ps1）。
    [string]$NoAssetsTargetDir = "target/ci-sim",
    # 无素材沙箱里同时跑几个测试二进制（透传）。
    [int]$NoAssetsJobs = 4
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location $root
$PSDefaultParameterValues['Out-File:Encoding'] = 'utf8'

# ---------------------------------------------------------------------------
# 基础工具
# ---------------------------------------------------------------------------

function Write-Step([string]$Text) { Write-Host ""; Write-Host "==> $Text" -ForegroundColor Cyan }

# 从 `.env` 里读一个键（不打印值；找不到返回 $null）。
function Get-DotEnvValue([string]$Key) {
    $envFile = Join-Path $root '.env'
    if (-not (Test-Path $envFile)) { return $null }
    foreach ($line in Get-Content $envFile) {
        if ($line -match "^\s*$([regex]::Escape($Key))\s*=(.*)$") {
            return $Matches[1].Trim()
        }
    }
    return $null
}

# 把 `postgres://.../dbname` 换成另一个库名（保留账号/主机/端口/参数）。
function Switch-DatabaseName([string]$Url, [string]$Database) {
    if ($Url -match '^([^?]*/)([^/?]+)(\?.*)?$') {
        return "$($Matches[1])$Database$($Matches[3])"
    }
    throw "无法从连接串里替换库名：$Url"
}

# 证据目录里的相对路径（summary 里尽量写相对路径，方便换机器复核）。
function Get-RelativeEvidencePath([string]$Path) {
    $prefix = (Join-Path $root '') 
    if ($Path.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
        return ($Path.Substring($prefix.Length) -replace '\\', '/')
    }
    return $Path
}

# ---------------------------------------------------------------------------
# 模式与环境准备
# ---------------------------------------------------------------------------

$mode = if ($Quick) { 'Quick' } elseif ($SelfTest) { 'SelfTest' } elseif ($Soak) { 'Soak' } else { 'Full' }
# Soak 是独立路径：不跑迁移演练、也不跑日常两步（否则一次 30 分钟以上）。
$runDrill = $mode -notin @('Quick', 'Soak')

$stamp = (Get-Date).ToString('yyyyMMdd-HHmmss')
if ([string]::IsNullOrWhiteSpace($EvidenceDir)) {
    $EvidenceDir = Join-Path $root (Join-Path 'tools/dev/evidence/check-routine' $stamp)
}
New-Item -ItemType Directory -Force -Path $EvidenceDir | Out-Null

# PG 连接串：优先沿用外部已设的 `MIR2_TEST_DATABASE_URL`，否则从 `.env` 的 `DATABASE_URL` 换库名到 `mir2_test`。
$testDatabaseUrl = $env:MIR2_TEST_DATABASE_URL
$pgSource = 'MIR2_TEST_DATABASE_URL（环境变量）'
if ([string]::IsNullOrWhiteSpace($testDatabaseUrl)) {
    $baseUrl = Get-DotEnvValue 'DATABASE_URL'
    if ([string]::IsNullOrWhiteSpace($baseUrl)) {
        Write-Host "[REFUSED] 既没有 MIR2_TEST_DATABASE_URL，也读不到 .env 的 DATABASE_URL（exit 2）" -ForegroundColor Red
        exit 2
    }
    $testDatabaseUrl = Switch-DatabaseName $baseUrl 'mir2_test'
    $pgSource = '.env 的 DATABASE_URL（库名换成 mir2_test）'
}

# 迁移演练：只认 MIR2_DRILL_DATABASE_URL；缺了就是环境错误（exit 2），绝不「悄悄跳过」。
if ($runDrill) {
    if ([string]::IsNullOrWhiteSpace($env:MIR2_DRILL_DATABASE_URL)) {
        Write-Host "[REFUSED] -$mode 需要 MIR2_DRILL_DATABASE_URL（迁移演练专用库；缺它 exit 2，不会跳过）" -ForegroundColor Red
        Write-Host "          例：`$env:MIR2_DRILL_DATABASE_URL = 'postgres://postgres:<pw>@nas:3001/mir2_drill'"
        Write-Host "              `$env:MIR2_DRILL_ALLOWED_HOSTS = 'nas'   # 非环回 host 必填"
        exit 2
    }
}

$drillScript = Join-Path $root 'tools/dev/migrate_drill.ps1'
$noAssetsScript = Join-Path $root 'tools/dev/test-no-assets.ps1'
foreach ($required in @($noAssetsScript)) {
    if (-not (Test-Path $required)) {
        Write-Host "[REFUSED] 找不到必需脚本：$required（exit 2）" -ForegroundColor Red
        exit 2
    }
}
if ($runDrill -and -not (Test-Path $drillScript)) {
    Write-Host "[REFUSED] 找不到迁移演练脚本：$drillScript（exit 2）" -ForegroundColor Red
    exit 2
}

# ---------------------------------------------------------------------------
# 步骤定义：每一步 = 一个真实外部命令（不伪造返回值）
# ---------------------------------------------------------------------------

$steps = New-Object System.Collections.Generic.List[object]

if ($mode -eq 'Soak') {
    $soakScript = Join-Path $root 'tools/stress/soak.py'
    if (-not (Test-Path $soakScript)) {
        Write-Host "[REFUSED] 找不到 soak 脚本：$soakScript（exit 2）" -ForegroundColor Red
        exit 2
    }
    $soakOut = Join-Path $root ("docs/testing/soak-data/" + $stamp)
    $steps.Add([pscustomobject]@{
        Name        = '01-soak'
        Title       = "长时 soak（$SoakSeconds s / 100 连接 / 10s 采样）"
        # soak.py 用 2 表示环境错误（抓不到 /metrics），与巡检的 2 = 环境错误口径一致。
        EnvTreat2As = 'environment'
        FilePath    = 'py'
        Arguments   = @(
            $soakScript,
            '--server', $SoakServer,
            '--metrics-url', $SoakMetricsUrl,
            '--clients', '100',
            '--duration', "$SoakSeconds",
            '--sample-interval', '10',
            '--out-dir', $soakOut
        )
    })
}

if ($runDrill) {
    $drillEvidence = Join-Path $EvidenceDir 'migrate-drill.md'
    $steps.Add([pscustomobject]@{
        # `migrate_drill.ps1` 自身会拒绝错误配置并以 2 退出；那属于环境错误，由主流程升级为 exit 2。
        Name       = '01-migrate-drill'
        Title      = '迁移演练（0013→0016，含 0014 空洞）'
        EnvTreat2As = 'environment'
        FilePath   = 'pwsh'
        Arguments  = @('-NoProfile', '-File', $drillScript, '-Repeat', '1', '-EvidencePath', $drillEvidence)
    })
}

if ($mode -eq 'Soak') {
    # soak 是独占路径：不追加无素材沙箱与 PG 用例（30 分钟跑完再排它们没有意义）。
} else {
$steps.Add([pscustomobject]@{
    Name       = '02-no-assets'
    Title      = '无素材沙箱（没有 resource/ 时全套测试必须全绿）'
    EnvTreat2As = 'fail'
    FilePath   = 'pwsh'
    Arguments  = @('-NoProfile', '-File', $noAssetsScript, '-Jobs', "$NoAssetsJobs", '-TargetDir', $NoAssetsTargetDir)
})

$pgUrlForRun = $testDatabaseUrl
$pgInjectionNote = ''
$pgInjectionDatabase = ''
if ($mode -eq 'SelfTest') {
    # 真实失败注入：把库名换成一个**不存在**的库 → 连接失败 → 该步真红。
    $pgInjectionDatabase = "mir2_check_routine_missing_$stamp"
    $pgUrlForRun = Switch-DatabaseName $testDatabaseUrl $pgInjectionDatabase
    $pgInjectionNote = "注入：MIR2_TEST_DATABASE_URL 的库名改成不存在的 `"$pgInjectionDatabase`""
}

$steps.Add([pscustomobject]@{
    Name        = '03-postgres'
    Title       = '关键 PG 用例（4 条 postgres_*，真落 PostgreSQL）'
    EnvTreat2As = 'fail'
    FilePath    = 'cargo'
    Arguments   = @('test', '-p', 'mir-server', '--no-fail-fast', 'postgres_')
    ExtraEnv    = @{ MIR2_TEST_DATABASE_URL = $pgUrlForRun }
})
}

# ---------------------------------------------------------------------------
# 执行
# ---------------------------------------------------------------------------

$results = New-Object System.Collections.Generic.List[object]
$startedAt = Get-Date

foreach ($step in $steps) {
    Write-Step "$($step.Title)"
    $logPath = Join-Path $EvidenceDir "$($step.Name).log"
    $savedEnv = @{}
    foreach ($key in $step.ExtraEnv.Keys) {
        $savedEnv[$key] = [Environment]::GetEnvironmentVariable($key)
        [Environment]::SetEnvironmentVariable($key, $step.ExtraEnv[$key])
    }
    $stepStart = Get-Date
    try {
        # 用 cmd 包一层做重定向：PS 5.1 在 $ErrorActionPreference=Stop 下会把原生命令的 stderr 当成异常。
        $quoted = ($step.Arguments | ForEach-Object { if ($_ -match '[\s"]') { '"' + ($_ -replace '"', '\"') + '"' } else { $_ } }) -join ' '
        $commandLine = "$($step.FilePath) $quoted > `"$logPath`" 2>&1"
        cmd /c $commandLine | Out-Null
        $exitCode = $LASTEXITCODE
    } finally {
        foreach ($key in $savedEnv.Keys) {
            [Environment]::SetEnvironmentVariable($key, $savedEnv[$key])
        }
    }
    $elapsed = [math]::Round(((Get-Date) - $stepStart).TotalSeconds, 1)
    $status = if ($exitCode -eq 0) { 'PASS' } elseif ($exitCode -eq 2 -and $step.EnvTreat2As -eq 'environment') { 'ENVERR' } else { 'FAIL' }
    $results.Add([pscustomobject]@{
        Name    = $step.Name
        Title   = $step.Title
        Status  = $status
        Code    = $exitCode
        Seconds = $elapsed
        Log     = (Get-RelativeEvidencePath $logPath)
    })
    $color = switch ($status) { 'PASS' { 'Green' } 'FAIL' { 'Red' } default { 'Yellow' } }
    Write-Host ("    {0,-6} exit={1,-3} {2,6}s  {3}" -f $status, $exitCode, $elapsed, $step.Name) -ForegroundColor $color
}

$finishedAt = Get-Date
$failed = @($results | Where-Object { $_.Status -eq 'FAIL' })
$envErrors = @($results | Where-Object { $_.Status -eq 'ENVERR' })

# ---------------------------------------------------------------------------
# summary.md
# ---------------------------------------------------------------------------

$summary = Join-Path $EvidenceDir 'summary.md'
$lines = New-Object System.Collections.Generic.List[string]
$lines.Add("# check-routine 巡检汇总（$mode）")
$lines.Add('')
$lines.Add("- 开始：$($startedAt.ToString('yyyy-MM-ddTHH:mm:ss'))（本地）")
$lines.Add("- 结束：$($finishedAt.ToString('yyyy-MM-ddTHH:mm:ss'))（本地）")
$lines.Add("- 总耗时：$([math]::Round(($finishedAt - $startedAt).TotalSeconds, 1))s")
$lines.Add("- 仓库根：$root")
$lines.Add("- PG 用例连接串来源：$pgSource")
if ($mode -eq 'Quick') {
    $lines.Add('- **迁移演练：SKIPPED**（`-Quick` 模式跳过；发版前请用 `-Full`）')
}
if ($mode -eq 'Soak') {
    $lines.Add("- **soak 独立路径**：只跑长时 soak（$SoakSeconds s / 100 连接 / 10s 采样），不含迁移演练与无素材沙箱；日常请用 `-Quick`、发版前用 `-Full`。")
}
if ($pgInjectionNote) {
    $lines.Add("- **故障注入（-SelfTest）**：$pgInjectionNote")
}
$lines.Add('')
$lines.Add('| # | 步骤 | 结果 | 退出码 | 耗时 | 日志 |')
$lines.Add('|---|------|------|--------|------|------|')
$index = 0
foreach ($result in $results) {
    $index++
    $mark = switch ($result.Status) {
        'PASS' { 'PASS' }
        'FAIL' { '**FAIL**' }
        'ENVERR' { 'ENVERR（环境错误）' }
        default { $result.Status }
    }
    $lines.Add("| $index | $($result.Title)<br>``$($result.Name)`` | $mark | $($result.Code) | $($result.Seconds)s | ``$($result.Log)`` |")
}
$lines.Add('')
$lines.Add('## 结论')
if ($envErrors.Count -gt 0) {
    $lines.Add("- **环境错误**：$($envErrors.Count) 步以 2 退出（$((@($envErrors | ForEach-Object { $_.Name })) -join ', ')）→ 巡检退出码 2。")
} elseif ($failed.Count -gt 0) {
    $lines.Add("- **失败**：$($failed.Count)/$($results.Count) 步 FAIL（$((@($failed | ForEach-Object { $_.Name })) -join ', ')）→ 巡检退出码 1。")
} else {
    $lines.Add("- **全部通过**：$($results.Count)/$($results.Count) 步 PASS → 巡检退出码 0。")
}
$lines.Add('')
$lines.Add('## 复核命令')
$lines.Add('```powershell')
$lines.Add('$env:MIR2_DRILL_DATABASE_URL = "<drill url>"')
$lines.Add('pwsh -NoProfile -File tools/dev/check-routine.ps1 -Full')
$lines.Add('pwsh -NoProfile -File tools/dev/check-routine.ps1 -Quick')
$lines.Add('pwsh -NoProfile -File tools/dev/check-routine.ps1 -SelfTest')
$lines.Add('```')

# ---------------------------------------------------------------------------
# -SelfTest：四条断言 + 恢复运行
# ---------------------------------------------------------------------------

$selfTestFailures = New-Object System.Collections.Generic.List[string]
$recoveryExit = $null
if ($mode -eq 'SelfTest') {
    Write-Step "自检断言（四条）"
    # ① 巡检进程整体应是「失败」语义：被注入那步真红了。
    $injected = $results | Where-Object { $_.Name -eq '03-postgres' }
    $others = @($results | Where-Object { $_.Name -ne '03-postgres' })
    if (-not $injected -or $injected.Status -ne 'FAIL') {
        $selfTestFailures.Add("注入步骤 03-postgres 期望 FAIL，实际 $($injected.Status)")
    }
    # ② 只有被注入那一步 FAIL。
    $otherBad = @($others | Where-Object { $_.Status -ne 'PASS' })
    if ($otherBad.Count -gt 0) {
        $selfTestFailures.Add("除注入步骤外还有非 PASS：$((@($otherBad | ForEach-Object { "$($_.Name)=$($_.Status)" })) -join ', ')")
    }
    # ③ 该步日志含注入错误的原文（数据库不存在）。
    $injectedLog = Join-Path $EvidenceDir '03-postgres.log'
    $logText = if (Test-Path $injectedLog) { Get-Content $injectedLog -Raw } else { '' }
    $hasErrorText = ($logText -match [regex]::Escape($pgInjectionDatabase)) -and ($logText -match 'does not exist|不存在')
    if (-not $hasErrorText) {
        $selfTestFailures.Add("03-postgres.log 未包含注入错误原文（期望含 `"$pgInjectionDatabase`" 与 does not exist）")
    }
    # ④ 随后跑一次正常 -Quick 必须 exit 0（证明不是恒失败）。
    Write-Step "恢复验证：正常 -Quick 必须 exit 0"
    $quickEvidence = Join-Path $EvidenceDir 'recovery-quick'
    $savedTestUrl = $env:MIR2_TEST_DATABASE_URL
    $env:MIR2_TEST_DATABASE_URL = $testDatabaseUrl
    try {
        cmd /c "pwsh -NoProfile -File `"$PSCommandPath`" -Quick -EvidenceDir `"$quickEvidence`" -NoAssetsTargetDir `"$NoAssetsTargetDir`" -NoAssetsJobs $NoAssetsJobs > `"$(Join-Path $EvidenceDir 'recovery-quick.log')`" 2>&1" | Out-Null
        $recoveryExit = $LASTEXITCODE
    } finally {
        $env:MIR2_TEST_DATABASE_URL = $savedTestUrl
    }
    Write-Host ("    恢复运行 -Quick exit=$recoveryExit")
    if ($recoveryExit -ne 0) {
        $selfTestFailures.Add("恢复运行 -Quick 期望 exit 0，实际 $recoveryExit（日志 recovery-quick.log）")
    }
    foreach ($failure in $selfTestFailures) { Write-Host "    [SELFTEST] FAIL：$failure" -ForegroundColor Red }
    $lines.Add('')
    $lines.Add('## 自检断言（-SelfTest）')
    $lines.Add('')
    $lines.Add('| 断言 | 结果 |')
    $lines.Add('|------|------|')
    $lines.Add("| ① 注入步骤 FAIL（真实失败，非伪造） | $(if ($injected -and $injected.Status -eq 'FAIL') { 'PASS' } else { 'FAIL' }) |")
    $lines.Add("| ② 其它步骤全部 PASS | $(if ($otherBad.Count -eq 0) { 'PASS' } else { 'FAIL' }) |")
    $lines.Add("| ③ 该步日志含注入错误原文 | $(if ($hasErrorText) { 'PASS' } else { 'FAIL' }) |")
    $lines.Add("| ④ 随后正常 `-Quick` exit 0 | $(if ($recoveryExit -eq 0) { 'PASS' } else { 'FAIL' }) |")
    $lines.Add('')
    $lines.Add("- 恢复运行证据：``recovery-quick/summary.md``、``recovery-quick.log``")
    if ($selfTestFailures.Count -eq 0) {
        $lines.Add('- **负向自检通过**（本进程仍以 1 退出：被注入的那次运行确实红了，这是预期）。')
    } else {
        $lines.Add("- **负向自检未通过**：$($selfTestFailures.Count) 条断言不成立。")
    }
}

$lines | Out-File -FilePath $summary -Encoding utf8

# 控制台汇总
Write-Step "汇总"
$results | Sort-Object Name | ForEach-Object {
    $flag = switch ($_.Status) { 'PASS' { 'OK  ' } 'FAIL' { 'FAIL' } default { 'ENV ' } }
    Write-Host ("    {0} {1,-18} exit={2,-3} {3,6}s  {4}" -f $flag, $_.Name, $_.Code, $_.Seconds, $_.Log)
}
if ($mode -eq 'Quick') { Write-Host "    SKIP 01-migrate-drill  （-Quick 跳过迁移演练）" -ForegroundColor Yellow }
if ($mode -eq 'Soak') { Write-Host "    （-Soak 独立路径：不含迁移演练与无素材沙箱）" -ForegroundColor Yellow }
Write-Host "    证据目录：$(Get-RelativeEvidencePath $EvidenceDir)" -ForegroundColor Cyan

if ($envErrors.Count -gt 0) {
    Write-Host "[EXIT 2] 环境错误：$($envErrors.Count) 步（见 summary.md）" -ForegroundColor Yellow
    exit 2
}
if ($mode -eq 'SelfTest') {
    if ($selfTestFailures.Count -eq 0 -and $failed.Count -eq 1) {
        Write-Host "[SELFTEST] PASS：注入失败确实变红，且正常 -Quick 已恢复（本进程 exit 1 属预期）" -ForegroundColor Green
    } else {
        Write-Host "[SELFTEST] FAIL：负向自检未达到预期（见 summary.md 的自检断言表）" -ForegroundColor Red
    }
    exit 1
}
if ($failed.Count -gt 0) {
    Write-Host "[FAIL] $($failed.Count)/$($results.Count) 步失败（见 summary.md）" -ForegroundColor Red
    exit 1
}
Write-Host "[OK] $($results.Count)/$($results.Count) 步全部通过" -ForegroundColor Green
exit 0
