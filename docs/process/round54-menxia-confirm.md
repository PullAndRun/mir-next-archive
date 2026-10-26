# R54 门下省确认与收官（gpt-6.1-sol）

主单 `JJC-20261026-002`｜HEAD=`69b1931c4b8fa62ccc4b4116def31b80b5f65cd1`（==`origin/main`，本王复核）｜确认 2026-10-26T11:34–11:52+08:00
只读：未改 `crates/`、未跑 `cargo`/`ci.ps1`、未 commit、未执行 edict CLI；只写本文件。

## 0. 结论

**PASS-WITH-PARTIAL 收口成立**。尚书 `ACCEPT-WITH-PARTIAL` 每项宣称经实物抽核成立，无一例假绿；两处 Δ 属文档级非阻断（冻结件、无授权写者），PARTIAL 6 类/13 项与刑部 manifest 逐条对齐。**本文件不宣称“AOI 全量验收”或“R54 完整”**。

## 1. 证据抽核（3 处全 PASS）

① **门禁**：`target/round54/gate/attempt-1.meta.txt` 载 `head=69b1931c…65cd1`、`exit_code=0`、`log_bytes=194319`、`log_sha256=7c18f3cf…ab43ff`；本机复算 `attempt-1.log` 194319 B、`7c18f3cfe55acef1ac59e8c82838f940f6af6068bf15d53f49169c05fcab43ff` —— bytes/sha 逐字段一致。`:3316-3327` 恰 12 行 `OK`（含 `cargo test --workspace` 211s、clippy `-D warnings`），`:3329`=`==> 全部通过（12 项）`；本轮测试名命中 `aoi_acceptance_`×10、`round54_frost_edges`×1；无 attempt-2、DB URL 已脱敏。精度备注（非阻断）：全日志 `^OK` 计 13，多出 `:3252` 属物品图标步骤自打印，非门禁项。
② **刑部 T**：`docs/testing/round54/round54-final-audit.md:11` PASS-WITH-PARTIAL、`:14` 必改 0、`:15` A–H 全过；`final-manifest.json` `verdict/must_fix=0/blocking=0/head_equals_origin_main=true`；`target/round54/audit/summary.log` 同口径（`[PARTIAL] count=6`）。交叉一致：1200 cast（Slow 43/Frozen 58/E=48∈[24,72]）、10/10 `OVERALL_FAIL=0`、全库 4PASS/1PARTIAL/0FAIL 且仅否定式出现“非全量”。
③ **M1/M2/M5**：`crates/mir-server/src/world.rs:2503`（存活/自击→安全区→`map_no_fight`）、`:2521` `!same_group`、`:2522` `!same_guild` 行号**逐字命中**；真实建关系 `crates/mir-server/tests/round54_frost_edges.rs:608`（party）/`:877`（guild）、1,185 行；`#[cfg(test)]`/`mod tests {` 在 `world.rs:5906-5907`。`target/round54/aoi/attempt-1/m2-summary.txt` 五前缀 `test 1/1 + lib 1/1`、`TOTAL_DISCOVERED=10 TOTAL_EXECUTED=10 OVERALL_FAIL=0`，`m2-matrix.log` `EXIT: 0`×20。`round54-carryover.md:1-6` 立“PARTIAL 不写 PASS”，路由名见 `round54-aoi-acceptance.md:47,54-55`。附：`world.rs` 树哈希 `cb18f27f…`/351,925 B 复算 1/1 命中；`git status --untracked-files=no` 为空（crates 零改动）。

## 2. Δ 判定（文档级非阻断 → R55 勘误）

| Δ | 位置 | 偏差 | 判定 |
|---|---|---|---|
| Δ1 | `docs/process/round54-plan.md:44` | 仍“对发现用例的目标运行”，未改“对所有…逐个运行” | 非阻断：`m2-summary` 五前缀×双目标 1/1 已兑现实质条款 |
| Δ2 | `docs/process/round54-plan.md:103` | 仍“A1/A2/A4/A5/A6/B”，与五路由名不符 | 非阻断：纠正已落矩阵侧/派发 §4，仅可追溯性缺口 |

两处均在 `plan.md`（冻结 process 提交 `c007691`），M2/M5 写者无编辑授权 → 本轮不改，R55 首轮勘误；不构成假绿。

## 3. PARTIAL 承接（6 类/13 项，与 manifest P1–P6 一一对应）

1. AOI-5 NPC 回收无生产入口（未手删容器）↔P1；2. maps 4/20 早退（`maps.rs:980,1041,1175,1236`，有效 16）↔P2；3. 私有测耦合（`world.rs` `*_private`/合成 MirDb）↔P3；4. batching 仅单 tick 两帧↔P4；5. A 集成首跑日志缺失（证据缺口，非“无失败”）↔P5；6. R52/R53 承接 13 项（19=关闭 6+承接 13，含 R52-4 缩窄）↔P6 CARRIED。口径“具名测试+可复现命令+原始 exit”未放宽。

## 4. R55 建议（按优先级）

1. 首轮勘误 Δ1/Δ2（或附勘误附录），消除派发口径与矩阵命名的可追溯性缺口。
2. NPC 回收入口单独立项（扩生产范围）；未立项前 AOI-5 子项仍写 PARTIAL。
3. maps 资源就绪后重跑 20 例，并把“4 早退”写入基线脚注，防 16/20 被误读为全绿。
4. 补 A 组集成首跑日志保全（成功/失败均留原件），封 P5。
5. batching 多 tick/多接收者/跨图混合压力矩阵；PK 完整矩阵、G1/G6、EnemyGuild 仍不在绿覆盖内，不得外推。

## 5. 边界

- 仅确认“ACCEPT-WITH-PARTIAL 是否有证据支撑”与“Δ/PARTIAL 承接是否成立”，不重启验收、不改判 M1–M5。
- 门禁绿只证明本轮受测范围；**不得宣称 AOI 全量验收、R54 完整、13 项承接已关闭**。
- 提交二（过程文档件）待 Lead 显式逐文件纳入；本王未 commit/push、未运行 edict CLI。
