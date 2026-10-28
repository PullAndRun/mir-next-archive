# R76 trap-contract：149 困魔咒有限映射合同

- 主单 JJC-20261028-003；阶段pre-audit，PASS-WITH-PARTIAL；[索引与门禁历史](README.md)、[计划](../../process/round76-plan.md)、[门下](../../process/round76-menxia-review.md)、[Lead裁定](../../process/round76-lead-ruling.md)共同限定本切片。
- Crystal实读：SpellObject.cs:30–38记录Location/Repeat；:42–48给Magic/1390/100ms/10帧/Blend；:288–302是首100ms量化、`>=`、每Process一步及Repeat回卷，elapsed不是此时钟。
- Crystal :323–339为格投影/移动偏移及DrawBlend(true,0.8F)，:363–370的AnimationOffset先加格再乘格尺寸；MLibrary.cs:692–709先offset后裁剪；DXManager.cs:353–384 NORMAL为SourceAlpha/One，BlendFactor不能推出src×0.8。
- Rust `spellfx.rs:331–340`：仅`event.spell == Spell::TrapHexagon && event.from_object_spell`，param两值准入；age用saturating_sub，`0<=age<4000`；`phase=(age/100)%10`、`index=1390+phase`，不是`.min(9)`。
- 格锚点`world_to_screen(center,event.location)`，`draw_effect_blend(frame,FxKind::Magic,index,x,y,true)`；当帧signed offset一次、CPU rate1 Add；不取尺寸居中、不预加offset、不铺额外格。
- 分支在公共duration过滤前；负age/age>=4000直接continue，缺帧不fallback且后续事件继续；4000ms是149 Trap从700ms改变的新产品精灵窗，非Crystal寿命。
- 公共duration_ms(TrapHexagon)、非149及文字仍700ms；非149困魔仍旧60ms通用居中Over；其他技能不随本分支迁移，其余26技能GAP-5原号照承。

|G|冻结测试合同与有效证据|
|---|---|
|G1 wire_mapping|真实149 encode/decode/apply/snapshot、event.id/object_id/位置/来源/两param、错库异色整帧；A基线首败:1051。|
|G2 frames_and_wrap|十帧及80/99/100、100k/100k+99、1000回1390、3999到1399；不是夹死末帧。|
|G3 window_and_source|负age/0/699/700/999/1000/3999/4000/4001、极值；公共/文字700；真实120/123来源控制。|
|G4 anchor_offset_crop|异尺寸signed offset、变center、重复查询、屏外移入/裁剪，格锚+offset恰一次。|
|G5 add_and_missing|非黑底/饱和/透明/同格叠加；RGB=min(255,dst+(src*a+127)/255)，a0保dst，否则alpha255；缺帧继续。|
|G6 ui_entry|UiState.sync→公开ui::draw、避HUD/文字ROI与独立Add金样及直接draw一致。|
|G7 legacy_and_query|其他来源/技能固定金样、重复draw与events/fields不变；R72迁移后9绿。|
|G8 asset_plan|真实analyze_usage入口计划含1390..1399、旧magic为子集且Magic2不变；Python正常退出的缺帧变异须断言红。|

- 批准M1–M7以计划:96–102为准：Magic→Swing；100→80；10→9；offset false；Add→Over；删from_object_spell；删素材十帧。测试头注的错位编号不替代批准定义。
- B七链最终前绿8→no-run0→指定G红101→恢复SHA→后绿8；C原M1–M6有效，原M7无效历史保全，C2补M7 Python0/no-run0/G8红101/恢复8绿，合并C独立7/7 CLOSED。
- R72 supersedes仅G5的149 Trap旧通用像素要求：原:1383/:1404注释、:1408/:1429枚举TrapHexagon→Reincarnation，另增顶注；原expected/fixture/其余断言保持。原65,322B/SHA `58f64ab2d30b69f3aea5c07a8975e988ca842bce45972499a6f93e120958baea`保全。
- R72现66,437B/SHA `3a9f42bd67a8e57e02d86fb970ad6347687d46991b313c3c7fb2316db97b4bfb`；删顶注3..15行后65,330B/`c95dd819…`、正文恰4增4删。旧sidecar中间稿66,439B/`52fe25…`错误不覆写，裁定1追加勘误闭合且流程PARTIAL。
- R59 supersedes仅冻结集106→116、规范序追加1390..1399、TRAP_BASE_INDEX=1390/TRAP_FRAMES=10及计数/指纹/说明；保留六测试语义、四样例与WZL哈希。旧585行/19,447B/`2cfff50c…`保全，新600行/20,596B/`849770b2…`。
- 116整集指纹 `e642330a568402b21528afb04459108f3dc596f7e37ad5c276670ef342940eff`；106旧子集 `e465026a50ee6f86d6762e3959386fa0132ffabf63da9c92afeebdf3d52a1e98`不变；历史R59/R72证据不回改。
- 门禁attempt-1 HEAD9bf6ec6 EXIT1/R59三败→裁定5 23cb621→fix41425b1→attempt-2 HEAD41425b1 EXIT0/12项；`9bf6ec6..41425b1`含R59+裁定两文件，仅fix单提交只有R59。
- attempt-2 UTC 2026-10-28T00:28:12–00:31:25Z，log215,013B/`85bdd3e6b5b725dd52d32ec9d0e43f038318f09c2a2208e7d674cc421afb1027`，meta802B/`71c9ca8c…`；内部workspace测试明确skip duo_/fault_pg。
- 素材为本地gitignore的game-assets，不入git；真实十帧探针不背书GPU/设备视觉；A R59另修本地written_png106→116，其首跑失败日志覆盖损失须继续披露。
- 环境：debug D8050以CARGO_PROFILE_DEV_DEBUG=0规避；独立R76/R72复跑带mir-render/legacy；legacy clippy既有lint101与官方workspace clippy0并列，不冒充全绿。
- PARTIAL原号：GAP-1/2/R70-C2（完整DrawBlend/rate/Mode与src×0.8未实施）；GAP-3/4/6（AnimationOffset/Light/DrawColour/mask/tint/shadow消费）；GAP-5/R70-C1其余26技能；R70-C3全局锚点/居中。
- PARTIAL原号：R71时钟/对象、R73-T1（严格NextMotion/Repeat/26寿命/owner/ActionFeed）；R72 B plan-M4/legacy lint；R71-C7永久、历史E1仅补救CLOSED、R69条件5；R68/R49–R70/R61–R67、R74 artifacts34；R75-E1/C-M1、R75-E2/C-M2。
- 流程PARTIAL：A sidecar瑕疵、B早期失败日志永久损失（裁定2）；C原M7中途无效红历史留存（C2已补）。合同不关闭其他历史项目，不称整轮通过/严格Crystal等价/生产寿命已修。
