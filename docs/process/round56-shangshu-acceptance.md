# R56 尚书省验收（JJC-20261026-003）

- **受验 HEAD**：`b5369d799a46143b823580ce9c355ea268207fd0`（= HEAD = `origin/main`，branch=main）
- **验收角色**：shangshu（只读复核；未跑 `ci.ps1`、未改代码/文档）。基线：计划/门下评审/派发/刑部终审四件 + gate 原件。

## 一、输入基线（核对一致）

| 项 | 值 |
|---|---|
| 门禁原件 | `target/round56/gate/attempt-1.log`，194,543B，sha256 `0ad14a35694be86724b7621d4606436b54e8ba4925b50f0614eeb19c4dfe0ef8` |
| 门禁 meta | `attempt-1.meta.txt`（非 `.json`，见 P5）：head/branch/exit/bytes/sha 与原件全等；start `05:05:11Z` / end `05:07:29Z` |
| 提交 | `d7a23e3` = `crates/mir-server/tests/aoi.rs` +139；`b5369d7` = 5 文档 +336（aoi.md +32、README +46、plan +109、review +69、dispatch +80） |
| 证据 | A `target/round56/aoi/attempt-1/`（21 件）；B `target/round56/docs/attempt-1/`（6 件） |
| 终审 | `docs/testing/round56/round56-final-audit.md`：PASS-WITH-PARTIAL、A–H 全 PASS、必改 0、PARTIAL 6 |

## 二、门禁核验（复算通过）

- bytes 194,543 与 meta 相等；sha256 复算 = `0ad14a35…ef8`，与 meta 相等。
- meta.head = `git rev-parse HEAD` = `git rev-parse origin/main`；exit=0；`target/round56/gate/` 仅 attempt-1（无 attempt-2）。
- 12 项 `OK` 汇总行齐全，末行 `全部通过（12 项）`；`cargo test --workspace` 段 `running 14 tests`/`14 passed` 命中 aoi.rs。
- 未发现 FAIL/伪绿/旧轮日志冒充；未跑全量门禁（任务禁跑），仅核原件自洽。

## 三、提交与证据核验（6/6 sha 复算 MATCH）

| 文件 | bytes | sha256（前 8） | 判定 |
|---|---|---|---|
| `crates/mir-server/tests/aoi.rs`(d7a23e3) | 58,553 | 605d1f9e | ✅ |
| `docs/world/aoi.md` | 25,511 | 653c8f00 | ✅ |
| `docs/testing/round56/README.md` | 3,201 | 10b36817 | ✅ |
| `docs/process/round56-plan.md` | 14,446 | a4473c9f | ✅ |
| `docs/process/round56-menxia-review.md` | 9,919 | 611868cd | ✅ |
| `docs/process/round56-shangshu-dispatch.md` | 8,929 | 8c581ce1 | ✅ |

- 两提交文件集不相交；`git diff e3b365f..b5369d7 -- crates/mir-server/src` 为空（未私改 src）。工作树仅 2 个历史大冻结 JSON + 终审/manifest 未跟踪。
- **M1**：`:1112-1127` 冻结说明，`advance_clock(0)` @`:1128`（两次 spawn 前），`:1143-1147` 双向 ExpireTime 显式断言（⑥ 红点可失败）。
- **M2**：`:1324-1334` 文档注释、`:1336` `ground_item_expiry_strict_ms_boundary`；断言 1ms 差、边界无 `ObjectRemove`（`:1389`）、a+1 仅删 a、b+1 删 b、二次 tick 零重复；无 `#[allow]`。
- **M3**：`--list` 14 tests；单例各 1 passed（`11` / `12` 原始 `EXIT=0`）；`09-fixed-clean-100-runs` RUNS=100/F0/P100，`08-nofreeze-sleep-100-runs` F100/P0；`07` 负结果如实登记。
- **M4**：`aoi.md:263-294` 五条证据（①`:268` 原语/②`:276` Crystal 0 调用/③`:280` 无卸载/④`:283` 淘汰排除/⑤`:286` 仅测试），`:291` 明写整体 PARTIAL；README 46 行，索引/口径自洽；新增行三红线词 0 命中（整文件 7 处既有 PASS 为 P3，已披露）；R57 统一句在 README:40-41。

## 四、G1–G4 签发

- **G1（flake 去根因）**：**ACCEPT**（冻结时钟 + 确定性新例，先红后绿 + 100/100 正反）。
- **G2（AOI-5 接线裁决）**：**ACCEPT（“无真实接线点”，不发明生命周期）**；AOI-5 整体维持 PARTIAL。
- **G3（文档）**：**ACCEPT**（§8.9 + README 索引落盘；未预写 PASS；不改既有结论段）。
- **G4（门禁/提交）**：**ACCEPT**（head 绑定、exit 0、12/12；两提交边界干净）。

## 五、PARTIAL 承接（6 项，均非阻断；P1 为目标本身）

P1 AOI-5 整体 PARTIAL（仅子项「无回收原语」CLOSED，生产零调用者）→ R57+ 返工或产品决策；P2 `world.rs:1378` 0x07(BEL) → R57；P3 整文件 grep 7 处既有 PASS → R57 清理或改口径；P4 meta 无候选文件 sha → R57 增列；P5 meta/产物命名（`meta.json` vs `meta.txt`、`round56-final-manifest.json` vs 磁盘 `final-manifest.json`）→ 后续统一；P6 README 受测 HEAD 写 `e3b365f`（代码基线）非门禁 `b5369d7` → R57 改为双标注。
**必改 0**；A–H 全 PASS 与终审一致，未见夸大。

## 六、遗留风险 / 项目问题（验收时新发现，均非阻断）

1. **R-1**：计划 G2 证据路径 `target/round56/aoi/attempt-1/10-wiring-greps.log` **不存在**；实际原件为 `target/round56/docs/attempt-1/00-evidence-greps.log`（aoi.md §8.9 引用正确）。建议 R57 修订计划路径或补别名。
2. **R-2**：T 产物 `round56-final-audit.md` + `final-manifest.json` 仍未入库（README 索引已引用）——派发 §2-B 要求其入提交二，但 T 必在门禁后产出，时序不可满足；需后续文档提交补入（本轮验收令禁止 commit）。
3. **R-3**：`docs/world/aoi.md:291` 写「生产零调用者接线待 **R56**」措辞陈旧（本轮即 R56，终审 P1 记 R57+）；建议 R57 改为「待 R57+」。
4. 既有 P3/P5/P6 已在 README 与终审如实登记，无隐瞒；`#![allow]` 0 命中，无假绿迹象。

## 七、结论

**ACCEPT-WITH-PARTIAL**。R56 的 G1–G4 目标达成、门禁与提交边界可核、必改 0；但 **AOI-5 整体维持 PARTIAL，仅其子项「无回收原语」CLOSED；不得宣称 AOI-5 关闭、不得宣称 R56 完整**。P1–P6 连同 R-1–R-3 由 R57 承接。本文件不 commit，由 Lead 决定入库方式。
