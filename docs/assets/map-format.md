# `.mnm`（Mir Next Map）地图格式

`.mnm` 是《传奇-Next》运行时自有地图格式；老 Crystal `.map`（v0/v1/v2/v3/v4/v5/v6/v7/v100）
只在**导入/构建**阶段由 `map_import` 读取。运行时的 `Map::load` 会优先找同名 `.mnm`，
没有才回退 `.map`，因此迁移期可逐张切换。

## 1. 设计目标

- **语义无损**：可走性、HighWall/LowWall、门、光照、钓鱼、back/middle/front 图像号、
  `front_index`、动画帧/ticks 全部保留；可逐格与源 `.map` 比较。
- **体积**：389 张 `cjlaaa/Maps`（约 314MB）目标 ≤ 原 1/3。当前骨架用 zlib + varint RLE，
  逐层压缩；后续可把同值层换成更紧凑的字典/位平面，不改变文件版本。
- **流式友好**：back/middle/front/属性/杂项各自独立压缩块，可按层解压，不必先解整张图。
- **可扩展**：尾部 TLV；未知 tag 直接跳过，不要求升版本。

## 2. 文件布局（little-endian）

```text
+0   magic        [4]  = "MNM1"
+4   version      u16  = 1
+6   flags        u16  = 0（bit0 预留 sha256）
+8   width        u16
+10  height       u16
+12  source_ver   u8   = 源 .map 的 MapVersion::raw()
+13  reserved     u8
+14  name_len     u16  | name bytes(UTF-8，无后缀)
+..  door_count   u16
+..  doors[]      index u8, x i32 zigzag-varint, y i32 zigzag-varint
+..  layer[5]     每层：u32 compressed_len + zlib(bytes)
+..  tlv_count    u16
+..  TLV[]        tag u16, len u32, data[len]
```

五个 layer 的顺序固定：

| 层 | 内容 | 编码 |
|---|---|---|
| 0 | back image | varint RLE，值 `u32`（v1 的 `BackImage` 是 i32） |
| 1 | middle image | varint RLE，值 `u16` |
| 2 | front image | varint RLE，值 `u16` |
| 3 | attribute | 每格 1 字节（0 Walk / 1 HighWall / 2 LowWall），zlib |
| 4 | misc | 每格 9 字节，zlib |

`misc` 每格：

```text
fishing_attribute i8 | door_raw u8 | light u8 | client_door_index u8 |
door_offset u8 | front_animation_frame u8 | front_animation_tick u8 |
front_index i16
```

`door_index` 由 `door_raw` 重算（`>0` 时 `&0x7F`），不单独存；门表另存以保留
`Door.location`。RLE 的 run 和 value 都是 LEB128 varint，run 必须 ≥1。

## 3. 与 `.map` 字段映射

| `MapCell` 字段 | v0 | v1 | v4 | `.mnm` |
|---|---|---|---|---|
| `attribute` | `u16@0/@2/@4` bit15 | `u32@0^0xAA38AA38` bit29 / `u16@6^xor` bit15 | `u16@0/@2` bit15 | `attribute` layer |
| `fishing_attribute` / `light` | `@11` | `@13` | 不读 | misc |
| `door_raw` / `door_index` | `@8` | `@8` | `@6` | misc + 重算 |
| `back_image` | `u16@0` | `i32@0^0xAA38AA38` | `u16@0^xor` | back RLE |
| `middle_image` | `u16@2` | `i16@4^xor` | `u16@2^xor` | middle RLE |
| `front_image` | `u16@4` | `i16@6^xor` | `u16@4^xor` | front RLE |
| `client_door_index` | `@6&0x7F` | `@8&0x7F` | `@6&0x7F` | misc |
| `door_offset` | `@7` | `@9` | `@7` | misc |
| `front_animation_frame` | `@8`（客户端语义，服务器门字节） | `@10&0x7F` | 无 | misc |
| `front_animation_tick` | `@9` | `@11` | `@7` | misc |
| `front_index` | `@10` | `@12 + 2`（102→90，>=255→-1） | `@10` | misc i16 |
| `version` | 0 | 1 | 4 | `source_ver` |

v1 的 `BackImage` 使用 32 位字段；bit29 参与高墙判定，渲染时仍按低 15 位取图号。
`front_index` 在 v1 已经是 tileset 序号（已含 +2/映射），render 不再加 base。

## 4. 压缩与体积

- 三个图像层：varint RLE + zlib；大片 `0` 会压成很少的 run。
- 属性层：1 字节/格，zlib；杂项层：9 字节/格，zlib。
- 每层独立压缩块，客户端可按需解一层。
- 当前不存 sha256（`flags.bit0` 预留）；导入工具的 `--verify` 会立即读回逐格比较。
- 样本实测（`.mnm / .map`）：
  - `resource/map/0100.map`：3292 B → 419 B（0.127）
  - `cjlaaa/Maps/0100.map`：4104 B → 422 B（0.103）
  - `mirfiles-apoc/.../LABY01.map`（v4）：480064 B → 28593 B（0.060）

## 5. 运行时加载与导入

```text
Map::load(path)         // 若同名 .mnm 存在→读 .mnm；否则读老 .map
Map::load_legacy(path)  // 强制老 .map（导入工具）
Map::open_mnm(path)     // 强制 .mnm
map_import --input <dir|glob> --out game-assets/maps --format mnm [--dry-run] [--verify]
```

## 6. 测试覆盖

- `mir-world::mnm::tests::mnm_roundtrip_is_lossless`：合成地图全字段往返。
- `mir-world` 真实 `.map` 往返测试（v0/v1/v4 各一张，文件缺失 skip）。
- `map_import --verify`：转换后立即逐格比较；体积统计打印在 stdout。

## 7. 未覆盖 / 后续扩展

- v5/v6/v7/v100 的导入目前仍可走 `Map::load_legacy`，但 `.mnm` 的 `source_ver` 已能保留；
  渲染层只对 v0/v1/v4 有 tileset 规则，其它版本沿用原行为。
- 尚未存 `TileAnimationImage/Offset/Frames`（v3/v4 有，当前 `MapCell` 未暴露）。
- 尚未存 sha256（flag 预留）；未来可作为 TLV tag `1` 加入，不升版本。
- 389 张全量转换尚未执行（本单只做格式 + 骨架 + 单样本验证）。
