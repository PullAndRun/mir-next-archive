# 第45轮自然旅程最终复核

## 裁决

**verdict：有条件成立。** 新版 `journey_natural` 足以显著升级“自然服务端游戏规则/同一角色状态连续性”的证据：正常属性战斗、真实掉落、真实地图门、真实NPC、任务完成、自然死亡、武器命中磨损、重登字段比较都有直接断言。但它**不能单独升级为无条件“玩家自主核心闭环成立”**：旅程仍通过集成测试直接调用 `World` API，怪物刷出位置由测试选定，前半下线保存绕过 `Session::leave_world`，没有完整客户端/TCP玩家输入整链；先前live分项也不是这次同角色旅程。

审计边界：只读当前代码与日志并执行指定旅程测试；未运行git、未改产品代码、未启动独立服务端。报告引用行号来自本次检查的工作树，未声称绑定clean commit。

## 测试实跑

命令：

```powershell
cargo test -p mir-server --test journey_server journey_natural -- --exact --nocapture *> target/audit-r45-natural.log
```

[输出:5–8](../../target/audit-r45-natural.log#L5-L8)：`1 passed; 0 failed; 0 ignored`，9.10s，进程exit0；有真实cjlaaa资源，不走缺库return。测试结果只证明这条测试通过，不等于真人客户端或全任务游戏体验都通过。

## 六点逐项对照

| t132主张 | 核验 | 直接证据/限制 |
|---|---|---|
| 1. 走路 | **成立：测试内实体步进；仍非客户端玩家实测** | [try_walk_to_on:180–215](../../crates/mir-server/tests/journey_server.rs#L180-L215) 以真实地图`walkable` BFS规划，逐步`World::walk`，被阻时tick重算；[217–225](../../crates/mir-server/tests/journey_server.rs#L217-L225) 断言抵达。出生到门[471–489](../../crates/mir-server/tests/journey_server.rs#L471-L489)、回程[551–570](../../crates/mir-server/tests/journey_server.rs#L551-L570)、Steven[491–493](../../crates/mir-server/tests/journey_server.rs#L491-L493)、Sarah[638–645](../../crates/mir-server/tests/journey_server.rs#L638-L645)、Judy[686–694](../../crates/mir-server/tests/journey_server.rs#L686-L694)、Smith[788–799](../../crates/mir-server/tests/journey_server.rs#L788-L799)、鸡肉[627–629](../../crates/mir-server/tests/journey_server.rs#L627-L629)均改为走路；该测试中未使用`place_player`。不过BFS会规划绕开怪/静态NPC，测试模型与客户端实走/实时拥挤碰撞并不等价；先前live walker受阻事实仍不应擦除。|
| 2. 战斗 | **成立：真实属性的World攻击/命中磨损** | [common默认玩家属性:284–302](../../crates/mir-server/tests/common/mod.rs#L284-L302)来自`player_base_stats`；测试固定seed[346–353](../../crates/mir-server/tests/journey_server.rs#L346-L353)，不覆盖玩家HP/DC/Accuracy。真实鸡模板与攻击循环[269–301](../../crates/mir-server/tests/journey_server.rs#L269-L301)，逐次World.attack/tick；[586–625](../../crates/mir-server/tests/journey_server.rs#L586-L625)累计真实攻击次数并断武器耐久确有下降且不超过每次1..4。生产磨损落在命中结算[world.rs:2852–2858](../../crates/mir-server/src/world.rs#L2852-L2858)，Session出手点已删除旧“按出手磨损”[session.rs:1524–1529](../../crates/mir-server/src/session.rs#L1524-L1529)。**日志没有输出本次实际命中数/耐久差值**，所以无法给出声称的`4000→3988、3次命中`实测；源码仅能支持断言界限，exact样本值未观测。仍由测试选定刷怪中心并调用`spawn_monster`[579–595](../../crates/mir-server/tests/journey_server.rs#L579-L595)，不是验证自然刷新AI/在自然怪群中自行找到鸡。|
| 3. 施法 | **成立：未手设HP/MP/时钟，断耗蓝3** | Healing学习[532–549](../../crates/mir-server/tests/journey_server.rs#L532-L549)；施法前记录当前MP并仅自然推进世界时钟[715–736](../../crates/mir-server/tests/journey_server.rs#L715-L736)，断`mp_after == mp_before-3`及S.Magic/观察者S.ObjectMagic[747–773](../../crates/mir-server/tests/journey_server.rs#L747-L773)。没有固定断言mp_before具体为多少，测试实际日志也不打印MP数值，所以本次只能确认断言关系，不应声称观察到绝对MP数字。|
| 4. 重登 | **部分成立：新Session加载+字段对照；退出保存非生产入口** | 新角色先正常写Character bind[329–368](../../crates/mir-server/tests/journey_server.rs#L329-L368)，前半World.leave、手工character_snapshot与MemoryStore.save/find[859–882](../../crates/mir-server/tests/journey_server.rs#L859-L882)；随后新World、新Session经ClientVersion/Login/StartGame加载[884–936](../../crates/mir-server/tests/journey_server.rs#L884-L936)。重登后Gold、完整equipment/inventory `UserItem`向量（含槽位/UID/index/耐久）、magics、completed quests、bind均与离开前Player相等[937–956](../../crates/mir-server/tests/journey_server.rs#L937-L956)。因此加载端和字段保真断言真实；但保存端并没有调用Session::leave_world，而生产函数[session.rs:3541–3577](../../crates/mir-server/src/session.rs#L3541-L3577)要先取quest inventory/社交侧表、构snapshot再save。memory store也不等于PG生产并发。不能把完整登录流程误说为完整生产登出/重登全程。|
| 5. 绑定回城 | **成立：角色Character bind输入及World绑定点优先** | 从刚创建Character行写入map/x/y并存储[334–368](../../crates/mir-server/tests/journey_server.rs#L334-L368)，复制到World Player[371–383](../../crates/mir-server/tests/journey_server.rs#L371-L383)。自然死亡段不写死状态：[wolf位置/击杀](../../crates/mir-server/tests/journey_server.rs#L814-L825)，随后等待12s断仍dead/无Revived[825–837](../../crates/mir-server/tests/journey_server.rs#L825-L837)；调用town_revive时给(10,10)fallback实参，却断言位置为Character bind (284,606)[839–857](../../crates/mir-server/tests/journey_server.rs#L839-L857)。这确实证明有效角色bind覆盖fallback，而非旧版本的fallback假阳性。红名PKTown差异不在本旅程范围。|
| 6. 武器磨损/修理 | **成立：命中结算磨损+修理账目硬断言** | 生产世界命中结算调用damage_weapon[world.rs:2852–2858](../../crates/mir-server/src/world.rs#L2852-L2858)；Session已不在出手时磨损[session.rs:1527–1529](../../crates/mir-server/src/session.rs#L1527-L1529)。旅程断自然战斗后的耐久下降[614–625](../../crates/mir-server/tests/journey_server.rs#L614-L625)，卸武器/找Smith/修理并断cost=2、扣2、耐久回满[775–812](../../crates/mir-server/tests/journey_server.rs#L775-L812)。因此旧的测试内damage_weapon直接补磨损已删除。测试没有打印本次耐久差值，且没有证明修好再打一刀属性/伤害恢复；不能引用未输出的数值或外推“修后再战已验”。|

## 其余连续链与随机性

- **任务**：Jane目标包[443–469](../../crates/mir-server/tests/journey_server.rs#L443-L469)，真实门往返[471–570](../../crates/mir-server/tests/journey_server.rs#L471-L570)，完成Judy任务[686–713](../../crates/mir-server/tests/journey_server.rs#L686-L713)，确为同一World玩家/character flow，不再是只接不交。
- **掉落与售卖**：实际鸡肉来自ground_items[572–636](../../crates/mir-server/tests/journey_server.rs#L572-L636)，未建item850；按真实耐久价格范围price60..79并断售出price/2[655–684](../../crates/mir-server/tests/journey_server.rs#L655-L684)。随机耐久使卖价30..39是正确的预期，不将固定40当成必然判据。最多击杀20只为声明内有限重试；本日志没有输出实际杀鸡数、耐久、售价，只能说测试通过该组断言，不能推回样本具体值。
- **资源与环境**：map0/0104真实文件断言加载[352–358](../../crates/mir-server/tests/journey_server.rs#L352-L358)；本次旅程对cjlaaa资源缺失会return，实际通过耗时9.10s且无skip输出，说明本次有通过测试路径，但没有额外归档资源文件hash。

## 裁决与剩余边界

新版已闭合t131的六个主要阻塞：走路不再place_player；战斗不放大属性、真实掉落不补造；施法不写HP/MP；bind来自Character且确实优先；磨损由真实命中结算；重登后有完整Player字段比较与新Session实际加载。故与前一轮相比，**服务端确定性“自然因果旅程”已成立，适合接受为核心玩法规则的同角色端到端测试**。价钱随机性与先学书后升级的执行顺序均属合理：用当次真实模板价格断言比例，而不是固定金币；顺序避免越级不使单步假绿。

对项目更强的“可玩的核心闭环”声明仍维持**有条件成立**，不要写成无条件成立：

**仍阻塞无条件完成声明**

1. 同一角色生产Session正常登出→保存→新Session重登的完整路径未跑；现测试前半使用World.leave+手工snapshot/save，独立登录仅覆盖加载。因此跨Session保存路径需要证据后才可关闭。
2. 该集成测试直接调用World API进行接任务/攻击/交易/施法，而非真实TCP客户端；既有live分项可支持登录、NPC交互、购买、施法/复活的部分包行为，但来自不同角色与run，不能组合成同次live旅程。若“核心闭环”定义只要求服务端核心规则，当前可升级该层级；若定义含玩家客户端自主完成与操作反馈，则仍需玩家端连续路径证据。
3. 并未覆盖修理后再次战斗的属性恢复，或者持续打怪至耐久归零/装备失效；这超出当前任务，但仍是装备维护的完整玩法闭环边界。

**可登记后补，不阻塞服务端旅程/基础试玩**：全PG故障注入（fault_pg环境当前为忽略）、其他职业/任务/地图/怪物AI、PkTown红名规则、长期掉落概率统计、真实客户端画面与可用性长测。它们不能作为已PASS宣传。

结论用语建议：**“第45轮服务端核心自然旅程成立；项目整体可玩核心闭环有条件成立，生产登出持久化和客户端自主连续旅程仍需独立证据。”**
