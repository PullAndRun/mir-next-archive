# R77 149 Trap 八帧映射差异与素材准入 Implementation Plan

> For agentic workers: REQUIRED SUB-SKILL: subagent-driven-development 或 executing-plans；门下独立批准后实施，作者不得自批。
> 中书规划；TASK_ID=JJC-20261028-004；实读 HEAD=origin/main=69428266cd00d900abafa97bd415b47b4f4782a5。当前仅送审，不启动执行者。

**Goal:** 生产零改，将149 Trap 的 Crystal 八帧要求、Rust当前通用闪光与默认素材不可准入，固化为可失败、可复算的差异合同。
**Architecture:** 新增七项集成测试，分别走真实wire/生产绘制与公开渲染API；独立Python工具只读Crystal和本地WZX/WZL，输出来源准入报告。参考映射、实际行为、素材就绪三种结论分列，拒绝把异版同号帧视为等价。
**Tech Stack:** Rust legacy合成夹具、CPU RGBA、Python标准库struct/zlib/hashlib/json；无新依赖，不调用数据库或下载器。
**Spec:** [roadmap最新§二十四](../roadmap.md#L374)、[R76 PARTIAL](../testing/round76/README.md#L24)、[R76门下收口](round76-menxia-confirm.md)、[R76后续前置](round76-plan.md#L158)、[R75差异合同先例](round75-plan.md)。

## 背景、候选比较与裁决

R76看板实读Done；README/roadmap仍是历史pre-audit措辞，本轮不回改历史。R76仅149困魔咒1390–1399映射，未完成其余技能、严格寿命、完整混合与素材全链。
分项1–5，越高越优；风险列表示可控度，5为风险低；等权合计25，属于规划判断，不冒称性能实测。

|候选|Crystal依据|单轮交付|可失败验收|风险可控|玩家可见价值|合计/裁决|
|---|---:|---:|---:|---:|---:|---|
|A 149 Trap八帧生产映射|5|1|5|1|4|16，否决；默认Magic2缺2360段，2025异版同号未证等价，且R76 G8精确冻结旧集合。|
|B 149 Trap映射差异与素材准入合同|5|5|5|5|2|22，选定；本轮交付可复算前置，直接可见价值有限，不能称新技能已上线。|
|C 完整DrawBlend/rate/Mode|4|2|4|2|4|16，暂缓；NORMAL为SourceAlpha/One，写BlendFactor不证明src乘0.8，需设备状态证据。|
|D AnimationOffset/Light/mask/tint消费|4|2|4|2|4|16，暂缓；AnimationOffset是格单位，mask依赖第二纹理解码，跨层契约未齐。|
|E 严格149对象寿命/26移除|5|2|5|2|4|18，暂缓；跨net与绘制状态，须先裁定R75现状合同supersede边界。|

音频/真socket/Live/Guild/攻城/地图/寻路/天气均为roadmap真实未做项，本轮不作未读源码的量化评分。R70先做Add入口、R71/73选有限映射而暂缓rate与全局offset、R75选差异合同、R76选十帧映射的裁决已实读承接。
初选A后素材头部预检失败，实际ERR_OUT_OF_RANGE已保留；复查八个本地Magic2索引，仅2025语料包含2360段。发现使主题收敛为B，主单标题保留“149 Trap八帧逐格映射”作为初始意图，当前交付范围以本计划为准。

## 现场实读锚点与决定

|来源文件:行号|事实/本轮约束|
|---|---|
|[SpellObject.cs:30](../../github/Crystal/Client/MirObjects/SpellObject.cs#L30)|149 Load取Location、Repeat=true；不能把瞬态700/4000ms写成Crystal对象寿命。|
|[SpellObject.cs:132](../../github/Crystal/Client/MirObjects/SpellObject.cs#L132)|Trap=Libraries.Magic2、2360、100ms、8帧、Blend=true；不等于TrapHexagon或ExplosiveTrap。|
|[SpellObject.cs:225](../../github/Crystal/Client/MirObjects/SpellObject.cs#L225)|Portal也用2360/100/8；同号并不说明Trap与Portal协议/玩法等同。|
|[SpellObject.cs:288](../../github/Crystal/Client/MirObjects/SpellObject.cs#L288)|初始100ms取整、>=、每Process一步、Repeat回卷；本轮不新增伪生产时钟。|
|[SpellObject.cs:323](../../github/Crystal/Client/MirObjects/SpellObject.cs#L323)|格投影，DrawBlend(...,true,0.8F)；[363](../../github/Crystal/Client/MirObjects/SpellObject.cs#L363) AnimationOffset先加格再乘CellWidth/Height。|
|[MLibrary.cs:692](../../github/Crystal/Client/MirGraphics/MLibrary.cs#L692)|当帧offset先于裁剪；[778](../../github/Crystal/Client/MirGraphics/MLibrary.cs#L778) mask复用主point/width/height，不消费MaskX/Y。|
|[DXManager.cs:353](../../github/Crystal/Client/MirGraphics/DXManager.cs#L353)|NORMAL SourceAlpha/One、INVLIGHT BlendFactor/InverseSourceColor；本轮不推导0.8公式。|
|[spellfx.rs:40](../../crates/mir-client/src/ui/spellfx.rs#L40)|Trap走默认CAST_MS700，R76仅TrapHexagon有独立4000ms窗；本轮生产零改。|
|[spellfx.rs:309](../../crates/mir-client/src/ui/spellfx.rs#L309)|现生产入口；[418](../../crates/mir-client/src/ui/spellfx.rs#L418) Trap走Magic、age/60 clamp0..9、尺寸居中、Over。|
|[entities.rs:497](../../crates/mir-render/src/entities.rs#L497)|Swing为Magic2/start0，Bolt额外+20；公开API能寻址不代表Trap已接线。|
|[net.rs:1541](../../crates/mir-client/src/net.rs#L1541)|SpellEvent有param/from_object_spell/started_ms；[5624](../../crates/mir-client/src/net.rs#L5624)149同时追加event并登记field；[7424](../../crates/mir-client/src/net.rs#L7424)事件与对象身份分离。|
|[generated.rs:5616](../../crates/mir-core/src/enums/generated.rs#L5616)|Spell::Trap=98；原枚举只读，不改生成产物。|
|[analyze_usage.py:330](../../tools/assets/analyze_usage.py#L330)|Magic116帧、Magic2仅0–2/10–14/20–29共18帧；本轮不扩集合。|
|[R76测试:2297](../../crates/mir-client/tests/round76_trap_hexagon_mapping.rs#L2297)|G8精确冻结Magic2旧18帧；未来生产扩集须单列有限supersede，不能静默放宽。|
|[R59测试:98](../../crates/mir-client/tests/round59_projectile_assets.rs#L98)|冻结的是magic目录116帧；不能把Magic2扩帧机械改成R59计数124。|

规划素材预检见[asset-preflight](../../target/round77/zhongshu/asset-preflight.json)：默认resource/data/magic2.wzx=5968B、declared1460、物理索引槽1480，均不足2368；尾部20槽不是可用帧证明。2025索引declared2580，2360–2367头部非零，但未解码、未证明语义匹配。此处只证头部，不称素材可交付。

## 全局约束、文件结构与写入白名单

本次仅本计划及target/round77/zhongshu/**、授权edict CLI；不改crates/tools/其他docs，不commit/push，不启动执行者，不跑cargo/CI。两份monster-native freeze JSON保持原字节及未跟踪状态。
下表为门下批准后的未来执行白名单，**所有生产crates/*/src与素材分析生产脚本均零改**；历史测试/证据原字节不动。

|属主|未来允许写入|责任|
|---|---|---|
|A兵部 deepseek|新增crates/mir-client/tests/round77_trap_readiness_contract.rs；新增tools/testing/round77_trap_readiness.py；target/round77/a/attempt-N/**|七G自包含测试、只读审计工具、固定expected/合成夹具/源码物理副本。|
|B工部 deepseek|target/round77/b/attempt-N/**|独立副本、六变异、Python整帧oracle、完整原日志与清单生成器。|
|C刑部 gpt|docs/testing/round77/round77-c-evidence.md及只增补录；target/round77/c/attempt-N/**|另副本独立G/M复算、范围/来源/原号核验，必改0才能交Lead。|
|Lead|docs/process/round77-*.md；target/round77/{lead,gate,menxia,shangshu,t}/**|独立审议/派发/唯一提交/全量门禁/终审/验收。|
|D礼部 gpt|docs/testing/round77/{README.md,round77-trap-readiness-contract.md,final-manifest.json,manifest.sha256}；docs/roadmap.md和docs/client.md仅追加R77节；target/round77/d/attempt-N/**|后置合同与证据索引，实跑自检及生成器落盘。|

不改resource/game-assets、不替换默认Magic2、不下载、不写数据库、不扩大FxKind或公共API；本轮不是对R76/R75合同的supersede。

## 精确接口、Review Focus与G1–G7

保持生产`pub fn draw_sprites(frame: &mut Frame, events: &[SpellEvent], scene: &mut MapScene, center: Point, now_ms: i64)`。
新增Python接口`audit(crystal: Path, wzx: Path, wzl: Path, binding: dict | None = None) -> dict`；CLI `--crystal PATH --wzx PATH --wzl PATH --out PATH [--binding JSON] [--require-ready]`。报告固定schema_version=1，分列crystal_mapping/default_behavior/corpus_readiness，含源bytes/SHA、declared/physical_slots、逐帧offset/format/size/decode_sha、reasons。
binding固定字段schema_version=1、spell=98、library="Magic2"、base=2360、count=8、interval_ms=100、crystal_sha256/wzx_sha256/wzl_sha256、review_evidence_path/review_evidence_sha256；全部字段/实存证据摘要匹配才移除UNVERIFIED_SEMANTICS。它只检查人工审核绑定的完整性，不自动证明图像语义；真实binding必须经独立门下批准，合成known fixture仅用自有测试证据、不授权实际语料。G5须额外覆盖无binding、源SHA错配、证据摘要错配均require2。
普通审计完成EXIT0，即使NOT_READY；`--require-ready`只有索引边界/尺寸/解码/来源映射证明全满足才EXIT0，否则EXIT2；损坏/读取错误EXIT1。同号/非零尺寸不构成映射证明，2025来源默认reason含UNVERIFIED_SEMANTICS。
七G恰七个同名Rust测试；G5/G6从Rust调用Python并断言JSON和退出码，缺Python/legacy/素材不SKIP。合成夹具及金样先冻结bytes/SHA/UTC，再首actual原stdout；不得用生产投影/混合helper生成expected。

|G / 测试名|固定断言与可失败条件|
|---|---|
|G1 r77_g1_wire_current_mapping|真实149 encode→decode→apply_packet_at，Trap98、ID/location/param两值/from_object_spell/started_ms逐字段；age0/120现画Magic0/2居中Over，Magic2同号及2360异色诱饵。固定整帧相等；若画Magic2或取错锚点即红。|
|G2 r77_g2_current_window|Trap age -1空、0/59为Magic0、60为Magic1、599/600/699为Magic9、700/799空；i64极值不panic。独立标注Crystal100ms/8帧循环要求，不把此现状当目标能力。|
|G3 r77_g3_sources_and_controls|真实120/123各cast两值、149两param均保持Trap通用行为；TrapHexagon/FireWall/PoisonCloud及成功雷电分别独立金样，防误扩大技能集合。此处Trap≠TrapHexagon。|
|G4 r77_g4_renderer_capability_only|合成Magic2 2360..2367逐帧可读，直接公开draw_effect_blend(Swing,index,...,true)八图；非黑底、饱和、透明黑、正负offset/屏外移入、两次清帧绘制整帧相等。与G1明确相反的寻址路径，只证渲染API能力。|
|G5 r77_g5_corpus_admission|合成declared2368/合法8帧但无语义证明→NOT_READY/require2；附明确审核绑定证明的合成known fixture→READY/0；截断WZX/越界WZL/zlib坏流→ERROR/1。真实默认declared1460/physical1480→MISSING_INDEX/2；2025即使解码成功仍UNVERIFIED_SEMANTICS/2。|
|G6 r77_g6_crystal_mapping_and_plan|工具实读Crystal的Trap case，输出Magic2/2360/100/8/Blend=true/继承Repeat=true/AnimationOffset(0,0)；不得误取TrapHexagon/Portal/ExplosiveTrap case。真实analyze_usage输出Magic116且Magic2旧18精确不变，旧集合独立冻结，不借生产常量算expected。|
|G7 r77_g7_ui_query|真实149快照→UiState.sync→公开ui::draw，避HUD/文字ROI对含/缺Magic0 scene的差分与独立Over金样一致；重复绘制及events/fields逐字段不变；缺Magic0不fallback2360且后续火墙仍画。|

五焦点：技能名与来源混淆(G1/G3)、700ms/60ms与100ms/8帧混淆(G2)、直接渲染能力冒充生产接线(G4/G7)、声明计数/尾槽/异版同号偷准入(G5)、错误case与历史素材锁漂移(G6)。
G4独立公式RGB=min(255,dst+(src*a+127)/255)，a0保持dst否则alpha255；G1/G7用Over固定金样，不因黑底巧合把两种混合当等价。B/C各用struct/zlib独算实际合成WZX/WZL与所有像素采样，报告各自case数及Rust expected/actual/Python三源一致。

## 六项精确变异与有效红标准

|M（只在独立物理副本单变异）|指定红|
|---|---|
|M1 spellfx通用kind选择的else仅Magic→Swing|G1错库异色；断言与夹具SHA不动。|
|M2 CAST_MS 700→800|G2 age700/799应空却有Magic9；基线先保证该帧可读。|
|M3 通用scene.draw_effect改为draw_effect_blend并追加true参数|G1尺寸居中后又offset/Add，非黑底整帧红；须编译成功。|
|M4 entities的draw_effect_impl只禁用apply_offset坐标叠加，Add算法不动|G4正负offset/屏外移入整帧红；不得改测试调用。|
|M5 审计工具只删除UNVERIFIED_SEMANTICS准入拒绝条件|G5合成同号未认证8帧应require2实际0；输出JSON合法，非异常红。|
|M6 审计工具Trap case匹配改为TrapHexagon case|G6 Magic2/2360/100/8字段断言红；不改固定expected，不以parser异常作红。|

每M前绿7/0/0→冻结测试/fixture/tool/oracleSHA→单patch→no-run编译EXIT0→指定G恰1项断言红EXIT101→恢复原字节SHA→后绿7/0/0；B/C分别独立重做六项。M5/M6的Python应正常完成审计，Rust断言失败才是红；0tests/编译失败/缺资源/ignored不算。
基线预期七绿，本轮无生产修复红；负控仅证明合同可失败，M1–M4是现状锁敏感性，不把这些未来可能改变的行为宣称正确目标。

## DAG、交接与实施步骤

`门下独立approve → 尚书先登记 → A → B → C → Lead唯一提交/全量门禁 → D → 独立终审 → 尚书验收 → 门下确认 → Done → Lead push`

- [ ] A（依赖批准，deepseek）：冻结七G/三类expected与夹具；先验证G5的拒绝路径，再实现只读工具并七绿；六项负控至少自查M1/M5；定向回归；交原字节、完整argv/UTC/首actual/stdout与manifest，不commit。
- [ ] B（depends_on=A，deepseek）：新物理副本nlink1，无symlink/junction/hardlink；独立CARGO_TARGET_DIR/TEMP/TMP/TMPDIR，七G/六M/像素oracle，生成器按固定排除规则重算inventory，不改A原件。
- [ ] C（depends_on=B，gpt）：另副本独立重做七G/六M/oracle；核源码锚点、非生产属性、真实素材拒绝理由、原日志/恢复SHA、历史测试与freeze保全，必改0才交Lead。
- [ ] Lead（depends_on=C必改0）：仅提交获批白名单，执行唯一全量并核门禁后生产/工具无新差异；失败attempt全部保全，修复后新attempt重闸。
- [ ] D（depends_on=成功门禁，gpt）：四测试文档与两追加节；身份分feature_head/gate_head/delivery_head，清单外置摘要免自引用；自己实际运行UTF8/LF/链接/JSON/逐项SHA检查并落target/round77/d/attempt-N/checks.json及原logs，不重跑cargo。

A交接包含固定case列表、known fixture语义证明格式及完整Python argv；B/C只消费同SHA并各自生成原日志；若发现合同缺口退A新attempt后重做链，禁止边改assert边沿用旧绿。

## 未来验收命令、门禁与提交策略

以下均未在本规划运行；新套件应7 passed/0 failed/0 ignored；每G追加名称及`-- --exact --nocapture --test-threads=1`须恰1项。

```text
cargo test -p mir-client --features mir-render/legacy --test round77_trap_readiness_contract -- --test-threads=1
cargo test -p mir-client --features mir-render/legacy --test round70_effect_blend_offset --test round71_firewall_mapping --test round72_poison_cloud_mapping --test round73_thunderbolt_hit_mapping --test round74_normal_blend_lock --test round75_spell_lifetime_contract --test round76_trap_hexagon_mapping --test round59_projectile_assets
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p mir-client --all-targets --features mir-render/legacy -- -D warnings
pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale
```

官方clippy与legacy clippy分列原EXIT/既有lint，不把后者历史101报0；新增lint必须修。全量仅Lead跑，不加SkipTests/SkipPostgres；脚本内部skip/ignored项目逐项披露，12/12不等于无条件全测，实际项目数以日志为准。
证据根target/round77/**；每命令记UTC起止/cwd/HEAD/argv/环境/EXIT/计数/stdout/stderr/bytes/SHA。失败也立即保留，attempt只增，禁止rm清旧日志、mtime复用导致不重编与inventory自引用；清单生成器固定排序、逐文件SHA/bytes与排除规则，B/C/D可从原目录复算。
本轮当前只交规划；通过静态检查不等于七G/六M通过。未来只由Lead提交，A/B/C/D不commit；功能门禁后纯文档不重跑，全量后crates/tools改变必须重闸。回滚仅逆撤本轮新增测试/工具/文档，不reset共享树、不删除历史证据。

## PARTIAL台账、风险、不做与R78前置

|原号/历史|R77处置|仍保留|
|---|---|---|
|GAP-5/R70-C1|仅Trap98映射差异/素材准入合同|其余149/Trap真实映射、1620起手、其它调用点；不机械把“其余26技能”减1。|
|GAP-1/2/R70-C2|无生产改动|完整DrawBlend/DrawColour/rate/Mode/INVLIGHT，禁止据BlendFactor推出src×0.8。|
|GAP-3/4/6、R70-C3|无生产改动|mask/tint/shadow消费、Light/AnimationOffset/全局尺寸高度/居中；shadow仍只解析。|
|R71时钟/对象、R73-T1|记录差异，不修寿命|NextMotion/Repeat/26移除/owner/ActionFeed/声音及700/4000产品窗偏差。|
|R72/R59有限supersede|保全现字节，不新增supersede|R72 B plan-M4、legacy lint双口径、R59旧106子集与现116集。|
|R76流程|新轮要求可复算inventory及D自检原件|旧inventory无生成器不可复算、D未留evidence、A sidecar与A R59首败损失、B早期失败日志永久损失；新证据不追溯补造旧字节。|
|R75七项|逐项原号承接|①GAP-1/2/R70-C2；②GAP-5/R70-C1/R70-C3/GAP-3/4/6；③R71时钟/R73-T1；④R72 B plan-M4/legacy lint/R71-C7永久/历史E1仅补救CLOSED/R69条件5；⑤R68/R49–R70/R61–R67；⑥R75-E1/C-M1；⑦R75-E2/C-M2。|
|R61–R67细项|无生产改变|R61 P01–P12、R62①–④、R63 P1–P7、R64①–⑥、R65 G1.2/G2、R66生命周期/全局水位、R67容量/保护/map epoch；R74 artifacts实数34照承。|
|roadmap§二十四与其它未做项|只新增合同|素材本地gitignored、设备视觉/GPU、音频WAV/PreviewSound/音乐通道、真socket/Live、Guild/攻城/地图/寻路/天气均未验收。|

风险：审计工具可能把同号素材误当语义证明，由G5/M5挡住；现状锁未来生产迁移会红，须显式supersede而非放宽assert；源码文本解析脆弱，由G6和原文hash保留可追溯性。合成已认证fixture只检准入算法，不能给2025语料背书。
不做：运行时映射/新素材绑定/默认库替换/扩素材计划、生产新时钟/删除事件、全技能审计、下载或DB写入、完整渲染与设备/可听/Live结论。
R78前置：R77完整审议与验收；若再选Trap生产映射，先找到与Crystal一致且可解码的来源及正式绑定路径，证明旧0–2/10–14/20–29素材不变，显式裁定R76 G8的18→26集合supersede与本轮G1/G2/G3/G7现状锁替代范围，再提出有限生产白名单；不默认接受2025同号，不自动开执行。

## 待门下裁决与规划回执

1. 是否接受“生产零改、七G/六M、可复算前置合同”的有限完成定义，玩家可见价值仅为后续映射降低风险；若必须本轮可见增量则封驳另选，不带缺素材条件硬上生产。
2. 是否接受默认NOT_READY、2025 UNVERIFIED_SEMANTICS与合成known fixture只证准入逻辑的三分口径；不得把audit EXIT0写成ready。
3. 是否接受不改R76 G8/R59/历史测试、不新增supersede；严格生命周期与其余PARTIAL全部保留。
4. 是否接受B/C六变异独立物理副本、inventory生成器/失败原件与D实存自检作为硬交付；作者不自批，不转Assigned。

[规划笔记](../../target/round77/zhongshu/planning-notes.md)、[自检](../../target/round77/zhongshu/plan-checks.json)、[CLI真实回执](../../target/round77/zhongshu/cli-receipts.json)保存本轮行数/bytes/SHA、UTC/EXIT及validate条数；静态通过仅代表计划可审。
