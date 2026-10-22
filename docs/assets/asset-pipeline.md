# 素材管线：引用分析 → 分类拆包 → 命名落盘

> 第 26 轮交付。负责人：户部二号（hubu2）。关联：
> [`analyze_usage.py`](../../tools/assets/analyze_usage.py)（引用分析）、
> [`export_asset_sample.py`](../../tools/assets/export_asset_sample.py)（独立解码样例）、
> [`make_mini_plan.py`](../../tools/assets/make_mini_plan.py)（map0+items+1 怪物的冒烟计划）、
> [`split_extract.py`](../../tools/assets/split_extract.py)（分片并行拆包编排）、
> [`verify_layout.py`](../../tools/assets/verify_layout.py)（新布局校验）、
> [`build_manifest.py`](../../tools/assets/build_manifest.py)（manifest.assets.json）、
> 户部的 `mir-tools assets unpack --plan`（Rust 拆包器）、
> [`game-assets.md`](game-assets.md)（上一轮的整目录复制方案，本文取代其 `data/` 部分）。

用户要求：**把原版 `.wzl/.wix/.wil` 素材包拆开，只保留正在用的图；资源按我们自己的规则分类命名；
开发期散装（每图一文件），发布期用 `.mnp` 小包（单包 ≤16MB）**。

本文定稿「判定规则、命名规则、统计口径、验证命令、已知缺口」。

---

## 1. 命名与分类（我们自己的规则）

原始库名**只作为 `index.json` 的 `source` 记录**，目录名不再使用 `resource/data` 里的文件名。

```
game-assets/                                  # 开发期：散装，一图一文件
  gfx/
    tiles/<tileset>/<index>.png               # 地图瓦片
    sprites/
      monster/<entity>/<frame>.png            # 怪物
      npc/<entity>/<frame>.png                # NPC
      player/body-<shape>/<frame>.png         # 玩家身体（hum*）
      player/weapon-<shape>/<frame>.png       # 武器层（weapon*）
      player/hair-<hair>/<frame>.png          # 头发层（hair*）
      player/wings-<effect>/<frame>.png       # 翅膀层（wingeffect_x）
      player/mount-<block>/<frame>.png        # 坐骑（渲染层必需 horse_x，3 块）
      item/icon-<image>/<index>.png           # 物品图标（单帧，文件名=库内 index=image）
      effect/<effect|magic|magic2>/<frame>.png
    ui/minimap/<index>.png                    # mmap.wzl（小地图/大地图共用）
  maps/<mapid>.mnm                            # 0.mnm / 0101.mnm（.map 只在导入/构建期使用）
  audio/sfx/<name>.wav                        # 音效
  audio/music/<name>.wav                      # 背景音乐（30001/30002…）
  data/server/Server.MirDB
  data/server/Envir/...
  fonts/<name>.otf
```

### 1.1 实体名怎么取（本单要回答的问题）

| 类别 | 目录键（entity） | 取号规则 | 为什么不直接用品名 |
|---|---|---|---|
| 怪物 | `mon-<image:04d>` | `MonsterInfo.Image`（0..539） | 一个外观可能被多个模板共用（同名不同 body、翻译改名、`00` 占位名）；body 号与美术 1:1、**跨语言/改名稳定**。DB 名写入 `index.json` 的 `aliases`，供检索 |
| NPC | `npc-<image:04d>` | `NpcInfo.Image` | 同上；NPC 名经常带地图前缀/活动后缀 |
| 玩家身体 | `body-<shape:03d>` | `ItemType::Armour` 的 `Shape`（0=裸体） | 与 `player_library()` 的块号换算一一对应 |
| 武器 | `weapon-<shape:03d>` | `ItemType::Weapon` 的 `Shape`（含 100/200 段位） | 段位已编码在数里（`shape/100+1` 选库） |
| 头发 | `hair-<hair:02d>` | 服务端 `character.id % 9`（0..8） | 没有 DB 模板，只能按角色发型号 |
| 翅膀 | `wings-<block+1:02d>` | 外观块 = `(Effect-1) % 2`（语料只有 2 块） | 客户端 `wing_layer(effect)` 本来就会取模；`Effect=100` 与 `Effect=2` 共用块 1，作为 `wings-02` 的 `aliases`，不重复拆 PNG |
| 物品图标 | `icon-<image:04d>` | `ItemInfo.Image`（items.wzl 0 基下标；文件名即该 index） | 图标是扁平 2D 资源，没有“实体名”概念；按模板 `Image` 号去重 |
| 瓦片 | `<tileset>` | 见 1.2 映射 | 一个 tileset 就是一个目录，不需要实体层 |
| 特效 | `effect/<lib>` | 固定三库：`effect`/`magic`/`magic2` | 代码按 `FxKind` 取固定帧段 |

补充：

- **单帧类别保留 `<entity>/<index>.png` 这一级**（如 `icon-0054/0054.png`），而不是扁平 `icon-0054.png`：与拆包器“每库一个目录 + 四位帧号”的契约一致，
  避免为 830 个图标写特例；以后真要扁平化只需一次重命名脚本。
- 同名实体不会冲突：怪物/NPC 用 body 号；玩家层用 shape 号；`index.json` 里另存 `lib`
  （如 `mon14.wzl`）、`source_index`（库内帧号）与 `aliases`（DB 名、中文名）。
- 大小写：目录一律小写；`lib`/`source` 保留磁盘真实大小写（`Mon11.wzl`），由拆包器按真实文件名打开。

### 1.2 tileset 命名映射

Crystal 的 `MapLibs` 编号 → 我们的 tileset 名（小写）：

| 源库 | `tileset` | 说明 |
|---|---|---|
| `tiles.wzl`（MapLibs 0） | `tiles0` | 后层瓦片 |
| `smtiles.wzl`（MapLibs 1） | `tiles1` | 中层瓦片 |
| `objects.wzl`（MapLibs 2） | `objects0` | 前层物件 |
| `objects2.wzl` … `objects27.wzl` | `objects1` … `objects26` | 即 `MapLibs - 2`，避免“名字里的 2 其实是第 1 个” |
| `objects_32bit.wzl`（90） | `objects32` | 32 位物件（本数据集未引用） |
| `anitiles1.wzl`（190） | `anitiles1` | 动画瓦片（本数据集未引用） |

> 没有采用“tiles0..tiles3”四分类：语料里瓦片族有 29 个真实库（tiles/smtiles/objects1..27），
> 四类会丢掉“哪一层/哪一段物件”的信息；当前命名保持 **1 库 = 1 tileset**，发布期再按体积切桶。

### 1.3 发布期分包（`.mnp`，户部已实现，见 §8）

开发期保持散装。发布期按「类别 + 桶」切 `.mnp`（单包 ≤16MB）：

- `tiles.*.mnp`：按 tileset 顺序装桶，桶内记录 `<tileset>/<index>` → 压缩后的 RGBA；
- `sprites.monster.*.mnp` / `sprites.player.*.mnp`：按实体名排序装桶；
- `ui.mnp` / `effects.mnp` / `items.mnp`：小类别单独一包；
- 清单 `manifest.mnp.json` 记录每包的 `sha256`、条目数、字节数、覆盖的实体/帧号区间。

本轮**以开发期散装 + 命名规则为主**；`.mnp` 打包器已由户部落地（见 §8），发布包体积/加载验收留后续轮次；命名规则已保证“按实体/类别可稳定切片”。

---

## 2. 判定口径：哪些素材算「正在用」

脚本：[`tools/assets/analyze_usage.py`](../../tools/assets/analyze_usage.py)。只读分析，不写 `game-assets/`。

| 类别 | 引用来源 | 判定规则 | 证据/代码 |
|---|---|---|---|
| 地图 | `Server.MirDB` 的 `MapInfo` | 389 条 `MapInfo.FileName`（385 个唯一文件）从 `cjlaaa/Maps/*.map` 导入并转成自有 `.mnm`；其余 1428 张不进包 | `mir-db/src/map.rs`、`mir-tools/bin/map_import.rs` |
| 瓦片 | 地图格的 Back/Middle/Front 图像号 | v0/v1/v2/v3/v4 按 Crystal 客户端布局解析；后层→`tiles.wzl`、中层→`smtiles.wzl`、前层→`objects{front_index+2}`；门动画帧 1..anim 一并收 | Crystal `Client/MirObjects/MapCode.cs` `LoadMapType0..4`；`mir-render/src/scene.rs` |
| 怪物 | `MonsterInfo.Image` | `lib=image/10+1`、槽 `image%10`；只收 `animation.rs` 动作帧表真正会播的 200/430 帧 | `mir-render/src/entities.rs:1401`、`animation.rs` |
| NPC | `NpcInfo.Image` | `lib=image/100+1`（1..4）、槽 `image%100`；保守保留整块 60 帧（站立帧表跨图像） | `entities.rs:1395` |
| 玩家身体/武器/头发 | 物品模板 `Shape` / 发型号 | 衣服 `Shape`→`hum*` 块，武器 `Shape/100+1` 选库、`Shape%100` 块，发型号 0..8；每块只收 240/600 帧 | `entities.rs:585-630`、`session.rs:899` |
| 翅膀 | 盔甲 `Effect` | >0 才画；块 `(effect-1)%2`，只收 288/600 帧 | `items.rs:1321`、`net.rs:953` |
| 物品图标 | `ItemInfo.Image` | 直接作为 `items.wzl` 的 0 基下标 | `mir-client/src/ui/*`、`net.rs` |
| 小/大地图 | `MapInfo.MiniMap` / `BigMap` | 非 0 帧；`mmap.wzl` | `mir-render/src/minimap.rs` |
| 特效 | 代码固定帧段 | `effect.wzl[0..2]`、`magic.wzl[0..9]`、`magic2.wzl[0..2] + [20..29]` | `entities.rs` `FxKind`、`ui/spellfx.rs` |
| 音频 | `sound.rs` + 施法音 + BGM | 事件音 15 个 + 107 个技能的 `M{spell}-0.wav` + 30001/30002；按三处音效目录顺序解析 | `mir-render/src/sound.rs`、`mir-client/src/audio.rs` |
| 字体 | `resource/fonts/*` | 全部保留（当前 1 个 Noto Sans SC） | `mir-render/src/text.rs` |

### 2.1 关键换算（v1 地图，本数据集 388/389 张）

`cjlaaa` 地图是 **v1（`Map 2010 Ver 1.0`，15 字节/格）**。按 Crystal `MapCode.LoadMapType1`：

```
BackImage   = i32 @ o+0  XOR 0xAA38AA38      （bit15 → 高墙；画图时 (v & 0x1FFFFFFF) - 1）
MiddleImage = i16 @ o+4  XOR xor             （bit15 置位 = 无效）
FrontImage  = i16 @ o+6  XOR xor             （取 & 0x7FFF 后 -1）
FrontIndex  = u8  @ o+12 + 2                 （102→90；>=255→-1）
动画帧号    = u8  @ o+10 & 0x7F              （门/前层动画）
```

v0/v4 为 `o+0/o+2/o+4`（v4 再异或 xor）；v2/v3 的前层库从 +120 起（Mir3 编号，本项目未收录）。
**当前 `mir-world` 的 V1 分支没有解析这些字段、`mir-render` 的 back/middle 库只对 V0|V4 返回**，
所以默认数据集在客户端还画不出瓦片（已上报，兵部修复后本脚本可直接复跑）。本管线**按正确规则统计瓦片**，
不会因为渲染层缺口而漏掉资源。

### 2.2 帧级裁剪（不是整块）

`indices` 是**精确帧号集合**，依据 `crates/mir-render/src/animation.rs`：

| 素材 | 每块帧数 | 实际用到 | 组成 |
|---|---:|---:|---|
| `hum*/weapon*/hair*` | 600 | **240** | 站立 32 + 走 48 + 跑 48 + 攻击 48 + 受击 24 + 死亡 32 + 尸体 8 |
| `mon*` | 430 | **200** | 站立 32 + 走 48 + 攻击 48 + 受击 16 + 死亡 48 + 尸体 8 |
| `npc*` | 60 | **60** | 保守整块（站立帧表跨到下一图像） |
| `wingeffect_x` | 600 | **288** | 站立/走/跑/攻击各 64 + 受击 24 + 死亡 32 + 尸体 8 |
| `horse_x` | 640 | 232 | **渲染层必需**（`PlayerLayer::Mount`），3 块 → `mount-1..3`，已默认收录 |

按整块拆会多 2.15 倍（怪物）到 2.5 倍（人类）的帧；帧级裁剪后总帧数 177,918（不是约 40 万）。

---

## 3. 统计数据（2026-10-22，数据集 `cjlaaa`）

自动生成：[`target/asset-usage-report.md`](../../target/asset-usage-report.md)（每次跑脚本覆盖）。

### 3.1 总量

| 项 | 使用 | 总量 | 不进包 |
|---|---:|---:|---:|
| `.wzl` 图库 | **66 个 / 177,918 计划帧（150,108 可解）/ 914.5 MP**，PNG 估计 **1.37 GB** | 349 个 / 7,509,950 项 / 21.69 GB | 约 **21.32 GB**（98.3%） |
| 地图 | **385 个唯一文件**：`.map` 313.8 MB → `.mnm` **13.1 MB**（压缩比 4.2%，≈23.9×） | 1817 张 / 1250.3 MB | 1428 张 / 936.5 MB |
| 音频 | **104 个 / 38.6 MB** | — | 20 个引用文件在三处音效目录缺失 |
| 字体 | 1 个 / 8.3 MB | — | — |
| 服务端 | `Server.MirDB` + `Envir/` 1140 文件 / 1.45 MB | — | — |

> PNG 估计 = `used_pixels × 1.5`（经验中值；实际 PNG 约 1–3 字节/像素）。`used_pixels` 只统计**能解出来**的帧，
> 空帧/越界帧不计。最终体积以拆包器落地文件为准。

### 3.2 按分类

| 分类 | 图库数 | 说明 |
|---|---:|---|
| `tiles` | 21 | tiles/smtiles/objects1..27 中被地图引用到的部分 |
| `sprites`（mon/npc） | 32 | 怪物 157 个可落地实体 / 28,395 帧；NPC 63 个图像 |
| `dress`（hum） | 2 | `hum.wzl`、`hum2.wzl`（衣服 shape 1..39；`hum3+` 未用） |
| `weapons` | 3 | `weapon.wzl`、`weapon2.wzl`、`weapon3.wzl`（shape 0..218） |
| `hair` | 2 | `hair.wzl`、`hair2.wzl`（发型号 0..8；`hair3/4` 未用） |
| `items` | 1 | `items.wzl`，830 个图标（658 可解，172 空槽） |
| `ui` | 1 | `mmap.wzl`，199 帧（143 可解） |
| `effects` | 4 | `effect`/`magic`/`magic2`/`wingeffect_x`（翅膀只落 2 个美术块） |

### 3.3 最小范围（本单先跑通的 map0 + items + 一只怪物）

产物：[`target/used-assets-mini.json`](../../target/used-assets-mini.json)（可直接喂拆包器 `--dry-run`）。

| 范围 | 图库 | 帧数 | 可解 | 空帧 |
|---|---:|---:|---:|---:|
| 地图 0 用到的瓦片 | 9 个 tileset | 8,688 | 8,031 | 657 |
| `items.wzl` 全部图标 | 1 | 830 | 658 | 172 |
| 怪物 `mon-0130`（image=130，mon14 槽 0） | 1 | 192 | **104** | 88（mon14 库只到 780 帧） |

map0 明细：`tiles.wzl` 986 引用 / 665 可解、`smtiles.wzl` 170/167、`objects*` 7,532/7,199。
样例 PNG 已用独立解码器导出到 `target/game-assets-mini/`（16,689 张 PNG / 127.8 MB，见 §5）。

### 3.4 未使用的大头（为什么能省 21GB）

| 族 | 未用体积 | 说明 |
|---|---:|---|
| `hum7..9`、`hum08/09`、`weapon4..8` | ~5.6 GB | 客户端只认 `hum1..3`/`weapon1..3` 的 shape 段 |
| `hum_ef`、`weapon_ef`、`hum_ck_*`、`cbo*` | ~8.7 GB | Crystal 扩展特效库，本项目渲染层没有引用路径 |
| `mon*` 未引用的槽/帧 | ~0.87 GB | 本数据集只用到 157 个怪物实体；其余库/槽不拆 |
| `ui*`、`tiles2..17`、`smtiles2..`、`objects28+` | ~1.3 GB | 客户端 UI 是代码绘制；这些瓦片族是 Mir3/其他地图集 |
| `.wil/.wix` | ~0.98 GB | 只在启动时打开索引计数，不参与渲染；不复制 |

---

## 4. 产物与中间格式

### 4.1 `target/used-assets.json`（拆包计划，v1.1）

```json
{
  "version": 1,
  "generated_at": "…",
  "source_root": "resource/data",
  "dataset": "cjlaaa",
  "libraries": [
    {
      "lib": "tiles.wzl",
      "category": "tiles",
      "dest": "gfx/tiles/tiles0",
      "indices": [0, 1, 2],
      "outputs": [
        {"dest": "gfx/tiles/tiles0", "indices": [0, 1, 2], "reason": "…"}
      ],
      "reason": "地图后层瓦片（v1）×389",
      "used_frames": 3,
      "loaded_frames": 2,
      "dropped_out_of_range": 0,
      "entries": 31775,
      "nonempty": 17592,
      "source": "resource/data/tiles.wzl",
      "missing_indices": [9],
      "missing_count": 1
    }
  ],
  "maps": ["0", "0100"],
  "audio": ["1.wav"],
  "fonts": ["NotoSansSC-Regular.otf"],
  "notes": ["…"],
  "stats": {"…": "…"}
}
```

约定（与户部 t120 对齐）：

- `lib`：相对 `resource/data` 的真实文件名（保留大小写，如 `Mon11.wzl`）；
- `indices`：**库内 0 基**（= `mir_asset::ImageLibrary::image(i)`）；全部显式整数数组，
  没有整库 `"all"`（这套数据没有整库引用的库）；
- `outputs`：**新增**。同一 `lib` 按实体拆到多个目录时使用；每项 `{dest, entity?, indices, requested, loaded_frames, missing_count, aliases?, reason}`。
  只有单个输出的库（瓦片/UI/特效）也写一条，`dest` 与顶层 `dest` 相同，向后兼容旧版“一个 lib 一个 dest”；
- `dest`：拆包器输出目录（相对 `game-assets/`），每帧文件名 `NNNN.png`（库内 0 基四位）；
- `missing_indices` / `out_of_range`：指向空槽/越界的已用索引（拆包器跳过，不报错）；
- `used_frames` 是计划帧数，`loaded_frames` 是真正能解出像素的帧数。

### 4.2 `index.json`（每目录一份）

由拆包器生成，字段与户部约定一致：`lib`、`source`、`category`、`entity`、`count`、`requested`、
`skipped_empty`、`entries[]`（`index/width/height/offset_x/offset_y/format/png/bytes/rgb_sha256/rgba_sha256/pixels_sha256/png_sha256/empty`）。

**alpha 约定（跨实现对账用）**：

- `.wzl` 的 8 位调色板 `palette[0] = (0,0,0,255)` 本身**不透明**（Crystal `WeMadeLibrary` 直接 `Color.FromArgb(255,…)`）；
- 本管线在**拆包时**把 `(0,0,0)` 置 `alpha=0`（等价于 `mir-render::scene::remove_black` 与 Crystal `LibraryEditor` 的
  `removeBlack`），这样运行时 `UnpackedLibrary` 不需要再判断格式/做 color-key；
- 因此跨实现比较以 **`rgb_sha256`** 为准（RGB 一致 = 解码定稿）；`rgba_sha256`/`pixels_sha256` 校验我们的透明约定本身，
  alpha 策略差异不算解码错误（户部 t120 实测：252 条差异全部是 palette[0] 的 alpha，RGB 0 差异）；
- `format` 原样保留：以后若要改成“原样 alpha + 渲染端 color-key”，可按 `format` 还原，不需要重拆。
本仓库的独立实现（`export_asset_sample.py`）产出的 `index.json` 已包含同样字段，可用于交叉校验；
`aliases`（DB 名/中文名）后续由户部或本脚本补入。

---

## 5. 验证命令

```powershell
# 1) 引用分析（推荐用带 numpy 的解释器；纯标准库也能跑，只是地图扫描慢）
& "C:\Users\diana\.dsh\dsh-runtimes\dsh-primary-runtime\dependencies\python\python.exe" `
    tools/assets/analyze_usage.py
#   → target/used-assets.json、target/asset-usage-stats.json、target/asset-usage-report.md

# 2a) 生成冒烟计划（map0 + items + 一只怪物）
& "C:\Users\diana\.dsh\dsh-runtimes\dsh-primary-runtime\dependencies\python\python.exe" `
    tools/assets/make_mini_plan.py --map 0 --monster-image 130

# 2) 独立解码样例（第二实现，不依赖 mir-asset；用于校验 PNG/索引）
& "C:\Users\diana\.dsh\dsh-runtimes\dsh-primary-runtime\dependencies\python\python.exe" `
    tools/assets/export_asset_sample.py   # 依赖 Pillow（py 没有时用上面的 bundled Python）
#   → target/game-assets-mini/（每目录 PNG + index.json + manifest.json）

# 3) 计划自检（命名/唯一性/帧数/索引一致性；拆包前门禁）
py tools/assets/check_plan.py target/used-assets.json
py tools/assets/check_plan.py target/used-assets-mini.json

# 4) 全量拆包（分片并行；默认 6 片，自动找 mir-tools；--dry-run 只分片）
py tools/assets/split_extract.py --plan target/used-assets.json --out-root game-assets --parts 6
py tools/assets/split_extract.py --dry-run
py tools/assets/split_extract.py --verify          # 拆完再跑全量 --verify

# 5) 新布局校验（index/PNG 数 = 计划 loaded、抽样 rgb/rgba、分类体积、旧布局统计）
py tools/assets/verify_layout.py --plan target/used-assets.json --game-assets game-assets

# 6) 生成资源库清单（给客户端/文档一条命令列出）
py tools/assets/build_manifest.py --plan target/used-assets.json --verify-json target/layout-verify.json
#   → game-assets/manifest.assets.json、game-assets/index/summary.json

# 7) 发布包对账（kind=0 比 pixels_sha256；kind=1 比文件 sha256；kind=2 用 palette.bin 展开比 rgb_sha256；kind=3 校验 768B）
py tools/assets/reconcile_packs.py
py tools/assets/reconcile_packs.py --categories maps,data,index   # 只查原样条目（秒级）
py tools/assets/reconcile_packs.py --quick                        # 只查覆盖率，不重算图像哈希

# 7) 单库/单目录拆包与对账（户部工具）
mir-tools assets unpack --plan target/used-assets-mini.json --out-root game-assets --dry-run
mir-tools assets unpack --plan target/used-assets.json --out-root game-assets --verify
```

分析脚本自测（每次跑都会打印）：

- 地图：389 张 / 20,880,105 格 / 版本 `{1:388, 5:1}` / 缺 0；
- 图库：349 个 / 7,509,950 项（非空 5,329,305）；
- 音频：104 个可解析 / 20 个引用缺失（`M1-0.wav` 等技能音在本素材集没有）。
- 2026-10-22 与户部第二次交叉校验：**`rgb_sha256` 1214/1214 一致、0 差异**（items 658、mon-0130 104、
  tiles0/objects3/19/20/22 若干；272 条“两侧都空”未计入）。Rust（`mir-asset`）与 Python 独立解码逐字节一致，
  剩余差异仅为 palette8 去黑约定，已在 §4.2 定义。

---

## 6. 已知限制与缺口

1. **v1 地图渲染缺口（产品级 blocker，已上报）**：`mir-world/src/map.rs` 的 V1 分支 `image_xor: None`、
   `mir-render/src/scene.rs` 的 `back_library()/middle_library()` 只对 `V0|V4` 返回 → 用 `cjlaaa` 地图时
   客户端一格瓦片都不画。本管线的瓦片统计按 Crystal 正确规则计算，修复后直接复跑即可。
2. **扁平语料本身是稀疏的**：`.wzx` 里大量槽指向共享的全零记录（偏移 48）。因此
   `tiles.wzl` 的 23,218 个引用中有 8,943 个空槽、373 个越界；`items.wzl` 830 个图标中 172 个空槽。
   拆包器会跳过并记入 `missing`，不是解析错误。
3. **怪物帧布局是近似的**：`mir-render` 按“每图像 430 帧、每库 10 图像”取帧，但语料里各 `monN.wzl`
   的实际帧数不一（780～7,610），且 `!Mon.dat`（逐怪物帧表）尚未解析。结果：261 个怪物图像中
   **157 个**有可落地帧（28,395 帧），其余槽越界。修复需要解析 `!Mon.dat` 或改用逐库真实帧表；
   本管线的 `dropped_out_of_range` 已逐个记录。
4. **怪物特殊段**：`Image >= 540` 的 14 个模板（900..902 攻城/城门、10000..10010 宠物）在 Crystal 走
   `Siege/Gates/Pets` 库，语料里没有对应库，未拆；客户端目前也映射不到。
5. **`objects12.wzl` 文件缺失**：地图前层引用了它（3,973 帧），但 `resource/data` 里没有该文件，
   计划里已标注 `missing_libraries`，拆包器会报缺库。
6. **v5 地图 1 张**（Mir3 格式）不产生瓦片引用；v2/v3 的 Mir3 前层库（+120）也未收录。
7. **音频**：20 个技能施法音（`M1-0.wav` 等）在三处音效目录都不存在；`30001/30002` BGM 在
   `resource/community/*/Sound`，目前只登记不复制（若发布包需要 BGM 再单独收）。
8. **`.mnp` 已有实现（户部，§8）**：本轮未做发布包体积/加载验收；客户端 Pack 态待接。
9. **`resource/ld`**：4 个文件，但代码里只有路径访问器、没有读取路径，未纳入。
10. **PNG 体积是估计值**：`×1.5` 是经验中值；实际以 `mir-tools assets unpack --dry-run` 与落地文件为准。
11. **翅膀只有 2 个美术块**：`wingeffect_x.wzl` 共 1200 帧 = 2×600；`Effect=1/2/100` 映射到 2 块，
    `Effect=100`（破凰天魔衣）与 `Effect=2`（天龙不死衣）共用块 1，计划里以 `aliases:[100]` 标注、只落一份 PNG。

---

## 7. 与户部的契约（定稿）

| 项 | 约定 |
|---|---|
| 计划文件 | `target/used-assets.json`（全量）、`target/used-assets-mini.json`（map0+items+1 怪物，冒烟用） |
| 索引语义 | 库内 0 基，与 `ImageLibrary::image(i)` 一致 |
| 落盘路径 | `outputs[].dest` 优先；缺失时回退 `dest`，再回退 `gfx/<category>/<stem>` |
| 空帧/越界 | 跳过并写 `index.json` 的 `missing` / `skipped_empty`，**exit 0** |
| 对账 | `lib + count`，逐 index 比 **`rgb_sha256`**（解码定稿）；`rgba_sha256`/`pixels_sha256` 只校验透明约定 |
| 计划门禁 | `tools/assets/check_plan.py`：dest 唯一且不含原库名、output 帧数自洽、无重复帧集合、索引升序去重；失败退出码 1 |
| 拆包编排 | `tools/assets/split_extract.py`：按帧数均衡分片并行（32 核全量 ~10min，单进程 ~100min），可 `--verify` |
| 布局校验 | `tools/assets/verify_layout.py`：逐 output 对计划（PNG/index/loaded/missing）+ 抽样 rgb/rgba + 旧布局统计 |
| 资源清单 | `tools/assets/build_manifest.py`：`manifest.assets.json`（逐库 outputs、maps_detail 385×sha256、pack_hints、packs 分类） |
| 包对账 | `tools/assets/reconcile_packs.py`：`.mnp` 条目按逻辑名对散装（kind=0 `pixels_sha256`、kind=1 文件 sha256、kind=2 `palette.bin` 展开比 `rgb_sha256`、kind=3 校验 768B）+ 覆盖率 + 包完整性；失败退出码 1 |
| 帧文件名 | 四位库内 index（`0000.png`/`0054.png`），单帧类别也保留 `<entity>/<index>.png` 这一级 |

分工：**hubu2** 负责引用分析、命名规则、统计与独立解码校验；**hubu** 负责 Rust 拆包器与
`UnpackedLibrary` 加载；瓦片渲染修复归兵部（v1 解析）与渲染线。

---

## 8. 发布期分包 `.mnp`：格式与命令（户部）

开发期用**散装**（每图一文件 + 目录内 `index.json`）；发布期用**自有小包** `.mnp`（Mir Next Pack）。

### 8.1 为什么分包

发布后资源有变动时，只需推送**变化的那一类小包**，而不是 20GB 大包；单包上限由 `--chunk-mb` 控制（默认 16MB）。

### 8.2 包格式（自描述，全部小端）

```text
0x00  magic        "MNP1"                 4B
0x04  version      u16 = 1                2B
0x06  flags        u16                    2B   bit0 = 包尾有 sha256
0x08  entry_count  u32                    4B
0x0C  index_offset u64                    8B   条目表偏移
0x14  reserved     12B 全 0
0x20  …            条目数据区（zlib(RGBA) 或原样字节）
index_offset …     条目表：每项
                     name_len u16 + name(UTF-8，'/' 分隔)
                     kind u8（0=RGBA+zlib，1=原样）
                     has_mask u8 + format u8 + reserved[3]
                     width u16 + height u16
                     offset_x i16 + offset_y i16 + shadow_x i16 + shadow_y i16
                     data_offset u64 + data_len u32 + raw_len u32
文件尾 - 32        sha256(前面全部字节)
```

加载端只读条目表（`index_offset`）即可 `len/info`，`image()` 时才解压；**整包 sha256 不符直接报错**（抓篡改/传输损坏）。

### 8.3 命令

```powershell
# 散装 → 小包（单包 ≤16MB；按 category + 分组切桶）
mir-tools assets pack --input game-assets --out game-assets/packs --chunk-mb 16

# 只读检查
mir-tools assets pack --input game-assets --out game-assets/packs --list
mir-tools assets pack --input game-assets --out game-assets/packs --verify
```

产物：

```
packs/
  manifest.packs.json      # 每包 {file, category, bytes, sha256, entries, prefix}
  tiles-tiles0-1.mnp
  sprites-item-1.mnp
  sprites-monster-1.mnp
  maps-1.mnp / audio-1.mnp / data-1.mnp
```

`--verify` 做三件事：① 每个包 sha256 与 manifest 一致；② 条目数与字节数一致；
③ **输入目录里每个文件都能在包集合里按逻辑名找到**（防漏打）。

### 8.4 命名（与第 1 节一致，原库名只进 `index.json.source`）

```
gfx/tiles/<tileset>/<index>.png
gfx/sprites/<kind>/<entity>/<frame>.png      # monster/npc/player/item/effect
gfx/ui/<panel>/<index>.png
maps/<mapid>.map
audio/sfx/<name>.wav
data/server/Server.MirDB + data/server/Envir/…
```

包内条目名 = 相对 `game-assets/` 的路径，**和散装目录一一对应**，所以开发期与发布期用同一套逻辑名。

### 8.5 加载优先级

```
散装 game-assets/gfx/**（开发滴：改完即见）
  → packs/（发布：PackSet 读 manifest.packs.json）
  → 原 .wzl（兼容兜底：没拆包/没装素材也能跑）
```

`mir-asset::PackSet` 已提供与 `ImageLibrary` 同形的 `len/info/image`（按逻辑名或前缀筛子集），
`LibrarySource` 当前已实现 `Packed/Unpacked` 两态；`Pack` 态在客户端接 `asset_mode` 时补上。

### 8.6 已知限制

- 桶内按「未压缩上界」估算大小（RGBA 4 字节/像素），所以实际包会比 `chunk_mb` 小一些——**宁小勿超**；
- `.mnp` 暂不压缩非图像条目（`.map`/`.wav`/`.MirDB` 原样存，`kind=1`）；
- 分包按 `category + 分组` 切，不按内容相似度（后续可按热度/改动频率重切）。

## 启动校验：`open_unverified`

客户端启动用 `PackSet::open_unverified()`（只读包头/条目表），**不**逐包哈希 410 MB；
`mir-tools assets pack --verify/--check` 与发布流程负责整包 sha256 校验。
实测启动 `open`：8.3 s → 0.34 s。
