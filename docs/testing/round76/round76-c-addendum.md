# R76-C2 刑部追加核验

- **裁定：C 独立七变异 7/7 CLOSED，整体维持 PASS-WITH-PARTIAL；不构成整轮通过。**
- 基线 HEAD `5aa1960`；仅补做 M7，承接 C 原报告六项有效变异，不重跑 M1–M6；UTC 2026-10-28。
| 必改 | 状态 | 追加依据 |
|---|---|---|
| C-M1 | CLOSED | 全新物理副本 M7：Python0 → no-run0 → 全量7绿1红/101 → G8目标红101 → 原字节恢复 → 8绿/0 |
| C-M2 | CLOSED-with-amendment / 流程 PARTIAL | A 剔除规则及主树权威 SHA 独立复算成立；旧 sidecar 保留 |
| C-M3 | CLOSED-with-disclosure / 永久 PARTIAL | B 丢失清单、最终七链及完整性实物复核成立；不重造历史字节 |
- M7 首败：`crates/mir-client/tests/round76_trap_hexagon_mapping.rs:2248:5`，`assert!(source.contains("range(1390, 1400)"))`；EXIT101，目标实际执行1测试，全量实际执行8测试。
- 首败 left/right：N/A（原生 `assert!` 无此字段）；实测布尔条件 false、期望 true；Python EXIT0，MAGIC_FRAMES 实际106/期望116，缺1390..1399十帧；未以编译错/0tests/环境失败冒充红。
- 注入只删 `+ list(range(1390, 1400))`，原文/变异/patch 均保留；变异 SHA `d6cdba33cd16aee24ee5aeb569cef8b79626b19069f278c844453f02a4c0b63a`；恢复 `e32297dabcffff1c8472831309d63111e63dcae44a671fec0319b162ba57cfe6`。
- A：逐字节删当前 R72 第3..15行 = 65,330B / `c95dd8191c253a5fef4e2dca15aaed0b71970da2bda2b280c0aabf253583b7a9`；对基线65,322B恰4增/4删；主树66,437B/`3a9f42bd…`，sidecar 66,439B/`52fe25ee…` 为披露的中间稿。
- B：补录31行/4,133B/`3eacc942…`；丢失86文件模式+1 pristine+2脚本；与 manifest D7 一致，作废轮不产通过证据；最终m1..m7逐条核对 pre-green8/no-run0/目标101/全量101/恢复SHA/post-green8。
- B 清单独立复算133行/132唯一路径：131字节精确匹配+1已披露sha256.txt自引用stale，按其口径OK132/MISSING0/MISMATCH_REAL0；未将自引用称为精确匹配。
- Lead 三项处置与实物一致；裁定28行/`4ecb4997…`，A amendment20行/1,912B/`870fd5f5…`，均复算一致；本追加闭合其 C-M1 待办。
- 隔离：`git -c core.autocrlf=false archive 9da7f45` + A五文件SHA覆盖，nlink=1；只读资源物理复制130文件SHA一致；独立CARGO_TARGET_DIR/TEMP/TMP/TMPDIR，`CARGO_PROFILE_DEV_DEBUG=0`。
- 残余 PARTIAL：A sidecar流程、B失败日志永久损失、原C报告历史R75/R71/R72/R69/R61–R67/R74项、Crystal严格时钟/对象寿命、DrawBlend(true,0.8)/src×0.8、公共/文字700ms及本地素材承接，不清零。
- 证据：`target/round76/c2/attempt-1/{runs.json,m7/,checks/,logs/}`；只新增本报告与c2指定目录，未改主树实现/测试/素材/既有清单，未commit/push，未跑 `tools/dev/ci.ps1`。
