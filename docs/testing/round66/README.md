# R66 交付说明（待审计）

阶段 `pre-audit`；限定完成（测试/契约层）。门禁 PASS ≠ 最终验收。
A/B 自报已由 C 独立复核，最终必改0；仍待独立终审与尚书验收。
本轮只补本地契约、参考模型和消费边界反例；生产多消费者、全局水位、
服务端回收、并发/fan-out、容量/LRU均未接线，继续 PARTIAL。

依据：[计划](../../process/round66-plan.md)、
[门下](../../process/round66-menxia-review.md)、
[尚书派发](../../process/round66-shangshu-dispatch.md)。
契约见[水位说明](round66-watermark.md)，实数和SHA见
[机器清单](final-manifest.json)；清单自身SHA另记D回执，避免循环。
名称偏差：本次直接指令使用水位说明文件名；旧计划中的contract名未创建。
既有roadmap与历史文档保持冻结，本页承接R67候选。

|交付|最终内容与事实源|限制|
|---|---|---|
|A|[参考函数][support]、[契约测试][contract]，3项|仅测试私有|
|B|[边界测试][boundary]2项、[waiter][waiter]1项|net仅cfg(test)+138|
|C|[独立证据][c]，最终PASS-WITH-PARTIAL、必改0|旧FAIL表为修复前快照|
|D|本说明、水位契约、机器清单|待审计；不替代验收|

[support]: ../../../crates/mir-client/tests/support/round66_watermark.rs
[contract]: ../../../crates/mir-client/tests/round66_watermark_contract.rs
[boundary]: ../../../crates/mir-client/tests/round66_consumer_boundary.rs
[waiter]: ../../../crates/mir-client/src/net.rs#L9850
[c]: round66-c-evidence.md

A support最终75行/3811B，A契约555行/21480B；B边界501行/20922B；
net9917行/404857B；C证据61行/8786B。逐项SHA与HEAD blob在机器清单。
C首次发现F1：逐成员先返回违反全集合优先级，owner已修复并补交叉断言。
最终顺序（全集合逐阶段，与成员排列无关）：
`InvalidBounds → NoConsumers → GenerationMismatch → BelowReclaimed`
`→ BeyondTail → min`。五Err均不得裁剪真实表；floor0/慢0阻止回收。
最终C [attempt-3][c3]重跑A3/B2/lib1及fmt/clippy，均EXIT0；
补充完整回归清单为58项，旧57项表不冒充最终计数。

[c3]: ../../../target/round66/c/attempt-3/runs.json

G1.2使用真实71/125流：源7/目标1..5，慢读S2水位2、快读S4水位4。
模型min2交真实ack→`(reclaimed,len,total)=(2,2,4)`、后缀[3,4]；
追加e5→`(2,3,5)`、后缀[3,4,5]。慢UI新增身份[3,4,5]、cue3，
重复sync新增view/cue均0。时钟冻结1000ms；先drain初始cue、记view基数。
G1.3独立同流现状分支：快ack4→`(4,0,4)`，push5→`(4,1,5)`；
慢UI新增[5]/cue1，水位却到5，漏e3/e4。安全分支新增3对现状1，
旧S4 clone仍含1..4，只能证明旧值独立，不能挽回权威历史。
G1.4低于floor/旧代/空集裁剪调用0；G1.5同Receiver取走S1/S2后
返回S2、try_recv=None、command_rx=Empty；G1.6依矩阵与生产范围检查。

唯一门禁由Lead执行 `tools/dev/ci.ps1 -KillStale`，原始EXIT=0，12/12 OK。
功能HEAD（非文档提交）：
`b99874211c906cfe9d0a75f03dabd5050863547e`。
规划基线：`f14a81dc4a1f141dd37d7229d3f623a07e9a0df5`。
UTC：2026-10-26T21:48:08Z 至 2026-10-26T21:51:46Z。
[原始log][gate-log] 213139B，SHA256：
`1d04184a0de71f36f454229177a237847912507a34d3ac9b8bdab7a94891d524`。
[meta][gate-meta] 454B，SHA256：
`6bcd9d4f4aff097e6cb8c94eedf68f9d896f434827bbc18f5e36c69ecc93024e`。
operator=lead；meta记录post_gate_crates_diff=none，D再次检查差异为空。
任何门禁后crates变化均使该门禁失效，须Lead新功能提交和新attempt。

[gate-log]: ../../../target/round66/gate/attempt-1.log
[gate-meta]: ../../../target/round66/gate/attempt-1.meta.txt

|门禁项|结果|
|---|---|
|cargo fmt --all --check|OK|
|cargo clippy --workspace --all-targets -D warnings|OK|
|mir-server duo_smoke（串行）|OK|
|cargo test --workspace|OK|
|协议生成器 --check|OK|
|文档链接与包编号|OK|
|TextKey完整性|OK|
|硬编码棘轮player-visible（阈值70）|OK|
|硬编码棘轮no-test（阈值0）|OK|
|运行时不引用老资源格式|OK|
|物品图标完整性|OK|
|PostgreSQL集成测试mir-store（串行）|OK|

真红来自隔离变异，目标断言实际执行、原始EXIT101；0 tests、编译错误、
skip或既有失败均不计有效红。补丁、源码/测试SHA、UTC、双流、首败见
[A证据][a-root]、[B证据][b-root]与C独立复跑记录，不在工作树应用变异。

|变异|目标首败（left/right）|
|---|---|
|A m1 min→max|契约:252，Ok(4)/Ok(2)|
|A m2 空集→tail|契约:300，None/Some(NoConsumers)|
|A m3 忽略代号|契约:513，None/Some(GenerationMismatch)|
|A m4 只查首成员下界|契约:407，Some(BeyondTail)/Some(BelowReclaimed)|
|B m1 min→max|边界:190，suffix 0/2，真实表断言|
|B m2/m2b 错前缀保len|边界:205，事件身份1/3|
|B m3 waiter注ack|变异net:9916，command_rx非Empty（布尔断言）|

[a-root]: ../../../target/round66/a/attempt-1/README.md
[b-root]: ../../../target/round66/b/attempt-1/README.md

B证据清单SHA（selfsha内容所指向的是sha256.txt本体）：
`86d29f051e1fa739f69dbf41fafcdda64f0736adb4f77b6bc004e0f8069d282f`。
selfsha文件自身另有SHA，机器清单分别列两项，避免混淆。

待终审偏差：A证据自述旧v1及m1–m3补丁被v2覆盖，不能声称A旧首败
永久保全；C initial/attempt-2/attempt-3独立记录仍在，此偏差交终审裁定。
A旧报告将R65失败归因外部net变化；B最终报告披露硬链接共享target导致
旧变异二进制复用，clean重编译后记录绿。以最终C复核及功能HEAD门禁
为最终运行证据，不将旧归因外推为生产缺陷。D未覆写上游历史记录。

PARTIAL台账按原轮原编号承接，模型不清零任何整项：

|原编号|继续保留的边界|
|---|---|
|R49–R60 / R60 P-1–P-6|按原台账；场景/掉落/护身符/素材/音频未外推|
|R61 P01|生产多消费者、长期内存；仅补本地min提案与反例|
|R61 P02|生产SkipFrames/ActionFeed|
|R61 P03|全局TargetPoint、容量与失效|
|R61 P04|ImageInfo/渲染扩面|
|R61 P05–P06|设备/Live；33路peak=22.844788削波|
|R61 P07|105 sfx逐项|
|R61 P08|HiddenSpanSource全局化|
|R61 P09|CWD/来源|
|R61 P10|u64::MAX饱和恢复，不设计回绕|
|R61 P11|缺素材SKIP|
|R61 P12|历史R60 P-1–P-6|
|R62 ①–④|服务端回收；位置容量/LRU；owner_skip_frames=false；局部570|
|R63 P1|生产动作来源|
|R63 P2|事件限容、服务端及全局回收|
|R63 P3|位置容量|
|R63 P4|真实素材可听/渲染|
|R63 P5|局部严格时钟/R61交接|
|R63 P6|历史阶段与当前终态区分|
|R63 P7|旧gate meta缺EXIT/HEAD|
|R64 ①–③|多消费者/clone最低水位；事件容量；位置容量/LRU；|
|R64 ④–⑥续|服务端回收；定向与唯一门禁区分；历史编号延续|
|R65 G1.2 / G2|clone仅值独立；G2未做，全局最低水位缺口仍在|

R67候选：生产多消费者/全局最低水位、事件表容量与位置LRU、
服务端回收、素材/音频，以及R61十二项；须另选目标与可失败验收。
D自检记录见[run-checks][d-checks]；机器清单按功能HEAD的tracked状态
逐项标注blob或untracked，SHA以指定来源字节计算；未对清单自身做自引用。

[d-checks]: ../../../target/round66/d/attempt-1/run-checks.json
