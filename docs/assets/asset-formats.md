# 《传奇-Next》自有资源格式总览

> 运行期原则（队长 2026-10-22 定）：**游戏运行时只读我们自己的格式**；
> `resource/` 里的老格式（`.wzl/.wix/.wil/.map/.wav/Server.MirDB/Envir/*.txt`）只允许被
> **导入/构建工具**读取。违反即 bug。
>
> 责任划分：`.mnp`/图像/音频（户部）、`.mnm` 地图（兵部，见 [`map-format.md`](map-format.md)）、
> `.mnd` 数据（兵部二号，见 [`data-format.md`](data-format.md)）、打包/引用分析见
> [`asset-pipeline.md`](asset-pipeline.md)。

## 1. 格式清单

| 资源 | 老格式（只读于工具） | 我们的格式 | 运行期位置 | 状态 |
|---|---|---|---|---|
| 图像 | `.wzl/.wix`、`.wil/.wix`、旧 `.Lib` | **散装 PNG**（开发）→ **图集 atlas**（发布）→ `.mnp` 打包 | `game-assets/gfx/**`、`game-assets/packs/*.mnp` | 散装 ✅、`.mnp` ✅、atlas ⏳ |
| 地图 | `.map`（v0/v1/v2/v3/v4/…） | **`.mnm`**（zlib + varint RLE 分层） | `game-assets/maps/<id>.mnm` | ✅（兵部，压缩比 4.2%） |
| 数据 | `Server.MirDB` + `Envir/*.txt` | **`.mnd`**（容器 + schema 版本） | `game-assets/data/game.mnd`（+ `game.json` 调试） | ✅ M1（兵部二号） |
| 音频 | `.wav`（PCM） | **`.mna`**（压缩音频） | `game-assets/audio/**` | ⏳ 本次设计 |
| 字体/文本 | `.ttf` 等 | 原样（非游戏资源格式） | `game-assets/fonts/` | ✅ |

版本号约定：所有容器第一字段都是 `format_version`（u16/u32）；**读方拒绝未知主版本**、
未知次版本按"忽略未知字段 + 默认值"处理；写方升级主版本时保留一份旧版读取器。

## 2. `.mnp`（Mir Next Pack）——已落地

- 一包 ≤ `--chunk-mb`（默认 16MB）；按类别 + 分组切包：`tiles-tiles0-1.mnp`、
  `sprites-monster-1.mnp`、`maps-1.mnp`、`audio-1.mnp`、`data-1.mnp`、`index-1.mnp`。
- 布局：`magic "MNP1" | version u16 | flags u16 | entry_count u32 | index_offset u64 | reserved`
  → 数据区 → 条目表（name/kind/has_mask/format/reserved/宽高/offset/shadow/data_offset/data_len/raw_len）
  → 包尾 `sha256[32]`（对包体）。
- 条目 `kind`：`0` = RGBA + zlib、`1` = 原样字节（`.mnm`/`.mnd`/字体）。
- 清单 `manifest.packs.json`：每包 `{file, category, bytes, sha256（整包）, entries, prefix}`。
- 自校验：读包时先验包尾 sha256；`mir-tools assets pack --check <manifest>` 逐包核
  字节/整包 sha256/条目数；`--verify` 额外核"输入目录每个自有格式文件都在包里"。
- 加载：`mir-asset::PackSet` 只读条目表，`image()` 才解压；`LibrarySource::Pack(..)` 与
  `ImageLibrary` 同形（`len/info/image`）。

#### 2.0.1 确定性打包

`PackWriter` 在内存里攒条目，`finish` 时**按逻辑名排序**写数据区与条目表：
同一组素材无论扫描顺序如何，产物逐字节相同；`--merge-manifest` 单包重打与整轮重打 sha 一致。
`manifest.packs.json` 带 `generator` / `deterministic: true` / `chunk_mb`。

发布指纹可复现：设置 `SOURCE_DATE_EPOCH` 后打包，`generated_at` 固定为该时间戳，
`manifest.packs.json` 也逐字节相同。示例：

```bash
SOURCE_DATE_EPOCH=1792645029 cargo run -p mir-tools -- assets pack \
  --input game-assets --out game-assets/packs --chunk-mb 16
```

### 2.0 启动校验策略（`open` vs `open_unverified`）

`.mnp` 包尾有整包 `sha256`（32 B）。读取端有两种打开方式：

| API | 校验 | 用途 |
|---|---|---|
| `PackFile::open` / `PackSet::open` | 逐包校验整包 sha256 | 导入/构建、`mir-tools assets pack --verify/--check`、发布前检查 |
| `PackFile::open_unverified` / `PackSet::open_unverified` | **只解析包头 + 条目表**，不哈希整包 | 客户端启动（信任分发包） |

实测（45 包 / 409.67 MB）：客户端 release 启动 `open` 从 **8.3 s → 0.34 s（约 24×）**；
完整性由发布流程与 `--verify` 兜底，运行期不再为 410 MB 逐字节算哈希。

## 2.1 条目 `kind` 表（含 palette8 索引化）

| kind | 内容 | 说明 |
|---|---|---|
| `0` | `zlib(RGBA)` | 通用图像（rgb565 / rgb24 / DXT1 / 反查失败的 palette8） |
| `1` | 原样字节 | `.mnm` 地图、`.mnd` 数据、字体、`audio/manifest.json` |
| `2` | `zlib(8bit 索引)` | **palette8 帧**：行优先、自上而下、无行内 padding；`raw_len = width×height` |
| `3` | 原样 768B RGB | 调色板：逻辑名 `<帧所在目录>/palette.bin`，`palette[i]=(r,g,b)` |

- 展开：`index==0 → alpha=0`（等价 `remove_black`），其余 `alpha=255`；缺 `palette.bin` 时回退内置 WeMade 表；
- 适用：`format ∈ {0,3}` 的帧；`empty:true` 的帧不打包；
- 对账：`kind=2` 用 `index.json.rgb_sha256` 比（只比 RGB，alpha 由规则决定）；
- 收益（实测）：tiles -23.4%、sprites -20.3%、ui -29.4%，总包 521.8 MB → **410.26 MB**。

### 2.2 `library-map.json`（散装 ↔ 库名）

`{"version":1,"libraries":{"hum":{"lib":"hum.wzl","dirs":["gfx/sprites/player/body-000", …],
"prefix":"gfx/sprites/player/body-000/"}}}`

- 键是库 stem（`hum.wzl → hum`），与渲染层 `EntityLibraries` 一致；
- `dirs` 用于开发期（一库多目录）→ `LibrarySource::UnpackedDirs`；
- `prefix` 用于发布期（`.mnp` 里的逻辑名前缀）→ `LibrarySource::Pack`；
- 由 `mir-tools assets unpack --plan … --out-root …`（`--map-only` 只补表不重拆）生成，
  `assets pack` 会把它复制进 `packs/`。

## 3. 图像图集 atlas（下一步）

目标：把"每帧一个 PNG"改成"每实体一张图集 + 元数据"，减少文件数与随机 I/O，并让 `.mnp`
里的条目数从 15 万降到几千。

```
gfx/<category>/<entity>/atlas.png       # 单张纹理（RGBA8，按 4 像素对齐打包）
gfx/<category>/<entity>/atlas.json      # {"version":1,"lib":"hum.wzl","entity":"body-003",
                                        #  "frames":[{"index":1800,"x":0,"y":0,"w":96,"h":120,
                                        #             "offset_x":-13,"offset_y":-44,
                                        #             "shadow_x":0,"shadow_y":0,"has_mask":false}]}
```

- 布局算法：按帧高度降序、行式装箱（shelf packing），纹理宽度上限 2048，超出换行；
- 与散装 PNG **像素一致**（解包后逐字节比较）；
- 加载：`mir-asset::AtlasLibrary` 与 `ImageLibrary` 同形，`image(i)` 直接切子矩形
  （避免每帧解 PNG，加载时间预计降一个数量级）。
- 体积目标：atlas 的 PNG（或 `.mnp` 内的 zlib RGBA）≤ 散装 PNG 总和的 **80%**
  （同纹理合并后 zlib 窗口利用率更高；实测后写数字）。

## 4. 音频 `.mna`（本次设计）

需求：104 个 `.wav`（36.8 MB PCM）→ 目标 **≤15 MB**（队长给的指标），且保留采样率/声道。

候选（都在 Windows + 纯 Rust 优先的约束下评估）：

| 方案 | crate | 类型 | 预计体积 | 备注 |
|---|---|---|---|---|
| **FLAC** | `flacenc`（纯 Rust 编码）/ `claxon`（解码） | 无损 | 30–50%（11–18 MB） | 无外部 C 依赖，最简单；体积可能刚好压线 |
| **Opus** | `audiopus`/`opus`（libopus，C） | 有损 | 5–10%（2–4 MB） | 压缩率最好；需要 vendored C 构建，Windows 交叉编译风险 |
| **Vorbis** | `vorbis-encoder`（纯 Rust，质量一般） | 有损 | 10–15% | crate 维护活跃度低 |

决策：**先做 FLAC**（无损、零风险、可逆），实测若 ≤15 MB 就用它；否则再引入 Opus 作为
可选编码（`--codec opus`），并把 FLAC 保留为无损选项。

容器 `.mna`：

```
magic "MNA1" | version u16 | codec u8（0=flac，1=opus）| channels u8 | sample_rate u32 |
frames u64 | data_len u32 | data | sha256[32]
```

工具：`mir-tools audio import --input game-assets/audio --out game-assets/audio --codec flac`
（`.wav` → `.mna`，删 `.wav`，写 `audio/manifest.json`）。
加载：`mir-client` 音频后端（`rodio`，已支持 wav/flac 解码器）直接播 `.mna` 里的 FLAC 流。

## 5. 体积与验收目标（实测会回填）

| 项 | 原始 | 现状 | 目标 |
|---|---|---|---|
| 图像 | `resource/data` 21.69 GB 原库 | 拆包 gfx **1,002 MiB** / 150,108 PNG（用到 150,108 / 177,918 帧） | `.mnp` **≤ 500 MB**（实测 47 包 / 497.9 MB） |
| 地图 | `.map` 299.2 MiB / 389 张 | `.mnm` **12.54 MiB** / 385 张（4.2%，23.9×） | ≤ 原 1/3 ✅ |
| 数据 | `Server.MirDB` 452 KB + `Envir/` 1,140 文件 | `.mnd` **665 KB** + `game.json` 213 KB | 单文件 ≤ 1 MB ✅ |
| 音频 | `.wav` 36.8 MB / 104 | 未转换 | **≤ 15 MB** |
| 运行时读取 | — | `game-assets/{gfx,maps,audio,data,index,packs}` | **零老格式** |

## 6. 兼容策略

- **开发期（`asset_mode=dev`）**：散装 `gfx/**` + `maps/*.mnm` + `data/game.mnd`；
- **发布期（`asset_mode=release`）**：只读 `packs/*.mnp`（含 `library-map.json` 与 `manifest.packs.json`）；
- **`auto`**：有散装用散装，否则用 `packs`；**不回退老格式**；
- **`legacy`**（仅开发兼容）：`散装 → packs → 老格式`，启动打 WARN，发布包禁止；
- 老格式只允许出现在 `tools/**` 与 `mir-tools … import` 子命令里。
