# R54 独立终审（刑部，gpt-6.1-sol）

**仓库**：`C:\Users\diana\Documents\github\mir-next`｜**分支**：`main`（`origin/main` == `HEAD`）
**HEAD**：`69b1931c4b8fa62ccc4b4116def31b80b5f65cd1`（候选提交，已提交并推送）
**上游 process 提交**：`c007691`（R54 方案 / 门下评审 / 尚书派发）
**审计方式**：只读。未改候选、未跑 `cargo`/`tools/dev/ci.ps1`、未 commit/push。
**审计时间**：2026-10-26T11:23–11:35+08:00（Asia/Shanghai）
**本文件为唯一审计落盘（另见 `final-manifest.json`、`target/round54/audit/summary.log`）**

## 0. Verdict

**PASS-WITH-PARTIAL**。

- 门禁 `attempt-1`：`head=69b1931`、`exit_code=0`、**12/12 OK**，日志与 meta 的 bytes/sha256 逐字段一致（§A）。
- 提交树 9 文件哈希：**9/9 MATCH**（§B）。
- 必核 A–H：**A–H 全过**（无 high/BLOCK；必改代码/证据 0 条）。
- 文字级发现 2 条（F1 文档措辞滞后于门禁、F2 `docs/process` → `docs/testing` 路径重定向补记），均不阻塞。
- 剩余 PARTIAL **6 类**（含 R52/R53 承接 13 项），本轮**不**宣称“AOI 全量验收”“R54 完整”（§H）。

> 判定口径：门禁绿 + 提交树哈希绑定 + A–H 通过 ⇒ 门禁层面 PASS；但产品/证据缺口仍在 ⇒ 整体
> `PASS-WITH-PARTIAL`，PARTIAL 逐条列于 §PARTIAL，绝不以 PASS 表述。

## A. 门禁真实性（PASS）

原件：`target/round54/gate/attempt-1.meta.txt`（374 B，sha256 `fee03b17b7f1768287f736b4e133b4bab179313288611a80b4c1c6613891d5f4`）、
`target/round54/gate/attempt-1.log`（194,319 B，sha256 `7c18f3cfe55acef1ac59e8c82838f940f6af6068bf15d53f49169c05fcab43ff`）。

| meta 字段 | 值 | 复核 |
| --- | --- | --- |
| attempt | 1 | ✅ 与目录名一致，未见 attempt-2 |
| command | `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale` | ✅ |
| cwd | `C:/Users/diana/Documents/github/mir-next` | ✅ |
| head | `69b1931c4b8fa62ccc4b4116def31b80b5f65cd1` | ✅ `git rev-parse HEAD` == `origin/main` 同值 |
| branch | `main` | ✅ |
| start_utc / end_utc | `2026-10-26T03:18:17Z` / `03:22:33Z` | ✅ 256 s；文件 mtime `11:22:33.687 +0800` 与 end 同刻 |
| exit_code | 0 | ✅ 以及 `[exit code: 0]` 由执行 shell 回报 |
| log_bytes | 194319 | ✅ `wc -c` = 194319 |
| log_sha256 | `7c18f3cf…` | ✅ `sha256sum` 同值 |

- **12 项逐项 OK**：末段汇总表 `target/round54/gate/attempt-1.log:3316-3327` 恰 **12** 行 `OK `（`awk` 计数 = 12），
  末行 `:3329` == `==> 全部通过（12 项）`。条目：`cargo fmt --all --check`、`cargo clippy --workspace --all-targets -D warnings`、
  `cargo test -p mir-server --test duo_smoke(--nocapture)`、`cargo test --workspace`、`协议检查 --check`、`文档链接检查`、
  `TextKey 键校验`、`硬编码检查(player-visible 阈值 70)`、`硬编码检查(no-test 阈值 0)`、`运行时新资源方式`、
  `物品图标完整性`、`PostgreSQL 集成测试（mir-store 内置）`。每项 `    OK   (Ns)` 独立存在（`:5,11,29,3119,3123,3126,3140,3247,3252,3313` 等）。
- **非旧轮冒充**：日志内实跑命中 R54 独有测试名 —— `aoi_acceptance_*` 10 个（含 `_private`）与 `round54_frost_edges`
  共 14 处命中（`grep -c`），且日志内 UTC 时间戳 `2026-10-26T03:21:47` 落在声明窗口内。旧轮日志不可能包含本轮新增测试名。
- **凭据**：日志首行 DB URL 已脱敏为 `postgres://postgres:***@nas:3001/mir2_test`（`***`），未见明文口令。

## B. 提交树绑定与提交边界（PASS）

复核命令（见 §复现命令 R2）：对 `target/round54/commit-verify/01-tree-hashes.txt` 每行执行
`git show 69b1931:<path> | sha256sum`，**9/9 MATCH**：

| 路径 | bytes（工作树/文件清单） | sha256（提交树复核） |
| --- | ---: | --- |
| `crates/mir-server/src/world.rs` | 351,925 | `cb18f27f5f0262213c2d0a7a193910bc598a53e4ea7d90725f01c22f8438a892` |
| `crates/mir-server/tests/aoi.rs` | 52,992 | `e6b4ffc3374843e686d314766b78c40ebd6f20dda5c0e3190b9eda5cca3480f3` |
| `crates/mir-server/tests/round54_frost_edges.rs` | 42,403 | `727718288c8c6380f98d87c6ad276049d68a69926c7f00c829165638b9a4d09b` |
| `docs/world/aoi.md` | 22,815 | `ee0d029f2ec838083a232d45e5fd5265461fbacb20313b665dd1caafd5fd6a4b` |
| `docs/testing/round54/README.md` | 11,729 | `5af41fcb1c7a7e5fbd16394fff0c4e2271db8c7af07c41de006b4ba13190e563` |
| `docs/testing/round54/round54-aoi-acceptance.md` | 14,028 | `b5a79ef774519d88a0cee3755859b4f5ff26eea687fd8e63cf166619f1b790c4` |
| `docs/testing/round54/round54-carryover.md` | 15,195 | `e3b936587ffe0734f14497180c1baec2136ca607f8b1edf45bdbbbf4dc4367f7` |
| `README.md` | 43,067 | `5db721ed098b0e280662eacd9c3f77749205f40a42231a51d52a4609281abb5c` |
| `docs/roadmap.md` | 21,119 | `2adcba84dc65674060bc7803bad1c99ec776222f7f90739c9243adc24fb3a22a` |

- 合计 575,108 B；最大 `world.rs` 351,925 B（<1 MiB）。
- 与 `target/round54/c/files.json`（A/B/C 证据清单）逐条一致；`c/files.json` 的 `patched_post_candidate_review`
  记录 F1/F2 修复（acceptance 14,028 B / sha `b5a79ef7…`），与提交树一致。
- **两提交边界清晰**：`c007691`（process，3 文件 +257）与 `69b1931`（候选，9 文件 +3,639/−11）互不重叠；
  `git show --stat` 无交叉路径。
- **工作树**：`git status --porcelain` 仅 2 行未跟踪历史冻结 JSON ——
  `docs/testing/monster-native/player-resource-freeze-before.json`（44,844,165 B）、
  `docs/testing/monster-native/player-resource-freeze.json`（45,219,533 B）；无多余修改，未跟踪文件未入库。

## C. M1 兑现：Frost 精确 +2 与 Group/Guild 真实护栏（PASS）

证据：`target/round54/frost/attempt-1/{00-summary.md,03-run.log,fix/03-run.log,fix/07-post-revert-run.log,06-lib-magic.log}`。

- **1200 次真实 cast**（60 施法 / 62 目标 / 技能 3，4 种子 × 300）：Slow 命中 **43**、Frozen 命中 **58**；
  双侧计数各 1200（`slow_hits+slow_misses==1200` 同 frozen）；期望 4% → **E=48**，允许区间 **[24,72]**，
  43/58 均在内且**未放宽、未换种子**。每样本 `damage>0`、HP 因果（`HP == 落地前 - dealt`）、落地前无 pending。
- **+3 对照**：目标 63 级 100 次真实施法全部受伤且**始终无 Slow/Frozen 两毒**（等级门跳过毒判定）。
- **Group/Guild 真实路径**：`World::group_invite`+`group_join`、`World::create_guild`+`guild_invite`+`guild_join`
  （`round54_frost_edges.rs` helper :335-356）；拒绝由 `crates/mir-server/src/world.rs:2503,2521-2522` 的
  `!same_group` / `!same_guild` 决定，并有翻模式正对照（同队/同会允许、异队/异会拒绝）。
- **cast+pending → 真实入队/入会 → 跨延迟落地拒绝** 链路实测（`selfcheck-landing-recheck.log`、`00-fix-summary.md`）。
- **无注毒/无直写关系字段**：零 `PoisonState` 注入，毒仅由落地 `resolve_frost_crunch` 真实产生；`group_id`/`guild`
  仅构造时置 `None`，不经赋值伪造。
- 门禁机械修复（格式化 + clippy）后字节 1,185 行、sha `727718288c8c…`，4 条测试命令复跑全绿，断言语义未变。

## D. M2/M3：发现=执行 与 NPC/maps 诚实（PASS，NPC 回收 PARTIAL）

- **10 discovered / 10 executed，`OVERALL_FAIL=0`**：`target/round54/aoi/attempt-1/m2-summary.txt`、
  `m2-matrix.log`（`TOTAL_DISCOVERED=10 TOTAL_EXECUTED=10`）、`m2-discovered.txt`（10 个具名例）；
  五前缀 × 两目标（`--test aoi` / `--lib`）各 1/1，逐条 `exit 0`。0 tests 从不判 PASS。
- **NPC 首帧生产入口**：`world.enter → spawn_npcs`（`crates/mir-server/tests/aoi.rs:691-708` →
  `crates/mir-server/src/world.rs:7284-7291` → `crates/mir-server/src/npc.rs:687`），非测试/GM 造物。
- **NPC 回收 PARTIAL 且未手删容器**：无生产 NPC 移除入口；私测 `world.rs:7761,7774` 证明长推进后容器仍存在
  且无 `ObjectRemove`，测试**未**手工清容器冒充回收。
- **maps 20 passed 中 4 例早退**：`crates/mir-server/tests/maps.rs:980,1041,1175,1236` 缺 `resource/Server.MirDB`
  时提前 `return`；**有效 16 例**，已在文档与证据中标注为有效数，不冒充 20 例全有效。

## E. M4：物品严格 `>` 边界与 batching 真实 tick（PASS）

- **严格 `>`**：Crystal `ItemObject.cs:129-135`（判据 `if` 在 :131）与本作 `crates/mir-server/src/items.rs:1629`
  均为严格 `>`；`crates/mir-server/tests/aoi.rs:1172-1221` 断言“等值保留、+1 ms 每对象恰一条 remove、二次 tick 零重复”，
  另有金币对照与怪物尸体回收。
- **batching 真实 flush**：`set_broadcast_batching(true)`（`tests/aoi.rs:1353`）后经真实 `world.tick()`
  （`:1404`）flush，按**每接收者 append 序**解码断言；**不**以 channel 取包时刻冒充全局发送序。
- **文档已写**：`docs/testing/round54/round54-aoi-acceptance.md:151`（§7）、`docs/world/aoi.md` §11、
  `docs/testing/round54/README.md` §8 均记“单 tick 两帧”压力边界。

## F. 假绿/捷径排查（PASS-WITH-2-TEXT-NITS）

- **变异自检转红**：A 组 5×2 变异各 5 处 → `0 passed; 5 failed` exit 101（`mutation/mutation-selfcheck.log:49`），
  逐条 panic 与路由对应；还原后 `5 passed`（`:73`）且 `cmp` BYTE-IDENTICAL；post-fmt baseline 复验仍转红
  （`fix/mutation-postfmt/mutation-selfcheck-postfmt.log`，`0 passed; 5 failed`）。
- **Frost 自检短路**（`magic.rs:4098`）→ `round54_frost_edges.rs:1166` 转红 exit 101，还原后 3/3 exit 0。
- **无 `#[allow]`**：`round54_frost_edges.rs` 0 处、`aoi.rs` 0 处；`world.rs` 新增 hunk 0 处（既有 4250/4324/4895 未在 diff）。
- **无 CJK 断言**：新增断言全英文，`grep -n "assert.*[一-龥]"` 于两测试文件 0 命中（exit 1）。
- **无改生产修绿**：`world.rs` 唯一 hunk `@@ -7174,0 +7175,922 @@ mod tests`，`#[cfg(test)]` 起 :5906 至 EOF，
  无生产 getter/开关/feature；`git diff --stat -- crates/mir-server/src` 的生产段为空。
- **无未执行命令写成通过**：原始数字 43/58/148800/124/1364、aoi13/lib7/maps20、lead 13/7/3/7/8/43/20/66/0
  均可回溯到各自日志原值。

## G. 文档与 M5（PASS，F4 重定向已补记）

- **一致性**：`README.md:78`、`docs/roadmap.md:26`、`docs/world/aoi.md:314-335`（§11）、
  `docs/testing/round54/{README.md,round54-aoi-acceptance.md,round54-carryover.md}` 均为
  **4 PASS / 1 PARTIAL / 0 FAIL**，且均明写“AOI 全量验收仍不可关闭 / PARTIAL”。
  全仓检索“AOI 全量通过 / 全量验收”仅命中上述**否定式**表述（0 处正面宣称）。
- **M5 19 项 = 关闭 6 + 承接 13**：`docs/testing/round54/round54-carryover.md:7`（“6 项关闭（13/14/15/17/18/19）、
  13 项承接（R52-4 缩窄、AOI-5 仅 NPC 子项承接）”），表体 19 行（`grep -c "^| [0-9]* |"` = 19），
  每条带本轮证据/原始 exit。来源可追溯：`docs/process/round53-shangshu-acceptance.md:29-47`（19 项原文）、
  `docs/testing/round53/round53-final-audit.md`（存在，`docs/testing/round53/` 目录内）。
- **F4 重定向补记（本审计新增一行，不改 process 文档）**：
  `docs/process/round54-plan.md:71` 与 `docs/process/round54-shangshu-dispatch.md:32` 写新文档落
  `docs/process/round54-{carryover,aoi-acceptance}.md`，实际提交落在 `docs/testing/round54/round54-{carryover,aoi-acceptance}.md`。
  **据此登记为路径重定向，非范围违规**：候选 9 文件清单（Lead 直委）采用 `docs/testing/round54/` 为准；
  两处 process 文本的旧路径不构成下一轮的事实来源，以本审计与本目录为准。
- **i18n 66/0**：`README:77-78` 与门禁日志 `:3236,3242` 的 66 / 0 一致，历史 67 见 `docs/i18n.md:919-927`，无新假数。

## H. 卫生与 PARTIAL 诚实（PASS）

- **入库卫生**：本轮 9 文件 575,108 B，最大 351,925 B（<1 MiB）；提交树无 `target/`（0 命中）、无 `.env`/密钥文件（0 命中）；
  门禁日志内 DB URL 已脱敏。工作树仅 2 个历史大冻结 JSON 未跟踪（44.8 MB / 45.2 MB），未入库。
  **F3（既有偏差，非本轮引入）**：`docs/testing/attack-variants/attack3-asset-plan.json` = **1,228,554 B**（>1 MiB），
  由 R47 `bfa9a09` 引入，不在 `69b1931` 的 9 文件内，本轮未读取其内容、未纳入提交；建议后续单独瘦身/外置，不作本轮阻塞项。
- **PARTIAL 清单完整**（§PARTIAL），无一条被写成 PASS；未见“AOI 全量验收 / R54 完整”字样。
- **诚实性**：A 组集成首跑未留日志已自列入 README §8；maps 4/20 早退、私有测耦合、batching 单 tick、
  NPC 回收无生产入口、13 项承接全部明列。

## Findings（≤7，全部非阻塞）

| # | 级别 | 位置 | 内容 | 处置 |
| --- | --- | --- | --- | --- |
| F1 | text | `README.md:78`、`docs/testing/round54/round54-aoi-acceptance.md:12,209`、`docs/testing/round54/README.md:4-5,135` | 提交（11:18 前）时写“全量门禁未运行”，而门禁 `attempt-1` 于 11:18:17–11:22:33 在同一 head 上以 exit 0 / 12 项通过完成 → 该句在本审计时点已滞后（方向保守，非假绿）。 | 下一轮文档同步改述“全量门禁已跑（attempt-1, 12/12），剩余 PARTIAL 为产品项”；本轮仅记录，不改文档（只写 3 文件约束）。 |
| F2 | text | `docs/process/round54-plan.md:71`、`docs/process/round54-shangshu-dispatch.md:32` | 路径写 `docs/process/round54-{carryover,aoi-acceptance}.md`，实际 `docs/testing/round54/`。 | 已在 §G 补记重定向一行；判定非范围违规。 |
| F3 | low（既有） | `docs/testing/attack-variants/attack3-asset-plan.json` | 1,228,554 B > 1 MiB，R47 `bfa9a09` 既有文件，非本轮 9 文件。 | 记录待后续瘦身；不作本轮阻塞。 |
| F4 | closed | `target/round54/commit-verify/01-tree-hashes.txt` | 候选评审 F5 要求提交后 `git show <commit>:<path>` 复核，已执行 **9/9 MATCH**。 | 已闭环，绑定见 §B / `final-manifest.json`。 |
| F5 | info | `docs/testing/round54/README.md:42-44` | README 仍记提交前哈希（未写 commit sha），由 `final-manifest.json` 承载提交树 sha 与两 commit。 | 下一轮文档可引 manifest；本轮以 manifest 为准。 |
| F6 | info | `target/round54/aoi/attempt-1/README` 缺失 | A 组集成首跑无日志（已在 README §8 自列）。 | PARTIAL 保留，不补造日志。 |

**必改条数：0**（无代码/证据/门禁必改；F1/F2 为文字级，按约束由下一轮文档处理或已在本审计补记）。

## PARTIAL 清单（6 类，逐条可追溯）

1. **NPC 回收**（AOI 回收路由 NPC 子项）：生产无 NPC 移除/回收入口；私测仅证明容器保留（`world.rs:7761,7774`）。来源 `round54-carryover.md` AOI-5。
2. **maps 资源早退**：20 passed 中 4 例缺 `resource/Server.MirDB` 提前 return（`maps.rs:980,1041,1175,1236`），有效 16。
3. **私有测耦合**：5 个 `*_private` 用例位于 `world.rs` 内部 `mod tests`，依赖私有可见性，非纯黑盒。
4. **batching 单 tick**：仅单 tick 两帧压力，长期/多 tick 队列行为未覆盖（README §8）。
5. **A 集成首跑日志缺失**：A 组集成目标首次执行未留原始日志（README §8 自列）。
6. **R52/R53 承接 13 项**：`round54-carryover.md:1-19`，来源 `round53-shangshu-acceptance.md:29-47`，本轮未执行验证，不得回填为绿。

## 复现命令（只读，不改仓库）

```bash
# R1 门禁字节/哈希/条目
cd C:/Users/diana/Documents/github/mir-next
sha256sum target/round54/gate/attempt-1.log          # 7c18f3cfe55acef1ac59e8c82838f940f6af6068bf15d53f49169c05fcab43ff
wc -c target/round54/gate/attempt-1.log              # 194319
sed -n '3310,3330p' target/round54/gate/attempt-1.log | awk '/^OK /{c++} END{print c}'   # 12
git rev-parse HEAD; git rev-parse origin/main        # 同 69b1931…

# R2 提交树 9 文件哈希复核（9/9 MATCH）
sed -n 's/^OK *\([^ ]*\) \([0-9a-f]\{64\}\)$/\2 \1/p' target/round54/commit-verify/01-tree-hashes.txt |
while read -r sha path; do [ "$(git show 69b1931:$path | sha256sum | cut -d' ' -f1)" = "$sha" ] && echo "MATCH $path" || echo "MISMATCH $path"; done

# R3 工作树边界（应仅 2 个未跟踪大 JSON）
git status --porcelain=v1
# R4 假绿扫描（应为 0 / 0）
grep -c '#\[allow' crates/mir-server/tests/round54_frost_edges.rs crates/mir-server/tests/aoi.rs
grep -rn "AOI 全量通过" docs/ | head
```

## 提交 / 门禁 / 证据绑定

| 项 | 绑定值 |
| --- | --- |
| 候选提交 | `69b1931c4b8fa62ccc4b4116def31b80b5f65cd1`（feat(test): R54 AOI 五路由补证与 Frost 正侧/同队同会护栏） |
| process 提交 | `c007691`（docs(process): R54 方案、门下评审与尚书派发） |
| 门禁 | `attempt=1`、`head=69b1931`、`exit=0`、`12/12`、`03:18:17Z–03:22:33Z`、`194,319 B`、`sha256 7c18f3cf…` |
| 树哈希清单 | `target/round54/commit-verify/01-tree-hashes.txt`（9/9 MATCH，本审计复算） |
| 证据索引 | `target/round54/{gate,aoi/attempt-1,frost/attempt-1,c,commit-verify,lead-verify2,candidate-review}` |
| 文档 | `docs/testing/round54/{README.md,round54-aoi-acceptance.md,round54-carryover.md}`、`docs/world/aoi.md` §11、`README.md:78`、`docs/roadmap.md:26` |
| 工具链 | `rustc/cargo 1.99.1`（`rust-toolchain.toml` 固定）、Python 3.14.7、Windows x86_64 |
| 机器清单 | `docs/testing/round54/final-manifest.json`｜摘要 `target/round54/audit/summary.log` |

---

**结论复述**：R54 门禁在候选提交树上真实通过（12/12, exit 0），9 文件提交树哈希 9/9 一致，A–H 全过，必改 0 条；
但 NPC 回收等 6 类 PARTIAL 与 13 项 R53 承接仍开放，故终审 **PASS-WITH-PARTIAL**，**不得**表述为“AOI 全量验收通过”或“R54 完整”。
