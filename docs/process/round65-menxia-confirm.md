# R65 门下独立确认（只读核验）

- 主单 `JJC-20261027-002`；确认意见：**PASS-WITH-PARTIAL，确认收口成立；必改 0**。仅确认当前阶段，不称最终通过。
- 基准 HEAD `dd7c47c`；本确认只写本文与 `target/round65/menxia/attempt-1/**`，未改 crates/旧证据/清单/看板，不 commit/push，不跑 `tools/dev/ci.ps1`，不执行 state/Done/confirm。
- 抽核时间：UTC `2026-10-26T20:31:26Z`；完整重算数据及 SHA 见本 attempt 的 `recompute.json`、`sha256.txt`。

| 项 | 独立抽核 | 结果 |
|---|---|---|
| 1 提交链/范围 | `git log --oneline -5`、四提交 `git show --stat`；`git diff 3730e2b..HEAD -- crates` | EXIT 0；顺序为 `3730e2b → ffed7a9 → 01b81ca → dd7c47c`，功能 diff 空；功能提交仅 cfg(test) 和新测试，未改生产逻辑。 |
| 2 唯一门禁 | 重算 log/meta bytes/SHA；核 meta 11 字段与日志摘要 | EXIT 0；log 206593B、SHA `3c9f40fc…64bfb3`，meta EXIT0/12 OK/0 FAIL/UTC `19:49:35Z–19:53:18Z`、gate_head `3730e2b` 全匹配；未重跑门禁。 |
| 3 文档/manifest | 复算 C/T/验收/manifest 与 9 artifacts 的 bytes/lines/SHA/tracked；解析 JSON | EXIT 0；C 10269/74、T 6494/39、验收 6363/46；九项 manifest 元数据全匹配；原八项相对 `ffed7a9` 的 bytes/lines/SHA 全不变。 |
| 4 BLOCK/M1 | 对照 T 原件、裁决及尚书验收，检查第九项 audit blob | T 仍明确唯一 M1 且 verdict BLOCK；尚书未改写其历史结论，裁决解释为口径澄清而非豁免。审计文档在 `01b81ca` 已跟踪，HEAD blob SHA 与清单一致；清单 `git_ls_files_at_audit:false` 是历史快照。 |
| 5 范围/PARTIAL | 检查 README、contract、计划与 PARTIAL 编号 | G1 仅单 UI 本地补证；不外推全局水位/容量/多消费者；G2 移出，服务端、素材音频、R61 P01–P12、R62 四项、R63 P1–P7、R64 六项继续承接。 |
| 6 卫生/真红 | `git diff --check`、`py -3 tools/docs/check_links.py`、JSON parse、新增 ignore/allow、wire diff；抽看 A/B 真红 SHA 与 C 记载 | diff-check/links/JSON EXIT0；新增 ignore/allow=0（历史7无新增），wire diff 空。抽查 A/B 真红原记录 SHA/退出与 C 报告一致；本次未重跑真红。 |

**M1 判断**
- 认可 CLOSED，范围是“无本地化依赖断言”，非“零中文诊断”。计划/门下 C6/契约未禁中文诊断；R64 可证中文诊断；未发现比较中文 UI 文案的断言。未借口径澄清抹除 T 的 BLOCK。
- 历史论据修正已落实：不再以未检出的 R62 测试或 R61 60 行终审证明历史意图；只依赖当轮明确约束、可证的 R64 中文诊断与断言实值。无反对项。

**时序、风险与边界**
- 提交时间顺序 `3730e2b` 03:49:26 → `ffed7a9` 04:01:36 → `01b81ca` 04:16:58 → `dd7c47c` 04:25:54（+08）；T 报告保留 BLOCK，随后裁决、尚书 accept；无先验收后补裁决/倒签迹象。
- 残余风险按既有 PARTIAL 台账继续；R66 建议先处理多消费者/全局最低水位与容量/服务端回收的契约、兼容矩阵及可失败测试设计，素材/真实设备音频仍单独核验，不将本轮局部补证泛化。
- 当前意见仅确认尚书 `ACCEPT-WITH-PARTIAL` 的限定 G1 收口成立；须看板 Done 后才可称最终通过。此确认官未执行任何看板操作。
