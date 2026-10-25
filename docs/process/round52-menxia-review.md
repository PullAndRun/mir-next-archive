# R52 门下省审议：批准派发，按裁决落实验收契约

主单 `JJC-20261025-004`；门下省 / gpt-6.1-sol；2026-10-25（Asia/Shanghai）。本审议仅写流程文档与看板，未运行 cargo、未执行 Git 写操作、未启动实现代理。
审议对象：[中书方案](round52-plan.md)，130 行 / 25,422 B / SHA256 `256902b355d28e255c24b1f92315a0a7378bb2e29973f6f91078b6a701906e10`；正文保持原字节，本裁决补充派发契约。

## 1. 核验事实与总体裁决

- 已通读方案；仓库 main，HEAD=origin/main=`f4e4497d132b356261c84ecb24d5db8a66ae449d`；审议初始主单 Menxia，CLI validate 为 116 单、schema v3.1、issues 空。
- R51 主单 `JJC-20261025-003` 已 Done；[终审 v4 §D](../testing/round51/round51-final-audit-v4.md)、[尚书验收](round51-shangshu-acceptance.md)的 11 项残余仍有效。P03/P10 已 closed，不可重写历史。
- [R51 gate8 原日志](../../target/round51/final-gate8.log)原字节 188,937 B，SHA256 `817f531c078679bb8d4519576b1a155f2119aae3b92e7fcfec502fda6ff28510`；[meta](../../target/round51/final-gate8.meta.txt)含 command/cwd/start/end/exit/head，exit=0、HEAD=`8cbe71332ff3dc2037b9bea4ab564f6fb3ff0e75`；[转码摘要](../../target/round51/final-gate8-summary.log)记录12项。原件为gb18030，未改写；历史门禁不能证明R52。
- **批准 Menxia→Assigned，交尚书登记七子单；M01–M08 是执行/验收硬契约，必须逐条回填。批准派发不表示功能已完成。**

## 2. D1–D5 逐项裁决

| 决策 | 裁决 | 理由与约束 |
|---|---|---|
| D1 范围及七子单 | **批准** | A/B1/B2/B3/B4/C/T覆盖主线与可独立补证项；G1–G3/G6核心有明确失败条件，G7为交付必需。G4/G5逐字段closed/PARTIAL；不得因时间不足静默删除主技能或空跑补证。 |
| D2 扣蓝顺序及Trap范围 | **修改后批准** | 保留公共扣蓝后检查材料、失败cast=false且不写本技能CastTime；补齐M02状态/包反例。保留3×2预检与3×3落地；地图坐标剪裁属明确安全偏差，按M03验边缘、重检与Shock。 |
| D3 施法者离线/异图 | **批准** | 接受本作离线/异图安全清场，独立登记生命周期PARTIAL。已补调用链：失联Caster置null，IsAttackTarget先拒null，原毒雾通常留场至到期而停伤；异图只有FireWall强制清场。不得以“Crystal必然空指针”或“所有场都如此”解释本作偏差；M04分别验证延迟与存量场。 |
| D4 仅新两技能接耗材 | **批准** | 避免扩大旧职业语义；其它SoulFireBall/buff/Hiding/Summon/Poisoning耗材另轮，guild/嵌套插槽PoisonAttack/Freezing来源继续PARTIAL。根装备模板/附加/生效buff的本轮承诺必须实证，不得随延期来源省略。 |
| D5 缺样本/专属AI/素材处置 | **修改后批准** | 缺默认记录只使对应G4项PARTIAL并点名；缺日志、零测试、伪fixture属BLOCK。专属AI漏控制、G6场生命周期缺失等核心漏洞返工；裁剪支持模板须门下另裁，不预授权降格。原生视觉/声音可延期，149/125消费与快照不可延期。 |

## 3. 必改项（尚书逐条纳入启动契约）

| 编号 / 严重度 | 必须落实与失败判据 |
|---|---|
| M01 / 致命 | B1+C联验装备UUID：部分扣数、耗尽摘槽、背包同模板不误扣、错误UUID不动；每件成功扣一次且精确Count，失败不发DeleteItem。服务端真实产包→encode→decode→WorldSnapshot，再核生产角色快照/既有存储/重登一致；仅背包快照或手造成功包即BLOCK。 |
| M02 / 高 | B2/B3分别构造缺符、缺毒、错shape/type、单堆不足、背包有而装备无、两堆合够但单堆不够、0耐久材料。缺第二项不得先扣第一项；材料失败仍扣公共MP，保留已写公共动作/SpellTime，cast=false、不写本技能CastTime、不排延迟效果/场。未学/越界/冷却/缺蓝走原路径，禁止统一退款/失败包。 |
| M03 / 高 | Trap预检只扫x中心±1、y中心-1至中心；单独放目标于y+1验预检不命中，再以预检内目标使落地扫到第三行。等级+2/+3、预检后死亡/移图/移位重检；外围合法格剪裁。Shock非Frozen：禁移动、远距清目标，CanAttack不能全局按Shock封死；有效受击解除，护甲早退不解除，并核攻击入口/守卫/召唤物/专属AI。 |
| M04 / 高 | 毒雾按有效格3×3且同格同类不叠；距离延迟、首个实际生成对象Show、训练次数、首跳与6秒/6秒后边界均断言。tick先MAC伤害，存活才12跳绿毒；后者读取当时有效SC与已算BonusDmg。覆盖安全区/PK双方、双方死亡/换图/下线；离线/异图在pending与已铺场阶段均停活体取值并清理，149/125及入视野补发对应。死亡语义另按调用链记录，不凭离线规则推导。 |
| M05 / 高 | B1原语维持pub(crate)：私有单测放新模块内，integration tests走公开真实施法/会话链，禁为测试任意扩public。B1可先证原语/生产存档链，完整施法交B2/B3+C闭环；Session私有current_row不能外调。生产链需越界文件时尚书先登记精确增补；禁手造UserItem序列化替代重登。 |
| M06 / 高 | G4先断言实际默认七记录及字段/来源SHA，再测MP与cooldown-1ms/到期；禁换Crystal假样本。G5真实施法Slow/Frozen非空、固定种子命中/未命中计数均非零；Freezing7上界11，零值不消费RNG，坏装备+buff、多根槽/附加分别验。五位置先is_some再排序，StormEscape需真实升级与跨图。 |
| M07 / 高 | 尚书先登记7真实子单及depends_on/路径/模型/验收/证据，再向Lead交启动契约；一次只唤醒一名成员。编码含测试仅deepseek，由Lead spawn_teammate；gpt用subagent(provider=sol-relay,model=gpt-6.1-sol)做文档/评审/审计，禁并行多gpt。B2/B3/B4共享技能文件串行，审阅后唯一Lead提交。 |
| M08 / 高 | 定向命令须命中named tests、实际运行≥1、0失败；保留真实exit及失败史。单证据文件和受控入库批≤1MiB；超限连续分片+原SHA，不截尾。门禁后仅审计/过程文档/manifest，核与被测HEAD仅文档差异；改源码注释/断言/工具/配置仍使门禁失效。 |

## 4. 静态锚点复核（行号以本次HEAD为准）

本次读取并抽查16组；Rust缺口是待实现事实。方案部分行号指向注释/区块入口，以下补准。

| # | Crystal 文件与行 | Rust 现状与判定 |
|---|---|---|
| 01 | [HumanObject:4543](../../github/Crystal/Server/MirObjects/HumanObject.cs#L4543) | [magic:599](../../crates/mir-server/src/magic.rs#L599)、[magic:1265](../../crates/mir-server/src/magic.rs#L1265)：两技能仍Unsupported；毒雾符5+shape1毒5、距离×50+500需新增。 |
| 02 | [HumanObject:4596](../../github/Crystal/Server/MirObjects/HumanObject.cs#L4596) | [magic:599](../../crates/mir-server/src/magic.rs#L599)：Trap预检3×2、符1、500ms尚无实现，放开Unsupported不算完成。 |
| 03 | [HumanObject:6859](../../github/Crystal/Server/MirObjects/HumanObject.cs#L6859) | [items:958](../../crates/mir-server/src/items.rs#L958)、[items:2451](../../crates/mir-server/src/items.rs#L2451)：6/9可装备，consume_one仅背包；根装备递增单堆搜索需新原语，不套Freezing耐久过滤。 |
| 04 | [HumanObject:1346](../../github/Crystal/Server/MirObjects/HumanObject.cs#L1346) | [net:5473](../../crates/mir-client/src/net.rs#L5473)：Crystal扣Count、发DeleteItem、归零摘槽；客户端现仅delete_inventory_item，是实质交叉验收缺口。 |
| 05 | [Map:1434](../../github/Crystal/Server/MirEnvir/Map.cs#L1434) | [magic:524](../../crates/mir-server/src/magic.rs#L524)、[magic:2694](../../crates/mir-server/src/magic.rs#L2694)：现为火墙型场；毒雾3×3、6000ms/1000ms及去重需独立分支。 |
| 06 | [SpellObject:150](../../github/Crystal/Server/MirObjects/SpellObject.cs#L150) | [magic:2694](../../crates/mir-server/src/magic.rs#L2694)：现有场不能证明MAC后存活绿毒12跳、当前SC及BonusDmg，需真实tick反例。 |
| 07 | [Map:1752](../../github/Crystal/Server/MirEnvir/Map.cs#L1752) | [monster:1101](../../crates/mir-server/src/monster.rs#L1101)、[world:5707](../../crates/mir-server/src/world.rs#L5707)：未有Shock链、ObjectMonster.shock_time=0；需目标绑定八外围场与剩余时间。 |
| 08 | [SpellObject:55](../../github/Crystal/Server/MirObjects/SpellObject.cs#L55)、[MapObject:428](../../github/Crystal/Server/MirObjects/MapObject.cs#L428) | [magic:2694](../../crates/mir-server/src/magic.rs#L2694)：失联先null、IsAttackTarget:430拒null；不可套火墙生命周期。 |
| 09 | [SpellObject:82](../../github/Crystal/Server/MirObjects/SpellObject.cs#L82) | [magic:2694](../../crates/mir-server/src/magic.rs#L2694)：强制换图清场原分支仅FireWall；D3毒雾清场保留偏差。 |
| 10 | [MonsterObject:634](../../github/Crystal/Server/MirObjects/MonsterObject.cs#L634)、[MonsterObject:650](../../github/Crystal/Server/MirObjects/MonsterObject.cs#L650)、[MonsterObject:1858](../../github/Crystal/Server/MirObjects/MonsterObject.cs#L1858) | [monster:1275](../../crates/mir-server/src/monster.rs#L1275)、[monster:1288](../../crates/mir-server/src/monster.rs#L1288)、[monster:1720](../../crates/mir-server/src/monster.rs#L1720)：尚缺Shock；原CanAttack无Shock判断，不等同Frozen。 |
| 11 | [MonsterObject:2588](../../github/Crystal/Server/MirObjects/MonsterObject.cs#L2588)、[MonsterObject:2613](../../github/Crystal/Server/MirObjects/MonsterObject.cs#L2613) | [monster:1101](../../crates/mir-server/src/monster.rs#L1101)：原装甲挡住先早退，有效伤害才ShockTime=0；新状态须接实际受击路径。 |
| 12 | [Envir:1097](../../github/Crystal/Server/MirEnvir/Envir.cs#L1097)、[Envir:1247](../../github/Crystal/Server/MirEnvir/Envir.cs#L1247)、[Envir:333](../../github/Crystal/Server/MirEnvir/Envir.cs#L333) | [gamedata:653](../../crates/mir-server/src/gamedata.rs#L653)：Trap7/3/9，毒雾30/5/9、18000-2000×level，Dash2500有原锚；默认记录尚未运行证明。 |
| 13 | [HumanObject:1858](../../github/Crystal/Server/MirObjects/HumanObject.cs#L1858)、[HumanObject:1965](../../github/Crystal/Server/MirObjects/HumanObject.cs#L1965) | [magic:3138](../../crates/mir-server/src/magic.rs#L3138)：根模板+added为1858/1859，插槽另1965/1966；[旧测试注释](../../crates/mir-server/src/magic.rs#L5823)普通added错引须B4勘误。 |
| 14 | [HumanObject:5987](../../github/Crystal/Server/MirObjects/HumanObject.cs#L5987) | [magic:3156](../../crates/mir-server/src/magic.rs#L3156)取0..freezing-1；[magic:6195](../../crates/mir-server/src/magic.rs#L6195)旧5..=12宽一，应收紧5..=11并运行。 |
| 15 | [HumanObject:149](../../github/Crystal/Server/MirObjects/HumanObject.cs#L149) | [world:2275](../../crates/mir-server/src/world.rs#L2275)为Frozen真实拒攻；方案2273为注释入口，仍需实际attack补证。 |
| 16 | [HumanObject:1346](../../github/Crystal/Server/MirObjects/HumanObject.cs#L1346) | [session:3428](../../crates/mir-server/src/session.rs#L3428)快照读世界活值，current_row私有；[lib:20](../../crates/mir-server/src/lib.rs#L20)加模块不代表外部测试可访问pub(crate)。 |

## 5. G目标、子单与路由/写范围

| 目标 | 主责/协同 | 必须可失败的验收点 |
|---|---|---|
| G1耗材 | B1；B2/B3/C | 装备顺序、双材料原子、Count/UUID/归零、生产存储重登与客户端相等；M01/M02/M05。 |
| G2毒雾 | B2；C | 延迟、3×3、SC/PoisonAttack及零RNG、tick/12跳、PK安全区/边界/生命周期；M04。 |
| G3 Trap | B3；C | 3×2/3×3反例、等级、Shock实际AI/释放、中心/外围对象、入视野与撤销；M03。 |
| G4默认数据 | B4 | 实际默认七记录、独立Crystal期望、MP/冷却边界与SHA；逐记录closed/PARTIAL。 |
| G5 R51补证 | B4 | Slow/Frozen玩家怪物双侧概率、拒攻、属性来源/RNG、StormEscape五位置/跨图；逐项统计。 |
| G6客户端 | C；B1/B2/B3 | 真实产包编解码快照、装备消耗/149/125；152/153错误预测+既有motion/队列，两分支有敏感反例。 |
| G7文档 | A；T/尚书 | R50/R51终态与R52证据分时点，11项逐条closed/PARTIAL/顺延；计数据实际结果。 |

| 子单 | owner / 模型与启动 | 批准写范围 / 前置 |
|---|---|---|
| A | 礼部 / gpt；Lead用上述subagent路由 | 方案A列总览/roadmap/client/magic/i18n/体验清单、R52索引；R51索引仅必要同步。批准后先文档批提交。 |
| B1 | 户部 / deepseek；仅Lead spawn_teammate | 新耗材模块+[lib模块声明](../../crates/mir-server/src/lib.rs)+新耗材测试；模块内私有单测允许；A提交后。 |
| B2 | 兵部 / deepseek；仅Lead spawn_teammate | [技能文件](../../crates/mir-server/src/magic.rs)+新毒雾测试；B1审阅/提交后，其它路径先登记。 |
| B3 | 兵部 / deepseek；仅Lead spawn_teammate | [技能文件](../../crates/mir-server/src/magic.rs)、[怪物文件](../../crates/mir-server/src/monster.rs)、[世界文件](../../crates/mir-server/src/world.rs)+新Trap测试；B2后。 |
| B4 | 兵部 / deepseek；仅Lead spawn_teammate | [技能文件](../../crates/mir-server/src/magic.rs)测试/引用勘误+新回归测试；B3后；生产修复单列。 |
| C | 工部 / deepseek；仅Lead spawn_teammate | [客户端网络](../../crates/mir-client/src/net.rs)+新wire测试；[R51 wire](../../crates/mir-client/tests/round51_spells_wire.rs)仅必要加强；B4后。 |
| T | 刑部 / gpt；Lead用上述subagent路由 | 仅R52终审/manifest/索引及target/round52/t；产品只读；C完成+Lead门禁后最终裁决。 |

DAG：批准→A→B1→B2→B3→B4→C→Lead唯一全量门禁→T→尚书验收→门下指定确认；各编码批后串行gpt独立评审。当前写范围无并发冲突，禁另开同文件并写或并跑共享DB门禁。

## 6. 验证口径、卫生、风险与交付物

- 已静态核对[server包](../../crates/mir-server/Cargo.toml)、[client包](../../crates/mir-client/Cargo.toml)，方案定向cargo命令语法成立；新增test target尚未创建，不能声称现已执行通过。B1可见性按M05；未来运行先列named tests，零命中失败。
- A执行links/TextKey/双i18n扫描；B1/B2/B3各自过滤器，B4过滤器+magic::tests，C过滤器+新旧wire；完整命令/cwd/时区/start/end/raw exit/HEAD/功能树SHA/实际测试数入证。本审议只做静态及文档检查。
- Lead唯一运行 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`；[脚本](../../tools/dev/ci.ps1#L107)当前默认分支实际12项，旧注释13不可作标准。逐项核名/产物，PG等不能skip，协议/links/i18n双扫描/legacy/icon保留；-KillStale尊重soak锁。meta必须有command/cwd/start/end/exit/head。
- 原证据全部target/round52/**、保持ignored；≤1MiB且可复算bytes/SHA，失败attempt保全。可入库仅受控文档/manifest及执行期登记源码，不含target/.env/凭据、[冻结JSON前件](../testing/monster-native/player-resource-freeze-before.json)、[冻结JSON后件](../testing/monster-native/player-resource-freeze.json)；禁git add .。
- 捷径黑名单：手造成功包/手工注毒、瞬时AOE冒充场、Frozen冒充Shock、只扫槽9、背包补材料、跨堆凑数、先扣符后验毒、耐久冒充Count、扩public绕测试、跳过安全区/移图/到期、搜到标识即完成、零测试/skip/放宽断言、删坏日志、改协议250/251、关闭门禁、gpt写测试、并发写共享文件，均禁止。
- PARTIAL仅已裁决边界：D3生命周期偏差、旧技能耗材、guild/嵌套槽来源、原生视觉/声音/真socket等。G4/G5按字段验，缺记录/未完成点名；核心失败/缺原日志/空断言皆BLOCK。T分别证明既定偏差实现正确和仍有偏差，两者不抵消。
- 明确不做：R51 P04火墙逐对象ProcessSpell/InTrapRock、P08原生dash表现、PetSave/PetExp、AOI全量、攻城/坐骑/天气/光照/攻击变招、CallDefaultNPC、行会投票/职位UI、R50商城残余；局部绿灯不覆盖这些系统。
- 交付：本审议；尚书七真实子单+启动契约+M01–M08落实表；各子单原日志/测试+指纹；Lead各attempt log/meta及冻结清单；T终审/manifest/R52索引；尚书验收与门下对口确认。审计后置提交须核功能树不变并重跑links/diff检查。

看板CLI回执另发Lead；尚书收到真实SUB_TASK_ID后登记七子单，本表草案名不等于已建单。
