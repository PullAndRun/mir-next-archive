# R57 刑部终审复审 v2（T2）

- **子单** `JJC-20261026-004-sub-e9251ec2` · **复审对象** HEAD `38f3afe` · **角色** 刑部复审 T2（只读，不 commit、不跑 cargo/ci）
- **时间** 2026-10-26T14:51:35+08:00
- **T1 原件保持不动**：`round57-final-audit.md`（sha256 `1c519dd7…`）、`final-manifest.json`（`588baaf…`）、`target/round57/audit/summary.log`（`53eced31…`）；本件为 v2，不覆盖 v1。

## 零、结论

**verdict = PASS-WITH-PARTIAL。** H1（门禁捕获）与 M1（magic.md 旧 125 口径）均闭合；attempt-2 日志可从字节级独立证明 12/12（12 项逐项输出 + 汇总 + fmt + 时长自洽）。**尚书签 12/12：有条件允许**（3 条文档收尾条件，见 §七，均不改变门禁结论）。

| 项 | 判定 | 一句话 |
|---|---|---|
| A H1 门禁捕获 | **PASS** | 200,569 B / sha `c47cef85…` / EXIT=0 / 12 项 OK + 末行全部通过 + fmt 项；meta 与日志字节/时长/HEAD/起止自洽 |
| B M1 125 口径 | **PASS** | 三组禁词 grep=0；4+1 处旧口径已改；新增行无「PASS/全链/已完整」新结论（唯一 PASS 为保留的 R52 历史句） |
| C 提交边界 | **PASS** | 3 提交分别 3/10/2 文件；tracked 无改动；13 候选 git blob sha 13/13 MATCH |
| D M2 登记 | **PARTIAL** | M2 已登记；PARTIAL 索引 4 必列项中 3 项在、缺 `can_fly` 双实现（1 行） |
| E 假绿 | **PASS** | R57 6 named 全 ok；无旧轮摘要冒充；0 tests 属 ci.ps1 明示跳过且有串行项覆盖；fmt 可见 |
| F 结论 | **PASS-WITH-PARTIAL** | 残余 M3/L2–L7 均非阻断，R58 承接；另 2 条文档收尾观察 |

## 一、A. H1 是否闭合 —— PASS

1. **独立可证的 12/12**：`target/round57/gate/attempt-2.log:3325-3337` 为 `== 汇总 ==` 与 12 条 `OK <项> <耗时>`；`:3338` 末行 `==> 全部通过（12 项）`；全文 `test result: FAILED` = 0、`^failures:` = 0、`panicked` = 0。
2. **逐项真实输出**（行号）：fmt `:4-5`（`==> cargo fmt --all --check` + `OK (3 s)`）与汇总 `:3326`；clippy `:7-9`；duo_smoke 串行 `:11-27`；workspace `:29-3130`；协议生成 `:3131-3133`（`packets: 432, files changed: 0`）；链接 `:3135-3137`（`[OK] 1706 条…`）；TextKey `:3139-3151`（243/243）；棘轮 70 `:3153-3247`（66 处 ≤70）；棘轮 0 `:3249-3253`（0 处）；老格式 `:3255-3258`；图标 `:3260-3263`（kept=1346, 违规=0）；PG `:3265-3323`（15+1+4+4+1 全 ok）。
3. **meta 与日志一致**（`attempt-2.meta.txt`，sha256 `acad9d8c…`）：`LOG_BYTES=200569` = 磁盘 stat；`LOG_SHA256=c47cef85…` = 实测；`HEAD=38f3afe…` = `git rev-parse HEAD`；`START_UTC=06:44:02Z` / `END_UTC=06:46:15Z` / `DURATION_S=133` 三者自洽，且日志 mtime `14:46:15.775+08:00` = `06:46:15.775Z` 与 END 秒级吻合；12 项耗时合计 3+1+11+111+1+1+0+0+0+0+0+1+3 = **132s ≈ 133s**（若为旧轮摘要粘贴不可能与本次时长吻合——attempt-1 总耗 237.8s）。
4. **attempt-1 失败史保留且有说明**：`attempt-1.log` 199,054 B / sha `73f823e8…` 未动；实测无 `== 汇总 ==`、无 `cargo fmt` 横幅、无 `==>`，末行停在 mir_store doctests（即 H1 现场）；`attempt-2.meta.txt` 的 `NOTE=attempt-1 exists but captured only pipeline success stream (missing summary/exit/fmt) - retained for history` 对缺陷与保留意图说明齐全。
5. **GIT_DIFF_SHA256 = 空**：值 `e3b0c442…` 即空串 sha256；`git status --porcelain` 仅 4 个未跟踪件（2 历史冻结 JSON + 2 T1 产物），tracked 零改动，`target/` 被 `.gitignore:4` 忽略。**限定**：该值只覆盖 tracked 内容；未跟踪的 T1 审计件当时已在工作树（涉及 §七 条件 1）。

## 二、B. M1 是否闭合 —— PASS

- `grep -n "无服务端生产者\|服务端无生产者\|代码批在途" docs/world/magic.md` = **0 命中**。
- `38f3afe` 实改 5 处：`:39`、`:685`、`:774`、`:806` 四处旧句统一为「125 客户端渲染未接（R57 起 FireBounce 为服务端 125 生产者，PARTIAL）」；`:687` 矩阵行「🟡 代码批在途」→「✅ 服务端生产者（R57，门禁与终审判定见 `docs/testing/round57/round57-final-audit.md`）；客户端 125 渲染 PARTIAL」，描述列补「服务端 125 生产者已落盘 / 客户端 125 渲染未接 / 125 不回显施法者为已知偏差」。与要求句一致。
- **新增行无新结论**：`git diff -U0 858147b..38f3afe` 的 `+` 行中「PASS/全链/已完整」仅 1 处，为 `:39` 保留的 **R52 历史句**「attempt2 门禁 12/12 PASS（exit0）」（改动前即存在，非 R57 新结论）；新增文字无「全链」「已完整」。
- **观察（非阻断，见 §七 条件 1）**：`:687` 引用的 `round57-final-audit.md` 目前**未跟踪且为 v1**，v1 结论「12/12 不可独立取证」在 attempt-2 后已过时；第三次提交应改指 v2。

## 三、C. 提交边界与候选 sha —— PASS

- `git show --stat 025564c`：恰好 **3 文件**（`crates/mir-server/src/magic.rs`、`crates/mir-server/src/world.rs`、`crates/mir-server/tests/round57_firebounce.rs`；1044+/5−）。
- `git show --stat 858147b`：恰好 **10 文件**（文档批）。
- `git show --stat 38f3afe`：恰好 **2 文件**（`docs/testing/round57/README.md` 9+/1−、`docs/world/magic.md` 10+/5−，合计 +13/−6，与 Lead 描述一致）。
- 13 候选 = 3 代码 + 10 文档的唯一并集（38f3afe 只改其中 2 个已在 10 文件内的文档）。
- **13/13 MATCH**：逐文件 `git show HEAD:<path> | sha256sum` 与 `attempt-2.meta.txt` 附加 13 行全等（含 `magic.rs 0f2a0da5…`、`round57_firebounce.rs c5ce11b2…`、`magic.md 91e58b4d…`）。meta 按 **git blob** 记账（符合 T1 L1 建议口径）；`docs/world/magic.md` 磁盘为 CRLF 67,617 B、blob 为 LF 66,733 B / `91e58b4d…`，meta 记 66,733/`91e58b4d…` 正确；`git status` 干净由 autocrlf 归一化保证，无内容漂移。
- 工作树未跟踪件仅：`docs/testing/monster-native/player-resource-freeze{,-before}.json`（历史冻结）+ T1 的 `final-manifest.json`、`round57-final-audit.md`；target/ 被忽略。

## 四、D. M2 登记与 PARTIAL 索引 —— PARTIAL

- **M2 已登记**：`docs/testing/round57/README.md:45`「终局弹跳 `remaining==0` 的 RNG 抽签顺序与 Crystal 有 1 次 `MirRng` 差异（M2 未登记→本轮登记，待 R58 评估；`magic.rs:2774`）」——满足「remaining==0 RNG 差 1 次、待 R58」口径。
- **四必列项**：125 不回显施法者（`:41`）✓；Hero 无实例（`:43`）✓；一般场景 ≤ level+2（`:44`）✓；**`can_fly` 双实现 ✗ 未列**（全仓 grep：`can_fly` 只出现在 v1 审计 `:24/:64/:131` 与 `final-manifest.json:191`，README §4 无）。
- 现有 7 条 PARTIAL（`:40-47`）：125 不回显、客户端 125 渲染、Hero、≤level+2、M2 RNG、默认 DB 缺记录、次级目标随机序列。补 1 行 `can_fly` 双实现（`magic.rs:4029` vs `:4047`）即闭合 T1 L6（§七 条件 2）。

## 五、E. 假绿检查 —— PASS

- **R57 独有测试名逐个具名全 ok**：`:2706-2716`（`fire_bounce_cast_emits_magic_and_object_magic_without_projectile`、`fire_bounce_first_hit_lands_after_distance_delay`、`fire_bounce_does_not_emit_126_or_149_and_chain_accepts_player_hero_candidates`、`fire_bounce_chain_stops_without_nearby_monster_or_bounce_exhausted`、`fire_bounce_emits_125_before_each_secondary_hit_and_hits_level_plus_two_targets`、`fire_bounce_secondary_target_filter_and_random_choice`；`6 passed; 0 failed; 0 ignored`）。
- **无旧轮摘要冒充**：唯一 `==> 全部通过` 1 处（`:3338`），且 12 项耗时合计 132s 与本次 133s 自洽（attempt-1 为 237.8s，摘要无法复用）。
- **0 tests 不充当通过**：`running 0 tests` 共 9 处均为 main.rs/无单测 crate/空 doctests 的附带目标；workspace 项内 duo_smoke「0 passed … 7 filtered out」系 `tools/dev/ci.ps1:118` 明示的 `--skip duo_ --skip fault_pg`（`:110-117` 注释说明覆盖率由串行项与第 5 步 PG 门禁兜住），而**串行 duo_smoke 项本体**在 `:11-27` 跑 7 用例（5 passed / 2 ignored 附原因）。
- **fmt 项有真实执行**：`:4-5` 横幅 + `OK (3 s)` + 汇总 `:3326`；`cargo fmt --check` 成功时静默无 diff 属预期行为，非缺证据。

## 六、F. 残余项处置（均非阻断）

| 项 | 状态 | 处置 |
|---|---|---|
| M2 | **已闭合** | README:45 登记，待 R58 评估 |
| M3 | 未修 | `target/round57/firebounce/attempt-1/run-checks.sh:33` 仍 `packets::server::tests::...` 错路径（真实 `packets::server::roundtrip::...`）；用户已定非阻断，R58 修 |
| L1 | 实质闭合 | attempt-2 meta 统一按 git blob 记账；仅 `magic.md` 磁盘 CRLF 未统一，无损 |
| L2 | 未修 | A0 探针不在提交树；数据集与字段已交叉核对，R58 承接 |
| L3 | 未修 | clippy 第二轮红无独立日志；R58 承接 |
| L4 | 部分改善 | attempt-2 meta 已含 EXIT/耗时；attempt-1 历史件按设计保留；R58 承接 |
| L5 | 未修 | attempt-2 仍 10 行 GBK 乱码（`:152-157`、`:763`、`:3030-3032`），均为 ignored 用例中文说明，不影响 12 项断言与用例名；R58 统一捕获编码 |
| L6 | **3/4 闭合** | 见 §四；缺 `can_fly` 一行 |
| L7 | 未修 | `crates/mir-server/src/world.rs:1378` 注释仍 `dvance_clock`（纯注释）；R58 |
| O1 | 新增观察 | `magic.md:687`、`README.md:40` 引用未跟踪 v1 审计件；check_links 通过因该文件恰在工作树，第三次提交须纳入 v2（否则提交树链接悬空/引用过时） |
| O2 | 观察 | workspace 内 duo_smoke「0 tests」为 ci.ps1 明示跳过，独立串行项已覆盖（非假绿） |

## 七、是否允许尚书签 12/12

**允许**（门禁 12/12 已可从 attempt-2 独立取证），**附 3 条文档收尾条件**（签发前完成；不改变门禁/代码结论）：

1. 第三次（文档/审计）提交必须包含 `docs/testing/round57/round57-final-audit{,-v2}.md` 与 `final-manifest{,-v2}.json`，使 `magic.md:687` / `README.md:40` 的引用在提交树内可解析；建议将 `magic.md:687` 与 `README.md:40` 的引用改指 `round57-final-audit-v2.md`（v1 结论已被 attempt-2 取代）。
2. `docs/testing/round57/README.md:40-47` 补 1 行 `can_fly` 双实现，闭合 T1 L6；否则签发所附 PARTIAL 清单仍不完整。
3. 签发表述沿用 T1 限定：服务端 125 生产者=已实现；客户端 125 渲染=PARTIAL；125 不回显施法者=已知偏差；不得称 125 全链或 R57 完整验收。
4. R58 承接：M3、L2、L3、L4、L5、L7。

## 八、复现命令

```bash
cd C:/Users/diana/Documents/github/mir-next
sha256sum target/round57/gate/attempt-2.log                 # c47cef85… / 200569 B
sed -n '3325,3338p' target/round57/gate/attempt-2.log        # 12 项 OK + 全部通过
sed -n '4,5p;2706,2716p' target/round57/gate/attempt-2.log   # fmt 项 + R57 6 named
git show --stat 025564c 858147b 38f3afe                      # 3 / 10 / 2 文件
for f in <13 candidate files>; do git show HEAD:$f | sha256sum; done  # 13/13 与 meta 附加行比对
grep -n "无服务端生产者\|服务端无生产者\|代码批在途" docs/world/magic.md   # 0
grep -n "can_fly\|MirRng\|125 不回显\|Hero" docs/testing/round57/README.md  # can_fly 缺失
```

（只读审计未跑 cargo/ci；12/12 以 Lead 落盘的 attempt-2 原始日志为证。）

## 九、与 v1 差异

- v1 的两条阻断（H1 捕获、M1 口径）均已关闭 → 本件无阻断项。
- v1 的 L6 由「4 项未列」降为「1 项未列（can_fly）」，其余 3 项已入索引。
- 新增 O1/O2 两条观察；判定仍为 **PASS-WITH-PARTIAL**，允许尚书签 12/12（条件见 §七）。
