# R53 尚书省派发单

> 主单：`JJC-20261026-001`；基准 HEAD：`2c3bccf`。依据 `docs/process/round53-plan.md` 与 `docs/process/round53-menxia-review.md`。门下结论为 PASS-WITH-CHANGES；批准推进不等于实现或 AOI 验收已通过。按必改 1–5 执行，验收口径不扩义。

## 一、结论与范围

按门下批准派发 F（PvP FrostCrunch）、A（AOI 审计）、T（终审），并由 Lead 执行 G 门禁。F：deepseek 编码/测试，独立 gpt 评审；A：gpt 只读审计；G：Lead 唯一提交与全量门禁；T：gpt 在门禁后终审。依赖严格串行：F 冻结并由 Lead 提交后，A 才基于该提交树审计。

## 二、执行子任务

| 子任务 | SUB_TASK_ID | 部门/模型 | 写入范围与交付 | 依赖 |
|---|---|---|---|---|
| F / PvP Frost | `JJC-20261026-001-sub-61e3f0eb` | 兵部；deepseek 编码 + 独立 gpt 评审 | 改 `crates/mir-server/src/magic.rs`；新建 `crates/mir-server/tests/round53_frost_pvp.rs`；原始证据置于 `target/round53/frost-pvp/**`。 | 无；F 完成后 Lead 提交 |
| A / AOI 审计 | `JJC-20261026-001-sub-0ac5e1d8` | 刑部；gpt | crates 只读；写 `docs/process/round53-aoi-acceptance.md` 与 `target/round53/aoi-audit/**`。按路由列 Crystal 行为、Rust 实现、测试名/命令、原始日志、结论。 | F 提交树冻结 |
| T / 终审 | `JJC-20261026-001-sub-391bea87` | 刑部；gpt | 门禁后写 `docs/testing/round53/round53-final-audit.md`、`target/round53/final-manifest.json` 与 `target/round53/summary.log`；报告与 summary log 各 ≤1 MiB。 | Lead 唯一全量门禁完成 |

## 三、串行波次与职责

1. **F**：deepseek 编码并跑定向测试、留原始证据；完成后独立 gpt 评审；由 Lead 检查并作唯一提交。
2. **A**：在 F 提交树上只读 crates，完成 AOI 逐路由矩阵；缺覆盖/实测证据按 PARTIAL 报告，不改代码。
3. **G**：仅 Lead 执行唯一全量门禁：`pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`。方案误拼 `-KillSlale`，实际参数采用 `-KillStale`。失败 attempt 原样保留，不覆盖或伪装为 PASS。
4. **T**：门禁后核验代码 HEAD、A/F 证据与过程材料，生成终审报告及清单/summary log。
5. **尚书验收 → 门下确认**：尚书按提交、原始证据及 PARTIAL 口径验收；请求完成后由指定门下确认。门禁后代码/测试/CI/配置再变更，Lead 重跑受影响门禁并更新绑定证据。

## 四、子单验收命令与证据

### F

至少新增并实际执行以下具名测试：
- `round53_frost_crunch_hits_pk_eligible_player`
- `round53_frost_crunch_rechecks_pk_at_impact`
- `round53_frost_crunch_player_level_and_probability_edges`
- `round53_frozen_by_frost_blocks_real_player_attack`

命令：
- `cargo test -p mir-server --test round53_frost_pvp -- --nocapture`（非零执行、无 ignore/filter；拒绝路径断言被删时应失败）
- `cargo test -p mir-server --test round52_regression`
- `cargo test -p mir-server --lib magic`

依门下必改项核验真实 cast→延迟 tick→玩家 HP 下降且结算前未扣血；施法后 PK 资格变化、安全区、换图、死亡、落点距离 2/3 格边界均拒绝且 HP/状态不变；等级差 +2、概率成功/失败两侧、Slow 4s / Frozen 2s、Frozen 后真实 `World::attack` 拒绝与到期恢复。固定种子须报告命中及未命中计数，怪物回归保持通过。

### A

命令：
- `cargo test -p mir-server --test aoi -- --nocapture`
- `cargo test -p mir-server random_operations_match_bruteforce_scan -- --nocapture`

逐路由审计并绑定实际测试名与回执；检查进图/离图、移动/跑步/切图、刷出/回收、广播接收者与顺序、视野边界/双向差分、门状态。逐项附原始命令、exit、HEAD、完整日志及 SHA-256；静态核对、仅测试数量或缺覆盖不可作 PASS。

### F/A/T 共通证据

`target/round53/**` 留存原始 stdout/stderr、精确命令、exit code、代码 HEAD、用例实际执行列表、起止时间及日志 SHA-256。清单能复算报告结论；不可只留摘要或覆盖失败日志。

## 五、写范围与禁改

- F 不得改 Crystal 公式、技能等级边界、PK 规则或 RNG 序列；不扩展 `EnemyGuild`。
- A 仅可读 crates；A/T 不改任何 crates、不 commit、不运行全量门禁。A/T 文档及证据限定于各自范围。
- Lead 是唯一功能提交者和全量门禁执行者；不得并行门禁、盲杀进程、`git add .`，提交排除 `target`、`.env`、凭据及历史大冻结 JSON。

## 六、PARTIAL 口径

`125` 服务端生产者、真 socket、行会战/`EnemyGuild` 继续 PARTIAL；AOI 缺逐路由真实测试/命令证据的项目记 PARTIAL（失败可记 BLOCK）。不得把部分闭环、静态说明或局部绿灯写成全量 PASS；不据本轮绿灯宣称全 Crystal 等价。

## 七、风险（最多 5 项）

1. F 改动触及共享 `magic.rs` 结算路径，F 提交前后须严格串行，避免 A 基于过期树。
2. 玩家 Slow/Frozen 属低概率分支；固定种子应证明概率成功/失败两侧，报告实际计数。
3. 地形、落点、距离 2/3 格及延迟期间状态变化可能改变判定；落地需重检并断言 HP/状态不变。
4. A 只读不得修码；发现缺陷/缺覆盖须报告 PARTIAL/BLOCK 并交回 Lead，不在审计中扩写实现。
5. 保留所有门禁失败史；失败日志不可覆盖，后续重跑需形成新 attempt 并绑定 HEAD。

## 八、给 Lead 的启动契约

以看板真实子单 ID 启动并映射 agent ID：F 指派 deepseek 编码，F 完成后另启独立 gpt 评审；两者提交意见/结果后由 Lead 检查并提交。随后串行唤醒 A（gpt，只在 F 提交树上只读审计）；A 完成后仅 Lead 跑一次指定全量门禁；再启动 T（gpt，门禁后终审）。每批完成须先核验实际产出与原始证据，再独立评审，最后由 Lead 决定唯一提交/门禁；未满足依赖不得提前唤醒下游。当前派发仅登记，不代表任何 agent 已启动或任何测试/门禁已运行。
