# R54 尚书省验收（gpt-6.1-sol）

主单 `JJC-20261026-002`｜子单 A=`sub-0ab4fe01` B=`sub-442ee0f5` C=`sub-88709a65` T=`sub-cf3f4e3c`
HEAD=`69b1931c4b8fa62ccc4b4116def31b80b5f65cd1`（== `origin/main`）｜验收 2026-10-26T11:29–11:40+08:00
范围：R54 AOI 增量五路由 + Frost 边角。**本文件不宣称“AOI 全量验收”或“R54 完整”**。

## 0. 结论

**ACCEPT-WITH-PARTIAL**。M1–M5 实质条款全部兑现（4 项全绿、1 项含合法 PARTIAL）；门禁真实且绑定提交树；
提交树 9/9 MATCH（本王独立复核 4/9 一致）；T 必改 0；PARTIAL 6 类诚实承接 R55。
判 **不可 ACCEPT** 的唯一理由：AOI-5 NPC 子项无生产入口、maps 4/20 资源早退、13 项 R53 承接仍开放，
按门下 §4“PARTIAL 不写 PASS”只能给 WITH-PARTIAL。两部文字偏差 Δ1/Δ2 见 §5，均无授权写者，不构成假绿。

## 1. M1–M5 逐条判定

| # | 判定 | 证据（路径:行） |
|---|---|---|
| M1 high B/T | **PASS** | 拒绝原因由生产判定给出：`crates/mir-server/src/world.rs:2521-2522`（`!same_group`/`!same_guild`，拒绝码见 :2503 前置链 存活/自击/安全区/map_no_fight）。真实建关系+正对照：`crates/mir-server/tests/round54_frost_edges.rs:608-871`（party）、`:877-1185`（guild），helpers `:335-356` 走 `group_invite/group_join`、`create_guild/guild_invite/guild_join`；cast+pending→关系改变→跨延迟落地重检；guild join HP 100000→1364 已在测量前重置。无注毒/无直写 group_id/guild（构造仅 `None`）。概率口径：4 种子×300=1200 次真实 cast，Slow 43 / Frozen 58，E=48∈[24,72] 未放宽，命中+miss 各 1200。 |
| M2 med A/T | **PASS（Δ1）** | 逐前缀双目标 `--list`+执行：`target/round54/aoi/attempt-1/m2-summary.txt` 五前缀各 `test_aoi_discovered=1/executed=1 lib_discovered=1/lib_executed=1`，`TOTAL_DISCOVERED=10 TOTAL_EXECUTED=10 OVERALL_FAIL=0`；20 条命令全 `EXIT: 0`（`m2-matrix.log`）。0 tests 未被计通过（发现数=执行数）。`#cfg(test)` 实体确在 `crates/mir-server/src/world.rs:5906-5907`。Δ1：`docs/process/round54-plan.md:44` 原文仍为“对发现用例的目标”，未改“对所有…逐个运行”——实质条款已由 m2-summary 兑现。 |
| M3 high A/T | **PASS（NPC 回收 PARTIAL）** | NPC 首帧走生产路径：`crates/mir-server/tests/aoi.rs:691-708` 经 `world.enter`→`spawn_npcs`，断言返回 ID/名称/坐标精确集 `(1,14,10)/(2,28,28)`；返回类型 `pub fn spawn_npcs(&mut self, map_file:&str)->Vec<u32>` = `crates/mir-server/src/npc.rs:687-748`。`:757-762` 文档注释自证“测试与 GM 用；生产路径走 `World::spawn_npcs`”，未冒充生产。NPC 回收无生产入口→PARTIAL，私测 `world.rs:7761,7774` 证明长推进后容器仍在且零 ObjectRemove，**未手删容器**。 |
| M4 med A/C/T | **PASS** | 到期锚点+严格 `>`：Crystal `github/Crystal/Server/MirObjects/ItemObject.cs:129-135`（判定 `:131` `if (Envir.Time > ExpireTime)`）↔ Rust `crates/mir-server/src/items.rs:1630`（`if now > ground.expire_time`）；断言 `tests/aoi.rs:1172-1221`：等值保留（含金币对照）→`advance_clock(1)` 每对象恰一条 remove→二次 tick 零重复。batching 与直接 send 已区分：`world.rs:5577-5582`（`broadcast_batching||broadcast_batch.is_some()` 入批）→ 真实 `tick()` flush，按每接收者 append 序解码，不以 channel 取包时刻证全局序；README §8 自限“仅单 tick 两帧”。 |
| M5 med 尚书/C/T | **PASS（Δ2）** | 矩阵已按五路由名/完整前缀命名：`docs/testing/round54/round54-aoi-acceptance.md:14-16,199-209`（`aoi_acceptance_enter_snapshot` 等，非 A1/A2）；19 项冻结：`docs/testing/round54/round54-carryover.md:1-32`，来源 `docs/process/round53-shangshu-acceptance.md:29-47`，逐条资源/动作/断言/有效数/原 exit，统计 **6 关闭（13/14/15/17/18/19）/13 承接**。Δ2：`docs/process/round54-plan.md:103` 源文仍写“A1/A2/A4/A5/A6”，纠正落在矩阵侧（派发 §4 已重下该口径）。首红独立 attempt 保留（`frost/attempt-1/selfcheck-0{1,2}-*.log` 101→0）。 |

## 2. 门禁真实性（PASS）

- `target/round54/gate/attempt-1.meta.txt`：`head=69b1931c…65cd1`（== 当前 HEAD == origin/main，本王复核）、`exit_code=0`、
  `start/end_utc=2026-10-26T03:18:17Z/03:22:33Z`（256 s）、`log_bytes=194319`、`log_sha256=7c18f3cf…ab43ff`。
- 本机复算：`stat -c %s`=194319、`sha256sum`=`7c18f3cfe55acef1ac59e8c82838f940f6af6068bf15d53f49169c05fcab43ff` —— **bytes/sha 逐字段一致**。
- 12/12：`target/round54/gate/attempt-1.log:3316-3327` 恰 12 行 `OK`，末行 `:3329` = `==> 全部通过（12 项）`；含 `cargo test --workspace`（211 s）与 clippy -D warnings。
- 非旧轮冒充：日志内命中本轮独有测试名 `aoi_acceptance_*`×10 + `round54_frost_edges`；DB URL 已脱敏 `postgres://postgres:***@nas:3001/mir2_test`。无 attempt-2，首红未覆盖。

## 3. 提交树与 T（PASS）

- 提交树 9/9 MATCH：`target/round54/commit-verify/01-tree-hashes.txt` ↔ `git show 69b1931:<path>`。本王独立复算 4 条全中：
  `world.rs` `cb18f27f…`(351,925 B)、`aoi.rs` `e6b4ffc3…`、`round54-aoi-acceptance.md` `b5a79ef7…`、`round54-carryover.md` `e3b93658…`。
- 提交边界：`c007691`(process, +257/-3) 与 `69b1931`(candidate, +3639/-11, 9 文件) 不重叠；工作树仅 2 份未跟踪历史冻结 JSON（44.8/45.2 MB）+ 本轮过程文档，未入库。
- T：`docs/testing/round54/round54-final-audit.md:12` PASS-WITH-PARTIAL、`:16` 必改 0、A–H 全过；`final-manifest.json` `must_fix=0/blocking=0`、`head_equals_origin_main=true`；`target/round54/audit/summary.log` 同口径。
- 候选评审 F1（acceptance 引用死链 `regression-lib-aoi.log`）已修为 `regression-lib-aoi-grid.log`（`round54-aoi-acceptance.md:8`）；`check_links.py` exit 0（1701 链接）。

## 4. PARTIAL 承接清单（R55，6 类 / 13 项）

1. **AOI-5 NPC 回收**无生产入口（产品缺陷，不得手删容器造绿）。
2. **maps 4/20 资源早退**（`maps.rs:980,1041,1175,1236` 缺 `resource/Server.MirDB`；reported 20 ≠ 有效 16）。
3. **私有测耦合**（`world.rs` `*_private`/合成 MirDb，非生产路径全覆盖）。
4. **batching 仅单 tick 两帧**（多 tick/多接收者/跨图混合未压）。
5. **A 集成首跑日志缺失**（进图首跑失败未保全，证据缺口，非无失败）。
6. **R52/R53 承接 13 项**（carryover #1–12 + AOI-5 子项；含 R52-4 缩窄）。

## 5. 残余风险

- **Δ1/Δ2 文字偏差**：`round54-plan.md:44`、`:103` 源文未改（该文件属冻结 process 提交，M2/M5 写者无授权）；实质条款已由 m2-summary 与五路由命名兑现，但文档与派发口径存在可追溯性缺口，建议 R55 首轮勘误。
- **NPC 回收为产品缺口**，非测试缺口；如需关闭须扩生产范围（本轮禁区）。
- **maps 4 例早退**：资源就绪后必须重跑，否则 16/20 基线易被误读为全绿。
- **门禁绿只覆盖本轮受测范围**；batching 与 PK 完整矩阵/G1/G6/EnemyGuild 仍未测，不得外推。
- 本王未执行 edict CLI、未 commit、未改 crates；提交二（文档件）待 Lead 显式逐文件纳入。
