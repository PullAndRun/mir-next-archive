# R63 刑部独立终审

- verdict：**PASS-WITH-PARTIAL，仅限 G1+G2；R63 整体仍 PARTIAL**。
- 审计时点：2026-10-26T16:03:16Z 起；HEAD `5abdcd3371aa3dab1919fa79c5387ad1be68ca1e`。
- 功能对象：`1fcf4fccaf4177475af971dc50db69ffc3a3d1d9`；基线 `f80724e`；文档批 `5abdcd3`。
- 方法：独立读代码/Crystal、重算原字节和提交 blob、执行指定复核；未重新执行 Lead 门禁或历史红侧。
- 本报告不代替尚书验收、门下确认或 Lead 发布决定；原 README 的“终审尚未进行”为审计前快照。

| # | 必审项 | 判定 | 独立证据/限制 |
|---|---|---|---|
| 1 | 门禁与提交 | PARTIAL | 功能 5 文件 +874/−66；后一批 10 路径全 docs、crates 零差异；gate 190207B/SHA 匹配、末尾 12 OK；meta 无 EXIT/HEAD，见 M3 |
| 2 | 产物哈希 | PASS | 5 个交付文件字节/SHA 全匹配；JSON 可解析，17 artifacts 全匹配；5 tracked 的 git show blob SHA 全匹配 |
| 3 | 测试真实性 | PASS | 3 测试名与 G1/G2 对应；对象9七项断言、99负例均存在；新增 ignore/allow=0，历史真实属性7个，见注1 |
| 4 | 红绿证据 | PASS | B 红：1p/2f、exit101、L182 571≠570/L290 1≠2；SHA=6438cce5…；A 红2个断言+exit101；本次绿24/3/16/11/5 |
| 5 | 文档一致性 | PARTIAL | run-checks 16/16、fmt/clippy exit0、链接 exit0（1831相对链接/2篇README引用）；README明确审计前待办；出版状态与种子包路径措辞待修 M1/M2 |
| 6 | 偏离准确性 | PASS | D1 stale值013c475…存在；D2中间稿0bf883…仍在，final=6438cce5…；D3有C日志佐证；D4仅核披露，不重做clean |
| 7 | 禁称扫描 | PASS | 改动文件与交付文档正面禁称0；命中均属否定/禁称清单/扫描规则，人工复核上下文 |

注1：ObjectProjectile(125) 与 ObjectRemove 真实 encode→decode；ObjectMonster 种子直接 apply_packet_at，不能声称三者均经 wire。

必改/建议（文书与证据闭合；未发现阻断 G1/G2 的实现缺陷）：

- M1【必改，轻微】：README:14、manifest:21/237 的“文档未 commit”是创建时快照，需注明快照或更新为已存在的 `5abdcd3`；勿将功能 head 改成文档 head。
- M2【必改，中等】：测试头:15–17、B summary:41 的“所有包均 encode→decode”超出实现；最终文书须明确 ObjectMonster 为包处理器种子、125/Remove 为真实编解码。保持已冻结测试/历史证据原字节并用补充勘误定位。
- M3【建议补证，证据 PARTIAL】：meta 原文只有 UTC_START/UTC_END；12 OK 可独立核实，历史进程 exit=0/执行时 HEAD 仅见 manifest/报告声明。请附运行器原始记录；禁止倒填历史 meta 冒充原始证据。本审计未宣称独立证明该历史进程退出码。
- M4【建议】：manifest 的 tracked 字段实际表示“在功能提交1fcf4fc中跟踪”；文档提交后仍为 false，应明确该基准，避免误读为当前 git 跟踪状态。

PARTIAL 清单（沿用原编号，不得以本终审消除）：

1. P1：生产 SkipFrames/ActionFeed 未接，owner_skip_frames 恒 false。
2. P2：命中后物理回收、事件表减容未做，append-only 保留。
3. P3：known_positions 无容量上限/LRU，G3 仅契约。
4. P4：570 素材属于 game-assets、不入库，真实渲染/可听未证。
5. P5：严格 NextFrame 限本模块570，R61十二项边界继续承接。
6. P6：R63整体 PARTIAL；本次仅刑部终审，尚书验收/门下确认仍待处理。
7. P7：历史 gate 进程 EXIT/HEAD 原始元数据缺失，不能由12 OK替代进程退出证据。

证据根：`target/round63/audit/attempt-1/`。

- `manifest-recheck.json` / `hash-command.log`：5份文档和17项产物逐条复算、5 blob对照、原始meta。
- `function-stat.log` / `function-numstat.log` / `docs-names.log` / `docs-crates-diff.log` / `function-diff.patch`：提交范围原始输出。
- `run-checks.log` / `checks/1.log`–`checks/16.log` / `checks/summary.tsv`：本次独立复跑原始输出，16 PASS/0 FAIL/0 SKIP，12秒。
- `test-and-gate-review.json`：计数/历史测试名16/11/5保持不变、fmt/clippy输出、meta缺项记录。
- `attributes-scan.log` / `claims-scan.log`：git grep与新增行扫描、人工上下文判读。
- `gate-decoded.log` / `links-decoded.log`：原日志非UTF-8的显示副本，原字节不改；哈希仅按原字节。
- `evidence-notes.md`：三测试映射、红绿与偏离逐项定位；`code-review-report.html` 为技能附属报告。

评分（仅限已审范围）：质量8/安全8/性能7/可维护性8，总体8/10；未发现严重实现问题，存在上述文书精确性问题。
