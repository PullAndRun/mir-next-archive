# 第74轮尚书省复验（attempt 2）

裁决：**ACCEPT-WITH-PARTIAL（必改 0；S-K 已解除）**；仅验收 R74 普通 NORMAL/rate1 CPU Add 跨 Magic/Magic2 测试锁切片。
主单 `JJC-20261028-001`；实审 HEAD=`d85bc2e755e2727d7b357148b69cb1e509fdb652`；本地 origin/main=`c3542c5fea27184d039379a94396928f4416b7a3`，二者本次不相等（新增首轮报告提交）。
窗口 UTC 2026-10-27T18:46:00Z–18:48:53Z；gpt-6.1-sol 独立复验。命令/UTC/原始 EXIT/日志 bytes/SHA 见 [runs](../../target/round74/shangshu/attempt-2/runs.json)，全文结果与独算见 [checks](../../target/round74/shangshu/attempt-2/checks.json)。

|复验项|结论|独立证据|
|---|---|---|
|S-K / D 收口|PASS|18:46:02.165–02.398Z `node …/edict.mjs show JJC-20261028-001-sub-05088f98 --json` EXIT0；Done、delegation_result 290字符非空；updatedAt=18:41:43Z。礼部真实承接者回执20行/2244B/SHA5830c987…，原 delegate-result EXIT0。|
|D 结果全文摘要|PASS|六文件及 attempt-1 自检交付；锚点笔误按裁定4披露（DXManager 实在 MirGraphics、Oracle.cs 不存在）；post-audit 34项/外置摘要一致；真实礼部补录且不改变交付物字节。全文保存在 show 日志及 checks，非本官代签。|
|父单 / readiness|PASS|五次 show 均原始 EXIT0；父 Doing、todos=[]、ready_to_close=false；list --all 仅列四个直属子单，A/B/C/D 均 Done、结果非空、todos=[]，派生收口齐备；随后 done 实际门禁通过。|
|独立 R74 抽证|PASS|`cargo test -p mir-client --features mir-render/legacy --test round74_normal_blend_lock`；18:46:02.648–18:47:56.022Z 原始 EXIT0，**5 passed / 0 failed / 0 ignored**。|
|fmt|PASS|`cargo fmt --all -- --check`；18:47:56.024–18:48:06.120Z 原始 EXIT0。|
|清单重新复算|PASS|34/34、mismatch=0；tracked 从 delivery_head=`ec49369b0228aa6aace9d99efa8e1ad2687cda53` git blob 取字节并核 blob ID；untracked 从工作树取字节；目录按排序 path:bytes LF inventory，32项/64773B。清单15045B/6ccfae09…、外置摘要吻合。|
|唯一门禁字节|PASS|log=213918B/SHA256 e2b1c25ac15d8207b0c786386fcb193279d9956c6f75eac770264784ef9771b3；meta=573B/4e622efe3f95b2094731790dae45b107d4c3499ac8c36ea88cccd430482a8c75；字段 mismatch=0，原门禁12 OK/0 FAIL、EXIT0。|
|交付后生产范围|PASS|`git diff ec49369..HEAD -- crates tools` 与 `git diff c222fe6..HEAD -- crates tools` 均原始 EXIT0/空；本次未重跑全量门禁。|
|诚实性 / 字节保全|PASS|首轮仍为 BLOCK，5294B/SHA488b56fc643d4d04bcaa648ef6f9613f6d0491caf0998e0b4ce44fc5e1710ad9；首轮及 T/C 原报告 git log 各仅一次提交；六文件 SHA/bytes 与首轮逐项一致，两历史 freeze bytes/SHA 未变。|
|CLI 收口|PASS|`node …/edict.mjs done JJC-20261028-001 --as shangshu` UTC18:48:52.566–52.969Z 原始 EXIT0；包装进程 EXIT0，无GBK报错；父单后续 show 原始 EXIT0，state=Review（未声称看板终态Done）。|
|CLI validate|PASS|`node …/edict.mjs validate --as shangshu` UTC18:48:52.975–53.146Z 原始 EXIT0，246条任务符合 schema v3.1。|

四子单：A=`JJC-20261028-001-sub-39860302`、B=`JJC-20261028-001-sub-82b02278`、C=`JJC-20261028-001-sub-c55440fc`、D=`JJC-20261028-001-sub-05088f98`；均 Done / delegation_result 非空。
必改：0。首轮 BLOCK 是当时有效裁决；本次基于礼部补录与独立复验解除 S-K，不回写、不升级首轮历史结论。
PARTIAL 原号承接：GAP-1/2、R70-C2、GAP-5/R70-C1（双记）、R70-C3、R71时钟/对象、R72 B plan-M4、R73-T1、GAP-3/4/6；R71-C7永久PARTIAL、E1仅补救CLOSED；R69条件5、R68/R49–R70/R61–R67照承。
LIMIT：完整 DrawBlend/BlendMode/DXManager、rate0.8/INVLIGHT设备状态、严格NextMotion/对象寿命、其他技能、真实素材/GPU/设备视觉/全UI/跨后端/Live音频仍PARTIAL；本裁决不构成整轮通过、严格Crystal等价或GPU/设备视觉签核。
隔离：CARGO_TARGET_DIR=`target/round74/shangshu/attempt-2/cargo-target`；TEMP=TMP=TMPDIR=`target/round74/shangshu/attempt-2/tmp`（绝对路径），CARGO_PROFILE_DEV_DEBUG=0。
探针披露：首次复算因目录尾斜杠重复拼接，探针 EXIT1/假 mismatch1；修正 inventory 路径后34/34 mismatch0，首次原始 checks 保存在 logs/audit-inventory-probe-first.json，交付物未变。R73/R72/spellfx/clippy复用首轮通过事实，未冒称本次重跑。
写入仅本报告与授权 attempt-2 证据目录；未改其他文件、未 commit/push、未运行 ci.ps1；全局看板仅执行题面授权 done 及只读命令。
