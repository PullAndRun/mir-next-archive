# R66 门下独立审核（attempt-1）

- 主单：JJC-20261027-003；审核官：gpt-6.1-sol / menxia；裁决：**PASS，必改 0**。
- 审核对象为规划可执行性，未执行实现/测试；始于2026-10-26T20:43:13Z，终检UTC另见证据（12分钟预算内）。
- 基线实读：f14a81dc4a1f141dd37d7229d3f623a07e9a0df5；R65 主单实读 Done。
- 计划复算：134 行 / 18,915 B / UTF-8 无 BOM / LF=134、CR=0、末尾 LF。
- 计划 SHA256：`8a3dbbc66812f0743188b8407eee28eea02515de8bf19ae58c0b0467042a215c`；与 planning-receipt.json 全部一致。
- 看板 now 留有“133行”历史说明；回执已说明，最终文件实测 134 行，不构成元数据掩盖。

|审核项|判定|可失败性、依据与限制|
|---|---|---|
|G1.1 参考函数|PASS|[4,2]→2、[4,0]→0、单4/重复2、五种 Err 与 Err 不裁剪均明确；min→max、空集→tail 分别须红，不能仅凭 helper 常数追绿。|
|G1.2 安全后缀|PASS|真实71/125→apply_ack：先(2,2,4)、追加后(2,3,5)；新增目标[3,4,5]/cue3/重复0；min→max 须败在真实表/身份/cue断言。|
|G1.3 现状反例|PASS|快 ack4→裁空→push5，慢UI仅新增[5]/cue1、水位5；与安全分支差分且旧S4 clone含1..4；错前缀保长变异须由G1.2身份断言抓住。|
|G1.4 代与晚加入|PASS|floor2/tail4：低水位、旧代、空集三 Err，裁剪计数0/表不变；合法2/4→2；忽略代/接纳低水位须触发断言。|
|G1.5 等待器|PASS|预送S1/S2，predicate total>=2返回S2，随后try_recv=None且command_rx=Empty；提前返S1/注入ack均可被断言抓住；执行者应使用有限timeout。|
|G1.6 矩阵/零行为|PASS|四行均有事实边界与测试/源码落点；范围检查可因生产改动/漏行/虚假PASS失败，此项明确不是运行时红。|
|有效红证据|PASS|隔离副本最小变异、same-test SHA、首败永久保全、UTC/原始EXIT/实值/双流/补丁与源码SHA均明确；0 tests、编译失败、skip、既有失败不算红。|
|Crystal 事实|PASS|全部指定锚点逐条打开，实际内容见下；模型明确为本项目提案，未借原作冒称全局水位。|
|边界/兼容|PASS|单app UI、旧clone、同Receiver waiter、未来显式消费者四行够覆盖本轮；生产注册/退出/租约/fan-out、容量/LRU、server回收、饱和恢复仍PARTIAL。|
|隔离/卫生/职责|PASS|support仅integration-test引用，net增量只cfg(test)，生产逻辑/wire/server零变更；禁新ignore/allow；A→B→C→D串行、deepseek编码、gpt核验文档、Lead唯一功能提交/门禁。|
|元数据/工作树|PASS|git diff --exit-code -- crates EXIT0且空；索引空；两既有未跟踪freeze JSON按SHA保全，不暂存；未改计划。|

Crystal 实读关键内容（完整摘录与各源文件SHA见证据）：
- [Effect.cs:144](../../github/Crystal/Client/MirObjects/Effect.cs#L144)：`public static List<Missile> Missiles = new List<Missile>();`；155构造Add；191–195 Remove先base.Remove再`Missiles.Remove(this)`，仅活动列表。
- [GameScene.cs:4687–4714](../../github/Crystal/Client/MirScenes/GameScene.cs#L4687)：4689找源，4691空源return；4697出生音，4699创建410；4701目标非空挂Complete，4707死亡return，4708创建570、4709命中音。
- [PlayerObject.cs:750](../../github/Crystal/Client/MirObjects/PlayerObject.cs#L750)：`SkipFrames = this != User && ActionFeed.Count > 0;`；966–967取`ActionFeed[0]`后RemoveAt(0)，FIFO。
- [PlayerObject.cs:3546–3556](../../github/Crystal/Client/MirObjects/PlayerObject.cs#L3546)：找对象，3548以TargetPoint为回退，3550非空改用CurrentLocation；没有缓存容量/LRU。
- [MonsterObject.cs:346](../../github/Crystal/Client/MirObjects/MonsterObject.cs#L346)：`SkipFrames = ActionFeed.Count > 1;`；528–529取[0]并RemoveAt(0)，FIFO。
- [ServerPackets.cs:3564–3578](../../github/Crystal/Shared/ServerPackets.cs#L3564)：Spell/uint Source/uint Destination；ReadByte+两ReadUInt32、Write(byte Spell)+两uint，无ack/序号。
- [Map.cs:2375–2383](../../github/Crystal/Server/MirEnvir/Map.cs#L2375)：RemoveObject按Race移除Players/NPCs/Spells/Heroes，最后`GetCell(ob.CurrentLocation).Remove(ob)`；仅地图对象移除，不能当投射物确认/服务端事件回收。
- 当前net.rs:3030–3074支持reclaimed与校代后裁前缀；3853–3895 waiter/try_recv共用Receiver；ui/projectile.rs:748–761直接将游标推进到dropped，足以产生计划中的漏历史反例。

执行条件（重申既有验收约束，不新增实施范围）：
- C1：A/B逐项保全有效红和对应绿；G1.2 min→max必须运行到真实suffix/事件身份/cue断言；C独立复跑核同测试SHA、发现数>0、无skip。
- C2：冻结now_ms=1000，初始cue先drain并记录view基数；对新增view断言顺序[3,4,5]对[5]、出生cue3对1，禁止只断言水位5。
- C3：尚书派发前重核HEAD与白名单；C核生产零diff、新增ignore/allow为0、四行矩阵齐全；错误退原owner，不由C修实现。
- C4：Lead唯一执行ci.ps1并记录门禁HEAD/原始EXIT；任何门禁后crates变更须新提交/新attempt；D只据真实证据落盘，全部历史PARTIAL按原编号承接。
- M项：**无（0）**。无反对项、无整改前置；可选加强G1.1多重非法输入的优先级组合断言，但不扩本轮主目标。
- 未执行cargo、ci.ps1、commit/push、done或子单创建；本轮审核PASS不代表实现/门禁/最终验收PASS。

流转与证据：
- `state JJC-20261027-003 Assigned --as menxia`：2026-10-26T20:47:51Z，EXIT0；show复读Assigned。
- `validate --as menxia`：EXIT0，202条任务符合schema v3.1；仅板字段校验，不代替行为验证。
- 证据根：[attempt-1](../../target/round66/menxia/attempt-1/)；[复算](../../target/round66/menxia/attempt-1/recomputed.json)、[Crystal摘录](../../target/round66/menxia/attempt-1/crystal-anchors.json)、[终检](../../target/round66/menxia/attempt-1/final-checks.json)、[SHA清单](../../target/round66/menxia/attempt-1/sha256-manifest.json)。
- 各命令JSON均含命令/UTC起止/原始EXIT/stdout/stderr；SHA清单排除自身避免循环；两freeze JSON前后SHA见终检，初终status/index与crates-diff原始输出一并保全。
