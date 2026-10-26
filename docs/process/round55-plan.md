# R55 第55轮方案（中书省 / gpt-6.1-sol）

主单 `JJC-20261025-005`（第55轮：docs 勘误 + maps 真实数据入口 + NPC 回收入口）｜HEAD=origin/main=`67c618c`｜规划 2026-10-26T11:4x+08:00
只读轮：本文件是本轮唯一写入；未改 `crates/`、未跑 `cargo`/`ci.ps1`、未 commit。R54 主单 `JJC-20261026-002` 已 Done（PASS-WITH-PARTIAL，PARTIAL 6 类/13 项）。

## 一、事实（Crystal/Rust 文件:行号）

| # | 事实 | 依据 |
|---|---|---|
| 1 | 候选库表是空常量，`discover()` 的候选路径恒为空 | `crates/mir-server/src/gamedata.rs:59` `pub const DB_CANDIDATES: &[&str] = &[]`；`candidate_paths()` `:952-963` 由它展开 2×0 条 |
| 2 | `discover()` 只遍历候选，空表直接落 Err；Err 文本带绝对路径与 searched 列表 | `crates/mir-server/src/gamedata.rs:395-408`（循环）、`:409-421`（Err 构造） |
| 3 | maps 4 个真实数据例全部 `discover()` 失败即 `eprintln!`+`return`，不 panic 也不计 PASS | `crates/mir-server/tests/maps.rs:979-981`、`:1040-1042`、`:1174-1176`、`:1235-1237`；口径声明 `:8` "缺失时跳过" |
| 4 | 本机样本确实存在，**资源不是阻塞项**，阻塞项是发现链 | `resource/community/suprcode-crystal-database/cjlaaa/Server.MirDB`，452,106 B，mtime 2025-07-28（本规划者 `ls` 实测） |
| 5 | `discover()` 只有 1 个生产调用点，其余全是测试/示例；生产边界可被 grep 钉死 | 生产 `crates/mir-server/src/server.rs:616`；库内 `crates/mir-server/src/quests.rs:2233,2279`；示例 4 个（`examples/{ai_audit,droprates,movements,spawns}.rs`）；测试见事实 3 与 `tests/{guild,magic,common}.rs` |
| 6 | 生产已接受 env 覆盖 DB 路径的先例，但走的是 `ServerConfig`，不是 `GameData` | `crates/mir-server/src/config.rs:230-234` `MIR2_DB_PATH`（空串视为未设）；`config.rs:226` `MAP_ROOT_ENV` 同法 |
| 7 | Rust NPC 只有查询与生成，没有回收；源码注释把"只随地图销毁"当成现状 | `crates/mir-server/src/npc.rs:658-670`（`npcs/npc/npcs_on`）、`:687` `spawn_npcs`、`:762-773` `spawn_npc`（push + `aoi_insert(..., AoiKind::Npc, ...)`）；`:770-771` 注释"NPC 是静态对象…只随地图销毁" |
| 8 | Crystal NPC 生成入口：构造即挂进 Envir 容器 | `github/Crystal/Server/MirObjects/NPCObject.cs:47-55`（`Envir.NPCs.Add(this)`） |
| 9 | Crystal 有**容器级**移除原语，Merchant 分支就是 NPC 回收 | `github/Crystal/Server/MirEnvir/Map.cs:2375-2383` `RemoveObject`（`if (ob.Race == ObjectType.Merchant) NPCs.Remove((NPCObject)ob);` + `GetCell(...).Remove(ob)`）；对照 `:2364-2373` `AddObject` |
| 10 | 但 `RemoveObject` 的调用者全是玩家/英雄/物品/法术/宠物/攻城，**没有一处是 NPC**；运行时 NPC 回收在 Crystal 里不存在 | `HeroObject.cs:225,677`、`HumanObject.cs:1697`、`ItemObject.cs:133`、`MapObject.cs:784`、`IntelligentCreatureObject.cs:120,633,642,846`、`ConquestObject.cs:707`（`grep -rn "RemoveObject(" github/Crystal/Server` 全量 10 命中） |
| 11 | DB 级 `Envir.Remove(NPCInfo)` 存在但服务端零调用者，只能算编辑器/工具面 | `github/Crystal/Server/MirEnvir/Envir.cs:4367-4371`；同文件 `CreateNPCInfo` `:4329-4332`、`NPCInfoList` 存盘 `:2480-2482`；全量 grep 仅该定义行命中 |
| 12 | Rust 侧可照抄的"移除 + 广播 + AOI 摘除"模板已存在，C 不需要新造工具 | `crates/mir-server/src/world.rs:1959-1979`（玩家离场：广播 `S.ObjectRemove` → `aoi_remove`）；`:4999-5008` `despawn_pets`（同样顺序）；AOI 原语 `crates/mir-server/src/aoi.rs:190` `insert` / `:200` `remove` |
| 13 | R54 两处勘误原文与依据 | `docs/process/round54-plan.md:44`（"对发现用例的目标运行"）、`:103`（"A1/A2/A4/A5/A6/B"）；判定见 `docs/process/round54-menxia-confirm.md:16-23` Δ1/Δ2，承接见 `:27` P1/P2 |
| 14 | 门禁与提交口径属既有契约，本轮不新造 | `docs/process/team-and-roles.md:119-157`（原始 exit/时间/sha、禁假绿、soak 锁）；R54 实测 12 项、`log_bytes=194319`、`log_sha256=7c18f3cf…ab43ff`（`round54-menxia-confirm.md:12`） |

## 二、目标（可证伪）

- **G1（文档勘误闭合 Δ1/Δ2）**：`docs/process/round54-plan.md:44` 改为"对**所列每个**发现用例的目标**逐个**运行"；`:103` 改为五路由名 `aoi_acceptance_{enter_snapshot,leave_cleanup,change_map_sets,recycle_all_kinds,broadcast_identity_order}` + `B`；两行均保留可追溯的勘误注（原文可见），文件仍在原位置。
  - 证伪：`grep -n "对发现用例的目标运行\|A1/A2/A4/A5/A6/B" docs/process/round54-plan.md` 必须 0 命中（勘误注里以代码块/引用形式出现需显式加"原文"前缀，且 grep 断言用 `-v 原文`）；`git diff --check` exit0。
- **G2（maps 4/20 真跑）**：为测试/开发提供**受控**数据入口，使 4 例在给定 DB 时执行真实断言、在无 DB/无 env 时与今天**逐字节同行为**（Err→跳过）。
  - 证伪：a) `grep -n "discover()" crates/mir-server/src/server.rs` 仍为 `:616` 一处且调用名不变（生产未改）；b) 带 env 的定向命令输出必须含 4 个 `real_database_*` 用例的实际打印且 **0** 行 `跳过：resource/ 里没有 Server.MirDB 样本`；c) 不带 env 全量 20 例仍 exit0 且 4 例 skip 计数可复现（基线脚注，不当全绿）；d) 若带 env 后任一断言失败 ⇒ 报 PARTIAL/缺陷，不改断言迁就。
- **G3（NPC 回收生产入口 + AOI-5 关闭）**：新增最小 `World::despawn_npc(object_id)`，语义= Crystal `Map.RemoveObject` 的 NPC 分支（容器移除 + 位置索引移除 + 周围玩家恰好一次 `S.ObjectRemove` + AOI 摘除）。
  - 证伪：新集成例断言 ①spawn 后观察者收到 1 次 `ObjectNpc`；②despawn 后观察者 `ObjectRemove` 恰好 1 次、ID 精确；③`world.npc(id)` 变 None 且 AOI 查询为空；④未知 ID 返回 None 且**零包**；⑤第二次 despawn 仍 None 且零新增包；⑥非 NPC 对象 ID 不误删。任一不成立即 FAIL，不得手删容器造绿。
- **G4（索引/时态一致）**：新建 `docs/testing/round55/README.md` 索引（本轮单号、A/B/C 证据路径、G1–G3 判定、PARTIAL 承接）；R54 文档中"4 早退/16 有效"不被写成全绿，maps 基线脚注显式写 `20 例 / 16 有效 / 4 需 MIR2_DEV_DB_PATH`。
  - 证伪：`py -3 tools/docs/check_links.py` exit0；grep 断言 `16/20` 附近必须出现 `PARTIAL` 或 `早退` 字样。
- **G5（门禁绑定）**：Lead 唯一全量门禁 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale` 12 项 exit0，原始 stdout/stderr 字节 + `exit_code` + `HEAD` + `log_sha256` 落 `target/round55/gate/attempt-N/`；两次提交（代码批、文档/审计批）各自记录受测 HEAD 与文件 sha。
  - 证伪：日志缺失 sha/exit 或与 HEAD 不符即无效；任一门禁项红不得写全绿。

## 三、子任务 DAG（4 个执行单，不超 5）

| key | owner | depends_on | 写范围（唯一写者） | 验收命令 | 证据 |
|---|---|---|---|---|---|
| A | 礼部，sol-relay/gpt-6.1-sol | 门下 approve | `docs/process/round54-plan.md`（仅 :44/:103 两行+勘误注）、新建 `docs/testing/round55/README.md`、`docs/roadmap.md` 一行基线脚注 | `py -3 tools/docs/check_links.py`；`git diff --check`；G1 两条 grep | `target/round55/docs/attempt-1/` |
| B | 兵部，deepseek-flash 编码 | 门下 approve、A 不阻塞 | `crates/mir-server/src/gamedata.rs`（新增 dev 入口，`discover()` 本体不动）、`crates/mir-server/tests/maps.rs`（4 处调用点换入口） | `cargo test -p mir-server --test maps -- --list`（须发现 20 例）；`$env:MIR2_DEV_DB_PATH='resource/community/suprcode-crystal-database/cjlaaa/Server.MirDB'; cargo test -p mir-server --test maps real_database_ -- --nocapture`（4 例真跑）；同文件不带 env 全量一次 | `target/round55/maps/attempt-1/{log,meta.json,evidence.json}` |
| C | 兵部，deepseek-flash 编码 | 门下 approve、可与 B 并行（文件不重叠） | `crates/mir-server/src/npc.rs`（`despawn_npc` + 关系清理）、新建 `crates/mir-server/tests/round55_npc_recycle.rs`；若需广播上下文才动 `world.rs` 既有 `cfg(test)` 外的**一处**调用点，须在证据里单列 diff | 先 `--list` 发现具名例，再 `cargo test -p mir-server --test round55_npc_recycle -- --nocapture`；回归 `cargo test -p mir-server --test aoi -- --nocapture`、`--lib aoi::tests` | `target/round55/npc/attempt-1/` |
| T | 刑部，独立 sol-relay/gpt-6.1-sol，只读评审 | A/B/C 完整结果 + Lead 门禁 | `docs/testing/round55/round55-final-audit.md`、`final-manifest.json` | 抽核 A/B/C 原件字节与 sha、G1–G3 判定、门禁 12 项 | `target/round55/audit/` |

- DAG：独立门下 → 尚书 delegate 登记（取得 SUB_TASK_ID）→ B ∥ C → A（可先动笔，收口待 B/C 证据）→ Lead 全量门禁（唯一，串行）→ T → 尚书验收 → 门下 confirm → Lead 提交二。
- B/C 写文件不重叠可并行；`cargo`/`target` 锁由 Lead 排串行；A 只碰文档，T 不得兼职作者。
- B 的实现口径（最小、可审计）：`GameData::discover()` 保持现状；新增 `pub fn discover_dev() -> Result<Self, MirDbError>`，顺序 = `MIR2_DEV_DB_PATH`（非空）→ 现有 `discover()`；4 个测试改用 `discover_dev()`。不把 `MIR2_DB_PATH` 并入 `GameData`（避免改生产 `ServerConfig` 语义），不写死 cjlaaa 路径，不改 `DB_CANDIDATES`。
- C 的实现口径：只镜像 `Map.cs:2375-2383` 的 NPC 分支；不加 NPC 脚本/DB 编辑 API，不碰 `Envir.Remove(NPCInfo)` 语义，不做"NPC 定时消失"这类无 Crystal 锚点的发明。

## 四、门禁与提交口径（只由 Lead 执行）

1. 提交一：`gamedata.rs`+`maps.rs`+`npc.rs`+新测试；先独立 gpt 评审，再由 Lead 跑唯一全量门禁（12 项）；红则交原执行者返工，新 attempt 从头保全字节，旧 attempt 不覆盖。
2. 提交二：A 文档、T 审计/清单、尚书/门下件；逐文件显式纳入；若触及代码/测试/CI/配置，原门禁作废须对新树重跑。
3. 原件：`target/round55/gate/attempt-N/`，含完整 stdout/stderr、原进程 `exit_code`、起止时区、命令/cwd、`HEAD`、候选文件 sha、`log_bytes`/`log_sha256`；TMP/TEMP 指向 `target/tmp`。
4. 每条定向命令同样留 `--list` 发现数、实际例名、有效用例数、exit 码；0 tests/提前 return/跳过 一律不计 PASS。
5. >1MiB 只登记本地路径/B/sha 不入库；`target`、`.env`、两历史 JSON 不纳入；`git diff --check` + 链接校验必过。

## 五、风险 / 不做清单

- 风险1：`discover_dev()` 若被生产误用会改变服务器装载行为 → 靠事实 5 的 grep 断言 + 评审盯 `server.rs:616` 未变；不在 `main.rs`/`server.rs` 引入 env 分支。
- 风险2：带 env 的 4 例可能真实暴露旧断言错误（如 `>=3` 传送点、比奇坐标） → 如实报 PARTIAL/缺陷，不迁就改阈值。
- 风险3：C 的广播路径若需要 `&mut self` 重入（观察者集合与 `self.npcs` 借用冲突）可能牵动 `world.rs`；若预探发现需 >1 处生产改动，C 降级为"实证 + PARTIAL 结论"，编码转 R56。
- **不做**：D 研究（125 真生产者/真 socket、原生 dash 帧）本轮裁掉——A/B/C 已含 2 个生产文件改动、2 个测试面与一次全量门禁，预算不足；G4/G5 大项、EnemyGuild/行会战、PK 完整矩阵、AOI 网格重写、Crystal 公式/RNG 修改继续顺延。
- 不做：改 `DB_CANDIDATES` 常量、下载/改 DB 素材、手造 NPC 生命周期、改 CI 契约、加"完整验收命令"之外的新工具、把 4 例 skip 写成 PASS、宣称 AOI 全量或 R55 完整。

## 六、待裁决项（4）

1. **B 的受控入口用新 env `MIR2_DEV_DB_PATH` 是否可接受**（不改 `GameData::discover()`、不并用 `MIR2_DB_PATH`）？代价：全量门禁里 4 例仍 skip，真跑只发生在定向命令。
2. **maps 的 PASS 口径**：接受"定向命令 + env 实跑 4 例"作为 P2 关闭证据，还是要求 CI 也装载真实资源（后者需改 CI 契约，超本轮范围）？建议前者。
3. **C 是否构成"发明接口"**：Crystal 只有容器级 `Map.RemoveObject`、运行时无 NPC 调用者；本轮把它落成 `despawn_npc` 生产入口并据此关闭 AOI-5，是否批准？若判属扩范围，C 只交实证、AOI-5 维持 PARTIAL。
4. **裁掉 D 是否接受**（本轮只 A/B/C，125/dash 顺延 R56 起）。
