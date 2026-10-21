# `.mnd` 游戏数据格式（Mir Next Data）

> 状态：**M1 骨架已落地**（容器 + 导入工具 + 服务端加载优先级）。
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
| `sections` | total / db / envir_files / envir_bytes |
| `size_bytes` / `sha256` | `.mnd` 体积与整包哈希 |

## 3. 容器布局（M1）

小端；索引在前、payload 在后：

```text
偏移 0   magic         4 字节  "MND1"
       format_version u16     = 1
       schema_version u16     = 1
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

- 魔数/`format_version` 不匹配直接报错；`schema_version` 只记录，语义校验由上层做；
- 未知 `tag` 保留在 `MndFile.sections` 里，上层忽略即可（TLV 扩展点）；
- 未知 `codec` 报错；`raw_len` 与解压结果不一致报错；偏移越界/截断报错；
- 段名必须 UTF-8；导入工具按 `Envir/<相对路径>`（正斜杠）命名，读取端拒绝 `..` 等穿越组件。

### 3.1 段类型（tag）

| tag | 名称 | 内容 | codec |
|---|---|---|---|
| `0x0001` | `db` | DB 载荷（见 §4） | zlib |
| `0x0002` | `Envir/*` | 脚本文本（原 UTF-8 文本） | zlib |
| `0x0003` | 保留 | manifest/调试段（M1 未用） | — |

### 3.2 压缩

- M1 所有段统一 **zlib**（`flate2::Compression::default()`）；段内独立压缩，可随机读单段；
- 大文件后续可加 `codec=2`（zstd/字典）而不动容器布局；
- 已实测 cjlaaa：`Server.MirDB` 452,106 B + 1,139 个 Envir 文件 → `game.mnd` **665,264 B（约 650 KiB）**，
  导入+校验 0.8–4.9 s（冷/热构建差异）。

## 4. DB 段（M1 过渡 + M2 原生）

### 4.1 M1（当前）
`db` 段 = 导入工具读出的 `Server.MirDB` 字节（解析校验通过后原样入包，zlib 压缩）。
服务端 `GameData::load_mnd` 用 `MirDb::from_bytes` 解析**包内字节**，不再读 `resource/` 文件。

> 为什么先这样：`mir-db` 结构体还没有 serde/原生编码器；M1 先打通「运行时只读自有包」，
> 让 `.mnd` 的容器、索引、manifest、加载优先级与测试先落地。

### 4.2 M2（计划）
把 `db` 段换成**原生表编码**（`codec` 新值 + `schema_version` 升级）：

| `MirDb` 段 | 原生编码计划 | 备注 |
|---|---|---|
| `header` | 固定字段表 | 版本/各段计数器 |
| `maps` | 行表 + 变长字符串池 | 含 movements/respawns/safezones/minezones |
| `items` | 行表 + stats 字典 | 字段与 `mir-protocol::data::ItemInfo` 对齐 |
| `monsters` | 行表 + stats/掉落路径 | `MonsterInfo` |
| `npcs` | 行表 + 字符串池 | `NpcInfo` |
| `quests` | 行表 | `QuestInfo` 头 |
| `magics` | 行表 | `MagicInfo`（spell 去重规则保留） |
| `gameshop` / `conquests` / `respawn` / `gt_maps` / `dragon` | 行表 | 与 `mir-db` 段一一对应 |

目标：删除「包内再解析老 MirDB 字节」这一步，老格式彻底只存在于导入工具。

## 5. Envir 段映射

导入工具递归打包 `Envir/` 下**所有文件**，段名 = `Envir/` + 相对路径（`\`→`/`）：

| 老目录 | 用途 | 服务端读取 |
|---|---|---|
| `Envir/NPCs/**` | NPC 脚本（`NpcScript::load`） | `GameData::npc_script` |
| `Envir/Quests/**` | 任务脚本（`QuestScript::load`） | `GameData::quest_script` |
| `Envir/Drops/**` | 掉落表（`DropTable::load`） | `GameData::drop_table` |
| `Envir/Recipe/**` | 合成配方 | 现有惰性读取路径 |
| `Envir/Routes/**` | 路线脚本 | 同上 |
| `Envir/SystemScripts/**` | 系统脚本 | 同上 |
| `Envir/NameLists/**`、根目录 `*.txt` | 名称表/公告等 | 同上 |

**M1 加载方式**：`GameData::load_mnd` 把 `Envir/*` 段解到系统临时目录
`%TEMP%/mir-next-mnd-<path-hash>/`，再复用现有脚本解析器；服务端运行时不读 `resource/`。
M2 计划改为内存虚拟 FS（`NpcScript::load`/`QuestScript::load`/`DropTable::load` 接受
`&dyn ScriptSource`），去掉临时目录落盘。

> M1 已知限制：临时目录只按 `.mnd` 路径 hash，**同路径替换数据包后旧脚本文件不会被清理**；
> 正式发布前用新的 `--out` 目录或清 `%TEMP%/mir-next-mnd-*`。M2 虚拟 FS 后此问题消失。

## 6. 导入工具

新 bin：`crates/mir-tools/src/bin/data_import.rs`（不改 `mir-tools/src/main.rs`）。

```text
cargo run -p mir-tools --bin data_import -- \
  --source resource/community/suprcode-crystal-database/cjlaaa \
  --out game-assets/data [--json] [--verify]
```

- `--source`：数据集目录（含 `Server.MirDB` + `Envir/`）或直接给 `Server.MirDB` 文件；
- `--out`：输出目录（默认 `game-assets/data`），产出 `game.mnd` + `manifest.json`（`--json` 再加 `game.json`）；
- `--verify`：回读 `.mnd`，比较 DB 各段计数与 Envir 段数；
- 工具只负责**读老格式 → 写自有格式**；服务端不调用它。

## 7. 服务端加载优先级

`mir-server::load_game_data`：

1. `<asset_root>/data/game.mnd` 存在 → `GameData::load_mnd`（正常路径）；
2. 不存在且**开发期**（`debug_assertions` 或 `MIR2_ALLOW_LEGACY_DATA=1`）→ WARN 后回退
   `config.db_path` / `GameData::discover()`（老 `Server.MirDB`）；
3. 不存在且 release → WARN 并跳过数据加载（不允许旧格式进正式运行）。

`asset_root` 解析见 `config.rs`（`MIR2_ASSET_DIR` > 配置 > `game-assets` > `resource` 回落）。

## 8. 验证与测试

- `crates/mir-db/src/mnd.rs` 单测：段往返、压缩、坏魔数/截断；
- `crates/mir-server/tests/mnd_data.rs`：老库 → `.mnd` → `GameData` 读回，
  `MirDb` **逐字段相等**；NPC 脚本与掉落表从包内解出可用；
- `mir-server/src/server.rs` 单测：有 `.mnd` 时优先加载；无 `.mnd` 时开发期回退旧库；
- `data_import --verify`：导入后立即回读校验。

### 8.1 实测（cjlaaa，2026-10-22）
| 指标 | 旧格式 | `.mnd`（M1） |
|---|---|---|
| 运行期读取的文件 | `Server.MirDB` + 1,139 个 `Envir/*.txt`（惰性） | 单个 `game.mnd`（665,264 B） |
| 加载耗时 | 11.99 ms（DB 段；脚本惰性，尚未读） | 664.8 ms（含 1,139 份脚本**立即解包**到临时目录） |
| 服务端启动日志 | `已加载 Server.MirDB path=...cjlaaa/Server.MirDB` | `已加载 .mnd 数据包 path=game-assets\data\game.mnd schema_version=1 sections=1140 scripts=1139` + `使用 .mnd 数据包` |
| 进图验证 | — | `出生地图已初始化 map=0 npcs=45 groups=358 spawned=2252`；新角色收到 5 条 `S.NewItemInfo`，NPC/地图/欢迎语中文 |

> M1 的 `.mnd` 加载比「旧 DB 惰性加载」慢，原因是**一次性解包 1,139 份脚本**；M2 改内存
> 虚拟 FS 后脚本仍按需解析，耗时预计回到与旧路径同量级或更低（少了 1,139 次目录/文件打开）。

## 9. 未覆盖 / 后续

| 项 | 状态 | 计划 |
|---|---|---|
| DB 原生编码（去老 MirDB 字节） | 未做 | M2 §4.2 |
| Envir 内存虚拟 FS（去临时目录） | 未做 | M2，`ScriptSource` 抽象 |
| `Envir/Recipe`、`Routes` 等的专门缓存 | 沿用现有惰性路径 | 需要时补 |
| 地图瓦片 `.mnm` | 兵部负责 | 单独格式 |
| 图像/音频资源 | 户部负责 | `game-assets/gfx|audio` |
| 多数据集（Jev/Daneo） | 未做 | 同一工具换 `--source`，manifest 记录来源 |
| 增量/分包（单包过大时） | 未做 | 段索引已支持按需读；可加 `game-<hash>.mnd` 分片 |
| 版本迁移（旧 `.mnd` → 新 schema） | 未做 | `schema_version` + 迁移工具 |
