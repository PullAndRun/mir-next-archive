# R70 实施计划：混合特效逐帧 signed offset 生产消费

> 中书拟案，主单 JJC-20261027-007；送门下独立审议，不代表实现/验收通过。
> 规划只写本页及 target/round70/zhongshu/**，不写 crates、不跑 cargo/ci.ps1、不 commit/push。

## 目标与选题裁决

**目标：** 单轮交付渲染层 Add 特效入口，显式选择消费当前帧 signed offset，恰好一次、先于裁剪，以可失败像素测试闭环。
**架构：** EntityRenderer 内部统一实现，旧 Over 与新 Add 入口共享元数据/裁剪顺序；MapScene 薄转发，沿用 Frame::blit_add，无全局混合状态。
**技术：** Rust CPU RGBA8、WZL/WZX 合成夹具、cargo 定向测试、JSON证据；不加依赖。
**规格：** [R69条件6:39](round69-menxia-review.md#L39)、[矩阵C2:896](../testing/round69/render-contract.json#L896)；使用 writing-plans 与 edict。
**选定：** 方向1，C2 的混合分支 offset 生产消费子切片。rate固定为既有Add语义对应的1，不新增rate参数；完整DrawBlend与完整C2仍PARTIAL。
**否决C1/C3：** FireWall须先解图库/帧/坐标准入，Crystal1630/6×120ms/blend0.8 对 Rust0/10×60ms/Over；直接迁移会混入映射/时序，C3也依赖该坐标契约。
**否决其他：** shadow在Crystal没有绘制消费，不能发明行为；mask需第二纹理解码契约；方向2 magic精确化牵涉播放时长/server时序，另案客户端侧规划。

## 实读事实锚点

- HEAD=origin/main=`72d9313e225dbb825cece84ffd5753e0a1e25612`；R69主单JJC-20261027-006 Done。221为此前validate计数，本轮以新回执为准。
- [Crystal MLibrary:692](../../github/Crystal/Client/MirGraphics/MLibrary.cs#L692)：DrawBlend默认offSet=false/rate=1，699先Offset，701判屏外，704–709只保存/恢复bool。
- [Crystal DXManager:353](../../github/Crystal/Client/MirGraphics/DXManager.cs#L353)：378–379 NORMAL=SourceAlpha/One；373–375 INVLIGHT=BlendFactor/InverseSourceColor；383–384设255×rate，不能称统一One/One。
- [Crystal Effect:201](../../github/Crystal/Client/MirObjects/Effect.cs#L201)：先逐帧index/运动量，213–216 Draw/DrawBlend均传true，后者传Rate；本轮不改动画语义。
- [Crystal MLibrary:778](../../github/Crystal/Client/MirGraphics/MLibrary.cs#L778)：785主图偏移，792–794 mask用同主point/width/height，不消费MaskX/Y。
- [Rust entities:1816](../../crates/mir-render/src/entities.rs#L1816)：绝对坐标旧入口；[1847](../../crates/mir-render/src/entities.rs#L1847)读取当帧offset并saturating_add；[1923](../../crates/mir-render/src/entities.rs#L1923)缓存仅blit/Over。
- [Rust scene:417](../../crates/mir-render/src/scene.rs#L417)、[433](../../crates/mir-render/src/scene.rs#L433)：只有两个Over包装，无Add特效入口。
- [Rust frame:226](../../crates/mir-render/src/frame.rs#L226)已有blit_add；[428](../../crates/mir-render/src/frame.rs#L428)、[468](../../crates/mir-render/src/frame.rs#L468)实际RGB=min(255,dst+round(src×alpha/255))；保留alpha行为，不称D3D RGBA等价。
- [R68测试:1](../../crates/mir-client/tests/round68_image_offset_wire.rs#L1)、[237](../../crates/mir-client/tests/round68_image_offset_wire.rs#L237)：合成WZL/WZX→真实EntityLibraries解码→公开API→独立整帧RGBA oracle，可复用思路，不改历史断言。
- [矩阵gaps:802](../testing/round69/render-contract.json#L802)为6项；candidates的landing/criteria/non_goals见[C1:872](../testing/round69/render-contract.json#L872)、[C2:896](../testing/round69/render-contract.json#L896)、[C3:919](../testing/round69/render-contract.json#L919)。
- [roadmap:283](../roadmap.md#L283)、[client:1406](../client.md#L1406)、[R69终审补充:19](../testing/round69/round69-final-audit-addendum.md#L19)承接PARTIAL，旧pre-audit措辞不覆盖终审事实。

## 最小diff与不做

- [ ] A在entities:1816–1870收敛私有实现：`fn draw_effect_impl(&mut self, frame: &mut Frame, kind: FxKind, index: u32, x: i32, y: i32, apply_offset: bool, additive: bool) -> bool`。
- [ ] 新增 `pub fn draw_effect_blend(&mut self, frame: &mut Frame, kind: FxKind, index: u32, x: i32, y: i32, apply_offset: bool) -> bool`；scene:433–443后同名薄转发，kind沿用crate::entities::FxKind，无需新导出类型。
- [ ] ensure_image→当帧metadata→可选saturating_add→缓存图绘制；Add调blit_add，否则blit。缓存命中不克隆Image、不新增逐帧分配、不修改metadata。
- [ ] 旧draw_effect=(false,false)，旧with_offset=(true,false)，新blend=(参数,true)；有图完全屏外true，缺图false，不把可见性当存在性。
- 生产白名单仅entities/scene；新增测试 `crates/mir-client/tests/round70_effect_blend_offset.rs`；允许frame原单测区加半透明Add断言（仅测试代码）。
- 新API是实际可调用的生产渲染实现，由集成测试调用；本轮不迁移游戏UI/事件调用点，不称玩家已看到新效果。C2公共API全面收敛仍PARTIAL。
- 不碰wire/server/magic映射、图库帧号/播放时长/全局语义、125/570、spellfx居中、GameScene条件、资源、历史freeze；不改R69矩阵冻结值/工具以迁就实现。
- 不做rate=0/0.5、INVLIGHT、状态机、tint/mask/shadow绘制、GPU/设备/真实素材签核、全局UI offset。

## 子任务DAG与执行预算（100分钟）

`门下批准 → B夹具/oracle → A实现 → B最终绿/四变异 → C独立核验 → D文档 → Lead全量门禁 → 尚书/门下终审`

|任务|owner/预算|依赖/产物|验收命令/断言|
|---|---|---|---|
|A工部|deepseek/25分钟|依赖B约定签名和oracle；共同实现及scene转发|T70、两套T68全过；新增API实际调用非桩；旧Over整帧不变，缓存无克隆/新分配|
|B兵部|deepseek/25分钟|先设计夹具；A后固定最终测试SHA，保全绿/四变异红绿|T70至少7命名测试、0ignored/0filtered；M1–M4有效断言红后恢复绿|
|C刑部|gpt/20分钟|A/B后，独立物理副本、自有out/tmp/target；不改A/B产物|同SHA T70/T68独立跑，核首败/diff/命令/计数/EXIT/SHA；可BLOCK，不代改测试|
|D礼部|gpt/10分钟|C后，新R70索引/报告并适量更新client/roadmap|本地链接、UTF-8/LF；逐G真实命令/断言/EXIT；PARTIAL表；禁止把计划写成实跑|
|Lead|主会话/20分钟储备|C/D后；超时保全attempt调整预算，不降级|最终树唯一全量EXIT0且逐项成功；门禁后生产再改须重跑|

T70（未来B/C各自副本运行）：
```text
cargo test -p mir-client --features mir-render/legacy --test round70_effect_blend_offset -- --test-threads=1
```
T68（未来分别执行，不能用filter后0tests充数）：
```text
cargo test -p mir-client --features mir-render/legacy --test round68_image_offset_wire
cargo test -p mir-client --features mir-render/legacy --test round68_effect_offset_live
```
半透明补测命令：`cargo test -p mir-render --lib`；独立oracle覆盖alpha=0/128/255、非黑背景与饱和，不调用生产blit_add生成期望值。
规划15分钟硬预算；本页命令未在规划阶段运行。执行超时退门下缩案，不能省掉独立核验/全量门禁。

## 可失败验收G与red→green双证据

|G|失败模式/断言|红证据|绿条件|
|---|---|---|---|
|G1逐帧offset|两帧(+8,-8)/(-8,+8)，原点/单轴/上一帧无残留；M1关闭Add offset|编译成功后位置整帧RGBA assertion首败，expected/actual保全|独立oracle整帧相等、metadata不变、只叠一次|
|G2先偏移后裁剪|屏外锚点经offset移入、部分裁剪、负坐标/i32极值；M2加偏移前早退|移入像素缺失assertion失败|正确像素出现；有效屏外true/缺图false；极值不panic|
|G3实际Add|非黑背景opaque彩色2×2 WZL；M3改Add为Over；另测alpha0/128/255|RGB覆盖与加法不同，整帧/通道assertion失败|独立公式匹配、饱和255、alpha0无变化、alpha行为锁定|
|G4显式关闭|blend(false)绝对坐标、true偏移；M4忽略false总偏移|false落点assertion失败|两路径各正确，不接受任意外来offset，不引全局状态|
|G5兼容无泄漏|交替Add/Over/旧with_offset、重复缓存/跨帧、scene与entities|如回归保留T70/T68首败；禁止伪造旧基线红|T68全过；旧Over与变更前整帧一致；Add不污染后续Over|
|G6证据边界|SHA不同、共享root/tmp、改freeze或越界即拒收|审计FAIL列首个不一致，不算功能红|C独立重现四变异同SHA；0→101→0或实际等价断言退出码；证据只增不改|
|G7全量|Rust生产却无最终全量、失败或门禁后再改|Lead新attempt日志/EXIT/失败项|最终树全量EXIT0且逐项成功，不借R69门禁|

- 至少7命名测试：正负/跨帧、裁剪、存在语义、Add像素、关闭offset、交替无泄漏、scene转发；不许只有源码字符串检查。
- 新API不存在导致编译错只是开发准备，不是功能红；有效红使用最终API已编译的物理副本，生产语句M1–M4最小变异，各保存同最终测试SHA的green-before/red/green-after。
- B/C每个attempt独立repo物理副本，禁symlink/junction/hardlink共享源码；独立CARGO_TARGET_DIR、TEMP/TMP/TMPDIR、cwd/out；可复用只读下载缓存，不共享可变编译target。
- 清单逐文件bytes/SHA256；先绿、仅改生产目标语句、保存diff/SHA、恢复原字节再绿。编译错/0tests/导入失败/未知基线失败/skip/ignore均拒作红。
- `target/round70/{b,c}/attempt-N/`只新建：命令/cwd/UTC/隔离路径/stdout/stderr/EXIT/计数/首败/manifest；不覆盖旧attempt，不清历史target。
- C核同测试/工具SHA与相同计数；测试/工具改动后四变异及C须重跑；修复追加报告，不改首败原件。

## 门禁、风险与回滚

- 只有Lead运行[唯一门禁:1](../../tools/dev/ci.ps1#L1)：`pwsh -NoProfile -File tools/dev/ci.ps1`，不加SkipTests/SkipPostgres等降级参数；B/C只跑定向测试。
- 冷构建/数据库超预算保留失败/未完成状态与原始输出，重新排预算；环境错误不当功能红，静态检查不替代全量。
- 复用Add公式/saturating_add，禁止改frame算法；oracle从原始合成像素独立计算；缓存/旧Over由G5和代码审阅把关。
- 回滚仅撤本轮entities/scene差异与新增测试/文档链接；提供最小逆向diff由Lead审阅应用；不重置用户工作区、不删历史证据、不动两freeze；无需协议/服务端迁移。
- 探路曾读不存在测试路径/根目录ci路径，已纠正为实存路径，不作事实锚点；CLI失败保存真实回执，不手改看板JSON/冒用角色。

## PARTIAL承接（逐项）

|原项|本轮做|不做/口径|
|---|---|---|
|GAP-1 DrawBlend|rate=1既有Add子集新增特效入口+offset|rate/INVLIGHT/完整DrawBlend仍PARTIAL，原验收未全达成|
|GAP-2 bool恢复|显式参数，G5无泄漏|不仿造Crystal缺陷；场景状态后置|
|GAP-3 mask|无|解码/第二层/tint仍PARTIAL，不加MaskX/Y偏移|
|GAP-4 shadow|无|解析不消费；不称shadow全解析/绘制完成|
|GAP-5 spellfx|无UI迁移|旧居中/图库帧坐标准入/逐帧消费仍PARTIAL|
|GAP-6 GameScene|仅底层bool表达本次偏移|fileIndex/index条件/照明仍PARTIAL|
|R70-C1 landing/criteria/non_goals|无|spellfx:257–265未接，准入/时序差异未解，非目标不松绑|
|R70-C2 landing/criteria/non_goals|entities共同实现+Add子切片；单次/先裁剪/关闭/无分配|公共单API全面收敛未做，不动mapping/time/wire，不建状态机|
|R70-C3 landing/criteria/non_goals|无|尺寸/高度/居中基准仍PARTIAL|
|R69条件5历史保全|本轮证据只增不改|a77红/隔离/日志不可恢复，永久PARTIAL，本轮重做不修历史|
|DrawBlend/shadow/mask/spellfx|仅Add定位子集|其余缺口保留，禁止称全局UI offset完成|
|真实素材/像素/GPU/设备|合成夹具CPU确定性RGBA|真实素材视觉/跨后端像素/GPU设备未签核|
|R68遗留|T68回归125/570，运动/图像偏移各一次|旧manifest措辞/共享isolated旧日志归档限制保留|
|R49–R67遗留|按原编号全量承接|不消账、不动历史freeze/资源/协议/server|

## 规划交付与门下审点

- 真实CLI/偏差见[planning-notes](../../target/round70/zhongshu/planning-notes.md)；机器检查见[plan-checks](../../target/round70/zhongshu/plan-checks.json)。
- 正文≤220行、UTF-8无BOM/LF、本地链接存在；记录行数/bytes/SHA256、show/validate实际EXIT；看板停Menxia，不代批准/派发。
- 门下审点：生产渲染API落地而UI未迁移的边界；四变异独立失败机制；GAP-1/C2仍PARTIAL；独立物理副本、首败保全与唯一全量门禁。
