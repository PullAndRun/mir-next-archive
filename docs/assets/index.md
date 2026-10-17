# 素材索引（`mir2_dev`）

项目的所有游戏素材（地图、客户端资源包、音效、启动器资源、社区仓库归档）都放在仓库根的
`resource/` 下，并由 PostgreSQL 数据库 `mir2_dev` 建立索引。**取素材的顺序**：

1. 先在索引里查（`tools/asset-index/find_asset.py`）；
2. 命中就按索引里的相对路径去 `resource/` 取文件；
3. 索引里没有 → 联网下载（优先 `GOPEED_EXEC`）→ 存进 `resource/` → 重新跑索引脚本补登。

## 目录约定

| 目录 | 内容 |
| --- | --- |
| `resource/map/` | 客户端地图 `.map`（`mir-world` 的 `Map::load` 直接读） |
| `resource/data/` | 客户端数据与资源包：`.wzl`/`.wzx`（图片库）、`MapInfo.dat`、`ItemDesc.dat`、音效配置等 |
| `resource/wav/` | 音效 |
| `resource/ld/` | 登录器/更新器资源（`.pak`） |
| `resource/community/` | 社区仓库归档：每个子目录是一个 GitHub 仓库快照，`SOURCES.md` 记录出处 |

`resource/` 不进 git（体积太大），索引库才是「哪里有素材」的唯一事实来源。

## 索引结构

```sql
-- 每个文件一行，path 相对 resource/ 且唯一
resources(
    id bigserial primary key,
    path text not null unique,   -- 例：community/clgforever910-mir2database/Daneo1989/Server.MirDB
    top text not null,           -- 顶层目录：community / data / map / wav / ld
    name text not null,          -- 文件名
    ext text not null default '',-- 小写扩展名，含点
    size bigint not null,
    mtime timestamptz,
    sha256 text,                 -- 只对 --hash-ext 指定的扩展名计算，其余为 NULL
    indexed_at timestamptz not null default now()
)

-- 社区仓库的 SOURCES.md 原文（知道素材从哪来，避免版权/来源不明的文件混进来）
resource_sources(path text primary key, content text not null, indexed_at timestamptz)
```

为什么不给全部文件算哈希：`resource/community/` 有约 230 万个文件、300 GB，全量哈希要跑几个小时；
`size + mtime` 已足够判断文件是否换过，需要强校验时用 `--hash-ext` 对特定扩展名单独算。

## 用法

```bash
# 建立/更新索引（幂等，可反复跑；--roots 限定目录，--dry-run 只统计）
python tools/asset-index/index_resources.py
python tools/asset-index/index_resources.py --roots data map wav ld
python tools/asset-index/index_resources.py --roots community --hash-ext .MirDB

# 查询
python tools/asset-index/find_asset.py --name Server.MirDB
python tools/asset-index/find_asset.py --ext .wzl --top data --limit 20
python tools/asset-index/find_asset.py --repo crystal-database --json
```

脚本默认从仓库根 `.env` 读 `MIR2_DEV_DATABASE_URL`（回退 `DATABASE_URL`）。

## 已知的关键素材

| 用途 | 索引里怎么找 |
| --- | --- |
| Crystal 服务端数据库（怪物/物品/NPC/刷怪点） | `--name Server.MirDB`（`community/clgforever910-mir2database/*/Server.MirDB`，另有 `suprcode-crystal-database/cjlaaa/Server.MirDB` 作校验样本） |
| Crystal 配置（BaseStats/ExpList/GoodsSystem…） | `--name Configs --top community` 后看 `*/Configs/*.ini` |
| 客户端图片库 | `--ext .wzl --top data` |
| 地图 | `--ext .map --top map` |
| 中文字体 | `--name NotoSans --ext .otf`（本地 `resource/fonts/NotoSansSC-Regular.otf`，SIL OFL 1.1；客户端字体发现的第一顺位就是 `resource/fonts/`） |

`Server.MirDB` 的二进制格式见 [`docs/store/mirdb-format.md`](../store/mirdb-format.md)，
Rust 读取实现在 `crates/mir-db`。

## 字体

客户端的文字渲染（`mir-render::TextRenderer`）按以下顺序找字体，找到第一个可用的就用：

1. `resource/fonts/*.ttf|*.otf|*.ttc`（按路径排序）；
2. Windows 系统字体 `msyh.ttc` → `simhei.ttf` → `simsun.ttc`；
3. Linux 常见路径（DejaVu / Noto CJK / WQY / Droid）与 `/usr/share/fonts` 下常见中文字体；
4. 都没有时退化成内置 3×5 点阵（ASCII 可读，中文空白）并打印一次警告。

仓库**不携带**字体文件（`resource/` 不进 git）。为了保证中文在任何机器上都可显示，
把 **Noto Sans SC**（SIL OFL 1.1，可自由分发）放到 `resource/fonts/`：

```bash
# 已在索引里：先用索引确认，再按路径取
python tools/asset-index/find_asset.py --name NotoSans --ext .otf
# 没有就下载（Google 的 noto-cjk 仓库，SIL OFL 1.1）
curl -L -o resource/fonts/NotoSansSC-Regular.otf \
  https://raw.githubusercontent.com/notofonts/noto-cjk/main/Sans/SubsetOTF/SC/NotoSansSC-Regular.otf
python tools/asset-index/index_resources.py --roots fonts
```
