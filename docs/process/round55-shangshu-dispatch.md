# R55 尚书省派发（gpt-6.1-sol）

主单 `JJC-20261025-005`｜HEAD=origin/main=`67c618c`｜派发 2026-10-26T11:5x+08:00（Asia/Shanghai）
依据：计划 `docs/process/round55-plan.md`（73 行/sha `07dd7d75…`）+ 门下评审 `docs/process/round55-menxia-review.md`（64 行/8,788 B/sha `2baa3886…`，verdict **PASS-WITH-CHANGES**）。
只读：未改 `crates/`、未跑 `cargo`/`ci.ps1`、未 commit、未 spawn 执行 agent（Lead 负责 spawn）。本文为尚书唯一写入。

## 一、派发边界与串行规则

1. **唯一写者**：A=礼部、B=兵部、C=兵部、T=刑部；四单写范围见 §二，**无交集**。尚书/门下不代笔任何子单产出。
2. **并行**：A ∥ B ∥ C 可同时动笔（文件互不重叠）。B、C 均在 `crates/mir-server/` 下但分属不同文件（B=`tests/maps.rs` 单文件；C=`src/npc.rs`+新 `tests/round55_npc_recycle.rs`），无写冲突。
3. **`cargo`/`target` 由 Lead 串行**：执行者不得自行并发跑 `cargo`（含 `--list`），以免 `target` 锁与污染；定向命令按 Lead 排定窗口执行。
4. **T 依赖门禁**：T 必须在 A/B/C 完整结果**且** Lead 唯一全量门禁（12 项）落盘后启动，只读终审。
5. **门禁唯一**：`pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale` 仅 Lead 一条路径；执行者自测不得冒充门禁。
6. **不做**：不加 D（125 真生产者/原生 dash 帧，顺延 R56）；不改 CI 契约；不改 `DB_CANDIDATES`；不新增工具；不宣称"R55 完整/AOI 全量"。

## 二、逐子单契约

### A ｜礼部（sol-relay/gpt-6.1-sol）｜SUB=`…-sub-44533cd5`｜证据 `target/round55/docs/attempt-1/`

- **写范围（唯一）**：`docs/process/round54-plan.md` **仅 :44、:103 两行 + 同行勘误注**；新建 `docs/testing/round55/README.md`；`docs/roadmap.md` **一行**基线脚注（门下 §4 已按"已声明范围"接受，须在 `c/files.json` 单列该行 diff）。
- **交付物**：`:44` 改为"对**所列每个**发现用例的目标**逐个**运行"；`:103` 改为五路由名 `aoi_acceptance_{enter_snapshot,leave_cleanup,change_map_sets,recycle_all_kinds,broadcast_identity_order}` + `B`；勘误注**同行**以 `> 原文（R54）：…` 前缀保留旧文（M3）；`README.md` 为本轮索引（单号/证据路径/状态列/PARTIAL 承接/不做清单），并**同文**写入 M1 口径「AOI-5 整体 PARTIAL、仅子项"无回收原语"CLOSED、生产零调用者接线待 R56（M1 三处同文之一）」。**禁止**：预写 G2/G3 判定（M5）、改写计划/评审原文、把 maps 4 例 skip 写成 PASS。
- **验收命令（原样可复制）**：
  ```bash
  py -3 tools/docs/check_links.py
  git diff --check
  grep -n "对发现用例的目标运行\|A1/A2/A4/A5/A6/B" docs/process/round54-plan.md          # 须 1 行 = 勘误注
  grep -n "对发现用例的目标运行\|A1/A2/A4/A5/A6/B" docs/process/round54-plan.md | grep -v 原文   # 须 0 命中
  ```

### B ｜兵部（deepseek-flash 编码）｜SUB=`…-sub-482b341d`｜证据 `target/round55/maps/attempt-1/{log,meta.json,evidence.json}`

- **写范围（唯一）**：**仅** `crates/mir-server/tests/maps.rs`（顶部自读 env + 4 处调用点）。
- **实现口径（零生产改动，门下 D1/M4 首选）**：测试文件顶部自读 `MIR2_DEV_DB_PATH`（**空串视为未设**，仿 `crates/mir-server/src/config.rs:230-233`）；未设 → 回退 `GameData::discover()`（行为与今天逐字节同）；命中 → `GameData::load(path)`（`crates/mir-server/src/gamedata.rs:223` 已 `pub`）。**不碰** `gamedata.rs`、`discover()`、`DB_CANDIDATES`、`crates/mir-server/src/server.rs:616`。
- **交付物**：4 个 `real_database_*` 例在给定 DB 时执行真实断言；无 env 时保持 `eprintln!`+`return` 早退（`maps.rs:979-981`、`:1040-1042`、`:1174-1176`、`:1235-1237` 四处的跳过路径不动）。
- **验收命令（原样可复制）**：
  ```powershell
  cargo test -p mir-server --test maps -- --list                      # 须 20 例
  $env:MIR2_DEV_DB_PATH='resource/community/suprcode-crystal-database/cjlaaa/Server.MirDB'; cargo test -p mir-server --test maps real_database_ -- --nocapture   # 4 例真跑
  Remove-Item Env:\MIR2_DEV_DB_PATH; cargo test -p mir-server --test maps -- --nocapture                                                     # 同文件不带 env 全量一次
  ```
- **PASS 为条件 PASS（门下 D2）**：基线脚注必须 `20 例 / 16 有效 / 4 需 MIR2_DEV_DB_PATH`；定向证据**同屏**含 env 设置 + 完整命令 + `--list` 20 + 4 例具名 + **0** 行 `跳过：resource/ 里没有 Server.MirDB 样本` + 原始 exit；带 env 后旧断言红 ⇒ 报 **PARTIAL/缺陷，不改阈值迁就**（含 `>=20` 传送点、比奇坐标）。

### C ｜兵部（deepseek-flash 编码）｜SUB=`…-sub-bd27ecd2`｜证据 `target/round55/npc/attempt-1/`

- **写范围（唯一）**：`crates/mir-server/src/npc.rs`（`despawn_npc` + 关系清理）+ 新建 `crates/mir-server/tests/round55_npc_recycle.rs`。
- **实现口径**：逐句镜像 Crystal `github/Crystal/Server/MirEnvir/Map.cs:2375-2383`（`:2378` `NPCs.Remove`、`:2382` 位置索引摘除）；模板 `crates/mir-server/src/world.rs:1961-1980`（广播 `S.ObjectRemove` → `aoi_remove`）、`:4999-5010`（`despawn_pets`）。**doc 必须写"当前无生产调用者"**（D3①）。
- **降级红线**：若预探需 **>1 处** `world.rs` 生产改动或广播路径借用冲突 ⇒ 按计划风险3 降级"仅实证 + PARTIAL"，编码转 R56（D3③）。
- **验收命令（原样可复制）**：
  ```powershell
  cargo test -p mir-server --test round55_npc_recycle -- --list
  cargo test -p mir-server --test round55_npc_recycle -- --nocapture
  cargo test -p mir-server --test aoi -- --nocapture
  cargo test -p mir-server --lib aoi::tests
  ```
- **必跑断言（6 条，手删容器造绿即判 FAIL）**：①spawn 后观察者收 1 次 `ObjectNpc`；②despawn 后 `ObjectRemove` 恰好 1 次且 ID 精确；③`world.npc(id)` 为 None 且 AOI 查询空；④未知 ID → None 且**零包**；⑤二次 despawn → None 且零新增包；⑥非 NPC 对象 ID 不误删。

### T ｜刑部（独立 sol-relay/gpt-6.1-sol，只读终审）｜SUB=`…-sub-1ea152c0`｜证据 `target/round55/audit/`

- **写范围（唯一）**：`docs/testing/round55/final-manifest.json`（**≤1 MiB**，>1 MiB 只登记本地路径/B/sha）+ `docs/testing/round55/round55-final-audit.md` + `target/round55/audit/summary.log`。
- **交付物**：抽核 A/B/C 原件**字节与 sha**、G1–G3 判定（**T 签发**，非作者自判）、门禁 12 项（HEAD/exit/stdout+stderr/`log_sha256` 对齐）、M2 实测更正、**同文**写入 M1 口径「AOI-5 整体 PARTIAL、仅子项"无回收原语"CLOSED、生产零调用者接线待 R56」、不做清单显式写"D（125/dash）顺延 R56"。**禁止**：宣称"AOI 全量/R55 完整"、替 A/B/C 补写产出、自判代签结论扩张。
- **验收命令（原样可复制）**：
  ```bash
  git rev-parse HEAD
  py -3 tools/docs/check_links.py
  pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale        # 仅 Lead 执行，T 只引用其原件
  ```

## 三、必改 M1–M5 逐字纳入（门下 §2 原文口径）

| # | 纳入位置 | 原文要求 |
|---|---|---|
| **M1（阻塞）** | **三处同文**：①本派发件 §二 C/T；②A 的 `docs/testing/round55/README.md`；③T 的 `round55-final-audit.md` | **AOI-5 整体 PARTIAL、仅子项"无回收原语"CLOSED、生产零调用者接线待 R56**。禁止任何处写"**AOI-5 已关闭**"/"**R55 完整**"；任一处出现即判**假绿**。C 的 `despawn_npc` 唯一调用者是本轮新测试，不得据此整体关闭 AOI-5。 |
| M2 | T 审计件 | 计划 §一 事实10"全量 10 命中"**实测为 26 命中**（漏 `MonsterObject.cs:942,1207`、`PlayerObject.cs:353,402,1418,4506,4553,7520`、`HumanObject.cs:4198`、`PlayerObject.cs:3353` 类）；结论（0 处 NPC 调用者）成立。以"**实测更正**"记入，**不改** `round55-plan.md`（无授权写者）。 |
| M3 | A 的证据 + A 的勘误注写法 | 勘误注必须写成**同一行** `> 原文（R54）：…` 前缀；A 证据须**同时**贴 `grep -n "…" round54-plan.md`（命中 1 行=勘误注）与加 `\| grep -v 原文` 后 **0 命中** 两条原始输出。 |
| M4 | B 实现 + 本派发 §二 B | 取"测试侧 env + `pub fn load`（`gamedata.rs:223`）"为**默认**；若确用 `discover_dev()` 须补齐 doc、"非 Postgres URL"说明、`grep -rn "discover_dev" crates/` 证据。**本轮显式选定零生产改动方案，不引入 `discover_dev()`**。 |
| M5 | A 的 README | 只登记证据路径/状态列，**不得预写 G2/G3 判定**（作者不自判）；G2/G3 由 T 签发、尚书验收、门下确认。 |

## 四、门禁与提交（Lead 唯一执行）

1. **门禁命令**：`pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`，12 项，**仅 Lead 一条路径**；执行者自测不计门禁。
2. **原件**：`target/round55/gate/attempt-N/`，含完整 stdout/stderr、原进程 `exit_code`、起止时区、命令/cwd、`HEAD`、候选文件 sha、`log_bytes`/`log_sha256`；TMP/TEMP → `target/tmp`。
3. **两次提交**：提交一（代码批）= `tests/maps.rs` + `npc.rs` + 新测试；提交二（文档批）= A 文档、T 审计/清单、尚书/门下件，**逐文件显式纳入**。若提交二触及代码/测试/CI/配置，原门禁作废须对新树重跑。
4. **sha 绑定 HEAD + 红线**：日志 sha/exit/HEAD 任一缺失或与受测树不符即**无效**，每次提交各自记录受测 HEAD 与文件 sha；0 tests / 提前 return / 跳过 一律**不计 PASS**，红项不得写全绿，`git diff --check` + `check_links.py` 必过。

## 五、黑名单 / 风险

- **黑名单（越界即驳回）**：改 `crates/` 之外的 B/C 写范围；A 越权改计划/评审原文；B 碰 `gamedata.rs`/`discover()`/`DB_CANDIDATES`/`server.rs:616`；C 发明 NPC 脚本/DB 编辑 API 或"NPC 定时消失"；T 兼职作者；任何处宣称"R55 完整/AOI 全量"；把 4 例 skip 写成 PASS；手删容器造绿；为"完整"放宽阈值。
- **风险1**：B 若被生产误用会改装载行为 → 靠 `grep -n "discover()" crates/mir-server/src/server.rs` 仍为 `:616` 一处钉死。
- **风险2**：带 env 的 4 例可能真实暴露旧断言错误 → 如实报 PARTIAL/缺陷，不迁就。
- **风险3+4**：C 广播路径 `&mut self` 重入可能牵动 `world.rs` → 超 1 处即降级"仅实证 + PARTIAL"；文档批若误触代码则门禁作废 → 提交二严格限定 `.md`/`.json` 文档面。
