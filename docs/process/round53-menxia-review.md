# R53 门下省评审：PvP FrostCrunch 与 AOI 验收收尾

> 主单：`JJC-20261026-001`；评审裁决来源：gpt-6.1-sol 门下裁决（Lead 依裁决记录落盘，原文见会话 e126fe19 回执）；2026-10-26（Asia/Shanghai）。
> 事实基准：`main` / `origin/main` = `2c3bccf`；送审方案 `docs/process/round53-plan.md`（72 行）。

## 结论

**PASS-WITH-CHANGES** —— 批准按方案推进（F PvP FrostCrunch 真实玩家目标闭合 + A AOI 全量验收收尾 + G 队长门禁），但须满足下列必改验收条件；本评审**不**代表实现已通过，也**不**代表 AOI 已全量验收。

## 一、锚点核验（评审抽样）

| # | 锚点 | 核验方式 | 结论 |
|---|---|---|---|
| 1 | Crystal `HumanObject.cs:3503` `case Spell.FrostCrunch`、`:5961-5995` FrostCrunch 结算 | grep 命中 | 存在 |
| 2 | Crystal `MapObject.cs:428` 起 `IsAttackTarget`（Dead/自身/安全区/AI56 训练豁免/按 Race 分派） | sed 读原文 | 存在 |
| 3 | Rust `crates/mir-server/src/magic.rs:2671-2682` 延迟命中仅搜 `AoiKind::Monster` | 读方案引用 | 与 R52 PARTIAL 一致，本轮目标 |
| 4 | Rust `crates/mir-server/src/world.rs:2480-2525` 玩家 PK 谓词 | 读方案引用 | 已存在，D2 要求逐项对照 Crystal |
| 5 | Rust `crates/mir-server/tests/round52_regression.rs:814-1009` 怪物真实施法 / 玩家 PoisonState 拒攻 | 读方案引用 | 存在；玩家真实施法命中仍缺 |
| 6 | AOI：Crystal `PlayerObject.cs:1741-1768` 33×33、`Map.cs:2361-2383/2437-2448`、`HumanObject.cs:7864-8152`；Rust `tests/aoi.rs` + `docs/world/aoi.md` | 读方案引用 + 文档 | 方向一致；覆盖须逐路由举证 |

## 二、裁决

- **D1 范围：批准。** F 与 A 可同批，但按 `magic.rs` 串行实施（F 冻结后 A 才审）。`125` 服务端生产者、真 socket、真实客户端窗口明确留待后续，不作为本轮隐性门槛。
- **D2 Frost 口径：修改后批准。** 仅对现有玩家 PK 谓词允许的目标按 Crystal 规则执行；延迟落地前再次完整校验 PK、安全区、同图、存活及 2 格落点；校验失败则目标 HP 与状态效果**均不得变化**。`EnemyGuild`/行会战差异继续 PARTIAL。
- **D3 AOI 收尾：批准。** 仅凭逐路由真实测试/命令证据及门禁后可复算的原始日志关闭；静态说明、测试数量或方向一致不得替代运行证据；证据缺失一律 PARTIAL，不以改代码补证据。

## 三、必改项（≤5，逐条可执行）

1. **F 真实施法链**：新增真实 cast → 延迟 tick → 合法玩家 HP 下降的集成测试，并证明结算前未提前扣血；禁止以注入 `PoisonState` 冒充命中。验收：`cargo test -p mir-server --test round53_frost_pvp -- --nocapture` 非零执行、无 ignore/filter。
2. **F 落地重检**：分别覆盖施法后 PK 资格变化、进入安全区、换图、目标死亡、离开落点（距离 2/3 格边界）；拒绝路径断言 HP 与相关状态均无新增变化。
3. **F 等级/概率/时长**：固定种子覆盖等级差 `+2` 判定边界、效果概率成功/失败两侧、Slow 4s / Frozen 2s 时长；真实 Frozen 后 `World::attack` 拒攻与到期恢复；不得改 Crystal 公式与 RNG 序列，怪物回归保持通过。
4. **A 矩阵**：逐路由建立「Crystal 行为 → Rust 实现 → 测试名/命令 → 原始日志 → 结论」矩阵；未执行、仅静态核对或缺覆盖均标 PARTIAL 并写明缺口。
5. **G 证据绑定**：补齐 F 专项、既有怪物回归、AOI 与全量门禁的确切命令；保存代码版本/HEAD、种子、原始退出码与完整输出；门禁后再改代码须重跑受影响门禁并更新证据。

## 四、PARTIAL 与门禁证据口径

- 继续 PARTIAL：`125` 无生产者、真 socket、行会战/`EnemyGuild`；AOI 任何无逐路由实测证据项。
- 测试运行须绑定代码提交；验收提交须关联该代码版本与日志原字节 SHA。
- 总退出码为零不足以关闭条目：还须核对目标测试实际执行、未被过滤或忽略。
- 全部适用路由获证后，方可提交刑部验收并按流程关闭 Roadmap 待验收项。

## 五、风险提示

FrostCrunch 改动触及共享 `magic.rs` 结算路径，且玩家效果概率较低，易致怪物回归或偶发漏测；必须串行实施，并以固定种子与确定性证据验收。
