# R75-C2 刑部追加复核

- verdict：**BLOCK；证据必改 1**（同一 A 补录基线冲突）；技术 PASS 原结论不变，原 C 报告 FAIL 历史原文不改。
- 检查窗：2026-10-27T20:32Z–20:35Z；47 项检查，45 PASS／2 FAIL，两条 FAIL 同指一项基线冲突；逐项实算 bytes/SHA/UTC 见 `target/round75/c2/attempt-1/checks.json`。
- E1/C-M1：**CLOSED-AS-PARTIAL**；根因/窗口、注入及红/恢复/绿未运行、无有效矩阵证据被毁、不可恢复、幸存凭证、重跑归属与「不臆造原文」齐全。
- E2/C-M2：声明级 **PARTIAL 内容可接受，身份基线未闭合**；旁证明确不替代 stdout，冻结事实仍成立，不影响技术矩阵。
- 唯一必改：任务指定 A amendment 3,216B/SHA `600460581f3ca1eba26227bf0f903862a1ba43b299a6e341addc3b5b4f8de8b6`，实物为 3,210B/SHA `26eaf4faa657661f2dea8aeaf27e9d0165973d7d5defd743358f68f410046224`；Lead 裁定/erratum 引用后者。须新增基线勘误解释差异后追加复核，不得覆盖任一版本。
- B 新增实算：aborted record 3,134B/`cb2cc30ba149a4ffdaccc4de665b5023cb46f50d844ecdb8e1f76a4c3f3f7765`；其 .sha256 92B/`d93dd089c62bf4cd3d13b9f44d9aad3b810ef831a8ce802775be73306d78d519`；amendment 1,243B/`80f9966daf91d011f9a79fcc9a6caa4b05b3cfd9c526e9ffceeae1d323a4a233`。
- A .sha256 实算 125B/`1851f73c051817b0526bc6855bd06d24b43332cc34acd11d23a2d2ac972a07a8`，内容对应实物 26eaf4fa；PROVENANCE/SHA256SUMS 与四夹具存在，四 SHA/bytes/mtime 均匹配清单。
- 冻结 expected 实算 12,630B/`e9fb241ce4b5922caa765b61f66769b8fb937da9475d2ba1007e65a88ce8458a`，generated_utc=19:31:57Z，mtime=2026-10-27T19:31:57.7711079Z。
- 只增不改：**既有基线 PASS**；C 原报告 39 行/5,365B/`8d9bb8fcc97057b844f6ed02c530d180804440a4c32a8179e4817ed338fbb529`；A/B 既有 README、B wrapper 与 C audit 的 17 份 A/B 日志均匹配，实算值见 checks。
- A manifest 27,075B/`c10a84fe46c5233c9530f73dbae126d2949e0996576ee5cec76fa87ea4b73520`；B manifest 10,461B/`a881667d7f2ac76fc345fb0c93ba7f3feef1503343c61378f70679c3f3e42dc1`。
- 测试 56,403B/1,454 行/`ecff550dfa0bcd671d32723bd18fbcd444c93c4d173c1562bfbc0f8ad185a05c`；无 `#[ignore]`/`#[allow]`。
- freeze-before 44,844,165B/`70845aefd4dd689a55ab0618b36bbf692a4b33ebb5ee2f3a662dedc17a3dc9de`；freeze 45,219,533B/`c7d896e7f5eee4706b943912c0c564d10d61c98d163143f051c2d632daecde32`。
- 主树 status 仅 A 新测试、两 freeze 与新增 C 报告/erratum/裁定；`git diff -- crates`、`git diff --cached -- crates` 均 EXIT0/空；本官仅新增本报告、checks 与核验日志脚本。
- C5/C2 处置为披露历史缺口并承接 PARTIAL；不伪造重写，不将目录 mtime 升格为首次 stdout。Lead/erratum 与现存补录内容一致，但不能消除上述任务基线冲突。
- 历史 PARTIAL 按原 C 报告第37–39行全部原号承接；E1 不可恢复字节、E2 首次时序仍为 PARTIAL，不称生产寿命修复/Crystal 严格等价/Blizzard 迁移。
- 边界：本次只读复核，不重跑矩阵、不改既有文件、不 commit/push、不运行 ci.ps1；唯一全门禁仍由 Lead 在提交测试后执行，本报告不替代门禁。
