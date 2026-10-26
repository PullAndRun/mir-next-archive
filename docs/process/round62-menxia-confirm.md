# R62 门下省确认

- 确认者：gpt-6.1-sol；日期：2026-10-26（Asia/Shanghai）；仅抽核现有证据。
- 结论：**PASS-WITH-PARTIAL**；与尚书 ACCEPT-WITH-PARTIAL 一致；必改 0，无 BLOCK。
- 范围：功能 `a7362fc` → 文档 `98aa9db`；六项抽核如下，历史测试结果不冒充本次复跑。

| 项 | 命令／动作 | 结果 | 证据 |
|---|---|---|---|
| 1 门禁原件 | `sha256sum target/round62/gate/attempt-1.log`；`wc -c`；读取 meta/日志汇总 | PASS | 209193 B，SHA 见下；meta HEAD=a7362fc7f93b50bfd0027dca5383e5eadd6c29bb、exit=0、items=12；日志:3467–3479 共12项 OK |
| 2 文档无代码 | `git diff --exit-code a7362fc..98aa9db -- crates/` | PASS | 空输出，exit=0 |
| 3 哈希抽检两项 | `sha256sum docs/testing/round62/final-manifest.json`；`git show a7362fc:crates/mir-client/src/ui/projectile.rs \| sha256sum` | PASS | 两值见下；与验收:37–38、manifest:55一致；D attempt-1 summary 是测试结果，不含哈希 |
| 4 红线与承接 | 读取 `docs/testing/round62/README.md:65–71`、`docs/client.md:1351–1359`，对照验收:49–56 | PASS（边界 PARTIAL） | “125 全链已证/真实可听/R62 完整”仅禁称或未证实语境；四项与 R61 P01–P12 均保留 |
| 5 治理链 | `sha256sum` 四份 process 文件及 C 独立证据；读取派发/验收 | PASS | 计划→门下审阅→尚书派发→C证据→验收齐全；原字节 SHA 见下，可写入本确认 |
| 6 确认交付 | 本文件编码/行数/换行断言、`git status --porcelain` 前后对照 | PASS | UTF-8/LF、≤40行；仅新增本确认，既有未跟踪验收及两冻结 JSON 保持 |

独立复算 SHA256（原始字节，源码取提交 blob，非 Git object id）：
- gate：`81e8ef05131460635adaee4bf02066d4e2187a2a852573a8ae7191a23adfd9ab`
- manifest：`df70ce05fa4e2e7110cf6c46f2a7f782ce15cc97756fdb9da1822a2281029308`
- a7362fc:crates/mir-client/src/ui/projectile.rs：`ca8e4a47fe485eb92f5ac18e069a1ffc5f46ea9e98a707c55c6cc2ac373eb8d1`
- docs/process/round62-plan.md：`dc8e62ee6c76a73513321c273df66d11ed11f9da2ce2a393637044859ca1152f`
- docs/process/round62-menxia-review.md：`584be23bebadc68057f301c3545d69026f6849d5a6ba213387a2ee446f834da3`
- docs/process/round62-shangshu-dispatch.md：`f437732022d08379a51fd75793d049b69d18048f3ab94262e502dd2b95f9d92a`
- docs/testing/round62/round62-c-evidence.md：`b9db80f93817eb086d995444a59d39976bad8cdbc01f0f2a32a07fa223944d52`
- docs/process/round62-shangshu-acceptance.md：`ff1f2b3a20f42f62d2a1f6b7a6bb11b4f32b7450252a74e048d16c273868e5e2`

遗留与口径：
- PARTIAL 承接 R63：事件表物理回收无服务端协议；known_positions 无容量上限/LRU；owner_skip_frames 生产固定 false、缺 ActionFeed；570 残影仍 elapsed（10帧/600ms）。
- R61 P01–P12 全量承接，含音频/Live、削波、105 sfx、HiddenSpanSource、ImageInfo.offset、素材及 R60 P-1..P-6；7 个既有 ignore 不计通过、未新增的事实沿用 C/验收，不据局部进展闭项。
- D `target/round62/r62-d/attempt-1/checks/summary.tsv` 为 5 PASS/2 FAIL（links/diff）；`target/round62/r62-d/attempt-2/checks/summary.tsv` 为 7 PASS/0 FAIL/0 SKIP、45s，保留旧失败并采用后次结果，无矛盾。
- 验证档位 L0：现有文件/提交差异/原始哈希抽核；无构建、无测试复跑、未跑全量 ci.ps1；命令耗时以工具回执为准。
- 边界：仅新增本文件，未改代码/证据/历史文档，未 commit/push；主任务 Review 状态沿用委派事实，未操作看板。
