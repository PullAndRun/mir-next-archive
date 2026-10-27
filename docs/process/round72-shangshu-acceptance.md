# R72 尚书省验收

- 任务 `JJC-20261027-009`；验收官 shangshu（gpt-6.1-sol）；日期 2026-10-27 UTC。
- verdict：**ACCEPT-WITH-PARTIAL；必改 0；仅限 149 毒云逐格精灵映射切片**。
- 快照：HEAD=origin/main=`ee7e05c85b2583e803e9989cace086f1561d7e44`；未改交付物，报告留工作树由 Lead 提交。

| 必核项 | 结论 | 本次独立证据 |
| --- | --- | --- |
| 看板 | PASS | 父单 Doing、todos=[]；A/B/C/D 均 Done、todos=[]、delegation/delegation_result 非空；历史 BLOCK 未抹除。 |
| 提交链 | PASS | 2a31554→d8b6157→90a7516→87ffee3→60fd5fe→7f01b18→fc7f6c1→9586d46→ee7e05c；功能2文件+1698。 |
| 变更边界 | PASS | `git diff 2a31554 -- crates tools`、`-- docs/mir*` 均空/EXIT0；ci.ps1 legacy测试特性始于R68提交6ba5d45。 |
| freeze | PASS | 两文件仍未跟踪；本次重算bytes/SHA与C原始static-audit完全一致，详见下方。 |
| 唯一门禁 | PASS | 原字节复算212729B；12 OK/0 FAIL；meta/manifest的feature HEAD、EXIT0、UTC、operator=lead一致，未另跑全量门禁。 |
| post-audit清单 | PASS | 18/18 bytes/SHA；14/14 tracked blob对delivery_head=9586d468ab62868235e81155b47ccf56bc0903a5一致；无自引用、外置SHA匹配。 |
| T/T2锚点 | PASS | T首轮87ffee3的blob=43f46d0866c4e8cb2e446922ea325552d0b49d21；T2 9586d46的blob=59e3d89fb90d9ddb643667061f22957f819f4c72，均在清单。 |
| 独立隔离复跑 | PASS | R72=9、R71=8、R70=9、R63=3；合计29 passed/0 failed/0 ignored；cargo test与fmt均EXIT0。 |
| 证据诚实 | PASS-WITH-PARTIAL | T首轮BLOCK(T-DOC-1)→修复→T2关闭；C首轮BLOCK→B attempt-2补证/撤回错误声明→Lead裁定→C2；A/B同测试SHA，无肯定式整轮/等价扩称。 |
| 文档/差异检查 | PASS | `py -3 tools/docs/check_links.py`、`git diff --check` 均EXIT0。 |
| CLI收口 | PASS | `done JJC-20261027-009 --as shangshu` EXIT0；`validate --json --as shangshu` EXIT0、ok=true、236 tasks、issues=[]；再次show确认Review。 |

- 独立命令：`cargo test -p mir-client --features mir-render/legacy --test round72_poison_cloud_mapping --test round71_firewall_mapping --test round70_effect_blend_offset --test round63_projectile_wire`；`cargo fmt -p mir-client -- --check`。
- 环境：独立全新 `CARGO_TARGET_DIR=target/round72/shangshu/cargo-target`；`TEMP=TMP=TMPDIR=target/round72/shangshu/tmp`（实际绝对路径）；`CARGO_PROFILE_DEV_DEBUG=0`。
- 复跑UTC：2026-10-27T13:32:59.608165Z–13:33:32.575430Z；fmt至13:33:33.417922Z；CLI收口13:35:35.186895Z–13:35:35.359045Z。
- 清单8570B，SHA256=`540b0ef2be51130eaf63e2aeb2ec5cd6f70c1ac75112335cbf988de88c5944c7`；gate SHA256=`6fe99c93ee0bfaed2ede492071d5c2822415e453df37a3ec6025436410b43a3c`。
- 门禁HEAD=`2a31554bfe31a05bb828119f3fb96d8a6544fba3`；UTC=12:36:59Z–12:41:04Z（终点按原日志最终mtime秒值）；meta EXIT0/items12/0/operator=lead。
- freeze-before=44844165B/SHA256=`70845aefd4dd689a55ab0618b36bbf692a4b33ebb5ee2f3a662dedc17a3dc9de`；freeze=45219533B/SHA256=`c7d896e7f5eee4706b943912c0c564d10d61c98d163143f051c2d632daecde32`。
- A红/绿及B留存测试SHA同为`58f64ab2d30b69f3aea5c07a8975e988ca842bce45972499a6f93e120958baea`；本次仅验证留存链，未重做变异。
- PARTIAL原号承接：R71严格NextMotion/初始100ms取整/每Process一步/149持久场/26移除/对象寿命；4000ms仅产品瞬态窗。
- PARTIAL原号承接：GAP-1/2、R70-C2 rate0.8/src×0.8×alpha/DrawColour/INVLIGHT/完整DrawBlend；GAP-3/4/6 Light3/mask/tint/shadow/场景offset/AnimationOffset。
- PARTIAL原号承接：GAP-5/R70-C1其余26技能及1620/其他调用点；R70-C3居中、B plan-M4精确写法残余、legacy lint债、真实素材/GPU视觉未签核、R71-C7及更早原号均不清零。
- 偏差：首次脚本cwd误落父目录，cargo101/fmt1/git128等无效预检未计绿；原始记录复制保全至preflight-invalid-cwd，父目录同名临时证据仍保留；纠正后有效复跑如上。门禁混合编码按原字节校验、摘要按ASCII行首/秒数计数；CLI无包装异常。
- 证据：[runs.json](../../target/round72/shangshu/attempt-1/runs.json)、[checks.json](../../target/round72/shangshu/attempt-1/checks.json)、[logs](../../target/round72/shangshu/attempt-1/logs/)；含每命令UTC/EXIT、原始日志、逐项SHA/blob、门禁末25行及29例计数；未commit/push。
