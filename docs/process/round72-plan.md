# R72 149毒云逐格精灵映射 Implementation Plan

> 中书拟案；主单 `JJC-20261027-009`。只读规划交付，不启动执行者；门下独立批准前不执行。
> 执行团队按 subagent-driven-development 逐任务交付；勾选仅表示取得相应实录证据。

**Goal:** 单轮交付仅149来源 PoisonCloud 的 Magic2 1650–1669、20×120ms elapsed循环、格锚点与当帧offset接线。
**Architecture:** 在既有draw_sprites增加来源+技能限定分支，复用R70 Add入口及现有Magic2零起点FxKind::Swing；不新增渲染框架。
**Tech Stack:** Rust、CPU RGBA8、真实149编码/解码链、合成WZL/WZX、独立Python oracle；无新依赖。
**Spec:** [roadmap §十九](../roadmap.md#L310)、[R71契约](../testing/round71/round71-firewall-contract.md)、[R71索引](../testing/round71/README.md)、[client §5.36](../client.md#L1427)。

## 全局约束与选题

- 输入基线：R71主任务JJC-20261027-008 Done、validate 231；现场HEAD=c3cf8b5。R71 E1补救CLOSED、C7永久PARTIAL。
- R71现存三份结果文档仍写pre-audit/待终审，属于本地历史阶段快照，不能据此倒退已收口状态，也不能改写其原件。
- 现场roadmap §十九310–323止于PARTIAL台账，未见字面“R72候选表”；本案依据其GAP-5/R70-C1及用户候选①拟定，差异交末节裁量。
- 只选一个技能：PoisonCloud；20帧循环能在既有4000ms窗口内验证完整周期及回卷，不涉及协议、对象生命周期或新图库API。
- 否决策略：若需新增协议/跨轮对象同步、素材设备视觉签核或扩大渲染架构，封驳重拟；不得执行中扩项。
- 否决整体LightBlend/rate×alpha近似：DXManager的NORMAL因子及状态语义不能由CPU src×0.8×alpha证明等价；该差异继续PARTIAL。
- 规划者只写本页与CLI看板；不改crates、不提交/push、不跑cargo/ci.ps1。两份既有未跟踪历史freeze不触碰。
- 下列生产改动、测试命令与证据目录均为后续执行契约，本次没有执行这些命令。

## 实读事实锚点与映射缺口盘点

|文件:行（均现场实读）|结论/选择依据|
|---|---|
|[SpellObject.cs:30](../../github/Crystal/Client/MirObjects/SpellObject.cs#L30)|Load取Location、Spell、Direction；37默认Repeat=true。|
|[SpellObject.cs:42](../../github/Crystal/Client/MirObjects/SpellObject.cs#L42)|TrapHexagon=Magic1390/10×100ms/Blend；Rust未特判，现有700ms窗不能覆盖全部帧与回卷，本轮不选。|
|[SpellObject.cs:49](../../github/Crystal/Client/MirObjects/SpellObject.cs#L49)|FireWall=Magic1630/6×120ms/Light3/Blend；R71已交付，不重复立项。|
|[SpellObject.cs:57](../../github/Crystal/Client/MirObjects/SpellObject.cs#L57)|PoisonCloud=Magic2、1650、120ms、20帧、Light3、Blend=true；继承Repeat=true，AnimationOffset默认0。|
|[SpellObject.cs:71](../../github/Crystal/Client/MirObjects/SpellObject.cs#L71)|Blizzard=Magic2 1550/30×100ms、AnimationOffset(0,-20)、Repeat=false；多出偏移/终止规则，本轮不选。|
|[SpellObject.cs:81](../../github/Crystal/Client/MirObjects/SpellObject.cs#L81)|MeteorStrike叠加1600附属Effect及1610主体/非循环，本轮不选。|
|[SpellObject.cs:103](../../github/Crystal/Client/MirObjects/SpellObject.cs#L103)|Reincarnation=Magic2 1680/10×100ms；112的ExplosiveTrap按Param分支且需Magic3；132 Trap与225 Portal=Magic2 2360/8×100ms，232 HealingCircle需Magic3，均仍缺独立映射。|
|[SpellObject.cs:288](../../github/Crystal/Client/MirObjects/SpellObject.cs#L288)|初始NextMotion向100ms取整；294–302每Process至多一步、NextMotion=CMain.Time+FrameInterval。本案elapsed允许跳帧，非严格等价。|
|[SpellObject.cs:323](../../github/Crystal/Client/MirObjects/SpellObject.cs#L323)|格坐标生成DrawLocation并加全局/运动offset；335–339调用DrawBlend(DrawFrame+FrameIndex,...,DrawColour,true,0.8F)。|
|[GameScene.cs:5412](../../github/Crystal/Client/MirScenes/GameScene.cs#L5412)|ObjectSpell包创建SpellObject再Load；映射不在施法目标选择表中。|
|[GameScene.cs:11925](../../github/Crystal/Client/MirScenes/GameScene.cs#L11925)|11937–11942为PoisonCloud目标选择；11981–11985生成方向/落点/targetID，不是Magic2帧号证据。|
|[MLibrary.cs:692](../../github/Crystal/Client/MirGraphics/MLibrary.cs#L692)|699先加当帧mi.X/Y、701再裁剪；704–709保存/恢复bool，不能当完整rate状态恢复。|
|[DXManager.cs:353](../../github/Crystal/Client/MirGraphics/DXManager.cs#L353)|374–375 INVLIGHT=BlendFactor/InverseSourceColor；378–384 NORMAL=SourceAlpha/One并赋BlendFactor，不证明0.8乘色语义。|
|[spellfx.rs:40](../../crates/mir-client/src/ui/spellfx.rs#L40)|PoisonCloud时长AREA_MS=4000；272–278仅FireWall特判，其余age/60 clamp0..9，毒云落Magic前10帧，缺Magic2 1650映射。|
|[entities.rs:497](../../crates/mir-render/src/entities.rs#L497)|509–522 Swing→Magic2/start0；Bolt→Magic2/start20；Magic→Magic/start0。1904最终帧=index+start_frame。|
|[scene.rs:452](../../crates/mir-render/src/scene.rs#L452)|现有draw_effect_blend转发；[entities.rs:1915](../../crates/mir-render/src/entities.rs#L1915)当帧signed offset各saturating_add一次，1923 Add。|
|[net.rs:1541](../../crates/mir-client/src/net.rs#L1541)|SpellEvent保留param/from_object_spell/started_ms；5624–5647真实149同时写瞬态事件与持久spell_fields，本案仅消费前者。|

## 范围、接口与不做

- 后续唯一生产写文件：[spellfx.rs](../../crates/mir-client/src/ui/spellfx.rs#L252)；新增测试 `crates/mir-client/tests/round72_poisoncloud_mapping.rs`。
- 新常量：`POISON_CLOUD_BASE_INDEX: u32 = 1650`、`POISON_CLOUD_FRAME_COUNT: i64 = 20`、`POISON_CLOUD_FRAME_MS: i64 = 120`。
- 保持 `pub fn draw_sprites(frame: &mut Frame, events: &[SpellEvent], scene: &mut MapScene, center: Point, now_ms: i64)` 签名。
- 在已有age过滤和FireWall分支后、通用frame_index前，严格准入 `event.spell == Spell::PoisonCloud && event.from_object_spell`。
- `age=now_ms.saturating_sub(event.started_ms)`；仅0≤age<4000；`phase=(age/120)%20`；精确图库帧1650+phase。
- `(x,y)=scene.world_to_screen(center,event.location)`；调用 `scene.draw_effect_blend(frame, FxKind::Swing, 1650+phase, x, y, true)` 后continue。
- Swing在此仅作为现有Magic2/start0寻址入口，须写明解释；不得用Bolt导致+20，也不得减20绕过或改FxKind语义。
- 每149事件只画一张当前帧，不复制格子；不effect_size居中、不预加offset、不重复通用精灵。
- param=true/false、八方向不改变映射；缺目标帧/图库跳当前事件，不退0/相邻帧、不吞后续事件；原色块/文字保留。
- 非149毒云保持旧Magic/age60/clamp/居中/Over；R71火墙、其余ObjectSpell全部保持原行为。
- 不改net/wire/server、spell_fields与26移除、duration_ms、Frame/MapScene/EntityRenderer、公开API/图库枚举。
- 不做严格NextMotion、Light3、DrawColour、0.8/rate近似、INVLIGHT、mask/tint/shadow、真实素材/GPU/设备视觉签核。

## Review Focus

- Magic与Magic2、Bolt额外20帧混淆：G1放三类诱饵并断言整帧。
- 20帧回卷与末端/未来事件：G2覆盖边界及极值，窗口等号明确无图。
- 不同尺寸/正负offset和屏外原点：G3独立坐标oracle，证明先加offset再裁剪。
- 缺帧吞后续事件或双绘制：G4/G5检查背景与事件顺序。
- 误迁非149或FireWall：G5及R71回归；夹具不可读/零测试只能FAIL。

## G1–G7 可失败验收（9个命名测试）

|G|命名测试及独立断言|失败信号|
|---|---|---|
|G1|`r72_g1_wire_mapping`：真实encode→decode149→apply_packet_at→UiState/snapshot→公开draw_sprites；先assert来源/位置/目标帧可读。Magic1650、Magic2 0/20/1670放异色诱饵；两条不同ID事件只落各自格。|目标应为Magic2 1650；整帧RGBA与oracle不同即FAIL；M1/M2。|
|G2|`r72_g2_frame_boundaries`：循环遍历k=0..19的120k与120k+119→1650+k；另2399→1669、2400→1650、2520→1651、3999→1663。`r72_g2_age_window`：-1/4000、i64极值无图且安全。|不得冻结、用60ms或丢尾帧；M3。|
|G3|`r72_g3_anchor_offsets`：1650/51不同尺寸，offset=(-3,5)/(4,-2)，重复draw/换center分别清帧。`r72_g3_offscreen_crop`：屏外格原点经offset移入、部分负坐标裁剪。|独立整帧oracle；居中/双加位置不得有误图；M4/M5。|
|G4|`r72_g4_add_per_cell`：非黑底、饱和、两事件同格叠加及旁邻空白；复核每RGB `min(255,dst+(src*a+127)/255)`，目标alpha沿用既有Frame规则。|M6仅改带offset的Over，隔离混合因子；不用旧无offset入口混淆。|
|G5|`r72_g5_legacy_sources`：真实125/570毒云、149 FireWall/Blizzard/TrapHexagon、雷电旧整帧金样；`r72_g5_missing_frame`：缺1651/空Magic2且后续FireWall仍画；`r72_g5_param_direction`：两param×八方向等价。|M7去来源限制必须非149断言红；R71八测试保持绿，其他技能不迁。|
|G6|B/C独立物理副本、同最终测试SHA、9项全部运行；独立Python从声明的夹具像素/锚点/索引计算RGBA与SHA，与Rust expected和生产actual三源比对。|0tests/ignored/skip、SHA错、仅比较生产与自抄公式、漏日志均FAIL。|
|G7|Lead最终功能提交HEAD唯一全量及范围检查；D只记录实录并披露PARTIAL。|EXIT非0/缺步骤/门禁后crates或工具变化/假称视觉等价均不放行。|

## M1–M7 变异矩阵

|M|仅在隔离生产副本改动|必须命中的红控|
|---|---|---|
|M1|毒云FxKind::Swing→Magic|G1图库异色。|
|M2|毒云FxKind::Swing→Bolt|G1实际1670诱饵，检测start20。|
|M3|毒云phase `%20`→clamp0..19|G2 age2400回卷。|
|M4|仅毒云恢复按帧宽高居中|G3非对称尺寸/坐标。|
|M5|仅毒云apply_offset true→false|G3屏外移入/当帧offset。|
|M6|仅毒云draw_effect_blend→draw_effect_with_offset（保留offset，移除末bool）|G4非黑底Add/Over数值。|
|M7|毒云准入去掉from_object_spell|G5非149旧行为。|

- 每项独立：前绿9/0→注入→编译成功→运行9例且指定assert红/EXIT非0→恢复原字节→后绿9/0。
- B/C各自重做七项；编译错、feature缺失、D8050、导入错/PermissionDenied都不算有效红，保留原件后新attempt。
- 测试/夹具/oracle SHA在前绿、红、后绿与C复验保持不变；必要新增测试则全矩阵重新冻结，不减少九项。

## DAG 与实施步骤

门下独立审核 → 尚书先登记子单/agent映射 → A兵部 → B工部 → C刑部 → Lead提交+唯一门禁 → D礼部 → 尚书验收 → 指定门下确认。

|任务/依赖|写入范围与可审交付|
|---|---|
|A兵部/门下approve|仅生产单文件+新测试、`target/round72/a/attempt-N/**`；九项测试、独立oracle、功能红绿及定向回归。|
|B工部/A|仅`target/round72/b/attempt-N/**`；独立源码物理副本、七变异、Python oracle；测试缺口退A。|
|C刑部/B|自有副本及`target/round72/c/attempt-N/**`、`docs/testing/round72/round72-c-evidence.md`；独立九项/七变异/SHA审核，可BLOCK。|
|Lead/C必改0|审白名单、提交功能、独自运行全量，原日志/meta写`target/round72/gate/attempt-N/**`。|
|D礼部/Lead门禁通过|仅`docs/testing/round72/**`、roadmap/client相关小节与`target/round72/d/attempt-N/**`；索引/manifest/外置sha，静态核验，不跑Cargo。|

- [ ] A先写G1–G5九项测试，运行T72取得可读夹具条件下映射/像素断言红；固定最终测试SHA。
- [ ] A实现限定分支，T72绿及R回归绿，提交diff与文件SHA给Lead，不自行commit。
- [ ] B运行七变异与独立oracle，逐项记录首败expected/actual及恢复SHA。
- [ ] C独立复现B流程，逐G结论及实现/测试/证据必改清单均可失败；必改0才交Lead。
- [ ] Lead审阅/提交并唯一门禁；D随后写实录、保全静态证据，尚书与指定门下依真实看板收口。

## 执行命令、证据与门禁边界

T72（未来A/B/C执行，不是本次规划已跑）：
```text
cargo test -p mir-client --features mir-render/legacy --test round72_poisoncloud_mapping -- --test-threads=1
```
R（未来逐条保存EXIT）：
```text
cargo test -p mir-client --features mir-render/legacy --test round71_firewall_mapping -- --test-threads=1
cargo test -p mir-client --features mir-render/legacy --test round70_effect_blend_offset
cargo test -p mir-client --lib ui::spellfx
cargo clippy -p mir-client --all-targets -- -D warnings
cargo fmt --check
```

- 证据根`target/round72/**`；每角色attempt只增不改。每命令保存UTC/cwd/完整命令/HEAD/环境/EXIT/统计/stdout/stderr、逐文件bytes/SHA256。
- B/C用独立真实物理副本，禁hardlink/junction/symlink共享源码，记录nlink=1；隔离CARGO_TARGET_DIR及TEMP/TMP/TMPDIR。
- 必要CARGO_PROFILE_DEV_DEBUG=0全对照一致披露；legacy feature对合成WZL必需，缺feature不能充当功能红。
- 每变异保全diff、编译日志、前绿/断言红/后绿原日志、原字节/恢复SHA；不可只留摘要或丢中间SHA对应原件。
- manifest不自引用；外置摘要；Python oracle不调用生产phase/坐标/混合helper。WZL无连续alpha则用可表达样本，不伪称真实半透明全矩阵。
- 唯一全量仅Lead：`pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`，不得SkipTests/SkipPostgres或复用R71旧绿。
- 全量须EXIT0且脚本实际步骤全部OK（基线12/12）；原log/meta记录门禁HEAD与bytes/SHA。失败保留后新attempt，不覆盖。
- D只做文档时不追加全量；若门禁后crates/工具变化，由Lead重审并新attempt；门禁HEAD与后置文档HEAD分别披露。
- 回滚由Lead逆向撤本轮毒云分支/常量及新测试与新文档链接，不reset共享树、不删证据；无协议/数据库迁移。

## PARTIAL 台账（原号承接）

|原号|本轮可闭合子项|继续PARTIAL|
|---|---|---|
|GAP-5 / R70-C1|仅149 PoisonCloud图库/20帧/格锚点/offset接线|非149毒云、其他ObjectSpell/起手/UI调用点、真实素材视觉。|
|R70-C3|仅目标毒云去尺寸居中|全局尺寸/高度/居中、其他技能。|
|GAP-1/2 / R70-C2|仅复用rate1 Add|rate0.8及src×0.8×alpha近似未实施；DrawColour/INVLIGHT/完整DrawBlend/公共API。|
|GAP-3/4/6|无|mask/tint/shadow（shadow只解析不消费）、GameScene条件offset、Light3。|
|R71时钟/对象偏差|新增毒云elapsed有限映射|严格NextMotion/初始100ms取整/每Process一步、149持久场/26移除；4000ms仅瞬态窗。|
|R71-C7 / E1|E1补救维持CLOSED|C7中间日志永久PARTIAL，不以R72新证据追补成历史完整。|
|R68 / R69条件5 / R49–R70 / R61–R67|新增独立证据|a77历史永久缺口、历史原号、全UI/设备/跨后端、Live音频等均不清零。|

## 规划静态核验与CLI披露

- create返回JJC-20261027-009、默认Taizi；显式state Taizi遭Taizi→Taizi非法转换EXIT3，show仍Taizi，保留失败事实后合法转Zhongshu。
- create --help实际EXIT1“缺少标题”，只读list证实无意外建单；采用用户指定create --title成功，不改看板原始JSON。
- 本页补全后转Menxia并validate；任务数与计划行数/字节/SHA、UTF-8/LF及链接/diff检查结果由规划者实测回报，不能将其称为功能测试通过。

## 待裁量（门下/Lead）

1. 认可缺失字面R72候选表的输入差异，接受依据GAP-5/R70-C1选择149毒云这一单一切片。
2. 接受复用名称为Swing的现有Magic2/start0寻址入口；若必须新增语义枚举或改API，应先重拟范围，不能暗中扩项。
3. 认可elapsed/4000ms/rate1偏差与真实资源视觉仅PARTIAL；若要求Crystal整体等价，封驳本案。
4. 审九项测试、七变异、独立oracle、原件保全与Lead唯一门禁；本作者不自批、不代门下转Assigned。
