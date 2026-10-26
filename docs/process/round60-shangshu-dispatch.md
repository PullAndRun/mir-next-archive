# 第 60 轮（R60）尚书省派发件

> 派发：尚书省（shangshu，gpt-6.1-sol）｜方案 rev2 `docs/process/round60-plan.md`（119 行/19,345B/sha256 `e8e3f29a7c791c61794603a8d5b2f9374687c13c985fcf61c96f279c670b8139`）｜
> 评审 `docs/process/round60-menxia-review.md`（101 行/14,627B/sha256 `c923b90f7d8f1f73cbb8b3fb29c09fde8ed1f0652a89a4c5e51486e968e4b74b`，**PASS-WITH-CHANGES**，M-1…M-7 + §七 5 裁决已逐条落入本件）｜
> 基线 HEAD `3a9a104`＝`origin/main`｜口径：**通道打通 + 去重 + PARTIAL 诚实登记**。禁称「125 音频完整」「无重复」「真实可听」。

## §一 边界与串行

- 固定串行链：`D1∥D2 →（Lead 提交①）→ 唯一门禁 → D3 → D4 →（Lead 提交②）`；D1/D2 写范围零交集，可并行编辑、串行唤醒。
- 写范围唯一 owner（越界即封驳；需新增文件先回报尚书出改派件，执行者不得自行扩围）：

| 单 | 部门/模型 | 写范围（唯一 owner） |
|---|---|---|
| D1 | 兵部/deepseek-flash | `crates/mir-client/src/ui/mod.rs`（仅新增 accessor）、`crates/mir-client/src/app.rs`（drain 自由函数 + 调用点 + `#[cfg(test)] mod tests`） |
| D2 | 工部/deepseek-flash | `crates/mir-client/src/net.rs`（`projectile_events_total` + 内联测试）、`crates/mir-client/src/ui/projectile.rs`（`consume` + 内联测试） |
| D3 | 礼部/gpt-6.1-sol | `docs/client.md`、`docs/roadmap.md`、`docs/testing/round60/README.md`（新建）、`docs/testing/round60/run-checks.sh`（新建）、`docs/testing/round60/01-tree-hashes.txt`（新建）、`docs/testing/round59/README.md`（仅 §4 承接表与 P-2 状态列，不删原文） |
| D4 | 刑部/gpt-6.1-sol | 只读 `crates/**`；仅写 `docs/testing/round60/final-manifest.json`、`docs/testing/round60/round60-final-audit.md`、`target/round60/audit/summary.log` |
| D5 | 尚书/gpt-6.1-sol | 本派发件 + 看板登记；不写源码、不跑门禁、不 git |

- 路由：D1/D2 由 Lead `spawn_teammate` 启动（provider 继承 runtime，不手填 `sol-relay`）；D3/D4 由 Lead `subagent` 显式 `provider:"sol-relay", model:"gpt-6.1-sol"`，回执须写实际 provider/model。
- Lead 只做：spawn D1/D2 → 提交① → 唯一门禁 → spawn D3/D4 → 提交② → 只读核对看板；`git add/commit` 仅 Lead。尚书不跑 cargo/ci、不 commit、不改 `crates/**`。
- 门禁 attempt-1 为唯一有效门禁，禁第三人复跑顶替；失败只许修因重跑并记 `attempt-2` 与首败原因。

## §二 逐子单契约（写范围 / 验收命令 / 证据 / 禁项）

证据一律落 `target/round60/<单>/attempt-<n>/`（不入库）；每条命令记**原始 exit**；`--list` 发现数为 0 判 FAIL。

**D1（G1）兵部/deepseek-flash**

- 范围：`ui/mod.rs` 加 `pub fn take_projectile_sound_cues(&mut self) -> Vec<&'static str>`（仅转调 `self.projectiles.take_sound_cues()`）；`app.rs` 加自由函数 `fn play_projectile_cues(sound: &mut SoundPlayer, cues: Vec<&'static str>, now_ms: u64) -> usize`，在 `crates/mir-client/src/app.rs:2725` 的 `live.ui.sync(&live.snapshot)` 之后同帧 drain 并逐个 `sound.play(file, now_millis_u64())`。
- 验收：`grep -n "take_projectile_sound_cues" crates/mir-client/src/ui/mod.rs crates/mir-client/src/app.rs`（各 ≥1）；`cargo test -p mir-client --bin mir-client -- --list`（0 tests 判 FAIL）；`cargo test -p mir-client --bin mir-client`（行为断言：一次取 N 条并 `play` N 次、同批第二次返回 0）；`cargo test -p mir-client --lib audio`（既有降级/频控/fake-sink 用例仍绿）。
- 素材断言（写在 `app.rs` 测试内）：`SoundLibrary::from_repo_root` 的 `contains("M34-1.wav")`/`contains("M34-2.wav")` 为真、`resolve(...)` 后缀为 `.mna`、`mir_asset::AudioClip::open` 成功；缺素材打印 `SKIP: missing M34-1.mna` 由脚本记 PARTIAL（SKIP≠PASS，禁 `#[ignore]`、禁以 `M34-0` 顶替）。
- **禁项**：改 `crates/mir-client/src/audio.rs` 生产逻辑、改 `crates/mir-client/src/net.rs`、新建 `crates/mir-client/tests/round60_audio_channel.rs`（本件已收窄写范围，见 §五 R-1）。

**D2（G2）工部/deepseek-flash**

- 范围：`net.rs` 的 `WorldSnapshot` 加 `pub projectile_events_total: u64`，`push_projectile_event` 每次 `saturating_add(1)`，`impl Default for WorldSnapshot`（`net.rs:2882`）与快照重建（`:2901-2903`）归零；`ui/projectile.rs::consume` 按 M-5 参考实现结算：`dropped=total.saturating_sub(len)`、`start=consumed.saturating_sub(dropped)`、`dropped>consumed → consumed=dropped`，**保留** `len<consumed → consumed=0` 旧语义但触发条件收紧为**空表/快照整体重建**；ring 满后 `consumed==total>len` 属正常态，**禁止**照抄 M-5 字面组合（M-5 low 字面以 `len<consumed` 为复位条件，但 ring 满后 `consumed==total(17)>len(16)` 并非复位；D2 必须写成「`len==0 && consumed>0`（重建）或旧游标 > 新表长且 `dropped==0`」这类真重建条件，绝不能在 `consumed==total>len` 时复位，否则第 18 事件会重放旧 view，M-1 断言 3 第 2 步必红）。
- 验收：`cargo test -p mir-client --lib ui::projectile -- --list`（0 tests 判 FAIL）；`cargo test -p mir-client --lib ui::projectile`（含：`len=16,total=17,consumed=16` → 新增 **1** view 且 payload 唯一等于第 17 事件、不含 #1–#16；push 至 18 → 新增 1；空表重建 → `events_consumed()==0`）；`cargo test -p mir-client --lib net::`（自增/归零）；`cargo test -p mir-client --test round59_projectile_render`（须 16 passed，禁改其断言值）；`cargo test -p mir-client --test round58_projectile_wire`（须 7 passed）；`cargo test -p mir-render --lib sound`（须 9 passed）。
- 红→绿证据：删 `ui/projectile.rs` 的 `dropped` 跳跃/`start` 计算修复行复核转红再还原；`git diff` + 两次原始输出入 `target/round60/d2/attempt-1/`。
- **禁项**：改 `ProjectileEvent`（`net.rs:1503-1511`）、改协议线格式、改 `EFFECT_EVENT_LIMIT=16`、以「旧实现重放 16 帧」为前提写用例（假红，A8）。

**D3（G3）礼部/gpt-6.1-sol**

- 范围：`docs/testing/round60/README.md`（新建，含 §1 矩阵、§2 证据索引、§3 PARTIAL/偏差台账、§4 R61 承接、§5 复跑命令、§6 改动说明）；`docs/testing/round60/run-checks.sh`（新建，≥9 项，支持 `--list` 零执行零写入，每项先 `cargo test … -- --list` 校验发现数、执行期 grep `running 0 tests`/`0 passed` 判 FAIL，识别 `SKIP:`/`PARTIAL:` 字面量）；`docs/testing/round60/01-tree-hashes.txt`（新建，头部 `head=<提交① HEAD>`、`gate_head=<门禁 HEAD，同提交①>`、`gate_log_sha256=<target/round60/gate/attempt-1.log>`，覆盖 `git diff 提交①..提交②` 全部 `crates/**`+`docs/**`，生成命令 `git show <commit>:<path> | sha256sum` 写入 README §5）；`docs/client.md` 新增 §5.27 并回填 §5.26 G4 PARTIAL；`docs/testing/round59/README.md` §4 承接表逐项改判、P-2 状态列写「R60 已修『丢失』？——否：ring 16 仍丢最旧」；`docs/roadmap.md` R60 行。
- 验收：`py -3 tools/docs/check_links.py`（exit 0）；`bash docs/testing/round60/run-checks.sh --list`（exit 0）；`git diff --check`（exit 0）。
- **禁项**：写「125 音频完整/无重复/真实可听」；改 `crates/**`；把 SKIP 记 PASS；清单自身 sha 入清单（只入 D4 的 `final-manifest.json`）。

**D4（G4 终审）刑部/gpt-6.1-sol**

- 范围：只读；写 `docs/testing/round60/final-manifest.json`（含 `01-tree-hashes.txt` 自身 sha256）、`docs/testing/round60/round60-final-audit.md`、`target/round60/audit/summary.log`。
- 验收：假绿扫描（`--list` 计数、原始 exit、日志 sha、`running 0 tests`）、M-1…M-7 落点逐条抽核（符号存在性优先，不单信行号）、PARTIAL 边界（可听性/素材/ring 16 丢事件）核对；`py -3 tools/docs/check_links.py` 复跑 exit 0。
- **禁项**：改受测文件、补跑 ignored/门禁、代修、把 PARTIAL 写成 PASS。

**D5（尚书协调）**：复用已建子单 `JJC-20261026-007-sub-fb617792`（Doing），不重复建单。

## §三 M-1…M-7 落点表

| M 项 | 结论 | 落点 |
|---|---|---|
| M-1 | 去重口径改写 | D2 契约：`len=16,total=17,consumed=16` → 新增 **1** view（非 0）；空表重建归零；F6 改「第 17+ 事件永久丢弃」，禁旧话术 |
| M-2 | 确定红→绿 | D2 验收红→绿复核 + `git diff`/两次输出入 `target/round60/d2/attempt-1/`；禁「旧实现重放 16 帧」前提 |
| M-3 | 降级用例环境无关 | D1 契约：`enabled=false`/`gain<=0` 用例 + fake-sink 计数走 crate 内单测（复用 `cargo test -p mir-client --lib audio`；本件收窄后不新增 `audio.rs` 测试，见 R-1） |
| M-4 | 素材 SKIP 单列 | D1 素材断言（`.mna` 后缀 + `AudioClip::open`）与 `SKIP: missing M34-1.mna`；D3 脚本识别 `SKIP:`/`PARTIAL:` |
| M-5 | 复位语义对齐 | D2 契约：计数器 `u64+saturating_add(1)` 对齐 `next_spell_id`；**保留**旧复位分支但收紧触发条件，禁止 ring 满时复位 |
| M-6 | 0-tests 守卫 | D3 `run-checks.sh` ≥9 项、`--list` 零写入、`running 0 tests`/`0 passed` 判 FAIL |
| M-7 | 冻结形式 | D3 清单头部三字段 + 生成命令入 README §5；D4 `final-manifest.json` 存清单自身 sha |

## §四 门禁与两次提交

- **提交①**（仅 Lead）：D1+D2 的 `crates/**`；提交信息记 R60 与「F6 更正为第 17+ 事件丢失」。
- **唯一门禁**（仅 Lead，提交① HEAD 上 attempt-1 一次）：`pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`，`*>` 完整捕获 → `target/round60/gate/attempt-1.log`；记原始 exit 码、字节数、sha256；`gate_head` = 提交① HEAD。`target/soak.lock` 存在时禁 `-KillStale`。
- **提交②**（仅 Lead）：D3+D4 的 `docs/**`；`01-tree-hashes.txt` 头写提交① HEAD 与门禁日志 sha；清单自身 sha 只入 `final-manifest.json`。
- 门禁前后必跑静态：`cargo fmt --all --check`、`cargo clippy -p mir-client -p mir-render --all-targets -- -D warnings`、`bash docs/testing/round60/run-checks.sh`；提交前 `git status --porcelain` 与证据 sha 对照；不 amend、不 force push。

## §五 黑名单与风险（PARTIAL 边界）

- 禁称「125 音频完整」「无重复」「真实可听」；只许「cue→解码→混音器路径已通；**可听性 PARTIAL**」。
- 禁 GM/传送/改 DB/伪造库存；禁强杀冒充正常退出、缺素材 `return Ok`、旧日志当本次证据；禁把 SKIP 当 PASS。
- R-1（本件偏差，须门下/Lead 知悉）：方案 §三 原给 D1/D2 的集成测试新文件（`tests/round60_audio_channel.rs`、`tests/round60_projectile_dedupe.rs`）**已从写范围移出**，改以 `app.rs`/`ui/projectile.rs`/`net.rs` 内联 `#[cfg(test)] mod tests` 承接，验收命令按真实存在的 target 改写（`--bin mir-client`、`--lib ui::projectile`、`--lib net::`）。若门下要求恢复集成测试文件，由派发件 rev2 改派，执行者不得自行扩围。
- R-2 可听性：真机有声卡时 `!has_output()` 不成立，真实可听需 Lead 手听一次；缺手听 G1 记 **PARTIAL（可听性）**。
- R-3 素材：`game-assets/` 被 `.gitignore:27` 忽略，换机可能无 `M34-1/2.mna` → SKIP + PARTIAL，禁伪造。
- R-4 ring 16 仍丢最旧（已知不修）：G2 只保证已消费事件不重放、第 17+ 事件不再被游标吞掉，不保证「不丢」；README §3 与 R59 P-2 分两行登记。
- R-5 频控：同文件 100ms 内只响一次 ≠ Crystal 每发都响；drain 计数区分「取走 cue 数」与「`play` 返回 true 数」。
- R-6 行号漂移：引用一律「文件:行号 + 符号名」双锚，刑部按符号存在性抽核。

## §六 SUB 映射与回报格式

| 单 | 部门 | SUB_ID | 状态 |
|---|---|---|---|
| D1 | 兵部 | `JJC-20261026-007-sub-5d0d9ee6` | Doing |
| D2 | 工部 | `JJC-20261026-007-sub-a611e708` | Doing |
| D3 | 礼部 | `JJC-20261026-007-sub-ed46468f` | Doing |
| D4 | 刑部 | `JJC-20261026-007-sub-2d097222` | Doing |
| D5 | 尚书 | `JJC-20261026-007-sub-fb617792` | Doing（复用） |

- 回填：六部用 `delegate-result <SUB_ID> "<结果>"` 回填，不 `state`、不代验收；gpt 子单同法回填结论。结果须含：原始 exit、`--list` 发现数、证据路径、`git rev-parse HEAD`、`git status --porcelain`、PARTIAL 边界、未决项。
- 尚书按 §二 验收并请求主单 `Doing → Review`；`Review → Done` 由门下 `confirm`；Lead 只在 confirm 生效后提交。
