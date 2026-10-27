# R71 火墙逐格精灵映射与锚点迁移 Implementation Plan

> 中书拟案；主单 `JJC-20261027-008`；独立门下批准前不执行，不代表功能/门禁已通过。
> 执行团队按 subagent-driven-development 逐任务交付；每步勾选仅表示执行者已取得对应证据。

**Goal:** 单轮交付 `ObjectSpell` 火墙的 Magic 1630–1635、120ms循环与当帧 signed offset 生产接线。
**Architecture:** 仅在既有 `spellfx::draw_sprites` 中增加 `FireWall && from_object_spell` 专属分支，沿用 R70 Add+offset API；非目标路径继续原行为。复用事件时钟/4000ms窗口，明确保留 Crystal 调度及生命周期差异。
**Tech Stack:** Rust、CPU RGBA8、真实149协议解码、合成WZL/WZX、Cargo定向测试；无新依赖。
**Spec:** [roadmap §十八:298](../roadmap.md#L298)、[R70台账:56](../testing/round70/README.md#L56)、[R70尚书验收:21](round70-shangshu-acceptance.md#L21)。

## 全局约束与选题

- 规划基线 `ff5a6cae52eea37bc2dbb0b84d65acac1ea738a1`；主树已有两份历史freeze未跟踪，禁止触碰/纳入提交。
- 本规划者只写本页及 `target/round71/zhongshu/**`；不写crates、不跑cargo/CI。本文命令均为未来执行契约。
- 选优先候选①的有限切片：R70已提供生产绘制能力，现可用真实149→事件→公开绘制入口取得玩家调用点证据。
- 不称“Crystal FireWall完整迁移”：0.8参数/DrawColour、Light=3、严格NextMotion、对象存续及移除均未对齐。
- 完整DrawBlend矩阵跨rate/INVLIGHT/状态/mask，单轮范围不受控；GAP-3/4缺第二层/绘制契约，本轮否决。
- 服务端回收协议、完整音频Live、全UI/GPU/真实素材视觉签核单轮不可闭环，否决纳入。
- coding仅deepseek团队；独立门下/刑部不得与方案作者/实现者同人；只有Lead提交、push与运行全量。

## 实读事实锚点

|来源|现场事实与本轮含义|
|---|---|
|[Crystal SpellObject:30](../../github/Crystal/Client/MirObjects/SpellObject.cs#L30)|实际文件在 `Client/MirObjects`，没有本任务所指的 `Client/MirScenes/SpellObject.cs`；Load读取149位置，37行Repeat=true。|
|[Crystal SpellObject:49](../../github/Crystal/Client/MirObjects/SpellObject.cs#L49)|FireWall=Libraries.Magic、1630、120ms、6帧、Light=3、Blend=true；不能借用通用前10帧。|
|[Crystal SpellObject:288](../../github/Crystal/Client/MirObjects/SpellObject.cs#L288)|NextMotion初始向100ms取整；294用>=，每次Process最多前进一帧，296–302循环。本轮elapsed取模并非严格等价。|
|[Crystal SpellObject:323](../../github/Crystal/Client/MirObjects/SpellObject.cs#L323)|DrawLocation从格坐标及相机/运动量生成，没有按帧width/height居中；FireWall的AnimationOffset默认0。|
|[Crystal SpellObject:328](../../github/Crystal/Client/MirObjects/SpellObject.cs#L328)|335–339 DrawBlend(DrawFrame+FrameIndex,DrawLocation,DrawColour,true,0.8F)。|
|[Crystal PlayerObject:1802](../../github/Crystal/Client/MirObjects/PlayerObject.cs#L1802)|1620/10帧挂施法者，属于起手；[3057](../../github/Crystal/Client/MirObjects/PlayerObject.cs#L3057)完成分支仅声音，不是地面1630帧序列。|
|[Crystal MLibrary:692](../../github/Crystal/Client/MirGraphics/MLibrary.cs#L692)|699先加当帧mi.X/Y、701再裁剪；704–709只保存恢复bool。|
|[Crystal DXManager:353](../../github/Crystal/Client/MirGraphics/DXManager.cs#L353)|378–379 NORMAL=SourceAlpha/One；383设置255×rate BlendFactor。不可仅凭0.8推导CPU的src×0.8，完整D3D等价仍PARTIAL。|
|[Rust spellfx:235](../../crates/mir-client/src/ui/spellfx.rs#L235)|242–250统一age/60并clamp0..9；252–256雷电Bolt，其余Magic；257–265按尺寸居中后旧Over。|
|[Rust spellfx:40](../../crates/mir-client/src/ui/spellfx.rs#L40)|FireWall=AREA_MS=4000，158以age<0或>=duration跳过；本轮沿用窗口。|
|[Rust FxKind:497](../../crates/mir-render/src/entities.rs#L497)|509–522 Magic映射magic.wzl且start_frame=0，Bolt映射magic2且start_frame=20；FireWall直接传索引1630+phase。|
|[Rust scene:452](../../crates/mir-render/src/scene.rs#L452)|既有draw_effect_blend(...,apply_offset:bool)->bool薄转发；[entities:1874](../../crates/mir-render/src/entities.rs#L1874)及1915–1923完成一次saturating_add→Add。|
|[Rust net:1541](../../crates/mir-client/src/net.rs#L1541)|SpellEvent含from_object_spell/started_ms；[5624](../../crates/mir-client/src/net.rs#L5624)真实149同时写瞬态spell_events及持久spell_fields。|
|[Rust UI:4280](../../crates/mir-client/src/ui/mod.rs#L4280)|生产UI已调spellfx::draw_sprites(snapshot.spell_events)，本轮无需改UI入口/网络生产代码。|
|[R68夹具:127](../../crates/mir-client/tests/round68_effect_offset_live.rs#L127)|可复用合成WZL→EntityLibraries→MapScene模式；独立新夹具，不改历史断言/冻结值。|

## 最小范围、接口与不做

- 唯一生产写文件：[spellfx](../../crates/mir-client/src/ui/spellfx.rs#L226)；新增测试 `crates/mir-client/tests/round71_firewall_mapping.rs`。
- 新常量：`FIRE_WALL_BASE_INDEX: u32 = 1630`、`FIRE_WALL_FRAME_COUNT: i64 = 6`、`FIRE_WALL_FRAME_MS: i64 = 120`，放既有MAGIC_FX常量附近。
- 不新增公开映射框架/时钟类型；保留 `pub fn draw_sprites(frame: &mut Frame, events: &[SpellEvent], scene: &mut MapScene, center: Point, now_ms: i64)` 签名。
- 在现有age/duration过滤之后、通用frame_index/size计算之前分支：仅 `event.spell == Spell::FireWall && event.from_object_spell`。
- phase=`(age / FIRE_WALL_FRAME_MS) % FIRE_WALL_FRAME_COUNT`；索引1630+phase；直接使用 `scene.world_to_screen(center,event.location)`。
- 调 `scene.draw_effect_blend(frame, FxKind::Magic, index, x, y, true)` 后continue；不调用effect_size居中、不另加offset、不画通用闪光第二次。
- 149每格独立事件，每事件一图；不在客户端再铺十字或按方向复制。非149 FireWall保留原通用行为，且明确其仍非Crystal起手1620迁移。
- 缺目标文件/帧跳过该图，无低帧/相邻帧/占位回退；后续其他事件正常绘制。已有色块/文字叠层保留。
- 不改net/协议/服务端、spell_fields/4000ms寿命、125/570、其他技能、Frame/EntityRenderer/MapScene算法或公共API。
- 不做rate参数、INVLIGHT、tint/mask/shadow、Light、真实素材导入、严格NextMotion/SkipFrames和持续场对象生命周期。

## Review Focus

- 149与非149 FireWall混淆：G1/G5必须分别断言；1620/通用0帧不能冒充1630。
- 720ms回卷或3999ms末端冻结：G2钉住所有边界；等号/未来事件均有断言。
- 当帧尺寸和偏移同时变化：G3整帧oracle证明没有居中或双加修正。
- 屏外锚点被提前裁剪/缺帧吞后续事件：G3/G5分别覆盖。
- 合成夹具错误地图/图库导致空白假绿：G1先断言帧可读取，禁止SKIP与零测试。

## 子任务DAG：A → B → C → D

门下批准 → 尚书先登记四子单与agent映射 → A兵部 → B工部 → C刑部 → D礼部 → Lead唯一门禁 → 尚书验收 → 指定门下确认。

|任务|责任署/依赖|允许写入与交付|
|---|---|---|
|A|兵部 deepseek；依赖门下批准|生产单文件+新测试；先写8项验收测试、再最小分支实现；`target/round71/a/attempt-N/**`保存定向命令/结果。|
|B|工部 deepseek；depends_on=A|独立物理副本验证与五变异；只写 `target/round71/b/attempt-N/**`。发现测试缺口退A修改，B不代改正式生产/测试。|
|C|刑部独立审核者；depends_on=B|自有物理副本重跑定向、复现五变异、核SHA/原件/范围；只写证据目录及 `docs/testing/round71/round71-c-evidence.md`；可BLOCK。|
|D|礼部；depends_on=C通过|新增 `docs/testing/round71/README.md`、manifest及外置sha；更新[客户端](../client.md)/[roadmap](../roadmap.md)相关小节；只用实录结果，不重跑Cargo。|

- [ ] A先编写下表G1–G5的8个命名测试及独立RGBA oracle；确认目标149包链与原有分支能编译。
- [ ] A运行T71，取得映射/坐标断言失败（旧实现若编译失败则先修测试准备，不能记功能红）。
- [ ] A实现限定分支，运行T71与回归R，提交文件列表/SHA及diff给Lead审阅，不自行git commit。
- [ ] B冻结最终测试SHA，逐一执行M1–M5的green-before→断言red→原字节恢复green-after。
- [ ] C在自有副本复跑同测试SHA及M1–M5，逐G给PASS/BLOCK/PARTIAL，必改0后才交D。
- [ ] D写限定切片结果/PARTIAL台账/证据索引，Lead核验后进行唯一全量；全量结果由Lead追加并独立验收。

## 可执行命令与G验收

以下由A/B/C在各自副本运行；工作目录、UTC、HEAD、EXIT、stdout/stderr和测试统计全部保全。

T71：
```text
cargo test -p mir-client --features mir-render/legacy --test round71_firewall_mapping -- --test-threads=1
```
R（每条分别记EXIT，不用后续成功覆盖前条失败）：
```text
cargo test -p mir-client --features mir-render/legacy --test round70_effect_blend_offset
cargo test -p mir-client --features mir-render/legacy --test round68_image_offset_wire
cargo test -p mir-client --features mir-render/legacy --test round68_effect_offset_live
cargo test -p mir-client --lib ui::spellfx
cargo test -p mir-render --lib
cargo clippy -p mir-client --all-targets -- -D warnings
cargo fmt --check
```

|G|T71必有的可失败断言|通过条件/变异|
|---|---|---|
|G1 图库/真实生产链|`r71_g1_wire_mapping`：encode_frame→decode_server_packet→WorldSnapshot::apply_packet_at→spell_events→公开draw_sprites；先assert from_object_spell/位置/帧存在。Magic0、1620、Magic2同索引放不同色诱饵。|t=0画Magic1630，旧0/20/1620位置无误图；M1将1630改0必须RGBA断言红。|
|G2 帧序/窗口|`r71_g2_frame_boundaries`：age=0,119→1630；120→1631；599→1634；600,719→1635；720→1630；840→1631；3999→1633。`r71_g2_age_window`：-1、4000、i64边界无图/不panic。|每时点独立清帧；M2将循环改clamp必须720断言红；不宣称Crystal严格NextMotion。|
|G3 锚点/裁剪|`r71_g3_anchor_offsets`：1630/31尺寸不同且offset=(-3,5)/(4,-2)，world_to_screen原点+当帧offset一次；跨帧/同帧重复/换center。`r71_g3_offscreen_crop`：屏外格锚点被offset移入、负坐标部分裁剪。|独立整帧oracle相等，原居中位置/双offset位置无误绘；M3恢复尺寸居中、M4关闭offset均各自产生断言红。|
|G4 Add/逐格|`r71_g4_add_per_cell`：非黑背景/饱和像素；真实五条149十字（独立object_id）各一图，叠加与旁邻空白均断言。|按独立公式min(255,dst+round(src*a/255))验证RGB；M5换draw_effect_with_offset即Over应断言红；不推定D3D RGBA/rate等价。|
|G5 兼容/缺帧|`r71_g5_legacy_sources`：非149 FireWall、ThunderBolt/Lightning、PoisonCloud与旧居中/Over整帧金样一致。`r71_g5_missing_frame`：缺1631/无Magic，不退0帧；混合列表下一事件仍能绘制。|8测试实际运行、0failed/ignored/filtered；缺素材测试是显式期望无图，其余映射测试必须有素材且非空；R全绿。|
|G6 证据保全|B/C工具验证源文件/测试SHA、隔离物理文件、每变异五组前绿/红/后绿日志、恢复SHA；任何 mismatch/0tests/skip 均FAIL。|M1–M5均编译成功并实际执行T71后在预定断言失败，恢复8/0；C同SHA独立重现，不复用B结果冒充。|
|G7 唯一全量/边界|Lead在最终功能树运行下述门禁；`git diff --check`及逐文件清单核验。|EXIT0且12/12 OK；HEAD/UTC/命令/原log/meta字节与SHA齐全；门禁后crates或工具改动必须由Lead新attempt重跑。|

## 红绿与证据规则

- B/C每个attempt独立真实物理repo副本，禁hardlink/symlink/junction共享可变源码；记录绝对cwd、源root、nlink=1及隔离target/tmp/out。
- 各自 `CARGO_TARGET_DIR`、TEMP/TMP/TMPDIR；必要时 `CARGO_PROFILE_DEV_DEBUG=0`应在全部对照中一致并披露，不能把D8050环境错误算红。
- 最终测试文件/夹具/执行器SHA在前绿、红、后绿、C复验完全一致；仅生产目标语句按M1–M5各改一处，保存diff和恢复前后SHA。
- 每个有效red必须编译成功、running8、命中指定断言失败/EXIT非0，随后恢复8passed/EXIT0；编译错、0tests、导入错、PermissionDenied、SKIP/ignore不是红。
- 测试个数若因必要新增变化，A提交新清单，B/C全部重新冻结重跑；不得减少本表8项或以过滤后0测试交差。
- 证据 `target/round71/{a,b,c,gate}/attempt-N/**` 只增不改，含命令/环境/cwd/HEAD/UTC/EXIT/计数/首败expected-actual/manifest；失败attempt也保留。
- manifest记录逐文件bytes/SHA256且不自引用，外置sha摘要；两历史freeze及R69原始证据不改不删。

## 唯一门禁、提交边界与回滚

- 仅Lead：`pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`；不加SkipTests/SkipPostgres等降级项，不借R70旧绿。
- A/B/C/D不commit/push、不跑全量；Lead逐批审阅后只暂存明确白名单，功能批仅生产单文件+新测试；证据/文档批另提交。
- 本规划提交例外已获本轮用户明确授权：只提交本计划，消息 `docs(process): R71 计划（第71轮）`，不push。
- D收口可写上述四类文档；不得扩改历史freeze/R69矩阵冻结值/CI脚本来换绿。
- 若范围/断言需要改动，退门下重审；环境阻断保存原log并修环境，不降低通过标准。
- 回滚由Lead逆向撤本轮spellfx分支及新增测试/文档链接，不reset共享工作树、不删证据，无数据库迁移。

## PARTIAL承接与风险

|原项|本轮可结子项|继续PARTIAL|
|---|---|---|
|GAP-5 / R70-C1|限定149 FireWall的图库/帧/格锚点/offset生产接线|非149起手1620、其他spellfx/UI调用点、真实素材视觉。|
|R70-C3|仅目标FireWall去尺寸居中，按格原点+帧offset|全局尺寸/高度/居中与其他技能基准。|
|GAP-1/2 / R70-C2|仅复用R70 rate1 Add，无新状态|rate0.8完整语义、INVLIGHT/DrawColour/完整DrawBlend/公共API全面收敛。|
|GAP-3/4/6|无|mask/tint/shadow绘制、GameScene条件offset/照明；shadow保持解析不消费。|
|本切片显式偏差|6×120ms elapsed循环，仅0≤age<4000|Crystal初始100ms取整/逐Process步进、Light3、149持久场与26移除；已有瞬态事件寿命并非对象寿命。|
|R68 / R69条件5|仅新增独立证据|历史归档/措辞、全UI、真实设备/跨后端；a77原红/隔离/日志缺失永久PARTIAL。|
|R49–R67/R61–R67原编号|无整项清零|全部未闭合历史项按R70台账原编号承接，服务端回收/全局水位/Live音频不扩大。|

风险：扁平语料未必含1630；合成WZL只能证明语义，真实图库缺帧按无图处理并保留视觉PARTIAL。elapsed跳帧与4000ms寿命会和Crystal持续场不同，不能以本轮通过声称其已解决。

## 待门下裁决（放行前）

1. 接受“149 FireWall映射与锚点有限迁移”，明确保留rate0.8、严格时钟、Light/DrawColour、对象生命周期偏差；若要求完整Crystal等价，应封驳重拟，不能执行中偷偷扩范围。
2. 认可G1真实149链+G2 elapsed循环而非严格NextMotion；认可非149 FireWall本轮保持旧行为且台账不清零。
3. 审核五变异覆盖、8项测试命名、物理副本/同SHA/禁0tests/编译错非红、Lead唯一门禁/提交边界可执行。
4. 独立门下出具approve/reject材料后，尚书才能登记A→B→C→D；本规划者不自批，不代门下Assigned。

CLI披露：create默认Taizi；显式 `state ... Taizi` 因Taizi→Taizi不合法EXIT3，show证实仍Taizi，再按合法路径转Zhongshu；原回执不伪称成功。后续以真实show/validate为准。
