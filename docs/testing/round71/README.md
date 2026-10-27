# R71：149 FireWall 逐格精灵映射与锚点迁移

阶段：**pre-audit，待独立终审，不代表 R71 全量通过或 Crystal 生产等价**。本批仅文档；[契约](round71-firewall-contract.md)、[清单](final-manifest.json)、[外置摘要](manifest.sha256)。

范围：仅149来源 ObjectSpell FireWall → Magic1630–1635、6×120ms elapsed循环、格锚点+当帧signed offset一次、复用R70 rate=1 Add。非149旧行为保留，1620起手不迁。

Crystal基准（均实读）：[SpellObject:30/49](../../../github/Crystal/Client/MirObjects/SpellObject.cs#L49)读取149位置、Repeat=true、Magic1630/6帧/120ms/Light3/Blend；[288–302](../../../github/Crystal/Client/MirObjects/SpellObject.cs#L302)初始100ms取整、每Process最多一步，实际为 `NextMotion = CMain.Time + FrameInterval`。
[323–339](../../../github/Crystal/Client/MirObjects/SpellObject.cs#L323)格坐标DrawLocation及DrawBlend(true,0.8F)；[MLibrary:692–709](../../../github/Crystal/Client/MirGraphics/MLibrary.cs#L692)先加帧offset后裁剪、保存恢复bool；[DXManager:378–384](../../../github/Crystal/Client/MirGraphics/DXManager.cs#L378)NORMAL=SourceAlpha/One，BlendFactor不证明0.8等价；[PlayerObject:1802](../../../github/Crystal/Client/MirObjects/PlayerObject.cs#L1802)的1620是施法者起手，[3057](../../../github/Crystal/Client/MirObjects/PlayerObject.cs#L3057)完成分支仅声音。

|交付/责任|实录与边界|
|---|---|
|[生产分支](../../../crates/mir-client/src/ui/spellfx.rs#L231)|cedf903；348行/14779B，新增31行；SHA d7f86c29150f51aa865c2d668786bde45c1d7eb06c0172dc06e8d2d3a82343a6。|
|[8项测试 G1–G5](../../../crates/mir-client/tests/round71_firewall_mapping.rs#L765)|1590行/59751B；SHA 64ff3f597e2530ce0d32abb21eb09b36638083384f27638a0b29177ac262b422；合成WZL/WZX+独立RGBA oracle+真实encode/decode/apply/UiState链。|
|A 最终红→绿|[red-final2](../../../target/round71/a/attempt-1/red-final2.log)编译成功、8例、2 passed/6 failed EXIT101；[green-final2](../../../target/round71/a/attempt-1/green-final2.log)8 passed EXIT0；同一最终测试SHA。|
|B 独立验证|[B清单](../../../target/round71/b/attempt-1/manifest.json)：core.autocrlf=false archive cedf903，tar SHA 589c1cf26b940e3d0d5b732804380f710d6b09f64099f5a265df54019a48ec56，1366成员、nlink=1；基线8/0/0。|
|C 独立验证|[C报告](round71-c-evidence.md)：28次Cargo=14绿各8/0/0+7编译EXIT0+7断言红EXIT101；独立py oracle四像素一致。|

五变异实录见[B清单](../../../target/round71/b/attempt-1/manifest.json)与[C对照](../../../target/round71/c/attempt-1/comparison.json)；每项前绿8/8→patch→编译EXIT0→断言RED101→恢复原字节→后绿8/8，测试SHA不变。

|变异|指定命中断言（测试行:列）|变异后生产SHA前缀|
|---|---|---|
|M1 1630→1620|G1 811:5，(384,224)诱饵色|ed250944|
|M2 `%6`→clamp|G2 frame_boundaries 909:9，age720|f05a7cbf|
|M3 恢复尺寸居中|G3 anchor 1011:5|149fada3|
|M4 offset=false|G3 offscreen 1193:5|6da9c236|
|M5 改旧Over入口|G4 1343:5，另命中G1/G2/G3/G5；旧入口同时不消费offset，非隔离混合因子|5b379b49|

B独立Python oracle：27项fail=0、covered=45；整帧SHA 252a30f71d7682b5ffd9ac33f60fd7263c666e1680765d6f0abf932f2c03ea6c，Python/测试expected/生产actual三源一致。M5的Add结论另由G4背景饱和与公式支撑。

C原报告为**BLOCK，实现/测试必改0、证据必改E1**：A中间c5c7d8两日志缺失且不可恢复，仅SHA残留。现行索引必须并读[A原清单:64/76](../../../target/round71/a/attempt-1/manifest.md#L64)+[A勘误](round71-a-erratum.md)+[Lead裁定](../../process/round71-lead-ruling.md)+[门下证据裁决](../../process/round71-menxia-evidence-ruling.md)，原清单保留作错误历史，不裸引其完整保全声称。
[C2追加](round71-c-addendum.md)为PASS-WITH-PARTIAL、实现/测试/证据补救必改0，E1补救CLOSED，**C7历史保全永久PARTIAL**；原C BLOCK及初稿历史保留。字面无feature命令EXIT101、0/8/0是夹具拒绝；加 `--features mir-render/legacy` 后8/0/0 EXIT0，非功能断言红。C2独立归档副本，fmt/clippy均EXIT0。

门禁唯一attempt-1：HEAD=`16644d28df142a0b99264c9700674ff701fdea97`，EXIT0、12/12；[原日志](../../../target/round71/gate/attempt-1.log)212193B，SHA d1fcbf77dfe7751eefeaf03b5a6f8af70e564eacf94a8fc15b10789d2b2c8173；[meta](../../../target/round71/gate/attempt-1.meta.txt)476B，SHA 0d5478e7423655777957576b9b5f0c5d8d5ea3af2a44e31329e175ffe3972cfd，门禁后补全，实测12字段（任务输入称13，按实物披露）。
提交链：`cedf903`（2文件+1621）→`16644d2`（C初稿PASS，门禁HEAD）→`adf63c1`（C BLOCK/勘误/Lead及门下裁定/C2门禁后文档）。不称最终C BLOCK及裁定已被门禁HEAD覆盖。相对cedf903的crates/tools diff均空；两份历史freeze未改。

PARTIAL台账承接[计划132–138原号](../../process/round71-plan.md#L132)：

|原号|仍未闭合|
|---|---|
|R71-C7|c5c7d8中间日志永久缺口；E1关闭不等于C7满足。|
|GAP-5 / R70-C1|非149旧行为、1620起手/其他spellfx/UI调用点、真实素材与设备视觉签核。|
|R70-C3|其他技能/全局尺寸、阴影/高度/居中基准。|
|GAP-1/2 / R70-C2|实际rate=1；rate0.8及src×0.8×alpha近似未实施；DrawColour/INVLIGHT/完整DrawBlend/公共API全面收敛。|
|GAP-3/4/6|mask/tint/shadow绘制（shadow只解析不消费）、场景条件offset/Light3。|
|本切片时钟/对象|严格NextMotion、初始100ms取整、每Process一步、149持久场/26移除；4000ms仅瞬态窗口。|
|R68 / R69条件5 / R49–R70|历史归档/措辞、R69 a77永久缺口、R49–R67及R61–R67原号、全UI/设备/跨后端承接。|

D验证与完整命令/UTC/cwd/EXIT见[D证据](../../../target/round71/d/attempt-1/verification.json)：JSON parse、逐项bytes/SHA复算、外置SHA、链接检查与diff卫生；不运行Cargo或全量CI。最终独立终审/尚书验收/指定门下确认仍待实际履责。
