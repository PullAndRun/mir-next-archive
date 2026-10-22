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
  root_rel text NOT NULL DEFAULT 'game-assets',  -- 多资源根隔离（prune 按 root 生效）
  updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS assets_file_category_idx ON assets.file (category, kind);
CREATE INDEX IF NOT EXISTS assets_file_sha_idx ON assets.file (sha256);
CREATE INDEX IF NOT EXISTS assets_file_root_idx ON assets.file (root_rel, category);

-- 变更历史：created / changed / removed（删除也能被 changed --since 看到）
CREATE TABLE IF NOT EXISTS assets.file_history (
  id BIGSERIAL PRIMARY KEY,
  path text NOT NULL,
  action text NOT NULL,
  bytes bigint,
  sha256 text,
  run_id bigint REFERENCES assets.catalog_run(id),
  at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS assets_file_history_run_idx ON assets.file_history (run_id, action);
```

> 相对队长给的原始 DDL 只多一列 `root_rel`：同步测试根（如 `target/asset-catalog-test`）时
> `--prune` 必须按 root 隔离，否则会误删 `game-assets` 的行（已实测并修复）。

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
py tools/assets/catalog.py stats [--top 30] [--root game-assets]
py tools/assets/catalog.py changed --since <run_id>
py tools/assets/catalog.py dupes [--limit 30] [--root game-assets]
py tools/assets/catalog.py pack-plan [--chunk-mb 16] [--root game-assets] [--out plan.json] [--check manifest.packs.json]
py tools/assets/catalog.py self-test
```

- `sync`：按 `path` 幂等 upsert，只有**体积/分类/kind/output_dir/origin/root/sha 真变化**才更新
  `run_id`/`updated_at`；`--prune` 只删**同一 root** 下已不存在的行；`--hash` 计算 sha256（3 GB 级会慢些）。
- `stats`：`category × kind` 与 `category` 两级汇总（按 `--root` 过滤，默认 `game-assets`）。
- `changed`：读 `assets.file_history`，按 `run_id > since` 输出 `created/changed/removed` 分组；
  删除也能看到（`removed`）。基线行为：历史表首次启用/清空后，存量行补记 `created` 并沿用
  文件原 `run_id`，不会被后续 `changed --since` 误报。
- `dupes`：按 sha256 找重复组与浪费体积（需先 `--hash` 同步过）。
- `pack-plan`：按 category 出分包计划，每片 ≤ `--chunk-mb`；
  `packs:[{name,category,files:[{path,bytes,sha256}],bytes}]`；
  `--check <manifest.packs.json>` 对账实际包：先核对同目录 `.mnp` 的**整包 bytes + sha256**，
  再比较包名、条目数、逐条目 path/bytes/sha256；`--check` 发现任何缺失/不一致都 exit 1。
- `self-test`：不连库，校验路径拒绝规则 + 分类样例。

## 4. 实测（2026-10-22，`game-assets/`）

### 4.1 `sync --dry-run`（首次快照）
```
扫描 game-assets：156313 个文件
文件总数：156313；总字节：3174750008（3027.7 MiB）
按 category：gfx.tiles 104160 / gfx.sprites 46986 / maps 2587 / data 2283 / gfx.ui 144 /
             audio 104 / packs 44 / index 4 / fonts 1
```
> 其他成员随后并发重建资源库（`map/`、`packs/`、`data/server/` 被裁剪/重建）；正式同步时
> 是 151,786 个文件、1,112,457,543 B（1060.9 MiB）。这正说明「目录清单进 PG」比肉眼数文件可靠。

### 4.2 `sync --hash --prune`（历史基线，run_id=9）
```
扫描 game-assets：151786 个文件
...
已清理 game-assets 下多余的 0 行（history 记 removed）
history 基线补记 151786 行（created，沿用文件原 run_id）
同步完成：run_id=9，151786 行，history 151786 条
```
> upsert 只在**内容/分类/体积/root/sha 真的变化**时更新 `run_id`/`updated_at`（SQL
> `ON CONFLICT ... WHERE`），所以 `changed --since` 不会被「重复同步同一批文件」刷满。

### 4.3 `stats`（最终快照）
```
assets.file[root=game-assets]：151786 行，1112457543 字节（1060.9 MiB）

category      kind           files       MiB
gfx.tiles     png           104139     611.9
gfx.sprites   png            45826     245.5
gfx.tiles     json              21      86.4
gfx.sprites   json            1160      45.4
audio         wav              104      36.8
gfx.ui        png              143      13.1
maps          mnm              385      12.5
fonts         otf                1       7.9
data          mnd                1       0.6
index         json               3       0.4
data          json               2       0.2
gfx.ui        json               1       0.1
```
> `data/` 现在只有 `game.mnd`/`game.json`/`manifest.json`；之前的 `data/server/Server.MirDB`
> + `Envir/**` 与 `server/**` 老格式副本已被户部二号清掉（原 `data/mirdb` 1 条已消失）。

### 4.4 `changed` 的 created/changed/removed（临时 root 自测）
```
run_id > 10 的变更（created/changed/removed）：
  other         changed         1      0.0 MiB
  other         created         1      0.0 MiB
  (removed)     removed         1      0.0 MiB
```
> 用 `target/asset-catalog-test` 做增/改/删三步，`--prune` 按 `root_rel` 隔离，主库
> `game-assets` 的 151,786 行不受影响；测试后已清理。

### 4.5 `dupes`（前 5）
```
重复组：20（浪费约 7.5 MiB）
  x12  1339596 B  c416a09f7c52…  gfx/sprites/npc/npc-0034/2040.png
  x12  1197708 B  47b28d3bebed…  gfx/sprites/npc/npc-0045/2700.png
  x10   935450 B  30d567b0105c…  gfx/sprites/npc/npc-0079/4770.png
  x2    759968 B  1eb49d868181…  audio/sfx/M38-0.wav
  x2    611656 B  64b3b54e1550…  audio/sfx/M41-0.wav
```
> 硬链接去重**暂缓**（见 §5）：20 组 ~3.7 MiB 收益低，且硬链接会干扰打包/manifest 的 sha 对账。

### 4.6 `pack-plan --chunk-mb 16` + `--check`
```
已写出 target/asset-pack-plan.json：73 个分片，1112457543 字节
计划 73 个分片，1112457543 字节（用 --out 写完整 JSON）
对账通过：73 个实际包与计划一致
```
> 逐条目 `files:[{path,bytes,sha256}]` 已包含在 plan JSON（19.5 MB）；`--check` 对实际
> `manifest.packs.json` 先核对同目录 `.mnp` 的整包 bytes+sha256，再比较包名、条目数、逐条目
> path/bytes/sha256；故意改错 1 个条目数 + 1 个字节 + 1 个整包 sha 时，正确输出 3 项失败并 exit 1。

## 5. 已知限制与后续

- `changed` 已覆盖删除（`file_history` 的 `removed`）；历史表清空后第一次同步会做 created 基线，
  基线沿用文件原 `run_id`，不会误报。
- `sha256` 默认不算；`dupes` 需要带 `--hash` 同步过；后续同步沿用旧 sha（COALESCE）。
- **硬链接去重暂缓**（队长决定）：`dupes` 仅 20 组 ~3.7 MiB，收益低，且硬链接会让 `.mnp` 打包
  与 manifest sha256 对账复杂化。
- 多资源根用 `root_rel` 隔离；`stats/dupes/pack-plan` 默认只看 `game-assets`，可用 `--root` 指定。
- 与户部分包对齐：`pack-plan` 输出的 `name/category/files/bytes` 供 `assets pack --plan <json>`
  读取；`--check` 用于打包后对账。`assets pack` 的 `--plan` 由户部实现，本工具不碰其 CLI。
- `.mnp` 打包后的产物再同步一次即可看到 `packs/**` 的体积；户部重建 `packs/` 期间数字会跳动。
