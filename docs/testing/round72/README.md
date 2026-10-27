# R72：149 PoisonCloud 逐格映射（pre-audit）

仅有限毒云切片交付；待尚书/门下终审，不代表整轮或 Crystal 等价验收。

## 功能与契约
- 功能 HEAD `2a31554`：feat(client): R72 149 PoisonCloud 逐格精灵映射与锚点迁移（第72轮）；2 文件 +1698，已提交并推送；D 文档已于 `d8b6157` 提交，manifest-only 清单固化于 `90a7516`，当前 HEAD 为清单提交后状态。
- [实现](../../../crates/mir-client/src/ui/spellfx.rs) M：386 行/17,755B，SHA256 `df572677d7b013c3c09dab9c764afbda2095cacec820f3292bfc7abca2f78be6`。
- [新增测试](../../../crates/mir-client/tests/round72_poison_cloud_mapping.rs)：1,660 行/65,322B，SHA256 `58f64ab2d30b69f3aea5c07a8975e988ca842bce45972499a6f93e120958baea`。
- [契约与映射](round72-poison-contract.md)：仅 `spell == Spell::PoisonCloud && event.from_object_spell` → Magic2 `1650+phase`，`phase=(age/120)%20`，格锚点+当帧 signed offset 一次；rate1 CPU Add，4000ms 是 AREA_MS 产品瞬态窗。
- 非149/1620起手/R71路径不变；合成像素证据不等于真实素材视觉签核。

## 审核链与勘误
- [计划](../../process/round72-plan.md)：161 行/16,013B；[门下](../../process/round72-menxia-review.md)：35 行；[尚书派发](../../process/round72-shangshu-dispatch.md)：39 行/4,517B；完整摘要见清单。
- [C 首轮](round72-c-evidence.md)：42 行/6,291B，SHA256 `5f6addd04b331f5341f0b6032332e768d76fb0e7585571e6f7d1fea630f7ed6a`，历史 BLOCK 两项：B矩阵不符、legacy clippy既有债；保留原报告。
- [B attempt-2](../../../target/round72/b/attempt-2/link.md) 补 plan-M2/M7并撤回 attempt-1「一一对应」；plan-M4精确居中仍为B近似残余，C独立七语义已覆盖。
- plan-M2 Bolt 红首败 `:939:5`，(384,256) actual `[48,136,176,255]` vs expected `[0,0,0,255]`；plan-M7去来源红 `:1491:5`，(381,229) actual `[248,4,0,255]` vs expected `[0,0,0,255]`。
- [Lead裁定](../../process/round72-lead-ruling.md)：两项必改 CLOSED（补证/勘误、既有lint债登记后续清理），非实现已补齐全部差异。
- [C2追加核验](round72-c-addendum.md)：20行，PASS-WITH-PARTIAL、必改0；[checks](../../../target/round72/c2/attempt-1/checks.json) 保留当时快照。

## 唯一门禁（Lead实跑，D未复跑）
- [attempt-1日志](../../../target/round72/gate/attempt-1.log)：212,729B，SHA256 `6fe99c93ee0bfaed2ede492071d5c2822415e453df37a3ec6025436410b43a3c`；HEAD `2a31554`，EXIT0，12项OK/0FAIL。
- UTC起点2026-10-27T12:36:59Z；终点2026-10-27T12:41:04Z，取[门禁日志](../../../target/round72/gate/attempt-1.log)最终写入 mtime（UTC），[UTF-8 meta](../../../target/round72/gate/attempt-1.meta.txt) 终版同值；先前交接的 12:41:34Z 与中间版 12:41:53Z 均为人工/中间快照，已撤回，原件日志不改；operator=lead。
- 官方门禁clippy不带legacy；带legacy探针EXIT101的两处needless_return为既有债，不冒称该探针通过。

## PARTIAL原号承接
- R71时钟/对象偏差：严格NextMotion、初始100ms取整、每Process一步、149持久场/26移除与对象寿命；4000ms不证明Crystal寿命。
- GAP-1/2 / R70-C2：rate0.8与 `src*0.8*alpha` 未实施，本轮仅rate1 CPU近似；DrawColour/INVLIGHT/完整DrawBlend继续PARTIAL。
- GAP-3/4/6：Light3、mask/tint/shadow、场景条件offset与AnimationOffset未闭合；GAP-5 / R70-C1：其他26技能及1620/其他调用点未迁。
- R70-C3全局尺寸居中、B plan-M4精确写法残余、legacy clippy既有债、真实素材/GPU视觉签核未做；R71-C7与更早历史原号不清零。

## 清单与快照差异
- [pre-audit清单](final-manifest.json) 不收自身或外置摘要；[外置SHA256](manifest.sha256) 校验清单；tracked项用 `git rev-parse <delivery_head>:<path>` 标识blob（`delivery_head=d8b6157`；crates 文件在其与功能 HEAD `2a31554` 的 blob 相同），工作树证据记SHA256/bytes/tracked:false。
- C2实际SHA为 `f7315ce0ffa8045ebdcf1b8903842c75af4b15cce7268b2ca2c6ebe1aaa716f8`，不是交接缩写7f3ad7a4…；C2记载Lead裁定旧快照e0fbe…/3767B，当前裁定a5758e…/3766B（测试SHA缩写已订正），不回写历史。
- D仅写六个授权文档，不改实现/测试/过程文书、不触碰两份monster-native freeze JSON、不commit/push、不跑cargo或CI；文档静态自检另由回执记录。
