# 第47轮TCP核心闭环最终裁决

**verdict：有条件成立。** 本轮已新增同一真实TCP角色完成售肉收入→修理扣款→跨图买书→学习/施法→正常退出重登的成功样本，上一轮“TCP经济链尚无成功证据”可关闭。结合确定性自然旅程，可认可基础可玩闭环已存在；但**不能按“TCP 9/9全量状态、任务已交付且重登全部进度已证”的口径无条件宣布完成**：当前live任务仅接取，active任务重登判据使用未清理的缓存，且首次重登槽位不一致的原因尚无直接证据闭合。

本报告只读当前源码/已有日志并独立执行自然旅程测试；未运行git、未启动服务端、未改产品代码。读取 [main ref](../../.git/refs/heads/main#L1) 为 `63e2b96313376e9cf36a79ff69e4fcc638450f0d`；[TCP日志首行](../../target/journey-loop-1.log#L1) 同哈希且自报 `clean=yes dirty_lines=0`，属于脚本记录，未另核构建二进制哈希。

## 逐项TCP证据

| 阶段 | verdict | 原日志与边界 |
|---|---|---|
| 穿装 | pass | [13–14](../../target/journey-loop-1.log#L13-L14)：2件；初始账号pt967458/角色PT967458，端口7147，[4–9](../../target/journey-loop-1.log#L4-L9)。|
| quest1接取 | pass：接取；未覆盖交付 | [23–34](../../target/journey-loop-1.log#L23-L34)：中文Jane页、#866x5、目标朱迪(294,619)、`active=[1]`，原文明确“journey不交付”。不能称该live任务生命周期完整通过。|
| 死亡/复活 | pass：协议与继续行动 | [183–215](../../target/journey-loop-1.log#L183-L215)：自然死亡(340,519)、12s未Revived、TownRevive响应约0.001s；回到出生区域且装备原UID两件保留。与声明中的0.009s不同，以本日志为准；可攻击是发C.Attack而非本处观察命中。|
| 分段导航 | pass | [216–221](../../target/journey-loop-1.log#L216-L221)：到(342,311)，阶段exit0。是脚本既定航点导航，不是新玩家自主理解任务UI实测。|
| 卖肉/修理 | pass：有界重试后的最终成功 | 首次售肉[247–248](../../target/journey-loop-1.log#L247-L248) +40，但磨损子步骤[266](../../target/journey-loop-1.log#L266)失败，再次找肉3杀无肉[535–536](../../target/journey-loop-1.log#L535-L536)导致第1次阶段FAIL。第2次[652–653](../../target/journey-loop-1.log#L652-L653)售肉+34，40→74；[665–692](../../target/journey-loop-1.log#L665-L692) 武器3957→3999/3999，真实LoseGold -2，74→72。不是把第一次未掉肉算通过，而是实际再次操作取得成功；重试延续已有40金币，不能说从清洁状态重跑。|
| 买书/学技能/施法 | pass | [723–734](../../target/journey-loop-1.log#L723-L734)：真实0→0104，货架#1016获得包、治愈术spell61、ObjectMagic=True、MP13→10。本书是免费首本，金币72不变，不能称本例验证付费购买；有价购买仍引用既有独立测试。|
| 回出生区域 | pass | [735–740](../../target/journey-loop-1.log#L735-L740)：0104→map0，最终(332,269)，并非精确出生格(331,270)，属脚本允许邻格。|
| 第1次重登 | fail：有观察差异，不推断根因 | [744–752](../../target/journey-loop-1.log#L744-L752)：前快照武器在槽0且4000/4000；后快照同UID在背包槽40、3999/3999，衣服5000→4982。此前修理包已显示3999/3999，说明前快照至少含旧数据；但仅凭日志仍不能证明所有差异均属脚本缓存或产品落盘。不能直接写“确认只是未重穿”已闭合。|
| 第2次重登 | pass：本次可见字段；active任务未验证 | [753–765](../../target/journey-loop-1.log#L753-L765)：先正常穿回同UID武器，再正常LogOut/新对象2310重登，前后equipment2/inventory3完整打印相同，武器3999/3999、衣服4982/5000、gold72、spells61、completed空；随后移动成功。[766](../../target/journey-loop-1.log#L766)的`quests=[1]`不是活跃任务重登对照证据，见下。|

## “9/9、0 SKIP、重试成功”的真实含义

源码 [journey1_flow:3292–3300](../../tools/playtest/playtest.py#L3292-L3300) 实际列 **8个阶段**，再加 [3304–3325](../../tools/playtest/playtest.py#L3304-L3325) 的非空状态检查，若将其计为第9项必须明确这一定义。日志没有独立的九阶段矩阵。

[stage:3206–3219](../../tools/playtest/playtest.py#L3206-L3219) 每阶段最多2次；PASS条件是函数code0且新增结果没有SKIP，**没有检查新增结果中是否出现FAIL**；因此 [768](../../target/journey-loop-1.log#L768)“全部通过（0跳过）”是最终控制流成功摘要，不是整份运行从未失败。日志真实保留第1次卖肉/修理及重登失败，审计不抹去它们。

有界重试本身可接受：随机掉落、动态阻挡、普通玩家重新穿装后再退出均是合法恢复操作；关键是必须提供真实的最后成功状态。本例最后第二次重登的快照确实相等，故不能因存在重试就否认经济与技能旅程的成功。相应地，一次成功不能证明重登稳定可靠、所有失败已根因闭合或35分钟旅程的新手体验达标。

指定文件里没有 `LOOP_RESULT=PASS run=1` 文本，也未内嵌OS退出码；本审计未找到独立loop包装器产物，因此**外层exit0/LOOP_RESULT未独立验证**。日志末尾与源码返回路径支持最终成功，但不伪造外部工具输出。

## 持久化关键判据：active任务并未在重登后重建核对

- [quest1_flow:4215](../../tools/playtest/playtest.py#L4215) 直接将1加入`active_quest_ids`。
- [ps1_flow:4530–4557](../../tools/playtest/playtest.py#L4530-L4557) snapshot包含金币、装备、背包、**completed_quests**及技能；没有active任务。
- [4593–4598](../../tools/playtest/playtest.py#L4593-L4598) 新连接清空inventory/equipment、技能和completed，但**未清空active_quest_ids**；搜索该字段没有重登包重建逻辑。
- [3305–3324](../../tools/playtest/playtest.py#L3305-L3324) 非空检查继续使用原active集合。因此“重登后active=[1]全等”不能从现判据推出。对active的PASS应降为未验证，不影响本次gold72/spell61/物品槽位实际快照成功。
- [4621–4647](../../tools/playtest/playtest.py#L4621-L4647) 硬比仅槽位/UID/index，耐久差异仅观察；本次第二组快照肉眼相等，证据成立，但脚本以后仍能漏耐久回滚，数量亦未比较。

## 确定性自然旅程和服务端修复

[自然旅程源码](../../crates/mir-server/tests/journey_server.rs)保留真实World.walk/门、正常属性攻击/实际掉落、Judy finish、主动复活、Session生产退出和新Session重登全字段比较；可补TCP未交付和完成任务字段的服务端逻辑证据。它不能等同同次live已交付或客户端active任务已经同步恢复。

本次实际命令：

```powershell
cargo test -p mir-server --test journey_server -- --nocapture *> target/audit-r47-journey2.log
```

自然旅程[独立运行输出:5–13](../../target/audit-r47-journey2.log#L5-L13)：武器4000→3988（delta12，attacks5，**不是命中数5**）；price79、售39、killed2；MP13→10/cost3；修理回4000/4000、cost2、金币39→37；修后攻击4000→3997、damage=Some(3)；0耐久不贡献DC/AC；1 passed/0 failed/0 ignored，5.91s，exit0。上述是本次真实数值，不借用声明7.04s为本次耗时。前一次审计作业因attempt切换无可用运行记录且日志仅编译行，未计通过，确认无后台任务后重跑。

服务端 [UserInformation发送位置](../../crates/mir-server/src/session.rs#L1164-L1280) 已先排StartGame/MapInformation/UserInformation，再处理物品与任务辅助包，支持所述避免构造阻塞的变更。新增测试 [indoor0104](../../crates/mir-server/tests/persistence_audit.rs#L1016-L1065)、[skill/quest/gold](../../crates/mir-server/tests/persistence_audit.rs#L1068-L1112)、[PG版本](../../crates/mir-server/tests/persistence_audit.rs#L1115-L1176) 均可见；它们启动测试监听器，按本任务“不起服”未运行，仅核查断言，PG版缺环境变量会return。这三条主要证明UserInformation可达/金币，**没有校验active任务重登后的包内容**。不能用“有三条测试”填补上述active缓存缺口。

## 残余项分类

**阻塞原始“自主新手任务生命周期与进度保持已完整完成”的声明：**

1. active任务需从重登后新收到的数据重建并比较；当前缓存自证不能通过。若要将本run称完整任务闭环，还需同一TCP角色交付成功/奖励及完成状态保持，或清晰限定本run只验接取。
2. 首次重登槽位差异与旧快照的来源需有直接证据说明，防止将生产状态变化误归为脚本；第二次成功可以认可，但不能据此宣布持久化异常已经修复。无需因为一次不掉肉/一次重穿操作就重开所有游戏功能。
3. 内部FAIL与恢复成功必须在最终结果保留区别，明确“第N次重试后PASS”；所有必需阶段仍失败则应FAIL。当前stage只检查返回码与无SKIP，不能称判据已排除内部FAIL假绿。队长已安排修改，但新版本未验证，不提前计入本判决。

**可登记后补、不阻塞当前基础试玩成功样本：** 掉落/寻路效率与35:11旅程时间优化；多次独立角色稳定性复跑；0104首次加载/死亡掉落专项；PG故障注入；v2资源切换；PkTown/其他职业地图AI；真人客户端指引、操作便利性和画面实测。若目标明确要求玩家自主完成，真人UI的可用性验证仍不能被协议脚本冒充。

**最终表述：同角色TCP基础经济/技能/恢复/重登成功样本成立，核心玩法可试玩；整体核心闭环有条件成立，活跃任务重登与完整任务生命周期的live证据尚未闭合。** 不使用“九项所有字段均全PASS、无失败、所有任务进度已恢复”的表述。
