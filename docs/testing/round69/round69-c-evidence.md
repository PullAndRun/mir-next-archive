# R69-C 刑部独立核验

- 子单 `JJC-20261027-006-sub-fcc589b0`；核验官 gpt-6.1-sol；基线 `9e8d9e828742f866f8b3a42f4bf352c21ce9667c`。
- verdict：**BLOCK**；必改1：B须恢复被覆盖的首轮原始证据（可用备份时），并证明历史保全；无备份则保留不可补证的限制交Lead裁决，不能宣称条件5全关。最终契约正确性通过，生产一致性保持 PARTIAL。
- C 只写本页与 `target/round69/c/**`；未修改 A/B、crates、矩阵、两个 freeze JSON；无 commit/push、cargo、ci.ps1。

|必做核验|状态|实读/执行证据|
|---|---|---|
|冻结与禁止自证|PASS（历史时间精度有限）|人工 freeze 03:35:12Z；A 首跑自报位于03:36:51Z–03:41:14Z，先冻结后首跑；首轮 G2/G3 误红只修工具、矩阵 e513…未变。历史首轮 audit 被 A 后续审计覆盖，不能给独立精确首跑秒。|
|独立复跑|PASS|最终 B SHA1f3bb169…：A EXIT0 5/5、B EXIT0 15/15/0/0；原指定 B 命令 EXIT2（限制 B 工作根），未算红；owner新增`--work-root`后在C专属目录通过。|
|四个独立变异|PASS|最终同SHA全新物理副本R1–R4均0→1→0；无复用B patch/root；见下表与isolation.json。|
|独立 oracle|PASS|自写原文固定范围/正则片段，不导入 A/B；五G事实全一致；6 gaps 均有事实/消费/缺口/落点/验收/责任。|
|诚实性/范围|PARTIAL/BLOCK|C自身白名单、冻结SHA与tracked diff检查通过；B notes:191承认覆盖首轮原始runs/isolation/logs/mutations等，违背历史证据禁改。|
|条件映射/R70|PARTIAL|条件1–4已关；条件5历史保全未关；条件6候选/验收已登记，生产动作留R70，FireWall契约需准入证明。|

- 首轮证据 `target/round69/c/attempt-1/`；最终冻结复核 `target/round69/c/attempt-2/`；UTC/完整argv/EXIT逐条见各 runs.json；原 EXIT2、脚本失败与重试均保留。
- A 命令：`py -3 tools/testing/round69_render_contract.py --root . --out target/round69/c/attempt-1/audit.json`，04:08:17Z–04:08:18Z EXIT0，5/5，与 A 自报一致。
- B 修订复跑：`py -3 docs/testing/round69/contract-tests.py --root . --work-root target/round69/c/suite-replay --out target/round69/c/suite-replay/attempt-1/test-results.json`，04:10:57Z–04:11:23Z EXIT0，15/15/0/0，与 B 计数一致。
- 三个临时环境变量均设 C 自有 tmp；各变异单独绝对 root/tmp/out；每份1349文件，nlink均1、无reparse/symlink、fileID与源不同。
- 首轮 B SHA `d653c0e1…`；最终 B `1f3bb16927ff877da2e8c33b53c9f238e2caf37e95c6f24f5a13e5117a65c315`；A工具`1fdf4069…`、矩阵`e513550f…`始终一致；A/B/C 103源文件manifest及checks完全相同，无自引用。
- 最终 `py -3 docs/testing/round69/contract-tests.py --root . --work-root target/round69/c/attempt-2/suite --out target/round69/c/attempt-2/suite/attempt-1/test-results.json`，04:16:48Z–04:17:12Z EXIT0，15/15/0/0；全套四红于04:18:02Z完成。

|独立变异|与 B 不同的有效代码破坏|目标/实际 fail|baseline/red/restored|
|---|---|---|---|
|R1|DrawBlend `point.Offset(mi.Y, mi.X)`交换坐标，保留调用顺序|G2 / G2，offset_call|0/1/0|
|R2|恢复调用改 `SetBlend(oldBlend, 0.5F, BlendMode.INVLIGHT)`|G3 / G2+G3，restore argc 1→3|0/1/0|
|R3|mask rectangle 改 MaskWidth/MaskHeight|G4 / G4，mask_draw.rect|0/1/0|
|R4|VisiblePixel `int accuracy = 2 + _images[index].ShadowY`|G5 / G5，other_read 0→1|0/1/0|

- B 自身 R2 附带 G2、R3 附带 G5 已披露且是冻结事实真实漂移；C R2也附带G2，目标G3确实失败，非输入SHA或EXIT2抢先拒绝。
- 每个红的 mutated.json、完整 mutated-source.cs、patch.diff、baseline/restored、首败 mismatch、原/变/恢复 SHA 均保留；可信工具从主冻结路径加载，不从变异root导入。
- Oracle G1：MLibrary.cs:640–673，Point默认false，true时一次offset先于裁剪，整数重载0次；G2:692–711 默认false/rate=1、一次先裁剪。
- Oracle G3：MLibrary.cs:704–709只存bool、恢复1实参；DXManager默认1F/NORMAL，NORMAL=SourceAlpha/One，INVLIGHT=BlendFactor/InverseSourceColor。
- Oracle G4：MLibrary.cs:778–797，主体/mask共point/Width/Height；MaskX/Y未绘制消费；纹理创建用主体尺寸与Length。
- Oracle G5：冻结97个Client C#，6命中行/8标识符，2声明+4反序列化、0render/0other read；只证明该全集。
- gaps 6条皆 actionable；逐段实读含 Effect、SpellObject、GameScene 条件offset及照明边界；Rust读取 entities/frame/scene/spellfx/projectile/image，与矩阵消费说明一致；没有把整个场景混合状态还原误作已实现。

|门下强制条件|关闭状态|依据|
|---|---|---|
|1 actionable缺口|PASS/关闭文档项|GAP-1–6完整七字段；GameScene、各重载/照明边界均登记；实现仍PARTIAL。|
|2 独立expected/输入路径|PASS（历史时间限度已披露）|独立原文oracle一致；工具写出仅--out；B实际调用缺项/重复ID检查；不以observed回填。|
|3 四真红|PASS|完整有效C#内替换；目标语义EXIT1；R4有效字段读；G1独立语义断言。|
|4 隔离/可信冻结|PASS|最终1f3bb169…同SHA重核；4×1349文件，无链接、fileID/nlink/绝对root/out/tmp齐；C两attempt保全。|
|5 命名测试/保全|PARTIAL/BLOCK|15测试含五命名G、0skip；B首轮runs/timings/isolation/logs/tool-green/tool-semantic/mutations/input-errors被覆盖（B notes:191）；旧test-results保留不足替代首败原件。|
|6 R70生产前置|PASS登记/PARTIAL实现|矩阵3候选＋dispatch:33–35 FireWall单一路径/来源/验收已登记，未实施。|

- R70独立意见：优先C2渲染层optional-offset API＋已证单一调用锚点；FireWall Crystal Magic1630/6帧×120ms/blend0.8，对照 Rust Magic0/10帧×60ms/Over，不应直接宣称等价。先证明图库/帧/坐标契约，再按逐帧signed offset一次且先裁剪、正负/跨帧/屏外RGBA与失效变异推进；保持mapping/time/event/wire。
- PARTIAL承接：6 gaps全部未实现；生产DrawBlend rate/INVLIGHT完整状态、mask、shadow、spellfx/全UI、真实素材设备/pixel/GPU一致性均未背书；R70三候选待选，不能连续用静态矩阵代替生产推进。
- 必改最小复现：读 `target/round69/b/attempt-1/notes.md:179–191` 与 `isolation.json:14–17`，首轮声明a77…而原路径已是最终1f3bb169…，notes明确承认覆盖。退B恢复原始首败/恢复/隔离/调用原件（必须真实备份，禁止重跑伪造历史），最终修订应另存attempt；无备份则如实不可补证，由Lead裁决，C不降格为已关闭。
- CLI：2026-10-27T04:21:51Z `node C:/Users/diana/.dsh/skills/edict/scripts/edict.mjs delegate-result JJC-20261027-006-sub-fcc589b0 <BLOCK摘要>` EXIT0，原始回执见delegate-result.json；随后C包装脚本打印✅触发GBK UnicodeEncodeError EXIT1，回执已写盘，未重发CLI。
