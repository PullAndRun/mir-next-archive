# R73-C 刑部独立核验

- 子单 `JJC-20261027-010-sub-cb519bdf`；HEAD `93e11407fad3da5fb08b030781058e216ccf8232`；核验 UTC 2026-10-27T15:01 起。
- **verdict：PASS-WITH-PARTIAL（必改 0；当前有限实现功能缺陷 0）**。初审BLOCK的M3/M5证据项经Lead显式裁定与B溯源补记关闭，历史发现与实值保留如下；PARTIAL 不清零。
- 核验只写本页与 `target/round73/c/**`；未 commit/push、未跑 ci.ps1；实现/交付测试及两 freeze 未改。

|核验清单|结论|证据/边界|
|---|---|---|
|1 范围/诚实|PASS|初始 status 恰 A 两文件+两 freeze；diff 仅 spellfx +54；无协议/net/magic/server/旧测试改动；新增 allow/ignore=0。|
|2 实现/Crystal|PASS-WITH-PARTIAL|限定 ThunderBolt && !from_object_spell && param；age>=400 不画；10+min(age/80,4)；TargetPoint 格锚点；Swing/start0、offset 一次、rate1 Add；其他分支保旧。|
|3 独立复跑|PASS|新 C target/tmp：R73 9/0/0，回归29/0/0，lib4/0/0；clippy/fmt EXIT0，与A/B自报相符。|
|4 真红/变异|PASS-WITH-PARTIAL|A同最终测试基线真红核实；C独立M1/M4及精确M3/M5真红/恢复绿；B原M3/M5转辅助扩展，差异与裁量保留如下。|
|5 独立oracle|PASS|C自写二进制解码/坐标/裁剪/RGB565/Add/Over；抽8条 actual=A expected=C Python 逐字节/SHA一致；未调用生产helper或B脚本。|
|6 登记/隔离|PASS|CLI show A/B均Done；A终态写result EXIT3已披露；B attempt-2用memo，attempt-1保旧impl；B脚本复用如实披露。|
|7 PARTIAL承接|PASS-WITH-PARTIAL|A manifest逐号承接9组台账，B继承并列B2-D1..5；“148”不存在为需求勘误，实际120/123/149矩阵有证；不扩称完整等价。|

## 初审必改及最终裁量（已关闭）

- Lead显式裁定见 `docs/process/round73-lead-ruling.md`：以C精确M3/M5实跑为权威；保留clamp4、400ms泄漏14替代不可达15诱饵要求；不重冻最终测试。B `attempt-2/PROVENANCE-M3-M5.md` 及 `manifest.md` 附节已核实将两项标作辅助扩展并引用C证据，原日志未重写。最终必改0。
- CLI初次delegate-result于UTC15:10:03 EXIT0提交的是历史BLOCK；裁定收敛后以memo补记PASS-WITH-PARTIAL，不再对Done子单重复写result。

1. **M3契约缺口**：最终测试 `MAIN_MAGIC2`（:152–170）无15诱饵；B `patches/M3.diff` 除删截止外还改 min(4)→min(5)，且 `M3-apply.json` 测试SHA变为 `7728bd30234f9194bfb66d12910657604b8b5e5824e25faf5bb984fac320697d`，非最终 `1ec3f6db…`。派发:18/24/26要求同测试/夹具SHA与仅删continue；披露不等于满足。须由Lead/指定门下明确接受“仅删截止、clamp4保留、400ms泄露14”的原测试证据，或补15后重冻测试并重跑受影响证据；禁止把B扩展变异算作原契约已通过。
2. **M5原定义不完整**：B `patches/M5.diff` 仅去来源判断，仍保留param；原契约要求准入仅留技能条件。须补仅技能条件的 build0→指定G5 RED101→恢复SHA/9绿证据，并在B登记原变异与扩展变异区别。C已在自有副本补原定义实跑供裁量，不修改B历史。
- 非阻塞注释勘误：`spellfx.rs:309` 写“Time<=NextFrame 才推一帧”，Crystal `Effect.cs:81` 实为该条件即return；应写 Time>NextFrame 才推进。实现本轮elapsed语义正确；纠正需Lead按变更规则重冻，C不越权改源码。

## 独立复跑与真红

- 完整命令、cwd、UTC起止、环境、EXIT、计数、日志SHA见 `target/round73/c/attempt-1/runs.json`；stdout/stderr合并保存在 `logs/`。
- `cargo test -p mir-client --features mir-render/legacy --test round73_thunderbolt_hit_mapping -- --test-threads=1`：UTC15:01:59–15:02:31，9/0/0，EXIT0。
- `cargo test -p mir-client --features mir-render/legacy --test round72_poison_cloud_mapping --test round71_firewall_mapping --test round70_effect_blend_offset --test round63_projectile_wire`：3+9+8+9=29/0/0，EXIT0。
- `cargo test -p mir-client --lib ui::spellfx`：4/0/0（328 filtered）；`cargo clippy -p mir-client --all-targets -- -D warnings`、`cargo fmt --all -- --check`：均EXIT0。
- 全新 `CARGO_TARGET_DIR=target/round73/c/cargo-target`；TEMP=TMP=TMPDIR=`target/round73/c/tmp`；CARGO_PROFILE_DEV_DEBUG=0，实际绝对路径见runs；变异另用 `red-cargo-target`。
- C `git archive 93e1140` 物理副本位于 `target/round73/c/red-copy/`，只叠当前A两文件；nlink=1，无链接；原件/变异/恢复SHA在 `attempt-1/independent-red/`。
- M1仅Swing→Bolt：build EXIT0；RED 0/9/0 EXIT101；首败G1 UI :1239，(384,256) actual=[48,136,176,255] expected=[0,0,0,255]；恢复e0bcaef0…/1ec3f6db…后9/0/0 EXIT0。
- M4仅true→false：build EXIT0；RED 0/9/0 EXIT101；首败G1 UI :1239，(384,256) actual=[248,0,0,255] expected=[0,0,0,255]；恢复同SHA后9/0/0 EXIT0。
- A红证据 `red-05-final-run.log`：基线impl df572677…=HEAD，测试1ec3f6db…；0/9/0 EXIT101；首败G1 :1239，(381,261) actual=[0,0,0,255] expected=[248,0,0,255]；与最终同测试SHA。
- B attempt-2 M1..M5 build均EXIT0、RED均EXIT101、恢复绿均9/0/0；红计数依次0/9、5/4、7/2、0/9、8/1；全部日志SHA逐项复算匹配；恢复副本原SHA均匹配。M2仅静态核验，B扩展M3/M5不冒称原定义通过。
- C额外原定义M3（只删continue，未改测试）：前绿9→build0→7/2/0 EXIT101；首败G2 :1413，age400 (415,255) actual=[248,0,248,255] expected=[0,0,0,255]；恢复e0bcaef0…/1ec3f6db…后绿9。15诱饵要求仍须裁定。
- C额外原定义M5（仅技能条件）：前绿9→build0→8/1/0 EXIT101；首败G5 :2127，120/cast=false (292,190) actual=[0,252,0,255] expected=[0,0,0,255]；恢复原SHA后绿9。实跑结果不自动改写B历史或门下条件。

## Oracle、卫生及PARTIAL

- 独立脚本 `attempt-1/oracle_check.py`，结果 `oracle-check/result.json` 与8份Python RGBA；UTC15:03:47，EXIT0，8/8；B dump共63条，本C仅抽核8条。
- 抽seq0/1：G1 age0整帧/公共UI 3×3 ROI；seq18/9/10：age80/399/400；seq38：offset(50,-30)将屏外格原点移至(2,2)后裁剪；seq42：两事件顺序Add；seq53：cast=false旧Bolt/Over金样。三源每条完整SHA见result。
- 格坐标手算(384,256)+(target-center)×(48,32)，每帧signed offset恰一次；WZX表偏移48、WZL头16、zlib、bottom-up/4字节行对齐；RGB565位移、0透明、Add饱和/Over均独立实现。
- 实现SHA `e0bcaef0ff6a6103079e5cdb3d31f16e2a2db918b3ae390babd7290d00d3fc1b`（21862B）；测试SHA `1ec3f6dbbf7368e7dbd82c78529f1e6605908b04c11164ba860ab28b42513c5e`（84910B）。
- 两freeze SHA `70845aefd4dd689a55ab0618b36bbf692a4b33ebb5ee2f3a662dedc17a3dc9de`（44844165B）、`c7d896e7f5eee4706b943912c0c564d10d61c98d163143f051c2d632daecde32`（45219533B）原值未变。
- allow仅HEAD既有3处：worktree :144/:162/:372；测试allow/ignore=0。A manifest SHA e6546e21…/d235eb54…；B manifest及副本/patch/log哈希核验在 `static-audit.json`。
- Crystal逐读锚点：PlayerObject.cs:2956/2819/1708/3225；Effect.cs:38/81/113；MLibrary.cs:692；DXManager.cs:353。Rate默认1，正常SourceAlpha/One仅支持有限CPU近似。
- 原号承接：GAP-5/R70-C1其余技能/1620/起手20/3/300；R70-C3全局尺寸/高度/owner锚点；R71时钟/对象偏差strict NextMotion/初始100ms/每Process一步/149持久场/26移除/寿命/4000窗；R73-T1严格等号/跳帧/ActionFeed/回执起算/owner跟随/Light6/声音，均PARTIAL。
- GAP-1/2/R70-C2 rate0.8、src×0.8×alpha、DrawColour/INVLIGHT/完整DrawBlend/API；GAP-3/4/6 Light3/mask/tint/shadow/场景offset/AnimationOffset；R72 B plan-M4及legacy lint；R71-C7/E1/R69条件5永久历史缺口；R68/R49–R70/R61–R67真实素材/GPU/设备/全UI/跨后端/Live音频继续原号保留，E1仅补救CLOSED。
- B2-D1 bundled Python 3.12.14替代系统存根；B2-D2扩展M3/新M1 patch；B2-D3复用脚本但重生成dump；B2-D4不存在148、按实际矩阵；B2-D5新增allow0，均已核实；材料替换/蓝本矩阵/历史PARTIAL不据合成证据清零。

## Lead 裁定收敛（2026-10-27）

- 裁定依据：`docs/process/round73-lead-ruling.md`，SHA256 `7927d0332f6e9fff20112542c0e460c7c91d8a4fb500804af57776ec17d9aa5c`；初审 BLOCK 作为历史审计结论保留在 addendum，不删原发现。
- 裁定1：M3/M5 CLOSED，以 C 精确契约实跑为权威（初读 :35–36；并发改写后 :38–39）；M3 仅删 continue、clamp4 保留，7/2/0 EXIT101，G2:1413 age400 (415,255) actual=[248,0,248,255]→expected=[0,0,0,255]；M5 仅技能条件，8/1/0 EXIT101，G5:2127 (292,190) actual=[0,252,0,255]→expected=[0,0,0,255]；均 build0、恢复原SHA后绿9。
- B 扩展 M3/M5 仅作辅助红证据；`target/round73/b/attempt-2/PROVENANCE-M3-M5.md`（SHA256 `8c2954c518b714f88b97d14b859b601453c69d84cd193ed860336d5e4e5a81af`）及 `manifest.md:65–69` 已标注并交叉引用本报告。
- 15 诱饵要求不采纳；门下要求与最终夹具偏差保留披露，以 400ms 泄漏 frame14 为最小真实断言，不重冻测试。
- 裁定2：spellfx.rs:309 注释勘误属 MINOR、非阻断，由 R74 承接；严格 Crystal NextFrame 时钟仍为 PARTIAL。
- 裁定3：其余独立复跑/oracle/范围/新增allow=0/两freeze结论采纳；原号 PARTIAL 台账不变，实现功能缺陷0。
- **收敛后 verdict：PASS-WITH-PARTIAL；必改 0**。
- C2 仅追加本节、新建 addendum 及 C2 证据；未改实现/测试/其它文档，未 commit/push、未运行 ci.ps1；行数/bytes/SHA256 见 `target/round73/c2/attempt-1/sha256.txt`。初读49行与追加前52行的并发差异已向 Lead 披露，C2未回改前文。
