# R75 Lead 裁定：证据缺口 C-M1 / C-M2 处置（第75轮）

- 依据：刑部 C 报告 `docs/testing/round75/round75-c-evidence.md`（39 行/5,365B/sha256 `8d9bb8fcc97057b844f6ed02c530d180804440a4c32a8179e4817ed338fbb529`）——技术矩阵 PASS（84 passed、oracle 125 agree/0 mismatch、五变异红实值一致），但证据必改 2 项；门下条件 C5「证据只增不改」、C2「冻结断言/金样 SHA」。
- 本裁定只处置**证据记录/披露**，不改实现、测试、历史日志；功能结论不变。

## R75-E1（B：aborted-attempt 日志被覆盖）

- 事实：B 矩阵首次尝试在注入阶段因裸 `python` 解析到 WindowsApps 重定向器 EXIT49 中止（窗口 19:59:09Z–20:00:33Z）；m1–m5 的 `--no-run` 已 EXIT0，但注入/红/恢复/绿四步未运行，该 attempt **未产生任何有效红绿证据**；其 `green-norun.log/apply.log/summary.txt` 与 `console.log` 被 20:01 重跑同名覆盖、原始字节不可恢复；B 已在原 manifest `deviations` 主动披露。
- 闭合材料（只增不改）：`target/round75/b/attempt-1/aborted-attempt-record.md` 3,134B/sha256 `cb2cc30ba149a4ffdaccc4de665b5023cb46f50d844ecdb8e1f76a4c3f3f7765`；`.sha256` 92B/sha256 `d93dd089c62bf4cd3d13b9f44d9aad3b810ef831a8ce802775be73306d78d519`；`attempt-log-amendment.md` 1,243B/sha256 `80f9966daf91d011f9a79fcc9a6caa4b05b3cfd9c526e9ffceeae1d323a4a233`；既有 manifest（`a881667d…`）、README（`d7e62e60…`）、`run-all-mutations.log`（`65e5007a…`）mtime/SHA 均未变。
- 裁定：C5 的规范意图是**有效证据不得被改写/删除**；环境失败 attempt 未生成有效证据，其处置方式为显式披露+事后只增补录，已满足。**E1 记为流程 PARTIAL（不可恢复字节不伪造），作为必改项闭合，不阻断提交。**

## R75-E2（A：首次 actual 时间序声明）

- 事实：A manifest/README 称 expected 冻结 19:31:57Z 早于首次 actual 19:35:21Z；现存日志最早 19:42:01Z，缺 19:35Z 原始 stdout；A 已确认早期同名日志被 19:42 最终重跑覆盖。
- 闭合材料（只增不改）：`target/round75/a/attempt-1/first-actual-claim-amendment.md` 3,210B/32 行/sha256 `26eaf4faa657661f2dea8aeaf27e9d0165973d7d5defd743358f68f410046224`；`first-actual-artifacts/`（首轮 G6 临时夹具目录副本 + `PROVENANCE.txt` + `SHA256SUMS`，目录 mtime 19:35:43Z，仅作旁证）；冻结事实证据仍成立：`frozen/expected.json` 12,630B/sha256 `e9fb241ce4b5922caa765b61f66769b8fb937da9475d2ba1007e65a88ce8458a`（`generated_utc=19:31:57Z`、mtime 同值）。
- 裁定：「冻结早于首次 actual」**降级为声明级 PARTIAL**，不伪造原始日志；冻结文件内容/时戳事实不变；不影响 7 绿、五负控、回归与门禁结论。**E2 作为必改项闭合。**

## 执行边界与承接

1. 不修改 `docs/testing/round75/round75-c-evidence.md` 原报告与任何 A/B 既有证据；本裁定与 `docs/testing/round75/round75-evidence-erratum.md` 为新增记录。
2. 修正后由 C 以追加核验（C2）复核 E1/E2 闭合；C2 通过后 Lead 提交测试文件并运行唯一全量门禁（不 SkipTests/不 SkipPostgres）。
3. 遗留 PARTIAL 原号承接 R76；后续轮次如需修复历史证据规范，须另建任务，不得回改本轮原字节。
