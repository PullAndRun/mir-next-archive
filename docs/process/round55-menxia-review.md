# R55 门下省评审（gpt-6.1-sol）

主单 `JJC-20261025-005`｜HEAD=origin/main=`67c618cbb589f353e0fba5b2220dae45712fb4da`（本王 `git rev-parse --verify` 双查，均同值）｜评审 2026-10-26T11:47+08:00 起（Asia/Shanghai）
只读：未改 `crates/`、未跑 `cargo`/`ci.ps1`、未 commit；唯一写入为本文件。计划 `round55-plan.md` 73 行/12,597 B/sha `07dd7d75…` 与输入一致。

## 0. Verdict

**PASS-WITH-CHANGES**：范围（A/B/C/T 四单）、DAG、写范围唯一性、门禁绑定（HEAD+原始日志 sha）成立，可下发；但有 **1 条阻塞级口径（M1：AOI-5 不得整体关闭）** 与 4 条非阻塞必改。按 §二 可证伪性看，G1/G2/G4/G5 达标，**G3 标题中"AOI-5 关闭"须改口径**。本文件不宣称任何一单已完成。

## 1. 裁决 D1–D4

**D1（B 新增 `MIR2_DEV_DB_PATH` + `discover_dev()`、不并 `MIR2_DB_PATH`）——可接受，但要求降级为实现首选。**
理由：`discover()` 保持字节不变、不写死 `cjlaaa`、不碰 `DB_CANDIDATES`，方向正确；不并 `MIR2_DB_PATH` 亦正确（`ServerConfig` 语义不被测试污染）。
但实测 `GameData::load` 已是 `pub fn`（`gamedata.rs:223`），故存在**零生产改动**的更小方案：在 `tests/maps.rs` 顶部自读 env（空串视为未设，同 `config.rs:230-233` 先例），未设时回落 `discover()`，命中时 `GameData::load(path)`。B 的验收命令不变（仍是 `$env:MIR2_DEV_DB_PATH=…; cargo test --test maps real_database_`）。
若 B 仍选 `discover_dev()`：①doc 注释必须写"开发/测试专用，生产 `server.rs:616` 不调用"；②`MIR2_DEV_DB_PATH` 是 **Server.MirDB 文件路径**，须与 `.env` 的 `MIR2_DEV_DATABASE_URL`（Postgres URL）显式区分；③证据含 `grep -rn "discover_dev" crates/`（仅定义+测试 4 处）。

**D2（maps PASS 口径 = 定向命令 + env 实跑 4 例）——接受，但为"条件 PASS"，不得表述为 20/20 全绿。**
条件：①全量门禁里 4 例仍 skip，基线脚注必须写 `20 例 / 16 有效 / 4 需 MIR2_DEV_DB_PATH`；②定向证据须同屏含 env 设置、完整命令、`--list` 发现数 20、实际执行 4 例具名、`0` 行 `跳过：resource/ 里没有 Server.MirDB 样本`、原始 exit；③带 env 后任一断言失败 ⇒ 报 PARTIAL/缺陷，**不改阈值迁就**（含 `>=20` 传送点、比奇坐标等旧断言）；④不得要求 CI 装载真实资源（改 CI 契约超本轮）。

**D3（C 的 `despawn_npc` 是否算"发明接口"、能否据此关闭 AOI-5）——有条件批准实现，但不批准整体关闭 AOI-5。**
定性：`despawn_npc` 逐句镜像 `Map.cs:2375-2383` 的 Merchant→`NPCs.Remove` 分支（`:2378`）+ `:2382` 位置索引摘除，并以 `world.rs:1961-1980`（`leave` 广播→`aoi_remove`）与 `:4999-5010`（`despawn_pets`）为模板，**不属新造语义**，可以落。
但不批准"AOI-5 关闭"：Crystal 运行时对 NPC 零调用者（本王实测 `RemoveObject(` 全仓 26 处，`grep -ci npc` = **0**；`Envir.Remove(NPCInfo)` 仅定义行 `Envir.cs:4367`、零调用），R55 的 `despawn_npc` 唯一调用者将是本轮新测试本身——这是"测试自证函数存在"，把 PARTIAL 改 PASS 等于把"生产无回收路径"换成"生产有一个没人调用的回收函数"，语义缺口未变。
口径：AOI-5 **整体维持 PARTIAL**，仅其子项"无回收原语"可标 CLOSED，并注明"生产零调用者，接线待 R56（NPC 脚本/DB 编辑或地图卸载路径）"。
条件：①`despawn_npc` 的 doc 必须写"当前无生产调用者"；②仍禁手删容器造绿；③若预探发现需 >1 处 `world.rs` 生产改动或广播路径借用冲突，按计划风险3降级为"仅实证 + PARTIAL"，编码转 R56；④未知 ID 零包、二次 despawn 零新增、非 NPC ID 不误删三条断言必须真跑。

**D4（裁掉 D：125 真生产者/原生 dash 帧）——接受。**
A/B/C 已含 2 处生产文件改动 + 2 个测试面 + 一次全量门禁，预算不足以再开 D。条件：R55 README 与 T 审计件的"不做清单"须显式写"D（125/dash）顺延 R56"，防静默丢失；不得在结论里写"R55 完整"。

## 2. 必改列表（5 条：1 阻塞 / 4 非阻塞）

| # | 必改 | 阻塞 | owner |
|---|---|---|---|
| M1 | 口径统一：G3 的"AOI-5 关闭"改为"AOI-5 子项 CLOSED / 整体 PARTIAL"；尚书派发指令、A 的 `docs/testing/round55/README.md`、T 的 audit 三处同文。任一处写"AOI-5 已关闭"即判假绿。 | **是** | 尚书派发 + A/T |
| M2 | 计划 §一 事实10"全量 10 命中"实测为 **26 命中**（漏 `MonsterObject.cs:942,1207`、`PlayerObject.cs:353,402,1418,4506,4553,7520`、`HumanObject.cs:4198`、`PlayerObject.cs:3353` 类）；结论（0 处 NPC 调用者）成立。须在 T 审计件以"实测更正"更正；不改 `round55-plan.md`（无授权写者）。 | 否 | T |
| M3 | G1 的 grep 断言与"原文可见"自相矛盾：勘误注必须写成同一行 `> 原文（R54）：…` 前缀，A 的证据须同时贴 `grep -n "…" round54-plan.md`（命中 1 行=勘误注）与加 `\| grep -v 原文` 后 **0 命中** 两条原始输出。 | 否 | A |
| M4 | D1 实现取"测试侧 env + `pub fn load`(`gamedata.rs:223`)"为默认；若确用 `discover_dev()`，补齐 doc、"非 Postgres URL"说明与 `discover_dev` 调用点 grep 证据。 | 否 | B |
| M5 | A 的 README 只登记证据路径/状态列，**不得预写 G2/G3 判定**（作者不自判）；G2/G3 判定由 T 签发、尚书验收、门下确认。 | 否 | A |

## 3. 抽查锚点表（14 行，全部实读）

| 计划锚点 | 实测结论 |
|---|---|
| `gamedata.rs:59` | ✅ `pub const DB_CANDIDATES: &[&str] = &[]`；`:45-58` 注释解释为何空（默认集 `cjlaaa`） |
| `gamedata.rs:395-421` discover | ✅ fn 在 `:395`，循环 `:397-408`，Err 构造 `:409-431`（绝对路径 `:412-415`、`searched` 列表 `:419-429`）。计划端点 `:421` 略窄，非阻断 |
| `gamedata.rs:952-963` candidate_paths | ✅ `#[must_use]` `:952`、fn `:953`、2×0 展开 `:955-961`，空表即空 Vec |
| `server.rs:616` | ✅ 唯一生产调用 `crate::gamedata::GameData::discover()`，未被他处包裹 |
| `config.rs:230-234` | ✅ `MIR2_DB_PATH` 读 env + `filter(!trim().is_empty())` `:230-233`，与计划先例描述一致 |
| `maps.rs` 4 早退 | ✅ `:979-981`、`:1040-1042`、`:1174-1176`、`:1235-1237` 均为 `eprintln!("跳过…")` + `return`；`#[test]` 计数 **20**，与 B 的 `--list` 20 例口径一致 |
| Crystal `Map.cs:2375-2383` | ✅ `:2375` 签名、`:2378` `if (ob.Race == ObjectType.Merchant) NPCs.Remove((NPCObject)ob);`、`:2382` `GetCell(...).Remove(ob)`；对照 `AddObject` `:2368/:2372` |
| Crystal RemoveObject 全量 | ❌ 计划"10 命中"→ 实测 **26** 命中；且 `grep -ci npc` = **0**，结论成立（见 M2） |
| `Envir.cs:4367` Remove(NPCInfo) | ✅ 仅定义行命中，全仓零调用者 |
| `npc.rs:658-773` | ✅ `npcs()` `:658`、`npc()` `:664`、`spawn_npcs` `:687`、`spawn_npc` `:762-773`、注释"只随地图销毁" `:770-771`；全仓 `despawn_npc` **0** 命中 |
| `world.rs:1959-1979` / `:4999-5008` | ✅ `leave` `:1961-1980`：广播 `:1965-1978` → `aoi_remove` `:1979`；`despawn_pets` `:4999-5010`，实际广播在 `:5009` `forget_monster`（计划端点 `:5008` 略窄，非阻断） |
| `aoi.rs:190/:200` | ✅ `pub fn insert` `:190`、`pub fn remove` `:200` |
| R54 `plan.md:44/:103` | ✅ 两处旧文仍在（`:44` "对发现用例的目标运行"、`:103` "A1/A2/A4/A5/A6/B"），G1 勘误有对象 |
| HEAD | ✅ `67c618cbb589f353e0fba5b2220dae45712fb4da` == `origin/main` |

## 4. 范围与 PARTIAL 口径意见

- **唯一写范围成立**：B=`gamedata.rs`+`tests/maps.rs`，C=`npc.rs`+新测试，A=三处文档，T=审计件；无重叠。若采纳 D1 降级，B 缩为 `tests/maps.rs` 单文件，更安全，建议如此。
- **计划外但已声明**：A 的 `docs/roadmap.md` 一行基线脚注未在 G4 出现，属已声明的唯一写范围，可接受，须在证据 `c/files.json` 单列该行 diff。
- **门禁口径**：§四 绑定 HEAD+原始 stdout/stderr+`exit_code`+`log_sha256`、两次提交分记受测 HEAD、`0 tests/提前 return/跳过 一律不计 PASS` —— 与 `team-and-roles.md:119-157` 一致，无需新造。
- **PARTIAL 承接**：P1（AOI-5）本轮**不得整体关闭**；P2（maps）按 D2 条件关闭，须双记录（CI 16/20 + 定向 4/4）；P3/P4/P5/P6 不动。R55 结论上限为 **PASS-WITH-PARTIAL**，任何"R55 完整/AOI 全量"表述即判越界。
- **预算**：A/B/C/T 四单 + 一次全量门禁已贴近；本轮不再加 D、不加新工具、不改 CI 契约。建议 A 先动笔、B/C 并行、cargo 由 Lead 串行。
