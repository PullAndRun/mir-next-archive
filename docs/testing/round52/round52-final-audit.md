# R52 终审（六项限定范围）

**verdict: BLOCK**。功能门禁 attempt2 的 12 项通过及原始字节指纹成立；本次未发现需要改产品代码的阻断。阻断在 **G7 最终文档收口**：当前总览、路线图、技能/客户端文档和 R52 索引仍把已经提交且测试通过的交付写为“未实现/计划中”，两份流程文档也没有落地 PoisonCloud L3=10000 的勘误。仅凭此报告补充事实，不能把这些文档的一致性检查判为通过。

审计基线：`HEAD=origin/main=46be80bbd45e2eea18a73d1915b1258f7784c9b2`，main；时区 Asia/Shanghai。本次仅写两份终审文件及门禁摘要，不改产品、测试、门禁、阈值或历史日志，不重跑全量。JSON 清单保留原件指纹、抽样证据指纹、排除候选及复现命令。

## 1. 门禁：PASS；输入失败数量须纠正

| 原件 | bytes | SHA-256 | exit / HEAD |
|---|---:|---|---|
| [attempt1](../../../target/round52/final-gate.log) | 140961 | `be54c5dbb5ac1bc2bc1759db3d456b7edb5e3af6a817106146a564ff6ac6d11c` | 1 / 0d7b4a1 |
| [attempt2](../../../target/round52/final-gate2.log) | 192631 | `4da8ea80cc7a396e6690bac2ed68170a52a6ff824c7875577f4c9fd40d57bd42` | 0 / 46be80b |

两件 bytes/SHA 均独立复算；attempt2 与任务指定值吻合。[meta2](../../../target/round52/final-gate2.meta.txt) 记录 UTC `2026-10-25T20:23:09Z–20:27:15Z`、246s、main、完整命令 `pwsh -NoProfile -File tools/dev/ci.ps1`；对应的 HEAD 短号与当前完整 HEAD 一致。[显示副本尾部](../../../target/round52/final-gate2.utf8.log#L3281) 列 12 个 OK，末行“全部通过（12 项）”。脚本实际汇总 `$results.Count`，其顶部旧注释“13项”不改变本次实际 12 项。

attempt1 失败原件原样保留。原件检索 `final-gate.log:2197–2218` 可核实 monster_combat 是 **33 passed / 1 failed**，失败名 `rng_is_reproducible_for_the_same_seed`，随后 workspace FAIL；不能将任务输入的“monster_combat 4 红”当成这个原件的实测数。四个受影响的行走/逃跑/RNG 分支在修复说明及最终 34/34 中存在，但它们不等于该次门禁有四个 FAILED。i18n 失败史 103/2 保留在原始门禁及 [baseline 扫描](../../../target/round52/i18n/00-baseline-with-test.raw.log) 和 [no-test baseline](../../../target/round52/i18n/00-baseline-no-test.raw.log)。

原始门禁含混合编码，严格 UTF-8 reader 拒绝读取，未转写原件；SHA 针对原字节。尤其 [attempt1 UTF-8 副本](../../../target/round52/final-gate.utf8.log) **仅505行并截在 attack_variant 用例中**，不包含原件后面的失败及总结，不能作为完整原件替代。attempt1 meta 声明 duration220s，而时间戳相减221s，作为秒级计时差异保留，不据此篡改 meta。

## 2. 证据真实性：抽样通过，历史缺失透明保留

| 目录/证据 | 独立抽核结果与边界 |
|---|---|
| [b3-fix3 结果](../../../target/round52/b3-fix3/00-result.md) / [combat 原日志](../../../target/round52/b3-fix3/20-monster-combat.log) | command/start/end/exit齐；monster unit14、combat34/34，未把0命中视为通过。 |
| [i18n 最终扫描](../../../target/round52/i18n/10-with-test.log) / [no-test](../../../target/round52/i18n/11-no-test.log) | 66≤70、0≤0，与当前独立复跑一致；内部 archived baseline 明确为历史树。 |
| [C 原日志](../../../target/round52/c/02-test.log) / [C 复审](../../../target/round52/c-review/review-v2.md) | C执行2个named且2 passed；复审另有两次2/2、R51 wire2/2及生产耗材存档1/1。 |
| [B4 评审日志](../../../target/round52/b4-review/verify.log) / [最终回归](../../../target/round52/b4/02-regression.log) | 初评8/8带旧命名；五处摘要/注释/命名修正后最终日志8/8，玩家PoisonState注入与怪物真实施法拆名，未冒称玩家真实Frost。 |
| [B2c 日志](../../../target/round52/b2/b2c-tests-1.log) / [摘要](../../../target/round52/b2/b2c-00-summary.md) | cargo过滤会产生其它target的0测试段；对应PoisonCloud target实际8/8，不能只截首段0测试判空。初轮/b2b失败日志仍在。 |
| [B3 v3 复审](../../../target/round52/b3-review/review-v3.md) | 独立Trap7/7、PoisonCloud8/8、magic36；FireWall/Show-ID/专属AI反例有具体包ID与次数。目录旧20-trap.log为更早6例历史，不能误作v3七例日志。 |

有0字节日志但有合理语义：[B4 fmt](../../../target/round52/b4/06-fmt.log) 为 `cargo fmt --all --check` 成功无输出；配套 meta/运行摘要记录 exit0。这不是空测试日志，不能仅凭空 stdout 认定伪造。[B4摘要:56](../../../target/round52/b4/00-summary.md#L56) 同时披露开发期fmt/clippy红日志被复跑覆盖，无法恢复这些历史原件。

C F1 已按要求诚实标注：[重建声明:9](../../../target/round52/c/reconstructed-failure/03-README.md#L9) 和 [边界:26](../../../target/round52/c/reconstructed-failure/03-README.md#L26) 明确“原失败日志未保全、反事实重建、非历史原件”；原fmt红日志也未保全。复审接受的是透明披露，**不证明原历史失败已恢复**。本报告不扩大该接受范围。

逐份读取 M1–M5 原日志，全部实际编译完成、运行1 test、真实断言失败、exit101：

| 变异原件 | 实际断言 |
|---|---|
| [M1](../../../target/round52/c/09-mutation-M1-dashfail.log#L28) | test:1027，Walking != Standing |
| [M2](../../../target/round52/c/10-mutation-M2-deleteitem-only-bag.log#L11) | test:667，装备6没有扣成1 |
| [M3](../../../target/round52/c/11-mutation-M3-objectremove-keeps-field.log#L11) | test:814，26未摘同ID场 |
| [M4](../../../target/round52/c/12-mutation-M4-objectspell-not-registered.log#L26) | test:731，登记集合{} != {2} |
| [M5](../../../target/round52/c/13-mutation-M5-entry-resend.log#L12) | test:772，入场149补发0 != 1 |

M1 在1027就panic，未执行后面的1050清SelfMotion断言。[C摘要:97](../../../target/round52/c/00-summary.md#L97)“同时命中新增SelfMotion清理断言”过强；应读为同一补强用例被拦截。正常绿跑确有非空motion前置、300ms<600ms、生产prepare_frame后None/Interrupted，见 [复审F2](../../../target/round52/c-review/review-v2.md#L13)。此精度提示不升级为产品阻断。

## 3. PARTIAL 诚实性：局部摘要如实；总体验收不吞并

以下逐项保留，不以12项门禁全绿或局部PASS消除：

- **125无服务端生产者**：协议ObjectProjectile(125)不是撤销包；C仅手造负向包经过真实编解码，证明它不误删场，不是服务端正链路。真实场149生成/26撤销有生产者及消费断言。
- **无真socket**：World真实产包→encode/decode→WorldSnapshot为进程内链路，不能称真网络栈或真实客户端窗口验收；原生视觉、音频、dash及Walking借帧继续PARTIAL。
- **D3离线/异图**：本作安全清场/丢弃pending与Crystal留场停伤或保留ActionList不等价。毒雾已实测清理，但偏差保留。
- **PvP Frost玩家目标**：真实Frost施法仅怪物目标；玩家侧为PoisonState注入后真实attack拒攻，不是玩家被真实Frost命中。
- **G1/G6 open**：各批摘要的整体未闭合状态保留；根装备耗材及快照已证明的子项可以引用，但不据此自动关闭所有存档、125、socket与表现边界。
- **FireWall26归服务端证据**：C仅断创建与新入场0补发；C没有断FireWall撤销26。B3服务器按具体ID覆盖替换/到期26，不与Trap/PoisonCloud差分链混写。
- **B4 deviation**：先施法后change_map会丢pending，750ms不传送；先换图后施法则落当前图。这是两个不同场景，当前修订摘要已拆明。89耗蓝=65+8×3，TemporalFlux30%是另一分支。
- D2外围坐标安全剪裁、旧技能耗材、guild/嵌套槽1965–1966来源等仍保留授权边界。
- C与B4原失败日志缺失按第2项披露；不把重建、覆盖后绿日志称原件完整。

PoisonCloud实测默认L2=12000、L3=10000，B4 [摘要:98](../../../target/round52/b4/00-summary.md#L98) 已记录勘误；但它明确说 **plan/dispatch文件未改，待Lead批改**。检查 [plan:47](../../process/round52-plan.md#L47) 与 [dispatch:61](../../process/round52-shangshu-dispatch.md#L61) 仍是L3=12000，尚无相应10000勘误落盘，属于必改。

## 4. Crystal 五组抽核：PASS，纠正两处引用

| 组 | 实读源码与结论 |
|---|---|
| B1装备耗材 | [HumanObject:6859](../../../github/Crystal/Server/MirObjects/HumanObject.cs#L6859) GetAmulet、[6870](../../../github/Crystal/Server/MirObjects/HumanObject.cs#L6870) GetPoison均按装备索引递增，单件Count≥需求才返回，不合并多堆。任务指定5987实际为Frozen时长公式，不是耗材查找。 |
| B2有效格 | [Map:2480](../../../github/Crystal/Server/MirEnvir/Map.cs#L2480) `Valid` 的getter确为 `Attribute == CellAttribute.Walk`。 |
| B3 Trap/Shock | [HumanObject:4596](../../../github/Crystal/Server/MirObjects/HumanObject.cs#L4596) 施法预检x±1且y<中心+1，是3×2；真正3×3落地在 [Map:1760](../../../github/Crystal/Server/MirEnvir/Map.cs#L1760)，不是同一4596–4644段。落地写ShockTime且清Target；[MonsterObject:604](../../../github/Crystal/Server/MirObjects/MonsterObject.cs#L604)声明独立字段、[642](../../../github/Crystal/Server/MirObjects/MonsterObject.cs#L642)移动严格now>ShockTime、[1858](../../../github/Crystal/Server/MirObjects/MonsterObject.cs#L1858)受Shock时清目标并return。 |
| C DeleteItem | [GameScene:3629](../../../github/Crystal/Client/MirScenes/GameScene.cs#L3629)先背包及嵌套槽，再装备及嵌套槽，按UUID减Count/摘槽；“仅背包”不是Crystal完整语义。R52覆盖根装备，未冒称嵌套槽全等价。 |
| B4默认资料 | [真实默认文件](../../../game-assets/data/game.mnd)独立复算283257B/SHA `fb67712fe65099e3b0d484e9fe6132ff3dca71ade75c86322ad5e7a8eea0a460`；107条来自B4真实加载断言及8/8日志。[B4摘要:63](../../../target/round52/b4/00-summary.md#L63)记录同一默认加载链、cjlaaa来源及manifest字段比对，不是手造零冷却fixture。 |

## 5. 仓库卫生与修复：PASS，有准确性限定

实际git log最近9提交按正序为 `6f975fa → 9a72d16 → f1f2e22 → 4efe600 → 6310329 → d06a916 → 0d7b4a1 → 55e3593 → 46be80b`。任务指定八提交主链均在，另有f1f2e22派发勘误，不应漏写为严格连续八提交。

初始工作树仅两份历史未跟踪冻结JSON（44,844,165B /45,219,533B），本次后新增两份终审文档；产品没有dirty。target保持ignored，`git ls-files target .env '*/.env'`无输出。R52新增/修改候选无>1MiB文件。全仓并非没有大文件：[既有attack3计划](../attack-variants/attack3-asset-plan.json)为1,228,554B，6f975fa父提交已存在，本轮未动；不扩大为清理历史大文件任务。

`55e3593`仅改 [monster源](../../../crates/mir-server/src/monster.rs)，+40/−1，核心 [1297](../../../crates/mir-server/src/monster.rs#L1297) 为 `&& (self.shock_time == 0 || now > self.shock_time)`。0是未Shock哨兵，虚拟时钟now0不应被Shock拦住；其他dead/Frozen/action/move条件保留。[回归:3225](../../../crates/mir-server/src/monster.rs#L3225)令move/action=-1隔离Shock条件，验证0/0可移动，以及shock500时499/500不可、501可，旧实现会在0/0失败，未放宽真实Shock到期边界，也未删旧RNG测试。[最终combat日志](../../../target/round52/b3-fix3/20-monster-combat.log)34/34含四个受影响分支。

`46be80b`三文件39增39删，diff逐项为字符串英文化，无数值、条件、控制流或assert结构改变，扫描器/阈值未改。**“仅测试字面量”不完全准确**：还翻译 [magic:1725](../../../crates/mir-server/src/magic.rs#L1725)及[1762](../../../crates/mir-server/src/magic.rs#L1762)的生产debug_assert诊断消息；语义修复仍可认定无变化，但交接不能隐去该事实。

## 6. 文档与i18n：i18n PASS；G7 BLOCK

本次独立执行且exit0：含测试strict66≤70；no-test strict0≤0；TextKey验证243/243，zh/en各243、差异0；文档链接检查1401引用通过；git diff --check通过。链接存在检查不等于内容一致。

必须同步的当前失配：

- [README:48](../../../README.md#L48)仍写PoisonCloud/Trap/耗材未实现、R52计划中。
- [路线图:103](../../roadmap.md#L103)至下一节仍写主线未实现及全部计划。
- [客户端:1858](../../client.md#L1858)仍说敏感回纠证据缺，[1860](../../client.md#L1860)仍写DeleteItem只背包；现有C证据已改变这些事实。
- [技能支持文档:39](../../world/magic.md#L39)、[685](../../world/magic.md#L685)、[772](../../world/magic.md#L772)仍以UnsupportedSpell/未实现描述已实现两技能。
- [体验清单:9](../../client-experience-backlog.md#L9)及 [R52索引:3](README.md#L3)、[5](README.md#L5)仍是A阶段计划状态，未添加最终测试与PARTIAL映射。
- [i18n:915](../../i18n.md#L915)67/0可保留为A历史，但需新增当前66/0时点；[920](../../i18n.md#L920)仍将两技能/耗材称未实现。

## 必改清单与提交建议

1. **G7文档批**：上列七组文档加最终证据时点和已交付子项，保留R49/R50/R51历史结论及本报告全部PARTIAL；不可把125、PvP Frost、socket、FireWall C撤销和原失败史缺失写成闭合。
2. **PoisonCloud勘误**：在plan:47与dispatch:61显式记录“原L3=12000错误；项目默认16000−2000×L，L2=12000、L3=10000”，保留原历史错误来源，不靠B4局部摘要替代。

非阻断精度提示：C摘要M1“同时命中”应校正；B3 fix2摘要2153行应标为历史误计（实际2202）；门禁首轮应按原件写1个FAILED，四个受影响分支另列；46be80b和Crystal行号按本报告准确口径交接。

建议Lead先提交限定文档收口批，再针对相同功能树复核G7后更新终审结论。仅文档变更不需要本审计代理擅自重跑全量；若动产品、测试、工具、阈值、语言包或素材指纹，则交Lead重新冻结门禁。不得因本次BLOCK回滚已验证功能，也不得先宣称整轮PASS。

本次提交候选只应显式添加 [manifest](final-manifest.json) 和本报告，门禁摘要在ignored target中保留。禁止将两份历史大冻结JSON一起入库。独立审计的BLOCK仅表示最终文档未闭合，产品链路保留已验证结果及PARTIAL边界。
