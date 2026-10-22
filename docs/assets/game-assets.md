# 独立游戏资源目录 `game-assets/`

> 说明：本页记录的是**首版整目录复制方案**（22.5 GB、按原库名存放）。当前发布布局已改为自有格式
> `gfx|maps|audio|data|fonts` + `.mnp/.mnm/.mnd/.mna`，判定规则与命令见
> [`asset-pipeline.md`](asset-pipeline.md)；本页第 4 节的体积表已过时，保留作历史对照。

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

**道具图标透明**：`items.wzl` 的 RGB565 黑底已在拆包时去透明（657 个目录 / 243,254 像素，
详见 §5.1）；管道参数 `--black-transparent-formats 0,3,5`。

## 3) 运行时怎么指定资源根

**优先级（高 → 低）**——服务端与客户端一致，实现分别在
`crates/mir-server/src/config.rs`（`resolve_asset_root`）与
`crates/mir-client/src/assets.rs`（`AssetRoot::resolve`）：

| 档位 | 服务端 | 客户端 |
|---|---|---|
| 1（最高） | 配置结构体 `ServerConfig.asset_root` | `--asset-dir <path>` |
| 2 | 环境变量 `MIR2_ASSET_DIR` | 环境变量 `MIR2_ASSET_DIR` |
| 3 | — | `config/client.toml` 的 `asset_root` |
| 4 | 默认 `game-assets` | 默认 `game-assets` |
| 5（回落） | `resource`（**默认值缺失打 info，显式配置缺失打 warn**） | 同左 |

派生目录（都在资源根下）：

| 用途 | 路径 | 说明 |
|---|---|---|
| 地图 | `<root>/map` | 服务端 `ServerConfig.map_root`；显式配置或 `MIR2_MAP_ROOT` 优先 |
| 图库/UI | `<root>/data` | 客户端 `--data-dir` 显式给则优先 |
| 音效 | `<root>/wav` | 客户端由 `data_dir` 的父目录推出（`SoundLibrary::around_data_dir`），`--sound-dir` 优先级更高 |
| 字体 | `<root>/fonts` | `AssetRoot::fonts_dir()` |
| 图库索引 | `<root>/ld` | `AssetRoot::ld_dir()` |
| 数据库 | `<root>/server/Server.MirDB` | 服务端 `db_path`；`MIR2_DB_PATH` 优先 |

```powershell
# 服务端：用抽出来的独立资源
$env:MIR2_ASSET_DIR = "D:\mir-assets"; pwsh -NoProfile -File tools/dev/serve.ps1

# 客户端：只给这一条命令指定
cargo run -p mir-client -- --asset-dir D:\mir-assets

# 冒烟（3 档资源根 + 客户端解析单测）
pwsh -NoProfile -File tools/dev/asset-smoke.ps1
```

> `game-assets/` **不存在**时行为与旧版完全一致（回落 `resource/`），
> 所以测试/CI 不装素材也能全绿；显式指定的目录不存在会 **warn** 并回落，不掩盖配置错误。


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

## 5.1) 道具图标黑底去透明（`items.wzl`，2026-10-22 修复）

`items.wzl` 的图标帧是 **RGB565**（`format=5`），原拆包只对 palette8（0/3）做 black→transparent，
导致图标四角是 `(0,0,0,255)` 的黑底。修复后：

- 计划里 `items.wzl` 带 `black_transparent_formats: [0,3,5]`，拆包器对该库 format 5 生效；
- 规则是**边框连通**（`mir_asset::black_to_transparent_rgb_border`）：从四边洪泛，只把与边框连通的纯黑
  置 `alpha=0`，图标内部纯黑（黑剑/描线）保留；
- 实测（`game-assets/gfx/sprites/item` 830 个目录）：657 个变化、新增透明像素 243,254 个；
  抽样 `icon-0000/0001/0100/3790` 四角 `alpha=0`、`opaque_black=0`，`icon-0850` 保留 47 个内部黑像素；
- 重打包后仅 `sprites-item-1.mnp` 变化（572,375 → 576,320 B），45 包总量
  433,976,575 → **433,980,520 B**，`--verify` 45/45、`reconcile_packs.py` issues=0；
- 复现：`mir-tools assets unpack --plan target/used-assets-items.json --out-root game-assets`；
  补齐后重打包：`sprites-item-1.mnp` 658 → **826** 条、576,320 → **771,854 B**，45 包总量
  433,980,520 → **434,328,059 B**、条目 151,581 → **151,750**（含 data 包的 `clean-report.json`）。

### 5.1.1) 空帧补齐（2026-10-22 第二轮）

`items.wzl`（cjlaaa 数据集的这版）里有 **172/830** 个被 DB 引用的索引是空帧（`empty:true`）。
从其它版本的 Items 库按**同索引**补齐了 **168** 个：

| 来源库 | 补齐数 | 说明 |
|---|---:|---|
| `resource/community/mirfiles-wemade-korean-server/KoreanServer/Client/Data/items.Lib` | 104 | 韩服官方 Items 库 |
| `resource/community/hbacc-dx-game-engine-data/game/Data/Items.wzl` | 37 | DX 引擎素材 |
| `resource/community/mirfiles-arcadia-project69/Arcadia&Project69/Data/items2.wzl` | 27 | Arcadia/Project69 |

- 每个补齐目录的 `index.json` 新增 `filled_from`（相对路径）与 `filled_note`，原 `missing` 记录保留；
  `images[0]` 填实际宽高/偏移/格式并重算 `rgb_sha256`/`rgba_sha256`/`png_sha256`；
- 补 PNG 走同一拆包器（`--black-transparent-formats 0,3,5`），四角透明与黑底规则一致；
- **最后 4 个空目录（3486、3487、3493、3576）已删除**：它们只在 `stateitem.wzl` 有同索引帧
  （150×144 ~ 174×172 的穿戴态大图，语义不符），而对应物品（#403 GM 专用、#533/#589/#590 无引用）
  已在兵部二号的 `.mnd` 清洗 pass 中剔除，不再保留目录；其中 **HardenedRing（#403）** 由
  `icon-0433` 派生自绘图标 `icon-3576/3576.png`（`clean-report.warn.icon_filled=true`），
  `game-assets/gfx/sprites/item` 现有 **827** 个目录；
- 抽样：`icon-0075/0076/0077/0132/0277/2884/2885/2886/3792` 四角 `alpha=0` 且非空像素充足；
  `icon-3789` 四角是内容本身（黄色），非黑底。

## 6) 为什么不入库

`game-assets/` 与 `manifest.json` 都是**生成物**（22.5 GB），已加进 `.gitignore`。
重建命令写在本页第 2 节；`manifest.json` 里的 `generated_at` / `sample_sha256` 用来确认"这份资源是哪次抽的"。
