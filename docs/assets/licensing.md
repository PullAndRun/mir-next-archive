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
