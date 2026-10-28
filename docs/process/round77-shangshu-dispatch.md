# R77 尚书省派发：149 Trap八帧映射差异与素材准入合同

主单 `JJC-20261028-004` 已 Assigned→Doing；HEAD=origin/main=`69428266cd00d900abafa97bd415b47b4f4782a5`。仅登记四子单；未启动执行者，CLI delegate自动Doing不代表执行已开始。看板保留初始标题“第77轮：149 Trap八帧逐格映射”，本轮范围以本派发契约为准。

|主/子|真实 TASK_ID|部门/模型|依赖/写范围|
|---|---|---|---|
|主|JJC-20261028-004|尚书/Lead|仅Lead提交及唯一全量门禁|
|A|JJC-20261028-004-sub-2a7d07c6|兵部/deepseek|B前置后；仅新增测试/只读工具，target/round77/a/attempt-N/**|
|B|JJC-20261028-004-sub-ab0c9973|工部/deepseek|先独立工具/夹具；A后同SHA独立六M；target/round77/b/attempt-N/**|
|C|JJC-20261028-004-sub-31c36f9a|刑部/gpt|A及B六M后；round77-c-evidence.md只增补录，target/round77/c/attempt-N/**|
|D|JJC-20261028-004-sub-ace66068|礼部/gpt|Lead成功全量后；下述四文档及两追加节，target/round77/d/attempt-N/**|

以下八条逐字写入四子单return_spec，完整角色白名单/验收/原始show见[contracts](../../target/round77/shangshu/contracts.json)及[CLI收据](../../target/round77/shangshu/cli-receipts.json)。
计划：`docs/process/round77-plan.md` 153 行/19,895B/sha `d2138698ff4751009775881fa97d2914cc2721005721477683a9a0c6fb6ceaaa`（含七 G、六 M、白名单：仅新增自包含 Rust 测试 + 只读 Python 素材准入审计工具；生产/wire/server/历史测试零改；不新增 supersede）。
门下 C1–C5 与唯一必改 **M1**：素材调查结论**仅限八套 WZX**；须在 D 文档/R78 前置承接发现的候选来源——`resource/community/mirfiles-default-crystal/Default Crystal/Data/Magic2.Lib` 与 `resource/community/mirfiles-crystal-patch/crystal__patch_gz__Data__Magic2.Lib`（v2/count2790/21,091,901B/SHA `f449aa4c96632f13454d7746ab47678c06175be02e69a6dc8fdfd036755fc0d2`；2360..2367 gzip 解压=width*height*4；**只是待认证候选，不得宣称已认证**）；不得扩本轮生产范围。
默认 Magic2：WZX 5,968B declared1460/physical1480，WZL 10,290,606B；2360..2367 缺索引。2025 语料八帧物理/format5/zlib 匹配但 UNVERIFIED_SEMANTICS，禁止同号替换默认库。
历史冻结：R76 G8 精确 Magic2 旧 18 帧、R59 magic 116 帧不动；不新增 supersede；R78 处理 18→26 替代范围。
七 G（必须每项可失败断言）：G1 默认 Magic2 2360..2367 缺索引且物理边界不足；G2 2025 候选八帧物理/format/zlib 边界匹配；G3 2025 候选语义 UNVERIFIED 不得 READY/不得同号替换；G4 Magic2.Lib 候选（两份同 SHA）存在、v2/count2790、八帧 gzip 解压尺寸=w*h*4、且状态=待认证候选；G5 Trap 现行 Rust 行为锁（spellfx.rs:40-60 默认 700ms、:418-435 通用 Magic/60ms/居中 Over）与 Crystal 2360/8帧/100ms 差异矩阵逐项可核；G6 准入状态机（NOT_READY/UNVERIFIED_SEMANTICS/AWAITING_BINDING/READY 仅人类批准后）+ 禁自动升 READY；G7 R76 G8/R59 冻结不改（R76 18 帧、R59 116 帧）且本轮零生产 diff。
六 M（B 独立复现）：M1 破坏 WZX declared/physical 断言；M2 2025 八帧解压/边界变异；M3 把 UNVERIFIED 改 READY（禁自动升）；M4 Magic2.Lib SHA/count 变异；M5 差异矩阵数值变异（100ms→120ms 或 8→7 帧）；M6 准入状态机绕行变异。
DAG：B(独立验证工具/夹具前置) → A(新测试+只读工具实现) → C(刑部独立核验) → Lead 提交+唯一全量门禁 → D(礼部文档：`docs/testing/round77/{README.md,round77-trap-asset-admission.md,final-manifest.json,manifest.sha256}` + `docs/client.md §5.42` + `docs/roadmap.md §二十五`) → T 终审 → 尚书验收 → 门下确认 → Done。
证据根 `target/round77/**`；A/B/C 独立物理副本（禁硬链接）、同测试/工具 SHA、orig stdout 保全；D 只记实跑不伪造；禁称「生产已映射/素材已认证/整轮通过」。

C1（计划:67–70/78）：合成known fixture绑定只证明算法；真实binding须独立门下审议。无绑定/SHA错/证据SHA错require2；普通audit0不等于READY；错误1与NOT_READY2分开。
C2（计划:70/83/96/109）：七绿0失败0忽略；每M单patch、no-run0、指定G恰1断言红101、恢复SHA后七绿；0tests/编译错/资源缺失/异常不算红。先冻expected/fixture及首actual，独立Python整帧oracle不调用生产helper。
C3（计划:96/104–105/125）：B/C分别六M，物理副本nlink1，禁symlink/junction/hardlink；独立target/temp；源码/测试/夹具/工具原SHA保全；清单生成器固定排除/排序并可重算，失败attempt只增。
C4（计划:107/124–126）：D只记本人实跑文档检查与原日志；全量仅Lead且不SkipTests/SkipPostgres，内部skip逐项披露；官方/legacy clippy原EXIT分列；全量后crates/tools变化重闸。
C5（计划:52–62/126/144）：未来代码仅新增一测试一只读工具；生产/wire/server/历史测试/素材零改；D/Lead获批文档例外按表执行。唯一Lead提交；本审议无commit/push。
必改M1（计划:25/47/144/149）：交付文档限定“八套本地WZX”而非全部Magic2格式；承接上述Lib路径/SHA/可解压事实与来源绑定缺口至R78。不得默认为2025背书、不得据候选改本轮生产或扩格式白名单；C核验D落盘后关闭此项。
映射：C1→A准入/B负控/C独审/D措辞；C2→A固定断言/B及C六M；C3→A/B/C副本与日志；C4→Lead唯一门禁/D实跑；C5→全员范围/Lead唯一提交；必改M1→B候选事实/A状态断言/C核验/D与R78承接，D后由C只读复核交T/尚书，当前不宣称已关闭。
CLI差异：实读--help要求`delegate <ID> <派发方> <承接方> <指令> <交付要求>`，故四次显式`shangshu <部门> ... --as shangshu`；与指令短式差异如实披露。state/validate均EXIT0，validate JSON为schema v3.1、261条、issues=[]；四次show核父/部门/return_spec全字匹配。
工具异常：初建证据脚本前两次write参数校验失败（justification空串；workspace-write非更宽模式），均无文件写入；后在当前danger-full-access模式成功。首轮静态脚本EXIT1：审议必改M1有Markdown加粗标记，原文匹配失败；保留失败原件，修正为仅去加粗标记后核文字，四份return_spec不改。无CLI失败/重试，无执行者agent_id。
契约差异：本次七G/六M及B→A覆盖旧计划:74–105的编号/DAG，旧wire/UI/公开API行为覆盖作补充；D文件以本次asset-admission命名覆盖旧readiness-contract；计划不改。必改M1≠变异M1。G4由B/C只读探针核候选事实，不扩格式白名单；READY须人类批准且真实binding须独立门下审议。
门禁/边界：本次仅派发与静态；未来定向七G/六M及历史回归按契约，Lead独跑fmt/两种clippy/完整ci.ps1 -KillStale且披露skip；全量后crates/tools变化重闸。A/B/C/D不commit/push，本次不commit/push/不跑cargo/CI/不启动执行者。
风险：素材仅本地，Lib可解压不等于已认证；R76 18帧/R59 116帧精确冻结，R78须先认证绑定并显式裁定18→26替代范围。计划/审议及两freeze bytes/SHA、crates/tools零diff与git diff --check核验详见[checks](../../target/round77/shangshu/dispatch-checks.json)。
