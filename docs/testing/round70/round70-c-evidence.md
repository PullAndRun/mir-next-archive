# R70 C 独立核验

- **verdict**: **PASS-WITH-PARTIAL**；未发现实现必改，但本轮独立 Cargo/变异执行被宿主 MSVC `ring v0.17.14` 构建错误阻断。
- 时间：2026-10-27 UTC（命令日志见 `target/round70/c/attempt-1/logs/`）。范围：仅核验；未改实现/测试/历史 freeze，未 commit/push，未跑 `tools/dev/ci.ps1`。

| 项 | 结果 | 证据/摘要 |
|---|---|---|
| 1 静态核验 | PASS | status 仅 entities/scene/frame M、新 R70 测试、两 freeze ??；protocol/server diff 为空；旧入口转发；无新依赖；生产新增无 unwrap/panic；唯一 `too_many_arguments` 为私有 8 参 impl；新增 ignore=0；`blit_cached_add` 仅 cache.get + blit_add。 |
| 2 独立复跑 | PARTIAL | 5 Cargo 命令均启动但在编译 `ring` 失败，未产生可计数测试结果；fmt EXIT 0。首败：`cl : Command line error D8050 ... c1.dll ... failed to get command line into debug records`。 |
| 3 SHA 核对 | PASS | test `b188febc...df84`；frame `b2b18643...9aee`；entities `f8dff727...7afc6`；scene `d570258f...5db98`，见 runs/static JSON。 |
| 4 四变异 | PARTIAL | `isolated/repo-m1..m4` 均由 `git archive HEAD` + 四文件独立拷贝构造，mutation.patch 留存且恢复原字节；本轮 red/green 均被同一 D8050 阻断，不能声称编译成功运行 9 tests。 |
| 5 兼容/泄漏 | PARTIAL | G5/T68/半透明 alpha 断言存在；本轮未执行。B 既有 green 记录为 9 passed、T68 2+2 passed。 |
| 6 边界 | PASS | 文档明确 CPU Add 子集，不宣称 rate/INVLIGHT/完整 DrawBlend、真实素材/GPU 签核，也未把四变异称全部变异；frame.rs 补测在允许范围。 |

## 独立命令计数

| 命令 | UTC/EXIT | 计数 |
|---|---|---|
| round70_effect_blend_offset --test-threads=1 | logs/01；101 | 未运行 tests（ring 编译失败） |
| round68_image_offset_wire | logs/02；101 | 未运行 tests（同上） |
| round68_effect_offset_live | logs/03；101 | 未运行 tests（同上） |
| mir-render --lib | logs/04；101 | 未运行 tests（同上） |
| clippy mir-render/mir-client -D warnings | logs/05；101 | 未进入 clippy（同上） |
| cargo fmt --all -- --check | logs/06；0 | check clean |

## 四变异矩阵

本轮副本均已构造、物理改句、保存 patch、恢复原字节；执行阶段统一被 D8050 阻断。B 对照首败：M1 L461 `(28,12)` `[0,0,0,255] -> [248,0,0,255]` EXIT101；M2 L622 `(2,1)` `[0,0,0,255] -> [248,0,248,255]` EXIT101；M3 L506 `(28,12)` `248 -> 255`，G3 目标 L758 `(3,3)` `[248,0,0,255] -> [255,50,60,255]` EXIT101；M4 L885 `(10,10)` `[0,0,0,255] -> [0,252,0,255]` EXIT101。B green 为 EXIT0；本轮 green/red 为 BLOCKED，不等同 B 证据。

## PARTIAL 承接

- C-P2：在具备可工作的 MSVC/ring 构建宿主后，重跑 logs/01–05，记录 9/2/2/133 计数、clippy 与变异 red-green。
- C-P3：补写本轮每个变异的编译成功、`running 9 tests`、首败断言行和左右值。

缺陷：宿主构建环境最小复现为 `ring` C 编译执行 `cl.exe ... c1.dll` 报 D8050；非实现断言缺陷。
