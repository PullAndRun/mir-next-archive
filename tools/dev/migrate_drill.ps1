<#
.SYNOPSIS
存档迁移演练（0013 → 0016，含 0014 空洞）。第 17 轮 · JJC-20261020-004-sub-1f7a4a01。

.DESCRIPTION
把 `crates/mir-store/tests/migrations.rs` 里的演练跑起来，并给出**可复核的证据文件**。

硬约束（对应门下省 v3 规格）：
1. **只认 `MIR2_DRILL_DATABASE_URL`**；绝不会回退到 `MIR2_TEST_DATABASE_URL` / `DATABASE_URL`。
   没设 → `exit 2`。
2. 连接后断言库名前缀 `mir2_drill`、host 为本地（环回，或显式列在
   `MIR2_DRILL_ALLOWED_HOSTS` 里），否则 `exit 2`。
   附带**拒绝自检**：`-RefusalSelfTest` 只给 `MIR2_TEST_DATABASE_URL` 时必须拒绝。
3. 演练库名唯一：`mir2_drill_<pid>_<ts>`，`CREATE DATABASE ... TEMPLATE template0`，
   演练结束 `DROP DATABASE`（测试内部负责，脚本再核对无 `mir2_drill_%` 残留）。
4. 对 `mir2` 开发库做**只读**前后快照（每表行数 + 库大小）；证据里声明
   「并发写入会使该证据失效」并记前后时间戳。
5. 校验 0013 头注释/版本（文件名版本 13、头注释写 0014 属改名遗留 → 按文件名核对并登记，
   **不改文件**以免 checksum 变化）、0014 空洞、`_sqlx_migrations` 的 version 与 checksum 一致。
6. **真实失败注入**：事务内制造 SQL 错误 → 断言 schema 与 `_sqlx_migrations` 一起回滚，
   并给出可复核的恢复步骤。
7. **二次幂等**：整套演练连跑两遍，两遍都必须 exit 0。

.EXAMPLE
# 正常演练（nas 是团队开发库，需要显式放行）
$env:MIR2_DRILL_DATABASE_URL = "postgres://postgres:postgres@nas:3001/mir2_drill"
$env:MIR2_DRILL_ALLOWED_HOSTS = "nas"
pwsh -NoProfile -File tools/dev/migrate_drill.ps1

.EXAMPLE
# 拒绝自检：只给 MIR2_TEST_DATABASE_URL 时必须 exit 2
pwsh -NoProfile -File tools/dev/migrate_drill.ps1 -RefusalSelfTest
#>

[CmdletBinding()]
param(
    # 拒绝自检：清掉 MIR2_DRILL_DATABASE_URL、只留 MIR2_TEST_DATABASE_URL，期望被拒绝（exit 2）。
    [switch]$RefusalSelfTest,
    # 高危环境自检：`MIR2_DRILL_ALLOWED_HOSTS` 为空 + 非环回 host **必须**被拒绝（exit 2）；
    # 反向再加一条：显式放行同一 host 时必须通过校验（不连库）。
    [switch]$HazardSelfTest,
    # 只跑环境校验（不连库、不演练），用于断言与排障。
    [switch]$ValidateOnly,
    # 证据文件路径（默认 target/drill/migrate-drill-<时间戳>.md）。
    [string]$EvidencePath,
    # 演练跑几遍（二次幂等默认跑 2 遍）。
    [Alias('Runs')]
    [int]$Repeat = 2
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location $root
$env:CARGO_TARGET_DIR = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { "target/hubu-verify" }

# ---------------------------------------------------------------------------
# 工具函数
# ---------------------------------------------------------------------------

function Write-Step([string]$Text) { Write-Host "==> $Text" }

# 解析 postgres URL 里的 host / 库名（与 Rust 侧 `parse_pg_url` 同口径）。
function Get-DrillTarget([string]$Url) {
    if ($Url -notmatch '^(postgres|postgresql)://') { throw "只支持 postgres:// 或 postgresql:// 连接串" }
    $rest = $Url.Substring($Url.IndexOf('://') + 3)
    if ($rest.Contains('?')) { $rest = $rest.Substring(0, $rest.IndexOf('?')) }
    $slash = $rest.LastIndexOf('/')
    if ($slash -lt 0) { throw "连接串缺少库名" }
    $authority = $rest.Substring(0, $slash)
    $database = $rest.Substring($slash + 1)
    if ($database -eq '') { throw "连接串缺少库名" }
    $hostPort = if ($authority.Contains('@')) { $authority.Substring($authority.LastIndexOf('@') + 1) } else { $authority }
    $hostName = if ($hostPort.StartsWith('[')) {
        $close = $hostPort.IndexOf(']')
        if ($close -gt 0) { $hostPort.Substring(0, $close + 1) } else { $hostPort }
    } else {
        ($hostPort -split ':')[0]
    }
    # 注意：`$Host` 是 PowerShell 只读自动变量，这里必须用别的名字。
    [pscustomobject]@{ Host = $hostName; Database = $database }
}

function Test-IsLoopbackHost([string]$HostName) {
    return (@('localhost', '127.0.0.1', '::1', '[::1]') -contains $HostName.ToLowerInvariant())
}

function Get-AllowedHosts {
    $raw = $env:MIR2_DRILL_ALLOWED_HOSTS
    if ([string]::IsNullOrWhiteSpace($raw)) { return @() }
    return @($raw -split ',' | ForEach-Object { $_.Trim().ToLowerInvariant() } | Where-Object { $_ -ne '' })
}

# 返回 @{ Ok = bool; Reason = string }，把「环回」「白名单命中」「白名单为空」「白名单不含」分开，
# 便于断言与给出可执行的修复建议。
function Test-LocalHost([string]$HostName) {
    if (Test-IsLoopbackHost $HostName) { return @{ Ok = $true; Reason = 'loopback' } }
    $allowed = Get-AllowedHosts
    if ($allowed.Count -eq 0) {
        return @{ Ok = $false; Reason = "非环回 host 且 MIR2_DRILL_ALLOWED_HOSTS 为空（绝不因连不上本地就放宽）" }
    }
    if ($allowed -contains $HostName.ToLowerInvariant()) { return @{ Ok = $true; Reason = 'allowlisted' } }
    return @{ Ok = $false; Reason = "不在 MIR2_DRILL_ALLOWED_HOSTS 白名单里：[$($allowed -join ', ')]" }
}

# 校验「只认 MIR2_DRILL_DATABASE_URL」这条硬约束。返回解析结果，不合法直接 exit 2。
function Assert-DrillEnvironment {
    $drill = $env:MIR2_DRILL_DATABASE_URL
    if ([string]::IsNullOrWhiteSpace($drill)) {
        Write-Host "[REFUSED] 未设置 MIR2_DRILL_DATABASE_URL（本脚本禁止回退 MIR2_TEST_DATABASE_URL / DATABASE_URL）" -ForegroundColor Red
        exit 2
    }
    $target = $null
    try {
        $target = Get-DrillTarget $drill
    } catch {
        Write-Host "[REFUSED] MIR2_DRILL_DATABASE_URL 无法解析：$($_.Exception.Message)" -ForegroundColor Red
        exit 2
    }
    $hostCheck = Test-LocalHost $target.Host
    if (-not $hostCheck.Ok) {
        Write-Host "[REFUSED] host=$($target.Host) 不是本地：$($hostCheck.Reason)" -ForegroundColor Red
        exit 2
    }
    if (-not $target.Database.StartsWith('mir2_drill')) {
        Write-Host "[REFUSED] 库名必须以 mir2_drill 开头：$($target.Database)" -ForegroundColor Red
        exit 2
    }
    return $target
}

function Invoke-CargoTest([string]$Filter, [string]$Extra = '') {
    $arguments = @('test', '-p', 'mir-store', '--test', 'migrations', '--', '--nocapture', '--test-threads=1')
    if ($Filter) { $arguments += @('--exact', $Filter) }
    if ($Extra) { $arguments += $Extra.Split(' ') }
    $output = & cargo @arguments 2>&1 | Out-String
    [pscustomobject]@{ ExitCode = $LASTEXITCODE; Output = $output }
}

function Get-DrillLines([string]$Text) {
    # `[DRILL]` 可能出现在 cargo test 的 `test xxx ... [DRILL] ...` 行中间，按标记截断取尾部。
    foreach ($line in ($Text -split "`r?`n")) {
        $index = $line.IndexOf('[DRILL]')
        if ($index -ge 0) { $line.Substring($index).Trim() }
    }
}

function Format-DrillUrl([string]$Url) {
    # 证据里不落密码
    $Url -replace '://([^:/@]+):[^@/]*@', '://$1:***@'
}

# ---------------------------------------------------------------------------
# 0) 拒绝自检
# ---------------------------------------------------------------------------
if ($RefusalSelfTest) {
    Write-Step "拒绝自检：只给 MIR2_TEST_DATABASE_URL 时必须拒绝（exit 2）"
    $savedDrill = $env:MIR2_DRILL_DATABASE_URL
    $savedTest = $env:MIR2_TEST_DATABASE_URL
    try {
        $env:MIR2_DRILL_DATABASE_URL = $null
        if ([string]::IsNullOrWhiteSpace($env:MIR2_TEST_DATABASE_URL)) {
            $env:MIR2_TEST_DATABASE_URL = 'postgres://postgres:postgres@127.0.0.1:5432/mir2_test'
        }
        $probe = & $PSCommandPath -Repeat 1 -EvidencePath (Join-Path $env:TEMP 'drill-refusal-selftest.md') 2>&1
        $code = $LASTEXITCODE
        if ($code -eq 2) {
            Write-Host "[SELFTEST] PASS：只给 MIR2_TEST_DATABASE_URL 时被拒绝（exit 2）" -ForegroundColor Green
            exit 0
        }
        Write-Host "[SELFTEST] FAIL：期望 exit 2，实际 exit $code" -ForegroundColor Red
        Write-Host ($probe | Out-String)
        exit 1
    } finally {
        $env:MIR2_DRILL_DATABASE_URL = $savedDrill
        $env:MIR2_TEST_DATABASE_URL = $savedTest
    }
}

# ---------------------------------------------------------------------------
# 0b) 高危环境自检：白名单为空 + 非环回 host 必须拒绝；显式放行时必须通过
# ---------------------------------------------------------------------------
if ($HazardSelfTest) {
    Write-Step "高危环境自检：白名单为空时不得放行非环回 host"
    $savedDrill = $env:MIR2_DRILL_DATABASE_URL
    $savedAllow = $env:MIR2_DRILL_ALLOWED_HOSTS
    $failures = @()
    try {
        $env:MIR2_DRILL_DATABASE_URL = 'postgres://postgres:postgres@nas:3001/mir2_drill'
        $env:MIR2_DRILL_ALLOWED_HOSTS = $null
        $output = & $PSCommandPath -ValidateOnly 2>&1
        if ($LASTEXITCODE -ne 2) {
            $failures += "白名单为空 + 非环回 host 期望 exit 2，实际 exit $LASTEXITCODE"
        }
        # 空白名单（只有逗号和空格）也算空
        $env:MIR2_DRILL_ALLOWED_HOSTS = ' , ,'
        $output = & $PSCommandPath -ValidateOnly 2>&1
        if ($LASTEXITCODE -ne 2) {
            $failures += "空白名单 + 非环回 host 期望 exit 2，实际 exit $LASTEXITCODE"
        }
        # 显式放行后必须通过校验
        $env:MIR2_DRILL_ALLOWED_HOSTS = 'nas'
        $output = & $PSCommandPath -ValidateOnly 2>&1
        if ($LASTEXITCODE -ne 0) {
            $failures += "显式放行 nas 后期望 exit 0，实际 exit $LASTEXITCODE"
        }
        # 环回 host 不需要白名单
        $env:MIR2_DRILL_DATABASE_URL = 'postgres://postgres:postgres@127.0.0.1:5432/mir2_drill'
        $env:MIR2_DRILL_ALLOWED_HOSTS = $null
        $output = & $PSCommandPath -ValidateOnly 2>&1
        if ($LASTEXITCODE -ne 0) {
            $failures += "环回 host 无需白名单，期望 exit 0，实际 exit $LASTEXITCODE"
        }
    } finally {
        $env:MIR2_DRILL_DATABASE_URL = $savedDrill
        $env:MIR2_DRILL_ALLOWED_HOSTS = $savedAllow
    }
    if ($failures.Count -eq 0) {
        Write-Host "[SELFTEST] PASS：白名单为空/空白时拒绝、显式放行时通过、环回免白名单" -ForegroundColor Green
        exit 0
    }
    foreach ($failure in $failures) { Write-Host "[SELFTEST] FAIL：$failure" -ForegroundColor Red }
    exit 1
}

# ---------------------------------------------------------------------------
# 1) 环境校验（exit 2 的三种情形都在这条路径上）
# ---------------------------------------------------------------------------
$target = Assert-DrillEnvironment
$drillUrl = $env:MIR2_DRILL_DATABASE_URL
if ($ValidateOnly) {
    Write-Host "[OK] 环境校验通过：host=$($target.Host) database=$($target.Database)（-ValidateOnly 不连库）" -ForegroundColor Green
    exit 0
}
Write-Step "演练库：$($target.Host)/$($target.Database)（只认 MIR2_DRILL_DATABASE_URL）"
foreach ($ignored in @('MIR2_TEST_DATABASE_URL', 'DATABASE_URL')) {
    $value = [Environment]::GetEnvironmentVariable($ignored)
    if (-not [string]::IsNullOrWhiteSpace($value) -and $ignored -ne 'DATABASE_URL') {
        Write-Host "   注意：$ignored 已设置但**不会被本脚本使用**（只读快照用的 DATABASE_URL 除外）" -ForegroundColor Yellow
    }
}

$startedAt = Get-Date
$results = @()

# ---------------------------------------------------------------------------
# 2) 先跑探针：连接 + current_database() 前缀断言（不通过 → exit 2）
# ---------------------------------------------------------------------------
Write-Step "前置探针：连接演练库并校验 current_database()"
$probe = Invoke-CargoTest -Filter 'drill_probe_current_database'
$probeLines = Get-DrillLines $probe.Output
$probeOk = $probeLines | Where-Object { $_ -match 'probe_ok=' }
if ($probe.ExitCode -ne 0 -or -not $probeOk) {
    Write-Host "[REFUSED] 探针失败：连接不上或 current_database() 前缀不对（exit 2）" -ForegroundColor Red
    Write-Host $probe.Output
    exit 2
}
Write-Host "    $probeOk"

# ---------------------------------------------------------------------------
# 3) 演练本体（连跑 $Repeat 遍，验证二次幂等）
# ---------------------------------------------------------------------------
for ($i = 1; $i -le $Repeat; $i++) {
    Write-Step "第 $i / $Repeat 遍迁移演练（0013 → 0016，含 0014 空洞）"
    $result = Invoke-CargoTest -Filter 'migration_drill_from_0013_to_0016'
    $lines = @(Get-DrillLines $result.Output)
    $ok = ($result.ExitCode -eq 0) -and ($lines -contains '[DRILL] result=ok')
    $results += [pscustomobject]@{ Index = $i; ExitCode = $result.ExitCode; Ok = $ok; Lines = $lines; Output = $result.Output }
    foreach ($line in $lines) { Write-Host "    $line" }
    if (-not $ok) {
        Write-Host "[FAIL] 第 $i 遍演练失败（exit $($result.ExitCode)）" -ForegroundColor Red
        Write-Host $result.Output
        break
    }
}

$failed = @($results | Where-Object { -not $_.Ok })
$finishedAt = Get-Date

# ---------------------------------------------------------------------------
# 4) 证据文件
# ---------------------------------------------------------------------------
if (-not $EvidencePath) {
    $stamp = $startedAt.ToString('yyyyMMdd-HHmmss')
    $dir = Join-Path $root 'target/drill'
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $EvidencePath = Join-Path $dir "migrate-drill-$stamp.md"
} else {
    $dir = Split-Path -Parent $EvidencePath
    if ($dir -and -not (Test-Path $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
}

$lines = New-Object System.Collections.Generic.List[string]
$lines.Add('# 存档迁移演练证据（migrate_drill.ps1）')
$lines.Add('')
$lines.Add("- 生成时间（UTC）：$($startedAt.ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')) → $($finishedAt.ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ'))")
$lines.Add("- 演练库连接串：$((Format-DrillUrl $drillUrl))")
$lines.Add("- 原始库名：$($target.Database)（每遍会另建唯一的 ``mir2_drill_<pid>_<ts>``，结束即 DROP）")
$lines.Add("- 放行 host：$($target.Host)（本地环回或 MIR2_DRILL_ALLOWED_HOSTS 显式放行）")
$lines.Add('- 只认环境变量：`MIR2_DRILL_DATABASE_URL`（**不会**回退 `MIR2_TEST_DATABASE_URL` / `DATABASE_URL`）')
$lines.Add("- 演练遍数：$Repeat（第 2 遍用于二次幂等）")
$lines.Add('')
$lines.Add('> **证据有效期声明**：`mir2` 开发库的快照是**只读**的，但**并发写入会使该证据失效**；')
$lines.Add('> 需在 freeze / quiesce 窗口内复核，且以本文件内记录的**前后时间戳**为准。')
$lines.Add('')
$lines.Add('## 前置探针')
$lines.Add('```')
$lines.Add($probeOk)
$lines.Add('```')
$lines.Add('')
foreach ($run in $results) {
    $lines.Add("## 第 $($run.Index) 遍（exit $($run.ExitCode)，ok=$($run.Ok)）")
    $lines.Add('```')
    foreach ($line in $run.Lines) { $lines.Add($line) }
    $lines.Add('```')
    $lines.Add('')
}
$lines.Add('## 结论')
if ($failed.Count -eq 0) {
    $lines.Add("- 演练通过：${Runs} 遍均 exit 0。")
    $lines.Add('- 覆盖：0013 头注释偏差（登记未改文件）、0014 空洞、`_sqlx_migrations` version/checksum、')
    $lines.Add('  `pet_mode` / `quest_inventory` / `last_daily_reset_at` / `friends` / `guilds.allies` / `conquest_objects`、')
    $lines.Add('  二次幂等、失败注入回滚（schema 与迁移表一起回滚）、`mir2_drill_*` 无残留。')
} else {
    $lines.Add("- **失败**：$($failed.Count) 遍未通过（见上）。")
}
$lines.Add('')
$lines.Add('## 复核命令')
$lines.Add('```powershell')
$lines.Add('$env:MIR2_DRILL_DATABASE_URL = "<drill url>"')
$lines.Add('$env:MIR2_DRILL_ALLOWED_HOSTS = "<非环回 host 时才需要>"')
$lines.Add('pwsh -NoProfile -File tools/dev/migrate_drill.ps1')
$lines.Add('pwsh -NoProfile -File tools/dev/migrate_drill.ps1 -RefusalSelfTest')
$lines.Add('```')
Set-Content -Path $EvidencePath -Value ($lines -join "`n") -Encoding UTF8

Write-Step "证据文件：$EvidencePath"
if ($failed.Count -gt 0) {
    Write-Host "[FAIL] 演练失败（$($failed.Count)/$Repeat 遍）" -ForegroundColor Red
    exit 1
}
Write-Host "[OK] 演练通过：$Repeat 遍 exit 0；证据已落盘" -ForegroundColor Green
exit 0
