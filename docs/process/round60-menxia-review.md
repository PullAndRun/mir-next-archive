# 第 60 轮（R60）门下省评审与裁决

- 主单：`JJC-20261026-007`「第60轮：客户端 125 音效真实通道与投射物事件去重」，评审时状态 Menxia。
- 被评审：`docs/process/round60-plan.md`（116 行，sha256 `eb81496b5296152ace9e9e3109581065093944661daba5f6fcb9989ffa9e4849`，七节）。
- 基线：HEAD `3a9a104e05db9f28dd5431e8bd1c6d04e9024406` ＝ `origin/main`（核对一致；工作树仅 2 个未跟踪大 JSON）。
- 边界：只读 `crates/`，未跑 cargo/ci，未 commit；本文件为唯一写入。
- **verdict：PASS-WITH-CHANGES** —— G1/G3/G4 通过；G2 方向正确但 **F6 诊断为误判**、断言 3 期望值与自身公式矛盾，必须按 M-1/M-2/M-5 改写后方可派发。

## 一 抽查锚点表（16 个：Crystal 4 / Rust 10 / 文档 2）

| # | 锚点（文件:行 + 符号） | 方案主张 | 实测 | 判定 |
|---|---|---|---|---|
| A1 | `github/Crystal/Client/MirScenes/GameScene.cs:4697` | 出生音 `20000+34*10+1` | 原文 `SoundManager.PlaySound(20000 + (ushort)Spell.GreatFireBall * 10 + 1)`，在 `CreateProjectile`（`:4699`）之前 | 命中 |
| A2 | `GameScene.cs:4701` | 仅 `missile.Target != null` 才挂 `Complete` | 原文 `if (missile.Target != null)` 包住 `missile.Complete +=` | 命中 |
| A3 | `GameScene.cs:4707`/`:4709` | 目标 `Dead` 直接 return；否则 570 特效 + `+2` 音 | `:4707 if (sender.Target.CurrentAction == MirAction.Dead) return;`；`:4708` Effect(570,10,600)；`:4709` PlaySound(+2) | 命中 |
| A4 | `github/Crystal/Shared/ServerPackets.cs:3560` + `Shared/Enums.cs:1561` | 125 包无线 ID | `:3560 public sealed class ObjectProjectile : Packet`；`Enums.cs:1561 ObjectProjectile,`；载荷仅 Spell/Source/Destination | 命中 |
| A5 | `crates/mir-client/src/net.rs:166` `EFFECT_EVENT_LIMIT` | 16 | `const EFFECT_EVENT_LIMIT: usize = 16;` | 命中 |
| A6 | `net.rs:7246-7260` `push_projectile_event` | push + 满 16 丢最旧 | `push` 后 `if len > 16 { remove(0) }`；`projectile_events` 全仓无其它清空点（仅 `:2901` 新建快照置空） | 命中 |
| A7 | `net.rs:2767` `next_spell_id` / `:7277-7278` | 单调序号范式可复用 | `saturating_add(1)` + `push(SpellEvent{id,..})`；`:2901-2903` 归零 | 命中 |
| A8 | `crates/mir-client/src/ui/projectile.rs:446-462` `consume` | F6：`len < consumed` → `consumed=0` → **整表重放 16 帧** | 分支存在但**不可达**：`while` 结束必有 `consumed == len`；`len` 封顶 16、只增不减，故 `len < consumed` 在生产中永不成立（仅快照重建时 `len=0` 触发——那正是要的重置语义） | **不符（F6 误判）** |
| A9 | `projectile.rs:413-415` `take_sound_cues`、`:290-341` sink、`:482-485`/`:509-531` 出生/命中 cue | 队列只涨无人 drain | 与主张一致；`SOUND_CUE_LIMIT` 丢最旧 | 命中 |
| A10 | `crates/mir-client/src/audio.rs:592-625` `play` | 频控→resolve→解码→`mixer.add` | 一致；但 **`:594` 先判 `!has_output()`** 再 resolve/解码（F4 未记此顺序，影响降级用例，见 M-3）；`:615-621` 测试计数路径返回 **true** | 偏差（顺序/返回值） |
| A11 | `audio.rs:196-215` `resolve`、`:240-253` `decoder_for`、`:234-236` `contains`、`:384-386` `set_test_sink` | 断言 3 用 `contains("M34-1.wav")` | `contains(&self, file) -> resolve(file).is_some()`，`.mna` 优先（`:198-206`）→ `contains("M34-1.wav")` 真实可用；`decoder_for` 私有；`set_test_sink` 为 `#[cfg(test)]` | 命中 |
| A12 | `audio.rs:79-85` `SoundThrottle::allow` + `mir_render::SOUND_COOLDOWN_MS` | 100ms 且吞同文件并发 | 按文件名键 `HashMap<String,u64>`，`now-last < 100ms` 返回 false | 命中 |
| A13 | `crates/mir-client/src/app.rs:76-116`（`sound` 在 `:84`）、`:178-191 play_pending_sounds`、`:2725 live.ui.sync` | drain 放 `:2725` 之后同帧可行 | `LiveState` 同时持 `ui`/`sound`；`now_millis_u64()` 已在 `:146/:153/:169` 使用；`take` 返回 `Vec<&'static str>` 不持借用，随后 `sound.play` 无借用冲突 | 命中（可行） |
| A14 | `crates/mir-client/src/ui/mod.rs:1114` 字段、`:1266-1268` getter、`:1564` sync | 字段 pub / getter 只读 | 一致；因字段 pub，`live.ui.projectiles.take_sound_cues()` 已可用，accessor 仅为可读性 | 命中 |
| A15 | `docs/testing/round59/README.md:52-62` P-2 | R59 已记「可能漏事件」 | 原文「满队列后『丢最旧+追加』在 `len` 上不可观测，**可能漏事件**」——R59 诊断比 R60 的 F6 更准 | 命中（反向佐证 A8） |
| A16 | `docs/process/round59-shangshu-acceptance.md:20`、`docs/testing/round59/final-manifest.json:302/:314` | 清单未随 HEAD 重签；建议冻结自身 sha + 门禁 HEAD 头 | `:20` 记「对 HEAD 14/16」；manifest `:302` N-4「建议冻结自身哈希」、`:314` 自身 sha 复跑命令 | 命中 |

**统计**：抽查 16，命中 13，偏差 2（A10 降级顺序、A8 的 F6 结论），不符 1（G2 断言 3 期望值，见 M-1）。Crystal 4/4 全中；Rust 10 项 9 中、1 偏差（A10）。

## 二 plan §七 5 项逐条裁决

1. **G2 用客户端绝对序号游标（新增 `projectile_events_total`，不动结构与协议）——修改后批准。**
   方向正确：无线上 ID（A4）、`next_spell_id` 范式现成（A7）、不改 `ProjectileEvent` 可避免 R58 wire 回归。
   但 **F6 的「ring 截断导致整表重放」不成立**：`consume` 的重置分支在生产中不可达（A8），真实缺陷是**第 17 个及之后的事件被永久静默丢弃**（`consumed` 停在 16、`len` 恒 16，`while` 永不再进），这比重放更严重（该会话后续所有 125 表现消失）。
   且断言 3 的「total=17/consumed=16 → 新增 view **0**」与断言 2 公式 `start = consumed - dropped` 自相矛盾（该公式得 `start=15`，必然产出 **1** 个 view）。按断言 3 落地会**把「事件丢失」固化成验收绿**。必须按 M-1/M-2/M-5 改写后批准。

2. **G1 机器证据＝通道+解码+注入混音器+降级用例，真实可听归 Lead 手听（可选），无手听即 PASS-WITH-PARTIAL——修改后批准。**
   接受 PARTIAL 口径与「禁把可听写成已证」。但两条证据当前不可执行：(a) `play` 返回 false 依赖 `!has_output()`（A10/A11），真机有声卡时恒为 true → 「无设备时 play 返回 false」不是环境无关断言，在有声卡机器必红；(b) `set_test_sink` 是 `#[cfg(test)]`，**集成测试 `tests/*.rs` 看不到**（lib 以非 test cfg 编译），fake-sink 计数只能进 crate 内单测。按 M-3/M-4 改写。

3. **125 cue 保留 100ms 频控（降噪优先）而非旁路——批准。**
   `SoundThrottle` 按文件名 100ms（A12），旁路需改既有设计与 `mir_render` 常量，收益仅是「同技能连发都响」；且旁路后同一帧多个同文件 cue 会叠加爆音。保留，但必须：README §3 单列「同文件 100ms 内只响一次 ≠ Crystal 每发都响」；drain 计数区分「取走 cue 数」与「`play` 返回 true 数」，不得把频控吞掉的计入「已播」。

4. **门禁跑在代码提交①后、文档提交②前（R59 同法）——批准。**
   符合「每轮唯一一次全量门禁」：门禁只跑提交① HEAD，文档批不再触发重跑。条件：`target/round60/gate/attempt-1.log` 字节数 + sha256 原文入 README §2，`gate_head` 记提交① HEAD（非 docs 批 HEAD，见 M-7）。

5. **树哈希清单入树 + 头部 `gate_head`/`gate_log_sha256` + 清单自身 sha 入 `final-manifest.json`——修改后批准。**
   满足 R59 尚书建议（A16）且破自哈希循环。补两项：生成命令必须可复跑并写入 README §5；清单须覆盖 `git diff 提交①..提交②` 的全部 `crates/**`+`docs/**`（R59 曾因漏签 docs 记 14/16）。

## 三 必改 M 项（7 条：high 3 / med 3 / low 1）

**M-1（high｜G2 口径）F6 诊断与断言 3 必须改写，禁止把「丢失」写成 PASS。**
F6 改为：「ring 永不重放已消费事件；`consumed` 与 `len` 同为 16 后新事件被**永久丢弃**（可观测：`events_consumed()` 停在 16、`views` 不再增长）」。
断言 3 改为：`len=16,total=17,consumed=16` → 新增 **1** 个 view 且其 payload 唯一等于第 17 个事件（用可区分 `destination`），`views.len()==1`、**不含** #1–#16 任一 payload；再 push 至 `total=18` → 新增 1。另加「旧语义保留」用例：`len < consumed` 时游标归零（对齐 `crates/mir-client/tests/round59_projectile_render.rs:225-246` 的 `cursor_resets_when_the_snapshot_event_table_is_replaced`，该用例断言重建快照后 `events_consumed()==0`，不得改断言值）。

**M-2（high｜G2 回归证据）「旧红→新绿」必须是确定回归，且用例值按 M-1 修正。**
确定回归：`len=16,total=17,consumed=16` 下断言「新增 1 view＝第 17 个事件」——旧实现新增 **0**（转红），新实现 **1**（转绿）。删除 `projectile.rs` 修复行（`dropped` 跳跃/`start` 计算）复核转红并还原，`git diff` 与两次原始输出入 `target/round60/d2/attempt-1/`。
注意：**旧实现不会重放 16 帧**（A8），任何以「旧实现重放 16 帧」为前提的用例会假红；派发件与文档须同步更正，禁沿用 F6 原话。

**M-3（high｜G1 降级用例）降级用例必须环境无关，fake sink 用例搬进 crate 内单测。**
改为两条确定断言：(a) `SoundPlayer` 音量置 0（`volume*master=0`，命中 `audio.rs:594` 的 `gain <= 0.0`）或 `enabled=false` → `play` 返回 `false` 且不 panic（与有无声卡无关）；(b) 注入 fake sink（`set_test_sink`，`audio.rs:383-386`）→ 解码成功后 `play` 返回 `true`、计数 +1，证明 cue 真进播放器。
因 `set_test_sink` 是 `#[cfg(test)]`，该用例只能放 `src/audio.rs` 或 `src/app.rs` 的 `#[cfg(test)] mod tests`（`cargo test -p mir-client --bin mir-client`），**不能**放 `tests/round60_audio_channel.rs`；plan §四 须补 `cargo test -p mir-client --bin mir-client -- app::` 之外的 `--bin mir-client` 全量或指名用例。

**M-4（med｜G1 素材边界）素材缺失 SKIP 单列 PARTIAL，禁假绿。**
`contains("M34-1.wav")`/`("M34-2.wav")` 可用（A11），但须额外断言解析结果后缀为 `.mna`（防 legacy wav 顶替，`resolve` 在 `allow_legacy_wav=false` 时才拒 wav），并断言 `mir_asset::AudioClip::open` + rodio `Decoder::new` 均成功。`game-assets/` 不入库 → 缺素材时打印显式 `SKIP: missing M34-1.mna` 并以非 0 或 SKIP 标记让 `run-checks.sh` 记 **PARTIAL**；禁 `#[ignore]`、禁把 SKIP 当 PASS、禁以 `M34-0` 顶替。
文档口径硬约束：禁写「125 音频完整」「无重复」「真实可听」；统一写「cue→解码→混音器路径已通；**可听性 PARTIAL**」。

**M-5（med｜G2 复位语义与命名）计数器范式对齐 `next_spell_id`，复位检测不能只看 total。**
`projectile_events_total` 命名/类型/自增/归零对齐 `net.rs:2767`+`:2901`+`:7277`（`u64` + `saturating_add(1)`）。
复位检测**必须保留** `len < consumed → consumed = 0` 旧分支并叠加跳跃，否则手工构造快照（`round59`/`round58` 用例直接 `push` 事件、total 恒 0）会误判复位 → 重放 → 破坏幂等与 R59 用例。参考实现（非唯一，D2 可等价改写）：
```rust
let len = events.len();
if len < self.consumed { self.consumed = 0; }            // 快照重建：旧语义，勿动
let dropped = total.saturating_sub(len as u64);          // ring 已丢最旧数
if dropped > self.consumed { self.consumed = dropped; }  // 从未消费就被丢：跳过，不重放
let mut i = (self.consumed - dropped) as usize;
while i < len { self.spawn(events[i], ..); i += 1; self.consumed += 1; }
```
`EFFECT_EVENT_LIMIT` 保持 16（A5）；`git diff` 中 `ProjectileEvent{..}`（`net.rs:1503-1511`）零改动。

**M-6（med｜门禁与脚本）0-tests 守卫与 SKIP/STRICT 标记写死。**
`docs/testing/round60/run-checks.sh` ≥9 项、`--list` 零执行零写入；每条测试命令先 `cargo test … -- --list` 校验发现数门槛，执行期再 grep `running 0 tests` / `0 passed` 判 FAIL（R59 脚本头 `:13-16` 已是范式）；素材相关条必须输出 `SKIP:`/`PARTIAL:` 字面量并被脚本识别，SKIP ≠ PASS。

**M-7（low｜冻结形式）自哈希与门禁 HEAD 规则写死。**
`01-tree-hashes.txt` 自身 sha **只**入 `final-manifest.json`（A16）；头部 `head=` 记提交①、`gate_head=` 记跑门禁时的 HEAD（两者相同）、`gate_log_sha256=` 记 `attempt-1.log`；清单由 `git show <commit>:<path> | sha256sum` 生成且命令入 README §5 可复跑。

## 四 对 plan / DAG 的补充

1. **D1 断言 1 的 grep 门槛偏弱**：`take_projectile_sound_cues` 各 ≥1 命中不等于行为正确，须补「drain 一次取走 N 条、第二次为 0、`play` 调用数 == N」的单元断言；`UiState.projectiles` 已 pub（A14），accessor 可省，保留亦可（仅转调，不碰 `ui/projectile.rs`，与 D2 无交叉）。
2. **D2 写范围**建议显式加「`ui/projectile.rs` 内联 `#[cfg(test)] mod tests` 增 ring 溢用例」，与 `net.rs` 单测（push/归零）同批；`tests/round60_projectile_dedupe.rs` 保留 ≥5 条中必须含 M-1 的两条强断言。
3. **§四 验收命令**补 `cargo test -p mir-client --bin mir-client`（承载 M-3 的 fake sink/降级用例）；`-- app::` 过滤会漏掉 `audio::` 用例。
4. **D3 台账**：R59 `README.md:52-62` P-2 的状态列要写「R60 已修『丢失』？——否：ring 16 仍丢最旧（只保证已消费事件不重放、新事件不再被游标吞掉）；漏事件风险仍在」，与 R4 的「不保证不丢」一致。
5. **串行链不变**（`D1∥D2 → 提交① → 门禁 → D3 → D4 → 提交②`），但 G2 的 F6 改写属方案级更正，须在派发件里同批下发，避免 D2 按旧 F6 写用例。

## 五 R61 承接提示（不膨胀本轮）

1. ring 16 的**真实丢事件**（客户端卡顿 >16 事件）仍存在：R61 评估按需扩容或改为「消费即 drain + 服务端重发」，本轮不做。
2. 「同文件 100ms 只响一次」与 Crystal 每发都响的表现差：R61 若做打击感对齐，需先定「同类音效并发上限」而非直接删频控。
3. 真实可听性（Lead 手听）+ 真 socket 端到端 125 表现：R61 与 Hero/真 socket 前置一起做，本轮只登记 PARTIAL。
4. P-3（每帧重算 Duration/Count）、P-4（`magic.wzl` 逐帧 offset）维持 R59 台账，继续不承接。
