# R73-C2 Lead 裁定收敛补记（2026-10-27）

- 审计对象：HEAD `93e11407fad3da5fb08b030781058e216ccf8232`（93e1140）。
- impl SHA256 `e0bcaef0ff6a6103079e5cdb3d31f16e2a2db918b3ae390babd7290d00d3fc1b`；测试 SHA256 `1ec3f6dbbf7368e7dbd82c78529f1e6605908b04c11164ba860ab28b42513c5e`。
- **verdict：PASS-WITH-PARTIAL；必改 0**；实现功能缺陷0，PARTIAL不清零。
- 裁定：`docs/process/round73-lead-ruling.md`；SHA256 `7927d0332f6e9fff20112542c0e460c7c91d8a4fb500804af57776ec17d9aa5c`。

|依据|核对与定位|
|---|---|
|裁定1：M3/M5 CLOSED|C精确实跑为权威；原报告初读 :35–36，追加前因他人改写变为 :38–39。M3仅删continue：7/2/0 EXIT101，G2:1413 age400 (415,255) [248,0,248,255]→[0,0,0,255]；M5仅技能条件：8/1/0 EXIT101，G5:2127 (292,190) [0,252,0,255]→[0,0,0,255]；均build0、恢复原SHA后绿9。|
|B辅助/扩展溯源|`target/round73/b/attempt-2/PROVENANCE-M3-M5.md`，12行，SHA256 `8c2954c518b714f88b97d14b859b601453c69d84cd193ed860336d5e4e5a81af`；`manifest.md:65–69`已标注并交叉引用C报告；15诱饵要求不采纳，以400ms泄漏frame14为最小断言。|
|裁定2：MINOR非阻断|spellfx.rs:309注释应为Time>NextFrame才推进，R74承接；实现elapsed正确，严格Crystal时钟仍PARTIAL。|
|裁定3：其余结论采纳|原报告独立复跑9/0/0、29/0/0、lib4/0/0、clippy/fmt EXIT0；M1/M4红绿；oracle8/8；范围/新增allow=0/两freeze一致。|
|历史BLOCK记录保留|初读49行 :4为BLOCK（证据/契约项；功能缺陷0），SHA256 `a4d9180c7ea3a310ef8cca672d5ecc41a482e1916b6fb57774d466a3f0566d1e`；M3/M5历史发现保留，不把B扩展冒称原契约。|

- 不一致披露：C2初读后、追加前报告被并发修改为52行（:4/:12/:17及新增:19–20）；首次追加被文件冲突拒绝。C2已告知Lead，保留他人全文，仅在其后追加；未擅改裁定。
- PARTIAL原号承接：GAP-5/R70-C1、R70-C3；R71 strict NextMotion/初始100ms/每Process一步/149持久场/26移除/寿命/4000窗；R73-T1严格等号/跳帧/ActionFeed/回执起算/owner跟随/Light6/声音。
- GAP-1/2/R70-C2 rate0.8、src×0.8×alpha、DrawColour/INVLIGHT/DrawBlend/API；GAP-3/4/6 Light3/mask/tint/shadow/场景offset/AnimationOffset；R72 B plan-M4/legacy lint。
- R71-C7/E1/R69条件5永久历史缺口；R68/R49–R70/R61–R67真实素材/GPU/设备/全UI/跨后端/Live音频继续保留，E1仅补救CLOSED；B2-D1..5承接不变；注释勘误由R74承接。
- 范围：C2只追加证据主报告、新建本补记及 `target/round73/c2/attempt-1/**`；未改主树实现/测试/其它文档，未commit/push，未运行ci.ps1；不重复跑既有功能测试。
- 本文件及主报告最终行数/bytes/SHA256统一见 `target/round73/c2/attempt-1/sha256.txt`（避免文件自引用哈希）；CLI命令/EXIT与核验记录见同目录 `runs.json`。
