# R73 Lead 裁定（C 审计后、功能提交前）

- 主单 `JJC-20261027-010`；基线 `93e1140`；交付：`crates/mir-client/src/ui/spellfx.rs`（impl `e0bcaef0ff6a6103079e5cdb3d31f16e2a2db918b3ae390babd7290d00d3fc1b`，21862B，+54/−0）+ `crates/mir-client/tests/round73_thunderbolt_hit_mapping.rs`（测试 `1ec3f6dbbf7368e7dbd82c78529f1e6605908b04c11164ba860ab28b42513c5e`，84910B，9 测试）。
- C 审计（`docs/testing/round73/round73-c-evidence.md`，49 行，verdict BLOCK：实现功能缺陷 0、证据/契约为阻塞项）。

## 裁定 1：M3/M5 变异口径 → CLOSED（以 C 精确契约实跑为权威，B 版本为辅助扩展）

- M3 原契约「仅删除 `age>=400 continue`」；最终测试 `MAIN_MAGIC2`（:152–170）无 15 帧诱饵，故以「clamp(4) 保留、400ms 泄漏 frame14 尾帧」为最小真实目标断言。C 独立实跑：build EXIT0 → RED `7/2/0` EXIT101，首败 G2:1413 age400 (415,255) actual `[248,0,248,255]` expected `[0,0,0,255]`，恢复 `e0bcaef0…/1ec3f6db…` 后绿 9/0/0。→ 权威红证据成立。
- M5 原契约「准入仅留技能条件（同时去 `!from_object_spell` 与 `event.param`）」；C 独立实跑：build EXIT0 → RED `8/1/0` EXIT101，首败 G5:2127 120/cast=false (292,190) actual `[0,252,0,255]` expected `[0,0,0,255]`，恢复后绿 9/0/0。→ 权威红证据成立。
- B attempt-2 的 M3（额外 `min(4)→min(5)` + 测试副本 index15 诱饵、测试 SHA `7728bd30…` 仅存在于隔离副本）与 M5（仅去来源、保留 param）判为**辅助/扩展变异**：有效红证据保留，但不得冒称原契约通过；B 须在 `target/round73/b/attempt-2/PROVENANCE-M3-M5.md` + `manifest.md` 附节如实标注并交叉引用 C 报告（已在执行）。
- 本项不计为功能缺陷；要求 A 加 15 诱饵并重冻测试**不采纳**（成本高于收益且无行为缺陷），缺口如实登记为「门下 M3 15 诱饵要求与最终测试夹具存在偏差 → 以 400ms 泄漏 14 为最小断言」。

## 裁定 2：`spellfx.rs:309` 注释表述错误 → MINOR 文档勘误，非阻断，R74 承接

- 注释写「`CMain.Time <= NextFrame` 才推一帧」；Crystal `Effect.cs:81` 实为 `<=` 时 return、`>` 才推进。实现 elapsed 语义正确（严格 Crystal NextFrame 时钟仍为 PARTIAL）。
- 不在本轮回改源码：避免再次变更生产 SHA 重置 A/B/C 全链红绿证据。C 报告已登记、D 文档不得复制错误表述、R74 承接修复。T 终审须按本裁定检查「披露充分性」而非「是否已改」。

## 裁定 3：其余 C 结论采纳

- 独立复跑 9/0/0、29/0/0、lib 4/0/0、clippy/fmt EXIT0；M1/M4 独立真红+恢复绿；Python 抽核 8/8 三源一致；范围/allow=0/freeze 卫生 PASS。
- 明确：本切片的「新增 ignore/allow=0」已满足（worktree allow 3 = 基线 3）；「148」不存在属需求侧笔误，实际矩阵 120/123/149 × ThunderBolt/Lightning × cast/param。
- 待办：C2 追加报告按本裁定把 verdict 收敛为 PASS-WITH-PARTIAL（必改 0）后，Lead 执行功能提交 + 唯一门禁 → D 文档 → T 终审 → 尚书 → 门下 → Done。

## 裁定 4：门禁 attempt-1 失败根因＝真实素材缺 `magic2[10..14]` → 扩拆包计划（不改生产代码）

- 事实：功能提交 `6183394` 后唯一门禁 attempt-1（HEAD=6183394）EXIT=1；唯一失败项 `client_fx::attack_swing_and_spell_flash_render_with_real_assets`（`crates/mir-client/tests/client_fx.rs:116` `assert!(cache > 0, "攻击/技能特效精灵应当解码进缓存")`）。日志 `target/round73/gate/attempt-1.log` 49,146B sha256 `d1cbb67e22f2d4e2f6d15266f72e9fbf3d2099e2010397a2deca4583e8f328f3`，UTC 15:14:44–15:15:53Z。
- 根因：真实素材 `game-assets/gfx/sprites/effect/magic2/` 仅拆了 `[0..2]+[20..29]`（`tools/assets/analyze_usage.py` `MAGIC2_FRAMES`，R59 D3 口径）；R73 新分支按 Crystal 播 `Magic2[10..14]`，真实素材缺帧 → `draw_effect_blend` 返回 false、不缓存，旧测试仍靠通用分支取 cache>0 的假设被 149 命中路径取代。
- 修复（素材侧，不改生产代码/不动实现与测试 SHA）：`tools/assets/analyze_usage.py` `MAGIC2_FRAMES` 增 `range(10,15)`（`10..14` 来自 `resource/data/magic2.wzl`，1480 帧内非空 11 帧）；重生成 `target/used-assets.json` → `mir-tools assets unpack --plan`（仅 magic2 子计划）→ 重拆 `game-assets/gfx/sprites/effect/magic2/`；旧 `[0..2]+[20..29]` 元数据/PNG 与旧索引逐项一致（overlap identical 0-2/20-29，新增 10-14，23..29 仍 empty）。`game-assets/` 不入库（`gitignore:27`），证据落 `target/round73/asset-fix/**`。
- 影响：A/B/C 证据（impl `e0bcaef0…`、测试 `1ec3f6db…`）保持有效；门禁需以修复后的工作树（素材）在提交 `fix(assets)` 的新 HEAD 上重跑 attempt-2。D/T 须披露：attempt-1 失败、素材扩帧、`game-assets` 未入库、扩帧对 `client_fx` 的影响。
- 记录：Lead，2026-10-27T15:29Z（UTC）。

记录：Lead，2026-10-27T15:12Z（UTC；本机 23:12+08）
