# 第74轮门下省独立收口确认
裁决：**PASS-WITH-PARTIAL，收口成立；必改0；父单终态 Done**。仅确认普通 NORMAL/rate1 CPU Add 跨 Magic/Magic2 测试锁切片。
主单 `JJC-20261028-001`；实审 HEAD=`b6508230e6c40674b56a775657e86db9c452732e`；origin/main=`c3542c5fea27184d039379a94396928f4416b7a3`。
原始命令、UTC、EXIT及日志摘要见 [runs](../../target/round74/menxia/attempt-1/runs.json)，逐字节结果见 [checks](../../target/round74/menxia/attempt-1/checks.json)。

|确认项|结果|独立证据|
|---|---|---|
|1 看板|PASS|父单 Review、todos=[]；四子单逐一show，均Done/有效结果非空/todos=[]；list仅四直属子单；各show EXIT0。ready_to_close=false未替代真实门禁。|
|2 尚书两报告/D|PASS|首轮29行/5294B/SHA488b56fc643d4d04bcaa648ef6f9613f6d0491caf0998e0b4ce44fc5e1710ad9保留BLOCK；复验27行/4612B/SHAf9ad3a21dac3fa25e84dbe13d34c667d2b4a30830e7a124d2d7da43fdd6c9b3f，ACCEPT-WITH-PARTIAL/必改0/S-K解除；D结果290字符、回执20行/2244B/5830c987…，六文件SHA全吻合。|
|3 门禁|PASS|log213918B/e2b1c25ac15d8207b0c786386fcb193279d9956c6f75eac770264784ef9771b3；meta573B/4e622efe3f95b2094731790dae45b107d4c3499ac8c36ea88cccd430482a8c75；原门禁HEAD c222fe6/EXIT0/12 OK、0 FAIL。|
|3 清单|PASS|超额复算34/34，mismatch0；tracked取ec49369 git blob，untracked取工作树；目录32项/64773B/8f07549d…，按repo相对path:bytes排序、LF分隔、无末尾LF；清单15045B/6ccfae0967ec0b076052ecc3f75f127ef0af83ee7ae1ff632f014959d587e437，外置sha吻合。|
|3 保全/范围|PASS|ec49369..HEAD及c222fe6..HEAD的crates/tools差异均EXIT0/空；T/C git log各仅一次，原SHA60797b82…/ab7f9904…未变；C补证4008B/0c480d44…、T2 E1 CLOSED。|
|3 freeze|PASS|before44844165B/70845aefd4dd689a55ab0618b36bbf692a4b33ebb5ee2f3a662dedc17a3dc9de；after45219533B/c7d896e7f5eee4706b943912c0c564d10d61c98d163143f051c2d632daecde32，未变。|
|4 最小复跑|PASS|cargo test -p mir-client --features mir-render/legacy --test round74_normal_blend_lock：UTC18:55:26.435–18:57:46.386Z EXIT0，5 passed/0 failed/0 ignored；cargo fmt --all -- --check：18:57:46.387–49.950Z EXIT0。|
|5 文档诚实|PASS|实读README无正面验收完成/整轮通过；manifest stage=post-audit、audit明确T BLOCK历史与T2补救；候选/待办历史快照保留，禁称未正面出现。|
|6 CLI state|PASS|node …/edict.mjs state JJC-20261028-001 Done --as menxia：UTC19:00:17.918–18.009Z原始EXIT0，进入PendingConfirm；随后show EXIT0，confirm_by=menxia。|
|6 CLI confirm|PASS|node …/edict.mjs confirm JJC-20261028-001 approve --as menxia：UTC19:00:18.102–18.202Z原始EXIT0，Done。|
|6 CLI validate/show|PASS|validate --as menxia：UTC19:00:18.203–18.275Z原始EXIT0，246条schema v3.1；最终show：19:00:18.276–18.342Z原始EXIT0，state=Done。|
|7 工作树卫生|PASS|初始仅两历史freeze未跟踪；收尾仅本报告与两freeze未跟踪，证据目录被target忽略；git diff --check EXIT0，未改交付物。|

隔离：CARGO_TARGET_DIR=target/round74/menxia/attempt-1/cargo-target，TEMP=TMP=TMPDIR=同目录tmp（绝对路径）；CARGO_PROFILE_DEV_DEBUG=0。
探针披露：首目录探针误加末尾LF产生假mismatch；继而误用上次实测作期望，断言EXIT1发生在CLI之前；改为原清单期望后34/34吻合。首探针保留，未改交付物、未绕过CLI。
PARTIAL原号：GAP-1/2、R70-C2、GAP-5/R70-C1双记、R70-C3、R71时钟/对象、R72 B plan-M4、R73-T1、GAP-3/4/6；R71-C7永久PARTIAL、E1仅补救CLOSED；R69条件5、R68/R49–R70/R61–R67照承。
LIMIT：完整DrawBlend/BlendMode/DXManager、rate0.8/INVLIGHT设备状态、严格NextMotion/对象寿命、其他技能、真实素材/GPU/设备视觉/全UI/跨后端/Live音频继续PARTIAL；不构成整轮通过、严格Crystal等价或设备视觉签核。
本官仅写本报告与授权menxia/attempt-1目录；未commit/push，未运行ci.ps1；全局看板仅经授权CLI收口。
