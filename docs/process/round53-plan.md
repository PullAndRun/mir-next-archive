# R53 实施方案：PvP 冰咆哮与 AOI 验收收尾

> 主单：`JJC-20261026-001`；2026-10-26（Asia/Shanghai）。本稿为规划与看板治理交付：只读仓库和看板；未改 `crates/**`、未运行 `cargo`、未执行 Git 写操作。
> 当前事实基准按任务快照为 `main` / `origin/main` = `2c3bccf`。开始实施须等待门下独立审核；不得将本计划视为执行批准。

**本轮取舍：**先闭合 R52「玩家真实 FrostCrunch 目标」PARTIAL；再收尾 Roadmap 的 AOI 全量验收。明确不接 125 真 socket 生产者，因为其涉及额外投射物生命周期、真实客户端链路和证据，不应与 PvP 技能共用一次局部绿灯。

## 一、事实（源码静态锚点；不是本轮运行证据）

### R52 承接与 PvP FrostCrunch

- [`docs/process/round52-shangshu-acceptance.md:44-57`](round52-shangshu-acceptance.md#L44) 列明 12 项 R52 PARTIAL；第 4 项（第 49 行）说明目前只有注入 `PoisonState` 后真实 `attack` 拒攻，没有真实施法命中玩家。T3 报告亦保留同一边界：[`docs/testing/round52/round52-final-audit-v2.md:11`](../testing/round52/round52-final-audit-v2.md#L11)。
- Crystal 施法分派为 `HumanObject.cs:3497-3505`；落地结算 `github/Crystal/Server/MirObjects/HumanObject.cs:5961-5995` 要求目标 `IsAttackTarget(this)`、同图、存活且在施法落点 2 格内，使用 MAC 伤害；对玩家等级差上限为施法者 +2，Slow/Frozen 采用玩家概率分支，时长分别 4 秒/2 秒。`MapObject.cs:428-455` 规定存活、自身/安全区与攻击类型检查；具体 PvP 判定依 `HumanObject` 派生目标规则。
- Rust 施法解析已用玩家/怪物共通的 `magic_attack_position` 和 PK 谓词（[`crates/mir-server/src/magic.rs:1487-1504`](../../crates/mir-server/src/magic.rs#L1487)、[`crates/mir-server/src/magic.rs:2363-2377`](../../crates/mir-server/src/magic.rs#L2363)、[`crates/mir-server/src/world.rs:2500-2525`](../../crates/mir-server/src/world.rs#L2500)）；但延迟结算命中谓词只搜 `AoiKind::Monster`（[`magic.rs:2671-2682`](../../crates/mir-server/src/magic.rs#L2671)），魔法伤害 helper 只走 `damage_monster`（[`magic.rs:2684-2700`](../../crates/mir-server/src/magic.rs#L2684)）。已有 `resolve_frost_crunch` 能区分玩家等级并走玩家 PoisonState，但受怪物限定命中/伤害前置阻断（[`magic.rs:4051-4144`](../../crates/mir-server/src/magic.rs#L4051)）。
- 回归测试 `round52_regression_real_frost_crunch_slow_frozen_monsters` 覆盖真实施法到怪物（[`crates/mir-server/tests/round52_regression.rs:814-929`](../../crates/mir-server/tests/round52_regression.rs#L814)）；玩家只有状态注入后实际攻击拒绝，见同文件 `:950-1009`。不能把后者当成 PvP 施法闭环。
- 玩家目标 PK 谓词已有安全区、NoFight、Peace、Group/Guild/All/RedBrown 边界；`EnemyGuild` 暂恒 false，行会战未实现（[`crates/mir-server/src/world.rs:2480-2525`](../../crates/mir-server/src/world.rs#L2480)）。本轮不得宣称行会战 PvP 等价。

### Roadmap AOI 验收

- Crystal `PlayerObject.GetObjects` 逐格扫描视野 33×33（`github/Crystal/Server/MirObjects/PlayerObject.cs:1741-1768`）；`Map.AddObject/RemoveObject` 维护 cell 集合（`github/Crystal/Server/MirEnvir/Map.cs:2361-2383`），广播按 `Globals.DataRange` 过滤（`Map.cs:2437-2448`）；移动视野条带同步在 `HumanObject.cs:7864-8152`。
- Rust AOI 为 8×8 桶、类型+对象 ID 索引与按 `(y,x,id)` 稳定排序；说明及差异见 [`docs/world/aoi.md:28-43`](../world/aoi.md#L28)、`:75-108`、`:226-243`。该文档记录 7 项网格随机/边界单测及 6 项世界集成测试（`:245-270`）；`crates/mir-server/tests/aoi.rs:136-278` 可见范围、广播、双向差分、近战索敌、怪物移动测试，`:296-383` 覆盖门状态与 wire。
- Roadmap 仍写「待刑部全量验收」（[`docs/roadmap.md:24-27`](../roadmap.md#L24)）；AOI 及其他后续项优先级见 `docs/roadmap.md:101-125`，列明 AOI 全量验收待收尾，并明确不可把局部证据外推。

## 二、目标（逐项可判定）

1. **Frost 玩家命中：**在真实 `World` 已学习技能、同图且 PK 允许的设置下，真实 FrostCrunch cast→延迟结算使合法玩家目标 HP 下降；命中时按 Crystal 玩家等级边界与概率产生 Slow/Frozen，Frozen 后调用真实 `World::attack` 必须被拒。记录真实目标 HP/毒状态/战斗结果和包证据。
2. **PvP 护栏：**安全区、NoFight、Peace/同组/同会等禁止对象不受伤、不挂毒；死亡/异图/落点外目标不落地；施法后到落地前 PK 条件失效也不得绕过攻击谓词。允许命中样本和未命中样本均由固定种子组产生并报告计数，不能只挑一个绿例。
3. **怪物回归：**原有 FrostCrunch 怪物慢/冻概率两侧、冻结移动/攻击限制、伤害与升级测试继续通过；不改变现有 R52 口径和 RNG 顺序。
4. **AOI 验收：**逐条对照 Crystal/Rust 锚点和既有 AOI 单测/集成测试，核实声明的进图/离图、移动/跑步/切图、刷出/回收、广播接收者与顺序、视野边界/双向差分、门状态。每条结论必须链接可复查测试和原始命令回执；缺覆盖或失败一律记 PARTIAL/BLOCK，不改写为 PASS。
5. 本轮完成仅能宣称：PvP Frost 已闭合到明确支持的 PK 模型；AOI 达到审计矩阵实证的边界。不得声称真 socket、全 Crystal 等价或行会战覆盖。

## 三、子任务 DAG、范围、验收

串行关系：`门下批准 → F（PvP Frost 编码/测试）→ A（AOI 独立审计）→ Lead 唯一全量门禁 → 尚书验收 → 门下确认`。F 修改共享 `magic.rs`，AOI 审计须在 F 的最终树上进行；不并行写同一文件。不得创建看板子单，待门下批准后由尚书依派活矩阵另行登记。

| ID / 交付 | Owner 模型 | 写入范围 | 依赖 | 可证伪验收命令 | 证据路径 |
|---|---|---|---|---|---|
| F PvP FrostCrunch | deepseek 编码/测试（由 Lead 依 `spawn_teammate` 派发）；gpt 只读独立评审 | `crates/mir-server/src/magic.rs`；新 `crates/mir-server/tests/round53_frost_pvp.rs` | 门下批准 | `cargo test -p mir-server --test round53_frost_pvp -- --nocapture`；`cargo test -p mir-server --test round52_regression round52_regression_real_frost_crunch_slow_frozen_monsters -- --exact --nocapture`；新增测试必须非零执行、无忽略/跳过，拒绝路径应在断言被删时失败 | `target/round53/frost-pvp/`（每次运行原始 stdout/stderr、exit、HEAD、用例列表、起止时间、SHA-256） |
| A AOI 验收矩阵与收尾报告 | gpt（只读核验 + `docs/process/**` 文档）；必要时由 Lead 指定刑部独立复核 | 只读 `crates/mir-server/src/aoi.rs`、`world.rs`、`maps.rs`、`monster.rs`、`tests/aoi.rs`、Crystal 对照及 [`docs/world/aoi.md`](../world/aoi.md)；可写 `docs/process/round53-aoi-acceptance.md` | F 完成并冻结；若发现代码缺陷则退回中书，不在审计中直接修代码 | `cargo test -p mir-server --test aoi -- --nocapture`；`cargo test -p mir-server random_operations_match_bruteforce_scan -- --nocapture`；逐项附实际测试名/结果，不接受只有 grep 或文档自述 | `target/round53/aoi-audit/` + `docs/process/round53-aoi-acceptance.md` |
| G 最终门禁、文档/状态核验 | 唯一 Lead 运行门禁；gpt 复核证据、过程文档 | Lead 冻结/提交功能；gpt 只写 `docs/process/**` 与门禁后获准的审计文档 | F、A 通过或 PARTIAL 有准确依据 | 队长唯一执行 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`；审计后 `py -3 tools/docs/check_links.py`、`git diff --check`。不得由本计划作者重跑 cargo/全量门禁 | `target/round53/final-gate/attemptN.log`、同前缀 `.meta.txt`；`docs/process/round53-aoi-acceptance.md` |

F 的建议具名断言：`round53_frost_crunch_hits_pk_eligible_player`、`round53_frost_crunch_rechecks_pk_at_impact`、`round53_frost_crunch_player_level_and_probability_edges`、`round53_frozen_by_frost_blocks_real_player_attack`。测试真实 cast、延迟 tick、HP/PoisonState 与真实拒攻；固定种子循环必须同时出现命中与未命中。玩家等级差按 Crystal `+2`，随机条件逐字节核对 `Next(100) <= magic.Level` 的既有 Rust RNG 等价，不凭测试实现推算期望。

## 四、提交与门禁策略

- 本次只创建计划/看板；当前不运行 cargo，不进行 git 写操作，不改任何 `crates/**`。功能阶段须经门下批准后另行执行。
- Lead 是唯一全量门禁执行者及提交者；子任务仅定向测试，禁止启动全量 gate、并行门禁、`git add .` 或盲杀进程。功能与测试树冻结并提交后，按唯一脚本记录完整 HEAD、原始 exit、日志原字节及 SHA-256；失败 attempt 保留，不能覆盖/改名冒充 PASS。
- 门禁后 gpt/刑部仅补 `docs/process/**` 审计产物为批准例外；列明与被测功能 HEAD 仅文档差异，并补 links/diff 静态检查。代码、测试、CI、配置变化均使旧 gate 失效，须由 Lead 重跑。
- `target/round53/**` 保留可复核原始日志/manifest；提交排除 target、`.env`、凭据和历史大冻结 JSON。当前状态不代表任何测试已经运行。

## 五、风险与 PARTIAL 口径

- Frost 的发起校验已支持玩家，但落地谓词和伤害仍偏怪物；只加 PoisonState 而不实际扣玩家 HP，或直接调用普通攻击替代 MAC 魔法伤害，均不通过。
- 同图/死亡/落点与 PK 条件在延迟施法期间可能变化；必须以落地时条件重新校验，避免施法后切换 Peace/进安全区仍受击。不得绕开 safe-zone、NoFight、同队/同会保护。
- 玩家 Slow/Frozen 概率低；没有双侧稳定种子计数、没有玩家 Frozen 后真实拒攻、或 RNG 序列扰动怪物回归时保持 PARTIAL/BLOCK。NoFight/安全区等 fixture 必须基于真实地图判定链，不可只造 PoisonState。
- AOI 的现有 6 个集成用例和网格随机对拍不自动证明所有维护路径。报告需点名 `run`、跨图、leave、地面物品/NPC 同步等缺失覆盖；无法在本轮经实测闭环就保持 PARTIAL，不据说明文档声称全验收。
- 现有 `EnemyGuild`/行会战争差异、真实 socket、125 ObjectProjectile、客户端真实窗口、AOI 多实例地图和未覆盖 Crystal 语义均不随本轮绿灯关闭。

## 六、不做清单

- 不做 ObjectProjectile 125 服务端生产者/真 socket/真实客户端窗口；不补 PvP FireWall、旧技能耗材、行会战或 guild/嵌套槽属性。
- 不做攻城旗子/控制点/领地/修理，不做行会职位/邀请 UI，不重写 AOI 网格或调整 8×8 桶/16 格边界。
- 不改变 Crystal 已有 FrostCrunch 技能协议、伤害公式、随机序列、技能等级边界或 PK 规则；不扩展 `EnemyGuild`。
- 不更新 road map 的 PASS/Done 时态，除非最终证据及尚书、门下审查均闭合；本轮工作树不执行任何仓库 Git 写入或产品代码变更。

## 七、待裁决项（交门下）

- **D1 范围**：批准 F（真实 PvP FrostCrunch 与落地重检）及 A（AOI 审计收尾）作为同一 R53 批次；125/真 socket 另轮，不将部分闭环合并作全面技能等价。
- **D2 Frost 口径**：玩家目标必须遵循现有 `is_attack_target_player` 和 Crystal 同图/存活/落点规则；延迟命中时重新判定 PK 与安全区，受保护目标不受伤/不挂毒。`EnemyGuild` 未实现继续明确 PARTIAL。
- **D3 AOI 收尾**：只有每条路由都有真实测试/命令证据、门禁后原日志可复算才可关闭 Roadmap 待刑部验收项；现有测试未覆盖的维护路径不得以静态说明补成 PASS，缺证据时以 PARTIAL 报告而非扩写代码范围。

看板记录：`JJC-20261026-001` 已创建为 high，标签 `round53,rust,crystal`；太子→中书→门下状态转换及中书 memo 已完成。计划现待门下独立审核；此状态不等于批准或开始执行。
