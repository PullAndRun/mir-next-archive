# R67 尚书省验收

**verdict：ACCEPT-WITH-PARTIAL；必改 0。** 仅验收测试私有提案与既有 UI 边界；不清零生产及历史缺口，不代门下确认。
任务 `JJC-20261027-004`；验收官 gpt-6.1-sol / shangshu；起始 UTC 2026-10-27T00:11:28Z；先落盘骨架再取证，15 分钟预算内。
证据根 `target/round67/shangshu/attempt-1/`；[静态命令/UTC/EXIT](../../target/round67/shangshu/attempt-1/static-runs.json)、[抽跑](../../target/round67/shangshu/attempt-1/test-runs.json)、[补核](../../target/round67/shangshu/attempt-1/supplemental-runs.json)、[逐项复算](../../target/round67/shangshu/attempt-1/recompute.json)保存完整命令、双流与真实退出码。
CLI 前缀 E=`node C:/Users/diana/.dsh/skills/edict/scripts/edict.mjs`；以下 UTC 日期均 2026-10-27，精确毫秒见 JSON。

|逐项|命令 / UTC / EXIT|独立结论|
|---|---|---|
|1 主单|E show JJC-20261027-004 --json；00:14:03.729–.786；0|Doing，todos=[]；ready_to_close=false 不作绕过依据|
|1 四子单|E show 主单-sub-{71e056ca,bd2cae45,0b916504,5c265347} --json，逐个实际执行；00:14:03.786–04.096；均0|均 Done，delegation_result 非空；A/B rev1、C BLOCK 为历史回执，以 rev2/addendum 覆盖当前裁决|
|1 全量子单|E list --all --json；00:16:15.761–.821；0|parent_task 精确筛出4项，无遗漏子单|
|2 提交链|git log --oneline 52d95b1..175421e；00:14:04.096–.141；0|78a8613→909a55c→b146dcb→175421e；功能→文档→审计→post-audit固化|
|2 HEAD|git rev-parse HEAD origin/main；00:14:04.353–.389；0|均175421ed5f7f4aec9c7a495e8b0a1ae1ed4f4e2c|
|2 门禁后范围|git diff 78a8613..175421e -- crates；00:14:04.141–.177；0|空；git diff 175421e -- crates（.177–.228；0）亦空|
|2 工作树|git status --porcelain；00:14:04.229–.273；0|两历史freeze JSON，加本次先落盘的验收报告；无其他脏项|
|2 tracked/属性|git ls-files target；00:14:04.273–.310；0；git diff 52d95b1..175421e -- '*.rs' \| grep -E '^\+.*#\[(ignore\|allow)'；00:16:15.242–.761；1|target tracked=0；grep EXIT1为空匹配，非检查通过码；独立 diff 扫描新增 ignore/allow=0|
|3 原门禁|node target/round67/shangshu/attempt-1/recompute.cjs；00:15:08.776–10.243；0|原日志207862B/meta285B；摘要末尾12 OK/0 FAIL；meta EXIT0、gate_head=78a8613、post_gate_crates_diff_lines=0|
|4 清单|同上；git cat-file blob HEAD:path / git rev-parse HEAD:path 逐项命令均0，精确UTC见复算JSON|15/15 worktree SHA/bytes匹配；13 tracked HEAD原blob SHA/bytes/id全匹配；mismatch=[]、自引用0|
|4 历史blob|逐项 git rev-parse commit:path；同上窗口；均0|audit 对b146dcb、docs对909a55c、三测试对78a8613均匹配；清单5507B，外置SHA匹配|
|5 R67|cargo test -p mir-client --test round67_position_policy --test round67_position_boundary；00:14:21.256–00:15:01.018；0|policy2+boundary2 passed，0 failed/ignored|
|5 R65|cargo test -p mir-client --test round65_projectile_lifecycle；00:15:01.020–02.931；0|5 passed/0 failed/0 ignored|
|5 R66|cargo test -p mir-client --test round66_consumer_boundary；00:15:02.931–04.806；0|2 passed/0 failed/0 ignored|
|5 fmt|cargo fmt -p mir-client -- --check；00:15:04.806–05.689；0|格式检查通过；抽跑合11互异测试，与C追加/T2相同；T2重复计数15次执行不作15互异|
|6 links|py -3 tools/docs/check_links.py；00:14:04.432–06.461；0|2115相对链接、2篇README引用与市场包编号一致；原stdout为CP936，专用解码读取，原字节保留|
|6 静态|E validate；00:14:06.462–.518；0；git diff --check；00:16:15.822–.863；0|均通过；校验不代替行为证据|
|7 诚实性|逐行读README/contract/C原报告/addendum/T2及R67称词检索；00:14–00:16Z；文件读取无CLI EXIT|未见生产LRU/限容、全局水位、125全链已证或整轮已通过正面扩称；阶段声明保留|
|9 板面流转|E done JJC-20261027-004 --as shangshu；00:17:30.479–.545；**EXIT0**|原输出“执行完成，已提交尚书省审查”；show复读00:17:30.546–.599/EXIT0确认Review；validate .600–.655/EXIT0、211任务；[回执](../../target/round67/shangshu/attempt-1/done-runs.json)；未执行最终确认|

门禁原字节 SHA256：log=`056255b32147ac506f52c42522b73d7f68b6641231b581d77f6a30e4ca54bc82`；meta=`75d0bf415cffce0869f257a08fe6547942f2da333fe41f13d6ead6307fe655aa`。
原门禁 UTC 2026-10-26T23:43:26–23:47:08Z；operator=lead；唯一 attempt-1，本验收未重跑全量门禁。
清单 SHA256=`c1fda924b3938340e444dfd8567dcc5a116e69d26244f52acdb70545ca28857a`；[外置摘要](../testing/round67/manifest.sha256)一致；audit原件3155B/32行，SHA=`bc59922bc48b7406ca3493ca7203acd1a9f3d9f222942b2e07daec0b779b0014`。
全新绝对 CARGO_TARGET_DIR 为仓库下 `target/round67/shangshu/attempt-1/cargo-target`，启动断言不存在；TMP=TEMP=TMPDIR 为同根 `tmp`，先建目录；[环境](../../target/round67/shangshu/attempt-1/environment.json)可复查。

披露裁定：D子代理在清单前失败，D四份文档产物与Lead机械补清单分开归属；D delegate-result及[lead-mapping:14](../../target/round67/lead-mapping.md#L14)已披露；generated_by明确Lead刷新。T首派无产物、T2精简终审见[映射:19](../../target/round67/lead-mapping.md#L19)，不把首派计成功。
README本身未写D/T失败，仍为pre-audit，末行“尚未跟踪/pre-audit清单”属历史措辞；当前清单已post-audit。此为非阻断文档时态/披露位置差异，以本报告与实际blob校正，不冒称README已完整同步，不改冻结上游件。
A/B rev1 superseded及B首个CRLF变异未命中/EXIT0不算红，README:44准确披露；C初轮BLOCK三项由owner rev2修复，addendum逐项CLOSED，历史原报告不覆盖。B旧回执“无PARTIAL”不采用，统一承接本报告边界。
PARTIAL：生产 known_positions 无界无淘汰；多消费者与生产注册/退出/租约/补发/fan-out、全局最低水位、125无点坐标、服务端回收、自动保护集、map epoch/同代ID复用、真实容量接线、素材/音频/设备/Live历史缺口均不清零；R49–R66原编号按README台账承接。
必改清单：空（无最小失败复现/责任方需要派回）；后续礼部可同步阶段措辞。生产能力缺口继续PARTIAL，不因11测试绿而闭合。
工具偏差：全库glob超时后限定目录；先写骨架的两次参数/模式校验错误无写入，第三次成功；链接stdout非UTF8读取失败后以CP936只读解码，原件未覆盖；均非产品/测试失败。
最终复查：UTC 00:18:03.983–04.752，py -3 tools/docs/check_links.py / git diff --check / git status --porcelain 均EXIT0；2124条链接、2篇README引用一致；仅本验收报告加两历史freeze未跟踪，UTF-8无BOM/LF、≤60行；命令见 `final-runs.json`，实测字节/SHA外置 `sha256.json`。
