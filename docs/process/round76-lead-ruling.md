# R76 Lead 裁定：C 核验三项必改的处置

- 背景：R76 刑部 C（gpt-6.1-sol）独立核验裁定 **PASS-WITH-PARTIAL**，报告 `docs/testing/round76/round76-c-evidence.md`（25 行）；提出三项必改：C-M1=C 自身 M7 未形成有效红；C-M2=R72 supersede sidecar 与主树 SHA/bytes 不一致；C-M3=B manifest D7 承认早期失败日志被覆盖。
- 本裁定只处置证据与流程，**不修改任何 A/B/C 既有文件与日志原文**；所有补证只增不改。

## 裁定 1（C-M2，R72 supersede 记录不一致）— CLOSED-with-amendment，记流程 PARTIAL

- 事实：`target/round76/a/attempt-1/supersede-r72.txt` 记 `final_sha 52fe25…/66,439B`；主树实算 `crates/mir-client/tests/round72_poison_cloud_mapping.rs` = **66,437B / `3a9f42bd67a8e57e02d86fb970ad6347687d46991b313c3c7fb2316db97b4bfb`**。
- A 只增补证：`target/round76/a/attempt-1/supersede-r72-amendment.md` 20 行/1,912B/sha256 `870fd5f5bb335a89d62a4b805d12af277df0d1ad268438bf282b3eb56b9486dd`（+`.sha256`）。剔除规则固定为删除当前文件顶注块第 3..15 行（含块后空注释），结果 **65,330B / `c95dd8191c253a5fef4e2dca15aaed0b71970da2bda2b280c0aabf253583b7a9`**，与顶注逐字节一致；剔除后与 R72 基线（`58f64ab2…/65,322B`）diff 恰为 4 insertions/4 deletions，证明仅四处 supersede。
- 结论：顶注与正文/断言/expected/fixture 均未被污染；sidecar 为交付前中间稿（V5 归一化清除顶注 2 字节行尾空格后未重算），属证据记录偏差，不改功能结论。**旧 sidecar 原件保留，主树实测值为权威**。流程瑕疵记 PARTIAL（历史不追溯清零）。

## 裁定 2（C-M3，B 失败轮次日志被覆盖）— C5 CLOSED-with-disclosure，记永久 PARTIAL

- 事实：B 变异驱动 v1（cp936 解析 FILE 失败、`cp -p` 旧 mtime 致 Cargo 不重编 → m6/m7 误报红）与 v2 部分（TRI 锚点歧义拒绝注入 m1/m4/m5）产生失败日志，最终驱动 `rm -rf` 覆盖，实际留存 0。
- B 只增补证：`target/round76/b/attempt-1/aborted-attempt-record.md` 31 行/4,133B/sha256 `3eacc942d78d79135b83c5b4a17ff06087cc73f9dd2245391749ccf83f5433c6`（+`.sha256`、`integrity-recheck.log`）：如实列出丢失清单 86 条文件模式 + 1 pristine + 2 旧脚本、时间窗口（约 23:25–23:33Z）、作废轮**不产生任何最终通过证据**、最终 m1..m7 全部重跑且各自 pre-green 8/8→no-run0→目标红101→全量红→恢复原字节→post-green 8/8、不可恢复字节不伪造。
- `integrity-recheck.log`：132 唯一路径 OK=132、MISSING=0、MISMATCH_REAL=0（唯一 raw mismatch 为 sha256.txt 自引用行，生成顺序产物）。
- 结论：**有效证据未被破坏**；不符合「只增不改」的中间失败日志属永久字节损失，如实披露，记永久 PARTIAL；不要求也不能重造历史日志。C5 以「披露+最终矩阵全链重跑」闭合。

## 裁定 3（C-M1，C 自身 M7 未形成有效红）— 由 C 属主补做（C2）

- C 自述其 M7 注入后 G8 仍 EXIT0，未形成有效红，故不得声称独立 M7 通过。B/A 的 M7 红证据（目标 G8 断言 EXIT101）保全且与 C 独立复算红绿一致，可作交叉证据，但不能替代 C 自身独立复现。
- 处置：spawn 刑部 C2（gpt）在**全新物理副本**中仅重做 M7 有效红：`analyze_usage.py` 删除 `range(1390,1400)`（或等价使计划缺十帧的变异）→ 构建 `--no-run` EXIT0 → G8 目标断言红 EXIT101（记录首败行/left/right）→ 恢复原字节 SHA → 8 绿；写 `docs/testing/round76/round76-c-addendum.md`（≤20 行）并更新 C 结论为「M7 CLOSED」。若 C2 仍无法形成有效红，则如实记 C 独立 M7 为 PARTIAL、整体 C 结论保持 PASS-WITH-PARTIAL，且整轮说明不得声称「C 独立复现七变异」。

## 裁定 4（提交与门禁）

- 功能白名单与 SHA 不变（spellfx `84fdb324…`、测试 `8b680973…`、R72 `3a9f42bd…`、analyze_usage `e32297da…`、asset-pipeline `2d2fa100…`）；**不得为证据勘误改动测试/生产**（改动会重置红绿同 SHA 链）。
- 待 C2 追加报告与补证落盘后，Lead 提交功能批并运行唯一全量门禁（12 项）；门禁通过后 D 文档批、独立终审、尚书验收、门下确认、Done、push。
- 残余 PARTIAL 原号承接：Crystal 严格 NextClock/对象寿命、DrawBlend(true,0.8) 未实施、src×0.8 不可由 BlendFactor 推导、公共/文字 700ms 既有路径、素材本地不入 git、B 失败日志永久损失、A sidecar 流程瑕疵、C M7（若 C2 未闭合）。
