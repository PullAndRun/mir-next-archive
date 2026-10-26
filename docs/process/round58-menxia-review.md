# R58 门下省评审（JJC-20261026-005）

主单：`JJC-20261026-005`「第58轮：FireBounce 终局 RNG 对齐与 125 客户端接线」｜现态 Menxia（0/0 todos）｜HEAD=`1a9aeb3`=`origin/main`（R57 Done）
被审件：`docs/process/round58-plan.md`（141 行，2026-10-26T15:10+08:00）｜承接源：`docs/process/round57-shangshu-acceptance.md` §四（12 项）、`docs/testing/round57/README.md` §4
评审范围：只读。未改 `crates/`、未跑 `cargo`/`ci.ps1`、未 commit。下文行号全部来自本次实读片段。

## 一、结论

**PASS-WITH-CHANGES（批准立项，附必改 M1–M5；M1/M2 必须在 S2 开工前闭环）**

方案骨架成立：G1–G4 各有可证伪判据、有预置退路、有写范围隔离、有「不做清单」；R57 的 12 项承接与 §六 分流表逐项对应，无遗漏、无「全链已通」式表述。扣分点集中在两处「待实证」被写进了实现约束（M1/M2）与一处锚点行号偏移（M3），均可 1 行修正，不构成驳回理由。

## 二、C1–C5 裁决

| # | 议题 | 裁决 | 一行理由 | 必改 |
|---|---|---|---|---|
| C1 | RNG 改序授权 | **批准**（带条件） | 实读 `HumanObject.cs:5949-5956`：先 `targets[Envir.Random.Next(...)]` 抽签、后 `--bounce`，而 `magic.rs:2774-2777` 先 `remaining==0 return` 再抽签；level=0 时 Crystal 抽 2 次（第 2 次抽中即废）、Rust 抽 1 次，差恰 1 次，改序是唯一对齐手段 | M4 |
| C2 | 客户端只接线不渲染是否收口 | **修改后批准** | Crystal 侧参数完备（`410/Magic/blend/6/30/4`、命中 `570/10/600`、音效 `+1/+2`，`GameScene.cs:4697-4709`），但 Rust 客户端无任何 Missile 基座，单轮补渲染必失控；可收口为「本轮只做解码登记」，但**不得**表述为 125 已闭环 | M5 |
| C3 | `can_fly` 双实现 | **批准不合并** | 实读 `:4029` 用 `walkable_on`（含 `conquest_object_blocking`）、`:4047` 用 `poison_cloud_cell_valid`（仅 `Cell.Valid`）；`:1536`（FireBall/GreatFireBall/FrostCrunch 共用臂）走前者、`:1555`（FireBounce）走后者，语义不同且在产，合并即改行为 | M3、R59 单列见下 |
| C4 | 门禁编码退路 | **修改后批准** | 本机 pwsh 受限模式下 `Tee-Object -Encoding utf8` 可能不可用（plan §六 R3 已识别），GBK 原件 + UTF-8 转换件双件并存可接受；但须同目录同名 `.gbk.log`/`.utf8.log` 成对入库，且转换后 `diff` 只允许编码差异 | M5 |
| C5 | 脚本入库路径 | **裁定 `docs/testing/round58/run-checks.sh`（随轮次）** | 实查 `ls scripts/` = 不存在，选 `scripts/round-checks.sh` 等于凭空新建顶层目录且本轮无人维护；`docs/testing/roundN/` 已是既有惯例（round56/57 同构），与 S3 写范围一致。R59 若继续复用，由该轮 Lead 一次性提升为 `scripts/round-checks.sh` 并留 shim，避免两处并存 | 无（写入 M3 说明） |

C3 附加裁决：`:1536` 的攻城期偏差（FireBall 路径用含阻挡版，Crystal 用 `Cell.Valid`）**单列 R59 修复项**，本轮只登记不改；R59 该项必须自带「攻城期阻挡格上的 FireBall 目标判定」回归用例，否则不许动 `:1536`。

## 三、锚点抽查表（≥6，全部实读）

| # | 锚点 | 判定 | 说明 |
|---|---|---|---|
| A1 | `github/Crystal/Server/MirObjects/HumanObject.cs:5813` | 命中 | 函数顶 `if (target == null \|\| !target.IsAttackTarget(this) \|\| !source.CanFly(...) \|\| bounce == 0) return false;`，与 plan 锚点 4 一致 |
| A2 | 同上 `:5826` 广播 / 排队 | **偏差** | 广播确在 `:5826`；但 `DelayedAction` 排队在 `:5829`，plan 锚点 4 写「`:5827` 排队」偏移 2 行 → M3 |
| A3 | `HumanObject.cs:5949-5956` | 命中 | 命中结算 → 仅 `Race == Monster` → `FindAllNearby(3,…)` 过滤 → `targets[Envir.Random.Next(targets.Count)]` → `--bounce` 传参，抽签确在递减之前 |
| A4 | `github/Crystal/Client/MirScenes/GameScene.cs:4687-4713` | 命中 | `source == null` 即 return（`:4691`）；`CreateProjectile(410, Libraries.Magic, true, 6, 30, 4, targetID: p.Destination)`；`Dead` 早退；`Effect(Libraries.Magic, 570, 10, 600, …)`；音效 `+1`/`+2` 两段 |
| A5 | `github/Crystal/Client/MirObjects/MapObject.cs:339` | 命中 | 签名 `(int baseIndex, MLibrary library, bool blend, int count, int interval, int skip, int lightDistance = 6, bool direction16 = true, Color? lightColour = null, uint targetID = 0)`，基类 `return null` |
| A6 | `crates/mir-server/src/magic.rs:2774-2777` | 命中 | `let remaining = bounce.saturating_sub(1); if remaining == 0 { return; }` 确在 `fire_bounce_next_target` 抽签之前 |
| A7 | `crates/mir-server/src/magic.rs:4029` / `:4047` | 命中 | 前者 `walkable_on`、后者 `poison_cloud_cell_valid`；后者文档注释已自述「与 `can_fly` 的唯一区别」 |
| A8 | `crates/mir-server/src/magic.rs:1536` / `:1555` | 命中 | `:1536` 在 `SpellKind::FrostCrunch` 臂内调 `can_fly`；`:1555` 在 `SpellKind::FireBounce` 臂内调 `can_fly_cell_valid` |
| A9 | `crates/mir-client/src/net.rs:6412` | 命中 | `_ => {}` 紧跟 `ServerPacket::Disconnect` 分支，125 确被兜底吞掉 |
| A10 | `packets::server::roundtrip` / `packets::client::roundtrip` | 命中 | `server.rs:10696`、`client.rs:5596` 均 `mod roundtrip {`，`fn default_packets_roundtrip` 在 `:10700`/`:5600`；plan 的过滤串正确 |
| A11 | 125 协议层 | 命中 | `server.rs:3903-3912` `ObjectProjectile { spell, source, destination }`，Index 125 注释在案 |

未实证（保留「待实证」）：`push_effect_event` 封顶常量（plan 锚点 12 / P2）——本次未读，见 M1。

## 四、必改（M1–M5）

| 编号 | severity | owner | 落点 | 要求 |
|---|---|---|---|---|
| M1 | **BLOCKER**（S2 前置） | deepseek（S2） | `crates/mir-client/src/net.rs` + `docs/testing/round58/README.md` | 先跑 P2 取 `push_effect_event` 真实封顶常量并写入证据页，再实现 `push_projectile_event`；不得自造常量 |
| M2 | **MAJOR**（S2 前置） | deepseek（S2） | `crates/mir-client/src/net.rs` + S2 用例 | 明确非 FireBounce 的 125 落法：Crystal 的 `switch (p.Spell)` 无 `default`，故**只登记 FireBounce、其余丢弃**；须补一条「Spell 非 FireBounce → 0 事件」断言，防将来误扩 |
| M3 | MINOR | gpt-6.1-sol（S4） | `docs/process/round58-plan.md` 锚点 4、§二 | 排队行号 `:5827`→`:5829` 修正；C3/C5 裁决写入 plan 待裁项回执，`can_fly` 差异固化段落引用 `:4029/:4047` 原文各 1 行 |
| M4 | MAJOR | deepseek（S1） | `crates/mir-server/tests/round58_rng_order.rs` | 「RNG 流已推进终局 1 抽」须写成**可失败的确定断言**：固定种子下断言改序后第 N 次 `MirRng` 输出等于「多抽 1 次」的期望值，而非仅断言状态变化；同时保留 125 数=`level+1`、命中数=`level+2`、无 126/149 三条 |
| M5 | MAJOR | Lead + gpt-6.1-sol（S4/S5） | `docs/testing/round58/README.md`、`docs/process/round58-shangshu-acceptance.md`、本文件 | 三处文档逐字同文：「服务端 125 生产者已实现；客户端 125 仅解码登记，投射物渲染 PARTIAL 待 R59」；全文禁「125 全链」「A2 已通」类表述；C4 双件须成对入库且附 `diff` 结论 |

## 五、风险与验收口径

- **R1（改序外溢）**：C1 授权同时意味着同种子后续随机流改变。plan §一 预置退路（>2 处既有用例红且 30 行内收不敛则转永久 MINOR）**予以确认**，但触发时必须写清用例名与红因，不得静默改期望值。
- **R2（dead_code）**：`-D warnings` 下未被消费的 `ProjectileEvent` 会红；plan 的「`pub` 字段 + 用例消费」方案可接受，M2 的负例正好充当消费者。
- **R3（证据易丢）**：S3 入树后仍须在 `docs/testing/round58/README.md` 登记每条命令与判据，`target/` 只作原始件。
- **R4（越权写入）**：`crates/mir-server/src/world.rs:1378` 纯注释 1 行归 S4（文档角色），与「deepseek 只写代码」边界不冲突，但需在 commit 说明中单列，禁与 S1 代码混提。
- **验收口径**：本轮 PASS 的必要条件 = §四 S1–S5 判据全真 + M1/M2/M4 落地 + M5 三处同文 + `validate --as menxia` exit0；`crates/` 零改动则整体降为 PARTIAL，不得报 PASS。

## 六、评审边界

- 未读：`push_effect_event` 实现段（M1 依赖）、`round57_firebounce.rs` 用例清单（P3 由 S1 自取）、mir2_dev 美术图元索引（R59 前置）。
- 未跑：任何 `cargo`/测试/门禁；本轮不产生代码证据，仅裁方案与锚点。
