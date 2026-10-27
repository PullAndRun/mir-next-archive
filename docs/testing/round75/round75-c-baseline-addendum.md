# R75-C3 刑部基线追加复核

- 结论：**PASS-WITH-PARTIAL；必改 0；任务书基线冲突 CLOSED**；允许 Lead 提交测试后进入唯一全量门禁，本报告不替代门禁。
- 新基线勘误实算：7 行／1,560B／SHA256 `82cf8b52355e2731dd3286f61a3aa0a61b1ebb851ca18148c75086a085e5f32a`；已解释 3,216B／`60046058…` 为撰写中间快照，定稿为唯一权威，未扩称技术结论或改写 C/C2 历史结论。
- A 定稿实算：32 行／3,210B／SHA256 `26eaf4faa657661f2dea8aeaf27e9d0165973d7d5defd743358f68f410046224`，与 .sha256 一致；PROVENANCE、SHA256SUMS、四夹具均存在且 SHA/bytes 保持 C2 实算值。
- C2 两 FAIL：`bytes-sha256:target/round75/a/attempt-1/first-actual-claim-amendment.md`、`task-A-hash-discrepancy`；均期待 3,216B／`60046058…`，实测已为定稿，唯一基线缺口由新增勘误闭合；其余 45 PASS 不受影响。
- 只增不改：全量复算 C2 所列文件及 17 份历史日志；测试、两 freeze、A/B manifest、A/B README、B wrapper、B 三补录、C 原报告均匹配 C2 原实算 SHA/bytes；两项 crates diff 均空，status 仅新增 A 测试、两 freeze 与 R75 docs。
- C2 报告实算：18 行／3,207B／SHA256 `f14bc7fb22eb6d264ed55a68dfa0a81ec7509bf60cf8c5a20d6403ba32f63fc1`，历史 BLOCK 原文保留；C2 checks 原文未改并已记录实算 SHA。
- 残留：E1 维持 **CLOSED-AS-PARTIAL**（不可恢复的 aborted-attempt 原字节）；E2 维持**声明级 PARTIAL**（首次 actual stdout 缺失，夹具旁证不替代 stdout）；其他历史 PARTIAL 原号承接。
- 限定：中间快照经过属 Lead 归因说明，本次不重建其原字节；勘误“04:31Z”与定稿记录 20:31Z／本地 04:31 时段有标注瑕疵，不影响版本身份和闭合结论，非新增必改。
- 核验：L0 静态复核，55 项 PASS／0 FAIL，实测 269ms（哈希与断言脚本）；逐项实算值与 UTC 见 `target/round75/c3/attempt-1/checks.json`。
- 边界：仅新增本报告、C3 checks 与 logs；不修改任何既有文件、不 commit/push、不运行 ci.ps1、不重跑矩阵；不称生产寿命已修复、Crystal 严格等价或扩大迁移结论。
