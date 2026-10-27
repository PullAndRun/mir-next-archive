# R67-C 追加核验（rev2）
**verdict：PASS-WITH-PARTIAL；原 BLOCK 三项全部 CLOSED，新增必改 0。仅放行本次追加核验，不作整轮完成宣称。**
核验官：gpt-6.1-sol / 刑部；HEAD=origin/main=`78a8613b1b90ac2c66ce76b7c058311e514d9f27`；UTC 2026-10-26T23:47:37Z 起，结束见 final-checks.json，8 分钟内。
原 C 报告保全：67 行 / 8,709B / SHA256 `219835cf26c99084b7c704140143fb39c2f585e3cc5123184d2ee00f2d182a81`。
证据：[命令/UTC/EXIT/环境](../../../target/round67/c/addendum-1/runs.json)、[逐项静态核验](../../../target/round67/c/addendum-1/static-audit.json)、[审计输出](../../../target/round67/c/addendum-1/audit.stdout)；stdout/stderr 原件均存同目录。

|原必改|追加判定|证据|
|---|---|---|
|A clippy 4 条|CLOSED / PASS|policy:9/12/14 文档反引号、:30 `expected: &EvictionPlan` 已修；独立 clippy EXIT0。|
|B clippy 1 条|CLOSED / PASS|boundary:376–379 结构初始化替代默认后赋值；独立 clippy EXIT0。|
|B manifest 自引用|CLOSED / PASS|223 files；0 自引用、0 重复 path；独立全量223/223 SHA与bytes匹配（超过抽样30要求）。|

|逐项核验 / 命令|EXIT|计数 / 实值|
|---|---:|---|
|A rev2 SHA/尺寸|0|207行/7,279B，SHA `5f8b7b25d31bc2f24e734f7416bf96e677448274db5f2abeed60ebb72722c2d7`；support 88行/4,127B，SHA `80b88e624bd823fe046e7627386f526ab61e49822f7a7220bb7a21b2a927ab49` 未变。|
|B rev2 SHA/尺寸|0|428行/18,373B，SHA `5cbc4f02cff421c0c16d074e3ab260f87fa45d75e47f69bccb86228484f6b705`。|
|`cargo clippy -p mir-client --all-targets -- -D warnings`|0|0 warning / 0 error；UTC 23:49:30–23:50:13。|
|`cargo test -p mir-client --test round67_position_policy --test round67_position_boundary`|0|2+2 passed，0 failed/ignored；UTC 23:50:13–23:50:54。|
|`cargo fmt -p mir-client -- --check`|0|格式检查通过；UTC 23:50:54–23:50:55。|
|`cargo test -p mir-client --test round66_consumer_boundary --test round65_projectile_lifecycle`|0|R66=2、R65=5 passed，0 failed/ignored；UTC 23:50:55–23:50:58；合计11个测试通过。|
|`py -3 target/round67/c/addendum-1/audit.py`|0|manifest/7红SHA断言通过；完整 git 命令及各EXIT0见 static-audit。|
所有独立 cargo 共用全新绝对 `CARGO_TARGET_DIR=C:\Users\diana\Documents\github\mir-next\target\round67\c\addendum-1\cargo-target`；TMP/TEMP/TMPDIR 均为同根 `addendum-1\tmp`，脚本启动断言 target 原先不存在。
B manifest 原字节45,360B，SHA `b2a66b724d06f7bdc0df8c986105c56fb916b69a82f0eae77795310f14435d8d`，与外置 manifest.sha256 的 SHA/bytes 一致。
红 SHA：A attempt-1/2/3/4-rev2 共4份均等于当前 policy；B attempt-2/3/4-rev2 共3份均等于当前 boundary；核验副本原字节，无覆盖。
红首败（核原件，未重新运行变异）：A m4 rev2 policy:40 left0/right1；B m1 rev2 boundary:394 实际[(1,(19,10))]/期望空（rev1:392）；m2 :367 left(11,10)/right(19,10)；m3 :286 left[4]/right[1]；与rev1语义相同，均EXIT101、1passed/1failed/0ignored；A m1–m3 rev2亦EXIT101、0passed/2failed。
提交核验 PASS：`git show --stat HEAD` 与 `git diff --name-status 52d95b1..78a8613 -- crates` 均EXIT0，仅3个新增测试文件、+723；无src/protocol/旧测试改动；`git diff -- crates` EXIT0空。
门禁核验 PASS：Lead attempt-1 原日志207,862B / SHA `056255b32147ac506f52c42522b73d7f68b6641231b581d77f6a30e4ca54bc82`；末段12项OK、0FAIL；exit原件RC=0。
门禁 meta：HEAD=78a8613，EXIT0、12/12；UTC 23:43:26–23:47:08Z，post_gate_crates_diff_lines=0；本次独立git核验仍零diff。未重跑唯一门禁。
工作树核验 PASS：`git status --porcelain` EXIT0，仅process三件、testing/round67目录与两历史freeze；`git diff --check` EXIT0。
诚实性检查 PASS：检索并逐条审读三新测试及本轮报告的相关称词，未见“生产 LRU/限容已实现”“125 全链已证”“R67 通过”正称；否定与PARTIAL语境保留。
PARTIAL 不变：生产 known_positions 仍无界无淘汰；多消费者、生产注册/退出/租约/补发/fan-out、全局水位、自动保护集、map epoch/同代ID复用未闭；G1.3 仅测试历史模拟，非生产容量方案。原C历史编号/素材设备Live等缺口及D最终矩阵职责均不因本追加清零。
偏差披露：首轮静态审计已写JSON，但控制台GBK打印遇U+FFFD退出1；改为UTF8输出后同审计EXIT0。门禁日志含非UTF8字节，文字辅助解码有替换，大小/SHA始终按原始bytes核验；首个python命令不可用(EXIT49)，后续统一py -3。红证据只复核原件与SHA，未追加变异执行。
范围：仅写本文及 `target/round67/c/addendum-1/**`；未改实现/测试/原C报告，未commit/push，未运行 `tools/dev/ci.ps1`。
