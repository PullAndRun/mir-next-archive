# R58 第58轮方案（中书省 / gpt-6.1-sol）

主单：待 `create` 回执（标题「第58轮：FireBounce 终局 RNG 对齐与 125 客户端接线」）｜HEAD=`1a9aeb316c288150b32bb19859fc35905c510272`=`origin/main`（R57 Done）｜规划 2026-10-26T15:10+08:00（Asia/Shanghai）

只读轮：本轮唯一写入是本文件；未改 `crates/`、未跑 `cargo`/`ci.ps1`、未 commit。R57 主单 `JJC-20261026-004` 已 Done（PASS-WITH-PARTIAL），其 PARTIAL 12 项由 §六 分流到 R58/后续轮。

## 一、本轮目标与可证伪验收

### G1（代码·服务端）FireBounce 终局 RNG 抽签顺序对齐 Crystal
`crates/mir-server/src/magic.rs:2774-2777` 现为「先 `bounce-1`，为 0 直接 return，**不抽签**」；Crystal 为「先 `Envir.Random.Next(targets.Count)` 抽下一目标，再 `--bounce`，`FireBounce` 顶部才 `bounce == 0` 返回」（`Server/MirObjects/HumanObject.cs:5949-5956`，函数顶 `:5811-5813`）。本轮改为：候选集合非空时**先抽签**，再判 `remaining == 0` 提前返回（保持不广播 125、不排伤害）。
- 可证伪：① 新用例断言 level=0 一轮连跳后 RNG 流已推进「终局 1 抽」；② 125 条数仍恒为 `level+1`、命中数仍恒为 `level+2`；③ FireBounce 全程仍不发 126/149。
- 任一为假 → G1 FAIL。
- 预置退路（写死在方案里，避免临场改口）：若改序导致 >2 处既有用例红且无法在 30 行内收敛，**放弃改序**，转「永久登记 MINOR」并保留原用例；此分支须在证据页明写触发条件与实际用例名。

### G2（代码·客户端）125 解码与投射物事件落点（接线，不渲染）
客户端现无 `ObjectProjectile` 分支（`grep -n "ObjectProjectile" crates/mir-client/src/net.rs` = 0 命中，包被通配 `_ => {}` 吞掉）。本轮加：解码 125 → 解析 `Source`/`Destination` → 压入 `ProjectileEvent`（含 `spell/source/destination/at`）；`Source` 查不到对象时**丢弃**（对齐 Crystal `GameScene.cs:4691-4693` `if (source == null) return;`）；事件表容量沿用既有 `push_effect_event` 的封顶策略。
- 可证伪：① `Source` 未知 → 不产生事件（丢弃）；② `Source` 已知 + `Spell=FireBounce` → 产生 1 条事件且 `destination` 原样保留；③ 既有 `round52_spells_wire` 负例仍绿（本轮不动其语义）。
- **不渲染**：投射物飞行、命中 `Effect 570`、音效属 G4 登记，不在本轮（见 §七）。

### G3（脚本/文档）R57 校验脚本入树并修正协议路径
`target/round57/firebounce/attempt-1/run-checks.sh:33` 现为 `cargo test -p mir-protocol --lib packets::server::tests::default_packets_roundtrip -- --exact`，模块路径错（真实为 `mod roundtrip`，见 §二 锚点 4/5），且脚本只在被 `.gitignore` 的 `target/` 下，干净检出不可复现。本轮把脚本入树并修正为两条真实路径 + 补 `00-db`/`10-clippy`/`15b`/`19` 手工件。
- 可证伪：`bash docs/testing/round58/run-checks.sh --list` 列出的每条 `cargo test` 过滤串，在 `cargo test --no-run` 下能被唯一匹配；`grep -c "packets::tests" ` = 0。

### G4（文档·清理）R57 承接 10 项登记清零
clippy 二轮原始日志独立补件、A0 DB 探针入树（脱敏）、门禁捕获统一 UTF-8、`world.rs:1378` 注释 `dvance_clock`→`advance_clock`（纯注释）、`can_fly` 双实现**差异固化**（不合并，见 §二 锚点 6）、默认 DB 缺 FireBounce 的学习/掉落路径登记、次级目标随机序列 MINOR、125 不回显施法者、Hero 无实例、客户端渲染 PARTIAL。
- 可证伪：`docs/testing/round58/README.md` 的 PARTIAL 索引条数与 §六 表一致；`grep -rn "dvance_clock" crates/` = 0。

## 二、事实锚点（已真读；未读者显式标「待实证」）

| # | 事实 | 依据 | 状态 |
|---|---|---|---|
| 1 | 客户端 125 只处理 FireBounce：按 `Source` 取对象，取不到即 return；`CreateProjectile(410, Libraries.Magic, true, 6, 30, 4, targetID: p.Destination)`；`missile.Complete` 中若目标非 Dead 则加 `Effect(Libraries.Magic, 570, 10, 600, target)`，并放 `20000+GreatFireBall*10+1/+2` 两段音效 | `github/Crystal/Client/MirScenes/GameScene.cs:4687-4713` | 已读 |
| 2 | `CreateProjectile` 签名：`(int baseIndex, MLibrary library, bool blend, int count, int interval, int skip, int lightDistance = 6, bool direction16 = true, Color? lightColour = null, uint targetID = 0)`；`Target` 由 `targetID` 解析 | `github/Crystal/Client/MirObjects/MapObject.cs:339` | 已读 |
| 3 | 连跳结算顺序：命中结算后，仅 `target.Race == ObjectType.Monster` 时 `FindAllNearby(3, …)` 过滤 `x != target && IsAttackTarget && target.CanFly(x.CurrentLocation)`，**再抽签** `targets[Envir.Random.Next(targets.Count)]`，最后 `FireBounce(nextTarget, magic, target, --bounce)` | `github/Crystal/Server/MirObjects/HumanObject.cs:5944-5956` | 已读 |
| 4 | `FireBounce` 函数顶判定 `target == null \|\| !IsAttackTarget \|\| !source.CanFly(...) \|\| bounce == 0` 即 `return false`；`bounce == -1` 时才对 `delay += 500`，否则广播 125；`delay = MaxDistance*50` | `HumanObject.cs:5811-5833`（`:5813` 顶部判定、`:5826` 广播、`:5829` 排队；R58 门下 M3 勘误：原文误记 `:5827`） | 已读 |
| 5 | Rust 现状：`remaining = bounce.saturating_sub(1); if remaining == 0 { return; }` 在**抽签之前**，故终局少 1 次 `MirRng` 抽签 | `crates/mir-server/src/magic.rs:2774-2777` | 已读 |
| 6 | `can_fly`（`:4029`，用 `walkable_on`，含攻城阻挡）与 `can_fly_cell_valid`（`:4047`，用 `poison_cloud_cell_valid`，仅 `Cell.Valid`）**语义不同且在产**：`:1536`（FireBall/GreatFireBall/FrostCrunch）用前者、`:1555`（FireBounce）与 `:2971`/`:2989` 用后者 → **不可合并**；Crystal `CanFly` 是 `Cell.Valid` 语义，故 `:1536` 路径存在攻城期偏差 | `crates/mir-server/src/magic.rs:4029`、`:4047`、`:1536`、`:1555`、`:2971`、`:2989` | 已读 |
| 7 | 协议层 125 已完备（结构/Index/编解码顺序），且 `mod roundtrip` 是真实模块名：`packets::server::roundtrip::default_packets_roundtrip`、`packets::client::roundtrip::default_packets_roundtrip` | `crates/mir-protocol/src/packets/server.rs:3903-3929`、`:10696`、`:10700`；`crates/mir-protocol/src/packets/client.rs:5596`、`:5600` | 已读 |
| 8 | 客户端零 125 处理：`grep -n "ObjectProjectile" crates/mir-client/src/net.rs` = 0；事件落点范式为 `push_effect_event` + `EffectEvent`（`id/object_id/effect/time`）与 `SpellEvent` | `crates/mir-client/src/net.rs:1481-1495`、`:5356-5358`、`:7183` | 已读 |
| 9 | `world.rs:1378` 注释仍为 `dvance_clock`（缺 `a`）；同函数文档行 `:1374` 引用 `advance_clock` 正常 | `crates/mir-server/src/world.rs:1374`、`:1378` | 已读 |
| 10 | `run-checks.sh` 只在 `target/round57/firebounce/attempt-1/`（61 行，`.gitignore` 覆盖）；`:33` 路径为 `packets::server::tests::…` 且缺 client 侧 | `target/round57/firebounce/attempt-1/run-checks.sh:33`；`find . -name "run-checks*.sh" -not -path "./target/*"` = 0 | 已读 |
| 11 | 125 现被 `_ => {}` 通配静默吞掉（该通配紧跟 `ServerPacket::Disconnect` 分支，是包分发的兜底臂） | `crates/mir-client/src/net.rs:6412` | 已读 |
| 12 | `push_effect_event` 的容量封顶数值（新 `push_projectile_event` 需沿用） | `crates/mir-client/src/net.rs:7183` 起 | 待实证（探针 P2） |

## 三、子任务 DAG（owner / 写范围 / 依赖）

| ID | 目标 | owner | 写范围（严格） | 依赖 |
|---|---|---|---|---|
| S1 | G1 服务端 RNG 终局抽签对齐 + 用例 | deepseek（编码） | `crates/mir-server/src/magic.rs`（`:2740-2800` 区段）、`crates/mir-server/tests/round58_rng_order.rs`（新增） | — |
| S2 | G2 客户端 125 接线（不渲染） | deepseek（编码） | `crates/mir-client/src/net.rs`（事件结构 + 一个 match 分支 + push 函数）、`crates/mir-client/tests/round58_projectile_wire.rs`（新增） | — |
| S3 | G3 脚本入树 + 路径修正 | deepseek（编码） | `docs/testing/round58/run-checks.sh`（新增）、`docs/testing/round58/README.md`（新建索引） | S1、S2（软：取用例名） |
| S4 | G4 文档清理 10 项 | gpt-6.1-sol（文档） | `docs/testing/round58/**`、`docs/process/round58-*.md`、`crates/mir-server/src/world.rs:1378`（**纯注释 1 行**） | S1–S3 日志 |
| S5 | 复核与门禁 | gpt-6.1-sol（评审）→ Lead | 只读 + `target/round58/gate/` | S1–S4 |

并行度：S1/S2 无共享文件，可同轮并行；S3 需 S1/S2 定名后落；S4 收尾；S5 唯一门禁。Lead 是唯一执行 git/门禁的角色，中书/尚书/门下不 commit。

## 四、逐子任务验收命令与证据路径

证据根：`target/round58/<子任务>/attempt-<n>/*.log`（`target/` 不入库；入库件放 `docs/testing/round58/`）。

**S1**
```bash
cargo test -p mir-server --test round58_rng_order 2>&1 | tee target/round58/s1/01-rng-order.log
cargo test -p mir-server --test round57_firebounce 2>&1 | tee target/round58/s1/02-r57-regress.log
cargo test -p mir-server --test magic          2>&1 | tee target/round58/s1/03-magic-lib.log
cargo test -p mir-server --test round52_regression 2>&1 | tee target/round58/s1/04-round52.log
```
判据：03/04 全绿 fail=0；01 断言「终局抽签 1 次」且 125 数=`level+1`；02 若红须按 S1 预置退路分流并记录用例名。证据：`docs/testing/round58/README.md` §S1 摘要 + 上述 log。

**S2**
```bash
cargo test -p mir-client --test round58_projectile_wire 2>&1 | tee target/round58/s2/01-wire.log
cargo test -p mir-client --test round52_spells_wire   2>&1 | tee target/round58/s2/02-spells-wire.log
cargo test -p mir-protocol --lib packets::server::roundtrip::default_packets_roundtrip -- --exact 2>&1 | tee target/round58/s2/03-proto-server.log
cargo test -p mir-protocol --lib packets::client::roundtrip::default_packets_roundtrip -- --exact 2>&1 | tee target/round58/s2/04-proto-client.log
```
判据：01 断言「未知 Source 丢弃 / 已知 Source 产生 1 条事件 / destination 原样」三条全绿；02/03/04 不回归。

**S3**
```bash
bash docs/testing/round58/run-checks.sh --list | tee target/round58/s3/01-list.log
grep -c "packets::tests" docs/testing/round58/run-checks.sh   # 期望 0
cargo test -p mir-protocol --no-run 2>&1 | tee target/round58/s3/02-proto-norun.log
```
判据：`--list` 含 server+client 两条 roundtrip；`packets::tests` 计数 0；`--no-run` exit0。

**S4**
```bash
grep -rn "dvance_clock" crates/ | tee target/round58/s4/01-typo.log   # 期望空
git status --porcelain | tee target/round58/s4/02-status.log
sha256sum docs/testing/round58/README.md
```
判据：`01` 空；`02` 仅本轮白名单文件（含 2 个历史 44/45MB 冻结 JSON 仍未跟踪、不入库）；PARTIAL 表条数与 §六 一致。

**探针（写入前必须先跑，消除「待实证」）**
- P1：`grep -n "_ => {}" crates/mir-client/src/net.rs | sed -n '1,40p'` 定位 125 现落分支，并在 plan 证据页写下真实行号。
- P2：`sed -n '7183,7210p' crates/mir-client/src/net.rs` 取封顶常量，S2 沿用同一常量。
- P3：`cargo test -p mir-server --test round57_firebounce -- --list` 取既有用例名，避免 S1 误改。

## 五、提交与门禁策略（Lead 唯一）

- 门禁脚本与 CI 由 Lead 执行：`pwsh -File scripts/ci.ps1`（若存在则以其为准；不存在则由 Lead 逐条跑 §四 命令并把 stdout 原样重定向到日志，统一 UTF-8）。门禁捕获编码本轮统一 UTF-8（`chcp 65001` / `$OutputEncoding=[Text.UTF8Encoding]::new($false)` + `2>&1 | Tee-Object -Encoding utf8`），消除 R57 attempt-2 的 10 行 GBK 乱码；若环境不允许，须在证据页注明「捕获为 GBK，附转换件」。
- 分工：deepseek 只提交代码与其自测日志；gpt 只写文档与评审；**git add/commit/push、门禁判定、PASS/PARTIAL 裁定只由 Lead** 发起。
- 提交粒度建议 3 个 commit：`fix(magic): FireBounce 终局抽签对齐` / `feat(client): 125 解码与投射物事件落点` / `docs(round58): 脚本入树与 PARTIAL 索引`。每个 commit 只含其写范围，禁止夹带 `docs/testing/monster-native/*.json`。
- 准入门槛：§四 全部判据为真 + `validate --as zhongshu` exit0 + 门下确认单存在，方可由 Lead 提交。

## 六、风险与 PARTIAL 声明（R57 承接 12 项 → R58 分流）

| R57 项 | R58 处置 | 本轮状态 |
|---|---|---|
| 1 脚本路径错 | S3 入树并修正（`packets::{client,server}::roundtrip::…`） | 本轮闭 |
| 2 clippy 二轮原始日志 | S4 独立补件（不覆盖后写 note） | 本轮闭 |
| 3 A0 探针入树 | S4 脱敏入树 | 本轮闭 |
| 4 门禁捕获编码 | §五 统一 UTF-8 | 本轮闭 |
| 5 `dvance_clock` | S4 补 `a`（纯注释 1 行） | 本轮闭 |
| 6 `can_fly` 双实现 | **不合并**：语义不同且在产（锚点 6），S4 固化差异 + 登记 `:1536` 攻城期偏差 | 本轮登记闭，代码不动 |
| 7 M2 终局 RNG | S1 改序对齐（带预置退路） | 本轮闭或转 MINOR |
| 8 默认 DB 缺 FireBounce 学习/掉落 | S4 登记 PARTIAL（生产学习路径仍未证） | 留 PARTIAL |
| 9 次级目标随机序列 | S4 登记 MINOR（集合等价、非逐次同序） | 留 PARTIAL |
| 10 125 不回显施法者 | S4 登记（`World::broadcast` 语义差异） | 留 PARTIAL |
| 11 Hero 无实例 | S4 登记（候选过滤已按 Monster/Player/Hero 写） | 留 PARTIAL |
| 12 客户端 125 渲染 | S2 只做接线；A2 渲染本轮**不做** | 留 PARTIAL（细化） |

风险：R1 改序可能触及既有 RNG 期望值（已备退路）；R2 客户端事件若无人消费会被 `-D warnings` 判 dead_code（以 `pub` 字段 + 用例消费化解）；R3 门禁编码在本机 pwsh 受限模式下 `Tee-Object -Encoding` 可能不可用（退路：`cmd /c chcp 65001` + 重定向）；R4 `target/` 不入库导致脚本证据易丢（S3 已入树）。

## 七、不做清单与待门下裁决项

### 7.1 不做清单（本轮明确排除）
1. **客户端投射物渲染（A2）**：不做。理由：Crystal 侧参数完备（锚点 1/2：`410/Magic/blend/6/30/4/skip`、命中 `Effect 570,10,600`、两段音效），但 Rust 客户端**无任何投射物/导弹渲染基座**（`grep -rln "missile\|Missile" crates/mir-client/src/` = 0），补渲染需先建飞行插值、帧动画、命中回调、特效与音效通道，属整系统而非单轮最小可见实现；且本项目美术资源索引（mir2_dev）对应 Magic 库 410/570 图元尚未核对。故本轮只接线，登记 PARTIAL 并提出下轮 G 目标「Missile 基座 + FireBounce 410 飞行 + 命中 570」。
2. 不新增任何未见于 Crystal 的表现常量（不发明弹速/特效编号/音效）。
3. 不动 `can_fly`（`:4029`）与 `FireBall` 路径语义（§六 第 6 项只登记）。
4. 不做 R56 遗留、roadmap 下一批（技能耗材/guild/攻城）：本轮已有 2 个代码目标 + 10 项清理，纳入会稀释。
5. 不提交 `docs/testing/monster-native/player-resource-freeze*.json`（44/45MB，历史冻结件，排除入库）。
6. 不写「125 全链」「R57 完整」类表述；125 客户端渲染仍是 PARTIAL。

### 7.2 待门下裁决项（3–5 条）
1. **C2：R58 是否接受「客户端只接线不渲染」作为 A 项的收口？** 若门下要求可见效果，须把 A2 拆为 R59 主目标并追加美术图元核对（Magic 410/570 在 mir2_dev 的索引存在性）。
2. **C1：G1 改序授权**——改 RNG 抽签顺序会改变同种子下的后续随机流（伤害/掉落序列），是否允许？若不允许，S1 直接走「永久登记 MINOR」分支。
3. **C3：`can_fly` 偏差（`:1536` FireBall 路径用含攻城阻挡版，Crystal 用 `Cell.Valid`）** 是否单列 R59 修复项？本轮仅登记。
4. **C4：门禁编码退路**——若 `Tee-Object -Encoding utf8` 在本机受限模式不可用，是否接受「GBK 原件 + UTF-8 转换件」双件并存？
5. **C5：S3 脚本入库路径**——`docs/testing/round58/run-checks.sh`（随轮次）vs `scripts/round-checks.sh`（长期复用），请裁定一个，避免下一轮再搬。
