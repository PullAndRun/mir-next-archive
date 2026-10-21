# 独立游戏资源目录 `game-assets/`

游戏**运行时只引用这一份素材**（不再直接读 `resource/`），且资源根可以手动指定。

- 生成工具：[`tools/assets/build_game_assets.py`](../../tools/assets/build_game_assets.py)
- 来源索引：[`docs/assets/index.md`](index.md)（`resource/` 的全量索引，两者不冲突：`resource/` 是素材仓库，`game-assets/` 是运行时视图）

## 1) 目录结构

```
game-assets/
  data/     ← resource/data            客户端图库（.wzl/.wzx/.wil）、UI、minimap、books
  map/      ← cjlaaa/Maps              地图（默认；Server.MirDB 引用的那批）
  wav/      ← resource/wav             音效
  fonts/    ← resource/fonts           字体
  ld/       ← resource/ld              图库索引数据
  server/   ← cjlaaa/Server.MirDB + cjlaaa/Envir/
  manifest.json                        每个子树的来源/文件数/字节数/生成时间 + 抽样 sha256
```

默认 `--map-source cjlaaa` 取 `cjlaaa/Maps`（服务端数据库引用的地图集）；
用 `--map-source resource` 则改取 `resource/map`（Crystal 原版地图集）。

**不复制可执行体**（`.dll/.exe/.pdb/.so/...`）：例如 `resource/data` 里的
`libeay32.dll`、`ssleay32.dll` 会被跳过（对齐仓库既有采集纪律）。

## 2) 生成与校验

```powershell
# 全量抽取（约 22.5 GB，本机 ~40s；建议后台跑）
py tools/assets/build_game_assets.py

# 冒烟：每个子树只取前 N 个文件（产物带 manifest.smoke=true，适合 CI/测试）
py tools/assets/build_game_assets.py --smoke 20 --target target/game-assets-smoke

# 只校验（对比 manifest 与目标；不一致退出码 1 并打印差多少字节）
py tools/assets/build_game_assets.py --verify

# 计划预览 / 强制重拷 / 删除目标里多余的文件
py tools/assets/build_game_assets.py --dry-run
py tools/assets/build_game_assets.py --force
py tools/assets/build_game_assets.py --prune
```

**幂等**：`manifest.json` 与来源一致时直接跳过（打印 `[幂等]`）；
来源变化时打印**每个子树的差异**（文件数/字节数），按 `size+mtime` 增量复制。

**篡改可发现**：`--verify` 除了比文件数/总字节，还会抽查 `manifest.subtrees[*].sample_sha256`
（每子树前 8 个 ≤64MB 的文件）——改动一个字节就会被判 `sha256 不匹配`。

## 3) 运行时怎么指定资源根

优先级（高 → 低）：

| 端 | 覆盖顺序 |
|---|---|
| 服务端 | CLI/配置文件 `asset_root` > 环境变量 `MIR2_ASSET_DIR` > 默认 `game-assets` > **回落 `resource`（WARN）** |
| 客户端 | `--asset-dir <path>` > 环境变量 `MIR2_ASSET_DIR` > `config/client.toml` 的 `asset_root` > 默认 `game-assets` > **回落 `resource`（WARN）** |

```powershell
# 服务端：用抽出来的独立资源
$env:MIR2_ASSET_DIR = "D:\mir-assets"; pwsh -NoProfile -File tools/dev/serve.ps1

# 客户端：也可以只给这一条命令指定
cargo run -p mir-client -- --asset-dir D:\mir-assets
```

`game-assets/` **不存在**时行为与旧版完全一致（回落到 `resource/` 并打一条 WARN），
所以测试/CI 不装素材也能全绿。

## 4) 体积（2026-10-22 实测）

| 子树 | 文件数 | 体积 | 来源 |
|---|---|---|---|
| `data/` | 918 | 21.01 GB | `resource/data`（920 文件，**排除 2 个 dll**） |
| `map/` | 1817 | 1.16 GB | `cjlaaa/Maps` |
| `wav/` | 786 | 207.41 MB | `resource/wav` |
| `fonts/` | 1 | 7.95 MB | `resource/fonts` |
| `ld/` | 4 | 143.83 MB | `resource/ld` |
| `server/` | 1140 | 1.39 MB | `cjlaaa/Server.MirDB` + `cjlaaa/Envir/` |
| **合计** | **4666** | **22.53 GB** | — |

## 5) 已知未裁剪项（下一步）

- **`data/` 全量 21 GB**：客户端实际只打开其中一部分图库（`LibraryFile`/`Prguse` 等）。
  `--prune` 目前只做"删目标里多出来的文件"，**还没有按引用裁剪**；
  后续计划：从客户端 `LibraryFile` 引用表 + 服务端 `Server.MirDB` 的地图/怪物贴图引用生成白名单，
  再加 `--prune-referenced`（工具里已留 TODO 说明）。
- `Crystal` 原版地图集（`resource/map`，1116 文件）当前**整体不进** `game-assets/`
  （默认用 `cjlaaa/Maps`）；需要混用时用 `--map-source resource` 单独抽一份。

## 6) 为什么不入库

`game-assets/` 与 `manifest.json` 都是**生成物**（22.5 GB），已加进 `.gitignore`。
重建命令写在本页第 2 节；`manifest.json` 里的 `generated_at` / `sample_sha256` 用来确认"这份资源是哪次抽的"。
