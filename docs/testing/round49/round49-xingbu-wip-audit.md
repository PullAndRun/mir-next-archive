# 第49轮 WIP 只读审计（刑部／御史台）

**结论：BLOCK。** 当前 WIP 尚不可整体提交。负方向步长仍被额外禁止、最终证据的代码／二进制／批次尚未闭环，且两份完整玩家资源冻结 JSON 违反单文件 1 MB 入库上限。源码门禁通过与证据可追溯是两个分别需要满足的条件。

任务：`JJC-20261025-001`；审计角色 `xingbu`；日期：2026-10-25，Asia/Shanghai；基线 HEAD：`f0be364`（R48）。本报告仅审未提交怪物 WIP，不将商城下一阶段纳入完成判定。审计员未修改生产代码，未执行 commit、reset、checkout 或全量 Cargo／CI。

审计采用已存在日志、逐行源文对照、Git 只读检查、两个 Python 标准库证据核验脚本。脚本和详细数据分别为 [基础核验脚本](audit_probe.py)、[基础核验结果](audit-probe-results.json)、[批次与图片核验脚本](audit_detail_probe.py)、[批次与图片核验结果](audit-detail-results.json)。这些是审计产物，不是生产测试辅助。

## 审计快照与验证范围

开始时为 **45 个 tracked M + 10 个 untracked 条目**。审计期间队长并行修复了 CLI 首帧帧表和证据用例缺素材空过，并新增共用素材加载函数及 CLI 测试；复查时为 **46 个 M + 13 个 untracked 条目**（目录条目不等于文件数）。因此不能把开始时的全量门禁成绩直接套到最后的生产代码。

实际只读命令：

```text
git status --porcelain
git diff --stat
git diff --check
git diff -- crates/mir-asset/src/lib.rs crates/mir-asset/src/lib_container.rs crates/mir-asset/src/library.rs
# 另逐组读取 mir-client / mir-render / mir-tools diff、新测试及 Crystal 原文
py -3 docs/testing/round49/audit_probe.py
py -3 docs/testing/round49/audit_detail_probe.py
```

两次 `git diff --check` 均 exit 0；只有 LF→CRLF 提示。两个核验脚本均 exit 0。初次扫描 124 个 WIP／证据文件，147,902,293 字节；PNG 签名、块 CRC、解压数据及 UTF-8 文本／JSON检查无错误。扫描后的生产代码变化另行复读；哈希快照的时间及变化清单在 [详细结果](audit-detail-results.json#L1)。**未声称审计员重跑过 Cargo 测试。**

| 已有日志 | 可据此确认的事实 | 实用限制 |
|---|---|---|
| [全量门禁日志:3122](../../../target/gate-r49-final2.log#L3122) | 3122–3134 列出 12 项 OK，包括 fmt、clippy、workspace tests、协议、文档、i18n、资源与数据库检查 | 约 02:12 完成，早于 02:16 证据重生成及本次并行修复；部分中文是旧编码，12 项 ASCII OK 可辨 |
| [证据日志:1](../../../target/r49-evidence-tests.log#L1) | real_framesets：1 passed、3 ignored | 不是 118 库每条动作均获得渲染验收 |
| [证据日志:10](../../../target/r49-evidence-tests.log#L10) | camera::monster_motion_tests：6 passed | 无真实网络完整包队列及窗口边界覆盖 |
| [证据日志:20](../../../target/r49-evidence-tests.log#L20) | monster_native ignored：3 passed | 执行时源码仍有可缺素材空过分支；本机确有素材，产物可交叉确认 |
| [证据日志:30](../../../target/r49-evidence-tests.log#L30) | monster_native_evidence ignored：1 passed，33.99s | 只重写 raw manifest／PNG，不自动重建 freeze／汇总 manifest |
| [证据日志:40](../../../target/r49-evidence-tests.log#L40) | monster_native_import ignored：4 passed | 导入正确不等于渲染支持所有动作、Blend 或 Effect |

最后三个测试的 PNG／数据实际存在，不因代码曾有空过分支而倒推本次执行必然空过；但修复空过分支后仍需新的失败能失败的验证记录。

## A．X1 四项与 Crystal 语义

### A1．负 Skip、零步长：部分修复，仍有 high

**stride=0 已修复。** [布局检查:73](../../../crates/mir-asset/src/frames.rs#L73) 接受 `Count>1 && Count+Skip==0`，也接受 `Count=1,Skip=-1`；[有符号怪物索引:938](../../../crates/mir-render/src/entities.rs#L938) 使用 i64 checked 运算，避免把负 Skip 提前转为无符号。真实 010／041／043／062 的八方向共享 Standing 已有 [测试:95](../../../crates/mir-render/tests/monster_native.rs#L95) 与 [stride-zero 证据](../monster-motion/stride-zero-render.json)。

**“所有负方向步长非法”的限制仍无 Crystal 依据。** [布局检查:77](../../../crates/mir-asset/src/frames.rs#L77) 明确拒绝 `Count>1 && Count+Skip<0`，又在 86–87 行拒绝 `Count=1 && Skip<-1`；[frame_indices:102](../../../crates/mir-asset/src/frames.rs#L102) 同样拒绝前者。[Crystal Frames:231](../../../github/Crystal/Client/MirObjects/Frames.cs#L231) 直接读取有符号 i32，`OffSet = Count + Skip`；[MonsterObject:357](../../../github/Crystal/Client/MirObjects/MonsterObject.cs#L357) 直接计算 `Start + OffSet*Direction + FrameIndex`，没有“方向步长必须非负”的要求。

可判定反例：`Start=20,Count=6,Skip=-8,Interval=100`。方向 0 的帧是 20..25，方向 7 是 6..11，八方向全部非负且在一个 26 帧库内；Crystal 能按该公式取帧，Rust 当前整条记录拒绝，且 [解析循环:301](../../../crates/mir-asset/src/frames.rs#L301) 会让整库帧表读取失败。这不是要求接受越界表：应按八方向实际首尾及库长判边界，保留 checked 算术；不能用步长符号替代边界验证。

[中书方案:75](../../../docs/process/round49-plan.md#L75) 明文要求保留负 stride／stride=0 既有语义。裁决：X1-01 **未完全关闭**；增加合法负 stride、负 Skip 零 stride、真实越界三种互相区分的断言后才能关闭。

### A2．动作生命期 Count×Interval：核心通过

[动作时长函数:315](../../../crates/mir-render/src/animation.rs#L315) 按 raw action 0／1／2／9／18／21 查询实际表并 checked multiply Count×Interval；有整表但缺动作返回 None，不凭空造默认时长；Dead 不自动结束。[finished_action:2277](../../../crates/mir-client/src/net.rs#L2277) 对走／跑优先使用建立 motion 时冻结的时长，其余怪物动作查完整表；2303–2309 行将 Struck／攻击／走／跑变 Standing、Dying 变 Dead。[动画测试:265](../../../crates/mir-client/tests/anim.rs#L265) 的怪物攻击边界从错误 300ms 修到 600ms。

Crystal 依据：[默认怪物表:22](../../../github/Crystal/Client/MirObjects/Frames.cs#L22)、[设置 FrameInterval:524](../../../github/Crystal/Client/MirObjects/MonsterObject.cs#L524) 和 ProcessFrames／UpdateFrame 的 Count 边界。默认 Standing=4×500、Walking=6×100、Attack1=6×100、Struck=2×200、Die=10×100；不把服务器受击／命中延迟当客户端动画长度。

限制：默认表原文没有 Running，Rust 无整表时把 Running 也设 600ms，是既有旧扁平移动兼容项，不能描述成原文 DefaultMonster 的 Running 项。真实 184 的 Walking=10×100ms 用于证明不恒为 600ms；真实素材包没有覆盖每种 Running／特殊动作。X1-02 可关闭核心错误，特殊动作仍按本轮 PARTIAL 边界处理。

### A3．参数进入 session／首帧：初始 high 已修，形态一致性仍需条件

初始 [CLI 连接入口:541](../../../crates/mir-client/src/main.rs#L541) 曾恒传空 `monster_frames`。审计已立即通知队长；最新代码改为连接前调用 [共用加载函数:193](../../../crates/mir-client/src/assets.rs#L193)，并随 ConnectOptions 传递。UI [登录入口:1655](../../../crates/mir-client/src/app.rs#L1655) 也在 connect 前加载；[session 首帧:3629](../../../crates/mir-client/src/net.rs#L3629) 同时复制完整表并派生每怪 Walking／Running 参数。[backfill:564](../../../crates/mir-client/src/app.rs#L564) 用 `entry.or_insert` 补缺，不覆盖新快照已有参数。

[首帧会话测试:109](../../../crates/mir-client/tests/monster_motion.rs#L109) 经过真实 NetClient 网络线程和回环 socket，证明第一条快照同时携带 count=6 与 count=10；[新增 CLI 用例:12](../../../crates/mir-client/tests/cli_monster_frames.rs#L12) 则验证真实素材加载与 ConnectOptions 保留。后者不是 CLI 二进制端到端测试，不应扩大其证明范围。

**剩余 med：** [主入口形态选择:379](../../../crates/mir-client/src/main.rs#L379) 支持 CLI／配置／环境变量；共用加载函数却在 195 行重新调用 [discover_for_data_dir:140](../../../crates/mir-client/src/assets.rs#L140)，只看环境变量并默认 Auto。显式 `--asset-mode release` 且 dev 文件仍在时，session 可能读 dev 表而 scene 选 release 包，生命期与图像来自不同版本。应复用同一份已经解析的 ResolvedAssets／EntityLibraries，或证明显式 dev／pack-only release 的表哈希与 session 参数一致。初始“CLI 恒空表” high 关闭，最终回归结果仍待新日志。

### A4．OffSetMove 符号、八方向、余数、DrawY：通过核心，Sorting 不应宣称全等价

[Crystal:384](../../../github/Crystal/Client/MirObjects/MonsterObject.cs#L384) 先从终点反推 Movement；395–423 行给八方向和负数 `%2`；433–437 行使用 `DrawY=max(Movement.Y,CurrentLocation.Y)`，并对 DrawLocation 加 **`-OffSetMove`**。

[MonsterMotion:298](../../../crates/mir-client/src/camera.rs#L298) 返回 offset 的反号；307–326 行八方向与有符号 `%` 一致，不用 rem_euclid。[实体视图:3211](../../../crates/mir-client/src/net.rs#L3211) 使用 `motion.from + advance`，3218–3221 行给 max(from.y,to.y)；[渲染排序:1383](../../../crates/mir-render/src/entities.rs#L1383) 消费该 sort_y。Right 首帧 `(8,0)`，500ms `(48,0)`，落点转换不会反向跳 96px；见 [相机测试:1181](../../../crates/mir-client/src/camera.rs#L1181) 与 [运动 manifest](../monster-motion/manifest.json)。X1-04 的符号错误可关闭。

Sorting 仍是近似：Crystal [队列取动作:554](../../../github/Crystal/Client/MirObjects/MonsterObject.cs#L554) 将 MapLocation.X 设为**目标格** action.Location.X；Rust 同 Y 用起点 position.x。Crystal [同格排序:113](../../../github/Crystal/Client/MirObjects/MapCode.cs#L113) 还按 Item／Spell、Dead、ObjectID 排序，Rust 只稳定排 Y／X。同格活怪／尸体、交错移动遮挡未由当前截图证明；不得写“Crystal 排序全部移植”。

### A5．其余 Crystal 行为与本轮边界

- **缺动作：** [原生接管:1467](../../../crates/mir-render/src/entities.rs#L1467) 明确区分无表与有表缺动作；后者 `Some(None)` 只报警一次并安静跳过，不借 Standing／别库。与 [Crystal:511](../../../github/Crystal/Client/MirObjects/MonsterObject.cs#L511) 的 TryGetValue／null 返回基线一致。原文对 AttackRange／Special 等有少数明确动作回退，但本轮没有声称这些动作已移植。
- **Reverse／Blend：** 解析／打包保留字段，但当前 [原生绘制:1496](../../../crates/mir-render/src/entities.rs#L1496) 只正向 local_at、最终普通 `frame.blit`，没有消费 Reverse／Blend。[详细核验](audit-detail-results.json#L679) 在 118 真实库中数到 **107 条 Revive(action28) reverse、4 条 raw25 reverse、monster-117 的 7 条 Blend 动作（包括 Standing、Walking）**。不能把“保留元数据”表述为“渲染实现”。117 的普通动作已受影响，应实现 Crystal Blend，或在支持矩阵明确列 PARTIAL 并选有说明的降级；Revive 等未接动作也须明列未支持。
- **Effect：** EffectStart／Count／Skip／Interval 已保留，当前怪物绘制未消费；[Crystal:358](../../../github/Crystal/Client/MirObjects/MonsterObject.cs#L358) 单独维护 DrawWingFrame。本地 118 表的 EffectCount 均为 0，故现有真实样本不能验证 Effect 路径，不能写已覆盖。
- Push／Jump／Dash、坐骑、`!Mon.dat` 精确映射、怪物动作队列追帧／SkipFrames、窗口边界与 FPS 测量不在本轮完成范围，须保持 [运动 README:43](../monster-motion/README.md#L43) 和 [体验待办 P1/P2](../../client-experience-backlog.md) 的 PARTIAL 声明。

## B．假绿、自创与证据一致性

**未发现 PNG 人工拼造或把跨动作借帧混入原生生产路径。** 测试内固定相机和对象坐标、合成帧表、模拟包用于可复现渲染，均位于 tests／审计目录；它们不能充当“玩家正常登录、自然怪物 AI 行动”的证据。生产时长来自表，不再固定所有怪物 300／600ms。48×32 格子尺寸及 1／2 格步长有 Crystal 原文依据。

无表回旧扁平路径是方案允许的兼容策略，不等于原生资源导入成功。[旧扁平路径:1420](../../../crates/mir-render/src/entities.rs#L1420) 仍会尝试 `[exact,standing,base,0]`；这是已有兼容行为，报告必须说明它会借旧 Standing／首帧，不能把它写成“所有路径都不借帧”。有原生表但缺动作则跳过，边界清楚。

### B1．缺素材空过：初始 high 已修，最终执行证据未更新

初始 ignored 真实证据中有 `if !素材存在 { eprintln!(SKIP); return; }`，可在缺素材时给 ok。已通知队长。最新 [stride-zero 用例:100](../../../crates/mir-render/tests/monster_native.rs#L100)、真实原生证据用例和 [新增 CLI 用例:18](../../../crates/mir-client/tests/cli_monster_frames.rs#L18) 改为 assert；运动用例也删除空过。方向正确，提交前应记录“正常素材通过；缺前置素材明确失败”的结果，避免只有更新前日志。

此外 [R46 回环移动测试:331](../../../crates/mir-client/tests/remote_motion_live.rs#L331) 新增 `#[ignore]`。这是门禁范围实质缩小；已有 5 组定向日志不含该 ignored 测试。可以把真实资源用例显式化，但必须另行运行并记录该条，或如实列未验证，不能沿用“R46 全量不回归”的结论。

### B2．证据链仍阻塞：像素真实，但汇总元数据属于旧代码／批次

[详细结果:1882](audit-detail-results.json#L1882) 保存了两份 gate／evidence 日志的 SHA256 和逐行摘要。独立核验 **52 张 monster-native PNG 的文件 SHA256 与非透明 crop SHA256 均匹配汇总 manifest**；25 张 attack-variants 的 PNG 字节 FNV、解码 RGBA 像素 FNV也全匹配新 manifest。没有哈希不匹配就指控截图伪造的依据。

但是 [raw-manifest](../monster-native/raw-manifest.json) 已重生成至 `generated_at_unix_ms=1792865639318`，而 [汇总 manifest](../monster-native/manifest.json)／[freeze](../monster-native/freeze.json) 仍是 `batch=1792847715360`。汇总 manifest 的二进制 SHA256 为 `00b3df83ea96a9c954e045a2e75d87338b9b446b2a78eaec9d766ef76ea30544`，磁盘当次测试 EXE 实测为 `da0ab79ad6660a062db168f5bd68e7fde30b4e10dbadd46487e8fd28f1b2fecc`。二进制可在后续构建变化，所以要求保存的是产出时的二进制身份，不是保证永远等于当前 target；当前证据没有解释这次变化。

更直接的是汇总 source_blobs 与 WIP 不一致：

| 源码 | 汇总记录 SHA256 | 当次审计实测 SHA256 |
|---|---|---|
| [entities.rs](../../../crates/mir-render/src/entities.rs) | `0153679045a9c09d227e50dd362eef7c2fcb528ab6a75b11b6b40ed9b57281f9` | `8aad2e0745743812aafe2b345f92e38b41e99e0b67b5be9400e46a472043d432` |
| [animation.rs](../../../crates/mir-render/src/animation.rs) | `ba905a9c484d435c12b8d787f0ad2ce2a89bc77db33bd7bf85d5c03970110bc7` | `b08dfecad239bc397fcb9533f303918c973f5bfb6cbe01615c679a05c5064ed6` |
| [camera.rs](../../../crates/mir-client/src/camera.rs) | `3ae7a5a2eea6147a4ddef36b33611ddfa700585aedbd42ecd2eedb53ae1b538a` | `c57dc92d627c45fe50d545b16872da43e3b5ea761a52d0756ba1996d8f6e9272` |

[详细结果:1830](audit-detail-results.json#L1830) 还记录：汇总列的 native_packs 只有 **12** 项，不能证明后来 118 库全部 release 渲染；freeze 仍声称 attack-variants 对 HEAD diff 为 0，与当前 27 个 M 矛盾。部分 raw 顶层 png 路径也缺少真正的 `png/` 子目录。必须从最后一版代码产物重建一份统一 batch 的 raw→summary→freeze，记录测试 EXE、源码／输入资源哈希、确切命令、实际覆盖数、未执行项，禁止手写“全绿”代替因果链。

### B3．“原生／全量”措辞与 1 MB 约定

应统一写为：“已导入本地可得的 **118 个 v3 `.Lib`**，原始帧表元数据保留；本轮真实动作／渲染验证以列出的样本为准。另 4 个 v2 无帧表、约 800 个 appearance 缺源；无原生表回旧扁平库，Blend／Reverse／Effect／特殊动作按支持矩阵标明。”118 不是全部 Crystal appearance，也不是 118 库每个动作全量渲染通过。

[资源卫生:15](../../process/repo-hygiene.md#L15) 明确单文件上限 **1 MB**。当前必须处理：

| 文件 | 字节 | 决定 |
|---|---:|---|
| [完整 before 冻结](../monster-native/player-resource-freeze-before.json) | 44,844,165 | 排除提交；移到已经忽略的 target/r49 或本地资源索引归档 |
| [嵌套完整 after 冻结](../monster-native/player-resource-freeze.json) | 45,219,533 | 用小摘要替换；完整 372,448 张 PNG 的逐文件哈希移到 target/r49，不入库 |
| [monster-native 目录](../monster-native/) | 126,287,870（约 120.4 MiB） | 先去掉上述 90,063,698 字节；其余 52 张整场景图也应择关键样本／保留小 crop，避免 36 MB 的重复图整体入库 |

小摘要保留 262 个玩家 key、372,448 张 PNG 的数量、排序后整体清单 SHA256、before／after 相等布尔与分组哈希、必需 key 缺失清单、资源／pack manifest 哈希、生成命令与输出位置。不可只保存“true”却丢掉计算对象。gzip／zip 只有实测压到单文件 1 MB 内且确有长期用途才有意义；不建议把 44 MB 文本换名成大压缩包绕过约定。

[README](../monster-native/README.md) 必须说明完整快照是本地产物、哪些小摘要入库及如何复现，不再把旧 EXE／旧 source hash 当本次代码证据。库覆盖说明同步到 [导入 README](../native-monster/README.md)、[客户端 R49 段落](../../client.md)。

## C．attack-variants 的裁决

**裁决：图片属于合理、可验证的回归重生成；在当前冻结声明下直接覆盖旧 R47 批次还不能原样提交。** 无需把它判成造假，也不能默认为本轮已授权的冻结证据更新。

实测 old batch=`1792807006205` → new batch=`1792862532745`，packs **46→96**，25 张像素 hash 全变；PNG 字节／解码像素与新 manifest 全部一致。25 条样本的动作、elapsed、variant、logical／actual index、layers、camera 等关键字段无变化；原测试经过两个回环客户端接包后调用当前 MapScene 渲染，当前共用渲染路径已含 R48 原生玩家及 R49 怪物，包数量与场景像素随资源扩展变化是合理副产物。**没有隔离背景／原生玩家路径的差分实验，不能断言所有像素差异仅由 R49 怪物导致。**

存在两项提交前条件：

1. [attack capture:271](../../../crates/mir-client/tests/attack_variant_live.rs#L271) 仍用 `player_layer_plan_at` 计算扁平计划并把 216／344 等写作“实际渲染 index”；而 [玩家绘制:1532](../../../crates/mir-render/src/entities.rs#L1532) 会优先 draw_native_player。原生 Attack1／Attack3 Start 是 136／232，Right 基础帧 148／248，另有实际外观列偏移。需记录真正被选中的 source／library key／index，或把旧字段如实命名为 flat_plan_index，分别验证原生／扁平路径；旧字段不能继续证明新 PNG 确实使用该索引。
2. [中书方案:57](../../../docs/process/round49-plan.md#L57) 冻结攻击变招且 58 行不改 R47／R48 冻结证据；[攻击 README](../attack-variants/README.md)、[CHANGELOG](../../../CHANGELOG.md) 与新批次没有同步说明，[monster-motion README:47](../monster-motion/README.md#L47) 仍称未碰攻击目录。可由队长作一项明确、可追溯的范围裁决：**推荐保存旧批次，新批次移到本轮目录并写 supersedes／regression_of；如决定更新原目录，则必须同时改 README／CHANGELOG、freeze 与范围记录，说明批次、packs 数及渲染来源改变，保留旧批次追溯入口。**

本次审计不恢复／搬移这 27 个 tracked 修改；只有队长可执行最终选定的入库处置。语义本身不要求新增攻击动作。

## D．精确提交建议与仓库卫生

**以下是解除阻塞后的候选清单，不是当前 `git add -A` 授权。** 完整逐文件 SHA256／大小清单见 [inventory](audit-detail-results.json#L4)；为免漏掉后来新增 CLI 测试，再单列如下。

### D1．源码／测试／文档应一并提交

```text
# modified 生产源码（含审计期间修复）
crates/mir-asset/src/lib.rs
crates/mir-asset/src/lib_container.rs
crates/mir-asset/src/library.rs
crates/mir-client/src/app.rs
crates/mir-client/src/assets.rs
crates/mir-client/src/camera.rs
crates/mir-client/src/main.rs
crates/mir-client/src/net.rs
crates/mir-render/src/animation.rs
crates/mir-render/src/entities.rs
crates/mir-tools/src/main.rs

# modified 兼容／回归测试；ignore 范围变更另须补验证
crates/mir-client/tests/anim.rs
crates/mir-client/tests/attack_variant_live.rs
crates/mir-client/tests/live_world.rs
crates/mir-client/tests/remote_motion_live.rs
crates/mir-client/tests/ui_interaction.rs
crates/mir-client/tests/ui_test.rs
crates/mir-render/tests/native_player_evidence.rs

# new 解析器与六个新测试文件（原任务五个 + CLI 补漏）
crates/mir-asset/src/frames.rs
crates/mir-asset/tests/real_framesets.rs
crates/mir-client/tests/cli_monster_frames.rs
crates/mir-client/tests/monster_motion.rs
crates/mir-render/tests/monster_native.rs
crates/mir-render/tests/monster_native_evidence.rs
crates/mir-tools/tests/monster_native_import.rs

# 当前 M / ?? 文档
# 不包含尚未完成的商城功能文档
docs/client.md
docs/process/round49-plan.md
docs/process/round49-menxia-review.md
docs/testing/round49/round49-xingbu-wip-audit.md
```

门下文档只作为进程记录收录；本审计不以等待／借用门下结论替代 WIP 独立判断。生产变更没有 Cargo.toml／Cargo.lock／数据库迁移修改，不需要额外把无关依赖或生成数据塞入本次提交。

### D2．小证据候选，重建元数据后显式加入

```text
docs/testing/monster-motion/README.md
docs/testing/monster-motion/manifest.json
docs/testing/monster-motion/frame0.png
docs/testing/monster-motion/frame5.png
docs/testing/monster-motion/frame6_005_600ms.png
docs/testing/monster-motion/frame10_184_1000ms.png
docs/testing/monster-motion/stride-zero-render.json
docs/testing/monster-motion/stride-zero/010-standing-down.png
docs/testing/monster-motion/stride-zero/041-standing-down.png
docs/testing/monster-motion/stride-zero/043-standing-down.png
docs/testing/monster-motion/stride-zero/062-standing-down.png
docs/testing/native-monster/README.md
docs/testing/monster-native/README.md
docs/testing/monster-native/monster-frame-table.csv
docs/testing/monster-native/raw-manifest.json
docs/testing/monster-native/manifest.json
docs/testing/monster-native/freeze.json
docs/testing/monster-native/player-resource-freeze.json   # 仅替换后的小摘要
```

52 张整场景图不建议一次性提交；可保留满足六动作最低覆盖的以下 6 个实际样本，再加两张混合场景（均实测 <1MB）；其余列于本地完整产物索引，摘要中的 PNG 数／路径按实际入库样本更新。若需男女五职业与替代列对照，应继续使用已有 R48 证据并补明确回归结果，不能单靠删图后说已验证不变。

```text
docs/testing/monster-native/png/monster-005-standing.png
docs/testing/monster-native/png/monster-005-walking.png
docs/testing/monster-native/png/monster-022-attack1.png
docs/testing/monster-native/png/monster-023-struck.png
docs/testing/monster-native/png/monster-039-dying.png
docs/testing/monster-native/png/monster-039-dead.png
docs/testing/monster-native/png/self-still-dev.png
docs/testing/monster-native/png/self-moved-dev.png
```

上述是精确的最小候选，不妨碍用独立小 crop 代替整场景；替代时必须保存 crop 的来源 bbox／完整图 SHA256、命令及完整图在本地的位置。dev／release 像素相等需留断言结果，不需为每张相同图复制两份 PNG。[详细结果:2270](audit-detail-results.json#L2270) 列出了重复内容的文件组。

### D3．当前明确排除与条件项

```text
# 必须排除大冻结，完整内容移入已忽略的 target/r49
# 对当前 after 文件是替换内容，不把45MB原件加入索引
docs/testing/monster-native/player-resource-freeze-before.json
# 其余44张 monster-native/png：除D2列出的8张外均先留本地
# target/**、game-assets/**、resource/**、github/**、日志／EXE／.mnp／临时目录不入库
```

当前攻击证据 27 个 M 全部列为**条件项**，包括一个 manifest、一个 log 与以下完整 PNG 集合；完成 C 的范围／元数据修正后才更新原目录，或将新批次另存本轮并由队长保全旧批次。

```text
docs/testing/attack-variants/live-manifest.json
docs/testing/attack-variants/live-run.log
docs/testing/attack-variants/female-attack1-down-frame0.png
docs/testing/attack-variants/female-attack1-down-frame1.png
docs/testing/attack-variants/female-attack1-down-frame2.png
docs/testing/attack-variants/female-attack1-down-frame3.png
docs/testing/attack-variants/female-attack1-down-frame4.png
docs/testing/attack-variants/female-attack1-down-frame5.png
docs/testing/attack-variants/female-attack1-down-frame6.png
docs/testing/attack-variants/female-attack1-right-frame0.png
docs/testing/attack-variants/female-attack1-right-frame1.png
docs/testing/attack-variants/female-attack3-right-frame0.png
docs/testing/attack-variants/female-attack3-right-frame1.png
docs/testing/attack-variants/female-attack3-right-frame2.png
docs/testing/attack-variants/female-attack3-right-frame3.png
docs/testing/attack-variants/female-attack3-right-frame4.png
docs/testing/attack-variants/female-attack3-right-frame5.png
docs/testing/attack-variants/female-attack3-right-frame6.png
docs/testing/attack-variants/female-attack3-right-frame7.png
docs/testing/attack-variants/female-attack3-right-frame8.png
docs/testing/attack-variants/male-attack1-right-frame0.png
docs/testing/attack-variants/male-attack1-right-frame1.png
docs/testing/attack-variants/male-attack1-right-frame2.png
docs/testing/attack-variants/male-attack1-right-frame3.png
docs/testing/attack-variants/male-attack1-right-frame4.png
docs/testing/attack-variants/male-attack1-right-frame5.png
docs/testing/attack-variants/male-attack1-right-frame6.png
```

[现有忽略规则:4](../../../.gitignore#L4) 已覆盖 target／嵌套 target、pycache、素材根和根日志；`git status` 没有 target 日志、临时 EXE 或原始素材混入。**当前无需为 target 再补规则。** 建议把完整冻结生成位置固定到 target；若继续保留 docs 下的本地完整副本，补精确规则而非忽略整个证据目录：

```gitignore
/docs/testing/monster-native/player-resource-freeze-before.json
/docs/testing/monster-native/player-resource-freeze-full*.json
```

本轮审计脚本、两个小 JSON 与 HTML 可择有复现价值者入库，均应保持 <1MB；无长期复现用途的中间快照不加入。切勿把修正后的轻量 summary 用上述通配规则误忽略。

## E．运行风险（5 条）

| 风险 | 严重度／证据 | 处理／验证边界 |
|---|---|---|
| 合法负 stride 被拒；Blend／Reverse 只保存不执行；Effect 无真实样本 | high／med：[frames.rs:77](../../../crates/mir-asset/src/frames.rs#L77)、[entities.rs:1496](../../../crates/mir-render/src/entities.rs#L1496) | 正确校验八方向实际范围；117 Blend 与未接特殊动作分开列支持矩阵，不能把元数据成功当渲染成功 |
| session／scene 素材形态不同，或原生缺源降级只在终端提示 | med：[assets.rs:195](../../../crates/mir-client/src/assets.rs#L195)、[原生加载:1135](../../../crates/mir-render/src/entities.rs#L1135)、[绘制 fallback:1419](../../../crates/mir-render/src/entities.rs#L1419) | 传同一 resolved source；缺表／坏表会被缓存成 None 后用旧库，玩家可能只看见旧图；全缺时方块可见，但“坏表与无表”缺明确玩家可见区分 |
| 生命周期与快照重复／动作队列差异 | med：[net.rs:3398](../../../crates/mir-client/src/net.rs#L3398)、[运动参数:3595](../../../crates/mir-client/src/net.rs#L3595) | 本轮清理覆盖动作结束、受击、死亡、删除、切图、断线；未见共享内存数据竞争。网络线程拥有快照、UI 接收克隆；丢包／追帧／重复位移的 Crystal 队列语义及 R46 ignored 回归仍需对应证据 |
| 首次读取与逐帧分配／大快照克隆 | med：[怪物表缓存:1475](../../../crates/mir-render/src/entities.rs#L1475)、[排序分配:1383](../../../crates/mir-render/src/entities.rs#L1383)、[UI 帧表克隆:2655](../../../crates/mir-client/src/app.rs#L2655) | renderer 的表和 PNG 会缓存，未见热循环每帧反复读同一表；但每怪每帧 format key、排序 Vec、快照／backfill 重复克隆全部 FrameSet 有分配成本。未测 FPS，不可声称无性能影响 |
| 排序只覆盖 DrawY 主锚，未复刻同格规则 | med：[net.rs:3218](../../../crates/mir-client/src/net.rs#L3218)、[entities.rs:1384](../../../crates/mir-render/src/entities.rs#L1384)、[Crystal MapCode:115](../../../github/Crystal/Client/MirObjects/MapCode.cs#L115) | 移动起点 X 与 Crystal 目标 X、死活／Item／Spell／ObjectID 次序不同；先明确 PARTIAL，补同格尸体与移动交错遮挡案例再宣称一致 |

## 必须修与解除阻塞条件

1. **high，X1-01：** 去掉“负方向步长一概非法”的额外规则，按实际八方向边界校验；补上述合法负 stride 与越界失败的区分测试。证据：A1。
2. **high，证据闭环：** 最后一版代码完成后，生成统一 batch 的 raw／summary／freeze，绑定输入、源码与产出二进制；修正 R47 的实际索引陈述及“零 diff”断言。需要的新定向验证包括首帧 CLI 与缺前置素材失败；将 R46 从自动 gate 移到 ignored 后须显式补跑记录。已有 02:12 门禁只记历史成绩，不冒充最终 WIP 门禁。证据：A3、B1/B2、C。
3. **high，仓库体积：** 两份 44–45 MB 全量冻结不得入库；替换轻量摘要／命令，完整产物移至已忽略位置；按 D2 选择关键图而非 121 MB 整目录加入。证据：B3。
4. **med，输入来源一致性：** CLI／UI session 复用 scene 同一个 resolved asset source，补显式 dev／release 的一致性验证。证据：A3。
5. **med，支持范围与冻结文档：** 对 117 Blend、Reverse／Effect、Sorting、约 800 缺源及 v2 降级如实列 PARTIAL；选定 C 的旧批次保全方案并在 README／CHANGELOG／freeze 同步说明。未实现行为不能以“原生／全量”措辞掩盖。证据：A5、B3、C。

初始发现的“CLI 恒空表”和“ignored 缺素材成功返回”已被队长修到源代码，故不重复列为仍未修的 high；它们的最终验证归第 2 项。只读审计到此不替队长执行入库清理、不改变主任务状态。所有 high 关闭、med 条件落实或按明确 PARTIAL 边界裁决、最后源码门禁及证据因果链可复查后，方可改判提交就绪。

看板已记录 `xingbu` 结论 memo，exit 0；主任务仍为 Doing。当前 CLI 实际语法多一个角色位置参数：`memo JJC-20261025-001 xingbu "御史台 WIP 只读审计：BLOCK；…" --as xingbu`。首次照任务消息省略角色位置参数时返回用法错误；已读取 CLI 前置校验并用只读 show 核对，无状态变更，再按实际语法成功写备注。
