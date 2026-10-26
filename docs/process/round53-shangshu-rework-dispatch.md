# R53 尚书省补派文书

> 主单：`JJC-20261026-001`；补记基准 HEAD：`48aaec0`。本文为已登记子单的补记；Lead 已登记看板，本文不再运行 edict CLI。

## 一、背景与裁定

- R53-A 原审计 `docs/process/round53-aoi-acceptance.md`（52 行，SHA-256 前缀 `b9d725ca…`）结论为 **BLOCK**：八路由 PASS 2 / PARTIAL 5 / BLOCK 1。
- 唯一代码阻断：`crates/mir-server/src/world.rs` 的 run 路径漏调 `sync_player_players`；walk 路径已有该同步，Crystal 锚点为 `HumanObject.cs:2596-2602`。
- F2 已在 `world.rs:2241` 按 walk 同序补同步，并新增跑入/跑出双向真实 `World::run` 测试；已提交并推送 `48aaec0`。现有 F2 实测：aoi 8/8、round53_frost_pvp 7/7、round52_regression 8/8、fmt/clippy exit 0；证据在 `target/round53/f2/**`。
- 原审计中的其余 **5 条 PARTIAL 原样保留**；F2 修复不改变审计口径，也不自动改判其他路由。

## 二、处置决定

1. 唯一 run 玩家集合差分缺陷交兵部按 F2 修复；不回写或覆盖原审计 v1。
2. 刑部按 A2 独立复审 F2 真伪与逐路由证据，另写 `docs/process/round53-aoi-acceptance-v2.md`；以 v2 作为复审结果，v1 保持原样。
3. 5 条既有 PARTIAL 继续逐条列明依据与边界；不得因 F2 修复或局部绿灯改判为 PASS。
4. 不做 AOI 网格重写，不改变门、切图语义或其他行为口径。

## 三、子任务、写入范围与验收

| 子单 | 部门 | 交付与写入范围 | 验收 |
|---|---|---|---|
| `JJC-20261026-001-sub-7d64c20b`（F2） | 兵部 / deepseek | 已完成：仅 `crates/mir-server/src/world.rs` run 同步及 `crates/mir-server/tests/aoi.rs` 两个双向用例；证据 `target/round53/f2/**`。代码提交 `48aaec0`。 | 实测 `cargo test -p mir-server --test aoi -- --nocapture`（8/8）；`cargo test -p mir-server --test round53_frost_pvp -- --nocapture`（7/7）；`cargo test -p mir-server --test round52_regression`（8/8）；`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings` 均 exit 0。核验测试覆盖真实 `World::run` 的跑出/跑入、双向 `ObjectRemove`/`ObjectPlayer`、无幽灵且 visible 集合一致；变异反证有效。 |
| `JJC-20261026-001-sub-e8afe1c8`（A2） | 刑部 / gpt | crates 只读；另写 `docs/process/round53-aoi-acceptance-v2.md`，证据写入 `target/round53/a2/**`；不得覆盖 v1。 | v2 列出八路由矩阵及每项实际命令、exit、原始日志与 SHA-256；独立核实 F2 改动和测试证据。初步结论 PASS-WITH-PARTIAL（3 PASS / 5 PARTIAL / 0 BLOCK），最终以 v2 为准。资源依赖用例的跳过必须注明，不得作为通过。 |
| `JJC-20261026-001-sub-391bea87`（T） | 刑部 / gpt | 仅 Lead 唯一全量门禁完成后启动；按既有派单交付 `docs/testing/round53/round53-final-audit.md`、`target/round53/final-manifest.json`、`target/round53/summary.log`。 | 门禁后三件交付相互核对，绑定代码 HEAD、A/F 证据及门禁原始结果；不得提前启动或改写门禁结果。 |

## 四、串行与门禁顺序

1. F2 已完成并由 Lead 提交、推送 `48aaec0`；其证据留在 `target/round53/f2/**`。
2. A2 基于该提交树完成独立复审，写 v2 与 `target/round53/a2/**`；不得并行启动 T。
3. A2 后仅 Lead 执行唯一全量门禁，`attempt=1`：`pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`。记录精确命令、HEAD、起止时间、原始 exit 和日志 SHA-256；失败史原样保留，不覆盖、不伪装为通过。
4. 门禁完成后启动 T；随后由尚书验收，最后交门下确认。门禁后代码、测试或配置若有变化，须按 Lead 的门禁规则处理并更新证据绑定。

## 五、证据与禁令

- 每条验收命令的记录均须含代码 HEAD、原始 exit、起止时间及完整日志 SHA-256；测试用例实际执行清单须可复核。
- `0 tests`、filtered 用例或因缺少资源而跳过，均不得记作通过；特别标注 A2 依赖 `Server.MirDB` 样本缺失而跳过的用例及其影响。
- F2、A2 子单均已 CLI 登记；前次尚书 subagent 超时未交付，本单为补记，保留此前事实。Lead 已登记看板，本单不重复登记。
- 不修改 crates（F2 已按其子单完成并提交）；本补记只写本文，不 commit、不执行全量门禁、不写看板。

## 六、残余风险

- 5 条 PARTIAL 仍待各自证据闭环；F2 只解除 run 玩家同步这一项代码阻断，不代表 AOI 全量验收通过。
- 首帧按类型分组顺序与广播接收/发送顺序差异，列入 R54 跟踪；不得在本轮擅自改语义或声称逐包等价。
- A2 资源依赖用例因 `Server.MirDB` 样本缺失而跳过时，须在 v2 明示范围、理由和对结论的限制。
