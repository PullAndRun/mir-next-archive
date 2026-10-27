# R69 渲染契约工具轮

**阶段：契约工具轮 PASS；生产接线未做，待 R70。** 本批为 pre-audit，独立终审待办，不代表整轮最终验收。
任务 `JJC-20261027-006`；D 子单 `JJC-20261027-006-sub-6326a5ec`。

## 范围与交付

目标是把 Crystal 源码事实冻结成可失败的 G1–G5 契约，并验证审计工具能发现真实语义变异。
本轮未改生产渲染、协议/wire、服务端或历史 freeze JSON；像素/GPU、真实素材/设备、全 UI 一致性未背书。

- [人工 expected 矩阵](render-contract.json)：先人工冻结，工具只产 observed，禁止反向回填自证；详见[契约摘要](round69-contract.md)。
- [审计工具](../../../tools/testing/round69_render_contract.py)：1,356 行；最终只读审计 G1–G5 5/5。
- [契约测试](contract-tests.py)：2,026 行、84,962B；B selfcheck 11/11；命名契约测试 15 tests、0 skip。
- 提交 `8fbb71b`：工具/矩阵/测试 3 文件 +4,344；`492b20c` 修复构建证据链接检查；D snapshot HEAD 与逐件 blob/SHA/bytes 见 [pre-audit 清单](final-manifest.json)，自身 SHA 单列于 [manifest.sha256](manifest.sha256)。

## Crystal 证据基准

以下行号基于矩阵冻结源码，所述事实不扩大到运行时像素一致性。

| 来源 | 行号与事实 |
|---|---|
| [MLibrary.cs](../../../github/Crystal/Client/MirGraphics/MLibrary.cs) | 640–673：整数重载不偏移，Point 默认 offSet=false，true 时单次偏移先于裁剪；692–710：DrawBlend 同顺序与 rate；704–709：仅保存/恢复 bool；778–797：主体/mask 同 point/宽高；880–901：元数据反序列化；919–928：mask 用 Width/Height 与 Length；862–900：shadow/mask 声明与读取。 |
| [DXManager.cs](../../../github/Crystal/Client/MirGraphics/DXManager.cs) | 353–389：SetBlend 默认 1F/NORMAL；NORMAL=SourceAlpha/One，INVLIGHT=BlendFactor/InverseSourceColor。 |
| [Effect.cs](../../../github/Crystal/Client/MirObjects/Effect.cs) | 112–132、196–216：Draw/DrawBlend 消费显式 true，混合传 Rate。 |
| [SpellObject.cs](../../../github/Crystal/Client/MirObjects/SpellObject.cs) | 323–347：绘制点、AnimationOffset、显式 true 与 blend 0.8F。 |
| [GameScene.cs](../../../github/Crystal/Client/MirScenes/GameScene.cs) | 10923–10935：条件 offset 与整数重载；11085–11087、11264–11273：场景 raw blend/照明边界，不能泛化为 G3 完整状态恢复。 |
| [PlayerObject.cs](../../../github/Crystal/Client/MirObjects/PlayerObject.cs) | 1708–1709、2956–2963：FireWall 来源与特效分派核对锚点，仅用于候选准入。 |

## 红绿与独立核验

B 最终权威证据为 [attempt-3 PROVENANCE](../../../target/round69/b/attempt-3/PROVENANCE.md) 与 [46 件 manifest](../../../target/round69/b/attempt-3/manifest.json)。
四变异及补充分支：R1→G2，R2→G2+G3，R3→G4+G5，R4/R4b→G5；每项基线 EXIT0→变异 EXIT1→恢复 EXIT0。
逐变体物理隔离复制、无硬链接/符号链接/reparse、独立 root/out/tmp；相同最终测试 SHA 下首败原文与恢复件保全。
[C 首轮](round69-c-evidence.md) **BLOCK**：必改 1 仅溯源；功能/测试 PASS，生产一致性 PARTIAL。
[C 追加](round69-c-addendum.md) **PASS-WITH-PARTIAL**：必改 1 关闭为“披露+重做闭合”；46/46 SHA+bytes 吻合，五变体 0→1→0，三冻结 SHA 匹配，mtime≤04:35Z（mtime 非不可篡改证明）。

## 历史保全事件与裁定

Lead 要求默认模式重跑，覆盖 B attempt-1 的 a77 原始红/隔离/日志；无备份、不可恢复，保留的旧输出不能替代原始首败件。
[Lead 裁定](../../process/round69-lead-ruling.md)归因为默认重跑指令设计缺陷；attempt-3 只作为最终 SHA 权威首败证据，不冒充历史原件。
**条件 5 历史保全永久 PARTIAL**；补救完成不抹除首轮 BLOCK。此后证据目录只增不改，重跑必须新 attempt/新 work-root。
偏差已由 C 追加披露：PROVENANCE:46 的 R2 字段笔误（G2 实际首败 restore_args，G3 才是 restore_call.argc）；采样 HEAD 并发变化，冻结文件 SHA 未变。

## Lead 门禁与环境

| 门禁 | HEAD | EXIT/结果 | 原件 |
|---|---|---|---|
| attempt-1 | `8fbb71b90e1e1d7fd4b9757c18697b69789208d1` | 1；仅文档链接检查失败 | [log](../../../target/round69/gate/attempt-1.log)、[meta](../../../target/round69/gate/attempt-1.meta.txt) |
| attempt-2 | `492b20c53c40c5d103bd8115f692397efc4eb080` | 0；12/12 OK；04:59:06–05:01:36Z | [log](../../../target/round69/gate/attempt-2.log)、[meta](../../../target/round69/gate/attempt-2.meta.txt) |

用户要求清理 target，Lead 已执行约 340GB→2.75GB：删除重建型构建产物和已关闭 R49–R68 target 证据，保留本轮目录；历史 target 链接断裂导致 attempt-1 失败，非功能缺陷。
`492b20c` 使链接检查跳过 target/ 构建证据链接；1660 检查、564 跳过。attempt-2 log 210,828B，SHA `ced721954f9b9e446c4f663cd1efff723be583a41f773fd2e728b76b4633d084`；meta 741B，SHA `0ba8cd38dad86120f632cc8a4edeb548ca872fa7c3bd9ad7a7f79ea1f85b0e0f`。
D 核对 `git diff 492b20c -- crates` 为空；D 不重跑 ci.ps1。

## R70 候选与自检

三候选原文见[契约](round69-contract.md)：C1 spellfx 单路径 signed offset，C2 可选 offset API，C3 帧尺寸/高度基准。
FireWall 单一路径候选为 [spellfx](../../../crates/mir-client/src/ui/spellfx.rs) 257–265 → [entities](../../../crates/mir-render/src/entities.rs) 1847–1868；生产接线尚未做。
先证明图库/帧/坐标准入：Crystal Magic1630、6帧×120ms、blend0.8 与 Rust Magic0、10帧×60ms、Over 不等价；magic 映射精确化须独立规划，不能借 offset 接线偷改时序/wire。
D 自检命令、实际 EXIT 与实算结果见 [pre-audit 清单](final-manifest.json)。冻结工具只支持 --root/--out/--matrix；要求中的 --audit --check-observed 按原样记录 EXIT2，并以受支持默认审计另证 5/5。
