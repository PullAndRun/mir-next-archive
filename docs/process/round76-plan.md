# R76 149困魔咒十帧逐格映射 Implementation Plan

> For agentic workers: REQUIRED SUB-SKILL: subagent-driven-development 或 executing-plans；门下独立批准后才实施。
> 最终实读HEAD：cf84e40f1de1f290357e2a73fcb5adafafdf1fc3（R75门下确认Done）；初读源码基线f43c4244c8d823d6e933f27cc3f4900224904271，间隔仅R75收口文档提交。TASK_ID：JJC-20261028-003；本次只规划，提交门下，不启动执行者。

**Goal:** 将149来源 TrapHexagon（困魔咒）从通用闪光迁至 Magic1390–1399、100ms elapsed循环、格原点与当帧signed offset一次的有限生产切片。
**Architecture:** 仅在draw_sprites内新增来源限定分支及独立4000ms精灵产品窗；沿用Magic/start0与rate1 CPU Add，不迁移持久对象时钟。新增一套八项合同，并显式替换R72的一处旧行为控制组。
**Tech Stack:** Rust、legacy合成WZL/WZX、CPU RGBA、独立Python struct/zlib oracle；现有素材分析/提取工具，无新依赖。
**Spec:** [路线图§二十二/二十三](../roadmap.md#L351)、[R75索引及七项PARTIAL](../testing/round75/README.md#L42)、[R75计划台账](round75-plan.md#L126)、[R74裁决](round74-plan.md#L20)、[R73候选](round73-plan.md#L11)。

## 背景与候选评分

R75锁定的是生命周期差异，生产零改；R76选择可见映射增量，不能宣称严格Crystal寿命已补齐。路线图仍有pre-audit历史措辞，最新HEAD由本页实读值标识，不回写历史。
评分每项1–5、越高越优；风险列为“风险可控度”（5=风险较低），等权总分25。评价是规划判断，不是实测性能。

|候选|Crystal依据|单轮交付|可失败验收|风险可控|玩家价值|合计/裁决|
|---|---:|---:|---:|---:|---:|---|
|A 149困魔咒十帧映射|5|4|5|4|4|22，选定；参数直给，无AnimationOffset/新图库依赖，存在一处旧合同需显式迁移。|
|B 完整DrawBlend/rate/Mode|3|2|3|2|4|14，暂缓；DXManager状态不足以推导NORMAL乘0.8，需设备状态证据。|
|C 全局尺寸居中/AnimationOffset|4|2|4|2|4|16，暂缓；SpellObject的AnimationOffset参与格坐标乘法，不能当像素offset直接加。|
|D 严格149时钟与26生产寿命|5|2|5|2|4|18，暂缓；需对象驱动、显式supersede R75现状合同，跨net/渲染状态。|

- R73注释勘误候选已排除：本次实读spellfx.rs:309已为`CMain.Time > NextFrame`，R74已修，不重复立项。
- Socket/Live、Audio、Guild/攻城、地图/寻路/天气本次未做源码研究，不编造评分；各需独立单轮范围。
- A保留rate/寿命偏差，但可用异色帧、边界及真实wire断言证伪；比继续增加现状锁更直接改善困魔咒可见性。

## Crystal与Rust现场实读锚点

|文件:行号|事实与实施约束|
|---|---|
|[SpellObject.cs:30–48](../../github/Crystal/Client/MirObjects/SpellObject.cs#L30)|Load记录包Location、AddObject、Repeat=true；TrapHexagon为Libraries.Magic、DrawFrame1390、FrameInterval100、FrameCount10、Blend=true。|
|[SpellObject.cs:288–302](../../github/Crystal/Client/MirObjects/SpellObject.cs#L288)|初始100ms向下取整、Time>=NextMotion、每Process一步、Repeat回卷、next=now+interval；本轮elapsed不是该状态机。|
|[SpellObject.cs:323–339](../../github/Crystal/Client/MirObjects/SpellObject.cs#L323)|DrawLocation格投影/显示移动偏移，DrawFrame+FrameIndex、DrawBlend(true,0.8F)。本轮仅格原点/帧图/offset，CPU rate1为明确偏差。|
|[SpellObject.cs:363–370](../../github/Crystal/Client/MirObjects/SpellObject.cs#L363)|AnimationOffset加在CurrentLocation后乘CellWidth/Height，支撑候选C暂缓；困魔咒没有此赋值。|
|[MLibrary.cs:692–709](../../github/Crystal/Client/MirGraphics/MLibrary.cs#L692)|先point.Offset(mi.X,mi.Y)后裁剪；SetBlend传rate，仅恢复旧bool。|
|[DXManager.cs:353–384](../../github/Crystal/Client/MirGraphics/DXManager.cs#L353)|NORMAL SourceAlpha/One，INVLIGHT BlendFactor/InverseSourceColor；写BlendFactor不证明NORMAL乘rate。|
|[spellfx.rs:27–61](../../crates/mir-client/src/ui/spellfx.rs#L27)|TrapHexagon当前duration_ms走默认CAST_MS700；本轮不改公共duration_ms/is_area，避免非149/文字层扩窗。|
|[spellfx.rs:287–388](../../crates/mir-client/src/ui/spellfx.rs#L287)|入口先age过滤，已有雷电/火墙/毒云特例；困魔咒下落通用Magic age/60、尺寸居中、Over。新分支放在公共duration过滤前，自带完整age过滤。|
|[entities.rs:497–523](../../crates/mir-render/src/entities.rs#L497)|FxKind::Magic→Magic/start0，Swing→Magic2/start0，Bolt另加20；禁止误用Swing/Bolt。|
|[net.rs:7424–7475](../../crates/mir-client/src/net.rs#L7424)|事件追加、场按ID更新；本轮不改去重/26移除或事件寿命。|
|[R72测试:1381–1486](../../crates/mir-client/tests/round72_poison_cloud_mapping.rs#L1381)|r72_g5_legacy_sources显式要求149 TrapHexagon绘制旧magic2号帧；生产迁移必使该旧合同失效，须下述有限supersede。|
|[analyze_usage.py:322–331](../../tools/assets/analyze_usage.py#L322)|MAGIC_FRAMES当前无1390–1399；需只追加此十帧，旧帧集合保持，不能只靠合成夹具。|

## 全局约束、文件结构与白名单

本次规划写入仅本计划与target/round76/zhongshu/**，另经用户指定CLI操作看板；不改crates/tools/其他docs，不commit/push，不跑cargo/CI，不启动A/B/C/D。
下表是门下批准后的未来执行白名单，不是本次写入授权扩大。

|属主|未来允许写入|责任与边界|
|---|---|---|
|A兵部|crates/mir-client/src/ui/spellfx.rs|只新增困魔咒常量/来源分支/准确注释；其余分支零语义差异。|
|A兵部|新增crates/mir-client/tests/round76_trap_hexagon_mapping.rs|自包含八项命名测试、夹具、独立固定expected。|
|A兵部|crates/mir-client/tests/round72_poison_cloud_mapping.rs|仅r72_g5_legacy_sources中的TrapHexagon控制组换为Reincarnation：两处Spell枚举及两处对应注释；既有像素expected/fixture/其余测试不改。|
|A兵部|tools/assets/analyze_usage.py|只给MAGIC_FRAMES追加range(1390,1400)及本行说明；不顺手修其他历史注释。|
|A/B/C各自|target/round76/{a,b,c}/attempt-N/**|源码物理副本、冻结夹具/期望、oracle、patch、完整日志及hash。|
|Lead|docs/process/round76-*.md；target/round76/{lead,gate,menxia,shangshu,t}/**|独立审议/派发/裁定/唯一提交门禁，先登记后启动。|
|C刑部|docs/testing/round76/round76-c-evidence.md及只增附录|逐G/M独立证据，BLOCK历史不覆写。|
|D礼部|docs/testing/round76/{README.md,round76-trap-hexagon-contract.md,final-manifest.json,manifest.sha256}；docs/roadmap.md与docs/client.md仅追加R76节；target/round76/d/**|范围、显式supersede、原号台账、逐项清单与后置文档检查。|

- 本地game-assets仅按既有工具重提取magic；生成物不入库，不扩大源码白名单。先查本地资源索引/既有提取参数，不新增下载或数据库写入。
- 历史R72证据/提交/合同原字节不动；R76合同写明supersedes仅“r72_g5_legacy_sources的149 TrapHexagon旧闪光要求”。替换控制技能保持原用例检测其他技能不误迁移的目的，不能称R72历史字节不变。
- 两份未跟踪monster-native freeze JSON原字节/状态保全、不入库；不reset共享树，不新增ignore/allow，不改历史R70/R71/R73/R74/R75测试。

## 精确接口与实现决定

- 保持`pub fn draw_sprites(frame: &mut Frame, events: &[SpellEvent], scene: &mut MapScene, center: Point, now_ms: i64)`。
- 新增常量`TRAP_HEXAGON_BASE_INDEX: u32 = 1390`、`TRAP_HEXAGON_FRAME_MS: i64 = 100`、`TRAP_HEXAGON_FRAME_COUNT: i64 = 10`、`TRAP_HEXAGON_SPRITE_MS: i64 = 4000`。
- 在循环首部算age=now_ms.saturating_sub(event.started_ms)，仅`event.spell == Spell::TrapHexagon && event.from_object_spell`进入专属分支；param两值均准入。
- 专属分支仅0<=age<4000绘制；phase=(age/100)%10，index=1390+phase；world_to_screen(center,event.location)→draw_effect_blend(frame,FxKind::Magic,index,x,y,true)，无条件continue；负age/窗外也continue，禁止落回通用。
- 不取effect_size、不居中、不预加offset；一包一落格，一事件一张当前帧，不额外铺3×3/六边形。公共duration_ms仍700，仅该149精灵4000，文字/色块仍700。
- 4000ms沿用既有149瞬态精灵产品窗的选择，**不是Crystal困魔咒寿命事实**；26残影/同ID重入/Repeat/严格时钟仍PARTIAL。不重写R75针对火墙毒云的合同。
- 非149困魔咒120/123仍通用700ms、60ms帧/居中/Over；所有其他技能现状保持。

## Review Focus与八项G可失败验收

恰八项命名测试；各测试固定expected先冻结bytes/SHA/UTC，再首次actual。测试不用生产phase/投影/混合helper计算oracle。

|G / 测试名|必须成立的断言与失败条件|
|---|---|
|G1 r76_g1_wire_mapping|真实149 encode→decode→apply_packet_at→snapshot，TrapHexagon value73、ID/location/started_ms/from_object_spell逐字段断言；param=false/true均画Magic1390。Magic0/1391与Magic2同号异色诱饵，整帧错误即FAIL。|
|G2 r76_g2_frames_and_wrap|k=0..9采100k与100k+99→1390+k；1000→1390、1100→1391、3999→1399；每帧异色/非零offset，独立整帧金样，错间隔/错帧数/夹死即FAIL。|
|G3 r76_g3_window_and_source|149 age -1空、0/699/700/999/1000/3999有、4000/4001空，i64极值不panic；非149真实120/123两种cast在699仍旧图、700/999空；专属窗漏准入/公共700先截断即FAIL。|
|G4 r76_g4_anchor_offset_crop|1390尺寸3×3/offset(-3,5)、1391尺寸5×2/offset(4,-2)；受控格原点(384,256)，变center、重复清帧draw、屏外格经offset移入、部分裁剪；格原点+offset一次整帧相等，居中/先裁剪即FAIL。|
|G5 r76_g5_add_and_missing|非黑底/饱和/透明黑、同格两149叠加；RGB=min(255,dst+(src*a+127)/255)，a0保持dst，否则alpha255。缺1391但1390/1392存在不得fallback，后续火墙仍画；错Over/缺帧吞列表即FAIL。|
|G6 r76_g6_ui_entry|同149快照经UiState.sync→公开ui::draw，受控time；含帧scene与缺帧scene在避开HUD/文字的ROI差值为独立Add金样，并与直接draw_sprites一致；UI断线/ROI无信号FAIL。|
|G7 r76_g7_legacy_and_query|非149困魔咒120/123 cast两值与149 Blizzard/Reincarnation/FireWall/PoisonCloud、非149成功雷电对各自固定金样；两次draw相同、events/fields逐字段不变；R72替换控制技能后仍9项绿，其他历史测试SHA不变。|
|G8 r76_g8_asset_plan|调用真实Python analyze_usage.py既有计划入口并解析输出，magic帧集合包含1390..1399且旧集合为子集、Magic2集合不变；冻结旧集合独立expected。删除新增段必须断言红，不以脚本语法错误当红。|

五焦点：错图/来源（G1/G3）、等号/回卷/700与4000分离（G2/G3）、尺寸/offset/裁剪（G4）、混合/缺帧继续（G5）、UI/历史合同/素材漏提取（G6/G7/G8）。
G8固定以Python `-B <副本>/target/round76/a/asset-plan-probe.py <副本>/tools/assets/analyze_usage.py`运行：probe用runpy.run_path（非__main__）加载真实模块，构造RefBook，调用collect_db_refs({"monsters":[],"npcs":[],"items":[],"maps":[]},book,False,[])，输出book.libs中magic/magic2的sorted(indices) JSON；禁改生产常量/函数。Python路径由load_workspace_dependencies实读，完整argv冻结供B/C复跑，不触发main的DB/rustc/地图扫描。

## 精确变异M1–M7

|M（仅隔离物理源码副本、单变异）|指定红|
|---|---|
|M1 仅新分支FxKind::Magic→Swing|G1错库异色；index1390不改。|
|M2 FRAME_MS 100→80|G2 age80/99应1390实际1391；额外采80纳入G2。|
|M3 FRAME_COUNT 10→9|G2 age900应1399实际1390。|
|M4 仅新分支apply_offset true→false|G4当帧位置/屏外移入红。|
|M5 仅新分支draw_effect_blend(...,true)→draw_effect(...)，移除末尾bool以匹配现有签名|G5非黑底Add变Over红。|
|M6 新分支删from_object_spell限制、只留技能条件|G3真实120/123 age700应空却画1397，来源边界红。|
|M7 仅analyze_usage.py删新增range(1390,1400)|G8缺十帧集合断言红；Python正常EXIT0，Rust合同EXIT101。|

- A最终同SHA八测试先放基线物理副本，G1实际像素断言红→最小实现/素材计划→八绿；历史R72迁移只随获批supersede生效。
- B/C每M各自前绿8/0/0→冻结测试/fixture/oracleSHA→单patch→cargo --no-run编译EXIT0→指定G断言红EXIT101→恢复原字节/SHA→八绿。M7额外记录Python正常输出/EXIT。
- 0tests/ignored/编译错误/缺legacy/路径失败不是有效红；每次无效attempt保留，新attempt修正；不改assert/expected让变异通过。
- B/C各自Python从实际WZX/WZL自解RGB565/offset/手算投影与Add，输出G1–G7全采样整帧RGBA/SHA；分别披露case数，Rust expected/actual/Python三源一致，不借用生产helper。

## DAG、任务步骤、交接接口

`门下独立approve → 尚书登记 → A → B → C → Lead提交/唯一门禁 → D → 独立终审 → 尚书验收 → 门下确认`

|段/depends_on|独立可拒绝的交付与步骤|
|---|---|
|A兵部/批准|[ ] 冻结八项测试/fixture/expected；[ ] 基线真红；[ ] 最小生产/素材计划/单控制组迁移；[ ] 八绿与定向回归；[ ] 真实magic十帧提取/可读探针；原字节交Lead，不commit。|
|B工部/A冻结|[ ] 实体副本nlink1、无symlink/junction；独立CARGO_TARGET_DIR/TEMP/TMP/TMPDIR；[ ] 八绿、七变异、独立oracle；[ ] 每M首败expected/actual与恢复SHA、原stdout只增保留。|
|C刑部/B完成|[ ] 另物理副本独立重做八G/七M/oracle；[ ] 核有限supersede、真实帧索引、范围/历史保全；[ ] 报必改与PARTIAL，必改0才交Lead。|
|Lead/C必改0|[ ] 只提交获批白名单；[ ] 唯一全量门禁；[ ] 核门禁后crates/tools无新差异；不拿历史绿替本轮。|
|D礼部/门禁成功|[ ] 四测试文档+roadmap/client追加；[ ] 清单外置SHA免自引用，feature/gate/delivery身份分别写；[ ] 逐blob/bytes/SHA/链接/JSON核查，历史证据不回改。|

A接口为冻结文件原字节+manifest（含完整命令与采样列表）；B产逐G/M/oracle索引供C复核；C交必改0结论与原日志给Lead；D仅消费成功门禁的HEAD/log/meta。顺序有依赖，不并行启动A/B/C/D。

## 未来验收命令、证据与提交门禁

以下命令**本次未运行**；未来新套件8 passed/0 failed/0 ignored，每个G单跑用测试名与--exact验证恰1项。

```text
cargo test -p mir-client --features mir-render/legacy --test round76_trap_hexagon_mapping -- --test-threads=1
cargo test -p mir-client --features mir-render/legacy --test round70_effect_blend_offset --test round71_firewall_mapping --test round72_poison_cloud_mapping --test round73_thunderbolt_hit_mapping --test round74_normal_blend_lock --test round75_spell_lifetime_contract --test round63_projectile_wire
cargo clippy -p mir-client --all-targets -- -D warnings
cargo fmt --all -- --check
pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale
```

- 定向历史数量按实际输出逐套计数，禁止泛称全量通过；R72代码控制技能更换须单列diff与迁移前后SHA，其余历史测试逐SHA核不变。
- 全量仅Lead跑，不SkipTests/SkipPostgres；实际项目数/UTC/EXIT以原日志记录。D纯文档后不重跑，crates/tools再改必须重闸。
- 每条命令记录UTC起止/cwd/HEAD/完整argv/环境/EXIT/测试计数/stdout/stderr/bytes/SHA；证据根target/round76/**，attempt只增。expected冻结原stdout要先于首次actual且两者原件均保存，避免R75-E1/E2重复。
- 真实资源提取缺帧/不可读即阻止“可交付映射”放行；由Lead退中书裁定，不能把合成绿称玩家已看到。真素材probe不代表人工视觉/GPU签核。
- 本次静态检查只核规划UTF-8无BOM/LF、≤220行、相对链接存在、git diff --check EXIT0、crates零diff、看板validate；回执见target/round76/zhongshu，不据此称功能PASS。

## PARTIAL承接、风险、回滚与R77前置

|历史原号|R76可交付子项|仍保留|
|---|---|---|
|GAP-5/R70-C1|仅149 TrapHexagon Magic1390–1399|其余149技能、1620/起手/其他调用点；“其他26技能”历史计数不机械改写，须按明确集合另核。|
|R70-C3|仅新困魔咒分支格原点/offset|全局尺寸/居中/高度、owner DrawLocation、R72 B plan-M4历史残余。|
|GAP-1/2、R70-C2|复用rate1 CPU Add|完整DrawBlend/DrawColour/rate0.8/Mode/INVLIGHT；禁推导src×0.8×alpha。|
|GAP-3/4/6|当帧图像offset一次|mask/tint/shadow消费（shadow只解析）、Light/场景条件offset/AnimationOffset。|
|R71时钟/对象、R73-T1|仅elapsed产品窗映射|NextMotion/Repeat/26完整生产寿命、ActionFeed/owner/严格Effect时钟/声音；注释已修仅该子项不重开。|
|R72 legacy lint、R71-C7/E1、R69条件5|无追溯修复|lint债、C7永久PARTIAL、历史E1仅补救CLOSED、R69条件5永久证据缺口。|
|R68/R49–R70/R61–R67|本轮独立证据|R61 P01–P12、R62①–④、R63 P1–P7、R64①–⑥、R65 G1.2/G2、R66生命周期/全局水位、R67容量/保护/map epoch均不清零；R74 artifacts实数34照承。|
|R75-E1/C-M1|原失败attempt保全要求|B aborted原字节不可恢复，流程PARTIAL；新一轮证据不能补造成旧原件。|
|R75-E2/C-M2|冻结/首actual原stdout留存|A首actual缺失仍声明级PARTIAL；C3只关基线身份冲突。|

风险：专属4000与公共700分窗易错，以G3锁；历史控制组冲突显式supersede；真实素材可能缺1390段，以提取/可读探针挡住；CPU合成不背书真实设备视觉。
回滚由Lead逆撤本轮分支/素材集合增量/控制组更换/新增测试文档，恢复基线SHA；不reset共享树、不删除原证据，不做数据库迁移。
不做：严格对象寿命/26删事件、服务端困魔伤害/控制逻辑、全技能映射、1620、完整rate/Mode、全局居中、mask/tint/shadow/Light、Socket/Live/Audio/Guild/地图/天气/寻路、人工视觉或跨后端验收。
R77前置：R76独立终审/尚书/门下完成且新切片/历史supersede身份固化；若再做寿命，必须先定义对象驱动与R75合同替代范围；若做下一技能，先核真实帧可用性与旧测试控制组冲突；不自动续开执行。

## 待门下裁决项与规划回执

1. 接受“149困魔咒elapsed循环+4000ms精灵产品窗，非149与文字700ms不变”的有限定义；要求严格寿命则封驳重拟。
2. 接受R72 G5控制技能两处枚举TrapHexagon→Reincarnation及两处对应注释的有限supersede；原expected/fixture与历史证据不动；拒绝则本计划不可执行，不静默跳过旧测试。
3. 接受生产白名单含素材分析单行集合增量，并以真实十帧可读为交付门槛；无素材需退中书，不以设备等价代偿。
4. 接受八G/七M、B/C独立物理副本与原日志保全、仅Lead提交和唯一全量门禁；作者不自批、不转Assigned。
5. CLI create→Zhongshu→Menxia与validate的真实UTC/EXIT/任务数存[CLI回执](../../target/round76/zhongshu/cli-receipts.json)；[自检](../../target/round76/zhongshu/plan-checks.json)记录计划行数/bytes/SHA与边界核查。
