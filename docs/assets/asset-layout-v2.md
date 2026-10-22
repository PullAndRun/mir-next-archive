# 资源编排 v2（自有语义 ID + 自有元数据）

> 目标：`game-assets/` 只保存**我们自己的资产编排**，不再复制 `resource/` 的库结构。
> 原库信息（`lib`/`source`/`source_sha256`/`data_offset`…）全部移到 `index/provenance.json`，发布包只带 `index/catalog.json`。
> 本页是 v2 方案与等价性证明口径；落地脚本 `tools/assets/make_asset_v2.py`（不改 Rust）。

## 1) 命名（v1 → v2）

| v1（源库痕迹） | v2（自有语义 ID） |
|---|---|
| `gfx/sprites/monster/mon-0130/` | `gfx/sprites/monsters/0130/` |
| `gfx/sprites/npc/npc-0015/` | `gfx/sprites/npcs/0015/` |
| `gfx/sprites/item/icon-0054/` | `gfx/sprites/items/0054/` |
| `gfx/sprites/player/body-003/` | `gfx/sprites/players/body/003/` |
| `gfx/sprites/player/hair-02/` | `gfx/sprites/players/hair/02/` |
| `gfx/sprites/player/weapon-054/` | `gfx/sprites/players/weapon/054/` |
| `gfx/sprites/player/wings-01/` | `gfx/sprites/players/wings/01/` |
| `gfx/sprites/player/mount-1/` | `gfx/sprites/players/mount/1/` |
| `gfx/sprites/effect/{effect,magic,magic2}/` | `gfx/sprites/effects/{swing,hit,bolt}/`（语义名，见 provenance 映射） |
| `gfx/tiles/{tiles0,tiles1,objectsN}/` | 不变（`tiles0`/`tiles1`/`objectsN` 本来就是我们起的语义名，不是 `.wzl` 名） |
| `gfx/ui/minimap/`（143 帧混放） | `gfx/ui/minimaps/<id:04>/`（一图一资产） |
| `maps/*.mnm`、`audio/**/*.mna`、`data/game.mnd`、`fonts/*` | 不变（已是自有格式/命名） |

ID 规则：数值 ID 一律零填充（monster/item/npc 4 位、body/hair/weapon 3 位、mount 无填充）。

## 2) 每个资产目录的内容

```
<asset-dir>/
  frames.json        # 我们的 schema
  0000.png 0001.png …（一帧一文件；文件名 = 帧内序号）
```

`frames.json`（严格按约定字段，不再放源库字段）：

```json
{
  "id": "monsters/0130",
  "kind": "monster",
  "format": "palette8",
  "frames": [
    {"id": 0, "file": "0000.png", "w": 70, "h": 90, "ox": -35, "oy": -70}
  ]
}
```

**移除**：`lib`、`source`、`source_bytes`、`source_sha256`、`data_offset`、`data_len`、
`format_code`、`reserved`、`shadow_x/shadow_y`、`has_mask`、`aliases`、`reason`、`missing`。

## 3) 目录级索引（`game-assets/index/`）

- `catalog.json`（**进发布包**）：资产目录。字段：
  `{ "version": 2, "assets": { "<id>": { "kind", "format", "frames", "bytes", "sha256", "pack_prefix" } } }`
  `sha256` = `frames.json` 的内容哈希；`pack_prefix` = 发布包里的逻辑名前缀（v2 路径）。
- `provenance.json`（**不进发布包**）：只在这里记录源库：
  `{ "version": 2, "sources": { "<id>": { "source_dataset", "source_file", "source_sha256", "source_indices": [..] } } }`
  同一目录来自多个源索引（如翅膀别名、items 补齐）时按索引分别列出。

## 4) `library-map.json` v2

```json
{
  "version": 2,
  "libraries": {
    "monsters": { "kind": "monster", "assets": ["monsters/0000", "monsters/0001"] },
    "items":    { "kind": "item",    "assets": ["items/0000"] },
    "tiles/tiles0": { "kind": "tile", "assets": ["tiles/tiles0"] },
    "players/body": { "kind": "player-layer", "assets": ["players/body/003"] }
  }
}
```

键是**我们的库 ID**（按 kind/用途分组），不再出现 `Mon11`/`hum2`/`items.wzl` 这类原库名；
源库名只出现在 `provenance.json`。

## 5) 等价性证明（切 v1 之前必须全绿）

1. **全量计数**：v2 资产数 = v1 目录数；每个资产的 `frames[].length` = v1 `index.json` 的非空帧数；
   帧像素 SHA（解码 PNG 后的 RGBA sha256）与 v1 `index.json.rgba_sha256` 全量一致；
2. **抽查 20 个**：随机抽 20 个资产（覆盖 monster/npc/item/player/tile/effect/minimap），逐帧比对像素与
   `rgb_sha256`/`rgba_sha256`；
3. **不变量**：v2 帧文件字节与 v1 完全一致（`os.link`/`copy` 保证），
   `frames.json` 里不得出现 `lib/source/data_offset/format_code/reserved/shadow_*` 字段（脚本自检）；
4. 通过后再切换运行时路径并删除 v1；Rust 侧改动（`LibraryMap` v2 / `catalog.json`）另行排期。

## 6) 交付节奏

1. 本页方案（1 页）✅；
2. `tools/assets/make_asset_v2.py` 抽到 `game-assets-v2/`（硬链接，省磁盘）；
3. 等价性报告 `target/asset-v2-equivalence.json`；
4. 队长确认后再切换/删除 v1。

## 7) Rust 对接清单（v2，本轮只列清单不改代码）

> 抽取已完成：`game-assets-v2/` = 1,317 资产 / 150,972 帧 / 11 个库分组 / 496 个 passthrough 文件。
> 下面是切 v2 需要动的**文件:函数**、测试与顺序；改动量按“只加 v2 支持、不动 v1 语义”估。

### 7.1 `mir-asset`（结构层）

| 文件:函数 | 要做什么 | 估行数 |
|---|---|---:|
| `library_map.rs::LibraryMap` / `LibraryEntry` / `parse` / `load` / `get` / `dirs` / `prefix` | 新增 `version:2` 分支：`libraries { <our-lib-id>: { kind, assets[] } }`；保留 v1 解析（双读期） | +120/-20 |
| `library_map.rs::LibraryMap::asset_dir(kind, id)`（新增） | 由 `our-lib-id + id` 直接构造 v2 目录（如 `gfx/sprites/monsters/0130`） | +30 |
| 新增 `catalog.rs::Catalog::load` / `asset(id)` / `prefix(id)` | 读 `index/catalog.json`：`id → kind/format/frames/bytes/sha256/pack_prefix`；发布期走它拿包前缀 | +80 |
| `pack.rs::PackSet::contains/meta/image` | 无需改结构；调用方改用 catalog 的 `pack_prefix` 拼逻辑名 | 0 |
| `lib.rs` | 导出 `Catalog`、`LibraryMapV2` | +5 |
| `library_map.rs` 单测 | 新增 v2 解析 + v1/v2 双读用例 | +40 |

### 7.2 `mir-render`（路径构造）

| 文件:函数 | 要做什么 | 估行数 |
|---|---|---:|
| `entities.rs::SpriteLibrary::stem` | v2 下不再返回 `hum2`/`Mon11`；改为返回语义 kind（`monster`/`player-body`/`item`…） | +40/-30 |
| `entities.rs::EntityLibraries::map_dirs` / `path_for_stem` / `library` | 用 `LibraryMap::asset_dir` 或 `Catalog::asset` 取目录；`mount`/`wings`/`effect` 走语义名（`players/mount/1`、`players/wings/01`、`effects/swing`） | +80/-50 |
| `entities.rs::EntityLibraries::open_dir` | v2 分支不再扫 `.wzl`（legacy 仅 dev 兜底） | +10/-10 |
| `tiles.rs::TileLibraries::open_with_sources` / `library` | tile 语义名不变（`tiles0`/`tiles1`/`objectsN`），只把 `map.dirs()` 换成 v2 目录解析 | +20/-10 |
| `minimap.rs::MinimapScene::open_dir` | 现在直接 `mmap.wzl`；v2 改成读 `gfx/ui/minimaps/<id:04>/frames.json` + `0000.png`（一图一资产） | +60/-30 |
| `text.rs::FONT_DIRECTORY` / 字体候选 | `game-assets/fonts` 不变；仅确认 v2 根也支持（`--asset-root` 传入即可） | +5 |
| `scene.rs::load_with_sources` | 透传 v2 `LibraryMap`/`Catalog` | +15 |

### 7.3 `mir-client`（装配）

| 文件:函数 | 要做什么 | 估行数 |
|---|---|---:|
| `assets.rs::AssetMode` / `dev()` / `release()` / `load()` | dev 读 v2 `library-map.json` + `index/catalog.json`；release 读 `packs/library-map.json`（v2）+ 包内 `index/catalog.json`；`auto` 优先级不变（散装→packs→legacy） | +60/-20 |
| `app.rs:457`、`main.rs:683` 的 `EntityLibraries::open_with_sources` | 参数换 v2 map/catalog | +10 |
| `assets.rs` 单测 | dev/release/auto 三档在 v2 布局下的路径断言 | +40 |

### 7.4 `mir-tools`（打包/校验）

| 文件:函数 | 要做什么 | 估行数 |
|---|---|---:|
| `main.rs::discover_pack_items` | 目录→逻辑名改为读 `index/catalog.json` 的 `pack_prefix`（v2 前缀 `gfx/sprites/monsters/0130/`），不再从 v1 目录名推 | +40/-20 |
| `main.rs::discover_own_files`（verify 期望集） | 同样以 catalog 为准（或 v2 布局规则），避免 `reports/` 这类 dev 文件误入 expected | +30/-15 |
| `main.rs::pack_from_plan` | 计划里的 `dest` 用 v2 id | +10 |
| `main.rs::verify_packs` / `list_packs` | 打印 v2 前缀；`--check` 保持逐条 sha | +15 |

### 7.5 需要改的测试

| 测试 | 改动 |
|---|---|
| `mir-render/tests/asset_modes.rs` | 造 v2 散装根（`gfx/...` + `frames.json` + `catalog.json`）+ v1 兼容用例 |
| `mir-render/tests/release_load.rs` | release 读 v2 packs（前缀来自 catalog） |
| `mir-render/tests/minimap.rs` | 小地图改 `gfx/ui/minimaps/<id>` |
| `mir-render/tests/{animation_assets,player_layers}.rs` | 路径断言改 v2 语义名 |
| `mir-client/tests/{client_fx,mount_wings,real_assets,player_looks}.rs` | 素材根/路径断言改 v2；`mount_wings` 已有 game-assets 版本，直接换根 |
| `mir-client/tests/font_portability.rs` | 字体目录不变，只加 v2 根用例 |
| `mir-asset` 单测 | `library_map` v2 解析、`catalog` 读取 |

估：**10–14 个文件、约 600–900 行**（含测试）。

### 7.6 切换顺序与风险

```
1. v2 抽取（已完成，game-assets-v2/）
2. Rust 双读：LibraryMap 同时认 v1/v2；Catalog 可选；运行时默认仍 v1
3. 默认切 v2（dev 先切、release 后切）→ 全量测试 + release 冒烟
4. 删 v1（game-assets/ 的旧目录结构）
5. 删双读分支与 v1 解析
```

风险：
- **包逻辑名与 sha 全变**：v2 前缀不同 → 45 个 `.mnp` 全量重打、重新对账（`reconcile_packs.py` 需加 v2 前缀模式）；
- **双读期**：`library-map.json` version 字段必须显式区分，旧读者遇到 v2 要报错而不是静默空表；
- **minimap 语义拆分**（一图一资产）与 `mmap.wzl` 原始索引的映射只在 `provenance.json.frame_map` 里，渲染层不要再依赖 `mmap` 索引；
- **provenance 不进发布包**：发布期若需要追溯，必须另带（当前约定不带）；
- 与户部的 **atlas 功能**有交互：atlas 的 `atlas.json` 也要纳入 `frames.json`/catalog 的资产模型，建议 atlas 落地时直接按 v2 schema 输出。
