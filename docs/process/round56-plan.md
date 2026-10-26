# R56 第56轮方案（中书省 / gpt-6.1-sol）

主单 `JJC-20261026-003`（第56轮：AOI 毫秒边界去 flake + AOI-5 生产接线裁决 + 文档同步）｜HEAD=`e3b365f440cfb6e979d92584ac34cacfcbb013ba`=`origin/main`｜规划 2026-10-26T12:4x+08:00（Asia/Shanghai）

只读轮：本文件是本轮唯一写入；未改 `crates/`、未跑 `cargo`/`ci.ps1`、未 commit。R55 主单 `JJC-20261025-005` 已 Done（PASS-WITH-PARTIAL，PARTIAL 6 条/13 项），本轮只承接其中可在单轮闭环的 P2/P4。

## 一、事实（文件:行号 / 实测）

| # | 事实 | 依据 |
|---|---|---|
| 1 | `aoi_acceptance_recycle_all_kinds` 在同一次运行里先后调用两次 `spawn_ground_item`，每次各自读实时时钟 | `crates/mir-server/tests/aoi.rs:1104`（fn）、`:1114`（普通物品）、`:1124`（金币堆） |
| 2 | `spawn_ground_item` 的到期时间 = 调用瞬间 `self.now() + item_expire_ms`，两次调用相隔 ≥1 实毫秒即得到相差 1 的 `expire_time` | `crates/mir-server/src/items.rs:1512`（入口）、`:1527`（带 expire 的内核）、`:1536`（`let now = self.now()`）、`:1544`（`expire_time: now + expire_ms`） |
| 3 | 回收判据是严格 `now > expire_time`，等于边界不回收 | `crates/mir-server/src/items.rs:1629`（`if now > ground.expire_time`）；测试注释 `tests/aoi.rs:1168-1169` |
| 4 | 测试以“普通物品”的到期时刻为基准快进：`boundary_delta = item_expire - world.now()`，再 `advance_clock(boundary_delta)`、`advance_clock(1)`；金币堆若晚 1ms 生成，则 +1 后仍差 1ms，金币堆不被回收 | `crates/mir-server/tests/aoi.rs:1173-1179`、`:1195`、`:1201-1204`（失败断言：`the expired coin pile must be removed by the World tick`） |
| 5 | 世界时钟未冻结时 = 真实流逝毫秒；`advance_clock` 会把当前时刻固化为偏移并冻结，之后 `now()` 只由偏移决定（正是为消除这类毫秒抖动而设计） | `crates/mir-server/src/world.rs:1376-1383`（`now`）、`:1389-1397`（`advance_clock` 注释与冻结实现） |
| 6 | 该用例并非孤例：R55 C 首跑 `target/round55/npc/attempt-1/03-aoi.log` 出现 1 红（`aoi.rs:1201`，`12 passed; 1 failed`），Lead 5 连跑 13/13 全绿、候选评审复跑 13/13 绿、C 单例重复 5 次 1 passed —— 判定为既有毫秒边界 flaky，非 R55 代码因果 | `docs/testing/round55/round55-final-audit.md:88`（§F）、`:64-81`（§E）；`target/round55/lead-aoi-flake/repeated-runs.log`（run1–run5 均 `13 passed`） |
| 7 | AOI 五路由验收例已全部存在，共 13 个 `#[test]`；`recycle_all_kinds` 是其中唯一依赖“多对象同刻到期”的一个 | `crates/mir-server/tests/aoi.rs:685`（enter）、`:813`（leave）、`:928`（change_map）、`:1104`（recycle）、`:1292`（broadcast）；`grep -c '#[test]' tests/aoi.rs` = **13** |
| 8 | `despawn_npc` 原语在 R55 已落（容器 + 格 + 广播 + 关系四要素），doc 明确“当前无生产调用者”，唯一调用者是 R55 新测试 | `crates/mir-server/src/npc.rs:804-811`（doc 段 + fn）、`:811`；`grep -rn "\.despawn_npc(" crates/` 仅 `tests/round55_npc_recycle.rs`（4 处） |
| 9 | Crystal 服务端对 NPC 的 `RemoveObject` 调用者为 **0**；`Envir.Remove(NPCInfo)` 只有定义、无服务端调用者 —— 生产“NPC 运行期回收”在原型里不存在 | `github/Crystal/Server/MirEnvir/Map.cs:2375-2383`（`:2378` `NPCs.Remove`）；`grep -rn "RemoveObject(" github/Crystal/Server` = 26 命中，传 `NPCObject` = 0；`Envir.cs:4367-4371` 仅定义 |
| 10 | 本作没有地图卸载/重载路径：`initialize_map` 对同一图幂等早退；地图缓存淘汰只淘汰“没有玩家/怪物/NPC/地面物品”的图，因此不会留下悬空 NPC | `crates/mir-server/src/maps.rs:410-412`（`initialized_maps` 早退）、`:434`（`spawn_npcs`）、`:871-900`（`evict_maps` 的 busy 集显式包含 `npcs`） |
| 11 | 生产 NPC 只在启动/首次进图刷出，且动态 `spawn_npc` 仅测试调用；没有“运行期新增 NPC”的生产路径可与回收配对 | `crates/mir-server/src/server.rs:531`（启动图初始化）、`crates/mir-server/src/world.rs:1833`（首次进图初始化）；`grep -rn "\.spawn_npc(" crates/` 无生产命中 |
| 12 | AOI 文档列了“NPC 刷出”，但没有“NPC 回收/无生产调用者”条目，存在文档缺口 | `docs/world/aoi.md:123-128`（`:124` “NPC 刷出”，无回收行） |
| 13 | 125（`S.ObjectProjectile`）在 Crystal 的唯一生产者是 `FireBounce` 的二次弹跳分支；本作 magic 无 bounce 实现，补 125 需先有弹跳玩法，不是一轮可闭环项 | `github/Crystal/Server/MirObjects/HumanObject.cs:5818-5827`（`:5826` 广播）；`grep -rn "bounce\|Bounce" crates/mir-server/src/magic.rs` = 0 |
| 14 | R55 门禁与提交口径仍是本轮契约：12 项、原始 exit、日志 bytes+sha、绑定受测 HEAD、两次提交、首红不覆盖、≤1MiB | `docs/process/round55-shangshu-acceptance.md:12-14,57-68`；`docs/process/team-and-roles.md:119-157` |

## 二、本轮可证伪目标

### G1（P4 关闭：AOI 毫秒边界 flaky 根因消除）
把 `aoi_acceptance_recycle_all_kinds` 改为**先冻结时钟再排两个到期对象**（`world.advance_clock(0)`），使两次 spawn 共享同一 `now`；新增 1 个确定性回归例 `ground_item_expiry_strict_ms_boundary`，显式构造“对象相隔 1ms 到期”，断言严格 `now > expire_time` 的三个时刻（边界不删、+1 删前件、+1 删后件）。**只改测试文件**。
- 验收命令与期望：
  1. `cargo test -p mir-server --test aoi -- --list` → **14 tests / 0 benchmarks**（原 13 + 新 1）。
  2. `cargo test -p mir-server --test aoi aoi_acceptance_recycle_all_kinds ground_item_expiry_strict_ms_boundary -- --nocapture` → `2 passed; 0 failed`，exit 0。
  3. 压力：同一精确用例重复 100 次（PowerShell 循环，逐次记录 exit）→ **100/100 exit 0**，日志 100 条 `1 passed`。
  4. **因果自证（首红保留）**：在 HEAD 原文件的两处 spawn 之间临时插入 `std::thread::sleep(Duration::from_millis(2))` → 必须 exit 101 且红在 `coin pile must be removed`；随后在修复版上再插同一 sleep → 必须 `1 passed`；两次都还原 sleep。旧树红/新树绿才算根因成立。
  5. 回归：`cargo test -p mir-server --test round55_npc_recycle -- --nocapture` → `1 passed`，exit 0。
- 证据：`target/round56/aoi/attempt-1/{00-list.log,01-fixed-2cases.log,02-repeat-100.log,03-prefix-red-with-sleep.log,04-fixed-green-with-sleep.log,05-npc-recycle.log,meta.json,files.json}`。
- 失败判定：100 次里任一红、或加 sleep 后修复版仍红、或新例未在 `--list` 出现 → G1 FAIL，P4 保持开启。

### G2（P2/P3 裁决：AOI-5 生产接线“不可接线”的可证伪结论）
不做“发明 NPC 生命周期”的接线；以四条独立证据判定 R56 **不具备真实接线点**：①Crystal 运行时 0 个 NPC 回收调用者；②本作无地图卸载，`initialize_map` 幂等；③`evict_maps` 排除有 NPC 的图；④动态 `spawn_npc` 仅测试。结论写进文档：AOI-5 整体维持 PARTIAL，`despawn_npc` 仍无生产调用者；**若发现任一真实生产路径需要回收，则 G2 证伪，必须改为实现接线或判 BLOCKED**。
- 验收命令与期望：
  1. `grep -rn "\.despawn_npc(" crates/` → 仅 `tests/round55_npc_recycle.rs`，生产 0 命中。
  2. `grep -rn "fn unload\|fn remove_map\|fn despawn_npc" crates/mir-server/src/` → 无 `unload`/`remove_map`，`despawn_npc` 仅定义。
  3. `grep -rn "RemoveObject(" github/Crystal/Server | grep -c NPCObject` → **0**；总数记录为 26（不写 10）。
- 证据：`target/round56/docs/attempt-1/00-evidence-greps.log`（原文（R56）：`target/round56/aoi/attempt-1/10-wiring-greps.log`；原路径未落盘，按实际证据路径勘误）；文档落点见 G3。
- 失败判定：出现生产调用点或地图卸载路径 → G2 FAIL，方案退回。

### G3（文档同步，不预判）
- `docs/world/aoi.md` 增一节“NPC 运行期回收（原语存在 / 生产零调用者）”，逐条列 G2 四条证据与 Crystal 锚点；不改既有 123-128 行结论。
- 新建 `docs/testing/round56/README.md`（≤60 行）：本轮单号、G1–G3 证据路径、状态列只写“已落盘/待 T 抽核”，**不得预写 PASS**。
- 验收：`py -3 tools/docs/check_links.py` → `[OK]`、exit 0；`grep -n "AOI-5 已关闭\|AOI 全量\|R56 完整" docs/world/aoi.md docs/testing/round56/README.md` → 0 命中；`git diff --check` exit 0。

### G4（门禁绑定，只由 Lead 执行）
唯一全量门禁 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale` 12/12 exit 0；原始 stdout/stderr、`exit_code`、起止时区、`HEAD`、`log_bytes`、`log_sha256` 落 `target/round56/gate/attempt-1*`；首红 attempt 不覆盖。两次提交：提交一仅 `tests/aoi.rs`，提交二 `round56-plan.md` + `docs/world/aoi.md` + `docs/testing/round56/**` + 审计件；分别记录受测 HEAD 与文件 sha。

## 三、子任务 DAG（3 个执行单 + Lead 门禁）

| key | owner | depends_on | 写范围（唯一写者） | 验收命令 |
|---|---|---|---|---|
| A | 兵部，deepseek-flash | 门下 approve | `crates/mir-server/tests/aoi.rs`（仅冻结时钟 + 新例；不碰 `src/`） | 见 G1 的 1–5 |
| B | 礼部，gpt-6.1-sol | 门下 approve；可与 A 并行（文件不重叠） | `docs/world/aoi.md`（仅新增一节）、新建 `docs/testing/round56/README.md` | 见 G3；证据 `target/round56/docs/attempt-1/` |
| T | 刑部，独立 gpt-6.1-sol，只读评审 | A、B 完成 + Lead 门禁 | `docs/testing/round56/round56-final-audit.md`、`round56-final-manifest.json` | 抽核 A/B 原件 sha、G1–G3 判定、门禁 12 项 |
| L | Lead | 门下 approve | 门禁日志、两次提交 | G4 |

- DAG：门下 approve → A ∥ B → Lead 全量门禁（唯一、串行）→ T → 尚书验收 → 门下 confirm → 提交二。
- A 的 `cargo` 与 B 的文档无共享写范围；`target`/`cargo` 锁由 Lead 排串行。
- A 实现口径：冻结调用必须在 `enter`/`drain` 之后、两次 spawn 之前；新例只断言 `expire_time` 差/严格边界，不新增生产 API；断言全英文。
- T 不兼职作者；T 只读，不改 A/B 文件。

## 四、门禁与提交口径（只由 Lead 执行）

1. 提交一（代码/测试）：`crates/mir-server/tests/aoi.rs`；先由独立 gpt 评审 diff，再跑唯一全量门禁；红则交 A 返工，新 attempt 从头保全字节，旧 attempt 不覆盖。
2. 提交二（文档/审计）：本计划、`docs/world/aoi.md`、`docs/testing/round56/{README.md,round56-final-audit.md,round56-final-manifest.json}`、尚书/门下件；逐文件显式 staging。
3. 原件：`target/round56/gate/attempt-N/`，含完整 stdout/stderr、原进程 `exit_code`、起止时区、命令/cwd、`HEAD`、候选文件 sha、`log_bytes`/`log_sha256`；`TMP`/`TEMP` 指向 `target/tmp`。
4. 每条定向命令留 `--list` 发现数、实际例名、有效用例数、原始 exit；`0 tests`/提前 return/跳过 一律不计 PASS。
5. >1MiB 只登记路径/bytes/sha 不入库；`target/`、`.env`、两份历史冻结 JSON 不纳入；`git diff --check` + 链接校验必过；新增断言一律英文，禁新增 `#[allow]`。

## 五、PARTIAL 与承接

| # | R55 PARTIAL | 本轮处置 |
|---|---|---|
| P1 | maps 条件 PASS（CI 默认 16 有效/4 skip，依赖 `MIR2_DEV_DB_PATH`） | **不动**；不并 CI 契约、不自动读 `.env`、不写死 DB 路径 |
| P2 | AOI-5 整体 PARTIAL（生产零调用者） | **维持 PARTIAL**；G2 给出“无可接线点”的可证伪结论，不发明地图卸载/NPC 生命周期 |
| P3 | `despawn_npc` 无生产调用者 | **维持 PARTIAL**；与 P2 同源，不重复计数升级 |
| P4 | `aoi_acceptance_recycle_all_kinds` 毫秒边界 flaky | **G1 关闭候选**；100 次 + sleep 自证全绿才可写 CLOSED |
| P5 | R52 承接三项（125 生产者/真 socket/原生 dash） | 原样顺延；本轮不碰 |
| P6 | R53→R54 承接 13 项 | 原样顺延；见 `docs/testing/round54/round54-carryover.md` |
| 新增 | 若 G1 的修复只覆盖单测、全量门禁仍红，或 G2 发现真实接线点 | 记新增未闭合项，门下裁决是否 R57 继续 |

- 计数口径：R55 的 6 条不合并、不拆分；本轮最多把 P4 改 CLOSED，P2/P3 仍各算 1 条。
- 任何“AOI-5 已关闭 / R56 完整”的正面表述即判假绿。

## 六、不做清单

- 不做 125 真生产者：Crystal 唯一生产者是 `FireBounce` 二次弹跳（`HumanObject.cs:5826`），本作无 bounce 机制，需先做弹跳玩法。
- 不做原生 dash 帧/视觉/音频/时序、不做真 socket/真实客户端窗口（无客户端 harness）。
- 不做 AOI-5 生产接线：Crystal 运行时 0 个 NPC 回收调用者、本作无地图卸载；强行接线=发明语义。
- 不改 CI 契约、不自动装载 DB、不写死 `resource/.../Server.MirDB`、不把 4 例 skip 写成 PASS。
- 不改 `crates/mir-server/src/{world,items,npc,maps}.rs` 任何生产逻辑；本轮唯一代码改动是 `tests/aoi.rs`。
- 不做 AOI 网格重写、Crystal 公式/RNG 修改、EnemyGuild/行会战、PK 完整矩阵；不加新工具、不加 `#[allow]`、不手删容器造绿。
- `D` 类顺延项（125/dash）再次明确顺延 R57，不静默丢失。

## 七、待门下裁决项（5）

1. **D1（AOI-5 不接线）**：接受“Crystal 0 调用者 + 无地图卸载 + evict 排除 NPC + 无动态生产 spawn”作为“R56 无可接线点”的充分证据，判 P2/P3 维持 PARTIAL 而非强行实现？若门下认为必须有生产调用者才算承接，请直接驳回本方案。
2. **D2（flake 修复只动测试）**：接受以 `advance_clock(0)` 冻结时钟 + 新增确定性回归例关闭 P4，而不改 `src/` 时钟/到期逻辑？
3. **D3（PASS 门槛）**：G1 的 PASS 是否必须同时满足“100 次重复全绿 + 旧树注入 2ms sleep 必红 + 新树同 sleep 必绿”三条？只满足前两条是否可判条件 PASS？
4. **D4（文档写范围）**：批准 B 只写 `docs/world/aoi.md` 新增节 + `docs/testing/round56/README.md`，本轮不更新 `docs/roadmap.md`（避免与 R55 基线脚注冲突）？
5. **D5（T 的时点）**：T 是否必须在 Lead 唯一全量门禁之后启动（推荐），还是允许 A/B 完成后先抽核、门禁后补签？前者串行更慢但证据链完整。
