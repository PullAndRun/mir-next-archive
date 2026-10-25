# R51 门下独立评审

- 主单：`JJC-20261025-003`；审核：menxia / gpt-6.1-sol；2026-10-25（Asia/Shanghai）。
- **verdict：PASS-WITH-CHANGES**。批准本报告修订后的派发契约；原方案不能原样执行，尚书须逐条吸收以下7项并登记子单后再启动执行者。
- 基线HEAD：`716db4292ad6d475e92bf8b6cbd4f230f307684f`；独立读取[中书方案](round51-plan.md)全部85行及Crystal/Rust源码。本报告直接给出替代验收与裁剪，不以日后补写方案为批准前提。
- 本次只写本报告，未改crates、未运行cargo/CI、未git写。看板在本报告落盘后操作，此处不预写流转成功。

## 一、必改项（7项）

| ID | 级别 | 必改内容与完成条件 |
|---|---|---|
| M1 | high | 消除“七技能完整补全”与排除护身符消耗的冲突：本轮不扩张耗材范围，PoisonCloud、TrapHexagon完整移植明确PARTIAL；不得上线绕过必需耗材的免费施法。保留真实拒绝/未支持状态并交缺口证据，不能用“不再Unsupported”强行判绿。 |
| M2 | high | R51-A以§三七行矩阵验收，覆盖资料、耗蓝/惩罚、筛选、延迟/生命周期、随机门槛、失败与包序；补TemporalFlux、Dash专包、SpellObject/中毒/ShockTime源证据，不能只换分派枚举。 |
| M3 | high | 用§五真实命令替代不存在的文档脚本、“现有客户端命令”“全量既有命令”等占位；过滤命令须核对非零用例数，缺新增行为断言仍PARTIAL。文档检查不证明商城/技能语义。 |
| M4 | high | 用§四固定owner/依赖/精确写范围；R51-C归同一兵部功能子单，A产生服务端行为包，C验证解码和客户端状态，不重复实现；Walk/Run/商城包仅回归，禁顺带功能扩张。 |
| M5 | high | 纠正职责：[团队职责](team-and-roles.md#L24)规定Lead唯一执行全量门禁和git；礼部只交文档diff，兵部只交功能diff，尚书只登记/验收，不提交、不跑全量。刑部终审必须依赖Lead门禁完成。 |
| M6 | med | P12只更新两份活动文档的当前状态并链接R50-C2/T2/验收证据；保留旧提交、缺陷和返工历史，不全局删除旧HEAD/“返工中”，不清除P02–P08/P10和已披露Crystal差异。 |
| M7 | med | 更正锚点和误判：测试实在world.rs:6801–6823，针对被动Fencing，不能替换为七技能成功测试；Blink查名不是生产接线，客户端labels/spellfx只是部分映射/色块；默认资料锚点须扩至完整对象。 |

## 二、锚点抽查（逐条实读，Crystal 12项、Rust 9项）

| 侧 | 方案锚点 → 实际证据 | 核验结论 |
|---|---|---|
| Crystal | [Envir.cs:931–934](../../github/Crystal/Server/MirEnvir/Envir.cs#L931) | Blink真实存在；BaseCost=10、LevelCost=3、Range=9。只证明上游资料，不证明Rust支持。 |
| Crystal | [Envir.cs:935–952](../../github/Crystal/Server/MirEnvir/Envir.cs#L935) | StormEscape存在；原935–942截掉消耗/范围；完整值65/8/9。 |
| Crystal | [Envir.cs:333–350](../../github/Crystal/Server/MirEnvir/Envir.cs#L333) | ShoulderDash存在；原只到Level1；BaseCost=4、LevelCost=4、DelayBase=2500。 |
| Crystal | [Envir.cs:711–728](../../github/Crystal/Server/MirEnvir/Envir.cs#L711) | FrostCrunch存在；15/3/Range9；原711–718不足以证明完整资料。 |
| Crystal | [Envir.cs:729–748](../../github/Crystal/Server/MirEnvir/Envir.cs#L729) | ThunderStorm存在；29/9/Range0；伤害参数亦在原引用尾部之外。 |
| Crystal | [Envir.cs:1247–1266](../../github/Crystal/Server/MirEnvir/Envir.cs#L1247) | PoisonCloud存在；30/5/Range9、DelayBase18000、DelayReduction2000。 |
| Crystal | [Envir.cs:1097–1112](../../github/Crystal/Server/MirEnvir/Envir.cs#L1097) | TrapHexagon存在；7/3/Range9，原引用截断。 |
| Crystal | [HumanObject.cs:3577–3658](../../github/Crystal/Server/MirObjects/HumanObject.cs#L3577) | 只覆盖五技能；Blink在3537–3539，FrostCrunch在3501–3504；“该范围分派七技能”不准确。 |
| Crystal | [HumanObject.cs:4264–4270](../../github/Crystal/Server/MirObjects/HumanObject.cs#L4264)、[6079–6128](../../github/Crystal/Server/MirObjects/HumanObject.cs#L6079) | 雷击入地图延迟500ms；StormEscape延迟传送且带TemporalFlux；原6079–6114仅到Blink标签，漏Blink实际语义。 |
| Crystal | [HumanObject.cs:4543–4563](../../github/Crystal/Server/MirObjects/HumanObject.cs#L4543)、[4596–4641](../../github/Crystal/Server/MirObjects/HumanObject.cs#L4596) | PoisonCloud需护身符5+毒5并各消耗5；TrapHexagon需护身符1；与排除清单直接冲突。 |
| Crystal | [Map.cs:1301–1341](../../github/Crystal/Server/MirEnvir/Map.cs#L1301)、[1434–1488](../../github/Crystal/Server/MirEnvir/Map.cs#L1434)、[1752–1826](../../github/Crystal/Server/MirEnvir/Map.cs#L1752) | 雷系中心5×5且非亡灵ThunderStorm伤害/10；毒云3×3实体6000ms/1000ms；困魔ShockTime/清目标与外围实体，不是伤害圈。 |
| Crystal | [SpellObject.cs:150–166](../../github/Crystal/Server/MirObjects/SpellObject.cs#L150)、[503–548](../../github/Crystal/Server/MirObjects/SpellObject.cs#L503) | 方案遗漏毒云MAC伤害+绿毒、ObjectSpell的Show/CastLocation规则；须包含生成/消失生命周期。 |
| Rust | [magic.rs:184–202](../../crates/mir-server/src/magic.rs#L184)、[503–528](../../crates/mir-server/src/magic.rs#L503)、[1134–1138](../../crates/mir-server/src/magic.rs#L1134) | 七技能都走兜底Unsupported；REGISTERED_UNSUPPORTED列表本身仅五种其他技能，不能拿列表证明七技能。 |
| Rust | [world.rs:6043–6052](../../crates/mir-server/src/world.rs#L6043) | Blink是本地化测试中的查名，缺资料早退；无生产施法效果。 |
| Rust | [net.rs:7060–7097](../../crates/mir-client/src/net.rs#L7060)、[labels.rs:1735–1753](../../crates/mir-client/src/ui/labels.rs#L1735) | 仅部分分组/名称；spell_effect只给Blink映Teleport；labels未列全七技能，不能称全接线完成。 |
| Rust | [spellfx.rs:40–99](../../crates/mir-client/src/ui/spellfx.rs#L40) | 是时长/色块/区域分组，未证明Crystal视觉素材或运行时包链。 |
| Rust | [server.rs:822–879](../../crates/mir-protocol/src/packets/server.rs#L822)、[7774–7775](../../crates/mir-protocol/src/packets/server.rs#L7774) | ObjectWalk INDEX28、ObjectRun INDEX29和注册真实；不证明Dash或移动表现完整。 |
| Rust | [server.rs:5959–5985](../../crates/mir-protocol/src/packets/server.rs#L5959)、[7955–7956](../../crates/mir-protocol/src/packets/server.rs#L7955)、[manual/server.rs:1089–1119](../../crates/mir-protocol/src/manual/server.rs#L1089) | GameShopInfo手写并re-export，INDEX250；Stock INDEX251；结构/编解码存在，不等价于商城全量验收。 |
| Rust | [world.rs:6801–6823](../../crates/mir-server/src/world.rs#L6801) | 原magic.rs:6808–6812越界（实际3455行）；此处Fencing被动技能拒绝/不扣MP/不发Magic，应保留。 |
| Rust | [magic.rs:1286–1294](../../crates/mir-server/src/magic.rs#L1286) | 传送惩罚仅对Teleport，且先普通再传送惩罚；新增Blink/StormEscape须核对Crystal顺序和整数截断。 |
| Rust/文档 | [gameshop.md:38–41](../world/gameshop.md#L38)、[384–386](../world/gameshop.md#L384)、[client.md:909–910](../client.md#L909) | P12滞后属实；R50验收已说明C2修复/P11闭环，可定点更新当前状态、保留历史。 |

## 三、七技能可证伪矩阵（兵部逐行交证据，非本次运行结果）

共同要求：逐技能核完整MagicInfo（等级/修炼/威力/延迟），耗蓝按Crystal [MagicCost](../../github/Crystal/Server/MirObjects/HumanObject.cs#L3375)顺序；逐行提供成功、拒绝、延迟边界断言和实际包序。
普通分支对照[HumanObject.cs:3773–3779](../../github/Crystal/Server/MirObjects/HumanObject.cs#L3773)的CastTime、本人Magic、他人ObjectMagic；ShoulderDash提前return，不能强套普通包。

| 技能 | 资料/消耗 | 效果、目标、持续/位置 | 包序与必要接线 |
|---|---|---|---|
| Blink | 10+3L；Range9；传送惩罚再普通惩罚 | [3537–3539](../../github/Crystal/Server/MirObjects/HumanObject.cs#L3537)延迟200ms；[6114–6128](../../github/Crystal/Server/MirObjects/HumanObject.cs#L6114)禁传、范围、有效落点、Random.Next(4)门槛、Teleport成功；30s TemporalFlux/30% | 普通回显→延迟传送出入/AOI→ObjectEffect.Teleport/升级/buff实序；不能随机Teleport代替指定落点。 |
| StormEscape | 65+8L；Range9；同上惩罚 | [3580–3588](../../github/Crystal/Server/MirObjects/HumanObject.cs#L3580)先排500ms自身中心雷击，再排750ms落点传送；禁传/随机失败不抹去先前伤害；30s惩罚 | 分别断言500/750ms两阶段、ObjectEffect.StormEscape、位置与效果包；非Blink别名。 |
| ShoulderDash | 4+4L；Delay2500ms | [4975–5157](../../github/Crystal/Server/MirObjects/HumanObject.cs#L4975)：受困/不可走、距离2+L+[0,1]、安全区/阻挡、低等级敌目标、Pushed、伤害/修炼、CellTime500ms | UserDash/ObjectDash逐格、零移动DashFail、最后MagicCast；补空间对象移除/加入与碰撞，Walk/Run不能替代。 |
| FrostCrunch | 15+3L；Range9 | [Fireball:3939–3952](../../github/Crystal/Server/MirObjects/HumanObject.cs#L3939)敌目标/CanFly/距离×50+500ms；[5962–5995](../../github/Crystal/Server/MirObjects/HumanObject.cs#L5962)MAC伤害、原落点±2/同地图/节点，等级/概率控制Slow和Frozen | 普通包→延迟伤害/状态包；区分玩家/怪物概率和时长，不能遗漏减速/冰冻。 |
| ThunderStorm | 29+9L；Range0 | [4264–4270](../../github/Crystal/Server/MirObjects/HumanObject.cs#L4264)500ms；[Map.cs:1303–1341](../../github/Crystal/Server/MirEnvir/Map.cs#L1303)施法时自身中心5×5敌玩家/怪，非亡灵伤害/10 | 普通包→地图延迟伤害；测边界/友方/亡灵，不做持续毒云或指定落点雷区。 |
| PoisonCloud | 30+5L；Range9；18000−2000L冷却；护身符5+毒5 | 落点3×3去重/有效格；距离×50+500ms；实体6000ms/1000ms tick；绿毒Duration12、SC均值+BonusDmg；敌玩家/怪 | 普通包、ObjectSpell的Show/CastLocation、tick伤害/中毒/移除；耗材缺口保持PARTIAL，不得免耗成功。 |
| TrapHexagon | 7+3L；Range9；护身符1 | [4596–4641](../../github/Crystal/Server/MirObjects/HumanObject.cs#L4596)预检y上界严格<，不擅自改成对称3×3；怪等级≤施法者+2；500ms后ShockTime/清目标，持续(5L+10)s、外围8实体 | Cast真假、ObjectSpell生成/移除、Shock解除；[SpellObject.cs:57–79](../../github/Crystal/Server/MirObjects/SpellObject.cs#L57)关联ShockTime消失清实体；耗材缺口保持PARTIAL。 |

## 四、可直接派发的owner/写范围/依赖

| 子任务 | owner与可写范围 | 依赖与交付 | 命令编号 |
|---|---|---|---|
| R51-B | libu/gpt；仅[gameshop.md](../world/gameshop.md)、[client.md](../client.md) | 无；按已交付R50证据定点修订，交diff/状态映射；Lead提交文档批 | B1、B2 |
| R51-A+C | bingbu/deepseek；[magic.rs](../../crates/mir-server/src/magic.rs)、[world.rs](../../crates/mir-server/src/world.rs)、[buffs.rs](../../crates/mir-server/src/buffs.rs)、[monster.rs](../../crates/mir-server/src/monster.rs)及内嵌测试；客户端仅[net.rs](../../crates/mir-client/src/net.rs)、[spellfx.rs](../../crates/mir-client/src/ui/spellfx.rs)、[labels.rs](../../crates/mir-client/src/ui/labels.rs)及内嵌测试；协议仅[manual/server.rs](../../crates/mir-protocol/src/manual/server.rs)相关测试 | B交付且文档批提交；唯一功能写者，交七行行为/差异表、真实包链；生成协议文件只读。需其他生产文件先交尚书审定精确扩围 | A1、A2、C1–C3 |
| R51-D | shangshu/gpt；仅新建本轮派发/验收材料及主任务memo；Lead唯一git/全量执行 | A/B/C交付→Lead全量→刑部终审→尚书验收；证据含HEAD、原始exit、起止、字节/SHA和实际项数 | D1–D3 |
| 终审 | xingbu/gpt；仅新建docs/audit/round51-final-audit.md | A/B/C与Lead本次门禁全部完成；只读审计，列七行证据和每个PARTIAL，不代修/不补跑 | T1、T2 |

先串行，禁止共享文件并写。模型按[团队路由](team-and-roles.md#L12)：gpt治理/文档，deepseek实现，由Lead启动。尚书先登记此矩阵和命令再派执行者。

## 五、证据与验收命令修订清单

cwd统一仓库根；以下为后续契约，本次未运行cargo/CI。预期exit0且列实际通过/总数；过滤0用例不能通过。

```text
B1: py -3 tools/docs/check_links.py
B2: git diff --check
A1: cargo test -p mir-server --lib magic -- --nocapture
A2: cargo test -p mir-server --lib -- --nocapture
C1: cargo test -p mir-protocol
C2: cargo test -p mir-client --lib net::tests -- --nocapture
C3: cargo test -p mir-client --lib ui:: -- --nocapture
D1: pwsh -NoProfile -File tools/dev/ci.ps1
D2: git log -2 --oneline
D3: git status --porcelain
T1: git diff --check
T2: git rev-parse HEAD
```

- 已实测B1：落盘前833条、落盘后894条相对链接，均exit0、2篇README引用；stdout中文受宿主编码影响，数字/exit明确。原scripts/validate-docs.mjs全仓路径搜索无结果，删除“若存在”占位。
- B1只查链接/市场包编号，不覆盖商城250/251语义；B2另人工对照[R50验收](round50-shangshu-acceptance.md#L27)与[P12说明](round50-shangshu-acceptance.md#L53)，每条当前“已交付”须源证据。
- A1保留magic单测；A2涵盖world测试，必须新增七行行为断言并交测试名→维度映射；原Fencing拒绝断言保留。A2退出0本身不证明七技能完成。
- C1–C3回归Walk28/Run29、商城250/251；补本批真实服务端输出→编解码→net状态断言，自构造往返不能冒充真实链路。
- D1沿用R50真实入口，以本轮脚本实际项数/退出码为准；禁止SkipTests/SkipPostgres冒充全量，soak.lock存在时禁KillStale；仅Lead执行。D2核文档批→功能批，不伪造恰好两次历史提交。
- T1/T2为刑部只读命令，另读Lead原始日志并复算SHA256/字节、核起止/HEAD；终审在全量之后，旧R50日志不能覆盖本轮。

## 六、对方案§七五条裁决

1. **条件允许**缺实体/字段时裁剪并标PARTIAL；不允许发明字段、改包号或伪装不同语义。必需语义缺失不计技能完成，毒云/困魔按M1禁免耗。
2. **条件允许**协议回环+枚举作为C的协议子层最低证据；须加真实包消费/状态断言，视觉单列PARTIAL，不可将C整体或视觉标PASS。
3. **允许**文档批先提交，仅作已交付R50事实基线；**不允许**“唯一事实基线”覆盖Crystal源码/新功能证据或删除历史。Lead提交。
4. **允许并要求**保留R50入口D1，尚书派发时登记本报告定向增量命令，功能后补实际新增断言清单，Lead执行；不能功能后才决定最低标准。
5. **允许**PetSave/PetExp作为R52明确跟踪项，由有权角色登记关联主单/子单并记ID；本次不越权建R52单、不虚构ID。它仍是既有未完成移植缺口，不计R51新增回归，也不从差异表删除。

## 七、范围裁剪与风险（5项）

- 保持技能首批/P12同步；完整目标为五技能，PoisonCloud/TrapHexagon交缺口/PARTIAL；若五技能仍缺Crystal必需接线则逐项PARTIAL，不承诺假绿。
- C只做本批接线和既有包回归，不追加插值、商城功能、K重绑、AOI全量、攻城、宠物持久化等R52+事项。
- 风险1：耗材排除导致两技能无法完整交付；禁免耗/明确PARTIAL，不以枚举支持结项。
- 风险2：随机门槛、整数截断、延迟中心/包序遗漏；按矩阵分支断言，不只看终态HP/位置。
- 风险3：Shock/Slow/Frozen、实体生命周期不匹配；范围不足先审定或交PARTIAL，不写假字段。
- 风险4：回环/色块与Crystal视觉不同；协议、状态、视觉分别结论，缺素材PARTIAL。
- 风险5：状态更新覆盖旧失败证据或职责混淆；活动状态定点更新，历史/冻结证据不动，Lead唯一git/全量。
