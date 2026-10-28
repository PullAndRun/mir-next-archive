# R76 刑部独立终审

- **裁定：PASS-WITH-PARTIAL；必改 0，可推进尚书验收。** 主单 JJC-20261028-003；终审基线 HEAD=`72fb0a4a04a3f93004b21cd5b4930e2b4343a0de`，不据此宣称整轮通过或 Done。
- 范围：本轮T执行隔离复跑并留证 `target/round76/t/attempt-1/**`；T2仅修改本报告与 `target/round76/t2/attempt-1/**`，未改实现、历史测试、交付清单/素材及既有证据，未 commit/push，未重跑 `tools/dev/ci.ps1`。

| 核验项 | 裁定 | 独立实读/复算结果 |
|---|---|---|
| 提交/范围 | PASS | 实读 log -12 与 `96e46cc..HEAD`；96e46cc 是 R75 规划起点而非收口，a5161f8 是 R76 plan SHA 前缀而非提交。R76 功能9bf6ec6为5文件；23cb621裁定→41425b1单文件R59 fix→02c3ea8审计→b743c23文档→72fb0a4清单。 |
| 门禁后源代码 | PASS | `git diff 41425b1..HEAD -- crates tools` 为0B；后续只文档/审计，未以新代码套用旧门禁。 |
| 清单 | PASS | 24 artifacts=19 file+5 inventory；19 file逐项bytes/SHA全部一致，17个带git_blob项均与delivery_head=b743c238050cea5dee89e9f4c6e99e1fb2de11cb及当前字节一致；无自引用。 |
| 清单摘要/目录 | PASS-WITH-PARTIAL（抽样） | manifest=7336B/4ac0665b101936b21607017e6aac20bdcc65d8de12c02653526bc110017b79a8；外置87B摘要匹配。A/B/C/C2/C3目录及顶层证据分别抽查SHA/实存；五目录聚合生成器未保全，仅producer-declared，聚合SHA不可复现记流程PARTIAL，不替代实物证据。 |
| 门禁历史 | PASS | attempt-1 SHA979cc747…/69152B、EXIT1/R59三败保留；attempt-2 log=215013B/85bdd3e6b5b725dd52d32ec9d0e43f038318f09c2a2208e7d674cc421afb1027，原日志汇总12个OK；meta802B/71c9ca8c…与gate段HEAD41425b1、EXIT0、12/12、00:28:12–00:31:25Z一致。 |
| 隔离复跑 | PASS | `git -c core.autocrlf=false archive HEAD`物理副本；独立target/TEMP/TMP/TMPDIR，CARGO_PROFILE_DEV_DEBUG=0；Magic117个文件含index逐字节一致且nlink=1，必要resource/data/magic.wzl/.wzx复制。 |
| 实跑计数（本轮 T 执行） | PASS | R76 8/8、R59 6/6、R72 9/9、R75 7/7、R73 9/9、R71 8/8、R70 9/9，合计56/56、0 ignored，全部EXIT0；fmt0；官方`cargo clippy --workspace --all-targets -- -D warnings`（无legacy）0。 |
| 复跑偏差 | 披露 | R76/R72/R73/R71/R70带legacy；T首跑R75遗漏legacy，2绿5败/101（合成WZL不可读），原日志保留，追加带legacy后7/7；资源全量复制准备任务曾中止，未产生测试结论。 |
| R72/R59 supersede | PASS | R72只新增顶注及G5四处Trap→Reincarnation，expected/fixture未改；R59只常量、帧序追加1390..1399、指纹及措辞，6测试/4样例/WZL哈希不变。旧R59=19447B/2cfff50ce46a967e742b29ead5a7e794655287fa279b316335b54e290d089f34与A保全一致。 |
| 素材指纹 | PASS | 对真实116 PNG逐帧独算SHA；116整集e642330a568402b21528afb04459108f3dc596f7e37ad5c276670ef342940eff；106旧子集e465026a50ee6f86d6762e3959386fa0132ffabf63da9c92afeebdf3d52a1e98不变；本地game-assets仍不入git。 |
| 文档/链接 | PASS-WITH-PARTIAL | README/contract/client§5.41/roadmap二十四均保留有限范围、attempt-1失败/fix、本地素材与PARTIAL；R75编号不变。官方链接检查EXIT0（2165、跳过target838），另实查R76两文档21条链接含target全部存在；git diff --check EXIT0。 |

- A真红：red7原日志2绿6败/101，测试SHA=`8b680973a30193113c65e6cc936cccd6cfc8ac33941dbff43d4867419f3562d0`与当前/绿一致；:1051 `(381,261)` actual `[0,0,0,255]` / expected `[120,132,248,255]`，green8为8/8。
- B M1：patch SHA=`908cfc4255b1643f54a44626708ead6b79e73b10c2982c49b3784dd38925b85d`；:1051 `(384,256)` actual `[32,68,24,255]` / expected `[0,0,0,255]`，目标1测试红101；恢复spellfx `84fdb324…c8e0f1`，后绿8/8。
- B M4：patch SHA=`7356bfaa0c00b9b4bd7bcbbbee6a6d060cc9941abc02f7e669d8a5c1c8ef7399`；:1441 `(384,256)` actual `[120,132,248,255]` / expected `[0,0,0,255]`，目标红101；恢复同上，后绿8/8。
- B M7：patch SHA=`fa89832963612cb53bba5947536bbde910885243d90ff2209a5944fa14cd68cb`；:2248 `assert!(source.contains("range(1390, 1400)"))`，left/right=N/A，布尔false/应true；红101→恢复脚本 `e32297da…57cfe6`→8绿。C2独立同首败、Python0/no-run0、实际106/应116、缺十帧，恢复同SHA后8绿；原C无效M7不抹除。
- 真红保全：上述B patch/日志/恢复/后绿逐项重算；B sha256清单133行仅既有自引用stale，其他实物一致；完整SHA与原文见T `checks/red-check.json`，C2 patch与原/变异字节亦已独算。
- 语义：实读Crystal SpellObject.cs:42–48/:288–302/:323–339、MLibrary.cs:692–709、DXManager.cs:353–384；Rust仅149/ObjectSpell Trap条件，Magic1390+`(age/100)%10`、`0<=age<4000`，分支先于公共700ms过滤；格锚点+signed offset一次、CPU rate1 Add，缺帧continue，非149与文字仍700ms。Crystal初始量化/每Process一步/Repeat对象及DrawBlend0.8均未被本实现等价迁移。
- 瑕疵裁定：测试头M编号错位仅不执行注释且合同明确计划权威；A sidecar中间稿由只增勘误闭合；B86条早期失败日志永久损失照承；D无落盘自检证据已在README披露，T独立静态/链接/清单复核补足当前可核验事实，不追认D已留证；legacy clippy既有两lint与官方0双口径；初稿重复编号已由Lead修复。均非本切片阻断。
- 文档陈旧口径：README:5仍记02c3ea8及delivery_head留空，按历史pre-audit快照读取，以当前清单b743c23为交付权威；meta/README省略workspace命令中的legacy，实读ci.ps1:120带该feature。记录为非阻断措辞瑕疵，不修改冻结交付物。
- D/Lead证据限度：终审过程中Lead追加`target/round76/lead-verify/{README.md,refresh-manifest.py,manifest-recompute.log,head.txt,checks.log}`；实读前三者确认19 file通过及inventory无生成器的声明。D未留证历史不追认；T独立证据为本裁定依据，聚合不可复现不阻断已核实的功能/原始红绿链。
- PARTIAL原号承接：GAP-1/2/R70-C2、GAP-3/4/6、GAP-5/R70-C1、R70-C3；R71时钟/对象/C7永久、R73-T1、R72 B plan-M4/legacy lint、历史E1/R69条件5、R68/R49–R70/R61–R67、R74 artifacts34、R75-E1/C-M1与R75-E2/C-M2；A sidecar及B日志损失流程项不清零；完整DrawBlend/rate/Mode、src×0.8、严格寿命、26技能及设备视觉均未验收。
- 证据入口：`target/round76/t/attempt-1/runs.json`、`checks/{manifest-check,extra-checks,red-check,semantic-evidence}.json`及`logs/`；本结论限定R76有限切片与可复算证据，不关闭历史PARTIAL。
- T2三项L0抽核均EXIT0：实读HEAD=72fb0a4及log -6，`41425b1..HEAD`仅docs下7文件、309增/2删；复算manifest 7336B/4ac0665b…与外置摘要一致；门禁attempt-2 log 215013B/85bdd3e6…、meta 802B/71c9ca8caad3b18512bd407ce4eac60137835eea8d1b7685282803069007cc67匹配；JSON断言确认19/19 file通过、inventory 5项与真红摘要。
- T2证据：`target/round76/t2/attempt-1/runs.json`、`logs/`与`spotcheck.py`；执行器/WindowsApps别名/默认GBK启动失败单独保留，bundled Python `-X utf8`后通过，均不计入T的56项；未重跑重门禁。写入时发现T已补出30行原稿，保留其核验细节并补本次抽核，最终裁定不变。
