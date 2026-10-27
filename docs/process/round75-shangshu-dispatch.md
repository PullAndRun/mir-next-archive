# R75 尚书省执行派发

- 父单：`JJC-20261028-002`「第75轮：149精灵生命周期差异合同」；基线 HEAD=origin/main=`6502a90f83a977562c20a1b5b901a5672b0410d2`。
- 依据：[计划](round75-plan.md)136行/16247B；[门下](round75-menxia-review.md)35行/5506B，PASS-WITH-CHANGES，必改0；二者SHA实核一致且原件未改。

|执行|真实子单ID|部门/模型|任务与依赖|
|---|---|---|---|
|A|JJC-20261028-002-sub-a9d799da|兵部 bingbu / deepseek|测试作者：七G、Crystal模型、fixture/expected先冻结、同断言红绿、至少M4；登记后可启动。|
|B|JJC-20261028-002-sub-5d3090ba|工部 gongbu / deepseek|依赖A冻结；物理副本、五M逐次编译绿→指定红101→恢复七绿、独立全采样oracle、回归/卫生。|
|C|JJC-20261028-002-sub-195ef58c|刑部 xingbu / gpt|依赖B；共享原件只读；隔离复跑G/M/oracle、范围/诚实/语义核验，写C证据；必改0交Lead。|
|D|JJC-20261028-002-sub-db4a1027|礼部 libu / gpt|依赖Lead提交及唯一门禁成功；README/合同/清单/外置SHA/client §5.40/roadmap §二十三。|

- DAG：A→B→C→Lead提交+唯一全量门禁→D→独立终审→尚书验收→门下确认；子单自动Doing仅表示登记，B/C/D须等依赖，不并发抢跑。
- 完整契约：[dispatch-contract.json](../../target/round75/shangshu/dispatch-contract.json)；四份看板return_spec均完整含C1–C6原文、各自白名单、G/M要求、证据根`target/round75/{a,b,c,d}/**`，show逐字验证一致。
- C1 A/B/C：冻结同断言/expected/fixture SHA；模型变异另存原文件SHA、patch、恢复SHA，不能称整文件SHA不变。
- C2 B/C：nlink1物理副本、禁硬链接/junction/symlink；独立target/TEMP/TMP/TMPDIR；no-run 0→指定exact断言101→恢复七绿。
- C3 A/B/C：独立fixture oracle，先expected后actual；G1/2/4/5/6全采样整帧与G3/7模型轨迹分别计数；非空异色offset可见，G4空帧可黑。
- C4 全体：差异合同不宣称生产寿命/严格等价/Blizzard映射已实现；不混SpellObject>=与Effect>，不把2000当寿命，不否定共用底层时间来源。
- C5 全体：attempt只增不改；R71–R74及两freeze原字节保全；仅Lead提交/全门禁，不SkipTests/SkipPostgres；D后crates/tools再改才重闸。
- C6 Lead/D：七G/五M矩阵与两类覆盖数齐、C必改0才门禁；PARTIAL按原号承接，R74 artifacts实数34，披露计划112旧35口径。
- 白名单：A仅新增测试及a证据；B仅b证据；C仅C报告及c证据；D仅六文档及d证据；精确路径见完整契约。所有生产src（含spellfx）、wire/server、两历史freeze均禁改。
- 门禁边界：生产零改仍需Lead唯一全量`pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`；A/B/C/D禁commit/push/ci.ps1。尚书本次未跑cargo/CI、未提交。
- 风险：模型自证→独立三源全采样；现状负控不等于功能修复；未来替换须supersede；纯软件不证明真实素材/GPU/设备/全UI/Live音频；完整寿命及历史缺口仍PARTIAL。
- 建议落实：load通常正时戳，G2 start0为合法合成输入；协议锚点使用`crates/mir-protocol/src/packets/server.rs`；不改计划原件。
- CLI差异：实机使用delegate而非臆造子命令；无模型/depends_on参数，契约承担，实际agent由Lead启动并补映射；delegate return_spec实测保留斜杠，普通state备注清洗仍适用。
- CLI结果：四delegate/四show/state Doing/validate/父show均EXIT0、stderr空；validate为251条schema v3.1；原始UTC/argv/EXIT/stdout/stderr见[cli-receipts.json](../../target/round75/shangshu/cli-receipts.json)。
- 工具异常披露：glob一次30秒超时；write两次空justification拒绝及一次非更宽模式拒绝，均未写入；后续正常写入成功；无CLI失败、无伪造成功；详见[checks.json](../../target/round75/shangshu/checks.json)。
- 状态与启动：父单实读Doing；A可由Lead启动，B/C/D等待；派发官depth1无可用下层agent权限，未声称执行者已经启动，agent_id暂null。
