# 第48轮TCP核心闭环裁决

## 结论

**verdict：成立——限于本项目“最小基础可玩核心闭环”，允许正常操作中的有限重试。**

本轮同一真实TCP角色已实际完成穿装、免费买书/学习/施法、跨图、接取并交付任务、死亡后主动复活、战斗拾取售肉、付费修理、正常退出与重登；完成标记、技能、非零金币及实际装备/背包在最后一次重登前后相同。叠加服务端确定性自然旅程，上一轮的任务交付与完成状态保持缺口已有直接证据。**不再因同一个脚本观察器缺陷重复阻塞基础闭环成立。**

这不是“全部阶段第一次就通过”、三连跑稳定性完成、全功能/全职业/所有数据无缺陷或正式发布验收，也不是已验证真人客户端首次使用的易用性。当前源码和日志实际为 **9个执行阶段 + 非空断言**，不是任务摘要所写8个；重登明确第2次成功。应以“首个完整run最终成功，含PS重试”表述。

本报告核验现有日志、源码并复跑自然旅程；未改产品代码、未运行git、未启动服务端。直接读取 [main ref](../../.git/refs/heads/main#L1) 为 `8793c9b78d798c97eac36086d807bde711c1f006`；[live首行](../../target/journey-3x-1.log#L1) 同HEAD、`clean=no dirty_lines=4`。没有核验dirty文件清单或构建哈希，故结论针对已有产物/当前观察，不伪称clean HEAD发布验收。

## TCP逐阶段核验

角色为 [pt386846/PT386846](../../target/journey-3x-1.log#L2-L9)，起始对象2298，端口7158。正常重登后对象ID会改变，不因此当作换角色；脚本使用同账号和原char_index重新登录。

| 阶段 | verdict | 直接原文/数值 |
|---|---|---|
| 1 穿装 | pass | [13–14](../../target/journey-3x-1.log#L13-L14)：穿2件，第1次成功。|
| 2 1级买书/学技能/施法 | pass | [16–27](../../target/journey-3x-1.log#L16-L27)：0→0104，获得#1016，S.NewMagic治愈术61，ObjectMagic=True，MP13→10；价格0。这证明新手免费购买，不证明有价购买。|
| 3 回map0 | pass | [28–31](../../target/journey-3x-1.log#L28-L31)：0104→0，(323,248)。|
| 4 quest1接取+交付 | pass | [51–66](../../target/journey-3x-1.log#L51-L66)：中文Jane页，ChangeQuest活动1，目标朱迪(294,619)，交付后completed含1、经验0→50，active从[1]移除为空。已不再是前轮的只接取。|
| 5 自然死亡/主动复活 | pass | [236–268](../../target/journey-3x-1.log#L236-L268)：自然死亡(344,520)；死亡时间1792742850.301至请求1792742869.238约18.937s仍无Revived；请求后即收到Revived，日志显示0.000s（时间戳精度约0.001s），不能改写成精确0.001s。随后移动到(331,270)，装备原UID2件，inventory5。可攻击在此为发包/连接保持，实际战斗另有后续命中证据。|
| 6 分段导航 | pass | [269–274](../../target/journey-3x-1.log#L269-L274)：到(342,311)，阶段第1次通过。|
| 7 卖肉+修理 | pass：最终达成 | 前几次击杀无掉落属随机结果且日志保留FAIL；[372–373](../../target/journey-3x-1.log#L372-L373) 最终售鸡肉+33，余额33；[387–404](../../target/journey-3x-1.log#L387-L404) 耐久3963→3999/3999，真实0103门dest10到Vincent修理，金币33→31扣2，并成功穿回槽0。不是手工置金币/耐久。|
| 8 回出生区域 | pass | [405–415](../../target/journey-3x-1.log#L405-L415)：0103首个回程点BFS失败，换回程点正常换图，最终map0(332,271)，属出生邻域，不是精确出生格。|
| 9 正常退出/重登 | pass：第2次；第一次是观察器快照不一致 | [419–427](../../target/journey-3x-1.log#L419-L427) 首次snapshot缺Feather、耐久旧值而FAIL；[431–440](../../target/journey-3x-1.log#L431-L440) 第2次正常LogOutSuccess及重登，前后equipment2/inventory4完整打印一致，武器3999/3999、衣服4985/5000、gold31、completed[1]、active[]、spells[61]，随后移动成功。|
| 非空检查 | pass | [441–443](../../target/journey-3x-1.log#L441-L443)：gold31、装备2、背包4、技能61；任务已完成，因此允许active空，completed[1]有前后直接证据。|

## Feather 857裁决：真实获得已证，脚本缓存问题可后补

**不把它当成“重登凭空多出物品”的生产持久化缺陷。** 直接因果证据：

1. [317–319](../../target/journey-3x-1.log#L317-L319) 在第一次重登约9分钟以前已有 `S.GainedItem item_index=857 uid=1879826355094290442 count=1` 和 `捡起掉落 857x1`；
2. [424](../../target/journey-3x-1.log#L424) 重登后的Feather正是**同一UID**、同一模板857，不是新UID，第二次也仅同一件；
3. 当前 [观察器935–943](../../tools/playtest/playtest.py#L935-L943) 收S.GainedItem只append `inventory_items`，未同步 `inventory_by_slot`；而 [snapshot4714–4723](../../tools/playtest/playtest.py#L4714-L4723) 只读后者。重登UserInformation重建槽位缓存，所以原本已经获得的物品此时才进入被比较的视图。

因此首次“多出857”差异来源已有可直接推出的观察器解释，**不是仅因第二次PASS就猜测第一次没问题**。首次前快照漏掉的还有任务奖励/消耗后的状态风险，不能继续信任这套缓存做所有物品完整性证明；但本例已获得Feather的事实、第二次两次完整打印状态和确定性测试共同足以支持基础闭环。

耐久也存在同类缓存缺口：[DuraChanged912–923](../../tools/playtest/playtest.py#L912-L923) 更新列表、[ItemRepaired928–934](../../tools/playtest/playtest.py#L928-L934) 只记事件，未维护PS所读的全部槽位对象。因此首次前快照4000/4000与刚观察的修后3999/3999矛盾，不能当作实际耐久回滚；第二次打印则都正确相等。修复观察器并补负例属于明确质量项，可后补基础试玩，不可继续以旧缓存“严格全字段”宣传。

## 上轮任务缓存缺口是否闭合

- [715–729](../../tools/playtest/playtest.py#L715-L729) 由CompleteQuest更新completed并从active移除；ChangeQuest重建active，NewQuestInfo定义不当作活动任务。
- [PS snapshot4724–4730](../../tools/playtest/playtest.py#L4724-L4730) 同时保存completed/active/技能；[4771–4785](../../tools/playtest/playtest.py#L4771-L4785) 重连清空completed/active并排空新包；[4800–4813](../../tools/playtest/playtest.py#L4800-L4813) 实际比较两者。
- 本次重登以**completed非空、active为空**验收任务完成状态；这是正确的已交付场景，不能额外要求active必须非空。任意未完成任务多目标进度保持仍未被本例覆盖，单列专项即可，不再把它追加为最小闭环阻塞。

## 判据/元数据修正

[journey1执行表3367–3376](../../tools/playtest/playtest.py#L3367-L3376) 是9执行阶段；非空检查另算。首个run整体最终成功不等于各阶段首试成功：PS第1次明确FAIL，第2次PASS。阶段内部多次无掉落打印FAIL后取得真实肉，并未跳过所需状态，因此最终玩法成功可以认可，但不叫“无失败”。

[stage3273–3285](../../tools/playtest/playtest.py#L3273-L3285) 仍以code0且无SKIP判PASS，rows虽标重试但阶段打印为PASS；[summarize5390–5412](../../tools/playtest/playtest.py#L5390-L5412) code0时隐藏失败汇总。这是报告可读性问题，需保留“最终成功含重试”的限定，不改变已经能由原日志逐项复核的成功事实。

指定日志内没有 `LOOP_RESULT=PASS run=1` 与OS退出码。外层包装器输出/退出码**未验证**；已有末尾“0跳过”、全阶段原状态和正常返回路径支持最终成功，但不伪造外层原文，也不宣称已3连跑。23:07为最后记录时长，任务称23:08可以是外层耗时，二者不混用。

## 独立确定性复核

实际命令：

```powershell
cargo test -p mir-server --test journey_server -- --nocapture *> target/audit-r48-journey.log
```

[输出6–14](../../target/audit-r48-journey.log#L6-L14)：

- 武器4000→3988，delta12、attacks5（**attack调用次数，不是命中次数**）；
- 真实掉肉price79、sold39、killed2；
- MP13→10、cost3；修理4000/4000、cost2、金币39→37；
- 修后攻击4000→3997、damage=Some(3)；0耐久不贡献DC/AC；
- `1 passed; 0 failed; 0 ignored`、6.30s、exit0。

[Session实际退出和新Session字段比较](../../crates/mir-server/tests/journey_server.rs#L859-L969) 已覆盖上一轮生产退出缺口；[finish_quest](../../crates/mir-server/tests/journey_server.rs#L670-L692) 补同角色任务规则，且当前TCP又给出真实交付完成记录。耐久0是副本边界断言，不是live自然耗尽，但与自然磨损/修后再战分层证据合用，不需把玩家打坏4000点耐久作为此次最小闭环前置。

资源/12项门禁声明本轮未重新运行或查新终态，按先前已审计结果仅作背景，不扩大为当前dirty树发布证明。v2资源切换未实施不属于最小玩法闭环阻塞；不得说已经切换。

## 残余分类

**阻塞最小基础可玩闭环成立：本次未发现仍可直接推出的阻塞项。** 不以修饰名称、让同一随机场景多试直到“零失败”为新的完成门槛；真实成功、失败可恢复和关键进度保留已具备。

**可后补、必须登记但不阻塞本次里程碑：**

1. medium：修复PS观察器两套列表/槽位缓存和新增/消耗/修理/磨损同步，补防假绿负例；将“首次全PASS”改成最终成功且PS重试。
2. medium：补独立角色重复通过、首次重登/0104加载、死亡掉落时序和非空active任务专项；本次仅一次成功样本，不能推出可靠性百分比。PG故障注入仍独立未覆盖。
3. low：精确阶段数、外层退出码和loop结果归档、clean构建身份；v2切换、其他职业/地图/PKTown和真人中文UI易用性另立范围。

**允许的最终声明：第48轮最小基础可玩核心闭环成立，已有一次同角色TCP任务/经济/技能/死亡恢复/重登成功样本并经确定性自然旅程补证；PS观察器存在已定位缓存缺陷，成功包含重试，稳定性和发布验收另行进行。**
