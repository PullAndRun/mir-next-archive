# 第 59 轮（R59）方案：客户端 125 渲染最小闭环（含 PARTIAL 边界）

> 规划：中书省（gpt-6.1-sol）｜基线 HEAD `3cb1df1`＝`origin/main`（工作树仅 2 个历史 freeze JSON 未跟踪）｜
> 口径：**最小闭环 + PARTIAL**。禁称「125 全链」「渲染完整」；本方案只覆盖 Crystal `GameScene.cs:4687-4715` 的客户端表现链。

## §一 事实（真读锚点）

### 1.1 Crystal（参照语义）

| 锚点 | 事实 |
|---|---|
| `github/Crystal/Client/MirScenes/GameScene.cs:4687-4715` | `:4691` source 未知 → 直接 return；`:4693` switch 仅 `Spell.FireBounce`，无 default；`:4697` 播 `20000 + (ushort)GreatFireBall * 10 + 1`；`:4699` `source.CreateProjectile(410, Libraries.Magic, true, 6, 30, 4, targetID: p.Destination)`；`:4701` 仅 `missile.Target != null` 才挂 `Complete`；`:4707` 目标 `CurrentAction == Dead` → return（命中+`+2` 都不播）；`:4708` `new Effect(Libraries.Magic, 570, 10, 600, sender.Target)`；`:4709` `+2` 音效 |
| `github/Crystal/Client/MirObjects/MapObject.cs:339-342` | 基类 `virtual Missile CreateProjectile(...) => null`（投射物由子类实现） |
| `github/Crystal/Client/MirObjects/PlayerObject.cs:3539-3567` | `duration = MaxDistance(CurrentLocation, targetPoint) * 50`；`targetPoint` 优先 `ob.CurrentLocation`，`ob==null` 回退自身 `TargetPoint`；`Missile{Target=ob, Interval=30, FrameCount=6, Blend=true, Skip=4, Light=6}`，`duration/interval` 传 `count` |
| `github/Crystal/Client/MirObjects/Effect.cs:142-219` | Missile：`Direction = MapControl.Direction16(Source,Destination)`；`Process` 每帧把 `Destination` 跟到 `Target.CurrentLocation`；`Duration = MaxDistance*50`、`Count = max(1, Duration/Interval)`；`Draw` 的 `index = BaseIndex + (CurrentFrame % FrameCount) + Direction * (Skip + FrameCount)`，位置 `(Destination-Source)*CurrentFrame/Count` 线性插值 |
| `github/Crystal/Client/MirScenes/GameScene.cs:12044-12065` | `Direction16`：夹角 +11.25° 后 `/22.5` 取整到 0..15（`destination.X < c.X` 走 360° 镜像分支） |
| `github/Crystal/Shared/Enums.cs:1168/:1264` | `GreatFireBall = 34`、`FireBounce = 154` → 音效编号 **20341**（出生）/ **20342**（命中），对应 `M34-1.wav` / `M34-2.wav` |

### 1.2 Rust 现状

| 锚点 | 事实 |
|---|---|
| `crates/mir-client/src/net.rs:1495-1511` | `ProjectileEvent{spell,source,destination}`，**无时间戳、无目标坐标**；`destination` 允许未知 |
| `crates/mir-client/src/net.rs:5385-5400` | 125 入队条件：`spell == FireBounce && object_position(source).is_some()`；注释明示 R59 才做渲染 |
| `crates/mir-client/src/net.rs:166`、`:7257-7259` | `EFFECT_EVENT_LIMIT = 16`，超限 `remove(0)`（丢最旧）；本轮消费方必须 drain，不改常量 |
| `crates/mir-render/src/scene.rs:408-427` | `effect_image/effect_size/draw_effect(kind,index)` **接受任意 index** → 画 410/570 无需新基座 |
| `crates/mir-render/src/entities.rs:497-514` | `FxKind::{Swing,Magic,Bolt}`，`Magic → magic.wzl` |
| `crates/mir-client/src/ui/spellfx.rs:233-300` | 现只画 `MAGIC_FX_FRAMES=10` / `MAGIC_FX_FRAME_MS=60` 的 `magic.wzl[0..9]`，无投射物实体 |
| `crates/mir-client/src/audio.rs:119-137` | 运行期只认 `game-assets/audio{,/sfx}` 的 `.mna`（非 legacy 不放 `.wav`） |
| `crates/mir-render/src/sound.rs:219-225` | `spell_sound` 只产 `M{spell}-0.wav`，无 `-1/-2` 通道 |
| `tools/assets/analyze_usage.py:324`、`:926` | `MAGIC_FRAMES = list(range(0, 10))` → 素材包 magic 只拆 0..9 |
| `docs/roadmap.md:61-64` | 客户端已有实体渲染/插值/特效池，但路线图无「投射物」条目 → 本轮新增能力而非改旧 |

### 1.3 素材实测（本轮只读真跑，非推断）

| 证据 | 结果 |
|---|---|
| `py -3` 读 `resource/data/magic.wzx` | 4010 项；非空偏移区间恰为 `(410,415)(420,425)(430,435)…(560,565)` = **16 方向 × 6 帧**，另有 `(570,579)` 全 10 帧在 → 410 基座与 570 命中素材**真实齐备** |
| `ls game-assets/audio/sfx` | 102 个 `.mna`，仅 `M34-0.mna`；`M34-1/2` **缺**；wav 源在 `resource/community/gqf2008-crystal/Sound/M34-1.wav`、`M34-2.wav` |
| `git check-ignore -v resource/data/magic.wzl` / `.gitignore:27` | `resource/`、`game-assets/` 均不入库 → 素材类断言缺包时只能显式 SKIP |
| `git ls-files docs/testing/round58/` | 仅 4 文件；R58 `encoding.log`(O-1) 在 `target/`，**树内不可修** |

## §二 可判定目标（每条含证据与素材声明）

- **G1 事件消费**：渲染层每帧对 `WorldSnapshot::projectile_events` 做一次 `std::mem::take`；同一事件只生成 1 个 `ProjectileView`，消费后 vector 为空（复用 R58 ring 语义，不改 `EFFECT_EVENT_LIMIT`）。证据：`crates/mir-client/tests/round59_projectile_render.rs` 断言 take 前长度、take 后为空、二次渲染不再生成。
- **G2 飞行插值**：`duration_ms = max_distance(source, dest_at_spawn) * 50`；`frame_count = max(1, duration_ms / 30)`；`sprite_index = 410 + (frame % 6) + direction16 * 10`；`direction16` 按 `GameScene.cs:12044-12065` 移植。断言具体值：dir=0 → 410..415、dir=1 → 420..425、dir=15 → 560..565、取值域 0..=15；5 格 → duration 250ms / count 8；位置 t=0 为源格、t=duration 为目标格。素材声明：**真实素材**（`magic.wzx` 实测见 §1.3），无 fixture。
- **G3 命中 570**：到达帧若目标对象仍存在且 `EntityView.dead == false`，在其格生成 `570..579` × 60ms = 600ms 表现（`FxKind::Magic`）；目标未知或 `dead` → 不生成（对齐 `:4701/:4707`）。证据：状态机单测 + 素材测试 `draw_effect(Magic, 570)` 返回 true（有 `game-assets/` 时）。
- **G4 音效**：`mir-render/sound.rs` 新增 `projectile_sound(spell)`：FireBounce 出生 `M34-1.wav`、命中 `M34-2.wav`；单测用 fake sink 断言两次文件名与次数；`game-assets/audio/sfx/M34-{1,2}.mna` 存在时 `SoundLibrary::contains` 真，否则该用例 SKIP 并记 **PARTIAL**。
- **G5 素材包扩展**：`analyze_usage.py:324` 的 `MAGIC_FRAMES` 增加 `410..=565`、`570..=579`；重生 `target/used-assets.json` → `game-assets`；断言 `cargo run -p mir-tools -- assets unpack --plan target/used-assets.json --out-root game-assets --verify` exit 0。素材声明：**真实素材 + 本地生成物**（`game-assets/` 不入库，证据落 `target/`）。
- **G6 文档**：①`docs/client.md` 新增「5.26 投射物飞行与命中（R59）」；②`docs/world/magic.md` FireBounce 行由「仅解码」改为「最小闭环（边界列明）」；③`docs/testing/round58/README.md:3` HEAD `1a9aeb3` → R58 交付 HEAD `2f3ab9f`（1 行，事实依据 `round58-menxia-confirm.md:6`）；④新建 `docs/testing/round59/README.md`；`py -3 tools/docs/check_links.py` exit 0 且断链数 0。
- **G7 不破坏回归**：`cargo test -p mir-client --test round58_projectile_wire`（16 长度/首尾 destination 断言）、`cargo test -p mir-client --test client_fx`、`cargo test -p mir-render` 全绿；`net.rs` 的 125 入队条件与 `EFFECT_EVENT_LIMIT` 零改动（`git diff` 可查）。

## §三 子任务 DAG（owner / 写范围 / 依赖 / 串行）

| # | owner | 任务 | 写范围 | 依赖 | 串行 |
|---|---|---|---|---|---|
| D1 | 工部/deepseek | 投射物实体 + 飞行插值（G1,G2） | 新建 `crates/mir-client/src/ui/projectile.rs`；`ui/mod.rs` 挂载与绘制调用 | 无 | 与 D3 并行；先行 |
| D2 | 工部/deepseek | 命中 570 + 音效映射（G3,G4 代码） | `ui/projectile.rs` 完成回调；`crates/mir-render/src/sound.rs` 增 `projectile_sound` | D1 | D1→D2 |
| D3 | 工部/deepseek | 素材包扩展（G5） | `tools/assets/analyze_usage.py:324` 常量；本地重生 `game-assets/`（不入库） | 无 | 可与 D1 并行 |
| D4 | 兵部/deepseek | 测试与门禁脚本（G1–G4 证据） | `crates/mir-client/tests/round59_projectile_render.rs`、`round59_projectile_assets.rs`、`docs/testing/round59/run-checks.sh` | D1,D2,D3 | D2/D3 后 |
| D5 | 礼部/gpt | 文档入树（G6） | `docs/client.md`、`docs/world/magic.md`、`docs/testing/round58/README.md:3`、新建 `docs/testing/round59/README.md` | D2 结论 | 与 D4 并行 |
| D6 | 刑部/gpt | 终审（G1–G7 复核 + 假绿扫描） | 只读；产出 `target/round59/xingbu/attempt-1/`（结论文档入树另议） | D4,D5 | D4&D5 后 |
| D7 | 尚书/gpt + Lead | 派发、门禁、提交（`git add/commit` 仅 Lead） | `crates/**` 已由工部改；Lead 只做门禁与 git | D6 | 最后串行 |

## §四 验收命令（每条证据落 `target/round59/<D>/attempt-<n>/`，`target/` 不入库）

```bash
# 逻辑测试（不依赖素材；0 tests 判 FAIL，不计 PASS）
cargo test -p mir-client --test round59_projectile_render
# 素材测试（缺 game-assets/ 时打印 SKIP 并单列，SKIP≠PASS）
cargo test -p mir-client --test round59_projectile_assets
# 回归
cargo test -p mir-client --test round58_projectile_wire
cargo test -p mir-client --test client_fx
cargo test -p mir-render
cargo clippy -p mir-client -p mir-render --all-targets -- -D warnings
# 素材刷新（本地；game-assets/ 被 .gitignore 忽略）
py -3 tools/assets/analyze_usage.py
cargo run -p mir-tools -- assets unpack --plan target/used-assets.json --out-root game-assets
cargo run -p mir-tools -- assets unpack --plan target/used-assets.json --out-root game-assets --verify
mkdir -p target/round59/audio-in
cp resource/community/gqf2008-crystal/Sound/M34-1.wav resource/community/gqf2008-crystal/Sound/M34-2.wav target/round59/audio-in/
cargo run -p mir-tools -- audio import --input target/round59/audio-in --out game-assets/audio/sfx --keep-source
cargo run -p mir-tools -- audio verify --input game-assets/audio
# 文档与静态
py -3 tools/docs/check_links.py
git diff --check
bash docs/testing/round59/run-checks.sh   # 9 项；任一项 0 tests 即整体 FAIL
```

判定：D4 的 `run-checks.sh` 必须 exit 0 且汇总 9 行 `OK`；门禁由 Lead 复跑并保留逐条原始 exit（GBK 原件 + UTF-8 转换件成对，沿用 R58 §4.3 约定）。

## §五 风险与真实降级

| # | 风险 | 真实降级（不得自创） |
|---|---|---|
| R1 | `game-assets` 缺 410..565/570..579（当前只拆 0..9） | G2/G3 的素材用例 SKIP，代码仍以真实 index 公式实现并在逻辑测试中断言；素材表现记 **PARTIAL**，禁以「占位色块」冒充 410 图元 |
| R2 | `M34-1/2.mna` 缺（现仅 `M34-0`），`audio import` 需本机 ffmpeg | 逻辑测试用 fake sink 断言文件名与次数；真实可听记 **PARTIAL**，禁用其它音效顶替 |
| R3 | 125 包**不带落点坐标**；Crystal 靠 `TargetPoint` 兜底 | `destination` 在快照中解析不到时：不生成投射物、不生成命中、不播 `+2`（因为客户端没有落点）；登记为已知边界，禁止自造坐标 |
| R4 | ring 16 丢最旧 | 每帧 drain 后「丢最旧」只影响高积压帧；本轮不改常量，只登记消费契约（R58 §4.9 已入台账） |
| R5 | `ProjectileEvent` 无时间戳 | 以 drain 时刻为 `started_ms`（与 `SpellEvent` 同法）；duration 用源格与目标当前格，Crystal 公式一致 |
| R6 | 目标移动/消失 | 每帧按 `Target.CurrentLocation` 更新落点（`Effect.cs:176`）；目标消失则用生成时落点收尾并跳过命中（`dead` 判定用 `EntityView.dead`，已存在 `entities.rs:331`） |

## §六 不做清单（本轮明确不做）

1. **④ Hero 实例与候选等价**：需服务端 Hero 实体，非 30 行内可证。
2. **⑤ 默认 DB 学习书/掉落生产路径**：需 DB/掉落表验证，属服务端数据面。
3. **⑥ `:1536` 攻城期 `can_fly`**：须先有攻城期回归用例，单列（R58 §3 已定「不合并」）。
4. **⑦ ring 扩容代码**：只登记契约，不动 `EFFECT_EVENT_LIMIT`。
5. **125 不回显施法者**（`World::broadcast` 不含本人）：服务端语义项，客户端不补偿。
6. **O-1 `encoding.log` 修正**：原件在 `target/`（树内不存在，§1.3 已核），不做。
7. **完整 Missile/Effect 类体系、光照、16 方向逐帧资源重制**：只按 Crystal 公式取真实帧，不重构表现框架。

## §七 待门下裁决项（≤5）

1. **渲染保真度口径**：允许「按 Crystal 公式算 index、直接用 `draw_effect(FxKind::Magic, index)` 画 `magic.wzl`」，不新增 Missile 类，是否判定为保真达标？（本方案默认：是，逐帧 index 与位置公式逐字对齐）
2. **音效是否本轮**：`M34-1/-2` 不在生成包，须本机 `audio import`（依赖 ffmpeg）。若不导入即记 G4 PARTIAL，是否接受「logic 绿 + 真实可听 PARTIAL」的收口？
3. **unknown destination 边界**：R3 的「无落点即不生成」是否可作为 R59 的正式边界（Crystal 有 `TargetPoint` 兜底，本作协议无此字段）？
4. **PARTIAL 门槛**：G1–G3 绿、G4/G5 因素材缺而 PARTIAL，是否判 **PASS-WITH-PARTIAL**（不阻断提交）？
5. **`docs/testing/round58/README.md:3` 的 HEAD 取值**：更正为 R58 交付 HEAD `2f3ab9f`（本方案默认），还是等 R59 提交后统一回填？
