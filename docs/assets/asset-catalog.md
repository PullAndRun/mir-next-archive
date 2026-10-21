# 素材目录数据库（`assets` schema）

> 状态：已落地（`tools/assets/catalog.py`，2026-10-22 第 27 轮）。
> 目的：`game-assets/` 有 15 万级文件，用文件树管理「类型/体积/重复/该打进哪个包」太笨重；
> 把元数据同步进 PostgreSQL 的 `assets` schema，**只存相对资源根的 POSIX 路径，不存绝对地址**。

## 1. 表结构

```sql
CREATE SCHEMA IF NOT EXISTS assets;

CREATE TABLE IF NOT EXISTS assets.catalog_run (
  id BIGSERIAL PRIMARY KEY,
  started_at timestamptz NOT NULL DEFAULT now(),
  finished_at timestamptz,
  root_rel text NOT NULL,          -- 相对路径，例如 "game-assets"
  files bigint, bytes bigint, note text
);

CREATE TABLE IF NOT EXISTS assets.file (
  id BIGSERIAL PRIMARY KEY,
  path text NOT NULL UNIQUE,       -- 相对资源根的 POSIX 路径
  category text NOT NULL,          -- gfx.tiles / gfx.sprites / gfx.ui / maps / audio / data / fonts / packs / index
  kind text NOT NULL,              -- png / json / mnm / mnd / wav / mnp / ttf / otf / mirdb / txt ...
  bytes bigint NOT NULL,
  sha256 text,                     -- 只有 `--hash` 才计算
  output_dir text,                 -- 逻辑实体目录，如 gfx/sprites/monster/mon-0130
  origin text NOT NULL,            -- imported / generated / authored
  run_id bigint REFERENCES assets.catalog_run(id),
  updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS assets_file_category_idx ON assets.file (category, kind);
CREATE INDEX IF NOT EXISTS assets_file_sha_idx ON assets.file (sha256);
```

工具首次运行会自动建 schema/表/索引；**只写 `assets` schema**，不碰账号/存档表。

## 2. 路径与分类规则

- 路径必须：相对、POSIX（`/`）、无空段、无 `.`/`..`、无盘符、无反斜杠、无 NUL；
  绝对路径直接拒绝（`safe_relative()`）。
- `root_rel` 也只存相对路径（`--root` 必须是相对目录）。
- `category`：按资源根第一/第二级目录判定：

| 路径前缀 | category | 说明 |
|---|---|---|
| `gfx/tiles/**` | `gfx.tiles` | 瓦片 PNG + 索引 JSON |
| `gfx/sprites/**` | `gfx.sprites` | 怪物/装备/特效等 PNG + 索引 JSON |
| `gfx/ui/**` | `gfx.ui` | UI 图 |
| `maps/**`、`map/**` | `maps` | `.mnm` 与老 `.map`（来源不同，用 `origin` 区分） |
| `data/**`、`server/**` | `data` | `game.mnd`、`Server.MirDB`、`Envir/**` |
| `audio/**` | `audio` | `sfx/`、`music/` 的 wav |
| `packs/**` | `packs` | `.mnp` 资源包 |
| `fonts/**` | `fonts` | 字体 |
| `index/**`、根目录 `*.json` | `index` | 索引/清单 |

- `kind`：扩展名小写；无扩展名 = `none`；`Server.MirDB` = `mirdb`。
- `output_dir`：文件所在逻辑目录（父目录 POSIX 路径；根文件 = `.`）。
- `origin`：
  - `imported`：`map/**`（老 `.map`）、`server/**`、`data/server/**`、`fonts/**`；
  - `generated`：`gfx/**`、`maps/**`、`audio/**`、`packs/**`、`data/game.mnd`、`index/**`；
  - `authored`：保留（当前无）。

## 3. CLI

连接串优先级：`--dsn` > 环境变量 `MIR2_ASSET_DB` > `.env` 的 `DATABASE_URL` > 环境变量 `DATABASE_URL`。

```text
py tools/assets/catalog.py sync --root game-assets [--dsn ...] [--hash] [--prune] [--dry-run] [--batch 2000] [--note ...]
py tools/assets/catalog.py stats [--top 30]
py tools/assets/catalog.py changed --since <run_id>
py tools/assets/catalog.py dupes [--limit 30]
py tools/assets/catalog.py pack-plan [--chunk-mb 16] [--out plan.json]
py tools/assets/catalog.py self-test
```

- `sync`：按 `path` 幂等 upsert；每次运行写一条 `catalog_run`（`run_id`）；`--prune` 删除库里
  已不存在的行；`--hash` 计算 sha256（3 GB 级别会慢一些）；`--dry-run` 只扫描打印。
- `stats`：`category × kind` 与 `category` 两级汇总。
- `changed`：`run_id > since` 的行（新增/更新）；删除的行已被 `--prune` 清掉，无法回溯。
- `dupes`：按 sha256 找重复组与浪费体积（需先 `--hash` 同步过）。
- `pack-plan`：按 category 顺序、每片 ≤ `--chunk-mb` 出 JSON 清单（`category/files/bytes/name`）。
- `self-test`：不连库，校验路径拒绝规则 + 分类样例。

## 4. 实测（2026-10-22，`game-assets/`）

### 4.1 `sync --dry-run`
```
扫描 game-assets：156313 个文件
文件总数：156313；总字节：3174750008（3027.7 MiB）
按 category：gfx.tiles 104160 / gfx.sprites 46986 / maps 2587 / data 2283 / gfx.ui 144 /
             audio 104 / packs 44 / index 4 / fonts 1
```
> 随后其他成员在并发重建资源库（`map/`、`packs/` 被裁剪/重建），正式同步时是 152,927 个文件、
> 1,113,907,431 B（1062.3 MiB）。这正说明「目录清单进 PG」比肉眼数文件可靠。

### 4.2 `sync --hash --prune`（run_id=1 首次全量）
```
扫描 game-assets：152927 个文件
...
同步完成：run_id=1，152927 行
```
> upsert 只在**内容/分类/体积真的变化**时更新 `run_id`/`updated_at`（SQL `ON CONFLICT ... WHERE`），
> 所以 `changed --since` 不会被「重复同步同一批文件」刷满。

### 4.3 `stats`（run_id=3 快照）
```
assets.file：152955 行，1397942401 字节（1333.2 MiB）

category      kind           files       MiB
gfx.tiles     png           104139     611.9
packs         mnp               28     270.9
gfx.sprites   png            45826     245.5
gfx.tiles     json              21      86.4
gfx.sprites   json            1160      45.4
audio         wav              104      36.8
gfx.ui        png              143      13.1
maps          mnm              385      12.5
fonts         otf                1       7.9
data          txt             1138       1.0
data          mnd                1       0.6
data          mirdb              1       0.4
index         json               4       0.4
data          json               2       0.2
gfx.ui        json               1       0.1
data          none               1       0.0
```
> 期间户部在并发重建 `packs/*.mnp`，所以 `packs` 从 12 → 28 条；这正是 `changed` 的价值。

### 4.4 `changed --since 2`
```
run_id > 2 的文件：
  packs         mnp             17    147.1 MiB

最近批次：
  run=3 ... files=152955 bytes=1397942401 root=game-assets
  run=2 ... files=152939 bytes=1244905645 root=game-assets
  run=1 ... files=152927 bytes=1113907431 root=game-assets
```

### 4.5 `dupes`（前 5）
```
重复组：20（浪费约 7.5 MiB）
  x12  1339596 B  c416a09f7c52…  gfx/sprites/npc/npc-0034/2040.png
  x12  1197708 B  47b28d3bebed…  gfx/sprites/npc/npc-0045/2700.png
  x10   935450 B  30d567b0105c…  gfx/sprites/npc/npc-0079/4770.png
  x2    759968 B  1eb49d868181…  audio/sfx/M38-0.wav
  x2    611656 B  64b3b54e1550…  audio/sfx/M41-0.wav
```
> 全量重复浪费不止 7.5 MiB（`--limit` 只取体积最大的 20 组）；后续可据此做硬链接/去重。

### 4.6 `pack-plan --chunk-mb 16`
```
已写出 target/asset-pack-plan.json：82 个分片
audio-1..3 / data-1 / fonts-1 / gfx-sprites-1..N / gfx-tiles-1..N / gfx-ui-1 / index-1 / maps-1 / packs-*
```

## 5. 已知限制与后续

- `changed` 只统计**真正变化**（体积/分类/kind/output_dir/origin/sha 任一不同）的行；
  删除的行依赖 `--prune` 清理，当前无法从 `assets.file` 回溯「删了什么」；需要完整历史时再加
  `assets.file_history`（记录 path + run_id + action）。
- `sha256` 默认不算；`dupes` 需要带 `--hash` 同步过。
- 目前按「文件」粒度；`.mnp` 打包后的产物再同步一次即可看到 `packs/**` 的体积。
- 与户部分包对齐：`pack-plan` 输出的 `name/category/files/bytes` 可直接对照
  `mir-tools assets pack` 的输入；后续可加 `--emit mnp-manifest` 直接生成打包清单。
