# R71 FireWall 有限切片契约

阶段：pre-audit；待独立终审，不代表 R71 全量通过或 Crystal 生产等价。[结果索引](README.md)、[清单](final-manifest.json)。

|契约|当前生产行为|
|---|---|
|准入|仅 `event.spell == Spell::FireWall && event.from_object_spell`；真实149解码来源，其他来源不迁移。|
|窗口|`age=now_ms.saturating_sub(started_ms)`；仅 `0≤age<4000`；负age/4000及i64极值安全跳过。4000ms不是Crystal对象寿命。|
|phase|`(age / 120) % 6`，Magic索引1630+phase；0/119→1630、120→1631、599→1634、600/719→1635、720→1630、840→1631、3999→1633。|
|锚点|`world_to_screen(center,event.location)`格原点；不调用effect_size、不按宽高再居中。|
|offset|`draw_effect_blend(...,true)`读取当前帧signed offset，x/y分别saturating_add一次后裁剪；调用方不预加、不跨帧累积。|
|逐格|一条149一张当前帧；服务端五条独立object_id生成十字，客户端不再扩散；绘制后continue，不重复画通用精灵。|
|Add|复用R70 rate=1；逐RGB `min(255,dst+(src*alpha+127)/255)`，保留既有渲染算法；非黑背景及饱和由G4校验。|
|缺帧|缺Magic/目标帧返回false；当前图不画、不退0/1620/邻帧、不panic；continue仅跳当前事件，后续仍绘制；有图完全屏外返回true。|
|legacy|保留合成WZL/WZX加载所需legacy feature；既有色块/文字叠层保持。|
|非149|FireWall及其他技能继续通用age/60 clamp0..9、尺寸居中和旧Over；Bolt/Magic选择保持。125/570/wire/net/server/渲染算法未改；1620起手未迁。|

来源实读：[生产分支252–297](../../../crates/mir-client/src/ui/spellfx.rs#L252)；[Crystal FireWall49–56](../../../github/Crystal/Client/MirObjects/SpellObject.cs#L49)、[格锚点323](../../../github/Crystal/Client/MirObjects/SpellObject.cs#L323)、[DrawBlend335](../../../github/Crystal/Client/MirObjects/SpellObject.cs#L335)、[MLibrary offset699/裁剪701](../../../github/Crystal/Client/MirGraphics/MLibrary.cs#L699)。

|验收组（合计8个命名测试）|证据内容|
|---|---|
|G1 `r71_g1_wire_mapping`|合成可读帧、真实encode→decode149→apply→UiState→公开draw_sprites；五格各一图；Magic0/1620/Bolt诱饵不出现。|
|G2 `r71_g2_frame_boundaries` / `r71_g2_age_window`|逐边界整帧oracle、720回卷、3999尾帧、未来/4000/i64极值。|
|G3 `r71_g3_anchor_offsets` / `r71_g3_offscreen_crop`|跨帧尺寸与signed offset变化、重复draw/换center、拒绝居中/双加；屏外原点经offset移入后裁剪。|
|G4 `r71_g4_add_per_cell`|非黑背景、RGB饱和、五格叠加与邻格空白；独立RGBA算式。|
|G5 `r71_g5_legacy_sources` / `r71_g5_missing_frame`|非149 FireWall/雷电/毒云旧整帧金样；缺1631不退0，后续事件仍绘制，空库安全。|

[8项测试](../../../crates/mir-client/tests/round71_firewall_mapping.rs#L765)在最终同SHA下A红2/6 EXIT101→绿8/0 EXIT0；B/C独立五变异前绿→编译0→断言红101→恢复绿8/8。M5旧Over入口同时不消费offset，不能单凭其红隔离混合因子；Add另有公式/G4/oracle支撑。
标准定向命令：`cargo test -p mir-client --features mir-render/legacy --test round71_firewall_mapping -- --test-threads=1`。C2字面省feature命令0/8/0 EXIT101，仅为夹具拒绝，加入feature后8/0/0 EXIT0；D不重跑Cargo。

边界承接[计划132–138](../../process/round71-plan.md#L132)：实际rate=1；rate0.8及src×0.8×alpha近似未实施。Crystal [DXManager378–384](../../../github/Crystal/Client/MirGraphics/DXManager.cs#L378)默认SourceAlpha/One与BlendFactor赋值不证明完整等价，DrawColour/INVLIGHT/完整DrawBlend仍PARTIAL。
严格时钟仍PARTIAL：[Crystal288–302](../../../github/Crystal/Client/MirObjects/SpellObject.cs#L288)初始100ms取整、每Process最多一步，实际 `NextMotion = CMain.Time + FrameInterval`；elapsed允许跳帧，不能当严格迁移。149持久场/26移除、Light3、真实素材视觉、全UI/设备/跨后端未验收。
GAP-3/4/6的mask/tint/shadow（只解析不消费）、场景offset/照明与R70-C3全局尺寸高度居中继续PARTIAL；非149/1620/其他调用点承接GAP-5/R70-C1；R49–R70历史原号不清零。
证据链：[C原BLOCK](round71-c-evidence.md)→[勘误](round71-a-erratum.md)+[Lead裁定](../../process/round71-lead-ruling.md)+[门下裁决](../../process/round71-menxia-evidence-ruling.md)→[C2 PASS-WITH-PARTIAL](round71-c-addendum.md)；E1补救CLOSED，C7永久PARTIAL，R69条件5同样永久PARTIAL。
