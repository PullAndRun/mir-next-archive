# R63 门下省独立确认

- 结论：**PASS-WITH-PARTIAL 确认收口成立（必改 0；仅 G1+G2，R63 整体仍 PARTIAL）**。
- 抽核时点：2026-10-27 北京时间；当前 HEAD=`899b22edd2926f8775cac407f2798ec1fd75edd7`；尚书复验基准=`97714865959626ce93f309ec06d3fb2ce3089d92`，其后仅追加尚书定稿文书。
- 刑部 PASS-WITH-PARTIAL；[尚书定稿](round63-shangshu-acceptance.md) ACCEPT-WITH-PARTIAL（必改 0），S1/S1b/S2 闭环；本文是独立文书确认，不代表看板状态已转换或已 push。
- [原始证据目录](../../target/round63/menxia/attempt-1/)；复算命令：`PYTHONIOENCODING=utf-8 py -3 'target/round63/menxia/attempt-1/independent-recheck.py apparent'`（exit0）。

| # | 抽核项 | 命令、证据与结论 |
|---|---|---|
| 1 | 门禁 | Python `read_bytes/hashlib.sha256` 与脚本第9项：190207B，SHA256 `15adfa307592ef1ab3ea4cec4df248a852c62c814974c8d5043c8f52e2ac1656`；原日志末尾汇总独立计得12 OK；见 [原字节摘要](../../target/round63/menxia/attempt-1/test-gate-link-summary.txt)。meta 60B仅UTC起止；E3附件为Lead事后补录WRAPPER_EXIT=0/HEAD，非倒填，不能独立证明历史进程EXIT，P7保留。 |
| 2 | 提交 | `git log --oneline -6`，另以`-7`补齐功能提交；链f80724e→1fcf4fc→5abdcd3→0a44ace→9453e32→9771486→899b22e。`git diff --stat f80724e..1fcf4fc`为5文件+874/−66；`git diff --name-only 1fcf4fc..9771486`为11条docs；`git diff --name-only 1fcf4fc..HEAD -- crates`为空；见[提交证据](../../target/round63/menxia/attempt-1/git-log.txt)。 |
| 3 | 产物 | 独立复算5份交付SHA见下表；`git show 1fcf4fc:<path>`核5功能blob，全部与当前字节相同；manifest 17 artifacts逐项size/SHA及登记行数mismatch=0；[复算明细](../../target/round63/menxia/attempt-1/manifest-recheck.json)。生成器与冻结清单复验exit0。 |
| 4 | 红线 | `git diff --unified=0 f80724e..1fcf4fc -- crates`新增ignore/allow=0；`git grep -n -E`核历史ignore属性7个，见[属性扫描](../../target/round63/menxia/attempt-1/attributes.txt)。脚本第14项禁称扫描exit0，人工核命中仅否定/历史引用，正面越界0；P1–P7不清零。 |
| 5 | 阶段 | 人工核README:3/87、roadmap:234/245、manifest publication/P6/verification及尚书:20；[阶段摘录](../../target/round63/menxia/attempt-1/stage-scan.txt)。冻结快照“尚书待复验/门下未确认”由后续定稿及本文顺序承接；有效阶段字段“终审尚未进行”0处，审计文书历史引用保留；push待Lead。 |
| 6 | 确认 | `R63_LOG_DIR=target/round63/menxia/attempt-1/checks bash docs/testing/round63/run-checks.sh` exit0：16 PASS/0 FAIL/0 SKIP，12秒；lib24/wire3/R59 16/R61 11/R62 5均0失败0忽略，fmt/clippy exit0；[原始输出](../../target/round63/menxia/attempt-1/run-checks.log)。仅确认限定交付，三确定性测试点绿不外推无flake。 |

| 交付原件 | 行/字节 | 本次SHA256 |
|---|---|---|
| [README](../testing/round63/README.md) | 87 / 8060 | `cae217cadcd0d9518bdfb36d72b8f013a3f8246ce265117034d41340f82dd426` |
| [final-manifest](../testing/round63/final-manifest.json) | 439 / 15477 | `8cd5bad7312d2f2e4726c2dfdfe4fa53860816700d754e8e2fc003fa7d3aff37` |
| [commit-tree-hashes](../testing/round63/commit-tree-hashes.txt) | 13 / 1311 | `86f6c559df6c305579da31713269b080947f4bb29ab67bebaebaed24000067cf` |
| [刑部终审](../testing/round63/round63-final-audit.md) | 48 / 4709 | `bd7863a0043668f1f259bb78f8d8fade22d8d5edd0d2e0e607bd7bf3915762b3` |
| [尚书验收](round63-shangshu-acceptance.md) | 57 / 6174 | `fcb4428ad2804071b46b73ab871160414e4b97dae07ef8bf498f37921dd803de` |

- 登记差异：HEAD已推进899b22e；manifest实测15477B而非交接疑值15352B。本次链接检查exit0、1834相对链接/2 README，与尚书1844计数不同；均按各次输出记录，不回写历史证据。
- P1：生产SkipFrames/ActionFeed未接，owner_skip_frames恒false。
- P2：命中后物理回收/事件表减容未做，维持append-only。
- P3：known_positions容量上限/LRU未做，G3仅契约。
- P4：570素材属于game-assets且不入库，真实渲染/设备可听未证。
- P5：严格NextFrame限本模块570；R61 P01–P12按原编号承接，不整项清零。
- P6：R63整体PARTIAL；刑部/尚书已出限定结论，本次门下文书确认成立，看板转换与发布待Lead。
- P7：历史gate原meta无EXIT/HEAD；E3事后附件与12 OK不能替代原始独立进程记录。
- R64交接：[roadmap十三](../roadmap.md#十三第-64-轮r64候选)六候选继续登记：生产来源、回收/容量、550场景、索敌切换、掉落表、治理收口；R61十二项台账维持。
- 剩余风险：D3共享target陈旧二进制方法论风险保留；本次未clean或重跑Lead门禁，不外推真实设备、容量或长期无flake。
- 权限：仅新增本文及指定target原始输出；未改crates/看板、未commit/push、未运行Lead门禁；初始两份未跟踪资源冻结JSON保持原样。
- 流转交Lead：主任务`JJC-20261026-010`按真实readiness执行`done --as shangshu`→Review→请求Done→PendingConfirm→指定门下`confirm approve`；本文不代执行CLI。
