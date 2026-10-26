# R60 刑部终审（D4b · gpt-6.1-sol · 只读独立复核）

- 仓库：`C:\Users\diana\Documents\github\mir-next`｜main｜HEAD=`e053905be38c57863b6f1a554e0cab111c1bd8e7`｜origin/main=`e053905`
- 复核时间：2026-10-26T18:47–19:04 +08:00（D4b 只读会话；未改 `crates/**`、未改既有 `docs/**`、未 commit/push、未跑 `ci.ps1`）
- 证据根：`target/round60/audit/`（`run-checks-d4b.log`、`d4b-cargo.log`、`i18n-*.log`、`summary.log`）

## VERDICT：**PASS-WITH-PARTIAL**

门禁真实（exit 0、12/12、字节/sha/meta 三方一致且日志内确有 R60 用例执行痕迹）；提交②边界干净（门禁后 `crates` 零改动）；树哈希 13/13 逐条复算一致且按 M-7 排除自哈希；D1 接线真实、D2 去重正确且有确定红→绿（101→0）；M-1..M-7 七项落点全部在位；无红线宣称。未给 PASS 的唯一原因：P-1 可听性仍未证、ring 16 仍丢最旧、无 Live 窗口级集成测试——均为**已诚实登记的 PARTIAL**，不是新缺陷。

## A. 门禁真实性：通过

| 项 | 实测 | 结论 |
|---|---|---|
| `wc -c target/round60/gate/attempt-1.log` | `205317`（exit 0） | 与已知一致 |
| `sha256sum` 同文件 | `193b6fe4f8a0a46397a0026c2e8a2d66f0e041c9fa33f0ad6ca3a17d1f3678dd` | 与 meta/清单一致 |
| `tail -20` | 12 条 `OK` 汇总 + `==> 全部通过（12 项）`；末行为空行（exit 0） | 12/12 真绿 |
| `grep -c "^OK "` | `13`（exit 0）——12 条汇总 + 第 3340 行程序输出 `OK 所有保留物品模板都有图标`，**非第 13 项门禁** | 计数口径已澄清，无隐藏项 |
| `cat attempt-1.meta.txt` | `START=2026-10-26T10:29:53Z END=…10:32:26Z BYTES=205317 SHA=193b6fe4…78dd` | 三相自洽 |
| `grep -rn "round60\|r60_" attempt-1.log` | 6 行：`net::tests::r60_d2_*` 2 条、`ui::projectile::tests::r60_d2_*` 4 条（全 `... ok`） | 门禁确实跑到 R60 用例，非空跑/跳过 |

推断：门禁记录绑定 head=`bb53321`（提交①），文档批 `e053905` 未重跑全量——与「每轮唯一一次全量门禁」口径一致，不构成假绿。

## B. 提交边界 / 树哈希：通过

- `git show --stat bb53321`：4 files `+593/−12`（`app.rs` 263、`net.rs` 76、`ui/mod.rs` 9、`ui/projectile.rs` 257）；`e053905`：10 files `+925/−7`，仅 `docs/**`。
- `git diff bb53321..e053905 --stat -- crates` → **空**（exit 0）：门禁后零代码改动，门禁结论对最终 HEAD 仍成立（docs 不影响行为）。
- `git status --short` → 仅 2 个历史未跟踪大冻结 JSON（`docs/testing/monster-native/player-resource-freeze{,-before}.json`），与已知一致，未被本轮引入。
- `docs/testing/round60/commit-tree-hashes.txt`：2209 B，sha256 `520379f025184edc5ca127e4db70b84d3d0ceebb6edd676622e7d27766abf450`；13 条数据行**逐条**复算 `wc -c`+`sha256sum` → `entries_ok=13 entries_bad=0`（非抽查 8 条，全量过）。
- 头字段齐备：`# head=bb53321`、`# gate_head=bb53321`、`# gate_log_sha256=`、`# gate_log_bytes=205317`、`# gate_log_path=`、`# gen=`；并明写「清单自身 sha256 不写入本清单，只由 D4 final-manifest.json 记录」——M-7 自哈希规则成立，无自指循环。

## C. D1 音效接线：通过（可听性仍 PARTIAL）

- `crates/mir-client/src/app.rs:201` `fn play_projectile_cues(sound: &mut SoundPlayer, cues: Vec<&'static str>, now_ms: u64) -> usize` → 转调 `:209` `fn play_projectile_cues_with(cues, play: impl FnMut(&'static str) -> bool) -> usize`；**`play` 返回 true 才计入已播**，频控/静音/无设备吞掉不计数（与派发件 R5 一致）。
- 生产调用点 `crates/mir-client/src/app.rs:2757` `live.ui.take_projectile_sound_cues()`、`:2759` `play_projectile_cues(&mut live.sound, projectile_cues, now)`，紧随 `:2753` `live.ui.sync(&live.snapshot)`——**同帧 drain**，不再是只采集不消费。
- 取件口 `crates/mir-client/src/ui/mod.rs:1275` 仅转调 `projectile::Projectiles::take_sound_cues()`，未复制/未清源队列。
- `TMP=TEMP=target/tmp cargo test -p mir-client --bin mir-client` → `21 passed; 0 failed; 0 ignored`，exit **0**（含 R60 5 条内联 + M34-1/2 真实 `.mna` 解码断言）。
- 限制同 P-1：headless 无音频设备，**只证「cue→解码→混音器」路径打通**，不证「能听见」。

## D. D2 去重：通过

- `crates/mir-client/src/ui/projectile.rs:461` `fn consume`：`dropped = projectile_events_total.saturating_sub(len)`（`:469-471`）；重建判定收紧为「空表」或「consumed > len 且 dropped == 0」（`:473-481`）；下标 `= cursor - dropped`（`:487`），游标推进到绝对序号（`:495`）。
- `crates/mir-client/src/net.rs:166` `const EFFECT_EVENT_LIMIT: usize = 16;` 未变；`:2772` `pub projectile_events_total: u64`（文档 `:2767` 写明 `total - len = 最旧被弃数`）；`:7265` `saturating_add(1)` 每次 accepted push 自增；`:2911` 默认 0。
- `git diff 3a9a104..HEAD -- crates/mir-protocol` → **0 行**：`ProjectileEvent` 与协议零改动（net.rs 仅 `+76/−0`）。
- `--lib ui::projectile` → `12 passed`（含 17 事件、连续 consume 不重放、重建归零），exit **0**；`--lib net::` → `30 passed`（含 total 自增与重建归零），exit **0**。

## E. 测试证据：通过

- `target/round60/d1/attempt-1/00-summary.md`：DONE；6 条验收 exit 0；M34-1/2 真实解码、**无 SKIP**；自述「不能断言能听见」与 PARTIAL 口径一致。
- `target/round60/d2/attempt-1/`：`04-red-before.log:28 exit=101`（旧实现第 17 事件新增 **0**）→ `05-green-after.log:13 exit=0`（新实现新增 **1**）；`03-red-green-sanity.log:8-9` 明确 `red exit=101 / green exit=0`，且有 `git diff` 快照（`04a`/`05b`）与还原日志（`05a`）——确定红→绿，非事后补绿。
- `bash docs/testing/round60/run-checks.sh` → `共 11 项：PASS=11 FAIL=0 SKIP=0`、`EXIT=0`（D4b 实跑，日志 `target/round60/audit/run-checks-d4b.log`）。其 11 项含 01–07 用例计数断言（`mir-client@21/@19/@12/@30/…`）、08 fmt、09 clippy、10 三接线锚点、11 M34 素材（缺失记 SKIP 非 PASS）。
- 注：门禁 12 项 ⊃ run-checks 11 项（门禁另含 workspace 全量、协议生成器、文档链接、TextKey、双棘轮、老资源格式、图标、PostgreSQL）。两套数字并存属设计，不是矛盾。

## F. M-1–M-7 落点：全部在位

| 项 | 要求 | 落点（已核） |
|---|---|---|
| M-1 | F6 诊断改写，禁把「丢失」写成 PASS | `docs/testing/round60/README.md:60-63` §4「F6 更正说明（旧诊断作废）」：旧「ring 截断→整表重放」标为生产不可达，真实缺陷＝第 17+ 事件被永久静默丢弃 |
| M-2 | 确定红→绿且断言 3 改为新增 **1** | `docs/process/round60-plan.md:38`；证据 `target/round60/d2/attempt-1/04-red-before.log`(101) → `05-green-after.log`(0)；`projectile.rs` 用例断言 `consumed==17`、`total==17` 而非 0 |
| M-3 | 降级用例环境无关、fake sink 进 crate 内单测 | `app.rs:209` `_with` 注入闭包；测试在 `--bin mir-client`（21 passed）；`docs/client.md:1279-1304` §5.27 A/B 记录注入与「不碰声卡」 |
| M-5 | 计数器范式对齐 `next_spell_id`，复位检测不只看 total | `net.rs:2772` 字段 + `:7265` 自增；`projectile.rs:473-481` 复位判定改为「空表/无 dropped 的缩短」；两处重建归零测试（net 30 / projectile 12 全过） |
| M-6 | 0-tests 守卫与 SKIP/STRICT 写死在脚本 | `docs/testing/round60/run-checks.sh:53-63` 每项带期望计数（`@21` 等）、`:293`/`:315` `EXIT=$((miss == 0 ? 0 : 1))` 计数不符即红；11 项「缺失记 SKIP 非 PASS」 |
| M-7 | 自哈希与门禁 HEAD 规则写死 | `commit-tree-hashes.txt` 头 10 行：# 自哈希不入清单、`head=bb53321`、`gate_head=bb53321`（提交①而非 docs HEAD）、`gate_log_*` 三字段 |
| （M-4 旁证） | SKIP 语义 | `run-checks.sh` 第 11 项 + D1 `SKIP.log` 语义：缺失打印 SKIP 并记非 PASS |

未发现 M 项缺失 → **无因 M 缺位的必改**。

## G. 假绿 / 红线：通过（含 1 处命令口径澄清）

- 禁称扫描：`git diff 3a9a104..HEAD | grep -E '^\+.*(125 全链|R60 是否|R60 完整|音频完整|无重复|真实可听已证)'` → 12 处命中，**全部为否定语境**（「不等于」「禁写」「禁称」「不宣称」「禁项」「口径：通道打通+去重+PARTIAL」），无一处肯定式宣称。
- `#[ignore]`：`git diff 3a9a104..HEAD -- crates | grep -c '#\[ignore\]'` → **1**；但 `git diff -U0 … | grep -E '^\+.*#\[ignore\]'` 的唯一命中是 `app.rs` 测试模块的**文档注释**「A missing asset prints `SKIP…` and returns (never `#[ignore]`, never…)」→ **新增 ignore 属性 0 个**，`cargo test` 三组均 `0 ignored`。属「字面量出现在注释里」的 grep 误报，非红线。
- 不变量：`EFFECT_EVENT_LIMIT == 16`、`ProjectileEvent` 零改动（见 D）。
- i18n 棘轮（D4b 实跑，UTF-8 环境）：
  - 门禁同款 `--no-test --player-visible --strict --fail-above 0` → **0 处 / 上限 0 / exit 0**（与门禁 OK 一致，`tools/dev/ci.ps1:136` 即此命令）；
  - `--player-visible --strict --fail-above 70` → 66 处 / 上限 70 / **exit 0**；
  - 任务书原样命令（缺 `--player-visible`）`--no-test --strict --fail-above 0` → **356 处 / exit 1**：差异纯由「把日志类非玩家可见行也计入」造成，**非代码回归**；门禁用的是带 `--player-visible` 的版本。建议后续把该口径写进 README，避免终审复跑误判。
- 无 `#[allow]` 新增、未删既有测试（diff 仅 +593/−12 且删行集中在旧游标分支，`net.rs` 纯增 76 行）。

## H. 卫生：通过

- 三份产物均 < 1 MiB（本清单 JSON ≈ 5 KB、本文 ≈ 9 KB、`summary.log` ≈ 3 KB）。
- `git ls-files | grep -E '(^|/)(target|resource|game-assets)/|\.env$'` → 无输出：`target/`、`resource/`、`game-assets/`、`.env` 均未入库（`.gitignore:27`）。
- `py -3 tools/docs/check_links.py` → `[OK] 1746 …`，**exit 0**；`git diff --check` → 无输出，**exit 0**。
- 工作树仅 2 个历史未跟踪大冻结 JSON（见 B），本轮只新增 `docs/testing/round60/{final-manifest.json,round60-final-audit.md}` 与 `target/round60/audit/**`（后者未跟踪）。

## 必改（2 条，均**非阻断**、文档级，建议 R61 顺手收口）

1. `crates/mir-client/src/app.rs:249`（diff 内新增行）测试文档注释含字面量 `#[ignore]`，使「新增 ignore 断言 = 0」的朴素 grep 变 1。建议改写为「never marked ignored」之类，保持棘轮 grep 长期为 0。（不影响编译/测试，`0 ignored` 已实测。）
2. `docs/client.md:1304` 把 100 ms 同文件频控的偏差台账写成「P-6/R-5」，而同页 §5.27 PARTIAL 表 `P-4` 才是频控（`P-6`＝脚本运行环境）。交叉引用错位，建议改为 `P-4/R-5`。

（另：任务书 G 段 i18n 命令缺 `--player-visible`，复跑会 exit 1；这是**校验命令口径**问题，不是仓库必改。已在本报告 G 段与 manifest `checks[]` 双记。）

## PARTIAL 台账（P-1..P-6，全部为已登记项，非新增风险）

| # | 内容 | D4b 复核 |
|---|---|---|
| P-1 | **可听性 PARTIAL**：headless 无音频设备，只证 cue→解码→混音器与降级不 panic；`apply_snapshot` 无 Live 窗口级集成测试，未真人手听 | 属实（C 段）；不得写成「真实可听已证」 |
| P-2 | `from_repo_root` 找不到 M34 素材（既有 `audio.rs`/`main.rs` 根路径行为），换机/改 CWD 时按 `SKIP: missing M34-*.mna` 记非 PASS | 本轮未改该行为，如实登记 |
| P-3 | ring 16 仍丢最旧（R4a 不修）：只保证已消费不重放、第 17+ 不被游标吞 | 与 `consume` 实现一致；不得写成「无重复/不丢」 |
| P-4 | 同文件 100 ms 频控：同帧多 cue 只响一次，≠ Crystal 每发都响（批准保留的降噪） | 属设计取舍；drain 数 vs played 数已分开 |
| P-5 | 素材来源：`game-assets/audio/sfx/M34-{1,2}.mna` 由 R59 从 `resource/community/gqf2008-crystal/Sound/M34-{1,2}.wav` 导入；两目录均 gitignore，证据只在 `target/` | 未入库已核（H 段） |
| P-6 | 脚本环境：attempt-1 首跑 01/02 因系统 TEMP 不可写 FAIL，脚本改指 `target/tmp` 后 11/11 PASS；首跑日志保留不删不盖 | D4b 实跑已带 `TMP/TEMP=target/tmp`，11/11 PASS |

## non-claims（本报告**不**主张）

- 不主张「125 全链」「R60 完整」「音频完整」「无重复」「真实可听已证」「G1–G4 全绿」。
- 不主张可听性已验证、不主张 ring 16 丢失已消除、不主张有 Live 窗口级集成测试。
- 不主张 P-2 根路径问题已修、不主张跨机可复现门禁（TMP 依赖见 P-6）。
- 不主张门禁可重跑得到相同 sha（时间戳/环境差异），仅主张**本次记录**与仓库状态自洽。
- 不评价 M-4 素材数量完整性（本轮只核 M-1/2/3/5/6/7）。

## 复核命令摘要（完整见 `target/round60/audit/summary.log`）

`wc -c` / `sha256sum` 门禁日志、`tail -20`、`grep -c "^OK "`、`cat *.meta.txt`、`grep -rn "round60\|r60_"`、`git show --stat`×2、`git diff bb53321..e053905 --stat -- crates`、`git status --short`、树哈希逐条复算（13/13）、`cargo test -p mir-client --bin mir-client`、`--lib ui::projectile`、`--lib net::`、`bash docs/testing/round60/run-checks.sh`（11/11）、`git diff 3a9a104..HEAD` 三项 grep、`py -3 tools/docs/check_links.py`、`git diff --check`、i18n 三口径。**全部 exit 0**，唯二非零：`--no-test --strict --fail-above 0`（缺 `--player-visible`，356 处，exit 1，口径问题）与 `#[ignore]` 字面量 grep（1，注释放，exit 0 但期望 0）。
