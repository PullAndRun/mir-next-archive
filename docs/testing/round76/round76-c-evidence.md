# R76-C 刑部独立核验

- 子单：`JJC-20261028-003-sub-af9acdec`；基线 HEAD `9da7f45`；UTC 2026-10-28。
- **裁定：PASS-WITH-PARTIAL；必改 2 项，未达“必改 0”提交门槛，退回原 owner，不自修。**

| 项 | 结论 | 独立证据（命令/EXIT） |
|---|---|---|
| 范围/白名单 | PASS | `git status --porcelain` 7 行（A 五文件+2 freeze）；白名单外 diff 0B；freeze SHA/bytes 与 B 一致；`git diff --check` EXIT0 |
| A 红绿与回归 | PASS | 独立 archive 物理副本，`CARGO_PROFILE_DEV_DEBUG=0`；R76 8/8、R72 9/9、R71 8/8、R70 9/9、R73 9/9、R75 7/7，均 EXIT0；A red7 首败 `(381,261)` actual `[0,0,0,255]` vs expected `[120,132,248,255]`，测试 SHA `8b680973…f3562d0` 与绿相同 |
| 变异 M1/M2/M3/M4/M5/M6 | PASS | 独立副本、nlink=1；每项 pre-green 8/8→no-run EXIT0→目标 EXIT101→恢复 SHA→post-green 8/8；M1 首败 1051，M2/M3 1197，M4 1441，M5 1656，M6 1343；实值见 `target/round76/c/attempt-1/mutations/summary.json` |
| 变异 M7 | PARTIAL | C 独立注入后 Python EXIT0，但 G8 仍 EXIT0（未形成有效红）；B 证据报告 M7 EXIT101。C 不能把 B 结果冒充独立复现，需原 owner 补 C 可复查的有效 M7 注入/红日志 |
| 独立 oracle | PASS | 新写 `oracle.py` 自解 `magic.wzl/.wzx`；1390–1399 10/10 format code=3、尺寸/offset/PNG RGBA SHA 与 index 一致；1390 八像素向量及整帧 PNG SHA 见 `oracle.json`；真实 WZL SHA `3b8a2dba…a02ab25` |
| R72 有限 supersede | PASS-WITH-PARTIAL | 原 `58f64ab2…/65322B`；去顶注后 `c95dd819…/65330B`；仅四处 hunk、四处 TrapHexagon→Reincarnation；迁移后 suite 9/9。**侧录 final_sha `52fe25…/66439B` 与现文件实测 `3a9f42bd…/66437B` 不一致，须 owner 解释/重生保全记录** |
| 诚实性/静态 | PARTIAL | 4000 明确为新产品窗，公共/文字 700 保持；无 Crystal 寿命、src×0.8、设备视觉扩称；D5 头注 M 编号错位仅注释，非阻断但须披露；D1 固定 work/共享 target、D8 legacy clippy 既有 `minimap.rs:353`/`tiles.rs:311` 已披露；`py -3 tools/docs/check_links.py` EXIT0 |

## 独立证据计数与承接
- C 独立复跑：6 套基线、6 套 R76 绿回归、6 个有效变异红绿循环；M7 无效，故不能宣称七变异独立完整通过。
- B oracle 的实测格式为 palette8/format_code=3（不是任务摘要的 format5）；旧 106 帧 SHA 全保全，1390–1399 十帧可读。
- D7 记载早期 v1/v2 失败日志被覆盖，违反“失败 attempt 只增保留”口径；应作为流程 PARTIAL/整改项，不由后续最终绿覆盖。
- 历史 PARTIAL 原号继续承接：R75 七组、R75-E1/C-M1 原字节不可恢复、R75-E2/C-M2 首 actual 缺失、R71/R72/R69/R61–R67 与 R74 artifacts34；本核验不清零。
- 证据根：`target/round76/c/attempt-1/`（runs、copies、mutations、oracle、logs、static.json、sha 清单）；未改实现/测试/素材/清单，未 commit/push，未跑 `tools/dev/ci.ps1`。

**必改清单：** (1) 补齐 C 独立有效 M7 红复现及原始日志；(2) 修正/解释 R72 supersede final SHA/bytes 不一致并保留可复算原件；同时 D7 覆盖式旧失败日志应补充事实说明或退回上游裁定。 

<!-- 报告 ≤70 行；本报告为核验记录，不构成实现通过。 -->
