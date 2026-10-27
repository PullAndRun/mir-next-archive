# R71 Lead 裁定：C 证据必改 E1（c5c7d8 中间日志缺失）

- 依据：`docs/testing/round71/round71-c-evidence.md`（verdict BLOCK；实现/测试必改 0，证据必改 1）
  的 E1 与 `docs/testing/round71/round71-a-erratum.md` 的事实核验。
- 认定：
  1. E1 属于**证据记录/历史保全缺陷**，非实现或测试缺陷；功能验收基础（最终冻结 SHA 下红/绿原件、
     B 五变异矩阵、C 在冻结提交物理副本上的独立复算）均成立，C 报告亦确认实现/测试必改 0。
  2. C7（失败 attempt 全保留）在 A 的 c5c7d8 中间阶段确实被破坏：该阶段两日志被后续迭代覆盖且不可恢复。
     依 R69 先例（证据目录只增不改、不可恢复即永久 PARTIAL），本轮记录 **C7 = PARTIAL（永久）**，
     不以任何补写/伪造方式声称历史完整。
  3. 处置：A 清单勘误（`round71-a-erratum.md`）随本裁定提交；C 报告 BLOCK 版原样保留提交，不覆盖其初稿历史；
     请门下省对「C7 缺口 + 勘误 + 独立复算可接受性」作明确裁决。
- 门禁与提交边界：
  - 功能提交 `cedf903`；唯一门禁 attempt-1（HEAD=`16644d2`，含 C 初稿提交）EXIT=0 12/12，
    log 212,193B sha256 `d1fcbf77dfe7751eefeaf03b5a6f8af70e564eacf94a8fc15b10789d2b2c8173`；
  - C BLOCK 版、勘误与本裁定为**门禁后文档提交**，`git diff cedf903 -- crates` 必须为空；
  - 后续独立终审须同时核验：功能/门禁原字节、C BLOCK 与初稿的关系、勘误真实性、C7 PARTIAL 不被淡化。
