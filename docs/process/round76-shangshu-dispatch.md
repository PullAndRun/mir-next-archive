# R76 尚书派发：登记完成，未启动执行者
主单 JJC-20261028-003：Assigned→Doing；基线 cf84e40f1de1f290357e2a73fcb5adafafdf1fc3；本次只写本页与target/round76/shangshu/**；无cargo/CI、commit/push、执行者启动。
依据：[计划](round76-plan.md)166行/19473B/SHA256 a5161f8afaa5b5bf6460932cedc53f5cf97b74a56da11c44cecea834fd845138；[门下](round76-menxia-review.md)36行/5934B/SHA256 f15e0344e92f674c3ff25079c652a830b505999e0f55808ca11e52ea0b71f0a9，PASS-WITH-CHANGES。
|段/推荐模型（非看板字段）|真实子TASK_ID|写范围与证据根|
|---|---|---|
|A/bingbu/deepseek|JJC-20261028-003-sub-691ce59a|crates/mir-client/src/ui/spellfx.rs；crates/mir-client/tests/round76_trap_hexagon_mapping.rs；crates/mir-client/tests/round72_poison_cloud_mapping.rs（仅四处）；tools/assets/analyze_usage.py（仅MAGIC_FRAMES十帧单行）；target/round76/a/attempt-N/**；证据首attempt target/round76/a/attempt-1/**|
|B/gongbu/deepseek|JJC-20261028-003-sub-05f353fc|target/round76/b/attempt-N/**；证据首attempt target/round76/b/attempt-1/**|
|C/xingbu/gpt|JJC-20261028-003-sub-af9acdec|docs/testing/round76/round76-c-evidence.md（只增附录）；target/round76/c/attempt-N/**；证据首attempt target/round76/c/attempt-1/**|
|D/libu/gpt|JJC-20261028-003-sub-433959ad|docs/testing/round76/README.md；docs/testing/round76/round76-trap-hexagon-contract.md；docs/testing/round76/final-manifest.json；docs/testing/round76/manifest.sha256；docs/client.md（仅追加§5.40）；docs/roadmap.md（仅追加§二十三）；target/round76/d/**；证据首attempt target/round76/d/**|
完整执行契约：[contracts.json](../../target/round76/shangshu/contracts.json)含各段验收/依赖/原样C/M；[registered-tasks.json](../../target/round76/shangshu/registered-tasks.json)含真实ID；所有agent_id为空/未启动。
DAG串行：门下批准→尚书登记→A冻结→B→C必改0→Lead唯一提交与成功全量门禁→D→独立终审→尚书验收→门下确认；D后代码/工具再变必须重闸，不把Doing当执行完成。
A：实现149来源/两param专属分支、100ms十帧elapsed/0<=age<4000、格锚与当帧signed offset一次、边界continue/在公共700前；八G按批准计划80–87行命名，G1另核event.id/object_id/payload参数方向，G2含80/99/100，G3同时锁公共duration_ms=700与文字700；G4裁剪，G5 Add/缺帧，G6公开UI Add像素，G7旧路径，G8素材合同。
A：R72仅1383/1404注释与1408/1429两枚举Trap→Reincarnation，expected/fixture不动；素材仅MAGIC_FRAMES增range(1390,1400)、Magic2不变；红绿物理副本/原stdout/迁移与恢复SHA、真实十帧来源及本地pack证据必须完整。
B：git -c core.autocrlf=false archive cf84e40独立物理副本+A两文件逐字节覆盖，另应用获批R72/素材补丁以保持G8/回归；nlink1/无symlink或junction、独立target/TEMP/TMP/TMPDIR、CARGO_PROFILE_DEV_DEBUG=0；八绿→每M no-run0→指定断言红101→原字节恢复SHA→八绿，M7 Python0且Rust G8红101。
B/C：独立oracle不得复用A脚本/生产helper，至少G1/G4/G6整帧像素且按计划G1–G7全采样，case数逐项披露；R75=7/R74=5/R73=9/R72=9逐套原stdout；C独立复现七M（至少M1/M4/M6亲做）、核范围/两freeze/R72原件四处diff/真实十帧与诚实性，M1/M2必改0再交Lead。
D：六件=docs/testing/round76四文档+client §5.40+roadmap §二十三；pre-audit≥16 artifacts、gate锚待Lead，外置SHA免自引用；新合同/manifest显式披露新产品窗和仅R72 G5旧锁supersede，C后置只读复核。
以下C1–C5/M1–M2逐字纳入执行契约（非摘要、不得放宽）：
C1（尚书/Lead）：派发逐字纳入本C/M，先登记后启动；生产只改crates/mir-client/src/ui/spellfx.rs，另仅新增R76测试、获批R72四处替换、素材单行增量与本轮文档；禁止泛化docs/process/round76-*.md白名单覆盖既有审批原件。
C2（A/C）：新分支在公共700过滤前，只准149来源、两param，负age/4000边界均continue；G3同时断言duration_ms(TrapHexagon)==700、文字700，G1分别核event.id与object_id，G2必须含80/99。
C3（A/B/C）：先保全原R72完整源码及bytes/SHA/UTC，公开四处diff与前后SHA；基线58f64ab2d30b69f3aea5c07a8975e988ca842bce45972499a6f93e120958baea/65,322B；保留原历史日志/fixture/expected，不宣称R72字节不变；C独立检查仅批准行改动与迁移后9项绿。
C4（A/C）：真实十帧逐帧记录源WZL/WZX哈希、索引/format/尺寸/signed offset、PNG/输出索引哈希与读取结果；按现有资源索引提取，再验证本地pack或运行时路径；完整argv/工具身份可复算，缺帧即退中书，禁以合成绿代替。
C5（B/C/Lead/D）：独立物理副本，冻结expected/fixture及原stdout早于actual；每M前绿→编译0→指定断言红101→恢复SHA→八绿；M7 Python0且G8断言红；case数/逐套计数/恢复与失败attempt只增保留，Lead唯一提交及全门禁，D后代码再变重闸。
M1（A/D必改口径）：执行合同/新注释明确“149 Trap从700改为4000的新产品精灵窗”；不得沿用计划71行“既有149窗”而暗示Trap原来4000，禁src×0.8推断；不扩改历史注释。
M2（D/C必改披露）：新合同和manifest显式列supersedes仅R72 G5的149 Trap旧像素锁，并公开C3要求；其余技能/历史合同不被默示关闭，M1/M2完成由C核必改0再交Lead。
差异/启动门禁：简报A写Swing但批准计划69行及门下15/22行要求Magic（Swing实际Magic2）；待Lead书面澄清，不静默执行错库。
差异/启动门禁：简报M3删4000窗/M5 clamp/M6 Over，与批准计划M3 %10→9/M5 Over/M6删来源不同；批准M1 Magic→Swing/M2 100→80/M4 offset false/M7删十帧。两套编号原样留contracts，待Lead确认；禁止漏掉来源测试。
差异：简报G5/G6/G8描述与批准八G分组不同，附加断言归入对应G4/G5–G6/G1，不削减G6公开UI或G8素材计划；D路径按批准白名单四测试文档+client/roadmap，若改范围须Lead澄清。
风险/禁称：4000是Trap从700变化的新产品精灵窗，非Crystal等价；保留严格NextMotion/Repeat/26寿命、owner/AnimationOffset/rate/Mode/Light与设备视觉PARTIAL，禁src×0.8推断、合成绿代替真实素材可交付。
历史承接：计划143–153全部原号与R74 artifacts34不清零；含R72 B plan-M4、R71-C7/E1、R69条件5、R61–R67、R75-E1/C-M1旧失败原字节不可恢复、R75-E2/C-M2首actual原stdout缺失；不追溯造证。
保全与回滚：两monster-native freeze未跟踪JSON/计划/门下原件不动；本地素材不入git；只Lead按白名单逆撤本轮、不reset共享树/不删失败attempt。
真实CLI：node C:/Users/diana/.dsh/skills/edict/scripts/edict.mjs --help EXIT0；delegate --help EXIT1（只打印合法位置参数用法，并非独立子帮助成功），不得伪报0。
真实CLI：delegate JJC-20261028-003 shangshu <bingbu|gongbu|xingbu|libu> <指令> <交付> --as shangshu 四条EXIT0；state JJC-20261028-003 Doing <说明> --as shangshu EXIT0；validate --as shangshu EXIT0/256条（UTC 2026-10-27T21:43:37.743Z–21:43:37.807Z）。
每条完整真实argv（含逐字指令/交付）、UTC起止/EXIT/stdout/stderr：[cli-receipts.json](../../target/round76/shangshu/cli-receipts.json)；CLI不支持depends_on/model参数，未写不存在字段，推荐模型/依赖只在本合同；登记≠启动。
验核：[dispatch-checks.json](../../target/round76/shangshu/dispatch-checks.json)记录行/B/SHA、审批/两freeze保全、真实子单读回、HEAD与git diff --check；只验证派发，不声称功能通过。

## Lead 澄清回执（2026-10-28）

本回执由 Lead 授权、尚书执行；如有再冲突以计划+门下为准。本节只追加澄清，不改审批原件、A/B/C/D真实ID、主单状态或既有登记记录；原第25–27行的待澄清事项以本回执明确的口径执行。

以下四点为 Lead 书面澄清，逐字收录：

1. 库/入口：TrapHexagon 149 走 **Magic 库**，`FxKind::Magic`（非 Swing、非 Magic2），`index=1390+phase`，`phase=(age/100)%10`，`0<=age<4000`；`draw_effect_blend(FxKind::Magic, 1390+phase, x, y, apply_offset=true)`；格锚点 `world_to_screen(center, event.location)`；该分支在公共 700ms 过滤前，边界 continue。凡简报含 `FxKind::Swing` 之处属 Lead 简报笔误，一律以门下裁决/计划:69 为准并披露。
2. 七变异 M1–M7：以计划字面为准；简报 M3/M5/M6 的简述不作数，契约须引用计划行号定义。
3. 八 G 分组：以计划 G1–G8 字面为准（G1 wire 身份、G2 帧边界含 80/99、G3 公共/文字 700、G4 锚点/offset 一次、G5 Add/缺帧、G6 UI、G7 非149旧路径、G8 素材计划）；简报 G5/G6/G8 错位更正，附加断言可留但标注为细化。
4. D 写范围：四份 `docs/testing/round76/**`（README、合同、final-manifest.json、manifest.sha256）+ `docs/client.md §5.40` + `docs/roadmap.md §二十三`，共六授权文件。

逐条执行定义与依据：

- 库/入口以[计划第69行](round76-plan.md#L69)及[门下第15、22、26行](round76-menxia-review.md#L15)为准；Magic→Magic、Swing→Magic2，错库披露适用于A实现及A/B/C独立验收。4000仍按门下第20、30行定义为Trap从700变化的新产品精灵窗。
- 七变异按[计划第96–102行](round76-plan.md#L96)逐项定义：M1 新分支 `FxKind::Magic→Swing`（第96行）；M2 `FRAME_MS 100→80`（第97行）；M3 `FRAME_COUNT 10→9`（第98行）；M4 新分支 `apply_offset true→false`（第99行）；M5 新分支 `draw_effect_blend(...,true)→draw_effect(...)`，移除末尾bool匹配签名（第100行）；M6 新分支删 `from_object_spell` 限制、只留技能条件（第101行）；M7 仅删素材新增 `range(1390,1400)`（第102行）。不得用简报M3删4000窗、M5 clamp、M6 Over替代；A/B/C契约以这些行号为定义。
- 八G按[计划第80–87行](round76-plan.md#L80)原名及字面逐项执行，并叠加[门下C2第26行](round76-menxia-review.md#L26)。简报“G5屏外/G6 Add/G8payload”分别归入G4屏外移入/裁剪、G5及G6的Add验证、G1 wire身份，均标注为细化，不能替换G5 Add/缺帧、G6公开UI或G8素材计划。
- D六授权文件依据[计划第58、119行](round76-plan.md#L58)：`docs/testing/round76/README.md`、`docs/testing/round76/round76-trap-hexagon-contract.md`、`docs/testing/round76/final-manifest.json`、`docs/testing/round76/manifest.sha256`、`docs/client.md`仅追加§5.40、`docs/roadmap.md`仅追加§二十三；不是在testing目录再扩为六件。既有D证据根登记保持。

逐条简报片段、裁定、依据行号与影响范围见[clarification.json](../../target/round76/shangshu/clarification.json)；追加性、原件保全、文件bytes/SHA及diff统计见[clarification-checks.json](../../target/round76/shangshu/clarification-checks.json)。本次无看板操作、无执行者启动、无cargo/CI、无commit/push。
