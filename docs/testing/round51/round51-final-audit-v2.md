# 第51轮 T2 独立终审：BLOCK

- 主单：`JJC-20261025-003`；刑部子单：`JJC-20261025-003-sub-c0e495c7`。
- 审计对象：`main`，`HEAD=origin/main=f500269974c92d32b3510aef902e1d8fd2e7c2c6`；独立核对日期：2026-10-25（Asia/Shanghai）。
- **结论：BLOCK。** attempt5 的提交树门禁合法通过，不能据此宣布五技能完整或 P03–P10 全部关闭。P03 的施法者 Freezing 汇总仍遗漏装备模板与 buff；P10 的矩阵仍夸大几处行为覆盖。必须先修复或由有权验收角色明确裁决这些新增缺口，再决定主单能否 PASS-WITH-PARTIAL。
- 本人只读审查既有代码、提交和原始证据，仅新建本报告、[v2 manifest](<final-manifest-v2.json>)、[gate5 摘要](<../../../target/round51/final-gate5-summary.log>)。未改代码、历史产物、原始日志，未 commit/push，未运行全量门禁或 ignored 证据脚本。独立复跑限于两条 Python 校验及指定客户端集成测试。

## A．门禁原件合法性：PASS，纠正一处基线误述

[gate5 原始日志](<../../../target/round51/final-gate5.log>)与 [meta](<../../../target/round51/final-gate5.meta.txt>)均存在。独立按原始字节计算：**189824 B**，SHA256 **`091e096410bdb4788487da7966517a0de5d73e0a5801591c3475a86818367d11`**，与 meta 完全一致。meta 的 head 是本次完整 HEAD，branch=main，exit_code=0，UTC 起止 **03:07:21–03:11:33**，墙钟252秒。原日志末尾明确“全部通过（12项）”；没有用 attempt4 替代。

| Name（原日志名称） | Code/状态 | Seconds |
|---|---|---:|
| cargo fmt --all --check | 0 / OK | 3 |
| cargo clippy --workspace --all-targets -D warnings | 0 / OK | 20 |
| cargo test -p mir-server --test duo_smoke（串行） | 0 / OK | 15 |
| cargo test --workspace | 0 / OK | 206 |
| 协议生成器 --check | 0 / OK | 1 |
| 文档链接与包编号 | 0 / OK | 0 |
| TextKey 完整性 | 0 / OK | 0 |
| 硬编码棘轮（player-visible，阈值70） | 0 / OK | 0 |
| 硬编码棘轮（no-test，阈值0） | 0 / OK | 0 |
| 运行时不引用老资源格式 | 0 / OK | 0 |
| 物品图标完整性 | 0 / OK | 1 |
| PostgreSQL 集成测试（mir-store 串行） | 0 / OK | 4 |

原摘要逐项打印的是 OK/FAIL，不另存每个子进程的数字 Code；上表 Code=0 是依据门禁脚本成功分类所作映射，不能冒充另一份逐进程退出码记录。整数秒合计250秒，与252秒墙钟有2秒执行/取整差。日志混合 Rust UTF-8 与宿主中文代码页；摘要区按 GB18030 解码，SHA 始终计算原字节。

| attempt | 原件证据与 head | 退出结果及解释 |
|---|---|---|
| 1 | [meta](<../../../target/round51/final-gate.meta.txt>) / [log](<../../../target/round51/final-gate.log>)，7802eb9；188766 B | **exit1**。实际唯一 FAIL 是 player-visible 硬编码棘轮96>70，clippy 是 OK。Lead 基线所称“唯一 clippy 21条”与该原件不符；此前独立 clippy 失败日志不能混充本次 gate1。 |
| 2 | [meta](<../../../target/round51/final-gate2.meta.txt>) / [log](<../../../target/round51/final-gate2.log>)，7802eb9；186895 B | exit0、12/12，仍为功能未提交时的树证据，不能代替提交树。 |
| 3 | [meta](<../../../target/round51/final-gate3.meta.txt>) / [log](<../../../target/round51/final-gate3.log>)，13f7c78；186888 B | exit0、12/12；是 T1 所审提交，不能套到 B2 或当前 HEAD。 |
| 4 | [meta](<../../../target/round51/final-gate4.meta.txt>) / [log](<../../../target/round51/final-gate4.log>)，997eea4；63050 B | **exit1**。唯一 FAIL 为 workspace 测试：mir-core `text_key_all_is_complete` 实际243、期望241；其余11项OK。 |
| 5 | 本节首述，f500269；189824 B | exit0、12/12；仅此项为当前提交树最终门禁。 |

五份原件 SHA/字节均与各自 meta 相符，详细值见 manifest。失败链保留诚实。门禁范围遵循既有 CI：workspace 使用 `--skip duo_ --skip fault_pg`，duo另项串行；本报告不把12项称为仓库全部可能测试，也不替历史进程重建退出观察。

## B．返工闭环：大部分修复真实，P03 仍开

| 条目 | Rust / Crystal 证据及独立结论 |
|---|---|
| P03：Frozen / Slow 生效 | [monster.rs:1273](<../../../crates/mir-server/src/monster.rs#L1273>)实际否决 Frozen 的移动/攻击；[magic.rs:4212](<../../../crates/mir-server/src/magic.rs#L4212>) Slow 每跳两项速度+100ms、上限3500，结束恢复模板；对应 [MonsterObject.cs:634](<../../../github/Crystal/Server/MirObjects/MonsterObject.cs#L634>)、[1544](<../../../github/Crystal/Server/MirObjects/MonsterObject.cs#L1544>)。玩家 Frozen 接 walk/run/attack，Slow 接动作延迟；对应 [world.rs:2067](<../../../crates/mir-server/src/world.rs#L2067>)、[2161](<../../../crates/mir-server/src/world.rs#L2161>)、[2273](<../../../crates/mir-server/src/world.rs#L2273>) 与 [HumanObject.cs:121](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L121>)、[249](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L249>)。这些原 T1 阻断点确有代码修复。 |
| P03：1000ms / OperateTime | [resolve_frost_crunch:3147](<../../../crates/mir-server/src/magic.rs#L3147>)为两类毒传入1000ms tick，成功上毒重置 OperateTime；后续 tick 使用每个 PoisonState 的 tick_speed_ms。与 [HumanObject.cs:5972](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L5972>)至5991相符。旧“续跳固定2000ms”已修。 |
| **P03：Freezing 汇总仍错** | [caster_freezing:3115](<../../../crates/mir-server/src/magic.rs#L3115>)只做 `item.added_stats.get(Stat::Freezing)` 求和；`freeze_roll` 随后返回0或0..Freezing−1，Frozen 为5+roll。Crystal [HumanObject.cs:1858](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L1858>)、[1965](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L1965>)同时累加模板 Stats 与 AddedStats，[2332](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L2332>)累加 buff.Stats，再由[5987](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L5987>)读取总 `Stats[Stat.Freezing]`。当前只对“附加属性为唯一来源”的子集正确；模板或buff含Freezing而附加值0时，Rust始终5跳，Crystal可能延长。Rust既有[refresh_stats:1369](<../../../crates/mir-server/src/items.rs#L1369>)也区分模板、附加、buff，遗漏不能解释为完整等价。代码注释自承 CombatStats 缺字段，却宣称语义一致，不成立。**新确认的阻断缺口，未执行改码探针。** |
| P04：前置与零步 | [cast_shoulder_dash:3498](<../../../crates/mir-server/src/magic.rs#L3498>)检查存活、Frozen、Paralysis、ActionTime/CellTime；不满足时提前返回，通用扣蓝已发生，不发技能包/不记技能冷却。与 [HumanObject.cs:4977](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L4977>)的 CanWalk 部分相符；无 InTrapRock 单独列 PARTIAL。零步与成功共同写 CellTime=now+500、StepCounter=0，原提前返回漏写已修。 |
| P04：推人、聊天、火墙 | 低等级敌玩家走实际 `push_player_once`，本人127、周围128，位置/AOI/反向朝向/动作时间实际更新；等高等级阻挡。对应 [HumanObject.cs:2649](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L2649>)、[5021](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L5021>)。零步发送152/153及按当前位置安全区选择的两种系统文案，对应[5128](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L5128>)至5156。火墙入格命中即停是真实简化，未对同格所有对象逐一 ProcessSpell，保留 PARTIAL；不是完整原作冲撞。 |
| P05：地图冻结 | ThunderStorm 和 StormEscape 共有的雷击延迟动作现在携带 `map`，[结算:2219](<../../../crates/mir-server/src/magic.rs#L2219>)读取 cast_map；区域格使用 walkable_on。对应 [HumanObject.cs:4268](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L4268>)挂施法时地图 ActionList、[Map.cs:1303](<../../../github/Crystal/Server/MirEnvir/Map.cs#L1303>)。跨图旧地图怪物受伤、另一地图不受伤的反向断言存在；StormEscape禁传后雷击保留测试存在。**closed**，但未另跑 StormEscape 独立跨图用例；共用生产雷击路径已静态核实。 |
| P06：TemporalFlux与自身光效 | [broadcast_spell_effect:3002](<../../../crates/mir-server/src/magic.rs#L3002>)先向施法者发124再广播，原漏自机已修。[add_temporal_flux:3025](<../../../crates/mir-server/src/magic.rs#L3025>)创建真实30s buff，values[0]=30；cost读实际buff，生命周期由 tick_buffs 移除。 [2238](<../../../crates/mir-server/src/magic.rs#L2238>) StormEscape 为光效→buff→修炼；Blink为光效→修炼→buff，与 [HumanObject.cs:6079](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L6079>)、[6114](<../../../github/Crystal/Server/MirObjects/HumanObject.cs#L6114>)相符。测试有自机光效、到期和耗蓝三态；严格包序未测，列 P10 测试残余。核心修复 **closed**。 |
| P07：真实链 | [round51_spells_wire.rs:251](<../../../crates/mir-client/tests/round51_spells_wire.rs#L251>)确实由真实 World 走/跑/施法，从通道取服务端编码帧，decode后送 WorldSnapshot；122/150/151及127/128有数量或状态效果断言，第二例含152/153。本人复跑**2 passed，0 failed/ignored，exit0**。已补上 T1 所缺真实链。失败例初始坐标已等于权威坐标、没有预置活动motion，不能证明移除152/153消费后会失败，严格失败状态覆盖仍为 **partial**。 |
| P10：矩阵 | [B2矩阵](<../../../target/round51/b2/07-seven-row-matrix.md>)确实纠正普通技能不发122、冲撞只有末包122、Teleport先于ObjectEffect、首版diff +1481/−20等。仍写“非零原step_counter被清零”，[实际用例:5412](<../../../crates/mir-server/src/magic.rs#L5412>)未设非零；仍写 TemporalFlux 测试断言 Bell 顺序，实际该例未检查包顺序；将手工注毒用例称真实 Frost 非空触发也超出证据。B2摘要宣称“P03–P07/P10全部关闭、未决无”应撤回。**open**。 |

## C．PARTIAL 诚实性：PASS，不能扩张成全部完成

[Unsupported拒绝测试:5256附近](<../../../crates/mir-server/src/magic.rs#L5256>)仍逐技能断言 PoisonCloud/TrapHexagon 为 Unsupported、返回 UnsupportedSpell、MP不变，生产在扣蓝前拒绝。未发现免费成功、删 Unsupported 断言或放宽成允许施法。

[B2矩阵:35](<../../../target/round51/b2/07-seven-row-matrix.md#L35>)明确保留 FrostCrunch 玩家目标留PvP批、冲撞火墙简化、无InTrapRock系统；玩家消费控制能力已接，并不使 Frost 施放玩家目标变为完成。上述四类降级与代码相符。P08冲撞/推移动作继续借 Walking/Walk 声音，无本轮原生dash帧、像素/视觉运行证据，必须另列可见降级，不能随包链通过一起消失。

## D．假绿与捷径：未见恶意旁路，仍有可失败性缺口

对 R51 相对基线716db42的 crates/config/tools 新增3725行检查：新增 `#[allow(clippy...)]` **0**；私钥头标志与常见硬编码token模式命中均0。此为有界静态扫描，不是对历史全库无密钥的证明。生产未见按测试坐标写死成功、免费施法或将错误吞成成功；测试里的坐标/合成 MagicInfo 是fixture，未进入生产。基线 magic 文件的35个同缩进函数名在HEAD全部保留（HEAD共85），结合diff未见原拒绝断言被删除；仅函数名比较不能替代语义审查。

可失败性具体分开判断：

- 有效负向断言：Blink199/200ms边界、超程MP不变、高等级Frost不叠毒、5×5外不受雷击、跨图另一图不受伤、禁传保留雷伤、等等级玩家不被推、Unsupported不扣蓝。会检查实际错误/状态，不能统称假绿。
- [Frost施法测试:4968附近](<../../../crates/mir-server/src/magic.rs#L4968>)仍遍历可能为空的毒列表。新增控制例[5288](<../../../crates/mir-server/src/magic.rs#L5288>)直接注入PoisonState，能验消费/续跳，不能证明真实施法概率分支至少生成一次毒。反事实：若仅删 resolve_frost_crunch 的施毒调用，该空循环与手工注毒例仍可能通过；**本轮未实际修改/编译该变体**。
- Freezing测试只造 `added_stats=17`，未测模板/buff来源；对roll只要求落在0..16，恒0的错误随机实现也满足范围断言。应加入总来源与非退化随机分布/固定种子结果的明确验证。
- 零步StepCounter原值为0，删归零赋值仍可通过该断言；152/153失败链的同格静止初态也使消费缺失可能绿灯。应设非零计数、错误预测格、已存在motion，再消费真实帧验证回纠与Standing/清队列。
- 玩家测试验walk/run报错与action_delay×2，但未直接调用attack证明Frozen拒攻；生产接线已读，不把静态证据写成运行覆盖。
- TemporalFlux耗蓝测试base=4、30%按整数计为+1，能击穿“完全不收惩罚”，未证明默认MagicInfo的真实耗蓝/2500ms冲撞冷却，也未断言完整包序。

没有执行会修改源码的变异探针；以上反事实边界和未验证项公开记录，避免把静态推导冒充测试日志。客户端[Pushed/ObjectPushed:5157](<../../../crates/mir-client/src/net.rs#L5157>)真实落位和动作更新已接，不是空match。

## E．提交卫生与入库体积：PASS（在已核范围内）

五个R51提交的顺序及完整SHA见manifest；相对716db42最终改动24个路径，所查R51提交版本无>1MiB blob；最终最大blob为客户端网络文件350031 B，其次世界文件307947 B、魔法文件249520 B。未包含target路径、.env、私钥文件或大型冻结JSON；语言包小JSON是有意功能变更，不是资源大JSON入库。[T1报告](<round51-final-audit.md>)、[v1 manifest](<final-manifest.json>)、[索引](<README.md>)已在997eea4入库，分别26594、22669、495 B（git blob）。

审计开始的未跟踪文件恰为两个已知冻结数据：[before](<../monster-native/player-resource-freeze-before.json>) **44844165 B**、[after](<../monster-native/player-resource-freeze.json>) **45219533 B**。未处理它们。审计后预期新增两个未跟踪v2文档，摘要属于ignored target产物；这不算既有工作树异常。Git tracked状态及HEAD在收尾再次核验，结果记manifest。

## F．文档一致性：PASS，历史口径按历史保留

[i18n.md:322](<../../i18n.md#L322>)现在 `200+11+32=243`，ALL/两包各243；[README:188](<../../../README.md#L188>)=243，[items.md:592](<../../world/items.md#L592>)=243。[quests.md:556](<../../world/quests.md#L556>)保留第5轮的TextKey201历史叙述，没有把当时计数改成当前值。[i18n.md:867](<../../i18n.md#L867>)保留R50历史结果与R51当前计数区分。

活动文档的旧HEAD/“尚未提交”已由礼部改述为历史取证基线和当时已提交状态，见[返工记录:49](<../../process/round51-libu-docs.md#L49>)及[当前T1索引:3](<README.md#L3>)。**T1原报告P09表仍保留当时发现**，末尾明确其13f7c78历史边界；不能声称v1正文已被重新终审为当前HEAD。本报告负责当前状态判断，既不覆盖历史也不把历史BLOCK误当f500269结论。

本人在v2落盘前复跑 `py -3 tools/docs/check_links.py`：**1039条相对链接、2篇README、exit0**；`py -3 tools/i18n/verify_text_keys.py`：**243键、32扩展键、两包一致、exit0**。新报告增加链接，落盘后的再次复跑计数单列manifest，不拿旧1039冒充新增文档后的数字。

## G．客户端与协议：核心PASS，表现与失败状态测试PARTIAL

相对716db42，协议crate及CI脚本无diff：商城250/251的类型、字段及生成定义未被本轮改动，R50冻结保持。客户端网络diff为两批消费分支的增加（首批+39，返工+27），没有删除或重写既有走/跑/攻击分支。150/151/152/153、127/128、122均有真实处理，详见[B2包消费:5125](<../../../crates/mir-client/src/net.rs#L5125>)。

本人执行且已收取后台作业结果：`cargo test -p mir-client --test round51_spells_wire`，**2/2通过，exit0**，无环境受限。旧net28/ui197及B2的29 magic/179 server等只作为原日志旁证，不声称本人重跑。没有运行截图、资源播放、原生dash帧或全套UI；真实状态链成立不等于视觉对齐完成。

## H．P01–P10映射、处置与残余风险

| P编号 | T2状态（closed/partial/open） | 仍开项及处置 |
|---|---|---|
| P01 | partial | PoisonCloud耗材/语义未实现，Unsupported真拒绝；可接受固定PARTIAL，后续资源/耗材批完成。 |
| P02 | partial | TrapHexagon耗材/语义未实现，同上；可接受固定PARTIAL。 |
| P03 | **open** | **Freezing模板/buff漏计，必须修复后复审，不能在本轮无授权降级。** Frost玩家目标可单独接受PARTIAL；真实施法上毒非空、玩家attack断言列下一轮必补。 |
| P04 | partial | 前置/零步计时/推人/两类聊天已修；火墙逐对象语义、InTrapRock缺系统为明确可接受PARTIAL；非零StepCounter回归必须R52补。 |
| P05 | closed | 捕获施法地图、地形、跨图旧坐标结算、禁传保留雷击已核；StormEscape专门跨图运行例可R52补，不能标本人已跑。 |
| P06 | closed | 真实TemporalFlux30s/+30%、自机124、生产效果/修炼/buff顺序已核；默认MagicInfo与严格包序回归必须R52补。 |
| P07 | partial | 真实World→帧→decode→快照链已建立并独立2/2通过；152/153应以错误预测格和已有motion证明消费不可删除，必须R52补。 |
| P08 | partial | 127/128消费缺口已修；Walking/Walk借用、原生dash视觉与时序未验证，明示可接受表现PARTIAL。 |
| P09 | closed | 活动文档时态与243计数修正，v1按历史快照保留；当前事实以本v2为准。 |
| P10 | **open** | **B2矩阵/摘要撤回不实覆盖与全关闭声明；必须在验收前修正。** 真MagicInfo、概率两侧、非空施毒、失败回纠/队列、包序等测试补证优先R52。 |

对主单的建议：当前 **BLOCK，保持返工/复审，不推进Done**。P03汇总与P10证据声明修正后，若针对变更重验且有权验收接受下列范围，可考虑 **PASS-WITH-PARTIAL**：P01/P02固定拒绝、Frost玩家目标、冲撞火墙简化、无InTrapRock、Walking借帧/原生视觉未验，以及明确登记到R52的测试残余。不能以“未决无”或“门禁全绿”替代这份清单。

与v1关系：[T1终审](<round51-final-audit.md>)审13f7c78并BLOCK，本报告独立审f500269；承认P04/P05/P06和真实包链已实质返工，也保留新发现和未关闭项。v1、attempt1–4、B/B2原件均未覆盖。原始target证据未进git，异机复核需单独保全；manifest是索引与hash，不能恢复不存在的原件。

复现命令与所有原始日志hash见[v2 manifest](<final-manifest-v2.json>)；manifest记录本报告最终SHA256，本报告不嵌自身SHA，避免循环自引用。manifest自身最终SHA由刑部向Lead回报登记。
