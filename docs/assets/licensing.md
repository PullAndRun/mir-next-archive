# 素材授权政策与现状

日期：2026-10-21｜基线 HEAD `9ede73f`｜范围：`resource/` 下的游戏素材（不含仓库源码/文档）。

## 1. 政策声明（最新，优先于本文件其它统计）

> **2026-10-20 项目所有者裁决：`resource/` 目录中的资源无视资源授权，全部可以使用。**

* 本政策**仅覆盖 `resource/` 内资源**（地图、图库/贴图、音效、字体等）；
* 仓库自身的 Rust 源码与文档仍按仓库 `README`/`LICENSE` 的许可处理（GPL-3.0-or-later），
  不因本政策改变；
* `resource/` 已被 `.gitignore` 排除，不进入仓库；发布包是否携带素材是**打包策略**，
  不再作为授权判定条件；
* 现有 `SOURCES.md`/`SOURCES*.md`、`resource_sources` 索引继续作为**溯源记录**保留，
  便于以后追溯来源，但**不再逐目录核验授权**。

音频子集的技术可行性（格式/映射/播放冒烟）见
[`docs/audio/feasibility.md`](../audio/feasibility.md)。

## 2. 现状统计（可复跑，2026-10-21 实测）

### 2.1 口径

* 统计脚本：`target/licensing_stats.py`（临时脚本，未入库；下面贴完整命令）；
* **递归**遍历（`os.walk`），目录数**包含根目录**；
* 一个目录「有 SOURCES」= 该目录**直接包含** `SOURCES*.md`（大小写敏感，匹配
  `SOURCES.md`/`SOURCES.*.md`）；不把祖先目录的记录算给子目录；
* `files_without_sources` = 直接父目录没有 `SOURCES*.md` 的文件数。

### 2.2 命令

```powershell
# 与 target/licensing_stats.py 等价的只读统计（PowerShell）
$roots = 'resource/data','resource/map','resource/wav'
foreach ($root in $roots) {
  $files = Get-ChildItem $root -File -Recurse
  $dirs  = @((Get-Item $root).FullName) + (Get-ChildItem $root -Directory -Recurse | ForEach-Object FullName)
  $sources = Get-ChildItem $root -File -Recurse -Filter 'SOURCES*.md'
  [pscustomobject]@{
    Root = $root; Dirs = $dirs.Count; Files = $files.Count;
    Sources = $sources.Count; DirsWithout = $dirs.Count - ($sources.DirectoryName | Sort-Object -Unique).Count;
    FilesWithout = ($files | Where-Object { $sources.DirectoryName -notcontains $_.DirectoryName }).Count
  }
}
```

> 注：上面的 PowerShell 是等价写法；本次实际运行的是 Python 脚本（`os.walk`，更快），
> 两者口径一致。实际输出见 §2.3。

### 2.3 输出（本机实测）

```text
{'root': 'resource/data', 'dirs': 12, 'files': 920, 'sources': 0,
 'dirs_with_sources': 0, 'dirs_without_sources': 12, 'files_without_sources': 920}
{'root': 'resource/map', 'dirs': 1, 'files': 1116, 'sources': 0,
 'dirs_with_sources': 0, 'dirs_without_sources': 1, 'files_without_sources': 1116}
{'root': 'resource/wav', 'dirs': 1, 'files': 786, 'sources': 0,
 'dirs_with_sources': 0, 'dirs_without_sources': 1, 'files_without_sources': 786}
--- community top-level
{'dirs': 521, 'with_sources': 519, 'without_sources': 2,
 'without_names': ['capitainblack-mir2', 'mirfiles-data-hdassets']}
```

| 目录 | 目录数（含根） | 文件数 | `SOURCES*.md` | 无 SOURCES 的目录 | 无 SOURCES 的文件 |
|---|---:|---:|---:|---:|---:|
| `resource/data` | 12 | 920 | 0 | 12 | 920 |
| `resource/map` | 1 | 1116 | 0 | 1 | 1116 |
| `resource/wav` | 1 | 786 | 0 | 1 | 786 |
| `resource/community`（仅顶层仓库快照） | 521 | — | **519** | **2** | — |

**复核结论**：第 23 轮礼部报的 **519/521** 正确；缺 `SOURCES.md` 的两个仓库快照是
`capitainblack-mir2`、`mirfiles-data-hdassets`。主素材目录（data/map/wav）**完全没有**
`SOURCES*.md`，这与第 23 轮音频调研时的观察一致。

> 为什么社区目录有、主目录没有：`resource/community/` 是按「GitHub 仓库快照」采集的，
> 采集流程会自动写 `SOURCES.md`；`data/map/wav` 是直接按索引从客户端/归档挑出来的文件，
> 没有经过同一套来源登记流程。按 §1 政策，这不是阻塞项；但作为溯源记录，建议后续补
> `resource/data/SOURCES.md`、`resource/map/SOURCES.md`、`resource/wav/SOURCES.md`。

## 3. 发布前建议（工程注意事项，不与政策冲突）

1. **发布包只含实际用到的素材**：`package.ps1` 当前不打包 `resource/`；若将来要带，
   按地图/图库/音效清单裁出最小集，不要整目录 700+ MB 全带。
2. **保留 `resource/community/**/SOURCES*.md` 出处信息**：即使授权已由所有者裁决，
   出处仍对排查「素材版本/替换/回归」有用；删除或重命名会丢失追溯能力。
3. **素材不进 git**：`resource/` 已是 `.gitignore`；若未来公开仓库，保持这个边界，
   只提交索引/清单（如 `docs/assets/index.md`、素材清单 JSON），不提交二进制素材。
4. **发布说明写清素材策略**：随包素材 vs 用户自备素材，两种模式二选一并在 README/发布页
   明示；不要把「仓库不含素材」误写成「运行不需要素材」（缺素材会走占位回退，见
   `docs/client.md` 素材缺失审计）。

## 4. 历史记录（裁决前，保留作溯源参考）

* 2026-10-20 裁决前，`docs/assets/index.md` 只写「`SOURCES.md` 记录出处、避免版权/来源不明
  的文件混进来」，没有整体授权结论；唯一明确许可是字体 **Noto Sans SC（SIL OFL 1.1）**。
* 裁决前统计：community 521 个仓库快照中 519 个有 `SOURCES.md`；有记录的 514 个里
  412 个是「未核验（按用户指示：开发阶段不因授权阻断采集，由用户人工核验）」、
  43 个「未确认」、25 个「未核验」、20 个「未确认（无 LICENSE）」、8 个「未确认（人工核验）」、
  2 个「未确认（分发前需确认）」、仅 4 个明确许可（AGPL-3.0 / Apache-2.0 / MIT 各 1 +
  1 个「代码 MIT、美术未确认」）、5 个有文件但无状态行、2 个无 `SOURCES.md`。
* 这些数字现在只说明「当时来源登记得不完整」，**不再是授权阻塞项**；政策以 §1 为准。

## 5. 商城素材 manifest（R50-D）

商城面板（[`docs/world/gameshop.md`](../world/gameshop.md) §7.3.5）需要的商品图标，
本轮新增一份**可复核的授权/哈希清单**：

| 项 | 值 |
|---|---|
| 清单文件 | [`docs/assets/gameshop-manifest.json`](gameshop-manifest.json)（81,386 B，`sha256=fdf1e178…`，2,038 行） |
| 生成/校验器 | [`tools/assets/gameshop_manifest.py`](../../tools/assets/gameshop_manifest.py)（只读源码素材，不新造图标） |
| 条目 | **82**（`gindex` 去重 82，`ok=82`、`partial=0`；商品池是 `Server.MirDB` 商城段的 118 条中图标可解析的 82 条） |
| 政策 | `license.policy_id = resource-policy-2026-10-20`，即本文 §1 的 2026-10-20 裁决；条目级 `license` 字段逐条重复该 id |
| 覆盖 | 只覆盖 `resource/` 内素材；清单本身是索引，不含二进制 |

### 5.1 命令与退出码（实测，2026-10-25）

```powershell
py tools/assets/gameshop_manifest.py --write          # 0：wrote: … bytes=81386 sha256=fdf1e178… entries=82 ok=82 partial=0
py tools/assets/gameshop_manifest.py --check          # 0：manifest ok: 82
py tools/assets/gameshop_manifest.py --verify-icons   # 0：icons ok: 82（逐条 sha256）
py tools/assets/gameshop_manifest.py --verify-release # 0：release: sprites-item-1.mnp entries=827 icons ok: 82
```

日志与交叉核对：`target/round50/r50-d/{commands.log,cross-check.txt,icons.sha256,mirdb-report.txt}`，
说明见 `target/round50/r50-d/README.md`。

### 5.2 字段说明

顶层：

| 字段 | 含义 |
|---|---|
| `schema_version` / `kind` / `round` / `generated_by` | `1` / `gameshop-manifest` / `R50` / 生成命令 |
| `license.{policy_id,policy,source_doc,unverified_marker,notes}` | 政策 id 与出处（`unverified` 只用于 partial 条目） |
| `source.database.{path,bytes,sha256,db_version,counters,sections}` | 源 `Server.MirDB` 的路径/大小/哈希、各段计数（`gameshop: 118`） |
| `source.clean_report.{db_sha256,db_bytes}` 等 | 图标派生的中间产物（`mirdb` 解析报告）哈希，用于证明「同一样本」 |
| `icon.{root,dev_path_pattern,logical_pattern,client_lookup}` | 图标在开发树的路径规则，以及客户端取图代码位置（`entities.rs item_icon` / `ui/mod.rs load_icon`） |
| `icon.release.{file,bytes,sha256,entries,prefix,pack_sha256_actual,index_entries_parsed}` | 发布包 `sprites-item-1.mnp` 的实测指纹：**772,675 B**、`sha256=3b28fa1b997a29e7…`、**827** 条、前缀 `gfx/sprites/item/` |
| `price.{credit,gold,unit_note}` | 两种货币单价的口径（元宝 = `CreditPrice`、金币 = `GoldPrice`；总额 = 单价 × 数量） |
| `counts.{entries,ok,partial}` | `82 / 82 / 0` |
| `groups.*` | 按商品 `Category` 分组计数（`Potion 27`、`Special 12`、`Scroll 11`…） |

`entries[]` 每条：

| 字段 | 含义 |
|---|---|
| `gindex` / `item_index` / `name` / `category` / `class` / `count` | 商品与物品模板信息（`gindex` 是购买键，见 gameshop.md §7.3.3） |
| `credit` / `credit_source` / `gold` / `gold_source` / `price_unit` | 单价及其来源列（`Server.MirDB gameshop.credit_price` 等，便于逐条回溯） |
| `stock` | 限量时 > 0，`0` = 不限量 |
| `icon` / `icon_image` / `icon_pack` / `source_path` | 逻辑图标路径、图像编号、所在发布包、开发树路径 |
| `bytes` / `sha256` | 该图标文件的实测大小与哈希（**逐条**，不是整包一个） |
| `license` | `resource-policy-2026-10-20`；缺素材时为 `unverified` |
| `partial` / `partial_reason` | 缺素材时必须 `true` + 明文原因，`bytes/sha256` 置 `null` |

### 5.3 fail-closed 语义（缺素材即失败）

* 任一条目 `partial: true`（图标缺失/解析失败）→ `--check` 打印 `partial: N` 并 **exit 1**；
  只有显式 `--check --allow-partial` 才放行（用于过渡期，不用于发布）。
* `--verify-icons` 缺失条目 **exit 1**，并列出缺失清单；哈希不符同样 exit 1。
* 发布包校验 `--verify-release` 要求 `entries` 与 `manifest.packs` 索引一致（827/827）。
* 源 DB 被替换/篡改 → `--write` **拒绝生成**（exit 2，「源 DB 不是固定核验样本」）；
  伪值的 `source.database.sha256` 会被 `--check` 判为问题。
* 模拟证据：`target/round50/r50-d/partial-sim.log`（隔离副本上复现三类异常，不碰真实 `resource/`）。

> 为什么这么严：商城图标是**玩家可见的素材**，缺图时客户端会画占位方块
> （[`docs/client.md`](../client.md) §5.16.1「缺 manifest 占位规则」）；若清单把缺失当成功，
> 发布包会带着空图标上线而无人发现。因此清单的「ok」必须等于「每个图标文件都在且哈希一致」。
>
> 与 [`docs/assets/game-assets.md`](game-assets.md) §5.1.1 的口径关系：那里的发布包快照
> 已按本轮实测更正为 **827 条 / 772,675 B / `sha256=3b28fa1b…`**；本节 `icon.release`
> 与之同源，旧值 826 条 / 771,854 B 作废。**只信本节与 manifest 的实测字段**。
