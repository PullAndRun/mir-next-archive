# 本地全量门禁 —— 本项目**不使用远端 CI**，这是唯一的自动化门禁入口。
#
# 全部检查在本地跑（无远端 CI）。改代码时常跑 `-SkipTests`，收口前跑全量。
#
# 用法（PowerShell 7）：
#   pwsh -NoProfile -File tools/dev/ci.ps1              # 全跑（13 项）
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

# 测试临时目录固定到仓库内：本机沙箱会拒绝 cargo 派生的测试进程写系统 %TEMP%
# （`mir-render`/`mir-tools` 共 4 个用例以 PermissionDenied(5) 失败），
# 而工作区内的 `target\tmp` 可写且方便清理。放在这里让所有 `cargo test` 项继承。
$env:TEMP = Join-Path $root "target\tmp"
$env:TMP = $env:TEMP
New-Item -ItemType Directory -Force -Path $env:TEMP | Out-Null

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
# **soak / 压测锁**：跑长时压测的人会写 `target/soak.lock`（内含 PID 与说明），
# 此时 `-KillStale` 必须**拒绝杀进程**——否则会把别人正在跑的 30 分钟 soak 一刀砍掉
# （第 20 轮真发生过：soak 服务端在 15:42 被外部 `-KillStale` 杀掉）。
$soakLock = Join-Path $root "target/soak.lock"
if (Test-Path $soakLock) {
    $lockInfo = (Get-Content $soakLock -Raw -ErrorAction SilentlyContinue).Trim()
    Write-Host "==> 检测到压测锁 $soakLock" -ForegroundColor Yellow
    Write-Host "    $lockInfo" -ForegroundColor Yellow
    if ($KillStale) {
        Write-Host "    拒绝执行 -KillStale：压测进行中，杀掉进程会破坏对方的 30 分钟 soak。" -ForegroundColor Red
        Write-Host "    等压测结束（对方会删除锁文件）后再跑；或自己确认锁已过期后手动删 $soakLock。" -ForegroundColor Red
        exit 2
    }
    Write-Host "    为安全起见，本次不做任何进程清理（构建可能因 exe 被占用而失败）。" -ForegroundColor Yellow
}

$stale = @(Get-Process -Name mir-server, mir-client -ErrorAction SilentlyContinue)
if ($stale.Count -gt 0) {
    $list = ($stale | ForEach-Object { "$($_.Id)@$($_.StartTime.ToString('HH:mm:ss'))" }) -join ", "
    if ($KillStale -and -not (Test-Path $soakLock)) {
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
    # workspace 再跑一遍 duo_smoke 是**并行**的：与其它 crate 的测试抢 CPU 时，
    # `duo_group_experience_*` 的实时战斗/经验回调会超时（实测 227s 仍 FAILED）。
    # 该套用例已由上一项**串行**跑过（覆盖率不变），这里只跳过重复的并行执行。
    # `fault_pg` 是「真 PG 故障注入」套件：并行跑会与其它 PG 用例抢连接/被 terminate 干扰
    # （实测 workspace 并行时 `transient_terminate_is_transparent_*` 偶发红；单独跑 0.44s 通过）。
    # 这里跳过并行版本，改由第 5 步的**串行** PG 门禁跑（覆盖率不变）。
    # R68 起：真实 125/570 逐帧偏移集成用例依赖 mir-render 的 legacy WZL 读取路径，
    # workspace 测试必须显式启用该特性，否则 live 用例会因「合成图库不可读」而假红/跳过。
    Invoke-Gate "cargo test --workspace" "cargo test --workspace --features mir-render/legacy -- --skip duo_ --skip fault_pg"
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
Invoke-Gate "硬编码棘轮（no-test，阈值 0）" "$python tools/i18n/scan_hardcoded.py --no-test --player-visible --strict --fail-above 0"

# 4b) 资源规则：运行时不得引用老资源格式（棘轮，基线见 tools/dev/legacy-runtime-allowlist.txt）
Invoke-Gate "运行时不引用老资源格式" "$python tools/dev/check_legacy_refs.py"

# 4c) 数据清洗后的物品图标完整性：无图标模板必须补图/删引用或显式白名单
Invoke-Gate "物品图标完整性" "$python tools/assets/verify_item_icons.py"

# 5) PostgreSQL 集成测试（连接串已在脚本开头统一设好，这里只跑门禁）
if (-not $SkipTests -and -not $SkipPostgres) {
    Invoke-Gate "PostgreSQL 集成测试（mir-store 串行）" "cargo test -p mir-store -- --test-threads=1"
    # `fault_pg`（PG 故障注入 / pg_proxy）**默认不进 CI 门禁**：它依赖 `tools/fault/pg_proxy.py` + python，
    # 在本机串行跑会因 proxy 子进程残留而挂起（实测 >10 分钟无输出，手工 kill 后才恢复）；
    # 并行跑又会与其它 PG 用例抢连接/被 terminate 干扰（历史 flake）。
    # 需要时手工跑：`MIR2_TEST_DATABASE_URL=... cargo test -p mir-server --test fault_pg -- --test-threads=1`
    # 该套件覆盖的「落盘故障/恢复」语义另有 mir-store 串行门禁 + 单测覆盖。
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
