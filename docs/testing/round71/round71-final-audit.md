# R71 御史台独立终审

- **verdict：PASS-WITH-PARTIAL；实现/测试/证据补救必改均 0。** 仅认可149 FireWall有限切片；C7永久PARTIAL。
- 启动UTC 2026-10-27T10:16:52Z；独立核验HEAD `7fadbe247ca44c26b370259eb6b909990e0d0954`；15分钟预算内完成。
- 命令、UTC、EXIT、计数及日志SHA见[运行账](../../../target/round71/t/attempt-1/runs.json)；[环境](../../../target/round71/t/attempt-1/environment.json)、[语义/原件验证](../../../target/round71/t/attempt-1/validation.json)。

| 必核项 | 结论 | 独立实值与边界 |
|---|---|---|
| 1 提交链/范围 | PASS | `git log --oneline cedf903..HEAD`、各提交`show --stat`：cedf903仅spellfx +31及测试1590；16644d2初稿→adf63c1 BLOCK/C2等5文档→7fadbe2六文档；`git diff cedf903 -- crates tools`空，均EXIT0。 |
| 2 门禁原字节 | PASS | [复算](../../../target/round71/t/attempt-1/gate-check.json)：212193B/476B、12字段、汇总12 OK；HEAD16644d2、EXIT0；UTC09:49:36–09:53:37。adf63c1 UTC10:05:55、7fadbe2 UTC10:16:21，明确为门禁后docs，不冒充同HEAD门禁。 |
| 3 清单/冻结 | PASS | [逐项复算](../../../target/round71/t/attempt-1/manifest-recompute.json)：21/21 bytes/SHA吻合，mismatch=0；246行/10971B、外置单行同SHA、无自引用，stage=pre-audit/head_snapshot=adf63c1、gate字段一致；两freeze实算吻合。 |
| 4 独立复跑 | PASS | 新target/tmp，debug=0；定向8 passed/0 failed/0 ignored EXIT0（10:18:09–10:18:47 UTC）；fmt EXIT0；clippy全targets -D warnings EXIT0（10:18:47–10:19:19 UTC）；无D8050。 |
| 5 语义/范围 | PASS（切片） | 实读Crystal SpellObject:288–302/323–339、MLibrary:692–709、DXManager:353–389；条件/phase/window/格锚点/offset一次/缺帧后续/Add逐格吻合；非149旧路径保留，wire/net/server/渲染算法无新增修改。 |
| 6 真红/裁决 | PASS / C7 PARTIAL | [抽核](../../../target/round71/t/attempt-1/deep-check.json)：A同测试SHA红2/6 EXIT101→绿8/0 EXIT0；B五变异均前绿→patch→build0→RED101→原SHA恢复→绿，补丁SHA可独立重建，物理副本已恢复且测试SHA一致；B/C失败集合一致，C原账34份日志SHA吻合。 |
| 7 文档诚实 | PASS | 实读[索引](README.md)、[契约](round71-firewall-contract.md)、[client §5.36](../../client.md#L1427)、[roadmap十九](../../roadmap.md#L310)：阶段/PARTIAL/真实视觉边界齐全；无全量通过、生产等价或多消费者已实现扩称；链接检查EXIT0（1844条，另跳过670条target链接），diff-check EXIT0。 |
| 8 卫生 | PASS | cedf903新增差异内ignore/allow=0，旧allow未改；freeze未动；工作树仅两历史未跟踪freeze及本次报告，target证据被忽略；未改实现/测试/其他docs/清单，未commit/push、未运行ci.ps1。 |

- 门禁log SHA256：`d1fcbf77dfe7751eefeaf03b5a6f8af70e564eacf94a8fc15b10789d2b2c8173`；meta：`0d5478e7423655777957576b9b5f0c5d8d5ea3af2a44e31329e175ffe3972cfd`。[12项解码汇总](../../../target/round71/t/attempt-1/logs/gate-tail-utf8.log)。meta为门禁后补全，按原件12字段，不改写为13。
- manifest SHA256：`add24e0f090e54e942ca4de48ec1dd0877f048ce629153c9d018c14d991f87d3`。client/roadmap/process/audit及本批新tracked文档均经`git rev-parse HEAD:<path>`→`git cat-file blob`核对；HEAD blob内容SHA全部匹配。null blob/tracked=false为adf63c1采样口径，已披露，并非现HEAD不跟踪。
- 独立命令：`cargo test -p mir-client --features mir-render/legacy --test round71_firewall_mapping`；`cargo fmt -p mir-client -- --check`；`cargo clippy -p mir-client --all-targets -- -D warnings`；`py -3 tools/docs/check_links.py`；`git diff --check`。均EXIT0，起止UTC与原输出见运行账。
- 8命名测试：[原日志](../../../target/round71/t/attempt-1/logs/test.log)：r71_g1_wire_mapping、r71_g2_frame_boundaries、r71_g2_age_window、r71_g3_anchor_offsets、r71_g3_offscreen_crop、r71_g4_add_per_cell、r71_g5_legacy_sources、r71_g5_missing_frame。
- A测试冻结SHA `64ff3f597e2530ce0d32abb21eb09b36638083384f27638a0b29177ac262b422`；B补丁SHA前缀M1–M5=`ed250944/f05a7cbf/149fada3/6da9c236/5b379b49`。M2红7/1，其余2/6；M5耦合offset/混合，不单凭其红隔离Add因子。
- 终审另写整帧算式：800×600、五格各3×3、格原点+(-3,5)、RGB565红248；45像素，SHA `252a30f71d7682b5ffd9ac33f60fd7263c666e1680765d6f0abf932f2c03ea6c`，与测试expected/生产actual二进制三源一致；此次未重跑五变异，不冒充新执行。
- C2无feature披露实核：原日志running8、0/8/0 EXIT101，测试319行要求legacy；属于夹具拒绝，不计功能真红。本终审按含feature契约独立8/0/0；C/C2原复算的隔离和历史边界保留。
- E1补救CLOSED依据必须并读：[A原清单:64/76](../../../target/round71/a/attempt-1/manifest.md#L64)＋[勘误](round71-a-erratum.md)＋[门下裁决](../../process/round71-menxia-evidence-ruling.md)＋[C2](round71-c-addendum.md)；两c5c7d8日志仍不存在，不补造/追认历史，[C BLOCK](round71-c-evidence.md)和16644d2初稿均保留。
- PARTIAL承接：[计划132–138原号](../../process/round71-plan.md#L132)全保留；R71-C7永久、R69条件5永久；GAP-5/R70-C1非149/1620/其他调用点/真实视觉，R70-C3全局尺寸高度居中；GAP-1/2/R70-C2 rate0.8/DrawColour/INVLIGHT/完整DrawBlend，GAP-3/4/6 mask/tint/shadow/Light3；严格NextMotion/100ms取整/每Process一步/持久149对象及26移除、全UI/设备/跨后端、R49–R70历史不清零。
- 实现实际为rate=1 Add、6×120ms elapsed、0≤age<4000；rate0.8及src×0.8×alpha近似未实现；4000ms仅瞬态窗口。允许继续尚书验收及指定门下确认，不等于终态确认。
- 终审辅助脚本曾因env的exit大小写及汇总账无log字段出现两次解析错误（EXIT1），均在限定证据域修正后复核EXIT0；不是功能红，见收尾记录。
- delegate-result实际 **EXIT3**：子单已Done，终态冻结拒绝写入，未重试/绕过；[回执](../../../target/round71/t/attempt-1/edict-result.json)、[收尾/辅助脚本错误](../../../target/round71/t/attempt-1/closeout.json)。最终链接/diff-check/crates-tools检查均EXIT0，已send_message给lead。
