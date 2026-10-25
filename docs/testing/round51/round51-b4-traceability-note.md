# R51-B4 追溯性校正说明（T4 严格 F 条款）

- 时间：2026-10-25（Asia/Shanghai），Lead 记录。
- 背景：T4 首次复审发现 `target/round51/b4/00-summary.md` 与 `07-seven-row-matrix.md` 头部把 B4 子单误写为 `JJC-20261025-003-sub-28585983`；该编号实为 **R51-B2**。同时 `docs/testing/round51/README.md` 在 B4 提交后仍写“B4 待提交/待 attempt7”，属索引滞后。

## 权威子单映射（本轮）

| 角色 | SUB_TASK_ID | 状态 |
|---|---|---|
| R51-B2 零库存边界返工 | `JJC-20261025-003-sub-28585983` | Done |
| R51-B3 P03/P10 首修 | `JJC-20261025-003-sub-3b310e70` | Doing（结果待 Lead 以 main 回填） |
| R51-B4 耐久过滤与包序断言 | `JJC-20261025-003-sub-e2cb39cb` | Doing（结果待 Lead 以 main 回填） |
| R51-T3 B3 后独立复审 | `JJC-20261025-003-sub-d0ea04bd` | Doing（结果待回填） |
| R51-T4 B4 后独立复审 | `JJC-20261025-003-sub-1c3b4c52` | Doing |

## 事实

- B4 代码提交：`0d27e5e`（`crates/mir-server/src/magic.rs` +299/−10，36 magic 测试全绿）。
- B4 后提交树：`edad41b`（含 T3 v3 审计产物与 B4/T4 派发）。
- 全量门禁 attempt7：`target/round51/final-gate7.log` 189,032 B sha256 `f734bc55c78e5e718c7ebf5f463af2093ed6ff1124193dc965fd94f366f17477`；`target/round51/final-gate7.meta.txt` 含 command、cwd、start/end、duration、exit_code=0、head=edad41b，12/12 项 OK。
- 校正方式：B4 证据原件保留，仅追加“校正附录”明确真实子单；本文件与 README 索引同步更新，不重写历史文件。

## 追溯结论

B4 交付内容与证据归属 `sub-e2cb39cb`；B2 的 `sub-28585983` 不应被当作 B4 的验收对象。后续 T4 复审以本文件映射为准。
