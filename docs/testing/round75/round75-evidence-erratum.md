# R75 证据勘误：C-M1 / C-M2 闭合（第75轮）

- 性质：**只增不改**的补充记录；原文（C 报告 `round75-c-evidence.md`、A/B manifest 与日志、测试文件）均未修改。
- 依据：`docs/process/round75-lead-ruling.md`（R75-E1/E2 裁定）；门下 C5/C2 条件口径。
- 结论：两项证据必改均按「显式披露 + 事后只增补录 + PARTIAL」闭合；技术矩阵结论不变（84 passed、oracle 125 agree/0 mismatch、五变异一致）。

## E1：B aborted-attempt（WindowsApps python EXIT49）历史

- 窗口：UTC 2026-10-27T19:59:09Z（5 份副本创建完成）–20:00:33Z（wrapper 结束）；阶段：每副本 `mutation.sh` 注入 step；根因：裸 `python` → `WindowsApps\python.exe` AppInstallerPythonRedirector，EXIT49，apply 未执行（apply.log 0B）。
- 影响：m1–m5 的注入/红/恢复/绿四步未运行，**未产生有效红绿证据**，副本保持原字节；被 20:01 重跑覆盖的 `m1..m5/{green-norun.log,apply.log,summary.txt}` 与 `logs/mutation.m*.console.log` 原始字节不可恢复。
- 幸存凭证：`logs/run-all-mutations.log`（mtime 20:00:33Z、sha256 `65e5007a…`，含 5×(green_norun=0,inject=49)）与 `copy.m*.log`（19:59:09Z）。
- 补录：`target/round75/b/attempt-1/aborted-attempt-record.md`（3,134B/`cb2cc30b…`）、`.sha256`（92B/`d93dd089…`）、`attempt-log-amendment.md`（1,243B/`80f9966d…`）。
- 状态：PARTIAL（不可恢复字节不伪造），只增补录+原 manifest 披露已闭合；不影响最终矩阵、oracle、回归与卫生结论。

## E2：A「冻结早于首次 actual」时间序声明

- 声明：expected 冻结 19:31:57Z 早于首次 actual 19:35:21Z；现存日志最早 19:42:01Z（最终七绿）。
- 事实：早期同名日志被 19:42 最终重跑覆盖、无备份；首轮 2 红未落盘；`target/debug/deps` 编译产物被重编译覆盖。
- 冻结事实（可独立复核）：`frozen/expected.json` 12,630B/sha256 `e9fb241c…`、`generated_utc=2026-10-27T19:31:57Z`、mtime 同值；`frozen/frozen_consts.rs.txt` 2,561B/`09756835…`；夹具四文件同批 mtime。
- 旁证：`first-actual-artifacts/`（首轮 G6 临时夹具目录副本 mtime 19:35:43Z + PROVENANCE/SHA256SUMS），仅证「19:35:43Z 发生过执行」，不替代 stdout。
- 补录：`target/round75/a/attempt-1/first-actual-claim-amendment.md`（3,210B/32 行/`26eaf4fa…`）、`.sha256`（125B）。
- 状态：声明级 PARTIAL，不伪造原始日志；冻结文件事实不变，不影响 7 绿/五负控/回归有效性。

## 边界

- 本勘误不改变 C 报告原 FAIL 结论与其审计计数；C2 复核仅针对 E1/E2 闭合与「只增不改」事实。
- 测试文件 SHA `ecff550d…`、两 freeze（`70845aef…`/`c7d896e7…`）、A/B manifest（`c10a84fe…`/`a881667d…`）均未变。
- 唯一全量门禁仍由 Lead 在提交测试文件后执行；本轮不称生产寿命已修复或与 Crystal 严格等价。
