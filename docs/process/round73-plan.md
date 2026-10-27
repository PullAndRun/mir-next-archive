# R73 雷电术非149落点命中五帧映射 Implementation Plan

> 中书规划；主任务 `JJC-20261027-010`；基线 `ea9fd791a79ab61403f730e2d79a7a481511ed9d`（HEAD=origin/main）。
> For agentic workers: REQUIRED SUB-SKILL: subagent-driven-development；A→B→C→D逐任务交付，门下独立批准前不得执行。

**Goal:** 单轮接通成功施法120/123来源ThunderBolt静态落点命中：Magic2 10–14、5×80ms elapsed非循环、400ms消隐、格锚点与当帧signed offset/Add。
**Architecture:** 仅在现有spellfx::draw_sprites通用分支前增加来源/技能/cast限定分支；复用Swing的Magic2/start0和R70 Add入口，保留现有事件模型。
**Tech Stack:** Rust、CPU RGBA8、真实120/123 wire编解码、合成WZL/WZX、独立Python二进制oracle；无新依赖。
**Spec:** [路线图§十九/二十](../roadmap.md#L310)、[R72验收](round72-shangshu-acceptance.md)、[R71契约](../testing/round71/round71-firewall-contract.md)、[R70契约](../testing/round70/round70-blend.md)。

## 全局约束、选题与排除

- 本轮优先用户候选①：非149主动技能雷电术；不是重复149火墙/毒云，也不扩成全技能族。
- 单轮可交付依据：已有Magic2/start0、目标格SpellEvent与UI入口；5帧400ms落在现有CAST_MS=700窗内，不需改net/协议/时长API。
- 重要区分：Crystal雷电术起手Magic2 20/3帧300ms与命中Magic2 10/5帧400ms是两阶段，Lightning另有方向性Magic映射；本轮仅命中。
- 次选rate0.8拒绝：DXManager NORMAL=SourceAlpha/One却赋BlendFactor，不能据此证明src×0.8×alpha；AnimationOffset另有格单位风险，暂不选。
- 规划仅写本计划与target/round73/zhongshu/**、经CLI建单流转；不启动执行者、不改crates、不commit/push、不跑cargo/ci。
- 两份未跟踪freeze JSON各>1MB保持原字节且永不入库；所有证据只增新attempt、不覆盖历史。
- 不做：起手阶段、动作队列/施法动画结束才触发、目标owner跟随/TargetID保留、strict NextFrame/SkipFrames、Light6、音频与真实素材/GPU视觉签核。
- 不做：149新技能/持久场/26移除、0.8调光、DrawColour/INVLIGHT、完整DrawBlend位级等价、mask/tint/shadow、全局AnimationOffset/居中与渲染框架重构。
- 下文测试命令均为未来执行契约；规划静态核验不能称功能PASS。回滚由Lead逆向撤本轮分支/测试/文档，不reset共享树、不删证据。

## 事实锚点（现场实读）

|文件:行|事实/决定|
|---|---|
|[PlayerObject.cs:1708](../../github/Crystal/Client/MirObjects/PlayerObject.cs#L1708)|ThunderBolt起手Effect(Magic2,20,3,300,this)，不得拿20–29称命中映射。|
|[PlayerObject.cs:2819](../../github/Crystal/Client/MirObjects/PlayerObject.cs#L2819)|动作帧完成后、Cast为true才执行技能switch，2831用TargetID取ob；本轮收到回执即计时是明确产品偏差。|
|[PlayerObject.cs:2956](../../github/Crystal/Client/MirObjects/PlayerObject.cs#L2956)|ob==null时Effect(Magic2,10,5,400,TargetPoint)，否则同参数挂ob；本轮只做静态落点像素映射。|
|[PlayerObject.cs:3224](../../github/Crystal/Client/MirObjects/PlayerObject.cs#L3224)|Lightning为Magic 970+Direction*20、6帧，不能与ThunderBolt合并迁移。|
|[GameScene.cs:4663](../../github/Crystal/Client/MirScenes/GameScene.cs#L4663)|ObjectMagic把Spell/TargetID/Target/Cast等加入对象ActionFeed；4683不是立即绘制。|
|[Effect.cs:20](../../github/Crystal/Client/MirObjects/Effect.cs#L20)|Light=6、Blend=true、Rate=1F；此Effect不适用SpellObject.Draw的0.8F。|
|[Effect.cs:38](../../github/Crystal/Client/MirObjects/Effect.cs#L38)|NextFrame=Start+(Duration/Count)*(CurrentFrame+1)；81在Time<=NextFrame时return，83处理Owner.SkipFrames。|
|[Effect.cs:113](../../github/Crystal/Client/MirObjects/Effect.cs#L113)|owner用Owner.DrawLocation，否则Source格坐标；130 DrawBlend(BaseIndex+CurrentFrame,...,true,Rate)。|
|[SpellObject.cs:335](../../github/Crystal/Client/MirObjects/SpellObject.cs#L335)|149场对象另用DrawBlend(...,true,0.8F)，不把该rate移植至ThunderBolt。|
|[MLibrary.cs:692](../../github/Crystal/Client/MirGraphics/MLibrary.cs#L692)|699叠当帧signed offset后701裁剪；704–709仅保存恢复bool，不代表完整混合状态等价。|
|[DXManager.cs:353](../../github/Crystal/Client/MirGraphics/DXManager.cs#L353)|374–375 INVLIGHT，378–384 NORMAL SourceAlpha/One及BlendFactor；仅复用现有rate1 CPU Add子集。|
|[server.rs:385](../../crates/mir-protocol/src/manual/server.rs#L385)|Magic INDEX=120，含target_id/target/cast；471 ObjectMagic INDEX=123还含caster location/direction。|
|[net.rs:1541](../../crates/mir-client/src/net.rs#L1541)|SpellEvent含location/param/from_object_spell/started_ms，无target_id，不能实现owner跟随。|
|[net.rs:5573](../../crates/mir-client/src/net.rs#L5573)|120/123均以cast写param、from_object_spell=false；spell_anchor(ThunderBolt)=true（7484–7488），落target而非caster。|
|[spellfx.rs:268](../../crates/mir-client/src/ui/spellfx.rs#L268)|317–334 ThunderBolt/Lightning共走age/60 clamp0..9、Bolt、尺寸居中与Over；目前缺Magic2 10–14命中。|
|[entities.rs:509](../../crates/mir-render/src/entities.rs#L509)|Swing→Magic2/start0，Bolt→Magic2/start20；不能Swing→Bolt或减20掩盖语义。|
|[scene.rs:452](../../crates/mir-render/src/scene.rs#L452)|draw_effect_blend薄转发；[entities.rs:1915](../../crates/mir-render/src/entities.rs#L1915)当帧signed offset各加一次，1923 Add。|
|[ui/mod.rs:4199](../../crates/mir-client/src/ui/mod.rs#L4199)|公开ui::draw先sync，4280–4285将snapshot.spell_events及UiState.time_millis送入draw_sprites。|
|[R71测试:718](../../crates/mir-client/tests/round71_firewall_mapping.rs#L718)、[R72测试:839](../../crates/mir-client/tests/round72_poison_cloud_mapping.rs#L839)|历史G5的手造ThunderBolt均param=false，保持旧金样，不需改历史测试来迁就新分支。|

## 文件白名单与精确接口

|属主|允许写文件|责任|
|---|---|---|
|A兵部|crates/mir-client/src/ui/spellfx.rs|唯一生产改动；新增限定分支/常量与准确注释。|
|A兵部|crates/mir-client/tests/round73_thunderbolt_mapping.rs|新建9项命名集成测试、合成夹具、独立Rust期望计算；不改R70–R72测试。|
|A/B/C/D各自|target/round73/{a,b,c,d}/attempt-N/**|独立脚本、物理源码副本、夹具、oracle、原日志、hash与清单。|
|C刑部|docs/testing/round73/round73-c-evidence.md；必要新增round73-c-addendum.md|独立核验、BLOCK与追加复核，历史报告不回写。|
|D礼部|docs/testing/round73/README.md、round73-thunderbolt-contract.md、final-manifest.json、manifest.sha256；docs/roadmap.md、docs/client.md相关R73小节|事实、原号PARTIAL、阶段/HEAD、清单及证据链接；不改历史报告。|
|Lead/协调|docs/process/round73-*.md；target/round73/{zhongshu,menxia,lead,gate,shangshu}/**|规划/审核/派发/裁定/验收、唯一门禁与机械刷新清单。|

- 保持 `pub fn draw_sprites(frame: &mut Frame, events: &[SpellEvent], scene: &mut MapScene, center: Point, now_ms: i64)` 签名。
- A定义 `THUNDER_BOLT_HIT_BASE_INDEX: u32 = 10`、`THUNDER_BOLT_HIT_FRAME_COUNT: i64 = 5`、`THUNDER_BOLT_HIT_FRAME_MS: i64 = 80`、`THUNDER_BOLT_HIT_DURATION_MS: i64 = 400`。
- 准入仅 `event.spell == Spell::ThunderBolt && !event.from_object_spell && event.param`；放在既有age过滤后、通用frame_index前，不改R71/R72分支。
- age仍用saturating_sub；仅0≤age<400绘制：phase=age/80，index=10+phase；age≥400必须continue，禁止落回通用闪光（含400–699ms）。
- `(x,y)=scene.world_to_screen(center,event.location)`，`scene.draw_effect_blend(frame,FxKind::Swing,index,x,y,true)`，随后continue；不取effect_size、不居中、不预加offset。
- 一事件一张当前帧、非循环；缺Magic2/目标帧仅跳当前事件，不退0/20/邻帧、不吞后续事件；既有色块/文字仍按700ms窗，不称全UI400ms消失。
- 149 ThunderBolt、param=false、Lightning及所有其他技能完全保持旧分支；param=false旧闪光保留是范围边界，不称已解决失败施法反馈。

## Review Focus

- 起手20与命中10混淆、错误图库或Bolt额外+20：G1三类异色诱饵与整帧独立期望。
- 恰等于80/400ms与大步跳时：G2钉elapsed契约并披露strict NextFrame差异。
- 尺寸变化、正负offset、屏外移入及重复draw：G3证明格锚点/一次offset/先偏移后裁剪。
- 非黑底Add、饱和/透明像素与重叠两事件：G4独立整数公式，不以黑底误判Over。
- Cast=false/149/Lightning误迁、缺帧吞后续事件、UI入口断线：G1 UI与G5真实wire矩阵。

## G1–G7 可失败验收（新套件恰9项命名测试）

|G|命名测试/断言|明确失败条件|
|---|---|---|
|G1|`r73_g1_wire_mapping`：120/123真实encode→decode_server_packet→apply_packet_at→WorldSnapshot，assert INDEX/cast/非149/target/started_ms，再UiState.sync→draw_sprites；target与caster故意相隔≥2格；Magic10/Magic2 20/30为异色诱饵。|必须Magic2 10整帧RGBA；caster/20/30任何误图即FAIL，M1。|
|G1|`r73_g1_ui_draw`：同真实wire快照通过公开ui::draw（UiText::fallback、UiState受控毫秒），两份独立scene分别含/缺目标帧；比较安全地图ROI（离HUD/文字/色块边线），独立期望=无该帧基线+当前帧Add；同ROI须与draw_sprites输出吻合。|不能只直接调用helper充UI证据；UI移除draw_sprites应失败；无差异、遮挡或字体污染均FAIL，调整夹具后重跑。|
|G2|`r73_g2_frame_boundaries`：k=0..4逐80k与80k+79→10+k，80/160/240/320分别11/12/13/14。|60ms、十帧或错误回卷即FAIL，M2。|
|G2|`r73_g2_age_window`：-1、0、399、400、401、699、700与i64两端；400–699只断精灵无图，允许既有色块层；极值不panic。|末帧后重画通用/Bolt或循环即FAIL，M3。|
|G3|`r73_g3_anchor_offsets`：10/11尺寸3×3与5×2，offset(-3,5)/(4,-2)，受控center=(5,5)对应格原点(384,256)，其他格每步48/32；重复绘制清帧与换center。|手算坐标/整帧不符、居中/双offset/累积即FAIL，M4。|
|G3|`r73_g3_offscreen_crop`：格原点屏外、signed offset移入，负坐标部分裁剪与完全屏外，先assert目标帧可读。|不读元数据/先裁剪导致失图或越界即FAIL，M4。|
|G4|`r73_g4_add_per_cell`：非黑底、饱和、RGB565透明黑与两事件同格叠加；RGB=min(255,dst+(src*a+127)/255)，a=0保留dst否则目标alpha255。|整帧oracle不符即FAIL；独立Python复算，不宣称连续半透明素材已覆盖。|
|G5|`r73_g5_source_cast_legacy`：真实120/123 Cast两值×149 Param两值×ThunderBolt/Lightning；仅非149+true雷电迁移，其余对旧通用独立金样；R71火墙/R72毒云正例仍画。|149/false误迁或Lightning受影响即FAIL，M5。|
|G5|`r73_g5_missing_frame`：缺11但10/12/20存在、空Magic2；后续149 FireWall仍绘制，缺帧不得fallback/中止列表。|缺帧有误图、panic或后续消失即FAIL。|
|G6|B/C各自独立物理副本运行九项+五变异；独立Python用struct/zlib读实际WZX/WZL bytes、RGB565扩展、手工坐标/裁剪/Add，产PNG/RGBA/SHA；Rust expected、actual、Python三源相等。|调用生产phase/坐标/混合helper当oracle、仅复制Rust计算、缺原件/SHA不同/0tests/ignored/SKIP均FAIL。|
|G7|C必改0→Lead功能提交HEAD唯一全量→D据实记录→尚书/指定门下确认；范围/diff、freeze保全、清单tracked blob/bytes/SHA逐项核。|全量非0/缺步骤/门禁后crates或工具改动未重闸、假称全技能或视觉等价均不放行。|

## 五变异 M1–M5（B/C各独立重做，不得语义替换）

|M|仅隔离生产副本注入|指定必须红的断言|
|---|---|---|
|M1|仅雷电命中FxKind::Swing→Bolt（index保持10+phase）|G1实际Magic2 30诱饵，expected10；检测start20，不得以别的错库替代。|
|M2|仅THUNDER_BOLT_HIT_FRAME_MS 80→60|G2 age60/79应10实际11，钉80ms。|
|M3|仅移除本分支age≥400的continue，让其执行10+age/80|G2 age400必须无图，夹具强制存在Magic2 15诱饵，确认不是缺帧伪绿。|
|M4|仅雷电命中draw_effect_blend末bool true→false|G3当帧offset/屏外移入断言红。|
|M5|准入去掉`!event.from_object_spell && event.param`，仅留技能条件|G5真实149与Cast=false旧像素金样红。|

- 每项独立：同最终测试/夹具/oracleSHA的前绿9/0/0→单变异→编译成功→运行九项且指定断言红/EXIT101→恢复生产原字节/SHA→后绿9/0/0。
- 0tests、编译错、缺legacy、环境/权限/D8050等只能无效attempt，不能冒充功能红；保留原日志后新attempt纠正。
- A先将最终同SHA九项测试置基线物理副本取得映射/像素断言真红，再最小生产改动转绿；baseline可通过测试不要求每项都红。
- 测试变更后必须重冻SHA并重做所受影响全矩阵；B缺口退A，B/C不得偷偷修改交付测试或共用构建缓存。

## 四段 DAG、操作顺序与交付

门下独立approve→尚书CLI登记四子任务/depends_on与agent映射→A兵部(deepseek)→B工部(deepseek)→C刑部(gpt)→Lead提交+唯一门禁→D礼部(gpt)→尚书验收→指定门下确认。

|段/依赖|可审交付与步骤|
|---|---|
|A/门下通过|仅两白名单代码文件+a证据；先写九项/冻结SHA、baseline断言红→最小分支→九项绿→定向回归；给Lead diff/bytes/SHA，不commit。|
|B/A冻结交付|只写b物理副本/证据；从HEAD+交付原字节重建、独立Python oracle、五变异前绿/红/恢复绿；附逐M首败expected/actual与恢复SHA。|
|C/B完成|只写c副本/报告；独立九项、五变异、oracle与回归，核B每条与获批矩阵一一对应；可BLOCK，所有必改复核0才放行。|
|D/Lead门禁成功|只写文档白名单+d证据；记录功能HEAD/门禁HEAD/后置文档HEAD与真实结论，历史BLOCK及PARTIAL逐号保留，清单不自引用、外置SHA。|

- [ ] A写测试→基线真红→实现→定向绿→文件原字节交Lead。
- [ ] B独立物理隔离→九项绿→五变异→二进制oracle→回执。
- [ ] C独立重做→逐G/逐M核对→必改/裁量清单→必改0。
- [ ] Lead审核白名单→唯一功能提交→唯一全量；D随后据实收口；仅Lead可提交/推送。

## 未来命令、证据与门禁

```text
cargo test -p mir-client --features mir-render/legacy --test round73_thunderbolt_mapping -- --test-threads=1
cargo test -p mir-client --features mir-render/legacy --test round72_poison_cloud_mapping --test round71_firewall_mapping --test round70_effect_blend_offset --test round63_projectile_wire
cargo test -p mir-client --lib ui::spellfx
cargo clippy -p mir-client --all-targets -- -D warnings
cargo fmt --all -- --check
```

- 九项新测试、定向29项历史测试均须实际运行、0 failed/0 ignored；历史G5 param=false不supersede、不改SHA。新增ignore/allow=0。
- A/B/C各自新CARGO_TARGET_DIR与TEMP/TMP/TMPDIR，绝对路径记录；可统一CARGO_PROFILE_DEV_DEBUG=0但须完整披露；B/C物理副本禁hardlink/junction/symlink、记录nlink=1。
- 证据根target/round73/**；每命令保存UTC起止/cwd/完整命令/HEAD/环境/EXIT/计数/stdout/stderr/文件bytes/SHA；变异diff、基线/注入/恢复原字节及每次日志均留存。
- 唯一全量仅Lead：`pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`；不SkipTests/SkipPostgres，必须EXIT0且实际12项全OK；不拿R72旧绿代替。
- 门禁原日志/meta固定门禁HEAD/operator=lead/UTC/EXIT/实际步骤/bytes/SHA；失败新attempt保留。D纯文档后不重跑全量；crates/tools若再变必须Lead新闸。
- 官方clippy不带legacy；带legacy两处既有needless_return债按R72 Lead裁定披露，不擅自加白名单顺手修。
- D清单分别标feature_head/gate.head/delivery_head；tracked逐blob复算，不硬写将失效的文档HEAD；本轮不得重犯R72 T-DOC-1时态与清单锚点不一致。

## PARTIAL 台账（R72原号承接，不整项清零）

|原号|本轮可证子项|仍PARTIAL|
|---|---|---|
|GAP-5 / R70-C1|非149成功ThunderBolt静态落点10–14映射|起手20/3帧300ms、Lightning/其余技能、1620/其他调用点、Cast=false旧闪光；不把“其他26技能”计数机械减一（原计数属149域）。|
|R70-C3|仅本分支格原点、不按尺寸居中|全局高度/尺寸/居中与目标owner DrawLocation不同来源。|
|R71时钟/对象偏差|新雷电elapsed 80ms/400ms产品窗|R71/R72 strict NextMotion、初始100ms、每Process一步、149持久场/26移除/寿命、4000ms产品窗均原号保留。|
|R73-T1新增|五帧静态命中切片|Effect.Time>NextFrame严格等号/跳帧、ActionFeed/施法动画结束触发、目标跟随、收到回执起算、Light6/音频；不是完整ThunderBolt等价。|
|GAP-1/2 / R70-C2|复用rate1 Add；雷电Effect本来Rate1|R71/R72 rate0.8/src×0.8×alpha未实施；DrawColour/INVLIGHT/完整DrawBlend/API收敛不清零。|
|GAP-3/4/6|只消费现有图像offset|Light3、mask/tint/shadow、场景offset/AnimationOffset及雷电Light6。|
|R72 B plan-M4残余 / legacy lint|无追溯改写|历史B近似居中残余与带legacy lint既有债继续披露。|
|R71-C7 / E1 / R69条件5|E1补救维持CLOSED|C7及a77原红/隔离/日志不可恢复为永久PARTIAL，新证据不补造成历史完整。|
|R68 / R49–R70 / R61–R67|本轮独立合成证据|真实素材/GPU/设备视觉、全UI/跨后端/Live音频及所有更早原编号不清零。|

## 门下需裁量与规划回执

1. 接受非149成功施法静态落点+回执elapsed计时这一单轮切片；若要求完整owner跟随/ActionFeed/strict Effect时钟则封驳重拟，不在执行中扩项。
2. 接受复用名为Swing的Magic2/start0寻址入口和旧Cast=false闪光保留；原R71/R72测试param=false不改变，门下应核来源门禁与五变异的可证性。
3. rate1据Effect默认值选择；既有149 rate0.8仍PARTIAL，禁止类推“调光已补全”。R72 roadmap旧pre-audit措辞是历史文档残余，以T2/尚书/最终看板为准。
4. CLI实读cmdCreate的title=args[0]，用户create JJC标题形式需附--title防误存JJC；本次真实执行保留该形式并加--title，成功建单后Zhongshu→Menxia。
5. 本页完成后计划骨架/文件元数据与validate回执存target/round73/zhongshu/**；实测行数/bytes/SHA/任务数由最终回报给Lead，本规划者不自批、不转Assigned、不启动A/B/C/D。
