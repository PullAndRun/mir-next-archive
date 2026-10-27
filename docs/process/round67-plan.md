# 第67轮：位置缓存容量与保护引用契约 Implementation Plan

> **For agentic workers:** 门下独立批准后由尚书登记 A→B→C→D，使用 subagent-driven-development 或 executing-plans 技能；本次仅规划，未启动执行者。

**Goal:** 用测试私有淘汰提案和真实快照/UI 差分，钉死位置缓存容量政策的安全边界；生产限容继续 PARTIAL。
**Architecture:** 参考函数仅计算淘汰候选，不链接生产、不改变 known_positions。真实71/125链分别预热完整历史与候选保留历史，验证缓存命中/缺失对出生实体和cue的影响；预算为测试输入，不设置生产默认值。
**Tech Stack:** Rust integration tests、WorldSnapshot、UiState、既有协议编解码；Markdown/JSON。
**Spec:** [路线图](../roadmap.md#L236)、[R64计划](round64-plan.md#L24)、[R65计划](round65-plan.md#L37)、[R66计划](round66-plan.md#L116)、[R66验收](round66-shangshu-acceptance.md#L24)。

## Global Constraints 与事实核验

- 主单 `JJC-20261027-004`「第67轮：位置缓存容量与保护引用契约」；核表当日001/002/003均Done，无004/R67重单，create与Taizi→Zhongshu均EXIT0；随后Zhongshu→Menxia并show实读Menxia，validate --as zhongshu EXIT0、207条符合schema v3.1；送审不代表获准实施。
- 基线 HEAD=origin/main=`52d95b133d040e6d3c4ddded113be126059b2111`；log -5为52d95b1/7374422/b3ce9dc/dde1af7/02ca88b。
- 初始status仅两份历史未跟踪 `docs/testing/monster-native/player-resource-freeze-before.json`、`docs/testing/monster-native/player-resource-freeze.json`，不暂存/覆盖；规划骨架先落盘再迭代。
- `create --help` 实际EXIT1“缺少标题”；全局help及CLI源码1626–1629行无--desc，实际采用--remark保存范围，原失败不伪称成功。回执在 target/round67/zhongshu/。
- 本阶段只写本文及 target/round67/zhongshu/**，只经CLI改看板；不建子单、不改crates、不commit/push、不运行cargo或全量门禁。
- 已实读 R64/R65/R66计划、roadmap §十三/十四/R61原编号表、R66验收与README；README:3–6的pre-audit是历史，最终看板R66 Done，不回写冻结材料。
- 默认中文可切语言；不新增依赖、`#[ignore]`/`#[allow]`；测试不依赖素材/声卡、不按本地化文案断言；所有“PASS”须来自实际命令。

## 背景缺口与 Crystal 对照（均已逐段打开）

|事实源|已核语义与本轮约束|
|---|---|
|[PlayerObject.cs:3546–3566](../../github/Crystal/Client/MirObjects/PlayerObject.cs#L3546)|当前位置优先，否则TargetPoint；Missile保存落点并进Effects。没有全局位置LRU、容量或保护集策略，参考提案不得归因Crystal。|
|[Effect.cs:38–48](../../github/Crystal/Client/MirObjects/Effect.cs#L38)、[144–194](../../github/Crystal/Client/MirObjects/Effect.cs#L144)|Owner/Source属于效果；Missile加入活动列表、保存Destination、跟踪Target，Remove移出。只能支撑活动引用须保留落点的语义，不能推导缓存容量。|
|[MonsterObject.cs:346](../../github/Crystal/Client/MirObjects/MonsterObject.cs#L346)|SkipFrames来自ActionFeed.Count>1，与缓存LRU无关；不把此行当淘汰依据。|
|[GameScene.cs:4687–4714](../../github/Crystal/Client/MirScenes/GameScene.cs#L4687)|源缺失return；410投射物出生音，目标存在且未死才570/命中音。Rust缓存miss无出生是既有披露差异，不扩原作全等主张。|
|[ServerPackets.cs:3564–3578](../../github/Crystal/Shared/ServerPackets.cs#L3564)|125只有Spell/Source/Destination，无落点/缓存容量/ack；禁止新增字段。|
|[projectile.rs:579–585](../../crates/mir-client/src/ui/projectile.rs#L579)、[764–778](../../crates/mir-client/src/ui/projectile.rs#L764)|known_positions是HashMap；每帧insert、换代clear，没有容量/淘汰。位置缓存不等于事件表。|
|[projectile.rs:794–827](../../crates/mir-client/src/ui/projectile.rs#L794)、[849–903](../../crates/mir-client/src/ui/projectile.rs#L849)|源必须当前可见；目标当前→缓存→miss不生成；在飞/命中特效更新又写缓存。淘汰历史会改变未来生成，不能偷偷接生产。|
|[既有C2测试:1386–1405](../../crates/mir-client/src/ui/projectile.rs#L1386)、[R66 fixture:63–100](../../crates/mir-client/tests/round66_consumer_boundary.rs#L63)|已有单目标回退，不算R67新增；可复用71/125编码→解码→apply_packet_at写法，但不跨target导入私有helper。|
|[R66 README:107–136](../testing/round66/README.md#L107)、[roadmap:242–251](../roadmap.md#L242)|位置容量/LRU、生产全局水位与服务端协议仍PARTIAL；本轮只补D1契约，不清零整项。|

## 选题、边界与决策记录

- **主方向D1，限定契约＋可失败回归；无副项。** 单轮增量是“确定淘汰候选/保护超额”与“淘汰历史确会少出生”的可执行差分；无需资源库、设备、生产注册或wire。
- 不做生产LRU/配置项/硬上限/内存长跑结论；不接app/net会话循环，不改server、协议、现有测试、动作FIFO、音频或素材，不声称已限容。
- D2需生产注册/退出/原子性与Receiver分发，R66只有参考模型，单轮风险高；D3/D5依赖105素材来源/哈希/索引、Live/HiddenSpanSource/削波独立链；D4尚未核出新的场景缺口与红侧，均移出。
- “严格参考、禁止自创”的处理：仅保全上表原作可证行为；下面LRU候选是本项目**待裁决测试提案**，不进入玩法或生产。若门下认为提案也超范围，封驳重选，不静默升级运行时策略。

## G1 精确接口与契约（仅tests/support）

- 新 `tests/support/round67_positions.rs`：`pub(crate) struct EvictionPlan { pub evict: Vec<u32>, pub remaining: usize, pub over_budget: usize }`，derive Debug/PartialEq/Eq。
- `pub(crate) enum PlanError { DuplicateId }`，derive Debug/PartialEq/Eq；`pub(crate) fn plan_evictions(entries: &[(u32, u64)], protected: &[u32], budget: usize) -> Result<EvictionPlan, PlanError>`。
- entries是唯一(id,last_used_tick)集合；任意重复id先返回DuplicateId，不修改输入。protected重复/未知id无副作用、不增加条目；仅表示测试显式假设，不声称生产已收集引用。
- 计算：候选为未保护条目，按(tick,id)升序；删最少前缀使remaining≤budget，候选耗尽即停；evict按该顺序，remaining=entries.len−evict.len，over_budget=remaining.saturating_sub(budget)。
- budget=0仍保护全部受保护条目，不能以限容名义丢活跃引用；budget≥len不删；相同tick以id稳定排序，输入排列不影响结果；绝不造坐标/淘汰未确认事件。
- 可见对象、在飞/命中特效目标、尚未处理125的源/目标为未来保护集候选；本轮只显式测试保护ID，不实现自动提取/计时/读写touch或生产时钟。真实LRU接入、map epoch/ID复用仍另轮。

## 可失败验收与 Review Focus（全部必做）

|ID/测试名|固定输入、断言与红侧|
|---|---|
|G1.1 `r67_eviction_order_and_budget`|entries=[(10,1),(20,2),(30,3),(40,4)],protected=[10],budget2→evict[20,30],remaining2,over0；反序entries同结果；tick并列[(20,1),(10,1)]/budget1→evict[10]。隔离变异升序→降序须身份断言红。|
|G1.2 `r67_protected_overflow_zero_and_duplicates`|本行固定entries=[(10,1),(20,2),(30,3)]；全保护/budget2→evict[]/remaining3/over1；budget0仅保护10→evict[20,30]/remaining1/over1；budget≥len不删；protected=[10,10,999]等价[10]；重复entry id→Err且输入不变。去保护/强行截尾变异须红；每分支均有值断言。|
|G1.3 `r67_budget_candidate_changes_future_fallback`|真实71源7=(10,10)、目标1..4分别(11,10)..(14,10)，model entries含7/1..4，ticks分别0/1..4，保护7/1、budget2→evict[2,3,4]、保留[7,1]。完整UI预热全部71；候选UI只预热保留71。两者再收源7可见、目标全缺的同序125目标[1,2,3,4,999]，now=1000冻结；完整新增目的地[1,2,3,4]/坐标四个/cue4，候选仅[1]/(11,10)/cue1；两者consumed=5、重复sync新增view/cue0。差分3证明历史淘汰有代价；不是生产已接淘汰。隔离变异从保护集合移除1须令候选真实view断言失败，不能只败在helper。|
|G1.4 `r67_generation_and_current_position_dominate_cache`|新UI先同代预热目标1=(11,10)，再次可见更新=(19,10)且125→落点(19,10)；新generation首批非空只源7+125目标1，清旧cache后新增view/cue0且游标1；本代随后目标1可见=(13,10)+新125→只生成(13,10)/cue1，重复sync0。隔离移除reset的cache.clear或调换当前位置/缓存优先须相应真实落点/数量断言红。|
|G1.5 范围/矩阵|文档列生产无界缓存、参考候选、保护超额、未知/换代、map/ID复用五行，逐行挂测试或明确PARTIAL；正常构建生产逻辑/wire/server diff=0，无新增ignore/allow。静态越界即失败，不造运行时红。|

- G1.3中的“候选UI”是独立历史预热模拟，未修改生产cache、没有删真实快照事件；实体消失用fixture直接移除并披露非wire，只有71/125真实encode→decode→apply；不称真socket/对象Remove包已闭环。
- G1.3不直接assert模型evict结果，先由其结果筛选历史后跑真实UI，再断言view身份/坐标/cue；防止变异只败在helper、未证明UI安全边界。源7始终可见，999从未见且无cue。

## 子任务 DAG、角色与精确写域（尚未建子单）

`A → B → C → D`；C通过之后由Lead提交功能并独占门禁，D随后记录事实。同文件单owner，依赖未齐不得启动。

|节点/depends_on|责任与白名单|交付|
|---|---|---|
|A/门下批准|兵部deepseek实现/测试；仅新crates/mir-client/tests/support/round67_positions.rs、crates/mir-client/tests/round67_position_policy.rs、target/round67/a/**|G1.1/G1.2接口与红绿；无库导出/运行时引用。|
|B/A有效回执|工部deepseek测试；仅新crates/mir-client/tests/round67_position_boundary.rs、target/round67/b/**；只读A support|path引用support；G1.3/G1.4真实UI差分及隔离红；不改src/旧测试。|
|C/B有效回执|刑部gpt-6.1-sol独立审计；仅新docs/testing/round67/round67-c-evidence.md、target/round67/c/**|独立target复跑G1.1–G1.5及负控，核原始退出/计数/同测试SHA/范围；必改0交Lead，缺陷退原owner，不自修。|
|D/C通过且Lead门禁事实|礼部gpt-6.1-sol；仅新docs/testing/round67/README.md、round67-position-contract.md、final-manifest.json、target/round67/d/**|五行矩阵、原编号台账、真实门禁/红绿索引与SHA；roadmap/client/旧轮冻结只读。|

- [ ] A：写精确assert→隔离实现变异有效红→同一测试SHA功能树绿；签接口交B。
- [ ] B：写真实包夹具/UI差分→隔离模型去保护、cache.clear移除、错误位置优先红→同测试绿；交C。
- [ ] C：独立重跑/最小负控、审生产零差异与历史回归；不把覆盖补证当生产策略实现。
- [ ] D：仅依据实际结果填数字/HEAD/SHA，校links/JSON/编码/清单，交尚书验收与独立门下确认。

## 验收命令与红→绿规则（将来实施，本次未运行）

- A：`cargo test -p mir-client --test round67_position_policy -- --nocapture`；B：`cargo test -p mir-client --test round67_position_boundary -- --nocapture`。新target分别至少2测试，通过数≥2、failed0、ignored0。
- C独立复跑A/B；`cargo test -p mir-client --lib ui::projectile::tests:: -- --nocapture`；`cargo test -p mir-client --test round63_projectile_wire --test round64_projectile_ack --test round65_projectile_lifecycle --test round66_watermark_contract --test round66_consumer_boundary`。
- C：`cargo fmt --all -- --check`；`cargo clippy -p mir-client --all-targets -- -D warnings`；`git diff --check`。D：`py -3 tools/docs/check_links.py`、JSON parse、逐文件UTF-8无BOM/LF/bytes/SHA，manifest自身SHA另记。
- 每owner绝对化 CARGO_TARGET_DIR=target/round67/<owner>/cargo-target，TMP/TEMP/TMPDIR=target/round67/<owner>/tmp；红/绿各用独立cargo-target，禁止硬链接target与复用变异二进制。
- 门下批准包含在target/round67/<owner>/red-copy/attempt-N/**做上述最小变异；隔离副本不得硬链接源码，不写工作树，不提交变异。每变异新attempt，首败原件及SHA永久保全。
- 有效红须运行到指定assert，原始EXIT非零且有测试名/左右实值；编译错、0 tests、skip、既有失败、expect故意panic均不算。基线已绿只算覆盖补证，必须另取变异红。
- 同一测试文件红绿SHA不变；记录完整HEAD、UTC起止、完整命令、双流、EXIT、源码/补丁SHA/bytes；C独立复现。缺素材/跳过→PARTIAL，不得总PASS。

## PARTIAL 承接清单与禁称

- R49–R60/R59声效音效与R60 P-1–P-6按原轮原编号保留；本轮无素材/音频证据，不整项清零。
- R61 P01生产多消费者/长期内存；P02生产SkipFrames/ActionFeed；P03全局TargetPoint、容量/LRU/失效（本轮只补契约）；P04 ImageInfo.offset/渲染扩面均保留。
- R61 P05/P06设备/Live及33路peak=22.844788削波；P07 105 sfx；P08 HiddenSpanSource全局化；P09 CWD/来源；P10 u64::MAX恢复；P11缺素材SKIP；P12历史R60项，均保留。
- R62①服务端物理回收②位置容量/LRU③生产owner_skip_frames=false④局部570不外推；R63 P1–P7原项继续（含旧gate meta缺EXIT/HEAD）。
- R64①全局最低水位②事件容量③位置容量④服务端回收⑤定向≠门禁⑥历史编号；R65 G1.2仅clone值独立/G2未做；R66模型/消费反例不等于生产注册/退出/租约/补发/fan-out，全承接。
- 禁称“生产LRU/限容已实现”“全局最低水位已接”“服务端ack闭环”“内存已保证有界”“真实可听/全链已证”；只可称位置容量提案/安全边界测试补证，且伴PARTIAL。

## 风险、回滚与 Lead 唯一门禁

- 保护集漏掉未来事件/活动引用会少生成；G1.3必须实际view/cue差分。保护项超额允许超过预算，不能称硬上限；map切换/同代ID复用未证，保留PARTIAL，不扩生产策略。
- 同代空快照不清缓存、新代首批非空才reset；G1.4只证明既有generation边界，不冒充map epoch或实时多消费者。候选历史预热不是运行时逐帧LRU。
- 回滚仅撤本轮新增tests/support/文档；无配置/协议/数据迁移。失败保全退owner；若需src生产修复，先退门下重新裁决与白名单，不偷偷改源追绿。
- C必改0后，只有Lead可提交功能批并在确定HEAD独占 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`；先核soak锁/tmp归属，A/B/C/D不得代跑、commit或push。
- gate/attempt-N保存双流、原始EXIT、UTC、完整HEAD、operator、bytes/SHA；失败不覆盖。门禁后包括测试/注释的任何crates变动使旧门禁失效，Lead新功能提交/新attempt；D只写事实。
- 文档验收后刑部/尚书/独立门下按真实看板收口；本次仅state Menxia与validate，不预写未来PASS。当前计划≤140行，证据/清单另存允许目录。

## 待门下裁决（≤3条）

1. 批准D1仅测试私有预算/保护淘汰提案与真实UI差分，生产LRU继续PARTIAL；是否符合不自创原作玩法的边界。
2. 批准隔离副本的排序/保护/cache.clear/位置优先最小变异及同测试SHA红绿规则；禁止变异进入工作树。
3. 批准候选UI历史预热模拟与明确非wire移除披露；map/ID复用、自动保护集与真正生产限容另轮评审。
