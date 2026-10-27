# R68 门下省独立审核（JJC-20261027-005）
verdict：PASS-WITH-CHANGES

审核范围：计划 docs/process/round68-plan.md 全文83行；基线HEAD=2c69c70fff34913e59fabb779b5bd602a6c32ff5；Crystal HEAD=0e315fe327192afe52c3d7357ddd1f5b7e26c5b8，均实测；以下为计划准入裁决，不代表实现或测试已通过。

| 审查项 | 证据与结论 |
| --- | --- |
| Crystal 锚点真实性 | MLibrary.cs:880-890 的 X/Y 由 ReadInt16 读取，:658-670 先 point.Offset 后屏幕裁剪；Effect.cs:112-132 的 Owner/地图锚点及 :201-216 的 Missile 逐帧运动后均传 true；四处属实，DrawBlend 仅作参考。 |
| Rust 现状与缺口 | image.rs:35-44 为 i16；wzl.rs:161-186 保留 signed；library.rs:410-448 有合成WZL/WZX范式；entities.rs:1800-1830 缓存图像但 draw_effect 不消费 offset；scene.rs:413-427 转发；projectile.rs:917-929 飞行/命中两路均缺消费；spellfx.rs:257-265 已居中。 |
| 新入口必要性 | 同意 EntityRenderer/MapScene 新增 draw_effect_with_offset，按 kind.library() 与 index+start_frame() 取当帧 cached_info，分别 saturating_add(i32::from(offset))；旧 draw_effect 保持绝对左上角，避免居中调用二次偏移。 |
| 裁剪顺序与负坐标 | 同意先叠图像偏移再 blit_cached；frame.rs:222-223/:292-305 已支持负坐标及 i64 裁剪，不得在加偏移前提前剔除锚点；缺帧false，存在但完全屏外仍true。 |
| 测试与真红 | G1–G4 覆盖读入→缓存→公开UI像素链，R1/R2/R3可分别检出漏偏移/漏飞行/漏命中；新增API缺失的编译错不能作基线红；须落实M1–M5后由B/C实际执行，门下未跑cargo。 |
| 边界与 PARTIAL | 同意125/570切片，仅承接R61 P04图像偏移消费子项；全UI、通用spellfx、真实素材视觉、DrawBlend、shadow/mask及其余历史项继续PARTIAL，不据合成像素清零整项。 |

## 放行条件 C1–C8（纳入派发及验收）
- C1：G1须经真实LibrarySource读回(-3,5)/(4,-2)/(-2,-3)/(3,4)，以整帧像素断言signed负偏移及450/451、570/571逐帧不同offset，原点/双偏移处不得误绘。
- C2：G3/G4须走71/125真实encode/decode→apply_packet_at→UiState.sync→projectiles().draw，先断言views/hits、精确sprite_index及严格推进，再独立计算投影+运动一次+当帧offset，换center与重复draw结果符合平移且不累积。
- C3：G2须证明旧入口带非零offset仍画原点、返回值维持原义、缓存meta与调用者坐标不变，重建Frame及交替新旧入口重复绘制不累积。
- C4：G2须覆盖零offset、部分负坐标裁剪、全屏外、缺帧及屏外锚点经offset移入屏内，分别断言完整像素与存在性返回值，极端i32坐标不panic。
- C5：R1/R2/R3须各在无硬链接的独立物理源码副本中单独变异，红绿对应测试SHA一致，记录绝对manifest/cwd、独立CARGO_TARGET_DIR/TMP/TEMP/TMPDIR及源SHA，证明实际重编译了变异源。
- C6：有效红须编译成功、目标命名测试执行、断言首败带left/right与行号且exit101；0 tests、ignored/skip、编译错、改测试造红及无关失败一律不计，绿须各2 passed/0 failed/0 ignored。
- C7：合成素材验收不得因缺game-assets而SKIP，也不得称真实视觉签核或设备Live通过，文档仅关闭R61 P04的125/570消费子项并逐项保留PARTIAL。
- C8：只许按A/B/C/Lead/D白名单与串行依赖执行，禁wire/服务端/旧入口语义/Frame全局行为变更，禁改两份历史freeze，任何受影响源码或测试SHA变化须重跑对应红绿及失效门禁。

## 必改 M1–M5（均为可执行契约补强，无已证硬缺陷）
- M1：A/B派发明确“测试先落盘→最小可编译旧语义转发壳→断言红→消费offset实现→绿”，转发壳不得先加offset；API不存在的编译失败仅记准备态，不能充真红或替代R1–R3。
- M2：B将G2返回值写死为“缺帧false；有效图像即使全屏外仍true”，增加原锚点屏外经offset移入可见区及i32边界用例，禁止将true解释成必有可见像素。
- M3：B将G4到达写成独立固定tick表：出生1000，依次1032/1063/1094/1125/1156/1187/1218/1249，到达产生570；1309相等不进，1310进571，禁止一次大跳伪造到达。
- M4：A/B回执固定索引与坐标独立预期：Magic start_frame=0，右向dir16=4，450→451；1032运动=(30,0)，不得复用被测draw_location/offset生成oracle，G4也禁用elapsed helper推进帧。
- M5：A同步修正scene.rs:412“返回宽高”旧注释为bool存在性；projectile.rs:911及新增入口注明运动偏移与ImageInfo偏移各一次，并由C对照派发逐项列C1–C8/M1–M5证据和实际计数。

## 派发约束与收口
A→B→C→Lead功能提交与唯一门禁→D严格串行；A/B先测试后实现由同一工程责任链安排，B测试白名单不得越界改生产，C生产/测试只读，修复退A/B。
尚书将本审核条件附入子单，C独立重跑G1–G4、R59/R62回归并另做R2/R3之一；同SHA三红、freeze前后SHA及白名单均须核实，必改0且PASS-WITH-PARTIAL才交Lead。
仅Lead依计划执行功能提交与ci.ps1 -KillStale，须核无并发cargo/rustc，EXIT0且12/12；门禁后crates变化使门禁失效；D仅在门禁成功后按事实落文，清单不自引用，保留失败attempt。

## 范围与 PARTIAL 边界
R61 P04仅限定补齐125飞行/570命中逐帧图像偏移；全UI offset、通用/手spellfx居中迁移、真实素材视觉签核、DrawBlend、shadow/mask均不作。
R49–R60/R60 P-1–P-6、R61其余P01–P12、R62①–④、R63 P1–P7、R64①–⑥、R65 G1.2/G2、R66生命周期/全局水位、R67容量/保护/map epoch/同代ID复用继续承接；125无点坐标语义不变。
历史README的pre-audit/Doing是快照，按上级R67已Done事实接续，不改历史；client.md:1263-1276为R59历史偏差，不外推其他偏差已在R68修复。

本审核仅写本文件，未运行edict CLI、cargo或ci.ps1，未commit/push、未改计划/crates；行数/bytes/SHA256由落盘后外部实测回报，避免文件自摘要。
