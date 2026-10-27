# R74 尚书省派发登记

主单 `JJC-20261028-001`：Assigned → **Doing**；门下 PASS-WITH-CHANGES C1–C4；基线 HEAD=origin/main=`0ee34e1b86b6fa2a9a9e1506a08e022eee5bb965`。
依据：[计划](round74-plan.md)、[门下审议](round74-menxia-review.md)；本次仅登记，不启动执行者，不作功能验收。

|单|TASK_ID|承接/模型|依赖与交付|
|---|---|---|---|
|主|JJC-20261028-001|尚书/Lead|A → B → C → Lead提交及唯一全量门禁 → D → 独立终审|
|A|JJC-20261028-001-sub-39860302|兵部 deepseek|门下批准；仅新 round74_normal_blend_lock.rs，G1–G5与独立期望冻结、基线五绿及M4负控|
|B|JJC-20261028-001-sub-82b02278|工部 deepseek|A冻结；物理副本、独立WZX/WZL解析与RGBA oracle、M1–M5红绿链|
|C|JJC-20261028-001-sub-c55440fc|刑部 gpt|B完成；同SHA另副本独立复现、范围/证据核验、round74-c-evidence.md追加附录，必改0交Lead|
|D|JJC-20261028-001-sub-05088f98|礼部 gpt|Lead门禁成功；六文档及manifest，纯文档静态核验|

|条件|责任与执行契约（已逐项写入四子单return_spec）|
|---|---|
|C1|A在首个生产actual日志前冻结expected RGBA、夹具与测试bytes/SHA256/UTC；B/C分别独立解析实际WZX/WZL、另算G4，三源整帧一致。|
|C1续|B/C逐M前绿5/0/0 → 单变异 → build EXIT0 → 指定断言 EXIT101 → 原字节恢复SHA → 后绿5/0/0；同测试/夹具SHA，普通文件nlink=1，独立target与TEMP/TMP/TMPDIR，禁链接、0tests/ignored/SKIP及编译/feature错冒充红。|
|C2|Lead独立小提交仅spellfx.rs:309注释Time>NextFrame才推进；留parent/commit/blob/SHA/单行diff，可执行语句零差异。A不改注释；其余crates仅一个新测试，历史零差异，新增allow/ignore=0；game-assets与两freeze不入库，freeze逐字节保全。|
|C3|D交付docs/testing/round74/{README.md,round74-normal-blend-contract.md,final-manifest.json,manifest.sha256}及docs/client.md §5.39、docs/roadmap.md §二十二；逐项blob或工作树来源/bytes/SHA，manifest排除自身和外置摘要，外置摘要校验manifest，核链接/原号/禁称。|
|C4|Lead唯一全量：`pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`，不得Skip*；新5与历史38按实跑核计，fmt/clippy及全量原始UTC/EXIT/实际门禁计数存证，禁硬填12或13；失败保留attempt，D后crates/tools再变更重闸。|

证据根：`target/round74/{a,b,c}/attempt-N/**`、`target/round74/{lead,gate,d}/**`；本次原始证据见[CLI记录](../../target/round74/shangshu/cli-records.json)、[契约与ID](../../target/round74/shangshu/registrations.json)、[输入冻结](../../target/round74/shangshu/inputs.json)。
本次状态/validate原始UTC分别为2026-10-27T16:42:08.801Z / 16:42:08.862Z，EXIT均0；validate为**246条任务符合schema v3.1**。每次help/show/delegate/state/validate均留完整argv/cwd/UTC/EXIT/stdout/stderr。
门禁边界：本次未跑cargo/CI、未commit/push、不改crates/计划/门下文件；子单agent_id均null，Lead另按DAG启动。
风险：RGB565仅alpha0/255，G4直接RGBA仅锁CPU rate1；完整DrawBlend/rate/Mode、真实素材/GPU/视觉/跨后端/Live音频仍PARTIAL。
历史原号承接GAP-1/2、R70-C2混合与GAP-5/R70-C1映射，R70-C3/R71时钟对象/R72 B plan-M4/R73-T1/GAP-3/4/6不改；C7永久PARTIAL、E1补救CLOSED；仅注释勘误可单独CLOSED。
禁称整轮通过、严格Crystal等价、设备视觉签核、完整DrawBlend闭合、rate0.8已实现、时钟/寿命已修复、C7已补齐、基线功能红或无条件PASS。
披露：CLI help明确create仅接标题，故按现有合法delegate建四子单；delegate自动将子单置Doing，表示登记状态，不代表执行者启动；依赖在return_spec中明确，须Lead串行落实。
披露：指定state备注中的A/B/C/D被CLI清洗为A，另以无斜杠progress澄清四项均登记；所有CLI均EXIT0。首次文件读取limit超限、空证据目录glob与脚本write空justification校验失败均未写入，修正后继续；无cargo/CI失败可报告。
本次静态验收与文档行数/bytes/SHA256另存[检查记录](../../target/round74/shangshu/checks.json)，不将看板校验称作功能PASS。
