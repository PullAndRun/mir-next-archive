# R70 混合特效逐帧偏移证据索引

阶段：**功能已提交+门禁通过；独立终审待办；不宣称 R70 完整/全链通过**。本批为 D 文档批、待独立终审（pre-audit）。
主单 `JJC-20261027-007`；子单 A `sub-d0d00e43`、B `sub-845fa668`、C `sub-1bdf62e4`、D `sub-67ec1a50`。
提交链：`72d9313` → `b11deac`（过程）→ `6a08c65`（功能）→ `89daaaa`（C证据）；本批不 commit/push。
依据：[计划](../../process/round70-plan.md)、[门下 C1–C6](../../process/round70-menxia-review.md)、[尚书派发](../../process/round70-shangshu-dispatch.md)。
范围：仅渲染层 Add+可选当前帧 offset 生产入口，rate 固定1；[契约](round70-blend.md)、[逐文件清单](final-manifest.json)、[外置摘要](manifest.sha256)。
功能 `6a08c65` 为4文件 +1283/−15；entities/scene 为生产改动，frame 仅既有单测区，新 R70 测试1140行/9例。

## Crystal 实读基准

- [MLibrary:692–711](../../../github/Crystal/Client/MirGraphics/MLibrary.cs#L692)：默认 offSet=false/rate=1；699偏移先于701裁剪；704–709仅保存/恢复 bool。
- [MLibrary:778–797](../../../github/Crystal/Client/MirGraphics/MLibrary.cs#L778)：主图偏移后裁剪；mask共用主图point/尺寸，不消费MaskX/Y。
- [DXManager:353–389](../../../github/Crystal/Client/MirGraphics/DXManager.cs#L353)：NORMAL=SourceAlpha/One，INVLIGHT=BlendFactor/InverseSourceColor；非统一One/One，不承诺D3D RGBA等价。
- [Effect:201–216](../../../github/Crystal/Client/MirObjects/Effect.cs#L201)：逐帧index/运动量先算，Draw/DrawBlend显式true，后者传Rate；本轮不改时序。

## A/B/C/C2 证据（报告实录，D 未重跑 Cargo）

|角色|结果与原件|
|---|---|
|A 兵部|[manifest](../../../target/round70/a/attempt-1/manifest.json)：生产入口/薄转发；R70 9、R68 2+2、clippy/fmt EXIT0；A原lib132，B补半透明后最终133。|
|B 工部|[manifest](../../../target/round70/b/attempt-1/manifest.json)：最终R70 9/0、R68 2/0+2/0、render lib133/0、clippy/fmt EXIT0；四物理隔离变异均0→101→0。|
|C 首轮|[报告](round70-c-evidence.md) + [勘误](round70-c-erratum.md)：PASS-WITH-PARTIAL；准确分类=4 条 Cargo 命令被 ring/MSVC D8050 编译阻断（01/02/03/05，未运行 tests） + 1 条 `mir-render --lib` 编译成功实际运行 133 例（129/4，四败均临时目录 PermissionDenied code5）；不得概括为“5 条/未运行 tests”；fmt EXIT0。|
|C2 补充|[报告](round70-c-addendum.md)：PASS-WITH-PARTIAL；独立执行 PARTIAL CLOSED（C-P2/C-P3），R70 9/0、R68 2/0+2/0、render lib133/0、clippy/fmt EXIT0。|

C2 使用全新 target 与 `CARGO_PROFILE_DEV_DEBUG=0` 绕行 D8050，UTC 06:45:57Z–06:49:55Z；副本四文件前后 SHA一致、st_nlink=1，未改主树。
披露：初始M2注入器误用 `Frame.width()/height()`（实为字段），E0599/EXIT101且未运行tests，**不计有效red**；原patch/log保留，另以M2-corrected获得目标断言红。

## G1–G7 实际覆盖

|G|实证与边界|
|---|---|
|G1 当帧偏移|R70 ±8跨帧、单次叠加/metadata不变；B+C2 M1有效红后恢复9 passed。|
|G2 先偏移后裁剪|R70屏外移入/部分裁剪/负坐标/i32极值、存在性返回；M2-corrected有效红。|
|G3 实际Add|非黑背景/饱和整帧oracle；M3有效红；render lib半透明alpha=0/128/255锁既有行为。|
|G4 显式关闭|false绝对坐标，true消费元数据；M4有效红。|
|G5 兼容无泄漏|交替Add/Over、旧with_offset/缓存、scene转发；R70 9/0和两套R68各2/0。|
|G6 隔离与保全|B独立物理四副本；C首轮环境阻断→C2独立复跑闭合；冻结测试SHA与四文件恢复一致，未改freeze。|
|G7 唯一全量|Lead attempt-1 EXIT0、12/12 OK；如下原字节核验，非D新跑。|

## 四有效变异矩阵（C2；actual → expected）

|变异|首败/像素|red / 恢复green|
|---|---|---|
|M1 关闭offset|L461 (28,12) `[0,0,0,255] → [248,0,0,255]`|编译成功/running9/FAILED/101；0/9 passed|
|M2-corrected 偏移前早退|L622 (2,1) `[0,0,0,255] → [248,0,248,255]`|编译成功/running9/FAILED/101；0/9 passed|
|M3 Add改Over|L506 `[248,0,0,255] → [255,0,0,255]`；另L758 `[248,0,0,255] → [255,50,60,255]`|编译成功/running9/FAILED/101；0/9 passed|
|M4 忽略false|L885 (10,10) `[0,0,0,255] → [0,252,0,255]`|编译成功/running9/FAILED/101；0/9 passed|

## 唯一门禁原件

Lead于HEAD=`6a08c652df794a9b19a8324a180d589bcce33d24`执行 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`，EXIT0、items_ok=12/items_fail=0。
UTC `2026-10-27T06:54:11Z–06:58:15Z`；[log](../../../target/round70/gate/attempt-1.log) 211886B，SHA256 `6acfc87fb6602dd510e94c72a212455f3a571c29d18957dcc103c85c0f390e28`。
[meta](../../../target/round70/gate/attempt-1.meta.txt) 465B，SHA256 `4cbb0ee8b2db1edd7b8b27c268279704cd55633c4d0e2396c3c0dbd0e66efe86`；D已实测原字节匹配；ci.ps1本轮未改，门禁后crates差异为空。

## PARTIAL 台账（沿原编号承接）

|原项|本轮结账边界/保留|
|---|---|
|GAP-1 / R70-C2|仅rate=1 Add与可选offset子切片；完整DrawBlend/rate/INVLIGHT/公共单API全面收敛仍PARTIAL。|
|GAP-2|显式参数/无混合泄漏已测；完整状态恢复/场景状态未实现。|
|GAP-3 / GAP-4|mask/tint第二层与shadow绘制仍PARTIAL；shadow保持解析不消费。|
|GAP-5 / R70-C1|spellfx/UI调用点迁移未做；FireWall图库/帧/坐标准入及magic映射差异（ThunderBolt/Bolt/FireWall）另案，仍PARTIAL。|
|GAP-6|GameScene fileIndex/index条件offset/照明仍PARTIAL。|
|R70-C3|尺寸/高度/居中基准仍PARTIAL；不改mapping/time/wire/server。|
|R69条件5|a77原红/隔离/日志不可恢复，历史保全永久PARTIAL；本轮不修历史。|
|R68|125/570回归已测；旧manifest措辞/共享isolated日志归档限制、全UI/真实素材设备视觉签核仍PARTIAL。|
|R49–R67|全部历史遗留按原编号承接；真实素材视觉/跨后端像素/GPU设备与DrawBlend混合动效全矩阵未签核。|

## D 文档批自检

- `git status --porcelain --untracked-files=all`：仅本批六文件（client/roadmap M，四testing ??）+两份既有历史freeze ??；无越界最终差异，freeze未写入。
- `git diff --check` EXIT0；仅Git提示client/roadmap未来检出CRLF，当前文档UTF-8无BOM/LF；`git diff 6a08c65 -- crates tools/dev/ci.ps1`为空。
- `py -3 tools/docs/check_links.py` EXIT0：checked=1755，skipped_target=626；2篇README引用与市场包编号一致。target证据链接被全局工具跳过，D已另核本页引用原件存在与SHA。
- Python JSON parse EXIT0、14/14 项 bytes/SHA 复算一致（13 交付 + 1 勘误）、外置 manifest SHA 匹配；T 终审 BLOCK→勘误→T2 复审后由 Lead post-audit 固化清单（15 artifacts=14+T2 复审报告），清单不自引用。
