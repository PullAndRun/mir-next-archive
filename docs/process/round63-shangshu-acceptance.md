# R63 尚书省独立验收（attempt-2 定稿）

- verdict：**ACCEPT-WITH-PARTIAL（必改 0；限定 G1+G2，R63 整体仍 PARTIAL）**。
- 复验 HEAD：`97714865959626ce93f309ec06d3fb2ce3089d92`（`9771486`）；2026-10-27 北京时间独立复算。
- 历史：attempt-1 因 S1 判 BLOCK；中间复核因 S1b（manifest 清单 artifact 旧值及阶段措辞残留）继续 BLOCK，现经 `9453e32`、`9771486` 两个 docs-only 提交修正。
- 权限：仅更新本文及获准 target 证据；未 commit/push、未跑 Lead 门禁、未改 crates/看板；**门下确认尚未完成，push 待 Lead**。
- [原始证据根](../../target/round63/shangshu/attempt-2/)；`timestamps.txt` 记录起止，下列相对证据名均指该目录。

## S1 / S1b / S2 闭环

| 项 | 判定 | 当前字节、命令与独立结果 |
|---|---|---|
| S1：生成器与快照不一致 | CLOSED | `bash docs/testing/round63/run-checks.sh` 完整实跑 exit0，16 PASS/0 FAIL/0 SKIP，11秒；第10项 exit0，`checks/10.log` 为 tree-hashes-reproducible；另跑生成器 exit0、`diff -u` exit0，`tree-diff.txt` 空 |
| S1：README 清单值 | CLOSED | README 8060B/87行/SHA256 `cae217cadcd0d9518bdfb36d72b8f013a3f8246ce265117034d41340f82dd426`；生成器、冻结清单、manifest 与工作树一致 |
| S1b：manifest 清单 artifact 旧值 | CLOSED | commit-tree-hashes 1311B/13行/SHA256 `86f6c559df6c305579da31713269b080947f4bb29ab67bebaebaed24000067cf`；manifest 当前值逐字节匹配，旧 f1a7b7f9 值已替换 |
| S2 / S1b：阶段措辞 | CLOSED | 人工核 README:3/87、roadmap:234/245、manifest P6/publication；终审已完成 PASS-WITH-PARTIAL、尚书修正后待复验、门下未确认的时序一致；目标位置旧“终审尚未进行”命中0；`stage-scan.txt`；第14项正面禁称扫描 exit0 |

- 独立复算：`py -3 target/round63/shangshu/attempt-2/independent-recheck.py` exit0；`manifest-recheck.json` 记录17 artifacts，size/SHA mismatch=0，登记行数均匹配；5功能 blob 全匹配。
- 第10项独立命令：`bash docs/testing/round63/gen-tree-hashes.sh` 输出与 `docs/testing/round63/commit-tree-hashes.txt` 作 `diff -u`，均 exit0。
- README/manifest 阶段语句是复验前快照；本轮尚书 verdict 以本文为准，不倒改冻结产物或预写门下确认。完整脚本只检查manifest可解析/非空，17项SHA正确性另经独立复算证实。

## 门禁、提交、产物、测试与红线

| 项 | 判定 | 实际复核结果与证据 |
|---|---|---|
| 提交链 | PASS | `git log --oneline -6` exit0：f80724e→1fcf4fc→5abdcd3→0a44ace→9453e32→9771486；`git-log.txt` |
| 功能范围与后续提交 | PASS | `git diff --stat f80724e..1fcf4fc`：5文件 +874/−66；`git diff --name-only 1fcf4fc..9771486`：11路径全 docs；crates diff 空；命令均exit0，见 function-stat/docs-names/crates-diff |
| 门禁原始字节 | PASS | 日志190207B/SHA256 `15adfa307592ef1ab3ea4cec4df248a852c62c814974c8d5043c8f52e2ac1656`；第9项exit0；原日志末尾12 OK 独立核得，`historical-evidence.txt` |
| 历史门禁 EXIT/HEAD | PARTIAL | 原 meta 60B，仅 UTC_START/UTC_END；E3附件声明 WRAPPER_EXIT=0、HEAD=1fcf4fc，为Lead事后补录；非本官对历史进程的独立观察，保留M3/P7 |
| 刑部终审原件 | PASS | 4709B/48行/SHA256 `bd7863a0043668f1f259bb78f8d8fade22d8d5edd0d2e0e607bd7bf3915762b3`，PASS-WITH-PARTIAL，仅G1+G2；原件与manifest audit登记一致 |
| 功能冻结 | PASS | 5文件工作树字节均等于 `git show 1fcf4fc:<path>`；新wire测试16848B/415行/SHA256 `6438cce5f43358e5ede1195ae357c9041b2645662bc49df1b05b307ccf1c0ded`，红绿冻结值维持 |
| 功能回归 | PASS | 本次lib24、wire3、R59 16、R61 11、R62 5，全部0失败/0忽略；`checks/1.log`–`checks/5.log` 与 `tests-summary.txt` |
| 格式与静态检查 | PASS | fmt exit0，无输出；clippy exit0，无诊断；`checks/6.log`、`checks/7.log` |
| 链接与格式 | PASS | 独立 `py -3 tools/docs/check_links.py` exit0、0断链；复验前1844相对链接/2篇README，与交接1833计数不同；定稿后计数以 `final-links-decoded.txt` 为准；第16项git diff --check exit0 |
| ignore/allow与禁称 | PASS | 功能diff新增ignore/allow=0，mir-client历史ignore属性7个，位置逐项保留；`attribute-scan.txt`；第14项exit0，人工核对否定边界，无正面越界结论 |

## M1–M4 与限制

| 项 | 本轮状态 | 依据 |
|---|---|---|
| M1 提交状态 | 维持闭环 | README E1/manifest publication及errata明确5abdcd3已提交；功能HEAD仍1fcf4fc，旧描述按快照解释 |
| M2 种子包路径 | 维持闭环 | README E2/manifest E2明确ObjectMonster直接apply_packet_at；测试:57–117与真实125/Remove encode→decode路径一致 |
| M3 历史补证 | PARTIAL保留 | E3附件存在且明确事后补录，原meta原字节未倒填，P7独立性缺口未被抹去 |
| M4 tracked基准 | 维持闭环 | E4/hash_basis说明功能提交基准；tracked=false不代表当前未跟踪；5个tracked blob重新核得一致 |

## PARTIAL P1–P7 与剩余风险

- P1：生产SkipFrames/ActionFeed未接，owner_skip_frames恒false。
- P2：命中后物理回收/事件表减容未做，append-only保留。
- P3：known_positions无容量上限/LRU，G3仅契约。
- P4：570素材属game-assets且不入库，真实渲染/设备可听未证。
- P5：严格NextFrame仅本模块570；R61 P01–P12按原编号承接，不推导全链完成。
- P6：R63整体仍PARTIAL；刑部终审已完成，本轮尚书ACCEPT-WITH-PARTIAL；门下确认未完成。
- P7：历史gate EXIT/HEAD原始meta缺项；事后附件与12 OK不能替代历史进程独立原始记录。
- 剩余风险：D3共享target陈旧二进制方法论风险继续披露；本次按要求复跑、未clean/重跑门禁，不扩称长期无flake或真实设备/容量已证。
- R64六候选与R61十二项台账继续承接；本轮无新增一致性必改，不授权扩实现或清零PARTIAL。
- 工作树初始另有两份未跟踪资源冻结JSON，保持原样；尚书结果交Lead，门下独立确认和最终push仍待办理。
