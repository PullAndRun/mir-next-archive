# 第 41 轮核心闭环最终裁决

## 结论与适用范围

**裁决：有条件成立。** 已有证据足以认可“基础玩法可以试玩，关键能力由真实协议分项验证和确定性测试共同支撑”的工程里程碑；**不足以宣布“首次进入的玩家靠游戏内指引，自主连续完成一轮、退出后保留全部进度”的原始目标已完全验收**。这不是零 SKIP 全链路 PASS，也不是所有 Crystal 行为对齐。

条件是对结论范围的限制，不是将 SKIP 换算为 PASS：保留 live 升级、0耐久整链、任务追踪 UI、非空任务/技能与耐久重登等未覆盖标签。若“核心闭环完成”必须取原先自主新手整链的含义，则答案是**尚不成立，仍需本报告末尾三项证据**。

审计时间：2026-10-22，Asia/Shanghai。只核对现有日志/源码并运行测试和 packs 冒烟；没有运行 git，没有启动服务端，没有改产品代码。通过直接读取 [main ref](../../.git/refs/heads/main#L1) 核到 `1940b32cf112e202501cd9684a145e562038d028`；不据此声称工作树 clean。旧 live 日志属于不同端口/版本，不能统一冒充在此 commit 上新跑的整链。[tp1-final 首行](../../target/tp1-final.log#L1) 甚至明确记录 `54d6f8a... clean=no dirty_lines=36`，这是来源边界，不是当前 HEAD 的 clean 证明。

第40轮 [历史审计](core-loop-audit-round40.md#L5) 的 partial 保持原时间窗结论；以下仅评价本轮新增证据，不追改历史裁决。

## 逐项裁决与原始证据

| # | 声明 | verdict | 直接证据与不能外推的部分 |
|---|---|---|---|
| 1 | quest1 | **pass：任务协议链** | [core1i:220–231](../../target/core1i.log#L220-L231)：Jane 中文页“你好 PT250486, 这是你的首次冒险吗?”；接取、任务物品 #866x5、交付后 `completed_quests 含 1；经验 27->77（+50，cjlaaa）`。本次经验不是0→50；只能报告增量+50。|
| 2 | TP-1 新口径 | **pass：NPC定位与书店跨图；partial：任务自主指引** | [tp1-final:10–19](../../target/tp1-final.log#L10-L19)：`任务目标=助理·简 map=0 pos=(284,606) quest_ids=None`，再实际进入0104，Steven距离1；确实不再是任意首门皇宫。源码 [4035–4092](../../tools/playtest/playtest.py#L4035-L4092) 只识别Jane并检查坐标/地图非空，再调用书店路线；没有接任务或验证追踪文字与该目的地因果关联。不能证明任务追踪UI、点击、到交付NPC的整链。|
| 3 | PS-1 strict | **pass：穿装槽位即时重登；partial：全部进度** | [ps1-strict:12–25](../../target/ps1-strict.log#L12-L25)：两件装备槽0/1和背包槽2/3/4前后完整打印相同，收到 `S.LogOutSuccess` 后新连接重登，移动(330,270)→(331,270)。本样本耐久也相等，但金币0、completed/spells为空。源码 [4203–4229](../../tools/playtest/playtest.py#L4203-L4229) 已分别比较装备与背包的槽位/UID/index，关闭上轮跨容器假绿；耐久仍只观察，未比数量。日志+2s是总时长的粗粒度显示，不是精确的“穿装后延迟≤2s”测量。|
| 4 | DR-1 cjlaaa与绑定回城 | **pass：自然死亡与主动复活协议；partial：真实UI整链/绑定点live** | [dr1-cjlaaa:91–103](../../target/dr1-cjlaaa.log#L91-L103)：自然死亡，死亡时间1792664648.335，到1792664666.715仍未复活（约18.380s）；TownRevive发送后0.011s收到S.Revived，HP16，随后位置到(331,270)。这是主动路径的真服证据，已补上轮缺口。“可攻击”只证明C.Attack发送且连接保持，未观察攻击结算；复活包处坐标是旧缓存，不据此判错误落点。客户端点击弹窗另有10条测试，不是同次真人UI实测。当前绑定优先及无效回退由下述两条测试独立证明，旧7123日志不能证明1940b32的新绑定点live。|
| 5 | EQ-1 | **pass：卖肉收入/磨损修理与0耐久属性规则；partial：完整live耗尽整链** | [sell-gate14:67–76](../../target/sell-gate14.log#L67-L76)：杀鸡捡#850、卖给Sarah，+40；[93–95](../../target/sell-gate14.log#L93-L95)：`[金币] -2（累计 +38）`、同UID修理3978→4000。末尾“40→40扣0”与包级证据矛盾，采用包级-2，余额38由算术推出。`@Sell/@Repair`为NPC页键，不是GM指令。0耐久由当前 `zero_durability_equipment_contributes_no_stats` 证明，未证明live自然耗尽/失效/修后再战。|
| 6 | buy1 | **pass：免费书获得路径** | [buy1b:12–21](../../target/buy1b.log#L12-L21)：0104、货架UID201、#973、价格0；正确补包来源 [core1d:70–74](../../target/core1d.log#L70-L74) 为 `S.GainedItem item_index=973 ... count=1`。两个run不能说成同一条完整包追踪；免费物品不能证明付费购买扣款。已有商店确定性测试为有价/负例补证，见第40轮审计第5项。|
| 7 | magic1 | **pass：学习/耗蓝/施法协议** | [magic1b:18–24](../../target/magic1b.log#L18-L24)：#1016到包、`S.NewMagic name=治愈术 spell=61`、`S.ObjectMagic=True MP15->12`。括号“期望13→10”为旧显示，实际耗蓝3一致；不外推技能升级或所有法术。|
| 8 | core1组合 | **partial** | [core1i:232–249](../../target/core1i.log#L232-L249) 明确5个SKIP：卖肉、买书、修理、PS、升级；原文 `7 阶段全部 PASS/SKIP`、`全部通过（含5个跳过项）`。这只能证明已执行阶段无终态失败，不是七阶段全PASS。所称进程exit0未保存在该文本内，本审计不重新执行，不能从尾句独立核实OS退出码。|
| 9 | 升级 | **pass：确定性；live SKIP** | 当前复跑 `level_up_from_99_plus_five_matches_crystal` 与 `multiple_levels_take_the_new_threshold_after_each_level` 均ok，见 [audit-final-lib:137–138](../../target/audit-final-lib.log#L137-L138)。[core1i:240–249](../../target/core1i.log#L240-L249) live升级仍SKIP。单测覆盖阈值/连续跨级/LevelChanged逻辑，不替代真实客户端升级展示。|
| 10 | 发布形态/自有格式 | **pass：packs渲染冒烟；partial：完整发布运行/格式隔离** | 独立复跑release-smoke，输出另存 [audit-final-release](../../target/audit-final-release.log)。45包/480000像素/diff0表示指定场景渲染一致，不能证明全地图/全资源正确。格式`.mnd/.mnm/.mna/.mnp`不等于版权归属；未在本任务新增启动完整launcher，不能借格式名证明运行时无旧格式回退。|
| 11 | 12项门禁 | **pass：现有终态证据** | [gate-bind3-195304:2911–2924](../../target/gate-bind3-195304.log#L2911-L2924) 十二项OK，原文 `全部通过（12项）`。其中 [20–27](../../target/gate-bind3-195304.log#L20-L27) 两条组队经验live用例ignored，不叫零跳过。未重跑全量/未KillStale，以免影响现有服务；关键规则本轮独立复跑。|

## 当前工作树独立复核

实际命令（不启动服务端）：

```powershell
cargo test -p mir-server --lib -- --nocapture *> target/audit-final-lib.log
cargo test -p mir-client --lib ui::death -- --nocapture *> target/audit-final-death-ui.log
pwsh -NoProfile -File tools/dev/release-smoke.ps1 *> target/audit-final-release.log
```

前两命令均exit0；服务端 [汇总:164](../../target/audit-final-lib.log#L164) 为149 passed/0 failed，客户端死亡弹窗 [5–18](../../target/audit-final-death-ui.log#L5-L18) 为10 passed/0 failed。release-smoke也exit0，[1–11行](../../target/audit-final-release.log#L1-L11) 为45包、433980520字节、151581条目、67库、480000像素、diff0，清单SHA256=`3e8c5e12220c82f92bdec8b76575a1f435eaa20556944e7fdf89d91a174580f1`。服务端输出 [8–16行](../../target/audit-final-lib.log#L8-L16) 有 ExpList.ini / Server.MirDB 样本缺失的早退提示，不能声称149条均无隐式跳过；本裁决依赖的规则测试均另查名称与断言。

本轮关键测试：

- [items:3440–3481](../../crates/mir-server/src/items.rs#L3440-L3481)：构造基础属性、耐久0武器/衣服不加DC/AC，耐久1加成恢复；这是边界状态单测，**不是自然战斗打坏证据**。本轮输出 [59行](../../target/audit-final-lib.log#L59) ok。
- [world:6203–6246](../../crates/mir-server/src/world.rs#L6203-L6246)：`town_revive_prefers_valid_player_bind_point`、`town_revive_falls_back_to_start_when_bind_is_invalid`，分别验证bind图(7,8)优先于出生点、无效时回退；本轮输出 [146–150](../../target/audit-final-lib.log#L146-L150) ok。源码 [3414–3458](../../crates/mir-server/src/world.rs#L3414-L3458) 明确PkTown未实现。绑定字段快照/存储往返测试也ok，但不是新版本PG+live整链。
- [death UI测试输出:6–16](../../target/audit-final-death-ui.log#L6-L16)：包含动画结束后弹窗、12s不点击、是只发一次、否不发包、i18n和实际点击处理。这补客户端行为逻辑，不能拼成一次真人从死亡到点击复活的实录。
- [穿装攻击模型输出:156](../../target/audit-final-lib.log#L156)：accuracy7、DC2..4、7225/10000命中；确定性模型支持装备生效，不把随机实战命中率设成固定值。

## 已知边界能否接受

| 边界 | 裁决 | 接受范围 |
|---|---|---|
| live升级SKIP，确定性测试通过 | **试玩里程碑可接受** | 仅逻辑已验；真实击杀升级、客户端表现及重登升级后状态未验，不可计入live完成率。|
| 0耐久仅单测 | **试玩里程碑可接受** | 磨损/付费修理live+属性边界单测能支撑功能存在；完整EQ-1自然耗尽整链仍partial。|
| PkTown未实现 | **非PK新手范围可接受** | 必须说明红名也走绑定/出生点；不可声称完整Crystal死亡规则或PK玩法闭环。|
| core1五项SKIP | **分项证据汇总可接受；完整整链不接受** | 独立成功不能证明同一角色跨阶段状态交互，特别是死亡掉落、奖励物品、技能与重登相互作用。|
| PS耐久仅观察、非空任务/技能未覆盖 | **仅该穿装即时重登样本可接受** | 槽位假绿已修；不能外推全进度不丢或旧组合失败根因已全部消除。|
| TP任务目标识别后去书店 | **跨图能力可接受；自主任务指引不接受** | `quest_ids=None`且未接任务；仅找Jane坐标不等于玩家从追踪面板找到交付目标。|
| 分别运行的真服脚本+UI单测 | **工程验证可接受** | 不能称真人第一次进入的易用性实测，不能通过修改完成定义悄悄替代原验收。|

## 若要无条件宣布原始闭环完成，必须补齐

1. **同一角色的连续玩家旅程**：游戏内指引接任务→实际战斗/奖励→买/穿装备→至少一次正常退出重进，关键非空进度与物品/槽位/数量/耐久有因果可复核；阶段不得以其他run成功换成本run的PASS，失败可恢复。该旅程是否能由新玩家自主完成需实际UI证据。
2. **失败恢复与维护整链**：实际客户端死亡弹窗确认→绑定/出生点→有效行动；自然耐久失效/修理后属性恢复与再次战斗的现象补验。单测/注入状态继续保留为分层测试，不冒充live。
3. **来源与保留标签**：构建身份、判据版本、运行窗口与终态退出码同run留证；live升级仍标SKIP直到真实升级观察完成；TP目的地需与实际任务追踪目标关联。当前没有证据证明上述未覆盖项存在产品缺陷，只能判“未完成验证”。

因此，本轮可对外表述为：**基础可玩能力有条件成立，已完成分项验证；自主新手整链和部分边界场景仍未完成验收。** 不使用“核心闭环全PASS”或“全部live覆盖”的表述。
