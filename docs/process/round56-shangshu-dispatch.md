# R56 尚书省派发（shangshu / gpt-6.1-sol）

主单 `JJC-20261026-003`｜HEAD=`e3b365f`｜协调子任务 `JJC-20261026-003-sub-ff0cb009`｜2026-10-26T12:4x+08:00（Asia/Shanghai）

**输入（已定稿，逐字纳入）**
- `docs/process/round56-plan.md`（109 行，sha256 `a4473c9f…769d`，七节；G1 去根因／G2 接线裁决／G3 文档／G4 门禁，DAG 3 单 + Lead）。
- `docs/process/round56-menxia-review.md`（69 行，sha256 `611868cd…2f21`，**修改后批准**；M1–M7 必改，D1–D5 已裁决）。
**本件边界**：只落盘派发件 + 登记看板；不跑 cargo/ci.ps1、不改 `crates/`、不 commit、不 spawn 执行者（Lead 启动）。

## 一、边界与串行

- **A ∥ B 可并行**：写范围不重叠（A=`crates/mir-server/tests/aoi.rs`；B=`docs/world/aoi.md`+新建 `docs/testing/round56/README.md`）。
- **cargo 由 Lead 串行**：A 只用 §2-A 五条命令自证，不跑全量门禁；全量门禁唯一执行者=Lead（`pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`）。
- **T 必须门禁后**：启动前置=A/B 完成 + gate `attempt-1` 全量 12/12；T 只读，不改 A/B 文件（D5 裁决）。
- **单写者**：A 禁碰 `crates/mir-server/src/`；B 禁碰 `docs/world/aoi.md:123-128` 与 `docs/roadmap.md`；T 只写本契约三产物。
- **TMP/TEMP** 一律指向 `target/tmp`；`target/` 不入库。

## 二、逐单契约

### 2-A 兵部 bingbu（deepseek）｜R56-A AOI flake 去根因
- 写范围：仅 `crates/mir-server/tests/aoi.rs`（冻结调用 + 新例 + 断言）；禁改 `src/`、禁新增 `#[allow]`、断言全英文。
- **M1 `[M1→A:c1]` 冻结后显式断言**：`world.enter`/`drain` 之后、两次 `spawn_ground_item` 之前调 `advance_clock(0)`；两次 spawn 之后断言 `normal.expire_time == coin.expire_time`（差 0）。自证一次：临时注释掉 `advance_clock(0)` 必须红（自证不入库）。
- **M2 `[M2→A:c2]` 确定性新例 `ground_item_expiry_strict_ms_boundary`**：两次 spawn 之间 `advance_clock(1)` 构造确定 1ms 差，断言 `expire_b - expire_a == 1`；三时刻断言（a 边界两者都在、a+1 仅删 a、b+1 删 b），边界处无 `ObjectRemove`；只断言 `expire_time`/可见性，不新增生产 API。
- **M3 `[M3→A:c3]` 命令形态（禁双位置过滤器）**：`cargo test -p mir-server --test aoi -- --list` 先确认 **14 tests/0 benchmarks**；再**分别两次单例**运行（各 `1 passed`/exit 0），或整文件 `14 passed`；**禁** `cargo test … <A> <B>` 两个位置参数（cargo 只接受一个 TESTNAME，"2 passed" 不作证据）。
- 另两条必留：`round55_npc_recycle` 回归 `1 passed`；旧树插 2ms sleep 必红在 `coin pile must be removed` / 新树同 sleep 必绿（G1 因果自证，三条缺一即 FAIL）。
- 证据：`target/round56/aoi/attempt-N/`（原件、逐次 exit、日志 sha）；命令文本与原始 exit 全记。

### 2-B 礼部 libu（gpt）｜R56-B 文档同步
- 写范围：`docs/world/aoi.md` 仅**新增一节**「NPC 运行期回收（原语存在／生产零调用者）」+ 新建 `docs/testing/round56/README.md`（≤60 行）。
- **M4 `[M4→B:c1]` 红线**：新节逐条列四条证据 + Crystal 锚点（`Map.cs:2375-2383`、26 处 `RemoveObject(` 中 0 处传 `NPCObject`）+「`despawn_npc` 无生产调用者」；**不改** `docs/world/aoi.md:123-128`；README 状态列只写「已落盘／待 T 抽核」，全文不得出现 PASS/已关闭/已接线；README 记一行「roadmap 基线脚注差异留待 R57 统一」。
- 验收：`py -3 tools/docs/check_links.py` → `[OK]` exit 0；`grep -n "PASS\|已关闭\|已接线" docs/world/aoi.md docs/testing/round56/README.md` 命中 **0**；`git diff --check` exit 0。
- 证据：`target/round56/docs/attempt-N/`（两文档 diff、两条命令原始 exit 与日志 sha）。

### 2-T 刑部 xingbu（gpt）｜R56-T 门禁后只读终审
- 启动前置（**M5 `[M5→T:c1]`**）：Lead 全量门禁 `attempt-1` 12/12 之后才启动；只读，不兼职作者。
- 抽核：A/B 文件 sha256 与 `target/round56/gate/attempt-1/meta.json` 的 HEAD/文件 sha 一致；G1 只认**原件三件套**（100 次逐次 exit、旧树红在 `coin pile must be removed`、新树绿），不认 A 自述；G2 复核 grep 原始输出；M1–M4 逐条抽查落实。
- 产物：`docs/testing/round56/round56-final-audit.md`、`docs/testing/round56/round56-final-manifest.json`、`target/round56/audit/summary.log`；每项带原件路径 + sha，无原件不得写"通过"；禁宣称「AOI-5 已关闭」或「R56 完整」。

## 三、M1–M7 落点（审计 grep 用）

| # | 级 | 归属 | 落点 | 审计 grep |
|---|---|---|---|---|
| M1 | 高 | A | §2-A `[M1→A:c1]` 冻结后 `normal.expire_time == coin.expire_time` 断言 + 注释冻结必红自证 | `M1→A` |
| M2 | 高 | A | §2-A `[M2→A:c2]` 新例 `advance_clock(1)` 确定 1ms 差 + 三时刻 + 边界无 `ObjectRemove` | `M2→A` |
| M3 | 高 | A+Lead | §2-A `[M3→A:c3]` `--list`=14 + 两次单例各 `1 passed`；§四 `[M3→Lead]` 门禁复核命令形态 | `M3→A` `M3→Lead` |
| M4 | 高 | B | §2-B `[M4→B:c1]` 新节四证据/不改 123-128/README ≤60 行/状态列只「已落盘·待 T 抽核」/grep 0 命中 | `M4→B` |
| M5 | 中 | T | §2-T `[M5→T:c1]` 门禁后启动、只读、sha 与 gate meta HEAD 绑定、只认原件 | `M5→T` |
| M6 | 中 | Lead | §四 `[M6→Lead]` 逐命令 `--list` 发现数/原始 exit/100 行 `1 passed`/首红不覆盖/提交一 `git diff --stat` 仅 `tests/aoi.rs` | `M6→Lead` |
| M7 | 低 | Lead | §五 `[M7→Lead]` 登记 `crates/mir-server/src/world.rs:1378` 注释含 0x07(BEL)，本轮禁改 `src/`，留 R57 | `M7→Lead` |

## 四、门禁与提交（只由 Lead 执行）

- `[M6→Lead]` 唯一全量门禁 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale` → 12/12 exit 0；每条定向命令记 `--list` 发现数、实际例名、有效用例数、原始 exit；**0 tests/提前 return/跳过一律不计 PASS**。
- 原件落 `target/round56/gate/attempt-N/`：完整 stdout/stderr、原进程 `exit_code`、起止时区、命令/cwd、`HEAD`、候选文件 sha、`log_bytes`/`log_sha256`；`TMP`/`TEMP`=`target/tmp`；**首红 attempt 不覆盖**，失败项全列不降级。
- 提交一（代码批）：仅 `crates/mir-server/tests/aoi.rs`；先独立 gpt 评 diff，再门禁，红则交 A 返工；提交前 `git diff --stat` 仅此一文件。
- 提交二（文档批）：`round56-plan.md` + `round56-menxia-review.md` + 本派发件 + `docs/world/aoi.md` + `docs/testing/round56/{README.md,round56-final-audit.md,round56-final-manifest.json}`；逐文件显式 staging。
- 两次提交均记受测 HEAD 与各文件 sha256；`>1MiB` 只登记路径/bytes/sha 不入库；`git diff --check` + 链接校验必过。
- 提交二前核对 `[M7→Lead]` 的 R57 承接登记已存在（T 审计件或 README 一行）。

## 五、黑名单与风险

- 禁改：`crates/mir-server/src/{world,items,npc,maps}.rs` 生产逻辑、CI 契约、`.env`、`docs/roadmap.md`、历史冻结 JSON；`target/` 不入库。
- 禁法：`#[allow]`、手删容器造绿、cargo 并发、管道吞 exit（100 次循环须逐次取 `$LASTEXITCODE`）、`AOI-5 已关闭`/`R56 完整` 等正面表述。
- `[M7→Lead]` 既有缺陷 `crates/mir-server/src/world.rs:1378` 注释 `advance_clock` 首字母为 0x07(BEL) 控制字节；本轮禁改 `src/`，R57 承接（README 或审计件一行）。
- 风险登记：R1 冻结位置漂移（靠 M1 拦）｜R2 循环吞 exit（假全绿）｜R3 双位置过滤器（命令错误记成红/绿）｜R4 T 只看自述（M5 绑定原件）｜R5 BEL 污染｜R6 预写 PASS（M4 grep）｜R7 `>1MiB` 入库。

## 六、回执与 CLI 结果

| 序 | 命令（`--as shangshu`） | 结果 |
|---|---|---|
| 1 | `delegate … bingbu "R56-A …"` | EXIT=0｜`📋 委派 JJC-20261026-003-sub-79e17f4a：尚书省 → 兵部（depth=1）` |
| 2 | `delegate … libu "R56-B …"` | EXIT=0｜`📋 委派 JJC-20261026-003-sub-0ba8f8de：尚书省 → 礼部（depth=1）` |
| 3 | `delegate … xingbu "R56-T …"` | EXIT=0｜`📋 委派 JJC-20261026-003-sub-280260aa：尚书省 → 刑部（depth=1）` |
| 4 | `state JJC-20261026-003 Doing` | EXIT=0｜`✅ JJC-20261026-003 状态更新：Assigned → Doing` |
| 5 | `memo JJC-20261026-003 shangshu "R56派发完成…"` | EXIT=0｜`📝 JJC-20261026-003 任务记忆：shangshu` |
| 6 | `validate` | EXIT=0｜`✅ 看板校验通过：149 条任务符合 schema v3.1` |

- 子任务 ID：A=`JJC-20261026-003-sub-79e17f4a`｜B=`JJC-20261026-003-sub-0ba8f8de`｜T=`JJC-20261026-003-sub-280260aa`；主单 state=`Doing`；六步全 exit 0，无偏差。

- 子任务 ID ↔ 执行者映射、主单 state、validate 结果由尚书回报 Lead；任一 `exit≠0` 原样记录并停止后续步骤，不改 JSON、不绕过。
