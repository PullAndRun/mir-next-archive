# R70 尚书省验收（第70轮：混合特效逐帧偏移生产入口）

- 主单：`JJC-20261027-007`；审计 HEAD `91c1273`（功能提交 `6a08c65`）。
- **verdict：ACCEPT-WITH-PARTIAL（限定「混合特效逐帧偏移生产入口」切片），必改 0。**
- 执行方式披露：尚书 gpt 子代理 attempt-1（`b7e019d3`）/attempt-2（`4134fbbf`）/attempt-3（`2766a123`）连续失败且未落盘验收文档；按 R61 先例与 `docs/process/round70-lead-ruling.md`，由 Lead 代笔执行机械验收，证据根 `target/round70/shangshu/attempt-2/`（核验脚本 `checks.sh` 由 Lead 编写、逐命令留日志）。本文件不冒充 gpt 尚书署名。

## 一、看板与范围

- 四子单均 Done：A `sub-d0d00e43`（06:30:57Z 兵部）、B `sub-845fa668`（06:36:27Z 工部）、C `sub-1bdf62e4`（06:39:42Z 刑部）、D `sub-67ec1a50`（07:36:25Z 礼部，Lead 对 D 交付事实回填 delegate-result EXIT0）；主单 Doing（待本验收转 Review）。
- 范围：仅 `entities.rs`（`draw_effect_blend`/私有 impl/`blit_cached_add`）+ `scene.rs` 转发 + `frame.rs` 既有单测区新增 + 新测试 1140 行；`crates`/`tools/dev/ci.ps1` 在 `6a08c65..94808d5` 零差异（实测空）。

## 二、逐项核验（命令/EXIT/计数见 `target/round70/shangshu/attempt-2/`）

1. 提交链 6 条（`72d9313..94808d5`）：`b11deac`→`6a08c65`→`89daaaa`→`5f0d781`→`4d3d84b`→`94808d5`；`git rev-list --count`=6。
2. 唯一门禁 attempt-1（HEAD=6a08c65）：`attempt-1.log` **211,886B / sha256 `6acfc87f…390e28`**、`attempt-1.meta.txt` **465B / `4cbb0ee8…efe86`**、12 OK / 0 FAIL / EXIT0、UTC 06:54:11–06:58:15Z、gate_head 6a08c65。
3. 清单 `docs/testing/round70/final-manifest.json` **15/15 artifacts bytes+SHA mismatch 0**、外置 sha `17134337…7f4f1a66` 匹配、无自引用；HEAD blob 抽 4（entities/scene/frame/新测试）全 MATCH。
4. 独立抽跑（全新 target/tmp、`CARGO_PROFILE_DEV_DEBUG=0`）：R70 `round70_effect_blend_offset` **9 passed/0/0 EXIT0**（07:55:41–07:56:16Z）；`mir-render --lib` **133 passed/0/0 EXIT0**（官方门禁口径，不加 legacy 特性，07:56:16–07:56:28Z）；`cargo fmt --check`/`check_links`/`git diff --check` 全 EXIT0；`git status` 仅 2 历史 freeze。
5. 诚实性抽 5 处：T1 首轮 BLOCK（R70-T1 证据历史分类失真）→ `round70-c-erratum.md` 勘误（4 条 D8050 编译阻断 + 1 条 129/4 PermissionDenied code5）→ T2 addendum（22 行/3,368B/`69ed78db…`）判 CLOSED、必改 0；C2 `CARGO_PROFILE_DEV_DEBUG=0` 绕行与 M2 注入器 `Frame.width()` E0599 不计有效红均有披露；README/manifest 无「R70 全通过/生产全链已证/真实素材视觉已签核」扩称。
6. 差异裁定（`round70-lead-ruling.md`）：`--features mir-render/legacy` 的 clippy 2 条 `unneeded return`（minimap.rs:353、tiles.rs:311）为历史 `f37a402` 遗留 lint 债、本轮零 diff、官方门禁不加该特性 EXIT0 → PARTIAL 披露项，非必改。

## 三、PARTIAL 承接（原号）

magic 映射/spellfire 迁移（Crystal Effect/FireWall 差异）、完整 DrawBlend 组合与 rate/Shadow/Mask 全矩阵、真实素材视觉签核、既有 legacy lint 债、R61/R68/R69 台账未闭合项。

## 四、结论

限定切片内实现、测试、证据、门禁、清单与文档诚实性均满足；**ACCEPT-WITH-PARTIAL，必改 0**，可转 Review 交门下省确认。验收通过不代表 R70 全链或生产完整性已证。
