# R55 刑部独立终审报告（只读）

> 审计：刑部独立终审（gpt-6.1-sol，只读，子单 `JJC-20261025-005-sub-1ea152c0`）
> 基线：HEAD=`d9ea92e1cab2e830389a82fe1f83faf4e846102e`=`origin/main`（已 push）
> 契约：`docs/process/round55-shangshu-dispatch.md`（§二/§三 M1–M5/§五）；门下口径 `docs/process/round55-menxia-review.md`
> 审计时刻：2026-10-26T12:16→12:34 +08:00；只读，未改代码/docs/git、未跑 `tools/dev/ci.ps1`、未 commit/push

---

## 0. 结论（Verdict）

**VERDICT：PASS-WITH-PARTIAL（R55）** — 门禁真实、提交树绑定成立、G1 PASS、G2 **条件 PASS**、G3 **子项 CLOSED / AOI-5 整体 PARTIAL**；0 阻塞、必改 0 条（评审 3 项非阻塞必改已全部处置，第 3 项=本报告 §E.5 登记）。

| 项 | 判定 | 一句话 |
| --- | --- | --- |
| 门禁真实性 | **PASS** | exit 0，12 项全绿，194,592 B / sha `43e7baad…` 与原件一致，含 R55 独有测试名 |
| 提交树绑定 | **PASS** | 9/9 文件 `git show d9ea92e:` sha 全 MATCH；三提交文件集互不重叠 |
| G1 文档勘误（A） | **PASS** | `:44`/`:103` 同行勘误注；1 行/0 命中；README 50 行无预判；check_links 1705 exit 0 |
| G2 maps（B） | **条件 PASS** | 带 env 4/4 passed 0 skip；不带 env 20 passed 4 skip（CI 默认仍 **16 有效/4 skip**） |
| G3 NPC 回收（C） | **子项 CLOSED / 整体 PARTIAL** | `despawn_npc` 四要素对齐 Crystal；Crystal 运行时零 NPC 调用者，生产接线待 R56 |
| 口径一致性 | **PASS** | M1 三处同文；"R55 完整/AOI 全量/AOI-5 已关闭"零正面表述 |
| 假绿/卫生 | **PASS** | i18n 两闸 0 命中 exit 0；无新增 `#[allow]`；无 0 tests 冒充；跳过不当 PASS |
| 必改（评审非阻断） | **0 条待办** | ①README 状态列已修 ②提交卫生已达成 ③`npc_ids_near` 本轮登记 |

---

## A. 门禁真实性（`target/round55/gate/attempt-1.*`）

| 字段 | 报称 | 实测 | 判定 |
| --- | --- | --- | --- |
| attempt | 1（Lead 唯一执行） | 目录仅 `attempt-1.log`/`attempt-1.meta.txt`，无第二 attempt | ✅ |
| log bytes | 194,592 B | `wc -c` = 194,592 B | ✅ |
| log sha256 | `43e7baaddab322275293ca2963f848c8969a42bdd2cb82aa0babdde25c2bdb9b` | `sha256sum` 逐字节相同 | ✅ |
| meta HEAD | `d9ea92e1…46102e` | `d9ea92e1cab2e830389a82fe1f83faf4e846102e`（=HEAD=origin/main） | ✅ |
| 原始 exit | EXIT=0 | meta `EXIT=0`；末项后 `==> 全部通过（12 项）` | ✅ |
| 起止 | meta `2026-10-26T04:11:20Z`（=12:11:20+08） | log 无起始戳；逐项耗时合计 7+27+26+215+1+0+0+0+0+0+1+3=**280 s** → 反推开始 ≈`04:06:40Z`（推算，已标注） | ✅（单时间戳，起为推算） |
| 12 项 | 全部通过 | 末尾逐项 `OK` 12 行：fmt/clippy/duo_smoke/workspace test/导出检查/文档/TextKey/硬编码 70→0/运行时资源/商品图标/PG 集成 | ✅ |

- **本轮日志而非旧轮**：log 内含 R55 独有 `round55_npc_recycle`（1 处）、maps 真实用例名（`real_database_has_movement_points_that_resolve_to_maps`、`bichon_has_a_walkable_movement_point_near_the_plaza` 等）、既有 `aoi_acceptance` 10 处；R55 之前的树不可能出现 `round55_npc_recycle`，故排除"把首红/旧轮日志当本轮"。
- **DB URL 脱敏**：log 内全部 URL 形如 `postgres://postgres:***@nas:3001/mir` / `…/mir2_test`，口令段为 `***`，无明文凭据。✅

## B. 提交树绑定

- 9 个文件逐一对 `git show d9ea92e:<path> | sha256sum` 与 `target/round55/commit-verify/01-tree-hashes.txt` 对照：**9/9 MATCH**（plan 12597、menxia 8788、dispatch 10697、round54-plan 16400、roadmap 21401、README 4195、maps.rs 51986、npc.rs 129013、round55_npc_recycle.rs 8057）。
- 三提交边界**互不重叠**：`4d672cc` 仅 3 份 `docs/process/round55-*`（+227）；`6e1771c` 仅 `npc.rs`(+66/−0) + `tests/maps.rs`(+47/−6) + 新测试(+241)；`d9ea92e` 仅 `round54-plan.md`(2 行) + `roadmap.md`(+2) + `docs/testing/round55/README.md`(+50)。零文件重叠。
- `git ls-tree -r HEAD` 复核：`target/` **0 条**；`player-resource-freeze*.json` **0 条**（45 MB 冻结件仍为 `??` 未跟踪，排除项成立）。

## C. G1 文档勘误（A 子单）→ PASS

- `docs/process/round54-plan.md:44` 与 `:103` 均为**同一行内**"新文 + ` > 原文（R54）：…`"；`grep -n "原文（R54）"` 行号恰为 44/103（2 处，无第三处）。
- 两条 grep 原样复跑：①`grep -n "对发现用例的目标运行\|A1/A2/A4/A5/A6/B"` → **1 行**（`:44` 勘误注内）；②同上 `| grep -v 原文` → **0 命中**（exit 1）。原件 `target/round55/docs/attempt-1/{03-grep-raw,04-grep-no-original}.log`。
- `docs/testing/round55/README.md`：50 行 / 4,195 B / sha `292932697523321c338fdc859f710d0b26397bf2de882cbfd8272b5a9d4f1df0`；`:3/:38` 明写"不预写 G2/G3（T 签发）"；`:14-15` 状态列"🟡 已落盘且证据齐，待 T 抽核（G2/G3 未签发）"未写 PASS；`:35` M1 同文；无"AOI-5 已关闭/R55 完整/AOI 全量"正面表述（`grep` exit 1）。
- `py -3 tools/docs/check_links.py` → `[OK] 1705 条相对链接…` **exit 0**（A 的 `01` 记 1701 属写 README 前时序，以当前树 1705 为准）。
- 注：A `files.json.head=67c618c`（A 单工作时点），A 三文件 sha 与 `d9ea92e` blob 一致。

## D. G2 maps 真实用例（B 子单）→ **条件 PASS**

- 改动面：`crates/mir-server/tests/maps.rs` **仅** +helper（`load_real_game_data()` `:43-75`）、import 增 `MirDbError`、4 处 `GameData::discover()`→`load_real_game_data()`；4 处 `eprintln!("跳过：resource/ …")+return` 逐字节未动（`grep -c` = 4，diff 无 `-`/`+` 行）。
- 计数：`--list` = **20 tests / 0 benchmarks**；带 `MIR2_DEV_DB_PATH='resource/community/suprcode-crystal-database/cjlaaa/Server.MirDB'` 跑 4 精确用例名 = **4 passed / 0 failed / 0 skip 行**；清 env 全量 = **20 passed / 4 行 skip**（早退不计 PASS）。
- 旧断言零放宽：diff 中无任何阈值改动；实跑值 74 传送点 / 比奇 70 条 / 18 格 / 36 步与 B 证据、评审复跑逐字一致。
- 契约命令 `real_database_` filter 只匹配 4 名中第 1 名 → B 已如实登记并补 4 名 multi-filter（`meta.json.commands[with-env-A/B]`）；本轮复核采 4 名精确 filter，结论不变。
- **条件 PASS 的边界（必须写明）**：CI 默认（无 env）下仍是 **20 例中 16 例有效 + 4 例 skip**；G2 的"4/4 真跑"依赖显式设置 `MIR2_DEV_DB_PATH`，不得当成"CI 已覆盖真实数据"。

## E. G3 NPC 回收（C 子单）→ 子项 CLOSED / AOI-5 整体 PARTIAL

- `crates/mir-server/src/npc.rs:811` `pub fn despawn_npc(&mut self, object_id: u32) -> Option<Npc>`，逐要素对齐 Crystal `github/Crystal/Server/MirEnvir/Map.cs:2375-2383`：
  | Crystal | 本作 | 判定 |
  | --- | --- | --- |
  | `:2378` `NPCs.Remove((NPCObject)ob)` | `self.npcs.retain(\|npc\| npc.object_id != object_id)` | ✅ 语义等价 |
  | `:2382` `GetCell(ob.CurrentLocation).Remove(ob)` | `self.aoi_remove(&npc.map, AoiKind::Npc, object_id)` | ✅ |
  | `MapObject.Despawn`→`Map.Broadcast`（`Map.cs:2437-2448`，发 `DataRange` 内全部玩家） | `object_remove_frame(id)` 先发落点 `VIEW_RANGE` 内全部玩家，不按"看见过"过滤 | ✅ |
  | `Player.NPC` 关系 | 本作无该字段；清 `visible_npcs` + 对话中 `player.npc.close()` | ✅ |
  | **顺序**：容器(`:2378`)先于格(`:2382`) | 本作 `aoi_remove` 先、`retain` 后（**相反**） | 记一句：两容器互不依赖，**无可观测差异**（包在两者之前已发），不要求改（评审提示②） |
- 未知 ID / 二次回收 / 非 NPC（怪物、玩家）均首查 `find()?` 早退 → 零包、零误删（测试 ④⑤⑥ 真断言）。
- `world.rs` diff **空**（`git show 6e1771c -- src/world.rs` 无输出）；未改广播模板，仅复用 `World::leave`/`despawn_pets` 既有形状。
- 新测试 `crates/mir-server/tests/round55_npc_recycle.rs`（241 行 / 8,057 B）：1 个测试 `despawn_npc_recycles_once_and_clears_relations`，**27 处 `assert`（6 断言组）全部为真断言**（`assert_eq!`/`assert!`，无 println 充数）；刷出走生产路径 `World::spawn_npcs`、进图走 `enter`；**无手删容器**；**无新增 `#[allow]`**（`npc.rs` 仅 `:594/:629/:647` 三处既有 `cast_precision_loss`，均为 R55 之前）。
- doc 有独立 `# 当前无生产调用者` 段，明写唯一调用者是本轮新测试。

**E.5 必改 #3 登记（本报告行）**：`crates/mir-server/src/npc.rs:675-683` 新增 `pub fn npc_ids_near(&self, map: &str, center: Point, radius: i32) -> Vec<u32>`（签名在 `:681`，doc `:675-680`）——**R55 新增测试观测入口、生产零调用**：全仓唯一使用方是 `crates/mir-server/tests/round55_npc_recycle.rs`（`:98/:171` 等），`grep -rn "npc_ids_near" crates/` 除定义与测试外 0 命中。该函数超出派发 §二 C 的"`despawn_npc` + 关系清理"括号清单，属只读 getter（非发明数据库/脚本 API），不构成范围扩张，但**必须在审计口径登记**；后续可考虑 `#[doc(hidden)]`/`#[cfg(test)]` 收敛（R55 不改，保最小改动）。

**G3 判定**：`despawn_npc` 子项 **CLOSED**；但 Crystal 服务端**运行时零 NPC 调用者**（`grep -rn "RemoveObject(" github/Crystal/Server` = **26 命中**，其中传入 `NPCObject` 的 = **0**；`Envir.Remove(NPCInfo)` 仅定义无调用者），本作亦无生产接线 → **AOI-5 整体仍 PARTIAL，生产接线待 R56**。

## F. 证据与复算

- 三套证据齐备：A `target/round55/docs/attempt-1/`（9 文件）、B `target/round55/maps/attempt-1/`（7 文件 + `meta.json`/`evidence.json`）、C `target/round55/npc/attempt-1/`（11 文件 + `meta.json`/`files.json`）。
- 抽查 ≥10 项 sha/bytes：9 个提交 blob（§B）+ B `meta.json` 4 个 log sha256（list/with-env/without-env/lint）+ C `meta.json` 2 文件 sha + A `files.json` 3 文件 sha = 18 项，全部与磁盘实算一致。
- `target/round55/candidate-review/review.md`：**verdict PASS-WITH-CHANGES**（0 阻塞 / 3 必改 + 2 提示）。3 必改处置：①`README.md:14-15` 状态列 → **已修**（见 `05-readme-status.log`，sha 与提交值一致）；②提交卫生（逐文件 staging、排除 2 份 45 MB 旧 JSON、`target/` 不入库）→ **已达成**（§B）；③`npc_ids_near` 登记 → **本报告 §E.5 完成**。
- **aoi flaky 如实记录**：C 首跑 `target/round55/npc/attempt-1/03-aoi.log` 中 `aoi_acceptance_recycle_all_kinds` 1 红（`crates/mir-server/tests/aoi.rs:1201`，`12 passed; 1 failed`）；Lead 5 连跑 13/13 全绿（`target/round55/lead-aoi-flake/repeated-runs.log`）；候选评审独立 13/13 绿（`verify.log`）；C 单例重复 5 次均 1 passed。判定**既有 flaky**，与本轮改动无因果（`npc.rs` 新码仅经新 API 可达），不掩盖、未改断言。

## G. 口径一致性

- **M1 三处同文**：①派发件 `:21/:60/:72`"**AOI-5 整体 PARTIAL、仅子项"无回收原语"CLOSED、生产零调用者接线待 R56**"；②A 的 `docs/testing/round55/README.md:35` 同文；③本报告 §0/§E 同文。三处一致。
- **零正面表述**：R55 三件中"AOI-5 已关闭/R55 完整/AOI 全量"仅出现在"禁止/不宣称"否定句（`grep` 复核 0 正面命中）。
- **不做清单**：README `:42` 与派发件均显式写"**D（125 真生产者 / 原生 dash 帧）顺延 R56**"。
- **M2 实测更正**：计划事实 10 声称 `grep -rn "RemoveObject(" github/Crystal/Server` "全量 10 命中"，实测 **26 命中**（传 `NPCObject` 的 0）；结论方向不变（NPC 零调用者），但数字须按 26 记。
- **PARTIAL 清单完整**（不含夸大）：①G2 条件 PASS（CI 默认 16 有效/4 skip）；②AOI-5 整体 PARTIAL；③`despawn_npc` 无生产调用者；④`aoi_acceptance_recycle_all_kinds` 既有 flaky；⑤R52 承接 R52-1（125 ObjectProjectile 无生产者）/R52-2（真 socket、客户端窗口）/R52-3（原生 dash、Walking 借帧）；⑥R53 十九项→R54 关闭 6、**承接 13**（`docs/testing/round54/round54-carryover.md`）。

## H. 假绿 / 卫生

| 检查 | 命令 | 实测 | 判定 |
| --- | --- | --- | --- |
| i18n 硬编码闸 | `py -3 tools/i18n/scan_hardcoded.py --no-test --player-visible --strict --fail-above 0` | `合计 0 处硬编码（33 个文件扫描）`、`[OK] 缺失 0 处、未翻译 0` **exit 0** | ✅ |
| i18n TextKey 闸 | `py -3 tools/i18n/verify_text_keys.py` | `[OK] 207 键 / 200 语言 / 11 模块 / 1 分类 / 32 扩展 / 243 TextKey 全部对应`、漏删 0、孤键 0 **exit 0** | ✅ |
| 断言英文 | 新测试全部英文断言消息（`spawn_npcs should spawn exactly one NPC` 等） | ✅ |
| 新增 `#[allow]` | `grep -n "allow(" npc.rs round55_npc_recycle.rs` | 仅 3 处既有 `cast_precision_loss`（`:594/:629/:647`），本轮 0 新增 | ✅ |
| 0 tests 冒充 PASS | `round55_npc_recycle` 实跑 1 passed；maps 双目标计数分列（20 / 4） | ✅ |
| 跳过当 PASS | maps 4 例 skip 全程标"条件 PASS / 16 有效"；B 证据 0 skip 行仅在带 env 时出现 | ✅ |
| 工作树卫生 | `git diff --check` **exit 0**（仅 LF→CRLF 提示）；`git status` 仅 2 份历史冻结 JSON `??` | ✅ |

## I. 非阻断异常（逐条登记）

1. **派发件报称值 vs 提交值不一致（已解释，内容完整性成立）**：尚书回报 10,787 B / sha `9bafcf7e…`；提交树（`git show d9ea92e:`）为 **10,697 B / sha `535a9adb72685b3dc9af0480db901f87332970143baf4e08eb85e3046300046a`**。差 **90 B = 文件 90 行**，成因是仓库 `core.autocrlf=true`：工作树为 CRLF（10,787 B），git blob 归一为 LF（10,697 B）。实证：`file` 报 `with CRLF line terminators`；`sed 's/\r$//' | sha256sum` = `535a9adb…`（与提交值**逐字节相同**）；`git diff --quiet d9ea92e -- <file>` = **空**。故：**报称值是工作树视图、提交值是权威值，内容 90 行/M1 三处完整，不据此否定派发**。
2. A 证据 `01-check-links.log`（1701）与当前树（1705）差 4 条链接：A 写完 README 后新增，属时序正常，以当前树 1705 为准。
3. 门禁 meta 仅 1 个时间戳（无起始戳），起始为按逐项耗时反推（已在 §A 标注"推算"）。
4. `npc_ids_near` 无 `#[doc(hidden)]`（评审提示①，非必改）。
5. `despawn_npc` 容器/格顺序与 Crystal 相反（§E，无可观测差异，记录不要求改）。

## J. 复现命令（只读）

```bash
# 门禁原件
wc -c target/round55/gate/attempt-1.log && sha256sum target/round55/gate/attempt-1.log
cat target/round55/gate/attempt-1.meta.txt
# 提交树 9/9
while read -r p sz sha; do a=$(git show d9ea92e:"$p" | sha256sum | cut -d' ' -f1); [ "$a" = "$sha" ] && echo "MATCH $p" || echo "MISMATCH $p"; done < target/round55/commit-verify/01-tree-hashes.txt
# 卫生
git ls-tree -r HEAD --name-only | grep -c '^target/'      # 0
git diff --check                                          # exit 0
# G1
grep -n "原文（R54）" docs/process/round54-plan.md        # 恰 44、103
py -3 tools/docs/check_links.py                           # [OK] 1705 exit 0
# G2（不改代码，只跑测试）
cargo test -p mir-server --test maps -- --list            # 20 tests, 0 benchmarks
MIR2_DEV_DB_PATH='resource/community/suprcode-crystal-database/cjlaaa/Server.MirDB' cargo test -p mir-server --test maps real_database_has_movement_points_that_resolve_to_maps bichon_has_a_walkable_movement_point_near_the_plaza real_map_files_load_and_change_map_moves_between_them player_can_walk_from_the_plaza_into_a_natural_teleport_point -- --nocapture   # 4 passed / 0 skip
cargo test -p mir-server --test maps -- --nocapture       # 20 passed / 4 skip
# G3
cargo test -p mir-server --test round55_npc_recycle -- --nocapture   # 1 passed
sed -n '2375,2383p' github/Crystal/Server/MirEnvir/Map.cs
grep -rn "RemoveObject(" github/Crystal/Server | wc -l     # 26
grep -rn "RemoveObject(" github/Crystal/Server | grep -c NPCObject   # 0
grep -rn "npc_ids_near" crates/ | wc -l                   # 定义 + 测试
# i18n 两闸
py -3 tools/i18n/scan_hardcoded.py --no-test --player-visible --strict --fail-above 0
py -3 tools/i18n/verify_text_keys.py
```
