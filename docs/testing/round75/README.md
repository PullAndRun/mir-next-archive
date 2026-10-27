# R75 149 事件→精灵生命周期差异合同

- 阶段：**D 文档批、待独立终审**；`stage=pre-audit`；子单 `JJC-20261028-002-sub-db4a1027`。
- 范围：仅新增 [7 项测试锁](../../../crates/mir-client/tests/round75_spell_lifetime_contract.rs)，生产 `crates/**/src/**` 相对 `96e46cc` 零差异；不是生产寿命修复。
- 提交链：`96e46cc`（规划/门下/派发）→`a6e77ef`（证据/裁定/勘误）→`30246bc`（测试文件）；HEAD 快照 `30246bc`。
- [计划](../../process/round75-plan.md) / [门下 C1–C6](../../process/round75-menxia-review.md#L25) / [派发](../../process/round75-shangshu-dispatch.md) / [差异合同](round75-lifecycle-contract.md) / [清单](final-manifest.json) / [外置 SHA](manifest.sha256)。
- 本次授权合同名为 `round75-lifecycle-contract.md`，替代旧计划中的 `round75-lifetime-contract.md`；G 编号以已提交测试及计划为准。

## 源码基准与 G1–G7

- [SpellObject.cs:288–302](../../../github/Crystal/Client/MirObjects/SpellObject.cs#L288)：初始 `next=start+interval` 后向下取整到 100ms；`>=` 推进，每 Process 一步，随后按 `now+interval` 排期。
- [Effect.cs:81](../../../github/Crystal/Client/MirObjects/Effect.cs#L81)：`Time <= NextFrame` 返回，严格 `>`；不可套给 SpellObject，不否定共用底层时间来源。
- [net.rs:5322–5333](../../../crates/mir-client/src/net.rs#L5322)：26 只撤 spell_fields，不清 spell_events；[5624–5647](../../../crates/mir-client/src/net.rs#L5624) 的 149 同写两表，同对象 ID 重入追加事件。
- [spellfx.rs:287–389](../../../crates/mir-client/src/ui/spellfx.rs#L287) 仅消费 events，draw 不推进/回收；FireWall/PoisonCloud 的 4000ms 为产品窗。

|锁|已提交测试的固定口径|
|---|---|
|G1|初始量化/等号：1234→next1300，1299不进、1300帧1；生产 age66 仍1630，与参考1631不同。|
|G2|稀疏 Process 2000 仅帧1、next2120，再2120帧2；生产 age2000 为1634。|
|G3|纯模型 Repeat 回卷 frame0/Ended=true，removed=false、仍可见；继续下一步仍推进。|
|G4|真实149 FireWall/PoisonCloud：age -1/4000/4001 空，0/1999/2000/3999 可见；signed offset 整帧锁。|
|G5|149(A/B)→26(A)：fields 只剩B，events 仍[id1,id2]及原起点；1120仍画两处。|
|G6|再149(A)重入：fields 2、events 3（旧A/B/新A）；1240 append 三 op；重复绘制相同且两表不变。|
|G7|纯模型 Blizzard 非Repeat到30仅隐藏、31仍未删，显式remove才删；Effect等号不进作对照。|

- M1 `>=`→`>` 指定 G1 红；M2 `next=now+interval`→`next+=interval` 指定 G2 红；M3 回卷额外 removed=true 指定 G3 红，均为测试私有模型变异。
- M4 隔离生产 AREA_MS 4000→2000 指定 G4 红；M5 隔离生产26额外删events 指定 G5 红；后者仅证伪现状合同，不评价未来修复设计。
- A/B/C 五变异均有编译 EXIT0→指定断言红101→恢复七绿；M1–M3 变体测试 SHA 不同，恢复为基线；固定断言/expected/fixture不改。
- [A manifest](../../../target/round75/a/attempt-1/manifest.json) / [负控汇总](../../../target/round75/a/attempt-1/negctrl/logs/negctrl-summary.json)；A/B 回归 R74/R73/R72 为9/9/5；新增 ignore/allow=0。
- [B manifest](../../../target/round75/b/attempt-1/manifest.json)：独立 oracle 82/82 agree，声明12个frame samples；[C oracle](../../../target/round75/c/attempt-1/oracle/oracle-output.json)：13个整帧case、63个模型轨迹采样，125 agree/0 mismatch；计数不互换。G3/G7不计生产覆盖。

## 门禁与证据复核链

- Lead 唯一全量 attempt-1：HEAD `30246bc`，EXIT0、12/12 OK、0 FAIL，UTC `2026-10-27T20:44:09Z–20:47:52Z`；门禁后 crates diff 空，D 不重跑 cargo/CI/矩阵。
- [门禁 log](../../../target/round75/gate/attempt-1.log)：214,417B，SHA256 `e6eec12095ea80b042dde250f4f1fd55d3d4b76aa5708a94d2da15df2aa38342`；[meta](../../../target/round75/gate/attempt-1.meta.txt)：481B，SHA256 `4483324991985472bc1bbb7e4a023fdb5d1d53b37e88564512858edd7c8a1fba`。
- [C 首核](round75-c-evidence.md)：FAIL/退补证，技术 PASS、必改2（C-M1 B、C-M2 A）；[Lead裁定](../../process/round75-lead-ruling.md)与[证据勘误](round75-evidence-erratum.md)处置 E1/E2。
- [C2](round75-c-addendum.md)：BLOCK/必改1，唯一因为任务书误引3,216B/`60046058…`中间快照，实物为3,210B/`26eaf4fa…`；[基线勘误](round75-baseline-erratum.md)明确任务书错误。
- [C3](round75-c-baseline-addendum.md)：PASS-WITH-PARTIAL、必改0、基线冲突 CLOSED；[checks](../../../target/round75/c3/attempt-1/checks.json) 为55 PASS/0 FAIL；不改写 C/C2 历史结论。
- A [只增补录](../../../target/round75/a/attempt-1/first-actual-claim-amendment.md)及[旁证](../../../target/round75/a/attempt-1/first-actual-artifacts/)：首个actual原stdout缺失；expected 12,630B/`e9fb241c…` generated_utc=19:31:57Z，19:35:43Z夹具旁证不替代stdout，E2为声明级 PARTIAL。
- B [aborted记录](../../../target/round75/b/attempt-1/aborted-attempt-record.md)及[日志补录](../../../target/round75/b/attempt-1/attempt-log-amendment.md)：WindowsApps python EXIT49 中止，未产生有效红绿证据；被覆盖字节不可恢复，E1=CLOSED-AS-PARTIAL（流程）。

## 七项 PARTIAL 与历史原号

1. GAP-1/2、R70-C2：完整DrawBlend/rate/Mode/DrawColour；不推导或实施 `src×0.8×alpha`。
2. GAP-5/R70-C1 其余149技能/1620/起手/其他调用点、R70-C3尺寸/居中，以及 GAP-3/4/6 mask/tint/shadow/Light/AnimationOffset；shadow只解析不消费。
3. R71时钟/对象、R73-T1：严格NextMotion/Repeat/26完整生产寿命仍PARTIAL；R73-T1反向注释子项已由历史提交修正，不扩大关闭范围。
4. R72 B plan-M4及legacy lint债；R71-C7永久PARTIAL、历史E1仅补救CLOSED；R69条件5永久证据缺口。
5. R68/R49–R70/R61–R67全部原号；R61 P01–P12、R62①–④、R63 P1–P7、R64①–⑥、R65 G1.2/G2、R66生命周期/全局水位、R67容量/保护/map epoch均不清零；R74 artifacts实数34照承。
6. R75-E1/C-M1：B aborted-attempt不可恢复原字节，流程 PARTIAL；必改闭合不等于原始证据恢复。
7. R75-E2/C-M2：A首次actual原stdout缺失，冻结早于首次actual仅声明级 PARTIAL；C3只关闭身份基线冲突。

- 不称生产寿命修复、Crystal严格等价或Blizzard生产迁移；模型与产品各自固定预期。真实素材/GPU/设备视觉/全UI/跨后端/Live音频未覆盖；未来替换现状合同须显式supersede并新增生产验收。
- 两历史freeze未跟踪且SHA不变；[D静态自检](../../../target/round75/d/attempt-1/)记录逐项bytes/SHA、链接、JSON、差异与生产零改；本批等待独立终审。
