# R75 尚书省独立验收

- verdict：**ACCEPT-WITH-PARTIAL；必改清单：无（0）**。仅验收当前差异合同与证据；门下确认仍待办理，不宣称整轮收口。
- 主单 `JJC-20261028-002`；被审 HEAD=origin/main=`5a10d727b2916131d15d5a0a0f5ff48f353c7ea7`；执行者 gpt-6.1-sol。
- 核验窗 UTC 2026-10-27T21:12:40Z–21:16:25Z；命令/逐条UTC/原始EXIT/日志bytes与SHA见 [runs](../../target/round75/shangshu/attempt-1/runs.json)、[补核](../../target/round75/shangshu/attempt-1/supplement-runs.json)、[checks](../../target/round75/shangshu/attempt-1/checks.json)。

|项|裁定|独立实值|
|---|---|---|
|1 看板|PASS|父单实读Doing、todos=[]；A a9d799da/B 5d3090ba/C 195ef58c/D db4a1027 均Done且delegation_result非空；list另核仅这4子单；五show/list均EXIT0。|
|2 提交/生产/freeze|PASS|七提交96e46cc→a6e77ef→30246bc→baf71d2→6b6e4bc→a74cfbc→5a10d72；crates仅新增R75测试，生产src零改；protocol/server/tools diff空；两freeze字节/SHA如下。|
|3 唯一门禁|PASS|gate_head=30246bc为HEAD祖先；log/meta原字节一致；GB18030解码12 OK/0 FAIL；meta为12结构字段加meta_note；gate后crates/tools diff空；未重跑全量。|
|4 清单|PASS|39/39逐项bytes/SHA，tracked由git cat-file blob及记录commit/delivery_head OID双核、worktree读原字节；无自引用；外置摘要、gate及audit段一致。|
|5 隔离抽证|PASS|git -c core.autocrlf=false archive 5a10d72全新物理副本1324文件、链接0；覆盖测试SHA一致；legacy R75=7/0/0，R74/R73/R72=5/9/9（均0失败/0忽略），fmt EXIT0。|
|6 诚实性|PASS/PARTIAL|C FAIL/必改2→C2 BLOCK/必改1→基线勘误→C3 PASS/必改0历史完整；T原字节未改；A/B/C3记录45份历史实物复算一致；只增补录；README计数已修；全文diff抽读无正面扩称。|
|7 范围/卫生|PASS|D六文档恰白名单；client/roadmap仅尾部追加；新增审计/裁定/勘误均docs授权范围；tracked target=0；入场工作树仅两freeze，交接仅另增本报告；未改交付物。|

- 清单：21,043B，SHA256 `f046ebb9d58319f1bd2a9bc45817743b34a93a0d4d89640ed220d5a304c47fce`；stage=post-audit；delivery_head=`a74cfbcf3a7bc253e44b42f7a258a9cd51aecfbf`。
- 外置SHA：87B，SHA256 `ef4ad82104b655be3fdc2eff1cad8dd96019611e82d5d064c7967503a8a22986`；内容等于上述清单摘要；自身与清单均不计入39 artifacts。
- gate log：214,417B／`e6eec12095ea80b042dde250f4f1fd55d3d4b76aa5708a94d2da15df2aa38342`；meta：481B／`4483324991985472bc1bbb7e4a023fdb5d1d53b37e88564512858edd7c8a1fba`。
- meta UTC=2026-10-27T20:44:09Z–20:47:52Z、原始EXIT0；attempt=1、operator=lead；命令为历史ci.ps1 -KillStale，仅核验未执行。
- T报告：32行／5,658B／`acdafa7e90ce0e60a9e136524f91d571747178a034dbd3b2ebff066724f605e4`；PASS-WITH-PARTIAL/必改0；清单audit与之对应，历史38项口径不回写。
- freeze-before：44,844,165B／`70845aefd4dd689a55ab0618b36bbf692a4b33ebb5ee2f3a662dedc17a3dc9de`；freeze：45,219,533B／`c7d896e7f5eee4706b943912c0c564d10d61c98d163143f051c2d632daecde32`；均未跟踪。
- 覆盖测试：56,403B／1454行／`ecff550dfa0bcd671d32723bd18fbcd444c93c4d173c1562bfbc0f8ad185a05c`；G1/2/4/5/6生产实画、G3/7模型，不推导生产完整寿命修复。
- 隔离：`target/round75/shangshu/attempt-1/repo`；CARGO_TARGET_DIR=`同根/cargo-target`；TEMP/TMP/TMPDIR=`同根/tmp`；DEV/TEST_DEBUG=0；nlink=1、无symlink。
- 命令：`cargo test -p mir-client --features mir-render/legacy --test <R75/R74/R73/R72文件>`；`cargo fmt --check`；五命令均EXIT0，30 passed/0 failed/0 ignored。
- 实测L2：R75 35,382ms、R74 1,506ms、R73 2,711ms、R72 4,002ms、fmt 4,516ms；独立执行是本次验收硬要求，不降为旧报告引用。
- 工具偏差：首archive捕获stdout触发`spawnSync git ENOBUFS`（EXIT=null），截断tar解包EXIT2；改Git直接-o新归档后archive/unpack均0，原失败日志完整保留。
- 非阻断口径：README现为54物理行，清单旧lines=53属计数元数据；bytes/SHA与blob均吻合；合同pre-audit阶段说明作为D历史交付保留，以当前清单post-audit为准。
- PARTIAL 1：GAP-1/2、R70-C2，完整DrawBlend/rate/Mode/DrawColour与src×0.8×alpha未实施。
- PARTIAL 2：GAP-5/R70-C1、R70-C3、GAP-3/4/6，其余149/1620/起手/调用点、尺寸居中、mask/tint/shadow/Light/AnimationOffset。
- PARTIAL 3：R71时钟/对象、R73-T1，NextMotion/Repeat/26完整寿命；历史反向注释子项修正不扩大关闭。
- PARTIAL 4：R72 B plan-M4/legacy lint、R71-C7永久PARTIAL、历史E1仅补救CLOSED、R69条件5永久缺口。
- PARTIAL 5：R68/R49–R70/R61–R67原号；R61 P01–P12、R62①–④、R63 P1–P7、R64①–⑥、R65 G1.2/G2、R66生命周期/水位、R67容量/保护/map epoch；R74 artifacts=34。
- PARTIAL 6：R75-E1/C-M1 CLOSED-AS-PARTIAL；B aborted原字节不可恢复，不伪造有效红绿证据。
- PARTIAL 7：R75-E2/C-M2声明级PARTIAL；A首次actual stdout缺失，旁证不替代stdout；C3仅关闭任务书基线身份冲突。
- CLI：UTC 2026-10-27T21:16:25Z，`node …/edict.mjs done JJC-20261028-002 --as shangshu` 原始EXIT=0；随后`validate --as shangshu` 原始EXIT=0（251任务schema v3.1）；再show EXIT0确认Review；未绕过readiness、未代门下确认。
- 边界：仅新增本报告与`target/round75/shangshu/attempt-1/**`；不commit/push、不运行tools/dev/ci.ps1；真实素材/GPU/设备/全UI/跨后端/Live音频仍未覆盖。
