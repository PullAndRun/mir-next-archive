# R61 D4 刑部独立终审

verdict：**PASS-WITH-PARTIAL**。必改 0 项；PARTIAL 台账 12 项。仅放行本轮限定修正，未作整体 PASS 结论。

## 审计边界

- 独立读取代码、测试、Crystal 原件、门禁原件、meta 与 D1/D2 已留存证据；未跑 ci.ps1，未重跑测试。
- 本审仅写 final-manifest.json、本报告与 target/round61/audit/summary.log；未改源代码、原证据或历史冻结 JSON，未 commit/push。
- HEAD 与 origin/main 均为 `1e6cfca4d9b14b67f71f2cfd36a72006af7f5377`。
- 提交① `ef317b040fdb6ef78a478b1fa247ae3cee775e20`：8 文件，+2216/−232。
- 提交② `1e6cfca`：11 个 docs 文件，+458/−8；`git diff ef317b0..1e6cfca -- crates/` 为空。

## 门禁与树哈希

- 原件：`target/round61/gate/attempt-1.log`，207593 B。
- SHA256：`6ff5f68dd112020e20d42ddf8d6f835be7099dab999e1a411d1b9b4cff77ad96`，复算 MATCH。
- meta 十字段逐项读核：attempt=1、head=ef317b040fdb6ef78a478b1fa247ae3cee775e20、branch=main、exit=0。
- UTC 窗口 `2026-10-26T12:06:56Z`–`2026-10-26T12:09:29Z`；capture 为完整 stderr+stdout，路径/字节/哈希均吻合。
- 时间、运行时 HEAD 与退出码以留存 meta 为来源；本审没有重新执行进程以证明这些历史字段。
- 日志 :892–926 出现 R61 queue 11 例、sound 8 例通过；:3454 末行 `==> 全部通过（12 项）`，排除仅引用旧轮测试。
- :3361–3368 棘轮实值 66/0，阈值分别为 70/0；不把 66 误称为阈值。
- commit-tree-hashes 中 8 个代码路径逐个以 `git show ef317b0:<path> | sha256sum` 复算，8/8 MATCH。
- 清单自身不列入自身：其字节 SHA256 为 `21d66744086f5ef45c6e529c1cc340efb3a04d1643e372dcba7c782386415289`，只登记到 manifest。

## 行为审查

| 项 | 原件与结论 |
|---|---|
| G1/M1 | `net.rs::push_projectile_event` 仅追加并推进 total；`projectile.rs::consume` 保留绝对游标；`RecordingSoundSink::play` 仅 push，16 ring 与 cue32 丢弃均移除。queue 11 例含 17/33/257、顺序、不重复 sync/drain、空表重建及 total0。 |
| G2/M2 | 真读 Crystal `Effect.cs:81–100,185–187`；Rust `advance` 每帧先取目标，再算 duration/count；`frame_interval_ms` 整数除法，`frame_at` 用 `(elapsed−1)/interval` 严格越界，零间隔另分支。动态远移/近移/重合及边界有实际断言。完整时钟仍 PARTIAL。 |
| G3/M3 | `play_projectile_cue/cues` 不读写普通 throttle；`app.rs` 投射物路径接该入口；普通 play/play_event/play_spell 保留原频控。2/33 同刻提交及交错边界有断言。 |
| G4/M4 | `with_offline_mixer` 注入真实 rodio mixer，共用 decoder/amplify/submit_file；并非只用计数器。sound 8 例与 bin 接线测试检查样本、增益、叠加与出生/命中差异。 |
| span 修复 | 真读本机 rodio 0.22.2 `source/uniform.rs:55–66,145–162`：Some(0) 导致 Take 不推进 decoder；`HiddenSpanSource` 返回 None，实际解码器样本可进入 mixer。修复仅覆盖本 crate。 |
| M5/M6 | 门禁基于代码提交①；②不变代码。MF 注释与历史 P-6→P-4 修正落盘；README/client §5.28 保留限定边界。 |

离线指标与 `target/round61/d2/attempt-1/03-sample-energy.md`、对应样本日志交叉核对：

- M34-1/2 混音样本数 175126/125528；单条 peak 0.692266/0.733601。
- gain 0.25 的 RMS/能量比 0.25/0.0625；3 路叠加比 3/9。
- 真实出生/命中各 2 cue，peak 1.384532/1.467202；33 cue peak=22.844788。
- 这些数字证明离线样本和线性叠加，不构成声卡可听或 Live 证明。

## PARTIAL 台账（12 项）

1. P01：append-only 无确认/安全裁剪/回收契约；snapshot clone 与历史容量增长。
2. P02：完整状态式 NextFrame/SkipFrames 未实现，算术严格边界不等同完整时钟。
3. P03：TargetPoint 尚未完整实现。
4. P04：像素偏移增量尚未完整实现。
5. P05：真实设备可听与 Live 未验证。
6. P06：33 路 peak=22.844788，真实输出存在削波风险。
7. P07：105 sfx 未逐一验证；本次实测集中 M34-1/2。
8. P08：HiddenSpanSource 仅本 crate 的 decoder_for 路径。
9. P09：CWD 与素材来源解析边界保留。
10. P10：无会话代号，非空重建且已超过旧游标的歧义仍在；空表重建有测试。
11. P11：缺素材测试存在 early-return；SKIP 应为 PARTIAL，run-checks 对其判失败。
12. P12：R60 P-1..P-6 不自动清零；渲染/效果语义未据此宣称完整。

## 卫生与假绿

- 新增行静态扫描：allow/ignore/cfg(feature) 绕过属性为 0；新增中文非注释字符串仅 3 条 expect（目录/坏素材），未新增中文断言。
- 19 个变更 blob 全部 <1MiB，最大为 net.rs 365507 B；两份历史冻结 JSON 不计为本轮新增且未触碰。
- `git ls-files target resource game-assets .env` 输出为空；`git diff --check` 与提交区间差异检查通过。
- `run-checks.sh:33–44` 对发现数 0/低于最小值、running 0 tests、0 passed、SKIP/PARTIAL 与非零退出作失败处理；本审只读其实现。
- README、client §5.28 及新增文本未发现将禁用术语作为正面完成断言；历史 R59/R60 表述未提升为本轮证明。
- 结束工作树允许新增本审两份 docs；target summary 被忽略。原有两个冻结 JSON 仍未跟踪。

## 必改与哈希约定

必改：无。本结论保留以上全部 PARTIAL，不能改写为整体 PASS。

manifest_self_sha256 采用将该字段置 null 后的 UTF-8 `JSON.stringify` 字节（无尾换行、原插入顺序、不排序）计算，避免自引用固定点；最终文件字节 SHA256 单独回报。
