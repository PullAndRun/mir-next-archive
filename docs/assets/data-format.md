# `.mnd` 游戏数据格式（Mir Next Data）

> 状态：**M2 已落地**（原生 DB 编码 + Envir bundle 内存 FS；M1 只读兼容）。
> 负责人：兵部二号（服务端经济与成长线 / 数据）。地图 `.mnm` 见 `docs/assets/map-format.md`（兵部），
> 图像/音频见户部文档。
> 运行时原则：**服务端只读 `game-assets/data/game.mnd`**；老格式（`Server.MirDB`、
> `Envir/*.txt`）只允许出现在导入工具与开发期回退里。

## 1. 目标与边界

- 一个紧凑、可校验、可扩展的**运行时数据包**，替代 `Server.MirDB` + `Envir/` 文本目录；
- 内容覆盖服务端启动所需：物品/怪物/NPC/地图/任务/技能/商店/掉落/经验/传送点/攻城等
  （`mir-db` 现有结构）+ `Envir/` 脚本（NPC/Quests/Drops/Recipe/Routes/SystemScripts/NameLists）；
- 不做：地图瓦片（`.mnm` 单独格式）、图像/音频（`gfx/`、`audio/` 由户部格式负责）、
  客户端专用资源。
- 兼容策略：`format_version` 管容器布局，`schema_version` 管数据语义；两者独立升级。

## 2. 目录与产物

```
game-assets/data/
  game.mnd          发布用数据包（服务端唯一读取目标）
  manifest.json     来源/计数/sha256/段统计（校验与排错）
  game.json         --json 调试索引（段名/sha256/大小，不含正文）
```

`manifest.json` 字段：

| 字段 | 含义 |
|---|---|
| `schema_version` / `format_version` | 数据语义版本 / 容器版本 |
| `magic` | 固定 `MND1` |
| `source.db` / `source.envir` | 导入来源路径（审计用） |
| `created_unix` | 导入时刻（Unix 秒） |
| `counts` | 各段条数：maps/items/monsters/npcs/quests/magics/gameshop/conquests/gt_maps |
| `db_encoding` / `db_bytes` | `bincode`；原生 DB 段未压缩字节数 |
| `envir_bundle` | `true`（M2 把 1,139 份脚本打成一个 bundle 段） |
| `sections.configs` / `config_bytes` | Configs 文件数 / 原始字节数 |
| `sections` | total / db / envir_files / envir_bytes / configs / config_bytes |
| `size_bytes` / `sha256` | `.mnd` 体积与整包哈希 |

## 3. 容器布局（M2）

小端；索引在前、payload 在后：

```text
偏移 0   magic         4 字节  "MND1"
       format_version u16     = 2（M1=1 只读兼容）
       schema_version u16     = 2
       flags          u16     = 0（保留）
       reserved       u16     = 0
       section_count  u32
       index_bytes    u32
index  section_count 条，逐条：
       tag            u16     段类型
       codec          u8      0=raw，1=zlib
       flags          u8      0（保留，逐段）
       raw_len        u64     解压后长度
       stored_len     u64     payload 中长度
       offset         u64     相对 payload 起点
       name_len       u16
       name           UTF-8（段名）
payload 各段压缩字节（按索引顺序拼接）
```

读取端规则：

- 魔数不匹配报错；`format_version` 只接受 `1/2`（M1 只读兼容），`schema_version` 只记录；
- 未知 `tag` 保留在 `MndFile.sections` 里，上层忽略即可（TLV 扩展点）；
- 未知 `codec` 报错；`raw_len` 与解压结果不一致报错；偏移越界/截断报错；
- 段名必须 UTF-8；导入工具按 `Envir/<相对路径>`（正斜杠）命名，读取端拒绝 `..` 等穿越组件。

### 3.1 段类型（tag）

| tag | 名称 | 内容 | codec |
|---|---|---|---|
| `0x0001` | `db` | **M1**：`Server.MirDB` 原始字节（只读兼容） | zlib |
| `0x0002` | `Envir/*` | **M1**：逐文件脚本文本（只读兼容） | zlib |
| `0x0003` | 保留 | manifest/调试段 | — |
| `0x0004` | `db.bin` | **M2**：`mir_db::schema` 的 bincode 原生 DB（见 §4） | zlib |
| `0x0005` | `envir.bin` | **M2**：全部 Envir 脚本 bundle（见 §5） | zlib |
| `0x0006` | `Configs/*` | `Configs/ExpList.ini`、`GuildSettings.ini` 等运营配置（见 §5.1） | zlib |

### 3.2 压缩与体积

- 所有段统一 **zlib**（`flate2::Compression::default()`）；段内独立压缩，可随机读单段；
- M2 只压 2 个段（`db.bin` + `envir.bin`），不再为 1,139 份脚本各建一个 zlib 流；
- 已实测 cjlaaa：`Server.MirDB` 452,106 B + 1,139 个 Envir 文件（1,001,133 B）+
  30 个 `Configs/*`（46,851 B）→ `game.mnd` **283,323 B（约 276 KiB）**，
  比 M1 的 665,264 B **小约 57%**；导入+校验约 0.5–0.9 s。

## 4. DB 段（M2 原生）

### 4.1 M2（当前）
`db.bin` 段 = 导入工具把 `Server.MirDB` 解析成 `MirDb` 后，用 `mir_db::schema::encode_db`
序列化的 **bincode**（varint 编码）。服务端 `GameData::load_mnd` 直接 `decode_db`，
**不再解析老 MirDB 字节**；老格式只存在于导入工具。

字段一一对应 `mir-db` 结构：

| `MirDb` 段 | 内容 |
|---|---|
| `header` | 版本/各段计数器（`MirDbHeader`） |
| `maps` | 地图 + safezones/respawns/movements/minezones |
| `items` | 物品模板 + `Stats`（含经验/价格/需求等全部字段） |
| `monsters` | 怪物模板 + `Stats`/掉落路径/经验 |
| `npcs` | NPC 模板（地图/坐标/文件名） |
| `quests` | 任务头 |
| `dragon` | 龙系统 |
| `magics` | 技能（保留 Crystal 的 spell 去重语义） |
| `gameshop` | 商城条目（含绑定后的 `ItemInfo`） |
| `conquests` | 攻城（含 archer/structure/flag） |
| `respawn` | 刷怪计时器 |
| `gt_maps` | GT 城尾段 |

`MirDb.sections`（老文件字节范围）是解析产物，`#[serde(skip)]` 不写入 `.mnd`；
读回后为空，比较时按数据字段逐项比较（测试里的 `assert_db_content_eq`）。

### 4.2 M1 只读兼容
`format_version=1` 的 `db` 段仍是 `Server.MirDB` 原始字节；`GameData::load_mnd` 检测到
`TAG_DB` 时用 `MirDb::from_bytes` 解析。M2 导入器不再产出 M1，兼容只保留读取。

## 5. Envir bundle + 内存 FS

导入工具递归打包 `Envir/` 下**所有文件**，M2 写成一个 `envir.bin` 段：

```text
u32 count
重复 count 次：u32 name_len | name | u32 data_len | data
```

`name` 是相对 `Envir/` 的 POSIX 路径（`NPCs/...`、`Quests/...`、`Drops/...`）。

| 老目录 | 用途 | 服务端读取 |
|---|---|---|
| `Envir/NPCs/**` | NPC 脚本 | `GameData::npc_script` |
| `Envir/Quests/**` | 任务脚本 | `GameData::quest_script` |
| `Envir/Drops/**` | 掉落表 | `GameData::drop_table` |
| `Envir/Recipe/**`、`Routes/**`、`SystemScripts/**`、`NameLists/**`、根 `*.txt` | 其他脚本/名称表 | `GameData::read_script`（按需） |

**M2 加载方式**：`GameData::load_mnd` 解压 `envir.bin` 后只建
`HashMap<路径, (offset,len)>` 索引，正文留在一整块 `blob` 里；`read_script`/`npc_script`/
`quest_script`/`drop_table` 都从内存切片，**不再写临时目录**（`envir_dir()` 为 `None`）。
NPC 的 `#INSERT` 在内存里展开（追加到行尾再删指令行，与 Crystal 一致）。

### 5.1 Configs 段

导入工具把 `Configs/**` 递归打进 `TAG_CONFIG` 段，段名 = `Configs/<相对路径>`：

| 文件 | 服务端用途 |
|---|---|
| `Configs/ExpList.ini` | 等级经验表（`ExpTable`） |
| `Configs/GuildSettings.ini` | 行会等级/人数/费用/增益（`GuildSettings`） |
| 其他 `Configs/*.ini` | 目前未读取，一并打包供后续使用 |

服务端 `GameData::read_config("Configs/ExpList.ini")` / `read_config("Configs/GuildSettings.ini")`
优先从 `.mnd` 读；读不到才回退旧 `config/` 或 `resource/` 候选路径（开发期 WARN）。
这样纯 `MIR2_ASSET_DIR=game-assets` 起服时不会再读 `resource/.../Configs/*.ini`。

## 6. 导入工具

新 bin：`crates/mir-tools/src/bin/data_import.rs`（不改 `mir-tools/src/main.rs`）。

```text
cargo run -p mir-tools --bin data_import -- \
  --source resource/community/suprcode-crystal-database/cjlaaa \
  --out game-assets/data [--json] [--verify]
```

- `--source`：数据集目录（含 `Server.MirDB` + `Envir/`）或直接给 `Server.MirDB` 文件；
- `--out`：输出目录（默认 `game-assets/data`），产出 `game.mnd` + `manifest.json`（`--json` 再加 `game.json`）；
- `--verify`：回读 `.mnd`，解码原生 DB 段并**逐字段比较**（header/maps/items/monsters/npcs/
  quests/dragon/magics/gameshop/conquests/respawn/gt_maps），再核对 Envir bundle 份数；
- 工具只负责**读老格式 → 写自有格式**；服务端不调用它。

## 7. 服务端加载优先级

`mir-server::load_game_data`：

1. 显式 `config.db_path` / `MIR2_DB_PATH` 优先：`.mnd` → `GameData::load_mnd`；旧格式仅在
   开发期（`debug_assertions` 或 `MIR2_ALLOW_LEGACY_DATA=1`）允许；
2. 没有显式 `db_path` 时：`<asset_root>/data/game.mnd` → `GameData::load_mnd`（正常路径）；
3. 缺失且开发期 → WARN 后 `GameData::discover()`（老 `Server.MirDB`）；release 缺失 → 跳过。

`asset_root` 解析见 `config.rs`（`MIR2_ASSET_DIR` > 配置 > `game-assets` > `resource` 回落）。
`derived_db_path()` 的优先级：`<asset_root>/data/game.mnd` → `<asset_root>/data/server/Server.MirDB`
→ `<asset_root>/server/Server.MirDB`（后两者仅过渡/兼容）；显式 `db_path`/`MIR2_DB_PATH` 指向
`.mnd` 时也直接走 `load_mnd`。

## 8. 验证与测试

- `crates/mir-db/src/mnd.rs` 单测：段往返、压缩、坏魔数/截断、版本 1/2 读取；
- `crates/mir-db/src/schema.rs`：DB bincode 往返、Envir bundle 编解码；
- `crates/mir-server/tests/mnd_data.rs`：
  - M1 只读兼容：原始 `db` 段仍能加载；
  - M2：原生 `db.bin` + `envir.bin` → `GameData` 读回，`MirDb` **数据字段逐项相等**
    （header/maps/items/monsters/npcs/quests/dragon/magics/gameshop/conquests/respawn/gt_maps）；
  - 内存 FS：1,139 份脚本 `read_script` 全部命中，`envir_dir()` 为 `None`，加载前后临时目录数量不变；
  - Configs：`ExpList.ini`/`GuildSettings.ini` 从 `.mnd` 读回，`ExpTable`/`GuildSettings` 与源文件
    **逐项相等**；缺失配置返回 `None`；
  - NPC 脚本、掉落表从 bundle 直接解析；
- `mir-server/src/server.rs` 单测：有 `.mnd` 时优先加载；无 `.mnd` 时开发期回退旧库；
- `data_import --verify`：原生 DB 关键字段逐项对比 + Envir 份数。

### 8.1 实测（cjlaaa，2026-10-22）
| 指标 | 旧格式 | `.mnd` M1 | `.mnd` M2 |
|---|---|---|---|
| 体积 | `Server.MirDB` 452,106 B + 1,139 个文本 + 30 个 Configs | 665,264 B | **283,323 B（-57%）** |
| 运行期读文件 | 1 个 DB + 1,139 个脚本（惰性）+ Configs | 1 个包 | 1 个包 |
| 加载（debug） | 10.9 ms（DB；脚本惰性） | 76.0 ms（1,139 段解压） | **48.6 ms**（DB decode 23.4 ms + bundle 1.0 ms + 解压/建索引） |
| 加载（release） | 1.76 ms | 10.5 ms | **9.5 ms**（decode_db 1.04 ms + bundle 0.85 ms） |
| 启动日志 | `已加载 Server.MirDB` | `format_version=1 ... scripts=1139` | `format_version=2 schema_version=2 sections=32 scripts=1139 configs=30`；`已加载等级经验表 source=".mnd Configs/ExpList.ini"`；`已加载行会配置 source=".mnd Configs/GuildSettings.ini"`；`已加载数据包 ... source=".mnd"`；`已加载地图 path=...0.mnm source="mnm"` |
| 进图 | — | — | `出生地图已初始化 map=0 npcs=45 groups=358 spawned=2252`；新角色 5 条 `S.NewItemInfo`；NPC/地图/欢迎语中文 |

> M2 debug 仍比「旧 DB 惰性加载」慢，主要是 bincode 在 debug 下解码 315 KB 原生 DB 的开销；
> release 已回到 9.5 ms（旧 DB 1.76 ms，同一数量级）。Envir 已不再解临时目录；
> 纯 `MIR2_ASSET_DIR=game-assets` 起服已不读 `resource/` 的 Configs。

## 9. 未覆盖 / 后续

| 项 | 状态 | 计划 |
|---|---|---|
| DB 原生编码（去老 MirDB 字节） | **已做（M2）** | bincode + `schema_version=2` |
| Envir 内存虚拟 FS（去临时目录） | **已做（M2）** | `ScriptFs` + bundle |
| Configs（ExpList/GuildSettings） | **已做（M2）** | `TAG_CONFIG`；纯 `game-assets` 起服不再读 `resource/` |
| 其他 `Configs/*.ini` | 已打包、暂未读取 | 需要时接对应模块 |
| `Envir/Recipe`、`Routes` 等的专门缓存 | 沿用 `read_script` 按需 | 需要时补 |
| 地图瓦片 `.mnm` | 兵部负责 | 单独格式 |
| 图像/音频资源 | 户部负责 | `game-assets/gfx|audio` |
| 多数据集（Jev/Daneo） | 未做 | 同一工具换 `--source`，manifest 记录来源 |
| 增量/分包（单包过大时） | 未做 | 段索引已支持按需读；可加 `game-<hash>.mnd` 分片 |
| 版本迁移（旧 `.mnd` → 新 schema） | M1 只读兼容 | 后续迁移工具；M2 写出端固定 2 |
| bincode 字段演进 | `schema_version` 守门 | 字段变更时升级 schema 并写迁移 |
