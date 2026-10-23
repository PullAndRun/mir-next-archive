# 第46轮核心闭环复核

## 裁决

**verdict：有条件成立，不能宣称整体核心闭环无条件成立。**

新版确定性旅程已补齐 Session 生产退出、修理后再战、耐久0状态和数值日志，服务端同一角色自然规则证据达到可接受标准；但真实TCP旅程的经济链在本轮明确失败（4只鸡、0鸡肉、600秒窗口），所以“玩家实际完整闭环”仍未完成。失败必须按FAIL记录，不能改写为SKIP，也不能用确定性服务端测试替代真实TCP经济链。

## 证据核验

### 1. Session生产退出：通过，但限于MemoryStore/测试Session

旅程现在从真实 `Session::new → ClientVersion → Login → StartGame` 开始：[journey_server.rs:369–403](../../crates/mir-server/tests/journey_server.rs#L369-L403)。下线调用生产入口 `session.leave_world().await`：[879](../../crates/mir-server/tests/journey_server.rs#L879)。该入口本身取任务物品、好友、社交，再执行世界leave、character_snapshot、save：[session.rs:3541–3583](../../crates/mir-server/src/session.rs#L3541-L3583)。

重登前后逐项核对：inventory/equipment完整对象（包括uid/index/耐久）、gold、magics、quests、bind：[journey_server.rs:880–895](../../crates/mir-server/tests/journey_server.rs#L880-L895)，新Session Login/StartGame及字段断言：[911–968](../../crates/mir-server/tests/journey_server.rs#L911-L968)。独立测试套件 [输出:5–8](../../target/audit-r46-journey-suite.log#L5-L8) 为 `1 passed; 0 failed; 0 ignored`，5.80s，exit0。

边界：存储后端仍是 `MemoryStore`，不证明PG并发/故障注入；但“生产Session退出入口被调用”这一项已不再是旧版手工snapshot桥接。

### 2. 修理后再战：通过

测试硬断 cost=2、金币减2、耐久回满：[journey_server.rs:756–774](../../crates/mir-server/tests/journey_server.rs#L756-L774)。随后重新攻击真实鸡，断 `report.is_some()`、命中后耐久下降1..=4，并输出修后耐久和damage：[journey_server.rs:776–812](../../crates/mir-server/tests/journey_server.rs#L776-L812)。这证明修理后武器仍可产生攻击结算并再次磨损；没有把未提供的具体 `4000→3998/damage=2` 当作已观察事实。

### 3. 耐久0：通过（规则边界，不是自然损坏实录）

旅程内构造同一武器副本并置0，调用 `apply_equipment_stats` 后断言统计为默认：[journey_server.rs:813–824](../../crates/mir-server/tests/journey_server.rs#L813-L824)。生产规则实现/独立边界单测同样在 [items.rs:3440–3482](../../crates/mir-server/src/items.rs#L3440-L3482)。这证明耐久0不贡献DC/AC；不证明真实玩家自然打到0并看见失效。

### 4. 磨损入口与数值日志：通过（确定性旅程）

命中结算在 `world.rs:2852–2858` 调用 `damage_weapon`，而非出手次数：[world.rs:2852–2858](../../crates/mir-server/src/world.rs#L2852-L2858)；Session只发起攻击，不再重复磨损：[session.rs:1524–1529](../../crates/mir-server/src/session.rs#L1524-L1529)。测试新增实际日志：战斗durability/attacks [journey_server.rs:601–616](../../crates/mir-server/tests/journey_server.rs#L601-L616)、售肉price/gold/killed [654–658](../../crates/mir-server/tests/journey_server.rs#L654-L658)、MP before/after [725–730](../../crates/mir-server/tests/journey_server.rs#L725-L730)、修理gold/durability [758–771](../../crates/mir-server/tests/journey_server.rs#L758-L771)、修后攻击 [809–812](../../crates/mir-server/tests/journey_server.rs#L809-L812)、耐久0 [824](../../crates/mir-server/tests/journey_server.rs#L824-L824)。本次测试重定向文件仅保留测试摘要，未回显stderr数值行；不能从摘要虚构具体实测值。

### 5. 真实TCP旅程：FAIL，阻塞完整玩家闭环

[日志首行](../../target/journey-tcp.log#L1)记录HEAD `7fc7e1c`、clean=no、dirty_lines=1。穿装、任务接取、死亡复活0.009s、分段导航和耐久磨损均有成功证据：[1–59](../../target/journey-tcp.log#L1-L59)、[142–182](../../target/journey-tcp.log#L142-L182)。

但卖肉阶段明确失败：4只鸡、0鸡肉，600秒内仍未完成；[187–221](../../target/journey-tcp.log#L187-L221) 多次“附近没有可拾取地面物品”，[258–290](../../target/journey-tcp.log#L258-L290) 最终 `[FAIL] sell1 捡鸡肉 —— 已击杀 4 只 Hen，未掉鸡肉`；随后重试也未形成成功链，[335–346](../../target/journey-tcp.log#L335-L346) 记录阶段FAIL、总FAIL 7项。故不存在同一TCP角色的卖肉→金币→买书/施法→修理→重登完整证据。

## 残余项

### 阻塞整体“核心闭环成立”

1. **真实TCP同一角色经济链**：卖肉未完成，后续金币、技能、修理、重登无法在同一live角色上成立；应保留FAIL，不以掉率随机为理由抹除。
2. **客户端自主闭环**：TCP脚本已有部分协议证据，但不是完整客户端玩家操作/反馈链；若目标定义包含玩家自主完成，仍需同一角色从接任务到经济/技能/修理/重登的连续证据。

### 可登记后补，不阻塞服务端确定性旅程

- PG故障注入/持久化故障恢复（本轮未覆盖）；
- 自然耐久降至0后的真实UI损坏态与修后属性显示；
- 其他职业、地图、AI及PkTown；
- 掉率长期统计和更稳定的TCP寻路/击杀策略。

## 最终用语

可以对外写：**“第46轮服务端生产Session退出、自然战斗/掉落/修理/复活/字段重登验证通过；项目整体可玩核心闭环仍有条件成立，真实TCP经济链本轮FAIL，客户端自主连续旅程待补。”**
