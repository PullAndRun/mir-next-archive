# R72-C2 只读追加核验

- 日期：2026-10-27（Asia/Shanghai）；基线：69b2e0c；命令实值见 `target/round72/c2/attempt-1/checks.json`。
- 核验对象 SHA256（bytes / wc -l）：
  - C 原报告：`5f6addd04b331f5341f0b6032332e768d76fb0e7585571e6f7d1fea630f7ed6a`（6291 / 42）。
  - Lead 裁定：`e0fbe58c49d32b800154eb9aa41f1b994daba1351405ca80d2a45bf3e045efcf`（3767 / 30）。
  - B attempt-2 link：`6e91372b62348a9cdf2155d826cc1c963672bcd92e86dd17c16dcec92a42816e`（11550 / 98）。
  - C mutations：`a93888d99086581d757a45cb9c45259fd2216243acd4820a77c49abd4d8d2cce`（4239 / 71；末行无换行，文本逻辑行72）。
  - B plan-M2 / plan-M7 red.exit 各：`db7a099b9d88b62fe56e6b7bd7c22332f789037e559b48b75b6273a2b8bf5af2`（9 / 1），内容均为 EXIT=101。
- 必改1 → CLOSED：B attempt-2 补 plan-M2/M7；link 明确撤回 attempt-1「一一对应」，plan-M4 近似残余已披露；C 自身七语义完整，含 plan-M2-bolt、plan-M4-center、plan-M7-source。
- 七项 test_sha 全为 `58f64ab2d30b69f3aea5c07a8975e988ca842bce45972499a6f93e120958baea`；restored_sha 全为 `df572677d7b013c3c09dab9c764afbda2095cacec820f3292bfc7abca2f78be6`。
- 必改2 → CLOSED 为 PARTIAL 披露：证据记载 legacy clippy 两处既有 needless_return（minimap.rs:353、tiles.rs:311）；本轮相对69b2e0c两文件 diff 空；官方门禁不带 legacy，双口径结果由原证据记录，本次未重跑。
- git status --porcelain 共6行：spellfx.rs M、测试文件 ??、两 freeze JSON ??、docs/testing/round72/ ??、round72-lead-ruling.md ??，与允许范围一致。
- 已读 Lead 裁定全文：两项处置与上述证据一致；「R72 全量通过/Crystal 等价」仅以禁止扩称语境出现，无肯定扩称。
- 文书小误：Lead 第4行测试SHA缩写 `58f64ab2…58b44` 尾段误写；正确尾段为 `58baea`，以本次实值及原报告完整SHA为准，不影响两项裁定。
- verdict：**PASS-WITH-PARTIAL，必改 0，可放行提交（限 149 毒云切片）**；全量门禁仍由 Lead 执行。
- 残余 PARTIAL：B plan-M4 精确写法未补跑（C 独立语义已补齐）；legacy clippy 两处既有 lint 债。
- 残余 PARTIAL：严格 NextMotion/初始100ms取整/每Process一步；149持久场、26移除与真实寿命；4000ms仅产品瞬态窗。
- 残余 PARTIAL：rate0.8/src×0.8×alpha、DrawColour/INVLIGHT/Light3/mask/tint/shadow；其他26技能 GAP-5、真实素材/GPU视觉、R71-C7历史缺口继续承接。
- 只读声明：核验未改实现、测试或既有证据，仅新增本附录与 checks.json；未 commit/push，未跑 cargo 或 ci.ps1。
