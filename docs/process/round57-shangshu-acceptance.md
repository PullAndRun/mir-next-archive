# R57 尚书省验收（JJC-20261026-004）

- **角色**：尚书令 shangshu（gpt-6.1-sol，只读；未改 `crates/`、未跑 `cargo`/`ci.ps1`、未 commit/push）
- **时间**：2026-10-26T14:58–15:01+08:00（Asia/Shanghai）
- **主单**：`JJC-20261026-004`「第57轮：ObjectProjectile生产者与R56承接清理」；验收前 state=Doing，验收后 state=Review
- **受测 HEAD**：`38f3afe8261837529a563482131d89436a6178c1`（attempt-2 门禁树）；验收时 HEAD=`eab6986`（post-gate 审计提交）
- **输入复算**：`target/round57/shangshu-acceptance/01-input-sha.log`（13 件 sha256+bytes，meta 自报值逐项相等）

## 一、结论

**ACCEPT-WITH-PARTIAL**：门禁 12/12 可从 attempt-2 原件独立取证；13 候选 blob 与受测树全等；T2 必改 1 项（`can_fly` 登记）已闭合。**允许主单进 Review**。
口径：服务端 125 生产者=已实现；客户端 125 渲染=PARTIAL；125 不回显施法者=已知偏差。**不称「125 全链」「R57 完整验收」**。

## 二、证据表

| 证据 | 实测 | 独立取证 |
|---|---|---|
| `target/round57/gate/attempt-2.log` | 200,569 B / sha256 `c47cef85e79ce655cc78ddaf834b90a37f59cdc3b26c56501882f06548e4e41e` | 与 meta `LOG_BYTES`/`LOG_SHA256` 全等 |
| 12 项汇总 | `:3326-3337` 12 行 `OK`；`:3338` `==> 全部通过（12 项）`；`test result: FAILED`/`^failures:`/`panicked` 均 0 | `grep -c "^OK   "`=12 |
| fmt 项 | `:4` 横幅 + `:5` `OK (3 s)`，汇总 `:3326` 复现 | 非「成功即静默」缺证 |
| meta 绑定 | `HEAD=38f3afe`、`EXIT_CODE=0`、06:44:02Z–06:46:15Z/133s；12 项耗时合计 132s | 日志 mtime `14:46:15.775+08:00` 秒级吻合 |
| `attempt-1.log` | 199,054 B / `73f823e8…`；无 `== 汇总 ==`、无 fmt、无 `==>` | H1 现场保留未覆盖（meta NOTE 有说明） |
| 13 候选 @HEAD | **13/13 blob sha MATCH**（`git show 38f3afe:<path>` 复算） | `02-gate-verify.log:60-73` |
| `eab6986` | 6 文件全 docs（T1/T2 审计 4 + `README.md`/`magic.md` 修正），`crates/` 改动 0 | `git show --name-only` |

## 三、必核 A–F

- **A 门禁真实 — PASS**：attempt-2 半全流捕获（`CAPTURE=full(*> all streams)`），fmt/12 项/末行齐全；meta 与日志 bytes/sha/HEAD/起止/exit 自洽；attempt-1 失败史原样保留，H1 已由 attempt-2 闭合。
- **B 提交与受测树 — PASS**：13 候选在 HEAD=`38f3afe` 的 git blob sha 与 meta 逐一相等（含 `magic.rs` `0f2a0da5…`、`round57_firebounce.rs` `c5ce11b2…`）；`025564c`=3 码、`858147b`=10 文档、`38f3afe`=2 文档、`eab6986`=6 文档且 0 crates；工作树仅 2 个历史冻结 JSON，tracked diff 空。post-gate 审计提交符合派发 §四 M4/§六（T 产物不参与候选 sha）。
- **C 结论承接 — PASS**：T2 verdict=`PASS-WITH-PARTIAL`；必改仅 1 项（`can_fly` 双实现）已在 `docs/testing/round57/README.md:44` 补登；`docs/world/magic.md:687`、`docs/testing/round57/README.md:40` 引用已改指 v2（T2 §七条件 1/2 均闭合）；`magic.md:687`/`round57/README.md:41-46`/`round57-final-audit-v2.md:79` 三处「服务端已实现 / 客户端 PARTIAL / 不回显已知偏差」一致，全仓无「125 全链/R57 完整」正面表述。
- **D PARTIAL 台账 — PASS**：现 8 条——条件式 LOCK（`README:45`）、客户端 125 渲染（`:42`）、Hero 无实例（`:43`）、`can_fly` 双实现 `magic.rs:4029` vs `:4047`（`:44`）、M2 终局 RNG 差 1 次 `magic.rs:2774`（`:46`）、默认 DB 缺记录时生产学习路径（`:47`）、次级目标随机序列（`:48`）、125 不回显（`:41`）——与 T2 §四 及 README §4 对齐，可整体转 R58。
- **E 残余非阻断 — PASS（登记）**：M3 `target/round57/firebounce/attempt-1/run-checks.sh:33` 仍错路径 `packets::server::tests::…`（实为 `…::roundtrip::…`）；L2 A0 探针 `00-db-firebounce.log` 仅在 gitignore 的 `target/`；L3 clippy 二轮红仅 `10b-clippy-second-red-note.log` 说明、无独立原始日志；L5 attempt-2 有 **10 行** GBK 乱码（`:152-157`/`:763`/`:3030-3032`，均为 ignored 用例中文说明，严格 UTF-8 解码 0 失败）；L7 `crates/mir-server/src/world.rs:1378` BEL 已删但注释仍为 `dvance_clock`。均不影响 12 项断言与代码语义。
- **F 决定 — ACCEPT-WITH-PARTIAL**：A/B/C/D 全过、E 全为非阻断；**允许主单进 Review**，R58 承接下表。

## 四、R58 承接清单（12 项，按优先级）

1. **M3** 修 `target/round57/firebounce/attempt-1/run-checks.sh:33` 协议测试全路径并补 `00-db`/`10-clippy`/`15b`/`19` 手工件，使脚本可复现。
2. **L3** clippy 二轮红补独立原始日志（不得覆盖后再写 note）。
3. **L2** A0 DB 探针脚本/原始输出入提交树（脱敏），替代仅存 `target/`。
4. **L5** 统一 CI 捕获编码（UTF-8），清 attempt-2 的 10 行乱码。
5. **L7** `world.rs:1378` 注释 `dvance_clock` 补回 `a`（纯注释）。
6. **can_fly 双实现**：评估合并 `magic.rs:4029` 通用与 `:4047` FireBounce 落点专用，或文档固化差异理由。
7. **M2 RNG**：终局 `remaining==0` 的 1 次 `MirRng` 抽签与 Crystal 不同（`magic.rs:2774`），评估改序或永久登记 MINOR。
8. **默认 DB 缺 FireBounce 记录**：补生产学习/掉落路径（现仅测试 fixture），登记 PARTIAL。
9. **次级目标随机序列**：集合/分布等价、非逐次同序，保持 MINOR。
10. **125 不回显施法者**：`World::broadcast` 语义差异，评估是否按 Crystal 回显。
11. **Hero 无实例**：候选过滤已按 Monster/Player/Hero 写，Hero 无实例需登记而非改语义。
12. **客户端 125 渲染**：原生弹道 `410`、命中 `Effect 570` 与音效未接（D3），仍为 PARTIAL。
（另：`eab6986` 的 2 处候选文档 post-gate 修正未重跑 attempt-3；因 0 代码改动且尚书已复跑链接/空白检查，判可接受；若门下要求严格 §六，可追认或补跑。）

## 五、CLI 回执（原始，见 `target/round57/shangshu-acceptance/03-cli.log`）

| # | 命令 | exit | 回执 |
|---|---|---|---|
| 1 | `memo JJC-20261026-004 shangshu "<ACCEPT-WITH-PARTIAL 摘要>" --as shangshu` | 0 | `📝 JJC-20261026-004 任务记忆：shangshu` |
| 2 | `done JJC-20261026-004 --as shangshu` | **0** | `✅ JJC-20261026-004 执行完成，已提交尚书省审查`（未被 readiness 拦截） |
| 3 | `show JJC-20261026-004 --as shangshu` | 0 | 状态 **Review**，归属 尚书省；流转 `执行中 → 尚书省 待汇总` |
| 4 | `validate --as shangshu` | 0 | `✅ 看板校验通过：154 条任务符合 schema v3.1` |

## 六、边界声明

- 只读验收：未改 `crates/`/门禁原件/看板卡片，仅新增本文件与 `target/round57/shangshu-acceptance/**`（4 件：01 输入 sha、02 门禁复算、03 CLI、04 检查）。
- 未跑 `cargo`/`ci.ps1`；12/12 以 Lead 落盘的 attempt-2 原件为准，尚书只复算 sha/bytes/行号与候选 blob。
- 本件不追认 Crystal「无效目标仍扣蓝」差异、不追认 126/149、不追认客户端渲染；PARTIAL 未闭项一律沿用 T2/README 口径。
- 签发仅表示「本轮交付可进 Review」，不等于「R57 完整验收」；Review 后的门下 confirm 与主单 Done 由后续环节裁决。
