# 第 58 轮（R58）文档与证据索引

**本轮范围**（主单 `JJC-20261026-005`，S4 文档批 G4；HEAD `1a9aeb3`=`origin/main`）：G1 FireBounce 终局 RNG 对齐（S1）、G2 客户端 125 解码接线（S2，不渲染）、G3 校验脚本入树（S3）、G4 R57 承接 10 项清理（S4）、门禁（S5/Lead）。本索引只登记已落盘文档、约定与证据线索，不预写门禁/终审结论。

**口径**：状态列只允许「已落盘」「约定」「PARTIAL」「MINOR」；不写「全绿/已闭环」类未证结论。（S1–S3 均已交付并合入，原提交前状态词已无使用者，故从允许值中移除。）

> **125 口径（M5 同文）**：服务端 125 生产者已实现；客户端 125 仅解码登记，投射物渲染 PARTIAL 待 R59

## 1. G4 十项落点

| # | 承接项（R57 → R58） | 落点 | 状态 |
|---|---|---|---|
| ① | clippy 二轮原始日志缺失 | 本文件 §4.1 登记「第二次 clippy 红原件缺失；R58 起原始日志一律进 `attempt-<n>/` 目录」 | 已落盘 |
| ② | A0 默认 DB 探针脱敏入树 | 本文件 §4.2（只写结论与命令，不复制大日志） | 已落盘 |
| ③ | 门禁捕获编码 | 本文件 §4.3：GBK 原件 + UTF-8 转换件成对入库，附 `diff` 结论 | 已落盘（约定） |
| ④ | `dvance_clock` 错字 | `crates/mir-server/src/world.rs:1378` 注释补 `a`（纯注释 1 行） | 已落盘 |
| ⑤ | `can_fly` 双实现差异固化 | [magic.md](../../world/magic.md) §11 新增条 + 本文件 §3（**不合并**） | 已落盘 |
| ⑥ | 默认 DB 路径 | 本文件 §4.4（`MIR2_DEV_DB_PATH` 是 Server.MirDB 文件路径） | 已落盘 |
| ⑦ | 次级随机序影响面 | 本文件 §4.5（同种子后续随机流改变，C1 已授权） | 已落盘（S1） |
| ⑧ | 125 不回显 + 客户端只存事件不绘制 | 本文件 §4.6 + [magic.md](../../world/magic.md) FireBounce 行 | 已落盘 |
| ⑨ | Hero 无实例 | 本文件 §4.7（登记 PARTIAL） | PARTIAL |
| ⑩ | 客户端 125 渲染 | 本文件 §4.8 + [magic.md](../../world/magic.md) FireBounce 行 + [round57 索引](../round57/README.md) §4 | PARTIAL（R59） |

## 2. R57 承接 12 项处置（与 [round58-plan.md](../../process/round58-plan.md) §六 一一对应）

| R57 项 | R58 处置 | 状态 |
|---|---|---|
| 1 脚本路径错 | S3 入树 `docs/testing/round58/run-checks.sh`，过滤串改 `packets::{client,server}::roundtrip::…` | 已落盘（S3） |
| 2 clippy 二轮原始日志 | §4.1 独立登记，不覆盖后写 note | 已落盘 |
| 3 A0 探针入树 | §4.2 脱敏 | 已落盘 |
| 4 门禁捕获编码 | §4.3 双件成对 | 约定 |
| 5 `dvance_clock` | `world.rs:1378` | 已落盘 |
| 6 `can_fly` 双实现 | §3 + magic.md，不合并；`:1536` 偏差单列 R59 | 已落盘 |
| 7 M2 终局 RNG | S1 改序；同种子后续流改变已授权 | 已落盘（S1） |
| 8 默认 DB 缺 FireBounce 学习/掉落 | §4.4：A0 证明 MagicInfo 存在；学书/掉落生产路径仍未证 | PARTIAL |
| 9 次级目标随机序列 | §4.5 登记 MINOR（集合等价、非逐次同序） | MINOR |
| 10 125 不回显施法者 | §4.6（`World::broadcast` 不含本人） | PARTIAL |
| 11 Hero 无实例 | §4.7（候选过滤按 Monster/Player 构造） | PARTIAL |
| 12 客户端 125 渲染 | §4.8（只解码登记，不绘制） | PARTIAL（R59） |

## 3. `can_fly` 双实现差异固化（C3 裁决：不合并）

- `crates/mir-server/src/magic.rs:4029` 的 `can_fly` 走 `walkable_on`，**含攻城阻挡**（`conquest_object_blocking`）。
- `crates/mir-server/src/magic.rs:4047` 的 `can_fly_cell_valid` 只判 `Cell.Valid`（`poison_cloud_cell_valid`），与 Crystal `CanFly` 同语义。
- 两处均在产、语义不同：`:1536`（FireBall/GreatFireBall/FrostCrunch 共用臂）用前者，`:1555`（FireBounce）用后者；合并即改行为，故 **R58 不合并、不改代码**。
- 已知偏差：`:1536` 在攻城期对阻挡格的判定与 Crystal 不一致，**单列 R59 修复项**；R59 动 `:1536` 前必须自带「攻城期阻挡格上的 FireBall 目标判定」回归用例。

## 4. 承接明细

### 4.1 clippy 原始日志

R57 clippy 首红原件仍在（`10-clippy-first-fail.log`：`magic.rs:58` 模块文档缺反引号）；但第二次 clippy 红的原始日志被绿色重跑覆盖，**原件缺失**。仅有 `10b-clippy-second-red-note.log` 记录该轮两条 lint（`clippy::doc_markdown` 于 `crates/mir-server/tests/round57_firebounce.rs:288`、`clippy::type_complexity` 于 `:542`）及修复方式（无 `#[allow]`）。**R58 起**：门禁/clippy/测试的每次原始输出一律落 `target/round58/<子任务>/attempt-<n>/`，重跑开新 attempt 目录，不覆盖旧件。

### 4.2 A0 默认数据探针（脱敏）

- 数据源：`resource/community/suprcode-crystal-database/cjlaaa/Server.MirDB`（与 `common::default_data()` 同源）。
- 命令：`cargo test -p mir-server --test round57_firebounce -- --nocapture a0_default_data_fire_bounce_probe`。
- 结论：`magic_count=107`、`distinct_magic_count=107`，**FOUND FireBounce(154)**（`base_cost=5`、`level_cost=1`、`range=9`、`levels=(15,18,21)`、`power=(10,0)`、`m_power=(6,0)`、`delay_base=1800`）。即真实默认数据含 154 记录，施法测试无需 fixture，也不存在「默认 DB 缺记录导致施法卸载」。
- 仍未证：生产「学书/掉落」获得路径（学习道具与掉落表）未在本轮验证 → 见 §2 第 8 项 PARTIAL。
- 原始件：`target/round57/firebounce/attempt-1/00-db-firebounce.log`（`target/` 不入库，此处只保留可复查摘要）。

### 4.3 门禁捕获编码（C4）

R58 门禁（Lead/S5）对同一命令同时留存 **GBK 原件**与 **UTF-8 转换件**：同目录、同名、后缀区分（`*.gbk.log` / `*.utf8.log`）成对入库；转换后 `diff` 只允许编码差异，结论写入对应证据页。若本机 pwsh 受限模式不支持 `Tee-Object -Encoding utf8`，保留 GBK 原件并在证据页注明「捕获为 GBK，附 UTF-8 转换件」，不因此重跑覆盖。

### 4.4 默认 DB 路径（`MIR2_DEV_DB_PATH`）

- `MIR2_DEV_DB_PATH` 是 **`Server.MirDB` 文件路径**，与 `.env` 的 `MIR2_DEV_DATABASE_URL`（Postgres URL）不是一回事，不要混用。
- 用法：`$env:MIR2_DEV_DB_PATH='resource/community/suprcode-crystal-database/cjlaaa/Server.MirDB'; cargo test -p mir-server --test maps real_database_`。
- 未设该 env 时回退 `GameData::discover()`：`maps` 基线为 **20 例 / 16 有效 / 4 skip**（`MIR2_DEV_DB_PATH` 4/4 真跑），不得把 4 skip 写成全绿（R55 口径）。

### 4.5 次级随机序（RNG 改序影响面）

- G1 把「先判 `remaining==0` 再抽签」改为「候选非空先抽签、再判 0」，使 level=0 的终局也推进 1 次 `MirRng`，与 Crystal `HumanObject.cs:5949-5956` 对齐。
- 影响面：**同一 `MirRng` 种子下的后续随机流会改变**（同 tick 之后的伤害/掉落抽签序列），属 C1 已授权的预期影响面，**不是缺陷**；任何既有用例若因期望值红，须按 plan §一 预置退路记录用例名与红因，不得静默改期望。
- 不变的硬断言：125 条数仍恒为 `level+1`、命中数仍恒为 `level+2`、FireBounce 全程不发 126/149；S1 用例 `crates/mir-server/tests/round58_rng_order.rs` 已落盘（证据 `target/round58/s1/attempt-1/`，`target/` 不入库）。

### 4.6 125 不回显 + 客户端只存事件不绘制

- 服务端：续跳 125 沿用 `World::broadcast`（**不含施法者本人**），Crystal 为 `Map.Broadcast`（含本人），属已知偏差（`magic.rs` 模块「简化与差异」节）。
- 客户端：R58 只把 125 解码为事件登记（未知 `Source` 丢弃；`Spell` 非 FireBounce 丢弃），**不绘制投射物、不放命中特效/音效**。

### 4.7 Hero（登记 PARTIAL）

Crystal `FindAllNearby` 会把 Hero 收进连跳候选，本作 Hero 无实例（`S.HeroInformation.magics` 恒为空），候选过滤按 Monster/Player 构造并在注释中保留 Hero 分支；无 Hero 场景可证但非等价 → PARTIAL（`magic.rs` `fire_bounce_next_target` 注释）。

### 4.8 客户端渲染（PARTIAL，R59）

R58 只做解码登记；410 投射物飞行插值/帧动画、命中 `Effect 570`（`10,600`）、音效 `20000+GreatFireBall*10+1/+2` 全部留 R59。R57 已核参数（`GameScene.cs:4687-4713`、`MapObject.cs:339`）但仍无 Missile 基座，不在本轮伪造实现。

### 4.9 事件 ring 语义（P7，已知限制）

- `ProjectileEvent`（`S.ObjectProjectile`(125) 的解码产物）复用既有 `EFFECT_EVENT_LIMIT = 16`（`crates/mir-client/src/net.rs:166`）；超过 16 条即丢最旧（`net.rs:7257-7259` 的 `remove(0)`），不自造新常量。
- 后果：高频 FireBounce 连跳若在同一帧内到账，早期事件可能被新事件顶掉 —— 客户端只保证「最近 16 条」，属 UI 侧环形缓冲策略，不是协议缺失。
- 消费方约定：R59 渲染须接受该「可能丢最旧」语义，或届时评估扩容；改动该常量必须同步 `projectile_ring_reuses_measured_effect_event_limit` 的 16 长度与首尾 destination 断言。
- 证据：`target/round58/s2/attempt-1/00-summary.md §8.2`（`target/` 不入库，此处只留可复查摘要）；测试 `crates/mir-client/tests/round58_projectile_wire.rs:250-270`。

## 5. 证据与脚本约定

| 项 | 路径 | 状态 |
|---|---|---|
| S4 证据 | `target/round58/s4/attempt-1/{00-summary.md,01-diff.patch,02-links.log,03-redline.log,04-meta.txt}` | 已落盘（本批） |
| 文档校验 | `py -3 tools/docs/check_links.py`、`git diff --check` | 本批已跑，见 S4 证据 |
| S1/S2/S3 证据 | `target/round58/{s1,s2,s3}/attempt-1/` | 已落盘（`target/` 不入库，本文件只登记路径与结论） |
| 脚本 | `docs/testing/round58/run-checks.sh` | 已落盘（S3） |

## 6. 边界

- 本文件不写门禁通过、终审判定或整轮验收结论；`crates/` 本轮由 S1/S2 改动，S4 只改 `world.rs:1378` 纯注释 1 行。
- 不写未证的端到端表述；125 客户端渲染与 Hero 均为 PARTIAL，判定权归门禁后的 T 与 Lead。
- 关联：[R57 索引](../round57/README.md)、[magic.md](../../world/magic.md)、[R58 方案](../../process/round58-plan.md)、[R58 门下评审](../../process/round58-menxia-review.md)。
