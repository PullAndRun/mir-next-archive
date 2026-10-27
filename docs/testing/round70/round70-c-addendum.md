# R70 C 补充独立核验

- **verdict：PASS-WITH-PARTIAL；独立执行 PARTIAL CLOSED（C-P2/C-P3 关闭）**。本轮最终无未解 BLOCK；CPU Add 子集之外的既有边界不扩大签核。
- HEAD `b11deacd1a591247f1fe806b221c28a913e0ea35`；执行 UTC `2026-10-27T06:45:57Z–06:49:55Z`；宿主 Windows/MSVC。
- 绕行：`CARGO_PROFILE_DEV_DEBUG=0`；全新 `CARGO_TARGET_DIR=target/round70/c2/attempt-1/cargo-target`；`TEMP=TMP=TMPDIR=target/round70/c2/attempt-1/tmp`（实际使用仓库下绝对路径）。首次 R70 编译并执行 40.266s，未再出现 ring D8050。
- 每条完整命令、UTC 起止、EXIT、计数、日志索引及初次失败均见 [runs.json](../../../target/round70/c2/attempt-1/runs.json)；以下计数顺序为 passed/failed/ignored/filtered。

| 独立命令 | EXIT | 计数 | 日志 |
|---|---:|---|---|
| `cargo test -p mir-client --features mir-render/legacy --test round70_effect_blend_offset -- --test-threads=1` | 0 | 9/0/0/0 | [01](../../../target/round70/c2/attempt-1/logs/01-r70.log) |
| `cargo test -p mir-client --features mir-render/legacy --test round68_image_offset_wire` | 0 | 2/0/0/0 | [02](../../../target/round70/c2/attempt-1/logs/02-r68-wire.log) |
| `cargo test -p mir-client --features mir-render/legacy --test round68_effect_offset_live` | 0 | 2/0/0/0 | [03](../../../target/round70/c2/attempt-1/logs/03-r68-live.log) |
| `cargo test -p mir-render --lib` | 0 | 133/0/0/0 | [04](../../../target/round70/c2/attempt-1/logs/04-render-lib.log) |
| `cargo clippy -p mir-render -p mir-client --all-targets -- -D warnings` | 0 | clean | [05](../../../target/round70/c2/attempt-1/logs/05-clippy.log) |
| `cargo fmt --all -- --check` | 0 | clean | [06](../../../target/round70/c2/attempt-1/logs/06-fmt.log) |

- 隔离副本 [repo](../../../target/round70/c2/mut/repo/) 来自 `git archive HEAD`，仅覆盖工作树四文件，四项 `st_nlink=1`；四变异顺序注入/逐项恢复，共用本轮 target。有效 red 均含编译完成、`running 9 tests`、`FAILED`；首败位置均指冻结 R70 测试。

| 变异 / patch | 首败行与 actual → expected | red EXIT / passed,failed | 恢复 green |
|---|---|---|---|
| M1 关闭偏移 [patch](../../../target/round70/c2/attempt-1/mut/M1.patch) | G1 L461 `(28,12)` `[0,0,0,255] → [248,0,0,255]` | [101 / 2,7](../../../target/round70/c2/attempt-1/logs/M1-red.log) | [EXIT0 / 9 passed](../../../target/round70/c2/attempt-1/logs/M1-green.log) |
| M2 偏移前屏外早退 [校正 patch](../../../target/round70/c2/attempt-1/mut/M2-corrected.patch) | G2 L622 `(2,1)` `[0,0,0,255] → [248,0,248,255]` | [101 / 8,1](../../../target/round70/c2/attempt-1/logs/M2-corrected-red.log) | [EXIT0 / 9 passed](../../../target/round70/c2/attempt-1/logs/M2-corrected-green.log) |
| M3 Add 改普通 blit [patch](../../../target/round70/c2/attempt-1/mut/M3.patch) | G1 L506 `(28,12)` `[248,0,0,255] → [255,0,0,255]`；G3 L758 `[248,0,0,255] → [255,50,60,255]` | [101 / 5,4](../../../target/round70/c2/attempt-1/logs/M3-red.log) | [EXIT0 / 9 passed](../../../target/round70/c2/attempt-1/logs/M3-green.log) |
| M4 强制偏移 [patch](../../../target/round70/c2/attempt-1/mut/M4.patch) | G4 L885 `(10,10)` `[0,0,0,255] → [0,252,0,255]` | [101 / 7,2](../../../target/round70/c2/attempt-1/logs/M4-red.log) | [EXIT0 / 9 passed](../../../target/round70/c2/attempt-1/logs/M4-green.log) |

- **初次 M2=BLOCK，注入器校正后 CLOSED**：[原 patch](../../../target/round70/c2/attempt-1/mut/M2.patch) 误用 `Frame.width()/height()`，产生 [E0599 / EXIT101](../../../target/round70/c2/attempt-1/logs/M2-red.log)，未运行 tests，不能计有效 red。原 patch/log/恢复绿均保留；另以 `M2-corrected` 使用既有公有字段执行，未覆盖或隐去失败，未修改主树实现/测试。
- 每项完整首败摘要见 [M1](../../../target/round70/c2/attempt-1/mut/M1-first-failure.json)、[M2 校正](../../../target/round70/c2/attempt-1/mut/M2-corrected-first-failure.json)、[M3](../../../target/round70/c2/attempt-1/mut/M3-first-failure.json)、[M4](../../../target/round70/c2/attempt-1/mut/M4-first-failure.json)。所有恢复 SHA 均为下表 entities 值。

| 文件（主树执行前/后与副本最终均一致） | SHA256 |
|---|---|
| [R70 测试](../../../crates/mir-client/tests/round70_effect_blend_offset.rs)（1140 行 / 45215B） | `b188febc6a67e26ee14a815672db0d597bcc7e054423a5c103d9b41bddc1df84` |
| [frame](../../../crates/mir-render/src/frame.rs) | `b2b186430fd706d7a9e6655a42e6ad5484d1b4e237b77a001650db2dbce29aee` |
| [entities](../../../crates/mir-render/src/entities.rs) | `f8dff727ee970fbe2dc4c8d55f06c7a2587cf84df134c7c4e08a670bc8a7afc6` |
| [scene](../../../crates/mir-render/src/scene.rs) | `d570258f7139a7adaf2cb26f245e68bbdc56248d787d7a95b612f68f3125db98` |

- 与 [B](../../../target/round70/b/attempt-1/manifest.json) 的 9/2/2/133、四变异首败行和左右值一致；B 基于 `72d9313`，本轮基于 `b11deac`、全新 target 且无 debug。M2 本轮采用实际帧宽高字段，B 使用固定范围 4096；M3 本轮在 add helper 内改 blit，B 改 dispatch，目标语义相同。与 [A](../../../target/round70/a/attempt-1/manifest.json) 的生产 SHA 一致；A 原 lib 132，本轮含 B 新增 alpha 测试为 133。
- 本追加报告关闭原 [C 报告](round70-c-evidence.md) 的独立执行、兼容执行及四变异执行缺口；未宣称完整 Crystal DrawBlend、INVLIGHT/rate、真实素材/GPU 验收。
- 范围：仅写本报告与 `target/round70/c2/**`；未改主树实现/测试/冻结产物，未 commit/push，未跑 `tools/dev/ci.ps1`。
