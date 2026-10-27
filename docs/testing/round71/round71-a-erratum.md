# R71-A 证据清单勘误（E1）

- 触发：R71-C 独立核验发现 `target/round71/a/attempt-1/manifest.md:64` 与 `:76` 声称存在
  `red-c5c7d8.log` / `green-c5c7d8.log`，实际两文件均不存在（`find target -iname "*c5c7d8*"` 无结果；
  C 侧 `missing-index.py` 复现 exists=false，见 `target/round71/c/attempt-1/missing-index.json`）。
- 事实核验（Lead 独立复核）：
  - 现存 A 证据目录中与 c5c7d8 阶段相关的只有 SHA 记录：
    `sha-final.txt:2` 与 `test-sha-green.txt:1` 均记录
    `c5c7d8d6598047c746c29d66dbf89334bac345cd8dc427832d71fa59a14537b8`（测试文件 SHA）。
  - 该阶段之后测试文件又经历 `f5fd2f39…`（`test-sha-final.txt`）与最终 `64ff3f59…`（`test-sha-red2/green2.txt`），
    最终冻结红绿为 `red-final2.log`（2 passed/6 failed，EXIT101）与 `green-final2.log`（8 passed，EXIT0）。
  - 结论：c5c7d8 中间阶段的 red/green 日志在后续 final/final2 迭代中被覆盖，未归档，**不可恢复**。
- 更正口径（只增不改；原 manifest.md 保持原样作为历史记录）：
  1. 撤回「`red-c5c7d8.log`/`green-c5c7d8.log` 存在」的表述；
  2. c5c7d8 阶段仅保留 SHA 级记录（`sha-final.txt`/`test-sha-green.txt`），不再声称该阶段完整日志已保全；
  3. 行为验收依据为最终冻结 SHA `64ff3f59…` 下的红/绿原件、B 五变异矩阵与 C 在冻结提交物理副本上的独立复算，
     不依赖 c5c7d8 阶段日志。
- 影响与边界：不改变 A 的实现/测试交付与最终红绿结论；本项属于证据历史保全缺口（C7），
  由 `docs/process/round71-lead-ruling.md` 裁定为永久 PARTIAL，禁止伪造或补写历史日志。
