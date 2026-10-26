# R53 刑部终审（T）

**Verdict：PASS-WITH-PARTIAL。** 受审 HEAD 为 `eb72958170ac9d7de4fcf467137fff8d096efa09`，与本地 `origin/main` 相同。Frost 真实玩家目标链和 F2 跑步双向同步可信；Lead 唯一全量门禁 attempt=1 的字节、哈希、12 项成功汇总与 meta 自洽。AOI 保持 **3 PASS / 5 PARTIAL / 0 BLOCK，全量不可关闭**。F 报告“无 PARTIAL/全部闭合”只接受为核心链完成；精确等级 +2 正侧、同会护栏仍有定向测试盲区。

日期：2026-10-26；刑部子单 `JJC-20261026-001-sub-391bea87`。三件骨架先落盘再补证据；本审计只写本报告、[清单](final-manifest.json)及[门禁摘要/独立复跑原输出](../../../target/round53/final-gate-summary.log)。没有修改 crates、Crystal、既有审计原件、主单状态，没有 commit/push，没有运行全量门禁。交付路径采用本次直接授权，优先于旧派单的 target/final-manifest 与 summary 旧路径。技能的额外 HTML 格式因三件写范围未采用。

## A–H 核验

| 项 | 判定与证据 |
|---|---|
| A 门禁真实性 | T 自己的 Node `fs.readFileSync` / `crypto.createHash('sha256')` / `TextDecoder('gb18030')` 核验原字节：193575B，`fdd252e00918bab434f9eda526a515b297158c97050f178fe9582a64974ef2a4`。内存解码，未转码覆盖。[meta](../../../target/round53/final-gate.meta.txt) 记录 attempt=1、head=eb72958、exit=0、UTC 00:49:21–00:53:34，命令 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`。逐项12行 OK 与末个非空行“全部通过（12 项）”已直接检查，详见摘要。**限制：**各项 OK 是脚本成功分类，不能当作12个独立原始 Code=0；HEAD/总exit/起止由 meta 提供，主日志不能独立证明运行身份。253s 为起止推算。 |
| B F/F2 真实性 | T 在受审 HEAD，设置 `TMP=TEMP=$PWD/target/tmp` 串行独立复跑六命令，均 exit 0、非零实际执行，见下表。真实 cast→延迟 tick→HP、包、毒及真实 attack 拒绝/恢复均有可失败断言；F2两例真实 run，断言指定ID包数/零幽灵/精确双方集合；历史删sync变异两例均失败。Frost +2 正侧精确边界及同会定向场景保留盲区，不把源码核对当实跑。 |
| C AOI 诚实性 | [A2](../../process/round53-aoi-acceptance-v2.md) 3/5/0 可维持且均限矩阵子范围：PASS=移动/跑步、网格/边界双向差分、基础门生命周期。进图/离图/切图/刷出回收/广播五条 PARTIAL 没有被 F2 吞并。A2 maps 日志20 reported passed中四次明确缺 Server.MirDB 提前return，实际只计16合成例。C9源码 `None, 1000` 实为金币，不证明普通物品回收。AOI全量不可关闭。 |
| D PARTIAL 完整性 | 下方承接R52十二项，并另列 EnemyGuild、AOI五路由和门等子边界。R52第4项“仅注入毒”被R53真实施法证据替代，不能再机械写成未修；同时保留F覆盖盲区。125、真socket、离线/异图、B4/C证据残留均可追溯。 |
| E 证据链 | F实际目录是 `target/round53/frost-pvp/`，并无要求简写中的 `f/`；没有移名。F meta18项当前文件哈希全匹配；旧world哈希另对617e202 Git blob匹配（F2后当前值变化合理）。F2 SHA表13项匹配；A v1两日志匹配；A2 evidence递归64个哈希声明匹配（包含重复文件声明）。候选10文件逐字节大小/哈希入清单。T原始输出直接存于摘要，不另造第四证据文件。 |
| F 治理留痕 | 先读计划→门下→派单→补派→A1/A2，再实际 CLI recall/show。F=sub-61e3f0eb→617e202；A=sub-0ac5e1d8→A1 BLOCK文档（eb72958入库）；F2=sub-7d64c20b→48aaec0；A2=sub-e8afe1c8→A2文档（eb72958入库）；T=sub-391bea87→本三件，未提交。原A1保留BLOCK历史，A2仅解除run阻断。F增加world返回值超出最初表列magic+新test，但源码结算未改、独立评审明确核验、Lead已提交；记为已披露实现范围调整，不是审计方越权修码。 |
| G 入库卫生 | 初检 tracked diff为空；porcelain为两份历史大冻结JSON及T新建的 `docs/testing/round53/`（不能声称落盘后“只有两JSON”）。Git差异10个本批候选均<1MiB，未跟踪target；候选路径没有.env/密钥文件，改动文本未见凭据。历史JSON不读取内容、不纳入提交。T无暂存/提交动作，tracked干净；最终静态检查回执见交付说明。 |
| H 假绿/捷径 | 本批crates diff无新增 `#[allow]`/`#[ignore]`、无删除assert、无新增提前return跳过。字符串“skip both poison rolls”是失败诊断，不是跳过控制流。新增12处 `Point::new` 均在合成测试fixture；产品修复无硬编码坐标捷径。A2既有资源skip被明示排除，不能笼统声称整个workspace无skip。Crystal原件抽核至少8处，见下文。公式/RNG分支未改，怪物回归实跑通过。 |

## 独立复跑

仓库根，统一前缀 `TMP=TEMP=$PWD/target/tmp`；原始输出和UTC起止均保存在摘要。以下通过数不能简单相加：随机对拍是7个AOI库测试的同一例复跑。

| 精确命令 | UTC起止 | exit | 实际结果 |
|---|---|---:|---|
| `cargo test -p mir-server --test round53_frost_pvp -- --nocapture` | 00:57:02–00:57:06 | 0 | 7 passed / 0 failed / 0 ignored / 0 filtered |
| `cargo test -p mir-server --test round52_regression` | 00:57:06–00:57:07 | 0 | 8 passed / 0 failed / 0 ignored / 0 filtered |
| `cargo test -p mir-server --lib magic` | 00:57:08–00:57:08 | 0 | 43 passed / 0 failed / 153 filtered |
| `cargo test -p mir-server --test aoi -- --nocapture` | 00:57:08–00:57:09 | 0 | 8 passed / 0 failed / 0 ignored / 0 filtered |
| `cargo test -p mir-server --lib aoi::tests` | 00:57:09–00:57:09 | 0 | 7 passed / 0 failed / 189 filtered |
| `cargo test -p mir-server --lib random_operations_match_bruteforce_scan` | 00:57:09–00:57:10 | 0 | 1 passed / 0 failed / 195 filtered |

Frost固定种子3/5/7/11，每种300次，共1200真实施法；Slow=43命中/1157未中，Frozen=58/1142；期望48、区间24..72。断言既有成功/失败两侧又限制频率，能拒绝明显错误分母。这里没有独立再做变异；F2变异是保留的历史原件，2 failed / 0 passed，left=0/right=1，SHA见清单/摘要。

## Crystal 与 Rust 抽核

| 原件锚点 | 实际内容与对照 |
|---|---|
| [HumanObject.cs:2596](../../../github/Crystal/Server/MirObjects/HumanObject.cs#L2596)–2602 | run摘旧格→RemoveObjects→换位置/加格→AddObjects；Rust [world.rs:2241](../../../crates/mir-server/src/world.rs#L2241) 补玩家差分，与walk:2140同序。 |
| [HumanObject.cs:4547](../../../github/Crystal/Server/MirObjects/HumanObject.cs#L4547)–4559 | 实际是PoisonCloud护符/毒各5与延迟注册，**不是FrostCrunch锚点**；用于核对承接旧耗材范围，未冒充本轮Frost证据。 |
| [HumanObject.cs:5967](../../../github/Crystal/Server/MirObjects/HumanObject.cs#L5967)–5994 | 同图、IsAttackTarget、Node、落点2；MAC>0门控；玩家+2/100分母、Slow4/Frozen2、1000ms。Rust [magic.rs:4098](../../../crates/mir-server/src/magic.rs#L4098)、:4126/:4131/:4150逐项吻合，F提交未改这些概率/时长分支。 |
| [HumanObject.cs:7214](../../../github/Crystal/Server/MirObjects/HumanObject.cs#L7214)–7220 | Struck/ObjectStruck、HP变化及return damage-armour；Rust [world.rs:3198](../../../crates/mir-server/src/world.rs#L3198) 返回i32，并从既有结算返回dealt，早退0；怪物回归未受损。不能把返回值解释为扣到0后截断的HP差。 |
| [PlayerObject.cs:4650](../../../github/Crystal/Server/MirObjects/PlayerObject.cs#L4650)–4674 | Peace/Group/Guild/EnemyGuild/RedBrown护栏；Rust [world.rs:2503](../../../crates/mir-server/src/world.rs#L2503) 复用既有谓词，EnemyGuild恒false继续PARTIAL。 |
| [PlayerObject.cs:1741](../../../github/Crystal/Server/MirObjects/PlayerObject.cs#L1741)–1768 | y/x/cell插入序首帧；Rust按类型分组，不能宣称逐包等价。 |
| [MapObject.cs:408](../../../github/Crystal/Server/MirObjects/MapObject.cs#L408)–452 | 广播逆序玩家/排自身/距离过滤及攻击类型分派；Rust范围查询顺序差异仍列广播PARTIAL。 |
| [Map.cs:2437](../../../github/Crystal/Server/MirEnvir/Map.cs#L2437)–2448 | 广播按Players逆序，区别Rust `(y,x,id)` 查询；现有AOI文档ID序/空间序陈述需后续勘误。 |

F测试锚点：[真实HP/延迟](../../../crates/mir-server/tests/round53_frost_pvp.rs#L435)、[落地重检](../../../crates/mir-server/tests/round53_frost_pvp.rs#L487)、[等级/概率](../../../crates/mir-server/tests/round53_frost_pvp.rs#L554)、[真实拒攻/恢复](../../../crates/mir-server/tests/round53_frost_pvp.rs#L658)、[落点2/3](../../../crates/mir-server/tests/round53_frost_pvp.rs#L807)。

## PARTIAL 逐条承接

依据 [R52尚书验收第44–59行](../../process/round52-shangshu-acceptance.md#L44)，逐项保留，不以R53局部绿灯替代：

1. ObjectProjectile125无服务端生产者，手造负向包不证明生命周期；149/26已证子项不外推。
2. World→编码/解码→快照是进程内链；真socket与真实客户端窗口待验收。
3. 原生dash视觉/音频/时序、Walking借帧等价性未闭合。
4. **R53已修核心链：**真实PvP Frost玩家HP、毒、拒攻恢复已通过；旧“只有注入毒”的缺口被替代。精确+2正侧、同会专场及更全PK矩阵未实测，不称全Crystal PvP等价。
5. G1/G6总体open；根装备耗材/存档和快照子项不关闭全部边界，R51 B4补证保留。
6. D3离线/异图安全清场、丢pending与Crystal留场停伤/保留ActionList差异继续接受并追踪。
7. D2 Trap外围坐标安全剪裁差异保留。
8. FireWall26：B3具体ID替换/到期已证，C仅创建及新入场0补发，未断言撤销。
9. 无InTrapRock系统；FireWall逐对象ProcessSpell/InTrapRock延后。
10. B4：先施法后change_map丢pending、750ms不传送；先换图后施法落当前图。89耗蓝=65+8×3；TemporalFlux30%为另一分支。
11. guild、1965–1966嵌套槽来源、旧技能耗材顺延；本轮不扩展EnemyGuild/行会战，仍PARTIAL。
12. C首次lifecycle/fmt失败原件未保全；F1 reconstructed-failure是反事实重建。B4首次fmt/clippy红日志被覆盖，当前绿不能恢复历史。

AOI五条：[A2第59–63行](../../process/round53-aoi-acceptance-v2.md#L59)——进图完整四类/自身排除/包序；离图seen/索引/后续不可达/重复remove；切图玩家精确集/旧NPC/旧图隔离；普通物品和NPC回收/精确撤销；广播跨图负例/接收者身份/完整包序。门远端、精确5000ms、跑步撞门、初进图已开门快照、多实例同名图继续未覆盖。四个真实maps资源用例排除，资源就绪后重跑，不能算通过。

## Findings（按严重度，5条）

1. **中等，非阻塞，F验收覆盖口径偏强。** F摘要称“+2边界/无PARTIAL”，七例在等级上只测60→63拒毒和60→60概率，没有60→62正侧；无同会护栏专场。修改比较为错误的更紧阈值可能逃过现有边界用例。源码当前正确，核心链已实跑，因此保留为覆盖PARTIAL。复核办法：后续新增恰好+2的成功/失败侧种子及同会真实施法拒绝断言，先list确认非零。本审计不改代码。
2. **中等，非阻塞，AOI五路由仍有缺口。** 精确集合、回收和身份/包序不完整，资源四例提前return。A2已诚实披露，维持3/5/0，禁止关Roadmap。复核按A2建议先补具名断言再跑，不以0 tests当通过。
3. **轻微，文档顺序契约陈旧。** [AOI文档:155](../../world/aoi.md#L155) 写广播ID序，:175写差分空间序，与查询/集合差实际不同；首帧分组与Crystal插入序也非等价。本轮不改原件，R54勘误并补序列断言。
4. **轻微，历史证据保全不足。** F摘要明确首轮日志被最终复跑覆盖；R52 B4/C首次失败原件丢失承接不变。不得把当前绿/重建日志写成历史原始记录；各次新attempt今后独立保全。
5. **轻微，治理与路径偏移已披露。** F新增world返回值未在最初F表写范围列出，后独立评审及Lead提交明确接受；T交付路径依本次直接指令调整。F子单终态后补强结果写板被拒（exit3）保留原文，本次不冒名/不重开主单，最终结果交Lead核验。

## 历史失败与超时链

- F独立评审先PASS-WITH-CHANGES：初版320样本仅双侧非零不能拒错误分母；后增为1200和24..72区间，当前7例复跑通过。F摘要明确“日志已覆盖为最终版”，故早批字节不可恢复，未伪称全部attempt原件齐全。
- A1 BLOCK是静态确认run漏玩家同步，未伪称当时失败测试；F2后新增两例证明修复。F2去掉sync的反证2 failed/exit101；新增例用2000ms曾CannotRunYet/exit101，因idle reset而900ms合规。原怪物测试被replace_all误伤，已恢复2000；最终提交只增161行测试、0删除，不认作放宽既有断言。
- 前尚书补派子代理超时未交付由补派文书明确记录；本审计没有独立运行时超时原件可追加证明，不杜撰时间/退出码。F终态progress/delegate-result拒绝exit3见历史原件，非代码失败。
- R53最终全量仅有attempt=1被提供且为PASS；T未发现/未制造其它R53最终门禁attempt。R52历史attempt1失败→attempt2/3/4通过链仅按上轮验收承接，未复跑/重新验证其全部原件。
- 本T落盘工具前两次因参数错误拒绝零写入；一次误名骨架已核实绝对路径后删除，最终仅三件。没有借失败覆盖任何历史证据。

## 提交建议与未执行项

建议Lead仅显式纳入本报告与清单，target摘要保留本地不入库；受审10文件已在现有链内，不重复暂存。T已执行 `py -3 tools/docs/check_links.py`，exit0，1651条相对链接/2篇README引用核验通过；`git diff --check` exit0，tracked diff为空。提交应为纯审计文档，不改代码/测试/CI/配置，不能把AOI改PASS/Done。历史大冻结JSON、target、.env均排除。后续尚书验收与门下确认由对口角色处理，T不代最终确认。

T未执行全量门禁、代码变异、真实Crystal程序对拍、资源依赖四例补跑、真socket/真实客户端窗口、精确+2/同会新增测试；这些均未计通过。Toolchain：rustc 1.99.1 (58cf8f9c3 2026-10-13)、cargo 1.99.1 (5f94df478 2026-08-27)、host x86_64-pc-windows-msvc、LLVM23.1.1、Node v24.18.0。最后三件字节/哈希通过独立读取交Lead，清单不包含自身哈希，避免自引用。
