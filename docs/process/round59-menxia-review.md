# R59 门下省评审（menxia，gpt-6.1-sol，只读复核）

## §1 受审对象与 HEAD

- 主单 `JJC-20261026-006`（R59 客户端 125 渲染最小闭环），状态 **Menxia**。
- 受审对象：`docs/process/round59-plan.md`，120 行 / 13,225 B / sha256 `5a233b58cb4ae5f178e691cc0217ae3bebfc2ee7137d0a6bf46abb0789a78111`（实读一致）。
- HEAD = `origin/main` = `3cb1df1`（`git rev-parse` 双值全等）；工作树仅 plan（新）+ 2 个历史 freeze JSON 未跟踪，与计划声明一致。
- 参照：`docs/process/round58-menxia-confirm.md` §4（R59 承接 8 项）、`docs/testing/round58/README.md`、`docs/world/magic.md:687`、`docs/client.md`（末尾 §5.25）、`docs/roadmap.md:61-64`。
- 本轮**不跑 cargo/ci**，素材断言以只读真跑（python 直读 `.wzx`/`.wzl`）与静态锚点为准。

## §2 锚点抽核表（16 组，命中/偏差）

| 锚点 | 抽核结论 |
|---|---|
| `github/Crystal/Client/MirScenes/GameScene.cs:4691` | 命中：`if (source == null) return;` |
| `…/GameScene.cs:4693-4695` | 命中：`switch (p.Spell)` 仅 `case Spell.FireBounce`，**无 default** |
| `…/GameScene.cs:4697` | 命中：`SoundManager.PlaySound(20000 + (ushort)Spell.GreatFireBall * 10 + 1);` → 基准是 **GreatFireBall=34**，非 FireBounce=154 |
| `…/GameScene.cs:4699` | 命中且**计划漏述**：`CreateProjectile(410, Magic, true, 6, 30, 4, targetID: p.Destination)` **无条件调用** → 参数实为 `count=6, interval=30, skip=4` |
| `…/GameScene.cs:4701` | 命中：仅 `missile.Target != null` 才挂 `Complete`（**只有命中被门控，投射物不被门控**） |
| `…/GameScene.cs:4707-4709` | 命中：`Target.CurrentAction == Dead` → return；`new Effect(Magic, 570, 10, 600, sender.Target)`；`+2` 音效 |
| `…/GameScene.cs:12044-12065` | 命中：`angle += 11.25` → `/22.5` → 0..15，`destination.X < c.X` 走 360° 镜像 |
| `github/Crystal/Client/MirObjects/MapObject.cs:339` | 命中：`virtual Missile CreateProjectile(int baseIndex, MLibrary library, bool blend, int count, int interval, int skip, int lightDistance = 6, bool direction16 = true, Color? lightColour = null, uint targetID = 0)` |
| `github/Crystal/Client/MirObjects/PlayerObject.cs:3539-3567` | 命中：`targetPoint` 优先 `ob.CurrentLocation`、`ob==null` 回退 `TargetPoint`；`duration = MaxDistance(CurrentLocation, targetPoint) * 50`；`Missile{count=duration/interval, FrameCount=count(6), Skip=skip(4), Interval=interval(30)}` |
| `github/Crystal/Client/MirObjects/Effect.cs:174-181` | **偏差**：`Process` **每帧重算** `Duration = MaxDistance(Source, Destination)*50; Count = Duration/Interval; if (Count==0) Count=1;`；计划 G2 冻结在生成时（可接受简化，须登记） |
| `…/Effect.cs:196` | 命中：`index = BaseIndex + (CurrentFrame % FrameCount) + Direction * (Skip + FrameCount)` = `410 + f%6 + dir*10` |
| `…/Effect.cs:206-212` | 命中：`DrawLocation.Offset(x * CurrentFrame / Count, y * CurrentFrame / Count)` 线性插值，`Source` 生成时固定 |
| `github/Crystal/Shared/Enums.cs:1168/:1264` | 命中：`GreatFireBall = 34`、`FireBounce = 154` → `20341/20342` |
| `crates/mir-client/src/net.rs:1495-1511` | 命中：`ProjectileEvent{spell,source,destination}`，无时间戳、无目标坐标；doc 明示“unknown destination must not panic or drop the event” |
| `crates/mir-client/src/net.rs:5385-5400`、`:166`、`:7257-7259` | 命中：入队条件 `spell==FireBounce && object_position(source).is_some()`；`const EFFECT_EVENT_LIMIT: usize = 16;`；`> LIMIT → remove(0)`。全仓 `projectile_events` 无其它消费方（仅 R58 测试） |
| `crates/mir-render/src/entities.rs:1800-1802/1807/1825`、`:518-523` | **计划漏述**：`effect_image/draw_effect` 实为 `image(lib, index + kind.start_frame())`，`start_frame`：`Swing\|Magic=0`、`Bolt=20`；故 `draw_effect(Magic,410)`→`magic.wzl[410]` 成立，但依赖 Magic 偏移为 0 |
| `crates/mir-render/src/{scene.rs:408-427,sound.rs:219-225}`、`mir-client/src/audio.rs:119-137`、`tools/assets/analyze_usage.py:324/:926` | 命中：`draw_effect` 接受任意 index；`spell_sound` 产 `M{spell}-0.wav`（`FireBounce`→`M154-0`，与 34 不同源）；运行期只读 `game-assets/audio{,/sfx}` 且 `allow_legacy_wav=false`；`MAGIC_FRAMES = list(range(0, 10))` 在 :324、`add_many("magic.wzl", MAGIC_FRAMES, …)` 在 :926 |
| `docs/testing/round58/README.md:3`、`round58-menxia-confirm.md:6` | 命中：README:3 仍写 `HEAD 1a9aeb3`；确认件明示交付 HEAD=`2f3ab9f`、`1a9aeb3` 是**边界父**；`git cat-file` 两提交均存在 |
| 素材只读真跑（非推断） | `magic.wzx` 头偏移 **44** 的 count=4010，偏移表自 48 起；逐帧解码 `magic.wzl`：**410–415 真帧**（84×82/84×78/72×79…）、**416–419 全空**（fmt0/wh0×0/zlen0）、420–425 真、…、560–565 真、**566–569 全空**、**570–579 真**（80×77…92×119）。`game-assets/gfx/sprites/effect/magic` 现仅 `0000–0009.png`+`index.json`。`game-assets/audio/sfx` 102 个 `.mna` 仅 `M34-0.mna`；`resource/community/gqf2008-crystal/Sound/M34-{1,2}.wav` 在（319,532 B / 229,420 B）；**ffmpeg 9.0.2 本机可用** |

**偏差小结**：①计划 §1.3「非空**偏移**区间恰为…」措辞失准——4010 个偏移全非 0，“空”在**帧**（416–419/566–569 为 0×0）；②漏 `kind.start_frame()` 偏移；③漏 `:4699` 的 count/interval/skip 参数映射与“无条件生成”语义；④`projectile_sound` 与既有 `spell_sound` 的 `M{spell}` 命名不同源。其余锚点逐字命中。

## §3 C1–C5 裁决

- **C1 按公式取真实 `magic.wzl` 帧、不建 Missile 类 → 修改后批准。** 条件：index 与位置公式须逐字照 `Effect.cs:196/206-212`（`410+f%6+dir*10`、`lerp(Source, 该帧 Target.CurrentLocation, f/Count)`）、`dir=Direction16(Source, Destination)`；**禁止**新增 `FxKind` 变体或改 `start_frame()`（`entities.rs:518-523`），断言须证明 `draw_effect(Magic,410)`/`(Magic,570)` 命中 `magic.wzl[410]/[570]`；G1 的 `mem::take` 需 `&mut WorldSnapshot`，若绘制路径只持 `&` 则改游标（禁用 clone 快照绕过）。不建 Missile 类可接受，但**不得**写作“Effect/Missile 体系已移植”。
- **C2 音效须本机 ffmpeg 导入，缺则 PARTIAL → 修改后批准。** 实测 ffmpeg 9.0.2 已在 PATH、`M34-1/2.wav` 源齐 → **“缺 ffmpeg”在本机不构成 PARTIAL 理由**；D3/D4 必须真跑 `audio import` + `audio verify` 且 exit 0。**仅当导入真失败**（须贴原始 stderr）才许记 PARTIAL；`M34-1/2.mna` 属 gitignore 的 `game-assets/`，证据须落 `target/round59/`（日志 + `.mna` sha256）并在 `docs/testing/round59/README.md` 给重生命令。
- **C3 unknown destination 不生成 → 修改后批准（须登记为偏离，不得称对齐）。** `:4699` 无条件建投射物、`ob==null` 回退 `TargetPoint`，仅 `:4701` 命中受 `Target != null` 门控；本作无落点字段且禁造坐标，故“无投射物、无命中、无 +2”是**诚实降级**但**与 Crystal 有差**：须以「已知偏离 D-R59-1」入 `docs/client.md` §5.26 与 `docs/testing/round59/README.md`，并明确 `:4697` 出生音效策略（建议：随投射物生成，无投射物则无声）与对应断言。
- **C4 PASS-WITH-PARTIAL 门槛 → 修改后批准（附约束）。** 逻辑门（G1–G3、G7）**不得 SKIP**，0 tests 判 FAIL；G4/G5 在本机素材齐备（含 ffmpeg）前提下不属于“素材缺失”，故默认应为**绿**而非 PARTIAL。仅当 D3 未落地时，G2/G3 只能表述为“公式已证、像素未证”，且 `draw_effect(Magic,570)==true` **不得**计 PASS（SKIP≠PASS）。验收须给“绿 N 项 + PARTIAL M 项 + 依据”三行式，禁写“G1–G7 全绿”。
- **C5 `README.md:3` HEAD → 批准更正为 `2f3ab9f`，不等待。** 依据 `round58-menxia-confirm.md:6`（交付 HEAD `2f3ab9f`；`1a9aeb3` 只是边界父），`1a9aeb3` 是**事实性错值**，等 R59 提交会把 R58 记录与 R59 HEAD 混同。条件：只改该 1 行、不得写入 R59 自身 HEAD，并在 `docs/testing/round59/README.md` 记一行更正理由（闭合 R58 承接 #8 与 H PARTIAL）。

## §4 必改 M1–M3（须落进派发件）

- **M1（G5 帧集必须精确，否则空帧污染 `--verify`）**：`magic.wzl` 的 416–419、566–569 是 **0×0 空帧**（fmt0/zlen0），`range(410, 566)` 会带进 60 个空帧。`analyze_usage.py:324` 须改**显式段列表**（`[i for d in range(16) for i in range(410 + d*10, 416 + d*10)] + list(range(570, 580))`）或显式过滤空帧，并在 D3 证据中断言“空帧不计 missing / 不产出 0×0 PNG”。素材声明仍为“真实素材 + 本地生成物”。
- **M2（音效基准 = GreatFireBall=34，且与既有命名区分）**：`projectile_sound` 必须以 `GreatFireBall`(34) 为基产出 `M34-1.wav`/`M34-2.wav`，**不得**用 `FireBounce`(154) 拼 `M154-…`；`sound.rs:219-225` 既有 `spell_sound` 是 `M{spell}-0.wav`，新函数须用显式常量 + 注释引用 `GameScene.cs:4697/4709`，fake-sink 测试须断言字面量 `M34-1.wav`/`M34-2.wav` 与次数（出生 1 / 命中 1），不得只断言“非空”。
- **M3（保真口径与偏离登记三连）**：①`draw_effect` 实为 `index + start_frame()`——派发件须写明“D1/D2 只用 `FxKind::Magic`，禁止改 `start_frame`/新增变体”，G3 素材断言带“命中 `magic.wzl` 第 570 号图”字样；②C3 偏离 D-R59-1（无落点即不生成投射物/命中/音效）与 Crystal `:4699`+`:4701` 的差别须逐行引用入文档；③G2 须登记“`Duration/Count` 在 Crystal `Effect.cs:174-181` 为每帧重算、本作生成时冻结”，不得表述为“与 Crystal 逐字一致”。

## §5 判定

**PASS-WITH-CHANGES**。G1–G7 目标可证伪、16 组锚点中 13 组逐字命中，方案骨架可执行；放行条件为 M1–M3 全部写入 `round59-shangshu-dispatch.md` 并落到 D1–D4 验收口径。**禁称**「125 全链」「渲染完整」「与 Crystal 逐字一致」「G1–G7 全绿」；PARTIAL 仅限 D-R59-1 边界与（真失败时的）音频可听性，且 SKIP≠PASS。
