# R76 尚书省验收
- Verdict：**ACCEPT-WITH-PARTIAL；必改 0**。主单 `JJC-20261028-003`；只验收 R76 有限切片，不宣称整轮通过、严格 Crystal 等价或生产寿命已修。
- 验收基线：HEAD=origin/main=`1eed4ac2352f4e74d871fbd7577bf6aaf4d93154`；清单交付头=`ec6ec0217769323ff659ceae05d2148ec4877a94`，stage=post-audit。
- 证据根：[attempt-1](../../target/round76/shangshu/attempt-1/)；[runs.json](../../target/round76/shangshu/attempt-1/runs.json)记录完整命令、UTC起止、原始EXIT；[checks.json](../../target/round76/shangshu/attempt-1/checks.json)保留逐项实算；[logs](../../target/round76/shangshu/attempt-1/logs/)保留原始输出。

| 必核项 | 结论 | 实读/实算与命令 |
|---|---|---|
| 1 看板 | PASS | `show --json`父单Doing/todos=[]；A/B/C/D逐张show为Done、todos=[]、delegation_result非空；`list --all --json`确认恰4子单。D原flow为libu于2026-10-28T01:19:09Z回填。 |
| 2 提交/范围 | PASS | `git log --oneline -12`、`rev-parse HEAD origin/main`一致；`git diff 41425b1..HEAD -- crates tools`=0B；后续8个变化路径均docs；`git diff --check` EXIT0。 |
| 2 freeze | PASS | 两JSON仍未跟踪：before=44,844,165B/70845aefd4dd689a55ab0618b36bbf692a4b33ebb5ee2f3a662dedc17a3dc9de；current=45,219,533B/c7d896e7f5eee4706b943912c0c564d10d61c98d163143f051c2d632daecde32。 |
| 3 门禁历史 | PASS | 原log/meta独立SHA256复算一致；attempt-2汇总12 OK/0 FAIL、EXIT0、gate_head=41425b1，UTC 00:28:12–00:31:25Z；attempt-1原log仍在，EXIT1且R59恰3失败、116/106原文保留。 |
| 4 清单file/blob | PASS | 25 artifacts=20 file+5 inventory；20 file逐项bytes/SHA mismatch=0；17带git_blob项均匹配delivery_head；外置摘要匹配、无自引用。 |
| 4 inventory | PARTIAL | A/B/C/C2/C3五目录实存并抽查顶层条目；聚合为producer-declared且无生成器，不背书其聚合bytes/SHA可复现。 |
| 5 独立抽证 | PASS | `git -c core.autocrlf=false archive HEAD`物理副本；17 tracked交付文件SHA匹配/nlink1/非symlink；必要Magic素材119文件SHA一致/nlink1；独立target/TEMP/TMP/TMPDIR、CARGO_PROFILE_DEV_DEBUG=0。 |
| 6 证据诚实性 | PASS | 实读T/T2、C/C2/C3、A/B补证、裁定1–5及README/合同：原C缺口由C2有效M7红闭合，C3有限R59 supersede；历史失败、范围与PARTIAL均保留，无扩称。 |
| 7 CLI收口 | PASS | `done JJC-20261028-003 --as shangshu`原始EXIT0，父单Doing→Review；随后`validate --as shangshu`原始EXIT0/256条，CLI原文与UTC见末两行及runs.json。 |

- 提交链实值：cf84e40（R75收口）→9da7f45→5aa1960（原背景漏列）→268b394→9bf6ec6→23cb621→41425b1→02c3ea8→b743c23→72fb0a4→ec6ec02→1eed4ac；96e46cc为R75规划起点，不是R75收口。
- 功能9bf6ec6实为5文件/4008增1603删；fix41425b1仅R59/29增14删。六份功能/测试/工具/文档SHA均与清单相符；旧R59保全由C3/T实证承接。
- 门禁实值：attempt-2.log=215,013B/85bdd3e6b5b725dd52d32ec9d0e43f038318f09c2a2208e7d674cc421afb1027；meta=802B/71c9ca8caad3b18512bd407ce4eac60137835eea8d1b7685282803069007cc67。
- 失败原件：attempt-1.log=69,152B/979cc747e1a9cea999ac26869bf340b28a5677a54a7fca46342b356895ef8e10；workspace内部带legacy且skip duo_/fault_pg，12项绿不等于无条件全测；meta的docs零差异仅当时快照，现后续仅文档。
- 清单=8,562B/8e91292ecaf24aec6b0f763db2e7400f7e5a42223d0aa1b3b149e16c800e7ca7；T终审32行/7,362B/2e57ec6fe2c66e7e4d362521775bb66b83558379c3a7bc47525e2d621781f30c，PASS-WITH-PARTIAL/必改0。
- 抽证命令1：`cargo test -p mir-client --features mir-render/legacy --test round76_trap_hexagon_mapping`；UTC 2026-10-28T01:24:03.502Z–01:24:38.974Z；原始EXIT0；8 passed/0 failed/0 ignored，外部实测35.472s。
- 抽证命令2：`cargo test -p mir-client --features mir-render/legacy --test round59_projectile_assets`；UTC 2026-10-28T01:24:38.976Z–01:24:40.552Z；原始EXIT0；6 passed/0 failed/0 ignored，外部实测1.576s。
- 隔离执行偏差：首tar以Windows绝对路径解包报`tar: Cannot connect to C: resolve failed`/EXIT128，未启动测试；原日志保留，改相对路径后EXIT0。两条测试无失败重跑。L2共14/0/0；为独立动态抽证不可降为静态。
- 残余流程PARTIAL：五inventory聚合不可复现；D未落盘自检证据（T及本次复核不追认D留证）；B86条早期日志模式等永久损失；A sidecar中间稿瑕疵；A R59首败日志覆盖损失；legacy clippy两lint/官方0双口径。
- 历史原号照承：GAP-1/2/R70-C2、GAP-3/4/6、GAP-5/R70-C1、R70-C3；R71时钟/对象/C7永久、R73-T1、R72 B plan-M4；历史E1/R69条件5、R68/R49–R70/R61–R67、R74 artifacts34、R75-E1/C-M1、R75-E2/C-M2。完整DrawBlend/rate/Mode、src×0.8、严格寿命/26技能及设备视觉未验收。
- 范围声明：只写本报告与target/round76/shangshu/attempt-1/**；未改交付物/实现/测试/素材/既有证据，未commit/push，未重跑全量ci.ps1；仅通过CLI执行授权收口。
- CLI done：`node C:/Users/diana/.dsh/skills/edict/scripts/edict.mjs done JJC-20261028-003 --as shangshu`；UTC 2026-10-28T01:27:02.849Z–01:27:02.918Z；原始EXIT0；原文“✅ JJC-20261028-003 执行完成，已提交尚书省审查”；读回Review，后续门下确认由Lead协调。
- CLI validate：`node C:/Users/diana/.dsh/skills/edict/scripts/edict.mjs validate --as shangshu`；UTC 2026-10-28T01:27:02.920Z–01:27:02.985Z；原始EXIT0；原文“✅ 看板校验通过：256 条任务符合 schema v3.1”。
