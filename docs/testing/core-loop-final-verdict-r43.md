# 第43轮核心闭环复核

**verdict：有条件成立。基础玩法分项和同一角色的服务端状态组合测试可认可；“新玩家自主连续完成核心闭环”尚未成立。**

明确回答：**新增确定性测试 + 既有 live 分项，仍不足以证明自然的同一角色连续旅程。必须补的阻塞项是自然旅程，不能把本次状态注入测试等同于它。** 本报告没有发现证据足以认定所有失败均属产品缺陷；结论是验证缺口，不是未经核实的故障归因。

范围：仅核对当前源码、已有日志并独立运行指定测试/资源冒烟；未改产品代码、未运行 git、未启动服务端。直接读取 [main ref](../../.git/refs/heads/main#L1) 时为 `667fb435ce945b4f34cc9c24627ca11c21c6cbf3`；测试按当时工作树评审，不声称clean。以下源码行号对应 t130 检查时的 `same_character_core_loop_journey`；随后开发中的新测试 **journey_natural 未验证，不计入本结论**。

## 证据与逐项判定

| 证据 | 判定 | 可直接核实的事实及限制 |
|---|---|---|
| 指定确定性测试实际通过 | **pass：测试执行** | 本次命令 `cargo test -p mir-server --test journey_server same_character_core_loop_journey -- --exact --nocapture`，[独立输出:5–8](../../target/audit-r43-journey.log#L5-L8)：`1 passed; 0 failed; 0 ignored`，0.08s、exit0。本次无skip输出；但源码 [134–137](../../crates/mir-server/tests/journey_server.rs#L134-L137) 缺cjlaaa时会return，不能仅凭未来测试ok就说链路实际执行。|
| 同一角色出生装/穿装 | **pass：World状态/包断言** | [148–226](../../crates/mir-server/tests/journey_server.rs#L148-L226)：同一Taoist，5件出生装，装备槽0/1及两条成功EquipItem包。测试用 [blank_map](../../crates/mir-server/tests/journey_server.rs#L44-L54)、关闭负重、初始置于Jane附近 [144–155](../../crates/mir-server/tests/journey_server.rs#L144-L155)；不证明实际新号移动可达性。|
| 接任务/追踪目标 | **pass：接取和目标文本；partial：完整任务生命周期** | [229–278](../../crates/mir-server/tests/journey_server.rs#L229-L278) 验证quest1接取、非空ChangeQuest目标、朱迪(294,619)，并比较`client_quest_info`返回定义。后者是服务端API结果，不是NewQuestInfo真实下发/客户端渲染证据。全测试无finish_quest，结尾 [733](../../crates/mir-server/tests/journey_server.rs#L733) 只要求quest仍active。|
| 杀鸡→掉肉→拾取 | **partial：状态组合，非自然因果链** | [kill_hen:97–103](../../crates/mir-server/tests/journey_server.rs#L97-L103) 直接设HP=1000000、DC/Accuracy=10000，并在 [113–116](../../crates/mir-server/tests/journey_server.rs#L113-L116) 清冷却；[305–318](../../crates/mir-server/tests/journey_server.rs#L305-L318) 若没掉肉就create_fresh_item+spawn_ground_item。真实鸡模板、attack/tick/pickup确实被调用，但正常角色战斗与实际掉落因果被绕过。未记录本次是否走了生成分支，因此不能声称本次鸡肉必为自然掉落。|
| Sarah卖肉 | **pass：商店状态接口与包断言** | [361–393](../../crates/mir-server/tests/journey_server.rs#L361-L393) 正确校验850模板、Price80、SellItem成功、GainedGold40、余额40；站位直接place_player，不是实际导航。|
| 0104购买/学治愈术/施法 | **pass：书店脚本、物品与法术状态组合；partial：跨图** | [395–409](../../crates/mir-server/tests/journey_server.rs#L395-L409) 把书店NPC的map_index改1、坐标改(284,605)，刷在map0；并未经过真实gate去0104。[430–481](../../crates/mir-server/tests/journey_server.rs#L430-L481) 验证两本免费书、Healing学习；[484–535](../../crates/mir-server/tests/journey_server.rs#L484-L535) 设置HP/MP/施法时间后，验证cost3/MP下降/S.Magic与观察者ObjectMagic。不能据此认可自然地图旅程或恢复补给整链。|
| 修理 | **pass：直接磨损→修理接口；partial：固定账目与再战** | [543–557](../../crates/mir-server/tests/journey_server.rs#L543-L557) 直接调damage_weapon；[584–603](../../crates/mir-server/tests/journey_server.rs#L584-L603) 断言修满、扣`repaired.cost`、ItemRepaired、重新穿装。没有`cost==2`或`gold==38`硬断言，[721–725](../../crates/mir-server/tests/journey_server.rs#L721-L725) 仍仅40−cost；即使cost0也可满足这部分断言。没有修后有效战斗断言。|
| 死亡/主动复活 | **pass：预设死亡后的计时与绑定复活；partial：死亡发生链** | [608–615](../../crates/mir-server/tests/journey_server.rs#L608-L615) 直接写hp=0/dead=true/death_time/revive_time；因此绕过真实受击死亡处理与死亡副作用。[617–645](../../crates/mir-server/tests/journey_server.rs#L617-L645) 确实验证12s无自动、绑定点复活、HP/MP满与Revived。|
| 存档/恢复 | **pass：MemoryStore及字段编码往返；partial：实际退出重登** | [648–685](../../crates/mir-server/tests/journey_server.rs#L648-L685) 保存后MemoryStore读回整行全等；[687–711](../../crates/mir-server/tests/journey_server.rs#L687-L711) 克隆旧Player，再手动解码替换装备/背包/技能/任务等，二次snapshot全等。不是Session正常退出、PG持久化、生产进世界加载；无任务交付，completed未被显式置成非空。|
| 前轮live分项 | **pass：各自记录范围；不能替代整链** | [core1i:220–231](../../target/core1i.log#L220-L231) 真实任务接取/交付+50；[tp1-final:10–19](../../target/tp1-final.log#L10-L19) 识别Jane并到0104；[ps1-strict:14–23](../../target/ps1-strict.log#L14-L23) 槽位一致但completed/spells空、gold0；[dr1-cjlaaa:94–103](../../target/dr1-cjlaaa.log#L94-L103) 自然死亡主动复活；[sell-gate14:67–95](../../target/sell-gate14.log#L67-L95) 卖肉+40、修理包-2；[magic1b:18–22](../../target/magic1b.log#L18-L22) 学习/耗蓝/施法；[core1d:72](../../target/core1d.log#L72) #973获得包。这些属于不同run，不拼成同一角色全链PASS。|
| 本轮live journey1 | **partial：未见完整终态** | [journey-final:59–70](../../target/journey-final.log#L59-L70) 任务只接不交付，随后反向走位FAIL；较新 [journey1n:24–31](../../target/journey1n.log#L24-L31) 同样明确“journey不交付”，[148–180](../../target/journey1n.log#L148-L180) 有主动复活与航点到达，但该日志读取时止于190，无完整售卖/购买/退出重登终态。脚本受阻不自动等于产品不可达，也不自动等于产品已验证。|
| 数据/图标 | **pass：报告与当前检查；未逐项重验导入来源** | [clean-report:2–8](../../game-assets/data/clean-report.json#L2-L8) 1349→1346，pruned3/warn1/icon_filled1；[8090–8133](../../game-assets/data/clean-report.json#L8090-L8133) 列三项删除及HardenedRing的GM引用、自绘派生标记。独立 `py tools/assets/verify_item_icons.py` [输出](../../target/audit-r43-icons.log#L1-L2)：kept1346、违规0，exit0。该守卫检查报告/目录/PNG存在，不等于所有物品画面语义与包内图标已逐一确认。|
| 发布包 | **pass：独立冒烟** | `pwsh -NoProfile -File tools/dev/release-smoke.ps1` exit0，[输出:1–11](../../target/audit-r43-release.log#L1-L11)：45包、151750条、434176803字节、67库、480000像素、diff0；manifest SHA256 `a237fdfec22bd96032480ca1b08b9d21c6ccfb76c8f7184d9622f025f45cb7d9`。|
| 门禁/PG故障注入 | **pass：修改后12项；partial：故障覆盖** | 最新已结束 [gate-journey3:2910–2923](../../target/gate-journey3-013359.log#L2910-L2923) 实际12项，图标加入而fault_pg退出，不是13项同覆盖。该日志 [1477–1484](../../target/gate-journey3-013359.log#L1477-L1484) 是0 passed/6 ignored；前次 [gate-journey2:1477–1508](../../target/gate-journey2-011200.log#L1477-L1508) 是3 passed/3 FAILED，报Elapsed超时，不只有“挂起”。无法凭此裁定环境或产品根因；忽略后绿不代表故障恢复已通过。|

## 三项发现与裁决

1. **high：报告层级与测试事实不符。** 测试通过真实，但使用放大属性、可生成肉、搬NPC、手设死亡；还没有任务交付、固定修理费或自然死亡断言。必须将现有结果标为“状态组合测试”，拒绝其作为自然旅程完成证明；测试有价值，无需因范围有限而否定其接口组合验证。
2. **high：生产重登/连续玩家路径仍缺。** 同一对象上的World调用和MemoryStore解码往返，无法排除Session/PG进出世界中的跨阶段状态问题；前轮strict只测了空任务/技能。必须补能完成任务且保留非空进度的同角色自然旅程及正常恢复路径，才可宣布原目标完成。
3. **medium：门禁覆盖减少必须披露。** 当前是12项且fault_pg六条忽略，前次有超时失败；删除故障项后的绿不能被描述成该能力修好。短期可登记为故障恢复未验证，不阻塞正常联机试玩，但阻塞“当前版本PG故障恢复已经全验”的声明。

## 阻塞完成项与可后补项

**阻塞“自主核心闭环完成”的最小项：自然旅程。** 必须证明同一角色使用正常战斗属性、实际掉落、真实地图/gate与原位NPC，完成接取→交付/奖励→玩法经济/装备技能的连续因果链；死亡由正常受击产生、确认复活后能继续；正常退出和生产恢复路径保留非空任务/技能、金币、装备与背包槽位/UID/模板/数量/耐久。没有要求伤害或掉率固定；允许固定RNG及预声明有限重试，但不得未掉就补物、临时增伤或改状态保证成功。无掉落的随机结果不是产品失败，无法拿到状态时也不能说该步骤已验。

确定性测试可以承担不依赖真实时间的生产逻辑验证；不要求每次都联网跑真人长流程。但要取代live整链，必须保持生产因果入口与世界配置，且对TCP/Session/PG/UI实际跨层边界保留针对性证据。MemoryStore自然旅程即使通过，也不能直接宣称PG故障恢复或真人自主引导完成。

**可登记后补、不阻塞正常新手试玩：** 长期随机掉率统计、全AI与全部地图、红名PkTown、GM戒指画面品质、长跑稳定性与PG故障注入环境修复；都不得包装为已验。实时walker脚本能力修复可以后补，前提是另有等价玩家可见可达/指引证据；当前未见等价证据，不能仅以“脚本问题”宣布可达性已验。

本轮队长已确认上述状态注入缺口，并另派 `journey_natural`；**本报告按现有证据结案，不等待新测试、不预先授予新测试PASS**。新结果应另行复核，历史裁决不改。
