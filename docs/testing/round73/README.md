# R73：非149雷电术命中五帧映射

- 主单 `JJC-20261027-010`；基线 `93e1140`；DAG：A兵部 → B工部 → C刑部 → Lead提交/门禁 → D礼部。
- 阶段：**独立终审完成：PASS-WITH-PARTIAL（首轮 BLOCK→T-D1 清单字段修复→T2 复审 CLOSED，必改 0）**；`stage=post-audit`；尚书验收/附录门禁结论见终审两份报告；本页不代替整轮验收或尚书验收。
- 提交链：`6183394`（feat(client): R73 非149雷电术命中五帧逐格映射（第73轮），2文件 +2318）→ `3378303`（fix(assets): magic2 拆包补雷电命中 10..14，修复 R73 门禁真实素材缺帧）。
- HEAD快照 `3378303`；D只写六份授权文档及 `target/round73/d/attempt-1/**` 自检证据，不提交、不推送、不重跑全量门禁。

## 范围与契约

- 仅 ThunderBolt、非149/非ObjectSpell、Cast=true（`param=true && !from_object_spell`）命中分支；120/123成功施法来源与149来源严格区分，Lightning/失败施法维持旧路径。
- Magic2 `10+phase`，`phase=(age/80).min(4)`，五帧×80ms；`age>=400ms` 隐藏命中精灵。格锚点 `world_to_screen(center, event.location)`，`draw_effect_blend(FxKind::Swing, apply_offset=true)`，当帧signed offset一次、rate1 CPU Add。
- 详见[命中契约](round73-thunder-contract.md)、[client §5.38](../../client.md#538-r73非149雷电术命中五帧映射)、[路线图](../../roadmap.md#二十一第73轮非149雷电命中切片)。
- Crystal雷电依据：`Client/MirObjects/PlayerObject.cs:2956–2963`（Magic2/10/5/400ms）、`Effect.cs:38–61/81–96/113–130`（时钟、落点）、`MirGraphics/MLibrary.cs:692–709`（offset后裁剪）、`DXManager.cs:353–384`（Blend）。
- 输入锚点勘误：`SpellObject.cs:57–64` 是R72毒云（1650/20/120ms/Light3），不能当作雷电10/5/80依据；`:323–339` 仅作149格锚点/DrawBlend继承参照。雷电Light6、目标owner跟随仍属PARTIAL。

## A/B/C 与 Lead 裁定

- [计划](../../process/round73-plan.md)、[门下审议](../../process/round73-menxia-review.md)、[尚书派发](../../process/round73-shangshu-dispatch.md)、[Lead四节裁定](../../process/round73-lead-ruling.md)构成审计链。
- A（`bingbu-thunder-a`）：[attempt-1](../../../target/round73/a/attempt-1/manifest.md)；archive基线+最终测试的物理红副本（独立target/TEMP、nlink=1、debug=0），红0/9 EXIT101 → 绿9/0/0，同最终测试SHA。首败 g1_ui_draw:1226，(381,261) actual `[0,0,0,255]` / expected `[248,0,0,255]`（C复核日志记:1239，位置以各原日志为准）。
- A回归29（R70 9+R71 8+R63 3+新9）及lib4、clippy/fmt EXIT0；新增ignore/allow=0，allow=基线3。
- B（`gongbu-thunder-b`）：[attempt-1](../../../target/round73/b/attempt-1/manifest.md)（184文件）与[attempt-2](../../../target/round73/b/attempt-2/manifest.md)；基线9绿、五变异build0 → RED101 → 恢复绿，回归29；独立bundled Python自解WZX/WZL、RGB565/Add/Over，63/63 actual/expected/Python三源一致。
- M1 Swing→Bolt、M2 80→60、M4 offset=false；M3/M5权威定义及实跑以C为准，B扩展差异见[溯源](../../../target/round73/b/attempt-2/PROVENANCE-M3-M5.md)。实际矩阵120/123/149；148不存在；A曾有+1 cast allow已删除，改为 `u32::try_from(phase).unwrap_or(0)`。
- [C主证据](round73-c-evidence.md)保留BLOCK历史与收敛，[C补记](round73-c-addendum.md)：**PASS-WITH-PARTIAL、必改0**；独立复跑含t9无关项、oracle抽核8/8。
- 裁定：M3仅删截止、clamp4保留，400ms泄漏frame14为最小断言；M5准入仅留技能条件；两项C精确红证据CLOSED。B扩展只作辅助；15诱饵要求与最终夹具偏差明示，不重冻测试。
- `spellfx.rs:309` 注释把 `Time<=NextFrame` 写成推进条件，实际应为 `Time>NextFrame` 才推进；MINOR非阻断，R74勘误，不回改生产SHA。

## Lead 门禁尝试历史与素材修复

|尝试|HEAD / EXIT|事实与证据|
|---|---|---|
|attempt-1|6183394 / 1|UTC 15:14:44–15:15:53Z；唯一失败 `client_fx::attack_swing_and_spell_flash_render_with_real_assets`，`client_fx.rs:116` cache>0；[日志](../../../target/round73/gate/attempt-1.log) 49,146B，SHA `d1cbb67e22f2d4e2f6d15266f72e9fbf3d2099e2010397a2deca4583e8f328f3`。|
|attempt-2|3378303 / 1|外部PostgreSQL连接池瞬时超时，`pg_relogin_with_skill_quest_gold_gets_user_information` / `pool timed out`；[日志](../../../target/round73/gate/attempt-2.log) 182,653B，SHA `fec31da6a4e2500af17fef54c6ed4fd0e025de593577f0ec11a966ebcfb966b2`。|
|attempt-3|3378303 / 0|唯一成功全量结果：12/12 OK、0 FAIL；UTC 2026-10-27T15:28:00–15:30:18Z；[日志](../../../target/round73/gate/attempt-3.log) 213,076B，SHA `676b9d4f36a9a5ef59c47365eac890d42463d195c80532a6d12944ef49beddac`；[meta](../../../target/round73/gate/attempt-3.meta.txt)。|

- 门禁由Lead执行 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`；attempt-2/3同命令，失败原日志保留；D只引用上述结果。
- attempt-1根因：`MAGIC2_FRAMES`仅 `[0..2]+[20..29]`，新分支读取Magic2[10..14]缺帧。`3378303`扩为 `[0..2]+[10..14]+[20..29]`，重生成 `target/used-assets.json`，仅重拆magic2。
- [素材证据](../../../target/round73/asset-fix/README.md)含PNG/index SHA：旧0–2/20–29逐项一致，新增10–14；index count18/written11/missing23–29 empty。`game-assets/` 被 `.gitignore:27` 排除，扩帧仅本地落地、不入库；实现和测试SHA均未改。合成证据与门禁不构成人工视觉签核。

## PARTIAL 原号承接

- GAP-5 / R70-C1：仅非149成功ThunderBolt静态命中子项；起手20/3/300ms、Lightning、1620/其他调用点、Cast=false旧闪光仍保留；149域其余26技能不机械减一。
- R70-C3：全局尺寸/高度/居中、owner DrawLocation锚点仍PARTIAL。
- R71时钟/对象偏差：strict NextMotion、初始100ms取整、每Process一步、149持久场/26移除/对象寿命、4000ms产品窗继续承接；本分支elapsed 80ms/400ms是产品近似。
- R73-T1：严格等号/跳帧、ActionFeed/施法动画结束触发、回执起算、owner跟随、Light6/声音未迁移。
- GAP-1/2 / R70-C2：仅rate1 Add；rate0.8及 `src*0.8*alpha` 近似未实施，DrawColour/INVLIGHT/完整DrawBlend/API收敛未闭合。
- GAP-3/4/6：Light3、mask/tint/shadow、场景offset/AnimationOffset及Light6未迁移。
- R72 B plan-M4残余 / legacy lint：历史近似居中及带legacy lint债继续披露；R71-C7 / E1 / R69条件5：E1补救CLOSED，C7及历史原红/隔离/日志缺口永久PARTIAL。
- R68 / R49–R70 / R61–R67：真实素材/GPU/设备视觉、全UI/跨后端/Live音频及更早原号不清零；`spellfx.rs:309` 注释勘误交R74，素材本地扩帧不等于视觉签核。

## 清单与D自检

- [final-manifest.json](final-manifest.json)为post-audit（35 artifacts：33 交付+终审报告+复审附录）；生产/测试使用 `3378303` git blob（与 `6183394` 同内容），其余逐工作树/受限提交 blob bytes/SHA冻结；[外置摘要](manifest.sha256)不作自引用。
- 终审证据：[终审报告](round73-final-audit.md)（首轮 BLOCK，T-D1 已修复）、[复审附录](round73-final-audit-addendum.md)（PASS-WITH-PARTIAL、T-D1 CLOSED）；证据根 `target/round73/{t,t2}/attempt-1/`。
- D证据目录：[attempt-1](../../../target/round73/d/attempt-1/)；执行链接检查、diff空白检查、JSON解析/外置SHA/逐项bytes和SHA复算、`git diff 3378303 -- crates`空检查；以该目录实际EXIT和结果为准。
