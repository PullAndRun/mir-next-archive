# R71 门下省确认
- **verdict：PASS-WITH-PARTIAL，收口成立；必改0；仅限149 FireWall切片。** 主单`JJC-20261027-008`已由Review→PendingConfirm→Done；不代表R71全量通过、Crystal生产等价或多消费者已实现。
- 确认官gpt-6.1-sol；UTC核验10:41:24起，CLI终态10:42:57；HEAD=`83e3ccce9759aa44ae45ba69b3a0204a6294eb1b`（尚书验收提交），origin/main=`b52890f5dde145275d3bd264630c6334d15a0731`，未代为推送。
- 原始命令/UTC/EXIT：[runs](../../target/round71/menxia/attempt-1/runs.json)；实值/22项完整bytes及SHA：[checks](../../target/round71/menxia/attempt-1/checks.json)；[日志](../../target/round71/menxia/attempt-1/logs/)。

|六项抽核|结论|独立实测|
|---|---|---|
|1 尚书报告|PASS|[报告](round71-shangshu-acceptance.md)29行/5447B；SHA `c844a6f5790ced2b07626a090466006b4e74010ddeb41d6c775beb39cf1d852c`；ACCEPT-WITH-PARTIAL/必改0。|
|2 门禁log/meta|PASS|[log](../../target/round71/gate/attempt-1.log)212193B、SHA `d1fcbf77dfe7751eefeaf03b5a6f8af70e564eacf94a8fc15b10789d2b2c8173`；[meta](../../target/round71/gate/attempt-1.meta.txt)476B、SHA `0d5478e7423655777957576b9b5f0c5d8d5ea3af2a44e31329e175ffe3972cfd`；12 OK/EXIT0，锚16644d2。|
|3 post-audit清单|PASS|[清单](../testing/round71/final-manifest.json)12163B，22/22实算bytes/SHA吻合、mismatch0；[外置SHA](../testing/round71/manifest.sha256)同值 `1f7ae382b040353a80a9021240e6a181e4d0502253a83ed650cc02111987eb42`；非T采样的pre-audit21项。|
|4 T终审/E1/C7|PASS / C7永久PARTIAL|[T报告](../testing/round71/round71-final-audit.md)6317B、SHA `18d94c43fe1e3a9f1116f1bbc508e1a25d54a1616c82d8dab297bbf7d87a13e0`；PASS-WITH-PARTIAL/必改0；并读[C2](../testing/round71/round71-c-addendum.md)、[裁决](round71-menxia-evidence-ruling.md)：E1补救CLOSED，不等于C7满足。|
|5 两freeze保全|PASS|[before](../testing/monster-native/player-resource-freeze-before.json)44844165B/SHA `70845aefd4dd689a55ab0618b36bbf692a4b33ebb5ee2f3a662dedc17a3dc9de`；[freeze](../testing/monster-native/player-resource-freeze.json)45219533B/SHA `c7d896e7f5eee4706b943912c0c564d10d61c98d163143f051c2d632daecde32`，未写、仍历史未跟踪。|
|6 功能冻结/范围|PASS|[生产](../../crates/mir-client/src/ui/spellfx.rs)14779B/SHA `d7f86c29150f51aa865c2d668786bde45c1d7eb06c0172dc06e8d2d3a82343a6`；[测试](../../crates/mir-client/tests/round71_firewall_mapping.rs)59751B/SHA `64ff3f597e2530ce0d32abb21eb09b36638083384f27638a0b29177ac262b422`；`git diff cedf903 -- crates tools`空/EXIT0。|

- 独立快证：`cargo test -p mir-client --features mir-render/legacy --test round71_firewall_mapping` **EXIT0，8 passed/0 failed/0 ignored**；UTC10:41:25.041–10:41:59.623（34.582秒）；[原输出](../../target/round71/menxia/attempt-1/logs/independent-test.stdout.log)。
- 独用新建`CARGO_TARGET_DIR=target/round71/menxia/attempt-1/cargo-target`（启动前不存在），TEMP=TMP=TMPDIR均attempt下tmp绝对路径，`CARGO_PROFILE_DEV_DEBUG=0`；未借其他角色target，未重跑五变异/全量门禁。
- CLI回执：`state JJC-20261027-008 Done --as menxia` **EXIT0**→PendingConfirm；实读confirm_by=menxia后，`confirm JJC-20261027-008 approve --as menxia` **EXIT0**→Done；`validate --json --as menxia` **EXIT0**，ok=true/tasks=231/issues=[]；最终show EXIT0、Done（见[runs](../../target/round71/menxia/attempt-1/runs.json)）。
- 提交链：cedf903功能→16644d2门禁锚→adf63c1 BLOCK/勘误/裁定/C2→7fadbe2文档→ddc637c终审→b52890f post-audit→83e3ccc尚书验收；后置docs不冒充已在门禁HEAD内，meta为门禁后补全。
- 诚实性：实读[契约](../testing/round71/round71-firewall-contract.md)与上述报告，仅149、6×120ms elapsed、格锚点/当帧offset一次、实际rate=1 Add；rate0.8及src×0.8×alpha近似未实施；未发现将本切片扩称全量/生产等价/多消费者实现的肯定式声明。
- PARTIAL承接[计划132–138原号](round71-plan.md#L132)：R71-C7永久、R69条件5永久；两c5c7d8历史日志缺失，不伪造、不追认；GAP-5/R70-C1非149/1620/其他调用点/真实视觉，R70-C3全局尺寸高度居中，GAP-1/2/R70-C2 rate0.8/DrawColour/INVLIGHT/完整DrawBlend，GAP-3/4/6 mask/tint/shadow/场景offset/Light3。
- 严格NextMotion/初始100ms取整/每Process一步/持久149对象及26移除、全UI/真实设备/跨后端、R49–R70历史原号均不清零；4000ms仅瞬态窗口。
- 代笔/偏差：本确认及快证由本门下独立执行；A/B历史agent=main回填为既存转录，非本次代执行证明。承接尚书#list误建后completed及oracle首轮y=224误写EXIT1的原披露，不抹除、不当功能红。
- 本次偏差：首次glob超时；骨架写工具两次参数/模式校验失败后成功，均无先前写入效果；无CLI exit3，无重试或绕过。HEAD领先origin一个验收提交，如实保留。
- 最终卫生：除本报告外工作树仅两历史freeze未跟踪；只写本报告与指定attempt证据，未改交付物/其他文档、未commit/push、未运行ci.ps1。
