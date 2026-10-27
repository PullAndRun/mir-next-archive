# R71 尚书省验收

- **verdict：ACCEPT-WITH-PARTIAL；实现/测试/证据补救必改0。** 仅验收149 FireWall有限切片，C7永久PARTIAL；不代表R71全量通过、Crystal生产等价或多消费者已实现。
- 验收官gpt-6.1-sol；UTC开始2026-10-27T10:28:55Z；实测收口10:34:23Z；HEAD/origin/main=`b52890f5dde145275d3bd264630c6334d15a0731`。
- 完整命令/UTC/EXIT：[runs](../../target/round71/shangshu/attempt-1/runs.json)；逐项实值：[checks](../../target/round71/shangshu/attempt-1/checks.json)；[日志](../../target/round71/shangshu/attempt-1/logs/)。只读复核原件，不重跑全量门禁。

|必核项|结论|实测/命令与边界|
|---|---|---|
|1 四子单/父todo|PASS|`edict show <父/四子ID> --json --as shangshu`均EXIT0；A96fd3df7/B e4596e20/C3cd9c7fd/D8a71743a均Done、delegation_result非空；父初始todos=[]。C原BLOCK回填保留，与C2并读。|
|1 提交/范围|PASS|`git log --oneline -8`及六提交`git show --format=fuller --stat`均EXIT0：cedf903→16644d2→adf63c1→7fadbe2→ddc637c→b52890f；功能仅spellfx +31/测试1590行；`git diff cedf903 -- crates tools`空、EXIT0。|
|2 门禁原件|PASS|SHA256/bytes复算log=212193B、meta=476B/12字段；解码原日志汇总12 OK；meta EXIT0/FAIL0、HEAD16644d2、UTC09:49:36–09:53:37；adf63c1提交10:05:55及后续docs晚于门禁。|
|3 清单逐项|PASS|22/22 bytes及SHA吻合、mismatch=0；`git rev-parse HEAD:<path>`核15个tracked blob，mismatch=0；stage=post-audit，无manifest/外置sha自引用；gate段与原件全同；audit PASS-WITH-PARTIAL/must_fix0。|
|3 外置摘要/freeze|PASS|清单12163B，外置单行SHA同值；两freeze原字节SHA均吻合、未修改，详下。|
|4 独立抽证|PASS|`cargo test -p mir-client --features mir-render/legacy --test round71_firewall_mapping` EXIT0，8 passed/0 failed/0 ignored（10:30:17.541–10:30:52.284Z）；`cargo fmt -p mir-client -- --check` EXIT0（至10:30:53.293Z）。|
|5 证据链诚实|PASS|实读C BLOCK→A勘误/Lead及门下裁决→C2 E1 CLOSED；C7继续永久PARTIAL；T29行/6317B及SHA吻合。A同64ff测试SHA红2/6 EXIT101→绿8/0 EXIT0；五变异补丁和25份B/C原日志SHA吻合、失败集合一致；未重跑五变异。|
|5 语义/扩称|PASS（切片）|实读生产272–277及契约：仅149→Magic1630..1635、6×120ms elapsed、格锚点+当帧signed offset一次、R70 rate=1 Add；非149旧行为保留、1620不迁；索引/client/roadmap均限定范围，无肯定式全量/生产等价/多消费者扩称。|
|6 历史保全|PARTIAL（永久）|E1补救CLOSED不等于C7满足；两c5c7d8日志缺失，不补造、不用最终复算追认中间历史。|
|7 CLI收口|PASS|`done JJC-20261027-008 --as shangshu` EXIT0→Review；`validate --json --as shangshu` EXIT0、ok=true/tasks=231/issues=[]；未请求Done终态/代门下confirm。|

- 新建独立`CARGO_TARGET_DIR=target/round71/shangshu/attempt-1/cargo-target`（启动前不存在）；TEMP=TMP=TMPDIR均该attempt下tmp（实际绝对路径见runs），CARGO_PROFILE_DEV_DEBUG=0；8项命名测试均有原日志，不借其他角色target。
- 门禁log SHA：`d1fcbf77dfe7751eefeaf03b5a6f8af70e564eacf94a8fc15b10789d2b2c8173`；meta SHA：`0d5478e7423655777957576b9b5f0c5d8d5ea3af2a44e31329e175ffe3972cfd`。meta门禁后补全，不能称后置BLOCK已在门禁HEAD内。
- 清单SHA：`1f7ae382b040353a80a9021240e6a181e4d0502253a83ed650cc02111987eb42`；T报告SHA：`18d94c43fe1e3a9f1116f1bbc508e1a25d54a1616c82d8dab297bbf7d87a13e0`。T审的是pre-audit21项，本次核post-audit22项，两个采样时点不混同。
- freeze-before SHA：`70845aefd4dd689a55ab0618b36bbf692a4b33ebb5ee2f3a662dedc17a3dc9de`；freeze SHA：`c7d896e7f5eee4706b943912c0c564d10d61c98d163143f051c2d632daecde32`；两文件仍历史未跟踪状态。
- 独立整帧算式800×600、五格3×3、格原点+(-3,5)、RGB565红248，共45像素；SHA `252a30f71d7682b5ffd9ac33f60fd7263c666e1680765d6f0abf932f2c03ea6c`，与B expected/actual二进制同值；[补充核验](../../target/round71/shangshu/attempt-1/deep-check-corrected.json) EXIT0。
- PARTIAL原号承接：[计划132–138](round71-plan.md#L132)、R71-C7永久、R69条件5永久；GAP-5/R70-C1非149/1620/其他调用点/真实视觉，R70-C3全局尺寸高度居中，GAP-1/2/R70-C2 rate0.8及近似/DrawColour/INVLIGHT/完整DrawBlend，GAP-3/4/6 mask/tint/shadow/场景offset/Light3。
- 严格NextMotion/初始100ms取整/每Process一步/持久149对象及26移除仍PARTIAL；4000ms仅瞬态窗口；全UI/设备/跨后端及R49–R70历史原号不清零。
- CLI偏差：按指定`todo <ID> list --as shangshu`实执行EXIT0，但本机将list解析为todo编号，创建#list not-started；已核源码cmdTodo并用完整四位置参数将该尚书条目标记小写completed，EXIT0；原父无todo，未替其他角色补造工作。
- 代笔/偏差：本报告与独立抽证由本验收官执行；A/B历史delegate-result的agent=main为既存转录，不替代独立原件。辅助oracle首轮中心y误写224而非256导致EXIT1；原失败与修正新版本均保留，修正后EXIT0，不算功能红。两次写工具参数校验失败无写入，之后骨架成功落盘。
- 写域仅本报告及指定attempt目录；未改实现/测试/其他docs、未commit/push、未运行ci.ps1。当前看板Review，后续指定门下确认由lead按真实门禁继续。
