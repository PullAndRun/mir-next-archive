# 第44轮自然旅程复核

**verdict：有条件成立；自然旅程仍有阻塞项，不能升级为“新玩家自主自然连续闭环成立”。**

改进真实且显著：正常战斗属性、实际掉落、不搬书店、真实任务交付、怪物致死、固定修理费及新Session加载均有证据。但该测试仍使用位置跳转和手动下线保存桥接，不能将接口组合等同于完整生产旅程。以下是验证范围裁决，不据未覆盖直接推断产品故障。

审计对象为读取时的 [journey_server.rs](../../crates/mir-server/tests/journey_server.rs)（897行）。只读源码/日志并复跑测试，不改产品代码、不执行git、不启动服务端；本文为唯一文档交付。后续源码变化应另作复核，不追改本时间窗结论。

## 独立执行证据

```powershell
cargo test -p mir-server --test journey_server journey_natural -- --exact --nocapture *> target/audit-r44-natural.log
```

[输出5–8行](../../target/audit-r44-natural.log#L5-L8)：`test journey_natural ... ok`，`1 passed; 0 failed; 0 ignored`，3.12s，exit0。没有skip输出，表明本次未走缺素材早退；源码 [270–273](../../crates/mir-server/tests/journey_server.rs#L270-L273) 仍允许缺cjlaaa时return，今后不能只用测试名ok冒充有素材执行。

## 八条声明与旧缺口逐项对照

| 声明/旧缺口 | verdict | 源码事实 |
|---|---|---|
| 1. 不放大HP/DC/Accuracy，正常属性战斗 | **pass：战斗属性；partial：自然追击/全程不写HP** | [common:284–302](../../crates/mir-server/tests/common/mod.rs#L284-L302) 使用player_base_stats；[292](../../crates/mir-server/tests/journey_server.rs#L292) 固定种子，旧百万HP/万点DC/Accuracy已删除；[217–245](../../crates/mir-server/tests/journey_server.rs#L217-L245) 通过World.attack/tick杀怪。但每次攻击前 [230](../../crates/mir-server/tests/journey_server.rs#L230) 直接place_player到怪旁；[671–675](../../crates/mir-server/tests/journey_server.rs#L671-L675) 施法前仍写HP/MP/动作与施法时钟。后者不是战斗增伤，不能夸成“仍放大属性”，也不能声称“全测试不写HP”。|
| 2. 不造鸡肉，最多20只真实掉落 | **pass** | [507–546](../../crates/mir-server/tests/journey_server.rs#L507-L546) 取真实鸡模板，逐只spawn/attack/tick，查实际ground_items，20只无肉即失败；已删除create_fresh_item/spawn_ground_item补850。该测试没有记录击杀数/掉落概率统计；通过只证明本固定种子自然获得850，不证明概率恒为1/5。|
| 3. 真实地图与门往返 | **pass：实际门；partial：完整路程** | [297–302](../../crates/mir-server/tests/journey_server.rs#L297-L302) 加载真实map0/0104；[402–424](../../crates/mir-server/tests/journey_server.rs#L402-L424) 实际walk踩门并断0104(4,10)，[486–505](../../crates/mir-server/tests/journey_server.rs#L486-L505) 实际门回map0(323,248)。但 [403–407](../../crates/mir-server/tests/journey_server.rs#L403-L407) 是直接place_player到(322,247)，不是从Jane走到门边。声明“玩家走到(322,247)”不准确。|
| 4. 真实Steven，不搬书店 | **pass** | [426–466](../../crates/mir-server/tests/journey_server.rs#L426-L466) 在真实0104查Librarian_Steven，walk到(5,9)，开店买1016/973；无改NPC地图位置/手动spawn书店。两本0价格、在背包的断言存在；[467–484](../../crates/mir-server/tests/journey_server.rs#L467-L484) 正常use_item学Healing。|
| 5. Jane接取、Judy交付 | **pass：任务生命周期** | [374–400](../../crates/mir-server/tests/journey_server.rs#L374-L400) 验证ChangeQuest目的地朱迪(294,619)；[628–655](../../crates/mir-server/tests/journey_server.rs#L628-L655) walk到Judy旁，finish_quest完成且completed含1。旧“只有active任务”缺口已闭合；未断具体奖励数，不外推奖励账目全验。|
| 6. 狼打死、12s无自动、回城 | **pass：自然致死/复活接口；partial：角色绑定点** | [765–805](../../crates/mir-server/tests/journey_server.rs#L765-L805) spawn真实狼并tick至dead，未直接写dead/revive_time，等12s仍dead且无Revived，town_revive后满HP/MP、Revived。**不是绑定点优先证明**：[common:350–351](../../crates/mir-server/tests/common/mod.rs#L350-L351) 初始bind为0/default，测试未另设bind；[794](../../crates/mir-server/tests/journey_server.rs#L794) 直接把(284,606)作为回退出生点传入，位置相同只证明fallback。角色绑定优先可引用前轮独立单测，不能把本例说成它的自然验证。|
| 7. 新World/Session真实加载 | **pass：生产登录加载；partial：生产退出和字段保真** | [808–831](../../crates/mir-server/tests/journey_server.rs#L808-L831) World.leave后自己snapshot/save/find，读档字段与before相等；[833–890](../../crates/mir-server/tests/journey_server.rs#L833-L890) 新World与Session.handle版本/Login/StartGame，加载非空Healing/completed、金币及2件装备，已去掉克隆旧Player解码。**未走带进度的Session退出**；[892](../../crates/mir-server/tests/journey_server.rs#L892) 最后才调用leave_world，之后没有再重登核对。重进World后只断装备数量2，未断槽位UID/模板/耐久/背包/绑定字段。|
| 8. cost2/扣2/修满 | **pass：金额合同；partial：自然磨损/修后有效战斗** | [751–759](../../crates/mir-server/tests/journey_server.rs#L751-L759) cost==2、gold_before−2、current==max均硬断，旧cost0可假绿已消除。不固定余额38合理。[554–559](../../crates/mir-server/tests/journey_server.rs#L554-L559) 无磨损时直接调damage_weapon，[741–742](../../crates/mir-server/tests/journey_server.rs#L741-L742) 直接place到Smith。重穿后未有玩家有效攻击/属性恢复断言。|

## 数据事实与允许的边界

- **鸡肉按实际价格一半售出、修理再扣2，可接受。** [607–620](../../crates/mir-server/tests/journey_server.rs#L607-L620) 使用本实例item_price与实际金币对照；随机耐久导致不同卖价，不应把40/38固定成错误判据。测试当前价范围断言60..79，能证明范围内样本，不证明所有可能分布。
- **先买/学书再战斗升级，可接受。** 这是一条可合法完成的顺序，无需所有排列都通过；但不能据此声称“升级后仍可学7级要求书”或玩家必定能理解这条顺序。
- **“产品攻击只磨护甲，不磨武器”与代码不符。** [Session攻击处理1493–1528](../../crates/mir-server/src/session.rs#L1493-L1528) 明确在取得attack report后调用`world.damage_weapon(object_id)`。World层直接attack绕过了Session这一入口，所以测试补调damage_weapon是低层模拟，不能用它推断产品缺武器磨损，也不能说自然入口已在本测试覆盖。
- **World.leave不是Session.leave_world等价路径。** [生产退出3545–3576](../../crates/mir-server/src/session.rs#L3545-L3576) 会在leave前取任务物品栏/宠物/好友/社交侧表，经Session.character_snapshot及save_character_row落盘；[3585–3590](../../crates/mir-server/src/session.rs#L3585-L3590) 还flush行会/邮件/市场。新测试的手工snapshot保存没有验证这些调度和失败传播，不据此声称一定丢失，只是没测到。
- 真实地形、固定RNG、有限20只重试与加速世界时钟可以作为确定性测试条件，不要求等待真实分钟。直接改位置、手设补给和跳过Session退出却会改变待证明的生产因果，必须显式保留partial。

## 三项发现及阻塞项

1. **high / 自然路程和资源恢复仍被绕过。** 门边、每次接敌、Smith和狼旁有place_player，施法前重置HP/MP/冷却；无法用本测试证明此前受阻的移动/追击/补给整链。必须以正常动作进入目标状态，或给等价同角色玩家可见证据；固定RNG和时钟可以保留。
2. **high / 正常退出→恢复仍未闭合。** 起始旅程不是Session，带非空进度的存盘由测试直接调用MemoryStore，之后虽真实Session加载却只查部分字段。必须补生产退出/保存入口与新Session重入后的关键槽位UID/模板/耐久、背包、金币、非空任务/技能对比，才能关闭原持久化阻塞项。
3. **medium / 两个声明范围错误。** (284,606)是回退实参，不是已建立绑定点；武器自然磨损入口实际在Session。应修正声明并以相应生产入口证据验证，不能靠函数名或注释判“自然”。

**升级裁决：不升级。** 可确认上一轮六类问题中，属性放大、手工补肉、搬NPC、直接制造死亡、无任务交付和修理费无硬判均有实质修正；旧克隆加载也改成了真实Session加载。仍未证明的是正常位置/补给/退出路径和重登后逐字段保真。认可这些进展，不把新测试命名为natural当成全部生产因果已通过。

边界登记后补：PG故障注入/长跑/所有地图AI/红名PkTown可继续单独追踪，不要求它们都塞进这条新手旅程；但是MemoryStore本例通过不能外推PG恢复、安全落盘或真人自主UI已验。既有live分项继续有效，其间缺失的同角色交互不能通过拼接不同run消除。
