# R71-C2 刑部补充复检

- **verdict：PASS-WITH-PARTIAL；实现/测试/证据补救必改均 0。** E1 补救 CLOSED；C7 历史保全永久 PARTIAL，原 C BLOCK 裁决不改。
- 独立采样 UTC 2026-10-27 10:01–10:05；[命令/起止UTC/EXIT/计数/日志SHA](../../../target/round71/c2/attempt-1/runs.json)、[静态原件审计](../../../target/round71/c2/attempt-1/static-audit.json)、[日志](../../../target/round71/c2/attempt-1/logs/)为本次证据。

|必核项|结论|独立事实与边界|
|---|---|---|
|1 E1 补救|CLOSED|勘误明确撤回两日志存在/完整保全；原清单未重写；门下 A/B 接受补救关闭，C7 永久 PARTIAL。|
|2 禁历史冒充/改写|CLOSED|受核原件前后 SHA 全同；最终证据只验最终冻结行为，不追认 c5c7d8 红绿，不伪造历史。|
|3 SHA/提交事实|CLOSED|目标文件名搜索空；A 的完整 c5c7d8 SHA 仅在 sha-final.txt:2、test-sha-green.txt:1；red2/green2/现行测试均为下述64ff SHA；crates/tools diff 空。|
|4 门禁/历史关系|CLOSED|实算门禁212193B、下述SHA，meta HEAD=16644d2、EXIT0、12/12，原日志尾部12项OK；该提交仅C初稿55行PASS，当前56行BLOCK为门禁后未提交修改，勘误/裁定不在该HEAD。|
|5 裁决可执行性|CLOSED|门下 F:33 五项C2条件明确；D:24–26承接计划132–138原号与R69先例，不要求改写manifest。|
|6 独立复算|CLOSED（命令偏差披露）|字面无feature命令 EXIT101、0/8/0，夹具要求legacy；带原契约feature后 EXIT0、8/0/0；fmt/clippy均EXIT0，无D8050。|
|C7 历史保全|PARTIAL（永久）|当前搜索只证不存在；覆盖原因与不可恢复性依据Lead披露，不声称已核验全部历史。|

- 现行索引必须并列读取：[A原清单:64/76](../../../target/round71/a/attempt-1/manifest.md#L64)＋[A勘误:7–19](round71-a-erratum.md#L7)＋[门下裁决A–F](../../process/round71-menxia-evidence-ruling.md#L6)；不得裸引原清单“均有日志”。
- 交叉核对：[C原报告:3/21/56](round71-c-evidence.md#L3)要求勘误或门下明确裁决；[勘误:13–19](round71-a-erratum.md#L13)撤回并禁伪造；[Lead:14–17](../../process/round71-lead-ruling.md#L14)区分门禁与后续文档；门下B/C/F口径一致，未将E1关闭写成C7满足。
- 门禁SHA256 `d1fcbf77dfe7751eefeaf03b5a6f8af70e564eacf94a8fc15b10789d2b2c8173`；当前C报告SHA `e41c1edb4483b13a64c1cf90f07a59188c7d4e091a6d4d51b4c9dd2a72df6629`，均独立实算。
- 最终测试SHA `64ff3f597e2530ce0d32abb21eb09b36638083384f27638a0b29177ac262b422`；A最终红2/6/0 EXIT101、绿8/0/0 EXIT0原件核读；B/C五变异原红日志计数/失败集合逐组相同，本次不重复五变异执行。
- 副本由 `git -c core.autocrlf=false archive 16644d2` 实体提取，生产/测试nlink=1、SHA与冻结一致；独用 `target/round71/c2/attempt-1/{cargo-target,tmp}`，TEMP/TMP/TMPDIR同tmp，未借A/B/C编译物。
- 字面 `cargo test -p mir-client --test round71_firewall_mapping` 10:01:27–10:02:08 UTC，EXIT101、0/8/0，夹具319行报“请加 --features mir-render/legacy”，原件保留，不计为功能断言红或D8050。
- 按原契约追加 `cargo test -p mir-client --features mir-render/legacy --test round71_firewall_mapping -- --test-threads=1`，10:02:59–10:03:13 UTC，EXIT0、8/0/0；`cargo fmt --check` EXIT0；`cargo clippy -p mir-client --all-targets -- -D warnings` EXIT0。
- PARTIAL台账：[计划132–138](../../process/round71-plan.md#L132)全部原号保留：GAP-5/R70-C1非149/1620/其他调用点/真实视觉；R70-C3全局尺寸高度居中；GAP-1/2/R70-C2 rate0.8/DrawColour/INVLIGHT/完整DrawBlend；GAP-3/4/6 mask/tint/shadow/场景offset/Light3。
- 严格NextMotion、初始100ms取整、逐Process一步、149持久场/26移除仍PARTIAL；4000ms仅事件窗口；R68/R69条件5（含a77永久缺口）、R49–R67/R61–R67历史原号、全UI/真实设备/跨后端全部承接。
- 本追加满足门下F的C2条件，可推进D/独立终审/尚书验收；仅认可149 FireWall六帧elapsed循环及格锚点/offset接线，不代表R71全量通过、生产完整等价、完整Crystal迁移或真实视觉签核。
- 范围：仅新增本报告及C2 attempt-1证据；未改实现/测试/既有报告与清单，未commit/push，未运行ci.ps1；原BLOCK、初稿历史与缺失原状均保留。
