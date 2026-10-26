# 第 60 轮（R60）方案：客户端 125 音效真实通道 + 投射物事件去重 + 证据收口

> **版本**：rev2（2026-10-26）｜依据 [`round60-menxia-review.md`](round60-menxia-review.md)（sha256 `c923b90f7d8f1f73cbb8b3fb29c09fde8ed1f0652a89a4c5e51486e968e4b74b`，verdict PASS-WITH-CHANGES）按 M-1…M-7 修订；rev1（sha256 `eb81496b…`）正文见 git 历史。**核心更正**：F6 诊断（真实缺陷＝第 17+ 事件永久丢失，非 ring 截断导致整表重放）与 G2 断言 3（新增 **1** 而非 0）。
> 规划：中书省（gpt-6.1-sol）｜基线 HEAD `3a9a104`＝`origin/main`（工作树仅 2 个历史 freeze JSON 未跟踪，44.8MB/45.2MB，>1MiB 不入库）。
> 口径：**通道打通 + 去重 + PARTIAL 诚实登记**。禁称「125 音频完整」「无重复」「真实可听」。
> 依据：R59 台账 [README.md:52-62](../testing/round59/README.md) 与承接 [README.md:64-71](../testing/round59/README.md)（只做 P-2 与 G4 可听性）。

## §一 事实锚点（只读真读；Crystal 与 Rust 双列）

| # | 事实 | Crystal 锚点 | Rust 锚点 |
|---|---|---|---|
| F1 | 125 出生/命中音：`20000+GreatFireBall(34)*10+{1,2}` → `M34-1.wav`/`M34-2.wav` | `github/Crystal/Client/MirScenes/GameScene.cs:4697`（出生）、`:4709`（命中，仅在 `:4701` `missile.Target != null` 时挂 `Complete`；`:4707` 目标 `Dead` 直接 return） | `crates/mir-render/src/sound.rs:234`（`PROJECTILE_SOUND_BASE=34`）、`:236`/`:238`（20341/20342）、`:276` `projectile_sound`；`crates/mir-client/src/ui/projectile.rs:482-485`（spawn cue）、`:509-537`（hit cue） |
| F2 | cue 队列：UI 只把「文件名」交给 sink，播放端在外层 | 无对应类（Crystal `SoundManager` 直接播） | `crates/mir-client/src/ui/projectile.rs:290-341`（trait/`RecordingSoundSink`，上限 32 丢最旧）、`:371` `sounds` 字段、`:413-415` `take_sound_cues()`；`:422-427` `sync` |
| F3 | **R59 PARTIAL 根因**：`take_sound_cues()` 无生产消费者 → 队列只会涨、玩家听不到 | — | `crates/mir-client/src/ui/projectile.rs:54`（注释自承「接 `app.rs` 的真实播放不在 R59 写范围」）；`crates/mir-client/src/app.rs:2725`（`live.ui.sync` 后无 drain） |
| F4 | 播放基座已存在：`play(file, now_ms)` = 开关/增益/输出端（`!has_output()`）→ 频控 → 素材 resolve（`.mna` 优先）→ 解码（`mir_asset::AudioClip::open` + rodio `Decoder`）→ `mixer.add(amplify(gain))`；`set_test_sink` 为 `#[cfg(test)]` | `SoundManager.PlaySound` 无频控（节流靠步帧/延迟队列） | `crates/mir-client/src/audio.rs:592-625` `play`（`:594` 先判 `!has_output()`）、`:196-215` `resolve`、`:240-253` `decoder_for`、`:384-386` `set_test_sink`、`:389-397` `has_output`（测试 sink 也算有输出）、`:119-128` 默认根 |
| F5 | 素材真实存在：`game-assets/audio/sfx/{M34-1,M34-2}.mna`（该目录 105 个 `.mna`；R59 `audio verify files=106`） | 源 wav：`resource/community/gqf2008-crystal/Sound/M34-{1,2}.wav` | 本轮 `ls game-assets/audio/sfx/M34-*` 实测 3 个（含 `M34-0`）；`game-assets/` 被 `.gitignore:27` 忽略 → 证据只落 `target/` |
| F6 | **125 事件无 ID/时间戳**；ring 容量 16、超限 `remove(0)` 丢最旧（`len` 只增不减）；`consume` 的 `if events.len() < self.consumed { self.consumed = 0 }` 在**生产不可达**（`while` 终态恒 `consumed == len`，`len` 由 `push_projectile_event` 封顶 16）——原「ring 截断→整表重放」诊断**作废**；**真实缺陷＝第 17 个及之后的 125 事件被永久静默丢弃**（`consumed` 停在 16、`views` 不再增长，该会话后续投射物表现消失）。R59 台账 P-2「可能漏事件」比原判准确 | Crystal 无事件 ring（`Map.cs:2437-2448` 就地广播，`:5826` 每跳发一次，不排除施法者） | `crates/mir-client/src/net.rs:1503-1511` `ProjectileEvent`、`:166` `EFFECT_EVENT_LIMIT=16`、`:7246-7260` `push_projectile_event`；`crates/mir-client/src/ui/projectile.rs:446-462` `consume`（`:452-461` 复位分支 + `while`）；反证 `docs/testing/round59/README.md:52-62` P-2 |
| F7 | 既有「单调序号」范式可复用（不改协议线格式） | `S.ObjectProjectile{Spell,Source,Destination}` 无线 ID（`Shared/ServerPackets.cs:3560`、`Shared/Enums.cs:1561`） | `crates/mir-client/src/net.rs:2767` `next_spell_id` 字段、`:2882` `impl Default for WorldSnapshot` 与 `:2901-2903` 归零、`:7277-7278` `push_spell_event` 自增；同理可加 `projectile_events_total` |
| F8 | 主循环位置：新快照到达 → `live.ui.sync(&live.snapshot)`；`LiveState` 同时持 `sound`（唯一播放器）与 `ui` | — | `crates/mir-client/src/app.rs:76-116`（`LiveState`，`sound` 在 `:84`）、`:178-191`（既有 sound 事件 drain，可作范式）、`:2725`（sync 点）、`:3369`（`mod tests`，`cargo test -p mir-client --bin mir-client` 可跑） |
| F9 | `UiState.projectiles` 是 `pub` 字段，可直接 drain；现有 `projectiles()` 只给 `&` | — | `crates/mir-client/src/ui/mod.rs:1114` 字段、`:1266-1268` 只读 getter、`:1564` 每帧 `sync` |
| F10 | 树哈希清单未随 HEAD 重签（R59 尚书已记） | — | `docs/process/round59-shangshu-acceptance.md:20`（`01-tree-hashes.txt` 对 `edaa5cd` 16/16、对 HEAD 14/16）、`docs/testing/round59/final-manifest.json:302`/`:314`（建议冻结清单自身 sha + 门禁 HEAD 头） |
| F11 | 判定基线：CI 无音频设备（`rodio` 打开失败即 `backend=None`）；但真机有声卡时 `!has_output()` 不成立，「无设备 `play=false`」**不是环境无关断言** | — | `crates/mir-client/src/audio.rs:12-15`、`:344-358` 设备降级、`:592-596` `play` 前置判定；`tools/dev/ci.ps1` 12 项 |

## §二 目标（可证伪断言 + 验收命令 + 证据路径）

- **G1 125 音效真实通道**（对应 F1–F5、F8、F9）
  - 断言 1：`crates/mir-client/src/ui/mod.rs` 新增 `pub fn take_projectile_sound_cues(&mut self) -> Vec<&'static str>`，转调 `projectiles.take_sound_cues()`；`app.rs:2725` 之后**同帧** drain 并逐个 `sound.play(file, now_millis_u64())`（`grep -n "take_projectile_sound_cues" crates/mir-client/src/{ui/mod.rs,app.rs}` 各 ≥1 命中）。
  - 断言 2：drain 函数是自由函数 `fn play_projectile_cues(sound: &mut SoundPlayer, cues: Vec<&'static str>, now_ms: u64) -> usize`（可单测、不进 `LiveState`）；行为断言：drain 一次取走 N 条并调用 `play` N 次、同一批第二次调用返回 0（不重复播）。
  - 断言 3（素材）：`SoundLibrary::from_repo_root` 下 `contains("M34-1.wav")`/`contains("M34-2.wav")` 为真，且 `resolve` 结果后缀为 `.mna`（`audio.rs:196-215` `resolve`，防 legacy wav 顶替）、`mir_asset::AudioClip::open` + rodio `Decoder::new`（`audio.rs:240-253` `decoder_for`）均成功；缺素材打印显式 `SKIP: missing M34-1.mna`、exit 非 0 或 SKIP 标记，由 `run-checks.sh` 记 **PARTIAL** 单列；禁 `#[ignore]`、禁以 `M34-0` 顶替、SKIP≠PASS。
  - 断言 4（降级，环境无关）：`enabled=false` 或 `volume*master=0`（命中 `audio.rs:594` `play` 的 `gain<=0`）→ `play` 返回 `false` 且不 panic；fake sink（`audio.rs:384-386` `set_test_sink`）→ 解码成功后 `play` 返回 `true`、计数 +1。因 `set_test_sink` 为 `#[cfg(test)]`，集成测试看不到，该用例放 `src/audio.rs`/`src/app.rs` 的 `#[cfg(test)] mod tests`。
  - 验收：`cargo test -p mir-client --test round60_audio_channel`（≥3 passed）+ `cargo test -p mir-client --bin mir-client`（含 `audio::`/`app::`，0 tests 判 FAIL；不可只跑 `-- app::`）；证据 `target/round60/d1/attempt-1/`。
  - 边界：机器只能证「cue→解码→混音器/降级」；**真实可听**需本机有音频设备手听一次（Lead，可选）。缺手听 → G1 记 **PARTIAL（可听性）**，通道部分为 PASS。

- **G2 125 事件去重（绝对序号游标；第 17+ 事件不再丢失）**（对应 F6、F7）
  - 断言 1：`WorldSnapshot` 新增 `pub projectile_events_total: u64`（`net.rs:7246-7260` `push_projectile_event` 每次 `saturating_add(1)`；`net.rs:2882` `impl Default for WorldSnapshot` / `:2901-2903` 快照重建归零；范式 `next_spell_id` 见 `net.rs:2767`/`:2901-2903`/`:7277`）；**不动** `ProjectileEvent` 结构、**不动**协议线格式。
  - 断言 2：`ui/projectile.rs::consume` 结算：`dropped=total.saturating_sub(len)`、`start=consumed.saturating_sub(dropped)`、`dropped>consumed → consumed=dropped`；保留「快照整体重建 → 游标归零」旧语义（原 `len<consumed→0` 分支，触发条件须收紧为**空表/重建**；ring 满后 `consumed==total>len` 属正常态，若按字面条件归零会使第 18 事件把旧 view 全量重放，断言 3 第 2 步必红）；`EFFECT_EVENT_LIMIT` 仍 16（`net.rs:166`），`ProjectileEvent`（`net.rs:1503-1511`）零改动。
  - 断言 3：构造 `len=16, total=17, consumed=16`（第 1–17 事件 `destination` 可区分）→ `sync` **新增 1 个 view**，`views.len()==1`，其 payload 唯一等于第 17 事件、**不含** #1–#16 任一 payload；再 push 至 `total=18` → 新增 1。另加「快照重建」用例：空表重建后 `events_consumed()==0`，对齐 `crates/mir-client/tests/round59_projectile_render.rs:225-246` `cursor_resets_when_the_snapshot_event_table_is_replaced`，**不得改其断言值**。
  - 断言 4（确定红→绿，M-2）：上述「新增 1 view＝第 17 事件」——旧实现新增 **0**（红）、新实现 **1**（绿）；删 `projectile.rs` 修复行（`dropped` 跳跃/`start` 计算）复核转红后还原，`git diff` + 两次原始输出入 `target/round60/d2/attempt-1/`；**禁止**以「旧实现会生成 16 个 view」为前提写用例（会假红）。
  - 断言 5：`cargo test -p mir-client --test round59_projectile_render` 仍 **16 passed**；`round58_projectile_wire` 仍 7 passed。
  - 验收：`cargo test -p mir-client --test round60_projectile_dedupe`（≥5 passed，含断言 3/4）；证据 `target/round60/d2/attempt-1/`。

- **G3 文档与树哈希收口**（对应 F10）
  - 断言 1：新建 `docs/testing/round60/README.md`，`:3` 写基线 HEAD `3a9a104` 与交付 HEAD（两段式），含 §1 矩阵、§2 证据索引、§3 PARTIAL/偏差台账、§4 R61 承接、§5 复跑命令、§6 改动说明。
  - 断言 2：`docs/testing/round59/README.md:64-71` §4 承接表逐项改判（1 事件 ID/序列号、5 素材/音效收口→R60 已交付；2/3/4/6 转 R61），不删原文、只加状态列；`:52-62` P-2 状态列写「R60 已修『丢失』？——否：ring 16 仍丢最旧（只保证已消费事件不重放、第 17+ 事件不再被游标吞掉）；漏事件风险仍在」。
  - 断言 3：`docs/client.md` 新增 §5.27「125 音效真实通道与事件去重（R60）」，并回填 §5.26 的 G4 PARTIAL 口径；`docs/world/magic.md` FireBounce 行同步；`docs/roadmap.md` R60 行。
  - 断言 4：`docs/testing/round60/01-tree-hashes.txt` 由 `git show <commit>:<path> | sha256sum` 生成，首部含 `head=<提交① HEAD>`、`gate_head=<门禁 HEAD，同提交①>`、`gate_log_sha256=<attempt-1.log>`；覆盖 `git diff 提交①..提交②` 全部 `crates/**`+`docs/**`；**清单自身 sha 只入** `final-manifest.json`（破自哈希循环）；生成命令入 README §5 可复跑。
  - 断言 5：`docs/testing/round60/run-checks.sh` ≥9 项；每条测试先 `cargo test … -- --list` 校验发现数，执行期再 grep `running 0 tests` / `0 passed` 判 FAIL；`--list` 零执行零写入；`SKIP:`/`PARTIAL:` 字面量被脚本识别，**SKIP≠PASS**。
  - 验收：`py -3 tools/docs/check_links.py` exit 0；`bash docs/testing/round60/run-checks.sh --list` exit 0；`git diff --check` exit 0；证据 `target/round60/d3/attempt-1/`。素材声明：真实素材 + 本地生成物（`game-assets/` 不入库）。

- **G4 唯一全量门禁不回归**
  - 断言：`pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale` 在**代码提交① HEAD** 上 attempt-1 一次、exit 0、12/12；日志 `target/round60/gate/attempt-1.log` 的字节数与 sha256 原文入 `docs/testing/round60/README.md §2`。

## §三 子任务 DAG（写范围互不重叠；deepseek 只编码，gpt 规划/评审/文档）

| # | 角色 | 任务 | 写范围（唯一 owner） | 依赖 |
|---|---|---|---|---|
| D1 | 兵部/deepseek | G1 音效真实通道 + 降级/素材用例 | `crates/mir-client/src/ui/mod.rs`（仅新增 accessor）、`crates/mir-client/src/app.rs`（drain 自由函数 + 调用点 + `mod tests` 用例）、`crates/mir-client/src/audio.rs`（仅 `#[cfg(test)] mod tests` 内降级/fake-sink 用例）、新建 `crates/mir-client/tests/round60_audio_channel.rs` | 无（与 D2 并行） |
| D2 | 工部/deepseek | G2 绝对序号游标 + 去重用例 | `crates/mir-client/src/net.rs`（`projectile_events_total` + push 自增 + 归零）、`crates/mir-client/src/ui/projectile.rs`（`consume` 结算 + 内联 `#[cfg(test)] mod tests` ring 溢用例）、新建 `crates/mir-client/tests/round60_projectile_dedupe.rs` | 无（与 D1 并行） |
| D3 | 礼部/gpt | G3 文档/树哈希/验收脚本 | `docs/client.md`、`docs/world/magic.md`、`docs/roadmap.md`、`docs/testing/round59/README.md`（仅 §4 与 P-2 状态列）、新建 `docs/testing/round60/`（README/run-checks.sh/01-tree-hashes.txt/final-manifest.json 由 D4 写 manifest） | D1,D2 + 门禁 attempt-1 结果 |
| D4 | 刑部/gpt | 终审（假绿扫描、锚点抽核、PARTIAL 边界） | 只读 crates；仅写新建 `docs/testing/round60/round60-final-audit.md`、`docs/testing/round60/final-manifest.json` | D3 |
| D5 | 尚书/gpt + Lead | 派发、登记、唯一门禁、两次提交（`git add/commit` 仅 Lead） | 无源码写范围；`target/round60/` 证据目录 | D4 |

串行：`D1∥D2 →（Lead 提交①）→ 门禁 → D3 → D4 →（Lead 提交②）`。D1/D2 文件无交集；F6 改写与 G2 断言 3 随派发件同批下发，D2 不得按旧 F6 写用例。

## §四 验收命令与门禁（唯一一次全量；两次提交）

```bash
# D1（G1）
cargo test -p mir-client --test round60_audio_channel
cargo test -p mir-client --bin mir-client        # 含 audio::（fake sink/降级）与 app::；不可只 -- app::
# D2（G2）
cargo test -p mir-client --test round60_projectile_dedupe
cargo test -p mir-client --test round59_projectile_render      # 回归：须 16 passed
cargo test -p mir-client --test round58_projectile_wire        # 回归：须 7 passed
cargo test -p mir-render --lib sound                           # 回归：须 9 passed
# 静态与文档
cargo fmt --all --check
cargo clippy -p mir-client -p mir-render --all-targets -- -D warnings
py -3 tools/docs/check_links.py
git diff --check
bash docs/testing/round60/run-checks.sh          # ≥9 项，0 tests / SKIP 均不得记 PASS
# 唯一一次全量门禁（Lead；仅代码提交 HEAD 上跑一次，attempt-1 为唯一有效门禁）
pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale
```

- **提交①**：D1+D2 代码与测试（`crates/**`）；提交后跑门禁，保留原始 exit + `target/round60/gate/attempt-1.log` 的**字节数与 sha256**；`gate_head` 记提交① HEAD。
- **提交②**：D3+D4 文档/证据（`docs/**`），`01-tree-hashes.txt` 头部写提交① HEAD 与门禁日志 sha；清单自身 sha 写 `final-manifest.json`。
- 禁第三人复跑门禁顶替 attempt-1；门禁失败只许修因后重跑并在 README 记 `attempt-2` 及首败原因。

## §五 风险与真实降级（PARTIAL 边界；禁把缺素材 SKIP 当 PASS）

| # | 风险 | 真实降级 |
|---|---|---|
| R1 | 真机有声卡时 `!has_output()` 不成立，`play` 走真实后端 | 降级断言改用 `enabled=false`/`gain<=0`（环境无关）；fake sink 计数走 crate 内单测；真实可听记 **PARTIAL（可听性）**，不写「已可听」 |
| R2 | `game-assets/` 不入库（`.gitignore:27`），换机可能无 `M34-1/2.mna` | 用例打印 `SKIP:` 并单列，G1 降 PARTIAL；断言 `.mna` 后缀 + `AudioClip::open` + `Decoder::new` 成功；禁以 `M34-0` 或其它技能音顶替 |
| R3 | 100ms 频控吞同文件多发（同一技能连发只响一次，Crystal 每发都响） | 保留既有降噪设计（门下裁决 3 批准保留）并登记台账；README §3 单列「同文件 100ms 内只响一次 ≠ Crystal 每发都响」 |
| R4a | ring 16 丢最旧（**已知、不修**）：客户端卡顿 >16 事件时最旧事件不可恢复 | G2 不保证「不丢」；只保证已消费事件不重放、第 17+ 事件不再被游标吞掉；R61 再评估扩容 |
| R4b | 第 17+ 事件丢失（原 F6 真实缺陷：`consumed` 停在 16、后续 125 表现消失） | **本轮已修**（`projectile_events_total` 绝对序号游标）；README §3 与 R59 P-2 分两行登记 |
| R5 | 去重改 `consume` 可能破坏 R59 用例 | 必须保留「事件表整体重建 → 游标归零」旧语义；`round59_projectile_render` 16 passed 为硬门，改测试须在 D2 证据里给 diff 与理由 |
| R6 | 树哈希自哈希循环 / 文档批在门禁之后 | 清单自身 sha 只入 `final-manifest.json`；头部记「门禁 HEAD」而非 docs 批 HEAD；两次提交切分固定 |
| R7 | 行号漂移（R59 M-1 前例） | 所有引用写「文件:行号 + 符号名」双锚，刑部按符号存在性抽核，不单信行号 |

## §六 不做清单（本轮明确不做）

1. Hero/真 socket/攻城/天气/行会（R59 §4-6 与 D 类，不顺延）。
2. `Missile`/`Effect` 类体系、光照、逐帧重算 `Duration/Count`（P-3）、`magic.wzl` 逐帧 `draw offset`（P-4）、命中完整生命周期状态机。
3. 不改协议线格式、不给 125 加线上 ID；`EFFECT_EVENT_LIMIT` 保持 16，不扩容。
4. 不做 `source+destination+spell+时间窗` 启发式去重（会吞合法同帧多发，且 Crystal 无此逻辑）。
5. 零服务端改动（125 生产者 R57 已交付，`crates/mir-server/**` 本轮不动）。
6. 不补 `M34-0`、不做其它技能音效、不重构音频后端（rodio/频控/音量沿用）。
7. 不新建 `round60` 之外的目录结构，不动 `target/` 之外的未跟踪大 JSON。

## §七 门下裁决结论（review §二，rev2 已落定）

1. **去重口径：修改后批准。** 采用客户端绝对序号游标（不改结构/协议）；F6 更正为「第 17+ 事件永久丢失」，不再以整表重放为修复目标。
2. **可听性判定：修改后批准。** PARTIAL 口径维持；G1 降级用例改环境无关（`enabled=false`/`gain<=0`），fake sink 用例搬进 crate 内单测。
3. **频控旁路：批准（保留频控）。** 125 cue 保留 100ms 频控；README §3 单列与 Crystal「每发都响」的差异；drain 计数区分「取走 cue 数」与「`play` 返回 true 数」，频控吞掉的不计「已播」。
4. **两次提交与门禁位置：批准。** 门禁跑提交①后、提交②前；`gate_head` 记提交① HEAD（非 docs 批 HEAD）。
5. **树哈希冻结形式：修改后批准。** 清单自身 sha 只入 `final-manifest.json`；生成命令入 README §5；清单覆盖 `git diff 提交①..提交②` 全部 `crates/**`+`docs/**`。
