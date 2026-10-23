# 自机移动插值（第 45 轮）：证据索引与覆盖边界

本目录是第 45 轮「自机走/跑插值 + 相机跟随」的**正式证据索引**。
实现与行为说明见 [`docs/client.md`](../../client.md) §4；后续体验清单见
[`docs/client-experience-backlog.md`](../../client-experience-backlog.md)。

> 一句话结论：公式与 **12 项** `camera.rs` 单测（公式 6 + 生命周期/生产前处理 6，X1-01）、
> 单方向 live_world 回归已通过；**96 行 CSV 是参考对照表，不是全组合动态覆盖**；
> **四类真实窗口边界未实测**；提交验收以**最终门禁后冻结的 D 批次 + 哈希**为准
> （历史 C 批次只作参考，不是提交选定），
> `build/evidence/self-motion/` 随时会被重跑覆盖。

## 1. 文件清单

| 文件 | 作用 | 入库 | 说明 |
|---|---|---|---|
| [`frame-comparison.md`](./frame-comparison.md) | Crystal ↔ Rust 逐帧对照、公式/符号/变换顺序、覆盖边界 | ✅ | 与本索引互相引用；§5 是动态覆盖边界，§7 是批次冻结，§8 是窗口未实测 |
| [`frame-comparison.csv`](./frame-comparison.csv) | **96 行完整参考对照表**（Walk 8 方向 × 6 帧 + Run 8 方向 × 6 帧） | ✅ | 「对应测试名」列只表示公式族归属，**不是逐行动态断言**；单测实际覆盖见 §3 |
| [`live-world-run.log`](./live-world-run.log) | 归档的 live_world 运行日志（Down 批次，17.88s，2 passed） | ✅ | 其中的 manifest 路径是当次 `build/` 临时产物，已被后续运行覆盖 |
| [`frozen/evidence-manifest.json`](./frozen/evidence-manifest.json) | **D 提交批次的 manifest**（Down；2026-10-24 00:29:48 最终门禁当次；18 样本） | ✅ | sha256 = `a3e874a9…dd7b40a`；18 PNG 在 `target/self-motion-frozen-final-20261024T0030/` 本地保留 |
| [`frozen-evidence.sha256`](./frozen-evidence.sha256) | **D 批次冻结清单**：manifest（build 原路径 + docs 入库副本）+ 18 PNG 的哈希 | ✅ | D 的 18 PNG + manifest；C 旧批次哈希仅作历史对照（`c08e2bc2…cf8`） |
| [`final-gate.md`](./final-gate.md) | 最终 12 项门禁记录、候选标识（父提交 + 7 个源码 blob）、D 冻结与残余限制 | ✅ | 12/12 全绿，退出码 0；批次 A/B/C/D 关系与不得混同说明 |
| `build/evidence/self-motion/`（本地） | 最近一次 live_world 的临时 manifest + 18 PNG | ❌ `.gitignore` | **不能作为提交证据**；门禁/测试重跑会覆盖 |
| `target/self-motion-frozen-final-20261024T0030/`（本地） | **D 批次**的 18 PNG + manifest 副本（19 文件） | ❌ `.gitignore` | 本轮提交用 D 批次冻结副本 |
| `target/self-motion-frozen-20261023T2205/`（本地） | 历史 C 批次的 18 PNG + manifest 副本（19 文件） | ❌ `.gitignore` | 仅作历史对照，不再作为提交选定 |

## 2. 证据批次（必须分开写，不能声称同一次运行）

| 批次 | 方向/链 | 时间（本地） | 产物 | 状态 |
|---|---|---|---|---|
| **A 归档日志** | Down 2 Walk + 1 Run：`(330,270)→(330,271)→(330,272)→(330,274)` | `live-world-run.log`：21:53 独立 `live_world` 运行（finished 17.88s） | `docs/testing/self-motion/live-world-run.log` | 日志已入库；当次 manifest 未保留 |
| **B 临时覆盖** | DownRight 2 Walk + 1 Run：`(330,270)→(331,271)→(332,272)→(334,274)` | manifest 生成 21:54:05（15431 B） | 当次 `build/evidence/self-motion/evidence-manifest.json` | **未冻结、已被覆盖**；只用于说明「方向相同 ≠ 同一次运行」 |
| **C 历史参考批次** | Down 2 Walk + 1 Run：`(330,270)→(330,271)→(330,272)→(330,274)` | 22:00 门禁重跑产物（manifest 生成 22:01:07） | `target/self-motion-frozen-20261023T2205/`（19 文件 = manifest + 18 PNG，本地） | **历史参考，不是提交选定**；旧 manifest sha256 `c08e2bc2…cf8` 仅作历史对照 |
| **D 提交批次** | Down 2 Walk + 1 Run：`(330,270)→(330,271)→(330,272)→(330,274)` | **2026-10-24 00:29:48 最终门禁当次**（`target/final-gate-20261024-002851.log`，12/12） | `docs/testing/self-motion/frozen/evidence-manifest.json`（sha256 `a3e874a9…dd7b40a`）+ `target/self-motion-frozen-final-20261024T0030/`（19 文件 = manifest + 18 PNG） | **本轮提交选定**；18 样本/18 PNG；哈希见 `frozen-evidence.sha256` |

要点：

- A、C、D **方向、参数与结果相同，但不是同一次运行**：A 是 21:53 独立 `live_world` 运行（17.88s），
  C 是 22:00 门禁重跑产物（manifest 22:01:07 生成），D 是 2026-10-24 00:29:48 最终门禁当次产物；不得声称同一次运行。
- **本轮提交选定为 D 批次**（最终门禁当次）；C 的 manifest/18 PNG 只保留为历史对照，不声称是提交选定；
  D 的入库 manifest + 哈希清单已更新，提交记录引用 D。
- B 是审核期间出现的 DownRight 覆盖（21:54:05），**未冻结**，不能引用为提交证据。
- 最终门禁已执行并冻结 D（12/12 全绿，见 `final-gate.md` / `target/final-gate-20261024-002851.log`）。
- **D 冻结集 = manifest + 18 PNG（19 文件），不含独立 log**；D 运行记录以 `final-gate.md` §2 与本地门禁日志为准（本地、不入库）；A 的 `live-world-run.log` 只属 A 批次。
- `build/` 与 `target/` 都被 `.gitignore` 忽略；测试/门禁重跑会覆盖 `build/evidence/self-motion/`。
  因此**不要依赖 build 临时路径**：提交用 D 批次的入库 manifest + PNG + 更新后的哈希清单
  （C 仅历史对照）。
- 证据性质：**权威回包快照冻结后的确定性时间重放**（每步 6×100ms），
  `rendered_at_ms` 是模拟采样时刻、`captured_at_wall_ms` 是实际渲染墙钟；不是窗口实时录屏/实时截图。
- 归档 log 中第一条用例 `live_client_walks_and_renders_a_frame` 打印「没有可用的地图素材，改用合成地图」
  属旧冒烟（不阻断）；第二条 G3 用例 `PASS 地形素材：game-assets/maps/0.mnm` 才是真实素材证据。

## 3. 动态覆盖范围（防夸大）

| 层 | 实际覆盖 | 不能声称 |
|---|---|---|
| 公式 | `camera.rs` **公式 6 个单测**：整数截断、`%2` 偶化、八方向符号、600ms 清除、非法跳变、`apply_camera` 自机居中/世界偏移 | 不能说「8 方向 × 6 帧 × 2 动作全部逐项动态断言」 |
| 生命周期 / 前处理 | `camera.rs` **新增 6 项（X1-01）**：200/600ms 延迟回包 Walk+Run 动作与 motion 同钟、同坐标快照不重启、后续权威步替换前一步、显式打断（攻击/死亡等）立即清、传送跳变清；走 `prepare_frame` 顺序 | 不能把动画层计时当成 motion 生命周期；不能声称 live 覆盖完整前处理（见下） |
| Walk 单测 | **右/下各 6 帧逐帧**；上/左与四个对角只锁第 5 帧 → 18/48 个「方向×帧」组合 | 不能说 Walk 48 组合全覆盖 |
| Run 单测 | **下 6 帧逐帧**；右只锁第 0/5 帧；右下只锁第 5 帧 → 9/48 个组合 | 不能说 Run 48 组合全覆盖 |
| CSV | 96 行完整参考对照表（公式期望值） | 不能把 CSV 行数当作动态断言数 |
| live_world | 每次运行只抽**一个方向链**（候选顺序 Down → DownRight → …）；归档/冻结批次都是 Down 的 2 Walk + 1 Run | 不能代表全方向/全帧动态验证 |
| 渲染合成 | `mir-render::render_at_offset_composes_global_and_entity_pixel_offsets` 证明全局偏移 + 实体偏移的合成顺序 | 不能替代像素级窗口实测 |
| 窗口边界 | **四类全部未实测**：中央移动 / 触边回拉 / 沿边 / 反向离边 | 不能改称通过 |
| ≥600ms / 跳变 | 只有代码级断言（`prepare_frame` 自然到期、`SelfMotion::new` 拒绝跳变、显式打断/传送/重连/切图清 motion） | **无窗口录屏佐证**，必须保留限制 |

> **live_world 分层限制**：`crates/mir-client/tests/live_world.rs:348` 的 capture 仍直接调用
> `camera::apply_camera`，**没有走 `prepare_frame` 的生产前处理**（tick_animations → 自然到期 →
> 显式打断）。延迟回包/打断/替换等生命周期回归由 `camera.rs` 单测覆盖；live 只作
> 「真实服务端 + 生产渲染入口」的分层证据，不能宣称 live 覆盖完整生产前处理。

> **FPS/帧率口径**：本轮**未测 FPS/帧率**；`docs/client.md` 中出现的性能数字仅第 13 轮历史数据，
> 不适用于本轮移动插值/相机改动。

## 4. 复核与验证

```powershell
# live_world：真实服务端 + 生产渲染入口（每次只抽一个方向链）
cargo test -p mir-client --test live_world -- --nocapture

# camera 单测（12 项：公式 6 + 生命周期 6；也可直接跑整个 client）
cargo test -p mir-client --lib camera

# 客户端 + 渲染门禁
cargo test -p mir-client -p mir-render
```

`frame-comparison.md` §9 记录了本轮 4 条门禁命令与独立复核结论
（camera 12/12（X1-01 后；公式部分早期独立复核 6/6）；CSV 0 mismatch、live_world 2 passed、
manifest 0 mismatch、地形位移 = `world_offset` 差）。
12 项全量门禁由尚书在最终候选上执行（[`../local-ci.md`](../local-ci.md)）。

## 5. 冻结校验（提交前）

以下校验的是 **D 提交批次**（最终门禁当次）；C 为历史对照，不再作为提交选定。
`frozen-evidence.sha256` 的 manifest 入库副本可直接校验；PNG 条目写的是冻结当时的
`build\evidence\self-motion\...` 原路径，复现时按文件名映射到 `target/self-motion-frozen-final-20261024T0030/`：

```powershell
# 20 条全部核对（manifest build 原路径 + docs 入库副本 + 18 PNG；build 路径按同名文件映射到 target 冻结目录）
$bad = 0
Get-Content docs/testing/self-motion/frozen-evidence.sha256 |
  Where-Object { $_ -and -not $_.StartsWith('#') } |
  ForEach-Object {
    if ($_ -match '^([0-9a-f]{64})\s+(.+)$') {
      $hash = $Matches[1]; $rel = $Matches[2].Trim()
      $path = $rel
      if ($rel -like 'build\evidence\self-motion\*') {
        $path = Join-Path 'target\self-motion-frozen-20261023T2205' (Split-Path $rel -Leaf)
      }
      $actual = if (Test-Path $path) { (Get-FileHash $path -Algorithm SHA256).Hash.ToLower() } else { 'MISSING' }
      if ($actual -ne $hash) { Write-Host "FAIL $rel"; $bad++ }
    }
  }
"bad=$bad"; if ($bad -gt 0) { exit 1 }
```

- 关键哈希（D 批次）：manifest `a3e874a9e33ead3606bcbd8b5c0cd79b911abb70721585f1847d78592dd7b40a`；
  D 运行记录见 `final-gate.md` + `target/final-gate-20261024-002851.log`（12/12）。
  历史 C 旧 manifest `c08e2bc285a694f6e1cebe65ee2c1d1ae169a917f263dead2bea57c18a605cf8` 仅作对照；
  归档 log（A 批次）`012291349fa239d00cd543f303e40c158f636bb8c1a1278bb4268f18b05e4bcd`。
- 冻结之后若又跑过测试/门禁，`build/` 下文件会被覆盖；这属预期，**以入库 manifest + PNG 副本 + 哈希为准**（当前为 D 批次）。

## 6. 最终验收口径

1. 公式/单元测试：以 `camera.rs` **12 项**（公式 6 + 生命周期/前处理 6）与 `mir-render` 合成测试为准（不是 96 组合全覆盖）。
2. 真实服务端：以**最终门禁后的 D 批次**为准（C 为历史参考；同参数/同结果、18 样本/18 PNG、Down 单方向链）。
3. 真实窗口：**四类边界仍未实测**，≥600ms/跳变仍无窗口录屏；这是保留的残余风险，
   不得在验收文档中改称通过。
4. 12 项全量门禁：已在最终候选上执行（`final-gate.md`，12/12，退出码 0）；
   提交时引用 **D 批次**冻结副本与更新后的 `frozen-evidence.sha256`；
   门禁后仅允许证据/文档归档变化，源码 blob 不变；若源码再变则重新冻结并重跑。

## 7. 关联文档

- [`docs/client.md`](../../client.md) §4「移动插值」与 §8「尚未实现」（限制与后续方向）
- [`docs/client-experience-backlog.md`](../../client-experience-backlog.md)——本轮登记的后续体验清单
- [`docs/testing/local-ci.md`](../local-ci.md)——12 项本地门禁
- `github/Crystal` HEAD `0e315fe327192afe52c3d7357ddd1f5b7e26c5b8`（源码行号见 `frame-comparison.md` §1）

## 8. 修订记录

- 2026-10-23（工部 G1/G3）：首版逐帧对照、CSV、live 证据。
- 2026-10-23（礼部 L1，MX-01/MX-02）：明确 CSV 仅为参考对照、单测动态覆盖有限、
  live_world 只抽 Down 链；分开 A/B/C 批次；改为引用入库 frozen manifest + 哈希；
  保留四类窗口未实测与 ≥600ms/跳变无录屏限制；按 gongbu 复核修正地图边缘 clamp 口径
  （正常落点均在界内，clamp 为 no-op，自机仍严格屏幕居中）。
- 2026-10-24（礼部 L1，X1-01 delta）：camera 单测由 6 项更新为 **12 项**（新增 6 项生命周期/
  前处理回归）；§4 生命周期改为「motion 起点 = 网络层应用 `S.UserLocation` 时刻 +
  `prepare_frame` 按 `motion.started_ms` 自然到期 + 显式打断另行清理」；补 live_world capture
  直接 `apply_camera`、未走 `prepare_frame` 的分层限制。
