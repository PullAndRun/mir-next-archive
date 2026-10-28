# R77：149 Trap 八帧映射差异与素材准入合同

- 主单 `JJC-20261028-004`；阶段 **post-audit**，**PASS-WITH-PARTIAL（御史台独立终审；必改 0）**；这是合同与准入审计层交付，生产零改；仍属 PARTIAL，不作整轮通过结论。
- 范围：只新增[七 G 合同测试](../../../crates/mir-client/tests/round77_trap_asset_admission_contract.rs)与[只读素材准入工具](../../../tools/testing/round77_asset_admission.py)；没有 crates/src 生产改动、素材变更或新 supersede。
- 提交链：`0bbc322`（规划/门下/派发）→`0e1be09`（Lead 裁定）→`dcb0c89`（C 首核 FAIL 与 C2b 追加）→`24f0826`（功能 HEAD）→`1bdbe4a`（文档批）→清单 `delivery_head=1bdbe4a` 已固化；[御史台终审](round77-final-audit.md)两提交 `docs(audit)`/清单 post-audit 见 manifest `delivery_head`。
- 本轮合同详见[准入合同](round77-trap-asset-admission.md)；[候选清单](final-manifest.json)与[外置摘要](manifest.sha256)记录检出字节和历史 blob，清单不自引用。

## 准入结论与矩阵

- default Magic2 WZX 5,968B、declared=1460、slots=1480；2360..2367 全部缺索引/越界；2360 所需 9488..9492 已超表，状态 **NOT_READY**。
- 2025 magic 同号八帧 zlib 全通、物理连续 `[13316944,13357626)`；解压实测 RGB565 行四字节对齐，Crystal 语义未证，尺寸 66..176×48..149 不匹配 Lib 396..432×363，状态 **UNVERIFIED_SEMANTICS**，同号不可替代。
- 两份 Magic2.Lib 字节一致（21,091,901B，SHA `f449aa4c96632f13454d7746ab47678c06175be02e69a6dc8fdfd036755fc0d2`）、v2/count2790；八帧 gzip 为 w*h*4（574,992..627,264B），邻居2357/58/59/2368/69为4×1空帧；来源/语义绑定未证，状态 **AWAITING_BINDING**。
- G5 current 是 Rust 通用非149 Trap；target 是 Crystal SpellObject.cs:132–138：Magic→Magic2（BLOCKED_BY_ASSET_ADMISSION）；0..9→2360..2367、10→8、60→100ms、Over→Blend(true,0.8F)（DIFF_TARGET_REQUIRED）；700→700ms（SAME）；centered→DrawLocation（PARTIAL）。
- F1 CLOSED：撤去误用 Hexagon 的旧 current；内联矩阵、源码解析与工具三方交叉；interval100→120、frame_count8→7 两负控均唯一 G5 红 EXIT101，恢复后七绿。
- F2 CLOSED：撤销“未索引空间小于解压 RGBA ⇒ 物理不存在”论证；NOT_READY 独立依据为 WZX 越界。2025 起始13,316,944超过 default WZL EOF10,290,606（差3,026,338B）仅 reference_only；压缩载荷114,603B、含头114,739B，不证明全库不存在。

## 验证链与门禁

- A 旧 SHA 七绿→[C 首核 FAIL/必改2](round77-c-evidence.md)→A attempt-2 返工（F1/F2）→B attempt-3 新 SHA 独立复验→[C2b PASS-WITH-PARTIAL/必改0](round77-c-addendum.md)；首核 FAIL 历史保全，不回改。
- 当前测试60,061B/1621行/SHA `f809b5d44966b28dc1dad95d854eda843fefe36aea012e6cced75668f9e6a27e`；工具69,755B/1233行/SHA `10f957e59e3041234ed5a2b64eafe1d6ed343b52a94b4196055fd44f8bba9b5e`。
- A attempt-2 七绿、assert82项/selftest7项；[expected-freeze](../../../target/round77/a/attempt-2/fixtures/expected-freeze.json)15,057B。B [attempt-3清单](../../../target/round77/b/attempt-3/manifest.json)verdicts8/8，基线03:52:13–03:52:47Z七绿。
- B 六变异各独立物理副本：no-run0→唯一目标G红101→原SHA恢复→七绿；M1→G1、M2→G2、M3→G3、M4→G4、M5→G7、M6→G6；另F1两负控覆盖G5。
- B oracle300/300 agree、0 mismatch；A digest `0f66806c5b59c4f9969c27fb0cf822033567871c442f31135471320d3386c881` 独立复现。canonical 为工具 EXPECTED dict，JSON sort_keys+compact、ensure_ascii=false；旧摘要声明由新冻结承接。
- B 回归R76 8/8、R72 9/9、R59 6/6；fmt EXIT0、clippy(mir-client) EXIT0（3 warnings/0 errors）。C2b另做interval负控后恢复七绿；只抽核300/300声明与清单，未重跑全量oracle。
- Lead 唯一全量门禁 attempt-1：HEAD `24f0826576b4ac724ad44a49870e40153e1d8513`，`pwsh -NoProfile -File tools/dev/ci.ps1 -KillStale`；2026-10-28T04:36:46Z–04:40:33Z，EXIT0、12/12 OK、0 FAIL。
- [gate log](../../../target/round77/gate/attempt-1.log)199,530B/SHA `6d9c974f73bc6125bdb270015f846eb188bd754fa226a8dc7bd3f6e822283858`；[meta](../../../target/round77/gate/attempt-1.meta.txt)887B/SHA `c6fa86c6d53fd1d66c2d2c19743fdb2d060a4d99c2b2f22aa84608bc306371ef`。日志GBK，以原字节/SHA为准；13行OK含子行，脚本汇总12项为权威。

- 御史台独立终审：**PASS-WITH-PARTIAL，必改 0**；被审 HEAD `5f30403`；隔离复跑 R77 7/7、R76 8/8、R72 9/9、R59 6/6、R59 render 16/16、fmt EXIT0；F1（G5）与 M6（G6）真红独立复现；清单16项与12个 tracked blob 0 失配、外置 SHA 一致、A digest 独立命中。详见[终审报告](round77-final-audit.md)与 `target/round77/t/attempt-1/**`。
- 严格 legacy clippy 实测 EXIT101（`mir-render/src/minimap.rs:353`、`tiles.rs:311` 历史 `needless_return`，两文件与 `0bbc322` 原字节一致）→ **历史承接 FAIL，不虚报 0**；门禁 workspace 口径（不带 legacy）仍 EXIT0/12 项。
- 清单 post-audit 刷新由 Lead 机械执行（新增审计 artifact、README 新 blob、delivery_head 前移、外置 SHA 重算）；不宣称 R77 通过。
## 偏差披露与承接

- A 首次补丁脚本非法newline致工具截断0字节；从attempt-1备份恢复后重做，旧证据保留。
- B 首次裸git archive继承core.autocrlf=true，CRLF令G7红（spellfx.rs25218/25705）；改`git -c core.autocrlf=false archive`后绿。dispatch原M5指G5，Lead要求六变异各覆盖不同G，实际M5=G7；G5由F1两负控覆盖。
- C2b合并`--matrix --selftest --assert`因参数互斥EXIT2，分开执行均EXIT0；工具`--attempt-ready`预期EXIT2/AUTO_READY_FORBIDDEN。
- 门下 **M1 OPEN**：Lib候选真实来源/语义绑定移交R78；2025 RGB565行对齐语义、矩阵anchor、严格Crystal等价、真实设备视觉及全部技能/素材准入均未完成。
- 历史PARTIAL原号照承：[R76索引](../round76/README.md)及R68/R49–R70/R61–R67/R71/R72/R73/R74/R75/R76；包括R72 B plan-M4/legacy lint、R71-C7/E1、R69条件5，不以本轮闭合它们。
- R76十八帧精确集、R59的116帧冻结集合文本保持原SHA；R78真实绑定及18→26 supersede仅待审，不属于本轮交付。
- 不得称“R77通过”“READY”“已认证”“生产已修”“Crystal严格等价”或“素材已可用”；准入状态仅NOT_READY / UNVERIFIED_SEMANTICS / AWAITING_BINDING。
- D仅做静态自检，不重跑cargo/CI；结果保存在[checks](../../../target/round77/d/attempt-1/checks.json)、[链接日志](../../../target/round77/d/attempt-1/check-links.log)、[状态快照](../../../target/round77/d/attempt-1/git-status.txt)。首检整份历史文档额外扫描发现3处旧target缺链（R50一处、R72两处），[首检原记录](../../../target/round77/d/attempt-1/checks-initial.json)保留；官方检查EXIT0，本轮新增链接另逐项核验，未改历史缺链。
- 过程依据：[计划](../../process/round77-plan.md)、[门下审议](../../process/round77-menxia-review.md)、[派发](../../process/round77-shangshu-dispatch.md)、[Lead裁定](../../process/round77-lead-ruling.md)。
