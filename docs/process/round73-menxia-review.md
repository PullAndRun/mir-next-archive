# R73 门下独立审议

- verdict：**PASS-WITH-CHANGES（附条件准奏 / approve）**；主单 `JJC-20261027-010`。仅放行下列有限切片，功能验证待A/B/C执行。
- 审议HEAD/origin/main：`bd71bd049f516c3d7984a77c241d2630cad291d3`；计划159行/18697B/SHA256 `7b9be54c4ae3d953123337a77e2ae21e173e77db6bc98145474b675ce597596e`。

|必核项|实读结论|
|---|---|
|Crystal命中/起手|[PlayerObject.cs:2956](../../github/Crystal/Client/MirObjects/PlayerObject.cs#L2956) 为Magic2/10/5/400；ob==null用TargetPoint，否则挂ob；[1708](../../github/Crystal/Client/MirObjects/PlayerObject.cs#L1708) 为起手20/3/300，本轮不迁。|
|调度/锚点|[PlayerObject.cs:2819](../../github/Crystal/Client/MirObjects/PlayerObject.cs#L2819) 动作完成且Cast后取TargetID；[Effect.cs:113](../../github/Crystal/Client/MirObjects/Effect.cs#L113) owner用DrawLocation、无owner用Source格坐标；本轮回执起算与静态target均为显式偏差。|
|offset/Add|[MLibrary.cs:692](../../github/Crystal/Client/MirGraphics/MLibrary.cs#L692) 699先offset、701裁剪；[DXManager.cs:353](../../github/Crystal/Client/MirGraphics/DXManager.cs#L353) NORMAL=SourceAlpha/One，BlendFactor赋值不能证明完整rate等价；[Effect.cs:24](../../github/Crystal/Client/MirObjects/Effect.cs#L24) 默认Rate=1F。|
|Rust安全落点|[spellfx.rs:317](../../crates/mir-client/src/ui/spellfx.rs#L317) 原ThunderBolt/Lightning共用Bolt20–29/60ms/clamp/居中/Over；在其前加限定分支安全；Swing→Magic2/start0、当帧offset一次/Add可复用。|
|wire/UI|[net.rs:5573](../../crates/mir-client/src/net.rs#L5573) 120/123以cast→param、from_object_spell=false，ThunderBolt落target；[ui/mod.rs:4280](../../crates/mir-client/src/ui/mod.rs#L4280) 消费快照与毫秒；不需改协议/事件结构。|
|可失败验收|G1–G5合计9项具真实wire/UI、帧时钟、坐标/offset/Add、Lightning守卫、缺帧/屏外断言；G6三源oracle与五变异、G7门禁可执行；均尚未实跑。|
|范围/静态|仅生产spellfx与新测试；不动magic映射/队列/协议/其他技能。25个计划本地链接均存在，UTF-8/LF，git diff --check EXIT0；两freeze字节/SHA已留证。|

- C1：400是Crystal构造Duration事实；80ms elapsed与恰400ms停止仅为产品精灵窗，700ms文字/色块窗不变，不能称全UI消隐。
- C2：[Effect.cs:81](../../github/Crystal/Client/MirObjects/Effect.cs#L81) 为Time<=NextFrame即return；严格等号、每Process至多一次普通推进及Owner.SkipFrames额外推进均未迁；80/400边界和大步跳时须标elapsed偏差。
- C3：目标不存在必须解释为TargetPoint，不能误取caster；目标存在的owner跟随仍PARTIAL；不新增TargetID/协议/动作队列实现。
- C4：rate1 CPU Add仅有限近似，非GPU/完整DrawBlend等价；非黑底、饱和、透明黑与两事件叠加需独立整帧断言，不宣称连续半透明素材已覆盖。
- C5：A/B/C物理副本禁hardlink/junction/symlink，nlink=1且独立target/tmp；同最终测试/夹具SHA，基线成功编译后映射/像素断言红；禁0tests/ignored/SKIP/编译错/环境错充红。
- C6：B/C各实跑原样M1–M5，逐项前绿9/0/0→指定断言红/EXIT101→原字节/SHA恢复→后绿；Python直接解实际WZX/WZL，actual/Rust expected/Python RGBA及SHA三源一致，不能调用生产helper。
- C7：先CLI登记DAG与agent映射再启动A；C必改0后仅Lead功能提交与唯一12项全OK门禁；D后置文档分列feature/gate/delivery HEAD；crates/tools再变重闸。
- C8：每attempt原日志/UTC/命令/cwd/HEAD/环境/EXIT/统计/首败expected-actual/bytes/SHA只增；历史freeze不改不入库，历史BLOCK及永久缺口不回写。

- M1（派发前，A/B/C共同契约）：更正计划L3旧基线ea9fd79为本审议HEAD；保留原计划SHA审议快照，另记修订SHA。真实链明确120/123；125/71不是本轮施法包链。
- M2（A注释、B/C来源核验、D台账）：写入C1–C4，特别区分“Duration=400事实”和“elapsed截止产品窗”；无owner=TargetPoint；rate1近似与完整视觉PARTIAL。
- M3（A/B/C测试契约）：G2显式测60/79/80及399/400/401，M3须先证Magic2[15]诱饵存在；G5表按120/123×cast、149×param及两技能逐行列输入/旧金样结果，禁止矩阵近似替换。
- M4（B/C交付契约）：完整逐M日志/原件/恢复SHA及9项计数；oracle算法独立、共享夹具原字节SHA可复核，各自Python实现单独冻结，不把独立实现要求误读为跨语言脚本同SHA。
- M5（A/B/C与D）：定向历史测试实际计数及SHA保留；R71/R72旧ThunderBolt param=false金样不得改；C逐条对照原变异patch，任何条件不满足即BLOCK/返工。

- 锚点勘误：计划主要源码锚点正确；协议INDEX精确行在[server.rs:402](../../crates/mir-protocol/src/manual/server.rs#L402) / [496](../../crates/mir-protocol/src/manual/server.rs#L496)，不是结构声明385/471；Lightning为[PlayerObject.cs:3225](../../github/Crystal/Client/MirObjects/PlayerObject.cs#L3225) Magic970+Direction*20/6帧，须保旧行为。
- PARTIAL台账：承接R71/R72原号——GAP-5/R70-C1其他技能/1620/其他调用点；R70-C3全局尺寸高度居中；GAP-1/2/R70-C2 rate0.8/DrawColour/INVLIGHT/完整DrawBlend/API；GAP-3/4/6 mask/tint/shadow/场景offset/Light3；时钟/100ms取整/持久149/26移除/寿命与4000ms产品窗；R72 B plan-M4残余、legacy lint债。
- PARTIAL继续：R71-C7永久、E1仅补救CLOSED，R69条件5/a77永久缺口；R68/R49–R70/R61–R67及服务端回收/全局水位/Live音频/全UI/设备/跨后端/真实视觉全部保留；新增R73-T1起手/ActionFeed/owner跟随/严格Effect时钟/回执起算/Light6/音频/400ms截止偏差，不能整项清零。
- CLI：审前show EXIT0，Menxia、todos=[]、无delegation字段；state Assigned --as menxia EXIT0，复读Assigned；validate --json --as menxia EXIT0，ok=true/schema v3.1/237 tasks/issues=[]；须由尚书登记执行职责，流程校验不代表功能通过。
- 证据：[门下静态与CLI目录](../../target/round73/menxia/attempt-1/)；本审议未改crates/原计划，未跑cargo/CI，未commit/push。
