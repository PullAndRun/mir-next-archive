# R52 范围与证据索引（A+H 文档批）

2026-10-26（Asia/Shanghai）；实测基线 `HEAD=origin/main=46be80b`（完整值见 [终审](round52-final-audit.md)）。本索引登记已交付的 A/B1/B2/B3/B4/C 批次、证据与评审路径：attempt1 失败史保留，attempt2 功能门禁 **12/12 PASS（exit 0）**；T 终审仍为 **BLOCK（仅 G7 文档一致性）**，本 H 文档收口已补齐，待 T/尚书复核。

依据：[中书方案](../../process/round52-plan.md)、[门下 D1–D5/M01–M08](../../process/round52-menxia-review.md)、[尚书派发 §3](../../process/round52-shangshu-dispatch.md)。R51 已 **Done / PASS-WITH-PARTIAL**，五技能 Blink/StormEscape/FrostCrunch/ThunderStorm/ShoulderDash 已交付；R52 B1 护身符耗材、B2 PoisonCloud、B3 TrapHexagon、B4 R51 定向回归与 C 客户端 wire 均已有对应证据和独立评审路径。125 无服务端生产者、真 socket、原生 dash 表现、guild/嵌套槽等边界继续标为 PARTIAL，不能称七技能或全链路完整。

| 目标 | 已交付批次 / 证据 | 当前事实与边界 |
|---|---|---|
| G1 耗材 | B1+C；`target/round52/b1/`、`c/`及各自 review | 合法根装备槽、单堆顺序/双项原子、Count/UUID/耗尽摘槽、生产存档重登与客户端消费已有子项证据；嵌套槽/真 socket仍 PARTIAL |
| G2 PoisonCloud | B2+C；`target/round52/b2/`、`c/`及 review | 符5+绿毒5、3×3毒雾、6秒/1秒tick、149/26生命周期已有证据；125无服务端生产者，真 socket/原生素材表现不覆盖 |
| G3 TrapHexagon | B3+C；`target/round52/b3/`、`c/`及 review | 符1、3×2预检/3×3落地、独立Shock/真实AI/受击释放、八场/入视野已有证据；玩家不受拘束，仍保留原生表现边界 |
| G4 默认数据 | B4；`target/round52/b4/`及 review | 默认七技能资料共107条、来源与 MP/冷却边界已有记录；缺覆盖项仍按 PARTIAL 处理 |
| G5 R51补证 | B4；`target/round52/b4/`及 review | Slow/Frozen/玩家目标边界、拒攻、根槽/added/buff与RNG、StormEscape五位置/跨图均有定向结果；PvP Frost真实命中仍 PARTIAL |
| G6 客户端 | C+B1/B2/B3；`target/round52/c/`及 review | 真实产包→encode→decode→快照、装备DeleteItem/149/125、152/153敏感回纠已有进程内 wire 证据；不等于真实 socket |
| G7 文档 | A+H；`target/round52/a/`、`target/round52/h/`及本 H diff | A 文档检查与 i18n 历史均保留；T 原件仍 BLOCK，H 已补齐收口证据，待 T/尚书复核 |

R51 [终审v4 §D](../round51/round51-final-audit-v4.md)、[尚书验收](../../process/round51-shangshu-acceptance.md)、[门下确认](../../process/round51-menxia-confirm.md)的十一项逐条映射于[路线图 §五](../../roadmap.md)。第1–6项及第10项根槽/第11项已有本轮对应证据但仍须保留未闭合子项；第7项火墙ProcessSpell/InTrapRock、第8项原生dash、第9项guild/真socket及第10项嵌套插槽顺延，不能被局部绿灯吞并。

证据目录与评审路径（批次结果已落盘，原失败史和边界不覆盖）：

- A：[批次摘要](../../../target/round52/a/00-summary.md) / [评审](../../../target/round52/a-review/review.md)。
- B1：[批次摘要](../../../target/round52/b1/00-summary.md) / [评审](../../../target/round52/b1-review/review.md)。
- B2：[批次摘要](../../../target/round52/b2/00-summary.md) / [最终评审 v3](../../../target/round52/b2-review/review-v3.md)。
- B3：[批次摘要](../../../target/round52/b3/00-summary.md) / [最终评审 v3](../../../target/round52/b3-review/review-v3.md)。
- B4：[批次摘要](../../../target/round52/b4/00-summary.md) / [评审](../../../target/round52/b4-review/review.md)。
- C：[批次摘要](../../../target/round52/c/00-summary.md) / [最终复审 v2](../../../target/round52/c-review/review-v2.md)。
- attempt1 失败原件与 attempt2 PASS 原件分别为 `target/round52/final-gate.log`、`target/round52/final-gate2.log`（12/12，exit0，SHA 见终审）。
- T：独立终审原件由[终审报告](round52-final-audit.md)索引，当前 verdict=BLOCK，阻断仅为 G7 文档收口；本 H 批已补齐关联文档和 `target/round52/h/` 自检证据，待 T/尚书复核。
- 各证据单文件≤1MiB；保全原字节/全部失败、command/cwd/完整HEAD/功能与工具配置指纹/dirty/时区/start/end/raw exit/实际用例数。target保持ignored，manifest不替代原日志。

i18n 收口：含测试 strict 当前 **66≤70**，no-test strict **0≤0**；R52 返工历史为 **96→103→66** 与 **2→0**。另需披露：`46be80b` 除测试字面量外，还翻译了 `crates/mir-server/src/magic.rs:1725`、`:1762` 两处生产 `debug_assert` 消息；逻辑未变。

PARTIAL 口径：B1–B3及C的耗材/场/wire闭环已交付但仍按子项边界记录；125无服务端生产者、无真 socket、D3离线/异图清场偏差、旧技能耗材、guild/1965–1966嵌套槽来源、原生视觉/音频/dash继续有界。缺日志、零命中、伪fixture/空断言不能降为PARTIAL通关；专属AI核心漏控或 G6 场消费缺失仍属核心阻断。

顺序：A→B1→B2→B3→B4→C→Lead唯一全量门禁→T→尚书→指定门下；每批串行独立gpt评审，编码含测试仅deepseek，仅Lead提交。A只做授权五项文档检查，不改crates/语言包/扫描器/阈值，不运行全量、不提交、不调用看板done。
