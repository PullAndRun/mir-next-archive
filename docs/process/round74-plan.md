# R74 普通 NORMAL Blend 跨图库测试锁 Implementation Plan

> 中书规划；基线HEAD=origin/main=`0ee34e1b86b6fa2a9a9e1506a08e022eee5bb965`；TASK_ID见看板回执。
> For agentic workers: REQUIRED SUB-SKILL: subagent-driven-development 或 executing-plans；门下独立批准前不得执行。

**Goal:** 候选②降级：普通NORMAL/rate1 CPU Add跨Magic/Magic2测试锁及rate/Mode差异合同，完整DrawBlend保持PARTIAL。
**Architecture:** 保留无rate/Mode的API，以跨图库/0与20起点/混合交替及独立oracle锁现有CPU行为；生产只修R73一行注释。
**Tech Stack:** Rust、合成RGB565 WZL/WZX、CPU RGBA8、独立Python oracle；无新依赖。
**Spec:** [R73台账](../testing/round73/README.md#L39)、[R72台账](../testing/round72/README.md#L26)、[R71台账](../testing/round71/README.md#L36)、[研究](../../target/round74/zhongshu/research.md)。

## 全局约束与完成定义

- 2–3人日；生产仅`crates/mir-client/src/ui/spellfx.rs:309`一行注释；一个新测试文件；不改历史测试。
- 本规划只写本计划、target/round74/zhongshu/**及CLI看板；不写crates、不commit/push、不跑cargo/CI、不启动执行者。
- 完成定义：新5项绿、5个单变异编译成功→指定断言红→恢复绿，B/C独立oracle三源一致，D差异合同与PARTIAL齐备。
- 测试锁原基线预期已绿；无功能修复红，可失败验收来自变异红→恢复绿；编译/feature/环境失败不算功能红。
- 非目标：rate0.8/INVLIGHT/DrawColour实现、FireWall/PoisonCloud合同改写、strict时钟/寿命、通用0/20映射、owner/Light/音频/真实视觉。
- 两份未跟踪monster-native freeze JSON原字节保全、不入库；不reset共享树、不覆盖历史证据。

## 选题与实读锚点

|锚点|事实与决定|
|---|---|
|[Effect.cs:81](../../github/Crystal/Client/MirObjects/Effect.cs#L81)|Time<=NextFrame直接return；Time>NextFrame才推进，修R73反向注释。|
|[Effect.cs:38](../../github/Crystal/Client/MirObjects/Effect.cs#L38)|NextFrame=Start+(Duration/Count)*(CurrentFrame+1)；83为SkipFrames，96重排期；Rate默认1。|
|[SpellObject.cs:288](../../github/Crystal/Client/MirObjects/SpellObject.cs#L288)|仅首次100ms向下取整；294为Time>=NextMotion，302后按当前时间+间隔。|
|[net.rs:1541](../../crates/mir-client/src/net.rs#L1541)|SpellEvent无帧/NextMotion/目标owner；[draw_sprites](../../crates/mir-client/src/ui/spellfx.rs#L287)只读事件、elapsed计算；候选①无法单文件闭合两类时钟/对象驱动。|
|[SpellObject.cs:335](../../github/Crystal/Client/MirObjects/SpellObject.cs#L335)|149场另用DrawBlend(true,0.8F)，本轮禁改。|
|[MLibrary.cs:692](../../github/Crystal/Client/MirGraphics/MLibrary.cs#L692)|699先offset后701裁剪；704仅保存bool，705传rate，709恢复bool但默认rate/Mode。|
|[DXManager.cs:353](../../github/Crystal/Client/MirGraphics/DXManager.cs#L353)|缓存bool/rate/Mode；NORMAL SourceAlpha/One；INVLIGHT BlendFactor/InverseSourceColor；383写BlendFactor不证明NORMAL的src×rate×alpha。|
|[entities.rs:497](../../crates/mir-render/src/entities.rs#L497)|Magic→Magic/start0；Swing→Magic2/start0；Bolt→Magic2/start20。|
|[entities.rs:1874](../../crates/mir-render/src/entities.rs#L1874)|API无rate/Mode；1904为index+start，1916/1917一次offset，1923 Add。|
|[R70测试](../../crates/mir-client/tests/round70_effect_blend_offset.rs#L42)|夹具仅Magic图库；已覆盖单库Add/Over交替，新增跨图库隔离与0/20锁才有增量。|

- 候选①严格时钟降级PARTIAL：Effect与SpellObject不同，通用0/20无统一Crystal实例；仅承接已登记勘误。
- 候选②完整rate/Mode否决：NORMAL因子与设备状态未闭合，不能简单乘0.8；选其允许的文档+测试锁。
- 候选③–⑤未选：已有更高优先级可交付降级项；未读目标选择/Shutdown不冒称已排除，C7/E1不重开。

## 写范围与接口

|属主|白名单|责任|
|---|---|---|
|A兵部|crates/mir-client/src/ui/spellfx.rs仅注释；新crates/mir-client/tests/round74_normal_blend_lock.rs|恰5项测试及夹具；无mir-render生产改动。|
|A/B/C各自|target/round74/{a,b,c}/attempt-N/**|物理副本、原日志、冻结SHA、变异diff/oracle。|
|C刑部|docs/testing/round74/round74-c-evidence.md及追加附录|独立逐G/逐M核验，保留BLOCK历史。|
|Lead/协调|docs/process/round74-*.md；target/round74/{lead,gate,menxia,shangshu,t,t2}/**|唯一提交/门禁、裁定、独立终审/验收。|
|D礼部|docs/testing/round74/README.md、round74-normal-blend-contract.md、final-manifest.json、manifest.sha256；docs/roadmap.md新R74节、docs/client.md新R74节；target/round74/d/**|六文档及自检，不改历史合同/报告。|

- 保持`MapScene::draw_effect_blend(&mut self, frame: &mut Frame, kind: FxKind, index: u32, x: i32, y: i32, apply_offset: bool) -> bool`。
- G4用`Frame::blit_add(&mut self, image: &Image, x: i32, y: i32)`；不新增rate API、不伪造0.8调用。
- 新夹具仅参考R70格式，不导入私有测试helper；Magic/Magic2同索引异色，Magic2 0/20/21设诱饵。

## Review Focus 与 G1–G5

|命名测试|最小可失败断言|
|---|---|
|G1 r74_g1_cross_library_start_indices|Magic(0)/Swing(0)/Bolt(0)/Bolt(1)读Magic0/Magic2 0/20/21；同scene与分离renderer相等；整帧独立expected，禁止effect_image作期望。|
|G2 r74_g2_cross_library_signed_offsets|两库同帧不同尺寸与offset(-3,2)/(4,-1)，true/false；屏外锚点经offset移入、完全屏外true、缺帧false；坐标手算整帧。|
|G3 r74_g3_cross_library_add_over_sequence|同像素Magic Add→Magic2 Add→Magic Over→Magic2 Add，底[40,60,80,255]、RGB565红绿蓝/透明黑；每步整帧对oracle，重复不污染offset。|
|G4 r74_g4_cpu_alpha_rounding_is_unscaled|RGBA源[101,53,7,a]，a=0/1/127/128/254/255，非黑及饱和底；a128时[10,20,30,255]→[61,47,34,255]；与src×0.8模型不同，仅锁CPU rate1。|
|G5 r74_g5_missing_and_legacy_continuation|缺Magic2帧false后同scene继续Magic成功；再Bolt20及旧Over各对金样；禁缺帧fallback/吞后续。|

- 五焦点依次为错库/+20、offset先后/返回、跨库混合/缓存、舍入/误乘0.8、缺帧继续/兼容；均有对应G断言。
- WZL只有alpha0/255；G4 RGBA128独立于WZL链，不称真实半透明素材/设备验证。
- B/C各自Python struct/zlib读实际WZX/WZL，自解RGB565/坐标/Add/Over，输出整帧RGBA/SHA；G4另读RGBA向量，禁生产helper作oracle。

## 五变异与失败验收

|变异（仅隔离生产副本）|指定红|
|---|---|
|M1 Bolt start_frame 20→0|G1应Magic2[20]实际[0]。|
|M2 Swing library Magic2→Magic|G1错库异色。|
|M3 draw_effect_impl仅offset_x加法改减法|G2负offset坐标。|
|M4 draw_effect_blend末additive true→false|G3非黑底Add变Over。|
|M5 Add分支非零source alpha强制255|G4 a128应[61,47,34]实际[111,73,37]。|

- B/C各自前绿5/0/0→单变异→build0→指定断言RED101→恢复原字节SHA→后绿5/0/0；不语义替换。
- 最终测试/夹具SHA不变；0tests/ignored/SKIP/feature失败不算红；失败原日志保留，新attempt纠正。
- A先基线五绿，再至少M4隔离红→恢复绿；不伪造基线功能缺陷。

## DAG、依赖与步骤

`门下批准 → 尚书登记 → A → B → C → Lead提交/门禁 → D → 独立终审 → 尚书验收 → 门下确认`

|段/depends_on|步骤/交付|
|---|---|
|A/门下批准|[ ] 五项测试冻结SHA；[ ] 基线绿/M4负控；[ ] 注释勘误；[ ] 定向回归；原字节交Lead，不commit。|
|B/A冻结|[ ] 物理副本nlink=1与独立target/TEMP；[ ] 五绿/五变异/独立Python oracle，不改A文件。|
|C/B完成|[ ] 另副本独立重做变异/oracle；[ ] 核首败expected/actual、恢复SHA及历史保全；必改0才交Lead。|
|Lead/C必改0|[ ] 只提交白名单；[ ] 唯一门禁及原始EXIT，保留失败attempt；[ ] 成功后交D。|
|D/Lead门禁成功|[ ] 六文档含NORMAL/INVLIGHT/rate表、勘误、PARTIAL；[ ] 逐blob/bytes/SHA/链接核，再交终审。|

## 未来命令与门禁边界（本次未运行）

```text
cargo test -p mir-client --features mir-render/legacy --test round74_normal_blend_lock -- --test-threads=1
cargo test -p mir-client --features mir-render/legacy --test round70_effect_blend_offset --test round71_firewall_mapping --test round72_poison_cloud_mapping --test round73_thunderbolt_mapping --test round63_projectile_wire
cargo clippy -p mir-client --all-targets -- -D warnings
cargo fmt --all -- --check
```

- 新5项及历史38项（9+8+9+9+3，以实跑核计数）0 failed/ignored；不改历史SHA，不新增allow/ignore。
- 唯一全量仅Lead：`pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`，不SkipTests/SkipPostgres，真实输出核12项；D纯文档后不重跑，crates/tools变更须重闸。
- 当前仅静态：UTF-8无BOM/LF、≤170行、链接存在、git diff --check/stat、crates零差异、看板validate原始EXIT；不称功能PASS。
- 每命令记录UTC/cwd/完整命令/HEAD/环境/EXIT/计数/stdout/stderr/bytes/SHA；独立CARGO_TARGET_DIR和TEMP/TMP/TMPDIR，禁共享缓存/hardlink/junction/symlink。

## 风险、回滚与PARTIAL

- 依赖本地Crystal、legacy、Python及R70格式；无新增资源/数据库需求；未来全量数据库仅Lead负责。
- 合成锁不裁定D3D状态；NORMAL rate0.8仅登记事实，另轮补设备/状态证据。若新矩阵已被历史完整覆盖，退中书缩纯合同，不堆重复测试。
- 带legacy clippy既有needless_return债继续披露、不顺手修；B/C先核空间/日志保全。
- Lead仅逆撤新增测试/文档与一行注释；可执行语句与基线相同；不删证据、不reset历史。
- GAP-1/2 / R70-C2（用户候选②标题称C1，历史DrawBlend归C2）完整rate/Mode/DrawColour/API仍PARTIAL，不重编号。
- GAP-5/R70-C1映射、R70-C3、R72 B plan-M4、GAP-3/4/6原号照承；FireWall/PoisonCloud合同不改。
- R71时钟/对象、R73-T1仍PARTIAL，仅注释勘误可CLOSED；C7永久PARTIAL、E1补救CLOSED不重开；R69条件5与更早原号照承。
- R68/R49–R70/R61–R67真实素材/GPU/设备/全UI/跨后端/Live音频未验收；不代改历史裁决。
- 禁称：整轮通过、严格Crystal等价、设备视觉签核、完整DrawBlend闭合、rate0.8已实施、时钟/寿命已修复、C7已补齐、基线功能红、无条件PASS。
- 下一步仅门下独立审核“文档+测试锁、完整混合仍PARTIAL”；作者不自批、不转Assigned、不派发执行者。
