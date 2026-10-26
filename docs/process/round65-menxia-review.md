# R65 门下独立审核：PASS-WITH-CHANGES

结论：**附条件批准（approve）**；G1准入，C1–C8须写入派单并由C复核；G2允许整项移出；wire锁定零变更。无当前BLOCK；本裁决不是实现PASS。
基线：HEAD=origin/main=`2deacee47efc9e505d034fe87e58692dfeb50187`；起始仅计划及两份历史资源冻结JSON未跟踪，受控文件无diff。
计划：初读102行/13,320B/SHA256 `35bf2d67e9156ba12c93baa4a5009ea39081019eb9d0a1f9c0c0ba427dbcfbc2`；审核中更新后重读108行/13,792B/`d5906954567a9fc3012258ab1b5a0d85b009886ad91bfea17876322155841fe5`，下述计划行号指后者。

|必核项|裁定|独立证据与含义|
|---|---|---|
|1 Crystal真实性|PASS|下表逐条打开，无缺文件/错行；参考实现不外推全局策略。|
|2 已有/新增与A-M3|PASS|计划18–19明确R64已有；30固定三状态、5次出生cue和重复drain=0；R64终审25/42仅组合覆盖PARTIAL。|
|3 单轮范围/DAG|PASS-WITH-CHANGES|计划26–37/65–72可在mir-client测试/契约闭环；A→B→C→Lead门禁→D串行，M1锁单owner。|
|4 真红|PASS-WITH-CHANGES|计划76–81明确桩视为修改、隔离、同测SHA、无批准不完整；本审准限定负控，C6补内嵌测试SHA。|
|5 门下裁决|PASS-WITH-CHANGES|G1必做、G2可移出、隔离负控获准；生产逻辑修复须另报。|
|6 风险/不做|PASS/PARTIAL|计划103–108无容量常数/多消费者支持宣称；容量/全局水位/服务端回收仍PARTIAL，禁区C5。|

|Crystal锚点（均以github/Crystal/为根）|结果及实读内容|
|---|---|
|Client/MirObjects/Effect.cs:144,191–195|PASS：静态Missiles表，155/164构造加入、194 Remove(this)，仅活动生命周期。|
|Client/MirScenes/GameScene.cs:4687|PASS：4689查源、4691缺源return；4697出生音，4701–4709目标存在且未死才命中特效/音。|
|Client/MirObjects/PlayerObject.cs:750|PASS：`SkipFrames = this != User && ActionFeed.Count > 0`。|
|Client/MirObjects/PlayerObject.cs:966–967|PASS：ActionFeed[0]/RemoveAt(0)，FIFO。|
|Client/MirObjects/MonsterObject.cs:346|PASS：`SkipFrames = ActionFeed.Count > 1`。|
|Client/MirObjects/MonsterObject.cs:528–529|PASS：ActionFeed[0]/RemoveAt(0)，FIFO。|
|Client/MirObjects/PlayerObject.cs:3546|PASS：查目标；3548默认源TargetPoint、3550当前目标位置优先；无容量/LRU依据。|
|Shared/ServerPackets.cs:3564|PASS：3564–3566仅Spell/Source/Destination，3568–3578读写，无ack/序号。|
|Server/MirEnvir/Map.cs:2375|PASS：RemoveObject移除分类表/cell对象，非125回收确认协议。|

现状实读：`crates/mir-client/src/net.rs:3030/3046/3070`为reclaimed/前缀drain/generation校验；`crates/mir-client/src/ui/projectile.rs:602/715–761`已有绝对游标/后缀消费；`crates/mir-client/src/app.rs:210/2796–2808`已有同步后本地ack。
入口：`crates/mir-client/src/app.rs:2679`为UI接收；`crates/mir-client/src/net.rs:3869–3897`另从同一Receiver等待predicate、不ack；`crates/mir-client/src/net.rs:4300–4302/4592–4604`确认InGame gate后、send前返回。

- **C1 精确半消费**：B保留S2→权威S4→ack2→push5→消费后缀→ack5；断言(reclaimed,len,total)=(2,2,4)/(2,3,5)/(5,0,5)，区分e1–e5身份/顺序、累计出生cue=5、重复sync/drain新增0；冻结时刻/目标距离，避免混入命中cue。
- **C2 隔离修改批准**：批准A/B仅在各自`target/round65/<owner>/`源码副本中，将trim_projectile_events_through改no-op、next_projectile_ack改返回None，以及trim中保持移除数量但删错前缀的负控；仅上述函数体，禁止覆盖生产树，记录补丁/副本SHA。C独立复核，不借此改生产实现。
- **C3 clone/计数边界**：裁前S4克隆逐事件不变；clone不注册、不钉住权威回收；第二UI仅离线对照。水位为已消费条数（含本条），零基events[i]对应事件序号reclaimed+i+1；旧注释reclaimed+i仅可称零基偏移，不混称水位。饱和只证不溢出，恢复/唯一序号/全局安全仍PARTIAL。
- **C4 同步等待单列**：覆盖wait_for_snapshot的predicate不满足继续等、满足返回且不自动ack；不登记全局最低水位成员，不称全仓只有一个Snapshot接收点。app宿主至少1个r65_命中，保留真实TCP ack零字节＋Turn有字节正对照。
- **C5 禁区/阶段**：禁止server语义/回收协议、wire字段/编解码、运行时容量/LRU/常数、生产SkipFrames/动作FIFO、掉落/护身符、素材/设备/Live、570/550扩面；禁称已限容/有界/多消费者已支持/全链已闭。限定单UI契约，历史PARTIAL不清零。
- **C6 真红规则**：编译错、0 tests、旧失败、任意panic/空泛assert不算红；必须预期assert失败、非零EXIT、名称/行号/左右值。独立测试文件红绿全字节SHA相同；net/app内嵌测试提取完整测试块逐字节同SHA，保全范围/diff，另记宿主红绿各自SHA，禁止称桩修改后宿主整文件同SHA。负控下目标测试若绿则不合格；未获批准或未取有效红则PARTIAL。
- **C7 生命周期**：重复/后退/越尾、旧代拒绝、新代空首批延迟复位/首批非空仅一次清views/570/cues/cache与水位均须行为断言；现有功能只补回归，不包装新策略。真实缺陷先报门下最小修复，不能默扩白名单。
- **C8 独立证据/门禁**：A/B/C独立CARGO_TARGET_DIR及TMP/TEMP/TMPDIR；命令/HEAD/UTC/原始EXIT/输出/源码/测试SHA齐备，过滤发现并通过≥1，SKIP→PARTIAL。仅Lead跑门禁，之后crates含测试/注释变更均重门禁，不预写PASS。

必须修正/派发保留（尚书落实后启动对应子单，C逐条销项）：
- **M1 白名单/owner**：A唯一owner写net/app现有测试/注释；B仅新round65_projectile_lifecycle测试；C仅自身证据根/核验报告；D仅计划72行三文件。兵部/工部二选一落实真实子单/agent，禁同文件双owner；副本按C2精确登记。
- **M2 SHA/序号**：派单带C3/C6，补测试块提取方案和红绿文件身份映射，消除整宿主同SHA矛盾；不得事后改断言迁就负控。
- **M3 target/观测**：核lib/bin/integration实际宿主/名称及r64/r65过滤计数；A保留next_projectile_ack宿主，B列五事件身份/顺序/cue/drain预期，C独立target复跑批准清单。
- **M4 D只读锁定材料**：R64计划/门下/README/终审及冻结JSON只读；roadmap/client默认只读。R61 P01–P12、R62四项、R63 P1–P7、R64六项原编号逐项承接；历史阶段BLOCK不冒充当前终态。
- **M5 G2**：允许整项移出，D明确本轮未做；保留无上限/增长风险、未确认backlog不丢最旧；未来配置/淘汰/换图/ID复用策略仅待评审PARTIAL。
- **M6 门禁/归属**：A→B→C→Lead功能提交/门禁→D；本审核者不提交。尚书仅暂存白名单，历史JSON/资源/target不混入；保全首败日志，D按实际功能HEAD及门禁事实记载。

边界：本审仅写本文并执行指定edict一次state流转；未改crates/计划/其他仓库文件，未commit/push，未运行cargo或ci.ps1。只读元数据探针曾语法错误exit1，后用只读SHA/字节命令复算，不计功能证据。
流转回执：指定state命令（含用户原定summary）exit0，`✅ JJC-20261027-002 状态更新：Menxia → Assigned`；随后validate exit0，`✅ 看板校验通过：197 条任务符合 schema v3.1`。Assigned仅表示上述附条件准入。
