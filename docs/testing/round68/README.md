# R68：125 与 570 逐帧图像偏移

任务 `JJC-20261027-005`「第68轮：125与570逐帧图像偏移」；D 子单 `JJC-20261027-005-sub-61532b37`。
状态：**PASS-WITH-PARTIAL 候选**。阶段声明：**D 文档批 + 待独立终审**。
功能提交 `c9e94c4bf78386028e45b54f1056df6abacee1e8`，5 文件 +1394/−8；成功门禁 HEAD `6ba5d459091ec4a5e18b457a3304926b2441a4c3`。

## 范围与不做清单

仅登记 R61 P04 的 125 飞行/570 命中逐帧图像偏移消费子项：新显式入口取当前帧 signed offset，saturating_add 后 blit；MapScene 转发，ProjectileState 两处接线。详见[契约与 G1–G4](round68-offset.md)。
旧 draw_effect 绝对左上角语义不变；spellfx 仍走旧入口。生产 wire/server 零改动；新增 ignore/allow=0，历史 ignore=7 不变。
本轮不做全 UI offset、spellfx 居中迁移、真实素材/设备视觉签核、DrawBlend/shadow/mask、事件/位置容量、多消费者或服务端回收。
夹具为合成 WZL + 真实协议/渲染链；不构成真实素材视觉签核。125 无点坐标语义不变。

## 交付文件与指纹

下表 crates SHA 按成功门禁 HEAD 的 git_blob 内容计算；文档/证据按工作树原始字节计算。完整 bytes/lines/SHA 见[清单](final-manifest.json)，其自身 SHA 外置于[摘要文件](manifest.sha256)，清单不自引用。

| 交付/依据 | 行数 / 字节 | SHA256 |
|---|---|---|
| [entities.rs](../../../crates/mir-render/src/entities.rs#L1847) | 3415 / 142361 | `965197467c5392c713e01926cdd1ecc7a5392242aa0aa6d8f4963667c328d8ef` |
| [scene.rs](../../../crates/mir-render/src/scene.rs#L433) | 1281 / 48880 | `8a44a5a9764c731391ab1acd6b588b667889d765b5c212679b40165a960f6f93` |
| [projectile.rs](../../../crates/mir-client/src/ui/projectile.rs#L927) | 2086 / 99433 | `3528a4ee0252a1bc0af5235dace5e92f437b79075300828fabcdc1ed409acdea` |
| [wire 测试](../../../crates/mir-client/tests/round68_image_offset_wire.rs) | 569 / 23355 | `c330a4b628749184461b595fe0681fbf161f042b578586ebd02483e390908c73` |
| [live 测试](../../../crates/mir-client/tests/round68_effect_offset_live.rs) | 751 / 30666 | `39a032c74d71b52f23f06be653e31cbb81c96967691cea384064bd5965116038` |
| [计划](../../process/round68-plan.md) | 83 / 10243 | `ba4d7149c997617a952e844119da3255d2a3f7e90797f9a59b5890b969afd294` |
| [门下评审](../../process/round68-menxia-review.md) | 42 / 6181 | `95c408148438d9c0748f6caa8d75305c972e1bb67ff3db3dce0e0fc5ee5a9f96` |
| [尚书派发](../../process/round68-shangshu-dispatch.md) | 40 / 6599 | `f3da0c77f57c87b578afdf042d88464ee50e18909d2d75d27f219a7dea2cb6c8` |

门下准入 PASS-WITH-CHANGES 的 C1–C8/M1–M5，由 [C 首轮逐项矩阵](round68-c-evidence.md#L35)、[追加核验](round68-c-addendum.md)和[最终核验](round68-c-final.md)连续记录。实际测试文件名及回归组相对初始计划有调整：两新增测试均落在 mir-client/tests，C 最终复跑为 R68 2+2、R62 5、R63 3，不把计划命令当执行事实。
D 交付还包括本索引、[偏移契约](round68-offset.md)、[client §5.33](../../client.md#533-r68125570-逐帧图像偏移)及[路线图 §十六](../../roadmap.md)。

## 真红矩阵与最终绿

| 证据 | 变异/结果 | SHA 与边界 |
|---|---|---|
| [A attempt-3](../../../target/round68/a/attempt-3/manifest.json) | 旧语义可编译壳；live EXIT101，0 passed/2 failed，G4/G3 首败均 live:283:9；恢复绿共 12 passed | 最终 live `39a032c7…`；wire 红交叉引用 [attempt-2](../../../target/round68/a/attempt-2/manifest.json)，同最终 wire SHA |
| [B attempt-3](../../../target/round68/b/attempt-3/manifest.json) | R1 支持层 offset 置零：wire 0/2、live 0/2；R2 仅飞行退旧入口：wire 2/0、live 1/1；R3 仅命中退旧入口：wire 2/0、live 0/2；各红 EXIT101 | 独立物理副本；wire 为最终 SHA；live 为中间 `19cf4cce…`，不能称最终 live 三变异复跑 |
| [B attempt-4](../../../target/round68/b/attempt-4/manifest.json) | helper 增加直接 `left.pixels == right.pixels`，绿 12 passed/0 failed/0 ignored | 最终 live SHA；本批只重做绿，最终 live 红由 A attempt-3 提供 |
| [C 最终](round68-c-final.md) | 独立新 target/tmp：R68 4 + R62 5 + R63 3 = 12 passed/0 failed/0 ignored；clippy/fmt EXIT0 | 直接 RGBA 字节比较；signed、逐帧、center/运动各一次与旧入口边界 |

有效红须编译成功、目标用例实际执行、目标 offset 断言首败带 left/right 与行号、EXIT101；编译错/0 tests/skip 不计。G3 到达时也绘命中，故 R3 的 live 双红有明确原因。

## Lead 门禁与失败披露

外层命令：`pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`。D 未运行该门禁，以下转录 Lead 证据。

| attempt | HEAD / UTC（2026-10-27） | EXIT / 结果 | 原件 |
|---|---|---|---|
| 1（失败，保留） | `c9e94c4bf78386028e45b54f1056df6abacee1e8`；02:26:28Z–02:28:42Z | 1；单点失败为 cargo test --workspace 的 R68 live 两用例，未启用 mir-render/legacy，合成 magic.wzl 不可读 | [log](../../../target/round68/gate/attempt-1.log) / [meta](../../../target/round68/gate/attempt-1.meta.txt) |
| 2（成功基线） | `6ba5d459091ec4a5e18b457a3304926b2441a4c3`；02:31:50Z–02:34:56Z | 0；12/12 OK，0 FAIL；门禁后 crates 零 diff | [log](../../../target/round68/gate/attempt-2.log) / [meta](../../../target/round68/gate/attempt-2.meta.txt) |

修复提交 `6ba5d45`「build(ci): R68 门禁 workspace 测试启用 mir-render/legacy 特性」，仅 ci.ps1:118 增加 `--features mir-render/legacy`（+3/−1）；内部命令变为 `cargo test --workspace --features mir-render/legacy -- --skip duo_ --skip fault_pg`。D 不修改 ci.ps1。
attempt-1 log 70912B，SHA `54716e56907a07ea9e84ba05cf4b2a1167d0c3cddd3c4773767d0c90061906eb`；attempt-2 log 210789B，SHA `54cc09020c6b724ffa606a22a1634cac4429d1130c73e853e55c2d9f79da3f38`。
attempt-2 meta 由 Lead 在门禁结束后按日志实测值补全为 13 行/687B，含完整命令、12/0、bytes/SHA 与 operator；原始日志字节未改。其 SHA 为 `413dade505b63a5312d9b905b3661233b2312e4b11de0b22b85d43f3034e13b5`。attempt-1 meta 保留原件。

## C 三轮结论

| 报告 | 结论与范围 | 行数 / bytes / SHA256 |
|---|---|---|
| [首轮](round68-c-evidence.md) | BLOCK；必改1/2/3退 B，实现缺陷0 | 60 / 6816 / `e8898e2c7b4ff3f3af89527fc5c6ac5cb73ac14c2eecbe5242e5cac8344a8d24` |
| [追加](round68-c-addendum.md) | BLOCK；2/3 CLOSED，1 PARTIAL（摘要比较不足以证明字节相等） | 36 / 6198 / `5f83587196e81922fcdec92196d68f10917cecb2350b4ec884a31f5bbf57f7ff` |
| [最终](round68-c-final.md) | PASS-WITH-PARTIAL；1/2/3 全 CLOSED，剩余必改0 | 26 / 4021 / `0152aaad1be48b5616cffbcc8890469f51cc8c401f02ef94cddf54550a466411` |

## PARTIAL 台账

- A 旧 manifest:394 措辞勘误
- B 共享 isolated 旧日志归档限制
- 真实素材/设备视觉签核
- 全 UI offset
- spellfx 居中迁移
- DrawBlend/shadow/mask
- R49–R67 历史遗留

A 旧“真实资源字节”措辞应以合成 WZL 限定理解，正式历史勘误仍承接；B 旧共享原日志被替换，旧首败引用 C 留存摘录与旧日志 SHA，不能称全部原日志未覆盖。
历史原编号承接：R49–R60/R60 P-1–P-6、R61 其余 P01–P12、R62①–④、R63 P1–P7、R64①–⑥、R65 G1.2/G2、R66 生命周期/全局水位、R67 容量/保护/map epoch/同代 ID 复用；详见[R67 台账](../round67/README.md)。
R69 候选为真实素材视觉签核、全 UI offset、spellfx、DrawBlend/shadow/mask，目标留规划。D 文档与静态自检不代替独立终审。
