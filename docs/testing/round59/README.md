# 第 59 轮（R59）文档与证据索引

**本轮范围**（主单 `JJC-20261026-006`，D5 文档批 G6；基线 HEAD `3cb1df1`=`origin/main`）：只做客户端 125
（`S.ObjectProjectile` / `Spell.FireBounce`）**表现链最小闭环 + PARTIAL**——事件消费 → 飞行插值 →
命中 570 → 音效；**不建 `Missile`/`Effect` 类体系**。方案见 [round59-plan.md](../../process/round59-plan.md)，
裁决见 [round59-menxia-review.md](../../process/round59-menxia-review.md)（PASS-WITH-CHANGES，C1–C5 + M1–M3），
派发见 [round59-shangshu-dispatch.md](../../process/round59-shangshu-dispatch.md)。

**口径**：状态列只允许「已落盘」「已落盘 + 已实跑」「PARTIAL」「在途」；「已实跑」只表示对应命令在本机跑过并留证
（命令/发现数/exit），**不等于 T/刑部或门禁判定**。不写整体通过/已闭环类未证结论，**不预写 T/刑部审计结论**。
本索引的「状态」= D5b 同步时点（D2–D4 已落盘）的实际落盘情况，最终判定以刑部 D6 与本轮门禁为准。

> **125 口径**：服务端 125 生产者已实现（R57）；客户端 R58 只解码入队，**R59 起按游标消费 + 飞行插值 +
> 命中 570–579 + `M34-1/-2` 音效**；D2/D3/D4 已落盘，仍是最小闭环，**不等于整体链路打通**；真实可听受
> `app.rs` 音频通道未接约束（PARTIAL）。

## 1. 目标与验收矩阵（已实跑 N / PARTIAL M / 依据）

| # | 目标 | 状态 | 依据 |
|---|---|---|---|
| G1 | 事件消费：每帧按位置游标消费 `WorldSnapshot::projectile_events`，一事件一 `ProjectileView`，同快照重复 `sync` 不重复生成 | 已落盘 + 已实跑 | `ui/projectile.rs:422-439`（`sync`）、`:446-462`（`consume`）、`:207-246`（`ProjectileView`）；`cargo test -p mir-client --test round59_projectile_render` → **16 passed / exit 0**（`target/round59/d4/attempt-1/02-render.log`）；D1 证据 `target/round59/d1/attempt-1/` |
| G2 | 飞行插值：`duration=MaxDistance*50`、`count=max(1,d/30)`、`index=410+f%6+dir16*10`、`offset=Δ*(48,32)*f/count`、`dir16` 按 `GameScene.cs:12044-12065` | 已落盘 + 已实跑 | 同上（16 passed / exit 0）；段值断言 410..=565 且避开空帧 `416-419/566-569` |
| G3 | 命中 570：到达帧且目标存在、`dead==false` → `magic.wzl[570..=579]`×60ms=600ms；目标未知/已死 → 不播（含不播 `+2`） | 已落盘 + 已实跑 | `ui/projectile.rs:79-85`（`HIT_EFFECT_BASE_INDEX=570`/`FRAME_COUNT=10`/`DURATION_MS=600`）与删除点 `:505-537`；`round59_projectile_render` 16 passed（含 `dead_target_skips_impact_and_hit_sound`、`hit_effect_plays_all_ten_magic_frames_when_the_target_survives`）；证据 `target/round59/d2/attempt-1/`、`target/round59/d4/attempt-1/` |
| G4 | 音效：出生 `M34-1.wav`(20341)、命中 `M34-2.wav`(20342)，基准 `GreatFireBall=34`（非 FireBounce=154） | 逻辑落盘；真实可听 PARTIAL | `mir-render/src/sound.rs:234-281`：`PROJECTILE_SOUND_BASE=34`、`PROJECTILE_SOUND_SPAWN_INDEX=20341`、`PROJECTILE_SOUND_HIT_INDEX=20342`、`ProjectileSoundPhase{Spawn,Hit}`、`projectile_sound(spell, phase) -> Option<&'static str>` → `M34-1.wav`/`M34-2.wav`；`cargo test -p mir-render --lib sound` → **9 passed / exit 0**（`target/round59/d2/attempt-1/04-tests.log`）；`round59_projectile_render` 16 passed 含 `spawn_and_hit_sounds_use_m34_once_each`、`production_sound_cues_are_drained_once_per_projectile`。**PARTIAL**：UI 只持 `&WorldSnapshot`，`take_sound_cues()`（`ui/projectile.rs:413`）尚未被 `app.rs` 音频通道 drain → 真实可听未证 |
| G5 | 素材包扩展：`analyze_usage.py:324-327` 改显式 106 帧段（16×6 + 570–579），空帧不计 missing、不产 0×0 PNG | 已落盘 + 已实跑 | `cargo test -p mir-client --test round59_projectile_assets` → **6 passed / exit 0**（`target/round59/d4/attempt-1/03-assets.log`）；`run-checks.sh` 第 9 项素材校验 exit 0；D3：106 PNG、`missing=[]`、`skipped_empty=0`、全计划 `used_frames` 186,294→186,390（`target/round59/d3/attempt-1/00-summary.md`） |
| G6 | 文档入树：本索引 + `client.md §5.26` + `magic.md` FireBounce 行 + `round58/README.md:3` + `asset-pipeline.md:112` + `roadmap.md` R59 行 | 已落盘（D5/D5b） | 本文件；D5 `target/round59/d5/attempt-1/`；D5b 状态回填 `target/round59/d5b/attempt-1/`（`00-summary.md`…`04-diff-stat.txt`） |

**三行式结论（D5b 同步时点，D2–D4 已落盘）**：已落盘 + 已实跑 **4** 项（G1/G2/G3/G5）＋ 逻辑落盘、真实可听
**PARTIAL** **1** 项（G4）＋ 文档 **1** 项（G6）。**依据**：G1/G2/G3 用 D4 `round59_projectile_render`（16 passed /
exit 0）与 D2 命中状态机，G5 用 D4 `round59_projectile_assets`（6 passed / exit 0）与 D3 的 106 PNG/`index.json`/
`--verify` 日志，G4 用 `mir-render sound` 单测（9 passed / exit 0）与 cue 队列断言；本地人工脚本
`run-checks.sh` 9/9 条 exit 0（脚本自报汇总见 §5）只证明本机本次跑过。**T 未审、门禁未跑，不写整体通过类结论。**
SKIP ≠ PASS。

## 2. 证据索引

| 路径 | 内容 | 状态 |
|---|---|---|
| `target/round59/d1/attempt-1/` | `00-summary.md`、`01-check.log`、`02-clippy.log`、`03-fmt.log`、`04-tests.log`、`05-diff.txt` | 已落盘（D1） |
| `target/round59/d2/attempt-1/` | G3 命中 + G4 `projectile_sound`/cue 队列：`00-summary.md`、`04-tests.log`（`mir-render sound` 9 passed）、`05-diff.txt`、`06-mir-render-tests.log`（128 passed/4 failed，4 例均为环境写权限，非本单文件） | 已落盘（D2） |
| `target/round59/d3/attempt-1/` | `00-summary.md`、`01-analyze-usage-diff.txt`、`02-generate.log`、`03-frame-count.log`、`04-wzx-vs-png-check.log`、`05-index-json-sample.json`、`magic-plan.json`、`_neg-*` 负控 | 已落盘（D3） |
| `target/round59/d4/attempt-1/` | `00-summary.md`、`01-list.log`、`02-render.log`（16 passed）、`03-assets.log`（6 passed）、`04-script-run.log`（9/9 PASS、总 EXIT=0）、`07-audio-import.log`、`08-audio-verify.log`（`files=106 bytes=4.25MB`）、`09-audio-sha256.txt` | 已落盘（D4） |
| `target/round59/lead-verify/` | Lead 复跑：`10-run-checks.log`（9 项 PASS、总 EXIT=0）、`02-lib-sound-retry.log`（8 passed）、`03-render.log`、`04-assets.log` | 已落盘（Lead） |
| [run-checks.sh](run-checks.sh) | 9 项人工验收脚本（420 行；任一项 0 tests 即整体 FAIL）；证据默认写 `target/round59/d4/script/` | 已落盘（D4） |
| `target/round59/d5/attempt-1/` | D5 自检：`00-summary.md`、`01-check-links.log`、`02-diff-check.log`、`03-redline-grep.log`、`04-diff-stat.txt` | 已落盘（D5） |
| `target/round59/d5b/attempt-1/` | D5b 状态同步自检：`00-summary.md`、`01-check-links.log`、`02-diff-check.log`、`03-redline-grep.log`、`04-diff-stat.txt` | 已落盘（D5b） |

素材声明：G2/G3 的帧号来自**真实素材**（`resource/data/magic.{wzl,wzx}`，`magic.wzx` 4010 项，
410–415/420–425/…/560–565 与 570–579 为真帧，416–419/566–569 为 0×0 空帧）；`game-assets/` 与 `resource/`
均被 `.gitignore` 忽略（`.gitignore:27`），故素材类断言只能靠 `target/` 日志与本地重生。

## 3. PARTIAL / 偏差台账

| 编号 | 项 | 说明（与 Crystal 的差） | 归属 |
|---|---|---|---|
| **D-R59-1** | 无落点即不生成 | Crystal `GameScene.cs:4699` **无条件** `CreateProjectile`（落点可用 `TargetPoint` 兜底，`:4701` 只门控命中）；125 只带 `source`/`destination` 两个 ID，`destination` 不在快照里 → 本作**不生成投射物、不生成命中、不播 `+2`**，不伪造坐标。**诚实降级但非对齐** | D1 已实现；文档登记 |
| P-2 | 事件无 ID | `ProjectileEvent` 无时间戳/序号（`net.rs:1503-1511`）；队列容量 16、超限丢最旧（`net.rs:7246-7260`），满队列后「丢最旧+追加」在 `len` 上不可观测，可能漏事件 | 残留风险，R60 承接 |
| P-3 | 冻结时长 | 本作 `duration`/`count` 在 spawn 时冻结（`projectile.rs:465-497`）；Crystal `Effect.cs:172-191` `Process` 内每帧重算，目标移动会改变总时长 | 已知简化 |
| P-4 | Draw offset | `draw_effect` 不叠加 `magic.wzl` 每帧 `ImageInfo.offset`（Crystal `MLibrary` `offSet=true` 路径）；属 `mir-render` 既有绘制缺口，本轮未改 | 已知缺口 |
| P-5 | 音效素材/可听性 | `M34-1/-2` **素材已导入**：由 `resource/community/gqf2008-crystal/Sound/M34-{1,2}.wav` 经 `mir-tools audio import`（本机 ffmpeg）产出，落 `game-assets/audio/sfx/M34-{1,2}.mna`（该目录现 105 个文件）；`audio verify --input game-assets/audio` → `files=106 bytes=4.25MB`、exit 0（`target/round59/d4/attempt-1/08-audio-verify.log`）。**但真实可听仍 PARTIAL**：`take_sound_cues()` 未被 `app.rs` 音频通道 drain（UI 只持 `&WorldSnapshot`），D4 只证 cue 字面量/顺序/每投射物一次；`game-assets/` 不入库，`.mna` 证据仅存 `target/` | PARTIAL（可听性）；素材已落盘 |
| P-6 | 无类体系 | 未实现 `Missile`/`Effect` 类、光照（`Light=6`/`lightDistance`）、`Skip=4` 之外的图库语义；不得写作「Effect/Missile 体系已移植」 | 明确不做 |
| P-7 | 无 Hero / 无真 socket | 证据限于进程内 encode/decode/快照链；Hero 实例与 socket 端到端未做 | 明确不做 |

## 4. R60 承接

1. **事件 ID/序列号**：在 `net.rs` 给 `ProjectileEvent` 编号或带 `started_ms`，消除游标/丢最旧歧义（P-2）。
2. **每帧重算 Duration/Count**：对齐 Crystal `Effect.cs:172-191`（P-3），需先定「目标移动改变总时长」的表现口径。
3. **wzl 逐帧 `ImageInfo.offset`**：`mir-render` 绘制层补 offset 通路（P-4），会同时影响其它 `FxKind`。
4. **命中生命周期**：目标 `dead`/消失与 `Complete` 回调的完整状态机（含 `+2` 门控）落到 socket 级用例。
5. **素材与音效收口**：把 `M34-1/-2.mna` 与 106 帧 PNG 的生成纳入可复跑脚本（当前 `game-assets/` 不入库，P-5）。
6. **Hero 实例**、**真 socket**：R59 不做，仍为后续轮次前置。

## 5. 复跑命令

```bash
# 逻辑测试（D4 实测 16 passed / exit 0；缺素材也会跑，0 tests 判 FAIL）
cargo test -p mir-client --test round59_projectile_render
# 素材测试（D4 实测 6 passed / exit 0；缺 game-assets/ 时打印 SKIP；SKIP≠PASS）
cargo test -p mir-client --test round59_projectile_assets
# 音效映射单测（D2 实测 9 passed / exit 0）
cargo test -p mir-render --lib sound
# 素材刷新（本地；game-assets/ 被 .gitignore 忽略）
py -3 tools/assets/analyze_usage.py
cargo run -p mir-tools -- assets unpack --plan target/used-assets.json --out-root game-assets --verify
# 音效（本机 ffmpeg 可用；D4 已产出 M34-1/-2.mna）
mkdir -p target/round59/audio-in
cp resource/community/gqf2008-crystal/Sound/M34-1.wav resource/community/gqf2008-crystal/Sound/M34-2.wav target/round59/audio-in/
cargo run -p mir-tools -- audio import --input target/round59/audio-in --out game-assets/audio/sfx --keep-source
cargo run -p mir-tools -- audio verify --input game-assets/audio
# ↑ 实测：audio verify ok files=106 bytes=4.25MB、exit 0（D4 `08-audio-verify.log`）
# 文档与静态自检
py -3 tools/docs/check_links.py
git diff --check
# 人工验收（D4 交付 9 项；Lead 2026-10-26 17:19 复跑：9/9 PASS、总 EXIT=0）
bash docs/testing/round59/run-checks.sh
# ↑ 上一行是脚本自报输出（`target/round59/lead-verify/10-run-checks.log`）；T 未审前不作整体通过结论
```

## 6. 本轮文档改动说明（含一处历史更正）

- [docs/client.md](../../client.md) §5.26 新增「125 ObjectProjectile 客户端渲染」：事件来源/消费语义/插值公式/
  命中 570/音效/五项已知偏差（D-R59-1、事件无 ID、冻结时长、draw offset、无 Hero 与真 socket）。
- [docs/world/magic.md](../../world/magic.md) FireBounce 行：由「仅解码、PARTIAL 待 R59」改为「最小闭环 +
  PARTIAL 边界逐条列明」；同文件 R52/R57 段落里「125 客户端渲染未接」的旧口径同步为 R59 现状。
- [docs/testing/round58/README.md](../round58/README.md) 第 3 行：HEAD `1a9aeb3` → `2f3ab9f`。**理由**：
  `1a9aeb3` 是 R58 的**边界父**，`2f3ab9f` 才是 R58 交付 HEAD（依据
  [round58-menxia-confirm.md](../../process/round58-menxia-confirm.md):6，C5 裁决）；只改该 1 行、不写入 R59
  自身 HEAD，闭合 R58 承接 #8 与 H 的 PARTIAL。
- [docs/assets/asset-pipeline.md](../../assets/asset-pipeline.md):112：`magic.wzl[0..9]` → 106 帧显式段
  （410–415/…/560–565 + 570–579，显式排除 0×0 空帧 416–419/566–569；依据 R59 D3 与 M1 口径）。
- [docs/roadmap.md](../../roadmap.md) §八：R59 行状态「🟡 已落盘待 T/门禁抽核」。
- **D5b（2026-10-26）状态同步**：D2/D3/D4 落盘后，本文件 §1 把 G1/G2/G3/G5 改为「已落盘 + 已实跑」、
  G4 改为「逻辑落盘 + 真实可听 PARTIAL」，§2 恢复 `run-checks.sh` 可点击链接并补 D2/D4/Lead 证据，
  §3 P-5 由「素材缺」改为「素材已导入、可听性 PARTIAL」，§5 补命令实测数；`client.md §5.26` D/E 与
  `roadmap.md` §八同步。只改文档事实，不改代码、不跑 CI、不写 T/门禁结论。
