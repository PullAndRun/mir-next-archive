# R57 尚书省派发件（JJC-20261026-004）

派发 2026-10-26T13:45+08:00（Asia/Shanghai）｜尚书令 shangshu｜HEAD=`fe86aaa`=`origin/main`（R56 Done）｜工作树仅 2 个历史冻结 JSON + 未跟踪 plan/review
依据（复算 sha 全等）：方案 `docs/process/round57-plan.md` 114 行/15,833B/sha256 `f5e91513135a1fbd70927f26998504c8a15b3fbdf9274d4234f8c109cf348e0a`；评审 `docs/process/round57-menxia-review.md` 98 行/13,702B/sha256 `8f351ece8f13c65ec3f181e1965a3506ee82ba6f0c4982a4074637d5119a8f2d`（**PASS-WITH-CHANGES，0 BLOCK，M1–M5 必改**）
主单 `JJC-20261026-004` 派发前 Assigned（门下子单 `JJC-20261026-004-sub-c4685932`），本件落盘后置 Doing。
本文件是本轮尚书省唯一写入；未改 `crates/`、未跑 `cargo`/`ci.ps1`、未 commit/push、未 spawn 执行者。

## 一、输入与裁决（门下 D1–D4，已生效）

- **D1 全链**：总命中 `level+2`、125 条数 `level+1`（level0→2 命中/1 条 125；level3→5 命中/4 条 125）。
- **D2 随机**：世界级 `MirRng`，候选按对象 ID 稳定排序后 `rng % len`，测试固定种子；与 Crystal 链表序**集合/分布等价、非逐次同序**（MINOR/PARTIAL）。
- **D3**：客户端 125 渲染本轮**不做**，记 PARTIAL；任何产出不得写「125 已全链交付」。
- **D4**：`world.rs:1378` BEL 注释**改派 A**（B 纯文档、不碰 `.rs`）；A 须 `sed -n 1378p | cat -v` before/after 证据，仅删 0x07、行数与语义不变。
- 编排差异登记：plan §三 曾把 `docs/world/magic.md` 支持矩阵行列在 A 的写范围；按用户「deepseek 只编码、gpt 决策/审核/文档」硬约束，该行**归 B**，A 只写代码，文件集不相交。

## 二、子单登记与写者边界（唯一写者）

| key | 归属 / 模特 | 子单 | 写范围（唯一） | 交付 |
|---|---|---|---|---|
| A | 兵部 / deepseek-flash | `JJC-20261026-004-sub-425b7ac8` | `crates/mir-server/src/magic.rs`、`crates/mir-server/tests/round57_firebounce.rs`（新建）、`crates/mir-server/src/world.rs:1378`（仅注释） | G1/G2 |
| B | 礼部 / gpt | `JJC-20261026-004-sub-362391da` | `docs/world/magic.md`、`docs/world/aoi.md:291`、`docs/process/round56-plan.md:45`、`README.md:48`、`docs/roadmap.md`、`docs/testing/round56/README.md:5`、`docs/testing/round57/README.md`（新建） | G3 |
| T | 刑部 / gpt | `JJC-20261026-004-sub-e9251ec2` | `docs/testing/round57/round57-final-audit.md`、`docs/testing/round57/final-manifest.json`、`target/round57/audit/summary.log` | 只读终审 |
| G | Lead | 不建子单 | `target/round57/gate/**`、`target/round57/commit/**` | G4 门禁 |

A 与 B **可并行编辑**（文件集不相交），`cargo` 一律由 Lead 串行执行；T 在 G 门禁绿后启动；任何门禁后改动重跑 `attempt-N+1`。

看板回执（exit 全 0）：主单 `JJC-20261026-004` Assigned→**Doing**；A/B/T 三子单均 depth=1 登记完毕，`memo shangshu` 已写决策/风险；`validate` 条数见回报。

## 三、执行规格

### A（兵部/deepseek-flash）G1 FireBounce 全链（D1）+ G2 包序

- 注册 `SpellKind::FireBounce` 生产者（`Spell::FireBounce=154` 协议层已存在，**不改线格式**）；首跳**不发 125**、伤害排期 `500+MaxDistance*50ms`；此后每跳**先**广播 125（`Source=上一受害者`、`Destination=下一目标`）**再**排 `dist*50ms` 伤害；仅「受击者是 Monster」才续跳；候选=半径 3 内、非自身、`IsAttackTarget`、`CanFly`、种族 **Monster/Player/Hero**（M2-c 修正，**禁写 ignores_players**）；候选按对象 ID 稳定排序后取 `MirRng % len`（**禁 `thread_rng`/`rand::random`**）；全程**不发 126/149**；命中走 MAC、`dealt>0` 才 `level_magic`。
- **A0 第一动作（M3）**：实读默认 DB 是否含 `spell=154` 的 `MagicInfo`，原始 SQL/grep 全量输出落 `target/round57/firebounce/attempt-1/00-db-firebounce.log`（带 exit_code）；缺则测试**显式 fixture 注入**（不改生产 DB、不改 `mir-db` 解析、不伪造记录），生产学习/掉落路径记 **PARTIAL**。
- 测试 6 例同名落盘（第 6 例按 M2-c 更名，plan §四 原名含 `ignores_players` 已被评审否决，数量仍 6）：`..._does_not_emit_126_or_149_and_chain_accepts_player_hero_candidates`；`--list` 恰 6 行 `: test` + `6 tests, 0 benchmarks`。
- **LOCK 口径条件化（M2）**：受控场景（候选充足且均为 Monster）总命中=`level+2`、125 条数=`level+1`；一般场景仅保证 `≤ level+2`；失败例 **≥3**：①`bounce` 用尽（level0 时 1 条 125 后停）②半径 3 内无候选即停 ③非 Monster 受害者不续跳**但可被选为次级命中目标**；125 时序断言**同一 tick 内先于该跳伤害**（先取包列表、再 `advance_clock` 到 `dist*50`）；固定种子**复跑 2 次同结果**。
- 命令 6 改全路径：`cargo test -p mir-protocol --lib packets::server::tests::default_packets_roundtrip -- --exact` → `1 passed; 0 failed`（M2/R9：短名 `--exact` 0 命中假绿）。
- 证据 `target/round57/firebounce/attempt-N/**`；英文断言、无 `#[allow]`、缺资源 panic 不 skip、TMP/TEMP=`target/tmp`、禁 commit/CI。

### B（礼部/gpt）G3 R56 承接清理（纯文档）

- **不碰任何 `.rs`**。`docs/world/magic.md` §9 支持矩阵（`:652`）新行**只允许**「FireBounce 服务端生产者（R57）+ 客户端 125 渲染 PARTIAL」，不得写「已接线/PASS」（M1）；`docs/world/aoi.md:291`「待 R56」→「待 R57+」；`docs/process/round56-plan.md:45` 证据路径→`target/round56/docs/attempt-1/00-evidence-greps.log`；`README.md:48` R52 行改写「R57 起服务端有生产者，客户端渲染仍 PARTIAL」；`docs/roadmap.md` 增 R57 状态并与 R55 maps 脚注统一；`docs/testing/round56/README.md:5` 受测 HEAD 改「代码基线 `e3b365f` / 门禁 HEAD `b5369d7`」（P6）；新建 `docs/testing/round57/README.md`（≤60 行，状态列仅「已落盘/待 T 抽核」，**不预写 PASS**）。
- 红线自查 `target/round57/docs/attempt-1/04-grep-redline.log`：`git diff -U0` **新增行**红线词（`PASS|已实现|已接线|全链`）计数=0，逐条带 exit；既有 `aoi.md:349/:353-355/:357/:359/:365` 旧 PASS 词**二选一**（清理到 0，或明确「未新增」并附行号清单+理由），**禁整文件 grep 冒充**。
- 证据 `target/round57/docs/attempt-1/**`。

### T（刑部/gpt，门禁后只读）

- 三产物：`docs/testing/round57/round57-final-audit.md`、`docs/testing/round57/final-manifest.json`（≤1MiB）、`target/round57/audit/summary.log`。
- 必核：①门禁原件真实（12 项/exit/日志 bytes+sha/HEAD 绑定）②候选 A/B **逐文件** sha MATCH（candidate 仅 A/B 文件）③M1 表述红线（「客户端全链/125 全链/R57 完整」零出现）④M2 测试/LOCK 口径与 ≥3 失败例 ⑤M3 A0 DB 实测原始日志 ⑥M4 命名统一 ⑦G1 服务端生产者=已实现、客户端渲染=PARTIAL ⑧卫生。
- T 产物走**第三次提交**（不纳入门禁候选 sha）；T **禁跑门禁/ignored、禁改代码/git 写操作**。

## 四、M1–M5 原样纳入（不得弱化）

**M1（高）范围与表述红线**：A/B/T 三份产物禁止出现「客户端全链交付/125 已全链/R57 完整验收」；T 终审必须逐条写「服务端生产者=已实现、客户端渲染=PARTIAL」；`docs/world/magic.md` 新行只允许「FireBounce 服务端生产者（R57）+ 客户端 125 渲染 PARTIAL」，不得写「已接线/PASS」；派生：根 `README.md:48` R52 行仍写「125 无服务端生产者」，B 必须改写为「R57 起服务端有生产者，客户端渲染仍 PARTIAL」，否则文档自相矛盾。

**M2（高）可证伪测试与 LOCK 口径**：LOCK 必须条件式——受控场景（候选充足且均为 Monster）总命中=`level+2`、125 条数=`level+1`；一般场景只保证 `≤ level+2`；失败例 ≥3（①`bounce` 用尽 ②无候选即停 ③非 Monster 不续跳但**可被选为**次级命中目标）；125 时序断言在同一 tick 内先于该跳伤害（先取包列表、再 `advance_clock` 到 `dist*50`），固定种子复跑 2 次同结果；**修正 C6**：候选种族按 Crystal 收 `Monster/Player/Hero`，测试名与断言**不得写 ignores players**，Hero 无实例则登记「Hero 无实例」而非改语义；命令 6 改 `cargo test -p mir-protocol --lib packets::server::tests::default_packets_roundtrip -- --exact` 并断言 `1 passed; 0 failed`；`--list`/`--exact` 均须复核实际运行数（禁 0 命中）；证据 `target/round57/firebounce/attempt-1/` 下 11 件齐全，`00-baseline.log` 记录 `cargo test -p mir-server --test magic` 首跑 passed 数。

**M3（高）A0 数据实测**：A0 第一动作实读默认 DB 是否含 `spell=154` 的 `MagicInfo`（读库并 grep，原始输出落 `00-db-firebounce.log`）；缺则测试显式注入 fixture（不改生产 DB、不伪造记录），生产学习/掉落路径记 PARTIAL；`grep` 证据与 `exit_code` 入 meta；**禁止为让测试绿而改 `mir-db` 解析或默认数据**。

**M4（高）门禁与提交绑定**：candidate=`git diff > target/round57/commit/candidate.patch` 只取 **A/B 候选文件**，`git_diff_sha256` 与逐文件 sha 只比这批；T 产物走**第三次提交**（文档/审计批，post-gate），不参与候选 sha 等式，§六.3 口径改为「候选文件 blob 逐一相等」；`attempt-N.meta.json` 必含 `head`、`git_diff_sha256`、`files[{path,sha256,bytes}]`、`log_sha256`、`log_bytes`、`started_at/finished_at`(+时区)、`exit_code`、`attempt`，首红 attempt 原样保留不覆盖；命令 13 预期清单补 `docs/process/round57-{menxia-review,shangshu-dispatch,shangshu-acceptance,menxia-confirm}.md`。

**M5（中）R56 承接缺口 + 红线 grep**：**P6** `docs/testing/round56/README.md:5` 受测 HEAD 错值，必须加入 B 的 G3 范围并改为「代码基线 `e3b365f` / 门禁 HEAD `b5369d7`」；**P3** `docs/world/aoi.md` 既有 7 处 PASS（`:349/:353-355/:357/:359/:365`）B 须二选一并落证（清理到 0，或把红线 grep 限定为 `git diff -U0` 新增行并附行号清单与理由），禁整文件 grep 冒充；**P5** 命名统一 `meta.json`（证据目录）/`attempt-N.meta.json`（门禁）/`final-manifest.json`（T），`docs/testing/round57/README.md` 索引名与 T 实物同名；B 红线自查落 `04-grep-redline.log`（新增行 `PASS|已实现|已接线|全链` 计数=0）；B 的 `docs/testing/round57/README.md` 只写「已落盘/待 T 抽核」；B 不碰任何 `.rs`。

## 五、门禁与提交（M4 修正）

- **Lead 唯一**执行 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`，12/12 exit 0；`attempt-N` 原件**首红不覆盖**；meta 含 HEAD + `git diff` sha + 候选文件 sha + 日志 bytes/sha。
- candidate 仅 A/B 文件；逐文件 blob sha 相等即通过。提交**三次**：①代码批（A：`magic.rs`/`tests/round57_firebounce.rs`/`world.rs` 注释）②文档批（B 文档 + process：`round57-plan.md`/`round57-menxia-review.md`/`round57-shangshu-dispatch.md`）③T 产物第三次文档提交。
- 时序：A0→A→B（可并行编辑，cargo 由 Lead 串行）→Lead 门禁→T→尚书验收→门下 confirm→Lead 提交推送。
- **黑名单**：禁暂存 44.8/45.2MB 冻结 JSON（`docs/testing/monster-native/player-resource-freeze*.json`）、`target/`；不得修改生产 DB；不得改协议线格式；不得发明「PASS/已实现」表述。

## 六、红线、PARTIAL 与回报

- 门禁后任何代码/文档改动 → 重跑 `attempt-N+1`；不接受口头一致。
- PARTIAL 终态：客户端 125 渲染/410+570 特效、Hero 无实例、「无效目标仍扣蓝」差异、默认 DB 缺记录时生产学习路径、次级目标随机序列（集合等价 MINOR）。
- 回报：A/B 向尚书报 `SUB_TASK_ID` + 证据目录 + 命令 exit；T 报三产物 path/bytes/sha256；Lead 报门禁 attempt 号 + 12 项 exit + 候选 sha 清单。
