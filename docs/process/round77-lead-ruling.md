# R77 执法裁定（Lead）

- 日期：2026-10-28（UTC）
- 轮次：R77（主单 JJC-20261028-004，149 Trap 八帧映射差异与素材准入合同，生产零改）
- 基线 HEAD：`0bbc322`（R77 过程文书提交，origin/main=6942826）
- 关联：计划 `round77-plan.md`、门下 `round77-menxia-review.md`（C1–C5/M1）、派发 `round77-shangshu-dispatch.md`

## 裁定 1：C 首轮 FAIL 有效，A 返工改判

- C（gpt dbf1a856）对 A 旧交付（测试 `514b13d8…`/工具 `4e1735b5…`）判 **FAIL，必改 2（F1/F2）**，报告 41 行/7,542B/sha `b0f744d1…`。
- Lead 接受 C 裁定；A 旧 SHA 交付**作废（superseded）**，A 返工 attempt-2 新 SHA：
  - 测试 `crates/mir-client/tests/round77_trap_asset_admission_contract.rs`：60,061B/1,621 行/sha `f809b5d44966b28dc1dad95d854eda843fefe36aea012e6cced75668f9e6a27e`
  - 工具 `tools/testing/round77_asset_admission.py`：69,755B/1,233 行/sha `10f957e59e3041234ed5a2b64eafe1d6ed343b52a94b4196055fd44f8bba9b5e`
  - 冻结 `target/round77/a/attempt-2/fixtures/expected-freeze.json`（15,057B/sha `da1e2141…`，freeze UTC 2026-10-28T03:42:39Z，82 项 digest `0f66806c…`，supersedes attempt-1 `b36bf18a…`）。
  - A 事故如实披露：首轮补丁脚本非法 newline 参数致工具文件截断 0 字节；已从 attempt-1 绿备份恢复（sha `4e1735b5…`，与 B a-originals 一致）后重做，无有效证据丢失。

## 裁定 2：F1（矩阵必须可证伪）成立

- A 旧矩阵 current 列误用 R76 Hexagon（Magic/1390/10/100ms/4000ms/Add）而非现行非149 Trap（Rust 通用：Magic / 0..9 / 10 帧 / 60ms / 700ms / 居中 Over）；interval 误标 SAME。
- 修正口径（A attempt-2 已执行）：
  - current 列取自 `crates/mir-client/src/ui/spellfx.rs` 通用非 149 分支与常量（实读解析）；
  - target 列取自 Crystal `Client/MirObjects/SpellObject.cs:132-138`（Magic2 / 2360..2367 / 8 帧 / 100ms / Blend(true,0.8) / DrawLocation）；
  - interval 状态 = `DIFF_TARGET_REQUIRED`（100 vs 60）；
  - G5 三方交叉断言：内联矩阵 ↔ Rust 源码解析值 ↔ 工具 `--matrix` JSON；
  - 必做负控：矩阵 target interval 100→120、frame_count 8→7 必须各自使唯一 G5 红（A attempt-2 已实跑 EXIT101 两条，见 `target/round77/a/attempt-2/red/`）。

## 裁定 3：F2（撤销“物理不可能”论证）成立

- 旧“未索引空间 2,637,986B < 解压 RGBA 4,809,024B ⇒ 压缩帧物理不存在”不成立：Lib 八帧 gzip 载荷实测合计 114,603B（含 8×17B 帧头 114,739B），未索引空间远大于压缩载荷。
- 修正口径（A attempt-2 已执行）：
  - NOT_READY 独立依据 = WZX 索引表越界（default declared 1460/slots 1480；帧 2360 需索引表字节 9488..9492 > wzx 5,968B）→ `MISSING_INDEX` / `OUT_OF_INDEX_TABLE`；
  - 2025 组起始 13,316,944 > default magic2.wzl EOF 10,290,606（距离 3,026,338B）仅作 `reference_only` 的地址不可读参考（`CORPUS_ADDRESS_BEYOND_DEFAULT_WZL_EOF`）；
  - 移除 `physically_absent` / `physical_boundary_insufficient` / RGBA 4,809,024 相关断言；selftest 增 `no_absence_claim` 防措辞复活；
  - B 的 attempt-1/2 相关表述保留为历史记录，不再作为本轮结论；D 文档须按修正后口径写。

## 裁定 4：验证链重排

- B 以 attempt-3 按新 SHA 重验：基线 7/7、六变异（M1→G1 / M2→G2 / M3→G3 / M4→G4 / M5→G7 / M6→G6）+ **F1 矩阵负控两条**（100→120、8→7 各唯一 G5 红）+ F2 口径核查 + 新 oracle 交叉核验 + 回归 R76 8/R72 9/R59 6 + 卫生。
- C 首轮 FAIL 报告（旧 SHA）保留为历史；C2（gpt）在新 SHA 下复核 F1/F2 闭合、负控有效、六变异与卫生，出追加核验报告；必改清零且 PASS-WITH-PARTIAL 后才可 Lead 提交。
- C 的 A digest `eecc2114…` 不可复现判为 PARTIAL 非阻断说明项（82 项值与原字节可复算）。

## 裁定 5：范围与冻结

- 白名单不变：`crates/mir-client/tests/round77_trap_asset_admission_contract.rs` + `tools/testing/round77_asset_admission.py` 两个新文件；生产零改、无新 supersede。
- R76 十八帧精确集与 R59 116 帧冻结集合保持原 SHA；素材只读（`resource/data/magic2.wzx`/`magic.wzl`/`magic2.wzl`、`game-assets/gfx/sprites/effect/{magic,magic2}/`、两份 Magic2.Lib 候选）。
- 门下 M1（Lib 候选→R78/D 披露）仍 OPEN；本轮不得宣称 READY/已认证/生产已修/Crystal 等价。
- 唯一全量门禁由 Lead 在功能提交后运行；A/B/C 不跑 ci.ps1。
