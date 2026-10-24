# R49 D · 怪物原生帧表、双模式与门禁证据（历史批次）

批次：`1792847715360`（`manifest.json` 的 `batch`）。本目录只写 R49 D 证据；R49 起攻击 live
测试只写 `target/attack-variants/`（gitignored），**不改** `docs/testing/attack-variants`
（R47，只读）与 `docs/testing/native-player-layout`（R48）冻结目录。证据链当前为 **PARTIAL**
（`raw-manifest.json` 的生成时间晚于本目录批；磁盘测试 exe 与 `client_binary.sha256` 不同），
差异见 §8；当前工作树最终全量门禁未跑，不沿用历史批次结论。

## 1. 数据源与范围

- 默认资产：`game-assets`，`AssetMode::Auto` 解析为 `Dev/unpacked`（`library-map.json` + `gfx`）。
- release：只给 `packs` + `packs/library-map.json`，不读 `gfx/data/resource`，断言 `source_kind=pack`。
- Crystal 参照：`github/Crystal` HEAD `0e315fe327192afe52c3d7357ddd1f5b7e26c5b8`。
- 怪物样本：`monster-005`（demo）、`monster-022`（高频）、`monster-039`（A0 按
  `spawn_points` 合计的第三高频，761）、`monster-023`（结构合格替代）。
- 动作：Standing/Walking/Attack1/Struck/Dying/Dead 真实渲染；CSV 额外覆盖 frames.json
  中存在的全部动作与 8 方向，不存在的动作标 `present=false`。

## 2. 文件

| 文件 | 内容 |
|---|---|
| `manifest.json` | 批次、源/资产/pack 哈希、52 张 PNG 的 png/crop sha256、36 个样本、2 个双模式同场、4 库动作表、限制 |
| `monster-frame-table.csv` | 4 库 × 全部动作 × 8 方向，共 224 行；`expected_first/actual_first` 对照 Crystal 整数公式 |
| `png/` | 24 个玩家样本 ×2 张 + 4 怪 ×6 动作 + 2 个双模式场景 ×2 张，共 52 张 |
| `player-resource-freeze-before.json` | A2 前玩家 262 键、372448 PNG 的逐文件 sha256 |
| `player-resource-freeze.json` | A2 后玩家同结构数据、前/后相等标志与 4 项 native_player_layout 输出 |
| `freeze.json` | 本批次证据、门禁日志、玩家冻结文件的聚合哈希与采样批次门禁历史结果（当前工作树 PARTIAL，不沿用） |
| `raw-manifest.json` | 渲染测试原始输出；最终 manifest 的可追溯输入 |

## 3. 双模式同场

`crates/mir-render/tests/monster_native_evidence.rs` 在同一候选上分别构造：

- dev：`EntityLibraries::open_with_sources(..., gfx_dir=Some(game-assets/gfx), packs...)`；
- release：`EntityLibraries::open_with_sources(..., gfx_dir=None, asset_root=game-assets/packs, packs=manifest...)`。

同一 `mixed_views`（5 职业男女、刺客/弓手 alt true/false、005/022/039/023 怪）只替换
renderer，两模式逐像素 `pixel_diff=0`；`source_kind` 分别断言 `unpacked`/`pack`；
`self-still`/`self-moved` 两个相机中心都验证有可见像素且无 Standing 回退。两模式不
使用 `--features legacy`。

## 4. 玩家资源回归

- `player-resource-freeze-before.json`（A2 前）与 `player-resource-freeze.json`（A2 后）的
  `player_library_keys_sha256`、262 个 `index.json` 哈希、372448 个 PNG 哈希全部相同。
- `player-resource-freeze.json` 保留 after 的全量逐文件 PNG 哈希，并记录 before 文件路径与
  聚合哈希；两份文件都在本目录，不需要去 `target/` 找前置证据。
- 必需键 `carmour-00/carmour-24/aarmour-00/ararmour-00/cweapon-00/aweapon-00-r/aweapon-00-l/arweapon-00/arweapon-00-s/chair-00/ahair-00/arhair-00` 全部存在（`freeze.json` 的 `required_key_presence` 记录）。
- pack 容器 hash 因 R49 怪物 pack 重建而变化，属预期；玩家 entry 以 index/PNG 哈希为准。

## 5. 复现

```text
TEMP=target/tmp TMP=target/tmp cargo test -p mir-render --test monster_native_evidence -- --ignored --nocapture
TEMP=target/tmp TMP=target/tmp cargo test -p mir-render --test native_player_layout -- --ignored --nocapture --test-threads=1
TEMP=target/tmp TMP=target/tmp cargo test -p mir-render --test monster_native -- --ignored --nocapture
TEMP=target/tmp TMP=target/tmp cargo test -p mir-tools --test monster_native_import -- --ignored
TEMP=target/tmp TMP=target/tmp cargo test -p mir-client -p mir-render
py tools/docs/check_links.py
pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale
# 若 journey_natural 在并行全量测试中抖动（自然鸡价断言），isolated 复跑并保留原日志；
# 历史采样批次（1792847715360）的门禁记录见 freeze.json；当前工作树最终全量门禁未跑，PARTIAL。
```

## 6. 计数与哈希

- 52 个 PNG，38 个唯一 PNG sha256、38 个唯一 crop sha256；36 个 manifest 样本（12 玩家案例 + 24 怪动作帧）。
- 每个 PNG 为 800×600 全场景；`crop_bbox` 为非透明像素包围盒，`crop_sha256` 对裁剪后的 RGBA 字节取哈希。
- manifest 的 `samples[]` 与 `png_files[]` 均含 `png_sha256`、`crop_sha256`；玩家样本含 `standing/attack1` 两份。
- 怪物源 `.Lib` sha256 来自 A0 `target/r49/used-monster-images.json`；sample 的
  `source_sha256/source_path` 逐项写入 `manifest.json`。
- 原生 pack 共 **12 个**（`sprites-monster-native-1..12.mnp`；第 12 个是 `monster-184`
  独立前缀的 `sprites-monster-native-12.mnp`，689 个条目），合计 **29,762 个条目、
  183,611,378 字节**；`native_pack_set_sha256` 为按文件名+sha 排序的聚合哈希；每个 sample
  的 `pack_sha256` 指向该聚合（不是单 chunk）。旧口径“11 包 / 29,380 条目 / 167,384,604 字节”
  是拆出第 12 包前的值，已过期。
- 门禁日志：`target/gate-r49-d.log`（采样批次的历史日志）；当前工作树未跑最终全量门禁，
  `current_gate=PARTIAL`，不能把该日志当成当前结论。历史失败日志
  `target/gate-r49-d-first-fail.log` 仅作诊断。

## 7. PARTIAL 与未测

- 逐帧真实渲染只覆盖 005/022/039/023；其余 used 库只有结构/计数证据。
- Revive/Attack2/3/4 等动作在部分库缺表，CSV 标 `present=false`，不回退其他动作。
- Push/Jump/Dash、坐骑帧表、窗口四边界、FPS 未测。
- `Dying` 按表夹末帧；Dead 由客户端 `tick_animations` 推进，不由帧表自动切。
- 时间戳：证据为确定性模拟时间 + 实际抓取时间分离，非回环 TCP live 采样。

## 8. 证据链状态（2026-10-25 复核，PARTIAL）

| 项 | 值 |
|---|---|
| `manifest.json` 批 | `1792847715360` |
| `manifest.json` sha256 | `d3b1d4c4c89b3484…`（与 `freeze.json.evidence_manifest_sha256` 一致） |
| `monster-frame-table.csv` sha256 | `c05bb902a2aa9e07…`（与 `freeze.json` 一致） |
| `player-resource-freeze-before.json` | 44,844,165 B，`70845aefd4dd689a…` |
| `player-resource-freeze.json` | 45,219,533 B，`c7d896e7f5eee470…` |
| `raw-manifest.json` 生成时间 | `1792865639318`（**晚于**本目录批，说明渲染测试后又跑过） |
| `manifest.client_binary.sha256` | `00b3df83ea96a9c9…`（采样批次的值） |
| 当前磁盘同名测试 exe sha256 | `da0ab79ad6660a06…`（**不同**） |
| `baseline.source_blobs` | `entities.rs`/`animation.rs`/`camera.rs` 与当前工作树不同；`player_layout.rs` 相同 |
| `docs/testing/attack-variants/` | 复核时冻结产物 `git status` 为 0（仅 README 在 R49-05 修订）；审计时曾出现 27 个 M，0 diff 只是采样结论、需队长恢复/复核 |
| 当前工作树最终门禁 | **未跑，PARTIAL**；不沿用历史批次结论 |

> 两份 44/45 MB 玩家冻结 JSON 超过 1 MiB 入库上限，只在本目录保留并在
> `docs/testing/round49/wip-freeze.json` 登记路径/字节/SHA256，仓库不提交其内容。
> 这些哈希只证明采样批次（`1792847715360`）的文件状态，不证明当前工作树通过。
