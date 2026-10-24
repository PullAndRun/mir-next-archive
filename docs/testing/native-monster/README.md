# R49 A2 · 怪物原生帧导入证据

本目录记录 R49 A2 的 **怪物 `.Lib` 原生帧表导入**。目标不是“怪物能显示”，而是把
Crystal 怪物帧表语义落到可复现、可复核、可被发布包消费的 `frames.json`，并明确
哪些库因源数据限制不能伪装成 native。

## 1. 输入与冻结

A0（t39）已冻结：

- SQL 结果：`target/r49/used-monster-images.json`（122 个 used appearance + demo 5；
  SQL sha256 `315d8b76e42ed5b2ee72421cf94421461693f5ba33f48a87af52d21039a073ed`）。
- 源审计：`target/r49/source-audit.json`；像素证据：`target/r49/A0-evidence.md`、
  `target/r49/core-pixel-audit.json`。
- 源根：`resource/community/mirfiles-alliance/Alliance/Data/Data(Mount-Npc...)/Monster`。
- 122 used 中：118 个有 v3 帧表可导入；4 个是 v2（无 frameSeek，不能按 `Frames.cs` 语义读）；
  全库另有 >800 个 appearance 无对应 `.Lib`，保留为 missing（不伪造成“全 122 已原生”）。

A0 的 `005/022/039` 是 spawn_points 前三（039 合计 761，高于 023）；A2 导入清单
按 A0 结果生成，而不是按早期 023。验收要求的 `005/022/023` 三库也在清单中，
计数分别为 `234/244/224`。

## 2. A1 帧表解析

`crates/mir-asset/src/frames.rs` 与 `crates/mir-asset/src/lib_container.rs`：

- 35 字节 `Frame`：8 个 `i32` + 2 个 `bool`（u8）+ 1 个 action 字节；动作名映射
  `0=Standing,1=Walking,2=Running,9=Attack1,10=Attack2,11=Attack3,12=Attack4,18=Struck,
  21=Die,22=Dead,28=Revive`；未知 action 保留 `raw_action`。
- 支持 v1/v2/真 v3/v3 无 frameSeek；v4 明确拒绝。
- 有帧表但内容损坏 → `Err`；无帧表 → `None`（下游才走 `DefaultMonster`）。
- 负 count/seek、越界、方向步长**为负**、重复动作、非法 bool、Skip=-1 静态帧均有测试。
  `Count>1 && Count+Skip==0`（stride 0）是 Crystal 合法的「8 方向共用同一批帧」，
  不再误拒。

## 3. A2 导入

计划：`target/native-layout/native-monster-all-plan.json`（118 个 v3 库）；t46 的 10 个
stride=0 库单独用 `target/native-layout/native-monster-stride-zero-10.json` 重导。

- 从 A0 的 118 个 v3 帧表库中，导入了 **118 个**；t40 首轮导入 108 个，t46 重导了
  `monster-010, 041, 043, 062, 117, 130, 131, 132, 164, 218` 这 10 个
  `Count>1 && Count+Skip==0`（8 方向共用帧）的库，并生成 `frames.json`。
- 4 个 v2 和 800 个缺源库保持 A0 的 missing/old 状态，不进入本批 native。
- 生成：118 个目录、**29,644 张非空 PNG**（空帧 skipped，不写 0 字节）。
- 每个目录：
  - `index.json`：`count`、`written_png`、`skipped_empty`、`images[left/top/width/height/offset_x/offset_y]`；
  - `frames.json`：`schema_version`、`image`、`library_key`、`source_hash`（兼容
    `source_sha256`）、`layout: "v3_frames"`、`frame_source: "native"`、`layout`、
    `count`/`frame_count`、`actions[]`（含 `action/raw_action/start/count/skip/interval/
    effect/reverse/loop/frames_by_direction`）；`frames_by_direction[8]` 按
    `start + d*(count+skip)`。
  - PNG；`source` 仅作来源审计记录，A2 运行时不回读 `resource/`，只读 unpacked
    `game-assets` 或 release pack。
- 默认消费根是 `game-assets`；发布消费根是 `game-assets/packs`。

复导入命令（真实源，非测试 fixture）：

```bash
cargo run -q --release -p mir-tools --bin mir-tools -- assets unpack \
  --plan target/native-layout/native-monster-all-plan.json \
  --out-root game-assets --format png --verify
```

## 4. 测试与证据

已执行（本机）：
- `cargo test -p mir-tools`：库测试与集成测试；真实导入测试为 ignored。
- `cargo test -p mir-tools --test monster_native_import -- --ignored --nocapture`：
  真实 `005/022` 导入 + `frames.json` 7 动作 + pack 往返 + 只读 pack 回读；
  `--verify` 篡改 `frames.json` 必须失败；118 库 pack 存在性测试；核心
  `005/022/023` 计数 `234/244/224`。
- `target/r49/a2-unpack-all.log`、`target/r49/a2-unpack-verify-2.log`：
  108/108 成功；t46 的 `target/r49/t46-unpack-10.log` / `t46-verify-10.log`
  追加 10/10 成功、903 张 PNG；`gfx/sprites/monster-native/**` 指纹一致。
- pack 重建：`target/r49/t46-pack.log`，release manifest
  `game-assets/packs/manifest.packs.json`：96 个 pack、总 1,229,460,254 字节；其中
  `sprites-monster-native-*` **12 个、29,762 个条目、183,611,378 字节**（第 12 包是
  `monster-184` 独立前缀、689 个条目；旧口径 11 包 / 29,380 条目 / 167,384,604 字节
  已过期）。`t46-pack-verify.log` 只读回读 96/96、561,089 个条目全部覆盖。
- 当前工作树最终全量门禁未跑：`current_gate=PARTIAL`，本目录不写“12/12”固定结论；
  历史采样批次的门禁日志见 `docs/testing/monster-native/freeze.json` 的 `gate` 字段。

## 5. A3 渲染与动作生命期（R49 B）

`crates/mir-render/src/entities.rs` 的 `EntityRenderer` 在 `EntityKind::Monster` 且
`image <= 800` 时，按 `monster-{image:03}` 延迟读取同目录的 `frames.json`：

- 动作直接用 A2 的 `raw_action`：0 Standing、1 Walking、2 Running、9 Attack1、
  18 Struck、21 Dying、22 Dead；**没有**按 Crystal 枚举值猜表的映射。
- 每个方向帧号 = `start + direction * (count + skip) + local`；局部帧按
  `interval` 推进，Standing 循环，其余动作夹到最后一帧。
- 动作生命期（只决定何时回 Standing/Dying→Dead，不改变逐帧帧号）由
  `monster_action_duration_ms(action, frames)` 按 `Count × Interval` 从
  `frames.json` 派生：Standing 2000ms、Walking/Running 600ms、Attack1 600ms、
  Struck 2×200=400ms、Dying 1000ms、Dead 永不再转。无表/缺动作才回退
  `Frames.cs:25-31` 的 DefaultMonster（Struck 400ms）。
- 缺 `frames.json`、坏表或动作表缺失：该实体退回旧的扁平逻辑（与原行为一致），
  并只在第一次打印诊断；**不会**用 `DefaultMonster` 的整表回退去伪造 native。
- 原生表存在但某个动作缺表（例如 005 没有 Running）时，该帧安静跳过，
  不借用 Standing/Attack1 的帧。

真实素材证据：`crates/mir-render/tests/monster_native.rs`（ignored）直接加载
`game-assets`，渲染 `monster-005` 的 Standing/Walking/Attack1/Struck，逐帧断言
非背景像素，并写 `target/r49/t41/monster-005-*.png`。本机结果（220×280 RGBA）：

| 样本 | 非背景像素 | PNG sha256 前 16 位 |
|---|---:|---|
| standing | ≥500 | `4622b8054fa991eb` |
| walking | ≥500 | `a1c302fc03131c30` |
| attack1 | ≥500 | `270f3e7a4d9400c0` |
| struck | ≥500 | `52f50d081711d4f2` |

同一测试还断言：005 无 Running 动作时输出画布 **0 个非背景像素**，证明“有表但缺动作”
不会借 Standing/Attack1，也不会退回占位方块。005 的原生表为
Standing 4×500、Walking 6×100、Attack1 6×100、Struck 2×200、Dying 10×100、
Dead 1×1000；`net.rs` 用 `monster_action_duration_ms(action, None)` 的
DefaultMonster 回退（Struck 400ms），渲染层持有 `frames.json` 时按实际
`Count×Interval` 派生。


## 6. 限制（必须保留）

- A2 只做到“原生帧导入 + 只读 pack 消费”，**不做 A3 渲染/动画/亮度/特效**；
  `DefaultMonster` 的整表回退、A4 门禁不在本目录证据范围。
- 4 个 v2、800 个无源 appearance 是真实限制；不得把它们写成
  “全部 122 used 都已 native”。118 个 v3 used 已全部 native 导入。
- 非核心 used 只做结构/帧表检查，未逐像素解码；核心 `005/022/039` 已逐像素审计。
- 没有 `.mnd` 文件；A0 按 Crystal appearance→`Monster/NNN.Lib` 直接映射，未伪造
  `.mnd` 记录。
- 未导入 `game-assets` 之外的源到仓库；生成资产在运行时目录，不提交。
