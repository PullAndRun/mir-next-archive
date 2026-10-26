# 第 59 轮（R59）尚书省派发件

> 派发：尚书省（shangshu，gpt-6.1-sol）｜计划 `docs/process/round59-plan.md` 120 行/13,225B/sha256 `5a233b58…`｜
> 评审 `docs/process/round59-menxia-review.md` 53 行/10,257B/sha256 `7fce80fa…`，**PASS-WITH-CHANGES**（C1–C5 裁决 + M1–M3 必改逐条纳入本件）｜
> 基线 HEAD = origin/main = `3cb1df1`｜口径：**最小闭环 + PARTIAL**；禁称「125 全链」「渲染完整」「与 Crystal 逐字一致」「G1–G7 全绿」。

## §一 边界与串行

- **D1 工部/deepseek（G1,G2）** 写范围：新建 `crates/mir-client/src/ui/projectile.rs`；`crates/mir-client/src/ui/mod.rs` 挂载与绘制调用；只调用 `crates/mir-render/src/scene.rs` 的 `draw_effect`，不改其签名。
- **D2 工部/deepseek（G3,G4 代码）** 依赖 D1→D2；写范围：`crates/mir-client/src/ui/projectile.rs` 完成回调；`crates/mir-render/src/sound.rs` 增 `projectile_sound`。
- **D3 工部/deepseek（G5）** 与 D1 并行（写范围不重叠）；写范围：`tools/assets/analyze_usage.py`（`:324` 常量）；本地重生 `game-assets/`（不入库）。
- **D4 兵部/deepseek（G1–G4 证据）** 依赖 D1,D2,D3；写范围：`crates/mir-client/tests/round59_projectile_render.rs`、`crates/mir-client/tests/round59_projectile_assets.rs`、`docs/testing/round59/run-checks.sh`。
- **D5 礼部/gpt（G6 文档）** 依赖 D2 结论；写范围：`docs/client.md`、`docs/world/magic.md`、`docs/testing/round58/README.md:3`、新建 `docs/testing/round59/README.md`。
- **D6 刑部/gpt（终审）** 依赖 D4,D5；只读，不改工作树；产出 `target/round59/xingbu/attempt-1/`。
- **D7 尚书+Lead** 派发、门禁、git；`cargo/target` 由 Lead 串行；`git add/commit` 仅 Lead。
- D6 必须在 Lead **唯一**全量门禁 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`（`*>` 完整捕获落盘）之后启动。

## §二 逐单契约（D1–D6：写范围 / 验收 / 证据 / 禁止）

证据一律落 `target/round59/<d>/attempt-<n>/`（`target/` 不入库）；验收命令逐条记原始 exit；`--list` 发现数为 0 判 **FAIL**；缺素材打印 **SKIP≠PASS**；禁止项对所有单通用：改协议、新增依赖、`#[allow]`、中文断言、自创 `Missile` 类、新增 `FxKind` 变体、改 `start_frame()`。

| 单 | 验收命令（精确） | 额外禁止 |
|---|---|---|
| D1 | `cargo test -p mir-client --test round59_projectile_render`；`cargo test -p mir-client --test round59_projectile_render -- --list`；`cargo clippy -p mir-client -p mir-render --all-targets -- -D warnings` | 不得在绘制路径 clone 快照绕 `&mut`；`mem::take` 需 `&mut WorldSnapshot`，只持 `&` 则改游标 |
| D2 | `cargo test -p mir-render`；`cargo test -p mir-client --test round59_projectile_render`；fake-sink 断言 `M34-1.wav`/`M34-2.wav` 字面量与次数（出生 1/命中 1） | 不得只断言「非空」；不得复用 `spell_sound` 的 `M{spell}-0` 命名 |
| D3 | `py -3 tools/assets/analyze_usage.py`；`cargo run -p mir-tools -- assets unpack --plan target/used-assets.json --out-root game-assets`；同命令加 `--verify`；`cargo test -p mir-client --test round59_projectile_assets` | 不得手删/手改 `game-assets`、`resource` 下文件；空帧不得计 missing、不得产 0×0 PNG |
| D4 | `bash docs/testing/round59/run-checks.sh`（9 项，任一项 0 tests 即整体 FAIL）；`cargo test -p mir-client --test round58_projectile_wire`；`--test client_fx`；`cargo test -p mir-render` | 不得改 `net.rs` 125 入队条件与 `EFFECT_EVENT_LIMIT`；不得以 SKIP 计 PASS |
| D5 | `py -3 tools/docs/check_links.py`（exit 0、断链 0）；`git diff --check` | 不得写 R59 自身 HEAD 进 R58 记录；不得改 R58 已收口证据 |
| D6 | 只读复核 G1–G7 + 假绿扫描：`--list` 计数、证据 sha、原始 exit 与日志对照 | 不得修改任何受测文件；不得以结论文档掩盖缺证 |

## §三 必须纳入的 M1–M3（门下必改，逐字落口径）

- **M1 帧集精确**：`magic.wzl` 的 416–419、566–569 是 **0×0 空帧**（fmt0/zlen0）；`analyze_usage.py:324` 的 `MAGIC_FRAMES` 必须改显式段 `[i for d in range(16) for i in range(410+d*10,416+d*10)] + list(range(570,580))`（或等义过滤）。D3 证据须断言「空帧不计 missing、不产出 0×0 PNG」；禁 `range(410,566)` 带进 60 个空帧。
- **M2 音效基准 = GreatFireBall(34)**：`projectile_sound` 以 `GameScene.cs:4697` 的 `GreatFireBall=34` 为基产 `M34-1.wav`（出生）/`M34-2.wav`（命中），**不得**用 FireBounce=154 拼 `M154-…`；须显式常量 + 注释引用 `:4697/:4709`；fake-sink 断言字面量与次数（出生 1/命中 1）。
- **M3 保真与偏离登记三连**：①`draw_effect/effect_image` 实为 `index + kind.start_frame()`（`entities.rs:1801/1807/1825`；`Magic=0`）→ D1/D2 只用 `FxKind::Magic`、禁改 `start_frame`，G3 断言命中 `magic.wzl[410]/[570]`；②D-R59-1 入 `docs/client.md` 与 `docs/testing/round59/README.md`，逐行引用 `GameScene.cs:4699`（无条件生成投射物）与 `:4701`（仅命中受 `Target != null` 门控）；③G2 须登记「Crystal `Effect.cs:174-181` 每帧重算 `Duration/Count`，本作生成时冻结」，不得写作逐字一致。

## §四 C1–C5 裁决落点（门下裁决，逐条执行）

- **C1** index/位置公式逐字照 `Effect.cs:196/206-212`：`index = 410 + f%6 + dir*10`、位置 `lerp(Source, 该帧 Target.CurrentLocation, f/Count)`、`dir = Direction16(Source,Destination)`；不建 `Missile` 类，但不得写作「Effect/Missile 体系已移植」。
- **C2** ffmpeg 9.0.2 已在 PATH（本机实测）→「缺 ffmpeg」不构成 PARTIAL；D3/D4 必须真跑 `cargo run -p mir-tools -- audio import --input target/round59/audio-in --out game-assets/audio/sfx --keep-source` + `audio verify --input game-assets/audio` 且 exit 0；**仅真失败贴原始 stderr** 才许记 PARTIAL；`.mna` 证据（日志 + sha256）落 `target/round59/`。
- **C3** unknown destination **不生成**投射物/命中/`+2`，属诚实降级但**与 Crystal 有差**：登记 **D-R59-1** 偏离，不得称对齐；出生音效随投射物生成，无投射物则无声并留断言。
- **C4** 验收只许三行式「绿 N 项 + PARTIAL M 项 + 依据」；逻辑门（G1–G3、G7）不得 SKIP，0 tests 判 FAIL；`draw_effect(Magic,570)==true` 在 D3 未落地时不得计 PASS。
- **C5** `docs/testing/round58/README.md:3` 的 `1a9aeb3` → `2f3ab9f`（R58 交付 HEAD，边界父不写），只改该 1 行，并在 `docs/testing/round59/README.md` 记一行更正理由。

## §五 黑名单与红线

- 禁称「125 全链」「渲染完整」「与 Crystal 逐字一致」「G1–G7 全绿」；只许「最小闭环 + 已知偏离/ PARTIAL」表述。
- 禁改 R58 已收口证据（`docs/testing/round58/*` 除 `README.md:3` 该 1 行）；禁把 R58 结论翻案。
- 禁手删/手改 `game-assets/`、`resource/` 下任何文件；素材缺失不得伪造、不得以占位色块冒充 410/570 图元。
- 禁改协议字段、禁新增依赖、禁 `#[allow]`、禁英文/中文混写断言以外的新造口径；禁自创 `Missile`/`Effect` 类体系。

## §六 门禁与提交

- 两次提交（仅 Lead）：**代码批** `D1/D2/D3/D4`（`crates/**`、`tools/assets/analyze_usage.py`、`docs/testing/round59/run-checks.sh`）；**文档批** `D5`（文档 4 处）；门禁后 `D6` 审计三件由 Lead **第三次**提交。
- 门禁唯一入口 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`，`*>` 完整捕获；门禁绑定：受测 HEAD 全 sha、13 候选 sha、逐条原始 exit、日志落盘路径。
- 提交前 `git status --porcelain` 与证据 sha 对照；提交信息记 R59 与 D-R59-1；不 amend、不 force push。
