# R58 尚书省派发件（JJC-20261026-005）

**签发**：尚书省 / gpt-6.1-sol｜2026-10-26T15:22+08:00｜主单现态 `Assigned`（0/0 todos）｜HEAD=`1a9aeb3`=`origin/main`（R57 Done）
**输入**：`docs/process/round58-plan.md`（141 行 / 16,257B / sha256 `8d97b419…`）；`docs/process/round58-menxia-review.md`（64 行 / 8,656B / sha256 `9bd9ac0510cd6dfe09edf3a511c6d3a752b594dece9bf5d147b9de093d2988fe`，**PASS-WITH-CHANGES**，C1–C5 裁决与 M1–M5 必改全在文内）
**边界**：本文件是尚书省本轮唯一写入。未改 `crates/`、未跑 `cargo`/`ci.ps1`、未 commit；看板只经 CLI 写。

## 一、输入与边界

| 项 | 事实 |
|---|---|
| 受审 HEAD | `1a9aeb3`（`1a9aeb316c288150b32bb19859fc35905c510272`）= `origin/main` |
| 工作树未跟踪 | `docs/process/round58-plan.md`、`docs/process/round58-menxia-review.md`、`docs/testing/monster-native/player-resource-freeze-before.json`(44MB)、`player-resource-freeze.json`(45MB) |
| 排除项 | 两个冻结 JSON **不入库**、不被任何子单 `git add`；`target/` 不入库 |
| 串行规则 | S1/S2 可并行（写不同 crate 的不同文件）；但 `cargo`/`target/` 由 Lead 串行持有，同树门禁唯一（一次 `ci.ps1` 定生死，禁子单各自起门禁） |
| 依赖 | S3 依赖 S1/S2 定用例名；S4 依赖 S1–S3 日志；T 依赖 Lead 门禁原始 exit |

## 二、五个执行子单契约

### S1（兵部 bingbu / deepseek）— G1 服务端 RNG 终局抽签对齐
- **写范围**：`crates/mir-server/src/magic.rs`（`:2740-2800` 区段）+ 新建 `crates/mir-server/tests/round58_rng_order.rs`。**不得**碰其它 crate/文件。
- **必须实现**：① 候选集合非空时**先抽签**，再判 `remaining == 0` 提前返回（保持不广播 125、不排伤害）；② **M4 断言**：固定种子下确定性断言「第 N 次 `MirRng` 输出 == 多抽 1 次的期望值」，**不得只断言状态变化**；同时保留 125 数=`level+1`、命中数=`level+2`、无 126/149 三条。
- **预置退路（写死）**：若改序导致 **>2 处既有用例红且 30 行内收不住** → 立即回滚改序，转**永久 MINOR**，保留原用例；须留回滚证据（用例名 + 红因 + 回滚 diff）于 `target/round58/s1/attempt-1/04-rollback.log`，禁静默改期望值。
- **验收**：`cargo test -p mir-server --test round58_rng_order`、`--test round57_firebounce`、`--test magic`、`--test round52_regression`，日志 `target/round58/s1/attempt-1/*.log`。
- **禁止**：改 `can_fly`/`:1536`/`FireBall` 语义；新增 `#[allow]`/`unwrap` 绕过；断言含 CJK。

### S2（兵部 bingbu / deepseek）— G2 客户端 125 接线（不渲染）
- **写范围**：`crates/mir-client/src/net.rs`（事件结构 + 一个 match 分支 + push 函数）+ 新建 `crates/mir-client/tests/round58_projectile_wire.rs`。
- **必须实现**：① **M1 先跑 P2 取真实封顶常量**（`net.rs:7183` 附近 `push_effect_event`），写入证据页后方可写 `push_projectile_event`，**不得自造常量**；② 解码 125 → `ProjectileEvent{spell,source,destination}`，复用 `push_effect_event` 同一封顶策略；③ `Source` 查不到对象 → **丢弃**；④ **M2 非 FireBounce 的 125 只丢弃** + 一条「Spell 非 FireBounce → 0 事件」负例；⑤ **不渲染**（飞行/`Effect 570`/音效全不做）。
- **验收**：`cargo test -p mir-client --test round58_projectile_wire`、`--test round52_spells_wire`、两条 `cargo test -p mir-protocol --lib packets::{server,client}::roundtrip::default_packets_roundtrip -- --exact`；日志 `target/round58/s2/attempt-1/*.log`。
- **禁止**：写任何渲染/资源加载；`ProjectileEvent` 留成 dead_code（`-D warnings` 会红，须以 `pub` 字段 + 用例消费化解）。

### S3（兵部 bingbu / deepseek）— G3 校验脚本入树 + 路径修正
- **写范围**：新建 `docs/testing/round58/run-checks.sh`（可执行）+ `target/round58/scripts/` 证据。**禁改 `crates/`**。
- **必须实现**：路径修正为 `packets::{server,client}::roundtrip::default_packets_roundtrip`（真实 `mod roundtrip`）；补 `00-db`/`10-clippy`/`15b`/`19` 手工件；`packets::tests` 计数须为 0。
- **验收**：`bash docs/testing/round58/run-checks.sh --list`（须含 server+client 两条 roundtrip）、`grep -c "packets::tests" …`=0、`cargo test -p mir-protocol --no-run` exit0；日志 `target/round58/s3/attempt-1/`。
- **禁止**：建 `scripts/round-checks.sh`（C5 已裁定随轮次；R59 若要长期化，由该轮 Lead 提升并留 shim）。

### S4（礼部 libu / gpt）— G4 文档清理 10 项
- **写范围**：`docs/world/magic.md`（125 口径）、`docs/testing/round57/README.md` §4（承接索引）、新建 `docs/testing/round58/README.md`、`docs/process/round58-plan.md`（**M3**：锚点 4/§二 `:5827`→`:5829`；C3/C5 裁决回执；`can_fly` 差异段引用 `:4029/:4047` 原文各 1 行）、`docs/process/round58-menxia-review.md`（追补 M5 同文句，不改其裁决内容）、`crates/mir-server/src/world.rs:1378` **纯注释 1 行** `dvance_clock`→`advance_clock`。
- **必须实现**：**M5 三处逐字同文**「服务端 125 生产者已实现；客户端 125 仅解码登记，投射物渲染 PARTIAL 待 R59」，全文禁「125 全链」「A2 已通」类表述；`docs/testing/round58/README.md` 登记每条命令与判据（target/ 只作原始件）。
- **验收**：`grep -rn "dvance_clock" crates/` 空；`git status --porcelain` 仅白名单；PARTIAL 表条数与 plan §六 一致；日志 `target/round58/s4/attempt-1/`。
- **禁止**：写其它 crates 逻辑（`world.rs` 仅此注释 1 行，须在 commit 说明单列，禁与 S1 代码混提）。

### S5/T（刑部 xingbu / gpt）— 只读终审
- **前置**：Lead 唯一门禁完成（原始 exit 已落 `target/round58/gate/attempt-1.log`）。**S5 不得自起门禁**。
- **写范围**：`docs/testing/round58/final-manifest.json`、`docs/testing/round58/round58-final-audit.md`、`target/round58/audit/summary.log`。
- **必须实现**：逐项核对 S1–S4 证据绑 HEAD/sha/原始 exit；核对 M1–M5 落地；给出 PASS / PARTIAL 裁定与残余风险（`crates/` 零改动则整体降 PARTIAL）。

## 三、M1–M5 落点表

| 编号 | severity | 关闭子单 | 验收如何证 |
|---|---|---|---|
| M1 | **BLOCKER**（S2 前置） | S2 | P2 探针真实封顶常量写入 `docs/testing/round58/README.md` §S2 并附 `target/round58/s2/attempt-1/00-p2-cap.log`；`push_projectile_event` 复用同一常量，禁自造 |
| M2 | **MAJOR**（S2 前置） | S2 | 「Spell 非 FireBounce → 0 事件」负例断言在 `round58_projectile_wire.rs` 且绿；只登记 FireBounce，其余丢弃 |
| M3 | MINOR | S4 | plan 锚点 4/§二 改 `:5829`；`:4029/:4047` 原文各 1 行入差异固化段；C3/C5 裁决回执入 plan |
| M4 | MAJOR | S1 | 固定种子「第 N 次 MirRng == 多抽 1 次期望值」的**可失败确定断言**（非状态变化），含 125=`level+1`、命中=`level+2`、无 126/149 |
| M5 | MAJOR | S4（+Lead 写验收件） | 三处逐字同文（`docs/testing/round58/README.md`、`docs/process/round58-shangshu-acceptance.md`（Lead 写）、`docs/process/round58-menxia-review.md` 追补句）；C4 双件成对入库 + `diff` 结论 |

## 四、验收口径（全局 PASS 必要条件）

1. **0 tests / 0 命中 / 跳过一律不算 PASS**；`--list` 发现数必须与执行数一致（`started == passed`）。
2. 断言不得含 CJK（i18n 棘轮）；禁新增 `#[allow]`/`unwrap` 绕过。
3. 证据必须绑 HEAD/sha/原始 exit：每条 log 首行记 `HEAD=1a9aeb3`、命令原文、原始 exit code，禁裁剪 stderr。
4. **C1** 改序批准**带条件**＝M4 落真；**C3** `can_fly` **不合并**（`:1536` FireBall 偏差由 R59 单列 + 自带攻城期回归用例，本轮只登记）；**C4** GBK 原件 + UTF-8 转换件**成对入库**（同名 `.gbk.log`/`.utf8.log`）+ diff 仅编码差异结论；**C5** 脚本路径 `docs/testing/round58/run-checks.sh`。
5. 本轮 PASS = S1–S5 判据全真 + M1/M2/M4 落地 + M5 三处同文 + `validate --as menxia` exit0；`crates/` 零改动则整体降 PARTIAL。

## 五、串行与门禁

`S1 ∥ S2`（并行，写不同 crate 不同文件）→ **S3**（依赖 S1/S2 定用例名）→ **S4** → **Lead 唯一门禁** `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`（新日志 `target/round58/gate/attempt-1.log`，保留原始 exit，禁复用旧日志）→ **T（S5）**。

- 门禁只有一次；失败由 Lead 定位后打回对应子单，子单返工后仍由 Lead 重跑门禁并记 `attempt-2`。
- `git add/commit/push`、PASS/PARTIAL 裁定只由 Lead；三个 commit 粒度：`fix(magic)` / `feat(client)` / `docs(round58)`，各只含自身写范围。

## 六、黑名单与风险

- **禁**：改协议线格式（125 编解码已在 `packets/server.rs:3903-3929`，本轮只读）；实现任何投射物渲染；手改 DB；把 `player-resource-freeze*.json` 入库；动 i18n 阈值；写「125 全链」类表述。
- **R1 改序外溢**：同种子后续随机流会变（伤害/掉落）。触发退路须写清用例名与红因。
- **R2 dead_code**：`ProjectileEvent` 未消费在 `-D warnings` 下会红 → `pub` 字段 + 用例消费（M2 负例即消费者）。
- **R3 证据易丢**：`target/` 不入库，S3 脚本与 README 才是可复查件；每条命令与判据须入 `docs/testing/round58/README.md`。
- **R4 越权写入**：`world.rs:1378` 纯注释 1 行归 S4，commit 说明单列。
