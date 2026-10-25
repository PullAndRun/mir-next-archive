# R52 尚书省派发：七子单串行契约

主单 `JJC-20261025-004`；门下交尚书委派 `JJC-20261025-004-sub-f0e732fd`；尚书省 / gpt-6.1-sol；2026-10-25（Asia/Shanghai）。
本文件仅登记流程与看板，不代表功能完成；本派发官未改 crates、未运行 cargo、未作 Git 写操作、未启动执行代理。仅 Lead 可启动成员、按路径提交和推送。

## 1. 原件核验与裁决优先级

- [中书方案](round52-plan.md)：原件 130 行 / 25,422 B / SHA256 `256902b355d28e255c24b1f92315a0a7378bb2e29973f6f91078b6a701906e10`，已逐字节重算一致。
- [门下审议](round52-menxia-review.md)：原件 94 行 / 16,614 B / SHA256 `36a2fd7b5c75da53af3ec47966aca6528741881574897d555b3d7802bf941424`，已逐字节重算一致；D1–D5、M01–M08 均完整存在。
- D1 批准七单及范围；D2 修改后批准：公共扣蓝后材料检查、Trap 3×2/3×3与安全剪裁；D3 批准离线/异图安全清场并单列生命周期 PARTIAL；D4 批准仅新两技能接耗材；D5 修改后批准：缺项点名，核心缺口返工，不预授权降格。
- 编号严格按门下原件：M03 含 Shock/CanAttack；M04 为毒雾；M05 含生产存档及私有边界；M06 为默认数据/R51补证；M07 为路由和串行；M08 为证据/门禁。草案、短摘要均不能覆盖原件硬契约。
- 输入功能基线 `main` / HEAD=origin/main=`f4e4497d132b356261c84ecb24d5db8a66ae449d`；治理文档后续提交由 Lead 记录新 HEAD。本派发未以旧 HEAD 冒充后续测试指纹。

## 2. 登记、路由、依赖与启动共约

| 批次 / 标题建议 | SUB_TASK_ID | owner / 模型启动 | depends_on |
|---|---|---|---|
| A：R52文档现状纠偏与证据索引 | JJC-20261025-004-sub-8dc283b5 | 礼部 libu / Lead `subagent(provider=sol-relay, model=gpt-6.1-sol)` | 门下批准 |
| B1：装备耗材原语与生产存档核验 | JJC-20261025-004-sub-a2b51ae2 | 户部 hubu / deepseek，仅 Lead `spawn_teammate` | A文档已评审并由Lead提交 |
| B2：PoisonCloud真实毒雾场与耗材 | JJC-20261025-004-sub-5e9ebbde | 兵部 bingbu / deepseek，仅 Lead `spawn_teammate` | B1定向绿灯、独立gpt评审、Lead提交 |
| B3：TrapHexagon范围与Shock控制 | JJC-20261025-004-sub-aaff7a87 | 兵部 bingbu / deepseek，仅 Lead `spawn_teammate` | B2定向绿灯、独立gpt评审、Lead提交 |
| B4：R51默认数据与状态回归补证 | JJC-20261025-004-sub-dd381bee | 兵部 bingbu / deepseek，仅 Lead `spawn_teammate` | B3定向绿灯、独立gpt评审、Lead提交 |
| C：装备耗材与技能场真实wire同步 | JJC-20261025-004-sub-fa4e23ed | 工部 gongbu / deepseek，仅 Lead `spawn_teammate` | B4定向绿灯、独立gpt评审、Lead提交 |
| T：R52独立终审与证据清单 | JJC-20261025-004-sub-b6dcd00e | 刑部 xingbu / Lead `subagent(provider=sol-relay, model=gpt-6.1-sol)` | C评审/功能提交 + Lead唯一全量门禁 |

严格 DAG：A → B1 → B2 → B3 → B4 → C → Lead唯一全量门禁 → T → 尚书验收 → 指定门下确认。CLI无依赖写入参数；真实子单ID、路径、依赖、模型与验收写入delegate指令和本契约，不手改看板JSON。
一次只唤醒一名成员；编码（含测试）仅deepseek。每批结束停止写入，再串行启动一名独立gpt评审；评审不写产品/测试代码，不并行多gpt。共享文件不并写，共享DB门禁不并跑。Lead保存SUB_TASK_ID ↔ agent/teammate ID；本轮登记时尚未建立执行者映射。
所有执行者必读：仓库 `AGENTS.md`、本文件、上述plan/review、`C:/Users/diana/.dsh/skills/edict/references/roles/README.md`及对应`<部门>.md`；再读各批补充原件与上游实际产出。CLI固定 `node C:/Users/diana/.dsh/skills/edict/scripts/edict.mjs`，先`recall JJC-20261025-004`、`progress <子ID> ... --as <部门>`，每子单最多一个todo为in-progress。
以下验收命令是将来执行合同，本派发未运行；全部cwd为仓库根。每个过滤器先以`-- --list`核named tests，再执行原命令；实际命中≥1、零失败，零命中即BLOCK。证据必须包含预期named test与实际运行列表映射。

## 3. 七子单逐条启动契约

### A / 礼部 / gpt文档批

写范围精确为 `README.md`、`docs/roadmap.md`、`docs/client.md`、`docs/world/magic.md`、`docs/i18n.md`、`docs/client-experience-backlog.md`、新`docs/testing/round52/README.md`；`docs/testing/round51/README.md`仅必要同步；证据`target/round52/a/**`。其它只读。
启动契约：读取R51尚书验收、门下确认、终审v4与11项残余；核对R50商城已交付状态，保留历史段与原日志。将R51 Done/PASS-WITH-PARTIAL、R52尚未实现、11项承接逐条区分，243个TextKey等数字仅实测后更新；不写未来功能已完成。落实D1/D4/D5、G7、M07/M08，交独立gpt评审后由Lead先提交文档批，再启动B1。
验收命令：`py tools/docs/check_links.py`；`py tools/i18n/verify_text_keys.py`；`py tools/i18n/scan_hardcoded.py --player-visible --strict --fail-above 70`；`py tools/i18n/scan_hardcoded.py --no-test --player-visible --strict --fail-above 0`；`git diff --check`。逐项exit0，扫描计数和R51交叉核对摘要归证，未跑不得写PASS。

### B1 / 户部 / deepseek耗材原语

写范围 `crates/mir-server/src/spell_consumables.rs`（新模块及模块内私有单测）、`crates/mir-server/src/lib.rs`（仅模块声明）、`crates/mir-server/tests/round52_consumables.rs`（新）；证据`target/round52/b1/**`。生产存档需其它路径时先报尚书精确登记，当前未批准写`session.rs`或扩public。
启动契约：A提交后读Crystal HumanObject.cs耗材搜索/ConsumeItem与生产快照/存储/重登路径；实现World方法`pub(crate) fn find_spell_material(&self, player_id: u32, count: u32, shape: u16) -> Option<usize>`、`pub(crate) fn consume_spell_material(&mut self, player_id: u32, slot: usize, count: u32) -> bool`。按合法根装备槽递增搜索单堆Type/Shape/Count，含6/9，不过滤耐久；双材料同一World锁共同预检后扣除。成功每UUID只发一次准确Count的DeleteItem，归零摘槽，失败不发包。落实D2/D4、M01/M02/M05；私有原语在新模块内单测，外部integration只走公开真实链，不越Session.current_row私有边界。
验收命令：`cargo test -p mir-server round52_consumables -- --nocapture`；至少含`round52_consumables_equipment_order_atomic_and_exhaustion`、`round52_consumables_save_reload_equipment_count`。证明部分扣数/耗尽/错误UUID/背包同模板不误扣、双项原子及生产角色快照→既有存储→重登；不以手造UserItem序列化代替。B1可先完成原语/生产存档证据，真实施法及客户端闭环由B2/B3+C完成，T前不得写G1已闭合。

### B2 / 兵部 / deepseek PoisonCloud

写范围 `crates/mir-server/src/magic.rs`、新`crates/mir-server/tests/round52_poison_cloud.rs`；证据`target/round52/b2/**`；其它文件变更先尚书登记。B1评审/Lead提交后启动。
启动契约：读Crystal HumanObject.cs:4543、Map.cs:1434、SpellObject.cs:51/150及IsAttackTarget链；接原语，耗符5+shape1绿毒5。公共检查后材料失败仍扣MP并保留动作/SpellTime，cast=false、不写本技能CastTime、不排效果。按距离×50+500延迟、有效3×3、同格同类不叠、首实际生成Show、训练、6000ms/1000ms与首跳边界实现；先MAC伤害，存活再12跳绿毒，按tick当前有效SC+已算BonusDmg；根模板/added/有效buff与0附伤不耗RNG需实证。落实D2/D3/D4/D5、M01/M02/M04/M05。
验收命令：`cargo test -p mir-server round52_poison_cloud -- --nocapture`；至少含`round52_poison_cloud_consumes_both_after_common_cast_checks`、`round52_poison_cloud_grid_tick_poison_and_expiry`、`round52_poison_cloud_pk_and_caster_lifecycle`。覆盖缺符/毒、错type/shape、单堆不足、背包有而装备无、跨堆不合并、0耐久；安全区/PK双方、双方死亡/换图/下线；pending与已铺场分别核离线/异图清场及149/125/入视野。死亡语义独立追踪，不能由离线规则推断。
PARTIAL：D3离线/异图安全清场是已接受偏差，Crystal原毒雾通常留场停伤至到期，强制换图清场原分支仅FireWall。须同时证明本作清场正确且列明仍不等价；不得写Crystal必然空指针。

### B3 / 兵部 / deepseek TrapHexagon

写范围 `crates/mir-server/src/magic.rs`、`crates/mir-server/src/monster.rs`、`crates/mir-server/src/world.rs`、新`crates/mir-server/tests/round52_trap_hexagon.rs`；证据`target/round52/b3/**`。B2评审/Lead提交后启动。
启动契约：读Crystal HumanObject.cs:4596、Map.cs:1752、MonsterObject.cs:634/650/1858/2588/2613与各实际AI入口；耗符1，500ms落地，持续`(level*5+10)*1000`。预检x中心±1/y中心-1至中心为3×2，落地重检3×3；合法外围坐标安全剪裁并说明差异。独立Shock世界时钟状态限制移动、远距清目标，不能全局禁止CanAttack或复用Frozen；有效受击解除，护甲早退不解除，并核攻击入口解除/守卫/召唤物/专属AI。八外围场绑定中心怪，含ObjectMonster剩余时间及进视野同步；不拘束玩家。落实D2/D5、M01/M02/M03/M05。
验收命令：`cargo test -p mir-server round52_trap_hexagon -- --nocapture`；至少含`round52_trap_hexagon_precheck_3x2_resolve_3x3`、`round52_trap_hexagon_shock_ai_release_and_eight_fields`、`round52_trap_hexagon_level_edges_and_recast`。单独目标y+1预检不命中，再用预检内目标使落地扫第三行；等级+2/+3、预检后死亡/移图/移位、地图角/边、重复施法、八场生成/回收、入视野均有反例。专属AI漏控制即核心返工，不能默认宣称537模板全支持；需裁剪模板时交门下另裁，未预授权PARTIAL。

### B4 / 兵部 / deepseek R51定向补证

写范围 `crates/mir-server/src/magic.rs`（测试及引用勘误）、新`crates/mir-server/tests/round52_regression.rs`；证据`target/round52/b4/**`；生产修复另列并先登记，客户端装备DeleteItem生产改动归C，不默认划入本批。B3评审/Lead提交后启动。
启动契约：读实际默认GameData/DB加载链、Crystal七技能MagicInfo及R51终审v4；先断言默认七记录关键字段与来源SHA，独立期望，逐技能MP/cooldown-1ms/到期；Dash2500ms、PoisonCloud等级3为12000ms。真实施法产生非空Slow/Frozen，固定种子命中/未命中计数都非零，玩家/怪物与Frozen后真实World::attack拒攻；Freezing7≤11、多有效根槽/added、坏装备+buff、0耐久+added、零RNG后续序列对照。StormEscape真实升级，五位置先is_some再排序并跑跨图；普通added引用1858/1859与插槽1965/1966分开。落实D1/D4/D5、M06/M08；旧反例不删。
验收命令：`cargo test -p mir-server round52_regression -- --nocapture`；`cargo test -p mir-server --lib magic::tests:: -- --nocapture`。named tests至少`round52_regression_default_magic_info_cost_cooldown`、`round52_regression_real_slow_frozen_players_monsters`、`round52_regression_frozen_attack_rejected`、`round52_regression_freezing_sources_and_rng_state`、`round52_regression_stormescape_order_cross_map`。缺默认记录只将对应G4记录点名PARTIAL；假fixture、空断言、无日志、零命中均BLOCK。

### C / 工部 / deepseek客户端与真实wire

写范围 `crates/mir-client/src/net.rs`、新`crates/mir-client/tests/round52_spells_wire.rs`；`crates/mir-client/tests/round51_spells_wire.rs`仅必要加强；证据`target/round52/c/**`。B4评审/Lead提交后启动；所需服务端测试接缝若超前批范围先尚书登记，不越pub(crate)私有边界。
启动契约：读net.rs:5473/5125/5360与WorldSnapshot消费链，接B1/B2/B3真实服务端产包，经实际encode→decode→WorldSnapshot。DeleteItem按UUID覆盖装备部分扣数/归零摘槽、错误UUID不动、背包同模板不误扣且保留背包行为；逐物品准确Count/一次包，失败无包，联核生产存档重登；原来仅delete_inventory_item属致命缺口。149/125实际生成/撤销及入视野消费不能延期。152/153分别以错误预测格+非空既有motion/队列为前置，验证权威位置/Standing/清队列；隔离副本变异或明确反事实证明消费删除后必红，不在共享树删分支。落实D5、M01/M03/M04/M05/M08。
验收命令：`cargo test -p mir-client round52_ -- --nocapture`；`cargo test -p mir-client --test round52_spells_wire -- --nocapture`；`cargo test -p mir-client --test round51_spells_wire -- --nocapture`。named tests至少`round52_wire_equipped_material_count_and_field_lifecycle`、`round52_dash_fail_wrong_prediction_with_existing_motion`，后者分别断言152/153。手造成功包或仅背包快照即BLOCK；true-wire不宣称真实socket，不以素材延期放过场生命周期。

### T / 刑部 / gpt独立终审

写范围 `docs/testing/round52/round52-final-audit.md`、`docs/testing/round52/final-manifest.json`、`docs/testing/round52/README.md`及证据`target/round52/t/**`；其它只读。C完成评审/功能提交且Lead门禁exit0后做最终裁决，不再跑全量。
启动契约：读七子单真实show/结果、全部attempt原日志/meta、最终功能树与R51残余；对G1–G7、D1–D5、M01–M08逐项映射到真实子单/源码/测试名/日志范围。重算bytes/SHA，核named tests非空、失败史完整、默认数据来源/预批准PARTIAL准确；必须复核B1→B2/B3→C装备消耗/存档闭环及Trap专属AI。定向复跑必要命令并记真实exit，不写代码/测试；可报告BLOCK或严格有界PASS-WITH-PARTIAL，核心不满足返工。落实全部D/M。
验收命令：`py tools/docs/check_links.py`；`git diff --check`；按需要复跑上列定向命令，不运行workspace全量。manifest逐文件列bytes/SHA/attempt及被测完整HEAD/功能树指纹，终审核与被测HEAD仅文档差异。任何源代码注释/断言/工具/配置/语言包/素材manifest改动都使门禁失效，退Lead重新冻结与门禁。

## 4. M条款主责与硬失败判据

| 条款 | 落实点 / 联验负责人 |
|---|---|
| M01 | B1原语及生产存档，B2/B3真实施法，C装备UUID编解码快照，T闭环；仅背包、假包或重登缺失BLOCK。 |
| M02 | B1双项同锁原子，B2/B3失败材料矩阵及MP/动作/SpellTime/CastTime/延迟包反例；缺第二项先扣第一项BLOCK。 |
| M03 | B3 3×2预检/3×3落地、安全剪裁、Shock非Frozen、CanAttack/受击/专属AI；C状态/场消费。 |
| M04 | B2毒雾延迟/场/tick/PK/死亡及pending/存量清场分验；C 149/125和入视野；D3偏差明记PARTIAL。 |
| M05 | B1新模块私有单测、pub(crate)、生产存储链；B2/B3/C公开真实链；Session私有current_row禁外调，越文件先登记。 |
| M06 | B4默认七技能/来源SHA/独立期望、真实状态概率双侧、真实拒攻、11上界/来源/RNG、五位置/跨图。 |
| M07 | 本派发七真实子单/依赖/写范围/路由，Lead一名成员串行启动、各批独立gpt评审、Lead唯一提交。 |
| M08 | 各批named tests实际≥1、零失败、原exit/失败史/≤1MiB及SHA；Lead唯一全量门禁，T核功能树/仅文档后置。 |

## 5. 门禁、证据、PARTIAL与禁止捷径

- 原证据仅`target/round52/<a|b1|b2|b3|b4|c|t|gate-attempt>/**`并保持ignored。每attempt记录command/cwd/完整HEAD/功能与测试及工具配置路径blob指纹/dirty状态/时区/start/end/raw exit/实际用例数/关键断言；失败同样保全。
- 单证据文件及受控入库批≤1MiB；超限按用例或连续编号分片，保留完整原字节与SHA清单，不能截尾/改码页/删失败。可入库审计摘要与manifest在`docs/testing/round52/**`且≤1MiB，不能替代原日志。排除target、.env/凭据和两份历史大冻结JSON。
- 只有Lead在最终功能树提交并冻结后运行`pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`，核实际12项（非旧注释13项），PG/协议/links/TextKey/双i18n/legacy/icon等不得skip；遵守soak锁，不盲杀其它agent。每attempt独立log/meta，代码/测试/工具/配置变化令旧结果失效。
- T、尚书、门下后置只改审计/manifest/过程文档，记录相对被测HEAD的纯文档差异并复跑links/diff检查；不夹带源码注释或断言。Lead按受控路径提交，禁止`git add .`。
- G1–G3及G6耗材/场链为核心硬门槛，G7必交；任何缺失不得称“两技能完整”。G4/G5逐记录/子项closed或点名PARTIAL；缺记录可按D5列对应G4 PARTIAL，缺日志/零测试/伪fixture/空断言/skip不可用PARTIAL通关。
- 已批准PARTIAL边界：D3生命周期偏差、其它旧技能耗材、guild/1965/1966嵌套槽PoisonAttack/Freezing、原生视觉/音频/dash与真socket。根装备模板/added/有效buff、149/125快照仍须本轮实证。专属AI核心漏控或支持模板裁剪须门下另裁。
- 保留R51十一项逐条映射，P03/P10 closed为历史事实；P04火墙ProcessSpell/InTrapRock、P08原生dash及其它明确顺延系统不被本轮局部绿灯吞并。R50商城残余、PetSave/PetExp/AOI全量/攻城/坐骑/天气/光照/攻击变招/CallDefaultNPC/行会UI均不扩入。
- 禁止只扫槽9、背包补材料、跨堆拼数、按耐久取代Count、先扣符后验毒、UseItem冒充DeleteItem、统一失败退款或保持Unsupported冒充实现；禁止瞬时AOE/手工注毒、Frozen冒充Shock、仅动画无控制、扩public绕测试、假默认零冷却fixture。
- 禁止协议250/251改号、放松门禁、allow/ignore/skip、删旧反例/坏日志、改五位置存在性断言、Freezing7上界12、硬编码中文绕TextKey、gpt写测试或产品代码、并发共享文件/DB门禁、未登记先执行。
- 执行子单在真实验收/独立评审满足后由承接角色`delegate-result`；主单仍须全部委派及todo齐备后尚书done→Review→请求Done→指定menxia确认，禁止Doing直Done。本文件不代最终验收。

## 6. 本次看板回执

核验前`validate --json`：`ok:true / schema:v3.1 / tasks:117 / issues:[]`，父单Assigned。A/B1/B2/B3/B4/C/T依表顺序逐一`delegate ... --as shangshu`，七次均exit0并返回上表真实ID；均为父单直系委派，未重复创建主单。
`memo JJC-20261025-004 shangshu ... --as shangshu`已记录映射/严格DAG/M01–M08与风险，exit0；`state JJC-20261025-004 Doing "尚书派发：七子单已登记" --as shangshu`回执`Assigned → Doing`，exit0，无exit3。
最终`validate --json`：`ok:true / schema:v3.1 / tasks:124 / issues:[]`，exit0；真实`show`确认父单Doing、now为“尚书派发：七子单已登记”，末子单owner=xingbu、依赖C+Lead门禁完整。子单Doing为CLI登记默认状态，不代表依赖满足或执行已启动。
无执行代理启动、无cargo/git写操作、功能验收均未运行；Lead应先启动A，其后按DAG与独立评审逐批释放。派发文档的最终bytes/SHA及文档静态校验回执另经send_message交Lead，不将文件自身SHA写回产生循环。
