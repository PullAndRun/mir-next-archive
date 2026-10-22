<#
.SYNOPSIS
    《传奇-Next》发布前冒烟检查：清单/包完整性、library-map、只读 packs 的 headless 渲染。

.DESCRIPTION
    一条命令检查"发布包是否正确、能否加载、能否渲染"：

      1. `manifest.packs.json` 存在且 `deterministic = true`；打印包数/字节/条目/生成器/时间戳；
      2. `mir-tools assets pack --check` 逐包校验 sha256 与清单一致；
      3. `packs/library-map.json` 存在且库数 == -ExpectLibraries（默认 67）；
      4. `mir-render --example release_smoke`：把 data/gfx 指到临时空目录，**只读 packs**
         解码 tiles0 / mon-0130、渲染 map0 一帧，断言非空像素 > 0 且与散装 dev 渲染差异 = 0；
      5. 失败 exit 1，成功 exit 0。

.PARAMETER PackDir
    packs 目录（含 `manifest.packs.json`、`*.mnp`、`library-map.json`）。默认 `game-assets/packs`。

.PARAMETER AssetsRoot
    散装素材根（含 `library-map.json`、`gfx/`、`data/`、`maps/`），用于 dev 对照渲染。
    默认取 `-PackDir` 的父目录。

.PARAMETER ExpectLibraries
    `library-map.json` 里应有的库数量。默认 67。

.PARAMETER PackPath
    地图路径（相对 `-AssetsRoot`）。默认 `maps/0.mnm`。

.PARAMETER NoCheck
    跳过 `mir-tools assets pack --check`（只做加载/渲染检查，调试用）。

.EXAMPLE
    pwsh -NoProfile -File tools/dev/release-smoke.ps1 -PackDir game-assets/packs

.EXAMPLE
    pwsh -NoProfile -File tools/dev/release-smoke.ps1 -PackDir D:\build\packs -AssetsRoot D:\build\game-assets
#>
[CmdletBinding()]
param(
    [string]$PackDir = "game-assets/packs",
    [string]$AssetsRoot = "",
    [int]$ExpectLibraries = 67,
    [string]$MapPath = "maps/0.mnm",
    [switch]$NoCheck
)

$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $false

function Fail([string]$Message) {
    Write-Host "release-smoke: FAILED：$Message" -ForegroundColor Red
    exit 1
}

function Step([string]$Message) {
    Write-Host "release-smoke: $Message" -ForegroundColor Cyan
}

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$PackDir = if ([System.IO.Path]::IsPathRooted($PackDir)) { $PackDir } else { Join-Path $RepoRoot $PackDir }
if (-not $PackDir) { Fail "缺 -PackDir" }
if (-not (Test-Path -LiteralPath $PackDir -PathType Container)) { Fail "packs 目录不存在：$PackDir" }
$PackDir = (Resolve-Path -LiteralPath $PackDir).Path
if (-not $AssetsRoot) { $AssetsRoot = Split-Path -Parent $PackDir }
$AssetsRoot = if ([System.IO.Path]::IsPathRooted($AssetsRoot)) { $AssetsRoot } else { Join-Path $RepoRoot $AssetsRoot }
$AssetsRoot = (Resolve-Path -LiteralPath $AssetsRoot).Path

$ManifestPath = Join-Path $PackDir "manifest.packs.json"
if (-not (Test-Path -LiteralPath $ManifestPath -PathType Leaf)) { Fail "缺清单：$ManifestPath" }

# 1) 清单基本检查
$manifest = Get-Content -LiteralPath $ManifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
if (-not $manifest.packs -or @($manifest.packs).Count -eq 0) { Fail "清单里没有包" }
if (-not $manifest.deterministic) { Fail "manifest.deterministic 必须为 true（用新版 mir-tools 重打）" }
$totalBytes = (@($manifest.packs) | Measure-Object -Property bytes -Sum).Sum
$totalEntries = (@($manifest.packs) | Measure-Object -Property entries -Sum).Sum
$manifestSha = (Get-FileHash -LiteralPath $ManifestPath -Algorithm SHA256).Hash.ToLowerInvariant()
Step ("packs={0} bytes={1} entries={2} generator={3} deterministic={4} generated_at={5}" -f `
        @($manifest.packs).Count, $totalBytes, $totalEntries, $manifest.generator, $manifest.deterministic, $manifest.generated_at)
Step "manifest.packs.json sha256=$manifestSha"

# 2) 逐包 sha256 校验（mir-tools --check）
if (-not $NoCheck) {
    Step "mir-tools assets pack --check（逐包 sha256）…"
    & cargo run -q --manifest-path (Join-Path $RepoRoot "Cargo.toml") -p mir-tools --bin mir-tools -- `
        assets pack --input $AssetsRoot --out $PackDir --check $ManifestPath
    if ($LASTEXITCODE -ne 0) { Fail "mir-tools --check 失败（exit $LASTEXITCODE）" }
}

# 3) library-map
$LibraryMapPath = Join-Path $PackDir "library-map.json"
if (-not (Test-Path -LiteralPath $LibraryMapPath -PathType Leaf)) { Fail "缺 $LibraryMapPath（发布包必须带 library-map.json）" }
$libraryMap = Get-Content -LiteralPath $LibraryMapPath -Raw -Encoding UTF8 | ConvertFrom-Json
$libraryCount = @($libraryMap.libraries.PSObject.Properties).Count
Step "library-map libraries=$libraryCount（期望 $ExpectLibraries）"
if ($libraryCount -ne $ExpectLibraries) { Fail "library-map 库数 $libraryCount != 期望 $ExpectLibraries" }

# 4) 只读 packs 的 headless 渲染
Step "release_smoke：只读 packs 加载/渲染，并与散装 dev 逐像素对比 …"
$smokeOutput = & cargo run -q --manifest-path (Join-Path $RepoRoot "Cargo.toml") -p mir-render `
    --example release_smoke -- --packs $PackDir --assets $AssetsRoot --expect-libraries $ExpectLibraries --map $MapPath
$smokeExit = $LASTEXITCODE
$smokeOutput | ForEach-Object { Write-Host "  $_" }
if ($smokeExit -ne 0) { Fail "release_smoke 失败（exit $smokeExit）" }

# 5) 汇总
Step "OK（packs=$(@($manifest.packs).Count) bytes=$totalBytes entries=$totalEntries manifest=$manifestSha）"
exit 0
