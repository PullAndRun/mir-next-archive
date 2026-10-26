# 第 57 轮（R57）刑部独立终审报告

- 子单：`JJC-20261026-004-sub-e9251ec2`（刑部 T，只读）
- 审计对象：HEAD `858147bc2e5118c64a09084d03ee7c008294adc9`（main = origin/main）
  - 代码提交 `025564c272f9ede43a33918a04f22619fe137540`（14:20:38 +0800）
  - 文档提交 `858147bc2e5118c64a09084d03ee7c008294adc9`（14:20:56 +0800）
- 门禁：`target/round57/gate/attempt-1.log`（199,054 B，sha256 `73f823e8…`，meta EXIT=0，2026-10-26T06:21:16–06:25:15Z，237.8 s）
- 结论：**PASS-WITH-PARTIAL**（代码语义与测试证据成立；门禁日志缺 12 项汇总与 fmt 证据，文档仍有 125 旧口径）
- 允许进尚书验收：**有条件允许**（见 §四；尚书签发 12/12 前须补 H1，合入前须清 M1）

---

## 一、A–H 判定表

| 项 | 判定 | 一句话理由 |
|---|---|---|
| A 门禁真实性 | **PARTIAL** | log/meta 字节·sha·head·起止·exit 自洽，R57 6 用例在册；但无 `== 汇总 ==`/`12 项`/exit 行，fmt 门禁无输出 |
| B 提交边界 | **PASS** | 13/13 git-object sha MATCH；代码提交仅 3 文件、文档提交仅 10 文件；工作树仅 2 个历史冻结 JSON |
| C 语义对齐 Crystal | **PASS** | C1–C4、C6 逐条对齐；C5（125 不回显施法者）为模块文档明示的已知偏差；终局 RNG 抽签差异见 M2 |
| D 测试质量 | **PASS** | 6 named、受控 LOCK、125 先于伤害、失败例≥3、固定种子复跑、英文断言、无 allow/rand、无 126/149 |
| E 证据复现 | **PASS** | 6/43/7/8/1/2/1 与 fmt/clippy 日志、exit 齐全，首红保留、红→绿诚实；A0 真实读默认 DB |
| F 文档 M1–M5 | **PARTIAL** | README:48/:76、aoi:291、round56-plan:45、红线三分记录成立；magic.md 4 处旧「125 无服务端生产者」+ :687「代码批在途」未同步 |
| G 假绿/捷径 | **PASS** | 15b 已识破错误路径假绿并用真实路径取 1 passed；无 0 tests/过滤器/提前 return/skip；run-checks.sh:33 仍留错路径（非阻断，M3） |
| H 入库卫生与 PARTIAL | **PARTIAL** | 无 >1MiB 新文件、无 target/.env；PARTIAL 索引不全（125 不回显、Hero、can_fly 双实现、≤ 上限未入索引） |

---

## 二、逐项证据

### A. 门禁真实性 — PARTIAL

已核通过：

1. **逐字段一致**：`sha256sum target/round57/gate/attempt-1.log` = `73f823e8…`、`wc -c` = 199,054，与 meta `LOG_SHA256`/`LOG_BYTES` 一致；meta `HEAD=858147bc…`、`START_UTC=06:21:16Z`、`END_UTC=06:25:15Z`、`DURATION_S=237.8`、`EXIT_CODE=0`、`GIT_DIFF_SHA256=e3b0c442…`（空 diff，对应干净工作树）。
2. **R57 独有测试在册**：`attempt-1.log:2696-2707` 为 `Running tests\round57_firebounce.rs`，6 个 named 全 `ok`，`6 passed; 0 failed; 0 ignored`。全日志所有 `test result:` 均 0 failed。不是旧轮日志冒充。
3. **12 项可识别（11 项）**：clippy（`:1-3` `Finished dev…16.51s`）、duo_smoke（`:5-20`）、workspace（多段）、protocol-gen（`:3119` `packets: 432, files changed: 0`）、check_links（`:3120` `[OK] 1706 条相对链接…`）、TextKey、scan70（`:3223` 66 处）、scan0（`:3226` 0 处）、legacy、icons（`:3230-3231` kept=1346/违规=0）、mir-store（`:3261-3288`）。
4. **无 soak 记录**：`grep -c soak attempt-1.log` = 0，无 soak.lock 或残留进程说明。
5. 日志内 tracing 时间戳（如 06:24:28Z）落在 meta 起止窗口内，时长与 237.8 s 相容。

未通过（PARTIAL 依据）：

1. **无 12 项汇总/末行**：`grep -nE '== 汇总 ==|全部通过|12/12|EXIT' attempt-1.log` 0 命中；末行是 `:3288 all doctests ran in 0.98s…`。`tools/dev/ci.ps1:165-176` 必然打印 `== 汇总 ==`、逐项 `OK/FAIL` 与 `==> 全部通过（12 项）`；日志中同样无任何 `==>` 横幅（`ci.ps1:36/80/95/98/126/157`）。
2. **fmt 门禁无输出**：`cargo fmt --all --check` 成功时静默，且其横幅随 Write-Host 丢失；12 项中唯一无法从日志复核的就是 fmt。
3. **成因判断（不判假绿）**：现象与「PowerShell 仅重定向 success 流（`>`），Write-Host 走信息流未落盘」完全吻合——已捕获的 11 项原生命令输出、编译耗时与 R57 用例均真实，meta 的 EXIT=0 与 sha 自洽。故判**捕获不完整**，判 PARTIAL 而非 FAIL；但尚书不得据此直接签「12/12 PASS」，须重跑并用 `*>` 或 `2>&1 | Tee-Object` 完整捕获。

另：用户描述 meta 为 8 字段，实际为 11 个键 + 13 行候选文件（bytes/sha），非缺项。

### B. 提交边界 — PASS

- **13/13 MATCH**：`git show 858147b:<path> | sha256sum` 与 `target/round57/commit-verify/01-tree-hashes.txt` 全 13 行一致（含 magic.rs `0f2a0da5…`、world.rs `06959962…`、测试 `c5ce11b2…`、magic.md `5da5a83a…`）。文件 bytes 亦一致。
- **代码提交仅 3 文件**：`git show 025564c --stat` = magic.rs（+269/−5）、world.rs（+1/−1）、`tests/round57_firebounce.rs`（新 778 行）。
- **文档提交仅 10 文件**：README、round56-plan、round57-plan/menxia-review/shangshu-dispatch、roadmap、round56/README、round57/README、aoi.md、magic.md。
- **工作树干净**：`git status --porcelain` 仅 2 个未跟踪历史 JSON：`docs/testing/monster-native/player-resource-freeze(-before).json`。
- **B 未触碰 .rs**：B meta `constraints.rs_touched=false`，B 的 7 个交付全为 docs/README；构建树中 B 先于 A 提交，A 的 3 文件不含 docs。
- **A 未触碰 docs（除 world.rs:1378）**：025564c 无 docs 文件；world.rs 的 1 行 diff 为删除注释中的 0x07 BEL 控制字节（`cat -v` 前后对照见 `11-world-bel-before.log`/`12-world-bel-after.log`），无代码语义变更。**残留瑕疵**：该注释仍为 `dvance_clock`（原 BEL 顶替了字母 a），属 R56 遗留文本损坏，建议顺手修为 `advance_clock`（L7，非阻断）。

### C. 语义对齐 Crystal — PASS

Crystal 路径：`github/Crystal/Server/…`（用户写 `github/Suprcode/Crystal`，实际 clone 在 `github/Crystal`）。

- **C1 注册**：`crates/mir-server/src/magic.rs:755` `Spell::FireBounce => SpellKind::FireBounce`，不再落入 `_ => SpellKind::Unsupported`；`REGISTERED_UNSUPPORTED`（`:347`）不含 FireBounce，单测 `:5796-5797` 断言「登记未实现者必须仍为 Unsupported」，不矛盾。
- **C2 首跳**：cast 分支 `magic.rs:1545-1563`；首跳排期 `:2052-2071` 为 `MaxDistance(CurrentLocation,target)*50 + 500`、`bounce = level+2`，**不产 125**；伤害 = `attack_power(MinMC,MaxMC)` 后 `magic.damage(info, base, rng)`，即 `(base+GetPower())*Multiplier`（`:440-470`）。与 Crystal `HumanObject.cs:5811-5836` 一致；测试 750 ms 精确落地（`04-suite.log` case2）、level3 damage=26 = 攻 10 + 威力 16。
- **C3 续跳**：命中处理 `:2749-2834`：`magic_target_hittable` → MAC 伤害 → `dealt>0` 才 `level_magic` → 仅 `self.monster(target_id)` 存在（Race=Monster；尸体仍是 Monster）才续跳 → `remaining = bounce-1`，`:2775` `remaining==0` 返回 → `fire_bounce_next_target` → **先 encode+broadcast `ObjectProjectile{spell, source=本跳 victim, destination=next}` 再 push `PendingMagic{at=now+dist*50}`**。对照 Crystal `:5934-5957`（先 Broadcast 再 DelayedAction，`:5947` 仅 `target.Race==ObjectType.Monster`，`:5942` dealt>0 才 LevelMagic，`:5953 --bounce`）一致；测试证明 level3→5 命中/4 条 125，且每条 125 在该跳伤害前一个 tick 到达。
- **C4 候选**：`fire_bounce_next_target` `:2953-3004`：±3 逐格、`can_fly_cell_valid`（`:4047`，逐格 `poison_cloud_cell_valid` = in_bounds + `walkable`，对应 Crystal `Cell.Valid`/`Map.cs:2477-2480`）、怪物过滤 `!dead && attack_target && can_fly`、玩家按 AOI cell + `!dead` + `is_attack_target_player` + `can_fly`、对象 ID 排序 + `MirRng.next_u32()%len`、空候选返回 None。Crystal `FindAllNearby(3)`（`MonsterObject.cs:2347-2384`，只收 Monster/Player/Hero）语义等价；Hero 无实例已登记。
- **C5 已知偏差**：本作 `World::broadcast` 沿用「不回显施法者本人」；Crystal `Map.Broadcast`（`Map.cs:2437-2448`）包含施法者。**该偏差已在 `crates/mir-server/tests/round57_firebounce.rs:15-18` 模块文档明示**，本报告亦登记为 PARTIAL（P1）。测试一律用 observer 观测 125，未把该偏差当作等价。
- **C6 协议零 diff**：025564c 三文件不含 `crates/mir-protocol/**`，`ObjectProjectile` 沿用既有线格式。

**M2（语义 note）**：Crystal 在 bounce 递减到 0 的那一帧**仍先抽一次候选 RNG**再递归、由入口 `bounce==0` 返回；本作 `:2775` 在 `remaining==0` 时**先 return 再做候选**。链路包与伤害完全一致，但共享 RNG 流少走一次取模，后续随机序列与 Crystal 不同。无文档登记，建议注明（不影响本轮 LOCK 断言）。

### D. 测试质量 — PASS

`crates/mir-server/tests/round57_firebounce.rs`（778 行）：

- `03-list.log:4-9` 恰 6 个 named，`6 tests, 0 benchmarks`；`04-suite.log` 6 passed EXIT=0；`05-exact-case3` 1 passed/5 filtered（非 0 命中）。
- **受控 LOCK 真实受控**：case3（`:465-535`）5 只 1 HP、x 轴间距 3 的怪；每跳前一只死亡被 `IsAttackTarget` 排除，半径 3 内唯一候选，路径与种子无关；level3 精确 5 命中 / 4 条 125，预算用尽后 0 条。
- **125 先于伤害**：每跳在同一 tick 先 `packets(observer)` 断言 125 与 `hp(next)==1`，再 `elapse(150)` 才扣血；不是事后补断言。
- **失败例≥3**：非法初始目标（`:309`）、无候选即停（`:644` b）、bounce 用尽（`:644` a）、尸体/半径外过滤（`:590`）、非 Monster 不续跳（`:717`）。
- **固定种子复跑**：seed 41 连跑 2 次结果相等（`:606-614`）；seed 42..=57 证明怪物与玩家均可被选中（`:617-636`）。
- **断言英文**：所有 `assert!` 消息英文；无 `#[allow]`、无 `thread_rng`/`rand::random`（`19-compliance.log` 与全文核对）；无 126/149（helper `:289` 全程扫描）。
- 第 6 例名 `…_chain_accepts_player_hero_candidates`，不含 `ignores_players`；Player 候选真实构造（`:549`、`:721`）。
- 数据集缺失时 panic 不 skip（`:50-58`），没有静默假绿。

### E. 证据复现 — PASS

`target/round57/firebounce/attempt-1/` 原生日志齐全，exit 可核：

| 项 | 日志 | 结果 |
|---|---|---|
| R57 套件 | `04-suite.log` | 6 passed, EXIT=0 |
| magic lib | `06-magic-lib.log` | 43 passed / 158 filtered, EXIT=0 |
| round53 | `07-round53-frost-pvp.log` | 7 passed, EXIT=0 |
| round52_regression | `08-round52-regression.log` | 8 passed, EXIT=0 |
| round55_npc | `09-round55-npc-recycle.log` | 1 passed, EXIT=0 |
| client wire | `16-client-round52-spells-wire.log` | 2 passed, EXIT=0 |
| protocol | `15-protocol-roundtrip.log` | 真实路径 1 passed / 177 filtered, EXIT=0 |
| magic 套件基线 | `13-test-magic.log` | 24 passed, EXIT=0 |
| poison cloud | `14-round52-poison-cloud.log` | 8 passed, EXIT=0 |
| fmt / clippy | `01/02/10-clippy.log` | EXIT=0；`10-clippy-first-fail.log` EXIT=101 首红保留（doc_markdown） |

- **红→绿诚实**：clippy 首红 101 原样保留，随后 0 warning；无「测试首红被删」迹象（首轮即绿，未伪称红）。
- **A0 真实读 DB**：`00-db-firebounce.log` 记录 `dataset: resource/community/suprcode-crystal-database/cjlaaa/Server.MirDB`（文件实测 452,106 B）、`magic_count=107 distinct_magic_count=107`、`FOUND FireBounce(154): base_cost=5 level_cost=1 range=9 levels=(15,18,21) power=(10,0) m_power=(6,0) delay_base=1800…`、`test … ok`、`# exit: 0`。字段与代码使用一致：`levels`→`magic.level`、`power/m_power`→`UserMagic::power()`（10 + 6/4×(3+1)=16）、`range`→施法距离校验、`delay_base` 本技能冷却路径。**A0 是临时探针测试**（提交树仅 6 个 named，见 L2），数据集本身存在，结论可复核但探针不可直接复跑。
- `15b` 诚实记录错误路径会 0 passed/178 filtered 的假绿，并给出真实路径 1 passed。

### F. 文档 M1–M5 — PARTIAL

成立部分：

- `README.md:48`：仍保留 R52 历史 `attempt2 PASS（12/12）` 与 `PASS-WITH-PARTIAL`，新增句为「**R57 起服务端有生产者（FireBounce），客户端 125 渲染仍 PARTIAL**」，并明写「不能夸大为完整闭环」；`:76` 派生同步为同一口径。无「125 全链」。
- `docs/world/aoi.md:291` 已由「待 R56」改「待 R57+」，且 `git show 858147b -- docs/world/aoi.md` 仅此 1 行 diff；既有 7 处旧 PASS（`:349/353/354/355/357/359/365`）非新增、均带「仅限列出的断言范围」限定（`:349`、`:365` 还有防冒充实证句）。
- `docs/process/round56-plan.md:45` 勘误为实际证据路径并保留「原文（R56）：…」，可追溯；原路径目录不存在已由 B 记录。
- 红线三分记录可信：`03-grep-redline.log` 17 条新增行 3 条命中（README:48/76 为 R52 历史结论、roadmap:143 为任务钦定「条件 PASS」）+ 新文件 2 条（round57 README:5 否定式「全链未交付」、:15 引用钦定口径）；`new_unverified_claim_hits=0`，词级 `--word-diff` 复核一致。
- 门下 M1–M5 在 `docs/process/round57-menxia-review.md:50-80` 齐全，派发件同文。

未成立部分（M1）：

- `docs/world/magic.md:39`（R52 历史段）、`:685`（PoisonCloud 行）、`:774`（§11）、`:806`（B2 表）仍写「**125 无服务端生产者**」；R57 起该全局判断已为假（本技能 125 有生产者），与 README:48/:76 自相矛盾。
- `docs/world/magic.md:687` 矩阵状态仍「🟡 **代码批在途** … 待 T 抽核」；提交/门禁后应至少改为「服务端生产者已落盘/已过门禁，客户端 125 PARTIAL」。
- `docs/testing/round57/README.md` 仍称「A 批代码批在途、门禁未运行」，属门禁前口径（提交时真实），提交后已过时；不单独计 finding，随 M1 一并同步。

### G. 假绿/捷径 — PASS（1 项非阻断偏差）

- **命令 6 路径笔误已识破**：`docs/process/round57-shangshu-dispatch.md:37`、`round57-menxia-review.md:61`、`target/round57/firebounce/attempt-1/run-checks.sh:33` 均写 `packets::server::tests::default_packets_roundtrip`（错误模块段）；A 的 `15-protocol-roundtrip.log:1` 用真实路径 `packets::server::roundtrip::default_packets_roundtrip` 得 1 passed，`15b` note 明确记录错误路径 0 passed/178 filtered 的假绿风险。按用户口径记**非阻断偏差**（M3）。
- **脚本与证据不同源**：`run-checks.sh` 第 33 行仍为错误路径，若照脚本重跑会产出 0 passed，与 15 日志不符；且脚本不覆盖 `00-db`、`10-clippy-first-fail`、`11/12`、`15b`、`19` 等手工件。属复现链缺陷，不影响 A 日志本身可信。
- 其余：无 0 命中测试被当 PASS（15b 例外已自曝）、无过滤器名错通过、无提前 return/skip 冒充、无 `#[ignore]` 新增、无 126/149。

### H. 入库卫生与 PARTIAL 清单 — PARTIAL

- 无 >1 MiB 新文件：R57 两提交新增最大为测试 29,268 B，docs 最大 66 KB；仓库内 1.22 MB `attack3-asset-plan.json` 为既有文件，不在本轮提交。
- 无 `target/`、`.env` 入库；`Cargo.lock` 为既有跟踪文件（正常）。
- 无「125 全链」/「R57 完整」正面宣称：README:48「不能夸大为完整闭环」、roadmap §七「不得表述为闭环」、round57 README:5「125 全链未交付」均为否定式，符合红线。
- **PARTIAL 清单不完整**：`docs/testing/round57/README.md §4` 只列 3 项（客户端渲染、默认 DB 缺记录、随机序列 MINOR），未列：125 不回显施法者（模块文档 :15-16 有）、Hero 无实例（:17-18 有）、`can_fly` 双实现（代码注释 :4039-4047 有）、「一般场景 ≤ level+2」（仅派发 M2 有）、协议路径笔误、门禁捕获缺口。模块文档与索引口径不一致，记 PARTIAL（L6）。

---

## 三、Findings

### High（尚书签发前必须关闭）

**H1 门禁 12/12 与 fmt 无法从日志取证**
- 证据：`attempt-1.log` 无 `== 汇总 ==`、逐项 OK/FAIL、`==> 全部通过（12 项）`、无 EXIT；`greps` 0 命中；末行 `:3288` 为 doctests。`tools/dev/ci.ps1:165-176` 必打印上述内容；日志中连 `==>` 横幅都没有。12 项里 11 项有原生命令输出，唯 fmt（成功静默）不可见。
- 影响：meta `EXIT_CODE=0` 是 Lead 记录值，虽与日志字节/sha 自洽、R57 6 用例与各门禁输出真实，但「12/12」不是从日志独立可证的结论。
- 必改：用 `pwsh -File tools/dev/ci.ps1 *> target/round57/gate/attempt-2.log`（或 `2>&1 | Tee-Object`）重跑重捕；若坚持 attempt-1，须 Lead 出具原始终端回执/汇总补件。尚书不得据 attempt-1 签 12/12。

### Medium

**M1 magic.md 旧 125 口径与过时状态**
- `docs/world/magic.md:39/:685/:774/:806`「125 无服务端生产者」与 R57 事实矛盾；`:687`「代码批在途」过时。README:48/:76 已改，magic.md 未同步。
- 必改：四处改为「FireBounce 已提供 125 服务端生产者；客户端渲染/其它技能边界 PARTIAL」，矩阵行状态改「服务端生产者已落盘，客户端 125 PARTIAL」。

**M2 终局 RNG 抽签差异未登记**
- `magic.rs:2775` 在 `remaining==0` 先 return；Crystal `:5953` 先 `FindAllNearby`+随机抽一个，再递归被 `bounce==0` 拒绝。链路结果相同但共享 RNG 少走一次取模。
- 建议：在 magic.rs 注释或 magic.md PARTIAL 中登记「终局少一次抽签，无包差异，后续 RNG 序列可能不同」。

**M3 协议命令路径笔误与脚本不同源（用户已定非阻断）**
- `dispatch:37`、`menxia-review:61`、`run-checks.sh:33` 用错路径；15 日志用真实路径。修 run-checks.sh 并注明 15 为人工更正运行。

### Low

- **L1** B `meta.json` 的 magic.md sha `05760679…` 是工作树 CRLF 哈希，git blob 为 `5da5a83a…`（`file` 确认 CRLF）；同一文件两个 sha，建议统一按 git blob 记账，并统一该文件 EOL。
- **L2** A0 探针测试未进提交树（HEAD `--list` 仅 6 个），`00-db` 结论不能从 HEAD 直接复跑；数据集与字段已交叉核对。
- **L3** clippy 第二轮红（测试 `:288` doc_markdown、`:542` type_complexity）只有 `10b` 文字 note，无独立日志。
- **L4** `18-exit-summary.log` 中 00-db 的 EXIT 为空（日志内 `# exit: 0`）；`12-bel-after.log` 末条 `git diff --stat` 无输出无 EXIT（世界线已提交）；汇总脚本不覆盖 8 类手工件。
- **L5** `attempt-1.log:143` 存在 GBK 乱码（`鐪熷疄绱犳潗`），PowerShell 输出编码问题。
- **L6** PARTIAL 索引不完整（见 H 节），模块文档有、round57 README 无。
- **L7** `world.rs:1378` 注释仍为 `dvance_clock`（原 BEL 顶掉字母 a），建议修回 `advance_clock`（纯注释）。

---

## 四、是否允许进尚书验收

**允许进入尚书验收，但为有条件通过：**

1. **阻断签发**：H1 未关闭前，尚书不得签发「12/12 PASS / 全量门禁通过」，也不得据此宣称 R57 完整。允许以「代码与测试 PASS-WITH-PARTIAL、门禁证据待补」记录。
2. **合入前必改**：M1（magic.md 四处旧口径 + `:687` 状态）。
3. **同批非阻断**：M2 登记、M3 修脚本路径；L1/L2/L4/L6 在 R58 承接清单登记即可。
4. **不得宣称**：125 全链、客户端渲染闭环、R57 完整验收。服务端生产者=已实现；客户端渲染=PARTIAL；125 不回显施法者=已知偏差。

结论：**PASS-WITH-PARTIAL**；建议尚书以「条件通过 + H1 补件」处理，不建议 BLOCK（代码/测试/语义证据成立，问题集中在证据捕获与文档同步）。

---

## 五、复现命令（只读，未执行 cargo/ci）

```bash
sha256sum target/round57/gate/attempt-1.log            # 73f823e8… / 199054 B
tail -5 target/round57/gate/attempt-1.log              # 无汇总行（H1）
grep -nE '== 汇总 ==|全部通过|EXIT' target/round57/gate/attempt-1.log   # 0 命中
sed -n '2696,2707p' target/round57/gate/attempt-1.log  # R57 6 named 全 ok
git show 858147b:<path> | sha256sum                    # 13/13 MATCH
git show 025564c --stat                                # 3 files
git show 025564c -- crates/mir-server/src/world.rs     # 1 行 BEL 移除
git show 858147b -- docs/world/aoi.md                  # 仅 :291 一行
grep -n '125 无服务端生产者' docs/world/magic.md        # 4 处旧口径（M1）
grep -n 'packets::server' docs/process/round57-shangshu-dispatch.md \
  docs/process/round57-menxia-review.md \
  target/round57/firebounce/attempt-1/run-checks.sh    # 错误路径（M3）
```

复核计数：`04-suite` 6、`06-magic-lib` 43、`07` 7、`08` 8、`09` 1、`16` 2、`15` 1、`13` 24、`14` 8，均 EXIT=0。

---

## 六、只读声明

- 未修改任何 `crates/**`、`docs/**` 既有文件；未 commit/push；未运行 `cargo`、`tools/dev/ci.ps1`、ignored 测试。
- 仅新建 3 个产物：本文件、`docs/testing/round57/final-manifest.json`、`target/round57/audit/summary.log`。
- 审计时间：2026-10-26T14:25–14:34 +08:00（预算 15 分钟内）。
