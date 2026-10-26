# 第 56 轮（R56）文档与证据索引

**本轮范围**（主单 `JJC-20261026-003`，受测 HEAD `e3b365f`，只读轮）：
- **G1** AOI 毫秒边界 flake 去根因：只改 `crates/mir-server/tests/aoi.rs`（冻结时钟 + 确定性边界新例）。
- **G2** AOI-5 生产接线裁决：不发明 NPC 生命周期，给出「无可接线点」的可证伪证据。
- **G3** 文档同步：`docs/world/aoi.md` 新增 §8.9 + 本索引。

**口径**：本文件只记录已落盘文件与命令的原始 exit。状态列只允许「已落盘」「待 T 抽核」；
未执行项写「未运行」；判定权归门禁后的 T 与 Lead。

## 1. 本轮交付与状态

| # | 交付 | 内容 | 状态 |
|---|---|---|---|
| 1 | `crates/mir-server/tests/aoi.rs` | G1：`advance_clock(0)` 冻结 + `normal.expire_time == coin.expire_time` 断言 + 新例 `ground_item_expiry_strict_ms_boundary`（`--list` = 14） | 待 T 抽核 |
| 2 | `docs/world/aoi.md` | G3：新增 §8.9「NPC 回收原语与生产接线状态」五条证据（Rust 行号 + Crystal 锚点） | 已落盘 |
| 3 | `docs/testing/round56/README.md` | 本索引（≤60 行） | 已落盘 |

## 2. 证据索引

| 路径 | 内容 |
|---|---|
| `target/round56/aoi/attempt-1/` | G1 原件：`--list` 发现数、两条单例运行日志（各 exit 0）、100 次重复逐次 exit、旧树注入 sleep 首红、新树同注入复跑、日志 sha256 |
| `target/round56/gate/attempt-1/` | Lead 唯一全量门禁 `pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale` 的原始 stdout/stderr、逐项 exit、HEAD、文件 sha、`log_bytes`/`log_sha256` |
| `target/round56/docs/attempt-1/` | 本轮文档自检原件：`00-evidence-greps.log`、`01-check-links-before.log`、`02-check-links-after.log`、`03-git-checks.log`、`04-grep-redline.log` |
| `docs/testing/round56/round56-final-audit.md` | T 门禁后只读终审（逐项带原件路径 + sha） |
| `docs/testing/round56/round56-final-manifest.json` | T 机器可读清单（HEAD / 文件 sha / 门禁 12 项） |

## 3. 文档自检（原始 exit，原件在 `target/round56/docs/attempt-1/`）

| 检查 | 命令 | 原始结果 |
|---|---|---|
| 链接完整性 | `py -3 tools/docs/check_links.py`（落盘前 / 落盘后各一次） | exit 0，见 `01`/`02` 日志 |
| 空白错误 | `git diff --check` | exit 0，见 `03` 日志 |
| 三个红线禁用词 | 本文件全文 grep → **0 命中**；`docs/world/aoi.md` 只核 §8.9 新增行范围 → **0 命中** | 见 `04` 日志（含带行号范围的原始命令） |
| 行数 | `wc -l` 两文件 | 见 `03` 日志 |

## 4. 承接说明

- **roadmap 脚注差异留待 R57 统一**：本轮不改 `docs/roadmap.md`，避免与 R55 基线脚注冲突；
  差异不在本轮静默丢失，R57 统一对齐。
- `docs/world/aoi.md` §8.9 只记录「原语存在 + 生产零调用者」；AOI-5 整体维持 PARTIAL，
  闭合判定留给 T 与 Lead。
- 既有缺陷 `crates/mir-server/src/world.rs:1378` 注释 `advance_clock` 首字母为 0x07(BEL)
  控制字节；本轮禁改 `src/`，留 R57。
- R56 不宣称 AOI 全量验收或本轮完成；以门禁 attempt-1 的 12 项原始 exit 与 T 抽核为准。
