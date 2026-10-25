# R52 范围与证据索引（A 文档批）

2026-10-25（Asia/Shanghai）；任务书基线 main=`e9602d4`，A 实读 HEAD=origin/main=`489816af0e6b39e5e7c4a4e348adc9e944b36c7e`。本索引登记范围、目标和证据规划，**R52 功能尚未实现 / 计划中，未执行本轮功能门禁**。

依据：[中书方案](../../process/round52-plan.md)、[门下 D1–D5/M01–M08](../../process/round52-menxia-review.md)、[尚书派发 §3](../../process/round52-shangshu-dispatch.md)。R51 已 **Done / PASS-WITH-PARTIAL**，五技能 Blink/StormEscape/FrostCrunch/ThunderStorm/ShoulderDash 已交付；PoisonCloud/TrapHexagon 与护身符耗材仍未实现，不能称七技能完整。R50 独立商城已交付，沿用其残余边界。

| 目标 | 计划批次 | 待实现 / 待证范围 |
|---|---|---|
| G1 耗材 | B1+B2/B3+C | 合法根装备槽含6/9、单堆顺序/双项原子、Count/UUID/耗尽摘槽、生产存档重登 |
| G2 PoisonCloud | B2+C | 符5+绿毒5、真实3×3毒雾/6秒1秒tick、MAC后存活绿毒、PK/生命周期/149与125 |
| G3 TrapHexagon | B3+C | 符1、3×2预检/3×3落地、独立Shock/真实AI/受击释放、八场/入视野 |
| G4 默认数据 | B4 | 实际默认七记录/来源SHA、独立期望、MP与冷却边界；缺哪条点名哪条PARTIAL |
| G5 R51补证 | B4 | Slow/Frozen/玩家目标、拒攻、根槽/added/buff与RNG、StormEscape五位置/跨图 |
| G6 客户端 | C+B1/B2/B3 | 真实产包→encode→decode→快照、装备DeleteItem/149/125、152/153敏感回纠 |
| G7 文档 | A+T/尚书 | R50/R51终态、十一项映射、实测i18n、最终证据追溯 |

R51 [终审v4 §D](../round51/round51-final-audit-v4.md)、[尚书验收](../../process/round51-shangshu-acceptance.md)、[门下确认](../../process/round51-menxia-confirm.md)的十一项逐条映射于[路线图 §五](../../roadmap.md)。第1–6项及第10项根槽/第11项为上述定向计划；第7项火墙ProcessSpell/InTrapRock、第8项原生dash、第9项guild/真socket及第10项嵌套插槽顺延，不能被局部绿灯吞并。

证据目录规划（下列批次目录/报告为规划，不表示已有结果）：

- `target/round52/a/`：本批命令、原exit、243/32与67/0实测、差异/时区/HEAD指纹；[00-summary](../../../target/round52/a/00-summary.md)。
- `target/round52/b1/`、`b2/`、`b3/`、`b4/`：分别保存耗材、毒雾、Trap、R51定向补证的真实named tests/失败与绿灯attempt。
- `target/round52/c/`：真实wire/装备快照/场生命周期/敏感回纠；`target/round52/gate-attempt*/`：Lead唯一全量门禁。
- `target/round52/t/`：独立终审原证据；本目录后续由T形成终审报告及final-manifest，本A批不预写PASS。
- 各证据单文件≤1MiB；保全原字节/全部失败、command/cwd/完整HEAD/功能与工具配置指纹/dirty/时区/start/end/raw exit/实际用例数。超限连续分片+SHA，不截尾。target保持ignored，manifest不替代原日志。

PARTIAL 口径：G1–G3及G6耗材/场闭环为核心硬门槛，缺失返工；G4/G5逐子项判定。缺日志、零命中、伪fixture/空断言不能降为PARTIAL通关。D2材料失败保留公共扣蓝/动作/SpellTime，Trap保留范围差异和安全剪裁；D3离线/异图安全清场是已批准但待实现/实证的生命周期偏差，仍须单列PARTIAL。D4只接新两技能；旧技能耗材、guild/1965–1966嵌套槽来源、原生视觉/音频/dash与真socket继续有界。专属AI核心漏控或G6场消费缺失不得预授权降格（D5）。

顺序：A→B1→B2→B3→B4→C→Lead唯一全量门禁→T→尚书→指定门下；每批串行独立gpt评审，编码含测试仅deepseek，仅Lead提交。A只做授权五项文档检查，不改crates/语言包/扫描器/阈值，不运行全量、不提交、不调用看板done。
