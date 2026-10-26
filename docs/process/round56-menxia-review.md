# R56 门下省评审（menxia / gpt-6.1-sol）

主单 `JJC-20261026-003`｜受审 HEAD=`e3b365f`（实测 `git rev-parse --short HEAD`）｜受审计划 `docs/process/round56-plan.md` 109 行 sha256=`a4473c9f…d769d`（实测一致）｜评审 2026-10-26T12:3x+08:00（Asia/Shanghai）
**裁定：修改后批准**。G1 修法是真去根因、非掩症状；G2 不接线裁决经独立复核成立，不驳回。但验收命令形态与断言强度有 4 条高优先必改（M1–M4），未落实前不得判 PASS。

## 一、结论

1. G1 方向成立。flaky 根因是**测试**在两次 spawn 之间读真实流逝毫秒（`items.rs:1536` `let now = self.now()`），而 `advance_clock` 的设计目的就是冻结时钟（`world.rs:1387-1397`）。`advance_clock(0)` 之后两次 spawn 共享同一 `now()`，消除的是非确定性而非断言强度：严格 `>` 语义（`items.rs:1629`）与边界三时刻断言（`aoi.rs:1172-1219`）全部保留。
2. 缺口：冻结目前只由调用顺序隐式成立，缺"两物品 `expire_time` 差 0"的显式断言。没有它，后续删掉 `advance_clock(0)` 仍可能 100/100 绿，P4 会以"绿"的形式复发（M1）。
3. G2 独立复核成立且未发现被遗漏的真实生产路径：`npcs` 的唯一变异点在 `despawn_npc` 内部（`npc.rs:829` 的 `retain`），`spawn_npc` 仅 2 个测试文件调用，Crystal 26 处 `RemoveObject(` 中 0 处传 `NPCObject`。故 D1 批准。
4. 与方案无关的既有缺陷：`world.rs:1378` 注释中 `advance_clock` 首字母是 0x07(BEL) 控制字节（`od` 实测 `\a d v a n c e`）。本轮禁改 `src/`，登记 R57（M7）。
5. 不接受 D3 的"两条版"条件 PASS，不接受 T 在门禁前抽核（见二）。

## 二、裁决 D1–D5

| 项 | 裁决 | 理由 |
|---|---|---|
| D1 AOI-5 不接线 | **批准** | 五条证据各自独立且互不替代：Crystal 运行时 0 个 NPC 回收调用者（26 处 `RemoveObject(` 全传玩家/怪物/技能/pet）；`Envir.Remove(NPCInfo)` 只改数据库表；本作无地图卸载，`initialize_map` 幂等早退；`evict_maps` 的 busy 集显式含 `npcs`，有 NPC 的图不会被淘汰，故不会留悬空 NPC；`spawn_npc` 动态路径仅测试。强行接线=发明 NPC 生命周期语义，维持 P2/P3 各 1 条 PARTIAL 正确。 |
| D2 只动测试冻结时钟 | **批准（附 M1、M2）** | 生产 `>` 语义与 Crystal 同构，`src/` 无需改；把测试的非确定性剔除是正确层。但必须补显式等价断言与确定性 1ms 新例，否则修复不可证伪。 |
| D3 PASS 三合一 | **批准，且不接受"只满足两条"** | 旧树 5 连跑全绿已证明"偶然会绿"，100 次绿不能单独作为因果证据；旧树注入 sleep 必红才证明机制，新树同 sleep 必绿证明修复不是绕过。三条缺一即 FAIL，不设条件 PASS。 |
| D4 文档写范围 | **修改后批准** | 写范围限 `docs/world/aoi.md` 新增节 + 新建 `docs/testing/round56/README.md` 可接受，不动 roadmap 可接受；但须在 README 记一句"roadmap 基线脚注差异留待 R57 统一"，避免静默丢失。红线见 M4。 |
| D5 T 的时点 | **批准（门禁后启动）** | T 的判定对象是"受测 HEAD + gate attempt-1 原始 exit"；门禁前抽核等于对未受测的树出结论，证据链断裂。只读、不兼职作者。 |

## 三、必改 M1..M7

| # | 级别 | 归属 | 必改内容 | 验收口径 |
|---|---|---|---|---|
| M1 | 高 | A | 冻结后可证伪断言：`world.enter`/`drain` 之后、两次 spawn 之前调 `advance_clock(0)`；两次 spawn 之后断言 `normal.expire_time == coin.expire_time`（差 0），使"冻结失效"立刻红。 | 断言存在且可失败（临时注释掉 `advance_clock(0)` 时必须红，自证一次即可，不必入库）；`--list` 为 14。 |
| M2 | 高 | A | 新例 `ground_item_expiry_strict_ms_boundary`：用两次 spawn 之间 `advance_clock(1)` 构造**确定** 1ms 差，断言 `expire_b - expire_a == 1`；三时刻断言（a 边界两者都在、a+1 仅删 a、b+1 删 b），边界处无 `ObjectRemove`。只断言 `expire_time`/可见性，不新增生产 API、不加 `#[allow]`、断言全英文。 | 单例运行 `1 passed`、exit 0、`--nocapture` 可见断言名。 |
| M3 | 高 | A + Lead | 校正验收命令形态：`cargo test … <A> <B>` 把两个名字作位置参数交给 cargo（cargo 只接受一个 TESTNAME），要么报错、要么只跑一个——"2 passed"都不能作为证据。改为先 `--list` 确认 14，再**分别两次单例运行**（各 `1 passed`/exit 0），或整文件运行取 `14 passed`；两种都留原始 exit 与日志 sha。 | `target/round56/aoi/attempt-1/` 内两条单例日志各含 `1 passed` + 原始 exit，命令文本不含双位置过滤器。 |
| M4 | 高 | B | 文档红线：aoi.md 新增节逐条列四条证据 + Crystal 锚点 + "despawn_npc 无生产调用者"；**不改** 123-128 既有行；README ≤60 行、状态列只写"已落盘/待 T 抽核"，全文不得出现 PASS/已关闭/已接线字样。 | `py -3 tools/docs/check_links.py` → `[OK]` exit 0；`grep -n "PASS\|已关闭\|已接线" docs/world/aoi.md docs/testing/round56/README.md` 命中 0；`git diff --check` exit 0；两处命令日志落盘。 |
| M5 | 中 | T | 启动条件与抽核口径：必须在 gate attempt-1 全量 12/12 之后；抽核 A/B 文件 sha256 与 `target/round56/gate/attempt-1/meta.json` 的 HEAD/文件 sha 一致；G1 只认三件套**原件**（100 次逐次 exit、旧树红在 `coin pile must be removed`、新树绿），不认 A 自述；G2 复核 grep 原始输出。只读。 | final-audit 每项带原件路径 + sha；无原件不得写"通过"。 |
| M6 | 中 | Lead | 证据绑定与提交边界：每条命令记 `--list` 发现数/实际例名/有效用例数/原始 exit；100 次循环留 100 行 `1 passed` 与逐次 exit；旧 attempt 不覆盖；提交一 `git diff --stat` 仅 `tests/aoi.rs`，提交二含计划/aoi.md/README/审计件；>1MiB 只登记路径/bytes/sha。 | 提交前 `git diff --stat` 与计划 §四 逐项一致；首红 attempt 字节保全。 |
| M7 | 低 | Lead | 登记既有缺陷 `world.rs:1378` 注释含 0x07 控制字节（`advance_clock` 被写坏），本轮不改 `src/`，落 R57 待修。 | README/审计件含该条一行；R56 内不得顺手改 `src/`。 |

## 四、抽查锚点表（实读，非引述）

| # | 锚点 | 实测 | 结论 |
|---|---|---|---|
| 1 | `crates/mir-server/tests/aoi.rs:1104,1114,1124` | fn 起点与两次 `spawn_ground_item`；HEAD 中此前无冻结调用 | 与计划一致，flaky 机制真实存在 |
| 2 | `crates/mir-server/src/items.rs:1536,1544` | `let now = self.now();` / `expire_time: now + expire_ms` | 两次调用跨 ≥1 实毫秒即差 1，一致 |
| 3 | `crates/mir-server/src/items.rs:1629` | `if now > ground.expire_time` | 严格 `>`，等于边界不回收，一致 |
| 4 | `crates/mir-server/tests/aoi.rs:1173-1179,1195,1201-1204` | `boundary_delta` 快进、`+1`、coin pile 断言 | 红点定位与计划一致 |
| 5 | `crates/mir-server/src/world.rs:1376-1383,1389-1397` | 冻结分支与 `advance_clock` 注释 | 冻结语义成立；**`:1378` 含 0x07 控制字节**（M7） |
| 6 | `crates/mir-server/src/npc.rs:804-811,829` | 「当前无生产调用者」doc；`npcs.retain` 仅在本函数内 | 生产无 NPC 移除路径，一致 |
| 7 | `crates/mir-server/src/maps.rs:410-412,434` | `initialized_maps` 早退；`spawn_npcs` 只在初始化 | 幂等成立，一致 |
| 8 | `crates/mir-server/src/maps.rs:871-900` | busy 集 `:881` 含 `npcs`，`break` 于全忙 | 有 NPC 的图不淘汰，一致 |
| 9 | `crates/mir-server/src/server.rs:531` | 启动只 `initialize_map(&config.start_map)` | 一致 |
| 10 | Crystal `Server/MirEnvir/Map.cs:2375-2383` | `:2378` `NPCs.Remove((NPCObject)ob)`；全库 26 处 `RemoveObject(`，逐行核对无一处传 `NPCObject` | 0 调用者成立 |
| 11 | Crystal `Server/MirEnvir/Envir.cs:4367-4371` | 仅 `NPCInfoList.Remove` 定义，无服务端调用者 | 一致 |
| 12 | `docs/world/aoi.md:123-128` | `:124`「NPC 刷出」；无回收行；`:128` evict 不操作 AOI | 文档缺口真实 |
| 13 | `crates/mir-server/tests/aoi.rs` 计数 / HEAD | `grep -c '#[test]'` = **13**；HEAD=`e3b365f` | 13+1=14 成立 |

## 五、风险

- R1 冻结位置漂移：后人把 `advance_clock(0)` 移到 spawn 之后（或删掉）时，只有 M1 断言能拦；否则 P4 静默复发。
- R2 100 次循环若把 exit 吞在管道里（`| Select-String` 取末段），会得到"全绿"假象；必须逐次取 `$LASTEXITCODE`。
- R3 M3 的命令形态风险：`cargo test` 双位置过滤器可能直接报错，若照抄会把"命令错误"记成红或只跑一例记成绿。
- R4 T 只看 A/B 自述存在假绿空间；M5 要求原件与 sha 绑定。
- R5 `world.rs:1378` BEL 控制字节是既有污染，本轮不动 `src/`，仅登记。
- R6 "不预写 PASS"目前无自动门禁，靠 M4 的 grep 日志；B 若把"待 T 抽核"写成"已通过"即判假绿。
- R7 提交二若把 >1MiB 日志入库会污染仓库；计划 §四.5 已列，保持。

## 六、CLI 回执（原样，按序执行，全部 exit 0）

1. `memo JJC-20261026-003 menxia "R56门下沉：修改后批准（M1-M4高优先），评审 docs/process/round56-menxia-review.md，D1-D5已裁决" --as menxia` → `📝 JJC-20261026-003 任务记忆：menxia` / EXIT=0
2. `state JJC-20261026-003 Assigned --as menxia` → `✅ JJC-20261026-003 状态更新：Menxia → Assigned` / EXIT=0
3. `delegate JJC-20261026-003 menxia shangshu "R56 派发" "按评审M1..Mn派发A/B/T" --as menxia` → `📋 委派 JJC-20261026-003-sub-ff0cb009：门下省 → 尚书省（depth=1）` / **SUB_TASK_ID=`JJC-20261026-003-sub-ff0cb009`** / EXIT=0
4. `validate --as menxia` → `✅ 看板校验通过：146 条任务符合 schema v3.1` / EXIT=0
