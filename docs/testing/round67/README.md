# R67 位置缓存容量与保护引用契约

阶段：**pre-audit**。主单 `JJC-20261027-004` Doing；礼部子单 `JJC-20261027-004-sub-5c265347`。功能提交 `78a8613b1b90ac2c66ce76b7c058311e514d9f27` 已推送，3 个新增测试文件、+723 行。
**门禁 PASS 与 C addendum PASS 不代表最终验收/收口。**

## 范围与五行最终矩阵（G1.5 / M6）

|范围|本轮证据与结论|保留边界|
|---|---|---|
|生产无界缓存|[生产实现](../../../crates/mir-client/src/ui/projectile.rs#L579)仍为 known_positions HashMap；可见刷新、换代 clear|未接容量、LRU、硬上限；生产逻辑/wire/server 本轮零改动|
|参考候选|G1.1/G1.3；显式预算/ticks，候选历史预热后真实 UI 差分 4→1|仅测试私有提案；G1.3 同代0 default夹具、历史筛选非 wire，非运行时淘汰|
|保护超额|G1.2 全保护 remaining3/over1；B m3只在隔离support忽略ID1，真实UI [4]≠[1]|允许保护超额；非自动保护集；保留[7,4]是算法推导，实际view由assert核验|
|未知/换代|G1.3 未见999不生成；G1.4 刷新19→隐藏回退19，新代旧cache清空|隐藏实体与generation赋值是非wire夹具；71/125真编解码；交换优先级可能被刷新/advance覆盖而幸存，不计有效红，替代为禁可见刷新红|
|map/ID复用|PARTIAL|map epoch、同代ID复用、多消费者及全局最低水位未做|

## 四个测试（实际名称）

|文件|测试名 / G1|断言要点|
|---|---|---|
|[policy](../../../crates/mir-client/tests/round67_position_policy.rs#L48)|`r67_eviction_order_and_budget` / G1.1|保护10预算2→evict[20,30]/remaining2/over0；反序相同；并列tick按id|
|[policy](../../../crates/mir-client/tests/round67_position_policy.rs#L99)|`r67_protected_overflow_zero_and_duplicates` / G1.2|全保护超额；budget0/≥len；未知/重复保护；重复entry Err且输入不变|
|[boundary](../../../crates/mir-client/tests/round67_position_boundary.rs#L185)|`r67_historical_preheat_candidate_vs_full` / G1.3|71预热源7/目标1..4；125目标[1,2,3,4,999]；完整四身份四坐标/cue4，候选[1]/(11,10)/cue1；两支consumed5，重复新增0|
|[boundary](../../../crates/mir-client/tests/round67_position_boundary.rs#L331)|`r67_cache_refresh_and_generation_clear_protection` / G1.4|11→可见19→隐藏→125落点19；新代view/cue0、游标1；本代13/cue1，重复0|

测试私有 [support](../../../crates/mir-client/tests/support/round67_positions.rs) 与完整[契约](round67-contract.md)配套；cue 只证请求，未证设备实际可听；125 wire 仍只有 Spell/Source/Destination，无点坐标，不称全链闭环。

## 红证据（rev2，七份有效实现变异）

P = policy 测试 SHA256 `5f8b7b25d31bc2f24e734f7416bf96e677448274db5f2abeed60ebb72722c2d7`。
B = boundary 测试 SHA256 `5cbc4f02cff421c0c16d074e3ab260f87fa45d75e47f69bccb86228484f6b705`。
下表 SHA 列逐行引用上述完整值；均与对应绿文件相同；首败位置按 rev2 原件，EXIT101。

|变异 / 实现位置|同测试SHA|首败左右实值 / 位置|原件|
|---|---|---|---|
|A m1 support排序降序|P|[40,30] ≠ [20,30]；policy:32|[stderr](../../../target/round67/a/attempt-1/red/m1/rev2/stderr)|
|A m2 support忽略保护|P|[10,20] ≠ [20,30]；policy:32|[stderr](../../../target/round67/a/attempt-1/red/m2/rev2/stderr)|
|A m3 support预算比较反转|P|[] ≠ [20,30]；policy:32|[stderr](../../../target/round67/a/attempt-1/red/m3/rev2/stderr)|
|A m4 support over_budget反转|P|0 ≠ 1；policy:40；仅G1.2失败，G1.1绿|[stderr](../../../target/round67/a/attempt-1/red/m4/rev2/stderr)|
|B m1 projectile reset删clear|B|实际[(1,(19,10))] ≠ 空；boundary:394|[首败](../../../target/round67/b/attempt-1/red/m1-rev2/first-failure.txt)|
|B m2 projectile可见insert改or_insert|B|(11,10) ≠ (19,10)；boundary:367|[首败](../../../target/round67/b/attempt-1/red/m2-rev2/first-failure.txt)|
|B m3 support保护判断忽略ID1|B|[4] ≠ [1]；boundary:286（真实UI）|[首败](../../../target/round67/b/attempt-1/red/m3-rev2/first-failure.txt)|

A m1–m3各0 passed/2 failed；A m4与B三红各1 passed/1 failed，ignored0。A四份 attempt-1..4-rev2，B三份 attempt-2..4-rev2；绝对 cwd/manifest/target/TMP三变量、编译来源及无硬链接证据见 [A索引](../../../target/round67/a/attempt-1/commands.json)、[B索引](../../../target/round67/b/attempt-1/commands.json)。
rev1 测试 SHA（A `b3953cb5…`、B `229facde…`）与红已被 rev2 superseded，原件保留；B最初CRLF变异未生效、EXIT0，不计红。A排列/并列及部分G1.2支路只有绿覆盖，不声称每支均有变异红。C独立红为另一常量0变异，保留于原报告，不混作上述七份。

## 绿、静态与 C 核验时间线

|记录|实际结果 / 证据|
|---|---|
|A rev2 policy|`cargo test -p mir-client --test round67_position_policy` EXIT0，2 passed/0 failed/0 ignored；UTC 23:29:56–23:29:59Z|
|B rev2 boundary|`cargo test -p mir-client --test round67_position_boundary` EXIT0，2 passed/0 failed/0 ignored；UTC 23:27:48–23:27:52Z|
|静态|A rev2 all-targets clippy -D warnings EXIT0、fmt EXIT0；C追加独立复跑同绿|
|C首次（23:22:38–23:26:02Z）|[历史报告](round67-c-evidence.md) BLOCK：A lint4、B lint1、B manifest自引用；原文67行/8709B未改；9组28 passed，补2组29次执行，合57次执行不称互异57测试|
|owner rev2修复|A反引号/借用期望值；B结构初始化、manifest去自引用；红绿随新SHA重跑，旧原件保留|
|C追加（23:47:37Z起）|[addendum](round67-c-addendum.md) PASS-WITH-PARTIAL，三项全CLOSED、新必改0；clippy 23:49:30–23:50:13 EXIT0，R67 2+2、fmt、R66 consumer2+R65 lifecycle5全绿；B manifest223/223匹配、0自引用/重复；7红SHA复核|
|D文档自检|实际命令、EXIT、工作树白名单与逐项SHA/bytes见[自检](../../../target/round67/d/attempt-1/checks.json)，不另跑全量门禁|

日期均为 2026-10-26 UTC；完整命令及精确时间以原件为准。C追加为独立复跑；D只核原件与文档，不把历史命令冒称本次执行。

## 唯一 Lead 门禁

HEAD=`78a8613b1b90ac2c66ce76b7c058311e514d9f27`；attempt-1 **EXIT=0，12/12 OK，0 FAIL**；UTC `2026-10-26T23:43:26Z`–`2026-10-26T23:47:08Z`；operator=lead，post_gate_crates_diff_lines=0。
[日志](../../../target/round67/gate/attempt-1.log)：207862B；SHA256 `056255b32147ac506f52c42522b73d7f68b6641231b581d77f6a30e4ca54bc82`。
[meta](../../../target/round67/gate/attempt-1.meta.txt)：285B；SHA256 `75d0bf415cffce0869f257a08fe6547942f2da333fe41f13d6ead6307fe655aa`。D未运行门禁；门禁后crates变更需Lead新提交/新attempt。

## PARTIAL 原编号台账与 R68 候选

|原编号|承接项（本轮不清零）|
|---|---|
|R49–R60 / R60 P-1–P-6|声效、素材、设备等历史缺口保留|
|R61 P01–P04|生产多消费者/长期内存；SkipFrames/ActionFeed；全局TargetPoint、容量/LRU/失效；ImageInfo.offset/渲染扩面|
|R61 P05–P09|设备/Live、33路peak=22.844788削波；105 sfx；HiddenSpanSource全局化；CWD/来源|
|R61 P10–P12|u64::MAX恢复；缺素材SKIP继续PARTIAL；R60原项|
|R62①–④|服务端物理回收；位置容量；生产owner_skip_frames=false；局部570不外推|
|R63 P1–P7|原项逐项保留，含旧gate meta缺EXIT/HEAD；本轮门禁完整不补写旧证据|
|R64①–⑥|全局最低水位；事件容量；位置容量；服务端回收；定向≠门禁；历史编号|
|R65 G1.2 / G2|仅clone值独立 / G2未做|
|R66|生产注册/退出/租约/补发/fan-out/全局水位仍未实现|
|R67 / R68候选|生产known_positions无界无淘汰；自动保护集、map epoch/同代ID复用、容量实际接线、多消费者与全局最低水位生产侧、服务端回收、素材/音频另轮评审|

[计划](../../process/round67-plan.md)、[门下条件C1–C10/M1–M6](../../process/round67-menxia-review.md)、[派发](../../process/round67-shangshu-dispatch.md)三件尚未跟踪、随本批交Lead；本次直接指令授权契约名采用 round67-contract、更新client/roadmap，取代旧派发只读范围；旧文档不回写。完整工作树摘要及git blob见[pre-audit清单](final-manifest.json)；清单不含自身，外置[自身摘要](manifest.sha256)。target原件、历史freeze与大文件不登记tracked、不暂存；尚书/门下最终验收仍待履行。
