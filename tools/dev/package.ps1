# 发布打包（手动时机：发版前；不并入 ci.ps1 的 10 项门禁，release 构建是分钟级）。
#
# 产出：dist/mir-next-<版本>-win64/
#   mir-server.exe / mir-client.exe
#   config/lang/*.json、config/client.toml.example
#   resources/README.txt（素材不随包分发，说明如何拷贝）
#   manifest.sha256.json（除 manifest 自身外的每个文件的 SHA-256）
#
# 用法：
#   pwsh -NoProfile -File tools/dev/package.ps1
#   pwsh -NoProfile -File tools/dev/package.ps1 -Version 0.2.0 -SkipBuild
#   pwsh -NoProfile -File tools/dev/package.ps1 -Verify
#   pwsh -NoProfile -File tools/dev/package.ps1 -Verify -Dist dist/mir-next-0.2.0-win64
#
#   pwsh -NoProfile -File tools/dev/package.ps1 -Verify -Path $env:TEMP/vA
# -Verify：逐文件比对 manifest 哈希与文件数（不含 manifest 自身）；
#           删除文件 → MISSING、改一字节 → TAMPERED，均 exit 1 并打印文件名。
param(
    [switch]$Verify,
    [switch]$SkipBuild,
    [string]$Version = "",
    [Alias('Path')]
    [string]$Dist = ""
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location $root

function Fail([string]$message) {
    Write-Host "FAIL: $message" -ForegroundColor Red
    exit 1
}

function Get-Sha256([string]$path) {
    (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant()
}

# Windows PowerShell 5.1（.NET Framework）没有 [IO.Path]::GetRelativePath，用 Uri 兜底。
function Get-RelativeTo([string]$baseDir, [string]$path) {
    $baseUri = New-Object System.Uri (($baseDir.TrimEnd('\') + '\'))
    $pathUri = New-Object System.Uri $path
    return [System.Uri]::UnescapeDataString($baseUri.MakeRelativeUri($pathUri).ToString()).Replace('/', '\')
}

function Find-LatestDist {
    $dirs = Get-ChildItem -Path (Join-Path $root 'dist') -Directory -Filter 'mir-next-*-win64' -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending
    if (-not $dirs -or $dirs.Count -eq 0) { return "" }
    return $dirs[0].FullName
}

if ($Verify) {
    $target = if ($Dist) {
        if ([IO.Path]::IsPathRooted($Dist)) { $Dist } else { Join-Path $root $Dist }
    } else { Find-LatestDist }
    if (-not $target -or -not (Test-Path $target)) { Fail "找不到 dist 目录（先跑 tools/dev/package.ps1）" }
    $manifestPath = Join-Path $target 'manifest.sha256.json'
    if (-not (Test-Path $manifestPath)) { Fail "缺少 manifest.sha256.json：$manifestPath" }
    $manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
    $entries = @($manifest.files)
    $actual = @(Get-ChildItem -Path $target -Recurse -File | Where-Object { $_.Name -ne 'manifest.sha256.json' })
    $failed = $false
    if ($actual.Count -ne $entries.Count) {
        Write-Host "FAIL: 文件数不匹配：dist=$($actual.Count) manifest=$($entries.Count)" -ForegroundColor Red
        $failed = $true
    }
    foreach ($entry in $entries) {
        $path = Join-Path $target $entry.path
        if (-not (Test-Path -LiteralPath $path)) {
            Write-Host "MISSING: $($entry.path)" -ForegroundColor Red
            $failed = $true
            continue
        }
        $hash = Get-Sha256 $path
        if ($hash -ne $entry.sha256) {
            Write-Host "TAMPERED: $($entry.path)" -ForegroundColor Red
            $failed = $true
        }
    }
    $manifestPaths = @($entries | ForEach-Object { $_.path })
    foreach ($file in $actual) {
        $relative = (Get-RelativeTo $target $file.FullName).Replace('\', '/')
        if ($manifestPaths -notcontains $relative) {
            Write-Host "EXTRA: $relative" -ForegroundColor Red
            $failed = $true
        }
    }
    # 冒烟：客户端 --help 必须能启动并退出 0（不依赖数据库）。
    # 服务端暂未实现 --help（会直接启动监听），只校验哈希/存在性；
    # 启动耗时另见 docs/deploy.md 的实测数字。
    $smokeName = 'mir-client.exe'
    $smokeExe = Join-Path $target $smokeName
    if (Test-Path -LiteralPath $smokeExe) {
        $log = Join-Path $env:TEMP ('mir-next-verify-' + [guid]::NewGuid().ToString('N') + '.log')
        $process = Start-Process -FilePath $smokeExe -ArgumentList '--help' -WindowStyle Hidden -PassThru -Wait -RedirectStandardOutput $log -RedirectStandardError ($log + '.err')
        if ($process.ExitCode -ne 0) {
            Write-Host "FAIL: $smokeName --help exit=$($process.ExitCode)" -ForegroundColor Red
            $failed = $true
        }
        Remove-Item -LiteralPath $log, ($log + '.err') -ErrorAction SilentlyContinue
    }
    if ($failed) { exit 1 }
    Write-Host "OK: $($entries.Count) 个文件哈希一致（$target）" -ForegroundColor Green
    exit 0
}

if (-not $Version) {
    $cargo = Get-Content -Raw -LiteralPath (Join-Path $root 'Cargo.toml')
    $match = [regex]::Match($cargo, '(?m)^version\s*=\s*"([^"]+)"')
    if (-not $match.Success) { Fail '无法从 Cargo.toml 读取版本号' }
    $Version = $match.Groups[1].Value
}
if (-not $SkipBuild) {
    Write-Host 'cargo build --release -p mir-server -p mir-client ...'
    & cargo build --release -p mir-server -p mir-client
    if ($LASTEXITCODE -ne 0) { Fail 'cargo build --release 失败' }
}

$out = Join-Path $root "dist\mir-next-$Version-win64"
if (Test-Path $out) { Remove-Item -Recurse -Force $out }
New-Item -ItemType Directory -Force -Path (Join-Path $out 'config\lang') | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $out 'resources') | Out-Null

foreach ($name in @('mir-server.exe', 'mir-client.exe')) {
    $source = Join-Path $root "target\release\$name"
    if (-not (Test-Path $source)) { Fail "缺少 target/release/$name（先 cargo build --release）" }
    Copy-Item -LiteralPath $source -Destination (Join-Path $out $name)
}
Get-ChildItem -Path (Join-Path $root 'config\lang') -File -Filter '*.json' | ForEach-Object {
    Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $out 'config\lang' $_.Name)
}

$clientExample = @"
# 客户端统一设置示例（复制为 config/client.toml 后按需修改）。
language = "zh-cn"
master_volume = 0.500
sound_volume = 0.500
music_volume = 0.500
window_mode = "windowed"
scale = 1.0
show_minimap_gates = true

[keys]
attack = "Space"
magic = "F1"
inventory = "I"
group = "P"
friends = "F"
settings = "O"
"@
Set-Content -LiteralPath (Join-Path $out 'config\client.toml.example') -Value $clientExample -Encoding UTF8

$resourceReadme = @"
素材目录（resource/）不随发布包分发，请从开发仓库复制或按 docs/deploy.md 获取：
  resource/data/     .wzl/.wil 图库与调色板
  resource/map/      .map 地图
  resource/wav/      音效
  resource/fonts/    中文字体（Noto Sans SC 等）
缺素材时客户端仍可启动：地图/精灵退化为纯色占位，不 panic。
"@
Set-Content -LiteralPath (Join-Path $out 'resources\README.txt') -Value $resourceReadme -Encoding UTF8

$files = @(Get-ChildItem -Path $out -Recurse -File | Where-Object { $_.Name -ne 'manifest.sha256.json' } | Sort-Object FullName)
$entries = foreach ($file in $files) {
    $relative = (Get-RelativeTo $out $file.FullName).Replace('\', '/')
    [ordered]@{
        path   = $relative
        sha256 = Get-Sha256 $file.FullName
        bytes  = $file.Length
    }
}
$manifest = [ordered]@{
    name    = "mir-next-$Version-win64"
    version = $Version
    files   = @($entries)
}
$manifestPath = Join-Path $out 'manifest.sha256.json'
$manifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $manifestPath -Encoding UTF8

$distCount = @(Get-ChildItem -Path $out -Recurse -File).Count
$entryCount = @($entries).Count
Write-Host "已生成 $out"
Write-Host "文件数（含 manifest）=$distCount  manifest 条目数=$entryCount（不含 manifest 自身）"
Write-Host "server=$((Get-Item (Join-Path $out 'mir-server.exe')).Length) bytes  client=$((Get-Item (Join-Path $out 'mir-client.exe')).Length) bytes"
if ($entryCount + 1 -ne $distCount) { Fail '文件数与 manifest 不一致' }
