# R52 中书省实施方案：护身符耗材与毒雾/陷阱补全、R51 定向补证

> 主单 `JJC-20261025-004`；中书省 gpt-6.1-sol；2026-10-25（Asia/Shanghai）。本稿只立旨、规划和维护看板，未改 crates、未运行 cargo、未执行 Git 写操作。
> 执行者：按 `subagent-driven-development` / `executing-plans` 分任务执行；编码（含测试代码）仅 deepseek，经 `spawn_teammate`；gpt 负责规划、独立评审、审计、验收、文档与项目管理。门下批准后由尚书登记真实子单再派发，草案编号不是看板子单 ID。
> 目标：让装备栏耗材真正约束 PoisonCloud/TrapHexagon，并证明服务端效果、装备数量、客户端快照及存档一致；只补 R51 中与当前链路可独立验收的缺口。

## 一、事实锚点（静态已核不等于本轮运行已验）

- 仓库 `C:/Users/diana/Documents/github/mir-next`；已执行 `git log --oneline -8`、`git status --porcelain`、分支与双 ref 查询：main，HEAD=origin/main=`f4e4497d132b356261c84ecb24d5db8a66ae449d`；前序为 ec53dfa、8cbe713、edad41b、0d27e5e、42a49e0、1fe36dd、f500269。
- 初始 tracked 无改动；两份历史未跟踪 `docs/testing/monster-native/player-resource-freeze-before.json` / `docs/testing/monster-native/player-resource-freeze.json`（约 44/45 MB）排除入库，保留原件。
- R51 主单 `JJC-20261025-003` 已核 Done；[尚书验收](round51-shangshu-acceptance.md#L3)、[门下确认](round51-menxia-confirm.md#L53)、[v4 终审](../testing/round51/round51-final-audit-v4.md#L31)、[B4 摘要](../../target/round51/b4/00-summary.md)明确 PASS-WITH-PARTIAL；P03/P10 closed，不等于七技能全部完成。
- [roadmap §五](../roadmap.md#L101)仍有 R50 前时态；[README 技能/商城行](../../README.md#L48)遗漏 R51 并仍说独立商城未做；[商城 §12](../world/gameshop.md#L683)已记录 R50 三项前置交付，不能再按未实施派活。
- [体验清单](../client-experience-backlog.md#L10)是 R45 历史登记，部分他者/怪物插值后续已交付；保留历史，补当前边界。[R51 索引](../testing/round51/README.md#L3)已经写 Done，核对即可；[i18n](../i18n.md#L322)已为 243 键，但扫描数字仍有 R50 快照，须分清时点。

| # | 已抽查的源码锚点（仓库相对路径:行） | 本次结论 |
|---|---|---|
| 01 | [github/Crystal/Server/MirObjects/HumanObject.cs:4543](../../github/Crystal/Server/MirObjects/HumanObject.cs#L4543) | 已核：PoisonCloud 先找到 `GetAmulet(5)` 与 `GetPoison(5,1)`，延迟 `距离*50+500`，两项各扣 5，PoisonAttack 参与随机附伤。 |
| 02 | [github/Crystal/Server/MirObjects/HumanObject.cs:4596](../../github/Crystal/Server/MirObjects/HumanObject.cs#L4596) | 已核：Trap 预检 x 为中心±1、y 为中心-1至中心（`y < location.Y + 1`），只认可攻击且等级≤施法者+2的怪；1 符、500ms、`(level*5+10)*1000`。 |
| 03 | [github/Crystal/Server/MirObjects/HumanObject.cs:6859](../../github/Crystal/Server/MirObjects/HumanObject.cs#L6859) | 已核：按装备索引递增找单堆；符为 Amulet/shape0，绿毒同为 Amulet/shape1；不扫背包、不跨堆凑数，不按耐久过滤。 |
| 04 | [github/Crystal/Server/MirObjects/HumanObject.cs:1346](../../github/Crystal/Server/MirObjects/HumanObject.cs#L1346) | 已核：ConsumeItem 按数量扣除并发 DeleteItem，耗尽摘槽；不是扣 durability。 |
| 05 | [github/Crystal/Server/MirEnvir/Map.cs:1434](../../github/Crystal/Server/MirEnvir/Map.cs#L1434) | 已核：毒雾按有效格铺 3×3、同格同类场不重叠；6000ms、1000ms tick；首个成功对象 Show=true，训练在地图结算路径。 |
| 06 | [github/Crystal/Server/MirObjects/SpellObject.cs:150](../../github/Crystal/Server/MirObjects/SpellObject.cs#L150) | 已核：先 MAC 伤害，存活目标再上绿毒12跳，TickSpeed 随场、值为当前 SC 均值+BonusDmg；须遵守可攻击规则。 |
| 07 | [github/Crystal/Server/MirEnvir/Map.cs:1752](../../github/Crystal/Server/MirEnvir/Map.cs#L1752) | 已核：Trap 落地扫3×3、ShockTime并清目标、八个外围场绑定中心怪；中心候选顺序、边界判断与预检不同。 |
| 08 | [github/Crystal/Server/MirObjects/SpellObject.cs:51](../../github/Crystal/Server/MirObjects/SpellObject.cs#L51) | 已核：Trap 依赖目标ShockTime/到期回收；只有 FireWall 强制施法者换图清场，不能套用到所有场。施法者失联后的毒雾行为仍需 B2 追踪。 |
| 09 | [github/Crystal/Server/MirObjects/MonsterObject.cs:634](../../github/Crystal/Server/MirObjects/MonsterObject.cs#L634)、[github/Crystal/Server/MirObjects/MonsterObject.cs:1858](../../github/Crystal/Server/MirObjects/MonsterObject.cs#L1858) | 已核：ShockTime 限制移动；CanAttack不直接判断ShockTime，远距ProcessTarget清目标；不能实现成 Frozen 的别名。 |
| 10 | [github/Crystal/Server/MirObjects/MonsterObject.cs:2613](../../github/Crystal/Server/MirObjects/MonsterObject.cs#L2613) | 已核：有效受击解除ShockTime，护甲挡住的早退在前；怪物攻击入口也会解除。 |
| 11 | [github/Crystal/Server/MirEnvir/Envir.cs:1097](../../github/Crystal/Server/MirEnvir/Envir.cs#L1097)、[github/Crystal/Server/MirEnvir/Envir.cs:1247](../../github/Crystal/Server/MirEnvir/Envir.cs#L1247)、[github/Crystal/Server/MirEnvir/Envir.cs:333](../../github/Crystal/Server/MirEnvir/Envir.cs#L333) | 已核：Trap BaseCost7/LevelCost3/Range9；毒雾30/5、Delay18000-2000×level、Range9；ShoulderDash DelayBase2500。 |
| 12 | [crates/mir-server/src/magic.rs:599](../../crates/mir-server/src/magic.rs#L599)、[crates/mir-server/src/magic.rs:1265](../../crates/mir-server/src/magic.rs#L1265) | 已核：两技能当前 Unsupported，早于扣蓝拒绝；不能把去掉拒绝当成实现。 |
| 13 | [crates/mir-server/src/magic.rs:1431](../../crates/mir-server/src/magic.rs#L1431) | 已核：公共流程先锁动作/SpellTime，再检冷却/MP并扣蓝，效果分派在后；新增耗材失败应明确与这条顺序的关系。 |
| 14 | [crates/mir-server/src/items.rs:2451](../../crates/mir-server/src/items.rs#L2451)、[crates/mir-server/src/items.rs:958](../../crates/mir-server/src/items.rs#L958) | 已核：consume_one仅背包单件；Amulet可进装备6/9，不能只查9或冒用consume_one。 |
| 15 | [crates/mir-server/src/magic.rs:524](../../crates/mir-server/src/magic.rs#L524)、[crates/mir-server/src/magic.rs:2694](../../crates/mir-server/src/magic.rs#L2694) | 已核：SpellField只有火墙型数据/处理，毒雾及目标绑定Trap须分支和各自生命周期。 |
| 16 | [crates/mir-server/src/monster.rs:1101](../../crates/mir-server/src/monster.rs#L1101)、[crates/mir-server/src/world.rs:5707](../../crates/mir-server/src/world.rs#L5707) | 已核：Monster未有ShockTime链；ObjectMonster.shock_time硬编码0，进视野也需同步。 |
| 17 | [crates/mir-server/src/magic.rs:3109](../../crates/mir-server/src/magic.rs#L3109)、[crates/mir-server/src/magic.rs:3156](../../crates/mir-server/src/magic.rs#L3156) | 已核：Freezing汇总有效装备模板+附加+生效buff，零值不抽随机；guild/嵌套插槽属性尚无完整等价证明。 |
| 18 | [crates/mir-server/src/world.rs:2273](../../crates/mir-server/src/world.rs#L2273)、[crates/mir-server/src/magic.rs:6196](../../crates/mir-server/src/magic.rs#L6196) | 已核：攻击入口实际拒Frozen；Freezing7旧断言5..=12宽于理论5..=11，须运行补证。 |
| 19 | [crates/mir-client/src/net.rs:5473](../../crates/mir-client/src/net.rs#L5473) | 已核：DeleteItem只调用delete_inventory_item，装备耗材会留下客户端旧计数，属于本轮必修链路。 |
| 20 | [crates/mir-client/src/net.rs:5125](../../crates/mir-client/src/net.rs#L5125)、[crates/mir-client/src/net.rs:5360](../../crates/mir-client/src/net.rs#L5360) | 已核：152/153有回纠分支；ObjectSpell已有消费但尚不能据此宣称两技能场完整。 |
| 21 | [crates/mir-server/src/session.rs:3428](../../crates/mir-server/src/session.rs#L3428) | 已核：autosave读取世界当前角色快照；装备扣数/耗尽重登及失败重试仍需运行验证，无需先加迁移。 |
| 22 | [crates/mir-server/src/magic.rs:5823](../../crates/mir-server/src/magic.rs#L5823)、[crates/mir-server/src/magic.rs:5908](../../crates/mir-server/src/magic.rs#L5908) | 已核：普通附加错引1965，应1859；插槽另为1965/1966。旧“四包”文字须勘误为五个位置，保全历史原件。 |

未核/不作完成事实：本轮尚无运行结果；真实默认DB逐技能MagicInfo取值、PoisonAttack全部来源、Trap专属怪物覆写、毒雾失联Caster、默认客户端毒雾/Trap素材渲染仍待对应子单。不得以搜索命中或旧日志代替本轮证据。

## 二、可判定目标与 PARTIAL 规则

- G1 耗材：仅从装备6/9及其它合法根装备槽按顺序选符合Type/Shape/单堆Count的物品；缺任一不扣任何物品；成功毒雾符5+绿毒5，Trap符1；精确扣数/耗尽摘槽，DeleteItem UUID/Count一致，存档重登一致。错误模板/shape/不足/背包只有材料/两堆合足但单堆不足/耐久0均有反例。
- G2 毒雾：距离延迟、SC伤害与PoisonAttack随机、有效格3×3/不叠场、6秒/1秒边界、伤害后存活才12跳绿毒、施法者/目标换图死亡下线、训练次数及149/125生命周期逐项有运行断言；玩家目标按既有PK规则验收。不得用瞬时AOE或普通Poisoning冒充。
- G3 Trap：施放预检/落地重检范围、等级差+2/+3、清目标、真实移动/攻击AI限制、受击解除、持续时间与八场生成/撤销、地图边缘/重复施法/目标死亡移图均可证伪；不拘束玩家，不把场装饰当控制生效。
- G4 默认资料：从实际默认GameData/DB读七技能MagicInfo并记录来源SHA，先断言关键字段后逐技能测试MP/冷却前一毫秒及到期边界；肩部冲刺=2500ms；PoisonCloud 冷却的默认值为 L2=12000ms、L3=10000ms。原文“毒雾等级3=12000ms”是错误记录，现明确勘误为 L3=10000ms；保留本条原错值以供审计追溯。
- G5 R51定向补证：真实施法产生非空Slow及Frozen，覆盖概率命中/不命中和玩家/怪物边界；Frozen后调用真实attack断言拒攻；Freezing7收紧≤11，多槽durability>0叠加、坏装备+有效buff、0耐久+非零added与同种子后续RNG对照。StormEscape升级场景五位置存在后严格验证buff→LevelMagic，另跑跨图例。
- G6 客户端：服务端真实产包→encode→decode→WorldSnapshot，装备DeleteItem扣数/归零与149/125生成撤销一致；152/153初态为错误预测格+非空既有motion/队列，分别证明权威位置/Standing/清队列，消费删除或逻辑反事实必须使断言失败。既有正确坐标空队列不算补证。
- G7 文档：R50/R51终态、R52本轮证据和顺延11项逐项映射；更新总览/技能/客户端/i18n时点，不改历史BLOCK/PASS或日志原字节，不把计数改大/改小当覆盖完成。
- **验收分级**：G1–G3及G6耗材/场链为主交付硬条件，任一缺失不得称“两技能完整”，主线应返工；G4/G5每个子项单独closed或PARTIAL。R52可为PASS-WITH-PARTIAL，仅限门下先批准的来源/表现/顺延边界；缺日志、空断言、skip、默认fixture失真属BLOCK，不能靠写PARTIAL通关。
- 本轮“完整”限定已实现的根装备模板/附加/有效buff、既有PK与地图模型；guild来源、Crystal1965/1966嵌套插槽、原生视觉/音频及真socket另记PARTIAL，不吞并为系统全等价。

## 三、子任务 DAG、写入范围与验收契约（7 个草案）

`门下批准 → A文档批并提交 → B1 → B2 → B3 → B4 → C → 队长全量门禁 → T → 尚书验收 → 门下终态确认`。
A后各编码项串行：B2/B3/B4共享技能文件，B3还改怪物/世界文件；C等待服务端真实产包。禁止同文件并写。T可早做只读预审，最终裁决必须等待门禁及最终功能树。

所有证据写 `target/round52/<子单或attempt>/`，含命令/cwd/完整HEAD/功能树指纹/时区/起止时间/raw exit/用例数及关键断言；单文件≤1MiB，超限按用例/连续编号分片并留SHA256清单，不丢尾、不改码页原件。target保持ignored；可入库的审计摘要与manifest另落 `docs/testing/round52/`，也≤1MiB。

| 草案 | owner / 路由 | 写范围（除此只读） | 依赖 / 验收命令与输出契约 |
|---|---|---|---|
| A 文档纠偏 | 礼部 / gpt-6.1-sol | [README](../../README.md)、[roadmap](../roadmap.md)、[client](../client.md)、[magic文档](../world/magic.md)、[i18n](../i18n.md)、[体验清单](../client-experience-backlog.md)；R51索引仅必要同步；新 `docs/testing/round52/README.md` | 门下批准；`py tools/docs/check_links.py`、TextKey校验及双i18n棘轮exit0；核对R51结论逐项一致；证据 `target/round52/a/`；不把未来功能写已完成。 |
| B1 耗材原语 | 户部 / deepseek（spawn_teammate） | 新 `crates/mir-server/src/spell_consumables.rs`、[lib.rs](../../crates/mir-server/src/lib.rs)模块声明、新 `crates/mir-server/tests/round52_consumables.rs` | A已提交；`cargo test -p mir-server round52_consumables -- --nocapture`：存在且执行≥1测试、0失败；装备顺序、双项原子预检、耗尽包/快照/重登；证据 `target/round52/b1/`。 |
| B2 PoisonCloud | 兵部 / deepseek（spawn_teammate） | [magic.rs](../../crates/mir-server/src/magic.rs)、新 `crates/mir-server/tests/round52_poison_cloud.rs`；其它文件改动先交尚书登记 | B1；`cargo test -p mir-server round52_poison_cloud -- --nocapture`，G2逐项非空运行并覆盖错误耗材；证据 `target/round52/b2/`。 |
| B3 TrapHexagon | 兵部 / deepseek（spawn_teammate） | [magic.rs](../../crates/mir-server/src/magic.rs)、[monster.rs](../../crates/mir-server/src/monster.rs)、[world.rs](../../crates/mir-server/src/world.rs)、新 `crates/mir-server/tests/round52_trap_hexagon.rs` | B2；`cargo test -p mir-server round52_trap_hexagon -- --nocapture`，G3及守卫/召唤物等边界；证据 `target/round52/b3/`。 |
| B4 R51补证与勘误 | 兵部 / deepseek（spawn_teammate） | [magic.rs](../../crates/mir-server/src/magic.rs)测试与引用勘误、新 `crates/mir-server/tests/round52_regression.rs` | B3；`cargo test -p mir-server round52_regression -- --nocapture` + `cargo test -p mir-server --lib magic::tests:: -- --nocapture`；G4/G5逐条映射、保留旧反例；证据 `target/round52/b4/`。 |
| C 客户端与wire | 工部 / deepseek（spawn_teammate） | [net.rs](../../crates/mir-client/src/net.rs)、新 `crates/mir-client/tests/round52_spells_wire.rs`；已有 [round51_spells_wire.rs](../../crates/mir-client/tests/round51_spells_wire.rs)仅必要加强 | B4；`cargo test -p mir-client round52_ -- --nocapture`、`cargo test -p mir-client --test round52_spells_wire -- --nocapture`、`cargo test -p mir-client --test round51_spells_wire -- --nocapture`；G6；证据 `target/round52/c/`。 |
| T 独立审计 | 刑部 / gpt-6.1-sol | 新 `docs/testing/round52/round52-final-audit.md`、`final-manifest.json`与索引；`target/round52/t/**` | 功能提交+队长门禁；核验G1–G7矩阵、原日志/指纹/失败史、定向复跑；links和diff检查exit0；不改产品代码，不再跑全量；报告BLOCK或PASS-WITH-PARTIAL的精确范围。 |

- [ ] A：保留R49/R50历史段，新增R51现状与R52范围；先跑便宜文档检查，交gpt审阅，由队长提交文档批。i18n当前扫描实测后才更新，243键不凭推断改数。
- [ ] B1：先写 `round52_consumables_equipment_order_atomic_and_exhaustion` 等红例，再实现最小原语；提出固定接口 `find_spell_material(&self, player_id: u32, count: u32, shape: u16) -> Option<usize>`、`consume_spell_material(&mut self, player_id: u32, slot: usize, count: u32) -> bool`（World方法，仅crate可见）；slot/type转换在边界检查，不复制物品或额外加数据库迁移。
- [ ] B1：双材料先共同验证、再在同一World锁内扣除；DeleteItem只在成功扣除后各发一次。增加 `round52_consumables_save_reload_equipment_count` 验证生产角色快照→既有存储→重登，不能仅序列化手造UserItem。
- [ ] B2：写 `round52_poison_cloud_consumes_both_after_common_cast_checks`、`round52_poison_cloud_grid_tick_poison_and_expiry`、`round52_poison_cloud_pk_and_caster_lifecycle`；确认红灯原因，再实现SpellKind/延迟效果/场分支、当前SC与PoisonAttack根装备+附加+生效buff汇总；0附伤随机不消耗RNG。
- [ ] B2：不复用火墙清场假设；明确 `>`/`>=` 临界，6秒时与6秒后、tick首跳时间、入视野补发、ObjectRemove一一验证；失联Caster按门下D3口径单列证据。
- [ ] B3：写 `round52_trap_hexagon_precheck_3x2_resolve_3x3`、`round52_trap_hexagon_shock_ai_release_and_eight_fields`、`round52_trap_hexagon_level_edges_and_recast`；实现独立shock到期状态、AI/受击挂点与ObjectMonster剩余时间，使用世界时钟，不用Frozen毒替代。
- [ ] B3：先追踪攻击/推开/死亡及不同怪AI入口，再给每条受影响路径反例；若专属AI覆写不能完成，报告准确模板范围并交门下裁决，不能默认扩展为全537模板支持。
- [ ] B4：新增 `round52_regression_default_magic_info_cost_cooldown`，断言读取的实际默认记录确实含七技能，记录DB来源/哈希；逐技能字段与独立Crystal值核对，不能复制被测实现计算期望。
- [ ] B4：新增 `round52_regression_real_slow_frozen_players_monsters`、`round52_regression_frozen_attack_rejected`、`round52_regression_freezing_sources_and_rng_state`、`round52_regression_stormescape_order_cross_map`；固定种子有上下分支各自命中计数，禁止只循环找到一个绿例就退出。
- [ ] C：扩展DeleteItem UUID查找至装备并正确清空耗尽槽，保留背包既有行为；新增 `round52_wire_equipped_material_count_and_field_lifecycle`；真实世界产包不能手造成功包代替服务端。
- [ ] C：新增 `round52_dash_fail_wrong_prediction_with_existing_motion` 分别覆盖152/153；保持五位置`is_some`支配顺序比较。用明确反事实断言或隔离副本变异证明消费必要性，不在共享工作树删除处理分支；证据不得声称真实socket。
- [ ] 各编码单：红灯原因→最小实现→定向绿灯→gpt独立审阅；测试过滤输出0条即失败，named test清单须与实际运行列表一致；生产修复不吞入“纯补证”统计。
- [ ] T：逐项链接到真实子单、源码/测试名与原日志范围；重新算SHA/bytes，确认所有attempt及真实exit均保留；G1–G3/G6核心不满足即返工，未核字段不可填closed。

## 四、提交策略与唯一全量门禁

1. 本稿+门下审议及A先作独立文档提交，明确R51已Done与R52未实施；本规划角色不提交。后续B1/B2/B3/B4/C经各自定向检查与gpt评审，队长按路径分功能批提交，单批证据≤1MiB，禁 `git add .`。
2. 队长唯一执行 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`；子单只跑表中定向命令，禁止各自workspace/全量、盲杀其它agent进程或并行启动共享DB门禁。保留12项真实结果；脚本旧注释不能当13项验收。
3. 功能树冻结并提交后门禁，记录完整HEAD、功能/测试/工具/配置路径blob指纹及脏状态；每次attempt独立log/meta，失败不能改名成PASS。门禁中途若改功能/测试/CI/语言包/素材manifest，旧结果失效，队长重新门禁。
4. 门禁后T及尚书/门下只写审计/manifest/过程文档，追加文档提交不使已冻结功能树门禁失效；须列明与被测HEAD仅文档差异，重跑links与`git diff --check`。不得借“文档提交”夹带源码注释/断言/工具变更。
5. 入库只含本轮受控源文件与≤1MiB文档/manifest；target、.env、凭据、两份大冻结JSON排除。原日志离机移交清单含bytes/SHA，manifest不能替代原字节。
6. 收口按真实show/子单结果核对 → 尚书done进Review → 对口请求Done/PendingConfirm → 指定门下confirm；终态冻结。本轮完成由队长提交并推送，未获门下批准不抢先转Doing。

## 五、风险与捷径黑名单

- 耗材搜索不要套用Freezing的durability>0过滤；Crystal材料按Type/Shape/Count，0耐久仍是反例。不可只扫槽9、自动补背包、跨堆凑数、先扣符后发现无毒，或发UseItem冒充DeleteItem。
- 新两技能耗材不足发生在公共MP扣减之后：按Crystal应扣本次MP但不扣材料、cast=false、不写本技能CastTime；公共动作/SpellTime保留。未学/越界/冷却/缺蓝依旧走各自真实路径。禁止为省事统一“失败零扣蓝”，也禁止仍用Unsupported伪称完成。
- Trap预检3×2与落地3×3是已核差异；不能“顺手修正”循环。外围坐标安全剪裁、中心目标选择及不同AI要独立记录；不得越界访问来机械复制Crystal错误边界。
- 只增加场对象/动画而无毒tick、ShockTime真实AI挂点，或把绿毒写入当作所有语义，均不通过。禁止改协议编号250/251、放松CI、加allow/ignore/skip、删旧反例/失败日志或手工注毒冒充真实施法。
- R51日志证据保留历史时点和坏尝试；普通1859与插槽1965/1966分开；五个位置先证明存在再排序；Freezing7不是上界12；true-wire不是socket，Walking/Walk不是原生dash。
- 原生素材与资源授权沿用现有清单；本轮不下载或大范围重建资产。若新增玩家提示，必须同步TextKey与zh/en并按实测统计，禁止硬编码中文绕棘轮。

## 六、明确不做与剩余清单承接

| R51转R52项 | 本轮决定 |
|---|---|
| 1 耗材+两技能，Frost玩家PvP | 主交付G1–G3；Frost玩家目标纳入G5。其它已支持技能全面强制耗材另轮，避免更改全职业旧玩法。 |
| 2 默认逐技能耗蓝/冷却 | G4，必须真实默认资料；若样本缺记录不得用假fixture补绿。 |
| 3 非空Slow/概率两侧/玩家怪 | G5；统计与准确种子写证据。 |
| 4 StormEscape严格包序/跨图 | G5；按真实升级触发产包。 |
| 5 Frozen实际拒攻 | G5；调用World::attack并验证无伤害/无攻击包。 |
| 6 152/153敏感回纠 | G6；已有motion/错误预测是必备前置。 |
| 7 火墙逐对象ProcessSpell/InTrapRock | 明确顺延R53候选；保留P04，不能借TrapHexagon之名吞并TrapRock系统。 |
| 8 原生dash帧/视觉/声音/时序 | 顺延；P08保留；本轮只修耗材快照/包消费。 |
| 9 guild Freezing/真socket | 顺延；现有wire与根装备/buff来源有界验收，不写socket端到端。 |
| 10 多槽有效装备与Crystal1965/1966 | 多个根装备槽叠加纳G5；嵌套插槽统计顺延并单列PARTIAL。 |
| 11 零RNG/坏装备+buff/0耐久+added | G5；Freezing7上界同步收紧，不能只改文字。 |

其余不做：PetSave/PetExp、AOI全量翻绿、攻城旗子/控制点/修门/领地、坐骑/天气/光照/攻击变招、CallDefaultNPC、行会投票/职位UI；不改变R50商城交易语义/既有残余。理由是它们各有独立系统/资产/端到端验收链，不能与本轮主线共享一次局部绿灯。

## 七、待裁决项（交独立门下省，不问用户）

- D1 范围：批准7草案及上表裁剪；G1–G3/G6核心为硬门槛，G4/G5逐项验，顺延仍保留PARTIAL。若时间不足，先退回裁剪补证，禁止执行中静默砍掉任一主技能。
- D2 默认建议：新增两技能遵从Crystal公共扣蓝后耗材检查的顺序；失败cast=false且不设CastTime。Trap保留3×2预检/3×3落地；内存安全边界用合法Rust地图坐标剪裁并明确偏差，不复制越界。门下批准或给明确替代口径。
- D3 默认建议：毒雾Caster失联须先补Crystal调用链证据；本作采用离线/异图停止继续取活体属性并安全清场，作为显式生命周期偏差单列PARTIAL；若门下要求严格留场，B2需冻结施法者归属/属性并补偿所有测试后方可放行，不得借用火墙默认清场隐瞒差异。
- D4 默认建议：本轮耗材只接新两技能；其它SoulFireBall/各buff/Hiding/Summon/Poisoning耗材与毒型策略保持已记载差异，另轮迁移。guild/嵌套插槽PoisonAttack/Freezing来源保留PARTIAL，不能写“与Crystal全等价”。
- D5 审核时落实：数据真实样本缺默认技能、专属怪AI无法覆盖、客户端场素材不足时，必须点名哪些G项不满足并决定返工或改范围；无证据不能附条件写closed。最终功能树与文档后置规则由队长/刑部双重核对。

看板回执：`create --id JJC-20261025-004 --as taizi`、`Taizi→Zhongshu --as taizi`、`memo --as zhongshu`、`Zhongshu→Menxia --as zhongshu`均已成功；`validate --json`为 `ok:true / schema:v3.1 / tasks:116 / issues:[]`。主单当前Menxia，尚未批准或派发。
静态校验：相对链接1286条、README引用2篇及市场包号检查exit0；篇幅≤220行。CLI的state说明会去路径，show.now显示`Plan ready: docs; seven drafts; decide D1-D5 with independent review`，完整路径已存memo；state不自动写flow_log，转换凭CLI回执/audit核对。本文件不是运行PASS或Done凭据。
