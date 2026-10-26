# R66 门下省独立确认
- **verdict：PASS-WITH-PARTIAL；确认收口成立；BLOCK/必改 0。** 仅本地测试/契约层，不外推生产能力。
- 主单 JJC-20261027-003；独立确认官 gpt-6.1-sol / menxia；开始 2026-10-26T22:18:31Z，抽核完成 22:24:22Z。
| 抽核项 | 实值与结论 |
| --- | --- |
| HEAD/卫生 | 7374422db0e6fbc6a0b5c10327b64948bf31b96d；除本文外仅两个历史 freeze JSON 为??；无其他脏项。 |
| crates | git diff b998742 -- crates、02ca88b..7374422 -- crates 均空，命令 EXIT0。 |
| 尚书验收 | 35行/4813B；SHA256 11b1f3458822503574b62b0f3f527fe75c726dc9b08d33998f600bac328d1745，吻合。 |
| 门禁原件 | log 3549行/213139B；SHA256 1d04184a0de71f36f454229177a237847912507a34d3ac9b8bdab7a94891d524。 |
| meta/摘要 | 15行/454B；SHA256 6bcd9d4f4aff097e6cb8c94eedf68f9d896f434827bbc18f5e36c69ecc93024e；HEAD b998742、EXIT0、12 OK/0 FAIL、operator lead、21:48:08–21:51:46Z一致；未重跑ci。 |
| post-audit清单 | 15项/178行/6792B；SHA256 cc8d01bcdcebb85b4ca297903c69a4ece84b8009b50519576dd202086b4e0673。抽6项全部bytes/lines/SHA吻合。 |
| tracked抽核 | README 7402B/45181b51…593b1；watermark 3422B/eec8edd6…72bc8；C 8786B/36f1665c…705dc；audit 6537B/55cc0c23…fc253；均git show b3ce9dc，blob匹配。 |
| audit归属 | audit_commit=dde1af7；该提交与b3ce9dc audit blob同为e6345158c3a1ed7463f84b981eaffcdd94c4c6d7；另2项untracked抽gate log/meta工作树原字节。 |
| A真红原件 | m1 EXIT101、实际1 failed，contract:252 Ok(4)/Ok(2)；隔离副本/最终测试SHA同为62f36a3923d682bc01dcd8a53446c09add97d68e588e7654bbb796b66acd54eb。 |
| B真红原件 | m1 EXIT101、实际2 failed，boundary:190 suffix 0/2；隔离副本/最终测试SHA同为727d25f5a0dac0c475c6de9fd09f9bd32a5419707fdd57bace9ffabdefe76d52；均非编译错/0tests。 |
| 独立复跑 | 全新绝对target/tmp，cargo test -p mir-client --test round66_watermark_contract --test round66_consumer_boundary；EXIT0，contract3+boundary2，合计5 passed/0 failed/0 ignored。 |
| 运行时间/成本 | L2，2026-10-26T22:23:45.119Z–22:24:22.254Z，37135ms；静态不能替代运行，不能再降档；未复做变异或全量门禁。 |
| 诚实性 | 实读README140行、水位60行、C61行及T/尚书全文；无生产多消费者/全局水位/125全链已实现等正面断言；门禁PASS≠最终验收、旧FAIL为历史仍明确。 |
| 看板 | show/list实查：父Review，todos=[]；A/B/C/D四子单Done且结果非空，仅D一个todo=completed；无新子单/孙代，不以ready_to_close单字段代替。 |
- 裁定采纳：A旧v1首败覆盖为已披露非阻断保全偏差，不改称旧首败保全；C独立attempt-2/3补强。B硬链接假红clean已消除，本次新target5绿；F1全集合分阶段优先级已修复且交叉测试实绿；命名偏差不影响定位。
- PARTIAL：生产注册/退出/租约/补发/并发fan-out、全局最低水位、事件容量、known_positions容量/LRU、服务端回收、u64::MAX恢复、素材与真实可听/Live不在本次完成范围。
- 原编号继续：R49–R60/R60 P-1–P-6；R61 P01–P12；R62①–④；R63 P1–P7；R64①–⑥；R65 G1.2仅值独立/G2未做；无清零。
- 证据：[抽核实值](../../target/round66/menxia/attempt-1/checks.json)、[命令双流/退出码](../../target/round66/menxia/attempt-1/commands.json)、[独立复跑](../../target/round66/menxia/attempt-1/test.json)。完整SHA见抽核实值，表内短SHA仅显示用途。
- 审计工具偏差：首次glob超时；骨架文件名误加后缀，核实绝对路径后仅更正文件名；首次清单读取误指target返回NOT_FOUND后改读docs真实清单；均非产品失败，未改上游文件。
- CLI：判决先落盘；2026-10-26T22:25:17Z三步真实执行，state Done EXIT0→PendingConfirm（confirm_by=menxia）；confirm approve EXIT0→Done；validate EXIT0（206任务符合schema v3.1）；最终show EXIT0/Done/todos=[]。见[原始回执](../../target/round66/menxia/attempt-1/cli-receipts.json)，无重试。
- 完成：2026-10-26T22:25:17.570Z；[SHA清单](../../target/round66/menxia/attempt-1/sha256.json)包含本文及原始证据摘要；最终本文SHA另记录，避免自引用。
